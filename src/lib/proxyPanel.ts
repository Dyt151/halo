/**
 * 代理（VPN）二级页的纯逻辑。
 *
 * 放 lib 里是为了能离线断言：三档模式的叫法、没开控制口时该说哪句话、哪些成员
 * 其实是订阅商塞进策略组的流量信息行、接管开关什么时候该按不动——这些直接决定
 * 页面像不像 FlClash，也决定用户会不会误以为「接管 = VPN 全局接管」。
 *
 * 口径约定：数值阈值和文案对齐 FlClash（它的菜单栏下拉用 `↑/↓ + 单位`），单位用
 * 1024 进制但写作 KB/s、MB/s——和系统活动监视器不同，这里跟的是代理软件的习惯。
 */
import type {
  AutoSwitchState,
  ProxyMode,
  StatusBarProxyGroup,
  StatusBarProxyNode,
  StatusBarProxyOutcome,
  StatusBarProxyPage,
} from "./types";

/** 三档模式：顺序就是页面上从左到右的顺序，和 FlClash 的分段控件一致。 */
export const PROXY_MODES: readonly { id: ProxyMode; label: string }[] = [
  { id: "rule", label: "规则" },
  { id: "global", label: "全局" },
  { id: "direct", label: "直连" },
];

export function modeLabel(mode: ProxyMode | null): string {
  return PROXY_MODES.find((item) => item.id === mode)?.label ?? "—";
}

/** 字节每秒 → 「1.2 MB/s」；1024 进制，最多一位小数，null/0 都写作 0 KB/s。 */
export function formatBytesPerSecond(bps: number | null): string {
  if (bps === null || !Number.isFinite(bps) || bps <= 0) return "0 KB/s";
  const units = ["B/s", "KB/s", "MB/s", "GB/s"];
  let value = bps;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  // 整数不留小数：1.0 KB/s 读起来像没测准。
  const text = value >= 100 || Number.isInteger(value) ? value.toFixed(0) : value.toFixed(1);
  return `${text} ${units[unit]}`;
}

/** 上行/下行一行：`↑ 1.2 MB/s ↓ 34 KB/s`；两个都读不到时整行不显示。 */
export function speedText(up: number | null, down: number | null): string | null {
  if (up === null && down === null) return null;
  return `↑ ${formatBytesPerSecond(up)} ↓ ${formatBytesPerSecond(down)}`;
}

/**
 * 摘要行标题（面板第一页那格）。
 *
 * 「已接管」而不是「已连接」：这一格说的是系统流量有没有指向代理端口，不是核心
 * 在不在跑——核心在跑但没接管时，用户的流量其实一分都没走代理。
 */
export function proxyTitle(page: StatusBarProxyPage | null): string {
  if (!page) return "代理";
  // 接管还挂在没人听的端口上：这格必须说「失效」而不是「已接管」，
  // 否则用户看到的是一句正常状态，浏览器却在等一个没人监听的端口。
  if (isStaleTakeover(page)) return "接管已失效";
  if (page.takeover) return "流量已接管";
  return page.core_running ? "直连（未接管）" : "代理核心未启动";
}

/**
 * 接管还挂着，但系统代理指向的那个端口上已经没人监听了。
 *
 * 这是整套读数里唯一会真正把用户网断掉的组合：所有 HTTP/HTTPS 都会失败。核心退出、
 * 或者核心换了端口而系统代理还指着旧的，都会走到这里——所以判断只看后端回传的
 * `takeover_alive`，不能拿 `core_running` 代替。页面必须把它当成一等状态画出来，
 * 并保留关闭入口。
 */
export function isStaleTakeover(page: StatusBarProxyPage | null): boolean {
  return page !== null && page.takeover && !page.takeover_alive;
}

/** 开关下面那句说明：写系统代理实际指向的端口，而不是配置里读到的混合端口。 */
export function takeoverLine(page: StatusBarProxyPage | null): string {
  if (!page || !page.takeover) return "系统代理直连";
  const port = page.proxy_port > 0 ? page.proxy_port : page.mixed_port;
  if (isStaleTakeover(page)) return `127.0.0.1:${port} 上已经没人监听了`;
  return `系统代理已指向 127.0.0.1:${port}`;
}

/** 摘要行副标题：接管状态 + 模式 + 网速；读不到的项直接省掉，不留空括号。 */
export function proxySubtitle(
  page: StatusBarProxyPage | null,
  speed: { up_bps: number | null; down_bps: number | null } | null,
): string {
  if (!page) return "正在读取…";
  // 接管挂着而端口没人听：先说要怎么把它关掉，别报模式或端口读数。
  if (isStaleTakeover(page)) return "接管端口没人监听，点进来关闭";
  // 核心没在监听时不要写「规则模式 · 本地端口 7890」这种自相矛盾的读数。
  if (!page.core_running) return "没找到本地代理核心";
  const parts: string[] = [];
  if (page.mode && page.source === "clash") parts.push(`${modeLabel(page.mode)}模式`);
  const text = speedText(speed?.up_bps ?? page.up_bps, speed?.down_bps ?? page.down_bps);
  if (text) parts.push(text);
  if (parts.length > 0) return parts.join(" · ");
  return `本地端口 ${page.mixed_port}`;
}

/**
 * 接管开关能不能按。
 *
 * 关永远允许（关掉就是直连，风险方向是「不通」而不是「断网」）；开只有在核心
 * 真的在监听时才允许——后端同样会拒绝，前端提前禁用以免用户按下去才发现没动。
 */
export function canEnableTakeover(page: StatusBarProxyPage | null): boolean {
  return page !== null && page.core_running;
}

/** 页面顶部的说明句：只在有实质内容时返回，正常状态返回 null 不占高度。 */
export function proxyNotice(page: StatusBarProxyPage | null): string | null {
  if (!page) return null;
  if (!page.available) return "没找到本地代理核心。启动 FlClash 后再刷新。";
  if (page.message) return page.message;
  if (page.source === "probe") {
    return "只探到了本地端口：FlClash 没开外部控制，模式、节点和延迟改不了。";
  }
  return null;
}

/**
 * 策略组 → 页面上要显示的那几个。
 *
 * 后端已经滤掉 hidden 与 DIRECT/REJECT 这类内置项，这里再滤掉「没有成员的组」：
 * 空的 select 组在 Clash 里是订阅商的占位，点进去只会是一篇空白三级页。
 */
export function visibleGroups(page: StatusBarProxyPage | null): StatusBarProxyGroup[] {
  if (!page) return [];
  return page.groups.filter((group) => !group.hidden && group.nodes.length > 0);
}

/**
 * 订阅商塞进策略组的流量信息行。
 *
 * 这些「节点」根本不能连（mihomo 把它们做成 name-only 成员，只为在菜单里显示
 * 剩余流量），点过去只会得到一个失败的延迟。按关键词识别后画成一行灰字。
 */
const INFO_KEYWORDS = ["流量", "到期", "过期", "重置", "套餐", "官网", "续费", "客服", "公告", "使用"];

export function isInfoNode(name: string): boolean {
  return INFO_KEYWORDS.some((keyword) => name.includes(keyword));
}

/** 三级页要列的成员：信息行不参与选择，单独放前面当说明。 */
export function splitNodes(nodes: StatusBarProxyNode[]): {
  infos: StatusBarProxyNode[];
  selectable: StatusBarProxyNode[];
} {
  const infos: StatusBarProxyNode[] = [];
  const selectable: StatusBarProxyNode[] = [];
  for (const node of nodes) {
    (isInfoNode(node.name) ? infos : selectable).push(node);
  }
  return { infos, selectable };
}

/**
 * 延迟档位：绿 <100ms、白 <300ms、红更慢或超时。
 *
 * 阈值取自 FlClash 的观感：它把「超时」画成红、>300 画成橙、其余正常。这里只有
 * 三档颜色可用，超时和慢一起归 `slow`，因为对用户来说都是「别用这个」。
 */
export function delayTone(delay: number | null): "fast" | "normal" | "slow" | "none" {
  if (delay === null || delay <= 0) return "none";
  if (delay < 100) return "fast";
  if (delay < 300) return "normal";
  return "slow";
}

export function delayText(delay: number | null): string {
  if (delay === null || delay <= 0) return "超时";
  return `${delay}ms`;
}

/** 写操作结果 → 要显示的一句话；成功返回 null（成功不需要打扰）。 */
export function outcomeNotice(outcome: StatusBarProxyOutcome | null): string | null {
  if (!outcome || outcome.code === "ok") return null;
  return outcome.message || "操作没有生效";
}

/** 节点名只留第一段（国旗 + 地区名），去掉 `|BGP|流媒体` 这类尾巴。 */
export function shortNodeName(name: string): string {
  const head = name.split("|")[0].trim();
  return head || name;
}

/**
 * 自动换节点的一句话状态：把后端的诊断长句压成一眼能懂的词。
 * 过程细节（复测倒计时、今日次数、候选对比）留在后台日志里，不摆给用户。
 */
export function autoSwitchBrief(state: AutoSwitchState | null): string {
  if (!state) return "正在读取…";
  if (state.backend === "none") return "未连接";
  const s = state.status;
  if (s.startsWith("已自动切换")) return "已切换";
  if (s.includes("上限")) return "今日已达上限";
  if (s.includes("都不通")) return "节点都不通";
  if (s.includes("不达标") || s.includes("低于阈值")) return "正在提速…";
  if (s.includes("保持当前") || s.startsWith("正常") || s === "") return "已是最快";
  return "监测中";
}
