//! 非 macOS 平台：状态栏面板不提供系统控制（窗口本身仍然可用）。

use super::future;
use super::{
    AudioDeviceState, AudioLevels, AudioScope, BatteryDetails, BatteryState, BluetoothState,
    GlyphState, InputSources, NetworkState, StatusBarState, WifiScan,
};

pub fn input_sources() -> InputSources {
    InputSources::default()
}

/// 非 macOS 没有 TSM：不订阅，HUD 窗口永远不会被唤起。
pub fn watch_input_source_changes(_app: &tauri::AppHandle) {}

pub fn snapshot(_sources: InputSources) -> StatusBarState {
    StatusBarState {
        accent: "#007AFF".to_string(),
        audio_output: AudioDeviceState::unavailable(),
        audio_input: AudioDeviceState::unavailable(),
        battery: BatteryState::absent(),
        input_sources: Vec::new(),
        current_input_source: None,
        network: NetworkState::unavailable(),
        bluetooth: BluetoothState::unavailable(),
        capabilities: future::capabilities(),
    }
}

pub fn glyph_state() -> GlyphState {
    GlyphState::empty()
}

pub fn battery_details() -> BatteryDetails {
    BatteryDetails::default()
}

/// 非 macOS 没有 Wi-Fi 扫描：前端拿到 `noInterface` 就显示「没有可用的 Wi-Fi 网卡。」
pub fn wifi_scan() -> WifiScan {
    WifiScan::unavailable()
}

pub fn audio_levels() -> AudioLevels {
    AudioLevels {
        output: AudioDeviceState::unavailable(),
        input: AudioDeviceState::unavailable(),
    }
}

pub fn volume_feedback_enabled() -> bool {
    false
}

pub fn set_volume(_scope: AudioScope, _volume: f64) -> Result<(), String> {
    Err("系统控制仅支持 macOS".to_string())
}

pub fn set_muted(_scope: AudioScope, _muted: bool) -> Result<(), String> {
    Err("系统控制仅支持 macOS".to_string())
}

pub fn set_default_audio(_scope: AudioScope, _device_id: u32) -> Result<(), String> {
    Err("系统控制仅支持 macOS".to_string())
}

pub fn select_input_source(_id: &str) -> Result<(), String> {
    Err("系统控制仅支持 macOS".to_string())
}

pub fn set_wifi(_enabled: bool) -> Result<(), String> {
    Err("系统控制仅支持 macOS".to_string())
}

pub fn set_bluetooth(_enabled: bool) -> Result<(), String> {
    Err("系统控制仅支持 macOS".to_string())
}

pub fn charge_to_full_state() -> super::ChargeToFullState {
    super::ChargeToFullState::unavailable()
}

pub fn charge_to_full() -> Result<(), String> {
    Err("系统控制仅支持 macOS".to_string())
}

pub fn cancel_charge_to_full() -> Result<i64, String> {
    Err("系统控制仅支持 macOS".to_string())
}

pub fn request_location_permission() -> &'static str {
    "not_needed"
}

/// 非 macOS 不读 Wi-Fi 名称，等价于「无需授权」。
pub fn wifi_name_access() -> &'static str {
    "authorized"
}

/// 没有菜单栏外观可读（托盘退回模板图，颜色不参与）。
pub fn menu_bar_dark() -> Option<bool> {
    None
}

/// 非 macOS 没有系统强调色可读，保持默认蓝。
pub fn cache_system_accent() {}
