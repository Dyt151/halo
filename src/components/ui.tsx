/**
 * Halo 共享 UI 控件库
 *
 * 所有控件严格遵循 docs/UI-DESIGN.md（Apple HIG 落地规范）：
 * - 高度固定（按钮 28 / 小按钮 24 / 输入框 28 / 导航 30 / 工具栏 44）
 * - 圆角只用 5/6/7/8/10/12/full 六档
 * - 字号只用 11/12/13/15/17/20/28 七档
 * - 动效 100–200ms + Apple 缓动曲线
 * - 强调色（琥珀）只出现在主按钮 / 选中态 / 聚焦光环
 */
import { forwardRef, useEffect, useRef, useState, type ButtonHTMLAttributes, type ReactNode } from "react";

/** 条件类名拼接，过滤掉 falsy 值。 */
export function cx(...parts: (string | false | null | undefined)[]): string {
  return parts.filter(Boolean).join(" ");
}

/* ------------------------------------------------------------------ */
/* 按钮                                                                */
/* ------------------------------------------------------------------ */

type BtnVariant = "primary" | "secondary" | "danger" | "ghost";

interface BtnProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: BtnVariant;
  size?: "md" | "sm";
}

const VARIANT: Record<BtnVariant, string> = {
  primary: "btn-primary",
  secondary: "btn-secondary",
  danger: "btn-danger",
  ghost: "btn-ghost",
};

export const Btn = forwardRef<HTMLButtonElement, BtnProps>(function Btn(
  { variant = "secondary", size = "md", className, ...rest },
  ref,
) {
  return (
    <button
      ref={ref}
      type="button"
      className={cx("btn", VARIANT[variant], size === "sm" && "btn-sm", className)}
      {...rest}
    />
  );
});

interface IconBtnProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  /** 必填：无障碍与 tooltip 双重保障（规范：每个图标按钮必须有 title）。 */
  title: string;
  danger?: boolean;
}

export const IconBtn = forwardRef<HTMLButtonElement, IconBtnProps>(function IconBtn(
  { danger, className, ...rest },
  ref,
) {
  return (
    <button
      ref={ref}
      type="button"
      className={cx(danger ? "btn-icon-danger" : "btn-icon", className)}
      {...rest}
    />
  );
});

/** 卡片内嵌的图标按钮（规范 §9.1）：视觉图标 13px，热区 24×24（macOS HIG ≥24pt）。 */
export const CardIconBtn = forwardRef<
  HTMLButtonElement,
  IconBtnProps & { active?: boolean }
>(function CardIconBtn({ danger, active, className, ...rest }, ref) {
  return (
    <button
      ref={ref}
      type="button"
      className={cx(
        "grid h-[24px] w-[24px] shrink-0 place-items-center rounded-[5px] outline-none",
        "transition-[background-color,color,transform] duration-120 ease-out-soft active:scale-[0.98]",
        active
          ? "text-accent-500"
          : danger
            ? "text-white/40 hover:bg-danger-fill/15 hover:text-danger"
            : "text-white/40 hover:bg-white/[0.10] hover:text-white",
        className,
      )}
      {...rest}
    />
  );
});

/* ------------------------------------------------------------------ */
/* 分段控件 / Pill / Kbd                                               */
/* ------------------------------------------------------------------ */

export interface SegmentOption<T extends string = string> {
  value: T;
  label: string;
  icon?: ReactNode;
  /** 可选计数徽标（如「图片 8」）。 */
  count?: number;
}

/**
 * 分段控件：外槽 + 选中滑块（规范 §5.4）。
 *
 * 评审 U5：role="tablist" 只给了角色，没有键盘行为 —— 方向键不移动、
 * Tab 会逐个停留、未选中项没有 aria-selected="false"。这里补上 roving
 * tabindex（只有选中项可 Tab 到）与 ←/→/↑/↓/Home/End 导航。
 */
export function Segment<T extends string = string>({
  items,
  value,
  onChange,
  className,
  label = "筛选",
}: {
  items: SegmentOption<T>[];
  value: T;
  onChange: (v: T) => void;
  className?: string;
  /** tablist 的无障碍名称。 */
  label?: string;
}) {
  const refs = useRef<(HTMLButtonElement | null)[]>([]);

  const onKeyDown = (e: React.KeyboardEvent, index: number) => {
    const last = items.length - 1;
    let next = -1;
    if (e.key === "ArrowRight" || e.key === "ArrowDown") next = index === last ? 0 : index + 1;
    else if (e.key === "ArrowLeft" || e.key === "ArrowUp") next = index === 0 ? last : index - 1;
    else if (e.key === "Home") next = 0;
    else if (e.key === "End") next = last;
    if (next < 0) return;
    e.preventDefault();
    // 阻止冒泡：面板在 window 上也监听方向键做卡片导航，
    // 不拦住的话「在分段上按方向键」会同时切换筛选并移动卡片选中项。
    e.stopPropagation();
    onChange(items[next].value);
    refs.current[next]?.focus();
  };

  return (
    <div className={cx("seg", className)} role="tablist" aria-label={label}>
      {items.map((it, index) => {
        const active = value === it.value;
        return (
          <button
            key={it.value}
            ref={(el) => {
              refs.current[index] = el;
            }}
            type="button"
            role="tab"
            aria-selected={active}
            tabIndex={active ? 0 : -1}
            onClick={() => onChange(it.value)}
            onKeyDown={(e) => onKeyDown(e, index)}
            className={cx("seg-item", active && "seg-item-active")}
          >
            {it.icon}
            {it.label}
            {typeof it.count === "number" && (
              <span className="ml-[4px] text-[11px] tabular-nums opacity-60">{it.count}</span>
            )}
          </button>
        );
      })}
    </div>
  );
}

/** 胶囊标签：全部 / 分类 / 标签 / 能力 chip（规范 §5.5）。 */
export function Pill({
  selected,
  className,
  children,
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & { selected?: boolean }) {
  return (
    <button
      type="button"
      disabled={rest.disabled}
      className={cx(
        "pill",
        selected && "pill-active",
        rest.disabled && "cursor-not-allowed opacity-40 hover:bg-white/[0.06] hover:text-white/60",
        className,
      )}
      aria-pressed={selected}
      {...rest}
    >
      {children}
    </button>
  );
}

/** 快捷键键帽（规范 §5.10）。 */
export function Kbd({ children, className }: { children: ReactNode; className?: string }) {
  return <kbd className={cx("kbd", className)}>{children}</kbd>;
}

/** 通用「✕」小按钮：搜索清空 / 提示条关闭 / chip 移除共用。 */
export function CloseButton({
  onClick,
  title = "关闭",
  tone = "plain",
}: {
  onClick: () => void;
  title?: string;
  /** plain = 中性；danger = 悬停变红（用于移除类操作）。 */
  tone?: "plain" | "danger";
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      title={title}
      aria-label={title}
      className={cx(
        "grid h-[14px] w-[14px] shrink-0 place-items-center rounded-full",
        "text-white/40 transition-colors duration-120 ease-out-soft",
        tone === "danger"
          ? "hover:bg-white/10 hover:text-danger"
          : "hover:bg-white/10 hover:text-white",
      )}
    >
      <svg viewBox="0 0 24 24" className="h-[8px] w-[8px]" fill="none" stroke="currentColor" strokeWidth="3.5" strokeLinecap="round">
        <path d="M6 6l12 12M18 6L6 18" />
      </svg>
    </button>
  );
}

/** 表单字段：label 上、控件下（弹窗 / 表单页用）。 */
export function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="block">
      <span className="mb-[4px] block text-[12px]/[16px] text-white/60">{label}</span>
      {children}
    </label>
  );
}

/* ------------------------------------------------------------------ */
/* 开关 / 滑块 / 输入                                                  */
/* ------------------------------------------------------------------ */

/** 28×17 开关（规范 §5.2）。 */
export function Switch({
  checked,
  onChange,
  label,
  disabled,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  /** 无障碍标签：作为独立控件使用时必填。 */
  label?: string;
  /** 功能未实现时置灰（macOS 习惯：不可用控件仍可见但不响应）。 */
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      aria-disabled={disabled}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className={cx(
        "switch",
        checked ? "switch-on" : "switch-off",
        disabled && "cursor-not-allowed opacity-40",
      )}
    >
      <span className="switch-thumb" />
    </button>
  );
}

/** 带标签的设置行（label 左 / 控件右），规范 §9.3。 */
export function Row({
  label,
  hint,
  children,
}: {
  label: ReactNode;
  hint?: string;
  children: ReactNode;
}) {
  return (
    <div className="flex min-h-[40px] items-center justify-between gap-[16px]">
      <div className="min-w-0">
        <div className="truncate text-[13px]/[18px] text-white/85">{label}</div>
        {hint && <div className="mt-[2px] text-[12px]/[16px] text-white/45">{hint}</div>}
      </div>
      <div className="flex shrink-0 items-center gap-[8px]">{children}</div>
    </div>
  );
}

/** 开关 + 标签的合成行，替代旧 Toggle。 */
export function ToggleRow({
  label,
  hint,
  checked,
  onChange,
  disabled,
}: {
  label: string;
  hint?: string;
  checked: boolean;
  onChange: (v: boolean) => void;
  disabled?: boolean;
}) {
  return (
    <Row label={label} hint={hint}>
      <Switch checked={checked} onChange={onChange} label={label} disabled={disabled} />
    </Row>
  );
}

/** 滑块行：标签 + 右对齐数值 + 统一样式 range（规范 §5.3）。 */
export function SliderRow({
  label,
  value,
  min,
  max,
  step,
  format,
  onChange,
  ticks,
}: {
  label: string;
  value: number;
  min: number;
  max: number;
  step: number;
  format: (v: number) => string;
  onChange: (v: number) => void;
  ticks?: string[];
}) {
  return (
    <div className="space-y-[8px]">
      <div className="flex items-center justify-between">
        <span className="text-[13px]/[18px] text-white/85">{label}</span>
        <span className="text-[13px]/[18px] font-medium tabular-nums text-white">
          {format(value)}
        </span>
      </div>
      <input
        type="range"
        className="slider"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
        aria-label={label}
      />
      {ticks && (
        <div className="flex justify-between px-[2px] text-[11px]/[14px] text-white/40">
          {ticks.map((t) => (
            <span key={t}>{t}</span>
          ))}
        </div>
      )}
    </div>
  );
}

export const Input = forwardRef<HTMLInputElement, React.InputHTMLAttributes<HTMLInputElement>>(
  function Input({ className, ...rest }, ref) {
    return <input ref={ref} className={cx("input", className)} {...rest} />;
  },
);

export const Select = forwardRef<
  HTMLSelectElement,
  React.SelectHTMLAttributes<HTMLSelectElement>
>(function Select({ className, ...rest }, ref) {
  return (
    <select
      ref={ref}
      className={cx(
        "h-[28px] rounded-md border border-white/[0.08] bg-white/[0.05] px-[8px]",
        "text-[13px]/[18px] text-white outline-none",
        "transition-[border-color,box-shadow] duration-120 ease-out-soft",
        "focus:border-accent-500/70 focus:shadow-[0_0_0_3px_rgba(232,163,61,0.30)]",
        className,
      )}
      {...rest}
    />
  );
});

/** 带标签的下拉行。 */
export function SelectRow({
  label,
  hint,
  value,
  options,
  onChange,
  width = "w-[140px]",
}: {
  label: string;
  hint?: string;
  value: string;
  options: { value: string; label: string }[];
  onChange: (v: string) => void;
  width?: string;
}) {
  return (
    <Row label={label} hint={hint}>
      <Select
        value={value}
        aria-label={label}
        onChange={(e) => onChange(e.target.value)}
        className={width}
      >
        {options.map((o) => (
          <option key={o.value} value={o.value} className="bg-group">
            {o.label}
          </option>
        ))}
      </Select>
    </Row>
  );
}

/** 搜索框：放大镜前置图标 + 圆角输入（规范 §5.6）。 */
export function SearchField({
  value,
  onChange,
  placeholder = "搜索",
  width = "w-[240px]",
  className,
  inputRef,
}: {
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  width?: string;
  className?: string;
  /** 外部聚焦用（面板唤出即聚焦搜索）。 */
  inputRef?: React.Ref<HTMLInputElement>;
}) {
  // 输入法组合期间（中文拼音未上屏）不向上抛：避免每个拼音字母触发一次查询。
  const composing = useRef(false);
  const [local, setLocal] = useState(value);

  useEffect(() => {
    if (!composing.current) setLocal(value);
  }, [value]);

  return (
    <div className={cx("relative", width, className)}>
      <svg
        viewBox="0 0 24 24"
        aria-hidden
        className="pointer-events-none absolute left-[8px] top-1/2 h-[13px] w-[13px] -translate-y-1/2 text-white/40"
        fill="none"
        stroke="currentColor"
        strokeWidth="2.2"
        strokeLinecap="round"
      >
        <circle cx="11" cy="11" r="7" />
        <path d="m20 20-3.5-3.5" />
      </svg>
      <input
        ref={inputRef}
        value={local}
        onChange={(e) => {
          setLocal(e.target.value);
          if (!composing.current) onChange(e.target.value);
        }}
        onCompositionStart={() => {
          composing.current = true;
        }}
        onCompositionEnd={(e) => {
          composing.current = false;
          onChange(e.currentTarget.value);
        }}
        placeholder={placeholder}
        aria-label={placeholder}
        className="input pl-[26px] pr-[24px]"
      />
      {local && (
        <div className="absolute right-[6px] top-1/2 -translate-y-1/2">
          <CloseButton
            title="清空搜索"
            onClick={() => {
              setLocal("");
              onChange("");
            }}
          />
        </div>
      )}
    </div>
  );
}

/* ------------------------------------------------------------------ */
/* 分组卡 / 提示条 / 空状态                                            */
/* ------------------------------------------------------------------ */

/** 设置分组卡：无边框、#2C2C2E 填充、无阴影（规范 §9.3）。 */
export function CardGroup({
  title,
  subtitle,
  action,
  className,
  children,
}: {
  title?: string;
  subtitle?: string;
  /** 标题右侧的操作区（如「刷新」按钮）。 */
  action?: ReactNode;
  className?: string;
  children: ReactNode;
}) {
  return (
    <section className={cx("rounded-[10px] bg-group p-[16px]", className)}>
      {(title || action) && (
        <div className={cx("mb-[12px] flex items-start justify-between gap-[12px]")}>
          {title ? (
            <h4 className="text-[13px]/[18px] font-semibold text-white">{title}</h4>
          ) : (
            <span />
          )}
          {action}
        </div>
      )}
      {subtitle && <p className="-mt-[8px] mb-[12px] text-[12px]/[16px] text-white/45">{subtitle}</p>}
      {children}
    </section>
  );
}

/** 分组内分隔线。 */
export function Divider({ className }: { className?: string }) {
  return <div className={cx("h-px bg-white/[0.08]", className)} />;
}

type NoticeTone = "success" | "danger" | "info";

const NOTICE_TONE: Record<NoticeTone, string> = {
  success: "bg-success/[0.12] text-success-soft",
  danger: "bg-danger-fill/[0.12] text-danger-soft",
  info: "bg-white/[0.06] text-white/70",
};

/** 12% 透明度语义色提示条，替代旧的高饱和横幅（规范 §9.4）。 */
export function Notice({
  tone = "info",
  children,
  onClose,
  className,
}: {
  tone?: NoticeTone;
  children: ReactNode;
  onClose?: () => void;
  className?: string;
}) {
  return (
    <div
      role="status"
      className={cx(
        "flex min-h-[36px] items-center gap-[8px] rounded-[8px] px-[12px] py-[8px]",
        "text-[12px]/[16px] animate-fade-in",
        NOTICE_TONE[tone],
        className,
      )}
    >
      <span className="min-w-0 flex-1">{children}</span>
      {onClose && <CloseButton title="关闭提示" onClick={onClose} />}
    </div>
  );
}

/** 空状态：图标 + 双行文案（规范 §9.1/§9.5）。 */
export function EmptyState({
  icon,
  title,
  hint,
  compact,
}: {
  icon?: ReactNode;
  title: string;
  hint?: string;
  compact?: boolean;
}) {
  return (
    <div
      className={cx(
        "flex flex-col items-center justify-center gap-[8px] text-center",
        compact ? "py-[24px]" : "py-[32px]",
      )}
    >
      {icon && <div className="text-white/30">{icon}</div>}
      <p className="text-[13px]/[18px] text-white/45">{title}</p>
      {hint && <p className="text-[12px]/[16px] text-white/[0.28]">{hint}</p>}
    </div>
  );
}

/** 旋转指示器（加载态）。 */
export function Spinner({ size = 14 }: { size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      className="animate-spin"
      fill="none"
      aria-hidden
    >
      <circle cx="12" cy="12" r="9" stroke="currentColor" strokeOpacity="0.25" strokeWidth="3" />
      <path
        d="M21 12a9 9 0 0 0-9-9"
        stroke="currentColor"
        strokeWidth="3"
        strokeLinecap="round"
      />
    </svg>
  );
}
