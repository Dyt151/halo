import { describe, expect, it } from "vitest";
import { normalizeSection } from "./section";

describe("normalizeSection · 存量旧值必须回落，不能白屏", () => {
  it("「permissions」并入通用页后落到通用（存量用户的 last_settings_section）", () => {
    expect(normalizeSection("permissions")).toBe("general");
  });

  it("已下线的快捷键分区落到通用", () => {
    expect(normalizeSection("hotkeys")).toBe("general");
  });

  it("已下线的快捷回复分区落到通用", () => {
    expect(normalizeSection("replies")).toBe("general");
  });

  it("已下线的模块分区落到通用", () => {
    expect(normalizeSection("modules")).toBe("general");
  });

  it("旧别名「settings」落到通用", () => {
    expect(normalizeSection("settings")).toBe("general");
  });

  it("旧别名「convert」落到环形转换", () => {
    expect(normalizeSection("convert")).toBe("ring");
  });
});

describe("normalizeSection · 现存分区原样透传", () => {
  it("每个还在侧边栏里的分区都原样返回", () => {
    for (const page of [
      "general",
      "clipboard",
      "ring",
      "context-menu",
      "statusbar",
      "sync",
      "backup",
      "about",
      "history",
    ] as const) {
      expect(normalizeSection(page)).toBe(page);
    }
  });
});

describe("normalizeSection · 空值与未知值", () => {
  it("null / undefined / 空串都返回 null（调用方应保持原样）", () => {
    expect(normalizeSection(null)).toBeNull();
    expect(normalizeSection(undefined)).toBeNull();
    expect(normalizeSection("")).toBeNull();
  });

  it("未知分区返回 null，而不是猜一个页面", () => {
    expect(normalizeSection("no-such-page")).toBeNull();
    expect(normalizeSection("Permissions")).toBeNull(); // 大小写敏感
  });

  it("返回的永远是当前真实存在的分区或 null", () => {
    const known = new Set([
      "general",
      "clipboard",
      "ring",
      "context-menu",
      "statusbar",
      "sync",
      "backup",
      "about",
      "history",
    ]);
    for (const raw of ["permissions", "hotkeys", "settings", "convert", "about", "nope", ""]) {
      const out = normalizeSection(raw);
      expect(out === null || known.has(out)).toBe(true);
    }
  });
});
