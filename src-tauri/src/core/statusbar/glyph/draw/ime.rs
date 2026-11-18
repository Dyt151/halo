//! 输入法切换字牌。

use super::super::*;
use super::*;

pub(super) fn draw_ime_chip(ctx: &CGContext, palette: &Palette, flash: ImeFlash) {
    let alpha = flash.alpha.clamp(0.0, 1.0);
    if alpha <= 0.0 {
        return;
    }
    ctx.save();
    if (flash.scale - 1.0).abs() > 1e-6 {
        ctx.translate(IME_CX, IME_CY);
        ctx.scale(flash.scale, flash.scale);
        ctx.translate(-IME_CX, -IME_CY);
    }
    set_ink(ctx, palette, palette.ink, alpha);
    let (x, y) = (IME_CX - IME_W / 2.0, IME_CY - IME_H / 2.0);
    if flash.filled {
        let mut p = Path::new(ctx);
        p.rounded_rect(x, y, IME_W, IME_H, IME_R);
        ctx.fill_path();
        // 正版把字从字牌里挖空，壁纸从字里透出来。
        ctx.set_blend_mode(CGBlendMode::DestinationOut);
        draw_ime_text(ctx, palette, flash.marker, alpha);
        ctx.set_blend_mode(CGBlendMode::Normal);
    } else {
        // 描边画在路径中心，向内收半个线宽，外沿才落在字牌边界上。
        let inset = IME_STROKE / 2.0;
        let mut p = Path::new(ctx);
        p.rounded_rect(
            x + inset,
            y + inset,
            IME_W - IME_STROKE,
            IME_H - IME_STROKE,
            IME_R - inset,
        );
        ctx.set_line_width(IME_STROKE);
        ctx.stroke_path();
        draw_ime_text(ctx, palette, flash.marker, alpha);
    }
    ctx.restore();
}

/// 字牌字符：用 CTLine 画（中文/假名/韩文各自走系统字体，自带字形回落），
/// 再按墨迹包围盒把字符摆到字牌正中。
pub(super) fn draw_ime_text(ctx: &CGContext, palette: &Palette, marker: char, alpha: f64) {
    use core_foundation::attributed_string::CFMutableAttributedString;
    use core_foundation::base::{CFRange, TCFType};
    use core_foundation::string::CFString;
    use core_graphics::color::CGColor;
    use core_text::line::CTLine;

    let Some(font) = ime_font(marker) else { return };
    let text = CFString::new(&marker.to_string());
    let mut attr = CFMutableAttributedString::new();
    attr.replace_str(&text, CFRange::init(0, 0));
    let range = CFRange::init(0, text.char_len());
    unsafe {
        attr.set_attribute(range, core_text::string_attributes::kCTFontAttributeName, font);
        let ink = palette.resolve(palette.ink);
        let color = CGColor::rgb(
            ink[0] as f64 / 255.0,
            ink[1] as f64 / 255.0,
            ink[2] as f64 / 255.0,
            alpha,
        );
        attr.set_attribute(
            range,
            core_text::string_attributes::kCTForegroundColorAttributeName,
            &color,
        );
    }
    let line = CTLine::new_with_attributed_string(attr.as_concrete_TypeRef());
    // 笔位在用户空间，而 `get_image_bounds` 给的是文本空间（y 向上）的墨迹框：
    // 水平居中直接减，垂直要把笔位抬到墨迹框底下再向下让「框高 - 半高」。
    ctx.set_text_position(0.0, 0.0);
    let bounds = line.get_image_bounds(ctx);
    let dx = IME_CX - (bounds.origin.x + bounds.size.width / 2.0);
    let dy = IME_CY + (bounds.origin.y + bounds.size.height / 2.0);
    ctx.set_text_position(dx, dy);
    line.draw(ctx);
}

/// 字牌字体按字符脚本取系统字体；否则 CTFontCreateWithName 拿不到 SF 的
/// 命名实例（会静默退到 Times），中文字符也会变成豆腐块。
fn ime_font(marker: char) -> Option<&'static core_text::font::CTFont> {
    static LATIN: std::sync::OnceLock<Option<core_text::font::CTFont>> =
        std::sync::OnceLock::new();
    static CJK: std::sync::OnceLock<Option<core_text::font::CTFont>> =
        std::sync::OnceLock::new();
    static KANA: std::sync::OnceLock<Option<core_text::font::CTFont>> =
        std::sync::OnceLock::new();
    static HANGUL: std::sync::OnceLock<Option<core_text::font::CTFont>> =
        std::sync::OnceLock::new();

    let fallback = |cell: &'static std::sync::OnceLock<Option<core_text::font::CTFont>>,
                    name: &str,
                    size: f64| {
        cell.get_or_init(|| core_text::font::new_from_name(name, size).ok())
            .as_ref()
    };
    match marker as u32 {
        0x3040..=0x30FF => fallback(&KANA, "HiraginoSans-W6", IME_CJK_SIZE),
        0xAC00..=0xD7AF => fallback(&HANGUL, "AppleSDGothicNeo-SemiBold", IME_CJK_SIZE),
        _ if marker.is_ascii() => LATIN
            .get_or_init(|| sf_named_instance(".SFNS-Semibold", IME_LATIN_SIZE))
            .as_ref(),
        _ => fallback(&CJK, "PingFangSC-Semibold", IME_CJK_SIZE),
    }
}

/// 从 SFNS.ttf 里挑命名实例（与电量数字取 `.SFNSRounded-Bold` 同一套办法）。
fn sf_named_instance(instance: &str, size: f64) -> Option<core_text::font::CTFont> {
    let url =
        core_foundation::url::CFURL::from_path("/System/Library/Fonts/SFNS.ttf", false)?;
    let descs: core_foundation::array::CFArray<
        core_text::font_descriptor::CTFontDescriptor,
    > = unsafe {
        let raw = core_text::font_manager::CTFontManagerCreateFontDescriptorsFromURL(
            url.as_concrete_TypeRef(),
        );
        if raw.is_null() {
            return None;
        }
        core_foundation::base::TCFType::wrap_under_create_rule(raw)
    };
    let desc = descs.iter().find(|d| d.font_name() == instance)?;
    Some(core_text::font::new_from_descriptor(&*desc, size))
}

