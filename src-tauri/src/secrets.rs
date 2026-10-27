//! 敏感凭据存储（macOS 钥匙串）。
//!
//! 评审 S6：WebDAV 的登录密码此前直接写在 `config.json` 明文中，任何能读到
//! 用户主目录的进程/备份都能拿走。这里改为存进系统钥匙串，`config.json` 里
//! 只保留 URL / 用户名 / 「是否已存密钥」的布尔标记。
//!
//! 实现走 `/usr/bin/security` 命令行而不是 Security.framework FFI：钥匙串
//! 读写是低频操作（保存设置时一次、同步时一次），进程启动开销可以忽略，
//! 但省掉了 CFDictionary 的手工构造与释放，出问题的面小得多。

use std::process::Command;

use crate::config::settings::SyncConfig;

/// 钥匙串 service 名（钥匙串访问.app 里显示的条目名）。
const SERVICE: &str = "ClipBase";
/// WebDAV 密码的 account 名。
const ACCOUNT_WEBDAV: &str = "webdav-password";
/// 端到端加密口令的 account 名（同为敏感信息，一并入钥匙串）。
const ACCOUNT_PASSPHRASE: &str = "sync-passphrase";

/// 写入（覆盖）一条通用密码。
///
/// 代价说明（已知问题，见 docs/IMPLEMENTATION.md §6）：`security -w <value>` 会把
/// 密码放进 argv，同一用户下的其它进程在此刻 `ps` 得到。要彻底消除得直接调
/// Security.framework（SecItemAdd），但那会带上钥匙串 ACL 归属问题：存量条目
/// 是由 `security` CLI 创建的，换实现可能让用户重新授权/读不到旧密码。
pub fn store(account: &str, value: &str) -> Result<(), String> {
    // `-U` 表示已存在则更新；`-w` 传密码。
    let output = Command::new("/usr/bin/security")
        .args([
            "add-generic-password",
            "-s",
            SERVICE,
            "-a",
            account,
            "-w",
            value,
            "-U",
        ])
        .output()
        .map_err(|e| format!("无法调用钥匙串: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

/// 读取一条通用密码；条目不存在返回 `None`。
pub fn load(account: &str) -> Option<String> {
    let output = Command::new("/usr/bin/security")
        .args(["find-generic-password", "-s", SERVICE, "-a", account, "-w"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim_end_matches('\n').to_string();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

/// 删除一条通用密码（不存在不报错）。
pub fn delete(account: &str) -> Result<(), String> {
    let output = Command::new("/usr/bin/security")
        .args(["delete-generic-password", "-s", SERVICE, "-a", account])
        .output()
        .map_err(|e| format!("无法调用钥匙串: {e}"))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    // 条目本来就不存在是按幂等处理，不算失败。
    if stderr.contains("could not be found") || stderr.contains("The specified item could not be found") {
        Ok(())
    } else {
        Err(stderr.trim().to_string())
    }
}

/// 有没有存过 WebDAV 密码。
pub fn has_webdav_password() -> bool {
    load(ACCOUNT_WEBDAV).is_some()
}

/// 把密码写进钥匙串。空字符串表示「删除」。
pub fn set_webdav_password(password: &str) -> Result<(), String> {
    if password.is_empty() {
        delete(ACCOUNT_WEBDAV)
    } else {
        store(ACCOUNT_WEBDAV, password)
    }
}

/// 读取 WebDAV 密码。
pub fn webdav_password() -> Option<String> {
    load(ACCOUNT_WEBDAV)
}

/// 写/删端到端加密口令。空字符串表示「删除」。
pub fn set_sync_passphrase(passphrase: &str) -> Result<(), String> {
    if passphrase.is_empty() {
        delete(ACCOUNT_PASSPHRASE)
    } else {
        store(ACCOUNT_PASSPHRASE, passphrase)
    }
}

/// 有没有存过端到端加密口令（清单 8：界面需要区分「已存入钥匙串」与「未设置」）。
pub fn has_sync_passphrase() -> bool {
    load(ACCOUNT_PASSPHRASE).is_some()
}

/// 读取端到端加密口令。
fn sync_passphrase() -> Option<String> {
    load(ACCOUNT_PASSPHRASE)
}

/// 同步前把钥匙串里的凭据补进内存中的配置副本。
///
/// 只在 `config.json` 对应字段为空时补：这样「用户刚在界面上输入、还没
/// 保存」的即时测试仍然用界面上的值。
pub fn hydrate_sync(sync: &mut SyncConfig) {
    if sync.credentials.password.is_empty() {
        if let Some(pw) = webdav_password() {
            sync.credentials.password = pw;
        }
    }
    if sync.passphrase.is_empty() {
        if let Some(pp) = sync_passphrase() {
            sync.passphrase = pp;
        }
    }
}

/// 抹掉配置里的明文密码，返回被抹掉前的值（用于落库到钥匙串）。
pub fn take_password(sync: &mut SyncConfig) -> String {
    std::mem::take(&mut sync.credentials.password)
}

/// 抹掉配置里的加密口令，返回被抹掉前的值。
pub fn take_passphrase(sync: &mut SyncConfig) -> String {
    std::mem::take(&mut sync.passphrase)
}
