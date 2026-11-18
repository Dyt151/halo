//! 面板窗口 / 主窗口的显示、隐藏与定位。
//!
//! Halo 有两个窗口，职责严格分离（与 iCopy 等原生剪贴板工具一致）：
//! - `panel`：快捷键唤出的**剪贴板面板**，无边框、贴底停靠（或跟随光标），
//!   只承载历史卡片流；失焦即隐藏。
//! - `main`：**管理窗口**（设置/历史/环形转换/模块），从面板入口打开，
//!   关闭时隐藏到后台。
//!
//! 这一点很关键：早期版本二者共用一个窗口，快捷键只是把管理窗口压成底部条，
//! 导致面板里出现导航与设置残留，观感与原生工具完全不同。

use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use tauri::{
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Monitor, Position, Size,
    WebviewWindow,
};

use crate::config::settings::Config;
use crate::lock_ext::LockExt;

/// 面板「刚显示」的时间戳（毫秒），用于失焦事件的宽限期判断。
static PANEL_SHOWN_AT_MS: AtomicU64 = AtomicU64::new(0);
/// 面板是否已获得过焦点。二者配合：宽限期内忽略失焦；宽限期外失焦即隐藏。
static PANEL_FOCUSED: AtomicBool = AtomicBool::new(false);

/// 面板宽度上限（逻辑像素）。
///
/// 贴底停靠时不能真的占满整块屏：超宽屏（27" 以上 / 带鱼屏）上 168px 卡片横排
/// 会拉出一条数百厘米的视觉长带，观感失衡（评审 U6）。超出上限即居中，两侧留白。
const PANEL_MAX_WIDTH: f64 = 1180.0;

/// 面板与工作区的最小间距（左右/上下各留）：避免贴边，也让居中时两侧留白对称。
const PANEL_MARGIN: f64 = 24.0;

/// 失焦宽限期：`show()` 后系统可能补发失焦事件，短于该间隔的一律忽略。
const FOCUS_GRACE_MS: u64 = 400;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/* ------------------------------------------------------------------ */
/* 鼠标 / 多显示器定位                                                  */
/* ------------------------------------------------------------------ */

/// 鼠标指针的屏幕逻辑坐标：**原点在主屏左上角**、单位 point，与
/// [`LogicalPosition`] 以及拖放回调里 `CGEventGetLocation` 的坐标系完全一致。
///
/// 为什么不用 `window.cursor_position()`：tao 0.37 在 macOS 上把
/// `NSEvent.mouseLocation`（point、左下原点）与 `CGDisplay.pixels_high`（pixel）
/// 混算后再乘一次缩放比，Retina 屏（scale = 2）上纵坐标会偏出约 3 倍——
/// `follow_cursor` 会把面板钳到屏幕底部，浮动盘也会落到屏幕外。
/// 这里直接用 CoreGraphics 取鼠标事件的全局坐标：原生就是左上原点 + point。
fn cursor_logical_position(window: &WebviewWindow) -> Option<(f64, f64)> {
    #[cfg(target_os = "macos")]
    {
        let _ = window;
        #[repr(C)]
        struct CGPoint {
            x: f64,
            y: f64,
        }
        #[link(name = "CoreGraphics", kind = "framework")]
        extern "C" {
            fn CGEventCreate(source: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
            fn CGEventGetLocation(event: *mut std::ffi::c_void) -> CGPoint;
        }
        unsafe {
            // 源传 NULL = 取当前鼠标状态的一次性快照，不需要辅助功能权限。
            let event = CGEventCreate(std::ptr::null_mut());
            if event.is_null() {
                return None;
            }
            let point = CGEventGetLocation(event);
            // CGEvent 是 CF 对象，必须手动释放（每次唤出面板都会走到这里）。
            #[link(name = "CoreFoundation", kind = "framework")]
            extern "C" {
                // 签名与 `core/statusbar/cf.rs` 里的声明保持一致（同一符号只能有一种原型）。
                fn CFRelease(cf: *const std::ffi::c_void);
            }
            CFRelease(event.cast_const());
            Some((point.x, point.y))
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        // 非 macOS 平台 tao 的实现是正确的（物理像素），按窗口缩放比换算即可。
        let scale = window.scale_factor().unwrap_or(1.0).max(1.0);
        window
            .cursor_position()
            .ok()
            .map(|p| (p.x / scale, p.y / scale))
    }
}

/// 找出包含给定逻辑坐标的那块屏（各屏按自身缩放比换算到逻辑坐标）。
fn monitor_containing(window: &WebviewWindow, (cx, cy): (f64, f64)) -> Option<Monitor> {
    let monitors = window.available_monitors().ok()?;
    monitors.into_iter().find(|m| {
        let scale = m.scale_factor().max(1.0);
        let x = m.position().x as f64 / scale;
        let y = m.position().y as f64 / scale;
        let w = m.size().width as f64 / scale;
        let h = m.size().height as f64 / scale;
        cx >= x && cx < x + w && cy >= y && cy < y + h
    })
}

/// 光标所在的那块屏；取不到光标/未命中时退回窗口所在屏，再不行退回主屏。
///
/// 面板与浮动盘都按这个顺序选屏：隐藏中的窗口仍「停在」上一次的位置，
/// 只用 `current_monitor()` 会把热键唤出的面板钉在另一台显示器上。
fn monitor_at_cursor(window: &WebviewWindow) -> Option<Monitor> {
    cursor_logical_position(window)
        .and_then(|p| monitor_containing(window, p))
        .or_else(|| window.current_monitor().ok().flatten())
        .or_else(|| window.primary_monitor().ok().flatten())
}

/// 浮动面板与工作区边缘的最小间距（避开菜单栏 / Dock）。
pub(super) const FLOAT_MARGIN: f64 = 12.0;

/// 设置 NSWindow 的 window level（仅 macOS）。
#[cfg(target_os = "macos")]
fn set_window_level(window: &WebviewWindow, level: isize) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;

    let Ok(ptr) = window.ns_window() else {
        return;
    };
    let ns_window = ptr as *mut AnyObject;
    if ns_window.is_null() {
        return;
    }
    // 在 UI 线程调用 setLevel: 是安全的（本函数由快捷键/托盘事件触发，均在主线程）。
    unsafe {
        let _: () = msg_send![ns_window, setLevel: level];
    }
}

/* ------------------------------------------------------------------ */
/* 管理窗口                                                            */
/* ------------------------------------------------------------------ */

/// 待跳转页（面板/托盘请求主窗口落到某一页）。

mod cmdbar;
mod float;
mod main;
mod panel;
mod tray_panel;
mod visual;

pub use cmdbar::*;
pub use float::*;
pub use main::*;
pub use panel::*;
pub use tray_panel::*;
pub use visual::*;
