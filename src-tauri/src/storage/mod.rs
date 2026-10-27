//! 存储层门面：数据库连接 + 数据目录路径工具。

pub mod db;
pub mod migrations;
pub mod schema;

use std::path::PathBuf;

/// 解析应用数据根目录（`~/.clipbase/`）。
pub fn data_dir() -> PathBuf {
    if let Some(home) = dirs::home_dir() {
        home.join(".clipbase")
    } else {
        PathBuf::from(".clipbase")
    }
}

/// 主数据库路径。
pub fn db_path() -> PathBuf {
    data_dir().join("data").join("clipbase.db")
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

/// 确保应用数据目录结构存在。
pub fn ensure_dirs() -> std::io::Result<()> {
    std::fs::create_dir_all(files_dir())?;
    std::fs::create_dir_all(icons_dir())?;
    std::fs::create_dir_all(modules_dir())?;
    std::fs::create_dir_all(logs_dir())?;
    Ok(())
}
