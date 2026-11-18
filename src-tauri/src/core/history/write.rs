//! 历史写入：入库去重、删除/撤销、用户标记变更。

use super::maintenance::{all_image_paths, prune_image_files};
use super::*;

impl HistoryService {
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
