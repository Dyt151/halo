//! 剪贴板监控：轮询 `changeCount`，捕获内容变化并去重后经通道发出事件。

pub mod platform;
pub mod types;

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tokio::sync::mpsc;
use tokio::time::{self, Duration};

use self::platform::{platform_port, ClipboardPort};
use self::types::{hash_content, ClipContent, ClipEvent};
use crate::error::AppError;
use crate::lock_ext::LockExt;

/// 监控器共享状态（可在后台任务间安全克隆）。
struct MonitorState {
    port: Arc<dyn ClipboardPort>,
    last_change_count: AtomicU64,
    last_hash: Mutex<Option<String>>,
    running: AtomicBool,
    /// 代数：每次 start/stop 自增，旧轮询任务发现代数变化后自行退出，
    /// 避免「停止后立即重启」时两个任务同时轮询同一剪贴板。
    generation: AtomicU64,
    /// 当前轮询间隔（毫秒），用于判断配置变更是否需要重启。
    interval_ms: AtomicU64,
    /// 本进程自己写剪贴板时登记的指纹（hash + 登记时刻）。
    ///
    /// 回贴/复制都会写剪贴板，`changeCount` 随之 +1，轮询会把这次写入当成
    /// 「用户新复制的内容」再入库一遍：轻则条目被顶到最前、多响一次提示音，
    /// 重则（开着「粘贴为纯文本」）hash 变了，历史里多出一条重复记录。
    self_write: Mutex<Option<(String, std::time::Instant)>>,
}

/// 自写指纹的有效期：只用来吃掉紧接着的那一次回声。
const SELF_WRITE_TTL: Duration = Duration::from_secs(3);

impl MonitorState {
    fn new() -> Self {
        Self {
            port: Arc::from(platform_port()),
            last_change_count: AtomicU64::new(0),
            last_hash: Mutex::new(None),
            running: AtomicBool::new(false),
            generation: AtomicU64::new(0),
            interval_ms: AtomicU64::new(0),
            self_write: Mutex::new(None),
        }
    }

    /// 登记一次「自己写的剪贴板内容」，下一轮轮询读到同样的 hash 就跳过。
    fn mark_self_write(&self, content: &ClipContent) {
        let mut guard = self.self_write.lock_ok();
        *guard = Some((hash_content(content), std::time::Instant::now()));
    }

    /// 判断本次读到的内容是不是自己刚写的那一份（是则消费掉登记）。
    fn consume_self_write(&self, hash: &str) -> bool {
        let mut guard = self.self_write.lock_ok();
        match guard.as_ref() {
            Some((pending, at)) if at.elapsed() <= SELF_WRITE_TTL => {
                if pending == hash {
                    *guard = None;
                    true
                } else {
                    // 这 3 秒内用户又复制了别的内容：保留登记，等真正的那次回声。
                    false
                }
            }
            // 过期或本来就没有登记。
            _ => {
                *guard = None;
                false
            }
        }
    }

    fn poll_once(&self) -> Result<Option<ClipEvent>, AppError> {
        let cc = self
            .port
            .change_count()
            .map_err(|e| AppError::with_details(1001, "读取剪贴板变化计数失败", e.to_string()))?;
        let prev = self.last_change_count.load(Ordering::SeqCst);
        if cc == prev {
            return Ok(None);
        }
        // 先读取再提交计数，避免读失败导致漏抓。
        // 读失败（类型不支持/临时失败）也提交计数：否则同一 changeCount 会
        // 每轮都重读重失败，既空转又永远不前进。
        // 机密/临时内容在读之前就跳过：不把密码等数据读进内存，也不入库。
        if self.port.is_concealed() {
            self.last_change_count.store(cc, Ordering::SeqCst);
            return Ok(None);
        }
        let content = match self.port.read() {
            Ok(c) => c,
            Err(_) => {
                self.last_change_count.store(cc, Ordering::SeqCst);
                return Ok(None);
            }
        };
        self.last_change_count.store(cc, Ordering::SeqCst);
        let hash = hash_content(&content);
        // 自己刚写进去的内容不算「用户复制」：直接丢掉这次事件。
        if self.consume_self_write(&hash) {
            return Ok(None);
        }
        let mut last_hash = self.last_hash.lock_ok();
        if last_hash.as_deref() == Some(hash.as_str()) {
            return Ok(None);
        }
        *last_hash = Some(hash);
        let remote = self.port.is_remote_clipboard();
        Ok(Some(ClipEvent {
            content,
            source_app: if remote {
                Some(types::REMOTE_CLIPBOARD_SOURCE.to_string())
            } else {
                self.port.frontmost_app()
            },
            remote,
        }))
    }
}

/// 剪贴板监控器。
#[derive(Clone)]
pub struct ClipboardMonitor {
    state: Arc<MonitorState>,
}

impl ClipboardMonitor {
    pub fn new() -> Self {
        Self {
            state: Arc::new(MonitorState::new()),
        }
    }

    pub fn is_running(&self) -> bool {
        self.state.running.load(Ordering::SeqCst)
    }

    /// 启动后台轮询任务（幂等）。
    pub fn start(&self, tx: mpsc::UnboundedSender<ClipEvent>, poll_interval_ms: u64) {
        let interval_ms = poll_interval_ms.max(100);
        self.state.interval_ms.store(interval_ms, Ordering::SeqCst);
        if self.state.running.swap(true, Ordering::SeqCst) {
            return;
        }
        let generation = self.state.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let state = self.state.clone();
        tauri::async_runtime::spawn(async move {
            let mut interval = time::interval(Duration::from_millis(interval_ms));
            interval.tick().await; // 首轮立即执行
            while state.running.load(Ordering::SeqCst)
                && state.generation.load(Ordering::SeqCst) == generation
            {
                interval.tick().await;
                if let Ok(Some(event)) = state.poll_once() {
                    if tx.send(event).is_err() {
                        state.running.store(false, Ordering::SeqCst);
                        break;
                    }
                }
            }
        });
    }

    /// 按配置应用监控状态（启停 + 轮询间隔）。
    ///
    /// 配置页改「剪贴板监控」或轮询间隔后调用：间隔变化需要换一个轮询任务，
    /// 否则 `tokio::time::interval` 的周期还是旧值。
    pub fn apply(&self, tx: mpsc::UnboundedSender<ClipEvent>, poll_interval_ms: u64, enabled: bool) {
        if !enabled {
            self.stop();
            return;
        }
        let interval_ms = poll_interval_ms.max(100);
        let running = self.state.running.load(Ordering::SeqCst);
        let same_interval = self.state.interval_ms.load(Ordering::SeqCst) == interval_ms;
        if running && same_interval {
            return;
        }
        self.stop();
        self.start(tx, interval_ms);
    }

    /// 停止后台轮询。
    pub fn stop(&self) {
        self.state.running.store(false, Ordering::SeqCst);
        // 换代：让旧任务无论处于哪个 tick 都会在下一次判断时退出。
        self.state.generation.fetch_add(1, Ordering::SeqCst);
    }

    /// 登记「这份内容是本进程刚写进剪贴板的」，让下一轮轮询忽略这次变化。
    ///
    /// 必须在 `port.write()` **之前**调用：写入会立刻让 `changeCount` +1，
    /// 先登记才能保证回声一定被认出来。
    pub fn mark_self_write(&self, content: &ClipContent) {
        self.state.mark_self_write(content);
    }

    /// 单次轮询（供手动触发或测试使用）。
    pub fn poll_once(&self) -> Result<Option<ClipEvent>, AppError> {
        self.state.poll_once()
    }
}

impl Default for ClipboardMonitor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn self_write_fingerprint_suppresses_exactly_one_echo() {
        let state = MonitorState::new();
        let content = ClipContent::Text("hello".to_string());
        let hash = hash_content(&content);

        // 没登记时不能误吞真实复制。
        assert!(!state.consume_self_write(&hash));

        state.mark_self_write(&content);
        // 回声只吃掉一次：紧接着的下一轮跳过。
        assert!(state.consume_self_write(&hash));
        // 再下一轮就该正常入库（用户又复制了同一段文本）。
        assert!(!state.consume_self_write(&hash));
    }

    #[test]
    fn self_write_fingerprint_keeps_waiting_for_a_different_content() {
        let state = MonitorState::new();
        state.mark_self_write(&ClipContent::Text("written by us".to_string()));

        // 用户在这 3 秒内复制了别的内容：不能误吞，登记继续保留。
        let other = hash_content(&ClipContent::Text("user copy".to_string()));
        assert!(!state.consume_self_write(&other));

        // 我们的回声随后到达，仍然应该被认出来。
        let ours = hash_content(&ClipContent::Text("written by us".to_string()));
        assert!(state.consume_self_write(&ours));
    }
}
