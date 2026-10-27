//! 模块管理命令。

use tauri::{Emitter, State};

use crate::error::{AppError, CommandResult};
use crate::modules::capabilities::Capability;
use crate::modules::host::{ContributionEntry, ModuleRecord};
use crate::modules::manifest::ModuleManifest;
use crate::state::AppState;

#[tauri::command]
pub fn list_modules(state: State<'_, AppState>) -> CommandResult<Vec<ModuleRecord>> {
    state.modules.list()
}

#[tauri::command]
pub fn discover_modules(state: State<'_, AppState>) -> CommandResult<Vec<ModuleManifest>> {
    state.modules.discover()
}

#[tauri::command]
pub async fn install_module(
    state: State<'_, AppState>,
    path: String,
) -> CommandResult<ModuleManifest> {
    // 安装要递归复制整个包（可能几十 MB）：同步命令会卡住主线程，
    // 用户会看到界面定住但没有任何进度。
    let path = path.trim().to_string();
    if path.is_empty() {
        return Err(AppError::new(4001, "请选择模块目录"));
    }
    let modules = state.modules.clone();
    let manifest = tauri::async_runtime::spawn_blocking(move || {
        modules.install(std::path::Path::new(&path))
    })
    .await
    .map_err(|e| AppError::with_details(4002, "安装模块意外终止", e.to_string()))??;
    let _ = state.app.emit(crate::events::MODULE_CHANGED, &manifest.id);
    Ok(manifest)
}

#[tauri::command]
pub fn uninstall_module(state: State<'_, AppState>, id: String) -> CommandResult<()> {
    state.modules.uninstall(&id)?;
    let _ = state.app.emit(crate::events::MODULE_CHANGED, &id);
    Ok(())
}

#[tauri::command]
pub async fn enable_module(
    state: State<'_, AppState>,
    id: String,
    grants: Vec<Capability>,
) -> CommandResult<()> {
    state.modules.enable(&id, grants).await?;
    let _ = state.app.emit(crate::events::MODULE_LOADED, &id);
    let _ = state.app.emit(crate::events::MODULE_CHANGED, &id);
    Ok(())
}

#[tauri::command]
pub fn disable_module(state: State<'_, AppState>, id: String) -> CommandResult<()> {
    state.modules.disable(&id)?;
    let _ = state.app.emit(crate::events::MODULE_CHANGED, &id);
    Ok(())
}

#[tauri::command]
pub async fn invoke_module(
    state: State<'_, AppState>,
    id: String,
    method: String,
    params: serde_json::Value,
) -> CommandResult<serde_json::Value> {
    state.modules.invoke_method(&id, &method, params).await
}

#[tauri::command]
pub fn list_module_contributions(state: State<'_, AppState>) -> CommandResult<Vec<ContributionEntry>> {
    state.modules.list_contributions()
}

#[tauri::command]
pub fn read_module_ui(state: State<'_, AppState>, id: String) -> CommandResult<String> {
    state.modules.read_module_ui(&id)
}
