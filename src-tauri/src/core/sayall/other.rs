//! 非 macOS 平台：能力不存在，全部如实报「没装」。
//!
//! SayAll 只有 macOS 版，别的系统上这一项整个不显示，不做假按钮。

use super::State;

pub fn state() -> State {
    State::default()
}

pub fn open() -> Result<(), String> {
    Err("当前系统不支持无线麦 SayAll".to_string())
}

pub fn quit() -> Result<State, String> {
    Err("当前系统不支持无线麦 SayAll".to_string())
}

pub fn set_status_bar_visible(_visible: bool) -> Result<State, String> {
    Err("当前系统不支持无线麦 SayAll".to_string())
}
