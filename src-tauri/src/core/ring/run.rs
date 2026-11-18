//! 转换执行与结果反馈。

use super::*;

/// 执行一份预设，返回汇总（供通知，任务 18/19/25/26/27）。
#[derive(Default)]
pub(super) struct RunOutcome {
    pub(super) converted: usize,
    pub(super) skipped: usize,
    pub(super) failed: usize,
    pub(super) cancelled: usize,
    pub(super) outputs: Vec<String>,
    pub(super) errors: Vec<String>,
    pub(super) warnings: Vec<String>,
}

/// 跑一份预设。
///
/// 逐文件是否可转、为什么跳过、失败原因全部由 Job 给出（Filee 行为）：这一层不再做
/// 「交集过滤」，也就不存在以前那种 `_ => {}` 静默丢文件。
pub(super) fn run_conversion(
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
pub(super) fn notify_result(outcome: &RunOutcome) {
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
        // 没有错误时优先暴露「转成功了但不保真」的警告（Filee 行为：不能静默丢失）。
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
    crate::core::services::notify("Halo 环形转换", &message);
}

/// 读取当前配置（无 AppHandle 时的兜底：直接用状态里的 ConfigService）。
pub(super) fn current_config() -> Config {
    if let Some(app) = RingManager::app() {
        if let Some(state) = app.try_state::<crate::state::AppState>() {
            return state.config.get();
        }
    }
    Config::default()
}
