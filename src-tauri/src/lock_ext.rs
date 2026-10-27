//! 忽略「中毒」状态的取锁工具。
//!
//! `std::sync` 的 `Mutex`/`RwLock` 在持锁线程 panic 后会变成 poisoned，
//! 之后**所有** `lock().unwrap()` 都会 panic。对本项目来说这是最糟的失败模式：
//! 数据库连接只有一份（`Arc<Mutex<Connection>>`），任何一处 panic 会让它在
//! 进程余下的生命里持续抛 panic —— 界面上表现为「每个命令都报错、重启前无法
//! 恢复」，而用户完全看不出原因。
//!
//! 继续用「已中毒」的锁是否安全，取决于被保护的数据有没有**跨字段不变式**：
//! - 数据库连接：SQLite 自己保证事务回滚，连接对象本身没有中间态，可继续用；
//! - `Config` 缓存：整体替换，写入是单条 `*guard = cfg`，不会半更新；
//! - 撤销栈 / 注册表 / 状态位：都是「一条一条」推进，最坏丢掉正在做的那一步。
//!
//! 所以统一走 `lock_ok()`：panic 的那一次操作已经失败了，但不该让后续操作
//! 跟着永久失败。真正需要暴露的 panic 由 `tracing` 与命令层错误上报负责。

use std::sync::{Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard};

/// 对 `Mutex` 取锁，中毒时继续使用内部数据。
pub trait LockExt<T> {
    fn lock_ok(&self) -> MutexGuard<'_, T>;
}

impl<T> LockExt<T> for Mutex<T> {
    fn lock_ok(&self) -> MutexGuard<'_, T> {
        self.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// 对 `RwLock` 取读锁/写锁，中毒时继续使用内部数据。
pub trait RwLockExt<T> {
    fn read_ok(&self) -> RwLockReadGuard<'_, T>;
    fn write_ok(&self) -> RwLockWriteGuard<'_, T>;
}

impl<T> RwLockExt<T> for RwLock<T> {
    fn read_ok(&self) -> RwLockReadGuard<'_, T> {
        self.read().unwrap_or_else(|e| e.into_inner())
    }

    fn write_ok(&self) -> RwLockWriteGuard<'_, T> {
        self.write().unwrap_or_else(|e| e.into_inner())
    }
}
