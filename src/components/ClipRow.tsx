import { convertFileSrc } from "@tauri-apps/api/core";
import { fileBasename, formatRelativeTime, formatSize, typeLabel } from "../lib/format";
import type { ClipItem as ClipItemType } from "../lib/types";
import { CardIconBtn, cx, Kbd } from "./ui";

interface Props {
  clip: ClipItemType;
  hotkey?: number;
  selected?: boolean;
  keyword?: string;
  onPaste: () => void;
  onCopy: () => void;
  onDelete: () => void;
  onToggleFavorite: (v: boolean) => void;
  onContextMenu: (e: React.MouseEvent) => void;
  onOpenDetail?: () => void;
}

const TYPE_GLYPH: Record<ClipItemType["content_type"], string> = {
  text: "T",
  html: "</>",
  image: "",
  files: "⌘",
  unknown: "?",
};

function rowTitle(clip: ClipItemType): string {
  if (clip.content.kind === "files") {
    const names = clip.content.value.map(fileBasename);
    if (names.length === 1) return names[0];
    return `${names[0] ?? "文件"} 等 ${names.length} 个文件`;
  }
  if (clip.content_type === "image") {
    const dims =
      clip.content.kind === "image" ? `${clip.content.value.width}×${clip.content.value.height}` : "";
    return dims ? `图片 ${dims}` : "图片";
  }
  if (clip.content_type === "html") return clip.preview || "网页片段";
  const text = clip.content.kind === "text" ? clip.content.value : clip.preview;
  return text.replace(/\s+/g, " ").trim() || clip.preview || "（空）";
}

function RowThumb({ clip }: { clip: ClipItemType }) {
  if (clip.content.kind === "image") {
    return (
      <img
        src={convertFileSrc(clip.content.value.path)}
        alt=""
        draggable={false}
        className="h-7 w-9 shrink-0 rounded-[5px] border border-white/[0.06] object-cover"
      />
    );
  }
  return (
    <span
      aria-hidden
      className={cx(
        "grid h-7 w-7 shrink-0 place-items-center rounded-[6px] border border-white/[0.06]",
        "bg-white/[0.045] text-[11px] font-semibold text-white/55",
        clip.content_type === "files" && "text-[#e8a33d]",
      )}
    >
      {TYPE_GLYPH[clip.content_type]}
    </span>
  );
}

/**
 * 面板行：44px 固定高度。比卡片多容纳约 3 倍条目，时间右对齐、
 * 悬停时才浮出操作，是面板的默认形态。
 */
export default function ClipRow({
  clip,
  hotkey,
  selected,
  keyword,
  onPaste,
  onCopy,
  onDelete,
  onToggleFavorite,
  onContextMenu,
  onOpenDetail,
}: Props) {
  const title = rowTitle(clip);
  const meta = [typeLabel(clip.content_type), clip.source_app ?? "未知来源"]
    .filter(Boolean)
    .join(" · ");

  return (
    <div
      role="option"
      aria-selected={!!selected}
      tabIndex={-1}
      data-clip-card
      title={title}
      onContextMenu={onContextMenu}
      onClick={(e) => {
        if (e.detail === 1) onCopy();
      }}
      onDoubleClick={onPaste}
      className={cx(
        "group relative flex h-11 w-full cursor-default items-center gap-2.5 rounded-[6px] px-2",
        "transition-colors duration-120 ease-out-soft",
        selected
          ? "bg-[var(--accent-tint)] shadow-[inset_2px_0_0_var(--accent-500)]"
          : "hover:bg-white/[0.05]",
      )}
    >
      <RowThumb clip={clip} />

      <div className="min-w-0 flex-1">
        <p
          className={cx(
            "truncate text-[13px]/[17px]",
            selected ? "text-[var(--text-primary)]" : "text-white/90",
          )}
        >
          {keyword ? highlight(title, keyword) : title}
        </p>
        <p className="truncate text-[11px]/[14px] text-white/40">
          {meta}
          <span className="px-1.5 text-white/25">·</span>
          <span className="tabular-nums">{formatSize(clip.size_bytes)}</span>
        </p>
      </div>

      {hotkey && hotkey <= 9 ? (
        <Kbd className="shrink-0 transition-opacity group-hover:opacity-0">{`⌘${hotkey}`}</Kbd>
      ) : null}

      <span className="shrink-0 text-[11px]/[14px] tabular-nums text-white/35 transition-opacity group-hover:opacity-0">
        {formatRelativeTime(clip.created_at)}
      </span>

      <div
        className="absolute inset-y-0 right-1 flex items-center gap-0.5 opacity-0 transition-opacity duration-120 group-hover:opacity-100"
        onClick={(e) => e.stopPropagation()}
        onDoubleClick={(e) => e.stopPropagation()}
      >
        {onOpenDetail && (
          <CardIconBtn title="查看全文" onClick={onOpenDetail}>
            <svg viewBox="0 0 24 24" className="h-[13px] w-[13px]" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <path d="M15 3h6v6M9 21H3v-6M21 3l-7 7M3 21l7-7" />
            </svg>
          </CardIconBtn>
        )}
        <CardIconBtn
          title={clip.favorite ? "取消收藏" : "收藏"}
          active={clip.favorite}
          onClick={() => onToggleFavorite(!clip.favorite)}
        >
          <svg viewBox="0 0 24 24" className="h-[13px] w-[13px]" fill={clip.favorite ? "currentColor" : "none"} stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <path d="M12 3.5l2.6 5.3 5.9.9-4.3 4.2 1 5.9-5.2-2.8-5.2 2.8 1-5.9-4.3-4.2 5.9-.9z" />
          </svg>
        </CardIconBtn>
        <CardIconBtn title="复制" onClick={onCopy}>
          <svg viewBox="0 0 24 24" className="h-[13px] w-[13px]" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <rect x="9" y="9" width="12" height="12" rx="2" />
            <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
          </svg>
        </CardIconBtn>
        <CardIconBtn title="删除" danger onClick={onDelete}>
          <svg viewBox="0 0 24 24" className="h-[13px] w-[13px]" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <path d="M3 6h18M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2m3 0v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6h14z" />
          </svg>
        </CardIconBtn>
      </div>
    </div>
  );
}

/** 与卡片一致的搜索命中高亮。 */
function highlight(text: string, keyword: string): React.ReactNode {
  const query = keyword.trim();
  if (!query) return text;
  const safe = query.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return text.split(new RegExp(`(${safe})`, "gi")).map((part, i) =>
    part.toLowerCase() === query.toLowerCase() ? (
      <mark key={i} className="rounded-[2px] bg-accent-500/30 px-[1px] text-[var(--text-primary)]">
        {part}
      </mark>
    ) : (
      <span key={i}>{part}</span>
    ),
  );
}
