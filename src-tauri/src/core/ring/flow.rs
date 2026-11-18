//! 会话生命周期：开始探测、执行、取消与环形窗显示。

use super::plan::{action_slice, build_snapshot};
use super::run::{current_config, notify_result, run_conversion};
use super::session::session;
use super::*;

impl RingManager {
    pub(super) fn begin(app: &AppHandle, paths: Vec<String>) {
        // 转换进行中忽略新唤出，避免重复提交/二次转换（Filee 行为）。
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
    /// 执行只用当次冻结的路径快照与预设副本（Filee 行为），配置后续变更、Finder 选中
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

        // 原地进度：不立即关窗，保留环形盘显示进度环与成功/失败反馈（Filee 行为）。
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
        // 一份快照，Job 全程用它，之后改配置不会影响已经开始的转换（Filee 行为）。
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
        // （Filee 行为约定）；转换本身继续跑，不因收起而中断。
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

/// 探测一批路径 → 选 Profile → 算切片 → 进入 Ready（无可用扇区时 Empty）。
fn probe_and_ready(app: &AppHandle, generation: u64, paths: Vec<String>) {
    {
        let handle = session();
        let mut s = handle.lock_ok();
        // 已被更新会话取代则放弃（Filee 行为：generation 丢弃上一会话迟到的结果）。
        if s.generation != generation {
            return;
        }
        s.state = State::Probing;
        s.paths = paths;
    }
    let config = current_config();
    let paths = session()
        .lock()
        .map(|s| s.paths.clone())
        .unwrap_or_default();
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
    // 快照里已经只含可用扇区，这里的判断与过滤后的盘面一致。
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
        s.profile = snapshot
            .profile
            .as_ref()
            .map(|p| p.id.clone())
            .unwrap_or_default();
    }

    tracing::info!(
        generation,
        count = snapshot.count,
        profile = snapshot
            .profile
            .as_ref()
            .map(|p| p.id.as_str())
            .unwrap_or("-"),
        slices = snapshot.actions.len(),
        enabled = snapshot.actions.iter().filter(|a| a.enabled).count(),
        state = snapshot.state.as_str(),
        "环形盘就绪"
    );
    show_ring_on_main_with(app, snapshot);
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
    show_ring_on_main_with(app, snapshot);
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
    crate::core::services::notify("Halo 环形转换", &message);
}

/// 把环形盘显示投递到主线程，并在**显示之后**再推快照。
///
/// AppKit 的窗口事务（show / setFrame / setLevel）必须在主线程，从后台线程
/// 直接调会撞 `NSWMWindowCoordinator` 的 “transaction must be valid” 断言
/// （进程直接 SIGTRAP）。探测线程与转换线程都不能自己动窗口。
///
/// 快照必须跟在 show 后面：隐藏中的 WKWebView 会把入场动画（`opacity:0` +
/// `fill-mode:both`）停在首帧，窗口显示后也不恢复——盘面就成了「看不见但点得到」，
/// 松手照样按坐标命中扇区、文件照样被转走。之前在 show 之后单独 emit 是异步的，
/// 事件可能先于窗口显示落地，这条竞态实机表现为「按了 Option 没看到环」。
fn show_ring_on_main_with(app: &AppHandle, update: RingSnapshot) {
    let Some(origin) = *DRAG_ORIGIN.lock_ok() else {
        tracing::warn!("环形盘没有起手点，跳过显示");
        return;
    };
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        crate::window::show_ring_window(&handle, origin);
        let _ = handle.emit(RING_UPDATE, &update);
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
