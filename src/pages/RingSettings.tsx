/**
 * 设置 → 环形转换（总规范：docs/REA-Filee-Ring-Integrated.md §1.3）。
 *
 * 信息架构分四层，对应三层数据模型：
 * 1. 手势与权限 —— 只有 Option＋拖动一个入口；
 * 2. 文件类型 Profile —— 「拖什么文件用哪张盘」，顺序即匹配优先级；
 * 3. 当前 Profile 的圆环 —— 唯一一面的 preset_ids 编排；
 * 4. 可用预设 / 预设编辑器 —— 一条预设就是一份完整配方。
 *
 * 运行态（拖动手势、真实转换、进度与结果）不在这里，见 `core/ring` 与
 * `windows/RingWindow.tsx`。设置页自己仍可用鼠标与键盘编辑（§0 的说明），
 * 只是不再宣传运行态不存在的入口。
 *
 * 拖动会话统一由本组件协调：圆环视图只上报「指针落在哪个 slot」，
 * 模型变更全部在这里做，避免两个视图互相引用、也避免预览与落点各算一套几何。
 */
import { useEffect, useMemo, useRef, useState } from "react";
import RingEditor, { RING_EDITOR_CENTER, ringPointOf } from "../components/RingEditor";
import { Btn, CardGroup, Divider, Input, Row, SearchField, Segment, SliderRow, ToggleRow } from "../components/ui";
import { api } from "../lib/ipc";
import { toast } from "../lib/toast";
import {
  DRAG_THRESHOLD,
  MAX_SLICES,
  hitTest,
  withInsert,
  withMove,
  withRemove,
} from "../lib/ringGeometry";
import type { ConversionPreset, ConvertFormat, GestureStatus, RingConfig, RingProfile } from "../lib/types";
import { useClipStore } from "../store/useClipStore";

type Category = "image" | "doc";
type Filter = "all" | Category;

/** 图片格式（候选面板的「图片 / 文档」分类用）。 */
const IMAGE_TARGETS: ConvertFormat[] = ["png", "jpg", "webp", "gif", "bmp", "tiff"];

/**
 * 扩展名别名的**前端镜像**（与 Rust `ConvertFormat::from_ext` 同一张表）。
 *
 * 两边各写一份是不可避免的（前端不能调 Rust 纯函数），所以这里用「同一个
 * 校验口径」而不是「同一个字符串列表」：复合扩展名（`tar.gz`）只有在表里
 * 出现过才被接受，否则用户会存下一个永远匹配不到的字符串。
 */
const KNOWN_EXTENSIONS = new Set([
  "png",
  "jpg",
  "jpeg",
  "jpe",
  "webp",
  "gif",
  "bmp",
  "tif",
  "tiff",
  "md",
  "markdown",
  "mdown",
  "mkd",
  "html",
  "htm",
  "xhtml",
  "txt",
  "text",
]);
/** 单个扩展名最长多少字符（Rust `MAX_EXTENSION_LEN`）。 */
const MAX_EXTENSION_LEN = 16;
/** 扩展名快速填充：图片族 / 文本族。 */
const IMAGE_EXTS = ["png", "jpg", "jpeg", "webp", "gif", "bmp", "tiff"];
const TEXT_EXTS = ["md", "html", "txt"];

/** 内置 Profile 的「恢复默认」顺序（与 Rust `RingProfile::seeds` 对齐）。 */
const DEFAULT_SLICES: Record<string, string[]> = {
  images: ["img.jpg", "img.png", "img.webp", "img.gif", "img.bmp", "img.tiff"],
  text: ["doc.md", "doc.html", "doc.txt"],
  mixed: [
    "img.jpg",
    "img.png",
    "doc.md",
    "doc.html",
    "doc.txt",
    "img.webp",
    "img.gif",
    "img.bmp",
    "img.tiff",
  ],
};

/** 预设副标题：目标 + 关键参数，让同目标不同质量的预设能一眼分开。 */
function presetCaption(preset: ConversionPreset): string {
  if (preset.target === "*") return "同格式重编码";
  const label = preset.target.toUpperCase();
  if (preset.target === "jpg") return `${label} ${preset.image.jpeg_quality}% · 背景 ${preset.image.alpha_background}`;
  return preset.output.dir ? `${label} · 自定义目录` : label;
}

function presetCategory(preset: ConversionPreset): Category {
  if (preset.target === "*") return "image";
  return IMAGE_TARGETS.includes(preset.target) ? "image" : "doc";
}

/**
 * 扩展名规范化（前端镜像 Rust `core::ring::profile::normalize_extension`）。
 *
 * 返回 `null` 表示这个 token 会被后端丢掉，界面上要立刻提示，而不是等
 * 保存后发现「我写的扩展名怎么没了」。
 */
function normalizeExtension(raw: string): string | null {
  const trimmed = raw.trim().toLowerCase();
  if (trimmed.includes("..")) return null;
  let text = trimmed.replace(/^\*+\s*/, "");
  text = text.replace(/^\.+/, "").replace(/\.+$/, "");
  if (!text || text.length > MAX_EXTENSION_LEN) return null;
  if (!/^[a-z0-9._-]+$/.test(text)) return null;
  if (text.includes(".") && !KNOWN_EXTENSIONS.has(text)) return null;
  return text;
}

/** 把输入框里的内容拆成扩展名（支持逗号、顿号、空格分隔），并按序去重。 */
function parseExtensions(text: string): { valid: string[]; dropped: number } {
  const tokens = text.split(/[\s,，、;；/]+/).filter(Boolean);
  const valid: string[] = [];
  let dropped = 0;
  for (const token of tokens) {
    const ext = normalizeExtension(token);
    if (!ext) {
      dropped += 1;
      continue;
    }
    if (!valid.includes(ext)) valid.push(ext);
  }
  return { valid, dropped };
}

/** 某个 Profile 的扩展名是否被列表里更靠前的普通 Profile 完全覆盖（§2.2 末条）。 */
function shadowedBy(profiles: RingProfile[], index: number): RingProfile | null {
  const profile = profiles[index];
  if (!profile || profile.kind !== "normal" || profile.extensions.length === 0) return null;
  return (
    profiles.slice(0, index).find(
      (earlier) =>
        earlier.kind === "normal" &&
        profile.extensions.every((ext) =>
          earlier.extensions.some((own) => own.toLowerCase() === ext.toLowerCase()),
        ),
    ) ?? null
  );
}

/** 拖动会话：chip 来自候选面板，slice 来自圆环。 */
interface DragState {
  source: "chip" | "slice";
  id: string;
  /** slice 的原索引；chip 为 -1。 */
  from: number;
  label: string;
  startX: number;
  startY: number;
  x: number;
  y: number;
  /** 是否已越过阈值。 */
  active: boolean;
  /** 当前落点 slot；-1 = 不在有效位置。 */
  slot: number;
  /** 是否位于候选面板上方（slice → 移除）。 */
  overPalette: boolean;
  /** 拖动开始时的添加资格快照（拖动中刷新搜索不能改变它）。 */
  canAdd: boolean;
}

/** 只保留还存在的预设：配置里若有已删预设的残留 id，读写前就地清掉。 */
function clean(list: string[], known: ReadonlySet<string>): string[] {
  return list.filter((id) => known.has(id));
}

/** 生成不冲突的 Profile id。 */
function uniqueProfileId(profiles: RingProfile[]): string {
  const used = new Set(profiles.map((p) => p.id));
  let n = profiles.length + 1;
  let id = `profile-${n}`;
  while (used.has(id)) {
    n += 1;
    id = `profile-${n}`;
  }
  return id;
}

export default function RingSettings() {
  const config = useClipStore((s) => s.config);
  const patchConfig = useClipStore((s) => s.patchConfig);

  /** 正在编辑的 Profile（圆环编排的对象）。 */
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [editingProfile, setEditingProfile] = useState<string | null>(null);
  const [search, setSearch] = useState("");
  const [filter, setFilter] = useState<Filter>("all");
  const [drag, setDrag] = useState<DragState | null>(null);
  const [editingPreset, setEditingPreset] = useState<string | null>(null);
  const [gesture, setGesture] = useState<GestureStatus | null>(null);
  /** 权限操作的反馈（与拖动提示分开，不串到圆环预览下面）。 */
  const [axNote, setAxNote] = useState<string | null>(null);
  const [hint, setHint] = useState<string | null>(null);

  const ringRef = useRef<HTMLDivElement>(null);
  const paletteRef = useRef<HTMLDivElement>(null);
  /** 拖动回调读最新值，避免 window 监听拿到陈旧闭包。 */
  const latest = useRef<{ list: string[] }>({ list: [] });
  const dragRef = useRef<DragState | null>(null);
  const reduceMotion = usePrefersReducedMotion();

  /**
   * 轮询手势现场状态。
   *
   * 为什么不是只查一次：授权是在**另一个进程**（系统设置）里改的，App 这边
   * 只有 `AXIsProcessTrusted` 能看见。1.5s 一次的成本可以忽略，却能把
   * 「勾上了但页面还写着未授权」这类误导彻底消掉。
   */
  useEffect(() => {
    let alive = true;
    const pull = () => {
      void api
        .ringGestureStatus()
        .then((next) => {
          if (alive) setGesture(next);
        })
        .catch(() => {
          if (alive) setGesture(null);
        });
    };
    pull();
    const timer = window.setInterval(pull, 1500);
    window.addEventListener("focus", pull);
    return () => {
      alive = false;
      window.clearInterval(timer);
      window.removeEventListener("focus", pull);
    };
  }, []);

  const axOk = gesture?.accessibility ?? null;

  /** 请求辅助功能授权：系统弹窗并把 ClipBase 加进白名单。 */
  const requestAx = () => {
    void api
      .requestAccessibility()
      .then((granted) => {
        setAxNote(
          granted
            ? "已授权，手势会在 3 秒内自动接管。"
            : "系统提示已弹出：勾选 ClipBase 后本页会自动变成「已授权」，不用重启。",
        );
      })
      .catch(() => setAxNote("请求失败，请用「打开系统设置」手动添加。"));
  };

  /**
   * 清除失效的授权记录。
   *
   * ad-hoc 签名的 App 每次重新构建都会换 cdhash，系统留着一条指向旧二进制的
   * 记录：界面上开关是打开的，`AXIsProcessTrusted()` 却一直是 false。这种时候
   * 重新勾选也没用，必须先 reset 再登记（实际最常遇到的坑就是这个）。
   */
  const repairAx = () => {
    void api
      .ringGestureRepair()
      .then((r) => setAxNote(r.detail))
      .catch(() => setAxNote("修复失败：请在系统设置里先取消勾选、再重新勾选 ClipBase。"));
  };

  const ring = config?.ring ?? null;
  const presets = useMemo(() => config?.conversion?.presets ?? [], [config]);
  const presetById = useMemo(() => new Map(presets.map((p) => [p.id, p])), [presets]);
  const known = useMemo(() => new Set(presets.map((p) => p.id)), [presets]);

  /**
   * 整块 `ring` 的「已提交但还没回写」基线。
   *
   * `ring` 在配置里是**整块替换**，而回写是异步的：刚改完扩展名紧接着改
   * Option 开关，第二笔若拿本次渲染的 ring 计算，会把刚提交的扩展名整块
   * 抹回去。所有写入口都从这里取基线。
   */
  const pendingRing = useRef<RingConfig | null>(null);
  const ringBase = (): RingConfig | null => pendingRing.current ?? ring;
  const commitRing = (next: RingConfig) => {
    pendingRing.current = next;
    patchConfig({ ring: next }).catch(() => {
      // 失败时回落到服务端真值，否则本地基线会带着一笔没落盘的改动继续往下滚。
      if (pendingRing.current === next) pendingRing.current = null;
    });
  };

  const profiles = useMemo(() => ringBase()?.profiles ?? [], [ring]); // eslint-disable-line react-hooks/exhaustive-deps

  /** 选中的 Profile：明确选择 → 已存在的第一个。 */
  const selected = useMemo(() => {
    const found = profiles.find((p) => p.id === selectedId);
    return found ?? profiles[0] ?? null;
  }, [profiles, selectedId]);
  const selectedPresetIds = useMemo(
    () => (selected ? clean(selected.preset_ids, known) : []),
    [selected, known],
  );
  latest.current = { list: selectedPresetIds };

  /** 扇区/拖动 ghost 的展示名 = 预设名（JPG 带质量）。 */
  const labelOf = (id: string) => presetById.get(id)?.name ?? id;

  /* ---------------- 预设库 ---------------- */

  const pendingPresets = useRef<ConversionPreset[] | null>(null);
  const presetsBase = () => pendingPresets.current ?? presets;

  const patchPresets = (next: ConversionPreset[]) => {
    pendingPresets.current = next;
    patchConfig({ conversion: { presets: next } }).catch(() => {
      if (pendingPresets.current === next) pendingPresets.current = null;
    });
  };

  /** 改一条预设（质量 / 目录 / 冲突策略……），子对象做浅合并。 */
  const patchPreset = (id: string, update: Partial<ConversionPreset>) => {
    patchPresets(
      presetsBase().map((p) => {
        if (p.id !== id) return p;
        const next = { ...p, ...update };
        // `image` / `output` 是子对象：整块替换会把并发提交的另一个字段抹掉。
        if (update.image) next.image = { ...p.image, ...update.image };
        if (update.output) next.output = { ...p.output, ...update.output };
        return next;
      }),
    );
  };

  /* ---------------- Profile 编辑 ---------------- */

  const commitProfiles = (next: RingProfile[]) => {
    const base = ringBase();
    if (!base) return;
    commitRing({ ...base, profiles: next });
  };

  const patchProfile = (id: string, update: Partial<RingProfile>) => {
    commitProfiles(profiles.map((p) => (p.id === id ? { ...p, ...update } : p)));
  };

  const moveProfile = (index: number, delta: number) => {
    const to = index + delta;
    if (to < 0 || to >= profiles.length) return;
    const next = [...profiles];
    const [item] = next.splice(index, 1);
    next.splice(to, 0, item);
    commitProfiles(next);
  };

  const addProfile = () => {
    const id = uniqueProfileId(profiles);
    const next: RingProfile = {
      id,
      name: "新 Profile",
      // 新建先给图片族扩展名：这是最常见的「自定义一类文件」需求，
      // 用户可以在弹窗里立刻改成别的，空着反而永远匹配不到文件。
      extensions: [...IMAGE_EXTS],
      kind: "normal",
      preset_ids: [],
    };
    commitProfiles([...profiles, next]);
    setSelectedId(id);
    setEditingProfile(id);
  };

  const deleteProfile = (id: string) => {
    const target = profiles.find((p) => p.id === id);
    if (!target) return;
    if (profiles.length <= 1) {
      toast("至少要留一个 Profile", { tone: "info" });
      return;
    }
    // 兜底盘是「没配过的文件类型也有圆环」的保证；删掉它 Rust 侧也会重建一个，
    // 那样用户会看到自己刚删的东西又回来了，不如直接拦住并说明原因。
    if (target.kind === "mixed_fallback" && !profiles.some((p) => p.kind === "mixed_fallback" && p.id !== id)) {
      toast("至少要留一个混合兜底，否则没配置的文件类型就没有圆环了", { tone: "info" });
      return;
    }
    const next = profiles.filter((p) => p.id !== id);
    commitProfiles(next);
    setEditingProfile(null);
    setSelectedId(next[0]?.id ?? null);
  };

  /* ---------------- 圆环编排 ---------------- */

  const commitSlices = (ids: string[]) => {
    if (!selected) return;
    patchProfile(selected.id, { preset_ids: ids.slice(0, MAX_SLICES) });
  };

  const append = (id: string) => {
    if (selectedPresetIds.length >= MAX_SLICES) {
      setHint(`这张盘已满 ${MAX_SLICES} 个，先移除一个再添加`);
      return;
    }
    commitSlices([...selectedPresetIds, id]);
  };

  const beginDrag = (source: DragState["source"], id: string, from: number, e: React.PointerEvent) => {
    if (e.button !== 0) return;
    // 不在这里 preventDefault：那会连双击（追加）和按钮聚焦（Enter 追加）
    // 一起吞掉。拖拽位移由容器上的 select-none / 原生 dragstart 拦截。
    const canAdd = source === "chip" ? selectedPresetIds.length < MAX_SLICES : true;
    if (!canAdd) {
      setHint(`这张盘已满 ${MAX_SLICES} 个，先移除一个再添加`);
      return;
    }
    setHint(null);
    const next: DragState = {
      source,
      id,
      from,
      label: presetById.get(id)?.name ?? id,
      startX: e.clientX,
      startY: e.clientY,
      x: e.clientX,
      y: e.clientY,
      active: false,
      slot: -1,
      overPalette: false,
      canAdd,
    };
    dragRef.current = next;
    setDrag(next);
  };

  const dragging = drag !== null;

  useEffect(() => {
    if (!dragging) return;

    const onMove = (e: PointerEvent) => {
      const st = dragRef.current;
      if (!st) return;
      const list = latest.current.list;
      const moved = Math.hypot(e.clientX - st.startX, e.clientY - st.startY);
      const active = st.active || moved > DRAG_THRESHOLD;

      // 命中一律用「预览后的布局」：添加 n+1 个 slot、排序仍是 n 个（规范 §3.2）。
      let slot = -1;
      const el = ringRef.current;
      if (el) {
        const p = ringPointOf(el, e.clientX, e.clientY);
        const slots = st.source === "slice" ? list.length : list.length + 1;
        const hit = hitTest(RING_EDITOR_CENTER.cx, RING_EDITOR_CENTER.cy, p.x, p.y, slots, RING_EDITOR_CENTER.R);
        if (hit.kind === "segment") slot = hit.index;
        else if (hit.kind === "empty") slot = 0;
        // 圆心：chip 追加到末尾；slice 拖到圆心不是排序。
        else if (hit.kind === "center") slot = st.source === "chip" ? list.length : -1;
      }

      let overPalette = false;
      const pal = paletteRef.current;
      if (pal) {
        const b = pal.getBoundingClientRect();
        overPalette =
          e.clientX >= b.left && e.clientX <= b.right && e.clientY >= b.top && e.clientY <= b.bottom;
      }

      const next: DragState = { ...st, x: e.clientX, y: e.clientY, active, slot, overPalette };
      dragRef.current = next;
      setDrag(next);
    };

    const onUp = () => {
      const st = dragRef.current;
      dragRef.current = null;
      setDrag(null);
      if (!st || !st.active) return;
      const list = latest.current.list;

      if (st.source === "chip") {
        // 松在候选面板或无效区域：不变更模型（规范 §1.2）。
        if (!st.canAdd || st.overPalette || st.slot < 0) return;
        commitSlices(withInsert(list, st.id, st.slot));
        return;
      }

      if (st.overPalette) {
        commitSlices(withRemove(list, st.from));
        return;
      }
      if (st.slot >= 0 && st.slot !== st.from) {
        commitSlices(withMove(list, st.from, st.slot));
      }
    };

    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("pointercancel", onUp);
    return () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("pointercancel", onUp);
    };
    // 只在「开始/结束拖动」时重绑；拖动中的坐标走 ref。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [dragging]);

  /** 换 Profile：先终止拖动并清理预览。 */
  const selectProfile = (id: string) => {
    dragRef.current = null;
    setDrag(null);
    setSearch("");
    setHint(null);
    setSelectedId(id);
  };

  /** 扇区键盘：←/→ 排序、Delete 移除、Enter 编辑（设置页保留键盘可访问性）。 */
  const onSliceKey = (index: number, key: string) => {
    if (key === "Delete" || key === "Backspace") {
      commitSlices(withRemove(selectedPresetIds, index));
      return;
    }
    const to = key === "ArrowLeft" || key === "ArrowUp" ? index - 1 : index + 1;
    if (to < 0 || to >= selectedPresetIds.length) return;
    commitSlices(withMove(selectedPresetIds, index, to));
  };

  /* ---------------- 候选面板 ---------------- */

  const used = useMemo(() => new Set(selectedPresetIds), [selectedPresetIds]);
  const q = search.trim().toLowerCase();
  const candidates = useMemo(
    () =>
      presets.filter((p) => {
        if (used.has(p.id)) return false;
        if (filter !== "all" && presetCategory(p) !== filter) return false;
        if (!q) return true;
        return p.name.toLowerCase().includes(q) || presetCaption(p).toLowerCase().includes(q);
      }),
    [presets, used, filter, q],
  );

  /** 拖动中的预览顺序：与 drop 后模型完全一致（规范 §3.2）。 */
  const previewIds = useMemo(() => {
    if (!drag?.active) return selectedPresetIds;
    if (drag.overPalette) return selectedPresetIds;
    if (drag.slot < 0) return selectedPresetIds;
    return drag.source === "chip"
      ? withInsert(selectedPresetIds, drag.id, drag.slot)
      : withMove(selectedPresetIds, drag.from, drag.slot);
  }, [drag, selectedPresetIds]);

  if (!config || !ring || !selected) {
    return <p className="py-[32px] text-center text-[13px] text-white/45">加载中…</p>;
  }

  const removeTargeted = Boolean(drag?.active && drag.source === "slice" && drag.overPalette);
  const editingPresetData = editingPreset ? (presetById.get(editingPreset) ?? null) : null;
  const editingProfileData = editingProfile ? (profiles.find((p) => p.id === editingProfile) ?? null) : null;

  /** 复制一份预设：同目标不同质量的第二条动作就靠它做出来。 */
  const duplicatePreset = async (src: ConversionPreset) => {
    // 同一毫秒连点两次、或旧配置里已有同名 id 时不能撞车：时间戳 + 随机后缀再兜底查重。
    const base = `preset-${Date.now().toString(36)}`;
    let id = `${base}${Math.random().toString(36).slice(2, 5)}`;
    let n = 1;
    while (known.has(id)) id = `${base}-${n++}`;
    const copy: ConversionPreset = { ...src, id, name: `${src.name} 副本`, builtin: false };
    const nextPresets = [...presetsBase(), copy];
    // 先落预设库，再往圆环里插：两个 patch 若并发，先发的响应后到会把后发的那笔改回去。
    try {
      await patchConfig({ conversion: { presets: nextPresets } });
    } catch {
      return; // 错误已由 patchConfig 提示
    }
    pendingPresets.current = nextPresets;
    setEditingPreset(id);
    if (selectedPresetIds.length < MAX_SLICES) commitSlices([...selectedPresetIds, id]);
    // 盘满时不默默创建一个用不上的副本：明确告知，并指引拖拽替换。
    else toast(`「${selected.name}」已有 ${MAX_SLICES} 个动作，副本已建好，拖一个旧的出去再放它进来`, { tone: "info" });
  };

  /** 删预设：同一次提交里清掉库记录和所有 Profile 引用，不留「点了没反应」的扇区。 */
  const deletePreset = (id: string) => {
    const base = ringBase();
    if (!base) return;
    const nextPresets = presetsBase().filter((p) => p.id !== id);
    pendingPresets.current = nextPresets;
    commitRing({
      ...base,
      profiles: base.profiles.map((p) => ({
        ...p,
        preset_ids: p.preset_ids.filter((x) => x !== id),
      })),
    });
    patchConfig({ conversion: { presets: nextPresets } }).catch(() => {
      if (pendingPresets.current === nextPresets) pendingPresets.current = null;
    });
    setEditingPreset(null);
  };

  /** 预设当前出现在哪些 Profile 里（弹窗底部说明用）。 */
  const profilesOfPreset = (id: string) => profiles.filter((p) => p.preset_ids.includes(id));

  return (
    <div className="space-y-[16px]">
      <CardGroup
        title="手势与权限"
        subtitle="唯一入口：Finder 里按住 Option 拖动文件，拖到扇区上松手就换格式"
      >
        <ToggleRow
          label="Option + 拖动唤起"
          hint="按住 Option 拖动选中文件超过 8px 即弹出圆环，盘钉在起手点；只在前台是 Finder 时生效"
          checked={ring.option_tap}
          onChange={(v) => {
            const base = ringBase();
            if (base) commitRing({ ...base, option_tap: v });
          }}
        />
        <Divider />
        <Row label="辅助功能权限" hint="全局手势与回贴共用同一套授权；重建 App 后需要重新勾选一次">
          <span
            className={
              axOk ? "text-[12px]/[16px] text-emerald-300/90" : "text-[12px]/[16px] text-amber-300/90"
            }
          >
            {axOk === null ? "检测中…" : axOk ? "已授权" : "未授权：Option+拖动不会生效"}
          </span>
          {!axOk && (
            <>
              <Btn size="sm" onClick={requestAx}>
                请求授权
              </Btn>
              <Btn size="sm" onClick={repairAx}>
                修复授权
              </Btn>
              <Btn size="sm" onClick={() => void api.openSystemSettings("accessibility")}>
                打开系统设置
              </Btn>
            </>
          )}
        </Row>
        <Row label="事件监听" hint="授权后 3 秒内自动接管，不需要重启 App">
          <span
            className={
              gesture?.tapAlive
                ? "text-[12px]/[16px] text-emerald-300/90"
                : "text-[12px]/[16px] text-amber-300/90"
            }
          >
            {gesture === null ? "检测中…" : gesture.tapAlive ? "已就绪" : "未启动"}
          </span>
        </Row>
        {gesture?.lastNote && (
          <p className="mt-[4px] text-[12px]/[16px] text-amber-300/80">
            {gesture.tapAlive ? "最近一次没有响应" : "当前阻塞"}：{gesture.lastNote}
          </p>
        )}
        {axNote && <p className="mt-[4px] text-[12px]/[16px] text-white/55">{axNote}</p>}
      </CardGroup>

      <CardGroup
        title="文件类型 Profile"
        subtitle="拖什么文件用哪张盘；列表顺序就是匹配顺序，第一个「覆盖这批全部扩展名」的获胜"
      >
        <div className="divide-y divide-white/[0.06]">
          {profiles.map((profile, index) => {
            const shadow = shadowedBy(profiles, index);
            const active = profile.id === selected.id;
            const exts = profile.extensions.length === 0
              ? profile.kind === "mixed_fallback"
                ? "所有未匹配的类型（兜底）"
                : "未设置扩展名：不会匹配任何文件"
              : profile.extensions.join(" · ");
            return (
              <div
                key={profile.id}
                className={`flex items-start justify-between gap-[12px] py-[10px] ${active ? "" : "opacity-80"}`}
              >
                <button
                  type="button"
                  onClick={() => selectProfile(profile.id)}
                  className="min-w-0 flex-1 text-left"
                  aria-pressed={active}
                >
                  <div className="flex items-center gap-[6px]">
                    <span className="text-[13px]/[18px] font-medium text-white">{profile.name || profile.id}</span>
                    <span
                      className={
                        profile.kind === "mixed_fallback"
                          ? "rounded-full bg-white/[0.08] px-[6px] text-[11px]/[16px] text-white/55"
                          : "rounded-full bg-accent/15 px-[6px] text-[11px]/[16px] text-accent"
                      }
                    >
                      {profile.kind === "mixed_fallback" ? "兜底" : "普通"}
                    </span>
                    <span className="text-[11px]/[16px] text-white/35">
                      {profile.preset_ids.length} / {MAX_SLICES}
                    </span>
                    {active && <span className="text-[11px]/[16px] text-accent">· 正在编辑</span>}
                  </div>
                  <div className="mt-[2px] truncate text-[12px]/[16px] text-white/50">{exts}</div>
                  {shadow && (
                    <div className="mt-[2px] text-[11px]/[15px] text-amber-300/80">
                      被「{shadow.name}」完全覆盖：排在后面就轮不到它
                    </div>
                  )}
                </button>
                <div className="flex shrink-0 items-center gap-[6px]">
                  <Btn size="sm" disabled={index === 0} onClick={() => moveProfile(index, -1)}>
                    上移
                  </Btn>
                  <Btn size="sm" disabled={index === profiles.length - 1} onClick={() => moveProfile(index, 1)}>
                    下移
                  </Btn>
                  <Btn size="sm" onClick={() => setEditingProfile(profile.id)}>
                    编辑
                  </Btn>
                </div>
              </div>
            );
          })}
        </div>
        <div className="mt-[10px] flex items-center justify-between">
          <span className="text-[12px]/[16px] text-white/35">
            普通 Profile 优先于兜底；同族混选（png + jpg）会落在第一个覆盖它们的普通 Profile
          </span>
          <Btn size="sm" onClick={addProfile}>
            新建 Profile
          </Btn>
        </div>
      </CardGroup>

      <CardGroup
        title={`圆环编排 · ${selected.name || selected.id}`}
        subtitle="拖动扇区排序，拖到下方候选面板即移除；从候选面板拖入可插入到任意位置"
      >
        <div className="mb-[10px] flex flex-wrap items-center gap-[8px]">
          <Segment
            label="编辑哪个 Profile"
            items={profiles.map((p) => ({
              value: p.id,
              label: p.name || p.id,
              count: p.preset_ids.length,
            }))}
            value={selected.id}
            onChange={selectProfile}
          />
          <span className="ml-auto text-[12px]/[16px] text-white/40">
            {selectedPresetIds.length} / {MAX_SLICES}
          </span>
        </div>

        <RingEditor
          containerRef={ringRef}
          ids={previewIds}
          labelOf={labelOf}
          centerTitle={`${previewIds.length} / ${MAX_SLICES}`}
          centerHint={selected.name || selected.id}
          draggingId={drag?.active && drag.source === "slice" ? drag.id : null}
          removeTargeted={removeTargeted}
          emptyHint="拖入第一个动作"
          animate={!reduceMotion}
          onSlicePointerDown={(id, index, e) => beginDrag("slice", id, index, e)}
          onSliceEdit={(id) => setEditingPreset(id)}
          onSliceKey={(_, index, key) => onSliceKey(index, key)}
        />

        {hint && <p className="mt-[8px] text-center text-[12px]/[16px] text-amber-300/90">{hint}</p>}
        <div className="mt-[10px] flex items-center justify-between">
          <span className="text-[12px]/[16px] text-white/35">
            索引 0 从 12 点开始顺时针 · hover 铅笔或右键编辑 · 扇区聚焦后 ←/→ 排序、Delete 移除
          </span>
          {DEFAULT_SLICES[selected.id] && (
            <Btn size="sm" onClick={() => commitSlices(clean(DEFAULT_SLICES[selected.id], known))}>
              恢复默认
            </Btn>
          )}
        </div>
      </CardGroup>

      <CardGroup
        title="可用动作"
        subtitle="这张盘还没用到的预设；拖入圆环或双击追加到末尾"
      >
        <div ref={paletteRef} className={["rounded-[10px] transition-shadow", removeTargeted ? "ring-2 ring-danger/60" : ""].join(" ")}>
          <div className="mb-[10px] flex flex-wrap items-center gap-[8px]">
            <SearchField value={search} onChange={setSearch} placeholder="搜索动作" width="w-[200px]" />
            <Segment
              label="按类别筛选"
              items={[
                { value: "all", label: "全部" },
                { value: "image", label: "图片" },
                { value: "doc", label: "文档" },
              ]}
              value={filter}
              onChange={(v) => setFilter(v)}
            />
            <span className="ml-auto text-[12px]/[16px] text-white/40">
              {removeTargeted ? "松手即从这张盘移除" : `可添加 ${candidates.length} 个`}
            </span>
          </div>

          {candidates.length === 0 ? (
            <p className="py-[18px] text-center text-[12px]/[16px] text-white/45">
              {used.size >= presets.length
                ? "全部预设都已放上这张盘"
                : "没有匹配的动作，换个关键词或类别"}
            </p>
          ) : (
            <div className="flex flex-wrap gap-[6px]">
              {candidates.map((p) => (
                <button
                  key={p.id}
                  type="button"
                  title={`${p.name}：${presetCaption(p)}`}
                  onPointerDown={(e) => beginDrag("chip", p.id, -1, e)}
                  onDoubleClick={() => append(p.id)}
                  onContextMenu={(e) => {
                    e.preventDefault();
                    setEditingPreset(p.id);
                  }}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") {
                      e.preventDefault();
                      append(p.id);
                    } else if (e.key === "F10" && e.shiftKey) {
                      e.preventDefault();
                      setEditingPreset(p.id);
                    }
                  }}
                  className="cursor-grab rounded-full border border-white/15 bg-white/[0.06] px-[10px] py-[4px] text-[12px]/[16px] text-white/80 hover:border-accent/60 hover:text-white focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent"
                >
                  {p.name}
                  <span className="ml-[6px] text-white/35">{presetCaption(p)}</span>
                </button>
              ))}
            </div>
          )}
        </div>
      </CardGroup>

      <CardGroup
        title="转换预设"
        subtitle="每条预设都是一份完整配方：目标格式 + 质量 + 输出规则；两个 JPG 预设可以有不同质量"
      >
        <div className="divide-y divide-white/[0.06]">
          {presets.map((p) => (
            <Row key={p.id} label={p.name} hint={presetCaption(p)}>
              <div className="flex items-center gap-[6px]">
                <span className="max-w-[180px] truncate text-[12px]/[16px] text-white/45">
                  {p.output.dir || "同目录"} ·{" "}
                  {p.output.conflict === "rename"
                    ? "重名递增"
                    : p.output.conflict === "overwrite"
                      ? "覆盖输出"
                      : "重名跳过"}
                </span>
                <Btn size="sm" onClick={() => setEditingPreset(p.id)}>
                  编辑
                </Btn>
              </div>
            </Row>
          ))}
        </div>
      </CardGroup>

      <p className="text-[12px]/[16px] text-white/45">
        在 Finder 里按住 Option 拖动文件唤出圆环，把文件推到扇区上松手即执行；推到中心或环外松手取消。
        不可用的扇区会留在原位并变暗，松手在上面不会执行、只显示原因。全程不打开主窗口、不改变
        Finder 选择；转换时原地显示进度与结果，超过 5 秒自动收缩交给系统通知。
        每次转换会记住该类别的最后动作，下次只做预高亮，不会自动执行。
      </p>

      {/* 拖动跟随指针的名称 ghost（偏移 (+12,+12)）。 */}
      {drag?.active && (
        <div
          className="on-dark pointer-events-none fixed z-50 rounded-full border border-accent/70 bg-black/80 px-[10px] py-[4px] text-[12px]/[16px] text-white shadow-lg"
          style={{ left: drag.x + 12, top: drag.y + 12 }}
        >
          {drag.label}
        </div>
      )}

      {editingProfileData && (
        <ProfileDialog
          key={editingProfileData.id}
          profile={editingProfileData}
          others={profiles.filter((p) => p.id !== editingProfileData.id)}
          onClose={() => setEditingProfile(null)}
          onPatch={(update) => patchProfile(editingProfileData.id, update)}
          onDelete={() => deleteProfile(editingProfileData.id)}
        />
      )}

      {editingPresetData && (
        <PresetDialog
          // 换一条预设时整块重建：草稿（名称/背景色）天然回到新预设的值，
          // 不依赖 effect 比对 id。
          key={editingPresetData.id}
          preset={editingPresetData}
          inProfiles={profilesOfPreset(editingPresetData.id)}
          onClose={() => setEditingPreset(null)}
          onPatch={(update) => patchPreset(editingPresetData.id, update)}
          onDuplicate={() => void duplicatePreset(editingPresetData)}
          onDelete={() => deletePreset(editingPresetData.id)}
          onRemoveFrom={(profileId) => {
            const target = profiles.find((p) => p.id === profileId);
            if (!target) return;
            patchProfile(profileId, {
              preset_ids: clean(target.preset_ids, known).filter((id) => id !== editingPresetData.id),
            });
            setEditingPreset(null);
          }}
        />
      )}
    </div>
  );
}

/** `#RRGGBB` / `RRGGBB` / `#RGB`（与 Rust `parse_hex_rgb` 接受范围一致）。 */
const HEX_COLOR = /^#?([0-9a-fA-F]{3}|[0-9a-fA-F]{6})$/;

/**
 * Profile 编辑器：名称 + 匹配扩展名 + 类型（普通 / 混合兜底）。
 *
 * 扩展名用「文本输入 + 快速填充」而不是一个个 chip 输入：手写一行
 * `png, jpg, .webp` 比连点十几次快，规范化规则与后端完全一致（非法项
 * 即时丢弃并提示），不会出现「界面上有、后端存不下」的偏差。
 */
function ProfileDialog({
  profile,
  others,
  onClose,
  onPatch,
  onDelete,
}: {
  profile: RingProfile;
  others: RingProfile[];
  onClose: () => void;
  onPatch: (update: Partial<RingProfile>) => void;
  onDelete: () => void;
}) {
  const [name, setName] = useState(profile.name);
  const nameDirty = useRef(false);
  useEffect(() => {
    if (!nameDirty.current) setName(profile.name);
  }, [profile.name]);

  const [extText, setExtText] = useState(profile.extensions.join(", "));
  const extDirty = useRef(false);
  // 只在切到另一个 Profile 时重同步：跟着回写同步会在输入过程中抢走光标。
  useEffect(() => {
    if (extDirty.current) return;
    setExtText(profile.extensions.join(", "));
  }, [profile.id]); // eslint-disable-line react-hooks/exhaustive-deps

  const parsed = parseExtensions(extText);
  const missing: string[] = [];
  for (const token of extText.split(/[\s,，、;；/]+/).filter(Boolean)) {
    if (!normalizeExtension(token)) missing.push(token);
  }

  const commitName = () => {
    if (!nameDirty.current) return;
    nameDirty.current = false;
    const next = name.trim();
    if (next && next !== profile.name) onPatch({ name: next });
    else setName(profile.name);
  };

  /** 扩展名合法就即时保存（与「每次成功编辑立即提交」一致）。 */
  const commitExt = (text: string) => {
    extDirty.current = false;
    const { valid } = parseExtensions(text);
    const same =
      valid.length === profile.extensions.length &&
      valid.every((ext, i) => ext === profile.extensions[i]);
    if (!same) onPatch({ extensions: valid });
  };

  const close = () => {
    commitName();
    commitExt(extText);
    onClose();
  };
  const closeRef = useRef(close);
  closeRef.current = close;

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      e.stopPropagation();
      closeRef.current();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, []);

  const mixed = profile.kind === "mixed_fallback";
  /** 兜底盘的空扩展名 = catch-all；普通盘的空扩展名 = 永不匹配，必须说清楚。 */
  const emptyWarning = !mixed && parsed.valid.length === 0;
  const shadow = shadowedBy([...others, profile], [...others, profile].length - 1);

  return (
    <div
      className="fixed inset-0 z-50 grid place-items-center bg-black/55 p-[24px]"
      role="dialog"
      aria-modal="true"
      aria-label={`${profile.name} Profile`}
      onClick={close}
    >
      <div
        className="max-h-[86vh] w-[420px] overflow-y-auto rounded-[12px] border border-white/12 bg-[#2C2C2E] p-[16px] shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        <Input
          aria-label="Profile 名称"
          autoFocus
          value={name}
          onChange={(e) => {
            nameDirty.current = true;
            setName(e.target.value);
          }}
          onBlur={commitName}
          onKeyDown={(e) => {
            if (e.key === "Enter") (e.target as HTMLInputElement).blur();
          }}
        />
        <div className="mt-[6px] text-[12px]/[16px] text-white/45">
          {profile.id} · {profile.preset_ids.length} / {MAX_SLICES} 个动作
        </div>

        <Divider />

        <Row label="类型" hint="普通 Profile 优先匹配；混合兜底负责「没配置的类型」">
          <Segment
            items={[
              { value: "normal", label: "普通" },
              { value: "mixed_fallback", label: "混合兜底" },
            ]}
            value={profile.kind}
            onChange={(v) => onPatch({ kind: v })}
          />
        </Row>

        <div className="mt-[10px]">
          <div className="mb-[6px] text-[12px]/[16px] text-white/60">匹配扩展名</div>
          <Input
            aria-label="匹配扩展名"
            value={extText}
            placeholder={mixed ? "留空 = 兜住所有未匹配的类型" : "例如 png, jpg, jpeg, webp"}
            onChange={(e) => setExtText(e.target.value)}
            onBlur={() => commitExt(extText)}
            onKeyDown={(e) => {
              if (e.key === "Enter") (e.target as HTMLInputElement).blur();
            }}
            spellCheck={false}
          />
          <div className="mt-[6px] flex flex-wrap items-center gap-[6px]">
            <span className="text-[11px]/[15px] text-white/35">快速填充</span>
            <Btn size="sm" onClick={() => setExtText(IMAGE_EXTS.join(", "))}>
              图片族
            </Btn>
            <Btn size="sm" onClick={() => setExtText(TEXT_EXTS.join(", "))}>
              文本族
            </Btn>
            <Btn size="sm" onClick={() => setExtText("")}>
              清空
            </Btn>
          </div>
          <div className="mt-[6px] text-[11px]/[15px] text-white/40">
            已识别 {parsed.valid.length} 个：{parsed.valid.join(" · ") || "无"}
          </div>
          {missing.length > 0 && (
            <div className="mt-[4px] text-[11px]/[15px] text-amber-300/85">
              已忽略 {missing.length} 个无效项（{missing.slice(0, 4).join("、")}
              {missing.length > 4 ? "…" : ""}）：最长 {MAX_EXTENSION_LEN} 字符，只允许字母、数字与 . _ -
            </div>
          )}
          {emptyWarning && (
            <div className="mt-[4px] text-[11px]/[15px] text-amber-300/85">
              普通 Profile 没有扩展名就不会匹配任何文件；要兜住所有类型请改成「混合兜底」
            </div>
          )}
          {shadow && (
            <div className="mt-[4px] text-[11px]/[15px] text-amber-300/85">
              这些扩展名已被「{shadow.name}」完全覆盖：在当前排序下轮不到它
            </div>
          )}
        </div>

        <div className="mt-[16px] flex justify-end gap-[8px]">
          <Btn size="sm" variant="danger" onClick={onDelete}>
            删除 Profile
          </Btn>
          <Btn size="sm" variant="primary" onClick={close}>
            完成
          </Btn>
        </div>
      </div>
    </div>
  );
}

/**
 * 预设编辑器：一份配方 = 名称 + 目标 + 图像选项 + 输出规则。
 *
 * 以前这里只有一个全局 JPG 质量滑块，所以「两个不同质量的 JPG 动作」根本做不出来；
 * 现在每条预设自带质量与输出规则，复制一份就能得到第二个 JPG。
 */
function PresetDialog({
  preset,
  inProfiles,
  onClose,
  onPatch,
  onDuplicate,
  onDelete,
  onRemoveFrom,
}: {
  preset: ConversionPreset;
  /** 这条预设当前出现在哪些 Profile 里。 */
  inProfiles: RingProfile[];
  onClose: () => void;
  onPatch: (update: Partial<ConversionPreset>) => void;
  onDuplicate: () => void;
  onDelete: () => void;
  onRemoveFrom: (profileId: string) => void;
}) {
  const [name, setName] = useState(preset.name);
  /**
   * 有未提交的改名时，外部回写（自己的 patch 响应也算）不得覆盖草稿。
   *
   * 早期依赖 `[preset.id, preset.name]` 无条件同步：输入到一半时只要配置
   * 回流一次，光标就会跳到末尾（与下面背景色草稿同一个坑）。
   */
  const nameDirty = useRef(false);
  useEffect(() => {
    if (!nameDirty.current) setName(preset.name);
  }, [preset.name]);

  /** 背景色草稿：合法值即时保存，非法值不落盘（后端回退成白色会让人误以为设置没生效）。 */
  const [bg, setBg] = useState(preset.image.alpha_background);
  // 只在切到另一条预设时重同步：跟着 `preset.image.alpha_background` 同步会在
  // 每次输入合法值→回写→effect 把草稿抢回去，光标跳到最后（#abc → #abcdef 打不完）。
  useEffect(() => setBg(preset.image.alpha_background), [preset.id]);
  const bgValid = HEX_COLOR.test(bg.trim());

  const targetsJpg = preset.target === "jpg";
  const isImage = preset.target !== "*" && IMAGE_TARGETS.includes(preset.target);

  /**
   * 质量滑块：拖动时只改本地草稿，停手 300ms 才提交。
   *
   * 直接绑 `preset.image.jpeg_quality` 有两个毛病：①每挑 5% 就写一次配置；
   * ②回写比拖动慢时，滑块会被拽回上一档（40→45→50 显示成 45→50→45→50）。
   */
  const [quality, setQuality] = useState(preset.image.jpeg_quality);
  const qualityTimer = useRef<number | null>(null);
  const qualityPending = useRef<number | null>(null);

  const flushQuality = () => {
    if (qualityTimer.current !== null) {
      window.clearTimeout(qualityTimer.current);
      qualityTimer.current = null;
    }
    const v = qualityPending.current;
    qualityPending.current = null;
    if (v !== null && v !== preset.image.jpeg_quality) {
      onPatch({ image: { ...preset.image, jpeg_quality: v } });
    }
  };
  // 卸载（点「完成」/ 切到别的预设）时把最后一档写回去，否则快速关闭会丢改动。
  const flushRef = useRef(flushQuality);
  flushRef.current = flushQuality;
  useEffect(() => () => flushRef.current(), []);

  const onQuality = (v: number) => {
    setQuality(v);
    qualityPending.current = v;
    if (qualityTimer.current !== null) window.clearTimeout(qualityTimer.current);
    qualityTimer.current = window.setTimeout(() => flushRef.current(), 300);
  };

  const commitName = () => {
    if (!nameDirty.current) return;
    nameDirty.current = false;
    const next = name.trim();
    if (next && next !== preset.name) onPatch({ name: next });
    else setName(preset.name);
  };

  /**
   * 关闭前先把未提交的名称写回去。
   *
   * 只有 Esc 走这条路时，直接在遮罩上点一下就会丢掉刚输入的改名；
   * 现在遮罩、Esc、「完成」按钮都走同一个出口。
   */
  const close = () => {
    commitName();
    onClose();
  };
  const closeRef = useRef(close);
  closeRef.current = close;

  // Esc 关闭（与其余浮层一致）。只绑一次：回调走 ref 取最新闭包，
  // 否则每敲一个字符就重绑一次监听。
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      e.stopPropagation();
      closeRef.current();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, []);

  return (
    <div
      className="fixed inset-0 z-50 grid place-items-center bg-black/55 p-[24px]"
      role="dialog"
      aria-modal="true"
      aria-label={`${preset.name} 预设`}
      onClick={close}
    >
      <div
        className="max-h-[86vh] w-[400px] overflow-y-auto rounded-[12px] border border-white/12 bg-[#2C2C2E] p-[16px] shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        <Input
          aria-label="预设名称"
          autoFocus
          value={name}
          onChange={(e) => {
            nameDirty.current = true;
            setName(e.target.value);
          }}
          onBlur={commitName}
          onKeyDown={(e) => {
            if (e.key === "Enter") (e.target as HTMLInputElement).blur();
          }}
        />
        <div className="mt-[6px] text-[12px]/[16px] text-white/50">
          {presetCaption(preset)} · {preset.id}
        </div>

        {targetsJpg && (
          <div className="mt-[12px]">
            <SliderRow
              label="输出质量"
              value={quality}
              min={40}
              max={100}
              step={5}
              format={(v) => `${v}%`}
              onChange={onQuality}
            />
          </div>
        )}

        {targetsJpg && (
          <Row label="透明背景" hint="透明图转 JPG 时的合成色；不显式指定会是黑底">
            <Input
              aria-label="透明背景色"
              className={"w-[110px]" + (bgValid ? "" : " border-danger/50")}
              value={bg}
              onChange={(e) => {
                const next = e.target.value;
                setBg(next);
                if (HEX_COLOR.test(next.trim())) {
                  onPatch({ image: { ...preset.image, alpha_background: next.trim() } });
                }
              }}
              onBlur={() => {
                if (!bgValid) setBg(preset.image.alpha_background);
              }}
              onKeyDown={(e) => {
                if (e.key === "Enter") (e.target as HTMLInputElement).blur();
              }}
              spellCheck={false}
            />
            {!bgValid && (
              <span className="text-[11px]/[14px] text-danger-soft">#RRGGBB</span>
            )}
          </Row>
        )}

        {isImage && (
          <ToggleRow
            label="同格式也重编码"
            hint="源文件已经是目标格式时仍重新编码（例如 JPG 降质量）；默认跳过"
            checked={preset.reencode_same_format}
            onChange={(v) => onPatch({ reencode_same_format: v })}
          />
        )}

        <Divider />

        <Row label="输出目录" hint="留空 = 与源文件同目录；源文件永远不会被覆盖">
          <div className="flex items-center gap-[6px]">
            <Btn
              size="sm"
              onClick={() =>
                void api
                  .pickDirectory()
                  .then((dirs) => {
                    if (dirs[0]) onPatch({ output: { ...preset.output, dir: dirs[0] } });
                  })
                  .catch(() => {})
              }
            >
              选择…
            </Btn>
            <span className="max-w-[160px] truncate text-[12px]/[16px] text-white/55">
              {preset.output.dir || "同目录"}
            </span>
            {preset.output.dir && (
              <Btn size="sm" onClick={() => onPatch({ output: { ...preset.output, dir: "" } })}>
                清空
              </Btn>
            )}
          </div>
        </Row>

        <Row label="重名处理" hint="只对输出文件生效，源文件始终受保护">
          <Segment
            items={[
              { value: "rename", label: "递增" },
              { value: "overwrite", label: "覆盖" },
              { value: "skip", label: "跳过" },
            ]}
            value={preset.output.conflict}
            onChange={(v) => onPatch({ output: { ...preset.output, conflict: v } })}
          />
        </Row>

        <ToggleRow
          label="只在变小时保存"
          hint="结果比同名输出更大时丢弃，不白白占空间"
          checked={preset.output.only_if_smaller}
          onChange={(v) => onPatch({ output: { ...preset.output, only_if_smaller: v } })}
        />

        <div className="mt-[12px] text-[12px]/[16px] text-white/45">
          当前位于：
          {inProfiles.length === 0 ? "未使用" : inProfiles.map((p) => p.name || p.id).join("、")}
        </div>

        <div className="mt-[14px] flex flex-wrap justify-end gap-[8px]">
          <Btn size="sm" onClick={onDuplicate}>
            复制一份
          </Btn>
          {!preset.builtin && (
            <Btn size="sm" variant="danger" onClick={onDelete}>
              删除预设
            </Btn>
          )}
          {inProfiles.map((p) => (
            <Btn key={p.id} size="sm" variant="danger" onClick={() => onRemoveFrom(p.id)}>
              从「{p.name || p.id}」移除
            </Btn>
          ))}
          <Btn size="sm" variant="primary" onClick={close}>
            完成
          </Btn>
        </div>
      </div>
    </div>
  );
}

/** 跟随系统「减少动态效果」：开启后所有预览直接到位，功能不变。 */
function usePrefersReducedMotion(): boolean {
  const [reduce, setReduce] = useState(false);
  useEffect(() => {
    const mq = window.matchMedia("(prefers-reduced-motion: reduce)");
    setReduce(mq.matches);
    const on = () => setReduce(mq.matches);
    mq.addEventListener("change", on);
    return () => mq.removeEventListener("change", on);
  }, []);
  return reduce;
}
