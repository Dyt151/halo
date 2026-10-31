//! 面板窗口 / 主窗口的显示、隐藏与定位。
//!
//! ClipBase 有两个窗口，职责严格分离（与 iCopy 等原生剪贴板工具一致）：
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
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Monitor, Position, Size, WebviewWindow,
};
use window_vibrancy::{apply_vibrancy, NSVisualEffectMaterial, NSVisualEffectState};

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
                // 签名与 `core/desktop_menu/macos.rs` 里的声明保持一致（同一符号只能有一种原型）。
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

/* ------------------------------------------------------------------ */
/* 管理窗口                                                            */
/* ------------------------------------------------------------------ */

/// 待跳转页（面板/托盘请求主窗口落到某一页）。
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
        let _ = app.emit("clipbase://navigate", p);
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

/// 隐藏管理窗口。
pub fn hide_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
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

/// 显示剪贴板面板（`panel`）：按配置设置尺寸与位置。
pub fn show_panel_window(app: &AppHandle) {
    let Some(panel) = app.get_webview_window("panel") else {
        return;
    };
    let config = app.state::<crate::state::AppState>().config.get();
    apply_panel_layout(&panel, &config);

    // 抬高窗口层级：剪贴板面板必须盖过其它 App 的浮动面板
    // （iCopy 等同类工具用 NSStatusWindowLevel=25，比 Tauri 默认的浮动层级高），
    // 否则同屏装了两个剪贴板工具时，我们的面板会被对方压住。
    #[cfg(target_os = "macos")]
    set_window_level(&panel, 25);
    // 每次唤出都重申一次：全屏 App 里按快捷键时，面板必须能进当前 Space。
    allow_over_fullscreen(&panel);

    // 捕获「唤出面板前」的前台 App，回贴时把焦点还给它（对标 iCopy 的
    // `preActiveApp` + `activateWithOptions:`）。必须在此刻（set_focus 之前）取，
    // 否则取到的是 ClipBase 自己，⌘V 会贴进面板而不是用户正在用的 App。
    //
    // 走 `remember_frontmost_app` 而不是无条件写入：前台可能就是 ClipBase
    //（管理窗口开着时按快捷键），那样 ⌘V 会贴回自己的搜索框，还会把「上一个
    // 真实目标 App」的 pid 冲掉。
    remember_frontmost_app();

    PANEL_SHOWN_AT_MS.store(now_ms(), Ordering::SeqCst);
    PANEL_FOCUSED.store(false, Ordering::SeqCst);
    // 顺序很关键：先让宿主（NSPanel）进入**当前** Space——全屏 Space 也算——
    // 再激活 App 抢键盘。直接对 Tauri 窗口 `show()` 会先触发激活，系统会把空间
    // 切回桌面，面板就留在桌面 Space（用户看到的就是「全屏里按快捷键没反应」）。
    #[cfg(target_os = "macos")]
    reveal_panel_host(&panel);
    #[cfg(target_os = "macos")]
    order_front_regardless(&panel);
    let _ = panel.show();
    let _ = panel.set_focus();
    // 面板尺寸/位置每次都会变，macOS 的窗口阴影按旧矩形缓存，
    // 不重算就会看到「圆角内容 + 直角阴影」的错位（评审 M6）。
    refresh_shadow(&panel);

    // 立即补检剪贴板：用户常「复制后立刻按快捷键」，常规轮询（默认 500ms）
    // 未必来得及捕获，面板里会短暂看不到刚复制的内容。这里异步补一次 poll
    //（与监控线程共享 changeCount / last_hash，天然去重），保证面板出现即最新。
    {
        // State 的 clone 只克隆引用视图，需 inner() 拿拥有的 AppState 才能进 spawn。
        let state = app.state::<crate::state::AppState>().inner().clone();
        tauri::async_runtime::spawn(async move {
            if let Ok(Some(ev)) = state.monitor.poll_once() {
                state.process_clip_event(ev);
            }
        });
    }

    // 通知前端「面板已出现」：重放出现动画、复位滚动与选中态。
    let _ = app.emit("clipbase://panel-shown", ());
}

/// 记录「唤出面板前」前台 App 的 pid（回贴后归还焦点用）。
static PRE_ACTIVE_APP_PID: AtomicI32 = AtomicI32::new(-1);

/// 取回「唤出面板前」前台 App 的 pid；无记录时返回 -1。
pub fn pre_active_app_pid() -> i32 {
    PRE_ACTIVE_APP_PID.load(Ordering::SeqCst)
}

/// 从托盘「最近 N 条」回贴前记录前台 App。
///
/// 这条路径不会先显示面板（[`show_panel_window`] 里的记录逻辑跑不到），
/// 若不复用陈旧的 pid，⌘V 会注入到上一次的面板目标 App（评审 S4）。
/// 若当前前台就是自己（比如用户正在面板里操作），保留原记录。
pub fn remember_frontmost_app() {
    let pid = capture_frontmost_app_pid();
    if pid <= 0 || pid == std::process::id() as i32 {
        return;
    }
    PRE_ACTIVE_APP_PID.store(pid, Ordering::SeqCst);
}

/// 捕获当前前台 App 的 processIdentifier（NSWorkspace.frontmostApplication）。
#[cfg(target_os = "macos")]
fn capture_frontmost_app_pid() -> i32 {
    use objc2::msg_send;
    use objc2::runtime::{AnyClass, AnyObject};

    let cls: &AnyClass = objc2::class!(NSWorkspace);
    unsafe {
        let workspace: *mut AnyObject = msg_send![cls, sharedWorkspace];
        if workspace.is_null() {
            return -1;
        }
        let app: *mut AnyObject = msg_send![workspace, frontmostApplication];
        if app.is_null() {
            return -1;
        }
        msg_send![app, processIdentifier]
    }
}

#[cfg(not(target_os = "macos"))]
fn capture_frontmost_app_pid() -> i32 {
    -1
}

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

/// 让窗口能压到别的 App 的**全屏 Space** 上（仅 macOS）。
///
/// Tauri 的 `visibleOnAllWorkspaces` 只设 `NSWindowCollectionBehaviorCanJoinAllSpaces`，
/// 缺了 `FullScreenAuxiliary` 时窗口属于「普通辅助窗口」：切到某个 App 的全屏
/// Space 后，快捷键唤出的面板只会出现在桌面 Space 上，全屏界面里看不到。
/// 置上这两位，窗口才允许在任意 Space（含全屏 Space）显示。
#[cfg(target_os = "macos")]
pub fn allow_over_fullscreen(window: &WebviewWindow) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;

    /// NSWindowCollectionBehaviorCanJoinAllSpaces
    const CAN_JOIN_ALL_SPACES: usize = 1 << 0;
    /// NSWindowCollectionBehaviorFullScreenAuxiliary
    const FULL_SCREEN_AUXILIARY: usize = 1 << 8;

    let Ok(ptr) = window.ns_window() else {
        return;
    };
    let ns_window = ptr as *mut AnyObject;
    if ns_window.is_null() {
        return;
    }
    unsafe {
        let behavior: usize = msg_send![ns_window, collectionBehavior];
        let updated = behavior | CAN_JOIN_ALL_SPACES | FULL_SCREEN_AUXILIARY;
        let _: () = msg_send![ns_window, setCollectionBehavior: updated];
    }
}

#[cfg(not(target_os = "macos"))]
pub fn allow_over_fullscreen(_window: &WebviewWindow) {}

/// 面板的「Space 宿主」窗口（仅 macOS）。
///
/// 别的 App 的全屏 Space 不接受 Tauri 的普通 `NSWindow`：哪怕设了
/// `CanJoinAllSpaces | FullScreenAuxiliary`，`orderFrontRegardless:` 之后它也只
/// 排在桌面 Space，用户在 Safari/终端全屏里按快捷键什么都看不到。
///
/// 解法是另建一个**真实的** `NSPanel`（borderless + nonactivatingPanel）当宿主，
/// 把面板挂成它的子窗口：宿主进哪个 Space，面板就跟到哪个 Space。面板本身仍是
/// Tauri/tao 的窗口，拖拽、键盘与失焦逻辑都不用改。
///
/// 不直接改 Tauri 窗口的类：`object_setClass` 换成 NSPanel 时内存仍按 NSWindow
/// 分配，NSPanel 多出来的实例变量会读到越界内存，`canBecomeKeyWindow` 与排序
/// 行为随之时好时坏（实测会退化成拿不到键盘）。
#[cfg(target_os = "macos")]
static PANEL_HOST: Mutex<usize> = Mutex::new(0);

#[cfg(target_os = "macos")]
fn ns_window_ptr(window: &WebviewWindow) -> Option<*mut objc2::runtime::AnyObject> {
    let ptr = window.ns_window().ok()? as *mut objc2::runtime::AnyObject;
    (!ptr.is_null()).then_some(ptr)
}

/// 创建面板宿主（每进程一个）并把面板挂成它的子窗口。只在 setup 里调用一次。
#[cfg(target_os = "macos")]
pub fn prepare_panel_host(window: &WebviewWindow) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;
    use objc2_foundation::CGRect;

    /// NSWindowStyleMaskNonactivatingPanel：不激活 App 也能收键盘。
    const STYLE_NONACTIVATING_PANEL: usize = 1 << 7;
    const BACKING_BUFFERED: usize = 2;
    const BEHAVIOR_CAN_JOIN_ALL_SPACES: usize = 1 << 0;
    const BEHAVIOR_FULL_SCREEN_AUXILIARY: usize = 1 << 8;
    /// NSWindowAbove
    const WINDOW_ABOVE: isize = 1;

    let Some(panel) = ns_window_ptr(window) else {
        return;
    };
    let mut host_slot = PANEL_HOST.lock_ok();
    if *host_slot != 0 {
        return;
    }

    unsafe {
        let alloc: *mut AnyObject = msg_send![objc2::class!(NSPanel), alloc];
        let host: *mut AnyObject = msg_send![
            alloc,
            initWithContentRect: CGRect::ZERO,
            styleMask: STYLE_NONACTIVATING_PANEL,
            backing: BACKING_BUFFERED,
            defer: false
        ];
        if host.is_null() {
            return;
        }
        let _: () = msg_send![host, setLevel: 25isize];
        let _: () = msg_send![host, setOpaque: false];
        let clear: *mut AnyObject = msg_send![objc2::class!(NSColor), clearColor];
        let _: () = msg_send![host, setBackgroundColor: clear];
        let _: () = msg_send![host, setHasShadow: false];
        let _: () = msg_send![
            host,
            setCollectionBehavior: BEHAVIOR_CAN_JOIN_ALL_SPACES | BEHAVIOR_FULL_SCREEN_AUXILIARY
        ];
        let _: () = msg_send![host, setHidesOnDeactivate: false];
        // 宿主只是 Space 锚点，鼠标一律穿透给下面的面板/其他 App。
        let _: () = msg_send![host, setIgnoresMouseEvents: true];
        let _: () = msg_send![host, setReleasedWhenClosed: false];
        let _: () = msg_send![host, setBecomesKeyOnlyIfNeeded: false];
        let _: () = msg_send![host, setFloatingPanel: true];
        let _: () = msg_send![host, addChildWindow: panel, ordered: WINDOW_ABOVE];
        *host_slot = host as usize;
    }
}

#[cfg(not(target_os = "macos"))]
pub fn prepare_panel_host(_window: &WebviewWindow) {}

/// 显示前把面板重新挂回宿主、对齐位置与尺寸，并把宿主排进当前 Space。
#[cfg(target_os = "macos")]
pub fn reveal_panel_host(window: &WebviewWindow) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;
    use objc2_foundation::CGRect;

    let Some(panel) = ns_window_ptr(window) else {
        return;
    };
    let host = *PANEL_HOST.lock_ok() as *mut AnyObject;
    if host.is_null() {
        return;
    }
    /// NSWindowAbove
    const WINDOW_ABOVE: isize = 1;
    unsafe {
        let frame: CGRect = msg_send![panel, frame];
        // 收起面板时 AppKit 会把子窗口从父窗口摘掉（实测 `orderOut:` 之后
        // `parentWindow` 变 nil）。不重新挂回去，面板就退回普通 NSWindow，
        // 再次唤出只会出现在桌面 Space —— 表现为「Esc 收起后全屏里叫不出来」。
        let _: () = msg_send![host, removeChildWindow: panel];
        let _: () = msg_send![host, addChildWindow: panel, ordered: WINDOW_ABOVE];
        // 挂回子窗口会把层级压到宿主的 floating 层级，补回状态栏层级，
        // 保证仍能盖过其它 App 的浮动面板（与 set_window_level 的取值一致）。
        let _: () = msg_send![panel, setLevel: 25isize];
        let _: () = msg_send![host, setFrame: frame, display: false];
        let _: () = msg_send![host, orderFrontRegardless];
    }
}

#[cfg(not(target_os = "macos"))]
pub fn reveal_panel_host(_window: &WebviewWindow) {}

/// 面板收起时一并收起宿主，避免 Space 里留一个透明空窗口。
#[cfg(target_os = "macos")]
pub fn conceal_panel_host() {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;

    let host = *PANEL_HOST.lock_ok() as *mut AnyObject;
    if host.is_null() {
        return;
    }
    unsafe {
        let _: () = msg_send![host, orderOut: std::ptr::null_mut::<AnyObject>()];
    }
}

#[cfg(not(target_os = "macos"))]
pub fn conceal_panel_host() {}

/// 不管 App 是否在前台，都把这个窗口排到当前 Space 的最前面（仅 macOS）。
///
/// `NSWindow.show()` 对应的 `makeKeyAndOrderFront:` 在 App 非前台时会等激活，
/// 而激活本身又会被别的 App 的全屏 Space 挡住——两头堵，窗口就留在桌面 Space。
#[cfg(target_os = "macos")]
fn order_front_regardless(window: &WebviewWindow) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;

    let Ok(ptr) = window.ns_window() else {
        return;
    };
    let ns_window = ptr as *mut AnyObject;
    if ns_window.is_null() {
        return;
    }
    unsafe {
        let _: () = msg_send![ns_window, orderFrontRegardless];
    }
}

/// 让 macOS 重新计算窗口阴影：窗口内容变成圆角后，旧的矩形阴影会露出直角。
#[cfg(target_os = "macos")]
fn refresh_shadow(window: &WebviewWindow) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;

    let Ok(ptr) = window.ns_window() else {
        return;
    };
    let ns_window = ptr as *mut AnyObject;
    if ns_window.is_null() {
        return;
    }
    unsafe {
        let _: () = msg_send![ns_window, invalidateShadow];
    }
}

#[cfg(not(target_os = "macos"))]
fn refresh_shadow(_window: &WebviewWindow) {}

/// 隐藏剪贴板面板。
pub fn hide_panel_window(app: &AppHandle) {
    if let Some(panel) = app.get_webview_window("panel") {
        PANEL_FOCUSED.store(false, Ordering::SeqCst);
        conceal_panel_host();
        let _ = panel.hide();
    }
}

/// 唤起/收起剪贴板面板（快捷键 Control+C 等的入口）。
pub fn toggle_panel_window(app: &AppHandle) {
    let Some(panel) = app.get_webview_window("panel") else {
        return;
    };
    if panel.is_visible().unwrap_or(false) {
        hide_panel_window(app);
    } else {
        show_panel_window(app);
    }
}

/// 面板失焦时隐藏（点击桌面/其他 App 即收起，与原生面板行为一致）。
///
/// 两个防误判：
/// 1. 宽限期：`show()` 之后系统可能补发失焦事件，短于 [`FOCUS_GRACE_MS`] 的一律忽略；
/// 2. 焦点复核：只有「确实曾拿到焦点」才隐藏，避免抢焦点失败后面板永久浮在顶层。
pub fn handle_panel_focus_lost(app: &AppHandle) {
    let shown_at = PANEL_SHOWN_AT_MS.load(Ordering::SeqCst);
    if shown_at > 0 && now_ms().saturating_sub(shown_at) < FOCUS_GRACE_MS {
        return;
    }
    if !PANEL_FOCUSED.swap(false, Ordering::SeqCst) {
        return;
    }
    // 双重确认：若实际上仍是焦点窗口（事件乱序），不隐藏。
    if let Some(panel) = app.get_webview_window("panel") {
        if panel.is_focused().unwrap_or(false) {
            PANEL_FOCUSED.store(true, Ordering::SeqCst);
            return;
        }
    }
    hide_panel_window(app);
}

/// 面板获得焦点：打标记，供失焦判断使用。
pub fn handle_panel_focus_gained() {
    PANEL_FOCUSED.store(true, Ordering::SeqCst);
}

/* ------------------------------------------------------------------ */
/* 环形转换盘 / 回复选择器（现场浮动面板）                              */
/* ------------------------------------------------------------------ */

/// 环形盘边长（逻辑像素）：单层最多 12 个扇区，边长按几何 `2×(R+PopOut+阴影14)`
/// 推算（R=150 → 342），见 docs/REA-Filee-Ring-Wheel.md §2.2（评审 C3 / 任务 20）。
const RING_SIZE: f64 = 342.0;
/// 浮动面板与工作区边缘的最小间距（避开菜单栏 / Dock，任务 24）。
const FLOAT_MARGIN: f64 = 12.0;

/// 环形窗当前的左上角（屏幕逻辑坐标）。
///
/// 拖放模式下窗口不接收鼠标（`ignore_cursor_events`），指针位置只能从全局手势旁听，
/// 在这里换成窗口内坐标再发给前端——比前端异步读窗口位置可靠。
static RING_WINDOW_ORIGIN: Mutex<Option<(f64, f64)>> = Mutex::new(None);

/// 环形窗左上角的屏幕坐标（不在拖放模式时为 `None`）。
pub fn ring_window_origin() -> Option<(f64, f64)> {
    *RING_WINDOW_ORIGIN.lock_ok()
}

/// 显示环形转换盘：钉在拖动起手点，且**从不抢焦点**。
///
/// 运行态只有 Option＋拖动一个入口（总规范 §3.1），所以定位只有「起手点」
/// 一种：拖动的是 Finder，抢焦点会打断它的鼠标追踪（松手事件就回不去了）。
/// 鼠标事件也必须穿过去：否则松手会投递给环形窗，Finder 会卡在拖拽状态。
pub fn show_ring_window(app: &AppHandle, origin: (f64, f64)) {
    let Some(ring) = app.get_webview_window("ring") else {
        tracing::warn!("环形盘窗口不存在，无法显示");
        return;
    };
    let (cx, cy) = origin;
    let placed = place_float_window_at(&ring, RING_SIZE, RING_SIZE, cx, cy);
    *RING_WINDOW_ORIGIN.lock_ok() = placed;
    set_float_level(&ring);
    allow_over_fullscreen(&ring);
    let _ = ring.set_ignore_cursor_events(true);
    if let Err(e) = ring.show() {
        tracing::warn!("显示环形盘失败: {e}");
    }
    tracing::info!(x = cx, y = cy, "环形盘窗口已显示");
    let _ = app.emit("clipbase://ring-shown", ());
}

/// 隐藏环形转换盘。
pub fn hide_ring_window(app: &AppHandle) {
    *RING_WINDOW_ORIGIN.lock_ok() = None;
    if let Some(ring) = app.get_webview_window("ring") {
        let _ = ring.hide();
    }
}

/// 浮动面板层级：与剪贴板面板一致（NSStatusWindowLevel=25），盖过其它 App 浮层。
#[cfg(target_os = "macos")]
fn set_float_level(window: &WebviewWindow) {
    set_window_level(window, 25);
}
#[cfg(not(target_os = "macos"))]
fn set_float_level(_window: &WebviewWindow) {}

/// 以「鼠标所在屏幕」为基准，把 `w×h` 的浮动窗口居中到光标附近，并把越界部分
/// 平移回收进工作区（任务 24：四边、菜单栏、Dock、多显示器）。
fn position_float_window(window: &WebviewWindow, w: f64, h: f64) {
    if let Some((cx, cy)) = cursor_logical_position(window) {
        place_float_window_at(window, w, h, cx, cy);
        return;
    }
    // 取不到光标（极少见）：退回窗口所在屏的中心，至少不让窗口留在旧位置。
    let Some(monitor) = monitor_at_cursor(window) else {
        return;
    };
    let scale = monitor.scale_factor().max(1.0);
    let work = monitor.work_area();
    let cx = work.position.x as f64 / scale + work.size.width as f64 / scale / 2.0;
    let cy = work.position.y as f64 / scale + work.size.height as f64 / scale / 2.0;
    place_float_window_at(window, w, h, cx, cy);
}

/// 以屏幕逻辑坐标 `(cx, cy)` 为中心放置浮动窗口（Option+拖动时钉在起手点）。
///
/// 返回窗口最终的左上角（已做工作区 clamp），供指针坐标换算。
pub fn place_float_window_at(
    window: &WebviewWindow,
    w: f64,
    h: f64,
    cx: f64,
    cy: f64,
) -> Option<(f64, f64)> {
    // 按**落点**选屏而不是按窗口原位置：环形盘可能上一秒还在另一块屏上。
    let monitor = monitor_containing(window, (cx, cy))
        .or_else(|| window.current_monitor().ok().flatten())
        .or_else(|| window.primary_monitor().ok().flatten())?;
    let scale = monitor.scale_factor().max(1.0);
    let work = monitor.work_area();
    let origin_x = work.position.x as f64 / scale;
    let origin_y = work.position.y as f64 / scale;
    let avail_w = work.size.width as f64 / scale;
    let avail_h = work.size.height as f64 / scale;

    // 以给定点为中心展开；越界时向工作区内 clamp（等价于边缘翻转/平移）。
    let raw_x = cx - w / 2.0;
    let raw_y = cy - h / 2.0;
    let x = raw_x.clamp(origin_x + FLOAT_MARGIN, (origin_x + avail_w - w - FLOAT_MARGIN).max(origin_x + FLOAT_MARGIN));
    let y = raw_y.clamp(origin_y + FLOAT_MARGIN, (origin_y + avail_h - h - FLOAT_MARGIN).max(origin_y + FLOAT_MARGIN));

    let _ = window.set_size(Size::Logical(LogicalSize::new(w, h)));
    let _ = window.set_position(Position::Logical(LogicalPosition::new(x, y)));
    Some((x, y))
}

/// 把焦点归还「唤出浮动面板前」的前台 App（环形盘执行/取消后回焦 Finder，任务 25）。
///
/// 与 `paste::restore_frontmost_app` 同源，但这里是通用版本：浮动面板（环形盘）
/// 结束时要还给 Finder，而不是只有回贴路径才归还。
#[cfg(target_os = "macos")]
pub fn restore_frontmost_app() {
    use objc2::msg_send;
    use objc2::runtime::{AnyObject, Bool};

    let pid = pre_active_app_pid();
    if pid <= 0 || pid == std::process::id() as i32 {
        return;
    }
    unsafe {
        // `runningApplicationWithProcessIdentifier:` 是 NSRunningApplication 的
        // **类方法**（`+[NSRunningApplication ...]`），不是 NSWorkspace 的实例方法：
        // 发错接收者会撞 objc2 的选择子校验（panic "method not found"），而 release
        // 构建是 panic=abort，整个进程会直接 abort（真机上表现为「转完就消失」）。
        let app: *mut AnyObject = msg_send![
            objc2::class!(NSRunningApplication),
            runningApplicationWithProcessIdentifier: pid
        ];
        if app.is_null() {
            return;
        }
        // NSApplicationActivateAllWindows(1)。不再叠加 IgnoringOtherApps(2)：
        // macOS 14 起该位已废弃且被系统忽略，传了只会误导后来者。
        let options: usize = 1;
        let activated: Bool = msg_send![app, activateWithOptions: options];
        if !activated.as_bool() {
            tracing::debug!(pid, "归还焦点失败：系统拒绝了本次 activate（目标 App 可能已退出）");
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub fn restore_frontmost_app() {}

/// 按配置应用面板高度百分比（`board_height_scale`）与出现位置（`board_position`）。
///
/// 坐标一律以**当前显示器**为基准（副屏原点不为 0，不减原点会把面板推到屏外）：
/// 贴底停靠用完整屏幕矩形（要压住程序坞），其余两种用工作区矩形 + [`panel_width`]。
fn apply_panel_layout(window: &WebviewWindow, config: &Config) {
    // 按光标选屏：热键是在用户正在看的那块屏上按下的，隐藏中的面板仍停在旧屏，
    // 用 `current_monitor()` 会把它唤到另一台显示器上。
    let Some(monitor) = monitor_at_cursor(window) else {
        return;
    };
    let scale = monitor.scale_factor().max(1.0);
    let work = monitor.work_area();
    let origin_x = work.position.x as f64 / scale;
    let origin_y = work.position.y as f64 / scale;
    let avail_width = work.size.width as f64 / scale;
    let avail_height = work.size.height as f64 / scale;
    // 屏幕物理范围（含程序坞占用区）。贴底停靠按它算，保持面板原地不动：
    // 程序坞盖住面板靠窗口层级压过去，而不是把面板往上挪。
    let screen_pos = monitor.position();
    let screen_size = monitor.size();
    let screen_x = screen_pos.x as f64 / scale;
    let screen_y = screen_pos.y as f64 / scale;
    let screen_width = screen_size.width as f64 / scale;
    let screen_height = screen_size.height as f64 / scale;

    // 面板高度 = 屏幕可用高度的 board_height_scale%，上下限保证可用性。
    // 下限 220：卡片固定三段（28 头部 + 预览 + 30 底部）+ 工具栏 44 + 标签行 34
    // + 状态栏 36 已接近 200，160 会把预览压到几乎不可见。
    let max_height = (avail_height - PANEL_MARGIN * 2.0).max(220.0);
    let height = (avail_height * config.board_height_scale.clamp(10, 60) as f64 / 100.0)
        .round()
        .max(220.0)
        .min(max_height);
    let width = panel_width(avail_width);

    let centered_x = origin_x + (avail_width - width) / 2.0;
    let centered_y = origin_y + (avail_height - height) / 2.0;

    let (x, y, w) = match config.board_position.as_str() {
        // 贴底停靠：横向铺满屏幕，下边缘贴屏幕物理底边（原位置，不要挪）。
        // 程序坞露在桌面上时确实会压到这里，但那由窗口层级解决：面板在
        // reveal 时被抬到 NSStatusWindowLevel(25)，高于程序坞的 20。
        "bottom" => (screen_x, screen_y + screen_height - height, screen_width),
        // 屏幕居中。
        "screen" => (centered_x, centered_y, width),
        _ => {
            // follow_cursor：左边缘对齐光标、下方 8pt；越界则回收到工作区内。
            match cursor_logical_position(window) {
                Some((cx, cy)) => (
                    cx.clamp(origin_x, origin_x + (avail_width - width).max(0.0)),
                    (cy + 8.0).clamp(origin_y, origin_y + (avail_height - height).max(0.0)),
                    width,
                ),
                None => (centered_x, centered_y, width),
            }
        }
    };

    let _ = window.set_size(Size::Logical(LogicalSize::new(w, height)));
    let _ = window.set_position(Position::Logical(LogicalPosition::new(x, y)));
}

/// 面板宽度（居中类布局用）：工作区的 60%，收敛到 [`PANEL_MAX_WIDTH`]，
/// 且始终留出 [`PANEL_MARGIN`] 边距。贴底停靠不用这个宽度，直接占满整屏。
fn panel_width(avail_width: f64) -> f64 {
    let usable = (avail_width - PANEL_MARGIN * 2.0).max(240.0);
    (avail_width * 0.6)
        .clamp(240.0, PANEL_MAX_WIDTH)
        .min(usable)
}
