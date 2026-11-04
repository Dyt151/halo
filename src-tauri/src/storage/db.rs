//! SQLite 连接初始化：打开/创建数据库、执行迁移、启用必要 PRAGMA。

use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use crate::error::{AppError, CommandResult};
use rusqlite::Connection;

use super::migrations::migrations;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

/// 创建目录并确保仅当前用户可读写（0700）。
///
/// 默认 `create_dir_all` 受 umask 影响，会把 `~/.halo` 数据目录暴露给同机其它用户。
/// 这里在 Unix 上把已存在路径逐级收敛权限到 `~/.halo` 一层为止（更高层无权限会被忽略）；
/// 其它平台退化为原生 `create_dir_all`。
#[cfg(unix)]
fn ensure_private_dir(path: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(path)?;
    let mut cur = Some(path);
    while let Some(p) = cur {
        let _ = std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o700));
        cur = p.parent();
    }
    Ok(())
}

#[cfg(not(unix))]
fn ensure_private_dir(path: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(path)
}

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
        ensure_private_dir(parent).map_err(AppError::internal)?;
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

    // 收紧数据库文件权限为仅当前用户可读写（防止同用户其它进程 / 备份读到明文）。
    #[cfg(unix)]
    {
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
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
