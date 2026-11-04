/**
 * 输入法切换提示（`imehud` 窗口）。
 *
 * 切输入法时系统只改菜单栏那个小符号，用户低头看屏幕时并不知道现在是什么输入
 * 法。这里在光标所在屏的底部居中浮一枚胶囊：字牌 + 中文名 + 位置圆点，
 * 进 200ms / 停 1100ms / 出 240ms，三段时长与 Rust 的收窗时间一致。
 *
 * 窗口从不抢焦点：Rust 用 `show()` + `set_ignore_cursor_events(true)`
 * （与环形盘同一套已验证的做法），所以这里不收任何键盘/鼠标事件。
 */
import { useEffect, useRef, useState } from "react";
import { onInputSourceChanged } from "../lib/windows";
import { InputSourceMarker } from "../components/InputSourceMarker";
import type { InputSourceHint } from "../lib/types";

/** 出场开始的时刻：进场 200 + 停留 1100，与 `INPUT_HUD_TOTAL_MS` 对齐。 */
const OUT_AT_MS = 200 + 1100;

/** 圆点最多画这么多枚，再多就只留名称（超过就说明列表太长，点位没有意义）。 */
const MAX_DOTS = 8;

/** 无标记输入源的替代符号：macOS 自己用的是「地球 + 文」。 */
function GlobeGlyph() {
  return (
    <svg
      width="15"
      height="15"
      viewBox="0 0 24 24"
      className="shrink-0 text-white"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.7}
      aria-hidden
    >
      <circle cx="12" cy="12" r="8.5" />
      <path d="M3.5 12h17M12 3.5c2.6 2.4 2.6 14.6 0 17-2.6-2.4-2.6-14.6 0-17Z" />
    </svg>
  );
}

export default function InputHud() {
  const [hint, setHint] = useState<InputSourceHint | null>(null);
  /** 每次切换换一个 key，让进场动画从头再播一次（连续快切也要重新起算）。 */
  const [runId, setRunId] = useState(0);
  const [leaving, setLeaving] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    let disposed = false;
    let unsubscribed: (() => void) | undefined;
    void onInputSourceChanged((next) => {
      if (disposed) return;
      if (timer.current) clearTimeout(timer.current);
      setLeaving(false);
      setHint(next);
      setRunId((id) => id + 1);
      timer.current = setTimeout(() => setLeaving(true), OUT_AT_MS);
    }).then((fn) => {
      if (disposed) fn();
      else unsubscribed = fn;
    });
    return () => {
      disposed = true;
      if (timer.current) clearTimeout(timer.current);
      unsubscribed?.();
    };
  }, []);

  if (!hint) return <div className="app-surface--hud h-full w-full" />;

  const showsDots = hint.total >= 2 && hint.total <= MAX_DOTS && hint.index >= 1;

  return (
    <div className="app-surface--hud flex h-full w-full items-center justify-center">
      <div key={runId} className={leaving ? "ime-hud-out" : "ime-hud-in"}>
        <span className="grid h-[16px] w-[20px] shrink-0 place-items-center">
          <InputSourceMarker marker={hint.marker} fallback={<GlobeGlyph />} />
        </span>
        <span className="min-w-0 truncate text-[13px]/none font-semibold text-white">
          {hint.name}
        </span>
        {showsDots && (
          <span className="flex shrink-0 items-center gap-[4px]" aria-hidden>
            {Array.from({ length: hint.total }, (_, i) => (
              <span
                key={i}
                className={
                  i + 1 === hint.index
                    ? "h-[5px] w-[5px] rounded-full bg-white"
                    : "h-[5px] w-[5px] rounded-full bg-white/30"
                }
              />
            ))}
          </span>
        )}
      </div>
    </div>
  );
}
