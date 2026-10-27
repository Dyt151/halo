//! 数据库版本化迁移（rusqlite_migration）。

use rusqlite_migration::{Migrations, M};

/// 返回迁移列表；新增 schema 变更时向后追加，不要修改已发布的迁移。
pub fn migrations() -> Migrations<'static> {
    Migrations::new(vec![
        // v1：初始 schema
        M::up(
            r#"
            CREATE TABLE clips (
                id           INTEGER PRIMARY KEY AUTOINCREMENT,
                content_type TEXT    NOT NULL,
                content      TEXT    NOT NULL,
                search_text  TEXT    NOT NULL DEFAULT '',
                preview      TEXT    NOT NULL DEFAULT '',
                source_app   TEXT,
                created_at   INTEGER NOT NULL,
                last_used_at INTEGER NOT NULL,
                pinned       INTEGER NOT NULL DEFAULT 0,
                favorite     INTEGER NOT NULL DEFAULT 0,
                sensitive    INTEGER NOT NULL DEFAULT 0,
                size_bytes   INTEGER NOT NULL DEFAULT 0,
                hash         TEXT    NOT NULL,
                sync_state   TEXT    NOT NULL DEFAULT 'local'
            );

            CREATE INDEX idx_clips_created_at ON clips(created_at DESC);
            CREATE INDEX idx_clips_hash ON clips(hash);
            CREATE INDEX idx_clips_content_type ON clips(content_type);

            CREATE VIRTUAL TABLE clips_fts USING fts5(
                preview,
                search_text,
                content='clips',
                content_rowid='id',
                tokenize='unicode61'
            );

            CREATE TRIGGER clips_ai AFTER INSERT ON clips BEGIN
                INSERT INTO clips_fts(rowid, preview, search_text)
                VALUES (new.id, new.preview, new.search_text);
            END;
            CREATE TRIGGER clips_ad AFTER DELETE ON clips BEGIN
                INSERT INTO clips_fts(clips_fts, rowid, preview, search_text)
                VALUES ('delete', old.id, old.preview, old.search_text);
            END;
            CREATE TRIGGER clips_au AFTER UPDATE ON clips BEGIN
                INSERT INTO clips_fts(clips_fts, rowid, preview, search_text)
                VALUES ('delete', old.id, old.preview, old.search_text);
                INSERT INTO clips_fts(rowid, preview, search_text)
                VALUES (new.id, new.preview, new.search_text);
            END;

            CREATE TABLE config (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            CREATE TABLE modules (
                id           TEXT PRIMARY KEY,
                name         TEXT NOT NULL,
                version      TEXT NOT NULL,
                manifest     TEXT NOT NULL,
                enabled      INTEGER NOT NULL DEFAULT 0,
                grants       TEXT NOT NULL DEFAULT '[]',
                installed_at INTEGER NOT NULL
            );

            CREATE TABLE sync_state (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            "#,
        ),
        // v2：来源 App 图标（PNG 文件绝对路径，导出失败/非 macOS 时为 NULL）
        M::up("ALTER TABLE clips ADD COLUMN source_app_icon TEXT;"),
        // v3：分组标签（NULL = 未分组）
        M::up("ALTER TABLE clips ADD COLUMN tag TEXT;"),
        // v4：快捷回复（分组 + 回复模板；files 为附件文件绝对路径的 JSON 数组）
        M::up(
            r#"
            CREATE TABLE reply_groups (
                id   INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT    NOT NULL,
                sort INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE replies (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                group_id   INTEGER NOT NULL,
                title      TEXT    NOT NULL DEFAULT '',
                content    TEXT    NOT NULL DEFAULT '',
                files      TEXT    NOT NULL DEFAULT '[]',
                sort       INTEGER NOT NULL DEFAULT 0,
                created_at REAL    NOT NULL DEFAULT 0
            );

            CREATE INDEX idx_replies_group_id ON replies(group_id);
            "#,
        ),
        // v5：同步墓碑。
        //
        // 同步是「整快照覆盖」，本地删掉的条目下次拉取时又会被远端快照写回来
        // （越是手动清理过的库，越会不停「长」回旧条目）。显式删除时记一条墓碑，
        // 合并阶段据此跳过远端同 hash 的条目，并把其它设备也删过的内容一并清掉。
        M::up(
            r#"
            CREATE TABLE sync_tombstones (
                hash       TEXT PRIMARY KEY,
                deleted_at INTEGER NOT NULL
            );
            "#,
        ),
    ])
}
