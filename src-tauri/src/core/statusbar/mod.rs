//! 状态栏面板的系统控制后端。
//!
//! 已接入：电池、音量、声音输入、输入法（第一批）与网络、蓝牙（第二批）。
//! VPN 与私有能力（AirPods 降噪、充电上限、iPhone 电量）只在 [`future`]
//! 里留接口，不实现。

pub mod future;
pub mod glyph;
pub mod tray;

#[cfg(target_os = "macos")]
pub(crate) mod cf;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
mod volume_feedback;
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
    /// 输入法自带的设置 App 路径（微信输入法 → `WeTypeSettings.app`）。
    /// Apple 自带的输入法没有这种东西，为 null。
    pub settings_app: Option<String>,
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

/// 代理（VPN）详情页里的一个策略组（Clash `/proxies` 的一项）。
#[derive(Debug, Clone, Serialize)]
pub struct ProxyGroup {
    pub name: String,
    /// `select` / `url-test` / `fallback` / `load-balance`…
    pub kind: String,
    pub current: Option<String>,
    /// Clash 的 `hidden`：策略组本身不在菜单里出现。
    pub hidden: bool,
    /// 组自己的最近一次延迟（毫秒），来自 `/proxies` 的 `history`。
    pub delay: Option<u32>,
    pub nodes: Vec<ProxyNode>,
}

/// 策略组里的一个成员。信息行（「剩余流量…」）也是成员，由前端按名字区分画法。
#[derive(Debug, Clone, Serialize)]
pub struct ProxyNode {
    pub name: String,
    pub kind: String,
    pub delay: Option<u32>,
}

/// 代理详情页一次读数。
///
/// `source` 决定页面能给到什么：`clash` 有控制口（模式/节点/延迟全能控），
/// `probe` 只探到端口（只能开关接管），`none` 什么都没找到（`available=false`，
/// 整行不显示）。核心已退但系统代理还挂着时是 `available=true` + `none`：
/// 这种「失效的接管」必须留在页面上，否则用户没法把它关掉。
#[derive(Debug, Clone, Serialize)]
pub struct ProxyPage {
    pub available: bool,
    pub source: String,
    /// mixed-port 能连上 = 核心在跑。
    pub core_running: bool,
    pub mixed_port: u16,
    /// 实际控制口地址（`127.0.0.1:9097`）；没有则空串。
    pub controller: String,
    /// 系统代理三样里有没有任一项开着且指向本机。
    pub takeover: bool,
    pub takeover_web: bool,
    pub takeover_https: bool,
    pub takeover_socks: bool,
    /// 系统代理实际指向的本机端口；没接管时 0。核心换过端口后它和 `mixed_port` 可以不同。
    pub proxy_port: u16,
    /// `proxy_port` 上有没有人听。接管开着而这里是 `false`，网页就打不开——
    /// 判定只看系统代理实际指向的端口，不看配置里的混合端口。
    pub takeover_alive: bool,
    /// `networksetup` 用的网络服务名（如 `Wi-Fi`）；空串 = 认不出当前服务。
    pub service: String,
    pub mode: Option<String>,
    pub groups: Vec<ProxyGroup>,
    /// 虚拟网卡（TUN）状态：只读，第一版不给开关。
    pub tun_enabled: Option<bool>,
    pub up_bps: Option<u64>,
    pub down_bps: Option<u64>,
    /// `proxy` = 代理流量（控制口），`interface` = 本机网卡计数。
    pub speed_scope: String,
    pub test_url: String,
    /// 可直接展示的中文说明；正常时为空串。绝不包含订阅密钥。
    pub message: String,
}

impl ProxyPage {
    pub fn unavailable() -> Self {
        Self {
            available: false,
            source: "none".to_string(),
            core_running: false,
            mixed_port: 0,
            controller: String::new(),
            takeover: false,
            takeover_web: false,
            takeover_https: false,
            takeover_socks: false,
            proxy_port: 0,
            takeover_alive: false,
            service: String::new(),
            mode: None,
            groups: Vec::new(),
            tun_enabled: None,
            up_bps: None,
            down_bps: None,
            speed_scope: "interface".to_string(),
            test_url: String::new(),
            message: String::new(),
        }
    }
}

/// 网速轮询：面板可见时每两秒一次，只做一次读数与差值。
#[derive(Debug, Clone, Serialize)]
pub struct ProxySpeed {
    pub up_bps: Option<u64>,
    pub down_bps: Option<u64>,
    pub speed_scope: String,
}

impl ProxySpeed {
    pub fn unavailable() -> Self {
        Self {
            up_bps: None,
            down_bps: None,
            speed_scope: "interface".to_string(),
        }
    }
}

/// 代理写操作的结果。`code`：`ok` / `failed` / `unsupported`。
#[derive(Debug, Clone, Serialize)]
pub struct ProxyOutcome {
    pub code: String,
    pub message: String,
}

impl ProxyOutcome {
    pub fn ok() -> Self {
        Self {
            code: "ok".to_string(),
            message: String::new(),
        }
    }

    pub fn failed(message: impl Into<String>) -> Self {
        Self {
            code: "failed".to_string(),
            message: message.into(),
        }
    }

    /// 平台不支持：前端按提示处理，不当成一次操作失败。
    pub fn unsupported() -> Self {
        Self {
            code: "unsupported".to_string(),
            message: "系统控制仅支持 macOS".to_string(),
        }
    }
}

/// 自动换节点配置（存 `~/.halo/autoswitch.json`，路由器密钥另存钥匙串）。
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct AutoSwitchConfig {
    #[serde(default)]
    pub enabled: bool,
    /// 路由器 OpenClash 里承载主流量的 Selector 组名。
    #[serde(default = "default_autoswitch_group")]
    pub router_group: String,
    /// 本机 FlClash 里承载主流量的 Selector 组名。
    #[serde(default = "default_autoswitch_group")]
    pub local_group: String,
    /// 低于这个速度（KB/s）算不达标。
    #[serde(default = "default_autoswitch_min_speed")]
    pub min_speed_kbps: u32,
    /// 巡检间隔（秒）。
    #[serde(default = "default_autoswitch_interval")]
    pub probe_interval_secs: u32,
    /// 每天最多自动切换次数（防 IP 频繁变化）。
    #[serde(default = "default_autoswitch_cap")]
    pub daily_cap: u32,
    /// 只读标记：钥匙串里是否已存路由器 API 密钥。
    #[serde(default)]
    pub has_router_secret: bool,
}

fn default_autoswitch_group() -> String {
    "良心云".to_string()
}

fn default_autoswitch_min_speed() -> u32 {
    300
}

fn default_autoswitch_interval() -> u32 {
    30
}

fn default_autoswitch_cap() -> u32 {
    12
}

impl Default for AutoSwitchConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            router_group: default_autoswitch_group(),
            local_group: default_autoswitch_group(),
            min_speed_kbps: default_autoswitch_min_speed(),
            probe_interval_secs: default_autoswitch_interval(),
            daily_cap: default_autoswitch_cap(),
            has_router_secret: false,
        }
    }
}

/// 自动换节点的一次读数（前端卡片直接渲染）。
#[derive(Debug, Clone, Serialize)]
pub struct AutoSwitchState {
    pub config: AutoSwitchConfig,
    /// `router` / `local` / `none`。
    pub backend: String,
    pub backend_label: String,
    pub current_node: String,
    pub last_speed_kbps: Option<u32>,
    /// Unix 秒；下同。
    pub last_probe_at: Option<u64>,
    pub last_switch_at: Option<u64>,
    pub last_switch_from: String,
    pub last_switch_to: String,
    pub switches_today: u32,
    pub consecutive_bad: u32,
    /// 可直接展示的中文一句话。
    pub status: String,
    /// 最近 20 条决策日志（新→旧）。
    pub log: Vec<String>,
}

impl AutoSwitchState {
    pub fn unavailable() -> Self {
        Self {
            config: AutoSwitchConfig::default(),
            backend: "none".to_string(),
            backend_label: "系统控制仅支持 macOS".to_string(),
            current_node: String::new(),
            last_speed_kbps: None,
            last_probe_at: None,
            last_switch_at: None,
            last_switch_from: String::new(),
            last_switch_to: String::new(),
            switches_today: 0,
            consecutive_bad: 0,
            status: String::new(),
            log: Vec::new(),
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

/// 菜单栏图标上的代理（VPN）状态。
///
/// 值来自缓存而不是现读：判定接管要问 `networksetup`（三个子进程），而图标每两秒
/// 就重画一次。缓存由代理页读数、接管写入和一个只看端口的观察线程一起维护。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub enum ProxyBar {
    /// 还没读过（开机后没打开过代理页）。图标不提代理。
    #[default]
    Unknown,
    /// 读过，且系统代理没开。这是默认状态，也不必占提示语。
    Off,
    On,
    /// 接管还挂着，但那个端口上已经没有核心了：提示用户去关掉，否则网页打不开。
    Stale,
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
    pub proxy: ProxyBar,
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
            proxy: ProxyBar::default(),
        }
    }
}


mod api;
mod spec;

pub use api::*;
pub use spec::*;

#[cfg(test)]
mod tests;
