//! 非 macOS 平台：没有本机 App 索引这一说，全部报「不支持」。

use super::{AppInfo, Located};

pub fn installed() -> Vec<AppInfo> {
    Vec::new()
}

pub fn open(_app: &AppInfo) -> Result<(), String> {
    Err("当前系统不支持用语音启动 App".to_string())
}

pub fn quit(_query: &str) -> Result<Located, String> {
    Err("当前系统不支持用语音退出 App".to_string())
}
