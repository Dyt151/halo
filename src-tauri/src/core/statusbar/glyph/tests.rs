//! 菜单栏图标的几何、配色与光栅化回归测试。

use super::*;
use super::charging::gap_fraction;
use super::palette::{contrast_ratio, relative_luminance};

fn spec(battery: Option<u8>, center: CenterGlyph, volume: Option<f64>) -> GlyphSpec {
    GlyphSpec {
        battery,
        charging: false,
        plugged: false,
        low_power: false,
        percent_label: battery,
        center,
        volume,
        volume_style: VolumeStyle::Arc,
    }
}

fn wifi(bars: u8, powered: bool, connected: bool) -> CenterGlyph {
    CenterGlyph::Wifi {
        bars,
        powered,
        connected,
    }
}

#[test]
fn rssi_maps_to_three_bars() {
    assert_eq!(wifi_bars(Some(-52)), 3);
    assert_eq!(wifi_bars(Some(-70)), 3);
    assert_eq!(wifi_bars(Some(-71)), 2);
    assert_eq!(wifi_bars(Some(-78)), 2);
    assert_eq!(wifi_bars(Some(-79)), 1);
    assert_eq!(wifi_bars(Some(-95)), 1);
    assert_eq!(wifi_bars(None), 0);
}

#[test]
fn volume_maps_to_four_steps() {
    assert_eq!(volume_steps(0.0, false), 0);
    assert_eq!(volume_steps(0.1, true), 0);
    assert_eq!(volume_steps(0.25, false), 1);
    assert_eq!(volume_steps(0.26, false), 2);
    assert_eq!(volume_steps(0.75, false), 3);
    assert_eq!(volume_steps(1.0, false), 4);
}

#[test]
fn ime_marker_picks_filled_chip_for_input_methods_and_outline_for_latin() {
    assert!(ImeFlash::new('拼', 1.0, 1.0).filled);
    assert!(ImeFlash::new('あ', 1.0, 1.0).filled);
    assert!(ImeFlash::new('한', 1.0, 1.0).filled);
    assert!(!ImeFlash::new('A', 1.0, 1.0).filled);
    assert!(!ImeFlash::new('Z', 1.0, 1.0).filled);
}

#[cfg(target_os = "macos")]
#[test]
fn ime_chip_replaces_the_composite_and_knocks_the_character_out() {
    let palette = Palette::colored(true);
    let sample = spec(Some(81), wifi(3, true, true), Some(0.5));
    let base = render(&sample, &palette);
    let filled = render_with_ime(&sample, &palette, None, Some(ImeFlash::new('拼', 1.0, 1.0)));
    let outlined = render_with_ime(&sample, &palette, None, Some(ImeFlash::new('A', 1.0, 1.0)));
    assert_ne!(filled, base);
    assert_ne!(outlined, base);
    assert_ne!(filled, outlined);

    let alpha = |img: &[u8], x: usize, y: usize| img[(y * IMAGE_SIZE + x) * 4 + 3];
    if let Ok(dir) = std::env::var("HALO_GLYPH_DUMP") {
        let _ = std::fs::write(format!("{dir}/ime_filled_52.rgba"), &filled);
        let _ = std::fs::write(format!("{dir}/ime_outline_52.rgba"), &outlined);
        let _ = std::fs::write(format!("{dir}/composite_52.rgba"), &base);
    }
    // 字牌只占中间 22×16pt：四角必须是空的。
    assert_eq!(alpha(&filled, 0, 0), 0);
    assert_eq!(alpha(&filled, IMAGE_SIZE - 1, IMAGE_SIZE - 1), 0);
    // 字牌内部有墨（字顶上方那条实心带）。
    assert!(alpha(&filled, IMAGE_SIZE / 2, 12) > 240);
    // 文字是挖空的：字牌中心区域必须有透明像素。
    let holes = (18..34)
        .flat_map(|y| (16..36).map(move |x| (x, y)))
        .filter(|&(x, y)| alpha(&filled, x, y) == 0)
        .count();
    assert!(holes > 20, "拼 应从字牌里挖掉，透明像素只有 {holes} 个");
}

#[test]
fn center_mode_and_style_fall_back_to_defaults() {
    assert_eq!(CenterMode::parse("wifi"), CenterMode::Wifi);
    assert_eq!(CenterMode::parse("bluetooth"), CenterMode::Bluetooth);
    assert_eq!(CenterMode::parse("clipboard"), CenterMode::Clipboard);
    assert_eq!(CenterMode::parse("nonsense"), CenterMode::Auto);
    assert_eq!(Appearance::parse("dark"), Appearance::Dark);
    assert_eq!(Appearance::parse("light"), Appearance::Light);
    assert_eq!(Appearance::parse("system"), Appearance::Auto);
    assert_eq!(VolumeStyle::parse("dots"), VolumeStyle::Dots);
    assert_eq!(VolumeStyle::parse(""), VolumeStyle::Arc);
}

#[test]
fn charging_hides_the_digits_and_only_keeps_the_bolt() {
    let charging = GlyphSpec {
        charging: true,
        ..spec(Some(42), CenterGlyph::Clipboard, None)
    };
    assert_eq!(charging.gap(), Gap::Bolt);
    assert_eq!(charging.top_gap_width(), GAP_INDICATOR);
    let discharging = GlyphSpec {
        battery: Some(42),
        percent_label: Some(42),
        ..GlyphSpec::plain()
    };
    assert_eq!(discharging.gap(), Gap::Value);
    assert_eq!(discharging.top_gap_width(), GAP_VALUE);
}

#[test]
fn plugged_but_not_charging_shows_the_plug_not_the_digits() {
    let plugged = GlyphSpec {
        plugged: true,
        ..spec(Some(90), CenterGlyph::Clipboard, None)
    };
    // 正版 connectedIndicator 默认是插头：缺口收窄到 50，数字让位。
    assert_eq!(plugged.gap(), Gap::Plug);
    assert_eq!(plugged.top_gap_width(), GAP_INDICATOR);
    // 充电优先于插头。
    let both = GlyphSpec {
        charging: true,
        ..plugged
    };
    assert_eq!(both.gap(), Gap::Bolt);
    // 没有环就不谈缺口。
    let no_ring = GlyphSpec {
        battery: None,
        ..plugged
    };
    assert_eq!(no_ring.gap(), Gap::Closed);
}

#[test]
fn charging_phase_wraps_at_the_cycle_boundary() {
    let steady = charging_phase(0.0, ChargingKind::Steady, 1.0).expect("steady");
    assert_eq!((steady.step, steady.steps_per_cycle), (0, 36));
    // 1.8s 整是下一圈的第一帧。
    assert_eq!(
        charging_phase(STEADY_CYCLE, ChargingKind::Steady, 1.0)
            .unwrap()
            .step,
        0
    );
    assert_eq!(
        charging_phase(0.05 * 35.0, ChargingKind::Steady, 1.0)
            .unwrap()
            .step,
        35
    );
    let burst = charging_phase(0.0, ChargingKind::Burst, 1.0).unwrap();
    assert_eq!(burst.steps_per_cycle, 12);
    assert!(charging_phase(-1.0, ChargingKind::Steady, 1.0).is_none());
    assert!(charging_phase(f64::NAN, ChargingKind::Steady, 1.0).is_none());
    assert!(charging_phase(0.0, ChargingKind::Steady, -1.0).is_none());
}

#[test]
fn charging_sweep_advances_and_stops_at_the_gap_edge() {
    let phase = |step| ChargingPhase {
        step,
        steps_per_cycle: 36,
        kind: ChargingKind::Steady,
        heartbeat_multiplier: 1.0,
    };
    let start = charging_frame(0.6, phase(0), GAP_INDICATOR).unwrap();
    assert_eq!(start.head, 0.0);
    assert!(start.tail.is_none());

    let mid = charging_frame(0.6, phase(15), GAP_INDICATOR).unwrap();
    assert!(mid.head > 0.0 && mid.head < 0.6);
    assert!(mid.tail.is_some(), "扫过之后应该拖出尾焰");

    // 60% 的填充正好停在缺口里：头部只能走到缺口左沿。
    let endpoint = last_visible_progress(0.6, GAP_INDICATOR);
    let last = charging_frame(0.6, phase(31), GAP_INDICATOR).unwrap();
    assert!((last.head - endpoint).abs() < 1e-9);
    assert!(last.head_visible);

    // 心跳帧：光晕变亮、外扩。峰值那一帧（33）光晕最亮但半径回落到 1，
    // 前后两帧（32/34）才是外扩最大的时刻。
    let beat = charging_frame(0.6, phase(33), GAP_INDICATOR).unwrap();
    assert!(beat.heartbeat_alpha > 0.9);
    assert_eq!(beat.heartbeat_scale, 1.0);
    assert!(beat.bead_alpha < 1.0);
    let expanding = charging_frame(0.6, phase(32), GAP_INDICATOR).unwrap();
    assert!(expanding.heartbeat_scale > 1.0);
    assert!(expanding.heartbeat_alpha > 0.0);
}

#[test]
fn charging_bolt_pulses_while_the_head_crosses_the_top_gap() {
    let phase = |step| ChargingPhase {
        step,
        steps_per_cycle: 36,
        kind: ChargingKind::Steady,
        heartbeat_multiplier: 1.0,
    };
    let inside = charging_frame(0.9, phase(20), GAP_INDICATOR).unwrap();
    let gap_start = last_visible_progress(0.5, GAP_INDICATOR);
    assert!(inside.head > gap_start && inside.head < 1.0 - gap_start);
    assert!(inside.bolt_scale > 1.0);
    assert!(inside.bolt_arc_color_amount > 0.0);

    // 头部离开缺口后闪电恢复原大小、原色。
    let outside = charging_frame(0.9, phase(5), GAP_INDICATOR).unwrap();
    assert_eq!(outside.bolt_scale, 1.0);
    assert_eq!(outside.bolt_arc_color_amount, 0.0);
}

#[test]
fn visible_fraction_round_trips_outside_the_gap() {
    for progress in [0.0, 0.2, 0.35, 0.7, 0.9, 1.0] {
        let fraction = visible_fraction(progress, GAP_INDICATOR);
        let back = progress_for_visible_fraction(fraction, GAP_INDICATOR);
        assert!((back - progress).abs() < 1e-9, "{progress} -> {back}");
    }
    // 缺口里的进度统一映射到缺口左沿。
    let gap_start = (1.0 - gap_fraction(GAP_INDICATOR)) / 2.0;
    assert!((visible_fraction(0.5, GAP_INDICATOR) - gap_start).abs() < 1e-9);
    assert!((last_visible_progress(0.5, GAP_INDICATOR) - gap_start).abs() < 1e-9);
    // 没有缺口就原样返回。
    assert_eq!(visible_fraction(0.42, 0.0), 0.42);
    assert_eq!(progress_for_visible_fraction(0.42, 0.0), 0.42);
}

#[test]
fn automatic_highlight_clears_the_contrast_floor() {
    for base in [[255, 214, 10], [48, 209, 88], [255, 69, 58], [120, 120, 120]] {
        let highlight = automatic_highlight(base);
        assert!(
            contrast_ratio(base, highlight) >= 1.75,
            "{base:?} -> {highlight:?} 对比度不足"
        );
    }
    // 充电绿对白底已经有 2:1，扫光色会往白提亮，闪电直接用这个颜色。
    let green = [48, 209, 88];
    let highlight = automatic_highlight(green);
    assert!(relative_luminance(highlight) > relative_luminance(green));
    assert_eq!(charging_bolt_highlight(green, highlight), highlight);

    // 黄色提不亮，只能压暗；压暗后的扫光色比底色暗，闪电退回「混 35% 白」。
    let yellow = [255, 214, 10];
    let dark = automatic_highlight(yellow);
    assert!(relative_luminance(dark) < relative_luminance(yellow));
    assert_eq!(
        charging_bolt_highlight(yellow, dark),
        blend_rgb(yellow, [255, 255, 255], 0.35)
    );
}

#[test]
fn no_label_closes_the_ring() {
    let no_digits = GlyphSpec {
        percent_label: None,
        ..spec(Some(42), CenterGlyph::Clipboard, None)
    };
    assert_eq!(no_digits.gap(), Gap::Closed);
    assert_eq!(no_digits.top_gap_width(), 0.0);
    // 没有环也没有缺口可言。
    let no_ring = GlyphSpec {
        charging: true,
        ..spec(None, CenterGlyph::Clipboard, None)
    };
    assert_eq!(no_ring.gap(), Gap::Closed);
}

#[test]
fn gap_splits_the_arc_into_two_visible_pieces() {
    // 进度落在缺口里：只留下缺口前那段。
    let gap = gap_fraction(GAP_VALUE);
    let gap_start = (1.0 - gap) / 2.0;
    let inside = battery_arc_segments(0.0, gap_start + gap * 0.5, GAP_VALUE);
    assert_eq!(inside.len(), 1);
    assert!(inside[0].1 < RING_START + RING_SWEEP * (0.5 + 0.01));
    // 进度越过缺口：切成两段，中间那段被跳过。
    let across = battery_arc_segments(0.0, 0.9, GAP_VALUE);
    assert_eq!(across.len(), 2);
    assert!(across[0].1 < across[1].0);
    // 不留缺口就是一段到底。
    assert_eq!(battery_arc_segments(0.0, 1.0, 0.0).len(), 1);
}

#[test]
fn gap_fraction_matches_the_shipped_dasharray() {
    // 正版 status-menubar.svg 的 dasharray 35.3/29.4/35.3/4 里，缺口占 0.294。
    assert!((gap_fraction(GAP_VALUE) - 0.294).abs() < 0.01);
    assert!(gap_fraction(GAP_INDICATOR) < gap_fraction(GAP_VALUE));
    assert_eq!(gap_fraction(0.0), 0.0);
}

#[test]
fn low_battery_only_changes_the_color() {
    let palette = Palette::colored(true);
    assert_eq!(palette.battery_rgb(19, false, false), palette.low);
    // 正版的临界判定是「严格小于 20」，20 本身还不算红。
    assert_eq!(palette.battery_rgb(20, false, false), palette.ink);
    assert_eq!(palette.battery_rgb(90, false, true), palette.charging);
    // 常态弧用前景墨色：整枚图标只有一笔颜色。
    assert_eq!(palette.battery_rgb(90, false, false), palette.ink);
    // 临界红优先于充电：插着电的 10% 在正版也是红的。
    assert_eq!(palette.battery_rgb(10, false, true), palette.low);
}

#[test]
fn low_power_mode_wins_over_the_powered_green() {
    let palette = Palette::colored(true);
    // 正版 batteryColorRole：低电量模式那一档排在充电/接电源之前。
    assert_eq!(palette.battery_rgb(90, true, false), palette.low_power);
    assert_eq!(palette.battery_rgb(90, true, true), palette.low_power);
    // 但临界红仍然压住黄色。
    assert_eq!(palette.battery_rgb(10, true, true), palette.low);
    // 黄色只是弧换色，缺口里该写数字还写数字。
    let low_power = GlyphSpec {
        low_power: true,
        ..spec(Some(90), CenterGlyph::Clipboard, None)
    };
    assert_eq!(low_power.gap(), Gap::Value);
    assert_eq!(low_power.top_gap_width(), GAP_VALUE);
}

#[test]
fn appearance_override_beats_the_system_reading() {
    let with = |colored: bool, appearance: Appearance| GlyphOptions {
        colored,
        appearance,
        ..GlyphOptions::default()
    };
    for menu_dark in [Some(true), Some(false), None] {
        let palette = Palette::for_options(&with(true, Appearance::Dark), menu_dark);
        assert_eq!(palette.ink, [255, 255, 255]);
        assert_eq!(
            Palette::for_options(&with(true, Appearance::Light), menu_dark).ink,
            [24, 24, 26]
        );
    }
    assert_eq!(
        Palette::for_options(&with(true, Appearance::Auto), Some(true)).ink,
        [255, 255, 255]
    );
    assert_eq!(
        Palette::for_options(&with(true, Appearance::Auto), Some(false)).ink,
        [24, 24, 26]
    );
    assert_eq!(
        Palette::for_options(&with(true, Appearance::Auto), None).ink,
        [24, 24, 26]
    );
    let template = Palette::for_options(&with(false, Appearance::Dark), Some(true));
    assert!(!template.colored);
    assert_eq!(template.resolve([255, 214, 10]), [0, 0, 0]);
}

#[test]
fn volume_style_defaults_to_the_arc() {
    let options = GlyphOptions::default();
    assert_eq!(options.volume_style, VolumeStyle::Arc);
    // 四个链路各自的开关默认全关：专用标记才是正版的默认长相。
    assert!(!options.wifi_for_ethernet);
    assert!(!options.wifi_for_hotspot);
    assert!(!options.wifi_for_temporary);
    assert!(!options.wifi_for_sharing);
}

#[cfg(target_os = "macos")]
mod raster {
    use super::super::*;
    use super::{spec as sample, wifi};

    fn alpha_at(img: &[u8], x: usize, y: usize) -> u8 {
        img[(y * IMAGE_SIZE + x) * 4 + 3]
    }

    fn ink(img: &[u8]) -> u32 {
        img.chunks(4).map(|px| px[3] as u32).sum()
    }

    #[test]
    fn template_output_is_black_with_alpha() {
        let img = render(
            &sample(Some(70), CenterGlyph::Clipboard, Some(0.6)),
            &Palette::template(),
        );
        assert_eq!(img.len(), IMAGE_SIZE * IMAGE_SIZE * 4);
        assert!(img
            .chunks(4)
            .all(|px| px[0] == 0 && px[1] == 0 && px[2] == 0));
        assert!(img.chunks(4).any(|px| px[3] > 200));
        assert!(img.chunks(4).any(|px| px[3] == 0));
    }

    #[test]
    fn colored_output_only_tints_the_battery_arc() {
        let palette = Palette::colored(true);
        let charging = GlyphSpec {
            charging: true,
            ..sample(Some(70), CenterGlyph::Clipboard, Some(0.6))
        };
        let img = render(&charging, &palette);
        // 充电时弧是绿的、中心图形与音量是墨色：一张图里两种颜色。
        assert!(img.chunks(4).any(|px| px[3] > 200 && px[..3] == palette.charging));
        assert!(img
            .chunks(4)
            .any(|px| px[3] > 200 && px[..3] == [255, 255, 255]));
        // 不充电时整幅只剩墨色一色：正版常态弧用的就是前景色。
        let plain = render(
            &sample(Some(70), CenterGlyph::Clipboard, Some(0.6)),
            &palette,
        );
        assert!(plain
            .chunks(4)
            .all(|px| px[3] == 0 || px[..3] == [255, 255, 255]));
    }

    #[test]
    fn percent_digits_stay_ink_coloured_while_the_arc_carries_the_color() {
        let palette = Palette::colored(true);
        let img = render(&sample(Some(42), CenterGlyph::Clipboard, None), &palette);
        // 环顶缺口那一带：弧端够不着，只有数字。
        // 52px 图里缺口左沿的环端落在 x≈13px，数字本体从 x≈18px 开始。
        let mut white = 0;
        let mut tinted = 0;
        for y in 1..10 {
            for x in 17..32 {
                let px = &img[(y * IMAGE_SIZE + x) * 4..(y * IMAGE_SIZE + x) * 4 + 4];
                if px[3] < 128 {
                    continue;
                }
                if px[..3] == [255, 255, 255] {
                    white += 1;
                } else if px[0] > 170 && px[2] < 110 {
                    tinted += 1;
                }
            }
        }
        assert!(white > 20, "缺口里没有白墨数字，只有 {white} 个像素");
        assert_eq!(tinted, 0, "数字被染成了电池色：状态栏那圈才负责颜色");
    }

    #[test]
    fn plugged_draws_the_connector_not_the_digits_or_bolt() {
        let palette = Palette::colored(true);
        let base = sample(Some(90), CenterGlyph::Clipboard, None);
        let plugged = render(
            &GlyphSpec {
                plugged: true,
                ..base
            },
            &palette,
        );
        // 正版：接电源未充电时弧已经是绿的了，缺口里却是墨色插头。
        assert!(plugged
            .chunks(4)
            .any(|px| px[3] > 200 && px[..3] == palette.charging));
        assert!(plugged
            .chunks(4)
            .any(|px| px[3] > 200 && px[..3] == [255, 255, 255]));
        // 缺口那一带要落到东西，且既不是数字也不是闪电。
        let in_gap = |img: &[u8]| {
            (1..11)
                .flat_map(|y| (17..32).map(move |x| (y, x)))
                .filter(|&(y, x)| img[(y * IMAGE_SIZE + x) * 4 + 3] > 128)
                .count()
        };
        assert!(in_gap(&plugged) > 20, "缺口里没画出插头");
        assert_ne!(plugged, render(&base, &palette), "缺口里还写着数字");
        let bolt = render(
            &GlyphSpec {
                charging: true,
                ..base
            },
            &palette,
        );
        assert_ne!(plugged, bolt, "插头和闪电画成了同一个形状");
        // 扫光只属于「正在充电」，插着电不充不该有动画。
        let frame = ChargingPhase {
            step: 5,
            steps_per_cycle: 36,
            kind: ChargingKind::Steady,
            heartbeat_multiplier: 1.0,
        };
        assert_eq!(
            render_frame(
                &GlyphSpec {
                    plugged: true,
                    ..base
                },
                &palette,
                Some(frame)
            ),
            plugged
        );
    }

    #[test]
    fn charging_animation_moves_the_sweep() {
        let palette = Palette::colored(true);
        let spec = GlyphSpec {
            charging: true,
            ..sample(Some(80), CenterGlyph::Clipboard, None)
        };
        let frame = |step| ChargingPhase {
            step,
            steps_per_cycle: 36,
            kind: ChargingKind::Steady,
            heartbeat_multiplier: 1.0,
        };
        let early = render_frame(&spec, &palette, Some(frame(5)));
        let late = render_frame(&spec, &palette, Some(frame(20)));
        let still = render(&spec, &palette);
        assert_ne!(early, late, "扫光位置变了，图必须变");
        assert_ne!(early, still, "动画帧不能和静止图一样");

        // 尾焰用的是「自动提亮/压暗」后的电池色，会在环上留下高亮像素。
        let highlight = automatic_highlight(palette.charging);
        let seen = early.chunks(4).any(|px| {
            px[3] > 200
                && (0..3).all(|c| (px[c] as i32 - highlight[c] as i32).abs() <= 4)
        });
        assert!(seen, "扫光高亮没画出来：{highlight:?}");
    }

    #[test]
    fn every_digit_renders_differently() {
        // 字形解析退回 Helvetica 时最典型的症状是「和别的数字一样」，逐个比对兜住。
        let palette = Palette::template();
        let mut seen: Vec<Vec<u8>> = Vec::new();
        for d in 0..=9u8 {
            let img = render(&sample(Some(d), CenterGlyph::Clipboard, None), &palette);
            assert!(
                !seen.contains(&img),
                "数字 {d} 和前面某个数字画成了同一张图"
            );
            seen.push(img);
        }
    }

    #[test]
    fn charging_replaces_the_digits_with_the_bolt() {
        let palette = Palette::template();
        let digits = sample(Some(78), CenterGlyph::Clipboard, None);
        let charging = GlyphSpec {
            charging: true,
            ..digits
        };
        let a = render(&digits, &palette);
        let b = render(&charging, &palette);
        assert_ne!(a, b);
        // 闪电比两个数字占面小：总墨量必然下降。
        assert!(ink(&b) < ink(&a));
    }

    #[test]
    fn higher_battery_draws_more_ink() {
        let palette = Palette::template();
        let low = render(&sample(Some(15), CenterGlyph::Clipboard, None), &palette);
        let high = render(&sample(Some(90), CenterGlyph::Clipboard, None), &palette);
        assert!(ink(&high) > ink(&low));
    }

    #[test]
    fn every_region_gets_its_own_band() {
        // 正版把 120 单位整幅铺进图标（scale = size / 120，不留内缩），
        // 所以这里只要求三区各占一条横带、互不重叠。
        let img = render(
            &sample(Some(78), wifi(3, true, true), Some(0.6)),
            &Palette::template(),
        );
        let a = |x: usize, y: usize| alpha_at(&img, x, y) as u32;
        let band = |rows: std::ops::Range<usize>| -> u32 {
            let mut sum = 0;
            for y in rows {
                for x in 0..IMAGE_SIZE {
                    sum += a(x, y);
                }
            }
            sum
        };
        assert!(band(0..8) > 400, "环顶缺口里必须有数字");
        assert!(band(9..28) > 400, "中心必须有网络符号");
        assert!(band(28..IMAGE_SIZE) > 400, "底部必须有音量");
        // 四角留白：贴到边缘的只有弧与数字的正中。
        for (x, y) in [
            (0, 0),
            (IMAGE_SIZE - 1, 0),
            (0, IMAGE_SIZE - 1),
            (IMAGE_SIZE - 1, IMAGE_SIZE - 1),
        ] {
            assert_eq!(alpha_at(&img, x, y), 0, "角 ({x},{y}) 不该有墨");
        }
    }

    #[test]
    fn link_types_are_their_own_marks() {
        let palette = Palette::template();
        let hotspot = render(
            &sample(
                None,
                CenterGlyph::Hotspot {
                    powered: true,
                    connected: true,
                },
                None,
            ),
            &palette,
        );
        let ethernet = render(
            &sample(None, CenterGlyph::Ethernet { connected: true }, None),
            &palette,
        );
        let temporary = render(
            &sample(None, CenterGlyph::Temporary { connected: true }, None),
            &palette,
        );
        let shared = render(
            &sample(None, CenterGlyph::Shared { connected: true }, None),
            &palette,
        );
        let wific = render(&sample(None, wifi(3, true, true), None), &palette);
        // 每种链路都要一眼分得开。
        for other in [&hotspot, &ethernet, &temporary, &shared] {
            assert_ne!(other, &wific);
        }
        assert_ne!(temporary, shared);
        // 未连接的热点只剩转淡轮廓，墨量必然更低。
        let idle = render(
            &sample(
                None,
                CenterGlyph::Hotspot {
                    powered: true,
                    connected: false,
                },
                None,
            ),
            &palette,
        );
        assert!(ink(&idle) < ink(&hotspot));
    }

    #[test]
    fn wifi_mark_is_the_measured_symbol_size() {
        // 用户对比出来的第二个 bug：中心符号只有正版的一套兜底矢量那么大
        // （倍率 1.0），正版菜单栏上那枚是 SF Symbol @ 38×1.5。
        let img = render(&sample(None, wifi(3, true, true), None), &Palette::template());
        let box_ = ink_box(&img);
        let (x0, y0, x1, y1) = box_;
        let to_units = |px: f64| px * CANVAS / IMAGE_SIZE as f64;
        let w = to_units(x1 - x0 + 1.0);
        let h = to_units(y1 - y0 + 1.0);
        // 实测符号墨迹 64.67 × 46.92 单位；留出抗锯齿的余量。
        assert!(
            (59.0..70.0).contains(&w) && (42.0..52.0).contains(&h),
            "Wi-Fi 墨迹 {w:.1}×{h:.1} 单位，不在正版符号的 64.7×46.9 附近"
        );
        // 墨迹中心必须坐在水准点上。
        let cx = to_units((x0 + x1 + 1.0) / 2.0);
        let cy = to_units((y0 + y1 + 1.0) / 2.0);
        assert!(
            (cx - SYMBOL_CX).abs() < 1.5 && (cy - SYMBOL_CY).abs() < 1.5,
            "Wi-Fi 墨迹中心 ({cx:.1}, {cy:.1}) 偏离水准点 ({SYMBOL_CX}, {SYMBOL_CY})"
        );
    }

    #[test]
    fn every_center_mark_shares_the_wifi_size_class() {
        // 热点/临时/共享在正版里跟 Wi-Fi 同一个倍率，谁都不许自己缩回去。
        let marks = [
            CenterGlyph::Hotspot {
                powered: true,
                connected: true,
            },
            CenterGlyph::Temporary { connected: true },
            CenterGlyph::Shared { connected: true },
        ];
        for mark in marks {
            let img = render(&sample(None, mark, None), &Palette::template());
            let (x0, y0, x1, y1) = ink_box(&img);
            let w = (x1 - x0 + 1.0) * CANVAS / IMAGE_SIZE as f64;
            let h = (y1 - y0 + 1.0) * CANVAS / IMAGE_SIZE as f64;
            assert!(
                w > 40.0 && h > 28.0,
                "{mark:?} 墨迹只有 {w:.1}×{h:.1} 单位，缩回了倍率 1.0"
            );
        }
    }

    /// 半透明以上像素的包围盒（像素坐标）。
    fn ink_box(img: &[u8]) -> (f64, f64, f64, f64) {
        let (mut x0, mut y0, mut x1, mut y1) = (IMAGE_SIZE, IMAGE_SIZE, 0usize, 0usize);
        for (i, px) in img.chunks(4).enumerate() {
            if px[3] >= 128 {
                let (x, y) = (i % IMAGE_SIZE, i / IMAGE_SIZE);
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
        (x0 as f64, y0 as f64, x1 as f64, y1 as f64)
    }

    #[test]
    fn wifi_state_changes_the_picture() {
        let palette = Palette::template();
        let on = render(&sample(None, wifi(3, true, true), None), &palette);
        let weak = render(&sample(None, wifi(1, true, true), None), &palette);
        let disconnected = render(&sample(None, wifi(0, true, false), None), &palette);
        let off = render(&sample(None, wifi(0, false, false), None), &palette);
        assert_ne!(on, weak);
        assert_ne!(weak, disconnected);
        assert_ne!(disconnected, off);
        assert!(ink(&on) > ink(&weak));
    }

    #[test]
    fn volume_arc_and_dots_are_two_different_pictures() {
        let palette = Palette::template();
        let base = sample(None, CenterGlyph::Clipboard, Some(0.6));
        let arc = render(&base, &palette);
        let dots = render(
            &GlyphSpec {
                volume_style: VolumeStyle::Dots,
                ..base
            },
            &palette,
        );
        assert_ne!(arc, dots);
        let loud = GlyphSpec {
            volume: Some(1.0),
            ..base
        };
        let quiet = GlyphSpec {
            volume: Some(0.1),
            ..base
        };
        assert!(ink(&render(&loud, &palette)) > ink(&render(&quiet, &palette)));
        assert!(
            ink(&render(
                &GlyphSpec {
                    volume_style: VolumeStyle::Dots,
                    ..loud
                },
                &palette,
            )) > ink(&render(
                &GlyphSpec {
                    volume_style: VolumeStyle::Dots,
                    ..quiet
                },
                &palette,
            ))
        );
    }

    #[test]
    fn bluetooth_on_is_heavier_than_off() {
        let palette = Palette::template();
        let on = render(
            &sample(None, CenterGlyph::Bluetooth { powered: true }, None),
            &palette,
        );
        let off = render(
            &sample(None, CenterGlyph::Bluetooth { powered: false }, None),
            &palette,
        );
        assert_ne!(on, off);
        assert!(ink(&on) > ink(&off));
    }
}
