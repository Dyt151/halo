//! 同步管理器：开关 / 定时 / 游标 / 加密 / LWW 合并。

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use base64::Engine;
use serde::Serialize;

use super::crypto::Crypto;
use super::provider::{SyncError, SyncProvider, SyncSnapshot, SyncTombstone};
use super::webdav::WebDavProvider;
use crate::config::ConfigService;
use crate::core::clipboard::types::{ClipContent, ClipItem};
use crate::core::history::HistoryService;
use crate::error::{AppError, CommandResult};
use crate::lock_ext::{LockExt, RwLockExt};

/// 同步状态（前端展示）。
#[derive(Debug, Clone, Serialize)]
pub struct SyncStatus {
    pub enabled: bool,
    pub state: String, // idle | syncing | success | error | disabled
    pub last_sync_at: Option<i64>,
    pub last_error: Option<String>,
    pub item_count: i64,
}

impl Default for SyncStatus {
    fn default() -> Self {
        Self {
            enabled: false,
            state: "idle".to_string(),
            last_sync_at: None,
            last_error: None,
            item_count: 0,
        }
    }
}

const SALT_KEY: &str = "e2e_salt";

#[derive(Clone)]
pub struct SyncManager {
    db: Arc<Mutex<rusqlite::Connection>>,
    history: HistoryService,
    config: ConfigService,
    status: Arc<RwLock<SyncStatus>>,
    /// 定时循环的「代号」。
    ///
    /// 不能用 bool：改间隔的逻辑是「先 `stop()` 再 `start()`」，而旧循环此刻正
    /// 阻塞在 `interval.tick()` 上；bool 被重新置回 true 后旧循环会「复活」，
    /// 于是两个循环并行、每个周期同步两次（PUT 也两次）。改成自增代号后，
    /// 旧循环醒来发现代号已变就自行退出。
    generation: Arc<AtomicU64>,
    /// 手动/定时同步互斥标记：连点「立即同步」不能并发跑两轮 PUT（评审 G9）。
    busy: Arc<AtomicBool>,
}

impl SyncManager {
    pub fn new(
        db: Arc<Mutex<rusqlite::Connection>>,
        history: HistoryService,
        config: ConfigService,
    ) -> Self {
        Self {
            db,
            history,
            config,
            status: Arc::new(RwLock::new(SyncStatus::default())),
            generation: Arc::new(AtomicU64::new(0)),
            busy: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn status(&self) -> SyncStatus {
        let mut s = self.status.read_ok().clone();
        s.enabled = self.config.get().sync.enabled;
        s
    }

    fn set_status(&self, state: &str, error: Option<String>) {
        let mut s = self.status.write_ok();
        s.state = state.to_string();
        s.last_error = error;
        s.enabled = self.config.get().sync.enabled;
        if state == "success" {
            s.last_sync_at = Some(now_ms());
        }
    }

    /// 立即同步一次。
    ///
    /// 用 `busy` 做互斥：上一轮还没结束时再次点击直接返回「正在同步」，
    /// 避免两轮 PUT 交叉写同一个远端文件（评审 G9）。
    pub async fn sync_now(&self) -> CommandResult<SyncStatus> {
        let mut cfg = self.config.get();
        // 密码/口令不在 config.json 落盘，同步前从钥匙串补回（评审 S6）。
        crate::secrets::hydrate_sync(&mut cfg.sync);
        if !cfg.sync.enabled {
            let msg = "同步未开启";
            self.set_status("disabled", Some(msg.to_string()));
            return Err(AppError::new(3001, msg));
        }
        // 开了端到端加密却没有口令：继续跑只会在 derive_key 处报一句
        // 「passphrase is empty」，不如现在说清楚该去哪里填。
        if cfg.sync.encryption && cfg.sync.passphrase.is_empty() {
            let msg = "已开启端到端加密，但钥匙串里没有口令";
            self.set_status("error", Some(msg.to_string()));
            return Err(AppError::new(3003, msg));
        }
        if self
            .busy
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(AppError::new(3002, "正在同步中，请稍候"));
        }

        self.set_status("syncing", None);
        let result = self.run_sync(&cfg.sync).await;
        self.busy.store(false, Ordering::SeqCst);
        match result {
            Ok(()) => {
                self.set_status("success", None);
                Ok(self.status())
            }
            Err(e) => {
                self.set_status("error", Some(e.to_string()));
                Err(AppError::with_details(3000, "同步失败", e.to_string()))
            }
        }
    }

    async fn run_sync(&self, sync_cfg: &crate::config::settings::SyncConfig) -> Result<(), SyncError> {
        let provider: Box<dyn SyncProvider> = match sync_cfg.provider {
            crate::config::settings::SyncProviderKind::WebDav => {
                Box::new(WebDavProvider::new()?)
            }
            crate::config::settings::SyncProviderKind::S3 => {
                return Err(SyncError::Provider(
                    "S3 provider is not implemented yet (P1)".to_string(),
                ))
            }
        };

        provider.test(&sync_cfg.credentials).await?;

        // 1. 拉取远端并合并。
        let remote = self.pull_snapshot(&*provider, sync_cfg).await?;
        let merged_count = self.merge_remote(&remote)?;
        tracing::info!("sync: merged {merged_count} remote items, {} tombstones", remote.deleted.len());

        // 2. 构造本地快照并推送。
        let snapshot = self.build_snapshot()?;
        let payload = self.encode_snapshot(&snapshot, sync_cfg)?;
        provider
            .push(&sync_cfg.credentials, &sync_cfg.remote_file, &payload)
            .await?;

        let mut s = self.status.write_ok();
        s.item_count = snapshot.items.len() as i64;
        Ok(())
    }

    /// 把远端快照并进本地库（纯本地操作，不碰网络，便于单测）。
    ///
    /// 顺序很关键：**先**应用墓碑，**再**合并条目。否则本轮会把一块
    /// 其它设备刚删掉的内容写回本地，下一轮才被墓碑清掉。
    ///
    /// 返回实际新增的条目数。
    pub fn merge_remote(&self, remote: &SyncSnapshot) -> CommandResult<usize> {
        for tombstone in &remote.deleted {
            if self
                .history
                .apply_remote_delete(&tombstone.hash, tombstone.deleted_at)?
            {
                tracing::debug!("sync: remote delete applied for {}", tombstone.hash);
            }
        }

        let mut merged = 0usize;
        for item in &remote.items {
            // 已有同内容：保留本地的时间/置顶/标签，不做任何改动。
            // （旧实现走 `insert`，它会把 created_at 刷成当天，
            //  于是每同步一次，所有重叠条目就被顶到列表最前面。）
            if self.history.exists_hash(&item.hash)? {
                continue;
            }
            if self.history.tombstoned_after(&item.hash, item.created_at)? {
                continue;
            }
            let Some(item) = portable_item(item) else {
                continue;
            };
            let created_at = item.created_at;
            self.history.insert_with_time(item, created_at)?;
            merged += 1;
        }
        Ok(merged)
    }

    async fn pull_snapshot(
        &self,
        provider: &dyn SyncProvider,
        sync_cfg: &crate::config::settings::SyncConfig,
    ) -> Result<SyncSnapshot, SyncError> {
        let raw = match provider.pull(&sync_cfg.credentials, &sync_cfg.remote_file).await {
            Ok(raw) => raw,
            Err(SyncError::NotFound) => {
                return Ok(SyncSnapshot {
                    items: Vec::new(),
                    deleted: Vec::new(),
                    cursor: Default::default(),
                    updated_at: 0,
                })
            }
            Err(e) => return Err(e),
        };
        let plain = if sync_cfg.encryption {
            let key = self.derive_key(sync_cfg)?;
            // AEAD 校验失败无法区分「口令错」与「远端不是密文」：两条都提示。
            Crypto::decrypt(&key, &raw).map_err(|_| {
                SyncError::Crypto(
                    "远端快照解密失败：口令与上传时不一致，或远端文件不是本应用加密的".to_string(),
                )
            })?
        } else {
            // 远端是密文而本机关了加密：直接用 serde 会报「expected value at line 1 column 1」，
            // 用户完全看不出是加密开关不一致。
            if raw.iter().find(|b| !b.is_ascii_whitespace()) != Some(&b'{') {
                return Err(SyncError::Crypto(
                    "远端快照看起来是加密数据，但本机未开启端到端加密".to_string(),
                ));
            }
            raw
        };
        serde_json::from_slice(&plain).map_err(|e| SyncError::Serialization(e.to_string()))
    }

    /// 构造本机快照（推送前用）。`pub` 是为了让测试能直接检查「会上传什么」。
    pub fn build_snapshot(&self) -> CommandResult<SyncSnapshot> {
        // 敏感内容（密码/token）不上云：默认不加密的 WebDAV 上就是一份明文密码表。
        // 图片条目按本机绝对路径存储，文件已不在（被容量/清理删掉）时也没有意义。
        let items = self
            .history
            .all()?
            .into_iter()
            .filter_map(to_portable)
            .collect();
        let deleted = self
            .history
            .tombstones(SYNC_TOMBSTONE_LIMIT)?
            .into_iter()
            .map(|(hash, deleted_at)| SyncTombstone { hash, deleted_at })
            .collect();
        Ok(SyncSnapshot {
            items,
            deleted,
            cursor: Default::default(),
            updated_at: now_ms(),
        })
    }

    fn encode_snapshot(
        &self,
        snapshot: &SyncSnapshot,
        sync_cfg: &crate::config::settings::SyncConfig,
    ) -> Result<Vec<u8>, SyncError> {
        let plain = serde_json::to_vec(snapshot).map_err(|e| SyncError::Serialization(e.to_string()))?;
        if sync_cfg.encryption {
            let key = self.derive_key(sync_cfg)?;
            Crypto::encrypt(&key, &plain).map_err(SyncError::Crypto)
        } else {
            Ok(plain)
        }
    }

    fn derive_key(
        &self,
        sync_cfg: &crate::config::settings::SyncConfig,
    ) -> Result<[u8; 32], SyncError> {
        let salt = self.get_or_create_salt().map_err(SyncError::Crypto)?;
        Crypto::derive_key(&sync_cfg.passphrase, &salt).map_err(SyncError::Crypto)
    }

    fn get_or_create_salt(&self) -> Result<[u8; 16], String> {
        let conn = self.db.lock_ok();
        let existing: Option<String> = conn
            .query_row(
                "SELECT value FROM sync_state WHERE key = ?1",
                [SALT_KEY],
                |r| r.get(0),
            )
            .ok()
            .filter(|s: &String| !s.is_empty());

        if let Some(encoded) = existing {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .map_err(|e| e.to_string())?;
            let mut salt = [0u8; 16];
            if bytes.len() != 16 {
                return Err("invalid stored salt".to_string());
            }
            salt.copy_from_slice(&bytes);
            return Ok(salt);
        }

        let salt = Crypto::generate_salt();
        let encoded = base64::engine::general_purpose::STANDARD.encode(salt);
        conn.execute(
            "INSERT INTO sync_state (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            rusqlite::params![SALT_KEY, encoded],
        )
        .map_err(|e| e.to_string())?;
        Ok(salt)
    }

    /// 启动定时同步（会取代上一轮循环）。
    pub fn start(&self, interval_secs: u64) {
        if interval_secs == 0 {
            return;
        }
        let token = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let this = self.clone();
        tauri::async_runtime::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(interval_secs.max(30)));
            interval.tick().await; // 首轮跳过，等待一个完整周期
            loop {
                interval.tick().await;
                // 已被新一轮 start 取代（或已被 stop）：退出，别和新循环抢着同步。
                if this.generation.load(Ordering::SeqCst) != token {
                    break;
                }
                if this.config.get().sync.enabled {
                    let _ = this.sync_now().await;
                }
            }
        });
    }

    /// 连通性/凭据测试（不落盘、不改配置、不写远端）。
    pub async fn test_connection(
        &self,
        provider_kind: crate::config::settings::SyncProviderKind,
        creds: &crate::config::settings::SyncCredentials,
    ) -> CommandResult<()> {
        if creds.url.trim().is_empty() {
            return Err(AppError::new(3003, "请填写服务器地址"));
        }
        let provider: Box<dyn SyncProvider> = match provider_kind {
            crate::config::settings::SyncProviderKind::WebDav => Box::new(
                WebDavProvider::new()
                    .map_err(|e| AppError::with_details(3006, "初始化网络客户端失败", e.to_string()))?,
            ),
            crate::config::settings::SyncProviderKind::S3 => {
                return Err(AppError::new(3004, "S3 同步尚未实现"))
            }
        };
        provider
            .test(creds)
            .await
            .map_err(|e| AppError::with_details(3005, "连接失败", e.to_string()))
    }

    /// 停止定时同步（正在睡的那一轮会在下一个周期醒来时退出）。
    pub fn stop(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// 快照里最多携带多少条墓碑（足够覆盖绝大多数手动删除，又不会让文件无限长胖）。
const SYNC_TOMBSTONE_LIMIT: i64 = 500;

/// 一条条目能不能跨设备落地；不能时返回 `None`。
///
/// - 敏感内容：不上云、也不从云上落地（即使远端快照里有，本机也不收）；
/// - 图片：`ClipContent::Image` 存的是**本机绝对路径**。文件名是内容寻址的
///   (`clip_<sha256 前 16 字节>.png`)，所以先试着换成本机 `files/` 下的同名文件；
///   本机没有这张图时（数据目录不同步）落地也只是一条打不开的空图，直接跳过。
///   反过来，推上云时由 [`to_portable`] 把路径压成裸文件名。
fn portable_item(item: &ClipItem) -> Option<ClipItem> {
    if item.sensitive {
        return None;
    }
    let ClipContent::Image { path, width, height } = &item.content else {
        return Some(item.clone());
    };
    let name = std::path::Path::new(path).file_name()?;
    // 本机已经有这张图：换成本机绝对路径（推上云时它会被压成裸文件名）。
    let local = crate::storage::files_dir().join(name);
    if local.is_file() {
        return Some(ClipItem {
            content: ClipContent::Image {
                path: local.to_string_lossy().into_owned(),
                width: *width,
                height: *height,
            },
            ..item.clone()
        });
    }
    // 本机没有：如果路径已经是裸文件名，说明它来自快照（对端也没有二进制），跳过；
    // 如果是本机绝对路径但文件丢了（被容量淘汰/手动删），同样跳过。
    None
}

/// 上传前的最后一道处理：把图片路径里的**本机绝对路径**压成裸文件名。
///
/// 快照只带 JSON，不带图片二进制；把 `/Users/alice/...` 写进云端文件既没有用处，
/// 也泄露了用户名。落地侧只按文件名在本地 `files/` 里查找（内容寻址，同名即同图）。
fn to_portable(item: ClipItem) -> Option<ClipItem> {
    let item = portable_item(&item)?;
    let ClipContent::Image { path, width, height } = &item.content else {
        return Some(item);
    };
    let name = std::path::Path::new(path).file_name()?.to_string_lossy().into_owned();
    Some(ClipItem {
        content: ClipContent::Image {
            path: name,
            width: *width,
            height: *height,
        },
        ..item
    })
}
