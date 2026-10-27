import { describe, expect, it } from "vitest";
import { matchesMime } from "./mime";
import type { ContentType } from "./types";

describe("matchesMime · 放行规则", () => {
  it("空 mime 列表 = 全放行", () => {
    expect(matchesMime("text", [])).toBe(true);
    expect(matchesMime("image", [])).toBe(true);
    expect(matchesMime("files", [])).toBe(true);
  });

  it("* 与 */* 全放行", () => {
    expect(matchesMime("files", ["*"])).toBe(true);
    expect(matchesMime("unknown", ["*/*"])).toBe(true);
  });
});

describe("matchesMime · 大类匹配", () => {
  it("group/* 只比对大类", () => {
    expect(matchesMime("text", ["text/*"])).toBe(true);
    expect(matchesMime("image", ["image/*"])).toBe(true);
    expect(matchesMime("text", ["image/*"])).toBe(false);
    expect(matchesMime("files", ["application/*"])).toBe(false);
  });

  it("具体类型按大类就近匹配", () => {
    expect(matchesMime("text", ["text/plain"])).toBe(true);
    expect(matchesMime("image", ["image/png"])).toBe(true);
    expect(matchesMime("image", ["text/plain"])).toBe(false);
  });
});

describe("matchesMime · html 属于 text 族", () => {
  it("text/html 命中 html", () => {
    expect(matchesMime("html", ["text/html"])).toBe(true);
  });

  it("text/plain 也命中 html（同族）", () => {
    expect(matchesMime("html", ["text/plain"])).toBe(true);
  });

  it("text/* 不命中 html（sub=* 时只比大类）", () => {
    expect(matchesMime("html", ["text/*"])).toBe(false);
  });
});

describe("matchesMime · 文件类型", () => {
  it("application/pdf 命中 files", () => {
    expect(matchesMime("files", ["application/pdf"])).toBe(true);
  });

  it("application/pdf 不命中 text", () => {
    expect(matchesMime("text", ["application/pdf"])).toBe(false);
  });
});

describe("matchesMime · 多条 OR 与未知类型", () => {
  it("任一命中即放行", () => {
    expect(matchesMime("files", ["text/plain", "application/pdf"])).toBe(true);
    expect(matchesMime("text", ["image/png", "text/plain"])).toBe(true);
    expect(matchesMime("image", ["text/plain", "application/pdf"])).toBe(false);
  });

  it("unknown 类型不命中任何具体 mime", () => {
    expect(matchesMime("unknown", ["text/plain"])).toBe(false);
  });
});

// 覆盖 ContentType 全枚举，确保无分支遗漏。
const ALL_TYPES: ContentType[] = ["text", "html", "image", "files", "unknown"];
describe("matchesMime · 全类型冒烟", () => {
  it("任何类型对空列表都放行", () => {
    for (const t of ALL_TYPES) expect(matchesMime(t, [])).toBe(true);
  });
});
