import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { ClipItem as ClipItemType } from "../lib/types";
import { promptDialog } from "../lib/dialog";
import { cx } from "./ui";

export interface ContextMenuState {
  x: number;
  y: number;
  clip: ClipItemType;
}

export interface ModuleAction {
  module_id: string;
  module_name: string;
  label: string;
}

interface Props {
  state: ContextMenuState | null;
  availableTags: string[];
  moduleActions: ModuleAction[];
  onClose: () => void;
  onPaste: (clip: ClipItemType) => void;
  onCopy: (clip: ClipItemType) => void;
  onDelete: (clip: ClipItemType) => void;
  onTogglePin: (clip: ClipItemType, v: boolean) => void;
  onToggleFavorite: (clip: ClipItemType, v: boolean) => void;
  onPushToStack: (clip: ClipItemType) => void;
  onSetTag: (clip: ClipItemType, tag: string | null) => void;
  onModuleAction: (moduleId: string, clip: ClipItemType) => void;
}

/** 菜单与屏幕边缘的最小间距（Apple HIG）。 */
const EDGE_MARGIN = 8;

/** 复选标记：13px checkmark，琥珀色（替代旧的 ✓ 文字符号）。 */
function Check() {
  return (
    <svg
      viewBox="0 0 24 24"
      className="h-[13px] w-[13px] shrink-0 text-accent-500"
      fill="none"
      stroke="currentColor"
      strokeWidth="3"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d="m5 13 4 4L19 7" />
    </svg>
  );
}

/** 子菜单箭头（评审 U4）：有下级菜单的项右端必须有 › ，否则用户不知道还能展开。 */
function Chevron() {
  return (
    <svg
      viewBox="0 0 24 24"
      className="menu-arrow"
      fill="none"
      stroke="currentColor"
      strokeWidth="2.4"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d="m9 5 7 7-7 7" />
    </svg>
  );
}

/**
 * 右键菜单：NSMenu 观感（毛玻璃材质 + 整行琥珀高亮 + 真 checkmark + 右侧快捷键）。
 *
 * 位置在挂载后按实际尺寸做边界翻转，右/下溢出时贴反向边；标签收进子菜单
 * （评审 U4）：标签一多，平铺会把「删除」挤到屏幕外，也没有 NSMenu 的层级感。
 */
export default function ClipContextMenu({
  state,
  availableTags,
  moduleActions,
  onClose,
  onPaste,
  onCopy,
  onDelete,
  onTogglePin,
  onToggleFavorite,
  onPushToStack,
  onSetTag,
  onModuleAction,
}: Props) {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState({ left: 0, top: 0 });
  const [size, setSize] = useState({ w: 200, h: 200 });
  const [submenu, setSubmenu] = useState<"tags" | null>(null);
  // Esc 需要读到最新值，但又不能把它放进 effect 依赖（会在每次展开时重绑监听）。
  const submenuRef = useRef<"tags" | null>(null);
  useEffect(() => {
    submenuRef.current = submenu;
  }, [submenu]);

  useLayoutEffect(() => {
    if (!state) return;
    const el = ref.current;
    const w = el?.offsetWidth ?? 200;
    const h = el?.offsetHeight ?? 200;
    const maxLeft = window.innerWidth - w - EDGE_MARGIN;
    const maxTop = window.innerHeight - h - EDGE_MARGIN;
    setSize({ w, h });
    setPos({
      left: Math.max(EDGE_MARGIN, Math.min(state.x, maxLeft)),
      top: Math.max(EDGE_MARGIN, Math.min(state.y, maxTop)),
    });
    setSubmenu(null);
  }, [state]);

  useEffect(() => {
    if (!state) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      // 捕获相位 + 阻断传播：面板窗口自己也监听 Esc（收起面板），
      // 不拦的话「菜单开着按 Esc」会把菜单和面板一起关掉。
      e.preventDefault();
      e.stopPropagation();
      // Esc 先收子菜单，再收整个菜单（与 NSMenu 的逐层退出一致）。
      if (submenuRef.current) {
        setSubmenu(null);
        return;
      }
      onClose();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [state, onClose]);

  if (!state) return null;

  const run = (action: () => void) => () => {
    action();
    onClose();
  };

  const item = (
    label: string,
    action: () => void,
    opts?: { danger?: boolean; mark?: boolean; shortcut?: string; title?: string },
    /** 列表 key：模块动作可能重名（两个模块都叫「翻译」），不能只拿 label 当 key。 */
    keyName?: string,
  ) => (
    <button
      key={keyName ?? label}
      type="button"
      onClick={run(action)}
      title={opts?.title}
      className={cx("menu-item", opts?.danger && "menu-item-danger")}
      role="menuitem"
    >
      <span className="flex-1 truncate text-left">{label}</span>
      {opts?.shortcut && <span className="menu-shortcut">{opts.shortcut}</span>}
      {opts?.mark && <Check />}
    </button>
  );

  // 子菜单朝右放不下时改为朝左（贴屏幕右缘的右键菜单）。
  const submenuOnLeft = pos.left + size.w + size.w > window.innerWidth - EDGE_MARGIN;

  return (
    <div className="fixed inset-0 z-50" onClick={onClose} onContextMenu={(e) => e.preventDefault()}>
      <div
        ref={ref}
        className="menu absolute animate-pop-in"
        // 不能在这里加 max-height + overflow：标签子菜单要飞出父级边界（Apple HIG），
        // 一旦父级变成滚动容器就会被裁掉。超高时靠 useLayoutEffect 的边界钳制兜底。
        style={{ left: pos.left, top: pos.top }}
        onClick={(e) => e.stopPropagation()}
        role="menu"
      >
        {item("回贴", () => onPaste(state.clip), { shortcut: "⏎" })}
        {item("复制", () => onCopy(state.clip), { shortcut: "⌘C" })}
        {item("加入堆栈", () => onPushToStack(state.clip))}

        <div className="menu-sep" />
        {item(
          state.clip.pinned ? "取消置顶" : "置顶",
          () => onTogglePin(state.clip, !state.clip.pinned),
          { mark: state.clip.pinned },
        )}
        {item(
          state.clip.favorite ? "取消收藏" : "收藏",
          () => onToggleFavorite(state.clip, !state.clip.favorite),
          { mark: state.clip.favorite, shortcut: "⌘D" },
        )}

        <div className="menu-sep" />
        {/* 标签子菜单：hover 展开，点击也可切换（键盘/触控板都可达）。 */}
        <div
          className="relative"
          onMouseEnter={() => setSubmenu("tags")}
          onMouseLeave={() => setSubmenu(null)}
        >
          <button
            type="button"
            className={cx("menu-item", submenu === "tags" && "menu-item-active")}
            onClick={() => setSubmenu((cur) => (cur === "tags" ? null : "tags"))}
            aria-haspopup="menu"
            aria-expanded={submenu === "tags"}
            role="menuitem"
          >
            <span className="flex-1 truncate text-left">标签</span>
            {state.clip.tag && (
              <span className="shrink-0 text-[12px]/[16px] text-white/40">{state.clip.tag}</span>
            )}
            <Chevron />
          </button>

          {submenu === "tags" && (
            <div
              className={cx(
                "menu absolute top-[-5px] animate-pop-in",
                submenuOnLeft ? "right-full mr-[2px]" : "left-full ml-[2px]",
              )}
              role="menu"
            >
              <div className="scroll-apple max-h-[300px] overflow-y-auto">
                {availableTags.length === 0 && (
                  <div className="px-[10px] py-[6px] text-[12px]/[16px] text-white/40">
                    还没有标签
                  </div>
                )}
                {availableTags.map((tag) => (
                  <button
                    key={tag}
                    type="button"
                    role="menuitem"
                    onClick={run(() => onSetTag(state.clip, state.clip.tag === tag ? null : tag))}
                    className="menu-item"
                  >
                    <span className="flex-1 truncate text-left">{tag}</span>
                    {state.clip.tag === tag && <Check />}
                  </button>
                ))}
              </div>
              <div className="menu-sep" />
              <button
                type="button"
                role="menuitem"
                className="menu-item"
                onClick={() => {
                  void promptDialog("新标签名称").then((name) => {
                    const trimmed = name?.trim();
                    if (trimmed) onSetTag(state.clip, trimmed);
                  });
                  onClose();
                }}
              >
                <span className="flex-1 truncate text-left">新建标签…</span>
              </button>
              {state.clip.tag && (
                <button
                  type="button"
                  role="menuitem"
                  className="menu-item"
                  onClick={run(() => onSetTag(state.clip, null))}
                >
                  <span className="flex-1 truncate text-left">从标签移除</span>
                </button>
              )}
            </div>
          )}
        </div>

        {moduleActions.length > 0 && <div className="menu-sep" />}
        {moduleActions.map((action) =>
          item(
            action.label,
            () => onModuleAction(action.module_id, state.clip),
            // 多个模块可能提供同名动作：悬停给出模块名，才知道是谁在跑。
            { title: `来自模块「${action.module_name}」` },
            `mod:${action.module_id}:${action.label}`,
          ),
        )}

        <div className="menu-sep" />
        {item("删除", () => onDelete(state.clip), { danger: true, shortcut: "⌘⌫" })}
      </div>
    </div>
  );
}
