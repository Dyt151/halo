//! 工具动作管线：打包 / 解压 / PDF 每页出图 / PDF → CBZ。
//!
//! 动作与格式转换的差别（docs/REA-Filee-Conversion.md 动作模型）：
//! - 不走路由图，没有「源格式 → 目标格式」的边可言；
//! - 产物可能是**目录**（解压 / 分页）或**一个批次的单个文件**（打包）；
//! - 只用 macOS 自带工具：`zip` / `tar` / `ditto`（ZIP 解压）+ PDFKit 桥。
//!
//! 安全约定与 `output.rs` 一致：
//! 1. **候选路径 == 任一源路径时永不覆盖**（源文件保护优先于冲突策略）；
//! 2. 文件型产物先写 `.<名字>.clipbase-<uuid>.tmp` 再 rename（原子提交）；
//! 3. 目录型产物先在工作目录里建 staging，完成后再搬到位（失败不留半成品）；
//! 4. Overwrite 策略下目录**合并**内容（绝不删除已存在的目标目录）。

use std::path::{Path, PathBuf};
use std::process::Command;

use super::edge::ConvertError;
use super::engines::pdfkit::render_pages as pdfkit_pages;
use super::format::{detect_format, ConvertFormat};
use super::job::{CancelToken, ConversionFileResult, FileState, SkipCode};
use super::output::same_file;
use super::preset::{ArchiveFormat, ConflictPolicy, ConversionPreset, PresetKind};

/// 动作输出的分配结果：与 `AllocError` 同构（跳过 ≠ 错误）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionAllocError {
    Skipped(String),
    Io(String),
}

/// 每文件动作（解压 / PDF 分页 / PDF → CBZ）的完整执行。
///
/// 调用方（job.rs）负责串行化与进度回调；这里只回答「这个文件出什么结果」。
pub fn run_file_action(
    source: &str,
    preset: &ConversionPreset,
    work_dir: &Path,
    cancel: &CancelToken,
) -> ConversionFileResult {
    let mut result = ConversionFileResult::pending(source);
    if cancel.is_cancelled() {
        result.state = FileState::Cancelled;
        result.error = Some(ConvertError::cancelled());
        return result;
    }
    match &preset.kind {
        PresetKind::Extract => run_extract(source, preset, work_dir),
        PresetKind::PdfPages { to } => run_pdf_pages(source, *to, preset, work_dir),
        PresetKind::PdfToCbz => run_pdf_to_cbz(source, preset, work_dir),
        _ => result.fail(ConvertError::unsupported(format!(
            "动作 {} 不是单文件动作",
            preset.name
        ))),
    }
}

/// 整批打包：所有源 → 一个压缩包。
///
/// 成功时每个源文件的结果都是 `Done` 且 `outputs` 指向同一个压缩包；
/// 失败时问题文件报 `Failed`，其后的标 `Cancelled`（批次已中止）。
pub fn run_pack(
    sources: &[String],
    preset: &ConversionPreset,
    cancel: &CancelToken,
) -> Vec<ConversionFileResult> {
    let mut results: Vec<ConversionFileResult> =
        sources.iter().map(|s| ConversionFileResult::pending(s)).collect();
    if sources.is_empty() {
        return results;
    }
    let archive = match preset.kind {
        PresetKind::Pack { archive } => archive,
        _ => {
            results[0] =
                results[0].clone().fail(ConvertError::unsupported("不是打包动作".to_string()));
            return results;
        }
    };

    if cancel.is_cancelled() {
        for result in &mut results {
            result.state = FileState::Cancelled;
            result.error = Some(ConvertError::cancelled());
        }
        return results;
    }

    // 批次前置检查：一个源读不了，整批不开始（压缩包是整体产物）。
    for (index, source) in sources.iter().enumerate() {
        match std::fs::metadata(source) {
            Ok(meta) if !meta.is_file() => {
                return abort_pack(
                    results,
                    index,
                    ConvertError::io("不是普通文件（文件夹或特殊设备无法打包）"),
                );
            }
            Ok(_) => {}
            Err(e) => {
                return abort_pack(
                    results,
                    index,
                    ConvertError::io(format!("读取文件信息失败：{e}")),
                );
            }
        }
    }

    let (final_path, temp_path) = match reserve_file(
        Path::new(&sources[0]),
        &preset.output,
        archive.as_str(),
        sources,
    ) {
        Ok(pair) => pair,
        Err(ActionAllocError::Skipped(detail)) => {
            for result in &mut results {
                let source = result.source.clone();
                *result = ConversionFileResult::pending(&source)
                    .skip_with(SkipCode::Conflict, detail.clone());
            }
            return results;
        }
        Err(ActionAllocError::Io(detail)) => {
            return abort_pack(results, 0, ConvertError::io(detail));
        }
    };

    let common = common_parent(sources);
    let rels: Vec<PathBuf> = sources
        .iter()
        .map(|s| {
            Path::new(s)
                .strip_prefix(&common)
                .unwrap_or_else(|_| Path::new(s))
                .to_path_buf()
        })
        .collect();

    let output = match archive {
        ArchiveFormat::Zip => Command::new("zip")
            .arg("-q")
            .arg("-r")
            .arg(&temp_path)
            .args(&rels)
            .current_dir(&common)
            .output(),
        ArchiveFormat::Tar => Command::new("tar")
            .arg("-cf")
            .arg(&temp_path)
            .args(&rels)
            .current_dir(&common)
            .output(),
        ArchiveFormat::TarGz => Command::new("tar")
            .arg("-czf")
            .arg(&temp_path)
            .args(&rels)
            .current_dir(&common)
            .output(),
    };
    let run_failed = |output: &Result<std::process::Output, std::io::Error>| -> ConvertError {
        match output {
            Err(e) => ConvertError::io(format!("无法启动打包工具：{e}")),
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let stderr = stderr.trim();
                ConvertError::encode(format!(
                    "打包 {} 失败{}",
                    archive.label(),
                    if stderr.is_empty() {
                        String::new()
                    } else {
                        format!("：{stderr}")
                    }
                ))
            }
        }
    };
    if output.as_ref().map_or(true, |o| !o.status.success()) {
        let error = run_failed(&output);
        let _ = std::fs::remove_file(&temp_path);
        return abort_pack(results, 0, error);
    }

    if let Err(e) = std::fs::rename(&temp_path, &final_path) {
        let _ = std::fs::remove_file(&temp_path);
        return abort_pack(results, 0, ConvertError::io(format!("提交压缩包失败：{e}")));
    }

    let out = final_path.to_string_lossy().to_string();
    for result in &mut results {
        result.state = FileState::Done;
        result.progress = 1.0;
        result.outputs = vec![out.clone()];
    }
    results
}

/// 打包中止：第 `index` 个源报错，前面的保持 Pending，后面的标 Cancelled。
fn abort_pack(
    mut results: Vec<ConversionFileResult>,
    index: usize,
    error: ConvertError,
) -> Vec<ConversionFileResult> {
    for (i, result) in results.iter_mut().enumerate() {
        if i == index {
            *result = result.clone().fail(error.clone());
        } else if i > index {
            result.state = FileState::Cancelled;
            result.error = Some(ConvertError::cancelled());
        }
    }
    results
}

/// 解压一个压缩包到「同目录下的 <文件名> 文件夹」。
fn run_extract(source: &str, preset: &ConversionPreset, work_dir: &Path) -> ConversionFileResult {
    let mut result = ConversionFileResult::pending(source);
    let Some(archive) = ArchiveFormat::from_path(source) else {
        return result.skip_with(
            SkipCode::UnknownFormat,
            "不是支持的压缩包（只支持 ZIP / TAR / TAR.GZ）",
        );
    };

    let final_dir = match reserve_dir(Path::new(source), &preset.output) {
        Ok(dir) => dir,
        Err(ActionAllocError::Skipped(detail)) => {
            return result.skip_with(SkipCode::Conflict, detail);
        }
        Err(ActionAllocError::Io(detail)) => return result.fail(ConvertError::io(detail)),
    };
    let staging = staging_dir(work_dir, "extract");

    // 先解到 staging，成功后整目录搬到 final：失败不会在目标目录留下半成品。
    let output = match archive {
        ArchiveFormat::Zip => Command::new("ditto")
            .arg("-x")
            .arg("-k")
            .arg(source)
            .arg(&staging)
            .output(),
        ArchiveFormat::Tar | ArchiveFormat::TarGz => Command::new("tar")
            .arg("-xf")
            .arg(source)
            .arg("-C")
            .arg(&staging)
            .output(),
    };
    match output {
        Err(e) => {
            let _ = std::fs::remove_dir_all(&staging);
            return result.fail(ConvertError::io(format!("无法启动解压工具：{e}")));
        }
        Ok(output) if !output.status.success() => {
            let _ = std::fs::remove_dir_all(&staging);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stderr = stderr.trim();
            return result.fail(ConvertError::encode(format!(
                "解压 {} 失败：{}",
                archive.label(),
                if stderr.is_empty() {
                    "压缩包可能已损坏"
                } else {
                    stderr
                }
            )));
        }
        Ok(_) => {}
    }

    if let Err(e) = install_dir(&staging, &final_dir) {
        let _ = std::fs::remove_dir_all(&staging);
        return result.fail(ConvertError::io(format!("提交解压结果失败：{e}")));
    }

    result.state = FileState::Done;
    result.progress = 1.0;
    result.outputs = vec![final_dir.to_string_lossy().to_string()];
    result
}

/// PDF 每页 → 一张 PNG / JPG，输出到 `<文件名>` 文件夹。
fn run_pdf_pages(
    source: &str,
    to: ConvertFormat,
    preset: &ConversionPreset,
    work_dir: &Path,
) -> ConversionFileResult {
    let mut result = ConversionFileResult::pending(source);
    if detect_format(source) != Some(ConvertFormat::Pdf) {
        return result.skip_with(SkipCode::UnknownFormat, "只有 PDF 支持每页出图");
    }
    let final_dir = match reserve_dir(Path::new(source), &preset.output) {
        Ok(dir) => dir,
        Err(ActionAllocError::Skipped(detail)) => {
            return result.skip_with(SkipCode::Conflict, detail);
        }
        Err(ActionAllocError::Io(detail)) => return result.fail(ConvertError::io(detail)),
    };
    let staging = staging_dir(work_dir, "pages");
    match pdfkit_pages(Path::new(source), &staging, to, preset.image.jpeg_quality) {
        Ok(0) => {
            let _ = std::fs::remove_dir_all(&staging);
            return result.fail(ConvertError::decode("PDF 没有页面".to_string()));
        }
        Ok(_) => {}
        Err(e) => {
            let _ = std::fs::remove_dir_all(&staging);
            return result.fail(e);
        }
    }
    if let Err(e) = install_dir(&staging, &final_dir) {
        let _ = std::fs::remove_dir_all(&staging);
        return result.fail(ConvertError::io(format!("提交分页结果失败：{e}")));
    }

    result.detected_format = Some("pdf".to_string());
    result.target_format = Some(to.as_str().to_string());
    result.state = FileState::Done;
    result.progress = 1.0;
    result.outputs = vec![final_dir.to_string_lossy().to_string()];
    result.warnings = vec!["每页按 144 dpi 渲染".to_string()];
    result
}

/// PDF → CBZ 漫画包（每页一张 PNG 打进 zip）。
fn run_pdf_to_cbz(source: &str, preset: &ConversionPreset, work_dir: &Path) -> ConversionFileResult {
    let mut result = ConversionFileResult::pending(source);
    if detect_format(source) != Some(ConvertFormat::Pdf) {
        return result.skip_with(SkipCode::UnknownFormat, "只有 PDF 支持转 CBZ");
    }
    let (final_path, temp_path) = match reserve_file(Path::new(source), &preset.output, "cbz", &[]) {
        Ok(pair) => pair,
        Err(ActionAllocError::Skipped(detail)) => {
            return result.skip_with(SkipCode::Conflict, detail);
        }
        Err(ActionAllocError::Io(detail)) => return result.fail(ConvertError::io(detail)),
    };
    let staging = staging_dir(work_dir, "cbz");
    match pdfkit_pages(
        Path::new(source),
        &staging,
        ConvertFormat::Png,
        preset.image.jpeg_quality,
    ) {
        Ok(0) => {
            let _ = std::fs::remove_dir_all(&staging);
            return result.fail(ConvertError::decode("PDF 没有页面".to_string()));
        }
        Ok(_) => {}
        Err(e) => {
            let _ = std::fs::remove_dir_all(&staging);
            return result.fail(e);
        }
    }
    let output = Command::new("ditto")
        .arg("-c")
        .arg("-k")
        .arg(&staging)
        .arg(&temp_path)
        .output();
    let _ = std::fs::remove_dir_all(&staging);
    match output {
        Err(e) => {
            let _ = std::fs::remove_file(&temp_path);
            return result.fail(ConvertError::io(format!("无法启动 ditto：{e}")));
        }
        Ok(output) if !output.status.success() => {
            let _ = std::fs::remove_file(&temp_path);
            return result.fail(ConvertError::encode("打包 CBZ 失败".to_string()));
        }
        Ok(_) => {}
    }
    if let Err(e) = std::fs::rename(&temp_path, &final_path) {
        let _ = std::fs::remove_file(&temp_path);
        return result.fail(ConvertError::io(format!("提交 CBZ 失败：{e}")));
    }

    result.detected_format = Some("pdf".to_string());
    result.target_format = Some("cbz".to_string());
    result.state = FileState::Done;
    result.progress = 1.0;
    result.outputs = vec![final_path.to_string_lossy().to_string()];
    result.warnings = vec!["CBZ 页面为 144 dpi PNG".to_string()];
    result
}

/// staging 目录就位：目标不存在 → 原子 rename；存在（Overwrite）→ 合并内容。
fn install_dir(staging: &Path, final_dir: &Path) -> std::io::Result<()> {
    if !final_dir.exists() {
        return std::fs::rename(staging, final_dir);
    }
    merge_dir(staging, final_dir)?;
    std::fs::remove_dir_all(staging)
}

/// 把 `from` 的内容合并进 `to`（同名文件覆盖、子目录递归）。
fn merge_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let dest = to.join(entry.file_name());
        if entry.path().is_dir() {
            std::fs::create_dir_all(&dest)?;
            merge_dir(&entry.path(), &dest)?;
        } else {
            let _ = std::fs::remove_file(&dest);
            std::fs::rename(entry.path(), &dest)?;
        }
    }
    Ok(())
}

/// 文件型动作产物（CBZ / 压缩包）的分配：冲突策略 + 源文件保护 + 临时路径。
///
/// 源文件保护无条件优先：候选路径撞上源（含同批次其他源）时直接编号，
/// 即使策略是 Overwrite（想替换源文件是另一个显式操作）。
fn reserve_file(
    source: &Path,
    rule: &super::preset::OutputRule,
    ext: &str,
    batch_sources: &[String],
) -> Result<(PathBuf, PathBuf), ActionAllocError> {
    let dir = rule.dir_or_source(source);
    std::fs::create_dir_all(&dir)
        .map_err(|e| ActionAllocError::Io(format!("创建输出目录失败：{e}")))?;
    let stem = stem_of(source);
    let blocked = |candidate: &Path| {
        same_file(candidate, source)
            || batch_sources
                .iter()
                .map(Path::new)
                .any(|s| same_file(candidate, s))
    };

    let mut n: u32 = 1;
    let mut candidate = dir.join(format!("{stem}.{ext}"));
    loop {
        if blocked(&candidate) || (candidate.exists() && rule.conflict == ConflictPolicy::Rename) {
            n += 1;
            if n > 9999 {
                return Err(ActionAllocError::Io(
                    "同一目录下同名文件过多，无法分配输出路径".to_string(),
                ));
            }
            candidate = dir.join(format!("{stem} ({n}).{ext}"));
            continue;
        }
        if candidate.exists() {
            match rule.conflict {
                ConflictPolicy::Skip => {
                    return Err(ActionAllocError::Skipped(format!(
                        "目标已存在（策略：跳过）：{}",
                        candidate.display()
                    )))
                }
                ConflictPolicy::Overwrite => break,
                ConflictPolicy::Rename => unreachable!("上面已经处理了 Rename"),
            }
        }
        break;
    }

    let temp = dir.join(format!(
        ".{}.clipbase-{}.tmp",
        candidate
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("out"),
        uuid::Uuid::new_v4()
    ));
    Ok((candidate, temp))
}

/// 目录型动作产物（解压 / 分页）的分配：`<目录>/<文件名>`，重名按策略处理。
///
/// Overwrite 不删除已有目录——解压/分页是**合并**内容（见模块头约定 4）。
fn reserve_dir(source: &Path, rule: &super::preset::OutputRule) -> Result<PathBuf, ActionAllocError> {
    let dir = rule.dir_or_source(source);
    std::fs::create_dir_all(&dir)
        .map_err(|e| ActionAllocError::Io(format!("创建输出目录失败：{e}")))?;
    let stem = stem_of(source);
    let candidate = dir.join(&stem);
    if candidate.exists() {
        match rule.conflict {
            ConflictPolicy::Skip => {
                return Err(ActionAllocError::Skipped(format!(
                    "目标文件夹已存在（策略：跳过）：{}",
                    candidate.display()
                )))
            }
            ConflictPolicy::Overwrite => {}
            ConflictPolicy::Rename => {
                for n in 2..=9999u32 {
                    let numbered = dir.join(format!("{stem} ({n})"));
                    if !numbered.exists() {
                        return Ok(numbered);
                    }
                }
                return Err(ActionAllocError::Io(
                    "同一目录下同名文件夹过多，无法分配输出路径".to_string(),
                ));
            }
        }
    }
    Ok(candidate)
}

fn stem_of(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "output".to_string())
}

/// 工作目录下的 staging 目录：不存在才建，存在就递增编号（并发生成不撞车）。
fn staging_dir(base: &Path, tag: &str) -> PathBuf {
    for n in 0..10_000u32 {
        let candidate = base.join(if n == 0 {
            format!("clipbase-{tag}-{}", std::process::id())
        } else {
            format!("clipbase-{tag}-{}-{n}", std::process::id())
        });
        match std::fs::create_dir(&candidate) {
            Ok(()) => return candidate,
            Err(_) => continue,
        }
    }
    std::env::temp_dir().join(format!(
        "clipbase-{tag}-{}-{}",
        std::process::id(),
        uuid::Uuid::new_v4()
    ))
}

/// 一批路径的最长公共父目录（按**文件所在目录**比较，跨卷时退到根目录）。
///
/// 用父目录而不是完整路径：只有一个源文件时公共前缀就是文件本身，
/// 打包时的相对路径会变成空串。
fn common_parent(paths: &[String]) -> PathBuf {
    let comps: Vec<Vec<std::path::Component<'_>>> = paths
        .iter()
        .map(|p| {
            Path::new(p)
                .parent()
                .unwrap_or_else(|| Path::new("/"))
                .components()
                .collect()
        })
        .collect();
    let Some(first) = comps.first() else {
        return PathBuf::from("/");
    };
    let mut common: Vec<std::path::Component<'_>> = Vec::new();
    'outer: for (i, comp) in first.iter().enumerate() {
        for other in comps.iter().skip(1) {
            if other.get(i) != Some(comp) {
                break 'outer;
            }
        }
        common.push(*comp);
    }
    if common.is_empty() {
        PathBuf::from("/")
    } else {
        common.iter().collect()
    }
}

/// 动作可用性（环形盘切片启用/禁用）：返回 (可执行数, 跳过数, 首个原因)。
pub fn availability(preset: &ConversionPreset, paths: &[String]) -> (usize, usize, Option<String>) {
    match &preset.kind {
        PresetKind::Pack { .. } => (paths.len(), 0, None),
        PresetKind::Extract => {
            let convertible = paths
                .iter()
                .filter(|p| ArchiveFormat::from_path(p).is_some())
                .count();
            let skipped = paths.len() - convertible;
            (
                convertible,
                skipped,
                (skipped > 0).then(|| "有文件不是 ZIP / TAR / TAR.GZ 压缩包".to_string()),
            )
        }
        PresetKind::PdfPages { .. } | PresetKind::PdfToCbz => {
            let convertible = paths
                .iter()
                .filter(|p| detect_format(p) == Some(ConvertFormat::Pdf))
                .count();
            let skipped = paths.len() - convertible;
            (
                convertible,
                skipped,
                (skipped > 0).then(|| "有文件不是 PDF".to_string()),
            )
        }
        PresetKind::Convert => (0, paths.len(), Some("不是工具动作".to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::convert::preset::{ImageOptions, OutputRule, TargetSpec};

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "clipbase-action-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create scratch");
        dir
    }

    fn default_preset(kind: PresetKind) -> ConversionPreset {
        ConversionPreset {
            id: "act.test".into(),
            name: "测试动作".into(),
            kind,
            target: TargetSpec::SameAsSource,
            image: ImageOptions::default(),
            output: OutputRule::default(),
            reencode_same_format: false,
            builtin: false,
        }
    }

    #[test]
    fn pack_creates_zip_with_all_sources_and_renames_on_conflict() {
        let dir = scratch("pack");
        std::fs::write(dir.join("a.txt"), b"aaa").unwrap();
        std::fs::write(dir.join("b.txt"), b"bbb").unwrap();
        let sources = vec![
            dir.join("a.txt").to_string_lossy().to_string(),
            dir.join("b.txt").to_string_lossy().to_string(),
        ];
        let results = run_pack(
            &sources,
            &default_preset(PresetKind::Pack { archive: ArchiveFormat::Zip }),
            &CancelToken::new(),
        );
        assert!(results.iter().all(|r| r.state == FileState::Done), "{results:?}");
        let out = &results[0].outputs[0];
        assert!(out.ends_with("a.zip"), "{out}");
        assert!(Path::new(out).exists());
        assert!(results.iter().all(|r| r.outputs[0] == *out));

        // 第二次跑：Rename 策略 → a (2).zip。
        let again = run_pack(
            &sources,
            &default_preset(PresetKind::Pack { archive: ArchiveFormat::Zip }),
            &CancelToken::new(),
        );
        assert!(again[0].outputs[0].ends_with("a (2).zip"), "{:?}", again[0].outputs);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pack_never_overwrites_a_source_archive() {
        let dir = scratch("pack-src");
        std::fs::write(dir.join("a.zip"), b"original").unwrap();
        let sources = vec![dir.join("a.zip").to_string_lossy().to_string()];
        let results = run_pack(
            &sources,
            &{
                let mut p = default_preset(PresetKind::Pack { archive: ArchiveFormat::Zip });
                p.output.conflict = ConflictPolicy::Overwrite;
                p
            },
            &CancelToken::new(),
        );
        assert_eq!(results[0].state, FileState::Done);
        assert!(results[0].outputs[0].ends_with("a (2).zip"));
        assert_eq!(std::fs::read(dir.join("a.zip")).unwrap(), b"original");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pack_with_missing_source_aborts_batch() {
        let dir = scratch("pack-missing");
        std::fs::write(dir.join("ok.txt"), b"ok").unwrap();
        let sources = vec![
            dir.join("ok.txt").to_string_lossy().to_string(),
            dir.join("gone.txt").to_string_lossy().to_string(),
        ];
        let results = run_pack(
            &sources,
            &default_preset(PresetKind::Pack { archive: ArchiveFormat::Zip }),
            &CancelToken::new(),
        );
        assert_eq!(results[0].state, FileState::Pending);
        assert_eq!(results[1].state, FileState::Failed);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn extract_unzips_into_stem_folder() {
        let dir = scratch("extract");
        let inner = dir.join("payload");
        std::fs::create_dir_all(&inner).unwrap();
        std::fs::write(inner.join("x.txt"), b"xyz").unwrap();
        let zip_path = dir.join("bundle.zip");
        let ok = Command::new("ditto")
            .arg("-c")
            .arg("-k")
            .arg(&inner)
            .arg(&zip_path)
            .status()
            .expect("ditto");
        assert!(ok.success());

        let result = run_file_action(
            &zip_path.to_string_lossy().to_string(),
            &default_preset(PresetKind::Extract),
            &std::env::temp_dir(),
            &CancelToken::new(),
        );
        assert_eq!(result.state, FileState::Done, "{result:?}");
        let out = Path::new(&result.outputs[0]);
        assert!(out.join("x.txt").exists(), "{out:?}");

        // 非压缩包 → 跳过。
        let plain = dir.join("note.txt");
        std::fs::write(&plain, b"hi").unwrap();
        let result = run_file_action(
            &plain.to_string_lossy().to_string(),
            &default_preset(PresetKind::Extract),
            &std::env::temp_dir(),
            &CancelToken::new(),
        );
        assert_eq!(result.state, FileState::Skipped);
        assert_eq!(result.skip.unwrap().code, SkipCode::UnknownFormat);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn extract_respects_skip_policy() {
        let dir = scratch("extract-skip");
        let zip_path = dir.join("bundle.zip");
        std::fs::write(&zip_path, b"PK not real").unwrap();
        std::fs::create_dir_all(dir.join("bundle")).unwrap();

        let mut preset = default_preset(PresetKind::Extract);
        preset.output.conflict = ConflictPolicy::Skip;
        let result = run_file_action(
            &zip_path.to_string_lossy().to_string(),
            &preset,
            &std::env::temp_dir(),
            &CancelToken::new(),
        );
        assert_eq!(result.state, FileState::Skipped);
        assert_eq!(result.skip.unwrap().code, SkipCode::Conflict);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn non_pdf_files_skip_pdf_actions() {
        let dir = scratch("pdf-skip");
        let txt = dir.join("note.txt");
        std::fs::write(&txt, b"hi").unwrap();
        for kind in [
            PresetKind::PdfPages { to: ConvertFormat::Png },
            PresetKind::PdfToCbz,
        ] {
            let result = run_file_action(
                &txt.to_string_lossy().to_string(),
                &default_preset(kind),
                &std::env::temp_dir(),
                &CancelToken::new(),
            );
            assert_eq!(result.state, FileState::Skipped, "{result:?}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn availability_counts_action_applicable_files() {
        let dir = scratch("avail");
        std::fs::write(dir.join("a.zip"), b"PK").unwrap();
        std::fs::write(dir.join("b.tar.gz"), b"tgz").unwrap();
        std::fs::write(dir.join("c.txt"), b"hi").unwrap();
        let paths: Vec<String> = ["a.zip", "b.tar.gz", "c.txt"]
            .iter()
            .map(|n| dir.join(n).to_string_lossy().to_string())
            .collect();

        let (ok, skip, reason) = availability(&default_preset(PresetKind::Extract), &paths);
        assert_eq!((ok, skip), (2, 1));
        assert!(reason.is_some());

        let (ok, skip, reason) =
            availability(&default_preset(PresetKind::Pack { archive: ArchiveFormat::Zip }), &paths);
        assert_eq!((ok, skip), (3, 0));
        assert!(reason.is_none());

        let (ok, skip, reason) =
            availability(&default_preset(PresetKind::PdfPages { to: ConvertFormat::Png }), &paths);
        assert_eq!((ok, skip), (0, 3));
        assert!(reason.is_some());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn common_parent_finds_shared_prefix() {
        let paths = vec![
            "/tmp/x/a.txt".to_string(),
            "/tmp/x/sub/b.txt".to_string(),
        ];
        assert_eq!(common_parent(&paths), PathBuf::from("/tmp/x"));

        let mixed = vec!["/tmp/a.txt".to_string(), "/Users/b.txt".to_string()];
        assert_eq!(common_parent(&mixed), PathBuf::from("/"));

        let single = vec!["/a/b/c.txt".to_string()];
        assert_eq!(common_parent(&single), PathBuf::from("/a/b"));
    }
}
