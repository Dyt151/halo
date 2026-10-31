/**
 * 设置页圆环编辑器（`docs/REA-Filee-Ring-Integrated.md` §4「圆环规则」）。
 *
 * 受控视图：只画当前快照、上报指针/键盘意图，不读转换引擎、不写磁盘、
 * 不自己算第二套几何——绘制与命中都用 `lib/ringGeometry`（与运行态环形盘
 * 同一份代码），因此「预览落点」与「drop 后模型」天然一致。
 *
 * 拖动会话（跨视图的 chip → 环、环 → 候选面板）由宿主 `RingSettings` 统一
 * 协调（规范 §4.2：两个视图不互相引用）。
 */
import { useState } from "react";
import {
  RING_GEOM,
  RING_SIDE,
  innerRadius,
  labelAnchor,
  outwardUnit,
  sectorPath,
} from "../lib/ringGeometry";

/** 画布边长（SVG 逻辑坐标，与运行态环形盘一致）。 */
const SIDE = RING_SIDE;
const C = SIDE / 2;
const R = RING_GEOM.R;
const r = innerRadius(R);

/** 铅笔按钮直径：命中半径 13px（Editor §1.5），所以直径 26px。 */
const PENCIL_PX = 26;

/**
 * 扇区多到这个数之后标签改为沿半径排布（与运行态环形盘同一阈值）。
 *
 * 16 片时一段弧长只有 ~42px，「打包 TAR.GZ」这种名字切向排布必然压到
 * 隔壁；沿半径有 R−r ≈ 87px，放得下。
 */
const RADIAL_LABELS_FROM = 13;

export interface RingEditorProps {
  /** 当前 Profile 的预设 id（顺时针，索引 0 在 12 点）。 */
  ids: string[];
  labelOf: (id: string) => string;
  /** 圆心主标题（动作数量）。 */
  centerTitle: string;
  /** 圆心副标题（Profile 名 / 类型）。 */
  centerHint: string;
  /** 正在拖动的扇区 id：画成半透明临时扇区。 */
  draggingId?: string | null;
  /** 拖动经过候选面板：整环进入「松手即移除」提示。 */
  removeTargeted?: boolean;
  /** 空环提示。 */
  emptyHint?: string;
  /** 是否播放过渡（减少动态效果时关掉）。 */
  animate?: boolean;
  /** 宿主命中换算需要圆环的屏幕矩形。 */
  containerRef?: React.RefObject<HTMLDivElement>;
  onSlicePointerDown?: (id: string, index: number, e: React.PointerEvent) => void;
  onSliceEdit?: (id: string) => void;
  onSliceKey?: (id: string, index: number, key: string) => void;
}

export default function RingEditor({
  ids,
  labelOf,
  centerTitle,
  centerHint,
  draggingId,
  removeTargeted = false,
  emptyHint = "拖动预设到这里",
  animate = true,
  containerRef,
  onSlicePointerDown,
  onSliceEdit,
  onSliceKey,
}: RingEditorProps) {
  const [hover, setHover] = useState(-1);
  const n = ids.length;
  const pct = (v: number) => `${(v / SIDE) * 100}%`;
  const ease = animate ? "150ms" : "0ms";

  return (
    <div
      ref={containerRef}
      className="relative mx-auto select-none"
      style={{ width: 320, height: 320, touchAction: "none" }}
      onDragStart={(e) => e.preventDefault()}
      onPointerLeave={() => setHover(-1)}
    >
      <svg viewBox={`0 0 ${SIDE} ${SIDE}`} className="h-full w-full overflow-visible">
        {/* 空环：保留完整圆形落点区域（规范 §3.3），用内外两条虚线暗示可放置 */}
        {n === 0 && (
          <>
            <circle
              cx={C}
              cy={C}
              r={R}
              fill="none"
              stroke="currentColor"
              strokeDasharray="5 6"
              strokeWidth={1.5}
              className="text-white/20"
            />
            <circle
              cx={C}
              cy={C}
              r={r}
              fill="none"
              stroke="currentColor"
              strokeDasharray="5 6"
              strokeWidth={1.5}
              className="text-white/20"
            />
          </>
        )}

        {ids.map((id, i) => {
          const isDragging = draggingId === id;
          const isHover = hover === i && !draggingId;
          const u = outwardUnit(i, n);
          return (
            <path
              key={id}
              d={sectorPath(C, C, R, r, i, n)}
              role="button"
              tabIndex={0}
              aria-label={`${labelOf(id)}，第 ${i + 1} 个，共 ${n} 个`}
              className="ring-sector cursor-grab outline-none focus-visible:stroke-accent"
              data-hover={isHover || undefined}
              data-drag={isDragging || undefined}
              strokeWidth={1}
              style={{
                transform: isHover ? `translate(${u.x * 4}px, ${u.y * 4}px)` : undefined,
                transition: `transform ${ease}, fill ${ease}`,
              }}
              onPointerEnter={() => setHover(i)}
              onPointerDown={(e) => onSlicePointerDown?.(id, i, e)}
              onContextMenu={(e) => {
                // 右键扇区 = 编辑该预设，与铅笔同一入口（Editor §1.5）。
                e.preventDefault();
                onSliceEdit?.(id);
              }}
              onKeyDown={(e) => {
                if (e.key === "Enter" || e.key === " ") {
                  e.preventDefault();
                  onSliceEdit?.(id);
                  return;
                }
                if (["ArrowLeft", "ArrowUp", "ArrowRight", "ArrowDown", "Delete", "Backspace"].includes(e.key)) {
                  e.preventDefault();
                  onSliceKey?.(id, i, e.key);
                }
              }}
            />
          );
        })}
      </svg>

      {/* 标签与铅笔按钮：HTML 覆盖层，避免 SVG 文字在 Retina 下模糊。 */}
      {ids.map((id, i) => {
        const p = labelAnchor(C, C, R, r, i, n);
        const isHover = hover === i && !draggingId;
        const isDragging = draggingId === id;
        // 片数多时标签沿半径排布：外层绕圆心转到本扇区角度，内层再把文字转
        // 90°；左半圈反过来，保证任何位置都不会倒着读。
        const radial = n >= RADIAL_LABELS_FROM;
        const deg = (i * 360) / n;
        const flip = deg > 90 && deg < 270;
        return (
          <div
            key={id}
            className="pointer-events-none absolute -translate-x-1/2 -translate-y-1/2"
            style={{
              left: pct(p.x),
              top: pct(p.y),
              transform: `translate(-50%, -50%)${radial ? ` rotate(${deg.toFixed(2)}deg)` : ""}`,
            }}
          >
            {isHover ? (
              <button
                type="button"
                title={`编辑 ${labelOf(id)}`}
                aria-label={`编辑 ${labelOf(id)}`}
                onClick={() => onSliceEdit?.(id)}
                className="on-dark pointer-events-auto grid place-items-center rounded-full border border-white/25 bg-black/55 text-white/85 hover:border-accent hover:text-accent"
                style={{ width: PENCIL_PX, height: PENCIL_PX }}
              >
                <svg viewBox="0 0 24 24" className="h-[12px] w-[12px]" fill="none" stroke="currentColor" strokeWidth="2">
                  <path d="M4 20h4L20 8l-4-4L4 16v4Z" />
                </svg>
              </button>
            ) : (
              <span
                className={`block ${radial ? "max-w-[88px]" : "max-w-[86px]"} truncate text-center font-medium ${radial ? "text-[10px]/[13px]" : "text-[11px]/[14px]"}`}
                style={{
                  transform: radial ? `rotate(${flip ? 90 : -90}deg)` : undefined,
                  color: isDragging ? "var(--ring-label-active)" : "var(--ring-label)",
                  opacity: isDragging ? 0.7 : 1,
                  transition: `color ${ease}, opacity ${ease}`,
                }}
              >
                {labelOf(id)}
              </span>
            )}
          </div>
        );
      })}

      {/* 圆心：数量 / Profile 名，拖到圆心 = 追加（命中由宿主判定）。 */}
      <div className="pointer-events-none absolute inset-0 grid place-items-center">
        <div className="text-center" style={{ width: r * 1.6 }}>
          <div className="text-[13px]/[17px] font-semibold text-white">{centerTitle}</div>
          <div className="mt-[1px] text-[11px]/[14px] text-white/45">{centerHint}</div>
        </div>
      </div>

      {/* 空环提示：只在完全为空时出现，不挡命中。 */}
      {n === 0 && (
        <div className="pointer-events-none absolute inset-0 grid place-items-center">
          <span className="mt-[46px] text-[11px]/[14px] text-white/35">{emptyHint}</span>
        </div>
      )}

      {/* 移除提示：拖动经过候选面板时整环变红，表示「松手即移除」。 */}
      {removeTargeted && (
        <div className="pointer-events-none absolute inset-0 grid place-items-center">
          <div className="rounded-full border border-danger/60 px-[14px] py-[6px] text-[12px] text-danger-soft">
            松手移除
          </div>
        </div>
      )}
    </div>
  );
}

/** 把屏幕坐标换算成圆环逻辑坐标（宿主命中测试用）。 */
export function ringPointOf(el: HTMLElement, clientX: number, clientY: number): { x: number; y: number } {
  const box = el.getBoundingClientRect();
  const scale = SIDE / box.width;
  return { x: (clientX - box.left) * scale, y: (clientY - box.top) * scale };
}

/** 圆环中心与半径（宿主可直接拿去 hitTest）。 */
export const RING_EDITOR_CENTER = { cx: C, cy: C, R, r };
