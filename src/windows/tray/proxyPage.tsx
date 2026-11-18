/**
 * 代理（VPN）二级页与三级页。
 *
 * 外观与交互对齐 FlClash 的菜单栏下拉：接管开关在最上面，模式三档紧跟其后，
 * 再往下是策略组列表，点进去才是节点清单（带延迟、可测全部延迟）。判定与文案
 * 全部来自 `lib/proxyPanel.ts`，这里只负责画。
 */
import type { ReactNode } from "react";
import { useState } from "react";
import { cx, Spinner, Switch } from "../../components/ui";
import {
  PROXY_MODES,
  autoSwitchBrief,
  canEnableTakeover,
  delayText,
  delayTone,
  isStaleTakeover,
  proxyNotice,
  shortNodeName,
  splitNodes,
  takeoverLine,
  visibleGroups,
} from "../../lib/proxyPanel";
import type {
  AutoSwitchConfig,
  AutoSwitchState,
  ProxyMode,
  StatusBarProxyGroup,
  StatusBarProxyNode,
  StatusBarProxyPage,
} from "../../lib/types";
import {
  CheckGlyph,
  ChevronRightGlyph,
  RefreshGlyph,
} from "./glyphs";
import {
  BackRow,
  DetailActionButton,
  DetailRow,
  DIVIDER,
  IconAction,
} from "./rows";

const TONE_CLASS: Record<ReturnType<typeof delayTone>, string> = {
  fast: "text-[#34C759]",
  normal: "text-white/70",
  slow: "text-[#FF9F0A]",
  none: "text-white/35",
};

/** 面板里的三档模式控件；没有控制口时整排禁用，理由写在下面的说明里。 */
function ModeSegment({
  mode,
  disabled,
  busy,
  onChange,
}: {
  mode: ProxyMode | null;
  disabled: boolean;
  busy: boolean;
  onChange: (mode: ProxyMode) => void;
}) {
  return (
    <div className="flex items-center gap-[6px]" role="group" aria-label="代理模式">
      {PROXY_MODES.map((item) => {
        const active = mode === item.id;
        return (
          <button
            key={item.id}
            type="button"
            aria-pressed={active}
            disabled={disabled || busy}
            onClick={() => onChange(item.id)}
            className={cx(
              "h-[24px] flex-1 rounded-[6px] text-[12px]/[16px] outline-none transition-colors duration-100",
              active ? "bg-white/[0.16] text-white" : "bg-white/[0.06] text-white/60",
              !disabled && !active && "hover:bg-white/[0.1] hover:text-white",
              (disabled || busy) && "cursor-not-allowed opacity-40",
            )}
          >
            {item.label}
          </button>
        );
      })}
    </div>
  );
}

function SectionTitle({ children }: { children: ReactNode }) {
  return <p className="text-[11px]/[13px] font-semibold text-white/45">{children}</p>;
}

/**
 * 写操作失败的一句话。
 *
 * 只在失败时出现，点击即消失：接管失败的原因（端口没人监听、回读不一致已回滚）
 * 必须让用户读到，但不能变成常驻横幅——下一次成功之后它就是噪音。
 */
function NoticeRow({ text, onDismiss }: { text: string; onDismiss: () => void }) {
  return (
    <button
      type="button"
      onClick={onDismiss}
      className="w-full rounded-[6px] bg-[color:var(--danger-fill,#FF3B30)]/12 px-[8px] py-[5px] text-left text-[11px]/[14px] text-[#FF9F0A] outline-none"
    >
      {text}
    </button>
  );
}

/**
 * 自动换节点卡片（极简版）：只给结果——当前出口实测速度、节点短名、一句状态。
 * 诊断过程（复测倒计时、今日次数、候选对比、日志）留在后台，不摆给用户；
 * 阈值等参数默认收起，点「参数」才展开。
 */
function AutoSwitchSection({
  state,
  busy,
  onConfig,
  onSecret,
  onCheckNow,
}: {
  state: AutoSwitchState | null;
  busy: boolean;
  onConfig: (config: AutoSwitchConfig) => void;
  onSecret: (secret: string) => void;
  onCheckNow: () => void;
}) {
  const [secret, setSecret] = useState("");
  const [showParams, setShowParams] = useState(false);
  const cfg = state?.config ?? null;

  const commit = (patch: Partial<AutoSwitchConfig>) => {
    if (cfg) onConfig({ ...cfg, ...patch });
  };

  return (
    <div className="flex flex-col gap-[6px]">
      <div className="flex items-center gap-[8px]">
        <SectionTitle>自动换节点</SectionTitle>
        <span className="flex-1" />
        <Switch
          checked={cfg?.enabled ?? false}
          disabled={busy || !cfg}
          onChange={(enabled) => commit({ enabled })}
          label="自动换节点"
        />
      </div>

      <div className="flex items-baseline gap-[8px]">
        <span className="shrink-0 text-[22px]/[26px] font-semibold tabular-nums text-white">
          ↓ {state?.last_speed_kbps ?? "—"}
          <span className="ml-[2px] text-[12px]/[16px] font-normal text-white/50">KB/s</span>
        </span>
        <span className="min-w-0 flex-1 truncate text-[12px]/[16px] text-white/70">
          {state?.current_node ? shortNodeName(state.current_node) : ""}
        </span>
      </div>

      <div className="flex items-center gap-[10px]">
        <span className="text-[11px]/[14px] text-white/55">{autoSwitchBrief(state)}</span>
        <span className="flex-1" />
        <button
          type="button"
          disabled={busy || !cfg}
          onClick={onCheckNow}
          className="flex items-center gap-[6px] text-[12px]/[16px] text-white/85 outline-none hover:text-white disabled:opacity-40"
        >
          {busy ? <Spinner size={12} /> : null}
          测速
        </button>
        <button
          type="button"
          onClick={() => setShowParams((v) => !v)}
          className="text-[11px]/[14px] text-white/45 outline-none hover:text-white/70"
        >
          {showParams ? "收起" : "参数"}
        </button>
      </div>

      {state?.backend === "router" && cfg && !cfg.has_router_secret && (
        <div className="flex items-center gap-[6px]">
          <input
            type="password"
            value={secret}
            disabled={busy}
            onChange={(e) => setSecret(e.target.value)}
            placeholder="路由器 API 密钥（OpenClash 外部控制）"
            className="h-[24px] min-w-0 flex-1 rounded-[6px] bg-white/[0.06] px-[8px] text-[11px]/[16px] text-white/85 outline-none placeholder:text-white/30"
          />
          <button
            type="button"
            disabled={busy || secret.trim().length === 0}
            onClick={() => {
              onSecret(secret.trim());
              setSecret("");
            }}
            className="h-[24px] shrink-0 rounded-[6px] bg-white/[0.12] px-[10px] text-[11px]/[16px] text-white/85 outline-none hover:bg-white/[0.18] disabled:opacity-40"
          >
            保存
          </button>
        </div>
      )}

      {showParams && cfg && (
        <div className="flex flex-wrap items-center gap-x-[10px] gap-y-[4px] text-[11px]/[14px] text-white/55">
          <label className="flex items-center gap-[4px]">
            阈值
            <input
              key={`min-${cfg.min_speed_kbps}`}
              type="number"
              defaultValue={cfg.min_speed_kbps}
              disabled={busy}
              onBlur={(e) => commit({ min_speed_kbps: Number(e.target.value) || 300 })}
              className="h-[22px] w-[56px] rounded-[5px] bg-white/[0.06] px-[6px] tabular-nums text-white/85 outline-none"
            />
            KB/s
          </label>
          <label className="flex items-center gap-[4px]">
            巡检
            <input
              key={`interval-${cfg.probe_interval_secs}`}
              type="number"
              defaultValue={cfg.probe_interval_secs}
              disabled={busy}
              onBlur={(e) => commit({ probe_interval_secs: Number(e.target.value) || 30 })}
              className="h-[22px] w-[48px] rounded-[5px] bg-white/[0.06] px-[6px] tabular-nums text-white/85 outline-none"
            />
            秒
          </label>
          <label className="flex items-center gap-[4px]">
            每日上限
            <input
              key={`cap-${cfg.daily_cap}`}
              type="number"
              defaultValue={cfg.daily_cap}
              disabled={busy}
              onBlur={(e) => commit({ daily_cap: Number(e.target.value) || 12 })}
              className="h-[22px] w-[48px] rounded-[5px] bg-white/[0.06] px-[6px] tabular-nums text-white/85 outline-none"
            />
            次
          </label>
        </div>
      )}
    </div>
  );
}

/** 概览页。 */
export function ProxyOverviewPage({
  page,
  loading,
  busy,
  notice,
  autoSwitch,
  autoSwitchBusy,
  onBack,
  onRefresh,
  onTakeover,
  onMode,
  onOpenGroup,
  onOpenApp,
  onDismissNotice,
  onAutoSwitchConfig,
  onRouterSecret,
  onCheckAutoSwitch,
}: {
  page: StatusBarProxyPage | null;
  loading: boolean;
  /** 某一次写操作正在路上：整页控件禁用，避免连点把系统代理写成半截状态。 */
  busy: boolean;
  /** 上一次写操作的失败原因；成功时为 null。 */
  notice: string | null;
  autoSwitch: AutoSwitchState | null;
  autoSwitchBusy: boolean;
  onBack: () => void;
  onRefresh: () => void;
  onTakeover: (enabled: boolean) => void;
  onMode: (mode: ProxyMode) => void;
  onOpenGroup: (group: StatusBarProxyGroup) => void;
  onOpenApp: () => void;
  onDismissNotice: () => void;
  onAutoSwitchConfig: (config: AutoSwitchConfig) => void;
  onRouterSecret: (secret: string) => void;
  onCheckAutoSwitch: () => void;
}) {
  const [showMore, setShowMore] = useState(false);
  const explanation = proxyNotice(page);
  const groups = visibleGroups(page);
  const takeover = page?.takeover ?? false;
  const stale = isStaleTakeover(page);
  const hasController = page?.source === "clash";

  return (
    <div className="flex flex-col gap-[12px]">
      <div className="flex items-center gap-[8px]">
        <span className="min-w-0 flex-1">
          <BackRow title="VPN / 代理" onBack={onBack} />
        </span>
        <IconAction label="刷新" disabled={loading} onClick={onRefresh}>
          {loading ? <Spinner size={12} /> : <RefreshGlyph />}
        </IconAction>
      </div>

      {notice && <NoticeRow text={notice} onDismiss={onDismissNotice} />}

      <div className="flex items-center gap-[10px]">
        <span className="min-w-0 flex-1">
          <span className="block truncate text-[13px]/[17px] text-white/85">流量接管</span>
          <span
            className={cx(
              "block truncate text-[10px]/[13px]",
              stale ? "text-[#ff9f0a]" : "text-white/45",
            )}
          >
            {takeoverLine(page)}
          </span>
        </span>
        <Switch
          checked={takeover}
          disabled={busy || (!takeover && !canEnableTakeover(page))}
          onChange={onTakeover}
          label="流量接管"
        />
      </div>

      <AutoSwitchSection
        state={autoSwitch}
        busy={autoSwitchBusy}
        onConfig={onAutoSwitchConfig}
        onSecret={onRouterSecret}
        onCheckNow={onCheckAutoSwitch}
      />

      <button
        type="button"
        onClick={() => setShowMore((v) => !v)}
        className="self-start text-[11px]/[14px] text-white/45 outline-none hover:text-white/70"
      >
        {showMore ? "收起更多" : "更多设置"}
      </button>

      {showMore && (
        <div className="flex flex-col gap-[12px]">
          <div className="flex flex-col gap-[6px]">
            <SectionTitle>模式</SectionTitle>
            <ModeSegment
              mode={page?.mode ?? null}
              disabled={!hasController}
              busy={busy}
              onChange={onMode}
            />
          </div>

          {groups.length > 0 && (
            <div className="flex max-h-[150px] flex-col gap-[5px] overflow-y-auto">
              <SectionTitle>策略组</SectionTitle>
              {groups.map((group) => (
                <button
                  key={group.name}
                  type="button"
                  disabled={busy}
                  onClick={() => onOpenGroup(group)}
                  className="flex items-center gap-[8px] text-left outline-none disabled:opacity-40"
                >
                  <span className="min-w-0 flex-1 truncate text-[12px]/[16px] text-white/85">
                    {group.name}
                  </span>
                  <span className="max-w-[45%] shrink-0 truncate text-right text-[11px]/[14px] text-white/50">
                    {group.current ?? "未选择"}
                  </span>
                  {group.delay !== null && group.delay > 0 && (
                    <span className={cx("shrink-0 text-[11px]/[14px] tabular-nums", TONE_CLASS[delayTone(group.delay)])}>
                      {delayText(group.delay)}
                    </span>
                  )}
                  <span className="shrink-0 text-white/35" aria-hidden>
                    <ChevronRightGlyph />
                  </span>
                </button>
              ))}
            </div>
          )}

          {explanation && <p className="text-[11px]/[14px] text-white/55">{explanation}</p>}

          <div className={DIVIDER} />

          <DetailRow label="虚拟网卡" value={page?.tun_enabled === null || page?.tun_enabled === undefined ? "未知" : page.tun_enabled ? "已启用" : "未启用"} />
          <DetailRow label="控制口" value={page?.controller || "未开启"} copyable={Boolean(page?.controller)} />
          <p className="text-[11px]/[14px] text-white/45">接管只改系统代理，不动路由和 DNS。</p>
          <DetailActionButton label="打开 FlClash" onClick={onOpenApp} />
        </div>
      )}
    </div>
  );
}

/** 三级页：一个策略组里的节点。 */
export function ProxyNodesPage({
  group,
  nodes,
  testing,
  busy,
  notice,
  onBack,
  onSelect,
  onTestLatency,
  onDismissNotice,
}: {
  group: StatusBarProxyGroup;
  /** 测完一轮后的最新节点；没测过退回读数里的节点。 */
  nodes: StatusBarProxyNode[] | null;
  testing: boolean;
  busy: boolean;
  notice: string | null;
  onBack: () => void;
  onSelect: (node: string) => void;
  onTestLatency: () => void;
  onDismissNotice: () => void;
}) {
  const list = nodes ?? group.nodes;
  const { infos, selectable } = splitNodes(list);

  return (
    <div className="flex flex-col gap-[12px]">
      <div className="flex items-center gap-[8px]">
        <span className="min-w-0 flex-1">
          <BackRow title={group.name} onBack={onBack} />
        </span>
        <IconAction label="测试全部延迟" disabled={testing || busy} onClick={onTestLatency}>
          {testing ? <Spinner size={12} /> : <RefreshGlyph />}
        </IconAction>
      </div>

      {notice && <NoticeRow text={notice} onDismiss={onDismissNotice} />}

      {infos.length > 0 && (
        <div className="flex flex-col gap-[3px]">
          {infos.map((info) => (
            <p key={info.name} className="truncate text-[11px]/[14px] text-white/45">
              {info.name}
            </p>
          ))}
        </div>
      )}

      <div className="flex max-h-[260px] flex-col gap-[4px] overflow-y-auto">
        {testing && (
          <p className="flex items-center gap-[8px] text-[11px]/[14px] text-white/55">
            <Spinner size={12} />
            正在测试 {selectable.length} 个节点…
          </p>
        )}
        {selectable.map((node) => {
          const selected = node.name === group.current;
          return (
            <button
              key={node.name}
              type="button"
              disabled={busy || testing}
              onClick={() => onSelect(node.name)}
              className="flex items-center gap-[8px] text-left outline-none disabled:opacity-40"
            >
              <span className="grid h-[14px] w-[14px] shrink-0 place-items-center text-white/85" aria-hidden>
                {selected && <CheckGlyph />}
              </span>
              <span
                className={cx(
                  "min-w-0 flex-1 truncate text-[12px]/[16px]",
                  selected ? "text-white" : "text-white/80",
                )}
              >
                {node.name}
              </span>
              <span className={cx("shrink-0 text-[11px]/[14px] tabular-nums", TONE_CLASS[delayTone(node.delay)])}>
                {delayText(node.delay)}
              </span>
            </button>
          );
        })}
        {selectable.length === 0 && !testing && (
          <p className="text-[11px]/[14px] text-white/45">这个组里没有可选择的节点。</p>
        )}
      </div>
    </div>
  );
}
