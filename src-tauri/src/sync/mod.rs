//! 同步层聚合。
//!
//! 该模块作为同步层的统一出口，`pub use` 便于外部以 `crate::sync::Xxx` 引用。

#![allow(unused_imports)]

pub mod crypto;
pub mod manager;
pub mod provider;
pub mod webdav;

pub use manager::{SyncManager, SyncStatus};
pub use provider::{SyncCursor, SyncError, SyncProvider, SyncSnapshot};
pub use webdav::WebDavProvider;
