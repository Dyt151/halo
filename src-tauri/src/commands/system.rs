//! 系统级命令：撤销/重做、原生文件选择、登录项、App 信息、面板收起。
//!
//! 这些能力都直接对接 AppKit / ServiceManagement，无对应前端插件，
//! 因此统一放在这里，由 `lib.rs` 注册。

use std::sync::mpsc;
use std::time::Duration;

use serde::Serialize;
use tauri::{Emitter, State};

use crate::core::clipboard::types::ClipItem;
use crate::error::{AppError, CommandResult};
use crate::state::AppState;

/// 原生选择面板的超时：用户可能把面板晾在一边很久，给足时间。
const PICK_TIMEOUT: Duration = Duration::from_secs(600);

/* ------------------------------------------------------------------ */
/* 撤销 / 重做                                                         */
/* ------------------------------------------------------------------ */

#[tauri::command]
pub fn undo(state: State<'_, AppState>) -> CommandResult<Option<ClipItem>> {
    let restored = state.history.undo()?;
    if restored.is_some() {
        let _ = state.app.emit(crate::events::HISTORY_DELETED, ());
    }
    Ok(restored)
}

#[tauri::command]
pub fn redo(state: State<'_, AppState>) -> CommandResult<Option<i64>> {
    let removed = state.history.redo()?;
    if removed.is_some() {
        let _ = state.app.emit(crate::events::HISTORY_DELETED, ());
    }
    Ok(removed)
}

/// 全部分组标签（不受当前筛选影响，修正「标签栏自我收窄」）。
#[tauri::command]
pub fn list_tags(state: State<'_, AppState>) -> CommandResult<Vec<String>> {
    state.history.tags()
}

/// 系统「降低透明度」是否开启（前端首帧据此决定用不用实色表面）。
#[tauri::command]
pub fn reduce_transparency() -> bool {
    crate::window::reduce_transparency()
}

/// 退出 Halo。
///
/// 菜单栏图标右键菜单里有「退出」，设置页「关于」也保留一个按钮指向这里，
/// 保证不用键盘也能退出。
#[tauri::command]
pub fn quit_app(app: tauri::AppHandle) {
    // 先把面板收起来，再退进程：否则窗口与进程消失的时序不一致，
    // 屏幕上会闪一两帧空底。
    crate::window::hide_panel_window(&app);
    app.exit(0);
}

/* ------------------------------------------------------------------ */
/* 原生文件/目录选择                                                    */
/* ------------------------------------------------------------------ */

/// 原生「打开文件」面板（可多选）。取消返回空数组。
#[tauri::command]
pub fn pick_files(app: tauri::AppHandle, multiple: Option<bool>) -> CommandResult<Vec<String>> {
    pick(&app, multiple.unwrap_or(false), false)
}

/// 原生「选择文件夹」面板。取消返回空数组。
#[tauri::command]
pub fn pick_directory(app: tauri::AppHandle) -> CommandResult<Vec<String>> {
    pick(&app, false, true)
}

fn pick(app: &tauri::AppHandle, multiple: bool, dirs: bool) -> CommandResult<Vec<String>> {
    #[cfg(target_os = "macos")]
    {
        // NSOpenPanel 必须在主线程 runModal；命令本身跑在工作线程，
        // 因此投递到主线程并同步等结果（用户交互期间阻塞该工作线程是预期行为）。
        let (tx, rx) = mpsc::channel();
        app.run_on_main_thread(move || {
            let _ = tx.send(run_open_panel(multiple, dirs));
        })
        .map_err(|e| AppError::with_details(9100, "无法打开文件选择面板", e.to_string()))?;
        rx.recv_timeout(PICK_TIMEOUT)
            .map_err(|e| AppError::with_details(9101, "文件选择超时", e.to_string()))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, multiple, dirs);
        Err(AppError::new(9102, "当前平台暂不支持原生文件选择"))
    }
}

#[cfg(target_os = "macos")]
fn run_open_panel(multiple: bool, dirs: bool) -> Vec<String> {
    use objc2::runtime::{AnyObject, Bool};
    use objc2::{class, msg_send};

    unsafe {
        let panel: *mut AnyObject = msg_send![class!(NSOpenPanel), openPanel];
        if panel.is_null() {
            return Vec::new();
        }
        let _: () = msg_send![panel, setCanChooseFiles: Bool::from(!dirs)];
        let _: () = msg_send![panel, setCanChooseDirectories: Bool::from(dirs)];
        let _: () = msg_send![panel, setAllowsMultipleSelection: Bool::from(multiple)];
        let _: () = msg_send![panel, setResolvesAliases: Bool::YES];
        let _: () = msg_send![panel, setMessage: nsstring(if dirs {
            "请选择文件夹"
        } else {
            "请选择文件"
        })];
        // NSModalResponseOK == 1
        let response: i64 = msg_send![panel, runModal];
        if response != 1 {
            return Vec::new();
        }
        let urls: *mut AnyObject = msg_send![panel, URLs];
        if urls.is_null() {
            return Vec::new();
        }
        let count: usize = msg_send![urls, count];
        let mut out = Vec::with_capacity(count);
        for i in 0..count {
            let url: *mut AnyObject = msg_send![urls, objectAtIndex: i];
            if url.is_null() {
                continue;
            }
            let path: *mut AnyObject = msg_send![url, path];
            if path.is_null() {
                continue;
            }
            let utf8: *const std::os::raw::c_char = msg_send![path, UTF8String];
            if utf8.is_null() {
                continue;
            }
            if let Ok(s) = std::ffi::CStr::from_ptr(utf8).to_str() {
                out.push(s.to_string());
            }
        }
        out
    }
}

#[cfg(target_os = "macos")]
unsafe fn nsstring(value: &str) -> *mut objc2::runtime::AnyObject {
    use objc2::{class, msg_send};
    let c = std::ffi::CString::new(value).unwrap_or_default();
    msg_send![class!(NSString), stringWithUTF8String: c.as_ptr()]
}

/* ------------------------------------------------------------------ */
/* 登录项（SMAppService）                                               */
/* ------------------------------------------------------------------ */

/// 登录项状态：`enabled` 为 true 表示已随登录启动。
#[derive(Debug, Clone, Serialize)]
pub struct LoginItemStatus {
    pub supported: bool,
    pub enabled: bool,
    /// requires_approval：用户需要在「系统设置 → 通用 → 登录项」里手动允许。
    pub requires_approval: bool,
}

#[tauri::command]
pub fn login_item_status() -> LoginItemStatus {
    #[cfg(target_os = "macos")]
    {
        let status = macos::sm_status();
        LoginItemStatus {
            supported: true,
            enabled: status == 1,
            requires_approval: status == 2,
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        LoginItemStatus {
            supported: false,
            enabled: false,
            requires_approval: false,
        }
    }
}

/// 注册/取消注册登录项。返回注册后的最新状态。
#[tauri::command]
pub fn set_login_item(state: State<'_, AppState>, enabled: bool) -> CommandResult<LoginItemStatus> {
    #[cfg(target_os = "macos")]
    {
        macos::sm_set(enabled)?;
        // 配置只是镜像，真实状态以系统为准（下次启动时会用系统值回写 UI）。
        //
        // 镜像写失败不能让它静默消失：系统登录项已经生效，配置却没跟上时，
        // 界面下次读到的还是旧值。这里记日志而不是直接返回错误——
        // 用户的操作（开/关开机启动）本身已经成功了，报错会让人以为没生效。
        if let Err(e) = state
            .config
            .patch(serde_json::json!({ "launch_at_login": enabled }))
        {
            tracing::warn!("写回 launch_at_login 镜像失败: {e}");
        }
        Ok(login_item_status())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (state, enabled);
        Err(AppError::new(9103, "当前平台暂不支持登录项注册"))
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use objc2::runtime::{AnyObject, Bool};
    use objc2::{class, msg_send};

    use crate::error::{AppError, CommandResult};

    // ServiceManagement 不会被 AppKit 隐式加载，必须显式链接；
    // 链接后 `class!(SMAppService)` 才能取到类对象。
    #[link(name = "ServiceManagement", kind = "framework")]
    extern "C" {}

    fn service() -> *mut AnyObject {
        unsafe { msg_send![class!(SMAppService), mainAppService] }
    }

    /// SMAppServiceStatus: 0 = notRegistered, 1 = enabled, 2 = requiresApproval, 3 = notFound。
    pub fn sm_status() -> i64 {
        unsafe {
            let svc = service();
            if svc.is_null() {
                return 0;
            }
            msg_send![svc, status]
        }
    }

    pub fn sm_set(enabled: bool) -> CommandResult<()> {
        unsafe {
            let svc = service();
            if svc.is_null() {
                return Err(AppError::new(9104, "无法访问登录项服务"));
            }
            let mut error: *mut AnyObject = std::ptr::null_mut();
            let ok: Bool = if enabled {
                msg_send![svc, registerAndReturnError: &mut error as *mut *mut AnyObject]
            } else {
                msg_send![svc, unregisterAndReturnError: &mut error as *mut *mut AnyObject]
            };
            if !bool::from(ok) {
                let message = if error.is_null() {
                    "登录项注册失败".to_string()
                } else {
                    let desc: *mut AnyObject = msg_send![error, localizedDescription];
                    let utf8: *const std::os::raw::c_char = msg_send![desc, UTF8String];
                    if utf8.is_null() {
                        "登录项注册失败".to_string()
                    } else {
                        std::ffi::CStr::from_ptr(utf8)
                            .to_string_lossy()
                            .into_owned()
                    }
                };
                return Err(AppError::with_details(9105, "登录项注册失败", message));
            }
            Ok(())
        }
    }
}

/* ------------------------------------------------------------------ */
/* App 信息（关于页）                                                   */
/* ------------------------------------------------------------------ */

/// 关于页展示用信息（版本 / 架构 / 数据目录）。
#[derive(Debug, Clone, Serialize)]
pub struct AppInfo {
    pub version: String,
    pub arch: String,
    pub data_dir: String,
    pub db_path: String,
}

#[tauri::command]
pub fn app_info(app: tauri::AppHandle) -> AppInfo {
    AppInfo {
        version: app.package_info().version.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        data_dir: crate::storage::data_dir().to_string_lossy().into_owned(),
        db_path: crate::storage::db_path().to_string_lossy().into_owned(),
    }
}

/* ------------------------------------------------------------------ */
/* 面板收起                                                            */
/* ------------------------------------------------------------------ */

/// 由前端在收起动画播完后调用（动画见 `Panel` 的 `closing` 状态）。
#[tauri::command]
pub fn hide_panel(app: tauri::AppHandle) {
    crate::window::hide_panel_window(&app);
}
