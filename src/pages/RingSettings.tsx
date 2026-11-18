/**
 * 设置 → 环形转换（总规范：README「三、环形文件转换」）。
 *
 * 信息架构分四层，对应三层数据模型：
 * 1. 手势与权限 —— 只有 Option＋拖动一个入口；
 * 2. 文件类型 Profile —— 「拖什么文件用哪张盘」，顺序即匹配优先级；
 * 3. 当前 Profile 的圆环 —— 一张盘的扇区编排（打包 / 解压 / PDF 分页等
 *    工具动作由开关统一追加在末尾，不占这张表的 16 个名额）；
 * 4. 可用预设 / 预设编辑器 —— 一条预设就是一份完整配方。
 *
 * 运行态（拖动手势、真实转换、进度与结果）不在这里，见 `core/ring` 与
 * `windows/RingWindow.tsx`。设置页自己仍可用鼠标与键盘编辑（Filee 行为 的说明），
 * 只是不再宣传运行态不存在的入口。
 *
 * 拖动会话统一由本组件协调：圆环视图只上报「指针落在哪个 slot」，
 * 模型变更全部在这里做，避免两个视图互相引用、也避免预览与落点各算一套几何。
 */
import { useEffect, useMemo, useRef, useState } from "react";
import RingEditor, { RING_EDITOR_CENTER, ringPointOf } from "../components/RingEditor";
import { ProfileDialog } from "./ring/ProfileDialog";
import { PresetDialog } from "./ring/PresetDialog";
import { usePrefersReducedMotion } from "../lib/usePrefersReducedMotion";
import {
  DEFAULT_SLICES,
  IMAGE_EXTS,
  clean,
  presetCaption,
  presetCategory,
  shadowedBy,
  uniqueProfileId,
} from "./ring/model";
import type { Filter } from "./ring/model";
import { Btn, CardGroup, Row, SearchField, Segment, ToggleRow } from "../components/ui";
import { toast } from "../lib/toast";
import {
  DRAG_THRESHOLD,
  MAX_SLICES,
  hitTest,
  withInsert,
  withMove,
  withRemove,
} from "../lib/ringGeometry";
import type { ConversionPreset, RingConfig, RingProfile } from "../lib/types";
import { useClipStore } from "../store/useClipStore";

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
  const [hint, setHint] = useState<string | null>(null);

  const ringRef = useRef<HTMLDivElement>(null);
  const paletteRef = useRef<HTMLDivElement>(null);
  /** 拖动回调读最新值，避免 window 监听拿到陈旧闭包。 */
  const latest = useRef<{ list: string[] }>({ list: [] });
  const dragRef = useRef<DragState | null>(null);
  const reduceMotion = usePrefersReducedMotion();

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
  /** 当前盘的扇区顺序，去掉已删预设。 */
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

      // 命中一律用「预览后的布局」：添加 n+1 个 slot、排序仍是 n 个（Filee 行为）。
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
        // 松在候选面板或无效区域：不变更模型（Filee 行为）。
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

  // 同一张盘里一个预设只能出现一次（Rust 归一化也会去重）。
  const used = useMemo(() => new Set(selected?.preset_ids ?? []), [selected]);
  const utilityOn = ringBase()?.utility_actions ?? true;
  const q = search.trim().toLowerCase();
  const candidates = useMemo(
    () =>
      presets.filter((p) => {
        if (used.has(p.id)) return false;
        // 开关打开时工具动作已经固定追加在末尾，再手动放一份就会重复。
        if (utilityOn && presetCategory(p) === "tool") return false;
        if (filter !== "all" && presetCategory(p) !== filter) return false;
        if (!q) return true;
        return p.name.toLowerCase().includes(q) || presetCaption(p).toLowerCase().includes(q);
      }),
    [presets, used, utilityOn, filter, q],
  );

  /** 拖动中的预览顺序：与 drop 后模型完全一致（Filee 行为）。 */
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
      <CardGroup title="手势">
        <ToggleRow
          label="Option + 拖动唤起"
          checked={ring.option_tap}
          onChange={(v) => {
            const base = ringBase();
            if (base) commitRing({ ...base, option_tap: v });
          }}
        />
      </CardGroup>

      <CardGroup title="文件类型 Profile">
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
                      {profile.preset_ids.length} 个动作
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
                  <Btn
                    size="sm"
                    onClick={() => {
                      // 编辑哪张盘就让圆环预览哪张盘，否则关闭弹窗后看到的还是上一张。
                      selectProfile(profile.id);
                      setEditingProfile(profile.id);
                    }}
                  >
                    编辑
                  </Btn>
                </div>
              </div>
            );
          })}
        </div>
        <div className="mt-[10px] flex items-center justify-between">
          <span />
          <Btn size="sm" onClick={addProfile}>
            新建 Profile
          </Btn>
        </div>
      </CardGroup>

      <CardGroup title={`圆环编排 · ${selected.name || selected.id}`}>
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
            每张盘最多 {MAX_SLICES} 个动作
          </span>
        </div>

        <ToggleRow
          label="末尾追加工具动作"
          checked={ring.utility_actions}
          onChange={(v) => {
            const base = ringBase();
            if (base) commitRing({ ...base, utility_actions: v });
          }}
        />

        <RingEditor
          containerRef={ringRef}
          ids={previewIds}
          labelOf={labelOf}
          centerTitle={`${previewIds.length} 个动作`}
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
        <div className="mt-[10px] flex flex-wrap items-center justify-between gap-[8px]">
          <span />
          {DEFAULT_SLICES[selected.id] && (
            <Btn
              size="sm"
              className="shrink-0 whitespace-nowrap"
              onClick={() => commitSlices(clean(DEFAULT_SLICES[selected.id], known))}
            >
              恢复默认
            </Btn>
          )}
        </div>
      </CardGroup>

      <CardGroup title="可用动作">
        <div ref={paletteRef} className={["rounded-[10px] transition-shadow", removeTargeted ? "ring-2 ring-danger/60" : ""].join(" ")}>
          <div className="mb-[10px] flex flex-wrap items-center gap-[8px]">
            <SearchField value={search} onChange={setSearch} placeholder="搜索动作" width="w-[200px]" />
            <Segment
              label="按类别筛选"
              items={[
                { value: "all", label: "全部" },
                { value: "image", label: "图片" },
                { value: "doc", label: "文档" },
                { value: "tool", label: "工具" },
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

      <CardGroup title="转换预设">
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
        在 Finder 里按住 Option 拖动文件唤出圆环，把文件推到扇区上松手即执行；推到中心或环外松手取消，
        拖动中按 Esc 取消。图片／文本／文档各有专属盘，没配置过的类型落到「其他文件」兜底盘。
        不可用的扇区会留在原位并变暗，松手在上面不会执行、只显示原因。全程不打开主窗口、
        不改变 Finder 选择；转换时原地显示进度与结果，超过 5 秒自动收缩交给系统通知。
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

