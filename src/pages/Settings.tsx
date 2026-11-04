/**
 * 设置分区内容（由主窗口侧边栏驱动，评审任务 1–3、28）。
 *
 * 不再有自带的顶部 tab / 第二套侧边栏：当前分区由 `section` 传入，
 * 本组件只负责渲染对应分区。通用/剪贴板共用一份带防抖自动保存的草稿；
 * 其余分区（右键/同步/权限/关于/环形转换/历史）为自含组件。
 * 快捷回复 / 模块 分区已下线，侧边栏不再给出入口；快捷键分区已恢复
 * （HotkeySettings，含录制与系统注册结果回显）。
 */
import { useCallback, useEffect, useRef, useState } from "react";
import ContextMenuSettings from "../components/ContextMenuSettings";
import HotkeySettings from "../components/HotkeySettings";
import PermissionSection from "../components/PermissionSection";
import SyncSettings from "../components/SyncSettings";
import BackupSettings from "../components/BackupSettings";
import StatusBarSettings from "./StatusBarSettings";
import RingSettings from "./RingSettings";
import HistoryPage from "./HistoryPage";
import type { MainPage } from "../lib/windows";
import { useClipStore } from "../store/useClipStore";
import { useAppVersion } from "../lib/version";
import { api } from "../lib/ipc";
import { errorMessage } from "../lib/errors";
import { confirm } from "../lib/dialog";
import { toast, toastError } from "../lib/toast";
import type { Config } from "../lib/types";
import {
  Btn,
  CardGroup,
  CloseButton,
  Divider,
  Input,
  Pill,
  Row,
  Segment,
  SelectRow,
  SliderRow,
  Switch,
  ToggleRow,
} from "../components/ui";

const TTL_OPTIONS = [
  { days: 0, label: "无限制" },
  { days: 1, label: "1 天" },
  { days: 7, label: "1 周" },
  { days: 30, label: "1 月" },
  { days: 180, label: "半年" },
  { days: 365, label: "1 年" },
];

const DISK_PRESETS = [
  { mb: 200, label: "200MB" },
  { mb: 500, label: "500MB" },
  { mb: 1024, label: "1GB" },
  { mb: 2048, label: "2GB" },
  { mb: 5120, label: "5GB" },
];

/** 自动保存防抖：macOS 偏好设置惯例是「改完即生效」，不需要手动点保存。 */
const AUTOSAVE_DELAY_MS = 400;

type SaveState = "idle" | "pending" | "saved" | "error";

/** `app_info` 的返回结构（关于页 E10）。 */
interface AppInfo {
  version: string;
  arch: string;
  data_dir: string;
  db_path: string;
}

function formatMb(mb: number): string {
  if (mb >= 1024) return `${Math.round(mb / 1024)} GB`;
  return `${mb} MB`;
}

export default function Settings({ section }: { section: MainPage }) {
  const config = useClipStore((s) => s.config);
  /**
   * 评审 F3：改配置必须走 patch（只提交改动字段）。
   *
   * 旧实现每个设置页各持一份整 Config 草稿并整份写回：别的窗口刚把监控关掉，
   * 这里随手改一下主题就把旧值覆盖回去。现在只把「本次改过的字段」发给后端。
   */
  const patchConfig = useClipStore((s) => s.patchConfig);
  // 草稿仅服务通用/剪贴板两个配置分区（其余分区为自含组件）。
  const [draft, setDraft] = useState<Config | null>(null);
  const [blacklistInput, setBlacklistInput] = useState("");
  const [saveState, setSaveState] = useState<SaveState>("idle");
  // 用户已编辑未保存时，外部 config 刷新（跨窗口改动）不覆盖草稿，
  // 否则输入会被静默清掉（评审 A7）。
  const dirtyRef = useRef(false);
  const saveTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  // 累积「还没提交的改动字段」，flush 时作为一次 patch 发出去。
  const pendingRef = useRef<Partial<Config>>({});

  /**
   * 把累积的改动一次性提交。
   *
   * 失败时把 patch **放回队列**：早期实现先清队列再提交，一旦后端报错，
   * 界面已经是新值、后端还是旧值，用户再改别的字段也不会把这一项带上，
   * 只能靠重启窗口「碰运气」。现在失败后仍留在队列里，下次任意改动或点
   * 「重试」都会重新提交。
   */
  const flush = useCallback(async () => {
    const patch = pendingRef.current;
    pendingRef.current = {};
    dirtyRef.current = false;
    if (Object.keys(patch).length === 0) return;
    setSaveState("pending");
    try {
      await patchConfig(patch);
      setSaveState("saved");
    } catch {
      // 新的改动优先（用户刚改的值不能又被旧快照盖掉）。
      pendingRef.current = { ...patch, ...pendingRef.current };
      dirtyRef.current = true;
      setSaveState("error");
    }
  }, [patchConfig]);

  useEffect(() => {
    if (!dirtyRef.current) setDraft(config);
  }, [config]);

  // 400ms 防抖自动保存：连续拖动滑块只提交一次。
  useEffect(() => {
    if (!draft || !dirtyRef.current) return;

    if (saveTimer.current) clearTimeout(saveTimer.current);
    saveTimer.current = setTimeout(() => {
      saveTimer.current = null;
      void flush();
    }, AUTOSAVE_DELAY_MS);

    return () => {
      if (saveTimer.current) {
        clearTimeout(saveTimer.current);
        saveTimer.current = null;
      }
    };
  }, [draft, flush]);

  /**
   * 卸载前把还没提交的改动发出去。
   *
   * `MainWindow` 对内容区用了 `key={section}`：切分区会把整个子树重建，
   * 上一条 effect 的 cleanup 只「清定时器」而不提交，于是改完一个开关后
   * 400ms 内切页 = 这次改动静默丢掉（界面已切走，用户根本发现不了）。
   */
  useEffect(() => {
    return () => {
      if (Object.keys(pendingRef.current).length > 0) void flush();
    };
  }, [flush]);

  // 非配置分区（自含组件）：直接路由，不依赖通用/剪贴板的草稿。
  // 「permissions」不再是合法分区值：权限已并入通用，旧值由 `normalizeSection`
  // 统一映射成 `general`（见 `lib/section.ts`），所以这里不需要再为它开分支。
  if (section !== "general" && section !== "clipboard") {
    switch (section) {
      case "ring":
        return <RingSettings />;
      case "history":
        return <HistoryPage />;
      case "context-menu":
        return <ContextMenuSettings />;
      case "statusbar":
        return <StatusBarSettings />;
      case "sync":
        return <SyncSettings />;
      case "backup":
        return <BackupSettings />;
      case "about":
        return <AboutSection />;
      default:
        return null;
    }
  }

  const update = (patch: Partial<Config>) => {
    dirtyRef.current = true;
    pendingRef.current = { ...pendingRef.current, ...patch };
    setDraft((prev) => (prev ? { ...prev, ...patch } : prev));
    setSaveState("pending");
  };

  // 权限状态与配置无关：配置加载慢或加载失败时，权限信息照样要能看到
  // （用户正是靠它排查「为什么功能不生效」）。所以权限卡不走草稿守卫，
  // 而是在通用页内部按 draft 是否就绪分别渲染。
  const isGeneral = section === "general";

  if (!draft && !isGeneral) {
    return <p className="py-[32px] text-center text-[13px] text-white/45">加载中…</p>;
  }

  // 收窄用：通用页允许 draft 为空（权限卡照常显示），但下面这些计算
  // 只有 draft 就绪时才有意义，所以统一走这个局部变量。
  const cfg = draft;

  // 「已保存」不再 1.5s 就消失（评审 U11）：保留到下一次改动，用户随时能确认状态。
  const saveStatus =
    saveState === "saved"
      ? "已保存"
      : saveState === "error"
        ? "保存失败"
        : dirtyRef.current || saveState === "pending"
          ? "正在保存…"
          : "更改会自动保存";

  const ttlIndex = Math.max(0, TTL_OPTIONS.findIndex((o) => o.days === (cfg?.history_ttl_days ?? 0)));
  const maxMb = Math.round((cfg?.max_bytes ?? 0) / 1024 / 1024);

  const addBlacklist = () => {
    const name = blacklistInput.trim();
    if (!name || !cfg) return;
    if (cfg.app_blacklist.some((b) => b.toLowerCase() === name.toLowerCase())) {
      setBlacklistInput("");
      return;
    }
    update({ app_blacklist: [...cfg.app_blacklist, name] });
    setBlacklistInput("");
  };

  return (
    <div className="space-y-[16px]">

      {isGeneral && (
        <div className="space-y-[16px]">
          {/* 权限卡与配置无关：配置没加载好时它照样要能看到。 */}
          <PermissionSection />

          {draft ? (
            <CardGroup title="基础设置" subtitle="剪贴板记录与界面外观">
              <div className="space-y-[2px]">
                <ToggleRow
                  label="剪贴板监控"
                  hint="关闭后不再记录新的剪贴内容"
                  checked={draft.monitor_enabled}
                  onChange={(v) => update({ monitor_enabled: v })}
                />
                <Divider />
                <ToggleRow
                  label="提示音"
                  hint="复制入库与回贴成功后播放系统音效"
                  checked={draft.sound_enabled}
                  onChange={(v) => update({ sound_enabled: v })}
                />
                <Divider />
                <Row label="外观" hint="跟随系统时随 macOS 浅色/深色自动切换">
                  <Segment
                    items={[
                      { value: "system", label: "跟随系统" },
                      { value: "dark", label: "深色" },
                      { value: "light", label: "浅色" },
                    ]}
                    value={(draft.theme as "system" | "dark" | "light") ?? "system"}
                    onChange={(v) => update({ theme: v })}
                  />
                </Row>
              </div>
            </CardGroup>
          ) : (
            <p className="py-[32px] text-center text-[13px] text-white/45">加载中…</p>
          )}
        </div>
      )}

      {section === "clipboard" && cfg && (
        <div className="space-y-[16px]">
          <div className="grid grid-cols-1 items-start gap-[16px] md:grid-cols-2">
            <CardGroup title="面板" subtitle="快捷键唤出的悬浮横条">
              <div className="space-y-[2px]">
                <SelectRow
                  label="出现位置"
                  value={cfg.board_position}
                  options={[
                    { value: "follow_cursor", label: "跟随光标" },
                    { value: "screen", label: "屏幕中央" },
                    { value: "bottom", label: "贴底停靠" },
                  ]}
                  onChange={(v) => update({ board_position: v as Config["board_position"] })}
                  width="w-[120px]"
                />
                <Divider />
                <div className="py-[8px]">
                  <SliderRow
                    label="面板高度"
                    value={cfg.board_height_scale}
                    min={20}
                    max={60}
                    step={5}
                    format={(v) => `${v}%`}
                    onChange={(v) => update({ board_height_scale: v })}
                  />
                </div>
                <Divider />
                <SelectRow
                  label="卡片排列"
                  value={cfg.board_style}
                  options={[
                    { value: "horizontal", label: "横向" },
                    { value: "vertical", label: "纵向" },
                  ]}
                  onChange={(v) => update({ board_style: v })}
                  width="w-[120px]"
                />
                <Divider />
                <SelectRow
                  label="文本预览"
                  value={cfg.text_preview}
                  options={[
                    { value: "rich", label: "富文本" },
                    { value: "plain", label: "纯文本" },
                  ]}
                  onChange={(v) => update({ text_preview: v })}
                  width="w-[120px]"
                />
              </div>
            </CardGroup>

            <CardGroup title="交互设置">
              <div className="space-y-[2px]">
                <ToggleRow
                  label="粘贴后自动收起面板"
                  checked={cfg.auto_close_after_paste}
                  onChange={(v) => update({ auto_close_after_paste: v })}
                />
                <Divider />
                <ToggleRow
                  label="粘贴为纯文本"
                  checked={cfg.paste_as_plain}
                  onChange={(v) => update({ paste_as_plain: v })}
                />
                <Divider />
                <ToggleRow
                  label="粘贴前去除尾部空白"
                  hint="回贴文本时去掉末尾的空行/空格"
                  checked={cfg.paste_trim_trailing}
                  onChange={(v) => update({ paste_trim_trailing: v })}
                />
                <Divider />
                <ToggleRow
                  label="单击卡片直接回贴"
                  hint="关闭时单击仅复制，双击或 Enter 回贴"
                  checked={cfg.paste_on_single_click}
                  onChange={(v) => update({ paste_on_single_click: v })}
                />
                <Divider />
                <ToggleRow
                  label="删除前确认"
                  hint="删除单条或清空历史前弹出确认"
                  checked={cfg.confirm_on_delete}
                  onChange={(v) => update({ confirm_on_delete: v })}
                />
              </div>
            </CardGroup>
          </div>

          <CardGroup title="剪贴板堆栈" subtitle="收集多条后统一粘贴">
            <StackPanel />
          </CardGroup>

          <HotkeySettings />

          <CardGroup title="存储与监控" subtitle="保留多少、扫描多勤；修改后立即生效">
            <div className="grid grid-cols-1 items-start gap-[24px] md:grid-cols-2">
              <div className="space-y-[12px]">
                <SliderRow
                  label="磁盘大小上限"
                  value={maxMb}
                  min={100}
                  max={5120}
                  step={50}
                  format={formatMb}
                  onChange={(v) => update({ max_bytes: v * 1024 * 1024 })}
                />
                <div className="flex flex-wrap gap-[6px]">
                  {DISK_PRESETS.map((p) => (
                    <Pill
                      key={p.mb}
                      selected={maxMb === p.mb}
                      onClick={() => update({ max_bytes: p.mb * 1024 * 1024 })}
                    >
                      {p.label}
                    </Pill>
                  ))}
                </div>
              </div>

              <div className="space-y-[12px]">
                <SliderRow
                  label="保留时长"
                  value={ttlIndex}
                  min={0}
                  max={TTL_OPTIONS.length - 1}
                  step={1}
                  format={(v) => TTL_OPTIONS[v].label}
                  onChange={(v) => update({ history_ttl_days: TTL_OPTIONS[v].days })}
                  ticks={TTL_OPTIONS.map((o) => o.label)}
                />
              </div>

              <div className="space-y-[12px]">
                <SliderRow
                  label="最多保留条目"
                  value={cfg.max_items}
                  min={200}
                  max={5000}
                  step={100}
                  format={(v) => `${v} 条`}
                  onChange={(v) => update({ max_items: v })}
                />
              </div>

              <div className="space-y-[12px]">
                <SliderRow
                  label="扫描间隔"
                  value={cfg.poll_interval_ms}
                  min={100}
                  max={2000}
                  step={50}
                  format={(v) => `${v} ms`}
                  onChange={(v) => update({ poll_interval_ms: v })}
                  ticks={["100ms 灵敏", "500ms", "2000ms 省电"]}
                />
              </div>
            </div>
            <div className="mt-[16px] border-t border-white/[0.08] pt-[12px]">
              <SelectRow
                label="敏感内容策略"
                value={cfg.sensitivity}
                options={[
                  { value: "flag", label: "入库并标记" },
                  { value: "ignore", label: "不入库" },
                  { value: "mask", label: "脱敏后入库" },
                ]}
                onChange={(v) => update({ sensitivity: v as Config["sensitivity"] })}
                width="w-[140px]"
              />
            </div>
          </CardGroup>

          <CardGroup title="应用黑名单" subtitle="来自这些 App 的内容不会被记录">
            <div className="flex items-center gap-[8px]">
              <Input
                value={blacklistInput}
                onChange={(e) => setBlacklistInput(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") addBlacklist();
                }}
                placeholder="输入 App 名称，如 Safari"
                className="flex-1"
              />
              <Btn variant="primary" onClick={addBlacklist}>
                添加
              </Btn>
            </div>
            {cfg.app_blacklist.length > 0 ? (
              <div className="mt-[12px] flex flex-wrap gap-[6px]">
                {cfg.app_blacklist.map((name) => (
                  <span
                    key={name}
                    className="inline-flex h-[22px] items-center gap-[6px] rounded-full bg-white/[0.06] pl-[10px] pr-[6px] text-[12px] text-white/60"
                  >
                    {name}
                    <CloseButton
                      title={`移除 ${name}`}
                      tone="danger"
                      onClick={() =>
                        update({ app_blacklist: cfg.app_blacklist.filter((b) => b !== name) })
                      }
                    />
                  </span>
                ))}
              </div>
            ) : (
              <p className="mt-[12px] text-[12px]/[16px] text-white/45">
                暂无黑名单，所有 App 的内容都会被记录。
              </p>
            )}
          </CardGroup>
        </div>
      )}

      <div className="flex items-center justify-end gap-[12px] text-[12px]/[16px]">
        <span
          className={
            saveState === "error"
              ? "text-danger"
              : saveState === "saved"
                ? "text-success-soft"
                : "text-white/40"
          }
          role="status"
        >
          {saveStatus}
        </span>
        {saveState === "error" && (
          <Btn size="sm" onClick={() => void flush()}>
            重试
          </Btn>
        )}
      </div>
    </div>
  );
}

/** 关于分区（评审 E10）：自含地拉取运行环境与数据位置。 */
function AboutSection() {
  const version = useAppVersion();
  const [appInfo, setAppInfo] = useState<AppInfo | null>(null);

  useEffect(() => {
    void api.appInfo().then(setAppInfo).catch(() => setAppInfo(null));
  }, []);

  return (
    <div className="space-y-[16px]">
      <CardGroup>
        <div className="mx-auto max-w-[440px] pb-[8px] pt-[16px] text-center">
          <div className="mx-auto grid h-16 w-16 place-items-center rounded-[12px] bg-accent-500">
            <svg
              viewBox="0 0 24 24"
              className="h-8 w-8 text-accent-on"
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
          <h3 className="mt-[12px] text-[17px]/[22px] font-semibold text-white">Halo</h3>
          <p className="mt-[2px] text-[12px]/[16px] text-white/45">
            版本 {appInfo?.version || version || "—"}
          </p>
          <p className="mt-[12px] text-[13px]/[18px] text-white/60">
            一个开源、可扩展的 macOS 剪贴板管理器。
          </p>
          <p className="mt-[6px] text-[11px]/[16px] text-white/35">
            状态栏面板与菜单栏图标的几何、配色取自 Status Trio（Apache-2.0，Copyright 2026
            lingyired），已改写为 Rust + CoreGraphics 并调整了参数；完整许可见仓库根目录的
            LICENSE-STATUS-TRIO.txt 与 NOTICE-STATUS-TRIO.txt。
          </p>
        </div>

        <Divider className="my-[8px]" />
        {/* 评审 E10：关于页不再只有一句 slogan，给出运行环境与数据位置。 */}
        <Row label="架构" hint="构建目标">
          <span className="font-mono text-[12px]/[16px] text-white/60">
            {appInfo?.arch ?? "—"}
          </span>
        </Row>
        <Divider />
        <Row label="数据目录" hint={appInfo?.db_path ?? "数据库与图片缓存的存放位置"}>
          <div className="flex items-center gap-[6px]">
            <Btn
              size="sm"
              disabled={!appInfo}
              onClick={() => {
                if (!appInfo) return;
                void navigator.clipboard
                  .writeText(appInfo.data_dir)
                  .then(() => toast("已复制数据目录路径", { tone: "success" }))
                  .catch((e) => toastError(e, "复制失败"));
              }}
            >
              复制路径
            </Btn>
            <Btn
              size="sm"
              disabled={!appInfo}
              onClick={() => {
                if (!appInfo) return;
                void api.revealPath(appInfo.data_dir).catch((e) => toastError(e, "打开失败"));
              }}
            >
              在 Finder 中显示
            </Btn>
          </div>
        </Row>
        <Divider />
        <Row label="技术栈">
          <span className="text-[12px]/[16px] text-white/45">
            Tauri 2 · React 18 · Rust · SQLite(FTS5)
          </span>
        </Row>
      </CardGroup>

      <CardGroup title="隐私说明">
        <ul className="space-y-[8px] text-[12px]/[18px] text-white/60">
          <li>· 剪贴内容默认只存在本机 SQLite，不会上传到任何服务器。</li>
          <li>· 同步需你主动开启；密码与口令存在系统钥匙串，不写入配置文件。</li>
          <li>· 富文本预览经过白名单消毒，图片/图标只能读 ~/.halo 内的文件。</li>
          <li>· 回贴依赖系统「辅助功能」权限，未授权时降级为仅复制。</li>
        </ul>
        <div className="mt-[12px] flex flex-wrap gap-[6px]">
          <Btn size="sm" onClick={() => void api.openSystemSettings("accessibility")}>
            辅助功能权限
          </Btn>
          <Btn size="sm" onClick={() => void api.openSystemSettings("privacy")}>
            隐私与安全性
          </Btn>
        </div>
      </CardGroup>

      {/* 入口说明：Halo 没有 Dock 图标，用户很容易找不到「怎么关掉它」，
          这里把后台形态、唤出方式与退出入口写清楚。 */}
      <CardGroup title="后台运行">
        <ul className="space-y-[8px] text-[12px]/[18px] text-white/60">
          <li>· Halo 不占 Dock，关掉窗口只是隐藏，进程继续监听剪贴板。</li>
          <li>· 菜单栏图标点开是系统状态面板；剪贴板面板用快捷键（默认 ⌃⌥C）唤出。</li>
          <li>· 要彻底停止，点下面的「退出 Halo」，或在任一窗口聚焦时按 ⌘Q。</li>
        </ul>
        <div className="mt-[12px]">
          <Btn
            size="sm"
            variant="danger"
            onClick={() => {
              void (async () => {
                if (
                  !(await confirm("退出后不再监听剪贴板，需要时重新打开 Halo 即可。", {
                    title: "退出 Halo",
                    okLabel: "退出",
                    danger: true,
                  }))
                ) {
                  return;
                }
                // 退出会直接终止进程，promise 多半不会 resolve，所以不接错误分支。
                void api.quitApp();
              })();
            }}
          >
            退出 Halo
          </Btn>
        </div>
      </CardGroup>
    </div>
  );
}

/** 剪切板堆栈面板：锁定开关、条目预览、发送第一条/发送全部/清空。 */
function StackPanel() {
  const setError = useClipStore((s) => s.setError);
  const [locked, setLocked] = useState(false);
  const [ids, setIds] = useState<number[]>([]);
  const [previews, setPreviews] = useState<Record<number, string>>({});
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  const refresh = async () => {
    try {
      const [lockedNow, list] = await Promise.all([api.stackLocked(), api.stackList()]);
      setLocked(lockedNow);
      setIds(list);
      const entries = await Promise.all(
        list.slice(0, 5).map(async (id) => {
          try {
            const clip = await api.getClip(id);
            return [id, clip.preview || `#${id}`] as const;
          } catch {
            return [id, `#${id}`] as const;
          }
        }),
      );
      setPreviews(Object.fromEntries(entries));
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  useEffect(() => {
    void refresh();
  }, []);

  const toggleLock = async (v: boolean) => {
    try {
      await api.stackSetLock(v);
      setLocked(v);
      setMessage(v ? "堆栈已锁定，暂停接收新条目" : "堆栈已解锁，恢复接收新条目");
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  const pasteFirst = async () => {
    setBusy(true);
    try {
      const outcome = await api.stackPasteFirst();
      setMessage(
        outcome
          ? outcome.status === "pasted"
            ? "已发送第一条（栈底）"
            : outcome.reason || "已复制到剪贴板，但没能自动粘贴"
          : "堆栈为空",
      );
      await refresh();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const pasteAll = async () => {
    setBusy(true);
    try {
      const r = await api.stackPasteAll();
      // `pasted` 是真实贴上屏幕的条数：无辅助功能权限时全部计入 failed，
      // 必须把第一条原因带出来，否则用户只看到「0/3 条」不知道该怎么办。
      const firstError = r.items.find((i) => !i.ok)?.error;
      setMessage(
        r.failed === 0
          ? `已粘贴 ${r.pasted} 条`
          : `已粘贴 ${r.pasted}/${r.total} 条，${r.failed} 条失败${firstError ? `（${firstError}）` : ""}`,
      );
      await refresh();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const clear = async () => {
    setBusy(true);
    try {
      await api.stackClear();
      setMessage("堆栈已清空");
      await refresh();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const empty = ids.length === 0;

  return (
    <div className="space-y-[10px]">
      <div className="flex flex-wrap items-center justify-between gap-[12px]">
        <div className="flex items-center gap-[16px]">
          <span className="text-[13px]/[18px] text-white/85">堆栈条目 {ids.length} 条</span>
          <div className="flex items-center gap-[6px]">
            <span className="text-[12px] text-white/45">锁定</span>
            <Switch checked={locked} onChange={(v) => void toggleLock(v)} label="锁定堆栈" />
          </div>
        </div>
        <div className="flex flex-wrap gap-[6px]">
          <Btn size="sm" disabled={empty || busy} onClick={() => void pasteFirst()}>
            发送第一条
          </Btn>
          <Btn size="sm" disabled={empty || busy} onClick={() => void pasteAll()}>
            发送全部
          </Btn>
          <Btn size="sm" disabled={empty || busy} onClick={() => void clear()}>
            清空
          </Btn>
        </div>
      </div>

      {empty ? (
        <p className="text-[12px]/[16px] text-white/45">堆栈为空，可在历史列表右键「加入堆栈」。</p>
      ) : (
        <ul className="space-y-[2px]">
          {ids.slice(0, 5).map((id, i) => (
            <li key={id} className="truncate text-[12px]/[16px] text-white/60" title={previews[id]}>
              <span className="mr-[4px] text-white/40">{i + 1}.</span>
              {previews[id] ?? `#${id}`}
            </li>
          ))}
          {ids.length > 5 && <li className="text-[12px] text-white/40">…等 {ids.length} 条</li>}
        </ul>
      )}

      {message && <p className="text-[12px]/[16px] text-white/45">{message}</p>}
    </div>
  );
}

