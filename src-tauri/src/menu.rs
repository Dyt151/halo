//! 原生菜单栏：App menu + 编辑 + 显示 + 窗口（评审 M1 / M2 / M8）。
//!
//! WKWebView 不接管键盘编辑命令：没有 Edit menu 时，设置页输入框里的
//! ⌘C/⌘V/⌘A 全部失效。窗口菜单补齐管理窗口的 ⌘M（最小化）、缩放、全屏
//! （⌃⌘F）与 ⌘W（关闭 = 隐藏，与标题栏行为一致）。
//!
//! 「撤销 / 重做」是**自定义**菜单项而不是系统预定义的 `undo:`/`redo:`：
//! 预定义项走响应者链，对自绘的 webview 卡片列表完全无效（评审 M8）。
//! 这里改成发事件给前台窗口，由前端按焦点分流——输入框里做文本撤销，
//! 其它情况下撤销「删除条目」。

use tauri::menu::{IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Emitter, Manager, Wry};

/// 构建菜单栏。macOS 额外挂 App menu（关于 / 设置 ⌘, / 服务 / 隐藏 / 退出 ⌘Q）。
pub fn build(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let edit = build_edit_menu(app)?;
    let view = build_view_menu(app)?;
    let window = build_window_menu(app)?;

    #[cfg(target_os = "macos")]
    {
        let app_menu = build_app_menu(app)?;
        Menu::with_items(app, &[&app_menu, &edit, &view, &window])
    }

    #[cfg(not(target_os = "macos"))]
    {
        Menu::with_items(app, &[&edit, &view, &window])
    }
}

#[cfg(target_os = "macos")]
fn build_app_menu(app: &AppHandle) -> tauri::Result<Submenu<Wry>> {
    // 版本号取自打包元数据，与前端 `getVersion()` 同源（清单 12）。
    let about = PredefinedMenuItem::about(
        app,
        Some("关于 Halo"),
        Some(tauri::menu::AboutMetadata {
            name: Some("Halo".into()),
            version: Some(app.package_info().version.to_string()),
            ..Default::default()
        }),
    )?;
    let settings = MenuItem::with_id(app, "settings", "设置…", true, Some("CmdOrCtrl+,"))?;
    let services = PredefinedMenuItem::services(app, Some("服务"))?;
    let hide = PredefinedMenuItem::hide(app, Some("隐藏 Halo"))?;
    let hide_others = PredefinedMenuItem::hide_others(app, Some("隐藏其他"))?;
    let show_all = PredefinedMenuItem::show_all(app, Some("全部显示"))?;
    let quit = PredefinedMenuItem::quit(app, Some("退出 Halo"))?;

    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let sep3 = PredefinedMenuItem::separator(app)?;
    let sep4 = PredefinedMenuItem::separator(app)?;

    let items: Vec<&dyn IsMenuItem<Wry>> = vec![
        &about,
        &sep1,
        &settings,
        &sep2,
        &services,
        &sep3,
        &hide,
        &hide_others,
        &show_all,
        &sep4,
        &quit,
    ];
    Submenu::with_items(app, "Halo", true, &items)
}

fn build_edit_menu(app: &AppHandle) -> tauri::Result<Submenu<Wry>> {
    // 自定义项：见文件头注释（自绘列表的撤销必须由我们自己实现）。
    let undo = MenuItem::with_id(app, "undo", "撤销", true, Some("CmdOrCtrl+Z"))?;
    let redo = MenuItem::with_id(app, "redo", "重做", true, Some("CmdOrCtrl+Shift+Z"))?;
    let cut = PredefinedMenuItem::cut(app, Some("剪切"))?;
    let copy = PredefinedMenuItem::copy(app, Some("复制"))?;
    let paste = PredefinedMenuItem::paste(app, Some("粘贴"))?;
    let select_all = PredefinedMenuItem::select_all(app, Some("全选"))?;

    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;

    let items: Vec<&dyn IsMenuItem<Wry>> = vec![
        &undo, &redo, &sep1, &cut, &copy, &paste, &sep2, &select_all,
    ];
    Submenu::with_items(app, "编辑", true, &items)
}

fn build_view_menu(app: &AppHandle) -> tauri::Result<Submenu<Wry>> {
    let panel = MenuItem::with_id(app, "show_panel", "显示剪贴板面板", true, None::<&str>)?;
    let history = MenuItem::with_id(app, "show_history", "打开历史", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "设置…", true, None::<&str>)?;

    let sep = PredefinedMenuItem::separator(app)?;
    let items: Vec<&dyn IsMenuItem<Wry>> = vec![&panel, &history, &sep, &settings];
    Submenu::with_items(app, "显示", true, &items)
}

fn build_window_menu(app: &AppHandle) -> tauri::Result<Submenu<Wry>> {
    let minimize = PredefinedMenuItem::minimize(app, Some("最小化"))?;
    let zoom = PredefinedMenuItem::maximize(app, Some("缩放"))?;
    let fullscreen = PredefinedMenuItem::fullscreen(app, Some("进入全屏幕"))?;
    let close = PredefinedMenuItem::close_window(app, Some("隐藏窗口"))?;

    let sep = PredefinedMenuItem::separator(app)?;
    let items: Vec<&dyn IsMenuItem<Wry>> =
        vec![&minimize, &zoom, &sep, &fullscreen, &close];
    Submenu::with_items(app, "窗口", true, &items)
}

/// 菜单事件：只处理自定义项；其余是系统预定义行为，无需转发。
pub fn handle_event(app: &AppHandle, event: tauri::menu::MenuEvent) {
    match event.id().0.as_str() {
        "settings" => crate::window::show_main_window_with_page(app, Some("settings".into())),
        "show_history" => crate::window::show_main_window_with_page(app, Some("history".into())),
        "show_panel" => crate::window::toggle_panel_window(app),
        // 撤销/重做只在**前台**窗口生效，具体做什么由前端按焦点决定。
        "undo" => emit_to_focused(app, "halo://menu-undo"),
        "redo" => emit_to_focused(app, "halo://menu-redo"),
        _ => {}
    }
}

/// 发给当前聚焦的窗口；没有聚焦窗口就什么都不做。
///
/// 不能在没有聚焦窗口时全局广播：主窗口已隐藏到托盘时广播，会让「看不见的
/// 历史列表」默默撤销一条删除（用户完全无从得知发生了什么）。
fn emit_to_focused(app: &AppHandle, event: &str) {
    let target = app
        .webview_windows()
        .into_iter()
        .find(|(_, w)| w.is_focused().unwrap_or(false))
        .map(|(label, _)| label);
    if let Some(label) = target {
        let _ = app.emit_to(label, event, ());
    }
}
