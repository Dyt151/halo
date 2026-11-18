//! 中心网络符号与底部音量区。

use super::super::*;
use super::*;

pub(super) fn draw_center(ctx: &CGContext, palette: &Palette, center: CenterGlyph) {
    ctx.set_line_width(SYMBOL_LINE);
    match center {
        CenterGlyph::Wifi {
            bars,
            powered,
            connected,
        } => draw_wifi(ctx, palette, bars, powered, connected),
        CenterGlyph::Hotspot { powered, connected } => {
            draw_hotspot(ctx, palette, powered, connected)
        }
        CenterGlyph::Ethernet { connected } => draw_ethernet(ctx, palette, connected),
        CenterGlyph::Temporary { connected } => draw_temporary(ctx, palette, connected),
        CenterGlyph::Shared { connected } => draw_shared(ctx, palette, connected),
        CenterGlyph::Bluetooth { powered } => draw_bluetooth(ctx, palette, powered),
        CenterGlyph::Clipboard => draw_clipboard(ctx, palette),
    }
}

/// Wi-Fi：外弧 + 中弧 + 点。3 格点亮两弧，2 格只点亮中弧，1 格只剩点。
///
/// 弧与点直接按 `WIFI_SYMBOL_*` 的实测几何画在最终位置（正版那张 SF Symbol
/// 本身就是 1.5 倍的），只有斜杠和感叹号还要再套一次倍率。
fn draw_wifi(ctx: &CGContext, palette: &Palette, bars: u8, powered: bool, connected: bool) {
    let solid = powered && connected;
    // 点是 Wi-Fi 的身份，永远画出来；不亮时整枚转淡。
    set_ink(
        ctx,
        palette,
        palette.ink,
        if solid { 1.0 } else { INACTIVE_ALPHA },
    );
    let (dx, dy, rx, ry) = WIFI_SYMBOL_DOT;
    ctx.fill_ellipse_in_rect(CGRect::new(
        &CGPoint::new(dx - rx, dy - ry),
        &CGSize::new(rx * 2.0, ry * 2.0),
    ));

    let arcs = [
        (
            WIFI_SYMBOL_OUTER_CY,
            WIFI_SYMBOL_OUTER_R,
            WIFI_SYMBOL_OUTER_SWEEP,
            WIFI_SYMBOL_OUTER_LINE,
            3,
        ),
        (
            WIFI_SYMBOL_MIDDLE_CY,
            WIFI_SYMBOL_MIDDLE_R,
            WIFI_SYMBOL_MIDDLE_SWEEP,
            WIFI_SYMBOL_MIDDLE_LINE,
            2,
        ),
    ];
    for (cy, r, sweep, line, need) in arcs {
        let lit = solid && bars >= need;
        set_ink(
            ctx,
            palette,
            palette.ink,
            if lit { 1.0 } else { INACTIVE_ALPHA },
        );
        ctx.set_line_width(line);
        stroke_arc(ctx, SYMBOL_CX, cy, r, 270.0 - sweep, 270.0 + sweep);
    }
    ctx.set_line_width(SYMBOL_LINE);

    if !powered {
        // 关掉：一条斜杠压在整组弧上（正版整枚换成 SF Symbol `wifi.slash`，
        // 斜杠跟着符号一起放大）。
        ctx.save();
        apply_center_scale(ctx);
        set_ink(ctx, palette, palette.ink, 1.0);
        let mut p = Path::new(ctx);
        p.move_to(WIFI_OFF_SLASH[0].0, WIFI_OFF_SLASH[0].1);
        p.line_to(WIFI_OFF_SLASH[1].0, WIFI_OFF_SLASH[1].1);
        ctx.stroke_path();
        ctx.restore();
    } else if !connected {
        // 连着但没网：一竖加一颗点（正版 `wifi.exclamationmark`）。
        ctx.save();
        apply_center_scale(ctx);
        set_ink(ctx, palette, palette.ink, 1.0);
        let mut p = Path::new(ctx);
        p.move_to(NO_INTERNET_STEM[0].0, NO_INTERNET_STEM[0].1);
        p.line_to(NO_INTERNET_STEM[1].0, NO_INTERNET_STEM[1].1);
        ctx.stroke_path();
        ctx.set_line_width(2.0);
        let (x, y, d) = NO_INTERNET_DOT;
        ctx.fill_ellipse_in_rect(CGRect::new(&CGPoint::new(x, y), &CGSize::new(d, d)));
        ctx.restore();
    }
}

/// 一个热点链环的轮廓：两端是半圆的圆角环。
fn stroke_link(ctx: &CGContext, center: (f64, f64)) {
    let (w, h) = HOTSPOT_LINK;
    let mut p = Path::new(ctx);
    p.rounded_rect(center.0 - w / 2.0, center.1 - h / 2.0, w, h, h / 2.0);
    ctx.stroke_path();
}

/// 个人热点：正版直接画 SF Symbol `personalhotspot`（pointSize 38×1.5，
/// 版面框 55×39、倍率 1.0 时实测墨迹 49×32.75）。这里用两个互扣的圆角环复现
/// 同一轮廓，尺寸按实测墨迹对齐到水准点 (59.5, 64)，再套上中心符号倍率；
/// 下层环被压住的一侧抠一条缝才读得出互扣。
fn draw_hotspot(ctx: &CGContext, palette: &Palette, powered: bool, connected: bool) {
    ctx.save();
    apply_center_scale(ctx);
    set_ink(
        ctx,
        palette,
        palette.ink,
        if powered && connected {
            1.0
        } else {
            INACTIVE_ALPHA
        },
    );
    ctx.set_line_width(HOTSPOT_LINE);
    stroke_link(ctx, HOTSPOT_BACK);
    ctx.save();
    ctx.set_blend_mode(CGBlendMode::Clear);
    ctx.set_line_width(HOTSPOT_LINE + 2.0 * HOTSPOT_GAP);
    stroke_link(ctx, HOTSPOT_FRONT);
    ctx.restore();
    ctx.set_line_width(HOTSPOT_LINE);
    stroke_link(ctx, HOTSPOT_FRONT);

    if !powered {
        set_ink(ctx, palette, palette.ink, 1.0);
        let mut p = Path::new(ctx);
        p.move_to(WIFI_OFF_SLASH[0].0, WIFI_OFF_SLASH[0].1);
        p.line_to(WIFI_OFF_SLASH[1].0, WIFI_OFF_SLASH[1].1);
        ctx.stroke_path();
    }
    ctx.restore();
    ctx.set_line_width(SYMBOL_LINE);
}

/// 有线：双折线 + 三个触点（正版把 SF Symbol 的 87.736×46.376 画框缩进
/// 56×29.6 的目标框，这里把换算结果写死）。
fn draw_ethernet(ctx: &CGContext, palette: &Palette, connected: bool) {
    set_ink(
        ctx,
        palette,
        palette.ink,
        if connected { 1.0 } else { INACTIVE_ALPHA },
    );
    ctx.set_line_width(4.886_659_979_939_819);
    for chevron in [
        [(46.28, 53.64), (33.94, 65.98), (46.28, 78.36)],
        [(72.72, 53.64), (85.06, 65.98), (72.72, 78.36)],
    ] {
        let mut p = Path::new(ctx);
        p.move_to(chevron[0].0, chevron[0].1);
        p.line_to(chevron[1].0, chevron[1].1);
        p.line_to(chevron[2].0, chevron[2].1);
        ctx.stroke_path();
    }
    let r = 2.443_329_989_969_909_5;
    for x in [49.73, 59.49, 69.27] {
        ctx.fill_ellipse_in_rect(CGRect::new(
            &CGPoint::new(x - r, 65.98 - r),
            &CGSize::new(r * 2.0, r * 2.0),
        ));
    }
    ctx.set_line_width(SYMBOL_LINE);
}

/// 临时网络：Wi-Fi 外弧收成实心扇形，再抠掉一台小显示器。正版整组图形套
/// `applyWiFiScale`，所以这里也一样绕水准点放大。
fn draw_temporary(ctx: &CGContext, palette: &Palette, connected: bool) {
    ctx.save();
    apply_center_scale(ctx);
    draw_wedge(ctx, palette, connected);
    ctx.set_blend_mode(CGBlendMode::Clear);
    ctx.set_line_width(2.5);
    let mut outline = Path::new(ctx);
    outline.rounded_rect(50.5, 53.5, 18.0, 12.0, 2.5);
    ctx.stroke_path();
    let mut stand = Path::new(ctx);
    stand.move_to(57.5, 65.5);
    stand.line_to(61.5, 65.5);
    stand.line_to(61.5, 67.5);
    stand.line_to(63.0, 67.5);
    stand.line_to(63.0, 70.5);
    stand.line_to(56.0, 70.5);
    stand.line_to(56.0, 67.5);
    stand.line_to(57.5, 67.5);
    stand.close();
    ctx.fill_path();
    ctx.restore();
}

/// 互联网共享：同样的扇形，抠掉一支上箭头。
fn draw_shared(ctx: &CGContext, palette: &Palette, connected: bool) {
    ctx.save();
    apply_center_scale(ctx);
    draw_wedge(ctx, palette, connected);
    ctx.set_blend_mode(CGBlendMode::Clear);
    let mut arrow = Path::new(ctx);
    arrow.move_to(AX, 51.5);
    arrow.line_to(67.5, 59.5);
    arrow.line_to(63.0, 59.5);
    arrow.line_to(63.0, 72.5);
    arrow.line_to(56.0, 72.5);
    arrow.line_to(56.0, 59.5);
    arrow.line_to(51.5, 59.5);
    arrow.close();
    ctx.fill_path();
    ctx.restore();
}

/// 扇形本体：外弧 → 拉到圆心 → 闭合，描边 + 填充一起走。
fn draw_wedge(ctx: &CGContext, palette: &Palette, connected: bool) {
    set_ink(
        ctx,
        palette,
        palette.ink,
        if connected { 1.0 } else { INACTIVE_ALPHA },
    );
    ctx.set_line_width(SYMBOL_LINE);
    let mut p = Path::new(ctx);
    p.arc(
        AX,
        WIFI_OUTER_CY,
        WIFI_OUTER_R,
        WIFI_OUTER_START,
        WIFI_OUTER_END,
    );
    p.line_to(AX, 77.45);
    p.close();
    ctx.draw_path(CGPathDrawingMode::CGPathFillStroke);
}

/// 蓝牙（Halo 自己的字形，按 120 单位设计空间重画）。
fn draw_bluetooth(ctx: &CGContext, palette: &Palette, powered: bool) {
    /// 正版在 24 单位方格里画的蓝色符号线，这里等比放进 120 空间。
    const RUNE: [(f64, f64); 6] = [
        (6.72, 17.03),
        (17.28, 6.47),
        (12.0, 1.5),
        (12.0, 22.5),
        (17.28, 17.53),
        (6.72, 6.97),
    ];
    const RUNE_SCALE: f64 = 2.0;
    let map = |p: (f64, f64)| -> (f64, f64) {
        (
            SYMBOL_CX + (p.0 - 12.0) * RUNE_SCALE,
            SYMBOL_CY + (p.1 - 12.0) * RUNE_SCALE,
        )
    };
    set_ink(
        ctx,
        palette,
        palette.ink,
        if powered { 1.0 } else { INACTIVE_ALPHA },
    );
    ctx.set_line_width(6.0);
    let mut p = Path::new(ctx);
    let first = map(RUNE[0]);
    p.move_to(first.0, first.1);
    for pt in &RUNE[1..] {
        let q = map(*pt);
        p.line_to(q.0, q.1);
    }
    ctx.stroke_path();
    if !powered {
        set_ink(ctx, palette, palette.ink, 1.0);
        let mut s = Path::new(ctx);
        s.move_to(WIFI_OFF_SLASH[0].0, WIFI_OFF_SLASH[0].1);
        s.line_to(WIFI_OFF_SLASH[1].0, WIFI_OFF_SLASH[1].1);
        ctx.stroke_path();
    }
}

/// 剪贴板（Halo 自身标识）。
fn draw_clipboard(ctx: &CGContext, palette: &Palette) {
    set_ink(ctx, palette, palette.ink, 1.0);
    ctx.set_line_width(3.6);
    let mut board = Path::new(ctx);
    board.rounded_rect(47.5, 47.0, 24.0, 34.0, 2.0);
    ctx.stroke_path();
    // 夹子骑在顶边上，剪贴板才不会像个空电池。
    let mut clip = Path::new(ctx);
    clip.rounded_rect(52.0, 43.5, 15.0, 7.0, 2.0);
    ctx.fill_path();
    ctx.set_line_width(3.0);
    for y in [62.0, 71.0] {
        let mut row = Path::new(ctx);
        row.move_to(53.0, y);
        row.line_to(66.0, y);
        ctx.stroke_path();
    }
}

pub(super) fn draw_footer(ctx: &CGContext, palette: &Palette, style: VolumeStyle, volume: f64) {
    match style {
        VolumeStyle::Dots => {
            ctx.set_line_width(SYMBOL_LINE);
            let lit = volume_steps(volume, volume <= 0.0);
            for (i, (dx, dy)) in VOLUME_DOTS.iter().enumerate() {
                set_ink(
                    ctx,
                    palette,
                    palette.ink,
                    if (i as u8) < lit { 1.0 } else { INACTIVE_ALPHA },
                );
                ctx.fill_ellipse_in_rect(CGRect::new(
                    &CGPoint::new(dx - VOLUME_DOT_R, dy - VOLUME_DOT_R),
                    &CGSize::new(VOLUME_DOT_R * 2.0, VOLUME_DOT_R * 2.0),
                ));
            }
        }
        VolumeStyle::Arc => {
            ctx.set_line_width(FOOT_LINE);
            set_ink(ctx, palette, palette.ink, INACTIVE_ALPHA);
            stroke_arc(
                ctx,
                RING_CX,
                RING_CY,
                RING_R,
                VOLUME_ARC_START,
                VOLUME_ARC_END,
            );
            let progress = volume.clamp(0.0, 1.0);
            if progress > 0.0 {
                set_ink(ctx, palette, palette.ink, 1.0);
                let end = VOLUME_ARC_START - (VOLUME_ARC_START - VOLUME_ARC_END) * progress;
                stroke_arc(ctx, RING_CX, RING_CY, RING_R, VOLUME_ARC_START, end);
            }
        }
    }
}

