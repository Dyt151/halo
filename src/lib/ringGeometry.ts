/**
 * 环形几何（移植自 Filee `DonutGeometry`，见 docs/REA-Filee-Ring-Integrated.md §4）。
 *
 * 约定与 Filee 一致：**从 12 点开始、顺时针增加**的弧度；半径/孔径比例可调；
 * 每个扇区两侧按「固定 3px 像素间隙」校正角度，而不是固定角间隙；
 * 命中测试优先判中心/圆环外，其余按角度取扇区——圆角切掉的角落仍算原扇区。
 *
 * 纯函数、无副作用，便于在 RingWindow 与单元测试里复用。
 */

export interface Pt {
  x: number;
  y: number;
}

export type RingHit =
  | { kind: "center" }
  | { kind: "outside" }
  | { kind: "empty" }
  | { kind: "segment"; index: number };

/** 与窗口尺寸匹配的外半径 + 孔径比例（Filee 默认 R=150 / HoleRatio=0.42）。 */
export const RING_GEOM = {
  /** 外半径。 */
  R: 150,
  /** 孔径比例：内半径 = R × clamp(holeRatio, 0.2, 0.75)。 */
  holeRatio: 0.42,
  /** 相邻扇区之间的固定像素间隙。 */
  gapPixels: 3,
  /** hover 时向外弹出的最大像素，命中测试外半径额外放宽这么多。 */
  popOut: 7,
};

/** 每个扇区的角宽：N≤0 时整圆，否则均分。 */
export function spanFor(n: number): number {
  return n <= 0 ? Math.PI * 2 : (Math.PI * 2) / n;
}

/** 内半径。 */
export function innerRadius(R: number = RING_GEOM.R): number {
  return R * Math.min(0.75, Math.max(0.2, RING_GEOM.holeRatio));
}

/** 第 i 个扇区的中心角（弧度，12 点起顺时针）。 */
export function midAngle(i: number, n: number): number {
  return i * spanFor(n);
}

/** 极坐标 → 屏幕坐标（y 轴向下，故用 -cos）。 */
export function pointAt(cx: number, cy: number, radius: number, angle: number): Pt {
  return { x: cx + radius * Math.sin(angle), y: cy - radius * Math.cos(angle) };
}

/** 屏幕坐标 → 角度（12 点起顺时针，范围 [0, 2π)）。 */
export function angleOf(cx: number, cy: number, x: number, y: number): number {
  let a = Math.atan2(x - cx, cy - y);
  if (a < 0) a += Math.PI * 2;
  return a;
}

/** 角度 → 扇区索引（+span/2 因为中间角表示扇区中心）。 */
export function indexAt(angle: number, n: number): number {
  if (n <= 0) return 0;
  const span = spanFor(n);
  return Math.floor((angle + span / 2) / span) % n;
}

/**
 * 命中测试：中心孔 → 圆环外 → 空环 → 扇区。
 * 不使用 Path.contains，圆角切掉的角仍是原扇区（对齐 Filee）。
 */
export function hitTest(
  cx: number,
  cy: number,
  x: number,
  y: number,
  n: number,
  R: number = RING_GEOM.R,
): RingHit {
  const r = innerRadius(R);
  const dist = Math.hypot(x - cx, y - cy);
  if (dist < r) return { kind: "center" };
  if (dist > R + RING_GEOM.popOut) return { kind: "outside" };
  if (n === 0) return { kind: "empty" };
  return { kind: "segment", index: indexAt(angleOf(cx, cy, x, y), n) };
}

/**
 * 生成单个环扇区的 SVG path（外弧顺时针 + 内弧逆时针闭合）。
 * 两侧按固定像素间隙校正，保证 N 变化时视觉缝隙恒定。
 *
 * `n === 1` 时直接画整圆：只剩一个目标时那条 3px 缝没有任何信息量，
 * 反而像是「还差一块」（Wheel §2.2：不画无意义的单扇区分割）。
 */
export function sectorPath(
  cx: number,
  cy: number,
  R: number,
  r: number,
  i: number,
  n: number,
): string {
  if (n === 1) {
    // 外圈顺时针、内圈逆时针：nonzero 填充规则下自然挖出中心的孔。
    return [
      `M ${cx.toFixed(2)} ${(cy - R).toFixed(2)}`,
      `A ${R} ${R} 0 1 1 ${cx.toFixed(2)} ${(cy + R).toFixed(2)}`,
      `A ${R} ${R} 0 1 1 ${cx.toFixed(2)} ${(cy - R).toFixed(2)}`,
      "Z",
      `M ${cx.toFixed(2)} ${(cy - r).toFixed(2)}`,
      `A ${r} ${r} 0 1 0 ${cx.toFixed(2)} ${(cy + r).toFixed(2)}`,
      `A ${r} ${r} 0 1 0 ${cx.toFixed(2)} ${(cy - r).toFixed(2)}`,
      "Z",
    ].join(" ");
  }
  const span = spanFor(n);
  const mid = i * span;
  const outerHalf = Math.max(0.001, span / 2 - RING_GEOM.gapPixels / (2 * R));
  const innerHalf = Math.max(0.001, span / 2 - RING_GEOM.gapPixels / (2 * r));

  const os = pointAt(cx, cy, R, mid - outerHalf);
  const oe = pointAt(cx, cy, R, mid + outerHalf);
  const is = pointAt(cx, cy, r, mid - innerHalf);
  const ie = pointAt(cx, cy, r, mid + innerHalf);
  const large = outerHalf * 2 > Math.PI ? 1 : 0;

  return [
    `M ${os.x.toFixed(2)} ${os.y.toFixed(2)}`,
    `A ${R} ${R} 0 ${large} 1 ${oe.x.toFixed(2)} ${oe.y.toFixed(2)}`,
    `L ${ie.x.toFixed(2)} ${ie.y.toFixed(2)}`,
    `A ${r} ${r} 0 ${large} 0 ${is.x.toFixed(2)} ${is.y.toFixed(2)}`,
    "Z",
  ].join(" ");
}

/** 扇区标签锚点（外半径与内半径中线处）。 */
export function labelAnchor(
  cx: number,
  cy: number,
  R: number,
  r: number,
  i: number,
  n: number,
): Pt {
  return pointAt(cx, cy, (R + r) / 2, midAngle(i, n));
}

/** 第 i 个扇区向外的单位向量（用于 hover 弹出位移）。 */
export function outwardUnit(i: number, n: number): Pt {
  const a = midAngle(i, n);
  return { x: Math.sin(a), y: -Math.cos(a) };
}

/**
 * 窗口边长：`2 × (R + PopOut + 14)`（文档 §2.2 `SideFor`，无子环）。
 * 留出的 14px 给阴影，7px 给 hover 弹出——窗口小于它会把 hover 位移和
 * 外发光裁掉。
 */
export const RING_SIDE = 2 * (RING_GEOM.R + RING_GEOM.popOut + 14);

/* ------------------------------------------------------------------ */
/* 设置页圆环编辑器的顺序运算（docs/REA-Filee-Ring-Integrated.md §2.3）   */
/* ------------------------------------------------------------------ */

/** 每个 Profile 最多几个扇区（Filee `MaxSlices`，与 Rust `MAX_PROFILE_SLICES` 同值）。 */
export const MAX_SLICES = 12;
/** 拖动阈值：超过 5px 才算拖动，否则当点击（规范 §1.2）。 */
export const DRAG_THRESHOLD = 5;

function clamp(n: number, lo: number, hi: number): number {
  return Math.min(hi, Math.max(lo, n));
}

/**
 * 添加预览：把 id 插到 slot（slot ∈ 0..n）。
 * 预览时 slot 数按 `count + 1` 计算，所以命中测试也要用 n + 1 个扇区，
 * 否则缝隙一打开指针就会「跳」到隔壁 slot（规范 §3.2）。
 */
export function withInsert(ids: string[], id: string, slot: number): string[] {
  const at = clamp(slot, 0, ids.length);
  return [...ids.slice(0, at), id, ...ids.slice(at)];
}

/**
 * 排序预览：先排除源项，再插到 to（to ∈ 0..n-1）。
 * 等价于 Filee「源项之后先减 1、插入点及其后再移 1」，因此预览仍是 n 个扇区。
 */
export function withMove(ids: string[], from: number, to: number): string[] {
  if (from < 0 || from >= ids.length) return ids;
  const at = clamp(to, 0, ids.length - 1);
  if (at === from) return ids;
  const rest = ids.filter((_, i) => i !== from);
  return [...rest.slice(0, at), ids[from], ...rest.slice(at)];
}

/** 移除某一项。 */
export function withRemove(ids: string[], at: number): string[] {
  return ids.filter((_, i) => i !== at);
}
