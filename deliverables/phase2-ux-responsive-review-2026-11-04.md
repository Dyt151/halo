# Halo 主界面 · 可用性与响应式审查（Phase 2）

> 审查者：ux-ui-designer｜日期：2026-11-04｜工作区：`/Users/dayiteng/Documents/codex/小发明/halo`
> 范围：桌面 Tauri 应用「主界面整体」。**本阶段的「响应式」= 窗口尺寸变化下的布局自适应**（非移动端断点）。
> 本报告只做审查，未修改任何文件。所有像素值均由 Rust 侧真实约束 + CSS 类推演得出，推演前提已在文中写明。

---

## 0. 数据来源与窗口尺寸硬约束（Rust 侧实测）

### 0.1 窗口声明（`src-tauri/tauri.conf.json`）

| 窗口 | label | 默认宽×高 | 最小宽×高 | 可缩放 | 行号 |
|---|---|---|---|---|---|
| 管理窗口 | `main` | 900 × 640 | **420 × 360** | 是 | 14–34（宽 900@17、高 640@18、最小宽 420@19、最小高 360@20） |
| 剪贴板面板 | `panel` | 900 × 220 | **320 × 220** | 否 | 35–50（宽 900@38、高 220@39、最小宽 320@40、最小高 220@41） |

补充事实：
- `main` 注册了 `tauri-plugin-window-state`（`src-tauri/src/lib.rs:197–204`，`StateFlags::all()` 且去掉 `VISIBLE`）→ **管理窗口的尺寸会被持久化**：用户某次把窗口拖到 420px 宽，这个宽度会被记住，下次启动依然如此。
- `main` 的 `titleBarStyle: "Overlay"` + `hiddenTitle: true` + `trafficLightPosition {x:18,y:20}`（tauri.conf.json:25–27）→ 内容从窗口最顶端开始，红绿灯叠在内容之上（`MainWindow.tsx:249` 用 `pt-[36px]` 让位）。
- `panel` 的 `resizable: false`，所有尺寸变化都来自 Rust 的 `set_size`。

### 0.2 面板的运行时尺寸算法（`src-tauri/src/window.rs:1033–1097`）

```rust
// window.rs:1056-1060
let max_height = (avail_height - PANEL_MARGIN * 2.0).max(220.0);      // PANEL_MARGIN = 24.0 (window.rs:35)
let height = (avail_height * config.board_height_scale.clamp(10, 60) as f64 / 100.0)
    .round().max(220.0).min(max_height);

// window.rs:1070 / 1092-1097
"bottom" => (..., screen_width)          // 贴底停靠 = 整个屏幕物理宽度
fn panel_width(avail_width) -> f64 {
    let usable = (avail_width - PANEL_MARGIN * 2.0).max(240.0);
    (avail_width * 0.6).clamp(240.0, PANEL_MAX_WIDTH /*1180*/).min(usable)
}
```

因此面板的**真实尺寸区间**（`avail_*` = 显示器工作区，已扣掉菜单栏/程序坞）：

| 量 | 公式 | 最小 | 最大 |
|---|---|---|---|
| 面板高 | `clamp(220, avail_h × scale%, avail_h − 48)` | **220** | `avail_h − 48`（scale 上限 60%） |
| 面板宽（贴底） | `screen_width` | 屏幕宽 | 屏幕宽 |
| 面板宽（居中/跟随光标） | `clamp(240, avail_w × 0.6, 1180)` 且 ≤ `avail_w − 48` | **240** | 1180 |

具体到常见屏幕（`avail_h` 已扣菜单栏 25pt）：

| 屏幕（逻辑点） | avail_w × avail_h | 面板高 @30%（默认） | 面板高 @20% | 面板高 @60% | 面板宽（居中模式） |
|---|---|---|---|---|---|
| 13" MBP（默认 1280×800） | 1280 × 775 | 233 | **220**（被 220 下限吃掉） | 465 | 768 |
| 14" MBP（1512×982） | 1512 × 957 | 287 | 220 | 574 | 907 |
| 16" MBP（1728×1117） | 1728 × 1092 | 328 | 220 | 655 | 1037 |
| 1920×1080 外接 | 1920 × 1055 | 317 | 220 | 633 | 1152 |
| 2560×1440 外接 | 2560 × 1415 | 425 | 283 | 849 | 1180（封顶） |
| 13" MBP + 150% 缩放 | 1008 × 630 | 189→**220** | **220** | 378 | **605** |

### 0.3 面板内部固定高度（决定「可用高度」）

`Panel.tsx`：工具栏 `h-[44px] shrink-0`（460）→ 标签行 `h-[34px] shrink-0`（`TagBar.tsx:26`）→ 卡片流 `flex-1`（`VirtualList.tsx:53`）→ 状态栏 `h-[36px] shrink-0`（`StatusBar.tsx:48`）。
**固定 chrome = 44 + 34 + 36 = 114px**（无标签时 80px）。
→ 卡片流可视高度 = 面板高 − 114；横向排布时卡片高度 = 该值 − 上下 `py-[12px]`（`VirtualList.tsx:62`）= 面板高 − 138。
→ 卡片自身固定头部 28px + 底部 30px（`ClipItem.tsx:243`/`:271`）→ **预览区高度 = 卡片高 − 58**。

| 面板高 | 卡片流高 | 卡片高（横向） | **预览区高** | 可显示行数（12px/1.5 = 18px 行高） |
|---|---|---|---|---|
| 220（下限） | 106 | 82 | **24px** | **1.3 行** |
| 233 | 119 | 95 | 37px | 2.0 行 |
| 287（14" 默认 30%） | 173 | 149 | 91px | 5.0 行 |
| 328（16" 默认 30%） | 214 | 190 | 132px | 7.3 行 |
| 425 | 311 | 287 | 229px | 9 行（`line-clamp-[9]` 封顶） |
| 655（16" @60%） | 541 | 517 | 459px | 9 行 → **259px 空白** |

### 0.4 管理窗口的内容区宽度

侧边栏 `w-[200px] shrink-0`（`MainWindow.tsx:248`）→ 内容区 = 窗口宽 − 200。

| 窗口宽 | 内容区 | 减去 `p-[24px]`×2 | 历史页网格 inner（再减 padding 24×2） | 列数（minColumnWidth 200） | 列宽 |
|---|---|---|---|---|---|
| **420（最小）** | **220** | 172 | **124** | 1（`max(1, floor(136/212))`=1） | **124px** |
| 500 | 300 | 252 | 204 | 1 | 204px |
| 660 | 460 | 412 | 364 | 2 | 200px |
| 664 | 464 | 416 | 368 | 2 | 202px |
| 668 | 468 | 420 | 372 | 1 | **372px**（列数突变） |
| 900（默认） | 700 | 652 | 604 | 2 | 296px |
| 1200 | 1000 | 952 | 904 | 4 | 229px |
| 2000 | 1800 | 1752 | 1704 | 8 | 208.5px |

列宽公式：`VirtualList.tsx:107-109` → `columns = max(1, floor((inner+12)/(200+12)))`，行内 `repeat(columns, minmax(0,1fr))`（136）。

---

## 1. 可用性问题（U）

### U-01 🔴 面板方向键导航在「默认状态」下完全不可达，与首启引导文案自相矛盾

- **现象**：面板每次唤出都强制把焦点放进搜索框（`Panel.tsx:140-143` 的 `useLayoutEffect` 依赖 `shownKey`；`handleShown` 里再 `focusSearch()`（`:174`））。而键盘状态机在 `isTyping` 为真时直接 `ignore` 方向键（`panelKeys.ts:120`），方向键因此被输入框吃掉去移动光标。用户在面板出现的**那一刻**按 ←/→ 不会有任何反应。
- **证据**：`src/windows/Panel.tsx:140-143`、`:174`；`src/lib/panelKeys.ts:119-120`（`if (ctx.isTyping) return { type: "ignore" }` 位于方向键分支之前）；`src/components/FirstRunGuide.tsx:83` 明确写着 `← →  选择条目（纵向排列时用 ↑ ↓）`。
- **对用户的影响**：首启引导教的第一个操作就无效。用户按 ←/→ 没反应 → 以为面板坏了 → 去点卡片（单击默认是**复制**，双击才回贴，见 `ClipItem.tsx:206-225`）→ 内容被复制走却没贴出去。要用方向键，必须先按 Tab 把焦点从搜索框挪走，而 Tab 走到的第一个可聚焦元素是分段控件的选中项（roving tabindex，`ui.tsx:158`），再 Tab 一次才到卡片——全程没有任何视觉提示告诉用户焦点在哪。
- **建议改法**：在 `panelKeys.ts` 增加「搜索框为空时方向键归列表」的规则，并把 `isTyping` 判断下移到方向键分支之后：
  ```ts
  // panelKeys.ts：新增 ctx.searchEmpty
  if (ctx.isTyping && !ctx.searchEmpty) return { type: "ignore" };
  ...
  if (ctx.isTyping && !ctx.isSearch) return { type: "ignore" };  // 仅非搜索输入框
  ```
  `Panel.tsx` 传入 `searchEmpty: !keyword.trim()`。同时把 `FirstRunGuide.tsx:83` 的文案改成「← → 选择条目（搜索框为空时可直接用，输入文字后需先按 Esc 或 Tab 离开搜索框）」，并在搜索框右侧加一个「↓ 进入列表」的提示或把 ↓ 映射为「焦点移到卡片列表」。

### U-02 🔴 破坏性确认框里按 Enter = 执行破坏性操作（与焦点所在的「取消」相反）

- **现象**：`DialogHost` 注册了一个 window 级 **capture 阶段** keydown，只要对话框打开且 kind 不是 prompt，任意 Enter 都会 `closeDialog(true)`（确认）。但对话框打开时**默认焦点落在「取消」按钮上**（`DialogHost.tsx:53`）。于是：焦点明明在「取消」上，Enter 却执行「确定」。
- **证据**：`src/components/DialogHost.tsx:28-47`（`:38-42` 为 Enter → `closeDialog(true)`）；`:50-55`（默认焦点 → `cancelRef.current?.focus()`）；`:124-133`（取消按钮）。
- **对用户的影响**：删除记录、清空历史、退出 Halo 的确认框里，用户看到焦点框在「取消」上，放心按下 Enter —— 结果**记录被删除 / 历史被清空**。这正好命中「删除前确认」这个防错机制最关键的一步，破坏性最高的一类操作反而最容易被误触发。清空历史的确认文案还写着「此操作无法撤销」。
- **建议改法**：删掉全局 Enter 处理，改由按钮自身的激活行为决定；或只在焦点确实落在确认键上时才拦截：
  ```tsx
  // DialogHost.tsx：替换 :38-42
  if (e.key === "Enter" && dialog.kind !== "prompt"
      && document.activeElement !== cancelRef.current) { ... }
  ```
  更稳妥的做法是：确认框内禁用「回车即确定」，让 Tab → 确认键 → Space 成为唯一路径，并在文案上补一句「按 Esc 取消」。

### U-03 🔴 卡片「选中态」既不可见（浅色）也不被辅助技术感知 —— 面板主心流被切断

- **现象**：面板的「唤出→方向键→回贴」是一条键盘心流，但选中项只有 CSS 类 `card-clip-selected`（`ClipItem.tsx:238`），**没有任何 `aria-selected` / `aria-current`**，DOM 里也没有 `role="option"`；列表容器也没有 `role="listbox"`。同时浅色模式下该选中态只有 2.16:1 的描边（详见 Phase 1 复核 V-04）。
- **证据**：`src/components/ClipItem.tsx:200-204`（`role="group" tabIndex={0}`，无 `aria-selected`）、`:238`（唯一的状态表达）；`src/components/VirtualList.tsx:53`/`:124`（列表容器是普通 div）；`src/index.css:306-312` 与 `:742-746`（浅色选中态仅 `border-color: var(--accent-500)`，而 `:root.light`（`:85-126`）**没有覆盖 `--accent-500`**，仍是 `#e8a33d`）。
- **对用户的影响**：① 视觉用户：按下 ↓ 之后看不到高亮移动（浅色），无法确认当前要回贴哪一条，只能靠再次 Enter 前的猜测；② 读屏用户：完全听不到「当前选中第 N 条」，方向键导航对他们等于不存在。
- **建议改法**：
  1. 浅色选中态换用深一档的描边色（与 `.light [class~="text-accent-500"]`→`#8a5816` 的做法对齐），在 `:root.light` 里加 `--accent-500: #8a5816;` 或给 `.light .card-clip-selected` 单独指定 `border-color: #8a5816`；
  2. 卡片加 `aria-selected={selected}`，列表容器加 `role="listbox"` + `aria-activedescendant`，或退一步把卡片改成 `role="option"`（`ClipItem.tsx:201` 现在是 `role="group"`）；
  3. `aria-label` 改为「内容 + 类型 + 时间」而不是动作短语（见 U-06）。

### U-04 🔴 管理窗口侧边栏导航项**没有任何焦点指示**，键盘用户无法操作设置中心

- **现象**：9 个导航按钮都写了 `outline-none`，但 `.light`/深色两套样式里**都没有针对它们的 focus-visible 规则**，也没有 fallback 的 `outline` 恢复。焦点落到导航项上时，视觉上完全没有任何变化。
- **证据**：`src/windows/MainWindow.tsx:281-297`（`:288` 类名含 `outline-none`）；`src/index.css` 中所有 `focus-visible` 规则只覆盖 `.btn`(183)、`.btn-icon`(207)、`.seg-item`(221)、`.pill`(231)、`.switch`(245)、`.input`(173)、`.slider`(281-284) —— **没有任何一条覆盖裸 `<button>`**。
- **对用户的影响**：管理窗口打开后没有任何元素自动获得焦点，用户按 Tab 进入侧边栏，第一项是「通用」—— 但屏幕上什么都看不到，用户不知道自己在哪，只能用鼠标点。配合 `titleBarStyle: Overlay`（无系统标题栏），整个设置中心对键盘用户等于不可用。
- **建议改法**：在 `index.css` 补一条全局兜底（对所有可聚焦元素生效，且用 `:focus-visible` 避免鼠标点击也出环）：
  ```css
  :where(button, a, input, select, textarea, [tabindex]):focus-visible {
    outline: 2px solid var(--hairline-focus);
    outline-offset: 2px;
    border-radius: 6px;
  }
  ```
  并把 `MainWindow.tsx:288`/`:308` 的 `outline-none` 去掉（或改用 `focus-visible:outline-none` 只在非键盘聚焦时抑制）。另外主窗口打开时应把焦点移到当前分区（`MainWindow.tsx:191-198` 的 `setSection` 里 `sectionRef.current?.focus()`）。

### U-05 🔴 卡片内的 4 个操作按钮（查看全文 / 收藏 / 复制 / 删除）无焦点指示

- **现象**：`CardIconBtn` 带 `outline-none`，且没有任何 focus-visible 规则；`index.css` 的 focus 规则也不含卡片内按钮。
- **证据**：`src/components/ui.tsx:74-92`（`:80` 的 `outline-none`）；`src/index.css` 无对应 focus 规则（仅 173/183/207/221/231/245/263/281）。
- **对用户的影响**：键盘用户 Tab 进卡片后，按 Tab 会依次停在 4 个 24×24 的图标按钮上，但**屏幕上没有任何提示**，用户既不知道停在哪、也不知道那个图标是什么（13px 的纯 SVG，无文字）。这 4 个按钮是「复制 / 删除」这类高危操作的入口。
- **建议改法**：给 `CardIconBtn` 补 `focus-visible:ring-2 focus-visible:ring-accent-500 focus-visible:ring-offset-1 focus-visible:ring-offset-transparent`（或复用 U-04 的全局 `:focus-visible` 规则），并给这 4 个按钮补 `aria-label`（现在只有 `title`，虽然 title 可作为无障碍名，但图标形状对读屏用户无意义，建议写成「复制此条」「删除此条」）。

### U-06 🟠 历史页没有任何列表级键盘导航，只能靠 Tab 逐张卡片遍历

- **现象**：历史页没有 window 级方向键处理（只有多选态的 Esc，`HistoryPage.tsx:91-98`），也没有 ⌘F 聚焦搜索、⌘↓/⌘↑ 跳转、Home/End 跳到首尾。每张卡片 `tabIndex={0}`（`ClipItem.tsx:202`），卡片内部还有 4 个按钮（`ClipItem.tsx:287-369`）。
- **证据**：`src/pages/HistoryPage.tsx` 全文无方向键处理；`src/components/ClipItem.tsx:201-202`（`tabIndex={0}`）、`:287-369`（4 个 `CardIconBtn`）。
- **对用户的影响**：因为用了虚拟化，**Tab 遍历的并不是数百张**（这一点 Phase 1 的疑问可以排除：横向 `VirtualStrip` overscan 4、纵向 `VirtualGrid` overscan 2，DOM 里只有视口附近的十几张）。真正的代价是：到达第 5 张卡片要按 1 + 4×4 + 1 = 18 次 Tab；且每张卡片的 4 个按钮都在 Tab 序列里，其中「删除」与「复制」只隔 6px（`ClipItem.tsx:349`），**键盘用户连「删除」和「复制」都难以区分**，一次误按 Tab + Space 就是删除。
- **建议改法**：给历史页加与面板同源的列表键盘模型（↑/↓ 移动选中、Home/End 跳首尾、⌘F 聚焦搜索、Delete 删除、Space 打开全文），并给卡片列表实现 **roving tabindex**：列表容器 `tabIndex={0}`，只有当前选中卡片 `tabIndex={0}`、其余 `-1`；卡片内的 4 个按钮改为**只在卡片被键盘选中时才进入 Tab 序列**（或统一改为「卡片内按钮用 roving + 方向键进入」）。roving tabindex 目前**在列表层完全缺失**（`Segment` 的 `ui.tsx:158` 有，卡片列表没有）。

### U-07 🟠 历史页多选模式下每张卡片产生 2 个 Tab 停靠点 + 4 个死按钮

- **现象**：多选态下，外层 `div role="checkbox" tabIndex={0}`（`HistoryPage.tsx:152-154`）与内层 `ClipItemCard` 的 `article tabIndex={0}`（`ClipItem.tsx:202`）**同时可聚焦** → 每张卡片 2 个 Tab 停靠点。内层卡片的 4 个操作按钮被传成空回调（`onCopy={() => {}}` / `onDelete={() => {}}` / `onToggleFavorite={() => {}}`，`HistoryPage.tsx:170-172`），但按钮**仍然可见、仍在 Tab 序列里**，而且因为外层是 `onClickCapture` + `stopPropagation`（`:143-147`），点它们只会切换勾选，行为与图标语义完全相反。
- **证据**：`src/pages/HistoryPage.tsx:139-174`（`:154` `tabIndex={0}`、`:156-161` 空格/回车切换勾选、`:169-173` 四个空回调）；`src/components/ClipItem.tsx:202`。
- **对用户的影响**：进入多选态后，键盘用户每张卡片要 Tab 两次才能跳过；看到的「收藏 / 复制 / 删除」图标点了没反应（只是勾选），容易误判为应用坏了。
- **建议改法**：多选态下把内层 `ClipItemCard` 的 `tabIndex` 强制为 -1（给 `ClipItem` 加 `tabbable?: boolean` prop），并隐藏内层的操作按钮（多选态不需要「复制/删除」，批量工具条已经有）；外层 checkbox 保留唯一的 `tabIndex={0}` + roving。

### U-08 🟠 Esc 在搜索框里直接收起整个面板（不先清空搜索词）

- **现象**：状态机把 Esc 放在**所有输入判断之前**，无条件返回 `close`（`panelKeys.ts:74`），注释只说「Esc 收起面板」，但 `isTyping` 的放行分支在它之后（`:79`）。用户在搜索框里输入了半截词，想清空 → 按 Esc → **整个面板消失**，这半截输入也一起没了。
- **证据**：`src/lib/panelKeys.ts:70-79`（顺序：composing → **Escape→close** → ⌘F → isTyping 放行）；`src/windows/Panel.tsx:230-233`（close → `closePanel()`）。
- **对用户的影响**：macOS 通用约定是「Esc 先清空搜索框，再按才关闭」。Halo 的行为跳过了第一级，用户按 Esc 的预期落空，还连带丢掉了已输入的关键词与筛选状态。
- **建议改法**：在 `panelKeys.ts` 中把 Esc 分级：
  ```ts
  if (ctx.key === "Escape") {
    if (ctx.isSearch && ctx.searchNotEmpty) return { type: "clearSearch" };
    return { type: "close" };
  }
  ```
  `Panel.tsx` 增加 `clearSearch` 分支调 `setKeyword("")`。

### U-09 🟠 对话框关闭后不归还焦点，破坏「唤出即聚焦搜索框」

- **现象**：`DialogHost` 打开时把焦点移到按钮上，关闭时**没有把焦点还给触发元素**（对比 `ClipDetail.tsx:33-82` 做了 `previouslyFocused?.focus?.()`）。对话框卸载后焦点落到 `<body>`。
- **证据**：`src/components/DialogHost.tsx:50-55`（有初始焦点、无归还）；对照 `src/components/ClipDetail.tsx:36`/`:80`（有归还）。
- **对用户的影响**：面板里按 ⌘⌫ 删除 → 弹确认框 → 确定 → **搜索框失去焦点**。用户接着想继续打字筛选，结果按键全部无效（面板的键盘处理依赖 `e.target === searchRef.current` 来区分「在输入」与「在列表」），必须先用鼠标点一下搜索框。「唤出即搜」这条主心流在一次删除之后就被打断了。
- **建议改法**：`DialogHost` 记录打开前的 `document.activeElement`，在 `closeDialog` 后的 cleanup 里 `requestAnimationFrame(() => prev?.focus())`；面板场景下应显式 `focusSearch()`（因为面板的搜索框是「唤出即聚焦」的锚点）。

### U-10 🟠 首启引导是模态框，但没有焦点移入、没有焦点陷阱、没有焦点归还

- **现象**：`FirstRunGuide` 声明了 `role="dialog" aria-modal="true"`（`:70-72`），但**没有任何焦点处理**：不移动焦点、不困住 Tab、关闭后不归还。遮罩是 `absolute inset-0`（`:63`），背景的侧边栏按钮与设置控件仍然可以被 Tab 到。
- **证据**：`src/components/FirstRunGuide.tsx:61-113`（全文只有 `:41-51` 的 Esc 监听，无 `focus()`、无 trapTab）；对照 `src/components/DialogHost.tsx:67-83`（有 `trapTab`）、`src/components/ClipDetail.tsx:51-75`（有 Tab 陷阱）。
- **对用户的影响**：新用户第一次打开设置中心时，Tab 会从引导层「穿过去」聚焦到背景里的侧边栏与开关上；读屏用户听到的是「欢迎使用 Halo」的 dialog 宣告，但焦点其实在别处，背景内容也被读屏器遍历。引导层是**唯一一次**教育用户「怎么用」的机会（`:82-86` 列了快捷键），此时焦点不可控。
- **建议改法**：复用 `ClipDetail` 的焦点三件套（移入 / Tab 陷阱 / 归还），或把引导层改成走 `DialogHost` 的队列（`lib/dialog.ts` 已有 FIFO 队列与 `alert` 类型），统一获得焦点管理。

### U-11 🟠 撤销入口只存活 2.4 秒，确认文案却承诺「一键撤销」

- **现象**：删除后的 toast 带「撤销」按钮，但普通 tone 的默认时长是 **2400ms**（`toast.ts:75`），danger 才 4500ms。删除用的是默认 tone（`useClipStore.ts:351-354` 未指定 tone）→ 2.4 秒后按钮消失。而批量删除的确认文案明确写着「删除后可在提示条里一键撤销」（`HistoryPage.tsx:107`）。
- **证据**：`src/lib/toast.ts:75`（`const duration = options?.duration ?? (tone === "danger" ? 4500 : 2400)`）；`src/store/useClipStore.ts:346-359`；`src/pages/HistoryPage.tsx:107`。
- **对用户的影响**：用户点完「删除」后视线通常还在刚删掉的那张卡片位置，而撤销按钮在**屏幕底部**（管理窗口右下 `ToastHost.tsx:25`，面板底部居中 `bottom-[44px]`），且只存在 2.4 秒 —— 绝大多数情况下用户根本没看见，误删就变成了不可逆。`confirm_on_delete` 关闭时更糟：删除完全无提示、无撤销。
- **建议改法**：删除类 toast 固定 8–10 秒（`toast("已删除 1 条记录", { actionLabel: "撤销", duration: 8000 })`），并把 toast 在管理窗口里移到**操作发生处附近**而不是右下角；或者用常驻的撤销条（类似 Finder 的「已移到废纸篓 · 撤销」）。

### U-12 🟠 右键菜单只支持鼠标，键盘用户完全够不到

- **现象**：菜单有 `role="menu"` / `role="menuitem"`（语义正确），但**没有任何键盘行为**：打开时不移焦点、不支持 ↑/↓ 在菜单项间移动、不支持 Enter 激活（靠 Tab，而菜单渲染在 DOM 末尾，Tab 要先穿过所有卡片）。
- **证据**：`src/components/ClipContextMenu.tsx:100-113`（只做定位与边界钳制）、`:115-132`（只处理 Esc）、`:141-160`（`item()` 只渲染 button）、`:214-240`（子菜单靠 `onMouseEnter` 展开，`:196-197`）。
- **对用户的影响**：读屏用户右键（或用菜单键）弹出菜单后，焦点仍留在卡片上；继续按方向键 → 走的是**面板的卡片导航**（`panelKeys.ts:122-134`），菜单形同虚设；键盘用户无法「置顶 / 收藏 / 加入堆栈 / 标签 / 模块动作」这些只存在于右键菜单里的能力。
- **建议改法**：打开菜单时把焦点移到第一个菜单项，实现 ↑/↓/Home/End 循环、Esc 逐层退出（已有）、Enter 激活；子菜单用 ↑/→ 展开。菜单项加 roving tabindex（当前项 0，其余 -1）。

### U-13 🟠 加载 / 空 / 错误三态都没有播报，错误文案还被硬裁到 48px

- **现象**：
  1. `Spinner` 带 `aria-hidden` 且没有伴随文本 → 读屏完全不知道正在加载（`ui.tsx:600-619`，`Panel.tsx:379-382`、`HistoryPage.tsx:292-294`）。
  2. 空状态与错误状态没有 `role="status"` / `aria-live` → 从「加载中」切到「暂无历史」不会被播报。
  3. 错误文案用 `max-h-[48px] overflow-hidden` **硬裁**（`Panel.tsx:391`、`HistoryPage.tsx:299`），没有 tooltip、没有「复制」、也没有换行可滚动 —— 超过 3 行（48px / 16px 行高）的错误原因**永久看不到**。
- **证据**：`src/components/ui.tsx:600-619`（`:608` `aria-hidden`）、`:592-594`（hint `text-white/[0.28]`，2.51:1）；`src/windows/Panel.tsx:379-382`、`:391`；`src/pages/HistoryPage.tsx:292-294`、`:299`。
- **对用户的影响**：后端失败时，用户看到「历史加载失败」和一个被截断的半句错误，既不知道原因（无法自助），也不能复制给开发者。唯一能走的是「重试」和「打开设置」，但错误原因往往正是「去设置里开某项权限」的关键提示。
- **建议改法**：错误块改成 `max-h-[96px] overflow-y-auto` + `break-words` + `title={error}`（tooltip）+ 一枚「复制错误详情」按钮；把「重试/打开设置」三个按钮保留（现在有）；Spinner 外包一层 `role="status" aria-live="polite"` 并写「正在加载…」；空/错误容器加 `role="status"`。注意 `EmptyState` 的 hint（`text-white/[0.28]`）是「可行动指引」的载体，浅色下 2.51:1 建议同步提到 ≥4.5:1（Phase 1 V-08）。

### U-14 🟠 滑块没有 `aria-valuetext`，读屏读出裸数字

- **现象**：`SliderRow` 的 `<input type="range">` 只有 `aria-label`，没有 `aria-valuetext`。面板高度滑块的可见值是「30%」，但滑块的 value 是 30；磁盘上限滑块可见值是「200 MB / 2 GB」，value 是 MB 数值。
- **证据**：`src/components/ui.tsx:355-364`（`:363` 仅 `aria-label={label}`）；`src/pages/Settings.tsx:331-339`（面板高度，format `${v}%`）、`:413-421`（磁盘上限，format `formatMb`）。
- **对用户的影响**：读屏用户听到「面板高度，滑块，30」而不是「30%」；磁盘上限听到「5120」而不是「5 GB」。用户对当前设置的实际取值产生误判。
- **建议改法**：给 `SliderRow` 增加 `valueText` prop 并透传 `aria-valuetext={valueText ?? format(value)}`；`SliderRow` 内部直接 `aria-valuetext={format(value)}` 即可覆盖两处。

### U-15 🟡 ⌘C 会吞掉搜索框里的文字选区

- **现象**：`resolvePanelKey` 用 `window.getSelection()?.toString()` 判断「用户是否选中了文字」，但 `<input>` 里的选区**不出现在 document selection 里**，所以搜索框里选中一段文字按 ⌘C 时 `hasTextSelection` 为 false → 走「复制整条记录」。
- **证据**：`src/lib/panelKeys.ts:101-106`；`src/windows/Panel.tsx:219`（`hasTextSelection: !!window.getSelection()?.toString()`）。
- **对用户的影响**：用户从搜索框里复制一段关键词去别处粘贴，结果剪贴板里变成了一整条剪贴板记录。
- **建议改法**：把 `isSearch` 提前判断 —— 焦点在搜索框时，⌘C 一律交给浏览器（`if (ctx.isSearch) return { type: "ignore" }` 放到 ⌘C 分支最前）。

### U-16 🟡 「监控已暂停」徽标在浅色下几乎不可见 —— 用户以为工具坏了

- **现象**：徽标的**文字**走 `text-warning`，浅色下被 `index.css:617` 映射成 `var(--warning)`（浅色 `#9a6b00`），可读；但徽标的**底色** `bg-warning/[0.14]`（`StatusBar.tsx:54`）和 5px 状态点 `bg-warning`（`:57`）是 Tailwind 静态色 `#FFD60A`，**浅色下没有任何 remap 规则** → 浅黄点落在近白材质上约 1.35:1，几乎不可见。同类问题在 `PermissionCenter.tsx:30` 与 `ContextMenuSettings.tsx:124/149`（`bg-success-soft` / `bg-warning` 三态色点）。
- **证据**：`src/components/StatusBar.tsx:54`、`:57`；`src/components/PermissionCenter.tsx:30`；`src/pages/ContextMenuSettings.tsx:124`、`:149`；`tailwind.config.ts:36-37`（`success`/`warning` 是静态 hex，不是变量）；`src/index.css:85-126`（`:root.light` 只覆盖了 `--success`/`--warning` 变量，**没有任何 `bg-warning`/`bg-success-soft` 的类级 remap**）；`src/index.css:616-617`（只有 `text-*` 被 remap）。
- **对用户的影响**：用户关过一次「剪贴板监控」后，浅色模式下状态栏只有一行灰字加一个看不见的点 —— 「为什么复制了东西但面板里没有？」→ 去设置里找开关。信任成本很高。
- **建议改法**：把 `warning`/`success` 改成 `rgb(var(--warning) / <alpha-value>)` 形式（`tailwind.config.ts`），并在 `:root.light` 里把 `--warning` 设为更深的 `#9a6b00`（已是）、`--success` 设为 `#1f8a3b`（已是）；这样 `bg-warning`、`bg-warning/[0.14]`、`bg-success-soft` 会自动跟随变量，无需逐个补类。

### U-17 🟡 拖动面板后，下次唤起会弹回配置位置（与主窗口的尺寸记忆不一致）

- **现象**：面板 header 挂了 `data-tauri-drag-region`（`Panel.tsx:461`），用户可以把面板拖到任意位置；但 `show_panel_window` 每次都会调 `apply_panel_layout`（`window.rs:238-241` → `:1033`），把位置/尺寸重置为配置值。管理窗口则由 `tauri-plugin-window-state` 持久化（`lib.rs:197-204`）。
- **证据**：`src/windows/Panel.tsx:459-462`；`src-tauri/src/window.rs:238-241`、`:1086-1087`；`src-tauri/src/lib.rs:197-204`。
- **对用户的影响**：用户把面板拖到顺手的位置（尤其「跟随光标」模式下很自然会想拖开），下次按快捷键它又弹回老位置 → 像是「面板自己跳回去了」。贴底停靠模式下拖动还会把面板从底部拽走，与「贴底」的承诺矛盾。
- **建议改法**：贴底/居中模式下把 header 的拖拽区去掉（只保留搜索框左侧一小块留白），或把用户拖动的偏移量记进 `config` 并在 `apply_panel_layout` 里叠加。

### U-18 🟡 待验证：面板 header 的拖拽区可能吞掉搜索框/分段控件的点击

- **现象**：`<header data-tauri-drag-region>` 内部直接放了 `SearchField`、`Segment`、两个 `IconBtn`（`Panel.tsx:459-509`）。Tauri v2 的拖拽命中逻辑若使用 `closest('[data-tauri-drag-region]')`，这些可交互子元素也会被当成拖拽区。
- **证据**：`src/windows/Panel.tsx:459-509`。
- **待验证**：需实测「点击面板搜索框能否立即获得焦点并输入」。若不能，则面板的核心心流直接失效（优先级需上调为 🔴）。本轮未取得 Tauri v2 注入脚本的命中规则证据，故标注待验证。

---

## 2. 响应式问题（R）

### R-01 🔴 面板落到最小高度 220px 时，文本卡片的预览区只剩 24px（不足 1.5 行）

- **现象**：Rust 侧面板高度下限是 220px（`window.rs:1059`），而 13" MBP 默认 30% 只有 233px、把滑块拉到 20% 会直接吃到 220px 下限。按 §0.3 的算式：面板高 220 → 卡片流 106 → 卡片高 82 → **预览区 82 − 58 = 24px**。预览文字是 `text-[12px]/[1.5]`（行高 18px，`ClipItem.tsx:141`/`:148`/`:156`），24px 只够 1.3 行，且外层 `py-[4px]`（`ClipItem.tsx:266`）还要再吃掉 8px → **实际可见文字不足 1 行**。
- **证据**：`src-tauri/src/window.rs:1056-1060`；`src/windows/Panel.tsx:460`（44）、`:511`（TagBar 34）、`:535`（StatusBar 36）；`src/components/VirtualList.tsx:62`（`py-[12px]`）；`src/components/ClipItem.tsx:243`（28）、`:266`（预览区）、`:271`（30）。
- **对用户的影响**：在 13" 屏（或任何工作区高度 < 733px 的屏幕）上，面板里的文本条目只剩一条模糊的顶边；用户无法判断这条剪贴板是什么，只能靠双击「盲贴」。而 `board_height_scale` 滑块给用户的反馈是「我调小了为什么内容更看不清」。
- **建议改法**：把 220px 这个下限拆成两个概念：
  1. 面板窗口下限保持 220（Rust 侧物理下限 OK）；
  2. **卡片内容下限**改成自适应 —— 预览区用 `flex-1 min-h-[24px]`（已是）之外，给文本预览加 `line-clamp` 的**自适应**上限，或让卡片整体高度随面板高度走：`h-[calc(100%-24px)]` 之外，把 `line-clamp-[9]` 换成 `line-clamp-[var(--panel-lines)]`，由 JS 根据面板可视高度算出可见行数（1–9）写入 CSS 变量；
  3. 更简单的止血：面板高度 < 260px 时，**隐藏标签行与状态栏的次要字段**，把 114px 的固定 chrome 压到 80px（省 34px ≈ 多 1.5 行）。

### R-02 🔴 纵向板型（board_style=vertical）下，卡片固定 220px 远超面板可用高度，底部操作条整条不可见

- **现象**：`VirtualGrid` 的 `rowHeight={220}` 且渲染 `h-[220px] w-full`（`Panel.tsx:522-523`），但面板在 30% 高度下只有 149px 的卡片流空间（220px 面板时 106px）。**卡片比视口高 71–114px**，卡片底部 30px 的操作条（查看全文/收藏/复制/删除，`ClipItem.tsx:271-371`）整条落在可视区之外。
- **证据**：`src/windows/Panel.tsx:516-523`（`minColumnWidth={168}`、`rowHeight={220}`、`"h-[220px] w-full"`）；`src/components/VirtualList.tsx:114`（`estimateSize: () => rowHeight + gap`）；对比横向分支 `Panel.tsx:526-531`（卡片高度由容器决定，无固定值）。
- **对用户的影响**：用户把「卡片排列」改成「纵向」后（`Settings.tsx:342-351`），面板里的每张卡片都缺一条底边 —— 看不到体积、看不到 ⌘N 角标、**点不到收藏/复制/删除按钮**（必须先在列表里纵向滚动 71px 才能露出卡片底部，而滚动会同时带出下一行卡片，视觉上像是「卡片被切了一半」）。这是设置项与面板行为脱节的典型故障。
- **建议改法**：纵向板型的卡片高度不要写死 220 —— 改成由面板实际高度推导：
  ```tsx
  // Panel.tsx：用一个测量 ref 拿到卡片流高度，再算 rowHeight
  const rowHeight = Math.max(120, Math.min(220, listHeight - 24));
  ```
  并把 `render` 里的 `"h-[220px] w-full"` 改成 `style={{ height: rowHeight }}`；同时 `VirtualGrid` 的 `estimateSize` 同步改成 `rowHeight + gap`（已是）。

### R-03 🔴 管理窗口最小宽度下，历史页卡片被压到 124px，底部操作条被裁掉 —— 「删除」按钮完全不可见

- **现象**：窗口最小宽 420px − 侧边栏 200px = 内容区 220px；减去历史页工具栏 `px-[24px]`（`HistoryPage.tsx:270`）= 172px；再减去 `VirtualGrid` 的 `padding={24}`（`HistoryPage.tsx:348`）= **inner 124px**。`columns = max(1, floor((124+12)/(200+12))) = max(1, 0) = 1`（`VirtualList.tsx:108`）→ 列宽 124px。而卡片底部一行的**最小宽度**：
  `px-10×2 (20) + Kbd(min 20) + gap 6 + 体积文本(≈42, shrink-0) + 4 个 24px 按钮(96) + gap 2×3 (6) + 删除前 6px 间隔 (6) = 196px`。
  → **缺口 72px**，`card-clip` 是 `overflow-hidden`（`index.css:299`），最右边的**删除按钮被完整裁掉**。
- **证据**：`src-tauri/tauri.conf.json:19`（minWidth 420）；`src/windows/MainWindow.tsx:248`（200px 侧边栏）；`src/pages/HistoryPage.tsx:270`、`:343-350`（`:346` minColumnWidth 200、`:348` padding 24）；`src/components/VirtualList.tsx:107-109`、`:136`；`src/components/ClipItem.tsx:271-371`（底部行）、`:349`（`w-[6px]` 间隔）；`src/index.css:298-302`（`.card-clip` `overflow-hidden`）。
- **对用户的影响**：用户把设置窗口拖窄（420–460px 之间）后，历史页每张卡片只剩左侧一小条，**「删除」按钮消失**、体积信息消失、⌘N 角标消失。删除一条记录的唯一入口变成右键菜单（而右键菜单本身键盘不可达，见 U-12）。由于窗口尺寸被 window-state 持久化（`lib.rs:197-204`），这个坏状态会**跨重启保留**。
- **建议改法**：三选一，建议同时做前两条：
  1. `VirtualGrid` 的列数公式在 inner < minColumnWidth 时不要退化成 1 列窄列，而是改为**横向滚动**（把 minColumnWidth 当成硬下限）：`columns = max(1, ceil(inner / minColumnWidth))` 并给容器 `overflow-x-auto`；或
  2. 给 `ClipItem` 增加「紧凑模式」：容器宽度 < 200px 时隐藏 ⌘N 角标与体积文本，把操作按钮从 4 个减到 2 个（收藏 + 删除），并把操作区改为**换行到第二行**；或
  3. 提高 `main` 的 `minWidth`（tauri.conf.json:19）到 **560–600px**：420px 对一个「设置中心 + 侧边栏 + 多列表」的管理窗口本来就不合理，560px 时内容区 360px、列宽 264px，卡片完整。

### R-04 🟠 面板工具栏在逻辑工作区宽度 < ~1300px 时溢出，末尾筛选项被推出可视区

- **现象**：面板工具栏（`Panel.tsx:459-509`）是**单行 flex，不换行、无溢出处理**：搜索框 `w-[220px]`（`:467`）+ 筛选分段（`:470-478`，6 项：全部/文本/图片/文件/网页/收藏，`filters.ts:18-25`）+ 弹性空隙 + 2 个 30px 图标钮（`:480`、`:503`）。
  分段宽度按每项 `px-12×2 + 2 个汉字 24px + 计数` 估算 ≈ 65–72px/项 → **6 项 ≈ 404–446px**（含 5×2 gap 与 2×2 padding）。
  → 工具栏所需宽度 ≈ 16 + 220 + 8 + (404–446) + 8 + 8 + 8 + 30 + 8 + 30 + 16 = **756–798px**。
  而「屏幕中央 / 跟随光标」模式下面板宽 = `avail_w × 0.6`（`window.rs:1094`）→ 需要 `avail_w ≥ 1260–1330px` 才装得下。
  13" MBP（1280 逻辑宽）→ 面板 768px → **缺 0–30px**；13" + 150% 缩放（1008）→ 面板 605px → **缺 151–193px**。
- **证据**：`src/windows/Panel.tsx:459-509`（`:467` w-[220px]、`:470-478` Segment、`:480-508` 两个 IconBtn）；`src/lib/filters.ts:18-25`；`src/components/ui.tsx:214-225`（`.seg` / `.seg-item` 的 padding 与字号）；`src-tauri/src/window.rs:1092-1097`。
- **对用户的影响**：flex 收缩时 `.seg` 与 `.seg-item` 都不收缩（`inline-flex` 的子项不会缩），`.seg` 的背景框被压窄而内容溢出 → 「网页 / 收藏」两项会**画到右边的图标按钮下面/外面**，视觉上像是分段控件破掉，且「收藏」筛选点不到（被图标钮盖住）。同时搜索框被压到约 160px，搜索体验变差。
- **补充事实**：默认 `board_position = "bottom"`（`settings.rs:744-746`），此时宽度 = 屏幕全宽（`window.rs:1070`），**默认配置下不复现**；一旦用户把「出现位置」改成「屏幕中央」或「跟随光标」就会命中。
- **建议改法**：
  1. 工具栏加 `flex-wrap` + 给分段容器加 `min-w-0 overflow-x-auto`（与历史页 `HistoryPage.tsx:279` 同样的兜底），保证不溢出；
  2. 或者在分段里只显示图标 + 当前项文字（其余收起成「更多」）；
  3. 或者把 `panel_width` 的 60% 改成 `clamp(860px, 60%, 1180px)`，保证工具栏最小可容纳。

### R-05 🟠 面板高度滑块的下半段是「死区」，且百分比与真实像素高度对不上

- **现象**：设置页的滑块范围是 20%–60%（`Settings.tsx:333-336`），但 Rust 的实际算法是 `avail_h × scale%` **再 `.max(220)`**（`window.rs:1057-1060`）。工作区高度 < 1100px 时，20%–25% 这一段全部落到 220px 下限：
  - 13" MBP（775）：20%→155→**220**（实际 28.4%）；25%→194→**220**；30%→233。→ 用户把滑块从 20% 拖到 25%，面板**一点没变**。
  - 14" MBP（957）：20%→191→220（实际 23%）；25%→239（有效）；30%→287。
  → 只有工作区 > 1100px（16" MBP / 1080p 外接）时 20% 才真正生效。
- **证据**：`src/pages/Settings.tsx:331-339`（min 20 max 60 step 5）；`src-tauri/src/window.rs:1056-1060`；`src-tauri/src/config/settings.rs:740-742`（默认 30）。
- **对用户的影响**：用户以为面板高度坏了 —— 「我把高度调到 20%，它还是那么高」。滑块是「控制与现实不一致」的典型：控件给了反馈，窗口没动。
- **建议改法**：把下限从「固定 220px」改成「固定 + 比例」双约束：`height = max(220, min(avail_h × scale%, avail_h − 48))` 改成 `height = (avail_h × scale%).clamp(220, avail_h − 48)` 的同时，让前端滑块的**最小值按实际工作区换算**（Rust 把 `avail_h` 通过 `config` 或一条命令回传，前端据此把滑块下限设为 `max(20, ceil(220/avail_h×100))`），并在滑块旁显示**换算后的像素值**（如「30% · 287px」）。

### R-06 🟠 设置页用的是「窗口宽度」断点而不是「内容区宽度」断点 → 768–1100px 时两栏被挤成两半

- **现象**：设置页用了 Tailwind 的 `md:grid-cols-2`（`Settings.tsx:315` 与 `:411`）。Tailwind 的 `md` 是**视口（= 窗口）宽度 ≥ 768px**，而这个窗口左侧永远占着 200px 侧边栏，内容区实际只有 `窗口宽 − 200 − 48`。
  - 窗口 768px → 内容区 568 → 减 `p-[24px]`×2 = 520 → 两栏各 (520−16)/2 = **252px**；再减 `CardGroup` 的 `p-[16px]`×2 = **220px**。
  - 一行 `Row`（`ui.tsx:296-302`）= 标签（min-w-0，可缩）+ gap 16 + 控件（shrink-0）。控件是 `SelectRow width="w-[120px]"`（`Settings.tsx:327`）或 `w-[140px]`（`:483`）→ 标签只剩 220−140−16 = **64px**。「粘贴前去除尾部空白」（9 个汉字 ≈ 117px）被截成「粘贴前…」。
- **证据**：`src/pages/Settings.tsx:315`、`:411`（`md:grid-cols-2`）、`:327`/`:350`/`:483`（select 宽度 120/140）；`src/windows/MainWindow.tsx:248`（200px）、`:325-326`（`p-[24px]` + `max-w-[760px]`）；`src/components/ui.tsx:286-304`（`Row` 结构）、`:512-530`（`CardGroup` `p-[16px]`）。
- **对用户的影响**：用户把窗口拉到 800–1000px 这种最常见的尺寸时，设置页的两栏里所有标签都被截断成「…」，用户读不到设置项的真实名字（「剪贴板监控」「粘贴为纯文本」「磁盘大小上限」…），只能靠猜。**断点选错比没有响应式更糟** —— 它制造了一个「看起来响应式、实际更挤」的中间态。
- **建议改法**：把窗口断点换成**内容区断点**。Tailwind v3 的 `md:` 是视口断点，无法直接用容器查询；两条可行路径：
  1. **用 JS 测量内容区宽度**（`ResizeObserver` 观察 `<main>`），落一个 `data-narrow="true"` 属性，用它切换 1/2 栏（与 `VirtualGrid` 已有的 `ResizeObserver` 模式一致，`VirtualList.tsx:95-105`）；
  2. 或者把 `md:` 换成更保守的 `xl:`（1280px），在 768–1280px 之间保持单栏 —— 单栏时控件有 520px 宽，标签完整可读。

### R-07 🟠 历史页筛选分段在窄窗下退化成「一条几十像素宽的横向滚动条」

- **现象**：旧 R1 的整改（`HistoryPage.tsx:270-285`）确实加了 `flex-wrap` 与 `overflow-x-auto`，**解决了「类型筛选整组消失」**（这部分整改有效 ✓）。但它没有解决「分段被压成一条缝」：分段容器是 `min-w-0 max-w-full overflow-x-auto`（`:279`），在 flex-wrap 下它的假设主尺寸 = 内容的 max-content（≈404–446px）> 可用宽度 → **换行到第二行**，第二行宽度 = 容器全宽；但因为 `min-w-0`，它又被允许收缩。
  - 窗口 420 → 内容 220 → 工具栏可用 172 → 分段容器最终宽 ≈ 172px（第二行），而内容 404–446px → **用户只看到 2/6 个筛选项，剩下 4 个要在一个 172px 宽的框里横向滚动**；
  - 窗口 460 → 内容 260 → 可用 212 → 搜索框 `min-w-[140px] flex-1` 占 140 + gap 8 → 分段只剩 **64px**（比一行还窄）。
- **证据**：`src/pages/HistoryPage.tsx:270`（`flex-wrap`）、`:275`（`w-[240px] min-w-[140px] max-w-full flex-1`）、`:279`（`min-w-0 max-w-full overflow-x-auto`）、`:280-284`（Segment）；`src/components/ui.tsx:214-225`（`.seg` 无换行能力：`inline-flex`，无 `flex-wrap`）。
- **对用户的影响**：窄窗下用户想按「图片」筛选，得在一个 64–172px 的滑动窗口里横向滚动才能找到它，而且滚动条是 overlay 样式（`.scroll-apple` 没加在这个容器上，`HistoryPage.tsx:279`），几乎不可见。用户会认为「图片筛选不见了」。
- **建议改法**：
  1. 给 `.seg` 加 `flex-wrap`（或 `max-w-full flex-wrap`），让 6 个筛选项在窄容器里**换行成两行**而不是横向滚动；
  2. 或者在窄容器下把分段降级为「仅图标 + 当前项」（`.seg-item` 只显示图标，label 用 `sr-only`）；
  3. 给这个滚动容器补 `.scroll-apple`，让滚动条在 hover 时可见。

### R-08 🟠 权限页的三按钮行在窄窗下被裁掉「打开系统设置」

- **现象**：`PermissionCenter` 的「辅助功能」一行放了 3 个按钮（`请求授权` / `刷新` / `打开系统设置`，`PermissionCenter.tsx:147-159`），`Row` 的控件区是 `flex shrink-0`（`ui.tsx:301`）→ **不会被压缩，只会溢出**。窗口 420px 时：内容区 220 − `p-24×2` = 172 − `CardGroup p-16×2` = **140px 可用**；三个 `Btn size="sm"` 的宽度 ≈ 68 + 44 + 92 + 2×8 gap = **220px** → 溢出 80px，被 `<main>` 的 `overflow-hidden`（`MainWindow.tsx:319`）裁掉 → **「打开系统设置」按钮整个不可见**。
- **证据**：`src/components/PermissionCenter.tsx:137-160`（`:147-159` 三个按钮）；`src/components/ui.tsx:286-304`（`:301` `shrink-0`）、`:199-201`（`.btn-sm` `px-[10px] text-[12px]`）；`src/windows/MainWindow.tsx:319`（`overflow-hidden`）、`:248`（200px 侧边栏）。
- **对用户的影响**：权限页最重要的三个入口（请求授权 / 刷新 / 打开系统设置）里有一个在窄窗下消失，用户按指引「去系统设置里勾选」时找不到入口，只能自己去找系统设置的路径。
- **建议改法**：把 `Row` 的控件区改成可换行（`flex-wrap` + `justify-end`），或在这一行用 `Column` 布局：窄容器时标签在上、按钮在下并允许换行。最省事的是给 `Row` 加一个 `stack` 变体，由外层用 `ResizeObserver` 或 CSS container query 切换。

### R-09 🟠 `VirtualGrid` 的 `minColumnWidth` 契约在窄容器下失效

- **现象**：`columns = Math.max(1, Math.floor((inner + gap) / (minColumnWidth + gap)))`（`VirtualList.tsx:108`）。当 `inner < minColumnWidth` 时，floor 得到 0，被 `max(1, …)` 兜成 **1 列**，而这一列的宽度 = `inner`（**小于 minColumnWidth**）。也就是说「最小列宽 200」这个承诺在容器 < 200px 时直接失效，卡片被压到 124px（R-03 的根因）。
- **证据**：`src/components/VirtualList.tsx:107-109`、`:136`（`repeat(columns, minmax(0,1fr))`）；`src/pages/HistoryPage.tsx:346`（minColumnWidth 200）；`src/windows/Panel.tsx:521`（minColumnWidth 168）。
- **对用户的影响**：卡片内容（头部徽章 + 时间戳 + 来源名 + 图标，底部 4 个操作按钮）按 168/200px 设计，被压到 124px 后**头部与尾部同时溢出裁切**（`.card-clip` `overflow-hidden`）。
- **建议改法**：要么 `columns = Math.max(1, Math.ceil(inner / (minColumnWidth + gap)))` 并让容器横向滚动；要么在 `inner < minColumnWidth` 时给卡片套一个紧凑变体；要么把 `minWidth` 提高到 560（见 R-03 建议 3），从根上消除这个区间。

### R-10 🟡 列数在特定宽度上「突变」，拖窗时卡片整体跳变

- **现象**：由 `columns = floor((inner+12)/212)`，列数在 inner = 200, 412, 624, 836… 处翻页，且列宽在翻页点前后不连续：
  - 窗口 664 → inner 368 → 2 列 → 列宽 202px；
  - 窗口 668 → inner 372 → 1 列 → 列宽 **372px**（卡片宽度瞬间翻倍）。
  其余翻页点同理（1→2 列 372→202；2→3 列 202→205；3→4 列 205→229）。
- **证据**：`src/components/VirtualList.tsx:107-109`、`:136`；推演表见 §0.4。
- **对用户的影响**：用户拖动窗口边缘调整宽度时，卡片会在某一瞬间从 2 列大卡变成 1 列巨卡（宽 372px 的卡片里只有 168–200px 的内容，两侧大片空白），观感上是「布局跳了」。配合 §0.4 的列宽表，列宽在 202–296 之间来回跳。
- **建议改法**：列宽与列数解耦 —— 用 `grid-template-columns: repeat(auto-fill, minmax(minColumnWidth, 1fr))` 让浏览器自己处理（列宽连续变化，列数在能放下时才变），或保留虚拟化但把列宽也纳入虚拟器的测量（`measureElement`），使翻页点处的最后一列不被拉伸。

### R-11 🟡 同一张卡片在面板与历史页的宽度基准不一致（168 vs 200）

- **现象**：面板纵向网格 `minColumnWidth={168}`（`Panel.tsx:521`），历史页 `minColumnWidth={200}`（`HistoryPage.tsx:346`）；面板横向卡片固定 168px（`VirtualList.tsx:33`），而 `.card-clip` 也写死 `w-[168px]`（`index.css:299`）。同一份内容在两个界面上宽度不同（面板纵向时列宽 ≈199px）。
- **证据**：`src/windows/Panel.tsx:521`、`:526-531`；`src/pages/HistoryPage.tsx:346`；`src/components/VirtualList.tsx:33`（`itemWidth = 168`）；`src/index.css:299`。
- **对用户的影响**：用户在面板里看到的是 168px 窄卡（信息密度高），切到历史页是 200–296px 宽卡（更宽松），同一记录的两副面孔；「查看全文」的触发条件（>160 字，`ClipItem.tsx:194-196`）在两种宽度下的观感也不同。
- **建议改法**：统一到一个 token（如 `--card-min-w: 184px`），面板横向/纵向、历史页共用。

### R-12 🟡 系统显示缩放（125% / 150%）会把逻辑工作区压到面板的临界区

- **现象**：Rust 用 `monitor.size() / scale_factor`（`window.rs:1042-1043`）换算逻辑点，macOS 的「显示缩放」改变了报告给 App 的逻辑分辨率，于是 `avail_w` 变小：
  - 13" MBP（物理 2560×1600）默认逻辑 1280×800；缩放到「更多空间」或 150% 后逻辑宽可降到 ~1067/1008。
  - 面板宽（居中/跟随光标）= `avail_w × 0.6` → 640 / 605px → 触发 R-04 的工具栏溢出；
  - 面板高 = `avail_h × 30%`：1008×630 时 = 189 → **被 `.max(220)` 兜到 220** → 触发 R-01 的「预览只剩 24px」。
- **证据**：`src-tauri/src/window.rs:1042-1043`、`:1056-1060`、`:1092-1097`；`src/pages/Settings.tsx:331-339`。
- **对用户的影响**：同样的设置，在 100% 缩放的 Retina 屏上一切正常，换到 125%/150% 缩放（或外接 1024×768 显示器）后，面板工具栏溢出 + 文本预览几乎不可见，而设置里没有任何地方能看出原因。
- **建议改法**：面板的固定 chrome（114px）在逻辑像素里不随缩放变化，但**字号/行高在 WKWebView 里是逻辑像素**，所以 114px 的 chrome 在 150% 缩放下占的**物理**空间更大、视觉上更重 —— 因此 chrome 应按 `scale` 同比放大或按比例缩减卡片区。建议 Rust 把 `avail_w/avail_h/scale` 一并回传前端，前端据此选择「紧凑 chrome」模式（隐藏标签行/状态栏次要项）。

### R-13 🟡 面板高度调到 60% 时，卡片预览区有 259px 空白（内容高度与面板高度脱钩）

- **现象**：文本预览被 `line-clamp-[9]` 封顶（9 行 × 18px = 162px，`ClipItem.tsx:141`/`:148`/`:156`），而预览容器是 `flex-1`（`:266`）→ 面板高 655px 时预览区 459px，内容只有 162px，**297px 空白**（下方紧贴 30px 的操作条）。
- **证据**：`src/components/ClipItem.tsx:141`、`:148`、`:156`（`line-clamp-[9]`）、`:266`（`min-h-0 flex-1 overflow-hidden`）；§0.3 的高度表。
- **对用户的影响**：用户把面板拉到 60% 期待「看得更多」，结果每张卡片里还是那 9 行，下面一片空白 —— 高度滑块在 30% 以上没有任何实际收益，用户会误以为应用卡住了。
- **建议改法**：让 `line-clamp` 随可用高度自适应（见 R-01 建议 2），把富文本/长文本的可见行数与面板高度绑定；或者把面板高度上限压到「9 行 + chrome」≈ 340px，超出部分不再给用户（滑块上限按实际收益收敛）。

### R-14 🟡 面板滑块的最小值 20% 与 Rust 的 `clamp(10, 60)` 语义不一致（UI 无法表达 10–19%）

- **现象**：Rust 允许 10–60（`window.rs:1057`），UI 只给 20–60（`Settings.tsx:333-336`）。用户在 1080p 上想要一个更矮的面板（比如 15% = 158px）时，滑块根本到不了；即使到了，Rust 也会 `.max(220)` 兜底（15% of 1055 = 158 → 220）。所以「10–19%」这一段在实现上完全不可达。
- **证据**：`src-tauri/src/window.rs:1057`、`:1059`；`src/pages/Settings.tsx:333-336`。
- **对用户的影响**：不是直接故障，但配置语义与 UI 不一致，后续维护容易踩坑（例如有人把 Rust 的下限改成 150 而不知道 220 的硬下限还在）。
- **建议改法**：统一到一个常量来源（Rust 导出 `PANEL_MIN_HEIGHT`，前端滑块下限 = `ceil(PANEL_MIN_HEIGHT / avail_h × 100)`），并在注释里写明「220 是卡片三段 + 114 chrome 的物理下限」。

### R-15 🟡 管理窗口最小高度 360px 时，历史页的固定 chrome 就占掉 160px

- **现象**：`minHeight: 360`（tauri.conf.json:20）。历史页的页头（`pt-20 + 22 + pb-10` = 52px，`HistoryPage.tsx:200`）+ 工具栏（28+10 = 38px，`:270`）+ 标签行 34px（`:289`）+ 状态栏 36px（`StatusBar.tsx:48`）= **160px**，只剩 **200px** 给卡片流；而一行卡片是 220px → 底部 20px 的操作条被切掉。
- **证据**：`src-tauri/tauri.conf.json:20`；`src/pages/HistoryPage.tsx:200`、`:270`、`:289`、`:354`；`src/components/StatusBar.tsx:48`；`src/components/ClipItem.tsx:271`（30px 底部）。
- **对用户的影响**：窗口纵向压到最小时，历史页的卡片底部条（复制/删除）看不全，需要滚动列表才能看到。
- **建议改法**：`minHeight` 提高到 480，或在历史页高度不足时把状态栏折叠成一行 28px。

### R-16 🟢 待验证：面板 header 的 `data-tauri-drag-region` 与内部控件的命中关系

见 U-18（同一现象的响应式侧风险：面板在「屏幕中央 / 跟随光标」模式下若被拖拽，下次唤起会弹回，见 U-17）。

---

## 3. Phase 1 结论的复核

| 编号 | Phase 1 结论 | 判定 | 复核证据 |
|---|---|---|---|
| **V-01** | 焦点环硬编码 `rgba(232,163,61,0.35)`，`.light` 无覆盖；`--hairline-focus` 零引用；所有控件 `outline-none`；`MainWindow.tsx:288` 侧边栏连 focus-visible 规则都没有 | ✅ **确认** | `index.css:42`/`:108` 定义 `--hairline-focus`；全文件无 `var(--hairline-focus)` 引用（grep 无命中）；`outline-none` 位于 `index.css:171,180,203,219,228,243,263`、`ui.tsx:80,391`、`MainWindow.tsx:288,308`；`index.css` 的 focus-visible 规则只覆盖 `.btn/.btn-icon/.seg-item/.pill/.switch/.input/.slider` |
| **V-02** | `bg-warning` / `bg-success-soft` 浅色下 1.35:1 / 1.61:1；变量改了但 JSX 不消费 | ⚠️ **有出入（问题真实，但归因与行号需修正）** | `tailwind.config.ts:36-37`：`success`/`warning` 是**静态 hex**（不是变量），所以 JSX 里的 `bg-warning`/`bg-success-soft` 永远不吃 `--warning`/`--success` → 问题成立。**但**：`text-warning`（`StatusBar.tsx:54`）与 `text-success-soft`（`Settings.tsx:536`、`ToastHost.tsx:6`）**有**浅色 remap（`index.css:616-617`），所以「监控已暂停」**徽标的文字是可读的**（浅色 `#9a6b00`）。真正不可见的是：徽标底色 `bg-warning/[0.14]`（`StatusBar.tsx:54`）、5px 状态点 `bg-warning`（`:57`）、`PermissionCenter.tsx:30`、`ContextMenuSettings.tsx:124,149` 的三态色点。**Phase 1 把「文字对比度」与「色点对比度」混在一起，行号应精确到 54（底色）与 57（色点）** |
| **V-03** | `text-danger/80` 漏 remapping，浅色 2.22–2.32:1；两处 `max-h-[48px] overflow-hidden` 硬裁 | ✅ **确认** | `index.css:614-615` 只 remap 了 `text-danger` 与 `text-danger-soft`，**没有 `text-danger/80`**；`text-danger/80` 仅出现在 `Panel.tsx:391`、`HistoryPage.tsx:299`，两处都带 `max-h-[48px] overflow-hidden`（grep 确认全库仅此两处）。**补充（本阶段新增）**：这两处错误文案既无 tooltip 也无法复制，用户看不到失败原因 → 已升级为 U-13 |
| **V-04** | 卡片选中态浅色 border 仅 2.16:1 | ✅ **确认** | `index.css:306-309`（`.card-clip-selected` 用 `border-accent-500`）、`:742-746`（`.light` 用 `border-color: var(--accent-500)`）；**`:root.light`（`:85-126`）没有覆盖 `--accent-500`**，仍是 `#e8a33d` → 浅色下描边 2.16:1 成立。`ClipItem.tsx:238`、`Panel.tsx:359` 引用；`HistoryPage.tsx:188-189` 的多选内框是 JSX 里的固定 `rgba(232,163,61,0.85)`，同样不随浅色重映射。**本阶段补充**：该状态还没有 `aria-selected`（U-03） |
| **V-05** | 侧边栏 hover 与 active 仅差 1.049:1 | ✅ **确认** | `MainWindow.tsx:290`（active `bg-white/[0.10]`）vs `:291`（hover `bg-white/[0.06]`）；浅色 remap 后 `index.css:643`（0.10→0.07）vs `:675`（hover 0.06→0.05）→ 差值仍为 0.02 透明度 |
| **V-06** | 只有 `.btn-primary` 有 disabled 视觉 | ✅ **确认** | `index.css:187`（`.btn-primary disabled:bg-accent-500/30 disabled:text-white/30`）；`:189-192` `.btn-secondary`、`:193-195` `.btn-danger`、`:196-198` `.btn-ghost` **均无 disabled 样式**；`.btn` 基类只有 `disabled:cursor-not-allowed`（`:182`）。Pill 的 `opacity-40`（`ui.tsx:189`）与 Switch 的 `opacity-40`（`ui.tsx:277`）是另外两种表达 |
| **V-07** | 菜单勾标记 `text-accent-500` 与 hover 底 `bg-accent-500` 同色 → 深色下 1.0:1 | ✅ **确认** | `ClipContextMenu.tsx:41`（`text-accent-500`）；`.menu-item:hover/.menu-item-active` 背景 `bg-accent-500`（`index.css:336`）；深色下二者同为 `#E8A33D`。**补充**：浅色下 `text-accent-500` 被 remap 成 `#8a5816`（`index.css:623-625`），因此**浅色模式不受影响**，问题只在深色模式 |
| **V-08** | 空状态 hint `text-white/[0.28]` 仅 2.51:1；加载态 Spinner 无文字、无 aria-live | ✅ **确认** | `ui.tsx:594`（`text-white/[0.28]`）、`:592`（icon `text-white/30`）、`:608`（Spinner `aria-hidden`）；`Panel.tsx:379-382`、`HistoryPage.tsx:292-294` 的加载态都只有一个裸 Spinner。**补充**：空/错误容器也没有 `role="status"`（U-13） |
| **V-09** | 三级/四级文字 2.51–3.94:1（深色）/ 2.65–4.15:1 全部低于 AA | ✅ **确认** | `index.css:601-606`（`text-white/60 → rgba(0,0,0,0.58)` 等）与 `:608-613` 的补档；JSX 侧 `text-white/45`（`StatusBar.tsx:48`、`:202` 之外还有 `Panel.tsx:279` 等）、`text-white/40`、`text-white/35`、`text-white/[0.28]` 大量在用。**本阶段补充**：`text-white/45` 用在状态栏正文（`StatusBar.tsx:48`）与设置页副文案（`Settings.tsx:199`）上，直接影响「监控已暂停」等**状态信息的可读性**（U-16） |
| **V-12** | 卡片固定三段 28/flex-1/30 + `line-clamp-[9]`；「查看全文」仅在 >160 字时出现；头部 `max-w-[112px] shrink-0` 与来源名竞争；删除与复制按钮仅 6px | ✅ **确认** | `ClipItem.tsx:243`（28）、`:266`（`flex-1`）、`:271`（30）、`:141/148/156`（`line-clamp-[9]`）、`:194-196`（`canExpand` 条件 `value.length > 160`）、`:254`（`max-w-[112px] shrink-0`）、`:259`（来源名 `ml-auto min-w-0 truncate`）、`:349`（`w-[6px]`）。**本阶段补充**：R-03 证明这条 6px 间隔在窄容器下会连同整个操作条一起被裁掉 |
| 旧 L1 | 浅色 accent 对比 | ✅ 已修 | `index.css:623-625` → `#8a5816` |
| 旧 L2 | 侧边栏分层 | ✅ 已修（衍生 V-05） | `index.css:654`（`bg-black/[0.15]` → 7% 黑） |
| — | white 家族 remapping 0 漏类 | ✅ 确认 | `index.css:597-613` 覆盖了 JSX 里在用的全部档位 |
| U3 | Segment stopPropagation | ✅ 已修 | `ui.tsx:129-143`（`:140` `e.stopPropagation()`），方向键不会同时切筛选与移动卡片选中。**补充**：Segment 的 roving tabindex（`ui.tsx:158`）也在，但**卡片列表层没有**（U-06） |
| U5 | ClipDetail 焦点管理 | ✅ 已修 | `ClipDetail.tsx:33-82`（`:36` 记录原焦点、`:38` 移入、`:51-75` Tab 陷阱、`:80` 归还）。**对比**：DialogHost / FirstRunGuide / ClipContextMenu **都没有**归还（U-09 / U-10 / U-12） |
| U1/U2 | 卡片 `role="button"` → `role="group"` | ✅ 已改 | `ClipItem.tsx:201` 现为 `role="group"`；**但 `tabIndex={0}` 保留**（`:202`）→ 本阶段确认列表层缺 roving tabindex（U-06），且 `role="group"` + `tabIndex=0` 不是一个有效的可聚焦语义（U-03） |
| 旧 R1 | 历史页工具栏裁切 | ⚠️ **部分修好** | `HistoryPage.tsx:270`（`flex-wrap`）、`:275`（`min-w-[140px]`）、`:279`（`overflow-x-auto`）确实防止了「整组消失」 ✓；但**没有解决「被压成一条 64–172px 的横向滚动缝」**（R-07），也没有解决窗口 420–460px 时卡片被压到 124px、删除按钮被裁的问题（R-03） |

**Phase 1 未覆盖、本阶段新增的视觉/结构问题**（供 Phase 3 参考）：`bg-warning`/`bg-success-soft` 未走变量（V-02 的精确化）、`text-danger/80` 无 remap（V-03）、卡片选中态无 `aria-selected`（U-03）、`Row` 控件区 `shrink-0` 不可换行（R-08）、滑块无 `aria-valuetext`（U-14）。

---

## 4. 无障碍清单（按 WCAG 2.2 AA 对照）

| # | 项目 | 现状 | 证据 | 判定 |
|---|---|---|---|---|
| A1 | 焦点可见 | 侧边栏导航、卡片内 4 个图标按钮**无任何焦点指示** | `MainWindow.tsx:288,308`；`ui.tsx:80`；`index.css` 无对应 focus 规则 | ❌ 2.4.7 / 1.4.11 |
| A2 | 焦点移入 / 陷阱 / 归还 | ClipDetail ✅；DialogHost 有陷阱无归还；FirstRunGuide 三缺；ContextMenu 三缺 | `ClipDetail.tsx:33-82`；`DialogHost.tsx:50-55,67-83`；`FirstRunGuide.tsx:61-113`；`ClipContextMenu.tsx:100-132` | ❌ 2.4.3 |
| A3 | 列表语义 | 卡片是 `role="group" tabIndex={0}`，无 `aria-selected`，容器无 `role="listbox"` | `ClipItem.tsx:201-202`；`VirtualList.tsx:53,124` | ❌ 4.1.2 |
| A4 | 卡片无障碍名 | `aria-label` 描述的是**动作**（"单击复制，双击回贴"），不含内容/类型/时间 → 读屏连读 15 次「单击复制，双击回贴」 | `ClipItem.tsx:204` | ❌ 1.3.1 / 4.1.2 |
| A5 | 加载 / 空 / 错误播报 | Spinner `aria-hidden` 且无文案；空/错误容器无 `role="status"` / `aria-live` | `ui.tsx:600-619`；`Panel.tsx:379-382,386-408`；`HistoryPage.tsx:291-340` | ❌ 4.1.3 |
| A6 | 动态内容播报 | Toast 有 `role="status" aria-live="polite"` ✅；状态栏错误有 `role="alert"` ✅；保存状态有 `role="status"` ✅ | `ToastHost.tsx:27-28`；`StatusBar.tsx:70`；`Settings.tsx:539` | ✅ |
| A7 | 滑块值语义 | 无 `aria-valuetext`，读出裸数字 | `ui.tsx:355-364` | ❌ 4.1.2 |
| A8 | 开关 / 分段 / 胶囊语义 | `role="switch"+aria-checked` ✅；`role="tablist"/tab` + roving ✅；`aria-pressed` ✅ | `ui.tsx:266-271`；`:146,156-158`；`:192` | ✅ |
| A9 | 按钮无障碍名 | `IconBtn`/`CardIconBtn` 靠 `title` 命名 —— 可用，但 13px 纯 SVG 对读屏无意义，建议补 `aria-label` | `ui.tsx:56-68,71-92` | ⚠️ 建议 |
| A10 | 模态背景隔离 | `aria-modal="true"` 但背景未 `inert`，读屏虚拟光标仍可遍历背景 | `DialogHost.tsx:96-101`；`FirstRunGuide.tsx:68-73` | ❌ 4.1.2 |
| A11 | 对比度 | 见 Phase 1 V-01/V-02/V-03/V-04/V-08/V-09 | — | ❌ 1.4.3 |
| A12 | 目标尺寸 | 卡片内按钮 24×24（≥24pt 达 HIG 下限但**低于 WCAG 2.2 的 24×24 需 44×44 判据**） | `ui.tsx:80`（`h-[24px] w-[24px]`） | ⚠️ 2.5.8 |
| A13 | 键盘可达的右键菜单 | 无方向键、无焦点移入 | `ClipContextMenu.tsx:100-132,141-160` | ❌ 2.1.1 |
| A14 | 键盘可达的列表 | 历史页无方向键 / Home / End；卡片列表无 roving tabindex | `HistoryPage.tsx` 全文 | ❌ 2.1.1 |
| A15 | Esc 语义分级 | Esc 在搜索框里直接关面板 | `panelKeys.ts:74` | ❌ 3.2.2 |
| A16 | 减少动效 | `@media (prefers-reduced-motion: reduce)` 已覆盖动画与过渡 ✅ | `index.css:575-586` | ✅ |
| A17 | 降低透明度 | `@media (prefers-reduced-transparency: reduce)` 已覆盖表面材质 ✅（但 `.light` 的背景填充类不随之调整，浅色下仍可读，风险低） | `index.css:159-166` | ✅ |
| A18 | 键盘可发现性 | 首启引导宣称的「← →」在默认状态下不可用 | `FirstRunGuide.tsx:83` vs `panelKeys.ts:120` | ❌ 3.2.2 |

---

## 5. 优先修复顺序

| # | 条目 | 严重度 | 影响面 | 改动量 | 一句话改法 |
|---|---|---|---|---|---|
| 1 | **U-02** 确认框 Enter = 执行破坏性操作 | 🔴 | 删除 / 清空 / 退出 全链路 | 小 | 删掉 `DialogHost.tsx:38-42` 的全局 Enter 处理，改为「仅当焦点在确认键上才确认」。 |
| 2 | **R-03** 窄窗下历史页卡片底部被裁、删除按钮消失 | 🔴 | 历史页 + 设置窗最小宽 | 小 | `tauri.conf.json:19` 的 `minWidth` 420 → 560（一行改动即解）；并给 `VirtualGrid` 加列宽下限兜底。 |
| 3 | **U-01** 方向键导航在默认状态不可达 | 🔴 | 面板核心心流 + 首启引导 | 中 | `panelKeys.ts` 增加「搜索框为空时方向键归列表」规则，并同步改 `FirstRunGuide.tsx:83` 文案。 |
| 4 | **U-04 + U-05** 焦点指示缺失（侧边栏 + 卡片按钮） | 🔴 | 全部键盘可达性 | 小 | `index.css` 加一条全局 `:where(button,…):focus-visible { outline: 2px solid var(--hairline-focus) }`，并去掉 `outline-none`。 |
| 5 | **R-01 + R-02** 面板最小高度 / 纵向板型下预览不可见、操作条不可见 | 🔴 | 面板全部板型 | 中 | 面板高度下限 220 → 264；纵向板型的 `rowHeight` 改为按可用高度计算（`Panel.tsx:516-523`）。 |
| 6 | **U-03** 选中态不可见 + 不被读屏感知 | 🔴 | 面板主心流 | 中 | `:root.light` 增加 `--accent-500: #8a5816`；卡片加 `aria-selected`，列表加 `role="listbox"`。 |
| 7 | **R-06** 设置页用窗口断点而非内容区断点 | 🟠 | 设置页 768–1100px 全区间 | 中 | 用 `ResizeObserver` 测 `<main>` 宽度切 1/2 栏，或把 `md:` 换成 `xl:`。 |
| 8 | **R-04** 面板工具栏溢出 | 🟠 | 面板（非贴底模式） | 中 | 工具栏加 `flex-wrap` + 分段容器 `min-w-0 overflow-x-auto`；或 `panel_width` 改为 `clamp(860px, 60%, 1180px)`。 |
| 9 | **U-06 + U-07** 历史页无列表级键盘导航 / 多选态双 Tab 停靠 | 🟠 | 历史页键盘可达性 | 大 | 卡片列表实现 roving tabindex + ↑/↓/Home/End；多选态隐藏内层卡片的 tabIndex 与操作按钮。 |
| 10 | **U-09 + U-10** 对话框不归还焦点 / 首启引导无焦点管理 | 🟠 | 全部弹层 | 小 | DialogHost 记录并归还 `activeElement`；FirstRunGuide 复用 ClipDetail 的焦点三件套。 |
| 11 | **U-11** 撤销 toast 只有 2.4 秒 | 🟠 | 删除可恢复性 | 小 | 删除类 toast `duration: 8000`。 |
| 12 | **U-13 + V-03** 错误文案硬裁 48px / 无播报 | 🟠 | 错误处理 | 小 | 错误块改 `max-h-[96px] overflow-y-auto` + `title` + 复制按钮；Spinner/空态加 `role="status"`。 |
| 13 | **U-15 / U-16 / V-02** ⌘C 吞选区 / 监控徽标不可见 / 语义色不走变量 | 🟠 | 状态感知 | 小 | `panelKeys.ts` 搜索框内 ⌘C 放行；`tailwind.config.ts` 的 `warning/success` 改成 `rgb(var(--x) / <alpha-value>)`。 |
| 14 | **R-05 + R-14** 面板高度滑块死区与语义不一致 | 🟠 | 面板设置 | 中 | Rust 回传 `avail_h`，前端把滑块下限换算为 `max(20, ceil(220/avail_h×100))` 并显示像素值。 |
| 15 | **R-07 / R-08 / R-09** 分段被压成滚动缝 / 权限按钮被裁 / 列宽下限失效 | 🟠 | 窄窗可用性 | 中 | `.seg` 支持换行；`Row` 控件区允许换行；`VirtualGrid` 用 `ceil` + 横向滚动保底列宽。 |
| 16 | **U-12** 右键菜单键盘不可达 | 🟠 | 高级操作可达性 | 中 | 菜单加焦点移入 + ↑/↓ 循环 + 子菜单键盘展开。 |
| 17 | **U-14 / A7** 滑块无 `aria-valuetext` | 🟠 | 读屏 | 小 | `SliderRow` 透传 `aria-valuetext={format(value)}`。 |
| 18 | **R-13** 面板拉高后卡片大面积空白 | 🟡 | 面板高度 | 中 | `line-clamp` 与可用高度联动；滑块上限收到 9 行 + chrome。 |
| 19 | **R-10 / R-11** 列数突变 / 两处卡片宽度基准不一致 | 🟡 | 观感一致性 | 中 | 统一 `--card-min-w` token；列宽用 `auto-fill + minmax` 连续变化。 |
| 20 | **U-17 / R-15 / R-16 / U-15** 面板拖拽无记忆 / 最小高度 chrome 占比 / 拖拽命中待验证 / ⌘C 吞选区 | 🟢–🟡 | 细节 | 小–中 | 逐条按上文建议处理；U-18 需实测后定级。 |

**建议的实施顺序**：1 → 2 → 4 → 3 → 5 → 6 →（7、10、11、12、13、14、15、17 打包成一轮「小改」提交）→ 9（最大的一块）→ 其余。

---

*本报告未修改任何源文件。所有像素推演基于 `src-tauri/tauri.conf.json` 与 `src-tauri/src/window.rs` 的真实约束；标注「待验证」的条目需实测确认后再定级。*
