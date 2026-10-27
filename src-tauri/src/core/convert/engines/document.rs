//! 图片 → 文档引擎：把一张图变成「能打开的文档」。
//!
//! 两种产物的语义**不同**，必须说清楚（§5.1 / 验收 P0 最后一条）：
//! - **HTML**：图片以 base64 data URI **真正内联**，产物是自包含的单文件；
//! - **Markdown**：只写一行 `![](路径)`，**引用**源图，移动或删除源图就会失效。
//!
//! 因此 HTML 只接浏览器能直接渲染的图片格式（PNG/JPG/WebP/GIF）；TIFF、BMP
//! 会由路由器先转成 PNG（`tiff → png → html`，两步），而不是塞一个浏览器打不开
//! 的 data URI 进去。

use std::path::Path;

use base64::Engine;

use super::super::edge::{
    ConversionEdge, ConversionStep, ConvertError, ConverterEngine, EdgeMode, EngineId,
    EngineStatus, StepOutput,
};
use super::super::format::ConvertFormat;
use super::super::text::escape_html;

/// 能直接内联进 HTML 的格式。
const HTML_EMBEDDABLE: [ConvertFormat; 4] = [
    ConvertFormat::Png,
    ConvertFormat::Jpg,
    ConvertFormat::Webp,
    ConvertFormat::Gif,
];

/// 图片 → 文档引擎。
pub struct DocumentEngine;

/// 内联成 HTML 的原图上限（24 MB）。
///
/// base64 会把体积再抬 1/3，而且整页 HTML 要一次性拼成字符串：
/// 一张 200 MB 的 TIFF 转成 HTML 是 270 MB 字符串，直接把内存拉满。
const MAX_INLINE_BYTES: u64 = 24 * 1024 * 1024;

impl ConverterEngine for DocumentEngine {
    fn id(&self) -> EngineId {
        EngineId::Document
    }

    fn display_name(&self) -> &'static str {
        "图片转文档"
    }

    fn max_parallelism(&self) -> usize {
        2
    }

    fn status(&self) -> EngineStatus {
        EngineStatus::Available
    }

    fn edges(&self) -> Vec<ConversionEdge> {
        let mut out = Vec::new();
        for from in super::super::format::IMAGE_FORMATS {
            // 引用式：任何图片都能被引用。
            out.push(ConversionEdge::new(
                from,
                ConvertFormat::Md,
                EngineId::Document,
                20,
                EdgeMode::Terminal,
            ));
            if HTML_EMBEDDABLE.contains(&from) {
                out.push(ConversionEdge::new(
                    from,
                    ConvertFormat::Html,
                    EngineId::Document,
                    20,
                    EdgeMode::Terminal,
                ));
            }
        }
        out
    }

    fn convert(&self, step: &ConversionStep<'_>) -> Result<StepOutput, ConvertError> {
        let title = step
            .original
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("image")
            .to_string();

        let content = match step.to {
            ConvertFormat::Md => {
                let reference = markdown_reference(step.original, step.output);
                format!(
                    "![{}]({})\n",
                    escape_md_text(&title),
                    escape_md_url(&reference)
                )
            }
            ConvertFormat::Html => {
                if let Ok(meta) = std::fs::metadata(step.input) {
                    if meta.len() > MAX_INLINE_BYTES {
                        return Err(ConvertError::io(format!(
                            "图片太大（{:.0} MB），内联成 HTML 上限 {} MB（可改用 Markdown 引用）",
                            meta.len() as f64 / (1024.0 * 1024.0),
                            MAX_INLINE_BYTES / (1024 * 1024)
                        )));
                    }
                }
                let bytes = std::fs::read(step.input)
                    .map_err(|e| ConvertError::io(format!("读取图片失败：{e}")))?;
                let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
                html_page(&title, mime_of(step.from), &encoded)
            }
            other => {
                return Err(ConvertError::unsupported(format!(
                    "图片不能直接转成 {}（需要 OCR / 排版引擎）",
                    other.label()
                )))
            }
        };

        std::fs::write(step.output, content)
            .map_err(|e| ConvertError::io(format!("写入文件失败：{e}")))?;

        let warnings = match step.to {
            // 产物只是引用，不是自包含文档——别让用户以为图片被拷进去了。
            ConvertFormat::Md => vec![format!(
                "生成的是引用源图的 Markdown：图片没有被复制，移动或删除 {} 会让它失效",
                step.original
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("源图")
            )],
            ConvertFormat::Html => vec!["图片已内联为 base64，HTML 是自包含的".to_string()],
            _ => Vec::new(),
        };

        Ok(StepOutput { warnings })
    }
}

/// Markdown 里的图片引用：同目录用文件名（随文件夹搬走仍能用），否则绝对路径。
fn markdown_reference(original: &Path, output: &Path) -> String {
    let same_dir = output.parent() == original.parent();
    if same_dir {
        original
            .file_name()
            .and_then(|n| n.to_str())
            .map(str::to_string)
            .unwrap_or_else(|| original.to_string_lossy().to_string())
    } else {
        original.to_string_lossy().to_string()
    }
}

/// 自包含 HTML 页面：深色底、居中、图片按窗口缩放。
fn html_page(title: &str, mime: &str, base64: &str) -> String {
    format!(
        "<!doctype html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n<title>{title}</title>\n\
         <style>\nhtml, body {{ margin: 0; height: 100%; background: #141416; }}\n\
         body {{ display: grid; place-items: center; }}\n\
         img {{ max-width: 100%; max-height: 100vh; display: block; }}\n</style>\n\
         </head>
<body>
<img src=\"data:{mime};base64,{base64}\" alt=\"{title}\">
</body>
</html>
",
        title = escape_html(title),
        mime = mime,
        base64 = base64,
    )
}

/// 图片格式 → MIME。
fn mime_of(format: ConvertFormat) -> &'static str {
    match format {
        ConvertFormat::Png => "image/png",
        ConvertFormat::Jpg => "image/jpeg",
        ConvertFormat::Webp => "image/webp",
        ConvertFormat::Gif => "image/gif",
        ConvertFormat::Bmp => "image/bmp",
        ConvertFormat::Tiff => "image/tiff",
        _ => "application/octet-stream",
    }
}

/// Markdown 链接文字转义：`]` 不转义会让链接提前闭合。
fn escape_md_text(text: &str) -> String {
    text.replace('\\', "\\\\").replace(']', "\\]")
}

/// Markdown 链接地址转义：空格与括号要百分号编码。
fn escape_md_url(url: &str) -> String {
    url.replace(' ', "%20")
        .replace('(', "%28")
        .replace(')', "%29")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::convert::preset::{ConversionPreset, ImageOptions, OutputRule, TargetSpec};

    fn preset(target: ConvertFormat) -> ConversionPreset {
        ConversionPreset {
            id: "t".into(),
            name: "t".into(),
            target: TargetSpec::Explicit(target),
            image: ImageOptions::default(),
            output: OutputRule::default(),
            reencode_same_format: false,
            builtin: false,
        }
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "clipbase-doc-engine-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create scratch");
        dir
    }

    #[test]
    fn markdown_output_is_a_reference_and_says_so() {
        let dir = scratch("md");
        let src = dir.join("pic.png");
        image::RgbImage::new(3, 2).save(&src).expect("save");
        let out = dir.join("pic.md");
        let p = preset(ConvertFormat::Md);
        let result = DocumentEngine
            .convert(&ConversionStep {
                input: &src,
                output: &out,
                from: ConvertFormat::Png,
                to: ConvertFormat::Md,
                final_format: ConvertFormat::Md,
                preset: &p,
                original: &src,
                is_final: true,
            })
            .expect("convert");
        assert_eq!(
            std::fs::read_to_string(&out).unwrap(),
            "![pic](pic.png)\n"
        );
        assert!(
            result.warnings.iter().any(|w| w.contains("引用源图")),
            "{:?}",
            result.warnings
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn html_output_embeds_the_image_inline() {
        let dir = scratch("html");
        let src = dir.join("pic.png");
        image::RgbImage::new(3, 2).save(&src).expect("save");
        let out = dir.join("pic.html");
        let p = preset(ConvertFormat::Html);
        DocumentEngine
            .convert(&ConversionStep {
                input: &src,
                output: &out,
                from: ConvertFormat::Png,
                to: ConvertFormat::Html,
                final_format: ConvertFormat::Html,
                preset: &p,
                original: &src,
                is_final: true,
            })
            .expect("convert");
        let body = std::fs::read_to_string(&out).unwrap();
        assert!(body.contains("data:image/png;base64,"), "{body}");
        assert!(body.contains("<img src=\"data:"), "{body}");
        // 内联后不该再出现外部引用。
        assert!(!body.contains("src=\"pic.png\""), "{body}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tiff_is_not_offered_for_html_embedding() {
        let edges = DocumentEngine.edges();
        assert!(edges
            .iter()
            .any(|e| e.from == ConvertFormat::Tiff && e.to == ConvertFormat::Md));
        assert!(!edges
            .iter()
            .any(|e| e.from == ConvertFormat::Tiff && e.to == ConvertFormat::Html));
    }
}
