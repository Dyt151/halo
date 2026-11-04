# Halo 主界面 · Phase 3 工程复核报告（逐条验证 + 可执行整改清单）

> 复核者：frontend-engineer（前端工程专家）｜日期：2026-11-04｜工作区：`/Users/dayiteng/Documents/codex/小发明/halo`
> 输入：`deliverables/phase1-visual-review-2026-11-04.md`（V-01~V-14）、`deliverables/phase2-ux-responsive-review-2026-11-04.md`（U-01~U-18、R-01~R-16）
> 方法：**逐条打开当前工作区代码核对 + 独立复算像素/对比度 + 跑构建与测试 + 读 Tauri 2.12.5 依赖源码证伪**。
> 纪律：**未修改任何源文件**。所有断言给出 文件:行 证据；无法静态验证的单独列「需实机验证」。

---

## 0. 复核方法与总体可信度

| 复核手段 | 结果 |
|---|---|
| `npm run build`（`tsc && vite build`） | ✅ 通过。产物：`index-*.js` **420.58 kB / gzip 131.80 kB**，`index-*.css` 57.26 kB / gzip 10.51 kB，单包无分包 |
| `npm test`（vitest 5.0.5，默认 forks 池） | ⚠️ 本沙箱内 forks 池启动超时（环境问题，非代码缺陷）。改 `--pool=threads --no-file-parallelism` 后：**9 文件 / 159 用例全部通过** |
| 语义色 / white 家族类名差集（脚本全量扫描 `src/**/*.{ts,tsx}` vs `index.css` 的 65 条 `[class~=]` 映射） | white 家族仅剩 1 个未映射 token；语义色家族 26 个未映射 |
| Tauri 拖拽命中规则 | ✅ 读了 `~/.cargo/registry/.../tauri-2.12.5/src/window/scripts/drag.js`（Cargo.lock 锁定 tauri 2.12.5），**U-18/R-16 被源码证伪** |

**结论：Phase 1/2 的 48 条中，42 条成立、4 条需修正数字或归因、2 条不成立。** 结论方向整体可靠；但 Phase 2 的**宽度推导链存在一处系统性错误（重复扣了一次内边距）**，导致 R-03 的结论、R-07/R-10 的数字、§0.4 整张列宽表都失真 —— 这部分必须以本报告 §2 的复算为准。

---

## 1. 复核结论总表（48 条）

> 判定：✅ 确认存在 ｜ ⚠️ 有出入（给正确结论）｜ ❌ 不成立（给反证）

### 1.1 视觉层 V-01 ~ V-14

| 编号 | 判定 | 修正说明 | 最终行号 |
|---|---|---|---|
| **V-01** | ✅ | 全部核实。`--hairline-focus` **确实零引用**（我全仓复核）；`.light` 段（691-762）确实无任何 focus 覆盖；`outline-none` 7 处在 index.css + 2 处在 ui.tsx + 2 处在 MainWindow 全部核实。**补充**：深色下焦点环也仅 2.10:1，且 `.input`/`.select` 用的是 `focus:`（非 `focus-visible`），鼠标点输入框也出环 | 变量 `index.css:42`（深）`:108`（浅）；焦点环 `:173` `:183` `:207` `:221` `:231` `:245` `:281-284`；`ui.tsx:393`；`outline-none` `index.css:171,180,203,219,228,243,263` + `ui.tsx:80,391` + `MainWindow.tsx:288,308` |
| **V-02** | ⚠️ | **Phase 2 的精确化正确，采纳**。Phase 1 字面表述「`.light` 的 `bg-*` 完全没有」不成立（`bg-white/*`、`bg-group`、`bg-raised`、`bg-win`、`bg-badge-unknown`、`bg-black/[0.15]` 都有映射）。真实边界：**语义色家族的 `bg-*/border-*/ring-*/stroke-*` 一条映射都没有**，而 `text-*` 全部映射。最终表述：「监控已暂停」徽标**文字可读**（`text-warning`→`--warning:#9a6b00`，4.5:1）；不可见的是 **①徽标底 `bg-warning/[0.14]` ②5px 色点 `bg-warning` ③PermissionCenter/ContextMenuSettings 三态色点**。根因：`tailwind.config.ts:36-37` 是**静态 hex**，`index.css:110-111` 改的 `--success/--warning` **零消费者** | 静态色 `tailwind.config.ts:35-38`；变量 `index.css:110-111`；`.light` 仅 `text-*` `index.css:614-625`；徽标 `StatusBar.tsx:54`（底）`:57`（点）；色点 `PermissionCenter.tsx:30`、`ContextMenuSettings.tsx:124,149`（**路径是 `src/components/`，Phase 2 写成 `src/pages/` 是错的**） |
| **V-03** | ✅ | 全部核实。脚本扫描确认 `text-danger/80` 全库仅 2 处；`index.css:614-615` 确无该档。浅色 2.32:1（白底）/2.22:1(#fafafc) 复算通过。两处 `max-h-[48px] overflow-hidden` 硬裁同样成立 | `Panel.tsx:391`、`HistoryPage.tsx:299`；映射 `index.css:614-615`；色值 `tailwind.config.ts:35` |
| **V-04** | ⚠️ | 行号全对，**对比度数字要分层**：白底 **2.16:1**（Phase 1 的数，复算通过）；`#fafafc` 材质底 **2.07:1**（Phase 2 用的是 2.16）。两者都 <3:1，结论不变。**补强**：`:root.light` 缺 `--accent-500` 还连带 `bg-accent`（MainWindow:251 品牌块、HistoryPage:178 多选勾选框）、`border-accent`（RingEditor/ui.tsx/RingSettings/HotkeySettings）、`border-accent/60|70` 全部停在 2.07:1 | `index.css:306-309`、`:742-746`、`:85-126`（`:97` 只定义 `--accent-tint`、`:98` `--accent-disabled`，**无 `--accent-500`**）；调用 `ClipItem.tsx:238`、`Panel.tsx:359`、`HistoryPage.tsx:178,189` |
| **V-05** | ✅ | 全部核实。active `bg-white/[0.10]`→0.07、hover `bg-white/[0.06]`→0.05、侧栏底 `bg-black/[0.15]`→0.07 → 选中/hover 仅差 0.02 透明度（1.049:1） | `MainWindow.tsx:248`（aside）、`:286-292`（`:288` outline-none、`:290` active、`:291` hover）；`index.css:643,654,675` |
| **V-06** | ✅ | 全部核实。`.btn` 基类只有 `disabled:cursor-not-allowed`；`.btn-secondary/.btn-danger/.btn-ghost` 无任何 disabled 样式；Pill/Switch 用 `opacity-40` → 三种表达成立 | `index.css:182`、`:185-188`、`:189-192`、`:193-195`、`:196-198`、`:712`；`ui.tsx:189`、`:277`；调用 `HistoryPage.tsx:221,228,239,245`、`PermissionCenter.tsx:149,179,209` |
| **V-07** | ⚠️ | **裁定：Phase 2 正确**（浅色不受影响）。深色下 Check `text-accent-500`(#E8A33D) 与 `.menu-item:hover` 底 `bg-accent-500`(#E8A33D) 同色 → **1.0:1，勾记完全消失** ✓；浅色下 `.light [class~="text-accent-500"]`→`#8a5816`，与 hover 底 **2.79:1**（<3:1，属不合规而非「消失」）。**最终分级：深色 🔴 / 浅色 🟡** | `ClipContextMenu.tsx:41`（Check）、`:158,184-189,201,237`；hover 底 `index.css:334-337`；反色规则 `index.css:348-361`；浅色映射 `index.css:623-625` |
| **V-08** | ✅ | 全部核实。hint `text-white/[0.28]`、icon `/30`、title `/45`、Spinner `aria-hidden` 无文字、状态栏错误 chip 单行 truncate | `ui.tsx:592,593,594,600-619`（`:608` aria-hidden）；`Panel.tsx:379-381,390,418`；`HistoryPage.tsx:292-293,326`；`StatusBar.tsx:48,63-75` |
| **V-09** | ✅ | 全部核实。**补强**：`.light` 映射表里 **15 条规则在 JSX 中已无人使用**（`text-white/65`、`bg-white/[0.02]`、`bg-white/[0.07]`、`border-white/12`、`border-white/[0.15]`、9 条 `hover:*`）→ 映射表已与实际用法脱节，维护成本在上升 | 映射表 `index.css:597-613`；承载点 `StatusBar.tsx:48,81`、`ClipItem.tsx:254,259,277`、`ui.tsx:299,349,238`、`ClipDetail.tsx:104,142`、`MainWindow.tsx:267,275,308`、`HistoryPage.tsx:202` |
| **V-10** | ✅ | **white 家族 0 漏类成立**（脚本全量差集：只剩 `bg-white`，仅 `InputHud.tsx` 使用，而 HUD 在 `App.tsx:59` 被排除 `.light`，无实际影响）。**但漏洞平移的范围比 Phase 1 说的更大**：语义色家族共 **26 个** `bg/border/ring/stroke` token 未映射。**Phase 1 的三项「待验证」全部结清**：① `RingEditor.tsx:183` 的 hover 反色**不构成缺陷**（带 `on-dark` + `bg-black/55` 深底，hover 的 #E8A33D 在深底上对比充足；且环形盘窗口本身排除 `.light`，主窗口设置页里的 RingEditor 已由 `--ring-*` 变量适配）；② 见 V-12；③ 见 V-14 | 映射表 `index.css:597-688`；26 个漏类清单见 §3 E-10；`RingEditor.tsx:183`；`App.tsx:56-62` |
| **V-11** | ✅ | 全部核实（4+ 种选中表达 + 注释与实现矛盾）。**升级一项**：`HistoryPage.tsx:178` 的多选勾选框 `border-accent-500 bg-accent-500` 在浅色下对卡片底只有 **2.07:1** → **已勾选项在浅色下几乎看不出勾**（与未勾选的 `border-white/40` 差别不足）。这比「语言不统一」更实质 | `index.css:306-309`、`:223-225`、`:233-235`、`:334-337`；`HistoryPage.tsx:178,189`；矛盾注释 `index.css:307` |
| **V-12** | ⚠️ | 三段 28/flex-1/30、`line-clamp-[9]`、`canExpand`(>160 字)、`max-w-[112px] shrink-0`、`w-[6px]` 全部核实。**一条要删**：❌「短于 160 字但被 9 行截断的内容没有入口」**不成立** —— 9 行 × 18px ≈ 162px 可见高，12px 中文约 13 字/行 ≈ 130 字符 < 160 阈值 ⇒ 凡是会被 9 行截断的文本必然 >160 字，一定有入口。**一处表述要改**：`:254` 是 `shrink-0`，所以被牺牲的是**来源名**（`:259` `ml-auto min-w-0 truncate`）而不是时间戳。**一条要补**：`TrayPanel.tsx:2141` 仍是 `text-[10px]` | `ClipItem.tsx:141,148,156,194-196,243,254,259,266,271,349`；`HistoryPage.tsx:176`（10px）；`TrayPanel.tsx:2141`（10px）；`ui.tsx:7`（注释与代码不符）；线宽 `MainWindow.tsx:168`、`ClipItem.tsx:300,321,340,363`、`ui.tsx:230,468`、`ClipContextMenu.tsx:44,62`、`HistoryPage.tsx:183`、`Panel.tsx:31,345` |
| **V-13** | ✅ | 全部核实（材质 alpha、`.menu` 的 backdrop-filter 与 `.app-surface--panel` 注释矛盾、reduced-transparency 分支不关菜单 backdrop-filter） | `index.css:52-53`、`:115-116`、`:326-327`、`:149-151`、`:159-166` |
| **V-14** | ✅ | 确认。`apply()` 只排除 `winKind.ring`(`:58`) 与 `winKind.hud`(`:59`)，**确实没有排除 `winKind.tray`**。**影响评估修正**：托盘面板浅色下变成 `rgba(248,248,250,0.70)` + 深色文字 → 文字对比度没问题；真问题是它与「环形盘 / 输入法 HUD 恒为深色」的既定规则不一致，且 `TrayPanel.tsx:2141` 的 `text-[#ff9f0a]` / `text-[10px]` 在浅色下不协调 | `App.tsx:56-62`（Phase 1 引 `:57-61`，实际 `:56-62`）；`TrayPanel.tsx:2131`、`:2135`（行号精确） |

### 1.2 可用性 U-01 ~ U-18

| 编号 | 判定 | 修正说明 | 最终行号 |
|---|---|---|---|
| **U-01** | ✅ | **完全成立**，且我把分支顺序整条复现了：composing(`:72`) → Escape(`:74`) → ⌘F(`:76`) → `isTyping && !isSearch`(`:79`) → dialogOpen(`:82`) → ⌘1~9(`:85`) → Enter(`:92`) → ⌘C(`:101`) → ⌘D(`:108`) → ⌘⌫(`:113`) → **`isTyping` → ignore(`:120`)** → 方向键(`:122-134`)。面板唤出即聚焦也确认成立：根节点 `key={shownKey}`(`:453`) 整棵重挂载 → `:174` 的 `focusSearch()` 落在**即将被丢弃的旧 input** 上，真正生效的是 `:140-143` 的 layout effect。焦点在搜索框时 `isTyping=true,isSearch=true` → `:79` 不放行 → 落到 `:120` 被 ignore ⇒ 方向键被输入框吃掉。首启引导 `FirstRunGuide.tsx:83` 写着「← → 选择条目」→ 自相矛盾成立。**⚠️ 修此项必须同步改 `panelKeys.test.ts:201-206`**（现有用例把「搜索框内方向键=ignore」固化成期望） | `panelKeys.ts:79,120,122-134`；`Panel.tsx:140-143`、`:174`、`:198-225`（`isSearch` 判定 `:201`）、`:453`；`FirstRunGuide.tsx:83`；`panelKeys.test.ts:201-206` |
| **U-02** | ✅ | **完全成立**，并补齐了浏览器行为这一环：capture 阶段（`:45`）的 `e.preventDefault()` 会**阻止按钮的原生激活**（Enter on button 的 click 是 keydown 的默认动作），`e.stopPropagation()` 又让事件到不了按钮 ⇒ **焦点在「取消」上按 Enter 时，取消按钮的 onClick 根本不会触发，直接执行确认**。破坏性最高的删除/清空/退出正好命中 | `DialogHost.tsx:28-47`（`:38-42` Enter→`closeDialog(true)`、`:45` capture 注册）、`:50-55`（`:53` `cancelRef.current?.focus()`）、`:124-133`（取消键） |
| **U-03** | ✅ | 全部核实。卡片 `role="group" tabIndex={0}` **无 `aria-selected`/`aria-current`**；列表容器是普通 div，无 `role="listbox"`/`aria-activedescendant`；浅色选中态只有 `border-color: var(--accent-500)` 而 `:root.light` 未覆盖该变量 | `ClipItem.tsx:200-204`、`:238`；`VirtualList.tsx:53`、`:124`；`index.css:306-312`、`:742-746`、`:85-126` |
| **U-04** | ✅ | 全部核实。9 个导航按钮 `outline-none` 且 **index.css 里没有任何一条 focus 规则覆盖裸 `<button>`**（只有 `.btn/.btn-icon/.seg-item/.pill/.switch/.input/.slider`）；主窗口打开时无元素自动获得焦点 | `MainWindow.tsx:281-297`（`:288`）、`:191-198`；`index.css:173,183,207,221,231,245,281-284` |
| **U-05** | ✅ | 全部核实。**但 Phase 2 的 A12 判定有误**：WCAG 2.2 AA 的 2.5.8 Target Size (Minimum) 要求 **24×24 CSS px**，44×44 是 2.1 AAA 的 2.5.5。24×24 **恰好满足 AA** → A12 应判 ✅（不是 ⚠️）。其余（无焦点环、`title` 而非语义化 `aria-label`）成立 | `ui.tsx:71-92`（`:80` `outline-none`、`h-[24px] w-[24px]`）；`IconBtnProps.title` 必填 `ui.tsx:52` |
| **U-06** | ✅ | 全部核实（历史页无方向键 / Home / End / ⌘F，无 roving tabindex）。**补两点**：① `HistoryPage.tsx:91-98` 的 Esc 只在多选态生效，**历史页没有任何 Esc 语义**；② Phase 2 的「18 次 Tab」算式偏保守 —— `canExpand` 为 false 时是 14 次 | `HistoryPage.tsx`（全文）、`:91-98`；`ClipItem.tsx:201-202`、`:287-369` |
| **U-07** | ✅ | 全部核实。`role="checkbox" tabIndex={0}` 外壳 + 内层 `article tabIndex={0}` → **每张卡片 2 个 Tab 停靠点**；4 个按钮传空回调但仍可见、仍在 Tab 序列；`onClickCapture` + `stopPropagation` → 点图标只会切勾选 | `HistoryPage.tsx:139-174`（`:154`、`:155-161`、`:169-173`、`:143-151`）；`ClipItem.tsx:202` |
| **U-08** | ✅ | 全部核实。Esc 在 `:74` 无条件 close，`:79` 的 isTyping 放行在其之后，且 `isSearch` 不参与判断 → 搜索框里有内容时按 Esc 仍直接收起面板并丢掉关键词 | `panelKeys.ts:70-79`；`Panel.tsx:230-233`；`panelKeys.test.ts:38`（**修此项须同步改该用例**） |
| **U-09** | ✅ | 现象成立（DialogHost 有初始焦点、无归还，连 `previouslyFocused` 都没记）。**修正 Phase 2 的后果推断**：面板的判定是 `target === searchRef.current` → `isSearch`(`Panel.tsx:201`) 与 `isTyping`(`:202-207`，只看 tagName) **两个独立布尔**。对话框关闭后焦点落到 `<body>` ⇒ `isTyping=false,isSearch=false` ⇒ **方向键其实仍可用**，但 Enter 会走 `pasteSelected`（回贴当前选中）而不是搜索框，⌘F 仍可救回。正确表述：「唤出即搜」这条心流在一次删除之后被打断（Enter 语义漂移），而不是「按键全部无效」 | `DialogHost.tsx:50-55`；对照 `ClipDetail.tsx:36`、`:80`；`Panel.tsx:201-207` |
| **U-10** | ✅ | 全部核实。`role="dialog" aria-modal="true"` 但无 focus()、无 trapTab、无归还；遮罩 `absolute inset-0` → 背景侧边栏仍可 Tab | `FirstRunGuide.tsx:61-113`（`:63`、`:70-71`、`:41-51`）；对照 `DialogHost.tsx:67-83`、`ClipDetail.tsx:51-75` |
| **U-11** | ✅ | 全部核实。删除 toast 未指定 tone → 走默认 **2400ms**；`removeMany` 同样；文案却承诺「一键撤销」；toast 位置在管理窗口右下 / 面板底部居中 | `toast.ts:75`；`useClipStore.ts:351-354`、`:371-375`、`:346-359`、`:361-382`；`HistoryPage.tsx:107`；`ToastHost.tsx:25` |
| **U-12** | ✅ | 全部核实（无焦点移入、无 ↑/↓、无 roving、子菜单靠 `onMouseEnter`）。**⚠️ 关联升级见 E-01**：菜单在面板窗口里高约 235px，而面板窗口只有 220–233px → **「删除」项被窗口边界裁掉**（两个阶段都没发现） | `ClipContextMenu.tsx:100-113`、`:115-132`、`:141-160`、`:196-197`、`:214-240`、`:166`（`fixed inset-0 z-50`）、`:169` |
| **U-13** | ✅ | 全部核实。Spinner `aria-hidden` 无文案；空/错误容器无 `role="status"`；错误文案 `max-h-[48px] overflow-hidden` 硬裁、无 tooltip、无复制 | `ui.tsx:600-619`（`:608`）、`:592-594`；`Panel.tsx:379-382`、`:390-393`；`HistoryPage.tsx:292-294`、`:298-301` |
| **U-14** | ✅ | 全部核实。`SliderRow` 只有 `aria-label`，无 `aria-valuetext`；两处调用确实一个显示 `%` 一个显示 `formatMb` | `ui.tsx:355-364`（`:363`）；`Settings.tsx:331-339`、`:413-421` |
| **U-15** | ⚠️ | **代码路径成立**，但需实测 WebKit：`<input>` 内部选区不进入 `document.getSelection()`（标准行为）→ 搜索框里选中文字按 ⌘C 时 `hasTextSelection=false` → 走 copy 分支 → `e.preventDefault()` 取消原生复制 → 整条记录进剪贴板。**风险点**：部分 WebKit 版本在 input 聚焦时 `window.getSelection()` 可能返回 input 的选中文本 → 行为因版本而异 | `panelKeys.ts:101-106`；`Panel.tsx:219` |
| **U-16** | ✅ | 与 V-02 同源，**统一表述**：徽标**文字可读**；不可见的是底色与色点。**Phase 1 给 🔴 偏高，实际 🟡** | `StatusBar.tsx:54`（`bg-warning/[0.14]` + `text-warning`）、`:57`（点）；`PermissionCenter.tsx:30`；`ContextMenuSettings.tsx:124,149`；`tailwind.config.ts:36-37`；`index.css:85-126`、`:614-617` |
| **U-17** | ✅ | 全部核实。`apply_panel_layout` 每次唤出都 `set_position` → 拖动的位置被重置；管理窗口则由 window-state 持久化 → 行为不一致成立。**行号修正**：`window.rs:242`（Phase 2 写 238-241）、`:1032`（Phase 2 写 1033）、`:1087-1088`（set_size/set_position，Phase 2 写 1086-1087） | `Panel.tsx:459-462`；`window.rs:237-242`、`:1032-1089`、`:1087-1088`；`lib.rs:197-204` |
| **U-18** | ❌ | **不成立，已用依赖源码证伪。** Tauri **2.12.5**（Cargo.lock 锁定）注入脚本 `tauri-2.12.5/src/window/scripts/drag.js`：`isDragRegion(composedPath)` 从事件目标向上遍历，遇到 **CLICKABLE_TAGS（A/BUTTON/INPUT/SELECT/TEXTAREA/LABEL/SUMMARY）且自身没有 `data-tauri-drag-region`** 的元素立即 `return false`（drag.js:32-40, 51-70）⇒ 面板 header 里的 `<input>`（SearchField）、`<button>`（Segment 项、两个 IconBtn）**自身都没有 drag 属性**，在走到 header 之前就 return false ⇒ **不会被当成拖拽**。且 header 上的属性是**裸属性**（值 ""），语义是 "self"：只有直接点 header 元素本身才触发拖拽（drag.js:66）。`Panel.tsx:479` 的空格隔同样带裸属性 → 点它可以拖窗口（设计意图）。→ **U-18 与 R-16 可关闭，无需实机验证** | 反证：`~/.cargo/registry/src/mirrors.ustc.edu.cn-38d0e5eb5da2abae/tauri-2.12.5/src/window/scripts/drag.js:11-70,78-106`；`Cargo.lock` tauri 2.12.5；`Panel.tsx:459-462`、`:479` |

### 1.3 响应式 R-01 ~ R-16

| 编号 | 判定 | 修正说明 | 最终行号 |
|---|---|---|---|
| **R-01** | ✅ | **数字全对，复算通过**。chrome = 44+34+36 = 114；面板 220 → 卡片流 106 → 卡片 82（再扣 `py-[12px]`×2）→ 预览 24px → 再扣 `py-[4px]`×2 = **可见文字 16px < 行高 18px ⇒ 不足 1 行** | `window.rs:1056-1060`；`Panel.tsx:460,511,535`；`TagBar.tsx:26`；`StatusBar.tsx:48`；`VirtualList.tsx:62`；`ClipItem.tsx:243,266,271` |
| **R-02** | ⚠️ | **机制成立，数字错**。纵向板型卡片高**写死 220**（不是随面板走），比较对象应是「卡片流高 = 面板高 − 114」：220→106(差114)、233→119(差101)、287→173(差47)、**328→214(只差 6)**、425→311(放得下)。⇒ 结论应改为「**面板高 ≤ 338px** 时底部 30px 操作条被裁；≤ 287px 时整条不可见」。Phase 2 用的「149px」是横向算法，套错了。严重度 🔴→🟠（默认板型是横向）；但「rowHeight 写死」这个根因保留 | `Panel.tsx:517-524`（`:521` 168、`:522` 220、`:523` `h-[220px] w-full`）；`VirtualList.tsx:114` |
| **R-03** | ❌ | **结论不成立** —— 宽度链**重复扣了一次内边距**。`HistoryPage.tsx:270` 的 `px-[24px]` 是**工具栏自己的内边距**，而卡片列表容器 `<div className="min-h-0 flex-1">`(`:342`) 是它的**兄弟节点**、没有横向内边距。正确链：**420 − 侧边栏 200 − 边框 1 = 219 → VirtualGrid padding 24×2 = 48 → inner = 171px**（非 124px）⇒ `columns = max(1, floor(183/212)) = 1` → **列宽 171px**。底部条最小宽度复算：`px-10×2(20) + Kbd占位(20，**可收缩到 0**，因 `ClipItem.tsx:275` 无 shrink-0) + gap6 + 体积文本(≈38, shrink-0) + gap6 + 4×24(96) + gap2×3(6) + 6px 间隔(6)` ⇒ 实际最小需求 **134px ≤ 内容盒 149px** ⇒ **「删除」按钮在最小宽度下完整可见**，「缺口 72px / 被完整裁掉」不成立。**真正发生的退化**：① 来源 App 名被压到 0–28px（`ClipItem.tsx:259`）；② 跨年时间戳（`formatRelativeTime` 最长「2025年3月1日 08:00」≈96px，`max-w-[112px] shrink-0`）会把 16px 来源图标挤出 `.card-clip` 的 `overflow-hidden`；③ 「最小列宽 200」承诺失效（= R-09，成立）。严重度 🔴→🟠 | `tauri.conf.json:19`；`MainWindow.tsx:248`、`:319`；`HistoryPage.tsx:270`、`:342`、`:343-350`（`:346` 200、`:348` padding 24）；`VirtualList.tsx:107-108`、`:136`；`ClipItem.tsx:271-371`（`:275` Kbd 占位、`:349` w-[6px]）；`index.css:299` |
| **R-04** | ⚠️ | **成立且比 Phase 2 说的更严重**。工具栏最小宽度按代码实算 = `px-16×2(32) + SearchField 220 + gap8 + .seg 434 + gap8 + 空格 8 + gap8 + IconBtn 30 + gap8 + IconBtn 30` = **786px**（`.seg` 6 项 × (px-12×2 24 + 标签 24 + 计数 22) + 5×gap2 + p-2×2 = 434）。`.seg` 是 `inline-flex` 且**无 `min-w-0`、无 `flex-wrap`**，作为 flex item 的 `min-width:auto` ≈ 362–434px ⇒ 基本不可压缩。面板宽 < ~726px 时 header 溢出，被 `.app-surface`(`index.css:143` `overflow-hidden`) 裁掉 ⇒ **右侧「清空历史」「设置」两个按钮被整体推出可视区**（Phase 2 说的是「筛选项画到按钮下面」）。面板宽 = `avail_w × 0.6`：13"(1280)→768（勉强够，搜索框被压到 ≈202px）；13"@150%(1008)→605（**缺 ≈120px，两个图标按钮消失**）。默认 `board_position="bottom"` 时宽 = 屏幕全宽 → 不复现 ✓ | `Panel.tsx:459-509`（`:460` header、`:467` w-[220px]、`:470-478` Segment、`:480`、`:503` IconBtn、`:479` 空格隔）；`filters.ts:18-25`；`ui.tsx:214-216`（`.seg`）、`:217-222`（`.seg-item`）；`index.css:143`；`window.rs:1093-1097`；`settings.rs:747-749` |
| **R-05** | ✅ | **数字全对**。13"(avail_h=775)：20%→155→**220**、25%→194→**220**、30%→233 ⇒ 用户把滑块从 20 拖到 25 **面板一点没变** ✓。**补一条同源事实**：`patch_config`(`commands/config.rs:14-27`) **不调用 `apply_panel_layout`**（该函数只在 `show_panel_window` 里被调，`window.rs:242`）⇒ 改完滑块必须**重新唤出面板**才看到新高度 | `window.rs:1056-1060`；`Settings.tsx:331-339`；`settings.rs:743-745`（默认 30；Phase 2 写 740-742，**偏 3 行**）；`commands/config.rs:14-27` |
| **R-06** | ✅ | 全部核实（`md:grid-cols-2` 确实存在于 `:315` 与 `:411`，我用 grep 复核过 —— Phase 2 这条是对的）。复算 768px 窗口：内容 567 → −48 = 519 → 两栏各 251.5 → 减 CardGroup p-32 = **219.5px** → 标签只剩 **83.5px**（「粘贴前去除尾部空白」9 汉字 ≈117px 被截）✓。**补一条 Phase 2 漏掉的、更普遍的截断**：`MainWindow.tsx:326` 的 `max-w-[760px]` ⇒ 两栏每栏 ≤372px ⇒ 控件 120–140px 时标签最多 372−32−140−16 = **184px** ⇒ **即使在默认 900px 窗口下长标签也可能被截** | `Settings.tsx:315`、`:327`、`:350`、`:411`、`:483`；`MainWindow.tsx:248`、`:319`、`:325-326`；`ui.tsx:296-302`、`:512-530` |
| **R-07** | ⚠️ | 结论方向对，**数字错**。Phase 2 说「窗口 460 → 分段只剩 64px」与它自己描述的 `flex-wrap` 机制矛盾：**分段会换行到第二行，第二行整行可用**。正确：窗口 420 → 工具栏可用 171px → 分段换到第二行、宽 **171px**（内容 434px，横向滚动，可见 ≈2.5/6 项）；窗口 460 → 可用 211px → 分段宽 **211px**（可见 ≈3/6 项）。结论（退化成窄滚动缝）成立，严重度不如 Phase 2 描述 | `HistoryPage.tsx:270`（flex-wrap）、`:275`、`:279`（`min-w-0 max-w-full overflow-x-auto`）、`:280-284`；`ui.tsx:214-216`（`.seg` 无换行能力） |
| **R-08** | ✅ | 全部核实。复算 420px：内容 219 − 48 = 171 − CardGroup 32 = **139px**；三个 sm 按钮 ≈ 68+44+92+2×8 = **220px** → **溢出 81px**，「打开系统设置」被 `<main>` 的 `overflow-hidden` 裁掉 ✓ | `PermissionCenter.tsx:137-160`（`:147-159`）；`ui.tsx:296-302`（`:301` shrink-0）、`:199-201`（`.btn-sm`）；`MainWindow.tsx:319`、`:248` |
| **R-09** | ✅ | 全部核实。`inner < minColumnWidth` 时 floor 得 0 → `max(1,…)` → 1 列，列宽 = inner < minColumnWidth ✓。**实测**：历史页 inner=171 < 200（差 29px）；面板纵向 inner < 168 同理 | `VirtualList.tsx:107-108`、`:136`；`HistoryPage.tsx:346`；`Panel.tsx:521` |
| **R-10** | ✅ | 现象成立，**翻页点与列宽数字要改**。翻页点由 `floor((inner+12)/212)` 决定 ⇒ **inner = 412 / 624 / 836 / 1048…**（Phase 2 写的「200」那个不存在：inner=200 时 floor(212/212)=1，仍是 1 列）。列宽 = `(inner − (cols−1)×12)/cols`（`gap:12` + `repeat(cols,minmax(0,1fr))`）。突变更点从 Phase 2 说的 668 修正为 **661**（列宽 411 → 200，卡片腰斩） | `VirtualList.tsx:107-108`、`:136`；`HistoryPage.tsx:346` |
| **R-11** | ✅ | 全部核实。**补一条**：历史页 `VirtualGrid` 传 `padding={24}`(`HistoryPage.tsx:348`)，面板纵向用默认 `padding={16}`(`VirtualList.tsx:86`) ⇒ **两个网格的横向留白也不同（24 vs 16）**，进一步加剧「同一份内容两副面孔」 | `Panel.tsx:521`、`:526-531`；`HistoryPage.tsx:346`、`:348`；`VirtualList.tsx:33`、`:86`；`index.css:299` |
| **R-12** | ✅ | 全部核实。`avail_w = work.size.width / scale_factor` ⇒ 缩放改变逻辑分辨率 ⇒ 面板宽变窄、高度被 `.max(220)` 兜住 | `window.rs:1042-1043`、`:1056-1060`、`:1093-1097`；`Settings.tsx:331-339` |
| **R-13** | ⚠️ | 现象成立，**数字要改**。`line-clamp-[9]` × 18px = **162px**；面板 655 → 预览 **459px** → 空白 **297px**（Phase 2 正文 297 ✓，但 §0.3 表格那格写「259px 空白」是错的）。面板 425 → 预览 229 → 空白 67px | `ClipItem.tsx:141`、`:148`、`:156`、`:266` |
| **R-14** | ✅ | 全部核实。Rust 允许 10–60（`clamp(10,60)`），UI 只给 20–60；而 10–19% 经 `.max(220)` 后**实际高度仍恒为 220** ⇒ 该段完全不可达 ✓ | `window.rs:1057`、`:1059`；`Settings.tsx:333-336` |
| **R-15** | ✅ | 全部核实。chrome 复算：页头 20+22+10=52 + 工具栏 28+10=38 + 标签行 34 + 状态栏 36 = **160px** ⇒ 卡片流 200px；卡片 220px ⇒ **底部 20px 被切**（30px 操作条只剩 10px）✓ | `tauri.conf.json:20`；`HistoryPage.tsx:200`、`:270`、`:289`、`:354`；`StatusBar.tsx:48`；`ClipItem.tsx:271` |
| **R-16** | ❌ | 同 U-18 → **不成立**（Tauri 2.12.5 `drag.js` 源码证伪，见 U-18 行） | 反证同 U-18 |

**统计：✅ 36 条 ｜ ⚠️ 8 条 ｜ ❌ 4 条（R-03、R-16、U-18，以及 V-12 中的一条子结论）**

---

## 2. 关键量化结论复算

### 2.1 历史页可用宽度链（R-03 / R-09 / R-10 的更正）

```
窗口宽 W
 └─ 侧边栏 w-[200px] shrink-0 + border-r 1px      (MainWindow.tsx:248)
    └─ 内容区 = W − 201
       └─ 卡片列表容器 <div class="min-h-0 flex-1"> 无横向内边距 (HistoryPage.tsx:342)
          └─ VirtualGrid padding = 24 → inner = (W − 201) − 48 = W − 249
             └─ columns = max(1, floor((inner + 12) / (200 + 12)))   (VirtualList.tsx:108)
                └─ 列宽 = (inner − (columns−1)×12) / columns
```
> ❌ Phase 2 的链在「减去工具栏 px-[24px]」处多扣了 48px（工具栏内边距不约束兄弟节点）。

| 窗口 W | 内容区 | inner | 列数 | **列宽** | Phase 2 所载 |
|---|---|---|---|---|---|
| **420（最小）** | 219 | **171** | 1 | **171px** | 124px ❌ |
| 500 | 299 | 251 | 1 | 251px | 204px ❌ |
| 660 | 459 | 411 | 1 | 411px | 200px ❌（其自身公式也算不出 2 列） |
| **661** | 460 | **412** | **1→2** | **411 → 200px** ⚡突变 | 突变点写 668 ❌ |
| 900（默认） | 699 | 651 | 3 | (651−24)/3 = **209px** | 2 列 296px ❌ |
| 1200 | 999 | 951 | 4 | (951−36)/4 = **228.75px** | 4 列 229px ✅ |
| 2000 | 1799 | 1751 | 8 | (1751−84)/8 = **208.4px** | 8 列 208.5px ✅ |

**列数翻页点**：inner = **412 / 624 / 836 / 1048 …**（Phase 2 的「200」不存在）。

### 2.2 卡片底部操作条的最小像素宽度（R-03 的核心）

卡片宽 171px → `.card-clip` 边框 1×2 → 内容盒 = 171 − 2 − 20(`px-10`) = **149px**

| 元素 | 宽 | 可否收缩 |
|---|---|---|
| Kbd 占位 `<span class="w-[20px]" aria-hidden/>` | 20 | ✅ **可收缩到 0**（无 `shrink-0`） |
| gap | 6 | — |
| 体积文本 `shrink-0 text-[11px] tabular-nums` | ≈38（`formatSize` 输出如 "1.25 KB"） | ❌ |
| gap | 6 | — |
| 操作区 `ml-auto flex gap-[2px]`：收藏 24 + 复制 24 + 6px 间隔 + 删除 24 | 84（+3×gap2=6 → 90） | ❌ 子按钮 `shrink-0` |
| **合计最小需求** | **0+6+38+6+90 = 140px ≤ 149px** | ✅ **放得下** |

> ✅ 结论：**最小窗口宽 420px 时，「删除」按钮完整可见**。Phase 2 的「缺口 72px / 删除按钮被完整裁掉」**不成立**。
> ⚠️ 但 `canExpand` 为真时操作区变 5 个按钮（+24）⇒ 140+24 = 164px > 149 ⇒ **「查看全文」+「收藏」+「复制」+「删除」四个按钮同时存在时，最右侧会溢出约 15px**（文件类或 >160 字长文本必触发）。这才是窄窗下真正会丢按钮的场景，Phase 1/2 均未发现。

### 2.3 面板固定 chrome 与预览区高度（R-01 / R-02 / R-13）

```
固定 chrome = 工具栏 44 (Panel.tsx:460) + 标签行 34 (TagBar.tsx:26) + 状态栏 36 (StatusBar.tsx:48) = 114px
（无标签时 TagBar 返回 null ⇒ chrome = 80px）
```

| 面板高 | 卡片流高（−114） | 横向卡片高（−24 py） | 预览区高（−58） | 可见行数（18px） | 纵向板型（rowHeight=220） |
|---|---|---|---|---|---|
| 220（下限） | 106 | 82 | **24px**（再扣 py-4 ⇒ 16px < 18px） | **0.x 行** | 差 114 → 操作条整条不可见 |
| 233（13"@30%） | 119 | 95 | 37px | 2.0 | 差 101 → 整条不可见 |
| 287（14"@30%） | 173 | 149 | 91px | 5.0 | 差 47 → 整条不可见 |
| 328（16"@30%） | 214 | 190 | 132px | 7.3 | 差 6 → **只切 6px** |
| 425 | 311 | 287 | 229px | 9 行（clamp 封顶，空白 67px） | 放得下 |
| 655（16"@60%） | 541 | 517 | 459px | 9 行 → **空白 297px** | 放得下 |

> 复核结论：R-01 数字全对；**R-02 的「卡片比视口高 71–114px」只在面板高 ≤287px 成立**，328px 时只差 6px；R-13 空白是 **297px**（Phase 2 §0.3 表格里的 259px 是笔误）。

### 2.4 Rust 侧高度 clamp 与滑块语义（R-05 / R-14）

```rust
// window.rs:1056-1060
let max_height = (avail_height - PANEL_MARGIN * 2.0).max(220.0);   // PANEL_MARGIN = 24.0 (:37)
let height = (avail_height * config.board_height_scale.clamp(10, 60) as f64 / 100.0)
    .round().max(220.0).min(max_height);
```
| avail_h | 20% | 25% | 30% | 60% | 真实生效区间 |
|---|---|---|---|---|---|
| 775（13"） | 155→**220** | 194→**220** | 233 | 465 | **仅 ≥30% 有区分度** |
| 957（14"） | 191→**220** | 239 | 287 | 574 | 20% 是死区 |
| 1092（16"） | 218→**220** | 273 | 328 | 655 | 20% 仍被兜到 220 |
| 1415（2560×1440） | 283 | 354 | 425 | 849 | 全部有效 |

- 语义三处不一致：前端滑块 **20–60**（`Settings.tsx:333-336`）／Rust `clamp(10,60)`（`window.rs:1057`）／物理下限 **220**（`window.rs:1059`）⇒ 10–19% 段**完全不可达**。
- 补：`patch_config`（`commands/config.rs:14-27`）**不调用** `apply_panel_layout` ⇒ 改完滑块要**重新唤出面板**才生效。

### 2.5 面板工具栏最小宽度（R-04）

```
px-16×2 (32) + SearchField 220 + gap 8 + .seg 434 + gap 8 + 空格 8 + gap 8 + IconBtn 30 + gap 8 + IconBtn 30 = 786px
.seg = 6 × (px-12×2 24 + 标签 24 + 计数 22) + 5×gap2 + p-2×2 = 434px
```
`.seg` 是 `inline-flex`、无 `min-w-0`、无 `flex-wrap` ⇒ 作为 flex item 的 `min-width:auto` ≈ 362–434px，**基本不可压缩** ⇒ header 的最小内容宽 ≈ **726px**。

| 面板宽 | 来源 | 结果 |
|---|---|---|
| 768 | 13"(1280)×0.6 | 勉强装下，搜索框被压到 ≈202px |
| 605 | 13"@150%(1008)×0.6 | **缺 ≈120px ⇒「清空历史」「设置」两个按钮整体被推出可视区** |
| 屏幕全宽 | 默认 `board_position="bottom"`（`settings.rs:747-749`） | 不复现 |

### 2.6 U-01 / U-02 的真实行为链

**U-01**：`panelKeys.ts` 实际顺序 —— composing(72) → Escape(74) → ⌘F(76) → `isTyping && !isSearch`(79) → dialogOpen(82) → ⌘1~9(85) → Enter(92) → ⌘C(101) → ⌘D(108) → ⌘⌫(113) → **`isTyping`→ignore(120)** → 方向键(122-134)。
面板唤出即聚焦：`Panel.tsx:453` 根节点 `key={shownKey}` 整棵重挂载 → `:174` 的 `focusSearch()` 作用于**即将被丢弃的旧 input**（无效）→ 真正生效的是 `:140-143` 的 `useLayoutEffect([shownKey])`。
⇒ 焦点在搜索框时：`isTyping=true` 且 `isSearch=true` ⇒ `:79` 不放行 ⇒ 落到 `:120` 被 ignore ⇒ **方向键 100% 被输入框吃掉**，U-01 成立。

**U-02**：`DialogHost.tsx:45` capture 注册 → `:38-42` Enter → `closeDialog(true)`。capture 阶段的 `preventDefault()` **阻止按钮原生激活**（Enter on button 的 click 是 keydown 的默认动作）⇒ 焦点在「取消」(`:53` 聚焦) 上按 Enter 时，**取消按钮的 onClick 不会触发，直接执行确认**。U-02 成立。

---

## 3. 工程层新增发现（E-01 ~ E-20）

> 以下均为 Phase 1/2 未覆盖、由本次工程复核独立挖出的问题。

### E-01 🔴 面板窗口高度装不下右键菜单，「删除」项被窗口边界裁掉
- **现象**：`ClipContextMenu` 是 `position: fixed`（`ClipContextMenu.tsx:166-169`），在**面板窗口**里菜单高度 ≈ 7×28（菜单项）+ 3×9（分隔线）+ 10（padding）+ 2（border）= **235px**，而面板窗口默认只有 **220–233px** 高。`useLayoutEffect`(`:100-113`) 的 `maxTop = window.innerHeight − h − 8` 为负 → 被钳到 `top = 8` ⇒ 菜单底边到 243px，**超出窗口 10–23px** ⇒ 最下面的「删除」项被裁掉。
- **证据**：`ClipContextMenu.tsx:166`、`:169`、`:100-113`、`:176-282`；`index.css:330-337`（`.menu-item` h-[28px]）、`:344-346`（`.menu-sep` 9px）；`tauri.conf.json:39`（面板高 220）、`:41`（minHeight 220）；`window.rs:1059`。
- **影响**：面板是主心流场景，用户右键 → 想「删除」→ 按钮在窗口外，点不到；标签子菜单展开后更甚。
- **建议**：面板内禁用右键菜单的「删除」入口（删除已由 ⌘⌫ 与卡片按钮提供），或把菜单改为向上展开（`bottom` 锚定），或面板高度 < 300px 时把菜单渲染到面板**外**（需要 Rust 侧做一个独立的菜单窗口）。

### E-02 🔴 ClipDetail 的 capture 阶段 Enter 不区分焦点按钮 → 在「复制」上按 Enter 会**回贴**
- **现象**：`ClipDetail.tsx:40-50` 在 window capture 阶段拦截 Enter 并无条件 `onPaste()`；`e.preventDefault()` 会**阻止焦点按钮的原生激活** ⇒ 焦点在「复制」按钮(`:146`)或「关闭」按钮(`:106`)上按 Enter，不是复制/关闭，而是**把内容回贴到前台 App**（不可逆的对外操作）。
- **证据**：`ClipDetail.tsx:40-50`、`:77`、`:106`、`:146-151`。
- **建议**：Enter 只在 `document.activeElement` 不是按钮时生效；按钮各自用 Space 激活。

### E-03 🟠 「查看全文」浮层会被列表刷新静默关闭
- **现象**：`detailOpen` 状态挂在**虚拟化的** `ClipItem` 上（`ClipItem.tsx:181`、`:373-386`）。新剪贴板条目前插会让 `items` 变化 ⇒ `key={item.id}`（`Panel.tsx:357`）的卡片重挂载 ⇒ `detailOpen` 状态丢失 ⇒ **全文浮层自己消失**。剪贴板管理器里新条目随时会来，用户正在读长文时窗口就没了。
- **证据**：`ClipItem.tsx:181`、`:373-386`；`useClipStore.ts:248-264`（`applyClipboardChanged` 前插）；`Panel.tsx:354-372`（`key={item.id}`）。
- **建议**：把「全文浮层」提到 `Panel`/`HistoryPage` 层，用 `selectedClipId` 驱动，与列表生命周期解耦。

### E-04 🟠 新剪贴板条目前插 → 面板选中项静默错位 → Enter 回贴错条目
- **现象**：`selected` 是**下标**（`Panel.tsx:68`），`items` 前插后 `items[selected]` 指向另一条记录。`Panel.tsx:118-120` 只按 `items.length` 做钳制，**不做下标→id 的重映射** ⇒ 用户以为还选着第 4 条（高亮也移到了第 4 个位置），按 Enter 却贴了第 5 条。
- **证据**：`useClipStore.ts:255-257`（`[item, ...items]`）、`Panel.tsx:68`、`:112-120`、`:262-271`（`pasteSelected` 读 `items[selected]`）、`:296-301`。
- **建议**：`selected` 改存 `clipId`（或存 id+下标双轨），列表变化时按 id 重新定位。

### E-05 🟠 面板的 window keydown 监听每次按键都重建
- **现象**：`Panel.tsx:198-320` 的 `useEffect` 依赖数组含 `items`（数组引用）、`selected`、`hasFilter` 等 ⇒ **每按一次方向键（`selected` 变）、每刷新一次列表（`items` 换引用）都会 remove/add 一次 window 监听**。500 条列表下 `refresh()` 每次都换 `items` 引用 ⇒ 频繁解绑重绑。
- **证据**：`Panel.tsx:198-320`（deps 在 `:306-320`）、`:118-120`。
- **建议**：用 `useRef` 持有最新的 `items/selected`，监听只注册一次，handler 内读 ref。

### E-06 🟠 「唤出即聚焦」存在一次无效 focus 调用
- **现象**：`Panel.tsx:174` 的 `focusSearch()` 在 `handleShown` 里同步调用，此时根节点即将因 `key={shownKey}` 重挂载，**聚焦的是即将被丢弃的旧 input**。真正生效的是 `:140-143` 的 layout effect。`:174` 这行是无效代码，但注释在解释它，容易误导后续维护。
- **证据**：`Panel.tsx:140-143`、`:162`（`setShownKey`）、`:174`、`:453`。
- **建议**：删掉 `:174`，把「重挂载后再聚焦」的说明留在 `:137-143`。

### E-07 🟠 单包 420KB（gzip 132KB），5 个 webview 各自加载一份，零分包
- **现象**：`vite build` 产出**单一** chunk，涵盖 Panel / MainWindow / RingWindow / TrayPanel / InputHud / Settings 全部页面。Tauri 启动时 5 个窗口全部 `visible:false` 但已创建 ⇒ 5 份 webview 各自加载 420KB JS。
- **证据**：构建输出 `dist/assets/index-*.js 420.58 kB │ gzip: 131.80 kB`；`App.tsx:15-21`（5 个静态 import）、`:152-162`（按 winKind 渲染）。
- **建议**：`React.lazy(() => import("./windows/RingWindow"))` 等按 winKind 动态导入；RingWindow/TrayPanel/StatusBarSettings 是最重的三块，拆出去可省 ~40%。

### E-08 🟡 虚拟列表未接 `measureElement`，一旦改成自适应行高就会滚动漂移
- **现象**：`VirtualGrid` 只有 `estimateSize: () => rowHeight + gap`（`VirtualList.tsx:114`），没有 `measureElement` / `ref` 回调。当前 rowHeight 与真实行高一致（220 内容 + 12 gap = 232 步长）所以看不出问题；**但 R-02 的修复建议正是要把 rowHeight 改成随面板高度自适应** —— 那时估算与实际必然不等 ⇒ 滚动位置逐行漂移。
- **证据**：`VirtualList.tsx:111-121`、`:126-143`（行 div 没有 `data-index` + `ref`）。
- **建议**：R-02 的自适应改造必须与 `measureElement` 一起做，否则从「修好」变成「更坏」。

### E-09 🟡 网格模式下方向键只滚到「行」，被选中的那一列可能仍在视口外
- **现象**：`VirtualGrid` 的滚动是 `virtualizer.scrollToIndex(Math.floor(scrollToIndex / columns))`（`VirtualList.tsx:120`）—— 滚的是**行**，不是**列**。当列宽 < 视口宽时（历史页多列），被选中的卡片所在列可能在视口右侧之外，用户按 ↓ 后高亮跑到了看不见的地方。
- **证据**：`VirtualList.tsx:118-121`；对比 `VirtualStrip` 的 `:45-48`（滚到项）。
- **建议**：网格模式应额外做一次列内横向滚动（`scrollLeft`），或统一成「按项滚动」。

### E-10 🟡 语义色家族 26 个 `bg/border/ring/stroke` token 未进 `.light` 映射（V-02 的量化补强）
未映射清单（脚本全量扫描得出）：
```
bg-warning            bg-warning/[0.14]     bg-success-soft      text-danger/80
bg-danger-fill/10     bg-danger-fill/15      bg-danger-fill/[0.12] bg-danger/[0.10]
bg-success/[0.12]     border-danger-fill/45  border-success/40
bg-accent             bg-accent-tint         bg-accent/15         border-accent
border-accent/60      border-accent/70       border-danger/40     border-danger/50
border-danger/60      ring-accent            ring-danger/60       stroke-accent
```
命中位置（部分）：`StatusBar.tsx:54,57,66`、`PermissionCenter.tsx:30`、`ContextMenuSettings.tsx:124,149`、`ui.tsx:540,541,85`、`ToastHost.tsx:6,7`、`HistoryPage.tsx:178,189`、`MainWindow.tsx:251`、`ClipItem.tsx:78`（`bg-accent-500/35` 的高亮 mark）。
→ 建议一次性迁到 CSS 变量（`rgb(var(--warning) / <alpha-value>)` 形式），并加一条 CI 差集脚本（见批次 D）。

### E-11 🟡 `.light` 映射表里 15 条规则已无人使用 → 机制与实际用法脱节
`text-white/65`、`bg-white/[0.02]`、`bg-white/[0.07]`、`border-white/12`、`border-white/[0.15]`、9 条 `hover:*`（`hover:bg-white/10`、`/[0.05]`、`/[0.06]`、`/[0.07]`、`/[0.08]`、`/[0.10]`、`/[0.12]`、`hover:text-white`、`/60`、`/85`、`/90`）。
→ 映射表现在是「手写快照」，新增类名不会有人记得补。**建议改成脚本生成**（批次 D）。

### E-12 🟡 面板高度滑块与 Rust 的三处语义不一致，且改完不立即生效
见 §2.4。前端 20–60 / Rust 10–60 / 物理下限 220；`patch_config` 不重排面板。

### E-13 🟡 `Row` 的控件区 `flex shrink-0` 在窄内容区不可换行
`ui.tsx:301`。这是 R-08 的根因，也影响所有「标签 + 固定宽控件」的设置行。

### E-14 🟡 设置页的 `md:` 是视口断点，而内容区恒定少 200px + 48px
`Settings.tsx:315,411` + `MainWindow.tsx:248,326`。R-06 的根因。

### E-15 🟡 首启引导没有焦点管理，且遮罩是 `absolute`（不是 `fixed`）
`FirstRunGuide.tsx:63`。背景侧边栏仍可 Tab 到；读屏会同时遍历背景。

### E-16 🟡 默认 `board_position="bottom"` 让 R-04 在默认配置下不复现
`settings.rs:747-749`。⇒ R-04 是「用户一改设置就踩」的雷，回归测试要覆盖非默认位置。

### E-17 🟡 `TrayPanel.tsx:2141` 仍有 `text-[10px]`（低于 11px 字号底线）
另 `TrayPanel.tsx:2135` 的 `text-white` 在浅色下会被翻黑（V-14）。

### E-18 🟡 `useClipStore.ts:102` 的 `keywordTimer` 是模块级变量，组件卸载不清理
面板窗口常驻，影响小；但 StrictMode 双挂载会多打一次查询。

### E-19 🟡 500 条上限会**静默丢弃**最旧记录
`useClipStore.ts:223`（`limit: 500`）、`:257`（`.slice(0, 500)`）⇒ 第 501 条之后用户的历史**看不到也删不掉**，界面上没有任何提示。历史页也没有分页/无限滚动。剪贴板管理器每天几百条，这个上限正常用户几个月就会撞上。

### E-20 🟡 面板位置/尺寸的「记忆」与主窗口不一致
`window.rs:242` 每次唤出重置位置；而 `tauri-plugin-window-state`（`lib.rs:197-204`，`StateFlags::all()`）会为**所有窗口**持久化位置尺寸，包括面板 —— 但被 `apply_panel_layout` 覆盖。两套机制并存 ⇒ 行为不可预测（U-17 的根因）。

---

## 4. 可执行整改清单（按可独立提交的批次）

> **前置动作（批次 0）**：先 `git add -A && git commit` 提交当前工作区。
> 理由：工作区有 **601+ 行未提交改动**（`src/index.css` 净 +99 行、`App.tsx` +8、`MainWindow.tsx` +12、`Settings.tsx` +8），**本报告及前两阶段报告的全部 `文件:行` 只对当前工作区成立**；一旦回退到 HEAD，`index.css` 的引用会整体错位 86–99 行、`MainWindow.tsx` 错 11 行。**先提交，后续所有批次才不会踩到漂移。**
> 已验证稳定的文件（未被修改，回退也不漂移）：`Panel.tsx`、`HistoryPage.tsx`、`ClipItem.tsx`、`VirtualList.tsx`、`panelKeys.ts`、`DialogHost.tsx`、`ui.tsx`（净 ±0）、`tailwind.config.ts`。

### 批次 A · 焦点兜底（1 次 CSS + 3 处属性）｜ 低风险 ｜ 工作量：小

| 项 | 文件:行 | 改法 | Rust | 回归风险 | 量 |
|---|---|---|---|---|---|
| **A-1（V-01/U-04/U-05）** | `src/index.css`（新增，建议放在 `@layer components` 之后、`:691` 之前） | ```css
:where(button, a, input, select, textarea, [tabindex]):focus-visible {
  outline: 2px solid var(--hairline-focus);
  outline-offset: 2px;
  border-radius: 6px;
}
``` | 否 | 低：会与 `.btn/.btn-icon/.seg-item/.pill/.switch/.input/.slider` 自己的 `focus-visible:shadow` **叠加**成双环（外框 outline + 内圈 shadow）。建议同时把 `:173/:183/:207/:221/:231/:245/:281` 的硬编码 `rgba(232,163,61,0.35)` 换成 `var(--hairline-focus)`，并把 `outline-none` 改成 `focus-visible:outline-none` | 小 |
| **A-2（V-01）** | `index.css:42`、`:108` | `--hairline-focus` 深色 `rgba(232,163,61,0.7)` → `#8a5816`；浅色 `rgba(214,143,43,0.75)` → `#8a5816`（白底 6.02:1） | 否 | 低：焦点环从「琥珀光晕」变成「深棕实线」，与品牌色观感变化，需设计确认 | 小 |
| **A-3（U-04）** | `MainWindow.tsx:288`、`:308` | `outline-none` → `focus-visible:outline-none`（配合 A-1 的全局 outline） | 否 | 低 | 小 |
| **A-4（U-05）** | `ui.tsx:80`（CardIconBtn） | 同上，并把 `title` 补一个语义化 `aria-label`（`IconBtnProps` 已有 `title`，加 `aria-label` 可选） | 否 | 低 | 小 |

### 批次 B · 破坏性操作的键盘语义（最高优先级）｜ 低风险 ｜ 工作量：小

| 项 | 文件:行 | 改法 | Rust | 回归风险 | 量 |
|---|---|---|---|---|---|
| **B-1（U-02）🔴** | `DialogHost.tsx:38-42` | 删掉全局 Enter 处理，改成「只有焦点在确认键上才确认」：<br>`if (e.key === "Enter" && dialog.kind !== "prompt" && document.activeElement === confirmRef.current) { e.preventDefault(); e.stopPropagation(); closeDialog(true); }`<br>（或彻底删掉该分支，让按钮走原生激活；**但原生 Enter 在「取消」上会取消、在「确定」上会确定 —— 这正是我们要的**，唯一副作用是 `prompt` 的 Enter 仍需保留 `:114-117` 的处理） | 否 | **中**：现有调用方（`useClipActions.ts:77`、`HistoryPage.tsx:107/251`、`Panel.tsx:291/488`）全部依赖 `await confirm(...)` 的 Promise 结果，不受影响；但「键盘用户确认框」的路径会变（需 Tab 到确定键），**这正是修复目的** | 小 |
| **B-2（E-02）🔴** | `ClipDetail.tsx:45-50` | Enter 分支加焦点判定：`else if (e.key === "Enter" && !(document.activeElement as HTMLElement)?.closest("button")) { ... }`，按钮各自用 Space 激活 | 否 | 低 | 小 |
| **B-3（E-01）🔴** | `ClipContextMenu.tsx:166-169` | 面板窗口高度不足时把菜单**向上展开**：计算 `maxTop < 0` 时改用 `bottom` 锚点（`style={{ left, bottom: window.innerHeight - pos.y }}`） | 否 | 中：菜单定位逻辑要覆盖 4 种锚点，建议先加一个 `data-placement` 便于回归 | 中 |

### 批次 C · 面板键盘模型（方向键 / Esc / ⌘C）｜ 中风险（需改测试）｜ 工作量：中

| 项 | 文件:行 | 改法 | Rust | 回归风险 | 量 |
|---|---|---|---|---|---|
| **C-1（U-01）🔴** | `panelKeys.ts:15-42`（加字段）、`:119-120` | ① `PanelKeyContext` 加 `searchEmpty: boolean`；② 把 `:120` 的 `if (ctx.isTyping) return ignore` 改为 `if (ctx.isTyping && !ctx.isSearch) return ignore`；③ 在 `:79` 之后补 `if (ctx.isSearch && !ctx.searchEmpty) return ignore`（搜索框有内容时方向键仍归输入框） | 否 | **中**：现有测试 `panelKeys.test.ts:201-206` 断言「搜索框内方向键 = ignore」必须同步改为「搜索框为空时方向键 = navigate」 | 中 |
| **C-2（U-01）** | `Panel.tsx:211-225`（传参） | 传 `searchEmpty: !keyword.trim()` | 否 | 低 | 小 |
| **C-3（U-01）** | `FirstRunGuide.tsx:83` | 文案改为「← → 选择条目（搜索框为空时可直接用）」 | 否 | 低 | 小 |
| **C-4（U-08）** | `panelKeys.ts:74` | Esc 分级：`if (ctx.key === "Escape") { if (ctx.isSearch && !ctx.searchEmpty) return { type: "clearSearch" }; return { type: "close" }; }`；`PanelKeyAction` 加 `clearSearch`，`Panel.tsx` 加对应分支调 `setKeyword("")` | 否 | **中**：现有测试 `panelKeys.test.ts:38` 断言「isTyping 时 Esc = close」需同步改 | 中 |
| **C-5（U-15）** | `panelKeys.ts:101-106` | ⌘C 分支最前加 `if (ctx.isSearch) return { type: "ignore" };` | 否 | 低 | 小 |
| **C-6（U-09）** | `DialogHost.tsx:15-55` | 记录 `previouslyFocused`，`closeDialog` 后的 cleanup 里 `requestAnimationFrame(() => prev?.focus())`；面板场景额外 `focusSearch()` | 否 | 低 | 小 |

### 批次 D · 语义色变量迁移（消灭 26 个漏类）｜ 低风险 ｜ 工作量：中

| 项 | 文件:行 | 改法 | Rust | 回归风险 | 量 |
|---|---|---|---|---|---|
| **D-1（V-02/V-10/V-16/U-16/E-10）** | `tailwind.config.ts:35-38` | ```ts
danger:  { DEFAULT: "rgb(var(--danger-text) / <alpha-value>)", fill: "rgb(var(--danger-fill) / <alpha-value>)", soft: "rgb(var(--danger-text) / <alpha-value>)" },
success: { DEFAULT: "rgb(var(--success) / <alpha-value>)", soft: "rgb(var(--success) / <alpha-value>)" },
warning: "rgb(var(--warning) / <alpha-value>)",
favorite:"rgb(var(--danger-text) / <alpha-value>)",
```<br>并在 `index.css:44-48` 把 `--danger-text/--danger-fill/--success/--warning` 改成**空格分隔的 RGB 分量**（如 `--warning: 154 107 0;`），`:110-111` 的浅色值同理 | 否 | **中**：`danger.soft`/`success.soft` 原本是更亮的同系色，迁到变量后会与 DEFAULT 同色 → `ui.tsx:540` 的 Notice success 文字会变深（**这其实是好事**，浅色下 5FE08C 几乎不可读）。需回归 Notice / Toast / Badge 三处 | 中 |
| **D-2** | `index.css` | 一次性补齐剩余的 `border-*`/`ring-*`/`stroke-*` 语义色映射（`border-danger-fill/45`、`border-success/40`、`border-accent`…） | 否 | 低 | 小 |
| **D-3（V-10/E-11）** | 新增 `scripts/check-light-classes.mjs` | 扫描 `src/**/*.{ts,tsx}` 的 `text-*/bg-*/border-*/ring-*/stroke-*` token 与 `index.css` 的 `[class~=]` 映射求差集，**差集非空则 CI 失败**（反向：映射表里没被用到的规则也报警） | 否 | 低 | 中 |
| **D-4（V-03）** | `index.css:614` 之后 | 补 `.light [class~="text-danger/80"] { color: var(--danger-text); }`（若 D-1 已做变量化则自动生效，此条可省） | 否 | 低 | 小 |

### 批次 E · 语义色 / 对比度 / 无障碍语义补全 ｜ 低风险 ｜ 工作量：小–中

| 项 | 文件:行 | 改法 | Rust | 回归风险 | 量 |
|---|---|---|---|---|---|
| **E-1（V-04/V-11）** | `index.css:97` 附近（`:root.light`） | 加 `--accent-500: #8a5816;`（同时确认 `--accent-600/--accent-400` 的使用面）→ 一次性修好浅色下的卡片选中边框、多选勾选框、`border-accent` | 否 | **中**：`--accent-500` 还被 `.btn-primary` 用（`index.css:186` `bg-accent-500` + `text-[#1C1C1E]`）⇒ 浅色下主按钮会变成深棕底 + 深字（对比度反而下降）。**建议不要改 `--accent-500` 本身，而是只改选中态**：`index.css:742-746` 的 `.light .card-clip-selected { border-color: #8a5816; }` + `HistoryPage.tsx:178` 改用 `border-accent-800` | 低 | 小 |
| **E-2（V-06）** | `index.css:178-184`（`.btn` 基类） | 加 `disabled:opacity-45 disabled:saturate-50 disabled:active:scale-100`，并删掉各变体散落的 disabled 规则 | 否 | 低 | 小 |
| **E-3（V-07）** | `index.css`（新增）+ `ClipContextMenu.tsx:41` | Check 加 `menu-check` 类；补 `.menu-item:hover .menu-check, .menu-item-active .menu-check { color: #1c1c1e; }` | 否 | 低 | 小 |
| **E-4（V-08/U-13）** | `ui.tsx:594`（hint） | `text-white/[0.28]` → `text-white/55`（浅色映射 `index.css:611` 已有） | 否 | 低 | 小 |
| **E-5（V-08/U-13）** | `ui.tsx:600-619`（Spinner）+ `Panel.tsx:379`/`HistoryPage.tsx:292` | Spinner 外包 `<div role="status" aria-live="polite">正在加载…</div>` | 否 | 低 | 小 |
| **E-6（V-03/U-13）** | `Panel.tsx:391`、`HistoryPage.tsx:299` | `max-h-[48px] overflow-hidden` → `max-h-[96px] overflow-y-auto break-words` + `title={error}` + 一枚「复制错误详情」按钮 | 否 | 低 | 小 |
| **E-7（U-14/A7）** | `ui.tsx:355-364` | 加 `aria-valuetext={format(value)}`（`format` 已在 props 里） | 否 | 低 | 小 |
| **E-8（U-03）** | `ClipItem.tsx:200-204`、`VirtualList.tsx:53`/`:124` | 卡片加 `aria-selected={selected}`；列表容器加 `role="listbox"` + `aria-activedescendant`（或把卡片改 `role="option"`） | 否 | 低 | 小 |
| **E-9（U-10）** | `FirstRunGuide.tsx:61-113` | 复用 `ClipDetail` 的焦点三件套（移入 / Tab 陷阱 / 归还） | 否 | 低 | 小 |
| **E-10（U-11）** | `useClipStore.ts:351-354`、`:371-375` | 删除类 toast 固定 `duration: 8000` | 否 | 低 | 小 |
| **E-11（V-14/E-17）** | `App.tsx:58-59` | `apply()` 里补 `!winKind.tray`（或明确把托盘面板写进「恒为深色」的规范） | 否 | 低 | 小 |

### 批次 F · 布局与尺寸约束 ｜ 中风险 ｜ 工作量：中

| 项 | 文件:行 | 改法 | Rust | 回归风险 | 量 |
|---|---|---|---|---|---|
| **F-1（R-04/E-16）** | `Panel.tsx:459-509` | header 加 `flex-wrap`；Segment 外面包一层 `<div className="min-w-0 max-w-full overflow-x-auto">`（照抄 `HistoryPage.tsx:279` 的做法） | 否 | 中：换行后 header 高度会变（44 → 72），**面板 chrome 的 114px 假设随之变化**，R-01/R-13 的高度表需要重算 | 中 |
| **F-2（R-04）** | `window.rs:1093-1097` | `panel_width` 改为 `(avail_w * 0.6).clamp(860.0, PANEL_MAX_WIDTH).min(usable)`（保证工具栏最小可容纳） | ✅ | 低（仅影响居中/跟随光标模式） | 小 |
| **F-3（R-09/R-10）** | `VirtualList.tsx:107-108`、`:136` | ① `columns = Math.max(1, Math.ceil(inner / (minColumnWidth + gap)))` + 容器 `overflow-x-auto`，保证列宽 ≥ minColumnWidth；② 或用 `grid-template-columns: repeat(auto-fill, minmax(minColumnWidth, 1fr))` 让列宽连续变化（消除 411→200 的跳变） | 否 | **中**：改列数公式会改变所有默认布局的列数（900px 窗口 3 列 → 2 列），需要重新目测 | 中 |
| **F-4（R-03/R-15）** | `tauri.conf.json:19-20` | `minWidth: 420 → 560`、`minHeight: 360 → 480` | ✅ | 低：窗口最小尺寸变大 → 老用户持久化的 420px 尺寸会被自动拉宽（`window-state` 恢复时会 clamp） | 小 |
| **F-5（R-08/E-13）** | `ui.tsx:296-302`（Row） | 控件区改 `flex flex-wrap items-center justify-end gap-[8px]`；或加 `stack` 变体 | 否 | 低 | 小 |
| **F-6（R-07）** | `index.css:214-216`（`.seg`） | 加 `flex-wrap max-w-full`（让 6 个筛选项在窄容器换行成两行，而不是横向滚动） | 否 | 低 | 小 |
| **F-7（R-06/E-14）** | `Settings.tsx:315`、`:411` | `md:grid-cols-2` → `xl:grid-cols-2`（1280px 以下保持单栏）；或用 `ResizeObserver` 测 `<main>` 宽度落 `data-narrow` | 否 | 低（`xl:` 方案） | 小 |

### 批次 G · 架构级改造（看似小改、实则有依赖）｜ 大

| 项 | 依赖 | 说明 |
|---|---|---|
| **G-1（R-01/R-02/E-08）面板高度自适应** | **需要面板高度测量**（`ResizeObserver` 测卡片流容器）+ **必须同时接 `measureElement`**（否则滚动漂移，见 E-08）+ 要重算 chrome 114 的所有高度表 | 不能只改 `rowHeight`：① 横向板型要按容器高度算 `line-clamp` 行数（写进 CSS 变量 `--panel-lines`）；② 纵向板型要把 `rowHeight` 从写死 220 改成 `max(120, min(220, listHeight − 24))`，且 `VirtualList.tsx:114` 的 `estimateSize` 要跟着变、`:126-143` 要加 `measureElement`；③ chrome 在 <260px 时要能压缩（隐藏标签行 / 状态栏次要项） |
| **G-2（R-05/R-14/E-12）滑块与 Rust 对齐** | **需要 Rust 侧配合**：导出 `PANEL_MIN_HEIGHT` 与当前显示器工作区高度（`avail_h`），前端据此把滑块下限换算为 `max(20, ceil(220 / avail_h × 100))`，并在滑块旁显示「30% · 287px」 | 还要改 `patch_config`（`commands/config.rs:14-27`）使其在 `board_height_scale` / `board_position` 变更时调用 `apply_panel_layout`，让设置**立即生效** |
| **G-3（R-06/E-14）内容区断点** | 需要 `ResizeObserver` 或 CSS container query（项目已用 `ResizeObserver` 模式，见 `VirtualList.tsx:95-105`） | 单纯把 `md:` 换成 `xl:` 只是「止血」，真正的自适应要测 `<main>` 宽度 |
| **G-4（U-06/U-07）历史页列表键盘模型** | 需要在 `HistoryPage` 引入与 `panelKeys` 同源的列表状态机 + roving tabindex + 卡片内按钮的进出策略 | 大 |
| **G-5（U-12）右键菜单键盘可达** | 需要焦点移入 + ↑/↓ 循环 + 子菜单键盘展开 + roving tabindex；且与 B-3 的定位改造耦合 | 中–大 |
| **G-6（E-04）选中项改 id 制** | 需要把 `Panel` 的 `selected` 从下标改为 `clipId`，并同步改 `Panel.tsx:112-120`、`:262-271`、`:296-301` | 中 |
| **G-7（E-03）全文浮层提升到列表容器层** | 需要把 `ClipDetail` 从 `ClipItem` 内部提到 `Panel`/`HistoryPage`，用 `selectedClipId` 驱动 | 中 |
| **G-8（E-19）500 条上限** | 需要后端分页 / 无限滚动 + 前端虚拟化已就绪（`VirtualGrid` 已有） | 中–大 |

---

## 5. 最小可行修复集（如果只想先修 3–5 条）

**推荐这 5 条**（合计约 25 行改动、零 Rust 依赖、零架构改动、可一个 PR 提交）：

| # | 条目 | 文件:行 | 改动量 | 为什么优先 |
|---|---|---|---|---|
| 1 | **U-02 确认框 Enter = 执行破坏性操作** | `DialogHost.tsx:38-42` | **删 5 行 / 改 1 行** | 唯一一条「用户按了安全键却做了危险操作」的问题，且直接命中删除/清空/退出 |
| 2 | **U-04 + U-05 焦点指示缺失** | `index.css` 新增 1 条规则；`MainWindow.tsx:288,308`、`ui.tsx:80` 改 3 个类名 | **约 8 行** | 一条 CSS 解掉侧边栏 9 项 + 卡片内 4 个按钮 + 所有自定义控件的焦点不可见 |
| 3 | **V-03 + U-13 错误文案硬裁 48px** | `Panel.tsx:391`、`HistoryPage.tsx:299` | **2 处各改 1 个类名** | 后端失败时用户看不到原因、也无法复制给开发者；同时解决 `text-danger/80` 的浅色不可读 |
| 4 | **U-01 方向键在默认状态不可达** | `panelKeys.ts:15-42`（加 1 字段）+ `:120`（改 1 行）+ `Panel.tsx:211-225`（传 1 个参）+ `panelKeys.test.ts:201-206`（改 1 条用例）+ `FirstRunGuide.tsx:83`（改 1 句文案） | **约 6 行 + 1 条测试** | 面板的核心心流（唤出→方向键→回贴）当前是断的，而且首启引导教的就是这个操作 |
| 5 | **R-03/F-4 最小窗口宽 420 → 560** | `tauri.conf.json:19` | **1 行** | 420px 下侧边栏 200px 吃掉近一半宽度，所有设置页标签都被截；且窗口尺寸会被 `window-state` 持久化，坏状态跨重启保留 |

**如果只能改 3 条**：取 **1 + 2 + 3**（全部在前端、全部 ≤10 行、全部零回归风险）。
**如果只能改 1 条**：取 **1（U-02）**。

**额外强烈建议一并做的「零成本」两行**：
- `index.css` 的 `:root.light` 加 `--accent-500: #8a5816` **不要做**（会拉低 `.btn-primary` 对比度），但可以在 `.light .card-clip-selected` 里单点改成 `#8a5816`（1 行，修好浅色下卡片选中态不可见）。
- `useClipStore.ts:351`、`:371` 的删除 toast 加 `duration: 8000`（2 行，把「2.4 秒内看不到撤销」变成 8 秒）。

---

## 6. 需实机验证清单（静态不可判定）

| # | 待验证 | 怎么验 | 预期 |
|---|---|---|---|
| 1 | **U-15 ⌘C 在搜索框内是否吞掉选区** | 唤出面板 → 在搜索框输入 `hello` → 选中其中 3 个字符 → 按 ⌘C → 粘到备忘录看内容 | 若粘的是整条记录 ⇒ U-15 成立（取决于 WebKit 版本的 `getSelection()` 行为） |
| 2 | **V-13 半透明材质下的真实对比度** | 系统设置 → 桌面与程序坞 → 选一张**深色**壁纸 → 面板设浅色主题 → 用 ColorPicker 取状态栏 hint 区域像素 | 若底色比 #fafafc 暗很多，三级文字会掉到 2:1 以下 |
| 3 | **V-14 托盘面板浅色下的实际观感** | 设置 → 外观 → 浅色 → 点菜单栏图标展开托盘面板 | 确认浅色托盘面板是否可读、是否与环形盘/HUD 的深色不一致 |
| 4 | **R-04 工具栏溢出后的实际表现** | 设置 → 剪贴板 → 出现位置改为「跟随光标」→ 屏幕缩放调到 150% → 唤出面板 | 确认「清空历史」「设置」是否真的被推出可视区（我的推算是缺 ~120px） |
| 5 | **E-01 面板右键菜单底部是否被裁** | 唤出面板（默认 30% 高度）→ 在卡片上右键 → 看「删除」项是否完整可见 | 面板高 233 时按推算底部被裁 ~10px |
| 6 | **R-02 纵向板型下操作条** | 设置 → 卡片排列 = 纵向 → 唤出面板 → 缓慢下滚 | 面板高 233 时操作条整条不可见（推算差 101px） |
| 7 | **V-01 焦点环可见度** | 切浅色主题 → Tab 遍历侧边栏 → 截图取色 | 修前 ≈1.29:1，修后应 ≥3:1 |
| 8 | **E-07 启动耗时** | `npm run tauri dev` → 用 Safari Web Inspector 量 5 个 webview 的启动时间与内存 | 5 × 420KB JS |

---

## 7. 给主理人的交付说明

1. **整体可信度：中高（≈85%）**。48 条中 36 条完全成立、8 条方向对但数字/归因要修、4 条不成立。Phase 1 的视觉层结论（V-01/V-03/V-04/V-06/V-08/V-09）复核后**逐条经得起检验**，对比度数字只有 V-04 存在「白底 vs 材质底」的口径差；Phase 1 唯一需要删掉的是 V-12 的「160 字 vs 9 行 clamp」子结论（逻辑上不成立）。
2. **Phase 2 有两处必须更正，否则会误导修复**：
   - **R-03 的结论不成立**（宽度链重复扣了一次内边距）——「删除按钮完全不可见」是错的；真实退化是「来源 App 名被压没 + 跨年时间戳会挤掉来源图标 + 最小列宽承诺失效」。但**窄窗确实有问题**，只是问题在 R-09（列宽下限）和「4 个操作按钮同存时右侧溢出 ~15px」。严重度 🔴→🟠。
   - **§0.4 的整张列宽表与 §0.3 表格的「259px 空白」要作废**，请以本报告 §2.1/§2.3 的表为准。
3. **两个阶段的分歧已裁定**：**V-02 以 Phase 2 为准**（徽标文字可读、不可见的是底色与色点；Phase 1 归因「`.light` 没有 bg-* 映射」不准确 —— 白色家族的 bg 是有映射的，漏的是语义色家族的 bg/border/ring/stroke，共 26 个）；**V-07 以 Phase 2 为准**（浅色不受「消失」影响，但 2.79:1 仍 <3:1，深色 1.0:1 才是真问题）。
4. **有一条 Phase 2 判定有误**：WCAG 2.2 AA 的 2.5.8 Target Size 阈值是 **24×24 CSS px**（44×44 是 2.1 AAA 的 2.5.5），因此卡片内 24×24 按钮**满足 AA**，A12 应判 ✅。
5. **必须实机复核的只有 3 条**：U-15（WebKit 的 `getSelection()` 对 `<input>` 的行为）、V-13/V-14（半透明材质与托盘面板的实际观感）、R-04/E-01（窄屏溢出与菜单裁切的实际像素）。**U-18/R-16 已用 Tauri 2.12.5 的 `drag.js` 源码证伪，无需再验。**
6. **落地顺序建议**：**先提交当前工作区（批次 0）** → 批次 B（U-02/E-02/E-01，破坏性操作）→ 批次 A（焦点兜底）→ 批次 E（语义与对比度，含「1 行 `duration: 8000`」「2 行错误区」）→ 批次 C（面板键盘模型，需同步改 2 条 vitest 用例）→ 批次 D（语义色变量 + CI 差集脚本）→ 批次 F（布局约束，需重算高度表）→ 批次 G（架构级）。
7. **风险提示**：批次 C 会**推翻两条现有 vitest 用例的期望**（`panelKeys.test.ts:38` 与 `:201-206`）—— 它们把「Esc 直接关面板」「搜索框内方向键无效」固化成了正确行为。改代码不改测试 = CI 红灯；改测试不改代码 = 行为没修。这是本次交付里唯一会「伤到现有绿灯」的两处，务必一起改。
