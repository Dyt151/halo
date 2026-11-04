/**
 * 权限中心（清单 42 / 评审 F1）。
 *
 * 把「回贴为什么只复制」「Finder 扩展为什么没菜单」「开机启动是否生效」三件事
 * 从「出错后才发现」提前到一页集中展示：状态 + 用途 + 刷新 + 直达系统设置。
 * 所有状态都来自后端实时查询，不做缓存——系统设置里改完回来点刷新即可。
 */
import { useCallback, useEffect, useState } from "react";
import { api } from "../lib/ipc";
import { errorMessage } from "../lib/errors";
import type { FinderIntegrationStatus } from "../lib/types";
import { Btn, CardGroup, Notice, Row, cx } from "./ui";

type State = "granted" | "missing" | "unsupported" | "unknown";

/** 状态点 + 文案（颜色只表达「需要处理 / 已就绪」两档）。 */
function Status({ state, granted, missing, unsupported }: {
  state: State;
  granted: string;
  missing: string;
  unsupported: string;
}) {
  const label =
    state === "granted" ? granted : state === "unsupported" ? unsupported : missing;
  return (
    <span className="flex items-center gap-[6px] text-[12px]/[16px] text-white/60">
      <span
        className={cx(
          "h-[6px] w-[6px] shrink-0 rounded-full",
          state === "granted" ? "bg-success-soft" : state === "unsupported" ? "bg-white/25" : "bg-warning",
        )}
      />
      {label}
    </span>
  );
}

export default function PermissionCenter() {
  const [accessibility, setAccessibility] = useState<State>("unknown");
  const [loginItem, setLoginItem] = useState<State>("unknown");
  const [loginHint, setLoginHint] = useState<string | null>(null);
  const [finder, setFinder] = useState<State>("unknown");
  const [finderInfo, setFinderInfo] = useState<FinderIntegrationStatus | null>(null);
  const [busy, setBusy] = useState<null | "finder" | "login" | "access">(null);
  const [message, setMessage] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      setAccessibility((await api.accessibilityStatus()) ? "granted" : "missing");
    } catch {
      setAccessibility("unknown");
    }
    try {
      const status = await api.loginItemStatus();
      setLoginItem(
        !status.supported
          ? "unsupported"
          : status.enabled
            ? "granted"
            : "missing",
      );
      setLoginHint(
        status.requires_approval ? "已提交，等待你在系统设置里批准" : null,
      );
    } catch {
      setLoginItem("unknown");
    }
    try {
      const status = await api.finderIntegrationStatus();
      setFinderInfo(status);
      setFinder(!status.supported ? "unsupported" : status.enabled ? "granted" : "missing");
    } catch {
      setFinder("unknown");
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  /** 请求辅助功能授权：系统会弹窗并把 Halo 加进白名单，用户勾选后回来刷新。 */
  const requestAccess = async () => {
    setBusy("access");
    setMessage(null);
    try {
      const granted = await api.requestAccessibility();
      setAccessibility(granted ? "granted" : "missing");
      setMessage(
        granted
          ? "辅助功能已授权：回贴与 Option＋拖动转换都可以用了。"
          : "已弹出系统提示：请在「辅助功能」列表里勾选 Halo（若之前有旧条目先移除再勾选）。",
      );
    } catch (e) {
      setMessage(`请求辅助功能权限失败：${errorMessage(e)}`);
    } finally {
      setBusy(null);
    }
  };

  const toggleFinder = async () => {
    setBusy("finder");
    setMessage(null);
    try {
      const next = await api.setFinderExtensionEnabled(finder !== "granted");
      setFinderInfo(next);
      setFinder(next.supported ? (next.enabled ? "granted" : "missing") : "unsupported");
      setMessage(
        next.enabled
          ? "Finder 扩展已启用，右键菜单里会直接出现 Halo。"
          : "Finder 扩展已停用。",
      );
    } catch (e) {
      setMessage(`切换 Finder 扩展失败：${errorMessage(e)}`);
    } finally {
      setBusy(null);
    }
  };

  const toggleLogin = async () => {
    setBusy("login");
    setMessage(null);
    try {
      const next = await api.setLoginItem(loginItem !== "granted");
      setLoginItem(!next.supported ? "unsupported" : next.enabled ? "granted" : "missing");
      setLoginHint(next.requires_approval ? "已提交，等待你在系统设置里批准" : null);
    } catch (e) {
      setMessage(`切换开机启动失败：${errorMessage(e)}`);
    } finally {
      setBusy(null);
    }
  };

  return (
    <div className="space-y-[16px]">
      <CardGroup title="权限中心" subtitle="状态改完点刷新；所有入口直达对应系统面板">
        <div className="space-y-[2px]">
          <Row
            label="辅助功能"
            hint="回贴要模拟 ⌘V；Option＋拖动也要靠它接管事件。未授权时两者都不工作"
          >
            <Status
              state={accessibility}
              granted="已授权"
              missing="未授权"
              unsupported="非 macOS"
            />
            <Btn
              size="sm"
              disabled={busy === "access"}
              onClick={() => void requestAccess()}
            >
              {busy === "access" ? "请求中…" : "请求授权"}
            </Btn>
            <Btn size="sm" onClick={() => void refresh()}>
              刷新
            </Btn>
            <Btn size="sm" onClick={() => void api.openSystemSettings("accessibility")}>
              打开系统设置
            </Btn>
          </Row>

          <div className="h-px bg-white/[0.06]" />

          <Row
            label="开机启动"
            hint={
              loginHint ??
              "由系统登录项（SMAppService）托管，登录后在后台待命，不弹窗口"
            }
          >
            <Status
              state={loginItem}
              granted="已启用"
              missing="未启用"
              unsupported="当前系统不支持"
            />
            <Btn
              size="sm"
              disabled={busy === "login" || loginItem === "unsupported"}
              onClick={() => void toggleLogin()}
            >
              {loginItem === "granted" ? "关闭" : "启用"}
            </Btn>
            <Btn size="sm" onClick={() => void api.openSystemSettings("login-items")}>
              打开系统设置
            </Btn>
          </Row>

          <div className="h-px bg-white/[0.06]" />

          <Row
            label="Finder 扩展"
            hint={
              finder === "granted"
                ? "已启用：Finder 右键直接出现「新建文件 / 在终端中打开」"
                : finderInfo?.supported && finderInfo.appex_present === false
                  ? "当前是开发构建，App 内没有 Finder 扩展；打包后会随 App 一起安装"
                  : "未启用时右键菜单里看不到 Halo 的动作"
            }
          >
            <Status
              state={finder}
              granted="已启用"
              missing="未启用"
              unsupported="非 macOS"
            />
            <Btn
              size="sm"
              disabled={
                busy === "finder" ||
                finder === "unsupported" ||
                (finderInfo?.supported === true && finderInfo.appex_present === false)
              }
              onClick={() => void toggleFinder()}
            >
              {busy === "finder" ? "处理中…" : finder === "granted" ? "停用" : "启用"}
            </Btn>
            <Btn size="sm" onClick={() => void refresh()}>
              刷新
            </Btn>
          </Row>
        </div>

        {message && (
          <div className="mt-[12px]">
            <Notice tone="info">{message}</Notice>
          </div>
        )}
      </CardGroup>

      <p className="text-[12px]/[16px] text-white/45">
        系统会缓存辅助功能授权状态：在系统设置里勾选后回到本页点「刷新」即可，
        无需重启 App；若仍未生效，可退出并重新打开 Halo。
      </p>
      <p className="text-[12px]/[16px] text-white/45">
        本地重新打包会重签名 App，macOS 会把授权当成「另一个程序」：列表里可能还
        留着旧条目但不再生效。遇到「之前能用、重打包后失灵」时，先在列表里用减号
        移除 Halo，再点「请求授权」重新加入。
      </p>
    </div>
  );
}
