//! Wi-Fi 状态与开关：CoreWLAN 读取 + `getifaddrs` 取 IPv4 与掩码 +
//! `SCDynamicStore` 取详情地址（正版 `NetworkServiceResolver` 同源）。
//!
//! `link` 不只分「有线/无线」：Apple 的个人热点与互联网共享各自固定用一个 /28
//! NAT 网段，所以拿 IP + 前缀长度就能区分普通 Wi-Fi、iPhone 热点、互联网共享。
//!
//! SSID 在 macOS 14.4+ 属于定位权限保护的数据：应用没有定位授权时
//! `ssid()` 返回 nil，这里如实返回 `None`，面板显示「已连接」而不是伪造网络名。
//! 权限申请见 [`request_name_permission`]。
//!
//! CoreWLAN 没有 Rust 绑定，用 objc2 的运行时消息发送；框架按需 `dlopen`，
//! 老系统上缺符号时整块降级为不可用，不影响其它模块。

use std::ffi::{c_void, CStr};
use std::os::raw::c_char;
use std::ptr;
use std::sync::OnceLock;

use objc2::msg_send;
use objc2::runtime::{AnyClass, AnyObject, Bool};
use objc2_foundation::NSAutoreleasePool;

use super::super::cf;
use super::super::{WifiCandidate, WifiDetails, WifiScan};
use crate::core::statusbar::NetworkState;

static COREWLAN_LOADED: OnceLock<bool> = OnceLock::new();
static CORELOCATION_LOADED: OnceLock<bool> = OnceLock::new();

fn corewlan_loaded() -> bool {
    *COREWLAN_LOADED.get_or_init(|| unsafe {
        !libc::dlopen(
            c"/System/Library/Frameworks/CoreWLAN.framework/CoreWLAN".as_ptr(),
            libc::RTLD_LAZY,
        )
        .is_null()
    })
}

fn corelocation_loaded() -> bool {
    *CORELOCATION_LOADED.get_or_init(|| unsafe {
        !libc::dlopen(
            c"/System/Library/Frameworks/CoreLocation.framework/CoreLocation".as_ptr(),
            libc::RTLD_LAZY,
        )
        .is_null()
    })
}

/// 进程内「定位授权框是否已经弹过一次」（正版 `hasRequestedAuthorization`）。
static HAS_REQUESTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// `CWWiFiClient.sharedClient.interface`，没有 Wi-Fi 硬件时返回 None。
fn interface() -> Option<*mut AnyObject> {
    if !corewlan_loaded() {
        return None;
    }
    let cls = AnyClass::get("CWWiFiClient")?;
    unsafe {
        let client: *mut AnyObject = msg_send![cls, sharedWiFiClient];
        if client.is_null() {
            return None;
        }
        let iface: *mut AnyObject = msg_send![client, interface];
        (!iface.is_null()).then_some(iface)
    }
}

unsafe fn ns_string(obj: *mut AnyObject) -> Option<String> {
    if obj.is_null() {
        return None;
    }
    let utf8: *const c_char = msg_send![obj, UTF8String];
    if utf8.is_null() {
        return None;
    }
    Some(CStr::from_ptr(utf8).to_string_lossy().into_owned())
}

/// 本机所有非回环 IPv4（接口名, 点分地址, 前缀长度）。
fn ipv4_addresses() -> Vec<(String, String, u8)> {
    let mut result = Vec::new();
    unsafe {
        let mut head: *mut libc::ifaddrs = ptr::null_mut();
        if libc::getifaddrs(&mut head) != 0 {
            return result;
        }
        let mut cur = head;
        while !cur.is_null() {
            let ifa = &*cur;
            if !ifa.ifa_addr.is_null() && (*ifa.ifa_addr).sa_family as i32 == libc::AF_INET {
                let sin = ifa.ifa_addr.cast::<libc::sockaddr_in>();
                let ip = std::net::Ipv4Addr::from(u32::from_be((*sin).sin_addr.s_addr));
                if !ip.is_loopback() && !ip.is_link_local() {
                    let name = CStr::from_ptr(ifa.ifa_name).to_string_lossy().into_owned();
                    // 没有掩码（point-to-point 链路）时按 /32 处理，不会误判成热点段。
                    let prefix = if ifa.ifa_netmask.is_null() {
                        32
                    } else {
                        let mask = ifa.ifa_netmask.cast::<libc::sockaddr_in>();
                        u32::from_be((*mask).sin_addr.s_addr).count_ones() as u8
                    };
                    result.push((name, ip.to_string(), prefix));
                }
            }
            cur = ifa.ifa_next;
        }
        libc::freeifaddrs(head);
    }
    result
}

/// 当前首选链路的 IPv4：优先 Wi-Fi 接口，否则第一个 `en*`（有线/雷雳网卡）。
fn choose_ipv4(
    wifi_if: Option<&str>,
    wifi_connected: bool,
    addrs: &[(String, String, u8)],
) -> Option<(String, String, u8)> {
    if wifi_connected {
        if let Some(name) = wifi_if {
            if let Some(found) = addrs.iter().find(|(iface, _, _)| iface == name) {
                return Some(found.clone());
            }
        }
    }
    addrs
        .iter()
        .find(|(iface, _, _)| iface.starts_with("en"))
        .cloned()
}

fn preferred_ipv4(wifi_if: Option<&str>, wifi_connected: bool) -> Option<(String, String, u8)> {
    choose_ipv4(wifi_if, wifi_connected, &ipv4_addresses())
}

/// Wi-Fi 链路的实际种类。
///
/// Apple 的两套 NAT 网段是写死的，所以「连的是普通路由器还是手机热点」不用猜：
/// 个人热点固定发 172.20.10.0/28，Mac 自己的互联网共享固定发 192.168.2.0/28。
/// 只有 /28 才算数——公司里用 172.20.10.0/24 的网段不该被认成热点。
fn classify_link(wifi_connected: bool, ip: &str, prefix: u8, ssid: Option<&str>) -> &'static str {
    if !wifi_connected {
        return "ethernet";
    }
    // 临时接入的网络（802.11u/Passpoint 的临时关联）SSID 以 `Temp-` 开头。
    if ssid.is_some_and(|name| name.starts_with("Temp-")) {
        return "temporary";
    }
    if prefix != 28 {
        return "wifi";
    }
    if ip.starts_with("172.20.10.") {
        "hotspot"
    } else if ip.starts_with("192.168.2.") {
        "sharing"
    } else {
        "wifi"
    }
}

/// `CLLocationManager.authorizationStatus` 的四种取值（正版 `WiFiNameAccess`）。
///
/// CoreLocation 缺失（很老的系统）时按「不限制」处理：那时 SSID 本来就能读到。
pub fn name_access() -> NameAccess {
    if !corelocation_loaded() {
        return NameAccess::Authorized;
    }
    unsafe {
        let Some(cls) = AnyClass::get("CLLocationManager") else {
            return NameAccess::Authorized;
        };
        // 框架的返回值签名是 32 位枚举（不是 NSInteger），objc2 会在运行时校验。
        let status: i32 = msg_send![cls, authorizationStatus];
        NameAccess::from_raw(status as isize)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameAccess {
    Authorized,
    NotDetermined,
    Denied,
    Restricted,
}

impl NameAccess {
    /// 0=NotDetermined 1=Restricted 2=Denied 3=AuthorizedAlways 4=AuthorizedWhenInUse。
    fn from_raw(status: isize) -> Self {
        match status {
            0 => Self::NotDetermined,
            1 => Self::Restricted,
            2 => Self::Denied,
            3 | 4 => Self::Authorized,
            // 未知取值按最保守的「未决定」处理：副标题给授权入口，不装作已授权。
            _ => Self::NotDetermined,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Authorized => "authorized",
            Self::NotDetermined => "not_determined",
            Self::Denied => "denied",
            Self::Restricted => "restricted",
        }
    }
}

/// 正版用 `interface.wlanChannel().channelBand` 的 1/2/3 映射成显示串。
unsafe fn band(iface: *mut AnyObject) -> Option<String> {
    let channel: *mut AnyObject = msg_send![iface, wlanChannel];
    if channel.is_null() {
        return None;
    }
    let raw: isize = msg_send![channel, channelBand];
    Some(
        match raw {
            1 => "2.4 GHz",
            2 => "5 GHz",
            3 => "6 GHz",
            _ => return None,
        }
        .to_string(),
    )
}

pub fn state() -> NetworkState {
    // CWWiFiClient 返回的是 autoreleased 对象：命令线程没有池，必须自建。
    let _pool = unsafe { NSAutoreleasePool::new() };
    let Some(iface) = interface() else {
        return NetworkState::unavailable();
    };
    let access = name_access();
    unsafe {
        let powered: Bool = msg_send![iface, powerOn];
        let active: Bool = msg_send![iface, serviceActive];
        let rssi: isize = msg_send![iface, rssiValue];
        let ssid: *mut AnyObject = msg_send![iface, ssid];
        let name: *mut AnyObject = msg_send![iface, interfaceName];
        let band = band(iface);

        let powered = powered.as_bool();
        // 正版在发布处做同样的门控：没有定位授权时 SSID 视为读不到，
        // 不是「没有网络」，副标题要转成授权入口。
        let ssid = ns_string(ssid).filter(|value| !value.is_empty());
        let ssid = (access == NameAccess::Authorized).then_some(ssid).flatten();
        let interface = ns_string(name);
        // serviceActive 在少数机型上要等链路就绪才翻真，RSSI 为负同样说明已关联。
        let connected = powered && (active.as_bool() || ssid.is_some() || rssi < 0);
        let (link, ipv4) = match preferred_ipv4(interface.as_deref(), connected) {
            Some((iface, ip, prefix))
                if connected && Some(iface.as_str()) == interface.as_deref() =>
            {
                (classify_link(true, &ip, prefix, ssid.as_deref()), Some(ip))
            }
            Some((_, ip, _)) => ("ethernet", Some(ip)),
            None => ("none", None),
        };

        NetworkState {
            available: true,
            powered,
            connected,
            ssid: if connected { ssid } else { None },
            rssi: (connected && rssi < 0).then_some(rssi as i32),
            band: (connected && matches!(link, "wifi" | "hotspot"))
                .then(|| band)
                .flatten(),
            name_access: access.key().to_string(),
            interface,
            ipv4,
            link: link.to_string(),
        }
    }
}

/* ------------------------------------------------------------------ */
/* Wi-Fi 二级页：全信道扫描 + 连接详情                                  */
/* ------------------------------------------------------------------ */

#[link(name = "SystemConfiguration", kind = "framework")]
extern "C" {
    /// `SCDynamicStoreCreate(allocator, name, callout, context)`——回调与上下文都传 NULL，
    /// 我们只做一次性读数，不订阅变化。返回值归调用方释放。
    fn SCDynamicStoreCreate(
        allocator: cf::CFTypeRef,
        name: cf::CFTypeRef,
        callout: *const c_void,
        context: *const c_void,
    ) -> cf::CFTypeRef;
    fn SCDynamicStoreCopyKeyList(store: cf::CFTypeRef, pattern: cf::CFTypeRef) -> cf::CFTypeRef;
    fn SCDynamicStoreCopyValue(store: cf::CFTypeRef, key: cf::CFTypeRef) -> cf::CFTypeRef;
}

const SERVICE_PREFIX: &str = "State:/Network/Service/";
const GLOBAL_IPV4_KEY: &str = "State:/Network/Global/IPv4";

/// 正版 `WiFiSecurityKind` 的判定顺序：一个 AP 常常同时支持好几种安全，取第一个命中的。
///
/// 数值就是 `CWSecurity` 的原始值（CoreWLANTypes.h），正版的枚举与它一一对应，
/// 所以可以直接透出，前端只做显示名的映射。都不支持时为 -1（正版 `.unknown`）。
const SECURITY_PRIORITY: [isize; 16] = [13, 12, 11, 15, 14, 9, 8, 7, 10, 4, 3, 2, 5, 6, 1, 0];

/// 正版 `normalizedMeasurement`：RSSI / 噪声只在为负时才算读数。
fn normalized_measurement(raw: isize) -> Option<i32> {
    (raw < 0).then_some(raw as i32)
}

/// 正版 `displayBand`（`CWChannelBand` 的 1/2/3）。
fn display_band(raw: isize) -> Option<String> {
    Some(
        match raw {
            1 => "2.4 GHz",
            2 => "5 GHz",
            3 => "6 GHz",
            _ => return None,
        }
        .to_string(),
    )
}

/// 正版 `displayChannelWidth`（`CWChannelWidth` 的 1..4）。
fn display_channel_width(raw: isize) -> Option<String> {
    Some(
        match raw {
            1 => "20 MHz",
            2 => "40 MHz",
            3 => "80 MHz",
            4 => "160 MHz",
            _ => return None,
        }
        .to_string(),
    )
}

/// 正版 `displayPHY`（`CWPHYMode` 的 1..6）。照抄不扩：新增的 802.11be 也落回 nil。
fn display_phy(raw: isize) -> Option<String> {
    Some(
        match raw {
            1 => "802.11a",
            2 => "802.11b",
            3 => "802.11g",
            4 => "802.11n",
            5 => "802.11ac",
            6 => "802.11ax",
            _ => return None,
        }
        .to_string(),
    )
}

/// 正版 `WiFiSecurityKind(coreWLANRawValue:)`：越界取值一律当未知（-1）。
fn security_kind(raw: isize) -> i32 {
    if (0..=15).contains(&raw) {
        raw as i32
    } else {
        -1
    }
}

unsafe fn objc_optional_string(obj: *mut AnyObject) -> Option<String> {
    ns_string(obj).filter(|value| !value.is_empty())
}

/// 正版 `securityKind(for:)`：按优先级表问 `supportsSecurity:`。
unsafe fn supports_security(network: *mut AnyObject) -> i32 {
    for &raw in &SECURITY_PRIORITY {
        let ok: Bool = msg_send![network, supportsSecurity: raw];
        if ok.as_bool() {
            return raw as i32;
        }
    }
    -1
}

/// 正版 `projectCandidate(_:)`：SSID 读不到的那条直接丢掉（匿名广播）。
unsafe fn project_candidate(network: *mut AnyObject) -> Option<WifiCandidate> {
    let ssid = objc_optional_string(msg_send![network, ssid])?;
    let bssid = objc_optional_string(msg_send![network, bssid]);
    let rssi = normalized_measurement(msg_send![network, rssiValue]);
    let channel_obj: *mut AnyObject = msg_send![network, wlanChannel];
    let channel = if channel_obj.is_null() {
        None
    } else {
        let number: isize = msg_send![channel_obj, channelNumber];
        Some(number as i32)
    };
    Some(WifiCandidate {
        ssid,
        bssid,
        rssi,
        channel,
        security: supports_security(network),
    })
}

/// 按字符串键从动态存储的字典里取子字典（借出值，随父值一起释放）。
unsafe fn store_dict(snapshot: &[(String, cf::CFTypeRef)], key: &str) -> cf::CFTypeRef {
    snapshot
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| *value)
        .unwrap_or(std::ptr::null())
}

unsafe fn dict_field_string(dict: cf::CFTypeRef, field: &str) -> Option<String> {
    if dict.is_null() {
        return None;
    }
    let key = cf::cfstr(field)?;
    let value = cf::dict_get(dict, key);
    cf::release(key);
    cf::string(value)
}

/// 正版 `NetworkServiceResolver.strings(_:key:)`：数组或单个字符串都收。
unsafe fn dict_field_strings(dict: cf::CFTypeRef, field: &str) -> Vec<String> {
    if dict.is_null() {
        return Vec::new();
    }
    let Some(key) = cf::cfstr(field) else {
        return Vec::new();
    };
    let value = cf::dict_get(dict, key);
    cf::release(key);
    if value.is_null() {
        return Vec::new();
    }
    if cf::array_count(value) > 0 {
        let mut out = Vec::new();
        for index in 0..cf::array_count(value) {
            if let Some(item) = cf::string(cf::array_at(value, index)) {
                out.push(item);
            }
        }
        return out;
    }
    cf::string(value).map(|single| vec![single]).unwrap_or_default()
}

/// 一个网络服务的地址配置（正版 `NetworkServiceConfiguration` 里我们显示的那几项）。
struct ServiceConfig {
    ipv4_addresses: Vec<String>,
    ipv6_addresses: Vec<String>,
    router: Option<String>,
    dns_servers: Vec<String>,
}

/// 正版 `NetworkServiceResolver.selectService`：按 BSD 接口名找服务。
///
/// 命中多个又没有一个正是系统主服务时返回 None——猜错服务会把另一条链路的地址
/// 报到这个网络上，正版同样宁可留空。
unsafe fn select_service(snapshot: &[(String, cf::CFTypeRef)], interface: &str) -> Option<String> {
    let mut ids: Vec<String> = Vec::new();
    for (key, _) in snapshot {
        let Some(rest) = key.strip_prefix(SERVICE_PREFIX) else {
            continue;
        };
        let id = rest.split('/').next().unwrap_or("");
        if !id.is_empty() && !ids.iter().any(|existing| existing == id) {
            ids.push(id.to_string());
        }
    }
    let candidates: Vec<String> = ids
        .into_iter()
        .filter(|id| {
            let base = format!("{SERVICE_PREFIX}{id}");
            let device = dict_field_string(store_dict(snapshot, &format!("{base}/Interface")), "DeviceName");
            let v4 = dict_field_string(store_dict(snapshot, &format!("{base}/IPv4")), "InterfaceName");
            let v6 = dict_field_string(store_dict(snapshot, &format!("{base}/IPv6")), "InterfaceName");
            [device, v4, v6].iter().any(|name| name.as_deref() == Some(interface))
        })
        .collect();
    if candidates.len() == 1 {
        return candidates.into_iter().next();
    }
    let primary = dict_field_string(store_dict(snapshot, GLOBAL_IPV4_KEY), "PrimaryService");
    primary.filter(|id| candidates.iter().any(|candidate| candidate == id))
}

/// 正版 `networkConfiguration(interface:)` + `NetworkServiceResolver.resolve`。
unsafe fn service_configuration(interface: &str) -> Option<ServiceConfig> {
    let name = cf::cfstr("HaloStatusBar")?;
    let store = SCDynamicStoreCreate(ptr::null(), name, ptr::null(), ptr::null());
    cf::release(name);
    if store.is_null() {
        return None;
    }
    let result = unsafe {
        let Some(pattern) = cf::cfstr("State:/Network/.*") else {
            cf::release(store);
            return None;
        };
        let keys = SCDynamicStoreCopyKeyList(store, pattern);
        cf::release(pattern);
        let mut snapshot: Vec<(String, cf::CFTypeRef)> = Vec::new();
        if !keys.is_null() {
            for index in 0..cf::array_count(keys) {
                let Some(key) = cf::string(cf::array_at(keys, index)) else {
                    continue;
                };
                if key != GLOBAL_IPV4_KEY && !key.starts_with(SERVICE_PREFIX) {
                    continue;
                }
                let Some(cf_key) = cf::cfstr(&key) else {
                    continue;
                };
                let value = SCDynamicStoreCopyValue(store, cf_key);
                cf::release(cf_key);
                if !value.is_null() {
                    snapshot.push((key, value));
                }
            }
            cf::release(keys);
        }
        let resolved = select_service(&snapshot, interface).map(|id| {
            let base = format!("{SERVICE_PREFIX}{id}");
            let ipv4 = store_dict(&snapshot, &format!("{base}/IPv4"));
            let ipv6 = store_dict(&snapshot, &format!("{base}/IPv6"));
            let dns = store_dict(&snapshot, &format!("{base}/DNS"));
            ServiceConfig {
                ipv4_addresses: dict_field_strings(ipv4, "Addresses"),
                ipv6_addresses: dict_field_strings(ipv6, "Addresses"),
                router: dict_field_string(ipv4, "Router"),
                dns_servers: dict_field_strings(dns, "ServerAddresses"),
            }
        });
        for (_, value) in &snapshot {
            cf::release(*value);
        }
        resolved
    };
    cf::release(store);
    result
}

/// 正版 `makeDetails`：详情行的原料全部来自当前接口，噪声与国家码来自扫描结果里
/// 与当前 BSSID 相同的那条 AP。
unsafe fn connection_details(
    iface: *mut AnyObject,
    name_authorized: bool,
    actual_network: *mut AnyObject,
) -> WifiDetails {
    let mut details = WifiDetails::default();
    let ssid: *mut AnyObject = msg_send![iface, ssid];
    details.ssid = objc_optional_string(ssid).filter(|_| name_authorized);
    details.bssid = objc_optional_string(msg_send![iface, bssid]);
    let channel: *mut AnyObject = msg_send![iface, wlanChannel];
    if !channel.is_null() {
        let band: isize = msg_send![channel, channelBand];
        let width: isize = msg_send![channel, channelWidth];
        let number: isize = msg_send![channel, channelNumber];
        details.band = display_band(band);
        details.channel_width = display_channel_width(width);
        details.channel = Some(number as i32);
    }
    details.rssi = normalized_measurement(msg_send![iface, rssiValue]);
    details.phy_mode = display_phy(msg_send![iface, activePHYMode]);
    let rate: f64 = msg_send![iface, transmitRate];
    details.transmit_rate = rate.is_finite().then_some(rate).filter(|value| *value > 0.0);
    details.security = security_kind(msg_send![iface, security]);
    details.interface_name = objc_optional_string(msg_send![iface, interfaceName]);
    if !actual_network.is_null() {
        details.noise = normalized_measurement(msg_send![actual_network, noiseMeasurement]);
        details.country_code = objc_optional_string(msg_send![actual_network, countryCode]);
    }
    if let Some(config) = details
        .interface_name
        .as_deref()
        .and_then(|interface| service_configuration(interface))
    {
        details.ipv4_addresses = config.ipv4_addresses;
        details.ipv6_addresses = config.ipv6_addresses;
        details.router = config.router;
        details.dns_servers = config.dns_servers;
    }
    details
}

/// 正版 `NetworksetupWiFiKnownNetworkProvider`：`networksetup` 卡在病态网络栈上时
/// 会一直不返回，这里给它 3 秒期限，超时按「没有已知网络」降级。
fn preferred_networks(interface: &str) -> Vec<String> {
    let interface = interface.to_string();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let output = std::process::Command::new("/usr/sbin/networksetup")
            .args(["-listpreferredwirelessnetworks", &interface])
            .output()
            .ok()
            .filter(|out| out.status.success())
            .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
            .unwrap_or_default();
        let _ = tx.send(parse_preferred_output(&output));
    });
    rx.recv_timeout(std::time::Duration::from_secs(3))
        .unwrap_or_default()
}

/// 正版 `WiFiPreferredNetworkOutputParser`：丢掉「Preferred networks on en0:」首行，
/// 去掉行首制表符与行尾 `\r`，跳过空行。
fn parse_preferred_output(output: &str) -> Vec<String> {
    let mut lines: Vec<&str> = output.split('\n').collect();
    if lines.is_empty() {
        return Vec::new();
    }
    lines.remove(0);
    let mut seen = std::collections::HashSet::new();
    lines
        .into_iter()
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .map(|line| line.trim_start_matches('\t'))
        .filter(|line| !line.is_empty())
        .filter(|line| seen.insert((*line).to_string()))
        .map(String::from)
        .collect()
}

/// Wi-Fi 二级页一次完整读数（正版 `scanSynchronously`）。
///
/// 全信道扫描要一秒上下，绝不能跑在主线程：命令层用 `spawn_blocking` 调这里。
pub fn scan() -> WifiScan {
    let base = state();
    if !base.available {
        return WifiScan {
            name_access: base.name_access,
            ..WifiScan::unavailable()
        };
    }
    if !base.powered {
        return WifiScan {
            available: true,
            powered: false,
            name_access: base.name_access,
            list_state: "poweredOff".to_string(),
            ..WifiScan::unavailable()
        };
    }

    let _pool = unsafe { NSAutoreleasePool::new() };
    let Some(iface) = interface() else {
        return WifiScan {
            name_access: base.name_access,
            ..WifiScan::unavailable()
        };
    };
    let name_authorized = base.name_access == "authorized";
    let hotspot = base.link == "hotspot";

    unsafe {
        let mut error: *mut AnyObject = ptr::null_mut();
        let no_ssid: *mut AnyObject = ptr::null_mut();
        let networks: *mut AnyObject =
            msg_send![iface, scanForNetworksWithSSID: no_ssid, error: &mut error];
        let connected_bssid = objc_optional_string(msg_send![iface, bssid]);
        if networks.is_null() {
            return WifiScan {
                available: true,
                powered: true,
                name_access: base.name_access,
                list_state: "failed".to_string(),
                connected_bssid,
                details: connection_details(iface, name_authorized, ptr::null_mut()),
                hotspot,
                ..WifiScan::unavailable()
            };
        }

        let all: *mut AnyObject = msg_send![networks, allObjects];
        let count: usize = if all.is_null() {
            0
        } else {
            msg_send![all, count]
        };
        let mut candidates = Vec::with_capacity(count);
        let mut actual_network: *mut AnyObject = ptr::null_mut();
        for index in 0..count {
            let network: *mut AnyObject = msg_send![all, objectAtIndex: index];
            if network.is_null() {
                continue;
            }
            // 正版拿「与当前 BSSID 相同的那条 AP」补噪声与国家码，扫描结果里只认第一条。
            if actual_network.is_null() {
                if let (Some(left), Some(right)) = (
                    objc_optional_string(msg_send![network, bssid]),
                    connected_bssid.as_deref(),
                ) {
                    if left.eq_ignore_ascii_case(right) {
                        actual_network = network;
                    }
                }
            }
            if let Some(candidate) = project_candidate(network) {
                candidates.push(candidate);
            }
        }

        let known_ssids = objc_optional_string(msg_send![iface, interfaceName])
            .map(|name| preferred_networks(&name))
            .unwrap_or_default();
        let details = connection_details(iface, name_authorized, actual_network);

        WifiScan {
            available: true,
            powered: true,
            name_access: base.name_access,
            list_state: "ready".to_string(),
            candidates,
            connected_bssid,
            known_ssids,
            details,
            hotspot,
        }
    }
}

/// 打开/关闭 Wi-Fi。失败时返回 CoreWLAN 给出的本地化原因。
pub fn set_power(on: bool) -> Result<(), String> {
    let _pool = unsafe { NSAutoreleasePool::new() };
    let iface = interface().ok_or_else(|| "没有可用的 Wi-Fi 接口".to_string())?;
    unsafe {
        let mut error: *mut AnyObject = ptr::null_mut();
        let ok: Bool = msg_send![
            iface,
            setPower: if on { Bool::YES } else { Bool::NO },
            error: &mut error
        ];
        if ok.as_bool() {
            return Ok(());
        }
        let description = ns_string(error);
        Err(description.unwrap_or_else(|| "切换 Wi-Fi 失败".to_string()))
    }
}

/// 申请定位权限：macOS 14.4+ 只有授权后才返回 SSID。
///
/// 必须在主线程调用（调用方已切到主线程）。正版的行为照抄：
/// - 未决定且**从没弹过** → 弹系统授权框，返回 `requested`；
/// - 未决定但**已经弹过一次**（用户关掉/系统没递给你）→ 返回 `open_location_settings`，
///   由前端去开「定位服务」，不再无限重试；
/// - 已拒绝/受限 → `open_location_settings`；
/// - 已授权 → `not_needed`，前端顺带刷新一次读数（授权可能刚在系统设置里改完）。
pub fn request_name_permission() -> &'static str {
    match name_access() {
        NameAccess::Authorized => "not_needed",
        NameAccess::Denied | NameAccess::Restricted => "open_location_settings",
        NameAccess::NotDetermined => {
            if HAS_REQUESTED.swap(true, std::sync::atomic::Ordering::SeqCst) {
                return "open_location_settings";
            }
            if !corelocation_loaded() {
                return "not_needed";
            }
            unsafe {
                let Some(cls) = AnyClass::get("CLLocationManager") else {
                    return "not_needed";
                };
                let alloc: *mut AnyObject = msg_send![cls, alloc];
                if alloc.is_null() {
                    return "not_needed";
                }
                let manager: *mut AnyObject = msg_send![alloc, init];
                if manager.is_null() {
                    return "not_needed";
                }
                // 菜单栏 App 平时不是前台应用，TCC 不会把弹窗递给它：先激活再申请。
                if let Some(app_cls) = AnyClass::get("NSApplication") {
                    let app: *mut AnyObject = msg_send![app_cls, sharedApplication];
                    if !app.is_null() {
                        let _: () = msg_send![app, activateIgnoringOtherApps: Bool::YES];
                    }
                }
                let _: () = msg_send![manager, requestWhenInUseAuthorization];
                tracing::info!("Wi-Fi：已申请定位授权（弹窗由系统决定）");
                // `manager` 是 alloc/init 得到的 +1 引用，这里从不释放：
                // 它必须活到用户做出选择，否则授权框会被立即取消。泄漏量可忽略。
                let _ = manager;
            }
            "requested"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{choose_ipv4, classify_link};

    fn addrs(pairs: &[(&str, &str, u8)]) -> Vec<(String, String, u8)> {
        pairs
            .iter()
            .map(|(name, ip, prefix)| (name.to_string(), ip.to_string(), *prefix))
            .collect()
    }

    #[test]
    fn prefers_wifi_interface_when_connected() {
        let list = addrs(&[("en0", "192.168.1.9", 24), ("en2", "10.0.0.2", 24)]);
        let picked = choose_ipv4(Some("en0"), true, &list);
        assert_eq!(
            picked,
            Some(("en0".to_string(), "192.168.1.9".to_string(), 24))
        );
    }

    #[test]
    fn falls_back_to_first_ethernet_address() {
        let list = addrs(&[("utun3", "100.64.0.1", 32), ("en2", "10.0.0.2", 24)]);
        let picked = choose_ipv4(Some("en0"), false, &list);
        assert_eq!(
            picked,
            Some(("en2".to_string(), "10.0.0.2".to_string(), 24))
        );
    }

    #[test]
    fn returns_none_without_usable_interface() {
        let list = addrs(&[("utun3", "100.64.0.1", 32)]);
        assert_eq!(choose_ipv4(Some("en0"), true, &list), None);
    }

    #[test]
    fn apple_nat_ranges_are_the_hotspot_and_sharing_marks() {
        assert_eq!(classify_link(true, "172.20.10.2", 28, None), "hotspot");
        assert_eq!(classify_link(true, "192.168.2.3", 28, None), "sharing");
        assert_eq!(classify_link(true, "192.168.1.20", 24, None), "wifi");
    }

    #[test]
    fn same_address_with_a_wider_mask_is_ordinary_wifi() {
        // 公司网段撞上 172.20.10.0/24 不算热点：/28 是 Apple NAT 的硬特征。
        assert_eq!(classify_link(true, "172.20.10.2", 24, None), "wifi");
    }

    #[test]
    fn temporary_join_and_wired_are_separate_kinds() {
        assert_eq!(
            classify_link(true, "10.0.0.5", 24, Some("Temp-ABCD")),
            "temporary"
        );
        assert_eq!(classify_link(false, "10.0.0.5", 24, None), "ethernet");
    }
}
