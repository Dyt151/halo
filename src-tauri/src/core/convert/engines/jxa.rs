//! JXA（osascript -l JavaScript）桥：跑系统框架能力（PDFKit / WebKit）的共享入口。
//!
//! 引擎把脚本作为字符串内嵌，首次运行时写入进程级临时目录，之后每次调用
//! 直接 `osascript -l JavaScript <script> <args…>`。脚本约定：
//! - 成功：`run()` 返回 `"ok:…"`（可带负载）；
//! - 失败：返回 `"ERR:…"`，Rust 侧拆成 `ConvertError`。

use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

use super::super::edge::ConvertError;

/// 脚本驻留目录：进程级一次性创建，操作系统清理。
fn script_dir() -> PathBuf {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("halo-jxa-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        dir
    })
    .clone()
}

/// 运行一段内嵌 JXA 脚本，返回脚本的输出（去空白）。
///
/// 脚本只在第一次运行时落盘；并发调用同一脚本时 `Command` 各自拉起进程，
/// 互不干扰。
pub fn run(script_name: &str, script: &str, args: &[&str]) -> Result<String, ConvertError> {
    let path = script_dir().join(script_name);
    if !path.exists() {
        std::fs::write(&path, script)
            .map_err(|e| ConvertError::io(format!("无法写入桥接脚本：{e}")))?;
    }

    let output = Command::new("osascript")
        .arg("-l")
        .arg("JavaScript")
        .arg(&path)
        .args(args)
        .output()
        .map_err(|e| ConvertError::io(format!("无法启动 osascript：{e}")))?;

    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr);
        let detail = detail.trim();
        return Err(ConvertError::encode(format!(
            "系统脚本执行失败{}",
            if detail.is_empty() {
                String::new()
            } else {
                format!("：{detail}")
            }
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stdout = stdout.trim().to_string();
    if let Some(rest) = stdout.strip_prefix("ERR:") {
        return Err(ConvertError::encode(rest.trim().to_string()));
    }
    Ok(stdout)
}
