//! 转换引擎与「边」的契约（docs/REA-Filee-Conversion.md §2.3 / §8.3）。
//!
//! 引擎只回答两件事：我能直接完成哪些边、给我一步怎么执行。
//! 「从 A 到 B 怎么走」由 `router.rs` 用所有引擎声明的边一起规划；
//! 「写到哪个路径、重名怎么办」由 `output.rs` 的分配器负责。
//!
//! 引擎的三条硬约束（§8.3 末尾）：
//! 1. 不能读取用户配置（只拿 `ConversionStep` 里的 preset 快照）；
//! 2. 不能自行决定冲突策略；
//! 3. 绝不修改输入文件。

use std::path::Path;

use serde::Serialize;

use super::format::ConvertFormat;
use super::preset::ConversionPreset;

/// 引擎标识。新增引擎时同时要在这里、`engines/mod.rs` 的注册表里各加一处。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EngineId {
    /// 图片编解码（`image` crate）。
    Image,
    /// 文本互转（Markdown / HTML / TXT）。
    Text,
    /// 图片 → 引用图文档。
    Document,
}

impl EngineId {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Text => "text",
            Self::Document => "document",
        }
    }
}

/// 边的执行模式。
///
/// - `Normal`：可以出现在路由的任意位置；
/// - `Terminal`：只能作为**最后一步**（图片 → 文档就是这种：它产出的是引用
///   容器的文档，不能再被别的引擎当输入继续绕路，否则 `png → md → txt` 会
///   凭空造出「图片转文本」，等于偷偷替代了 OCR）；
/// - `Animated` / `Ocr`：特殊模式，v1 不声明任何这种边，也不参与普通选路。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeMode {
    Normal,
    Terminal,
    Animated,
    Ocr,
}

impl EdgeMode {
    /// 是否允许作为路径的中间步。
    pub fn is_chainable(&self) -> bool {
        matches!(self, Self::Normal)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Terminal => "terminal",
            Self::Animated => "animated",
            Self::Ocr => "ocr",
        }
    }
}

/// 一条有向转换边：`from` 通过 `engine` 直达 `to`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConversionEdge {
    pub from: ConvertFormat,
    pub to: ConvertFormat,
    pub engine: EngineId,
    /// 相对成本（越低越优先）；用于「直连引擎优先于绕路」。
    pub cost: u32,
    pub mode: EdgeMode,
}

impl ConversionEdge {
    pub const fn new(
        from: ConvertFormat,
        to: ConvertFormat,
        engine: EngineId,
        cost: u32,
        mode: EdgeMode,
    ) -> Self {
        Self {
            from,
            to,
            engine,
            cost,
            mode,
        }
    }

    /// 是否为 self edge（同格式重编码）。
    pub fn is_self(&self) -> bool {
        self.from == self.to
    }
}

/// 引擎可用性。单个引擎不可用只让它自己的边消失，不影响整张图（§2.4）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineStatus {
    Available,
    Missing(String),
}

impl EngineStatus {
    pub fn is_available(&self) -> bool {
        matches!(self, Self::Available)
    }
}

/// 结构化错误：`code` 供前端分类，`message` 给人看。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConvertError {
    pub code: String,
    pub message: String,
}

impl ConvertError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
        }
    }

    pub fn io(message: impl Into<String>) -> Self {
        Self::new("io", message)
    }

    pub fn decode(message: impl Into<String>) -> Self {
        Self::new("decode", message)
    }

    pub fn encode(message: impl Into<String>) -> Self {
        Self::new("encode", message)
    }

    pub fn unsupported(message: impl Into<String>) -> Self {
        Self::new("unsupported", message)
    }

    pub fn cancelled() -> Self {
        Self::new("cancelled", "转换已取消")
    }
}

impl std::fmt::Display for ConvertError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

/// 一次引擎调用的输入。引擎只允许写 `output`，不许碰 `input`。
pub struct ConversionStep<'a> {
    /// 本步输入文件（可能是上一步的中间产物）。
    pub input: &'a Path,
    /// 本步要写出的文件（最后一步指向分配器给的临时文件）。
    pub output: &'a Path,
    pub from: ConvertFormat,
    pub to: ConvertFormat,
    /// 整条路由的最终格式（多步时与 `to` 不同）。
    pub final_format: ConvertFormat,
    /// 配方快照。
    pub preset: &'a ConversionPreset,
    /// 用户原始选中的文件（不是中间产物）。
    pub original: &'a Path,
    /// 是否最后一步（只有最后一步代表最终产物）。
    pub is_final: bool,
}

/// 引擎执行结果：只说「我写了什么、有哪些不得不声明的限制」。
#[derive(Debug, Clone, Default)]
pub struct StepOutput {
    /// 无法保真但已完成时必须说明的限制（动画只保留首帧、ICC 未保留……）。
    pub warnings: Vec<String>,
}

impl StepOutput {
    pub fn warn(mut self, message: impl Into<String>) -> Self {
        self.warnings.push(message.into());
        self
    }
}

/// 转换引擎契约。
pub trait ConverterEngine: Send + Sync {
    fn id(&self) -> EngineId;

    fn display_name(&self) -> &'static str;

    /// 本引擎能直接完成的边（`Normal` 模式）。
    fn edges(&self) -> Vec<ConversionEdge>;

    /// 本引擎允许的并发数（§3.4 的引擎层限流）。
    fn max_parallelism(&self) -> usize;

    /// 引擎是否可用。未安装外部引擎时应返回 `Missing(原因)`。
    fn status(&self) -> EngineStatus;

    /// 执行一步。成功时 `step.output` 必须已经写完并关闭。
    fn convert(&self, step: &ConversionStep<'_>) -> Result<StepOutput, ConvertError>;
}
