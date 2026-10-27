//! Finder 右键动作定义与平台无关的纯逻辑。
//!
//! 菜单本体由 Finder Sync 扩展提供（`src-tauri/finder-extension`）。扩展只把
//! 「动作 + Finder 给的路径」写进请求队列，主程序轮询后统一执行；因此快捷键、
//! 环形盘和右键菜单最终都走同一套建文件 / 开终端 / 环形转换代码。
//!
//! 本模块保留可单测的部分（目录解析、重名递增、AppleScript 转义、动作映射）；
//! macOS 专属的执行细节在 [`macos`]，扩展队列在 [`bridge`]。

use std::path::{Path, PathBuf};

use serde::Serialize;

#[cfg(target_os = "macos")]
mod bridge;
#[cfg(target_os = "macos")]
mod macos;

/// 新建文件的基名（与 macOS Finder 的「未命名」习惯一致）。
const UNTITLED_BASE: &str = "未命名";

/// Finder 扩展的 bundle id，与 `finder-extension/Info.plist` 保持一致。
pub const EXTENSION_BUNDLE_ID: &str = "com.clipbase.app.FinderExtension";

/// ClipBase 在 Finder 右键菜单里的 3 个动作。
///
/// `message()` 是扩展写入请求 JSON 的稳定协议值，不能随意改动。
///
/// Word / Excel / PowerPoint 不在这里：装了 WPS 的机器上原生「使用 WPS 新建」
/// 已经覆盖这三种文件，再放一份只会让菜单变长。
///
/// 环形转换**不**在这里：运行态只保留 Option＋左键拖动一个入口（总规范 §8.2）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceAction {
    /// 新建空文本文件（TextEdit 打开）。
    NewTextFile,
    /// 新建空 Markdown 文件（TextEdit 打开）。
    NewMarkdownFile,
    /// 在 Terminal 中进入目标目录。
    OpenInTerminal,
}

impl ServiceAction {
    /// 全部动作（顺序与扩展菜单一致）。
    pub const ALL: [ServiceAction; 3] = [
        ServiceAction::NewTextFile,
        ServiceAction::NewMarkdownFile,
        ServiceAction::OpenInTerminal,
    ];

    /// 扩展请求 JSON 的 `action` 值。
    pub fn message(&self) -> &'static str {
        match self {
            ServiceAction::NewTextFile => "newTextFile",
            ServiceAction::NewMarkdownFile => "newMarkdownFile",
            ServiceAction::OpenInTerminal => "openInTerminal",
        }
    }

    /// 面向用户的短标题（设置页、通知里用）。
    pub fn menu_title(&self) -> &'static str {
        match self {
            ServiceAction::NewTextFile => "新建文本文件",
            ServiceAction::NewMarkdownFile => "新建 Markdown 文件",
            ServiceAction::OpenInTerminal => "在终端中打开",
        }
    }

    /// 新建文件动作对应的扩展名；其它动作返回 `None`。
    pub fn file_extension(&self) -> Option<&'static str> {
        match self {
            ServiceAction::NewTextFile => Some("txt"),
            ServiceAction::NewMarkdownFile => Some("md"),
            _ => None,
        }
    }

    /// 从扩展请求的 `action` 字符串解析动作。
    pub fn from_message(value: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|action| action.message() == value)
    }
}

/// Finder 扩展的安装 / 启用状态（供设置页展示）。
#[derive(Debug, Clone, Serialize)]
pub struct FinderIntegrationStatus {
    /// 当前平台是否支持 Finder Sync（仅 macOS）。
    pub supported: bool,
    /// 系统扩展数据库里是否已有 ClipBase Finder 扩展。
    pub installed: bool,
    /// 扩展当前是否启用。macOS 15 起系统设置不再提供 Finder 扩展开关，
    /// 需要用 `pluginkit -e use` 启用，因此这里直接读真实状态。
    pub enabled: bool,
    /// 扩展的 bundle id。
    pub bundle_id: String,
    /// 当前 App 包里是否真的带有 `.appex`（开发构建通常没有）。
    pub appex_present: bool,
}

impl FinderIntegrationStatus {
    fn unsupported() -> Self {
        Self {
            supported: false,
            installed: false,
            enabled: false,
            bundle_id: EXTENSION_BUNDLE_ID.to_string(),
            appex_present: false,
        }
    }
}

/// 查询 Finder 扩展状态（无副作用）。
pub fn status() -> FinderIntegrationStatus {
    #[cfg(target_os = "macos")]
    {
        bridge::status()
    }
    #[cfg(not(target_os = "macos"))]
    {
        FinderIntegrationStatus::unsupported()
    }
}

/// 启用 / 停用 Finder 扩展。
///
/// 启用前会先确保扩展已注册（`pluginkit -a`）。这个操作只影响当前用户，
/// 不需要管理员权限；失败时把 `pluginkit` 的 stderr 原样带回设置页。
pub fn set_extension_enabled(enabled: bool) -> Result<FinderIntegrationStatus, String> {
    #[cfg(target_os = "macos")]
    {
        bridge::set_enabled(enabled)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = enabled;
        Ok(FinderIntegrationStatus::unsupported())
    }
}

/// 启动扩展请求轮询（幂等；重复调用会多一个轮询循环，因此只在 setup 调一次）。
pub fn start_extension_bridge(app: &tauri::AppHandle) {
    #[cfg(target_os = "macos")]
    {
        bridge::start(app.clone());
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
    }
}

/// 把动作直接落到指定目录（桌面空白处右键等「没有选中项」的入口）。
pub fn run_container_action(action: ServiceAction, container: &Path) {
    #[cfg(target_os = "macos")]
    {
        macos::run_action(action, &[], Some(container));
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (action, container);
    }
}

/// 从 Finder 选中项解析目标目录。
///
/// 规则：
/// - 选中文件夹 → 该文件夹；
/// - 选中文件 → 其父目录；
/// - 多选 → 取第一项的目录；
/// - 空列表 → `None`。
pub fn resolve_target_directory(selected: &[PathBuf]) -> Option<PathBuf> {
    let first = selected.first()?;
    if first.is_dir() {
        return Some(first.clone());
    }
    let parent = first.parent()?;
    if parent.as_os_str().is_empty() {
        return None;
    }
    Some(parent.to_path_buf())
}

/// 在 `directory` 下生成不冲突的「未命名.`extension`」路径。
///
/// 已存在时按 macOS Finder 习惯递增：「未命名 2」「未命名 3」……
pub fn unique_file_path(directory: &Path, extension: &str) -> PathBuf {
    let first = directory.join(format!("{UNTITLED_BASE}.{extension}"));
    if !first.exists() {
        return first;
    }
    let mut index: u32 = 2;
    loop {
        let candidate = directory.join(format!("{UNTITLED_BASE} {index}.{extension}"));
        if !candidate.exists() {
            return candidate;
        }
        index += 1;
    }
}

/// 转义 AppleScript 双引号字符串内容：反斜杠与双引号。
pub fn escape_applescript_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

/// 包装为 AppleScript 双引号字符串字面量（含转义）。
pub fn applescript_string_literal(value: &str) -> String {
    format!("\"{}\"", escape_applescript_string(value))
}

/// 生成「在 Terminal 中进入目录」的 AppleScript（`cd` 到目标目录）。
pub fn terminal_cd_script(directory: &Path) -> String {
    format!(
        "tell application \"Terminal\" to do script \"cd {}\"",
        escape_applescript_string(&directory.to_string_lossy())
    )
}

/// 系统通知（跨平台入口）：macOS 用 `osascript display notification`，其余平台仅记日志。
pub fn notify(title: &str, message: &str) {
    #[cfg(target_os = "macos")]
    {
        let script = format!(
            "display notification {} with title {}",
            applescript_string_literal(message),
            applescript_string_literal(title)
        );
        if let Err(err) = std::process::Command::new("osascript")
            .arg("-e")
            .arg(&script)
            .status()
        {
            tracing::debug!("发送系统通知失败: {err}");
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        tracing::info!("通知：{title} - {message}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "clipbase-services-{}-{name}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create scratch dir");
        dir
    }

    #[test]
    fn unique_file_path_avoids_existing_names() {
        let dir = scratch_dir("unique");
        let first = unique_file_path(&dir, "txt");
        assert_eq!(first, dir.join("未命名.txt"));
        fs::File::create(&first).expect("create first");

        let second = unique_file_path(&dir, "txt");
        assert_eq!(second, dir.join("未命名 2.txt"));
        fs::File::create(&second).expect("create second");

        let third = unique_file_path(&dir, "txt");
        assert_eq!(third, dir.join("未命名 3.txt"));

        let markdown = unique_file_path(&dir, "md");
        assert_eq!(markdown, dir.join("未命名.md"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolve_target_directory_prefers_folder_then_parent() {
        let dir = scratch_dir("resolve");
        let folder = dir.join("Projects");
        let file = dir.join("notes.txt");
        fs::create_dir_all(&folder).expect("create folder");
        fs::File::create(&file).expect("create file");

        assert_eq!(
            resolve_target_directory(std::slice::from_ref(&folder)),
            Some(folder.clone())
        );
        assert_eq!(
            resolve_target_directory(std::slice::from_ref(&file)),
            Some(dir.clone())
        );
        assert_eq!(
            resolve_target_directory(&[file.clone(), folder.clone()]),
            Some(dir.clone())
        );
        assert_eq!(resolve_target_directory(&[]), None);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn escape_applescript_string_handles_quotes_and_backslashes() {
        assert_eq!(escape_applescript_string("/tmp/plain"), "/tmp/plain");
        assert_eq!(escape_applescript_string("/tmp/it's"), "/tmp/it's");
        assert_eq!(escape_applescript_string("/tmp/a\"b"), "/tmp/a\\\"b");
        assert_eq!(escape_applescript_string("/tmp/a\\b"), "/tmp/a\\\\b");
        assert_eq!(escape_applescript_string("/tmp/\"x"), "/tmp/\\\"x");
    }

    #[test]
    fn terminal_cd_script_escapes_directory() {
        let script = terminal_cd_script(Path::new("/tmp/My \"Docs\""));
        assert_eq!(
            script,
            "tell application \"Terminal\" to do script \"cd /tmp/My \\\"Docs\\\"\""
        );
        let plain = terminal_cd_script(Path::new("/Users/demo/Desktop"));
        assert_eq!(
            plain,
            "tell application \"Terminal\" to do script \"cd /Users/demo/Desktop\""
        );
    }

    #[test]
    fn action_protocol_matches_extension_strings() {
        // 这些字符串同时出现在 Swift 的 Action enum 里，改动必须两边同步。
        let expected = ["newTextFile", "newMarkdownFile", "openInTerminal"];
        let actual: Vec<&str> = ServiceAction::ALL
            .iter()
            .map(ServiceAction::message)
            .collect();
        assert_eq!(actual, expected);

        for action in ServiceAction::ALL {
            assert_eq!(ServiceAction::from_message(action.message()), Some(action));
        }
        assert_eq!(ServiceAction::from_message("newWpsWriter"), None);
    }

    #[test]
    fn new_file_actions_map_to_extensions() {
        assert_eq!(ServiceAction::NewTextFile.file_extension(), Some("txt"));
        assert_eq!(ServiceAction::NewMarkdownFile.file_extension(), Some("md"));
        assert_eq!(ServiceAction::OpenInTerminal.file_extension(), None);
    }

    #[test]
    fn menu_titles_are_unique_and_non_empty() {
        let mut titles: Vec<&str> = ServiceAction::ALL
            .iter()
            .map(ServiceAction::menu_title)
            .collect();
        titles.sort_unstable();
        titles.dedup();
        assert_eq!(titles.len(), ServiceAction::ALL.len());
        assert!(titles.iter().all(|title| !title.is_empty()));
    }

    #[test]
    fn unsupported_platform_status_is_explicit() {
        let status = FinderIntegrationStatus::unsupported();
        assert!(!status.supported);
        assert!(!status.installed);
        assert!(!status.enabled);
        assert!(!status.appex_present);
        assert_eq!(status.bundle_id, EXTENSION_BUNDLE_ID);
    }
}
