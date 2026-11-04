/**
 * 设置中心「模块」分区：安装 / 授权启用 / 停用 / 卸载。
 *
 * 后端（`commands/modules.rs`）一直是完整的，只是主窗口收窄成设置中心时
 * 把模块页一起删掉了，于是 `loadModules()` 拉到的数据没有任何 UI 消费。
 * 这里按「列表 + 就地授权」的方式补回来：
 * - 启用必须显式过一遍权限（授权是 manifest 声明权限的子集，后端会校验）；
 * - 停用/卸载立即生效，贡献点跟随 `module://changed` 刷新；
 * - 不引入嵌套弹窗，授权面板就地展开，避免再叠一层模态。
 */
import { useEffect, useMemo, useState } from "react";
import { Btn, CardGroup, EmptyState, Notice, Pill, Spinner } from "../components/ui";
import { confirm } from "../lib/dialog";
import { api } from "../lib/ipc";
import { toast, toastError } from "../lib/toast";
import {
  CAPABILITY_LABELS,
  type Capability,
  type Contribution,
  type ModuleRecord,
} from "../lib/types";
import { useClipStore } from "../store/useClipStore";

/** 运行时标识 → 人话。 */
const RUNTIME_LABEL: Record<ModuleRecord["manifest"]["runtime"], string> = {
  sidecar: "独立进程",
  iframe: "网页界面",
  wasm: "WASM",
};

/** 运行状态 → 圆点颜色 + 文案。 */
const STATE_VIEW: Record<ModuleRecord["state"], { dot: string; text: string; tone: string }> = {
  running: { dot: "bg-success", text: "运行中", tone: "text-success-soft" },
  starting: { dot: "bg-accent-500", text: "启动中", tone: "text-white/70" },
  stopped: { dot: "bg-white/30", text: "已停用", tone: "text-white/45" },
  crashed: { dot: "bg-danger", text: "已崩溃", tone: "text-danger-soft" },
};

/** 贡献点类型 → 短标签。 */
const CONTRIBUTION_LABEL: Record<Contribution["type"], string> = {
  tray: "托盘",
  clipboard_action: "剪贴板动作",
  shortcut: "快捷键",
  settings: "设置页",
  command: "命令",
};

/**
 * `installed_at` 是毫秒时间戳（Rust `host::now_ms`）。
 * 早期版本曾按秒落库，这里兼容一下，避免老数据算出 5 万年后的日期。
 */
function formatInstalledAt(ts: number): string {
  if (!ts) return "";
  const ms = ts < 1e11 ? ts * 1000 : ts;
  const d = new Date(ms);
  if (Number.isNaN(d.getTime())) return "";
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

export default function Modules() {
  const modules = useClipStore((s) => s.modules);
  const loading = useClipStore((s) => s.modulesLoading);
  const loadModules = useClipStore((s) => s.loadModules);

  /** 正在等后端返回的模块 id（按钮/开关临时禁用，防重复提交）。 */
  const [pending, setPending] = useState<string | null>(null);
  const [installing, setInstalling] = useState(false);
  /** 就地展开授权面板的模块 id；null 表示没有展开的。 */
  const [granting, setGranting] = useState<string | null>(null);
  const [draftGrants, setDraftGrants] = useState<Set<Capability>>(new Set());

  useEffect(() => {
    void loadModules();
  }, [loadModules]);

  const grantingModule = useMemo(
    () => modules.find((m) => m.id === granting) ?? null,
    [granting, modules],
  );

  const refresh = async () => {
    await loadModules();
  };

  const install = async () => {
    const picked = await api.pickDirectory().catch((e) => {
      toastError(e, "无法打开选择面板");
      return [] as string[];
    });
    const path = picked[0];
    if (!path) return;
    setInstalling(true);
    try {
      const manifest = await api.installModule(path);
      toast(`已安装「${manifest.name}」v${manifest.version}`, { tone: "success" });
      await refresh();
    } catch (e) {
      toastError(e, "安装失败");
    } finally {
      setInstalling(false);
    }
  };

  const beginEnable = (m: ModuleRecord) => {
    const declared = m.manifest.permissions;
    if (declared.length === 0) {
      void applyEnable(m.id, []);
      return;
    }
    setGranting(m.id);
    // 之前授过就沿用用户当时的选择；首次启用则预勾全部声明权限（"授予并启用"那一下才是同意）。
    setDraftGrants(new Set(m.grants.length > 0 ? m.grants : declared));
  };

  const applyEnable = async (id: string, grants: Capability[]) => {
    setPending(id);
    try {
      await api.enableModule(id, grants);
      toast("模块已启用", { tone: "success" });
      setGranting(null);
      await refresh();
    } catch (e) {
      toastError(e, "启用失败");
    } finally {
      setPending(null);
    }
  };

  const disable = async (m: ModuleRecord) => {
    setPending(m.id);
    try {
      await api.disableModule(m.id);
      toast(`「${m.name}」已停用`, { tone: "info" });
      if (granting === m.id) setGranting(null);
      await refresh();
    } catch (e) {
      toastError(e, "停用失败");
    } finally {
      setPending(null);
    }
  };

  const uninstall = async (m: ModuleRecord) => {
    const ok = await confirm(
      `确定卸载「${m.name}」？模块目录与已授予的权限会一并移除，此操作不可撤销。`,
      { danger: true, title: "卸载模块", okLabel: "卸载" },
    );
    if (!ok) return;
    setPending(m.id);
    try {
      await api.uninstallModule(m.id);
      toast(`已卸载「${m.name}」`, { tone: "info" });
      if (granting === m.id) setGranting(null);
      await refresh();
    } catch (e) {
      toastError(e, "卸载失败");
    } finally {
      setPending(null);
    }
  };

  const openModuleDir = async () => {
    try {
      const info = await api.appInfo();
      await api.revealPath(`${info.data_dir}/modules`);
    } catch (e) {
      toastError(e, "打不开模块目录");
    }
  };

  const toggleGrant = (cap: Capability) => {
    setDraftGrants((prev) => {
      const next = new Set(prev);
      if (next.has(cap)) next.delete(cap);
      else next.add(cap);
      return next;
    });
  };

  return (
    <div className="space-y-[16px]">
      <CardGroup
        title="模块"
        subtitle="按目录安装第三方扩展。模块默认停用，启用时要逐项授权它需要的能力。"
      >
        <div className="flex flex-wrap items-center gap-[8px]">
          <Btn variant="primary" size="sm" onClick={() => void install()} disabled={installing}>
            {installing ? "安装中…" : "安装模块…"}
          </Btn>
          <Btn size="sm" variant="ghost" onClick={() => void openModuleDir()}>
            打开模块目录
          </Btn>
          <Btn size="sm" variant="ghost" onClick={() => void refresh()} disabled={loading}>
            刷新
          </Btn>
          {loading && <Spinner size={13} />}
        </div>
        <p className="mt-[10px] text-[12px]/[16px] text-white/45">
          选择包含 <span className="text-white/70">module.json</span> 的目录即可安装；
          仓库里的 <span className="text-white/70">modules.example/filee</span> 是一个可用的示例。
        </p>
      </CardGroup>

      {grantingModule && (
        <Notice tone="info" onClose={() => setGranting(null)}>
          <div className="space-y-[8px]">
            <div className="font-medium text-white">
              启用「{grantingModule.name}」需要授权
            </div>
            <div className="space-y-[6px]">
              {grantingModule.manifest.permissions.map((cap) => {
                const checked = draftGrants.has(cap);
                return (
                  <label
                    key={cap}
                    className="flex cursor-pointer items-center gap-[8px] rounded-[6px] px-[4px] py-[2px] hover:bg-white/[0.05]"
                  >
                    <input
                      type="checkbox"
                      checked={checked}
                      onChange={() => toggleGrant(cap)}
                      className="h-[13px] w-[13px] accent-[#E8A33D]"
                    />
                    <span className="text-white/80">{CAPABILITY_LABELS[cap]}</span>
                    <span className="text-white/35">{cap}</span>
                  </label>
                );
              })}
            </div>
            <div className="flex items-center gap-[8px]">
              <Btn
                size="sm"
                variant="primary"
                disabled={pending === grantingModule.id}
                onClick={() => void applyEnable(grantingModule.id, [...draftGrants])}
              >
                授予并启用
              </Btn>
              <Btn size="sm" variant="ghost" onClick={() => setGranting(null)}>
                取消
              </Btn>
            </div>
          </div>
        </Notice>
      )}

      {modules.length === 0 ? (
        <CardGroup>
          <EmptyState
            title={loading ? "正在读取模块…" : "还没有安装任何模块"}
            hint="Halo 本身不依赖模块，装不装都能正常使用。"
          />
        </CardGroup>
      ) : (
        <div className="space-y-[10px]">
          {modules.map((m) => {
            const view = STATE_VIEW[m.state] ?? STATE_VIEW.stopped;
            const busy = pending === m.id;
            const declared = m.manifest.permissions;
            const contributions = m.manifest.contributions ?? [];
            return (
              <CardGroup key={m.id}>
                <div className="flex items-start gap-[12px]">
                  <div className="min-w-0 flex-1">
                    <div className="flex flex-wrap items-center gap-[8px]">
                      <span className="truncate text-[13px]/[18px] font-semibold text-white">
                        {m.name}
                      </span>
                      <span className="text-[11px]/[14px] tabular-nums text-white/45">
                        v{m.version}
                      </span>
                      <Pill className="pointer-events-none" selected={false}>
                        {RUNTIME_LABEL[m.manifest.runtime] ?? m.manifest.runtime}
                      </Pill>
                      <span className={`inline-flex items-center gap-[5px] text-[11px]/[14px] ${view.tone}`}>
                        <span className={`h-[6px] w-[6px] rounded-full ${view.dot}`} aria-hidden />
                        {view.text}
                      </span>
                    </div>
                    <div className="mt-[3px] flex flex-wrap items-center gap-[8px] text-[11px]/[14px] text-white/35">
                      <span className="font-mono">{m.id}</span>
                      {formatInstalledAt(m.installed_at) && (
                        <span>安装于 {formatInstalledAt(m.installed_at)}</span>
                      )}
                    </div>
                  </div>

                  <div className="flex shrink-0 items-center gap-[8px]">
                    <Btn
                      size="sm"
                      variant={m.enabled ? "secondary" : "primary"}
                      disabled={busy}
                      onClick={() => (m.enabled ? void disable(m) : beginEnable(m))}
                    >
                      {busy ? "处理中…" : m.enabled ? "停用" : "启用"}
                    </Btn>
                    <Btn size="sm" variant="danger" disabled={busy} onClick={() => void uninstall(m)}>
                      卸载
                    </Btn>
                  </div>
                </div>

                {declared.length > 0 && (
                  <div className="mt-[10px] flex flex-wrap items-center gap-[6px]">
                    <span className="text-[11px]/[14px] text-white/35">权限</span>
                    {declared.map((cap) => {
                      const granted = m.grants.includes(cap);
                      return (
                        <Pill
                          key={cap}
                          selected={granted}
                          className="pointer-events-none"
                          title={granted ? "已授予" : "未授予"}
                        >
                          {CAPABILITY_LABELS[cap]}
                        </Pill>
                      );
                    })}
                  </div>
                )}

                {contributions.length > 0 && (
                  <div className="mt-[8px] flex flex-wrap items-center gap-[6px] text-[11px]/[14px] text-white/35">
                    <span>贡献</span>
                    {contributions.map((c, i) => (
                      <span key={`${c.type}-${i}`} className="rounded-[5px] bg-white/[0.06] px-[6px] py-[1px] text-white/60">
                        {CONTRIBUTION_LABEL[c.type] ?? c.type}
                        {"label" in c && c.label ? `：${c.label}` : ""}
                      </span>
                    ))}
                  </div>
                )}

                {m.state === "crashed" && (
                  <div className="mt-[8px]">
                    <Notice tone="danger">
                      模块进程异常退出。可先「停用」再「启用」重试；反复崩溃请查看
                      {" "}<span className="font-mono">~/.halo/logs</span>。
                    </Notice>
                  </div>
                )}
              </CardGroup>
            );
          })}
        </div>
      )}
    </div>
  );
}
