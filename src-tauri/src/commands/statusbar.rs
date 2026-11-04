//! 状态栏图标面板的命令：系统控制读写 + 窗口交互。

use tauri::{AppHandle, Manager};

use crate::core::statusbar::{self, AudioScope};
use crate::error::{AppError, CommandResult};
use crate::state::AppState;
use crate::window;

fn parse_scope(raw: &str) -> CommandResult<AudioScope> {
    AudioScope::parse(raw).ok_or_else(|| AppError::new(9000, "音频方向参数不合法"))
}

fn system_error(message: String) -> AppError {
    AppError::new(9000, message)
}

/// 系统状态被我们改动了：让菜单栏图标立刻重画，而不是等下一个轮询周期。
fn after_change<T>(result: Result<T, AppError>) -> Result<T, AppError> {
    statusbar::tray::request_refresh();
    result
}

/// 面板一次拉齐：输出/输入音量、电池、输入法、未实现能力清单。
///
/// 必须是 async：同步命令跑在主线程，而这里要读蓝牙控制器开关
/// （`IOBluetoothPreferenceGetControllerPowerState` 首次调用会等一个只能由主线程
/// 派的 CoreBluetooth 回调 = 自锁）外加一次 `system_profiler`（一秒以上）。
///
/// 但输入法这一项反过来：TSM 的 `TISGetInputSourceProperty` 只能在主线程调用，
/// 在 tokio worker 上会 `dispatch_assert_queue` 直接 abort（打开面板即崩）。所以
/// 先把这一小块丢回主线程读好，其余读数继续留在 worker。
#[tauri::command]
pub async fn statusbar_state(app: AppHandle) -> statusbar::StatusBarState {
    let (tx, rx) = std::sync::mpsc::channel();
    let sources = match app.run_on_main_thread(move || {
        let _ = tx.send(statusbar::input_sources());
    }) {
        Ok(()) => tauri::async_runtime::spawn_blocking(move || {
            rx.recv_timeout(std::time::Duration::from_secs(1))
                .unwrap_or_default()
        })
        .await
        .unwrap_or_default(),
        Err(_) => statusbar::InputSources::default(),
    };
    statusbar::snapshot(sources)
}

/// 设置页的菜单栏图标预览（轻量读数，不走 `system_profiler`）。
///
/// 用 `AppHandle` 而不是 `State<'_, AppState>` 取配置：async 命令的 future 必须
/// `'static`，借用的 State 过不了这条约束。
#[tauri::command]
pub async fn statusbar_glyph_preview(app: AppHandle) -> statusbar::tray::Preview {
    let options = app.state::<AppState>().config.get().statusbar.glyph_options();
    statusbar::tray::preview(&options)
}

#[tauri::command]
pub fn statusbar_set_volume(scope: String, volume: f64) -> CommandResult<()> {
    let scope = parse_scope(&scope)?;
    after_change(statusbar::set_volume(scope, volume).map_err(system_error))
}

#[tauri::command]
pub fn statusbar_set_mute(scope: String, muted: bool) -> CommandResult<()> {
    let scope = parse_scope(&scope)?;
    after_change(statusbar::set_muted(scope, muted).map_err(system_error))
}

#[tauri::command]
pub fn statusbar_set_default_device(scope: String, device_id: u32) -> CommandResult<()> {
    let scope = parse_scope(&scope)?;
    after_change(statusbar::set_default_audio(scope, device_id).map_err(system_error))
}

#[tauri::command]
pub fn statusbar_select_input_source(id: String) -> CommandResult<()> {
    statusbar::select_input_source(&id).map_err(system_error)
}

/// Wi-Fi 开关。
#[tauri::command]
pub fn statusbar_set_wifi(enabled: bool) -> CommandResult<()> {
    after_change(statusbar::set_wifi(enabled).map_err(system_error))
}

/// 蓝牙开关。同样避开主线程：这是 IOBluetooth 调用，和上面那条读数共用一套
/// 框架内部的一次性初始化。
#[tauri::command]
pub async fn statusbar_set_bluetooth(enabled: bool) -> CommandResult<()> {
    after_change(statusbar::set_bluetooth(enabled).map_err(system_error))
}

/// 申请定位权限：macOS 14.4+ 只有授权后才返回 Wi-Fi 名称。
///
/// CLLocationManager 的授权弹窗依赖主线程 RunLoop，必须切回主线程调用，
/// 再把正版的三态结果（`requested` / `open_location_settings` / `not_needed`）带回前端。
#[tauri::command]
pub async fn statusbar_request_location(app: AppHandle) -> &'static str {
    let (tx, rx) = std::sync::mpsc::channel();
    if app
        .run_on_main_thread(move || {
            let _ = tx.send(statusbar::request_location_permission());
        })
        .is_err()
    {
        return "not_needed";
    }
    tauri::async_runtime::spawn_blocking(move || {
        rx.recv_timeout(std::time::Duration::from_secs(2))
            .unwrap_or("not_needed")
    })
    .await
    .unwrap_or("not_needed")
}

/// 电池详情页读数：只在页面打开时按需读（正版页面自己每 2 秒拉一次）。
#[tauri::command]
pub async fn statusbar_battery_details() -> statusbar::BatteryDetails {
    tauri::async_runtime::spawn_blocking(statusbar::battery_details)
        .await
        .unwrap_or_default()
}

/// Wi-Fi 二级页读数：一次全信道扫描要一秒上下，绝不能在主线程跑。
///
/// 正版同样只在页面打开时扫（`WiFiNetworkController.activate`），并把最小扫描
/// 间隔压到 30 秒；这里由前端控制「打开即扫、点刷新再扫」，不自动循环。
#[tauri::command]
pub async fn statusbar_wifi_page() -> statusbar::WifiScan {
    tauri::async_runtime::spawn_blocking(statusbar::wifi_scan)
        .await
        .unwrap_or_else(|_| statusbar::WifiScan::unavailable())
}

/// 「立即充满电」读数：PowerUI 私有框架，几毫秒，面板可见时轮询。
#[tauri::command]
pub async fn statusbar_charge_to_full_state() -> statusbar::ChargeToFullState {
    tauri::async_runtime::spawn_blocking(statusbar::charge_to_full_state)
        .await
        .unwrap_or_else(|_| statusbar::ChargeToFullState::unavailable())
}

/// 临时解除 macOS 的充电上限：对系统有一次真实写入，前端必须先经用户点击。
#[tauri::command]
pub async fn statusbar_charge_to_full() -> CommandResult<()> {
    tauri::async_runtime::spawn_blocking(statusbar::charge_to_full)
        .await
        .map_err(|e| system_error(e.to_string()))?
        .map_err(system_error)
}

/// 取消「立即充满」：恢复发起前记下的上限档位。
#[tauri::command]
pub async fn statusbar_cancel_charge_to_full() -> CommandResult<i64> {
    tauri::async_runtime::spawn_blocking(statusbar::cancel_charge_to_full)
        .await
        .map_err(|e| system_error(e.to_string()))?
        .map_err(system_error)
}

/// 面板可见时的轻量音量轮询：只碰 CoreAudio，用来让滑条跟着硬件音量键走。
#[tauri::command]
pub async fn statusbar_audio_levels() -> statusbar::AudioLevels {
    match tauri::async_runtime::spawn_blocking(statusbar::audio_levels).await {
        Ok(levels) => levels,
        Err(_) => statusbar::AudioLevels {
            output: statusbar::AudioDeviceState::unavailable(),
            input: statusbar::AudioDeviceState::unavailable(),
        },
    }
}

/// 音量提示音的系统开关（正版 `com.apple.sound.beep.feedback`，缺省即开）。
#[tauri::command]
pub fn statusbar_volume_feedback_enabled() -> bool {
    statusbar::volume_feedback_enabled()
}

/// 正版的「多重回落」系统设置面板表：新式扩展 ID 在前，老式 anchor 在后。
///
/// `open` 对不认识的 anchor 也一样返回成功（它只负责把 URL 交给系统设置），
/// 所以顺序本身就是回落策略，与正版 `openSystemSettings(_:)` 的逐条尝试一致。
#[cfg(target_os = "macos")]
const SYSTEM_PANES: &[(&str, &[&str])] = &[
    (
        "battery",
        &[
            "com.apple.Battery-Settings.extension",
            "com.apple.preference.battery",
        ],
    ),
    (
        "wifi",
        &[
            "com.apple.wifi-settings-extension",
            "com.apple.Network-Settings.extension?Wi-Fi",
            "com.apple.preference.network?Wi-Fi",
        ],
    ),
    (
        "bluetooth",
        &[
            "com.apple.BluetoothSettings",
            "com.apple.preference.bluetooth",
        ],
    ),
    (
        "sound_output",
        &[
            "com.apple.Sound-Settings.extension?output",
            "com.apple.preference.sound?output",
        ],
    ),
    (
        "sound_input",
        &[
            "com.apple.Sound-Settings.extension?input",
            "com.apple.preference.sound?input",
        ],
    ),
    (
        "keyboard",
        &[
            "com.apple.Keyboard-Settings.extension",
            "com.apple.preference.keyboard",
        ],
    ),
    (
        "location",
        &[
            "com.apple.settings.PrivacySecurity.extension?Privacy_LocationServices",
            "com.apple.preference.security?Privacy_LocationServices",
        ],
    ),
    (
        "sharing",
        &[
            "com.apple.Sharing-Settings.extension",
            "com.apple.preferences.sharing",
        ],
    ),
    (
        "bluetooth_permission",
        &[
            "com.apple.settings.PrivacySecurity.extension?Privacy_Bluetooth",
            "com.apple.preference.security?Privacy_Bluetooth",
        ],
    ),
];

/// 面板里每一枚齿轮：收起面板，然后打开**系统设置**对应面板。
///
/// 正版从不把自己的设置页塞给齿轮——它开的是 `com.apple.*`；软件自己的设置
/// 入口只有页脚那枚「设置…」（见 `statusbar_open_settings`）。
#[tauri::command]
pub fn statusbar_open_pane(app: AppHandle, pane: String) -> CommandResult<()> {
    window::hide_tray_panel(&app);
    #[cfg(target_os = "macos")]
    {
        let urls = SYSTEM_PANES
            .iter()
            .find(|(key, _)| *key == pane.as_str())
            .map(|(_, urls)| *urls)
            .ok_or_else(|| AppError::new(9000, format!("未知的系统设置面板：{pane}")))?;
        let mut last_error = String::new();
        for url in urls {
            // 只接受白名单里的常量，绝不把前端字符串直接拼给 `open`。
            match std::process::Command::new("open")
                .arg(format!("x-apple.systempreferences:{url}"))
                .status()
            {
                Ok(status) if status.success() => return Ok(()),
                Ok(status) => last_error = format!("退出码 {}", status.code().unwrap_or(-1)),
                Err(e) => last_error = e.to_string(),
            }
        }
        return Err(AppError::with_details(
            9000,
            "无法打开系统设置面板",
            last_error,
        ));
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = pane;
        Ok(())
    }
}

/// 收起状态栏面板（前端在 Esc / 执行动作后调用）。
#[tauri::command]
pub fn statusbar_hide(app: AppHandle) {
    window::hide_tray_panel(&app);
}

/// 面板底部「设置…」：收起面板并打开主窗口的状态栏分区。
#[tauri::command]
pub fn statusbar_open_settings(app: AppHandle) {
    window::hide_tray_panel(&app);
    window::show_main_window_with_page(&app, Some("statusbar".into()));
}

/// 前端按实际内容高度回报，窗口据此重新贴到图标下方。
#[tauri::command]
pub fn statusbar_resize(app: AppHandle, height: f64) {
    window::resize_tray_panel(&app, height);
}
