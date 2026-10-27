//! macOS 剪贴板端口实现：`objc2` 轮询 `NSPasteboard.changeCount` + `arboard` 读写。
//!
//! macOS 无剪贴板变更通知 API，唯一可靠路径是轮询 `changeCount`（Maccy/Paste 同款）。
//! 读写走 `arboard`（跨平台事实标准），监控走 `objc2` 原始消息发送。
//!
//! 图片：读回的是 RGBA8 像素（arboard 在 macOS 侧只认 TIFF 表示），落盘为
//! **内容寻址**的 PNG（`clip_<sha256 前 16 字节>.png`）——同一张图永远同一路径，
//! 与 `types::hash_content` 的去重语义一致；写回时经 `image` crate 解码后
//! `set_image` 写 NSImage（TIFF/PNG 表示）。

use std::ffi::CString;

use arboard::Clipboard;
use objc2::runtime::AnyObject;
use objc2::{class, msg_send};

use super::{ClipError, ClipboardPort};
use crate::core::clipboard::types::ClipContent;

pub struct MacClipboardPort;

impl MacClipboardPort {
    pub fn new() -> Self {
        Self
    }

    /// 获取 `NSPasteboard.generalPasteboard` 的原始指针。
    fn pasteboard() -> *mut AnyObject {
        unsafe { msg_send![class!(NSPasteboard), generalPasteboard] }
    }

    /// 将 NSString 指针转换为 Rust `String`。
    ///
    /// # Safety
    /// `ns` 必须为有效的 NSString 指针（可为 null，null 返回 None）。
    unsafe fn nsstring_to_string(ns: *mut AnyObject) -> Option<String> {
        if ns.is_null() {
            return None;
        }
        let utf8: *const std::os::raw::c_char = msg_send![ns, UTF8String];
        if utf8.is_null() {
            return None;
        }
        // NSUTF8StringEncoding = 4
        let len: usize = msg_send![ns, lengthOfBytesUsingEncoding: 4usize];
        let bytes = std::slice::from_raw_parts(utf8 as *const u8, len);
        Some(String::from_utf8_lossy(bytes).into_owned())
    }
}

impl Default for MacClipboardPort {
    fn default() -> Self {
        Self::new()
    }
}

impl ClipboardPort for MacClipboardPort {
    fn read(&self) -> Result<ClipContent, ClipError> {
        let mut clipboard =
            Clipboard::new().map_err(|e| ClipError::Operation(e.to_string()))?;

        // 按优先级读取：文本 → HTML → 图片（P0 仅文本 + HTML）。
        match clipboard.get_text() {
            Ok(text) if !text.trim().is_empty() => return Ok(ClipContent::Text(text)),
            _ => {}
        }

        match clipboard.get().html() {
            Ok(html) if !html.trim().is_empty() => return Ok(ClipContent::Html(html)),
            _ => {}
        }

        // 文件列表（P1，数据模型已预留）。
        match clipboard.get().file_list() {
            Ok(files) if !files.is_empty() => {
                let paths = files
                    .iter()
                    .map(|p| p.to_string_lossy().into_owned())
                    .collect();
                return Ok(ClipContent::Files(paths));
            }
            _ => {}
        }

        // 图片：放在文件列表之后——Finder 复制的图片文件同时带 file-url，
        // 应记为「文件」而非「图片内容」（对标 iCopy 的 pastetype 语义）。
        if let Ok(img) = clipboard.get().image() {
            let (width, height) = (img.width as u32, img.height as u32);
            let path = save_image_png(img.bytes.into_owned(), width, height)?;
            return Ok(ClipContent::Image {
                path,
                width,
                height,
            });
        }

        Err(ClipError::UnsupportedType)
    }

    fn write(&self, content: &ClipContent) -> Result<(), ClipError> {
        let mut clipboard =
            Clipboard::new().map_err(|e| ClipError::Operation(e.to_string()))?;
        match content {
            ClipContent::Text(text) => clipboard
                .set_text(text.clone())
                .map_err(|e| ClipError::Operation(e.to_string())),
            ClipContent::Html(html) => {
                let fallback = crate::core::clipboard::types::ClipContent::Html(html.clone())
                    .preview(1024);
                clipboard
                    .set_html(html.clone(), Some(fallback))
                    .map_err(|e| ClipError::Operation(e.to_string()))
            }
            // 文件列表：写入 NSPasteboard 文件 URL 列表（多文件粘贴）。
            ClipContent::Files(files) => {
                let paths: Vec<&std::path::Path> =
                    files.iter().map(std::path::Path::new).collect();
                clipboard
                    .set()
                    .file_list(&paths)
                    .map_err(|e| ClipError::Operation(e.to_string()))
            }
            // 图片：解码后写 NSImage（arboard 用 CGImageAlphaInfo::Last，直通 alpha）。
            // 注意 GIF/WebP 动画只能贴首帧；要「贴成文件」需后续补 NSPasteboard 双写。
            ClipContent::Image { path, .. } => {
                let img = image::open(path)
                    .map_err(|e| ClipError::Operation(format!("open image `{path}`: {e}")))?;
                let rgba = img.to_rgba8();
                let (width, height) = rgba.dimensions();
                clipboard
                    .set_image(arboard::ImageData {
                        width: width as usize,
                        height: height as usize,
                        bytes: std::borrow::Cow::Owned(rgba.into_raw()),
                    })
                    .map_err(|e| ClipError::Operation(e.to_string()))
            }
        }
    }

    fn change_count(&self) -> Result<u64, ClipError> {
        // 在 NSAutoreleasePool 内执行，避免后台线程 autorelease 对象泄漏。
        unsafe {
            let pool: *mut AnyObject = msg_send![class!(NSAutoreleasePool), new];
            let pb = Self::pasteboard();
            let count: isize = msg_send![pb, changeCount];
            let _: () = msg_send![pool, release];
            Ok(count as u64)
        }
    }

    fn frontmost_app(&self) -> Option<String> {
        unsafe {
            let pool: *mut AnyObject = msg_send![class!(NSAutoreleasePool), new];
            let ws: *mut AnyObject = msg_send![class!(NSWorkspace), sharedWorkspace];
            let app: *mut AnyObject = msg_send![ws, frontmostApplication];
            let name: *mut AnyObject = msg_send![app, localizedName];
            let result = Self::nsstring_to_string(name);
            let _: () = msg_send![pool, release];
            result
        }
    }
}

/// 读取指定类型粘贴板字符串（HTML 用，保留备用）。
#[allow(dead_code)]
fn read_pasteboard_string(pasteboard_type: &str) -> Option<String> {
    unsafe {
        let c = CString::new(pasteboard_type).ok()?;
        let pb = MacClipboardPort::pasteboard();
        let type_ns: *mut AnyObject =
            msg_send![class!(NSString), stringWithUTF8String: c.as_ptr()];
        let content: *mut AnyObject = msg_send![pb, stringForType: type_ns];
        MacClipboardPort::nsstring_to_string(content)
    }
}

/// 把 RGBA8 像素编码为 PNG 存入 `data/files/`。
///
/// 文件名按像素内容 sha256 前 16 字节命名（内容寻址）：同一张图永远同一路径，
/// 使 `ClipContent::Image` 的 hash 只依赖内容，历史去重因此成立。
fn save_image_png(rgba: Vec<u8>, width: u32, height: u32) -> Result<String, ClipError> {
    use sha2::{Digest, Sha256};

    let digest = Sha256::digest(&rgba);
    let mut name = String::with_capacity(16 + 4);
    for byte in &digest[..16] {
        name.push_str(&format!("{byte:02x}"));
    }
    name.push_str(".png");

    let dir = crate::storage::files_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|e| ClipError::Operation(format!("create files dir: {e}")))?;
    let path = dir.join(format!("clip_{name}"));
    // 已有同内容图片（如重复复制）直接复用，避免重复编码。
    if path.is_file() {
        return Ok(path.to_string_lossy().into_owned());
    }
    let img = image::RgbaImage::from_raw(width, height, rgba)
        .ok_or_else(|| ClipError::Operation("invalid RGBA buffer".to_string()))?;
    img.save(&path)
        .map_err(|e| ClipError::Operation(format!("save image `{path:?}`: {e}")))?;
    Ok(path.to_string_lossy().into_owned())
}
