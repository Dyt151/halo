//! 剪贴板与历史相关命令。

use tauri::{Emitter, State};

use crate::core::clipboard::types::{ClipContent, ClipItem, HistoryQuery};
use crate::core::history::HistoryStats;
use crate::core::paste::PasteOutcome;
use crate::error::{AppError, CommandResult};
use crate::state::AppState;

#[tauri::command]
pub fn list_clips(state: State<'_, AppState>, query: HistoryQuery) -> CommandResult<Vec<ClipItem>> {
    state.history.query(query)
}

#[tauri::command]
pub fn get_clip(state: State<'_, AppState>, id: i64) -> CommandResult<ClipItem> {
    state.history.get(id)
}

/// 面板/托盘请求主窗口落到指定页（None = 仅唤出）。
#[tauri::command]
pub fn open_main_window(app: tauri::AppHandle, page: Option<String>) {
    // 这个值会经事件送回前端做路由：只收短标识符。不是白名单（前端加页时
    // 不必同步改后端），但挡掉控制字符/超长字符串这类明显不合理的负载。
    let page = page.map(|p| p.trim().to_string()).filter(|p| {
        !p.is_empty()
            && p.len() <= 32
            && p.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
    });
    crate::window::show_main_window_with_page(&app, page);
}

/// 主窗口挂载时取出待跳转页（Rust 侧存储，跨 webview 可信）。
#[tauri::command]
pub fn consume_pending_page() -> Option<String> {
    crate::window::consume_pending_page()
}

#[tauri::command]
pub fn paste_clip(state: State<'_, AppState>, id: i64) -> CommandResult<PasteOutcome> {
    state.paste_and_finish(id)
}

#[tauri::command]
pub fn copy_clip(state: State<'_, AppState>, id: i64) -> CommandResult<ClipContent> {
    state.paste.copy_to_clipboard(id)
}

#[tauri::command]
pub fn delete_clip(state: State<'_, AppState>, id: i64) -> CommandResult<()> {
    state.history.delete(id)?;
    let _ = state.app.emit(crate::events::HISTORY_DELETED, id);
    Ok(())
}

#[tauri::command]
pub fn clear_clips(state: State<'_, AppState>) -> CommandResult<()> {
    state.history.clear()?;
    let _ = state.app.emit(crate::events::HISTORY_DELETED, ());
    Ok(())
}

#[tauri::command]
pub fn pin_clip(state: State<'_, AppState>, id: i64, pinned: bool) -> CommandResult<()> {
    state.history.pin(id, pinned)
}

#[tauri::command]
pub fn favorite_clip(state: State<'_, AppState>, id: i64, favorite: bool) -> CommandResult<()> {
    state.history.favorite(id, favorite)
}

#[tauri::command]
pub fn set_clip_tag(state: State<'_, AppState>, id: i64, tag: Option<String>) -> CommandResult<()> {
    state.history.set_tag(id, tag)
}

#[tauri::command]
pub fn history_stats(state: State<'_, AppState>) -> CommandResult<HistoryStats> {
    state.history.stats()
}

/// 手动触发一次剪贴板捕获（调试/「立即捕获」用）。
#[tauri::command]
pub fn capture_now(state: State<'_, AppState>) -> CommandResult<Option<ClipItem>> {
    let event = state
        .monitor
        .poll_once()
        .map_err(|e| AppError::new(e.code, e.message))?;
    Ok(event.and_then(|e| state.capture_clip_event(e)))
}
