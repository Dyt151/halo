/**
 * 剪贴板面板（快捷键唤出的悬浮横条）。
 *
 * 严格按 docs/UI-DESIGN.md §9.1：工具栏 44px / 标签行 34px / 卡片流 flex-1 / 状态栏 36px，
 * 外框 12px 圆角 + 毛玻璃材质，出现 200ms / 收起 140ms 动画（评审 E2 / U9）。
 */
import { useCallback, useEffect, useRef, useState } from "react";
import { useClipStore } from "../store/useClipStore";
import { useClipActions } from "../hooks/useClipActions";
import { onPanelShown, openMainWindow } from "../lib/windows";
import { confirm, getDialog } from "../lib/dialog";
import { api } from "../lib/ipc";
import { FILTERS, type FilterKey } from "../lib/filters";
import { toastError } from "../lib/toast";
import ClipItemCard from "../components/ClipItem";
import ClipContextMenu from "../components/ClipContextMenu";
import StatusBar from "../components/StatusBar";
import TagBar from "../components/TagBar";
import ToastHost from "../components/ToastHost";
import { VirtualGrid, VirtualStrip } from "../components/VirtualList";
import { Btn, EmptyState, IconBtn, SearchField, Segment, Spinner } from "../components/ui";

const Icon = ({ children }: { children: React.ReactNode }) => (
  <svg
    viewBox="0 0 24 24"
    className="h-[16px] w-[16px]"
    fill="none"
    stroke="currentColor"
    strokeWidth="1.8"
    strokeLinecap="round"
    strokeLinejoin="round"
  >
    {children}
  </svg>
);

/** 收起动画时长，必须与 tailwind `panel-out` 一致。 */
const CLOSE_ANIM_MS = 140;

export default function Panel() {
  const keyword = useClipStore((s) => s.keyword);
  const setKeyword = useClipStore((s) => s.setKeyword);
  const contentType = useClipStore((s) => s.contentType);
  const setFilter = useClipStore((s) => s.setFilter);
  const favoriteOnly = useClipStore((s) => s.favoriteOnly);
  const activeTag = useClipStore((s) => s.activeTag);
  const clearFilters = useClipStore((s) => s.clearFilters);
  const stats = useClipStore((s) => s.stats);
  const boardStyle = useClipStore((s) => s.config?.board_style) ?? "horizontal";
  const clearAll = useClipStore((s) => s.clearAll);
  const confirmOnDelete = useClipStore((s) => s.config?.confirm_on_delete) ?? true;
  const autoCloseAfterPaste = useClipStore((s) => s.config?.auto_close_after_paste) ?? true;
  /** 面板内 ⌘1~9 回贴开关。 */
  const numHotkeyEnabled = useClipStore((s) => s.config?.num_hotkey_enabled) ?? true;
  const items = useClipStore((s) => s.items);
  const loading = useClipStore((s) => s.loading);
  const error = useClipStore((s) => s.error);
  const refresh = useClipStore((s) => s.refresh);
  const paste = useClipStore((s) => s.paste);
  const copyClip = useClipStore((s) => s.copy);
  const remove = useClipStore((s) => s.remove);
  const toggleFavorite = useClipStore((s) => s.toggleFavorite);
  const captureNow = useClipStore((s) => s.captureNow);
  const flushSearch = useClipStore((s) => s.flushSearch);

  const [selected, setSelected] = useState(0);
  // 每次「面板出现」自增：作为根节点 key 重放出现动画、复位滚动。
  // 面板窗口是 hide 而非销毁，React 只挂载一次，不这么做动画只在冷启动播一遍。
  const [shownKey, setShownKey] = useState(0);
  // 收起动画（评审 U9）：先播动画，动画结束再真正 hide 窗口。
  const [closing, setClosing] = useState(false);
  /** 收起流程是否已在跑（ref 供事件闭包读，state 供渲染读）。 */
  const closingRef = useRef(false);
  /** 收起定时器：面板在 140ms 内被重新唤出时必须取消，否则会把新面板又隐藏掉。 */
  const closeTimerRef = useRef<number | null>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const searchRef = useRef<HTMLInputElement>(null);
  const { menu, setMenu, moduleActions, handlers, openMenu, availableTags } = useClipActions();

  // 后端可能出现未纳入筛选器的内容类型（如 unknown），此时回落到「全部」。
  const rawKey = favoriteOnly ? "favorite" : (contentType ?? "all");
  const activeKey: FilterKey = FILTERS.some((f) => f.key === rawKey)
    ? (rawKey as FilterKey)
    : "all";

  // 筛选条件的变更自带刷新（见 useClipStore），此处只改状态。
  const selectFilter = (key: FilterKey) => {
    const f = FILTERS.find((x) => x.key === key);
    if (!f) return;
    // 一个分段同时含「类型 + 收藏」两维：一次写入，只发一次查询。
    setFilter(f.contentType, f.favorite);
  };

  /** 收起面板：先播放 140ms 缩放淡出，再让 Rust 侧隐藏窗口。 */
  const closePanel = useCallback(() => {
    // 连按 Esc 只排一个定时器：否则会重复调 hide_panel。
    if (closingRef.current) return true;
    closingRef.current = true;
    setClosing(true);
    closeTimerRef.current = window.setTimeout(() => {
      closeTimerRef.current = null;
      closingRef.current = false;
      setClosing(false);
      void api.hidePanel().catch((e) => toastError(e, "收起面板失败"));
    }, CLOSE_ANIM_MS);
    return true;
  }, []);

  // 切筛选/改关键词：回到第一条。
  useEffect(() => {
    setSelected(0);
  }, [activeKey, keyword]);

  // 列表长度变化（后台又复制了一条 / 删掉一条）只做「不越界」修正：
  // 旧写法把长度也当成复位信号，结果用户在浏览时后台复制一下，选中项就跳回第一条。
  useEffect(() => {
    setSelected((prev) => Math.min(prev, Math.max(items.length - 1, 0)));
  }, [items.length]);

  // 过滤标签计数：来自后端全局统计，不随当前筛选变化。
  const countOf = (key: FilterKey): number => {
    if (key === "all") return stats.total;
    if (key === "favorite") return stats.favorite;
    return stats.by_type[key] ?? 0;
  };

  // 有筛选/搜索/标签时，列表序号不再等于全局 ⌘N 语义，卡片上不展示 ⌘N 角标。
  const hasFilter = Boolean(keyword.trim() || contentType || favoriteOnly || activeTag);

  const focusSearch = () => {
    searchRef.current?.focus();
    searchRef.current?.select();
  };

  useEffect(() => {
    let disposed = false;
    let unsub: (() => void) | undefined;
    /** 上一次「面板出现」处理的毫秒时间（事件与原生 focus 几乎同时到达）。 */
    let lastShownAt = 0;

    const handleShown = () => {
      // `panel-shown` 与窗口焦点是同一件事，300ms 内的第二次只算一次：
      // 重复处理会多打一次全量查询（面板每次唤出都两条请求）。
      const now = Date.now();
      if (now - lastShownAt < 300) return;
      lastShownAt = now;
      // 上一次 Esc 的收起定时器还在跑：先取消，否则它会把刚唤出的面板又藏起来。
      if (closeTimerRef.current !== null) {
        window.clearTimeout(closeTimerRef.current);
        closeTimerRef.current = null;
      }
      setShownKey((k) => k + 1);
      setClosing(false);
      closingRef.current = false;
      setSelected(0);
      listRef.current?.scrollTo({ left: 0, top: 0 });
      // 评审 G3：不再沿用上次的搜索词 —— 再次唤出却还是旧结果，
      // 用户会以为列表空了。筛选类型与标签属于「显式选择」，保留。
      //
      // 这一行同时充当「重新拉一次」：关键词为空时 `setKeyword("")` 内部直接刷新，
      // 面板隐藏期间积压的 clipboard 事件由此追上。别再额外调 refresh()，那是双查询。
      setKeyword("");
      // 面板出现即聚焦搜索框：「唤出即搜」是面板的核心心流（E1）。
      focusSearch();
    };

    void onPanelShown(handleShown).then((fn) => {
      if (disposed) fn();
      else unsub = fn;
    });
    // 兜底：webview 还没就绪（或刚重启）时 Rust 发的事件会直接丢掉，
    // 那时面板会停在「空白 + 没有任何加载动作」的状态，点一下筛选才出内容。
    // 原生焦点事件不会丢：窗口 show + set_focus 后 webview 一定会收到 focus。
    window.addEventListener("focus", handleShown);
    // 冷启动（配置里 visible:false 但被直接 show）时也聚焦一次。
    focusSearch();
    return () => {
      disposed = true;
      unsub?.();
      window.removeEventListener("focus", handleShown);
    };
  }, [setKeyword]);

  // 面板键盘导航（状态机契约 W7）：Esc 收起 / ↑↓（纵向）或 ←→（横向）移动 /
  // Enter 回贴 / ⌘F 聚焦搜索 / ⌘D 收藏 / ⌘⌫ 删除 / ⌘1~9 回贴第 N 条。
  // 搜索框聚焦时：⌘1~9 与 Enter 照常生效（面板唤出就看到搜索框，不能反而失效），
  // 方向键与 ⌘⌫ 透传给输入框（光标移动 / 删到行首）。
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      // 中文输入法用 Enter 上屏候选词：这一下也带 keydown，绝不能当成「回贴」。
      // Safari/WKWebView 里 isComposing 在 keydown 上可能为 false，故再看 keyCode 229。
      if (e.isComposing || e.keyCode === 229) return;
      if (e.key === "Escape") {
        e.preventDefault();
        closePanel();
        return;
      }
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "f") {
        e.preventDefault();
        focusSearch();
        return;
      }

      const target = e.target as HTMLElement | null;
      const isSearch = !!target && target === searchRef.current;
      const typing =
        !!target &&
        (target.tagName === "INPUT" ||
          target.tagName === "TEXTAREA" ||
          target.tagName === "SELECT" ||
          target.isContentEditable);
      // 面板唤出即聚焦搜索框，所以「搜索框 = 打字区」不能一刀切：
      // ⌘1~9 与 Enter 在搜索框里必须能用（否则最常用的两个动作在默认焦点下永远失效）。
      // 其它输入型控件（对话框输入框、模板编辑框）仍然整套透传。
      if (typing && !isSearch) return;
      // 对话框打开时键盘归它：DialogHost 的 capture 处理器只吃 Esc/Enter，挡不住 ⌘1~9。
      if (getDialog()) return;

      const current = items[selected];

      // ⌘1~9 只在面板内生效（以前注册成全局快捷键，会把所有 App 的 ⌘1~9 抢走，
      // 浏览器切标签、邮件切账号都用不了）。编号与卡片角标一致：仅无筛选时显示角标。
      if ((e.metaKey || e.ctrlKey) && /^[1-9]$/.test(e.key)) {
        if (!numHotkeyEnabled || hasFilter) return;
        const index = Number(e.key) - 1;
        const item = items[index];
        if (!item) return;
        e.preventDefault();
        setSelected(index);
        void paste(item.id).then((outcome) => {
          if (outcome && autoCloseAfterPaste) void closePanel();
        });
        return;
      }

      // 评审 S3：卡片自己已经处理了 Enter（并从卡片内 stopPropagation），
      // 这里再补一层「目标在卡片内就不处理」，防止事件顺序变化时重复回贴。
      if (e.key === "Enter") {
        if (target?.closest("[data-clip-card]")) return;
        e.preventDefault();
        void (async () => {
          if (isSearch) {
            // 搜索防抖（250ms）还没落地就按了回车：先让这次查询真的发出去。
            // 否则贴到的是上一个关键词的结果 —— 用户眼里「打了 abc 却贴出 ab 的第一条」。
            await flushSearch();
            // 查询后列表已更新，序号要从最新状态取，不能读本次渲染的闭包值。
            const latest = useClipStore.getState().items[0];
            if (!latest) return;
            const outcome = await paste(latest.id);
            if (outcome && autoCloseAfterPaste) void closePanel();
            return;
          }
          if (!current) return;
          const outcome = await paste(current.id);
          if (outcome && autoCloseAfterPaste) void closePanel();
        })();
        return;
      }

      // 评审 E5：选中项的快捷操作，与右键菜单一致。
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "c") {
        if (!current) return;
        // 用户手动选中了文字时按 ⌘C，应当复制选中的文字而不是整条记录。
        if (window.getSelection()?.toString()) return;
        e.preventDefault();
        void copyClip(current.id);
        return;
      }
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "d") {
        if (!current) return;
        e.preventDefault();
        void toggleFavorite(current.id, !current.favorite);
        return;
      }
      if (e.key === "Backspace" && (e.metaKey || e.ctrlKey)) {
        // 搜索框里的 ⌘⌫ 是系统「删到行首」，不能被当成删除条目。
        if (typing || !current) return;
        e.preventDefault();
        void (async () => {
          if (confirmOnDelete && !(await confirm("确定删除这条记录？", { danger: true }))) return;
          await remove(current.id);
        })();
        return;
      }

      // 方向键在搜索框里是移动光标，不能拿来切卡片。
      if (typing) return;
      const delta =
        boardStyle === "vertical"
          ? e.key === "ArrowDown"
            ? 1
            : e.key === "ArrowUp"
              ? -1
              : 0
          : e.key === "ArrowRight"
            ? 1
            : e.key === "ArrowLeft"
              ? -1
              : 0;
      if (delta !== 0) {
        e.preventDefault();
        setSelected((prev) => Math.min(Math.max(prev + delta, 0), Math.max(items.length - 1, 0)));
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [
    autoCloseAfterPaste,
    boardStyle,
    closePanel,
    confirmOnDelete,
    copyClip,
    flushSearch,
    hasFilter,
    items,
    numHotkeyEnabled,
    paste,
    remove,
    selected,
    toggleFavorite,
  ]);

  const emptyIcon = (
    <svg
      viewBox="0 0 24 24"
      className="h-[24px] w-[24px]"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.6"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
      <path d="M14 2v6h6" />
    </svg>
  );

  const renderCard = (item: (typeof items)[number], i: number, cardClass: string) => (
    <ClipItemCard
      key={item.id}
      clip={item}
      hotkey={hasFilter || !numHotkeyEnabled ? undefined : i + 1}
      selected={i === selected}
      keyword={keyword}
      className={cardClass}
      onPaste={() => {
        setSelected(i);
        void paste(item.id).then((outcome) => {
          if (outcome && autoCloseAfterPaste) void closePanel();
        });
      }}
      onCopy={() => void copyClip(item.id)}
      onDelete={() => void handlers.onDelete(item)}
      onToggleFavorite={(v) => void handlers.onToggleFavorite(item, v)}
      onContextMenu={(e) => openMenu(e, item)}
    />
  );

  /** 加载 / 空 / 错误 / 列表四态外壳（两种排列方式共用）。 */
  const renderBody = (list: () => React.ReactNode) => {
    if (loading && items.length === 0) {
      return (
        <div className="grid h-full place-items-center text-white/40">
          <Spinner size={18} />
        </div>
      );
    }
    // 加载失败必须和「确实没有历史」区分开：两者都是空列表，但前者要
    // 告诉用户原因并给重试入口，否则看起来就是「面板坏了/内容没了」。
    if (items.length === 0 && error) {
      return (
        <div className="grid h-full place-items-center px-[24px]">
          <div className="flex w-full max-w-[560px] flex-col items-center gap-[10px]">
            <EmptyState icon={emptyIcon} title="历史加载失败" />
            <p className="max-h-[48px] w-full overflow-hidden break-words text-center text-[12px]/[16px] text-danger/80">
              {error}
            </p>
            <div className="flex items-center gap-[8px]">
              <Btn size="sm" variant="primary" onClick={() => void refresh()}>
                重试
              </Btn>
              <Btn size="sm" onClick={() => void openMainWindow("history")}>
                打开历史
              </Btn>
              <Btn size="sm" onClick={() => void openMainWindow("general")}>
                打开设置
              </Btn>
            </div>
          </div>
        </div>
      );
    }
    if (items.length === 0) {
      // 评审 U10：空状态不能只写「暂无历史」，要给出可点的下一步。
      const filtered = hasFilter;
      return (
        <div className="grid h-full place-items-center">
          <div className="flex flex-col items-center gap-[10px]">
            <EmptyState
              icon={emptyIcon}
              title={filtered ? "没有匹配的条目" : "暂无历史"}
              hint={filtered ? "试试清空搜索或切换筛选" : "复制一段内容试试"}
            />
            <div className="flex items-center gap-[8px]">
              {filtered ? (
                <Btn
                  size="sm"
                  onClick={() => {
                    // 搜索、类型、收藏、标签必须一次清完：只清一半的话列表仍是空的，
                    // 用户会以为「清除筛选」按钮坏了。
                    clearFilters();
                  }}
                >
                  清除筛选
                </Btn>
              ) : (
                <Btn size="sm" variant="primary" onClick={() => void captureNow()}>
                  捕获当前剪贴板
                </Btn>
              )}
              <Btn size="sm" onClick={() => void openMainWindow("general")}>
                打开设置
              </Btn>
              <Btn size="sm" onClick={() => void openMainWindow("history")}>
                打开历史
              </Btn>
            </div>
          </div>
        </div>
      );
    }
    return list();
  };

  return (
    <div
      key={shownKey}
      className={`app-surface app-surface--panel flex flex-col ${
        closing ? "animate-panel-out" : "animate-panel-in"
      }`}
    >
      {/* 工具栏 */}
      <header
        className="flex h-[44px] shrink-0 items-center gap-[8px] px-[16px]"
        data-tauri-drag-region
      >
        <SearchField
          value={keyword}
          onChange={setKeyword}
          placeholder="搜索历史"
          width="w-[220px]"
          inputRef={searchRef}
        />
        <Segment
          items={FILTERS.map((f) => ({
            value: f.key,
            label: f.label,
            count: countOf(f.key),
          }))}
          value={activeKey}
          onChange={selectFilter}
        />
        <div className="min-w-[8px] flex-1" data-tauri-drag-region />
        <IconBtn
          title="清空历史"
          danger
          onClick={async () => {
            // 与历史页同一套文案：清空不走撤销栈，图片文件也会被删，
            // 这里写「确定清空」会让人以为是可恢复操作。
            if (
              confirmOnDelete &&
              !(await confirm("确定清空全部历史记录？图片文件也会一并删除，此操作无法撤销。", {
                danger: true,
                title: "清空历史",
                okLabel: "清空全部",
              }))
            ) {
              return;
            }
            void clearAll();
          }}
        >
          <Icon>
            <path d="M3 6h18M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2m3 0v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6h14z" />
          </Icon>
        </IconBtn>
        <IconBtn title="设置" onClick={() => void openMainWindow("general")}>
          <Icon>
            <path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z" />
            <circle cx="12" cy="12" r="3" />
          </Icon>
        </IconBtn>
      </header>

      <TagBar className="border-b border-white/[0.06]" />

      {/* 卡片流：横向（默认）或纵向网格，由 board_style 决定。
          两者都走虚拟化（评审 G4）：最多 500 条时只渲染视口内外的少量卡片。 */}
      {renderBody(() =>
        boardStyle === "vertical" ? (
          <VirtualGrid
            items={items}
            scrollRef={listRef}
            scrollToIndex={selected}
            minColumnWidth={168}
            rowHeight={220}
            render={(item, i) => renderCard(item, i, "h-[220px] w-full")}
          />
        ) : (
          <VirtualStrip
            items={items}
            scrollRef={listRef}
            scrollToIndex={selected}
            render={(item, i) => renderCard(item, i, "w-full")}
          />
        ),
      )}

      <StatusBar />

      <ClipContextMenu
        state={menu}
        availableTags={availableTags}
        moduleActions={moduleActions}
        onClose={() => setMenu(null)}
        {...handlers}
      />

      <ToastHost variant="panel" />
    </div>
  );
}
