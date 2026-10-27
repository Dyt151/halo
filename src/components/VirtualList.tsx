/**
 * 虚拟化列表（评审 G4）。
 *
 * `@tanstack/react-virtual` 之前装上了但没用：面板与历史页都一次渲染最多 500
 * 张卡片，每张卡片还要跑一次 `formatRelativeTime` + 富文本消毒，滚动手感明显掉帧。
 * 这里提供两个封装：
 * - `VirtualStrip`：横向单行（面板默认排列）；
 * - `VirtualGrid`：按列数切行的网格（历史页 / 面板纵向排列）。
 *
 * 两者的滚动容器都由调用方持有（`scrollRef`），因为面板需要在「唤出时复位滚动」
 * 时直接操作它；选中项发生滚动通过 `scrollToIndex` 提示。
 */
import { useEffect, useState, type ReactNode, type RefObject } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { cx } from "./ui";

interface BaseProps<T> {
  items: T[];
  render: (item: T, index: number) => ReactNode;
  scrollRef: RefObject<HTMLDivElement>;
  /** 需要保证可见的索引（键盘导航选中的卡片），变化时自动滚动。 */
  scrollToIndex?: number;
  className?: string;
}

/** 横向单行虚拟列表。 */
export function VirtualStrip<T>({
  items,
  render,
  scrollRef,
  scrollToIndex,
  className,
  itemWidth = 168,
  gap = 12,
  padding = 16,
}: BaseProps<T> & { itemWidth?: number; gap?: number; padding?: number }) {
  const virtualizer = useVirtualizer({
    count: items.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => itemWidth + gap,
    horizontal: true,
    overscan: 4,
  });

  useEffect(() => {
    if (scrollToIndex === undefined) return;
    virtualizer.scrollToIndex(scrollToIndex, { align: "auto" });
  }, [scrollToIndex, virtualizer]);

  return (
    <div
      ref={scrollRef}
      className={cx("scroll-apple min-h-0 flex-1 overflow-x-auto overflow-y-hidden", className)}
    >
      <div
        className="relative h-full"
        style={{ width: virtualizer.getTotalSize() + padding * 2 }}
      >
        {virtualizer.getVirtualItems().map((v) => (
          <div
            key={v.key}
            className="absolute top-0 flex h-full items-stretch py-[12px]"
            style={{
              left: v.start + padding,
              width: itemWidth,
            }}
          >
            {render(items[v.index], v.index)}
          </div>
        ))}
      </div>
    </div>
  );
}

/** 网格虚拟列表：列数由容器宽度与最小列宽推导，行高固定。 */
export function VirtualGrid<T>({
  items,
  render,
  scrollRef,
  scrollToIndex,
  className,
  minColumnWidth = 200,
  gap = 12,
  rowHeight = 220,
  padding = 16,
}: BaseProps<T> & {
  minColumnWidth?: number;
  gap?: number;
  rowHeight?: number;
  padding?: number;
}) {
  const [width, setWidth] = useState(0);

  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    const observer = new ResizeObserver((entries) => {
      const next = entries[0]?.contentRect.width ?? el.clientWidth;
      setWidth(next);
    });
    observer.observe(el);
    setWidth(el.clientWidth);
    return () => observer.disconnect();
  }, [scrollRef]);

  const inner = Math.max(0, width - padding * 2);
  const columns = Math.max(1, Math.floor((inner + gap) / (minColumnWidth + gap)));
  const rows = Math.ceil(items.length / columns);

  const virtualizer = useVirtualizer({
    count: rows,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => rowHeight + gap,
    overscan: 2,
  });

  useEffect(() => {
    if (scrollToIndex === undefined) return;
    virtualizer.scrollToIndex(Math.floor(scrollToIndex / columns), { align: "auto" });
  }, [scrollToIndex, columns, virtualizer]);

  return (
    <div ref={scrollRef} className={cx("scroll-apple min-h-0 overflow-y-auto", className)}>
      <div className="relative" style={{ height: virtualizer.getTotalSize() + padding * 2 }}>
        {virtualizer.getVirtualItems().map((v) => {
          const start = v.index * columns;
          return (
            <div
              key={v.key}
              className="absolute left-0 right-0"
              style={{
                top: v.start + padding,
                paddingInline: padding,
                display: "grid",
                gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))`,
                gap,
              }}
            >
              {items.slice(start, start + columns).map((item, i) => render(item, start + i))}
            </div>
          );
        })}
      </div>
    </div>
  );
}
