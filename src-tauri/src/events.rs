//! 事件名常量（URI 风格 `domain://event`）+ 发布 helper。
//!
//! 与前端 `src/lib/events.ts` 中的常量一一对应，避免字符串漂移。

/// 剪贴板内容发生变化（监控捕获到新内容）。
pub const CLIPBOARD_CHANGED: &str = "clipboard://changed";
/// 历史列表新增条目。
pub const HISTORY_CREATED: &str = "history://created";
/// 历史列表被删除/清空。
pub const HISTORY_DELETED: &str = "history://deleted";
/// 同步状态发生变化。
pub const SYNC_STATUS_CHANGED: &str = "sync://status-changed";
/// 同步完成（成功或失败）。
pub const SYNC_COMPLETED: &str = "sync://completed";
/// 模块已加载/启用。
pub const MODULE_LOADED: &str = "module://loaded";
/// 模块进程崩溃。
pub const MODULE_CRASHED: &str = "module://crashed";
/// 模块列表发生变化（安装/卸载/启用/禁用）。
pub const MODULE_CHANGED: &str = "module://changed";
/// 配置发生变化。
pub const CONFIG_CHANGED: &str = "config://changed";
/// 一次粘贴已成功完成（前端据此决定是否播完动画收起面板）。
pub const PASTE_FINISHED: &str = "clipboard://paste-finished";
/// 模块向核心发布的总线事件（广播给前端）。
pub const MODULE_EVENT: &str = "module://event";
/// 模块向核心发送请求并期待核心响应（RPC）。
pub const MODULE_RPC: &str = "module://rpc";
