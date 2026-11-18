//! 历史读取：查询、单条获取、标签与统计。

use super::*;

impl HistoryService {
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
}
