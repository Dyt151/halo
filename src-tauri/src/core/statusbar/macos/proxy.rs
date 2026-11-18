//! 代理（VPN）读数与控制：驱动 Clash 系核心（本机是 FlClash）的本地控制口。
//!
//! 三条边界是有意的：
//! 1. 只连回环地址——控制口写成 `0.0.0.0` 时改打 `127.0.0.1`，写成别的机器就不用；
//! 2. 不写核心的配置文件、不重启核心，接管开关只动 macOS 系统代理；
//! 3. 核心没在跑时拒绝开启接管，免得把全部流量指向一个不存在的代理。
//!
//! 另外菜单栏图标吃的是缓存（[`bar_state`]），不现读：判定接管要起三个 `networksetup`
//! 子进程，而图标每两秒就重画一次。

use std::collections::BTreeMap;
use std::net::{TcpStream, ToSocketAddrs};
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU16, AtomicU8, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::Value;

use super::network;
use super::super::ProxyBar;
use super::{ProxyGroup, ProxyNode, ProxyOutcome, ProxyPage, ProxySpeed};

/// 配置文件没写控制口时试探的常用端口（mihomo / Clash Verge 默认值）。
const PROBE_PORTS: [u16; 3] = [9090, 9097, 9091];
/// 核心默认混合端口；配置文件读不到时用它。
const FALLBACK_MIXED_PORT: u16 = 7890;
const DEFAULT_TEST_URL: &str = "http://www.gstatic.com/generate_204";
/// 一轮延迟测试并发多少个节点：太多会把核心堆满待处理任务，太少要等半分钟。
const LATENCY_WORKERS: usize = 24;
/// 接管开启、而系统本身没有绕过列表时补的默认值：本机与内网走直连。
const DEFAULT_BYPASS: [&str; 6] = ["127.*", "localhost", "*.local", "10.*", "172.16.*", "192.168.*"];

/// 上一次网速采样的累计字节与时刻：速率要靠两次采样相减，所以得跨调用留着。
static SPEED_SAMPLE: Mutex<Option<Sample>> = Mutex::new(None);

/// 菜单栏图标吃的接管状态缓存（编码同 [`ProxyBar`] 的声明顺序）。
static PROXY_BAR: AtomicU8 = AtomicU8::new(0);
/// 最后一次读数看到的本机端口；观察线程只确认这一个端口通不通。
static PROXY_PORT: AtomicU16 = AtomicU16::new(0);

/// 图标提示用的接管状态：只读缓存，一次原子读，绝不起子进程。
pub fn bar_state() -> ProxyBar {
    match PROXY_BAR.load(Ordering::Relaxed) {
        1 => ProxyBar::Off,
        2 => ProxyBar::On,
        3 => ProxyBar::Stale,
        _ => ProxyBar::Unknown,
    }
}

fn remember(bar: ProxyBar, port: u16) {
    PROXY_BAR.store(bar as u8, Ordering::Relaxed);
    if port > 0 {
        PROXY_PORT.store(port, Ordering::Relaxed);
    }
}

/// 接管状态以哪个端口为准：接管开着就认系统代理实际指向的那个，否则用配置里的混合端口。
///
/// 用户流量只会去系统代理写着的端口。FlClash 换过混合端口而接管还挂在旧端口上时，
/// 新端口有人听、旧端口没人听——网页照样全打不开，所以不能拿配置里的端口下结论。
fn observed_port(takeover: &Takeover, mixed_port: u16) -> u16 {
    if takeover.active && takeover.port > 0 {
        takeover.port
    } else {
        mixed_port
    }
}

/// 观察线程的状态迁移：只在结论真的变了时才给出新值。
///
/// 端口从有人听到没人听 = 「失效」，反过来 = 恢复。两次都要求当前已经知道接管状态，
/// `Unknown` / `Off` 期间这条线什么都不做（那才是它几乎不花 CPU 的原因）。
fn next_bar(current: ProxyBar, listening: bool) -> Option<ProxyBar> {
    match (listening, current) {
        (false, ProxyBar::On) => Some(ProxyBar::Stale),
        (true, ProxyBar::Stale) => Some(ProxyBar::On),
        _ => None,
    }
}

/// 起一条只看端口的观察线程（重复调用只会起一条）。
///
/// 为什么值得为它留一条常驻线程：接管开着而核心退出，是这套设计里唯一真会把用户
/// 网断掉的组合——系统代理还指着 127.0.0.1 上一个没人监听的端口，所有网页打不开，
/// 而面板不打开就没人知道。这里因此不查系统代理（要起子进程），只做一次 250ms
/// 超时的 TCP 握手，并且只在「已接管/已失效」两种状态下才查。
///
/// 线程起来时先种一次缓存（[`seed_bar`]）：不种的话，开机后没打开过面板的那段时间
/// 里核心退出就没人报警。
pub fn spawn_watcher() {
    static STARTED: OnceLock<()> = OnceLock::new();
    STARTED.get_or_init(|| {
        std::thread::spawn(|| {
            // 等系统读数就位（网络服务名、Wi-Fi 网卡）再种，别在 setup 刚结束时抢跑。
            std::thread::sleep(Duration::from_secs(2));
            seed_bar();
            loop {
                std::thread::sleep(Duration::from_secs(15));
                let port = PROXY_PORT.load(Ordering::Relaxed);
                let current = bar_state();
                if port == 0 || !matches!(current, ProxyBar::On | ProxyBar::Stale) {
                    continue;
                }
                let listening = tcp_open(&format!("127.0.0.1:{port}"), Duration::from_millis(250));
                let Some(bar) = next_bar(current, listening) else {
                    continue;
                };
                if bar == ProxyBar::Stale {
                    tracing::warn!(port, "接管仍开着但端口已无人监听，菜单栏提示改为失效");
                } else {
                    tracing::info!(port, "端口恢复监听，接管状态回到已接管");
                }
                remember(bar, port);
                super::super::tray::request_refresh();
            }
        });
    });
}

/// 种一次接管缓存：三项系统代理 + 一次端口握手，只在启动时跑。
///
/// 认不出网络服务时什么都不写，保持 `Unknown`：这时读到「没接管」是猜的，
/// 而猜错的代价是把该报警的状态压掉了。
fn seed_bar() {
    let yaml = read_config();
    let interface = network::state().interface.unwrap_or_default();
    let service = active_service(&interface);
    if service.is_empty() {
        return;
    }
    let takeover = takeover_state(&service);
    let port = observed_port(&takeover, mixed_port_of(&yaml, None));
    let bar = if !takeover.active {
        ProxyBar::Off
    } else if tcp_open(&format!("127.0.0.1:{port}"), Duration::from_millis(250)) {
        ProxyBar::On
    } else {
        ProxyBar::Stale
    };
    remember(bar, port);
    super::super::tray::request_refresh();
}

#[derive(Debug, Clone, Copy)]
struct Sample {
    at: Instant,
    up: u64,
    down: u64,
}

/// FlClash 运行时写出的合并配置：控制口、混合端口、test-url 都在里面。
fn clash_config_path() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    Some(
        home.join("Library/Application Support/com.follow.clash")
            .join("config.yaml"),
    )
}

fn read_config() -> String {
    clash_config_path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .unwrap_or_default()
}

/// 取顶层（第 0 列）`key: value` 的值；缩进里的键属于子块，这里不收。
///
/// `Some("")` 表示「键在但值是空的」，要和「键不存在」（`None`）分开：
/// FlClash 关掉外部控制时写的正是 `external-controller: ""`。
fn top_level_scalar(yaml: &str, key: &str) -> Option<String> {
    let head = format!("{key}:");
    yaml.lines()
        .find_map(|line| line.strip_prefix(head.as_str()))
        .map(unquote)
}

/// 取 `block:` 子块里第一个 `key:` 的布尔值（读 `tun:` 下的 `enable:`）。
fn nested_bool(yaml: &str, block: &str, key: &str) -> Option<bool> {
    let head = format!("{block}:");
    let inner = format!("{key}:");
    let mut inside = false;
    for line in yaml.lines() {
        if !inside {
            inside = line.starts_with(head.as_str());
            continue;
        }
        if !line.starts_with(' ') && !line.starts_with('\t') {
            break;
        }
        if let Some(rest) = line.trim().strip_prefix(inner.as_str()) {
            return match unquote(rest).as_str() {
                "true" => Some(true),
                "false" => Some(false),
                _ => None,
            };
        }
    }
    None
}

fn unquote(raw: &str) -> String {
    let trimmed = raw.trim();
    if let Some(quote) = trimmed.chars().next().filter(|c| *c == '"' || *c == '\'') {
        let closing = trimmed[1..].find(quote).unwrap_or(trimmed.len() - 1);
        return trimmed[1..1 + closing].to_string();
    }
    let value = trimmed.split('#').next().unwrap_or("").trim();
    if value.is_empty() || value == "[]" || value == "null" {
        String::new()
    } else {
        value.to_string()
    }
}

/// 控制口地址：只接受回环。`0.0.0.0:9090`、`:9090`、`[::]:9090` 都归一到 `127.0.0.1:9090`，
/// 指向别的机器则返回 `None`——Halo 不替远端核心管代理。
fn loopback_controller(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let (host, port) = if let Some(rest) = trimmed.strip_prefix('[') {
        let closing = rest.find(']')?;
        (
            &rest[..closing],
            rest[closing + 1..].strip_prefix(':')?.parse::<u16>().ok()?,
        )
    } else {
        let (host, port) = trimmed.rsplit_once(':')?;
        (host, port.parse::<u16>().ok()?)
    };
    if port == 0 {
        return None;
    }
    let loopback = host.is_empty()
        || host == "0.0.0.0"
        || host == "::"
        || host == "::1"
        || host == "127.0.0.1"
        || host.eq_ignore_ascii_case("localhost");
    loopback.then(|| format!("127.0.0.1:{port}"))
}

/// 探一个端口是不是 Clash 控制口：光连得上不够，还得 `/version` 认账，
/// 否则混合端口（一个普通 HTTP 代理）会被误当成控制口。
///
/// 必须带上配置里的密钥：FlClash 开外部控制时会自动生成 secret，空密钥去问
/// 只会拿到 401，把真的控制口误判成「没开」。
fn reachable_controller(addr: &str, secret: &str) -> bool {
    if !tcp_open(addr, Duration::from_millis(250)) {
        return false;
    }
    matches!(
        http("GET", addr, secret, "/version", None, Duration::from_millis(900)),
        Ok((200, body)) if serde_json::from_str::<Value>(&body)
            .ok()
            .and_then(|value| value.get("version").and_then(Value::as_str).map(String::from))
            .is_some()
    )
}

fn tcp_open(addr: &str, timeout: Duration) -> bool {
    let Some(socket) = addr.to_socket_addrs().ok().and_then(|mut list| list.next()) else {
        return false;
    };
    TcpStream::connect_timeout(&socket, timeout).is_ok()
}

fn local_target(addr: &str) -> bool {
    addr.to_socket_addrs().is_ok()
}

/// 一次本地 HTTP 调用，借系统的 `curl`：省掉分块编码、连接池和 TLS 依赖，
/// 在 localhost 上代价就是起一个进程。返回 `(状态码, 响应体)`。
fn http(
    method: &str,
    addr: &str,
    secret: &str,
    path: &str,
    body: Option<&str>,
    timeout: Duration,
) -> Result<(u16, String), String> {
    if !local_target(addr) {
        return Err("控制口地址不合法".to_string());
    }
    let mut args: Vec<String> = vec![
        "-sS".to_string(),
        "--connect-timeout".to_string(),
        "1".to_string(),
        "-m".to_string(),
        timeout.as_millis().div_ceil(1000).max(1).to_string(),
        "-X".to_string(),
        method.to_string(),
        "-w".to_string(),
        "\n%{http_code}".to_string(),
    ];
    if !secret.is_empty() {
        args.push("-H".to_string());
        args.push(format!("Authorization: Bearer {secret}"));
    }
    if let Some(body) = body {
        args.push("-H".to_string());
        args.push("Content-Type: application/json".to_string());
        args.push("-d".to_string());
        args.push(body.to_string());
    }
    args.push(format!("http://{addr}{path}"));

    let output = Command::new("/usr/bin/curl")
        .args(&args)
        .output()
        .map_err(|e| format!("调用 curl 失败：{e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        return Err(short_message(&stderr));
    }
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    let (payload, code) = match text.rsplit_once('\n') {
        Some((payload, code)) => (payload, code),
        None => ("", text.as_str()),
    };
    Ok((code.trim().parse::<u16>().unwrap_or(0), payload.to_string()))
}

/// 把一大段命令输出压成一句能直接显示的话。
fn short_message(raw: &str) -> String {
    let line = raw
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with("**"))
        .unwrap_or("");
    let clipped: String = line.chars().take(72).collect();
    if clipped.is_empty() {
        "系统没有响应".to_string()
    } else {
        clipped
    }
}

/// URL 路径段编码：节点名里有中文、emoji 和空格，拼进 URL 前必须编码。
fn encode_segment(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len() * 3);
    for byte in raw.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char)
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

struct Controller {
    addr: String,
    secret: String,
}

/// 自动换节点模块用的本机核心快照：控制口地址 + 密钥 + 混合端口。
/// 仍然只认回环地址（[`find_controller`] 的边界不变）。
pub(crate) fn local_core() -> Option<(String, String, u16)> {
    let yaml = read_config();
    let controller = find_controller(&yaml)?;
    let port = mixed_port_of(&yaml, Some(&controller));
    Some((controller.addr, controller.secret, port))
}

/// 控制口选择：先用配置文件里写明的地址，再退回试探常用端口。
fn find_controller(yaml: &str) -> Option<Controller> {
    let secret = top_level_scalar(yaml, "secret").unwrap_or_default();
    let declared = top_level_scalar(yaml, "external-controller").unwrap_or_default();
    if let Some(addr) = loopback_controller(&declared) {
        if reachable_controller(&addr, &secret) {
            return Some(Controller { addr, secret });
        }
    }
    PROBE_PORTS
        .iter()
        .map(|port| format!("127.0.0.1:{port}"))
        .find(|addr| reachable_controller(addr, &secret))
        .map(|addr| Controller { addr, secret })
}

/// `/proxies` 解析：组按原顺序留下，节点延迟取自核心的 `history`（最近一次在前）。
///
/// 内置组（`DIRECT` / `REJECT` 等）和 `hidden` 组丢掉：正版菜单里没有它们。
fn parse_proxies(value: &Value) -> (Vec<ProxyGroup>, String) {
    let Some(entries) = value.get("proxies").and_then(Value::as_object) else {
        return (Vec::new(), String::new());
    };
    let kind_of = |name: &str| -> String {
        entries
            .get(name)
            .and_then(|entry| entry.get("type"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    };
    let mut test_url = String::new();
    let mut groups = Vec::new();
    for (name, entry) in entries {
        let kind = entry.get("type").and_then(Value::as_str).unwrap_or("");
        let members = match entry.get("all").and_then(Value::as_array) {
            Some(members) => members,
            None => continue,
        };
        if kind == "builtin" || entry.get("hidden").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        if test_url.is_empty() {
            if let Some(url) = entry.get("testUrl").and_then(Value::as_str) {
                if !url.is_empty() {
                    test_url = url.to_string();
                }
            }
        }
        let nodes = members
            .iter()
            .filter_map(Value::as_str)
            .map(|node| ProxyNode {
                delay: entries.get(node).and_then(entry_delay),
                name: node.to_string(),
                kind: kind_of(node),
            })
            .collect();
        groups.push(ProxyGroup {
            name: name.clone(),
            kind: kind.to_string(),
            current: entry
                .get("now")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .map(String::from),
            hidden: false,
            delay: entry_delay(entry),
            nodes,
        });
    }
    (groups, test_url)
}

fn entry_delay(entry: &Value) -> Option<u32> {
    let positive = |raw: u64| (raw > 0).then(|| raw as u32);
    if let Some(delay) = entry.get("delay").and_then(Value::as_u64).and_then(positive) {
        return Some(delay);
    }
    entry
        .get("history")
        .and_then(Value::as_array)
        .and_then(|history| history.first())
        .and_then(|item| item.get("delay").and_then(Value::as_u64))
        .and_then(positive)
}

/// `/connections` 的累计收发字节：mihomo 用 `upload-download`，写法不同的分支
/// 退回把每条连接的 `up`/`down` 加起来。
fn parse_totals(value: &Value) -> Option<(u64, u64)> {
    for key in ["upload-download", "up-download", "total-up-down"] {
        if let Some(pair) = value.get(key).and_then(Value::as_array) {
            let up = pair.first().and_then(Value::as_u64);
            let down = pair.get(1).and_then(Value::as_u64);
            if let (Some(up), Some(down)) = (up, down) {
                return Some((up, down));
            }
        }
    }
    let connections = value.get("connections").and_then(Value::as_array)?;
    let mut up = 0u64;
    let mut down = 0u64;
    for item in connections {
        up = up.saturating_add(item.get("up").and_then(Value::as_u64).unwrap_or(0));
        down = down.saturating_add(item.get("down").and_then(Value::as_u64).unwrap_or(0));
    }
    Some((up, down))
}

/// 两次累计字节换算成速率。时长为零或计数回绕（核心重启过）都按 0 处理。
fn rate_bps(previous: u64, current: u64, elapsed: Duration) -> u64 {
    let secs = elapsed.as_secs_f64();
    if secs <= 0.0 || current < previous {
        return 0;
    }
    ((current - previous) as f64 / secs).round().max(0.0) as u64
}

/// 记下这次采样，并给出它与上一次之间的速率。
fn speed_from_sample(now: (u64, u64), scope: &str) -> ProxySpeed {
    let mut guard = SPEED_SAMPLE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let previous = *guard;
    *guard = Some(Sample {
        at: Instant::now(),
        up: now.0,
        down: now.1,
    });
    let (up_bps, down_bps) = match previous {
        Some(sample) => {
            let elapsed = sample.at.elapsed();
            (
                rate_bps(sample.up, now.0, elapsed),
                rate_bps(sample.down, now.1, elapsed),
            )
        }
        // 第一次采样没有参照，速率留给下一次。
        None => (0, 0),
    };
    ProxySpeed {
        up_bps: Some(up_bps),
        down_bps: Some(down_bps),
        speed_scope: scope.to_string(),
    }
}

/// `netstat -ibn` 里某块网卡的累计收发字节：只认链路行，字段固定顺序
/// 为 `Name Mtu Network Address Ipkts Ierrs Ibytes Opkts Oerrs Obytes`。
///
/// 只取链路行还有个副作用要防：IP 行的 `Ierrs`/`Oerrs` 打印的是 `-`，任何按列
/// 序直接取数的写法都会在那几行上解析失败。
fn parse_netstat_counters(text: &str, interface: &str) -> Option<(u64, u64)> {
    text.lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>())
        .find(|fields| {
            fields.len() >= 10 && fields[0] == interface && is_link_row(fields[2])
        })
        .and_then(|fields| {
            let up = fields[6].parse::<u64>().ok()?;
            let down = fields[9].parse::<u64>().ok()?;
            Some((up, down))
        })
}

/// 链路行的 `Network` 列：macOS 打印 `<Link#12>`，别处也有不带尖括号的写法。
fn is_link_row(network: &str) -> bool {
    network
        .trim_matches(|c| c == '<' || c == '>')
        .to_ascii_lowercase()
        .starts_with("link#")
}

fn interface_counters(interface: &str) -> Option<(u64, u64)> {
    let output = Command::new("/usr/sbin/netstat")
        .args(["-ibn", "-I", interface])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_netstat_counters(&String::from_utf8_lossy(&output.stdout), interface)
}

/// 一条系统代理设置（`networksetup -getwebproxy` 那三行的结果）。
#[derive(Debug, Clone, Default)]
struct ProxyEntry {
    enabled: bool,
    server: String,
    port: u16,
}

fn parse_proxy_entry(text: &str) -> ProxyEntry {
    let mut entry = ProxyEntry::default();
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        match key.trim().to_ascii_lowercase().as_str() {
            "enabled" => entry.enabled = value.trim().eq_ignore_ascii_case("yes"),
            "server" => entry.server = value.trim().to_string(),
            "port" => entry.port = value.trim().parse().unwrap_or(0),
            _ => {}
        }
    }
    entry
}

/// 系统代理的三样：读用的动词、写端口用的动词、开关用的动词。
const PROTOCOLS: [(&str, &str, &str); 3] = [
    ("-getwebproxy", "-setwebproxy", "-setwebproxystate"),
    (
        "-getsecurewebproxy",
        "-setsecurewebproxy",
        "-setsecurewebproxystate",
    ),
    (
        "-getsocksfirewallproxy",
        "-setsocksfirewallproxy",
        "-setsocksfirewallproxystate",
    ),
];

fn run_networksetup(args: &[&str]) -> Result<String, String> {
    let output = Command::new("/usr/sbin/networksetup")
        .args(args)
        .output()
        .map_err(|e| format!("调用 networksetup 失败：{e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        return Err(short_message(&stderr));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn read_proxy_entry(get_verb: &str, service: &str) -> ProxyEntry {
    run_networksetup(&[get_verb, service])
        .map(|text| parse_proxy_entry(&text))
        .unwrap_or_default()
}

/// `-getproxybypassdomains` 在列表为空时打一句提示（`There aren't any bypass domains
/// set on Wi-Fi.`）；认出「真的没有」才补默认值，域名行本身不会带这个短语。
fn parse_bypass_domains(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| !line.starts_with("**"))
        .filter(|line| !line.to_ascii_lowercase().contains("bypass domains"))
        .map(String::from)
        .collect()
}

/// 把默认路由的网卡名（`en0`）换成 `networksetup` 认的服务名（`Wi-Fi`）。
fn parse_service_order(text: &str, interface: &str) -> Option<String> {
    let mut pending: Option<String> = None;
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("(Hardware Port:") {
            let service = pending.take();
            let device = trimmed
                .find("Device: ")
                .and_then(|at| trimmed.get(at + 8..))
                .map(|rest| rest.trim_end_matches(')').trim().to_string())
                .unwrap_or_default();
            if device == interface {
                return service;
            }
            continue;
        }
        if trimmed.is_empty() || trimmed.starts_with("An asterisk") {
            continue;
        }
        // 服务名单独成行，形如 `(1) Wi-Fi`；行尾的 `*` 表示这个服务被禁用。
        let name = trimmed
            .split_once(')')
            .map_or(trimmed, |(_, rest)| rest)
            .trim();
        if !name.is_empty() {
            pending = Some(name.trim_end_matches('*').trim_end_matches(' ').to_string());
        }
    }
    None
}

fn active_service(interface: &str) -> String {
    if interface.is_empty() {
        return String::new();
    }
    run_networksetup(&["-listnetworkserviceorder"])
        .ok()
        .and_then(|text| parse_service_order(&text, interface))
        .unwrap_or_default()
}

/// 系统代理三项的读数结果。
#[derive(Debug, Default)]
struct Takeover {
    /// 任意一项开着且指向本机。
    active: bool,
    web: bool,
    https: bool,
    socks: bool,
    /// 系统代理实际指向的本机端口；没接管时是 0。
    port: u16,
}

/// 开着且指向回环时给出它的端口，否则 `None`。
///
/// 只认回环：单位/家里那种指向 `10.x` 的代理不归 Halo 管，画成「已接管」再给个
/// 关闭按钮，等于让用户一键关掉别人的设置。
fn loopback_port(entry: ProxyEntry) -> Option<u16> {
    let local = matches!(entry.server.as_str(), "127.0.0.1" | "localhost" | "::1");
    (entry.enabled && local).then_some(entry.port)
}

/// 当前是否已经接管：三项里任意一项开着，且指向本机回环。
///
/// 故意不要求端口等于此刻读到的混合端口。核心换了端口或者直接退出了，系统代理还
/// 挂在旧端口上——那正是会把用户网络搞断的状态，必须认出来并交出关闭入口；否则
/// 他只能先把核心重新打开才能解除接管。
fn takeover_state(service: &str) -> Takeover {
    if service.is_empty() {
        return Takeover::default();
    }
    let mut state = Takeover::default();
    for (index, protocol) in PROTOCOLS.iter().enumerate() {
        let Some(port) = loopback_port(read_proxy_entry(protocol.0, service)) else {
            continue;
        };
        match index {
            0 => state.web = true,
            1 => state.https = true,
            _ => state.socks = true,
        }
        if state.port == 0 {
            state.port = port;
        }
    }
    state.active = state.web || state.https || state.socks;
    state
}

/// 开或关系统代理。动手前先记下三项原值，任一项失败或改完不符就整组回滚。
fn apply_takeover(service: &str, enabled: bool, mixed_port: u16) -> ProxyOutcome {
    if service.is_empty() {
        return ProxyOutcome::failed("认不出当前网络服务，接管开关先用不了");
    }
    let before: Vec<ProxyEntry> = PROTOCOLS
        .iter()
        .map(|protocol| read_proxy_entry(protocol.0, service))
        .collect();

    for (index, protocol) in PROTOCOLS.iter().enumerate() {
        if enabled && before[index].port != mixed_port {
            let port = mixed_port.to_string();
            if let Err(error) = run_networksetup(&[protocol.1, service, "127.0.0.1", &port]) {
                restore(service, &before, &PROTOCOLS[..index]);
                return ProxyOutcome::failed(error);
            }
        }
        if let Err(error) =
            run_networksetup(&[protocol.2, service, if enabled { "on" } else { "off" }])
        {
            restore(service, &before, &PROTOCOLS[..index]);
            return ProxyOutcome::failed(error);
        }
    }

    if enabled {
        ensure_bypass_defaults(service);
    }

    let wrong = PROTOCOLS.iter().any(|protocol| {
        read_proxy_entry(protocol.0, service).enabled != enabled
    });
    if wrong {
        restore(service, &before, &PROTOCOLS);
        tracing::warn!(service, enabled, "系统代理回读不符，已回滚到改动前");
        return ProxyOutcome::failed("系统代理没按要求改动，已恢复原状");
    }
    // 唯一会改系统状态的地方，留一行永久记录：出了问题能查到最后一次是谁把它改成这样的。
    tracing::info!(service, enabled, mixed_port, "系统代理已按要求改动并回读一致");
    // 自己动的手立刻反映到图标上，不等下一次读数——观察线程要靠这个端口才知道该不该查。
    remember(
        if enabled { ProxyBar::On } else { ProxyBar::Off },
        mixed_port,
    );
    ProxyOutcome::ok()
}

/// 回滚：只把已经动手的那几项按原值写回去。
fn restore(service: &str, before: &[ProxyEntry], touched: &[(&str, &str, &str)]) {
    for (index, protocol) in touched.iter().enumerate() {
        let Some(entry) = before.get(index) else {
            continue;
        };
        let _ = run_networksetup(&[
            protocol.2,
            service,
            if entry.enabled { "on" } else { "off" },
        ]);
    }
}

/// 系统没有绕过列表时补一份内网直连；已有列表一律不动。
fn ensure_bypass_defaults(service: &str) {
    let Ok(text) = run_networksetup(&["-getproxybypassdomains", service]) else {
        return;
    };
    if !parse_bypass_domains(&text).is_empty() {
        return;
    }
    let mut args: Vec<&str> = vec!["-setproxybypassdomains", service];
    args.extend(DEFAULT_BYPASS);
    let _ = run_networksetup(&args);
}

/// 混合端口：配置文件写了就用，其次问核心，都没有才用 7890。
fn mixed_port_of(yaml: &str, controller: Option<&Controller>) -> u16 {
    if let Some(port) = top_level_scalar(yaml, "mixed-port").and_then(|value| value.parse().ok()) {
        return port;
    }
    if let Some(controller) = controller {
        if let Ok((200, body)) = http(
            "GET",
            &controller.addr,
            &controller.secret,
            "/configs",
            None,
            Duration::from_millis(900),
        ) {
            if let Ok(value) = serde_json::from_str::<Value>(&body) {
                for key in ["mixed-port", "port"] {
                    if let Some(port) = value.get(key).and_then(Value::as_u64) {
                        if port > 0 && port <= u16::MAX as u64 {
                            return port as u16;
                        }
                    }
                }
            }
        }
    }
    FALLBACK_MIXED_PORT
}

/// 详情页一次完整读数。
///
/// 会起几个短命子进程（三项系统代理读数 + 控制口请求），只在页面打开和点刷新时
/// 调用；面板 150ms 的读数轮询不能走这里。
pub fn page() -> ProxyPage {
    let yaml = read_config();
    let controller = find_controller(&yaml);
    let mixed_port = mixed_port_of(&yaml, controller.as_ref());
    let core_running =
        tcp_open(&format!("127.0.0.1:{mixed_port}"), Duration::from_millis(250)) || controller.is_some();
    // 系统代理先读，再决定要不要走「什么都没找到」那条早退：核心没了但接管还挂着，
    // 恰恰是最需要页面能关掉它的时刻。
    let interface = network::state().interface.unwrap_or_default();
    let service = active_service(&interface);
    let takeover = takeover_state(&service);
    let port = observed_port(&takeover, mixed_port);
    // 接管指向的端口有没有人听：和混合端口是同一个时直接复用上面的握手结果，多花一次
    // 250ms 握手只发生在「配置换了端口、系统代理还指着旧的」这一种少见情况里。
    let port_alive = if port == mixed_port {
        core_running
    } else {
        tcp_open(&format!("127.0.0.1:{port}"), Duration::from_millis(250))
    };
    remember(
        if takeover.active {
            if port_alive {
                ProxyBar::On
            } else {
                ProxyBar::Stale
            }
        } else {
            ProxyBar::Off
        },
        port,
    );
    if !core_running && !takeover.active {
        return ProxyPage::unavailable();
    }

    let mut page = ProxyPage {
        available: true,
        source: if controller.is_some() {
            "clash"
        } else if core_running {
            "probe"
        } else {
            "none"
        }
        .to_string(),
        core_running,
        mixed_port,
        controller: controller.as_ref().map(|item| item.addr.clone()).unwrap_or_default(),
        takeover: takeover.active,
        takeover_web: takeover.web,
        takeover_https: takeover.https,
        takeover_socks: takeover.socks,
        proxy_port: takeover.port,
        takeover_alive: port_alive,
        service,
        tun_enabled: nested_bool(&yaml, "tun", "enable"),
        speed_scope: "interface".to_string(),
        test_url: top_level_scalar(&yaml, "test-url")
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| DEFAULT_TEST_URL.to_string()),
        ..ProxyPage::unavailable()
    };
    // 接管指向的端口没人听，是这套设计里唯一真会断网的状态，先说清楚再往下取数据。
    if takeover.active && !port_alive {
        if core_running {
            page.message = format!(
                "系统代理还指着 127.0.0.1:{port}，核心现在听的是 {mixed_port}：把「流量接管」关掉再打开，端口就改过来了。"
            );
        } else {
            page.message = format!(
                "系统代理还指着 127.0.0.1:{port}，但那个端口上已经没有核心了，网页会一直打不开。关掉「流量接管」就能恢复直连。"
            );
            return page;
        }
    }

    let Some(controller) = controller else {
        page.message = "核心在跑，但没开本地控制口：模式、节点和延迟改不了，只能开关接管。".to_string();
        if let Some(speed) = interface_speed(&interface) {
            page.up_bps = speed.up_bps;
            page.down_bps = speed.down_bps;
            page.speed_scope = speed.speed_scope;
        }
        return page;
    };

    if let Ok(value) = api_json("GET", &controller, "/proxies", Duration::from_millis(1200)) {
        let (groups, test_url) = parse_proxies(&value);
        page.groups = groups;
        if !test_url.is_empty() {
            page.test_url = test_url;
        }
    }
    if let Ok(value) = api_json("GET", &controller, "/configs", Duration::from_millis(1200)) {
        page.mode = value
            .get("mode")
            .and_then(Value::as_str)
            .map(|mode| mode.to_ascii_lowercase());
        if let Some(enable) = value
            .get("tun")
            .and_then(|tun| tun.get("enable"))
            .and_then(Value::as_bool)
        {
            page.tun_enabled = Some(enable);
        }
    }
    if let Some(speed) = proxy_speed(&controller) {
        page.up_bps = speed.up_bps;
        page.down_bps = speed.down_bps;
        page.speed_scope = speed.speed_scope;
    }
    page
}

fn api_json(method: &str, controller: &Controller, path: &str, timeout: Duration) -> Result<Value, String> {
    let (status, body) = http(method, &controller.addr, &controller.secret, path, None, timeout)?;
    if status != 200 {
        return Err(format!("控制口返回 {status}"));
    }
    serde_json::from_str::<Value>(&body).map_err(|_| "控制口返回的不是 JSON".to_string())
}

/// 控制口在线时的网速：`/connections` 的累计字节差分。
fn proxy_speed(controller: &Controller) -> Option<ProxySpeed> {
    let value = api_json(
        "GET",
        controller,
        "/connections?short=true",
        Duration::from_millis(1200),
    )
    .ok()?;
    let totals = parse_totals(&value)?;
    Some(speed_from_sample(totals, "proxy"))
}

/// 没有控制口时用本机网卡计数：口径是「这台机器的收发」，不是「走代理的收发」。
fn interface_speed(interface: &str) -> Option<ProxySpeed> {
    let totals = interface_counters(interface)?;
    Some(speed_from_sample(totals, "interface"))
}

/// 只刷新网速：面板可见时每两秒一次，比 [`page`] 便宜得多。
pub fn speed() -> ProxySpeed {
    if let Some(controller) = find_controller(&read_config()) {
        if let Some(sample) = proxy_speed(&controller) {
            return sample;
        }
    }
    let interface = network::state().interface.unwrap_or_default();
    if let Some(sample) = interface_speed(&interface) {
        return sample;
    }
    ProxySpeed {
        up_bps: None,
        down_bps: None,
        speed_scope: "interface".to_string(),
    }
}

/// 开关心跳：核心必须真的在监听，否则拒绝，免得把全部流量指向一个空端口。
pub fn set_takeover(enabled: bool) -> ProxyOutcome {
    let yaml = read_config();
    let interface = network::state().interface.unwrap_or_default();
    let service = active_service(&interface);
    let mixed_port = mixed_port_of(&yaml, find_controller(&yaml).as_ref());
    if enabled && !tcp_open(&format!("127.0.0.1:{mixed_port}"), Duration::from_millis(400)) {
        return ProxyOutcome::failed(format!(
            "本机 {mixed_port} 端口上没有代理核心，已保持直连"
        ));
    }
    apply_takeover(&service, enabled, mixed_port)
}

/// 模式切换：`rule` / `global` / `direct`。
pub fn set_mode(mode: &str) -> ProxyOutcome {
    let mode = mode.to_ascii_lowercase();
    if !matches!(mode.as_str(), "rule" | "global" | "direct") {
        return ProxyOutcome::failed("模式只有规则、全局、直连三档");
    }
    let Some(controller) = find_controller(&read_config()) else {
        return ProxyOutcome::failed("没开本地控制口，改不了模式");
    };
    let body = format!("{{\"mode\":\"{mode}\"}}");
    match http(
        "PATCH",
        &controller.addr,
        &controller.secret,
        "/configs",
        Some(&body),
        Duration::from_millis(2000),
    ) {
        Ok((200..=299, _)) => ProxyOutcome::ok(),
        Ok((code, _)) => ProxyOutcome::failed(format!("模式没有改成（{code}）")),
        Err(error) => ProxyOutcome::failed(error),
    }
}

/// 选节点：`PUT /proxies/<组>`，名字要编码。
pub fn select_node(group: &str, node: &str) -> ProxyOutcome {
    if group.is_empty() || node.is_empty() {
        return ProxyOutcome::failed("组名或节点名是空的");
    }
    let Some(controller) = find_controller(&read_config()) else {
        return ProxyOutcome::failed("没开本地控制口，选不了节点");
    };
    let path = format!("/proxies/{}", encode_segment(group));
    let body = format!("{{\"name\":{}}}", json_string(node));
    match http(
        "PUT",
        &controller.addr,
        &controller.secret,
        &path,
        Some(&body),
        Duration::from_millis(3000),
    ) {
        Ok((200..=299, _)) => ProxyOutcome::ok(),
        Ok((code, _)) => ProxyOutcome::failed(format!("这个节点切不过去（{code}）")),
        Err(error) => ProxyOutcome::failed(error),
    }
}

/// 手写 JSON 字符串转义：节点名里有引号、反斜杠和控制字符，`Value` 序列化也行，
/// 但这里只需要一个字段，直写更清楚。
fn json_string(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len() + 2);
    out.push('"');
    for ch in raw.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other if (other as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", other as u32)),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// 测一个组里所有节点的延迟：每个节点单独问一次 `GET /proxies/<节点>/delay`。
///
/// 分批并发，一批最多 [`LATENCY_WORKERS`] 个，单请求 5 秒；50 个节点最坏十几秒，
/// 前端在这段时间里显示转圈。
pub fn latency(group: &str) -> Vec<ProxyNode> {
    let yaml = read_config();
    let Some(controller) = find_controller(&yaml) else {
        return Vec::new();
    };
    let test_url = top_level_scalar(&yaml, "test-url")
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_TEST_URL.to_string());
    let path = format!("/proxies/{}", encode_segment(group));
    let Ok(value) = api_json("GET", &controller, &path, Duration::from_millis(1200)) else {
        return Vec::new();
    };
    let Some(members) = value.get("all").and_then(Value::as_array) else {
        return Vec::new();
    };
    let names: Vec<String> = members.iter().filter_map(Value::as_str).map(String::from).collect();

    let mut results = BTreeMap::new();
    for chunk in names.chunks(LATENCY_WORKERS) {
        std::thread::scope(|scope| {
            let mut handles = Vec::with_capacity(chunk.len());
            for name in chunk {
                let controller = &controller;
                let test_url = test_url.as_str();
                handles.push(scope.spawn(move || {
                    let path = format!(
                        "/proxies/{}/delay?url={}&timeout=5000",
                        encode_segment(name),
                        encode_segment(test_url)
                    );
                    let delay = api_json("GET", controller, &path, Duration::from_millis(5200))
                        .ok()
                        .and_then(|value| value.get("delay").and_then(Value::as_u64))
                        .filter(|delay| *delay > 0)
                        .map(|delay| delay as u32);
                    (name.clone(), delay)
                }));
            }
            for handle in handles {
                if let Ok((name, delay)) = handle.join() {
                    results.insert(name, delay);
                }
            }
        });
    }

    names
        .iter()
        .map(|name| ProxyNode {
            delay: results.get(name).copied().flatten(),
            name: name.clone(),
            kind: String::new(),
        })
        .collect()
}

/// 把 FlClash 主界面带到前台；没安装或起不来时返回原因，前端直接显示。
///
/// 用 bundle id 而不是路径：用户的第三方 App 装在 `/Applications/开源软件/`，
/// 写死路径会跟着目录习惯一起烂掉。
pub fn open_app() -> ProxyOutcome {
    match Command::new("/usr/bin/open").args(["-b", "com.follow.clash"]).output() {
        Ok(output) if output.status.success() => ProxyOutcome::ok(),
        Ok(output) => ProxyOutcome::failed(short_message(&String::from_utf8_lossy(&output.stderr))),
        Err(error) => ProxyOutcome::failed(format!("没能打开 FlClash：{error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        entry_delay, loopback_controller, loopback_port, next_bar, observed_port, parse_bypass_domains,
        parse_netstat_counters, parse_proxies, parse_proxy_entry, parse_service_order, parse_totals,
        rate_bps, top_level_scalar, unquote, ProxyBar, ProxyEntry, Takeover,
    };
    use serde_json::json;

    const SAMPLE: &str = r#"port: 0
socks-port: 0
mixed-port: 7890
allow-lan: false
mode: "rule"
external-controller: ""
external-controller-unix: "/tmp/x.sock"
secret: ""
test-url: "http://www.gstatic.com/generate_204"
tun:
  enable: false
  device: "FlClash"
dns:
  enable: true
"#;

    #[test]
    fn top_level_keys_ignore_indented_ones() {
        assert_eq!(top_level_scalar(SAMPLE, "mixed-port").as_deref(), Some("7890"));
        assert_eq!(top_level_scalar(SAMPLE, "mode").as_deref(), Some("rule"));
        // 关掉的外部控制：键在、值为空，和「没有这个键」不是一回事。
        assert_eq!(top_level_scalar(SAMPLE, "external-controller").as_deref(), Some(""));
        assert_eq!(top_level_scalar(SAMPLE, "external-controller-unix").as_deref(), Some("/tmp/x.sock"));
        assert_eq!(top_level_scalar(SAMPLE, "no-such-key"), None);
        assert_eq!(top_level_scalar(SAMPLE, "enable"), None);
    }

    #[test]
    fn super_block_reads_only_its_own_child() {
        assert_eq!(super::nested_bool(SAMPLE, "tun", "enable"), Some(false));
        assert_eq!(super::nested_bool(SAMPLE, "dns", "enable"), Some(true));
        assert_eq!(super::nested_bool(SAMPLE, "nope", "enable"), None);
    }

    #[test]
    fn controller_addresses_are_only_accepted_on_loopback() {
        assert_eq!(loopback_controller("127.0.0.1:9097").as_deref(), Some("127.0.0.1:9097"));
        assert_eq!(loopback_controller("localhost:9090").as_deref(), Some("127.0.0.1:9090"));
        // 全接口写法归一到回环；端口保留。
        assert_eq!(loopback_controller("0.0.0.0:9090").as_deref(), Some("127.0.0.1:9090"));
        assert_eq!(loopback_controller("[::]:9097").as_deref(), Some("127.0.0.1:9097"));
        // 别的机器一律不用：Halo 不替远端核心管代理。
        assert_eq!(loopback_controller("192.168.9.3:9090"), None);
        assert_eq!(loopback_controller(""), None);
        assert_eq!(loopback_controller("127.0.0.1:0"), None);
    }

    #[test]
    fn unquote_handles_yaml_shapes() {
        assert_eq!(unquote("  \"rule\"  "), "rule");
        assert_eq!(unquote("7890 # 端口"), "7890");
        assert_eq!(unquote("[]"), "");
        assert_eq!(unquote("null"), "");
    }

    #[test]
    fn proxy_entry_reads_all_three_fields() {
        let entry = parse_proxy_entry("Enabled: Yes\nServer: 127.0.0.1\nPort: 7890\nAuthenticated Proxy Enabled: 0\n");
        assert!(entry.enabled);
        assert_eq!(entry.server, "127.0.0.1");
        assert_eq!(entry.port, 7890);
        let off = parse_proxy_entry("Enabled: No\nServer: 10.0.0.1\nPort: 0\n");
        assert!(!off.enabled);
        assert_eq!(off.port, 0);
    }

    #[test]
    fn any_loopback_proxy_is_a_takeover_regardless_of_port() {
        let entry = |server: &str, port: u16, enabled: bool| ProxyEntry {
            enabled,
            server: server.to_string(),
            port,
        };
        // 端口和当前配置不一致也必须认出来：核心换了端口或已经退出时，
        // 系统代理正挂在旧端口上，那正是需要页面给出关闭入口的状态。
        assert_eq!(loopback_port(entry("127.0.0.1", 7891, true)), Some(7891));
        assert_eq!(loopback_port(entry("localhost", 7890, true)), Some(7890));
        assert_eq!(loopback_port(entry("::1", 7890, true)), Some(7890));
        // 指向别处的代理不归 Halo 管，画成「已接管」再给关闭按钮太危险。
        assert_eq!(loopback_port(entry("10.0.0.9", 3128, true)), None);
        assert_eq!(loopback_port(entry("127.0.0.1", 7890, false)), None);
    }

    #[test]
    fn takeover_port_used_for_the_bar() {
        let takeover = Takeover {
            active: true,
            port: 7890,
            ..Default::default()
        };
        // 核心换端口后，判断和报警都跟着系统代理实际指向的端口走。
        assert_eq!(observed_port(&takeover, 7891), 7890);
        let idle = Takeover::default();
        // 没接管时退回配置里的端口：这时没有「实际指向」可看。
        assert_eq!(observed_port(&idle, 7891), 7891);
    }

    #[test]
    fn controller_validation_sends_the_secret() {
        use std::io::{Read, Write};
        // 起一个假控制口：带对密钥才回 200，否则 401——正是 FlClash 的行为。
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        let server = std::thread::spawn(move || {
            // 每次校验先有一次只连不说话的 tcp_open，再有一次 curl，共四轮。
            for _ in 0..4 {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(2)))
                    .unwrap();
                let mut buf = [0u8; 4096];
                let n = stream.read(&mut buf).unwrap_or(0);
                if n == 0 {
                    continue;
                }
                let request = String::from_utf8_lossy(&buf[..n]).to_string();
                let (status, body) = if request.contains("Authorization: Bearer s3cret") {
                    ("200 OK", "{\"version\":\"test\"}")
                } else {
                    ("401 Unauthorized", "{\"message\":\"Unauthorized\"}")
                };
                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        assert!(super::reachable_controller(&addr, "s3cret"));
        assert!(!super::reachable_controller(&addr, ""));
        server.join().unwrap();
    }

    #[test]
    fn watcher_only_flips_between_on_and_stale() {
        // 端口没人听了 → 失效；端口回来了 → 恢复。这两条是唯一会改图标提示的迁移。
        assert_eq!(next_bar(ProxyBar::On, false), Some(ProxyBar::Stale));
        assert_eq!(next_bar(ProxyBar::Stale, true), Some(ProxyBar::On));
        // 状态没变、以及压根没接管时都不动：观察线程因此几乎不产生任何副作用。
        assert_eq!(next_bar(ProxyBar::On, true), None);
        assert_eq!(next_bar(ProxyBar::Stale, false), None);
        assert_eq!(next_bar(ProxyBar::Off, false), None);
        assert_eq!(next_bar(ProxyBar::Unknown, false), None);
    }

    #[test]
    fn bypass_marker_is_not_a_domain() {
        assert!(parse_bypass_domains("There aren't any bypass domains set on Wi-Fi.").is_empty());
        assert_eq!(
            parse_bypass_domains("127.*\nlocalhost\n*.local\n"),
            vec!["127.*".to_string(), "localhost".to_string(), "*.local".to_string()]
        );
    }

    #[test]
    fn service_name_follows_the_default_route_device() {
        let text = "An asterisk (*) denotes that a network service is disabled.\n(1) Wi-Fi\n(Hardware Port: Wi-Fi, Device: en0)\n\n(2) Thunderbolt Bridge\n(Hardware Port: Thunderbolt Bridge, Device: bridge0)\n";
        assert_eq!(parse_service_order(text, "en0").as_deref(), Some("Wi-Fi"));
        assert_eq!(parse_service_order(text, "bridge0").as_deref(), Some("Thunderbolt Bridge"));
        assert_eq!(parse_service_order(text, "utun9"), None);
    }

    #[test]
    fn interface_counters_take_the_link_line() {
        let text = "Name       Mtu   Network       Address            Ipkts Ierrs    Ibytes    Opkts Oerrs    Obytes  Coll\nen0        1500  <Link#8>        aa:bb:cc:dd:ee:ff  12345     0  987654321  54321     0  123456789       0\nen0        1500  192.168.9.8     fe80::1      12345     0  111111111  54321     0  222222222       0\ndlo0       16384 <Link#1>                                  1     0        10      1     0        10\n";
        assert_eq!(parse_netstat_counters(text, "en0"), Some((987654321, 123456789)));
        assert_eq!(parse_netstat_counters(text, "utun5"), None);
    }

    #[test]
    fn totals_prefers_the_mihomo_field() {
        assert_eq!(parse_totals(&json!({"upload-download": [7, 3]})), Some((7, 3)));
        assert_eq!(
            parse_totals(&json!({"connections": [{"up": 4, "down": 2}, {"up": 1, "down": 5}]})),
            Some((5, 7))
        );
        assert_eq!(parse_totals(&json!({"connections": []})), Some((0, 0)));
        assert_eq!(parse_totals(&json!({})), None);
    }

    #[test]
    fn rate_survives_a_core_restart() {
        let second = std::time::Duration::from_secs(2);
        assert_eq!(rate_bps(1000, 3000, second), 1000);
        // 计数回绕（核心重启过）不能算出一个巨大的速率。
        assert_eq!(rate_bps(5000, 100, second), 0);
        assert_eq!(rate_bps(100, 200, std::time::Duration::ZERO), 0);
    }

    #[test]
    fn groups_keep_order_and_node_delays() {
        let value = json!({"proxies": {
            "DIRECT": {"type": "builtin", "all": [], "hidden": true},
            "良心云": {"type": "select", "now": "自动选择", "hidden": false, "testUrl": "",
                "all": ["自动选择", "香港01"], "history": []},
            "自动选择": {"type": "url-test", "now": "香港01", "hidden": false,
                "testUrl": "http://www.gstatic.com/generate_204", "all": ["香港01"],
                "history": [{"delay": 182}, {"delay": 200}]},
            "香港01": {"type": "ss", "history": [{"delay": 88}]}
        }});
        let (groups, test_url) = parse_proxies(&value);
        assert_eq!(test_url, "http://www.gstatic.com/generate_204");
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].current.as_deref(), Some("自动选择"));
        assert_eq!(groups[0].nodes[1].delay, Some(88));
        assert_eq!(groups[1].delay, Some(182));
    }

    #[test]
    fn zero_delay_means_timeout() {
        assert_eq!(entry_delay(&json!({"history": [{"delay": 0}]})), None);
        assert_eq!(entry_delay(&json!({"delay": 0, "history": [{"delay": 90}]})), Some(90));
    }

    #[test]
    fn node_names_are_escaped_for_url_and_json() {
        assert_eq!(super::encode_segment("香港 01|BGP"), "%E9%A6%99%E6%B8%AF%2001%7CBGP");
        assert_eq!(super::encode_segment("ascii-01.~"), "ascii-01.~");
        assert_eq!(super::json_string("a\"b\\c"), "\"a\\\"b\\\\c\"");
    }

    /// 只读取本机真实状态，用来远程核对组合路径：`cargo test --lib -- --ignored --nocapture`。
    /// 断言只留环境无关的不变量，实际数值看打印。
    #[test]
    #[ignore = "会读真实配置与系统代理状态，不做无人值守断言"]
    fn live_page_and_speed_are_readable() {
        let page = super::page();
        println!("live page: {page:?}");
        let first = super::speed();
        std::thread::sleep(std::time::Duration::from_millis(1_200));
        let second = super::speed();
        println!("live speed: {first:?} -> {second:?}");
        assert!(page.mixed_port > 0, "至少要有一个候选端口");
        assert!(matches!(page.source.as_str(), "clash" | "probe" | "none"));
        assert!(matches!(second.speed_scope.as_str(), "proxy" | "interface"));
    }
}
