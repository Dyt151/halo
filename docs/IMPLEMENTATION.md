# ClipBase 实现说明（IMPLEMENTATION）

> 记录实际完成情况、与架构设计的差异点、如何运行、已知问题。

## 1. 完成情况总览

按架构任务 T01→T05 顺序，全部核心代码已落地，Rust 侧 `cargo check` / `cargo build` 与前端 `npm run build`（tsc + vite）均通过。

| 任务 | 内容 | 状态 |
| --- | --- | --- |
| T01 | 项目脚手架 + 基础设施（Tauri 2 / Vite / React / SQLite 迁移 / 配置 / 错误约定） | ✅ |
| T02 | 剪贴板核心（objc2 轮询监控 / 历史+FTS5 / 搜索 / 回贴 / 全局快捷键）+ 前端主窗口 | ✅ |
| T03 | 扩展框架（ModuleHost / MessageBus / module.json / 权限门控 / sidecar+iframe / 贡献点） | ✅ |
| T04 | 同步层（SyncProvider trait / WebDAV / E2E 加密 / 开关与定时） | ✅ |
| T05 | 托盘 + 设置 + 集成打磨 + 示例模块 | ✅ |

## 2. 文件清单（关键）

> 完整目录树见 `docs/ARCHITECTURE.md` §7（已按实际代码校对）。这里只列最容易找错入口的几处。

```
src-tauri/src/
├── main.rs / lib.rs            # 进程入口 / Tauri Builder（命令注册）
├── state.rs                    # AppState + 剪贴板事件处理管线
├── window.rs / menu.rs        # 三个窗口的定位与层级 / 应用菜单（无菜单栏图标）
├── config/{mod,settings}.rs    # 配置服务；RingConfig / ConversionConfig / migrate()
├── core/
│   ├── clipboard/ history/ hotkey/ paste/ sensitivity/ stack/ reply/
│   ├── convert/                # 格式转换：format/preset/edge/engines/router/output/job
│   └── ring/                   # Finder 环形转换会话状态机
├── modules/ sync/ qa_tests/
└── commands/                   # clipboard/config/convert/ring/reply/stack/services/sync/modules/system

src/                            # 前端 React
├── App.tsx / main.tsx / index.css
├── lib/{ipc,types,events}.ts + ringGeometry/dialog/toast/filters/format/sanitize
├── store/{useClipStore,useReplyStore}.ts
├── windows/{Panel,MainWindow,ReplyPicker,RingWindow}.tsx
├── pages/{HistoryPage,RingSettings,Settings}.tsx
└── components/                 # ui / RingEditor / ClipItem / DialogHost / …
```

## 3. 窗口模型（面板 / 设置中心 / 回复选择器 / 环形盘）

ClipBase 有四个独立 webview 窗口，职责严格分离：

| 窗口 label | 角色 | 唤出方式 | 关闭行为 |
| --- | --- | --- | --- |
| `panel` | 剪贴板面板（贴底悬浮横条），只有工具栏 / 标签行 / 卡片流 / 状态栏 | 全局快捷键（默认 `Control+C`） | 失焦即隐藏；关闭请求改为隐藏 |
| `main` | **设置中心**（通用 / 剪贴板 / 环形转换 / 右键菜单 / 同步 / 权限 / 关于；快捷键、快捷回复、模块三个分区已下线，见 §13），系统设置式侧边栏外壳；高级历史管理从面板进入 | 面板右上角齿轮（⌘, 也能从 App 菜单进） | 关闭时隐藏 |
| `picker` | 快捷回复选择器（默认 `Control+Shift+R`）：搜索 / 分组 / ↑↓ / Enter 回贴 / Esc 关闭 | 全局快捷键 | 选定或 Esc 后隐藏 |
| `ring` | Finder 环形转换盘：**唯一手势是 Option＋左键拖动**，盘钉在起手点、不抢焦点；只有一面、只有可用扇区可提交 | 上述手势（辅助功能权限） | 松手后失去悬停即收起；选扇区后后台执行并回焦 Finder |

**为什么必须拆开**：早期两者共用一个 `main` 窗口，快捷键只是把它压成底部条，导致面板里残留导航与设置内容，观感与 iCopy 等原生工具完全不同。拆分后面板是纯粹的卡片流，管理界面是完整的双栏窗口。后续又把管理窗口收窄为「只做设置」：历史变成面板的二级能力，回复和转换变成现场窗口（选择器 / 环形盘），避免用户在设置里干活。

**跨窗口通信**：四个 webview 各自挂载一份 React，共享同一份后端状态。面板→主窗口的「落到指定页」走两条路：已挂载时用 Tauri 全局事件 `clipbase://navigate` 立即导航；未挂载（或挂载中）时把目标页写进 Rust 侧 `window::PENDING_PAGE`，主窗口挂载时 `consume_pending_page()` 取一次。**不要改回 localStorage**：两个 webview 是否共享同一存储区在 WKWebView 下没有保证，而 AppState 是进程内唯一可信的共享点。

前端按窗口标签分发渲染（`src/App.tsx`）：

```ts
const [kind] = useState(() => ({ panel: isPanelWindow(), ring: isRingWindow(), picker: isPickerWindow() }));
return kind.ring ? <RingWindow /> : kind.picker ? <ReplyPicker /> : kind.panel ? <Panel /> : <MainWindow />;
```

面板高度 / 位置由 `board_height_scale` 与 `board_position`（贴底 / 屏幕中央 / 跟随光标）决定，逻辑在 `src-tauri/src/window.rs::apply_panel_layout`。

界面实拍（`docs/screenshots/`）：

| 文件 | 内容 |
| --- | --- |
| `panel.png` | 面板：贴底横条，搜索 + 类型分段 + 卡片流 + 状态栏 |

> 其余窗口（设置中心 / 回复选择器 / 环形盘）目前没有保留截图：UI 仍在迭代，
> 旧截图文不对版，宁可缺图。要看实际效果请直接运行 `npm run tauri dev`。

## 4. 与架构设计的差异点

0. **UI 设计规范独立成文**：`docs/UI-DESIGN.md` 是苹果风格改造的唯一实现依据（色彩 / 字号 / 圆角 / 材质 / 动效 / 逐页方案 / token 映射表），改造已按该文档落地，前端共享控件集中在 `src/components/ui.tsx`。

1. **keyring 未引入**：架构中「macOS Keychain 主密钥」未落地。当前 E2E 加密由用户口令经 argon2 派生密钥，盐存于 `sync_state` 表；本地 DB 字段级加密未启用，因此敏感内容策略只提供「忽略 / 脱敏」两种真实语义 —— 选 `Encrypt` 会被降级为脱敏并打一条 `debug` 日志（`state.rs::ingest_clip_event`），**绝不会按明文落库**。keyring 可在后续作为「主密钥」存储接入。
2. **同步 LWW 简化为「本地优先 + 整快照覆盖 + 墓碑」**：pull 时先应用远端墓碑（防止别的设备删掉的内容被本机写回），再按 hash 合并远端缺失条目（本地存在则保留本地的时间/置顶/标签），push 时整体覆盖远端；本地显式删除会把 hash 记进 `sync_tombstones`，合并阶段跳过「删除时间晚于条目创建时间」的远端同 hash 条目。真正的版本向量 LWW 留待 P1。
3. **`ModuleHostApi` 未以字面 trait 实现**：为支持 sidecar 的异步 JSON-RPC，`ModuleHost` 以具体 struct 暴露 `async fn invoke_method(...)`（`enable`/`invoke` 为 async）。架构中的 trait 语义已覆盖，签名略有调整。
4. **`MessageBus` 以「前端发射器 + 进程内订阅」实现**：核心事件统一发布给前端；模块侧反向订阅通过 `ModuleHost::broadcast` 向 sidecar 推送 JSON-RPC 通知。未实现完整的模块→总线订阅（留待 sidecar 双向 RPC 完善）。
5. **菜单栏图标已取消（E12）**：ClipBase 是纯后台常驻应用，唯一入口是全局快捷键唤出的面板，设置从面板右上角齿轮进入，退出在「设置 → 关于 → 后台运行」里（或窗口聚焦时 ⌘Q）。原先的托盘菜单（最近 N 条 / 模块区 / 暂停监控 / 清空历史 / 设置 / 退出）连同 `src-tauri/src/tray/mod.rs` 一并删除，`tray::refresh` 的 9 处调用点也已清理；模块清单里的 `tray` 贡献点仍能解析，但不再有渲染位置。
6. **WASM 运行时（L1）为占位**：`wasm.rs` 返回明确「未实现」错误，未引入 `wasmtime`。
7. **模块安装为「本地目录加载」**：后端按目录路径（含 `module.json`）复制到 `~/.clipbase/modules/<id>` 并入库（默认停用）。安装 / 启用 / 停用 / 卸载的界面在「设置 → 模块」，启用时必须逐项授予 manifest 声明过的能力（后端校验授权是其子集）。
8. **schema 常量**：`storage/schema.rs` 中的表名常量当前未在 SQL 中引用（SQL 用字面量），保留为后续复用。
9. **`min_core` 未做运行时版本比较**：安装时仅校验 semver 格式，是否低于当前核心版本未拦截（P0 从简）。
10. **开机启动用 `SMAppService` 真实注册**：`set_login_item` 会 `registerAndReturnError` / `unregisterAndReturnError`，并回读系统状态（0 未注册 / 1 已启用 / 2 待用户批准 / 3 未找到）。配置里的 `launch_at_login` 只是镜像，UI 以系统状态为准。

## 5. 如何运行

```bash
cd /Users/dayiteng/Documents/codex/clipbase
npm install                      # 已安装
npm run tauri dev                # 开发模式（Rust + 前端热更新）
npm run tauri build              # 打包 .app
open src-tauri/target/release/bundle/macos/ClipBase.app
```

首次运行会创建 `~/.clipbase/{data/clipbase.db,data/files,modules,logs}`。

**回贴权限**：macOS 首次回贴需在「系统设置 → 隐私与安全性 → 辅助功能」授权 ClipBase；无权限时自动降级为「仅复制」。

**示例模块**：`modules.example/filee` 演示 sidecar 模块的目录约定（`bin/filee-cli` 已 chmod +x）。
**安装模块**：设置中心 →「模块」→「安装模块…」，选一个含 `module.json` 的目录；装好后在卡片上「启用」，会先让你逐项授予它声明的能力。命令层（`list_modules` / `install_module` / `enable_module` / …）与贡献点同样可直接调用。

### 5.1 打包说明

`bundle.targets` 目前只保留 `["app"]`：本机 macOS 27 上 Tauri 的 DMG 步骤调用 `hdiutil` 已弃用接口，
`bundle_dmg.sh` 里的 `grep '/dev/diskNs'` 会因正则未转义而失败，导致打包整体报错。
`.app` 本身是完好的，需要 DMG 时手动用 `diskutil image create from` 生成，或等 Tauri 修复该脚本。

### 5.2 窗口可见性相关的坑（已修，勿回退）

| 现象 | 原因 | 修复 |
| --- | --- | --- |
| 从终端/脚本拉起时窗口"存在但看不见" | 进程不是「用户点击启动」，AppKit 可能把整个 App 置为 hidden：窗口 `isVisible()` 仍返回 true、也能被 `screencapture -l` 单独截到，但不参与屏幕合成 | `window::focus_main_window` 里显式 `NSApplication.unhide:` + `activateIgnoringOtherApps:` + `show()` |
| 面板被别的剪贴板工具浮窗压住 | Tauri `alwaysOnTop` 的窗口层级低于 iCopy 等工具使用的 `NSStatusWindowLevel`(25) | `show_panel_window` 里把面板 NSWindow 的 level 设为 25 |
| 切到别的桌面后快捷键唤不出面板 | 窗口默认只存在于创建时的 Space | `tauri.conf.json` 两个窗口都设 `visibleOnAllWorkspaces: true` |
| 面板里卡片没撑满高度、下方大片空白 | WebKit 把 `h-full` 百分比高度解析为 auto（父级是 overflow 容器） | 滚动容器本身设为 `flex`，靠 `items-stretch` 撑满，不用百分比 |

## 6. 已知问题 / 后续事项

1. 剪贴板监控为 500ms 轮询（最低 100ms），macOS 图片读写已实现（arboard `get_image`/`set_image` + 内容寻址 PNG 落盘）；P1 未做：Finder 落文件双写（png + file-url）、`public.png` 优先读取、GIF/WebP 动图仅首帧。
2. 敏感内容识别为启发式规则，可能误判/漏判；`Mask` 策略替换为「`[敏感内容已脱敏 <hash 前 8 位>]`」—— 带短摘要而不是同一个固定串，否则按 hash 去重的不同密码会被压成一条（用户会以为「后面的密码没记住」）。`Encrypt` 策略未实现真实字段加密，当前与 `Mask` 同效（设置页已标注）。
3. sidecar 双向 RPC（模块反向调用核心能力）协议已定义但仅支持 `clipboard.read/write`，`fs.*`/`network` 尚未实现具体 handler。
4. 模块自带的界面（`read_module_ui`）尚未内嵌渲染：「模块」页目前只展示清单、权限与贡献点，没有 iframe 取景框。
5. 无模块市场/签名校验（本地加载起步，符合 P0）。
6. `cargo check --all-targets` 当前无警告：为未来保留的公共 API 由 `lib.rs` 顶部的 `#![allow(dead_code)]` 统一压掉，新增无用代码不会再出现在输出里，需要时手动搜。
7. **⌘1~9 不是全局快捷键**：它只在面板唤出时由面板自己处理（`num_hotkey_enabled` 仍然是开关）。全局注册会抢走所有 App 的 ⌘1~9（浏览器切标签、邮件切账号全失效），已有回归测试 `number_keys_are_not_global_hotkeys` 锁死。
8. 同步不传图片二进制：快照只带 JSON，图片条目按「内容寻址的文件名」互通，本机没有同名文件就跳过（换机后图片不会跟过去）。

## 7. 当前验证状态（2026-10-26 第二轮加固后重跑）

| 命令 | 结果 |
| --- | --- |
| `cargo test --lib` | 266 passed / 0 failed |
| `cargo clippy --all-targets` | 0 warning / 0 error |
| `npx tsc --noEmit` | 通过 |
| `npm run build`（tsc + vite build） | 通过，85 modules，产物 361.41 kB JS / 50.21 kB CSS |

> 本节只记录「能不能构建、测试过不过」。功能行为、已知缺口见上方 §4 / §6。

### 7.1 已修回归点（勿回退）

以下每条都曾真实出错，改回去会立刻复发；括号里是配套测试或验收方式。

| 位置 | 曾经的错误 | 现在的做法 |
| --- | --- | --- |
| `core/convert/output.rs` | 同一批次里两个同名源文件（`a/photo.png`、`b/photo.png`）输出到同一目录、策略为 Overwrite 时，第二个会覆盖第一个，静默丢掉一个结果 | 先查 `is_taken` 走重试，再查 `exists()` 走冲突策略；覆盖只针对「磁盘上已有的旧文件」（`overwrite_still_separates_names_inside_one_batch`） |
| `modules/manifest.rs` | `Contribution` 用 `rename_all = "lowercase"`，标签序列化成 `clipboardaction`，前端按 `clipboard_action` 过滤 → 右键菜单里的模块动作永远不出现 | 改为 `rename_all = "snake_case"`（`contribution_tags_round_trip_as_snake_case`） |
| `components/ClipItem.tsx` | 双击回贴会先触发两次「单击复制」，弹出两个 toast，还可能把旧内容贴出去 | 用 `e.detail` 区分单/双击，单击延迟 220ms 并可在双击时取消；底部操作区吞掉 click / dblclick |
| `windows/RingWindow.tsx` | `onWheel` 在 React 的 passive 监听里 `preventDefault()` 无效，翻面会连带滚动页面；`canFlip` 不看当前面 | 原生 `addEventListener("wheel", …, { passive: false })`；`canFlip` 按当前面取动作数 |
| `windows/Panel.tsx` / `components/ClipContextMenu.tsx` | 浮层和面板都在 window 冒泡阶段监听 Esc，按一次 Esc 会叠加关闭两层 | 浮层一律 capture + `preventDefault` + `stopPropagation`；面板用 `closingRef` 防连按重复 `hide` |
| `components/TagBar.tsx` | 选中的标签被删空后仍作为隐形筛选生效，面板一片空白且无处可点 | `stale` 时不再隐藏，额外渲染可点击清除的选中 pill |
| `windows/ReplyPicker.tsx` | 回贴失败被 `.catch` 吞掉（picker 窗口里没有 toast 宿主），用户以为成功 | store 重新抛出，picker 底部显示失败原因（辅助功能未授权时给专有文案） |
| `hooks/useClipActions.ts` | 模块贡献点只在挂载时拉一次，启用模块后右键菜单仍是旧的 | 抽 `load()`，并订阅 `moduleChanged` / `moduleLoaded` / `moduleCrashed` |
| `components/HotkeySettings.tsx` | 主窗口收窄成设置中心时快捷键录制页一起没了：`validate_hotkey` / `apply_hotkeys` 成了没人调的命令，改键只能手改 `config.json` | 新增「设置 → 快捷键」：录制 → 后端校验（保留键/格式/修饰键）→ 落盘 → `apply_hotkeys` 逐槽报告是否被系统接管 |
| `store/useClipStore.ts` | `refresh()` 没有序号保护，先搜 `a` 后搜 `ab` 时旧响应可能后到并盖掉新结果；查询在路上时新入库的条目会被快照抹掉 | 加 `refreshSeq` 只让最后一次落地；`mergeLateArrivals` 把发出后才入库的条目合回列表 |
| `lib/ipc.ts` | Tauri 2 命令参数在 JS 侧是 camelCase，回复相关三处却写了 `group_id`：`add_reply` 直接报缺参（新建回复必失败），`list_replies` 静默忽略分组筛选 | 统一传 `groupId`；分组筛选与管理页、选择器都已验证 |
| `components/ReplySettings.tsx` | 选择器里写着「可在设置 → 快捷回复 中管理」，但那一页从未存在：`add_reply` / `update_reply` / `delete_reply` / `move_reply` / 导入导出全部没有调用方 | 新增「设置 → 快捷回复」：分组增删改、条目增删改与排序、Excel 导入导出（带逐行错误汇总） |
| `store/useReplyStore.ts` | `loadReplies()` 没有序号保护，切分组时旧响应后到会闪回上一组内容；管理页漏选分组会让选择器也只剩那一组 | `loadSeq` 只让最后一次落地；选择器唤出时 `resetFilters()` 恢复「全部 + 无关键词」 |
| `pages/RingSettings.tsx` | 复制预设用毫秒时间戳做 id，同一毫秒连点两次会重名；「复制→插入圆环」两笔 patch 并发，先发响应后到会把后一笔改回去；背景色每敲一个字符就落盘，非法值被后端静默回退成白色 | id 加随机后缀并查重；先 `await` 预设落盘再插圆环；背景色走本地草稿，只提交合法 `#RGB/#RRGGBB`，失焦回退 |
| `lib/ipc.ts` / `lib/errors.ts` | 命令失败时 `invoke` reject 的是 `{ code, message, details }` **对象**（`serde_json::to_value` 后以 application/json 回传），各处 `String(e)` 全是 `[object Object]`，`e instanceof Error` 也是 false —— 状态栏与 toast 显示不出任何原因，`ReplyPicker` 的「辅助功能」分支永远进不去 | `invoke` 外层统一折成 `ApiError`（`message = message：details`，`toString()` 同文案），`toastError` / `errorMessage` 都从这里取；业务代码不再直接 `import { invoke }` |
| `store/useClipStore.ts` | `patch_config` 返回整份配置并整份覆盖 store，两笔并发 patch 响应乱序时，后到的旧快照把后发那笔改回去（「刚拖好的旋钮自己弹回去」） | 模块级 `patchChain` 串行链：前一笔结束（无论成败）才发下一笔 |
| `pages/Settings.tsx` | 自动保存先清空 `pendingRef` 再提交，失败后界面是新值、后端是旧值，下次改别的字段也不会带上这一项（静默丢改动） | 抽 `flush()`：失败把 patch 并回队列并保留 dirty，状态栏出现「重试」 |
| `windows/RingWindow.tsx` | 点选后的 280ms「收缩→提交」定时器与 600ms 结果延迟会在新会话铺开后落地：拿上一批文件的预设去转新文件、用旧结果盖掉新盘面；翻面的 260ms `still` 定时器还会截断新会话的打开动画 | 会话令牌 `sessionTokenRef` + 定时器 ref：`applySnapshot` 自增令牌并清掉 `collapseTimerRef` / `flipTimerRef`，所有延时任务落地前校验令牌 |
| `pages/HistoryPage.tsx` / `components/ClipItem.tsx` | 多选态只拦了 `click`，`dblclick` 是独立事件 → 双击卡片仍会把内容回贴出去；卡片可 Tab 聚焦但多选态的 `onPaste` 是空回调，Enter 成了死键。单击回贴模式下双击会贴两遍（第一下 click 已贴、dblclick 又贴） | 多选态补 `onDoubleClickCapture`，卡片 `onPaste` 改为 `toggleSelect`；`ClipItem` 的 `onDoubleClick` 里 `if (pasteOnSingleClick) return` |
| `components/SyncSettings.tsx` | `dirtyRef` 是 ref 却参与渲染文案（只改密码时不显示「有未保存的改动」）；前端把间隔上限写成 86400 秒，比后端（只有下限 60 秒）更严，既有的大值配置会「怎么改都存不进去」；勾了端到端加密但没口令时不校验，要到第一次同步才在 `sync/crypto.rs` 报错 | `dirty` 改 state + ref 双写（`markDirty`）；删掉上限只保留下限；保存前校验口令并给 `aria-invalid` |
| `windows/MainWindow.tsx` / `lib/windows.ts` | `useState` 初始化里调 `takeSettingsTab()`（消费型），StrictMode 下初始化跑两次，第二次拿到 null → 首启引导的「查看权限」跳不过去 | 新增 `peekSettingsTab()` / `clearSettingsTab()`，初始化只读不消费、清空放到挂载 effect |
| `App.tsx` | 同步会把远端记录 `insert` 进本地库（`sync/manager.rs` pull 分支），但 `sync://completed` 只刷了状态栏 —— 拉下来的条目要等下次筛选才出现 | `onSyncCompleted` 同时 `loadSyncStatus()` + `refresh()` |
| `windows/ReplyPicker.tsx` | 唤出时 `resetFilters()` 已查一次，180ms 防抖又用空关键词查一次 | 关键词与 store 中相同时跳过 |
| `core/clipboard/mod.rs` / `core/paste/mod.rs` | 回贴/复制会写剪贴板 → `changeCount` +1 → 被自己的监控器当成「用户新复制」再入库：条目被顶到最前、多响一次提示音，开着「粘贴为纯文本」时 hash 变了还会多出一条重复记录 | 写入者先 `monitor.mark_self_write(content)` 登记指纹（**必须在 `port.write` 之前**），`poll_once` 读到同 hash 就吞掉这次回声；指纹 3s 过期、只吃一次（`self_write_fingerprint_suppresses_exactly_one_echo`） |
| `core/history/mod.rs` / `sync/manager.rs` | 同步是整快照覆盖，本地手动删掉的条目下次拉取又被写回来（越是清理过的库越会不停「长」回旧条目） | 删除/清空/重做时写 `sync_tombstones`（v5 迁移）；合并阶段**先**应用远端墓碑再合并条目，且跳过「墓碑时间 ≥ 条目创建时间」的同 hash 条目；重新复制同内容会清掉墓碑（`remote_tombstone_*` / `copying_again_clears_the_tombstone`） |
| `sync/manager.rs` | 合并远端条目用 `history.insert`，它把 `created_at` 刷成当天 → 每同步一次，所有重叠条目被顶到列表最前，远端旧条目变成「刚刚」 | 先 `exists_hash` 跳过已存在的，再用 `insert_with_time(item, item.created_at)` 保留原时间（`merge_preserves_remote_created_at`） |
| `sync/manager.rs` | 定时同步用 bool 标记存活；改间隔时「先 `stop()` 再 `start()`」，旧循环此刻正睡着，bool 被置回 true 后它「复活」——两个循环并行、每个周期同步两遍 | 改成自增 `generation` 代号：旧循环醒来发现代号已变就退出 |
| `sync/manager.rs` | 快照里带着敏感条目（默认未加密的 WebDAV 上就是一份明文密码表）与指向本机绝对路径的图片；推上去的 JSON 还泄露用户名 | `portable_item` 过滤敏感与失效图片；`to_portable` 把图片路径压成裸文件名（内容寻址，同名即同图）（`sensitive_items_never_leave_the_device` / `image_items_without_local_file_are_skipped`） |
| `core/hotkey/mod.rs` / `lib.rs` | `num_hotkey_enabled` 默认 true 时把 ⌘1~9 **注册成全局快捷键**，等于抢走所有 App 的 ⌘1~9（浏览器切标签、邮件切账号全失效） | 删除 `HotkeyAction::PasteNth` 与数字注册；改由面板自己处理，角标与快捷键同源（`number_keys_are_not_global_hotkeys`） |
| `windows/Panel.tsx` | ①选中项随 `items.length` 变化被重置，后台复制一下就跳回第一条；②面板唤出即聚焦搜索框，而键盘处理对「搜索框」一刀切透传 → ⌘1~9 与 Enter 在默认焦点下**永远用不了**；③中文输入法用 Enter 上屏候选词会被当成回贴；④搜索防抖（250ms）未落地就按回车，贴的是上一个关键词的结果 | 拆两个 effect（筛选复位 / 长度只 clamp）；搜索框不再算「透传」但方向键与 ⌘⌫ 仍交给输入框；`e.isComposing \|\| keyCode === 229` 直接返回；Enter 先 `flushSearch()` 再取最新列表第一条 |
| `state.rs` | 敏感策略 `Encrypt` 实际按**明文**落库（选项是谎话）；`Mask` 用同一个固定占位串，不同密码被 hash 去重压成一条 | `Encrypt` 降级为脱敏 + `debug` 日志（UI 已标注）；脱敏文案带原文 hash 前 8 位，不同密码落成不同条目 |
| `state.rs` / `commands/clipboard.rs` | 监控关闭时点「捕获当前剪贴板」静默无反应（入库入口先看 `monitor_enabled`） | 拆出 `ingest_clip_event(event, force)`，手动捕获走 `capture_clip_event`（强制入库） |
| `window.rs` | `show_panel_window` 无条件记录前台 pid；若当前前台就是 ClipBase（正在管理窗口里按快捷键），会把自身 pid 记成回贴目标，连上一个真实目标也冲掉 | 改走 `remember_frontmost_app()`（自带自我过滤），托盘「最近 N 条」也复用同一函数 |
| `tray/mod.rs` | 点「最近 N 条」回贴、点模块托盘动作失败时 `let _ = ...` 静默吞错 | 失败统一 `core::services::notify` 系统通知 + `warn` 日志 |
| `src-tauri/src/lock_ext.rs` | 全部用 `.lock().unwrap()`：任何一处 panic 后 Mutex 中毒，数据库连接从此每取必 panic —— App 在重启前彻底不可用，且看不出原因 | 统一走 `LockExt::lock_ok()` / `RwLockExt::read_ok()/write_ok()`（中毒时 `into_inner()` 继续用）；配套测试不依赖它，靠 `cargo test` 的并发用例验证 |

### 7.2 错误文案的唯一入口

命令失败时 `invoke` reject 的是 `{ code, message, details }` 对象。前端**只允许**通过 `src/lib/errors.ts` 归一化：

| 函数 | 用途 |
| --- | --- |
| `toApiError(e)` | 把任意 reject 值折成 `ApiError`（`message = "message：details"`，`toString()` 同文案） |
| `errorMessage(e, fallback?)` | 取一段可以直接展示的文案（日志/状态栏/内联提示） |
| `toastError(e, fallback?)` | 弹 toast，等同 `errorMessage` + 默认文案 |

业务代码里出现 `String(e)` 一定是 bug（得到 `[object Object]`），`e instanceof Error` 也恒为 false。需要区分错误类型时读 `ApiError.code`。

## 8. REA 逆向驱动润色（2026-10-26）

依据 `docs/REA-iCopy-Filee.md` 对 iCopy 的逆向结论，对齐了以下「用起来更顺手」的行为（证据均来自 iCopy 二进制选择器与解码配置，见 REA 报告 §1）：

| iCopy 证据 | ClipBase 落点 | 改动 |
| --- | --- | --- |
| `writeToPasteBoardWithTxtWithoutNewlineAndWhitespace:` | `Config.paste_trim_trailing`（默认开） | 回贴前去除文本尾部空白/换行（`paste::trim_trailing_whitespace`） |
| `confirmationOnDeletion: true` | `Config.confirm_on_delete`（默认开） | 单条删除 / 清空历史前弹确认 |
| `preActiveApp` + `activateWithOptions:` | `window::PRE_ACTIVE_APP_PID` | 唤出面板前捕获前台 App pid，回贴时 `activateWithOptions:` 归还焦点后再注入 ⌘V（修了「⌘V 贴进 ClipBase 自己」的隐患） |
| `pasteboardDiskLimit = 1024` | `default_max_bytes` | 磁盘上限 500MB → 1GB |
| `soundable` | `Config.sound_enabled` | 回贴成功播放系统提示音（`NSSound.beep`） |
| `pasteboardDefaultSelectGroupLast: true` | activeTag | **不跟随**：分组筛选不跨会话/跨窗口持久化（评审 G1）。旧实现写进 localStorage，结果面板里选了「工作」，管理窗口的历史页就带着一个看不见的筛选打开，用户既看不到标签栏也去不掉 |

快捷键默认值：面板 `⌃C`、堆栈回贴 `⌃⇧V` 与 iCopy 一致；快捷回复 `⌃⇧R` 是 ClipBase 新增的默认值（可在设置里改）。环形转换**没有**全局快捷键，只有 Option＋拖动一个手势。

## 9. 从 iCopy 迁移历史（2026-10-26）

iCopy 的数据在沙盒容器里（`~/Library/Containers/cn.better365.iCopy/Data/Documents/`），
表结构与 ClipBase 差异较大，且富文本用 iCopy 私有的 `bvx` 分块容器存储，
因此迁移分成「Python 解 iCopy 专有格式」+「Rust 用 ClipBase 自身逻辑写库」两段：

| 阶段 | 产物 | 说明 |
| --- | --- | --- |
| `scripts/import_icopy.py` | `~/.clipbase/data/import/icopy.json` | 读 iCopy 的 `data.sqlite` / `reply.sqlite`，解 `PasteModel`（NSKeyedArchiver）取 `sourceAppName`、`richText`，`bvx` 经 Apple LZFSE（libcompression）解压后提取 HTML 原文，图片复制到 `~/.clipbase/data/files/` |
| `cargo run --bin clipbase_import -- <json>` | 写入 ClipBase 库 | 走 `HistoryService::insert_with_time`，**hash / preview / search_text / 序列化全部由 ClipBase 自己算**，与正常捕获入库的记录完全同构，并保留原始时间戳 |

实测迁移结果（iCopy 共 238 条）：

- 纯文本 177 条、富文本（HTML 可还原）10 条、图片 51 条、快捷回复 1 条（分组「常用」）；
- 入库 207 条：iCopy 内部有 19 组内容完全相同的记录，ClipBase 按内容 hash 去重合并
  （与正常捕获时「复制同样的内容只保留一条」的行为一致）；
- iCopy 富文本的 `bvx` 容器已破案：是 **Apple LZFSE 压缩流**（Compression 框架），
  不是私有格式——Python 侧用 ctypes 调 `/usr/lib/libcompression.dylib`
  （`COMPRESSION_LZFSE = 0x801`）解压后提取 HTML。修复提取后重生成 JSON，用
  `clipbase_import --replace` 按「创建时间 + 来源 App」原位更新（保留收藏/标签），
  不产生重复行；提取器同时加了最小有效 HTML 校验，防止 `<meta charset='utf-8'>` 空壳覆盖正文。

## 10. 第三轮加固（2026-10-27）

本轮以「面板第一次唤出是空的」为线索做了一次端到端排查，顺带补齐了模块、导入、命令层的
几处安全与性能问题。每条都附了对应的回归测试或验证方式。

### 10.1 面板「首次唤出空白」的真实原因

排查结论（不是猜测，已在用户真实数据库的副本上复现并验证修复）：

| 环节 | 现象 | 根因 |
| --- | --- | --- |
| `HistoryService::query` | 面板一条都不显示，状态栏一条看不懂的报错 | `content` 列是序列化后的 JSON，**任意一行解析失败就让整份查询返回错误**。用户库里有一条旧版本写入的坏行（`{"kind":"text","value":正文}`，字符串值少了引号），于是「全部」查不出来 |
| 点筛选就好 | 点「图片」（恰好排除了那条坏行）后列表出现内容 | 失败时 `items` 不清空、成功时整份覆盖：坏行被筛选排除后查询成功，用户才看到内容 |
| 空态文案 | 空列表一律显示「暂无历史」 | 加载失败与「确实没有历史」共用同一个空态，用户无法判断是「没内容」还是「出错了」 |
| 状态栏 | 错误显示成 `[object Object]` | 旧版本状态栏直接渲染错误对象（`String(e)`）；对象形态是 `{code,message,details}` |

### 10.2 对应改动

| 文件 | 问题 | 改动 |
| --- | --- | --- |
| `core/history/mod.rs` | 单条损坏的 `content` 让 `list_clips` 整体失败（面板空白 + 报错） | `query` 改为逐行解析：坏行 `warn` 日志后跳过，其余条目照常返回；**全部行都坏**时才返回 2002「历史记录无法读取」，不伪装成「暂无历史」 |
| `core/history/mod.rs` / `lib.rs` | 坏行只要还在，就会被永远跳过，用户也永远看不到那条内容 | 新增 `repair_corrupt_content()`，启动时跑一次：按旧格式把正文救回来（重写成合法 JSON + 重算 preview/search_text/size_bytes，FTS 由 UPDATE 触发器跟随）；救不回来的保持原样，绝不删用户数据。幂等 |
| `core/history/mod.rs` | `get` 把「内容损坏」报成「记录不存在，可能已被删除」；`delete` 先 `get` 再删，坏行**删不掉** | `get` 区分 2001（不存在）/ 2002（内容损坏）；`delete` 直接读 `hash` 写墓碑，解析失败只是没有可撤销条目，删除本身照常成功 |
| `windows/Panel.tsx` | 数据加载完全依赖 `clipbase://panel-shown` 事件；webview 未就绪时事件会丢，面板停在空白 | 事件之外再加窗口 `focus` 兜底（同一套逻辑，300ms 内去重避免双查询）：`panel.show()` + `set_focus()` 后 webview 必定收到 focus |
| `windows/Panel.tsx` / `pages/HistoryPage.tsx` | 空列表只有「暂无历史」一种文案 | 新增「加载失败」分支：显示真实原因 + 「重试」按钮，与「没有数据」区分开 |
| `store/useClipStore.ts` / `components/StatusBar.tsx` | 错误对象漏到渲染层就是 `[object Object]` | `setError(unknown)` 在 store 入口折成文案；状态栏渲染前再兜底一次 `errorMessage(error)` |
| `modules/manifest.rs` / `modules/host.rs` | `manifest.id` 只校验「非空且含 `.`」，`"../../Documents"` 也能通过；`install` 会 `remove_dir_all(modules/<id>)` 后复制，可删模块目录外的任意目录；`entry.backend`/`entry.ui` 可写绝对路径或 `../`（可执行 `/bin/sh`、可读任意文件） | 新增 `valid_module_id()`（ASCII 字母数字 + `-_.`、必须含 `.`、不得以 `.` 开头、不得含 `..`、≤128）与 `safe_relative_path()`（非空/非绝对/无 `\`/无 `~`/无盘符/不含 `..`）；`ModuleHost` 侧对 id 与 entry 再做一次校验（命令层不可绕过）。测试见 `qa_tests/manifest.rs` |
| `import.rs` / `core/sensitivity/mod.rs` | iCopy 迁移导入直写 `sensitive: false`，绕过敏感策略：密码明文入库，还会被同步上云 | 抽出 `sensitivity::apply(detector, policy, content)` 作为**唯一落地出口**（`state.rs` 与 `import.rs` 共用）；`Ignore` 策略命中时计入 `skipped`，只记「跳过一条疑似敏感内容」不含正文 |
| `commands/convert.rs` 等 5 处 | Tauri 2 同步命令跑在**主线程**：批量转换、批量回贴（每条 300ms）、模块安装（递归复制）、xlsx 导入导出、Finder 扩展开关都会把界面卡住 | 改 `pub async fn` + `tauri::async_runtime::spawn_blocking`，进度事件照旧 `app.emit` |
| `core/convert/job.rs` | `EngineGate` 里 `wait_timeout(...).unwrap()`：任一 worker panic 会让等待方跟着 panic，把整个转换线程池带走 | `unwrap_or_else(|e| e.into_inner())`，中毒时照常取用内部计数 |
| `commands/system.rs` | 选文件夹时也提示「请选择文件」 | 按 `dirs` 分支给出「请选择文件夹 / 请选择文件」 |

### 10.3 本轮验证

| 项 | 结果 |
| --- | --- |
| `cargo test --lib` | 273 passed（新增 `query_skips_corrupt_rows_instead_of_failing` / `query_reports_error_when_every_row_is_corrupt` / `repair_corrupt_content_salvages_unquoted_json` 等） |
| `cargo clippy --all-targets` | 无警告 |
| `npx tsc --noEmit` / `npm run build` | 通过 |
| 真实数据副本验证 | 用 `~/.clipbase/data/clipbase.db` 的副本跑 `repair_corrupt_content()`：修复 1 条、`all()` 返回 250 条、二次运行 0 条（幂等），修好的条目正文完整可读 |

## 11. 第四轮审计：文件操作与命令层（2026-10-27）

本轮不再按「报障 → 修」推进，而是按两条路径逐行读代码做**清单式审计**：
一条是「凡是碰文件的地方」（转换 / 导入 / 导出 / 模块安装），另一条是「凡是收用户输入的命令」
（`src-tauri/src/commands/*.rs`）。判定标准只有两个：出事时**用户能不能看懂**，
以及**磁盘上会不会留下说不清的残留**。

### 11.1 audit9：文件操作路径

| 检查项 | 发现的问题 | 改动 |
| --- | --- | --- |
| 临时文件清理 | 转换输出走「写临时文件 → rename」原子提交，进程被杀 / 断电时 `.xxx.clipbase-<uuid>.tmp` 会永久留在用户目录；`.tmp` 带点前缀，Finder 里看不见，只能靠 `ls -a` 发现 | `core/convert/output.rs` 新增 `sweep_stale()`：每次向某目录 `reserve()` 时扫一遍，只删「点前缀 + 含 `.clipbase-` + `.tmp` 结尾 + 超过 1 小时」的文件；同目录一次会话只扫一次（`swept` 集合），不拖慢批量转换 |
| 临时目录创建失败 | `job.rs` 里 `let _ = create_dir_all(work_dir)` 把错误吞掉，后续每个文件都以「找不到工作目录」失败，用户看到一堆无因的失败 | 创建失败直接把整批文件标记为失败（逐条 `fail` 并 `unregister`），错误文案带出 OS 原因 |
| xlsx 导出半成品 | `write_export_file` 直接 `save(path)`：写到一半失败会留下一个打不开的 `.xlsx`，而它就在「下载」里、看起来像成功 | 改为先写同目录隐藏临时文件再 `rename`；`rename` 失败时删临时文件。导出仍固定 `~/Downloads`（用户可见、可预期） |
| 路径穿越（远端） | `sync/webdav.rs::target_url` 把用户填的 `remote_file` 直接拼到 WebDAV 地址后面：`../` 能把快照写到同步目录之外，`http://` 开头会换成另一台主机 | 新增 `pub fn valid_remote_file()`（非空 / ≤200 / 无 `://` / 无 `\` / 无控制字符与 `?#` / 逐段非空且不是 `.` `..`），命令层与 provider 各校验一次；单测 `remote_file_rejects_traversal_and_urls` |
| 路径穿越（模块包） | `entry.backend` / `entry.ui` 只做词法校验，模块包里放一个符号链接就能让路径「看起来在包内、实际指向包外」，可执行/读取任意文件 | `modules/host.rs` 新增 `resolve_in_module()`：词法校验后再 `canonicalize` 比对，真实路径不在模块目录内直接 4001；sidecar 入口与 `read_module_ui` 都改走它 |
| 路径穿越（本地命令） | `reveal_path` 把用户给的相对名直接交给 `open`，`-x` 这种会被当成命令行开关；`open_system_settings(pane)` / `open_main_window(page)` 把未清洗字符串交给 `open` / 事件路由 | `reveal_path` 判空 + 不存在即报错 + `canonicalize` 归一；`pane` 拒空白/控制字符/超长；`page` 只收 ≤32 的 ASCII 标识符（不做白名单，避免前后端耦合） |
| 大文件 / 解码炸弹 | 文本、图片、HTML 内联、xlsx 导入、import JSON、模块 UI 全是整份读进内存；一张「几 KB 但 30000×30000」的图片足以让解码器 OOM 掉整个进程 | 逐处加上限并给中文提示：文本 32 MB、图片 512 MB **且** 8000 万像素（先看 `decoder.dimensions()` 再解码）、HTML 内联 24 MB、xlsx 20 MB、`import` 64 MB、模块 UI 1 MB |
| 非普通文件 / 编码异常 | 目录、FIFO、空文件被当成普通文件读，错误信息是 OS 原文；文本按 UTF-8 强制解码失败时只说「读取失败」 | `convert_one` 先 `metadata`：非普通文件 → 明确提示；0 字节 → 归为「跳过：文件为空」而不是失败。`import` 先判「不是文件 / 是空的 / 太大」三档 |
| 失败回滚（模块安装） | 复制到一半失败（磁盘满 / 无权限）会留下半个包；入库失败则留下「列表里看不到、也卸不掉」的孤儿目录 | `copy_dir_all` 失败与 `INSERT` 失败都 `remove_dir_all(dest)` 再返回；`install` 前置校验 `is_dir` → 有无 `module.json` → 是否已安装，文案全部中文 |
| 失败回滚（导入） | `import_from_file` 中途失败没有整体回滚 | 明确写进注释：按 `hash` 幂等，重跑同一份文件不会产生重复行（比「回滚用户数据」更安全）；体积/类型错误在读取前就拦下 |
| 权限 | 转换输出目录、模块目录都要求可写，但错误只说「写入失败」 | 输出分配失败带出具体目录与 OS 原因（`AllocError::Io`），便于用户判断是「没权限」还是「磁盘满」 |

### 11.2 audit10：命令层参数校验与错误文案

逐个文件过了一遍 `src-tauri/src/commands/*.rs`（`mod/clipboard/convert/reply/sync/config/modules/stack/ring/services/system`），
结论是**边界值缺失集中在「空串」与「0 行」两处**：

| 命令文件 | 问题 | 改动 |
| --- | --- | --- |
| `clipboard.rs` | `pin_clip` / `favorite_clip` / `set_clip_tag` 对已删除的 id 静默成功（UPDATE 0 行当成功）；标签可写入任意长、首尾带空白的字符串 | 0 行统一报 2001「该记录不存在，可能已被删除」；`set_tag` 先 `trim`、纯空白等同清除、>64 字符报 2003；`list_clips` 的 `limit` 上限 2000、`offset` 改 `i64::from(u32)`（原 `.max(0)` 是无效调用） |
| `convert.rs` | 空 `paths` 会返回「0 个文件、状态 Completed」的空任务 | 先 `trim` + 过滤空串，全空报 9001「没有选择要转换的文件」 |
| `reply.rs` | 导入 Excel 的空路径会落到 `fs::metadata`，报出英文 `No such file or directory`；分组名/标题无长度上限；`add_reply` 可写进不存在的分组（无外键 → 孤儿记录） | 空路径报 9005「没有选择要导入的 Excel 文件」；分组名 ≤32 且非空（2006）、标题 ≤200 且允许为空（只挂附件的回复是合法用法，但标题/正文/附件全空要拒）、分组不存在报 2004、删除/更新 0 行报 2005 |
| `sync.rs` | 地址、远端文件名、用户名、间隔都没校验，配错要等第一次同步才暴露（且是英文 `reqwest` 错误） | 开启同步时校验地址非空且以 `http(s)://` 开头（3005）；`remote_file` 归一 + `valid_remote_file`；用户名 `trim`；间隔 0 表示关闭、其余抬到 ≥60s |
| `modules.rs` | 安装空路径只在 `is_dir()` 那层报「选择的不是模块目录」；`disable_module` / `uninstall_module` 对不存在的 id 静默成功 | 空路径 4001「请选择模块目录」；`disable` / `uninstall` 命中 0 行报 4002「模块未安装」；`invoke_module` 校验方法名（非空 / ≤64 / 仅 `A-Za-z0-9._:-`），否则 4001 |
| `stack.rs` | 压栈不检查条目是否还存在：历史被清空 / 被同步删除后，栈里留着「死 id」，「发送全部」时才逐条失败 | 新增 `HistoryService::exists()`（`SELECT 1`，不解析 JSON，内容损坏也不误报），压栈前校验，缺失报 2001 |
| `config.rs` | `patch_config` 合并任意 JSON 后反序列化，越界值（如 0 轮询间隔、0 容量）会真的落库 | 数值边界在消费端兜底（轮询 ≥100ms、容量 0 → 出厂默认、同步间隔 ≥60s）；`open_system_settings` 清洗 `pane` |
| `system.rs` / `services.rs` / `ring.rs` | 无外部字符串入参（布尔、窗口动作），或已有平台错误码（9100–9105） | 确认无需新增校验；错误文案均为中文 |

错误文案方面，全仓扫描 `AppError::new/with_details` 的 `message`（要求含中文），共修正 18 处英文：
`error.rs::internal`（`internal error` → 「内部错误」，所有 `?` 自动转换都经过这里）、
`modules/manifest.rs` 7 处、`modules/host.rs` 6 处、`modules/instance.rs` 2 处、`modules/wasm.rs` 1 处、
`core/clipboard/mod.rs` 1 处。重扫结果只剩测试夹具里的一句 `fake paste failed`（不出现在用户界面）。

`details` 只放技术细节（id、文件大小、OS 原因），因为前端会把 `message` 与 `details` 用「：」拼起来显示：
所以 `move_reply` / `get_reply` 这类查询不再把 `rusqlite` 的 `Query returned no rows` 当 details，
统一成 `id=<数字>`。

### 11.3 本轮验证

| 项 | 结果 |
| --- | --- |
| `cargo test --lib` | 280 passed（新增 `exists_tracks_insert_and_delete` / `mutating_missing_clip_reports_2001` / `set_tag_trims_and_limits_length` / `missing_reply_reports_2005_instead_of_silent_success` / `add_reply_rejects_blank_group_and_blank_body` / `group_name_is_trimmed_and_bounded` / `remote_file_rejects_traversal_and_urls`） |
| `cargo clippy --all-targets` | 无警告 |
| `npx tsc --noEmit` | 通过（本轮未改前端） |
| 文案扫描 | `AppError` 的 `message` 全为中文明文，仅测试夹具保留英文 |

### 11.4 明确「不做」的几件事

- **导入不做事务回滚**：整份导入是「逐条 hash 幂等写入」，回滚反而会丢掉前半截已成功的条目；重跑即可补齐。
- **远端目录不自动创建**：WebDAV 的 `MKCOL` 各服务端行为不一致，统一改成 409 时提示「请先在网盘里建好目录」。
- **不跟随系统符号链接做目录级沙箱**：模块入口只保证「真实路径在模块目录内」，不限制模块内文件本身的权限（那是 Gatekeeper 与用户授权的事）。
- **转换的临时工作目录不主动删除**：`convert-<job_id>` 里可能还有用户想找回的中间结果；只清理同目录下超期的输出 `.tmp`。

## 12. 第五轮：取消菜单栏图标 + 修 Finder 右键（2026-10-27）

### 12.1 为什么要去掉菜单栏图标

用户对 ClipBase 的定位是「纯后台常驻」：按快捷键弹面板，东西都在面板里。菜单栏图 标那一整块（剪贴板面板 / 快捷回复 / Finder 转换 / 最近 / 模块 / 暂停监控 / 清空历史 / 设置 / 退出）既占位置，又与面板里的入口重复。

| 项 | 改动 |
| --- | --- |
| 菜单栏图标 | 删掉 `src-tauri/src/tray/mod.rs`（291 行）；`lib.rs` 不再声明 `pub mod tray` 与 `build_tray` |
| 菜单重建调用 | 清理 9 处 `tray::refresh`：`state.rs`（入库）、`commands/clipboard.rs`（删/清空）、`commands/system.rs`（撤销/重做）、`commands/modules.rs`（装/卸/启/停） |
| Cargo feature | 去掉 `tauri` 的 `tray-icon` feature（已无引用） |
| 模块 `tray` 贡献 | `manifest.rs` 仍解析（旧模块包不因这一条整个装不上），但不再有渲染位置，已写进枚举注释 |
| 退出入口 | 新增命令 `quit_app`（先收面板再 `app.exit(0)`），设置 → 关于 → 「后台运行」里给了按钮 + 二次确认；窗口聚焦时 ⌘Q 照旧 |
| 首次使用认知 | 关于页新增「后台运行」卡片：不占 Dock、不驻菜单栏、关窗只是隐藏、怎么唤出、怎么退出 |

### 12.2 「右键用不了了」的真实原因

排查证据链：

1. 在桌面空白处投合成右键 → 弹的是 **Finder 原生菜单**（有「更改墙纸 / 编辑小组件」），没有 ClipBase 项。
2. `AXUIElementCopyElementAtPosition` 扫点确认：桌面能被识别为 `AXGroup/AXDesktop`（Finder），所以**不是**「桌面识别不出来」。
3. `pluginkit -m -i com.clipbase.app.FinderExtension` 返回空；`pluginkit -a <.appex>` 后再查立刻变成 `+`。
4. 代码侧：`services::bridge::start()` 同步调 `register_if_needed()`，紧接着 `lib.rs` 用 `services::status().enabled` 初始化 `desktop_menu`。

结论：**`pluginkit -a` 是异步落库的**。注册后立刻查 `-m -i` 拿不到条目 → `enabled=false` → 桌面右键的接管代码被当成「未启用」。上一轮能用，是因为扩展在更早一次启动里已经注册好了；每次重新打包（ad-hoc 重签）都会把旧条目弄失效，于是这个竞态每一轮都能复现。

排查中还得到一个反直觉的事实（也是本轮的行为变更）：**macOS 26 上 Finder Sync 的 `.contextualMenuForContainer` 已经能注入桌面原生菜单**。修好注册后，直接在桌面右键就能看到「新建文本文件 / 在终端中打开」等项，并且与 Finder 自己的「新建文件夹 / 排序方式 / 查看显示选项 / 使用叠放」共存。原先那套 `CGEventTap` 接管反而会把 Finder 原生项全换掉，还占用「辅助功能」权限。

### 12.3 修法

| 位置 | 改法 |
| --- | --- |
| `core/services/bridge.rs::start` | 注册 + 核实移到后台线程（`finder-extension-boot`），不阻塞启动 |
| `register_and_verify()` | `pluginkit -a` 后轮询 `-m -i`（100ms × 20），条目真出现再返回它的启用状态；超时才报 warn |
| `desktop_menu::init` | 不再接受 `enabled` 参数，只存 `AppHandle`；启停统一由核实后的 `set_enabled` 决定，避开启动瞬间的假状态 |
| `desktop_menu::TAKEOVER` | 默认 `false`：不再用 `CGEventTap` 接管桌面右键（保留了整条实现与开关，系统改回去时改一个常量即可恢复） |
| 右键设置页 | 新增提示：「插件库里还没这条记录（重打包后常见），点启用即可重新注册」；同时把「桌面右键靠接管、需要辅助功能」的旧文案改成「与 Finder 窗口同一套注入」 |

### 12.4 验证

| 项 | 结果 |
| --- | --- |
| `cargo test --lib` | 280 passed |
| `cargo clippy --all-targets` | 无警告（含去掉 `tray-icon` feature 后全量重编） |
| `npx tsc --noEmit` | 通过 |
| `pluginkit -a` → `-m -i` | 注册后立即返回 `+  com.clipbase.app.FinderExtension(0.1.0)` |
| 桌面右键（实拍验证） | 重打包后直接右键桌面，菜单里出现「新建文本文件 (.txt) / 新建 Markdown / Word / Excel / PowerPoint / 在终端中打开」，且 Finder 原生项仍在 |
| 菜单栏 | `System Events` 取 `menu bar 2` 报「无效的索引」，确认状态栏图标已消失；⌃C 仍能唤出面板（截图确认 254 条） |
| 退出路径 | 设置 → 关于 → 退出 ClipBase；App 无 Dock、无菜单栏，快捷键 ⌃C 仍能唤出面板 |

---

## 13. 设置分区再次精简：下线「快捷键 / 快捷回复 / 模块」

### 13.1 背景

这三页曾经删过一轮，后来为了修「装了模块看不到列表」「快捷键没录制入口」被补回源码；
重新打包后它们又出现在侧边栏，用户明确表示「只去掉这三页」，保留
通用 / 剪贴板 / 环形转换 / 右键菜单 / 同步 / 权限 / 关于。

### 13.2 改动

| 位置 | 改法 |
| --- | --- |
| `src/windows/MainWindow.tsx` | `NAV` 删除 `hotkeys` / `replies` / `modules` 三项（含图标 path） |
| `normalizeSection()` | 三个旧标识统一回落 `general`：旧配置 `last_settings_section`、旧深链、旧 `PENDING_PAGE` 都不会白屏 |
| `src/lib/windows.ts` | `MainPage` 联合类型去掉这三个键，其它地方无法再导航过去 |
| `src/pages/Settings.tsx` | 路由分支与三个 import 一并移除；「粘贴栈顶」提示从「在「快捷键」页」改成「全局快捷键」 |
| `src-tauri/src/lib.rs` | 快捷键注册失败时的兜底从 `hotkeys` 改为 `general`（快捷键页已不存在） |
| `src/components/FirstRunGuide.tsx` | 顺带修掉上轮遗留的「常驻菜单栏」措辞（菜单栏图标已取消） |

### 13.3 保留但不再打包的代码

`src/components/HotkeySettings.tsx`、`src/components/ReplySettings.tsx`、`src/pages/Modules.tsx`
三个文件**留在仓库，不删除**（本项目不是 git 仓库，删了不可恢复），但已没有任何 import，
Vite 不会打包进产物（包体不变）。`tsc --noEmit` 仍会检查它们，因此不会腐烂成编译错误。

后端命令（`apply_hotkeys` / `validate_hotkey` / `list_replies` 系列 / `list_modules` 系列 / `install_module`…）
与数据表（`replies`、`modules`）全部保留：
- 快捷回复选择器窗口（⌃⇧R）仍能读取已有条目并回贴；
- 模块的贡献点（右键菜单项、转换能力）仍由后端解析并生效；
- 只是**界面上不再提供管理入口**。

### 13.4 影响与验证

| 项 | 结果 |
| --- | --- |
| `npx tsc --noEmit` | 通过（无悬空 import / 无遗漏分支） |
| 侧边栏 | 功能组：通用 / 剪贴板 / 环形转换 / 右键菜单；系统组：同步 / 权限 / 关于 |
| 深链兜底 | 传 `hotkeys`/`replies`/`modules` 均落「通用」，不白屏 |
| 快捷键 | 固定在默认值（⌃C 面板 / ⌃⇧V 栈顶 / ⌃⇧R 回复），界面上不可改；注册失败仍会自动弹设置（落到通用页） |

---

## 14. 环形转换按总规范重构：三层模型 + 唯一手势 + 单面（2026-10-27）

本轮把环形转换整条链按 `docs/REA-Filee-Ring-Integrated.md`（下称「总规范」）重写，
目标不是加功能，而是**把入口收敛到一个、把配置模型说清楚**。

### 14.1 三层模型

| 层 | 数据结构 | 职责 |
| --- | --- | --- |
| 预设 | `ConversionPreset` | **怎么转**：目标格式、质量、输出目录、命名规则。不进圆环顺序表，只被 Profile 引用 |
| Profile | `RingProfile { id, name, extensions, kind, preset_ids }` | **什么文件用哪些预设**：`kind` 为 `normal`（按扩展名匹配）或 `mixed_fallback`（兜底）。每个 Profile 最多 `MAX_PROFILE_SLICES = 12` 个扇区 |
| 扇区 | `RingAction` | **本批文件能否用、为什么不能用**：运行态切片，带 `enabled` / `disabled_reason`，不落库 |

顺带把 `ring.actions` / `ring.back_actions` 两个旧字段删掉（`legacy_actions` /
`legacy_back_actions` 只用于一次性迁移，迁移后立刻清空）。

### 14.2 选盘规则（总规范 §2.1 / §2.2）

扩展名规范化（`core::ring::profile::normalize_extension`）：trim + 小写 → 拒 `..` →
去前导 `*` / 前导与末尾的 `.` → 限长 16 → 只允许 `[a-z0-9._-]` → 含点则必须是
`ConvertFormat::from_ext` 认识的复合扩展名。前端 `RingSettings.tsx` 的
`normalizeExtension` 是这套规则的镜像，服务端仍是唯一裁判。

选盘（`select_profile`）按顺序命中：

1. 第一个「覆盖本批全部扩展名」的 `normal`；
2. 第一个「扩展名非空且覆盖全部」的 `mixed_fallback`；
3. 第一个扩展名为空的 `mixed_fallback`（catch-all）；
4. 第一个 `mixed_fallback`；
5. 第一个 Profile。

关键收益：`png + jpg` 同族混选仍落图片盘（普通优先于兜底）；扩展名识别不出来时
直接走兜底，不会随机落在一个带扩展名限制的盘上。`shadowed_by` 只给设置页提示
「这个盘被前面那个完全遮蔽」，不阻止保存 —— 竞争本来就由列表顺序决定。

**缺陷修复**：`extensions_of` 只收 `ConvertFormat::from_ext` 认识的扩展名。
此前 `png + 备份.tar.gz` 会因为 `gz` 没人覆盖而整批掉到兜底盘，明明只有一张图要转。

### 14.3 运行态：起手文件才是权威载荷

`option_tap` 新建的全局 tap 在 `leftMouseDown` 回调里**同步**调用
`RingManager::grab_start_file(x, y)`：

- 主路径 `finder::file_at_point` 用 `AXUIElementCreateSystemWide` +
  `AXUIElementCopyElementAtPosition` 命中手指下的元素，比对 `AXUIElementGetPid`
  确认是 Finder 自己，再上溯 `AXURL`；
- AX 未命中（列表视图结构不认识 / 无权限）才退回「读 Finder 选中项」预取；
- **不做松手复核**：接管拖动后移动事件被吞，Finder 的选中项已经不可信，再读一次
  只会在多选时把载荷从「起手那个文件」改成「一堆文件」。

AX 取证的两处健壮性措施（都写进了 `finder/macos.rs` 的注释）：

| 措施 | 原因 |
| --- | --- |
| `file_url_of_ancestors` 整体 400ms 硬截止 | 函数跑在全局 tap 回调里，卡住会导致 tap 被系统禁用，之后手势直接失灵 |
| 每个 `AXParent` 单独 `AXUIElementSetMessagingTimeout(0.2)` | 新建的 AX 元素不继承别处的超时值，只给起始元素设一次等于没设 |

### 14.4 单面环 + 禁用项保留位置

`RingWindow.tsx` 重写后只有**一面**，输入只剩两个：拖动移动、松手提交。删掉的交互：
翻面、Tab / 方向键 / Enter / Esc、滚轮换面、单击直接执行、自动换面提示、翻面动画
（`@keyframes ring-sector-flip` 已从 `index.css` 删除）。

不可用的动作**不隐藏**，仍占原位置，只是压暗（`MUTED_FILL`）并在 hover 时变红底
（`REJECT_FILL`）：

- 松手时若命中禁用扇区 → 中心显示 `disabled_reason` 1.3s（`REJECT_HOLD`）后自动收起，
  **不创建 Job**；
- `RingManager::execute` 里还有第二道闸，直接对禁用项发命令同样被拦下；
- 初始高亮取「记住且当前可用」的扇区，否则第一个可用项 —— 只是高亮，不会自动执行。

窗口属性同步收敛：`show_ring_window(app, origin)` 只接受起手点，钉在原位、
永远 `set_ignore_cursor_events(true)`、**不 set_focus**（焦点必须留在 Finder，
否则拖动的追踪循环会断）。

### 14.5 已删除的额外入口

| 入口 | 处理 |
| --- | --- |
| 全局快捷键 `hotkeys.ring_convert` | 字段、`HotkeyAction::RingConvert`、注册逻辑一并删除 |
| Finder 右键服务 `ServiceAction::OpenRingConvert` | 删除，右键菜单表里那一行同步移除 |
| `start_from_hotkey` / `start_from_paths` 命令 | 删除，只剩拖动一条路径 |
| `RingSnapshot.back_actions` | 删除，改为 `profile: RingProfileInfo \| None` |
| 前端 `MAX_PER_FACE` | 改为 `MAX_SLICES`，与 Rust `MAX_PROFILE_SLICES` 对齐 |

> 环形转换现在**没有任何菜单/快捷键入口**，唯一手势是 Finder 里的「Option + 左键拖动」。

### 14.6 迁移与内置 Profile

`Config::migrate` 在 `ring.profiles` 为空时按旧字段生成 Profile：正面顺序为主、
背面追加去重、截到 12 项；超过 12 项的预设**留在预设库**，只是不上环。
随后 `RingProfile::seeds` 按预设产出的目标格式拆成三类：

- `images`（图片，`normal`，扩展名来自 `image_extensions()`）；
- `text`（文本，`normal`，非图片的预设）；
- `mixed`（其他文件，`mixed_fallback`，扩展名为空，作为 catch-all）。

只在顺序「完全等于旧默认」时才把 `doc.md` 这类被塞在背面的项升级到正面，
用户自己排过的顺序一律不动。`normalize_profiles` 每次落库都跑，因此必须幂等。

### 14.7 验证

| 项 | 结果 |
| --- | --- |
| `cargo test --lib` | 295 passed / 0 failed |
| `cargo clippy --lib --all-targets` | 0 warning |
| `npx tsc --noEmit` | 通过 |
| 残留引用扫描 | `ring_convert` / `RingConvert` / `start_from_hotkey` / `start_from_paths` / `back_actions` 在 `src`、`src-tauri/src`、`scripts` 内均无命中 |

本轮顺带修掉的三处真实问题：`extensions_of` 收进不可转换扩展名（见 14.2）、
`finder` 与 `desktop_menu` 重复声明的 AX 符号签名不一致（`clashing_extern_declarations`，
签名漂移即 UB）、`file_url_of_ancestors` 没有超时保护（见 14.3）。

## 15. 按三份专项文档逐条对齐运行态（2026-10-27）

以 `REA-Filee-Ring-Integrated.md`（总规范）、`REA-Filee-Ring-Wheel.md`（运行态
几何/动画/状态机）、`REA-Filee-Ring-Editor.md`（设置页编辑交互）为准，把代码里
**文档写死了数值或公式、但实现不一致**的地方逐条改齐。改动只落在渲染与取证两层，
会话状态机、选盘规则、Router/Job 均未动。

### 15.1 本次对齐清单

| # | 文档出处 | 原文要求 | 改前 | 改后 |
| --- | --- | --- | --- | --- |
| 1 | Wheel §2.2 | `N = 1` 时绘制完整圆环，不画无意义的单扇区分割 | 单扇区仍留 3px 缺口 | `sectorPath` 在 `n === 1` 画整圆（外顺内逆，nonzero 挖孔） |
| 2 | Wheel §1.3 | 打开从**中心**缩放并带轻微旋转 | `transform-origin` 设在扇区标签点，从扇区自身冒出 | origin 改为圆心 `(C,C)`，`rotate(-6deg) scale(0.18) → 1` |
| 3 | Wheel §1.3 | 关闭单扇区 0.16s、逐项 0.01s、EaseInCubic | 松手即隐藏窗口，无关闭动画 | 新增 `ring-sector-out`，逐项 `i×10ms`，播完再 `ringCancel` |
| 4 | Wheel §1.3 A | 中心显示文件数量和格式；同格式「数量 + 格式名」，混合显示混合数量 | 只显示 Profile 名 + `N 项` | `3 个 PNG` / `3 张图片` / `5 个文件 · 混合` |
| 5 | Integrated §4.2 | 可用扇区中心提示「松手转为〈Preset 名〉」 | 显示 `转 3 · 跳过 1` | 第二行改为「松手转为 X」，计数下移到第三行 |
| 6 | Wheel §1.3 | 高亮强调色最高 0.82 不透明度；禁用扇区 0.55 倍并带 tooltip | 强调色全不透明；禁用 0.5，无 tooltip | `ACCENT_FILL = rgba(232,163,61,0.82)`、`MUTED_FILL = rgba(18,18,20,0.45)`、禁用扇区加 `<title>` |
| 7 | Integrated §3.3 | 环外 `NoDrop`：松手同样取消 | 环外时中心仍写「松手转为 X」 | 新增 `noDrop`，环外中心改显「松手取消」 |
| 8 | Editor §1.5 | 右键扇区打开对应 preset 编辑器 | 只有 hover 铅笔与 Enter | 扇区 `onContextMenu` 走同一入口 |
| 9 | Integrated §10.2 | 非文件拖动不显示圆环 | 点在空白处也会弹「没读到拖动的文件」提示盘 | 新增 `finder::StartHit` 三态：`NotFile` 在 `on_down` 直接放弃接管；`Unknown`（AX 超时/他窗遮挡）仍保留解释盘 |
| 10 | Integrated §10.2 / 实际权限体验 | 非文件拖动与权限状态都要如实反映 | `option_tap` 只在启动时安装一次，没有权限就直接放弃，用户在系统设置里勾完必须重启 App | `run_tap` 改为每 3s 轮询重试（与 `desktop_menu` 同一套策略），授权到位后自动装上监听，不必重启 |

`StartHit` 的三态是第 9 条的关键：`file_at_point` 原来只返回 `Option<String>`，
把「确定不是文件」和「没读到」混成一种，于是空白处拖动也会弹盘。现在
`UrlLookup` 明确区分「上溯到根没有 AXURL」（`Absent` → 非文件）与「撞上 400ms
硬截止」（`Timeout` → 未知），并顺手把文件夹也归为 `NotFile`。

### 15.2 仍属 ClipBase 适配、不按文档改的部分

| 项 | 文档 | 现状与理由 |
| --- | --- | --- |
| 拖动阈值 | Wheel §1.1：取系统值，回退 4 物理像素 | 固定 8pt。接管后事件被吞，误触发的代价是用户那一下拖拽失效，比起「4px 就弹盘」宁可迟钝一点 |
| 多选范围 | Integrated §3.2：推荐真实 drag payload | 起手文件在 Option 按下时预取的选中集合内 → 转整批；否则只转起手那一个。属文档允许的过渡适配，已在 14.3 记录 |
| 收缩/重排动画 | Wheel §1.3：弹簧刚度 420 / 阻尼 36 | 用 CSS `cubic-bezier` 等效，保留「让位 + 半透明预览」的信息结构（文档 B 类） |
| 分类与布局尺寸 | Editor §1.6 / B 类 | 只保留「全部 / 图片 / 文档」，画布 320 逻辑像素内嵌，几何比例与运行态同一份 |

### 15.3 验证

| 项 | 结果 |
| --- | --- |
| `cargo check --lib` | 通过 |
| `cargo test --lib` | 296 passed / 0 failed |
| `cargo clippy --lib --all-targets` | 0 warning |
| `npx tsc --noEmit` | 通过 |


## 16. 纯后台运行：Dock 里不留图标（2026-10-27）

### 16.1 现象与真因

`src-tauri/Info.plist` 里一直写着 `LSUIElement = true`，打包产物的 Info.plist 里也确实是
`LSUIElement = true`，但 Dock 上照样出现 ClipBase 图标。`lsappinfo info -app com.clipbase.app`
给出的判据是 `type="Foreground"`——说明运行时它根本没被当成 UIElement。

真因在 tao：`tao-0.37.2/src/platform_impl/macos/app_state.rs` 的 `apply_activation_policy`
在 `applicationDidFinishLaunching` 里**无条件**调用
`NSApp.setActivationPolicy(...)`，取值来自内部 aux state（默认 `Regular`）。
它整份源码里没有 `LSUIElement` 这个字面量——也就是说 **tao 从不读 plist 的 LSUIElement**，
LaunchServices 启动时按 plist 判成 UIElement，启动完成后又被 tao 改回 Regular，图标就冒出来了。

### 16.2 修法

在 `lib.rs` 的 `setup` 里（一定晚于 `applicationDidFinishLaunching`）显式改回 Accessory：

```rust
#[cfg(target_os = "macos")]
app.set_activation_policy(tauri::ActivationPolicy::Accessory);
```

验证：`lsappinfo info -app com.clipbase.app | grep type=` 现在返回 `type="UIElement"`。

### 16.3 随之要补的两件事

| 风险 | 处理 |
| --- | --- |
| Accessory 下窗口不会自动成为前台 App，点了输入不进 | tao 的 `set_focus` 内部就是 `makeKeyAndOrderFront` + `activateIgnoringOtherApps:`，所以 `show_panel_window` / `show_reply_picker` / `show_main_window` 原有的 `set_focus()` 已经够用；`show_main_window` 还额外先调了一次 `activate_app()` |
| 没有 Dock 图标、也没有菜单栏图标，用户失去「点一下就有反应」的入口 | 改为接 `RunEvent::Reopen`：Finder 里双击 .app 或 `open ClipBase.app` 时 macOS 会发 reopen，这里调 `show_main_window`。tao 自己不实现 `applicationShouldHandleReopen:`，不接就是「双击了但什么都没发生」 |

环形盘窗口不需要 key window（交互全部由 CGEventTap 驱动，且窗口本身 `set_ignore_cursor_events(true)`），
所以不收焦点，行为不受影响。

### 16.4 验证

| 项 | 结果 |
| --- | --- |
| `cargo test --lib` | 296 passed / 0 failed |
| `cargo clippy --lib --all-targets` | 0 warning |
| `lsappinfo ... type=` | `UIElement` |
| 打包体积 | 14.00 MiB |

## 17. Option＋拖动「按了没反应」：把 TCC 失效变成可诊断（2026-10-27）

### 17.1 现象与真因

设置里开关是开的、辅助功能列表里也勾着 ClipBase，但按住 Option 拖动文件毫无反应。
日志里只有一行反复出现：

```text
INFO core::option_tap: Option+拖动：初始化 enabled=true accessibility=false
```

`accessibility=false` 意味着 `AXIsProcessTrusted()` 一直为假，`CGEventTapCreate` 从未成功，
手势层一次都没被调用——不是判定逻辑的问题。

真因在签名：`src-tauri/tauri.conf.json` 里 `signingIdentity = "-"`（ad-hoc）。
每次重新构建都会生成新的 cdhash，而 TCC 记录的是旧二进制的指纹。结果是**系统设置里那条
记录看着是打开的，实际对当前进程不生效**——用户重勾也没用，因为勾的还是同一条失效记录。

### 17.2 修法

| 位置 | 改动 |
| --- | --- |
| `core/option_tap::repair` | `tccutil reset Accessibility <bundle>` 清掉失效记录，再带 prompt 重新登记 |
| `core/option_tap::init` | 未授权时主动请求一次系统引导；`.ax_prompt_at` 做 7 天冷却，拒绝过的用户不会被每次启动骚扰 |
| `core/option_tap::status` | 新增 `GestureStatus`：`enabled / accessibility / tap_alive / frontmost_ok / state / last_note` |
| `option_tap::run_tap` | tap 建不出来、挂不上 run loop 时把原因写进 `last_note`，不再只进日志 |
| `option_tap::on_down` | 起手未待命时记录具体原因（关闭 / 盘在屏上 / 前台不是 Finder / 起手点不是文件） |
| `option_tap::on_drag` | 待命后从**第一个** `leftMouseDragged` 就吞，不再等到 8px：Finder 自己的拖拽阈值只有 3~4px，放行会让它抢先启动真实拖拽，之后再吞只会把拖拽卡在半路、松手时把文件丢在指针位置 |
| `commands::ring_gesture_status` / `ring_gesture_repair` | 供设置页轮询与一键修复 |
| `pages/RingSettings.tsx` | 1.5s 轮询 + 窗口 focus 立即刷新；分开显示「辅助功能」与「事件监听」；新增「修复授权」按钮与「最近一次没有响应」提示 |

`last_note` 的取值就是排障路径：点击、普通拖动不会覆盖它，只有按着 Option 的失败才写。

### 17.3 验证

| 项 | 结果 |
| --- | --- |
| `cargo test --lib` | 294 passed / 0 failed |
| `cargo clippy --lib` | 0 warning |
| `npx tsc --noEmit` | 0 error |
| 打包 | ClipBase.app 13.94 MiB |
| 重启后日志 | `未获得辅助功能权限：已请求系统引导`，随后 3s 轮询等待授权 |

### 17.4 已知限制

- ad-hoc 签名下，**每次重新构建都要重新授权一次**。根治办法是换稳定签名（本地自签名证书或
  Developer ID），让 TCC 的 designated requirement 落在证书上而不是 cdhash；本次未改签名配置。
- 辅助功能与回贴共用同一项授权，`reset` 之后两者都要重新勾选。
- tap 回调里同步做 AX 命中（超时 0.2s）是为了赶在 Finder 进入 tracking loop 之前取到起手文件；
  极端情况下事件回调会超时，tap 会被系统禁用，`TAP_DISABLED_BY_TIMEOUT` 分支会立即重新打开。

## 18. 浅色模式白底白字：把 Tailwind 字面量收进变量（2026-10-27）

### 18.1 现象与真因

用户反馈「剪切板的文字都看不清楚」，截图里历史卡片的正文几乎不可见，但顶部分类标签、时间、
字节数都是正常深色——即背景已经是浅色，只有正文还是白色。

外观切换（清单 25）的机制是：`App.tsx` 按配置/系统给 `<html>` 挂 `.light`，`index.css` 的
`:root.light` 变量组整体换肤，再靠一段 `.light [class~="text-white/NN"]` 补丁把 JSX 里为深色底
写的 `white/xx` 工具类逐档映射成 `black/xx`。补丁本身工作正常，问题在于**它只认 `white/xx` 一族**，
凡是绕过它的颜色都会漏色：

| 漏网处 | 定义 | 浅色下的表现 |
| --- | --- | --- |
| `colors.preview = "#E8E8ED"` | tailwind.config.ts 字面量 | 卡片正文 `text-preview` 近白 → 白底白字（本次报的就是这条） |
| `text-white/20·50·55·65·75·80` | JSX 直写，补丁没列这些档 | 仍是白色 |
| `bg-white/30`、`border-white/12·15·25·40`、`divide-white/[0.06]` | JSX 直写 | 填充 / 描边 / 分隔线在浅底上消失 |
| `hover:bg-white/[0.05]·[0.12]`、`hover:text-white/85·90` | JSX 直写 | 悬停态仍走白色 |
| `text-warning` `#FFD60A`、`text-danger-soft` `#FF8A80`、`text-accent` `#E8A33D` | tailwind 字面量 | 亮色字在浅底上对比不足（状态栏「监控已暂停」即为一例） |
| `text-badge-*-fg`（`unknown` 是 50% 白） | tailwind 字面量 | 类型徽章亮字读不清，`unknown` 直接看不见 |
| `bg-black/55` 铅笔按钮、`bg-black/80` 拖动 ghost | 故意深底 | 被 `text-white` 补丁翻成黑底黑字 |

### 18.2 修法

1. `colors.preview` 改为 `var(--text-preview)`（深色 `#E8E8ED` / 浅色 `#1C1C1E`），卡片正文与详情
   弹窗一起恢复；该色没有 `text-preview/NN` 这类带透明度的用法，换成 `var()` 不会丢档位。
2. 浅色补丁区补齐缺失档位（合计 62 条），并加语义色映射：`text-warning → var(--warning)`、
   `text-danger-soft → var(--danger-text)`、`text-accent → #B3741F`（accent-700 同值），徽章 fg 换同系
   深色、`bg-badge-unknown` 换 7% 黑。
3. `divide-white/[0.06]` 的颜色是加在**子元素**上的，所以规则写成
   `.light [class~="divide-white/[0.06]"] > :not([hidden]) ~ :not([hidden])`。
4. 新增 `on-dark` 标记 + 双属性选择器（特异性高于单属性补丁）作为例外通道，保护「故意深底」
   元素里的白字白边不被翻黑；hover 态仍由 Tailwind 自己的规则接管，不受该例外干扰。

### 18.3 为什么不用「强制深色」

把配置里的 `theme` 定成 `dark` 一行就能盖住症状，但浅色是清单 25 已交付的能力，系统外观也是用户
自己的设定。真正的缺陷是**字面量颜色绕过了换肤机制**，所以修法是把颜色收进变量 / 补齐补丁，
而不是取消浅色模式。

### 18.4 验证

| 项 | 结果 |
| --- | --- |
| `npm run build` | CSS 52.50 kB，产物含 `.text-preview{color:var(--text-preview)}` |
| 补丁条数 | `.light [class~=` 共 62 条（改前 45 条） |
| `on-dark` 例外 | 3 条规则进产物（压缩器去引号，选择器仍有效） |
| `npx tsc --noEmit` | 0 error |
| 打包 | ClipBase.app 13.94 MiB；release/ClipBase-0.1.0.dmg 7.2 MB，checksum VALID |

### 18.5 后续约束

新增 Tailwind 颜色时，凡是结构色 / 文字色一律指向 `var(--…)`，不写 hex 字面量：字面量会静默绕过
换肤，且只在用户系统恰好是浅色时暴露。改完浅色相关样式后，用 `grep -o '\.light \[class~=' dist/assets/*.css
数一下规则条数，能最快确认补丁没被 Tailwind 的 purge 吃掉。

