import { useCallback, useEffect, useState } from "react";
import { api } from "../lib/ipc";
import { errorMessage } from "../lib/errors";
import type { FinderIntegrationStatus } from "../lib/types";
import { Btn, CardGroup, Notice, Row, cx } from "./ui";

/// 菜单由 Finder Sync 扩展直接插入右键菜单，与 Windows 的「新建」位置类似；
/// 不再出现在「服务」子菜单里。
const MENU_ITEMS: { title: string; description: string }[] = [
  {
    title: "新建文件",
    description: "文本文件 (.txt)、Markdown (.md)；Word / Excel / PowerPoint 用 WPS 自带的「使用 WPS 新建」",
  },
  {
    title: "在终端中打开",
    description: "在新 Terminal 窗口中 cd 到所选文件夹",
  },
];

export default function ContextMenuSettings() {
  const [status, setStatus] = useState<FinderIntegrationStatus | null>(null);
  const [accessibility, setAccessibility] = useState<boolean | null>(null);
  const [busy, setBusy] = useState<null | "toggle" | "refresh" | "grant">(null);
  const [message, setMessage] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setBusy("refresh");
    setMessage(null);
    try {
      setStatus(await api.finderIntegrationStatus());
    } catch (e) {
      setMessage(`读取 Finder 扩展状态失败：${errorMessage(e)}`);
    }
    try {
      setAccessibility(await api.accessibilityStatus());
    } catch {
      setAccessibility(null);
    }
    setBusy(null);
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const toggle = async () => {
    if (!status) return;
    setBusy("toggle");
    setMessage(null);
    try {
      const next = await api.setFinderExtensionEnabled(!status.enabled);
      setStatus(next);
      setMessage(
        next.enabled
          ? "Finder 扩展已启用。到 Finder 里右键即可看到「新建文件 / 在终端中打开」。"
          : "Finder 扩展已停用。",
      );
    } catch (e) {
      setMessage(`切换 Finder 扩展失败：${errorMessage(e)}`);
    } finally {
      setBusy(null);
    }
  };

  const grant = async () => {
    setBusy("grant");
    setMessage(null);
    try {
      const trusted = await api.requestAccessibility();
      setAccessibility(trusted);
      setMessage(
        trusted
          ? "辅助功能已授权，桌面右键会在几秒内自动生效。"
          : "已弹出系统提示：请在「辅助功能」列表里勾选 Halo（若之前有旧条目，先移除再勾选），然后点「刷新状态」。",
      );
    } catch (e) {
      setMessage(`请求辅助功能权限失败：${errorMessage(e)}`);
    } finally {
      setBusy(null);
    }
  };

  const enabled = status?.enabled ?? false;
  const appexMissing = status?.supported === true && status.appex_present === false;

  return (
    <div className="space-y-[16px]">
      <CardGroup title="Finder 右键菜单">
        <p className="text-[13px]/[18px] text-white/60">
          启用 Finder 扩展后，<span className="font-medium text-white">Finder 窗口</span>
          里右键，菜单会直接出现 Halo 的「新建文件」和「在终端中打开」，
          不需要再进「服务」子菜单。选中文件夹时以该文件夹为目标，选中文件时以其所在文件夹为目标。
          环形转换不在右键菜单里：在 Finder 或桌面按住 Option 再左键拖动文件即可唤出（同样需要「辅助功能」权限）。
        </p>
        <p className="mt-[8px] text-[13px]/[18px] text-white/60">
          <span className="font-medium text-white">桌面空白处和桌面图标上</span>
          的右键也会出现同一组 Halo 项：未授权「辅助功能」时，它们追加在 Finder
          原生菜单底部；授权后由 Halo 接管，菜单换成自己的版本——空白处提供新建文件、
          新建文件夹、终端、粘贴和桌面背景，右键图标还会提供打开、显示简介和移到废纸篓，
          并保留「显示 Finder 菜单」随时退回系统菜单。
        </p>
        <ul className="mt-[12px] space-y-[8px]">
          {MENU_ITEMS.map((item) => (
            <li key={item.title} className="flex items-start gap-[8px]">
              <span className="mt-[6px] h-[6px] w-[6px] shrink-0 rounded-full bg-white/25" />
              <span className="min-w-0">
                <span className="text-[13px]/[18px] text-white">{item.title}</span>
                <span className="ml-[8px] text-[12px]/[16px] text-white/45">
                  {item.description}
                </span>
              </span>
            </li>
          ))}
        </ul>
      </CardGroup>

      <CardGroup title="扩展状态">
        <Row
          label={
            <span className="flex items-center gap-[6px]">
              <span
                className={cx(
                  "h-[6px] w-[6px] shrink-0 rounded-full",
                  enabled ? "bg-success-soft" : "bg-warning",
                )}
              />
              {enabled ? "已启用" : status?.installed ? "已安装，未启用" : "未安装"}
            </span>
          }
        >
          <Btn
            variant="primary"
            disabled={busy !== null || !status?.supported || appexMissing}
            onClick={() => void toggle()}
          >
            {busy === "toggle" ? "处理中…" : enabled ? "停用" : "启用扩展"}
          </Btn>
          <Btn size="sm" disabled={busy !== null} onClick={() => void refresh()}>
            {busy === "refresh" ? "刷新中…" : "刷新状态"}
          </Btn>
        </Row>

        <Row
          label={
            <span className="flex items-center gap-[6px]">
              <span
                className={cx(
                  "h-[6px] w-[6px] shrink-0 rounded-full",
                  accessibility ? "bg-success-soft" : "bg-warning",
                )}
              />
              桌面右键接管
              <span className="text-[12px]/[16px] text-white/45">
                {accessibility === null
                  ? "（状态未知）"
                  : accessibility
                    ? "（辅助功能已授权）"
                    : "（需要辅助功能权限）"}
              </span>
            </span>
          }
        >
          <Btn
            variant={accessibility ? undefined : "primary"}
            disabled={busy !== null || accessibility === true}
            onClick={() => void grant()}
          >
            {busy === "grant"
              ? "处理中…"
              : accessibility
                ? "已授权"
                : "授予辅助功能"}
          </Btn>
        </Row>

        {accessibility === false && enabled && (
          <div className="mt-[12px]">
            <Notice tone="info">
              未授权「辅助功能」时，桌面右键仍会出现 Halo 项，但用的是 Finder 原生菜单；
              授权后会换成 Halo 自己的菜单，同时恢复 Option＋拖动的环形转换。
              点上方「授予辅助功能」并在系统提示里勾选 Halo；授权后最多等 3 秒自动生效，不用重启。
              重新打包 App 后系统可能要求重新勾选一次。
            </Notice>
          </div>
        )}

        {!status?.supported && (
          <p className="mt-[8px] text-[12px]/[16px] text-white/45">
            Finder 扩展仅支持 macOS。
          </p>
        )}

        {appexMissing && (
          <div className="mt-[12px]">
            <Notice tone="info">
              当前运行的是开发构建，App 内没有 Finder 扩展。请用
              <span className="mx-[4px] font-medium text-white">npm run tauri build</span>
              生成完整的 Halo.app，再启动它。
            </Notice>
          </div>
        )}

        {!appexMissing && status?.appex_present && !status.installed && (
          <div className="mt-[12px]">
            <Notice tone="info">
              系统插件库里还没有这条扩展记录（App 重新打包后常见），点「启用扩展」即可重新注册。
            </Notice>
          </div>
        )}

        {message && (
          <div className="mt-[12px]">
            <Notice tone="info">{message}</Notice>
          </div>
        )}
      </CardGroup>

      <CardGroup title="如果右键菜单里看不到" subtitle="按顺序排查">
        <ol className="space-y-[6px] pl-[16px] text-[12px]/[18px] text-white/60">
          <li className="list-decimal">
            确认当前运行的是打包后的 Halo.app（不是
            <span className="mx-[4px] font-mono text-white/75">tauri dev</span>
            的裸二进制），并点上方「启用扩展」。
          </li>
          <li className="list-decimal">
            桌面接管依赖「辅助功能」权限：在设置 → 权限中心授权后最多等 3 秒会自动生效，
            不需要重启 App；Finder 窗口内的菜单不需要这个权限。
          </li>
          <li className="list-decimal">
            macOS 15 起系统设置不再提供 Finder 扩展开关，本页直接用
            <span className="mx-[4px] font-mono text-white/75">pluginkit</span>
            启用；macOS 14 及更早也可以在 系统设置 → 扩展 → Finder 扩展 中勾选。
          </li>
          <li className="list-decimal">
            仍不出现时重启 Finder：按住 Option 右键 Dock 里的 Finder → 重新开启，
            或在终端执行 <span className="font-mono text-white/75">killall Finder</span>。
          </li>
          <li className="list-decimal">
            扩展只监视主目录、桌面和外接卷。系统目录（如 /usr）不会出现 Halo 菜单。
          </li>
        </ol>
      </CardGroup>

      <p className="text-[12px]/[16px] text-white/45">
        菜单在右键菜单中的具体位置由 macOS 决定，第三方扩展无法像 Windows 一样
        精确控制到某一分区；但动作会直接出现，不需要展开「服务」。
      </p>
    </div>
  );
}
