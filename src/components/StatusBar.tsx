import { useClipStore } from "../store/useClipStore";
import { errorMessage } from "../lib/errors";
import { formatSize } from "../lib/format";
import { CloseButton, Kbd } from "./ui";
import { cx } from "./ui";

/**
 * 面板状态栏（h-[36px]）：条数 / 体积 / 监控徽标 / 同步状态 / 快捷键 / 错误。
 *
 * 评审 U7：错误此前是行内 `⚠ {error}` —— 既不换行也不可关闭，一条长错误会把
 * 整条状态栏挤爆且永久停在那里。现在改为截断 + tooltip + 可关闭。
 * 评审 E9：监控被关闭时给出「监控已暂停」徽标，否则用户会以为工具坏了。
 */
export default function StatusBar() {
  const stats = useClipStore((s) => s.stats);
  const items = useClipStore((s) => s.items);
  const keyword = useClipStore((s) => s.keyword);
  const contentType = useClipStore((s) => s.contentType);
  const favoriteOnly = useClipStore((s) => s.favoriteOnly);
  const activeTag = useClipStore((s) => s.activeTag);
  const syncStatus = useClipStore((s) => s.syncStatus);
  const config = useClipStore((s) => s.config);
  const error = useClipStore((s) => s.error);
  const setError = useClipStore((s) => s.setError);
  // 兜底再折一次：后端错误是对象（`{code,message,details}`），一旦漏到
  // 渲染层就会变成「[object Object]」—— 用户看到的是一条毫无信息量的报错。
  const errorText = error ? errorMessage(error) : null;

  const monitorPaused = config ? !config.monitor_enabled : false;

  const syncText = syncStatus
    ? syncStatus.state === "syncing"
      ? "同步中…"
      : syncStatus.state === "success"
        ? `已同步 · ${syncStatus.item_count} 条`
        : syncStatus.state === "error"
          ? "同步失败"
          : syncStatus.enabled
            ? "同步待命"
            : "同步关闭"
    : "—";

  // 过滤激活时展示「当前 / 总数」，避免把过滤结果误读为总量。
  const filtered = Boolean(keyword.trim() || contentType || favoriteOnly || activeTag);
  const countText = filtered ? `${items.length} / ${stats.total} 条` : `${stats.total} 条`;

  return (
    <footer className="flex h-[36px] shrink-0 items-center gap-[10px] border-t border-white/[0.06] px-[16px] text-[12px]/[16px] text-white/45">
      <span className="tabular-nums">{countText}</span>
      <span className="tabular-nums">{formatSize(stats.total_bytes)}</span>

      {monitorPaused && (
        <span
          className="inline-flex h-[18px] shrink-0 items-center gap-[4px] rounded-full bg-warning/[0.14] px-[7px] text-[11px]/[14px] text-warning"
          title="剪贴板监控已关闭，不会记录新的复制内容"
        >
          <span className="h-[5px] w-[5px] rounded-full bg-warning" aria-hidden />
          监控已暂停
        </span>
      )}

      {/* 错误：单行截断 + 悬浮看全文 + 可关闭（不再撑爆状态栏）。 */}
      {errorText && (
        <span
          className={cx(
            "flex min-w-0 items-center gap-[6px] rounded-[5px] bg-danger-fill/10 px-[6px]",
            "text-[11px]/[16px] text-danger",
          )}
          title={errorText}
          role="alert"
        >
          <span className="truncate">⚠ {errorText}</span>
          <CloseButton title="关闭错误提示" onClick={() => setError(null)} />
        </span>
      )}

      <span className="ml-auto shrink-0">{syncText}</span>
      {/* 同步状态与快捷键之间加分隔：避免让人以为快捷键是同步开关。 */}
      <span className="h-[12px] w-px shrink-0 bg-white/[0.12]" aria-hidden />
      <span className="flex shrink-0 items-center gap-[6px] whitespace-nowrap" title="唤起面板的全局快捷键">
        <span className="text-white/40">面板</span>
        <Kbd>{config?.hotkeys.toggle ?? "Control+Option+C"}</Kbd>
      </span>
    </footer>
  );
}
