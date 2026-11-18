//! CoreGraphics 绘制原语、整图编排与按区域拆分的绘制函数。

use super::*;

use core_foundation::base::TCFType as _;
use core_graphics::color_space::CGColorSpace;
use core_graphics::context::{
    CGBlendMode, CGContext, CGLineCap, CGLineJoin, CGPathDrawingMode,
};
use core_graphics::geometry::{CGAffineTransform, CGPoint, CGRect, CGSize};
use core_graphics::gradient::{CGGradient, CGGradientDrawingOptions};

const SUP: usize = 4;
const BUF: usize = IMAGE_SIZE * SUP;
/// 每设计单位多少输出像素。
const PX: f64 = BUF as f64 / CANVAS;

/// 系统圆体不能用 `CTFontCreateWithName("SF Pro Rounded")` 拿：这个名字在本机
/// 会被静默降级成 Helvetica。正版走 SwiftUI 的 `design: .rounded`，等价做法是
/// 直接从字体文件枚举命名实例，取 `.SFNSRounded-Bold`。
static VALUE_FONT: std::sync::OnceLock<Option<core_text::font::CTFont>> =
    std::sync::OnceLock::new();

fn value_font() -> Option<&'static core_text::font::CTFont> {
    VALUE_FONT
        .get_or_init(|| {
            let url = core_foundation::url::CFURL::from_path(
                "/System/Library/Fonts/SFNSRounded.ttf",
                false,
            )
            .or_else(|| {
                core_foundation::url::CFURL::from_path(
                    "/System/Library/Fonts/Supplemental/SFNSRounded.ttf",
                    false,
                )
            })?;
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
            let desc = descs
                .iter()
                .find(|d| d.font_name() == ".SFNSRounded-Bold")
                .or_else(|| descs.iter().find(|d| d.font_name().contains("Bold")))?;
            Some(core_text::font::new_from_descriptor(&*desc, FONT_SIZE))
        })
        .as_ref()
}

/// 往当前路径追加内容的构造器。
///
/// `CGContext` 不暴露当前笔位，而二次曲线转三次需要它，所以自己记一份。
struct Path<'a> {
    ctx: &'a CGContext,
    x: f64,
    y: f64,
    started: bool,
}

impl<'a> Path<'a> {
    fn new(ctx: &'a CGContext) -> Self {
        ctx.begin_path();
        Self {
            ctx,
            x: 0.0,
            y: 0.0,
            started: false,
        }
    }

    fn set(&mut self, x: f64, y: f64) {
        self.x = x;
        self.y = y;
        self.started = true;
    }

    fn move_to(&mut self, x: f64, y: f64) {
        self.ctx.move_to_point(x, y);
        self.set(x, y);
    }

    fn line_to(&mut self, x: f64, y: f64) {
        self.ctx.add_line_to_point(x, y);
        self.set(x, y);
    }

    /// 二次贝塞尔 → 三次（CoreGraphics 只提供三次）。
    fn quad_to(&mut self, cx: f64, cy: f64, x: f64, y: f64) {
        let (x0, y0) = (self.x, self.y);
        self.ctx.add_curve_to_point(
            x0 + 2.0 / 3.0 * (cx - x0),
            y0 + 2.0 / 3.0 * (cy - y0),
            x + 2.0 / 3.0 * (cx - x),
            y + 2.0 / 3.0 * (cy - y),
            x,
            y,
        );
        self.set(x, y);
    }

    /// 圆弧：`from`→`to` 角度制，方向由两者大小决定。
    ///
    /// core-graphics 0.25 没有 `CGContextAddArc` 的包装，只能按每段 ≤90° 拆三次贝塞尔。
    /// 起点与当前笔位不同时 CoreGraphics 会自动补一条连线——正版的扇形正是靠这一步
    /// 把弧的端点连到圆心的。
    fn arc(&mut self, cx: f64, cy: f64, r: f64, from: f64, to: f64) {
        let span = to - from;
        if span == 0.0 {
            return;
        }
        let dir = span.signum();
        let steps = (span.abs() / 90.0).ceil().max(1.0) as i32;
        let seg = span / steps as f64;
        // 三次贝塞尔逼近圆弧：控制点长度 = (4/3)·tan(θ/4)·r，θ 为单段的圆心角。
        let handle = (4.0 / 3.0) * deg(seg.abs() / 4.0).tan() * dir * r;
        let start = polar(cx, cy, r, from);
        if !self.started {
            self.ctx.move_to_point(start.x, start.y);
        }
        for i in 0..steps {
            let a0 = from + seg * i as f64;
            let a1 = a0 + seg;
            let p0 = polar(cx, cy, r, a0);
            let p1 = polar(cx, cy, r, a1);
            let t0 = tangent(a0);
            let t1 = tangent(a1);
            self.ctx.add_curve_to_point(
                p0.x + t0.0 * handle,
                p0.y + t0.1 * handle,
                p1.x - t1.0 * handle,
                p1.y - t1.1 * handle,
                p1.x,
                p1.y,
            );
            self.set(p1.x, p1.y);
        }
        if !self.started {
            self.set(start.x, start.y);
        }
    }

    /// 圆角矩形（正版临时网络里的显示器轮廓）。
    fn rounded_rect(&mut self, x: f64, y: f64, w: f64, h: f64, r: f64) {
        let k = 2.0 / 3.0;
        self.move_to(x + r, y);
        self.line_to(x + w - r, y);
        self.ctx.add_curve_to_point(
            x + w - r * (1.0 - k),
            y,
            x + w,
            y + r * (1.0 - k),
            x + w,
            y + r,
        );
        self.set(x + w, y + r);
        self.line_to(x + w, y + h - r);
        self.ctx.add_curve_to_point(
            x + w,
            y + h - r * (1.0 - k),
            x + w - r * (1.0 - k),
            y + h,
            x + w - r,
            y + h,
        );
        self.set(x + w - r, y + h);
        self.line_to(x + r, y + h);
        self.ctx.add_curve_to_point(
            x + r * (1.0 - k),
            y + h,
            x,
            y + h - r * (1.0 - k),
            x,
            y + h - r,
        );
        self.set(x, y + h - r);
        self.line_to(x, y + r);
        self.ctx
            .add_curve_to_point(x, y + r * (1.0 - k), x + r * (1.0 - k), y, x + r, y);
        self.set(x + r, y);
        self.ctx.close_path();
    }

    fn close(&mut self) {
        self.ctx.close_path();
    }
}

/// 圆上一点（角度制，画布坐标 y 向下）。
fn polar(cx: f64, cy: f64, r: f64, angle: f64) -> CGPoint {
    let a = deg(angle);
    CGPoint::new(cx + r * a.cos(), cy + r * a.sin())
}

/// 角度增大方向的单位切量。
fn tangent(angle: f64) -> (f64, f64) {
    let a = deg(angle);
    (-a.sin(), a.cos())
}

/// 单独描一段弧。
fn stroke_arc(ctx: &CGContext, cx: f64, cy: f64, r: f64, from: f64, to: f64) {
    let mut p = Path::new(ctx);
    p.arc(cx, cy, r, from, to);
    ctx.stroke_path();
}

/// 正版 `applyWiFiScale`：绕中心符号水准点把整套图形放大，线宽跟着 CTM 一起
/// 放大。调用方负责 save/restore。
fn apply_center_scale(ctx: &CGContext) {
    if CENTER_SCALE == 1.0 {
        return;
    }
    ctx.translate(SYMBOL_CX, SYMBOL_CY);
    ctx.scale(CENTER_SCALE, CENTER_SCALE);
    ctx.translate(-SYMBOL_CX, -SYMBOL_CY);
}

fn set_ink(ctx: &CGContext, palette: &Palette, rgb: [u8; 3], alpha: f64) {
    let rgb = palette.resolve(rgb);
    ctx.set_rgb_stroke_color(
        rgb[0] as f64 / 255.0,
        rgb[1] as f64 / 255.0,
        rgb[2] as f64 / 255.0,
        alpha,
    );
    ctx.set_rgb_fill_color(
        rgb[0] as f64 / 255.0,
        rgb[1] as f64 / 255.0,
        rgb[2] as f64 / 255.0,
        alpha,
    );
}

pub fn paint(
    spec: &GlyphSpec,
    palette: &Palette,
    phase: Option<ChargingPhase>,
    ime: Option<ImeFlash>,
) -> Vec<u8> {
    let space = CGColorSpace::create_device_rgb();
    let mut ctx = CGContext::create_bitmap_context(
        None,
        BUF,
        BUF,
        8,
        BUF * 4,
        &space,
        core_graphics::base::kCGImageAlphaPremultipliedLast,
    );
    // 设计空间 y 向下，和正版一样。
    ctx.translate(0.0, BUF as f64);
    ctx.scale(PX, -PX);
    ctx.set_line_cap(CGLineCap::CGLineCapRound);
    ctx.set_line_join(CGLineJoin::CGLineJoinRound);
    // 字形本身 y 向上，画布被翻成 y 向下，不预先翻回文字矩阵整串数字会转 180°。
    ctx.set_text_matrix(&CGAffineTransform::new(1.0, 0.0, 0.0, -1.0, 0.0, 0.0));

    // 输入法切换瞬间：整格换成字牌，电池/网络/音量一律让位。
    if let Some(flash) = ime {
        draw_ime_chip(&ctx, palette, flash);
        return readback(&mut ctx);
    }

    let gap_width = spec.top_gap_width();

    // --- 外环：电池 ---
    let effect = spec.battery.and_then(|percent| {
        let effect_phase = phase.filter(|_| spec.charging)?;
        let progress = (percent.min(100) as f64) / 100.0;
        charging_frame(progress, effect_phase, gap_width)
    });
    if let Some(percent) = spec.battery {
        let progress = (percent.min(100) as f64) / 100.0;
        ctx.set_line_width(RING_LINE);
        set_ink(&ctx, palette, palette.ink, INACTIVE_ALPHA);
        for (a, b) in battery_arc_segments(0.0, 1.0, gap_width) {
            stroke_arc(&ctx, RING_CX, RING_CY, RING_R, a, b);
        }
        let color = palette.battery_rgb(percent, spec.low_power, spec.charging || spec.plugged);
        set_ink(&ctx, palette, color, 1.0);
        for (a, b) in battery_arc_segments(0.0, progress, gap_width) {
            stroke_arc(&ctx, RING_CX, RING_CY, RING_R, a, b);
        }
        if let Some(effect) = effect {
            draw_charging_effect(&ctx, palette, color, effect, gap_width);
        }
    }

    // --- 缺口内容：数字 / 闪电 / 插头 ---
    match spec.gap() {
        Gap::Value => draw_digits(&ctx, palette, spec.percent_label.unwrap_or(0)),
        Gap::Bolt => draw_bolt(&ctx, palette, spec, effect),
        Gap::Plug => draw_plug(&ctx, palette),
        Gap::Closed => {}
    }

    draw_center(&ctx, palette, spec.center);

    if let Some(volume) = spec.volume {
        draw_footer(&ctx, palette, spec.volume_style, volume);
    }

    readback(&mut ctx)
}

/// 扫光：尾部渐变（裁剪在环形描边的轮廓里）、头部亮点、心跳光晕。

fn readback(ctx: &mut CGContext) -> Vec<u8> {
    let src = ctx.data().to_vec();
    let mut out = vec![0u8; IMAGE_SIZE * IMAGE_SIZE * 4];
    let n = (SUP * SUP) as u32;
    for y in 0..IMAGE_SIZE {
        for x in 0..IMAGE_SIZE {
            let mut acc = [0u32; 4];
            for sy in 0..SUP {
                for sx in 0..SUP {
                    let i = ((y * SUP + sy) * BUF + x * SUP + sx) * 4;
                    for c in 0..4 {
                        acc[c] += src[i + c] as u32;
                    }
                }
            }
            let alpha = acc[3] / n;
            let o = (y * IMAGE_SIZE + x) * 4;
            out[o + 3] = alpha as u8;
            for c in 0..3 {
                out[o + c] = if alpha > 0 {
                    ((acc[c] / n) * 255 / alpha).min(255) as u8
                } else {
                    0
                };
            }
        }
    }
    out
}

mod battery;
mod ime;
mod symbols;

pub(super) use battery::fill_dot;
use battery::{draw_bolt, draw_charging_effect, draw_digits, draw_plug};
use ime::{draw_ime_chip, draw_ime_text};
use symbols::{draw_center, draw_footer};
