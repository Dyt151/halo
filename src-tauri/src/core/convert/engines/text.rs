//! 文本引擎：Markdown / HTML / TXT 三方互转（6 条边，无 self edge）。
//!
//! 不声明 self edge 的原因：同格式文本重编码没有可观察的语义（不是压缩、不是
//! 变换），所以 `SameAsSource` 落在文本上会明确报「已是目标格式」，而不是
//! 假装转换成功（§2.4「源格式等于目标格式时只选择显式 self edge」）。
//!
//! 编码策略：输入不是合法 UTF-8 时按 lossy 解码并记警告——直接失败会让用户
//! 拿到一个干巴巴的 io 错误，而文本转换本身就是尽力而为的。

use std::path::Path;

use super::super::edge::{
    ConversionEdge, ConversionStep, ConvertError, ConverterEngine, EdgeMode, EngineId,
    EngineStatus, StepOutput,
};
use super::super::format::ConvertFormat;
use super::super::text::{
    html_to_markdown, html_to_text, markdown_to_html, markdown_to_text, text_to_html,
    text_to_markdown,
};

/// 文本引擎。
pub struct TextEngine;

/// 单个文本文件的读入上限（32 MB）。
///
/// 文本转换是「整文件读入 + 正则替换」：几百 MB 的日志会让 4 个 worker
/// 同时吃满内存，而用户本来也不需要把这种文件转成 HTML。
const MAX_TEXT_BYTES: u64 = 32 * 1024 * 1024;

impl ConverterEngine for TextEngine {
    fn id(&self) -> EngineId {
        EngineId::Text
    }

    fn display_name(&self) -> &'static str {
        "文本互转"
    }

    fn max_parallelism(&self) -> usize {
        4
    }

    fn status(&self) -> EngineStatus {
        EngineStatus::Available
    }

    fn edges(&self) -> Vec<ConversionEdge> {
        use ConvertFormat::{Html, Md, Txt};
        const PAIRS: [(ConvertFormat, ConvertFormat); 6] =
            [(Md, Html), (Md, Txt), (Html, Md), (Html, Txt), (Txt, Md), (Txt, Html)];
        PAIRS
            .into_iter()
            .map(|(from, to)| ConversionEdge::new(from, to, EngineId::Text, 5, EdgeMode::Normal))
            .collect()
    }

    fn convert(&self, step: &ConversionStep<'_>) -> Result<StepOutput, ConvertError> {
        let mut warnings = Vec::new();
        let raw = read_text(step.input, &mut warnings)?;
        let content = match (step.from, step.to) {
            (ConvertFormat::Md, ConvertFormat::Html) => markdown_to_html(&raw),
            (ConvertFormat::Md, ConvertFormat::Txt) => markdown_to_text(&raw),
            (ConvertFormat::Html, ConvertFormat::Txt) => html_to_text(&raw),
            (ConvertFormat::Html, ConvertFormat::Md) => html_to_markdown(&raw),
            (ConvertFormat::Txt, ConvertFormat::Md) => text_to_markdown(&raw),
            (ConvertFormat::Txt, ConvertFormat::Html) => text_to_html(&raw),
            (from, to) => {
                return Err(ConvertError::unsupported(format!(
                    "文本引擎不支持 {} → {}",
                    from.label(),
                    to.label()
                )))
            }
        };
        std::fs::write(step.output, content)
            .map_err(|e| ConvertError::io(format!("写入文件失败：{e}")))?;
        Ok(StepOutput { warnings })
    }
}

/// 读文本：非法 UTF-8 走 lossy 并留下警告。
fn read_text(path: &Path, warnings: &mut Vec<String>) -> Result<String, ConvertError> {
    // 先看体积再读：大文件的失败应该在「打开」这一步就报清楚，
    // 而不是读到一半内存暴涨、整个应用被系统杀掉。
    if let Ok(meta) = std::fs::metadata(path) {
        if meta.len() > MAX_TEXT_BYTES {
            return Err(ConvertError::io(format!(
                "文件太大（{:.0} MB），文本转换上限 {} MB",
                meta.len() as f64 / (1024.0 * 1024.0),
                MAX_TEXT_BYTES / (1024 * 1024)
            )));
        }
    }
    let bytes =
        std::fs::read(path).map_err(|e| ConvertError::io(format!("读取文件失败：{e}")))?;
    match String::from_utf8(bytes) {
        Ok(text) => Ok(text),
        Err(err) => {
            warnings.push("源文件不是合法 UTF-8，已按替换字符解码".to_string());
            Ok(String::from_utf8_lossy(err.as_bytes()).to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::convert::preset::ConversionPreset;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "clipbase-text-engine-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create scratch");
        dir
    }

    /// 6 条边必须条条能跑通（矩阵闭合）。
    #[test]
    fn all_six_edges_execute() {
        let dir = scratch("edges");
        let md = ConversionPreset {
            id: "t".into(),
            name: "t".into(),
            target: crate::core::convert::preset::TargetSpec::Explicit(ConvertFormat::Html),
            image: Default::default(),
            output: Default::default(),
            reencode_same_format: false,
            builtin: false,
        };

        let pairs = [
            ("a.md", ConvertFormat::Md, ConvertFormat::Html),
            ("b.md", ConvertFormat::Md, ConvertFormat::Txt),
            ("c.html", ConvertFormat::Html, ConvertFormat::Md),
            ("d.html", ConvertFormat::Html, ConvertFormat::Txt),
            ("e.txt", ConvertFormat::Txt, ConvertFormat::Md),
            ("f.txt", ConvertFormat::Txt, ConvertFormat::Html),
        ];
        for (name, from, to) in pairs {
            let src = dir.join(name);
            std::fs::write(&src, "# 标题\n\n正文 **粗**\n").expect("write");
            let out = dir.join(format!("{name}.out"));
            let step = ConversionStep {
                input: &src,
                output: &out,
                from,
                to,
                final_format: to,
                preset: &md,
                original: &src,
                is_final: true,
            };
            TextEngine
                .convert(&step)
                .unwrap_or_else(|e| panic!("{from:?} → {to:?} 失败：{e}"));
            let body = std::fs::read_to_string(&out).expect("read out");
            assert!(body.contains("标题"), "{from:?}→{to:?} 丢了正文：{body}");
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn invalid_utf8_is_decoded_lossily_with_warning() {
        let dir = scratch("utf8");
        let src = dir.join("bad.txt");
        std::fs::write(&src, [0x61, 0xFF, 0x62]).expect("write");
        let out = dir.join("bad.md");
        let preset = ConversionPreset {
            id: "t".into(),
            name: "t".into(),
            target: crate::core::convert::preset::TargetSpec::Explicit(ConvertFormat::Md),
            image: Default::default(),
            output: Default::default(),
            reencode_same_format: false,
            builtin: false,
        };
        let result = TextEngine
            .convert(&ConversionStep {
                input: &src,
                output: &out,
                from: ConvertFormat::Txt,
                to: ConvertFormat::Md,
                final_format: ConvertFormat::Md,
                preset: &preset,
                original: &src,
                is_final: true,
            })
            .expect("convert");
        assert_eq!(result.warnings.len(), 1, "{:?}", result.warnings);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
