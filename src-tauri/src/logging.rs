//! 运行日志落地：`~/.clipbase/logs/clipbase.log`。
//!
//! 为什么必须写文件：用 `open ClipBase.app` 启动时 stdout / stderr 全被系统丢弃，
//! 而环形盘这类「失败时没有任何 UI 反馈」的路径（缺辅助功能权限、CGEventTap
//! 建不起来、AX 读不到起手文件）只能靠日志定位。开发时仍同时打到 stderr。
//!
//! 只保留一代轮转（`.log` + `.log.1`）：够定位最近几次问题，又不会无限长。

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::sync::{Arc, Mutex};

use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

use crate::storage;

/// 单个日志文件的上限：超过就在启动时轮转成 `.1`。
const MAX_BYTES: u64 = 4 * 1024 * 1024;

/// 日志文件路径。
pub fn log_path() -> std::path::PathBuf {
    storage::logs_dir().join("clipbase.log")
}

/// 可克隆的写句柄：tracing 每次格式化都会要一个新的 writer。
#[derive(Clone)]
struct FileSink(Arc<Mutex<File>>);

impl Write for FileSink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self.0.lock() {
            Ok(mut file) => file.write(buf),
            // 日志写不进去不能反过来把主流程搞挂：当作丢弃。
            Err(_) => Ok(buf.len()),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self.0.lock() {
            Ok(mut file) => file.flush(),
            Err(_) => Ok(()),
        }
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for FileSink {
    type Writer = FileSink;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

/// 打开日志文件：目录不存在就建，超过上限就先轮转。
fn open_log() -> Option<File> {
    let dir = storage::logs_dir();
    if let Err(e) = fs::create_dir_all(&dir) {
        eprintln!("创建日志目录失败：{e}");
        return None;
    }
    let path = log_path();
    if let Ok(meta) = fs::metadata(&path) {
        if meta.len() > MAX_BYTES {
            let _ = fs::rename(&path, dir.join("clipbase.log.1"));
        }
    }
    match OpenOptions::new().create(true).append(true).open(&path) {
        Ok(file) => Some(file),
        Err(e) => {
            eprintln!("打开日志文件失败：{e}");
            None
        }
    }
}

/// 初始化全局订阅器。只在 `run()` 里调用一次。
///
/// 默认级别 `clipbase=debug,info`：本项目自己的 debug 日志（手势判定、起手文件、
/// 选盘结果）默认就落盘，第三方库仍只记 info，避免日志被淹没。
pub fn init() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("clipbase=debug,info"));
    // 两个分支各自创建 stderr layer：`fmt::Layer` 的订阅者类型参数是在
    // `.with()` 时推断的，共用一个变量会让两个分支的叠加类型互相矛盾。
    match open_log() {
        Some(file) => {
            let file_layer = tracing_subscriber::fmt::layer()
                // 文件里不留 ANSI 颜色码，否则 grep 满屏转义。
                .with_ansi(false)
                .with_writer(FileSink(Arc::new(Mutex::new(file))));
            let stderr_layer = tracing_subscriber::fmt::layer().with_writer(std::io::stderr);
            tracing_subscriber::registry()
                .with(filter)
                .with(file_layer)
                .with(stderr_layer)
                .init();
            tracing::info!(path = %log_path().display(), "日志已就绪");
        }
        None => {
            tracing_subscriber::registry()
                .with(filter)
                .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
                .init();
        }
    }
}
