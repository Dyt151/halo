import { describe, expect, it } from "vitest";
import {
  autoSwitchBrief,
  canEnableTakeover,
  delayText,
  delayTone,
  formatBytesPerSecond,
  isInfoNode,
  isStaleTakeover,
  modeLabel,
  outcomeNotice,
  proxyNotice,
  proxySubtitle,
  proxyTitle,
  shortNodeName,
  speedText,
  splitNodes,
  takeoverLine,
  visibleGroups,
} from "./proxyPanel";
import type {
  AutoSwitchState,
  StatusBarProxyGroup,
  StatusBarProxyNode,
  StatusBarProxyOutcome,
  StatusBarProxyPage,
} from "./types";

function page(overrides: Partial<StatusBarProxyPage> = {}): StatusBarProxyPage {
  return {
    available: true,
    source: "clash",
    core_running: true,
    mixed_port: 7890,
    controller: "127.0.0.1:9097",
    takeover: false,
    takeover_web: false,
    takeover_https: false,
    takeover_socks: false,
    proxy_port: 0,
    takeover_alive: true,
    service: "Wi-Fi",
    mode: "rule",
    groups: [],
    tun_enabled: false,
    up_bps: null,
    down_bps: null,
    speed_scope: "proxy",
    test_url: "http://www.gstatic.com/generate_204",
    message: "",
    ...overrides,
  };
}

function node(name: string, overrides: Partial<StatusBarProxyNode> = {}): StatusBarProxyNode {
  return { name, kind: "ss", delay: null, ...overrides };
}

function group(name: string, nodes: StatusBarProxyNode[], overrides: Partial<StatusBarProxyGroup> = {}): StatusBarProxyGroup {
  return { name, kind: "select", current: nodes[0]?.name ?? null, hidden: false, delay: null, nodes, ...overrides };
}

describe("formatBytesPerSecond", () => {
  it("读不到和零都按 0 KB/s 显示", () => {
    expect(formatBytesPerSecond(null)).toBe("0 KB/s");
    expect(formatBytesPerSecond(0)).toBe("0 KB/s");
    expect(formatBytesPerSecond(Number.NaN)).toBe("0 KB/s");
  });

  it("1024 进制、最多一位小数、整数不留小数", () => {
    expect(formatBytesPerSecond(512)).toBe("512 B/s");
    expect(formatBytesPerSecond(1024)).toBe("1 KB/s");
    expect(formatBytesPerSecond(1536)).toBe("1.5 KB/s");
    expect(formatBytesPerSecond(1_048_576)).toBe("1 MB/s");
    expect(formatBytesPerSecond(1_572_864)).toBe("1.5 MB/s");
    expect(formatBytesPerSecond(1_073_741_824)).toBe("1 GB/s");
    // 100 以上不再带小数：大数字的小数位没有意义，还会让行宽抖。
    expect(formatBytesPerSecond(204_800)).toBe("200 KB/s");
  });
});

describe("speedText", () => {
  it("两个方向都读不到时整行不显示", () => {
    expect(speedText(null, null)).toBeNull();
    expect(speedText(null, 2048)).toBe("↑ 0 KB/s ↓ 2 KB/s");
  });
});

describe("proxyTitle / proxySubtitle", () => {
  it("核心在跑但没接管说的是直连，不是已连接", () => {
    expect(proxyTitle(page({ takeover: false }))).toBe("直连（未接管）");
    expect(proxyTitle(page({ takeover: true }))).toBe("流量已接管");
    expect(proxyTitle(page({ core_running: false }))).toBe("代理核心未启动");
    expect(proxyTitle(null)).toBe("代理");
  });

  it("副标题把有读数的项目串起来，没读数退回端口而不是留空", () => {
    expect(proxySubtitle(page(), page({ up_bps: 1024, down_bps: 2048 }))).toBe("规则模式 · ↑ 1 KB/s ↓ 2 KB/s");
    expect(proxySubtitle(page({ mode: null }), null)).toBe("本地端口 7890");
    expect(proxySubtitle(page({ core_running: false, takeover: false }), null)).toBe("没找到本地代理核心");
    expect(proxySubtitle(null, null)).toBe("正在读取…");
  });

  it("只有真的连上控制口才报模式", () => {
    expect(proxySubtitle(page({ source: "probe", takeover: true }), null)).toBe("本地端口 7890");
  });
});

describe("核心已退但接管还挂着", () => {
  // 系统代理指向一个没人监听的回环端口 = 浏览器全挂，是唯一真会把网络搞断的组合。
  const stale = page({
    takeover: true,
    proxy_port: 7890,
    takeover_alive: false,
    core_running: false,
    source: "none",
  });

  it("标题说失效而不是已接管", () => {
    expect(isStaleTakeover(stale)).toBe(true);
    expect(proxyTitle(stale)).toBe("接管已失效");
    expect(isStaleTakeover(page({ takeover: false, core_running: false }))).toBe(false);
  });

  it("副标题不报网速，直接指路去关闭", () => {
    expect(proxySubtitle(stale, page({ up_bps: 9000, down_bps: 9000 }))).toBe(
      "接管端口没人监听，点进来关闭",
    );
  });

  it("接管说明用系统代理实际端口，而不是配置里的混合端口", () => {
    expect(takeoverLine(page({ takeover: true, proxy_port: 7891, mixed_port: 7890 }))).toBe(
      "系统代理已指向 127.0.0.1:7891",
    );
    expect(takeoverLine(stale)).toBe("127.0.0.1:7890 上已经没人监听了");
    expect(takeoverLine(page())).toBe("系统代理直连");
    expect(takeoverLine(null)).toBe("系统代理直连");
  });

  it("核心换了端口而系统代理还指着旧的，一样算失效", () => {
    // 后端只探系统代理实际指向的端口：core_running 仍然是 true（新端口有人听），
    // 但用户的流量去的是旧端口，那里已经没人监听。
    const moved = page({
      takeover: true,
      proxy_port: 7890,
      takeover_alive: false,
      mixed_port: 7891,
      core_running: true,
    });
    expect(isStaleTakeover(moved)).toBe(true);
    expect(proxyTitle(moved)).toBe("接管已失效");
    expect(takeoverLine(moved)).toBe("127.0.0.1:7890 上已经没人监听了");
  });

  it("失效时后端那句救援说明优先显示", () => {
    expect(proxyNotice({ ...stale, message: "系统代理还指着 127.0.0.1:7890" })).toContain(
      "127.0.0.1:7890",
    );
  });
});

describe("canEnableTakeover", () => {
  it("核心没在监听时不给打开", () => {
    expect(canEnableTakeover(page())).toBe(true);
    expect(canEnableTakeover(page({ core_running: false }))).toBe(false);
    expect(canEnableTakeover(null)).toBe(false);
  });
});

describe("proxyNotice", () => {
  it("没控制口时解释为什么模式与节点是灰的", () => {
    expect(proxyNotice(page({ source: "probe" }))).toContain("外部控制");
    expect(proxyNotice(page())).toBeNull();
  });

  it("后端已经写好的说明优先于前端模板", () => {
    expect(proxyNotice(page({ message: "本机 7890 端口上没有代理核心" }))).toBe("本机 7890 端口上没有代理核心");
  });

  it("找不到核心时给出下一步动作", () => {
    expect(proxyNotice(page({ available: false }))).toContain("FlClash");
  });
});

describe("visibleGroups", () => {
  it("空的与隐藏的组都不占一格", () => {
    const pageValue = page({
      groups: [
        group("有节点", [node("a")]),
        group("空组", []),
        group("隐藏组", [node("b")], { hidden: true }),
      ],
    });
    expect(visibleGroups(pageValue).map((g) => g.name)).toEqual(["有节点"]);
    expect(visibleGroups(null)).toEqual([]);
  });

  it("保持后端给的顺序：订阅里第一个组通常就是主策略", () => {
    const pageValue = page({ groups: [group("🚀 出口", [node("a")]), group("♻️ 自动", [node("b")])] });
    expect(visibleGroups(pageValue).map((g) => g.name)).toEqual(["🚀 出口", "♻️ 自动"]);
  });
});

describe("isInfoNode / splitNodes", () => {
  it("订阅商塞进策略组的流量信息行不能被选", () => {
    expect(isInfoNode("剩余流量：12.34 GB")).toBe(true);
    expect(isInfoNode("套餐到期时间: 2026-12-31")).toBe(true);
    expect(isInfoNode("距离下次重置 3 天")).toBe(true);
    expect(isInfoNode("🇭🇰 香港 01 | BGP")).toBe(false);
  });

  it("信息行单独归到 infos，节点留在 selectable 并保持原序", () => {
    const { infos, selectable } = splitNodes([
      node("剩余流量：1 GB"),
      node("香港 01"),
      node("日本 02"),
      node("官网"),
    ]);
    expect(infos.map((n) => n.name)).toEqual(["剩余流量：1 GB", "官网"]);
    expect(selectable.map((n) => n.name)).toEqual(["香港 01", "日本 02"]);
  });
});

describe("delayTone / delayText", () => {
  it("阈值分档：100 以内绿、300 以内白、更慢或超时红", () => {
    expect(delayTone(88)).toBe("fast");
    expect(delayTone(182)).toBe("normal");
    expect(delayTone(300)).toBe("slow");
    expect(delayTone(null)).toBe("none");
    // 后端把超时压成 null，但 0 也要按「没测出来」处理，不能画成最快的绿。
    expect(delayTone(0)).toBe("none");
  });

  it("没有延迟就写超时", () => {
    expect(delayText(182)).toBe("182ms");
    expect(delayText(null)).toBe("超时");
    expect(delayText(0)).toBe("超时");
  });
});

describe("outcomeNotice", () => {
  it("成功不打扰，失败必须有话", () => {
    expect(outcomeNotice(null)).toBeNull();
    expect(outcomeNotice({ code: "ok", message: "" } as StatusBarProxyOutcome)).toBeNull();
    expect(outcomeNotice({ code: "failed", message: "已回滚到直连" } as StatusBarProxyOutcome)).toBe("已回滚到直连");
    expect(outcomeNotice({ code: "failed", message: "" } as StatusBarProxyOutcome)).toBe("操作没有生效");
  });
});

describe("modeLabel", () => {
  it("三档名字与分段控件一致", () => {
    expect(modeLabel("rule")).toBe("规则");
    expect(modeLabel("global")).toBe("全局");
    expect(modeLabel("direct")).toBe("直连");
    expect(modeLabel(null)).toBe("—");
  });
});

function autoState(overrides: Partial<AutoSwitchState> = {}): AutoSwitchState {
  return {
    config: {
      enabled: true,
      router_group: "良心云",
      local_group: "良心云",
      min_speed_kbps: 300,
      probe_interval_secs: 30,
      daily_cap: 12,
      has_router_secret: true,
    },
    backend: "router",
    backend_label: "路由器 OpenClash（M8）",
    current_node: "🇸🇬新加坡高速02|BGP|CTCUCM",
    last_speed_kbps: 523,
    last_probe_at: null,
    last_switch_at: null,
    last_switch_from: "",
    last_switch_to: "",
    switches_today: 0,
    consecutive_bad: 0,
    status: "正常：523KB/s",
    log: [],
    ...overrides,
  };
}

describe("shortNodeName", () => {
  it("只留国旗+地区名，去掉线路尾巴", () => {
    expect(shortNodeName("🇸🇬新加坡高速02|BGP|CTCUCM")).toBe("🇸🇬新加坡高速02");
    expect(shortNodeName("🇯🇵日本专线01")).toBe("🇯🇵日本专线01");
    expect(shortNodeName("")).toBe("");
  });
});

describe("autoSwitchBrief", () => {
  it("把诊断长句压成一眼能懂的词", () => {
    expect(autoSwitchBrief(null)).toBe("正在读取…");
    expect(autoSwitchBrief(autoState({ backend: "none" }))).toBe("未连接");
    expect(autoSwitchBrief(autoState({ status: "正常：523KB/s" }))).toBe("已是最快");
    expect(autoSwitchBrief(autoState({ status: "候选并不比当前快，保持当前节点" }))).toBe("已是最快");
    expect(autoSwitchBrief(autoState({ status: "136KB/s 不达标，10 秒后复测确认" }))).toBe("正在提速…");
    expect(autoSwitchBrief(autoState({ status: "已自动切换到 🇸🇬新加坡高速02（523KB/s）" }))).toBe("已切换");
    expect(autoSwitchBrief(autoState({ status: "今日切换已达上限（12 次），保持当前节点" }))).toBe("今日已达上限");
    expect(autoSwitchBrief(autoState({ status: "所有节点都不通，保持当前节点" }))).toBe("节点都不通");
  });
});
