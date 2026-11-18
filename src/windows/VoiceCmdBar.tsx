/**
 * 语音指令条（`cmdbar` 窗口）。
 *
 * 为什么要有这个窗口：SayAll 那类语音 App 只有一条通道——把话变成文字打进
 * **当前焦点输入框**，它没有「语音指令」能力。所以这里提供一个能被听写的输入框，
 * 听写落在框里，再由后端把这句中文解释成动作。
 *
 * 界面刻意什么都不懂：
 * - 认出「打开 / 搜索 / 粘贴」是后端的活（`core::voicecmd`）；
 * - 收起也是后端决定的：执行成功时它已经顺手把窗口收了，失败时留下让用户改口；
 * - 这里只负责每次唤起时清空 + 重新聚焦，以及把结果显示成一行字。
 */
import { useCallback, useEffect, useRef, useState } from "react";
import type { KeyboardEvent as ReactKeyboardEvent } from "react";
import { api } from "../lib/ipc";
import { errorMessage } from "../lib/errors";
import { onVoiceCmdShown } from "../lib/windows";
import { MicGlyph } from "./tray/glyphs";

/** 没有结果时的一行示例说法：让人知道该说什么，比空白框有用。 */
const HINT = "说「打开抖音」「搜索今天的天气」「粘贴栈顶」，按回车执行";

export default function VoiceCmdBar() {
  const [text, setText] = useState("");
  const [detail, setDetail] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);
  const [busy, setBusy] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  /**
   * 唤起即清场。
   *
   * 窗口是 hide 而不是销毁，React 只挂载一次，不清的话第二次弹出来还带着上一条指令
   * 的文字和结果。聚焦要等一帧：`show()` 之后系统才把键盘交过来，当场 focus() 会盖不掉。
   */
  const reset = useCallback(() => {
    setText("");
    setDetail(null);
    setFailed(false);
    setBusy(false);
    requestAnimationFrame(() => inputRef.current?.focus());
  }, []);

  useEffect(() => {
    let disposed = false;
    let unsubscribe: (() => void) | undefined;
    void onVoiceCmdShown(() => {
      if (!disposed) reset();
    }).then((fn) => {
      if (disposed) fn();
      else unsubscribe = fn;
    });
    return () => {
      disposed = true;
      unsubscribe?.();
    };
  }, [reset]);

  // Esc 收整个窗口：绑定在 window 上，焦点万一不在输入框里也还能退出。
  useEffect(() => {
    const onKey = (e: globalThis.KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      void api.voiceCmdHide();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, []);

  const execute = useCallback(async () => {
    const query = text.trim();
    if (!query || busy) return;
    setBusy(true);
    try {
      const result = await api.voiceCmdExecute(query);
      setDetail(result.outcome.detail);
      setFailed(!result.outcome.ok);
      if (!result.keep_open) setText("");
    } catch (e) {
      setDetail(errorMessage(e, "指令执行失败"));
      setFailed(true);
    } finally {
      setBusy(false);
    }
  }, [busy, text]);

  const onKeyDown = (e: ReactKeyboardEvent<HTMLInputElement>) => {
    // 中文输入法的候选确认也是回车；语音听写在框里同样可能处于组合状态，
    // 这时候的回车是给输入法用的，不能当成「执行」。
    if (e.key === "Enter" && !e.nativeEvent.isComposing) {
      e.preventDefault();
      void execute();
    }
  };

  return (
    <div className="app-surface app-surface--panel flex flex-col justify-center gap-[8px] px-[16px] py-[14px]">
      <div className="flex items-center gap-[10px]">
        <span className="grid h-[28px] w-[28px] shrink-0 place-items-center rounded-full bg-accent-tint text-accent-500">
          <MicGlyph muted={false} className="h-[15px] w-[15px]" />
        </span>
        <input
          ref={inputRef}
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={onKeyDown}
          spellCheck={false}
          autoCapitalize="off"
          autoComplete="off"
          autoCorrect="off"
          placeholder="说一条指令…"
          aria-label="语音指令输入框"
          className="h-[30px] min-w-0 flex-1 rounded-[6px] border border-white/[0.10] bg-white/[0.05] px-[10px]
                     text-[15px]/[20px] text-white outline-none placeholder:text-white/35
                     focus:border-accent-500/70 focus:shadow-[0_0_0_3px_var(--focus-ring)]"
        />
        <span className="shrink-0 text-[11px]/[15px] tabular-nums text-white/30">⏎ 执行 · ⎋ 收起</span>
      </div>
      <div
        className={
          failed
            ? "truncate pl-[38px] text-[12px]/[16px] text-danger"
            : "truncate pl-[38px] text-[12px]/[16px] text-white/45"
        }
      >
        {detail ?? (busy ? "正在执行…" : HINT)}
      </div>
    </div>
  );
}
