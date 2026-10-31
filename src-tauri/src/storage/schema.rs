//! 表结构常量（与 migrations.rs 保持一致，供代码内查询复用）。

/// 历史主表。
pub const TABLE_CLIPS: &str = "clips";
/// FTS5 全文索引（external content，内容取自 `clips`）。
pub const TABLE_CLIPS_FTS: &str = "clips_fts";
/// 键值配置表。
pub const TABLE_CONFIG: &str = "config";
/// 模块注册表。
pub const TABLE_MODULES: &str = "modules";
/// 同步游标/状态表。
pub const TABLE_SYNC_STATE: &str = "sync_state";
