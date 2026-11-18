import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "./ipc";

const TRANSPARENCY = "(prefers-reduced-transparency: reduce)";
const MOTION = "(prefers-reduced-motion: reduce)";

/**
 * 把系统的辅助功能显示选项落到根节点：
 * - `reduce-transparency`：所有半透明表面换实色；
 * - `reduce-motion`：取消位移/错峰，只保留状态变化。
 *
 * 媒体查询负责实时跟随；`NSWorkspace` 读数与变更通知兜底不支持该媒体查询的
 * WKWebView 版本（原生读数只会把开关「打开」，不会误关媒体查询的结果）。
 */
export function useDisplayPreferences(): void {
  useEffect(() => {
    const root = document.documentElement;
    const transparency = window.matchMedia(TRANSPARENCY);
    const motion = window.matchMedia(MOTION);
    let nativeReduce = false;

    const applyTransparency = () => {
      root.classList.toggle("reduce-transparency", nativeReduce || transparency.matches);
    };
    const applyMotion = () => {
      root.classList.toggle("reduce-motion", motion.matches);
    };
    applyTransparency();
    applyMotion();

    void api.reduceTransparency().then((on) => {
      nativeReduce = on;
      applyTransparency();
    }).catch(() => {});

    transparency.addEventListener("change", applyTransparency);
    motion.addEventListener("change", applyMotion);

    let unlisten: (() => void) | undefined;
    void listen<boolean>("halo://reduce-transparency", (e) => {
      nativeReduce = Boolean(e.payload);
      applyTransparency();
    }).then((fn) => {
      unlisten = fn;
    });

    return () => {
      transparency.removeEventListener("change", applyTransparency);
      motion.removeEventListener("change", applyMotion);
      unlisten?.();
    };
  }, []);
}
