//! WebKit 排版桥（JXA）：HTML → PDF。
//!
//! 用 WKWebView 加载 HTML 再经 NSPrintOperation 免面板输出 PDF——系统自带
//! 排版引擎，不引入任何第三方 HTML 渲染器。限制如实告知：
//! - 输出是**排版快照**，没有可提取/复制的文本层（要文本请用 HTML 本身
//!   或先转 TXT/MD）；
//! - 本地相对路径（图片、CSS）按源文件所在目录解析；
//! - 每次调用拉起独立 WKWebView 进程，串行执行。

use super::super::edge::{
    ConversionEdge, ConversionStep, ConvertError, ConverterEngine, EdgeMode, EngineId,
    EngineStatus, StepOutput,
};
use super::super::format::ConvertFormat;
use super::jxa;

/// WebKit 排版引擎。
pub struct WebprintEngine;

/// HTML → PDF 脚本（WKWebView + NSPrintOperation，免打印面板）。
const WEBPRINT_SCRIPT: &str = r#"
ObjC.import("AppKit");
ObjC.import("WebKit");
function run(argv) {
  const input = argv[0], output = argv[1];
  const html = $.NSString.stringWithContentsOfFileEncodingError(
    $(input), $.NSUTF8StringEncoding, null);
  if (html.isNil()) return "ERR:读取 HTML 失败";
  const base = $.NSURL.fileURLWithPath($(input).stringByDeletingLastPathComponent);
  const web = $.WKWebView.alloc.initWithFrameConfiguration(
    $.NSMakeRect(0, 0, 612, 792), $.WKWebViewConfiguration.alloc.init);
  web.loadHTMLStringBaseURL(html, base);
  let guard = 0;
  while (web.isLoading && guard < 2000) {
    $.NSRunLoop.currentRunLoop.runUntilDate($.NSDate.dateWithTimeIntervalSinceNow(0.02));
    guard++;
  }
  if (web.isLoading) return "ERR:加载超时";
  $.NSRunLoop.currentRunLoop.runUntilDate($.NSDate.dateWithTimeIntervalSinceNow(0.15));
  $.NSRunLoop.currentRunLoop.runUntilDate($.NSDate.dateWithTimeIntervalSinceNow(0.15));
  const data = $.NSMutableData.data;
  const op = $.NSPrintOperation.PDFOperationWithViewInsideRectToData(web, web.bounds, data);
  op.setShowsPrintPanel(false);
  op.setShowsProgressPanel(false);
  if (!op.runOperation) return "ERR:PDF 输出失败";
  if (data.length < 1) return "ERR:PDF 输出为空";
  return data.writeToFileAtomically($(output), true) ? "ok" : "ERR:写入失败";
}
"#;

impl ConverterEngine for WebprintEngine {
    fn id(&self) -> EngineId {
        EngineId::Webprint
    }

    fn display_name(&self) -> &'static str {
        "系统 WebKit 排版（HTML → PDF）"
    }

    fn max_parallelism(&self) -> usize {
        // 每次一个 WKWebView 进程，串行最稳。
        1
    }

    fn status(&self) -> EngineStatus {
        if jxa::run("webprint-probe.js", "function run() { return 'ok'; }", &[]).is_ok() {
            EngineStatus::Available
        } else {
            EngineStatus::Missing("需要 macOS 的 osascript".to_string())
        }
    }

    fn edges(&self) -> Vec<ConversionEdge> {
        // Terminal：PDF 是排版快照（无文字层），不能再被 pdf→txt / pdf→图片
        // 接走——那会把「文本 → 图片」变成打印截图这种意外路径。
        vec![ConversionEdge::new(
            ConvertFormat::Html,
            ConvertFormat::Pdf,
            EngineId::Webprint,
            12,
            EdgeMode::Terminal,
        )]
    }

    fn convert(&self, step: &ConversionStep<'_>) -> Result<StepOutput, ConvertError> {
        jxa::run(
            "webprint.js",
            WEBPRINT_SCRIPT,
            &[
                &step.input.to_string_lossy(),
                &step.output.to_string_lossy(),
            ],
        )?;
        Ok(StepOutput {
            warnings: vec![
                "PDF 是排版快照：不含可复制文本层，样式按 WebKit 默认纸张（Letter）分页".to_string(),
            ],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webprint_declares_only_html_to_pdf() {
        let edges = WebprintEngine.edges();
        assert_eq!(edges.len(), 1);
        let edge = edges[0];
        assert_eq!(edge.from, ConvertFormat::Html);
        assert_eq!(edge.to, ConvertFormat::Pdf);
    }
}
