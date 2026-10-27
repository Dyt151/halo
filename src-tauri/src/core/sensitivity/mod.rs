//! 敏感内容识别（启发式规则，P1 架构预留，P0 提供基础规则）。

use serde::Serialize;

use super::clipboard::types::{hash_content, ClipContent};
use crate::config::settings::SensitivityPolicy;

/// 敏感内容检测结果。
#[derive(Debug, Clone, Serialize)]
pub struct SensitivityResult {
    pub sensitive: bool,
    pub reason: Option<String>,
}

impl SensitivityResult {
    pub fn clean() -> Self {
        Self {
            sensitive: false,
            reason: None,
        }
    }

    pub fn flagged(reason: impl Into<String>) -> Self {
        Self {
            sensitive: true,
            reason: Some(reason.into()),
        }
    }
}

/// 敏感内容检测器（启发式规则）。
#[derive(Clone)]
pub struct SensitivityDetector;

impl SensitivityDetector {
    pub fn new() -> Self {
        Self
    }

    /// 检测文本是否为密码/token 类敏感内容。
    pub fn detect(&self, content: &ClipContent) -> SensitivityResult {
        match content {
            ClipContent::Text(text) => Self::detect_text(text),
            ClipContent::Html(html) => Self::detect_text(html),
            // 图片/文件默认为非敏感（P1 可扩展）。
            ClipContent::Image { .. } | ClipContent::Files(_) => SensitivityResult::clean(),
        }
    }

    fn detect_text(text: &str) -> SensitivityResult {
        let trimmed = text.trim();
        // 过长的文本（如代码、段落）大概率不是单个密码/token。
        let short = trimmed.len() <= 400;

        if short {
            let lower = trimmed.to_lowercase();

            // 明确的密钥/token 前缀。
            const PREFIXES: &[&str] = &[
                "eyj",                      // JWT
                "ghp_", "gho_", "ghs_",     // GitHub token
                "xoxb-", "xoxp-", "xoxa-", // Slack token
                "akia", "asia",             // AWS access key
                "sk-",                      // OpenAI/常见 API key
                "-----begin ",              // PEM 私钥
                "ssh-rsa ", "ssh-ed25519 ", "ssh-dss ",
                "gpg-", "password", "passwd", "pwd", "secret", "token",
            ];
            for prefix in PREFIXES {
                if lower.starts_with(prefix) || lower.contains(prefix) {
                    return SensitivityResult::flagged(format!("matched pattern: {prefix}"));
                }
            }

            // 形如 key=value 且 key 命中敏感词。
            for line in trimmed.lines().take(20) {
                if let Some((k, v)) = line.split_once('=') {
                    let k = k.trim().to_lowercase();
                    let v = v.trim();
                    let key_hit = ["password", "passwd", "pwd", "secret", "token", "api_key", "apikey", "access_key"]
                        .iter()
                        .any(|kw| k.contains(kw));
                    if key_hit && !v.is_empty() {
                        return SensitivityResult::flagged(format!("credential pair: {k}=..."));
                    }
                }
            }
        }

        SensitivityResult::clean()
    }
}

impl Default for SensitivityDetector {
    fn default() -> Self {
        Self::new()
    }
}

/// 按策略处理一条内容，返回 `(是否敏感, 最终入库内容)`。
///
/// 这是「敏感内容到底怎么落地」的**唯一出口**：剪贴板捕获与历史迁移都必须
/// 走这里。迁移入口曾自己写 `sensitive: false` 然后直接插入，于是 iCopy 里的
/// 密码会绕过脱敏明文入库，还会因为「非敏感」而被同步到云端。
///
/// `None` 表示按策略丢弃（`Ignore`）。
pub fn apply(
    detector: &SensitivityDetector,
    policy: SensitivityPolicy,
    content: ClipContent,
) -> Option<(bool, ClipContent)> {
    let flagged = detector.detect(&content).sensitive;
    match (policy, flagged) {
        (SensitivityPolicy::Ignore, true) => None,
        (SensitivityPolicy::Mask, true) => Some((true, mask(&content))),
        (SensitivityPolicy::Encrypt, true) => {
            // 本地字段级加密还没落地（缺 Keychain 主密钥）：宁可降级为
            // 脱敏，也不能按明文存，否则这个选项就成了谎话。
            tracing::debug!("sensitivity=encrypt 暂未实现本地加密，本条按脱敏处理");
            Some((true, mask(&content)))
        }
        _ => Some((flagged, content)),
    }
}

/// 脱敏：用**原文 hash 的短前缀**代替内容。
///
/// 不能所有敏感内容都写同一个占位串：条目按 hash 去重，那样连着复制两个
/// 不同的密码只会剩一条，用户会以为「后面的密码没记住」。短前缀不含原文，
/// 但能让不同密码落成不同条目。
pub fn mask(content: &ClipContent) -> ClipContent {
    let digest = hash_content(content);
    ClipContent::Text(format!("[敏感内容已脱敏 {}]", &digest[..8]))
}
