//! 剪贴板堆栈：内存栈（LIFO）结构，保存历史条目 id。
//!
//! 用途：临时收集若干条历史（快捷键/右键菜单压入），再统一粘贴：
//! 栈顶（`paste_top`）、最先入栈的第一条（`paste_first`）、全部（`paste_all`）。
//! 支持锁定：锁定后拒绝压入新条目；弹出/粘贴/清空不受影响。
//! 当前为进程内存栈（不落库），重启后清空。

use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;

use crate::core::paste::{PasteOutcome, PasteService};
use crate::error::{AppError, CommandResult};
use crate::lock_ext::LockExt;

/// 堆栈已锁定时压入被拒的错误码（9xxx 通用段）。
const STACK_LOCKED_CODE: i32 = 9101;

/// 「发送全部」时每条粘贴之间的间隔：给前台 App 留出接收上一次粘贴的时间。
const PASTE_ALL_INTERVAL: Duration = Duration::from_millis(300);

/// 堆栈粘贴执行器：由 `PasteService` 实现；测试可注入假实现，避免真实剪贴板副作用。
pub trait StackPaster {
    fn paste(&self, id: i64) -> CommandResult<PasteOutcome>;
}

impl StackPaster for PasteService {
    fn paste(&self, id: i64) -> CommandResult<PasteOutcome> {
        PasteService::paste(self, id)
    }
}

/// 堆栈单条粘贴结果（含条目 id，便于前端展示与测试断言）。
#[derive(Debug, Clone, Serialize)]
pub struct StackPasteEntry {
    pub id: i64,
    pub outcome: PasteOutcome,
}

/// 「发送全部」中单条的执行结果。
#[derive(Debug, Clone, Serialize)]
pub struct StackPasteItemResult {
    pub id: i64,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// 「发送全部」汇总：总数 / 成功 / 失败计数与逐条结果。
#[derive(Debug, Clone, Serialize)]
pub struct StackPasteAllResult {
    pub total: usize,
    pub pasted: usize,
    pub failed: usize,
    pub items: Vec<StackPasteItemResult>,
}

/// 堆栈内部状态（条目 + 锁定标记），整体置于一把锁内保证一致性。
#[derive(Default)]
struct StackState {
    items: Vec<i64>,
    locked: bool,
}

/// 剪贴板堆栈（可跨线程共享克隆）。
#[derive(Clone, Default)]
pub struct ClipboardStack {
    inner: Arc<Mutex<StackState>>,
}

impl ClipboardStack {
    pub fn new() -> Self {
        Self::default()
    }

    /// 压入一条历史（clip_id）。锁定时拒绝并返回明确错误，栈内容不变。
    pub fn push(&self, clip_id: i64) -> CommandResult<()> {
        let mut state = self.inner.lock_ok();
        if state.locked {
            // 这句会原样弹成 toast，所以不能写英文内部文案。
            return Err(AppError::new(STACK_LOCKED_CODE, "堆栈已锁定，先解锁再添加"));
        }
        state.items.push(clip_id);
        Ok(())
    }

    /// 锁定堆栈：拒绝后续压入（弹出/粘贴/清空不受影响）。
    pub fn lock(&self) {
        self.inner.lock_ok().locked = true;
    }

    /// 解锁堆栈：恢复压入。
    pub fn unlock(&self) {
        self.inner.lock_ok().locked = false;
    }

    /// 当前是否处于锁定状态。
    pub fn is_locked(&self) -> bool {
        self.inner.lock_ok().locked
    }

    /// 弹出栈顶并返回（锁定不影响）。
    pub fn pop(&self) -> Option<i64> {
        self.inner.lock_ok().items.pop()
    }

    /// 查看栈顶（不移除）。
    pub fn peek(&self) -> Option<i64> {
        self.inner.lock_ok().items.last().copied()
    }

    /// 查看栈底（最先入栈的条目，不移除）。
    pub fn peek_bottom(&self) -> Option<i64> {
        self.inner.lock_ok().items.first().copied()
    }

    /// 清空堆栈（锁定不影响）。
    pub fn clear(&self) {
        self.inner.lock_ok().items.clear();
    }

    /// 当前栈大小。
    pub fn len(&self) -> usize {
        self.inner.lock_ok().items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 返回栈内全部 id（自底向顶）。
    pub fn list(&self) -> Vec<i64> {
        self.inner.lock_ok().items.clone()
    }

    /// 粘贴栈顶（复用 `PasteService`）；空栈返回 `None`。
    pub fn paste_top<P: StackPaster>(&self, paste: &P) -> CommandResult<Option<PasteOutcome>> {
        match self.peek() {
            Some(id) => Ok(Some(paste.paste(id)?)),
            None => Ok(None),
        }
    }

    /// 粘贴栈底（最先入栈）条目，不弹出；空栈返回 `None`。
    pub fn paste_first<P: StackPaster>(&self, paste: &P) -> CommandResult<Option<StackPasteEntry>> {
        match self.peek_bottom() {
            Some(id) => {
                let outcome = paste.paste(id)?;
                Ok(Some(StackPasteEntry { id, outcome }))
            }
            None => Ok(None),
        }
    }

    /// 从栈顶到栈底依次粘贴全部条目（每条之间间隔约 300ms）；
    /// 单条失败不中断其余，返回成功/失败计数与逐条结果。不移除栈内条目。
    pub fn paste_all<P: StackPaster>(&self, paste: &P) -> CommandResult<StackPasteAllResult> {
        // 先快照 id（自顶向底），避免粘贴期间持锁。
        let ids: Vec<i64> = {
            let state = self.inner.lock_ok();
            state.items.iter().rev().copied().collect()
        };

        let total = ids.len();
        let mut items = Vec::with_capacity(total);
        let mut pasted = 0usize;
        let mut failed = 0usize;

        for (idx, id) in ids.iter().enumerate() {
            match paste.paste(*id) {
                // 只有真的贴上去才算成功。
                Ok(PasteOutcome::Pasted) => {
                    pasted += 1;
                    items.push(StackPasteItemResult {
                        id: *id,
                        ok: true,
                        error: None,
                    });
                }
                // 写入剪贴板成功但没能注入 ⌘V（无辅助功能权限等）：不能计成功，
                // 否则界面会报「已粘贴 5 条」而实际一条都没贴。
                Ok(PasteOutcome::CopiedOnly { reason }) => {
                    failed += 1;
                    items.push(StackPasteItemResult {
                        id: *id,
                        ok: false,
                        error: Some(reason),
                    });
                }
                Err(e) => {
                    failed += 1;
                    items.push(StackPasteItemResult {
                        id: *id,
                        ok: false,
                        error: Some(e.to_string()),
                    });
                }
            }
            // 每条之间留出间隔（最后一条之后无需等待）。
            if idx + 1 < total {
                std::thread::sleep(PASTE_ALL_INTERVAL);
            }
        }

        Ok(StackPasteAllResult {
            total,
            pasted,
            failed,
            items,
        })
    }
}
