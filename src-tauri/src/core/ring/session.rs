//! 进程级会话状态：当前状态机、撤销代际与快照数据。

use super::*;

pub(super) fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub(super) fn session() -> Arc<Mutex<Session>> {
    SESSION
        .get_or_init(|| Arc::new(Mutex::new(Session::default())))
        .clone()
}

impl State {
    pub(super) fn as_str(&self) -> &'static str {
        match self {
            State::Idle => "idle",
            State::Capturing => "capturing",
            State::Probing => "probing",
            State::Ready => "ready",
            State::Empty => "empty",
            State::Converting => "converting",
        }
    }
}

impl Session {
    /// 取出构建快照所需的数据（避免长期持锁）。
    pub(super) fn clone_snapshot_data(&self) -> (u64, State, Vec<String>) {
        (self.generation, self.state.clone(), self.paths.clone())
    }
}
