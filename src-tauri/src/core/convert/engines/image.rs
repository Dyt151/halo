//! 图片编解码引擎（`image` crate）。
//!
//! 声明的边：PNG / JPG / WebP / GIF / BMP / TIFF 六种编解码格式两两互转 +
//! 每种格式的 self edge（同格式重编码）。self edge 只是「引擎有能力」，是否
//! 真的执行由 preset 的 `reencode_same_format` 决定——JPG→JPG 降质量是合法
//! 任务，PNG→PNG 无事可做就该在规划前被判成「已是目标格式」（§2.4 / 验收 P0）。
//!
//! 两条不静默保真的语义（§5.1）：
//! - 透明图转到无 alpha 格式（JPG）先按 preset 背景色合成，绝不留下随机黑底；
//! - EXIF 方向烘焙进像素（`Orientation` 由解码器给出），输出不带 EXIF；
//! - 动画 GIF 只有 GIF→GIF 才逐帧保留，其余目标保留首帧并**记警告**。

use std::fs::File;
use std::io::{BufReader, Write};
use std::path::Path;

use image::{AnimationDecoder, DynamicImage, ImageDecoder};

use super::super::edge::{
    ConversionEdge, ConversionStep, ConvertError, ConverterEngine, EdgeMode, EngineId,
    EngineStatus, StepOutput,
};
use super::super::format::{ConvertFormat, IMAGE_CODEC_FORMATS};

/// 图片引擎：解码 → （变换）→ 编码。
pub struct ImageEngine;

/// 单个图片文件的体积上限（512 MB）。
///
/// 解码是「整个文件读进内存」再交给 `image` 的，1 GB 的原图会让 4 个 worker
/// 同时吃满内存；512 MB 已经远超人能在剪贴板工具里处理的图片。
const MAX_IMAGE_BYTES: u64 = 512 * 1024 * 1024;

/// 解码后的像素上限（约 80 MP，RGBA 约 320 MB）。
///
/// 防的是「压缩炸弹」：一个 2 KB 的 PNG 可以声明 50000×50000，解码时
/// 申请 10 GB 内存，分配失败会直接 abort 整个进程（不是可捕获的错误）。
const MAX_IMAGE_PIXELS: u64 = 80_000_000;

/// 读入前的体积闸门（返回给用户的错误是中文，不带底层类型名）。
fn guard_file_size(path: &Path) -> Result<(), ConvertError> {
    let Ok(meta) = std::fs::metadata(path) else {
        // 读不到元信息就交给后面的真实读取去报错。
        return Ok(());
    };
    if meta.len() > MAX_IMAGE_BYTES {
        return Err(ConvertError::io(format!(
            "图片太大（{:.0} MB），上限 {} MB",
            meta.len() as f64 / (1024.0 * 1024.0),
            MAX_IMAGE_BYTES / (1024 * 1024)
        )));
    }
    Ok(())
}

impl ConverterEngine for ImageEngine {
    fn id(&self) -> EngineId {
        EngineId::Image
    }

    fn display_name(&self) -> &'static str {
        "图片编解码"
    }

    fn max_parallelism(&self) -> usize {
        // 大图解码是内存大户，限制到 2 路。
        2
    }

    fn status(&self) -> EngineStatus {
        EngineStatus::Available
    }

    fn edges(&self) -> Vec<ConversionEdge> {
        let mut out = Vec::with_capacity(IMAGE_CODEC_FORMATS.len() * IMAGE_CODEC_FORMATS.len());
        for from in IMAGE_CODEC_FORMATS {
            for to in IMAGE_CODEC_FORMATS {
                let cost = if from == to { 12 } else { 10 };
                out.push(ConversionEdge::new(
                    from,
                    to,
                    EngineId::Image,
                    cost,
                    EdgeMode::Normal,
                ));
            }
        }
        out
    }

    fn convert(&self, step: &ConversionStep<'_>) -> Result<StepOutput, ConvertError> {
        guard_file_size(step.input)?;

        // 动画 GIF → GIF：逐帧重编码，别把动图塌成一张静图。
        if step.from == ConvertFormat::Gif && step.to == ConvertFormat::Gif {
            if let Some(output) = reencode_gif(step.input, step.output)? {
                return Ok(output);
            }
        }

        let decoded = decode(step.input, step.from)?;
        let mut warnings: Vec<String> = Vec::new();

        warnings.extend(decoded.warnings.iter().cloned());
        if decoded.animated && step.to != ConvertFormat::Gif {
            warnings.push("动画 GIF 只保留第一帧：目标格式不支持帧序列".to_string());
        }

        // 透明 → 无 alpha 目标：按 preset 背景色合成。
        let mut image = decoded.image;
        if image.color().has_alpha() && !step.to.supports_alpha() {
            let bg = step.preset.image.background_rgb();
            warnings.push(format!(
                "透明通道已合成到背景 #{:02X}{:02X}{:02X}",
                bg[0], bg[1], bg[2]
            ));
            image = flatten_alpha(&image, bg);
        }

        encode_image(&image, step.output, step.to, step.preset.image.jpeg_quality)?;
        Ok(StepOutput { warnings })
    }
}

/// 解码结果 + 需要向用户声明的限制。
struct Decoded {
    image: DynamicImage,
    /// 是否为多帧动画（只对 GIF 判断）。
    animated: bool,
    warnings: Vec<String>,
}

/// 解码：应用 EXIF 方向、记录 ICC / 元数据丢失。
///
/// 先按魔数猜测格式；猜不出（TGA 没有可靠魔数）就按 `from` 的扩展名兜底——
/// 路由阶段已经按扩展名定过格式，这里不需要重新发明格式探测。
fn decode(path: &Path, from: ConvertFormat) -> Result<Decoded, ConvertError> {
    let bytes = std::fs::read(path).map_err(|e| ConvertError::io(format!("读取图片失败：{e}")))?;
    let mut warnings: Vec<String> = Vec::new();

    if has_icc_profile(&bytes) {
        warnings.push("ICC 色彩配置未随产物保留（按 sRGB 解释）".to_string());
    }

    let animated = is_animated_gif(&bytes);
    if animated {
        warnings.push("源文件是多帧动画 GIF".to_string());
    }

    let reader = match image::ImageReader::new(std::io::Cursor::new(bytes.clone()))
        .with_guessed_format()
    {
        Ok(reader) => reader,
        Err(_) => {
            let format = image::ImageFormat::from_extension(from.as_str())
                .ok_or_else(|| ConvertError::decode("无法识别图片内容"))?;
            image::ImageReader::with_format(std::io::Cursor::new(bytes), format)
        }
    };
    let mut decoder = reader
        .into_decoder()
        .map_err(|e| ConvertError::decode(format!("无法初始化解码器：{e}")))?;
    // 先问尺寸再解码：分配失败是 abort，不是可以用 Result 接住的错误。
    let (width, height) = decoder.dimensions();
    if u64::from(width) * u64::from(height) > MAX_IMAGE_PIXELS {
        return Err(ConvertError::decode(format!(
            "图片尺寸太大（{width}×{height}），解码需约 {} MB 内存，上限 {} 万像素",
            (u64::from(width) * u64::from(height) * 4) / (1024 * 1024),
            MAX_IMAGE_PIXELS / 10_000
        )));
    }
    let orientation = decoder
        .orientation()
        .map_err(|e| ConvertError::decode(format!("读取 EXIF 方向失败：{e}")))?;
    let mut image = DynamicImage::from_decoder(decoder)
        .map_err(|e| ConvertError::decode(format!("解码图片失败：{e}")))?;

    // 把方向烘焙进像素：输出不带 EXIF，不烘焙就会横躺。
    if orientation != image::metadata::Orientation::NoTransforms {
        image.apply_orientation(orientation);
        warnings.push("已按 EXIF 方向摆正像素（输出不带 EXIF）".to_string());
    }

    Ok(Decoded {
        image,
        animated,
        warnings,
    })
}

/// 动画 GIF → GIF：保留帧序列与延迟。单帧或非 GIF 返回 `Ok(None)`。
fn reencode_gif(input: &Path, output: &Path) -> Result<Option<StepOutput>, ConvertError> {
    let file =
        File::open(input).map_err(|e| ConvertError::io(format!("打开 GIF 失败：{e}")))?;
    let decoder = image::codecs::gif::GifDecoder::new(BufReader::new(file))
        .map_err(|e| ConvertError::decode(format!("解码 GIF 失败：{e}")))?;
    let frames = decoder
        .into_frames()
        .collect_frames()
        .map_err(|e| ConvertError::decode(format!("读取 GIF 帧失败：{e}")))?;

    if frames.len() <= 1 {
        return Ok(None);
    }

    let out = File::create(output).map_err(|e| ConvertError::io(format!("创建输出失败：{e}")))?;
    let mut writer = std::io::BufWriter::new(out);
    image::codecs::gif::GifEncoder::new(&mut writer)
        .encode_frames(frames.iter().cloned())
        .map_err(|e| ConvertError::encode(format!("编码 GIF 失败：{e}")))?;
    writer
        .flush()
        .map_err(|e| ConvertError::io(format!("写入输出失败：{e}")))?;

    Ok(Some(StepOutput {
        warnings: vec![format!("已逐帧保留动画，共 {} 帧", frames.len())],
    }))
}

/// 透明通道按背景色合成（source-over，alpha 已乘在像素上）。
fn flatten_alpha(img: &DynamicImage, bg: [u8; 3]) -> DynamicImage {
    let mut rgba = img.to_rgba8();
    for pixel in rgba.pixels_mut() {
        let a = pixel[3] as u32;
        if a == 255 {
            continue;
        }
        for (ch, bg_ch) in bg.iter().enumerate() {
            let value = (pixel[ch] as u32 * a + *bg_ch as u32 * (255 - a)) / 255;
            pixel[ch] = value as u8;
        }
        pixel[3] = 255;
    }
    DynamicImage::ImageRgba8(rgba)
}

/// 按目标格式选择编码器（JPEG 走质量参数，其余无损）。
fn encode_image(
    img: &DynamicImage,
    output: &Path,
    target: ConvertFormat,
    jpeg_quality: u8,
) -> Result<(), ConvertError> {
    use image::ImageEncoder;

    let file = File::create(output).map_err(|e| ConvertError::io(format!("创建输出失败：{e}")))?;
    let mut writer = std::io::BufWriter::new(file);

    match target {
        ConvertFormat::Png => {
            let buf = img.to_rgba8();
            image::codecs::png::PngEncoder::new(&mut writer)
                .write_image(
                    buf.as_raw(),
                    buf.width(),
                    buf.height(),
                    image::ExtendedColorType::Rgba8,
                )
                .map_err(|e| ConvertError::encode(format!("PNG 编码失败：{e}")))?;
        }
        ConvertFormat::Jpg => {
            // JPEG 不支持 alpha：调用方已按背景色合成，这里再转 RGB 兜底。
            let buf = img.to_rgb8();
            image::codecs::jpeg::JpegEncoder::new_with_quality(
                &mut writer,
                jpeg_quality.clamp(1, 100),
            )
            .write_image(
                buf.as_raw(),
                buf.width(),
                buf.height(),
                image::ExtendedColorType::Rgb8,
            )
            .map_err(|e| ConvertError::encode(format!("JPEG 编码失败：{e}")))?;
        }
        ConvertFormat::Gif => {
            let buf = img.to_rgba8();
            image::codecs::gif::GifEncoder::new(&mut writer)
                .write_image(
                    buf.as_raw(),
                    buf.width(),
                    buf.height(),
                    image::ExtendedColorType::Rgba8,
                )
                .map_err(|e| ConvertError::encode(format!("GIF 编码失败：{e}")))?;
        }
        ConvertFormat::Webp => {
            let buf = img.to_rgba8();
            image::codecs::webp::WebPEncoder::new_lossless(&mut writer)
                .write_image(
                    buf.as_raw(),
                    buf.width(),
                    buf.height(),
                    image::ExtendedColorType::Rgba8,
                )
                .map_err(|e| ConvertError::encode(format!("WebP 编码失败：{e}")))?;
        }
        ConvertFormat::Bmp => {
            let buf = img.to_rgba8();
            image::codecs::bmp::BmpEncoder::new(&mut writer)
                .write_image(
                    buf.as_raw(),
                    buf.width(),
                    buf.height(),
                    image::ExtendedColorType::Rgba8,
                )
                .map_err(|e| ConvertError::encode(format!("BMP 编码失败：{e}")))?;
        }
        ConvertFormat::Tiff => {
            let buf = img.to_rgba8();
            image::codecs::tiff::TiffEncoder::new(&mut writer)
                .write_image(
                    buf.as_raw(),
                    buf.width(),
                    buf.height(),
                    image::ExtendedColorType::Rgba8,
                )
                .map_err(|e| ConvertError::encode(format!("TIFF 编码失败：{e}")))?;
        }
        other => {
            return Err(ConvertError::unsupported(format!(
                "{} 不是图片格式，不能按图片编码",
                other.label()
            )))
        }
    }

    writer
        .flush()
        .map_err(|e| ConvertError::io(format!("写入输出失败：{e}")))
}

/// 快速判断是否为多帧 GIF（只解两帧，不整段解码）。
fn is_animated_gif(bytes: &[u8]) -> bool {
    let cursor = std::io::Cursor::new(bytes.to_vec());
    match image::codecs::gif::GifDecoder::new(cursor) {
        Ok(decoder) => decoder.into_frames().take(2).count() > 1,
        Err(_) => false,
    }
}

/// 是否带 ICC 色彩配置（JPEG APP2 / PNG iCCP / WebP ICCP）。
fn has_icc_profile(bytes: &[u8]) -> bool {
    contains(bytes, b"ICC_PROFILE") || contains(bytes, b"iCCP") || contains(bytes, b"ICCP")
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || haystack.len() < needle.len() {
        return false;
    }
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::convert::preset::{ConversionPreset, ImageOptions, TargetSpec};

    fn preset(background: &str) -> ConversionPreset {
        ConversionPreset {
            id: "test".into(),
            name: "test".into(),
            kind: crate::core::convert::preset::PresetKind::Convert,
            target: TargetSpec::Explicit(ConvertFormat::Jpg),
            image: ImageOptions {
                jpeg_quality: 90,
                alpha_background: background.into(),
            },
            output: Default::default(),
            reencode_same_format: false,
            builtin: false,
        }
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "clipbase-img-engine-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create scratch");
        dir
    }

    /// 透明 PNG → JPG 必须落到 presse 的背景色，而不是随机黑底。
    #[test]
    fn transparent_to_jpeg_uses_declared_background() {
        let dir = scratch("alpha");
        let src = dir.join("alpha.png");
        let mut rgba = image::RgbaImage::new(4, 4);
        for p in rgba.pixels_mut() {
            *p = image::Rgba([255, 0, 0, 0]); // 全透明
        }
        rgba.save(&src).expect("save png");

        let out = dir.join("out.jpg");
        let p = preset("#00FF00");
        let step = ConversionStep {
            input: &src,
            output: &out,
            from: ConvertFormat::Png,
            to: ConvertFormat::Jpg,
            final_format: ConvertFormat::Jpg,
            preset: &p,
            original: &src,
            is_final: true,
        };
        let result = ImageEngine.convert(&step).expect("convert");
        assert!(
            result.warnings.iter().any(|w| w.contains("#00FF00")),
            "{:?}",
            result.warnings
        );

        let back = image::open(&out).expect("open jpg").to_rgb8();
        let px = back.get_pixel(0, 0);
        // JPEG 有损，给一点容差。
        assert!(px[1] > 200, "绿色背景没生效：{px:?}");
        assert!(px[0] < 60, "红色残留：{px:?}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn icc_and_metadata_are_reported_not_silently_dropped() {
        let dir = scratch("icc");
        let src = dir.join("plain.png");
        image::RgbImage::new(2, 2).save(&src).expect("save");
        let out = dir.join("out.webp");
        let p = preset("#FFFFFF");
        let step = ConversionStep {
            input: &src,
            output: &out,
            from: ConvertFormat::Png,
            to: ConvertFormat::Webp,
            final_format: ConvertFormat::Webp,
            preset: &p,
            original: &src,
            is_final: true,
        };
        // 无 ICC / 无动画的普通图不该产生噪音警告。
        let result = ImageEngine.convert(&step).expect("convert");
        assert!(result.warnings.is_empty(), "{:?}", result.warnings);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
