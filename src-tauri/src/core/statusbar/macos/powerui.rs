//! 「立即充满电」：PowerUI 私有框架里的充电上限（MCL）读写。
//!
//! 桥接层是 Status Trio 的 `PowerUIPrivateAPI`（Apache-2.0，见 NOTICE-STATUS-TRIO.txt），
//! 这里只做 C ABI 封装与「取消时恢复到哪个上限」的会话记账。

use std::ffi::CStr;
use std::os::raw::{c_char, c_int};

const ERROR_LEN: usize = 512;

/// 与 `native/powerui/halo_powerui_shim.m` 里的 `HaloPowerUIStateC` 逐字段对应。
#[repr(C)]
struct RawState {
    available: c_int,
    reason: c_int,
    valid: c_int,
    mcl_enabled_raw: u64,
    mcl_limit: i64,
    override_allowed: c_int,
    error: [c_char; ERROR_LEN],
}

extern "C" {
    fn halo_powerui_read(out: *mut RawState);
    fn halo_powerui_charge_to_full(err: *mut c_char, len: usize) -> c_int;
    fn halo_powerui_restore_limit(limit: c_int, err: *mut c_char, len: usize) -> c_int;
}

use crate::core::statusbar::ChargeToFullState;

/// 正版认的上限档位；系统不支持列表查询时按这套回退。
const KNOWN_LIMITS: [i64; 4] = [80, 85, 90, 95];

fn c_string(buf: &[c_char]) -> Option<String> {
    let text = unsafe { CStr::from_ptr(buf.as_ptr()) }.to_string_lossy().trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

pub fn state() -> ChargeToFullState {
    let mut raw = RawState {
        available: 0,
        reason: 0,
        valid: 0,
        mcl_enabled_raw: 0,
        mcl_limit: -1,
        override_allowed: 0,
        error: [0; ERROR_LEN],
    };
    unsafe { halo_powerui_read(&mut raw) };

    let valid = raw.valid == 1;
    let limit = (raw.mcl_limit >= 0).then_some(raw.mcl_limit);
    let reported = limit.filter(|value| KNOWN_LIMITS.contains(value));
    ChargeToFullState {
        available: raw.available == 1,
        valid,
        has_active_limit: valid && raw.mcl_enabled_raw == 1 && reported.is_some(),
        override_active: valid && raw.mcl_enabled_raw == 3,
        limit: reported,
        error: c_string(&raw.error),
    }
}

fn write_call(result: c_int, buf: &[c_char]) -> Result<(), String> {
    if result == 1 {
        Ok(())
    } else {
        Err(c_string(buf).unwrap_or_else(|| "PowerUI 拒绝了请求".to_string()))
    }
}

/// 发起「立即充满」：先记下当前上限，等用户点取消时恢复它。
pub fn charge_to_full() -> Result<(), String> {
    let before = state();
    if !before.available {
        return Err(before.error.unwrap_or_else(|| "这台 Mac 不支持充电上限控制".to_string()));
    }
    if !before.has_active_limit {
        return Err("当前没有生效的充电上限".to_string());
    }
    if let Some(limit) = before.limit {
        save_session(limit);
    }

    let mut buf = [0 as c_char; ERROR_LEN];
    let ok = unsafe { halo_powerui_charge_to_full(buf.as_mut_ptr(), ERROR_LEN) };
    if let Err(message) = write_call(ok, &buf) {
        clear_session();
        return Err(message);
    }
    Ok(())
}

/// 取消「立即充满」：恢复到发起前记下的上限，返回恢复到的档位。
pub fn cancel_charge_to_full() -> Result<i64, String> {
    let limit = load_session()
        .or_else(|| state().limit)
        .ok_or_else(|| "读不到原来的充电上限，请在系统设置里恢复".to_string())?;

    let mut buf = [0 as c_char; ERROR_LEN];
    let ok = unsafe { halo_powerui_restore_limit(limit as c_int, buf.as_mut_ptr(), ERROR_LEN) };
    write_call(ok, &buf)?;
    clear_session();
    Ok(limit)
}

fn session_path() -> std::path::PathBuf {
    crate::storage::data_dir().join("charge-to-full.json")
}

fn save_session(limit: i64) {
    let path = session_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, format!("{{\"limit\":{limit}}}"));
}

fn load_session() -> Option<i64> {
    let text = std::fs::read_to_string(session_path()).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    value.get("limit")?.as_i64().filter(|v| KNOWN_LIMITS.contains(v))
}

fn clear_session() {
    let _ = std::fs::remove_file(session_path());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 读的是真机状态，CI 上没有意义，只在手动核对桥接时跑。
    #[test]
    #[ignore = "需要真实系统充电状态"]
    fn prints_live_state() {
        println!("{:?}", state());
    }
}
