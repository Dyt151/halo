import { describe, expect, it } from "vitest";
import {
  initialIndex,
  resolveRingDragEnd,
  ringCenterTitle,
} from "./ringState";
import type { RingAction } from "./types";

function action(overrides: Partial<RingAction> = {}): RingAction {
  return {
    id: "a",
    label: "A",
    target: "png",
    convert: 1,
    skip: 0,
    enabled: true,
    disabled_reason: null,
    ...overrides,
  };
}

describe("initialIndex · 初始高亮", () => {
  it("记住的上次动作可用时，高亮它", () => {
    const actions = [
      action({ id: "x", enabled: true }),
      action({ id: "y", enabled: true }),
    ];
    expect(initialIndex(actions, "y")).toBe(1);
  });

  it("记住的动作被禁用时，回落到第一个可用扇区", () => {
    const actions = [
      action({ id: "x", enabled: false, disabled_reason: "用不上" }),
      action({ id: "y", enabled: true }),
    ];
    expect(initialIndex(actions, "x")).toBe(1);
  });

  it("无记住动作时，选第一个可用扇区", () => {
    const actions = [
      action({ id: "x", enabled: false }),
      action({ id: "y", enabled: true }),
      action({ id: "z", enabled: true }),
    ];
    expect(initialIndex(actions, null)).toBe(1);
  });

  it("全部禁用时退回 0", () => {
    const actions = [action({ id: "x", enabled: false })];
    expect(initialIndex(actions, null)).toBe(0);
  });

  it("空列表退回 0", () => {
    expect(initialIndex([], null)).toBe(0);
  });
});

describe("ringCenterTitle · 中心标题", () => {
  it("无类别写「N 个文件」", () => {
    expect(ringCenterTitle(3, null)).toBe("3 个文件");
  });

  it("图片写「N 张图片」", () => {
    expect(ringCenterTitle(3, "图片")).toBe("3 张图片");
  });

  it("文本写「N 个文本」", () => {
    expect(ringCenterTitle(3, "文本")).toBe("3 个文本");
  });

  it("混合文件写「N 个文件 · 混合」", () => {
    expect(ringCenterTitle(3, "混合文件")).toBe("3 个文件 · 混合");
  });

  it("其它类别直接拼接", () => {
    expect(ringCenterTitle(2, "PDF")).toBe("2 个 PDF");
  });
});

describe("resolveRingDragEnd · 松手决策", () => {
  it("从未命中（null）→ 关闭", () => {
    expect(resolveRingDragEnd(null, action())).toEqual({ kind: "close" });
  });

  it("中心 / 环外 / 空环 → 关闭", () => {
    expect(resolveRingDragEnd("center", action())).toEqual({ kind: "close" });
    expect(resolveRingDragEnd("outside", action())).toEqual({ kind: "close" });
    expect(resolveRingDragEnd("empty", action())).toEqual({ kind: "close" });
  });

  it("命中扇区但没有对应动作 → 关闭", () => {
    expect(resolveRingDragEnd("segment", undefined)).toEqual({ kind: "close" });
  });

  it("命中可用扇区 → 执行", () => {
    expect(resolveRingDragEnd("segment", action({ enabled: true }))).toEqual({ kind: "execute" });
  });

  it("命中禁用扇区 → 拒绝并带原因", () => {
    expect(
      resolveRingDragEnd("segment", action({ enabled: false, disabled_reason: "这批文件里没有能用到它的输入" })),
    ).toEqual({ kind: "reject", reason: "这批文件里没有能用到它的输入" });
  });

  it("禁用扇区无原因时用默认文案", () => {
    expect(resolveRingDragEnd("segment", action({ enabled: false, disabled_reason: null }))).toEqual(
      { kind: "reject", reason: "这个动作在这批文件上不可用" },
    );
  });
});
