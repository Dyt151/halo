//! 命令行导入工具：把 JSON 格式的历史 / 回复导入 ClipBase 数据库。
//!
//! 用法：
//!     cargo run --bin clipbase_import -- <import.json> [--replace] [--fix-image-sizes] [--db <path>]
//!
//! JSON 由 `scripts/import_icopy.py` 产出（iCopy → ClipBase 迁移）。
//! 导入走 ClipBase 自己的 `HistoryService`，因此 hash / preview / 检索索引
//! 与正常捕获入库的记录完全一致，并保留原始时间戳。
//!
//! - `--replace`：按「创建时间 + 来源 App」原位更新既有记录（修复提取逻辑后
//!   重导入用；不加此参数时按 hash 去重，内容变化会产生新行）；
//! - `--fix-image-sizes`：按磁盘文件回填图片记录的 size_bytes 与缺失宽高（只维护不导入）；
//! - `--db <path>`：指定数据库路径（默认 `~/.clipbase/data/clipbase.db`，测试用）。

use std::sync::{Arc, Mutex};

use clipbase_lib::config::ConfigService;
use clipbase_lib::core::history::HistoryService;
use clipbase_lib::core::reply::ReplyService;
use clipbase_lib::import;
use clipbase_lib::storage;

fn main() {
    let mut path: Option<String> = None;
    let mut replace = false;
    let mut fix_image_sizes = false;
    let mut db_override: Option<String> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--replace" => replace = true,
            "--fix-image-sizes" => fix_image_sizes = true,
            "--db" => db_override = args.next(),
            other => path = Some(other.to_string()),
        }
    }

    if let Err(e) = storage::ensure_dirs() {
        eprintln!("创建数据目录失败：{e}");
        std::process::exit(1);
    }
    let db_path = db_override
        .map(std::path::PathBuf::from)
        .unwrap_or_else(storage::db_path);
    let conn = match storage::db::open(&db_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("打开数据库失败：{e}");
            std::process::exit(1);
        }
    };
    let db = Arc::new(Mutex::new(conn));
    let config = ConfigService::new(db.clone());
    let history = HistoryService::new(db.clone(), config.clone());
    let reply = ReplyService::new(db);
    if fix_image_sizes {
        match history.backfill_image_sizes() {
            Ok((fixed_sizes, fixed_dims)) => {
                println!("图片记录回填完成：size_bytes {fixed_sizes} 条，width/height {fixed_dims} 条");
            }
            Err(e) => {
                eprintln!("回填失败：{e}");
                std::process::exit(1);
            }
        }
        return;
    }

    let Some(path) = path.map(std::path::PathBuf::from) else {
        eprintln!("用法：clipbase_import <import.json> [--replace] [--fix-image-sizes] [--db <path>]");
        std::process::exit(2);
    };

    // 导入的条目与正常捕获走同一套敏感策略（否则迁移进来的密码会明文入库、
    // 并被当成非敏感条同步到云端）。
    let policy = config.get().sensitivity;
    match import::import_from_file(&path, &history, &reply, replace, policy) {
        Ok(summary) => {
            println!(
                "导入完成：历史新增 {} 条，合并 {} 条，回复 {} 条（新建分组 {} 个），跳过 {} 条{}",
                summary.clips,
                summary.merged,
                summary.replies,
                summary.groups,
                summary.skipped,
                if replace { "（--replace 模式：按时间原位更新）" } else { "" },
            );
            for e in &summary.errors {
                eprintln!("  ! {e}");
            }
        }
        Err(e) => {
            eprintln!("导入失败：{e}");
            std::process::exit(1);
        }
    }
}
