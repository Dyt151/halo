//! 音量调整反馈音。
//!
//! 后台轮询默认输出音量：键盘音量键、控制中心、其它 App 改音量都会走到这里，
//! 每变化一步响一次。面板自己写音量前会登记目标值，轮询到这次变化时不重复响。

use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::lock_ext::LockExt;

/// 轮询间隔：既要跟得上键盘连按，又不能太费。
const POLL_INTERVAL: Duration = Duration::from_millis(80);
/// 两次反馈音的最小间隔，避免长按音量键时声音叠成一团。
const MIN_TICK_GAP: Duration = Duration::from_millis(45);
/// 面板写音量的登记有效期。
const SELF_WRITE_TTL: Duration = Duration::from_secs(2);
/// 判定「音量真的变了」的最小差值。
const EPSILON: f64 = 0.01;

fn self_write() -> &'static Mutex<Option<(f64, Instant)>> {
    static SLOT: OnceLock<Mutex<Option<(f64, Instant)>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

/// 面板即将写音量：登记目标值，让轮询忽略这一次变化。
pub fn note_self_write(volume: f64) {
    *self_write().lock_ok() = Some((volume, Instant::now()));
}

fn consume_self_write(current: f64) -> bool {
    let mut guard = self_write().lock_ok();
    match guard.as_ref() {
        Some((target, at)) if at.elapsed() <= SELF_WRITE_TTL => {
            if (target - current).abs() <= 0.03 {
                *guard = None;
                true
            } else {
                false
            }
        }
        Some(_) => {
            *guard = None;
            false
        }
        None => false,
    }
}

/// 启动后台轮询（进程内只启动一次）。
pub fn start() {
    let _ = std::thread::Builder::new()
        .name("volume-feedback".to_string())
        .spawn(|| {
            let mut last = super::output_volume();
            let mut last_tick = Instant::now() - MIN_TICK_GAP;
            loop {
                std::thread::sleep(POLL_INTERVAL);
                let Some(current) = super::output_volume() else {
                    continue;
                };
                let Some(previous) = last.replace(current) else {
                    continue;
                };
                if (current - previous).abs() < EPSILON {
                    continue;
                }
                if !super::volume_feedback_enabled() || consume_self_write(current) {
                    continue;
                }
                if last_tick.elapsed() >= MIN_TICK_GAP {
                    last_tick = Instant::now();
                    crate::core::sound::play_volume_tick();
                }
            }
        });
}
