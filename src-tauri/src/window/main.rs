//! 主窗口与窗口材质。

use super::*;

use tauri::{AppHandle, Manager};
use window_vibrancy::{apply_vibrancy, NSVisualEffectMaterial, NSVisualEffectState};

///
/// 存在 Rust 侧而不是 localStorage：两个 webview 是否共享 localStorage 在
/// WKWebView 下没有保证，而 AppState 是进程内唯一可信的共享点。
static PENDING_PAGE: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// 记录待跳转页并显示主窗口（供面板/托盘调用）。
pub fn show_main_window_with_page(app: &AppHandle, page: Option<String>) {
    if let Some(p) = page {
        if let Ok(mut slot) = PENDING_PAGE.lock() {
            *slot = Some(p.clone());
        }
        // 通知已挂载的主窗口立即导航；未挂载时由 consume 路径兜底。
        let _ = app.emit("halo://navigate", p);
    }
    show_main_window(app);
}

/// 取出并清除待跳转页（主窗口挂载时调用一次）。
pub fn consume_pending_page() -> Option<String> {
    PENDING_PAGE.lock().ok()?.take()
}

/// 显示管理窗口。
///
/// 尺寸/位置由 `tauri-plugin-window-state` 持久化（评审 S9），这里只负责
/// 上屏与激活：App 以 `LSUIElement`（纯菜单栏）运行，不显式激活的话窗口
/// 会开在其它 App 后面（评审 M5）。
pub fn show_main_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    activate_app();
    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();
}

/// 把 App 从「隐藏」状态恢复并激活。
///
/// 纯菜单栏 App 没有 Dock 图标，窗口上屏时必须自己抢一次前台，否则用户
/// 点托盘「打开主窗口」只会在后台多出一个看不见的窗口。
#[cfg(target_os = "macos")]
pub fn activate_app() {
    use objc2::runtime::{AnyObject, Bool};
    use objc2::{class, msg_send};

    unsafe {
        let ns_app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
        if ns_app.is_null() {
            return;
        }
        let _: () = msg_send![ns_app, unhide: std::ptr::null_mut::<AnyObject>()];
        // activateIgnoringOtherApps: 在 macOS 14 已废弃，但仍是「从后台抢前台」
        // 唯一不需要用户额外点击的手段；新 API 需要 cooperative activation 令牌。
        let _: () = msg_send![ns_app, activateIgnoringOtherApps: Bool::YES];
    }
}

#[cfg(not(target_os = "macos"))]
pub fn activate_app() {}

/// 给窗口垫一层真实毛玻璃（评审 M3 / M6）。
///
/// `backdrop-filter` 在透明 WKWebView 里只能采样 webview 自身像素、采不到桌面，
/// 真正的桌面模糊必须由原生 `NSVisualEffectView` 提供：窗口 transparent、
/// webview 背景透明，用户看到的材质就是这一层。`radius` 同时解决了圆角：
/// 窗口级阴影会跟着这层 view 的形状走，不再是直角矩形阴影。
///
/// 只能在主线程调用（setup 内）。
pub fn apply_material(window: &WebviewWindow, material: NSVisualEffectMaterial, radius: f64) {
    #[cfg(target_os = "macos")]
    if let Err(e) = apply_vibrancy(
        window.clone(),
        material,
        Some(NSVisualEffectState::Active),
        Some(radius),
    ) {
        tracing::warn!("apply vibrancy failed: {e}");
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (window, material, radius);
}

/// 面板材质：HUD 窗口（Spotlight/音量 HUD 同款，深色下更透更暗）。
pub fn panel_material() -> NSVisualEffectMaterial {
    NSVisualEffectMaterial::HudWindow
}

/// 管理窗口材质：侧边栏（透出桌面、浅色下是磨砂白）。
pub fn window_material() -> NSVisualEffectMaterial {
    NSVisualEffectMaterial::Sidebar
}

/* ------------------------------------------------------------------ */
/* 剪贴板面板                                                          */
/* ------------------------------------------------------------------ */

