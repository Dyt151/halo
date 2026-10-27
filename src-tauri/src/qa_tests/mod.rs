//! QA 测试套件（由 QA 工程师「严过关」补测）。
//!
//! 工程侧未提供任何单元测试（`cargo test` 原为 0 tests），本模块覆盖核心链路：
//! 数据库迁移、历史 CRUD/去重/容量策略、FTS5 搜索、配置读写、模块清单校验、
//! 权限门控、同步 E2E 加密、敏感识别、剪贴板类型工具函数。

mod blacklist;
mod capabilities;
mod config;
mod crypto;
mod db;
mod history;
mod hotkey;
mod manifest;
mod reply;
mod search;
mod sensitivity;
mod stack;
mod sync;
mod types;

use std::sync::{Arc, Mutex};

use rusqlite::Connection;

/// 构建一个已执行迁移的「内存数据库」连接，供各测试共享同一套 schema。
pub fn test_db() -> Arc<Mutex<Connection>> {
    let mut conn = Connection::open_in_memory().expect("open in-memory db");
    crate::storage::migrations::migrations()
        .to_latest(&mut conn)
        .expect("apply migrations");
    Arc::new(Mutex::new(conn))
}
