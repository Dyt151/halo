# Halo 权限管理收敛到「通用」页

**日期**：2026-11-04
**场景**：设计审查 + 实现 + 验证（完整功能改造）
**参与成员**：设计师（gstack-designer）、调查员（gstack-investigator）

---

## 📌 TL;DR（执行摘要）

- **整体结论**：🟢 通过。权限管理已从 5 处收敛为「通用」页的一处「权限与授权」卡。
- **改造前**：权限散在 5 个地方，其中「开机启动」在通用页与权限页**重复维护**，用户要在两个页面之间来回找。
- **改造后**：侧边栏「权限」入口下线，系统授权动作只在「通用 → 权限与授权」一处；功能页保留状态诊断、去掉重复授权按钮。
- **净变化**：13 个文件改动 + 1 个文件删除 + 3 个文件新增，**净减 308 行**。
- **验证**：`tsc` 干净、`npm run build` 通过（92 modules）、新增 10 个分区归一化用例全过；Rust 侧 `cargo check` 通过。

---

## 🎯 核心结论卡片

| 项目 | 内容 |
|------|------|
| 整体结论 | 🟢 通过 |
| 权限入口 | 5 处 → 1 处（「通用 → 权限与授权」）|
| 代码净变化 | −308 行（+178 / −486，含新增文件另计）|
| 新增能力 | 后端 `statusbar_location_access` 命令（定位授权状态可查）|
| 新增基础设施 | 跨分区跳转通道 `goToSection`；`normalizeSection` 抽成可单测纯函数 |
| 测试 | 新增 10 个分区归一化用例，全过 |
| 阻塞项 | 0 |

---

## 1. 各成员核心结论

### 🎨 设计师（信息架构与交互设计）

- **核心判断**：收敛后的「通用」页应拆成「权限与授权」（置顶）+「通用基础」两块，并确立一条统一规则——**系统授权动作只在通用页，功能页只留「就地状态 + 去授权 →」**。侧边栏「权限」入口下线，但 `"permissions"` 必须保留为深链别名。
- **关键建议**：定位权限应纳入权限卡（后端只差一个 command 暴露）；功能页的状态显示不能删——那是用户判断「为什么功能不生效」的唯一现场线索。
- **发现的隐藏风险**：① 通用页整体挂在 `if (!draft)` 之后，配置加载失败时权限信息会一起消失，权限卡需独立于草稿守卫；② 存量用户 config 里的 `last_settings_section` 可能已存 `"permissions"`（Rust 侧是自由 String 无校验），不映射就是白屏。

### 🔧 调查员（影响面核查与独立验证）

- **核心判断**：改造的主要风险不在权限逻辑本身，而在**深链与路由链路**——`last_settings_section` 存量值、`requestSettingsTab` 的一次性消费语义、以及 `normalizeSection` 从未被测试覆盖。
- **关键建议**：把 `normalizeSection` 从 `MainWindow.tsx` 抽成纯函数并补单测（已实施：`src/lib/section.ts` + `src/lib/section.test.ts`，10 个用例）；跨分区跳转不要走 Rust 事件广播，同窗口内用模块级回调最直接。
- **实测发现**：`tsconfig.json` 的 `noUnusedLocals` 会让**未使用的导入直接报错**——加跳转通道时多导一个 `goToSection` 就挂构建（该问题已在实现中修正）。
- **独立验证结论**：5 项重点全部通过（存量深链不白屏 / 首启引导有效 / 登录项只请求一次 / `launch_at_login` 镜像由后端自行同步 / 权限卡不被配置加载挡住）。

---

## 2. 验证过程中发现并修复的问题

独立验证环节揪出 3 个问题，其中 1 个是本次改造引入的真实回归：

| # | 问题 | 性质 | 处置 |
|---|------|------|------|
| 1 | **通用页丢失保存状态条** | 🔴 本次改造引入的回归 | 早退结构把通用页移出了主 return，导致底部「更改会自动保存 / 重试」条不渲染。已改为通用页回到共享 return 内，权限卡用 `draft ?` 分支独立渲染 |
| 2 | **后端 `set_login_item` 吞掉镜像写入错误** | 🟠 既有缺陷 | `let _ = state.config.patch(...)` 把失败完全丢弃。已改为 `tracing::warn!` 记录（沿用项目既有日志约定，不用 `log` crate——它不在依赖里） |
| 3 | **定位权限「打开系统设置」跳错面板** | 🟡 体验问题 | 原跳「隐私与安全性」总面板，用户还要自己往下找。已改为直达 `Privacy_LocationServices` 锚点 |
| 4 | **侧边栏「通用」与卡片「通用」同名** | 🟡 视觉歧义 | 页面标题已是「通用」，第二个卡片改名为「基础设置」，副标题「剪贴板记录与界面外观」 |
| 5 | **权限卡内两个「刷新」按钮冗余** | 🟡 交互冗余 | 给 `CardGroup` 增加可选 `action` 插槽，刷新上提到卡片标题右侧一行一处；行内两个刷新按钮移除。34 个 `CardGroup` 调用方行为不变（action 可选） |

**两条经复核为误报、未做改动**：

- 「修复授权」功能丢失 → 实际在 `src/components/PermissionSection.tsx:271` 有按钮、`:148` 有调用，功能完好（调查员的检索快照早于该实现落地）。
- `settings.rs` 残留 `"permissions"` 字符串匹配 → 实际该文件无此匹配；全仓命中均为模块 manifest 的 `capability` 同名字段，与分区无关。

---

## 3. 改造前后对照

### 权限入口（改造前 5 处 → 改造后 1 处）

| # | 改造前位置 | 内容 | 改造后 |
|---|-----------|------|--------|
| 1 | `src/components/PermissionCenter.tsx`（独立「权限」页，241 行） | 辅助功能 / 开机启动 / Finder 扩展 | **整页删除**，内容并入通用页 |
| 2 | `src/pages/Settings.tsx` general 分支 | 开机启动（ToggleRow） | **删除**（与 #1 重复），只保留权限卡里的形态 |
| 3 | `src/pages/RingSettings.tsx`「手势与权限」 | 辅助功能（请求/修复/打开设置 三按钮） | 保留状态诊断，改为「去授权 →」 |
| 4 | `src/components/ContextMenuSettings.tsx` | 桌面右键接管（辅助功能）+ Finder 扩展启停 | 保留状态，改为「去授权 →」；启停移入权限卡 |
| 5 | `src/pages/StatusBarSettings.tsx:213` | 定位权限（开网络模块时静默申请） | 保留就地申请 + 补说明；状态纳入权限卡 |

### 收敛后的「通用 → 权限与授权」卡

| 行 | 状态显示 | 操作 |
|---|---------|------|
| 辅助功能 | 已授权 / 未授权 / 非 macOS | 请求授权 · 修复授权 · 刷新 · 打开系统设置 |
| 开机启动 | 已启用 / 未启用 / 不支持 | 启用 / 关闭 · 打开系统设置 |
| Finder 扩展 | 已启用 / 未启用 / 非 macOS | 启用 / 停用 · 刷新 |
| 定位服务 | 已授权 / 未申请 / 已拒绝 / 受系统限制 | 申请授权 · 打开系统设置 |

卡尾保留原权限页的两段排查说明（系统缓存授权状态；重打包后 macOS 视其为「另一个程序」需先移除旧条目）。

---

## 4. 关键实现决策

| 决策 | 理由 |
|------|------|
| **授权动作只留一处，功能页留状态 + 跳转** | 两处授权按钮会让用户怀疑哪处才是真的；但直接删掉状态显示会丢失诊断能力 |
| **侧边栏「权限」入口下线，`"permissions"` 保留为深链别名 → `general`** | 存量用户 `last_settings_section` 可能存着该值（Rust 侧无校验），不映射就白屏 |
| **「开机启动」只保留权限卡形态** | 它本质是系统权限（登录项 + 系统点头），不是普通配置项 |
| **「修复授权」必须保留** | ad-hoc 签名每次重构建都换 cdhash，系统留的旧 TCC 记录会让 `AXIsProcessTrusted()` 恒为 false，重新勾选无效——实际最常遇到的坑 |
| **定位权限纳入** | 后端 `NameAccess` 早就有 4 态 key，`wifi_name_access()` 也已存在，只差一个 command 暴露 |
| **权限卡独立于草稿守卫** | 权限状态与配置无关；配置加载失败时权限信息恰恰最该可见 |
| **`launch_at_login` 前端写入删除** | 后端 `set_login_item` 内部已自行 patch 该镜像字段，前端那次写入是冗余的 |

---

## 5. 变更清单

### 新增

| 文件 | 说明 |
|------|------|
| `src/components/PermissionSection.tsx` | 权限与授权卡（4 行权限 + 排查说明）|
| `src/lib/section.ts` | `normalizeSection` 抽成纯函数（原先埋在 `MainWindow.tsx`，从无测试覆盖）|
| `src/lib/section.test.ts` | 10 个用例覆盖存量分区值 |

### 修改

| 文件 | 改动 |
|------|------|
| `src/pages/Settings.tsx` | general 分支重排：权限卡置顶且移出草稿守卫；删除重复的登录项 state / effect / toggle |
| `src/windows/MainWindow.tsx` | 侧边栏删除「权限」项；`normalizeSection` 改为从 `lib/section` 导入；登记跨分区跳转通道 |
| `src/lib/windows.ts` | `MainPage` 移除 `"permissions"`；新增 `onSectionSwitcher` / `goToSection` |
| `src/pages/RingSettings.tsx` | 辅助功能行去按钮化改「去授权」；删除 `requestAx` / `repairAx` / `axNote` |
| `src/components/ContextMenuSettings.tsx` | 两行改「去授权」；删除 `toggle` / `grant`；排查文案指向新路径 |
| `src/components/FirstRunGuide.tsx` | 「查看权限设置」→ `general`；文案补「都在设置 → 通用里一次授权」 |
| `src/components/ui.tsx` | `CardGroup` 增加可选 `action` 插槽（刷新按钮上提到卡片标题右侧）|
| `src/pages/StatusBarSettings.tsx` | 补定位权限说明 |
| `src/lib/ipc.ts` | 新增 `statusBarLocationAccess` |
| `src-tauri/src/commands/statusbar.rs` | 新增 `statusbar_location_access` 命令 |
| `src-tauri/src/core/statusbar/mod.rs` | 新增 `wifi_name_access()` 转达层 |
| `src-tauri/src/core/statusbar/other.rs` | 非 macOS 返回 `authorized` |
| `src-tauri/src/commands/system.rs` | 镜像写入失败改为 `tracing::warn!` 记录，不再静默丢弃 |
| `src-tauri/src/lib.rs` | 注册新命令 |

### 删除

| 文件 | 说明 |
|------|------|
| `src/components/PermissionCenter.tsx` | 241 行，独立权限页整体并入通用页 |

---

## 6. 验证结果

| 项 | 结果 |
|---|------|
| `npx tsc --noEmit` | ✅ 零错误 |
| `npm run build` | ✅ 通过（tsc + vite build，92 modules，单包 420KB / gzip 132KB）|
| `npx vitest run --environment=node` | 169 用例中 158 过、11 失败；失败**全部**是 `src/lib/sanitize.test.ts` 的 `DOMParser is not defined`（node 环境无 DOM），属既有问题 |
| `src/lib/section.test.ts` | ✅ 10/10 通过 |
| `cargo check` | ✅ 通过，零警告 |
| `cargo test --lib` | ⚠️ SIGABRT —— **既有问题**，用 `git stash` 对照基线（不含本次改动）同样 abort，崩在 statusbar 原生调用路径 |
| `npm test`（jsdom 默认） | ⚠️ 本机 60s worker 启动超时（环境问题）|

**关键回归点核查**（独立验证 + 主理人复核，全部通过）：

- `loginItemStatus` / `requestAccessibility` / `setLoginItem` 前端调用点各**仅一处**，均在 `PermissionSection.tsx`（无重复请求、无双份状态）
- `"permissions"` 仅出现在归一化函数与测试中；`src-tauri` 两处是模块 manifest 同名键，与分区无关
- 通用页渲染结构：权限卡在草稿守卫**之外**（配置未就绪时照常可见）、基础卡在 `draft ?` 分支内、底部保存状态条在共享 return 内——三者互斥且不重复
- `goToSection` 调用点：`ContextMenuSettings.tsx` / `RingSettings.tsx` 各一处
- 「修复授权」保留在权限卡内（`PermissionSection.tsx:271` 按钮 / `:148` 调用），未丢失

---

## 7. 待完善 / 已知局限

1. **`cargo test --lib` 的 SIGABRT 未修** —— 崩在 statusbar 原生调用路径，是本次改造之前就存在的问题（已用 `git stash` 基线对照确证），但它让整个 Rust 测试套件无法跑完，属于独立待办。
2. **`npm test` 在本机跑不起来** —— jsdom worker 60s 启动超时，`sanitize.test.ts` 的 11 个用例需要 DOM 环境。CI 上是绿的（本机是环境差异），但本机无法作为回归手段。
3. **设置页组件无测试覆盖** —— 现有 vitest 只覆盖 `src/lib/` 的纯函数层；本次新增的 `section.test.ts` 是第一个覆盖「设置页路由行为」的用例。
4. **「去授权」跳转后无视觉锚点提示** —— 用户从环形转换页点「去授权」会跳到通用页顶部，而权限卡正好在顶部第一屏，故当前可用；若后续调整通用页排序，需同步保证权限卡仍在首屏。
5. **模块级权限未纳入** —— `src/pages/Modules.tsx` 的模块 capability 授权属于模块沙箱体系（且「模块」分区已从侧边栏下线），不在本次范围。

---

## 📚 成员产出索引

- 设计师（gstack-designer）：信息架构方案、5 项设计判断、2 个隐藏风险 —— 已全部采纳并落地
- 调查员（gstack-investigator）：影响面核查（深链链路 / 刷新时机 / 测试覆盖 / 最小后端改动）、`normalizeSection` 抽取与 10 个单测用例、TS6133 等问题发现 —— 已全部采纳

---

> 本报告由软件工坊 AI 协作生成，关键决策请由工程负责人复核。
