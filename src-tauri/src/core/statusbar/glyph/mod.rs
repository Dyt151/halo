//! 菜单栏图标：把 status-trio 的矢量常数直接交给 CoreGraphics 描边排版。
//!
//! 版式沿用 status-trio 的「三区一图标」约定（一个 NSStatusItem，而不是每个
//! 模块一个）：
//! - **外环** = 电池：从左下角起顺时针按电量填充；顶部按内容留缺口——写数字留
//!   64 单位，只放闪电/插头留 50 单位，两样都没有就不留缺口、环是闭合的。
//! - **中心** = 网络：普通 Wi-Fi 是两段弧加一颗水滴点；iPhone 个人热点是两个
//!   互扣的圆角环；有线是双折线加三个触点；临时网络 / 互联网共享是实心扇形再
//!   抠掉一小块（用 `.clear` 混合模式挖洞）。
//! - **底部** = 输出音量：4 个点，或一段圆弧条。音量弧用的就是电池环那个圆
//!   （半径 51.5，圆心 (59.5, 61.49)），121.82°→59.12° 恰好落在环本来就没画的
//!   那段 117.38° 里，所以两块天然不打架。
//!
//! 为什么改成 CoreGraphics：纯 CPU 光栅器画不出系统圆体，电量数字一直是别的
//! 字的形状；而正版的全部观感就藏在那套矢量常数和那枚 SF Pro Rounded 里。
//! 现在几何常数、线宽、缺口宽度、基线位置一律照抄，不再自己发明。
//!
//! 尺寸与粗细写死，不做设置项：菜单栏图标 26pt（`NSStatusItem` 的常规光学尺寸；
//! 24pt 的图比正版小一圈），52×52 的 RGBA 就是它的 2x（120 单位的设计空间 → 1 单位
//! ≈ 0.433 输出像素）。线宽取正版「粗」档（strokeScale 1.5），字号取正版默认
//! textScale 1.8（20×1.8 = 36 号）。tray-icon 自己会把每张图压成 18pt 高，所以
//! 贴图之后还要把 NSImage 的 size 覆写回 26pt（见 `tray::apply`）。
//!
//! 三处与正版的行为对齐，和早期实现不同：低电量只换颜色**不**加粗；数字与闪电
//! 都用墨色，电池色只给弧；充电时缺口里只放闪电、不写数字；接了电源但没在充
//! （已充满或到充电上限）时缺口里放插头、弧仍是绿色。
//!
//! 充电时外环上还有一段扫光：尾部渐变 + 亮点 + 心跳光晕，位置由 20fps 的
//! [`ChargingPhase`] 推出来，公式逐行照抄正版 `ChargingEffectTimeline` /
//! `ChargingEffectPolicy` / `ChargingEffectPalette`；插上电源先播 0.6s 的快扫
//! （burst），之后是 1.8s 一圈的常速扫光，每圈最后 5 帧做心跳。
//!
//! 设计空间的 y 轴朝下（靠 `translate(0, h)` + `scale(px, -px)` 得到），与正版
//! 一致；因此 `polar()` 里角度增大就是视觉顺时针。输出缓冲的首行对应画布顶边，
//! 读回时不再翻面。
//!
//! 几何与配色常数移植自 status-trio（Apache-2.0，完整许可与改动清单见仓库根目录
//! 的 LICENSE-STATUS-TRIO.txt 与 NOTICE-STATUS-TRIO.txt）的 StatusIconGeometry.swift
//! 与 StatusIconRenderer.swift，改写为 Rust + CoreGraphics；改动之处是：线宽/字号
//! 写死成单一档位，且 SF Symbol 中心符号换成按其实测尺寸复现的矢量路径（Rust 侧没有
//! NSImage）。

use serde::Serialize;

/// [`render`] 输出的边长（px）：26pt × 2x。
pub const IMAGE_SIZE: usize = 52;

/// 菜单栏图标的逻辑边长（pt）。与正版实测包围盒一致（50px @2x）。
pub const ICON_POINTS: f64 = 26.0;

// ---------------------------------------------------------------------------
// 设计空间常量（120 单位见方，y 向下）
// ---------------------------------------------------------------------------

/// 画布边长（正版 `StatusIconGeometry.canvas`）。
const CANVAS: f64 = 120.0;
/// 所有元素的水平锚点：正版刻意用 59.5 而不是画布中心 60.0（正版 issue #30，
/// 原画是从 x=15.5 铺到 x=103.5 的，中点就是 59.5）。
const AX: f64 = 59.5;
/// 电池环的圆心与半径。
const RING_CX: f64 = AX;
const RING_CY: f64 = 61.487_152_617_854_73;
const RING_R: f64 = 51.5;
/// 环的起点与总扫过角度（度）。
const RING_START: f64 = 148.690_086_892_811_17;
const RING_SWEEP: f64 = 242.619_826_214_377_7;
/// 顶部缺口宽度：写数字 64，只放指示符 50。
const GAP_VALUE: f64 = 64.0;
const GAP_INDICATOR: f64 = 50.0;

/// 线宽标度：写死正版「粗」档。
const STROKE_SCALE: f64 = 1.5;
/// 字号标度：写死正版默认值。
const TEXT_SCALE: f64 = 1.8;
/// 电池环线宽。
const RING_LINE: f64 = 8.0 * STROKE_SCALE;
/// 音量弧线宽。
const FOOT_LINE: f64 = 7.0 * STROKE_SCALE;
/// 中心符号线宽（正版扇形与音量弧同为 7）。
const SYMBOL_LINE: f64 = 7.0;

/// 电量数字：基础字号 20 × textScale 1.8。
const FONT_SIZE: f64 = 20.0 * TEXT_SCALE;
/// 正版对数字字距的处理：负的字距把两位数字挤到一起。
const FONT_KERN: f64 = -(FONT_SIZE * 0.04);

/// 闪电绕自己的尖端缩放，缩放前的包围盒高度（正版取 boundingBoxOfPath）。
const BOLT_PIVOT_X: f64 = AX;
const BOLT_PIVOT_Y: f64 = 2.1;
const BOLT_HEIGHT: f64 = 19.5597;
/// 闪电与数字的高度比（正版 batteryChargingBoltCalibration）。
const BOLT_CALIBRATION: f64 = 220.0 / 180.0;
/// 闪电路径的包围盒（含贝塞尔控制点，和 CGPath 的 boundingBox 一致）。
const BOLT_MIN_X: f64 = 51.3;
const BOLT_MAX_X: f64 = 67.6;
const BOLT_MIN_Y: f64 = 2.1;
const BOLT_MAX_Y: f64 = 22.0;

/// 插头相对闪电的光学高度标度（正版 batteryPlugHeightScale）。
const PLUG_HEIGHT_SCALE: f64 = 1.2;
/// `powerplug.portrait.fill` 排版盒的宽高比（本机 200pt 实测 192×266）。
const PLUG_BOX_ASPECT: f64 = 192.0 / 266.0;
/// 插头墨迹轮廓：本机把符号光栅化到 12× 后取 50% 等值线、按 0.5pt 简化得到，
/// `u` 是排版盒宽度归一值（墨迹水平中心落在 0.5），`v` 自盒顶向下、插针朝上。
const PLUG_INK: [(f64, f64); 61] = [
    (0.56991, 0.93423),
    (0.41410, 0.93303),
    (0.38111, 0.92493),
    (0.35759, 0.90946),
    (0.34664, 0.89348),
    (0.34107, 0.87312),
    (0.34045, 0.72274),
    (0.33265, 0.69173),
    (0.30742, 0.66291),
    (0.23311, 0.61598),
    (0.19306, 0.58177),
    (0.15920, 0.53822),
    (0.13746, 0.48841),
    (0.13048, 0.44142),
    (0.13052, 0.33678),
    (0.13541, 0.31610),
    (0.15095, 0.29731),
    (0.17625, 0.28589),
    (0.20142, 0.28217),
    (0.26949, 0.28133),
    (0.27077, 0.11090),
    (0.27850, 0.09555),
    (0.29065, 0.08396),
    (0.30733, 0.07503),
    (0.32729, 0.06980),
    (0.35377, 0.06949),
    (0.37894, 0.07636),
    (0.39819, 0.08897),
    (0.40993, 0.10558),
    (0.41296, 0.11936),
    (0.41323, 0.28165),
    (0.58772, 0.28164),
    (0.59068, 0.10683),
    (0.60239, 0.08960),
    (0.61983, 0.07764),
    (0.64240, 0.07036),
    (0.66931, 0.06917),
    (0.68927, 0.07319),
    (0.70868, 0.08239),
    (0.72283, 0.09524),
    (0.73058, 0.11028),
    (0.73222, 0.28164),
    (0.79865, 0.28217),
    (0.82729, 0.28687),
    (0.85342, 0.30075),
    (0.86782, 0.32362),
    (0.86952, 0.45207),
    (0.86302, 0.48841),
    (0.84998, 0.52224),
    (0.82784, 0.55796),
    (0.79569, 0.59305),
    (0.75915, 0.62166),
    (0.69535, 0.66120),
    (0.67216, 0.68452),
    (0.66359, 0.70175),
    (0.66013, 0.72024),
    (0.65989, 0.86623),
    (0.65470, 0.89129),
    (0.64332, 0.90883),
    (0.62460, 0.92243),
    (0.59943, 0.93089),
];

/// 转淡轨的 alpha（正版 inactiveTrackAlpha）。
const INACTIVE_ALPHA: f64 = 0.22;

/// 中心符号的水准点（正版 wifiSymbolCenter）。
const SYMBOL_CX: f64 = AX;
const SYMBOL_CY: f64 = 64.0;

/// 音量点：正版是 4 个，半径随线宽标度略微放大。
const VOLUME_DOTS: [(f64, f64); 4] = [(33.0, 104.2), (50.5, 111.2), (68.5, 111.7), (86.0, 105.8)];
const VOLUME_DOT_R: f64 = 5.5 * (1.0 + (STROKE_SCALE - 1.0) * 0.5);
/// 音量弧的角度范围（度，同一方向递减：从左下扫到右下）。
const VOLUME_ARC_START: f64 = 121.82;
const VOLUME_ARC_END: f64 = 59.12;

/// 中心符号的放大倍率。正版是设置项 `wifiSymbolScale`（区间 1.0–1.8，出厂默认
/// 1.6），这里写死成用户机器上正版对应的 **1.5**：SF Symbol `wifi` 在
/// pointSize 38×1.5=57 下的实测墨迹高 46.9 单位 = 参考截图里的 21px@2x，
/// 1.6 会到 22.1px，比屏幕上那枚大一圈。
const CENTER_SCALE: f64 = 1.5;

/// 输入法字牌（Apple 输入菜单图标）的尺寸：22×16pt，圆角 4.8pt。
///
/// 本机正版截图量得字牌墨迹 44×32px、圆角 9.5px、描边 2px（52px 图标内），
/// 按 120 单位画布换算即下面这组数。
const IME_W: f64 = 101.5;
const IME_H: f64 = 74.0;
const IME_R: f64 = 28.0;
const IME_STROKE: f64 = 4.6;
const IME_CX: f64 = AX;
const IME_CY: f64 = 60.0;
/// 中文/假名/韩文的字号（墨迹约 22×20px）。
const IME_CJK_SIZE: f64 = 51.0;
/// ABC 这类按键布局的字号（A 的墨迹约 16×17px）。
const IME_LATIN_SIZE: f64 = 60.0;

/// 正版兜底矢量 Wi-Fi（`StatusIconGeometry.wifiArcs`）的外弧 / 中弧圆心与角度，
/// 倍率 1.0。临时连接与互联网共享的扇形就建在这套弧上，所以必须留着。
const WIFI_OUTER_CY: f64 = 78.3;
const WIFI_OUTER_R: f64 = 31.0;
const WIFI_OUTER_START: f64 = 227.35;
const WIFI_OUTER_END: f64 = 312.65;
/// 真正出现在菜单栏上的 Wi-Fi：正版画的是 SF Symbol `wifi`（semibold，
/// pointSize = 38×1.5）。Rust 侧没有 NSImage，下面这组数是本机把该符号光栅化
/// 到 12× 后按径向中心线拟合圆得到的实测几何，并已按「墨迹中心坐进水准点
/// (59.5, 64)」平移。角度沿用设计空间 y 向下、270° 为正上方的写法，
/// `*_SWEEP` 是半张角。
const WIFI_SYMBOL_OUTER_CY: f64 = 86.34;
const WIFI_SYMBOL_OUTER_R: f64 = 41.24;
const WIFI_SYMBOL_OUTER_SWEEP: f64 = 42.25;
const WIFI_SYMBOL_OUTER_LINE: f64 = 9.21;
const WIFI_SYMBOL_MIDDLE_CY: f64 = 87.02;
const WIFI_SYMBOL_MIDDLE_R: f64 = 25.17;
const WIFI_SYMBOL_MIDDLE_SWEEP: f64 = 39.85;
const WIFI_SYMBOL_MIDDLE_LINE: f64 = 9.24;
/// 底部那颗点：实测墨迹 19.08×13.58 的扁椭圆，不是兜底矢量里的水滴。
const WIFI_SYMBOL_DOT: (f64, f64, f64, f64) = (SYMBOL_CX, 80.67, 9.54, 6.79);
/// 「关闭」斜杠与「无网络」标记（倍率 1.0，跟扇形一样绕水准点放大）。
const WIFI_OFF_SLASH: [(f64, f64); 2] = [(39.0, 46.0), (81.0, 79.0)];
const NO_INTERNET_STEM: [(f64, f64); 2] = [(AX, 54.5), (AX, 67.0)];
const NO_INTERNET_DOT: (f64, f64, f64) = (56.9, 72.9, 5.2);
/// 热点链环：单个圆角环的外框尺寸、线宽、互扣缝隙宽度（按正版符号实测墨迹
/// 49×32.75 反推：两个 37×23 的环错开 (−11, +9)）。
const HOTSPOT_LINK: (f64, f64) = (37.0, 23.0);
const HOTSPOT_LINE: f64 = 4.6;
const HOTSPOT_GAP: f64 = 1.2;
/// 两个链环的中心：上层偏右上，下层偏左下。
const HOTSPOT_FRONT: (f64, f64) = (65.5, 59.5);
const HOTSPOT_BACK: (f64, f64) = (54.5, 68.5);

/// 电量低于这个值就转红（正版 criticalThreshold 20，判定是严格小于）。
const LOW_BATTERY: u8 = 20;

fn deg(rad: f64) -> f64 {
    rad * std::f64::consts::PI / 180.0
}


// ---------------------------------------------------------------------------
// 内容开关与规格
// ---------------------------------------------------------------------------

/// 中心显示什么。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CenterGlyph {
    /// 普通 Wi-Fi（基础设施网络）：`bars` 为点亮的弧数（0~3）。
    /// `powered = false` 画斜杠；`connected = false` 画「无网络」标记。
    Wifi {
        bars: u8,
        powered: bool,
        connected: bool,
    },
    /// iPhone / iPad 个人热点：两只互扣的钩，不画 Wi-Fi 弧。
    Hotspot { powered: bool, connected: bool },
    /// 有线网络：双折线 + 三个触点。
    Ethernet { connected: bool },
    /// 临时网络：实心扇形，中间抠掉一台小显示器。
    Temporary { connected: bool },
    /// 互联网共享：实心扇形，中间抠掉一支上箭头。
    Shared { connected: bool },
    /// 蓝牙：关闭时整枚字形转淡并加斜杠。
    Bluetooth { powered: bool },
    /// 剪贴板（Halo 自身标识，用于没有任何可显示状态时）。
    Clipboard,
}

/// 中心图形的位置偏好（设置页「菜单栏图标」）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CenterMode {
    /// 有网络用网络（再按链路类型分热点/有线/Wi-Fi），否则蓝牙，否则剪贴板。
    Auto,
    Wifi,
    Bluetooth,
    Clipboard,
}

impl CenterMode {
    /// 前端传的是 `"auto"` / `"wifi"` / `"bluetooth"` / `"clipboard"`。
    pub fn parse(raw: &str) -> Self {
        match raw {
            "wifi" => Self::Wifi,
            "bluetooth" => Self::Bluetooth,
            "clipboard" => Self::Clipboard,
            _ => Self::Auto,
        }
    }
}

/// 彩色图标用哪套墨色（设置页「菜单栏明暗」）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Appearance {
    /// 跟随系统菜单栏：深底下白墨、浅底下黑墨。
    Auto,
    Light,
    Dark,
}

impl Appearance {
    /// 前端传的是 `"auto"` / `"light"` / `"dark"`。
    pub fn parse(raw: &str) -> Self {
        match raw {
            "light" => Self::Light,
            "dark" => Self::Dark,
            _ => Self::Auto,
        }
    }
}

/// 音量区的画法（正版「音量指示样式」二选一）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum VolumeStyle {
    /// 四个离散点。
    Dots,
    /// 连续圆弧条。
    Arc,
}

impl VolumeStyle {
    /// 前端传的是 `"dots"` / `"arc"`。
    pub fn parse(raw: &str) -> Self {
        match raw {
            "dots" => Self::Dots,
            _ => Self::Arc,
        }
    }
}

/// 图标内容开关（来自配置；三区不是全都必须出现）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GlyphOptions {
    pub center: CenterMode,
    pub battery_ring: bool,
    /// 画不画音量区。
    pub volume: bool,
    pub volume_style: VolumeStyle,
    /// 在环的缺口里画电量数字（关掉就只留弧）。
    pub battery_percent: bool,
    /// 「连接图标样式」四项：按链路类型分别决定要不要退回普通 Wi-Fi 图标。
    /// 正版是四个独立开关，不是一个总开关——热点想用 Wi-Fi 样式不代表有线也想。
    pub wifi_for_ethernet: bool,
    pub wifi_for_hotspot: bool,
    pub wifi_for_temporary: bool,
    pub wifi_for_sharing: bool,
    /// 彩色图标；关掉走系统模板反色。
    pub colored: bool,
    pub appearance: Appearance,
}

impl Default for GlyphOptions {
    fn default() -> Self {
        Self {
            center: CenterMode::Auto,
            battery_ring: true,
            volume: true,
            volume_style: VolumeStyle::Arc,
            battery_percent: true,
            wifi_for_ethernet: false,
            wifi_for_hotspot: false,
            wifi_for_temporary: false,
            wifi_for_sharing: false,
            colored: true,
            appearance: Appearance::Auto,
        }
    }
}

/// 环顶缺口的内容；决定缺口宽度与缺口里画什么。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gap {
    /// 不留缺口，环闭合。
    Closed,
    /// 写电量数字。
    Value,
    /// 只放闪电（正在充电）。
    Bolt,
    /// 只放插头（接了电源但没在充，例如已充满或到了充电上限）。
    Plug,
}

/// 一次重绘需要的全部状态；由 [`super::glyph_spec`] 从系统读数推导。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlyphSpec {
    /// `None` = 没有电池或不显示环。
    pub battery: Option<u8>,
    pub charging: bool,
    /// 是否接了电源（不一定在充电）。
    pub plugged: bool,
    /// 系统低电量模式：只影响弧的颜色（黄），不改缺口内容。
    pub low_power: bool,
    /// 缺口里的数字；`None` = 不写数字。
    pub percent_label: Option<u8>,
    pub center: CenterGlyph,
    /// 输出音量 0.0~1.0；`None` = 不画音量区。
    pub volume: Option<f64>,
    pub volume_style: VolumeStyle,
}

impl GlyphSpec {
    /// 只有剪贴板标识的最小规格（非 macOS 平台与预览兜底）。
    pub fn plain() -> Self {
        Self {
            battery: None,
            charging: false,
            plugged: false,
            low_power: false,
            percent_label: None,
            center: CenterGlyph::Clipboard,
            volume: None,
            volume_style: VolumeStyle::Arc,
        }
    }

    /// 环顶缺口放什么，优先级照抄正版 `batteryGapContent`：充电放闪电，其次接
    /// 电源放插头，再次数字，都没有就闭合。
    pub fn gap(&self) -> Gap {
        if self.battery.is_none() {
            return Gap::Closed;
        }
        if self.charging {
            Gap::Bolt
        } else if self.plugged {
            Gap::Plug
        } else if self.percent_label.is_some() {
            Gap::Value
        } else {
            Gap::Closed
        }
    }

    /// 缺口宽度（设计单位）；`Gap::Closed` 返回 0。
    pub fn top_gap_width(&self) -> f64 {
        match self.gap() {
            Gap::Closed => 0.0,
            Gap::Value => GAP_VALUE,
            Gap::Bolt | Gap::Plug => GAP_INDICATOR,
        }
    }

    /// 中心图形的名字（设置页文案与预览共用）。
    pub fn center_name(&self) -> &'static str {
        match self.center {
            CenterGlyph::Wifi { .. } => "wifi",
            CenterGlyph::Hotspot { .. } => "hotspot",
            CenterGlyph::Ethernet { .. } => "ethernet",
            CenterGlyph::Temporary { .. } => "temporary",
            CenterGlyph::Shared { .. } => "shared",
            CenterGlyph::Bluetooth { .. } => "bluetooth",
            CenterGlyph::Clipboard => "clipboard",
        }
    }

    /// 音量档位（0~4），给提示文字与设置页用。
    pub fn volume_steps(&self) -> Option<u8> {
        self.volume.map(|v| volume_steps(v, false))
    }
}

/// 输入法切换时整枚图标短暂换成的字牌。
///
/// 样式跟正版输入菜单图标一致：中文/假名/韩文用实心字牌挖空文字，ABC 这类
/// 按键布局用圆角描边字牌 + 字母。`alpha`/`scale` 承载淡入与缩放动画。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImeFlash {
    pub marker: char,
    /// 实心（挖空文字）；`false` 只描边。
    pub filled: bool,
    /// 0.0 ~ 1.0。
    pub alpha: f64,
    /// 1.0 = 原尺寸。
    pub scale: f64,
}

impl ImeFlash {
    pub fn new(marker: char, alpha: f64, scale: f64) -> Self {
        Self {
            marker,
            // 只有 ABC 这类拉丁按键布局用描边字母，其余输入法都是实心字牌。
            filled: !marker.is_ascii_uppercase(),
            alpha: alpha.clamp(0.0, 1.0),
            scale,
        }
    }
}

/// RSSI（dBm）→ 点亮弧数，阈值与面板里的信号格一致（正版 `StatusMappings.wifiBars`：
/// -70 起满格、-78/-88 递减）。
pub fn wifi_bars(rssi: Option<i32>) -> u8 {
    match rssi {
        Some(v) if v >= -70 => 3,
        Some(v) if v >= -78 => 2,
        Some(v) if v >= -88 => 1,
        Some(_) => 1,
        None => 0,
    }
}

/// 音量 0.0~1.0 → 点亮点数（0~4）；静音或 0 就是 0 个，与正版分档一致。
pub fn volume_steps(volume: f64, muted: bool) -> u8 {
    if muted {
        return 0;
    }
    if !volume.is_finite() || volume <= 0.0 {
        return 0;
    }
    let v = volume.clamp(0.0, 1.0);
    if v <= 0.25 {
        1
    } else if v <= 0.50 {
        2
    } else if v <= 0.75 {
        3
    } else {
        4
    }
}


mod charging;
mod draw;
mod palette;
mod render;

#[cfg(test)]
mod tests;

pub use charging::*;
pub use palette::*;
pub use render::*;
