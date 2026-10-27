//! 格式枚举与扩展名探测（纯函数，无 I/O、无副作用）。
//!
//! 格式是路由图的**节点**：能力不写在这里，而由各引擎声明的边给出
//! （见 `edge.rs` / `router.rs`）。枚举只回答「这是什么格式、是不是图片、
//! 能不能带透明通道」这类事实。

use std::path::Path;

use serde::{Deserialize, Deserializer, Serialize};

/// JPEG 输出默认质量（1-100）。
pub const DEFAULT_JPEG_QUALITY: u8 = 85;

/// 转换支持的格式（序列化取值 = 小写扩展名，与前端 `ConvertFormat` 对齐）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ConvertFormat {
    Png,
    Jpg,
    Webp,
    Gif,
    Bmp,
    Tiff,
    Md,
    Html,
    Txt,
}

impl ConvertFormat {
    /// 规范化字符串（小写扩展名，也是输出文件的后缀）。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpg => "jpg",
            Self::Webp => "webp",
            Self::Gif => "gif",
            Self::Bmp => "bmp",
            Self::Tiff => "tiff",
            Self::Md => "md",
            Self::Html => "html",
            Self::Txt => "txt",
        }
    }

    /// 展示名（错误信息 / UI 标签）。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Png => "PNG",
            Self::Jpg => "JPG",
            Self::Webp => "WebP",
            Self::Gif => "GIF",
            Self::Bmp => "BMP",
            Self::Tiff => "TIFF",
            Self::Md => "Markdown",
            Self::Html => "HTML",
            Self::Txt => "TXT",
        }
    }

    /// 是否图片格式（决定走哪个象限的引擎）。
    pub fn is_image(&self) -> bool {
        matches!(
            self,
            Self::Png | Self::Jpg | Self::Webp | Self::Gif | Self::Bmp | Self::Tiff
        )
    }

    /// 是否文本格式（md / html / txt）。
    pub fn is_text(&self) -> bool {
        !self.is_image()
    }

    /// 该格式能否承载透明通道。
    ///
    /// JPEG 只有 YCbCr，转过去必须先合成背景色——不声明就会得到随机黑底
    /// （透明像素底下的 RGB 往往是 0）。
    pub fn supports_alpha(&self) -> bool {
        matches!(
            self,
            Self::Png | Self::Webp | Self::Gif | Self::Bmp | Self::Tiff
        )
    }

    /// 按扩展名解析（大小写不敏感，含常见别名）。
    pub fn from_ext(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().as_str() {
            "png" => Some(Self::Png),
            "jpg" | "jpeg" | "jpe" => Some(Self::Jpg),
            "webp" => Some(Self::Webp),
            "gif" => Some(Self::Gif),
            "bmp" => Some(Self::Bmp),
            "tif" | "tiff" => Some(Self::Tiff),
            "md" | "markdown" | "mdown" | "mkd" => Some(Self::Md),
            "html" | "htm" | "xhtml" => Some(Self::Html),
            "txt" | "text" => Some(Self::Txt),
            _ => None,
        }
    }
}

impl<'de> Deserialize<'de> for ConvertFormat {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Self::from_ext(&raw)
            .ok_or_else(|| serde::de::Error::custom(format!("未知的格式：{raw}")))
    }
}

/// 全部支持格式（路由与候选目标的遍历顺序）。
pub const ALL_FORMATS: [ConvertFormat; 9] = [
    ConvertFormat::Png,
    ConvertFormat::Jpg,
    ConvertFormat::Webp,
    ConvertFormat::Gif,
    ConvertFormat::Bmp,
    ConvertFormat::Tiff,
    ConvertFormat::Md,
    ConvertFormat::Html,
    ConvertFormat::Txt,
];

/// 全部图片格式。
pub const IMAGE_FORMATS: [ConvertFormat; 6] = [
    ConvertFormat::Png,
    ConvertFormat::Jpg,
    ConvertFormat::Webp,
    ConvertFormat::Gif,
    ConvertFormat::Bmp,
    ConvertFormat::Tiff,
];

/// 全部文本格式。
pub const TEXT_FORMATS: [ConvertFormat; 3] = [
    ConvertFormat::Md,
    ConvertFormat::Html,
    ConvertFormat::Txt,
];

/// 纯函数：按扩展名探测文件格式；无法识别（无扩展名 / 不在支持列表）返回 `None`。
pub fn detect_format(path: &str) -> Option<ConvertFormat> {
    let ext = Path::new(path).extension()?.to_str()?;
    ConvertFormat::from_ext(ext)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_common_extensions() {
        assert_eq!(detect_format("/a/b/foo.png"), Some(ConvertFormat::Png));
        assert_eq!(detect_format("foo.jpg"), Some(ConvertFormat::Jpg));
        assert_eq!(detect_format("foo.jpeg"), Some(ConvertFormat::Jpg));
        assert_eq!(detect_format("foo.webp"), Some(ConvertFormat::Webp));
        assert_eq!(detect_format("foo.gif"), Some(ConvertFormat::Gif));
        assert_eq!(detect_format("foo.bmp"), Some(ConvertFormat::Bmp));
        assert_eq!(detect_format("foo.tiff"), Some(ConvertFormat::Tiff));
        assert_eq!(detect_format("foo.tif"), Some(ConvertFormat::Tiff));
        assert_eq!(detect_format("foo.md"), Some(ConvertFormat::Md));
        assert_eq!(detect_format("foo.markdown"), Some(ConvertFormat::Md));
        assert_eq!(detect_format("foo.html"), Some(ConvertFormat::Html));
        assert_eq!(detect_format("foo.htm"), Some(ConvertFormat::Html));
        assert_eq!(detect_format("foo.txt"), Some(ConvertFormat::Txt));
    }

    #[test]
    fn detect_is_case_insensitive_and_rejects_unknown() {
        assert_eq!(detect_format("FOO.PNG"), Some(ConvertFormat::Png));
        assert_eq!(detect_format("Foo.Jpg"), Some(ConvertFormat::Jpg));
        assert_eq!(detect_format("foo.PdF"), None);
        assert_eq!(detect_format("noext"), None);
        assert_eq!(detect_format(""), None);
    }

    #[test]
    fn alpha_capability_matches_real_encoders() {
        // JPEG 不能带 alpha，其余图片格式都能。
        assert!(!ConvertFormat::Jpg.supports_alpha());
        for f in IMAGE_FORMATS {
            assert_eq!(f.supports_alpha(), f != ConvertFormat::Jpg, "{f:?}");
        }
        assert!(TEXT_FORMATS.iter().all(|f| !f.supports_alpha()));
    }
}
