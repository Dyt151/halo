//! 电池、输入法与 Wi-Fi 二级详情页。

import { useState } from "react";
import type { ReactNode } from "react";
import { InputSourceMarker } from "../../components/InputSourceMarker";
import { api } from "../../lib/ipc";
import { showsActionArea } from "../../lib/chargeToFull";
import {
  groupNetworks,
  mergeNetworks,
  requiresPassword,
  wifiBars,
  wifiMessage,
  wifiRowAction,
  wirelessRows,
} from "../../lib/wifiPanel";
import type { WifiNetwork } from "../../lib/wifiPanel";
import { Spinner, Switch, cx } from "../../components/ui";
import type {
  StatusBarBattery,
  StatusBarBatteryDetails,
  StatusBarInputSource,
  StatusBarWifiScan,
} from "../../lib/types";
import type { ChargeToFullPresentation } from "../../lib/chargeToFull";
import {
  BatteryGlyph,
  CheckGlyph,
  ChevronUpGlyph,
  HotspotGlyph,
  InfoGlyph,
  KeyboardGlyph,
  LaptopGlyph,
  LockGlyph,
  RefreshGlyph,
  WifiBarsGlyph,
  WifiUnknownGlyph,
} from "./glyphs";
import {
  BackRow,
  ChargeToFullAction,
  DIVIDER,
  DetailActionButton,
  DetailRow,
  IconAction,
  PowerCard,
} from "./rows";
import {
  cardWattsText,
  fixed,
  remainingTimeText,
  wattsText,
} from "./format";

/**
 * 电池详情页（正版 `BatteryDetailsView`）。
 *
 * 流向图只在接电时出现；有图时「整机功耗（估算）」「充电（估算）」两行被过滤，
 * 只留「整机读取时间」——这是正版 `detailRows` 的判序，照抄。
 */
export function BatteryDetailPage({
  battery,
  details,
  loading,
  chargeToFull,
  onChargeToFull,
  onBack,
}: {
  battery: StatusBarBattery;
  details: StatusBarBatteryDetails | null;
  loading: boolean;
  chargeToFull: ChargeToFullPresentation;
  onChargeToFull: () => void;
  onBack: () => void;
}) {
  const plugged = battery.plugged;
  const sampledCharging = details?.battery_watts != null && details.battery_watts > 0
    ? details.battery_watts
    : null;
  const chargingWatts = sampledCharging ?? (battery.charging ? null : 0);
  const discharge =
    details?.battery_watts != null && details.battery_watts < 0 ? -details.battery_watts : null;

  const rows: { label: string; value: string }[] = [];
  if (!plugged) {
    rows.push({ label: "电池放电功率（估算）", value: wattsText(discharge) });
    if (details?.sampled_at) rows.push({ label: "电池采样时间", value: details.sampled_at });
  } else if (details?.sampled_at) {
    rows.push({ label: "整机读取时间", value: details.sampled_at });
  }
  if (details && details.voltage_volts !== null && details.current_amps !== null) {
    rows.push({ label: "电池电压", value: `${fixed(details.voltage_volts, 2)} V` });
    rows.push({ label: "电池电流", value: `${fixed(details.current_amps, 2)} A` });
  }
  if (details?.adapter_watts != null) {
    rows.push({ label: "适配器额定功率", value: `${details.adapter_watts} W` });
  }
  if (!plugged) rows.push({ label: "剩余时间", value: remainingTimeText(battery.minutes_to_empty) });
  if (details?.cycle_count != null) {
    rows.push({ label: "循环次数", value: String(details.cycle_count) });
  }
  rows.push({ label: "低电量模式", value: battery.low_power ? "开启" : "关闭" });

  return (
    <div className="flex flex-col gap-[12px]">
      <BackRow title={`电池 · ${battery.percent}%`} onBack={onBack} />
      <div className="flex flex-col gap-[6px]">
        {plugged && (
          <div className="flex flex-col gap-[10px]">
            <PowerCard
              icon={<BatteryGlyph percent={100} charging className="h-[7px] w-[14px]" />}
              value={cardWattsText(chargingWatts)}
              label="充入电池"
              color="var(--success)"
            />
            <PowerCard
              icon={<LaptopGlyph />}
              value={cardWattsText(details?.system_watts ?? null)}
              label="系统供电"
              color="#0a84ff"
            />
          </div>
        )}
        {loading && !details ? (
          <p className="text-[11px]/[13px] text-white/50">更新中…</p>
        ) : (
          <div className="flex flex-col gap-[5px]">
            {rows.map((row) => (
              <DetailRow key={row.label} label={row.label} value={row.value} />
            ))}
          </div>
        )}
        <p className="text-[11px]/[13px] text-white/50">
          功率为估算值，各传感器读数可能延迟或存在差异，三项功率不一定完全相加。额定功率并非实际输入。拔电后显示电池放电功率；部分机型的电池数据可能延迟约一分钟。
        </p>
      </div>
      {showsActionArea(chargeToFull) && (
        <>
          <div className={DIVIDER} />
          <ChargeToFullAction presentation={chargeToFull} onAction={onChargeToFull} />
        </>
      )}
      <div className={DIVIDER} />
      <div className="flex flex-col gap-[6px]">
        <DetailActionButton
          label="打开系统电源设置"
          onClick={() => void api.statusBarOpenPane("battery")}
        />
        <DetailActionButton
          label="打开应用电池设置"
          onClick={() => void api.statusBarOpenSettings()}
        />
      </div>
    </div>
  );
}

/**
 * 输入法列表页（正版 `InputSourceListView`）：20×16 标记 + 13pt 名称 + 勾选。
 * 标记没有图标文件时画一枚圆角字牌，字用 `destination-out` 从牌子上挖空。
 */
export function InputSourceDetailPage({
  sources,
  onBack,
  onSelect,
  onOpenSettings,
}: {
  sources: StatusBarInputSource[];
  onBack: () => void;
  onSelect: (id: string) => void;
  onOpenSettings: (source: StatusBarInputSource) => void;
}) {
  // 第三方输入法自带设置 App（微信输入法 → WeTypeSettings.app），系统输入法没有，
  // 所以这些入口按输入法出现，而不是永远挂一个点了没反应的按钮。
  const withOwnSettings = sources.filter((source) => source.settings_app !== null);
  return (
    <div className="flex flex-col gap-[12px]">
      <BackRow title="输入法" onBack={onBack} />
      {sources.length === 0 ? (
        <p className="text-[11px]/[15px] text-white/50">
          没有启用任何键盘输入法，请在键盘设置中添加。
        </p>
      ) : (
        <div className="flex max-h-[330px] flex-col gap-[2px] overflow-y-auto">
          {sources.map((source) => (
            <button
              key={source.id}
              type="button"
              onClick={() => onSelect(source.id)}
              className="flex w-full items-center gap-[8px] py-[3px] text-left outline-none"
            >
              <span className="grid h-[16px] w-[20px] shrink-0 place-items-center">
                <InputSourceMarker
                  marker={source.marker}
                  fallback={<KeyboardGlyph className="h-[11px] w-[11px]" />}
                />
              </span>
              <span className="min-w-0 flex-1 truncate text-[13px]/[18px] text-white/85">
                {source.name}
              </span>
              {source.selected && (
                <span className="grid h-[16px] w-[16px] shrink-0 place-items-center text-white/50" aria-hidden>
                  <CheckGlyph className="h-[11px] w-[11px]" />
                </span>
              )}
            </button>
          ))}
        </div>
      )}
      <div className={DIVIDER} />
      <div className="flex flex-col gap-[6px]">
        {withOwnSettings.map((source) => (
          <DetailActionButton
            key={source.id}
            label={`${source.name}设置`}
            onClick={() => onOpenSettings(source)}
          />
        ))}
        <DetailActionButton
          label="打开键盘设置"
          onClick={() => void api.statusBarOpenPane("keyboard")}
        />
      </div>
    </div>
  );
}

/** Wi‑Fi 二级页的一节：小标题 + 行（正版 `sectionTitle` 是 caption semibold secondary）。 */
export function WifiSection({ title, children }: { title: string; children: ReactNode }) {
  return (
    <>
      <p className="text-[11px]/[13px] font-semibold text-white/50">{title}</p>
      {children}
    </>
  );
}

/**
 * 网络行：16×16 勾选或 wifi 符 + 名称 + 锁 + 信号（正版 `WiFiNetworkListView.networkRow`）。
 *
 * 点一行的语义照抄正版：已连接那行是禁用的（只画勾），其余每一行都只把系统
 * Wi-Fi 面板打开。面板自己不连接、也不要密码——密码、802.1X、验证失败都由系统
 * 那一处处理，所以这里不存在「点了没反应」的第三种情况。
 */
export function WifiNetworkRow({
  network,
  onOpenSettings,
}: {
  network: WifiNetwork;
  onOpenSettings: () => void;
}) {
  const opensSettings = wifiRowAction(network) === "open_settings";
  const busy = !opensSettings;

  return (
    <button
      type="button"
      onClick={onOpenSettings}
      disabled={busy}
      aria-label={`${network.displayName}，${network.isConnected ? "已连接" : "未连接"}${opensSettings ? "，打开 Wi-Fi 设置" : ""}`}
      className="flex w-full items-center gap-[10px] py-[3px] text-left outline-none disabled:cursor-default"
    >
      <span
        className={cx(
          "grid h-[16px] w-[16px] shrink-0 place-items-center",
          network.isConnected ? "text-[color:var(--panel-accent)]" : "text-white/50",
        )}
        aria-hidden
      >
        {network.isConnected ? (
          <CheckGlyph className="h-[11px] w-[11px]" />
        ) : (
          <WifiBarsGlyph bars={3} className="h-[16px] w-[16px]" />
        )}
      </span>
      <span className="min-w-0 flex-1 truncate text-[13px]/[17px] text-white/85">
        {network.displayName}
      </span>
      {requiresPassword(network.security) && (
        <span className="grid h-[16px] w-[16px] shrink-0 place-items-center text-white/50" aria-hidden>
          <LockGlyph />
        </span>
      )}
      {network.hotspotSymbol ? (
        <span className="grid h-[16px] w-[16px] shrink-0 place-items-center text-white/50" aria-hidden>
          <HotspotGlyph className="h-[16px] w-[16px]" />
        </span>
      ) : (
        <span className="grid h-[16px] w-[16px] shrink-0 place-items-center text-white/50" aria-hidden>
          {wifiBars(network.rssi) > 0 ? (
            <WifiBarsGlyph bars={wifiBars(network.rssi)} />
          ) : (
            <WifiUnknownGlyph />
          )}
        </span>
      )}
    </button>
  );
}

/**
 * Wi‑Fi 二级页（正版 `WiFiNetworkListView`）。
 *
 * 结构逐段照抄：返回行 + 刷新 → Wi‑Fi 开关 → 滚动区（个人热点 / 已知网络 /
 * 其他网络 + 状态提示，最高 330）→ 分隔线 → 连接说明 → 打开 Wi‑Fi 设置 →
 * 共享互联网…。合并、分组、排序与文案判序都在 `lib/wifiPanel.ts`，可离线断言。
 * 点网络行的落点也和正版一致：交给系统 Wi-Fi 面板，页内不再自己连接。
 */
export function WifiDetailPage({
  scan,
  powered,
  scanning,
  busy,
  onBack,
  onRefresh,
  onTogglePower,
  onRequestNameAccess,
  onOpenLocationSettings,
  onOpenSettings,
}: {
  scan: StatusBarWifiScan | null;
  /** 开关态取快照：扫描要一秒，不能等它才回显。 */
  powered: boolean;
  scanning: boolean;
  busy: boolean;
  onBack: () => void;
  onRefresh: () => void;
  onTogglePower: (on: boolean) => void;
  onRequestNameAccess: () => void;
  onOpenLocationSettings: () => void;
  /** 正版 `onOpenWiFiSettings`：已连接之外的每一行都走这里。 */
  onOpenSettings: () => void;
}) {
  const [showsDetails, setShowsDetails] = useState(false);
  const [showsMore, setShowsMore] = useState(false);

  const networkRow = (network: WifiNetwork) => (
    <WifiNetworkRow key={network.key} network={network} onOpenSettings={onOpenSettings} />
  );

  const networks = scan
    ? mergeNetworks(scan.candidates, scan.connected_bssid, scan.known_ssids, scan.hotspot)
    : [];
  const groups = scan
    ? groupNetworks(networks, scan.hotspot)
    : { personalHotspot: [], known: [], other: [] };
  const hasVisibleNetworks =
    groups.personalHotspot.length + groups.known.length + groups.other.length > 0;
  const message = scan
    ? wifiMessage(scan, hasVisibleNetworks, scanning)
    : { text: "正在扫描网络…", intent: "scanning" as const };
  const showsConnectionDetails =
    scan !== null &&
    (scan.details.ssid !== null ||
      groups.known.some((n) => n.isConnected) ||
      groups.personalHotspot.some((n) => n.isConnected));
  // 详情开关只画一处：挂在有已连接行的那一节（热点行优先，与正版同序）。
  const detailsInSection = groups.personalHotspot.some((n) => n.isConnected)
    ? "personalHotspot"
    : "known";
  const noInterface = scan?.list_state === "noInterface";

  const detailsToggle = (
    <>
      <button
        type="button"
        onClick={() => setShowsDetails((open) => !open)}
        className="flex w-full items-center gap-[6px] py-[2px] text-left text-[13px]/[17px] text-white/85 outline-none hover:text-white"
      >
        <span className="grid h-[16px] w-[16px] shrink-0 place-items-center text-white/60" aria-hidden>
          {showsDetails ? (
            <ChevronUpGlyph className="h-[15px] w-[15px]" />
          ) : (
            <InfoGlyph />
          )}
        </span>
        {showsDetails ? "隐藏详情" : "显示详情"}
      </button>
      {showsDetails && scan && (
        <div className="flex flex-col gap-[5px] pl-[26px] text-[11px]/[13px]">
          {wirelessRows(scan.details, showsMore).map((row) => (
            <DetailRow key={row.label} label={row.label} value={row.value} copyable={row.copyable} />
          ))}
          <button
            type="button"
            onClick={() => setShowsMore((more) => !more)}
            className="w-full text-left text-[11px]/[13px] text-white/60 outline-none hover:text-white"
          >
            {showsMore ? "收起详情" : "更多详情"}
          </button>
        </div>
      )}
    </>
  );

  return (
    <div className="flex flex-col gap-[12px]">
      <div className="flex items-center gap-[8px]">
        <span className="min-w-0 flex-1">
          <BackRow title="Wi-Fi" onBack={onBack} />
        </span>
        <IconAction label="刷新网络" disabled={scanning || noInterface} onClick={onRefresh}>
          <RefreshGlyph />
        </IconAction>
      </div>

      <div className="flex items-center gap-[10px]">
        <span className="min-w-0 flex-1 truncate text-[13px]/[17px] text-white/85">Wi-Fi</span>
        <Switch checked={powered} disabled={noInterface || busy} onChange={onTogglePower} label="Wi-Fi" />
      </div>

      <div className="flex max-h-[330px] flex-col gap-[10px] overflow-y-auto">
        {groups.personalHotspot.length > 0 && (
          <WifiSection title="个人热点">
            <div className="flex flex-col gap-[5px]">
              {groups.personalHotspot.map((network) => networkRow(network))}
            </div>
            {detailsInSection === "personalHotspot" && showsConnectionDetails && detailsToggle}
          </WifiSection>
        )}

        {(groups.known.length > 0 || showsConnectionDetails) && (
          <WifiSection title="已知网络">
            <div className="flex flex-col gap-[5px]">
              {groups.known.map((network) => networkRow(network))}
            </div>
            {detailsInSection === "known" && showsConnectionDetails && detailsToggle}
          </WifiSection>
        )}

        {groups.other.length > 0 && (
          <WifiSection title="其他网络">
            <div className="flex flex-col gap-[5px]">
              {groups.other.map((network) => networkRow(network))}
            </div>
          </WifiSection>
        )}

        {message &&
          (message.intent === "request_name_access" || message.intent === "location_settings" ? (
            <button
              type="button"
              onClick={
                message.intent === "request_name_access" ? onRequestNameAccess : onOpenLocationSettings
              }
              className="w-full text-left text-[11px]/[14px] text-white/60 outline-none hover:text-white"
            >
              {message.text}
            </button>
          ) : (
            <p className="flex items-center gap-[8px] text-[11px]/[14px] text-white/60">
              {message.intent === "scanning" && <Spinner size={12} />}
              {message.text}
            </p>
          ))}
      </div>

      <div className={DIVIDER} />

      <p className="text-[11px]/[14px] text-white/50">选择网络会在系统设置中打开 Wi-Fi 面板。</p>
      <DetailActionButton label="打开 Wi-Fi 设置" onClick={onOpenSettings} />
      <DetailActionButton
        label="共享互联网…"
        onClick={() => void api.statusBarOpenPane("sharing")}
      />
    </div>
  );
}

