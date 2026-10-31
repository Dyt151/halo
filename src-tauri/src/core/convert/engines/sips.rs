//! macOS 系统 `sips` 引擎：HEIC 的读写 + 图片 ↔ PDF 桥。
//!
//! HEIC `image` crate 没有编解码器（要 x265），但 macOS 自带的 `sips` 原生
//! 支持。sips 的定位是**桥**而不是通用引擎：
//!
//! - 它只负责「HEIC ↔ 常规格式」这一段，常规格式之间的互转仍走 `ImageEngine`
//!   （透明图转 JPG 按预设背景色合成、EXIF 摆正这些语义只在 ImageEngine 里有）；
//! - 所以 sips 不声明 `heic → jpg` 直连边——JPG 没有 alpha，直连会把透明图
//!   压成随机黑底。路由器会选 `heic → png → jpg` 两步，背景合成交给
//!   ImageEngine（§5.1「绝不静默保真」）；PDF → JPG 同理，经 PNG 中转；
//! - 图片 → PDF：sips 把图嵌入单页 PDF（「图片转 PDF」预设就是它）；
//! - PDF → 图片：sips 只读**第一页**，这条限制在警告里如实说，每页出图走
//!   PDFKit 动作而不是边；
//! - sips 读不了 WebP，`heic → webp` 同样经 PNG 中转（两步，在
//!   `MAX_STEPS = 3` 预算内）。

use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;

use super::super::edge::{
    ConversionEdge, ConversionStep, ConvertError, ConverterEngine, EdgeMode, EngineId,
    EngineStatus, StepOutput,
};
use super::super::format::{ConvertFormat, SIPS_FORMATS};

/// macOS 系统 sips 引擎。
pub struct SipsEngine;

/// sips 直写的常规目标（都能带 alpha，避免透明图被压黑）。
const SIPS_BRIDGE_TARGETS: [ConvertFormat; 4] = [
    ConvertFormat::Png,
    ConvertFormat::Gif,
    ConvertFormat::Bmp,
    ConvertFormat::Tiff,
];

/// sips 能可靠读取、可送进异类编码器 / PDF 的常规源。
const SIPS_BRIDGE_SOURCES: [ConvertFormat; 5] = [
    ConvertFormat::Png,
    ConvertFormat::Jpg,
    ConvertFormat::Gif,
    ConvertFormat::Bmp,
    ConvertFormat::Tiff,
];

/// 图片 → PDF 的源集合：sips 读得动的图片格式（读不了 webp，
/// 那种经 PNG 桥两步到 PDF）。
const SIPS_PDF_SOURCES: [ConvertFormat; 6] = [
    ConvertFormat::Png,
    ConvertFormat::Jpg,
    ConvertFormat::Gif,
    ConvertFormat::Bmp,
    ConvertFormat::Tiff,
    ConvertFormat::Heic,
];

/// sips 命令是否存在（进程内只探测一次）。
fn sips_present() -> bool {
    static PRESENT: OnceLock<bool> = OnceLock::new();
    *PRESENT.get_or_init(|| {
        Command::new("sips")
            .arg("--formats")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    })
}

/// ConvertFormat → sips 的格式名（`sips --formats` 里的取值）。
fn sips_format_name(format: ConvertFormat) -> &'static str {
    match format {
        ConvertFormat::Jpg => "jpeg",
        ConvertFormat::Png => "png",
        ConvertFormat::Gif => "gif",
        ConvertFormat::Bmp => "bmp",
        ConvertFormat::Tiff => "tiff",
        ConvertFormat::Heic => "heic",
        ConvertFormat::Pdf => "pdf",
        // sips 写不了 webp；不能当彩色输出用的格式一律退回 png。
        _ => return "png",
    }
}

impl ConverterEngine for SipsEngine {
    fn id(&self) -> EngineId {
        EngineId::Sips
    }

    fn display_name(&self) -> &'static str {
        "系统 sips（HEIC）"
    }

    fn max_parallelism(&self) -> usize {
        // sips 每次开一个独立进程，AVIF/JP2 编码也吃 CPU，串行最稳。
        1
    }

    fn status(&self) -> EngineStatus {
        if sips_present() {
            EngineStatus::Available
        } else {
            EngineStatus::Missing("需要 macOS 自带的 sips 命令".to_string())
        }
    }

    fn edges(&self) -> Vec<ConversionEdge> {
        let mut out = Vec::with_capacity(
            SIPS_FORMATS.len() * (SIPS_BRIDGE_TARGETS.len() + 1)
                + SIPS_BRIDGE_SOURCES.len() * SIPS_FORMATS.len()
                + SIPS_PDF_SOURCES.len()
                + SIPS_BRIDGE_TARGETS.len()
                + SIPS_FORMATS.len(),
        );
        // 异类 → 常规桥目标（+ 同格式重编码）。
        for from in SIPS_FORMATS {
            for to in SIPS_BRIDGE_TARGETS {
                out.push(ConversionEdge::new(from, to, EngineId::Sips, 14, EdgeMode::Normal));
            }
            out.push(ConversionEdge::new(from, from, EngineId::Sips, 14, EdgeMode::Normal));
        }
        // 常规源 → 异类目标。
        for from in SIPS_BRIDGE_SOURCES {
            for to in SIPS_FORMATS {
                out.push(ConversionEdge::new(from, to, EngineId::Sips, 14, EdgeMode::Normal));
            }
        }
        // 图片 → PDF（单页嵌入）。Terminal：PDF 是最终产物，不能再被 pdf→txt
        // 之类的边接走——位图 PDF 没有文字层，提取出来只会是空文件。
        for from in SIPS_PDF_SOURCES {
            out.push(ConversionEdge::new(
                from,
                ConvertFormat::Pdf,
                EngineId::Sips,
                14,
                EdgeMode::Terminal,
            ));
        }
        // PDF → 图片（只第一页；不含 JPG，透明/无 alpha 语义交给 PNG 中转）。
        for to in SIPS_BRIDGE_TARGETS {
            out.push(ConversionEdge::new(ConvertFormat::Pdf, to, EngineId::Sips, 14, EdgeMode::Normal));
        }
        for to in SIPS_FORMATS {
            out.push(ConversionEdge::new(ConvertFormat::Pdf, to, EngineId::Sips, 14, EdgeMode::Normal));
        }
        out
    }

    fn convert(&self, step: &ConversionStep<'_>) -> Result<StepOutput, ConvertError> {
        let name = sips_format_name(step.to);
        let output = Command::new("sips")
            .arg("-s")
            .arg("format")
            .arg(name)
            .arg(step.input)
            .arg("--out")
            .arg(step.output)
            .output()
            .map_err(|e| ConvertError::io(format!("无法启动 sips：{e}")))?;

        if !output.status.success() {
            let detail = String::from_utf8_lossy(&output.stderr);
            let detail = detail.trim();
            return Err(ConvertError::encode(format!(
                "sips 转换 {} → {} 失败{}",
                step.from.label(),
                step.to.label(),
                if detail.is_empty() {
                    String::new()
                } else {
                    format!("：{detail}")
                }
            )));
        }

        if !Path::new(step.output).exists() {
            return Err(ConvertError::encode(format!(
                "sips 没有写出目标文件（{}）",
                step.to.label()
            )));
        }

        let mut warnings = vec![format!(
            "经系统 sips 转换（{}）：EXIF / ICC 等元数据不保留",
            step.from.label()
        )];
        if step.from == ConvertFormat::Pdf {
            warnings.push("PDF 只转换第一页，其余页面不会出现在结果里".to_string());
        }
        if step.to == ConvertFormat::Pdf {
            warnings.push("图片被嵌入为单页 PDF".to_string());
        }
        Ok(StepOutput { warnings })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_edges_stay_inside_image_or_pdf_quadrant() {
        for edge in SipsEngine.edges() {
            let in_quadrant =
                |f: ConvertFormat| f.is_image() || f == ConvertFormat::Pdf;
            assert!(in_quadrant(edge.from) && in_quadrant(edge.to), "{edge:?}");
        }
    }

    /// JPG 是唯一「不能带 alpha 的常规目标」，sips 不声明直连，逼路由器走
    /// PNG 中转，让 ImageEngine 按预设背景色合成。
    #[test]
    fn no_direct_edge_to_jpeg() {
        let edges = SipsEngine.edges();
        for from in SIPS_FORMATS.iter().copied().chain([ConvertFormat::Pdf]) {
            assert!(
                !edges
                    .iter()
                    .any(|e| e.from == from && e.to == ConvertFormat::Jpg),
                "{from:?} 不该直连 JPG"
            );
        }
    }

    /// 图片 ↔ PDF 的边按声明覆盖：sips 读得动的图片都能进 PDF，
    /// PDF 能到全部桥目标 + 异类目标（不含 JPG）。
    #[test]
    fn pdf_edges_cover_declared_sources_and_targets() {
        let edges = SipsEngine.edges();
        for from in SIPS_PDF_SOURCES {
            assert!(
                edges.iter().any(|e| e.from == from && e.to == ConvertFormat::Pdf),
                "{from:?} 应有 → PDF 边"
            );
        }
        for to in SIPS_BRIDGE_TARGETS.iter().copied().chain(SIPS_FORMATS) {
            assert!(
                edges
                    .iter()
                    .any(|e| e.from == ConvertFormat::Pdf && e.to == to),
                "PDF 应有 → {to:?} 边"
            );
        }
        // webp 不进 PDF 直连（sips 读不了），但有经 PNG 的两步路由。
        assert!(!edges
            .iter()
            .any(|e| e.from == ConvertFormat::Webp && e.to == ConvertFormat::Pdf));
        let route = crate::core::convert::router::plan(ConvertFormat::Webp, ConvertFormat::Pdf, false)
            .expect("webp → pdf 应有路由");
        assert!(route.steps.len() <= 2);
    }

    /// HEIC 能借 PNG 桥到任何常规目标（两步，预算内）。
    #[test]
    fn exotic_formats_reach_common_targets_via_png() {
        use crate::core::convert::router;
        for from in SIPS_FORMATS {
            for to in [ConvertFormat::Png, ConvertFormat::Jpg, ConvertFormat::Webp] {
                let route = router::plan(from, to, false)
                    .unwrap_or_else(|e| panic!("{from:?} → {to:?} 应有路由：{e:?}"));
                assert!(route.steps.len() <= 2, "{from:?} → {to:?}");
            }
        }
        // 文本方向仍不可达（没有 OCR）。
        assert!(!router::has_route(ConvertFormat::Heic, ConvertFormat::Txt));
    }

    #[test]
    fn sips_name_map_covers_all_bridge_formats() {
        let cases = [
            (ConvertFormat::Png, "png"),
            (ConvertFormat::Jpg, "jpeg"),
            (ConvertFormat::Gif, "gif"),
            (ConvertFormat::Bmp, "bmp"),
            (ConvertFormat::Tiff, "tiff"),
            (ConvertFormat::Heic, "heic"),
            (ConvertFormat::Pdf, "pdf"),
        ];
        for (format, name) in cases {
            assert_eq!(sips_format_name(format), name, "{format:?}");
        }
    }
}
