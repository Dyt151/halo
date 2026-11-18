//! 配置与快捷键命令。

use tauri::{Emitter, State};

use crate::config::settings::Config;
use crate::error::CommandResult;
use crate::state::AppState;

#[tauri::command]
pub fn get_config(state: State<'_, AppState>) -> Config {
    state.config.get()
}

#[tauri::command]
pub fn patch_config(state: State<'_, AppState>, patch: serde_json::Value) -> CommandResult<Config> {
    let saved = state.config.patch(patch)?;
    apply_hotkey(&state, &saved);
    apply_monitor(&state, &saved);
    // 环形盘的「Option + 拖动」开关需要即时生效（不用重启应用）。
    crate::core::option_tap::set_enabled(saved.ring.option_tap);
    // 状态栏图标开关同理：关掉后托盘图标立即消失，不需要重启。
    crate::window::apply_tray_visibility(&state.app, saved.statusbar.enabled);
    // 图标的三区内容（中心图形 / 电量环 / 音量点）也可能刚被改掉。
    crate::core::statusbar::tray::request_refresh();
    // 刚勾上 VPN 模块：接管状态的观察线程立即起来（重复调用只起一条），
    // 否则这次会话里核心退出就没人把图标改成「失效」。关掉模块不用停线程——
    // 它只在「已接管/已失效」时才动，其余时间只是睡着。
    if saved.statusbar.sections.iter().any(|id| id == "vpn") {
        crate::core::statusbar::start_proxy_watcher();
    }
    // 主题变化立即同步窗口外观，否则原生毛玻璃要等重启才跟上。
    crate::window::sync_theme(&state.app, &saved.theme);
    let _ = state.app.emit(crate::events::CONFIG_CHANGED, &saved);
    Ok(saved)
}

/// 按当前配置重新注册全部快捷键，并逐槽返回结果（评审 C6 / 清单 16）。
///
/// 前端保存快捷键后调用：只有这里能回答「到底注册成功没有」，
/// `patch_config` 内部只记日志（它还要服务托盘等无 UI 的调用方）。
#[tauri::command]
pub fn apply_hotkeys(state: State<'_, AppState>) -> Vec<crate::core::hotkey::HotkeySlotResult> {
    state.hotkey.apply_with_report(&state.config.get())
}

/// 设置页里的别名 → 系统设置真实锚点。
///
/// 前端只写语义名，锚点收在这张表里：`open` 对认不出的 anchor 一样返回成功，
/// 所以拼错的后果是「点了没反应」而不是报错——把 URL 集中一处才改得动。
/// 表里没有的入参仍按完整锚点透传，兼容已有的调用。
#[cfg(target_os = "macos")]
const SETTINGS_ANCHORS: &[(&str, &str)] = &[
    ("accessibility", "com.apple.Accessibility-Settings.extension"),
    (
        "login-items",
        "com.apple.General-Settings.extension?LoginItems",
    ),
];

/// 打开系统设置指定面板（设置页「服务」「辅助功能」等直达入口）。
#[tauri::command]
pub fn open_system_settings(pane: String) -> CommandResult<()> {
    #[cfg(target_os = "macos")]
    {
        // pane 会拼成 URL 交给 `open`：空白/控制字符一律拒绝，
        // 否则带 `\n` 的字符串会被 `open` 当成第二个参数（可能打开别的 URL）。
        let pane = pane.trim();
        if pane.is_empty()
            || pane.len() > 200
            || pane.chars().any(|c| c.is_control() || c.is_whitespace())
        {
            return Err(crate::error::AppError::with_details(
                9000,
                "系统设置定位参数不合法",
                pane.to_string(),
            ));
        }
        let anchor = SETTINGS_ANCHORS
            .iter()
            .find(|(key, _)| *key == pane)
            .map(|(_, anchor)| *anchor);
        let url = match anchor {
            Some(a) => format!("x-apple.systempreferences:{a}"),
            None if pane.starts_with("x-apple.systempreferences:") => pane.to_string(),
            None => format!("x-apple.systempreferences:{pane}"),
        };
        std::process::Command::new("open")
            .arg(&url)
            .status()
            .map_err(|e| {
                crate::error::AppError::with_details(9000, "无法打开系统设置", e.to_string())
            })?;
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = pane;
    }
    Ok(())
}

/// 辅助功能权限状态（清单 42：权限中心需要实时状态，不能只在回贴失败时才知道）。
#[tauri::command]
pub fn accessibility_status() -> bool {
    crate::core::paste::has_accessibility_permission()
}

/// 主动请求辅助功能权限（系统弹窗 + 自动把 Halo 加进白名单）。
///
/// 只由设置页的按钮调用：带 prompt 的查询每次都会弹窗。返回值是**调用当时**的
/// 状态，用户还没勾选时仍然是 `false`，前端应提示「已弹出系统提示，请勾选」。
#[tauri::command]
pub fn request_accessibility() -> bool {
    crate::core::paste::request_accessibility_permission()
}

/// 快捷键格式校验结果（清单 16）：不只回答「能不能解析」，还要给出拒绝原因。
#[derive(Debug, Clone, serde::Serialize)]
pub struct HotkeyCheck {
    pub ok: bool,
    pub message: Option<String>,
}

/// 把加速键规范成「排序后的小写修饰键 + 主键」，用于与保留键比较。
fn canonical(accelerator: &str) -> String {
    let mut mods: Vec<&'static str> = Vec::new();
    let mut key = String::new();
    for part in accelerator
        .split('+')
        .map(str::trim)
        .filter(|p| !p.is_empty())
    {
        match part.to_ascii_lowercase().as_str() {
            "control" | "ctrl" => mods.push("control"),
            "alt" | "option" => mods.push("alt"),
            "shift" => mods.push("shift"),
            "cmd" | "command" | "super" | "meta" => mods.push("cmd"),
            // 录制控件产生的是 `ArrowUp` 这类 W3C 键名，保留键表用的是方向词。
            "arrowup" | "up" => key = "up".into(),
            "arrowdown" | "down" => key = "down".into(),
            "arrowleft" | "left" => key = "left".into(),
            "arrowright" | "right" => key = "right".into(),
            "escape" | "esc" => key = "esc".into(),
            other => key = other.to_string(),
        }
    }
    if key.is_empty() {
        return String::new();
    }
    mods.sort_unstable();
    mods.dedup();
    format!("{}+{}", mods.join("+"), key)
}

/// 系统保留组合：注册可以「成功」但会抢系统行为，必须提前拒绝。
const RESERVED: &[(&str, &str)] = &[
    ("cmd+q", "⌘Q 是系统的「退出应用」"),
    ("cmd+w", "⌘W 是本应用的「隐藏窗口」"),
    ("cmd+m", "⌘M 是系统的「最小化」"),
    ("cmd+tab", "⌘⇥ 是系统的 App 切换器"),
    ("cmd+space", "⌘Space 是 Spotlight"),
    ("cmd+esc", "⌘⎋ 是系统的强制退出前台 App"),
    ("alt+cmd+esc", "⌥⌘⎋ 是系统的强制退出面板"),
    (
        "control+c",
        "⌃C 是终端的中断信号（SIGINT），抢走就没法中断命令",
    ),
    ("cmd+shift+3", "⌘⇧3 是系统截屏"),
    ("cmd+shift+4", "⌘⇧4 是系统截屏"),
    ("cmd+shift+5", "⌘⇧5 是系统截屏/录屏"),
    ("control+cmd+q", "⌃⌘Q 是锁定屏幕"),
    ("control+up", "⌃↑ 是调度中心"),
    ("control+down", "⌃↓ 是应用窗口"),
    ("control+left", "⌃← 是切换桌面空间"),
    ("control+right", "⌃→ 是切换桌面空间"),
];

/// 校验加速键：能解析 + 不是系统保留键 + 带修饰键。
#[tauri::command]
pub fn validate_hotkey(hotkey: String) -> HotkeyCheck {
    use std::str::FromStr;

    let normalized = canonical(&hotkey);
    if normalized.is_empty() {
        return HotkeyCheck {
            ok: false,
            message: Some("请按下完整组合键".into()),
        };
    }
    if !normalized.contains('+') {
        return HotkeyCheck {
            ok: false,
            message: Some("必须带 Control / Option / ⌘ 修饰键，否则会抢走普通输入".into()),
        };
    }
    // 修饰键 + 主键：`canonical` 已丢弃主键为空的输入，这里只需比对保留键。
    if let Some((_, reason)) = RESERVED.iter().find(|(combo, _)| *combo == normalized) {
        return HotkeyCheck {
            ok: false,
            message: Some((*reason).into()),
        };
    }
    if tauri_plugin_global_shortcut::Shortcut::from_str(&hotkey).is_err() {
        return HotkeyCheck {
            ok: false,
            message: Some("格式无效（需形如 Control+Shift+C）".into()),
        };
    }
    HotkeyCheck {
        ok: true,
        message: None,
    }
}

fn apply_hotkey(state: &AppState, config: &Config) {
    if let Err(e) = state.hotkey.register_all(config) {
        tracing::warn!("apply hotkey failed: {e}");
    }
}

fn apply_monitor(state: &AppState, config: &Config) {
    // 真正生效：开启时按（可能已变的）轮询间隔跑起来，关闭时停掉轮询。
    state.monitor.apply(
        state.monitor_tx.clone(),
        config.poll_interval_ms,
        config.monitor_enabled,
    );
}
