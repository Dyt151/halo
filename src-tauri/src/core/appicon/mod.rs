//! 剪贴板来源 App 图标导出（入库时调用）。
//!
//! macOS：`NSWorkspace.sharedWorkspace` → `frontmostApplication` → 其 `icon`
//! （NSImage）→ 经 `NSGraphicsContext` 绘制进 40x40 位图 → `NSBitmapImageRep`
//! `representationUsingType:properties:`（PNG）写文件，输出到
//! `~/.halo/icons/<安全文件名>.png`（同名复用，已存在不重复导出），返回绝对路径。
//! 其他平台：no-op（`source_app_icon` 字段留空）。
//!
//! 导出是尽力而为的旁路：任何一步失败都静默降级为 `None`，绝不影响入库主流程。
//!
//! 实现说明（本机 macOS 实测，Darwin 27）：
//! - App 图标的 `NSImage` 表示全是私有类 `NSISIconImageRep`，不能直接取
//!   `NSBitmapImageRep`，必须绘制栅格化；
//! - 单参 `representationUsingType:` 在运行时已不存在，只有
//!   `representationUsingType:properties:`；
//! - 该方法返回的 data 运行时类是 `OS_dispatch_data`，其 `bytes` 签名与
//!   `NSData` 不同，需用 `length` + `getBytes:length:` 读取。

use crate::storage;

/// 导出当前前台 App 图标为 PNG 文件，返回其绝对路径；失败或无图标时返回 `None`。
pub fn export_frontmost_app_icon() -> Option<std::path::PathBuf> {
    #[cfg(target_os = "macos")]
    {
        macos::export().ok().flatten()
    }
    #[cfg(not(target_os = "macos"))]
    {
        // 其他平台无 App 图标来源，字段留空。
        None
    }
}

/// 纯函数：把 App 名 / bundle id 清洗为安全文件名。
///
/// 规则：保留字母数字与 `.` `_` `-`，其余字符折叠为单个 `-`；去掉首尾的
/// `.` `-` `_`（防止隐藏文件/路径穿越）；限长 64 字符；结果为空时回落 `app`。
pub fn safe_file_name(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len().min(64));
    let mut prev_dash = false;
    for ch in raw.chars() {
        let is_dash = ch == '-';
        if ch.is_alphanumeric() || matches!(ch, '.' | '_' | '-') {
            // 连续 '-' 折叠，避免 "a - - b" 产出 "a-----b"。
            if is_dash && prev_dash {
                continue;
            }
            out.push(ch);
            prev_dash = is_dash;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    let cleaned: String = out
        .trim_matches(|c| c == '-' || c == '.' || c == '_')
        .chars()
        .take(64)
        .collect();
    if cleaned.is_empty() {
        "app".to_string()
    } else {
        cleaned
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use std::ffi::CString;
    use std::path::{Path, PathBuf};

    use objc2::encode::{Encode, Encoding, RefEncode};
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};

    use super::safe_file_name;
    use crate::storage;

    /// `NSBitmapImageFileTypePNG`（AppKit 稳定 ABI）。
    const PNG_FILE_TYPE: usize = 4;
    /// 导出边长：前端 20px 逻辑显示，Retina 2x 下取 40px 物理像素。
    const ICON_EDGE: f64 = 40.0;

    // --- CoreGraphics 几何结构 -------------------------------------------------
    // 与 AppKit 的 NSPoint/NSSize/NSRect ABI 相同；运行时类型元数据用 CG 前缀
    // 名称（本机 AppKit 已统一到 CoreGraphics 类型），故 Encoding 名用 "CG*"。

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGPoint {
        x: f64,
        y: f64,
    }

    unsafe impl Encode for CGPoint {
        const ENCODING: Encoding = Encoding::Struct(
            "CGPoint",
            &[Encoding::Double, Encoding::Double],
        );
    }
    unsafe impl RefEncode for CGPoint {
        const ENCODING_REF: Encoding = <CGPoint as Encode>::ENCODING;
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGSize {
        width: f64,
        height: f64,
    }

    unsafe impl Encode for CGSize {
        const ENCODING: Encoding = Encoding::Struct(
            "CGSize",
            &[Encoding::Double, Encoding::Double],
        );
    }
    unsafe impl RefEncode for CGSize {
        const ENCODING_REF: Encoding = <CGSize as Encode>::ENCODING;
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGRect {
        origin: CGPoint,
        size: CGSize,
    }

    unsafe impl Encode for CGRect {
        const ENCODING: Encoding = Encoding::Struct(
            "CGRect",
            &[
                <CGPoint as Encode>::ENCODING,
                <CGSize as Encode>::ENCODING,
            ],
        );
    }
    unsafe impl RefEncode for CGRect {
        const ENCODING_REF: Encoding = <CGRect as Encode>::ENCODING;
    }

    /// macOS 图标导出入口。返回 `Ok(None)` 表示无前台 App/无图标（正常降级）。
    pub fn export() -> Result<Option<PathBuf>, ()> {
        let icons_dir = storage::icons_dir();
        std::fs::create_dir_all(&icons_dir).map_err(|_| ())?;

        unsafe {
            let pool: *mut AnyObject = msg_send![class!(NSAutoreleasePool), new];

            let ws: *mut AnyObject = msg_send![class!(NSWorkspace), sharedWorkspace];
            let app: *mut AnyObject = msg_send![ws, frontmostApplication];
            let result = if app.is_null() {
                Ok(None)
            } else {
                export_for_app(app, &icons_dir)
            };

            let _: () = msg_send![pool, release];
            result
        }
    }

    /// # Safety
    /// `app` 必须为有效的 `NSRunningApplication` 指针。
    unsafe fn export_for_app(app: *mut AnyObject, icons_dir: &Path) -> Result<Option<PathBuf>, ()> {
        // 文件名主键优先 bundle id（唯一稳定），其次本地化名。
        let key = nsstring_to_string(msg_send![app, bundleIdentifier])
            .or_else(|| nsstring_to_string(msg_send![app, localizedName]))
            .unwrap_or_else(|| "app".to_string());
        let path = icons_dir.join(format!("{}.png", safe_file_name(&key)));

        // 同名复用：已导出过的不重复导出。
        if path.exists() {
            return Ok(Some(path));
        }

        let image: *mut AnyObject = msg_send![app, icon];
        if image.is_null() {
            return Ok(None);
        }
        let Some(png) = draw_icon_png(image) else {
            return Ok(None);
        };
        std::fs::write(&path, &png).map_err(|_| ())?;
        Ok(Some(path))
    }

    /// 把 NSImage 绘制进 `ICON_EDGE` 位图并导出为 PNG 字节。
    ///
    /// # Safety
    /// `image` 必须为有效的 NSImage 指针。
    unsafe fn draw_icon_png(image: *mut AnyObject) -> Option<Vec<u8>> {
        let edge = ICON_EDGE as isize;

        // NSCalibratedRGBColorSpace（设备无关，适合存 PNG）。
        let cs_name_c = CString::new("NSCalibratedRGBColorSpace").ok()?;
        let cs_name: *mut AnyObject =
            msg_send![class!(NSString), stringWithUTF8String: cs_name_c.as_ptr()];

        let allocated: *mut AnyObject = msg_send![class!(NSBitmapImageRep), alloc];
        let rep: *mut AnyObject = msg_send![
            allocated,
            initWithBitmapDataPlanes: std::ptr::null_mut::<*mut u8>(),
            pixelsWide: edge,
            pixelsHigh: edge,
            bitsPerSample: 8isize,
            samplesPerPixel: 4isize,
            hasAlpha: true,
            isPlanar: false,
            colorSpaceName: cs_name,
            bytesPerRow: 0isize,
            bitsPerPixel: 32isize
        ];
        if rep.is_null() {
            return None;
        }

        // 在位图上下文里绘制（AppKit 允许在非主线程绘制到离屏位图）。
        let ctx: *mut AnyObject =
            msg_send![class!(NSGraphicsContext), graphicsContextWithBitmapImageRep: rep];
        if ctx.is_null() {
            return None;
        }
        let previous: *mut AnyObject = msg_send![class!(NSGraphicsContext), currentContext];
        let _: () = msg_send![class!(NSGraphicsContext), setCurrentContext: ctx];
        let rect = CGRect {
            origin: CGPoint { x: 0.0, y: 0.0 },
            size: CGSize {
                width: ICON_EDGE,
                height: ICON_EDGE,
            },
        };
        let _: () = msg_send![image, drawInRect: rect];
        let _: () = msg_send![class!(NSGraphicsContext), setCurrentContext: previous];

        rep_to_png(rep)
    }

    /// 将 NSBitmapImageRep 导出为 PNG 字节。
    ///
    /// 注意：本机 macOS 上单参 `representationUsingType:` 已不存在，必须走
    /// `representationUsingType:properties:`；返回的 data 运行时类为
    /// `OS_dispatch_data`，`bytes` 签名与 NSData 不同，故用 `getBytes:length:`。
    ///
    /// # Safety
    /// `rep` 必须为有效的 NSBitmapImageRep 指针。
    unsafe fn rep_to_png(rep: *mut AnyObject) -> Option<Vec<u8>> {
        let data: *mut AnyObject = msg_send![
            rep,
            representationUsingType: PNG_FILE_TYPE,
            properties: std::ptr::null_mut::<AnyObject>()
        ];
        if data.is_null() {
            return None;
        }
        let len: usize = msg_send![data, length];
        if len == 0 {
            return None;
        }
        let mut buf = vec![0u8; len];
        let _: () = msg_send![
            data,
            getBytes: buf.as_mut_ptr() as *mut std::ffi::c_void,
            length: len
        ];
        Some(buf)
    }

    /// 将 NSString 指针转换为 Rust `String`（null 返回 None）。
    ///
    /// # Safety
    /// `ns` 必须为有效的 NSString 指针或 null。
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

#[cfg(test)]
mod tests {
    use super::safe_file_name;

    #[test]
    fn keeps_plain_app_name() {
        assert_eq!(safe_file_name("Safari"), "Safari");
    }

    #[test]
    fn keeps_bundle_id_verbatim() {
        assert_eq!(safe_file_name("com.apple.Safari"), "com.apple.Safari");
    }

    #[test]
    fn replaces_illegal_characters() {
        assert_eq!(safe_file_name("My App!"), "My-App");
        assert_eq!(safe_file_name("a/b\\c:d"), "a-b-c-d");
        assert_eq!(safe_file_name("app   name"), "app-name");
    }

    #[test]
    fn collapses_repeated_illegal_runs() {
        assert_eq!(safe_file_name("a///b"), "a-b");
        assert_eq!(safe_file_name("a - - b"), "a-b");
    }

    #[test]
    fn strips_leading_and_trailing_separators() {
        // 典型路径穿越输入：不得含 '/'，不得以 '.' 开头。
        let name = safe_file_name("../../etc/passwd");
        assert_eq!(name, "etc-passwd");
        assert!(!name.contains('/'));
        assert!(!name.starts_with('.'));
    }

    #[test]
    fn keeps_unicode_app_name() {
        assert_eq!(safe_file_name("微 信"), "微-信");
        assert_eq!(safe_file_name("微信"), "微信");
    }

    #[test]
    fn falls_back_when_nothing_usable() {
        assert_eq!(safe_file_name(""), "app");
        assert_eq!(safe_file_name("   "), "app");
        assert_eq!(safe_file_name("///"), "app");
    }

    #[test]
    fn limits_length() {
        let long = "a".repeat(200);
        assert_eq!(safe_file_name(&long).len(), 64);
    }

    #[test]
    fn result_is_usable_as_file_name() {
        for raw in [
            "Safari",
            "com.apple.Safari",
            "../../etc/passwd",
            "Visual Studio Code",
            "微 信",
            "app:v1.2",
            "",
        ] {
            let name = safe_file_name(raw);
            assert!(!name.is_empty());
            assert!(!name.contains('/'));
            assert!(!name.contains('\\'));
            assert!(!name.starts_with('.'));
            assert!(name.len() <= 64);
        }
    }
}
