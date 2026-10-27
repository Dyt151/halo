// Tauri 事件订阅封装（事件名与 Rust 侧 src/events.rs 对齐）。

import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { ClipItem, SyncStatus } from "./types";

export const EVENTS = {
  clipboardChanged: "clipboard://changed",
  historyCreated: "history://created",
  historyDeleted: "history://deleted",
  syncStatusChanged: "sync://status-changed",
  syncCompleted: "sync://completed",
  moduleLoaded: "module://loaded",
  moduleCrashed: "module://crashed",
  moduleChanged: "module://changed",
  moduleEvent: "module://event",
  configChanged: "config://changed",
  /** 转换进度（`convert_run` 执行期间逐文件推送）。 */
  convertProgress: "convert://progress",
} as const;

export function onClipboardChanged(handler: (item: ClipItem) => void): Promise<UnlistenFn> {
  return listen<ClipItem>(EVENTS.clipboardChanged, (event) => handler(event.payload));
}

export function onHistoryDeleted(handler: () => void): Promise<UnlistenFn> {
  return listen(EVENTS.historyDeleted, () => handler());
}

export function onSyncCompleted(handler: (status: SyncStatus) => void): Promise<UnlistenFn> {
  return listen<SyncStatus>(EVENTS.syncCompleted, (event) => handler(event.payload));
}

export function onModuleEvent(
  handler: (payload: { topic: string; data: unknown }) => void,
): Promise<UnlistenFn> {
  return listen<{ topic: string; data: unknown }>(EVENTS.moduleEvent, (event) =>
    handler(event.payload),
  );
}

/**
 * 模块状态变化（安装 / 启用 / 停用 / 崩溃）。
 *
 * 三者是独立事件，前端只需要「重拉一次列表」，所以合并成一个订阅，
 * 避免每个调用点重复写三行 listen。
 */
export function onModuleStateChanged(handler: () => void): Promise<UnlistenFn> {
  const names = [EVENTS.moduleChanged, EVENTS.moduleLoaded, EVENTS.moduleCrashed];
  return Promise.all(names.map((name) => listen(name, () => handler()))).then(
    (fns) => () => fns.forEach((fn) => fn()),
  );
}

export function onConfigChanged(handler: (payload: unknown) => void): Promise<UnlistenFn> {
  return listen(EVENTS.configChanged, (event) => handler(event.payload));
}
