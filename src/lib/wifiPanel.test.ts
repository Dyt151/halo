import { describe, expect, it } from "vitest";
import {
  groupNetworks,
  mergeNetworks,
  requiresPassword,
  securityKind,
  securityName,
  signalToNoiseRatio,
  wifiBars,
  wifiMessage,
  wifiRowAction,
  wirelessRows,
} from "./wifiPanel";
import type { StatusBarWifiCandidate, StatusBarWifiDetails, StatusBarWifiScan } from "./types";

function candidate(overrides: Partial<StatusBarWifiCandidate> = {}): StatusBarWifiCandidate {
  return {
    ssid: "Home",
    bssid: "aa:bb:cc:dd:ee:ff",
    rssi: -50,
    channel: 6,
    security: 4,
    ...overrides,
  };
}

function scan(overrides: Partial<StatusBarWifiScan> = {}): StatusBarWifiScan {
  return {
    available: true,
    powered: true,
    name_access: "authorized",
    list_state: "ready",
    candidates: [],
    connected_bssid: null,
    known_ssids: [],
    details: {
      ssid: null,
      bssid: null,
      band: null,
      channel: null,
      channel_width: null,
      rssi: null,
      noise: null,
      phy_mode: null,
      transmit_rate: null,
      security: -1,
      country_code: null,
      interface_name: null,
      ipv4_addresses: [],
      ipv6_addresses: [],
      router: null,
      dns_servers: [],
    },
    hotspot: false,
    ...overrides,
  };
}

function details(overrides: Partial<StatusBarWifiDetails> = {}): StatusBarWifiDetails {
  return {
    ssid: "Home",
    bssid: "aa:bb:cc:dd:ee:ff",
    band: "5 GHz",
    channel: 149,
    channel_width: "80 MHz",
    rssi: -47,
    noise: -95,
    phy_mode: "802.11ax",
    transmit_rate: 600.5,
    security: 4,
    country_code: "CN",
    interface_name: "en0",
    ipv4_addresses: ["192.168.1.8"],
    ipv6_addresses: ["fe80::1"],
    router: "192.168.1.1",
    dns_servers: ["192.168.1.1", "8.8.8.8"],
    ...overrides,
  };
}

describe("securityKind / requiresPassword / securityName", () => {
  it("CWSecurity 原值一一对应，越界当未知", () => {
    expect(securityKind(4)).toBe("wpa2Personal");
    expect(securityKind(13)).toBe("wpa3Transition");
    expect(securityKind(99)).toBe("unknown");
    expect(securityKind(-1)).toBe("unknown");
  });

  it("开放、OWE、未知不画锁，其余都要密码", () => {
    expect(requiresPassword("open")).toBe(false);
    expect(requiresPassword("owe")).toBe(false);
    expect(requiresPassword("oweTransition")).toBe(false);
    expect(requiresPassword("unknown")).toBe(false);
    expect(requiresPassword("wpa2Personal")).toBe(true);
    expect(requiresPassword("wpa3Enterprise")).toBe(true);
  });

  it("协议名不翻译，只有开放/企业要译（正版 securityName）", () => {
    expect(securityName("open")).toBe("开放网络");
    expect(securityName("wep")).toBe("WEP");
    expect(securityName("dynamicWEP")).toBe("WEP");
    expect(securityName("wpaPersonalMixed")).toBe("WPA");
    expect(securityName("personal")).toBe("WPA2");
    expect(securityName("wpa3Transition")).toBe("WPA3");
    expect(securityName("owe")).toBe("OWE");
    expect(securityName("oweTransition")).toBe("OWE Transition");
    expect(securityName("wpa2Enterprise")).toBe("企业网络");
    expect(securityName("unknown")).toBe("不可用");
  });
});

describe("wifiBars", () => {
  it("正版四档：-70 及以上满格（`StatusMappings.wifiBars` 的 -70/-78/-88）", () => {
    expect(wifiBars(null)).toBe(0);
    expect(wifiBars(-59)).toBe(3);
    expect(wifiBars(-70)).toBe(3);
    expect(wifiBars(-71)).toBe(2);
    expect(wifiBars(-78)).toBe(2);
    expect(wifiBars(-79)).toBe(1);
    expect(wifiBars(-88)).toBe(1);
    expect(wifiBars(-89)).toBe(0);
  });
});

describe("mergeNetworks", () => {
  it("同 SSID 同安全类型的多条 AP 合成一行，取最强信号", () => {
    const networks = mergeNetworks(
      [
        candidate({ bssid: "11:11:11:11:11:11", rssi: -80 }),
        candidate({ bssid: "22:22:22:22:22:22", rssi: -45 }),
        candidate({ bssid: "33:33:33:33:33:33", rssi: -70 }),
      ],
      null,
      [],
      false,
    );
    expect(networks).toHaveLength(1);
    expect(networks[0]?.rssi).toBe(-45);
    expect(networks[0]?.signalValue).toBe(1);
  });

  it("同 SSID 不同安全类型算两个网络（正版 identity 含安全）", () => {
    const networks = mergeNetworks(
      [candidate({ security: 4 }), candidate({ security: 0, bssid: "99:99:99:99:99:99" })],
      null,
      [],
      false,
    );
    expect(networks).toHaveLength(2);
  });

  it("排序：已连接 → 信号强 → 名称 → 安全类型", () => {
    const networks = mergeNetworks(
      [
        candidate({ ssid: "Zeta", bssid: "aa:aa:aa:aa:aa:01", rssi: -40 }),
        candidate({ ssid: "Alpha", bssid: "aa:aa:aa:aa:aa:02", rssi: -90 }),
        candidate({ ssid: "Beta", bssid: "aa:aa:aa:aa:aa:03", rssi: -70 }),
        candidate({ ssid: "Alpha", bssid: "aa:aa:aa:aa:aa:04", rssi: -90, security: 2 }),
      ],
      "aa:aa:aa:aa:aa:03",
      [],
      false,
    );
    expect(networks.map((n) => n.ssid)).toEqual(["Beta", "Zeta", "Alpha", "Alpha"]);
    expect(networks[0]?.isConnected).toBe(true);
    // 同名同信号的 Alpha 两条：WPA(2) 与 WPA2(4) 按原值升序。
    expect(networks.slice(2).map((n) => n.securityRaw)).toEqual([2, 4]);
  });

  it("BSSID 大小写不同也算已连接（正版 caseInsensitiveCompare）", () => {
    const networks = mergeNetworks(
      [candidate({ bssid: "AA:bb:CC:dd:EE:ff" })],
      "aa:BB:cc:DD:ee:FF",
      [],
      false,
    );
    expect(networks[0]?.isConnected).toBe(true);
  });

  it("无信号的条目排在最后但仍保留（RSSI 读不到按最小值）", () => {
    const networks = mergeNetworks(
      [candidate({ ssid: "Silent", rssi: null }), candidate({ ssid: "Loud", rssi: -55 })],
      null,
      [],
      false,
    );
    expect(networks.map((n) => n.ssid)).toEqual(["Loud", "Silent"]);
    expect(networks[1]?.signalValue).toBe(0.25);
  });
});

describe("groupNetworks", () => {
  const networks = mergeNetworks(
    [
      candidate({ ssid: "Phone", bssid: "aa:aa:aa:aa:aa:01", rssi: -40 }),
      candidate({ ssid: "Saved", bssid: "aa:aa:aa:aa:aa:02", rssi: -60 }),
      candidate({ ssid: "Stranger", bssid: "aa:aa:aa:aa:aa:03", rssi: -65 }),
    ],
    "aa:aa:aa:aa:aa:01",
    ["Saved"],
    true,
  );

  it("热点链路的已连接行进「个人热点」，不会再出现在已知节", () => {
    const groups = groupNetworks(networks, true);
    expect(groups.personalHotspot.map((n) => n.ssid)).toEqual(["Phone"]);
    expect(groups.known.map((n) => n.ssid)).toEqual(["Saved"]);
    expect(groups.other.map((n) => n.ssid)).toEqual(["Stranger"]);
  });

  it("非热点链路时已连接行留在「已知网络」", () => {
    const groups = groupNetworks(networks, false);
    expect(groups.personalHotspot).toHaveLength(0);
    expect(groups.known.map((n) => n.ssid)).toEqual(["Phone", "Saved"]);
  });
});

describe("wirelessRows", () => {
  it("折叠只露 9 行无线电读数", () => {
    const rows = wirelessRows(details(), false);
    expect(rows).toHaveLength(9);
    expect(rows.map((r) => r.label)).toEqual([
      "SSID",
      "BSSID",
      "频段",
      "信道",
      "信道宽度",
      "RSSI",
      "PHY 模式",
      "链路发送速率",
      "安全类型",
    ]);
  });

  it("展开补 8 行：噪声/信噪比/国家代码 + 共享 5 行", () => {
    const rows = wirelessRows(details(), true);
    expect(rows).toHaveLength(17);
    expect(rows.slice(9).map((r) => r.label)).toEqual([
      "噪声",
      "信噪比",
      "国家代码",
      "接口",
      "IPv4",
      "IPv6",
      "路由器",
      "DNS",
    ]);
  });

  it("数值格式与正版一致", () => {
    const rows = wirelessRows(details(), true);
    const byLabel = new Map(rows.map((r) => [r.label, r.value]));
    expect(byLabel.get("信道")).toBe("149");
    expect(byLabel.get("RSSI")).toBe("-47 dBm");
    expect(byLabel.get("噪声")).toBe("-95 dBm");
    expect(byLabel.get("信噪比")).toBe("48 dB");
    expect(byLabel.get("链路发送速率")).toBe("600.5 Mbps");
    expect(byLabel.get("DNS")).toBe("192.168.1.1, 8.8.8.8");
    expect(byLabel.get("安全类型")).toBe("WPA2");
  });

  it("只有地址类读数可复制（正版 isCopyable）", () => {
    const rows = wirelessRows(details(), true);
    expect(rows.filter((r) => r.copyable).map((r) => r.label)).toEqual([
      "BSSID",
      "IPv4",
      "IPv6",
      "路由器",
      "DNS",
    ]);
  });

  it("读不到的值写「不可用」，空字符串也算读不到", () => {
    const rows = wirelessRows(
      details({ ssid: "", channel: null, transmit_rate: null, ipv4_addresses: [] }),
      true,
    );
    const byLabel = new Map(rows.map((r) => [r.label, r.value]));
    expect(byLabel.get("SSID")).toBe("不可用");
    expect(byLabel.get("信道")).toBe("不可用");
    expect(byLabel.get("链路发送速率")).toBe("不可用");
    expect(byLabel.get("IPv4")).toBe("不可用");
  });
});

describe("signalToNoiseRatio", () => {
  it("两个读数都为负且 RSSI 不低于噪声才算", () => {
    expect(signalToNoiseRatio(-47, -95)).toBe(48);
    expect(signalToNoiseRatio(-95, -47)).toBeNull();
    expect(signalToNoiseRatio(0, -95)).toBeNull();
    expect(signalToNoiseRatio(null, -95)).toBeNull();
    expect(signalToNoiseRatio(-47, null)).toBeNull();
  });
});

describe("wifiMessage", () => {
  it("扫描中优先于其它一切", () => {
    expect(wifiMessage(scan(), true, true)).toEqual({ text: "正在扫描网络…", intent: "scanning" });
  });

  it("无网卡 / 关闭", () => {
    expect(wifiMessage(scan({ list_state: "noInterface" }), false, false)?.text).toBe(
      "没有可用的 Wi-Fi 网卡。",
    );
    expect(wifiMessage(scan({ list_state: "poweredOff" }), false, false)?.text).toBe("Wi-Fi 已关闭。");
  });

  it("扫描失败但从未申请过定位：这是授权入口，不是「无法扫描」", () => {
    expect(
      wifiMessage(scan({ list_state: "failed", name_access: "not_determined" }), false, false),
    ).toEqual({ text: "允许定位以显示 Wi-Fi 网络", intent: "request_name_access" });
    expect(wifiMessage(scan({ list_state: "failed" }), false, false)?.text).toBe(
      "无法扫描 Wi-Fi 网络。",
    );
  });

  it("空扫描 + 未决定：先说缺权限，不能说附近没网络", () => {
    expect(
      wifiMessage(scan({ name_access: "not_determined" }), false, false),
    ).toEqual({ text: "允许定位以显示 Wi-Fi 网络", intent: "request_name_access" });
  });

  it("空扫描 + 已拒绝：去定位服务设置", () => {
    expect(wifiMessage(scan({ name_access: "denied" }), false, false)).toEqual({
      text: "需要定位权限才能读取 Wi-Fi 名称和扫描结果。前往定位服务设置。",
      intent: "location_settings",
    });
  });

  it("已授权的空扫描才是「未发现附近网络」，有网络时无提示", () => {
    expect(wifiMessage(scan(), false, false)?.text).toBe("未发现附近网络。");
    expect(wifiMessage(scan(), true, false)).toBeNull();
  });
});

describe("wifiRowAction", () => {
  it("正版：除当前连接外，每一行都只把系统 Wi-Fi 面板打开", () => {
    const wpa2 = mergeNetworks([candidate({ security: 4 })], null, [], false)[0];
    const open = mergeNetworks([candidate({ ssid: "Cafe", security: 0 })], null, [], false)[0];
    expect(wifiRowAction(wpa2)).toBe("open_settings");
    expect(wifiRowAction(open)).toBe("open_settings");
  });

  it("已知网络也一样交回系统：面板自己不连接，所以没有「要密码」这一态", () => {
    const known = mergeNetworks([candidate({ security: 4 })], null, ["Home"], false)[0];
    expect(known.isKnown).toBe(true);
    expect(wifiRowAction(known)).toBe("open_settings");
    // 锁仍然照安全类型画（正版 `requiresPassword`），只是不代表面板要密码。
    expect(requiresPassword(known.security)).toBe(true);
  });

  it("已连接那一行是禁用的，只画勾", () => {
    const bssid = "aa:bb:cc:dd:ee:ff";
    const connected = mergeNetworks([candidate({ bssid })], bssid, [], false)[0];
    expect(connected.isConnected).toBe(true);
    expect(wifiRowAction(connected)).toBe("none");
  });
});
