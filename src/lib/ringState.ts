/**
 * 环形盘（ring 窗口）状态机的纯决策部分。
 *
 * `RingWindow.tsx` 的状态机高度依赖 Tauri 事件、React state/ref 和定时器，
 * 但其中「初始高亮选谁」「中心标题怎么写」「松手落在哪、该执行还是拒绝」
 * 是不带副作用、可以直接确定的规则。抽成纯函数既能单元测试，也让
 * 运行态与设置页（RingEditor）共享同一套语义的意图更清晰。
 */

import type { RingAction } from "./types";

/**
 * 初始高亮：记住的上次动作 → 第一个可用扇区。
 *
 * 优先「可用」是关键：如果上次用的是图片预设，这次拖的是文本，那一条会被
 * 禁用（占着原位），高亮它会让中心一上来就显示一串「不可用」的原因。
 */
export function initialIndex(actions: RingAction[], suggested: string | null): number {
  if (suggested) {
    const i = actions.findIndex((a) => a.id === suggested && a.enabled);
    if (i >= 0) return i;
  }
  const first = actions.findIndex((a) => a.enabled);
  return first < 0 ? 0 : first;
}

/**
 * 中心主标题：数量 + 格式（Wheel §1.3 A）。
 *
 * 单一格式写「3 个 PNG」，同类混选写「3 张图片」，跨类才写混合——
 * 选了 20 张 PNG 还只写「图片」等于什么都没说。
 */
export function ringCenterTitle(count: number, category: string | null): string {
  if (!category) return `${count} 个文件`;
  if (category === "图片") return `${count} 张图片`;
  if (category === "文本") return `${count} 个文本`;
  if (category === "混合文件") return `${count} 个文件 · 混合`;
  return `${count} 个 ${category}`;
}

/** 松手决策：关闭 / 执行 / 拒绝（禁用扇区把原因摆到中心）。 */
export type RingDragEndDecision =
  | { kind: "close" }
  | { kind: "execute" }
  | { kind: "reject"; reason: string };

/**
 * 松手时的决策（§3.3）。
 *
 * 松手按最后一次命中决定：落在扇区上就执行，其余（中心 / 环外 / 空环 /
 * 从未命中）都算取消。命中禁用扇区时不静默吞掉，而是把原因返回让组件
 * 摆在中心留 1.3s。
 */
export function resolveRingDragEnd(
  lastHit: "segment" | "center" | "outside" | "empty" | null,
  action: RingAction | undefined,
  defaultReason = "这个动作在这批文件上不可用",
): RingDragEndDecision {
  if (lastHit !== "segment") return { kind: "close" };
  if (!action) return { kind: "close" };
  if (action.enabled) return { kind: "execute" };
  return { kind: "reject", reason: action.disabled_reason ?? defaultReason };
}
