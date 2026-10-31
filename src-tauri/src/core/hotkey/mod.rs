//! 全局快捷键管理：基于 `tauri-plugin-global-shortcut`。
//!
//! 只注册真正需要「在其它 App 里也生效」的组合：隐藏面板 / 粘贴堆栈顶。
//! 注册时单个冲突不影响其它快捷键（容错并记录错误日志）。
//!
//! 注：「打开堆栈」快捷键已移除（没有对应的堆栈面板，按下却会直接粘贴栈顶，
//! 属文案与行为背离的误粘贴风险）。堆栈只保留「粘贴栈顶」。
//! 环形转换也**不**提供快捷键：运行态唯一入口是 Finder 里 Option＋左键拖动
//! （`docs/REA-Filee-Ring-Integrated.md` §3.1）。快捷回复已下线（自用不需要）。
//!
//! ⌘1~9 **不是**全局快捷键：全局注册会直接抢走所有 App 的 ⌘1~9（浏览器切标签、
//! 邮件切账号全失效）。它只在面板唤出时生效，由面板自己处理键盘事件。

use std::str::FromStr;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

use crate::config::settings::Config;
use crate::error::{AppError, CommandResult};
use crate::lock_ext::LockExt;

/// 单个快捷键槽位的注册结果（评审 C6 / 清单 16）。
///
/// 旧实现只在日志里 `warn!`：界面显示「已保存」，实际注册失败用户永远不知道。
/// 现在逐槽返回结果，前端可以把失败原因直接标在对应输入框下方。
#[derive(Debug, Clone, Serialize)]
pub struct HotkeySlotResult {
    /// 槽位：`toggle` / `stack_paste`。
    /// （⌘1~9 不在全局注册范围内，因此没有对应槽位。）
    pub slot: String,
    pub accelerator: String,
    pub ok: bool,
    pub message: Option<String>,
}

/// 快捷键触发动作（供全局 handler 分发）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HotkeyAction {
    /// 唤起/收起剪贴板面板。
    Toggle,
    /// 粘贴堆栈顶。
    StackPaste,
}

/// 将按下的快捷键解析为对应动作；无法匹配时返回 `None`。
pub fn resolve_action(config: &Config, shortcut: &Shortcut) -> Option<HotkeyAction> {
    let hotkeys = &config.hotkeys;
    if parse_eq(&hotkeys.toggle, shortcut) {
        return Some(HotkeyAction::Toggle);
    }
    if config.stack_enabled && parse_eq(&hotkeys.stack_paste, shortcut) {
        return Some(HotkeyAction::StackPaste);
    }
    None
}

/// 解析字符串并判断是否等于按下的快捷键。
fn parse_eq(accelerator: &str, shortcut: &Shortcut) -> bool {
    Shortcut::from_str(accelerator)
        .map(|parsed| parsed == *shortcut)
        .unwrap_or(false)
}

/// 全局快捷键管理器（多快捷键）。
#[derive(Clone)]
pub struct HotkeyManager {
    app: AppHandle,
    registered: Arc<Mutex<Vec<String>>>,
}

impl HotkeyManager {
    pub fn new(app: AppHandle) -> Self {
        Self {
            app,
            registered: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// 根据配置注册全部快捷键（先注销旧注册，逐个注册，单个失败不影响其它）。
    pub fn register_all(&self, config: &Config) -> CommandResult<()> {
        let results = self.apply_with_report(config);
        // 保留旧签名给启动流程用：整体有失败时返回一条聚合错误（逐槽信息走 apply_hotkeys）。
        let failed: Vec<String> = results
            .iter()
            .filter(|r| !r.ok)
            .map(|r| format!("{}（{}）", r.accelerator, r.message.clone().unwrap_or_default()))
            .collect();
        if failed.is_empty() {
            Ok(())
        } else {
            Err(AppError::with_details(
                1010,
                "部分快捷键注册失败",
                failed.join("；"),
            ))
        }
    }

    /// 注册全部快捷键并返回逐槽结果（清单 16）。
    ///
    /// 与 [`Self::register_all`] 的差别只有返回值：注册流程完全一致，
    /// 调用方（`apply_hotkeys` 命令）需要知道是哪个槽位、为什么失败。
    pub fn apply_with_report(&self, config: &Config) -> Vec<HotkeySlotResult> {
        if let Err(e) = self.unregister_all() {
            tracing::warn!("unregister all hotkeys failed: {e}");
        }

        let mut results: Vec<HotkeySlotResult> = Vec::new();
        let mut ok: Vec<String> = Vec::new();

        for (slot, accelerator) in [
            ("toggle", config.hotkeys.toggle.as_str()),
            ("stack_paste", config.hotkeys.stack_paste.as_str()),
        ] {
            let accelerator = accelerator.trim();
            if accelerator.is_empty() {
                continue;
            }
            match self.app.global_shortcut().register(accelerator) {
                Ok(()) => {
                    ok.push(accelerator.to_string());
                    results.push(HotkeySlotResult {
                        slot: slot.into(),
                        accelerator: accelerator.into(),
                        ok: true,
                        message: None,
                    });
                }
                Err(e) => results.push(HotkeySlotResult {
                    slot: slot.into(),
                    accelerator: accelerator.into(),
                    ok: false,
                    message: Some(format!("系统未接受该组合（可能已被其它 App 占用）：{e}")),
                }),
            }
        }

        // 数字快捷键不再全局注册：⌘1~9 是浏览器/编辑器/邮件里最常用的组合，
        // 系统级接管会让用户关掉所有 App 的这套键。它现在只在面板唤出时由
        // 面板自己处理（`num_hotkey_enabled` 仍作为开关）。

        *self.registered.lock_ok() = ok;
        results
    }

    /// 注销全部快捷键。
    pub fn unregister_all(&self) -> CommandResult<()> {
        let mut registered = self.registered.lock_ok();
        for accelerator in registered.iter() {
            if let Err(e) = self.app.global_shortcut().unregister(accelerator.as_str()) {
                tracing::warn!("unregister hotkey `{accelerator}` failed: {e}");
            }
        }
        registered.clear();
        Ok(())
    }

    /// 当前已成功注册的快捷键列表。
    pub fn registered(&self) -> Vec<String> {
        self.registered.lock_ok().clone()
    }

    /// 兼容旧接口：返回首个已注册快捷键（通常为 toggle）。
    pub fn current(&self) -> Option<String> {
        self.registered().into_iter().next()
    }
}
