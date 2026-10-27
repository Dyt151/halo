# ClipBase 苹果风格设计规范（UI-DESIGN）

> 版本：v1.0 · 2026-10-26 · 适用平台：macOS（Tauri 2 + React + Tailwind CSS 3.4）
> 本文档是 ClipBase UI 大改版的唯一实现依据。所有数值均为**可执行参数**，工程师可直接落入 `tailwind.config.ts` 与 `src/index.css`，无需二次决策。
> 设计基准：Apple Human Interface Guidelines（macOS）+ 第一方 App 观感（系统设置 / 备忘录 / 快捷指令 / Xcode / Safari）。
>
> **时效说明（2026-10-26）**：§1–§8 与 §10 的 token / 控件 / 材质 / 动效规范**仍然有效**，是当前实现的依据。
> §9 是改造当时的逐页方案，其中「历史页 / 转换页 / 模块页」后来被调整：主窗口收窄为设置中心，
> 历史变成面板的二级页，转换变成 Finder 环形盘，模块改成设置中心的「模块」分区。页面归属以 §9 开头的对照表为准；
> 各小节里的「现状 vs 目标」表是改造记录，保留不删。

---

## 目录

1. 设计原则（HIG 三原则落地）
2. 色彩系统
3. 字体排版
4. 间距与圆角
5. 控件规范
6. 材质与层次
7. 动效规范
8. 图标规范
9. 逐页改造方案（6 个界面 + 右键菜单 + 托盘）
10. 实现映射表（token → CSS 变量 / Tailwind config）

---

## 1. 设计原则

Apple HIG 三原则在 ClipBase 的具体落地：

| 原则 | 含义 | 本项目落地规则 |
|---|---|---|
| **Clarity 清晰** | 文字清晰可读、图标表意准确、控件有明确作用 | 文本四级灰度严格分级，正文预览永不小于 11px；每个图标按钮必须有 `title`/`aria-label`；卡片内信息密度固定三段式（头部元信息 / 中部预览 / 底部操作），禁止第四段 |
| **Deference 遵从内容** | UI 让位于内容，装饰退后 | 剪贴板**预览内容是绝对主角**：预览区占卡片高度 ≥ 60%，文本用 12px/#E8E8ED 近白；装饰元素（边框、底色、图标）一律用低对比（白 5%–12%），禁止高饱和装饰色；琥珀色只出现在"可交互的主操作与选中态"，不做大面积背景 |
| **Depth 深度** | 层级、阴影、材质传达前后关系 | 三层深度：① 窗口/面板（大环境阴影 + 毛玻璃）② 卡片/分组（1px hairline，无阴影）③ 悬浮层（菜单/弹窗，最大阴影 + 强材质）。同层元素禁止互相遮挡式投影 |

**反"网页感"铁律**（Tauri/Web 应用通病，逐条对照执行）：

1. 禁止 `rounded-lg`(8px) 以上圆角乱用：卡片 10px、按钮/输入框 6px、窗口 12px，全项目圆角只允许 5/6/7/8/10/12 六档。
2. 禁止纯黑 `#000` 阴影与 `shadow-2xl` 重投影：macOS 阴影永远是"大扩散、低透明度、单层"。
3. 禁止高饱和彩色文字（除语义色 success/danger 与琥珀强调）：正文一律白/灰阶。
4. 禁止 `transition-all duration-300` 之类慢速通用过渡：全部 100–200ms + Apple 缓动曲线。
5. 禁止方角窗口：根容器必须 `rounded-[12px] overflow-hidden`（窗口为 `transparent: true`，见 §6.1）。
6. 间距全部落在 8pt 网格（4/8/12/16/24/32），禁止 5/6/7/10/13/14 等随机值（圆角不受此限）。

---

## 2. 色彩系统

> 当前为深色主题优先（与现状一致）。所有颜色定义为 CSS 变量（见 §10），Tailwind 中通过 `text-white/60` 等透明度语法或映射后的语义色使用。**禁止在组件里再写死 hex**（现存 `#2C2C2E`、`#242426`、`#2a2a2c` 等散落值全部收敛为 token）。

### 2.1 背景层级（深色模式，对照 Apple NSColor）

| 层级 | Token | 值 | 对照苹果 | 用途 |
|---|---|---|---|---|
| 窗口底 | `--bg-window` | `#1E1E1E` | NSColor.windowBackgroundColor (dark, rgb 30/30/30) | 主窗口、设置页内容区背景 |
| 分组面 | `--bg-group` | `#2C2C2E` | systemGray5 (dark) | 设置页分组卡片、模态、右键菜单底 |
| 抬升面 | `--bg-raised` | `#3A3A3C` | systemGray4 (dark) | 分段控件选中滑块、菜单悬停高亮（备选） |
| 输入/填充 | `--fill-input` | `rgba(255,255,255,0.05)` | control fill | 输入框、未选中卡片底 |
| 悬停填充 | `--fill-hover` | `rgba(255,255,255,0.08)` | control hover | 行/图标按钮 hover |
| 按下填充 | `--fill-active` | `rgba(255,255,255,0.12)` | control active | 行/按钮按下 |
| 选中填充（中性） | `--fill-selected` | `rgba(255,255,255,0.10)` | 侧边栏选中行 | 侧边栏选中项底色（非强调色，见 §9.2） |
| 面板材质 | `--material-panel` | `rgba(30,30,32,0.82)` + blur | NSVisualEffectView HUD/Popover 近似 | 底部悬浮面板（见 §6.1） |
| 遮罩 | `--scrim` | `rgba(0,0,0,0.40)` | 模态背景遮罩 | 弹窗背后 |

浅色模式参考值（后续如做浅色主题，映射关系一一对应）：window `#FFFFFF`（实为 `#ECECEC` 窗口边距区 + `#FFFFFF` 内容区）、group `#F2F2F7`(systemGray6 light 实际为 `#F2F2F7`)、raised `#FFFFFF`、hairline `rgba(0,0,0,0.10)`、文本 primary `#000000`、secondary `rgba(0,0,0,0.55)`。

### 2.2 强调色：琥珀黄色阶（规范化）

保留品牌琥珀 `#E8A33D` 为 500 基准，向两侧扩展出完整色阶，并定义**四态**：

| 角色 | Token | 值 | 用途 |
|---|---|---|---|
| 主色 | `--accent-500` | `#E8A33D` | 主按钮、选中态、开关 on、滑块 fill、聚焦光环 |
| 悬停 | `--accent-400` | `#F0B35A` | 主按钮 hover（变亮 6%） |
| 按下 | `--accent-600` | `#D68F2B` | 主按钮 active（变暗 8%） |
| 禁用 | `--accent-disabled` | `rgba(232,163,61,0.32)` | 禁用主按钮底；配 `color: rgba(255,255,255,0.30)` 文字 |
| 强调色上的文字 | `--accent-on` | `#1C1C1E` | 琥珀底上的文字/图标一律深色（Apple 黄按钮用黑字的规则） |
| 淡琥珀底 | `--accent-tint` | `rgba(232,163,61,0.14)` | 选中 pill 底、信息提示条底 |
| 淡琥珀文字 | `--accent-tint-text` | `#F0B35A` | 选中 pill 文字、链接式文字 |

完整色阶（备用，供插画/徽章等扩展）：

```
accent-50 #FDF6EC · 100 #F9E8CD · 200 #F2D0A0 · 300 #EDB874
accent-400 #F0B35A · 500 #E8A33D · 600 #D68F2B · 700 #B3741F · 800 #8A5816 · 900 #5F3D10
```

**对比度实测（对 `#1E1E1E` 背景，WCAG 2.1）**：

| 组合 | 对比度 | 结论 |
|---|---|---|
| `#E8A33D` 文字 / 图标 on `#1E1E1E` | 7.6:1 | AA + AAA（大字）✅ |
| `#1C1C1E` 文字 on `#E8A33D`（琥珀按钮） | 9.7:1 | AAA ✅ |
| `#F0B35A` on `#1E1E1E` | 8.4:1 | AAA ✅ |
| `#FFFFFF` on `#1E1E1E` | 16.7:1 | AAA ✅ |
| `rgba(255,255,255,0.62)`(≈#9E9E9E) on `#1E1E1E` | 6.2:1 | AA ✅ |
| `rgba(255,255,255,0.45)`(≈#8A8A8A) on `#1E1E1E` | 4.8:1 | AA ✅ |
| `#FF6969`（danger）on `#1E1E1E` | 5.9:1 | AA ✅ |

### 2.3 文本层级（四级）

| 层级 | Token / Tailwind | 值 | 适用场景 |
|---|---|---|---|
| Primary 主文本 | `text-white` | `#FFFFFF` | 预览正文、标题、卡片文件名、数值 |
| Secondary 次文本 | `text-white/60` | `rgba(255,255,255,0.62)` | 控件标签、说明文字、时间、来源 App 名、未选中菜单项 |
| Tertiary 三级 | `text-white/45` | `rgba(255,255,255,0.45)` | 占位符、状态栏、辅助说明、空状态正文 |
| Quaternary 四级 | `text-white/25` | `rgba(255,255,255,0.25)` | 禁用文字、装饰性图标（禁止用于正文） |

预览正文特殊规则（Deference）：文本预览用 `#E8E8ED`（Apple 深色模式正文近白，比纯白柔和）——Tailwind 无内置值，用 `text-[#E8E8ED]` 或注册 token `text-preview`。

### 2.4 边框 / 分隔线（hairline）

| Token | 值 | 用途 |
|---|---|---|
| `--hairline` | `rgba(255,255,255,0.08)` | 卡片边框、分组分隔、工具栏底边（Apple separator dark ≈ 白 8%–15%） |
| `--hairline-strong` | `rgba(255,255,255,0.12)` | 输入框边框、菜单/模态边框、面板外框 |
| `--hairline-focus` | `rgba(232,163,61,0.70)` | 输入框聚焦边框（配合 3px 光环） |

实现：统一 `border`（1px）即可，Retina 下视觉等效 Apple 的 0.5pt hairline；**禁止用 `border-2` 表达强调**，选中态用 1px 彩色边 + `box-shadow: 0 0 0 1px` 补光环。

### 2.5 语义色

| 语义 | Token | 值 | 用途 |
|---|---|---|---|
| 成功 | `--success` | `#32D74B`（systemGreen dark） | 转换成功、导入成功提示 |
| 警告 | `--warning` | `#FFD60A` | 提示条、pickerHint |
| 危险（文字） | `--danger-text` | `#FF6969` | 删除按钮、错误文字、危险菜单项 |
| 危险（填充） | `--danger-fill` | `#FF453A`（systemRed dark） | 破坏性确认按钮（仅确认弹窗） |
| 收藏 | `--favorite` | `#FF6969` | 收藏星标激活态 |

类型徽章配色（卡片头部，现状保留但降饱和为 tint 风格）：

| 类型 | 底 | 字 |
|---|---|---|
| 文本 text | `rgba(10,132,255,0.14)` | `#64A8FF` |
| 图片 image | `rgba(48,209,88,0.14)` | `#5FE08C` |
| 文件 files | `rgba(232,163,61,0.14)` | `#F0B35A` |
| 网页 html | `rgba(191,90,242,0.14)` | `#C98BF5` |

> 规则：徽章 = 14% 透明度彩底 + 同色系亮字，永远不加边框。这是 Apple 深色模式徽章的标准做法。

---

## 3. 字体排版

### 3.1 字体栈

```css
--font-sf: -apple-system, BlinkMacSystemFont, "SF Pro Text", "SF Pro Display",
           "PingFang SC", "Helvetica Neue", "sans-serif";
```

- 英文/数字走 SF Pro，中文回落 **PingFang SC**（macOS 内置，与 SF Pro 搭配即苹果官方混排效果）。
- 数字与快捷键（⌘1）用系统字体而非 mono；仅代码片段（模块示例路径）可用 `ui-monospace, SFMono-Regular, Menlo, monospace`。
- `-apple-system` 已覆盖 SF Pro，显式声明只是兜底；**禁止引入 Web 字体**。

### 3.2 字号阶梯（7 档，全项目只允许这些值）

| Token | 字号/行高 | 字重 | 用途 |
|---|---|---|---|
| `text-mini` | 11px / 14px | 400–500 | 状态栏、卡片时间/大小、徽章、kbd |
| `text-caption` | 12px / 16px | 400–500 | 控件标签、分段/pill、卡片预览正文、辅助说明 |
| `text-body` | 13px / 18px | 400 | 主内容正文、设置行标签、菜单项、输入框文字 |
| `text-body-lg` | 15px / 20px | 400 | 模态标题、欢迎/空状态标题、大段正文 |
| `text-title-3` | 17px / 22px | 600 | 页面标题 |
| `text-title-2` | 20px / 25px | 600 | 空状态大标题 |
| `text-title-1` | 28px / 34px | 600 | （预留）关于页大标题 |

Tailwind 即 `text-[11px]/[14px]`、`text-[12px]/[16px]`、`text-[13px]/[18px]`……（当前代码里的 `text-sm`(14px) 一律改为 13 或 12；Apple 控件文字基准是 13px）。

### 3.3 字重规则

| 字重 | 用途 |
|---|---|
| 400 regular | 所有正文、预览、说明 |
| 500 medium | 控件文字、选中态文字、按钮、分段选中项、数值 |
| 600 semibold | 标题、卡片文件名、分组标题、导航选中项 |

禁止 700+（`font-bold` 不出现在 UI 中，仅预留 title-1）。中文 PingFang SC 无 550 中间档，medium 即 500。

---

## 4. 间距与圆角

### 4.1 8pt 网格

| 值 | 使用场景 |
|---|---|
| 4px | 图标与文字间距、徽章内距、分割线上下留白 |
| 8px | pill 内边距、小按钮组间距、图标按钮与输入框间距 |
| 12px | 卡片内边距（紧凑卡片）、工具栏水平内边距（次级）、网格 gap（密） |
| 16px | 面板/页面水平内边距、卡片间 gap、设置分组行水平内边距、模态内边距 |
| 24px | 设置页内容区间距、页面上下留白、分组卡片之间 gap |
| 32px | 空状态上下留白、大分区间隔 |

非网格值（5/6/7/10/13/14px）禁止出现在 padding/margin/gap 中。**现状问题**：`py-2`(8px)+`px-3`(12px) 混用于工具栏导致各行高度不齐——统一改为固定高度 + `px-4`。

### 4.2 圆角阶梯（6 档）

| 值 | Tailwind | 用途 |
|---|---|---|
| 5px | `rounded-[5px]` | 分段控件内格、小徽章 |
| 6px | `rounded-md` | 按钮、输入框、下拉、侧边栏选中项、kbd、复选框 |
| 7px | `rounded-[7px]` | 分段控件容器、小 popover |
| 8px | `rounded-lg` | 小卡片、提示条、标签容器 |
| 10px | `rounded-[10px]` | 剪贴板卡片、设置分组卡片、模态、popover |
| 12px | `rounded-xl` | **窗口/面板本身**、大模态、拖拽提示框 |
| full | `rounded-full` | pill、开关、滑块圆点、头像/App 图标 |

Apple 使用连续曲率（squircle），CSS `border-radius` 已足够近似，无需额外处理。

---

## 5. 控件规范

> 所有控件默认深色模式参数；高度基准：行内小控件 24px、常规控件 28px、主操作 32px。

### 5.1 按钮

**主要按钮（Primary，琥珀填充）**：

```
h-[28px] px-[12px] rounded-md bg-accent-500 text-[#1C1C1E] text-[13px] font-medium
hover:bg-accent-400 active:bg-accent-600 active:scale-[0.98]
disabled:bg-accent-500/30 disabled:text-white/30 disabled:cursor-not-allowed
transition-[background-color,transform] duration-100 ease-apple
```

**次要按钮（Secondary，填充+描边）**：

```
h-[28px] px-[12px] rounded-md bg-white/[0.06] border border-white/10
text-[13px] text-[#F5F5F7] hover:bg-white/10 active:bg-white/[0.12] active:scale-[0.98]
```

**危险按钮（文字型）**：`h-[28px] px-[12px] rounded-md text-[13px] text-[#FF6969] hover:bg-[#FF453A]/12`（确认删除才用填充型：`bg-[#FF453A] text-white`）。

**图标按钮（Icon button，工具栏/卡片操作）**：

```
h-7 w-7 rounded-md grid place-items-center text-white/55
hover:bg-white/[0.08] hover:text-white active:bg-white/[0.12]
transition-colors duration-120
```

图标 16px（`h-4 w-4`）。**现状差异**：当前 `p-1.5`(12px 点击区太小) → 统一 28×28 热区；删除 hover 才变红（`hover:text-[#FF6969]`）。

### 5.2 开关（Toggle / Switch）

Apple macOS 开关是**紧凑型**（非 iOS 大开关）：

```
轨道：w-[28px] h-[17px] rounded-full
  on：bg-accent-500 · off：bg-white/[0.16]
滑块：h-[13px] w-[13px] rounded-full bg-white shadow-[0_1px_2px_rgba(0,0,0,0.4)]
  位移：translate-x-[13px]（off 时 left 2px：left-[2px]）
动画：transition-transform duration-150 ease-apple（cubic-bezier(0.32,0.72,0,1)）
```

**现状差异**：当前 `h-5 w-9`(20×36px) 偏大偏 iOS 化；滑块 `top-0.5 h-4` 太厚重。按上表收敛。必带 `aria-pressed` / `role="switch"`。

### 5.3 滑块（Slider）

Apple 滑块 = 细轨道 + 无描边白色圆点：

```css
.slider { -webkit-appearance: none; width: 100%; height: 16px; background: transparent; }
.slider::-webkit-slider-runnable-track {
  height: 4px; border-radius: 9999px; background: rgba(255,255,255,0.10);
}
.slider::-webkit-slider-thumb {
  -webkit-appearance: none; margin-top: -5.5px;
  width: 15px; height: 15px; border-radius: 9999px; background: #FFFFFF;
  box-shadow: 0 1px 3px rgba(0,0,0,0.5), 0 0 0 0.5px rgba(0,0,0,0.15);
  transition: transform 100ms ease-apple;
}
.slider:active::-webkit-slider-thumb { transform: scale(1.12); }
.slider:focus-visible { box-shadow: 0 0 0 3px rgba(232,163,61,0.35); border-radius: 9999px; }
```

已填充部分用 `accent-color: #E8A33D` 即可（WebKit 支持，轨道左侧填充）——**现状**：裸 `<input type="range" className="accent-amber-500">` 保留但补足 thumb 阴影与 focus 光环。带刻度选项（TTL）时刻度文字 10px/tertiary，两端对齐。

### 5.4 分段控件（Segmented control）—— 替代类型筛选 pill 行

Apple 分段控件：外槽浅灰、选中格随系统强调色：

```
容器：inline-flex h-[26px] p-[2px] rounded-[7px] bg-white/[0.08]
选项：h-[22px] px-[12px] rounded-[5px] text-[12px] text-white/60
      选中：bg-accent-500 text-[#1C1C1E] font-medium shadow-[0_1px_2px_rgba(0,0,0,0.3)]
      切换：transition-colors duration-120
```

**现状差异**：当前 5 个独立 `rounded-full` pill 无容器感（网页感来源之一）→ 套进统一外槽即原生分段控件。选中态允许用强调色（对应 Apple `controlAccentColor` 行为）。

### 5.5 Pill（分组标签）—— 与分段控件区分

Pill 用于"标签"语义（TagBar、黑名单 chip、附件 chip），视觉权重低于分段控件：

```
未选：h-[22px] px-[10px] rounded-full bg-white/[0.06] text-[12px] text-white/60 hover:bg-white/10
已选：h-[22px] px-[10px] rounded-full bg-accent-500/15 text-[#F0B35A] ring-1 ring-accent-500/30
```

**现状差异**：当前 TagBar 选中 = 实心琥珀 `bg-amber-400 text-neutral-900`，与类型筛选抢戏 → 改为淡琥珀 tint，让筛选分段控件成为唯一"实心选中"。

### 5.6 输入框 / 搜索框

```
h-[28px] rounded-md bg-white/[0.05] border border-white/[0.08]
px-[10px] text-[13px] text-white placeholder:text-white/35
focus：border-accent-500/70 shadow-[0_0_0_3px_rgba(232,163,61,0.30)] outline-none
```

搜索框：左侧 14px 放大镜图标 `left-[8px]`，输入框 `pl-[28px]`；高 28px；不要 rounded-full（Apple 工具栏搜索框是 6px 圆角矩形）。

**聚焦 3px 光环**是 Apple focus ring 的标准实现（`box-shadow` 3px + 强调色 30% alpha），全项目输入类控件统一。

### 5.7 下拉（Select / PopUp button）

```html
<select class="appearance-none h-[28px] rounded-md bg-white/[0.05] border border-white/[0.08]
               pr-[26px] pl-[10px] text-[13px] text-white focus:ring-[3px] ...">
<!-- 右侧 chevron.down 12px，right-[8px]，text-white/45，pointer-events-none -->
```

禁止使用浏览器原生下拉箭头（破坏沉浸感）；`<option>` 列表由系统渲染，深色下自动跟随 `color-scheme: dark`（`index.css` 已设，保留）。

### 5.8 复选框 / 单选按钮

```
复选框：w-[14px] h-[14px] rounded-[4px] border border-white/25
        选中：bg-accent-500 border-accent-500，勾 ✓ 用 10px 白色？不——#1C1C1E 深色勾
单选：w-[14px] h-[14px] rounded-full border border-white/25
      选中：border-accent-500 + 中心 6px 圆点 bg-accent-500
```

均配 13px 标签文字，点击区域含标签（`label` 包裹）。当前设置页"样式"用两个描边小按钮当单选（RadioGroup）→ 改用真单选或分段控件（2 项场景分段控件最佳）。

### 5.9 滚动条（overlay 样式）

```css
.scroll-apple { scrollbar-width: thin; scrollbar-color: rgba(255,255,255,0.25) transparent; }
.scroll-apple::-webkit-scrollbar { width: 8px; height: 8px; }
.scroll-apple::-webkit-scrollbar-thumb {
  background: rgba(255,255,255,0.22); border-radius: 9999px;
  border: 2px solid transparent; background-clip: content-box; /* 视觉缩窄为 4px 轨道+4px 滑块 */
}
.scroll-apple::-webkit-scrollbar-thumb:hover { background: rgba(255,255,255,0.38); background-clip: content-box; }
.scroll-apple::-webkit-scrollbar-track { background: transparent; }
```

**现状差异**：当前全局 8px 常驻滚动条（`bg-neutral-700` 深灰块）→ 改为 overlay 式：轨道透明、滑块白 22%、hover 38%，仅滚动时显眼（视觉上更接近 macOS）。卡片流容器用此类。

### 5.10 快捷键帽（kbd）

```
inline-flex h-[18px] min-w-[20px] px-[4px] items-center justify-center
rounded-[4px] bg-white/[0.08] text-[11px] font-medium text-white/50
```

系统字体（非 mono）。卡片底部 `⌘1` 与菜单右侧快捷键统一此样式。

### 5.11 右键菜单（对照 NSMenu）

```
容器：min-w-[200px] rounded-md p-[4px]
      bg-[#2C2C2E]/85 backdrop-blur-[28px] backdrop-saturate-150
      border border-white/[0.12] shadow-menu（见 §6.3）
菜单项：h-[24px] px-[10px] rounded-[4px] text-[13px] text-white/85
        悬停/选中：bg-accent-500 text-[#1C1C1E]（Apple 菜单高亮 = 强调色整行）
        危险项：text-[#FF6969]；选中：bg-[#FF453A] text-white
        禁用项：text-white/25
分隔线：mx-[4px] my-[4px] h-px bg-white/[0.10]（通栏）
复选标记：左侧 13px checkmark，accent-500
```

**现状差异**：当前菜单是浅色思维（`hover:bg-neutral-100`、✓ 文字符号）→ 材质化 + 整行琥珀高亮 + 真 checkmark 图标。

---

## 6. 材质与层次

### 6.1 窗口与面板材质（Tauri 透明窗口方案）

现状 `tauri.conf.json` 已是 `decorations: false + transparent: true + backgroundColor #00000000 + shadow: true`——**这是苹果式无框窗口的正确底座**，但前端 `body` 现在涂了不透明 `#1C1C1E`，导致窗口是**方角、无材质**。改造：

**（a）悬浮面板（主面板，贴底横条）——毛玻璃（vibrancy 近似）**：

```html
<div class="app-surface app-surface--panel">
  <!-- 应用内容 -->
</div>
```

```css
.app-surface {
  height: 100%; width: 100%; overflow: hidden;
  border-radius: var(--radius-window-panel); /* 12px */
  background: var(--material-panel);        /* rgba(30,30,32,0.82) */
  -webkit-backdrop-filter: blur(40px) saturate(1.8);
  backdrop-filter: blur(40px) saturate(1.8);
  border: 1px solid var(--hairline-strong); /* rgba(255,255,255,0.12) */
}
```

- `body`/`#root` 改为透明（去掉 `bg-white dark:bg-[#1C1C1E]`），材质由 `.app-surface` 承担。
- 圆角处 alpha=0 区域天然点击穿透（Tauri 透明窗口特性），符合预期。
- 降级：`@media (prefers-reduced-transparency: reduce)` 时 `--material-panel: #1E1E1E`（不加 blur），与 Apple 系统设置"减少透明度"行为一致。

**（b）主窗口（设置等多页窗口）——近不透明 + 侧边栏材质**：

```css
.app-surface--window {
  border-radius: var(--radius-window-main); /* 10px */
  background: var(--bg-window);             /* #1E1E1E 不透明 */
}
/* 侧边栏用更轻的材质与内容区分隔 */
.sidebar-material {
  background: rgba(255,255,255,0.03);
  border-right: 1px solid var(--hairline);
}
```

**（c）关于窗口阴影**：保留 Tauri `shadow: true`（系统阴影沿矩形窗框，40px 模糊下与圆角面板兼容）；**不要**给 `.app-surface` 加大面积 CSS 外阴影（会被窗口边界裁剪，只在需要内嵌浮层时用 CSS 阴影）。若实测矩形光晕明显，替代方案：窗口尺寸比面板内容各大 20px、内容居中内缩，阴影改由 CSS `box-shadow` 绘制（需同步改 Rust 定位代码，非必要不动）。

### 6.2 卡片 / 分组阴影与边框规范

| 表面 | 填充 | 边框 | 阴影 |
|---|---|---|---|
| 剪贴板卡片 | `rgba(255,255,255,0.055)` | 1px `--hairline` | 无 |
| 卡片 hover | `rgba(255,255,255,0.08)` | 不变 | 无 |
| 卡片选中 | `rgba(255,255,255,0.09)` | 1px `accent-500/70` | `0 0 0 1px rgba(232,163,61,0.45)`（补光环） |
| 设置分组卡 | `#2C2C2E` | 无 | 无 |
| 回复行 / 转换行 | `rgba(255,255,255,0.04)` | 1px `--hairline` | 无 |

**Apple 列表是"填充+hairline"而不是"投影卡片"**——现状 `shadow-sm`（浅色思维的卡片投影）全部移除。

### 6.3 悬浮层阴影（深色模式三层）

```css
--shadow-menu:  0 0 0 0.5px rgba(0,0,0,0.55), 0 8px 32px rgba(0,0,0,0.50);
--shadow-popover: 0 0 0 0.5px rgba(0,0,0,0.5), 0 12px 40px rgba(0,0,0,0.55);
--shadow-modal: 0 0 0 0.5px rgba(0,0,0,0.5), 0 24px 80px rgba(0,0,0,0.60);
--shadow-panel-ambient: 0 32px 80px -16px rgba(0,0,0,0.65); /* 面板环境光（如需 CSS 绘制） */
```

规律：层级越高，阴影越大越柔和；永远带一圈 0.5px 近黑"描边影"（Apple 深色模式浮窗的标志性处理）。

### 6.4 悬浮面板整体视觉处理

- 圆角 12px（四角同径，贴底时下两角同样圆角——与屏幕边距 24px 已留出空间，见 Rust `apply_layout`）。
- 面板内三段横向分区用 hairline 分隔：工具栏区 / 标签区 / 卡片流；底部状态栏 `border-t`。
- 面板材质上**禁止**再放高不透明色块（会破坏毛玻璃），卡片填充 ≤ 白 8%。

---

## 7. 动效规范

### 7.1 缓动曲线与时长

```css
--ease-apple:  cubic-bezier(0.32, 0.72, 0, 1);   /* Apple 标志性"果断平滑"曲线：出现、开关、位移 */
--ease-out:    cubic-bezier(0.4, 0, 0.2, 1);     /* 颜色/透明度过渡 */
--ease-in:     cubic-bezier(0.4, 0, 1, 1);       /* 消失（比出现更快） */
```

| 场景 | 时长 | 曲线 | 属性 |
|---|---|---|---|
| hover 变色/变底 | 120ms | ease-out | background-color, color, border-color, opacity |
| 按钮按压 | 100ms | ease-apple | transform: scale(0.98) |
| 开关切换 | 150ms | ease-apple | transform: translateX |
| 分段/pill 切换 | 120ms | ease-out | background-color, color |
| 菜单/popover 出现 | 150ms | ease-apple | opacity 0→1 + scale 0.96→1 + translateY(-4px)→0 |
| 菜单消失 | 100ms | ease-in | opacity→0 |
| 面板出现 | 200ms | ease-apple | opacity 0→1 + translateY(10px)→0 + scale 0.985→1 |
| 面板消失 | 150ms | ease-in | opacity→0 + scale→0.99 |
| 页面切换（导航） | 150ms | ease-out | opacity 交叉淡化（无位移动画） |
| 模态出现 | 200ms | ease-apple | scrim opacity 150ms + 面板 scale 0.96→1 |

**面板出现关键帧**（快捷键唤起时，从底部 Dock 方向"长出来"的感觉）：

```css
@keyframes panel-in {
  from { opacity: 0; transform: translateY(10px) scale(0.985); }
  to   { opacity: 1; transform: none; }
}
.animate-panel-in { animation: panel-in 200ms var(--ease-apple); }
```

React 挂载时给 `.app-surface--panel` 加此类；隐藏用 150ms 淡出后 `window.hide()`（Rust 侧 toggle 已支持，前端如需退出动画需延迟 hide 150ms——可选优化）。

### 7.2 交互反馈细则

- 所有可点击元素必须有 hover 态（变色或变底），**没有 hover 反馈 = 不像原生**。
- 按压反馈：按钮 `active:scale-[0.98]`；卡片点击 = 直接执行回贴（当前逻辑），加 `active:bg-white/[0.12]` 100ms 反馈即可，不做缩放（列表缩放会抖）。
- 选中卡片过渡：`transition-[background-color,border-color,box-shadow] duration-150`。
- 无障碍：`@media (prefers-reduced-motion: reduce)` 时全局关闭 transform/scale 动画，仅保留 opacity 100ms。

---

## 8. 图标规范

### 8.1 SF Symbols 风格规则

- 线性图标：24 viewBox、`fill="none" stroke="currentColor"`、`stroke-linecap="round" stroke-linejoin="round"`。
- 描边宽度随渲染尺寸：16px 图标 `strokeWidth={1.75}`；20px `strokeWidth={1.75}`；24px `strokeWidth={2}`。现状统一 `strokeWidth="2"` 在 16px 下偏粗，收敛为 1.75。
- 选中/激活态才允许填充（`star.fill` 规则）：收藏星标激活 = `fill="currentColor"`。
- 视觉对齐：16px 工具栏图标用 `h-4 w-4`，与 13px 文字并排时基线对齐用 `flex items-center`，不要手调 margin。

### 8.2 尺寸阶梯

| 尺寸 | Tailwind | 用途 |
|---|---|---|
| 12px | `h-3 w-3` | 徽章内联图标、chevron（下拉/分段内） |
| 14px | `h-3.5 w-3.5` | 搜索框放大镜、行内小图标 |
| 16px | `h-4 w-4` | 工具栏、卡片操作按钮、设置侧边栏（实际 16，见 §9.2 用 16） |
| 20px | `h-5 w-5` | 空状态主图标、设置侧边栏（备选） |
| 24px | `h-6 w-6` | App 图标内剪头、大空状态 |

### 8.3 图标清单（SF Symbols 名 → 用途 → 现状对应）

| SF Symbols | 用途 | 现状 |
|---|---|---|
| `magnifyingglass` | 搜索 | ✅ 已有（统一 14px） |
| `trash` | 清空历史 / 删除 | ✅（hover 变 danger 色） |
| `gearshape` | 设置 | ✅ |
| `square.grid.2x2` | 模块 | ✅ |
| `star` / `star.fill` | 收藏 | ✅（激活 fill rose） |
| `doc.on.doc` | 复制 | ✅ |
| `arrow.up.doc` / `square.and.arrow.up` | 回贴/发送 | 现状自定义箭头，可保留 |
| `plus` | 新建 | ✅ |
| `xmark` | 关闭/移除 | ✅ |
| `chevron.down` / `chevron.up` / `chevron.right` | 下拉/折叠/跳转 | 新增 |
| `checkmark` | 选中/成功 | ✅（菜单内） |
| `tag` | 标签 | 新增（TagBar 空态） |
| `clock` | 时间 | 新增（卡片头部可选） |
| `square.and.arrow.down` / `square.and.arrow.up` | 导入/导出 Excel | ✅（Reply 页） |
| `arrow.triangle.2.circlepath` | 同步 | ✅（Settings tab） |
| `command` | 快捷键说明 | ✅ |
| `paperplane` / `paperplane.fill` | 发送回复 | 现状自定义，可换 |
| `photo` / `doc.text` / `folder` | 类型徽章前置图标（可选） | 新增 |
| `checkmark.circle` / `xmark.circle` | 转换结果成功/失败 | ✅（Convert 页） |
| `exclamationmark.triangle` | 警告提示 | 新增 |
| `lock` | 堆栈锁定 | 新增 |
| `arrow.up.arrow.down` / `arrow.down.arrow.up` | 回复排序 | ✅ |
| `pencil` | 编辑/重命名 | ✅ |
| `pin` / `pin.fill` | 置顶 | 新增（右键菜单已有置顶项） |
| `ellipsis` | 更多 | 新增 |
| `calendar` / `internaldrive` | TTL/磁盘限制说明 | 新增 |

App 图标内 glyph：剪贴板轮廓（现状 SVG）保留，40px 圆角 9px 容器。

---

## 9. 逐页改造方案

> 每页给出：布局结构 → 关键参数 → 现状 vs 目标 → Tailwind 组合示例。所有示例假设 §10 的 token 已落地（`bg-accent-500`、`text-white/60`、`ease-apple` 等）。

**页面现状对照（2026-10-26 校准，实现入口以此列为准）**：

| 本文小节 | 当时的页面 | 现在的落点 |
|---|---|---|
| §9.1 | 主面板 | `src/windows/Panel.tsx`（未变，工具栏已无「模块」按钮） |
| §9.2 | 主窗口侧边栏（历史/回复/转换/模块/设置） | `src/windows/MainWindow.tsx`：只剩设置，七项分功能/系统两组（功能：通用/剪贴板/环形转换/右键菜单；系统：同步/权限/关于） |
| §9.3 | 设置七 tab | `src/pages/Settings.tsx`：通用 / 剪贴板 / 环形转换 / 右键菜单 / 同步 / 权限 / 关于 |
| §9.4 | 快捷回复页 | 管理页已下线（`ReplySettings.tsx` 留在仓库但不再挂载）；选用仍是独立窗口 `src/windows/ReplyPicker.tsx` |
| §9.5 | 文件转换页 | 已移除；改为 Finder 环形盘 `src/windows/RingWindow.tsx` + 设置里的预设配方 `src/pages/RingSettings.tsx` |
| §9.6 | 模块页 | 已下线（`src/pages/Modules.tsx` 留在仓库但不再挂载）；后端安装/授权/贡献点仍生效 |
| §9.7 / §9.8 | 右键菜单 / 托盘 | 未变（右键菜单仍保留模块动作区） |

### 9.1 主面板（悬浮横条 · 历史卡片流）

**布局结构**（自上而下，高度全部固定，中间弹性）：

```
┌──────────────────────────────────────────────┐
│ 工具栏   h-[44px] px-[16px]  [搜索框][分段控件] … [清空][设置]        │  ← data-tauri-drag-region
├──────────────────────────────────────────────┤  hairline
│ 标签行   h-[34px] px-[16px]  [全部][标签 pills…] →                  │  ← 横向滚动 + 渐隐遮罩
├──────────────────────────────────────────────┤
│ 卡片流   flex-1 overflow-x-auto px-[16px] py-[12px] gap-[12px]      │
│         [卡片 168px][卡片][卡片]…                                    │
├──────────────────────────────────────────────┤  hairline
│ 状态栏   h-[36px] px-[16px] text-[12px] text-white/45               │
└──────────────────────────────────────────────┘
```

**关键参数**：

| 元素 | 参数 |
|---|---|
| 面板 | `rounded-[12px]` + 材质（§6.1a），外框 hairline-strong |
| 搜索框 | `w-[240px] h-[28px]`，见 §5.6；右侧分段控件（§5.4） |
| 清空 / 设置 | 28×28 图标按钮（§5.1）；清空 hover 变 `text-[#FF6969]` |
| 标签 pill | `h-[22px]`（§5.5）；标签行空时整行隐藏（现状逻辑保留） |
| 卡片 | `w-[168px] rounded-[10px]`，内部：头部 `h-[28px] px-[10px]`（徽章 18px + 时间 11px + App 图标 16px）、预览 `flex-1 px-[10px] text-[12px]/[1.5] text-[#E8E8ED] line-clamp-[9]`、底部 `h-[30px] px-[10px]`（kbd + 大小 11px + 3 个 22×22 图标按钮） |
| 卡片间距 | `gap-[12px]`；图片预览 `rounded-[6px] object-cover`（内缩 4px，更精致） |
| 状态栏 | 条数 / 体积 / 同步状态 / 快捷键；`text-[12px] text-white/45`；错误 `text-[#FF6969]` |

**现状 vs 目标**：

| 现状（不苹果的点） | 目标 |
|---|---|
| `body` 不透明方角背景，无毛玻璃 | `.app-surface--panel` 圆角 12px + vibrancy 近似 |
| 搜索 + 5 个独立圆 pill 同排，无容器 | 分段控件（外槽 + 选中格） |
| 卡片 `bg-[#2C2C2E]/70` 硬编码 hex、`rounded-xl` | `bg-white/[0.055] rounded-[10px]`，hover/选中四态明确 |
| 卡片预览 `text-sm`(14px) 在 156px 宽下压抑 | 12px + `#E8E8ED`，行高 1.5 |
| 选中态 `shadow-[0_0_0_1px_rgba(232,163,61,0.4)]` 单环 | 1px accent 边 + 0 0 0 1px 补环 + 底色提亮 |
| 工具栏 `py-2` 高度随意 | 固定 44px/34px/36px 三段式 |
| 状态栏浅色思维 `border-neutral-200 dark:border-neutral-700` | hairline 白 8% |
| 空状态一句灰字 | 20px `doc.text` 图标 + 13px 文案 + `py-[32px]` |
| 底部操作按钮 `p-1.5` 热区小 | 22×22（卡片内紧凑尺寸，可接受） |

**Tailwind 组合示例**：

```tsx
{/* 面板表面（App 根容器） */}
<div className="app-surface app-surface--panel flex flex-col animate-panel-in">

  {/* 工具栏 */}
  <header className="flex h-[44px] shrink-0 items-center gap-[8px] px-[16px]"
          data-tauri-drag-region>
    <SearchField />                          {/* §5.6，w-[240px] */}
    <TypeSegment />                          {/* §5.4 分段控件 */}
    <div className="flex-1" />
    <IconBtn title="清空历史" danger><TrashIcon /></IconBtn>
    <IconBtn title="设置"><GearIcon /></IconBtn>
  </header>

  {/* 标签行 */}
  <div className="flex h-[34px] shrink-0 items-center gap-[6px] overflow-x-auto px-[16px]
                  border-b border-white/[0.06] scroll-apple">
    <Pill selected={!tag} onClick={…}>全部</Pill>
    {tags.map(t => <Pill key={t} selected={tag===t}>{t}</Pill>)}
  </div>

  {/* 卡片流 */}
  <div className="scroll-apple min-h-0 flex-1 overflow-x-auto overflow-y-hidden">
    <div className="flex h-full items-stretch gap-[12px] px-[16px] py-[12px]">
      {items.map((item, i) => (
        <article className={cx(
          "group flex w-[168px] shrink-0 cursor-pointer flex-col overflow-hidden",
          "rounded-[10px] border transition-[background-color,border-color,box-shadow] duration-150",
          selected
            ? "border-accent-500/70 bg-white/[0.09] shadow-[0_0_0_1px_rgba(232,163,61,0.45)]"
            : "border-white/[0.08] bg-white/[0.055] hover:bg-white/[0.08]"
        )}>
          {/* 头部 */}
          <div className="flex h-[28px] shrink-0 items-center gap-[6px] px-[10px]">
            <span className="inline-flex h-[18px] items-center rounded-[5px] bg-sky-500/[0.14]
                             px-[6px] text-[11px] font-medium text-sky-300">文本</span>
            <span className="text-[11px] text-white/60">3 分钟前</span>
            <span className="ml-auto truncate text-[11px] text-white/60">Safari</span>
            <img className="h-4 w-4 rounded-[4px]" … />   {/* 来源 App 图标 */}
          </div>
          {/* 预览 */}
          <div className="min-h-0 flex-1 overflow-hidden px-[10px]">
            <p className="line-clamp-[9] whitespace-pre-wrap break-words text-[12px]/[1.5] text-[#E8E8ED]">…</p>
          </div>
          {/* 底部 */}
          <div className="flex h-[30px] shrink-0 items-center gap-[6px] px-[10px]">
            <Kbd>⌘1</Kbd>
            <span className="text-[11px] text-white/45">12 KB</span>
            <div className="ml-auto flex items-center gap-[2px]">
              <CardIconBtn active={fav}><StarIcon filled={fav} /></CardIconBtn>
              <CardIconBtn><CopyIcon /></CardIconBtn>
              <CardIconBtn danger><TrashIcon /></CardIconBtn>
            </div>
          </div>
        </article>
      ))}
    </div>
  </div>

  {/* 状态栏 */}
  <footer className="flex h-[26px] shrink-0 items-center gap-[12px] border-t border-white/[0.06]
                     px-[16px] text-[11px] text-white/45">
    <span>{stats.total} 条</span><span>3.2 MB</span>
    <span className="ml-auto">已同步 · 128 条</span>
    <span>⌘⇧V</span>
  </footer>
</div>
```

### 9.2 主窗口外壳与导航（当时五页 → 现为设置中心七项）

> 现在侧边栏只有设置的分区，分「功能 / 系统」两组：通用、剪贴板、环形转换、右键菜单 / 同步、权限、关于。
> 快捷键 / 快捷回复 / 模块三项已下线（见 `docs/IMPLEMENTATION.md` §13）。

**布局结构**：弃用顶层 Web 式 tab 栏，改为 **macOS 系统设置式双栏**：

```
┌────────────┬─────────────────────────────────┐
│ 侧边栏      │ 内容区                           │
│ w-[200px]  │ flex-1 overflow-auto p-[24px]   │
│             │  （页面切换 150ms 交叉淡化）      │
│ [App 图标] │                                  │
│ ClipBase   │                                  │
│ v0.1.0     │                                  │
│ ────────   │                                  │
│ 功能        │                                  │
│  ⌘ 通用     │                                  │
│  ▤ 剪贴板   │                                  │
│  ◉ 环形转换 │                                  │
│  ⌥ 右键菜单 │                                  │
│ 系统        │                                  │
│  ⇅ 同步     │                                  │
│  ⛨ 权限     │                                  │
│  ⓘ 关于     │                                  │
└────────────┴─────────────────────────────────┘
```

**关键参数**：

| 元素 | 参数 |
|---|---|
| 窗口表面 | `.app-surface--window`（10px 圆角、#1E1E1E 不透明） |
| 侧边栏 | `w-[200px] shrink-0 sidebar-material flex flex-col`；头部 `p-[16px]`：App 图标 `h-10 w-10 rounded-[9px] bg-accent-500`（内 glyph 20px `#1C1C1E`）+ 名称 `text-[13px] font-semibold` + 版本 `text-[11px] text-white/45` |
| 导航项 | `mx-[8px] h-[30px] rounded-md px-[8px] flex items-center gap-[8px] text-[13px]`；图标 16px；未选 `text-white/85`（Apple 侧边栏未选项接近主文本色，不是 60%）；hover `bg-white/[0.06]`；**选中** `bg-white/[0.10] font-medium` + 图标 `text-accent-500`（Apple 规则：选中行灰底、图标取强调色——不要整行琥珀底，那是网页 tab 思维） |
| 导航项间距 | `gap-[2px]`，组 `py-[8px]` |
| 内容区 | `p-[24px]`；页面容器 `mx-auto max-w-[760px]`（防止宽窗口拉散）；页标题 `text-[17px] font-semibold mb-[16px]` |
| 页面切换 | 150ms opacity 交叉淡化（key 变化时两个绝对定位层或简单 key+animate-fade） |

**现状 vs 目标**：

| 现状 | 目标 |
|---|---|
| 顶层 `nav` 横排 5 个按钮，选中 `bg-amber-500 text-neutral-900`（网页 tab 栏） | 系统设置式侧边栏：选中灰底 + 琥珀图标 |
| 无 App 标识区（顶部仅文字 ClipBase） | 侧边栏头部 App 图标 + 名称 + 版本（版本号从此处收敛，SettingsPanel 内的重复头部删除） |
| 页面硬切换无过渡 | 150ms 淡化 |
| 图标仅"转换"有，其余裸文字 | 十项全部配 16px 描线图标（参考名：`gearshape`/`doc.on.clipboard`/`keyboard`/`bubble.left.and.bubble.right`/`circle.circle`/`cursorarrow.click`/`puzzlepiece.extension`/`arrow.triangle.2.circlepath`/`lock.shield`/`info.circle`，实际为内联 SVG） |
| SettingsPanel 自带第二套侧边栏（w-44 琥珀选中） | **删除**，统一到外壳侧边栏（落地后设置项直接成为侧边栏分区，不再嵌套顶部分段控件） |

### 9.3 设置页（十个分区内容）

**布局结构**：

```
内容区 p-[24px] → max-w-[760px] mx-auto
├─ （现为侧边栏分区：点左侧导航直接换内容，不再有顶部 tab 行）
├─ 分组卡片（rounded-[10px] bg-[#2C2C2E]）gap-[16px]，密排用 grid-cols-2
│    行：min-h-[40px] px-[16px]  label 13px | 控件右侧
│    分隔：h-px bg-white/[0.08] mx-[16px]
└─ 主操作按钮右下（保存）h-[28px]
```

**关键参数**：

| 元素 | 参数 |
|---|---|
| tab 切换 | 分段控件（§5.4），容器 `bg-white/[0.08]`；超出宽度可横向滚动 |
| 分组卡 | `rounded-[10px] bg-[#2C2C2E] p-[16px]`；标题 `text-[13px] font-semibold text-white mb-[12px]`；副标题 `text-[12px] text-white/45` |
| 设置行 | `flex min-h-[40px] items-center justify-between gap-[16px]`；label `text-[13px] text-white/85`；右侧控件 |
| 开关 | §5.2（28×17） |
| 滑块 | §5.3 + 右侧数值 `text-[13px] font-medium text-white tabular-nums` |
| 单选（样式横/纵） | 改分段控件（2 项最贴苹果） |
| 下拉（主题/位置/预览） | §5.7 `w-[140px]` |
| kbd 展示（快捷键） | §5.10 + 右侧"修改"文字按钮 `text-[12px] text-accent-400 hover:underline` |
| 黑名单 chip | §5.5 pill + `xmark` 14px 移除 |
| 保存按钮 | 主按钮 §5.1，`mt-[24px] self-end`；**建议**：改为各分组内即时生效（更苹果），保留保存按钮时放内容区底部右下 |
| 堆栈面板 | 分组卡内子区，`border-t border-white/[0.08] pt-[12px] mt-[12px]`；条目行 `text-[12px] text-white/60`；操作按钮次要按钮 small（h-[24px] text-[12px]） |
| 关于页 | App 图标 64px 居中 + 名称 17px semibold + 版本 12px tertiary + 说明 13px secondary 居中，`max-w-[420px] mx-auto py-[24px]`；技术栈说明 12px tertiary |

**现状 vs 目标**：

| 现状 | 目标 |
|---|---|
| 卡片 `border + bg-white dark:bg-[#242426] + shadow-sm` | 无边框、`#2C2C2E` 填充、无阴影 |
| `grid-cols-2 gap-4` 卡片高度参差 | 同行卡片 `items-start`，gap 16；跨列卡 `col-span-2` 保留 |
| Toggle `h-5 w-9`、Switch 滑块 16px | 28×17 / 13px（§5.2） |
| RadioGroup 两个描边小按钮 | 分段控件 |
| 原生 range 无样式统一处理 | §5.3 统一样式（当前 `accent-amber-500` 保留为填充色来源） |
| "修改"按钮 `text-amber-600 dark:text-amber-400` | `text-accent-400`，12px |
| 双列网格里"历史记录限制"内又套 2 列，滑块刻度 10px 贴边 | 刻度行 `px-[2px]`，两端文字与轨道端点对齐 |
| 深/浅色类名混写（`dark:` 前缀遍布） | 深色优先后**删除全部 `dark:` 变体**，仅保留一套深色值（后续做浅色再引入 `.light` 主题变量整体切换） |

**Tailwind 组合示例**：

```tsx
<div className="mx-auto max-w-[760px] space-y-[16px]">
  <TypeSegment items={TABS} active={active} onChange={setActive} />

  <section className="rounded-[10px] bg-[#2C2C2E] p-[16px]">
    <h4 className="mb-[12px] text-[13px] font-semibold text-white">交互设置</h4>
    <div className="space-y-[2px]">
      <Row label="数字快捷键（⌘1~9 粘贴第 N 条）">
        <Switch checked={v} onChange={setV} />
      </Row>
      <Row label="操作完成后自动关闭窗口">
        <Switch checked={v} onChange={setV} />
      </Row>
    </div>
  </section>

  <div className="grid grid-cols-2 items-start gap-[16px]">
    <section className="rounded-[10px] bg-[#2C2C2E] p-[16px]">…</section>
    <section className="rounded-[10px] bg-[#2C2C2E] p-[16px]">…</section>
  </div>

  <div className="flex justify-end">
    <button className="h-[28px] rounded-md bg-accent-500 px-[12px] text-[13px] font-medium
                       text-[#1C1C1E] transition-colors duration-120 hover:bg-accent-400
                       active:scale-[0.98]">保存</button>
  </div>
</div>
```

### 9.4 快捷回复（管理在设置中心，选用是独立窗口）

> 管理是设置中心的「快捷回复」分区 `src/components/ReplySettings.tsx`（分组 / 条目 / 排序 / Excel 导入导出）；
> 选用是独立小窗口 `src/windows/ReplyPicker.tsx`（快捷键唤出、选完即关）。
> 下面的列表 / 编辑弹窗设计在管理分区内适用，「沿用侧边栏外壳」只在设置中心成立。

**布局结构**：沿用 §9.2 侧边栏外壳；页内：

```
├─ 工具栏行 h-[44px]：[分组 pills（可滚动）][+ 新建分组] | [搜索框 w-[200px]] [导入][导出] [新建回复(主按钮)]
├─ （条件）结果提示条：h-[36px] rounded-[8px]（成功/失败/摘要）
├─ 回复列表 flex-1 overflow-auto p-[16px] space-y-[8px]
└─ 编辑弹窗（模态）
```

**关键参数**：

| 元素 | 参数 |
|---|---|
| 分组 pill | §5.5（h-[22px]）；选中 tint 琥珀；"全部"同为 pill |
| 新建分组 | 28×28 图标按钮 `plus`，放 pills 末尾 |
| 搜索框 | §5.6 `w-[200px]` |
| 导入/导出 | 次要按钮 h-[28px] + 16px 图标；导入中 `disabled` 态 |
| 新建回复 | 主按钮（唯一实心琥珀，视觉终点） |
| 结果提示条 | `flex h-[36px] items-center gap-[8px] rounded-[8px] px-[12px] text-[12px]`；成功 `bg-[#32D74B]/12 text-[#5FE08C]`；失败 `bg-[#FF453A]/12 text-[#FF8A80]`；摘要 `bg-white/[0.06] text-white/70`；右侧关闭 16px 图标按钮；**替代现状三色横幅**（emerald/rose/amber 500/10 的高饱和条） |
| 回复行 | `rounded-[10px] bg-white/[0.04] border border-white/[0.08] p-[12px] hover:bg-white/[0.07]`；标题 `text-[13px] font-semibold`；分组徽章 pill 10px；预览 `text-[12px] text-white/60 line-clamp-2`；右侧操作：上移/下移/编辑/删除 24×24 图标按钮 + "发送"主按钮 small（`h-[24px] px-[10px] text-[12px] rounded-[5px] bg-accent-500 text-[#1C1C1E]`） |
| 拖拽提示 | `rounded-[12px] border-2 border-dashed border-accent-500/60 bg-black/50 backdrop-blur`，文案 `text-[13px] text-accent-400`（现状基本符合，补材质） |
| 编辑弹窗 | `w-[480px] rounded-[12px] bg-[#2C2C2E]/95 backdrop-blur-[28px] border border-white/[0.12] shadow-modal p-[20px]`；标题 `text-[15px] font-semibold`；表单 label `text-[12px] text-white/60 mb-[4px]`；输入框 §5.6；textarea `min-h-[120px] resize-none`；底部按钮右对齐：取消（次要）+ 保存（主）；scrim `bg-black/40` 150ms 淡入，弹窗 200ms scale 0.96→1 |
| 表情 popover | `w-[256px] rounded-[10px] bg-[#2C2C2E]/90 backdrop-blur border border-white/[0.12] shadow-popover p-[8px]`；格子 28×28 `rounded-[6px] hover:bg-white/10 text-[16px]` |

**现状 vs 目标**：

| 现状 | 目标 |
|---|---|
| 顶部一排 4 类按钮争抢（导入/导出/新建同级） | 层级：新建回复=主按钮；导入/导出=次要；搜索/分组=中性 |
| 三色高饱和提示横幅 | 12% 透明度语义色提示条 + 图标 |
| 行 `bg-white/5 rounded-xl` hover 变琥珀边 | 统一白 4%/7%，hover 只变底色不变边框色 |
| 弹窗 `bg-[#242426]` 硬编码 | `bg-[#2C2C2E]/95` + blur + shadow-modal |
| `window.prompt/confirm/alert` 原生弹窗（破坏沉浸） | 应用内模态（删除确认 = 小模态：图标 + 文案 + 取消/删除(危险填充)）——本期可先保留原生，列入 P1 |
| emoji 面板 `bg-[#2a2a2c]` 硬编码 | token 化 |

### 9.5 转换页（已移除 · 现为 Finder 环形盘）

> 本节描述的是早期的「选文件 → 选目标格式 → 批量转」独立页面，该页已删。
> 现在的转换入口是 Finder 里唤出的环形盘（`src/windows/RingWindow.tsx`），
> 动作由「预设配方」定义，配置在设置 → 环形转换（`src/pages/RingSettings.tsx`）。
> 下面保留的是当时的布局记录，**不要照此重建页面**。

**布局结构**：

```
├─ 工具栏 h-[44px]：[标题 13px semibold][副标题 12px tertiary] … [选择文件(主按钮)]
├─ 内容 flex-1 overflow-auto p-[16px] space-y-[16px] max-w-[760px] mx-auto
│   ├─ 路径输入行（输入框 + 添加 次要按钮）+ 说明 12px tertiary
│   ├─ 文件列表：分组卡（空态 = 拖拽区）
│   ├─ 转换选项分组卡（2 列：目标格式下拉 / 输出目录；JPEG 质量滑块条件出现）
│   ├─ [开始转换] 主按钮 h-[32px] w-full（禁用态明确）
│   └─ 结果列表
```

**关键参数**：

| 元素 | 参数 |
|---|---|
| 空态拖拽区 | `rounded-[10px] border border-dashed border-white/[0.15] bg-white/[0.02] py-[32px]`，居中：20px `photo` 图标 white/30 + 13px white/45"尚未选择文件"+ 12px tertiary 提示；拖拽激活时 `border-accent-500/60 bg-accent-500/[0.06]` |
| 文件行 | `rounded-[10px] bg-white/[0.04] border border-white/[0.08] p-[12px]`；文件名 `text-[13px] text-white`；格式徽章 pill 10px（§5.5 风格，tint 色按格式）；路径 `text-[11px] text-white/45 truncate font-mono`；移除 24×24 图标按钮 |
| 选项分组卡 | 同 §9.3 分组卡；字段 label `text-[12px] text-white/60 mb-[4px]` |
| JPEG 质量滑块 | §5.3 + 数值右显 `tabular-nums` |
| 开始转换 | `h-[32px] w-full rounded-md bg-accent-500 text-[13px] font-medium text-[#1C1C1E]`；disabled = accent/30 + white/30；loading 时按钮内 14px spinner + "转换中…" |
| 结果行 | 图标 16px（成功 `#32D74B` checkmark.circle / 失败 `#FF6969` xmark.circle）+ 文件名 13px + 输出路径 11px tertiary + "在 Finder 中显示"次要按钮 small |
| 拖拽悬浮提示 | 同 §9.4 |

**现状 vs 目标**：

| 现状 | 目标 |
|---|---|
| 虚线空框一句灰字 | 图标 + 双行文案 + 拖拽激活态 |
| 文件行 `hover:border-amber-500/40` | hover 只变底色 |
| 目标格式/输出目录裸 label+input 两列 | 收进分组卡 |
| `accent-amber-500` 裸 range | §5.3 统一样式 |
| 结果统计 `成功 N · 失败 M` 10px 灰字 | `text-[11px] text-white/45 tabular-nums`，配 16px 状态图标 |
| 错误信息 `text-rose-400` | `text-[#FF6969] text-[12px]` |

### 9.6 模块（现为设置中心分区 · `src/pages/Modules.tsx`）

> 主窗口收窄为设置中心时模块页被移除过；现已按本规范重建为「设置 → 模块」分区。
> 与当时设计稿的差别：安装不再手输路径，改成原生目录选择器；授权不做成常驻 chip，
> 而是点「启用」时就地展开可选清单（只有勾过的能力才会传给 `enable_module`）。
> 模块自带 UI（`read_module_ui`）尚未内嵌渲染，因此没有 iframe 预览区。

**布局结构**：内容区 `max-w-[760px] mx-auto space-y-[16px]`：

- 顶部卡：`安装模块…`（原生目录选择）/ `打开模块目录` / `刷新` 三个 sm 按钮 + 12px tertiary 说明。
- 授权条：授权未完成时在列表上方展示 `Notice tone="info"`，内含逐项 checkbox +「授予并启用」。
- 模块卡：`rounded-[10px] bg-group p-[16px]`：
  - 头部：模块名 `text-[13px] font-semibold` + 版本 `text-[11px] tabular-nums` + 运行时 pill + 状态点（6px 圆点：running `bg-success`、crashed `bg-danger`、stopped white/30）+ 右侧「启用/停用」「卸载」sm 按钮。
  - 权限行：manifest 声明的能力逐个 pill，已授予 = 选中样式（tint 琥珀），未授予 = 默认 pill。
  - 贡献行：`托盘：xxx` 等 11px 灰底小块。
  - 崩溃时：卡片内补一条 `Notice tone="danger"`，提示先停用再启用。

**现状 vs 目标**：状态文字 `text-emerald-500/rose-500` → 状态点 + 文字；卡片边框+填充 → 无边框 `bg-group`；操作按钮统一控件规范。

### 9.7 右键菜单（ClipContextMenu）

按 §5.11 实施。结构：回贴 / 复制 / 加入堆栈 / 置顶⇄ / 收藏⇄ / 标签子区（现有标签列表 + ✓）/ 模块动作区 / 删除（危险，最底部）。分隔线按语义分组。菜单位置：`fixed` + 边界翻转（右/下溢出时贴反向边）——现状需确认，建议补 8px 屏幕边距。出现动画 150ms scale 0.96→1 ease-apple。

### 9.8 托盘菜单（已取消）

> 2026-10-27 起不再有菜单栏图标，本节保留为设计留档；设置与退出改由面板右上角齿轮 / 设置页提供（见 `docs/IMPLEMENTATION.md` 第 12 章）。

- 菜单项补 SF Symbols 风格模板图标（`MenuItem::with_id` 的 icon 参数：`gearshape`/`arrow.clockwise`/`trash`/`power`），20px 黑白模板图。
- "最近"子菜单条目预览截断 40 字符（现状），改为 `text.truncate` 到 ~48 字符 + 副行类型徽章不可用（NSMenuItem 支持 attributed title 有限，保持纯文本即可）。
- 退出项放最后并加分隔线（现状已符合）。

---

## 10. 实现映射表

### 10.1 `tailwind.config.ts`（完整可替换）

```ts
import type { Config } from "tailwindcss";

export default {
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  darkMode: "class",
  theme: {
    extend: {
      colors: {
        /* 强调色：琥珀色阶 */
        accent: {
          DEFAULT: "#E8A33D",
          50: "#FDF6EC", 100: "#F9E8CD", 200: "#F2D0A0", 300: "#EDB874",
          400: "#F0B35A", 500: "#E8A33D", 600: "#D68F2B",
          700: "#B3741F", 800: "#8A5816", 900: "#5F3D10",
          on: "#1C1C1E",            /* 琥珀底上的文字 */
          tint: "rgba(232,163,61,0.14)",
          "tint-text": "#F0B35A",
          disabled: "rgba(232,163,61,0.32)",
        },
        /* 背景层级 */
        win: "#1E1E1E",            /* 窗口底 */
        group: "#2C2C2E",          /* 分组卡/模态 */
        raised: "#3A3A3C",         /* 抬升面 */
        preview: "#E8E8ED",        /* 预览正文 */
        /* 语义色 */
        danger: { DEFAULT: "#FF6969", fill: "#FF453A" },
        success: "#32D74B",
        warning: "#FFD60A",
      },
      fontFamily: {
        sans: [
          "-apple-system", "BlinkMacSystemFont", "SF Pro Text", "SF Pro Display",
          "PingFang SC", "Helvetica Neue", "sans-serif",
        ],
        mono: ["ui-monospace", "SFMono-Regular", "Menlo", "monospace"],
      },
      transitionTimingFunction: {
        apple: "cubic-bezier(0.32, 0.72, 0, 1)",
        "out-soft": "cubic-bezier(0.4, 0, 0.2, 1)",
        "in-soft": "cubic-bezier(0.4, 0, 1, 1)",
      },
      boxShadow: {
        menu: "0 0 0 0.5px rgba(0,0,0,0.55), 0 8px 32px rgba(0,0,0,0.50)",
        popover: "0 0 0 0.5px rgba(0,0,0,0.5), 0 12px 40px rgba(0,0,0,0.55)",
        modal: "0 0 0 0.5px rgba(0,0,0,0.5), 0 24px 80px rgba(0,0,0,0.60)",
        "select-ring": "0 0 0 1px rgba(232,163,61,0.45)",
      },
      keyframes: {
        "panel-in": {
          from: { opacity: "0", transform: "translateY(10px) scale(0.985)" },
          to: { opacity: "1", transform: "none" },
        },
        "fade-in": { from: { opacity: "0" }, to: { opacity: "1" } },
        "pop-in": {
          from: { opacity: "0", transform: "scale(0.96) translateY(-4px)" },
          to: { opacity: "1", transform: "none" },
        },
      },
      animation: {
        "panel-in": "panel-in 200ms cubic-bezier(0.32, 0.72, 0, 1)",
        "fade-in": "fade-in 150ms cubic-bezier(0.4, 0, 0.2, 1)",
        "pop-in": "pop-in 150ms cubic-bezier(0.32, 0.72, 0, 1)",
      },
    },
  },
  plugins: [],
} satisfies Config;
```

> 圆角：Tailwind 默认 `rounded-md=6px / rounded-lg=8px / rounded-xl=12px` 恰好覆盖 6/8/12；5/7/10 用任意值 `rounded-[5px]` 等。间距默认 4 的倍数（`p-4=16px`、`gap-3=12px`、`py-6=24px`）与 8pt 网格一致，直接用。

### 10.2 `src/index.css`（token 与组件类，完整可替换）

```css
@tailwind base;
@tailwind components;
@tailwind utilities;

:root {
  color-scheme: dark;

  /* 背景 */
  --bg-window: #1E1E1E;
  --bg-group: #2C2C2E;
  --bg-raised: #3A3A3C;
  --fill-input: rgba(255, 255, 255, 0.05);
  --fill-hover: rgba(255, 255, 255, 0.08);
  --fill-active: rgba(255, 255, 255, 0.12);
  --fill-selected: rgba(255, 255, 255, 0.10);
  --scrim: rgba(0, 0, 0, 0.40);

  /* 强调色 */
  --accent-500: #E8A33D;
  --accent-400: #F0B35A;
  --accent-600: #D68F2B;
  --accent-on: #1C1C1E;
  --accent-tint: rgba(232, 163, 61, 0.14);
  --accent-disabled: rgba(232, 163, 61, 0.32);

  /* 文本 */
  --text-primary: #FFFFFF;
  --text-preview: #E8E8ED;

  /* 边框 */
  --hairline: rgba(255, 255, 255, 0.08);
  --hairline-strong: rgba(255, 255, 255, 0.12);
  --hairline-focus: rgba(232, 163, 61, 0.70);

  /* 语义色 */
  --success: #32D74B;
  --warning: #FFD60A;
  --danger-text: #FF6969;
  --danger-fill: #FF453A;

  /* 材质 */
  --material-panel: rgba(30, 30, 32, 0.82);
  --material-menu: rgba(44, 44, 46, 0.85);
  --material-modal: rgba(44, 44, 46, 0.95);

  /* 圆角 */
  --radius-window-panel: 12px;
  --radius-window-main: 10px;

  /* 动效 */
  --ease-apple: cubic-bezier(0.32, 0.72, 0, 1);
  --ease-out: cubic-bezier(0.4, 0, 0.2, 1);
  --ease-in: cubic-bezier(0.4, 0, 1, 1);

  font-family: var(--font-sf, -apple-system, BlinkMacSystemFont, "PingFang SC", sans-serif);
  -webkit-font-smoothing: antialiased;
}

html, body, #root { height: 100%; margin: 0; }
body { background: transparent; }   /* 窗口为 transparent:true，底色由 .app-surface 承担 */

@layer components {
  /* 窗口/面板表面（透明 Tauri 窗口的唯一底色来源） */
  .app-surface {
    @apply h-full w-full overflow-hidden;
    background: var(--bg-window);
    border: 1px solid var(--hairline-strong);
  }
  .app-surface--panel {
    border-radius: var(--radius-window-panel);
    background: var(--material-panel);
    -webkit-backdrop-filter: blur(40px) saturate(1.8);
    backdrop-filter: blur(40px) saturate(1.8);
  }
  .app-surface--window { border-radius: var(--radius-window-main); }
  @media (prefers-reduced-transparency: reduce) {
    .app-surface--panel { background: var(--bg-window); backdrop-filter: none; }
  }

  /* 输入 / 搜索 / 下拉 */
  .input {
    @apply h-[28px] w-full rounded-md border border-white/[0.08] bg-white/[0.05]
           px-[10px] text-[13px] text-white outline-none
           placeholder:text-white/35
           focus:border-accent-500/70 focus:shadow-[0_0_0_3px_rgba(232,163,61,0.30)]
           transition-[border-color,box-shadow] duration-120 ease-out-soft;
  }

  /* 按钮 */
  .btn {
    @apply inline-flex h-[28px] items-center justify-center gap-[6px] rounded-md px-[12px]
           text-[13px] font-medium transition-[background-color,transform] duration-100 ease-apple
           active:scale-[0.98] disabled:cursor-not-allowed;
  }
  .btn-primary { @apply bg-accent-500 text-[#1C1C1E] hover:bg-accent-400 active:bg-accent-600
                        disabled:bg-accent-500/30 disabled:text-white/30; }
  .btn-secondary { @apply border border-white/10 bg-white/[0.06] text-[#F5F5F7]
                          hover:bg-white/10 active:bg-white/[0.12]; }
  .btn-danger { @apply bg-danger-fill text-white hover:brightness-110; }
  .btn-sm { @apply h-[24px] px-[10px] text-[12px] rounded-[5px]; }
  .btn-icon {
    @apply grid h-7 w-7 place-items-center rounded-md text-white/55
           transition-colors duration-120 ease-out-soft
           hover:bg-white/[0.08] hover:text-white active:bg-white/[0.12];
  }
  .btn-icon-danger { @apply btn-icon hover:bg-danger-fill/10 hover:text-[#FF6969]; }

  /* 分段控件 / pill / kbd */
  .seg { @apply inline-flex h-[26px] items-center gap-[2px] rounded-[7px] bg-white/[0.08] p-[2px]; }
  .seg-item { @apply inline-flex h-[22px] items-center rounded-[5px] px-[12px] text-[12px]
                     text-white/60 transition-colors duration-120 ease-out-soft
                     hover:text-white/85; }
  .seg-item-active { @apply bg-accent-500 text-[#1C1C1E] font-medium shadow-[0_1px_2px_rgba(0,0,0,0.3)]; }
  .pill { @apply inline-flex h-[22px] shrink-0 items-center rounded-full bg-white/[0.06]
                 px-[10px] text-[12px] text-white/60 transition-colors duration-120
                 hover:bg-white/10 hover:text-white/85; }
  .pill-active { @apply bg-accent-tint text-accent-tint-text ring-1 ring-accent-500/30; }
  .kbd { @apply inline-flex h-[18px] min-w-[20px] items-center justify-center rounded-[4px]
                bg-white/[0.08] px-[4px] text-[11px] font-medium text-white/50; }

  /* 开关 */
  .switch { @apply relative h-[17px] w-[28px] shrink-0 rounded-full transition-colors duration-150 ease-apple; }
  .switch-on { @apply bg-accent-500; }
  .switch-off { @apply bg-white/[0.16]; }
  .switch-thumb { @apply absolute top-[2px] left-[2px] h-[13px] w-[13px] rounded-full bg-white
                         shadow-[0_1px_2px_rgba(0,0,0,0.4)] transition-transform duration-150 ease-apple; }
  .switch-on .switch-thumb { @apply translate-x-[11px]; }

  /* 滑块 */
  .slider { @apply h-4 w-full cursor-pointer appearance-none bg-transparent; }
  .slider::-webkit-slider-runnable-track { @apply h-[4px] rounded-full bg-white/10; }
  .slider::-webkit-slider-thumb {
    -webkit-appearance: none; margin-top: -5.5px; @apply h-[15px] w-[15px] rounded-full bg-white;
    box-shadow: 0 1px 3px rgba(0,0,0,0.5), 0 0 0 0.5px rgba(0,0,0,0.15);
    transition: transform 100ms var(--ease-apple);
  }
  .slider:active::-webkit-slider-thumb { transform: scale(1.12); }
  .slider:focus-visible { box-shadow: 0 0 0 3px rgba(232,163,61,0.35); border-radius: 9999px; }
  .slider { accent-color: var(--accent-500); }

  /* 卡片 / 分组 / 菜单 */
  .card-clip {
    @apply flex w-[168px] shrink-0 flex-col overflow-hidden rounded-[10px] border
           border-white/[0.08] bg-white/[0.055]
           transition-[background-color,border-color,box-shadow] duration-150 ease-out-soft;
  }
  .card-clip:hover { @apply bg-white/[0.08]; }
  .card-clip-selected { @apply border-accent-500/70 bg-white/[0.09] shadow-[0_0_0_1px_rgba(232,163,61,0.45)]; }
  .card-group { @apply rounded-[10px] bg-group p-[16px]; }
  .menu {
    @apply min-w-[200px] rounded-md border border-white/[0.12] p-[4px];
    background: var(--material-menu);
    -webkit-backdrop-filter: blur(28px) saturate(1.5);
    backdrop-filter: blur(28px) saturate(1.5);
    @apply shadow-menu;
  }
  .menu-item { @apply flex h-[24px] w-full items-center gap-[8px] rounded-[4px] px-[10px]
                      text-[13px] text-white/85 transition-colors duration-75; }
  .menu-item:hover, .menu-item-active { @apply bg-accent-500 text-[#1C1C1E]; }
  .menu-item-danger { @apply text-[#FF6969]; }
  .menu-item-danger:hover { @apply bg-danger-fill text-white; }
  .menu-sep { @apply mx-[4px] my-[4px] h-px bg-white/10; }

  /* 滚动条（overlay 近似） */
  .scroll-apple { scrollbar-width: thin; scrollbar-color: rgba(255,255,255,0.22) transparent; }
  .scroll-apple::-webkit-scrollbar { width: 8px; height: 8px; }
  .scroll-apple::-webkit-scrollbar-thumb {
    background: rgba(255,255,255,0.22); border-radius: 9999px;
    border: 2px solid transparent; background-clip: content-box;
  }
  .scroll-apple::-webkit-scrollbar-thumb:hover {
    background: rgba(255,255,255,0.38); background-clip: content-box;
  }
  .scroll-apple::-webkit-scrollbar-track { background: transparent; }

  /* 富文本预览：重置 HTML 剪贴板内容自带的段落/列表间距 */
  .clip-rich-preview p, .clip-rich-preview ul, .clip-rich-preview ol { margin: 0; }
}

@media (prefers-reduced-motion: reduce) {
  *, *::before, *::after { animation-duration: 0.01ms !important; transition-duration: 100ms !important; }
  .btn:active, .switch-thumb { transform: none !important; }
}
```

### 10.3 Token → 语义色速查（组件里只允许这些组合）

| Token 名 | CSS 变量 | Tailwind 写法 | 用途 |
|---|---|---|---|
| accent-500 | `--accent-500` | `bg-accent-500` / `text-accent-500` | 主按钮、开关 on、选中 |
| accent-400 | `--accent-400` | `bg-accent-400` / `text-accent-400` | hover、链接文字 |
| accent-600 | `--accent-600` | `bg-accent-600` | active |
| accent-on | `--accent-on` | `text-[#1C1C1E]` | 琥珀底文字 |
| bg-window | `--bg-window` | `bg-win` | 窗口/内容区 |
| bg-group | `--bg-group` | `bg-group` | 分组卡、模态 |
| fill-hover | `--fill-hover` | `bg-white/[0.08]` | hover 填充 |
| text-primary | `--text-primary` | `text-white` | 主文本 |
| text-secondary | — | `text-white/60` | 次文本 |
| text-tertiary | — | `text-white/45` | 三级文本 |
| text-quaternary | — | `text-white/25` | 禁用/装饰 |
| text-preview | `--text-preview` | `text-preview` | 预览正文 |
| hairline | `--hairline` | `border-white/[0.08]` | 卡片边、分隔线 |
| hairline-strong | `--hairline-strong` | `border-white/[0.12]` | 输入框边、面板外框 |
| success / warning / danger | 同名变量 | `text-success` / `text-danger` | 语义色 |
| material-panel | `--material-panel` | `.app-surface--panel` | 面板材质 |
| shadow-menu / popover / modal | — | `shadow-menu` 等 | 悬浮层阴影 |
| ease-apple | `--ease-apple` | `ease-apple` | 位移动画曲线 |

### 10.4 落地状态（原施工清单，已全部完成）

| 原步骤 | 状态 | 备注 |
|---|---|---|
| 第一步：替换 `tailwind.config.ts` + `index.css`，body 透明、App 根包窗口材质 | 已落地 | token 与类名见 §10.1/10.2 |
| 第二步：抽公共控件库 | 已落地 | 集中在 `src/components/ui.tsx`（Btn/Pill/Segment/Switch/Slider/SearchField/IconBtn…） |
| 第三步：主面板重排（固定三段高度、分段控件、卡片四态） | 已落地 | `src/windows/Panel.tsx` |
| 第四步：主窗口改侧边栏外壳 | 已落地（有调整） | 设置项直接成为侧边栏分区，没有再加顶部分段控件 |
| 第五步：Reply / Convert / Modules 收敛 | 已落地（有调整） | 回复变独立窗口；Convert 页与 Modules 页已移除（见 §9 开头对照表） |
| 第六步：右键菜单材质化 + 动效 + 滚动条 | 已落地 | `src/components/ClipContextMenu.tsx` / `src/index.css` |

---

## 附：验收清单（UI Review 对照）

- [ ] 窗口四角为 12px（面板）/10px（主窗口）圆角，无方角、无白边
- [ ] 面板有毛玻璃透出桌面，减少透明度偏好下自动变实底
- [ ] 全项目无 `dark:` 前缀残留、无硬编码 hex（`#2C2C2E`/`#242426`/`#2a2a2c`/`#1e1e2e` 清零）
- [ ] 字号只有 11/12/13/15/17/20/28 七档；间距只有 4/8/12/16/24/32
- [ ] 圆角只有 5/6/7/8/10/12/full
- [ ] 所有 hover ≤150ms、按压 scale 0.98、面板出现 200ms ease-apple
- [ ] 琥珀只出现在：主按钮、选中态、聚焦光环、强调图标
- [ ] 每个图标按钮有 title；开关有 aria/role；对比度全部 ≥4.5:1
- [ ] 空状态/加载/错误三态齐全且样式统一
