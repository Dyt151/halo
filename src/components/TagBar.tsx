import { useClipStore } from "../store/useClipStore";
import { cx, Pill } from "./ui";

/**
 * 分组标签行：全部 + 各标签 pill；无标签时整行隐藏（规范 §9.1 标签行）。
 *
 * 评审 G2：标签列表早期是从**已过滤**的结果里聚合的。选中「工作」之后，
 * 其它标签在结果里不存在，标签栏自己收窄成只剩「工作」，用户再也切不回去。
 * 现在改为读后端 `list_tags` 的全集。
 *
 * 行高 34px（评审 M11）：pill 本身 24px，加上下留白仍低于 HIG 的点击舒适区。
 */
export default function TagBar({ className }: { className?: string }) {
  const activeTag = useClipStore((s) => s.activeTag);
  const setActiveTag = useClipStore((s) => s.setActiveTag);
  const tags = useClipStore((s) => s.tags);

  // 无标签且未筛选时整行隐藏；但 activeTag 指向的标签如果已经消失（最后一条
  // 被删/改了标签），仍要把这一行露出来，否则用户看不到也去不掉这个隐形筛选。
  const stale = activeTag !== null && !tags.includes(activeTag);
  if (tags.length === 0 && !stale) return null;

  return (
    <div
      className={cx(
        "scroll-apple flex h-[34px] shrink-0 items-center gap-[6px] overflow-x-auto px-[16px]",
        className,
      )}
    >
      <Pill selected={activeTag === null} onClick={() => setActiveTag(null)}>
        全部
      </Pill>
      {tags.map((t) => (
        <Pill key={t} selected={activeTag === t} onClick={() => setActiveTag(activeTag === t ? null : t)}>
          {t}
        </Pill>
      ))}
      {stale && (
        <Pill key={activeTag} selected onClick={() => setActiveTag(null)} title="这个标签已没有任何条目">
          {activeTag}
        </Pill>
      )}
    </div>
  );
}
