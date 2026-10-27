//! 剪贴板与历史的核心数据类型（与前端 `src/lib/types.ts` 对齐）。

use serde::{Deserialize, Serialize};

/// 内容类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ContentType {
    Text,
    Html,
    Image,
    Files,
    #[default]
    Unknown,
}

impl ContentType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ContentType::Text => "text",
            ContentType::Html => "html",
            ContentType::Image => "image",
            ContentType::Files => "files",
            ContentType::Unknown => "unknown",
        }
    }
}

/// 剪贴板实际内容。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "lowercase")]
pub enum ClipContent {
    Text(String),
    Html(String),
    Image {
        path: String,
        width: u32,
        height: u32,
    },
    Files(Vec<String>),
}

impl ClipContent {
    /// 内容对应的类型（避免调用方手填 content_type 导致与内容不一致）。
    pub fn content_type(&self) -> ContentType {
        match self {
            ClipContent::Text(_) => ContentType::Text,
            ClipContent::Html(_) => ContentType::Html,
            ClipContent::Image { .. } => ContentType::Image,
            ClipContent::Files(_) => ContentType::Files,
        }
    }

    /// 内容的字节大小（用于容量上限统计）。
    pub fn size_bytes(&self) -> u64 {
        match self {
            ClipContent::Text(s) => s.len() as u64,
            ClipContent::Html(s) => s.len() as u64,
            // 图片按磁盘文件计（文件缺失时计 0）。
            ClipContent::Image { path, .. } => {
                std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
            }
            ClipContent::Files(files) => files.iter().map(|f| f.len() as u64).sum(),
        }
    }

    /// 用于全文检索的纯文本内容。
    pub fn search_text(&self) -> String {
        match self {
            ClipContent::Text(s) => s.clone(),
            ClipContent::Html(s) => strip_html(s),
            ClipContent::Image { .. } => String::new(),
            ClipContent::Files(files) => files.join(" "),
        }
    }

    /// 截断预览文本。
    pub fn preview(&self, max_chars: usize) -> String {
        let text = match self {
            ClipContent::Text(s) => s.clone(),
            ClipContent::Html(s) => strip_html(s),
            ClipContent::Image { path, .. } => return format!("🖼 {}", path),
            ClipContent::Files(files) => files.join(", "),
        };
        let text = text.replace(['\n', '\r', '\t'], " ");
        let mut chars = text.chars();
        let preview: String = chars.by_ref().take(max_chars).collect();
        if chars.next().is_some() {
            format!("{preview}…")
        } else {
            preview
        }
    }
}

/// 简单 HTML 去标签（避免引入重 HTML 解析库）。
fn strip_html(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    // 折叠连续空白
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 同步状态标记。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SyncState {
    #[default]
    Local,
    Synced,
    Dirty,
}

/// 历史条目。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClipItem {
    pub id: i64,
    pub content_type: ContentType,
    pub content: ClipContent,
    pub preview: String,
    pub source_app: Option<String>,
    /// 来源 App 图标 PNG 文件绝对路径（导出失败/非 macOS 为 None）。
    pub source_app_icon: Option<String>,
    /// 分组标签（None = 未分组）。
    pub tag: Option<String>,
    pub created_at: i64,
    pub last_used_at: i64,
    pub pinned: bool,
    pub favorite: bool,
    pub sensitive: bool,
    pub size_bytes: u64,
    pub hash: String,
    pub sync_state: SyncState,
}

/// 剪贴板监控产生的事件。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipEvent {
    pub content: ClipContent,
    pub source_app: Option<String>,
}

/// 历史查询条件。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HistoryQuery {
    pub keyword: Option<String>,
    pub content_type: Option<ContentType>,
    pub pinned_only: bool,
    /// 仅显示已收藏条目。
    #[serde(default)]
    pub favorite_only: bool,
    /// 按分组标签筛选：None = 不按标签筛选；Some(t) = 只返回 tag = t 的条目。
    #[serde(default)]
    pub tag: Option<String>,
    pub limit: u32,
    pub offset: u32,
}

/// 计算内容 sha256 指纹（十六进制），用于历史去重。
pub fn hash_content(content: &ClipContent) -> String {
    use sha2::{Digest, Sha256};
    let json = serde_json::to_string(content).unwrap_or_default();
    let digest = Sha256::digest(json.as_bytes());
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}
