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

    /// 插入或更新（按 hash 去重）一条历史，返回其 id。
    pub fn insert(&self, item: ClipItem) -> CommandResult<i64> {
        self.insert_inner(item, now_ms(), true)
    }

    /// 插入一条历史并**保留指定的创建时间**。
    ///
    /// 普通捕获用 [`insert`]（时间取当前，重复内容会重新置顶）；
    /// 从其它工具迁移历史时用本方法：以传入的原始时间戳为准，
    /// 且重复内容**不覆盖**已有记录的时间（否则最早那条的时间会被冲成导入时刻）。
    pub fn insert_with_time(&self, item: ClipItem, created_at: i64) -> CommandResult<i64> {
        self.insert_inner(item, created_at, false)
    }

    fn insert_inner(
        &self,
        mut item: ClipItem,
        created_at: i64,
        refresh_time_on_duplicate: bool,
    ) -> CommandResult<i64> {
        let now = now_ms();
        item.created_at = if created_at > 0 { created_at } else { now };
        item.last_used_at = now;
        item.preview = item.content.preview(200);
        let search_text = item.content.search_text();
        let content_json = serde_json::to_string(&item.content)?;
        let content_type = item.content.content_type().as_str().to_string();
        let sync_state = sync_state_str(item.sync_state);
        let hash = item.hash.clone();

        let conn = self.db.lock_ok();

        // 去重：相同 hash 只保留一条。普通捕获时重新置顶（created_at 刷新为当前），
        // 迁移导入时保留已有记录的时间戳。
        let existing: Option<i64> = conn
            .query_row(
                "SELECT id FROM clips WHERE hash = ?1 LIMIT 1",
                [&item.hash],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(id) = existing {
            if refresh_time_on_duplicate {
                conn.execute(
                    "UPDATE clips SET last_used_at = ?1, created_at = ?2 WHERE id = ?3",
                    params![now, now, id],
                )?;
            } else {
                conn.execute(
                    "UPDATE clips SET last_used_at = ?1 WHERE id = ?2",
                    params![now, id],
                )?;
            }
            return Ok(id);
        }

        conn.execute(
            "INSERT INTO clips (
                content_type, content, search_text, preview, source_app, source_app_icon,
                created_at, last_used_at, pinned, favorite, sensitive,
                size_bytes, hash, sync_state, tag
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
                content_type,
                content_json,
                search_text,
                item.preview,
                item.source_app,
                item.source_app_icon,
                item.created_at,
                now,
                item.pinned as i64,
                item.favorite as i64,
                item.sensitive as i64,
                item.size_bytes as i64,
                item.hash,
                sync_state,
                item.tag,
            ],
        )?;
        // 重新复制过来的内容不该再被旧墓碑拦着（删了又存 = 用户还要它）。
        conn.execute("DELETE FROM sync_tombstones WHERE hash = ?1", [&hash])?;
        let id = conn.last_insert_rowid();
        let removed_paths = self.enforce_capacity(&conn)?;
        // 容量淘汰掉的行若引用图片文件，清理无主文件。
        prune_image_files(&conn, &removed_paths);
        Ok(id)
    }

    /// 迁移导入专用：按「创建时间 + 来源 App」定位已有记录并**原地更新内容**
    /// （hash / content_type / preview / search_text / size_bytes），保留
    /// pinned / favorite / tag / last_used_at 等用户操作；无匹配时按普通导入插入。
    /// 返回是否插入了新行（false = 更新了既有行）。
    pub fn replace_by_time(&self, mut item: ClipItem) -> CommandResult<bool> {
        let exists = {
            let conn = self.db.lock_ok();
            if item.created_at > 0 {
                conn.query_row(
                    "SELECT id FROM clips WHERE created_at = ?1 AND source_app IS ?2 LIMIT 1",
                    params![item.created_at, item.source_app],
                    |r| r.get::<_, i64>(0),
                )
                .optional()?
                .is_some()
            } else {
                false
            }
        };
        if !exists {
            let created_at = item.created_at;
            return self.insert_inner(item, created_at, false).map(|_| true);
        }

        item.last_used_at = now_ms();
        item.preview = item.content.preview(200);
        let search_text = item.content.search_text();
        let content_json = serde_json::to_string(&item.content)?;
        let conn = self.db.lock_ok();
        conn.execute(
            "UPDATE clips SET content_type = ?1, content = ?2, preview = ?3,
             search_text = ?4, size_bytes = ?5, hash = ?6
             WHERE created_at = ?7 AND source_app IS ?8",
            params![
                item.content.content_type().as_str(),
                content_json,
                item.preview,
                search_text,
                item.size_bytes as i64,
                item.hash,
                item.created_at,
                item.source_app,
            ],
        )?;
        Ok(false)
    }

    /// 维护：按内容重算全表的 `size_bytes`，返回修正条数。
    ///
    /// 计算口径与 `ClipContent::size_bytes` 完全一致：图片取磁盘文件大小、
    /// 文本/HTML 取内容字节数、files 取路径字符串字节数。旧版本写入 0 的脏数据
    /// 会在启动时被一次性修正（幂等，仅更新确实不一致的行）。
    pub fn backfill_sizes(&self) -> CommandResult<usize> {
        let conn = self.db.lock_ok();
        let pending: Vec<(i64, i64)> = {
            let mut stmt = conn.prepare("SELECT id, content, size_bytes FROM clips")?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            })?;
            let mut pending = Vec::new();
            for row in rows {
                let (id, json, stored) = row?;
                let Ok(content) = serde_json::from_str::<ClipContent>(&json) else {
                    continue;
                };
                let actual = content.size_bytes() as i64;
                // 文件缺失时 actual=0：保留原值，不把已知大小归零。
                if actual > 0 && actual != stored {
                    pending.push((id, actual));
                }
            }
            pending
        };

        for (id, size) in &pending {
            conn.execute(
                "UPDATE clips SET size_bytes = ?1 WHERE id = ?2",
                params![size, id],
            )?;
        }
        Ok(pending.len())
    }

    /// 迁移维护：按磁盘文件回填图片记录的 `size_bytes` 与缺失的 width/height。
    /// 返回（修正字节数的条数，修正尺寸的条数）。
    pub fn backfill_image_sizes(&self) -> CommandResult<(usize, usize)> {
        let conn = self.db.lock_ok();
        let mut fixed_sizes = 0;
        let mut fixed_dims = 0;
        let mut stmt =
            conn.prepare("SELECT id, content FROM clips WHERE content_type = 'image'")?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (id, json) = row?;
            let Ok(ClipContent::Image {
                path,
                mut width,
                mut height,
            }) = serde_json::from_str(&json)
            else {
                continue;
            };
            let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0) as i64;
            // 尺寸缺失（如 TIFF 伪装成 .png 的记录）：按内容嗅探真实宽高。
            if width == 0 || height == 0 {
                if let Ok((w, h)) = image::image_dimensions(&path) {
                    width = w;
                    height = h;
                    let new_content = serde_json::to_string(&ClipContent::Image {
                        path: path.clone(),
                        width,
                        height,
                    })?;
                    conn.execute(
                        "UPDATE clips SET content = ?1 WHERE id = ?2",
                        params![new_content, id],
                    )?;
                    fixed_dims += 1;
                }
            }
            if size > 0 {
                conn.execute(
                    "UPDATE clips SET size_bytes = ?1 WHERE id = ?2",
                    params![size, id],
                )?;
                fixed_sizes += 1;
            }
        }
        Ok((fixed_sizes, fixed_dims))
    }

    /// 查询历史（时间倒序，置顶优先）。
    ///
    /// **单条坏数据不能拖垮整份列表**：`content` 是序列化后的 JSON，旧版本
    /// 曾写进过非法 JSON（例如 `{"kind":"text","value":正文}` 少了引号）。
    /// 以前这里 `?` 一路向上抛，`list_clips` 整个失败 —— 面板上就是「一条都
    /// 没有 + 一个看不懂的报错」，点别的筛选（恰好把坏行排除掉）才恢复。
    /// 现在逐行降级：解析不了的行跳过并记日志，其余条目照常返回。
    pub fn query(&self, q: HistoryQuery) -> CommandResult<Vec<ClipItem>> {
        let (mut sql, mut params) = search::build_query(&q);
        sql.push_str(" ORDER BY c.pinned DESC, c.created_at DESC LIMIT ? OFFSET ?");
        // 上限来自前端传参，不能直接信：一次要 10 亿条会直接把内存吃干。
        // 500（面板/历史页）已经够用，2000 给今后的导出留余量。
        params.push(rusqlite::types::Value::Integer(i64::from(
            q.limit.clamp(1, MAX_QUERY_LIMIT),
        )));
        params.push(rusqlite::types::Value::Integer(i64::from(q.offset)));

        let conn = self.db.lock_ok();
        let mut stmt = conn.prepare(&sql)?;
        let mut rows = stmt.query(rusqlite::params_from_iter(params))?;
        let mut items = Vec::new();
        let mut skipped = 0usize;
        while let Some(row) = rows.next()? {
            match Self::row_to_item(row) {
                Ok(item) => items.push(item),
                Err(e) => {
                    skipped += 1;
                    let id = row.get::<_, i64>(0).unwrap_or(-1);
                    tracing::warn!("跳过无法解析的历史条目 id={id}: {e}");
                }
            }
        }
        // 全都读不出来时才报错：此时列表本来就是空的，必须让用户知道
        // 是「数据坏了」而不是「没有历史」（空状态文案会误导人）。
        if items.is_empty() && skipped > 0 {
            return Err(AppError::with_details(
                CODE_CORRUPT,
                "历史记录无法读取",
                format!("{skipped} 条记录的内容已损坏"),
            ));
        }
        Ok(items)
    }

    /// 修复旧版本写入的非法 `content`（返回修复条数）。
    ///
    /// 启动时跑一次即可：坏行只要还在，`query` 就得一直跳过它，用户也永远
    /// 看不到那条内容。这里按旧格式把正文救回来重写成合法 JSON；救不回来
    /// 的保持原样（不删用户数据），由 `query` 跳过。
    pub fn repair_corrupt_content(&self) -> usize {
        let conn = self.db.lock_ok();
        let pending: Vec<(i64, String, String)> = {
            let mut stmt = match conn.prepare("SELECT id, content_type, content FROM clips") {
                Ok(stmt) => stmt,
                Err(e) => {
                    tracing::warn!("扫描损坏内容失败: {e}");
                    return 0;
                }
            };
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            });
            match rows {
                Ok(rows) => rows.filter_map(std::result::Result::ok).collect(),
                Err(e) => {
                    tracing::warn!("扫描损坏内容失败: {e}");
                    return 0;
                }
            }
        };

        let mut fixed = 0usize;
        for (id, content_type, raw) in pending {
            if serde_json::from_str::<ClipContent>(&raw).is_ok() {
                continue;
            }
            let Some(content) = salvage_content(&raw, &content_type) else {
                tracing::warn!("历史条目 id={id} 的内容无法修复，查询时会跳过");
                continue;
            };
            let Ok(json) = serde_json::to_string(&content) else {
                continue;
            };
            let preview = content.preview(200);
            let search_text = content.search_text();
            if conn
                .execute(
                    "UPDATE clips SET content = ?1, preview = ?2, search_text = ?3, size_bytes = ?4 \
                     WHERE id = ?5",
                    params![json, preview, search_text, content.size_bytes() as i64, id],
                )
                .is_ok()
            {
                fixed += 1;
                tracing::info!("修复损坏的历史条目 id={id}");
            }
        }
        fixed
    }

    /// 按 id 获取单条。
    ///
    /// 错误分开报：「查不到」与「内容坏了」是两回事，合并成一句
    /// 「记录不存在」会把数据库损坏误导成「被删了」，无从排查。
    pub fn get(&self, id: i64) -> CommandResult<ClipItem> {
        let conn = self.db.lock_ok();
        match conn
            .query_row(
                "SELECT id, content_type, content, preview, source_app, created_at, \
                 last_used_at, pinned, favorite, sensitive, size_bytes, hash, sync_state, \
                 source_app_icon, tag \
                 FROM clips WHERE id = ?1",
                [id],
                Self::row_to_item,
            )
            .optional()
        {
            Ok(Some(item)) => Ok(item),
            Ok(None) => Err(AppError::with_details(
                2001,
                "该记录不存在，可能已被删除",
                format!("id={id}"),
            )),
            Err(rusqlite::Error::FromSqlConversionFailure(..)) => Err(AppError::with_details(
                CODE_CORRUPT,
                "该记录内容已损坏，无法使用",
                format!("id={id}"),
            )),
            Err(e) => Err(AppError::internal(e)),
        }
    }

    /// 记录是否存在。
    ///
    /// 供「只关心在不在」的路径用（如压入粘贴堆栈）：走一次 `SELECT 1`，
    /// 不解析 JSON 内容，也不会因为某条内容损坏就把整次操作报成失败。
    pub fn exists(&self, id: i64) -> CommandResult<bool> {
        let conn = self.db.lock_ok();
        let found: Option<i64> = conn
            .query_row("SELECT 1 FROM clips WHERE id = ?1", [id], |r| r.get(0))
            .optional()?;
        Ok(found.is_some())
    }

    /// 删除单条（可撤销）。
    ///
    /// 图片文件不在这里删：撤销需要它还在。无主文件在启动时统一回收，
    /// 不会真的留下垃圾。
    ///
    /// 内容损坏的条目**也能删**：以前先 `get` 再删，解析失败就删不掉，
    /// 那条坏数据会永远卡在库里、又把列表查询拖下水。
    pub fn delete(&self, id: i64) -> CommandResult<()> {
        let item = self.get(id).ok();
        let hash: String = {
            let conn = self.db.lock_ok();
            conn.query_row("SELECT hash FROM clips WHERE id = ?1", [id], |r| r.get(0))
                .optional()?
                .ok_or_else(|| {
                    AppError::with_details(2001, "该记录不存在，可能已被删除", format!("id={id}"))
                })?
        };
        {
            let conn = self.db.lock_ok();
            conn.execute("DELETE FROM clips WHERE id = ?1", [id])?;
            record_tombstone(&conn, &hash, now_ms());
        }
        // 内容坏掉时没有可撤销的条目，但删除本身必须成功。
        if let Some(item) = item {
            self.push_undo(item);
        }
        Ok(())
    }

    /// 撤销上一次删除；无可用撤销时返回 None。
    pub fn undo(&self) -> CommandResult<Option<ClipItem>> {
        let item = self.undo.lock_ok().undo.pop();
        let Some(item) = item else {
            return Ok(None);
        };
        let restored = self.restore_item(item)?;
        let mut stack = self.undo.lock_ok();
        stack.redo.push(restored.clone());
        if stack.redo.len() > UNDO_CAPACITY {
            stack.redo.remove(0);
        }
        Ok(Some(restored))
    }

    /// 重做上一次撤销（= 再删一次），返回被删除的 id；无可用重做时返回 None。
    pub fn redo(&self) -> CommandResult<Option<i64>> {
        let item = self.undo.lock_ok().redo.pop();
        let Some(item) = item else {
            return Ok(None);
        };
        let id = item.id;
        {
            let conn = self.db.lock_ok();
            conn.execute("DELETE FROM clips WHERE id = ?1", [id])?;
            record_tombstone(&conn, &item.hash, now_ms());
        }
        let mut stack = self.undo.lock_ok();
        stack.undo.push(item);
        if stack.undo.len() > UNDO_CAPACITY {
            stack.undo.remove(0);
        }
        Ok(Some(id))
    }

    fn push_undo(&self, item: ClipItem) {
        let mut stack = self.undo.lock_ok();
        stack.undo.push(item);
        if stack.undo.len() > UNDO_CAPACITY {
            stack.undo.remove(0);
        }
        // 新的删除动作作废重做分支（与编辑器语义一致）。
        stack.redo.clear();
    }

    /// 把一条已删除的记录写回库（保留原 id / 收藏 / 标签等所有字段）。
    fn restore_item(&self, mut item: ClipItem) -> CommandResult<ClipItem> {
        item.preview = item.content.preview(200);
        // 同内容又被复制过：直接复用现有行，避免出现两条相同 hash 的记录。
        {
            let conn = self.db.lock_ok();
            // 撤销删除 = 内容是用户还要的：把墓碑一并撤掉，否则下次同步又会删掉它。
            conn.execute("DELETE FROM sync_tombstones WHERE hash = ?1", [&item.hash])?;
            let existing: Option<i64> = conn
                .query_row("SELECT id FROM clips WHERE hash = ?1 LIMIT 1", [&item.hash], |r| {
                    r.get(0)
                })
                .optional()?;
            if let Some(id) = existing {
                return Ok(ClipItem { id, ..item });
            }
        }
        let search_text = item.content.search_text();
        let content_json = serde_json::to_string(&item.content)?;
        let content_type = item.content.content_type().as_str().to_string();
        let conn = self.db.lock_ok();
        let id_taken: bool = conn
            .query_row("SELECT EXISTS(SELECT 1 FROM clips WHERE id = ?1)", [item.id], |r| {
                r.get(0)
            })
            .unwrap_or(false);
        if id_taken {
            // 原 id 已被新内容占用（删后又存了好几条）：退化为普通插入拿新 id。
            drop(conn);
            item.id = self.insert(item.clone())?;
            return Ok(item);
        }
        conn.execute(
            "INSERT INTO clips (
                id, content_type, content, search_text, preview, source_app, source_app_icon,
                created_at, last_used_at, pinned, favorite, sensitive,
                size_bytes, hash, sync_state, tag
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            params![
                item.id,
                content_type,
                content_json,
                search_text,
                item.preview,
                item.source_app,
                item.source_app_icon,
                item.created_at,
                item.last_used_at,
                item.pinned as i64,
                item.favorite as i64,
                item.sensitive as i64,
                item.size_bytes as i64,
                item.hash,
                sync_state_str(item.sync_state),
                item.tag,
            ],
        )?;
        Ok(item)
    }

    /// 列出库中已有的全部分组标签（供标签筛选用）。
    ///
    /// 不能从「当前查询结果」推导：一旦按标签过滤，其它标签就从结果里消失了，
    /// 用户再也切不回去（评审 G2）。
    pub fn tags(&self) -> CommandResult<Vec<String>> {
        let conn = self.db.lock_ok();
        let mut stmt = conn.prepare(
            "SELECT DISTINCT tag FROM clips WHERE tag IS NOT NULL AND tag <> '' ORDER BY tag",
        )?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// 回收无主图片文件（删除后未被撤销、或启动前遗留）。
    /// 返回删除的文件数。
    ///
    /// 比对的是**文件名**而不是完整路径：库里的绝对路径与当前 `files_dir()`
    /// 一旦不一致（数据目录改名、换登录用户），按路径比对就会把全部图片
    /// 判成孤儿一次删光。文件名（`clip_<hash>.png`）本身已经足够唯一。
    pub fn prune_orphan_images(&self) -> usize {
        let referenced: std::collections::HashSet<std::ffi::OsString> = {
            let conn = self.db.lock_ok();
            all_image_paths(&conn)
                .iter()
                .filter_map(|p| std::path::Path::new(p).file_name().map(|n| n.to_owned()))
                .collect()
        };
        let dir = crate::storage::files_dir();
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return 0;
        };
        let mut removed = 0;
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            if !referenced.contains(&entry.file_name()) && std::fs::remove_file(&path).is_ok() {
                removed += 1;
            }
        }
        removed
    }

    /// 清空全部历史。
    pub fn clear(&self) -> CommandResult<()> {
        let conn = self.db.lock_ok();
        let paths = all_image_paths(&conn);
        let now = now_ms();
        // 先把全部 hash 记成墓碑再删：否则每次同步都会把刚清空的库整批拉回来。
        // `WHERE true` 不能省：SQLite 解析 `INSERT ... SELECT ... ON CONFLICT` 时
        // 会把没有 WHERE 的 SELECT 误当成带子句的写法而报语法错误。
        conn.execute(
            "INSERT INTO sync_tombstones (hash, deleted_at) SELECT hash, ?1 FROM clips WHERE true
             ON CONFLICT(hash) DO UPDATE SET deleted_at = MAX(deleted_at, excluded.deleted_at)",
            params![now],
        )?;
        conn.execute("DELETE FROM clips", [])?;
        for path in paths {
            let _ = std::fs::remove_file(path);
        }
        Ok(())
    }

    /// 读取最近的墓碑（触发同步快照用），按删除时间倒序。
    pub fn tombstones(&self, limit: i64) -> CommandResult<Vec<(String, i64)>> {
        let conn = self.db.lock_ok();
        let mut stmt = conn.prepare(
            "SELECT hash, deleted_at FROM sync_tombstones ORDER BY deleted_at DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// 该内容是否已被墓碑标记（且墓碑不比条目本身旧）。
    ///
    /// 时间比较很重要：先删后重新复制的同一条内容应当能再次同步，
    /// 只有「删除发生在条目创建之后」才算真的不要了。
    pub fn tombstoned_after(&self, hash: &str, created_at: i64) -> CommandResult<bool> {
        let conn = self.db.lock_ok();
        let deleted_at: Option<i64> = conn
            .query_row(
                "SELECT deleted_at FROM sync_tombstones WHERE hash = ?1",
                [hash],
                |r| r.get(0),
            )
            .optional()?;
        Ok(matches!(deleted_at, Some(at) if at >= created_at))
    }

    /// 应用远端墓碑：别的设备删了这条，本机也删。
    ///
    /// - 只删「创建时间不晚于删除时间」的行（避免把删除后又重新复制的内容误删）；
    /// - 不进撤销栈（这不是用户在本机的操作，⌘Z 不该撤出一批别人删的东西）；
    /// - 顺手把墓碑写进本机，下次推送时继续传给其它设备。
    ///
    /// 返回是否真的删了一行。
    pub fn apply_remote_delete(&self, hash: &str, deleted_at: i64) -> CommandResult<bool> {
        let conn = self.db.lock_ok();
        let existing: Option<(i64, i64)> = conn
            .query_row(
                "SELECT id, created_at FROM clips WHERE hash = ?1 LIMIT 1",
                [hash],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        record_tombstone(&conn, hash, deleted_at);
        let Some((id, created_at)) = existing else {
            return Ok(false);
        };
        if created_at > deleted_at {
            return Ok(false);
        }
        conn.execute("DELETE FROM clips WHERE id = ?1", [id])?;
        Ok(true)
    }

    /// 置顶/取消置顶。
    ///
    /// 影响 0 行就不是「成功」：条目可能已被同步/另一个窗口删掉，静默成功会让
    /// 用户以为置顶生效了（下次刷新卡片又回到原位）。
    pub fn pin(&self, id: i64, pinned: bool) -> CommandResult<()> {
        let conn = self.db.lock_ok();
        let updated = conn.execute(
            "UPDATE clips SET pinned = ?1 WHERE id = ?2",
            params![pinned as i64, id],
        )?;
        ensure_existing(updated, id)
    }

    /// 收藏/取消收藏（同 [`HistoryService::pin`]，0 行即报「不存在」）。
    pub fn favorite(&self, id: i64, favorite: bool) -> CommandResult<()> {
        let conn = self.db.lock_ok();
        let updated = conn.execute(
            "UPDATE clips SET favorite = ?1 WHERE id = ?2",
            params![favorite as i64, id],
        )?;
        ensure_existing(updated, id)
    }

    /// 设置/清除分组标签（None / 空白 = 清除）。
    ///
    /// 空白要当成「清除」而不是写进库里：否则标签栏会多出一个永远点不中的空 chip。
    /// 长度也要卡住：标签会整条渲染在卡片底部，几千字的长文本会把布局撑坏。
    pub fn set_tag(&self, id: i64, tag: Option<String>) -> CommandResult<()> {
        let tag = tag
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty());
        if let Some(t) = tag.as_deref() {
            if t.chars().count() > MAX_TAG_CHARS {
                return Err(AppError::with_details(
                    CODE_INVALID_ARG,
                    "标签太长，无法保存",
                    format!("上限 {MAX_TAG_CHARS} 个字符"),
                ));
            }
        }
        let conn = self.db.lock_ok();
        let updated = conn.execute(
            "UPDATE clips SET tag = ?1 WHERE id = ?2",
            params![tag, id],
        )?;
        ensure_existing(updated, id)
    }

    /// 更新最近使用时间（回贴时调用）。
    pub fn touch(&self, id: i64) -> CommandResult<()> {
        let conn = self.db.lock_ok();
        conn.execute(
            "UPDATE clips SET last_used_at = ?1 WHERE id = ?2",
            params![now_ms(), id],
        )?;
        Ok(())
    }

    /// 统计信息。
    pub fn stats(&self) -> CommandResult<HistoryStats> {
        let conn = self.db.lock_ok();
        let (total, total_bytes, favorite) = conn.query_row(
            "SELECT COUNT(*), COALESCE(SUM(size_bytes), 0), \
             COALESCE(SUM(favorite), 0) FROM clips",
            [],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?)),
        )?;

        let mut by_type = std::collections::HashMap::new();
        let mut stmt = conn.prepare("SELECT content_type, COUNT(*) FROM clips GROUP BY content_type")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
        for row in rows {
            let (kind, count) = row?;
            by_type.insert(kind, count);
        }

        Ok(HistoryStats {
            total,
            total_bytes,
            by_type,
            favorite,
        })
    }

    /// 是否存在指定 hash 的条目（同步去重用）。
    pub fn exists_hash(&self, hash: &str) -> CommandResult<bool> {
        let conn = self.db.lock_ok();
        let id: Option<i64> = conn
            .query_row("SELECT id FROM clips WHERE hash = ?1 LIMIT 1", [hash], |r| r.get(0))
            .optional()?;
        Ok(id.is_some())
    }

    /// 返回全部条目（同步快照用）。
    pub fn all(&self) -> CommandResult<Vec<ClipItem>> {
        self.query(HistoryQuery {
            keyword: None,
            content_type: None,
            pinned_only: false,
            favorite_only: false,
            tag: None,
            limit: 100_000,
            offset: 0,
        })
    }

    /// 容量策略：超出上限时按「最旧优先」删除（优先删未置顶项）。
    /// 返回被删记录引用的图片文件路径（供调用方清理无主文件）。
    fn enforce_capacity(&self, conn: &rusqlite::Connection) -> rusqlite::Result<Vec<String>> {
        let cfg = self.config.get();
        // 0 表示「没设过」而不是「一条不留」：手改配置、旧版本遗留或一次坏 patch
        // 留下 0，会让下一次入库把历史整个删空（用户在删完之后才知道）。
        // 两种容量分别回落到出厂默认值。
        let max_items = if cfg.max_items == 0 {
            crate::config::settings::DEFAULT_MAX_ITEMS
        } else {
            cfg.max_items
        };
        let max_bytes = if cfg.max_bytes == 0 {
            crate::config::settings::DEFAULT_MAX_BYTES
        } else {
            cfg.max_bytes
        };
        let mut removed_paths: Vec<String> = Vec::new();

        // 时间限制（TTL）：淘汰超过 N 天的未置顶条目（0 表示无限制）。
        if cfg.history_ttl_days > 0 {
            let cutoff = now_ms() - (cfg.history_ttl_days as i64) * 86_400_000;
            collect_image_paths_where(
                conn,
                "SELECT content FROM clips WHERE pinned = 0 AND created_at < ?1 \
                 AND content_type = 'image'",
                params![cutoff],
                &mut removed_paths,
            )?;
            conn.execute(
                "DELETE FROM clips WHERE pinned = 0 AND created_at < ?1",
                params![cutoff],
            )?;
        }

        collect_image_paths_where(
            conn,
            "SELECT content FROM clips WHERE pinned = 0 AND content_type = 'image' \
             AND id NOT IN (SELECT id FROM clips ORDER BY pinned DESC, created_at DESC LIMIT ?1)",
            params![max_items as i64],
            &mut removed_paths,
        )?;
        conn.execute(
            "DELETE FROM clips WHERE pinned = 0 AND id NOT IN (
                SELECT id FROM clips ORDER BY pinned DESC, created_at DESC LIMIT ?1
            )",
            params![max_items as i64],
        )?;

        // 总字节数超限则循环删除最旧未置顶项（全部置顶时停止）。
        for _ in 0..max_items {
            let total: i64 = conn.query_row(
                "SELECT COALESCE(SUM(size_bytes), 0) FROM clips",
                [],
                |r| r.get(0),
            )?;
            if (total as u64) <= max_bytes {
                break;
            }
            collect_image_paths_where(
                conn,
                "SELECT content FROM clips WHERE content_type = 'image' AND id = (
                    SELECT id FROM clips WHERE pinned = 0 ORDER BY created_at ASC LIMIT 1
                )",
                params![],
                &mut removed_paths,
            )?;
            let deleted = conn.execute(
                "DELETE FROM clips WHERE id = (
                    SELECT id FROM clips WHERE pinned = 0 ORDER BY created_at ASC LIMIT 1
                )",
                [],
            )?;
            if deleted == 0 {
                break;
            }
        }
        Ok(removed_paths)
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// `UPDATE ... WHERE id = ?` 影响 0 行时返回统一的「记录不存在」。
///
/// 面板、历史页、托盘都是「先渲染、后操作」：同步或另一个窗口删掉条目后，
/// 用户手里的卡片仍然是旧的。静默成功会让人以为操作生效了，刷新一看又变回去。
fn ensure_existing(updated: usize, id: i64) -> CommandResult<()> {
    if updated == 0 {
        return Err(AppError::with_details(
            2001,
            "该记录不存在，可能已被删除",
            format!("id={id}"),
        ));
    }
    Ok(())
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

/// 写一条墓碑（同一 hash 保留最晚的删除时间）。
///
/// 墓碑只服务于同步：本地删除后不让远端快照把同一条内容再写回来。
/// 容量淘汰不写墓碑（那是本机自己的上限策略，不该反过来删掉别的设备上的内容）。
fn record_tombstone(conn: &rusqlite::Connection, hash: &str, deleted_at: i64) {
    let _ = conn.execute(
        "INSERT INTO sync_tombstones (hash, deleted_at) VALUES (?1, ?2)
         ON CONFLICT(hash) DO UPDATE SET deleted_at = MAX(deleted_at, excluded.deleted_at)",
        params![hash, deleted_at],
    );
}

/// 取出指定记录的图片文件路径（非图片记录返回空）。
fn image_paths_of(conn: &rusqlite::Connection, id: i64) -> Vec<String> {
    let json: Option<String> = conn
        .query_row(
            "SELECT content FROM clips WHERE id = ?1 AND content_type = 'image'",
            [id],
            |r| r.get(0),
        )
        .optional()
        .unwrap_or(None);
    json.and_then(|j| match serde_json::from_str::<ClipContent>(&j) {
        Ok(ClipContent::Image { path, .. }) => Some(path),
        _ => None,
    })
    .into_iter()
    .collect()
}

/// 取出全部图片记录的路径（清空历史时用）。
fn all_image_paths(conn: &rusqlite::Connection) -> Vec<String> {
    let mut out = Vec::new();
    let Ok(mut stmt) = conn.prepare("SELECT content FROM clips WHERE content_type = 'image'")
    else {
        return out;
    };
    let Ok(mut rows) = stmt.query([]) else {
        return out;
    };
    while let Ok(Some(row)) = rows.next() {
        let Ok(json) = row.get::<_, String>(0) else { continue };
        let Ok(ClipContent::Image { path, .. }) = serde_json::from_str(&json) else {
            continue;
        };
        out.push(path);
    }
    out
}

/// 收集将按条件删除的图片记录路径（供删除后清理文件）。
fn collect_image_paths_where(
    conn: &rusqlite::Connection,
    sql: &str,
    params: &[&dyn rusqlite::ToSql],
    out: &mut Vec<String>,
) -> rusqlite::Result<()> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(params, |r| r.get::<_, String>(0))?;
    for row in rows {
        let Ok(json) = row else { continue };
        if let Ok(ClipContent::Image { path, .. }) = serde_json::from_str(&json) {
            out.push(path);
        }
    }
    Ok(())
}

/// 删除不再被任何图片记录引用的文件（内容寻址后同一文件可被多行共享）。
fn prune_image_files(conn: &rusqlite::Connection, candidates: &[String]) {
    for path in candidates {
        let referenced: bool = conn
            .query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM clips
                    WHERE content_type = 'image' AND json_extract(content, '$.value.path') = ?1
                )",
                [path],
                |r| r.get(0),
            )
            .unwrap_or(true);
        if !referenced {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// 从旧版本写坏的 `content` 里把正文救回来（修复入口用）。
///
/// 已知的坏形态是「JSON 字符串值少了引号」：
/// `{"kind":"text","value":正文}`。只处理能把正文完整取出的情况，
/// 否则返回 `None` —— 宁可跳过让用户看到其它条目，也不瞎猜内容。
fn salvage_content(raw: &str, content_type: &str) -> Option<ClipContent> {
    let rest = raw.trim().strip_prefix('{')?;
    if !rest.starts_with("\"kind\"") {
        return None;
    }
    let body = rest.split_once("\"value\":")?.1;
    // 尾部的 `}` 是外层对象收尾；正文自身以 `}` 结尾时只剥一个。
    let body = body.strip_suffix('}').unwrap_or(body);
    let body = body.trim();
    // 引号成对残留时一并去掉（`"正文"` → `正文`）；只有一侧就不动，
    // 避免把正文自带的引号吃掉。首尾引号都是 ASCII，按字节切安全。
    let body = if body.len() >= 2 && body.starts_with('"') && body.ends_with('"') {
        &body[1..body.len() - 1]
    } else {
        body
    };
    if body.is_empty() {
        return None;
    }
    match content_type {
        "text" => Some(ClipContent::Text(body.to_string())),
        "html" => Some(ClipContent::Html(body.to_string())),
        _ => None,
    }
}

fn json_err(e: serde_json::Error) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
    )
}
