//! 本地备份 / 导出 / 迁移。
//!
//! 导出的「包」是一个目录：
//! ```text
//! halo-export-<ms>/
//!   backup.json   全部历史（ClipItem 全字段）+ 配置
//!   files/        内容寻址的图片（include_files=true 时）
//!   icons/        来源 App 图标（尽力复制）
//! ```
//! 图片按内容寻址命名（`clip_<sha16>.png`），同名即同图，复制天然去重。
//! 导入时把图片路径重写回本机 `files_dir`，逐条 `insert_with_time` 保留原始时间戳，
//! 因此「迁移到新 Mac」= 把包拷过去再导入即可。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::core::clipboard::types::{ClipContent, ClipItem};
use crate::core::history::HistoryService;
use crate::error::{AppError, CommandResult};
use crate::state::AppState;
use crate::storage::files_dir;

/// 导出包格式版本（向前兼容：高于此值的包拒绝导入）。
const EXPORT_VERSION: u32 = 1;

/// 导出包内的备份文件（key 名 = `backup.json`）。
#[derive(Debug, Serialize, Deserialize)]
struct BackupFile {
    version: u32,
    exported_at: i64,
    app_version: String,
    clips: Vec<ClipItem>,
    config: serde_json::Value,
}

/// 导出结果。
#[derive(Debug, Serialize)]
pub struct ExportResult {
    /// 导出的包目录绝对路径。
    pub path: String,
    /// 导出的历史条数。
    pub clips: usize,
    /// 随包复制的图片文件数。
    pub files_copied: usize,
    /// 随包复制的图片总字节数。
    pub bytes: u64,
}

/// 导出入参。
#[derive(Debug, Deserialize)]
pub struct ExportInput {
    /// 目标目录（包会建在这个目录之下）。
    pub dest_dir: String,
    /// 是否把图片/图标一并复制进包（默认 true）。
    #[serde(default = "default_true")]
    pub include_files: bool,
}

/// 导入结果（与 [`crate::import::ImportSummary`] 字段对齐，便于复用展示）。
#[derive(Debug, Serialize, Default)]
pub struct ImportSummary {
    /// 成功写入的历史条数。
    pub clips: usize,
    /// 与既有记录合并（去重/按时间原位更新）的条数。
    #[serde(default)]
    pub merged: usize,
    /// 失败/跳过的条数。
    pub skipped: usize,
    /// 失败明细。
    pub errors: Vec<String>,
}

/// 导入入参。
#[derive(Debug, Deserialize)]
pub struct ImportInput {
    /// 包目录，或包内的 `backup.json` 文件绝对路径。
    pub path: String,
    /// 是否把包里的配置也恢复回本机（默认 false，只恢复历史）。
    #[serde(default)]
    pub restore_config: bool,
}

fn default_true() -> bool {
    true
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// 导出整库为可迁移包。
#[tauri::command]
pub fn export_data(input: ExportInput, state: State<'_, AppState>) -> CommandResult<ExportResult> {
    let dest = Path::new(&input.dest_dir);
    if !dest.exists() || !dest.is_dir() {
        return Err(AppError::with_details(
            2003,
            "导出目录不存在或不是文件夹",
            input.dest_dir,
        ));
    }

    let clips = state.history.all()?;
    let config = serde_json::to_value(state.config.get())
        .map_err(|e| AppError::with_details(2003, "序列化配置失败", e.to_string()))?;

    let bundle = dest.join(format!("halo-export-{}", now_ms()));
    std::fs::create_dir_all(&bundle)
        .map_err(|e| AppError::with_details(2003, "创建导出目录失败", e.to_string()))?;

    let mut files_copied = 0usize;
    let mut bytes = 0u64;
    if input.include_files {
        let files_dst = bundle.join("files");
        std::fs::create_dir_all(&files_dst)
            .map_err(|e| AppError::with_details(2003, "创建图片目录失败", e.to_string()))?;
        let icons_dst = bundle.join("icons");
        std::fs::create_dir_all(&icons_dst)
            .map_err(|e| AppError::with_details(2003, "创建图标目录失败", e.to_string()))?;

        for clip in &clips {
            // 图片：复制内容寻址文件到包的 files/（同名跳过，自动去重）。
            if let ClipContent::Image { path, .. } = &clip.content {
                if let Some(fname) = Path::new(path).file_name() {
                    let src = Path::new(path);
                    let dst = files_dst.join(fname);
                    if src.exists() && !dst.exists() {
                        if let Ok(n) = std::fs::copy(src, &dst) {
                            files_copied += 1;
                            bytes += n;
                        }
                    }
                }
            }
            // 图标：尽力复制，失败不阻断导出。
            if let Some(icon) = &clip.source_app_icon {
                if let Some(fname) = Path::new(icon).file_name() {
                    let src = Path::new(icon);
                    let dst = icons_dst.join(fname);
                    if src.exists() && !dst.exists() {
                        let _ = std::fs::copy(src, &dst);
                    }
                }
            }
        }
    }

    let file = BackupFile {
        version: EXPORT_VERSION,
        exported_at: now_ms(),
        app_version: app_version(),
        clips,
        config,
    };
    let json = serde_json::to_string_pretty(&file)
        .map_err(|e| AppError::with_details(2003, "序列化导出数据失败", e.to_string()))?;
    let backup_path = bundle.join("backup.json");
    std::fs::write(&backup_path, json)
        .map_err(|e| AppError::with_details(2003, "写入备份文件失败", e.to_string()))?;

    Ok(ExportResult {
        path: bundle.to_string_lossy().into_owned(),
        clips: file.clips.len(),
        files_copied,
        bytes,
    })
}

/// 从导出包恢复历史（可选恢复配置）。
#[tauri::command]
pub fn import_data(input: ImportInput, state: State<'_, AppState>) -> CommandResult<ImportSummary> {
    let path = Path::new(&input.path);
    // 入参可以是包目录，也可以是包内的 backup.json。
    let backup_json = if path.is_dir() {
        path.join("backup.json")
    } else {
        path.to_path_buf()
    };
    if !backup_json.exists() {
        return Err(AppError::with_details(
            2004,
            "找不到备份文件（应为包目录或 backup.json）",
            backup_json.display().to_string(),
        ));
    }

    let raw = std::fs::read(&backup_json)
        .map_err(|e| AppError::with_details(2004, "读取备份文件失败", e.to_string()))?;
    let file: BackupFile = serde_json::from_slice(&raw)
        .map_err(|e| AppError::with_details(2004, "解析备份文件失败", e.to_string()))?;
    if file.version > EXPORT_VERSION {
        return Err(AppError::with_details(
            2004,
            "备份文件版本过高，当前版本无法导入",
            format!("v{}", file.version),
        ));
    }

    let bundle_dir = backup_json.parent().map(|p| p.to_path_buf());
    let files_src: Option<PathBuf> = bundle_dir.as_ref().map(|b| b.join("files"));
    let local_files = files_dir();

    let mut summary = ImportSummary::default();

    for mut item in file.clips {
        // 图片路径重写回本机 files_dir；包里有对应文件就先拷过来。
        if let ClipContent::Image { path, .. } = &mut item.content {
            if let Some(fname) = Path::new(path)
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
            {
                let new_path = local_files.join(&fname);
                if let Some(src) = files_src.as_ref().map(|d| d.join(&fname)) {
                    if src.exists() && !new_path.exists() {
                        if let Err(e) = std::fs::copy(&src, &new_path) {
                            tracing::warn!("复制导入图片失败 {}: {e}", new_path.display());
                        }
                    }
                }
                *path = new_path.to_string_lossy().into_owned();
            }
        }

        let created = item.created_at;
        match state.history.insert_with_time(item, created) {
            Ok(_) => summary.clips += 1,
            Err(e) => {
                summary.skipped += 1;
                summary.errors.push(format!("导入一条历史失败：{e}"));
            }
        }
    }

    if input.restore_config {
        match serde_json::from_value::<crate::config::settings::Config>(file.config.clone()) {
            Ok(cfg) => {
                let patch = serde_json::to_value(cfg)
                    .map_err(|e| AppError::with_details(2004, "读取配置失败", e.to_string()))?;
                if let Err(e) = state.config.patch(patch) {
                    summary.errors.push(format!("恢复配置失败：{e}"));
                }
            }
            Err(e) => summary.errors.push(format!("恢复配置失败：{e}")),
        }
    }

    Ok(summary)
}
