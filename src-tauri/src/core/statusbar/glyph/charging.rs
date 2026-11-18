//! 电池进度、缺口映射与充电扫光时间线。

use super::*;

/// 缺口占整段弧的比例（正版用弧长归一化，不是角度）。
pub(super) fn gap_fraction(width: f64) -> f64 {
    if !width.is_finite() || width <= 0.0 {
        return 0.0;
    }
    let arc_len = RING_R * deg(RING_SWEEP);
    (width / arc_len).clamp(0.0, 1.0)
}

/// 进度空间 `start..end`（0.0~1.0）映射成角度区间列表。
///
/// 正版 `batteryArc` 的语义：有缺口时，落进缺口里的那段进度是不可见的，于是
/// 一段进度被切成「缺口前」与「缺口后」两段分别描。
pub fn battery_arc_segments(start: f64, end: f64, top_gap_width: f64) -> Vec<(f64, f64)> {
    let s = start.clamp(0.0, 1.0);
    let e = end.clamp(0.0, 1.0);
    if !(e > s) {
        return Vec::new();
    }
    let angle = |p: f64| RING_START + RING_SWEEP * p;
    if top_gap_width <= 0.0 {
        return vec![(angle(s), angle(e))];
    }
    let gap = gap_fraction(top_gap_width);
    let gap_start = (1.0 - gap) / 2.0;
    let gap_end = gap_start + gap;
    let mut out = Vec::new();
    let first_end = e.min(gap_start);
    if first_end > s {
        out.push((angle(s), angle(first_end)));
    }
    let second_start = s.max(gap_end);
    if e > second_start {
        out.push((angle(second_start), angle(e)));
    }
    out
}

// ---------------------------------------------------------------------------
// 充电扫光（移植自 status-trio 的 ChargingEffect*）
// ---------------------------------------------------------------------------

/// 正版动画帧率：`ChargingEffectTimeline.framesPerSecond`。
pub const CHARGING_FPS: f64 = 20.0;
/// 常速一圈的时长。
pub const STEADY_CYCLE: f64 = 1.8;
/// 插上电源时那圈快扫的时长。
pub const BURST_CYCLE: f64 = 0.6;

const HEARTBEAT_DURATION: f64 = 0.25;
const TAIL_RATIO: f64 = 0.32;
const MAX_TAIL_TO_FILL_RATIO: f64 = 0.78;
const MIN_VISIBLE_FILL_RATIO: f64 = 0.25;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChargingKind {
    /// 插上电源的那圈快扫。
    Burst,
    /// 常速扫光。
    Steady,
}

impl ChargingKind {
    fn duration(self) -> f64 {
        match self {
            Self::Burst => BURST_CYCLE,
            Self::Steady => STEADY_CYCLE,
        }
    }
}

/// 当前处在动画循环的第几帧。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChargingPhase {
    pub step: usize,
    pub steps_per_cycle: usize,
    pub kind: ChargingKind,
    /// 命中「电量跳档」那一圈时心跳放大到 1.35。
    pub heartbeat_multiplier: f64,
}

/// 由「这段动画开始后过了多久」算出当前帧（正版 `ChargingEffectTimeline.phase`）。
pub fn charging_phase(
    elapsed: f64,
    kind: ChargingKind,
    heartbeat_multiplier: f64,
) -> Option<ChargingPhase> {
    if !elapsed.is_finite()
        || elapsed < 0.0
        || !heartbeat_multiplier.is_finite()
        || heartbeat_multiplier < 0.0
    {
        return None;
    }
    let duration = kind.duration();
    let steps_per_cycle = (duration * CHARGING_FPS).round() as usize;
    if steps_per_cycle == 0 {
        return None;
    }
    let cycle_elapsed = elapsed % duration;
    // 20Hz 的边沿可能差 1e-6 帧，只把这点残差向前贴，避免卡一帧。
    let frame_position = cycle_elapsed * CHARGING_FPS;
    let step = (frame_position + 1e-6).floor() as usize % steps_per_cycle;
    Some(ChargingPhase {
        step,
        steps_per_cycle,
        kind,
        heartbeat_multiplier,
    })
}

/// 一帧扫光要画的全部东西（正版 `ChargingEffectFrame`）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChargingFrame {
    /// 尾部渐变在进度空间的区间。
    pub tail: Option<(f64, f64)>,
    pub head: f64,
    pub head_visible: bool,
    pub tail_alpha: f64,
    pub bead_alpha: f64,
    pub heartbeat_alpha: f64,
    pub heartbeat_scale: f64,
    pub bolt_scale: f64,
    pub bolt_arc_color_amount: f64,
}

fn clamp_unit(value: f64) -> f64 {
    if !value.is_finite() {
        return 0.0;
    }
    value.clamp(0.0, 1.0)
}

/// 进度空间 → 圆上的点（设计单位，y 向下）。
pub fn battery_point(progress: f64) -> (f64, f64) {
    let angle = deg(RING_START + RING_SWEEP * clamp_unit(progress));
    (
        RING_CX + RING_R * angle.cos(),
        RING_CY + RING_R * angle.sin(),
    )
}

/// 进度空间 → 可见弧长归一值（缺口里的进度都映射到缺口左沿）。
pub fn visible_fraction(progress: f64, top_gap_width: f64) -> f64 {
    let p = clamp_unit(progress);
    if top_gap_width <= 0.0 {
        return p;
    }
    let gap = gap_fraction(top_gap_width);
    let gap_start = (1.0 - gap) / 2.0;
    let gap_end = gap_start + gap;
    if p <= gap_start {
        p
    } else if p >= gap_end {
        p - gap
    } else {
        gap_start
    }
}

/// 可见弧长归一值 → 进度空间（正版 `progress(forVisibleFraction:)`）。
pub fn progress_for_visible_fraction(fraction: f64, top_gap_width: f64) -> f64 {
    let f = clamp_unit(fraction);
    if top_gap_width <= 0.0 {
        return f;
    }
    let gap = gap_fraction(top_gap_width);
    let visible_length = 1.0 - gap;
    if visible_length <= 0.0 {
        return 0.0;
    }
    let visible = f.min(visible_length);
    let gap_start = visible_length / 2.0;
    if visible <= gap_start {
        visible
    } else {
        visible + gap
    }
}

/// 落在缺口里的进度截到左沿（正版 `lastVisibleProgress`）。
pub fn last_visible_progress(progress: f64, top_gap_width: f64) -> f64 {
    let p = clamp_unit(progress);
    if top_gap_width <= 0.0 {
        return p;
    }
    let gap = gap_fraction(top_gap_width);
    let gap_start = (1.0 - gap) / 2.0;
    let gap_end = gap_start + gap;
    if p > gap_start && p < gap_end {
        gap_start
    } else {
        p
    }
}

/// 由电量进度与当前帧推出扫光参数（正版 `ChargingEffectPolicy.frame`）。
pub fn charging_frame(
    progress: f64,
    phase: ChargingPhase,
    top_gap_width: f64,
) -> Option<ChargingFrame> {
    if !progress.is_finite()
        || phase.steps_per_cycle == 0
        || phase.step >= phase.steps_per_cycle
        || !phase.heartbeat_multiplier.is_finite()
        || phase.heartbeat_multiplier < 0.0
    {
        return None;
    }
    let battery_progress = progress.clamp(0.0, 1.0);
    let visible_arc_length = visible_fraction(1.0, top_gap_width);
    let visible_fill = visible_fraction(battery_progress, top_gap_width);
    let endpoint = last_visible_progress(battery_progress, top_gap_width);

    let heartbeat_steps = ((HEARTBEAT_DURATION * CHARGING_FPS).round() as usize)
        .clamp(1, phase.steps_per_cycle);
    let travel_steps = (phase.steps_per_cycle - heartbeat_steps).max(1);
    let travel_fraction = (phase.step as f64 / travel_steps as f64).min(1.0);
    let head = endpoint * travel_fraction;

    let heartbeat_progress = if phase.step >= travel_steps {
        let pulse_step = phase.step - travel_steps;
        if heartbeat_steps <= 1 {
            0.0
        } else {
            pulse_step as f64 / (heartbeat_steps - 1) as f64
        }
    } else {
        0.0
    };
    let bounded_heartbeat = heartbeat_progress.clamp(0.0, 1.0);
    let pulse = if bounded_heartbeat == 0.0 || bounded_heartbeat == 1.0 {
        0.0
    } else {
        (std::f64::consts::PI * bounded_heartbeat).sin()
    };
    let multiplier = phase.heartbeat_multiplier.min(1.35);
    let heartbeat_alpha = pulse * multiplier;
    let is_heartbeat_step = phase.step >= travel_steps;
    let heartbeat_scale = if is_heartbeat_step {
        (1.0 + 0.55 * (1.0 - pulse)) * multiplier
    } else {
        1.0
    };
    let head_visible = (last_visible_progress(head, top_gap_width) - head).abs() < 1e-9;

    let (bolt_scale, bolt_arc_color_amount) = if top_gap_width > 0.0 {
        let gap_start = last_visible_progress(0.5, top_gap_width);
        let gap_end = 1.0 - gap_start;
        if endpoint >= gap_end && head > gap_start && head < gap_end {
            let amount = (std::f64::consts::PI
                * ((head - gap_start) / (gap_end - gap_start)).clamp(0.0, 1.0))
            .sin();
            (1.0 + 0.2 * amount, amount)
        } else {
            (1.0, 0.0)
        }
    } else {
        (1.0, 0.0)
    };

    let minimum_tail_fill = MIN_VISIBLE_FILL_RATIO * visible_arc_length;
    let tail_length =
        (TAIL_RATIO * visible_arc_length).min(MAX_TAIL_TO_FILL_RATIO * visible_fill);
    let tail = if visible_fill >= minimum_tail_fill && head > 0.0 && tail_length > 0.0 {
        let visible_head = visible_fraction(head, top_gap_width);
        let start = progress_for_visible_fraction(
            (visible_head - tail_length).max(0.0),
            top_gap_width,
        );
        let end = progress_for_visible_fraction(visible_head, top_gap_width);
        Some((start, end))
    } else {
        None
    };

    Some(ChargingFrame {
        tail,
        head,
        head_visible,
        tail_alpha: if tail.is_some() { 1.0 } else { 0.0 },
        bead_alpha: if head_visible {
            if is_heartbeat_step {
                (1.0 - bounded_heartbeat).max(0.0)
            } else {
                1.0
            }
        } else {
            0.0
        },
        heartbeat_alpha,
        heartbeat_scale,
        bolt_scale,
        bolt_arc_color_amount,
    })
}

