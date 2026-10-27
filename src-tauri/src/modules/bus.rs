//! 消息总线：主题订阅/发布。既通知前端（经 emitter），也支持进程内订阅者。

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use crate::lock_ext::LockExt;
use serde_json::Value;

type Handler = Box<dyn Fn(Value) + Send + Sync + 'static>;

/// 前端事件发射器：接收主题与载荷（通常封装 `AppHandle::emit`）。
type Emitter = Box<dyn Fn(String, Value) + Send + Sync + 'static>;

/// 主题消息总线。
pub struct MessageBus {
    emitter: Mutex<Option<Emitter>>,
    subscribers: Mutex<HashMap<String, HashMap<usize, Handler>>>,
    next_id: AtomicUsize,
}

impl MessageBus {
    pub fn new() -> Self {
        Self {
            emitter: Mutex::new(None),
            subscribers: Mutex::new(HashMap::new()),
            next_id: AtomicUsize::new(1),
        }
    }

    /// 设置前端事件发射器（通常封装 `AppHandle::emit`）。
    pub fn set_emitter(&self, emitter: impl Fn(String, Value) + Send + Sync + 'static) {
        *self.emitter.lock_ok() = Some(Box::new(emitter));
    }

    /// 发布消息到某主题：通知前端 + 进程内订阅者。
    pub fn publish(&self, topic: &str, payload: Value) {
        if let Some(emitter) = self.emitter.lock_ok().as_ref() {
            emitter(topic.to_string(), payload.clone());
        }
        if let Some(handlers) = self.subscribers.lock_ok().get(topic) {
            for handler in handlers.values() {
                handler(payload.clone());
            }
        }
    }

    /// 订阅主题，返回订阅 id（供取消订阅）。
    pub fn subscribe(&self, topic: &str, handler: impl Fn(Value) + Send + Sync + 'static) -> usize {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        self.subscribers
            .lock_ok()
            .entry(topic.to_string())
            .or_default()
            .insert(id, Box::new(handler));
        id
    }

    /// 取消订阅。
    pub fn unsubscribe(&self, topic: &str, id: usize) {
        if let Some(handlers) = self.subscribers.lock_ok().get_mut(topic) {
            handlers.remove(&id);
        }
    }
}

impl Default for MessageBus {
    fn default() -> Self {
        Self::new()
    }
}
