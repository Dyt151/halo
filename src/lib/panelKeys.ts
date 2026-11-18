/**
 * 面板键盘状态机（评审 W7）。
 *
 * 把 `Panel.tsx` 里那段内嵌在 `keydown` 监听里的「按键 → 动作」决策抽成
 * 纯函数：面板是 hide 而非销毁，键盘导航的状态机是它最容易在改动时悄悄
 * 失配的部分（Esc 收起 / Enter 回贴 / ⌘1~9 / 方向键 / ⌘C / ⌘D / ⌘⌫），
 * 抽出来才能对每一分支做确定性验证。
 *
 * 纯函数：不做任何副作用，只回答「这一击应该触发什么」。DOM 相关的事实
 * （焦点在搜索框、目标在卡片内、是否有文字选区、是否有对话框）由调用方
 * 在传入前换算成布尔，保持本模块零依赖。
 */

/** 面板键盘决策所需的现场事实（由组件从 DOM / store 换算后传入）。 */
export interface PanelKeyContext {
  /** `e.key`（原样）。 */
  key: string;
  /** `e.metaKey || e.ctrlKey`。 */
  metaOrCtrl: boolean;
  /** 中文输入法组合中 / keyCode 229：一律不当作快捷键。 */
  composing: boolean;
  /** 焦点是否在面板搜索框上（搜索框聚焦时 ⌘1~9 / Enter 仍生效）。 */
  isSearch: boolean;
  /** 搜索框当前是否为空（空时方向键才用于切换条目）。 */
  searchEmpty: boolean;
  /** 目标是否为任意可编辑控件（input/textarea/select/contenteditable，含搜索框）。 */
  isTyping: boolean;
  /** 目标是否落在某张卡片内（卡片自己处理 Enter，这里不再重复）。 */
  targetInClipCard: boolean;
  /** 是否有应用内对话框正在展示（对话框打开时键盘归它）。 */
  dialogOpen: boolean;
  /** 用户是否手动选中了文字（⌘C 应复制选中文字而非整条记录）。 */
  hasTextSelection: boolean;
  /** 是否有筛选/搜索/标签条件在生效（有筛选时 ⌘N 角标语义失效）。 */
  hasFilter: boolean;
  /** 配置：是否启用面板内 ⌘1~9 回贴。 */
  numHotkeyEnabled: boolean;
  /** 面板排列方式（非 "vertical" 一律按横向处理，与组件一致）。 */
  boardStyle: string;
  /** 当前列表条数。 */
  itemCount: number;
  /** 当前选中下标（组件已保证在合法范围内）。 */
  selected: number;
}

/** 决策结果：组件据此执行副作用。`ignore` 表示本次按键不拦截、不动作。 */
export type PanelKeyAction =
  | { type: "ignore" }
  | { type: "close" }
  | { type: "focusSearch" }
  /** 搜索框内有内容时按 Esc：先清空，不收起面板（macOS 惯例）。 */
  | { type: "clearSearch" }
  /** ⌘N 回贴第 N 条。 */
  | { type: "pasteIndex"; index: number }
  /** 搜索框内 Enter：先落地防抖查询，再回贴第一条。 */
  | { type: "pasteSearch" }
  /** 非搜索框 Enter：回贴当前选中。 */
  | { type: "pasteSelected" }
  | { type: "copy" }
  | { type: "toggleFavorite" }
  | { type: "delete" }
  | { type: "navigate"; delta: 1 | -1 };

/** 当前是否有可操作的选中项（itemCount > 0 时 selected 恒合法）。 */
function hasCurrent(ctx: PanelKeyContext): boolean {
  return ctx.itemCount > 0;
}

/**
 * 按键 → 动作决策。分支顺序与原 `onKey` 完全一致：
 * 输入法保护 → Esc → ⌘F → 其它输入控件透传 → 对话框屏蔽 → ⌘1~9 →
 * Enter → ⌘C → ⌘D → ⌘⌫ → 方向键。
 */
export function resolvePanelKey(ctx: PanelKeyContext): PanelKeyAction {
  // 中文输入法用 Enter 上屏候选词：这一下也带 keydown，绝不能当成「回贴」。
  if (ctx.composing) return { type: "ignore" };

  // Esc 分级：搜索框里有内容时先清空（保留面板），空搜索框或焦点在
  // 列表上时才收起。macOS 的查找栏就是这个语义。
  if (ctx.key === "Escape") {
    if (ctx.isSearch && !ctx.searchEmpty) return { type: "clearSearch" };
    return { type: "close" };
  }

  if (ctx.metaOrCtrl && ctx.key.toLowerCase() === "f") return { type: "focusSearch" };

  // 其它输入型控件（对话框输入框、模板编辑框）整套透传；搜索框除外。
  if (ctx.isTyping && !ctx.isSearch) return { type: "ignore" };

  // 对话框打开时键盘归它（DialogHost 只吃 Esc/Enter，挡不住 ⌘1~9）。
  if (ctx.dialogOpen) return { type: "ignore" };

  // ⌘1~9 只在面板内生效、且无筛选时才有 ⌘N 语义。
  if (ctx.metaOrCtrl && /^[1-9]$/.test(ctx.key)) {
    if (!ctx.numHotkeyEnabled || ctx.hasFilter) return { type: "ignore" };
    const index = Number(ctx.key) - 1;
    if (index >= ctx.itemCount) return { type: "ignore" };
    return { type: "pasteIndex", index };
  }

  if (ctx.key === "Enter") {
    // 卡片自己处理 Enter（并从卡片内 stopPropagation），这里不重复回贴。
    if (ctx.targetInClipCard) return { type: "ignore" };
    // 搜索框内 Enter：防抖查询可能还没落地，须先 flush 再取第一条。
    if (ctx.isSearch) return { type: "pasteSearch" };
    if (!hasCurrent(ctx)) return { type: "ignore" };
    return { type: "pasteSelected" };
  }

  if (ctx.metaOrCtrl && ctx.key.toLowerCase() === "c") {
    if (!hasCurrent(ctx)) return { type: "ignore" };
    // 手动选中文字时按 ⌘C，应当复制选中的文字而不是整条记录。
    if (ctx.hasTextSelection) return { type: "ignore" };
    return { type: "copy" };
  }

  if (ctx.metaOrCtrl && ctx.key.toLowerCase() === "d") {
    if (!hasCurrent(ctx)) return { type: "ignore" };
    return { type: "toggleFavorite" };
  }

  if (ctx.key === "Backspace" && ctx.metaOrCtrl) {
    // 搜索框里的 ⌘⌫ 是系统「删到行首」，不能被当成删除条目。
    if (ctx.isTyping || !hasCurrent(ctx)) return { type: "ignore" };
    return { type: "delete" };
  }

  // 方向键：搜索框有内容时是移动光标；搜索框为空时用于切换条目。
  // 这正是面板「唤出即聚焦搜索」后仍然能用方向键浏览的原因（评审 U-01）。
  if (ctx.isTyping && !ctx.isSearch) return { type: "ignore" };
  if (ctx.isSearch && !ctx.searchEmpty) return { type: "ignore" };

  const vertical = ctx.boardStyle === "vertical" || ctx.boardStyle === "list";
  const delta =
    vertical
      ? ctx.key === "ArrowDown"
        ? 1
        : ctx.key === "ArrowUp"
          ? -1
          : 0
      : ctx.key === "ArrowRight"
        ? 1
        : ctx.key === "ArrowLeft"
          ? -1
          : 0;
  if (delta !== 0) return { type: "navigate", delta };

  return { type: "ignore" };
}
