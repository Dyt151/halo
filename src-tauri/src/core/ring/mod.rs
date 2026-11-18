//! Finder 环形转换会话状态机（总规范 README「三、环形文件转换」）。
//!
//! 为什么单独一层：手势、执行与结果是同一件事的三个阶段，必须汇入**同一个**
//! 会话与执行路径，否则命名、输出目录、错误处理会各自漂移。
//!
//! 状态机（Filee 行为）：
//! `Idle → Capturing → Probing → Ready（唯一一面）/ Empty / PermissionDenied /
//! Timeout / Failed → Converting → Result`
//!
//! 关键约束：
//! - 唯一入口是 Finder 里 Option＋左键拖动（`option_tap`）；不再有快捷键、
//!   Finder 右键服务、单击、Enter、方向键。
//! - 只有一面盘（背面、Tab / 滚轮翻面已取消）；拖拽期间 Esc 取消。
//! - 打包 / 解压 / PDF 分页等工具动作不进 Profile，运行时按固定顺序追加在
//!   正面末尾（`RingConfig::utility_actions` 开关控制）。
//! - 载荷以**起手文件**为权威（`finder::file_at_point`），不再做松手复核，
//!   也不要求用户再拖一次。
//! - 选盘分两步：先用本批扩展名选一个 Profile，再算每个扇区是否可用（
//!   Filee 行为）。不可用的预设**不下发**：盘面只显示当前批次真正能执行的动作。
//! - 文件夹也是合法载荷：没有可探格式，落兜底盘后只剩打包等工具动作。
//! - 执行只用当次快照的路径与预设副本，配置后续变更不污染运行中的任务。

pub mod profile;

use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::config::settings::{Config, RingProfile, RingProfileKind};
use crate::core::convert::action;
use crate::core::convert::job::{self, CancelToken, ConversionFileResult};
use crate::core::convert::preset::{self, find_or_legacy, ConversionPreset};
use crate::core::convert::{self, ConvertFormat};
use crate::core::finder;
use crate::lock_ext::LockExt;

/// 环形会话状态更新事件（Rust → ring 窗口）。
pub const RING_UPDATE: &str = "ring://update";
/// 转换进度事件（逐文件，原地进度环，对齐 Filee）。
pub const RING_PROGRESS: &str = "ring://progress";
/// 转换结果事件（原地成功/失败反馈）。
pub const RING_RESULT: &str = "ring://result";
/// 转换超过 5s 时把环形盘收缩交给系统通知的事件（Filee 行为约定）。
pub const RING_HANDOFF: &str = "ring://handoff";
/// Option+拖动期间：指针位置（屏幕逻辑坐标），供拖放模式做命中与高亮。
pub const RING_POINTER: &str = "ring://pointer";
/// Option+拖动期间：松手，前端按当前 hover 提交或取消（Filee 拖放模式）。
pub const RING_DRAG_END: &str = "ring://drag-end";

/// 超过这个时长仍未转完，就把环形盘收缩交给通知，不再挡在屏幕中央。
const HANDOFF_AFTER: std::time::Duration = std::time::Duration::from_secs(5);

/// 启动时登记 AppHandle，供无 `app` 参数的调用点（ObjC 服务回调）使用。
static APP: OnceLock<AppHandle> = OnceLock::new();
/// 当前会话（同一时刻只有一个环形转换在跑）。
static SESSION: OnceLock<Arc<Mutex<Session>>> = OnceLock::new();
/// 拖放模式的起手点（屏幕逻辑坐标）；`None` = 盘还没定位。
static DRAG_ORIGIN: Mutex<Option<(f64, f64)>> = Mutex::new(None);
/// 起手文件（按下左键时指针下的 Finder 文件）。
///
/// 它是本次拖动载荷的**权威**部分：用户可能拖一个原本未选中的文件，而接管后
/// 拖拽事件被吞掉，Finder 只看到单击，松手时选中项已经被收缩掉。
static GRABBED: Mutex<Option<String>> = Mutex::new(None);
/// Option 按下时预取的 Finder 选中项（时间戳毫秒, 路径）。
///
/// 为什么不现读：Finder 在 mouseDown 后会进事件追踪循环，主线程不再响应 AX，
/// 读选中项会**一直阻塞到松手**（实测 4.9s），拖动全程拿不到动作。
/// 它只用来把「多选中的一项」扩展成整个选择，判决权始终在 [`GRABBED`]。
static PREFETCH: Mutex<Option<(u64, Vec<String>)>> = Mutex::new(None);
/// 预取的有效期：太久之前的选中项很可能已经不是用户要拖的文件。
const PREFETCH_TTL_MS: u64 = 5_000;
/// 最近一次拖动指针（已换算成环形窗内的 CSS 坐标）。
///
/// 环铺开前发出的 `RING_POINTER` 会被前端丢弃（那时还没 ready），所以松手时
/// 随 `RING_DRAG_END` 再带一次，避免「拖到扇区后停住、不再移动、直接松手」
/// 因为没收到过位置而被判成取消。
static LAST_POINTER: Mutex<Option<(f64, f64)>> = Mutex::new(None);


mod flow;
mod gesture;
mod plan;
mod run;
mod session;

#[cfg(test)]
mod tests;

/// 保存状态机的原始数据；对外快照 [`RingSnapshot`] 由它派生。
#[derive(Default)]
struct Session {
    /// 单调递增的会话标识：新会话递增它，使旧环形盘自动失效（任务 16）。
    generation: u64,
    state: State,
    /// 本次捕获的选中项路径快照（绝对路径）。
    paths: Vec<String>,
    /// 选中的 Profile id（诊断用，Filee 行为）：出问题时能看出用了哪张盘。
    profile: String,
}

/// 状态机的内部状态。
///
/// 只剩「单手势」能走到的阶段：失败/空不再分门别类枚举，而是同一张提示盘里
/// 带不同原因（`build_snapshot` 生成 hint），前端只需认 `empty` 一种。
#[derive(Debug, Default, Clone, PartialEq, Eq)]
enum State {
    #[default]
    Idle,
    Capturing,
    Probing,
    Ready,
    Empty,
    Converting,
}

/// 单个环形扇区（RingSlice）。
#[derive(Debug, Clone, Serialize)]
pub struct RingAction {
    /// 动作 id = **预设 id**（`img.jpg` / `doc.md`），不再是格式名（Filee 行为）。
    pub id: String,
    /// 展示名（含 JPEG 质量，如「JPG 85%」）。
    pub label: String,
    /// 目标格式（`None` = 与源格式相同，只有 `reencode_same_format` 预设会这样）。
    pub target: Option<String>,
    /// 该动作将实际转换的文件数（有路由、且不需要跳过）。
    pub convert: usize,
    /// 不会转换的文件数（已是目标格式 / 无路由 / 格式不认识）。
    pub skip: usize,
    /// 本批文件下能否执行。`false` 仍占原位置，只是降低不透明度并显示原因。
    pub enabled: bool,
    /// 禁用原因（`enabled == true` 时为 `None`）。
    pub disabled_reason: Option<String>,
}

/// 转换进度（逐文件）。
#[derive(Debug, Clone, Serialize)]
pub struct RingProgressEvent {
    pub generation: u64,
    pub done: usize,
    pub total: usize,
}

/// 拖放模式（Option+拖动）下的指针位置：环形窗内的 CSS 坐标（左上原点）。
#[derive(Debug, Clone, Copy, Serialize)]
pub struct PointerEvent {
    pub x: f64,
    pub y: f64,
}

/// 转换结果（原地成功/失败反馈，对齐 Filee）。
#[derive(Debug, Clone, Serialize)]
pub struct RingResultEvent {
    pub generation: u64,
    pub converted: usize,
    pub skipped: usize,
    pub failed: usize,
    /// 被用户取消、未完成的文件数（Filee 行为：取消同样要给结果，不能无声）。
    pub cancelled: usize,
    pub errors: Vec<String>,
    /// 转成功了但**不保真**的地方（动画只剩首帧、ICC 未保留……）。Filee 行为 要求
    /// 这些不能静默丢掉，所以随结果一起回给环形盘展示。
    pub warnings: Vec<String>,
    /// 单个输出文件的绝对路径（成功时用于「在 Finder 中显示」）。
    pub output: Option<String>,
}

/// 本次会话选中的文件类型 Profile（Filee 行为）。
#[derive(Debug, Clone, Serialize)]
pub struct ProfileInfo {
    pub id: String,
    pub name: String,
    /// 归属本 Profile 的扩展名（空 = catch-all）。
    pub extensions: Vec<String>,
    /// 是否是混合兜底（前端用它在中心标注「其他文件」）。
    pub fallback: bool,
}

/// 前端用的会话快照（ring 窗口据此渲染）。
#[derive(Debug, Clone, Serialize)]
pub struct RingSnapshot {
    pub generation: u64,
    pub state: String,
    /// 类别标签：图片 / 文本 / 混合文件。
    pub category: Option<String>,
    /// 捕获到的可识别文件总数。
    pub count: usize,
    pub hint: Option<String>,
    /// 本批文件选中的 Profile；没有 Profile 时为 `None`。
    pub profile: Option<ProfileInfo>,
    /// 这张盘的扇区（只含对当前批次可用的动作，禁用项已过滤）。
    pub actions: Vec<RingAction>,
    /// 按类别记住的最后动作：只用于初始高亮，不自动执行。
    pub suggested: Option<String>,
}

/// 环形会话管理器（无状态句柄，全部走进程级静态）。
pub struct RingManager;
