import { describe, it, expect } from "vitest";
import { ApiError, toApiError, errorMessage } from "./errors";

describe("ApiError", () => {
  it("toString 返回纯文案（无 Error 前缀）", () => {
    const e = new ApiError("写入失败");
    expect(e.toString()).toBe("写入失败");
    expect(String(e)).toBe("写入失败");
    expect(e.message).toBe("写入失败");
  });

  it("保留 code / details", () => {
    const e = new ApiError("m", 1001, "d");
    expect(e.code).toBe(1001);
    expect(e.details).toBe("d");
  });
});

describe("toApiError", () => {
  it("已是 ApiError 原样返回", () => {
    const original = new ApiError("x");
    expect(toApiError(original)).toBe(original);
  });

  it("普通 Error 取其 message", () => {
    const e = toApiError(new Error("boom"));
    expect(e).toBeInstanceOf(ApiError);
    expect(e.message).toBe("boom");
  });

  it("纯文本字符串直接作为文案", () => {
    expect(toApiError("hello").message).toBe("hello");
  });

  it("JSON 字符串解析出 message + details + code", () => {
    const e = toApiError('{"code":1001,"message":"写入失败","details":"磁盘已满"}');
    expect(e.message).toBe("写入失败：磁盘已满");
    expect(e.code).toBe(1001);
    expect(e.details).toBe("磁盘已满");
  });

  it("后端对象折叠为可读文案", () => {
    const e = toApiError({ code: 42, message: "对象错误", details: "详情" });
    expect(e.message).toBe("对象错误：详情");
    expect(e.code).toBe(42);
  });

  it("只有 message 的对象", () => {
    expect(toApiError({ message: "仅消息" }).message).toBe("仅消息");
  });

  it("空串回落 fallback", () => {
    expect(toApiError("").message).toBe("操作失败");
  });

  it("null / undefined 回落 fallback", () => {
    expect(toApiError(null).message).toBe("操作失败");
    expect(toApiError(undefined).message).toBe("操作失败");
  });

  it("空对象回落 fallback（[object Object] 不泄漏）", () => {
    expect(toApiError({}).message).toBe("操作失败");
  });

  it("自定义 fallback", () => {
    expect(toApiError("", "出错了").message).toBe("出错了");
  });
});

describe("errorMessage", () => {
  it("返回可读文案", () => {
    expect(errorMessage("hello")).toBe("hello");
    expect(errorMessage({ message: "m" })).toBe("m");
    expect(errorMessage(null)).toBe("操作失败");
  });
});
