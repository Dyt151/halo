/**
 * 跨窗口导航与小工具。
 *
 * Halo 有两个窗口（面板 / 管理窗口），二者是独立的 webview。
 * 「打开管理窗口并落到某一页」由 Rust 侧统一处理：
 *   - 目标页存在 AppState（进程内唯一可信的共享点，两个 webview 不保证共享 localStorage）；
 *   - 同时广播 `halo://navigate` 事件给已挂载的主窗口。
 * 前端只负责调命令、消费待跳转页、订阅事件。
 */
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { invokeCommand } from "./ipc";

export const PANEL_WINDOW_LABEL = "panel";
export const RING_WINDOW_LABEL = "ring";
export const TRAY_PANEL_WINDOW_LABEL = "traypanel";
export const VOICE_CMD_WINDOW_LABEL = "cmdbar";

/**
 * 主窗口的设置分区（评审任务 1–3：主窗口已精简为纯设置中心）。
 *
 * `history` 不在侧边栏一级展示，作为二级页从面板或「设置 → 剪贴板」进入（任务 5）。
 *
 * 快捷键 / 模块 分区已下线（用户只需要面板 + 设置），
 * 旧值由 `MainWindow::normalizeSection` 回落到「通用」。
 *
 * 权限页已并入「通用」：权限与授权是通用页里的一个区块，
 * 旧的 `permissions` 标识同样由 `normalizeSection` 映射到 `general`。
 */
export type MainPage =
  | "general"
  | "clipboard"
  | "ring"
  | "context-menu"
  | "statusbar"
  | "sync"
  | "backup"
  | "about"
  | "history";

/** 导航事件名：Rust → 主窗口。 */
const NAVIGATE_EVENT = "halo://navigate";

/** 当前窗口是否为剪贴板面板。 */
export function isPanelWindow(): boolean {
  return getCurrentWebviewWindow().label === PANEL_WINDOW_LABEL;
}

/** 当前窗口是否为环形转换盘。 */
export function isRingWindow(): boolean {
  return getCurrentWebviewWindow().label === RING_WINDOW_LABEL;
}

/** 当前窗口是否为菜单栏图标下方的状态栏面板。 */
export function isTrayPanelWindow(): boolean {
  return getCurrentWebviewWindow().label === TRAY_PANEL_WINDOW_LABEL;
}

/** 当前窗口是否为语音指令条。 */
export function isVoiceCmdWindow(): boolean {
  return getCurrentWebviewWindow().label === VOICE_CMD_WINDOW_LABEL;
}

/**
 * 订阅「语音指令条已显示」事件（Rust 在 show + set_focus 之后发出）。
 *
 * 指令条窗口是 hide 而非销毁，React 只挂载一次：清空上一次的输入与结果、
 * 把光标重新放进输入框，都必须由这个事件驱动，否则第二次唤起会带着上一条的话。
 */
export async function onVoiceCmdShown(handler: () => void): Promise<() => void> {
  return listen("halo://voice-cmd-shown", () => handler());
}

/**
 * 订阅「状态栏面板已显示」事件（Rust 在 show 后发出）。
 *
 * 面板窗口是 hide 而非销毁，React 只挂载一次；不监听这个事件的话，
 * 每次弹出看到的是上次的旧音量/旧输入法。
 */
export async function onTrayShown(handler: () => void): Promise<() => void> {
  return listen("halo://tray-shown", () => handler());
}

/** 订阅「状态栏面板已收起」事件（音量轮询等可见期间的后台任务据此停机）。 */
export async function onTrayHidden(handler: () => void): Promise<() => void> {
  return listen("halo://tray-hidden", () => handler());
}

/** 打开管理窗口，可指定落到哪一页。 */
export async function openMainWindow(page?: MainPage): Promise<void> {
  await invokeCommand("open_main_window", { page: page ?? null });
}

/** 取出并清除待跳转页（主窗口挂载时调用一次）。 */
export async function consumePendingPage(): Promise<MainPage | null> {
  try {
    const raw = await invokeCommand<string | null>("consume_pending_page");
    return (raw as MainPage | null) ?? null;
  } catch {
    return null;
  }
}

/** 收起当前（面板）窗口。 */
export async function hideCurrentWindow(): Promise<void> {
  await getCurrentWebviewWindow().hide();
}

/** 订阅「导航到某页」事件，返回取消订阅函数。 */
export async function onNavigate(handler: (page: MainPage) => void): Promise<() => void> {
  return listen<MainPage>(NAVIGATE_EVENT, (e) => handler(e.payload));
}

/**
 * 设置页「待打开的分页」（清单 42：首启引导里的「查看权限」要直接落到权限页）。
 *
 * 主窗口的 Settings 只挂载一次，导航到它时不会再重建；
 * 因此用模块级一次性变量传递，比事件广播更好控制时序。
 */
let pendingSettingsTab: string | null = null;

export function requestSettingsTab(tab: string): void {
  pendingSettingsTab = tab;
}

/** 读但不消费：供组件挂载时的初值计算（StrictMode 下初始化函数会跑两次）。 */
export function peekSettingsTab(): string | null {
  return pendingSettingsTab;
}

/** 消费后清空，避免下次打开又跳到同一页。 */
export function clearSettingsTab(): void {
  pendingSettingsTab = null;
}

export function takeSettingsTab(): string | null {
  const tab = pendingSettingsTab;
  pendingSettingsTab = null;
  return tab;
}

/**
 * 订阅「面板已显示」事件（Rust 在 show 后发出）。
 * 用于重放出现动画、复位滚动与选中态——面板窗口是 hide 而非销毁，
 * React 只挂载一次，不监听这个事件的话动画只在冷启动播一遍。
 */
export async function onPanelShown(handler: () => void): Promise<() => void> {
  return listen("halo://panel-shown", () => handler());
}
