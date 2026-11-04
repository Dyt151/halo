//! Finder 里 Option + 左键拖动文件 → 弹出环形盘换格式（Filee 默认手势）。
//!
//! 为什么不用全局快捷键：`RegisterEventHotKey` 只能注册「修饰键 + 主键」的组合，
//! Option+拖动不是热键，拿不到系统通知；要看到鼠标拖动只能靠 `CGEventTap`
//! （Filee 的原生实现同样在这一层做手势判定）。
//!
//! 判定与接管策略（宁可漏触发，也不误触发）：
//! - 起手时 Option 必须已经按下；拖动过半路再按 Option 不接管——那时 Finder
//!   已经在拖文件，吞事件会让它的拖拽会话卡住；
//! - 位移超过 [`DRAG_THRESHOLD`] 才接管，Option+单击不该弹盘；
//! - 接管后**吞掉**后续 `leftMouseDragged`：Finder 收不到移动就不会启动真正的
//!   文件拖拽，文件不会被拖去别处；松手事件照常放行，Finder 把它当成一次单击
//!   （于是选中该文件）。
//! - **载荷只看起手**：`mouseDown` 里同步调 `grab_start_file` 命中手指下的那个
//!   文件或文件夹，松手时不再复核。接管后移动事件被吞，Finder 的选中项已经不可信，
//!   多选时再读一次只会把载荷从「起手那个文件」改成「一堆文件」。
//! - 只在前台是 Finder 时接管；环形盘已经在屏上时不重复唤起。
//!
//! 权限：`CGEventTap` 属于「辅助功能」范畴，未授权时创建失败（回贴功能本来就依赖
//! 这套授权，所以这里不再单独弹系统引导，只在日志与设置页提示）。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use tauri::AppHandle;

use crate::core::finder;
use crate::core::ring::RingManager;
use crate::lock_ext::LockExt;

/// 位移超过这个距离（屏幕点）才接管：Option+单击不应该弹盘。
const DRAG_THRESHOLD: f64 = 8.0;

/// 用户配置里这个手势是否开启（`ring.option_tap`，与设置页开关同一个键）。
static ENABLED: AtomicBool = AtomicBool::new(false);
/// 监听线程是否已安装（只装一次）。
static INSTALLED: AtomicBool = AtomicBool::new(false);
/// tap 是否**真的建成功**。
///
/// 与 [`INSTALLED`] 的区别：线程可能一直在跑，却因为没授权每次都建不出 tap。
/// 设置页要能区分「没授权」和「授权了但监听没起来」，否则用户只能猜。
static TAP_ALIVE: AtomicBool = AtomicBool::new(false);
/// 最近一次判定结果（给人看的），成功后清空。
static LAST_NOTE: Mutex<Option<String>> = Mutex::new(None);
/// 事件监听线程要用的 AppHandle（事件回调里拿不到）。
static APP: OnceLock<AppHandle> = OnceLock::new();

/// 左键已按下且 Option 按着：本次手势有可能成为 Option+拖动。
static ARMED: AtomicBool = AtomicBool::new(false);
/// 已越过阈值并接管：后续拖拽一律吞掉，直到松手。
static TAKEN: AtomicBool = AtomicBool::new(false);
/// 起手点（屏幕逻辑坐标）。
static ORIGIN: Mutex<Option<(f64, f64)>> = Mutex::new(None);

/// 启动时按配置初始化（开启才真正安装事件监听）。
pub fn init(app: &AppHandle, enabled: bool) {
    let _ = APP.set(app.clone());
    ENABLED.store(enabled, Ordering::SeqCst);
    let accessibility = crate::core::paste::has_accessibility_permission();
    tracing::info!(enabled, accessibility, "Option+拖动：初始化");
    if enabled {
        if !accessibility {
            // 没有授权时 tap 一定建不出来，用户按 Option 永远「没反应」。
            // 这里主动把 App 登记进「辅助功能」列表（带系统弹窗），并在数据目录
            // 留时间戳做冷却——拒绝过的用户不会每次启动都被弹一次。
            maybe_prompt_accessibility();
        }
        install();
    }
}

/// 主动请求授权的冷却时间：一周内只在首次启动弹一次。
const PROMPT_COOLDOWN_SECS: u64 = 7 * 24 * 3600;

/// 没授权时主动弹一次系统引导（`AXIsProcessTrustedWithOptions` + prompt）。
///
/// 为什么需要节流：带 prompt 的调用每次都会把用户拽去系统设置，启动即弹会变成
/// 骚扰；而完全不弹，用户就只会看到「按下 Option 没反应」。
fn maybe_prompt_accessibility() {
    let stamp = crate::storage::data_dir().join(".ax_prompt_at");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if let Ok(text) = std::fs::read_to_string(&stamp) {
        if let Ok(prev) = text.trim().parse::<u64>() {
            if now.saturating_sub(prev) < PROMPT_COOLDOWN_SECS {
                return;
            }
        }
    }
    let _ = std::fs::write(&stamp, now.to_string());
    tracing::info!("未获得辅助功能权限：已请求系统引导");
    crate::core::paste::request_accessibility_permission();
}

/// 手势现场状态（`ring_gesture_status` 的返回体）。
///
/// 「按下 Option 没反应」有四种完全不同的原因：开关关了、没授权、监听没起来、
/// 前台不是 Finder 或起手点不是文件。把它们摊开给设置页，用户才不用猜。
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GestureStatus {
    /// 设置里是否开启此手势
    pub enabled: bool,
    /// 系统「辅助功能」是否已授权（tap 的硬前提）
    pub accessibility: bool,
    /// 事件监听线程是否真的建好了 tap
    pub tap_alive: bool,
    /// 当前前台是否满足条件（Finder 或 Halo 自己）
    pub frontmost_ok: bool,
    /// 环形会话状态（idle / capturing / ready / converting …）
    pub state: String,
    /// 最近一次「没反应」的原因；接管成功后为 None
    pub last_note: Option<String>,
}

/// 读取现场状态（只读，不产生副作用）。
pub fn status() -> GestureStatus {
    GestureStatus {
        enabled: is_enabled(),
        accessibility: crate::core::paste::has_accessibility_permission(),
        tap_alive: TAP_ALIVE.load(Ordering::SeqCst),
        frontmost_ok: crate::core::finder::ring_context_ok(),
        state: crate::core::ring::RingManager::state_label().to_string(),
        last_note: LAST_NOTE.lock_ok().clone(),
    }
}

/// 一次「修复授权」的结果。
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AxRepair {
    /// `tccutil reset` 是否成功
    pub reset: bool,
    /// 重置后立刻复查的授权状态（通常仍为 false，等用户勾选）
    pub trusted: bool,
    /// 给用户看的一句话
    pub detail: String,
}

/// 清除系统里本 App 的辅助功能授权记录，并重新登记。
///
/// 解决的是「开关明明打开着，却一直不生效」：ad-hoc 签名的 App 每次重新构建
/// 都会换一个 cdhash，系统按旧的 cdhash 留着一条**已失效**的授权记录，界面显示
/// 已勾选，`AXIsProcessTrusted()` 却是 false。此时唯一可靠的修法是先把记录删掉
/// （`tccutil reset`），让系统用当前二进制的指纹重新登记一次。
///
/// 副作用要说清楚：回贴与本手势共用同一项授权，重置后两者都需要重新勾一次。
pub fn repair(bundle_id: &str) -> AxRepair {
    let output = std::process::Command::new("/usr/bin/tccutil")
        .args(["reset", "Accessibility", bundle_id])
        .output();
    let (reset, mut detail) = match &output {
        Ok(o) if o.status.success() => (
            true,
            format!("已清除 {bundle_id} 的旧授权记录。请在打开的系统设置里勾选 Halo：勾上后 3 秒内自动接管，无需重启。"),
        ),
        Ok(o) => (
            false,
            format!(
                "清除旧记录失败（{}）。请在系统设置里先取消勾选 Halo、再重新勾选。",
                String::from_utf8_lossy(&o.stderr).trim()
            ),
        ),
        Err(e) => (
            false,
            format!("无法执行 tccutil（{e}）。请在系统设置里先取消勾选 Halo、再重新勾选。"),
        ),
    };
    // 重新登记（带 prompt）：列表里没有 Halo 时，用户连勾选入口都找不到。
    let trusted = crate::core::paste::request_accessibility_permission();
    if trusted {
        detail = "辅助功能已授权，手势会在 3 秒内自动接管。".to_string();
    }
    tracing::info!(reset, trusted, bundle_id, "Option+拖动：修复授权");
    AxRepair {
        reset,
        trusted,
        detail,
    }
}

/// 记录一次判定结果，供设置页显示「为什么没反应」。
fn set_note(text: Option<&str>) {
    *LAST_NOTE.lock_ok() = text.map(str::to_string);
}

/// 配置热更新：切换开关。开启时若监听还没装就补装。
pub fn set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::SeqCst);
    if enabled {
        install();
    }
}

/// 当前是否开启（供状态查询/测试）。
pub fn is_enabled() -> bool {
    ENABLED.load(Ordering::SeqCst)
}

/* ------------------------------------------------------------------ */
/* 非 macOS：保持接口一致，纯 no-op                                     */
/* ------------------------------------------------------------------ */

#[cfg(not(target_os = "macos"))]
fn install() {}

/* ------------------------------------------------------------------ */
/* macOS：CGEventTap 旁观左键拖动                                        */
/* ------------------------------------------------------------------ */

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use std::ffi::c_void;
    use std::sync::atomic::AtomicPtr;

    /// 事件类型（CGEventType）：左键按下 / 松开 / 拖拽 + 修饰键变化 + 键盘。
    const LEFT_DOWN: u32 = 1;
    const LEFT_UP: u32 = 2;
    const LEFT_DRAG: u32 = 6;
    const KEY_DOWN: u32 = 10;
    const FLAGS_CHANGED: u32 = 12;
    /// 系统因超时/用户输入主动禁用 tap 时回调收到的伪事件类型。
    const TAP_DISABLED_BY_TIMEOUT: u32 = 0xFFFF_FFFE;
    const TAP_DISABLED_BY_USER_INPUT: u32 = 0xFFFF_FFFF;

    /// 左键三件套 + 修饰键变化（后者只用来在其他 App 空闲时预取选中项）
    /// + 按键（拖拽期间的 Esc 取消）。
    const MASK: u64 = (1 << LEFT_DOWN)
        | (1 << LEFT_UP)
        | (1 << LEFT_DRAG)
        | (1 << KEY_DOWN)
        | (1 << FLAGS_CHANGED);

    /// 修饰键位（CGEventFlags）。
    const FLAG_OPTION: u64 = 1 << 19;
    const FLAG_COMMAND: u64 = 1 << 20;

    /// 虚拟键码：Esc = 53（`kVK_Escape`）。
    const KEY_ESC: i64 = 53;

    /// 上一次观察到的 Option 状态，用于只处理「按下」那一次。
    static OPTION_DOWN: AtomicBool = AtomicBool::new(false);

    /// `CGEventGetLocation` 返回的屏幕坐标（逻辑点，左上原点）。
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGPoint {
        x: f64,
        y: f64,
    }

    /// tap 句柄：系统禁用后要在回调里重新打开。
    static TAP: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

    type TapCallback = extern "C" fn(*mut c_void, u32, *mut c_void, *mut c_void) -> *mut c_void;

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
        fn CGEventGetFlags(event: *mut c_void) -> u64;
        fn CGEventGetLocation(event: *mut c_void) -> CGPoint;
        fn CGEventGetIntegerValueField(event: *mut c_void, field: u32) -> i64;
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
    }

    /// 安装监听：独立线程 + 自己的 run loop，不占 Tauri 主线程。
    pub(super) fn install() {
        if INSTALLED.swap(true, Ordering::SeqCst) {
            return;
        }
        std::thread::Builder::new()
            .name("option-drag-tap".into())
            .spawn(|| unsafe { run_tap() })
            .ok();
    }

    unsafe fn run_tap() {
        // 与桌面右键监听同一套策略：拿不到权限就隔几秒重试，而不是直接放弃。
        //
        // 为什么必须重试：TCC 授权是在**这个进程活着**的时候变的，用户刚在
        // 系统设置里勾上那一秒，进程不会自己重跑 `init`——一次性安装会让
        // 用户看到「明明授权了，还是没反应」，只能重启 App 才能兑现。
        let mut warned = false;
        loop {
            if !ENABLED.load(Ordering::SeqCst) {
                INSTALLED.store(false, Ordering::SeqCst);
                TAP_ALIVE.store(false, Ordering::SeqCst);
                return;
            }
            if !crate::core::paste::has_accessibility_permission() {
                TAP_ALIVE.store(false, Ordering::SeqCst);
                set_note(Some(
                    "缺少「辅助功能」权限：系统设置 → 隐私与安全性 → 辅助功能 里勾选 Halo",
                ));
                if !warned {
                    tracing::warn!(
                        "Option+拖动需要「辅助功能」权限，授权后会自动装上监听（不必重启）"
                    );
                    warned = true;
                }
            } else {
                // 会话级 tap + 默认 options：需要能**吞掉**接管后的拖拽事件。
                let tap = CGEventTapCreate(1, 0, 0, MASK, callback, std::ptr::null_mut());
                if tap.is_null() {
                    TAP_ALIVE.store(false, Ordering::SeqCst);
                    set_note(Some(
                        "事件监听创建失败：授权记录可能已失效，请点「修复授权」",
                    ));
                    if !warned {
                        tracing::warn!("创建 Option+拖动 监听失败（辅助功能权限不足？），稍后重试");
                        warned = true;
                    }
                } else if let Some(kind) = attach(tap) {
                    TAP_ALIVE.store(true, Ordering::SeqCst);
                    set_note(None);
                    tracing::info!(kind, "Option+拖动 监听已启动（仅 Finder 前台生效）");
                    CFRunLoopRun();
                    TAP_ALIVE.store(false, Ordering::SeqCst);
                } else {
                    TAP_ALIVE.store(false, Ordering::SeqCst);
                    set_note(Some("事件监听未能挂到 run loop"));
                }
            }
            std::thread::sleep(std::time::Duration::from_secs(3));
        }
    }

    /// 真装上监听。返回失败原因供日志（`"source"` = run loop source 建不出来）。
    unsafe fn attach(tap: *mut c_void) -> Option<&'static str> {
        TAP.store(tap, Ordering::SeqCst);
        let source = CFMachPortCreateRunLoopSource(std::ptr::null(), tap, 0);
        if source.is_null() {
            return None;
        }
        CFRunLoopAddSource(CFRunLoopGetCurrent(), source, kCFRunLoopCommonModes);
        CGEventTapEnable(tap, true);
        Some("ok")
    }

    extern "C" fn callback(
        _proxy: *mut c_void,
        event_type: u32,
        event: *mut c_void,
        _user_info: *mut c_void,
    ) -> *mut c_void {
        match event_type {
            // 系统会在回调超时/安全输入时禁用 tap，必须自己重新打开，
            // 否则「用一阵子就失灵」。
            TAP_DISABLED_BY_TIMEOUT | TAP_DISABLED_BY_USER_INPUT => unsafe {
                let tap = TAP.load(Ordering::SeqCst);
                if !tap.is_null() {
                    CGEventTapEnable(tap, true);
                }
                event
            },
            LEFT_DOWN => {
                unsafe { on_down(event) };
                event
            }
            LEFT_DRAG => unsafe { on_drag(event) },
            KEY_DOWN => unsafe { on_key(event) },
            FLAGS_CHANGED => {
                unsafe { on_flags(event) };
                event
            }
            LEFT_UP => {
                on_up();
                event
            }
            _ => event,
        }
    }

    /// Option 按下：趁 Finder 还没被 mouseDown 追踪循环占住主线程，先把选中项读出来。
    ///
    /// 拖动接管时再读会阻塞到松手（实测 4.9s），所以这里是拖放体验的关键一步。
    unsafe fn on_flags(event: *mut c_void) {
        let down = CGEventGetFlags(event) & FLAG_OPTION != 0;
        if down == OPTION_DOWN.swap(down, Ordering::SeqCst) || !down {
            return;
        }
        if !ENABLED.load(Ordering::SeqCst)
            || RingManager::is_displaying()
            || !finder::ring_context_ok()
        {
            return;
        }
        std::thread::spawn(|| {
            if let crate::core::finder::FinderSelection::Items(paths) = finder::read_selection() {
                RingManager::prefetch_selection(paths);
            }
        });
    }

    /// 起手：Option + 左键按下，且现场是 Finder、环形盘没在屏上 → 记下起点并
    /// **同步**命中起手文件。
    ///
    /// 为何在这里读：Finder 拿到 mouseDown 后立即进事件追踪循环，AX 会阻塞到
    /// 松手（实测 4.9s）；而此刻事件还没交给 Finder，指针下就是用户按下的那个文件。
    unsafe fn on_down(event: *mut c_void) {
        // 上一次手势没等到松手（切 App 等极端情况）就复位，免得后续拖拽被永久吞掉。
        TAKEN.store(false, Ordering::SeqCst);
        let flags = CGEventGetFlags(event);
        let option = flags & FLAG_OPTION != 0;
        let enabled = ENABLED.load(Ordering::SeqCst);
        let displaying = RingManager::is_displaying();
        let context_ok = finder::ring_context_ok();
        let armed = option && enabled && !displaying && context_ok;
        if !armed {
            // 四个条件任意一个不满足都只能「没反应」，所以必须留下是哪一个。
            // 只有**按着 Option** 的那次才写，普通点击不覆盖上一次的真实原因。
            if option {
                let reason = if !enabled {
                    "设置里已关闭「Option + 拖动唤起」"
                } else if displaying {
                    "环形盘已经在屏上"
                } else {
                    "前台不是 Finder（本手势只在 Finder 与 Halo 自己里生效）"
                };
                set_note(Some(reason));
            }
            tracing::debug!(
                option,
                enabled,
                displaying,
                context_ok,
                state = %RingManager::state_label(),
                "Option+拖动：起手未待命"
            );
            idle();
            return;
        }
        let p = CGEventGetLocation(event);
        *ORIGIN.lock_ok() = Some((p.x, p.y));
        // 起手点能确定不是文件也不是文件夹（空白 / 工具栏 / 侧边栏）：本次手势直接不成立。
        // 这就是验收项「非文件拖动不显示圆环」——不能先弹出盘再解释「没读到文件」。
        if !RingManager::grab_start_file(p.x, p.y) {
            set_note(Some(
                "起手点没有文件或文件夹：请从图标上按下左键（空白、工具栏、侧边栏都不算）",
            ));
            idle();
            return;
        }
        set_note(None);
        ARMED.store(true, Ordering::SeqCst);
        tracing::debug!(x = p.x, y = p.y, "option_drag: 待命（Option+左键按下）");
    }

    /// 拖动：越过阈值就接管，之后一律吞掉（返回空指针 = 事件不进 Finder）。
    unsafe fn on_drag(event: *mut c_void) -> *mut c_void {
        if TAKEN.load(Ordering::SeqCst) {
            let p = CGEventGetLocation(event);
            RingManager::drag_pointer(p.x, p.y);
            return std::ptr::null_mut();
        }
        if !ARMED.load(Ordering::SeqCst) {
            return event;
        }
        let Some((ox, oy)) = *ORIGIN.lock_ok() else {
            return event;
        };
        let p = CGEventGetLocation(event);
        if (p.x - ox).abs() <= DRAG_THRESHOLD && (p.y - oy).abs() <= DRAG_THRESHOLD {
            // 还没越过阈值，也必须吞。
            //
            // Finder 自己的拖拽阈值只有 3~4px，比这里小：一旦放行，它会先启动真实
            // 的文件拖拽，之后我们再把事件吞掉，只会把拖拽卡在半路，松手时把文件
            // 丢在指针位置。松手事件仍然放行，Finder 会把 down + up 当成一次单击。
            return std::ptr::null_mut();
        }
        // 越过阈值：接管本次手势，环形盘钉在起手点。
        TAKEN.store(true, Ordering::SeqCst);
        ARMED.store(false, Ordering::SeqCst);
        if let Some(app) = APP.get().cloned() {
            tracing::info!(x = ox, y = oy, "Option+拖动：唤起环形盘");
            std::thread::spawn(move || RingManager::start_from_drag(&app, ox, oy));
        }
        RingManager::drag_pointer(p.x, p.y);
        std::ptr::null_mut()
    }

    /// 拖拽期间的按键：Esc 取消，其余放行。
    ///
    /// 带 Cmd 的组合键放行（Cmd+Tab 是系统切换器，吞掉等于抢用户的操作）；
    /// 吞掉后 **TAKEN 保持 true** 直到松手：Finder 只收到 down + up，把这次
    /// 操作当成一次单击，不会中途启动真实拖拽（与拖拽事件的吞法同一套逻辑）。
    unsafe fn on_key(event: *mut c_void) -> *mut c_void {
        if !TAKEN.load(Ordering::SeqCst) {
            return event;
        }
        let flags = CGEventGetFlags(event);
        if flags & FLAG_COMMAND != 0 {
            return event;
        }
        let keycode = CGEventGetIntegerValueField(event, 9); // kCGKeyboardEventKeycode
        let autorepeat = CGEventGetIntegerValueField(event, 8); // kCGKeyboardEventAutorepeat
        if autorepeat != 0 {
            return event;
        }
        match keycode {
            KEY_ESC => {
                tracing::info!("Option+拖动：Esc 取消");
                RingManager::cancel();
                std::ptr::null_mut()
            }
            _ => event,
        }
    }

    /// 松手：放行给 Finder（它没收到移动，会把这次操作当成单击选中），
    /// 同时通知环形窗按当前 hover 提交。
    fn on_up() {
        let taken = TAKEN.swap(false, Ordering::SeqCst);
        idle();
        if taken {
            tracing::info!("Option+拖动：松手");
            RingManager::drag_end();
        }
    }

    fn idle() {
        ARMED.store(false, Ordering::SeqCst);
        *ORIGIN.lock_ok() = None;
    }
}

#[cfg(target_os = "macos")]
use macos::install;
