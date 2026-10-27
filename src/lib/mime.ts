/**
 * 模块贡献动作的 mime 过滤（判断剪贴板条目是否满足模块声明的过滤条件）。
 *
 * 从 `hooks/useClipActions.ts` 抽出：面板与历史页右键菜单都要用，抽成
 * 纯函数后既不拖 Tauri 依赖，也方便对每条 mime 匹配规则做单测。
 */

import type { ClipItem } from "./types";

/**
 * 判断 `contentType` 是否命中 `mime` 声明里的任一条。
 *
 * 规则与后端模块规范约定一致：空列表 = 全放行；`*` / `*`/*` = 全放行；
 * `group/*` 只比对大类；具体类型按大类就近匹配（html 也属于 text 族）。
 */
export function matchesMime(contentType: ClipItem["content_type"], mime: string[]): boolean {
  if (mime.length === 0) return true;
  const kind = contentType;
  return mime.some((m) => {
    if (m === "*" || m === "*/*") return true;
    const [group, sub] = m.split("/");
    if (sub === "*") return group === kind;
    if (kind === "text" && group === "text") return true;
    if (kind === "image" && group === "image") return true;
    if (kind === "html" && (m === "text/html" || group === "text")) return true;
    if (kind === "files" && m === "application/pdf") return true;
    return false;
  });
}
