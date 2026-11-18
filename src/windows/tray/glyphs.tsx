//! 面板里手绘的 SF Symbol 字形（WebView 无法直接使用系统符号）。

import type { ReactNode } from "react";

export const ICON_COLUMN = "grid h-[24px] w-[24px] shrink-0 place-items-center";

/* ------------------------------------------------------------------ */
/* 字形：SF Symbols 在 WebView 里用不了，按同样的 15pt 墨迹手画。       */
/* ------------------------------------------------------------------ */

export function Svg({
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

export function GearGlyph({ className = "h-[13px] w-[13px]" }: { className?: string }) {
  return (
    <Svg className={className}>
      <circle cx="12" cy="12" r="3" />
      <path d="M19.4 15a1.7 1.7 0 0 0 .34 1.87l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.7 1.7 0 0 0-1.87-.34 1.7 1.7 0 0 0-1 1.55V21a2 2 0 1 1-4 0v-.09a1.7 1.7 0 0 0-1.11-1.55 1.7 1.7 0 0 0-1.87.34l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.7 1.7 0 0 0 .34-1.87 1.7 1.7 0 0 0-1.55-1H3a2 2 0 1 1 0-4h.09a1.7 1.7 0 0 0 1.55-1.11 1.7 1.7 0 0 0-.34-1.87l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.7 1.7 0 0 0 1.87.34H9a1.7 1.7 0 0 0 1-1.55V3a2 2 0 1 1 4 0v.09a1.7 1.7 0 0 0 1 1.55 1.7 1.7 0 0 0 1.87-.34l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.7 1.7 0 0 0-.34 1.87V9a1.7 1.7 0 0 0 1.55 1H21a2 2 0 1 1 0 4h-.09a1.7 1.7 0 0 0-1.55 1z" />
    </Svg>
  );
}

export function ChevronRightGlyph({ className = "h-[11px] w-[11px]" }: { className?: string }) {
  return (
    <Svg className={className} strokeWidth={2.2}>
      <path d="m9 5.5 6.5 6.5L9 18.5" />
    </Svg>
  );
}

export function PowerGlyph({ className = "h-[14px] w-[14px]" }: { className?: string }) {
  return (
    <Svg className={className}>
      <path d="M12 3v8.5" />
      <path d="M6.7 6.8a7.5 7.5 0 1 0 10.6 0" />
    </Svg>
  );
}

export function RefreshGlyph({ className = "h-[13px] w-[13px]" }: { className?: string }) {
  return (
    <Svg className={className}>
      <path d="M20 11a8 8 0 1 0-2.2 6.4" />
      <path d="M20 4.5V11h-6.5" />
    </Svg>
  );
}

export function EllipsisCircleGlyph({ className = "h-[15px] w-[15px]" }: { className?: string }) {
  return (
    <Svg className={className}>
      <circle cx="12" cy="12" r="9" />
      <circle cx="8" cy="12" r="1.1" fill="currentColor" stroke="none" />
      <circle cx="12" cy="12" r="1.1" fill="currentColor" stroke="none" />
      <circle cx="16" cy="12" r="1.1" fill="currentColor" stroke="none" />
    </Svg>
  );
}

export function CheckGlyph({ className = "h-[11px] w-[11px]" }: { className?: string }) {
  return (
    <Svg className={className} strokeWidth={2.2}>
      <path d="m5 12.5 4.5 4.5L19 7" />
    </Svg>
  );
}

export function ChevronDownGlyph({ className = "h-[11px] w-[11px]" }: { className?: string }) {
  return (
    <Svg className={className} strokeWidth={2.2}>
      <path d="m5.5 9 6.5 6.5L18.5 9" />
    </Svg>
  );
}

export function ChevronLeftGlyph({ className = "h-[14px] w-[14px]" }: { className?: string }) {
  return (
    <Svg className={className} strokeWidth={2}>
      <path d="M15 5.5 8.5 12l6.5 6.5" />
    </Svg>
  );
}

/** 正版 `chevron.up`：详情已展开时「隐藏详情」行的行首符号。 */
export function ChevronUpGlyph({ className = "h-[11px] w-[11px]" }: { className?: string }) {
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
export function LaptopGlyph({ className = "h-[8px] w-[14px]" }: { className?: string }) {
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
export function BatteryGlyph({
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
export function BoltGlyph({ className = "h-[14px] w-[14px]" }: { className?: string }) {
  return (
    <svg viewBox="0 0 24 24" className={className} fill="currentColor" aria-hidden>
      <path d="M13.3 1.8 4.4 13.1h5.9L9.7 22.2l9.2-11.6h-6z" />
    </svg>
  );
}

/** SF `exclamationmark.triangle.fill`：动作行内的红色提示。 */
export function WarningGlyph({ className = "h-[10px] w-[10px]" }: { className?: string }) {
  return (
    <svg viewBox="0 0 24 24" className={className} fill="currentColor" aria-hidden>
      <path d="M12 2.6 1.6 20.4a1.2 1.2 0 0 0 1 1.8h18.8a1.2 1.2 0 0 0 1-1.8L12 2.6Zm-1 5.3h2v7.2h-2V7.9Zm0 9.1h2v2.1h-2v-2.1Z" />
    </svg>
  );
}

/** SF `checkmark.circle.fill`：充电上限已恢复。 */
export function CheckCircleGlyph({ className = "h-[14px] w-[14px]" }: { className?: string }) {
  return (
    <svg viewBox="0 0 24 24" className={className} fill="currentColor" aria-hidden>
      <path d="M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20Zm4.8 7.7-5.4 5.5a1 1 0 0 1-1.4 0l-2.4-2.4a1 1 0 0 1 1.4-1.4l1.7 1.7 4.7-4.8a1 1 0 0 1 1.4 1.4Z" />
    </svg>
  );
}

export function SpeakerGlyph({
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

export function MicGlyph({
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

export function KeyboardGlyph({ className = "h-[15px] w-[15px]" }: { className?: string }) {
  return (
    <Svg className={className}>
      <rect x="2.5" y="6" width="19" height="12" rx="2.5" />
      <path d="M7 10h.01M11 10h.01M15 10h.01M7 14h10" />
    </Svg>
  );
}

export function WifiGlyph({ powered }: { powered: boolean }) {
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

export function CableGlyph() {
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
export function WifiBarsGlyph({ bars, className = "h-[14px] w-[14px]" }: { bars: number; className?: string }) {
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
export function WifiUnknownGlyph({ className = "h-[14px] w-[14px]" }: { className?: string }) {
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
export function LockGlyph({ className = "h-[10px] w-[10px]" }: { className?: string }) {
  return (
    <svg viewBox="0 0 24 24" className={className} fill="currentColor" aria-hidden>
      <path d="M7.5 10V7.5a4.5 4.5 0 0 1 9 0V10h.5A2.5 2.5 0 0 1 19.5 12.5v6A2.5 2.5 0 0 1 17 21H7a2.5 2.5 0 0 1-2.5-2.5v-6A2.5 2.5 0 0 1 7 10h.5Zm2.5 0h4V7.5a2 2 0 1 0-4 0V10Z" />
    </svg>
  );
}

/** 正版 `personalhotspot`：已连接的个人热点在行尾多画一枚。 */
/** 正版 `person.shield`（macOS 网络设置里 VPN 用的那枚）：接管时盾里带勾。 */
export function ShieldGlyph({ checked }: { checked: boolean }) {
  return (
    <Svg className="h-[15px] w-[15px]" strokeWidth={1.8}>
      <path d="M12 3 4.5 5.8v5.4c0 4.5 3.1 7.6 7.5 9.6 4.4-2 7.5-5.1 7.5-9.6V5.8z" />
      {checked && <path d="m9 11.5 2.2 2.2L15.2 9.6" />}
    </Svg>
  );
}

export function HotspotGlyph({ className = "h-[15px] w-[15px]" }: { className?: string }) {
  return (
    <Svg className={className} strokeWidth={1.8}>
      <circle cx="12" cy="15" r="2.2" fill="currentColor" stroke="none" />
      <path d="M7.6 10.4a6.2 6.2 0 0 1 8.8 0" />
      <path d="M4.6 7a10.4 10.4 0 0 1 14.8 0" />
    </Svg>
  );
}

/** 正版 `info.circle` / `chevron.up`：「显示详情 / 隐藏详情」两态的行首符号。 */
export function InfoGlyph({ className = "h-[15px] w-[15px]" }: { className?: string }) {
  return (
    <Svg className={className} strokeWidth={1.8}>
      <circle cx="12" cy="12" r="9" />
      <path d="M12 11v5.5" />
      <circle cx="12" cy="7.8" r="1" fill="currentColor" stroke="none" />
    </Svg>
  );
}

export function BluetoothGlyph({ className = "h-[15px] w-[15px]" }: { className?: string }) {
  return (
    <Svg className={className} strokeWidth={1.8}>
      <path d="m7 7.5 10 9-5 4V3.5l5 4-10 9" />
    </Svg>
  );
}

export function HeadphonesGlyph({ className = "h-[15px] w-[15px]" }: { className?: string }) {
  return (
    <Svg className={className} strokeWidth={1.8}>
      <path d="M4.5 13.5v-1.8a7.5 7.5 0 0 1 15 0v1.8" />
      <rect x="3" y="13" width="4.2" height="6.5" rx="2.1" />
      <rect x="16.8" y="13" width="4.2" height="6.5" rx="2.1" />
    </Svg>
  );
}

export function MouseGlyph({ className = "h-[15px] w-[15px]" }: { className?: string }) {
  return (
    <Svg className={className} strokeWidth={1.8}>
      <rect x="6.5" y="2.5" width="11" height="19" rx="5.5" />
      <path d="M12 6.5v3.5" />
    </Svg>
  );
}

export function PhoneGlyph({ className = "h-[15px] w-[15px]" }: { className?: string }) {
  return (
    <Svg className={className} strokeWidth={1.8}>
      <rect x="7" y="2.5" width="10" height="19" rx="2.6" />
      <path d="M10.8 18.5h2.4" />
    </Svg>
  );
}

/** 按 `device_minorType` 选设备字形，未知类型退回蓝牙标记（对齐正版的行图标）。 */
export function bluetoothDeviceGlyph(kind: string | null) {
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

