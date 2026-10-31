/**
 * 管理窗口的「历史」页：工具栏 + 标签行 + 虚拟化自适应网格 + 状态栏。
 *
 * 与面板共用 ClipItem 卡片与右键菜单（useClipActions），保证两处交互一致。
 * 评审 G1：此前只有搜索框，面板里选中的标签会在历史页「隐形生效」且无法取消；
 * 现在筛选器、标签行、状态栏与面板完全对齐，activeTag 也不再跨窗口持久化。
 * 评审 G4：500 条全量渲染改为行虚拟化；评审 E6：支持多选批量删除。
 */
import { useEffect, useRef, useState } from "react";
import { useClipStore } from "../store/useClipStore";
import { useClipActions } from "../hooks/useClipActions";
import { confirm } from "../lib/dialog";
import { FILTERS, type FilterKey } from "../lib/filters";
import ClipItemCard from "../components/ClipItem";
import ClipContextMenu from "../components/ClipContextMenu";
import StatusBar from "../components/StatusBar";
import TagBar from "../components/TagBar";
import { VirtualGrid } from "../components/VirtualList";
import { Btn, EmptyState, SearchField, Segment, Spinner } from "../components/ui";

export default function HistoryPage() {
  const items = useClipStore((s) => s.items);
  const loading = useClipStore((s) => s.loading);
  // 加载失败要和「确实没有历史」区分开（与面板同一套规则）。
  const error = useClipStore((s) => s.error);
  const refresh = useClipStore((s) => s.refresh);
  const stats = useClipStore((s) => s.stats);
  const keyword = useClipStore((s) => s.keyword);
  const setKeyword = useClipStore((s) => s.setKeyword);
  const contentType = useClipStore((s) => s.contentType);
  const setFilter = useClipStore((s) => s.setFilter);
  const favoriteOnly = useClipStore((s) => s.favoriteOnly);
  const activeTag = useClipStore((s) => s.activeTag);
  const clearFilters = useClipStore((s) => s.clearFilters);
  const removeMany = useClipStore((s) => s.removeMany);
  const clearAll = useClipStore((s) => s.clearAll);
  const confirmOnDelete = useClipStore((s) => s.config?.confirm_on_delete) ?? true;

  const { menu, setMenu, moduleActions, handlers, openMenu, availableTags } = useClipActions();

  // 批量选择（评审 E6）：ids 用 Set 保存，勾选是 O(1) 且不受列表刷新影响。
  const [selectMode, setSelectMode] = useState(false);
  const [selectedIds, setSelectedIds] = useState<Set<number>>(new Set());
  const listRef = useRef<HTMLDivElement>(null);

  // 后端可能出现未纳入筛选器的内容类型（如 unknown），此时回落到「全部」。
  const rawKey = favoriteOnly ? "favorite" : (contentType ?? "all");
  const activeKey: FilterKey = FILTERS.some((f) => f.key === rawKey)
    ? (rawKey as FilterKey)
    : "all";
  const hasFilter = Boolean(keyword.trim() || contentType || favoriteOnly || activeTag);

  const selectFilter = (key: FilterKey) => {
    const f = FILTERS.find((x) => x.key === key);
    if (!f) return;
    // 一个分段同时含「类型 + 收藏」两维：一次写入，只发一次查询。
    setFilter(f.contentType, f.favorite);
  };

  const countOf = (key: FilterKey): number => {
    if (key === "all") return stats.total;
    if (key === "favorite") return stats.favorite;
    return stats.by_type[key] ?? 0;
  };

  const toggleSelect = (id: number) => {
    setSelectedIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const exitSelect = () => {
    setSelectMode(false);
    setSelectedIds(new Set());
  };

  // 列表刷新后剔除已经删掉的 id，避免「已选 3 项」里混进幽灵条目。
  useEffect(() => {
    setSelectedIds((prev) => {
      if (prev.size === 0) return prev;
      const alive = new Set(items.map((i) => i.id));
      const next = new Set([...prev].filter((id) => alive.has(id)));
      return next.size === prev.size ? prev : next;
    });
  }, [items]);

  // Esc 退出多选（与 Finder 一致：先退出选择态，再处理其它 Esc 语义）。
  useEffect(() => {
    if (!selectMode) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") exitSelect();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [selectMode]);

  const deleteSelected = async () => {
    const ids = [...selectedIds];
    if (ids.length === 0) return;
    if (
      confirmOnDelete &&
      // 批量删除不写「⌘Z 可撤销」：撤销栈是逐条推进的，按一次只能回来一条。
      // 整批恢复要走删除提示条里的「撤销」（内部循环调 undo）。
      !(await confirm(`确定删除选中的 ${ids.length} 条记录？删除后可在提示条里一键撤销。`, {
        danger: true,
        title: "删除记录",
        okLabel: `删除 ${ids.length} 条`,
      }))
    ) {
      return;
    }
    await removeMany(ids);
    exitSelect();
  };

  const renderCard = (item: (typeof items)[number]) => {
    const card = (
      <ClipItemCard
        key={item.id}
        clip={item}
        keyword={keyword}
        className="h-[220px] w-full"
        onPaste={() => void handlers.onPaste(item)}
        onCopy={() => void handlers.onCopy(item)}
        onDelete={() => void handlers.onDelete(item)}
        onToggleFavorite={(v) => void handlers.onToggleFavorite(item, v)}
        onContextMenu={(e) => openMenu(e, item)}
      />
    );
    if (!selectMode) return card;

    // 多选态：在捕获阶段拦掉卡片自身的「单击复制 / 双击回贴」，
    // 否则用户点一下卡片就把内容复制进剪贴板，体验突兀。
    // 注意 `dblclick` 是独立事件，光拦 click 挡不住它（双击会把内容回贴出去）。
    const checked = selectedIds.has(item.id);
    return (
      <div
        key={item.id}
        className="relative h-[220px] w-full cursor-pointer"
        onClickCapture={(e) => {
          e.preventDefault();
          e.stopPropagation();
          toggleSelect(item.id);
        }}
        onDoubleClickCapture={(e) => {
          e.preventDefault();
          e.stopPropagation();
        }}
        role="checkbox"
        aria-checked={checked}
        tabIndex={0}
        onKeyDown={(e) => {
          if (e.key === " " || e.key === "Enter") {
            e.preventDefault();
            e.stopPropagation();
            toggleSelect(item.id);
          }
        }}
        aria-label={`选择：${item.preview.slice(0, 30) || "剪贴板条目"}`}
      >
        <ClipItemCard
          clip={item}
          className="h-[220px] w-full"
          // 卡片自身可 Tab 聚焦：给个与外壳一致的语义，否则 Enter 落在空回调上
          // （stopPropagation 后外壳的快捷键也收不到，就成了死键）。
          onPaste={() => toggleSelect(item.id)}
          onCopy={() => {}}
          onDelete={() => {}}
          onToggleFavorite={() => {}}
          onContextMenu={(e) => e.preventDefault()}
        />
        <span
          className={`pointer-events-none absolute right-[7px] top-[7px] grid h-[17px] w-[17px] place-items-center rounded-full border text-[10px] ${
            checked
              ? "border-accent-500 bg-accent-500 text-[#1C1C1E]"
              : "border-white/40 bg-black/35 text-transparent"
          }`}
          aria-hidden
        >
          <svg viewBox="0 0 24 24" className="h-[10px] w-[10px]" fill="none" stroke="currentColor" strokeWidth="3.5" strokeLinecap="round" strokeLinejoin="round">
            <path d="m5 13 4 4L19 7" />
          </svg>
        </span>
        <span
          className={`pointer-events-none absolute inset-0 rounded-[10px] transition-[box-shadow] duration-120 ${
            checked ? "shadow-[inset_0_0_0_2px_rgba(232,163,61,0.85)]" : ""
          }`}
          aria-hidden
        />
      </div>
    );
  };

  return (
    <div className="flex h-full min-h-0 flex-col">
      {/* 页头：标题 + 计数 + 批量操作入口 */}
      <header className="flex shrink-0 items-center gap-[10px] px-[24px] pb-[10px] pt-[20px]">
        <h2 className="text-[17px]/[22px] font-semibold text-white">历史</h2>
        <span className="text-[12px]/[16px] tabular-nums text-white/40">
          {hasFilter ? `${items.length} / ${stats.total} 条` : `${stats.total} 条`}
        </span>

        <div className="ml-auto flex flex-wrap items-center justify-end gap-[8px]">
          {selectMode ? (
            <>
              <span className="text-[12px]/[16px] tabular-nums text-white/55">
                已选 {selectedIds.size} 项
              </span>
              <Btn
                size="sm"
                onClick={() =>
                  setSelectedIds(
                    selectedIds.size === items.length
                      ? new Set()
                      : new Set(items.map((i) => i.id)),
                  )
                }
                disabled={items.length === 0}
              >
                {selectedIds.size === items.length && items.length > 0 ? "取消全选" : "全选"}
              </Btn>
              <Btn
                size="sm"
                variant="danger"
                disabled={selectedIds.size === 0}
                onClick={() => void deleteSelected()}
              >
                删除
              </Btn>
              <Btn size="sm" variant="ghost" onClick={exitSelect}>
                完成
              </Btn>
            </>
          ) : (
            <>
              <Btn size="sm" onClick={() => setSelectMode(true)} disabled={items.length === 0}>
                选择
              </Btn>
              <Btn
                size="sm"
                variant="ghost"
                disabled={items.length === 0}
                onClick={async () => {
                  if (
                    confirmOnDelete &&
                    // 清空不走撤销栈（整库快照常驻内存不值当，也不该删图片后再试图恢复）。
                    // 文案必须说实话：早期写的是「可用 ⌘Z 逐条撤销」，实际按了没任何反应。
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
                清空
              </Btn>
            </>
          )}
        </div>
      </header>

      {/* 工具栏：搜索 + 类型分段（与面板一致） */}
      <div className="flex shrink-0 flex-wrap items-center gap-[8px] px-[24px] pb-[10px]">
        <SearchField
          value={keyword}
          onChange={setKeyword}
          placeholder="搜索历史"
          width="w-[240px] min-w-[140px] max-w-full flex-1"
        />
        {/* 窄窗（设置窗最小 420px、侧边栏 200px）下分段放不下时整组横向滚动，
            而不是被 main 的 overflow-hidden 裁掉、类型筛选整组消失。 */}
        <div className="min-w-0 max-w-full overflow-x-auto">
          <Segment
            items={FILTERS.map((f) => ({ value: f.key, label: f.label, count: countOf(f.key) }))}
            value={activeKey}
            onChange={selectFilter}
          />
        </div>
      </div>

      {/* 标签行：全集来自后端，不会因筛选而自我收窄（G2）。 */}
      <TagBar className="border-y border-white/[0.06] px-[24px]" />

      {loading && items.length === 0 ? (
        <div className="grid min-h-0 flex-1 place-items-center text-white/40">
          <Spinner size={18} />
        </div>
      ) : items.length === 0 && error ? (
        <div className="grid min-h-0 flex-1 place-items-center px-[24px]">
          <div className="flex w-full max-w-[560px] flex-col items-center gap-[10px]">
            <EmptyState title="历史加载失败" />
            <p className="max-h-[48px] w-full overflow-hidden break-words text-center text-[12px]/[16px] text-danger/80">
              {error}
            </p>
            <Btn size="sm" variant="primary" onClick={() => void refresh()}>
              重试
            </Btn>
          </div>
        </div>
      ) : items.length === 0 ? (
        <div className="grid min-h-0 flex-1 place-items-center px-[24px]">
          <div className="flex flex-col items-center gap-[10px]">
            <EmptyState
              icon={
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
              }
              title={hasFilter ? "没有匹配的条目" : "暂无历史"}
              hint={hasFilter ? "试试清空搜索或切换筛选" : "复制一段内容，或按快捷键唤出面板"}
            />
            {hasFilter && (
              <Btn
                size="sm"
                onClick={() => {
                  // 搜索、类型、收藏、标签一次清完，只发一次查询。
                  clearFilters();
                }}
              >
                清除筛选
              </Btn>
            )}
          </div>
        </div>
      ) : (
        <div className="min-h-0 flex-1">
          <VirtualGrid
            items={items}
            scrollRef={listRef}
            minColumnWidth={200}
            rowHeight={220}
            padding={24}
            render={(item) => renderCard(item)}
          />
        </div>
      )}

      <StatusBar />

      <ClipContextMenu
        state={menu}
        availableTags={availableTags}
        moduleActions={moduleActions}
        onClose={() => setMenu(null)}
        {...handlers}
      />
    </div>
  );
}
