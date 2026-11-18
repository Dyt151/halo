//! 菜单栏下拉面板：贴着图标定位与焦点处理。

use super::*;
use super::panel::refresh_shadow;



/// 托盘图标的屏幕矩形（物理像素，来自 Tauri 的 `TrayIconEvent::Click`）。
static TRAY_ICON_RECT: Mutex<Option<(f64, f64, f64, f64)>> = Mutex::new(None);
/// 面板刚显示的时间：宽限期内忽略系统补发的失焦事件。
static TRAY_PANEL_SHOWN_AT_MS: AtomicU64 = AtomicU64::new(0);
static TRAY_PANEL_FOCUSED: AtomicBool = AtomicBool::new(false);
/// 最近一次收起的时间：点击图标时「先失焦收起、再收到点击」不能又弹开。
static TRAY_PANEL_HIDDEN_AT_MS: AtomicU64 = AtomicU64::new(0);
/// 面板内容高度（逻辑像素，由前端回报）。
static TRAY_PANEL_HEIGHT: AtomicU64 = AtomicU64::new(360);

/// 与 Status Trio 面板一致：内容 330 宽（左右各 14 内边距）。
const TRAY_PANEL_WIDTH: f64 = 330.0;
const TRAY_PANEL_GAP: f64 = 6.0;
/// 面板与屏幕边缘的最小间距。
const TRAY_PANEL_MARGIN: f64 = 8.0;
/// 超过这个高度由前端自己滚动，窗口不再长高。
const TRAY_PANEL_MAX_HEIGHT: f64 = 720.0;

/// 记录托盘图标矩形（物理像素）。
pub fn set_tray_icon_rect(x: f64, y: f64, width: f64, height: f64) {
    *TRAY_ICON_RECT.lock_ok() = Some((x, y, width, height));
}

/// 把托盘的物理矩形换算成逻辑坐标（找到图标所在的那块屏再除以缩放比）。
fn tray_icon_logical(window: &WebviewWindow) -> Option<(f64, f64, f64, f64)> {
    let (x, y, w, h) = (*TRAY_ICON_RECT.lock_ok())?;
    let monitor = window
        .available_monitors()
        .ok()?
        .into_iter()
        .find(|m| {
            let pos = m.position();
            let size = m.size();
            let left = pos.x as f64;
            let top = pos.y as f64;
            x >= left && x < left + size.width as f64 && y >= top && y < top + size.height as f64
        })
        .or_else(|| window.current_monitor().ok().flatten())
        .or_else(|| window.primary_monitor().ok().flatten())?;
    let scale = monitor.scale_factor().max(1.0);
    Some((x / scale, y / scale, w / scale, h / scale))
}

/// 把面板贴到图标正下方（右对齐图标），并 clamp 进工作区。
fn place_tray_panel(window: &WebviewWindow, height: f64) {
    // 拿不到托盘矩形（例如用快捷键打开）时退回光标位置，至少不留在旧屏。
    let Some((ax, ay, aw, ah)) = tray_icon_logical(window)
        .or_else(|| cursor_logical_position(window).map(|(x, y)| (x, y, 0.0, 0.0)))
    else {
        return;
    };
    let Some(monitor) = monitor_containing(window, (ax, ay))
        .or_else(|| window.current_monitor().ok().flatten())
        .or_else(|| window.primary_monitor().ok().flatten())
    else {
        return;
    };
    let scale = monitor.scale_factor().max(1.0);
    let work = monitor.work_area();
    let origin_x = work.position.x as f64 / scale;
    let origin_y = work.position.y as f64 / scale;
    let avail_w = work.size.width as f64 / scale;
    let avail_h = work.size.height as f64 / scale;

    let width = TRAY_PANEL_WIDTH
        .min(avail_w - TRAY_PANEL_MARGIN * 2.0)
        .max(240.0);
    let height = height
        .clamp(120.0, TRAY_PANEL_MAX_HEIGHT)
        .min((avail_h - TRAY_PANEL_MARGIN * 2.0).max(120.0));

    let max_x = (origin_x + avail_w - width - TRAY_PANEL_MARGIN).max(origin_x + TRAY_PANEL_MARGIN);
    let x = (ax + aw - width).clamp(origin_x + TRAY_PANEL_MARGIN, max_x);
    let max_y = (origin_y + avail_h - height - TRAY_PANEL_MARGIN).max(origin_y + TRAY_PANEL_MARGIN);
    let y = (ay + ah + TRAY_PANEL_GAP).clamp(origin_y + TRAY_PANEL_MARGIN, max_y);

    let _ = window.set_size(Size::Logical(LogicalSize::new(width, height)));
    let _ = window.set_position(Position::Logical(LogicalPosition::new(x, y)));
}

/// 显示状态栏面板（点菜单栏图标）。
pub fn show_tray_panel(app: &AppHandle) {
    let Some(panel) = app.get_webview_window("traypanel") else {
        return;
    };
    place_tray_panel(&panel, TRAY_PANEL_HEIGHT.load(Ordering::SeqCst) as f64);

    TRAY_PANEL_SHOWN_AT_MS.store(now_ms(), Ordering::SeqCst);
    TRAY_PANEL_FOCUSED.store(false, Ordering::SeqCst);
    // 与剪贴板面板同层级：盖过其它 App 的浮层；全屏 Space 里也能出来。
    #[cfg(target_os = "macos")]
    set_window_level(&panel, 25);
    allow_over_fullscreen(&panel);
    // 顺序照抄剪贴板面板：先把宿主排进**当前** Space（全屏 Space 也算），再激活
    // App 抢键盘。反过来先激活，系统会因为 Halo 在当前 Space 上还没有窗口而把
    // 空间切回桌面——全屏里点菜单栏图标就表现为「什么都没发生」。
    #[cfg(target_os = "macos")]
    reveal_panel_host(&panel);
    #[cfg(target_os = "macos")]
    order_front_regardless(&panel);
    // Accessory 应用不主动激活就拿不到键盘（Esc 与滑条都需要）。
    activate_app();
    let _ = panel.show();
    let _ = panel.set_focus();
    refresh_shadow(&panel);

    let _ = app.emit("halo://tray-shown", ());
}

/// 收起状态栏面板。
pub fn hide_tray_panel(app: &AppHandle) {
    if let Some(panel) = app.get_webview_window("traypanel") {
        TRAY_PANEL_FOCUSED.store(false, Ordering::SeqCst);
        // 宿主一并收起，否则当前 Space 里会留一个透明的空窗口。
        conceal_panel_host("traypanel");
        let _ = panel.hide();
        TRAY_PANEL_HIDDEN_AT_MS.store(now_ms(), Ordering::SeqCst);
        let _ = app.emit("halo://tray-hidden", ());
    }
}

/// 点击菜单栏图标：可见则收起，不可见则弹出。
pub fn toggle_tray_panel(app: &AppHandle) {
    let Some(panel) = app.get_webview_window("traypanel") else {
        return;
    };
    if panel.is_visible().unwrap_or(false) {
        hide_tray_panel(app);
        return;
    }
    // 点图标时窗口会先失焦收起：这次点击的语义是「收起」，不能再弹开。
    if now_ms().saturating_sub(TRAY_PANEL_HIDDEN_AT_MS.load(Ordering::SeqCst)) < 350 {
        return;
    }
    show_tray_panel(app);
}

/// 面板失焦即收起（与剪贴板面板同一套宽限 + 复核逻辑）。
pub fn handle_tray_panel_focus_lost(app: &AppHandle) {
    let shown_at = TRAY_PANEL_SHOWN_AT_MS.load(Ordering::SeqCst);
    if shown_at > 0 && now_ms().saturating_sub(shown_at) < FOCUS_GRACE_MS {
        return;
    }
    if !TRAY_PANEL_FOCUSED.swap(false, Ordering::SeqCst) {
        return;
    }
    if let Some(panel) = app.get_webview_window("traypanel") {
        if panel.is_focused().unwrap_or(false) {
            TRAY_PANEL_FOCUSED.store(true, Ordering::SeqCst);
            return;
        }
    }
    hide_tray_panel(app);
}

pub fn handle_tray_panel_focus_gained() {
    TRAY_PANEL_FOCUSED.store(true, Ordering::SeqCst);
}

/// 托盘图标的稳定 id（配置里关掉状态栏图标时按它找回来）。
pub const TRAY_ICON_ID: &str = "halo-status";

/// 配置里的「显示状态栏图标」落地：隐藏/显示托盘图标。
pub fn apply_tray_visibility(app: &AppHandle, visible: bool) {
    if let Some(tray) = app.tray_by_id(TRAY_ICON_ID) {
        let _ = tray.set_visible(visible);
    }
    if !visible {
        hide_tray_panel(app);
    }
}

/// 前端按实际内容高度回报：记下来，可见时立即重排。
pub fn resize_tray_panel(app: &AppHandle, height: f64) {
    let height = height.clamp(120.0, TRAY_PANEL_MAX_HEIGHT).round();
    TRAY_PANEL_HEIGHT.store(height as u64, Ordering::SeqCst);
    if let Some(panel) = app.get_webview_window("traypanel") {
        if panel.is_visible().unwrap_or(false) {
            place_tray_panel(&panel, height);
        }
    }
}

/* ------------------------------------------------------------------ */
/* 输入法切换 HUD                                                       */
/* ------------------------------------------------------------------ */

