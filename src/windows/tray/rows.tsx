//! 面板里的通用行：摘要、音量、设备、详情动作与立即充满电。

import type { CSSProperties, ReactNode } from "react";
import { Spinner, cx } from "../../components/ui";
import { api } from "../../lib/ipc";
import { showsActionArea } from "../../lib/chargeToFull";
import type { ChargeToFullPresentation } from "../../lib/chargeToFull";
import type {
  AudioScope,
  StatusBarAudioState,
} from "../../lib/types";
import {
  BoltGlyph,
  ChevronLeftGlyph,
  GearGlyph,
  CheckCircleGlyph,
  ChevronDownGlyph,
  ChevronRightGlyph,
  SpeakerGlyph,
  MicGlyph,
  WarningGlyph,
  ICON_COLUMN,
} from "./glyphs";

export function IconAction({
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
export function SummaryRow({
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
export function AudioRow({
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
export function DeviceRow({
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
export function ExpansionButton({
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

export const DIVIDER = "h-px w-full bg-white/[0.12]";

/* ------------------------------------------------------------------ */
/* 二级页件                                                            */
/* ------------------------------------------------------------------ */

/** 正版 `NavigationBackRow`：32pt 命中框的返回箭头 + 8pt 间距 + headline 标题。 */
export function BackRow({ title, onBack }: { title: string; onBack: () => void }) {
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
export function PowerCard({
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
export function DetailRow({
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
export function DetailActionButton({ label, onClick }: { label: string; onClick: () => void }) {
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
export function ChargeToFullAction({
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
