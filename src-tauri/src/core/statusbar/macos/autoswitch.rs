//! 自动换节点：巡检当前出口的真实下载速度，连续两次不达标就实测候选并切换，
//! 全程目标 2 分钟内完成。两个后端同一套流程：
//!
//! - 连着 M8 的 Wi-Fi（默认网关 192.168.9.1）→ 管路由器 OpenClash 的「良心云」组；
//!   候选测速走专门的「测速」组 + speed.cloudflare.com 规则，不动用户出口 IP。
//! - 否则 → 管本机 FlClash 的「良心云」组；候选测速短暂切换主组（几百毫秒级），
//!   测完立刻定到最优节点。
//!
//! 边界与 [`super::proxy`] 一致：FlClash 只认回环控制口；不写核心配置文件、不重启核心。
//! 防封号四道闸：每日上限、同地区优先、离开 10 分钟内不回头、救援也要复验。

use std::collections::BTreeMap;
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::secrets;
use crate::storage;

use super::super::{AutoSwitchConfig, AutoSwitchState, ProxyOutcome};
use super::proxy;

const ROUTER_API: &str = "192.168.9.1:9090";
const ROUTER_GATEWAY: &str = "192.168.9.1";
const SECRET_ACCOUNT: &str = "autoswitch-router-secret";
const TEST_URL: &str = "https://speed.cloudflare.com/__down";
const DELAY_URL: &str = "http://www.gstatic.com/generate_204";
/// 巡检下载量。太小（旧的 128KB）会被 TLS 握手 + 代理首字节延迟主导：一条真实
/// ~1.3MB/s 的快链路只测出 ~300KB/s，正好卡在阈值上下反复误判「不达标」，把每日
/// 切换额度空烧光。256KB 能测到接近真实的稳定吞吐（同一链路 ~500KB/s），阈值 300
/// 不再被快链路误触；30s 一次约 720MB/天。
const PROBE_BYTES: u32 = 262_144;
/// 候选实测下载量。
const CANDIDATE_BYTES: u32 = 1_048_576;
const PROBE_TIMEOUT_SECS: u64 = 10;
const CANDIDATE_TIMEOUT_SECS: u64 = 8;
const DELAY_WORKERS: usize = 16;
/// 一次不达标后多久复测确认（两次不达标才救援，避免单次抖动误切）。
const CONFIRM_SECS: u64 = 10;
/// 离开某节点后多久内不回头（防横跳）；救援时若没有别的活节点则放宽。
const NO_RETURN_SECS: u64 = 600;
const LOG_KEEP: usize = 20;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct Persisted {
    day: String,
    switches_today: u32,
    last_switch_at: Option<u64>,
    last_switch_from: String,
    last_switch_to: String,
    /// 节点名 → 离开它的 Unix 秒。
    recent_left: BTreeMap<String, u64>,
}

#[derive(Debug, Default)]
struct Runtime {
    backend: String,
    current_node: String,
    last_speed_kbps: Option<u32>,
    last_probe_at: Option<u64>,
    consecutive_bad: u32,
    status: String,
    next_probe_at: u64,
    persisted: Persisted,
    log: Vec<String>,
}

fn runtime() -> &'static Mutex<Runtime> {
    static RT: OnceLock<Mutex<Runtime>> = OnceLock::new();
    RT.get_or_init(|| {
        let mut rt = Runtime::default();
        rt.persisted = read_persisted();
        Mutex::new(rt)
    })
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn today() -> String {
    // 本地日期：用 `date` 子进程太重，这里用 UTC+8 近似（用户在中国）。
    let secs = unix_now() + 8 * 3600;
    let days = secs / 86400;
    let (y, m, d) = civil_from_days(days as i64);
    format!("{y:04}-{m:02}-{d:02}")
}

/// 天数 → 年月日（Howard Hinnant 的 civil_from_days 算法）。
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

// ---------- 配置与持久化 ----------

fn config_path() -> std::path::PathBuf {
    storage::data_dir().join("autoswitch.json")
}

fn persisted_path() -> std::path::PathBuf {
    storage::data_dir().join("autoswitch_state.json")
}

fn log_path() -> std::path::PathBuf {
    storage::logs_dir().join("autoswitch.log")
}

fn read_config_file() -> AutoSwitchConfig {
    let mut cfg: AutoSwitchConfig = std::fs::read_to_string(config_path())
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();
    cfg.has_router_secret = secrets::load(SECRET_ACCOUNT).is_some();
    cfg
}

fn write_config_file(cfg: &AutoSwitchConfig) -> Result<(), String> {
    let text = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    std::fs::write(config_path(), text).map_err(|e| format!("写配置失败：{e}"))
}

fn read_persisted() -> Persisted {
    std::fs::read_to_string(persisted_path())
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn write_persisted(p: &Persisted) {
    if let Ok(text) = serde_json::to_string_pretty(p) {
        let _ = std::fs::write(persisted_path(), text);
    }
}

fn log_line(msg: &str) {
    let line = format!("[{}] {msg}", today_time());
    tracing::info!(target: "autoswitch", "{msg}");
    let mut rt = runtime().lock().unwrap();
    rt.log.insert(0, line.clone());
    rt.log.truncate(LOG_KEEP);
    drop(rt);
    let path = log_path();
    if let Ok(meta) = std::fs::metadata(&path) {
        if meta.len() > 1_000_000 {
            let _ = std::fs::write(&path, "");
        }
    }
    use std::io::Write;
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(file, "{line}");
    }
}

fn today_time() -> String {
    let secs = unix_now() + 8 * 3600;
    let rem = secs % 86400;
    format!(
        "{} {:02}:{:02}:{:02}",
        today(),
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

// ---------- 对外接口 ----------

pub fn config() -> AutoSwitchConfig {
    read_config_file()
}

pub fn set_config(mut cfg: AutoSwitchConfig) -> ProxyOutcome {
    cfg.min_speed_kbps = cfg.min_speed_kbps.clamp(50, 5000);
    cfg.probe_interval_secs = cfg.probe_interval_secs.clamp(10, 300);
    cfg.daily_cap = cfg.daily_cap.clamp(1, 50);
    if cfg.router_group.trim().is_empty() || cfg.local_group.trim().is_empty() {
        return ProxyOutcome::failed("组名不能为空");
    }
    cfg.has_router_secret = secrets::load(SECRET_ACCOUNT).is_some();
    match write_config_file(&cfg) {
        Ok(()) => {
            log_line(&format!(
                "配置已保存：开关={} 阈值={}KB/s 巡检={}s 上限={}",
                cfg.enabled, cfg.min_speed_kbps, cfg.probe_interval_secs, cfg.daily_cap
            ));
            ProxyOutcome::ok()
        }
        Err(e) => ProxyOutcome::failed(e),
    }
}

pub fn set_router_secret(secret: String) -> ProxyOutcome {
    let secret = secret.trim();
    if secret.is_empty() {
        return ProxyOutcome::failed("密钥是空的");
    }
    match secrets::store(SECRET_ACCOUNT, secret) {
        Ok(()) => {
            log_line("路由器 API 密钥已存入钥匙串");
            ProxyOutcome::ok()
        }
        Err(e) => ProxyOutcome::failed(format!("存钥匙串失败：{e}")),
    }
}

pub fn state() -> AutoSwitchState {
    let cfg = read_config_file();
    let rt = runtime().lock().unwrap();
    let (backend, label) = match rt.backend.as_str() {
        "router" => ("router", "路由器 OpenClash（M8）"),
        "local" => ("local", "本机 FlClash"),
        _ => ("none", "未连接"),
    };
    AutoSwitchState {
        config: cfg,
        backend: backend.to_string(),
        backend_label: label.to_string(),
        current_node: rt.current_node.clone(),
        last_speed_kbps: rt.last_speed_kbps,
        last_probe_at: rt.last_probe_at,
        last_switch_at: rt.persisted.last_switch_at,
        last_switch_from: rt.persisted.last_switch_from.clone(),
        last_switch_to: rt.persisted.last_switch_to.clone(),
        switches_today: rt.persisted.switches_today,
        consecutive_bad: rt.consecutive_bad,
        status: rt.status.clone(),
        log: rt.log.clone(),
    }
}

/// 立即跑一轮巡检（前端「立即测速」按钮与验收用）。
pub fn check_now() -> AutoSwitchState {
    let cfg = read_config_file();
    run_cycle(&cfg, true);
    state()
}

pub fn spawn_worker() {
    static STARTED: OnceLock<()> = OnceLock::new();
    STARTED.get_or_init(|| {
        std::thread::spawn(|| {
            std::thread::sleep(Duration::from_secs(5));
            log_line("自动换节点巡检线程已启动");
            loop {
                std::thread::sleep(Duration::from_secs(1));
                let cfg = read_config_file();
                if !cfg.enabled {
                    continue;
                }
                let now = unix_now();
                let due = {
                    let rt = runtime().lock().unwrap();
                    now >= rt.next_probe_at
                };
                if !due {
                    continue;
                }
                {
                    let mut rt = runtime().lock().unwrap();
                    rt.next_probe_at = now + u64::from(cfg.probe_interval_secs);
                }
                run_cycle(&cfg, false);
            }
        });
    });
}

// ---------- HTTP（借系统 curl，与 proxy.rs 同款） ----------

fn http(
    method: &str,
    addr: &str,
    secret: &str,
    path: &str,
    body: Option<&str>,
    timeout: Duration,
) -> Result<(u16, String), String> {
    let mut args: Vec<String> = vec![
        "-sS".into(),
        "--connect-timeout".into(),
        "2".into(),
        "-m".into(),
        timeout.as_millis().div_ceil(1000).max(1).to_string(),
        "-X".into(),
        method.into(),
        "-w".into(),
        "\n%{http_code}".into(),
    ];
    if !secret.is_empty() {
        args.push("-H".into());
        args.push(format!("Authorization: Bearer {secret}"));
    }
    if let Some(body) = body {
        args.push("-H".into());
        args.push("Content-Type: application/json".into());
        args.push("-d".into());
        args.push(body.to_string());
    }
    args.push(format!("http://{addr}{path}"));
    let output = Command::new("/usr/bin/curl")
        .args(&args)
        .output()
        .map_err(|e| format!("调用 curl 失败：{e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    let (payload, code) = match text.rsplit_once('\n') {
        Some((p, c)) => (p, c),
        None => ("", text.as_str()),
    };
    Ok((code.trim().parse::<u16>().unwrap_or(0), payload.to_string()))
}

fn api_json(
    method: &str,
    addr: &str,
    secret: &str,
    path: &str,
    body: Option<&str>,
) -> Result<Value, String> {
    let (status, text) = http(method, addr, secret, path, body, Duration::from_secs(6))?;
    if !(200..300).contains(&status) {
        return Err(format!("控制口返回 {status}"));
    }
    if text.trim().is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_str(&text).map_err(|_| "控制口返回的不是 JSON".to_string())
}

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

fn json_string(raw: &str) -> String {
    Value::String(raw.to_string()).to_string()
}

// ---------- 后端探测 ----------

struct Backend {
    kind: &'static str,
    api: String,
    secret: String,
    /// 测速用的 HTTP 代理地址（host:port）。
    proxy: String,
    /// 承载主流量的组。
    group: String,
    /// 候选测速走的组：路由器是「测速」，本机就是主组本身。
    test_group: String,
}

fn default_gateway() -> Option<String> {
    let output = Command::new("/sbin/route").args(["-n", "get", "default"]).output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines()
        .find_map(|line| line.trim().strip_prefix("gateway:").map(|g| g.trim().to_string()))
}

fn detect_backend(cfg: &AutoSwitchConfig) -> Result<Backend, String> {
    if default_gateway().as_deref() == Some(ROUTER_GATEWAY) {
        let secret = secrets::load(SECRET_ACCOUNT)
            .ok_or("连着 M8，但还没填路由器 API 密钥（在卡片里填一次）")?;
        let version = http("GET", ROUTER_API, &secret, "/version", None, Duration::from_secs(3));
        if !matches!(version, Ok((200, _))) {
            return Err("路由器控制口连不上（OpenClash 没跑？密钥不对？）".into());
        }
        let configs = api_json("GET", ROUTER_API, &secret, "/configs", None)?;
        let port = configs.get("mixed-port").and_then(Value::as_u64).unwrap_or(7893) as u16;
        let mode = configs.get("mode").and_then(Value::as_str).unwrap_or("rule");
        let group = if mode == "global" { "GLOBAL".to_string() } else { cfg.router_group.clone() };
        return Ok(Backend {
            kind: "router",
            api: ROUTER_API.into(),
            secret,
            proxy: format!("{ROUTER_GATEWAY}:{port}"),
            group,
            test_group: "测速".into(),
        });
    }
    let (api, secret, port) = proxy::local_core().ok_or("本机 FlClash 控制口没找到（没开外部控制？）")?;
    let configs = api_json("GET", &api, &secret, "/configs", None)?;
    let mode = configs.get("mode").and_then(Value::as_str).unwrap_or("rule");
    let group = if mode == "global" { "GLOBAL".to_string() } else { cfg.local_group.clone() };
    Ok(Backend {
        kind: "local",
        api,
        secret,
        proxy: format!("127.0.0.1:{port}"),
        group: group.clone(),
        test_group: group,
    })
}

// ---------- 测速 ----------

/// 通过代理下载一段数据，返回 KB/s；失败/超时返回 None（按 0 处理）。
fn probe_speed(proxy_addr: &str, bytes: u32, timeout_secs: u64) -> Option<u32> {
    let url = format!("{TEST_URL}?bytes={bytes}");
    let output = Command::new("/usr/bin/curl")
        .args([
            "-s",
            "-x",
            &format!("http://{proxy_addr}"),
            "-m",
            &timeout_secs.to_string(),
            "-o",
            "/dev/null",
            "-w",
            "%{speed_download} %{http_code}",
            &url,
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut parts = text.split_whitespace();
    let bps: f64 = parts.next()?.parse().ok()?;
    let code: u16 = parts.next()?.parse().ok()?;
    if code != 200 {
        return None;
    }
    Some((bps / 1024.0).round() as u32)
}

fn current_node(backend: &Backend) -> Result<String, String> {
    let value = api_json(
        "GET",
        &backend.api,
        &backend.secret,
        &format!("/proxies/{}", encode_segment(&backend.group)),
        None,
    )?;
    value
        .get("now")
        .and_then(Value::as_str)
        .map(String::from)
        .ok_or_else(|| "组里没有 now 字段".into())
}

fn select_node(backend: &Backend, group: &str, node: &str) -> Result<(), String> {
    let body = format!("{{\"name\":{}}}", json_string(node));
    api_json(
        "PUT",
        &backend.api,
        &backend.secret,
        &format!("/proxies/{}", encode_segment(group)),
        Some(&body),
    )?;
    Ok(())
}

fn is_pseudo_node(name: &str) -> bool {
    name.starts_with("剩余流量")
        || name.starts_with("套餐到期")
        || name.starts_with("距离下次重置")
        || matches!(name, "DIRECT" | "REJECT" | "PASS" | "REJECT-DROP" | "COMPATIBLE")
}

fn group_members(backend: &Backend) -> Result<Vec<String>, String> {
    let value = api_json(
        "GET",
        &backend.api,
        &backend.secret,
        &format!("/proxies/{}", encode_segment(&backend.group)),
        None,
    )?;
    let members = value
        .get("all")
        .and_then(Value::as_array)
        .ok_or("组里没有 all 字段")?;
    Ok(members
        .iter()
        .filter_map(Value::as_str)
        .filter(|name| !is_pseudo_node(name))
        .map(String::from)
        .collect())
}

/// 并发延迟筛活节点：返回 (节点名, 延迟ms)，按延迟升序。
fn delay_sweep(backend: &Backend, members: &[String]) -> Vec<(String, u32)> {
    let results: Mutex<Vec<(String, u32)>> = Mutex::new(Vec::new());
    let chunks: Vec<&[String]> = members.chunks(DELAY_WORKERS.max(1)).collect();
    for chunk in chunks {
        std::thread::scope(|scope| {
            for name in chunk {
                let results = &results;
                scope.spawn(move || {
                    let path = format!(
                        "/proxies/{}/delay?timeout=3000&url={}",
                        encode_segment(name),
                        encode_segment(DELAY_URL)
                    );
                    if let Ok(value) =
                        api_json("GET", &backend.api, &backend.secret, &path, None)
                    {
                        if let Some(delay) = value.get("delay").and_then(Value::as_u64) {
                            if delay > 0 {
                                results.lock().unwrap().push((name.clone(), delay as u32));
                            }
                        }
                    }
                });
            }
        });
    }
    let mut out = results.into_inner().unwrap();
    out.sort_by_key(|(_, delay)| *delay);
    out
}

/// 节点名里的地区（国旗 emoji 后面的中文国名），用于同地区优先。
fn region_of(name: &str) -> String {
    for region in ["新加坡", "日本", "美国", "香港", "台湾", "韩国", "英国", "德国", "法国", "印度", "土耳其", "阿根廷", "俄罗斯", "加拿大", "澳大利亚"] {
        if name.contains(region) {
            return region.to_string();
        }
    }
    String::new()
}

// ---------- 巡检与救援 ----------

fn run_cycle(cfg: &AutoSwitchConfig, manual: bool) {
    let backend = match detect_backend(cfg) {
        Ok(b) => b,
        Err(reason) => {
            let mut rt = runtime().lock().unwrap();
            rt.backend = "none".into();
            rt.status = reason.clone();
            drop(rt);
            if manual {
                log_line(&format!("巡检跳过：{reason}"));
            }
            return;
        }
    };
    let node = match current_node(&backend) {
        Ok(n) => n,
        Err(reason) => {
            let mut rt = runtime().lock().unwrap();
            rt.backend = backend.kind.into();
            rt.status = format!("读当前节点失败：{reason}");
            return;
        }
    };
    {
        let mut rt = runtime().lock().unwrap();
        rt.backend = backend.kind.into();
        rt.current_node = node.clone();
    }

    let cheap = probe_speed(&backend.proxy, PROBE_BYTES, PROBE_TIMEOUT_SECS).unwrap_or(0);
    // 256KB 仍偏保守，健康链路偶尔也会读进 300 以下；低就再用 1MB 复核一次，
    // 拿复核值判「慢不慢」并显示，避免把没问题的链路误判成不达标而空跑救援甚至误切。
    let kbps = if cheap >= cfg.min_speed_kbps {
        cheap
    } else {
        probe_speed(&backend.proxy, CANDIDATE_BYTES, CANDIDATE_TIMEOUT_SECS).unwrap_or(cheap)
    };
    let now = unix_now();
    let bad = kbps < cfg.min_speed_kbps;
    let consecutive = {
        let mut rt = runtime().lock().unwrap();
        rt.last_speed_kbps = Some(kbps);
        rt.last_probe_at = Some(now);
        if bad {
            rt.consecutive_bad += 1;
        } else {
            rt.consecutive_bad = 0;
        }
        rt.status = if bad {
            format!("当前 {kbps}KB/s 低于阈值 {}KB/s（第 {} 次）", cfg.min_speed_kbps, rt.consecutive_bad)
        } else {
            format!("正常：{kbps}KB/s")
        };
        rt.consecutive_bad
    };
    log_line(&format!(
        "巡检 [{}/{}] {node} → {kbps}KB/s{}",
        backend.kind, backend.group,
        if bad { "（不达标）" } else { "" }
    ));

    if !bad {
        return;
    }
    if consecutive < 2 {
        // 第一次不达标：10 秒后复测确认，而不是等下一个完整巡检周期。
        let mut rt = runtime().lock().unwrap();
        rt.next_probe_at = now + CONFIRM_SECS;
        rt.status = format!("{kbps}KB/s 不达标，{CONFIRM_SECS} 秒后复测确认");
        return;
    }
    rescue(cfg, &backend, &node);
}

fn rescue(cfg: &AutoSwitchConfig, backend: &Backend, current: &str) {
    let day = today();
    let switches_today = {
        let mut rt = runtime().lock().unwrap();
        if rt.persisted.day != day {
            rt.persisted.day = day.clone();
            rt.persisted.switches_today = 0;
        }
        rt.persisted.switches_today
    };
    if switches_today >= cfg.daily_cap {
        log_line(&format!("今日已切换 {switches_today} 次，达到上限 {}，本次不切", cfg.daily_cap));
        let mut rt = runtime().lock().unwrap();
        rt.status = format!("今日切换已达上限（{} 次），保持当前节点", cfg.daily_cap);
        rt.consecutive_bad = 0;
        return;
    }

    let members = match group_members(backend) {
        Ok(m) => m,
        Err(reason) => {
            log_line(&format!("救援失败：读组成员 {reason}"));
            return;
        }
    };
    log_line(&format!("开始救援：{} 个候选，先并发测延迟", members.len()));
    let alive = delay_sweep(backend, &members);
    log_line(&format!("延迟筛活 {} 个", alive.len()));
    if alive.is_empty() {
        log_line("救援中止：所有节点延迟测试都不通");
        let mut rt = runtime().lock().unwrap();
        rt.status = "所有节点都不通，保持当前节点".into();
        rt.consecutive_bad = 0;
        return;
    }

    // 防横跳：10 分钟内离开过的节点先排除；全被排除时放宽。
    let now = unix_now();
    let recent: Vec<String> = {
        let rt = runtime().lock().unwrap();
        rt.persisted
            .recent_left
            .iter()
            .filter(|(_, ts)| now.saturating_sub(**ts) < NO_RETURN_SECS)
            .map(|(name, _)| name.clone())
            .collect()
    };
    let mut candidates: Vec<(String, u32)> = alive
        .iter()
        .filter(|(name, _)| name != current && !recent.contains(name))
        .cloned()
        .collect();
    if candidates.is_empty() {
        candidates = alive.iter().filter(|(name, _)| name != current).cloned().collect();
        log_line("防横跳过滤后没有候选，放宽限制");
    }
    candidates.truncate(3);
    if candidates.is_empty() {
        log_line("救援中止：除当前节点外没有活节点");
        return;
    }

    // 当前节点也用同口径（1MB）测一次：巡检的小样本受延迟影响仍偏保守，
    // 拿它和候选的 1MB 比会把高延迟节点误判成「更快」，越换越慢。
    if backend.kind == "router" {
        let _ = select_node(backend, &backend.test_group, current);
    }
    let current_fair = probe_speed(&backend.proxy, CANDIDATE_BYTES, CANDIDATE_TIMEOUT_SECS).unwrap_or(0);
    log_line(&format!("当前节点 {current} 同口径实测：{current_fair}KB/s"));

    // 逐个实测：路由器切「测速」组（用户出口不动），本机短暂切主组。
    let mut measured: Vec<(String, u32, u32)> = Vec::new(); // (name, delay, kbps)
    for (name, delay) in &candidates {
        if select_node(backend, &backend.test_group, name).is_err() {
            log_line(&format!("候选 {name}：切不过去，跳过"));
            continue;
        }
        let kbps = probe_speed(&backend.proxy, CANDIDATE_BYTES, CANDIDATE_TIMEOUT_SECS).unwrap_or(0);
        log_line(&format!("候选 {name}：延迟 {delay}ms，实测 {kbps}KB/s"));
        measured.push((name.clone(), *delay, kbps));
    }
    // 本机后端：测速把主组切到了最后一个候选，先切回当前节点，避免用户出口停在候选上。
    if backend.kind == "local" {
        let _ = select_node(backend, &backend.group, current);
    }

    let Some(best) = pick_best(&measured, current, current_fair, cfg.min_speed_kbps) else {
        // 当前节点本身就不达标、又没有达标且更快的候选 → 多半是链路整体掉速，不是节点问题。
        let outage = current_fair < cfg.min_speed_kbps;
        log_line(if outage {
            "救援中止：当前与候选都偏慢（像是链路整体掉速/路由器抽风），保持当前节点"
        } else {
            "救援中止：候选没有明显更快，保持当前节点"
        });
        let mut rt = runtime().lock().unwrap();
        rt.status = if outage {
            "所有节点都不通，保持当前节点".into()
        } else {
            "候选并不比当前快，保持当前节点".into()
        };
        rt.consecutive_bad = 0;
        return;
    };

    if select_node(backend, &backend.group, &best).is_err() {
        log_line(&format!("救援失败：主组切到 {best} 没成功"));
        return;
    }
    if backend.kind == "router" {
        // 测速组跟随主组，下一轮巡检量的就是新节点。
        let _ = select_node(backend, &backend.test_group, &best);
    }
    let verify = probe_speed(&backend.proxy, CANDIDATE_BYTES, CANDIDATE_TIMEOUT_SECS).unwrap_or(0);
    log_line(&format!("已切换：{current} → {best}，复验 {verify}KB/s"));

    let mut rt = runtime().lock().unwrap();
    rt.persisted.switches_today += 1;
    rt.persisted.last_switch_at = Some(unix_now());
    rt.persisted.last_switch_from = current.to_string();
    rt.persisted.last_switch_to = best.clone();
    rt.persisted.recent_left.insert(current.to_string(), unix_now());
    rt.persisted.recent_left.retain(|_, ts| unix_now().saturating_sub(*ts) < 7200);
    rt.current_node = best.clone();
    rt.consecutive_bad = 0;
    rt.last_speed_kbps = Some(verify);
    rt.status = format!("已自动切换到 {best}（{verify}KB/s）");
    write_persisted(&rt.persisted);
}

/// 选最优：候选必须达标（>=min_speed）且比当前节点明显更快（>20%，同口径 1MB）才值得换；
/// 当前彻底卡死（0）时只放宽「比当前快」这一条，候选仍需达标——全都不达标就判为链路
/// 整体掉速、不切，避免在路由器抽风时切到另一个同样不通的节点白烧额度。
/// 合格者里同地区且速度在最优 20% 以内时优先同地区（IP 归属地稳定），否则取最快。
fn pick_best(
    measured: &[(String, u32, u32)],
    current: &str,
    current_fair: u32,
    min_speed: u32,
) -> Option<String> {
    let usable: Vec<(String, u32, u32)> = measured
        .iter()
        .filter(|(_, _, k)| {
            // 候选必须先达标：链路整体掉速（路由器抽风）时切到另一个同样慢的节点
            // 毫无意义，只会白烧额度。当前卡死(0)时只放宽「要比当前快」这一条，
            // 但仍要求候选本身够快，否则判为链路问题、不切。
            *k >= min_speed
                && (current_fair == 0 || u64::from(*k) * 5 > u64::from(current_fair) * 6)
        })
        .cloned()
        .collect();
    if usable.is_empty() {
        return None;
    }
    let top = usable.iter().map(|(_, _, k)| *k).max().unwrap_or(0);
    let current_region = region_of(current);
    let same_region = usable.iter().find(|(name, _, k)| {
        !current_region.is_empty() && region_of(name) == current_region && *k * 5 >= top * 4
    });
    let chosen = match same_region {
        Some(c) => c.clone(),
        None => usable.iter().max_by_key(|(_, _, k)| *k).unwrap().clone(),
    };
    Some(chosen.0)
}

// ---------- 单元测试 ----------

#[cfg(test)]
mod tests {
    use super::*;

    fn m(name: &str, delay: u32, kbps: u32) -> (String, u32, u32) {
        (name.to_string(), delay, kbps)
    }

    #[test]
    fn pseudo_nodes_filtered() {
        assert!(is_pseudo_node("剩余流量：90.42 GB"));
        assert!(is_pseudo_node("套餐到期：2026-12-01"));
        assert!(is_pseudo_node("距离下次重置：7 天"));
        assert!(is_pseudo_node("DIRECT"));
        assert!(!is_pseudo_node("🇸🇬新加坡专线02|BGP|流媒体"));
    }

    #[test]
    fn picks_fastest_when_regions_differ() {
        let measured = vec![
            m("🇯🇵日本高速01", 100, 900),
            m("🇸🇬新加坡专线02", 150, 2000),
        ];
        let best = pick_best(&measured, "🇸🇬新加坡专线01", 0, 300).unwrap();
        assert_eq!(best, "🇸🇬新加坡专线02");
    }

    #[test]
    fn prefers_same_region_within_20_percent() {
        let measured = vec![
            m("🇯🇵日本高速01", 100, 2000),
            m("🇸🇬新加坡专线02", 150, 1700),
        ];
        let best = pick_best(&measured, "🇸🇬新加坡专线01", 0, 300).unwrap();
        assert_eq!(best, "🇸🇬新加坡专线02");
    }

    #[test]
    fn same_region_too_slow_loses() {
        let measured = vec![
            m("🇯🇵日本高速01", 100, 2000),
            m("🇸🇬新加坡专线02", 150, 1000),
        ];
        let best = pick_best(&measured, "🇸🇬新加坡专线01", 0, 300).unwrap();
        assert_eq!(best, "🇯🇵日本高速01");
    }

    #[test]
    fn all_slow_keeps_current_even_when_dead() {
        // 候选不达标（<300）时，即使当前卡死(0)也不切：切到另一个慢节点没用，
        // 像是链路整体掉速，白烧额度。
        let slow = vec![m("🇯🇵日本高速01", 100, 100)];
        assert!(pick_best(&slow, "🇸🇬新加坡专线01", 50, 300).is_none());
        assert!(pick_best(&slow, "🇸🇬新加坡专线01", 0, 300).is_none());
        // 当前卡死、但有达标候选时，切过去。
        let good = vec![m("🇯🇵日本高速01", 100, 400)];
        assert_eq!(pick_best(&good, "🇸🇬新加坡专线01", 0, 300).unwrap(), "🇯🇵日本高速01");
    }

    #[test]
    fn region_extraction() {
        assert_eq!(region_of("🇸🇬新加坡专线02|BGP|流媒体"), "新加坡");
        assert_eq!(region_of("🇯🇵日本高速05|BGP|CUCM"), "日本");
        assert_eq!(region_of("剩余流量：90 GB"), "");
    }

    #[test]
    fn civil_date_known_value() {
        // 2026-11-13 距 1970-01-01 共 20770 天。
        assert_eq!(civil_from_days(20770), (2026, 11, 13));
    }

    /// 联网验收（路由器后端）：把「测速」组指到死节点模拟卡死，跑两轮巡检触发救援，
    /// 断言主组被切到活节点、全程 < 2 分钟、用户出口 IP 不变。
    /// 跑法：M8_API_SECRET=<密钥> cargo test --lib live_router_rescue -- --ignored --nocapture
    #[test]
    #[ignore]
    fn live_router_rescue() {
        let secret = std::env::var("M8_API_SECRET").expect("需要 M8_API_SECRET 环境变量");
        secrets::store(SECRET_ACCOUNT, &secret).unwrap();
        let cfg = AutoSwitchConfig { enabled: true, ..Default::default() };
        write_config_file(&cfg).unwrap();

        let backend = detect_backend(&cfg).expect("应识别为路由器后端");
        assert_eq!(backend.kind, "router");
        let original = current_node(&backend).unwrap();
        eprintln!("主组当前节点：{original}");

        // 用户出口 IP（走主组）：救援前后必须一致。
        let exit_ip = |proxy: &str| {
            Command::new("/usr/bin/curl")
                .args(["-s", "-x", &format!("http://{proxy}"), "-m", "10", "https://api.ipify.org"])
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_default()
        };
        let ip_before = exit_ip(&backend.proxy);
        eprintln!("救援前出口 IP：{ip_before}");

        // 只动「测速」组：主组不动，用户流量不受影响。
        let dead = "🇸🇬新加坡高速06|BGP|CTCU";
        select_node(&backend, &backend.test_group, dead).unwrap();
        // 测速组换节点不应改变用户出口 IP。
        let ip_during = exit_ip(&backend.proxy);
        assert_eq!(ip_before, ip_during, "切测速组改变了用户出口 IP");

        let start = std::time::Instant::now();
        run_cycle(&cfg, true); // 第一次不达标
        run_cycle(&cfg, true); // 第二次不达标 → 救援
        let elapsed = start.elapsed();
        eprintln!("两轮巡检 + 救援耗时：{elapsed:?}");
        assert!(elapsed.as_secs() < 120, "救援超过 2 分钟");

        let after = current_node(&backend).unwrap();
        eprintln!("救援后主组节点：{after}");
        assert_ne!(after, dead, "救援后仍停在死节点");
        assert_ne!(after, original, "救援没有切换主组");

        // 恢复用户原来的节点，验收不留副作用。
        select_node(&backend, &backend.group, &original).unwrap();
        select_node(&backend, &backend.test_group, &original).unwrap();
        let ip_after = exit_ip(&backend.proxy);
        assert_eq!(ip_before, ip_after, "恢复后出口 IP 变了");
        eprintln!("已恢复原节点，出口 IP 一致：{ip_after}");
    }

    /// 联网验收（本机 FlClash 后端）：只验证控制口发现 + 当前节点 + 测速读数链路。
    /// 跑法：cargo test --lib live_local_probe -- --ignored --nocapture
    #[test]
    #[ignore]
    fn live_local_probe() {
        let (api, secret, port) = proxy::local_core().expect("FlClash 控制口没找到（没运行？）");
        let backend = Backend {
            kind: "local",
            api,
            secret,
            proxy: format!("127.0.0.1:{port}"),
            group: "良心云".into(),
            test_group: "良心云".into(),
        };
        let node = current_node(&backend).unwrap();
        let speed = probe_speed(&backend.proxy, PROBE_BYTES, PROBE_TIMEOUT_SECS);
        eprintln!("FlClash 当前节点：{node}，测速：{speed:?} KB/s");
        let members = group_members(&backend).unwrap();
        eprintln!("组成员 {} 个（已滤伪节点）", members.len());
        assert!(!members.is_empty());
    }

    /// 联网验收（本机 FlClash 救援）：当前节点不达标时跑真实救援，切到实测最快的活节点。
    /// 跑法：cargo test --lib live_local_rescue -- --ignored --nocapture
    #[test]
    #[ignore]
    fn live_local_rescue() {
        let (api, secret, port) = proxy::local_core().expect("FlClash 控制口没找到");
        let backend = Backend {
            kind: "local",
            api,
            secret,
            proxy: format!("127.0.0.1:{port}"),
            group: "良心云".into(),
            test_group: "良心云".into(),
        };
        let cfg = AutoSwitchConfig { enabled: true, ..Default::default() };
        let before = current_node(&backend).unwrap();
        let speed = probe_speed(&backend.proxy, PROBE_BYTES, PROBE_TIMEOUT_SECS).unwrap_or(0);
        eprintln!("救援前：{before}，{speed}KB/s");
        if speed >= cfg.min_speed_kbps {
            eprintln!("当前节点已达标，无需救援");
            return;
        }
        let start = std::time::Instant::now();
        rescue(&cfg, &backend, &before);
        eprintln!("救援耗时：{:?}", start.elapsed());
        let after = current_node(&backend).unwrap();
        let after_speed = probe_speed(&backend.proxy, PROBE_BYTES, PROBE_TIMEOUT_SECS).unwrap_or(0);
        eprintln!("救援后：{after}，{after_speed}KB/s");
        assert_ne!(after, before, "救援没有切换节点");
    }

    /// 联网实测：把真实巡检循环连跑 6 轮（与后台线程同一份 run_cycle），
    /// 逐轮打印耗时 / 速度 / 决策，看它在真实网络下究竟怎么判断。
    /// 跑法：cargo test --lib live_loop -- --ignored --nocapture
    #[test]
    #[ignore]
    fn live_loop() {
        let cfg = AutoSwitchConfig { enabled: true, ..Default::default() };
        let backend = detect_backend(&cfg).expect("应识别到后端");
        eprintln!("后端 {} / 组 {} / 当前 {}", backend.kind, backend.group, current_node(&backend).unwrap());
        for i in 1..=6 {
            let t = std::time::Instant::now();
            run_cycle(&cfg, true);
            let rt = runtime().lock().unwrap();
            eprintln!(
                "[轮 {i}] 耗时 {:?} | 速度 {:?}KB/s | 连续不达标 {} | {}",
                t.elapsed(), rt.last_speed_kbps, rt.consecutive_bad, rt.status
            );
            drop(rt);
            if i < 6 {
                std::thread::sleep(std::time::Duration::from_secs(12));
            }
        }
    }
}