//! 文件格式转换：**预设（Preset）→ 任务（Job）→ 逐文件路由（Route）与执行**。
//!
//! 这一层的建模对齐 docs/REA-Filee-Conversion.md，和早期「源扩展名 + 目标格式
//! 字符串 + 全局质量 → 直接写最终文件」的做法有四点不同：
//!
//! 1. **动作身份是预设，不是格式**（`preset.rs`）：两个 JPG 动作可以有不同质量、
//!    不同命名规则，互不影响；质量和输出目录不再挂在 `RingConfig` 全局。
//! 2. **能力来自引擎声明的边，不是写死的矩阵**（`edge.rs` / `engines/`）：
//!    「能不能转」由路由图回答，「怎么转」由 Dijkstra 选最便宜路径（`router.rs`），
//!    图片 → HTML 这类还可能是两步（TIFF → PNG → HTML）。
//! 3. **每个输入都有结果**（`job.rs`）：converted / skipped / failed / cancelled
//!    四种状态都带结构化原因，不再有 `_ => {}` 静默丢文件。
//! 4. **先预留、写临时文件、再原子提交**（`output.rs`）：源文件永不因冲突策略被
//!    覆盖，失败或取消不会留下半成品。
//!
//! 当前引擎：图片编解码（`image`）、文本互转（md/html/txt）、图片转文档。
//! 故意缺的两条边——文本 → 图片（要排版渲染）、图片 → TXT（要 OCR）——在路由
//! 图上就是不可达，会给出明确原因而不是假装成功。

pub mod edge;
pub mod engines;
pub mod format;
pub mod job;
pub mod output;
pub mod preset;
pub mod router;
pub mod text;

pub use edge::{ConvertError, ConversionEdge, EngineId, EngineStatus};
pub use format::{
    detect_format, ConvertFormat, ALL_FORMATS, DEFAULT_JPEG_QUALITY, IMAGE_FORMATS, TEXT_FORMATS,
};
pub use job::{
    CancelToken, ConversionFileResult, ConversionJob, FileState, JobState, SkipCode, SkipReason,
};
pub use output::{AllocError, OutputAllocator, OutputRequest, ReservedOutput};
pub use preset::{
    builtin_presets, ConflictPolicy, ConversionPreset, ImageOptions, OutputRule, TargetSpec,
};
pub use text::{
    html_to_markdown, html_to_text, markdown_to_html, markdown_to_text, text_to_html,
    text_to_markdown,
};

/// 纯函数：返回该源格式能转换到的目标格式列表（**不含自身**）。
///
/// 从路由图上筛：一张图能去 5 种图片 + md/html；文本三方互转；图片 → TXT 因为
/// 需要 OCR，图上不可达，所以不会出现在这里。
pub fn available_targets(source: ConvertFormat) -> Vec<ConvertFormat> {
    ALL_FORMATS
        .into_iter()
        .filter(|target| *target != source && router::plan(source, *target, false).is_ok())
        .collect()
}



/// 现造一个临时预设：只给「按目标格式一把梭」的调用方用（旧命令、单测）。
///
/// 常规入口应当用配置里的预设（`preset.rs`），它带着质量、背景色和输出规则。
pub fn adhoc_preset(
    target: ConvertFormat,
    jpeg_quality: u8,
    dest_dir: Option<&str>,
) -> ConversionPreset {
    ConversionPreset {
        id: format!("adhoc.{}", target.as_str()),
        name: preset::default_preset_name(target, jpeg_quality),
        target: TargetSpec::Explicit(target),
        image: ImageOptions {
            jpeg_quality: jpeg_quality.clamp(1, 100),
            ..ImageOptions::default()
        },
        output: OutputRule {
            dir: dest_dir
                .map(str::trim)
                .filter(|d| !d.is_empty())
                .unwrap_or_default()
                .to_string(),
            ..OutputRule::default()
        },
        reencode_same_format: false,
        builtin: false,
    }
}

/// 单文件转换（兼容入口）：返回输出路径，失败时返回可读原因。
///
/// 内部走完整流水线（探测 → 路由 → 预留 → 临时文件 → 原子提交），
/// 只是把结果压成一个 `Result`。
pub fn convert_file(
    path: &str,
    target: ConvertFormat,
    jpeg_quality: u8,
    dest_dir: Option<&str>,
) -> Result<String, String> {
    let preset = adhoc_preset(target, jpeg_quality, dest_dir);
    let sources = vec![path.to_string()];
    let job = job::run(&sources, &preset, &mut |_, _, _| {});
    let Some(file) = job.files.into_iter().next() else {
        return Err("转换没有产生任何结果".to_string());
    };
    match file.state {
        FileState::Done => file
            .outputs
            .into_iter()
            .next()
            .ok_or_else(|| "转换成功但没有产出文件".to_string()),
        FileState::Failed => Err(file
            .error
            .map(|e| e.message)
            .unwrap_or_else(|| "转换失败".to_string())),
        _ => Err(file
            .skip
            .map(|s| s.detail)
            .or_else(|| file.error.map(|e| e.message))
            .unwrap_or_else(|| "转换未执行".to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn scratch_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "clipbase-convert-{}-{name}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create scratch dir");
        dir
    }

    #[test]
    fn available_targets_matrix() {
        // 图片：其余 5 种图片 + md/html（包成文档），唯独不能直接转 TXT（要 OCR）。
        assert_eq!(
            available_targets(ConvertFormat::Png),
            vec![
                ConvertFormat::Jpg,
                ConvertFormat::Webp,
                ConvertFormat::Gif,
                ConvertFormat::Bmp,
                ConvertFormat::Tiff,
                ConvertFormat::Md,
                ConvertFormat::Html,
            ]
        );
        for f in IMAGE_FORMATS {
            let targets = available_targets(f);
            assert!(!targets.contains(&f), "目标里不该包含自身");
            assert_eq!(targets.len(), 7);
            assert!(!targets.contains(&ConvertFormat::Txt), "图片→TXT 需要 OCR");
        }
        // 文本：md / html / txt 三方互转，但不含图片（需要排版引擎）。
        for f in TEXT_FORMATS {
            let targets = available_targets(f);
            assert!(!targets.contains(&f), "目标里不该包含自身");
            assert_eq!(targets.len(), 2);
        }
    }

    #[test]
    fn image_roundtrip_png_to_jpg_and_webp() {
        let dir = scratch_dir("image");

        let mut src_img = image::RgbImage::new(8, 6);
        for (x, y, pixel) in src_img.enumerate_pixels_mut() {
            *pixel = image::Rgb([(x * 30) as u8, (y * 40) as u8, 128]);
        }
        let src = dir.join("test.png");
        src_img.save(&src).expect("save test png");

        let out_jpg = convert_file(src.to_str().unwrap(), ConvertFormat::Jpg, 90, None)
            .expect("convert png -> jpg");
        assert!(out_jpg.ends_with("test.jpg"), "{out_jpg}");
        let back = image::open(&out_jpg).expect("read back jpg");
        assert_eq!((back.width(), back.height()), (8, 6));

        let out_webp = convert_file(src.to_str().unwrap(), ConvertFormat::Webp, 90, None)
            .expect("convert png -> webp");
        let back_webp = image::open(&out_webp).expect("read back webp");
        assert_eq!((back_webp.width(), back_webp.height()), (8, 6));

        let out_png = convert_file(&out_jpg, ConvertFormat::Png, 90, None)
            .expect("convert jpg -> png");
        let back_png = image::open(&out_png).expect("read back png");
        assert_eq!((back_png.width(), back_png.height()), (8, 6));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn convert_file_rejects_invalid_requests() {
        let dir = scratch_dir("reject");
        let unknown = dir.join("a.xyz");
        fs::write(&unknown, b"data").expect("write");
        assert!(convert_file(unknown.to_str().unwrap(), ConvertFormat::Png, 85, None).is_err());

        let png = dir.join("a.png");
        fs::write(&png, b"not a real png").expect("write png");
        // 同格式且不要求重编码 → 跳过。
        assert!(convert_file(png.to_str().unwrap(), ConvertFormat::Png, 85, None).is_err());
        // 图片 → TXT 需要 OCR，图上没有这条边。
        assert!(convert_file(png.to_str().unwrap(), ConvertFormat::Txt, 85, None).is_err());
        // 损坏的图片文件：解码失败。
        assert!(convert_file(png.to_str().unwrap(), ConvertFormat::Jpg, 85, None).is_err());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn text_matrix_all_pairs() {
        let dir = scratch_dir("text-matrix");

        let txt = dir.join("a.txt");
        fs::write(&txt, "第一行\r\n第二行").expect("write txt");
        let md = convert_file(txt.to_str().unwrap(), ConvertFormat::Md, 85, None).expect("txt->md");
        assert_eq!(fs::read_to_string(&md).unwrap(), "第一行\n第二行\n");

        let txt2 = dir.join("b.txt");
        fs::write(&txt2, "<b>不是标签</b> & 保留").expect("write txt");
        let html =
            convert_file(txt2.to_str().unwrap(), ConvertFormat::Html, 85, None).expect("txt->html");
        let body = fs::read_to_string(&html).unwrap();
        assert!(
            body.contains("&lt;b&gt;不是标签&lt;/b&gt; &amp; 保留"),
            "{body}"
        );
        assert!(body.contains("<pre>"), "{body}");

        let src_html = dir.join("c.html");
        fs::write(
            &src_html,
            "<h2>标题</h2><p>正文 <strong>粗</strong> <a href=\"https://x.test\">链接</a></p>\
             <ul><li>甲</li><li>乙</li></ul><img src=\"p.png\" alt=\"图\">\
             <pre>let a = 1;</pre>",
        )
        .expect("write html");
        let md2 =
            convert_file(src_html.to_str().unwrap(), ConvertFormat::Md, 85, None).expect("html->md");
        let out = fs::read_to_string(&md2).unwrap();
        assert!(out.contains("## 标题"), "{out}");
        assert!(out.contains("正文 **粗**"), "{out}");
        assert!(out.contains("[链接](https://x.test)"), "{out}");
        assert!(out.contains("- 甲"), "{out}");
        assert!(out.contains("![图](p.png)"), "{out}");
        assert!(out.contains("```\nlet a = 1;\n```"), "{out}");

        let plain = convert_file(&md2, ConvertFormat::Txt, 85, None).expect("md->txt");
        assert!(fs::read_to_string(&plain).unwrap().contains("标题"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn image_to_document_is_honest_about_reference_vs_embed() {
        let dir = scratch_dir("image-doc");
        let mut img = image::RgbImage::new(4, 3);
        for (x, y, p) in img.enumerate_pixels_mut() {
            *p = image::Rgb([(x * 20) as u8, (y * 20) as u8, 90]);
        }
        let src = dir.join("pic.png");
        img.save(&src).expect("save png");

        // Markdown：只引用源图（同目录用文件名）。
        let md = convert_file(src.to_str().unwrap(), ConvertFormat::Md, 85, None).expect("png->md");
        assert_eq!(fs::read_to_string(&md).unwrap(), "![pic](pic.png)\n");

        // HTML：真正内联成 data URI，不再依赖外部文件。
        let html =
            convert_file(src.to_str().unwrap(), ConvertFormat::Html, 85, None).expect("png->html");
        let body = fs::read_to_string(&html).unwrap();
        assert!(body.contains("data:image/png;base64,"), "{body}");

        // 文本 → 图片需要排版引擎，图上不可达。
        let txt = dir.join("t.txt");
        fs::write(&txt, "hi").expect("write txt");
        assert!(convert_file(txt.to_str().unwrap(), ConvertFormat::Png, 85, None).is_err());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn convert_batch_text_flow_reports_every_file() {
        let dir = scratch_dir("text");
        let md = dir.join("note.md");
        fs::write(&md, "# 标题\n\n正文 **粗体**。").expect("write md");
        let md_path = md.to_str().unwrap().to_string();

        // 批量入口统一走 Job（不再有 `convert_files` 旧包装），每个输入都必须有结果。
        let batch = |target: ConvertFormat, sources: &[String]| {
            let preset = adhoc_preset(target, 85, None);
            job::run(sources, &preset, &mut |_, _, _| {})
        };

        let html_job = batch(ConvertFormat::Html, std::slice::from_ref(&md_path));
        assert_eq!(html_job.files[0].state, FileState::Done, "{:?}", html_job.files[0]);
        let html = fs::read_to_string(&html_job.files[0].outputs[0]).expect("read html");
        assert!(html.contains("<h1>标题</h1>"), "{html}");

        let txt_job = batch(ConvertFormat::Txt, std::slice::from_ref(&md_path));
        let txt = fs::read_to_string(&txt_job.files[0].outputs[0]).expect("read txt");
        assert!(txt.contains("标题"));
        assert!(txt.contains("正文 粗体。"));

        let html_file = dir.join("page.html");
        fs::write(&html_file, "<h2>Hi</h2><p>Body <i>italic</i></p>").expect("write html");
        let txt_job = batch(ConvertFormat::Txt, &[html_file.to_str().unwrap().to_string()]);
        let txt2 = fs::read_to_string(&txt_job.files[0].outputs[0]).expect("read txt2");
        assert_eq!(txt2, "Hi\nBody italic");

        // 混合批次：失败项不影响成功项，且每项都有结果。
        let mixed = batch(ConvertFormat::Html, &[md_path, unknown_path(&dir)]);
        assert_eq!(mixed.files.len(), 2);
        assert_eq!(mixed.files[0].state, FileState::Done);
        // 扩展名不认识是「跳过 + 结构化原因」，不是失败（旧版会当成错误）。
        assert_eq!(mixed.files[1].state, FileState::Skipped);
        assert_eq!(
            mixed.files[1].skip.as_ref().map(|s| s.code),
            Some(SkipCode::UnknownFormat)
        );

        let _ = fs::remove_dir_all(&dir);
    }

    fn unknown_path(dir: &std::path::Path) -> String {
        let p = dir.join("data.bin");
        fs::write(&p, b"\x00\x01").expect("write bin");
        p.to_str().unwrap().to_string()
    }

    /// 格式探测：只认支持的后缀，不认识的给 `None`（旧 `convert_probe`
    /// 命令已移除，这里保住底层能力的覆盖）。
    #[test]
    fn detect_format_recognizes_supported_files_only() {
        assert_eq!(detect_format("a/b/foo.PNG"), Some(ConvertFormat::Png));
        assert_eq!(detect_format("note.md"), Some(ConvertFormat::Md));
        assert_eq!(detect_format("mystery.xyz"), None);
        assert_eq!(detect_format("plain.txt"), Some(ConvertFormat::Txt));
    }

    /// 多步路由：TIFF 不能内联进 HTML，应该先转成可内嵌的图片再内联，
    /// 且中间产物不落地。
    #[test]
    fn tiff_to_html_uses_two_steps_and_keeps_only_final_output() {
        let dir = scratch_dir("two-step");
        let src = dir.join("scan.tiff");
        image::RgbImage::new(5, 4).save(&src).expect("save tiff");

        let preset = adhoc_preset(ConvertFormat::Html, 85, None);
        let job = job::run(&[src.to_string_lossy().to_string()], &preset, &mut |_, _, _| {});
        assert_eq!(job.files[0].state, FileState::Done, "{:?}", job.files[0]);
        // 中间格式由边权决定（PNG / JPG / WebP / GIF 都行），只断言首尾与步数。
        let route = &job.files[0].route;
        assert_eq!(route.len(), 3, "route = {route:?}");
        assert_eq!((route[0].as_str(), route[2].as_str()), ("tiff", "html"));
        assert!(
            ["png", "jpg", "webp", "gif"].contains(&route[1].as_str()),
            "中间格式不能内联进 HTML：{route:?}"
        );
        // 只有一个产物，中间文件不进输出列表。
        assert_eq!(job.files[0].outputs.len(), 1);
        assert!(job.files[0].outputs[0].ends_with("scan.html"));
        // 输出目录里也不该留下任何中间文件（源 + 最终产物之外都算泄漏）。
        let leftovers: Vec<String> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n != "scan.tiff" && n != "scan.html")
            .collect();
        assert!(leftovers.is_empty(), "中间产物泄漏：{leftovers:?}");

        let _ = fs::remove_dir_all(&dir);
    }

    /// 取消：正在跑的任务要停下后续步骤，并给出 Cancelled 结果。
    #[test]
    fn cancelled_job_reports_cancelled_state() {
        let dir = scratch_dir("cancel");
        let preset = adhoc_preset(ConvertFormat::Jpg, 85, None);
        let sources: Vec<String> = (0..8)
            .map(|i| {
                let p = dir.join(format!("img{i}.png"));
                image::RgbImage::new(16, 16).save(&p).expect("save");
                p.to_string_lossy().to_string()
            })
            .collect();

        let token = CancelToken::new();
        token.cancel();
        // 预取消：所有文件都该是 Cancelled（而不是 Done / 静默丢弃）。
        let job = job::run_with_cancel(&sources, &preset, token, &mut |_, _, _| {});
        assert_eq!(job.state, JobState::Cancelled);
        assert!(job
            .files
            .iter()
            .all(|f| f.state == FileState::Cancelled));
        assert!(job.outputs().is_empty());

        let _ = fs::remove_dir_all(&dir);
    }
}
