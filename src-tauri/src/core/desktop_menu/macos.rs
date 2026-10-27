//! macOS 实现：CGEventTap 识别桌面空白处右键，弹出原生 NSMenu。
//!
//! 事件监听放在独立线程（与 [`crate::core::option_tap`] 同一套做法），菜单
//! 必须回主线程构建和弹出——AppKit 不是线程安全的，而且 `popUp` 会进入
//! 嵌套事件循环，绝不能阻塞事件 tap 的回调（否则菜单点击事件永远送不进来）。

use std::ffi::{c_void, CStr, CString};
use std::os::raw::c_char;
use std::path::PathBuf;
use std::process::Command;
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use objc2::encode::{Encode, Encoding};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject};
use objc2::{class, declare_class, msg_send, msg_send_id, mutability, sel, ClassType, DeclaredClass};
use tauri::AppHandle;

use super::{create_folder, paste_into, DesktopAction, MenuContext};
use crate::core::services;
use crate::lock_ext::LockExt;

const RIGHT_DOWN: u32 = 3;
const RIGHT_UP: u32 = 4;
const MASK: u64 = (1 << RIGHT_DOWN) | (1 << RIGHT_UP);
const TAP_DISABLED_BY_TIMEOUT: u32 = 0xFFFF_FFFE;
const TAP_DISABLED_BY_USER_INPUT: u32 = 0xFFFF_FFFF;

/// `kCGEventSourceUserData`：给「重新投递给 Finder」的合成事件打标记。
const SOURCE_USER_DATA_FIELD: u32 = 42;
/// 标记值随便取，只要不是硬件事件会带的值；"CLIPBASE" 的 ASCII。
const REPOST_MAGIC: i64 = 0x434C_4950_4241_5345;

const CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;

type AXUIElementRef = *mut c_void;
type CFStringRef = *const c_void;
type CFTypeRef = *const c_void;

#[repr(C)]
#[derive(Clone, Copy)]
struct CGPoint {
    x: f64,
    y: f64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CGSize {
    width: f64,
    height: f64,
}

// 与 objc2-foundation 的 NSPoint 同一布局：`{CGPoint=dd}`。
// 不直接依赖 objc2-foundation，避免再引一个版本号。
unsafe impl Encode for CGPoint {
    const ENCODING: Encoding = Encoding::Struct("CGPoint", &[f64::ENCODING, f64::ENCODING]);
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventTapCreate(
        tap: u32,
        place: u32,
        options: u32,
        events_of_interest: u64,
        callback: TapCallback,
        user_info: *mut c_void,
    ) -> *mut c_void;
    fn CGEventTapEnable(tap: *mut c_void, enable: bool);
    fn CGEventGetLocation(event: *mut c_void) -> CGPoint;
    fn CGEventGetIntegerValueField(event: *mut c_void, field: u32) -> i64;
    fn CGEventSetIntegerValueField(event: *mut c_void, field: u32, value: i64);
    fn CGEventSourceCreate(state: u32) -> *mut c_void;
    fn CGEventCreateMouseEvent(
        source: *mut c_void,
        mouse_type: u32,
        point: CGPoint,
        button: u32,
    ) -> *mut c_void;
    fn CGEventPost(tap: u32, event: *mut c_void);
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFMachPortCreateRunLoopSource(
        allocator: *const c_void,
        port: *mut c_void,
        order: isize,
    ) -> *mut c_void;
    fn CFRunLoopGetCurrent() -> *mut c_void;
    fn CFRunLoopAddSource(runloop: *mut c_void, source: *mut c_void, mode: *const c_void);
    fn CFRunLoopRun();
    static kCFRunLoopCommonModes: *const c_void;
    fn CFRelease(cf: *const c_void);
    fn CFRetain(cf: *const c_void) -> *const c_void;
    fn CFStringCreateWithCString(
        allocator: *const c_void,
        c_str: *const c_char,
        encoding: u32,
    ) -> CFStringRef;
    fn CFStringGetCString(
        string: CFStringRef,
        buffer: *mut c_char,
        size: isize,
        encoding: u32,
    ) -> u8;
    fn CFArrayGetCount(arr: *const c_void) -> isize;
    fn CFArrayGetValueAtIndex(arr: *const c_void, index: isize) -> *const c_void;
    fn CFURLGetFileSystemRepresentation(
        url: *const c_void,
        resolve: bool,
        buffer: *mut u8,
        size: isize,
    ) -> bool;
    fn CFGetTypeID(value: *const c_void) -> u64;
    fn CFURLGetTypeID() -> u64;
}

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXUIElementCreateSystemWide() -> AXUIElementRef;
    fn AXUIElementCopyElementAtPosition(
        application: AXUIElementRef,
        x: f32,
        y: f32,
        element: *mut AXUIElementRef,
    ) -> i32;
    fn AXUIElementCopyAttributeValue(
        element: *const c_void,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> i32;
    fn AXUIElementGetPid(element: AXUIElementRef, pid: *mut i32) -> i32;
    fn AXUIElementSetMessagingTimeout(element: *const c_void, seconds: f32) -> i32;
    fn AXValueGetValue(value: *const c_void, the_type: u32, value_out: *mut c_void) -> bool;
}

/// `kAXValueCGPointType` / `kAXValueCGSizeType`。
const AX_VALUE_CGPOINT: u32 = 1;
const AX_VALUE_CGSIZE: u32 = 2;

type TapCallback = extern "C" fn(*mut c_void, u32, *mut c_void, *mut c_void) -> *mut c_void;

/// 用户是否开启「Finder 右键菜单」（与扩展开关同一个状态）。
static ENABLED: AtomicBool = AtomicBool::new(false);
/// 监听线程是否已安装（只装一次）。
static INSTALLED: AtomicBool = AtomicBool::new(false);
static APP: OnceLock<AppHandle> = OnceLock::new();
static TAP: AtomicPtr<c_void> = AtomicPtr::new(ptr::null_mut());

/// 菜单正在屏上：期间不接管新的右键，避免嵌套弹出。
static MENU_OPEN: AtomicBool = AtomicBool::new(false);
/// 这次右键已经被我们接管，配对的 rightMouseUp 也要吞掉。
static SWALLOW_UP: AtomicBool = AtomicBool::new(false);
/// 用户点了「显示 Finder 菜单」：菜单关闭后把右键原样交给 Finder。
static WANT_NATIVE_MENU: AtomicBool = AtomicBool::new(false);
/// 本次右键的屏幕坐标（CG 坐标，左上原点）。
static ORIGIN: Mutex<Option<(f64, f64)>> = Mutex::new(None);
/// 本次右键命中的上下文；菜单回调（`handle_tag`）据此取目标路径。
static MENU_CONTEXT: Mutex<Option<MenuContext>> = Mutex::new(None);
/// 菜单回调的 target 对象（进程级单例，故意不释放）。
static MENU_TARGET: AtomicPtr<c_void> = AtomicPtr::new(ptr::null_mut());

/// 启动时初始化：只记住 AppHandle。
///
/// 「启不启」不在这里决定：启动瞬间 `pluginkit` 可能还没把扩展条目落库，
/// 那样会误判成「未启用」而永不接管。是否安装监听统一由
/// [`set_enabled`] 在扩展状态核实后决定。
pub fn init(app: &AppHandle) {
    let _ = APP.set(app.clone());
}

/// 跟随即 Finder 扩展开关一起启停。
pub fn set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::SeqCst);
    if enabled {
        install();
    }
}

fn install() {
    if INSTALLED.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::Builder::new()
        .name("desktop-menu-tap".into())
        .spawn(|| unsafe { run_tap() })
        .ok();
}

/// 安装监听；失败（登录初期 TCC 未就绪等）就隔几秒重试，直到成功或用户停用。
unsafe fn run_tap() {
    let mut warned = false;
    loop {
        if !ENABLED.load(Ordering::SeqCst) {
            INSTALLED.store(false, Ordering::SeqCst);
            return;
        }

        if !crate::core::paste::has_accessibility_permission() {
            if !warned {
                tracing::warn!("桌面右键菜单需要「辅助功能」权限；授权后会自动接管");
                warned = true;
            }
        } else {
            let tap = CGEventTapCreate(1, 0, 0, MASK, callback, ptr::null_mut());
            if !tap.is_null() {
                TAP.store(tap, Ordering::SeqCst);
                let source = CFMachPortCreateRunLoopSource(ptr::null(), tap, 0);
                if !source.is_null() {
                    CFRunLoopAddSource(CFRunLoopGetCurrent(), source, kCFRunLoopCommonModes);
                    CGEventTapEnable(tap, true);
                    tracing::info!("桌面右键菜单已启用（空白处 + 桌面图标）");
                    CFRunLoopRun();
                } else {
                    tracing::warn!("创建桌面右键监听 run loop source 失败");
                }
            } else if !warned {
                tracing::warn!("创建桌面右键监听失败（辅助功能权限不足？），稍后重试");
                warned = true;
            }
        }

        std::thread::sleep(Duration::from_secs(3));
    }
}

extern "C" fn callback(
    _proxy: *mut c_void,
    event_type: u32,
    event: *mut c_void,
    _user_info: *mut c_void,
) -> *mut c_void {
    match event_type {
        TAP_DISABLED_BY_TIMEOUT | TAP_DISABLED_BY_USER_INPUT => unsafe {
            let tap = TAP.load(Ordering::SeqCst);
            if !tap.is_null() {
                CGEventTapEnable(tap, true);
            }
            event
        },
        RIGHT_DOWN => unsafe {
            if on_right_down(event) {
                ptr::null_mut()
            } else {
                event
            }
        },
        RIGHT_UP => {
            if SWALLOW_UP.swap(false, Ordering::SeqCst) {
                ptr::null_mut()
            } else {
                event
            }
        }
        _ => event,
    }
}

/// 决定是否接管本次右键；返回 `true` 表示事件不交给 Finder。
unsafe fn on_right_down(event: *mut c_void) -> bool {
    // 「显示 Finder 菜单」投递的合成事件不能再被自己接管。
    if CGEventGetIntegerValueField(event, SOURCE_USER_DATA_FIELD) == REPOST_MAGIC {
        return false;
    }
    if !ENABLED.load(Ordering::SeqCst) || MENU_OPEN.load(Ordering::SeqCst) {
        return false;
    }

    let point = CGEventGetLocation(event);
    let Some(context) = classify(point) else {
        return false;
    };

    let Some(app) = APP.get().cloned() else {
        return false;
    };

    *ORIGIN.lock_ok() = Some((point.x, point.y));
    *MENU_CONTEXT.lock_ok() = Some(context.clone());
    MENU_OPEN.store(true, Ordering::SeqCst);
    SWALLOW_UP.store(true, Ordering::SeqCst);

    // 菜单在主线程弹出；这里只排队，绝不能等它关闭（否则 tap 回调超时，
    // 菜单自己的点击事件也进不来）。
    if app
        .run_on_main_thread(move || {
            unsafe { show_menu() };
            MENU_OPEN.store(false, Ordering::SeqCst);
        })
        .is_err()
    {
        *MENU_CONTEXT.lock_ok() = None;
        MENU_OPEN.store(false, Ordering::SeqCst);
        SWALLOW_UP.store(false, Ordering::SeqCst);
        return false;
    }

    tracing::info!(x = point.x, y = point.y, ?context, "桌面右键：接管并弹出 ClipBase 菜单");
    true
}

/// 判断这次右键落在桌面空白处还是桌面图标上。
///
/// AX 命中元素必须是 Finder 自己的；只有当它的祖先链在遇到 `AXWindow`
/// 之前先遇到 `AXSubrole == AXDesktop` 时才算桌面，Finder 窗口内的内容
/// 一律放行（窗口菜单由 Finder Sync 扩展提供）。
unsafe fn classify(point: CGPoint) -> Option<MenuContext> {
    let deadline = Instant::now() + Duration::from_millis(400);

    let system = AXUIElementCreateSystemWide();
    if system.is_null() {
        return None;
    }
    // Finder 忙（例如正在拖动文件）时不要让 tap 回调卡死。
    AXUIElementSetMessagingTimeout(system, 0.15);

    let mut element: AXUIElementRef = ptr::null_mut();
    let status =
        AXUIElementCopyElementAtPosition(system, point.x as f32, point.y as f32, &mut element);
    if status != 0 || element.is_null() {
        if !element.is_null() {
            CFRelease(element as CFTypeRef);
        }
        CFRelease(system as CFTypeRef);
        return None;
    }

    let mut pid: i32 = 0;
    AXUIElementGetPid(element, &mut pid);
    if pid <= 0 || !is_finder(pid) {
        CFRelease(element as CFTypeRef);
        CFRelease(system as CFTypeRef);
        return None;
    }

    // 向上找桌面容器，同时记录命中元素自己的 URL（图标会把 URL 挂在
    // AXImage 上，标签/子元素则需要靠下面的矩形命中回退）。
    let mut current: *const c_void = element;
    let mut owned: Option<*const c_void> = None;
    let mut url: Option<PathBuf> = None;
    let mut desktop: *const c_void = ptr::null();

    let mut depth = 0usize;
    while depth <= 12 {
        if Instant::now() >= deadline {
            break;
        }
        let role = ax_string(current, "AXRole");
        let subrole = ax_string(current, "AXSubrole");
        if url.is_none() {
            url = ax_url(current);
        }
        if subrole.as_deref() == Some("AXDesktop") {
            CFRetain(current);
            desktop = current;
            break;
        }
        if role.as_deref() == Some("AXWindow") {
            break;
        }

        let Some(parent) = copy_ax_attr(current, "AXParent") else {
            break;
        };
        if let Some(old) = owned.replace(parent) {
            CFRelease(old);
        }
        AXUIElementSetMessagingTimeout(parent, 0.15);
        current = parent;
        depth += 1;
    }

    if let Some(old) = owned {
        CFRelease(old);
    }
    CFRelease(element as CFTypeRef);
    CFRelease(system as CFTypeRef);

    if desktop.is_null() {
        return None;
    }
    // 命中元素通常直接是图标容器（AXGroup/AXDesktop）；个别系统/视图模式
    // 会先命中外层滚动区域，这里再往下取一层带 AXDesktop 子角色的组。
    let desktop = icon_container(desktop);

    let context = match url {
        Some(path) => MenuContext::Items(selection_for(desktop, path)),
        None => match desktop_child_at(desktop, point) {
            ChildHit::Icon(path) => MenuContext::Items(selection_for(desktop, path)),
            ChildHit::Widget => {
                CFRelease(desktop);
                return None;
            }
            // 没有 URL 也没命中图标/组件：点在桌面容器或图标标签的空白处，
            // 仍按空白处处理，至少能拿到 ClipBase 菜单。
            ChildHit::None => MenuContext::Blank,
        },
    };

    CFRelease(desktop);
    Some(context)
}

/// 返回桌面图标容器（`AXGroup/AXDesktop`）。入参若是外层容器，会在释放它
/// 之后返回保留下来的内层组。
unsafe fn icon_container(area: *const c_void) -> *const c_void {
    if ax_string(area, "AXRole").as_deref() == Some("AXGroup") {
        return area;
    }

    let Some(children) = copy_ax_attr(area, "AXChildren") else {
        return area;
    };
    let count = CFArrayGetCount(children);
    let mut group: *const c_void = ptr::null();
    for index in 0..count.max(0) {
        let child = CFArrayGetValueAtIndex(children, index);
        if child.is_null() {
            continue;
        }
        if ax_string(child, "AXSubrole").as_deref() == Some("AXDesktop") {
            CFRetain(child);
            group = child;
            break;
        }
    }
    CFRelease(children);

    if group.is_null() {
        area
    } else {
        CFRelease(area);
        group
    }
}

enum ChildHit {
    Icon(PathBuf),
    Widget,
    None,
}

/// 桌面直接子项里，哪一个覆盖了 `point`（图标标签在图标下方，往下放宽）。
///
/// 没有 `AXURL` 的直接子项是桌面小组件（天气/日历等），不接管它们的右键。
unsafe fn desktop_child_at(desktop: *const c_void, point: CGPoint) -> ChildHit {
    let Some(children) = copy_ax_attr(desktop, "AXChildren") else {
        return ChildHit::None;
    };

    let count = CFArrayGetCount(children);
    let mut hit = ChildHit::None;
    for index in 0..count.max(0) {
        let child = CFArrayGetValueAtIndex(children, index);
        if child.is_null() {
            continue;
        }
        let Some((x, y, width, height)) = ax_frame(child) else {
            continue;
        };
        if width <= 0.0 || height <= 0.0 {
            continue;
        }
        if point.x < x - 4.0 || point.x > x + width + 4.0 {
            continue;
        }
        if point.y < y || point.y > y + height + 28.0 {
            continue;
        }
        hit = match ax_url(child) {
            Some(path) => ChildHit::Icon(path),
            None => ChildHit::Widget,
        };
        break;
    }

    CFRelease(children);
    hit
}

/// 右键图标时，如果 Finder 当前选中项包含它，就把整批选中项一起作为上下文。
///
/// 右键未选中的图标时 Finder 还没来得及改选中（tap 早于 Finder 收到事件），
/// 所以只在「点中的图标确实在选中集合里」时才扩展。
unsafe fn selection_for(desktop: *const c_void, clicked: PathBuf) -> Vec<PathBuf> {
    let selected = desktop_selection(desktop);
    if selected.len() > 1 && selected.iter().any(|path| path == &clicked) {
        selected
    } else {
        vec![clicked]
    }
}

unsafe fn desktop_selection(desktop: *const c_void) -> Vec<PathBuf> {
    for attribute in ["AXSelectedChildren", "AXSelectedRows"] {
        let Some(value) = copy_ax_attr(desktop, attribute) else {
            continue;
        };
        let count = CFArrayGetCount(value);
        let mut paths = Vec::new();
        for index in 0..count.max(0) {
            let child = CFArrayGetValueAtIndex(value, index);
            if child.is_null() {
                continue;
            }
            if let Some(path) = ax_url(child) {
                paths.push(path);
            }
        }
        CFRelease(value);
        if !paths.is_empty() {
            return paths;
        }
    }
    Vec::new()
}

unsafe fn copy_ax_attr(element: *const c_void, name: &str) -> Option<*const c_void> {
    let name = CString::new(name).ok()?;
    let attribute = CFStringCreateWithCString(ptr::null(), name.as_ptr(), CF_STRING_ENCODING_UTF8);
    if attribute.is_null() {
        return None;
    }

    let mut value: CFTypeRef = ptr::null();
    let status = AXUIElementCopyAttributeValue(element, attribute, &mut value);
    CFRelease(attribute);
    (status == 0 && !value.is_null()).then_some(value)
}

unsafe fn ax_string(element: *const c_void, name: &str) -> Option<String> {
    let value = copy_ax_attr(element, name)?;

    let mut buffer = [0 as c_char; 64];
    let ok = CFStringGetCString(
        value,
        buffer.as_mut_ptr(),
        buffer.len() as isize,
        CF_STRING_ENCODING_UTF8,
    ) != 0;
    CFRelease(value);
    if !ok {
        return None;
    }

    CStr::from_ptr(buffer.as_ptr()).to_str().ok().map(str::to_owned)
}

unsafe fn ax_url(element: *const c_void) -> Option<PathBuf> {
    let value = copy_ax_attr(element, "AXURL")?;
    let path = if CFGetTypeID(value) == CFURLGetTypeID() {
        let mut buffer = [0u8; 4096];
        let ok = CFURLGetFileSystemRepresentation(
            value,
            true,
            buffer.as_mut_ptr(),
            buffer.len() as isize,
        );
        if ok {
            CStr::from_ptr(buffer.as_ptr() as *const c_char)
                .to_str()
                .ok()
                .map(PathBuf::from)
                .filter(|path| !path.as_os_str().is_empty())
        } else {
            None
        }
    } else {
        None
    };
    CFRelease(value);
    path
}

/// 读 AX 元素的 `(x, y, width, height)`（左上原点逻辑坐标）。
unsafe fn ax_frame(element: *const c_void) -> Option<(f64, f64, f64, f64)> {
    let position = copy_ax_attr(element, "AXPosition")?;
    let size = copy_ax_attr(element, "AXSize")?;

    let mut point = CGPoint { x: 0.0, y: 0.0 };
    let mut size_value = CGSize {
        width: 0.0,
        height: 0.0,
    };
    let has_point = AXValueGetValue(
        position,
        AX_VALUE_CGPOINT,
        &mut point as *mut CGPoint as *mut c_void,
    );
    let has_size = AXValueGetValue(
        size,
        AX_VALUE_CGSIZE,
        &mut size_value as *mut CGSize as *mut c_void,
    );

    CFRelease(position);
    CFRelease(size);
    (has_point && has_size).then_some((
        point.x,
        point.y,
        size_value.width,
        size_value.height,
    ))
}

unsafe fn is_finder(pid: i32) -> bool {
    let app: *mut AnyObject = msg_send![
        class!(NSRunningApplication),
        runningApplicationWithProcessIdentifier: pid
    ];
    if app.is_null() {
        return false;
    }
    let bundle: *mut AnyObject = msg_send![app, bundleIdentifier];
    if bundle.is_null() {
        return false;
    }
    let ptr: *const c_char = msg_send![bundle, UTF8String];
    if ptr.is_null() {
        return false;
    }
    CStr::from_ptr(ptr)
        .to_str()
        .map(|id| id == "com.apple.finder")
        .unwrap_or(false)
}

/* ------------------------------------------------------------------ */
/* NSMenu                                                              */
/* ------------------------------------------------------------------ */

declare_class!(
    struct MenuTarget;

    unsafe impl ClassType for MenuTarget {
        type Super = NSObject;
        type Mutability = mutability::InteriorMutable;
        const NAME: &'static str = "ClipBaseDesktopMenuTarget";
    }

    impl DeclaredClass for MenuTarget {}

    unsafe impl MenuTarget {
        #[method(handleMenuItem:)]
        fn handle_menu_item(&self, sender: *mut AnyObject) {
            let tag: isize = unsafe { msg_send![sender, tag] };
            handle_tag(tag);
        }
    }
);

fn menu_target() -> *mut AnyObject {
    let existing = MENU_TARGET.load(Ordering::SeqCst);
    if !existing.is_null() {
        return existing.cast();
    }
    unsafe {
        let target: Retained<MenuTarget> = msg_send_id![MenuTarget::alloc(), init];
        let raw = Retained::into_raw(target);
        MENU_TARGET.store(raw.cast(), Ordering::SeqCst);
        raw.cast()
    }
}

/// 主线程：构建并弹出菜单，直到用户选择或点空白处关闭。
unsafe fn show_menu() {
    let pool: *mut AnyObject = msg_send![class!(NSAutoreleasePool), new];
    let menu: *mut AnyObject = msg_send![class!(NSMenu), new];
    let _: () = msg_send![menu, setAutoenablesItems: false];

    let context = MENU_CONTEXT.lock_ok().clone().unwrap_or(MenuContext::Blank);
    let target = menu_target();
    let mut paste_available = None;

    for action in context.actions() {
        let enabled = match action {
            DesktopAction::Paste => *paste_available.get_or_insert_with(|| pasteboard_has_files()),
            _ => true,
        };

        if action.separator_before() {
            let count: isize = msg_send![menu, numberOfItems];
            if count > 0 {
                let separator: *mut AnyObject = msg_send![class!(NSMenuItem), separatorItem];
                let _: () = msg_send![menu, addItem: separator];
            }
        }
        add_item(menu, action, enabled, target);
    }

    // 用当前鼠标位置（NSEvent 是左下原点），与右键位置一致。
    let location: CGPoint = msg_send![class!(NSEvent), mouseLocation];
    let _: bool = msg_send![
        menu,
        popUpMenuPositioningItem: ptr::null_mut::<AnyObject>(),
        atLocation: location,
        inView: ptr::null_mut::<AnyObject>()
    ];

    let _: () = msg_send![menu, release];
    let _: () = msg_send![pool, drain];
    SWALLOW_UP.store(false, Ordering::SeqCst);

    // 上下文故意保留到下一次右键：个别系统版本会在 `popUp` 返回后的同一轮
    // runloop 里才派发菜单动作，提前清空会让「打开 / 简介 / 废纸篓」丢掉目标。

    if WANT_NATIVE_MENU.swap(false, Ordering::SeqCst) {
        // 等菜单的 tracking loop 完全退出，再把右键交给 Finder。
        std::thread::spawn(|| {
            std::thread::sleep(Duration::from_millis(150));
            unsafe { repost_right_click() };
        });
    }
}

unsafe fn add_item(menu: *mut AnyObject, action: DesktopAction, enabled: bool, target: *mut AnyObject) {
    let alloc: *mut AnyObject = msg_send![class!(NSMenuItem), alloc];
    let item: *mut AnyObject = msg_send![
        alloc,
        initWithTitle: ns_string(action.title()),
        action: sel!(handleMenuItem:),
        keyEquivalent: ns_string("")
    ];
    let _: () = msg_send![item, setTarget: target];
    let _: () = msg_send![item, setTag: action.tag()];
    let _: () = msg_send![item, setEnabled: enabled];

    let image: *mut AnyObject = msg_send![
        class!(NSImage),
        imageWithSystemSymbolName: ns_string(action.symbol()),
        accessibilityDescription: ptr::null_mut::<AnyObject>()
    ];
    if !image.is_null() {
        let _: () = msg_send![item, setImage: image];
    }

    let _: () = msg_send![menu, addItem: item];
    // addItem 会 retain，这里释放我们 alloc 出来的 +1。
    let _: () = msg_send![item, release];
}

unsafe fn ns_string(value: &str) -> *mut AnyObject {
    // 这里在主线程的 ObjC 回调里，panic 会连带把菜单构建整条链路带走。
    // 调用方传的都是常量，但内部 NUL 会让 CString::new 失败，因此先剔除再降级为空串。
    let sanitized = value.replace('\0', "");
    let c_string = CString::new(sanitized).unwrap_or_default();
    msg_send![class!(NSString), stringWithUTF8String: c_string.as_ptr()]
}

/// 剪贴板里有没有可粘贴的文件；没有就禁用「粘贴项目」。
unsafe fn pasteboard_has_files() -> bool {
    let pasteboard: *mut AnyObject = msg_send![class!(NSPasteboard), generalPasteboard];
    if pasteboard.is_null() {
        return false;
    }
    let classes: *mut AnyObject = msg_send![class!(NSArray), arrayWithObject: class!(NSURL)];
    let urls: *mut AnyObject = msg_send![
        pasteboard,
        readObjectsForClasses: classes,
        options: ptr::null_mut::<AnyObject>()
    ];
    if urls.is_null() {
        return false;
    }
    let count: usize = msg_send![urls, count];
    count > 0
}

/// 从剪贴板读出文件 URL 的路径（只在主线程调用）。
unsafe fn pasteboard_file_paths() -> Vec<PathBuf> {
    let pasteboard: *mut AnyObject = msg_send![class!(NSPasteboard), generalPasteboard];
    if pasteboard.is_null() {
        return Vec::new();
    }
    let classes: *mut AnyObject = msg_send![class!(NSArray), arrayWithObject: class!(NSURL)];
    let urls: *mut AnyObject = msg_send![
        pasteboard,
        readObjectsForClasses: classes,
        options: ptr::null_mut::<AnyObject>()
    ];
    if urls.is_null() {
        return Vec::new();
    }

    let count: usize = msg_send![urls, count];
    let mut paths = Vec::with_capacity(count);
    for index in 0..count {
        let url: *mut AnyObject = msg_send![urls, objectAtIndex: index];
        if url.is_null() {
            continue;
        }
        let path: *mut AnyObject = msg_send![url, path];
        if path.is_null() {
            continue;
        }
        let ptr: *const c_char = msg_send![path, UTF8String];
        if ptr.is_null() {
            continue;
        }
        if let Ok(text) = CStr::from_ptr(ptr).to_str() {
            if !text.is_empty() {
                paths.push(PathBuf::from(text));
            }
        }
    }
    paths
}

fn handle_tag(tag: isize) {
    let Some(action) = DesktopAction::from_tag(tag) else {
        return;
    };
    let context = MENU_CONTEXT.lock_ok().clone().unwrap_or(MenuContext::Blank);

    match action {
        DesktopAction::ShowFinderMenu => {
            WANT_NATIVE_MENU.store(true, Ordering::SeqCst);
        }
        DesktopAction::Paste => {
            let sources = unsafe { pasteboard_file_paths() };
            if sources.is_empty() {
                return;
            }
            let Some(target) = context.target_directory() else {
                tracing::warn!("找不到目标目录，无法粘贴");
                return;
            };
            std::thread::spawn(move || {
                for result in paste_into(&target, &sources) {
                    match result {
                        Ok(path) => tracing::info!("已粘贴 {}", path.display()),
                        Err(err) => {
                            tracing::warn!("粘贴失败: {err}");
                            services::notify("ClipBase", &format!("粘贴失败：{err}"));
                        }
                    }
                }
            });
        }
        DesktopAction::Wallpaper => {
            std::thread::spawn(open_wallpaper_settings);
        }
        DesktopAction::NewFolder => {
            let Some(target) = context.target_directory() else {
                tracing::warn!("找不到目标目录，无法新建文件夹");
                return;
            };
            std::thread::spawn(move || match create_folder(&target) {
                Ok(path) => tracing::info!("已新建文件夹 {}", path.display()),
                Err(err) => {
                    tracing::warn!("新建文件夹失败: {err}");
                    services::notify("ClipBase", &format!("新建文件夹失败：{err}"));
                }
            });
        }
        DesktopAction::Open => {
            let items = context.items().to_vec();
            if items.is_empty() {
                return;
            }
            std::thread::spawn(move || open_items(&items));
        }
        DesktopAction::ShowInfo => {
            let items = context.items().to_vec();
            if items.is_empty() {
                return;
            }
            std::thread::spawn(move || show_info(&items));
        }
        DesktopAction::MoveToTrash => {
            let items = context.items().to_vec();
            if items.is_empty() {
                return;
            }
            std::thread::spawn(move || move_to_trash(&items));
        }
        _ => {
            let Some(service) = action.service_action() else {
                return;
            };
            let Some(target) = context.target_directory() else {
                tracing::warn!("动作「{}」：没有可用的目标目录", service.menu_title());
                services::notify("ClipBase", "未能识别要操作的文件夹");
                return;
            };
            std::thread::spawn(move || {
                services::run_container_action(service, &target);
            });
        }
    }
}

fn open_items(paths: &[PathBuf]) {
    let existing: Vec<&PathBuf> = paths.iter().filter(|path| path.exists()).collect();
    if existing.is_empty() {
        return;
    }
    match Command::new("open").args(&existing).status() {
        Ok(status) if status.success() => {}
        Ok(status) => tracing::warn!("打开失败，退出码 {:?}", status.code()),
        Err(err) => {
            tracing::warn!("打开失败: {err}");
            services::notify("ClipBase", &format!("打开失败：{err}"));
        }
    }
}

/// Finder 的「显示简介」没有公开 API，只能走 AppleScript。
fn show_info(paths: &[PathBuf]) {
    let mut script = String::from("tell application \"Finder\"\n");
    let mut any = false;
    for path in paths.iter().filter(|path| path.exists()) {
        any = true;
        script.push_str("open information window of (POSIX file ");
        script.push_str(&services::applescript_string_literal(&path.to_string_lossy()));
        script.push_str(" as alias)\n");
    }
    if !any {
        return;
    }
    script.push_str("end tell");

    match Command::new("osascript").arg("-e").arg(script).output() {
        Ok(output) if output.status.success() => {}
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            tracing::warn!("显示简介失败: {stderr}");
            services::notify("ClipBase", &format!("显示简介失败：{stderr}"));
        }
        Err(err) => {
            tracing::warn!("无法执行 osascript: {err}");
            services::notify("ClipBase", &format!("显示简介失败：{err}"));
        }
    }
}

/// 用 `NSFileManager` 的回收站 API（可撤销，和 Finder 拖动到废纸篓等价）。
fn move_to_trash(paths: &[PathBuf]) {
    for path in paths {
        if !path.exists() {
            continue;
        }
        unsafe {
            let pool: *mut AnyObject = msg_send![class!(NSAutoreleasePool), new];
            let url: *mut AnyObject = msg_send![
                class!(NSURL),
                fileURLWithPath: ns_string(&path.to_string_lossy())
            ];
            let manager: *mut AnyObject = msg_send![class!(NSFileManager), defaultManager];

            if url.is_null() || manager.is_null() {
                let _: () = msg_send![pool, drain];
                continue;
            }

            let mut resulting: *mut AnyObject = ptr::null_mut();
            let mut error: *mut AnyObject = ptr::null_mut();
            let ok: bool = msg_send![
                manager,
                trashItemAtURL: url,
                resultingItemURL: &mut resulting as *mut *mut AnyObject,
                error: &mut error as *mut *mut AnyObject
            ];

            if ok {
                // `resultingItemURL` 是 NSURL，路径要再取一次 `path`。
                let moved = if resulting.is_null() {
                    None
                } else {
                    let moved_path: *mut AnyObject = msg_send![resulting, path];
                    ns_utf8(moved_path)
                }
                .map(PathBuf::from)
                .unwrap_or_else(|| path.clone());
                tracing::info!("已移到废纸篓 {}", moved.display());
            } else {
                let reason = ns_error_text(error).unwrap_or_else(|| "未知错误".to_string());
                tracing::warn!("移到废纸篓失败 {}: {reason}", path.display());
                services::notify(
                    "ClipBase",
                    &format!("移到废纸篓失败：{}（{reason}）", path.display()),
                );
            }

            let _: () = msg_send![pool, drain];
        }
    }
}

unsafe fn ns_utf8(value: *mut AnyObject) -> Option<String> {
    if value.is_null() {
        return None;
    }
    let ptr: *const c_char = msg_send![value, UTF8String];
    if ptr.is_null() {
        return None;
    }
    CStr::from_ptr(ptr).to_str().ok().map(str::to_owned)
}

unsafe fn ns_error_text(error: *mut AnyObject) -> Option<String> {
    if error.is_null() {
        return None;
    }
    let description: *mut AnyObject = msg_send![error, localizedDescription];
    ns_utf8(description)
}

fn open_wallpaper_settings() {
    const TARGETS: [&str; 2] = [
        "x-apple.systempreferences:com.apple.Wallpaper-Settings.extension",
        "x-apple.systempreferences:com.apple.preference.desktopscreeneffect",
    ];
    for target in TARGETS {
        match Command::new("open").arg(target).status() {
            Ok(status) if status.success() => return,
            Ok(status) => tracing::debug!("打开壁纸设置退出码 {:?}（{target}）", status.code()),
            Err(err) => tracing::warn!("打开壁纸设置失败: {err}"),
        }
    }
    tracing::warn!("无法打开「桌面背景」设置");
}

/// 把一次右键原样重投给 Finder（带标记，ours 会放行）。
unsafe fn repost_right_click() {
    let Some((x, y)) = *ORIGIN.lock_ok() else {
        return;
    };
    let source = CGEventSourceCreate(1);
    if source.is_null() {
        return;
    }
    let point = CGPoint { x, y };

    for (event_type, delay_ms) in [(RIGHT_DOWN, 0u64), (RIGHT_UP, 40)] {
        if delay_ms > 0 {
            std::thread::sleep(Duration::from_millis(delay_ms));
        }
        let event = CGEventCreateMouseEvent(source, event_type, point, 1);
        if event.is_null() {
            continue;
        }
        CGEventSetIntegerValueField(event, SOURCE_USER_DATA_FIELD, REPOST_MAGIC);
        CGEventPost(0, event);
        CFRelease(event as CFTypeRef);
    }

    CFRelease(source as CFTypeRef);
    tracing::info!("桌面右键：已交还给 Finder 原生菜单");
}
