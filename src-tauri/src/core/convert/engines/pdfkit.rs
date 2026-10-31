//! PDFKit 桥（JXA）：PDF → TXT 文本提取。
//!
//! - 引擎边只有 `Pdf → Txt` 一条：PDF 文本层提取，不涉及 OCR；
//! - 每页 → PNG/JPG（PDF 转图动作用）与 CBZ 打包走同一脚本的 `pages` 模式，
//!   由 `render_pages` 暴露给动作流水线；
//! - 扫描件没有文本层时提取结果为空文件——不假装失败也不假装成功，
//!   给一条明确警告。

use std::path::Path;

use super::super::edge::{
    ConversionEdge, ConversionStep, ConvertError, ConverterEngine, EdgeMode, EngineId,
    EngineStatus, StepOutput,
};
use super::super::format::ConvertFormat;
use super::jxa;

/// macOS 系统 PDFKit 桥。
pub struct PdfkitEngine;

/// PDFKit 脚本：`txt` 提取文本、`pages` 逐页出图（144 dpi）。
const PDFKIT_SCRIPT: &str = r#"
ObjC.import("PDFKit");
ObjC.import("AppKit");
function run(argv) {
  const mode = argv[0], input = argv[1];
  const doc = $.PDFDocument.alloc.initWithURL($.NSURL.fileURLWithPath($(input)));
  if (doc.isNil()) return "ERR:打开 PDF 失败（文件损坏或已加密）";
  const count = doc.pageCount;
  if (mode == "txt") {
    const text = doc.string;
    if (text.isNil()) return "ERR:读取文本失败";
    return text.writeToFileAtomicallyEncodingError($(argv[2]), true, $.NSUTF8StringEncoding, null)
      ? "ok" : "ERR:写入失败";
  }
  if (mode == "pages") {
    const outdir = argv[2], fmt = argv[3], quality = Number(argv[4] || 85) / 100.0;
    const fm = $.NSFileManager.defaultManager;
    fm.createDirectoryAtPathWithIntermediateDirectoriesAttributesError($(outdir), true, $(), null);
    for (let i = 0; i < count; i++) {
      const page = doc.pageAtIndex(i);
      const box = page.boundsForBox($.kPDFDisplayBoxMediaBox);
      const scale = 144.0 / 72.0;
      const size = $.NSMakeSize(box.size.width * scale, box.size.height * scale);
      const img = page.thumbnailOfSizeForBox(size, $.kPDFDisplayBoxMediaBox);
      if (img.isNil()) return "ERR:渲染第 " + (i + 1) + " 页失败";
      const rep = $.NSBitmapImageRep.imageRepWithData(img.TIFFRepresentation);
      const type = fmt == "jpg" ? $.NSBitmapImageFileTypeJPEG : $.NSBitmapImageFileTypePNG;
      const props = fmt == "jpg" ? $({ NSImageCompressionFactor: quality }) : $({});
      const data = rep.representationUsingTypeProperties(type, props);
      if (data.isNil()) return "ERR:编码第 " + (i + 1) + " 页失败";
      const name = String(i + 1).padStart(4, "0") + "." + fmt;
      data.writeToFileAtomically($(outdir + "/" + name), true);
    }
    return "ok:" + count;
  }
  return "ERR:未知模式";
}
"#;

impl ConverterEngine for PdfkitEngine {
    fn id(&self) -> EngineId {
        EngineId::Pdfkit
    }

    fn display_name(&self) -> &'static str {
        "系统 PDFKit（PDF → TXT）"
    }

    fn max_parallelism(&self) -> usize {
        2
    }

    fn status(&self) -> EngineStatus {
        // osascript / PDFKit 是 macOS 固定组件，只看解释器在不在。
        if jxa::run("pdfkit-probe.js", "function run() { return 'ok'; }", &[]).is_ok() {
            EngineStatus::Available
        } else {
            EngineStatus::Missing("需要 macOS 的 osascript".to_string())
        }
    }

    fn edges(&self) -> Vec<ConversionEdge> {
        vec![ConversionEdge::new(
            ConvertFormat::Pdf,
            ConvertFormat::Txt,
            EngineId::Pdfkit,
            10,
            EdgeMode::Normal,
        )]
    }

    fn convert(&self, step: &ConversionStep<'_>) -> Result<StepOutput, ConvertError> {
        jxa::run(
            "pdfkit.js",
            PDFKIT_SCRIPT,
            &[
                "txt",
                &step.input.to_string_lossy(),
                &step.output.to_string_lossy(),
            ],
        )?;
        Ok(StepOutput {
            warnings: vec![
                "PDF 文本提取只取文本层：扫描件（图片型 PDF）没有文本层时结果为空白，需要 OCR 才能识别".to_string(),
            ],
        })
    }
}

/// 把 PDF 每页渲染成 PNG / JPG，写入 `out_dir`（`0001.png`、`0002.png`…）。
///
/// 返回页数。动作流水线（PDF → 每页图片 / CBZ）专用。
pub fn render_pages(
    pdf: &Path,
    out_dir: &Path,
    format: ConvertFormat,
    jpeg_quality: u8,
) -> Result<usize, ConvertError> {
    let fmt = match format {
        ConvertFormat::Png => "png",
        ConvertFormat::Jpg => "jpg",
        other => {
            return Err(ConvertError::unsupported(format!(
                "PDF 每页出图只支持 PNG / JPG，不支持 {}",
                other.label()
            )))
        }
    };
    let stdout = jxa::run(
        "pdfkit.js",
        PDFKIT_SCRIPT,
        &[
            "pages",
            &pdf.to_string_lossy(),
            &out_dir.to_string_lossy(),
            fmt,
            &jpeg_quality.to_string(),
        ],
    )?;
    stdout
        .strip_prefix("ok:")
        .and_then(|n| n.trim().parse::<usize>().ok())
        .ok_or_else(|| ConvertError::encode(format!("PDFKit 返回了无法解析的结果：{stdout}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pdfkit_declares_only_pdf_to_txt() {
        let edges = PdfkitEngine.edges();
        assert_eq!(edges.len(), 1);
        let edge = edges[0];
        assert_eq!(edge.from, ConvertFormat::Pdf);
        assert_eq!(edge.to, ConvertFormat::Txt);
    }
}
