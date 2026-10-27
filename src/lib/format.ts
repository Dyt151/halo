// 展示用的格式化工具：相对时间、字节大小、类型标签。

import type { ContentType } from "./types";

/**
 * 时间展示（全应用唯一规则，同屏不再混用不同句式）：
 * - 今天 / 昨天：带时刻；
 * - 今年更早：`M月d日 HH:mm`；
 * - 跨年：`YYYY年M月d日 HH:mm`；
 * - 非法/缺失时间戳（<= 0，如迁移遗留）：显示「—」，不再出现「20752 天前」。
 */
export function formatRelativeTime(ts: number): string {
  if (!Number.isFinite(ts) || ts <= 0) return "—";

  const date = new Date(ts);
  if (Number.isNaN(date.getTime())) return "—";

  const now = new Date();
  const pad = (n: number) => String(n).padStart(2, "0");
  const hm = `${pad(date.getHours())}:${pad(date.getMinutes())}`;

  const startOfDay = (d: Date) =>
    new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  const dayDiff = Math.round((startOfDay(now) - startOfDay(date)) / 86_400_000);

  if (dayDiff <= 0) return `今天 ${hm}`;
  if (dayDiff === 1) return `昨天 ${hm}`;

  const monthDay = `${date.getMonth() + 1}月${date.getDate()}日`;
  if (date.getFullYear() === now.getFullYear()) return `${monthDay} ${hm}`;
  return `${date.getFullYear()}年${monthDay} ${hm}`;
}

/** 字节大小格式化：B/KB/MB/GB。 */
export function formatSize(bytes: number): string {
  if (bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  const value = bytes / Math.pow(1024, i);
  const text = value >= 100 || i === 0 ? value.toFixed(0) : value.toFixed(2);
  return `${text} ${units[i]}`;
}

/** 从文件路径提取文件名（兼容 / 与 \\ 分隔符）。 */
export function fileBasename(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

/** 类型徽章文案。 */
export function typeLabel(type: ContentType): string {
  switch (type) {
    case "text":
      return "文本";
    case "html":
      return "富文本";
    case "image":
      return "图片";
    case "files":
      return "文件";
    default:
      return "其他";
  }
}
