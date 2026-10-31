//! 从其它剪贴板工具迁移历史到 ClipBase。
//!
//! 导入数据由外部脚本产出（见 `scripts/import_icopy.py`），格式为 JSON：
//! `content` 字段直接就是 [`ClipContent`] 的 serde 表示，因此
//! **hash / preview / search_text / 序列化格式全部由 ClipBase 自身计算**，
//! 与正常捕获入库的记录完全同构。
//!
//! 迁移记录会**保留原始时间戳**（普通捕获用「当前时间」）。
//! 快捷回复功能已下线，导入只处理历史条目。

use serde::{Deserialize, Serialize};

use crate::config::settings::SensitivityPolicy;
use crate::core::clipboard::types::{hash_content, ClipContent, ClipItem, SyncState};
use crate::core::history::HistoryService;
use crate::core::sensitivity::{self, SensitivityDetector};
use crate::error::CommandResult;

/// 一条待导入的历史。
#[derive(Debug, Clone, Deserialize)]
pub struct ImportClip {
    /// 内容（serde 形态：`{"kind":"text","value":"..."}`）。
    pub content: ClipContent,
    /// 来源 App 名。
    #[serde(default)]
    pub source_app: Option<String>,
    /// 原始创建时间（Unix 毫秒）。
    #[serde(default)]
    pub created_at: i64,
    /// 是否收藏。
    #[serde(default)]
    pub favorite: bool,
    /// 分组标签。
    #[serde(default)]
    pub tag: Option<String>,
}

/// 导入文件整体结构。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ImportFile {
    #[serde(default)]
    pub clips: Vec<ImportClip>,
}

/// 导入结果摘要。
#[derive(Debug, Clone, Default, Serialize)]
pub struct ImportSummary {
    /// 新插入的历史条数。
    pub clips: usize,
    /// 与既有记录合并（去重/按时间原位更新）的条数。
    #[serde(default)]
    pub merged: usize,
    /// 失败/跳过的条数。
    pub skipped: usize,
    /// 失败明细。
    pub errors: Vec<String>,
}

/// 导入文件大小上限。
///
/// 整个文件是一次 `read` 进内存再 `serde_json::from_slice` 的：一个误选的
/// 视频文件（几个 GB）会直接把进程吃死。真实迁移数据（含几千条历史）
/// 通常只有几 MB，64 MB 已经非常宽松。
const MAX_IMPORT_BYTES: u64 = 64 * 1024 * 1024;

/// 把导入文件里的历史写入 ClipBase。
/// `replace` 为 true 时按「创建时间 + 来源 App」原位更新已有记录，
/// 供修复提取逻辑后重导入用（否则 hash 变化会新增重复行）。
/// `policy` 与剪贴板捕获共用同一套敏感内容规则（见 [`sensitivity::apply`]）。
///
/// **没有整体回滚，也不需要**：每条记录独立提交，hash 去重让重跑导入变成
/// 幂等的「补齐」操作；失败明细写在 [`ImportSummary::errors`] 里，用户改完
/// 源文件再跑一次即可，不会因为中途失败而丢掉已导入的部分。
pub fn import_from_file(
    path: &std::path::Path,
    history: &HistoryService,
    replace: bool,
    policy: SensitivityPolicy,
) -> CommandResult<ImportSummary> {
    let meta = std::fs::metadata(path).map_err(|e| {
        crate::error::AppError::with_details(2001, "读取导入文件失败", e.to_string())
    })?;
    if !meta.is_file() {
        return Err(crate::error::AppError::with_details(
            2001,
            "导入路径不是文件",
            path.display().to_string(),
        ));
    }
    if meta.len() == 0 {
        return Err(crate::error::AppError::with_details(
            2002,
            "导入文件是空的",
            path.display().to_string(),
        ));
    }
    if meta.len() > MAX_IMPORT_BYTES {
        return Err(crate::error::AppError::with_details(
            2002,
            "导入文件太大，无法处理",
            format!(
                "{} MB，上限 {} MB",
                meta.len() / (1024 * 1024),
                MAX_IMPORT_BYTES / (1024 * 1024)
            ),
        ));
    }
    let raw = std::fs::read(path).map_err(|e| {
        crate::error::AppError::with_details(2001, "读取导入文件失败", e.to_string())
    })?;
    let file: ImportFile = serde_json::from_slice(&raw).map_err(|e| {
        crate::error::AppError::with_details(2002, "解析导入文件失败", e.to_string())
    })?;
    Ok(import(file, history, replace, policy))
}

/// 导入内存中的历史。
pub fn import(
    file: ImportFile,
    history: &HistoryService,
    replace: bool,
    policy: SensitivityPolicy,
) -> ImportSummary {
    let mut summary = ImportSummary::default();
    let detector = SensitivityDetector::new();

    for clip in file.clips {
        // 迁移数据同样可能包含密码 / token：旧实现直接写 `sensitive: false`，
        // 迁移进来的密钥既没脱敏、又会被当成“可同步”推到云端。
        let Some((sensitive, content)) =
            sensitivity::apply(&detector, policy, clip.content)
        else {
            summary.skipped += 1;
            summary
                .errors
                .push("跳过一条疑似敏感内容（按当前敏感策略不入库）".to_string());
            continue;
        };
        let item = ClipItem {
            id: 0,
            content_type: content.content_type(),
            hash: hash_content(&content),
            content,
            preview: String::new(),
            source_app: clip.source_app,
            source_app_icon: None,
            tag: clip.tag,
            created_at: clip.created_at,
            last_used_at: 0,
            pinned: false,
            favorite: clip.favorite,
            sensitive,
            size_bytes: 0,
            sync_state: SyncState::Local,
        };
        // size_bytes 由 insert 内部按 content 计算，这里先补齐以保持字段一致。
        let mut item = item;
        item.size_bytes = item.content.size_bytes();
        let inserted = if replace {
            history.replace_by_time(item)
        } else {
            history
                .insert_with_time(item, clip.created_at)
                .map(|_| true)
        };
        match inserted {
            Ok(true) => summary.clips += 1,
            Ok(false) => summary.merged += 1,
            Err(e) => {
                summary.skipped += 1;
                summary.errors.push(format!("导入历史失败：{e}"));
            }
        }
    }

    summary
}
