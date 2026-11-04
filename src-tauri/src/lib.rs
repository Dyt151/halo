//! Tauri 应用入口：注册插件/命令/状态/托盘，启动核心服务。

// 架构冻结了扩展框架/同步层等公共 API（如 ModuleMessage、MessageBus::subscribe、
// schema 常量等），其中部分接口在 P0 运行时路径中尚未全部接线（留待后续模块市场/
// WASM 运行时等）。为避免噪音，允许这些「为未来保留」的公共项暂未使用。
#![allow(dead_code)]

// 说明：以下模块对 `halo_import` 命令行工具开放 pub 可见性，
// 以便迁移工具复用与 App 完全一致的历史/回复写入逻辑。
pub mod alert;
pub mod commands;
pub mod config;
pub mod core;
pub mod error;
pub mod events;
pub mod import;
pub mod lock_ext;
pub mod logging;
pub mod menu;
pub mod modules;
pub mod secrets;
pub mod state;
pub mod storage;
pub mod sync;
pub mod window;

#[cfg(test)]
mod qa_tests;

use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{Shortcut, ShortcutEvent, ShortcutState};

use crate::core::clipboard::types::ClipEvent;
use crate::core::{
    resolve_action, ClipboardMonitor, ClipboardStack, HistoryService, HotkeyAction, HotkeyManager,
    PasteService, SensitivityDetector,
};
use crate::modules::{CapabilityGate, MessageBus, ModuleHost};
use crate::state::AppState;
use crate::sync::SyncManager;

/// 全局快捷键处理器：按配置分发动作（唤起/堆栈/数字粘贴）。
fn handle_shortcut(app: &AppHandle, shortcut: &Shortcut, event: ShortcutEvent) {
    if event.state != ShortcutState::Pressed {
        return;
    }
    let state = app.state::<AppState>();
    let config = state.config.get();
    match resolve_action(&config, shortcut) {
        Some(HotkeyAction::Toggle) => window::toggle_panel_window(app),
        Some(HotkeyAction::StackPaste) => {
            // 快捷键不经过面板，此刻前台仍是用户正在用的 App，先记下它（评审 S4）。
            window::remember_frontmost_app();
            let _ = state.paste_stack_top();
        }
        None => {}
    }
}

/// 托盘矩形取物理像素（窗口定位按物理值再按屏缩放换算）；逻辑坐标的
/// 罕见情况直接忽略，退回到「光标所在屏」的兜底定位。
fn tray_rect_physical(rect: &tauri::Rect) -> Option<(f64, f64, f64, f64)> {
    match (rect.position, rect.size) {
        (tauri::Position::Physical(p), tauri::Size::Physical(s)) => {
            Some((p.x as f64, p.y as f64, s.width as f64, s.height as f64))
        }
        _ => None,
    }
}

/// 创建菜单栏图标：左键开合状态栏面板，右键菜单直达关于 / 设置 / 退出。
///
/// 图标不是静态资源，而是按当前系统状态现画的三区模板图（电量环 / 网络 /
/// 音量点，见 `core::statusbar::glyph`）；之后由 `statusbar::tray` 的循环重画。
fn create_tray_icon(
    app: &tauri::App,
    config: &crate::config::settings::Config,
) -> tauri::Result<()> {
    use tauri::menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    let about = MenuItemBuilder::with_id("tray-about", "关于 Halo").build(app)?;
    let settings = MenuItemBuilder::with_id("tray-settings", "设置…").build(app)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItemBuilder::with_id("tray-quit", "退出 Halo").build(app)?;
    let menu = MenuBuilder::new(app)
        .items(&[&about, &settings, &separator, &quit])
        .build()?;

    // 建图标这一步绝不读系统状态：`IOBluetoothPreferenceGetControllerPowerState`
    // 第一次调用会 dispatch_once 初始化 CoreBluetooth 协调器，而它等的那个回调只能
    // 由主线程派。setup 正跑在 applicationDidFinishLaunching 里，主线程被自己堵死，
    // 整个进程卡在启动这一步（托盘重画线程永远起不来）。首帧交给我马上启动的
    // `tray::start` 后台循环，这里只放一张无状态的剪贴板底图。
    let options = config.statusbar.glyph_options();
    let spec = core::statusbar::glyph_spec(&core::statusbar::GlyphState::empty(), &options);
    // 这一刻状态栏按钮的 effectiveAppearance 还没稳定（实测约 100ms 后才从
    // VibrantLight 变 VibrantDark），先给系统一张模板图让它自己反色，保证首帧
    // 墨色正确；tray::start 会在延时同步读到真实明暗后换成彩色图。
    let palette = core::statusbar::glyph::Palette::template();
    let size = core::statusbar::glyph::IMAGE_SIZE as u32;
    let icon =
        tauri::image::Image::new_owned(core::statusbar::glyph::render(&spec, &palette), size, size);
    let tray = TrayIconBuilder::with_id(window::TRAY_ICON_ID)
        .icon(icon)
        .icon_as_template(true)
        .tooltip("Halo")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "tray-about" => window::show_main_window_with_page(app, Some("about".into())),
            "tray-settings" => window::show_main_window_with_page(app, Some("general".into())),
            "tray-quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            // 用抬起事件：按下时窗口还没失焦，抬起时面板状态已经稳定，
            // 点图标收起的那一次不会被「先失焦、再点击」翻成重新弹出。
            let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                rect,
                ..
            } = event
            else {
                return;
            };
            if let Some((x, y, w, h)) = tray_rect_physical(&rect) {
                window::set_tray_icon_rect(x, y, w, h);
            }
            window::toggle_tray_panel(tray.app_handle());
        })
        .build(app)?;
    tray.set_visible(config.statusbar.enabled)?;
    // setup 正在主线程：先把菜单栏真实明暗与系统强调色读进缓存，首帧就用对墨色、
    // 面板第一次拉取也拿到正确强调色。
    core::statusbar::tray::sync_menu_bar_appearance(&tray);
    core::statusbar::cache_system_accent();
    // tray-icon 建图时会压成 18pt，这里立刻改回 26pt，别等第一帧重画才跳大。
    core::statusbar::tray::resize_status_item(&tray);
    Ok(())
}

pub fn run() {
    // 改名迁移要在日志初始化之前：日志会先创建 `~/.halo/logs`，目标一存在就搬不动了。
    storage::migrate_legacy_dir();
    // 日志同时落盘（`~/.halo/logs/halo.log`）与 stderr：用 `open` 启动时
    // stderr 会被系统丢掉，没有文件就无从定位「手势没反应」这类无 UI 反馈的失败。
    logging::init();

    let _ = storage::ensure_dirs();
    // 数据库打不开属于启动致命错误：run() 不返回 Result，此处直接 panic
    // （与下方 tauri run 的 expect 同一模式，均为进程入口的不可恢复失败）。
    let conn = storage::db::open(&storage::db_path()).expect("failed to open database");
    let db = Arc::new(Mutex::new(conn));

    let config = crate::config::ConfigService::new(db.clone());
    let history = HistoryService::new(db.clone(), config.clone());
    // 启动时修掉旧版本写坏的 `content`（非法 JSON）：坏行会让查询逐行降级，
    // 用户永远看不到那条内容；能挽回的先在启动阶段改写成合法 JSON。
    let repaired = history.repair_corrupt_content();
    if repaired > 0 {
        tracing::info!("repaired {repaired} corrupt clip contents");
    }
    // 启动时修正历史脏数据（旧版本写入 size_bytes=0 的记录）：
    // 图片按磁盘文件、文本/html 按字节、files 按路径字节重算，幂等。
    match history.backfill_sizes() {
        Ok(0) => {}
        Ok(n) => tracing::info!("backfilled size_bytes for {n} clips"),
        Err(e) => tracing::warn!("backfill size_bytes failed: {e}"),
    }
    let gate = Arc::new(CapabilityGate::new());
    let bus = Arc::new(MessageBus::new());
    let modules = ModuleHost::new(
        db.clone(),
        storage::modules_dir(),
        gate.clone(),
        bus.clone(),
    );
    let sync = SyncManager::new(db.clone(), history.clone(), config.clone());
    let monitor = ClipboardMonitor::new();
    let paste = PasteService::new(history.clone(), config.clone(), monitor.clone());
    let sensitivity = SensitivityDetector::new();

    tauri::Builder::default()
        // 单实例必须最先注册：第二个实例启动时直接拉起已有实例再退出，
        // 否则两个进程会同时监控剪贴板、同时写 SQLite（评审 S8）。
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            crate::window::show_main_window(app);
        }))
        // 窗口位置/尺寸持久化（评审 S9）。
        // 不恢复可见性：菜单栏应用退出时面板/管理窗口可能正开着，
        // 若把 visible 一起存回来，下次启动会直接弹窗（面板还会绕过
        // show_panel_window 的定位与宿主逻辑，无法键入）。
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(
                    tauri_plugin_window_state::StateFlags::all()
                        & !tauri_plugin_window_state::StateFlags::VISIBLE,
                )
                .build(),
        )
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(handle_shortcut)
                .build(),
        )
        // 原生菜单栏：App menu（关于/设置 ⌘,/退出 ⌘Q）+ 编辑菜单（⌘Z/⌘C/⌘V/⌘A）
        // + 窗口菜单（⌘M 最小化 / ⌘W 隐藏）。无菜单时 WKWebView 的编辑命令全部失效。
        .menu(menu::build)
        .on_menu_event(menu::handle_event)
        .invoke_handler(tauri::generate_handler![
            commands::list_clips,
            commands::get_clip,
            commands::open_main_window,
            commands::consume_pending_page,
            commands::paste_clip,
            commands::copy_clip,
            commands::delete_clip,
            commands::clear_clips,
            commands::undo,
            commands::redo,
            commands::list_tags,
            commands::pin_clip,
            commands::favorite_clip,
            commands::set_clip_tag,
            commands::history_stats,
            commands::capture_now,
            commands::get_config,
            commands::patch_config,
            commands::open_system_settings,
            commands::accessibility_status,
            commands::request_accessibility,
            commands::apply_hotkeys,
            commands::validate_hotkey,
            commands::stack_push,
            commands::stack_list,
            commands::stack_paste_first,
            commands::stack_paste_all,
            commands::stack_clear,
            commands::stack_set_lock,
            commands::stack_locked,
            commands::sync_now,
            commands::get_sync_status,
            commands::test_sync,
            commands::sync_secret_status,
            commands::set_sync_settings,
            commands::list_modules,
            commands::discover_modules,
            commands::install_module,
            commands::uninstall_module,
            commands::enable_module,
            commands::disable_module,
            commands::invoke_module,
            commands::list_module_contributions,
            commands::read_module_ui,
            commands::finder_integration_status,
            commands::set_finder_extension_enabled,
            commands::convert_run,
            commands::convert_cancel,
            commands::reveal_path,
            commands::pick_files,
            commands::pick_directory,
            commands::ring_snapshot,
            commands::ring_execute,
            commands::ring_cancel,
            commands::ring_dismiss,
            commands::ring_gesture_status,
            commands::ring_gesture_repair,
            commands::login_item_status,
            commands::set_login_item,
            commands::app_info,
            commands::hide_panel,
            commands::quit_app,
            commands::export_data,
            commands::import_data,
            commands::statusbar_state,
            commands::statusbar_glyph_preview,
            commands::statusbar_set_volume,
            commands::statusbar_set_mute,
            commands::statusbar_set_default_device,
            commands::statusbar_select_input_source,
            commands::statusbar_set_wifi,
            commands::statusbar_set_bluetooth,
            commands::statusbar_request_location,
            commands::statusbar_battery_details,
            commands::statusbar_charge_to_full_state,
            commands::statusbar_charge_to_full,
            commands::statusbar_cancel_charge_to_full,
            commands::statusbar_wifi_page,
            commands::statusbar_audio_levels,
            commands::statusbar_volume_feedback_enabled,
            commands::statusbar_open_pane,
            commands::statusbar_hide,
            commands::statusbar_open_settings,
            commands::statusbar_resize,
        ])
        .setup(move |app| {
            // Dock 里不留图标。Info.plist 里的 LSUIElement 只对 LaunchServices 的
            // **启动判定**有效；tao 在 applicationDidFinishLaunching 里会无条件调
            // `setActivationPolicy(Regular)`（它根本不读 LSUIElement），把 policy
            // 又改回普通 App，于是 Dock 上冒出一个图标。setup 一定跑在那次回调之后，
            // 所以这里改回 Accessory 才是真正生效的那一刀。
            //
            // 代价：Accessory 下窗口不会自动成为前台 App，必须显式 activate——
            // 这件事 window::show_main_window 里的 activate_app() 已经在做了。
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let app_handle = app.handle().clone();
            let hotkey = HotkeyManager::new(app_handle.clone());
            let stack = ClipboardStack::new();

            // 环形会话需要进程级 AppHandle（ObjC 服务回调 / 后台线程无 app 参数）。
            crate::core::ring::RingManager::set_app(app_handle.clone());

            // 监控事件通道：发送端存入 AppState，配置改动（启停/轮询间隔）时
            // 需要它把新任务接上（commands::config::apply_monitor）。
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<ClipEvent>();

            let state = AppState {
                app: app_handle.clone(),
                config: config.clone(),
                history: history.clone(),
                monitor: monitor.clone(),
                paste: paste.clone(),
                hotkey: hotkey.clone(),
                stack: stack.clone(),
                modules: modules.clone(),
                sync: sync.clone(),
                sensitivity: sensitivity.clone(),
                gate: gate.clone(),
                bus: bus.clone(),
                monitor_tx: tx.clone(),
            };

            // 总线 → 前端事件发射器。
            {
                let emitter_app = app_handle.clone();
                state.bus.set_emitter(move |topic, data| {
                    let _ = emitter_app.emit(
                        events::MODULE_EVENT,
                        serde_json::json!({ "topic": topic, "data": data }),
                    );
                });
            }

            // 监控事件循环：捕获 → 敏感识别 → 入库 → 发布。
            {
                let loop_state = state.clone();
                tauri::async_runtime::spawn(async move {
                    while let Some(event) = rx.recv().await {
                        loop_state.process_clip_event(event);
                    }
                });
            }

            // 必须在注册快捷键之前挂载：热键回调里会 `app.state::<AppState>()`，
            // 未挂载时取 State 会直接 panic（启动阶段按一下快捷键就能崩）。
            app.manage(state);

            // 回收无主图片文件：删除图片条目时为支持 ⌘Z 撤销而故意保留文件，
            // 真正回收依赖启动时扫一遍（`~/.halo/data/files` 里的孤儿 PNG）。
            // 要放在监控启动之前：此刻没有新图片写入，不存在「刚写盘还没入库就被删掉」的竞态。
            let pruned = history.prune_orphan_images();
            if pruned > 0 {
                tracing::info!("pruned {pruned} orphan image files");
            }

            let cfg = config.get();
            monitor.apply(tx, cfg.poll_interval_ms, cfg.monitor_enabled);

            // 快捷键是剪贴板面板的主入口：注册失败必须主动把设置弹出来，
            // 否则用户按不出面板也无从发现原因（状态栏图标是另一条入口）。
            let hotkey_report = hotkey.apply_with_report(&cfg);
            let toggle_failed = hotkey_report
                .iter()
                .any(|slot| !slot.ok && slot.slot == "toggle");
            for slot in hotkey_report.iter().filter(|slot| !slot.ok) {
                tracing::warn!(
                    slot = %slot.slot,
                    accelerator = %slot.accelerator,
                    "hotkey register failed: {}",
                    slot.message.as_deref().unwrap_or("")
                );
            }
            if toggle_failed {
                window::show_main_window_with_page(&app_handle, Some("general".into()));
            }

            // Option + 左键拖动文件唤起环形盘（macOS 原生事件监听，其他平台 no-op）。
            // 单按修饰键不是热键，只能靠 CGEventTap 旁听；非 macOS 不实现。
            crate::core::option_tap::init(&app_handle, cfg.ring.option_tap);

            sync.start(cfg.sync.interval_secs);

            // 真实毛玻璃：把 NSVisualEffectView 垫到 webview 之下（评审 M3）。
            // 面板用 HUD 材质（Spotlight 同款），管理窗口用侧边栏材质；
            // 圆角同时交给这层 view，窗口阴影才不会露出直角（评审 M6）。
            if let Some(panel) = app.get_webview_window("panel") {
                crate::window::apply_material(&panel, window::panel_material(), 12.0);
            }
            if let Some(main) = app.get_webview_window("main") {
                crate::window::apply_material(&main, window::window_material(), 10.0);
            }
            // 面板要能出现在别的 App 全屏 Space 上：普通 NSWindow 进不去，
            // 给它配一个真实的 NSPanel 宿主当 Space 锚点（见 window.rs）。
            if let Some(panel) = app.get_webview_window("panel") {
                crate::window::prepare_panel_host(&panel);
            }
            // 面板/环形盘要在别的 App 全屏时也能显示：补上 Tauri 没设的
            // FullScreenAuxiliary（否则只出现在桌面 Space）。
            if let Some(panel) = app.get_webview_window("panel") {
                crate::window::allow_over_fullscreen(&panel);
            }
            if let Some(ring) = app.get_webview_window("ring") {
                crate::window::allow_over_fullscreen(&ring);
            }
            // 状态栏面板：与剪贴板面板同一套 HUD 材质与圆角，只是更窄。
            if let Some(tray) = app.get_webview_window("traypanel") {
                crate::window::apply_material(&tray, window::panel_material(), 12.0);
                crate::window::allow_over_fullscreen(&tray);
            }
            // 环形盘不垫材质：窗口里只该有扇区本身，垫了会多出一块灰色圆角底板。

            // 菜单栏图标：左键开合状态栏面板，右键菜单直达关于 / 设置 / 退出。
            // 图标是按系统状态现画的三区模板图，之后由重画循环持续更新。
            create_tray_icon(app, &cfg)?;
            core::statusbar::tray::start(app_handle.clone(), config.clone());

            // 输入法切换提示：订阅系统的分布式通知。setup 正在主线程，正好是
            // CoreFoundation 通知回调与 TIS 读数唯一可用的线程。
            core::statusbar::watch_input_source_changes(&app_handle);

            // Finder 右键：菜单由 Finder Sync 扩展直接放进右键菜单（不再走
            // 「服务」子菜单）。这里注册扩展并开始轮询扩展投递的动作请求。
            // 扩展未启用时轮询空转，不影响启动；主窗口仍保持静默。
            crate::core::services::start_extension_bridge(&app_handle);

            // 桌面空白处 Finder Sync 不会注入菜单，由主程序用 CGEventTap 接管
            // 并弹原生 NSMenu。是否启停交给 services 启动时核实过的扩展状态：
            // 启动这一刻 `pluginkit` 可能还没把扩展条目落库。
            crate::core::desktop_menu::init(&app_handle);

            Ok(())
        })
        .on_window_event(|window, event| {
            let label = window.label();
            match event {
                // 关闭 = 隐藏：两个窗口都不参与退出流程，应用常驻托盘。
                tauri::WindowEvent::CloseRequested { api, .. } => {
                    api.prevent_close();
                    let _ = window.hide();
                }
                // 面板失焦即收起（点击桌面/其他 App）；管理窗口保持打开。
                tauri::WindowEvent::Focused(false) if label == "panel" => {
                    window::handle_panel_focus_lost(window.app_handle());
                }
                tauri::WindowEvent::Focused(true) if label == "panel" => {
                    window::handle_panel_focus_gained();
                }
                // 状态栏面板失焦即收起（点图标 / 点外部）。
                tauri::WindowEvent::Focused(false) if label == "traypanel" => {
                    window::handle_tray_panel_focus_lost(window.app_handle());
                }
                tauri::WindowEvent::Focused(true) if label == "traypanel" => {
                    window::handle_tray_panel_focus_gained();
                }
                // 环形盘失焦即取消（点外部/切走，任务 22）；转换中不作废已提交任务。
                tauri::WindowEvent::Focused(false) if label == "ring" => {
                    crate::core::ring::RingManager::cancel();
                }
                _ => {}
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building Halo")
        .run(|_app, _event| {
            // 再点一次 App（Finder / Spotlight）也是入口之一：macOS 会先发
            // reopen（对应 applicationShouldHandleReopen:），tao 自己不处理它，
            // 不接就会变成「双击了图标，什么也没发生」。这里把主窗口请到前台。
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = _event {
                crate::window::show_main_window(_app);
            }
        });
}
