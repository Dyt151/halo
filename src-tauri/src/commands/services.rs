//! Finder 扩展状态与开关命令。
//!
//! 菜单由 Finder Sync 扩展提供，主程序只负责报告状态、注册 / 启用扩展，
//! 以及轮询扩展投递过来的动作请求。

use crate::core::services::{self, FinderIntegrationStatus};
use crate::error::{AppError, CommandResult};

/// 查询 Finder 扩展是否已安装 / 启用。
#[tauri::command]
pub fn finder_integration_status() -> FinderIntegrationStatus {
    services::status()
}

/// 启用或停用 Finder 扩展。
///
/// 内部要跑 `pluginkit`（注册 / 启用系统扩展，典型耗时几百毫秒）：
/// 放在主线程上就是一次可感知的卡顿 + 开关回弹延迟，因此换到阻塞线程池。
#[tauri::command]
pub async fn set_finder_extension_enabled(
    enabled: bool,
) -> CommandResult<FinderIntegrationStatus> {
    tauri::async_runtime::spawn_blocking(move || {
        services::set_extension_enabled(enabled).map_err(|message| AppError::new(9105, message))
    })
    .await
    .map_err(|e| AppError::with_details(9105, "切换 Finder 扩展失败", e.to_string()))?
}
