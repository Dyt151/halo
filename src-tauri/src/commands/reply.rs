//! 快捷回复命令：分组 / 回复模板 CRUD、一键发送与 Excel 导入导出。

use std::path::Path;

use tauri::{AppHandle, State};

use crate::core::paste::PasteOutcome;
use crate::core::reply::xlsx;
use crate::core::reply::{Reply, ReplyGroup};
use crate::error::{AppError, CommandResult};
use crate::state::AppState;

/// 全部分组（按 sort 升序）。
#[tauri::command]
pub fn list_reply_groups(state: State<'_, AppState>) -> CommandResult<Vec<ReplyGroup>> {
    state.reply.list_groups()
}

/// 新建分组。
#[tauri::command]
pub fn add_reply_group(state: State<'_, AppState>, name: String) -> CommandResult<ReplyGroup> {
    state.reply.add_group(name)
}

/// 重命名分组。
#[tauri::command]
pub fn rename_reply_group(state: State<'_, AppState>, id: i64, name: String) -> CommandResult<()> {
    state.reply.rename_group(id, name)
}

/// 删除分组（级联删除组内回复）。
#[tauri::command]
pub fn delete_reply_group(state: State<'_, AppState>, id: i64) -> CommandResult<()> {
    state.reply.delete_group(id)
}

/// 查询回复：`group_id` 为 None 时返回全部；`keyword` 对标题/内容模糊匹配。
#[tauri::command]
pub fn list_replies(
    state: State<'_, AppState>,
    group_id: Option<i64>,
    keyword: Option<String>,
) -> CommandResult<Vec<Reply>> {
    state.reply.list_replies(group_id, keyword)
}

/// 新建回复。
#[tauri::command]
pub fn add_reply(
    state: State<'_, AppState>,
    group_id: i64,
    title: String,
    content: String,
    files: Vec<String>,
) -> CommandResult<Reply> {
    state.reply.add_reply(group_id, title, content, files)
}

/// 更新回复的标题、内容与附件。
#[tauri::command]
pub fn update_reply(
    state: State<'_, AppState>,
    id: i64,
    title: String,
    content: String,
    files: Vec<String>,
) -> CommandResult<()> {
    state.reply.update_reply(id, title, content, files)
}

/// 删除单条回复。
#[tauri::command]
pub fn delete_reply(state: State<'_, AppState>, id: i64) -> CommandResult<()> {
    state.reply.delete_reply(id)
}

/// 同组内移动排序：正数上移，负数下移。
#[tauri::command]
pub fn move_reply(state: State<'_, AppState>, id: i64, direction: i32) -> CommandResult<()> {
    state.reply.move_reply(id, direction)
}

/// 一键发送：把回复写入剪贴板并模拟粘贴到前台 App（复用回贴机制）。
#[tauri::command]
pub fn reply_paste(state: State<'_, AppState>, id: i64) -> CommandResult<PasteOutcome> {
    state.reply_paste_and_finish(id)
}

/// 关闭回复选择器浮动面板（Esc / 回贴后，不打开主窗口）。
#[tauri::command]
pub fn close_reply_picker(app: AppHandle) {
    crate::window::hide_reply_picker(&app);
}

/// 导出快捷回复为 xlsx（`group_id` 为 None 时导出全部），返回文件绝对路径。
///
/// 输出列：分组、标题、内容；写入 `~/Downloads/clipbase-replies-<时间戳>.xlsx`。
///
/// 同步命令跑在主线程上：回复多时拉数据 + 压缩写盘会让窗口卡住，
/// 所以连读写一起丢给阻塞线程池。
#[tauri::command]
pub async fn export_replies_xlsx(
    state: State<'_, AppState>,
    group_id: Option<i64>,
) -> CommandResult<String> {
    let reply = state.reply.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let groups = reply.list_groups()?;
        let replies = reply.list_replies(group_id, None)?;
        let rows = xlsx::export_rows(&groups, &replies);
        let dir = dirs::download_dir()
            .ok_or_else(|| AppError::new(9004, "无法定位下载目录".to_string()))?;
        let path = xlsx::export_file_path(&dir, now_ms());
        xlsx::write_export_file(&path, &rows)
            .map_err(|e| AppError::with_details(9004, "导出 Excel 失败", e))?;
        Ok(path.to_string_lossy().to_string())
    })
    .await
    .map_err(|e| AppError::with_details(9004, "导出 Excel 意外终止", e.to_string()))?
}

/// 从 xlsx 导入快捷回复：表头需含「标题」「内容」列，「分组」列可选
/// （有则按分组名归组，不存在则创建；无则落入「默认」分组，不存在则创建）。
///
/// 单行解析失败 / 单条插入失败收集到 `ImportSummary::errors`，不中断整体导入。
#[tauri::command]
pub async fn import_replies_xlsx(
    state: State<'_, AppState>,
    path: String,
) -> CommandResult<xlsx::ImportSummary> {
    let reply = state.reply.clone();
    tauri::async_runtime::spawn_blocking(move || {
        // 空路径不要落到 fs::metadata：「No such file or directory」这种
        // 英文系统文案对用户等于什么也没说。
        let path = path.trim().to_string();
        if path.is_empty() {
            return Err(AppError::new(9005, "没有选择要导入的 Excel 文件"));
        }
        let sheet = xlsx::read_sheet_rows(Path::new(&path))
            .map_err(|e| AppError::with_details(9005, "读取 Excel 失败", e))?;
        let (rows, parse_errors) = xlsx::parse_rows(&sheet);
        let mut summary = reply.import_replies(&rows)?;
        // 解析错误（表头 / 行级）排在导入错误之前展示。
        let mut errors = parse_errors;
        errors.append(&mut summary.errors);
        summary.errors = errors;
        Ok(summary)
    })
    .await
    .map_err(|e| AppError::with_details(9005, "导入 Excel 意外终止", e.to_string()))?
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
