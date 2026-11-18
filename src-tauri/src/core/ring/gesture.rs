//! 手势入口与只读状态查询（Option+拖动、指针、快照）。

use super::plan::build_snapshot;
use super::run::current_config;
use super::session::{now_ms, session};
use super::*;

impl RingManager {
    /// 启动时登记 AppHandle（在 `setup` 内调用一次）。
    pub fn set_app(app: AppHandle) {
        let _ = APP.set(app);
    }

    /// 当前 AppHandle（服务回调路径用）。
    pub(super) fn app() -> Option<AppHandle> {
        APP.get().cloned()
    }

    /// 起手文件：左键按下时指针下的 Finder 文件或文件夹（真实拖放载荷的权威部分）。
    ///
    /// 必须在 `leftMouseDown` 的回调里同步调用：Finder 一进事件追踪循环就不再
    /// 响应 AX，读会阻塞到松手（实测 4.9s），整个拖动拿不到载荷。
    ///
    /// 返回 `false` 表示可以**断言**这不是文件拖动（点在空白 / 工具栏上）：
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

    /// 本次拖动的真实载荷（Filee 行为）。
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
        tracing::info!(x, y, count = paths.len(), "Option+拖动：环形盘钉在起手点");
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

    pub(super) fn current_state() -> State {
        session().lock_ok().state.clone()
    }

    /// 当前状态名（只用于日志/诊断：手势没反应时要能看出卡在哪一步）。
    pub fn state_label() -> &'static str {
        Self::current_state().as_str()
    }
}
