//! 剪贴板堆栈命令。

use tauri::State;

use crate::core::paste::PasteOutcome;
use crate::core::stack::StackPasteAllResult;
use crate::error::{AppError, CommandResult};
use crate::state::AppState;

/// 压入一条历史到堆栈顶（锁定时返回「堆栈已锁定」错误）。
#[tauri::command]
pub fn stack_push(state: State<'_, AppState>, id: i64) -> CommandResult<()> {
    // 压栈前确认条目还在：历史可能刚被清空或被同步删除，把已不存在的 id
    // 塞进栈里，用户要到「发送全部」时才逐条收到失败，只能自己猜是哪一条。
    if !state.history.exists(id)? {
        return Err(AppError::with_details(
            2001,
            "该记录不存在，可能已被删除",
            format!("id={id}"),
        ));
    }
    state.stack.push(id)
}

/// 返回栈内全部 id（自底向顶）。
#[tauri::command]
pub fn stack_list(state: State<'_, AppState>) -> Vec<i64> {
    state.stack.list()
}

/// 粘贴堆栈底（最先入栈）条目，不弹出；空栈返回 `None`。
#[tauri::command]
pub fn stack_paste_first(state: State<'_, AppState>) -> CommandResult<Option<PasteOutcome>> {
    Ok(state.paste_stack_first()?.map(|e| e.outcome))
}

/// 从栈顶到栈底依次粘贴全部条目（每条间隔约 300ms），返回成功/失败计数。
///
/// 每条之间要等 300ms，10 条就是 3 秒：同步命令会把这 3 秒全部算在主线程头上
/// （窗口不重绘、按钮也不响应），所以放到阻塞线程池里跑。
#[tauri::command]
pub async fn stack_paste_all(state: State<'_, AppState>) -> CommandResult<StackPasteAllResult> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.paste_stack_all())
        .await
        .map_err(|e| AppError::with_details(9100, "批量粘贴意外终止", e.to_string()))?
}

/// 清空堆栈（锁定状态下同样可用）。
#[tauri::command]
pub fn stack_clear(state: State<'_, AppState>) -> CommandResult<()> {
    state.stack.clear();
    Ok(())
}

/// 设置堆栈锁定状态：锁定后拒绝压入新条目。
#[tauri::command]
pub fn stack_set_lock(state: State<'_, AppState>, locked: bool) -> CommandResult<()> {
    if locked {
        state.stack.lock();
    } else {
        state.stack.unlock();
    }
    Ok(())
}

/// 查询堆栈当前是否锁定。
#[tauri::command]
pub fn stack_locked(state: State<'_, AppState>) -> bool {
    state.stack.is_locked()
}
