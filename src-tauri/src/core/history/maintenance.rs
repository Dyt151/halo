//! 历史维护：容量淘汰、尺寸回填、损坏修复与无主图片回收。

use super::*;

impl HistoryService {
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

    /// 容量策略：超出上限时按「最旧优先」删除（优先删未置顶项）。
    /// 返回被删记录引用的图片文件路径（供调用方清理无主文件）。
    pub(super) fn enforce_capacity(&self, conn: &rusqlite::Connection) -> rusqlite::Result<Vec<String>> {
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

/// 取出全部图片记录的路径（清空历史时用）。
pub(super) fn all_image_paths(conn: &rusqlite::Connection) -> Vec<String> {
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
pub(super) fn collect_image_paths_where(
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
pub(super) fn prune_image_files(conn: &rusqlite::Connection, candidates: &[String]) {
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
