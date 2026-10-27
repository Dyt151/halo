//! 剪贴板堆栈测试：push / pop / peek / clear / list、锁定、发送第一条、发送全部。

use std::sync::Mutex;

use crate::core::paste::PasteOutcome;
use crate::core::stack::{ClipboardStack, StackPaster};
use crate::error::{AppError, CommandResult};

/// 假粘贴器：记录被粘贴的 id，可按配置让指定 id 失败，不触碰真实剪贴板。
struct FakePaster {
    pasted: Mutex<Vec<i64>>,
    fail: Vec<i64>,
    /// 写入剪贴板成功但没能注入 ⌘V（无辅助功能权限）的 id。
    copied_only: Vec<i64>,
}

impl FakePaster {
    fn new(fail: Vec<i64>) -> Self {
        Self::with_copied_only(fail, Vec::new())
    }

    fn with_copied_only(fail: Vec<i64>, copied_only: Vec<i64>) -> Self {
        Self {
            pasted: Mutex::new(Vec::new()),
            fail,
            copied_only,
        }
    }

    fn pasted(&self) -> Vec<i64> {
        self.pasted.lock().unwrap().clone()
    }
}

impl StackPaster for FakePaster {
    fn paste(&self, id: i64) -> CommandResult<PasteOutcome> {
        self.pasted.lock().unwrap().push(id);
        if self.fail.contains(&id) {
            return Err(AppError::new(9102, "fake paste failed"));
        }
        if self.copied_only.contains(&id) {
            return Ok(PasteOutcome::CopiedOnly {
                reason: "未授予「辅助功能」权限，只能复制到剪贴板".to_string(),
            });
        }
        Ok(PasteOutcome::Pasted)
    }
}

#[test]
fn stack_push_pop_peek_clear() {
    let s = ClipboardStack::new();
    assert!(s.is_empty());
    assert_eq!(s.peek(), None);
    assert_eq!(s.pop(), None);

    s.push(1).unwrap();
    s.push(2).unwrap();
    s.push(3).unwrap();
    assert_eq!(s.len(), 3);
    assert!(!s.is_empty());
    assert_eq!(s.peek(), Some(3));
    assert_eq!(s.list(), vec![1, 2, 3]);

    assert_eq!(s.pop(), Some(3));
    assert_eq!(s.pop(), Some(2));
    assert_eq!(s.peek(), Some(1));
    assert_eq!(s.len(), 1);

    s.clear();
    assert!(s.is_empty());
    assert_eq!(s.pop(), None);
}

#[test]
fn stack_peek_does_not_remove() {
    let s = ClipboardStack::new();
    s.push(42).unwrap();
    assert_eq!(s.peek(), Some(42));
    assert_eq!(s.peek(), Some(42));
    assert_eq!(s.len(), 1);
}

#[test]
fn stack_lock_rejects_push_and_unlock_restores() {
    let s = ClipboardStack::new();
    s.push(1).unwrap();

    s.lock();
    assert!(s.is_locked());
    let err = s.push(2).unwrap_err();
    assert_eq!(err.message, "堆栈已锁定，先解锁再添加");
    // 被拒后栈内容不变。
    assert_eq!(s.list(), vec![1]);
    // 已锁定的栈 pop / paste / clear 仍可用。
    assert_eq!(s.pop(), Some(1));
    s.push(5).unwrap_err();
    s.clear();
    assert!(s.is_empty());

    s.unlock();
    assert!(!s.is_locked());
    s.push(3).unwrap();
    s.push(4).unwrap();
    assert_eq!(s.list(), vec![3, 4]);
}

#[test]
fn stack_paste_first_pastes_bottom_without_pop() {
    let s = ClipboardStack::new();
    let paster = FakePaster::new(vec![]);
    s.push(10).unwrap();
    s.push(20).unwrap();
    s.push(30).unwrap();

    let entry = s.paste_first(&paster).unwrap().expect("non-empty stack");
    // 粘贴的是最先入栈（栈底）的条目。
    assert_eq!(entry.id, 10);
    assert_eq!(paster.pasted(), vec![10]);
    // 不弹出。
    assert_eq!(s.list(), vec![10, 20, 30]);
    assert_eq!(s.len(), 3);
}

#[test]
fn stack_paste_first_empty_returns_none() {
    let s = ClipboardStack::new();
    let paster = FakePaster::new(vec![]);
    assert!(s.paste_first(&paster).unwrap().is_none());
    assert!(paster.pasted().is_empty());
}

#[test]
fn stack_paste_all_counts_and_continues_after_failure() {
    let s = ClipboardStack::new();
    // 中间一条（20）粘贴失败。
    let paster = FakePaster::new(vec![20]);
    s.push(10).unwrap();
    s.push(20).unwrap();
    s.push(30).unwrap();

    let result = s.paste_all(&paster).unwrap();
    assert_eq!(result.total, 3);
    assert_eq!(result.pasted, 2);
    assert_eq!(result.failed, 1);
    // 自顶向底依次粘贴：30 → 20 → 10。
    assert_eq!(paster.pasted(), vec![30, 20, 10]);
    assert_eq!(result.items.len(), 3);
    assert!(result.items[0].ok);
    assert_eq!(result.items[0].id, 30);
    assert!(!result.items[1].ok);
    assert_eq!(result.items[1].id, 20);
    assert!(result.items[1].error.is_some());
    assert!(result.items[2].ok);
    assert_eq!(result.items[2].id, 10);
    // 不移除栈内条目。
    assert_eq!(s.list(), vec![10, 20, 30]);
}

/// 「只复制、没贴上」不能计成功，否则界面会报「已粘贴 N 条」而实际一条没贴。
#[test]
fn stack_paste_all_does_not_count_copied_only_as_success() {
    let s = ClipboardStack::new();
    let paster = FakePaster::with_copied_only(vec![], vec![20]);
    s.push(10).unwrap();
    s.push(20).unwrap();
    s.push(30).unwrap();

    let result = s.paste_all(&paster).unwrap();
    assert_eq!(result.total, 3);
    assert_eq!(result.pasted, 2);
    assert_eq!(result.failed, 1);
    assert_eq!(result.items[1].id, 20);
    assert!(!result.items[1].ok);
    assert!(
        result.items[1].error.as_deref().unwrap_or_default().contains("辅助功能"),
        "{:?}",
        result.items[1]
    );
}

#[test]
fn stack_paste_all_empty_stack() {
    let s = ClipboardStack::new();
    let paster = FakePaster::new(vec![]);
    let result = s.paste_all(&paster).unwrap();
    assert_eq!(result.total, 0);
    assert_eq!(result.pasted, 0);
    assert_eq!(result.failed, 0);
    assert!(result.items.is_empty());
    assert!(paster.pasted().is_empty());
}

#[test]
fn stack_locked_paste_first_and_paste_all_still_work() {
    let s = ClipboardStack::new();
    let paster = FakePaster::new(vec![]);
    s.push(7).unwrap();
    s.push(8).unwrap();
    s.lock();

    // 锁定不影响粘贴：先发送第一条（7），再发送全部（自顶向底 8 → 7）。
    let entry = s.paste_first(&paster).unwrap().expect("non-empty stack");
    assert_eq!(entry.id, 7);
    let result = s.paste_all(&paster).unwrap();
    assert_eq!(result.total, 2);
    assert_eq!(result.pasted, 2);
    assert_eq!(paster.pasted(), vec![7, 8, 7]);
}
