import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import { closeDialog, getDialog, subscribeDialog } from "../lib/dialog";
import { Btn } from "./ui";

/**
 * 应用内对话框宿主：渲染 confirm / prompt / alert。
 *
 * 评审 U1：按钮此前只写了 `btn-secondary` / `btn-primary` 两个「颜色类」，
 * 漏了 `.btn` 基类本身（高度/内边距/圆角/按压动画全在基类上），所以按钮
 * 看起来是塌陷的一行文字。现在统一走 <Btn>，与设置页按钮同源。
 * 评审 U2：破坏性确认的确定键改为红色 danger 变体。
 * 评审 U3：改为窗口顶部居中的 sheet 观感 + 出现动画（macOS 的 sheet 是从
 * 标题栏下滑出来的，不是网页那种飘在正中的浮层）。
 */
export default function DialogHost() {
  const dialog = useSyncExternalStore(subscribeDialog, getDialog, getDialog);
  const [value, setValue] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);
  const cancelRef = useRef<HTMLButtonElement>(null);
  const confirmRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (dialog?.kind === "prompt") {
      setValue(dialog.defaultValue ?? "");
    }
  }, [dialog]);

  useEffect(() => {
    if (!dialog) return;
    const onKey = (e: KeyboardEvent) => {
      // Esc 恒等于「取消」（含 prompt：关闭且不写入）。
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        closeDialog(null);
      }
      // Enter 不再统一确认：确认框默认焦点在「取消」，Enter 应激活当前聚焦的
      // 按钮。否则用户回车本想取消，实际执行了删除这类不可逆操作。
    };
    // capture 阶段拦截：避免底下的面板/列表也收到 Esc / Enter（评审 S3 同类问题）。
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [dialog]);

  // 默认焦点：prompt → 输入框；confirm → 「取消」（防误删）；alert → 「确定」。
  useEffect(() => {
    if (!dialog) return;
    if (dialog.kind === "prompt") inputRef.current?.focus();
    else if (dialog.kind === "confirm") cancelRef.current?.focus();
    else confirmRef.current?.focus();
  }, [dialog]);

  if (!dialog) return null;

  const submit = () => {
    if (dialog.kind === "prompt") closeDialog(value);
    else closeDialog(true);
  };

  const okLabel = dialog.okLabel ?? (dialog.kind === "alert" ? "好" : "确定");
  const cancelLabel = dialog.cancelLabel ?? "取消";

  /** 焦点陷阱：Tab/⇧Tab 只在弹窗内循环，不回到底层页面。 */
  const trapTab = (e: React.KeyboardEvent<HTMLDivElement>) => {
    if (e.key !== "Tab") return;
    const nodes = e.currentTarget.querySelectorAll<HTMLElement>(
      'button:not([disabled]), input:not([disabled]), [href], select, textarea, [tabindex]:not([tabindex="-1"])',
    );
    if (nodes.length === 0) return;
    const first = nodes[0];
    const last = nodes[nodes.length - 1];
    if (!e.shiftKey && document.activeElement === last) {
      e.preventDefault();
      first.focus();
    } else if (e.shiftKey && document.activeElement === first) {
      e.preventDefault();
      last.focus();
    }
  };

  // 模态期间阻断背景交互：遮罩自身吞掉指针事件，且「点遮罩」不关闭（误触保护）。
  return (
    <div
      className="fixed inset-0 z-[60] flex items-start justify-center bg-black/35 pt-[64px]"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) e.preventDefault();
      }}
      onClick={(e) => e.stopPropagation()}
      onContextMenu={(e) => e.preventDefault()}
    >
      <div
        className="menu w-[340px] animate-sheet-in rounded-[10px] p-[16px] shadow-modal"
        role="alertdialog"
        aria-modal="true"
        aria-label={dialog.title ?? dialog.message}
        onKeyDown={trapTab}
      >
        {dialog.title && (
          <h2 className="mb-[8px] text-[13px]/[18px] font-semibold text-white">{dialog.title}</h2>
        )}
        <p className="mb-[14px] whitespace-pre-wrap break-words text-[13px]/[1.5] text-white/80">
          {dialog.message}
        </p>

        {dialog.kind === "prompt" && (
          <input
            ref={inputRef}
            value={value}
            onChange={(e) => setValue(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") submit();
              if (e.key === "Escape") closeDialog(null);
            }}
            placeholder={dialog.placeholder ?? "输入名称…"}
            className="input mb-[14px] w-full"
          />
        )}

        <div className="flex justify-end gap-[8px]">
          {dialog.kind !== "alert" && (
            <button
              ref={cancelRef}
              type="button"
              className="btn btn-secondary"
              onClick={() => closeDialog(null)}
            >
              {cancelLabel}
            </button>
          )}
          <Btn
            ref={confirmRef}
            variant={dialog.danger ? "danger" : "primary"}
            onClick={submit}
          >
            {okLabel}
          </Btn>
        </div>
      </div>
    </div>
  );
}
