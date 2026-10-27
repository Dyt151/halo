//! Windows 剪贴板端口（预留）：arboard 读写 + 内容指纹变化检测。

#![cfg(target_os = "windows")]

use arboard::Clipboard;
use sha2::{Digest, Sha256};

use super::{ClipError, ClipboardPort};
use crate::core::clipboard::types::ClipContent;

pub struct WindowsClipboardPort;

impl WindowsClipboardPort {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WindowsClipboardPort {
    fn default() -> Self {
        Self::new()
    }
}

impl ClipboardPort for WindowsClipboardPort {
    fn read(&self) -> Result<ClipContent, ClipError> {
        let mut clipboard =
            Clipboard::new().map_err(|e| ClipError::Operation(e.to_string()))?;
        match clipboard.get_text() {
            Ok(text) if !text.trim().is_empty() => Ok(ClipContent::Text(text)),
            _ => match clipboard.get().html() {
                Ok(html) if !html.trim().is_empty() => Ok(ClipContent::Html(html)),
                _ => Err(ClipError::UnsupportedType),
            },
        }
    }

    fn write(&self, content: &ClipContent) -> Result<(), ClipError> {
        let mut clipboard =
            Clipboard::new().map_err(|e| ClipError::Operation(e.to_string()))?;
        match content {
            ClipContent::Text(text) => clipboard
                .set_text(text.clone())
                .map_err(|e| ClipError::Operation(e.to_string())),
            ClipContent::Html(html) => clipboard
                .set_html(html.clone(), Some(html.clone()))
                .map_err(|e| ClipError::Operation(e.to_string())),
            // 文件列表：写入系统多文件剪贴板格式（DROPFILES）。
            ClipContent::Files(files) => {
                let paths: Vec<&std::path::Path> =
                    files.iter().map(std::path::Path::new).collect();
                clipboard
                    .set()
                    .file_list(&paths)
                    .map_err(|e| ClipError::Operation(e.to_string()))
            }
            ClipContent::Image { .. } => Err(ClipError::UnsupportedType),
        }
    }

    fn change_count(&self) -> Result<u64, ClipError> {
        // 内容指纹：取 sha256 前 8 字节作为变化计数器。
        let mut hasher = Sha256::new();
        if let Ok(ClipContent::Text(text)) = self.read() {
            hasher.update(text.as_bytes());
        }
        let digest = hasher.finalize();
        Ok(u64::from_be_bytes([
            digest[0], digest[1], digest[2], digest[3], digest[4], digest[5], digest[6],
            digest[7],
        ]))
    }

    fn frontmost_app(&self) -> Option<String> {
        None
    }
}
