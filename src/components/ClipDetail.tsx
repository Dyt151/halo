import { useEffect, useMemo } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import type { ClipItem as ClipItemType } from "../lib/types";
import { fileBasename, formatRelativeTime, formatSize, typeLabel } from "../lib/format";
import { sanitizeHtml } from "../lib/sanitize";
import { Btn, CloseButton, Kbd } from "./ui";

/**
 * 全文查看浮层（评审 U8）。
 *
 * 卡片高度固定 220px，长文本只能 `line-clamp-9` 硬切，用户没有任何入口看到
 * 完整内容。这里给一个只读的全文视图：富文本走同一套白名单消毒，纯文本保留
 * 换行；底部给「复制」「回贴」两个动作，避免看完还要回去找卡片。
 */
export default function ClipDetail({
  clip,
  onClose,
  onCopy,
  onPaste,
}: {
  clip: ClipItemType;
  onClose: () => void;
  onCopy: () => void;
  onPaste: () => void;
}) {
  const html = useMemo(
    () => (clip.content.kind === "text" && clip.content_type === "html" ? sanitizeHtml(clip.content.value) : ""),
    [clip],
  );
  const text = clip.content.kind === "text" ? clip.content.value : clip.preview;

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        onClose();
      } else if (e.key === "Enter") {
        // 底部写着「回贴 ⏎」：这里必须真的能按。捕获相位拦下，
        // 否则事件会冒泡回面板，作用到「当前选中」的另一条记录。
        e.preventDefault();
        e.stopPropagation();
        onPaste();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [onClose, onPaste]);

  const meta = `${typeLabel(clip.content_type)} · ${formatSize(clip.size_bytes)} · ${formatRelativeTime(clip.created_at)}`;

  return (
    <div
      className="fixed inset-0 z-[55] flex items-center justify-center bg-black/40 p-[24px]"
      onClick={onClose}
    >
      <div
        className="menu flex h-full max-h-[560px] w-full max-w-[640px] flex-col rounded-[10px] p-0 shadow-modal"
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-modal="true"
        aria-label="查看完整内容"
      >
        <header className="flex h-[44px] shrink-0 items-center gap-[8px] border-b border-white/[0.08] px-[12px]">
          <span className="truncate text-[13px]/[18px] font-medium text-white">
            {clip.source_app ?? "剪贴板内容"}
          </span>
          <span className="shrink-0 text-[11px]/[14px] text-white/45">{meta}</span>
          <div className="ml-auto shrink-0">
            <CloseButton title="关闭（Esc）" onClick={onClose} />
          </div>
        </header>

        <div className="scroll-apple min-h-0 flex-1 overflow-y-auto px-[16px] py-[14px]">
          {clip.content.kind === "files" ? (
            <ul className="space-y-[4px]">
              {clip.content.value.map((f, i) => (
                <li
                  key={`${f}-${i}`}
                  className="break-all rounded-[5px] bg-white/[0.05] px-[8px] py-[5px] font-mono text-[12px]/[16px] text-white/75"
                >
                  {fileBasename(f)}
                </li>
              ))}
            </ul>
          ) : clip.content.kind === "image" ? (
            // 图片走 asset 协议（scope 已收敛到 $HOME/.clipbase），原图等比展示。
            <img
              src={convertFileSrc(clip.content.value.path)}
              alt={clip.preview || "剪贴板图片"}
              className="mx-auto max-h-[420px] max-w-full rounded-[6px] object-contain"
            />
          ) : html ? (
            <div
              className="clip-rich-preview whitespace-pre-wrap break-words text-[13px]/[1.6] text-preview"
              dangerouslySetInnerHTML={{ __html: html }}
            />
          ) : (
            <p className="whitespace-pre-wrap break-words text-[13px]/[1.6] text-preview">
              {text || "（空）"}
            </p>
          )}
        </div>

        <footer className="flex h-[48px] shrink-0 items-center gap-[8px] border-t border-white/[0.08] px-[12px]">
          <span className="text-[11px]/[14px] text-white/35">
            内容来自剪贴板，已按白名单过滤脚本与外部资源
          </span>
          <div className="ml-auto flex items-center gap-[8px]">
            <Btn size="sm" onClick={onCopy}>
              复制
            </Btn>
            <Btn size="sm" variant="primary" onClick={onPaste}>
              回贴 <Kbd className="ml-[2px]">⏎</Kbd>
            </Btn>
          </div>
        </footer>
      </div>
    </div>
  );
}
