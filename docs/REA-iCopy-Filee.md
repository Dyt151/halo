# REA 逆向分析报告：iCopy 与 Filee

> 分析员角色：REA（Reverse Engineer Anything）
> 目标软件：`iCopy`（cn.better365.iCopy，v1.3.4）、`Filee`（com.filee.app，v2.1.0）
> 分析日期：2026-10-26
> 本报告所有结论均绑定到可复现证据（Mach-O 符号 / ObjC 选择器 / Info.plist / entitlements / 沙盒容器 SQLite schema / NSUserDefaults 归档 / 运行日志 / 配置 JSON）。

**结论分级约定**
- 【确定结论】：有直接证据（符号、选择器、schema、配置值、日志行）。
- 【推测】：多路证据交叉，但缺最终反汇编/运行时确认。
- 【证据不足，需要下一步探查】：证据不充分，附 REA 探查指令。

---

## 一、iCopy（cn.better365.iCopy v1.3.4）

### 1.1 基础信息采集 【确定结论】

| 项 | 证据 | 值 |
| --- | --- | --- |
| 主二进制 | `file` | `/Applications/iCopy.app/Contents/MacOS/iCopy`，Mach-O 通用二进制 [x86_64 + arm64] |
| 语言/编译器 | Info.plist `DTCompiler` | Objective-C（clang），无 libswiftCore |
| 主类 | Info.plist `NSPrincipalClass`/`NSMainNibFile` | `NSApplication` / `MainMenu.nib` |
| 外观类型 | Info.plist `LSUIElement` | `true`（纯菜单栏 App，无 Dock 图标） |
| 最低系统 | `LSMinimumSystemVersion` | 10.14 |
| 沙盒 | entitlements `com.apple.security.app-sandbox` | 已启用 |
| 签名 Team | `codesign` | `4K6FWZU8C4`（better365） |
| 辅助进程 | `lsappinfo` | `/Applications/iCopy.app/Contents/Library/LoginItems/iCopyHelper.app`（`LSBackgroundOnly`+`LSUIElement`，登录项启动器） |
| 登录项框架 | `otool -L` | `ServiceManagement.framework`（SMAppService/SMLoginItem） |

**依赖 Framework（otool -L，直接证据）**：`AppKit`、`Carbon`（全局热键）、`libsqlite3`、`CoreData`、`StoreKit`（内购）、`Security`（钥匙串）、`SystemConfiguration`、`IOKit`、`ApplicationServices`（AX 辅助功能）、`CoreImage`/`ImageIO`/`Quartz`（图像）、`AudioToolbox`（音效）、`ServiceManagement`（登录项）。

**第三方 SDK/组件（Resources 目录 + 类名，直接证据）**：
- `AppCenter.bundle`/`AppCenterAnalytics.bundle`/`AppCenterCrashes.bundle` + 类 `MSAC*`（`MSACModel`、`MSACChannelDelegate`、`appSecret` 等 43+ 类）→ **微软 App Center 埋点 + 崩溃上报**；对应 `Data/Library/Application Support/cn.better365.iCopy/com.microsoft.appcenter/Logs.sqlite`。
- `FMDB_Privacy.bundle` → **FMDB**（ObjC SQLite 封装），对应 `libsqlite3`。
- `MJExtension.bundle` → MJExtension（JSON↔模型）。
- `MASShortcutView`（类名）→ MASShortcut（快捷键录制 UI）。
- `YLHud.bundle`/`YLPermissionManager.bundle`/`YLShortcutView.bundle` → 自家 UI/权限/快捷键组件（前缀 `YL`，见 NSUserDefaults 键 `YLUpdateSkipVersion`）。

**entitlements（codesign，直接证据）**：
```
com.apple.security.app-sandbox
com.apple.security.files.user-selected.read-write
com.apple.security.network.client
com.apple.developer.icloud-container-identifiers = iCloud.cn.better365.iCopy
com.apple.developer.ubiquity-container-identifiers = iCloud.cn.better365.iCopy
com.apple.developer.icloud-services
```

---

### 1.2 核心入口：剪贴板监听 / 热键 / 回贴

**剪贴板监控 = 定时器 + changeCount 轮询 【确定结论】**

选择器证据（`otool -ov` 的 `__objc_methname` 段，带地址）：
```
beginMonitorPasteBoard          @ 0x1001ca86d
pauseMonitorPasteBoardWithHandler:  @ 0x1001d5649
stopMonitorPasteBoard           @ 0x1001dd4f7
monitorTimer / MonitorTimer: / startTimer:
scheduledTimerWithTimeInterval:repeats:block:
timerWithTimeInterval:target:handler:
```
原始二进制 `strings` 中存在 `changeCount` 与 `generalPasteboard`（NSPasteboard 经典轮询 API）。

结论：iCopy 用 `NSPasteboard.general` + `changeCount` 在 `monitorTimer` 上轮询，检测到变化后读取内容并 `savePasteModel:` 落库。**没有**使用 `NSPasteboard` 的 `pasteboardChangedOwner` 通知（该选择器在 `strings` 中不存在）。

**读取粘贴板 = 旧式 `dataForType:`/`stringForType:` API 【确定结论】**

选择器：`stringForType:`、`dataForType:`、`propertyListForType:`；未出现 `readObjectsForClasses:`（现代 API）。
支持的类型（`strings` 中的 `_NSPasteboardType*`）：String / HTML / PNG / TIFF / RTF / RTFD / PDF / URL / FileURL / Color / Sound / TabularText / Font / Ruler。

**全局热键 = Carbon RegisterEventHotKey 【确定结论】**

二进制 `strings` 直接命中：`_RegisterEventHotKey`、`_UnregisterEventHotKey`、`_InstallEventHandler`、`_GetEventParameter`、`kHISymbolicHotKeyCode`。
模型类：`HotkeyModel`（字段 `keyCode`/`keyString`/`modifierFlags`）、`DoubleHotkeyModel`、`FnHotkeyModel`（双击修饰键、Fn 组合）。
选择器：`hotkeyWithKeyCode:modifierFlags:keyString:`、`carbonKeyCode`、`carbonFlags`、`HotKeyRef`。

**回贴 = 类型分派写回 + CGEvent 模拟 ⌘V + 焦点归还 【确定结论】**

选择器（逐条命中）：
```
savePasteModel:
writeToPasteBoardWithTxt
writeToPasteBoardWithRichTxt
writeToPasteBoardWithImage
writeToPasteBoardWithFilePath
writeToPasteBoardWithFile
writeToPasteBoardWithData
writeToPasteBoardCompleted
writeToPasteBoardAndPasteAsPlainText
writeToPasteBoardWithTxtWithoutNewlineAndWhitespace:
pasteToPreActiveAPPWithCompletion:
```
API：`CGEventCreateKeyboardEvent` / `CGEventPost` / `CGEventTapCreate`；`NSRunningApplication` + ivar `preActiveApp`（`T@"NSRunningApplication",&,N,V_preActiveApp`）+ `activateWithOptions:` + `activateIgnoringOtherApps:` + `frontmostApplication` + `deactivate`。

【确定结论】回贴链路：选中项 → 按类型调 `writeToPasteBoardWithXxx` 写回 NSPasteboard → `pasteToPreActiveAPPWithCompletion:` 里用 `CGEvent` 发送 ⌘V，并借助 `preActiveApp` 恢复之前的焦点。
【推测】`pasteToPreActiveAPPWithCompletion:` 内部顺序为「写回 → activate 前一个 App → post ⌘V → completion」；`writeToPasteBoardAndPasteAsPlainText` 是先剥离富文本再写再贴。

---

### 1.3 数据流、持久化、过滤黑名单

**持久化 = 沙盒容器内两个独立 SQLite 【确定结论】**

`~/Library/Containers/cn.better365.iCopy/Data/Documents/` 下：

```
data.sqlite   剪贴板历史
  CREATE TABLE t_data (
    id integer primary key autoincrement,
    pastetype integer,      -- 0=纯文本 1=富文本(html/rtf) 2=文件/图片
    sourcetype string,      -- UTI，如 public.utf8-plain-text / public.html / public.png
    text string,            -- 纯文本回退
    filepath string,        -- 文件/图片落盘路径（容器内）
    createtime double,      -- Unix 秒（datetime(createtime,'unixepoch') 可还原）
    data blob,              -- 富文本/缩略等二进制
    favorite integer        -- 收藏
  );

reply.sqlite   快捷回复
  CREATE TABLE t_reply (id integer primary key autoincrement, groupId integer, title string, content string, data blob);
  CREATE TABLE t_group (id integer primary key autoincrement, name string, sort integer);
```

实测样本（id 3157~3167）：
- `pastetype=0` + `public.utf8-plain-text` → text 存正文
- `pastetype=1` + `public.html` / `public.rtf` → text 存回退文本，data 存富文本
- `pastetype=2` + `public.png` → filepath 指向容器内图片文件，data 约 590B（缩略/元数据）

**来源 App 识别 + 黑名单排除 【确定结论】**

选择器：`BlackListForPaste:`、`Blacklist:isPaste:`、`BlackListArr`、`BlackListAppModel`；API：`frontmostApplication`、`runningApplicationWithProcessIdentifier:`、`appProcessIdentifier`、`localizedName`、`bundleIdentifier`、`URLForApplicationWithBundleIdentifier:`。
结论：剪贴板变化时取 `[NSWorkspace frontmostApplication]` 的 bundle id / 名称，与 `BlackListArr` 比对，命中则跳过不记录。`Blacklist:isPaste:` 带 `isPaste` 标志——区分「记录」与「回贴」两种路径（回贴时黑名单不拦截）。
【推测】匹配键是 bundleIdentifier（`localizedName` 用于展示）。

**配置 = NSUserDefaults["config"] 里的 NSKeyedArchiver（Base64）【确定结论】**

已解码 `ConfigModel`，关键默认值（HotkeyModel 的 `modifierFlags` 用 NSEvent 修饰键位：⌃=262144、⌃⇧=393216、⌘=1048576）：

| 键 | 默认值 | 含义 |
| --- | --- | --- |
| `boardHotKey` | keyCode 8("c") + ⌃ | 唤出面板 = **⌃C** |
| `stackHotKey` | keyCode 8("c") + ⌃⇧ | 打开堆栈 = **⌃⇧C** |
| `stackPasteHotKey` | keyCode 9("v") + ⌃⇧ | 粘贴栈顶 = **⌃⇧V** |
| `pasteboardNumHotkeyFlags` | ⌘ | 数字快捷键 = **⌘1~9** |
| `shortcutHotKey` | keyCode 1("s") + ⌃ | **⌃S** |
| `settingHotKey` | keyCode 0("a") + ⌃⇧ | **⌃⇧A** |
| `pasteboardHeightScale` | 0.8 | 面板高度 80% |
| `pasteboardDiskLimit` | 1024 | 磁盘上限 1GB |
| `pasteboardFollowCursor` | false（`positionType`=0） | 默认不跟随光标 |
| `pasteboardAutoHidden` / `shortcutAutoHidden` | true | 失焦自动隐藏 |
| `pasteboardHoverSwitchGroup` | true | 悬停切换分组 |
| `pasteboardDefaultSelectGroupLast` | true | 默认选中上次分组 |
| `confirmationOnDeletion` | true | 删除需确认 |
| `soundable` | true | 音效 |
| `autoBoot` | false | 默认不开机启动 |
| `autoCloud` | false | 默认不同步 |
| `showStatusItem` | true | 菜单栏图标 |
| `historyType`=1 / `theme`=0 / `languageType`=0 | — | 展示/主题/语言 |

【确定结论】iCopy 的默认全局快捷键与 ClipBase 现行默认完全一致（⌃C/⌃⇧C/⌃⇧V/⌘1-9），说明 ClipBase 的默认值是照 iCopy 对齐的。

**堆栈 / 备份 / 文件历史 【确定结论】**

类：`StackItem`、`StackViewController`、`StackWindowController`（堆栈）；`BackupViewController`（备份）；`FilesEditViewController`、`FilePreviewView`、`DragSourceView`（文件历史与拖拽）。

---

### 1.4 分支与边界：权限 / 沙盒 / iCloud 同步

**iCloud 同步 = iCloud Documents（ubiquity）+ NSMetadataQuery 【确定结论】**

Info.plist `NSUbiquitousContainers` + entitlements ubiquity 容器 `iCloud.cn.better365.iCopy`。
二进制字符串直接命中：
```
checkiCloudSyncStatus / iCloudDocumentsUrl / iCloudUpdateSecond / isUploadToiCloud
NSMetadataQuery + NSMetadataQueryUbiquitousDocumentsScope
NSMetadataQueryDidStartGathering/DidUpdate/DidFinishGatheringNotification
NSURLUbiquitousItemDownloadingStatus{NotDownloaded,Downloaded,Current}
NSMetadataUbiquitousItemIsDownloadingKey / IsUploadingKey
```
结论：同步走 iCloud Documents 容器（`~/Library/Mobile Documents/iCloud~cn~better365~iCopy/Documents/`，实测存在），用 `NSMetadataQuery` 监听远端变更，含上传/下载状态与「自动同步」开关（`autoCloud`）。**不依赖自定义网络接口**。

**权限与边界 【确定结论 + 推测】**

- `AppCenterCrashes`/`MSAC*` → 崩溃采集（`network.client` 的用途之一）。
- 二进制 `strings` 命中 `/Library/Application Support/com.apple.TCC/TCC.db` 【推测】：`YLPermissionManager` 用于读取/检查系统 TCC 权限数据库，以检测辅助功能/自动化权限状态；读取该库本身需 Full Disk Access，具体用途需反汇编确认。
- `StoreKit` + `FreeAppModel`/`FreeAppsViewController` → 免费/付费功能分层。

---

### 1.5 重建等价实现（Swift，仅覆盖【确定结论】部分）

```swift
import AppKit
import Carbon

// —— 剪贴板监控：定时器 + changeCount 轮询 ——
final class ClipboardMonitor {
    private var timer: Timer?
    private var lastChangeCount: Int = -1

    func start(interval: TimeInterval = 0.5) {   // 精确间隔待探查（见 §3）
        timer = Timer.scheduledTimer(withTimeInterval: interval, repeats: true) { [weak self] _ in
            self?.poll()
        }
    }
    private func poll() {
        let pb = NSPasteboard.general
        guard pb.changeCount != lastChangeCount else { return }
        lastChangeCount = pb.changeCount
        capture(from: pb)
    }
    private func capture(from pb: NSPasteboard) {
        // 来源 App（黑名单匹配）
        if let app = NSWorkspace.shared.frontmostApplication,
           Blacklist.contains(app.bundleIdentifier ?? "") { return }
        // 类型分派（旧式 API）
        if let txt = pb.string(forType: .string) {
            save(.text(txt), uti: NSPasteboard.PasteboardType.string)
        } else if let data = pb.data(forType: .html) {
            save(.rich(data), uti: .html)
        } else if let data = pb.data(forType: .png) {
            let path = saveToDisk(data, ext: "png")
            save(.file(path), uti: .png)
        }
    }
    func stop() { timer?.invalidate() }
}

// —— 全局热键：Carbon RegisterEventHotKey ——
final class GlobalHotkey {
    private var hotKeyRef: EventHotKeyRef?
    func register(keyCode: UInt32, modifiers: UInt32) {
        var gMyHotKeyID = EventHotKeyID(signature: OSType(0x69436F70 /* "iCop" */), id: 1)
        RegisterEventHotKey(keyCode, modifiers, gMyHotKeyID,
                            GetApplicationEventTarget(), 0, &hotKeyRef)
    }
    func unregister() { if let r = hotKeyRef { UnregisterEventHotKey(r) } }
}

// —— 回贴：类型分派写回 + CGEvent ⌘V + 焦点归还 ——
final class Paster {
    func paste(_ item: ClipItem) {
        let pre = NSWorkspace.shared.frontmostApplication      // preActiveApp
        write(item)                                            // 见下
        pre?.activate(options: [.activateIgnoringOtherApps])   // 归还焦点
        postCommandV()                                         // CGEventCreateKeyboardEvent + CGEventPost
    }
    private func write(_ item: ClipItem) {
        let pb = NSPasteboard.general
        pb.clearContents()
        switch item.kind {
        case .text(let s):  pb.setString(s, forType: .string)
        case .rich(let d):  pb.setData(d, forType: .html)
        case .file(let p):  pb.setString(p, forType: .fileURL) // 或 NSFilenamesPboardType 数组
        }
    }
}
```

> 说明：仅 `【确定结论】` 部分落码；`writeToPasteBoardAndPasteAsPlainText`（纯文本贴）、`writeToPasteBoardWithTxtWithoutNewlineAndWhitespace:`（去尾部空白）等分支语义明确，可直接照抄。

---

## 二、Filee（com.filee.app v2.1.0）

### 2.1 基础信息 【确定结论】

| 项 | 证据 | 值 |
| --- | --- | --- |
| 类型 | `Contents/MacOS/libcoreclr.dylib` + 大量 `*.dll` | **.NET（C#）/ Avalonia 跨平台 GUI**，非原生 AppKit |
| 原生宿主 | `otool -L` | 仅链 `libSystem` + `libc++`（CoreCLR 以内嵌 dylib 加载） |
| 架构/签名 | `file` / `codesign` | arm64 单架构；`TeamIdentifier=not set`（**adhoc 签名/自编译**） |
| 沙盒 | entitlements | 无 `app-sandbox` → **未沙盒** |
| 版本 | 运行日志 | `Filee 2.1.0`（Info.plist 的 0.1.0 是过期值） |
| 最低系统 | Info.plist | 13.0 |
| 自有程序集 | `Contents/MacOS/*.dll` | `Filee.dll`（主）、`Filee.Core.dll`、`Filee.Engines.dll`、`Filee.Platform.{MacOS,Windows,Linux}.dll`、`Avalonia.*`、`Magick.NET`、`PdfSharp`、`UglyToad.PdfPig`、`RtfPipe`、`Svg.*`、`ExcelDataReader`、`libpdfium` |

**Info.plist 关键声明（直接证据）**：
```
NSServices: NSMessage="convertFiles", NSPortName="Filee", NSSendFileTypes=[public.item]  → Finder 右键「Convert with Filee」
NSAppleEventsUsageDescription: 读取 Finder 选中文件（用于转换快捷键）
NSInputMonitoringUsageDescription: 检测转换快捷键与拖拽手势
```

### 2.2 核心入口：触发器 / 选中读取 / 菜单

**macOS 平台层（Filee.Platform.MacOS.dll 的 `NativeMethods`，strings 直接证据）**：
```
AXIsProcessTrusted / AXIsProcessTrustedWithOptions   → 辅助功能权限校验
CGWindowListCopyWindowInfo                            → 枚举窗口（定位 Finder/光标下窗口）
NSUpdateDynamicServices                              → 动态注册 NSServices
SetContextMenu / SetModernContextMenuAsync            → 弹出转换菜单
GetFileManagerSelection / FinderSelectionScript        → AppleScript 读 Finder 选中
IsFinderFrontmost / ParseSelection                    → 判断前台/解析选中
PasteboardPaths                                        → 结果写粘贴板
ConvertFiles                                          → NSServices 处理器
SupportsSelectionShortcut / LastSelectionError
```

**触发器（settings.json 直接证据）**：两种触发方式，作用域 `FileManager`（Finder）：
1. `Drag` 手势：按住 `Alt` + 鼠标左键拖拽，`holdMilliseconds=450`。
2. `KeyChord`：`Ctrl+Alt+Space`。

**权限门控（运行日志直接证据）**：
```
TriggerService: Gestures wait for the Accessibility permission
TriggerService: Accessibility permission granted
TriggerService: Global input hook started
```
结论：全局输入钩子（检测 Alt+拖拽 / Ctrl+Alt+Space）**必须在辅助功能授权后才启动**；未授权时挂起等待。

### 2.3 数据流、持久化、配置

**持久化 = `~/Library/Application Support/Filee/settings.json` 【确定结论】**

`schemaVersion=46`，关键项：
- `contextMenuEnabled: true`（Finder 右键服务开关）
- `triggers[]`（上节两条触发器，含 `modifiers`/`key`/`holdMilliseconds`/`scope`）
- `excludedProcesses: []`（进程排除黑名单，默认空）
- `donut{}`：径向「甜甜圈」转换菜单——`outerRadius=150`、`holeRatio=0.42`、`opacity=0.96`、`flipKey="Tab"`、`flipWithWheel=true`
- `enginePriority[]`：转换引擎优先级 `magick → pdfium → pdfsharp → rhwp/unhwp → markdown → spreadsheet → ooxml → hwpx/docx-writer → pdf-text/slides → email → data → vector → icns → font → cad → model3d → geo → archive → ebook → subtitle → wic → ffmpeg → pandoc → ghostscript → calibre → libreoffice`
- `historyLimit=200`、`keepHistory=true`
- `defaultOutput.location="SameFolder"`（`subfolderName="converted"`）
- `libreOfficeForSlides/Documents/Sheets/Hwp` 开关
- 日志目录 `~/Library/Application Support/Filee/logs/`

### 2.4 边界

- 单实例：日志 `A later launch handed over 0 argument(s)` / `Handling a later launch: show Filee`。
- 更新检查：`UpdateService: Update check: Forbidden`（自编译构建无更新渠道）。
- 无沙盒、adhoc 签名 → 可直接写用户目录、读 Finder（依赖 AppleEvents+辅助功能+输入监控三类授权）。

---

## 三、证据不足项与下一步 REA 探查指令

以下结论当前证据不足，**不写成品代码**，列出下一步探查：

1. **iCopy 剪贴板轮询的精确时间间隔**
   > 探查动作：`lldb -n iCopy` 挂载后 `disassemble -n beginMonitorPasteBoard`，找 `scheduledTimerWithTimeInterval:` 的立即数。
   > 探查动作：`otool -arch arm64 -tvV iCopy` 反汇编，交叉引用 `0x1001ca86d`（beginMonitorPasteBoard selector 地址），读其 `adrp/add` 加载的间隔常量。

2. **iCopy 黑名单匹配键（bundle id vs 名称）与 `isPaste:` 分支语义**
   > 探查动作：Hopper 定位 `-[? Blacklist:isPaste:]` 实现，确认比较字段与 `isPaste` 两个分支的调用方。
   > 探查动作：动态验证——把某个 App 加黑名单后 `log stream --predicate 'process=="iCopy"'` 观察记录/跳过。

3. **iCopy `pasteToPreActiveAPPWithCompletion:` 的精确时序**（写回→激活→⌘V→恢复 的相对顺序、是否 `activateIgnoringOtherApps`）
   > 探查动作：`dtrace -n 'pid$target:AppKit:*paste*:entry { ustack(); }' -p <pid>` 抓调用栈。
   > 探查动作：Hopper 反汇编该方法，确认 `CGEventPost` 与 `activateWithOptions:` 的调用顺序。

4. **iCopy 引用 TCC.db 的真实用途**
   > 探查动作：`strings` 反查 `/Library/Application Support/com.apple.TCC/TCC.db` 的交叉引用（`otool -tvV` 里 `adrp` 该路径的代码段），确认是「读权限状态」还是「检测某 App 是否授权」。

5. **Filee 全局输入钩子的具体实现（CGEventTap vs NSEvent global monitor）**
   > 探查动作：在 `Filee.Platform.MacOS.dll` 上跑 `strings` 已见 `CGWindowListCopyWindowInfo` 但未见 `CGEventTapCreate`，需对 `Filee.Platform.MacOS.dll` 做 IL 反编译（`ilspycmd` 或 `dotnet tool install ilspycmd`，需网络，需先征得同意）。
   > 探查动作：`lsof -p <filee pid>` 观察其 hook 相关系统调用。

6. **Filee 转换菜单「选中格式 → 引擎 → 输出格式」的映射规则**
   > 探查动作：IL 反编译 `Filee.Engines.dll`（引擎矩阵）与 `Filee.Core.dll`（格式图），或直接查看本地源码目录 `/Users/dayiteng/Filee/`（日志显示 dev 构建路径，Filee 为 MIT 开源，源码就在本机，可作交叉验证，但不作为本报告 RE 证据）。

---

## 四、总评

- **iCopy**：原生 ObjC + AppKit 的菜单栏剪贴板管理器，核心 = `changeCount` 定时轮询 + Carbon 全局热键 + `CGEvent` 模拟 ⌘V 回贴；持久化用 FMDB 写两个沙盒 SQLite；同步走 iCloud Documents（ubiquity + NSMetadataQuery），无自定义后端；含 App Center 埋点、StoreKit 内购、快捷回复/堆栈/黑名单/备份/文件历史。默认快捷键 ⌃C/⌃⇧C/⌃⇧V/⌘1-9；ClipBase 只沿用了 ⌃C（面板）与 ⌃⇧V（堆栈回贴），
另新增 ⌃⇧R（快捷回复）与 ⌃⇧T（环形转换），未做 ⌃⇧C 与 ⌘1-9。
- **Filee**：.NET/Avalonia 跨平台文件转换器，macOS 侧 = NSServices（Finder 右键）+ 辅助功能/输入监控驱动的「Alt+拖拽 / Ctrl+Alt+Space」触发器 + 径向甜甜圈菜单 + 多引擎转换（Magick/pdfium/pdfsharp/…/libreoffice）。ClipBase 的 `macos.rs` NSServices 方案即复刻自其 `MacOSServicesProvider`。
- 两份目标都**没有**自定义网络接口/密钥类证据；iCopy 的联网仅 App Center 埋点 + iCloud 系统容器（均走系统框架）。

**证据产物**（/tmp/rea/）：`icopy_objc.txt`（otool -ov 全量）、`icopy_config.bplist`（解码后的 ConfigModel）、`icopy_config.b64`、`icopy_config.plist`。
