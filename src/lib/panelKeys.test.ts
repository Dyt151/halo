import { describe, expect, it } from "vitest";
import { resolvePanelKey, type PanelKeyContext } from "./panelKeys";

function ctx(overrides: Partial<PanelKeyContext> = {}): PanelKeyContext {
  return {
    key: "",
    metaOrCtrl: false,
    composing: false,
    isSearch: false,
    searchEmpty: false,
    isTyping: false,
    targetInClipCard: false,
    dialogOpen: false,
    hasTextSelection: false,
    hasFilter: false,
    numHotkeyEnabled: true,
    boardStyle: "list",
    itemCount: 5,
    selected: 2,
    ...overrides,
  };
}

describe("resolvePanelKey · 输入法保护", () => {
  it("组合输入中，Enter 不当作回贴", () => {
    expect(resolvePanelKey(ctx({ key: "Enter", composing: true })).type).toBe("ignore");
  });

  it("组合输入中，⌘1~9 不当作回贴", () => {
    expect(
      resolvePanelKey(ctx({ key: "1", metaOrCtrl: true, composing: true })).type,
    ).toBe("ignore");
  });
});

describe("resolvePanelKey · Esc / ⌘F", () => {
  it("Esc 收起面板（搜索框为空 / 对话框打开都生效）", () => {
    expect(resolvePanelKey(ctx({ key: "Escape" }))).toEqual({ type: "close" });
    expect(resolvePanelKey(ctx({ key: "Escape", isTyping: true, searchEmpty: true }))).toEqual({
      type: "close",
    });
    expect(resolvePanelKey(ctx({ key: "Escape", dialogOpen: true }))).toEqual({ type: "close" });
  });

  it("搜索框有内容时 Esc 先清空，不收起面板", () => {
    expect(
      resolvePanelKey(ctx({ key: "Escape", isSearch: true, isTyping: true, searchEmpty: false })),
    ).toEqual({ type: "clearSearch" });
  });

  it("⌘F / Ctrl+F 聚焦搜索", () => {
    expect(resolvePanelKey(ctx({ key: "f", metaOrCtrl: true }))).toEqual({ type: "focusSearch" });
    expect(resolvePanelKey(ctx({ key: "F", metaOrCtrl: true }))).toEqual({ type: "focusSearch" });
  });

  it("不带修饰键的 f 不触发任何动作", () => {
    expect(resolvePanelKey(ctx({ key: "f" })).type).toBe("ignore");
  });
});

describe("resolvePanelKey · 其它输入控件与对话框", () => {
  it("焦点在非搜索的输入控件里，其它按键整套透传", () => {
    expect(
      resolvePanelKey(ctx({ key: "1", metaOrCtrl: true, isTyping: true, isSearch: false })).type,
    ).toBe("ignore");
  });

  it("对话框打开时，⌘1~9 / ⌘D 等被屏蔽", () => {
    expect(resolvePanelKey(ctx({ key: "1", metaOrCtrl: true, dialogOpen: true })).type).toBe(
      "ignore",
    );
    expect(resolvePanelKey(ctx({ key: "d", metaOrCtrl: true, dialogOpen: true })).type).toBe(
      "ignore",
    );
  });
});

describe("resolvePanelKey · ⌘1~9 回贴第 N 条", () => {
  it("⌘1 回贴第一条（index 0）", () => {
    expect(resolvePanelKey(ctx({ key: "1", metaOrCtrl: true }))).toEqual({
      type: "pasteIndex",
      index: 0,
    });
  });

  it("⌘9 回贴第九条（index 8）", () => {
    expect(resolvePanelKey(ctx({ key: "9", metaOrCtrl: true, itemCount: 9 }))).toEqual({
      type: "pasteIndex",
      index: 8,
    });
  });

  it("序号超出列表长度时不动作", () => {
    expect(resolvePanelKey(ctx({ key: "9", metaOrCtrl: true, itemCount: 5 })).type).toBe(
      "ignore",
    );
  });

  it("有筛选条件时 ⌘N 语义失效", () => {
    expect(resolvePanelKey(ctx({ key: "1", metaOrCtrl: true, hasFilter: true })).type).toBe(
      "ignore",
    );
  });

  it("配置关闭 ⌘N 时不动作", () => {
    expect(
      resolvePanelKey(ctx({ key: "1", metaOrCtrl: true, numHotkeyEnabled: false })).type,
    ).toBe("ignore");
  });
});

describe("resolvePanelKey · Enter 回贴", () => {
  it("普通 Enter 回贴当前选中", () => {
    expect(resolvePanelKey(ctx({ key: "Enter" }))).toEqual({ type: "pasteSelected" });
  });

  it("搜索框内 Enter 先落地防抖查询再回贴第一条", () => {
    expect(resolvePanelKey(ctx({ key: "Enter", isSearch: true, isTyping: true }))).toEqual({
      type: "pasteSearch",
    });
  });

  it("空列表 + 普通 Enter 不动作", () => {
    expect(resolvePanelKey(ctx({ key: "Enter", itemCount: 0 })).type).toBe("ignore");
  });

  it("搜索框内 Enter 即使列表为空也走 pasteSearch（由组件查最新）", () => {
    expect(
      resolvePanelKey(ctx({ key: "Enter", isSearch: true, isTyping: true, itemCount: 0 })).type,
    ).toBe("pasteSearch");
  });

  it("目标在卡片内时不重复回贴", () => {
    expect(resolvePanelKey(ctx({ key: "Enter", targetInClipCard: true })).type).toBe("ignore");
  });
});

describe("resolvePanelKey · ⌘C / ⌘D / ⌘⌫", () => {
  it("⌘C 复制当前选中（大小写均可）", () => {
    expect(resolvePanelKey(ctx({ key: "c", metaOrCtrl: true }))).toEqual({ type: "copy" });
    expect(resolvePanelKey(ctx({ key: "C", metaOrCtrl: true }))).toEqual({ type: "copy" });
  });

  it("手动选中了文字时 ⌘C 让位给系统复制", () => {
    expect(
      resolvePanelKey(ctx({ key: "c", metaOrCtrl: true, hasTextSelection: true })).type,
    ).toBe("ignore");
  });

  it("空列表时 ⌘C 不动作", () => {
    expect(resolvePanelKey(ctx({ key: "c", metaOrCtrl: true, itemCount: 0 })).type).toBe(
      "ignore",
    );
  });

  it("⌘D 切换收藏", () => {
    expect(resolvePanelKey(ctx({ key: "d", metaOrCtrl: true }))).toEqual({
      type: "toggleFavorite",
    });
  });

  it("⌘⌫ 删除当前选中", () => {
    expect(resolvePanelKey(ctx({ key: "Backspace", metaOrCtrl: true }))).toEqual({
      type: "delete",
    });
  });

  it("搜索框内 ⌘⌫ 是「删到行首」，不当作删除", () => {
    expect(
      resolvePanelKey(ctx({ key: "Backspace", metaOrCtrl: true, isSearch: true, isTyping: true }))
        .type,
    ).toBe("ignore");
  });
});

describe("resolvePanelKey · 方向键导航", () => {
  it("横向排列：→ 下一条，← 上一条", () => {
    const horizontal = { boardStyle: "horizontal" };
    expect(resolvePanelKey(ctx({ key: "ArrowRight", ...horizontal }))).toEqual({
      type: "navigate",
      delta: 1,
    });
    expect(resolvePanelKey(ctx({ key: "ArrowLeft", ...horizontal }))).toEqual({
      type: "navigate",
      delta: -1,
    });
    expect(resolvePanelKey(ctx({ key: "ArrowUp", ...horizontal })).type).toBe("ignore");
    expect(resolvePanelKey(ctx({ key: "ArrowDown", ...horizontal })).type).toBe("ignore");
  });

  it("纵向排列：↓ 下一条，↑ 上一条", () => {
    expect(resolvePanelKey(ctx({ key: "ArrowDown", boardStyle: "vertical" }))).toEqual({
      type: "navigate",
      delta: 1,
    });
    expect(resolvePanelKey(ctx({ key: "ArrowUp", boardStyle: "vertical" }))).toEqual({
      type: "navigate",
      delta: -1,
    });
    expect(resolvePanelKey(ctx({ key: "ArrowRight", boardStyle: "vertical" })).type).toBe(
      "ignore",
    );
  });

  it("搜索框有内容时方向键是移动光标", () => {
    expect(
      resolvePanelKey(ctx({ key: "ArrowRight", isSearch: true, isTyping: true })).type,
    ).toBe("ignore");
  });

  it("搜索框为空时 ↑↓ 可以浏览列表（唤出即搜的核心心流）", () => {
    expect(
      resolvePanelKey(
        ctx({ key: "ArrowDown", isSearch: true, isTyping: true, searchEmpty: true }),
      ),
    ).toEqual({ type: "navigate", delta: 1 });
    expect(
      resolvePanelKey(
        ctx({ key: "ArrowUp", isSearch: true, isTyping: true, searchEmpty: true }),
      ),
    ).toEqual({ type: "navigate", delta: -1 });
  });

  it("无关按键不动作", () => {
    expect(resolvePanelKey(ctx({ key: "x" })).type).toBe("ignore");
  });
});
