//! 输出分配器：路径分配、冲突处理、源文件保护、原子提交（§4 / §8.4）。
//!
//! 三条不能省掉的规则：
//! 1. **候选路径 == 源路径时永远不覆盖**，即使策略是 `Overwrite`——源文件保护
//!    优先于策略，想替换源文件是另一个显式操作；
//! 2. **先预留再写**：只查磁盘 `exists()` 会在并发下让两个文件拿到同一个空位，
//!    所以分配时就把路径记进本 Job 的预留表；
//! 3. **先写临时文件再 rename**：引擎写 `<目录>/.<名字>.clipbase-<id>.tmp`，
//!    成功关闭句柄后才提交到最终路径，失败 / 取消时删掉半成品，绝不留下一个
//!    看起来像成功结果的残文件。

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use super::format::ConvertFormat;
use super::preset::{ConflictPolicy, OutputRule};
use crate::lock_ext::LockExt;

/// 残留临时文件的「过期」阈值：超过这个时间的 `.*.clipbase-*.tmp` 才算上次崩溃留下的。
///
/// 不按「文件存在就删」处理：同一个输出目录里可能还有另一个进程/另一个窗口
/// 正在写的临时文件，按时间隔开才不会双双失败。
const STALE_TEMP_AGE: Duration = Duration::from_secs(60 * 60);

/// 一次分配请求。
pub struct OutputRequest<'a> {
    pub source: &'a Path,
    pub target: ConvertFormat,
    /// 预设 ID，供 `{preset}` 令牌使用。
    pub preset_id: &'a str,
    pub rule: &'a OutputRule,
    /// 同批次内的序号（`{index}` 从 1 开始）。
    pub index: usize,
}

/// 已预留的输出：最终路径 + 引擎真正要写的临时路径。
#[derive(Debug, Clone)]
pub struct ReservedOutput {
    pub final_path: PathBuf,
    pub temp_path: PathBuf,
    /// 源文件（用于提交时的自检）。
    pub source: PathBuf,
}

impl ReservedOutput {
    /// 引擎应该写这个路径。
    pub fn temp(&self) -> &Path {
        &self.temp_path
    }
}

/// 分配失败的两种结果：用户能理解的「跳过」和真正的 I/O 错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AllocError {
    /// 按冲突策略主动跳过（不是错误）。
    Skipped(String),
    /// 创建目录 / 生成临时名失败。
    Io(String),
}

/// 一个 Job 一份分配器：内部维护已预留路径集合。
#[derive(Debug, Default)]
pub struct OutputAllocator {
    taken: Mutex<HashSet<PathBuf>>,
    /// 已经扫过残留临时文件的目录（每个 Job 每个目录只扫一次）。
    swept: Mutex<HashSet<PathBuf>>,
}

impl OutputAllocator {
    pub fn new() -> Self {
        Self::default()
    }

    /// 预留一个输出位置（不创建文件，只创建目录）。
    pub fn reserve(&self, req: &OutputRequest<'_>) -> Result<ReservedOutput, AllocError> {
        let dir = req.rule.dir_or_source(req.source);
        std::fs::create_dir_all(&dir)
            .map_err(|e| AllocError::Io(format!("创建输出目录失败：{e}")))?;
        // 上一次转换被强杀（崩溃/关机/活动监视器）时，用户目录里会永久留下
        // `.<文件名>.clipbase-<uuid>.tmp`。下次往同一个目录输出时顺手收敛。
        self.sweep_stale(&dir);

        let stem = req
            .source
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "output".to_string());

        let mut attempt: u32 = 0;
        loop {
            let name = self.build_name(req, &stem, attempt);
            let candidate = dir.join(&name);

            // 源文件保护优先于任何冲突策略。
            if same_file(&candidate, req.source) {
                attempt = next_attempt(attempt)?;
                continue;
            }

            if candidate.exists() {
                match req.rule.conflict {
                    ConflictPolicy::Rename => {
                        attempt = next_attempt(attempt)?;
                        continue;
                    }
                    ConflictPolicy::Overwrite => {
                        // 覆盖的是**输出**文件，源文件上面已经拦住了。
                    }
                    ConflictPolicy::Skip => {
                        return Err(AllocError::Skipped(format!(
                            "目标文件已存在（策略：跳过）：{}",
                            candidate.display()
                        )))
                    }
                }
            }

            // 同一批次内已经占用：这不是「磁盘上已有旧文件」，
            // 不能拿冲突策略去覆盖——否则两个同名源文件（如 a/photo.png、b/photo.png）
            // 输出到同一目录时会静默丢掉一个结果。
            //
            // `claim` 必须一次加锁完成「查 + 占」：早期是 `is_taken()` 后再
            // `insert()`，两步各自加锁，转换线程池并行跑同名源文件时两边都能
            // 通过检查、抢到同一个路径，后写的把先写的覆盖掉（测试
            // `two_same_name_sources_do_not_collide` 会随机红）。
            if !self.claim(&candidate) {
                attempt = next_attempt(attempt)?;
                continue;
            }
            let temp_path = dir.join(format!(
                ".{}.clipbase-{}.tmp",
                candidate
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("out"),
                uuid::Uuid::new_v4()
            ));
            return Ok(ReservedOutput {
                final_path: candidate,
                temp_path,
                source: req.source.to_path_buf(),
            });
        }
    }

    /// 把临时文件原子地提交到最终路径。
    pub fn commit(&self, reserved: &ReservedOutput) -> Result<PathBuf, String> {
        if !reserved.temp_path.exists() {
            self.release(&reserved.final_path);
            return Err("引擎没有产出任何文件".to_string());
        }
        match std::fs::rename(&reserved.temp_path, &reserved.final_path) {
            Ok(()) => Ok(reserved.final_path.clone()),
            Err(e) => {
                let _ = std::fs::remove_file(&reserved.temp_path);
                self.release(&reserved.final_path);
                Err(format!("提交输出文件失败：{e}"))
            }
        }
    }

    /// 放弃预留：删掉半成品并释放占位。
    pub fn discard(&self, reserved: &ReservedOutput) {
        let _ = std::fs::remove_file(&reserved.temp_path);
        self.release(&reserved.final_path);
    }

    /// 释放一个已预留的路径。
    pub fn release(&self, path: &Path) {
        self.taken.lock_ok().remove(&normalize(path));
    }

    /// 清掉输出目录里上次崩溃留下的半成品（每个目录只扫一次）。
    fn sweep_stale(&self, dir: &Path) {
        if !self.swept.lock_ok().insert(normalize(dir)) {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            // 目录不可读不是致命的：后面的 `create_dir_all` / 写入会报出真原因。
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            // 只认本应用自己生成的临时名：`.<最终名>.clipbase-<uuid>.tmp`。
            if !(name.starts_with('.') && name.contains(".clipbase-") && name.ends_with(".tmp")) {
                continue;
            }
            let stale = entry
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.elapsed().ok())
                .is_some_and(|age| age >= STALE_TEMP_AGE);
            if stale {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }

    /// 原子地「检查并占位」：`false` 表示本批次已被别的线程先占了。
    fn claim(&self, path: &Path) -> bool {
        self.taken.lock_ok().insert(normalize(path))
    }

    /// 展开文件名模板并清洗非法字符。
    fn build_name(&self, req: &OutputRequest<'_>, stem: &str, attempt: u32) -> String {
        let mut name = req.rule.name_template.clone();
        if name.trim().is_empty() {
            name = "{name}.{ext}".to_string();
        }
        let (date, time) = utc_stamp();
        let mut out = name
            .replace("{name}", stem)
            .replace("{ext}", req.target.as_str())
            .replace("{preset}", &sanitize_component(req.preset_id))
            .replace("{date}", &date)
            .replace("{time}", &time)
            .replace("{index}", &req.index.to_string());

        // 保证扩展名与目标一致（模板里漏了 {ext} 时不至于产出没后缀的文件）。
        let suffix = format!(".{}", req.target.as_str());
        if !out.to_ascii_lowercase().ends_with(&suffix) {
            out.push_str(&suffix);
        }
        out = sanitize_component(&out);

        if attempt > 0 {
            // `foo.jpg` → `foo (2).jpg`（Finder 习惯）。
            if let Some(stripped) = out.strip_suffix(&suffix) {
                out = format!("{stripped} ({}){suffix}", attempt + 1);
            } else {
                out.push_str(&format!(" ({})", attempt + 1));
            }
        }
        out
    }
}

fn next_attempt(attempt: u32) -> Result<u32, AllocError> {
    let next = attempt + 1;
    if next > 9999 {
        return Err(AllocError::Io(
            "同一目录下同名文件过多，无法分配输出路径".to_string(),
        ));
    }
    Ok(next)
}

/// 文件名清洗：去掉路径分隔符、控制字符、冒号，以及 Windows 不接受的尾部点/空格。
fn sanitize_component(raw: &str) -> String {
    let mut out: String = raw
        .chars()
        .map(|ch| match ch {
            '/' | '\\' | ':' | '\0' => '-',
            c if c.is_control() => '-',
            c => c,
        })
        .collect();
    while out.ends_with('.') || out.ends_with(' ') {
        out.pop();
    }
    let trimmed = out.trim_start();
    if trimmed.is_empty() {
        "output".to_string()
    } else {
        trimmed.to_string()
    }
}

/// 路径归一（尽量解析符号链接与大小写；不存在时退化为「父目录 + 文件名」）。
fn normalize(path: &Path) -> PathBuf {
    if let Ok(canonical) = std::fs::canonicalize(path) {
        return canonical;
    }
    match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) if !parent.as_os_str().is_empty() => {
            match std::fs::canonicalize(parent) {
                Ok(canon) => canon.join(name),
                Err(_) => path.to_path_buf(),
            }
        }
        _ => path.to_path_buf(),
    }
}

/// 两个路径是否指向同一个文件（含「目标尚不存在」的情况）。
pub fn same_file(a: &Path, b: &Path) -> bool {
    normalize(a) == normalize(b)
}

/// 当前的 UTC 日期 / 时间（`{date}` = `YYYYMMDD`，`{time}` = `HHMMSS`）。
///
/// 只用标准库做公历换算（Howard Hinnant 的 civil_from_days），不引 chrono；
/// 时区固定 UTC——文件名里的日期跨时区差一天不影响正确性。
fn utc_stamp() -> (String, String) {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    let (hh, mm, ss) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    (
        format!("{y:04}{m:02}{d:02}"),
        format!("{hh:02}{mm:02}{ss:02}"),
    )
}

/// 1970-01-01 起的天数 → (年, 月, 日)。
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::convert::preset::OutputRule;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "clipbase-output-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create scratch");
        dir
    }

    fn request<'a>(source: &'a Path, target: ConvertFormat, rule: &'a OutputRule) -> OutputRequest<'a> {
        OutputRequest {
            source,
            target,
            preset_id: "img.jpg",
            rule,
            index: 1,
        }
    }

    #[test]
    fn rename_policy_increments_like_finder() {
        let dir = scratch("rename");
        let src = dir.join("foo.png");
        std::fs::write(&src, b"x").unwrap();
        std::fs::write(dir.join("foo.jpg"), b"old").unwrap();

        let rule = OutputRule::default();
        let alloc = OutputAllocator::new();
        let reserved = alloc.reserve(&request(&src, ConvertFormat::Jpg, &rule)).unwrap();
        assert_eq!(reserved.final_path, dir.join("foo (2).jpg"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 两个文件同时预留同一个空位时，第二个必须拿到别的名字（并发竞态的防线）。
    #[test]
    fn reservations_do_not_collide() {
        let dir = scratch("reserve");
        let src = dir.join("foo.png");
        std::fs::write(&src, b"x").unwrap();
        let rule = OutputRule::default();
        let alloc = OutputAllocator::new();

        let a = alloc.reserve(&request(&src, ConvertFormat::Jpg, &rule)).unwrap();
        let b = alloc.reserve(&request(&src, ConvertFormat::Jpg, &rule)).unwrap();
        assert_ne!(a.final_path, b.final_path);
        // 磁盘上两个文件都还不存在，冲突完全靠预留表解决。
        assert!(!a.final_path.exists() && !b.final_path.exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 并行预留（job 的文件线程池就是这么用的）：8 个线程同时抢同一个空位，
    /// 必须拿到 8 个互不相同的路径。
    ///
    /// 旧实现 `is_taken()` 与 `insert()` 分两次加锁，「检查」与「占位」之间
    /// 有窗口：并发时两个线程能同时通过检查抢到同一路径，最后互相覆盖。
    #[test]
    fn concurrent_reservations_are_atomic() {
        let dir = scratch("reserve-race");
        let src = dir.join("foo.png");
        std::fs::write(&src, b"x").unwrap();
        let rule = OutputRule::default();
        let alloc = OutputAllocator::new();
        const THREADS: usize = 8;

        let paths: Vec<PathBuf> = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..THREADS)
                .map(|_| {
                    scope.spawn(|| {
                        alloc
                            .reserve(&request(&src, ConvertFormat::Jpg, &rule))
                            .expect("reserve should succeed")
                            .final_path
                    })
                })
                .collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });

        let unique: std::collections::HashSet<&PathBuf> = paths.iter().collect();
        assert_eq!(unique.len(), THREADS, "并发预留拿到了重复路径：{paths:?}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Overwrite 也不能覆盖源文件：同格式重编码必须落到 `foo (2).png`。
    #[test]
    fn overwrite_never_touches_the_source() {
        let dir = scratch("source-guard");
        let src = dir.join("foo.png");
        std::fs::write(&src, b"source").unwrap();

        let rule = OutputRule {
            conflict: ConflictPolicy::Overwrite,
            ..OutputRule::default()
        };
        let alloc = OutputAllocator::new();
        let reserved = alloc.reserve(&request(&src, ConvertFormat::Png, &rule)).unwrap();
        assert_eq!(reserved.final_path, dir.join("foo (2).png"));
        assert_eq!(std::fs::read(&src).unwrap(), b"source");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 同批次里两个同名源文件即使策略是 Overwrite，也必须分别拿到不同名字：
    /// 覆盖只针对「磁盘上已有的旧文件」，不能把自己这一批的结果盖掉。
    #[test]
    fn overwrite_still_separates_names_inside_one_batch() {
        let dir = scratch("batch-overwrite");
        let nested = dir.join("sub");
        std::fs::create_dir_all(&nested).unwrap();
        let a = dir.join("photo.png");
        let b = nested.join("photo.png");
        std::fs::write(&a, b"a").unwrap();
        std::fs::write(&b, b"b").unwrap();

        // 两份都输出到同一个固定目录，且策略是覆盖。
        let out = dir.join("out");
        let rule = OutputRule {
            conflict: ConflictPolicy::Overwrite,
            dir: out.to_string_lossy().to_string(),
            ..OutputRule::default()
        };
        let alloc = OutputAllocator::new();
        let first = alloc.reserve(&request(&a, ConvertFormat::Jpg, &rule)).unwrap();
        let second = alloc.reserve(&request(&b, ConvertFormat::Jpg, &rule)).unwrap();
        assert_ne!(first.final_path, second.final_path);
        assert_eq!(second.final_path, out.join("photo (2).jpg"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn overwrite_replaces_existing_output() {
        let dir = scratch("overwrite");
        let src = dir.join("foo.png");
        std::fs::write(&src, b"x").unwrap();
        std::fs::write(dir.join("foo.jpg"), b"old").unwrap();

        let rule = OutputRule {
            conflict: ConflictPolicy::Overwrite,
            ..OutputRule::default()
        };
        let alloc = OutputAllocator::new();
        let reserved = alloc.reserve(&request(&src, ConvertFormat::Jpg, &rule)).unwrap();
        assert_eq!(reserved.final_path, dir.join("foo.jpg"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn skip_policy_reports_a_skip_not_an_error() {
        let dir = scratch("skip");
        let src = dir.join("foo.png");
        std::fs::write(&src, b"x").unwrap();
        std::fs::write(dir.join("foo.jpg"), b"old").unwrap();

        let rule = OutputRule {
            conflict: ConflictPolicy::Skip,
            ..OutputRule::default()
        };
        let alloc = OutputAllocator::new();
        match alloc.reserve(&request(&src, ConvertFormat::Jpg, &rule)) {
            Err(AllocError::Skipped(_)) => {}
            other => panic!("应跳过，实际 {other:?}"),
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn commit_moves_temp_atomically_and_discard_cleans_up() {
        let dir = scratch("commit");
        let src = dir.join("foo.png");
        std::fs::write(&src, b"x").unwrap();
        let rule = OutputRule::default();
        let alloc = OutputAllocator::new();

        let reserved = alloc.reserve(&request(&src, ConvertFormat::Jpg, &rule)).unwrap();
        std::fs::write(reserved.temp(), b"done").unwrap();
        let final_path = alloc.commit(&reserved).unwrap();
        assert_eq!(std::fs::read(&final_path).unwrap(), b"done");
        assert!(!reserved.temp_path.exists());

        let reserved2 = alloc.reserve(&request(&src, ConvertFormat::Jpg, &rule)).unwrap();
        std::fs::write(reserved2.temp(), b"half").unwrap();
        alloc.discard(&reserved2);
        assert!(!reserved2.temp_path.exists());
        assert!(!reserved2.final_path.exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn name_tokens_expand_and_sanitize() {
        let dir = scratch("tokens");
        let src = dir.join("报告 v1:final.png");
        std::fs::write(&src, b"x").unwrap();
        let rule = OutputRule {
            name_template: "{name}-{preset}-{index}.{ext}".into(),
            ..OutputRule::default()
        };
        let alloc = OutputAllocator::new();
        let reserved = alloc.reserve(&request(&src, ConvertFormat::Jpg, &rule)).unwrap();
        let name = reserved.final_path.file_name().unwrap().to_str().unwrap();
        assert_eq!(name, "报告 v1-final-img.jpg-1.jpg");
        assert!(!name.contains(':'), "{name}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn custom_directory_is_created() {
        let dir = scratch("destdir");
        let src = dir.join("foo.png");
        std::fs::write(&src, b"x").unwrap();
        let nested = dir.join("out/deep");
        let rule = OutputRule {
            dir: nested.to_string_lossy().to_string(),
            ..OutputRule::default()
        };
        let alloc = OutputAllocator::new();
        let reserved = alloc.reserve(&request(&src, ConvertFormat::Jpg, &rule)).unwrap();
        assert_eq!(reserved.final_path, nested.join("foo.jpg"));
        assert!(nested.is_dir());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn civil_dates_are_correct_for_known_days() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_000), (2022, 1, 8));
        assert_eq!(civil_from_days(20_000), (2024, 10, 4));
    }
}
