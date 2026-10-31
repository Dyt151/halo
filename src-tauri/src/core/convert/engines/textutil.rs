//! macOS 系统 `textutil` 引擎：TXT / HTML / DOCX 文档家族互转。
//!
//! textutil 是系统自带（/usr/bin/textutil），不引入任何第三方解析库——
//! OOXML 的解析交给系统做，我们只声明边和跑命令。
//! 三种格式两两互转共 6 条边（不声明 self edge：同格式重编码没有可观察语义，
//! 与 TextEngine 的约定一致）。
//!
//! 排版保真说明：textutil 只保证「内容」与基本结构，字体、分页、样式细节
//! 按 macOS 文本引擎的规则解释，转换结果会记一条警告。

use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;

use super::super::edge::{
    ConversionEdge, ConversionStep, ConvertError, ConverterEngine, EdgeMode, EngineId,
    EngineStatus, StepOutput,
};
use super::super::format::ConvertFormat;

/// macOS 系统 textutil 引擎。
pub struct TextutilEngine;

/// textutil 支持的三种格式（`-convert` 的取值同名）。
const TEXTUTIL_FORMATS: [ConvertFormat; 3] = [
    ConvertFormat::Txt,
    ConvertFormat::Html,
    ConvertFormat::Docx,
];

/// textutil 命令是否存在（进程内只探测一次）。
fn textutil_present() -> bool {
    static PRESENT: OnceLock<bool> = OnceLock::new();
    *PRESENT.get_or_init(|| {
        Command::new("textutil")
            .arg("-help")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    })
}

fn textutil_format_name(format: ConvertFormat) -> &'static str {
    match format {
        ConvertFormat::Txt => "txt",
        ConvertFormat::Html => "html",
        ConvertFormat::Docx => "docx",
        _ => unreachable!("textutil 边只会落在 TEXTUTIL_FORMATS 内"),
    }
}

impl ConverterEngine for TextutilEngine {
    fn id(&self) -> EngineId {
        EngineId::Textutil
    }

    fn display_name(&self) -> &'static str {
        "系统 textutil（DOCX）"
    }

    fn max_parallelism(&self) -> usize {
        // 每次一个独立进程，docx 解析吃 CPU；2 个并发够用。
        2
    }

    fn status(&self) -> EngineStatus {
        if textutil_present() {
            EngineStatus::Available
        } else {
            EngineStatus::Missing("需要 macOS 自带的 textutil 命令".to_string())
        }
    }

    fn edges(&self) -> Vec<ConversionEdge> {
        let mut out = Vec::with_capacity(6);
        for from in TEXTUTIL_FORMATS {
            for to in TEXTUTIL_FORMATS {
                if from != to {
                    out.push(ConversionEdge::new(from, to, EngineId::Textutil, 8, EdgeMode::Normal));
                }
            }
        }
        out
    }

    fn convert(&self, step: &ConversionStep<'_>) -> Result<StepOutput, ConvertError> {
        let name = textutil_format_name(step.to);
        let output = Command::new("textutil")
            .arg("-convert")
            .arg(name)
            .arg("-output")
            .arg(step.output)
            .arg(step.input)
            .output()
            .map_err(|e| ConvertError::io(format!("无法启动 textutil：{e}")))?;

        if !output.status.success() {
            let detail = String::from_utf8_lossy(&output.stderr);
            let detail = detail.trim();
            return Err(ConvertError::encode(format!(
                "textutil 转换 {} → {} 失败{}",
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
                "textutil 没有写出目标文件（{}）",
                step.to.label()
            )));
        }

        Ok(StepOutput {
            warnings: vec![format!(
                "经系统 textutil 转换（{} → {}）：字体与排版细节按 macOS 文本引擎解释，可能与原文档不完全一致",
                step.from.label(),
                step.to.label()
            )],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn textutil_edges_stay_inside_document_quadrant() {
        for edge in TextutilEngine.edges() {
            let in_quadrant = |f: ConvertFormat| f.is_text() || f.is_document();
            assert!(
                in_quadrant(edge.from) && in_quadrant(edge.to),
                "{edge:?}"
            );
            assert!(!edge.is_self(), "不声明 self edge：{edge:?}");
        }
    }

    #[test]
    fn textutil_name_map_covers_all_formats() {
        let cases = [
            (ConvertFormat::Txt, "txt"),
            (ConvertFormat::Html, "html"),
            (ConvertFormat::Docx, "docx"),
        ];
        for (format, name) in cases {
            assert_eq!(textutil_format_name(format), name, "{format:?}");
        }
    }

    /// textutil 三方互转应让所有文本/文档格式互相可达（一步或经 HTML 中转）。
    #[test]
    fn text_and_doc_formats_reach_each_other() {
        use crate::core::convert::router;
        let all = TEXTUTIL_FORMATS;
        for from in all {
            for to in all {
                if from == to {
                    continue;
                }
                let route = router::plan(from, to, false)
                    .unwrap_or_else(|e| panic!("{from:?} → {to:?} 应有路由：{e:?}"));
                assert!(route.steps.len() <= 2, "{from:?} → {to:?}");
            }
        }
    }
}
