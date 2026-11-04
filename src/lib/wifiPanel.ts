/**
 * Wi‑Fi 二级页的纯逻辑（正版 `WiFiNetworkModels` + `LinkDetailPresentation` +
 * `PanelDetailMapper.wifi` 的 TS 版）。
 *
 * 放在 lib 里是为了能离线断言：分组成哪几节、哪条排前面、折叠态露出几行、
 * 无权限时该说哪句话——这些都会直接决定和正版像不像。
 */
import type {
  StatusBarWifiCandidate,
  StatusBarWifiDetails,
  StatusBarWifiScan,
} from "./types";

/** 正版 `WiFiSecurityKind`：值就是 `CWSecurity` 原始值，-1 为未知。 */
export type WifiSecurity =
  | "unknown"
  | "open"
  | "wep"
  | "wpaPersonal"
  | "wpaPersonalMixed"
  | "wpa2Personal"
  | "personal"
  | "dynamicWEP"
  | "wpaEnterprise"
  | "wpaEnterpriseMixed"
  | "wpa2Enterprise"
  | "enterprise"
  | "wpa3Personal"
  | "wpa3Enterprise"
  | "wpa3Transition"
  | "owe"
  | "oweTransition";

const KIND_BY_RAW: Record<number, WifiSecurity> = {
  [-1]: "unknown",
  0: "open",
  1: "wep",
  2: "wpaPersonal",
  3: "wpaPersonalMixed",
  4: "wpa2Personal",
  5: "personal",
  6: "dynamicWEP",
  7: "wpaEnterprise",
  8: "wpaEnterpriseMixed",
  9: "wpa2Enterprise",
  10: "enterprise",
  11: "wpa3Personal",
  12: "wpa3Enterprise",
  13: "wpa3Transition",
  14: "owe",
  15: "oweTransition",
};

export function securityKind(raw: number): WifiSecurity {
  return KIND_BY_RAW[raw] ?? "unknown";
}

/** 正版 `requiresPassword`：开放、OWE 和未知都不画锁。 */
export function requiresPassword(security: WifiSecurity): boolean {
  return (
    security !== "open" && security !== "owe" && security !== "oweTransition" && security !== "unknown"
  );
}

/** 正版 `LinkDetailPresentation.securityName`：多数安全类型是协议名，不翻译。 */
export function securityName(security: WifiSecurity): string {
  switch (security) {
    case "open":
      return "开放网络";
    case "wep":
    case "dynamicWEP":
      return "WEP";
    case "wpaPersonal":
    case "wpaPersonalMixed":
      return "WPA";
    case "wpa2Personal":
    case "personal":
      return "WPA2";
    case "wpa3Personal":
    case "wpa3Transition":
      return "WPA3";
    case "owe":
      return "OWE";
    case "oweTransition":
      return "OWE Transition";
    case "wpaEnterprise":
    case "wpaEnterpriseMixed":
    case "wpa2Enterprise":
    case "enterprise":
    case "wpa3Enterprise":
      return "企业网络";
    case "unknown":
      return UNAVAILABLE;
  }
}

/** 正版 `StatusMappings.wifiBars`：只有 4 档（含 0）。 */
export function wifiBars(rssi: number | null): number {
  if (rssi === null) return 0;
  if (rssi >= -60) return 3;
  if (rssi >= -78) return 2;
  if (rssi >= -88) return 1;
  return 0;
}

const UNAVAILABLE = "不可用";

/** 一个网络（同一 SSID + 安全类型的多条 AP 合成一行）。 */
export interface WifiNetwork {
  key: string;
  ssid: string;
  security: WifiSecurity;
  /** `CWSecurity` 原值：正版排序最后一级用它。 */
  securityRaw: number;
  rssi: number | null;
  isConnected: boolean;
  isKnown: boolean;
  /** 正版只在读不到 SSID 时写「隐藏网络」；扫描阶段匿名广播已被丢弃。 */
  displayName: string;
  signalValue: number;
  /** 已连接时画 `personalhotspot`（正版 `trailingSymbol`）。 */
  hotspotSymbol: boolean;
}

/** 正版 `candidateComesFirst`：信号强的在前，其次按 BSSID 字典序。 */
function candidateComesFirst(a: StatusBarWifiCandidate, b: StatusBarWifiCandidate): number {
  const left = a.rssi ?? Number.MIN_SAFE_INTEGER;
  const right = b.rssi ?? Number.MIN_SAFE_INTEGER;
  if (left !== right) return right - left;
  return (a.bssid ?? "") < (b.bssid ?? "") ? -1 : (a.bssid ?? "") > (b.bssid ?? "") ? 1 : 0;
}

function sameBssid(left: string | null, right: string | null): boolean {
  if (!left || !right) return false;
  return left.toLowerCase() === right.toLowerCase();
}

/**
 * 正版 `WiFiNetwork.merge`：按 (SSID, 安全) 分组，
 * 排序为「已连接 → 信号强 → SSID 名 → 安全类型原值」。
 */
export function mergeNetworks(
  candidates: StatusBarWifiCandidate[],
  connectedBssid: string | null,
  knownSsids: string[],
  hotspot: boolean,
): WifiNetwork[] {
  const known = new Set(knownSsids);
  const groups = new Map<string, { ssid: string; security: number; items: StatusBarWifiCandidate[] }>();
  for (const candidate of candidates) {
    const key = `${candidate.ssid}\u0000${candidate.security}`;
    const group = groups.get(key);
    if (group) group.items.push(candidate);
    else groups.set(key, { ssid: candidate.ssid, security: candidate.security, items: [candidate] });
  }

  const networks: WifiNetwork[] = [];
  for (const [key, group] of groups) {
    const items = [...group.items].sort(candidateComesFirst);
    const isConnected = items.some((item) => sameBssid(item.bssid, connectedBssid));
    const security = securityKind(group.security);
    const rssi = items[0]?.rssi ?? null;
    networks.push({
      key,
      ssid: group.ssid,
      security,
      securityRaw: group.security,
      rssi,
      isConnected,
      isKnown: known.has(group.ssid),
      displayName: group.ssid || "隐藏网络",
      signalValue: Math.max(0.25, wifiBars(rssi) / 3),
      hotspotSymbol: hotspot && isConnected,
    });
  }

  return networks.sort((lhs, rhs) => {
    if (lhs.isConnected !== rhs.isConnected) return lhs.isConnected ? -1 : 1;
    const left = lhs.rssi ?? Number.MIN_SAFE_INTEGER;
    const right = rhs.rssi ?? Number.MIN_SAFE_INTEGER;
    if (left !== right) return right - left;
    const byName = compareNames(lhs.ssid, rhs.ssid);
    if (byName !== 0) return byName;
    return lhs.securityRaw - rhs.securityRaw;
  });
}

/** 正版 `localizedCaseInsensitiveCompare`：中文界面下按拼音序，忽略大小写。 */
function compareNames(left: string, right: string): number {
  if (left === right) return 0;
  const collator = new Intl.Collator("zh-Hans", { sensitivity: "base" });
  return collator.compare(left, right);
}

/** 正版 `WiFiNetworkPresentation.grouped`：热点 → 已知 → 其他。 */
export function groupNetworks(
  networks: WifiNetwork[],
  hotspot: boolean,
): { personalHotspot: WifiNetwork[]; known: WifiNetwork[]; other: WifiNetwork[] } {
  const personalHotspot = networks.filter((n) => hotspot && n.isConnected);
  const hotspotKeys = new Set(personalHotspot.map((n) => n.key));
  const remaining = networks.filter((n) => !hotspotKeys.has(n.key));
  return {
    personalHotspot,
    known: remaining.filter((n) => n.isKnown || n.isConnected),
    other: remaining.filter((n) => !n.isKnown && !n.isConnected),
  };
}

export interface WifiDetailRow {
  label: string;
  value: string;
  copyable: boolean;
}

function valueOrUnavailable(value: string | null | undefined): string {
  return value && value.length > 0 ? value : UNAVAILABLE;
}

/**
 * 正版 `LinkDetailPresentation.wirelessRows`：折叠 9 行（无线电），
 * 展开再加噪声 / 信噪比 / 国家代码 + 共享 5 行（地址类可复制）。
 */
export function wirelessRows(details: StatusBarWifiDetails, expanded: boolean): WifiDetailRow[] {
  const rows: WifiDetailRow[] = [
    { label: "SSID", value: valueOrUnavailable(details.ssid), copyable: false },
    { label: "BSSID", value: valueOrUnavailable(details.bssid), copyable: true },
    { label: "频段", value: valueOrUnavailable(details.band), copyable: false },
    { label: "信道", value: details.channel === null ? UNAVAILABLE : String(details.channel), copyable: false },
    { label: "信道宽度", value: valueOrUnavailable(details.channel_width), copyable: false },
    { label: "RSSI", value: details.rssi === null ? UNAVAILABLE : `${details.rssi} dBm`, copyable: false },
    { label: "PHY 模式", value: valueOrUnavailable(details.phy_mode), copyable: false },
    {
      label: "链路发送速率",
      value: details.transmit_rate === null ? UNAVAILABLE : `${details.transmit_rate.toFixed(1)} Mbps`,
      copyable: false,
    },
    { label: "安全类型", value: securityName(securityKind(details.security)), copyable: false },
  ];
  if (!expanded) return rows;

  const snr = signalToNoiseRatio(details.rssi, details.noise);
  return [
    ...rows,
    { label: "噪声", value: details.noise === null ? UNAVAILABLE : `${details.noise} dBm`, copyable: false },
    { label: "信噪比", value: snr === null ? UNAVAILABLE : `${snr} dB`, copyable: false },
    { label: "国家代码", value: valueOrUnavailable(details.country_code), copyable: false },
    { label: "接口", value: valueOrUnavailable(details.interface_name), copyable: false },
    { label: "IPv4", value: valueOrUnavailable(details.ipv4_addresses.join(", ")), copyable: true },
    { label: "IPv6", value: valueOrUnavailable(details.ipv6_addresses.join(", ")), copyable: true },
    { label: "路由器", value: valueOrUnavailable(details.router), copyable: true },
    { label: "DNS", value: valueOrUnavailable(details.dns_servers.join(", ")), copyable: true },
  ];
}

/** 正版 `signalToNoiseRatio`：两个读数都为负且 RSSI 不低于噪声才算。 */
export function signalToNoiseRatio(rssi: number | null, noise: number | null): number | null {
  if (rssi === null || noise === null) return null;
  if (rssi >= 0 || noise >= 0 || rssi < noise) return null;
  return rssi - noise;
}

export type WifiMessageIntent = "none" | "request_name_access" | "location_settings" | "scanning";

export interface WifiMessage {
  text: string;
  intent: WifiMessageIntent;
}

/**
 * 正版 `PanelDetailMapper.wifi` 的 message 分支，逐条照抄判序：
 * 未授权的空扫描要说「缺权限」，不能说「未发现附近网络」。
 */
export function wifiMessage(
  scan: StatusBarWifiScan,
  hasVisibleNetworks: boolean,
  scanning: boolean,
): WifiMessage | null {
  if (scanning) return { text: "正在扫描网络…", intent: "scanning" };
  const access = scan.name_access;
  switch (scan.list_state) {
    case "noInterface":
      return { text: "没有可用的 Wi-Fi 网卡。", intent: "none" };
    case "poweredOff":
      return { text: "Wi-Fi 已关闭。", intent: "none" };
    case "failed":
      if (access === "not_determined") {
        return { text: "允许定位以显示 Wi-Fi 名称", intent: "request_name_access" };
      }
      return { text: "无法扫描 Wi-Fi 网络。", intent: "none" };
    case "ready":
      if (access === "not_determined" && !hasVisibleNetworks) {
        return { text: "允许定位以显示 Wi-Fi 名称", intent: "request_name_access" };
      }
      if (!hasVisibleNetworks && access !== "authorized") {
        return access === "denied" || access === "restricted"
          ? { text: "需要定位权限才能读取 Wi-Fi 名称和扫描结果。前往定位服务设置。", intent: "location_settings" }
          : { text: "允许定位以显示 Wi-Fi 名称", intent: "request_name_access" };
      }
      if (!hasVisibleNetworks) return { text: "未发现附近网络。", intent: "none" };
      return null;
  }
}
