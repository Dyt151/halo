//! Finder 选中项读取（独立 provider，**不经过系统剪贴板**）。
//!
//! 为什么必须单独实现（评审 A5 / 任务 11）：现有 NSServices 回调里的选中项来自
//! Finder 传入的 `NSPasteboard`，那个 pasteboard 只在服务回调期间存在；全局快捷键
//! 没有这个参数。若图省事改读普通剪贴板，拿到的是「上一次复制的内容」而不是
//! 「当前选中的文件」，会静默转换错误的文件。
//!
//! 技术选择：AppleScript `tell application "Finder" to get selection`。
//! 代价是首次调用需要用户授予「控制 Finder」的自动化权限，因此这里把失败原因
//! 拆成明确状态（空选择 / 非 Finder / 权限拒绝 / 超时 / 读取失败），
//! 而不是统统压成空数组（评审 D4 / 任务 17）。
//!
//! 前台 App 判定走 `NSWorkspace.frontmostApplication`（无需额外权限），
//! 不用 `System Events`，避免再多弹一次自动化授权。

use serde::Serialize;

#[cfg(target_os = "macos")]
mod macos;

/// Finder 选中项读取结果。
///
/// 序列化为前端可读的 `state` 字符串 + 可选提示，状态机见 `crate::core::ring`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FinderSelection {
    /// 拿到至少一个选中项（绝对路径）。
    Items(Vec<String>),
    /// Finder 在最前台，但没有选中任何项。
    Empty,
    /// 当前前台不是 Finder（环形转换只在 Finder 现场提供）。
    NotFinder,
    /// 自动化权限被拒绝（TCC -1743）：必须和「真的没选文件」区分开。
    PermissionDenied,
    /// Apple 事件超时（Finder 无响应 / 正在忙）。
    Timeout,
    /// 其它读取失败（Finder 未运行、脚本报错等）。
    Failed(String),
    /// 当前平台不支持。
    Unsupported,
}

impl FinderSelection {
    /// 状态标识（前端按它分流提示）。
    pub fn state(&self) -> &'static str {
        match self {
            Self::Items(_) => "ready",
            Self::Empty => "empty",
            Self::NotFinder => "not_finder",
            Self::PermissionDenied => "permission_denied",
            Self::Timeout => "timeout",
            Self::Failed(_) => "failed",
            Self::Unsupported => "unsupported",
        }
    }

    /// 面向用户的提示文案（任务 17：每种失败都要能自我解释）。
    pub fn hint(&self) -> Option<String> {
        match self {
            Self::Items(_) => None,
            Self::Empty => Some("Finder 里没有选中文件，先在 Finder 选中要转换的文件。".into()),
            Self::NotFinder => {
                Some("请先切到 Finder 窗口并选中文件，再按快捷键。".into())
            }
            Self::PermissionDenied => Some(
                "没有「控制 Finder」的自动化权限。到 系统设置 → 隐私与安全性 → 自动化 里允许 Halo 控制 Finder。"
                    .into(),
            ),
            Self::Timeout => Some("Finder 没有响应，稍后再试一次。".into()),
            Self::Failed(detail) => Some(format!("读取 Finder 选中项失败：{detail}")),
            Self::Unsupported => Some("环形转换目前只在 macOS 的 Finder 中可用。".into()),
        }
    }

    /// 选中项路径（没有则空）。
    pub fn paths(&self) -> Vec<String> {
        match self {
            Self::Items(paths) => paths.clone(),
            _ => Vec::new(),
        }
    }
}

/// 前端用的读取结果快照（`Items` 只带数量，路径留给会话快照）。
#[derive(Debug, Clone, Serialize)]
pub struct SelectionReport {
    pub state: String,
    pub count: usize,
    pub hint: Option<String>,
}

impl From<&FinderSelection> for SelectionReport {
    fn from(value: &FinderSelection) -> Self {
        Self {
            state: value.state().to_string(),
            count: value.paths().len(),
            hint: value.hint(),
        }
    }
}

/// 读取当前 Finder 的选中项（阻塞调用，必须在工作线程执行）。
pub fn read_selection() -> FinderSelection {
    #[cfg(target_os = "macos")]
    {
        macos::read_selection()
    }
    #[cfg(not(target_os = "macos"))]
    {
        FinderSelection::Unsupported
    }
}

/// 前台 App 是否为 Finder（不弹权限，读 NSWorkspace）。
pub fn frontmost_is_finder() -> bool {
    #[cfg(target_os = "macos")]
    {
        macos::frontmost_is_finder()
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

/// 环形转换是否可以在当前现场唤出。
///
/// 允许两种前台情形：① Finder（正常路径）；② Halo 自己
///（环形盘/回复选择器拿到焦点后再次按快捷键）。后者不能判成「非 Finder」，
/// 否则用户按第二次快捷键只会收到一句答非所问的提示。
pub fn ring_context_ok() -> bool {
    #[cfg(target_os = "macos")]
    {
        macos::frontmost_is_finder() || macos::frontmost_is_self()
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

/// 起手点取证结果（Filee 行为：拖动内容必须包含文件或文件夹才进入会话）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartHit {
    /// 指针下就是 Finder 里的文件或文件夹（目录只保留打包等工具动作）。
    File(String),
    /// 指针在 Finder 里，但不是文件（空白处、工具栏、侧边栏）。
    ///
    /// 与 `Unknown` 分开是为了守住验收项「非文件拖动不显示圆环」：这一种是
    /// **确定不是文件**，不应该弹一张「没读到文件」的提示盘挡在屏幕中间。
    NotFile,
    /// 拿不到元素（AX 超时 / 别的窗口盖着 / 前台不是 Finder）。
    Unknown,
}

/// 屏幕坐标下的 Finder 起手文件（Option＋拖动时的权威载荷）。
///
/// 同步调用（毫秒级，带 0.2s AX 超时），必须在 `leftMouseDown` 回调里用：
/// Finder 一进事件追踪循环就不再响应 AX，之后再读会阻塞到松手。
pub fn start_hit(x: f64, y: f64) -> StartHit {
    #[cfg(target_os = "macos")]
    {
        macos::start_hit(x, y)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (x, y);
        StartHit::Unknown
    }
}

/// 拆分 AppleScript 返回的路径串。
///
/// 分隔符是 ASCII Unit Separator（`0x1F`），文件名里几乎不可能出现；
/// 换行符不能用作分隔符，因为 macOS 允许文件名含换行。
/// 末尾可能残留一个分隔符，需丢弃空片段。
pub fn split_paths(output: &str) -> Vec<String> {
    output
        .split('\u{1f}')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(ToString::to_string)
        .collect()
}

/// osascript 的 stderr 是否表示「自动化权限被拒」。
///
/// `-1743` = errAEEventNotPermitted（TCC 拒绝）；部分系统只给出文案不给错误码，
/// 因此同时匹配两种形式。
pub fn is_permission_error(stderr: &str) -> bool {
    let lower = stderr.to_ascii_lowercase();
    lower.contains("-1743")
        || lower.contains("not authorized")
        || lower.contains("未经授权")
        || lower.contains("不允许")
}

/// stderr 是否表示 Apple 事件超时（`-1712` = errAETimeout）。
pub fn is_timeout_error(stderr: &str) -> bool {
    let lower = stderr.to_ascii_lowercase();
    lower.contains("-1712") || lower.contains("event timed out") || lower.contains("已超时")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_paths_uses_unit_separator_and_drops_empties() {
        let raw = "/Users/demo/a.png\u{1f}/Users/demo/b folder/c.jpg\u{1f}";
        assert_eq!(
            split_paths(raw),
            vec![
                "/Users/demo/a.png".to_string(),
                "/Users/demo/b folder/c.jpg".to_string()
            ]
        );
        assert!(split_paths("").is_empty());
        assert!(split_paths("\u{1f}\u{1f}").is_empty());
    }

    #[test]
    fn split_paths_keeps_newline_inside_a_file_name() {
        // 文件名含换行时不能被误当成两条记录。
        let raw = "/Users/demo/weird\nname.png";
        assert_eq!(
            split_paths(raw),
            vec!["/Users/demo/weird\nname.png".to_string()]
        );
    }

    #[test]
    fn permission_and_timeout_are_recognised() {
        assert!(is_permission_error(
            "osascript: execution error:  Unauthorized Access -1743"
        ));
        assert!(is_permission_error(
            "“Halo”遇到错误：应用程序没有运行。-1743"
        ));
        assert!(!is_permission_error("syntax error"));
        assert!(is_timeout_error("AppleEvent已超时。-1712"));
        assert!(!is_timeout_error("-1743"));
    }

    #[test]
    fn selection_states_and_hints_are_distinct() {
        assert_eq!(FinderSelection::Empty.state(), "empty");
        assert_eq!(FinderSelection::NotFinder.state(), "not_finder");
        assert_eq!(
            FinderSelection::PermissionDenied.state(),
            "permission_denied"
        );
        assert_eq!(FinderSelection::Timeout.state(), "timeout");
        // 只有 Items 才带路径，其余状态的 hint 必须非空（任务 17）。
        for s in [
            FinderSelection::Empty,
            FinderSelection::NotFinder,
            FinderSelection::PermissionDenied,
            FinderSelection::Timeout,
            FinderSelection::Failed("x".into()),
            FinderSelection::Unsupported,
        ] {
            assert_eq!(s.paths().len(), 0);
            assert!(s.hint().is_some(), "{:?} 缺少提示文案", s);
        }
        let items = FinderSelection::Items(vec!["/tmp/a.png".into()]);
        assert_eq!(items.state(), "ready");
        assert!(items.hint().is_none());
        assert_eq!(items.paths(), vec!["/tmp/a.png".to_string()]);
    }
}
