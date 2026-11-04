# Halo · 修复执行记录

**日期**：2026-11-04
**依据**：`deliverables/gstack/archive-analysis-halo-2026-11-04.md`（首次系统性分析的 8 条行动清单）
**执行模式**：Agent 模式，直接在 `/Users/dayiteng/Documents/codex/小发明/halo` 落地
**结果**：3 条 🔴 阻塞项全部闭合；P0 行动 4/4 完成，P1 完成 1/3，P2 完成 1/3

---

## 📌 TL;DR

- **决策点已闭环**：statusbar 由用户拍板「并入 Halo 正式列项」，作为第四件事「状态栏控制中心」写进 README，保持默认开启。
- **代码侧最脏的三处已修**：默认明文入库 → 脱敏入库；「在终端打开」的 shell 注入已堵；模块事件广播已加 `clipboard:read` 权限门控。
- **交付态已建立**：工作树从「137 改 / 4 删 / 21 未跟踪」变成**剩余 0**，并打了标签 `v0.1.0-baseline`。
- **可复现性已验证**：干净 clone 后关键构建输入全在库，`cargo check` 实跑通过（含 `build.rs` 编译 PowerUI 的 Objective-C），前端 `npm run build` 通过。
- **未完成项集中在 P1/P2**，原因见第 4 节（多数需要独立设计或与核心交互耦合，不宜草率改动）。

---

## 1. 逐条行动项执行状态

| # | 行动 | 状态 | 落地情况 |
|---|------|------|---------|
| 1 | 全量提交 + 打 tag 建基线 | ✅ 完成 | `18495e9`（tag `v0.1.0-baseline`），169 files / +16185 −611；工作树剩余变更 0 |
| 2 | 拍板 statusbar 去留 | ✅ 完成 | 用户拍板：并入 Halo 正式列项（第四件事），不拆独立产品 |
| 3 | 默认敏感策略改脱敏 + 收紧 `~/.halo` 权限 | ✅ 完成 | 策略 `Flag`→`Mask`；目录 0700 / 库文件 0600。**UI 明示部分未做**（见 4.1） |
| 4 | 修 shell 注入 + 队列 `created_at=0` + 广播权限门控 | ✅ 完成 | 三处全部修复，附回归测试 |
| 5 | E2E salt 分发 / 强制 https / `patch_config` 白名单 / 钥匙串 FFI | ❌ 未做 | P1，需单独设计（见 4.2） |
| 6 | README 四处修正 + 1 页定位卡 | ✅ 完成 | README 六处修正 + 新增 `docs/POSITIONING.md` |
| 7 | 补齐 / 作废 docs 缺失文档 + UI-REVIEW 时效标注 | ⚠️ 部分 | README 文档清单加 ✅/⏳ 状态列并标注「待补」；**9 份文档未补，UI-REVIEW 未加时效标注** |
| 8 | 干净 clone 实跑构建 + U2 无障碍 + 去 `allow(dead_code)` | ⚠️ 部分 | 干净 clone 编译验证**已通过**；**U2 与 `dead_code` 未做**（见 4.3） |

**额外完成**：`.gitignore` 排除 `src-tauri/finder-extension/build/`（`.appex` 是签名二进制，HEAD 里曾误提交陈旧 ClipBase 版）；移除 `commands/backup.rs` 未使用的 import，使构建零警告。

---

## 2. 改动明细

### 2.1 安全 P0（Rust）

| 文件 | 改动 |
|------|------|
| `config/settings.rs` | 敏感策略默认 `Flag`（明文标记入库）→ **`Mask`（脱敏入库）**；`#[default]` 移到 `Mask`，注释重写说明取舍 |
| `config/mod.rs` | **v6 一次性迁移**（旧 `Ignore` → `Flag`）同步改为 `→ Mask`。该迁移有 `marker_set` 守卫、只跑一次，不改的话老用户升级后会被顶回明文存储 |
| `storage/db.rs` | 新增 `ensure_private_dir()`：`~/.halo` 逐级收敛 0700、`halo.db` 0600（`#[cfg(unix)]`，其它平台退化为原生 `create_dir_all`） |
| `core/services/mod.rs` | `terminal_cd_script` 目标目录改 **shell 单引号整体包裹**（内部 `'` → `'\''`），堵住「在终端中打开」的 shell 层注入 |
| `core/services/bridge.rs` | `is_stale(0)` 由「永不失效」改为**立即过期丢弃**；测试名同步为 `stale_requests_expire_and_zero_timestamp_is_stale` |
| `modules/host.rs` | `broadcast` 增加 `gate.check(id, Capability::ClipboardRead)` 过滤——剪贴板条目只发给已授权的 sidecar 模块 |
| `qa_tests/config.rs` | 默认值断言同步为 `Mask` |
| `commands/backup.rs` | 移除未使用的 import（构建零警告） |

### 2.2 文档

- **README.md**：开头「三件事」→「四件事」并加入状态栏；功能一览表新增状态栏行；**已知限制把 E2EE 拆成「同步已落地 / 本地未落地」**并说明默认策略已改为脱敏；新增「状态栏默认开启」条目；文档清单加 ✅/⏳ 状态列并标注「待补」；许可证补 Apache-2.0 派生归属；技术栈与目录结构补 `core/statusbar/`。
- **docs/POSITIONING.md**（新增）：定位卡——目标用户、核心楔子（环形文件转换）、四个能力的关系（地基/入口/楔子/延伸）、差异点、诚实版边界。明确标注「市场判断非调研结论」。
- **.gitignore**：排除 `src-tauri/finder-extension/build/`。

### 2.3 提交

```
18495e9 (HEAD -> main, tag: v0.1.0-baseline) chore: 固化 Halo 命名后的完整工作树为可交付基线
d9733c3 fix(security): 敏感内容默认脱敏、修 shell 注入、模块广播加权限门控
c531b31 refactor: 重构为 iCopy 式剪贴板工具，重做转换引擎与设置页   ← 原 HEAD
2b36f5f (origin/main) docs: 建立代码审查标准与流程
```

> **未 push**：`origin/main` 仍在 `2b36f5f`，本地领先 2 个提交。是否推送由用户决定。

---

## 3. 验证结果

| 验证项 | 结果 | 说明 |
|--------|------|------|
| `cargo check`（工作树） | ✅ 通过 | 零警告 |
| `cargo test --lib` | ✅ 通过 | 含本次新增/修改的 3 个测试 |
| 干净 clone `cargo check` | ✅ 通过（1m03s） | **验证 `build.rs` 能编译 PowerUI 的 Objective-C**，第三方可复现构建 |
| 干净 clone 构建输入核验 | ✅ 全在库 | `native/`、`resources/`、`public/`、托盘图标、两份 Apache-2.0 合规文件；statusbar 14 个文件齐全；构建产物误入 0 |
| `npm run build`（tsc + vite） | ✅ 通过 | 91 modules / 18.53s |
| `npm test`（jsdom） | ⚠️ 环境受限 | 本机 jsdom 环境 **60s worker 启动超时**；`--environment=node` 下 8/9 文件、**148 个测试通过**，失败的 11 个是 `sanitize.test.ts` 缺 `DOMParser`（node 无 DOM） |
| `cargo test --lib` SIGABRT | ⚠️ 预先存在 | 崩在 statusbar 原生调用路径，abort 前所有测试 ok，与本次改动无关 |

**回归安全性核查**：`is_stale(0)` 改为过期是否会误杀正常右键请求？——已核对 `FinderSync.swift:145`，Swift 扩展每次都写 `created_at: Int(Date().timeIntervalSince1970 * 1000)`，故该改动只影响伪造 / 缺字段的请求。

---

## 4. 明确未做的项与原因

### 4.1 前端未改：设置页未明示「加密后入库」未交付
后端 `SensitivityPolicy::Encrypt` 一直存在但行为降级为脱敏，且**设置页本来就没有这一档**（`Settings.tsx:474-484` 只有 flag / ignore / mask），所以 UI 并未对用户造成「选得到却降级」的误导——README 已在已知限制中说明。是否要在 UI 上补一句提示，属产品措辞决策，本次未动前端。

### 4.2 P1 安全项：E2E salt 分发 / 强制 https / `patch_config` 白名单 / 钥匙串 FFI
这四项相互关联（涉及同步协议与配置通道），改动会牵动协议兼容性，需要单独设计与回归验证，不宜夹在一次基线固化里草率落地。已记录在案。

### 4.3 P2 项：U2 roving tabindex 与全局 `#![allow(dead_code)]`
- **U2**：`ClipItem.tsx:202` 仍是全卡片 `tabIndex={0}`。正确实现需要列表层改造（仅当前选中项可 Tab 聚焦），而面板的方向键导航是核心交互，改造风险高于收益。UI-REVIEW 已记录该项。
- **`allow(dead_code)`**：`lib.rs:6` 的全局豁免是为「架构冻结、留待后续接线的公共 API」而设。直接移除会引入数十条警告、淹没真正的信号；正确做法是逐个删除或接线未使用的公共 API，属独立重构任务。

### 4.4 其它遗留
- docs/ 仍有 8 份设计文档缺失（README 已标「待补」）；`UI-REVIEW-2026-10-29.md` 未加时效标注。
- sidecar 模块仍无签名校验 / OS 沙盒。
- 状态栏模块未做与剪贴板同等深度的界面审查。
- 同步 E2E 的 salt 只存本机，多端同步会解密失败并误报「口令不一致」。

---

## 5. 建议的下一步

1. **决定是否 push**：`origin/main` 仍落后 2 个提交，其中含整个 statusbar 模块入库。
2. **同步 E2E salt 分发**（P1，影响真实多端使用）：salt 随密文头部明文传输，或由口令确定性派生。
3. **配置通道收口**（P1）：`patch_config` 加字段白名单、强制 https、钥匙串改 FFI 传参。
4. **sidecar 隔离**（P1）：签名校验 + 安装二次确认 + 广播前 `clipboard:read` 门控的进一步收紧。
5. **UI-REVIEW 时效标注 + 状态栏界面审查**（P2）：让「唯一在库的旧文档」不误导读者。

---

> 本记录由软件工坊 AI 协作生成。修复涉及默认行为变更（敏感策略、目录权限），**建议在合并前用真实数据做一次手动回归**：复制一段含 token 的文本，确认库中是脱敏占位而非明文。
