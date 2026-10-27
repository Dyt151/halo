/**
 * 内容类型筛选定义（面板与历史页共用）。
 *
 * 抽出来是因为历史页此前完全没有筛选器（评审 G1）：面板能按「文本/图片/文件/
 * 网页/收藏」过滤，历史页只有一个搜索框，两个窗口的筛选能力不对等。
 */
import type { ContentType } from "./types";

export type FilterKey = "all" | "text" | "image" | "files" | "html" | "favorite";

export interface FilterDef {
  key: FilterKey;
  label: string;
  contentType: ContentType | null;
  favorite: boolean;
}

export const FILTERS: FilterDef[] = [
  { key: "all", label: "全部", contentType: null, favorite: false },
  { key: "text", label: "文本", contentType: "text", favorite: false },
  { key: "image", label: "图片", contentType: "image", favorite: false },
  { key: "files", label: "文件", contentType: "files", favorite: false },
  { key: "html", label: "网页", contentType: "html", favorite: false },
  { key: "favorite", label: "收藏", contentType: null, favorite: true },
];
