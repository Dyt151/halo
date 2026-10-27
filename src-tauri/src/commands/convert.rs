//! 文件格式转换命令：预设 → Job（docs/REA-Filee-Conversion.md §8）。
//!
//! 命令层只做三件事：取预设快照、跑 Job、把逐文件进度转成事件。真正的格式
//! 探测、路由规划、输出分配与回收全在 `crate::core::convert`（纯函数 + 单测）。
//!
//! 注意：**动作身份是预设 id**，不再是目标格式名。两个 JPG 预设可以有不同质量，
//! 同格式重编码也由预设显式声明，命令层不再接受 `jpeg_quality` / `dest_dir`。

use tauri::{Emitter, State};

use crate::core::convert::job::{self, ConversionJob};
use crate::core::convert::preset::find_or_legacy;
use crate::error::{AppError, CommandResult};
use crate::state::AppState;

/// 单文件进度事件（`convert_run` 执行期间推送）。
pub const CONVERT_PROGRESS: &str = "convert://progress";

/// 按预设批量转换，阻塞到全部文件出结果后返回完整 Job。
///
/// **必须 async + `spawn_blocking`**：Tauri 的同步命令跑在主线程上，一次批量转换
/// 可能持续数分钟，主线程被占着时 webview 不重绘、连进度事件都投递不进去，
/// 用户看到的是「点了没反应、按 Esc 也取消不了」。
///
/// 每个输入都一定有可解释的结果（converted / skipped / failed / cancelled），
/// 前端不需要自己再猜「为什么这个文件没出现」。
#[tauri::command]
pub async fn convert_run(
    state: State<'_, AppState>,
    paths: Vec<String>,
    preset_id: String,
) -> CommandResult<ConversionJob> {
    // 空批次不能走到 Job 里：它会返回一个「0 个文件、状态 Completed」的空任务，
    // 前端只能展示一句含糊的「转换完成」。
    let paths: Vec<String> = paths
        .into_iter()
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect();
    if paths.is_empty() {
        return Err(AppError::new(9001, "没有选择要转换的文件"));
    }

    let config = state.config.get();
    let preset = find_or_legacy(&config.conversion.presets, &preset_id)
        .ok_or_else(|| AppError::new(9001, format!("未知的转换预设：{preset_id}")))?;

    // 同一时刻只跑一个 Job：否则后提交的任务会盖掉前一个的取消令牌，Esc 就失效了。
    if job::active_job_id().is_some() {
        return Err(AppError::new(
            9004,
            "已有转换任务在进行，请等它结束或取消后再试".to_string(),
        ));
    }

    let app = state.app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        job::run(&paths, &preset, &mut |file, done, total| {
            let _ = app.emit(
                CONVERT_PROGRESS,
                serde_json::json!({
                    "source": file.source,
                    "state": file.state,
                    "progress": file.progress,
                    "done": done,
                    "total": total,
                }),
            );
        })
    })
    .await
    .map_err(|e| AppError::with_details(9005, "转换任务意外终止", e.to_string()))?;
    Ok(result)
}

/// 取消正在运行的转换（返回是否有任务真的被取消）。
///
/// 取消不是「假装没发生」：已完成文件保留，正在写的临时文件被丢弃，
/// 该文件拥有的未完成输出一并清理（§3）。
#[tauri::command]
pub fn convert_cancel() -> bool {
    job::cancel_active()
}

/// 在系统文件管理器中显示文件（macOS：Finder 中选中；其他平台：打开所在目录）。
#[tauri::command]
pub fn reveal_path(path: String) -> CommandResult<()> {
    let raw = path.trim();
    if raw.is_empty() {
        return Err(AppError::new(9002, "没有可以显示的文件"));
    }
    let target = std::path::PathBuf::from(raw);
    if !target.exists() {
        return Err(AppError::new(9002, format!("文件不存在：{raw}")));
    }
    // 归一成绝对路径再交给 `open`：直接传 "-x" 这种相对名会被当成命令行开关
    // （`open -R -x` 是语法错误，用户只会看到一句莫名其妙的失败）。
    let target = std::fs::canonicalize(&target).unwrap_or(target);

    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg("-R").arg(&target).status();

    #[cfg(target_os = "windows")]
    let result = std::process::Command::new("explorer")
        .arg(format!(
            "/select,{}",
            target.to_string_lossy().replace('/', "\\")
        ))
        .status();

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    let result = std::process::Command::new("xdg-open")
        .arg(target.parent().unwrap_or(&target))
        .status();

    match result {
        Ok(status) if status.success() => Ok(()),
        Ok(_) => Err(AppError::new(9003, "打开文件管理器失败".to_string())),
        Err(e) => Err(AppError::internal(e)),
    }
}
