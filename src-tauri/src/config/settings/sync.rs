//! 同步服务配置。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SyncProviderKind {
    #[default]
    WebDav,
    /// 预留。
    S3,
}

/// WebDAV 凭据（序列化存储；`password` 属敏感字段，运行时从配置读取）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SyncCredentials {
    pub url: String,
    pub username: String,
    pub password: String,
}

/// 同步配置。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyncConfig {
    /// 同步开关，默认关闭（本地优先）。
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub provider: SyncProviderKind,
    #[serde(default)]
    pub credentials: SyncCredentials,
    /// 端到端加密开关。
    #[serde(default)]
    pub encryption: bool,
    /// E2E 加密口令（可选，为空表示未设置）。
    #[serde(default)]
    pub passphrase: String,
    /// 同步远端文件名。
    #[serde(default = "default_sync_file")]
    pub remote_file: String,
    /// 自动同步间隔（秒），0 表示关闭定时。
    #[serde(default = "default_sync_interval")]
    pub interval_secs: u64,
}

fn default_sync_file() -> String {
    "halo-sync.json".to_string()
}

fn default_sync_interval() -> u64 {
    300
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: SyncProviderKind::WebDav,
            credentials: SyncCredentials::default(),
            encryption: false,
            passphrase: String::new(),
            remote_file: default_sync_file(),
            interval_secs: default_sync_interval(),
        }
    }
}

