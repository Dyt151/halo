//! 转换任务（Job）：文件快照 + 配方快照 + 逐文件结果 + 取消 + 单调进度。
//!
//! 对照 docs/REA-Filee-Conversion.md §3 / §8.2 / §8.5：
//! - 每个输入都有一条 `ConversionFileResult`，**禁止静默忽略**（`_ => {}` 不许再出现）；
//! - 跳过与失败是两种状态，跳过还带结构化原因（空文件 / 格式不认识 / 已是目标 / 无路由 / 冲突跳过）；
//! - 进度按「步」单调推进，不用「已完成文件数 / 总数」冒充单文件进度；
//! - 三层并发各自独立：Job 由调用方串行化、文件按 worker 并行、每个引擎另有上限。
//!
//! 这一层不认识 Tauri：进度通过回调抛出去，谁需要谁订阅。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use super::edge::{ConversionStep, ConvertError, EngineId};
use super::engines;
use super::format::{detect_format, ConvertFormat};
use super::output::{AllocError, OutputAllocator, OutputRequest};
use super::preset::ConversionPreset;
use super::router::{self, NoRoute};
use crate::lock_ext::LockExt;

/// 单个文件的并行度上限（文件层）。
const MAX_FILE_WORKERS: usize = 4;

/// 逐文件状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FileState {
    Pending,
    Running,
    Done,
    Skipped,
    Failed,
    Cancelled,
}

/// 跳过原因码（结构化，前端可分类展示）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkipCode {
    /// 0 字节文件。
    EmptyFile,
    /// 扩展名不认识。
    UnknownFormat,
    /// 已经是目标格式，且预设没要求重编码。
    AlreadyTarget,
    /// 图上没有可达路径（需要 OCR / 排版引擎）。
    NoRoute,
    /// 按冲突策略跳过。
    Conflict,
    /// 结果不比已有文件小。
    OnlyIfSmaller,
    /// 引擎不可用。
    EngineMissing,
}

/// 跳过原因。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SkipReason {
    pub code: SkipCode,
    pub detail: String,
}

impl SkipReason {
    pub fn new(code: SkipCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }
}

/// 逐文件结果（§8.2 的 `ConversionFileResult`）。
#[derive(Debug, Clone, Serialize)]
pub struct ConversionFileResult {
    pub source: String,
    pub detected_format: Option<String>,
    pub target_format: Option<String>,
    pub state: FileState,
    /// 0…1，单调不回退。
    pub progress: f32,
    pub outputs: Vec<String>,
    pub skip: Option<SkipReason>,
    pub error: Option<ConvertError>,
    /// 无法保真但已完成时必须让用户知道（动画只剩首帧、ICC 未保留……）。
    pub warnings: Vec<String>,
    /// 实际走过的格式链（`["png","webp"]`），便于排查「为什么转了两步」。
    pub route: Vec<String>,
}

impl ConversionFileResult {
    fn pending(source: &str) -> Self {
        Self {
            source: source.to_string(),
            detected_format: None,
            target_format: None,
            state: FileState::Pending,
            progress: 0.0,
            outputs: Vec::new(),
            skip: None,
            error: None,
            warnings: Vec::new(),
            route: Vec::new(),
        }
    }

    fn skip_with(mut self, code: SkipCode, detail: impl Into<String>) -> Self {
        self.state = FileState::Skipped;
        self.skip = Some(SkipReason::new(code, detail));
        self
    }

    fn fail(mut self, error: ConvertError) -> Self {
        self.state = FileState::Failed;
        self.error = Some(error);
        self
    }
}

/// Job 级状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Queued,
    Running,
    Completed,
    CompletedWithErrors,
    Failed,
    Cancelled,
}

/// 取消令牌（Job 级）。
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// 一个转换任务。`preset` 是启动时的快照，之后改配置不影响它（§2.2）。
#[derive(Debug, Clone, Serialize)]
pub struct ConversionJob {
    pub id: String,
    pub preset: ConversionPreset,
    pub sources: Vec<String>,
    pub state: JobState,
    pub files: Vec<ConversionFileResult>,
    pub created_at: u64,
}

impl ConversionJob {
    pub fn converted(&self) -> usize {
        self.files
            .iter()
            .filter(|f| f.state == FileState::Done)
            .count()
    }

    pub fn skipped(&self) -> usize {
        self.files
            .iter()
            .filter(|f| f.state == FileState::Skipped)
            .count()
    }

    pub fn failed(&self) -> usize {
        self.files
            .iter()
            .filter(|f| f.state == FileState::Failed)
            .count()
    }

    pub fn cancelled(&self) -> usize {
        self.files
            .iter()
            .filter(|f| f.state == FileState::Cancelled)
            .count()
    }

    /// 全部产物路径（多输出时顺序即文件顺序）。
    pub fn outputs(&self) -> Vec<String> {
        self.files
            .iter()
            .flat_map(|f| f.outputs.iter().cloned())
            .collect()
    }

    /// 全部失败 / 跳过的可读原因。
    pub fn reasons(&self) -> Vec<String> {
        self.files
            .iter()
            .filter_map(|f| {
                f.error
                    .as_ref()
                    .map(|e| e.message.clone())
                    .or_else(|| f.skip.as_ref().map(|s| s.detail.clone()))
            })
            .collect()
    }
}

/// 进度回调：每完成一个文件调用一次（含跳过与失败）。
pub type UpdateFn<'a> = &'a mut dyn FnMut(&ConversionFileResult, usize, usize);

/// 当前正在跑的 Job（正常只有一个；这里用列表是为了**不会因为多次提交而
/// 丢掉前一个 Job 的取消令牌**——丢了就再也无法 Esc 取消它）。
static ACTIVE: Mutex<Vec<(String, CancelToken)>> = Mutex::new(Vec::new());

/// 取消正在运行的 Job；返回是否真有任务被取消。
pub fn cancel_active() -> bool {
    let guard = ACTIVE.lock_ok();
    if guard.is_empty() {
        return false;
    }
    for (_, token) in guard.iter() {
        token.cancel();
    }
    true
}

/// 最近一个正在运行的 Job ID（UI 层用它拦重复提交）。
pub fn active_job_id() -> Option<String> {
    ACTIVE.lock_ok().last().map(|(id, _)| id.clone())
}

fn register(id: &str, token: &CancelToken) {
    ACTIVE
        .lock_ok()
        .push((id.to_string(), token.clone()));
}

fn unregister(id: &str) {
    ACTIVE.lock_ok().retain(|(active, _)| active != id);
}

/// 引擎层限流：每个引擎一份信号量，上限来自 `ConverterEngine::max_parallelism`。
struct EngineGate {
    max: usize,
    active: Mutex<usize>,
    ready: Condvar,
}

impl EngineGate {
    fn new(max: usize) -> Self {
        Self {
            max: max.max(1),
            active: Mutex::new(0),
            ready: Condvar::new(),
        }
    }

    /// 取一个配额；被取消时返回 `None`（不会无限等待）。
    fn acquire(self: &Arc<Self>, cancel: &CancelToken) -> Option<GateGuard> {
        let mut active = self.active.lock_ok();
        while *active >= self.max {
            if cancel.is_cancelled() {
                return None;
            }
            // 中毒时照旧取用内部计数：这里是等待线程，panic 的只是别的 worker，
            // 让等待方跟着 panic 会把整个转换线程池一起带走。
            let (guard, _) = self
                .ready
                .wait_timeout(active, std::time::Duration::from_millis(50))
                .unwrap_or_else(|e| e.into_inner());
            active = guard;
        }
        if cancel.is_cancelled() {
            return None;
        }
        *active += 1;
        Some(GateGuard {
            gate: Arc::clone(self),
        })
    }
}

struct GateGuard {
    gate: Arc<EngineGate>,
}

impl Drop for GateGuard {
    fn drop(&mut self) {
        let mut active = self.gate.active.lock_ok();
        *active = active.saturating_sub(1);
        self.gate.ready.notify_one();
    }
}

type EnginePool = Arc<Mutex<HashMap<EngineId, Arc<EngineGate>>>>;

fn engine_pool() -> EnginePool {
    let mut map: HashMap<EngineId, Arc<EngineGate>> = HashMap::new();
    for engine in engines::engines() {
        map.insert(engine.id(), Arc::new(EngineGate::new(engine.max_parallelism())));
    }
    Arc::new(Mutex::new(map))
}

fn gate_of(pool: &EnginePool, id: EngineId) -> Option<Arc<EngineGate>> {
    pool.lock_ok().get(&id).cloned()
}

/// 执行一个 Job（阻塞，直到所有文件出结果），使用独立的取消令牌。
///
/// 调用方负责串行化 Job（环形盘在 `Converting` 状态时不会重复提交）。
pub fn run(sources: &[String], preset: &ConversionPreset, on_update: UpdateFn<'_>) -> ConversionJob {
    run_with_cancel(sources, preset, CancelToken::new(), on_update)
}

/// 执行一个 Job，取消由外部令牌控制（命令层 / 环形盘 Esc）。
pub fn run_with_cancel(
    sources: &[String],
    preset: &ConversionPreset,
    cancel: CancelToken,
    on_update: UpdateFn<'_>,
) -> ConversionJob {
    let job_id = uuid::Uuid::new_v4().to_string();
    let mut job = ConversionJob {
        id: job_id.clone(),
        preset: preset.clone(),
        sources: sources.to_vec(),
        state: JobState::Queued,
        files: sources
            .iter()
            .map(|s| ConversionFileResult::pending(s))
            .collect(),
        created_at: now_ms(),
    };

    if sources.is_empty() {
        job.state = JobState::Completed;
        return job;
    }

    register(&job_id, &cancel);
    job.state = JobState::Running;

    let work_dir = std::env::temp_dir().join(format!("clipbase-convert-{job_id}"));
    if let Err(e) = std::fs::create_dir_all(&work_dir) {
        // 临时目录建不出来时多步转换必然失败：与其让每个 worker 各报一句
        // 看不懂的 io 错误，不如把真原因一次性写进每个文件结果（进度事件
        // 也就不会推送一个永远停在 pending 的列表）。
        for index in 0..job.files.len() {
            let source = job.files[index].source.clone();
            job.files[index] = ConversionFileResult::pending(&source)
                .fail(ConvertError::io(format!("无法创建临时工作目录：{e}")));
        }
        job.state = JobState::CompletedWithErrors;
        unregister(&job_id);
        return job;
    }
    let allocator = Arc::new(OutputAllocator::new());
    let pool = engine_pool();

    let total = sources.len();
    // 文件级并行：1 个文件就不必开线程，多文件最多 `MAX_FILE_WORKERS` 个。
    let workers = total.clamp(1, MAX_FILE_WORKERS);
    let next = AtomicUsize::new(0);
    let (tx, rx) = mpsc::channel::<(usize, ConversionFileResult)>();

    {
        let on_update = &mut *on_update;
        std::thread::scope(|scope| {
            for _ in 0..workers {
                let tx = tx.clone();
                let allocator = Arc::clone(&allocator);
                let pool = Arc::clone(&pool);
                let work_dir = work_dir.clone();
                let cancel = cancel.clone();
                let next = &next;
                scope.spawn(move || loop {
                    let index = next.fetch_add(1, Ordering::SeqCst);
                    if index >= total {
                        break;
                    }
                    let result = convert_one(
                        index,
                        &sources[index],
                        preset,
                        &allocator,
                        &pool,
                        &work_dir,
                        &cancel,
                    );
                    if tx.send((index, result)).is_err() {
                        break;
                    }
                });
            }
            // 主线程留一份 sender 会让 rx 永远不结束。
            drop(tx);

            let mut done = 0usize;
            for (index, result) in rx {
                done += 1;
                job.files[index] = result;
                on_update(&job.files[index], done, total);
            }
        });
    }

    let _ = std::fs::remove_dir_all(&work_dir);
    unregister(&job_id);

    job.state = if cancel.is_cancelled() {
        JobState::Cancelled
    } else if job.failed() > 0 {
        JobState::CompletedWithErrors
    } else {
        JobState::Completed
    };
    job
}

/// 单个文件的完整流水线：Detect → Plan → Execute → Finalize（§3.1）。
fn convert_one(
    index: usize,
    source: &str,
    preset: &ConversionPreset,
    allocator: &OutputAllocator,
    pool: &EnginePool,
    work_dir: &Path,
    cancel: &CancelToken,
) -> ConversionFileResult {
    let path = Path::new(source);
    let mut result = ConversionFileResult::pending(source);

    if cancel.is_cancelled() {
        result.state = FileState::Cancelled;
        result.error = Some(ConvertError::cancelled());
        return result;
    }

    // 1. 空文件 / 非普通文件：明确报出来，不当成「格式不支持」。
    match std::fs::metadata(path) {
        // 文件夹、符号设备等：扩展名可能刚好对上（`新建文件夹.png`），
        // 交给引擎读只会得到一句含糊的 io 错误。
        Ok(meta) if !meta.is_file() => {
            return result.fail(ConvertError::io("不是普通文件（文件夹或特殊设备无法转换）"))
        }
        Ok(meta) if meta.len() == 0 => {
            return result.skip_with(SkipCode::EmptyFile, "文件为空（0 字节），没有可转换的内容")
        }
        Ok(_) => {}
        Err(e) => return result.fail(ConvertError::io(format!("读取文件信息失败：{e}"))),
    }

    // 2. 格式探测。
    let Some(from) = detect_format(source) else {
        return result.skip_with(
            SkipCode::UnknownFormat,
            "不是 ClipBase 支持的格式（图片 / Markdown / HTML / TXT）",
        );
    };
    result.detected_format = Some(from.as_str().to_string());

    // 3. 目标格式（SameAsSource 时等于源格式）。
    let to = preset.target_format(from);
    result.target_format = Some(to.as_str().to_string());

    // 4. 已是目标格式且预设没要求重编码 → 跳过（不是失败）。
    if preset.is_already_target(from) {
        return result.skip_with(
            SkipCode::AlreadyTarget,
            format!("已经是 {}，这个预设不需要重新编码", to.label()),
        );
    }

    // 5. 规划路由。
    let route = match router::plan(from, to, preset.reencode_same_format) {
        Ok(route) => route,
        Err(reason) => {
            let code = match reason {
                NoRoute::SameFormat => SkipCode::AlreadyTarget,
                _ => SkipCode::NoRoute,
            };
            return result.skip_with(code, reason.describe(from, to));
        }
    };
    result.route = route
        .formats()
        .iter()
        .map(|f| f.as_str().to_string())
        .collect();

    // 6. 预留输出位置（源文件保护 + 并发预留都在这里）。
    let request = OutputRequest {
        source: path,
        target: to,
        preset_id: &preset.id,
        rule: &preset.output,
        index: index + 1,
    };
    let reserved = match allocator.reserve(&request) {
        Ok(reserved) => reserved,
        Err(AllocError::Skipped(detail)) => return result.skip_with(SkipCode::Conflict, detail),
        Err(AllocError::Io(detail)) => return result.fail(ConvertError::io(detail)),
    };
    result.state = FileState::Running;

    // 7. 逐步执行。
    let step_count = route.steps.len();
    let mut input = path.to_path_buf();
    let mut intermediates: Vec<PathBuf> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    for (step_index, step) in route.steps.iter().enumerate() {
        if cancel.is_cancelled() {
            cleanup_intermediates(&intermediates);
            allocator.discard(&reserved);
            result.state = FileState::Cancelled;
            result.error = Some(ConvertError::cancelled());
            return result;
        }

        let is_final = step_index + 1 == step_count;
        let output = if is_final {
            reserved.temp_path.clone()
        } else {
            let intermediate = work_dir.join(format!(
                "{index}-step{}.{}",
                step_index + 1,
                step.to.as_str()
            ));
            intermediates.push(intermediate.clone());
            intermediate
        };

        let Some(engine) = engines::engine(step.engine) else {
            cleanup_intermediates(&intermediates);
            allocator.discard(&reserved);
            return result.fail(ConvertError::new(
                "missing_engine",
                format!("引擎 {} 不可用", step.engine.as_str()),
            ));
        };
        if let super::edge::EngineStatus::Missing(reason) = engine.status() {
            cleanup_intermediates(&intermediates);
            allocator.discard(&reserved);
            return result.fail(ConvertError::new("missing_engine", reason));
        }

        let Some(gate) = gate_of(pool, step.engine) else {
            cleanup_intermediates(&intermediates);
            allocator.discard(&reserved);
            return result.fail(ConvertError::new(
                "missing_engine",
                format!("引擎 {} 没有注册并发配额", step.engine.as_str()),
            ));
        };
        let Some(_permit) = gate.acquire(cancel) else {
            cleanup_intermediates(&intermediates);
            allocator.discard(&reserved);
            result.state = FileState::Cancelled;
            result.error = Some(ConvertError::cancelled());
            return result;
        };

        let conversion = ConversionStep {
            input: &input,
            output: &output,
            from: step.from,
            to: step.to,
            final_format: to,
            preset,
            original: path,
            is_final,
        };
        match engine.convert(&conversion) {
            Ok(step_output) => warnings.extend(step_output.warnings),
            Err(error) => {
                cleanup_intermediates(&intermediates);
                allocator.discard(&reserved);
                return result.fail(error);
            }
        }

        // 进度按步推进，只会变大（§3.3 单调）。
        let progressed = (step_index + 1) as f32 / step_count as f32;
        result.progress = result.progress.max(progressed);
        input = output;
    }

    // 8. OnlyIfSmaller：不比已有输出小就丢弃。
    if preset.output.only_if_smaller {
        let new_size = std::fs::metadata(&reserved.temp_path).map(|m| m.len());
        let old_size = std::fs::metadata(&reserved.final_path).map(|m| m.len());
        if let (Ok(new_size), Ok(old_size)) = (new_size, old_size) {
            if new_size >= old_size {
                cleanup_intermediates(&intermediates);
                allocator.discard(&reserved);
                return result.skip_with(
                    SkipCode::OnlyIfSmaller,
                    format!(
                        "新文件 {new_size} 字节不小于已有文件 {old_size} 字节，按规则保留原文件"
                    ),
                );
            }
        }
    }

    // 9. 原子提交。
    let committed = allocator.commit(&reserved);
    cleanup_intermediates(&intermediates);
    match committed {
        Ok(final_path) => {
            result.state = FileState::Done;
            result.progress = 1.0;
            result.outputs = vec![final_path.to_string_lossy().to_string()];
            result.warnings = warnings;
            result
        }
        Err(message) => result.fail(ConvertError::io(message)),
    }
}

fn cleanup_intermediates(paths: &[PathBuf]) {
    for path in paths {
        let _ = std::fs::remove_file(path);
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 判断一批文件在某个预设下是否可用（§2.5 的 `PresetAvailability`）。
///
/// 规则：至少一个文件可执行即可用；同格式是否可执行由路由图回答，
/// 不在这里猜。返回 (可执行数, 跳过数, 首个不可用原因)。
pub fn preset_availability(
    preset: &ConversionPreset,
    formats: &[Option<ConvertFormat>],
) -> (usize, usize, Option<String>) {
    let mut convertible = 0usize;
    let mut skipped = 0usize;
    let mut first_reason: Option<String> = None;

    for format in formats {
        let Some(from) = format else {
            skipped += 1;
            first_reason.get_or_insert_with(|| "有文件不是支持的格式".to_string());
            continue;
        };
        if preset.is_already_target(*from) {
            skipped += 1;
            first_reason.get_or_insert_with(|| {
                format!("已经是 {}，这个预设不需要重新编码", from.label())
            });
            continue;
        }
        let to = preset.target_format(*from);
        match router::plan(*from, to, preset.reencode_same_format) {
            Ok(_) => convertible += 1,
            Err(reason) => {
                skipped += 1;
                first_reason.get_or_insert_with(|| reason.describe(*from, to));
            }
        }
    }

    (convertible, skipped, first_reason)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::convert::preset::{ConflictPolicy, ImageOptions, OutputRule, TargetSpec};

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "clipbase-job-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create scratch");
        dir
    }

    fn preset_for(target: ConvertFormat, out_dir: &Path) -> ConversionPreset {
        ConversionPreset {
            id: "test".into(),
            name: "test".into(),
            target: TargetSpec::Explicit(target),
            image: ImageOptions::default(),
            output: OutputRule {
                dir: out_dir.to_string_lossy().to_string(),
                ..OutputRule::default()
            },
            reencode_same_format: false,
            builtin: false,
        }
    }

    fn write_png(path: &Path) {
        image::RgbImage::new(4, 3).save(path).expect("save png");
    }

    #[test]
    fn every_input_gets_a_result() {
        let dir = scratch("results");
        let out = dir.join("out");
        let png = dir.join("a.png");
        write_png(&png);
        let unknown = dir.join("b.bin");
        std::fs::write(&unknown, b"data").unwrap();
        let empty = dir.join("c.txt");
        std::fs::write(&empty, b"").unwrap();

        let sources = vec![
            png.to_string_lossy().to_string(),
            unknown.to_string_lossy().to_string(),
            empty.to_string_lossy().to_string(),
            dir.join("missing.png").to_string_lossy().to_string(),
        ];
        let preset = preset_for(ConvertFormat::Jpg, &out);
        let job = run(&sources, &preset, &mut |_, _, _| {});

        assert_eq!(job.files.len(), 4);
        assert_eq!(job.files[0].state, FileState::Done);
        assert_eq!(job.files[1].state, FileState::Skipped);
        assert_eq!(
            job.files[1].skip.as_ref().unwrap().code,
            SkipCode::UnknownFormat
        );
        assert_eq!(job.files[2].state, FileState::Skipped);
        assert_eq!(job.files[2].skip.as_ref().unwrap().code, SkipCode::EmptyFile);
        assert_eq!(job.files[3].state, FileState::Failed);
        assert_eq!(job.state, JobState::CompletedWithErrors);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn already_target_is_skipped_with_reason() {
        let dir = scratch("same-format");
        let out = dir.join("out");
        let png = dir.join("a.png");
        write_png(&png);
        let preset = preset_for(ConvertFormat::Png, &out);
        let job = run(&[png.to_string_lossy().to_string()], &preset, &mut |_, _, _| {});
        assert_eq!(job.files[0].state, FileState::Skipped);
        let skip = job.files[0].skip.as_ref().unwrap();
        assert_eq!(skip.code, SkipCode::AlreadyTarget);
        assert!(skip.detail.contains("已经是"), "{}", skip.detail);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 同格式 + 重编码：JPG 降质量必须真的执行，而不是一律按「同格式」拒绝。
    #[test]
    fn same_format_reencode_runs_when_preset_asks() {
        let dir = scratch("reencode");
        let out = dir.join("out");
        let jpg = dir.join("a.jpg");
        let img = image::RgbImage::new(32, 32);
        img.save(&jpg).unwrap();

        let mut preset = preset_for(ConvertFormat::Jpg, &out);
        preset.reencode_same_format = true;
        preset.image.jpeg_quality = 40;
        let job = run(&[jpg.to_string_lossy().to_string()], &preset, &mut |_, _, _| {});
        assert_eq!(job.files[0].state, FileState::Done, "{:?}", job.files[0]);
        assert_eq!(job.files[0].route, vec!["jpg", "jpg"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn source_file_is_never_modified() {
        let dir = scratch("source-guard");
        let png = dir.join("a.png");
        write_png(&png);
        let before = std::fs::read(&png).unwrap();

        // 输出目录 = 源目录 + Overwrite：源文件仍然不能被碰。
        let preset = ConversionPreset {
            id: "test".into(),
            name: "test".into(),
            target: TargetSpec::Explicit(ConvertFormat::Png),
            output: OutputRule {
                conflict: ConflictPolicy::Overwrite,
                ..OutputRule::default()
            },
            reencode_same_format: true,
            image: ImageOptions::default(),
            builtin: false,
        };
        let job = run(&[png.to_string_lossy().to_string()], &preset, &mut |_, _, _| {});
        assert_eq!(job.files[0].state, FileState::Done, "{:?}", job.files[0]);
        assert_eq!(std::fs::read(&png).unwrap(), before);
        assert!(job.files[0].outputs[0].ends_with("a (2).png"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn two_same_name_sources_do_not_collide() {
        let dir = scratch("collide");
        let out = dir.join("out");
        std::fs::create_dir_all(dir.join("x")).unwrap();
        std::fs::create_dir_all(dir.join("y")).unwrap();
        let a = dir.join("x/pic.png");
        let b = dir.join("y/pic.png");
        write_png(&a);
        write_png(&b);

        let preset = preset_for(ConvertFormat::Jpg, &out);
        let job = run(
            &[
                a.to_string_lossy().to_string(),
                b.to_string_lossy().to_string(),
            ],
            &preset,
            &mut |_, _, _| {},
        );
        assert_eq!(job.converted(), 2, "{:?}", job.files);
        let mut names: Vec<String> = job.outputs();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), 2, "同名来源互相覆盖了：{names:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn progress_is_monotonic_and_finishes_at_one() {
        let dir = scratch("progress");
        let out = dir.join("out");
        let txt = dir.join("a.txt");
        std::fs::write(&txt, "hello").unwrap();
        let preset = preset_for(ConvertFormat::Html, &out);
        let mut seen: Vec<f32> = Vec::new();
        let job = run(&[txt.to_string_lossy().to_string()], &preset, &mut |f, done, total| {
            assert_eq!(done, total);
            seen.push(f.progress);
        });
        assert_eq!(job.files[0].state, FileState::Done, "{:?}", job.files[0]);
        assert!(seen.windows(2).all(|w| w[0] <= w[1]), "{seen:?}");
        assert_eq!(*seen.last().unwrap(), 1.0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn availability_uses_the_router_not_guesses() {
        let out = std::env::temp_dir();
        let preset = preset_for(ConvertFormat::Jpg, &out);
        // png 可转、txt 不可转（需要排版引擎）。
        let (ok, skip, reason) = preset_availability(
            &preset,
            &[Some(ConvertFormat::Png), Some(ConvertFormat::Txt)],
        );
        assert_eq!(ok, 1);
        assert_eq!(skip, 1);
        assert!(reason.is_some());

        // 全是 JPG 时整条预设不可用（已是目标）。
        let (ok, skip, _) = preset_availability(&preset, &[Some(ConvertFormat::Jpg)]);
        assert_eq!((ok, skip), (0, 1));
    }
}
