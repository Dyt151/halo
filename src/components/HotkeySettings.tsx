/**
 * 设置中心「快捷键」分区。
 *
 * 后端一直备着 `validate_hotkey` / `apply_hotkeys`（能回答「系统到底接受了吗」），
 * 但主窗口收窄成设置中心时把录制入口弄丢了，于是配置里的 hotkeys 只能手改
 * `config.json`。这里按 macOS 的做法补回来：
 *
 * - 点行内胶囊进入录制，按下组合键即写入；只按下修饰键时给实时回显；
 * - 写入前先问后端：能解析、带修饰键、且不是系统保留组合（⌘Q/⌘Space/截屏…）；
 * - 写入后立刻 `apply_hotkeys`，把「系统是否真的接管」逐槽展示出来
 *   （注册失败最常见的原因是别的 App 已经占了同一个组合）；
 * - ⌘1~9 不是一条可编辑的键，也不是全局快捷键：它只在面板内生效，
 *   这里只提供一个开关（全局注册会抢走其它 App 的 ⌘1~9）。
 */
import { useCallback, useEffect, useState } from "react";
import { api } from "../lib/ipc";
import { toast, toastError } from "../lib/toast";
import { useClipStore } from "../store/useClipStore";
import type { HotkeyConfig } from "../lib/types";
import { Btn, CardGroup, Divider, Kbd, Notice, ToggleRow } from "./ui";

type Slot = keyof Hotkeys;

/** 只取快捷键字段本身（避免把其它 Config 字段混进 patch）。 */
type Hotkeys = HotkeyConfig;

/** 行定义：槽位 → 文案与说明。顺序即界面顺序。 */
const ROWS: { slot: Slot; label: string; hint: string }[] = [
  {
    slot: "toggle",
    label: "唤起 / 收起面板",
    hint: "全局生效。默认 ⌃⌥C：⌃C 是终端的中断信号，会抢走 Ctrl+C",
  },
  { slot: "stack_paste", label: "粘贴堆栈顶", hint: "不开面板时直接用堆栈里最新的一条" },
];

const DEFAULTS: HotkeyConfig = {
  toggle: "Control+Option+C",
  stack_paste: "Control+Shift+V",
};

/** 录制中只按了修饰键时的回显。 */
const MODIFIER_LABEL: Record<string, string> = {
  Meta: "⌘",
  Control: "⌃",
  Alt: "⌥",
  Shift: "⇧",
};

/** `Control+Shift+C` → `⌃⇧C`（只用于展示）。 */
function pretty(accelerator: string): string {
  if (!accelerator.trim()) return "未设置";
  return accelerator
    .split("+")
    .map((part) => {
      const p = part.trim().toLowerCase();
      if (p === "control" || p === "ctrl") return "⌃";
      if (p === "alt" || p === "option") return "⌥";
      if (p === "shift") return "⇧";
      if (p === "cmd" || p === "command" || p === "super" || p === "meta") return "⌘";
      if (p === "arrowup" || p === "up") return "↑";
      if (p === "arrowdown" || p === "down") return "↓";
      if (p === "arrowleft" || p === "left") return "←";
      if (p === "arrowright" || p === "right") return "→";
      if (p === "escape" || p === "esc") return "⎋";
      if (p === "space" || p === " ") return "Space";
      return part.length === 1 ? part.toUpperCase() : part;
    })
    .join("");
}

/** 修饰键别名 → 规范名：后端接受 `Ctrl`/`Option`/`Command` 等多种写法。 */
const MODIFIER_ALIAS: Record<string, string> = {
  control: "control",
  ctrl: "control",
  alt: "alt",
  option: "alt",
  shift: "shift",
  cmd: "cmd",
  command: "cmd",
  super: "cmd",
  meta: "cmd",
};

/**
 * 修饰键规范化排序键：用于「两个槽位是否撞了」的比较。
 *
 * 先把别名归一化再取交集：旧实现拿原始片段与归一化后的修饰键对比，`Ctrl+C`
 * 会同时被当成修饰键和主键，`Ctrl+C` 与 `Control+C` 就判不成同一个组合。
 */
function canonical(accelerator: string): string {
  const mods = new Set<string>();
  const keys: string[] = [];
  for (const raw of accelerator.split("+")) {
    const part = raw.trim().toLowerCase();
    if (!part) continue;
    const mod = MODIFIER_ALIAS[part];
    if (mod) mods.add(mod);
    else keys.push(part);
  }
  return [...mods].sort().concat(keys).join("+");
}

/** 把键盘事件转成加速键字符串；只按修饰键或无效键时返回 null。 */
function acceleratorOf(e: KeyboardEvent): string | null {
  const main = mainKeyOf(e);
  if (!main) return null;
  const mods: string[] = [];
  // 顺序不影响解析（后端按集合处理），但按 macOS 习惯排：⌃⌥⇧⌘。
  if (e.ctrlKey) mods.push("Control");
  if (e.altKey) mods.push("Alt");
  if (e.shiftKey) mods.push("Shift");
  if (e.metaKey) mods.push("Cmd");
  if (mods.length === 0) return null;
  return [...mods, main].join("+");
}

function mainKeyOf(e: KeyboardEvent): string | null {
  const key = e.key;
  if (key === "Control" || key === "Alt" || key === "Shift" || key === "Meta") return null;
  if (key === "Dead" || key === "Unidentified" || key === "Process") return null;
  if (key === " ") return "Space";
  if (key.startsWith("Arrow")) return key.slice(5); // ArrowUp → Up
  if (key.length === 1) return key.toUpperCase();
  return key; // Enter / Tab / Backspace / F1 / Home …
}

export default function HotkeySettings() {
  const config = useClipStore((s) => s.config);
  const patchConfig = useClipStore((s) => s.patchConfig);
  const loadConfig = useClipStore((s) => s.loadConfig);

  const [recording, setRecording] = useState<Slot | null>(null);
  /** 录制中已按下的修饰键（松开即清），给用户「我按到了」的即时反馈。 */
  const [modsHeld, setModsHeld] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  /** 最近一次 `apply_hotkeys` 的逐槽结果：只有它知道系统有没有真的接管。 */
  const [results, setResults] = useState<Record<string, { ok: boolean; message: string | null }>>({});

  const hotkeys = config?.hotkeys ?? DEFAULTS;
  const numEnabled = config?.num_hotkey_enabled ?? true;
  const stackEnabled = config?.stack_enabled ?? true;

  /** 写完配置后重新注册并收集结果。 */
  const reapply = useCallback(async () => {
    try {
      const list = await api.applyHotkeys();
      const map: Record<string, { ok: boolean; message: string | null }> = {};
      for (const r of list) map[r.slot] = { ok: r.ok, message: r.message };
      setResults(map);
      return map;
    } catch (e) {
      toastError(e, "应用快捷键失败");
      return {};
    }
  }, []);

  /**
   * 写入单个槽位（只落配置，不重新注册）。
   * 调用方负责紧接着 `reapply()`：这样可以从返回值里读出「到底注册成功没」。
   */
  const save = useCallback(
    async (patch: Partial<Hotkeys>) => {
      setBusy(true);
      try {
        await patchConfig({ hotkeys: { ...hotkeys, ...patch } });
      } catch {
        // patchConfig 已经弹过错误；这里不再叠一条。
      } finally {
        setBusy(false);
      }
    },
    [hotkeys, patchConfig],
  );

  /** 录制：捕获阶段抢在任何人之前处理，避免面板/对话框也跟着响应。 */
  useEffect(() => {
    if (!recording) return;
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();

      if (e.key === "Escape") {
        setRecording(null);
        setModsHeld([]);
        return;
      }
      if (e.key === "Backspace" && !e.metaKey && !e.ctrlKey && !e.altKey) {
        // 退回默认值比「留空」安全：空加速键会让功能彻底没有入口。
        void (async () => {
          await save({ [recording]: DEFAULTS[recording] });
          await reapply();
        })();
        setRecording(null);
        setModsHeld([]);
        return;
      }

      const held = Object.entries(MODIFIER_LABEL)
        .filter(([name]) => {
          if (name === "Meta") return e.metaKey;
          if (name === "Control") return e.ctrlKey;
          if (name === "Alt") return e.altKey;
          return e.shiftKey;
        })
        .map(([, glyph]) => glyph);
      setModsHeld(held);

      const accelerator = acceleratorOf(e);
      if (!accelerator) return; // 只按了修饰键：等主键。

      // 与其它槽位撞车：系统层面会「后者注册失败」，不如现在就拒绝并指明是谁占了。
      const clash = ROWS.find(
        (row) => row.slot !== recording && canonical(hotkeys[row.slot]) === canonical(accelerator),
      );
      if (clash) {
        toast(`已分配给「${clash.label}」，换一个组合`, { tone: "danger" });
        return;
      }

      void (async () => {
        const check = await api.validateHotkey(accelerator).catch((err) => {
          toastError(err, "无法校验快捷键");
          return { ok: false, message: null };
        });
        if (!check.ok) {
          toast(check.message ?? "这个组合不可用", { tone: "danger" });
          return;
        }
        setRecording(null);
        setModsHeld([]);
        await save({ [recording]: accelerator });
        const map = await reapply();
        if (map[recording]?.ok === false) {
          toast(map[recording]?.message ?? "系统没有接受这个组合", { tone: "danger" });
        } else {
          toast(`已设为 ${pretty(accelerator)}`, { tone: "success" });
        }
      })();
    };
    window.addEventListener("keydown", onKey, true);
    // 松开修饰键时清掉回显，避免用户以为还按着。
    const onKeyUp = () => setModsHeld([]);
    window.addEventListener("keyup", onKeyUp, true);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("keyup", onKeyUp, true);
    };
  }, [hotkeys, recording, reapply, save]);

  const resetAll = async () => {
    setBusy(true);
    try {
      await patchConfig({ hotkeys: DEFAULTS, stack_enabled: true, num_hotkey_enabled: true });
    } catch {
      // 已由 patchConfig 提示；这里直接返回，否则会紧跟着弹一条「已恢复默认」的假成功。
      return;
    } finally {
      setBusy(false);
    }
    await reapply();
    toast("已恢复默认快捷键", { tone: "info" });
  };

  const toggleNum = async (v: boolean) => {
    try {
      // ⌘1~9 不再全局注册，无需回调后端重新注册：面板下次渲染即读新开关。
      await patchConfig({ num_hotkey_enabled: v });
    } catch {
      // patchConfig 已经提示过。
    }
  };

  const toggleStack = async (v: boolean) => {
    try {
      await patchConfig({ stack_enabled: v });
      await reapply();
    } catch {
      // 已提示。
    }
  };

  const failures = ROWS.map((row) => ({ row, state: results[row.slot] })).filter(
    (x) => x.state && !x.state.ok,
  );

  return (
    <div className="space-y-[16px]">
      <CardGroup
        title="全局快捷键"
        subtitle="在任何 App 里都能用。点击右侧胶囊后直接按下新组合，Esc 取消，Backspace 恢复该项默认。"
      >
        <div className="space-y-[2px]">
          {ROWS.map((row, i) => {
            const value = hotkeys[row.slot];
            const active = recording === row.slot;
            const state = results[row.slot];
            return (
              <div key={row.slot}>
                {i > 0 && <Divider />}
                <div className="flex items-center gap-[12px] py-[9px]">
                  <div className="min-w-0 flex-1">
                    <div className="text-[13px]/[18px] text-white/85">{row.label}</div>
                    <div className="mt-[1px] text-[11px]/[15px] text-white/40">
                      {active
                        ? "请按下新的组合键…"
                        : state && !state.ok
                          ? state.message ?? "系统未接受这个组合"
                          : row.hint}
                    </div>
                  </div>
                  <button
                    type="button"
                    disabled={busy}
                    onClick={() => {
                      setRecording(active ? null : row.slot);
                      setModsHeld([]);
                    }}
                    className={`min-w-[92px] rounded-[6px] border px-[12px] py-[5px] text-[12px]/[16px] transition-colors duration-120 ${
                      active
                        ? "border-accent-500/70 bg-accent-tint text-accent-500"
                        : state && !state.ok
                          ? "border-danger/40 bg-danger/[0.10] text-danger"
                          : "border-white/[0.12] bg-white/[0.05] text-white/80 hover:bg-white/[0.10]"
                    }`}
                    title={active ? "按下新组合，Esc 取消" : "点击录制新的快捷键"}
                  >
                    {active ? (
                      <span className="tabular-nums">{modsHeld.join("") || "录制中…"}</span>
                    ) : (
                      <span className="tabular-nums">{pretty(value)}</span>
                    )}
                  </button>
                </div>
              </div>
            );
          })}
        </div>
        <div className="mt-[10px] flex items-center gap-[8px]">
          <Btn size="sm" variant="ghost" onClick={() => void resetAll()} disabled={busy}>
            恢复默认
          </Btn>
          <Btn size="sm" variant="ghost" onClick={() => void loadConfig()} disabled={busy}>
            重新读取
          </Btn>
          {busy && <span className="text-[11px] text-white/40">正在应用…</span>}
        </div>
      </CardGroup>

      <CardGroup
        title="编号快捷键"
        subtitle="面板唤出后按 ⌘1~9 直接回贴第 N 条（只在面板内生效，不占用其它 App 的快捷键）"
      >
        <ToggleRow
          label="启用 ⌘1~9"
          hint="面板内按 ⌘1~9 回贴对应序号；带筛选/搜索时序号不固定，自动停用"
          checked={numEnabled}
          onChange={(v) => void toggleNum(v)}
        />
        <Divider />
        <ToggleRow
          label="启用堆栈快捷键"
          hint="关闭后「粘贴堆栈顶」的全局组合不再注册，堆栈本身仍可用"
          checked={stackEnabled}
          onChange={(v) => void toggleStack(v)}
        />
        <div className="mt-[10px] flex flex-wrap items-center gap-[6px] text-[11px]/[15px] text-white/40">
          <span className="shrink-0">状态</span>
          <span className={numEnabled ? "text-white/55" : "text-white/35"}>
            {numEnabled ? "面板内 ⌘1~9 已启用" : "已关闭"}
          </span>
        </div>
      </CardGroup>

      {failures.length > 0 && (
        <Notice tone="danger">
          <div className="space-y-[4px]">
            <div className="font-medium">有快捷键没被系统接受</div>
            <div className="text-white/60">
              {failures.map((f) => `${f.row.label}（${pretty(hotkeys[f.row.slot])}）：${f.state?.message ?? "未知原因"}`).join("；")}
            </div>
            <div className="text-white/45">
              常见原因是该组合已被其它 App 或系统占用，换一个组合即可。
            </div>
          </div>
        </Notice>
      )}

      <CardGroup>
        <div className="flex flex-wrap items-center gap-[8px] text-[12px]/[18px] text-white/45">
          <span className="shrink-0">提示</span>
          <span>
            ⌘Q、⌘Space、⌘⇧3/4/5、⌃C 等系统保留组合会被直接拒绝；
            <span className="text-white/60">⌘W</span> 是本应用的关闭窗口，也不建议占用。
          </span>
          <Kbd>Esc</Kbd>
          <span>取消录制</span>
        </div>
      </CardGroup>
    </div>
  );
}
