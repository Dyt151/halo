//! 无线麦 SayAll 的命令。
//!
//! 全部 `async`：这些命令要么走 `open` 子进程，要么发退出 Apple Event 并等它结束
//! （音频链路收尾，本机实测要好几秒），同步命令跑在主线程上会把整个界面钉住。

use crate::core::sayall::{self, State};
use crate::error::{AppError, CommandResult};

fn system_error(message: String) -> AppError {
    AppError::new(9400, message)
}

/// 装没装、版本、在不在跑、菜单栏图标开没开（面板每次弹出与设置页挂载时拉一次）。
#[tauri::command]
pub async fn sayall_state() -> State {
    sayall::state()
}

/// 进到 SayAll：拉起或把它自己的窗口带到前台。
#[tauri::command]
pub async fn sayall_open() -> CommandResult<State> {
    sayall::open().map_err(system_error)?;
    Ok(sayall::state())
}

/// 退出 SayAll。低版本（不支持下面的开关）想让右上角干净只能走这条。
#[tauri::command]
pub async fn sayall_quit() -> CommandResult<State> {
    sayall::quit().map_err(system_error)
}

/// 改 SayAll 自己的「菜单栏显示图标」开关；它只认启动时的值，所以可能伴随一次重启。
///
/// 版本不认这个键时直接报错，不写一个没人读的偏好。
#[tauri::command]
pub async fn sayall_set_status_bar(visible: bool) -> CommandResult<State> {
    sayall::set_status_bar_visible(visible).map_err(system_error)
}
