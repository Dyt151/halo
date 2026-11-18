//! 语音指令条窗口（`cmdbar`）。
//!
//! 一句话说完就一个动作，所以这个窗口只做一件事：**成为一个能被听写的输入框**。
//! 与输入法 HUD / 环形盘那两类「从不抢焦点」的浮层相反，这里必须抢——SayAll 那类
//! 语音 App 只会把字打进当前焦点输入框，焦点不在我们这儿，指令条就是空的。
//!
//! 也因此它的收起条件和普通面板一样：失焦即收（[`handle_voice_cmd_focus_lost`]），
//! 加上和剪贴板面板同一套宽限期，避免刚显示时系统补发的失焦事件把它立刻收掉。

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use super::*;

/// 窗口标签（前端 `isVoiceCmdWindow()` 按这个判断）。
const VOICE_CMD_LABEL: &str = "cmdbar";
/// 宽 620：中文一句话（「打开抖音直播伴侣」）不折行；再宽就像搜索框了。
const VOICE_CMD_WIDTH: f64 = 620.0;
/// 高 108：输入行 + 结果/提示行。
const VOICE_CMD_HEIGHT: f64 = 108.0;
/// 顶边离工作区顶部的比例：和聚焦搜索同一视觉位置，说指令时眼睛本来就在那儿看。
const VOICE_CMD_TOP_RATIO: f64 = 0.22;

static VOICE_CMD_SHOWN_AT_MS: AtomicU64 = AtomicU64::new(0);
static VOICE_CMD_FOCUSED: AtomicBool = AtomicBool::new(false);

/// 弹出指令条：先记下用户正在用的 App，再抢键盘焦点。
///
/// `remember_frontmost_app()` 必须在 `set_focus()` 之前——粘贴栈顶这条指令要把字贴回
/// 原来那个 App，晚一步取到的前台就是 Halo 自己。
pub fn show_voice_cmd(app: &AppHandle) {
    let Some(bar) = app.get_webview_window(VOICE_CMD_LABEL) else {
        tracing::warn!("语音指令条窗口不存在");
        return;
    };
    place_voice_cmd(&bar);

    #[cfg(target_os = "macos")]
    set_window_level(&bar, 25);
    allow_over_fullscreen(&bar);
    remember_frontmost_app();

    VOICE_CMD_SHOWN_AT_MS.store(now_ms(), Ordering::SeqCst);
    VOICE_CMD_FOCUSED.store(false, Ordering::SeqCst);
    // 顺序同剪贴板面板：先把宿主排进当前 Space，再激活 App 抢键盘，
    // 反过来会在全屏 Space 里把空间切回桌面。
    #[cfg(target_os = "macos")]
    reveal_panel_host(&bar);
    #[cfg(target_os = "macos")]
    order_front_regardless(&bar);
    activate_app();
    let _ = bar.show();
    let _ = bar.set_focus();
    refresh_shadow(&bar);

    // 先显示再发事件：隐藏中的 webview 会被节流，事件发早了清空与聚焦就发生在看不见的地方。
    let _ = app.emit("halo://voice-cmd-shown", ());
}

/// 收起指令条。
pub fn hide_voice_cmd(app: &AppHandle) {
    if let Some(bar) = app.get_webview_window(VOICE_CMD_LABEL) {
        VOICE_CMD_FOCUSED.store(false, Ordering::SeqCst);
        conceal_panel_host(VOICE_CMD_LABEL);
        let _ = bar.hide();
    }
}

/// 快捷键入口：可见则收起，否则弹出。
pub fn toggle_voice_cmd(app: &AppHandle) {
    let visible = app
        .get_webview_window(VOICE_CMD_LABEL)
        .map(|bar| bar.is_visible().unwrap_or(false))
        .unwrap_or(false);
    if visible {
        hide_voice_cmd(app);
    } else {
        show_voice_cmd(app);
    }
}

/// 失焦即收起：宽限期内、以及「其实还是焦点窗口」的两种误判都不收。
pub fn handle_voice_cmd_focus_lost(app: &AppHandle) {
    let shown_at = VOICE_CMD_SHOWN_AT_MS.load(Ordering::SeqCst);
    if shown_at > 0 && now_ms().saturating_sub(shown_at) < FOCUS_GRACE_MS {
        return;
    }
    if !VOICE_CMD_FOCUSED.swap(false, Ordering::SeqCst) {
        return;
    }
    if let Some(bar) = app.get_webview_window(VOICE_CMD_LABEL) {
        if bar.is_focused().unwrap_or(false) {
            VOICE_CMD_FOCUSED.store(true, Ordering::SeqCst);
            return;
        }
    }
    hide_voice_cmd(app);
}

/// 拿到焦点：打标记，供上面的失焦判断使用。
pub fn handle_voice_cmd_focus_gained() {
    VOICE_CMD_FOCUSED.store(true, Ordering::SeqCst);
}

/// 定位：光标所在屏的工作区，水平居中、偏上。
fn place_voice_cmd(bar: &WebviewWindow) {
    let Some(monitor) = monitor_at_cursor(bar)
        .or_else(|| bar.current_monitor().ok().flatten())
        .or_else(|| bar.primary_monitor().ok().flatten())
    else {
        return;
    };
    let scale = monitor.scale_factor().max(1.0);
    let work = monitor.work_area();
    let origin_x = work.position.x as f64 / scale;
    let origin_y = work.position.y as f64 / scale;
    let avail_w = work.size.width as f64 / scale;
    let avail_h = work.size.height as f64 / scale;

    let width = VOICE_CMD_WIDTH.min((avail_w - FLOAT_MARGIN * 2.0).max(320.0));
    let height = VOICE_CMD_HEIGHT.min((avail_h - FLOAT_MARGIN * 2.0).max(88.0));
    let x = origin_x + (avail_w - width) / 2.0;
    let y = origin_y + ((avail_h - height) * VOICE_CMD_TOP_RATIO).max(FLOAT_MARGIN);

    let _ = bar.set_size(Size::Logical(LogicalSize::new(width, height)));
    let _ = bar.set_position(Position::Logical(LogicalPosition::new(x, y)));
}
