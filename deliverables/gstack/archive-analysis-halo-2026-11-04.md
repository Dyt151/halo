# Halo 软件档案 · 首次系统性分析

**日期**：2026-11-04
**场景**：软件档案尽调（产品评审 + 安全审计 + 技术健康度）
**参与成员**：调查员（技术健康）· 产品评审员（交付准备度）· 安全卫士（安全与隐私）
**分析对象**：`/Users/dayiteng/Documents/codex/小发明/halo`（Halo，macOS 剪贴板工具，Tauri 2 + Rust + React）
**方法**：三位成员各自独立只读核查源码、配置与版本库，结论回传主理人后统一去重汇编；全程未修改任何文件、未执行构建或项目脚本

---

## 📌 TL;DR（执行摘要）

- 整体结论：🟡 **有条件通过** — 代码是成品，工程底子扎实；但**档案（README + 版本库）不是交付态**，外部读者按现有描述评估必然系统性误判。
- 阻塞项数量：**3 条 🔴**（仓库与版本库断链、"只做三件事"叙事失效、默认明文入库且隐私防线可绕过）。
- 最关键结论：**代码 ≠ 档案 ≠ 版本库** 三者处于三个不同的版本。代码里已落地的东西 README 说没做，README 承诺的文档仓库里没有，README 描述的形态又是几个月前的旧形态。
- 下一步：**先固化基线 + 拍板 statusbar 去留**，其余全部依赖这两件事；在范围收敛之前补 README 是白费力气。

---

## 🎯 核心结论卡片

| 项目 | 内容 |
|------|------|
| Go / No-Go | 🟢 内部自用 Go / 🟡 继续开发 条件 Go / 🔴 **开源发布与对外评估 No-Go** |
| 严重度分布 | 🔴 3 / 🟠 9 / 🟡 2 / 🟢 1 |
| 关键行动项 | 8 条（P0 × 4，P1 × 3，P2 × 1） |
| 建议负责人 | 工程负责人（基线与范围）、产品评审员（定位与文档）、安全官（隐私默认值与注入面） |
| 代码健康度 | 🟢 高 — 367 个 Rust `#[test]` + 9 个 vitest 文件，CI 覆盖 cargo test/clippy/npm test/build |
| 档案可信度 | 🔴 低 — 135 处改动未提交、36 个文件未跟踪、10 份设计文档缺 9 份 |
| 最大的被低估风险 | 模块事件广播绕过权限白名单 + sidecar 无 OS 沙盒 |
| 最大的被高估风险 | "E2EE 未实现" —— 同步侧 E2EE 实际已落地，缺的只是本地字段级加密 |

---

## 1. 各成员核心结论

### 🔧 排障手（技术健康度与档案一致性）

- **核心判断**：代码是真实现，不是空壳（367 个 Rust 测试、7 个转换引擎、状态栏模块约 9,231 行）；但文档与版本库严重脱节。关键在交付态——`git status` 显示 135 个已跟踪文件被修改、4 个删除、36 个未跟踪，其中包含整个 `core/statusbar/` 子系统、构建必需的 `native/` 与 `resources/`、`NOTICE-STATUS-TRIO.txt` 合规文件。照当前仓库 clone 交付，会丢掉最大的子系统且大概率构建失败。`.gitignore` 本身是干净的（正确排除 node_modules / dist / release / target）。
- **关键判断**：README "端到端加密尚未落地"这句**判错了一半**。`sync/crypto.rs:12-63` 是完整的 Argon2id + ChaCha20Poly1305 实现，`sync/manager.rs:221-284` 已接线，`Cargo.toml:45-46` 有真依赖，`qa_tests/crypto.rs` 有 9 个用例，`lib.rs:245-249` 同步命令已注册。真正未实现的只有本地策略 `SensitivityPolicy::Encrypt`（`core/sensitivity/mod.rs:131-132` 明写"暂未实现本地加密，本条按脱敏处理"）。
- **关键建议**：先做全量提交并打 tag 建立基线，再从干净 clone 实跑一次 `npm ci && npm run tauri build` 验证构建输入是否补齐；然后以代码为准重写 README 的能力边界与已知限制。

### 🔍 产品官（产品定位与交付准备度）

- **核心判断**：工程完成度高、档案可信度低——"代码是成品，仓库是没收尾的工作台"。最反直觉的一条：**别先补 README**。statusbar 还在按 phase 2/3 继续加功能，文档写完就过期；真正的阻塞不是"文档缺了"，而是"**范围还没收敛**"。
- **关键判断**："只做三件事"的克制叙事已失效。README 把同步/模块/菜单栏降级为"支撑设施"，唯独对 statusbar 一字未提；而 `core/statusbar/` 约 8,000–9,200 行、`TrayPanel.tsx` 约 1,400–2,200 行、`lib.rs:279-299` 注册 20 个命令、`tauri.conf.json` 实际 5 个窗口（多出 traypanel / imehud），且 `config/settings.rs:452` **默认 enabled:true**。这不是遗留或实验分支，是一条默认开启的第二产品线（菜单栏控制中心）。
- **关键建议**：① 提交打 tag 建基线，并**拍板 statusbar 是并入还是拆成独立产品**（这一条决定后面所有文案）；② README 四处修正（文档清单、双许可+NOTICE、E2EE 措辞、能力边界）；③ 补 1 页定位卡——460 行 README 没有一句目标用户或竞品论证，环形转换是唯一的差异化楔子却没立起来；④ 发布链路二选一：补 updater/pubkey，或从 `release.sh` 撤下。

### 🛡️ 安全卫士（OWASP + STRIDE 首次风险扫描）

- **核心判断**：整体 🟠 中高。**不应**按"远程泄露"定 🔴——同步默认关闭、无云端账户、无入站监听、单机本地优先是真实前提。但"全局捕获 + 明文 SQLite"让 Halo 成为本机上最集中的凭证库。
- **关键判断**：最关键的一条是 `config/settings.rs` 默认敏感策略为 `flag`（入库并标记），意味着**命中的密码/token 原样明文写进 `halo.db`**，而设置页**根本没有"加密后入库"这一档**（`src/pages/Settings.tsx:474-484` 只有 flag/ignore/mask）。README 描述的三档策略，用户在 UI 里选不到第三档，也没有任何降级提示。
- **关键建议**：默认策略改为 Ignore/Mask；`~/.halo` 目录收紧 0700、库文件 0600；修 `terminal_cd_script` 的 shell 层注入与 Finder 队列 `created_at=0` 永不过期；模块事件广播前校验 `clipboard:read`。

---

## 2. 综合审查发现（去重合并后按严重度排序）

| # | 严重度 | 类别 | 位置 | 问题描述 | 建议 | 来源成员 |
|---|--------|------|------|---------|------|---------|
| 1 | 🔴 | 档案完整性 | `git status` / HEAD `c531b31` | 版本库与档案断链：135 改 / 4 删 / 36 未跟踪；未跟踪项含整个 `core/statusbar/`(15 .rs)、`commands/statusbar.rs`、`src/windows/TrayPanel.tsx`、`src-tauri/native/`、`resources/`、`public/`、`NOTICE-STATUS-TRIO.txt`。HEAD 仍是 10-31 的"ClipBase"改名期 | 全量提交 + 打 tag 建基线，核对提交清单 | 排障手 + 产品官 |
| 2 | 🔴 | 产品范围 | `core/statusbar/`、`config/settings.rs:452`、`tauri.conf.json:77-119` | "只做三件事"叙事失效且默认开启：statusbar 约 9,200 行、20 个命令、5 个窗口、`enabled:true` 默认值、`future.rs` 仍排 phase2/3 | 拍板 statusbar 去留；补写第四件事或拆为独立产品 | 产品官 + 排障手 |
| 3 | 🔴 | 安全 | `config/settings.rs:17,763,781`、`core/sensitivity/mod.rs:41-135`、`state.rs:55-125`、`storage/db.rs:25` | 默认明文入库，三道隐私防线均可绕过：黑名单默认为空且按 App 名子串匹配；敏感检测仅限 ≤400 字节文本，Image/Files 一律 clean；未命中与被 flag 内容都明文落盘；`create_dir_all` 未收紧目录权限 | 默认策略 flag→Ignore/Mask；内置黑名单改按 bundle id；目录 0700 / 库文件 0600 | 安全卫士 |
| 4 | 🟠 | 安全 | `src/pages/Settings.tsx:474-484`、`src/components/SyncSettings.tsx:214-231`、`README.md:27,85` | 声称与实现不符："加密后入库"在 UI 不存在，后端 Encrypt→mask 降级只对外部写入配置生效且无任何告知；UI 宣称"验证码识别后一律不上云"但检测器无 OTP 规则 | UI 明示降级或移除该档；README/UI/代码三方对齐；补 OTP 规则 | 安全卫士 |
| 5 | 🟠 | 安全 | `core/services/mod.rs:204-220`、`:306-312`、`services/macos.rs:105-113` | AppleScript 转义只覆盖脚本层，shell 层仍可注入：仅转义 `\` 与 `"`，测试明确断言单引号不转义；目录名含 `'` `$(...)` `;` 时点"在终端中打开"即以主进程权限执行任意命令 | 路径做 shell 单引号转义，或改参数化调用 | 安全卫士 |
| 6 | 🟠 | 安全 | `modules/host.rs:301-310,460-479`、`modules/capabilities.rs:76-115`、`state.rs:117`、`commands/modules.rs:50-59` | 权限白名单是软门控：sidecar 直接 spawn 模块包内可执行文件且无 OS 沙盒；权限 gate 只管 `invoke_method`；`state.rs:117` 把完整剪贴板条目广播给所有运行中模块 | 广播前校验 `clipboard:read`；sidecar 加签名校验与沙盒；安装二次确认 | 安全卫士 |
| 7 | 🟠 | 安全 | `sync/manager.rs:265-285,287-319` | 同步 E2E 的 salt 只存本机 `sync_state` 表，多端必然解不开：B 机拉 A 机快照 → salt 不同 → AEAD 校验失败并误报"口令不一致"（单设备往返正常，故缺陷隐蔽） | salt 随密文头部明文传输，或由口令确定性派生 | 安全卫士 |
| 8 | 🟠 | 可复现构建 | `src-tauri/build.rs`、`tauri.conf.json:134-136`、`finder-extension/build.sh:20-49` | 干净 clone 缺构建输入：工作树 `build.rs` 编译 `native/powerui/*.m` 但 `git ls-files src-tauri/native` = 0；`resources/zh-Hans.lproj/InfoPlist.strings`、`public/VolumeFeedback.wav` 未跟踪；HEAD 的 build.rs 仅 `tauri_build::build()`。CI 不跑 `tauri build` | 补齐并提交构建输入，从干净 clone 实跑一次完整构建 | 排障手 |
| 9 | 🟠 | 文档一致性 | `README.md:442-455`、`README.md:393`、`engines/mod.rs:5` | 文档清单是僵尸引用：README 列 10 份，`git ls-tree HEAD docs/` 只有 `UI-REVIEW-2026-10-29.md`；代码注释与 README 悬空引用 `docs/module-spec.md`、`docs/REA-Filee-Conversion.md` | 补齐或从 README 移除并标注失效；README 定为唯一事实源 | 产品官 + 排障手 |
| 10 | 🟠 | 文档一致性 | `README.md:27`、`README.md:394` | E2EE 表述自相矛盾：README 同时说"只在设计文档定义"与"支持加密同步"，而代码已实现同步侧 E2EE 原语与接线 | 拆成"同步 E2EE 已落地 / 本地敏感加密未落地"两句重写 | 排障手 + 产品官 |
| 11 | 🟠 | 许可证 | `README.md:459`、`NOTICE-STATUS-TRIO.txt`、`LICENSE-STATUS-TRIO.txt` | Apache-2.0 归属未披露：README 只写 MIT，未提状态栏面板设计与菜单栏图标几何/配色/字体常量派生自 lingyired/status-trio；而派生部分恰是 README 未提的 statusbar；两份合规文件还未纳入 git | README 增双许可与第三方归属段，NOTICE 随分发入库 | 产品官 + 排障手 + 安全卫士 |
| 12 | 🟠 | 交付准备度 | `tauri.conf.json`、`scripts/release.sh`、`release/`（空） | 发布意图 > 交付：release.sh 已备签名更新流程，但 `tauri.conf.json` 仅 `signingIdentity:"-"`、无 updater/pubkey，`release/` 空目录，README 不提安装方式 | 发布链路接通或从 release.sh 撤下 | 产品官 |
| 13 | 🟠 | 产品定位 | `README.md`（全文）、`docs/PRD.md`（不存在） | 定位与差异化零论证：460 行 README 无一句目标用户/竞品；Maccy/Paste 仅作技术参照，Raycast 未提，iCopy/Filee 只存在于缺失文档名与 `modules.example/filee/`；环形转换是唯一 wedge 却未立起来 | 补 1 页定位卡：环形转换作楔子，剪贴板作地基 | 产品官 |
| 14 | 🟡 | 输入/配置完整性 | `core/services/bridge.rs:387-396,408-421`、`commands/config.rs:15-27`、`capabilities/default.json:6-15`、`commands/sync.rs:111`、`secrets.rs:30-44` | 次级缺口：`created_at=0` 永不过期；队列目录含用户可写路径；`patch_config` 无字段白名单（前端可静默改同步 URL、清黑名单）；允许 `http://`（Basic Auth 与快照裸奔）；`security -w` 口令进 argv 可被 `ps` 读取 | 0 视为过期、队列校验来源、patch 白名单、强制 https、钥匙串改 FFI | 安全卫士 |
| 15 | 🟡 | 供应链 | `core/statusbar/macos/powerui.rs:76-131`、CI 配置 | PowerUI 私有框架桥写系统充电上限、恢复值存普通 JSON；CI 无依赖审计步骤，依赖版本区间宽泛 | 评估上架与系统升级风险；CI 增加依赖审计 | 安全卫士 |
| 16 | 🟢 | 工程健康度 | `src-tauri/src/qa_tests/`、`.github/workflows/ci.yml`、`index.css:623-625`、`ui.tsx:140`、`HistoryPage.tsx:275-279`、`lib.rs:6` | 正面为主：367 个 Rust `#[test]`（qa_tests 137 个 / 13 文件）+ 9 个 vitest，CI 覆盖 cargo test/clippy/npm test/build；UI-REVIEW 的 L1、U3、R1 已修复但文档未回填状态。遗留：U2 roving tabindex 仍缺（`ClipItem.tsx:202` 全卡片 `tabIndex={0}`）；`lib.rs:6` 全局 `#![allow(dead_code)]` 掩盖死代码 | 补 U2、去掉全局 allow、回填 UI-REVIEW 修复状态 | 排障手 |

> 注：第 1、2、4、7、9、10、11、13 条为多位成员交叉发现，已合并去重并统一严重度；第 3、5、6 条为安全卫士独家确证发现。

---

## 3. 威胁建模（STRIDE）+ OWASP Top 10 检查表

**前提校准**：同步默认关闭、无云端账户、无入站监听、单���本地优先 → "明文落盘"不构成远程批量泄露，不应一律判 🔴。真正的信任边界是**同用户本机**：任何能在该用户下运行的进程（已启用模块、被诱导的目录名、恶意 Finder 队列）本就能读剪贴板与钥匙串，Halo 只是把它们集中成一个高价值文件并放大后果。

### STRIDE 威胁表

| STRIDE | 攻击面 | 现状判定 | 对应发现 |
|---------|--------|---------|---------|
| Spoofing 欺骗 | 权限状态自检 / 能力宣称 | 🟠 UI 与 README 声称的"加密后入库"档不存在，降级无告知 | #4 |
| Tampering 篡改 | 多端同步 E2EE salt 分发 | 🟠 salt 仅存本机，多端必然解密失败且误报口令错误 | #7 |
| Repudiation 抵赖 | 转换/回贴审计日志 | 🟢 未列为本次关注点，无异常证据 | — |
| Information Disclosure 信息泄露 | 剪贴板全局捕获 + 明文 SQLite | 🔴 默认 flag 策略 + 黑名单可绕 + 目录权限未收紧 | #3 |
| Elevation of Privilege 权限提升 | AppleScript 执行 / 模块 sidecar | 🟠 shell 层注入 + 广播绕过权限白名单 + sidecar 无沙盒 | #5、#6 |
| Denial of Service 拒绝服务 | 三层限流 / 队列堆积 | 🟢 已有限流与过期丢弃，未发现确证缺口 | — |

### OWASP Top 10 映射

| OWASP | 判定 | 依据 |
|-------|------|------|
| A01 失效访问控制 | 🟠 模块能力模型为软门控，广播路径未覆盖 | #6 |
| A02 加密失效 | 🔴 默认明文入库；本地字段级加密未实现；多端 E2E salt 分发缺陷 | #3、#4、#7 |
| A03 注入 | 🟠 AppleScript → shell 边界转义不完整 | #5 |
| A04 不安全设计 | 🟠 能力边界未收敛（第二产品线默认开启） | #2 |
| A05 安全配置错误 | 🟡 `created_at=0` 永不过期、`patch_config` 无白名单、允许 http://、口令入 argv | #14 |
| A06 组件与依赖漏洞 | 🟡 无依赖审计步骤、PowerUI 私有框架 | #15 |
| A07 身份认证失效 | 🟢 同步 E2E 已实装 Argon2id + ChaCha20Poly1305（问题在 salt 分发而非算法） | #7 |
| A08 数据完整性失效 | 🟠 转换输出已做"先预留再原子提交"（正面），但归档链未审计 | — |
| A09 日志监控失效 | 🟡 手势层可观测性设计良好（正面），但无依赖/安全审计日志 | #15 |
| A10 SSRF | 🟡 WebDAV 目标地址由 `patch_config` 无白名单修改 | #14 |

---

## ✅ 行动清单

| # | 行动 | 负责方 | 紧急度 | 期望完成 |
|---|------|--------|--------|---------|
| 1 | 全量提交 36 个未跟踪文件与 135 处改动，`git tag` 建基线，并核对提交清单包含 `native/`、`resources/`、`public/`、NOTICE/LICENSE-STATUS-TRIO | 工程负责人 | P0 | 0.5 天 |
| 2 | **拍板 statusbar 去留**（并入 Halo 重写定位 / 拆为独立产品），同步重写 README 的"只做三件事"与能力边界 | 产品评审员 + 工程负责人 | P0 | 与 #1 同期决策 |
| 3 | 默认敏感策略 flag → Ignore/Mask；UI 明示"加密后入库"未交付；`~/.halo` 收紧 0700 / `halo.db` 0600 | 工程负责人 | P0 | 1 天 |
| 4 | 修 `terminal_cd_script` shell 层单引号转义；Finder 队列 `created_at=0` 视为过期；模块广播前校验 `clipboard:read` | 工程负责人 | P0 | 1 天 |
| 5 | 同步 E2E salt 随密文头部传输或由口令确定性派生；WebDAV 强制 https；`patch_config` 加字段白名单；钥匙串改 FFI 传参 | 工程负责人 | P1 | 2 天 |
| 6 | README 四处修正：文档清单、双许可 + NOTICE 披露、E2EE 措辞拆分、能力边界；补 1 页定位卡（目标用户 / 竞品 / 环形转换作楔子） | 产品评审员 | P1 | 0.5 天（**须在 #2 之后**） |
| 7 | 补齐或作废 docs/ 缺失的 9 份设计文档；`UI-REVIEW-2026-10-29.md` 加"非全产品验收、仅覆盖 10-29 范围"时效标注并回填已修项 | 产品评审员 + 排障手 | P1 | 1 天 |
| 8 | 从干净 clone 实跑 `npm ci && npm run tauri build` 验证可复现构建；补 U2 roving tabindex、去掉全局 `#![allow(dead_code)]` | 质量门神 + 工程负责人 | P2 | 1 天 |

---

## ⚠️ 待完善 / 已知局限

- **本次为纯静态只读分析**：未执行 `cargo test` / `vitest` / `npm run build` / `tauri build`，因此"测试通过率""干净 clone 能否构建成功"均为**代码路径判定，非实机复现**。成员结论中标注"待确认"的部分以此为准。
- **安全发现中的注入与 salt 缺陷未做实机验证**：`terminal_cd_script` 注入、同步 salt 多端失效均为代码路径确定，但未构造 PoC 复现。
- **未覆盖维度**：QA 测试策略与发布流程（`qa-lead`）、视觉与设计系统（`designer`）本次未上场；`docs/UI-REVIEW-2026-10-29.md` 已覆盖设计侧的多数结论，本次仅由排障手抽查其修复状态。
- **版本库状态随时变动**：分析基于 2026-11-04 的工作树快照（135 改 / 4 删 / 36 未跟踪）。若用户在此期间有提交，结论 #1 需要重新核对。
- **PowerUI 私有框架在上架环境的可用性未确认**（hardened runtime 下是否可用、是否符合 App Store 政策）——需真机验证。
- **根目录未见 Cargo.lock**（实际位于 `src-tauri/Cargo.lock` 且处于已修改未提交状态），依赖可复现性未做独立验证。

---

## 📚 成员产出索引

- **gstack-investigator（排障手）** — 技术健康度与档案一致性核查，7 条发现：交付态断链 / E2EE 判错一半 / 能力边界低估 / 可复现构建缺输入 / 文档缺失 / 许可证归属 / 工程健康度。关键证据：`git status`、`sync/crypto.rs:12-63`、`sync/manager.rs:221-284`、`core/sensitivity/mod.rs:131-132`、`tauri.conf.json:77-119`。
- **gstack-product-reviewer（产品官）** — 产品定位与交付准备度评审，6 条意见 + 5 条优先级清单。关键证据：`git log`（9 提交 / HEAD `c531b31`）、`config/settings.rs:452`、`lib.rs:279-299,431`、`tauri.conf.json`、`scripts/release.sh`。
- **gstack-security-officer（安全卫士）** — OWASP + STRIDE 首次风险扫描，7 条发现（确证 6 / 待确认 1）。关键证据：`config/settings.rs:17,763,781`、`core/sensitivity/mod.rs:41-135`、`state.rs:55-125,117`、`storage/db.rs:25`、`core/services/mod.rs:204-220`、`core/services/bridge.rs:387-421`、`modules/host.rs:301-310`、`modules/capabilities.rs:76-115`、`sync/manager.rs:265-319`、`commands/config.rs:15-27`、`secrets.rs:30-44`。

---

> 本报告由软件工坊 AI 协作生成，关键决策请由工程负责人复核。
