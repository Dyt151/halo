/**
 * 「立即充满电」的展示状态机，照抄 Status Trio `ChargeToFullNowController`。
 *
 * 后端只给 PowerUI 的原始读数；本地 phase 记「正在请求 / 正在恢复 / 刚恢复」
 * 这类只有前端知道的过程态。纯函数，便于测。
 */
import type { StatusBarBattery, StatusBarChargeToFullState } from "./types";

export type ChargeToFullPhase =
  | { kind: "idle" }
  | { kind: "applying" }
  | { kind: "cancelling" }
  /** 请求已受理，但 PowerUI 的临时状态最多要 10 秒才出现（正版同样给 15 秒宽限）。 */
  | { kind: "accepted"; limit: number | null; at: number }
  | { kind: "failed" }
  | { kind: "cancelFailed"; message: string | null }
  /** 上限已恢复，等电池朝这个档位充回去（正版 `limitRestored`）。 */
  | { kind: "restored"; limit: number; at: number };

export type ChargeToFullPresentation =
  | { kind: "hidden" }
  | { kind: "available" }
  | { kind: "applying" }
  | { kind: "chargingToFull" }
  | { kind: "failed" }
  | { kind: "unavailable" }
  | { kind: "cancelling" }
  | { kind: "cancelFailed"; message: string | null }
  | { kind: "limitRestored"; limit: number };

/** 正版 `ChargeToFullNowDemand.canStartChargeToFull`。 */
export function canStartChargeToFull(battery: StatusBarBattery): boolean {
  return (
    battery.present &&
    battery.plugged &&
    !battery.charged &&
    battery.percent < 100
  );
}

/**
 * 恢复后的上限行留到系统真的朝它充：看到过充电就跟着 `isCharging`，
 * 没看到过就给 30 秒宽限（系统恢复充电有延迟），之后收起。
 */
function restoredLimitHolds(
  phase: { limit: number; at: number },
  battery: StatusBarBattery,
  now: number,
): boolean {
  return (
    canStartChargeToFull(battery) &&
    battery.percent < phase.limit &&
    (battery.charging || now - phase.at < 30_000)
  );
}

export function chargeToFullPresentation(
  battery: StatusBarBattery,
  state: StatusBarChargeToFullState | null,
  phase: ChargeToFullPhase,
  now: number,
): ChargeToFullPresentation {
  if (!battery.present || !state?.available) return { kind: "hidden" };

  if (phase.kind === "applying") return { kind: "applying" };
  if (phase.kind === "cancelling") return { kind: "cancelling" };
  if (phase.kind === "cancelFailed") {
    return { kind: "cancelFailed", message: phase.message };
  }
  if (state.error) return { kind: "unavailable" };
  // 临时解除上限可能是 Halo 发起的，也可能是系统电池菜单发起的；两者都能取消。
  if (state.override_active) return { kind: "chargingToFull" };
  if (phase.kind === "accepted") {
    const stillOriginal =
      !state.has_active_limit || state.limit === null || state.limit === phase.limit;
    if (now - phase.at < 15_000 && stillOriginal) return { kind: "chargingToFull" };
  }
  if (phase.kind === "restored") {
    if (restoredLimitHolds(phase, battery, now)) {
      return { kind: "limitRestored", limit: phase.limit };
    }
  }
  if (!canStartChargeToFull(battery)) return { kind: "hidden" };
  if (phase.kind === "failed") return { kind: "failed" };
  return state.has_active_limit ? { kind: "available" } : { kind: "hidden" };
}

/** 正版 `showsActionArea`：没按钮的 `unavailable` / `limitRestored` 也占位。 */
export function showsActionArea(presentation: ChargeToFullPresentation): boolean {
  return presentation.kind !== "hidden";
}
