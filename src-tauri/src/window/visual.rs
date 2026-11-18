//! 系统外观偏好：减弱透明度与窗口外观。
//!
//! 毛玻璃由 `NSVisualEffectView` 提供（见 `main.rs`），但两件事它不替我们做：
//! - 「减弱透明度」需要把 webview 里所有半透明表面换成实色，否则桌面仍会
//!   从面板、菜单、Toast 里透出来；
//! - 应用内强制浅色/深色时，原生材质仍跟随系统，会和 webview 内容对不上。
//!
//! 这里读 `NSWorkspace` 的辅助功能读数并监听变化，前端据此在根节点挂
//! `.reduce-transparency`（CSS 媒体查询不支持时也有兜底）。

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{AppHandle, Emitter, Manager};

/// 是否开启「系统设置 → 辅助功能 → 显示 → 降低透明度」。
#[cfg(target_os = "macos")]
pub fn reduce_transparency() -> bool {
    use objc2::runtime::{AnyObject, Bool};
    use objc2::{class, msg_send};

    unsafe {
        let workspace: *mut AnyObject = msg_send![class!(NSWorkspace), sharedWorkspace];
        if workspace.is_null() {
            return false;
        }
        let value: Bool = msg_send![workspace, accessibilityDisplayShouldReduceTransparency];
        value.as_bool()
    }
}

#[cfg(not(target_os = "macos"))]
pub fn reduce_transparency() -> bool {
    false
}

/// 把应用主题同步到窗口外观。
///
/// webview 的 `:root.light` 只换 CSS 颜色；窗口外观决定原生毛玻璃、
/// 滚动条与系统控件是浅色还是深色。不同步的话「应用内强制深色 + 系统浅色」
/// 会得到深色内容浮在磨砂白上。
pub fn sync_theme(app: &AppHandle, theme: &str) {
    use tauri::Theme;

    let theme = match theme {
        "light" => Some(Theme::Light),
        "dark" => Some(Theme::Dark),
        _ => None,
    };
    for label in ["panel", "traypanel", "main"] {
        if let Some(window) = app.get_webview_window(label) {
            let _ = window.set_theme(theme);
        }
    }
}

/// 监听系统辅助功能显示选项的变化，并把最新状态广播给所有 webview。
///
/// 订阅一次即可；回调里再投递到主线程，避免在通知线程上碰窗口。
#[cfg(target_os = "macos")]
pub fn watch_display_preferences(app: AppHandle) {
    use block2::RcBlock;
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};

    static WATCHING: AtomicBool = AtomicBool::new(false);
    if WATCHING.swap(true, Ordering::SeqCst) {
        return;
    }

    unsafe {
        let center: *mut AnyObject = msg_send![class!(NSNotificationCenter), defaultCenter];
        if center.is_null() {
            return;
        }
        // NSWorkspace 的常量值就是它自己的名字，直接按字符串取通知名。
        let name: *mut AnyObject =
            msg_send![class!(NSString), stringWithUTF8String: c"NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification".as_ptr()];
        let handle = app.clone();
        let handler = RcBlock::new(move |_note: *mut AnyObject| {
            let handle = handle.clone();
            let _ = handle.clone().run_on_main_thread(move || {
                let reduce = reduce_transparency();
                let _ = handle.emit("halo://reduce-transparency", reduce);
            });
        });
        let _: *mut AnyObject = msg_send![
            center,
            addObserverForName: name,
            object: std::ptr::null_mut::<AnyObject>(),
            queue: std::ptr::null_mut::<AnyObject>(),
            usingBlock: &*handler
        ];
    }
}

#[cfg(not(target_os = "macos"))]
pub fn watch_display_preferences(_app: AppHandle) {}
