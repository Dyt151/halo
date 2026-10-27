//! 快捷回复服务：分组 + 回复模板的 CRUD、模糊查询与简化排序。

pub mod xlsx;

use std::sync::{Arc, Mutex};

use rusqlite::{params, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use crate::core::clipboard::types::ClipContent;
use crate::error::{AppError, CommandResult};
use crate::lock_ext::LockExt;
use xlsx::{GroupTarget, ImportRow, ImportSummary};

/// 回复分组。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReplyGroup {
    pub id: i64,
    pub name: String,
    pub sort: i64,
}

/// 回复模板（`files` 为附件文件绝对路径列表）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reply {
    pub id: i64,
    pub group_id: i64,
    pub title: String,
    pub content: String,
    pub files: Vec<String>,
    pub sort: i64,
    pub created_at: f64,
}

impl Reply {
    /// 转为剪贴板内容：有附件时写入文件列表（多文件 URL），否则写入纯文本。
    pub fn to_clip_content(&self) -> ClipContent {
        if self.files.is_empty() {
            ClipContent::Text(self.content.clone())
        } else {
            ClipContent::Files(self.files.clone())
        }
    }
}

/// 快捷回复服务（与 `HistoryService` 相同方式持有数据库连接）。
#[derive(Clone)]
pub struct ReplyService {
    db: Arc<Mutex<rusqlite::Connection>>,
}

/// 参数不合法（空的/超长的分组名与标题等）。
const CODE_INVALID_ARG: i32 = 2006;

/// 分组名的字符上限：分组会整条渲染在左侧栏，过长会直接把布局挤爆。
const MAX_GROUP_CHARS: usize = 32;

/// 回复标题的字符上限（内容不卡，只卡标题）。
const MAX_TITLE_CHARS: usize = 200;

/// 校验并归一化分组名：去首尾空白、不能为空、不能超长。
///
/// 空白分组名在 UI 上是一个点不中的空标签，删也删不对，不如在入口就拒绝。
fn valid_group_name(name: String) -> CommandResult<String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::new(CODE_INVALID_ARG, "分组名不能为空"));
    }
    if name.chars().count() > MAX_GROUP_CHARS {
        return Err(AppError::with_details(
            CODE_INVALID_ARG,
            "分组名太长",
            format!("上限 {MAX_GROUP_CHARS} 个字符"),
        ));
    }
    Ok(name)
}

/// 校验并归一化回复标题。
///
/// 标题**允许为空**：界面支持「只挂附件」的回复（标题留空、正文也空，
/// 列表里用附件名兜底）。只卡长度——标题会整行渲染在列表里。
fn normalize_title(title: String) -> CommandResult<String> {
    let title = title.trim().to_string();
    if title.chars().count() > MAX_TITLE_CHARS {
        return Err(AppError::with_details(
            CODE_INVALID_ARG,
            "标题太长",
            format!("上限 {MAX_TITLE_CHARS} 个字符"),
        ));
    }
    Ok(title)
}

/// 一条回复不能「什么都没有」：没标题、没正文、也没附件时回贴的是一片空白。
fn ensure_reply_has_body(title: &str, content: &str, files: &[String]) -> CommandResult<()> {
    if title.is_empty() && content.trim().is_empty() && files.is_empty() {
        return Err(AppError::new(
            CODE_INVALID_ARG,
            "标题、内容、附件不能全部为空",
        ));
    }
    Ok(())
}

impl ReplyService {
    pub fn new(db: Arc<Mutex<rusqlite::Connection>>) -> Self {
        Self { db }
    }

    // ---------- 分组 ----------

    /// 全部分组（按 sort 升序）。
    pub fn list_groups(&self) -> CommandResult<Vec<ReplyGroup>> {
        let conn = self.db.lock_ok();
        let mut stmt = conn.prepare("SELECT id, name, sort FROM reply_groups ORDER BY sort ASC, id ASC")?;
        let rows = stmt.query_map([], Self::row_to_group)?;
        let mut groups = Vec::new();
        for row in rows {
            groups.push(row?);
        }
        Ok(groups)
    }

    /// 新建分组（sort 追加到末尾），返回完整分组。
    pub fn add_group(&self, name: String) -> CommandResult<ReplyGroup> {
        let name = valid_group_name(name)?;
        let conn = self.db.lock_ok();
        let next_sort: i64 = conn.query_row(
            "SELECT COALESCE(MAX(sort), -1) + 1 FROM reply_groups",
            [],
            |r| r.get(0),
        )?;
        conn.execute(
            "INSERT INTO reply_groups (name, sort) VALUES (?1, ?2)",
            params![name, next_sort],
        )?;
        let id = conn.last_insert_rowid();
        Ok(ReplyGroup {
            id,
            name,
            sort: next_sort,
        })
    }

    /// 重命名分组。
    pub fn rename_group(&self, id: i64, name: String) -> CommandResult<()> {
        let name = valid_group_name(name)?;
        let conn = self.db.lock_ok();
        let updated = conn.execute(
            "UPDATE reply_groups SET name = ?1 WHERE id = ?2",
            params![name, id],
        )?;
        if updated == 0 {
            return Err(AppError::with_details(
                2004,
                "回复分组不存在，可能已被删除",
                format!("id={id}"),
            ));
        }
        Ok(())
    }

    /// 删除分组（级联删除组内全部回复）。
    pub fn delete_group(&self, id: i64) -> CommandResult<()> {
        let mut conn = self.db.lock_ok();
        let tx = conn.transaction()?;
        let updated = tx.execute("DELETE FROM reply_groups WHERE id = ?1", params![id])?;
        if updated == 0 {
            return Err(AppError::with_details(
                2004,
                "回复分组不存在，可能已被删除",
                format!("id={id}"),
            ));
        }
        tx.execute("DELETE FROM replies WHERE group_id = ?1", params![id])?;
        tx.commit()?;
        Ok(())
    }

    // ---------- 回复 ----------

    /// 查询回复：`group_id` 为 None 时返回全部；`keyword` 对 title/content 做 LIKE 模糊匹配。
    pub fn list_replies(
        &self,
        group_id: Option<i64>,
        keyword: Option<String>,
    ) -> CommandResult<Vec<Reply>> {
        let mut sql = String::from(
            "SELECT id, group_id, title, content, files, sort, created_at FROM replies WHERE 1 = 1",
        );
        let mut params: Vec<rusqlite::types::Value> = Vec::new();
        if let Some(gid) = group_id {
            sql.push_str(" AND group_id = ?");
            params.push(gid.into());
        }
        if let Some(kw) = keyword {
            let kw = kw.trim();
            if !kw.is_empty() {
                sql.push_str(" AND (title LIKE ? ESCAPE '\\' OR content LIKE ? ESCAPE '\\')");
                let pattern = format!("%{}%", escape_like(kw));
                params.push(pattern.clone().into());
                params.push(pattern.into());
            }
        }
        sql.push_str(" ORDER BY sort ASC, id ASC");

        let conn = self.db.lock_ok();
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(params), Self::row_to_reply)?;
        let mut replies = Vec::new();
        for row in rows {
            replies.push(row?);
        }
        Ok(replies)
    }

    /// 按 id 获取单条回复。
    pub fn get_reply(&self, id: i64) -> CommandResult<Reply> {
        let conn = self.db.lock_ok();
        // 只有「查不到」才是 2005；把 rusqlite 的错误原文当 details 会在
        // 界面上露出 `Query returned no rows` 这种英文内部文案。
        conn.query_row(
            "SELECT id, group_id, title, content, files, sort, created_at \
             FROM replies WHERE id = ?1",
            [id],
            Self::row_to_reply,
        )
        .optional()?
        .ok_or_else(|| {
            AppError::with_details(2005, "快捷回复不存在，可能已被删除", format!("id={id}"))
        })
    }

    /// 新建回复（sort 在组内追加到末尾），返回完整回复。
    pub fn add_reply(
        &self,
        group_id: i64,
        title: String,
        content: String,
        files: Vec<String>,
    ) -> CommandResult<Reply> {
        let title = normalize_title(title)?;
        ensure_reply_has_body(&title, &content, &files)?;
        let files_json = serde_json::to_string(&files)?;
        let created_at = now_ms_f64();
        let conn = self.db.lock_ok();
        // 分组不存在时直接拒绝：`replies.group_id` 没有外键约束，硬插进去
        // 就是一条「哪都看不到」的孤儿记录。
        if !group_exists(&conn, group_id)? {
            return Err(AppError::with_details(
                2004,
                "回复分组不存在",
                format!("group_id={group_id}"),
            ));
        }
        let next_sort: i64 = conn.query_row(
            "SELECT COALESCE(MAX(sort), -1) + 1 FROM replies WHERE group_id = ?1",
            [group_id],
            |r| r.get(0),
        )?;
        conn.execute(
            "INSERT INTO replies (group_id, title, content, files, sort, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![group_id, title, content, files_json, next_sort, created_at],
        )?;
        let id = conn.last_insert_rowid();
        Ok(Reply {
            id,
            group_id,
            title,
            content,
            files,
            sort: next_sort,
            created_at,
        })
    }

    /// 更新回复的标题、内容与附件（不动排序）。
    pub fn update_reply(
        &self,
        id: i64,
        title: String,
        content: String,
        files: Vec<String>,
    ) -> CommandResult<()> {
        let title = normalize_title(title)?;
        ensure_reply_has_body(&title, &content, &files)?;
        let files_json = serde_json::to_string(&files)?;
        let conn = self.db.lock_ok();
        let updated = conn.execute(
            "UPDATE replies SET title = ?1, content = ?2, files = ?3 WHERE id = ?4",
            params![title, content, files_json, id],
        )?;
        if updated == 0 {
            return Err(AppError::with_details(
                2005,
                "快捷回复不存在，可能已被删除",
                format!("id={id}"),
            ));
        }
        Ok(())
    }

    /// 删除单条回复。
    pub fn delete_reply(&self, id: i64) -> CommandResult<()> {
        let conn = self.db.lock_ok();
        let removed = conn.execute("DELETE FROM replies WHERE id = ?1", params![id])?;
        if removed == 0 {
            return Err(AppError::with_details(
                2005,
                "快捷回复不存在，可能已被删除",
                format!("id={id}"),
            ));
        }
        Ok(())
    }

    /// Excel 导入：按分组名归组（不存在则创建，同一批次内只建一次），逐条插入；
    /// 单条插入失败收集到 `ImportSummary::errors`，不中断整体导入。
    pub fn import_replies(&self, rows: &[ImportRow]) -> CommandResult<ImportSummary> {
        let mut summary = ImportSummary::default();
        // (id, name) 缓存：导入过程中新建的分组也并入，避免同批次重复建组。
        let mut groups: Vec<(i64, String)> = self
            .list_groups()?
            .into_iter()
            .map(|g| (g.id, g.name))
            .collect();

        for row in rows {
            let name = xlsx::normalize_group_name(row.group.as_deref());
            let group_id = match xlsx::resolve_group_target(&name, &groups) {
                GroupTarget::Existing(id) => id,
                GroupTarget::Create => match self.add_group(name.clone()) {
                    Ok(g) => {
                        groups.push((g.id, g.name.clone()));
                        summary.groups_created += 1;
                        g.id
                    }
                    Err(e) => {
                        summary
                            .errors
                            .push(format!("创建分组「{name}」失败：{e}"));
                        continue;
                    }
                },
            };
            match self.add_reply(group_id, row.title.clone(), row.content.clone(), vec![]) {
                Ok(_) => summary.imported += 1,
                Err(e) => summary
                    .errors
                    .push(format!("导入「{}」失败：{e}", row.title)),
            }
        }
        Ok(summary)
    }

    /// 同组内相邻排序交换（简化版拖拽排序）：正数上移，负数下移，0 不动；
    /// 已在组内首/末位时为无操作。
    pub fn move_reply(&self, id: i64, direction: i32) -> CommandResult<()> {
        if direction == 0 {
            return Ok(());
        }
        let mut conn = self.db.lock_ok();
        let row: Option<(i64, i64)> = conn
            .query_row(
                "SELECT group_id, sort FROM replies WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let (group_id, sort) = row.ok_or_else(|| {
            AppError::with_details(2005, "快捷回复不存在，可能已被删除", format!("id={id}"))
        })?;

        // 上移找组内前一项（sort 更小，sort 相同则 id 更小）；下移找后一项。
        let neighbor: Option<(i64, i64)> = if direction > 0 {
            conn.query_row(
                "SELECT id, sort FROM replies \
                 WHERE group_id = ?1 AND (sort < ?2 OR (sort = ?2 AND id < ?3)) \
                 ORDER BY sort DESC, id DESC LIMIT 1",
                params![group_id, sort, id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?
        } else {
            conn.query_row(
                "SELECT id, sort FROM replies \
                 WHERE group_id = ?1 AND (sort > ?2 OR (sort = ?2 AND id > ?3)) \
                 ORDER BY sort ASC, id ASC LIMIT 1",
                params![group_id, sort, id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?
        };

        if let Some((other_id, other_sort)) = neighbor {
            let tx = conn.transaction()?;
            tx.execute(
                "UPDATE replies SET sort = ?1 WHERE id = ?2",
                params![other_sort, id],
            )?;
            tx.execute(
                "UPDATE replies SET sort = ?1 WHERE id = ?2",
                params![sort, other_id],
            )?;
            tx.commit()?;
        }
        Ok(())
    }

    fn row_to_group(row: &Row<'_>) -> rusqlite::Result<ReplyGroup> {
        Ok(ReplyGroup {
            id: row.get(0)?,
            name: row.get(1)?,
            sort: row.get(2)?,
        })
    }

    fn row_to_reply(row: &Row<'_>) -> rusqlite::Result<Reply> {
        let files_json: String = row.get(4)?;
        // files 由本服务写入，解析失败时降级为空列表并告警，避免单行数据弄坏整张列表。
        let files = match serde_json::from_str(&files_json) {
            Ok(f) => f,
            Err(e) => {
                tracing::warn!("invalid reply files json: {e}");
                Vec::new()
            }
        };
        Ok(Reply {
            id: row.get(0)?,
            group_id: row.get(1)?,
            title: row.get(2)?,
            content: row.get(3)?,
            files,
            sort: row.get(5)?,
            created_at: row.get(6)?,
        })
    }
}

/// 转义 LIKE 特殊字符（`%`、`_`、`\`），配合 `ESCAPE '\'` 使用。
fn escape_like(s: &str) -> String {
    s.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}

fn now_ms_f64() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as f64)
        .unwrap_or(0.0)
}

/// 分组是否存在（插入回复前的外键替代检查）。
fn group_exists(conn: &rusqlite::Connection, id: i64) -> CommandResult<bool> {
    let found: Option<i64> = conn
        .query_row("SELECT id FROM reply_groups WHERE id = ?1", [id], |r| {
            r.get(0)
        })
        .optional()?;
    Ok(found.is_some())
}
