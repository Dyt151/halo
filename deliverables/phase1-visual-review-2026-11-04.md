# Halo 主界面 · 视觉层可用性审查（Phase 1）

> 审查日期：2026-11-04 ｜ 角色：视觉设计架构师（frontend-design-architect）｜ 范围：Panel / MainWindow / HistoryPage / Settings 及设置页 / components 公共组件 / index.css / tailwind.config.ts / App.tsx
> 边界：只审「视觉层如何影响可用性」（对比度、状态可辨识、信息层级、状态反馈可见性、深浅主题与毛玻璃）。交互流程 / 键盘模型 / 响应式断点归 Phase 2。
> 方法：逐文件通读 + 脚本比对作用域内全部 `white/xx` 属性类与 `.light` 的 65 条 `[class~="..."]` 映射 + WCAG 对比度实算。旧报告（2026-10-29）的每条视觉结论都重新打开当前代码核实。
> 说明：本文件为成员产出的忠实归档，内容未作改写。

## 总评

| 维度 | 结论 |
|------|------|
| 浅色 remapping（white 家族） | ✅ 旧 L1 已彻底整改，作用域内 0 漏类 |
| 浅色 remapping（语义色家族） | 🔴 漏洞整体转移，仍有 3 处实打实的漏类导致信息隐身 |
| 焦点态 | 🔴 浅色下几乎不可见，且全站 `outline-none` 无兜底 |
| 选中态 | 🟠 浅色下边界不可辨；语言从 3 种变成 4+ 种 |
| 三级/四级文字 | 🟠 旧问题未整改，浅色 remap 后反而更差 |
| 状态反馈可见性 | 🟠 监控暂停 / 权限降级 / 空状态指引均被低对比削弱 |

**一句话**：旧报告点名的 L1/L2 都改了，但「手工逐类映射」这套机制没有换成语义变量，漏洞从 `white/xx` 平移到了 `text-danger/80`、`bg-warning`、`bg-success-soft`；同时焦点环是硬编码琥珀且 `.light` 完全没覆盖，浅色下焦点指示 1.29:1 —— 这是本轮最该先修的一项。

---

## 一、问题清单

### V-01 · 浅色主题焦点环几乎不可见，`--hairline-focus` 变量定义了却零引用 🔴高
- **现象**：所有焦点指示都是硬编码的琥珀半透明光环 `focus-visible:shadow-[0_0_0_3px_rgba(232,163,61,0.35)]` 与 `focus:shadow-[...0.30]`，`.light` 段**没有任何 focus 相关覆盖**。而 `index.css:42`（深色 `rgba(232,163,61,0.7)`）与 `index.css:108`（浅色 `rgba(214,143,43,0.75)`）定义的 `--hairline-focus` 全仓只有这两处定义、**零处引用**。
- **证据**：`src/index.css:173`（.input）、`:183`（.btn）、`:207`（.btn-icon）、`:221`（.seg-item）、`:231`（.pill）、`:245`（.switch）、`:281`（.slider :focus-visible）、`:393`（ui.tsx Select 的 focus:border/shadow）；变量定义在 `:42`、`:108`；`src/components/ui.tsx:393`。
- **对比度实算**：光环 `rgba(232,163,61,0.35)` 落在浅色白底 → `(247,223,187)`，**1.29:1**；落在 `focus:border-accent-500/70` → **1.70:1**；深色 `#1e1e1e` 上也只有 **2.10:1**。WCAG 2.4.11 / 2.4.7 要求焦点指示与相邻色 ≥3:1。
- **对可用性的影响**：所有可交互控件都带 `outline-none`（`index.css:171,180,203,219,228,243,263`；`ui.tsx:80,391`；`MainWindow.tsx:288,308`），浏览器默认 outline 被关掉后**没有任何兜底**。侧边栏导航项（`MainWindow.tsx:288`）连 focus-visible 规则都没写。
- **建议改法**：① 把散落的 `shadow-[0_0_0_3px_rgba(232,163,61,0.35)]` 统一收敛到 `box-shadow: 0 0 0 3px var(--hairline-focus)`；② 把 `--hairline-focus` 浅色值改为实色深琥珀 `#8a5816`（白底 6.02:1）或系统蓝 `#007aff`；③ 在 `.light` 段给 `.input` / `.btn` / `.seg-item` / `.pill` / `.switch` / `.slider` 补 `focus-visible` 覆盖；④ 给 `MainWindow.tsx:288` 的导航按钮补 focus 光环。

### V-02 · 状态语义色不走 CSS 变量，浅色下「监控暂停 / 权限状态点」隐身 🔴高
- **现象**：`warning` / `success` / `danger` 在 `tailwind.config.ts` 里是**静态色**，`.light` 段只 remap 了 `text-*` 几类（`:614-617`），`bg-*` 完全没有。`:root.light` 里虽然把 `--warning` 改成了 `#9a6b00`、`--success` 改成 `#1f8a3b`（`:110-111`），但 JSX 里的 `bg-warning` / `bg-success-soft` 消费的是 tailwind 静态值，不消费这些变量。
- **证据**：`src/components/StatusBar.tsx:54`（`bg-warning/[0.14]`）、`:57`（5px 圆点 `bg-warning`）；`src/components/PermissionCenter.tsx:30`（`bg-success-soft` / `bg-warning` / `bg-white/25` 三态点）；`src/components/ContextMenuSettings.tsx:124,149`（同型）；静态色定义 `tailwind.config.ts:35-37`；`.light` 只覆盖 `text-warning` 见 `index.css:617`。
- **对比度实算**：`bg-warning`=`#FFD60A` 落在 `#fafafc` → **1.35:1**；`bg-success-soft`=`#5FE08C` → **1.61:1**；而徽标文字 `text-warning` 重映射后是 `#9a6b00` → 4.50:1。
- **对可用性的影响**：「监控已暂停」（`StatusBar.tsx:52-60`）是 18px 高、11px 字的微型徽标，浅色下色点消失后极易被忽略；PermissionCenter 的「已授权 / 未授权 / 不支持」三态**纯靠 6px 色点颜色区分**，浅色下三档几乎都褪成近白。
- **建议改法**：① `tailwind.config.ts` 把 `warning` / `success` / `danger` / `favorite` 全部改成 `var(--warning)` 等语义变量（注意 `var()` 不能直接参与 `/0.14` 透明度语法，需改用 `color-mix()` 或拆成独立 `--warning-tint` 变量）；② 至少补 `.light [class~="bg-warning"] { background-color: var(--warning); }` 与 `.light [class~="bg-success-soft"] { background-color: var(--success); }`；③ 状态点别只靠颜色，加形状/图标差异，并给徽标加 `border` 或 `ring`。

### V-03 · `text-danger/80` 漏 remapping，浅色下错误详情不可读 🔴高
- **现象**：错误文案用 `text-danger/80`；`.light` 段只映射了裸 `text-danger`（`:614`）与 `text-danger-soft`（`:615`），**没有 `text-danger/80`**。`danger.DEFAULT = #FF6969`（`tailwind.config.ts:35`），80% 落在白底 → `(255,135,135)`。
- **证据**：`src/windows/Panel.tsx:391`、`src/pages/HistoryPage.tsx:299`；映射表 `src/index.css:614-617`。
- **对比度实算**：浅色白底 **2.32:1** / 浅色材质 **2.22:1**；深色 `#1e1e1e` 上 4.25:1。同一条错误在状态栏里用 `text-danger`（`StatusBar.tsx:67`，浅色 remap 到 `#d70015`）是可读的，形成「同一错误、两种可见度」的不一致。
- **对可用性的影响**：这是「历史加载失败」时用户最需要读清原因的一段字，浅色下反而最淡。且两处都带 `max-h-[48px] w-full overflow-hidden`（`Panel.tsx:391`、`HistoryPage.tsx:299`），长错误链被硬裁、无省略号、无滚动、不可选中复制。
- **建议改法**：补 `.light [class~="text-danger/80"] { color: var(--danger-text); }`；把 `max-h-[48px] overflow-hidden` 改成 `max-h-[64px] overflow-y-auto` 或 `line-clamp-3`；错误文案允许选中/复制。

### V-04 · 卡片选中态浅色下三层信号全部失效 🟠中
- **现象**：`card-clip-selected` 的三层视觉 = `border-accent-500` + `bg-accent-tint` + `shadow-[0_0_0_1px_rgba(232,163,61,0.45)]`（`index.css:306-309`）。`.light .card-clip-selected`（`:742-746`）只是把值换成 `var(--accent-500)` / `var(--accent-tint)`，但 `--accent-500` 在 `:root.light` **未重定义**，仍是 `#e8a33d`。
- **对比度实算**：浅色下 border `#E8A33D` / 白底 **2.16:1**（WCAG 1.4.11 非文本组件边界需 3:1）、`bg-accent-tint`(0.18) **1.14:1**、shadow ring(0.45) **1.39:1**；深色下 border 7.73:1，非常清楚。
- **证据**：`src/index.css:306-312, 742-746`；调用见 `src/components/ClipItem.tsx:238`、`src/windows/Panel.tsx:359`；多选态 `src/pages/HistoryPage.tsx:188-189` 的 `shadow-[inset_0_0_0_2px_rgba(232,163,61,0.85)]` 在白底约 1.9:1。
- **对可用性的影响**：面板是「唤出即键盘导航」的心流，选中项是唯一的操作目标；浅色下选中卡片与未选中几乎一样，方向键移动选中的反馈链断了。
- **建议改法**：浅色下把 `border-color` 换成 `#8a5816`（6.02:1）或加 2px 实边；在 `:root.light` 单独重定义 `--accent-500`（需连带核查用到它的规则）；或改用系统蓝作选中边界。

### V-05 · 侧边栏 hover 与选中态在浅色下几乎同色（旧 L2 整改的副作用）🟠中
- **现象**：旧 L2 把 `bg-black/[0.15]` 从 3.5% 提到 7%（`index.css:654`），但选中项 `bg-white/[0.10]` 也被映射到 `rgba(0,0,0,0.07)`（`:643`）、hover 映射到 `0.05`（`:675`）—— 三者数值挤在一起。
- **证据**：`src/windows/MainWindow.tsx:248`（aside `bg-black/[0.15]`）、`:290-291`（active `bg-white/[0.10]` / hover `hover:bg-white/[0.06]`）、`:288`（`outline-none` 且无 focus-visible）；`src/index.css:643,654,675`。
- **对比度实算**：侧边栏底 `(237)`、选中 `(220)`、hover `(225)` → **选中/hover 1.049:1**、选中/底 1.17:1；深色下选中/hover 也只有 1.13:1。
- **对可用性的影响**：鼠标悬停与「当前所在分区」几乎无法区分；选中项没有焦点环，键盘 Tab 到侧边栏时无任何视觉反馈。
- **建议改法**：选中态改用语义化左侧 3px 竖条或把 active 提到 `bg-white/[0.14]` 与 hover 拉开；补 `:focus-visible` 光环；按「底 / hover / active 三档至少各差 1.15:1」验算。

### V-06 · 禁用态表达不一致，多数禁用按钮看起来完全可用 🟠中
- **现象**：只有 `.btn-primary` 有 disabled 视觉（`:187`），`.btn-secondary` / `.btn-danger` / `.btn-ghost`（`:189-198`）**没有任何 disabled 样式**，只有基类 `:182` 的 `disabled:cursor-not-allowed`。`Pill` 用 `opacity-40`（`ui.tsx:189`）、`Switch` 用 `opacity-40`（`ui.tsx:277`），是第三种表达。
- **证据**：`src/index.css:182,187,189-198,712`；大量禁用按钮见 `src/pages/HistoryPage.tsx:221,228,239,245`、`src/components/PermissionCenter.tsx:149,179,209`。
- **对比度实算**：浅色下 `.btn-primary:disabled` 底 `(248,227,197)`、字叠加后 `(174,159,138)` → **文字/底 2.07:1**。
- **对可用性的影响**：HistoryPage 工具栏四个按钮禁用时与可用态一模一样，只有鼠标光标变化；用户会反复点击无响应按钮并怀疑程序卡死。
- **建议改法**：在 `.btn` 基类统一加 `disabled:opacity-45 disabled:saturate-50 disabled:active:scale-100`，删掉各变体散落的 disabled 规则；确保 `aria-disabled` 与 `disabled` 同步。

### V-07 · 菜单勾标记在 hover/active 时与琥珀实底同色而消失 🟠中
- **现象**：`Check` 图标是 `text-accent-500`（`#E8A33D`），而 `.menu-item:hover, .menu-item-active` 的底是 `bg-accent-500`（同为 `#E8A33D`）。`index.css:351-360` 已为 `.menu-shortcut` 和 `.menu-arrow` 做了 hover 反色，唯独勾标记漏了。
- **证据**：`src/components/ClipContextMenu.tsx:41`（Check）、`:158,184-189`、`:201`、`:237`；`src/index.css:334-337, 351-360`；`.light` 把 `text-accent-500` 翻成 `#8a5816`（`:625`）但 `bg-accent-500` 未变 → 浅色下 2.79:1（仍 <3:1）。
- **对比度实算**：深色下勾与底同为 `#E8A33D` → **1.0:1**，完全消失。
- **对可用性的影响**：用户 hover 到「已置顶 / 已收藏」项时，恰恰在这一刻看不到「已生效」这个状态。
- **建议改法**：给 Check 加 `menu-check` 类，补 `.menu-item:hover .menu-check, .menu-item-active .menu-check { color: #1c1c1e; }`。

### V-08 · 空 / 加载 / 错误三态与状态栏：小字号 × 低对比叠加 🟠中
- **现象**：`EmptyState` 的**行动指引** `hint` 用 `text-white/[0.28]`（浅色 remap 到 `rgba(0,0,0,0.36)`），是整屏最淡的字；加载态只有一个 `Spinner` 无文字；状态栏整条 `text-white/45`，错误 chip 单行 `truncate` 只靠 `title` tooltip 看全文。
- **证据**：`src/components/ui.tsx:592`（图标 `/30`）、`:593`（标题 `/45`）、`:594`（hint `/[0.28]`）、`:600-619`（Spinner 仅 `aria-hidden`，无文字）；`src/windows/Panel.tsx:379,418`；`src/pages/HistoryPage.tsx:292,326`；`src/components/StatusBar.tsx:48,63-75`。
- **对比度实算**：hint 深色 **2.50:1** / 浅色 **2.51:1**；标题 `/45` 浅色 3.94:1；加载 spinner `text-white/40` 浅色 3.44:1；状态栏 `/45` 浅色 3.94:1。
- **对可用性的影响**：空状态里最该被读到的「下一步做什么」是 12px + 2.5:1；加载态既没文字也无 aria-live；状态栏错误 tooltip 不可选中复制。
- **建议改法**：`EmptyState` 的 hint 提到 `text-white/55`；加载态补一行「加载中…」并给容器 `aria-live="polite"`；状态栏错误 chip 允许两行或提供可复制入口；`title` 不作为唯一信息载体。

### V-09 · 三级/四级文字系统性低于 AA，浅色 remap 后更差（旧 🟠中 项仍未整改）🟠中
- **现象**：旧报告说深色下 `text-white/40`、`/45` 约 4.1:1 属临界、建议提到 `/50`–`/55`。当前代码不仅仍大量使用，还新增了 `/35`、`/30`、`/[0.28]`，且 `.light` 把它们 remap 到更浅的黑 alpha。
- **证据**：`src/index.css:597-613`（映射表，如 `:602` 把 `/45` → `rgba(0,0,0,0.5)`、`:603` 把 `/40` → `0.46`、`:606` 把 `/[0.28]` → `0.36`）；承载功能信息处：`src/components/StatusBar.tsx:48,81`、`src/components/ClipItem.tsx:254,259,277`、`src/components/ui.tsx:299`（Row hint）、`:349`（菜单快捷键 `/40`）、`:238`（kbd `/50`）、`src/components/ClipDetail.tsx:104,142`、`src/windows/MainWindow.tsx:267,275,308`、`src/pages/HistoryPage.tsx:202`。
- **对比度实算**（浅色材质 `#fafafc` 上）：`/45`→3.94:1、`/40`→3.44:1、`/35`→3.02:1、`/30`→2.66:1、`/[0.28]`→2.51:1；深色卡片底 `(42,42,44)` 上：`/45`→4.15:1、`/40`→3.58:1、`/30`→2.65:1。全部低于正文 AA 4.5:1。
- **对可用性的影响**：时间戳、来源 App、体积、菜单快捷键、分组标题、版本这些都是**功能性信息**，不是装饰；11px + 3.x:1 叠加后在 Retina 上也吃力。
- **建议改法**：建立语义变量 `--text-secondary`（≈`rgba(0,0,0,0.68)` / 深色 `rgba(255,255,255,0.68)`）与 `--text-tertiary`（≈0.55），把 JSX 里所有 `/40`–`/[0.28]` 收敛到这两档。

### V-10 · 浅色 remapping 机制脆弱：white 家族已补全，漏洞平移到语义色 🟢低（但已实证致害）
- **现象**：作用域内 16 个文件的全部 `white/xx` 属性类与 `index.css` 的 65 条 `[class~="..."]` 映射做差集 —— **white 家族 0 漏类**（`:607-613` 补了 `/80 /75 /65 /55 /50 /20` 六档）。**但**漏类转移到了非 white 家族 —— V-02 的 `bg-warning` / `bg-success-soft`、V-03 的 `text-danger/80` 均为实证漏类；`text-accent-tint-text` 靠 `.light .pill-active` 单独兜底，属同类风险。另 `on-dark` 例外只覆盖 `text-white` / `text-white/85` / `border-white/25` 三种（`:686-688`）—— **待验证**（RingEditor 属圆环编辑器，本次仅了解设置页结构）。
- **建议改法**：① 把 `accent / warning / success / danger / badge` 全量迁到 CSS 变量；② 加一条 CI/lint 脚本：扫描 JSX 中的 `text-*/*`、`bg-*/*`、`border-*/*` token 与 `.light` 映射表求差集并报警；③ 逐步把组件层 `@apply` 里的字面量换成 `var()`。

### V-11 · 选中态视觉语言仍不统一，且注释与实现矛盾（旧 🟢低 项仍存在/变形）🟢低
- **现象**：当前至少 4 种表达 —— 卡片 = 琥珀描边 + 淡彩底 + 1px 光环（`index.css:306-309`）；分段 = 琥珀实底反白（`:223-225`）；Pill = 淡彩底 + 琥珀字 + ring（`:233-235`）；菜单 = 琥珀实底反白（`:334-337`）；多选勾选框 = 琥珀实底 + inset ring（`HistoryPage.tsx:178,189`）。
- **额外**：`index.css:307` 注释写「选中态统一用品牌强调色…**不再出现黄色描边**」，但同一条规则里的 `border-accent-500` 正是琥珀描边 —— 注释与实现矛盾。
- **建议改法**：规范明确二分 —— 导航/开关类用「实底 accent + 反白字」，容器（卡片）类用「描边 + 淡彩底」，并把注释改成与实现一致。

### V-12 · 固定宽高 / 截断 / 小字号的累积影响（含旧报告残余项）🟢低
- **现象与证据**：
  - 卡片固定三段 28 / flex-1 / 30（`ClipItem.tsx:243,266,271`）+ `line-clamp-[9]`（`:141,148,156`），「查看全文」入口只在 `canExpand`（文件 / 文本 >160 字，`:194-196`）时出现，短于 160 字但被 9 行截断的内容没有入口 —— **待验证**。
  - 卡片头 `max-w-[112px] shrink-0` 时间戳（`:254`）与 `ml-auto min-w-0 truncate` 来源 App（`:259`）竞争，168px 横向卡片里时间戳 + 徽章 + 16px AppIcon 已把来源名压到接近 0 宽。
  - `text-[10px]` 仍在 `src/pages/HistoryPage.tsx:176`（多选勾选框）；`ui.tsx:7` 头部注释仍宣称「字号只用 11/12/13/15/17/20/28 七档」与代码不符。旧报告「个别字号越界」**部分整改**。
  - 图标线宽仍 1.6 / 1.8 / 2 / 2.2 / 2.4 / 3 / 3.5 七档并存（`MainWindow.tsx:168` 1.8、`ClipItem.tsx:300,321,340,363` 2、`ui.tsx:230` 3.5、`ui.tsx:468` 2.2、`ClipContextMenu.tsx:44` 3 / `:62` 2.4、`HistoryPage.tsx:183` 3.5、`Panel.tsx:31/345` 1.8/1.6）。
  - 「删除与复制之间仅 `w-[6px]` 间隔」仍在 `ClipItem.tsx:349`。
- **建议改法**：统一 10px → 11px；图标线宽收敛到 1.8/2.0 两档；时间戳改 `min-w-0 flex-1` 并给来源名 `shrink-0` 权重；`canExpand` 改为「预览实际被 clamp」而非字数阈值。

### V-13 · 毛玻璃 / 半透明背景下所有对比度数字只是理想值 🟢低（含待验证）
- **现象与证据**：`.app-surface--panel` / `--window` 用 `rgba(28,28,30,0.62)` / `rgba(30,30,32,0.74)`（`index.css:52-53`），浅色 `rgba(248,248,250,0.70)` / `rgba(250,250,252,0.80)`（`:115-116`）。本报告所有对比度按不透明 `#1e1e1e` / `#fafafc` 计算，是**上界**。
- **风险**：若用户桌面是深色壁纸且未开「降低透明度」，浅色面板实际底色会偏暗，而 `.light` 把 `text-white/xx` 翻成黑色 —— 三级文字会进一步掉到 2:1 以下。**待验证**。
- **内部矛盾**：`.menu` 仍带 `backdrop-filter: blur(28px) saturate(1.5)`（`:326-327`），而 `.app-surface--panel` 的注释明确写「透明 WKWebView 里 backdrop-filter 采不到桌面…不再使用」（`:149-151`）。`prefers-reduced-transparency` 兜底存在（`:159-166`）但只换 surface，没关菜单的 backdrop-filter。
- **建议改法**：给关键文字加不透明底衬或把 surface 的 alpha 提到 ≥0.85；reduced-transparency 分支一并关掉 `.menu` 的 backdrop-filter。

### V-14 · 边界共享变量缺陷：TrayPanel 未排除 `.light`（待验证）🟢低
- **现象**：`App.tsx:57-61` 的 `apply()` 只排除 `winKind.ring` 与 `winKind.hud`，**没有排除 `winKind.tray`**。而 `TrayPanel.tsx:2131` 用 `app-surface--panel`、`:2135` 整个内容 `text-white`。浅色主题下这些会被 `.light` 属性选择器翻成深色字。托盘面板不在本次范围，本条仅因「共享样式变量缺陷」记录，**待验证**。

---

## 二、已整改 / 已不存在的旧问题（逐条对照 2026-10-29 报告）

| 旧报告项 | 当前状态 | 证据 |
|---|---|---|
| **L1 · 浅色强调色对比度失效**（🔴） | ✅ **已整改** | `index.css:623-625` 把 `text-accent` / `text-accent-500` / `text-accent-400` 一起映射到 `#8a5816`（白底 6.02:1，旧值 2.16:1）；受影响点全部覆盖。 |
| **L2 · 浅色侧边栏分层消失**（🔴） | ✅ **已整改，但衍生变形** | `index.css:654` 把 `bg-black/[0.15]` 从 0.035 提到 0.07；由此产生 V-05。 |
| **浅色 remapping 方案脆弱**（🟠 工程项） | ⚠️ **机制仍在，漏洞已平移** | white 家族 0 漏类；但未迁到语义变量，漏洞转移到 `text-danger/80`（V-03）、`bg-warning` / `bg-success-soft`（V-02）。 |
| **图标线宽不统一**（🟠） | ❌ **仍存在，且档位更多** | 见 V-12，现为七档。 |
| **个别字号越界 `text-[10px]`**（🟠） | ⚠️ **部分整改** | ClipItem 内 10px 已清除；`HistoryPage.tsx:176` 仍是 `text-[10px]`。 |
| **三级文字对比临界，建议 `/50`–`/55`**（🟠） | ❌ **仍存在，且浅色下更差** | 见 V-09。 |
| **选中态视觉语言不统一**（🟢） | ❌ **仍存在，变形为 4+ 种** | 见 V-11。 |
| **错误态文案区 `max-h-[48px]` 截断**（🟢） | ❌ **仍存在** | `Panel.tsx:391`、`HistoryPage.tsx:299` 原样。 |
| **删除与复制之间仅 `w-[6px]` 间隔**（🟠） | ❌ **仍存在** | `ClipItem.tsx:349` 原样。 |
| **时间戳 `max-w-[112px]` 与来源名竞争**（🟢） | ❌ **仍存在** | `ClipItem.tsx:254,259` 原样。 |
| **R1 · 历史页窄设置窗工具栏裁切**（🔴 响应式项） | ✅ 代码已整改 | `HistoryPage.tsx:270` 加 `flex-wrap`；`:275` SearchField 改 `w-[240px] min-w-[140px] max-w-full flex-1`；`:279` 分段外层加 `min-w-0 max-w-full overflow-x-auto`。 |
| **U3 · Segment 方向键未 stopPropagation**（🟠） | ✅ **已整改** | `ui.tsx:140` 已加 `e.stopPropagation()`；Segment 补了 roving tabindex 与 `aria-selected`。 |
| **U5 · ClipDetail 焦点管理缺失**（🟠） | ✅ **已整改** | `ClipDetail.tsx:33-82` 已实现焦点移入、Tab 陷阱、Esc/Enter、焦点归还。 |
| **U1/U2 · 卡片内嵌套交互元素 + 缺 roving tabindex**（🔴） | ⚠️ **部分整改** | `ClipItem.tsx:201` 已从 `role="button"` 改为 `role="group"`，但 `:202` 的 `tabIndex={0}` 仍在每张卡片上；`HistoryPage.tsx:154` 多选外壳同样 `tabIndex={0}`。 |
| **面板 minHeight / `board_height_scale` 下限**（🔴 响应式项） | ➖ 不在视觉职责，交 Phase 2 | — |
| **值得保留项** | ✅ 前四项机制仍在，第五项部分失效 | 徽章 `index.css:629-634` 完好；reduced-motion `index.css:575-586` 完好；空状态指引在但可读性差；缓动 `tailwind.config.ts:66-73` 完好；语义色重映射 `text-*` 有效、`bg-*` 失效。 |

---

## 三、交给 Phase 2 的输入（视觉相关，请重点复核）

1. **键盘焦点可见性**（V-01）：浅色下焦点环 1.29:1 + 全站 `outline-none` 无兜底 + 侧边栏导航项连 focus-visible 规则都没有。
2. **状态反馈是否被忽略**（V-02 / V-08）：「监控已暂停」徽标、PermissionCenter 三态色点、空状态行动指引、无文字的加载 Spinner、状态栏单行 `truncate` 错误。
3. **选中态语义冲突**（V-04 / V-05 / V-07 / V-11）：四种选中表达并存；hover 到已置顶/已收藏菜单项时勾标记是否真的消失。
4. **禁用态可辨识**（V-06）：HistoryPage 工具栏与 PermissionCenter 的禁用按钮无视觉差异。
5. **remapping 漏类与主题实时切换**（V-03 / V-09 / V-10 / V-14）：`text-danger/80`、`on-dark` 例外只覆盖 3 类、`App.tsx` 未排除 `winKind.tray`。
6. **窄窗下截断叠加**（V-11 / V-12 / 旧 R1）：窄窗下时间戳 vs 来源名竞争、9 行 clamp 无「查看全文」入口、10px 勾选框、错误区 `max-h-[48px]` 硬裁。

---

### 附：审查纪律说明
- 未修改任何文件。
- 所有行号均来自当前代码逐行通读；对比度数值由 WCAG 相对亮度公式实算，底色假设已明示。
- 明确标注为「待验证」的 3 处：V-10 的 `RingEditor.tsx:183` hover 反色、V-12 的 160 字 / 9 行 clamp 关系、V-13/V-14 的半透明底与 TrayPanel 实机表现。
