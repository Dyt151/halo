//! FTS5 查询构造：根据 `HistoryQuery` 生成 SQL 与绑定参数。

use rusqlite::types::Value;

use super::super::clipboard::types::HistoryQuery;

/// 将用户关键词转成安全的 FTS5 MATCH 查询串（每个词双引号包裹，隐式 AND）。
pub fn fts_query(keyword: &str) -> String {
    keyword
        .split_whitespace()
        .map(|term| format!("\"{}\"", term.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" ")
}

/// 构造历史查询语句与参数。
///
/// 返回 `(sql, params)`，sql 以 `SELECT` 开头（调用方负责追加排序/分页）。
pub fn build_query(q: &HistoryQuery) -> (String, Vec<Value>) {
    let mut sql = String::from(
        "SELECT id, content_type, content, preview, source_app, created_at, \
         last_used_at, pinned, favorite, sensitive, size_bytes, hash, sync_state, \
         source_app_icon, tag \
         FROM clips c",
    );
    let mut wheres: Vec<String> = Vec::new();
    let mut params: Vec<Value> = Vec::new();

    if let Some(keyword) = q.keyword.as_deref().map(str::trim) {
        if !keyword.is_empty() {
            let fts = fts_query(keyword);
            wheres.push("c.id IN (SELECT rowid FROM clips_fts WHERE clips_fts MATCH ?)".into());
            params.push(Value::Text(fts));
        }
    }

    if let Some(ct) = q.content_type {
        wheres.push("c.content_type = ?".into());
        params.push(Value::Text(ct.as_str().to_string()));
    }

    if q.pinned_only {
        wheres.push("c.pinned = 1".into());
    }

    if q.favorite_only {
        wheres.push("c.favorite = 1".into());
    }

    if let Some(tag) = q.tag.as_deref() {
        wheres.push("c.tag = ?".into());
        params.push(Value::Text(tag.to_string()));
    }

    if !wheres.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&wheres.join(" AND "));
    }

    (sql, params)
}
