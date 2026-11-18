//! 环形转换盘与回复选择器等浮动面板。

use super::*;


/// 环形盘边长（逻辑像素）：单层最多 12 个扇区，边长按几何 `2×(R+PopOut+阴影14)`
/// 推算（R=150 → 342），见 README「三、环形文件转换」（评审 C3 / 任务 20）。
const RING_SIZE: f64 = 342.0;

/// 环形窗当前的左上角（屏幕逻辑坐标）。
///
/// 拖放模式下窗口不接收鼠标（`ignore_cursor_events`），指针位置只能从全局手势旁听，
/// 在这里换成窗口内坐标再发给前端——比前端异步读窗口位置可靠。
static RING_WINDOW_ORIGIN: Mutex<Option<(f64, f64)>> = Mutex::new(None);

/// 环形窗左上角的屏幕坐标（不在拖放模式时为 `None`）。
pub fn ring_window_origin() -> Option<(f64, f64)> {
    *RING_WINDOW_ORIGIN.lock_ok()
}

/// 显示环形转换盘：钉在拖动起手点，且**从不抢焦点**。
///
/// 运行态只有 Option＋拖动一个入口（Filee 行为），所以定位只有「起手点」
/// 一种：拖动的是 Finder，抢焦点会打断它的鼠标追踪（松手事件就回不去了）。
/// 鼠标事件也必须穿过去：否则松手会投递给环形窗，Finder 会卡在拖拽状态。
pub fn show_ring_window(app: &AppHandle, origin: (f64, f64)) {
    let Some(ring) = app.get_webview_window("ring") else {
        tracing::warn!("环形盘窗口不存在，无法显示");
        return;
    };
    let (cx, cy) = origin;
    let placed = place_float_window_at(&ring, RING_SIZE, RING_SIZE, cx, cy);
    *RING_WINDOW_ORIGIN.lock_ok() = placed;
    set_float_level(&ring);
    allow_over_fullscreen(&ring);
    let _ = ring.set_ignore_cursor_events(true);
    if let Err(e) = ring.show() {
        tracing::warn!("显示环形盘失败: {e}");
    }
    tracing::info!(x = cx, y = cy, "环形盘窗口已显示");
    let _ = app.emit("halo://ring-shown", ());
}

/// 隐藏环形转换盘。
pub fn hide_ring_window(app: &AppHandle) {
    *RING_WINDOW_ORIGIN.lock_ok() = None;
    if let Some(ring) = app.get_webview_window("ring") {
        let _ = ring.hide();
    }
}

/// 浮动面板层级：与剪贴板面板一致（NSStatusWindowLevel=25），盖过其它 App 浮层。
#[cfg(target_os = "macos")]
fn set_float_level(window: &WebviewWindow) {
    set_window_level(window, 25);
}
#[cfg(not(target_os = "macos"))]
fn set_float_level(_window: &WebviewWindow) {}

/// 以屏幕逻辑坐标 `(cx, cy)` 为中心放置浮动窗口（Option+拖动时钉在起手点）。
///
/// 返回窗口最终的左上角（已做工作区 clamp），供指针坐标换算。
pub fn place_float_window_at(
    window: &WebviewWindow,
    w: f64,
    h: f64,
    cx: f64,
    cy: f64,
) -> Option<(f64, f64)> {
    // 按**落点**选屏而不是按窗口原位置：环形盘可能上一秒还在另一块屏上。
    let monitor = monitor_containing(window, (cx, cy))
        .or_else(|| window.current_monitor().ok().flatten())
        .or_else(|| window.primary_monitor().ok().flatten())?;
    let scale = monitor.scale_factor().max(1.0);
    let work = monitor.work_area();
    let origin_x = work.position.x as f64 / scale;
    let origin_y = work.position.y as f64 / scale;
    let avail_w = work.size.width as f64 / scale;
    let avail_h = work.size.height as f64 / scale;

    // 以给定点为中心展开；越界时向工作区内 clamp（等价于边缘翻转/平移）。
    let raw_x = cx - w / 2.0;
    let raw_y = cy - h / 2.0;
    let x = raw_x.clamp(
        origin_x + FLOAT_MARGIN,
        (origin_x + avail_w - w - FLOAT_MARGIN).max(origin_x + FLOAT_MARGIN),
    );
    let y = raw_y.clamp(
        origin_y + FLOAT_MARGIN,
        (origin_y + avail_h - h - FLOAT_MARGIN).max(origin_y + FLOAT_MARGIN),
    );

    let _ = window.set_size(Size::Logical(LogicalSize::new(w, h)));
    let _ = window.set_position(Position::Logical(LogicalPosition::new(x, y)));
    Some((x, y))
}

/// 把焦点归还「唤出浮动面板前」的前台 App（环形盘执行/取消后回焦 Finder，任务 25）。
///
/// 与 `paste::restore_frontmost_app` 同源，但这里是通用版本：浮动面板（环形盘）
/// 结束时要还给 Finder，而不是只有回贴路径才归还。
#[cfg(target_os = "macos")]
pub fn restore_frontmost_app() {
    use objc2::msg_send;
    use objc2::runtime::{AnyObject, Bool};

    let pid = pre_active_app_pid();
    if pid <= 0 || pid == std::process::id() as i32 {
        return;
    }
    unsafe {
        // `runningApplicationWithProcessIdentifier:` 是 NSRunningApplication 的
        // **类方法**（`+[NSRunningApplication ...]`），不是 NSWorkspace 的实例方法：
        // 发错接收者会撞 objc2 的选择子校验（panic "method not found"），而 release
        // 构建是 panic=abort，整个进程会直接 abort（真机上表现为「转完就消失」）。
        let app: *mut AnyObject = msg_send![
            objc2::class!(NSRunningApplication),
            runningApplicationWithProcessIdentifier: pid
        ];
        if app.is_null() {
            return;
        }
        // NSApplicationActivateAllWindows(1)。不再叠加 IgnoringOtherApps(2)：
        // macOS 14 起该位已废弃且被系统忽略，传了只会误导后来者。
        let options: usize = 1;
        let activated: Bool = msg_send![app, activateWithOptions: options];
        if !activated.as_bool() {
            tracing::debug!(
                pid,
                "归还焦点失败：系统拒绝了本次 activate（目标 App 可能已退出）"
            );
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub fn restore_frontmost_app() {}

/// 按配置应用面板高度百分比（`board_height_scale`）与出现位置（`board_position`）。
///
/// 坐标一律以**当前显示器**为基准（副屏原点不为 0，不减原点会把面板推到屏外）：
/// 贴底停靠用完整屏幕矩形（要压住程序坞），其余两种用工作区矩形 + [`panel_width`]。
pub(super) fn apply_panel_layout(window: &WebviewWindow, config: &Config) {
    // 按光标选屏：热键是在用户正在看的那块屏上按下的，隐藏中的面板仍停在旧屏，
    // 用 `current_monitor()` 会把它唤到另一台显示器上。
    let Some(monitor) = monitor_at_cursor(window) else {
        return;
    };
    let scale = monitor.scale_factor().max(1.0);
    let work = monitor.work_area();
    let origin_x = work.position.x as f64 / scale;
    let origin_y = work.position.y as f64 / scale;
    let avail_width = work.size.width as f64 / scale;
    let avail_height = work.size.height as f64 / scale;
    // 屏幕物理范围（含程序坞占用区）。贴底停靠按它算，保持面板原地不动：
    // 程序坞盖住面板靠窗口层级压过去，而不是把面板往上挪。
    let screen_pos = monitor.position();
    let screen_size = monitor.size();
    let screen_x = screen_pos.x as f64 / scale;
    let screen_y = screen_pos.y as f64 / scale;
    let screen_width = screen_size.width as f64 / scale;
    let screen_height = screen_size.height as f64 / scale;

    // 面板高度 = 屏幕可用高度的 board_height_scale%，上下限保证可用性。
    // 下限 220：默认行列表的固定 chrome 是 40 工具栏 + 30 标签行 + 24 状态栏，
    // 220 仍能放下约 2.8 行；再低就连三行历史都看不到。
    let max_height = (avail_height - PANEL_MARGIN * 2.0).max(220.0);
    let height = (avail_height * config.board_height_scale.clamp(10, 60) as f64 / 100.0)
        .round()
        .max(220.0)
        .min(max_height);
    let width = panel_width(avail_width);

    let centered_x = origin_x + (avail_width - width) / 2.0;
    let centered_y = origin_y + (avail_height - height) / 2.0;

    let (x, y, w) = match config.board_position.as_str() {
        // 贴底停靠：横向铺满屏幕，下边缘贴屏幕物理底边（原位置，不要挪）。
        // 程序坞露在桌面上时确实会压到这里，但那由窗口层级解决：面板在
        // reveal 时被抬到 NSStatusWindowLevel(25)，高于程序坞的 20。
        "bottom" => (screen_x, screen_y + screen_height - height, screen_width),
        // 屏幕居中。
        "screen" => (centered_x, centered_y, width),
        _ => {
            // follow_cursor：左边缘对齐光标、下方 8pt；越界则回收到工作区内。
            match cursor_logical_position(window) {
                Some((cx, cy)) => (
                    cx.clamp(origin_x, origin_x + (avail_width - width).max(0.0)),
                    (cy + 8.0).clamp(origin_y, origin_y + (avail_height - height).max(0.0)),
                    width,
                ),
                None => (centered_x, centered_y, width),
            }
        }
    };

    let _ = window.set_size(Size::Logical(LogicalSize::new(w, height)));
    let _ = window.set_position(Position::Logical(LogicalPosition::new(x, y)));
}

/// 面板宽度（居中类布局用）：工作区的 60%，收敛到 [`PANEL_MAX_WIDTH`]，
/// 且始终留出 [`PANEL_MARGIN`] 边距。贴底停靠不用这个宽度，直接占满整屏。
fn panel_width(avail_width: f64) -> f64 {
    let usable = (avail_width - PANEL_MARGIN * 2.0).max(240.0);
    (avail_width * 0.6)
        .clamp(240.0, PANEL_MAX_WIDTH)
        .min(usable)
}
