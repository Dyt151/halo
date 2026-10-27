import Cocoa
import FinderSync

/// Finder 扩展只做三件事：出菜单、把动作和目标路径写进请求队列、唤醒主程序。
///
/// 真正的文件创建 / 开终端都在主程序里执行。扩展本身是沙盒进程，
/// 无法写任意目录，也不应该承担这些动作；这样只有一条执行路径，行为不会漂移。
/// 环形转换**不进右键菜单**：它的唯一入口是 Finder 里 Option＋左键拖动。

private let extensionBundleIdentifier = "com.clipbase.app.FinderExtension"
private let appGroupIdentifier = "group.com.clipbase.app"
private let wakeURLString = "clipbase-finder://wake"
private let queueFolderName = "ClipBaseFinder/Requests"

// 动作名必须与 Rust `ServiceAction::message()` 一致。
private enum Action: String {
    case newTextFile
    case newMarkdownFile
    case openInTerminal
}

/// Info.plist 的 `NSExtensionPrincipalClass` 指向这个 ObjC 名。
@objc(ClipBaseFinderSync)
final class ClipBaseFinderSync: FIFinderSync {
    private let fileManager = FileManager.default

    override init() {
        super.init()
        let monitored = monitoredDirectories()
        FIFinderSyncController.default().directoryURLs = monitored
        log("started from \(Bundle.main.bundlePath); monitoring \(monitored.map(\.path).sorted().joined(separator: ", "))")
    }

    /// 菜单出现的范围。Finder Sync 不会对未监视的目录调用 `menu(for:)`；
    /// 用户的主目录已经覆盖桌面、下载、文稿与普通 Finder 窗口，
    /// `/Volumes` 覆盖外接盘。不监视 `/`：那会让 Finder 在每个系统目录里都
    /// 唤醒扩展，收益很低。
    ///
    /// 沙盒里的 `NSHomeDirectory()` 会返回扩展自己的容器，必须用
    /// `getpwuid` 取真实主目录，否则监视的是容器、真实目录永远不出菜单。
    private func monitoredDirectories() -> Set<URL> {
        let home = realHomeDirectory()
        var urls: Set<URL> = [
            home,
            // 桌面是 Finder 的特殊容器，不会因为监视了主目录就自动带上。
            home.appendingPathComponent("Desktop", isDirectory: true),
            URL(fileURLWithPath: "/Volumes", isDirectory: true),
        ]
        // 目录不存在时不要塞进去，否则 Finder 会一直报监视失败。
        urls = urls.filter { fileManager.fileExists(atPath: $0.path) }
        return urls
    }

    private func realHomeDirectory() -> URL {
        if let entry = getpwuid(getuid()), let dir = entry.pointee.pw_dir {
            return URL(fileURLWithPath: String(cString: dir), isDirectory: true)
        }
        return URL(fileURLWithPath: NSHomeDirectory(), isDirectory: true)
    }

    override func menu(for menuKind: FIMenuKind) -> NSMenu? {
        switch menuKind {
        case .contextualMenuForItems:
            let menu = buildMenu()
            log("menu kind=\(menuKind.rawValue) items=\(menu.items.filter { !$0.isSeparatorItem }.count) selected=\(selectedPaths().count) target=\(FIFinderSyncController.default().targetedURL()?.path ?? "-")")
            return menu
        case .contextualMenuForContainer:
            let menu = buildMenu()
            log("menu kind=\(menuKind.rawValue) items=\(menu.items.filter { !$0.isSeparatorItem }.count) target=\(FIFinderSyncController.default().targetedURL()?.path ?? "-")")
            return menu
        default:
            log("menu kind=\(menuKind.rawValue) ignored")
            return nil
        }
    }

    private func buildMenu() -> NSMenu {
        let menu = NSMenu(title: "ClipBase")
        menu.autoenablesItems = false

        // Finder 对二级菜单项的 action/representedObject 转发不可靠，所以把
        // 「新建」拍平，和 Windows 右键一样一眼直达。
        addItem(to: menu, title: "新建文本文件 (.txt)", selector: #selector(createTextFile(_:)), symbol: "doc.text")
        addItem(to: menu, title: "新建 Markdown 文件 (.md)", selector: #selector(createMarkdownFile(_:)), symbol: "doc.plaintext")

        menu.addItem(.separator())
        addItem(to: menu, title: "在终端中打开", selector: #selector(openInTerminal(_:)), symbol: "terminal")

        // 环形转换不放进右键菜单：运行态唯一入口是 Finder 里 Option＋左键拖动。
        return menu
    }

    /// 每个菜单项挂自己的 selector。
    ///
    /// Finder 把菜单复制进自己的进程再回放动作，`representedObject` 会在
    /// 这趟转发里丢掉（实测点下去拿到的是 nil，表现为「点了没反应」）。
    /// selector 本身是必达的，所以把动作编码进方法名，而不是塞在 item 上。
    private func addItem(to menu: NSMenu, title: String, selector: Selector, symbol: String) {
        let item = NSMenuItem(title: title, action: selector, keyEquivalent: "")
        item.target = self
        item.isEnabled = true
        if let image = NSImage(systemSymbolName: symbol, accessibilityDescription: nil) {
            item.image = image
        }
        menu.addItem(item)
    }

    @objc private func createTextFile(_ sender: NSMenuItem) {
        run(.newTextFile)
    }

    @objc private func createMarkdownFile(_ sender: NSMenuItem) {
        run(.newMarkdownFile)
    }

    @objc private func openInTerminal(_ sender: NSMenuItem) {
        run(.openInTerminal)
    }

    private func run(_ action: Action) {
        // 这两个值只在菜单构建 / 菜单动作期间有效，动作里必须重新读。
        let controller = FIFinderSyncController.default()
        let paths = selectedPaths()
        let container = controller.targetedURL()?.path

        // 空白处右键时目标就是当前目录；选中文件/文件夹时交给主程序按
        // 「文件夹取自己、文件取父目录」的规则解析，扩展不碰文件系统。
        enqueue(action: action, paths: paths, container: container)
    }

    private func selectedPaths() -> [String] {
        FIFinderSyncController.default().selectedItemURLs()?.map(\.path) ?? []
    }

    private func enqueue(action: Action, paths: [String], container: String?) {
        guard let directory = requestDirectory() else {
            log("no writable queue directory for action \(action.rawValue)")
            return
        }

        let payload: [String: Any] = [
            "action": action.rawValue,
            "paths": paths,
            "container": container ?? "",
            "created_at": Int(Date().timeIntervalSince1970 * 1000),
        ]

        do {
            let data = try JSONSerialization.data(withJSONObject: payload, options: [])
            let name = "\(Int(Date().timeIntervalSince1970 * 1000))-\(UUID().uuidString).json"
            let url = directory.appendingPathComponent(name)
            // .atomic 先写临时文件再改名，主程序的轮询不会读到半截 JSON。
            try data.write(to: url, options: .atomic)
            log("queued \(action.rawValue) paths=\(paths.count) container=\(container ?? "-")")
        } catch {
            log("queue write failed: \(error.localizedDescription)")
            return
        }

        wakeMainApp()
    }

    /// 扩展是沙盒进程，不能直接 `openApplication`；自定义 URL scheme 由
    /// LaunchServices 唤醒主程序。主程序本来就在常驻时，这一步只是让轮询
    /// 立刻被触发；即使失败，轮询仍会取走请求。
    private func wakeMainApp() {
        guard let url = URL(string: wakeURLString) else { return }
        if !NSWorkspace.shared.open(url) {
            log("wake URL rejected; request will wait for the poller")
        }
    }

    /// 优先 App Group（正式签名时可用），否则退回扩展自己的沙盒容器。
    /// 两者主程序都能读；没有 App Group entitlement 的本地 ad-hoc 构建
    /// 会走后者，不影响功能。
    private func requestDirectory() -> URL? {
        if let group = fileManager.containerURL(forSecurityApplicationGroupIdentifier: appGroupIdentifier) {
            let dir = group.appendingPathComponent("FinderRequests", isDirectory: true)
            if ensureDirectory(dir) { return dir }
        }
        if let support = fileManager.urls(for: .applicationSupportDirectory, in: .userDomainMask).first {
            let dir = support.appendingPathComponent(queueFolderName, isDirectory: true)
            if ensureDirectory(dir) { return dir }
        }
        return nil
    }

    private func ensureDirectory(_ url: URL) -> Bool {
        do {
            try fileManager.createDirectory(at: url, withIntermediateDirectories: true)
            return true
        } catch {
            log("create queue directory failed at \(url.path): \(error.localizedDescription)")
            return false
        }
    }

    private func log(_ message: String) {
        NSLog("ClipBaseFinder[\(extensionBundleIdentifier)]: %@", message)
        guard let support = fileManager.urls(for: .applicationSupportDirectory, in: .userDomainMask).first else {
            return
        }
        let dir = support.appendingPathComponent("ClipBaseFinder", isDirectory: true)
        try? fileManager.createDirectory(at: dir, withIntermediateDirectories: true)
        let file = dir.appendingPathComponent("finder.log")

        // 日志只用于排查扩展是否被 Finder 唤起；限制大小，避免无限增长。
        if let attrs = try? fileManager.attributesOfItem(atPath: file.path),
           let size = attrs[.size] as? NSNumber,
           size.intValue > 512 * 1024 {
            try? fileManager.removeItem(at: file)
        }

        let line = "\(ISO8601DateFormatter().string(from: Date())) \(message)\n"
        guard let data = line.data(using: .utf8) else { return }
        if fileManager.fileExists(atPath: file.path),
           let handle = try? FileHandle(forWritingTo: file) {
            defer { try? handle.close() }
            _ = try? handle.seekToEnd()
            try? handle.write(contentsOf: data)
        } else {
            try? data.write(to: file, options: .atomic)
        }
    }
}
