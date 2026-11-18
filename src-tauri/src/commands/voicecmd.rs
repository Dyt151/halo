//! 语音指令条的命令。
//!
//! 界面那一侧只做三件事：显示输入框、把听写结果发过来执行、显示结果。
//! 「听懂了什么」和「做了什么」全在后端（[`crate::core::voicecmd`]），
//! 这样以后加说法不用动前端。

use serde::Serialize;
use tauri::AppHandle;

use crate::core::voicecmd::{self, Outcome};
use crate::error::CommandResult;
use crate::window;

/// 执行结果 + 这条指令是否该让指令条留在屏幕上。
///
/// 失败时故意留着：语音转写经常就差一个字，让用户看到「没找到叫 X 的 App」比
/// 面板一闪而过有用得多。
#[derive(Serialize)]
pub struct VoiceCmdResult {
    pub outcome: Outcome,
    pub keep_open: bool,
}

/// 执行一句话。空串按「没听懂」处理，不特殊报错。
///
/// 必须是**同步**命令：同步命令跑在主线程，而 [`voicecmd::run`] 要收起指令条
/// （走 AppKit 的 `orderOut:`）、并把字贴回原来那个 App；丢到 tokio worker 上
/// 这些 ObjC 调用就没有 RunLoop 了（见 `core::sayall::macos` 里踩过的坑）。
#[tauri::command]
pub fn voice_cmd_execute(app: AppHandle, text: String) -> CommandResult<VoiceCmdResult> {
    let outcome = voicecmd::run(&app, &text);
    let keep_open = !outcome.ok;
    Ok(VoiceCmdResult { outcome, keep_open })
}

/// 收起指令条（Esc 与点空白处走这里）。
#[tauri::command]
pub fn voice_cmd_hide(app: AppHandle) {
    window::hide_voice_cmd(&app);
}
