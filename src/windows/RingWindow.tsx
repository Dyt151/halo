/**
 * Finder 环形转换盘（总规范：docs/REA-Filee-Ring-Integrated.md）。
 *
 * 只有一面、只有一个手势：
 * - 环形盘由 Option＋左键拖动唤起，右半区只有**一面**（§4）：没有正/背面，
 *   没有 Tab/滚轮/自动换面，也没有翻面提示。
 * - 运行态不接受点击、Enter、方向键、Esc、滚轮（§3.1）：窗口在拖动期间
 *   `ignore_cursor_events`，前端只从 `ring://pointer` 拿指针坐标、从
 *   `ring://drag-end` 拿松手 —— 这是一次拖动内完成的唯一通道。
 * - 不可用的扇区**保留原位**（§2.3）：不重排、不消失，只降低不透明度；
 *   hover 时中心显示原因，松手落在上面不执行，把原因留 1.3s 再收起。
 *
 * 几何沿用 Filee `DonutGeometry`（12 点起顺时针、3px 间隙、径向命中、
 * hover 径向弹出 + 2.5% 放大），动画只保留「打开」与「点选收缩」。
 */
import { useEffect, useCallback, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "../lib/ipc";
import type { RingAction, RingProgress, RingResult, RingSnapshot } from "../lib/types";
import {
  RING_GEOM,
  RING_SIDE,
  hitTest,
  innerRadius,
  labelAnchor,
  midAngle,
  outwardUnit,
  sectorPath,
} from "../lib/ringGeometry";
import { initialIndex, resolveRingDragEnd, ringCenterTitle } from "../lib/ringState";

/** 与几何模块一致：窗口边长 = 2 × (R + 弹出预留 + 阴影预留)。 */
const SIZE = RING_SIDE;
const C = SIZE / 2;
const R = RING_GEOM.R;
const r = innerRadius(R);
const POP = RING_GEOM.popOut;
/** hover 时扇区额外放大 2.5%（Wheel §1.3）。 */
const HOVER_SCALE = 1.025;
/** 打开动画逐项延迟 0.028s。 */
const OPEN_STAGGER = 28;
/** 关闭动画逐项延迟 0.01s；总时长 = 160ms + 12 项 ≈ 270ms（Wheel §1.3）。 */
const CLOSE_STAGGER = 10;
const CLOSE_MS = 300;
/** 落在禁用扇区上时，原因停留多久再收起。 */
const REJECT_HOLD = 1300;
/**
 * 入场动画最多演这么久，之后无条件摘掉动画类。
 *
 * 动画只是观感，不能挡住正确性：隐藏中的 WKWebView 会把 `opacity:0` 的入场帧
 * 冻住，窗口显示后也可能不恢复，结果就是「环看不见、但松手仍按坐标命中扇区」。
 */
const STILL_AFTER_MS = 600;
/** 指针位移小于它就不算「真的在拖动」（与宿主接管阈值一致，防误触）。 */
const ARM_DISTANCE = 6;
/**
 * 扇区多到这个数之后，标签改为沿半径方向排布。
 *
 * 切向排布的字宽上限就是一段弧长（21 片时只有 ~30px），「打包 TAR.GZ」
 * 这种名字必然压到隔壁；沿半径有 R−r ≈ 87px，足够放下而不打架。
 */
const RADIAL_LABELS_FROM = 13;
/** 沿半径排布时的字号（片数越多越小）。 */
function radialFontSize(n: number): number {
  return n > 18 ? 9 : 10;
}

const EV_UPDATE = "ring://update";
const EV_PROGRESS = "ring://progress";
const EV_RESULT = "ring://result";
const EV_HANDOFF = "ring://handoff";
const EV_SHOWN = "halo://ring-shown";
/** 拖动中的指针位置与松手。 */
const EV_POINTER = "ring://pointer";
const EV_DRAG_END = "ring://drag-end";

const ACCENT = "#e8a33d";
/** 高亮填充最高 0.82 不透明度（Wheel §1.3：不盖住底下的阴影与毛玻璃）。 */
const ACCENT_FILL = "rgba(232,163,61,0.82)";
const ACCENT_ON = "#1b1408";
const SUCCESS = "#34d399";
const DANGER = "#f87171";
/** 禁用扇区：0.55 倍扇区不透明度，看得出「有这一项」但按不下去（Wheel §1.3）。 */
const MUTED_FILL = "rgba(18,18,20,0.45)";
const MUTED_TEXT = "rgba(255,255,255,0.42)";
const REJECT_FILL = "rgba(248,113,113,0.16)";

const PENDING = new Set(["idle", "capturing", "probing"]);

const EMPTY_SNAPSHOT: RingSnapshot = {
  generation: 0,
  state: "idle",
  category: null,
  count: 0,
  hint: null,
  profile: null,
  actions: [],
  suggested: null,
};

/** 清掉一个挂着的一次性定时器（ref 里同时置空，避免重复 clear）。 */
function clearTimer(ref: React.MutableRefObject<number | null>) {
  if (ref.current !== null) {
    window.clearTimeout(ref.current);
    ref.current = null;
  }
}

export default function RingWindow() {
  const [snap, setSnap] = useState<RingSnapshot>(EMPTY_SNAPSHOT);
  const [active, setActive] = useState(0);
  const [centerHover, setCenterHover] = useState(false);
  /** 指针在环外（NoDrop）：松手同样是取消，中心要如实说（§3.3）。 */
  const [noDrop, setNoDrop] = useState(false);
  /** 变一次就重播扇区动画（新会话）。 */
  const [animKey, setAnimKey] = useState(0);
  const [progress, setProgress] = useState<RingProgress | null>(null);
  const [result, setResult] = useState<RingResult | null>(null);
  /** 5s 交接：收缩淡出后交给系统通知。 */
  const [handoff, setHandoff] = useState(false);
  /** 提交后扇区向中心收缩（0.28s），收缩完才出现进度环。 */
  const [collapsing, setCollapsing] = useState(false);
  /** 松手落在禁用扇区上的原因（临时占住中心）。 */
  const [rejected, setRejected] = useState<string | null>(null);
  /** 没有命中可用扇区：先播关闭动画再关窗，避免“凭空消失”（Wheel §1.3）。 */
  const [closing, setClosing] = useState(false);
  /** 入场动画已放完（或页面不可见时直接置 true）：渲染不再挂动画类。 */
  const [still, setStill] = useState(true);

  const rootRef = useRef<HTMLDivElement | null>(null);
  /** 入场动画容器：兜底逻辑要在定时器里读它的计算样式。 */
  const layerRef = useRef<HTMLDivElement | null>(null);
  const resultSeenRef = useRef<number | null>(null);
  /**
   * 会话令牌：每应用一次新快照就自增。
   *
   * 用于作废「上一会话排下的延时动作」：松手后的 600ms 结果延迟、280ms 收缩
   * 定时器，若在新会话铺开后才落地，会拿旧动作去转新一批文件、或用旧结果
   * 盖掉新盘面。
   */
  const sessionTokenRef = useRef(0);
  /** 提交后「收缩 → 发起转换」的定时器（切换会话时必须撤掉）。 */
  const collapseTimerRef = useRef<number | null>(null);
  /** 禁用提示的收起定时器。 */
  const rejectTimerRef = useRef<number | null>(null);
  /** 关闭动画 → 真隐藏的定时器。 */
  const closeTimerRef = useRef<number | null>(null);
  /** 入场动画兜底：到点摘掉动画类，保证盘面一定画出来。 */
  const stillTimerRef = useRef<number | null>(null);
  /** 本次执行开始时间：进度环至少显示 0.6s。 */
  const startedRef = useRef(0);
  /** 拖起来的那一刻指针就在盘心：先忽略 hover，直到指针真的移动过，
   *  否则一铺开就显示红色「取消」。 */
  const armedRef = useRef(false);
  const firstPosRef = useRef<{ x: number; y: number } | null>(null);
  /** 最近一次命中的区域：松手时据此决定「执行」还是「取消」。 */
  const lastHitRef = useRef<"segment" | "center" | "outside" | "empty" | null>(null);
  // 事件订阅只能拿旧闭包，跨渲染的最新值统一用 ref 镜像。
  const actionsRef = useRef<RingAction[]>([]);
  const activeRef = useRef(0);
  const hitRef = useRef<(x: number, y: number, force?: boolean) => void>(() => {});
  const execRef = useRef<(action: RingAction | undefined) => void>(() => {});
  const rejectRef = useRef<(reason: string) => void>(() => {});
  const cancelRef = useRef<() => void>(() => {});
  const closeRef = useRef<() => void>(() => {});

  const actions = snap.actions;
  const n = actions.length;
  const ready = snap.state === "ready" && n > 0;
  /** 进度/结果遮罩是否可见。 */
  const executing = progress !== null || result !== null;
  /** 任何形式的忙碌都屏蔽输入（含收缩/关闭动画，防重复提交）。 */
  const busy = executing || collapsing || closing || rejected !== null;
  const activeIndex = n ? Math.min(active, n - 1) : 0;
  actionsRef.current = actions;
  activeRef.current = activeIndex;

  /**
   * 应用一份新快照：盘面回到「首个可用扇区 + 打开动画」，并作废上个会话
   * 所有的延时任务。`pull`（窗口挂载）与 `ring://update`（会话更新）共用。
   */
  const applySnapshot = useCallback((next: RingSnapshot) => {
    sessionTokenRef.current += 1;
    clearTimer(collapseTimerRef);
    clearTimer(rejectTimerRef);
    clearTimer(closeTimerRef);
    setSnap(next);
    setActive(initialIndex(next.actions, next.suggested));
    setCenterHover(false);
    setNoDrop(false);
    setProgress(null);
    setResult(null);
    setHandoff(false);
    setCollapsing(false);
    setRejected(null);
    setClosing(false);
    // 页面不可见（窗口还没 show）时别演入场动画：动画会在隐藏的 webview 里
    // 停在首帧，窗口显示后就变成一张全透明的盘。可见时正常演，并用定时器兜底：
    // 到点检查容器透明度，仍然停在首帧就把动画类摘掉，强制落到最终状态。
    const hidden = document.visibilityState !== "visible";
    setStill(hidden);
    clearTimer(stillTimerRef);
    stillTimerRef.current = window.setTimeout(() => {
      stillTimerRef.current = null;
      const layer = layerRef.current;
      const opacity = layer ? Number(window.getComputedStyle(layer).opacity) : 1;
      // 动画正常放完时是 1；被冻住的会停在首帧（0.6）。
      if (!(opacity > 0.95)) setStill(true);
    }, STILL_AFTER_MS);
    // 新会话重新计数指针位移，避免沿用上一次的 hover 状态。
    armedRef.current = false;
    firstPosRef.current = null;
    lastHitRef.current = null;
    setAnimKey((k) => k + 1);
  }, []);

  const pull = useCallback(() => {
    void api
      .ringSnapshot()
      .then(applySnapshot)
      .catch(() => {});
  }, [applySnapshot]);

  // 订阅：显示 + 快照更新 + 进度 + 结果 + 5s 交接 + 拖动坐标。
  useEffect(() => {
    pull();
    const unshown = listen(EV_SHOWN, pull);
    const unupdate = listen(EV_UPDATE, (e) => {
      applySnapshot(e.payload as RingSnapshot);
    });
    const unprogress = listen(EV_PROGRESS, (e) => setProgress(e.payload as RingProgress));
    // 进度环至少显示 0.6s：极快的任务也不至于来不及看到结果。
    const unresult = listen(EV_RESULT, (e) => {
      const payload = e.payload as RingResult;
      const elapsed = Date.now() - startedRef.current;
      const wait = Math.max(0, 600 - elapsed);
      if (wait === 0) {
        setResult(payload);
        return;
      }
      // 延时期间可能已经换了一批文件：旧结果不能再盖到新盘面上。
      const token = sessionTokenRef.current;
      window.setTimeout(() => {
        if (sessionTokenRef.current === token) setResult(payload);
      }, wait);
    });
    const unhandoff = listen(EV_HANDOFF, () => setHandoff(true));
    // 宿主已经把屏幕坐标换成窗口内的 CSS 坐标。
    const unpointer = listen<{ x: number; y: number }>(EV_POINTER, (e) => {
      hitRef.current(e.payload.x, e.payload.y);
    });
    const unend = listen<{ x: number; y: number } | null>(EV_DRAG_END, (e) => {
      // 松手时按最后一次命中决定：落在扇区上就执行，其余（中心 / 环外）算取消。
      // 铺开之前的指针事件会被 ready 门挡掉，所以宿主随松手再带一次最后位置，
      // 处理「拖到扇区上后停住不动、直接松手」这种一个位置都没收到的情况。
      if (lastHitRef.current !== "segment" && e.payload) {
        hitRef.current(e.payload.x, e.payload.y, true);
      }
      const action =
        lastHitRef.current === "segment" ? actionsRef.current[activeRef.current] : undefined;
      const decision = resolveRingDragEnd(lastHitRef.current, action);
      if (decision.kind === "close") {
        closeRef.current();
        return;
      }
      if (decision.kind === "execute") {
        execRef.current(action);
        return;
      }
      // 禁用扇区：不执行，也不静默吞掉——把原因摆在中心让用户看见。
      rejectRef.current(decision.reason);
    });
    return () => {
      clearTimer(collapseTimerRef);
      clearTimer(rejectTimerRef);
      clearTimer(closeTimerRef);
      clearTimer(stillTimerRef);
      void unshown.then((f) => f());
      void unupdate.then((f) => f());
      void unprogress.then((f) => f());
      void unresult.then((f) => f());
      void unhandoff.then((f) => f());
      void unpointer.then((f) => f());
      void unend.then((f) => f());
    };
  }, [applySnapshot, pull]);

  // 结果保持后收起：成功 0.9s、失败 1.4s，再加淡出时间（对齐 Filee §1.4）。
  useEffect(() => {
    if (!result) return;
    if (resultSeenRef.current === result.generation) return;
    resultSeenRef.current = result.generation;
    const hold = result.failed > 0 ? 1400 : 900;
    const t = setTimeout(() => {
      void api.ringDismiss().catch(() => {});
      setProgress(null);
      setResult(null);
    }, hold + 250);
    return () => clearTimeout(t);
  }, [result]);

  const doExecute = useCallback(
    (action: RingAction | undefined) => {
      if (!action || !action.enabled || busy) return;
      setResult(null);
      startedRef.current = Date.now();
      // 扇区先向中心收缩，收缩完再切进度环（对齐 Filee §1.4）。
      setCollapsing(true);
      const token = sessionTokenRef.current;
      clearTimer(collapseTimerRef);
      collapseTimerRef.current = window.setTimeout(() => {
        collapseTimerRef.current = null;
        // 这 280ms 内换了会话：动作 id 属于上一批文件，提交会把新文件转错格式。
        if (sessionTokenRef.current !== token) return;
        // Job 会为**每个输入**都产生结果（含跳过的），所以进度总数要算上 skip，
        // 否则进度会在后面的文件上报出 2/1。
        setProgress({ generation: snap.generation, done: 0, total: action.convert + action.skip });
        setCollapsing(false);
        void api.ringExecute(action.id).catch(() => setProgress(null));
      }, 280);
    },
    [busy, snap.generation],
  );

  const doCancel = useCallback(() => {
    void api.ringCancel().catch(() => {});
  }, []);

  /**
   * 松手没命中可用扇区：扇区逐项缩回（0.16s / 10ms 错峰）后再关窗。
   *
   * 直接隐藏会让「松手」和「界面凭空消失」在体感上分不清是取消还是没拖到，
   * 而这一手势没有第二次机会解释（Wheel §1.3 关闭动画）。
   */
  const doClose = useCallback(() => {
    if (closing) return;
    setClosing(true);
    clearTimer(closeTimerRef);
    const token = sessionTokenRef.current;
    closeTimerRef.current = window.setTimeout(() => {
      closeTimerRef.current = null;
      if (sessionTokenRef.current !== token) return;
      void api.ringCancel().catch(() => {});
      setClosing(false);
    }, CLOSE_MS);
  }, [closing]);

  /**
   * 禁用扇区被松手命中：原因留一会儿再收起。
   *
   * 不立刻关窗是因为「松手 → 盘消失」和「松手 → 什么都没发生」在体感上一样，
   * 用户会以为是自己没拖准。留 1.3s 把原因写清楚，比日志里记一条有用得多。
   */
  const doReject = useCallback((reason: string) => {
    setRejected(reason);
    clearTimer(rejectTimerRef);
    const token = sessionTokenRef.current;
    rejectTimerRef.current = window.setTimeout(() => {
      rejectTimerRef.current = null;
      if (sessionTokenRef.current !== token) return;
      void api.ringCancel().catch(() => {});
      setRejected(null);
    }, REJECT_HOLD);
  }, []);

  // 事件订阅用的是旧闭包，这里把最新一次渲染的实例挂上去。
  execRef.current = doExecute;
  rejectRef.current = doReject;
  cancelRef.current = doCancel;
  closeRef.current = doClose;

  /**
   * 指针位置 → hover 状态。
   *
   * 唯一来源是宿主转发的拖动坐标；窗口本身 `ignore_cursor_events`，拿不到
   * 任何鼠标事件（这是「松手事件必须回到 Finder」的前提）。
   */
  const applyHit = useCallback(
    (x: number, y: number, force = false) => {
      if (!ready || busy) return;
      const first = firstPosRef.current;
      // force：松手时补的命中，没有「指针移动过」的历史也要直接判定。
      if (!force) {
        if (!first) {
          firstPosRef.current = { x, y };
          return;
        }
        if (!armedRef.current) {
          if (Math.hypot(x - first.x, y - first.y) < ARM_DISTANCE) return;
          armedRef.current = true;
        }
      }
      const hit = hitTest(C, C, x, y, n, R);
      lastHitRef.current = hit.kind;
      if (hit.kind === "segment") {
        setActive(hit.index);
        setCenterHover(false);
        setNoDrop(false);
      } else if (hit.kind === "center") {
        setCenterHover(true);
        setNoDrop(false);
      } else if (hit.kind === "outside") {
        // 环外不是环：不能让中心还写着「松手转为 JPG」而实际松手是取消。
        setCenterHover(false);
        setNoDrop(true);
      } else {
        setCenterHover(false);
        setNoDrop(false);
      }
    },
    [busy, n, ready],
  );
  hitRef.current = applyHit;

  const activeAction = actions[activeIndex];
  const activeDisabled = activeAction !== undefined && !activeAction.enabled;
  /**
   * 中心主标题：数量 + 格式（Wheel §1.3 A）。
   *
   * 单一格式写「3 个 PNG」，同类混选写「3 张图片」，跨类才写混合——
   * 选了 20 张 PNG 还只写「图片」等于什么都没说。
   */
  const centerTitle = ringCenterTitle(snap.count, snap.category);

  // 扇区：几何 path + 动画参数（活动扇区沿径向弹出并额外放大 2.5%）。
  const sectors = useMemo(
    () =>
      actions.map((action, i) => {
        const isActive = i === activeIndex && !centerHover;
        const u = outwardUnit(i, n);
        const pop = isActive ? POP : 0;
        const label = labelAnchor(C, C, R, r, i, n);
        return {
          action,
          d: sectorPath(C, C, R, r, i, n),
          tx: u.x * pop,
          ty: u.y * pop,
          lx: label.x,
          ly: label.y,
          /** 沿半径排布用的参数：旋转角（12 点起顺时针）与半径中线。 */
          ang: (midAngle(i, n) * 180) / Math.PI,
          radialMid: (R + r) / 2,
          isActive,
          delay: i * OPEN_STAGGER,
          closeDelay: i * CLOSE_STAGGER,
        };
      }),
    [actions, activeIndex, centerHover, n],
  );

  return (
    <div
      ref={rootRef}
      className="relative select-none outline-none [&_*]:select-none"
      style={{ width: SIZE, height: SIZE }}
    >
      {ready && !executing && (
        <div
          ref={layerRef}
          className={`absolute inset-0 ${collapsing ? "ring-collapse" : handoff ? "ring-handoff" : closing || still ? "" : "ring-layer-in"}`}
        >
          <svg
            width={SIZE}
            height={SIZE}
            viewBox={`0 0 ${SIZE} ${SIZE}`}
            className="pointer-events-none"
          >
            {sectors.map((s) => {
              const enabled = s.action.enabled;
              const reject = !enabled && s.isActive;
              const fill = reject ? REJECT_FILL : enabled ? (s.isActive ? ACCENT_FILL : "rgba(18,18,20,0.82)") : MUTED_FILL;
              const text = reject
                ? "rgba(255,214,214,0.92)"
                : enabled
                  ? s.isActive
                    ? ACCENT_ON
                    : "rgba(255,255,255,0.82)"
                  : MUTED_TEXT;
              return (
                <g
                  key={`${s.action.id}-${animKey}`}
                  transform={`translate(${C} ${C}) scale(${s.isActive ? HOVER_SCALE : 1}) translate(${-C} ${-C}) translate(${s.tx.toFixed(2)} ${s.ty.toFixed(2)})`}
                  style={{
                    // hover 弹出走弹簧曲线（Wheel §1.3：320 刚度 20 阻尼的
                    // overshoot 手感，用 back-out 贝塞尔近似）。
                    transition: "transform 180ms cubic-bezier(0.34, 1.56, 0.64, 1)",
                  }}
                >
                  <g
                    className={closing ? "ring-sector-out" : still ? undefined : "ring-sector-in"}
                    style={{
                      animationDelay: `${closing ? s.closeDelay : s.delay}ms`,
                      /* 绕圆心缩放：Wheel §1.3「从中心缩放并带轻微旋转」。 */
                      transformOrigin: `${C}px ${C}px`,
                    }}
                  >
                    <path
                      d={s.d}
                      fill={fill}
                      stroke={s.isActive ? "rgba(255,255,255,0.18)" : "rgba(255,255,255,0.08)"}
                      strokeWidth={1}
                      strokeLinejoin="round"
                      style={{ transition: "fill 120ms ease" }}
                    >
                      {/* 禁用原因同时做原生 tooltip：中心被占住时仍能查。 */}
                      {!enabled && <title>{s.action.disabled_reason ?? "这批文件用不上它"}</title>}
                    </path>
                    {sectors.length >= RADIAL_LABELS_FROM ? (
                      // 片数多：文字沿半径排布（绕圆心转到本扇区角度），左半圈
                      // 转 180° 保持正立，避免倒着读。
                      <g transform={`rotate(${s.ang.toFixed(2)} ${C} ${C})`}>
                        <text
                          x={C}
                          y={C - s.radialMid}
                          transform={`rotate(${s.ang > 90 && s.ang < 270 ? 90 : -90} ${C} ${C - s.radialMid})`}
                          textAnchor="middle"
                          dominantBaseline="middle"
                          fontSize={radialFontSize(sectors.length)}
                          fontWeight={s.isActive ? 600 : 500}
                          fill={text}
                          style={{ pointerEvents: "none", transition: "fill 120ms ease" }}
                        >
                          {s.action.label}
                        </text>
                        {s.action.convert + s.action.skip > 0 && (
                          <text
                            x={C}
                            y={C - s.radialMid + 10}
                            transform={`rotate(${s.ang > 90 && s.ang < 270 ? 90 : -90} ${C} ${C - s.radialMid + 10})`}
                            textAnchor="middle"
                            dominantBaseline="middle"
                            fontSize={8}
                            fill="rgba(255,255,255,0.45)"
                            style={{ pointerEvents: "none", transition: "fill 120ms ease" }}
                          >
                            {s.action.convert + s.action.skip} 个
                          </text>
                        )}
                      </g>
                    ) : (
                      <>
                        <text
                          x={s.lx}
                          y={s.action.convert + s.action.skip > 0 ? s.ly - 4 : s.ly}
                          textAnchor="middle"
                          dominantBaseline="middle"
                          fontSize={11}
                          fontWeight={s.isActive ? 600 : 500}
                          fill={text}
                          style={{ pointerEvents: "none", transition: "fill 120ms ease" }}
                        >
                          {s.action.label}
                        </text>
                        {/* 副行：落到这个动作的文件数（Filee 每扇区 caption）。 */}
                        {s.action.convert + s.action.skip > 0 && (
                          <text
                            x={s.lx}
                            y={s.ly + 8}
                            textAnchor="middle"
                            dominantBaseline="middle"
                            fontSize={8}
                            fill="rgba(255,255,255,0.45)"
                            style={{ pointerEvents: "none", transition: "fill 120ms ease" }}
                          >
                            {s.action.convert + s.action.skip} 个
                          </text>
                        )}
                      </>
                    )}
                  </g>
                </g>
              );
            })}
          </svg>

          {/* 中心：数量+格式 / 松手转为 X / 松手取消（Wheel §1.3、Integrated §4.2）。 */}
          <div
            className={`pointer-events-none absolute left-1/2 top-1/2 grid -translate-x-1/2 -translate-y-1/2 place-items-center rounded-full text-center ${closing ? "ring-center-out" : ""}`}
            style={{
              width: r * 2 - 12,
              height: r * 2 - 12,
              background: centerHover ? "rgba(248,113,113,0.16)" : "rgba(0,0,0,0.35)",
              transition: "background 120ms ease",
            }}
          >
            {centerHover || noDrop ? (
              <span className="text-[20px]/[24px] font-semibold" style={{ color: DANGER }}>
                松手取消
              </span>
            ) : rejected ? (
              <div className="px-[8px]">
                <div className="text-[12px]/[16px] font-semibold" style={{ color: DANGER }}>
                  没有转换
                </div>
                <div className="mt-[2px] line-clamp-3 text-[11px]/[14px] text-white/70">
                  {rejected}
                </div>
              </div>
            ) : (
              <div className="px-[6px]">
                <div className="text-[15px]/[19px] font-semibold text-white">{centerTitle}</div>
                {activeAction && (
                  <div
                    className="mt-[3px] line-clamp-2 text-[11px]/[15px]"
                    style={{ color: activeDisabled ? MUTED_TEXT : ACCENT }}
                  >
                    {activeDisabled
                      ? activeAction.disabled_reason ?? "这批文件用不上它"
                      : `松手转为 ${activeAction.label}`}
                  </div>
                )}
                <div className="mt-[2px] text-[10px]/[14px] text-white/40">
                  {activeAction && activeAction.skip > 0
                    ? `转换 ${activeAction.convert} · 跳过 ${activeAction.skip}`
                    : (snap.profile?.name ?? "拖到扇区上松手")}
                </div>
              </div>
            )}
          </div>

          {/* 底部提示：这盘只有这一个手势。 */}
          <div className="pointer-events-none absolute bottom-[8px] left-0 right-0 text-center text-[11px]/[15px] text-white/45">
            拖到扇区上松手转换 · 中心取消
          </div>
        </div>
      )}

      {executing && <ProgressOverlay progress={progress} result={result} handoff={handoff} />}

      {!ready && !busy && (
        <div className="grid h-full w-full place-items-center">
          <div className="max-w-[260px] px-[16px] text-center">
            {PENDING.has(snap.state) ? (
              <p className="text-[13px]/[19px] text-white/60">读取拖动的文件…</p>
            ) : (
              <>
                <p className="text-[13px]/[19px] text-white/80">
                  {snap.hint ?? "这批文件没有可用的转换动作。"}
                </p>
                <p className="mt-[8px] text-[11px]/[15px] text-white/40">松开鼠标即可关闭</p>
              </>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

/** 原地进度环 + 成功/失败反馈（对齐 Filee §1.4）。 */
function ProgressOverlay({
  progress,
  result,
  handoff,
}: {
  progress: RingProgress | null;
  result: RingResult | null;
  handoff: boolean;
}) {
  const PR = 36; // 进度环半径（文档：36px / 线宽 6px）。
  const STROKE = 6;
  const circ = 2 * Math.PI * PR;
  const total = progress?.total ?? 0;
  const done = progress?.done ?? 0;
  const ratio = total > 0 ? Math.min(1, done / total) : 0;
  const success = result ? result.failed === 0 : false;
  const color = result ? (success ? SUCCESS : DANGER) : ACCENT;
  /** 第一条失败 / 保真警告：只给计数等于什么也没说，这里把原因带出来。 */
  const note = result?.errors[0] ?? result?.warnings[0] ?? null;

  return (
    <div
      className={`${handoff ? "ring-handoff" : "ring-layer-in"} absolute inset-0 grid place-items-center`}
    >
      <div className="relative grid place-items-center" style={{ width: PR * 2 + 24, height: PR * 2 + 24 }}>
        <svg width={PR * 2 + 24} height={PR * 2 + 24} className="pointer-events-none">
          <g transform={`translate(${PR + 12} ${PR + 12})`}>
            <circle r={PR} fill="none" stroke="rgba(255,255,255,0.12)" strokeWidth={STROKE} />
            <circle
              r={PR}
              fill="none"
              stroke={color}
              strokeWidth={STROKE}
              strokeLinecap="round"
              strokeDasharray={circ}
              strokeDashoffset={result ? 0 : circ * (1 - ratio)}
              transform="rotate(-90)"
              style={{ transition: "stroke-dashoffset 220ms ease, stroke 160ms ease" }}
            />
          </g>
        </svg>
        <div className="absolute inset-0 grid place-items-center text-center">
          {result ? (
            <span
              className="grid h-[34px] w-[34px] place-items-center rounded-full text-[20px] font-bold text-black"
              style={{ background: color }}
            >
              {success ? "✓" : "!"}
            </span>
          ) : (
            <span className="text-[14px]/[18px] font-semibold text-white">
              {total > 0 ? `${Math.round(ratio * 100)}%` : "…"}
            </span>
          )}
        </div>
      </div>
      {note && (
        <div className="absolute bottom-[25px] left-[10px] right-[10px] truncate text-center text-[10px]/[14px] text-white/45">
          {note}
        </div>
      )}
      <div className="absolute bottom-[8px] left-0 right-0 text-center text-[11px]/[15px] text-white/55">
        {result
          ? `已转换 ${result.converted} · 跳过 ${result.skipped} · 失败 ${result.failed}${
              result.cancelled > 0 ? ` · 取消 ${result.cancelled}` : ""
            }`
          : `转换中 ${done}/${total || "…"}`}
      </div>
    </div>
  );
}
