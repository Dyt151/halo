//! 同步 provider 抽象 + 快照/游标类型。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::config::settings::{SyncCredentials, SyncProviderKind};
use crate::core::clipboard::types::ClipItem;

/// 同步错误。
///
/// Display 直接面向用户（状态栏「上次同步」+ toast 会原样展示），
/// 因此一律用中文且说清楚下一步该检查什么，不保留 `provider error:` 这类内部前缀。
#[derive(Debug, Error)]
pub enum SyncError {
    #[error("同步未开启")]
    Disabled,
    #[error("服务端返回错误：{0}")]
    Provider(String),
    #[error("网络请求失败：{0}")]
    Network(String),
    #[error("加密密钥失败：{0}")]
    Crypto(String),
    #[error("快照格式不合法：{0}")]
    Serialization(String),
    #[error("远端还没有同步文件")]
    NotFound,
    #[error("本地错误：{0}")]
    Internal(String),
}

impl From<crate::error::AppError> for SyncError {
    fn from(e: crate::error::AppError) -> Self {
        SyncError::Internal(e.to_string())
    }
}

/// 同步游标（记录上次同步状态，LWW 用）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SyncCursor {
    pub last_sync_at: i64,
    pub last_hash: String,
}

/// 同步墓碑：某条内容在某个设备上被**显式删除**。
///
/// 快照是整份覆盖的，没有墓碑时「本地删掉 → 下一次拉取又被远端写回」，
/// 手动清理过的库会不停长回旧条目。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyncTombstone {
    pub hash: String,
    pub deleted_at: i64,
}

/// 同步快照：序列化为 JSON 文件（可选整体加密后上传）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncSnapshot {
    pub items: Vec<ClipItem>,
    /// 自旧版本快照（无此字段）读取时默认为空。
    #[serde(default)]
    pub deleted: Vec<SyncTombstone>,
    pub cursor: SyncCursor,
    pub updated_at: i64,
}

/// 同步 provider 抽象。
#[async_trait]
pub trait SyncProvider: Send + Sync {
    fn kind(&self) -> SyncProviderKind;
    /// 连通性/凭据测试。
    async fn test(&self, creds: &SyncCredentials) -> Result<(), SyncError>;
    /// 上传快照。
    async fn push(&self, creds: &SyncCredentials, remote_file: &str, payload: &[u8]) -> Result<(), SyncError>;
    /// 下载远端快照原始字节。
    async fn pull(&self, creds: &SyncCredentials, remote_file: &str) -> Result<Vec<u8>, SyncError>;
}
