//! 存储层门面：数据库连接 + 数据目录路径工具。

pub mod db;
pub mod migrations;

use std::path::PathBuf;

/// 旧版（ClipBase）数据目录名。改名后第一次启动时整目录搬过来。
const LEGACY_DIR_NAME: &str = ".clipbase";
/// 旧版数据库文件名；目录搬完后还要把它改成 `halo.db`。
const LEGACY_DB_NAME: &str = "clipbase.db";

/// 把 `~/.clipbase` 迁移成 `~/.halo`：只在目标不存在时搬，幂等。
///
/// 必须在 `logging::init()` 之前调用——日志初始化会先建出 `~/.halo/logs`，
/// 目标目录一旦存在，整目录 rename 就做不成了。
pub fn migrate_legacy_dir() {
    let Some(home) = dirs::home_dir() else {
        return;
    };
    let new = home.join(".halo");
    let old = home.join(LEGACY_DIR_NAME);
    let mut moved = false;
    if old.is_dir() && !new.exists() {
        match std::fs::rename(&old, &new) {
            Ok(()) => {
                eprintln!("已把旧数据目录 {} 迁移到 {}", old.display(), new.display());
                moved = true;
            }
            Err(e) => eprintln!("迁移旧数据目录失败：{e}"),
        }
    }
    let renamed = migrate_legacy_db();
    if moved || renamed {
        rewrite_legacy_paths(&home);
    }
}

/// 把 `data/clipbase.db` 改名成 `data/halo.db`。幂等。
///
/// `-wal`/`-shm` 必须跟着一起改：只改主文件的话 SQLite 会认不出旁边那份
/// 未 checkpoint 的日志，最近写入的剪贴历史就成孤儿了。
fn migrate_legacy_db() -> bool {
    let data = data_dir().join("data");
    let old = data.join(LEGACY_DB_NAME);
    let new = data.join("halo.db");
    if !old.is_file() || new.exists() {
        return false;
    }
    if let Err(e) = std::fs::rename(&old, &new) {
        eprintln!("迁移旧数据库失败：{e}");
        return false;
    }
    for suffix in ["-wal", "-shm"] {
        let from = data.join(format!("{LEGACY_DB_NAME}{suffix}"));
        if from.is_file() {
            let _ = std::fs::rename(&from, data.join(format!("halo.db{suffix}")));
        }
    }
    eprintln!("已把旧数据库改名成 {}", new.display());
    true
}

/// 把库里存的绝对路径前缀从 `.clipbase` 换成 `.halo`。
///
/// 图片条目的 `content` 是含绝对路径的 JSON，来源图标同理，都落在
/// `~/.halo/` 之下；不换前缀的话历史图片会被 asset 协议整片拒掉。
/// 只动 `image` 条目——文本条目里出现同样字样是用户自己的内容。
fn rewrite_legacy_paths(home: &std::path::Path) {
    let old_prefix = format!("{}/{LEGACY_DIR_NAME}/", home.display());
    let new_prefix = format!("{}/.halo/", home.display());
    let Ok(conn) = rusqlite::Connection::open(db_path()) else {
        return;
    };
    let mut changed = 0usize;
    for (table, column, filter) in [
        ("clips", "content", "AND content_type = 'image'"),
        ("clips", "source_app_icon", ""),
    ] {
        let sql = format!(
            "UPDATE {table} SET {column} = replace({column}, ?1, ?2) \
             WHERE {column} LIKE ?3 {filter}"
        );
        match conn.execute(&sql, rusqlite::params![old_prefix, new_prefix, format!("%{old_prefix}%")]) {
            Ok(n) => changed += n,
            Err(e) => eprintln!("重写旧路径失败（{table}.{column}）：{e}"),
        }
    }
    if changed > 0 {
        eprintln!("已重写 {changed} 条记录里的旧数据路径");
    }
}

/// 解析应用数据根目录（`~/.halo/`）。
pub fn data_dir() -> PathBuf {
    if let Some(home) = dirs::home_dir() {
        home.join(".halo")
    } else {
        PathBuf::from(".halo")
    }
}

/// 主数据库路径。
pub fn db_path() -> PathBuf {
    data_dir().join("data").join("halo.db")
}

/// 外置载荷文件目录（图片/富文本，P1 使用，P0 预留）。
pub fn files_dir() -> PathBuf {
    data_dir().join("data").join("files")
}

/// 来源 App 图标缓存目录（PNG，按 App 去重复用）。
pub fn icons_dir() -> PathBuf {
    data_dir().join("icons")
}

/// 已安装模块根目录。
pub fn modules_dir() -> PathBuf {
    data_dir().join("modules")
}

/// 日志目录。
pub fn logs_dir() -> PathBuf {
    data_dir().join("logs")
}

/// 确保应用数据目录结构存在，并收紧权限（0700）。
///
/// 这些目录里放着剪贴板原文、来源 App 图标、日志等用户内容，
/// 默认 `create_dir_all` 受 umask 影响会是 0755，同机其它用户可遍历。
pub fn ensure_dirs() -> std::io::Result<()> {
    for dir in [data_dir(), files_dir(), icons_dir(), modules_dir(), logs_dir()] {
        std::fs::create_dir_all(&dir)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
        }
    }
    Ok(())
}
