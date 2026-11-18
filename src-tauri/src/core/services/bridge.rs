//! Finder Sync 扩展 → 主程序的请求桥。
//!
//! 扩展是沙盒进程，不能写用户目录；它把一次右键动作写成 JSON 放进自己的
//! 容器（有 App Group 时放 Group Container），再用 `halo-finder://wake`
//! 唤醒主程序。主程序每 250ms 扫一遍候选目录，取走请求并执行。
//!
//! 为什么不用 NSServices：NSServices 只能出现在「服务」子菜单，桌面空白处
//! 也没有菜单入口；Finder 窗口与桌面的右键都由 Finder Sync 扩展注入条目，
//! 桌面本身的其余菜单保持系统原生。
//!
//! 为什么用文件队列而不是 XPC：主程序不是 launchd 常驻服务，没有稳定的
//! Mach service；文件队列在 App 未运行时也不会丢请求，启动后补执行即可。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use tauri::AppHandle;

use super::{FinderIntegrationStatus, ServiceAction, EXTENSION_BUNDLE_ID};

/// 扩展的 App Group（正式签名时扩展会优先写这里）。
const APP_GROUP_ID: &str = "group.com.halo.desktop";
/// 扩展自己的容器 / 非沙盒回退目录里的队列子路径。
const QUEUE_SUBDIR: &str = "HaloFinder/Requests";
/// 扩展的 .appex 名称。
const APPEX_NAME: &str = "HaloFinderExtension.appex";
/// 菜单动作与执行之间通常只隔几百毫秒；太旧的请求不执行，避免 App 长时间
/// 未运行时开机后突然建出一堆文件。
const MAX_REQUEST_AGE_MS: u64 = 10 * 60 * 1000;
/// 轮询间隔。250ms 足够让右键动作看起来是即时的，也不会明显耗电。
const POLL_INTERVAL: Duration = Duration::from_millis(250);

/// 扩展写入的请求体。
///
/// 字段不能随手改名：Swift 侧 `FinderSync.swift` 的 `payload` 与此对应。
#[derive(Debug, Deserialize)]
struct ExtensionRequest {
    action: String,
    #[serde(default)]
    paths: Vec<String>,
    #[serde(default)]
    container: Option<String>,
    #[serde(default)]
    created_at: u64,
}

/// 启动轮询。只在 macOS 调用。
pub fn start(_app: AppHandle) {
    // 注册与「桌面右键接管」都放后台：`pluginkit -a` 是异步落库的，立刻读
    // `pluginkit -m -i` 会拿到「未安装」，于是桌面菜单按「未启用」初始化，
    // 用户看到的现象就是「右键菜单没 Halo 项了」。这里轮询到条目真的
    // 出现为止，再按真实状态启停桌面菜单。
    std::thread::Builder::new()
        .name("finder-extension-boot".into())
        .spawn(|| {
            register_and_verify();
        })
        .ok();

    tauri::async_runtime::spawn(async move {
        loop {
            drain();
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    });
}

/// 查询扩展状态。
pub fn status() -> FinderIntegrationStatus {
    let appex_present = appex_path()
        .map(|path| path.exists())
        .unwrap_or(false);
    let (installed, enabled) = pluginkit_state();
    FinderIntegrationStatus {
        supported: true,
        installed,
        enabled,
        bundle_id: EXTENSION_BUNDLE_ID.to_string(),
        appex_present,
    }
}

/// 启用 / 停用扩展。启用前确保已注册。
pub fn set_enabled(enabled: bool) -> Result<FinderIntegrationStatus, String> {
    if enabled {
        register_if_needed()?;
        // 桌面接管依赖「辅助功能」。这是用户主动点「启用扩展」的时机，
        // 系统会顺手把 Halo 加进列表并弹窗，勾选后监听线程 3 秒内自愈。
        if !crate::core::paste::has_accessibility_permission() {
            crate::core::paste::request_accessibility_permission();
        }
    }

    let action = if enabled { "use" } else { "ignore" };
    let output = Command::new(PLUGINKIT)
        .args(["-e", action, "-i", EXTENSION_BUNDLE_ID])
        .output()
        .map_err(|err| format!("无法执行 pluginkit：{err}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            format!("pluginkit -e {action} 退出码 {:?}", output.status.code())
        } else {
            format!("pluginkit -e {action} 失败：{stderr}")
        });
    }

    Ok(status())
}

/// 注册扩展并等到系统插件库真的能查到它，返回「是否启用」。
///
/// `pluginkit -a` 只把条目排进队列，`pluginkit -m -i` 往往要过几百毫秒
/// 才看得到；重打包后旧条目的签名失效时，这个间隔还会更长。所以这里查到
/// 出现为止（最多 2 秒），拿到的状态才是可以用来决定「桌面右键接管不接管」的。
fn register_and_verify() -> bool {
    if !resync_registration() {
        if let Err(err) = register_if_needed() {
            tracing::warn!("Finder 扩展注册失败: {err}");
        }
    }

    const ATTEMPTS: u32 = 20;
    for attempt in 0..ATTEMPTS {
        let (installed, enabled) = pluginkit_state();
        if installed {
            if attempt > 0 {
                tracing::info!(attempt, "Finder 扩展已就绪（等待插件库刷新）");
            }
            return enabled;
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    tracing::warn!("Finder 扩展注册后仍查不到条目，右键菜单保持关闭");
    false
}

/// 扩展包换过（重建、覆盖安装）时做一次干净重注册，返回是否已经注册。
///
/// 把新 `.appex` 直接拷进 App 而不清旧记录时，插件库里会同时留着新旧两条
/// 记录：Finder 加载两个扩展实例，两个实例各往菜单里插一遍，右键菜单里
/// 每一项就出现两遍。用「大小 + 修改时间」当指纹，只有包真的换了才执行
/// `-r`（移除旧记录）→ `-a`（重新注册），正常启动不做多余动作。
fn resync_registration() -> bool {
    let Some(appex) = appex_path() else {
        return false;
    };
    let Some(fingerprint) = appex_fingerprint(&appex) else {
        return false;
    };
    let marker = crate::storage::data_dir().join("finder-extension.fingerprint");
    // 标记内容形如 `<bundle id>|<大小>-<修改时间>`：带上 bundle id 是为了
    // 改名后不被旧记录挡住；旧版只写指纹，自然不等于新格式。
    let marker_text = fs::read_to_string(&marker).ok().unwrap_or_default();
    if marker_text == format!("{EXTENSION_BUNDLE_ID}|{fingerprint}") {
        return false;
    }

    // 用户可能主动关过扩展；重新注册后要把这个选择还原。
    //
    // 但只在「同一个插件包」上还原：改名/换 bundle id 后 pluginkit 里那条新记录
    // 天然是未启用状态，照搬过来会把刚注册的扩展直接置成忽略。
    let same_plugin = marker_text
        .split_once('|')
        .map(|(id, _)| id == EXTENSION_BUNDLE_ID)
        .unwrap_or(false);
    let (was_installed, was_enabled) = pluginkit_state();
    let _ = Command::new(PLUGINKIT).arg("-r").arg(&appex).status();

    match Command::new(PLUGINKIT).arg("-a").arg(&appex).output() {
        Ok(output) if output.status.success() => {}
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            tracing::warn!(%stderr, "Finder 扩展重新注册失败");
            return false;
        }
        Err(err) => {
            tracing::warn!("无法执行 pluginkit：{err}");
            return false;
        }
    }

    let election = if same_plugin && was_installed && !was_enabled {
        "ignore"
    } else {
        "use"
    };
    if let Err(err) = Command::new(PLUGINKIT)
        .args(["-e", election, "-i", EXTENSION_BUNDLE_ID])
        .status()
    {
        tracing::warn!("Finder 扩展启用状态恢复失败: {err}");
    }

    if let Some(dir) = marker.parent() {
        let _ = fs::create_dir_all(dir);
    }
    let _ = fs::write(&marker, format!("{EXTENSION_BUNDLE_ID}|{fingerprint}"));
    tracing::info!(election, "Finder 扩展已重新注册（清理重建留下的重复记录）");
    true
}

fn appex_fingerprint(appex: &Path) -> Option<String> {
    let meta = fs::metadata(appex).ok()?;
    let modified = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
    Some(format!("{}-{}", meta.len(), modified.as_nanos()))
}

/// 把当前 App 内的 `.appex` 注册进系统扩展数据库。
fn register_if_needed() -> Result<(), String> {
    if status().installed {
        return Ok(());
    }

    let appex = appex_path().ok_or_else(|| "无法定位当前 App 的 PlugIns 目录".to_string())?;
    if !appex.exists() {
        return Err(format!(
            "App 内没有 Finder 扩展（{}）。开发构建请用 npm run tauri build，不要用 tauri dev。",
            appex.display()
        ));
    }

    let output = Command::new(PLUGINKIT)
        .arg("-a")
        .arg(&appex)
        .output()
        .map_err(|err| format!("无法执行 pluginkit：{err}"))?;

    if output.status.success() {
        return Ok(());
    }

    // 个别系统版本要求传宿主 App 而不是 .appex；再试一次。
    if let Some(app) = app_bundle_path() {
        let retry = Command::new(PLUGINKIT)
            .arg("-a")
            .arg(&app)
            .output()
            .map_err(|err| format!("无法执行 pluginkit：{err}"))?;
        if retry.status.success() {
            return Ok(());
        }
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Err(if stderr.is_empty() {
        "pluginkit -a 注册扩展失败".to_string()
    } else {
        format!("pluginkit -a 注册扩展失败：{stderr}")
    })
}

/// 读 `pluginkit -m -i <bundle>`：是否已安装、是否启用。
///
/// 输出形如 `+    com.halo.desktop.FinderExtension(0.1.0)`；行首 `+` 表示
/// 启用，`-` 表示被忽略，空格表示默认状态（视为未启用）。
fn pluginkit_state() -> (bool, bool) {
    let output = Command::new(PLUGINKIT)
        .args(["-m", "-i", EXTENSION_BUNDLE_ID])
        .output();

    let Ok(output) = output else {
        return (false, false);
    };
    if !output.status.success() {
        return (false, false);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout
        .lines()
        .find(|line| line.contains(EXTENSION_BUNDLE_ID));
    match line {
        Some(line) => (true, line.trim_start().starts_with('+')),
        None => (false, false),
    }
}

/// `Halo.app/Contents/PlugIns/HaloFinderExtension.appex`。
fn appex_path() -> Option<PathBuf> {
    Some(
        app_bundle_path()?
            .join("Contents/PlugIns")
            .join(APPEX_NAME),
    )
}

/// `Halo.app`。
fn app_bundle_path() -> Option<PathBuf> {
    // .../Halo.app/Contents/MacOS/halo → Contents → Halo.app
    let exe = std::env::current_exe().ok()?;
    let contents = exe.parent()?.parent()?;
    contents.parent().map(Path::to_path_buf)
}

/// 扫描所有候选队列目录。
fn drain() {
    for dir in request_dirs() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        let mut requests: Vec<PathBuf> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
            .collect();
        // 文件名以毫秒时间戳开头，排序即用户点击顺序。
        requests.sort();

        for path in requests {
            process_file(&path);
        }
    }
}

fn process_file(path: &Path) {
    let data = match fs::read_to_string(path) {
        Ok(data) => data,
        Err(err) => {
            tracing::warn!("读取 Finder 请求失败 {}: {err}", path.display());
            let _ = fs::remove_file(path);
            return;
        }
    };

    let request: ExtensionRequest = match serde_json::from_str(&data) {
        Ok(request) => request,
        Err(err) => {
            tracing::warn!("Finder 请求不是合法 JSON {}: {err}", path.display());
            // 保留现场，便于定位扩展与主程序协议不一致。
            let bad = path.with_extension("json.bad");
            let _ = fs::rename(path, bad).or_else(|_| fs::remove_file(path));
            return;
        }
    };

    if is_stale(request.created_at) {
        tracing::info!("忽略过期的 Finder 请求 {}", path.display());
        let _ = fs::remove_file(path);
        return;
    }

    if let Err(err) = handle_request(request) {
        tracing::warn!("Finder 动作执行失败: {err}");
        super::notify("Halo", &format!("Finder 动作失败：{err}"));
    }

    if let Err(err) = fs::remove_file(path) {
        tracing::warn!("删除已处理的 Finder 请求失败 {}: {err}", path.display());
    }
}

fn handle_request(request: ExtensionRequest) -> Result<(), String> {
    let action = ServiceAction::from_message(&request.action)
        .ok_or_else(|| format!("未知动作「{}」", request.action))?;

    // 只接受绝对路径，且要求仍然存在；Finder 菜单打开后文件可能已被移走。
    // 路径只用于建文件 / 开终端，不拼进 shell（终端走 AppleScript 转义）。
    let selected: Vec<PathBuf> = request
        .paths
        .into_iter()
        .filter(|path| Path::new(path).is_absolute())
        .map(PathBuf::from)
        .filter(|path| path.exists())
        .collect();

    let container = request
        .container
        .filter(|path| Path::new(path).is_absolute())
        .map(PathBuf::from)
        .filter(|path| path.is_dir());

    super::macos::run_action(action, &selected, container.as_deref());
    Ok(())
}

fn is_stale(created_at_ms: u64) -> bool {
    // `created_at == 0` 是异常 / 伪造值（扩展应当写入真实时间戳），直接视为过期丢弃，
    // 不能当作「永不失效」——否则同用户进程可投递 `created_at:0` 的请求触发建文件 / 开终端。
    if created_at_ms == 0 {
        return true;
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0);
    now.saturating_sub(created_at_ms) > MAX_REQUEST_AGE_MS
}

/// 候选队列目录：App Group → 扩展容器 → 普通 Application Support。
///
/// 三个目录都可能不存在；扩展写哪个取决于它的签名方式。主程序只读，
/// 不主动创建（尤其不去创建别的进程的容器目录）。
fn request_dirs() -> Vec<PathBuf> {
    dirs::home_dir()
        .map(|home| request_dirs_in(&home))
        .unwrap_or_default()
}

fn request_dirs_in(home: &Path) -> Vec<PathBuf> {
    let library = home.join("Library");
    vec![
        library
            .join("Group Containers")
            .join(APP_GROUP_ID)
            .join("FinderRequests"),
        library
            .join("Containers")
            .join(EXTENSION_BUNDLE_ID)
            .join("Data/Library/Application Support")
            .join(QUEUE_SUBDIR),
        library.join("Application Support").join(QUEUE_SUBDIR),
    ]
}

#[cfg(target_os = "macos")]
const PLUGINKIT: &str = "/usr/bin/pluginkit";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_paths_cover_group_container_and_app_support() {
        let dirs = request_dirs_in(Path::new("/Users/demo"));
        assert_eq!(dirs.len(), 3);
        assert_eq!(
            dirs[0],
            Path::new("/Users/demo/Library/Group Containers/group.com.halo.desktop/FinderRequests")
        );
        assert_eq!(
            dirs[1],
            Path::new(
                "/Users/demo/Library/Containers/com.halo.desktop.FinderExtension/Data/Library/Application Support/HaloFinder/Requests"
            )
        );
        assert_eq!(
            dirs[2],
            Path::new("/Users/demo/Library/Application Support/HaloFinder/Requests")
        );
    }

    #[test]
    fn parses_extension_request() {
        let raw = r#"{
            "action": "newMarkdownFile",
            "paths": ["/Users/demo/Desktop/a.png"],
            "container": "/Users/demo/Desktop",
            "created_at": 1730000000000
        }"#;
        let request: ExtensionRequest = serde_json::from_str(raw).expect("parse request");
        assert_eq!(request.action, "newMarkdownFile");
        assert_eq!(request.paths.len(), 1);
        assert_eq!(request.container.as_deref(), Some("/Users/demo/Desktop"));
        assert_eq!(request.created_at, 1730000000000);
    }

    #[test]
    fn request_defaults_are_empty() {
        let request: ExtensionRequest =
            serde_json::from_str(r#"{"action":"openInTerminal"}"#).expect("parse minimal");
        assert!(request.paths.is_empty());
        assert!(request.container.is_none());
        assert_eq!(request.created_at, 0);
    }

    #[test]
    fn stale_requests_expire_and_zero_timestamp_is_stale() {
        assert!(is_stale(0));
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_millis() as u64;
        assert!(is_stale(now - MAX_REQUEST_AGE_MS - 1_000));
        assert!(!is_stale(now));
    }

    #[test]
    fn unknown_action_is_rejected_before_touching_files() {
        let request = ExtensionRequest {
            action: "newWpsWriter".to_string(),
            paths: Vec::new(),
            container: None,
            created_at: 0,
        };
        let err = handle_request(request).expect_err("unknown action");
        assert!(err.contains("未知动作"));
    }
}
