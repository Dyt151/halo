//! macOS 实现：电池 / 音量 / 输入法走公开框架，网络 / 蓝牙见各模块注释。

mod audio;
mod autoswitch;
mod battery;
mod bluetooth;
mod input_source;
mod network;
mod powerui;
mod proxy;
mod smc;

use super::cf;
use super::future;
use super::{
    AudioLevels, AudioScope, BatteryDetails, GlyphState, InputSources, ProxyGroup, ProxyNode,
    ProxyOutcome, ProxyPage, ProxySpeed, StatusBarState, WifiScan,
};

/// 读输入法列表。TSM 只能在主线程调用，由命令层负责切回主线程。
pub fn input_sources() -> InputSources {
    input_source::state()
}

/// 打开输入法自带的设置 App（微信输入法的 `WeTypeSettings.app`）。
pub fn open_ime_settings(path: &str) -> Result<(), String> {
    input_source::open_settings_app(path)
}

pub fn snapshot(sources: InputSources) -> StatusBarState {
    StatusBarState {
        accent: super::system_accent(),
        audio_output: audio::output_state(),
        audio_input: audio::input_state(),
        battery: battery::state(),
        current_input_source: sources.current,
        input_sources: sources.sources,
        network: network::state(),
        bluetooth: bluetooth::state(),
        capabilities: future::capabilities(),
    }
}

/// 图标重画用的读数：跳过 `system_profiler`，蓝牙只读控制器开关。
pub fn glyph_state() -> GlyphState {
    let powered = bluetooth::controller_powered();
    GlyphState {
        battery: battery::state(),
        network: network::state(),
        bluetooth_available: powered.is_some(),
        bluetooth_powered: powered.unwrap_or(false),
        audio_output: audio::output_state(),
        // 代理状态只读缓存：现读要起三个 networksetup，图标每两秒就重画一次。
        proxy: proxy::bar_state(),
    }
}

/// 接管状态的观察线程（只在 macOS 有实体；重复调用只起一条）。
pub fn start_proxy_watcher() {
    proxy::spawn_watcher();
}

pub fn autoswitch_config() -> super::AutoSwitchConfig {
    autoswitch::config()
}

pub fn autoswitch_set_config(config: super::AutoSwitchConfig) -> ProxyOutcome {
    autoswitch::set_config(config)
}

pub fn autoswitch_set_router_secret(secret: String) -> ProxyOutcome {
    autoswitch::set_router_secret(secret)
}

pub fn autoswitch_state() -> super::AutoSwitchState {
    autoswitch::state()
}

pub fn autoswitch_check_now() -> super::AutoSwitchState {
    autoswitch::check_now()
}

/// 自动换节点巡检线程（重复调用只起一条；配置关闭时线程空转）。
pub fn start_autoswitch_worker() {
    autoswitch::spawn_worker();
}

pub fn set_volume(scope: AudioScope, volume: f64) -> Result<(), String> {
    audio::set_volume(scope, volume)
}

/// 默认输出音量（0..1），音量反馈轮询用。
pub fn output_volume() -> Option<f64> {
    audio::output_volume().map(f64::from)
}

/// 详情页读数：注册表 + IOPS，几毫秒，不碰 `system_profiler`。
pub fn battery_details() -> BatteryDetails {
    battery::details()
}

/// 「立即充满电」读数：PowerUI 私有框架，只读，几毫秒。
pub fn charge_to_full_state() -> super::ChargeToFullState {
    powerui::state()
}

pub fn charge_to_full() -> Result<(), String> {
    powerui::charge_to_full()
}

pub fn cancel_charge_to_full() -> Result<i64, String> {
    powerui::cancel_charge_to_full()
}

/// Wi-Fi 二级页读数：含一次全信道扫描（一秒上下），只在页面打开时调用。
pub fn wifi_scan() -> WifiScan {
    network::scan()
}

/// 订阅输入法切换：分布式通知的回调挂在主线程 RunLoop，所以必须在主线程注册。
pub fn watch_input_source_changes(app: &tauri::AppHandle) {
    input_source::watch_changes(app);
}

pub fn audio_levels() -> AudioLevels {
    AudioLevels {
        output: audio::output_state(),
        input: audio::input_state(),
    }
}

/// 正版音量「叮」的系统总开关：全局偏好的 `com.apple.sound.beep.feedback`。
///
/// 键不存在时按「开」处理（正版 `isEnabled(storedValue: nil) == true`），
/// 所以绝大多数机器默认就有提示音；只有关掉了系统声音反馈的用户才安静。
pub fn volume_feedback_enabled() -> bool {
    unsafe {
        let Some(key) = cf::cfstr("com.apple.sound.beep.feedback") else {
            return true;
        };
        let Some(domain) = cf::cfstr(".GlobalPreferences") else {
            cf::release(key);
            return true;
        };
        let value = cf::CFPreferencesCopyAppValue(key, domain);
        let enabled = if value.is_null() {
            true
        } else {
            cf::bool_value(value)
                .or_else(|| cf::i32_value(value).map(|raw| raw != 0))
                .unwrap_or(true)
        };
        cf::release(value);
        cf::release(key);
        cf::release(domain);
        enabled
    }
}

pub fn set_muted(scope: AudioScope, muted: bool) -> Result<(), String> {
    audio::set_muted(scope, muted)
}

pub fn set_default_audio(scope: AudioScope, device_id: u32) -> Result<(), String> {
    audio::set_default(scope, device_id)
}

pub fn select_input_source(id: &str) -> Result<(), String> {
    input_source::select(id)
}

pub fn set_wifi(enabled: bool) -> Result<(), String> {
    network::set_power(enabled)
}

pub fn set_bluetooth(enabled: bool) -> Result<(), String> {
    bluetooth::set_power(enabled)
}

/// VPN 二级页读数：起几个短命子进程，只在页面打开和点刷新时调用。
pub fn proxy_page() -> super::ProxyPage {
    proxy::page()
}

/// 网速轮询：面板可见时两秒一次，比 [`proxy_page`] 便宜得多。
pub fn proxy_speed() -> super::ProxySpeed {
    proxy::speed()
}

pub fn proxy_set_takeover(enabled: bool) -> super::ProxyOutcome {
    proxy::set_takeover(enabled)
}

pub fn proxy_set_mode(mode: &str) -> super::ProxyOutcome {
    proxy::set_mode(mode)
}

pub fn proxy_select_node(group: &str, node: &str) -> super::ProxyOutcome {
    proxy::select_node(group, node)
}

/// 测一个组里全部节点的延迟，前端在此期间显示转圈。
pub fn proxy_latency(group: &str) -> Vec<super::ProxyNode> {
    proxy::latency(group)
}

/// 把 FlClash 的主界面带到前台（核心没跑、或要改订阅时用户自己操作）。
pub fn proxy_open_app() -> super::ProxyOutcome {
    proxy::open_app()
}

/// 申请 Wi‑Fi 名称授权：返回正版的三态结果，前端据此决定要不要转去系统设置。
pub fn request_location_permission() -> &'static str {
    network::request_name_permission()
}

pub fn wifi_name_access() -> &'static str {
    network::name_access().key()
}

/// 读系统强调色（`NSColor.controlAccentColor`）写进缓存。
///
/// 必须在主线程调用：`controlAccentColor` 会走 AppKit 的外观解析。先转换到
/// sRGB 再取分量，否则在 P3 等色彩空间里拿到的值会偏。
pub fn cache_system_accent() {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;

    unsafe {
        let color: *mut AnyObject = msg_send![objc2::class!(NSColor), controlAccentColor];
        if color.is_null() {
            return;
        }
        let space: *mut AnyObject = msg_send![objc2::class!(NSColorSpace), sRGBColorSpace];
        if space.is_null() {
            return;
        }
        let converted: *mut AnyObject = msg_send![color, colorUsingColorSpace: space];
        if converted.is_null() {
            return;
        }
        let red: f64 = msg_send![converted, redComponent];
        let green: f64 = msg_send![converted, greenComponent];
        let blue: f64 = msg_send![converted, blueComponent];
        let channel = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        super::set_system_accent(channel(red), channel(green), channel(blue));
    }
}

/// 菜单栏是深色还是浅色（彩色图标要用它选墨色）。
///
/// 主线程从 `NSStatusBarButton.effectiveAppearance` 读到后由缓存接管（见
/// `statusbar::set_menu_bar_dark`）；这里只是缓存还没建立时的退路。菜单栏的真实
/// 明暗跟着壁纸走，`AppleInterfaceTheme` 只代表系统外观偏好，两者经常对不上。
pub fn menu_bar_dark() -> Option<bool> {
    unsafe {
        let key = cf::cfstr("AppleInterfaceTheme")?;
        let domain = cf::cfstr("GlobalPreferences")?;
        let value = cf::CFPreferencesCopyAppValue(key, domain);
        let theme = cf::string(value);
        if !value.is_null() {
            cf::release(value);
        }
        cf::release(key);
        cf::release(domain);
        // 键不存在 = 浅色（macOS 默认外观不写这个键）。
        Some(theme?.eq_ignore_ascii_case("Dark"))
    }
}
