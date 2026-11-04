//! 蓝牙状态与开关。
//!
//! 设备清单走公开的 `system_profiler -json`（一次一秒以上，只在面板打开时用）；
//! 控制器开关走 IOBluetooth 的偏好符号 `…GetControllerPowerState`，但这个符号可能
//! 永久卡住（见 `probe`），所以图标读的是后台探测写好的缓存，不直接调它。
//! 开关没有公开 API，只有 `…SetControllerPowerState`
//! （blueutil 用的同一套）：符号按需 `dlsym`，缺失时返回错误，
//! 面板降级为只读而不是崩溃。

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::OnceLock;

use crate::core::statusbar::{BluetoothDevice, BluetoothState};

type SetPower = unsafe extern "C" fn(i32);
type GetPower = unsafe extern "C" fn() -> i32;

#[derive(Clone, Copy)]
struct Symbols {
    set_power: Option<SetPower>,
    get_power: Option<GetPower>,
}

static SYMBOLS: OnceLock<Symbols> = OnceLock::new();

const NONE: Symbols = Symbols {
    set_power: None,
    get_power: None,
};

/// 一次 `dlopen` 取到两个偏好符号；缺符号（老系统/受限环境）时缓存 `None`，
/// 面板降级为只读、图标不选蓝牙中心，而不是崩溃。
fn symbols() -> Symbols {
    *SYMBOLS.get_or_init(|| {
        let handle = unsafe {
            libc::dlopen(
                c"/System/Library/Frameworks/IOBluetooth.framework/Versions/A/IOBluetooth".as_ptr(),
                libc::RTLD_LAZY,
            )
        };
        if handle.is_null() {
            return NONE;
        }
        unsafe {
            let set = libc::dlsym(
                handle,
                c"IOBluetoothPreferenceSetControllerPowerState".as_ptr(),
            );
            let get = libc::dlsym(
                handle,
                c"IOBluetoothPreferenceGetControllerPowerState".as_ptr(),
            );
            Symbols {
                set_power: (!set.is_null())
                    .then(|| std::mem::transmute::<*mut libc::c_void, SetPower>(set)),
                get_power: (!get.is_null())
                    .then(|| std::mem::transmute::<*mut libc::c_void, GetPower>(get)),
            }
        }
    })
}

/// 控制器开关的缓存：0 = 还没读到，1 = 关，2 = 开。
static POWER: AtomicU8 = AtomicU8::new(0);
/// 有一个探测线程正在读系统符号。
static PROBING: AtomicBool = AtomicBool::new(false);
/// 上一次发起探测的单调时间（秒）。
static LAST_PROBE: AtomicU64 = AtomicU64::new(0);

/// 两次探测的最小间隔：和图标轮询同量级，开关真变了下一轮就能补上。
const REPROBE_SECS: u64 = 2;

/// 后台探测一次开关，结果写进缓存。
///
/// 为什么不直接读：`IOBluetoothPreferenceGetControllerPowerState` 首次调用会
/// `dispatch_once` 初始化 `IOBluetoothCoreBluetoothCoordinator`，后者等一个
/// CoreBluetooth 回调。蓝牙隐私没批（或还在批）时这个回调永远不来，调用方
/// 就地冻死——命令行工具里 100ms 返回，App 里实测卡住几十秒。图标重画线程每两秒
/// 读一次，绝不能被这种调用挂住，所以读数一律走缓存，真正的调用丢给一次性线程；
/// 线程卡在里面的最坏结果只是「蓝牙状态读不到」，图标照常画。
fn probe() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if now - LAST_PROBE.load(Ordering::Relaxed) < REPROBE_SECS {
        return;
    }
    // 同一时刻只允许一个探测线程：卡住的那个不会被第二个替掉，也不会堆积。
    if PROBING.swap(true, Ordering::Relaxed) {
        return;
    }
    LAST_PROBE.store(now, Ordering::Relaxed);
    let Some(get) = symbols().get_power else {
        PROBING.store(false, Ordering::Relaxed);
        return;
    };
    let spawned = std::thread::Builder::new()
        .name("bt-power-probe".to_string())
        .spawn(move || {
            let on = unsafe { get() } != 0;
            POWER.store(if on { 2 } else { 1 }, Ordering::Relaxed);
            PROBING.store(false, Ordering::Relaxed);
        });
    if spawned.is_err() {
        PROBING.store(false, Ordering::Relaxed);
    }
}

/// 控制器开关的轻量读取（缓存值，永不阻塞）。
///
/// 菜单栏图标每两秒重画一次，靠 `system_profiler` 的话一次要一秒以上；这里读的是
/// 后台探测写好的缓存。`None` = 还没读到（首轮或系统不支持），中心图形因此不选蓝牙。
pub fn controller_powered() -> Option<bool> {
    probe();
    match POWER.load(Ordering::Relaxed) {
        1 => Some(false),
        2 => Some(true),
        _ => None,
    }
}

pub fn state() -> BluetoothState {
    let cached = controller_powered();
    match read_profile() {
        Some(profile) => {
            // 符号还没读到时先信 system_profiler，面板打开一次就能把控制器状态补进
            // 缓存，菜单栏图标随后也有蓝牙可选。
            if let Some(powered) = profile.powered {
                let _ = POWER.compare_exchange(
                    0,
                    if powered { 2 } else { 1 },
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                );
            }
            BluetoothState {
                available: true,
                // 控制器开关以符号读取为准：system_profiler 偶尔慢一拍，图标与面板会打架。
                powered: cached.or(profile.powered).unwrap_or(false),
                devices: profile.devices,
            }
        }
        // 清单读不出来（超时/格式变化）但缓存里有开关状态：如实报告开关，没有设备列表。
        None if cached.is_some() => BluetoothState {
            available: true,
            powered: cached.unwrap_or(false),
            devices: Vec::new(),
        },
        // 两边都读不到：可能没有蓝牙硬件。
        None => BluetoothState::unavailable(),
    }
}

/// `system_profiler` 里我们关心的部分：控制器开关可能缺席，设备清单可能为空。
struct Profile {
    powered: Option<bool>,
    devices: Vec<BluetoothDevice>,
}

fn read_profile() -> Option<Profile> {
    let output = std::process::Command::new("/usr/sbin/system_profiler")
        .args(["SPBluetoothDataType", "-json"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    parse_profile(&json)
}

fn parse_profile(json: &serde_json::Value) -> Option<Profile> {
    let root = json.get("SPBluetoothDataType")?.as_array()?.first()?;
    let powered = root
        .get("controller_properties")
        .and_then(|c| c.get("controller_state"))
        .and_then(|v| v.as_str())
        .and_then(|raw| match raw {
            "attrib_on" => Some(true),
            "attrib_off" => Some(false),
            _ => None,
        });

    // 已连接先收，未连接的已配对设备后收：同一地址只保留已连接那一条。
    let mut seen = std::collections::HashSet::new();
    let mut devices = Vec::new();
    collect_devices(root, "device_connected", true, &mut seen, &mut devices);
    collect_devices(root, "device_not_connected", false, &mut seen, &mut devices);
    order_devices(&mut devices);

    if powered.is_none() && devices.is_empty() {
        // 没有控制器状态也没有设备：多半是没有蓝牙硬件。
        return None;
    }
    Some(Profile { powered, devices })
}

/// 正版 `normalizedAddress`：只留十六进制字符并转大写。
fn normalized_address(raw: &str) -> String {
    raw.chars()
        .filter(|c| c.is_ascii_hexdigit())
        .flat_map(|c| c.to_uppercase())
        .collect()
}

/// 正版的「幽灵设备」判据：profiler 既给不出 `device_minorType`，
/// 也给不出 `device_minorClassOfDevice_string`——iPhone / iPad / Watch 的
/// BLE 广播就是这样混进列表的，正版直接丢掉。
fn is_ghost(info: &serde_json::Value) -> bool {
    info.get("device_minorType").and_then(|v| v.as_str()).is_none()
        && info
            .get("device_minorClassOfDevice_string")
            .and_then(|v| v.as_str())
            .is_none()
}

fn kind_of(info: &serde_json::Value) -> Option<String> {
    info.get("device_minorType")
        .or_else(|| info.get("device_minorClassOfDevice_string"))
        .or_else(|| info.get("device_majorClass"))
        .and_then(|v| v.as_str())
        .map(str::to_owned)
}

fn collect_devices(
    root: &serde_json::Value,
    key: &str,
    connected: bool,
    seen: &mut std::collections::HashSet<String>,
    out: &mut Vec<BluetoothDevice>,
) {
    let Some(list) = root.get(key).and_then(|v| v.as_array()) else {
        return;
    };
    for entry in list {
        let Some(map) = entry.as_object() else {
            continue;
        };
        for (name, info) in map {
            // 正版要求「地址非空 + 名称非空」，否则这条记录没意义。
            let Some(address) = info.get("device_address").and_then(|v| v.as_str()) else {
                continue;
            };
            if name.is_empty() || address.is_empty() || is_ghost(info) {
                continue;
            }
            let dedup_key = normalized_address(address);
            if !dedup_key.is_empty() && !seen.insert(dedup_key) {
                continue;
            }
            out.push(BluetoothDevice {
                name: name.clone(),
                address: Some(address.to_string()),
                battery: battery_percent(info),
                connected,
                kind: kind_of(info),
            });
        }
    }
}

/// 正版排序：已连接一组在前，组内「AirPods 类音频设备」领头，其余按名称不区分大小写。
fn order_devices(devices: &mut Vec<BluetoothDevice>) {
    devices.sort_by(|lhs, rhs| {
        let left_group = !lhs.connected;
        let right_group = !rhs.connected;
        left_group.cmp(&right_group).then_with(|| {
            is_airpods(lhs)
                .cmp(&is_airpods(rhs))
                .reverse()
                .then_with(|| lhs.name.to_lowercase().cmp(&rhs.name.to_lowercase()))
        })
    });
}

fn is_airpods(device: &BluetoothDevice) -> bool {
    let kind = device.kind.clone().unwrap_or_default().to_ascii_lowercase();
    let audio = kind.contains("headphone")
        || kind.contains("airpod")
        || kind.contains("earbud")
        || kind.contains("speaker");
    audio && device.name.to_lowercase().contains("airpods")
}

fn battery_percent(info: &serde_json::Value) -> Option<u8> {
    [
        "device_batteryLevel",
        "device_batteryLevelLeft",
        "device_batteryLevelMain",
        "device_batteryLevelRight",
        "device_batteryLevelCase",
    ]
    .iter()
    .find_map(|key| info.get(*key).and_then(|v| v.as_str()))
    .and_then(|raw| raw.trim_end_matches('%').trim().parse().ok())
}

pub fn set_power(on: bool) -> Result<(), String> {
    let set = symbols()
        .set_power
        .ok_or_else(|| "当前系统不支持直接切换蓝牙（缺少 IOBluetooth 符号）".to_string())?;
    unsafe { set(if on { 1 } else { 0 }) };
    // 立刻写缓存：图标要马上反映这次开关，而后台探测最长要等一个间隔才读到。
    POWER.store(if on { 2 } else { 1 }, Ordering::Relaxed);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        battery_percent, order_devices, parse_profile, BluetoothDevice,
    };

    #[test]
    fn battery_percent_accepts_percent_strings() {
        let json = serde_json::json!({ "device_batteryLevelLeft": "82%" });
        assert_eq!(battery_percent(&json), Some(82));
        let missing = serde_json::json!({});
        assert_eq!(battery_percent(&missing), None);
        let garbage = serde_json::json!({ "device_batteryLevel": "—" });
        assert_eq!(battery_percent(&garbage), None);
    }

    #[test]
    fn profile_lists_connected_and_paired_devices() {
        let json = serde_json::json!({
            "SPBluetoothDataType": [{
                "controller_properties": { "controller_state": "attrib_on" },
                "device_connected": [{
                    "AirPods Pro": {
                        "device_address": "AA:BB:CC:DD:EE:FF",
                        "device_minorType": "Headphones",
                        "device_batteryLevelMain": "76%"
                    }
                }],
                "device_not_connected": [{
                    "Magic Mouse": {
                        "device_address": "11:22:33:44:55:66",
                        "device_minorType": "Mouse"
                    }
                }]
            }]
        });
        let profile = parse_profile(&json).expect("profile");
        assert_eq!(profile.powered, Some(true));
        assert_eq!(profile.devices.len(), 2);
        let airpods = &profile.devices[0];
        assert!(airpods.connected);
        assert_eq!(airpods.battery, Some(76));
        assert_eq!(airpods.kind.as_deref(), Some("Headphones"));
        let mouse = &profile.devices[1];
        assert!(!mouse.connected);
        assert_eq!(mouse.battery, None);
    }

    #[test]
    fn profile_without_controller_or_devices_is_none() {
        let empty = serde_json::json!({ "SPBluetoothDataType": [{}] });
        assert!(parse_profile(&empty).is_none());
        // 控制器关着也算有效读数（面板要显示「蓝牙已关闭」而不是消失）。
        let off = serde_json::json!({
            "SPBluetoothDataType": [{
                "controller_properties": { "controller_state": "attrib_off" }
            }]
        });
        assert_eq!(parse_profile(&off).expect("profile").powered, Some(false));
    }

    #[test]
    fn ghosts_and_addressless_entries_are_dropped() {
        // 正版只丢「两个 minor 字段都没有」的设备：iPhone / iPad / Watch 的
        // BLE 广播就是这样混进来的，不是按名字猜。
        let json = serde_json::json!({
            "SPBluetoothDataType": [{
                "controller_properties": { "controller_state": "attrib_on" },
                "device_connected": [{
                    "AirPods Pro": {
                        "device_address": "AA:BB:CC:DD:EE:FF",
                        "device_minorType": "Headphones"
                    },
                    "依腾的 iPhone": { "device_address": "11:11:11:11:11:11" },
                    "无地址设备": { "device_minorType": "Speaker" }
                }]
            }]
        });
        let profile = parse_profile(&json).expect("profile");
        assert_eq!(
            profile
                .devices
                .iter()
                .map(|d| d.name.as_str())
                .collect::<Vec<_>>(),
            vec!["AirPods Pro"]
        );
    }

    #[test]
    fn dedup_uses_the_normalized_address_not_the_name() {
        // 同一台设备在两个集合里各出现一次（大小写/分隔符不同）：只留已连接那条。
        let json = serde_json::json!({
            "SPBluetoothDataType": [{
                "controller_properties": { "controller_state": "attrib_on" },
                "device_connected": [{
                    "MX Master": {
                        "device_address": "aa-bb-cc-dd-ee-ff",
                        "device_minorType": "Mouse"
                    }
                }],
                "device_not_connected": [{
                    "MX Master 3": {
                        "device_address": "AA:BB:CC:DD:EE:FF",
                        "device_minorType": "Mouse"
                    }
                }]
            }]
        });
        let profile = parse_profile(&json).expect("profile");
        assert_eq!(profile.devices.len(), 1);
        assert!(profile.devices[0].connected);
        assert_eq!(profile.devices[0].name, "MX Master");
    }

    #[test]
    fn connected_airpods_lead_and_the_rest_sort_by_name() {
        let mut devices = vec![
            device("索尼 WH-1000XM5", false, "Headphones"),
            device("Magic Keyboard", true, "Keyboard"),
            device("AirPods Max", false, "Headphones"),
            device("AirPods Pro", true, "Headphones"),
        ];
        order_devices(&mut devices);
        assert_eq!(
            devices.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(),
            vec!["AirPods Pro", "Magic Keyboard", "AirPods Max", "索尼 WH-1000XM5"]
        );
    }

    fn device(name: &str, connected: bool, kind: &str) -> BluetoothDevice {
        BluetoothDevice {
            name: name.to_string(),
            address: None,
            battery: None,
            connected,
            kind: Some(kind.to_string()),
        }
    }
}
