//! 历史服务：SQLite + FTS5 的剪贴板历史 CRUD 与容量策略。

pub mod search;

use std::sync::{Arc, Mutex};

use rusqlite::{params, OptionalExtension, Row};

use super::clipboard::types::{ClipContent, ClipItem, ContentType, HistoryQuery, SyncState};
use crate::config::ConfigService;
use crate::error::{AppError, CommandResult};
use crate::lock_ext::LockExt;

/// 历史条目统计信息（供状态栏 / 过滤标签计数展示）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct HistoryStats {
    pub total: i64,
    pub total_bytes: i64,
    /// 按内容类型计数（键为 `text`/`html`/`image`/`files`/`unknown`），
    /// 不受当前筛选条件影响，用于过滤标签的计数徽标。
    pub by_type: std::collections::HashMap<String, i64>,
    /// 已收藏条数（「收藏」筛选的计数）。
    pub favorite: i64,
}

#[derive(Clone)]
pub struct HistoryService {
    db: Arc<Mutex<rusqlite::Connection>>,
    config: ConfigService,
    /// 删除撤销栈：删除条目后 ⌘Z 可恢复（评审 M8）。
    ///
    /// 只做「内存里的最近 N 条」，不落盘：剪贴板工具用完即走，没必为了
    /// 一次跨重启的撤销把整个库再做一份快照。图片文件在删除时**故意不删**，
    /// 否则撤销回来的记录会指向已经消失的文件；无主文件由启动时的
    /// [`HistoryService::prune_orphan_images`] 回收。
    undo: Arc<Mutex<UndoStack>>,
}

/// 撤销/重做双向栈（容量固定，按时间淘汰最旧的）。
#[derive(Default)]
struct UndoStack {
    undo: Vec<ClipItem>,
    redo: Vec<ClipItem>,
}

/// 单次查询返回的最大条数（防止调用方传入天文数字的 `limit`）。
const MAX_QUERY_LIMIT: u32 = 2000;

/// 内容损坏时的错误码（与前端 `lib/errors.ts` 的分段约定一致）。
const CODE_CORRUPT: i32 = 2002;

/// 参数不合法（超长标签等）。
const CODE_INVALID_ARG: i32 = 2003;

/// 分组标签的长度上限（按字符计，emoji 算一个）。
const MAX_TAG_CHARS: usize = 64;

/// 撤销栈容量：比「一步」实用，又不至于把整库内容常驻内存。
const UNDO_CAPACITY: usize = 30;

impl HistoryService {
    pub fn new(db: Arc<Mutex<rusqlite::Connection>>, config: ConfigService) -> Self {
        Self {
            db,
            config,
            undo: Arc::new(Mutex::new(UndoStack::default())),
        }
    }

    /// 将数据库行映射为 `ClipItem`。
    ///
    /// 列顺序与 `search::build_query` / `get` 的 SELECT 列表一致：
    /// 0 id, 1 content_type, 2 content, 3 preview, 4 source_app,
    /// 5 created_at, 6 last_used_at, 7 pinned, 8 favorite, 9 sensitive,
    /// 10 size_bytes, 11 hash, 12 sync_state, 13 source_app_icon, 14 tag。
    fn row_to_item(row: &Row<'_>) -> rusqlite::Result<ClipItem> {
        let content_type_str: String = row.get(1)?;
        let content_json: String = row.get(2)?;
        let content: ClipContent = serde_json::from_str(&content_json).map_err(json_err)?;
        let sync_state_str: String = row.get(12)?;
        Ok(ClipItem {
            id: row.get(0)?,
            content_type: parse_content_type(&content_type_str),
            content,
            preview: row.get(3)?,
            source_app: row.get(4)?,
            source_app_icon: row.get(13)?,
            tag: row.get(14)?,
            created_at: row.get(5)?,
            last_used_at: row.get(6)?,
            pinned: row.get::<_, i64>(7)? != 0,
            favorite: row.get::<_, i64>(8)? != 0,
            sensitive: row.get::<_, i64>(9)? != 0,
            size_bytes: row.get::<_, i64>(10)? as u64,
            hash: row.get(11)?,
            sync_state: parse_sync_state(&sync_state_str),
        })
    }
}

mod maintenance;
mod read;
mod write;

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn parse_content_type(s: &str) -> ContentType {
    match s {
        "text" => ContentType::Text,
        "html" => ContentType::Html,
        "image" => ContentType::Image,
        "files" => ContentType::Files,
        _ => ContentType::Unknown,
    }
}

fn parse_sync_state(s: &str) -> SyncState {
    match s {
        "synced" => SyncState::Synced,
        "dirty" => SyncState::Dirty,
        _ => SyncState::Local,
    }
}

fn sync_state_str(state: SyncState) -> &'static str {
    match state {
        SyncState::Local => "local",
        SyncState::Synced => "synced",
        SyncState::Dirty => "dirty",
    }
}

fn json_err(e: serde_json::Error) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
    )
}
