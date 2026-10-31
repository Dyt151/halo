//! 应用全局状态：聚合各服务实例，作为 Tauri State 注入。

use std::sync::Arc;

use serde_json::Value;
use tauri::{AppHandle, Emitter};

use crate::config::ConfigService;
use crate::core::clipboard::types::{hash_content, ClipContent, ClipEvent, ClipItem, ContentType, SyncState};
use crate::core::history::HistoryService;
use crate::core::hotkey::HotkeyManager;
use crate::core::paste::{PasteOutcome, PasteService};
use crate::core::sensitivity::SensitivityDetector;
use crate::core::stack::{StackPasteAllResult, StackPasteEntry};
use crate::core::ClipboardMonitor;
use crate::core::ClipboardStack;
use crate::error::CommandResult;
use crate::modules::{CapabilityGate, MessageBus, ModuleHost};
use crate::sync::SyncManager;

/// 应用全局状态。
#[derive(Clone)]
pub struct AppState {
    pub app: AppHandle,
    pub config: ConfigService,
    pub history: HistoryService,
    pub monitor: ClipboardMonitor,
    pub paste: PasteService,
    pub hotkey: HotkeyManager,
    pub stack: ClipboardStack,
    pub modules: ModuleHost,
    pub sync: SyncManager,
    pub sensitivity: SensitivityDetector,
    pub gate: Arc<CapabilityGate>,
    pub bus: Arc<MessageBus>,
    /// 监控事件通道的发送端：配置变更后重启监控需要重新交给轮询任务。
    pub monitor_tx: tokio::sync::mpsc::UnboundedSender<ClipEvent>,
}

impl AppState {
    /// 处理一条剪贴板事件：敏感识别 → 入库 → 发布事件 → 广播给模块。
    /// 返回入库后的条目（被忽略时返回 None）。
    pub fn process_clip_event(&self, event: ClipEvent) -> Option<ClipItem> {
        self.ingest_clip_event(event, false)
    }

    /// 用户主动触发的「捕获当前剪贴板」。
    ///
    /// 绕过 `monitor_enabled`：关掉监控只是不要后台自动记录，不等于用户不想
    /// 手动抓这一次。不绕的话，监控关着时点那个按钮会毫无反应。
    pub fn capture_clip_event(&self, event: ClipEvent) -> Option<ClipItem> {
        self.ingest_clip_event(event, true)
    }

    fn ingest_clip_event(&self, event: ClipEvent, force: bool) -> Option<ClipItem> {
        let cfg = self.config.get();
        if !force && !cfg.monitor_enabled {
            return None;
        }

        // 按 App 黑名单过滤：来源 App 命中则跳过入库。
        if let Some(source_app) = &event.source_app {
            if is_blacklisted(source_app, &cfg.app_blacklist) {
                tracing::debug!("clipboard from `{source_app}` is blacklisted, skipping");
                return None;
            }
        }

        // 敏感内容统一走 `sensitivity::apply`：捕获、导入等所有入库路径共用
        // 同一套策略，避免某条旁路把密码按明文存下来。
        let (sensitive, content) =
            crate::core::sensitivity::apply(&self.sensitivity, cfg.sensitivity, event.content)?;

        let hash = hash_content(&content);
        // 来源 App 图标导出（尽力而为，失败静默降级为 None）。
        let source_app_icon = crate::core::appicon::export_frontmost_app_icon()
            .map(|p| p.to_string_lossy().into_owned());
        let item = ClipItem {
            id: 0,
            content_type: content_type_of(&content),
            content: content.clone(),
            preview: content.preview(200),
            source_app: event.source_app,
            source_app_icon,
            tag: None,
            created_at: 0,
            last_used_at: 0,
            pinned: false,
            favorite: false,
            sensitive,
            size_bytes: content.size_bytes(),
            hash,
            sync_state: SyncState::Local,
        };

        match self.history.insert(item.clone()) {
            Ok(id) => {
                let mut saved = item;
                saved.id = id;
                saved.preview = saved.content.preview(200);
                let payload = serde_json::to_value(&saved).unwrap_or(Value::Null);

                // 复制入库成功时按配置播放提示音。
                // 注意：本方法跑在 Tokio 工作线程，AppKit/NSSound 必须回主线程
                //（在非主线程调 NSSound beep 会崩掉整个事件循环）。
                if cfg.sound_enabled {
                    let app = self.app.clone();
                    let _ = app.run_on_main_thread(move || {
                        crate::core::sound::play_system_sound();
                    });
                }

                // 通知前端（clipboard://changed）与进程内总线（history://created）。
                let _ = self.app.emit(crate::events::CLIPBOARD_CHANGED, &saved);
                self.bus.publish(crate::events::HISTORY_CREATED, payload.clone());
                // 广播给运行中的模块。
                self.modules.broadcast(crate::events::CLIPBOARD_CHANGED, payload);
                Some(saved)
            }
            Err(e) => {
                tracing::warn!("history insert failed: {e}");
                None
            }
        }
    }

    /// 回贴并执行「粘贴后动作」：成功时按配置播放提示音、自动隐藏主窗口。
    pub fn paste_and_finish(&self, id: i64) -> CommandResult<PasteOutcome> {
        let outcome = self.paste.paste(id)?;
        self.after_paste(&outcome);
        Ok(outcome)
    }

    /// 粘贴堆栈顶并执行「粘贴后动作」；空栈返回 `None`。
    pub fn paste_stack_top(&self) -> CommandResult<Option<PasteOutcome>> {
        let outcome = self.stack.paste_top(&self.paste)?;
        if let Some(o) = &outcome {
            self.after_paste(o);
        }
        Ok(outcome)
    }

    /// 粘贴栈底（最先入栈）条目并执行「粘贴后动作」；空栈返回 `None`。
    pub fn paste_stack_first(&self) -> CommandResult<Option<StackPasteEntry>> {
        let entry = self.stack.paste_first(&self.paste)?;
        if let Some(e) = &entry {
            self.after_paste(&e.outcome);
        }
        Ok(entry)
    }

    /// 从栈顶到栈底依次粘贴全部条目；全部结束后统一执行一次「粘贴后动作」，
    /// 避免每条粘贴后都隐藏窗口导致后续条目无法送达前台 App。
    pub fn paste_stack_all(&self) -> CommandResult<StackPasteAllResult> {
        let result = self.stack.paste_all(&self.paste)?;
        if result.pasted > 0 {
            self.after_paste_ok();
        }
        Ok(result)
    }

    /// 粘贴成功后的统一收尾：`sound_enabled` 播放提示音；面板是否收起由前端
    /// 播完 140ms 收起动画后自行隐藏（这里只广播 `PASTE_FINISHED`，后端直接
    /// hide 会打断动画与成功提示）。
    fn after_paste(&self, outcome: &PasteOutcome) {
        if !matches!(outcome, PasteOutcome::Pasted) {
            return;
        }
        self.after_paste_ok();
    }

    /// 已知至少一条粘贴成功时的收尾（发送全部等批量场景复用）。
    fn after_paste_ok(&self) {
        let cfg = self.config.get();
        if cfg.sound_enabled {
            // 与复制入库同理由：统一回主线程播放（粘贴命令本身虽在主线程，
            // 但批量粘贴等路径可能来自其它线程）。
            let app = self.app.clone();
            let _ = app.run_on_main_thread(move || {
                crate::core::sound::play_system_sound();
            });
        }
        // 只通知前端：面板窗口在粘贴成功时由前端播完 140ms 收起动画再 hide
        // （后端立即 hide 会把动画与 Toast 一起掐掉）。主窗口不受影响。
        let _ = self.app.emit(crate::events::PASTE_FINISHED, ());
    }
}

/// 判断来源 App 是否命中黑名单（大小写不敏感、子串匹配）。
pub(crate) fn is_blacklisted(source_app: &str, blacklist: &[String]) -> bool {
    let lower = source_app.to_lowercase();
    blacklist
        .iter()
        .any(|b| !b.is_empty() && lower.contains(&b.to_lowercase()))
}

fn content_type_of(content: &ClipContent) -> ContentType {
    match content {
        ClipContent::Text(_) => ContentType::Text,
        ClipContent::Html(_) => ContentType::Html,
        ClipContent::Image { .. } => ContentType::Image,
        ClipContent::Files(_) => ContentType::Files,
    }
}
