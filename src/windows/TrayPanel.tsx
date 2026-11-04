/**
 * 菜单栏图标下拉面板（`traypanel` 窗口）。
 *
 * 结构与尺寸照抄 Status Trio 的 `StatusPopoverView`：330 宽、14 内边距、
 * 12 的行间距、纯分隔线（不画卡片）、24×24 图标列、13pt 标题 + 10pt 副标题、
 * 页脚「设置…」+ 省略号菜单。面板高度由内容决定（ResizeObserver 回报给
 * Rust，窗口贴着图标下方重排），空白处滚轮直接调音量；失焦由 Rust 收起。
 */
import { Fragment, useCallback, useEffect, useRef, useState } from "react";
import type { CSSProperties, ReactNode } from "react";
import { api } from "../lib/ipc";
import { errorMessage } from "../lib/errors";
import { useAppVersion } from "../lib/version";
import {
  groupNetworks,
  mergeNetworks,
  requiresPassword,
  wifiBars,
  wifiMessage,
  wirelessRows,
} from "../lib/wifiPanel";
import type { WifiNetwork } from "../lib/wifiPanel";
import {
  chargeToFullPresentation,
  showsActionArea,
} from "../lib/chargeToFull";
import type { ChargeToFullPhase, ChargeToFullPresentation } from "../lib/chargeToFull";
import { onTrayHidden, onTrayShown } from "../lib/windows";
import { useClipStore } from "../store/useClipStore";
import type {
  AudioScope,
  StatusBarAudioState,
  StatusBarBattery,
  StatusBarBatteryDetails,
  StatusBarBluetooth,
  StatusBarChargeToFullState,
  StatusBarInputSource,
  StatusBarNetwork,
  StatusBarSection,
  StatusBarSnapshot,
  StatusBarWifiScan,
} from "../lib/types";
import { Spinner, Switch, cx } from "../components/ui";
import { InputSourceMarker } from "../components/InputSourceMarker";

const DEFAULT_SECTIONS: StatusBarSection[] = [
  "battery",
  "network",
  "bluetooth",
  "volume",
  "audio_input",
  "input_source",
];

const ICON_COLUMN = "grid h-[24px] w-[24px] shrink-0 place-items-center";

/* ------------------------------------------------------------------ */
/* 字形：SF Symbols 在 WebView 里用不了，按同样的 15pt 墨迹手画。       */
/* ------------------------------------------------------------------ */

function Svg({
  children,
  className,
  strokeWidth = 1.7,
}: {
  children: ReactNode;
  className?: string;
  strokeWidth?: number;
}) {
  return (
    <svg
      viewBox="0 0 24 24"
      className={className}
      fill="none"
      stroke="currentColor"
      strokeWidth={strokeWidth}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      {children}
    </svg>
  );
}

function GearGlyph({ className = "h-[13px] w-[13px]" }: { className?: string }) {
  return (
    <Svg className={className}>
      <circle cx="12" cy="12" r="3" />
      <path d="M19.4 15a1.7 1.7 0 0 0 .34 1.87l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.7 1.7 0 0 0-1.87-.34 1.7 1.7 0 0 0-1 1.55V21a2 2 0 1 1-4 0v-.09a1.7 1.7 0 0 0-1.11-1.55 1.7 1.7 0 0 0-1.87.34l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.7 1.7 0 0 0 .34-1.87 1.7 1.7 0 0 0-1.55-1H3a2 2 0 1 1 0-4h.09a1.7 1.7 0 0 0 1.55-1.11 1.7 1.7 0 0 0-.34-1.87l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.7 1.7 0 0 0 1.87.34H9a1.7 1.7 0 0 0 1-1.55V3a2 2 0 1 1 4 0v.09a1.7 1.7 0 0 0 1 1.55 1.7 1.7 0 0 0 1.87-.34l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.7 1.7 0 0 0-.34 1.87V9a1.7 1.7 0 0 0 1.55 1H21a2 2 0 1 1 0 4h-.09a1.7 1.7 0 0 0-1.55 1z" />
    </Svg>
  );
}

function ChevronRightGlyph({ className = "h-[11px] w-[11px]" }: { className?: string }) {
  return (
    <Svg className={className} strokeWidth={2.2}>
      <path d="m9 5.5 6.5 6.5L9 18.5" />
    </Svg>
  );
}

function PowerGlyph({ className = "h-[14px] w-[14px]" }: { className?: string }) {
  return (
    <Svg className={className}>
      <path d="M12 3v8.5" />
      <path d="M6.7 6.8a7.5 7.5 0 1 0 10.6 0" />
    </Svg>
  );
}

function RefreshGlyph({ className = "h-[13px] w-[13px]" }: { className?: string }) {
  return (
    <Svg className={className}>
      <path d="M20 11a8 8 0 1 0-2.2 6.4" />
      <path d="M20 4.5V11h-6.5" />
    </Svg>
  );
}

function EllipsisCircleGlyph({ className = "h-[15px] w-[15px]" }: { className?: string }) {
  return (
    <Svg className={className}>
      <circle cx="12" cy="12" r="9" />
      <circle cx="8" cy="12" r="1.1" fill="currentColor" stroke="none" />
      <circle cx="12" cy="12" r="1.1" fill="currentColor" stroke="none" />
      <circle cx="16" cy="12" r="1.1" fill="currentColor" stroke="none" />
    </Svg>
  );
}

function CheckGlyph({ className = "h-[11px] w-[11px]" }: { className?: string }) {
  return (
    <Svg className={className} strokeWidth={2.2}>
      <path d="m5 12.5 4.5 4.5L19 7" />
    </Svg>
  );
}

function ChevronDownGlyph({ className = "h-[11px] w-[11px]" }: { className?: string }) {
  return (
    <Svg className={className} strokeWidth={2.2}>
      <path d="m5.5 9 6.5 6.5L18.5 9" />
    </Svg>
  );
}

function ChevronLeftGlyph({ className = "h-[14px] w-[14px]" }: { className?: string }) {
  return (
    <Svg className={className} strokeWidth={2}>
      <path d="M15 5.5 8.5 12l6.5 6.5" />
    </Svg>
  );
}

/** 正版 `chevron.up`：详情已展开时「隐藏详情」行的行首符号。 */
function ChevronUpGlyph({ className = "h-[11px] w-[11px]" }: { className?: string }) {
  return (
    <Svg className={className} strokeWidth={2.2}>
      <path d="m5.5 15 6.5-6.5L18.5 15" />
    </Svg>
  );
}

/**
 * SF `laptopcomputer`：正版「系统供电」卡的图标。
 *
 * 新版源码把这个节点改成了 `cpu`，但装机版 1.8.0 的二进制里没有 `cpu` 符号名、
 * 截图里也是这台笔记本（实测 13.8×7.9pt），所以照装机版画。
 */
function LaptopGlyph({ className = "h-[8px] w-[14px]" }: { className?: string }) {
  return (
    <svg
      viewBox="0 0 24 14"
      className={className}
      fill="none"
      stroke="currentColor"
      strokeWidth={1.7}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      <rect x="3.4" y="1.1" width="17.2" height="10.2" rx="1.6" />
      <path d="M1.2 13.2h21.6" strokeWidth={2.2} />
    </svg>
  );
}

/**
 * 电池：SF `battery.100.bolt` 的等价画法。
 *
 * 外壳用实描边、电量按比例填满内腔；充电/接电源时内腔填满、闪电用
 * evenodd 挖成洞（洞透出面板底色，与 SF Symbols 的负空间一致），而不是
 * 在填充上再盖一枚黑色闪电——浅色面板下黑闪电会糊成一团。
 */
function BatteryGlyph({
  percent,
  charging,
  className = "h-[12px] w-[22px]",
}: {
  percent: number;
  charging: boolean;
  className?: string;
}) {
  // 正版按 SF 电量档位取符号（100 / 75 / 50 / 25 / 0），不是连续比例。
  const level =
    percent >= 88 ? 1 : percent >= 63 ? 0.75 : percent >= 38 ? 0.5 : percent >= 13 ? 0.25 : 0;
  const innerWidth = 16 * level;
  return (
    <svg viewBox="0 0 24 13" className={className} aria-hidden>
      <rect
        x="0.75"
        y="0.75"
        width="19.5"
        height="11.5"
        rx="3.2"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.5"
      />
      <rect x="21.1" y="4.3" width="2.4" height="4.4" rx="1.2" fill="currentColor" />
      {charging ? (
        <path
          fillRule="evenodd"
          d="M4.2 2.5h12.6a1.7 1.7 0 0 1 1.7 1.7v4.1a1.7 1.7 0 0 1-1.7 1.7H4.2a1.7 1.7 0 0 1-1.7-1.7V4.2a1.7 1.7 0 0 1 1.7-1.7Zm8.2 1.2L8.4 7.3h2.3l-.8 2.3L13.9 6h-2.3Z"
        />
      ) : (
        <rect x="2.5" y="2.5" width={innerWidth} height="7.5" rx="1.7" fill="currentColor" />
      )}
    </svg>
  );
}

/** SF `bolt.fill`：立即充满电按钮的行首符号。 */
function BoltGlyph({ className = "h-[14px] w-[14px]" }: { className?: string }) {
  return (
    <svg viewBox="0 0 24 24" className={className} fill="currentColor" aria-hidden>
      <path d="M13.3 1.8 4.4 13.1h5.9L9.7 22.2l9.2-11.6h-6z" />
    </svg>
  );
}

/** SF `exclamationmark.triangle.fill`：动作行内的红色提示。 */
function WarningGlyph({ className = "h-[10px] w-[10px]" }: { className?: string }) {
  return (
    <svg viewBox="0 0 24 24" className={className} fill="currentColor" aria-hidden>
      <path d="M12 2.6 1.6 20.4a1.2 1.2 0 0 0 1 1.8h18.8a1.2 1.2 0 0 0 1-1.8L12 2.6Zm-1 5.3h2v7.2h-2V7.9Zm0 9.1h2v2.1h-2v-2.1Z" />
    </svg>
  );
}

/** SF `checkmark.circle.fill`：充电上限已恢复。 */
function CheckCircleGlyph({ className = "h-[14px] w-[14px]" }: { className?: string }) {
  return (
    <svg viewBox="0 0 24 24" className={className} fill="currentColor" aria-hidden>
      <path d="M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20Zm4.8 7.7-5.4 5.5a1 1 0 0 1-1.4 0l-2.4-2.4a1 1 0 0 1 1.4-1.4l1.7 1.7 4.7-4.8a1 1 0 0 1 1.4 1.4Z" />
    </svg>
  );
}

function SpeakerGlyph({
  muted,
  className = "h-[15px] w-[15px]",
}: {
  muted: boolean;
  className?: string;
}) {
  return (
    <Svg className={className} strokeWidth={1.8}>
      <path d="M11 5 6.5 9H3v6h3.5L11 19z" />
      {muted ? (
        <path d="m16 9 5 6M21 9l-5 6" />
      ) : (
        <>
          <path d="M15 9.5a4 4 0 0 1 0 5" />
          <path d="M17.5 7a7 7 0 0 1 0 10" />
        </>
      )}
    </Svg>
  );
}

function MicGlyph({
  muted,
  className = "h-[15px] w-[15px]",
}: {
  muted: boolean;
  className?: string;
}) {
  return (
    <Svg className={className} strokeWidth={1.8}>
      <rect x="9" y="3" width="6" height="11" rx="3" />
      <path d="M5 11a7 7 0 0 0 14 0M12 18v3" />
      {muted && <path d="m4 4 16 16" />}
    </Svg>
  );
}

function KeyboardGlyph({ className = "h-[15px] w-[15px]" }: { className?: string }) {
  return (
    <Svg className={className}>
      <rect x="2.5" y="6" width="19" height="12" rx="2.5" />
      <path d="M7 10h.01M11 10h.01M15 10h.01M7 14h10" />
    </Svg>
  );
}

function WifiGlyph({ powered }: { powered: boolean }) {
  return (
    <Svg className="h-[15px] w-[15px]" strokeWidth={1.8}>
      <path d="M2.5 8.5a15 15 0 0 1 19 0" />
      <path d="M6 12.5a10 10 0 0 1 12 0" />
      <path d="M9.5 16.5a5 5 0 0 1 5 0" />
      <circle cx="12" cy="19.5" r="1" fill="currentColor" stroke="none" />
      {!powered && <path d="m3.5 3.5 17 17" />}
    </Svg>
  );
}

function CableGlyph() {
  return (
    <Svg className="h-[15px] w-[15px]" strokeWidth={1.8}>
      <path d="M3.5 9.5V6h3v3.5a5.5 5.5 0 0 0 11 0V6h3v3.5a8.5 8.5 0 0 1-8.5 8.5v3" />
    </Svg>
  );
}

/**
 * 网络行的信号强度（正版 `wifi` + `variableValue`）：SF Symbols 按变量值逐条点亮
 * 弧线，WebView 里等价做成「圆点 + 1~3 条弧」，0 格由 `WifiUnknownGlyph` 接手。
 */
function WifiBarsGlyph({ bars, className = "h-[14px] w-[14px]" }: { bars: number; className?: string }) {
  return (
    <Svg className={className} strokeWidth={1.8}>
      {bars >= 3 && <path d="M2.5 8.5a15 15 0 0 1 19 0" />}
      {bars >= 2 && <path d="M6 12.5a10 10 0 0 1 12 0" />}
      {bars >= 1 && <path d="M9.5 16.5a5 5 0 0 1 5 0" />}
      <circle cx="12" cy="19.5" r="1" fill="currentColor" stroke="none" />
    </Svg>
  );
}

/** 正版 `wifi.exclamationmark`：信号读不到（0 格）时的行尾符号。 */
function WifiUnknownGlyph({ className = "h-[14px] w-[14px]" }: { className?: string }) {
  return (
    <Svg className={className} strokeWidth={1.8}>
      <path d="M2.5 8.5a15 15 0 0 1 12.5 0" />
      <path d="M6 12.5a10 10 0 0 1 7 0" />
      <path d="M19 4.5v5" />
      <circle cx="19" cy="14.5" r="1" fill="currentColor" stroke="none" />
    </Svg>
  );
}

/** 正版 `lock.fill`：需要密码的网络在名称右侧画一枚实心锁。 */
function LockGlyph({ className = "h-[10px] w-[10px]" }: { className?: string }) {
  return (
    <svg viewBox="0 0 24 24" className={className} fill="currentColor" aria-hidden>
      <path d="M7.5 10V7.5a4.5 4.5 0 0 1 9 0V10h.5A2.5 2.5 0 0 1 19.5 12.5v6A2.5 2.5 0 0 1 17 21H7a2.5 2.5 0 0 1-2.5-2.5v-6A2.5 2.5 0 0 1 7 10h.5Zm2.5 0h4V7.5a2 2 0 1 0-4 0V10Z" />
    </svg>
  );
}

/** 正版 `personalhotspot`：已连接的个人热点在行尾多画一枚。 */
function HotspotGlyph({ className = "h-[15px] w-[15px]" }: { className?: string }) {
  return (
    <Svg className={className} strokeWidth={1.8}>
      <circle cx="12" cy="15" r="2.2" fill="currentColor" stroke="none" />
      <path d="M7.6 10.4a6.2 6.2 0 0 1 8.8 0" />
      <path d="M4.6 7a10.4 10.4 0 0 1 14.8 0" />
    </Svg>
  );
}

/** 正版 `info.circle` / `chevron.up`：「显示详情 / 隐藏详情」两态的行首符号。 */
function InfoGlyph({ className = "h-[15px] w-[15px]" }: { className?: string }) {
  return (
    <Svg className={className} strokeWidth={1.8}>
      <circle cx="12" cy="12" r="9" />
      <path d="M12 11v5.5" />
      <circle cx="12" cy="7.8" r="1" fill="currentColor" stroke="none" />
    </Svg>
  );
}

function BluetoothGlyph({ className = "h-[15px] w-[15px]" }: { className?: string }) {
  return (
    <Svg className={className} strokeWidth={1.8}>
      <path d="m7 7.5 10 9-5 4V3.5l5 4-10 9" />
    </Svg>
  );
}

function HeadphonesGlyph({ className = "h-[15px] w-[15px]" }: { className?: string }) {
  return (
    <Svg className={className} strokeWidth={1.8}>
      <path d="M4.5 13.5v-1.8a7.5 7.5 0 0 1 15 0v1.8" />
      <rect x="3" y="13" width="4.2" height="6.5" rx="2.1" />
      <rect x="16.8" y="13" width="4.2" height="6.5" rx="2.1" />
    </Svg>
  );
}

function MouseGlyph({ className = "h-[15px] w-[15px]" }: { className?: string }) {
  return (
    <Svg className={className} strokeWidth={1.8}>
      <rect x="6.5" y="2.5" width="11" height="19" rx="5.5" />
      <path d="M12 6.5v3.5" />
    </Svg>
  );
}

function PhoneGlyph({ className = "h-[15px] w-[15px]" }: { className?: string }) {
  return (
    <Svg className={className} strokeWidth={1.8}>
      <rect x="7" y="2.5" width="10" height="19" rx="2.6" />
      <path d="M10.8 18.5h2.4" />
    </Svg>
  );
}

/** 按 `device_minorType` 选设备字形，未知类型退回蓝牙标记（对齐正版的行图标）。 */
function bluetoothDeviceGlyph(kind: string | null) {
  const k = (kind ?? "").toLowerCase();
  if (k.includes("headphone") || k.includes("airpod") || k.includes("speaker")) {
    return <HeadphonesGlyph className="h-[12px] w-[12px]" />;
  }
  if (k.includes("keyboard")) return <KeyboardGlyph className="h-[12px] w-[12px]" />;
  if (k.includes("mouse") || k.includes("trackpad")) {
    return <MouseGlyph className="h-[12px] w-[12px]" />;
  }
  if (k.includes("phone")) return <PhoneGlyph className="h-[12px] w-[12px]" />;
  return <BluetoothGlyph className="h-[12px] w-[12px]" />;
}

/* ------------------------------------------------------------------ */
/* 行件                                                                */
/* ------------------------------------------------------------------ */

/** 24×24 的图标按钮：与 `PanelActionMetrics.hitTarget` 一致。 */
function IconAction({
  label,
  onClick,
  disabled,
  className,
  children,
}: {
  label: string;
  onClick: () => void;
  disabled?: boolean;
  className?: string;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      title={label}
      aria-label={label}
      disabled={disabled}
      onClick={onClick}
      className={cx(
        "grid h-[24px] w-[24px] shrink-0 place-items-center rounded-[5px] outline-none",
        "text-white/60 transition-colors duration-100",
        "hover:bg-white/[0.08] hover:text-white disabled:opacity-35 disabled:hover:bg-transparent",
        className,
      )}
    >
      {children}
    </button>
  );
}

/**
 * 摘要行：24×24 图标 + 13pt 标题 + 10pt 副标题 + Spacer + chevron；gear 在行外。
 * 与 `BatteryStatusView` / `NetworkStatusView` 的 `summaryRow` 同构。
 */
function SummaryRow({
  icon,
  iconClass,
  title,
  subtitle,
  onActivate,
  chevron = false,
  trailing,
}: {
  icon: ReactNode;
  iconClass?: string;
  title: string;
  subtitle: string;
  onActivate?: () => void;
  chevron?: boolean;
  trailing?: ReactNode;
}) {
  return (
    <div className="flex items-center gap-[10px]">
      <button
        type="button"
        onClick={onActivate}
        disabled={!onActivate}
        className="flex min-w-0 flex-1 items-center gap-[10px] text-left outline-none"
      >
        <span className={cx(ICON_COLUMN, iconClass)} aria-hidden>
          {icon}
        </span>
        <span className="flex min-w-0 flex-1 flex-col gap-[2px]">
          <span className="truncate text-[13px]/[17px] font-semibold tabular-nums text-white/90">
            {title}
          </span>
          <span className="truncate text-[10px]/[13px] text-white/50">{subtitle}</span>
        </span>
        {chevron && (
          <span className="shrink-0 text-white/35" aria-hidden>
            <ChevronRightGlyph />
          </span>
        )}
      </button>
      {trailing}
    </div>
  );
}

/** 音量行：静音字 + accent 滑条 + 百分比 + gear（`VolumeControlsView`）。 */
function AudioRow({
  scope,
  state,
  onVolume,
  onMute,
}: {
  scope: AudioScope;
  state: StatusBarAudioState;
  onVolume: (scope: AudioScope, value: number) => void;
  onMute: (scope: AudioScope, muted: boolean) => void;
}) {
  const usable = state.available && state.volume_available;
  const percent = state.volume_available ? Math.round(state.volume * 100) : null;
  // 正版 `VolumeControlsView`：滑条 `.tint(.accentColor)`，静音时 `.secondary`。
  const sliderStyle = {
    "--slider-pct": `${percent ?? 0}%`,
    "--slider-fill": state.muted ? "var(--panel-muted-fill)" : "var(--panel-accent)",
  } as CSSProperties;

  return (
    <div className="flex min-h-[24px] items-center gap-[10px]">
      <IconAction
        label={state.muted ? "取消静音" : "静音"}
        disabled={!state.available || !state.mute_available}
        onClick={() => onMute(scope, !state.muted)}
        className={
          state.muted ? "text-[color:var(--danger-fill)] hover:text-[color:var(--danger-fill)]" : undefined
        }
      >
        {scope === "output" ? <SpeakerGlyph muted={state.muted} /> : <MicGlyph muted={state.muted} />}
      </IconAction>

      <input
        type="range"
        className="slider slider-accent mx-[2px] min-w-0 flex-1 disabled:opacity-40"
        style={sliderStyle}
        min={0}
        max={100}
        step={1}
        value={percent ?? 0}
        disabled={!usable}
        aria-label={scope === "output" ? "音量" : "输入音量"}
        aria-valuetext={percent === null ? "不可用" : `${percent}%`}
        onChange={(e) => onVolume(scope, Number(e.target.value) / 100)}
      />

      <span className="w-[34px] shrink-0 text-right text-[10px]/[13px] tabular-nums text-white/50">
        {percent === null ? "—" : `${percent}%`}
      </span>

      <IconAction
        label={scope === "output" ? "打开声音设置" : "打开输入设置"}
        onClick={() => void api.statusBarOpenPane(scope === "output" ? "sound_output" : "sound_input")}
      >
        <GearGlyph />
      </IconAction>
    </div>
  );
}

/** 设备行：24×24 圆底 + 设备名；选中项 accent 填充（`OutputDeviceRow`）。 */
function DeviceRow({
  icon,
  name,
  selected,
  trailing,
  onSelect,
}: {
  icon: ReactNode;
  name: string;
  selected: boolean;
  trailing?: ReactNode;
  onSelect?: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onSelect}
      disabled={!onSelect}
      className="flex w-full items-center gap-[10px] py-[3px] text-left outline-none disabled:cursor-default"
    >
      <span
        aria-hidden
        className={cx(
          "grid h-[24px] w-[24px] shrink-0 place-items-center rounded-full",
          selected
            ? "bg-[var(--panel-accent)] text-[color:var(--on-accent)]"
            : "bg-[var(--panel-tile)] text-white/60",
        )}
      >
        {icon}
      </span>
      <span
        className={cx(
          "min-w-0 flex-1 truncate text-[13px]/[18px]",
          selected ? "font-semibold text-white/90" : "text-white/85",
        )}
      >
        {name}
      </span>
      {trailing}
    </button>
  );
}

/** 正版 `DeviceListExpansionButton`：整行居中、16pt 圆底外的箭头 + 展开/收起。 */
function ExpansionButton({
  expanded,
  onToggle,
}: {
  expanded: boolean;
  onToggle: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onToggle}
      className="flex w-full items-center justify-center gap-[8px] py-[5px] text-[11px]/[14px] text-white/50 outline-none"
    >
      <span
        className={cx(
          "grid h-[16px] w-[16px] place-items-center transition-transform duration-200",
          expanded && "rotate-180",
        )}
        aria-hidden
      >
        <ChevronDownGlyph className="h-[10px] w-[10px]" />
      </span>
      {expanded ? "收起" : "展开"}
    </button>
  );
}

const DIVIDER = "h-px w-full bg-white/[0.12]";

/* ------------------------------------------------------------------ */
/* 二级页件                                                            */
/* ------------------------------------------------------------------ */

/** 正版 `NavigationBackRow`：32pt 命中框的返回箭头 + 8pt 间距 + headline 标题。 */
function BackRow({ title, onBack }: { title: string; onBack: () => void }) {
  return (
    <button
      type="button"
      onClick={onBack}
      className="flex h-[32px] items-center gap-[8px] text-left outline-none"
    >
      <span
        className="grid h-[32px] w-[32px] shrink-0 place-items-center text-white/70"
        aria-hidden
      >
        <ChevronLeftGlyph />
      </span>
      <span className="min-w-0 flex-1 truncate text-[13px]/[17px] font-semibold tabular-nums text-white/90">
        {title}
      </span>
    </button>
  );
}

/**
 * 功率流向卡（`BatteryPowerFlowView` 的整宽卡片版）：60pt 高、12pt 圆角、
 * 8% 分支色底，图标在上、数字居中、名称在下。
 */
function PowerCard({
  icon,
  value,
  label,
  color,
}: {
  icon: ReactNode;
  value: string;
  label: string;
  color: string;
}) {
  return (
    <div
      className="flex h-[60px] w-full flex-col items-center justify-center gap-[3px] rounded-[12px]"
      style={{ background: `color-mix(in srgb, ${color} 8%, transparent)` }}
    >
      <span className="grid h-[19px] place-items-center" style={{ color }} aria-hidden>
        {icon}
      </span>
      <span className="text-[13px]/[16px] font-semibold tabular-nums text-white/90">
        {value}
      </span>
      <span className="text-[10px]/[12px] text-white/50">{label}</span>
    </div>
  );
}

/** 详情页读数行：11pt、标签 secondary、数值右对齐（`PanelDetailRowsView`）。 */
function DetailRow({
  label,
  value,
  copyable = false,
}: {
  label: string;
  value: string;
  /** 正版只对地址类读数给复制入口（`LinkDetailRow.isCopyable`）。 */
  copyable?: boolean;
}) {
  const copy = () => {
    void navigator.clipboard?.writeText(value).catch(() => {});
  };
  return (
    <div className="flex items-baseline gap-[8px] text-[11px]/[13px]">
      <span className="shrink-0 text-white/50">{label}</span>
      <span className="min-w-0 flex-1" />
      {copyable ? (
        <button
          type="button"
          title="点击复制"
          onClick={copy}
          className="max-w-[62%] truncate text-right tabular-nums text-white/90 outline-none hover:text-white"
        >
          {value}
        </button>
      ) : (
        <span className="max-w-[62%] truncate text-right tabular-nums text-white/90">{value}</span>
      )}
    </div>
  );
}

/** 详情页底部的纯文字动作（正版 `.buttonStyle(.plain)` + body 字号）。 */
function DetailActionButton({ label, onClick }: { label: string; onClick: () => void }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="w-full truncate text-left text-[13px]/[16px] text-white/85 outline-none hover:text-white"
    >
      {label}
    </button>
  );
}

/**
 * 「立即充满电」动作行（正版 `ChargeToFullNowActionView`）：28pt 高、7pt 圆角、
 * 14pt 闪电/转圈、caption 字号；摘要行与详情页共用同一套外观。
 */
function ChargeToFullAction({
  presentation,
  onAction,
}: {
  presentation: ChargeToFullPresentation;
  onAction: () => void;
}) {
  if (!showsActionArea(presentation)) return null;

  const applying = presentation.kind === "applying";
  const cancelling = presentation.kind === "cancelling";
  const charging = presentation.kind === "chargingToFull";
  const disabled = applying || cancelling;

  const actionLabel =
    presentation.kind === "applying"
      ? "正在请求立即充满电…"
      : presentation.kind === "cancelling"
        ? "正在恢复充电上限…"
        : presentation.kind === "chargingToFull" ||
            presentation.kind === "cancelFailed"
          ? "取消立即充满"
          : "立即充满电";

  const icon =
    applying || cancelling ? (
      <Spinner size={14} />
    ) : presentation.kind === "available" || presentation.kind === "failed" ? (
      <BoltGlyph />
    ) : null;

  return (
    <div className="flex flex-col gap-[5px]">
      {presentation.kind !== "limitRestored" && presentation.kind !== "unavailable" && (
        <button
          type="button"
          disabled={disabled}
          onClick={onAction}
          className={cx(
            "flex min-h-[28px] w-full items-center gap-[7px] rounded-[7px] border border-white/[0.12]",
            "bg-white/[0.08] px-[10px] text-left outline-none",
            "hover:bg-white/[0.12] disabled:opacity-60 disabled:hover:bg-white/[0.08]",
          )}
        >
          {icon && (
            <span className="grid h-[14px] w-[14px] shrink-0 place-items-center" aria-hidden>
              {icon}
            </span>
          )}
          {charging ? (
            <>
              <span className="min-w-0 truncate text-[10px]/[13px] text-white/50">
                正在充电至 100%
              </span>
              <span className="min-w-[8px] flex-1" />
              <span className="shrink-0 truncate text-[10px]/[13px] font-medium text-white/90">
                {actionLabel}
              </span>
            </>
          ) : (
            <span className="min-w-0 flex-1 truncate text-[10px]/[13px] font-medium text-white/90">
              {actionLabel}
            </span>
          )}
        </button>
      )}

      {presentation.kind === "limitRestored" && (
        <div className="flex min-h-[28px] w-full items-center gap-[7px] px-[10px]">
          <span className="grid h-[14px] w-[14px] shrink-0 place-items-center text-[color:var(--success)]" aria-hidden>
            <CheckCircleGlyph />
          </span>
          <span className="min-w-0 truncate text-[10px]/[13px] text-white/50">
            {presentation.limit >= 100
              ? "充电上限已清除"
              : `充电上限已恢复为 ${presentation.limit}%`}
          </span>
        </div>
      )}

      {presentation.kind === "failed" && (
        <p className="flex items-start gap-[4px] text-[9px]/[12px] text-[color:var(--danger-fill)]">
          <span className="mt-[1px] shrink-0" aria-hidden>
            <WarningGlyph />
          </span>
          <span className="min-w-0">无法请求立即充满电，请重试。</span>
        </p>
      )}

      {presentation.kind === "cancelFailed" && (
        <p className="flex items-start gap-[4px] text-[9px]/[12px] text-[color:var(--danger-fill)]">
          <span className="mt-[1px] shrink-0" aria-hidden>
            <WarningGlyph />
          </span>
          <span className="min-w-0">无法恢复充电上限：{presentation.message ?? ""}</span>
        </p>
      )}

      {presentation.kind === "unavailable" && (
        <p className="flex items-start gap-[4px] text-[9px]/[12px] text-[color:var(--danger-fill)]">
          <span className="mt-[1px] shrink-0" aria-hidden>
            <WarningGlyph />
          </span>
          <span className="min-w-0">立即充满电当前不可用。</span>
        </p>
      )}
    </div>
  );
}

/**
 * 电池详情页（正版 `BatteryDetailsView`）。
 *
 * 流向图只在接电时出现；有图时「整机功耗（估算）」「充电（估算）」两行被过滤，
 * 只留「整机读取时间」——这是正版 `detailRows` 的判序，照抄。
 */
function BatteryDetailPage({
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
function InputSourceDetailPage({
  sources,
  onBack,
  onSelect,
}: {
  sources: StatusBarInputSource[];
  onBack: () => void;
  onSelect: (id: string) => void;
}) {
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
      <DetailActionButton
        label="打开键盘设置"
        onClick={() => void api.statusBarOpenPane("keyboard")}
      />
    </div>
  );
}

/** Wi‑Fi 二级页的一节：小标题 + 行（正版 `sectionTitle` 是 caption semibold secondary）。 */
function WifiSection({ title, children }: { title: string; children: ReactNode }) {
  return (
    <>
      <p className="text-[11px]/[13px] font-semibold text-white/50">{title}</p>
      {children}
    </>
  );
}

/**
 * 网络行（正版 `networkRow`）：16×16 勾选或 wifi 符 + 名称 + 锁 + 信号。
 * 正版从不自己连网：除当前连接外，点任何一行都是打开系统 Wi‑Fi 面板。
 */
function WifiNetworkRow({
  network,
  onOpenSettings,
}: {
  network: WifiNetwork;
  onOpenSettings: () => void;
}) {
  const opensSettings = !network.isConnected;
  return (
    <button
      type="button"
      onClick={onOpenSettings}
      disabled={!opensSettings}
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
 * 其他网络 + 状态提示，最高 330）→ 分隔线 → 切换说明 → 打开 Wi‑Fi 设置 →
 * 共享互联网…。合并、分组、排序与文案判序都在 `lib/wifiPanel.ts`，可离线断言。
 */
function WifiDetailPage({
  scan,
  powered,
  scanning,
  busy,
  onBack,
  onRefresh,
  onTogglePower,
  onRequestNameAccess,
  onOpenLocationSettings,
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
}) {
  const [showsDetails, setShowsDetails] = useState(false);
  const [showsMore, setShowsMore] = useState(false);

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

  const openSettings = () => void api.statusBarOpenPane("wifi");
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
              {groups.personalHotspot.map((network) => (
                <WifiNetworkRow key={network.key} network={network} onOpenSettings={openSettings} />
              ))}
            </div>
            {detailsInSection === "personalHotspot" && showsConnectionDetails && detailsToggle}
          </WifiSection>
        )}

        {(groups.known.length > 0 || showsConnectionDetails) && (
          <WifiSection title="已知网络">
            <div className="flex flex-col gap-[5px]">
              {groups.known.map((network) => (
                <WifiNetworkRow key={network.key} network={network} onOpenSettings={openSettings} />
              ))}
            </div>
            {detailsInSection === "known" && showsConnectionDetails && detailsToggle}
          </WifiSection>
        )}

        {groups.other.length > 0 && (
          <WifiSection title="其他网络">
            <div className="flex flex-col gap-[5px]">
              {groups.other.map((network) => (
                <WifiNetworkRow key={network.key} network={network} onOpenSettings={openSettings} />
              ))}
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
      <DetailActionButton label="打开 Wi-Fi 设置" onClick={openSettings} />
      <DetailActionButton
        label="共享互联网…"
        onClick={() => void api.statusBarOpenPane("sharing")}
      />
    </div>
  );
}

/* ------------------------------------------------------------------ */
/* 文案                                                                */
/* ------------------------------------------------------------------ */

function timeToFullText(minutes: number | null): string {
  if (minutes === null || minutes <= 0) return "正在计算充满时间";
  const h = Math.floor(minutes / 60);
  const m = minutes % 60;
  if (h === 0) return `预计 ${m} 分钟充满`;
  if (m === 0) return `预计 ${h} 小时充满`;
  return `预计 ${h} 小时 ${m} 分钟充满`;
}

function batterySubtitle(battery: StatusBarBattery): string {
  // 正版 `batterySubtitle` 的判序：无电池 → 已充满 → 充电中 → 低电量模式 → 已连接电源 → 电池供电。
  if (!battery.present) return "无电池设备";
  if (battery.charged) return "已充满";
  if (battery.charging) return timeToFullText(battery.minutes_to_full);
  if (battery.low_power) return "低电量模式";
  if (battery.plugged) return "已连接电源";
  return "电池供电";
}

/* 详情页的数字格式：正版用 `Formatted(.number.precision(.fractionLength(n)))`。 */

const UNAVAILABLE = "暂不可用";

function fixed(value: number, digits: number): string {
  return value.toFixed(digits);
}

/** 功率行：1 位小数 + " W"。 */
function wattsText(value: number | null): string {
  return value === null ? UNAVAILABLE : `${fixed(value, 1)} W`;
}

/** 功率卡：绝对值先四舍五入到 0.01，≥100 后不再带小数（窄卡放不下）。 */
function cardWattsText(value: number | null): string {
  if (value === null) return UNAVAILABLE;
  const rounded = Math.round(Math.abs(value) * 100) / 100;
  return `${value.toFixed(rounded >= 100 ? 0 : 2)} W`;
}

function remainingTimeText(minutes: number | null): string {
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
function summaryEligible(network: StatusBarNetwork): boolean {
  return (
    network.connected && (network.link === "wifi" || network.link === "hotspot")
  );
}

function networkTitle(network: StatusBarNetwork): string {
  if (summaryEligible(network) && network.ssid) return network.ssid;
  // 正版把有线链路单独画一行（`ethernet.title`）；本端口只有一行网络，沿用它的文案。
  return network.link === "ethernet" ? "以太网" : "Wi-Fi";
}

/** 正版 `measurements`：`%@ GHz · %d dBm`，读数缺失就少一段。 */
function networkMeasurements(network: StatusBarNetwork): string | null {
  if (!summaryEligible(network)) return null;
  const parts: string[] = [];
  if (network.band) parts.push(network.band);
  if (network.rssi !== null && network.rssi < 0) parts.push(`${network.rssi} dBm`);
  return parts.length > 0 ? parts.join(" · ") : null;
}

/** 正版 `wifiSubtitle(_:)`：链路状态本身，不含授权提示。 */
function networkStateText(network: StatusBarNetwork): string {
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
function networkSubtitle(network: StatusBarNetwork): string {
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
const MAX_VISIBLE_BLUETOOTH_DEVICES = 5;

function bluetoothSubtitle(state: StatusBarBluetooth): string {
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

/** 二级页（正版的 NavigationStack：一次只压一层，Esc 先返回再收起）。 */
type DetailPage = "battery" | "input_source" | "wifi";

export default function TrayPanel() {
  const config = useClipStore((s) => s.config);
  const version = useAppVersion();
  const [snapshot, setSnapshot] = useState<StatusBarSnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [detail, setDetail] = useState<DetailPage | null>(null);
  const [details, setDetails] = useState<StatusBarBatteryDetails | null>(null);
  const [detailsLoading, setDetailsLoading] = useState(false);
  /** PowerUI 充电上限读数：面板可见时每 2 秒拉一次。 */
  const [chargeState, setChargeState] = useState<StatusBarChargeToFullState | null>(null);
  const [chargePhase, setChargePhase] = useState<ChargeToFullPhase>({ kind: "idle" });
  /** Wi‑Fi 二级页读数：一次全信道扫描要一秒，只在进页/点刷新时拉。 */
  const [wifiScan, setWifiScan] = useState<StatusBarWifiScan | null>(null);
  const [wifiScanning, setWifiScanning] = useState(false);
  const [bluetoothExpanded, setBluetoothExpanded] = useState(false);
  const [menuOpen, setMenuOpen] = useState(false);
  /** 开关类操作进行中：避免连点，也让用户看到状态在等系统。 */
  const [busy, setBusy] = useState<{ network: boolean; bluetooth: boolean }>({
    network: false,
    bluetooth: false,
  });

  const rootRef = useRef<HTMLDivElement | null>(null);
  const contentRef = useRef<HTMLDivElement | null>(null);
  const snapshotRef = useRef<StatusBarSnapshot | null>(null);
  const volumeTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  /** 上一次滚轮生效时间：系统会把一次滚动合成为一串事件，不节流会一次跳半格。 */
  const lastWheelAt = useRef(0);
  /** 音量提示音：正版用 `NSSound`，这里用同一份 wav 的 `Audio` 元素。 */
  const tickSound = useRef<HTMLAudioElement | null>(null);
  const lastTickAt = useRef(0);
  const feedbackEnabled = useRef(true);
  /** 正在拖动滑条 / 写入未落：轮询不许抢走控制权（正版同规则）。 */
  const editingVolume = useRef(false);

  const statusbar = config?.statusbar;
  const sections = (statusbar?.sections ?? DEFAULT_SECTIONS).filter((id) =>
    DEFAULT_SECTIONS.includes(id),
  );

  const refresh = useCallback(async () => {
    try {
      const next = await api.statusBarState();
      snapshotRef.current = next;
      setSnapshot(next);
      setError(null);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setLoading(false);
    }
  }, []);

  /** Wi‑Fi 二级页读数：正版只在进页与点刷新时扫一次，不做轮询（扫描要一秒）。 */
  const loadWifiPage = useCallback(async () => {
    setWifiScanning(true);
    try {
      setWifiScan(await api.statusBarWifiPage());
      setError(null);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setWifiScanning(false);
    }
  }, []);

  /** 正版 `VolumeFeedback`：只在标量真的变了才响，且 100ms 内最多一声。 */
  const playVolumeTick = useCallback(() => {
    if (!feedbackEnabled.current) return;
    const now = performance.now();
    if (now - lastTickAt.current < 100) return;
    lastTickAt.current = now;
    const audio = tickSound.current;
    if (!audio) return;
    audio.currentTime = 0;
    void audio.play().catch(() => {
      // 自动播放被拒：提示音属于锦上添花，不打扰用户。
    });
  }, []);

  useEffect(() => {
    const audio = new Audio("/VolumeFeedback.wav");
    audio.preload = "auto";
    audio.volume = 1;
    tickSound.current = audio;
    void api
      .statusBarVolumeFeedbackEnabled()
      .then((enabled) => {
        feedbackEnabled.current = enabled;
      })
      .catch(() => {});
    return () => {
      audio.pause();
      tickSound.current = null;
    };
  }, []);

  useEffect(() => {
    let unsubShow: (() => void) | undefined;
    let unsubHide: (() => void) | undefined;
    void refresh();
    void onTrayShown(() => {
      setMenuOpen(false);
      void refresh();
    }).then((fn) => {
      unsubShow = fn;
    });
    void onTrayHidden(() => {
      setDetail(null);
    }).then((fn) => {
      unsubHide = fn;
    });
    // 面板可见期间每 500ms 跟一次系统音量：键盘/触控栏调音量时滑条要动。
    const poll = setInterval(() => {
      if (document.hidden) return;
      if (editingVolume.current) return;
      void api
        .statusBarAudioLevels()
        .then((levels) => {
          const current = snapshotRef.current;
          if (!current) return;
          const merge = (
            state: StatusBarAudioState,
            next: StatusBarAudioState,
          ): StatusBarAudioState =>
            state.available &&
            next.available &&
            (state.volume !== next.volume || state.muted !== next.muted)
              ? { ...state, volume: next.volume, muted: next.muted }
              : state;
          const output = merge(current.audio_output, levels.output);
          const input = merge(current.audio_input, levels.input);
          if (output === current.audio_output && input === current.audio_input) return;
          const next: StatusBarSnapshot = { ...current, audio_output: output, audio_input: input };
          snapshotRef.current = next;
          setSnapshot(next);
        })
        .catch(() => {});
    }, 500);
    return () => {
      clearInterval(poll);
      unsubShow?.();
      unsubHide?.();
    };
  }, [refresh]);

  // 电池详情页：正版打开时立刻读一次，之后有功率就 2 秒、没有就 15 秒。
  useEffect(() => {
    if (detail !== "battery") return;
    let cancelled = false;
    setDetailsLoading(true);
    const load = () => {
      void api
        .statusBarBatteryDetails()
        .then((next) => {
          if (!cancelled) setDetails(next);
        })
        .catch((e) => {
          if (!cancelled) setError(errorMessage(e));
        })
        .finally(() => {
          if (!cancelled) setDetailsLoading(false);
        });
    };
    load();
    const interval = setInterval(load, 2000);
    return () => {
      cancelled = true;
      clearInterval(interval);
    };
  }, [detail]);

  // 「立即充满电」读数：只在面板可见时每 2 秒读一次，打开面板立刻读。
  useEffect(() => {
    let cancelled = false;
    let unsub: (() => void) | undefined;
    const load = () => {
      if (document.hidden) return;
      void api
        .statusBarChargeToFullState()
        .then((next) => {
          if (!cancelled) setChargeState(next);
        })
        .catch(() => {});
    };
    load();
    const interval = setInterval(load, 2000);
    void onTrayShown(() => {
      setChargePhase((prev) =>
        prev.kind === "failed" || prev.kind === "cancelFailed" ? { kind: "idle" } : prev,
      );
      load();
    }).then((fn) => {
      unsub = fn;
    });
    return () => {
      cancelled = true;
      clearInterval(interval);
      unsub?.();
    };
  }, []);

  // 内容高度 → 窗口高度（Rust 会 clamp 到 720 并重新贴到图标下方）。
  useEffect(() => {
    const el = contentRef.current;
    if (!el) return;
    const report = () => {
      const height = Math.ceil(el.getBoundingClientRect().height) + 2;
      void api.statusBarResize(height).catch(() => {});
    };
    report();
    const observer = new ResizeObserver(report);
    observer.observe(el);
    return () => observer.disconnect();
  }, [snapshot, details, wifiScan, wifiScanning, sections.length, detail, bluetoothExpanded, menuOpen]);

  // Esc：正版先返回上一层，已在根页面才收起面板（窗口失焦由 Rust 处理）。
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      if (menuOpen) {
        setMenuOpen(false);
        return;
      }
      if (detail) {
        setDetail(null);
        return;
      }
      void api.statusBarHide();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [detail, menuOpen]);

  const setVolumeOptimistic = useCallback(
    (scope: AudioScope, value: number) => {
      const current = snapshotRef.current;
      if (!current) return;
      const key = scope === "input" ? "audio_input" : "audio_output";
      const state = current[key];
      if (!state.available || !state.volume_available) return;
      const volume = Math.min(1, Math.max(0, value));
      const changed = Math.abs(state.volume - volume) > 0.0001;
      const next: StatusBarSnapshot = {
        ...current,
        [key]: { ...state, volume, muted: false },
      };
      snapshotRef.current = next;
      setSnapshot(next);
      if (changed) {
        // 拖动/滚轮改音量期间暂停 500ms 轮询，否则硬件读数会把滑条拽回去。
        editingVolume.current = true;
        playVolumeTick();
      }

      if (volumeTimer.current) clearTimeout(volumeTimer.current);
      volumeTimer.current = setTimeout(() => {
        void api
          .statusBarSetVolume(scope, volume)
          .catch((e) => setError(errorMessage(e)))
          .finally(() => {
            editingVolume.current = false;
          });
      }, 90);
    },
    [playVolumeTick],
  );

  // 面板空白处滚轮调音量（设置里可关；方向可反转）。
  useEffect(() => {
    const el = rootRef.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      const sb = statusbar;
      if (!sb?.scroll_adjusts_volume) return;
      const current = snapshotRef.current;
      if (!current) return;
      const scope: AudioScope = sb.scroll_scope === "input" ? "input" : "output";
      const state = scope === "input" ? current.audio_input : current.audio_output;
      if (!state.available || !state.volume_available) return;
      e.preventDefault();
      const now = performance.now();
      if (now - lastWheelAt.current < 70) return;
      lastWheelAt.current = now;
      const up = e.deltaY < 0;
      const natural = sb.scroll_natural;
      const step = (up ? 1 : -1) * (natural ? -1 : 1) * 0.05;
      setVolumeOptimistic(scope, state.volume + step);
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, [statusbar, setVolumeOptimistic]);

  const toggleMute = async (scope: AudioScope, muted: boolean) => {
    const current = snapshotRef.current;
    if (!current) return;
    const key = scope === "input" ? "audio_input" : "audio_output";
    const next: StatusBarSnapshot = {
      ...current,
      [key]: { ...current[key], muted },
    };
    snapshotRef.current = next;
    setSnapshot(next);
    try {
      await api.statusBarSetMute(scope, muted);
    } catch (e) {
      setError(errorMessage(e));
      void refresh();
    }
  };

  const pickDevice = async (scope: AudioScope, id: number) => {
    try {
      await api.statusBarSetDefaultDevice(scope, id);
      await refresh();
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  const pickInputSource = async (id: string) => {
    try {
      await api.statusBarSelectInputSource(id);
      await refresh();
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  /**
   * Wi-Fi 名称授权（正版 `requestWiFiNameAccess`）：能弹就弹，弹不出来就直接把
   * 用户送到系统「定位服务」——不能只留一句「允许定位」却没有入口。
   */
  const requestNameAccess = async () => {
    try {
      const outcome = await api.statusBarRequestLocation();
      if (outcome === "open_location_settings") {
        void api.statusBarOpenPane("location");
        return;
      }
      if (outcome === "requested") window.setTimeout(() => void refresh(), 1500);
      else await refresh();
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  /** Wi-Fi 开关：先乐观更新，再等系统生效后重读（开关链路要 ~1s）。 */
  const toggleWifi = async (on: boolean) => {
    const current = snapshotRef.current;
    if (!current) return;
    const optimistic: StatusBarSnapshot = {
      ...current,
      network: {
        ...current.network,
        powered: on,
        connected: on ? current.network.connected : false,
        ssid: on ? current.network.ssid : null,
        rssi: on ? current.network.rssi : null,
        ipv4: on ? current.network.ipv4 : null,
        link: on ? current.network.link : "none",
      },
    };
    snapshotRef.current = optimistic;
    setSnapshot(optimistic);
    setBusy((prev) => ({ ...prev, network: true }));
    try {
      await api.statusBarSetWifi(on);
      window.setTimeout(() => void refresh(), 900);
    } catch (e) {
      setError(errorMessage(e));
      void refresh();
    } finally {
      setBusy((prev) => ({ ...prev, network: false }));
    }
  };

  // 进入 Wi‑Fi 页：立刻扫一次（正版 `refreshWiFi` 挂在 `.onAppear` 与刷新按钮上）。
  useEffect(() => {
    if (detail !== "wifi") return;
    void loadWifiPage();
  }, [detail, loadWifiPage]);

  /** 页里的 Wi‑Fi 开关：先按快照回显，等系统生效再重扫（否则列表还是旧的）。 */
  const toggleWifiFromPage = async (on: boolean) => {
    await toggleWifi(on);
    window.setTimeout(() => void loadWifiPage(), 1000);
  };

  /** 页里的「允许定位以显示 Wi‑Fi 名称」：答完授权弹窗后重扫一次。 */
  const grantNameAccessFromPage = async () => {
    await requestNameAccess();
    window.setTimeout(() => void loadWifiPage(), 2000);
  };

  /** 蓝牙开关：系统切换比 Wi-Fi 更慢，重读就一次。 */
  const toggleBluetooth = async (on: boolean) => {
    const current = snapshotRef.current;
    if (!current) return;
    const optimistic: StatusBarSnapshot = {
      ...current,
      bluetooth: {
        ...current.bluetooth,
        powered: on,
        devices: on ? current.bluetooth.devices : [],
      },
    };
    snapshotRef.current = optimistic;
    setSnapshot(optimistic);
    setBusy((prev) => ({ ...prev, bluetooth: true }));
    try {
      await api.statusBarSetBluetooth(on);
      window.setTimeout(() => void refresh(), 1200);
    } catch (e) {
      setError(errorMessage(e));
      void refresh();
    } finally {
      setBusy((prev) => ({ ...prev, bluetooth: false }));
    }
  };

  /** 当前展示态：摘要行和详情页共用同一份判定。 */
  const chargeToFull = snapshot
    ? chargeToFullPresentation(snapshot.battery, chargeState, chargePhase, Date.now())
    : ({ kind: "hidden" } as ChargeToFullPresentation);

  const startChargeToFull = async () => {
    const limit = chargeState?.limit ?? null;
    setChargePhase({ kind: "applying" });
    try {
      await api.statusBarChargeToFull();
      setChargePhase({ kind: "accepted", limit, at: Date.now() });
    } catch {
      setChargePhase({ kind: "failed" });
    }
    void api
      .statusBarChargeToFullState()
      .then(setChargeState)
      .catch(() => {});
  };

  const cancelChargeToFull = async () => {
    setChargePhase({ kind: "cancelling" });
    try {
      const limit = await api.statusBarCancelChargeToFull();
      setChargePhase({ kind: "restored", limit, at: Date.now() });
    } catch (e) {
      setChargePhase({ kind: "cancelFailed", message: errorMessage(e) });
    }
    void api
      .statusBarChargeToFullState()
      .then(setChargeState)
      .catch(() => {});
  };

  const runChargeToFull = () => {
    switch (chargeToFull.kind) {
      case "available":
      case "failed":
        void startChargeToFull();
        break;
      case "chargingToFull":
      case "cancelFailed":
        void cancelChargeToFull();
        break;
      default:
        break;
    }
  };

  const renderSection = (id: StatusBarSection): ReactNode => {
    if (!snapshot) return null;

    if (id === "battery") {
      const { battery } = snapshot;
      if (!battery.present) return null;
      const tint =
        battery.percent < 20
          ? "text-[color:var(--danger-fill)]"
          : battery.charging || battery.plugged
            ? "text-[color:var(--success)]"
            : "text-white/90";
      return (
        <div className="flex flex-col gap-[8px]">
          <SummaryRow
            icon={<BatteryGlyph percent={battery.percent} charging={battery.charging || battery.plugged} />}
            iconClass={tint}
            title={`电池 · ${battery.percent}%`}
            subtitle={batterySubtitle(battery)}
            chevron
            onActivate={() => setDetail("battery")}
            trailing={
              <IconAction
                label="打开电池设置"
                onClick={() => void api.statusBarOpenPane("battery")}
              >
                <GearGlyph />
              </IconAction>
            }
          />
          <ChargeToFullAction presentation={chargeToFull} onAction={runChargeToFull} />
        </div>
      );
    }

    if (id === "network") {
      const network = snapshot.network;
      if (!network.available) return null;
      // 正版把有线链路路由到另一页（`EthernetLinkView`），这里先不给入口。
      const isWired = network.link === "ethernet";
      return (
        <SummaryRow
          icon={network.link === "ethernet" ? <CableGlyph /> : <WifiGlyph powered={network.powered} />}
          iconClass="text-white/60"
          title={networkTitle(network)}
          subtitle={networkSubtitle(network)}
          chevron={!isWired}
          onActivate={isWired ? undefined : () => setDetail("wifi")}
          trailing={
            <>
              <IconAction
                label={network.powered ? "关闭 Wi-Fi" : "打开 Wi-Fi"}
                disabled={busy.network}
                onClick={() => void toggleWifi(!network.powered)}
                className={
                  network.powered
                    ? "text-[color:var(--panel-accent)] hover:text-[color:var(--panel-accent)]"
                    : undefined
                }
              >
                <PowerGlyph />
              </IconAction>
              <IconAction
                label="打开 Wi-Fi 设置"
                onClick={() => void api.statusBarOpenPane("wifi")}
              >
                <GearGlyph />
              </IconAction>
            </>
          }
        />
      );
    }

    if (id === "bluetooth") {
      const bluetooth = snapshot.bluetooth;
      if (!bluetooth.available) return null;
      const visibleBluetoothDevices = bluetoothExpanded
        ? bluetooth.devices
        : bluetooth.devices.slice(0, MAX_VISIBLE_BLUETOOTH_DEVICES);
      return (
        <div className="flex flex-col gap-[8px]">
          <div className="flex items-center gap-[10px]">
            <span className={cx(ICON_COLUMN, "text-white/60")} aria-hidden>
              <BluetoothGlyph />
            </span>
            <span className="flex min-w-0 flex-1 flex-col gap-[2px]">
              <span className="truncate text-[13px]/[17px] font-semibold text-white/90">蓝牙</span>
              <span className="truncate text-[10px]/[13px] text-white/50">
                {bluetoothSubtitle(bluetooth)}
              </span>
            </span>
            {/* 正版顺序：电源 → 刷新 → 设置。 */}
            <IconAction
              label={bluetooth.powered ? "关闭蓝牙" : "打开蓝牙"}
              disabled={busy.bluetooth}
              onClick={() => void toggleBluetooth(!bluetooth.powered)}
              className={
                bluetooth.powered
                  ? "text-[color:var(--panel-accent)] hover:text-[color:var(--panel-accent)]"
                  : undefined
              }
            >
              <PowerGlyph />
            </IconAction>
            <IconAction label="刷新蓝牙设备" disabled={busy.bluetooth} onClick={() => void refresh()}>
              <RefreshGlyph />
            </IconAction>
            <IconAction
              label="打开蓝牙设置"
              onClick={() => void api.statusBarOpenPane("bluetooth")}
            >
              <GearGlyph />
            </IconAction>
          </div>

          {bluetooth.powered && bluetooth.devices.length > 0 && (
            <>
              <div className="flex flex-col gap-[2px]">
                {visibleBluetoothDevices.map((device) => (
                  <DeviceRow
                    key={device.address ?? device.name}
                    icon={bluetoothDeviceGlyph(device.kind)}
                    name={device.name}
                    selected={device.connected}
                    trailing={
                      device.connected && device.battery !== null && (
                        <span className="shrink-0 text-[10px]/[13px] tabular-nums text-white/50">
                          {device.battery}%
                        </span>
                      )
                    }
                  />
                ))}
              </div>
              {bluetooth.devices.length > MAX_VISIBLE_BLUETOOTH_DEVICES && (
                <ExpansionButton
                  expanded={bluetoothExpanded}
                  onToggle={() => setBluetoothExpanded((open) => !open)}
                />
              )}
            </>
          )}
        </div>
      );
    }

    if (id === "volume" || id === "audio_input") {
      const scope: AudioScope = id === "volume" ? "output" : "input";
      const state = id === "volume" ? snapshot.audio_output : snapshot.audio_input;
      const devices = state.devices;
      const showsDevices = scope === "output" ? devices.length > 1 : devices.length > 0;
      return (
        <div className="flex flex-col gap-[8px]">
          <AudioRow
            scope={scope}
            state={state}
            onVolume={setVolumeOptimistic}
            onMute={(s, muted) => void toggleMute(s, muted)}
          />

          {showsDevices && (
            <>
              <div className={cx(DIVIDER, "mt-[2px]")} />
              <div className="flex flex-col gap-[2px]">
                {devices.map((device) => (
                  <DeviceRow
                    key={device.id}
                    icon={
                      scope === "output" ? (
                        <SpeakerGlyph muted={false} className="h-[12px] w-[12px]" />
                      ) : (
                        <MicGlyph muted={false} className="h-[12px] w-[12px]" />
                      )
                    }
                    name={device.name}
                    selected={device.is_default}
                    onSelect={
                      device.is_default ? undefined : () => void pickDevice(scope, device.id)
                    }
                  />
                ))}
              </div>
            </>
          )}
        </div>
      );
    }

    if (id === "input_source") {
      const sources = snapshot.input_sources;
      if (sources.length === 0) return null;
      const current = sources.find((source) => source.selected);
      return (
        <SummaryRow
          icon={<KeyboardGlyph />}
          iconClass="text-white/60"
          title="输入法"
          subtitle={current?.name ?? "未选择输入法"}
          chevron
          onActivate={() => setDetail("input_source")}
          trailing={
            <IconAction
              label="打开键盘设置"
              onClick={() => void api.statusBarOpenPane("keyboard")}
            >
              <GearGlyph />
            </IconAction>
          }
        />
      );
    }

    return null;
  };

  const renderedSections = sections
    .map((id) => ({ id, node: renderSection(id) }))
    .filter((entry) => entry.node !== null);

  /** 正版 `StatusPopoverView`：二级页整页替换摘要（含页脚），只留 14pt 内边距。 */
  const detailPage =
    detail === "battery" && snapshot ? (
      <BatteryDetailPage
        battery={snapshot.battery}
        details={details}
        loading={detailsLoading}
        chargeToFull={chargeToFull}
        onChargeToFull={runChargeToFull}
        onBack={() => setDetail(null)}
      />
    ) : detail === "input_source" && snapshot ? (
      <InputSourceDetailPage
        sources={snapshot.input_sources}
        onBack={() => setDetail(null)}
        onSelect={(id) => void pickInputSource(id)}
      />
    ) : detail === "wifi" && snapshot ? (
      <WifiDetailPage
        scan={wifiScan}
        powered={snapshot.network.powered}
        scanning={wifiScanning}
        busy={busy.network}
        onBack={() => setDetail(null)}
        onRefresh={() => void loadWifiPage()}
        onTogglePower={(on) => void toggleWifiFromPage(on)}
        onRequestNameAccess={() => void grantNameAccessFromPage()}
        onOpenLocationSettings={() => void api.statusBarOpenPane("location")}
      />
    ) : null;

  return (
    <div
      ref={rootRef}
      style={{ "--panel-accent": snapshot?.accent ?? "#007aff" } as CSSProperties}
      className="app-surface app-surface--panel h-full w-full overflow-y-auto"
    >
      <div
        ref={contentRef}
        className="flex w-full flex-col gap-[12px] px-[14px] py-[14px] text-white"
      >
        {detailPage ? (
          <>
            {detailPage}
            {error && (
              <p className="text-[10px]/[14px] text-[#ff9f0a]">
                <span aria-hidden>⚠ </span>
                {error}
              </p>
            )}
          </>
        ) : (
          <>
            {loading && !snapshot ? (
              <div className="flex items-center justify-center gap-[8px] py-[18px] text-[12px] text-white/50">
                <Spinner />
                正在读取系统状态…
              </div>
            ) : (
              renderedSections.map(({ id, node }, index) => (
                <Fragment key={id}>
                  {index > 0 && <div className={DIVIDER} />}
                  {node}
                </Fragment>
              ))
            )}

            {error && (
              <p className="text-[10px]/[14px] text-[#ff9f0a]">
                <span aria-hidden>⚠ </span>
                {error}
              </p>
            )}

            {!loading && renderedSections.length === 0 && (
              <p className="py-[10px] text-center text-[12px] text-white/50">
                没有启用的面板项目，可在「设置 → 状态栏图标」里勾选。
              </p>
            )}

            <div className={cx(DIVIDER, "my-[2px] opacity-60")} />

            <div className="flex items-center gap-[8px] text-[12px]/[16px] text-white/60">
              <button
                type="button"
                onClick={() => void api.statusBarOpenSettings()}
                className="flex min-h-[24px] min-w-0 flex-1 items-center gap-[6px] text-left outline-none hover:text-white"
              >
                <GearGlyph className="h-[13px] w-[13px] shrink-0" />
                <span className="truncate">设置…</span>
              </button>

              {version && (
                <span className="shrink-0 tabular-nums text-white/45">{version}</span>
              )}

              <div className="relative flex shrink-0 items-center">
                <button
                  type="button"
                  aria-label="更多"
                  title="更多"
                  aria-expanded={menuOpen}
                  onClick={() => setMenuOpen((open) => !open)}
                  className="grid h-[24px] w-[24px] place-items-center rounded-[5px] outline-none hover:bg-white/[0.08] hover:text-white"
                >
                  <EllipsisCircleGlyph />
                </button>

                {menuOpen && (
                  <>
                    <div className="fixed inset-0 z-10" onClick={() => setMenuOpen(false)} />
                    <div className="menu absolute bottom-[28px] right-0 z-20 !min-w-[160px]">
                      <button
                        type="button"
                        className="menu-item"
                        onClick={() => {
                          setMenuOpen(false);
                          void api.quitApp();
                        }}
                      >
                        退出 Halo
                      </button>
                    </div>
                  </>
                )}
              </div>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
