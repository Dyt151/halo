//! macOS 原生通知：`UNUserNotificationCenter`。
//!
//! 首次通知时请求「横幅 + 声音」授权，之后直接投递；授权被拒、框架不可用
//! 或进程没有 bundle identifier（`cargo run` 的裸二进制）时返回 `false`，
//! 由调用方回退到 AppleScript，保证任何运行方式下都有反馈。
//!
//! UserNotifications 没有 Rust 绑定，类按需从运行时查找；框架先 `dlopen`，
//! 否则 `objc_getClass` 看不到这些类。

use std::ffi::CString;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

use block2::RcBlock;
use objc2::runtime::{AnyClass, AnyObject, Bool};
use objc2::{class, msg_send};

/// `UNAuthorizationOptions`：alert = 4，sound = 2。
const AUTH_OPTIONS: usize = 4 | 2;

const AUTH_UNKNOWN: u8 = 0;
const AUTH_GRANTED: u8 = 1;
const AUTH_DENIED: u8 = 2;
const AUTH_REQUESTING: u8 = 3;

static AUTH: AtomicU8 = AtomicU8::new(AUTH_UNKNOWN);
static FRAMEWORK: OnceLock<bool> = OnceLock::new();

/// 投递一条通知；返回 `false` 表示原生路径不可用，调用方应回退。
pub fn post(title: &str, message: &str) -> bool {
    let framework = framework_loaded();
    let bundled = has_bundle_identifier();
    if !framework || !bundled {
        tracing::debug!(framework, bundled, "native notification unavailable");
        return false;
    }
    match AUTH.load(Ordering::SeqCst) {
        AUTH_GRANTED => {
            unsafe { deliver(title, message) };
            true
        }
        AUTH_DENIED => false,
        AUTH_REQUESTING => false,
        _ => request_then_deliver(title.to_string(), message.to_string()),
    }
}

fn framework_loaded() -> bool {
    *FRAMEWORK.get_or_init(|| unsafe {
        !libc::dlopen(
            c"/System/Library/Frameworks/UserNotifications.framework/UserNotifications".as_ptr(),
            libc::RTLD_LAZY,
        )
        .is_null()
    })
}

/// 没有 bundle 的裸二进制调通知中心会直接抛异常，先确认自己是 App。
fn has_bundle_identifier() -> bool {
    unsafe {
        let bundle: *mut AnyObject = msg_send![class!(NSBundle), mainBundle];
        if bundle.is_null() {
            return false;
        }
        let identifier: *mut AnyObject = msg_send![bundle, bundleIdentifier];
        !identifier.is_null()
    }
}

fn request_then_deliver(title: String, message: String) -> bool {
    if AUTH
        .compare_exchange(
            AUTH_UNKNOWN,
            AUTH_REQUESTING,
            Ordering::SeqCst,
            Ordering::SeqCst,
        )
        .is_err()
    {
        return false;
    }

    unsafe {
        let Some(center) = notification_center() else {
            AUTH.store(AUTH_DENIED, Ordering::SeqCst);
            return false;
        };
        let handler = RcBlock::new(move |granted: Bool, error: *mut AnyObject| {
            let error = if error.is_null() {
                None
            } else {
                nsstring_to_string(msg_send![error, localizedDescription])
            };
            tracing::debug!(
                granted = granted.as_bool(),
                error = error.as_deref().unwrap_or(""),
                "notification authorization result"
            );
            if granted.as_bool() {
                AUTH.store(AUTH_GRANTED, Ordering::SeqCst);
                deliver(&title, &message);
            } else {
                AUTH.store(AUTH_DENIED, Ordering::SeqCst);
                super::osascript_notify(&title, &message);
            }
        });
        let _: () = msg_send![
            center,
            requestAuthorizationWithOptions: AUTH_OPTIONS,
            completionHandler: &*handler
        ];
    }
    true
}

unsafe fn notification_center() -> Option<*mut AnyObject> {
    let class = AnyClass::get("UNUserNotificationCenter")?;
    let center: *mut AnyObject = msg_send![class, currentNotificationCenter];
    (!center.is_null()).then_some(center)
}

unsafe fn deliver(title: &str, message: &str) {
    let (Some(content_class), Some(request_class), Some(sound_class), Some(center)) = (
        AnyClass::get("UNMutableNotificationContent"),
        AnyClass::get("UNNotificationRequest"),
        AnyClass::get("UNNotificationSound"),
        notification_center(),
    ) else {
        return;
    };

    let pool: *mut AnyObject = msg_send![class!(NSAutoreleasePool), new];
    let content: *mut AnyObject = msg_send![content_class, new];
    if !content.is_null() {
        let _: () = msg_send![content, setTitle: nsstring(title)];
        let _: () = msg_send![content, setBody: nsstring(message)];
        let sound: *mut AnyObject = msg_send![sound_class, defaultSound];
        if !sound.is_null() {
            let _: () = msg_send![content, setSound: sound];
        }

        let identifier = nsstring(&format!("halo-{}", uuid::Uuid::new_v4()));
        let request: *mut AnyObject = msg_send![
            request_class,
            requestWithIdentifier: identifier,
            content: content,
            trigger: std::ptr::null_mut::<AnyObject>()
        ];
        if !request.is_null() {
            let _: () = msg_send![
                center,
                addNotificationRequest: request,
                withCompletionHandler: std::ptr::null_mut::<std::ffi::c_void>()
            ];
        }
        let _: () = msg_send![content, release];
    }
    let _: () = msg_send![pool, release];
}

unsafe fn nsstring(value: &str) -> *mut AnyObject {
    let c = CString::new(value.replace('\0', "")).unwrap_or_default();
    msg_send![class!(NSString), stringWithUTF8String: c.as_ptr()]
}

unsafe fn nsstring_to_string(value: *mut AnyObject) -> Option<String> {
    if value.is_null() {
        return None;
    }
    let utf8: *const std::os::raw::c_char = msg_send![value, UTF8String];
    if utf8.is_null() {
        return None;
    }
    Some(
        std::ffi::CStr::from_ptr(utf8)
            .to_string_lossy()
            .into_owned(),
    )
}
