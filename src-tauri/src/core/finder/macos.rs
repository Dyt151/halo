//! macOS 实现：AppleScript 读取 Finder 选中项 + NSWorkspace 取前台 App（仅 macOS 编译）。

use std::ffi::c_void;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use super::{is_permission_error, is_timeout_error, split_paths, FinderSelection, StartHit};

/// osascript 的最长等待：TCC 授权弹窗未决时 Apple 事件会一直挂着，
/// 给足时间让用户可以点「允许」；超时后按 Timeout 上报，不阻塞主线程。
const SELECTION_TIMEOUT: Duration = Duration::from_secs(60);

/// AppleScript：把 Finder 当前标签页的选中项转成 POSIX 路径，用 `0x1F` 连接。
///
/// `as alias` 用于把 Finder 的 specifier 换成可取路径的对象；
/// 个别条目（回收站里的项目等）取不到路径时按条跳过，不影响其余选中项。
const SELECTION_SCRIPT: &str = r#"
tell application "Finder"
    set sel to selection
    set out to {}
    repeat with i from 1 to (count of sel)
        try
            set end of out to POSIX path of ((item i of sel) as alias)
        end try
    end repeat
    set AppleScript's text item delimiters to (character id 31)
    return out as text
end tell
"#;

/// 前台 Finder 的 bundle id。
const FINDER_BUNDLE_ID: &str = "com.apple.finder";

/* ------------------------------------------------------------------ */
/* 快速路径：直接读 Finder 的 AX 树（毫秒级，不启子进程）                 */
/* ------------------------------------------------------------------ */

/// AppleScript 要现启一个 osascript 子进程（实测 1.5s 左右），
/// 对「Option+拖动、松手就要选格式」太慢；AX 直读同一棵树只要几十毫秒。
pub(super) enum FastSelection {
    Items(Vec<String>),
    Empty,
    /// 读不出来（不是 Finder / 没权限 / 树结构不认识）→ 交给 AppleScript 慢路径。
    Unknown,
}

// 注意：`copyElementAtPosition` / `getPid` / `createSystemWide` 三个符号在
// `core::desktop_menu` 里也声明了。同一 crate 内重复声明必须**签名完全一致**，
// 否则 rustc 报 `clashing_extern_declarations`，而且签名一旦漂移就是未定义行为。
// 改这里时请同步那边。
#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXUIElementCreateApplication(pid: i32) -> *const c_void;
    fn AXUIElementCreateSystemWide() -> *mut c_void;
    fn AXUIElementCopyElementAtPosition(
        application: *mut c_void,
        x: f32,
        y: f32,
        element: *mut *mut c_void,
    ) -> i32;
    fn AXUIElementGetPid(element: *mut c_void, pid: *mut i32) -> i32;
    fn AXUIElementSetMessagingTimeout(element: *const c_void, seconds: f32) -> i32;
    fn AXUIElementCopyAttributeValue(
        element: *const c_void,
        attribute: *const c_void,
        value: *mut *const c_void,
    ) -> i32;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFStringCreateWithCString(alloc: *const c_void, s: *const i8, enc: u32) -> *const c_void;
    fn CFArrayGetCount(arr: *const c_void) -> isize;
    fn CFArrayGetValueAtIndex(arr: *const c_void, i: isize) -> *const c_void;
    fn CFURLGetFileSystemRepresentation(
        url: *const c_void,
        resolve: bool,
        buf: *mut u8,
        len: isize,
    ) -> bool;
    fn CFGetTypeID(value: *const c_void) -> u64;
    fn CFArrayGetTypeID() -> u64;
    fn CFURLGetTypeID() -> u64;
    fn CFRelease(value: *const c_void);
}

/// `kCFStringEncodingUTF8`
const CF_UTF8: u32 = 0x0800_0100;
/// AX 递归的最大深度（Finder 的浏览器视图一般 5–7 层）。
const AX_MAX_DEPTH: usize = 10;

pub(super) fn read_selection_ax() -> FastSelection {
    if !frontmost_is_finder() {
        return FastSelection::Unknown;
    }
    let Some(pid) = frontmost_pid() else {
        return FastSelection::Unknown;
    };
    unsafe {
        let app = AXUIElementCreateApplication(pid as i32);
        if app.is_null() {
            return FastSelection::Unknown;
        }
        // 消息超时：Finder 忙（正在拖拽/刷新）时不让调用方无限等。
        AXUIElementSetMessagingTimeout(app, 0.6);
        let window = copy_attr(app, "AXFocusedWindow");
        let found = window.and_then(|w| {
            let hit = collect_selected(w, 0);
            CFRelease(w);
            hit
        });
        CFRelease(app);
        match found {
            Some(paths) if paths.is_empty() => FastSelection::Empty,
            Some(paths) => FastSelection::Items(paths),
            None => FastSelection::Unknown,
        }
    }
}

unsafe fn cfstr(s: &str) -> *const c_void {
    let Ok(c) = std::ffi::CString::new(s) else {
        return std::ptr::null();
    };
    CFStringCreateWithCString(std::ptr::null(), c.as_ptr(), CF_UTF8)
}

/// 读一个 AX 属性（调用方负责 `CFRelease` 返回值）。
unsafe fn copy_attr(el: *const c_void, name: &str) -> Option<*const c_void> {
    let attr = cfstr(name);
    if attr.is_null() {
        return None;
    }
    let mut value: *const c_void = std::ptr::null();
    let status = AXUIElementCopyAttributeValue(el, attr, &mut value);
    CFRelease(attr);
    (status == 0 && !value.is_null()).then_some(value)
}

unsafe fn array_items(value: *const c_void) -> Vec<*const c_void> {
    let n = CFArrayGetCount(value);
    (0..n.max(0)).map(|i| CFArrayGetValueAtIndex(value, i)).collect()
}

/// CFURL → POSIX 路径。
unsafe fn url_path(value: *const c_void) -> Option<String> {
    let mut buf = [0i8; 1024];
    let ok = CFURLGetFileSystemRepresentation(
        value,
        true,
        buf.as_mut_ptr() as *mut u8,
        buf.len() as isize,
    );
    if !ok {
        return None;
    }
    std::ffi::CStr::from_ptr(buf.as_ptr())
        .to_str()
        .ok()
        .map(str::to_string)
}

/// 在 AX 树里找第一个非空的选中项集合。
///
/// 列表视图把选中项挂在 `AXSelectedRows`、图标视图挂在 `AXSelectedChildren`，
/// 位置随视图模式变，所以按「先看自己、再往下找」递归——不写死层级。
unsafe fn collect_selected(el: *const c_void, depth: usize) -> Option<Vec<String>> {
    if depth > AX_MAX_DEPTH {
        return None;
    }
    for attr in ["AXSelectedRows", "AXSelectedChildren"] {
        let Some(value) = copy_attr(el, attr) else {
            continue;
        };
        let is_array = CFGetTypeID(value) == CFArrayGetTypeID();
        let mut paths = Vec::new();
        if is_array {
            for item in array_items(value) {
                if let Some(url) = copy_attr(item, "AXURL") {
                    if CFGetTypeID(url) == CFURLGetTypeID() {
                        if let Some(p) = url_path(url) {
                            paths.push(p);
                        }
                    }
                    CFRelease(url);
                }
            }
        }
        CFRelease(value);
        if !paths.is_empty() {
            return Some(paths);
        }
    }
    let children = copy_attr(el, "AXChildren")?;
    let is_array = CFGetTypeID(children) == CFArrayGetTypeID();
    let mut found = None;
    if is_array {
        for child in array_items(children).into_iter().take(60) {
            if let Some(paths) = collect_selected(child, depth + 1) {
                found = Some(paths);
                break;
            }
        }
    }
    CFRelease(children);
    found
}

/* ------------------------------------------------------------------ */
/* 指针命中：起手文件（真实拖放载荷的权威来源）                          */
/* ------------------------------------------------------------------ */

/// 屏幕坐标（逻辑点，左上原点）下的 Finder 起手点归类。
///
/// 为什么不用「选中项」：用户可能拖一个**原本未选中**的文件，Finder 会在
/// mouseDown 里改选中；拖动接管后又吞掉了移动事件，松手时选中已经不可信。
/// 直接按起手点命中 AX 元素，拿到的就是手指下那个东西。
///
/// 必须在 mouseDown 回调里同步调用（`option_tap` 就是这么做的）：Finder 一
/// 进入事件追踪循环就不再响应 AX，读会一直阻塞到松手（实测 4.9s）。
pub(super) fn start_hit(x: f64, y: f64) -> StartHit {
    if !frontmost_is_finder() {
        return StartHit::Unknown;
    }
    let Some(pid) = frontmost_pid() else {
        return StartHit::Unknown;
    };
    let pid = pid as i32;
    unsafe {
        let system = AXUIElementCreateSystemWide();
        if system.is_null() {
            return StartHit::Unknown;
        }
        // 起手瞬间 Finder 可能正忙：宁可拿不到，也不要把事件回调卡住。
        AXUIElementSetMessagingTimeout(system, 0.2);
        let mut element: *mut c_void = std::ptr::null_mut();
        let status = AXUIElementCopyElementAtPosition(system, x as f32, y as f32, &mut element);
        let mut out = StartHit::Unknown;
        if status == 0 && !element.is_null() {
            let mut hit_pid: i32 = 0;
            AXUIElementGetPid(element, &mut hit_pid);
            // 只认 Finder 自己的元素：别的窗口盖在上面时不能误取，也不算「不是文件」。
            if hit_pid == pid {
                out = match file_url_of_ancestors(element) {
                    UrlLookup::Found(path) => {
                        if Path::new(&path).is_dir() {
                            // 文件夹不是转换载荷（§3.1：拖动内容必须包含文件）。
                            StartHit::NotFile
                        } else {
                            StartHit::File(path)
                        }
                    }
                    // 上溯干净结束、没有 URL：指针在空白/工具栏/侧边栏上。
                    UrlLookup::Absent => StartHit::NotFile,
                    UrlLookup::Timeout => StartHit::Unknown,
                };
            }
            CFRelease(element);
        }
        CFRelease(system);
        out
    }
}

/// AX 上溯 `AXURL` 的三种结果（「没找到」和「没来得及找」必须区分）。
enum UrlLookup {
    Found(String),
    /// 正常走到根，始终没有 `AXURL`：命中物本身不是文件。
    Absent,
    /// 撞上 400ms 硬截止：可能是有文件但没读完。
    Timeout,
}

/// 从命中元素向上找 `AXURL`：图标视图把 URL 挂在行/列上，列表视图在单元格上。
///
/// `element` 由调用方持有（本函数不释放它），中间取到的 parent 由这里释放。
///
/// 整段有约 400ms 的硬截止：本函数跑在全局事件 tap 的回调里，单个 AX 调用
/// 再快，十层上溯也可能拖到 tap 超时（tap 被系统禁用后下一次手势就失灵）。
unsafe fn file_url_of_ancestors(element: *const c_void) -> UrlLookup {
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(400);
    let mut current = element;
    let mut owned: Option<*const c_void> = None;
    let mut depth = 0usize;
    let mut found = None;
    while depth <= AX_MAX_DEPTH {
        if std::time::Instant::now() >= deadline {
            // 还没读到头就到时了：不能断言「不是文件」。
            if let Some(old) = owned {
                CFRelease(old);
            }
            return UrlLookup::Timeout;
        }
        if let Some(value) = copy_attr(current, "AXURL") {
            let is_url = CFGetTypeID(value) == CFURLGetTypeID();
            let path = if is_url { url_path(value) } else { None };
            CFRelease(value);
            if let Some(path) = path {
                found = Some(path);
                break;
            }
        }
        let Some(parent) = copy_attr(current, "AXParent") else {
            break;
        };
        if let Some(old) = owned.replace(parent) {
            CFRelease(old);
        }
        // 每个新元素单独设超时：新建的 AX 元素不会继承别处的超时值。
        AXUIElementSetMessagingTimeout(parent, 0.2);
        current = parent;
        depth += 1;
    }
    if let Some(old) = owned {
        CFRelease(old);
    }
    found.map(UrlLookup::Found).unwrap_or(UrlLookup::Absent)
}

/// 读取 Finder 选中项（阻塞，调用方须在工作线程执行）。
pub fn read_selection() -> FinderSelection {
    // 快路径：AX 直读，几十毫秒；读不出来才退回 osascript。
    match read_selection_ax() {
        FastSelection::Items(paths) => return FinderSelection::Items(paths),
        FastSelection::Empty => return FinderSelection::Empty,
        FastSelection::Unknown => {}
    }
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || tx.send(run_osascript()));
    match rx.recv_timeout(SELECTION_TIMEOUT) {
        Ok(Ok(stdout)) => {
            let paths = split_paths(&stdout);
            if paths.is_empty() {
                FinderSelection::Empty
            } else {
                FinderSelection::Items(paths)
            }
        }
        Ok(Err(stderr)) => {
            if is_permission_error(&stderr) {
                FinderSelection::PermissionDenied
            } else if is_timeout_error(&stderr) {
                FinderSelection::Timeout
            } else {
                FinderSelection::Failed(one_line(&stderr))
            }
        }
        // 超时：osascript 仍在等授权/等 Finder，回收该线程由进程退出完成；
        // 这里必须立刻返回，否则环形盘永远不出现。
        Err(_) => FinderSelection::Timeout,
    }
}

/// 执行 osascript，成功返回 stdout，失败返回 stderr 文本。
fn run_osascript() -> Result<String, String> {
    let mut child = match Command::new("osascript")
        .arg("-e")
        .arg(SELECTION_SCRIPT)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(err) => return Err(format!("无法启动 osascript：{err}")),
    };

    // stderr 必须在 wait 前抽干：AppleScript 的报错文案在 stderr，
    // 管道满了会反过来卡住子进程。
    let mut stderr_pipe = child.stderr.take();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut buf = String::new();
        if let Some(pipe) = stderr_pipe.as_mut() {
            let _ = pipe.read_to_string(&mut buf);
        }
        let _ = tx.send(buf);
    });

    let mut stdout_buf = String::new();
    if let Some(mut out) = child.stdout.take() {
        let _ = out.read_to_string(&mut stdout_buf);
    }
    let status = match child.wait() {
        Ok(status) => status,
        Err(err) => return Err(format!("等待 osascript 失败：{err}")),
    };
    let stderr = rx.recv().unwrap_or_default();
    if status.success() {
        Ok(stdout_buf)
    } else {
        Err(if stderr.trim().is_empty() {
            format!("退出码 {:?}", status.code())
        } else {
            stderr
        })
    }
}

/// 把多行错误压成一行（只进提示文案与日志）。
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 前台 App 是否为 Finder。
pub fn frontmost_is_finder() -> bool {
    frontmost_bundle_id().as_deref() == Some(FINDER_BUNDLE_ID)
}

/// 前台 App 是否是本进程（环形盘/回复选择器自己拿到焦点时不能判成「非 Finder」）。
pub fn frontmost_is_self() -> bool {
    frontmost_pid() == Some(std::process::id() as i64)
}

fn frontmost_bundle_id() -> Option<String> {
    frontmost(|app| {
        let value: *mut objc2::runtime::AnyObject =
            unsafe { objc2::msg_send![app, bundleIdentifier] };
        unsafe { nsstring_to_option(value) }
    })
}

fn frontmost_pid() -> Option<i64> {
    frontmost(|app| unsafe {
        let pid: i32 = objc2::msg_send![app, processIdentifier];
        Some(pid as i64)
    })
}

/// 取 `NSWorkspace.frontmostApplication` 并交给闭包解读。
fn frontmost<T>(read: impl FnOnce(*mut objc2::runtime::AnyObject) -> Option<T>) -> Option<T> {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;

    unsafe {
        let workspace: *mut AnyObject = msg_send![objc2::class!(NSWorkspace), sharedWorkspace];
        if workspace.is_null() {
            return None;
        }
        let app: *mut AnyObject = msg_send![workspace, frontmostApplication];
        if app.is_null() {
            return None;
        }
        read(app)
    }
}

/// NSString → Option<String>（UTF8String + 长度，避免含 NUL/多字节截断）。
unsafe fn nsstring_to_option(string: *mut objc2::runtime::AnyObject) -> Option<String> {
    use objc2::msg_send;
    if string.is_null() {
        return None;
    }
    let utf8: *const std::os::raw::c_char = msg_send![string, UTF8String];
    if utf8.is_null() {
        return None;
    }
    // NSUTF8StringEncoding = 4
    let length: usize = msg_send![string, lengthOfBytesUsingEncoding: 4usize];
    let bytes = std::slice::from_raw_parts(utf8.cast::<u8>(), length);
    let text = String::from_utf8_lossy(bytes).into_owned();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}
