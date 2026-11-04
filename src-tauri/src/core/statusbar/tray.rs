//! 菜单栏图标的重画循环：把系统状态实时画进那张 18pt 的模板图。
//!
//! 为什么不是「事件驱动」：Wi-Fi 信号、电量、音量各自要注册不同的系统通知源
//! （CoreWLAN / IOKit / CoreAudio），三套监听的生命周期管理比这一格图标值钱得多。
//! 折中办法是低频轮询（2s）+ 任何用户动作打一下 [`request_refresh`] 脏标记，
//! 手感上就是「切了 Wi-Fi 图标马上变」。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use base64::Engine as _;
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::config::ConfigService;
use crate::lock_ext::LockExt;
use crate::window::TRAY_ICON_ID;

use super::{glyph, GlyphState};

/// 设置页的实时预览。
///
/// 直接把**将要画进菜单栏的那张图**交给前端：预览与真实图标共用同一个光栅化器，
/// 不会出现「设置页画得好看、菜单栏另一回事」。顺带带上文字摘要，设置页不必
/// 自己再推一遍判定规则。
#[derive(Clone, Serialize)]
pub struct Preview {
    /// base64 的 RGBA：模板图是「黑 + alpha」，彩色图直接带电池色与墨色。
    pub image: String,
    /// 图片边长（px），前端据此建 ImageData。
    pub size: u32,
    pub tooltip: String,
    /// 中心图形实际画的是哪一种（设置页用来说清热点/有线/共享各自长什么样）。
    pub center: &'static str,
    /// 彩色图标按哪套菜单栏明暗下的墨：预览条要用同一种底色才看得出效果。
    pub dark_bar: bool,
}

pub fn preview(options: &glyph::GlyphOptions) -> Preview {
    let state = super::glyph_state();
    let spec = super::glyph_spec(&state, options);
    let menu_dark = super::menu_bar_dark();
    let palette = glyph::Palette::for_options(options, menu_dark);
    Preview {
        image: base64::engine::general_purpose::STANDARD.encode(glyph::render(&spec, &palette)),
        size: glyph::IMAGE_SIZE as u32,
        tooltip: super::glyph_tooltip(&state, &spec),
        center: spec.center_name(),
        dark_bar: glyph::is_dark_bar(options.appearance, menu_dark),
    }
}

/// 轮询周期：电池/信号这类慢变化够用，CPU 基本不参与。
const POLL: Duration = Duration::from_millis(2000);
/// 不充电时的响应粒度（也是线程一次最多睡多久）。
const IDLE_TICK: Duration = Duration::from_millis(200);
/// 充电扫光的帧间隔：正版 `ChargingEffectClock.tickInterval` 就是 50ms。
const ANIM_TICK: Duration = Duration::from_millis(50);
/// 插上电源先播 Quick 扫光，再转常速。
const BURST_START_DELAY: Duration = Duration::from_millis(600);

/// 有状态变化待重画（切了 Wi-Fi/蓝牙、调了音量、改了图标配置）。
static DIRTY: AtomicBool = AtomicBool::new(true);

/// 唤醒计数：`request_refresh` 每打一次脏标记就 +1，重画线程只等「变了」或超时。
///
/// 光靠 200ms 空转的话，切输入法后字牌要等小半拍才出来；Apple 的输入菜单图标
/// 是按下即换，这里用条件变量把延迟压到一帧。
static WAKE: Mutex<u64> = Mutex::new(0);
static WAKE_CV: Condvar = Condvar::new();

/// 输入法字牌的闪现时长：淡入 100ms、停住 900ms、淡出 160ms。
const IME_IN: f64 = 0.10;
const IME_HOLD: f64 = 0.90;
const IME_OUT: f64 = 0.16;
const IME_TOTAL: f64 = IME_IN + IME_HOLD + IME_OUT;
/// 字牌动画的帧间隔。
const IME_TICK: Duration = Duration::from_millis(16);

/// 正在闪的输入法字牌（开始时刻 + 字符）。
struct ImeFlashState {
    marker: char,
    started: Instant,
}

static IME_FLASH: Mutex<Option<ImeFlashState>> = Mutex::new(None);

/// 输入法切换时让整枚图标闪一下新输入源的字牌。非阻塞，可在任意线程调用。
pub fn flash_input_source(marker: Option<&str>) {
    let Some(marker) = marker.and_then(|m| m.chars().next()) else {
        return;
    };
    *IME_FLASH.lock_ok() = Some(ImeFlashState {
        marker,
        started: Instant::now(),
    });
    request_refresh();
}

/// 当前这一帧的字牌；动画播完时清掉状态并打一次脏标记，让图标恢复复合图形。
fn flash_at(now: Instant) -> Option<glyph::ImeFlash> {
    let mut slot = IME_FLASH.lock_ok();
    let flash = slot.as_ref()?;
    let elapsed = now.saturating_duration_since(flash.started).as_secs_f64();
    if elapsed >= IME_TOTAL {
        *slot = None;
        // 恢复的那一帧要走完整重画路径（重新读系统状态），否则图标会停在字牌上。
        DIRTY.store(true, Ordering::Relaxed);
        return None;
    }
    let (alpha, scale) = if elapsed < IME_IN {
        let t = elapsed / IME_IN;
        // 平滑起步的一点点放大，像正版输入菜单图标弹出来。
        (t, 0.90 + 0.10 * (t * t * (3.0 - 2.0 * t)))
    } else if elapsed < IME_IN + IME_HOLD {
        (1.0, 1.0)
    } else {
        let t = (elapsed - IME_IN - IME_HOLD) / IME_OUT;
        ((1.0 - t).max(0.0), 1.0)
    };
    Some(glyph::ImeFlash::new(flash.marker, alpha, scale))
}

/// 一次要写到托盘上的全部内容。
#[derive(Clone, PartialEq)]
struct Frame {
    rgba: Vec<u8>,
    tooltip: String,
    /// 交给系统反色（模板图）还是原样显示（彩色图）。
    template: bool,
}

/// 当前这轮充电动画的时间线（正版 `ChargingEffectClock` 的简化版）。
///
/// 正版能从系统事件里精确知道「刚插上电源 / 电量刚跳档」；我们只有 2s 一次的
/// 轮询，所以用读数变化来推：开始充电 = 插上电源（播 burst），电量变大 = 跳档
/// （把当圈或下一圈的心跳放大到 1.35）。
struct AnimState {
    burst_started: Option<Instant>,
    steady_started: Instant,
    last_percent: u8,
    boosted_cycle: Option<u64>,
}

impl AnimState {
    fn phase(&self, now: Instant) -> Option<glyph::ChargingPhase> {
        if let Some(burst) = self.burst_started {
            let elapsed = now.saturating_duration_since(burst).as_secs_f64();
            if elapsed < glyph::BURST_CYCLE {
                return glyph::charging_phase(elapsed, glyph::ChargingKind::Burst, 1.0);
            }
        }
        let elapsed = now
            .saturating_duration_since(self.steady_started)
            .as_secs_f64();
        let cycle = (elapsed / glyph::STEADY_CYCLE).floor() as u64;
        let multiplier = if Some(cycle) == self.boosted_cycle {
            1.35
        } else {
            1.0
        };
        glyph::charging_phase(elapsed, glyph::ChargingKind::Steady, multiplier)
    }
}

/// 每次轮询后的静态读数 + 配色，动画帧只换 phase 重画同一份内容。
struct Live {
    spec: glyph::GlyphSpec,
    palette: glyph::Palette,
    tooltip: String,
    template: bool,
}

impl Live {
    fn frame(
        &self,
        phase: Option<glyph::ChargingPhase>,
        ime: Option<glyph::ImeFlash>,
    ) -> Frame {
        Frame {
            rgba: glyph::render_with_ime(&self.spec, &self.palette, phase, ime),
            tooltip: self.tooltip.clone(),
            template: self.template,
        }
    }
}

/// 请求立刻重画。非阻塞，可以在任何线程调用。
pub fn request_refresh() {
    DIRTY.store(true, Ordering::Relaxed);
    let mut wake = WAKE.lock_ok();
    *wake += 1;
    WAKE_CV.notify_all();
}

/// 睡到下一次唤醒或超时；期间有任何 `request_refresh` 就立刻返回。
fn pause(timeout: Duration) {
    let wake = WAKE.lock_ok();
    if DIRTY.load(Ordering::Relaxed) {
        return;
    }
    let seen = *wake;
    let _ = WAKE_CV.wait_timeout_while(wake, timeout, |current| *current == seen);
}

/// 启动重画线程（进程内只启一次；重复调用只是多一个空转线程，故由调用方保证）。
pub fn start(app: AppHandle, config: ConfigService) {
    // `effectiveAppearance` 在状态栏按钮挂进菜单栏后还要 ~100ms 才稳定：
    // setup 里读到的是 VibrantLight，稍后才变 VibrantDark。补一次延时同步，
    // 免得深色菜单栏上先闪一帧黑墨。
    let probe = app.clone();
    let _ = std::thread::Builder::new()
        .name("tray-appearance".to_string())
        .spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            refresh_system_ui(&probe);
        });
    let _ = std::thread::Builder::new()
        .name("tray-glyph".to_string())
        .spawn(move || {
            // 等延时同步把 appearance 读准，第一帧就直接是正确墨色。
            std::thread::sleep(Duration::from_millis(300));
            run(app, config);
        });
}

fn run(app: AppHandle, config: ConfigService) {
    let mut last: Option<Frame> = None;
    let mut live: Option<Live> = None;
    let mut anim: Option<AnimState> = None;
    let mut last_tooltip: Option<String> = None;
    let mut last_poll = Instant::now() - POLL;
    loop {
        let now = Instant::now();
        let flash = flash_at(now);
        let dirty = DIRTY.swap(false, Ordering::Relaxed);
        if dirty || last.is_none() || last_poll.elapsed() >= POLL {
            last_poll = Instant::now();
            refresh_system_ui(&app);
            match read_live(&config) {
                Some(read) => {
                    anim = next_anim(anim, &read.spec, now);
                    let frame = read.frame(anim.as_ref().and_then(|a| a.phase(now)), flash);
                    // 充电时每次轮询都会重画动画帧，tooltip 却通常没变；
                    // 按文字是否变化决定要不要重设，否则日志和提示每 2s 刷一次。
                    let announce = last_tooltip.as_deref() != Some(read.tooltip.as_str());
                    push(&app, &mut last, frame, announce);
                    if announce {
                        last_tooltip = Some(read.tooltip.clone());
                    }
                    live = Some(read);
                }
                None => {
                    // 图标被用户关掉了：不画，也不记 live，重新打开时立刻补一张。
                    live = None;
                    anim = None;
                    last_tooltip = None;
                }
            }
        } else if let Some(read) = live.as_ref() {
            // 字牌与充电动画都按帧重画；静态时保持 200ms 的空转。
            if anim.is_some() || flash.is_some() {
                let frame = read.frame(anim.as_ref().and_then(|a| a.phase(now)), flash);
                push(&app, &mut last, frame, false);
            }
        }
        let tick = if flash.is_some() {
            IME_TICK
        } else if anim.is_some() {
            ANIM_TICK
        } else {
            IDLE_TICK
        };
        pause(tick);
    }
}

fn read_live(config: &ConfigService) -> Option<Live> {
    let settings = config.get();
    if !settings.statusbar.enabled {
        return None;
    }
    let state = super::glyph_state();
    let options = settings.statusbar.glyph_options();
    let spec = super::glyph_spec(&state, &options);
    Some(Live {
        palette: glyph::Palette::for_options(&options, super::menu_bar_dark()),
        tooltip: super::glyph_tooltip(&state, &spec),
        template: !options.colored,
        spec,
    })
}

fn next_anim(
    previous: Option<AnimState>,
    spec: &glyph::GlyphSpec,
    now: Instant,
) -> Option<AnimState> {
    let percent = spec.battery?;
    if !spec.charging {
        return None;
    }
    match previous {
        None => Some(AnimState {
            burst_started: Some(now),
            steady_started: now + BURST_START_DELAY,
            last_percent: percent,
            boosted_cycle: None,
        }),
        Some(mut state) => {
            let burst_done = state
                .burst_started
                .is_some_and(|start| now.saturating_duration_since(start).as_secs_f64() >= glyph::BURST_CYCLE);
            if burst_done {
                state.burst_started = None;
            }
            if percent > state.last_percent {
                state.last_percent = percent;
                if !burst_done {
                    // 还在快扫里：等常速开始时自然会有一次完整扫光，不必额外放大。
                } else {
                    let elapsed = now
                        .saturating_duration_since(state.steady_started)
                        .as_secs_f64()
                        + 1e-5;
                    let cycle = (elapsed / glyph::STEADY_CYCLE).floor() as u64;
                    let into_cycle = elapsed % glyph::STEADY_CYCLE;
                    let heartbeat_starts = glyph::STEADY_CYCLE - 0.25;
                    state.boosted_cycle =
                        Some(cycle + u64::from(into_cycle >= heartbeat_starts));
                }
            }
            Some(state)
        }
    }
}

fn push(app: &AppHandle, last: &mut Option<Frame>, frame: Frame, announce: bool) {
    if last.as_ref() == Some(&frame) {
        return;
    }
    // 托盘对象只能在主线程碰：`TrayIcon` 内部是 `Rc`，tauri 只靠
    // `unsafe impl Send` 放行，后台线程 clone/drop 会和主线程抢那根
    // 非原子引用计数，曾因此在后台线程把 NSStatusItem 移除而崩溃。
    let payload = frame.clone();
    let mover = app.clone();
    if app
        .run_on_main_thread(move || apply(&mover, &payload, announce))
        .is_err()
    {
        return;
    }
    *last = Some(frame);
}

/// 每轮轮询让主线程重读菜单栏真实外观与系统强调色。
///
/// 两者都只能在主线程读（AppKit），而图标是在后台线程画的；这里把读取丢回
/// 主线程，明暗真的变了就由 [`super::set_menu_bar_dark`] 打脏标记重画。
fn refresh_system_ui(app: &AppHandle) {
    let mover = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(tray) = mover.tray_by_id(TRAY_ICON_ID) {
            sync_menu_bar_appearance(&tray);
        }
        super::cache_system_accent();
    });
}

/// 读 `NSStatusBarButton.effectiveAppearance` 并写进缓存。
///
/// 菜单栏会按壁纸明暗自动换墨（VibrantDark 等），这和系统「外观」偏好不是
/// 一回事：浅色系统配深色壁纸时，系统外观说浅色，菜单栏实际是深色。只能用
/// 状态栏按钮自己的 appearance。必须在主线程调用。
#[cfg(target_os = "macos")]
pub fn sync_menu_bar_appearance(tray: &tauri::tray::TrayIcon<tauri::Wry>) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;

    let dark = tray
        .with_inner_tray_icon(|inner| {
            let item = inner.ns_status_item()?;
            unsafe {
                let item = &*item as *const _ as *mut AnyObject;
                let button: *mut AnyObject = msg_send![item, button];
                if button.is_null() {
                    return None;
                }
                let appearance: *mut AnyObject = msg_send![button, effectiveAppearance];
                if appearance.is_null() {
                    return None;
                }
                let name: *mut AnyObject = msg_send![appearance, name];
                super::cf::string(name as super::cf::CFTypeRef).map(|name| name.contains("Dark"))
            }
        })
        .ok()
        .flatten();
    if let Some(dark) = dark {
        super::set_menu_bar_dark(dark);
    }
}

#[cfg(not(target_os = "macos"))]
pub fn sync_menu_bar_appearance(_tray: &tauri::tray::TrayIcon<tauri::Wry>) {}

/// tray-icon 把每张图的高度写死成 18pt（`set_icon_for_ns_status_item_button`），
/// 而这里的图标是 26pt。`set_icon` 每次都
/// 会把 NSImage 的 size 改回 18，所以每次贴图之后都要再覆盖一次。
///
/// 只能在主线程调用（`apply` 就在主线程）。主线程上 Tauri 的
/// `run_on_main_thread` 是内联执行的，`with_inner_tray_icon` 不会自锁。
#[cfg(target_os = "macos")]
pub(crate) fn resize_status_item(tray: &tauri::tray::TrayIcon<tauri::Wry>) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;

    let points = glyph::ICON_POINTS;
    let _ = tray.with_inner_tray_icon(move |inner| {
        let Some(item) = inner.ns_status_item() else {
            return;
        };
        unsafe {
            let item = &*item as *const _ as *mut AnyObject;
            let button: *mut AnyObject = msg_send![item, button];
            if button.is_null() {
                return;
            }
            let image: *mut AnyObject = msg_send![button, image];
            if image.is_null() {
                return;
            }
            let size = objc2_foundation::CGSize::new(points, points);
            let _: () = msg_send![image, setSize: size];
        }
    });
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn resize_status_item(_tray: &tauri::tray::TrayIcon<tauri::Wry>) {}

fn frame_for(
    state: &GlyphState,
    options: &glyph::GlyphOptions,
    menu_bar_dark: Option<bool>,
) -> Frame {
    let spec = super::glyph_spec(state, options);
    Frame {
        rgba: glyph::render(&spec, &glyph::Palette::for_options(options, menu_bar_dark)),
        tooltip: super::glyph_tooltip(state, &spec),
        template: !options.colored,
    }
}

fn apply(app: &AppHandle, frame: &Frame, announce: bool) {
    let Some(tray) = app.tray_by_id(TRAY_ICON_ID) else {
        tracing::warn!(id = TRAY_ICON_ID, "tray icon not found, glyph not applied");
        return;
    };
    let image = tauri::image::Image::new_owned(
        frame.rgba.clone(),
        glyph::IMAGE_SIZE as u32,
        glyph::IMAGE_SIZE as u32,
    );
    // 彩色图必须关掉模板反色；一次调用同时换图和换标记，避免先黑后彩闪一下。
    if let Err(e) = tray.set_icon_with_as_template(Some(image), frame.template) {
        tracing::warn!("tray set_icon_with_as_template failed: {e}");
        return;
    }
    // set_icon 会把 NSImage 的高度写回 18pt，紧跟着覆盖成 26pt。
    resize_status_item(&tray);
    if !announce {
        return;
    }
    if let Err(e) = tray.set_tooltip(Some(frame.tooltip.as_str())) {
        tracing::warn!("tray set_tooltip failed: {e}");
        return;
    }
    // 每次图标内容真的变了才记一行：菜单栏图标没有 UI 可以自查，
    // 「图标是不是没更新」这类问题只能靠这行日志回答。动画帧不记。
    tracing::info!(tooltip = %frame.tooltip, "tray glyph updated");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::statusbar::{AudioDeviceState, BatteryState, GlyphState, NetworkState};

    fn state() -> GlyphState {
        GlyphState {
            battery: BatteryState {
                present: true,
                percent: 42,
                charging: true,
                charged: false,
                plugged: true,
                low_power: false,
                minutes_to_empty: None,
                minutes_to_full: Some(60),
            },
            network: NetworkState {
                available: true,
                powered: true,
                connected: true,
                ssid: Some("Office".to_string()),
                rssi: Some(-58),
                band: Some("5 GHz".to_string()),
                name_access: "authorized".to_string(),
                interface: Some("en0".to_string()),
                ipv4: Some("10.0.0.7".to_string()),
                link: "wifi".to_string(),
            },
            bluetooth_available: true,
            bluetooth_powered: true,
            audio_output: AudioDeviceState {
                available: true,
                device_id: 3,
                device_name: "扬声器".to_string(),
                volume: 0.4,
                volume_available: true,
                muted: false,
                mute_available: true,
                devices: Vec::new(),
            },
        }
    }

    #[test]
    fn ime_flash_fades_in_holds_and_clears_itself() {
        let started = Instant::now();
        *IME_FLASH.lock_ok() = Some(ImeFlashState {
            marker: '拼',
            started,
        });
        let at = |ms: u64| flash_at(started + Duration::from_millis(ms));

        let first = at(0).expect("第一帧");
        assert_eq!((first.marker, first.filled), ('拼', true));
        assert!(first.alpha.abs() < 1e-9);
        assert!(first.scale < 1.0);

        let hold = at(500).expect("停住段");
        assert_eq!((hold.alpha, hold.scale), (1.0, 1.0));

        let out = at(1100).expect("淡出段");
        assert!(out.alpha > 0.0 && out.alpha < 1.0);

        assert!(at(1160).is_none());
        assert!(IME_FLASH.lock_ok().is_none(), "播完必须清掉字牌");
    }

    #[test]
    fn template_frame_is_black_with_alpha_only() {
        let options = glyph::GlyphOptions {
            colored: false,
            ..glyph::GlyphOptions::default()
        };
        let frame = frame_for(&state(), &options, None);
        assert_eq!(frame.rgba.len(), glyph::IMAGE_SIZE * glyph::IMAGE_SIZE * 4);
        assert!(frame.rgba.chunks(4).all(|px| px[..3] == [0, 0, 0]));
        assert!(frame.rgba.chunks(4).any(|px| px[3] > 0));
        assert!(frame.template, "关掉彩色必须交系统反色");
        assert!(frame.tooltip.contains("42%"));
    }

    #[test]
    fn colored_frame_ships_battery_color_and_ink_together() {
        let options = glyph::GlyphOptions::default();
        let dark = frame_for(&state(), &options, Some(true));
        assert!(!dark.template);
        // 一张图里同时有电池弧的充电绿与白墨（中心图形、音量点）。
        // 常态（不充电）弧本身就是墨色，所以这里必须用充电状态才看得出两种颜色。
        let palette = glyph::Palette::colored(true);
        let battery = palette.battery_rgb(42, false, true);
        assert_ne!(battery, palette.ink);
        assert!(dark
            .rgba
            .chunks(4)
            .any(|px| px[3] == 255 && px[..3] == battery));
        assert!(dark
            .rgba
            .chunks(4)
            .any(|px| px[3] == 255 && px[..3] == [255, 255, 255]));
        // 换菜单栏明暗必须换一张图，否则帧去重会把外观变化吞掉。
        let light = frame_for(&state(), &options, Some(false));
        assert_ne!(dark.rgba, light.rgba);
    }

    #[test]
    fn low_power_frame_ships_the_yellow_arc() {
        // 用户实机对比出来的 bug：拔了电源、系统开着低电量模式时，正版画黄环，
        // 我们画成墨色白环——因为压根没读低电量模式。
        let mut quiet = state();
        quiet.battery.charging = false;
        quiet.battery.plugged = false;
        quiet.battery.low_power = true;
        let options = glyph::GlyphOptions::default();
        let dark = frame_for(&quiet, &options, Some(true));
        let yellow = glyph::Palette::colored(true).low_power;
        assert!(dark
            .rgba
            .chunks(4)
            .any(|px| px[3] == 255 && px[..3] == yellow));
        // 只有弧换黄，其余照旧是墨色（中心图形、音量点）。
        assert!(dark
            .rgba
            .chunks(4)
            .any(|px| px[3] == 255 && px[..3] == [255, 255, 255]));
        assert!(dark.tooltip.contains("低电量模式"));
    }

    #[test]
    fn same_inputs_produce_the_same_frame() {
        let options = glyph::GlyphOptions::default();
        let a = frame_for(&state(), &options, Some(true));
        let b = frame_for(&state(), &options, Some(true));
        assert_eq!(a.rgba, b.rgba);
        assert_eq!(a.tooltip, b.tooltip);

        // 关掉图标内容开关必须换一张图（否则去重会把新配置吞掉）。
        let mut quiet = state();
        quiet.audio_output.muted = true;
        assert_ne!(frame_for(&quiet, &options, Some(true)).rgba, a.rgba);
    }

    #[test]
    fn preview_ships_the_real_tray_image() {
        let shot = preview(&glyph::GlyphOptions::default());
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&shot.image)
            .expect("preview image must be base64");
        // 设置页拿到的必须是和托盘同一份 2x 图：满尺寸 RGBA，且真有像素。
        assert_eq!(bytes.len(), glyph::IMAGE_SIZE * glyph::IMAGE_SIZE * 4);
        assert_eq!(shot.size as usize, glyph::IMAGE_SIZE);
        assert!(bytes.chunks(4).any(|px| px[3] > 0));
        assert!(shot.tooltip.starts_with("Halo"));
        // 预览读的是本机真实状态，这里只校验名字合法。
        assert!([
            "wifi",
            "hotspot",
            "ethernet",
            "temporary",
            "shared",
            "bluetooth",
            "clipboard"
        ]
        .contains(&shot.center));

        let template = preview(&glyph::GlyphOptions {
            colored: false,
            ..glyph::GlyphOptions::default()
        });
        let plain = base64::engine::general_purpose::STANDARD
            .decode(&template.image)
            .expect("template image");
        assert!(plain.chunks(4).all(|px| px[..3] == [0, 0, 0]));
    }
}
