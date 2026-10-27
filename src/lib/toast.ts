/**
 * 全局 toast（评审 E1 / U11）。
 *
 * 此前「已复制 / 已置顶 / 回贴失败」这类反馈只有两种去处：状态栏里一行
 * 不会消失的小字，或者干脆没有（`CopiedOnly` 被静默丢弃）。toast 用
 * 非阻塞的浮层给出即时反馈，2.6s 自动淡出（错误停留更久并有「重试」位）。
 *
 * 用法：`toast("已复制")` / `toastError(e)` / `toast("已删除", { actionLabel: "撤销", onAction })`
 */

import { errorMessage } from "./errors";

export type ToastTone = "success" | "danger" | "info";

export interface ToastItem {
  id: number;
  message: string;
  tone: ToastTone;
  /** 可选操作按钮（如「撤销」）。 */
  actionLabel?: string;
  onAction?: () => void;
}

/** 同时最多保留的 toast 数：再多会盖住内容，也更吵。 */
const MAX_TOASTS = 3;

let seq = 0;
let items: ToastItem[] = [];
const listeners = new Set<() => void>();

function emit() {
  for (const l of listeners) l();
}

/** 订阅 toast 列表（供 <ToastHost> 用）。 */
export function subscribeToasts(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function getToasts(): ToastItem[] {
  return items;
}

export function dismissToast(id: number) {
  const next = items.filter((t) => t.id !== id);
  if (next.length === items.length) return;
  items = next;
  emit();
}

/**
 * 弹出一条 toast。返回 id，可手动 dismiss。
 *
 * 错误停留 4.5s（够读完一句话），普通提示 2.4s。
 */
export function toast(
  message: string,
  options?: { tone?: ToastTone; actionLabel?: string; onAction?: () => void; duration?: number },
): number {
  const tone = options?.tone ?? "info";
  const id = ++seq;
  const item: ToastItem = {
    id,
    message,
    tone,
    actionLabel: options?.actionLabel,
    onAction: options?.onAction,
  };
  items = [...items, item].slice(-MAX_TOASTS);
  emit();

  const duration = options?.duration ?? (tone === "danger" ? 4500 : 2400);
  if (duration > 0) window.setTimeout(() => dismissToast(id), duration);
  return id;
}

/** 把任意异常转成一条错误 toast（后端错误对象已由 `lib/ipc` 归一化为可读文案）。 */
export function toastError(e: unknown, fallback = "操作失败"): number {
  return toast(errorMessage(e, fallback), { tone: "danger" });
}
