//! SQLite 连接初始化：打开/创建数据库、执行迁移、启用必要 PRAGMA。

use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use crate::error::{AppError, CommandResult};
use rusqlite::Connection;

use super::migrations::migrations;

/// 进程内串行化首次打开。
///
/// 同一进程里多个连接（测试并发、后台线程、模块宿主）同时首次打开同一个
/// 文件时，不比拼 SQLite 的锁重试：第一个跑完迁移，后面的直接读到版本表。
static OPEN_LOCK: Mutex<()> = Mutex::new(());

/// 跨进程重试次数与步长（见 `open` 的注释）。
const OPEN_RETRIES: u32 = 6;
const RETRY_STEP_MS: u64 = 30;

/// 打开（必要时创建）应用数据库并执行迁移。
pub fn open(path: &Path) -> CommandResult<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(AppError::internal)?;
    }

    // 先取进程内的锁；跨进程（双击双开）仍会撞，靠下面的退避重试兜住。
    let _guard = OPEN_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let mut last: Option<AppError> = None;
    for attempt in 0..OPEN_RETRIES {
        match open_once(path) {
            Ok(conn) => return Ok(conn),
            Err(e) => {
                last = Some(e);
                if attempt + 1 < OPEN_RETRIES {
                    // 每次都用**新连接**重试：失败的那次可能停在迁移事务里，
                    // 复用同一连接会一路错到底。
                    std::thread::sleep(Duration::from_millis(RETRY_STEP_MS));
                }
            }
        }
    }

    Err(last.unwrap_or_else(|| AppError::new(9001, "数据库迁移失败")))
}

/// 单次尝试：开连接 → 设 PRAGMA → 迁移到最新版本。
fn open_once(path: &Path) -> CommandResult<Connection> {
    let mut conn = Connection::open(path)?;
    conn.execute_batch(
        r#"
        PRAGMA journal_mode = WAL;
        PRAGMA foreign_keys = ON;
        PRAGMA synchronous = NORMAL;
        -- 多连接同时写时等锁，而不是立刻报 database is locked。
        PRAGMA busy_timeout = 5000;
        "#,
    )?;
    migrations()
        .to_latest(&mut conn)
        .map_err(|e| AppError::with_details(9001, "数据库迁移失败", e.to_string()))?;
    Ok(conn)
}
