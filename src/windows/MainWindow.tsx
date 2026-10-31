/**
 * 主窗口外壳：纯设置中心（评审任务 1–5、34）。
 *
 * 只保留**一套侧边栏**（不再有「外层功能导航 + 设置内部 tab」双导航），
 * 右侧按分区渲染。默认打开「通用」或上次停留的分区（`last_settings_section`）。
 * 历史不再是一级入口，作为二级页从面板或「剪贴板」分区进入。
 */
import { useEffect, useRef, useState } from "react";
import {
  clearSettingsTab,
  consumePendingPage,
  hideCurrentWindow,
  onNavigate,
  peekSettingsTab,
  takeSettingsTab,
  type MainPage,
} from "../lib/windows";
import { useAppVersion } from "../lib/version";
import { useClipStore } from "../store/useClipStore";
import Settings from "../pages/Settings";
import FirstRunGuide from "../components/FirstRunGuide";
import { cx } from "../components/ui";

/** 侧边栏一级分区（`history` 是二级页，不在此列，评审任务 3、5）。 */
const NAV: { key: MainPage; label: string; group: "功能" | "系统"; path: React.ReactNode }[] = [
  {
    key: "general",
    label: "通用",
    group: "功能",
    path: (
      <>
        <circle cx="12" cy="12" r="3" />
        <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09a1.65 1.65 0 0 0-1-1.51 1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09a1.65 1.65 0 0 0 1.51-1 1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33h.01a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51h.01a1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82v.01a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
      </>
    ),
  },
  {
    key: "clipboard",
    label: "剪贴板",
    group: "功能",
    path: (
      <>
        <rect x="8" y="8" width="12" height="12" rx="2" />
        <path d="M16 8V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h2" />
      </>
    ),
  },
  {
    key: "ring",
    label: "环形转换",
    group: "功能",
    path: (
      <>
        <circle cx="12" cy="12" r="9" />
        <circle cx="12" cy="12" r="3" />
        <path d="M12 3v3M12 18v3M3 12h3M18 12h3" />
      </>
    ),
  },
  {
    key: "context-menu",
    label: "右键菜单",
    group: "功能",
    path: (
      <>
        <path d="M9 5h11M9 12h11M9 19h11" />
        <path d="M4 5h.01M4 12h.01M4 19h.01" />
      </>
    ),
  },
  {
    key: "sync",
    label: "同步",
    group: "系统",
    path: (
      <>
        <path d="M21 12a9 9 0 0 1-9 9 9 9 0 0 1-7.5-4" />
        <path d="M3 12a9 9 0 0 1 9-9 9 9 0 0 1 7.5 4" />
        <path d="M20 3v4h-4M4 21v-4h4" />
      </>
    ),
  },
  {
    key: "backup",
    label: "备份与迁移",
    group: "系统",
    path: (
      <>
        <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
        <path d="M7 10l5 5 5-5" />
        <path d="M12 15V3" />
      </>
    ),
  },
  {
    key: "permissions",
    label: "权限",
    group: "系统",
    path: (
      <>
        <path d="M12 2 4 5v6c0 5 3.5 8 8 11 4.5-3 8-6 8-11V5z" />
        <path d="m9 12 2 2 4-4" />
      </>
    ),
  },
  {
    key: "about",
    label: "关于",
    group: "系统",
    path: (
      <>
        <circle cx="12" cy="12" r="9" />
        <path d="M12 11v5M12 8h.01" />
      </>
    ),
  },
];

/** 分区别名：面板深链与旧配置里可能传来已下线的分区，统一映射到现有页（避免空白页）。 */
function normalizeSection(raw: string | null | undefined): MainPage | null {
  if (!raw) return null;
  switch (raw) {
    case "settings":
      return "general";
    case "convert":
      return "ring";
    // 已下线的分区：快捷键 / 快捷回复 / 模块。旧配置（last_settings_section）
    // 或旧深链仍可能带这几个值，落到「通用」而不是白屏。
    case "hotkeys":
    case "replies":
    case "modules":
      return "general";
    case "history":
    case "general":
    case "clipboard":
    case "ring":
    case "context-menu":
    case "sync":
    case "backup":
    case "permissions":
    case "about":
      return raw;
    default:
      return null;
  }
}

/** 侧边栏导航图标（16px，SF Symbols 风格描边）。 */
function NavIcon({ children, active }: { children: React.ReactNode; active: boolean }) {
  return (
    <svg
      viewBox="0 0 24 24"
      className={cx("h-4 w-4 shrink-0 transition-colors duration-120", active && "text-accent-500")}
      fill="none"
      stroke="currentColor"
      strokeWidth="1.8"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      {children}
    </svg>
  );
}

export default function MainWindow() {
  const version = useAppVersion();
  const lastSection = useClipStore((s) => s.config?.last_settings_section);
  const patchConfig = useClipStore((s) => s.patchConfig);
  // 挂载初值优先级：首启引导请求 > 上次分区 > 通用。
  // 用 peek 而不是 take：StrictMode 下初始化函数会跑两次，take 会把值在第一次就读掉，
  // 第二次退回「通用」，导致「查看权限」跳不过去。清空放进 effect。
  const [section, setSectionState] = useState<MainPage>(() => {
    const guided = normalizeSection(peekSettingsTab());
    return guided ?? normalizeSection(lastSection) ?? "general";
  });
  // 记录已写入的分区，避免挂载即回写、也避免重复 patch。
  const persistedRef = useRef<MainPage | null>(null);

  const setSection = (next: MainPage) => {
    setSectionState(next);
    // 仅一级分区写回上次停留位置；history 是临时二级页，不覆盖记忆（评审任务 1）。
    if (next !== "history") {
      persistedRef.current = next;
      void patchConfig({ last_settings_section: next }).catch(() => {});
    }
  };

  // 首启引导「查看权限」：消费引导写入的目标分区，落到权限页（清单 42 / 任务 34）。
  const openGuidedSection = () => {
    setSection(normalizeSection(takeSettingsTab()) ?? "permissions");
  };

  // 挂载时的引导分区已被采纳，这里清掉，避免下次打开主窗口又跳一次。
  useEffect(() => {
    clearSettingsTab();
  }, []);

  // 面板/托盘可请求主窗口落到指定分区。
  useEffect(() => {
    let disposed = false;
    void consumePendingPage().then((pending) => {
      const target = normalizeSection(pending);
      if (!disposed && target) setSectionState(target);
    });

    let unsub: (() => void) | undefined;
    void onNavigate((p) => {
      const target = normalizeSection(p);
      if (!disposed && target) setSectionState(target);
    }).then((fn) => {
      if (disposed) fn();
      else unsub = fn;
    });
    return () => {
      disposed = true;
      unsub?.();
    };
  }, []);

  // config 首次加载完成时，若用户尚未手动切换且无深链，落到上次分区。
  useEffect(() => {
    if (persistedRef.current) return;
    const target = normalizeSection(lastSection);
    if (target) setSectionState((prev) => (prev === "general" ? target : prev));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [lastSection]);

  const activeItem = NAV.find((n) => n.key === section);
  const title = section === "history" ? "高级历史管理" : activeItem?.label ?? "设置";

  const groups: ("功能" | "系统")[] = ["功能", "系统"];

  return (
    <div className="app-surface app-surface--window relative flex">
      {/* 单套侧边栏 */}
      <aside className="flex w-[200px] shrink-0 flex-col border-r border-white/[0.08] bg-black/[0.15]">
        <header data-tauri-drag-region className="shrink-0 px-[16px] pb-[10px] pt-[36px]">
          <div className="flex items-center gap-[10px]">
            <div className="grid h-10 w-10 shrink-0 place-items-center rounded-[10px] bg-accent-500">
              <svg
                viewBox="0 0 24 24"
                className="h-5 w-5 text-accent-on"
                fill="none"
                stroke="currentColor"
                strokeWidth="2"
                strokeLinecap="round"
                strokeLinejoin="round"
              >
                <rect x="8" y="8" width="12" height="12" rx="2" />
                <path d="M16 8V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h2" />
              </svg>
            </div>
            <div className="min-w-0 flex-1" data-tauri-drag-region>
              <div className="truncate text-[13px]/[18px] font-semibold text-white">ClipBase 设置</div>
              <div className="text-[11px]/[14px] text-white/45">{version ? `v${version}` : ""}</div>
            </div>
          </div>
        </header>

        <nav className="scroll-apple flex min-h-0 flex-1 flex-col gap-[2px] overflow-y-auto py-[8px]">
          {groups.map((group) => (
            <div key={group} className="flex flex-col gap-[2px]">
              <div className="px-[18px] pb-[4px] pt-[8px] text-[11px]/[14px] font-medium uppercase tracking-wide text-white/35">
                {group}
              </div>
              {NAV.filter((item) => item.group === group).map((item) => {
                const active = section === item.key;
                return (
                  <button
                    key={item.key}
                    type="button"
                    onClick={() => setSection(item.key)}
                    aria-current={active ? "page" : undefined}
                    className={cx(
                      "mx-[8px] flex h-[34px] items-center gap-[8px] rounded-md px-[10px]",
                      "text-[13px]/[18px] outline-none transition-colors duration-120 ease-out-soft",
                      active
                        ? "bg-white/[0.10] font-medium text-white"
                        : "text-white/85 hover:bg-white/[0.06]",
                    )}
                  >
                    <NavIcon active={active}>{item.path}</NavIcon>
                    <span className="truncate">{item.label}</span>
                  </button>
                );
              })}
            </div>
          ))}
        </nav>

        <div className="mt-auto px-[8px] pb-[10px]">
          <button
            type="button"
            onClick={() => void hideCurrentWindow()}
            title="隐藏窗口（⌘W）"
            className="flex h-[34px] w-full items-center gap-[8px] rounded-md px-[10px] text-[12px]/[16px] text-white/45 outline-none transition-colors duration-120 hover:bg-white/[0.06] hover:text-white/85"
          >
            <svg viewBox="0 0 24 24" className="h-4 w-4" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round">
              <path d="M5 12h14" />
            </svg>
            隐藏窗口
          </button>
        </div>
      </aside>

      {/* 内容区：history 自己管内层滚动，其余分区用外层滚动容器。 */}
      <main className="flex min-w-0 flex-1 flex-col overflow-hidden">
        {section === "history" ? (
          <div key="history" className="flex h-full min-h-0 animate-fade-in flex-col">
            <Settings section="history" />
          </div>
        ) : (
          <div key={section} className="scroll-apple min-w-0 flex-1 overflow-y-auto p-[24px]">
            <div className="mx-auto max-w-[760px]">
              <h2 className="mb-[16px] text-[17px]/[22px] font-semibold text-white">{title}</h2>
              <Settings section={section} />
            </div>
          </div>
        )}
      </main>

      {/* 首启引导（清单 42）：只在主窗口首次打开时出现，不挡浮动面板。 */}
      <FirstRunGuide onOpenSettings={openGuidedSection} />
    </div>
  );
}
