//! 依赖私有 API / root 守护进程的能力：**只留接口与能力声明，不实现**。
//!
//! 按约定这三项要等后续再接（status-trio 的实现方式已调研清楚）：
//! - AirPods 降噪：私有 CoreAudio HAL 选择器 `lstm` / `lsms` / `iaap`；
//! - 充电上限：私有 `PowerUI.framework`（`PowerUISmartChargeClient`），macOS 26.4+；
//! - iPhone / Apple Watch 电量：内置 libimobiledevice 辅助进程（LGPL 依赖）。
//!
//! 面板读取 [`capabilities`] 来决定是否渲染这些行，避免出现「点了没反应」的假控件。

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Capability {
    pub id: &'static str,
    pub name: &'static str,
    /// 当前构建是否真的实现了该能力。
    pub supported: bool,
    /// 计划接入的批次（1 = 已实现，2 = 网络/蓝牙/VPN，3 = 私有能力）。
    pub phase: u8,
    /// 给用户看的一句话说明。
    pub reason: Option<&'static str>,
}

/// 面板设置页与提示条共用的能力清单。
pub fn capabilities() -> Vec<Capability> {
    vec![
        Capability {
            id: "airpods_anc",
            name: "AirPods 降噪切换",
            supported: false,
            phase: 3,
            reason: Some("需要私有 CoreAudio 选择器，已预留接口，尚未接入"),
        },
        Capability {
            id: "charge_limit",
            name: "电池充电上限",
            supported: false,
            phase: 3,
            reason: Some("需要 PowerUI 私有框架，已预留接口，尚未接入"),
        },
        Capability {
            id: "mobile_battery",
            name: "iPhone / Apple Watch 电量",
            supported: false,
            phase: 3,
            reason: Some("需要内置 libimobiledevice 辅助进程，已预留接口，尚未接入"),
        },
    ]
}

/// 降噪模式（未来 AirPods 行使用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AncMode {
    Off,
    NoiseCancellation,
    Transparency,
}

/// AirPods 降噪控制接口。
///
/// 将来的实现走私有 CoreAudio HAL 的 4CC 选择器（`lstm` 读、`lsms` 写），
/// 失败时返回 `Err`，由面板降级为「不可用」而不是崩溃。
pub trait AncControl {
    fn available(&self) -> bool {
        false
    }
    fn mode(&self) -> Result<AncMode, String> {
        Err("尚未接入".to_string())
    }
    fn set_mode(&self, _mode: AncMode) -> Result<(), String> {
        Err("尚未接入".to_string())
    }
}

/// 充电上限控制接口（未来走 PowerUI 私有 API）。
pub trait ChargeLimitControl {
    fn available(&self) -> bool {
        false
    }
    /// 当前上限（百分比）。
    fn limit(&self) -> Result<u8, String> {
        Err("尚未接入".to_string())
    }
    fn set_limit(&self, _percent: u8) -> Result<(), String> {
        Err("尚未接入".to_string())
    }
}

/// 移动设备电量读取接口（未来走 libimobiledevice 辅助进程）。
pub trait MobileBatteryReader {
    fn available(&self) -> bool {
        false
    }
    /// `(设备名, 电量百分比, 是否充电)`。
    fn read(&self) -> Result<Vec<(String, u8, bool)>, String> {
        Err("尚未接入".to_string())
    }
}
