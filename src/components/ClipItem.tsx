import { useEffect, useMemo, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { fileBasename, formatRelativeTime, formatSize, typeLabel } from "../lib/format";
import { htmlToPlainText, sanitizeHtml } from "../lib/sanitize";
import { useClipStore } from "../store/useClipStore";
import type { ClipItem as ClipItemType } from "../lib/types";
import ClipDetail from "./ClipDetail";
import { CardIconBtn, cx, Kbd } from "./ui";

interface Props {
  clip: ClipItemType;
  /** 数字快捷键序号（1~9）；不传则不展示 ⌘N 角标（过滤激活 / 管理窗口）。 */
  hotkey?: number;
  selected?: boolean;
  /** 当前搜索词（命中的片段高亮，评审 E3）。 */
  keyword?: string;
  className?: string;
  onPaste: () => void;
  onCopy: () => void;
  onDelete: () => void;
  onToggleFavorite: (v: boolean) => void;
  onContextMenu: (e: React.MouseEvent) => void;
}

/**
 * 类型徽章配色（规范 §2.5）：14% 透明度彩底 + 同色系亮字，永不加边框。
 * 这是 Apple 深色模式徽章的标准做法。
 */
const BADGE: Record<ClipItemType["content_type"], string> = {
  text: "bg-badge-text text-badge-text-fg",
  image: "bg-badge-image text-badge-image-fg",
  files: "bg-badge-files text-badge-files-fg",
  html: "bg-badge-html text-badge-html-fg",
  unknown: "bg-badge-unknown text-badge-unknown-fg",
};

/** 来源 App 图标：有导出文件时显示 16px 圆角小图，否则降级为首字母圆标。 */
function AppIcon({ clip }: { clip: ClipItemType }) {
  // 无图标时降级为首字母圆标（报告中提到的「L」图标）：
  // 必须带可见语义（title/aria-label），否则就是无图例的视觉噪声。
  const label = clip.source_app ? `来源：${clip.source_app}` : undefined;
  if (clip.source_app_icon) {
    return (
      <img
        src={convertFileSrc(clip.source_app_icon)}
        alt=""
        draggable={false}
        title={label}
        aria-label={label}
        className="h-4 w-4 shrink-0 rounded-[5px] object-contain"
      />
    );
  }
  if (clip.source_app) {
    return (
      <span
        title={label}
        aria-label={label}
        className="grid h-4 w-4 shrink-0 place-items-center rounded-[5px] bg-white/[0.10] text-[11px]/[14px] font-semibold leading-none text-white/60"
      >
        {clip.source_app.charAt(0).toUpperCase()}
      </span>
    );
  }
  return null;
}

/** 搜索命中高亮（评审 E3）：把关键词包成 <mark>，不改变其余文本结构。 */
function highlight(text: string, keyword: string): React.ReactNode {
  const query = keyword.trim();
  if (!query) return text;
  const safeQuery = query.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const parts = text.split(new RegExp(`(${safeQuery})`, "gi"));
  return parts.map((part, i) =>
    part.toLowerCase() === query.toLowerCase() ? (
      <mark
        key={i}
        className="rounded-[2px] bg-accent-500/35 px-[1px] text-white"
      >
        {part}
      </mark>
    ) : (
      <span key={i}>{part}</span>
    ),
  );
}

/** 中部预览：图片铺满、文件列路径、文本 12px 近白（规范：预览是主角）。 */
function Preview({ clip, keyword }: { clip: ClipItemType; keyword: string }) {
  const textPreview = useClipStore((s) => s.config?.text_preview) ?? "rich";

  // 评审 F1：HTML 先用白名单消毒再进 innerHTML；不进 memo 的话每次父级
  // 重渲染（滚动/选中变化）都会重新解析一遍 DOM，500 张卡片下就是卡顿源。
  const safeHtml = useMemo(
    () =>
      clip.content.kind === "text" && clip.content_type === "html"
        ? sanitizeHtml(clip.content.value)
        : "",
    [clip],
  );

  if (clip.content.kind === "image") {
    return (
      <img
        src={convertFileSrc(clip.content.value.path)}
        alt=""
        draggable={false}
        className="h-full w-full rounded-[6px] object-cover"
      />
    );
  }

  if (clip.content.kind === "files") {
    // 文件卡片只展示文件名（不展示 hash/完整路径）；超过 3 个折叠计数。
    const names = clip.content.value.map(fileBasename);
    const shown = names.slice(0, 3);
    return (
      <div className="space-y-[4px]">
        {shown.map((name, i) => (
          <div
            key={`${name}-${i}`}
            className="truncate rounded-[5px] bg-white/[0.06] px-[6px] py-[4px] text-[11px]/[14px] text-white/70"
          >
            {name}
          </div>
        ))}
        {names.length > shown.length && (
          <div className="px-[6px] text-[11px]/[14px] text-white/45">
            等 {names.length} 个文件
          </div>
        )}
      </div>
    );
  }

  if (clip.content_type === "html" && clip.content.kind === "text") {
    // text_preview = plain 时按纯文本展示（去掉标签），与设置项保持一致。
    if (textPreview === "plain") {
      const plain = htmlToPlainText(clip.content.value) || clip.preview;
      return (
        <p className="line-clamp-[9] whitespace-pre-wrap break-words text-[12px]/[1.5] text-preview">
          {keyword ? highlight(plain, keyword) : plain || "（空）"}
        </p>
      );
    }
    return (
      <div
        className="clip-rich-preview line-clamp-[9] break-words text-[12px]/[1.5] text-preview"
        dangerouslySetInnerHTML={{ __html: safeHtml }}
      />
    );
  }

  const text = clip.content.kind === "text" ? clip.content.value : clip.preview;
  return (
    <p className="line-clamp-[9] whitespace-pre-wrap break-words text-[12px]/[1.5] text-preview">
      {keyword ? highlight(text || "", keyword) : text || "（空）"}
    </p>
  );
}

/**
 * 剪贴板卡片：固定三段（头部 28px / 预览 flex-1 / 底部 30px）。
 * 宽度由外部容器决定（面板 168px、管理窗口自适应）。
 */
export default function ClipItem({
  clip,
  hotkey,
  selected,
  keyword = "",
  className = "",
  onPaste,
  onCopy,
  onDelete,
  onToggleFavorite,
  onContextMenu,
}: Props) {
  // 回贴是不可逆的对外操作：默认单击仅复制，双击/Enter 才回贴（可在设置里改）。
  const pasteOnSingleClick = useClipStore((s) => s.config?.paste_on_single_click) ?? false;
  // 评审 U8：卡片高度固定，长内容只能截断；给一个「查看全文」入口。
  const [detailOpen, setDetailOpen] = useState(false);
  // 双击回贴时浏览器会先派发两次 click：若单击直接 copy，回贴就会读到
  // 尚未写回的新内容（复制是异步的），还会弹两条 toast。所以单击复制
  // 延后到双击判定窗口之后再执行，双击则取消这次复制。
  const clickTimer = useRef<number | null>(null);
  const clearClickTimer = () => {
    if (clickTimer.current !== null) {
      window.clearTimeout(clickTimer.current);
      clickTimer.current = null;
    }
  };
  useEffect(() => clearClickTimer, []);
  // 文件/长文本才需要全文视图（短文本直接看得到全部）。
  const canExpand =
    clip.content.kind === "files" ||
    (clip.content.kind === "text" && clip.content.value.length > 160);

  return (
    <>
    <article
      role="group"
      tabIndex={0}
      data-clip-card
      aria-label={pasteOnSingleClick ? "回贴此条目" : "单击复制，双击回贴"}
      onContextMenu={onContextMenu}
      onClick={(e) => {
        if (pasteOnSingleClick) {
          // 双击会连发两次 click，只认第一下，避免一次双击回贴两遍。
          if (e.detail <= 1) onPaste();
          return;
        }
        if (e.detail > 1) return; // 第二下交给 onDoubleClick
        clearClickTimer();
        clickTimer.current = window.setTimeout(() => {
          clickTimer.current = null;
          onCopy();
        }, 220);
      }}
      onDoubleClick={() => {
        clearClickTimer();
        // 单击回贴模式下，双击的第一下（detail=1）已经贴过了；
        // 这里再贴一次就会把同一段内容往目标 App 里插两遍。
        if (pasteOnSingleClick) return;
        onPaste();
      }}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          // 评审 S3：面板/历史页在 window 上也监听了 Enter，
          // 卡片聚焦时两者都会触发，导致一次按键回贴两次（甚至作用于不同条目）。
          e.stopPropagation();
          onPaste();
        }
      }}
      title={pasteOnSingleClick ? "点击回贴" : "单击复制，双击回贴"}
      className={cx(
        "card-clip group cursor-pointer",
        selected ? "card-clip-selected" : "hover:bg-white/[0.08]",
        className,
      )}
    >
      {/* 头部：类型徽章 + 时间 + 来源 App */}
      <div className="flex h-[28px] shrink-0 items-center gap-[6px] px-[10px]">
        <span
          className={cx(
            "inline-flex h-[18px] shrink-0 items-center rounded-[5px] px-[6px]",
            "text-[11px]/[14px] font-medium",
            BADGE[clip.content_type],
          )}
        >
          {typeLabel(clip.content_type)}
        </span>
        <span
          className="max-w-[112px] shrink-0 truncate whitespace-nowrap text-[11px]/[14px] text-white/60"
          title={`创建时间 ${formatRelativeTime(clip.created_at)}`}
        >
          {formatRelativeTime(clip.created_at)}
        </span>
        <span className="ml-auto min-w-0 truncate text-[11px]/[14px] text-white/60">
          {clip.source_app ?? ""}
        </span>
        <AppIcon clip={clip} />
      </div>

      {/* 中部：预览（占卡片高度 ≥60%） */}
      <div className="min-h-0 flex-1 overflow-hidden px-[10px] py-[4px]">
        <Preview clip={clip} keyword={keyword} />
      </div>

      {/* 底部：⌘N + 大小 + 三个紧凑操作 */}
      <div className="flex h-[30px] shrink-0 items-center gap-[6px] px-[10px]">
        {hotkey && hotkey <= 9 ? (
          <Kbd>⌘{hotkey}</Kbd>
        ) : (
          <span className="w-[20px]" aria-hidden />
        )}
        <span className="shrink-0 text-[11px]/[14px] tabular-nums text-white/45">
          {formatSize(clip.size_bytes)}
        </span>

        {/* 操作区整体吞掉 click/dblclick：否则双击图标会冒泡到卡片，误触发回贴。 */}
        <div
          className="ml-auto flex items-center gap-[2px]"
          onClick={(e) => e.stopPropagation()}
          onDoubleClick={(e) => e.stopPropagation()}
        >
          {canExpand && (
            <CardIconBtn
              title="查看全文"
              onClick={(e) => {
                e.stopPropagation();
                setDetailOpen(true);
              }}
            >
              <svg
                viewBox="0 0 24 24"
                className="h-[13px] w-[13px]"
                fill="none"
                stroke="currentColor"
                strokeWidth="2"
                strokeLinecap="round"
                strokeLinejoin="round"
              >
                <path d="M15 3h6v6M9 21H3v-6M21 3l-7 7M3 21l7-7" />
              </svg>
            </CardIconBtn>
          )}
          <CardIconBtn
            title={clip.favorite ? "取消收藏" : "收藏"}
            active={clip.favorite}
            onClick={(e) => {
              e.stopPropagation();
              onToggleFavorite(!clip.favorite);
            }}
          >
            <svg
              viewBox="0 0 24 24"
              className="h-[13px] w-[13px]"
              fill={clip.favorite ? "currentColor" : "none"}
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
            >
              <path d="M12 3.5l2.6 5.3 5.9.9-4.3 4.2 1 5.9-5.2-2.8-5.2 2.8 1-5.9-4.3-4.2 5.9-.9z" />
            </svg>
          </CardIconBtn>
          <CardIconBtn
            title="复制"
            onClick={(e) => {
              e.stopPropagation();
              onCopy();
            }}
          >
            <svg
              viewBox="0 0 24 24"
              className="h-[13px] w-[13px]"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
            >
              <rect x="9" y="9" width="12" height="12" rx="2" />
              <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
            </svg>
          </CardIconBtn>
          {/* 删除与复制之间留间隔：降低高频操作误点删除的概率。 */}
          <span className="w-[6px]" aria-hidden />
          <CardIconBtn
            title="删除"
            danger
            onClick={(e) => {
              e.stopPropagation();
              onDelete();
            }}
          >
            <svg
              viewBox="0 0 24 24"
              className="h-[13px] w-[13px]"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
            >
              <path d="M3 6h18M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2m3 0v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6h14z" />
            </svg>
          </CardIconBtn>
        </div>
      </div>
    </article>
    {detailOpen && (
      <ClipDetail
        clip={clip}
        onClose={() => setDetailOpen(false)}
        onCopy={() => {
          onCopy();
          setDetailOpen(false);
        }}
        onPaste={() => {
          onPaste();
          setDetailOpen(false);
        }}
      />
    )}
    </>
  );
}
