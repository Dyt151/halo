/**
 * 首次启动引导（清单 42 / 评审 F1）。
 *
 * 只做一件事：在新用户第一次打开管理窗口时，用一屏把「怎么唤出面板」
 * 「数据存在哪」和「哪几项权限会影响功能」讲清楚，并把权限入口摆在按钮上，
 * 而不是等用户回贴失败后才去猜。
 *
 * 显示状态记在 localStorage：这是纯前端首启标记，不参与 `config.json`，
 * 也不参与同步（换机器重看一次引导反而是好事）。
 */
import { useEffect, useState } from "react";
import { useClipStore } from "../store/useClipStore";
import { requestSettingsTab } from "../lib/windows";
import { Btn, Kbd } from "./ui";

const STORAGE_KEY = "clipbase.onboarded.v1";

function seen(): boolean {
  try {
    return localStorage.getItem(STORAGE_KEY) === "1";
  } catch {
    // 存储不可用时不再反复弹：宁可少一次引导，也不要每次启动都挡住用户。
    return true;
  }
}

export default function FirstRunGuide({ onOpenSettings }: { onOpenSettings: () => void }) {
  const toggleHotkey = useClipStore((s) => s.config?.hotkeys.toggle) ?? "Control+Option+C";
  const [visible, setVisible] = useState(() => !seen());

  const dismiss = () => {
    try {
      localStorage.setItem(STORAGE_KEY, "1");
    } catch {
      // 忽略：下次启动会再引导一次，不影响功能。
    }
    setVisible(false);
  };

  // Esc 关闭：与其它弹层一致。
  useEffect(() => {
    if (!visible) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        dismiss();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [visible]);

  if (!visible) return null;

  const openPermissions = () => {
    requestSettingsTab("permissions");
    dismiss();
    onOpenSettings();
  };

  return (
    <div
      className="absolute inset-0 z-40 grid place-items-center bg-black/40 px-[24px]"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) e.preventDefault();
      }}
    >
      <div
        className="menu w-[440px] animate-sheet-in rounded-[12px] p-[20px] shadow-modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="first-run-title"
      >
        <h2 id="first-run-title" className="text-[15px]/[20px] font-semibold text-white">
          欢迎使用 ClipBase
        </h2>
        <p className="mt-[6px] text-[12px]/[18px] text-white/55">
          ClipBase 在后台常驻（没有 Dock 图标也不占菜单栏），快捷键唤出一个横向面板，选中即回贴到刚才的 App。
        </p>

        <div className="mt-[14px] space-y-[8px] rounded-[8px] bg-white/[0.04] p-[12px]">
          <Row kbd={<Kbd>{toggleHotkey}</Kbd>} text="唤出 / 收起面板" />
          <Row kbd={<Kbd>← →</Kbd>} text="选择条目（纵向排列时用 ↑ ↓）" />
          <Row kbd={<Kbd>⏎</Kbd>} text="回贴到之前的 App（单击默认复制，可在设置里改）" />
          <Row kbd={<Kbd>⌘1~9</Kbd>} text="直接回贴第 N 条历史" />
          <Row kbd={<Kbd>Esc</Kbd>} text="收起面板" />
        </div>

        <div className="mt-[14px] space-y-[6px] text-[12px]/[18px] text-white/55">
          <p>
            <span className="text-white/80">数据只在本机。</span>
            历史与截图存放在 ~/.clipbase，密码与加密口令存进系统钥匙串，
            默认不联网、不上传。
          </p>
          <p>
            <span className="text-white/80">两项权限按需授权。</span>
            「辅助功能」用于自动粘贴（未授权时只复制到剪贴板并提示原因）；
            「Finder 扩展」让右键菜单直接出现新建文件、开终端等动作。
          </p>
        </div>

        <div className="mt-[18px] flex items-center justify-end gap-[8px]">
          <Btn variant="ghost" onClick={dismiss}>
            以后再说
          </Btn>
          <Btn onClick={openPermissions}>查看权限设置</Btn>
          <Btn variant="primary" onClick={dismiss}>
            开始使用
          </Btn>
        </div>
      </div>
    </div>
  );
}

/** 一行快捷键说明：左键位、右描述。 */
function Row({ kbd, text }: { kbd: React.ReactNode; text: string }) {
  return (
    <div className="flex items-center gap-[10px]">
      <span className="w-[64px] shrink-0">{kbd}</span>
      <span className="min-w-0 text-[12px]/[16px] text-white/70">{text}</span>
    </div>
  );
}
