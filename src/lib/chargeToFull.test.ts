import { describe, expect, it } from "vitest";
import {
  canStartChargeToFull,
  chargeToFullPresentation,
  showsActionArea,
} from "./chargeToFull";
import type { ChargeToFullPhase } from "./chargeToFull";
import type { StatusBarBattery, StatusBarChargeToFullState } from "./types";

function battery(overrides: Partial<StatusBarBattery> = {}): StatusBarBattery {
  return {
    present: true,
    percent: 90,
    charging: false,
    charged: false,
    plugged: true,
    low_power: false,
    minutes_to_empty: null,
    minutes_to_full: null,
    ...overrides,
  };
}

function state(overrides: Partial<StatusBarChargeToFullState> = {}): StatusBarChargeToFullState {
  return {
    available: true,
    valid: true,
    has_active_limit: true,
    override_active: false,
    limit: 90,
    error: null,
    ...overrides,
  };
}

const IDLE: ChargeToFullPhase = { kind: "idle" };
const NOW = 1_000_000;

describe("canStartChargeToFull", () => {
  it("需要接电、未充满且低于 100%", () => {
    expect(canStartChargeToFull(battery())).toBe(true);
    expect(canStartChargeToFull(battery({ plugged: false }))).toBe(false);
    expect(canStartChargeToFull(battery({ charged: true }))).toBe(false);
    expect(canStartChargeToFull(battery({ percent: 100 }))).toBe(false);
    expect(canStartChargeToFull(battery({ present: false }))).toBe(false);
  });
});

describe("chargeToFullPresentation", () => {
  it("没有读数或这台 Mac 不支持时整行隐藏", () => {
    expect(chargeToFullPresentation(battery(), null, IDLE, NOW)).toEqual({ kind: "hidden" });
    expect(
      chargeToFullPresentation(battery(), state({ available: false }), IDLE, NOW),
    ).toEqual({ kind: "hidden" });
    expect(
      chargeToFullPresentation(battery({ present: false }), state(), IDLE, NOW),
    ).toEqual({ kind: "hidden" });
  });

  it("接电且上限生效时显示按钮", () => {
    expect(chargeToFullPresentation(battery(), state(), IDLE, NOW)).toEqual({ kind: "available" });
  });

  it("没有生效上限、拔电、已充满或 100% 时隐藏", () => {
    expect(
      chargeToFullPresentation(battery(), state({ has_active_limit: false }), IDLE, NOW),
    ).toEqual({ kind: "hidden" });
    expect(
      chargeToFullPresentation(battery({ plugged: false }), state(), IDLE, NOW),
    ).toEqual({ kind: "hidden" });
    expect(
      chargeToFullPresentation(battery({ charged: true }), state(), IDLE, NOW),
    ).toEqual({ kind: "hidden" });
    expect(
      chargeToFullPresentation(battery({ percent: 100 }), state(), IDLE, NOW),
    ).toEqual({ kind: "hidden" });
  });

  it("请求中与恢复中各自占位", () => {
    expect(
      chargeToFullPresentation(battery(), state(), { kind: "applying" }, NOW),
    ).toEqual({ kind: "applying" });
    expect(
      chargeToFullPresentation(battery(), state(), { kind: "cancelling" }, NOW),
    ).toEqual({ kind: "cancelling" });
  });

  it("临时解除上限生效后显示「正在充电至 100%」，拔电也能取消", () => {
    expect(
      chargeToFullPresentation(
        battery({ charging: true }),
        state({ override_active: true, has_active_limit: false, limit: null }),
        IDLE,
        NOW,
      ),
    ).toEqual({ kind: "chargingToFull" });
    expect(
      chargeToFullPresentation(
        battery({ plugged: false, charging: false }),
        state({ override_active: true, has_active_limit: false, limit: null }),
        { kind: "failed" },
        NOW,
      ),
    ).toEqual({ kind: "chargingToFull" });
  });

  it("受理后系统尚未反映时保持「正在充电至 100%」，最多 15 秒", () => {
    const accepted: ChargeToFullPhase = { kind: "accepted", limit: 90, at: NOW };
    expect(
      chargeToFullPresentation(battery(), state({ limit: 90 }), accepted, NOW + 5_000),
    ).toEqual({ kind: "chargingToFull" });
    expect(
      chargeToFullPresentation(battery(), state({ limit: 90 }), accepted, NOW + 16_000),
    ).toEqual({ kind: "available" });
  });

  it("请求失败后按钮保留，可重试", () => {
    expect(
      chargeToFullPresentation(battery(), state(), { kind: "failed" }, NOW),
    ).toEqual({ kind: "failed" });
    expect(
      chargeToFullPresentation(battery({ plugged: false }), state(), { kind: "failed" }, NOW),
    ).toEqual({ kind: "hidden" });
  });

  it("系统报错时显示不可用提示", () => {
    expect(
      chargeToFullPresentation(battery(), state({ error: "boom" }), IDLE, NOW),
    ).toEqual({ kind: "unavailable" });
  });

  it("恢复失败优先于系统报错，把原因带给用户", () => {
    expect(
      chargeToFullPresentation(
        battery(),
        state({ error: "boom" }),
        { kind: "cancelFailed", message: "恢复被拒绝" },
        NOW,
      ),
    ).toEqual({ kind: "cancelFailed", message: "恢复被拒绝" });
  });

  it("恢复后的上限行在充电时保留，到档位后消失", () => {
    const restored: ChargeToFullPhase = { kind: "restored", limit: 80, at: NOW };
    expect(
      chargeToFullPresentation(battery({ percent: 75, charging: true }), state({ limit: 80 }), restored, NOW + 60_000),
    ).toEqual({ kind: "limitRestored", limit: 80 });
    expect(
      chargeToFullPresentation(battery({ percent: 80, charging: false }), state({ limit: 80 }), restored, NOW + 60_000),
    ).toEqual({ kind: "available" });
  });

  it("恢复后系统迟迟不充电只等 30 秒", () => {
    const restored: ChargeToFullPhase = { kind: "restored", limit: 80, at: NOW };
    expect(
      chargeToFullPresentation(battery({ percent: 75, charging: false }), state({ limit: 80 }), restored, NOW + 10_000),
    ).toEqual({ kind: "limitRestored", limit: 80 });
    expect(
      chargeToFullPresentation(battery({ percent: 75, charging: false }), state({ limit: 80 }), restored, NOW + 31_000),
    ).toEqual({ kind: "available" });
  });

  it("恢复后拔电就收起", () => {
    const restored: ChargeToFullPhase = { kind: "restored", limit: 80, at: NOW };
    expect(
      chargeToFullPresentation(battery({ plugged: false, percent: 75 }), state({ limit: 80 }), restored, NOW + 1_000),
    ).toEqual({ kind: "hidden" });
  });
});

describe("showsActionArea", () => {
  it("只有 hidden 不占位", () => {
    expect(showsActionArea({ kind: "hidden" })).toBe(false);
    expect(showsActionArea({ kind: "unavailable" })).toBe(true);
    expect(showsActionArea({ kind: "limitRestored", limit: 80 })).toBe(true);
    expect(showsActionArea({ kind: "available" })).toBe(true);
  });
});
