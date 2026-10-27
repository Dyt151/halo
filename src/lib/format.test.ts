import { describe, it, expect, vi, afterEach } from "vitest";
import {
  formatRelativeTime,
  formatSize,
  fileBasename,
  typeLabel,
} from "./format";

afterEach(() => {
  vi.useRealTimers();
});

describe("formatRelativeTime", () => {
  it("非法 / 缺失时间戳显示占位符", () => {
    expect(formatRelativeTime(0)).toBe("—");
    expect(formatRelativeTime(-1)).toBe("—");
    expect(formatRelativeTime(Number.NaN)).toBe("—");
    expect(formatRelativeTime(Number.POSITIVE_INFINITY)).toBe("—");
  });

  it("今天显示时刻", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-10-27T12:00:00"));
    const ts = new Date("2026-10-27T09:30:00").getTime();
    expect(formatRelativeTime(ts)).toBe("今天 09:30");
  });

  it("昨天显示时刻", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-10-27T12:00:00"));
    const ts = new Date("2026-10-26T20:05:00").getTime();
    expect(formatRelativeTime(ts)).toBe("昨天 20:05");
  });

  it("今年更早显示 M月d日 HH:mm", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-10-27T12:00:00"));
    const ts = new Date("2026-03-15T08:07:00").getTime();
    expect(formatRelativeTime(ts)).toBe("3月15日 08:07");
  });

  it("跨年显示 YYYY年M月d日 HH:mm", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-10-27T12:00:00"));
    const ts = new Date("2025-12-31T23:59:00").getTime();
    expect(formatRelativeTime(ts)).toBe("2025年12月31日 23:59");
  });
});

describe("formatSize", () => {
  it("0 或负数返回 0 B", () => {
    expect(formatSize(0)).toBe("0 B");
    expect(formatSize(-5)).toBe("0 B");
  });

  it("B 级不带小数", () => {
    expect(formatSize(500)).toBe("500 B");
  });

  it("KB / MB / GB 带两位小数", () => {
    expect(formatSize(1024)).toBe("1.00 KB");
    expect(formatSize(1536)).toBe("1.50 KB");
    expect(formatSize(1048576)).toBe("1.00 MB");
    expect(formatSize(1073741824)).toBe("1.00 GB");
  });
});

describe("fileBasename", () => {
  it("POSIX 路径取最后一段", () => {
    expect(fileBasename("/a/b/c.txt")).toBe("c.txt");
  });

  it("Windows 反斜杠路径同样适用", () => {
    expect(fileBasename("C:\\Users\\me\\file.md")).toBe("file.md");
  });

  it("纯文件名原样返回", () => {
    expect(fileBasename("plain.txt")).toBe("plain.txt");
  });
});

describe("typeLabel", () => {
  it("四种已知类型", () => {
    expect(typeLabel("text")).toBe("文本");
    expect(typeLabel("html")).toBe("富文本");
    expect(typeLabel("image")).toBe("图片");
    expect(typeLabel("files")).toBe("文件");
  });

  it("未知类型回落「其他」", () => {
    expect(typeLabel("unknown")).toBe("其他");
  });
});
