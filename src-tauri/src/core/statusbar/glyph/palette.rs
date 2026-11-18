//! 菜单栏配色与扫光颜色计算。

use super::*;

// ---------------------------------------------------------------------------
// 配色
// ---------------------------------------------------------------------------

/// 配色：模板图只用黑，彩色图只给电池弧一个状态色、其余全是墨色。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    pub colored: bool,
    pub ink: [u8; 3],
    pub charging: [u8; 3],
    /// 低电量模式的黄（正版 `systemYellow`）。
    pub low_power: [u8; 3],
    /// 临界电量的红（正版 `systemRed`）。
    pub low: [u8; 3],
}

impl Palette {
    /// 交给系统反色的模板图：所有颜色落到 alpha 里，RGB 恒黑。
    pub fn template() -> Self {
        Self {
            colored: false,
            ink: [0, 0, 0],
            charging: [0, 0, 0],
            low_power: [0, 0, 0],
            low: [0, 0, 0],
        }
    }

    /// 彩色图：`dark_bar = true` 时菜单栏偏暗，墨色要下白。
    ///
    /// 三个状态色不是写死一套——正版 `BatteryStatusColorPalette.resolved(appearance:)`
    /// 每次渲染都按当前外观重新解析系统语义色，所以浅色菜单栏用 aqua 一套、
    /// 深色用 darkAqua 一套（本机 deviceRGB 实测值）。
    pub fn colored(dark_bar: bool) -> Self {
        Self {
            colored: true,
            ink: if dark_bar {
                [255, 255, 255]
            } else {
                [24, 24, 26]
            },
            charging: if dark_bar {
                [48, 209, 88]
            } else {
                [52, 199, 89]
            },
            low_power: if dark_bar {
                [255, 214, 0]
            } else {
                [255, 204, 0]
            },
            low: if dark_bar {
                [255, 66, 69]
            } else {
                [255, 56, 60]
            },
        }
    }

    /// 电池弧的颜色：正版 `batteryColorRole` 的判定顺序照抄——临界红 >
    /// 低电量模式黄 > 充电/接电源绿，**其余一律用前景墨色**。
    ///
    /// 黄色不是「电量低」，它只代表系统开着低电量模式，而且**压过绿色**：
    /// 插着电但开着低电量模式时，正版画的也是黄环。
    pub fn battery_rgb(&self, percent: u8, low_power: bool, powered: bool) -> [u8; 3] {
        if percent < LOW_BATTERY {
            self.low
        } else if low_power {
            self.low_power
        } else if powered {
            self.charging
        } else {
            self.ink
        }
    }

    /// 模板模式下颜色一律落回黑色，信息只能靠 alpha 表达。
    pub(super) fn resolve(&self, rgb: [u8; 3]) -> [u8; 3] {
        if self.colored {
            rgb
        } else {
            [0, 0, 0]
        }
    }
}

/// 明暗判定：`menu_bar_dark` 是系统读数，只有 `Appearance::Auto` 才用它。
pub fn is_dark_bar(appearance: Appearance, menu_bar_dark: Option<bool>) -> bool {
    match appearance {
        Appearance::Dark => true,
        Appearance::Light => false,
        // 读不到系统外观时按浅色处理：黑墨在浅色菜单栏上是 macOS 的默认样子。
        Appearance::Auto => menu_bar_dark.unwrap_or(false),
    }
}

impl Palette {
    /// 由图标开关 + 系统明暗推出配色（托盘与设置页预览共用同一条判定）。
    pub fn for_options(options: &GlyphOptions, menu_bar_dark: Option<bool>) -> Self {
        if !options.colored {
            return Self::template();
        }
        Self::colored(is_dark_bar(options.appearance, menu_bar_dark))
    }
}


// ---------------------------------------------------------------------------
// 扫光配色（移植自 ChargingEffectPalette）
// ---------------------------------------------------------------------------

pub(super) fn linearize(component: f64) -> f64 {
    if component <= 0.04045 {
        component / 12.92
    } else {
        ((component + 0.055) / 1.055).powf(2.4)
    }
}

pub(super) fn relative_luminance(rgb: [u8; 3]) -> f64 {
    let [r, g, b] = rgb.map(|c| linearize(c as f64 / 255.0));
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

pub(super) fn contrast_ratio(a: [u8; 3], b: [u8; 3]) -> f64 {
    let (la, lb) = (relative_luminance(a), relative_luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// 两色按比例混合（正版 `ChargingEffectPalette.blend`）。
pub fn blend_rgb(a: [u8; 3], b: [u8; 3], amount: f64) -> [u8; 3] {
    let t = amount.clamp(0.0, 1.0);
    [
        (a[0] as f64 + (b[0] as f64 - a[0] as f64) * t).round() as u8,
        (a[1] as f64 + (b[1] as f64 - a[1] as f64) * t).round() as u8,
        (a[2] as f64 + (b[2] as f64 - a[2] as f64) * t).round() as u8,
    ]
}

/// 与底色拉开 1.8:1 对比度的扫光色：优先提亮，提不亮就压暗。
pub fn automatic_highlight(rgb: [u8; 3]) -> [u8; 3] {
    const MINIMUM_CONTRAST: f64 = 1.8;
    let white = [255, 255, 255];
    let black = [0, 0, 0];
    let toward = if contrast_ratio(rgb, white) >= MINIMUM_CONTRAST {
        white
    } else if contrast_ratio(rgb, black) >= MINIMUM_CONTRAST {
        black
    } else if contrast_ratio(rgb, white) >= contrast_ratio(rgb, black) {
        white
    } else {
        black
    };
    // 32 步二分找「刚好达标」的混合比例，压低对比度变化的突兀感。
    let (mut low, mut high) = (0.0, 1.0);
    for _ in 0..32 {
        let mid = (low + high) / 2.0;
        if contrast_ratio(rgb, blend_rgb(rgb, toward, mid)) >= MINIMUM_CONTRAST {
            high = mid;
        } else {
            low = mid;
        }
    }
    blend_rgb(rgb, toward, high)
}

/// 闪电心跳时用的颜色：扫光色不够亮就退回「底色混 35% 白」。
pub fn charging_bolt_highlight(rgb: [u8; 3], highlight: [u8; 3]) -> [u8; 3] {
    if relative_luminance(highlight) > relative_luminance(rgb) {
        highlight
    } else {
        blend_rgb(rgb, [255, 255, 255], 0.35)
    }
}

