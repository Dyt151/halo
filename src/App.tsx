import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { useClipStore } from "./store/useClipStore";
import {
  onClipboardChanged,
  onConfigChanged,
  onHistoryDeleted,
  onModuleEvent,
  onModuleStateChanged,
  onSyncCompleted,
} from "./lib/events";
import { isPanelWindow, isRingWindow } from "./lib/windows";
import { getDialog } from "./lib/dialog";
import { isTextEditing, isTextEditingAt } from "./lib/focus";
import Panel from "./windows/Panel";
import MainWindow from "./windows/MainWindow";
import RingWindow from "./windows/RingWindow";
import DialogHost from "./components/DialogHost";
import ToastHost from "./components/ToastHost";

/**
 * 应用入口：按窗口标签分发。
 *
 * - `panel`：快捷键唤出的剪贴板面板（只呈现卡片流）。
 * - `ring`：Finder 环形转换盘（现场浮动面板）。
 * - `main`：主窗口 = 纯设置中心。
 *
 * 两个窗口是独立 webview，各自挂载一份 React，但共享同一份后端状态；
 * 因此初始化（拉配置/拉模块/订阅事件）在两边都要跑。
 */
export default function App() {
  const refresh = useClipStore((s) => s.refresh);
  const loadConfig = useClipStore((s) => s.loadConfig);
  const loadModules = useClipStore((s) => s.loadModules);
  const loadSyncStatus = useClipStore((s) => s.loadSyncStatus);
  const applyClipboardChanged = useClipStore((s) => s.applyClipboardChanged);
  const undo = useClipStore((s) => s.undo);
  const redo = useClipStore((s) => s.redo);
  const theme = useClipStore((s) => s.config?.theme);
  const [winKind] = useState(() => ({
    panel: isPanelWindow(),
    ring: isRingWindow(),
  }));
  const panel = winKind.panel;

  // 外观（清单 25）：system 跟随系统，dark/light 强制；浅色时给 <html> 挂 .light，
  // 由 index.css 的 :root.light 变量组整体换肤。环形盘是硬编码深色 HUD，
  // 挂 .light 会让半透明底变亮、看不清内容，这里直接跳过（评审 V02）。
  useEffect(() => {
    const root = document.documentElement;
    const media = window.matchMedia("(prefers-color-scheme: light)");
    const apply = () => {
      const light =
        !winKind.ring && (theme === "light" || (theme !== "dark" && media.matches));
      root.classList.toggle("light", light);
    };
    apply();
    media.addEventListener("change", apply);
    return () => media.removeEventListener("change", apply);
  }, [theme, winKind.ring]);

  useEffect(() => {
    void refresh();
    void loadConfig();
    void loadModules();
    void loadSyncStatus();

    const unlisteners: Promise<() => void>[] = [
      // 新条目局部插入（无筛选时不再全量重拉，复制后面板秒更）。
      onClipboardChanged((item) => applyClipboardChanged(item)),
      onHistoryDeleted(() => void refresh()),
      onModuleEvent((payload) => {
        if (payload.topic.startsWith("module://") || payload.topic.startsWith("sync://")) {
          void refresh();
        }
      }),
      // 安装/启用/停用/崩溃都要让「模块」页重拉一次（含跨窗口操作）。
      onModuleStateChanged(() => void loadModules()),
      onConfigChanged(() => {
        void loadConfig();
        void refresh();
      }),
      // 同步状态跨窗口刷新（评审 G9）：后台定时同步完成时两个窗口都要更新状态栏。
      // 同步会把远端记录写进本地库（`sync/manager.rs` 的 pull 分支直接 insert），
      // 所以列表也得重拉一次，否则新拉下来的条目要等到下次筛选才出现。
      onSyncCompleted(() => {
        void loadSyncStatus();
        void refresh();
      }),
    ];

    return () => {
      void Promise.all(unlisteners).then((fns) => fns.forEach((fn) => fn()));
    };
  }, [refresh, loadConfig, loadModules, loadSyncStatus, applyClipboardChanged]);

  // 编辑菜单的撤销/重做（评审 M8）。
  //
  // 自绘 webview 里的原生 Edit 菜单对「删除剪贴板条目」这种应用级操作无效，
  // 因此 Rust 侧菜单只负责转发事件，具体语义交给当前窗口的前端。
  // 同时监听键盘：WKWebView 里 ⌘Z 不一定能命中菜单项（焦点在自绘元素上时）。
  useEffect(() => {
    // 环形盘是现场窗口，根本没有「历史列表」这个概念：
    // 在这里按 ⌘Z 不该去撤销一条用户看不见的删除。
    if (winKind.ring) return;

    const onKey = (e: KeyboardEvent) => {
      if (!(e.metaKey || e.ctrlKey) || e.key.toLowerCase() !== "z") return;
      // 对话框在前台时一律让位：确认框里按 ⌘Z 不该悄悄删掉背后的条目。
      if (getDialog()) return;
      // 输入框内的撤销交给系统（否则会把用户正在编辑的文本撤掉）。
      if (isTextEditingAt(e.target as Element | null)) return;
      e.preventDefault();
      // ⌘Z 撤销、⌘⇧Z 重做（与 macOS 全局约定一致）。
      if (e.shiftKey) void redo();
      else void undo();
    };

    // 菜单栏的 ⌘Z 会先被原生 NSMenu 截走（webview 根本收不到 keydown），
    // 所以「在输入框里」这件事只能在这里补救：否则用户在设置页输入框里
    // 按 ⌘Z，撤销的会是一条他根本看不见的「删除历史」操作。
    const runMenuCommand = (kind: "undo" | "redo") => {
      if (isTextEditing()) {
        // 按键已被菜单吃掉，浏览器不会再自己处理，这里补上文本撤销/重做。
        // execCommand 虽已标记废弃，但 WKWebView 里仍是对输入框做撤销的唯一手段。
        document.execCommand(kind);
        return;
      }
      // 对话框在前台时让位：确认框里按 ⌘Z 不该悄悄改动背后的记录。
      if (getDialog()) return;
      void (kind === "undo" ? undo() : redo());
    };
    const unlisteners = [
      listen("clipbase://menu-undo", () => runMenuCommand("undo")),
      listen("clipbase://menu-redo", () => runMenuCommand("redo")),
    ];
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("keydown", onKey);
      void Promise.all(unlisteners).then((fns) => fns.forEach((fn) => fn()));
    };
  }, [undo, redo, winKind.ring]);

  return (
    <>
      {winKind.ring ? (
        <RingWindow />
      ) : panel ? (
        <Panel />
      ) : (
        <MainWindow />
      )}
      <DialogHost />
      {/* 面板自带贴边的 Toast 容器（ToastHost variant="panel"），这里只服务管理窗口。 */}
      {!panel && !winKind.ring && <ToastHost />}
    </>
  );
}
