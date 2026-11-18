//! 剪贴板面板：定位、层级、焦点与毛玻璃宿主。

use std::collections::HashMap;
use std::sync::OnceLock;

use super::*;
use super::float::apply_panel_layout;


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
    // 否则取到的是 Halo 自己，⌘V 会贴进面板而不是用户正在用的 App。
    //
    // 走 `remember_frontmost_app` 而不是无条件写入：前台可能就是 Halo
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
    let _ = app.emit("halo://panel-shown", ());
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

/// 面板的「Space 宿主」窗口（仅 macOS），按窗口 label 各存一个。
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
static PANEL_HOSTS: OnceLock<Mutex<HashMap<String, usize>>> = OnceLock::new();

#[cfg(target_os = "macos")]
fn panel_hosts() -> &'static Mutex<HashMap<String, usize>> {
    PANEL_HOSTS.get_or_init(|| Mutex::new(HashMap::new()))
}

#[cfg(target_os = "macos")]
fn ns_window_ptr(window: &WebviewWindow) -> Option<*mut objc2::runtime::AnyObject> {
    let ptr = window.ns_window().ok()? as *mut objc2::runtime::AnyObject;
    (!ptr.is_null()).then_some(ptr)
}

/// 取某个窗口已建好的宿主指针；没建过返回 null。
#[cfg(target_os = "macos")]
fn panel_host(label: &str) -> *mut objc2::runtime::AnyObject {
    let hosts = panel_hosts().lock_ok();
    (*hosts.get(label).unwrap_or(&0)) as *mut objc2::runtime::AnyObject
}

/// 创建面板宿主（每个 label 一个）并把面板挂成它的子窗口。只在 setup 里调用。
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
    let label = window.label().to_string();
    if panel_host(&label) != std::ptr::null_mut() {
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
        *panel_hosts().lock_ok().entry(label).or_insert(0) = host as usize;
    }
}

#[cfg(not(target_os = "macos"))]
pub fn prepare_panel_host(_window: &WebviewWindow) {}

/// 显示前把面板重新挂回宿主、对齐位置与尺寸，并把宿主排进当前 Space。
#[cfg(target_os = "macos")]
pub fn reveal_panel_host(window: &WebviewWindow) {
    use objc2::msg_send;
    use objc2_foundation::CGRect;

    let Some(panel) = ns_window_ptr(window) else {
        return;
    };
    let host = panel_host(window.label());
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
pub fn conceal_panel_host(label: &str) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;

    let host = panel_host(label);
    if host.is_null() {
        return;
    }
    unsafe {
        let _: () = msg_send![host, orderOut: std::ptr::null_mut::<AnyObject>()];
    }
}

#[cfg(not(target_os = "macos"))]
pub fn conceal_panel_host(_label: &str) {}

/// 不管 App 是否在前台，都把这个窗口排到当前 Space 的最前面（仅 macOS）。
///
/// `NSWindow.show()` 对应的 `makeKeyAndOrderFront:` 在 App 非前台时会等激活，
/// 而激活本身又会被别的 App 的全屏 Space 挡住——两头堵，窗口就留在桌面 Space。
#[cfg(target_os = "macos")]
pub fn order_front_regardless(window: &WebviewWindow) {
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
pub(super) fn refresh_shadow(window: &WebviewWindow) {
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
pub(super) fn refresh_shadow(_window: &WebviewWindow) {}

/// 隐藏剪贴板面板。
pub fn hide_panel_window(app: &AppHandle) {
    if let Some(panel) = app.get_webview_window("panel") {
        PANEL_FOCUSED.store(false, Ordering::SeqCst);
        conceal_panel_host("panel");
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
/* 菜单栏图标 / 下拉面板（traypanel）                                  */
/* ------------------------------------------------------------------ */
