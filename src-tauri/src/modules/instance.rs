//! 运行中的模块实例：状态机 + 运行时句柄 + 模块上下文。

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::bus::MessageBus;
use super::capabilities::{Capability, CapabilityGate};
use super::manifest::ModuleId;
use super::sidecar::SidecarHandle;
use crate::core::clipboard::platform::platform_port;
use crate::core::clipboard::types::ClipItem;
use crate::error::{AppError, CommandResult};
use crate::lock_ext::LockExt;

/// 模块运行状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModuleState {
    Stopped,
    Starting,
    Running,
    Crashed,
}

/// 模块实例运行时句柄。
pub enum InstanceHandle {
    Sidecar(Arc<SidecarHandle>),
    /// L0 iframe 模块：无后端进程，UI 由前端沙箱渲染。
    Iframe,
    /// L1 WASM 模块（P1 预留）。
    Wasm,
}

/// 模块与核心之间的消息。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ModuleMessage {
    /// 剪贴板动作（核心 → 模块）。
    ClipboardAction { clip: ClipItem },
    /// 通用请求（模块 → 核心，经门控）。
    Request { method: String, params: Value },
    /// 通用响应。
    Response { result: Value },
    /// 广播通知。
    Notification { topic: String, data: Value },
}

/// 模块 → 核心的受限发送通道。
#[derive(Clone)]
pub struct ModuleSender {
    module_id: String,
    bus: Arc<MessageBus>,
    gate: Arc<CapabilityGate>,
}

impl ModuleSender {
    pub fn new(module_id: String, bus: Arc<MessageBus>, gate: Arc<CapabilityGate>) -> Self {
        Self {
            module_id,
            bus,
            gate,
        }
    }

    /// 发布模块事件到总线（会转发给前端）。
    pub fn publish(&self, topic: &str, payload: Value) {
        self.bus
            .publish(&format!("module:{topic}"), payload);
    }

    /// 模块调用核心能力（经 `CapabilityGate` 校验）。
    pub fn request(&self, method: &str, params: Value) -> CommandResult<Value> {
        let cap = Capability::required_for_method(method).ok_or_else(|| {
            AppError::with_details(4003, "不支持的方法", format!("'{method}'"))
        })?;
        if !self.gate.check(&self.module_id, cap) {
            return Err(AppError::with_details(
                4003,
                "模块没有获得该权限",
                format!("模块 '{}' 缺少 '{}'", self.module_id, cap.id()),
            ));
        }
        match cap {
            Capability::ClipboardRead => {
                let port = platform_port();
                let content = port
                    .read()
                    .map_err(|e| AppError::with_details(4003, "读取剪贴板失败", e.to_string()))?;
                Ok(serde_json::to_value(content)?)
            }
            Capability::ClipboardWrite => {
                let content: crate::core::clipboard::types::ClipContent =
                    serde_json::from_value(params).map_err(|e| {
                        AppError::with_details(4003, "剪贴板内容不合法", e.to_string())
                    })?;
                let port = platform_port();
                port.write(&content)
                    .map_err(|e| AppError::with_details(4003, "写入剪贴板失败", e.to_string()))?;
                Ok(Value::Null)
            }
            _ => Err(AppError::with_details(
                4003,
                "该能力尚未实现",
                cap.id(),
            )),
        }
    }
}

/// 模块启动时注入的上下文。
pub struct ModuleContext {
    pub module_dir: PathBuf,
    pub bus: Arc<MessageBus>,
    pub sender: ModuleSender,
    pub grants: Arc<[Capability]>,
}

/// 一个运行中的模块实例（宿主内部持有）。
pub struct ModuleInstance {
    id: ModuleId,
    state: Arc<Mutex<ModuleState>>,
    handle: Mutex<Option<InstanceHandle>>,
    grants: Vec<Capability>,
    crash_notified: AtomicBool,
}

impl ModuleInstance {
    pub fn new(id: ModuleId, grants: Vec<Capability>) -> Self {
        Self {
            id,
            state: Arc::new(Mutex::new(ModuleState::Stopped)),
            handle: Mutex::new(None),
            grants,
            crash_notified: AtomicBool::new(false),
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn state(&self) -> ModuleState {
        *self.state.lock_ok()
    }

    pub fn grants(&self) -> &[Capability] {
        &self.grants
    }

    fn set_state(&self, state: ModuleState) {
        *self.state.lock_ok() = state;
    }

    /// 启动实例。
    pub fn start(&self, ctx: ModuleContext) -> CommandResult<()> {
        self.set_state(ModuleState::Starting);
        // sidecar 与 iframe 的运行时级别已由清单决定，这里按当前 handle 选择。
        // 由调用方（host）在 start 前设置 handle；此处仅切换状态。
        self.set_state(ModuleState::Running);
        self.crash_notified.store(false, Ordering::SeqCst);
        let _ = ctx; // ctx 由 host 在 spawn sidecar 时消费
        Ok(())
    }

    /// 停止实例。
    pub fn stop(&self) -> CommandResult<()> {
        let mut handle = self.handle.lock_ok();
        if let Some(InstanceHandle::Sidecar(sidecar)) = handle.take() {
            sidecar.kill();
        }
        self.set_state(ModuleState::Stopped);
        Ok(())
    }

    /// 设置运行时句柄（host 在 spawn 后调用）。
    pub fn set_handle(&self, handle: InstanceHandle) {
        *self.handle.lock_ok() = Some(handle);
    }

    /// 同步借用 sidecar 句柄（用于 `is_alive`/`kill` 等同步操作）。
    pub fn with_sidecar<F, R>(&self, f: F) -> Option<R>
    where
        F: FnOnce(&SidecarHandle) -> R,
    {
        let guard = self.handle.lock_ok();
        match guard.as_ref() {
            Some(InstanceHandle::Sidecar(sidecar)) => Some(f(sidecar.as_ref())),
            _ => None,
        }
    }

    /// 克隆 sidecar 句柄（用于异步 request，避免锁跨 await）。
    pub fn sidecar_handle(&self) -> Option<Arc<SidecarHandle>> {
        let guard = self.handle.lock_ok();
        match guard.as_ref() {
            Some(InstanceHandle::Sidecar(sidecar)) => Some(sidecar.clone()),
            _ => None,
        }
    }

    /// 标记为崩溃（心跳检测调用）。
    pub fn mark_crashed(&self) {
        self.set_state(ModuleState::Crashed);
        self.crash_notified.store(true, Ordering::SeqCst);
    }
}
