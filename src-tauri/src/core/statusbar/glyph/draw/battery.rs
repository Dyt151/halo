//! 电池环缺口里的数字、闪电、插头，以及充电扫光的落地绘制。

use super::super::*;
use super::*;

pub(super) fn draw_charging_effect(
    ctx: &CGContext,
    palette: &Palette,
    base_rgb: [u8; 3],
    effect: ChargingFrame,
    gap_width: f64,
) {
    let highlight = automatic_highlight(palette.resolve(base_rgb));
    let [r, g, b] = highlight.map(|c| c as f64 / 255.0);

    if let Some((start, end)) = effect.tail {
        let segments = battery_arc_segments(start, end, gap_width);
        if !segments.is_empty() {
            ctx.save();
            ctx.set_line_width(RING_LINE);
            let mut p = Path::new(ctx);
            for (a, b) in segments {
                // 每段先 move_to，避免两段之间被补上一条跨缺口的直线。
                let from = polar(RING_CX, RING_CY, RING_R, a);
                p.move_to(from.x, from.y);
                p.arc(RING_CX, RING_CY, RING_R, a, b);
            }
            ctx.replace_path_with_stroked_path();
            ctx.clip();
            let alpha = effect.tail_alpha.clamp(0.0, 1.0);
            let space = CGColorSpace::create_device_rgb();
            let gradient = CGGradient::create_with_color_components(
                &space,
                &[r, g, b, 0.0, r, g, b, alpha, r, g, b, 0.0],
                &[0.0, 0.82, 1.0],
                3,
            );
            let (x0, y0) = battery_point(start);
            let (x1, y1) = battery_point(end);
            ctx.draw_linear_gradient(
                &gradient,
                CGPoint::new(x0, y0),
                CGPoint::new(x1, y1),
                CGGradientDrawingOptions::default(),
            );
            ctx.restore();
        }
    }

    if effect.head_visible && effect.bead_alpha > 0.0 {
        let (x, y) = battery_point(effect.head);
        let radius = RING_LINE * 0.5 * 1.18;
        fill_dot(ctx, x, y, radius, highlight, effect.bead_alpha);
    }

    if effect.head_visible && effect.heartbeat_alpha > 0.0 {
        let (x, y) = battery_point(effect.head);
        let radius = RING_LINE * 0.95 * effect.heartbeat_scale;
        fill_dot(ctx, x, y, radius, highlight, effect.heartbeat_alpha);
    }
}

pub(crate) fn fill_dot(ctx: &CGContext, cx: f64, cy: f64, radius: f64, rgb: [u8; 3], alpha: f64) {
    ctx.set_rgb_fill_color(
        rgb[0] as f64 / 255.0,
        rgb[1] as f64 / 255.0,
        rgb[2] as f64 / 255.0,
        alpha.clamp(0.0, 1.0),
    );
    ctx.fill_ellipse_in_rect(CGRect::new(
        &CGPoint::new(cx - radius, cy - radius),
        &CGSize::new(radius * 2.0, radius * 2.0),
    ));
}

/// 电量数字：坐进环顶缺口，墨色（正版也是 `.primary`，电池色只归弧）。
pub(super) fn draw_digits(ctx: &CGContext, palette: &Palette, percent: u8) {
    let Some(font) = value_font() else { return };
    let cg = font.copy_to_CGFont();
    let text = percent.min(100).to_string();
    let glyphs: Vec<u16> = text
        .chars()
        .map(|c| {
            font.get_glyph_with_name(match c {
                '0' => "zero",
                '1' => "one",
                '2' => "two",
                '3' => "three",
                '4' => "four",
                '5' => "five",
                '6' => "six",
                '7' => "seven",
                '8' => "eight",
                _ => "nine",
            })
        })
        .collect();
    let upem = cg.get_units_per_em() as f64;
    let mut advances = vec![0i32; glyphs.len()];
    cg.get_glyph_advances(&glyphs, &mut advances);
    let widths: Vec<f64> = advances
        .iter()
        .map(|a| *a as f64 / upem * FONT_SIZE)
        .collect();
    let total: f64 = widths.iter().sum::<f64>() + FONT_KERN * (widths.len() as f64 - 1.0);
    // 基线随字号线性偏移（正版对 20→17、32→24 两点拟合出来的斜率 7/12）。
    let baseline = 17.0 + (FONT_SIZE - 20.0) * (7.0 / 12.0);
    let mut x = AX - total / 2.0;
    // 文字矩阵已经翻了 y，笔位就得按文字空间给负值；否则整串数字被推到画布外。
    let positions: Vec<CGPoint> = widths
        .iter()
        .map(|w| {
            let p = CGPoint::new(x, -baseline);
            x += w + FONT_KERN;
            p
        })
        .collect();
    set_ink(ctx, palette, palette.ink, 1.0);
    ctx.set_font(&cg);
    ctx.set_font_size(FONT_SIZE);
    ctx.show_glyphs_at_positions(&glyphs, &positions);
}

/// 「100」的数字墨高：正版用它换算闪电的大小。
fn digit_ink_height() -> Option<f64> {
    let font = value_font()?;
    let glyphs: Vec<u16> = ["one", "zero", "zero"]
        .iter()
        .map(|n| font.get_glyph_with_name(n))
        .collect();
    let rect = font.get_bounding_rects_for_glyphs(
        core_text::font_descriptor::kCTFontHorizontalOrientation,
        &glyphs,
    );
    if rect.size.height.is_finite() && rect.size.height > 0.0 {
        Some(rect.size.height)
    } else {
        None
    }
}

/// 顶部指示符（闪电或插头）的排版结果：正版 `batteryTopIndicatorCenter`
/// 的水准点，以及缩放后的字形包围盒。
struct IndicatorLayout {
    /// 目标墨迹高度（= 数字墨高 × 校准比）。
    target: f64,
    center_x: f64,
    center_y: f64,
    min_y: f64,
    max_y: f64,
}

/// 闪电绕自己的尖端缩放到校准高度，同时得到指示符共用的水准点。
fn top_indicator_layout() -> IndicatorLayout {
    let target = digit_ink_height()
        .map(|h| h * BOLT_CALIBRATION)
        .unwrap_or(BOLT_HEIGHT * TEXT_SCALE);
    let indicator = target / BOLT_HEIGHT;
    let grow = |value: f64, pivot: f64| pivot + (value - pivot) * indicator;
    let (min_x, max_x) = (grow(BOLT_MIN_X, BOLT_PIVOT_X), grow(BOLT_MAX_X, BOLT_PIVOT_X));
    let (min_y, max_y) = (grow(BOLT_MIN_Y, BOLT_PIVOT_Y), grow(BOLT_MAX_Y, BOLT_PIVOT_Y));
    IndicatorLayout {
        target,
        center_x: (min_x + max_x) / 2.0,
        center_y: (min_y + max_y) / 2.0,
        min_y,
        max_y,
    }
}

/// 充电闪电：只放闪电时缺口收窄到 50，尺寸按数字墨高校准。
///
/// 有扫光帧时再叠一层正版的 `centeredScale`（绕字形包围盒中心放大，纵向
/// 卡在画布内），并把手心扫到缺口时的放大/提色应用上去。
pub(super) fn draw_bolt(
    ctx: &CGContext,
    palette: &Palette,
    spec: &GlyphSpec,
    effect: Option<ChargingFrame>,
) {
    let layout = top_indicator_layout();
    let IndicatorLayout {
        center_x,
        center_y,
        min_y,
        max_y,
        ..
    } = layout;
    let indicator = layout.target / BOLT_HEIGHT;

    let animated = effect.map(|e| e.bolt_scale).unwrap_or(1.0);
    let vertical = animated
        .min(center_y / (center_y - min_y))
        .min((CANVAS - center_y) / (max_y - center_y));

    let s = |x: f64, y: f64| {
        let x1 = BOLT_PIVOT_X + (x - BOLT_PIVOT_X) * indicator;
        let y1 = BOLT_PIVOT_Y + (y - BOLT_PIVOT_Y) * indicator;
        (
            center_x + (x1 - center_x) * animated,
            center_y + (y1 - center_y) * vertical,
        )
    };

    let bolt_rgb = match effect {
        Some(effect) if palette.colored && spec.battery.is_some() => {
            let arc = palette.battery_rgb(spec.battery.unwrap_or(0), spec.low_power, true);
            let tint = charging_bolt_highlight(arc, automatic_highlight(arc));
            blend_rgb(palette.ink, tint, effect.bolt_arc_color_amount)
        }
        _ => palette.ink,
    };
    set_ink(ctx, palette, bolt_rgb, 1.0);
    let mut p = Path::new(ctx);
    p.move_to(s(62.1, 2.2).0, s(62.1, 2.2).1);
    p.quad_to(
        s(62.8, 2.5).0,
        s(62.8, 2.5).1,
        s(62.6, 3.3).0,
        s(62.6, 3.3).1,
    );
    p.line_to(s(61.2, 7.8).0, s(61.2, 7.8).1);
    p.line_to(s(65.9, 7.8).0, s(65.9, 7.8).1);
    p.quad_to(
        s(66.9, 7.8).0,
        s(66.9, 7.8).1,
        s(67.3, 8.6).0,
        s(67.3, 8.6).1,
    );
    p.quad_to(
        s(67.6, 9.3).0,
        s(67.6, 9.3).1,
        s(67.0, 10.0).0,
        s(67.0, 10.0).1,
    );
    p.line_to(s(57.0, 21.3).0, s(57.0, 21.3).1);
    p.quad_to(
        s(56.4, 22.0).0,
        s(56.4, 22.0).1,
        s(55.6, 21.6).0,
        s(55.6, 21.6).1,
    );
    p.quad_to(
        s(55.0, 21.3).0,
        s(55.0, 21.3).1,
        s(55.3, 20.5).0,
        s(55.3, 20.5).1,
    );
    p.line_to(s(57.4, 14.1).0, s(57.4, 14.1).1);
    p.line_to(s(52.9, 14.1).0, s(52.9, 14.1).1);
    p.quad_to(
        s(52.0, 14.1).0,
        s(52.0, 14.1).1,
        s(51.6, 13.3).0,
        s(51.6, 13.3).1,
    );
    p.quad_to(
        s(51.3, 12.6).0,
        s(51.3, 12.6).1,
        s(51.9, 12.0).0,
        s(51.9, 12.0).1,
    );
    p.line_to(s(61.1, 2.7).0, s(61.1, 2.7).1);
    p.quad_to(
        s(61.6, 2.1).0,
        s(61.6, 2.1).1,
        s(62.1, 2.2).0,
        s(62.1, 2.2).1,
    );
    p.close();
    ctx.fill_path();
}

/// 接电源但没在充电：缺口里放插头（正版 `connectedIndicator` 的默认内容）。
///
/// 排版照抄正版 `drawBatteryPlug`：排版盒高度 = 闪电校准高度 × 1.2，
/// 按 `powerplug.portrait.fill` 实测的排版盒比例（192×266）换算出盒宽，
/// 盒**竖直**居中在指示符水准点上、**墨迹**水平居中在轴线上；插针朝上。
pub(super) fn draw_plug(ctx: &CGContext, palette: &Palette) {
    let layout = top_indicator_layout();
    let box_h = layout.target * PLUG_HEIGHT_SCALE;
    let box_w = box_h * PLUG_BOX_ASPECT;
    let top = layout.center_y - box_h / 2.0;
    let at = |(u, v): (f64, f64)| {
        (
            layout.center_x + (u - 0.5) * box_w,
            top + v * box_h,
        )
    };
    set_ink(ctx, palette, palette.ink, 1.0);
    let mut p = Path::new(ctx);
    let (x0, y0) = at(PLUG_INK[0]);
    p.move_to(x0, y0);
    for &point in &PLUG_INK[1..] {
        let (x, y) = at(point);
        p.line_to(x, y);
    }
    p.close();
    ctx.fill_path();
}

