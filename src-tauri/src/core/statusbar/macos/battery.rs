//! IOKit 电源快照：电量、充电状态、剩余时间与电池详情读数。
//!
//! 电量走公开的 IOPowerSources 接口（IOPowerSources.h）。详情页的功率/循环数
//! 走 `AppleSmartBattery` 注册表与 `IOPSCopyExternalPowerAdapterDetails`——这两条
//! 也是公开符号，不需要 root，也不用碰 SMC 用户客户端。低电量模式**只读**
//! （`NSProcessInfo`），切换它才需要 root 守护进程，本版不做。

use super::cf;
use crate::core::statusbar::{BatteryDetails, BatteryState};

#[link(name = "IOKit", kind = "framework")]
extern "C" {
    fn IOPSCopyPowerSourcesInfo() -> cf::CFTypeRef;
    fn IOPSCopyPowerSourcesList(blob: cf::CFTypeRef) -> cf::CFTypeRef;
    fn IOPSGetPowerSourceDescription(blob: cf::CFTypeRef, source: cf::CFTypeRef) -> cf::CFTypeRef;
    fn IOPSCopyExternalPowerAdapterDetails() -> cf::CFTypeRef;
    fn IOServiceMatching(name: *const i8) -> cf::CFTypeRef;
    fn IOServiceGetMatchingService(main_port: u32, matching: cf::CFTypeRef) -> u32;
    fn IORegistryEntryCreateCFProperties(
        entry: u32,
        properties: *mut cf::CFTypeRef,
        alloc: cf::CFTypeRef,
        options: u32,
    ) -> i32;
    fn IOObjectRelease(object: u32) -> i32;
}

/// `kIOMainPortDefault`：两个版本的常量都等于 MACH_PORT_NULL。
const MAIN_PORT: u32 = 0;

fn with_key<T>(
    dict: cf::CFTypeRef,
    key: &str,
    read: unsafe fn(cf::CFTypeRef) -> Option<T>,
) -> Option<T> {
    let k = cf::cfstr(key)?;
    let value = unsafe { cf::dict_get(dict, k) };
    let out = unsafe { read(value) };
    unsafe { cf::release(k) };
    out
}

fn dict_i32(dict: cf::CFTypeRef, key: &str) -> Option<i32> {
    with_key(dict, key, |v| unsafe { cf::i32_value(v) })
}

fn dict_bool(dict: cf::CFTypeRef, key: &str) -> Option<bool> {
    with_key(dict, key, |v| unsafe { cf::bool_value(v) })
}

fn dict_string(dict: cf::CFTypeRef, key: &str) -> Option<String> {
    with_key(dict, key, |v| unsafe { cf::string(v) })
}

fn dict_i64(dict: cf::CFTypeRef, key: &str) -> Option<i64> {
    with_key(dict, key, |v| unsafe { cf::i64_value(v) })
}

fn dict_f64(dict: cf::CFTypeRef, key: &str) -> Option<f64> {
    with_key(dict, key, |v| unsafe { cf::f64_value(v) })
}

fn absent() -> BatteryState {
    BatteryState {
        present: false,
        percent: 0,
        charging: false,
        charged: false,
        plugged: false,
        low_power: false,
        minutes_to_empty: None,
        minutes_to_full: None,
    }
}

/// 系统「低电量模式」：正版的权威值就是 `ProcessInfo.isLowPowerModeEnabled`，
/// 这里走同一个属性。它决定电池弧是不是画成黄色，跟插不插电无关。
fn low_power_mode() -> bool {
    use objc2::msg_send;
    use objc2::runtime::{AnyObject, Bool};
    use objc2_foundation::NSAutoreleasePool;

    // +processInfo 返回 autoreleased 对象，命令线程没有池，必须自建。
    let _pool = unsafe { NSAutoreleasePool::new() };
    unsafe {
        let info: *mut AnyObject = msg_send![objc2::class!(NSProcessInfo), processInfo];
        if info.is_null() {
            return false;
        }
        let enabled: Bool = msg_send![info, isLowPowerModeEnabled];
        enabled.as_bool()
    }
}

pub fn state() -> BatteryState {
    unsafe {
        let blob = IOPSCopyPowerSourcesInfo();
        if blob.is_null() {
            return absent();
        }
        let list = IOPSCopyPowerSourcesList(blob);
        if list.is_null() {
            cf::release(blob);
            return absent();
        }
        let count = cf::array_count(list);
        let mut result = absent();
        for i in 0..count {
            let source = cf::array_at(list, i);
            let desc = IOPSGetPowerSourceDescription(blob, source);
            if desc.is_null() {
                continue;
            }
            if dict_string(desc, "Type").as_deref() != Some("InternalBattery") {
                continue;
            }
            let current = dict_i32(desc, "Current Capacity").unwrap_or(0).max(0);
            let max = dict_i32(desc, "Max Capacity").unwrap_or(100).max(1);
            let percent = ((current as f64 / max as f64) * 100.0).round() as u8;
            let state_text = dict_string(desc, "Power Source State").unwrap_or_default();
            let to_empty = dict_i32(desc, "Time to Empty").filter(|m| *m > 0);
            let to_full = dict_i32(desc, "Time to Full Charge").filter(|m| *m > 0);
            result = BatteryState {
                present: true,
                percent: percent.min(100),
                charging: dict_bool(desc, "Is Charging").unwrap_or(false),
                charged: dict_bool(desc, "Is Charged").unwrap_or(false),
                plugged: state_text == "AC Power",
                low_power: low_power_mode(),
                minutes_to_empty: to_empty.map(i64::from),
                minutes_to_full: to_full.map(i64::from),
            };
            break;
        }
        cf::release(list);
        cf::release(blob);
        result
    }
}

/// 本机时区的 `HH:mm:ss`：详情页「整机读取时间」就是采样落进内存的那一刻。
fn local_clock() -> Option<String> {
    unsafe {
        let mut now: libc::time_t = 0;
        libc::time(&mut now);
        let mut broken: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&now, &mut broken).is_null() {
            return None;
        }
        Some(format!(
            "{:02}:{:02}:{:02}",
            broken.tm_hour, broken.tm_min, broken.tm_sec
        ))
    }
}

/// 适配器额定功率（W）。`IOPSCopyExternalPowerAdapterDetails` 在拔电后仍可能
/// 留着上一次的读数，所以调用方只在接电时采用它。
fn adapter_watts() -> Option<u32> {
    unsafe {
        let details = IOPSCopyExternalPowerAdapterDetails();
        if details.is_null() {
            return None;
        }
        let watts = dict_i64(details, "Watts").filter(|v| *v > 0).map(|v| v as u32);
        cf::release(details);
        watts
    }
}

/// 详情页读数：功率三档 + 适配器额定功率 + 循环数 + 电压电流。
///
/// 功率与电池电压/电流优先取 SMC（`PSTR` / `PDTR` / `B0AV` / `B0AC`），
/// 与正版 `SystemPowerReader` 同源；SMC 读不到才退回 `AppleSmartBattery`
/// 注册表的 `PowerTelemetryData`（单位毫瓦）与 `Voltage`/`Amperage`（毫伏/毫安，
/// `Amperage` 用补码表示放电为负，必须按有符号读）。
pub fn details() -> BatteryDetails {
    let mut out = BatteryDetails {
        sampled_at: local_clock(),
        ..BatteryDetails::default()
    };
    let sensors = super::smc::read();
    out.system_watts = sensors.system_watts;
    out.input_watts = sensors.input_watts;
    out.battery_watts = sensors.battery_watts().filter(|watts| *watts > 0.0);
    out.voltage_volts = sensors.battery_volts;
    out.current_amps = sensors.battery_amps;
    unsafe {
        out.adapter_watts = adapter_watts();

        let name = b"AppleSmartBattery\0".as_ptr().cast::<i8>();
        let matching = IOServiceMatching(name);
        if matching.is_null() {
            return out;
        }
        // IOServiceGetMatchingService 会吃掉 matching 字典的这一次引用。
        let entry = IOServiceGetMatchingService(MAIN_PORT, matching);
        if entry == 0 {
            return out;
        }
        let mut props: cf::CFTypeRef = std::ptr::null();
        let status = IORegistryEntryCreateCFProperties(entry, &mut props, std::ptr::null(), 0);
        IOObjectRelease(entry);
        if status != 0 || props.is_null() {
            return out;
        }

        out.cycle_count = dict_i64(props, "CycleCount")
            .filter(|count| *count >= 0)
            .map(|count| count as u32);
        if out.voltage_volts.is_none() {
            out.voltage_volts = dict_f64(props, "Voltage")
                .filter(|mv| *mv > 0.0)
                .map(|mv| mv / 1000.0);
        }
        if out.current_amps.is_none() {
            out.current_amps = dict_i64(props, "Amperage").map(|ma| ma as f64 / 1000.0);
        }

        if let Some(telemetry) = cf::sub_dict(props, "PowerTelemetryData") {
            let milliwatts = |key: &str| {
                dict_f64(telemetry, key)
                    .filter(|mw| mw.is_finite())
                    .map(|mw| mw / 1000.0)
            };
            // 正版把 `SystemPowerIn` 当作 PDTR 缺席时的回落，仅此一项。
            if out.input_watts.is_none() {
                out.input_watts = milliwatts("SystemPowerIn");
            }
        }
        cf::release(props);
    }
    out
}
