# Halo 应用主界面 · 可用性与响应式审查（最终汇编）

> 日期：2026-11-04 ｜ 方法：UI 设计全栈专家团三分审（视觉 → 体验 → 前端工程），第三阶段对前两阶段逐条代码复核
> 范围：剪贴板面板 `Panel.tsx`、设置中心 `MainWindow.tsx` + `Settings.tsx`、历史管理 `HistoryPage.tsx`、公共组件 `components/*`、`index.css` + `tailwind.config.ts`、`App.tsx`，以及 `src-tauri` 侧的窗口尺寸约束
> 原始产出：`deliverables/phase1-visual-review-2026-11-04.md`（V-01~V-14）、`deliverables/phase2-ux-responsive-review-2026-11-04.md`（U-01~U-18、R-01~R-16）、`deliverables/phase3-engineering-review-2026-11-04.md`（复核 + E-01~E-20 + 批次清单）
> 本文件是三阶段结论的汇编与裁定，**不是新结论**；所有「文件:行」以 2026-11-04 当前工作区为准。

---

## 0. 审查怎么做的

| 阶段 | 角色 | 产出 | 结论数 |
|---|---|---|---|
| Phase 1 | 视觉设计专家 | 视觉层如何影响可用性（对比度、状态可辨识、层级、反馈可见性） | V-01 ~ V-14 |
| Phase 2 | 体验设计专家 | 可用性（键盘流、焦点、无障碍、IA、错误预防）+ 响应式（窗口尺寸自适应） | U-01 ~ U-18、R-01 ~ R-16 |
| Phase 3 | 前端工程专家 | 逐条打开代码复核 + 独立复算像素/对比度 + 跑构建与测试 + 读 Tauri 依赖源码证伪 | 复核 48 条 + 新增 E-01 ~ E-20 |

**复核结果：48 条中 ✅ 36 条成立、⚠️ 8 条方向对但数字/归因要修、❌ 4 条不成立。** 整体可信度约 85%，Phase 1 的视觉结论基本全部经得起检验，Phase 2 的宽度推导链有一处系统性错误（见 §3）。

---

## 1. 总评

| 维度 | 判断 | 一句话 |
|---|---|---|
| 可用性 | ⚠️ 中等 | 核心心流「唤出→方向键→回贴」在默认状态下**是断的**；破坏性确认框的 Enter 语义反了 |
| 响应式 | 🔴 偏弱 | Tauri 窗口尺寸约束（main 最小 420×360、panel 220 高固定）与前端布局假设**没有对齐**，多处在窄窗下裁切/溢出 |
| 无障碍 | 🔴 缺口明显 | 焦点指示在浅色下 1.29:1 且无兜底；列表无 `aria-selected`；右键菜单键盘不可达 |
| 视觉可读性 | ⚠️ 中等 | 旧报告的 L1/L2 已修好，但「手工逐类映射浅色主题」的机制没换，漏洞平移到语义色家族（26 个 token 未映射） |
| 工程落地 | ✅ 良好 | 构建通过、159 个单测全绿、虚拟化与监听清理到位；问题集中在配置与布局常量上 |

**一句话结论**：Halo 不是「做得差」，而是**几处设计假设与真实窗口约束脱节**——面板按 900px 宽设计、设置页按视口断点而非内容区断点设计、确认框按「回车=确定」设计，而系统给的最小窗口是 420×360、面板高度下限是 220、焦点环在浅色下只有 1.29:1。**修掉 5 处约 25 行代码，即可消掉最高危的一批。**

---

## 2. 复核后的最终问题清单

### 🔴 高危（11 条）

| 编号 | 问题 | 证据 | 用户会遇到什么 | 修法 |
|---|---|---|---|---|
| **U-02** | 确认框里按 Enter = 执行破坏性操作 | `DialogHost.tsx:38-42`（capture 阶段 Enter → `closeDialog(true)`）、`:53`（默认焦点在「取消」） | 焦点明明在「取消」上，按回车**删除/清空/退出**。capture 阶段的 `preventDefault()` 连按钮原生激活都拦掉了 | 删掉全局 Enter 分支，让按钮走原生激活；`prompt` 类型保留 `:114-117` |
| **E-02** | 详情浮层里在「复制」上按 Enter = **回贴** | `ClipDetail.tsx:40-50`、`:106`、`:146` | 焦点在按钮上，回车却把内容贴到前台 App（不可逆对外操作） | Enter 分支加 `activeElement.closest("button")` 判定 |
| **U-01** | 方向键在默认状态下不可达 | `panelKeys.ts:79→120`（`isTyping` 拦截在方向键分支之前）、`Panel.tsx:140-143`（唤出即聚焦搜索框）、`FirstRunGuide.tsx:83` | 面板一出现焦点在搜索框，方向键被输入框吃掉；**首启引导教的第一个操作是无效的**。用户改去点卡片 → 单击默认是复制 → 内容被复制走却没贴出 | `PanelKeyContext` 加 `searchEmpty`；`:120` 改 `isTyping && !isSearch`；同步改 `panelKeys.test.ts:201-206` 与引导文案 |
| **V-01 / U-04 / U-05** | 焦点环浅色下 1.29:1，且全站 `outline-none` 无兜底 | `--hairline-focus` 定义于 `index.css:42`/`:108` 但**全仓零引用**；焦点环硬编码 `rgba(232,163,61,0.35)`；`outline-none` 见 `index.css:171,180,203,219,228,243,263`、`ui.tsx:80,391`、`MainWindow.tsx:288,308`；**index.css 无任何 focus 规则覆盖裸 `<button>`** | 侧边栏 9 个导航项、卡片内 4 个图标按钮（其中含删除）**键盘 Tab 过去时屏幕毫无变化** | `index.css` 加一条全局 `:where(button,a,input,select,textarea,[tabindex]):focus-visible { outline: 2px solid var(--hairline-focus) }`；硬编码焦点色换成变量；`outline-none` → `focus-visible:outline-none` |
| **E-01** | 面板窗口装不下右键菜单，「删除」项被窗口边界裁掉 | `ClipContextMenu.tsx:166-169`（`position:fixed`）、菜单高 ≈235px、面板高 220–233px、`index.css:330-337` | 面板里右键想删一条 → 「删除」在窗口外，点不到 | 面板内改向上展开（`bottom` 锚点）或禁用菜单里的删除入口 |
| **R-04** | 面板工具栏最小宽 786px，窄屏下右侧两个按钮被整体推出可视区 | `Panel.tsx:459-509`（SearchField 220 + `.seg` 434 + 2×IconBtn 30）；`.seg` 是 `inline-flex` 无 `min-w-0`/`flex-wrap`；`window.rs:1093-1097`（面板宽 = avail_w×0.6） | 13"@150% 缩放时面板宽 605px → **缺约 120px**，「清空历史」「设置」消失（默认贴底模式不复现，一改设置就踩） | header 加 `flex-wrap` + 分段容器包 `min-w-0 overflow-x-auto`；Rust 侧 `panel_width` 改 `clamp(860px, 60%, 1180px)` |
| **R-01** | 面板落到最小高度 220px 时，文本预览只剩 16px（不足 1 行） | `window.rs:1056-1060`（`.max(220)`）；chrome = 44+34+36 = 114（`Panel.tsx:460`、`TagBar.tsx:26`、`StatusBar.tsx:48`）；卡片 28/预览/30 三段（`ClipItem.tsx:243,266,271`） | 13" 屏默认 233px 就只剩 2 行；用户拖高度滑块会「调小了更看不清」 | 面板高度 <260px 时压缩 chrome；预览行数随可用高度自适应（需 `measureElement`，见 §5） |
| **R-06** | 设置页用**视口**断点而非内容区断点 | `Settings.tsx:315,411`（`md:grid-cols-2`）、`MainWindow.tsx:248`（侧边栏 200px）、`:326`（`max-w-[760px]`） | 768–1100px（最常见尺寸）两栏被挤成两半，标签被截成「粘贴前…」；**即使默认 900px 窗口，长标签也可能被截**（每栏 ≤372px） | `md:` → `xl:`（止血），或用 `ResizeObserver` 测 `<main>` 宽度切换栏数 |
| **R-08** | 权限页「打开系统设置」按钮在窄窗下被裁 | `PermissionCenter.tsx:147-159`、Row 控件区 `shrink-0`（`ui.tsx:301`）、`<main>` `overflow-hidden`（`MainWindow.tsx:319`） | 420px 窗口下三个按钮需 220px、可用 139px → **溢出 81px**，最关键的入口消失 | Row 控件区改 `flex-wrap justify-end` |
| **V-03 / U-13** | 错误详情浅色 2.32:1 且被硬裁 48px | `text-danger/80` 全库仅 2 处、`.light` 未映射（`index.css:614-615`）；`Panel.tsx:391`、`HistoryPage.tsx:299` 均为 `max-h-[48px] overflow-hidden` | 后端失败时用户看不到原因、也不能复制给开发者 | 补 `.light` 映射 + 改 `max-h-[96px] overflow-y-auto break-words` + `title` + 复制按钮 |
| **U-03** | 选中态浅色下 2.16:1，且无 `aria-selected` | `index.css:306-309`/`:742-746`、`:root.light` 未覆盖 `--accent-500`；`ClipItem.tsx:200-204`（`role="group"` 无 `aria-selected`）、`VirtualList.tsx:53,124`（容器无 `role="listbox"`） | 浅色下按 ↓ 看不到高亮移动；读屏完全听不到「当前选中第 N 条」 | 只改 `.light .card-clip-selected` 的 border 为 `#8a5816`（**不要动 `--accent-500` 本身**，会拉低 `.btn-primary` 对比度）+ 补 `aria-selected`/`role="listbox"` |
| **V-02 / E-10** | 语义色家族 26 个 `bg/border/ring/stroke` token 未进 `.light` 映射 | `tailwind.config.ts:35-38` 是静态 hex，`index.css:110-111` 改的 `--success/--warning` **零消费者**；`.light` 只映射 `text-*` | 「监控已暂停」徽标**文字可读**（`text-warning`→`#9a6b00`），不可见的是**底色 `bg-warning/[0.14]` 与 5px 色点**（`StatusBar.tsx:54,57`）；权限三态色点（`PermissionCenter.tsx:30`、`ContextMenuSettings.tsx:124,149`）浅色下三档几乎都褪成近白 | `tailwind.config.ts` 改成 `rgb(var(--warning) / <alpha-value>)` 形式，`index.css` 改存 RGB 分量 |

### 🟠 中危（择要）

| 编号 | 问题 | 证据 |
|---|---|---|
| **U-08** | Esc 在搜索框里直接收起整个面板（丢掉关键词），macOS 约定是「先清空再关」 | `panelKeys.ts:70-79`（Esc 在 isTyping 放行之前）、`panelKeys.test.ts:38` 需同步改 |
| **U-09** | 对话框关闭后不归还焦点，「唤出即搜」心流被一次删除打断（Enter 语义漂移到回贴） | `DialogHost.tsx:50-55`（有初始焦点、无 `previouslyFocused`）；对照 `ClipDetail.tsx:36,80` |
| **U-06 / U-07** | 历史页无任何列表级键盘导航（无方向键/Home/End/⌘F），卡片列表层**无 roving tabindex**；多选态下每张卡片有 2 个 Tab 停靠点 + 4 个点了没反应的按钮 | `HistoryPage.tsx` 全文、`:154`、`:169-173`；`ClipItem.tsx:202` |
| **U-10** | 首启引导是模态框但无焦点移入/陷阱/归还，遮罩 `absolute` 使背景仍可 Tab | `FirstRunGuide.tsx:61-113`（`:63`） |
| **U-11** | 撤销入口只存活 2.4 秒，确认文案却承诺「一键撤销」；toast 在屏幕右下角 | `toast.ts:75`、`useClipStore.ts:351-354`、`:371-375`、`HistoryPage.tsx:107` |
| **U-12** | 右键菜单纯鼠标（无焦点移入、无 ↑/↓、子菜单靠 hover），键盘用户够不到「置顶/收藏/加堆栈/标签」 | `ClipContextMenu.tsx:100-132`、`:141-160`、`:196-197` |
| **E-04** | 选中项存**下标**，新剪贴板前插后静默错位 → Enter 回贴错条目 | `useClipStore.ts:255-257`、`Panel.tsx:68,112-120,262-271` |
| **E-03** | 「查看全文」浮层状态挂在虚拟化的 `ClipItem` 上，新条目一插就静默关闭 | `ClipItem.tsx:181,373-386`、`Panel.tsx:354-372` |
| **E-05** | 面板 window keydown 监听随 `items`/`selected` 变化频繁解绑重绑 | `Panel.tsx:198-320`（deps `:306-320`） |
| **R-02 / R-15** | 纵向板型 `rowHeight` 写死 220，面板高 ≤338px 时底部操作条被裁（≤287px 时整条不可见）；管理窗口最小高 360px 时历史页 chrome 占 160px | `Panel.tsx:517-524`、`VirtualList.tsx:114`、`tauri.conf.json:20` |
| **R-05 / R-14** | 高度滑块 20–60 与 Rust `clamp(10,60)` + 物理下限 220 三处语义不一致，20–25% 是死区；且 `patch_config` 不重排面板，改完要重新唤出才生效 | `Settings.tsx:331-339`、`window.rs:1056-1060`、`commands/config.rs:14-27` |
| **R-07 / R-09 / R-10** | 窄窗下筛选分段退化成 171–211px 的横向滚动缝；`minColumnWidth` 承诺在容器 <200px 时失效；列数在窗口宽 **661px** 突变（列宽 411→200px） | `HistoryPage.tsx:270-284`、`VirtualList.tsx:107-108,136` |
| **V-05 / V-11** | 侧边栏 hover 与 active 浅色下仅差 1.049:1；多选勾选框浅色下 2.07:1 看不出勾；`.light` 映射表里 15 条规则已无人使用 | `MainWindow.tsx:248,286-292`、`index.css:643,654,675`、`HistoryPage.tsx:178,189` |
| **V-06 / V-07 / V-09** | 只有 `.btn-primary` 有 disabled 视觉；深色下菜单勾记与 hover 底同色（1.0:1）**完全消失**；三级文字 2.5–4.2:1 全部低于 AA | `index.css:182-198,348-361,597-613` |
| **E-19** | 列表上限 500 条**静默丢弃**最旧记录，界面上无任何提示，几个月后必然撞上 | `useClipStore.ts:223,257` |
| **E-07** | 单包 420KB（gzip 132KB），5 个 webview 各加载一份，零分包 | 构建产物 `dist/assets/index-*.js` |

### 🟢 低（节选）
`V-06` 之外的低危项还有：状态点只靠颜色区分（建议加形状差异）、`text-[10px]` 仍存在于 `HistoryPage.tsx:176` 与 `TrayPanel.tsx:2141`、图标线宽七档并存（`ui.tsx:230`、`ClipContextMenu.tsx:44` 等）、`.menu` 的 `backdrop-filter` 与「透明 webview 采不到桌面」的注释自相矛盾（`index.css:326-327` vs `:149-151`）、`App.tsx:56-62` 未把托盘面板排除 `.light`（`V-14`）、`useClipStore.ts:102` 的模块级 `keywordTimer` 不随卸载清理（`E-18`）、`Panel.tsx:174` 的 `focusSearch()` 是无效调用（`E-06`）。

---

## 3. 复核推翻与更正的结论（避免按错误数字去修）

| 原结论 | 裁定 | 更正 |
|---|---|---|
| **R-03**「420px 下历史页卡片被压到 124px，删除按钮被完整裁掉，缺口 72px」 | ❌ **不成立** | 宽度链**重复扣了一次工具栏内边距**（`HistoryPage.tsx:270` 的 `px-[24px]` 是工具栏自己的，`:342` 的列表容器是兄弟节点、无横向内边距）。正确：inner = **171px**、列宽 171px；底部条最小需求 140px ≤ 可用 149px ⇒ **删除按钮完整可见**。真实退化是：来源 App 名被压到 0–28px、跨年时间戳（`max-w-[112px] shrink-0`）会挤掉 16px 来源图标、「最小列宽 200」承诺失效。严重度 🔴→🟠。**但窄窗确实有问题**，见 R-09 与「5 个按钮同存时右侧溢出约 15px」 |
| **U-18 / R-16**「面板 header 的拖拽区可能吞掉搜索框点击」 | ❌ **不成立**（源码证伪） | 读 Tauri 2.12.5 的 `drag.js`：`isDragRegion` 遇到 `CLICKABLE_TAGS`（A/BUTTON/INPUT/…）且自身无 `data-tauri-drag-region` 时立即 `return false` ⇒ input/button 不会被当拖拽。**无需实机验证** |
| **V-12**「短于 160 字但被 9 行截断的内容没有「查看全文」入口」 | ❌ **子结论不成立** | 9 行 × 18px ≈ 162px 可见高，12px 中文约 13 字/行 ≈ 130 字符 < 160 阈值 ⇒ 会被 9 行截断的文本必然 >160 字，一定有入口。**且被牺牲的是来源名**（`ClipItem.tsx:259` 是 `truncate`），不是时间戳（`:254` 是 `shrink-0`） |
| **A12**「卡片内 24×24 按钮不满足目标尺寸」 | ❌ **判定有误** | WCAG 2.2 AA 的 2.5.8 阈值是 **24×24**（44×44 是 AAA 的 2.5.5），24×24 **满足 AA** |
| **V-02** 归因 | ⚠️ **以 Phase 2 为准** | 「`.light` 的 `bg-*` 完全没有」不成立（白色家族的 bg 是有映射的）。真正漏的是**语义色家族的 bg/border/ring/stroke，共 26 个** |
| **V-07** 归因 | ⚠️ **以 Phase 2 为准** | 浅色下是 2.79:1 不达标，**不是「消失」**；深色下 1.0:1 才是真问题（深色 🔴 / 浅色 🟡） |
| **R-02 / R-07 / R-10 / R-13** 数字 | ⚠️ 已更正 | R-02：只在面板高 ≤338px 裁操作条（≤287px 整条不可见）；R-07：分段换行后是整行宽（171/211px），不是 64px；R-10：突变点在窗口宽 **661**（列宽 411→200）；R-13：空白 **297px**（Phase 2 表格里的 259 是笔误） |
| Phase 2 §0.4 整张列宽表 | ⚠️ **作废** | 以 Phase 3 §2.1 的复算表为准（420→171px/1 列、900→651px/3 列、1200→951px/4 列） |

---

## 4. 最小可行修复集（推荐先做这 5 条）

合计约 25 行、零 Rust 依赖、零架构改动，可一个 PR 提交。

| # | 条目 | 文件:行 | 改动量 |
|---|---|---|---|
| 1 | **U-02 确认框 Enter = 执行破坏性操作** | `DialogHost.tsx:38-42` | 删 5 行 / 改 1 行 |
| 2 | **U-04 + U-05 焦点指示缺失** | `index.css` 新增 1 条全局规则；`MainWindow.tsx:288,308`、`ui.tsx:80` 改 3 个类名 | 约 8 行 |
| 3 | **V-03 + U-13 错误文案硬裁 48px** | `Panel.tsx:391`、`HistoryPage.tsx:299` | 2 处各 1 个类名 |
| 4 | **U-01 方向键在默认状态不可达** | `panelKeys.ts:15-42`（加 1 字段）+ `:120`（改 1 行）+ `Panel.tsx:211-225`（传 1 参）+ `panelKeys.test.ts:201-206`（改 1 用例）+ `FirstRunGuide.tsx:83`（改 1 句） | 约 6 行 + 1 条测试 |
| 5 | **R-03/F-4 最小窗口宽 420 → 560** | `tauri.conf.json:19` | 1 行 |

**顺带两行零成本修复**：`.light .card-clip-selected` 的 border 单点改成 `#8a5816`（1 行，修好浅色下选中态不可见）；删除类 toast 加 `duration: 8000`（2 行，撤销窗口从 2.4 秒变 8 秒）。
**如果只能改 1 条**：改 #1（U-02）。

---

## 5. 修复路线（按可独立提交的批次）

| 批次 | 内容 | 风险 | 量 |
|---|---|---|---|
| **批次 0** | **先 `git commit` 当前工作区**（见 §6 的坑） | 低 | 小 |
| **批次 B** | 破坏性操作的键盘语义：U-02 / E-02 / E-01 | 低–中 | 小–中 |
| **批次 A** | 焦点兜底：全局 `:focus-visible` + 焦点色变量化 + `outline-none` 收敛 | 低 | 小 |
| **批次 E** | 语义与对比度补全：选中态、禁用态、菜单勾记、错误区、Spinner 播报、`aria-selected`、toast 时长、托盘 `.light` | 低 | 小–中 |
| **批次 C** | 面板键盘模型：方向键 / Esc 分级 / ⌘C 选区 / 对话框焦点归还（**需同步改 2 条 vitest 用例**） | 中 | 中 |
| **批次 D** | 语义色变量迁移（消灭 26 个漏类）+ `scripts/check-light-classes.mjs` CI 差集脚本 | 中 | 中 |
| **批次 F** | 布局与尺寸约束：面板工具栏换行、`panel_width` 下限、列宽公式、minWidth/minHeight、Row 换行、`.seg` 换行、设置页断点 | 中 | 中 |
| **批次 G** | 架构级：面板高度自适应（G-1，需 `measureElement`）、滑块与 Rust 对齐（G-2，需 Rust 配合）、内容区断点（G-3）、历史页列表键盘模型（G-4）、右键菜单键盘可达（G-5）、选中项改 id 制（G-6）、全文浮层提升（G-7）、500 条上限分页（G-8） | 高 | 大 |

---

## 6. 两个必须先知道的坑

1. **工作区有 601+ 行未提交改动**（`src/index.css` 净 +99 行、`App.tsx` +8、`MainWindow.tsx` +12、`Settings.tsx` +8）。本报告及前两阶段的**全部 `文件:行` 只对当前工作区成立**——一旦回退到 HEAD，`index.css` 会整体错位 86–99 行。**先提交，再开工。**（`Panel.tsx`、`HistoryPage.tsx`、`ClipItem.tsx`、`VirtualList.tsx`、`panelKeys.ts`、`DialogHost.tsx`、`ui.tsx`、`tailwind.config.ts` 未被修改，回退也不漂移。）
2. **批次 C 会让两条现有 vitest 用例变红**：`panelKeys.test.ts:38`（断言「isTyping 时 Esc = close」）与 `:201-206`（断言「搜索框内方向键 = ignore」）把错误行为固化成了正确行为。**代码与测试必须一起改**，否则 CI 红灯；只改测试不改代码等于没修。

---

## 7. 需实机验证的清单（静态不可判定，共 8 条）

| 待验证 | 怎么验 | 预期 |
|---|---|---|
| U-15 ⌘C 是否吞掉搜索框选区 | 唤出面板 → 搜索框输入 `hello` → 选中 3 个字符 → ⌘C → 粘到备忘录 | 若粘的是整条记录则成立（取决于 WebKit 版本 `getSelection()` 行为） |
| V-13 半透明材质下的真实对比度 | 深色壁纸 + 浅色主题 + 取状态栏 hint 像素 | 底色比 `#fafafc` 暗时，三级文字会掉到 2:1 以下 |
| V-14 托盘面板浅色观感 | 浅色主题 → 展开菜单栏托盘面板 | 确认是否可读、是否与环形盘/HUD 的深色规范冲突 |
| R-04 工具栏溢出 | 出现位置改「跟随光标」+ 缩放 150% → 唤出面板 | 确认「清空历史」「设置」是否真被推出可视区（推算缺约 120px） |
| E-01 面板右键菜单底部 | 默认高度唤出面板 → 卡片右键 | 面板高 233 时底部推算被裁约 10px |
| R-02 纵向板型操作条 | 卡片排列改「纵向」→ 唤出面板 → 缓慢下滚 | 面板高 233 时操作条整条不可见（推算差 101px） |
| V-01 焦点环可见度 | 浅色主题 → Tab 遍历侧边栏 → 截图取色 | 修前 ≈1.29:1，修后应 ≥3:1 |
| E-07 启动耗时 | Safari Web Inspector 量 5 个 webview 的启动时间与内存 | 5 × 420KB JS |

---

## 8. 工程健康度（Phase 3 实测）

- `npm run build`（tsc + vite）✅ 通过；产物单包 `index-*.js` **420.58 kB / gzip 131.80 kB**，`index-*.css` 57.26 kB / gzip 10.51 kB，**零分包**。
- `npm test`（vitest 5.0.5）：默认 forks 池在本沙箱启动超时（环境问题，非代码缺陷），改 `--pool=threads --no-file-parallelism` 后 **9 文件 / 159 用例全部通过**。
- 语义色与 white 家族类名差集扫描：white 家族仅剩 1 个未映射 token（`bg-white`，仅 `InputHud.tsx` 使用，而 HUD 已被排除 `.light`，无实际影响）；**语义色家族 26 个未映射**。
- Tauri 版本锁定 **2.12.5**（`Cargo.lock`），拖拽命中规则已按该版本源码判定。
