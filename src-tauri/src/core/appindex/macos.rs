//! macOS 实现：扫安装目录 + 读 `Info.plist` + `open` 启动 + 按 bundle id 退出。

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use objc2::runtime::{AnyObject, Bool};
use objc2::{class, msg_send};

use super::{normalize_name, AppInfo, Located};
use crate::core::statusbar::cf;

/// 缓存有效期：装/删一个 App 之后最多这么久就能被语音认出来。
///
/// 不做「目录变了就重扫」的监听——`/Applications` 的变更事件要开 kqueue，
/// 而这条链路只在用户真的说了一句「打开 X」时才走，60 秒的缓存足够便宜。
const CACHE_TTL_MS: u64 = 60_000;

static CACHE: Mutex<Option<(u64, Vec<AppInfo>)>> = Mutex::new(None);
/// 后台换新进行中：同一时刻只允许有一个扫描线程。
static REFRESHING: AtomicBool = AtomicBool::new(false);

/* ------------------------------------------------------------------ */
/* ObjC 小工具（与 `core/sayall/macos.rs` 同一套写法）                    */
/* ------------------------------------------------------------------ */

struct Pool(*mut AnyObject);

impl Pool {
    unsafe fn new() -> Self {
        Self(msg_send![class!(NSAutoreleasePool), new])
    }
}

impl Drop for Pool {
    fn drop(&mut self) {
        unsafe {
            let _: () = msg_send![self.0, release];
        }
    }
}

unsafe fn ns_string(value: &str) -> *mut AnyObject {
    let c = CString::new(value).unwrap_or_default();
    msg_send![class!(NSString), stringWithUTF8String: c.as_ptr()]
}

unsafe fn from_ns(obj: *mut AnyObject) -> Option<String> {
    if obj.is_null() {
        return None;
    }
    let utf8: *const c_char = msg_send![obj, UTF8String];
    if utf8.is_null() {
        return None;
    }
    Some(CStr::from_ptr(utf8).to_string_lossy().into_owned())
}

unsafe fn plist_string(dict: *mut AnyObject, key: &str) -> Option<String> {
    let raw: *mut AnyObject = msg_send![dict, objectForKey: ns_string(key)];
    from_ns(raw)
}

/* ------------------------------------------------------------------ */
/* 扫描                                                                */
/* ------------------------------------------------------------------ */

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 装 App 的常见位置。`/System/Applications` 那两条要带上，否则「打开终端」
/// 「打开 Siri」这类系统 App 一个都认不出来。
fn install_roots() -> Vec<PathBuf> {
    let mut roots = vec![
        PathBuf::from("/Applications"),
        PathBuf::from("/System/Applications"),
        PathBuf::from("/System/Library/CoreServices/Applications"),
    ];
    if let Some(home) = dirs::home_dir() {
        roots.push(home.join("Applications"));
    }
    roots
}

/// 目录本身是不是 `.app`，是就返回它。
fn as_app(path: &Path) -> Option<&Path> {
    (path.extension().and_then(|e| e.to_str()) == Some("app")).then_some(path)
}

/// 收集候选 `.app` 路径：根目录一层，外加根目录下的一层子目录。
///
/// 多那一层是为了 `/Applications/开源软件/SayAll.app` 和
/// `/System/Applications/Utilities/Terminal.app`；再深就不是安装习惯而是别的了。
fn app_dirs() -> Vec<PathBuf> {
    let mut found = Vec::new();
    for root in install_roots() {
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(app) = as_app(&path) {
                found.push(app.to_path_buf());
                continue;
            }
            if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            let Ok(inner) = std::fs::read_dir(&path) else {
                continue;
            };
            for sub in inner.flatten() {
                if let Some(app) = as_app(&sub.path()) {
                    found.push(app.to_path_buf());
                }
            }
        }
    }
    found
}

/// 中文界面名的两个来源，都读一遍；读不到就只剩英文名。
///
/// - `zh-Hans.lproj/InfoPlist.strings`：第三方 App 的中文界面名写在这里。微信的
///   `Info.plist` 里只有 `WeChat`，不读这一层就认不出「打开微信」。
/// - `LSCopyDisplayNameForURL`：和访达同一个来源。系统 App（备忘录 / 终端）包里
///   没有中文 strings，名字由系统按语言环境现解析——这一条在 Halo 里给的是中文，
///   但解析结果跟着进程的语言环境走，所以 mod.rs 里那张中文名表仍然留着兜底。
///
/// 两条都只在后台扫描线程上跑（见 [`installed`]），不占主线程。
fn localized_names(dir: &Path) -> Vec<String> {
    let _pool = unsafe { Pool::new() };
    // strings 文件在前：它给的是干净的「微信」，而 LaunchServices 会跟着访达的
    // 「显示扩展名」设置给出「微信.app」。
    let mut out = unsafe { strings_names(dir) };
    if let Some(name) = unsafe { launchservices_name(dir) } {
        out.push(name);
    }
    out
}

#[link(name = "CoreServices", kind = "framework")]
extern "C" {
    fn LSCopyDisplayNameForURL(
        url: *const std::ffi::c_void,
        display_name: *mut *const std::ffi::c_void,
    ) -> i32;
}

unsafe fn launchservices_name(dir: &Path) -> Option<String> {
    // NSURL 与 CFURL 自由桥接；`fileURLWithPath:` 给的是自动释放对象，只借不还。
    let url: *mut AnyObject =
        msg_send![class!(NSURL), fileURLWithPath: ns_string(&dir.to_string_lossy())];
    if url.is_null() {
        return None;
    }
    let mut name: *const std::ffi::c_void = std::ptr::null();
    // 0 = noErr。返回值是按 Create 规则交给我们的 +1 引用，必须释放。
    if LSCopyDisplayNameForURL(url.cast_const().cast(), &mut name) != 0 {
        return None;
    }
    let text = cf::string(name);
    cf::release(name);
    text
}

/// 系统语言目录的两种写法：第三方 App 用 `zh-Hans.lproj`，苹果自己的 App 用 `zh_CN.lproj`。
const ZH_STRINGS_DIRS: &[&str] = &[
    "zh-Hans.lproj",
    "zh_CN.lproj",
    "zh-Hant.lproj",
    "zh_TW.lproj",
];

unsafe fn strings_names(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    for lproj in ZH_STRINGS_DIRS {
        let path = dir.join(format!("Contents/Resources/{lproj}/InfoPlist.strings"));
        if !path.is_file() {
            continue;
        }
        // `.strings` 常见是 UTF-16 的老式 plist，`dictionaryWithContentsOfFile:` 读得动。
        let dict: *mut AnyObject = msg_send![
            class!(NSDictionary),
            dictionaryWithContentsOfFile: ns_string(&path.to_string_lossy())
        ];
        if dict.is_null() {
            continue;
        }
        for key in ["CFBundleDisplayName", "CFBundleName"] {
            if let Some(value) = plist_string(dict, key) {
                out.push(value);
            }
        }
        break;
    }
    out
}

/// 读一个 `.app` 的显示名与别名；plist 读不到就按文件名兜底。
unsafe fn read_app(dir: &Path) -> Option<AppInfo> {
    let stem = dir.file_stem()?.to_string_lossy().into_owned();
    let path = dir.to_string_lossy().into_owned();
    let dict: *mut AnyObject = msg_send![
        class!(NSDictionary),
        dictionaryWithContentsOfFile: ns_string(&dir.join("Contents/Info.plist").to_string_lossy())
    ];
    // plist 读不出来（权限/损坏）时四项都留空，下面的通用路径会退到文件名。
    let (display, bundle_name, executable, bundle_id) = if dict.is_null() {
        (None, None, None, None)
    } else {
        (
            plist_string(dict, "CFBundleDisplayName"),
            plist_string(dict, "CFBundleName"),
            plist_string(dict, "CFBundleExecutable"),
            plist_string(dict, "CFBundleIdentifier"),
        )
    };

    let localized = localized_names(dir);
    // LaunchServices 给的名字可能带 `.app`（取决于访达的「显示扩展名」），显示时去掉。
    let localized_display = localized
        .first()
        .map(|name| name.strip_suffix(".app").unwrap_or(name).to_string());

    let fallback_name = localized_display
        .clone()
        .or(display.clone())
        .or_else(|| bundle_name.clone())
        .unwrap_or_else(|| stem.clone());

    let mut candidates: Vec<&str> = localized.iter().map(String::as_str).collect();
    candidates.extend(
        [
            display.as_deref(),
            bundle_name.as_deref(),
            Some(stem.as_str()),
            executable.as_deref(),
        ]
        .into_iter()
        .flatten(),
    );

    let mut aliases: Vec<String> = Vec::new();
    for alias in candidates {
        let alias = normalize_name(alias);
        if !alias.is_empty() && !aliases.contains(&alias) {
            aliases.push(alias);
        }
    }
    super::add_zh_system_names(&mut aliases);
    let name = super::display_name(&aliases, &fallback_name);

    Some(AppInfo {
        name,
        path,
        bundle_id,
        aliases,
    })
}

fn scan() -> Vec<AppInfo> {
    let _pool = unsafe { Pool::new() };
    app_dirs()
        .iter()
        .filter_map(|dir| unsafe { read_app(dir) })
        .collect()
}

pub fn installed() -> Vec<AppInfo> {
    let mut cache = match CACHE.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    // 只在第一次（还没有任何缓存）时当场扫：扫描要读几百个 Info.plist，
    // 而调用它的命令跑在主线程上，卡在这里整个 App 就停了半秒。
    let Some((at, apps)) = cache.as_ref() else {
        let scanned = scan();
        let initial = scanned.clone();
        *cache = Some((now_ms(), scanned));
        return initial;
    };
    let current = apps.clone();
    if now_ms().saturating_sub(*at) < CACHE_TTL_MS {
        return current;
    }
    // 过期了：先给旧数据（这一句还是要立刻执行），另起一线程换新，下次调用生效。
    drop(cache);
    if !REFRESHING.swap(true, Ordering::SeqCst) {
        std::thread::spawn(|| {
            let scanned = scan();
            if let Ok(mut guard) = CACHE.lock() {
                *guard = Some((now_ms(), scanned));
            }
            REFRESHING.store(false, Ordering::SeqCst);
        });
    }
    current
}

/* ------------------------------------------------------------------ */
/* 启停                                                                */
/* ------------------------------------------------------------------ */

/// 路径必须是我们自己扫出来的绝对 `.app` 路径。
///
/// 这是这条链路上唯一的注入防线：语音内容会变成参数传给 `/usr/bin/open`，
/// 但传出去的一律是这里的 `path` 字段，不是用户那句话。
fn is_plausible_app_path(path: &str) -> bool {
    path.starts_with('/') && !path.contains('\0') && path.to_ascii_lowercase().ends_with(".app")
}

pub fn open(app: &AppInfo) -> Result<(), String> {
    if !is_plausible_app_path(&app.path) {
        return Err(format!("App 路径不可信：{}", app.path));
    }
    let output = std::process::Command::new("/usr/bin/open")
        .arg(&app.path)
        .output()
        .map_err(|e| format!("无法启动 {}：{e}", app.name))?;
    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "无法启动 {}：{}",
        app.name,
        String::from_utf8_lossy(&output.stderr).trim()
    ))
}

/// 按 bundle id 现查正在跑的实例再发退出请求。
///
/// 用类方法而不是 `NSWorkspace.runningApplications`：后者那张表要靠 RunLoop 收通知，
/// 而命令跑在没有 RunLoop 的线程上，本机实测会整个冻结（见 `core/sayall/macos.rs`）。
/// 这里也不等它真的退完：语音指令要的是「立刻有回应」，收尾交给系统。
pub fn quit(query: &str) -> Result<Located, String> {
    let app = super::find(query).ok_or_else(|| format!("没找到叫「{query}」的 App"))?;
    let Some(bundle_id) = app.bundle_id.clone() else {
        return Err(format!("{} 没有 bundle id，退不了", app.name));
    };
    let terminated = {
        let _pool = unsafe { Pool::new() };
        let langs: *mut AnyObject = unsafe {
            let item = ns_string("zh-Hans");
            msg_send![class!(NSArray), arrayWithObjects: &item count: 1usize]
        };
        unsafe {
            let defaults: *mut AnyObject = msg_send![class!(NSUserDefaults), standardUserDefaults];
            let _: () = msg_send![defaults, setObject: langs forKey: ns_string("AppleLanguages")];
        }
        unsafe {
            let apps: *mut AnyObject = msg_send![
                class!(NSRunningApplication),
                runningApplicationsWithBundleIdentifier: ns_string(&bundle_id)
            ];
            if apps.is_null() {
                false
            } else {
                let count: usize = msg_send![apps, count];
                if count == 0 {
                    false
                } else {
                    let instance: *mut AnyObject = msg_send![apps, objectAtIndex: 0usize];
                    let graceful: Bool = msg_send![instance, terminate];
                    bool::from(graceful)
                }
            }
        }
    };
    if terminated {
        return Ok(app.located());
    }
    Err(format!("{} 没在运行", app.name))
}
