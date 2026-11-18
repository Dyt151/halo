import { useSyncExternalStore } from "react";
import { dismissToast, getToasts, subscribeToasts } from "../lib/toast";
import { cx } from "./ui";

const TONE: Record<string, string> = {
  success: "border-success/40 text-success-soft",
  danger: "border-danger-fill/45 text-danger",
  info: "border-white/[0.12] text-white/85",
};

/**
 * toast 浮层（评审 E1 / U11）。
 *
 * 位置：面板里贴底部（状态栏上方），管理窗口贴右下角 —— 两个窗口都用同一份
 * 组件，靠 `variant` 区分落点。点击任意 toast 立即关掉（不阻塞操作）。
 */
export default function ToastHost({ variant = "floating" }: { variant?: "floating" | "panel" }) {
  const items = useSyncExternalStore(subscribeToasts, getToasts, getToasts);
  if (items.length === 0) return null;

  return (
    <div
      className={cx(
        "pointer-events-none fixed z-[70] flex flex-col items-center gap-[8px]",
        variant === "panel" ? "inset-x-0 bottom-[44px]" : "bottom-[24px] right-[24px] items-end",
      )}
      role="status"
      aria-live="polite"
    >
      {items.map((t) => (
        <div
          key={t.id}
          className={cx(
            "pointer-events-auto flex max-w-[420px] animate-toast-in items-center gap-[10px]",
            "rounded-[9px] border px-[12px] py-[8px] text-[12px]/[16px]",
            "toast-surface shadow-menu",
            TONE[t.tone] ?? TONE.info,
          )}
          onClick={() => dismissToast(t.id)}
        >
          <span className="min-w-0 flex-1 break-words">{t.message}</span>
          {t.actionLabel && (
            <button
              type="button"
              className="shrink-0 rounded-[5px] px-[6px] py-[1px] text-[12px] font-medium text-accent-400 hover:bg-white/10"
              onClick={(e) => {
                e.stopPropagation();
                t.onAction?.();
                dismissToast(t.id);
              }}
            >
              {t.actionLabel}
            </button>
          )}
        </div>
      ))}
    </div>
  );
}
