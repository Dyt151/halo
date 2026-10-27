//! 快捷键解析与动作分发测试。

use std::str::FromStr;

use tauri_plugin_global_shortcut::Shortcut;

use crate::config::settings::Config;
use crate::core::hotkey::{resolve_action, HotkeyAction};

fn parse(s: &str) -> Shortcut {
    Shortcut::from_str(s).unwrap()
}

#[test]
fn resolve_toggle() {
    let c = Config::default();
    assert_eq!(resolve_action(&c, &parse("Control+C")), Some(HotkeyAction::Toggle));
}

#[test]
fn resolve_stack_paste() {
    let c = Config::default();
    // 「打开堆栈」已移除：同一组合键不再被解析为任何动作。
    assert_eq!(resolve_action(&c, &parse("Control+Shift+C")), None);
    assert_eq!(
        resolve_action(&c, &parse("Control+Shift+V")),
        Some(HotkeyAction::StackPaste)
    );
}

#[test]
fn number_keys_are_not_global_hotkeys() {
    // ⌘1~9 曾经被注册成全局快捷键，直接抢走所有 App 的这套组合（浏览器切标签、
    // 邮件切账号全失效）。现在它只在面板内由前端处理，全局解析必须返回 None。
    let c = Config::default();
    for n in 1..=9 {
        assert_eq!(
            resolve_action(&c, &parse(&format!("Cmd+{n}"))),
            None,
            "⌘{n} 不应被当成全局快捷键"
        );
    }
}

#[test]
fn number_keys_stay_unresolved_even_when_enabled() {
    let c = Config {
        num_hotkey_enabled: true,
        ..Default::default()
    };
    assert_eq!(resolve_action(&c, &parse("Cmd+1")), None);
}

#[test]
fn resolve_disabled_stack_returns_none() {
    let c = Config {
        stack_enabled: false,
        ..Default::default()
    };
    assert_eq!(resolve_action(&c, &parse("Control+Shift+C")), None);
    assert_eq!(resolve_action(&c, &parse("Control+Shift+V")), None);
}

#[test]
fn resolve_unknown_returns_none() {
    let c = Config::default();
    assert_eq!(resolve_action(&c, &parse("Alt+Space")), None);
    assert_eq!(resolve_action(&c, &parse("Cmd+0")), None);
}
