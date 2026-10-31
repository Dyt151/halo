//! 引擎注册表：把「有哪些引擎、它们能做什么」集中成一份可缓存的目录。
//!
//! 对照 docs/REA-Filee-Conversion.md §2.4 的 `ConverterCatalog`：
//! - 某个引擎不可用只让它自己的边消失（`EngineStatus::Missing`），不影响整张图；
//! - `refresh()` 让边走一次重建（将来接外部引擎时用得上）。

pub mod document;
pub mod image;
pub mod jxa;
pub mod pdfkit;
pub mod sips;
pub mod text;
pub mod textutil;
pub mod webprint;

use std::sync::{Mutex, OnceLock};

use super::edge::{ConversionEdge, ConverterEngine, EngineId, EngineStatus};
use crate::lock_ext::LockExt;

static ENGINES: OnceLock<Vec<Box<dyn ConverterEngine>>> = OnceLock::new();

/// 边缓存：首次访问时按「当前可用引擎」构建，`refresh()` 之后重建。
static EDGES: Mutex<Option<Vec<ConversionEdge>>> = Mutex::new(None);

/// 全部已注册引擎（进程内单例）。
pub fn engines() -> &'static [Box<dyn ConverterEngine>] {
    ENGINES.get_or_init(|| {
        vec![
            Box::new(image::ImageEngine),
            Box::new(text::TextEngine),
            Box::new(document::DocumentEngine),
            Box::new(sips::SipsEngine),
            Box::new(textutil::TextutilEngine),
            Box::new(pdfkit::PdfkitEngine),
            Box::new(webprint::WebprintEngine),
        ]
    })
}

/// 按 ID 取引擎。
pub fn engine(id: EngineId) -> Option<&'static dyn ConverterEngine> {
    engines()
        .iter()
        .find(|e| e.id() == id)
        .map(|e| e.as_ref())
}

/// 当前所有**可用**引擎声明的边。
pub fn edges() -> Vec<ConversionEdge> {
    let mut guard = EDGES.lock_ok();
    if let Some(cached) = guard.as_ref() {
        return cached.clone();
    }
    let mut built: Vec<ConversionEdge> = Vec::new();
    for engine in engines() {
        if !engine.status().is_available() {
            continue;
        }
        built.extend(engine.edges());
    }
    *guard = Some(built.clone());
    built
}

/// 使边缓存失效（引擎状态变化后调用）。
pub fn refresh() {
    *EDGES.lock_ok() = None;
}

/// 引擎状态快照（诊断 / 设置页展示用）。
pub fn statuses() -> Vec<(EngineId, &'static str, EngineStatus)> {
    engines()
        .iter()
        .map(|e| (e.id(), e.display_name(), e.status()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_registered_engine_declares_edges_and_is_available() {
        for engine in engines() {
            assert!(
                !engine.edges().is_empty(),
                "{} 没有声明任何边",
                engine.display_name()
            );
            assert!(
                engine.max_parallelism() >= 1,
                "{} 并发上限必须 >= 1",
                engine.display_name()
            );
            assert!(engine.status().is_available());
        }
    }

    #[test]
    fn edges_only_connect_known_formats_and_are_weighted() {
        for edge in edges() {
            assert!(edge.cost > 0, "边成本必须为正：{edge:?}");
            assert!(
                matches!(
                    edge.mode,
                    super::super::edge::EdgeMode::Normal | super::super::edge::EdgeMode::Terminal
                ),
                "v1 不应声明特殊模式的边：{edge:?}"
            );
            use super::super::format::ConvertFormat;
            let image_or_pdf = |f: ConvertFormat| f.is_image() || f == ConvertFormat::Pdf;
            let text_or_doc = |f: ConvertFormat| f.is_text() || f.is_document();
            match edge.engine {
                // 图片引擎的边两端都是图片；sips 的边两端是图片或 PDF。
                EngineId::Image => {
                    assert!(edge.from.is_image() && edge.to.is_image())
                }
                EngineId::Sips => {
                    assert!(image_or_pdf(edge.from) && image_or_pdf(edge.to))
                }
                EngineId::Text => assert!(edge.from.is_text() && edge.to.is_text()),
                EngineId::Document => {
                    assert!(edge.from.is_image());
                    assert!(edge.to.is_text());
                }
                EngineId::Textutil => {
                    assert!(text_or_doc(edge.from) && text_or_doc(edge.to))
                }
                EngineId::Pdfkit => {
                    assert_eq!(edge.from, ConvertFormat::Pdf);
                    assert_eq!(edge.to, ConvertFormat::Txt);
                }
                EngineId::Webprint => {
                    assert_eq!(edge.from, ConvertFormat::Html);
                    assert_eq!(edge.to, ConvertFormat::Pdf);
                }
            }
        }
    }
}
