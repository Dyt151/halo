/**
 * 权限与授权区块（收敛自原独立「权限」页）。
 *
 * 原来权限散在 5 处：独立权限页、通用页的开机启动、环形转换页的辅助功能行、
 * 右键菜单页的桌面右键接管、状态栏页的定位授权。用户要「在一处完成所有授权」，
 * 于是这里把系统级权限收成一组：
 *
 *   - 辅助功能：回贴、Option＋拖动转换、桌面右键接管都依赖它；
 *   - 开机启动：登录项由 SMAppService 托管，授权后可能还要系统点头；
 *   - Finder 扩展：右键菜单靠它把动作插进系统菜单；
 *   - 定位（可选）：只影响状态栏读 Wi-Fi 名称，不授权不影响其它功能。
 *
 * 所有状态都来自后端实时查询，不做缓存——在系统设置里改完回来点「刷新」即可。
 */
import { useCallback, useEffect, useState } from "react";
import { api } from "../lib/ipc";
import { errorMessage } from "../lib/errors";
import type { FinderIntegrationStatus } from "../lib/types";
import { Btn, CardGroup, Divider, Notice, Row, cx } from "./ui";

type State = "granted" | "missing" | "unsupported" | "unknown";

/** 定位授权：后端返回 authorized / not_determined / denied / restricted。 */
type LocationState = "authorized" | "not_determined" | "denied" | "restricted";

/**
 * 定位服务的系统设置深链。
 *
 * 只开「隐私与安全性」总面板的话，用户还得自己往下找「定位服务」；
 * 直接指到那一页更省事。macOS 13+ 的系统设置用这个锚点。
 */
const LOCATION_PANE = "com.apple.settings.PrivacySecurity.extension?Privacy_LocationServices";

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

/** 定位授权的展示文案：四态各给一句人话。 */
function locationLabel(state: LocationState | null): string {
  switch (state) {
    case "authorized":
      return "已授权";
    case "not_determined":
      return "未申请";
    case "denied":
      return "已拒绝";
    case "restricted":
      return "受系统限制";
    default:
      return "状态未知";
  }
}

/** 定位授权的「是否需要用户处理」。只有 authorized 算就绪。 */
function locationNeedsAction(state: LocationState | null): boolean {
  return state !== null && state !== "authorized";
}

export default function PermissionSection() {
  const [accessibility, setAccessibility] = useState<State>("unknown");
  const [loginItem, setLoginItem] = useState<State>("unknown");
  const [loginHint, setLoginHint] = useState<string | null>(null);
  const [finder, setFinder] = useState<State>("unknown");
  const [finderInfo, setFinderInfo] = useState<FinderIntegrationStatus | null>(null);
  const [location, setLocation] = useState<LocationState | null>(null);
  const [busy, setBusy] = useState<null | "finder" | "login" | "access" | "location">(null);
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
    try {
      setLocation((await api.statusBarLocationAccess()) as LocationState);
    } catch {
      setLocation(null);
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

  /**
   * 修复辅助功能授权。
   *
   * ad-hoc 签名的 App 每次重新构建都会换 cdhash，系统留着一条指向旧二进制的
   * 记录：界面上开关是打开的，`AXIsProcessTrusted()` 却一直是 false。这种时候
   * 重新勾选也没用，必须先 reset 再登记（实际最常遇到的坑就是这个）。
   */
  const repairAccess = async () => {
    setBusy("access");
    setMessage(null);
    try {
      const result = await api.ringGestureRepair();
      setMessage(result.detail);
      // 修复后重新拉一次真实状态，别让界面停在修复前的旧值上。
      await refresh();
    } catch (e) {
      setMessage(`修复授权失败：${errorMessage(e)}`);
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
      // 需要系统点头时要说清楚「已经提交但还没生效」，
      // 否则用户以为开关没生效又去点一遍。
      if (next.requires_approval) {
        setMessage("已注册登录项，请在系统设置里允许。");
      }
    } catch (e) {
      setMessage(`切换开机启动失败：${errorMessage(e)}`);
    } finally {
      setBusy(null);
    }
  };

  /**
   * 申请定位授权。
   *
   * 后端返回三态：`requested`（系统已弹窗，等用户点同意）、
   * `open_location_settings`（系统不再弹窗，得自己去设置里开）、
   * `not_needed`（系统版本不需要定位）。
   */
  const requestLocation = async () => {
    setBusy("location");
    setMessage(null);
    try {
      const result = await api.statusBarRequestLocation();
      if (result === "requested") {
        setMessage("已弹出定位授权提示，点「允许」后点「刷新」查看状态。");
      } else if (result === "open_location_settings") {
        setMessage("系统不再重复弹窗，请到「系统设置 → 隐私与安全性 → 定位服务」里允许 Halo。");
      } else {
        setMessage("当前系统不需要定位权限。");
      }
      await refresh();
    } catch (e) {
      setMessage(`申请定位权限失败：${errorMessage(e)}`);
    } finally {
      setBusy(null);
    }
  };

  const locationRow = (
    <Row label="定位服务">
      <Status
        state={location === "authorized" ? "granted" : location === null ? "unknown" : "missing"}
        granted={locationLabel(location)}
        missing={locationLabel(location)}
        unsupported="当前系统不需要"
      />
      {locationNeedsAction(location) && (
        <Btn size="sm" disabled={busy === "location"} onClick={() => void requestLocation()}>
          {busy === "location" ? "申请中…" : "申请授权"}
        </Btn>
      )}
      <Btn size="sm" onClick={() => void api.openSystemSettings(LOCATION_PANE)}>
        打开系统设置
      </Btn>
    </Row>
  );

  return (
    <CardGroup
      title="权限与授权"
      action={
        <Btn size="sm" onClick={() => void refresh()}>
          刷新
        </Btn>
      }
    >
      <div className="space-y-[2px]">
        <Row label="辅助功能">
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
          <Btn size="sm" disabled={busy === "access"} onClick={() => void repairAccess()}>
            修复授权
          </Btn>
          <Btn size="sm" onClick={() => void api.openSystemSettings("accessibility")}>
            打开系统设置
          </Btn>
        </Row>

        <Divider />

        <Row label="开机启动" hint={loginHint ?? undefined}>
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

        <Divider />

        <Row
          label="Finder 扩展"
          hint={
            finderInfo?.supported && finderInfo.appex_present === false
              ? "当前是开发构建，App 内没有 Finder 扩展"
              : undefined
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
        </Row>

        <Divider />

        {locationRow}
      </div>

      {message && (
        <div className="mt-[12px]">
          <Notice tone="info">{message}</Notice>
        </div>
      )}
    </CardGroup>
  );
}
