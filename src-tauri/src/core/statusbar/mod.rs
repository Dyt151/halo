//! 状态栏面板的系统控制后端。
//!
//! 已接入：电池、音量、声音输入、输入法（第一批）与网络、蓝牙（第二批）。
//! VPN 与私有能力（AirPods 降噪、充电上限、iPhone 电量）只在 [`future`]
//! 里留接口，不实现。

pub mod future;
pub mod glyph;
pub mod tray;

#[cfg(target_os = "macos")]
mod cf;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as platform;

#[cfg(not(target_os = "macos"))]
mod other;
#[cfg(not(target_os = "macos"))]
use other as platform;

use std::sync::atomic::{AtomicU8, AtomicU32, Ordering};

use serde::Serialize;

/// 菜单栏明暗缓存：0 = 还没读到，1 = 浅色，2 = 深色。
///
/// 必须由主线程从 `NSStatusBarButton.effectiveAppearance` 写入：菜单栏会跟着
/// 壁纸明暗走（VibrantDark 之类），和系统「外观」偏好不是一回事。
static MENU_BAR_DARK: AtomicU8 = AtomicU8::new(0);

/// 系统强调色缓存（0xRRGGBB），默认 macOS 蓝。
static SYSTEM_ACCENT: AtomicU32 = AtomicU32::new(0x007A_FF);

/// 一个音频设备（输出或输入）。
#[derive(Debug, Clone, Serialize)]
pub struct AudioDevice {
    pub id: u32,
    pub name: String,
    pub is_default: bool,
}

/// 默认音频设备的状态；没有设备时 `available = false`。
#[derive(Debug, Clone, Serialize)]
pub struct AudioDeviceState {
    pub available: bool,
    pub device_id: u32,
    pub device_name: String,
    /// 0.0 ~ 1.0。
    pub volume: f64,
    /// 该设备是否真的暴露了音量属性（部分 HDMI / 虚拟设备没有）。
    pub volume_available: bool,
    pub muted: bool,
    pub mute_available: bool,
    pub devices: Vec<AudioDevice>,
}

impl AudioDeviceState {
    pub fn unavailable() -> Self {
        Self {
            available: false,
            device_id: 0,
            device_name: "无可用设备".to_string(),
            volume: 0.0,
            volume_available: false,
            muted: false,
            mute_available: false,
            devices: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BatteryState {
    pub present: bool,
    pub percent: u8,
    pub charging: bool,
    /// IOPS `Is Charged`：系统判定「真的充满了」，和「接电且 100%」不是一回事
    /// （有充电上限时永远不充满）。正版副标题的第一优先级就是它。
    pub charged: bool,
    /// 是否接电源（不一定在充电，例如已充满或充电上限）。
    pub plugged: bool,
    /// 系统「低电量模式」是否开着（正版 `isLowPowerMode`，只读不写）。
    pub low_power: bool,
    /// 用电池时的剩余分钟数。
    pub minutes_to_empty: Option<i64>,
    /// 充电时的充满分钟数。
    pub minutes_to_full: Option<i64>,
}

/// 「立即充满电」所需的 PowerUI 读数。
///
/// 系统在通过充电上限（MCL）限充电时，`has_active_limit` 为 true，面板才给出按钮；
/// `override_active` 表示临时解除已经生效，按钮换成「取消立即充满」。
#[derive(Debug, Clone, Serialize)]
pub struct ChargeToFullState {
    /// 这台机器能不能用这套接口。
    pub available: bool,
    /// 读数本身是否可信。
    pub valid: bool,
    pub has_active_limit: bool,
    pub override_active: bool,
    /// 当前生效的上限（80–95），读不到为 None。
    pub limit: Option<i64>,
    pub error: Option<String>,
}

impl ChargeToFullState {
    pub fn unavailable() -> Self {
        Self {
            available: false,
            valid: false,
            has_active_limit: false,
            override_active: false,
            limit: None,
            error: None,
        }
    }
}

impl BatteryState {
    pub fn absent() -> Self {
        Self {
            present: false,
            percent: 0,
            charging: false,
            charged: false,
            plugged: false,
            low_power: false,
            minutes_to_empty: None,
            minutes_to_full: None,
        }
    }
}

/// 电池详情页的读数（正版 `BatteryDetailsReader` 的对应物）。
///
/// 功率单位统一换算成瓦；`Amperage` 的补码负值在读取处已经还原成有符号安培。
#[derive(Debug, Clone, Default, Serialize)]
pub struct BatteryDetails {
    /// 充入电池的功率（W）：接电时为 0 就是「没在充」（充电上限/已充满）。
    pub battery_watts: Option<f64>,
    /// 系统供电（W）：整机实时负载，含 SoC/内存/显示控制器。
    pub system_watts: Option<f64>,
    /// 充电器输入（W）：适配器实际送进来的功率。
    pub input_watts: Option<f64>,
    /// 适配器额定功率（W）：`IOPSCopyExternalPowerAdapterDetails`。
    pub adapter_watts: Option<u32>,
    /// 循环次数（`AppleSmartBattery.CycleCount`）。
    pub cycle_count: Option<u32>,
    /// 电池电压（V）。
    pub voltage_volts: Option<f64>,
    /// 电池电流（A）：放电为负。
    pub current_amps: Option<f64>,
    /// 采样时刻（本机时区 `HH:mm:ss`）。
    pub sampled_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InputSource {
    pub id: String,
    pub name: String,
    pub selected: bool,
    /// 无图标输入源的 1~2 个字标记（正版 `InputSourceShortLabel`）：ABC → A、
    /// 简体拼音 → 拼。读不出来时为 null，前端退回键盘符号。
    pub marker: Option<String>,
}

/// 输入法列表与当前项。
///
/// 读取 TSM 只能在主线程（后台线程调用会 `dispatch_assert_queue` 直接 abort），
/// 所以先单独读好再拼进快照，别的系统读数继续留在后台线程做。
#[derive(Debug, Clone, Default)]
pub struct InputSources {
    pub sources: Vec<InputSource>,
    pub current: Option<String>,
}

/// 一次输入法切换的 HUD 提示。
///
/// 系统只通知「当前输入源变了」，不带给谁变了，所以切换瞬间重新读一遍
/// `TISCopyCurrentKeyboardInputSource`（此时还在主线程，正合 TIS 的要求）。
#[derive(Debug, Clone, Serialize)]
pub struct InputSourceHint {
    pub id: String,
    pub name: String,
    pub marker: Option<String>,
    /// 在可切换列表里的位置（从 1 开始）；读不到序号时为 0，前端不画圆点。
    pub index: usize,
    pub total: usize,
}

/// Wi-Fi / 首选链路状态。
#[derive(Debug, Clone, Serialize)]
pub struct NetworkState {
    /// 机器是否有 Wi-Fi 硬件。
    pub available: bool,
    pub powered: bool,
    pub connected: bool,
    /// macOS 14.4+ 未授权定位时为 None（不是「没有网络」）。
    pub ssid: Option<String>,
    /// dBm，越大越好；未连接时为 None。
    pub rssi: Option<i32>,
    /// 频段显示串（`2.4 GHz` / `5 GHz` / `6 GHz`）：正版副标题夹在 IP 与 RSSI 之间。
    pub band: Option<String>,
    /// Wi‑Fi 名称的授权状态（正版 `WiFiNameAccess`）：`authorized` / `not_determined`
    /// / `denied` / `restricted`。未授权时 `ssid` 一律为 None，副标题要出授权入口。
    pub name_access: String,
    pub interface: Option<String>,
    /// 首选链路的 IPv4。
    pub ipv4: Option<String>,
    /// 首选链路类型：`wifi` / `hotspot` / `sharing` / `temporary` / `ethernet` / `none`。
    /// 后三类靠 Apple 固定的 /28 NAT 网段与 `Temp-` 前缀判定（系统没有公开 API）。
    pub link: String,
}

impl NetworkState {
    pub fn unavailable() -> Self {
        Self {
            available: false,
            powered: false,
            connected: false,
            ssid: None,
            rssi: None,
            band: None,
            name_access: "not_determined".to_string(),
            interface: None,
            ipv4: None,
            link: "none".to_string(),
        }
    }
}

/// 一个扫描到的 AP（正版 `WiFiNetworkCandidate`）。
///
/// 同一 SSID 可能有多个 BSSID，正版把它们按 `(SSID, 安全类型)` 归组，
/// 归组与排序都在前端做（纯函数，可测），这里只交原料。
#[derive(Debug, Clone, Serialize)]
pub struct WifiCandidate {
    /// CoreWLAN 原样返回的 SSID，首尾空格也是身份的一部分（正版注释）。
    pub ssid: String,
    pub bssid: Option<String>,
    /// dBm；非负值按正版视为「读不到」。
    pub rssi: Option<i32>,
    pub channel: Option<i32>,
    /// `CWSecurity` 原始值，正版的 `WiFiSecurityKind` 直接吃它。
    pub security: i32,
}

/// 正版 `WiFiConnectionDetails`：Wi-Fi 二级页「显示详情」的原料。
#[derive(Debug, Clone, Default, Serialize)]
pub struct WifiDetails {
    pub ssid: Option<String>,
    pub bssid: Option<String>,
    pub band: Option<String>,
    pub channel: Option<i32>,
    pub channel_width: Option<String>,
    pub rssi: Option<i32>,
    pub noise: Option<i32>,
    pub phy_mode: Option<String>,
    pub transmit_rate: Option<f64>,
    pub security: i32,
    pub country_code: Option<String>,
    pub interface_name: Option<String>,
    pub ipv4_addresses: Vec<String>,
    pub ipv6_addresses: Vec<String>,
    pub router: Option<String>,
    pub dns_servers: Vec<String>,
}

/// Wi-Fi 二级页的一次读数（正版 `WiFiPanelState` 的数据面）。
#[derive(Debug, Clone, Serialize)]
pub struct WifiScan {
    /// 机器是否有 Wi-Fi 网卡（`list_state = "noInterface"` 时前端显示「没有可用的 Wi-Fi 网卡。」）。
    pub available: bool,
    pub powered: bool,
    pub name_access: String,
    /// `ready` / `poweredOff` / `noInterface` / `failed`：正版 `WiFiListState` 去掉
    /// `scanning`（那由前端自己的请求态表达）与 `permissionDenied`（从
    /// `name_access` + 空列表推，见 `PanelDetailMapper.wifi`）。
    pub list_state: String,
    pub candidates: Vec<WifiCandidate>,
    /// 当前关联的 BSSID：正版用它判定哪一行是「已连接」。
    pub connected_bssid: Option<String>,
    /// `networksetup -listpreferredwirelessnetworks` 的 SSID 列表（正版「已知网络」一节）。
    pub known_ssids: Vec<String>,
    pub details: WifiDetails,
    /// 当前链路是否走 iPhone 热点（正版把已连接的热点行单列一节并画 `personalhotspot`）。
    pub hotspot: bool,
}

impl WifiScan {
    pub fn unavailable() -> Self {
        Self {
            available: false,
            powered: false,
            name_access: "not_determined".to_string(),
            list_state: "noInterface".to_string(),
            candidates: Vec::new(),
            connected_bssid: None,
            known_ssids: Vec::new(),
            details: WifiDetails::default(),
            hotspot: false,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BluetoothDevice {
    pub name: String,
    pub address: Option<String>,
    /// 0 ~ 100；设备不报电量时为 None。
    pub battery: Option<u8>,
    /// 当前是否已连接。已配对的设备也列出来，但用灰色头像区分。
    pub connected: bool,
    /// `device_minorType`（Headphones / Keyboard / Mouse…），前端据此选字形。
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BluetoothState {
    pub available: bool,
    pub powered: bool,
    pub devices: Vec<BluetoothDevice>,
}

impl BluetoothState {
    pub fn unavailable() -> Self {
        Self {
            available: false,
            powered: false,
            devices: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct StatusBarState {
    /// 系统强调色（`#RRGGBB`）：面板的滑条、选中圆底、开启态开关都用它，
    /// 跟随「系统设置 → 外观 → 强调色」，而不是品牌橙。
    pub accent: String,
    pub audio_output: AudioDeviceState,
    pub audio_input: AudioDeviceState,
    pub battery: BatteryState,
    pub input_sources: Vec<InputSource>,
    pub current_input_source: Option<String>,
    pub network: NetworkState,
    pub bluetooth: BluetoothState,
    /// 尚未实现的能力（面板据此显示「即将支持」而不是假开关）。
    pub capabilities: Vec<future::Capability>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioScope {
    Output,
    Input,
}

impl AudioScope {
    /// 前端传的是 `"output"` / `"input"`。
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "output" => Some(Self::Output),
            "input" => Some(Self::Input),
            _ => None,
        }
    }
}

/// 菜单栏图标专用的轻量读数。
///
/// 图标每两秒重画一次，所以这里**不能**走 [`snapshot`]：蓝牙的设备清单靠
/// `system_profiler`，一次要一秒以上，常驻轮询会把 CPU 和耗电都带坏。
/// 控制器开关改用 IOBluetooth 的偏好读取，几微秒返回。
#[derive(Debug, Clone, Serialize)]
pub struct GlyphState {
    pub battery: BatteryState,
    pub network: NetworkState,
    /// 有蓝牙硬件（读到了控制器状态）。
    pub bluetooth_available: bool,
    pub bluetooth_powered: bool,
    pub audio_output: AudioDeviceState,
}

impl GlyphState {
    /// 没有电池/网络/蓝牙/音量时的空读数（非 macOS、以及系统读取全失败时）。
    pub fn empty() -> Self {
        Self {
            battery: BatteryState::absent(),
            network: NetworkState::unavailable(),
            bluetooth_available: false,
            bluetooth_powered: false,
            audio_output: AudioDeviceState::unavailable(),
        }
    }
}

/// 读输入法列表。必须在主线程调用。
pub fn input_sources() -> InputSources {
    platform::input_sources()
}

/// 订阅输入法切换：切换瞬间让菜单栏图标闪一下新输入源的字牌（仅 macOS 有实现）。
///
/// 必须在主线程调用——分布式通知的回调挂在主线程 RunLoop 上，而 TIS 读数本身
/// 也只认主线程，两件事正好落在同一个线程里。
pub fn watch_input_source_changes(app: &tauri::AppHandle) {
    platform::watch_input_source_changes(app);
}

/// 输入法切换后在菜单栏图标上闪一下字牌（`marker` 是「拼」「A」这类 1 字标记）。
pub fn flash_input_source(marker: Option<&str>) {
    tray::flash_input_source(marker);
}

pub fn snapshot(input_sources: InputSources) -> StatusBarState {
    platform::snapshot(input_sources)
}

pub fn glyph_state() -> GlyphState {
    platform::glyph_state()
}

/// 电池详情页读数：只在页面打开时按需读，不进两秒一轮的图标刷新。
pub fn battery_details() -> BatteryDetails {
    platform::battery_details()
}

/// 「立即充满电」的当前读数（PowerUI 私有框架，只读）。
pub fn charge_to_full_state() -> ChargeToFullState {
    platform::charge_to_full_state()
}

/// 临时解除 macOS 的充电上限，直接充到 100%。
pub fn charge_to_full() -> Result<(), String> {
    platform::charge_to_full()
}

/// 取消「立即充满」：恢复发起前记下的上限，返回恢复到的档位。
pub fn cancel_charge_to_full() -> Result<i64, String> {
    platform::cancel_charge_to_full()
}

/// Wi-Fi 二级页读数：全信道扫描要一秒左右，只能在 worker 线程调用。
///
/// 正版同样只在页面打开时扫（`WiFiNetworkController.activate`），菜单栏图标那条
/// 路径只用 `state()` 的轻量读数，两者不互相阻塞。
pub fn wifi_scan() -> WifiScan {
    platform::wifi_scan()
}

/// 面板打开时轮询的音量读数：只碰 CoreAudio，不走 `system_profiler`。
///
/// 正版靠 CoreAudio 属性监听实时跟随键盘音量键；这里用同样的语义、更便宜的
/// 实现（面板可见时每半秒读一次），换来「按键盘上的音量键，滑条立刻跟着动」。
pub fn audio_levels() -> AudioLevels {
    platform::audio_levels()
}

#[derive(Debug, Clone, Serialize)]
pub struct AudioLevels {
    pub output: AudioDeviceState,
    pub input: AudioDeviceState,
}

/// 音量提示音的系统总开关（正版读全局偏好 `com.apple.sound.beep.feedback`）。
pub fn volume_feedback_enabled() -> bool {
    platform::volume_feedback_enabled()
}

/// 菜单栏当前偏暗还是偏亮（彩色图标要用）；非 macOS 或读不到时 None。
///
/// 主线程读到的真实外观优先；还没读到时退回平台探测。
pub fn menu_bar_dark() -> Option<bool> {
    match MENU_BAR_DARK.load(Ordering::Relaxed) {
        1 => Some(false),
        2 => Some(true),
        _ => platform::menu_bar_dark(),
    }
}

/// 由主线程的托盘代码写入菜单栏真实外观；变化时立刻重画图标。
pub fn set_menu_bar_dark(dark: bool) {
    let next = if dark { 2 } else { 1 };
    let previous = MENU_BAR_DARK.swap(next, Ordering::Relaxed);
    if previous != next {
        // 菜单栏颜色没有可自查的 UI：把首次读数与后续切换记下来，
        // 「图标墨色不对」时能直接看日志判断是读数错还是配色没生效。
        tracing::info!(dark, "菜单栏明暗已同步");
        tray::request_refresh();
    }
}

/// 系统强调色（`#RRGGBB`）。
pub fn system_accent() -> String {
    let rgb = SYSTEM_ACCENT.load(Ordering::Relaxed);
    format!(
        "#{:02X}{:02X}{:02X}",
        (rgb >> 16) & 0xFF,
        (rgb >> 8) & 0xFF,
        rgb & 0xFF
    )
}

pub fn set_system_accent(red: u8, green: u8, blue: u8) {
    let rgb = (u32::from(red) << 16) | (u32::from(green) << 8) | u32::from(blue);
    SYSTEM_ACCENT.store(rgb, Ordering::Relaxed);
}

/// 刷新系统强调色缓存。必须在主线程调用。
pub fn cache_system_accent() {
    platform::cache_system_accent()
}

/// 由读数 + 用户开关推出图标规格（纯函数，两平台与单测共用）。
pub fn glyph_spec(state: &GlyphState, options: &glyph::GlyphOptions) -> glyph::GlyphSpec {
    // 「自动」是正版默认：有网络读数就画网络符号，否则退回蓝牙，再否则剪贴板。
    // 点名要某一种时，读不到也照画（用户想看到那个形状，而不是被静默换掉）。
    let center = match options.center {
        glyph::CenterMode::Clipboard => glyph::CenterGlyph::Clipboard,
        glyph::CenterMode::Bluetooth => glyph::CenterGlyph::Bluetooth {
            powered: state.bluetooth_powered,
        },
        glyph::CenterMode::Wifi | glyph::CenterMode::Auto => {
            if state.network.available || options.center == glyph::CenterMode::Wifi {
                network_center(state, options)
            } else if state.bluetooth_available {
                glyph::CenterGlyph::Bluetooth {
                    powered: state.bluetooth_powered,
                }
            } else {
                glyph::CenterGlyph::Clipboard
            }
        }
    };

    let volume =
        if options.volume && state.audio_output.available && state.audio_output.volume_available {
            // 静音在两种画法里都是「空」：点全暗、弧不画。
            Some(if state.audio_output.muted {
                0.0
            } else {
                state.audio_output.volume
            })
        } else {
            None
        };

    let battery = (options.battery_ring && state.battery.present).then_some(state.battery.percent);
    // 数字住在环顶缺口里，没有环就无处安放。
    let percent_label = options.battery_percent.then_some(battery).flatten();

    glyph::GlyphSpec {
        battery,
        charging: state.battery.charging,
        plugged: state.battery.plugged,
        low_power: state.battery.low_power,
        percent_label,
        center,
        volume,
        volume_style: options.volume_style,
    }
}

/// 网络符号：热点/有线/临时/共享各有专属字形，用户逐项勾选后才退回普通 Wi-Fi 弧。
fn network_center(state: &GlyphState, options: &glyph::GlyphOptions) -> glyph::CenterGlyph {
    // 用户点名要 Wi-Fi 弧（center=Wifi）时不看链路类型。
    let by_link = options.center == glyph::CenterMode::Auto;
    let wifi = || glyph::CenterGlyph::Wifi {
        bars: glyph::wifi_bars(state.network.rssi),
        powered: state.network.powered,
        connected: state.network.connected,
    };
    if !by_link {
        return wifi();
    }
    match state.network.link.as_str() {
        "hotspot" if !options.wifi_for_hotspot => glyph::CenterGlyph::Hotspot {
            powered: state.network.powered,
            connected: state.network.connected,
        },
        "ethernet" if !options.wifi_for_ethernet => {
            glyph::CenterGlyph::Ethernet { connected: true }
        }
        "temporary" if !options.wifi_for_temporary => glyph::CenterGlyph::Temporary {
            connected: state.network.connected,
        },
        "sharing" if !options.wifi_for_sharing => glyph::CenterGlyph::Shared {
            connected: state.network.connected,
        },
        _ => wifi(),
    }
}

/// 悬停提示：把图标里三区读出来的数值写成一句话。
pub fn glyph_tooltip(state: &GlyphState, spec: &glyph::GlyphSpec) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(percent) = spec.battery {
        let mut label = format!("电量 {percent}%");
        if spec.charging {
            label.push_str(" 充电中");
        } else if spec.plugged {
            label.push_str(" 已接电源");
        }
        if spec.low_power {
            label.push_str(" · 低电量模式");
        }
        parts.push(label);
    }
    match spec.center {
        glyph::CenterGlyph::Wifi {
            bars,
            powered,
            connected,
        } => {
            if !powered {
                parts.push("Wi-Fi 关闭".to_string());
            } else if !connected {
                parts.push("Wi-Fi 未连接".to_string());
            } else {
                // 互联网共享、临时连接仍画 Wi-Fi 弧，但提示里要说清是哪一种。
                let kind = match state.network.link.as_str() {
                    "sharing" => "互联网共享",
                    "temporary" => "临时网络",
                    _ => "Wi-Fi",
                };
                let signal = state
                    .network
                    .rssi
                    .map(|rssi| format!("（{rssi}dBm）"))
                    .unwrap_or_default();
                parts.push(format!("{kind} {bars} 格{signal}"));
            }
        }
        glyph::CenterGlyph::Hotspot { connected, .. } => parts.push(
            if connected {
                "iPhone 热点"
            } else {
                "热点未连接"
            }
            .to_string(),
        ),
        glyph::CenterGlyph::Ethernet { connected } => parts.push(
            if connected {
                "有线网络"
            } else {
                "有线未连接"
            }
            .to_string(),
        ),
        glyph::CenterGlyph::Temporary { connected } => parts.push(
            if connected {
                "临时网络"
            } else {
                "临时网络未连接"
            }
            .to_string(),
        ),
        glyph::CenterGlyph::Shared { connected } => parts.push(
            if connected {
                "互联网共享"
            } else {
                "互联网共享未连接"
            }
            .to_string(),
        ),
        glyph::CenterGlyph::Bluetooth { powered } => {
            parts.push(if powered { "蓝牙开" } else { "蓝牙关" }.to_string())
        }
        glyph::CenterGlyph::Clipboard => {}
    }
    if let Some(steps) = spec.volume_steps() {
        parts.push(if steps == 0 {
            "已静音".to_string()
        } else {
            format!("音量 {steps}/4")
        });
    }
    if parts.is_empty() {
        return "Halo".to_string();
    }
    format!("Halo · {}", parts.join(" · "))
}

pub fn set_volume(scope: AudioScope, volume: f64) -> Result<(), String> {
    platform::set_volume(scope, volume)
}

pub fn set_muted(scope: AudioScope, muted: bool) -> Result<(), String> {
    platform::set_muted(scope, muted)
}

pub fn set_default_audio(scope: AudioScope, device_id: u32) -> Result<(), String> {
    platform::set_default_audio(scope, device_id)
}

pub fn select_input_source(id: &str) -> Result<(), String> {
    platform::select_input_source(id)
}

pub fn set_wifi(enabled: bool) -> Result<(), String> {
    platform::set_wifi(enabled)
}

pub fn set_bluetooth(enabled: bool) -> Result<(), String> {
    platform::set_bluetooth(enabled)
}

/// 申请定位权限（Wi-Fi 名称读取）。返回正版的三态结果；非 macOS 时是 no-op。
pub fn request_location_permission() -> &'static str {
    platform::request_location_permission()
}

/// 定位授权的当前状态（Wi-Fi 名称读取）。
///
/// 取值：`authorized` / `not_determined` / `denied` / `restricted`。
/// 非 macOS 没有 Wi-Fi 名称这回事，一律按 `authorized` 报（等价于「不需要」）。
pub fn wifi_name_access() -> &'static str {
    platform::wifi_name_access()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn audio(volume: f64, muted: bool, volume_available: bool) -> AudioDeviceState {
        AudioDeviceState {
            available: true,
            device_id: 1,
            device_name: "扬声器".to_string(),
            volume,
            volume_available,
            muted,
            mute_available: true,
            devices: Vec::new(),
        }
    }

    fn state(battery_present: bool, wifi: bool, bluetooth: Option<bool>) -> GlyphState {
        GlyphState {
            battery: BatteryState {
                present: battery_present,
                percent: 66,
                charging: false,
                charged: false,
                plugged: false,
                low_power: false,
                minutes_to_empty: None,
                minutes_to_full: None,
            },
            network: NetworkState {
                available: wifi,
                powered: wifi,
                connected: wifi,
                ssid: wifi.then(|| "Office".to_string()),
                rssi: wifi.then_some(-52),
                band: wifi.then(|| "5 GHz".to_string()),
                name_access: "authorized".to_string(),
                interface: wifi.then(|| "en0".to_string()),
                ipv4: wifi.then(|| "10.0.0.2".to_string()),
                link: if wifi { "wifi" } else { "none" }.to_string(),
            },
            bluetooth_available: bluetooth.is_some(),
            bluetooth_powered: bluetooth.unwrap_or(false),
            audio_output: audio(0.6, false, true),
        }
    }

    #[test]
    fn audio_scope_parses_only_known_values() {
        assert_eq!(AudioScope::parse("output"), Some(AudioScope::Output));
        assert_eq!(AudioScope::parse("input"), Some(AudioScope::Input));
        assert_eq!(AudioScope::parse("mic"), None);
        assert_eq!(AudioScope::parse(""), None);
    }

    #[test]
    fn auto_center_prefers_wifi_then_bluetooth_then_clipboard() {
        let options = glyph::GlyphOptions::default();
        assert_eq!(
            glyph_spec(&state(true, true, Some(true)), &options).center,
            glyph::CenterGlyph::Wifi {
                bars: 3,
                powered: true,
                connected: true
            }
        );
        assert_eq!(
            glyph_spec(&state(true, false, Some(true)), &options).center,
            glyph::CenterGlyph::Bluetooth { powered: true }
        );
        assert_eq!(
            glyph_spec(&state(true, false, None), &options).center,
            glyph::CenterGlyph::Clipboard
        );
    }

    #[test]
    fn link_type_switches_the_center_mark() {
        let options = glyph::GlyphOptions::default();
        let mut hotspot = state(true, true, None);
        hotspot.network.ipv4 = Some("172.20.10.2".to_string());
        hotspot.network.link = "hotspot".to_string();
        assert_eq!(
            glyph_spec(&hotspot, &options).center,
            glyph::CenterGlyph::Hotspot {
                powered: true,
                connected: true
            }
        );

        let mut wired = state(true, true, None);
        wired.network.link = "ethernet".to_string();
        assert_eq!(
            glyph_spec(&wired, &options).center,
            glyph::CenterGlyph::Ethernet { connected: true }
        );

        // 共享/临时连接同样有专用字形：实心扇形再抠掉一块。
        let mut sharing = state(true, true, None);
        sharing.network.link = "sharing".to_string();
        assert_eq!(
            glyph_spec(&sharing, &options).center,
            glyph::CenterGlyph::Shared { connected: true }
        );
        let mut temporary = state(true, true, None);
        temporary.network.link = "temporary".to_string();
        assert_eq!(
            glyph_spec(&temporary, &options).center,
            glyph::CenterGlyph::Temporary { connected: true }
        );

        let wifi_arc = glyph::CenterGlyph::Wifi {
            bars: 3,
            powered: true,
            connected: true,
        };

        // 「连接图标样式」按链路逐项退回 Wi-Fi 弧，勾了哪种只影响哪种。
        let back_to_wifi = |flag: &str| {
            let mut o = glyph::GlyphOptions::default();
            match flag {
                "hotspot" => o.wifi_for_hotspot = true,
                "ethernet" => o.wifi_for_ethernet = true,
                "temporary" => o.wifi_for_temporary = true,
                _ => o.wifi_for_sharing = true,
            }
            o
        };
        let cases = [
            ("hotspot", &hotspot),
            ("ethernet", &wired),
            ("temporary", &temporary),
            ("sharing", &sharing),
        ];
        for (link, sample) in cases {
            assert_eq!(
                glyph_spec(sample, &back_to_wifi(link)).center,
                wifi_arc,
                "{link}"
            );
            // 别的链路不受影响。
            for (other, other_sample) in cases.iter().filter(|(l, _)| *l != link) {
                assert_eq!(
                    glyph_spec(other_sample, &back_to_wifi(link)).center,
                    glyph_spec(other_sample, &options).center,
                    "{link} 不该动 {other}"
                );
            }
        }

        // 点名要 Wi-Fi 弧时不看链路类型。
        let forced = glyph::GlyphOptions {
            center: glyph::CenterMode::Wifi,
            ..glyph::GlyphOptions::default()
        };
        assert_eq!(glyph_spec(&hotspot, &forced).center, wifi_arc);
    }

    #[test]
    fn forced_center_ignores_hardware_availability() {
        let options = glyph::GlyphOptions {
            center: glyph::CenterMode::Bluetooth,
            ..glyph::GlyphOptions::default()
        };
        assert_eq!(
            glyph_spec(&state(true, true, None), &options).center,
            glyph::CenterGlyph::Bluetooth { powered: false }
        );
    }

    #[test]
    fn ring_and_volume_respect_options_and_device_support() {
        let options = glyph::GlyphOptions::default();
        let spec = glyph_spec(&state(true, true, None), &options);
        assert_eq!(spec.battery, Some(66));
        assert_eq!(spec.percent_label, Some(66));
        assert_eq!(spec.volume_steps(), Some(3));

        // 台式机没有电池：不画环。
        let mut desktop = state(false, true, None);
        desktop.audio_output = audio(0.6, false, false);
        let spec = glyph_spec(&desktop, &options);
        assert_eq!(spec.battery, None);
        assert_eq!(spec.percent_label, None);
        assert_eq!(spec.volume, None);

        // 静音：保留音量区，但一格都不亮。
        let mut muted = state(true, true, None);
        muted.audio_output = audio(0.6, true, true);
        assert_eq!(glyph_spec(&muted, &options).volume_steps(), Some(0));

        // 关掉开关就不画。
        let options = glyph::GlyphOptions {
            battery_ring: false,
            volume: false,
            ..glyph::GlyphOptions::default()
        };
        let spec = glyph_spec(&state(true, true, None), &options);
        assert_eq!(spec.battery, None);
        assert_eq!(spec.volume, None);

        // 数字住在环顶缺口里：环关了，不许出现数字。
        let no_label = glyph::GlyphOptions {
            battery_percent: false,
            ..glyph::GlyphOptions::default()
        };
        assert_eq!(
            glyph_spec(&state(true, true, None), &no_label).percent_label,
            None
        );
        let ring_off = glyph::GlyphOptions {
            battery_ring: false,
            ..glyph::GlyphOptions::default()
        };
        assert_eq!(
            glyph_spec(&state(true, true, None), &ring_off).percent_label,
            None
        );
    }

    #[test]
    fn tooltip_reads_out_the_drawn_regions() {
        let options = glyph::GlyphOptions::default();
        let sample = state(true, true, None);
        let spec = glyph_spec(&sample, &options);
        assert_eq!(
            glyph_tooltip(&sample, &spec),
            "Halo · 电量 66% · Wi-Fi 3 格（-52dBm） · 音量 3/4"
        );

        let mut off = state(true, false, Some(false));
        off.audio_output = audio(0.6, true, true);
        let spec = glyph_spec(&off, &options);
        assert_eq!(
            glyph_tooltip(&off, &spec),
            "Halo · 电量 66% · 蓝牙关 · 已静音"
        );

        // 热点与有线：提示里必须点出链路类型，不能一律说 Wi-Fi。
        let mut hotspot = state(true, true, None);
        hotspot.network.link = "hotspot".to_string();
        let spec = glyph_spec(&hotspot, &options);
        assert_eq!(
            glyph_tooltip(&hotspot, &spec),
            "Halo · 电量 66% · iPhone 热点 · 音量 3/4"
        );

        // 互联网共享现在是实心扇形抠箭头，提示也跟着换名字。
        let mut sharing = state(true, true, None);
        sharing.network.link = "sharing".to_string();
        let spec = glyph_spec(&sharing, &options);
        assert_eq!(
            glyph_tooltip(&sharing, &spec),
            "Halo · 电量 66% · 互联网共享 · 音量 3/4"
        );
        let forced_wifi = glyph::GlyphOptions {
            center: glyph::CenterMode::Wifi,
            ..glyph::GlyphOptions::default()
        };
        assert_eq!(
            glyph_tooltip(&sharing, &glyph_spec(&sharing, &forced_wifi)),
            "Halo · 电量 66% · 互联网共享 3 格（-52dBm） · 音量 3/4"
        );
    }
}
