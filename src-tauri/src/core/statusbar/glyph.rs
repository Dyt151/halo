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
const WIFI_MIDDLE_CY: f64 = 78.89;
const WIFI_MIDDLE_R: f64 = 18.5;
const WIFI_MIDDLE_START: f64 = 227.5;
const WIFI_MIDDLE_END: f64 = 312.5;
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
    fn resolve(&self, rgb: [u8; 3]) -> [u8; 3] {
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

/// RSSI（dBm）→ 点亮弧数，阈值与面板里的信号格一致。
pub fn wifi_bars(rssi: Option<i32>) -> u8 {
    match rssi {
        Some(v) if v >= -60 => 3,
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

/// 缺口占整段弧的比例（正版用弧长归一化，不是角度）。
fn gap_fraction(width: f64) -> f64 {
    if !width.is_finite() || width <= 0.0 {
        return 0.0;
    }
    let arc_len = RING_R * deg(RING_SWEEP);
    (width / arc_len).clamp(0.0, 1.0)
}

/// 进度空间 `start..end`（0.0~1.0）映射成角度区间列表。
///
/// 正版 `batteryArc` 的语义：有缺口时，落进缺口里的那段进度是不可见的，于是
/// 一段进度被切成「缺口前」与「缺口后」两段分别描。
pub fn battery_arc_segments(start: f64, end: f64, top_gap_width: f64) -> Vec<(f64, f64)> {
    let s = start.clamp(0.0, 1.0);
    let e = end.clamp(0.0, 1.0);
    if !(e > s) {
        return Vec::new();
    }
    let angle = |p: f64| RING_START + RING_SWEEP * p;
    if top_gap_width <= 0.0 {
        return vec![(angle(s), angle(e))];
    }
    let gap = gap_fraction(top_gap_width);
    let gap_start = (1.0 - gap) / 2.0;
    let gap_end = gap_start + gap;
    let mut out = Vec::new();
    let first_end = e.min(gap_start);
    if first_end > s {
        out.push((angle(s), angle(first_end)));
    }
    let second_start = s.max(gap_end);
    if e > second_start {
        out.push((angle(second_start), angle(e)));
    }
    out
}

// ---------------------------------------------------------------------------
// 充电扫光（移植自 status-trio 的 ChargingEffect*）
// ---------------------------------------------------------------------------

/// 正版动画帧率：`ChargingEffectTimeline.framesPerSecond`。
pub const CHARGING_FPS: f64 = 20.0;
/// 常速一圈的时长。
pub const STEADY_CYCLE: f64 = 1.8;
/// 插上电源时那圈快扫的时长。
pub const BURST_CYCLE: f64 = 0.6;

const HEARTBEAT_DURATION: f64 = 0.25;
const TAIL_RATIO: f64 = 0.32;
const MAX_TAIL_TO_FILL_RATIO: f64 = 0.78;
const MIN_VISIBLE_FILL_RATIO: f64 = 0.25;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChargingKind {
    /// 插上电源的那圈快扫。
    Burst,
    /// 常速扫光。
    Steady,
}

impl ChargingKind {
    fn duration(self) -> f64 {
        match self {
            Self::Burst => BURST_CYCLE,
            Self::Steady => STEADY_CYCLE,
        }
    }
}

/// 当前处在动画循环的第几帧。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChargingPhase {
    pub step: usize,
    pub steps_per_cycle: usize,
    pub kind: ChargingKind,
    /// 命中「电量跳档」那一圈时心跳放大到 1.35。
    pub heartbeat_multiplier: f64,
}

/// 由「这段动画开始后过了多久」算出当前帧（正版 `ChargingEffectTimeline.phase`）。
pub fn charging_phase(
    elapsed: f64,
    kind: ChargingKind,
    heartbeat_multiplier: f64,
) -> Option<ChargingPhase> {
    if !elapsed.is_finite()
        || elapsed < 0.0
        || !heartbeat_multiplier.is_finite()
        || heartbeat_multiplier < 0.0
    {
        return None;
    }
    let duration = kind.duration();
    let steps_per_cycle = (duration * CHARGING_FPS).round() as usize;
    if steps_per_cycle == 0 {
        return None;
    }
    let cycle_elapsed = elapsed % duration;
    // 20Hz 的边沿可能差 1e-6 帧，只把这点残差向前贴，避免卡一帧。
    let frame_position = cycle_elapsed * CHARGING_FPS;
    let step = (frame_position + 1e-6).floor() as usize % steps_per_cycle;
    Some(ChargingPhase {
        step,
        steps_per_cycle,
        kind,
        heartbeat_multiplier,
    })
}

/// 一帧扫光要画的全部东西（正版 `ChargingEffectFrame`）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChargingFrame {
    /// 尾部渐变在进度空间的区间。
    pub tail: Option<(f64, f64)>,
    pub head: f64,
    pub head_visible: bool,
    pub tail_alpha: f64,
    pub bead_alpha: f64,
    pub heartbeat_alpha: f64,
    pub heartbeat_scale: f64,
    pub bolt_scale: f64,
    pub bolt_arc_color_amount: f64,
}

fn clamp_unit(value: f64) -> f64 {
    if !value.is_finite() {
        return 0.0;
    }
    value.clamp(0.0, 1.0)
}

/// 进度空间 → 圆上的点（设计单位，y 向下）。
pub fn battery_point(progress: f64) -> (f64, f64) {
    let angle = deg(RING_START + RING_SWEEP * clamp_unit(progress));
    (
        RING_CX + RING_R * angle.cos(),
        RING_CY + RING_R * angle.sin(),
    )
}

/// 进度空间 → 可见弧长归一值（缺口里的进度都映射到缺口左沿）。
pub fn visible_fraction(progress: f64, top_gap_width: f64) -> f64 {
    let p = clamp_unit(progress);
    if top_gap_width <= 0.0 {
        return p;
    }
    let gap = gap_fraction(top_gap_width);
    let gap_start = (1.0 - gap) / 2.0;
    let gap_end = gap_start + gap;
    if p <= gap_start {
        p
    } else if p >= gap_end {
        p - gap
    } else {
        gap_start
    }
}

/// 可见弧长归一值 → 进度空间（正版 `progress(forVisibleFraction:)`）。
pub fn progress_for_visible_fraction(fraction: f64, top_gap_width: f64) -> f64 {
    let f = clamp_unit(fraction);
    if top_gap_width <= 0.0 {
        return f;
    }
    let gap = gap_fraction(top_gap_width);
    let visible_length = 1.0 - gap;
    if visible_length <= 0.0 {
        return 0.0;
    }
    let visible = f.min(visible_length);
    let gap_start = visible_length / 2.0;
    if visible <= gap_start {
        visible
    } else {
        visible + gap
    }
}

/// 落在缺口里的进度截到左沿（正版 `lastVisibleProgress`）。
pub fn last_visible_progress(progress: f64, top_gap_width: f64) -> f64 {
    let p = clamp_unit(progress);
    if top_gap_width <= 0.0 {
        return p;
    }
    let gap = gap_fraction(top_gap_width);
    let gap_start = (1.0 - gap) / 2.0;
    let gap_end = gap_start + gap;
    if p > gap_start && p < gap_end {
        gap_start
    } else {
        p
    }
}

/// 由电量进度与当前帧推出扫光参数（正版 `ChargingEffectPolicy.frame`）。
pub fn charging_frame(
    progress: f64,
    phase: ChargingPhase,
    top_gap_width: f64,
) -> Option<ChargingFrame> {
    if !progress.is_finite()
        || phase.steps_per_cycle == 0
        || phase.step >= phase.steps_per_cycle
        || !phase.heartbeat_multiplier.is_finite()
        || phase.heartbeat_multiplier < 0.0
    {
        return None;
    }
    let battery_progress = progress.clamp(0.0, 1.0);
    let visible_arc_length = visible_fraction(1.0, top_gap_width);
    let visible_fill = visible_fraction(battery_progress, top_gap_width);
    let endpoint = last_visible_progress(battery_progress, top_gap_width);

    let heartbeat_steps = ((HEARTBEAT_DURATION * CHARGING_FPS).round() as usize)
        .clamp(1, phase.steps_per_cycle);
    let travel_steps = (phase.steps_per_cycle - heartbeat_steps).max(1);
    let travel_fraction = (phase.step as f64 / travel_steps as f64).min(1.0);
    let head = endpoint * travel_fraction;

    let heartbeat_progress = if phase.step >= travel_steps {
        let pulse_step = phase.step - travel_steps;
        if heartbeat_steps <= 1 {
            0.0
        } else {
            pulse_step as f64 / (heartbeat_steps - 1) as f64
        }
    } else {
        0.0
    };
    let bounded_heartbeat = heartbeat_progress.clamp(0.0, 1.0);
    let pulse = if bounded_heartbeat == 0.0 || bounded_heartbeat == 1.0 {
        0.0
    } else {
        (std::f64::consts::PI * bounded_heartbeat).sin()
    };
    let multiplier = phase.heartbeat_multiplier.min(1.35);
    let heartbeat_alpha = pulse * multiplier;
    let is_heartbeat_step = phase.step >= travel_steps;
    let heartbeat_scale = if is_heartbeat_step {
        (1.0 + 0.55 * (1.0 - pulse)) * multiplier
    } else {
        1.0
    };
    let head_visible = (last_visible_progress(head, top_gap_width) - head).abs() < 1e-9;

    let (bolt_scale, bolt_arc_color_amount) = if top_gap_width > 0.0 {
        let gap_start = last_visible_progress(0.5, top_gap_width);
        let gap_end = 1.0 - gap_start;
        if endpoint >= gap_end && head > gap_start && head < gap_end {
            let amount = (std::f64::consts::PI
                * ((head - gap_start) / (gap_end - gap_start)).clamp(0.0, 1.0))
            .sin();
            (1.0 + 0.2 * amount, amount)
        } else {
            (1.0, 0.0)
        }
    } else {
        (1.0, 0.0)
    };

    let minimum_tail_fill = MIN_VISIBLE_FILL_RATIO * visible_arc_length;
    let tail_length =
        (TAIL_RATIO * visible_arc_length).min(MAX_TAIL_TO_FILL_RATIO * visible_fill);
    let tail = if visible_fill >= minimum_tail_fill && head > 0.0 && tail_length > 0.0 {
        let visible_head = visible_fraction(head, top_gap_width);
        let start = progress_for_visible_fraction(
            (visible_head - tail_length).max(0.0),
            top_gap_width,
        );
        let end = progress_for_visible_fraction(visible_head, top_gap_width);
        Some((start, end))
    } else {
        None
    };

    Some(ChargingFrame {
        tail,
        head,
        head_visible,
        tail_alpha: if tail.is_some() { 1.0 } else { 0.0 },
        bead_alpha: if head_visible {
            if is_heartbeat_step {
                (1.0 - bounded_heartbeat).max(0.0)
            } else {
                1.0
            }
        } else {
            0.0
        },
        heartbeat_alpha,
        heartbeat_scale,
        bolt_scale,
        bolt_arc_color_amount,
    })
}

// ---------------------------------------------------------------------------
// 扫光配色（移植自 ChargingEffectPalette）
// ---------------------------------------------------------------------------

fn linearize(component: f64) -> f64 {
    if component <= 0.04045 {
        component / 12.92
    } else {
        ((component + 0.055) / 1.055).powf(2.4)
    }
}

fn relative_luminance(rgb: [u8; 3]) -> f64 {
    let [r, g, b] = rgb.map(|c| linearize(c as f64 / 255.0));
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

fn contrast_ratio(a: [u8; 3], b: [u8; 3]) -> f64 {
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

/// 渲染成 52×52 的 RGBA（模板模式下只有 alpha 承载信息）。
#[cfg(target_os = "macos")]
pub fn render(spec: &GlyphSpec, palette: &Palette) -> Vec<u8> {
    cg::paint(spec, palette, None, None)
}

/// 带充电扫光帧的渲染；`phase = None` 时与 [`render`] 完全一致。
#[cfg(target_os = "macos")]
pub fn render_frame(spec: &GlyphSpec, palette: &Palette, phase: Option<ChargingPhase>) -> Vec<u8> {
    cg::paint(spec, palette, phase, None)
}

/// 输入法切换瞬间的整格字牌：`ime` 有值时整枚图标换成字牌，不出电池环与音量。
#[cfg(target_os = "macos")]
pub fn render_with_ime(
    spec: &GlyphSpec,
    palette: &Palette,
    phase: Option<ChargingPhase>,
    ime: Option<ImeFlash>,
) -> Vec<u8> {
    cg::paint(spec, palette, phase, ime)
}

/// 非 macOS：保持可编译，交给一张全空的图（本项目只在 macOS 上真跑）。
#[cfg(not(target_os = "macos"))]
pub fn render(spec: &GlyphSpec, _palette: &Palette) -> Vec<u8> {
    let _ = spec;
    vec![0u8; IMAGE_SIZE * IMAGE_SIZE * 4]
}

#[cfg(not(target_os = "macos"))]
pub fn render_frame(
    spec: &GlyphSpec,
    _palette: &Palette,
    _phase: Option<ChargingPhase>,
) -> Vec<u8> {
    let _ = spec;
    vec![0u8; IMAGE_SIZE * IMAGE_SIZE * 4]
}

#[cfg(not(target_os = "macos"))]
pub fn render_with_ime(
    spec: &GlyphSpec,
    _palette: &Palette,
    _phase: Option<ChargingPhase>,
    _ime: Option<ImeFlash>,
) -> Vec<u8> {
    let _ = spec;
    vec![0u8; IMAGE_SIZE * IMAGE_SIZE * 4]
}

// ---------------------------------------------------------------------------
// CoreGraphics 光栅化（macOS）
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
mod cg {
    use super::*;
    use core_foundation::base::TCFType as _;
    use core_graphics::color_space::CGColorSpace;
    use core_graphics::context::{
        CGBlendMode, CGContext, CGLineCap, CGLineJoin, CGPathDrawingMode,
    };
    use core_graphics::geometry::{CGAffineTransform, CGPoint, CGRect, CGSize};
    use core_graphics::gradient::{CGGradient, CGGradientDrawingOptions};

    /// 超采样：120 单位先落到 208（约 1.73 像素/单位），再盒式平均成 52。
    /// 直接按 0.433 像素/单位画的话，细线和数字边缘会糊掉。
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
    fn draw_charging_effect(
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

    fn fill_dot(ctx: &CGContext, cx: f64, cy: f64, radius: f64, rgb: [u8; 3], alpha: f64) {
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
    fn draw_digits(ctx: &CGContext, palette: &Palette, percent: u8) {
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
        min_x: f64,
        max_x: f64,
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
            min_x,
            max_x,
            min_y,
            max_y,
        }
    }

    /// 充电闪电：只放闪电时缺口收窄到 50，尺寸按数字墨高校准。
    ///
    /// 有扫光帧时再叠一层正版的 `centeredScale`（绕字形包围盒中心放大，纵向
    /// 卡在画布内），并把手心扫到缺口时的放大/提色应用上去。
    fn draw_bolt(
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
    fn draw_plug(ctx: &CGContext, palette: &Palette) {
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

    /// 输入法字牌：整格图标，22×16pt 的圆角矩形 + 居中字符。
    fn draw_ime_chip(ctx: &CGContext, palette: &Palette, flash: ImeFlash) {
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
    fn draw_ime_text(ctx: &CGContext, palette: &Palette, marker: char, alpha: f64) {
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

    fn draw_center(ctx: &CGContext, palette: &Palette, center: CenterGlyph) {
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

    fn draw_footer(ctx: &CGContext, palette: &Palette, style: VolumeStyle, volume: f64) {
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

    /// CG 给的是预乘 RGBA，tauri 的 `Image::new_owned` 要非预乘；降采样也要在
    /// 预乘空间做，最后才除回 alpha。
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

    impl Path<'_> {
        /// 三次贝塞尔的便捷包装（正版 Wi-Fi 水滴点全是三次曲线）。
        fn add_curve(&mut self, c1: (f64, f64), c2: (f64, f64), end: (f64, f64)) {
            self.ctx
                .add_curve_to_point(c1.0, c1.1, c2.0, c2.1, end.0, end.1);
            self.set(end.0, end.1);
        }
    }
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(wifi_bars(Some(-60)), 3);
        assert_eq!(wifi_bars(Some(-61)), 2);
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
}
