//! 数据库初始化（open + 迁移）测试。

use crate::storage::db::open;

/// 进程内自增序号：`SystemTime` 在 macOS 上实际分辨率约微秒，两个测试线程
/// 同时取时间会拿到同一个数，落成同一个库文件后第二个 `open` 撞见
/// 「table clips already exists」（`fts_trigger_keeps_index_in_sync` 随机红）。
static TEMP_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn temp_db_path() -> std::path::PathBuf {
    let unique = format!(
        "clipbase_test_{}_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        TEMP_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    );
    std::env::temp_dir().join(unique).join("data").join("clipbase.db")
}

#[test]
fn open_creates_parent_dirs_and_tables() {
    let path = temp_db_path();
    let conn = open(&path).expect("open should create db");

    let names: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap();
        stmt.query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect()
    };

    for expected in ["clips", "clips_fts", "config", "modules", "sync_state"] {
        assert!(names.contains(&expected.to_string()), "missing table {expected}, got {names:?}");
    }

    drop(conn);
    let _ = std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap());
}

#[test]
fn clips_table_has_source_app_icon_column() {
    let path = temp_db_path();
    let conn = open(&path).expect("open should create db");

    let columns: Vec<String> = {
        let mut stmt = conn.prepare("PRAGMA table_info(clips)").unwrap();
        stmt.query_map([], |r| r.get::<_, String>(1))
            .unwrap()
            .map(|r| r.unwrap())
            .collect()
    };
    assert!(
        columns.contains(&"source_app_icon".to_string()),
        "clips 表应包含 source_app_icon 列，got {columns:?}"
    );

    drop(conn);
    let _ = std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap());
}

#[test]
fn reopen_existing_db_is_idempotent() {
    let path = temp_db_path();
    {
        let _conn = open(&path).expect("first open");
    }
    {
        let conn = open(&path).expect("second open should not re-migrate/panic");
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='table'", [], |r| r.get(0))
            .unwrap();
        assert!(count >= 5);
    }
    let _ = std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap());
}

#[test]
fn fts_trigger_keeps_index_in_sync() {
    let path = temp_db_path();
    let conn = open(&path).unwrap();
    conn.execute(
        "INSERT INTO clips (content_type, content, search_text, preview, source_app, created_at, last_used_at, pinned, favorite, sensitive, size_bytes, hash, sync_state)
         VALUES ('text', '{\"kind\":\"text\",\"value\":\"hello world\"}', 'hello world', 'hello world', NULL, 1, 1, 0, 0, 0, 11, 'h1', 'local')",
        [],
    )
    .unwrap();
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM clips_fts WHERE clips_fts MATCH '\"hello\"'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 1);

    conn.execute("DELETE FROM clips WHERE hash = 'h1'", []).unwrap();
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM clips_fts WHERE clips_fts MATCH '\"hello\"'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 0);

    drop(conn);
    let _ = std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap());
}

/// 双开/并行首次启动：多个连接同时跑迁移不能把后到的那个拍死。
///
/// 单实例插件生效在数据库打开之后，所以这个窗口真实存在于双击启动的瞬间。
#[test]
fn concurrent_first_open_shares_one_migration() {
    let path = temp_db_path();
    let mut handles = Vec::new();
    for _ in 0..4 {
        let p = path.clone();
        handles.push(std::thread::spawn(move || open(&p).map(|conn| {
            drop(conn);
        })));
    }
    for h in handles {
        h.join().expect("open thread panicked").expect("concurrent open failed");
    }

    let conn = open(&path).expect("db should still be usable");
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='table'", [], |r| r.get(0))
        .unwrap();
    assert!(count >= 5, "got {count} tables");
    drop(conn);
    let _ = std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap());
}
