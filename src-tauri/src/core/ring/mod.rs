//! Finder 环形转换会话状态机（总规范 `docs/REA-Filee-Ring-Integrated.md`）。
//!
//! 为什么单独一层：手势、执行与结果是同一件事的三个阶段，必须汇入**同一个**
//! 会话与执行路径，否则命名、输出目录、错误处理会各自漂移。
//!
//! 状态机（§3.3）：
//! `Idle → Capturing → Probing → Ready（唯一一面）/ Empty / PermissionDenied /
//! Timeout / Failed → Converting → Result`
//!
//! 关键约束：
//! - 唯一入口是 Finder 里 Option＋左键拖动（`option_tap`）；不再有快捷键、
//!   Finder 右键服务、单击、Enter、方向键。
//! - 只有一面盘（背面、Tab / 滚轮翻面已取消）；拖拽期间 Esc 取消。
//! - 打包 / 解压 / PDF 分页等工具动作不进 Profile，运行时按固定顺序追加在
//!   正面末尾（`RingConfig::utility_actions` 开关控制）。
//! - 载荷以**起手文件**为权威（`finder::file_at_point`），不再做松手复核，
//!   也不要求用户再拖一次。
//! - 选盘分两步：先用本批扩展名选一个 Profile，再算每个扇区是否可用（
//!   §2.3）。不可用的预设**保留位置**并带原因，不重排其他扇区。
//! - 执行只用当次快照的路径与预设副本，配置后续变更不污染运行中的任务。

pub mod profile;

use std::sync::{Arc, Mutex, OnceLock};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::config::settings::{Config, RingProfile, RingProfileKind};
use crate::core::convert::action;
use crate::core::convert::job::{self, CancelToken, ConversionFileResult};
use crate::core::convert::preset::{self, find_or_legacy, ConversionPreset};
use crate::core::convert::{self, ConvertFormat};
use crate::core::finder;
use crate::lock_ext::LockExt;

/// 环形会话状态更新事件（Rust → ring 窗口）。
pub const RING_UPDATE: &str = "ring://update";
/// 转换进度事件（逐文件，原地进度环，对齐 Filee §1.4）。
pub const RING_PROGRESS: &str = "ring://progress";
/// 转换结果事件（原地成功/失败反馈）。
pub const RING_RESULT: &str = "ring://result";
/// 转换超过 5s 时把环形盘收缩交给系统通知的事件（Filee §1.4 #10）。
pub const RING_HANDOFF: &str = "ring://handoff";
/// Option+拖动期间：指针位置（屏幕逻辑坐标），供拖放模式做命中与高亮。
pub const RING_POINTER: &str = "ring://pointer";
/// Option+拖动期间：松手，前端按当前 hover 提交或取消（Filee 拖放模式）。
pub const RING_DRAG_END: &str = "ring://drag-end";

/// 超过这个时长仍未转完，就把环形盘收缩交给通知，不再挡在屏幕中央。
const HANDOFF_AFTER: std::time::Duration = std::time::Duration::from_secs(5);

/// 启动时登记 AppHandle，供无 `app` 参数的调用点（ObjC 服务回调）使用。
static APP: OnceLock<AppHandle> = OnceLock::new();
/// 当前会话（同一时刻只有一个环形转换在跑）。
static SESSION: OnceLock<Arc<Mutex<Session>>> = OnceLock::new();
/// 拖放模式的起手点（屏幕逻辑坐标）；`None` = 盘还没定位。
static DRAG_ORIGIN: Mutex<Option<(f64, f64)>> = Mutex::new(None);
/// 起手文件（按下左键时指针下的 Finder 文件）。
///
/// 它是本次拖动载荷的**权威**部分：用户可能拖一个原本未选中的文件，而接管后
/// 拖拽事件被吞掉，Finder 只看到单击，松手时选中项已经被收缩掉。
static GRABBED: Mutex<Option<String>> = Mutex::new(None);
/// Option 按下时预取的 Finder 选中项（时间戳毫秒, 路径）。
///
/// 为什么不现读：Finder 在 mouseDown 后会进事件追踪循环，主线程不再响应 AX，
/// 读选中项会**一直阻塞到松手**（实测 4.9s），拖动全程拿不到动作。
/// 它只用来把「多选中的一项」扩展成整个选择，判决权始终在 [`GRABBED`]。
static PREFETCH: Mutex<Option<(u64, Vec<String>)>> = Mutex::new(None);
/// 预取的有效期：太久之前的选中项很可能已经不是用户要拖的文件。
const PREFETCH_TTL_MS: u64 = 5_000;
/// 最近一次拖动指针（已换算成环形窗内的 CSS 坐标）。
///
/// 环铺开前发出的 `RING_POINTER` 会被前端丢弃（那时还没 ready），所以松手时
/// 随 `RING_DRAG_END` 再带一次，避免「拖到扇区后停住、不再移动、直接松手」
/// 因为没收到过位置而被判成取消。
static LAST_POINTER: Mutex<Option<(f64, f64)>> = Mutex::new(None);

fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn session() -> Arc<Mutex<Session>> {
    SESSION.get_or_init(|| Arc::new(Mutex::new(Session::default()))).clone()
}

/// 保存状态机的原始数据；对外快照 [`RingSnapshot`] 由它派生。
#[derive(Default)]
struct Session {
    /// 单调递增的会话标识：新会话递增它，使旧环形盘自动失效（任务 16）。
    generation: u64,
    state: State,
    /// 本次捕获的选中项路径快照（绝对路径）。
    paths: Vec<String>,
    /// 选中的 Profile id（诊断用，§3.4）：出问题时能看出用了哪张盘。
    profile: String,
}

/// 状态机的内部状态。
///
/// 只剩「单手势」能走到的阶段：失败/空不再分门别类枚举，而是同一张提示盘里
/// 带不同原因（`build_snapshot` 生成 hint），前端只需认 `empty` 一种。
#[derive(Debug, Default, Clone, PartialEq, Eq)]
enum State {
    #[default]
    Idle,
    Capturing,
    Probing,
    Ready,
    Empty,
    Converting,
}

impl State {
    fn as_str(&self) -> &'static str {
        match self {
            State::Idle => "idle",
            State::Capturing => "capturing",
            State::Probing => "probing",
            State::Ready => "ready",
            State::Empty => "empty",
            State::Converting => "converting",
        }
    }
}

/// 单个环形扇区（RingSlice）。
#[derive(Debug, Clone, Serialize)]
pub struct RingAction {
    /// 动作 id = **预设 id**（`img.jpg` / `doc.md`），不再是格式名（§8.1）。
    pub id: String,
    /// 展示名（含 JPEG 质量，如「JPG 85%」）。
    pub label: String,
    /// 目标格式（`None` = 与源格式相同，只有 `reencode_same_format` 预设会这样）。
    pub target: Option<String>,
    /// 该动作将实际转换的文件数（有路由、且不需要跳过）。
    pub convert: usize,
    /// 不会转换的文件数（已是目标格式 / 无路由 / 格式不认识）。
    pub skip: usize,
    /// 本批文件下能否执行。`false` 仍占原位置，只是降低不透明度并显示原因。
    pub enabled: bool,
    /// 禁用原因（`enabled == true` 时为 `None`）。
    pub disabled_reason: Option<String>,
}

/// 转换进度（逐文件）。
#[derive(Debug, Clone, Serialize)]
pub struct RingProgressEvent {
    pub generation: u64,
    pub done: usize,
    pub total: usize,
}

/// 拖放模式（Option+拖动）下的指针位置：环形窗内的 CSS 坐标（左上原点）。
#[derive(Debug, Clone, Copy, Serialize)]
pub struct PointerEvent {
    pub x: f64,
    pub y: f64,
}

/// 转换结果（原地成功/失败反馈，对齐 Filee §1.4）。
#[derive(Debug, Clone, Serialize)]
pub struct RingResultEvent {
    pub generation: u64,
    pub converted: usize,
    pub skipped: usize,
    pub failed: usize,
    /// 被用户取消、未完成的文件数（§3：取消同样要给结果，不能无声）。
    pub cancelled: usize,
    pub errors: Vec<String>,
    /// 转成功了但**不保真**的地方（动画只剩首帧、ICC 未保留……）。§5 要求
    /// 这些不能静默丢掉，所以随结果一起回给环形盘展示。
    pub warnings: Vec<String>,
    /// 单个输出文件的绝对路径（成功时用于「在 Finder 中显示」）。
    pub output: Option<String>,
}

/// 本次会话选中的文件类型 Profile（§2.2）。
#[derive(Debug, Clone, Serialize)]
pub struct ProfileInfo {
    pub id: String,
    pub name: String,
    /// 归属本 Profile 的扩展名（空 = catch-all）。
    pub extensions: Vec<String>,
    /// 是否是混合兜底（前端用它在中心标注「其他文件」）。
    pub fallback: bool,
}

/// 前端用的会话快照（ring 窗口据此渲染）。
#[derive(Debug, Clone, Serialize)]
pub struct RingSnapshot {
    pub generation: u64,
    pub state: String,
    /// 类别标签：图片 / 文本 / 混合文件。
    pub category: Option<String>,
    /// 捕获到的可识别文件总数。
    pub count: usize,
    pub hint: Option<String>,
    /// 本批文件选中的 Profile；没有 Profile 时为 `None`。
    pub profile: Option<ProfileInfo>,
    /// 这张盘的扇区（位置固定，禁用项也在这里；末尾是工具动作）。
    pub actions: Vec<RingAction>,
    /// 按类别记住的最后动作：只用于初始高亮，不自动执行。
    pub suggested: Option<String>,
}

/// 环形会话管理器（无状态句柄，全部走进程级静态）。
pub struct RingManager;

impl RingManager {
    /// 启动时登记 AppHandle（在 `setup` 内调用一次）。
    pub fn set_app(app: AppHandle) {
        let _ = APP.set(app);
    }

    /// 当前 AppHandle（服务回调路径用）。
    fn app() -> Option<AppHandle> {
        APP.get().cloned()
    }

    /// 起手文件：左键按下时指针下的 Finder 文件（真实拖放载荷的权威部分）。
    ///
    /// 必须在 `leftMouseDown` 的回调里同步调用：Finder 一进事件追踪循环就不再
    /// 响应 AX，读会阻塞到松手（实测 4.9s），整个拖动拿不到载荷。
    ///
    /// 返回 `false` 表示可以**断言**这不是文件拖动（点在空白 / 工具栏 / 文件夹上）：
    /// 手势层应直接放弃本次接管，验收项「非文件拖动不显示圆环」。
    pub fn grab_start_file(x: f64, y: f64) -> bool {
        match finder::start_hit(x, y) {
            finder::StartHit::File(path) => {
                tracing::debug!(%path, x, y, "环形：起手文件");
                *GRABBED.lock_ok() = Some(path);
                true
            }
            finder::StartHit::NotFile => {
                tracing::debug!(x, y, "环形：起手点不是文件，本次手势不接管");
                *GRABBED.lock_ok() = None;
                false
            }
            finder::StartHit::Unknown => {
                // 读不到 ≠ 不是文件：保留预取作保底，盘会给出「没读到文件」的原因。
                tracing::debug!(x, y, "环形：起手文件读取失败（按未知处理）");
                *GRABBED.lock_ok() = None;
                true
            }
        }
    }

    /// Option 按下时预取 Finder 选中项（在拖动开始前读，避开 tracking loop 阻塞）。
    pub fn prefetch_selection(paths: Vec<String>) {
        tracing::debug!(count = paths.len(), "环形：预取选中项");
        *PREFETCH.lock_ok() = Some((now_ms(), paths));
    }

    /// 取预取结果（**不消耗**，5s 内可重复使用）。
    ///
    /// 不消耗是有意的：按住 Option 连拖两下时，第二次不会再有「按下 Option」
    /// 这个时机重新预取。过期就当没有，宁可只转起手那一个文件，也不转错。
    fn peek_prefetch() -> Option<Vec<String>> {
        let guard = PREFETCH.lock_ok();
        match guard.as_ref() {
            Some((at, paths)) if now_ms().saturating_sub(*at) <= PREFETCH_TTL_MS => {
                Some(paths.clone())
            }
            _ => None,
        }
    }

    /// 本次拖动的真实载荷（§3.2）。
    ///
    /// - 起手文件在预取选中项里：说明用户拖的是多选中的一项，Finder 会拖动
    ///   整个选择 —— 整批一起转，不能只转手指下那一个。
    /// - 起手文件不在预取里：用户拖的是原本未选中的文件，只转它自己。
    /// - AX 没命中（没点在文件图标上 / 读不到）：退回预取，宁可少转也不错转。
    fn drag_payload() -> Vec<String> {
        let grabbed = { GRABBED.lock_ok().take() };
        match grabbed {
            Some(file) => match Self::peek_prefetch() {
                Some(selected) if selected.iter().any(|p| p == &file) => selected,
                _ => vec![file],
            },
            None => Self::peek_prefetch().unwrap_or_default(),
        }
    }

    /// Option + 左键拖动入口（唯一入口，Filee `ShowForDrag`）。
    ///
    /// 环形盘钉在起手点，且不抢焦点；松手由 [`Self::drag_end`] 通知前端提交。
    pub fn start_from_drag(app: &AppHandle, x: f64, y: f64) {
        *DRAG_ORIGIN.lock_ok() = Some((x, y));
        *LAST_POINTER.lock_ok() = None;
        let paths = Self::drag_payload();
        tracing::info!(
            x,
            y,
            count = paths.len(),
            "Option+拖动：环形盘钉在起手点"
        );
        Self::begin(app, paths);
    }

    /// 拖放模式：把指针位置转给环形窗（换算成窗口内坐标）。
    ///
    /// 窗口此时 ignore 鼠标事件，前端拿不到指针，所以在这里做减法；
    /// 也让前端不必再去异步读窗口位置（那一版会丢掉铺开前后的前几个事件）。
    pub fn drag_pointer(x: f64, y: f64) {
        let Some(app) = Self::app() else {
            return;
        };
        let (lx, ly) = match crate::window::ring_window_origin() {
            Some((ox, oy)) => (x - ox, y - oy),
            None => (x, y),
        };
        *LAST_POINTER.lock_ok() = Some((lx, ly));
        let _ = app.emit(RING_POINTER, PointerEvent { x: lx, y: ly });
    }

    /// 拖放模式：松手（此时 Finder 已按“单击”处理，不会真的拖文件）。
    ///
    /// 顺带把最后一次指针位置带过去：前端在此之前可能一个位置都没收到（环还没
    /// ready 时的事件被丢），靠它补一次命中判定。
    pub fn drag_end() {
        if let Some(app) = Self::app() {
            let last = LAST_POINTER.lock_ok().take();
            let payload = last.map(|(x, y)| PointerEvent { x, y });
            let _ = app.emit(RING_DRAG_END, payload);
        }
    }

    /// 环形盘是否正占用屏幕（就绪/探测/转换都算），供重复唤出判断。
    ///
    /// 与 [`Self::is_active`] 的区别：它只看「还在跑」的状态，不包含已经铺开、
    /// 等用户选扇区的 Ready——拖拽中重复唤起时不能把盘重置重播。
    pub fn is_displaying() -> bool {
        matches!(
            Self::current_state(),
            State::Capturing | State::Probing | State::Ready | State::Converting
        )
    }

    /// 会话是否正在进行中（转换中禁止再次唤出，任务 16）。
    pub fn is_active() -> bool {
        matches!(
            session().lock_ok().state,
            State::Capturing | State::Probing | State::Converting
        )
    }

    /// 当前快照（ring 窗口挂载时拉一次）。
    pub fn snapshot() -> RingSnapshot {
        let config = current_config();
        // 先把数据拷出来再丢锁：`build_snapshot` 里会重新读配置/会话，
        // 持锁调用等于把自己锁在外面。
        let (generation, state, paths) = session().lock_ok().clone_snapshot_data();
        build_snapshot(generation, &state, &paths, &config)
    }

    fn current_state() -> State {
        session().lock_ok().state.clone()
    }

    /// 当前状态名（只用于日志/诊断：手势没反应时要能看出卡在哪一步）。
    pub fn state_label() -> &'static str {
        Self::current_state().as_str()
    }

    /// 开始一次会话。路径已由手势层冻结（真实起手文件 / 多选集合）。
    fn begin(app: &AppHandle, paths: Vec<String>) {
        // 转换进行中忽略新唤出，避免重复提交/二次转换（§3.4）。
        if matches!(Self::current_state(), State::Converting) {
            return;
        }

        let handle = session();
        let generation = {
            let mut s = handle.lock_ok();
            s.generation += 1;
            s.state = State::Capturing;
            s.paths.clear();
            s.profile.clear();
            s.generation
        };
        tracing::info!(generation, count = paths.len(), "环形会话开始");

        // 记住「唤出环形盘前」的前台 App（Finder），结束后归还焦点。
        crate::window::remember_frontmost_app();

        let app = app.clone();
        std::thread::spawn(move || probe_and_ready(&app, generation, paths));
    }

    /// 用户在环形盘选中一个可用扇区后提交：固定输入 → 回焦 Finder → 后台执行。
    ///
    /// 执行只用当次冻结的路径快照与预设副本（§3.4），配置后续变更、Finder 选中
    /// 变化都不能污染本次任务。这里**不再**读 Finder 复核：单手势产品要求一次
    /// 拖动内完成，任何「刷新会话后再拖一次」都是失败。
    pub fn execute(action_id: String) {
        let (paths, generation) = {
            let handle = session();
            let mut s = handle.lock_ok();
            if !matches!(s.state, State::Ready) {
                return;
            }
            s.state = State::Converting;
            (s.paths.clone(), s.generation)
        };

        // 原地进度：不立即关窗，保留环形盘显示进度环与成功/失败反馈（§4.3）。
        let Some(app) = Self::app() else {
            return;
        };
        let config = current_config();
        // 第二道闸：禁用扇区 / 不属于当前 Profile 的动作一律不启动 Job。
        // 前端已经拦过一次，这里是防止旧窗口事件或并发状态把禁用项当成可用项。
        let slice = action_slice(&config, &paths, &action_id);
        let slice = match slice {
            Some(slice) if slice.enabled => slice,
            other => {
                let reason = other
                    .and_then(|s| s.disabled_reason)
                    .unwrap_or_else(|| "这个动作在当前文件上不可用".to_string());
                tracing::warn!(%action_id, %reason, "环形：动作不可执行，未启动 Job");
                abort_execute(&app, generation, format!("没有转换：{reason}"));
                return;
            }
        };
        tracing::info!(
            generation,
            %action_id,
            convert = slice.convert,
            skip = slice.skip,
            "环形：开始转换"
        );
        // 动作 id 是预设 id：配方（目标 / 质量 / 输出规则）在预设库里。这里先取出
        // 一份快照，Job 全程用它，之后改配置不会影响已经开始的转换（§2.2）。
        let Some(preset) = find_or_legacy(&config.conversion.presets, &action_id) else {
            tracing::warn!(%action_id, "环形：动作找不到对应预设");
            abort_execute(
                &app,
                generation,
                format!("动作 {action_id} 已不存在，请在设置里重新配置圆环"),
            );
            return;
        };
        // 5s 交接看门狗：转得慢就把环形盘收缩交给系统通知，不再挡在屏幕中央
        // （Filee §1.4 #10）；转换本身继续跑，不因收起而中断。
        let watchdog = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(HANDOFF_AFTER);
            let still_converting = session()
                .lock()
                .map(|s| s.generation == generation && matches!(s.state, State::Converting))
                .unwrap_or(false);
            if still_converting {
                let _ = watchdog.emit(RING_HANDOFF, generation);
                RingManager::dismiss();
            }
        });
        std::thread::spawn(move || {
            let outcome = run_conversion(&app, generation, &paths, &preset);
            // 结束即回 Idle，允许下一次唤出（修复：转换后会话卡在 Converting）。
            {
                let handle = session();
                let mut s = handle.lock_ok();
                if s.generation == generation {
                    s.state = State::Idle;
                    s.paths.clear();
                    s.profile.clear();
                }
            }
            let _ = app.emit(
                RING_RESULT,
                RingResultEvent {
                    generation,
                    converted: outcome.converted,
                    skipped: outcome.skipped,
                    failed: outcome.failed,
                    cancelled: outcome.cancelled,
                    errors: outcome.errors.clone(),
                    warnings: outcome.warnings.clone(),
                    output: outcome.outputs.first().cloned(),
                },
            );
            // 轻量系统通知兜底（webview 动画失败/窗口已关时仍能告知结果）。
            notify_result(&outcome);
            // 注意：这里**不能**马上把焦点还给 Finder。回焦会让环形盘先失焦，
            // 而失焦就是「取消」，原地进度/成功反馈会被当场抹掉。焦点统一在
            // 窗口收起时归还（`hide_ring_on_main` 已经在隐藏后调了回焦）。
        });
    }

    /// 取消：隐藏环形盘并回到 Idle。
    ///
    /// 来源：松手落在中心 / 环外 / 禁用扇区后前端主动放弃、窗口失焦、手势期间
    /// Esc，以及转换中的内部取消。手势期间的 Esc 走这里（`option_tap` 调用），
    /// 同时发一份 idle 快照把前端盘面复位——否则之后的松手事件还会按上一次
    /// hover 提交到一张已隐藏的盘上。
    pub fn cancel() {
        // 窗口已经不在屏上就不是「取消」：结果播放完的 dismiss() 也会让 AppKit
        // 补发一条 Focused(false)，不加这道闸会让日志误报、白白重做隐藏与回焦。
        if let Some(app) = Self::app() {
            let visible = app
                .get_webview_window("ring")
                .and_then(|w| w.is_visible().ok())
                .unwrap_or(false);
            if !visible {
                return;
            }
        }
        tracing::info!("环形会话取消（松手取消 / 失焦 / Esc）");
        {
            let handle = session();
            let mut s = handle.lock_ok();
            let converting = matches!(s.state, State::Converting);
            s.state = State::Idle;
            s.paths.clear();
            s.profile.clear();
            drop(s);
            // 锁外取消：Job 结束时也要拿这把锁回写状态，不能抱着锁等它。
            if converting && job::cancel_active() {
                tracing::info!("环形：已取消运行中的转换，未完成的输出会被清理");
            }
        }
        if let Some(app) = Self::app() {
            hide_ring_on_main(&app);
            // 前端复位：新会话开始前盘面必须是干净的 idle（applySnapshot 会清掉
            // 上一次的 hover / 定时器）。窗口隐藏后到达的 RING_DRAG_END 也会被
            // ready 门挡掉，不会再触发执行。
            let idle = RingSnapshot {
                generation: 0,
                state: State::Idle.as_str().to_string(),
                category: None,
                count: 0,
                hint: None,
                profile: None,
                actions: Vec::new(),
                suggested: None,
            };
            let _ = app.emit(RING_UPDATE, &idle);
        }
    }

    /// 结果动画播完后收起环形盘（前端在成功/失败反馈结束后调用）。
    /// 只隐藏窗口，不改会话——会话在转换结束时已回到 Idle。
    pub fn dismiss() {
        if let Some(app) = Self::app() {
            hide_ring_on_main(&app);
        }
    }
}

// FinderSelection 各失败分支的 hint 已在 `finder` 模块给出，这里直接复用。

impl Session {
    /// 取出构建快照所需的数据（避免长期持锁）。
    fn clone_snapshot_data(&self) -> (u64, State, Vec<String>) {
        (self.generation, self.state.clone(), self.paths.clone())
    }
}

/// 探测一批路径 → 选 Profile → 算切片 → 进入 Ready（无可用扇区时 Empty）。
fn probe_and_ready(app: &AppHandle, generation: u64, paths: Vec<String>) {
    {
        let handle = session();
        let mut s = handle.lock_ok();
        // 已被更新会话取代则放弃（§3.4：generation 丢弃上一会话迟到的结果）。
        if s.generation != generation {
            return;
        }
        s.state = State::Probing;
        s.paths = paths;
    }
    let config = current_config();
    let paths = session().lock().map(|s| s.paths.clone()).unwrap_or_default();
    if paths.is_empty() {
        // 手势层没拿到起手文件，也没有可用预取：直接说清楚，不要让盘空转。
        tracing::warn!(generation, "环形：拖动没有带来任何文件");
        fail_state_with(
            app,
            generation,
            State::Empty,
            Some("没有读到拖动的文件：请按住 Option 从文件图标上开始拖动。".to_string()),
        );
        return;
    }
    let mut snapshot = build_snapshot(generation, &State::Ready, &paths, &config);

    // 一个可执行扇区都没有 → 就地提示，不铺一张点了没反应的盘。
    // 有可用扇区时，不可用的那些仍留在原位（只在扇区上标原因）。
    let state = if snapshot.actions.iter().any(|a| a.enabled) {
        State::Ready
    } else {
        State::Empty
    };
    snapshot.state = state.as_str().to_string();

    {
        let handle = session();
        let mut s = handle.lock_ok();
        if s.generation != generation {
            return;
        }
        s.state = state;
        s.profile = snapshot.profile.as_ref().map(|p| p.id.clone()).unwrap_or_default();
    }

    tracing::info!(
        generation,
        count = snapshot.count,
        profile = snapshot.profile.as_ref().map(|p| p.id.as_str()).unwrap_or("-"),
        slices = snapshot.actions.len(),
        enabled = snapshot.actions.iter().filter(|a| a.enabled).count(),
        state = snapshot.state.as_str(),
        "环形盘就绪"
    );
    show_ring_on_main(app);
    let _ = app.emit(RING_UPDATE, &snapshot);
}

/// 失败/空状态：记录状态 + 显示环形盘（用于就地提示，不打开主窗口）。
///
/// 提示文案由调用方给：运行态只有「没拿到拖动的文件」这一种前置失败，
/// 其余（格式不支持 / 没有可用动作）都在 `build_snapshot` 里带原因生成。
fn fail_state_with(app: &AppHandle, generation: u64, state: State, hint: Option<String>) {
    {
        let handle = session();
        let mut s = handle.lock_ok();
        if s.generation != generation {
            return;
        }
        s.state = state.clone();
        s.paths.clear();
        s.profile.clear();
    }
    let snapshot = RingSnapshot {
        generation,
        state: state.as_str().to_string(),
        category: None,
        count: 0,
        hint,
        profile: None,
        actions: Vec::new(),
        suggested: None,
    };
    show_ring_on_main(app);
    let _ = app.emit(RING_UPDATE, &snapshot);
}

/// 提交被静默拦下（动作不可用 / 预设已删）：回 Idle 并给出具体原因。
///
/// 不创建 Job、不动任何文件；结果事件仍带 generation，旧盘不会覆盖新会话。
fn abort_execute(app: &AppHandle, generation: u64, message: String) {
    {
        let handle = session();
        let mut s = handle.lock_ok();
        if s.generation == generation {
            s.state = State::Idle;
            s.paths.clear();
            s.profile.clear();
        }
    }
    let _ = app.emit(
        RING_RESULT,
        RingResultEvent {
            generation,
            converted: 0,
            skipped: 0,
            failed: 1,
            cancelled: 0,
            errors: vec![message.clone()],
            warnings: Vec::new(),
            output: None,
        },
    );
    crate::core::services::notify("ClipBase 环形转换", &message);
}

/// 把环形盘显示投递到主线程。
///
/// AppKit 的窗口事务（show / setFrame / setLevel）必须在主线程，从后台线程
/// 直接调会撞 `NSWMWindowCoordinator` 的 “transaction must be valid” 断言
/// （进程直接 SIGTRAP）。探测线程与转换线程都不能自己动窗口。
fn show_ring_on_main(app: &AppHandle) {
    let Some(origin) = *DRAG_ORIGIN.lock_ok() else {
        tracing::warn!("环形盘没有起手点，跳过显示");
        return;
    };
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        crate::window::show_ring_window(&handle, origin);
    });
}

/// 把环形盘收起 + 回焦 Finder 投递到主线程（同上约束）。
fn hide_ring_on_main(app: &AppHandle) {
    // 会话结束：起手点作废，下一次拖动重新钉位。
    *DRAG_ORIGIN.lock_ok() = None;
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        crate::window::hide_ring_window(&handle);
        crate::window::restore_frontmost_app();
    });
}

/// 由路径 + 配置计算对外快照（类别、数量、选中 Profile、固定位置切片）。
fn build_snapshot(generation: u64, state: &State, paths: &[String], config: &Config) -> RingSnapshot {
    // 第一步：每个文件探格式（不认得的计入 skip，不能静默丢）。
    let per_file = detect_per_file(paths);
    let recognized = per_file.iter().filter(|f| f.is_some()).count();
    let image = per_file
        .iter()
        .flatten()
        .filter(|f| f.is_image())
        .count();
    let docs = per_file
        .iter()
        .flatten()
        .filter(|f| f.is_document())
        .count();
    let text = recognized - image - docs;
    let formats: Vec<ConvertFormat> = per_file.iter().flatten().copied().collect();

    // 中心提示：同一源格式显示「数量 + 格式名」，同类混合显示类别名，
    // 跨类才叫「混合文件」——避免选了 20 张 PNG 还只写「图片」。
    let category = if recognized == 0 {
        None
    } else if formats.iter().all(|f| Some(*f) == formats.first().copied()) {
        formats.first().map(|f| f.label().to_string())
    } else if text == 0 && docs == 0 {
        Some("图片".to_string())
    } else if image == 0 && docs == 0 {
        Some("文本".to_string())
    } else if image == 0 && text == 0 {
        Some("文档".to_string())
    } else {
        Some("混合文件".to_string())
    };

    // 第二步：整批扩展名 → 唯一一个 Profile（§2.2）。
    // 选盘与可用性是两个阶段：先拿到固定的 preset_ids 顺序，再逐个算能不能执行，
    // 绝不因为不可用就从数组里删——否则设置里的位置与运行时的落点就是两回事。
    let extensions = profile::extensions_of(paths);
    let selected = profile::select_profile(&config.ring.profiles, &extensions);
    let (profile_info, ids) = match selected {
        Some(p) => (
            Some(ProfileInfo {
                id: p.id.clone(),
                name: if p.name.trim().is_empty() {
                    p.id.clone()
                } else {
                    p.name.clone()
                },
                extensions: p.extensions.clone(),
                fallback: p.kind == RingProfileKind::MixedFallback,
            }),
            slice_ids(p, config),
        ),
        None => (None, Vec::new()),
    };
    let actions = build_actions(&config.conversion.presets, &per_file, paths, &ids);

    // 记住的最后动作只用于预高亮，不自动执行；只能高亮真正可用的扇区。
    let suggested = if image > 0 && text == 0 && docs == 0 {
        Some("图片")
    } else if text > 0 && image == 0 && docs == 0 {
        Some("文本")
    } else if docs > 0 && image == 0 && text == 0 {
        Some("文档")
    } else {
        None
    }
    .and_then(|kind| {
        config
            .ring
            .last_action
            .iter()
            .find(|(k, _)| k == kind)
            .map(|(_, v)| v.clone())
    })
    .filter(|id| actions.iter().any(|a| &a.id == id && a.enabled));

    // 空状态的原因不止一种，提示不能一刀切：「没选中」曾被错用在「选中了但没有
    // 可转目标」上（比如只选了一个 .txt），用户会以为自己的选中丢了。
    let hint = if recognized == 0 {
        if paths.is_empty() {
            Some("没有读到拖动的文件，按住 Option 从文件图标上开始拖动。".to_string())
        } else {
            Some(format!(
                "选中的 {} 个文件都不是 ClipBase 支持的格式（图片 / 文档 / Markdown / HTML）。",
                paths.len()
            ))
        }
    } else if actions.is_empty() {
        Some(match profile_info.as_ref() {
            Some(p) => format!(
                "「{}」还没有圆环动作：到 设置 → 环形转换 里给它添加预设。",
                p.name
            ),
            None => "还没有可用的文件类型 Profile：到 设置 → 环形转换 里新建一个。".to_string(),
        })
    } else if !actions.iter().any(|a| a.enabled) {
        // 原因直接取可用性计算的那一条：比如「已经是 TXT，这个预设不需要重新编码」
        // 或「文本 → 图片 需要 OCR 引擎」，比笼统的「没有可用动作」有用。
        let reason = actions
            .iter()
            .find_map(|a| a.disabled_reason.clone())
            .unwrap_or_else(|| "这些预设都不适用于当前文件".to_string());
        Some(format!("选中的文件没有可执行的动作：{reason}。"))
    } else {
        None
    };

    RingSnapshot {
        generation,
        state: state.as_str().to_string(),
        category,
        count: recognized,
        hint,
        profile: profile_info,
        actions,
        suggested,
    }
}

/// 一张盘的完整扇区 id：Profile 顺序 + （开关打开时）工具动作固定顺序。
///
/// 工具动作按 id 去重：开关关着时用户可能手动把某条工具预设放进过这张盘，
/// 再打开开关不能出现两个「打包 ZIP」扇区。
fn slice_ids(profile: &RingProfile, config: &Config) -> Vec<String> {
    let mut ids = profile.preset_ids.clone();
    if config.ring.utility_actions {
        for id in preset::ACTION_BUILTIN_IDS {
            if !ids.iter().any(|existing| existing == id) {
                ids.push(id.to_string());
            }
        }
    }
    ids
}

/// 逐个文件探测格式（与 `Job` 同一份映射）。
fn detect_per_file(paths: &[String]) -> Vec<Option<ConvertFormat>> {
    paths
        .iter()
        .map(|path| convert::detect_format(path))
        .collect()
}

/// 按 Profile 的固定顺序构建扇区：不可用的**保留位置**并带原因（§2.3）。
///
/// 格式预设的可用性走的是和 Job 同一套 `preset_availability`；打包 / 解压 /
/// PDF 分页等工具动作按载荷本身判定（`action::availability`）。两条路径都和
/// 执行端共用，不会再出现「扇区能点、执行时却无路由」的偏差。
fn build_actions(
    presets: &[ConversionPreset],
    per_file: &[Option<ConvertFormat>],
    paths: &[String],
    ids: &[String],
) -> Vec<RingAction> {
    ids.iter()
        .filter_map(|id| {
            // 预设被删掉了：这一条无法渲染标签，只能不显示；
            // 迁移（`Config::migrate`）会保证配置里没有悬空 id。
            let preset = presets.iter().find(|p| &p.id == id)?;
            let (convert, skip, reason) = if preset.kind.is_convert() {
                job::preset_availability(preset, per_file)
            } else {
                action::availability(preset, paths)
            };
            Some(RingAction {
                id: preset.id.clone(),
                label: preset.ring_label(),
                target: match preset.target {
                    crate::core::convert::preset::TargetSpec::Explicit(f) => {
                        Some(f.as_str().to_string())
                    }
                    crate::core::convert::preset::TargetSpec::SameAsSource => None,
                },
                convert,
                skip,
                enabled: convert > 0,
                disabled_reason: if convert > 0 {
                    None
                } else {
                    Some(
                        reason
                            .filter(|r| !r.trim().is_empty())
                            .unwrap_or_else(|| "这批文件里没有能用到它的输入".to_string()),
                    )
                },
            })
        })
        .collect()
}

/// 当前批次里某个动作的切片（`execute` 的第二道闸）。
///
/// 重新按当前配置与当前批次算一次：旧窗口事件、并发状态都不能把禁用项
/// 当成可用项。工具动作也在 `slice_ids` 里，因此同样受这道闸约束。
fn action_slice(config: &Config, paths: &[String], action_id: &str) -> Option<RingAction> {
    let per_file = detect_per_file(paths);
    let extensions = profile::extensions_of(paths);
    let profile = profile::select_profile(&config.ring.profiles, &extensions)?;
    let ids = slice_ids(profile, config);
    build_actions(&config.conversion.presets, &per_file, paths, &ids)
        .into_iter()
        .find(|a| a.id == action_id)
}

/// 执行一份预设，返回汇总（供通知，任务 18/19/25/26/27）。
#[derive(Default)]
struct RunOutcome {
    converted: usize,
    skipped: usize,
    failed: usize,
    cancelled: usize,
    outputs: Vec<String>,
    errors: Vec<String>,
    warnings: Vec<String>,
}

/// 跑一份预设。
///
/// 逐文件是否可转、为什么跳过、失败原因全部由 Job 给出（§3）：这一层不再做
/// 「交集过滤」，也就不存在以前那种 `_ => {}` 静默丢文件。
fn run_conversion(
    app: &AppHandle,
    generation: u64,
    paths: &[String],
    preset: &ConversionPreset,
) -> RunOutcome {
    let mut on_update = |_file: &ConversionFileResult, done: usize, total: usize| {
        let _ = app.emit(
            RING_PROGRESS,
            RingProgressEvent {
                generation,
                done: done.min(total),
                total,
            },
        );
    };
    let job = job::run_with_cancel(paths, preset, CancelToken::new(), &mut on_update);

    let errors: Vec<String> = job
        .files
        .iter()
        .filter_map(|f| f.error.as_ref().map(|e| e.message.clone()))
        .collect();
    let warnings: Vec<String> = job
        .files
        .iter()
        .flat_map(|f| f.warnings.iter().cloned())
        .collect();

    // 记住本类别最后动作（任务 30）：仅按预设 id 记，不触发额外转换。
    // 跨类混选时不写，避免把图片的动作预高亮到文本上。
    let mut images = 0usize;
    let mut texts = 0usize;
    let mut docs = 0usize;
    for path in paths {
        match convert::detect_format(path) {
            Some(f) if f.is_image() => images += 1,
            Some(f) if f.is_document() => docs += 1,
            Some(_) => texts += 1,
            None => {}
        }
    }
    let kind = match (images, texts, docs) {
        (n, 0, 0) if n > 0 => Some("图片"),
        (0, n, 0) if n > 0 => Some("文本"),
        (0, 0, n) if n > 0 => Some("文档"),
        _ => None,
    };
    if let Some(kind) = kind {
        remember_last_action(&preset.id, kind);
    }

    RunOutcome {
        converted: job.converted(),
        skipped: job.skipped(),
        failed: job.failed(),
        cancelled: job.cancelled(),
        outputs: job.outputs(),
        errors,
        warnings,
    }
}

fn remember_last_action(preset_id: &str, category: &str) {
    let mut config = current_config();
    config.ring.last_action.retain(|(c, _)| c != category);
    config
        .ring
        .last_action
        .push((category.to_string(), preset_id.to_string()));
    if let Some(app) = RingManager::app() {
        if let Some(state) = app.try_state::<crate::state::AppState>() {
            let _ = state
                .config
                .patch(serde_json::json!({ "ring": config.ring }));
        }
    }
}

/// 轻量反馈：系统通知给出 转换/跳过/失败 数量（评审 E4 / 任务 26）。
fn notify_result(outcome: &RunOutcome) {
    let summary = if outcome.cancelled > 0 {
        format!(
            "已取消 · 完成 {} · 跳过 {} · 失败 {}",
            outcome.converted, outcome.skipped, outcome.failed
        )
    } else {
        format!(
            "已转换 {} · 跳过 {} · 失败 {}",
            outcome.converted, outcome.skipped, outcome.failed
        )
    };
    let detail = if outcome.converted == 1 && outcome.failed == 0 {
        outcome
            .outputs
            .first()
            .map(|p| p.rsplit('/').next().unwrap_or(p.as_str()).to_string())
            .unwrap_or_default()
    } else {
        // 没有错误时优先暴露「转成功了但不保真」的警告（§5：不能静默丢失）。
        outcome
            .errors
            .first()
            .or_else(|| outcome.warnings.first())
            .cloned()
            .unwrap_or_default()
    };
    let message = if detail.is_empty() {
        summary
    } else {
        format!("{summary}\n{detail}")
    };
    crate::core::services::notify("ClipBase 环形转换", &message);
}

/// 读取当前配置（无 AppHandle 时的兜底：直接用状态里的 ConfigService）。
fn current_config() -> Config {
    if let Some(app) = RingManager::app() {
        if let Some(state) = app.try_state::<crate::state::AppState>() {
            return state.config.get();
        }
    }
    Config::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(actions: &[RingAction]) -> Vec<&str> {
        actions.iter().map(|a| a.id.as_str()).collect()
    }

    fn enabled_ids(actions: &[RingAction]) -> Vec<&str> {
        actions.iter().filter(|a| a.enabled).map(|a| a.id.as_str()).collect()
    }

    fn ready_config() -> Config {
        let mut config = Config::default();
        config.migrate();
        config
    }

    /// 旧安装版的混杂布局（图片 + 文本目标同盘）。
    fn old_mixed_layout() -> Vec<String> {
        vec!["img.jpg", "img.png", "doc.html", "doc.txt"]
            .into_iter()
            .map(str::to_string)
            .collect()
    }

    /// 位置固定：选 .txt 时图片预设仍然占着原位置，只是禁用并带原因。
    #[test]
    fn txt_keeps_image_slices_but_disables_them() {
        let presets = preset::builtin_presets();
        let actions = build_actions(&presets, &[Some(ConvertFormat::Txt)], &[], &old_mixed_layout());

        assert_eq!(ids(&actions), vec!["img.jpg", "img.png", "doc.html", "doc.txt"]);
        assert_eq!(enabled_ids(&actions), vec!["doc.html"]);
        for action in actions.iter().filter(|a| !a.enabled) {
            assert!(action.disabled_reason.is_some(), "{} 缺少禁用原因", action.id);
        }
        assert_eq!(actions[2].target.as_deref(), Some("html"));
    }

    /// 已经是目标格式的同格式预设：禁用但仍在原位置。
    #[test]
    fn png_disables_identity_preset_in_place() {
        let presets = preset::builtin_presets();
        let actions = build_actions(&presets, &[Some(ConvertFormat::Png)], &[], &old_mixed_layout());

        assert_eq!(ids(&actions)[1], "img.png");
        assert!(!actions[1].enabled, "同格式且未声明重编码，应禁用");
        assert!(actions[1].disabled_reason.is_some());
        assert!(actions[0].enabled, "PNG → JPG 应可用");
    }

    /// 混合选择时：有路由的文件计入 convert，不认识的文件计入 skip。
    #[test]
    fn mixed_selection_counts_skips() {
        let presets = preset::builtin_presets();
        let per_file = [Some(ConvertFormat::Png), None];
        let actions = build_actions(&presets, &per_file, &[], &["img.jpg".to_string()]);

        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].convert, 1);
        assert_eq!(actions[0].skip, 1);
        assert!(actions[0].enabled);
    }

    /// 预设被删掉时扇区就消失，而不是留一个点了没反应的坑。
    #[test]
    fn missing_preset_drops_its_slice() {
        let presets = preset::builtin_presets();
        let actions = build_actions(&presets, &[Some(ConvertFormat::Png)], &[], &["preset-gone".into()]);
        assert!(actions.is_empty());
    }

    /// 一批 .png → 选到「图片」Profile，扇区就是它的完整正面（工具动作关）。
    #[test]
    fn snapshot_picks_image_profile_for_png() {
        let mut config = ready_config();
        config.ring.utility_actions = false;
        let snapshot = build_snapshot(1, &State::Ready, &["/tmp/a.png".to_string()], &config);

        assert_eq!(snapshot.profile.as_ref().map(|p| p.id.as_str()), Some("images"));
        assert_eq!(snapshot.category.as_deref(), Some("PNG"));
        assert_eq!(ids(&snapshot.actions), preset::DEFAULT_IMAGES.to_vec());
        assert!(enabled_ids(&snapshot.actions).contains(&"img.jpg"));
    }

    /// 文档格式走「文档」Profile（docx 不再落到 catch-all）。
    #[test]
    fn snapshot_picks_docs_profile_for_document_format() {
        let mut config = ready_config();
        config.ring.utility_actions = false;
        let snapshot = build_snapshot(1, &State::Ready, &["/tmp/a.docx".to_string()], &config);

        assert_eq!(snapshot.profile.as_ref().map(|p| p.id.as_str()), Some("docs"));
        assert_eq!(snapshot.category.as_deref(), Some("DOCX"));
        assert_eq!(ids(&snapshot.actions), preset::DEFAULT_DOCS.to_vec());
        assert!(enabled_ids(&snapshot.actions).contains(&"doc.txt"));
    }

    /// 图片 + 文本混选 → 没有普通 Profile 能覆盖全部扩展名，落到 catch-all。
    #[test]
    fn snapshot_picks_catch_all_for_cross_family_batch() {
        let config = ready_config();
        let paths = vec!["/tmp/a.png".to_string(), "/tmp/b.txt".to_string()];
        let snapshot = build_snapshot(1, &State::Ready, &paths, &config);

        assert_eq!(snapshot.profile.as_ref().map(|p| p.id.as_str()), Some("mixed"));
        assert!(snapshot.profile.as_ref().map(|p| p.fallback).unwrap_or(false));
        assert_eq!(snapshot.count, 2);
        // 混合盘同时含图片与文本预设，文本目标对图片禁用但不重排。
        assert!(snapshot.actions.iter().any(|a| a.id == "doc.html" && a.enabled));
    }

    /// 未知扩展名 → catch-all，不误入任何普通 Profile。
    #[test]
    fn snapshot_falls_back_for_unknown_extension() {
        let config = ready_config();
        let snapshot = build_snapshot(1, &State::Ready, &["/tmp/a.xyz".to_string()], &config);

        assert_eq!(snapshot.profile.as_ref().map(|p| p.id.as_str()), Some("mixed"));
        assert_eq!(snapshot.count, 0);
        assert!(snapshot.hint.is_some());
    }

    /// 工具动作固定追加在正面末尾；关闭开关后整组消失。
    #[test]
    fn utility_actions_appended_last_and_toggleable() {
        let config = ready_config();
        let snapshot = build_snapshot(1, &State::Ready, &["/tmp/a.png".to_string()], &config);
        let n = preset::ACTION_BUILTIN_IDS.len();
        let tail: Vec<&str> = snapshot
            .actions
            .iter()
            .rev()
            .take(n)
            .rev()
            .map(|a| a.id.as_str())
            .collect();
        assert_eq!(tail, preset::ACTION_BUILTIN_IDS.to_vec());

        // PNG 能打包；没有压缩包输入 → 解压占位但禁用且带原因。
        let zip = snapshot.actions.iter().find(|a| a.id == "act.pack.zip").expect("打包扇区");
        assert!(zip.enabled);
        let extract = snapshot
            .actions
            .iter()
            .find(|a| a.id == "act.extract")
            .expect("解压扇区");
        assert!(!extract.enabled);
        assert!(extract.disabled_reason.is_some());

        let mut off = ready_config();
        off.ring.utility_actions = false;
        let plain = build_snapshot(1, &State::Ready, &["/tmp/a.png".to_string()], &off);
        assert!(plain.actions.iter().all(|a| !a.id.starts_with("act.")));
    }
}
