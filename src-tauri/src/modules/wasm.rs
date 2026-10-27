//! WASM 模块运行时（P1 预留）。
//!
//! 首版不启用 `wasmtime`，此处仅保留接口与明确的「未实现」错误，便于后续接入。

use serde_json::Value;

use crate::error::{AppError, CommandResult};

/// WASM 实例占位句柄。
pub struct WasmHandle;

impl WasmHandle {
    pub async fn request(&self, _method: &str, _params: Value) -> CommandResult<Value> {
        Err(AppError::new(
            4004,
            "当前版本还运行不了 WASM 模块（后续版本支持）",
        ))
    }
}
