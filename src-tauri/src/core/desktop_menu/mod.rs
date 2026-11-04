//! 桌面右键菜单。
//!
//! Finder Sync 扩展负责 Finder 窗口内的右键；桌面这个特殊容器不会把扩展返回
//! 的菜单项注入可见菜单（实测 macOS 26 只在 `menu(for:)` 里被调用，桌面仍然
//! 弹系统菜单）。因此桌面由主程序用 `CGEventTap` 接管：
//!
//! - 空白处：新建文件 / 新建文件夹 / 终端 / 粘贴 / 更改桌面背景；
//! - 桌面图标上：打开 / 新建文件 / 新建文件夹 / 终端 / 显示简介 / 移到废纸篓。
//!
//! 两种上下文都保留「显示 Finder 菜单」兜底：把这次右键原样重投给 Finder，
//! 用户仍能拿到「排序方式 / 查看显示选项 / 使用叠放」等原生菜单。
//!
//! 这里的文件操作与平台无关，macOS 专属的事件监听 / 菜单绘制在 [`macos`]。

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::core::services::ServiceAction;

#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "macos")]
pub use macos::{init, set_enabled};

#[cfg(not(target_os = "macos"))]
pub fn init(_app: &tauri::AppHandle) {}

#[cfg(not(target_os = "macos"))]
pub fn set_enabled(_enabled: bool) {}

/// 新建文件夹的基名（与 Finder 的「未命名文件夹」一致）。
const FOLDER_BASE: &str = "未命名文件夹";
/// 粘贴重名时的后缀（与 Finder 的「副本」一致）。
const COPY_SUFFIX: &str = "副本";

/// 桌面菜单里的一个动作。
///
/// `tag` 会写进 `NSMenuItem.tag`，菜单回调据此分发，改动必须保持稳定。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DesktopAction {
    NewTextFile,
    NewMarkdownFile,
    NewFolder,
    OpenInTerminal,
    Paste,
    Wallpaper,
    ShowFinderMenu,
    /// 打开被点中的桌面项目。
    Open,
    /// Finder 的「显示简介」。
    ShowInfo,
    /// 移到废纸篓（可撤销，等价于 Finder 原生行为）。
    MoveToTrash,
}

/// 这次右键落在哪里：桌面空白处，还是具体项目上。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MenuContext {
    Blank,
    /// 被点中的桌面项目（多选时可能不止一个）。
    Items(Vec<PathBuf>),
}

impl MenuContext {
    /// 该上下文对应的菜单项（顺序即显示顺序，分组见 [`DesktopAction::separator_before`]）。
    pub(crate) fn actions(&self) -> Vec<DesktopAction> {
        let mut actions = match self {
            MenuContext::Blank => NEW_FILE_ACTIONS.to_vec(),
            MenuContext::Items(_) => {
                let mut actions = vec![DesktopAction::Open];
                actions.extend(NEW_FILE_ACTIONS);
                actions
            }
        };
        match self {
            MenuContext::Blank => actions.extend([
                DesktopAction::NewFolder,
                DesktopAction::OpenInTerminal,
                DesktopAction::Paste,
                DesktopAction::Wallpaper,
                DesktopAction::ShowFinderMenu,
            ]),
            MenuContext::Items(_) => actions.extend([
                DesktopAction::NewFolder,
                DesktopAction::OpenInTerminal,
                DesktopAction::ShowInfo,
                DesktopAction::MoveToTrash,
                DesktopAction::ShowFinderMenu,
            ]),
        }
        actions
    }

    /// 新建文件 / 开终端的目标目录：空白处取桌面，项目取其目录。
    pub(crate) fn target_directory(&self) -> Option<PathBuf> {
        match self {
            MenuContext::Blank => desktop_directory(),
            MenuContext::Items(paths) => {
                crate::core::services::resolve_target_directory(paths)
            }
        }
    }

    /// 可以被环形盘转换的项目（文件夹不参与）。
    pub(crate) fn convertible_paths(&self) -> Vec<PathBuf> {
        match self {
            MenuContext::Blank => Vec::new(),
            MenuContext::Items(paths) => paths
                .iter()
                .filter(|path| path.is_file())
                .cloned()
                .collect(),
        }
    }

    pub(crate) fn items(&self) -> &[PathBuf] {
        match self {
            MenuContext::Blank => &[],
            MenuContext::Items(paths) => paths,
        }
    }
}

/// 用户桌面目录（目录名被本地化时也能拿到真实路径）。
pub(crate) fn desktop_directory() -> Option<PathBuf> {
    dirs::desktop_dir().or_else(|| dirs::home_dir().map(|home| home.join("Desktop")))
}

/// 新建文件组；两种上下文都显示。
///
/// Word / Excel / PowerPoint 不放在这里：WPS 的「使用 WPS 新建」已经提供，
/// 再列一份只会让菜单变长。
pub(crate) const NEW_FILE_ACTIONS: [DesktopAction; 2] = [
    DesktopAction::NewTextFile,
    DesktopAction::NewMarkdownFile,
];

/// 全部动作（tag 唯一性测试与 `from_tag` 用）。
pub(crate) const ALL_ACTIONS: [DesktopAction; 10] = [
    DesktopAction::NewTextFile,
    DesktopAction::NewMarkdownFile,
    DesktopAction::NewFolder,
    DesktopAction::OpenInTerminal,
    DesktopAction::Paste,
    DesktopAction::Wallpaper,
    DesktopAction::ShowFinderMenu,
    DesktopAction::Open,
    DesktopAction::ShowInfo,
    DesktopAction::MoveToTrash,
];

impl DesktopAction {
    pub(crate) fn tag(self) -> isize {
        match self {
            DesktopAction::NewTextFile => 1,
            DesktopAction::NewMarkdownFile => 2,
            DesktopAction::NewFolder => 3,
            DesktopAction::OpenInTerminal => 4,
            DesktopAction::Paste => 5,
            DesktopAction::Wallpaper => 6,
            DesktopAction::ShowFinderMenu => 7,
            DesktopAction::Open => 8,
            DesktopAction::ShowInfo => 9,
            DesktopAction::MoveToTrash => 10,
        }
    }

    pub(crate) fn from_tag(tag: isize) -> Option<Self> {
        ALL_ACTIONS.iter().copied().find(|action| action.tag() == tag)
    }

    pub(crate) fn title(self) -> &'static str {
        match self {
            DesktopAction::NewTextFile => "新建文本文件 (.txt)",
            DesktopAction::NewMarkdownFile => "新建 Markdown 文件 (.md)",
            DesktopAction::NewFolder => "新建文件夹",
            DesktopAction::OpenInTerminal => "在终端中打开",
            DesktopAction::Paste => "粘贴项目",
            DesktopAction::Wallpaper => "更改桌面背景…",
            DesktopAction::ShowFinderMenu => "显示 Finder 菜单",
            DesktopAction::Open => "打开",
            DesktopAction::ShowInfo => "显示简介",
            DesktopAction::MoveToTrash => "移到废纸篓",
        }
    }

    /// SF Symbol 名称；取不到图标时菜单项照常显示，只是没有图标。
    pub(crate) fn symbol(self) -> &'static str {
        match self {
            DesktopAction::NewTextFile => "doc.text",
            DesktopAction::NewMarkdownFile => "doc.plaintext",
            DesktopAction::NewFolder => "folder",
            DesktopAction::OpenInTerminal => "terminal",
            DesktopAction::Paste => "doc.on.clipboard",
            DesktopAction::Wallpaper => "photo",
            DesktopAction::ShowFinderMenu => "contextualmenu.and.cursorarrow",
            DesktopAction::Open => "arrow.up.forward.app",
            DesktopAction::ShowInfo => "info.circle",
            DesktopAction::MoveToTrash => "trash",
        }
    }

    /// 该菜单项前面是否要有分隔线。
    ///
    /// `NewTextFile` 把「打开」与「新建文件」分开（空白处它是第一项，调用方会
    /// 跳过开头的分隔线），`NewFolder` 开始目录操作组，`ShowInfo` 开始项目操作组，
    /// `Wallpaper` 开始系统设置组，`ShowFinderMenu` 永远是最后一组。
    pub(crate) fn separator_before(self) -> bool {
        matches!(
            self,
            DesktopAction::NewTextFile
                | DesktopAction::NewFolder
                | DesktopAction::ShowInfo
                | DesktopAction::Wallpaper
                | DesktopAction::ShowFinderMenu
        )
    }

    /// 复用 Finder 右键动作的执行器；桌面专属动作返回 `None`。
    pub(crate) fn service_action(self) -> Option<ServiceAction> {
        match self {
            DesktopAction::NewTextFile => Some(ServiceAction::NewTextFile),
            DesktopAction::NewMarkdownFile => Some(ServiceAction::NewMarkdownFile),
            DesktopAction::OpenInTerminal => Some(ServiceAction::OpenInTerminal),
            _ => None,
        }
    }
}

/// 在桌面新建「未命名文件夹」（重名按 Finder 习惯加序号）。
pub(crate) fn create_folder(directory: &Path) -> io::Result<PathBuf> {
    for index in 0..1000 {
        let name = if index == 0 {
            FOLDER_BASE.to_string()
        } else {
            format!("{FOLDER_BASE} {}", index + 1)
        };
        let path = directory.join(name);
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "桌面同名文件夹太多，无法生成新名字",
    ))
}

/// 把剪贴板里的文件 / 文件夹复制到目标目录，重名时加「副本」。
///
/// 返回每个来源的结果；单个来源失败不影响其它来源。
pub(crate) fn paste_into(directory: &Path, sources: &[PathBuf]) -> Vec<io::Result<PathBuf>> {
    sources
        .iter()
        .map(|source| {
            let Some(name) = source.file_name() else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "来源路径没有文件名",
                ));
            };
            let destination = unique_copy_destination(directory, name);
            copy_entry(source, &destination)?;
            Ok(destination)
        })
        .collect()
}

/// 生成不冲突的副本路径：「a.txt」→「a 副本.txt」→「a 副本 2.txt」。
fn unique_copy_destination(directory: &Path, name: &std::ffi::OsStr) -> PathBuf {
    let first = directory.join(name);
    if !first.exists() {
        return first;
    }

    let path = Path::new(name);
    let stem = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| name.to_string_lossy().into_owned());
    let extension = path
        .extension()
        .map(|extension| extension.to_string_lossy().into_owned());

    for index in 1..1000 {
        let suffix = if index == 1 {
            format!(" {COPY_SUFFIX}")
        } else {
            format!(" {COPY_SUFFIX} {index}")
        };
        let candidate = match &extension {
            Some(extension) => directory.join(format!("{stem}{suffix}.{extension}")),
            None => directory.join(format!("{stem}{suffix}")),
        };
        if !candidate.exists() {
            return candidate;
        }
    }

    let fallback = format!("{stem} {COPY_SUFFIX} {}", std::process::id());
    match &extension {
        Some(extension) => directory.join(format!("{fallback}.{extension}")),
        None => directory.join(fallback),
    }
}

/// 递归复制（目录、普通文件、符号链接）。不覆盖：目标路径由调用方保证不存在。
fn copy_entry(source: &Path, destination: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(source)?;

    if metadata.file_type().is_symlink() {
        let target = fs::read_link(source)?;
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(target, destination)?;
            return Ok(());
        }
        #[cfg(not(unix))]
        {
            let _ = target;
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "当前平台不支持复制符号链接",
            ));
        }
    }

    if metadata.is_dir() {
        fs::create_dir(destination)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            copy_entry(&entry.path(), &destination.join(entry.file_name()))?;
        }
        return Ok(());
    }

    fs::copy(source, destination)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "halo-desktop-menu-{}-{name}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create scratch dir");
        dir
    }

    #[test]
    fn desktop_action_tags_are_unique_and_round_trip() {
        let menu = MenuContext::Blank.actions();
        let mut tags: Vec<isize> = menu.iter().map(|a| a.tag()).collect();
        tags.sort_unstable();
        tags.dedup();
        assert_eq!(tags.len(), menu.len());
        for action in ALL_ACTIONS {
            assert_eq!(DesktopAction::from_tag(action.tag()), Some(action));
        }
        assert_eq!(DesktopAction::from_tag(0), None);
        assert_eq!(DesktopAction::from_tag(99), None);
    }

    #[test]
    fn separators_split_the_file_group_from_the_rest() {
        assert!(DesktopAction::NewTextFile.separator_before());
        assert!(!DesktopAction::NewMarkdownFile.separator_before());
        assert!(DesktopAction::NewFolder.separator_before());
        assert!(!DesktopAction::OpenInTerminal.separator_before());
        assert!(DesktopAction::ShowInfo.separator_before());
        assert!(DesktopAction::ShowFinderMenu.separator_before());
        assert!(!DesktopAction::Open.separator_before());
    }

    #[test]
    fn item_context_starts_with_open_and_drops_blank_only_actions() {
        let actions = MenuContext::Items(vec![PathBuf::from("/tmp/a.txt")]).actions();
        assert_eq!(actions.first(), Some(&DesktopAction::Open));
        assert!(!actions.contains(&DesktopAction::Paste));
        assert!(!actions.contains(&DesktopAction::Wallpaper));
        assert!(actions.contains(&DesktopAction::ShowInfo));
        assert!(actions.contains(&DesktopAction::MoveToTrash));

        let blank = MenuContext::Blank.actions();
        assert!(!blank.contains(&DesktopAction::ShowInfo));
        assert!(blank.contains(&DesktopAction::Paste));
    }

    #[test]
    fn create_folder_uses_untitled_names() {
        let dir = scratch_dir("folder");
        let first = create_folder(&dir).expect("first folder");
        let second = create_folder(&dir).expect("second folder");
        assert_eq!(first.file_name().and_then(|n| n.to_str()), Some("未命名文件夹"));
        assert_eq!(second.file_name().and_then(|n| n.to_str()), Some("未命名文件夹 2"));
        assert!(first.is_dir() && second.is_dir());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn paste_keeps_names_and_adds_copy_suffix_on_conflict() {
        let dir = scratch_dir("paste");
        let source_root = dir.join("source");
        let destination = dir.join("desktop");
        fs::create_dir_all(&source_root).expect("source");
        fs::create_dir_all(&destination).expect("destination");

        let text = source_root.join("note.txt");
        fs::write(&text, b"hello").expect("write text");

        let folder = source_root.join("assets");
        fs::create_dir_all(folder.join("nested")).expect("nested");
        fs::write(folder.join("nested").join("a.md"), b"# a").expect("write nested");

        fs::write(destination.join("note.txt"), b"existing").expect("existing");

        let results = paste_into(&destination, &[text.clone(), folder.clone()]);
        let pasted_text = results[0].as_ref().expect("paste text");
        let pasted_folder = results[1].as_ref().expect("paste folder");

        assert_eq!(
            pasted_text.file_name().and_then(|n| n.to_str()),
            Some("note 副本.txt")
        );
        assert_eq!(fs::read(pasted_text).expect("read pasted"), b"hello");
        assert_eq!(
            fs::read(destination.join("note.txt")).expect("original"),
            b"existing"
        );

        assert_eq!(pasted_folder.file_name().and_then(|n| n.to_str()), Some("assets"));
        assert_eq!(
            fs::read(pasted_folder.join("nested").join("a.md")).expect("nested copy"),
            b"# a"
        );

        let again = paste_into(&destination, &[text]).remove(0).expect("second paste");
        assert_eq!(
            again.file_name().and_then(|n| n.to_str()),
            Some("note 副本 2.txt")
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn paste_does_not_escape_without_file_name() {
        let dir = scratch_dir("no-name");
        let results = paste_into(&dir, &[PathBuf::from("/")]);
        assert!(results[0].is_err());
        let _ = fs::remove_dir_all(&dir);
    }
}
