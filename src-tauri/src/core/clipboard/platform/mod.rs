//! 平台抽象层：`ClipboardPort` trait + 平台选择工厂。
//!
//! 所有 OS 相关能力收敛到 `platform/` 目录的 trait 实现里，核心代码不散落
//! `#[cfg(target_os = ...)]`。

pub mod linux;
pub mod macos;
pub mod windows;

use thiserror::Error;

use super::types::ClipContent;

/// 剪贴板操作错误。
#[derive(Debug, Error)]
pub enum ClipError {
    #[error("clipboard operation failed: {0}")]
    Operation(String),
    #[error("clipboard content type is not supported")]
    UnsupportedType,
    #[error("clipboard is empty")]
    Empty,
}

/// 剪贴板端口抽象：屏蔽 OS 差异。
pub trait ClipboardPort: Send + Sync {
    /// 读取当前剪贴板内容。
    fn read(&self) -> Result<ClipContent, ClipError>;
    /// 写入剪贴板。
    fn write(&self, content: &ClipContent) -> Result<(), ClipError>;
    /// macOS 返回 `NSPasteboard.changeCount`；Win/Linux 返回内容指纹（变化即不同）。
    fn change_count(&self) -> Result<u64, ClipError>;
    /// 前台应用名（用于来源展示，无法获取时返回 `None`）。
    fn frontmost_app(&self) -> Option<String>;
}

/// 创建当前平台的剪贴板端口。
pub fn platform_port() -> Box<dyn ClipboardPort> {
    #[cfg(target_os = "macos")]
    {
        Box::new(macos::MacClipboardPort::new())
    }
    #[cfg(target_os = "windows")]
    {
        Box::new(windows::WindowsClipboardPort::new())
    }
    #[cfg(target_os = "linux")]
    {
        Box::new(linux::LinuxClipboardPort::new())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        // 兜底：返回一个空实现（理论上不会走到）。
        Box::new(UnsupportedPort)
    }
}

/// 未支持平台的空实现（兜底）。
#[allow(dead_code)]
struct UnsupportedPort;

impl ClipboardPort for UnsupportedPort {
    fn read(&self) -> Result<ClipContent, ClipError> {
        Err(ClipError::UnsupportedType)
    }
    fn write(&self, _content: &ClipContent) -> Result<(), ClipError> {
        Err(ClipError::UnsupportedType)
    }
    fn change_count(&self) -> Result<u64, ClipError> {
        Ok(0)
    }
    fn frontmost_app(&self) -> Option<String> {
        None
    }
}
