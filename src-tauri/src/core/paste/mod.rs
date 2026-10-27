//! 回贴服务：写回剪贴板 + 模拟粘贴（enigo），含 macOS 辅助功能权限检测与降级。

use serde::Serialize;

use super::clipboard::platform::{platform_port, ClipboardPort};
use super::clipboard::types::ClipContent;
use super::clipboard::ClipboardMonitor;
use super::history::HistoryService;
use crate::config::ConfigService;
use crate::error::{AppError, CommandResult};

/// 回贴结果。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum PasteOutcome {
    /// 已写回剪贴板并触发粘贴。
    Pasted,
    /// 无辅助功能权限，仅复制到剪贴板。
    CopiedOnly { reason: String },
}

/// macOS 辅助功能权限检测。
///
/// 对外公开：权限中心（清单 42）需要把「为什么回贴只复制不粘贴」提前说清楚，
/// 而不是等用户发现回贴失效才去猜。
#[cfg(target_os = "macos")]
pub fn has_accessibility_permission() -> bool {
    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrusted() -> u8;
    }
    unsafe { AXIsProcessTrusted() != 0 }
}

#[cfg(not(target_os = "macos"))]
pub fn has_accessibility_permission() -> bool {
    true
}

/// 主动请求辅助功能权限。
///
/// 为什么不能只靠「打开系统设置」：授权面板是白名单式的，列表里没有 ClipBase
/// 时用户得自己点 `+` 去 `target/release/bundle/...` 里找 app（找不到就以为坏了）。
/// `AXIsProcessTrustedWithOptions` 带 prompt 会由系统把本 App 直接加进列表并
/// 弹窗提示，用户只需勾选。
///
/// 只应在用户主动点击时调用：带 prompt 的调用每次都会弹窗。
#[cfg(target_os = "macos")]
pub fn request_accessibility_permission() -> bool {
    use std::ffi::c_void;

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrustedWithOptions(options: *const c_void) -> u8;
        static kAXTrustedCheckOptionPrompt: *const c_void;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFDictionaryCreate(
            allocator: *const c_void,
            keys: *const *const c_void,
            values: *const *const c_void,
            num_values: isize,
            key_callbacks: *const c_void,
            value_callbacks: *const c_void,
        ) -> *const c_void;
        fn CFRelease(value: *const c_void);
        static kCFBooleanTrue: *const c_void;
        // 只取地址，不解引用：类型是 CFDictionaryKeyCallBacks 结构体。
        static kCFTypeDictionaryKeyCallBacks: u8;
        static kCFTypeDictionaryValueCallBacks: u8;
    }

    unsafe {
        let keys = [kAXTrustedCheckOptionPrompt];
        let values = [kCFBooleanTrue];
        let options = CFDictionaryCreate(
            std::ptr::null(),
            keys.as_ptr(),
            values.as_ptr(),
            1,
            std::ptr::addr_of!(kCFTypeDictionaryKeyCallBacks).cast(),
            std::ptr::addr_of!(kCFTypeDictionaryValueCallBacks).cast(),
        );
        if options.is_null() {
            return has_accessibility_permission();
        }
        let trusted = AXIsProcessTrustedWithOptions(options) != 0;
        CFRelease(options);
        trusted
    }
}

#[cfg(not(target_os = "macos"))]
pub fn request_accessibility_permission() -> bool {
    true
}

#[derive(Clone)]
pub struct PasteService {
    history: HistoryService,
    config: ConfigService,
    port: std::sync::Arc<dyn ClipboardPort>,
    /// 用于登记「这次写剪贴板是自己干的」，避免监控器把回声当成新内容入库。
    monitor: ClipboardMonitor,
}

impl PasteService {
    pub fn new(history: HistoryService, config: ConfigService, monitor: ClipboardMonitor) -> Self {
        Self {
            history,
            config,
            port: std::sync::Arc::from(platform_port()),
            monitor,
        }
    }

    /// 将历史条目写回剪贴板（不触发粘贴，保留原始富文本）。
    pub fn copy_to_clipboard(&self, id: i64) -> CommandResult<ClipContent> {
        self.write_to_clipboard(id, false)
    }

    /// 回贴：写回剪贴板 + 模拟 `Cmd+V`（无权限时降级为仅复制）。
    /// 若 `paste_as_plain` 开启，丢弃富文本/HTML，仅写入纯文本。
    pub fn paste(&self, id: i64) -> CommandResult<PasteOutcome> {
        let as_plain = self.config.get().paste_as_plain;
        self.write_to_clipboard(id, as_plain)?;
        self.trigger_or_copied_only()
    }

    /// 将任意内容写入剪贴板并模拟 `Cmd+V`（快捷回复等非历史内容复用）。
    pub fn paste_content(&self, content: &ClipContent) -> CommandResult<PasteOutcome> {
        self.write_port(content)?;
        self.trigger_or_copied_only()
    }

    /// 触发粘贴快捷键（供快捷键「快速回贴」复用）。
    pub fn trigger_paste(&self) -> CommandResult<()> {
        trigger_paste().map_err(|e| AppError::with_details(1003, "无法模拟粘贴，请检查辅助功能权限", e.to_string()))
    }

    /// 已写入剪贴板后的统一收尾：无辅助功能权限 / 注入失败时降级为仅复制。
    fn trigger_or_copied_only(&self) -> CommandResult<PasteOutcome> {
        if !has_accessibility_permission() {
            return Ok(PasteOutcome::CopiedOnly {
                reason: "未授予「辅助功能」权限，只能复制到剪贴板".to_string(),
            });
        }

        match trigger_paste() {
            Ok(()) => Ok(PasteOutcome::Pasted),
            Err(e) => {
                // 内容已经在剪贴板里了：降级成「仅复制」而不是报错。
                // 报错会让用户以为回贴彻底失败，其实手动 ⌘V 就能贴上。
                tracing::warn!("trigger_paste failed: {e}");
                Ok(PasteOutcome::CopiedOnly {
                    reason: format!("自动粘贴失败（{e}），已复制到剪贴板"),
                })
            }
        }
    }

    /// 读取历史并写回剪贴板；`as_plain` 为 true 时先规约为纯文本。
    fn write_to_clipboard(&self, id: i64, as_plain: bool) -> CommandResult<ClipContent> {
        let item = self.history.get(id)?;
        let mut content = if as_plain {
            plain_content(&item.content)
        } else {
            item.content.clone()
        };
        // 对标 iCopy：写回前去除文本尾部空白/换行（避免回贴带一串多余空行）。
        if self.config.get().paste_trim_trailing {
            content = trim_trailing_whitespace(content);
        }
        self.write_port(&content)?;
        // 只更新「最近使用时间」：写剪贴板已经成功了，这里即使失败也不该
        // 把一次成功的回贴报成失败（用户会看到「失败」但其实已经贴上了）。
        if let Err(e) = self.history.touch(id) {
            tracing::warn!("更新最近使用时间失败（id={id}）：{e}");
        }
        Ok(content)
    }

    /// 写剪贴板的唯一入口：先向监控器登记指纹，再真正写。
    ///
    /// 顺序不能颠倒——写入会让 `changeCount` 立刻 +1，若先写后登记，
    /// 快速轮询（最低 100ms）可能已经抢在登记前把回声当作新内容抓走。
    fn write_port(&self, content: &ClipContent) -> CommandResult<()> {
        self.monitor.mark_self_write(content);
        self.port
            .write(content)
            .map_err(|e| AppError::with_details(1002, "写入剪贴板失败", e.to_string()))
    }
}

/// 去除文本尾部空白与换行（空格/制表符/换行）。非文本内容原样返回。
pub fn trim_trailing_whitespace(content: ClipContent) -> ClipContent {
    match content {
        ClipContent::Text(t) => ClipContent::Text(t.trim_end().to_string()),
        other => other,
    }
}

/// 将内容规约为纯文本：HTML 去标签、文件列表拼接路径，文本原样保留。
pub fn plain_content(content: &ClipContent) -> ClipContent {
    match content {
        ClipContent::Html(_) => ClipContent::Text(content.search_text()),
        ClipContent::Files(files) => ClipContent::Text(files.join("\n")),
        // 文本已是纯文本；图片无法规约为文本，原样保留。
        ClipContent::Text(_) | ClipContent::Image { .. } => content.clone(),
    }
}

/// 使用 enigo 注入 `Cmd+V`（macOS）/ `Ctrl+V`（其他平台）。
fn trigger_paste() -> Result<(), String> {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};

    // 先把焦点还给「唤出面板前」的前台 App，否则 ⌘V 会贴进 ClipBase 自己。
    restore_frontmost_app();

    let mut enigo = Enigo::new(&Settings::default()).map_err(|e| e.to_string())?;

    #[cfg(target_os = "macos")]
    let modifier = Key::Meta;
    #[cfg(not(target_os = "macos"))]
    let modifier = Key::Control;

    enigo.key(modifier, Direction::Press).map_err(|e| e.to_string())?;
    let click = enigo.key(Key::Unicode('v'), Direction::Click);
    // 无论按下 v 成不成功，修饰键都必须松开：否则目标 App 会一直以为 ⌘ 被按住，
    // 后续所有快捷键全变成组合键（用户只能拔键盘才能恢复）。
    let release = enigo.key(modifier, Direction::Release);
    click.map_err(|e| e.to_string())?;
    release.map_err(|e| e.to_string())?;
    Ok(())
}

/// 重新激活「唤出面板前」的前台 App（对标 iCopy `preActiveApp` + `activateWithOptions:`）。
#[cfg(target_os = "macos")]
fn restore_frontmost_app() {
    use objc2::msg_send;
    use objc2::runtime::{AnyObject, Bool};

    let pid = crate::window::pre_active_app_pid();
    if pid <= 0 {
        return;
    }
    unsafe {
        // `runningApplicationWithProcessIdentifier:` 是 NSRunningApplication 的类方法：
        // 发给 NSWorkspace 实例会撞 objc2 的选择子校验并直接 abort（见 window.rs 同名函数）。
        let app: *mut AnyObject = msg_send![
            objc2::class!(NSRunningApplication),
            runningApplicationWithProcessIdentifier: pid
        ];
        if app.is_null() {
            return;
        }
        // NSApplicationActivateAllWindows(1)（IgnoringOtherApps 在 macOS 14 起已失效）。
        let options: usize = 1;
        let _: Bool = msg_send![app, activateWithOptions: options];
    }
    // 激活是异步的，稍等让目标 App 成为前台后再注入 ⌘V。
    std::thread::sleep(std::time::Duration::from_millis(60));
}

#[cfg(not(target_os = "macos"))]
fn restore_frontmost_app() {
    // 非 macOS 平台由 enigo 直接注入，暂不处理焦点归还。
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_content_strips_html() {
        let html = ClipContent::Html("<p>Hello <b>world</b></p>".to_string());
        match plain_content(&html) {
            ClipContent::Text(t) => assert_eq!(t, "Hello world"),
            other => panic!("expected plain text, got {other:?}"),
        }
    }

    #[test]
    fn plain_content_keeps_text() {
        let text = ClipContent::Text("  plain text\nwith newline  ".to_string());
        assert_eq!(plain_content(&text), text);
    }

    #[test]
    fn plain_content_joins_files() {
        let files = ClipContent::Files(vec!["/a.txt".to_string(), "/b.txt".to_string()]);
        match plain_content(&files) {
            ClipContent::Text(t) => assert_eq!(t, "/a.txt\n/b.txt"),
            other => panic!("expected plain text, got {other:?}"),
        }
    }

    #[test]
    fn plain_content_keeps_image_unchanged() {
        let image = ClipContent::Image {
            path: "/pic.png".to_string(),
            width: 10,
            height: 20,
        };
        assert_eq!(plain_content(&image), image);
    }

    #[test]
    fn trim_trailing_whitespace_trims_text() {
        let text = ClipContent::Text("  hello\nworld\n\n  ".to_string());
        assert_eq!(
            trim_trailing_whitespace(text),
            ClipContent::Text("  hello\nworld".to_string())
        );
    }

    #[test]
    fn trim_trailing_whitespace_keeps_non_text() {
        let image = ClipContent::Image {
            path: "/pic.png".to_string(),
            width: 10,
            height: 20,
        };
        assert_eq!(trim_trailing_whitespace(image.clone()), image);
    }
}
