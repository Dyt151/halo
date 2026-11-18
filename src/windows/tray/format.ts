//! 面板文案：电量、网络与蓝牙状态的短句。

import type {
  StatusBarBattery,
  StatusBarBluetooth,
  StatusBarNetwork,
} from "../../lib/types";

/* 文案                                                                */
/* ------------------------------------------------------------------ */

export function timeToFullText(minutes: number | null): string {
  if (minutes === null || minutes <= 0) return "正在计算充满时间";
  const h = Math.floor(minutes / 60);
  const m = minutes % 60;
  if (h === 0) return `预计 ${m} 分钟充满`;
  if (m === 0) return `预计 ${h} 小时充满`;
  return `预计 ${h} 小时 ${m} 分钟充满`;
}

export function batterySubtitle(battery: StatusBarBattery): string {
  // 正版 `batterySubtitle` 的判序：无电池 → 已充满 → 充电中 → 低电量模式 → 已连接电源 → 电池供电。
  if (!battery.present) return "无电池设备";
  if (battery.charged) return "已充满";
  if (battery.charging) return timeToFullText(battery.minutes_to_full);
  if (battery.low_power) return "低电量模式";
  if (battery.plugged) return "已连接电源";
  return "电池供电";
}

/* 详情页的数字格式：正版用 `Formatted(.number.precision(.fractionLength(n)))`。 */

export const UNAVAILABLE = "暂不可用";

export function fixed(value: number, digits: number): string {
  return value.toFixed(digits);
}

/** 功率行：1 位小数 + " W"。 */
export function wattsText(value: number | null): string {
  return value === null ? UNAVAILABLE : `${fixed(value, 1)} W`;
}

/** 功率卡：绝对值先四舍五入到 0.01，≥100 后不再带小数（窄卡放不下）。 */
export function cardWattsText(value: number | null): string {
  if (value === null) return UNAVAILABLE;
  const rounded = Math.round(Math.abs(value) * 100) / 100;
  return `${value.toFixed(rounded >= 100 ? 0 : 2)} W`;
}

export function remainingTimeText(minutes: number | null): string {
  if (minutes === null || minutes < 1) return UNAVAILABLE;
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  if (hours === 0) return `${rest}分钟`;
  if (rest === 0) return `${hours}小时`;
  return `${hours}小时${rest}分钟`;
}

/**
 * 正版 `WiFiSummaryPresentation` 的两条判定：`connection == .wifi`（走无线路由）
 * 且 `state ∈ {connected, hotspot}`，才会用网络名当标题、才会给链路读数。
 */
export function summaryEligible(network: StatusBarNetwork): boolean {
  return (
    network.connected && (network.link === "wifi" || network.link === "hotspot")
  );
}

export function networkTitle(network: StatusBarNetwork): string {
  if (summaryEligible(network) && network.ssid) return network.ssid;
  // 正版把有线链路单独画一行（`ethernet.title`）；本端口只有一行网络，沿用它的文案。
  return network.link === "ethernet" ? "以太网" : "Wi-Fi";
}

/** 正版 `measurements`：`%@ GHz · %d dBm`，读数缺失就少一段。 */
export function networkMeasurements(network: StatusBarNetwork): string | null {
  if (!summaryEligible(network)) return null;
  const parts: string[] = [];
  if (network.band) parts.push(network.band);
  if (network.rssi !== null && network.rssi < 0) parts.push(`${network.rssi} dBm`);
  return parts.length > 0 ? parts.join(" · ") : null;
}

/** 正版 `wifiSubtitle(_:)`：链路状态本身，不含授权提示。 */
export function networkStateText(network: StatusBarNetwork): string {
  if (!network.available) return "无法读取网络状态";
  if (!network.powered) return "Wi-Fi 关闭或不可用";
  if (!network.connected) return "Wi-Fi 开启，未关联";
  switch (network.link) {
    case "hotspot":
      return "使用 iPhone 热点";
    case "temporary":
      return "临时 Wi-Fi 连接";
    case "sharing":
      return "正在共享互联网";
    default:
      return "已连接";
  }
}

/**
 * 正版 `wifiSummary` 的副行顺序：有网络名 → 读数（读数缺失才用状态）；
 * 名字读不到但链路有名 → 直接用名；否则状态 + 读数；最前面再拼上本机 IP。
 */
export function networkSubtitle(network: StatusBarNetwork): string {
  const measurements = networkMeasurements(network);
  const hasSummaryName = summaryEligible(network) && Boolean(network.ssid);
  const body = hasSummaryName
    ? measurements ?? networkStateText(network)
    : network.ssid
      ? network.ssid
      : measurements
        ? `${networkStateText(network)} · ${measurements}`
        : networkStateText(network);
  return network.ipv4 ? `${network.ipv4} · ${body}` : body;
}

/** 蓝牙列表的可见上限（正版 `standard` 预设）。 */
export const MAX_VISIBLE_BLUETOOTH_DEVICES = 5;

export function bluetoothSubtitle(state: StatusBarBluetooth): string {
  if (!state.available) return "蓝牙不可用。";
  if (!state.powered) return "蓝牙已关闭。";
  const connected = state.devices.filter((device) => device.connected);
  if (connected.length === 0) return "没有已连接设备";
  // 正版 `BluetoothSummary.deviceSegments`：设备之间用「、」，电量属于该设备，
  // 用「 · 」挂在名字后面。
  return connected
    .map((device) =>
      device.battery === null ? device.name : `${device.name} · ${device.battery}%`,
    )
    .join("、");
}

/* ------------------------------------------------------------------ */
/* 面板                                                                */
/* ------------------------------------------------------------------ */
