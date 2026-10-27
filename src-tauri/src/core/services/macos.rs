//! macOS 动作执行器：新建文件 / 打开终端。
//!
//! Finder 扩展、快捷键、环形盘都从这里落地，保证三条入口行为一致：
//! - 目标目录规则：选中文件夹取自己，选中文件取父目录，空白处用当前目录；
//! - 新建文件用 `create_new` 占位，绝不覆盖已有文件。

use std::fs;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};
use std::process::Command;

use super::{notify, resolve_target_directory, terminal_cd_script, unique_file_path, ServiceAction};

/// 系统自带文本编辑器：txt / md 用它打开。
const TEXT_EDIT_APP: &str = "TextEdit";

/// 执行一次右键 / 扩展动作。
///
/// `selected` 是 Finder 选中项（可能为空，例如桌面空白处右键）；
/// `container` 是 Finder 当前目录，仅在 `selected` 为空时使用。
pub(super) fn run_action(action: ServiceAction, selected: &[PathBuf], container: Option<&Path>) {
    let directory = resolve_target_directory(selected)
        .or_else(|| container.filter(|path| path.is_dir()).map(Path::to_path_buf));

    let Some(directory) = directory else {
        tracing::warn!("动作「{}」：没有可用的目标目录", action.menu_title());
        notify("ClipBase", "未能识别要操作的文件夹");
        return;
    };

    tracing::info!(
        "Finder 动作「{}」：目标目录 {}",
        action.menu_title(),
        directory.display()
    );

    match action {
        ServiceAction::NewTextFile | ServiceAction::NewMarkdownFile => {
            create_and_open(action, &directory)
        }
        ServiceAction::OpenInTerminal => open_directory_in_terminal(&directory),
    }
}

/// 在目标目录创建文件并用 TextEdit 打开。
fn create_and_open(action: ServiceAction, directory: &Path) {
    match create_document(action, directory) {
        Ok(path) => {
            tracing::info!("已创建 {}", path.display());
            open_with_app(&path, TEXT_EDIT_APP);
        }
        Err(err) => {
            tracing::error!("创建文件失败: {err}");
            notify("ClipBase", &format!("创建文件失败：{err}"));
        }
    }
}

/// 创建「未命名.<ext>」空文件。
///
/// 先 `create_new` 占位，避免 TOCTOU 覆盖同名文件；唯一名生成
/// 期间若被其它进程抢先创建，就重新取下一个名字。
fn create_document(action: ServiceAction, directory: &Path) -> io::Result<PathBuf> {
    let extension = action
        .file_extension()
        .ok_or_else(|| io::Error::new(ErrorKind::InvalidInput, "该动作不创建文件"))?;

    for _ in 0..64 {
        let path = unique_file_path(directory, extension);
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(file) => {
                if let Err(err) = file.sync_all() {
                    drop(file);
                    let _ = fs::remove_file(&path);
                    return Err(err);
                }
                return Ok(path);
            }
            Err(err) if err.kind() == ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err),
        }
    }

    Err(io::Error::new(
        ErrorKind::AlreadyExists,
        "目录里同名文件太多，无法生成新文件名",
    ))
}

fn open_with_app(path: &Path, app: &str) {
    match Command::new("open").arg("-a").arg(app).arg(path).status() {
        Ok(status) if status.success() => {
            tracing::debug!("已用 {app} 打开 {}", path.display());
        }
        Ok(status) => tracing::warn!("`open -a {app}` 退出码 {:?}", status.code()),
        Err(err) => tracing::error!("执行 `open -a {app}` 失败: {err}"),
    }
}

/// 在 Terminal 中 cd 到目标目录（AppleScript）。
fn open_directory_in_terminal(directory: &Path) {
    let script = terminal_cd_script(directory);
    match Command::new("osascript").arg("-e").arg(&script).status() {
        Ok(status) if status.success() => {
            tracing::debug!("已在新 Terminal 窗口进入 {}", directory.display());
        }
        Ok(status) => tracing::warn!("osascript 退出码 {:?}", status.code()),
        Err(err) => tracing::error!("执行 osascript 失败: {err}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn text_documents_are_empty_but_created() {
        let dir = scratch_dir("text");
        let txt = create_document(ServiceAction::NewTextFile, &dir).expect("create txt");
        let md = create_document(ServiceAction::NewMarkdownFile, &dir).expect("create md");
        assert_eq!(txt.file_name().and_then(|name| name.to_str()), Some("未命名.txt"));
        assert_eq!(md.file_name().and_then(|name| name.to_str()), Some("未命名.md"));
        assert_eq!(fs::metadata(&txt).expect("txt metadata").len(), 0);
        assert_eq!(fs::metadata(&md).expect("md metadata").len(), 0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn create_document_rejects_non_file_actions() {
        let dir = scratch_dir("invalid");
        let err = create_document(ServiceAction::OpenInTerminal, &dir).expect_err("must reject");
        assert_eq!(err.kind(), ErrorKind::InvalidInput);
        let _ = fs::remove_dir_all(&dir);
    }
}
