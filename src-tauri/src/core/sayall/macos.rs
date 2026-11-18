//! macOS 实现：LaunchServices 探测 + 进程启停 + SayAll 偏好域读写。
//!
//! 全部走 AppKit 运行时（`msg_send!`），不引新依赖：这些调用都是「读系统现状 / 起
//! 进程」，没有对应的 Tauri API。跨进程读写同一个用户的偏好域在这台机器上验证过
//! （Halo 未沙盒，`NSUserDefaults initWithSuiteName:` 指的就是对方 App 的域）。

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use objc2::runtime::{AnyObject, Bool};
use objc2::{class, msg_send};

use super::{State, APP_BUNDLE_ID, STATUS_ICON_KEY};

/// 等 SayAll 真正退出：先给它体面退出的机会（`applicationWillTerminate` 里要
/// `model.stop()` 收音频链路，本机实测就要好几秒），超时才强杀。
const QUIT_GRACE: Duration = Duration::from_secs(10);
const QUIT_FORCE_GRACE: Duration = Duration::from_secs(3);
/// `open` 之后等进程出现的预算：冷启动一秒上下，慢机器给到五秒。
const LAUNCH_GRACE: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(120);

/* ------------------------------------------------------------------ */
/* ObjC 小工具                                                          */
/* ------------------------------------------------------------------ */

/// 一个 `NSAutoreleasePool`。
///
/// 这里的读数几乎全是 autoreleased 的 `NSString` / `NSArray`，而且面板每打开一次就
/// 读一遍；不兜池子这些对象要等到主线程下一次轮转才释放。
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

/// `-[NSString UTF8String]` → Rust `String`；空指针按 `None`。
///
/// 只对确定是 `NSString` 的对象调用（调用方都用了 `objectForKey:` 这类返回字符串的选择子）。
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

/// objc2 0.5 对 nil receiver 发消息会直接 panic，所以每个入口都要先判空。
unsafe fn workspace() -> Option<*mut AnyObject> {
    let ws: *mut AnyObject = msg_send![class!(NSWorkspace), sharedWorkspace];
    (!ws.is_null()).then_some(ws)
}

/* ------------------------------------------------------------------ */
/* 探测                                                                */
/* ------------------------------------------------------------------ */

/// 某个 `.app` 的 `(bundle id, 版本号)`；plist 读不到就返回 `None`。
unsafe fn plist_identity(app_path: &Path) -> Option<(String, Option<String>)> {
    let dict: *mut AnyObject = msg_send![
        class!(NSDictionary),
        dictionaryWithContentsOfFile: ns_string(&app_path.join("Contents/Info.plist").to_string_lossy())
    ];
    if dict.is_null() {
        return None;
    }
    let id = from_ns(msg_send![
        dict,
        objectForKey: ns_string("CFBundleIdentifier")
    ])?;
    let version = from_ns(msg_send![
        dict,
        objectForKey: ns_string("CFBundleShortVersionString")
    ]);
    Some((id, version))
}

/// 这个目录里的 `.app` 是不是我们要找的那个。
unsafe fn identity_of(app_path: &Path) -> Option<(String, Option<String>)> {
    let (id, version) = plist_identity(app_path)?;
    (id == APP_BUNDLE_ID).then(|| (app_path.to_string_lossy().into_owned(), version))
}

/// LaunchServices 记录的安装路径。
unsafe fn launch_services_path() -> Option<PathBuf> {
    let ws = workspace()?;
    let url: *mut AnyObject =
        msg_send![ws, URLForApplicationWithBundleIdentifier: ns_string(APP_BUNDLE_ID)];
    if url.is_null() {
        return None;
    }
    let path = from_ns(msg_send![url, path])?;
    let candidate = PathBuf::from(path);
    candidate.is_dir().then_some(candidate)
}

/// 常见安装位置：`/Applications`、按本机归类放法建的子目录、`~/Applications`，各看一层。
///
/// 只到第二层是为了覆盖 `/Applications/开源软件/SayAll.app`，同时保证一次读取最多
/// 读几十个 plist。而且这条路径只在 LaunchServices 查不到时才走。
fn install_roots() -> Vec<PathBuf> {
    let mut roots = vec![PathBuf::from("/Applications")];
    if let Some(home) = dirs::home_dir() {
        roots.push(home.join("Applications"));
    }
    roots
}

fn scanned_paths() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    for root in install_roots() {
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("app") {
                candidates.push(path);
            } else if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                let Ok(inner) = std::fs::read_dir(&path) else {
                    continue;
                };
                for sub in inner.flatten() {
                    let sub_path = sub.path();
                    if sub_path.extension().and_then(|e| e.to_str()) == Some("app") {
                        candidates.push(sub_path);
                    }
                }
            }
        }
    }
    candidates
}

/// 找到 SayAll：`(路径, 版本号)`。
///
/// 不能只信 LaunchServices——这台机器上真栽过（别的 App）：它的优选记录可能指向一个
/// 已弹出的镜像卷，于是按 bundle id 查直接返回 nil，而 App 其实好好躺在安装目录里。
/// 所以 LS 的结果要落地校验，查不到或路径不存在时再扫一遍安装目录。
fn find_app() -> Option<(String, Option<String>)> {
    let _pool = unsafe { Pool::new() };
    if let Some(candidate) = unsafe { launch_services_path() } {
        if let Some(found) = unsafe { identity_of(&candidate) } {
            return Some(found);
        }
    }
    scanned_paths()
        .iter()
        .find_map(|candidate| unsafe { identity_of(candidate) })
}

/// SayAll 的 `NSRunningApplication`；没在跑返回空指针。
///
/// 用类方法 `+[NSRunningApplication runningApplicationsWithBundleIdentifier:]`，**不用**
/// `NSWorkspace.runningApplications`：后者那张表靠 RunLoop 收通知更新，而 Halo 的这些
/// 命令跑在没有 RunLoop 的 worker 线程上，本机实测那张表会整个冻结在第一次查询的样子
/// （进程明明已经没了，它还一直报「在跑」，退出等待因此永远等不到）。类方法是现查的。
///
/// 返回的对象由调用方的 `Pool` 持有，只能在那个池子还开着的时候用。
unsafe fn running_application() -> *mut AnyObject {
    let apps: *mut AnyObject = msg_send![
        class!(NSRunningApplication),
        runningApplicationsWithBundleIdentifier: ns_string(APP_BUNDLE_ID)
    ];
    if apps.is_null() {
        return std::ptr::null_mut();
    }
    let count: usize = msg_send![apps, count];
    if count == 0 {
        return std::ptr::null_mut();
    }
    msg_send![apps, objectAtIndex: 0usize]
}

/// 在同一个自动释放池里对正在跑的实例执行 `f`；没在跑返回 `None`。
///
/// 池子必须在 `f` 用完对象之后才排空，所以指针不能漏出去。
fn with_running<R>(f: impl FnOnce(*mut AnyObject) -> R) -> Option<R> {
    let _pool = unsafe { Pool::new() };
    unsafe {
        let app = running_application();
        (!app.is_null()).then(|| f(app))
    }
}

fn is_running() -> bool {
    let _pool = unsafe { Pool::new() };
    unsafe { !running_application().is_null() }
}

/* ------------------------------------------------------------------ */
/* 偏好域                                                              */
/* ------------------------------------------------------------------ */

/// SayAll 的偏好域句柄（调用方负责 `release`）。
///
/// 用 `suiteName:` 而不是 `standardUserDefaults`：后者在 Halo 进程里指向 Halo 自己的域。
unsafe fn defaults() -> *mut AnyObject {
    let alloc: *mut AnyObject = msg_send![class!(NSUserDefaults), alloc];
    msg_send![alloc, initWithSuiteName: ns_string(APP_BUNDLE_ID)]
}

/// 读一个布尔项；「没有这条记录」返回 `fallback`（区别于记录本身就是 false）。
unsafe fn read_bool(defaults: *mut AnyObject, key: &str, fallback: bool) -> bool {
    let ns_key = ns_string(key);
    let raw: *mut AnyObject = msg_send![defaults, objectForKey: ns_key];
    if raw.is_null() {
        return fallback;
    }
    let value: Bool = msg_send![defaults, boolForKey: ns_key];
    bool::from(value)
}

/// 当前的「菜单栏显示图标」读数。
fn status_bar_visible() -> bool {
    let _pool = unsafe { Pool::new() };
    unsafe {
        let d = defaults();
        let value = read_bool(d, STATUS_ICON_KEY, true);
        let _: () = msg_send![d, release];
        value
    }
}

/// 写「菜单栏显示图标」，并用重新读一遍来判定成败。
///
/// 跨进程写偏好域的失败都是静默的，只看方法返回值会把没写进去的开关报成成功。
fn write_status_bar_visible(visible: bool) -> Result<(), String> {
    {
        let _pool = unsafe { Pool::new() };
        unsafe {
            let d = defaults();
            let num: *mut AnyObject =
                msg_send![class!(NSNumber), numberWithBool: Bool::from(visible)];
            let _: () = msg_send![d, setObject: num, forKey: ns_string(STATUS_ICON_KEY)];
            let _: () = msg_send![d, release];
        }
    }
    let echoed = status_bar_visible();
    if echoed == visible {
        Ok(())
    } else {
        Err(format!(
            "写入 SayAll 设置失败：{STATUS_ICON_KEY} 期望 {visible}，读回 {echoed}"
        ))
    }
}

/* ------------------------------------------------------------------ */
/* 进程启停                                                            */
/* ------------------------------------------------------------------ */

/// 用 `open <路径>` 拉起 / 唤起，路径只可能是 [`find_app`] 里校验过 plist 的那一条。
///
/// SayAll 是菜单栏 App（`LSUIElement`，可以连 Dock 图标都不要）：`open` 对已在跑的
/// 实例会走 `applicationShouldHandleReopen`，它在那里直接开自己的主窗口，所以「启动」
/// 和「带前台」是同一条命令。
fn open_app(path: &str) -> Result<(), String> {
    let output = std::process::Command::new("/usr/bin/open")
        .arg(path)
        .output()
        .map_err(|e| format!("无法启动 SayAll：{e}"))?;
    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "无法启动 SayAll：{}",
        String::from_utf8_lossy(&output.stderr).trim()
    ))
}

fn app_path() -> Result<String, String> {
    find_app()
        .map(|(path, _)| path)
        .ok_or_else(|| "没在这台 Mac 上找到无线麦 SayAll".to_string())
}

/// 强杀之后再等一轮。按 bundle id 重查，不复用可能已随池子失效的旧指针。
fn gone_within(budget: Duration) -> bool {
    within(budget, || !is_running())
}

/// 等进程出现。
///
/// `open` 只是把请求交给 LaunchServices，秒回；不等一下就读到的 `running` 永远是
/// false，界面会把刚启动的 SayAll 报成「没在运行」。等不到不算失败：窗口可能早就在了。
fn wait_until_running(budget: Duration) -> bool {
    within(budget, is_running)
}

/// 每 [`POLL`] 采一次 `done()`，先采一次再等。
fn within(budget: Duration, done: impl Fn() -> bool) -> bool {
    let deadline = Instant::now() + budget;
    loop {
        if done() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(POLL);
    }
}

/// 退出正在跑的 SayAll；没在跑直接算成功。
fn quit_running() -> Result<(), String> {
    let accepted = with_running(|app| unsafe {
        let graceful: Bool = msg_send![app, terminate];
        bool::from(graceful)
    });
    match accepted {
        None => return Ok(()),
        Some(false) => return Err("SayAll 拒绝了退出请求".to_string()),
        Some(true) => {}
    }
    if gone_within(QUIT_GRACE) {
        return Ok(());
    }
    // 体面退出超时：多半是音频/事件线程还在收尾。强杀只是催一把，成败仍以
    // 「进程有没有消失」为准——`forceTerminate` 返回 false 常常正是「已在收尾」。
    with_running(|app| unsafe {
        // 运行时签名给的是 `B`（Apple 文档写的 void 不准确）：按 `()` 发会被
        // objc2 的类型校验当场拦下来。
        let _forced: Bool = msg_send![app, forceTerminate];
    });
    if gone_within(QUIT_FORCE_GRACE) {
        Ok(())
    } else {
        Err("SayAll 未能退出，请在活动监视器里结束它".to_string())
    }
}

/* ------------------------------------------------------------------ */
/* 对外                                                               */
/* ------------------------------------------------------------------ */

pub fn state() -> State {
    let found = find_app();
    let version = found.as_ref().and_then(|(_, version)| version.clone());
    let supported = super::supports_status_icon_toggle(&version);
    State {
        installed: found.is_some(),
        status_bar_icon_supported: supported,
        // 低版本不读这个键，报「已隐藏」是把一个空转说成生效了。
        status_bar_visible: if supported { status_bar_visible() } else { true },
        version,
        running: is_running(),
    }
}

pub fn open() -> Result<(), String> {
    open_app(&app_path()?)?;
    wait_until_running(LAUNCH_GRACE);
    Ok(())
}

pub fn quit() -> Result<State, String> {
    quit_running()?;
    Ok(state())
}

pub fn set_status_bar_visible(visible: bool) -> Result<State, String> {
    let (path, version) = find_app().ok_or_else(|| "没在这台 Mac 上找到无线麦 SayAll".to_string())?;
    if !super::supports_status_icon_toggle(&version) {
        return Err(format!(
            "SayAll {} 没有「菜单栏显示图标」这个设置，图标是启动时无条件画的。升级到 {} 及以上才能在这里关掉。",
            version.unwrap_or_else(|| "版本未知".to_string()),
            super::STATUS_ICON_MIN_VERSION
        ));
    }
    write_status_bar_visible(visible)?;
    // 它只在启动时按这个值决定画不画菜单栏图标，所以在跑就得重启一次才看得见效果。
    if is_running() {
        quit_running()?;
        open_app(&path)?;
        wait_until_running(LAUNCH_GRACE);
    }
    Ok(state())
}

#[cfg(test)]
mod tests {
    use super::{find_app, status_bar_visible};
    use crate::core::sayall::supports_status_icon_toggle;

    /// 真机自检：`cargo test -- --ignored --nocapture sayall`。
    ///
    /// 只查盘和读偏好，不发 Apple Event、不重启对方 App——那两条会把用户正在用的
    /// 无线麦打断，不能塞进测试里。
    #[test]
    #[ignore = "需要本机装有无线麦 SayAll"]
    fn finds_the_installed_sayall_app() {
        let Some((path, version)) = find_app() else {
            return;
        };
        assert!(path.ends_with(".app"), "探到的路径不是 .app：{path}");
        let supported = supports_status_icon_toggle(&version);
        println!(
            "SayAll {path} v{version:?} 认菜单栏开关={supported} 偏好域里=菜单图标显示中={}",
            status_bar_visible()
        );
    }
}
