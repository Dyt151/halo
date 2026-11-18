//! 菜单栏图标与下拉面板配置。

use serde::{Deserialize, Serialize};

use super::default_true;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatusBarConfig {
    /// 是否显示菜单栏图标（关闭后托盘图标隐藏，快捷键与主窗口不受影响）。
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// 面板显示哪些模块、按什么顺序。
    #[serde(default = "default_statusbar_sections")]
    pub sections: Vec<String>,
    /// 菜单栏图标中心图形：`auto` / `wifi` / `bluetooth` / `clipboard`。
    #[serde(default = "default_glyph_center")]
    pub glyph_center: String,
    /// 外环画电量（没有电池的机器自动不画）。
    #[serde(default = "default_true")]
    pub glyph_battery_ring: bool,
    /// 环顶缺口里写电量数字。
    #[serde(default = "default_true")]
    pub glyph_battery_percent: bool,
    /// 底部画输出音量（四个点或一段圆弧条，见 `glyph_volume_style`）。
    ///
    /// 旧配置里的 `glyph_volume_dots` 是同一个开关，那时只有点这一种画法。
    #[serde(default = "default_true", alias = "glyph_volume_dots")]
    pub glyph_volume: bool,
    /// 音量区的画法：`arc`（圆弧条）/ `dots`（四个点）。
    #[serde(default = "default_glyph_volume_style")]
    pub glyph_volume_style: String,
    /// 有线连接改用普通 Wi-Fi 图标（关掉则画正版的双折线触点）。
    #[serde(default)]
    pub glyph_wifi_for_ethernet: bool,
    /// iPhone 个人热点改用普通 Wi-Fi 图标。
    #[serde(default)]
    pub glyph_wifi_for_hotspot: bool,
    /// 临时网络改用普通 Wi-Fi 图标。
    #[serde(default)]
    pub glyph_wifi_for_temporary: bool,
    /// 互联网共享改用普通 Wi-Fi 图标。
    #[serde(default)]
    pub glyph_wifi_for_sharing: bool,
    /// 彩色图标（电量弧上色）；关闭则交给系统按菜单栏明暗反色。
    #[serde(default = "default_true")]
    pub glyph_colored: bool,
    /// 彩色图标的墨色取向：`auto` / `light` / `dark`。
    #[serde(default = "default_glyph_appearance")]
    pub glyph_appearance: String,
}

/// 图标中心可选值（设置页与 normalize 共用一份）。
pub const GLYPH_CENTERS: [&str; 4] = ["auto", "wifi", "bluetooth", "clipboard"];

/// 彩色图标可选的明暗取向。
pub const GLYPH_APPEARANCES: [&str; 3] = ["auto", "light", "dark"];

/// 音量区画法可选值。
pub const GLYPH_VOLUME_STYLES: [&str; 2] = ["arc", "dots"];

fn default_glyph_center() -> String {
    "auto".to_string()
}

fn default_glyph_appearance() -> String {
    "auto".to_string()
}

fn default_glyph_volume_style() -> String {
    "arc".to_string()
}

/// 面板模块 id（顺序表与设置页共用）。
pub const STATUSBAR_SECTIONS: [&str; 7] = [
    "battery",
    "volume",
    "audio_input",
    "input_source",
    "network",
    "vpn",
    "bluetooth",
];

/// 真正能渲染的模块。
pub const STATUSBAR_AVAILABLE_SECTIONS: [&str; 7] = [
    "battery",
    "network",
    "vpn",
    "bluetooth",
    "volume",
    "audio_input",
    "input_source",
];

fn default_statusbar_sections() -> Vec<String> {
    STATUSBAR_AVAILABLE_SECTIONS
        .iter()
        .map(|s| s.to_string())
        .collect()
}

impl Default for StatusBarConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            sections: default_statusbar_sections(),
            glyph_center: default_glyph_center(),
            glyph_battery_ring: true,
            glyph_battery_percent: true,
            glyph_volume: true,
            glyph_volume_style: default_glyph_volume_style(),
            glyph_wifi_for_ethernet: false,
            glyph_wifi_for_hotspot: false,
            glyph_wifi_for_temporary: false,
            glyph_wifi_for_sharing: false,
            glyph_colored: true,
            glyph_appearance: default_glyph_appearance(),
        }
    }
}

impl StatusBarConfig {
    /// 去掉未知/重复的模块 id，并把图标中心修成合法值（可重复调用）。
    pub fn normalize(&mut self) {
        let mut seen = std::collections::HashSet::new();
        self.sections
            .retain(|id| STATUSBAR_SECTIONS.contains(&id.as_str()) && seen.insert(id.clone()));
        if !GLYPH_CENTERS.contains(&self.glyph_center.as_str()) {
            self.glyph_center = default_glyph_center();
        }
        if !GLYPH_APPEARANCES.contains(&self.glyph_appearance.as_str()) {
            self.glyph_appearance = default_glyph_appearance();
        }
        if !GLYPH_VOLUME_STYLES.contains(&self.glyph_volume_style.as_str()) {
            self.glyph_volume_style = default_glyph_volume_style();
        }
    }

    /// 图标内容的绘制参数。
    pub fn glyph_options(&self) -> crate::core::statusbar::glyph::GlyphOptions {
        crate::core::statusbar::glyph::GlyphOptions {
            center: crate::core::statusbar::glyph::CenterMode::parse(&self.glyph_center),
            battery_ring: self.glyph_battery_ring,
            volume: self.glyph_volume,
            volume_style: crate::core::statusbar::glyph::VolumeStyle::parse(
                &self.glyph_volume_style,
            ),
            battery_percent: self.glyph_battery_percent,
            wifi_for_ethernet: self.glyph_wifi_for_ethernet,
            wifi_for_hotspot: self.glyph_wifi_for_hotspot,
            wifi_for_temporary: self.glyph_wifi_for_temporary,
            wifi_for_sharing: self.glyph_wifi_for_sharing,
            colored: self.glyph_colored,
            appearance: crate::core::statusbar::glyph::Appearance::parse(&self.glyph_appearance),
        }
    }
}


