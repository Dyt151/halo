import { useSyncExternalStore } from "react";

const QUERY = "(prefers-reduced-motion: reduce)";

function subscribe(onChange: () => void): () => void {
  const mq = window.matchMedia(QUERY);
  mq.addEventListener("change", onChange);
  return () => mq.removeEventListener("change", onChange);
}

/**
 * 同步读取系统「减弱动态效果」。
 *
 * 用 `useSyncExternalStore` 而不是 effect + state：JS 驱动的动画（环形盘的
 * 错峰与收起）必须在首帧就拿到正确值，晚一帧仍会播一次位移动画。
 */
export function usePrefersReducedMotion(): boolean {
  return useSyncExternalStore(
    subscribe,
    () => window.matchMedia(QUERY).matches,
    () => false,
  );
}
