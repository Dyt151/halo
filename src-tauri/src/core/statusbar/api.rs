//! 平台读数与系统控制的薄封装。

use super::*;

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

/// 默认输出音量（0..1），音量反馈轮询用。
pub(crate) fn output_volume() -> Option<f64> {
    platform::output_volume()
}

/// 启动音量反馈：键盘音量键 / 控制中心改音量时逐步响一声。
pub fn start_volume_feedback_watcher() {
    #[cfg(target_os = "macos")]
    volume_feedback::start();
}

/// 面板自己写音量前登记目标值，轮询看到这次变化时不重复响。
pub fn note_self_volume_write(scope: AudioScope, volume: f64) {
    #[cfg(target_os = "macos")]
    if scope == AudioScope::Output {
        volume_feedback::note_self_write(volume);
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (scope, volume);
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

/// 打开输入法自带的设置 App；路径来自快照里的 `settings_app`。
pub fn open_ime_settings(path: &str) -> Result<(), String> {
    platform::open_ime_settings(path)
}

pub fn set_wifi(enabled: bool) -> Result<(), String> {
    platform::set_wifi(enabled)
}

pub fn set_bluetooth(enabled: bool) -> Result<(), String> {
    platform::set_bluetooth(enabled)
}

/// VPN 二级页读数：会起几个短命子进程，只能在 worker 线程调用。
pub fn proxy_page() -> ProxyPage {
    platform::proxy_page()
}

/// 面板可见时的网速轮询：一次控制口请求或一次 `netstat`。
pub fn proxy_speed() -> ProxySpeed {
    platform::proxy_speed()
}

/// 流量接管：只改 macOS 系统代理三项，写前快照、写后回读、不一致就回滚。
pub fn proxy_set_takeover(enabled: bool) -> ProxyOutcome {
    platform::proxy_set_takeover(enabled)
}

pub fn proxy_set_mode(mode: &str) -> ProxyOutcome {
    platform::proxy_set_mode(mode)
}

pub fn proxy_select_node(group: &str, node: &str) -> ProxyOutcome {
    platform::proxy_select_node(group, node)
}

/// 一轮节点延迟测试，最坏十几秒；前端在这段时间里显示转圈。
pub fn proxy_latency(group: &str) -> Vec<ProxyNode> {
    platform::proxy_latency(group)
}

pub fn proxy_open_app() -> ProxyOutcome {
    platform::proxy_open_app()
}

/// 接管状态的观察线程：只在「已接管」时确认端口还有没有人听。
pub fn start_proxy_watcher() {
    platform::start_proxy_watcher()
}

pub fn autoswitch_config() -> AutoSwitchConfig {
    platform::autoswitch_config()
}

pub fn autoswitch_set_config(config: AutoSwitchConfig) -> ProxyOutcome {
    platform::autoswitch_set_config(config)
}

pub fn autoswitch_set_router_secret(secret: String) -> ProxyOutcome {
    platform::autoswitch_set_router_secret(secret)
}

pub fn autoswitch_state() -> AutoSwitchState {
    platform::autoswitch_state()
}

pub fn autoswitch_check_now() -> AutoSwitchState {
    platform::autoswitch_check_now()
}

/// 自动换节点巡检线程：测速发现卡死就在 2 分钟内换节点。
pub fn start_autoswitch_worker() {
    platform::start_autoswitch_worker()
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
