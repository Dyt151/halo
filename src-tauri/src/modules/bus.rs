//! 消息总线：把主题消息经 emitter 发布给前端。

use std::sync::Mutex;

use crate::lock_ext::LockExt;
use serde_json::Value;

/// 前端事件发射器：接收主题与载荷（通常封装 `AppHandle::emit`）。
type Emitter = Box<dyn Fn(String, Value) + Send + Sync + 'static>;

/// 主题消息总线。
pub struct MessageBus {
    emitter: Mutex<Option<Emitter>>,
}

impl MessageBus {
    pub fn new() -> Self {
        Self {
            emitter: Mutex::new(None),
        }
    }

    /// 设置前端事件发射器（通常封装 `AppHandle::emit`）。
    pub fn set_emitter(&self, emitter: impl Fn(String, Value) + Send + Sync + 'static) {
        *self.emitter.lock_ok() = Some(Box::new(emitter));
    }

    /// 发布消息到某主题，通知前端。
    pub fn publish(&self, topic: &str, payload: Value) {
        if let Some(emitter) = self.emitter.lock_ok().as_ref() {
            emitter(topic.to_string(), payload);
        }
    }
}

impl Default for MessageBus {
    fn default() -> Self {
        Self::new()
    }
}