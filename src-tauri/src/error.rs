//! 统一错误类型：库内部用 thiserror 定义具体错误，命令边界统一转为 `AppError` 返回前端。

use serde::Serialize;

/// 面向前端的统一错误对象。
///
/// `code` 采用分类段：
/// - `1xxx` 剪贴板
/// - `2xxx` 历史
/// - `3xxx` 同步
/// - `4xxx` 模块
/// - `5xxx` 配置
/// - `9xxx` 通用
#[derive(Debug, Clone, Serialize)]
pub struct AppError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

impl AppError {
    pub fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: None,
        }
    }

    pub fn with_details(code: i32, message: impl Into<String>, details: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: Some(details.into()),
        }
    }

    /// 内部错误：`message` 是给用户看的中文短句，`details` 保留技术细节（日志/排查用）。
    ///
    /// 以前这里写的是 `"internal error"`：凡是从 `rusqlite` / `io` / `serde_json`
    /// 自动转换过来的错误都会把这句话原样显示给用户（前端会把 message 与
    /// details 用「：」拼起来），中文界面里出现英文是明显的破绽。
    pub fn internal(err: impl std::fmt::Display) -> Self {
        Self::with_details(9000, "内部错误", err.to_string())
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl std::error::Error for AppError {}

impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        Self::internal(err)
    }
}

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        Self::internal(err)
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(err: rusqlite::Error) -> Self {
        Self::internal(err)
    }
}

impl From<serde_json::Error> for AppError {
    fn from(err: serde_json::Error) -> Self {
        Self::internal(err)
    }
}

/// 所有 Tauri 命令统一返回该类型，便于前端统一处理。
pub type CommandResult<T> = Result<T, AppError>;
