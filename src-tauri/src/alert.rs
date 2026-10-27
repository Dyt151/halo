//! 原生提示框（`NSAlert`）：托盘/菜单等没有 webview 上下文的入口用它做确认与报错。
//!
//! 为什么不用前端 `lib/dialog.ts`：托盘菜单点击时面板/主窗口可能都没显示，
//! 自绘浮层没有宿主可挂。`NSAlert` 同时符合 macOS 的「破坏性操作必须二次确认」惯例。
//!
//! 调用约束：`runModal` 必须在主线程执行（本项目所有调用点都在托盘/菜单回调里）。

#[cfg(target_os = "macos")]
use objc2::runtime::AnyObject;
#[cfg(target_os = "macos")]
use objc2::{class, msg_send};

/// NSAlert 的第一个按钮返回值。
#[cfg(target_os = "macos")]
const NS_ALERT_FIRST_BUTTON: i64 = 1000;
/// NSAlert 的警告图标（`NSAlertStyleWarning`）。
#[cfg(target_os = "macos")]
const NS_WARNING: i64 = 0;

#[cfg(target_os = "macos")]
fn nsstring(value: &str) -> *mut AnyObject {
    let c = std::ffi::CString::new(value).unwrap_or_default();
    // stringWithUTF8String: 返回 autoreleased 对象，runModal 期间有效。
    unsafe { msg_send![class!(NSString), stringWithUTF8String: c.as_ptr()] }
}

/// 破坏性操作确认：返回 true 表示用户点了确认按钮。
#[cfg(target_os = "macos")]
pub fn confirm(title: &str, message: &str, ok: &str, cancel: &str) -> bool {
    unsafe {
        let alert: *mut AnyObject = msg_send![class!(NSAlert), new];
        if alert.is_null() {
            return false;
        }
        let _: () = msg_send![alert, setAlertStyle: NS_WARNING];
        let _: () = msg_send![alert, setMessageText: nsstring(title)];
        let _: () = msg_send![alert, setInformativeText: nsstring(message)];
        // 第一个按钮是「取消」时返回值判定会反掉，这里固定「确认」在首位。
        let _: () = msg_send![alert, addButtonWithTitle: nsstring(ok)];
        let _: () = msg_send![alert, addButtonWithTitle: nsstring(cancel)];
        let response: i64 = msg_send![alert, runModal];
        let _: () = msg_send![alert, release];
        response == NS_ALERT_FIRST_BUTTON
    }
}

/// 单按钮错误提示（如「在终端中打开」失败）。
#[cfg(target_os = "macos")]
pub fn alert(title: &str, message: &str) {
    unsafe {
        let alert: *mut AnyObject = msg_send![class!(NSAlert), new];
        if alert.is_null() {
            return;
        }
        let _: () = msg_send![alert, setAlertStyle: NS_WARNING];
        let _: () = msg_send![alert, setMessageText: nsstring(title)];
        let _: () = msg_send![alert, setInformativeText: nsstring(message)];
        let _: () = msg_send![alert, addButtonWithTitle: nsstring("好")];
        let _: () = msg_send![alert, runModal];
        let _: () = msg_send![alert, release];
    }
}

#[cfg(not(target_os = "macos"))]
pub fn confirm(title: &str, message: &str, ok: &str, cancel: &str) -> bool {
    let _ = (title, message, ok, cancel);
    true
}

#[cfg(not(target_os = "macos"))]
pub fn alert(title: &str, message: &str) {
    tracing::warn!("{title}: {message}");
}
