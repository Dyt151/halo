//! 核心模块聚合：剪贴板 / 历史 / 快捷键 / 回贴 / 敏感识别。
//!
//! 该模块作为核心能力的统一出口，`pub use` 便于外部以 `crate::core::Xxx` 引用。

#![allow(unused_imports)]

pub mod appicon;
pub mod clipboard;
pub mod convert;
pub mod desktop_menu;
pub mod finder;
pub mod history;
pub mod hotkey;
pub mod option_tap;
pub mod paste;
pub mod reply;
pub mod ring;
pub mod sensitivity;
pub mod services;
pub mod sound;
pub mod stack;

pub use clipboard::types::{ClipContent, ClipItem, ContentType, HistoryQuery, SyncState};
pub use clipboard::ClipboardMonitor;
pub use history::HistoryService;
pub use hotkey::{resolve_action, HotkeyAction, HotkeyManager};
pub use paste::PasteService;
pub use reply::ReplyService;
pub use sensitivity::SensitivityDetector;
pub use sound::play_system_sound;
pub use stack::ClipboardStack;
