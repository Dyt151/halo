//! 同步命令。

use serde::{Deserialize, Serialize};
use tauri::{Emitter, State};

use crate::config::settings::{SyncConfig, SyncCredentials, SyncProviderKind};
use crate::error::{AppError, CommandResult};
use crate::state::AppState;
use crate::sync::SyncStatus;

#[tauri::command]
pub async fn sync_now(state: State<'_, AppState>) -> CommandResult<SyncStatus> {
    let status = state.sync.sync_now().await?;
    let _ = state.app.emit(crate::events::SYNC_COMPLETED, &status);
    let _ = state.app.emit(crate::events::SYNC_STATUS_CHANGED, &status);
    Ok(status)
}

#[tauri::command]
pub fn get_sync_status(state: State<'_, AppState>) -> SyncStatus {
    state.sync.status()
}

/// 测试连通性（评审 S6）：直接用界面上**尚未保存**的凭据试一次 PROPFIND，
/// 成功才让用户点保存，避免「保存完才发现密码错」。
#[tauri::command]
pub async fn test_sync(
    state: State<'_, AppState>,
    credentials: SyncCredentials,
    provider: Option<SyncProviderKind>,
) -> CommandResult<String> {
    let mut creds = credentials;
    // 界面没有重新输入密码时（输入框只显示占位符），用钥匙串里的旧密码。
    if creds.password.is_empty() {
        if let Some(pw) = crate::secrets::webdav_password() {
            creds.password = pw;
        }
    }
    let kind = provider.unwrap_or(SyncProviderKind::WebDav);
    state.sync.test_connection(kind, &creds).await?;
    Ok("连接成功".to_string())
}

/// 同步凭据的落盘状态（前端据此显示「已存入钥匙串」）。
#[derive(Debug, Clone, Serialize)]
pub struct SyncSecretStatus {
    pub has_password: bool,
    /// 加密口令同样在钥匙串里（清单 8）：界面不能再用「输入框是否为空」猜。
    pub has_passphrase: bool,
}

#[tauri::command]
pub fn sync_secret_status() -> SyncSecretStatus {
    SyncSecretStatus {
        has_password: crate::secrets::has_webdav_password(),
        has_passphrase: crate::secrets::has_sync_passphrase(),
    }
}

/// 保存同步设置。
///
/// 与其它设置不同，同步设置不参与前端「整份 draft 自动保存」：
/// 1. 密码/口令要进钥匙串，不能跟着 `config.json` 一起落盘（评审 S6）；
/// 2. 保存后要重启定时器，让 `interval_secs` 立即生效（评审 S2 同类问题）。
#[derive(Debug, Clone, Deserialize)]
pub struct SyncSettingsInput {
    pub enabled: bool,
    pub provider: SyncProviderKind,
    pub url: String,
    pub username: String,
    /// `None` = 沿用已存的密码；`Some("")` = 清除；`Some(v)` = 设为 v。
    pub password: Option<String>,
    pub encryption: bool,
    /// `None` = 沿用已存口令；`Some("")` = 清除；`Some(v)` = 设为 v。
    pub passphrase: Option<String>,
    pub remote_file: String,
    pub interval_secs: u64,
}

/// 自动同步间隔下限：太短会给网盘打限流，也会把钥匙串读成瓶颈。
const MIN_INTERVAL_SECS: u64 = 60;

#[tauri::command]
pub fn set_sync_settings(
    state: State<'_, AppState>,
    input: SyncSettingsInput,
) -> CommandResult<SyncConfig> {
    if let Some(pw) = input.password.as_deref() {
        crate::secrets::set_webdav_password(pw)
            .map_err(|e| AppError::with_details(3100, "保存密码到钥匙串失败", e))?;
    }
    if let Some(pp) = input.passphrase.as_deref() {
        crate::secrets::set_sync_passphrase(pp)
            .map_err(|e| AppError::with_details(3101, "保存口令到钥匙串失败", e))?;
    }

    // 0 表示关闭定时；其余一律抬到下限，避免 1 秒一次的畸形值（评审 S6）。
    let interval_secs = if input.interval_secs == 0 {
        0
    } else {
        input.interval_secs.max(MIN_INTERVAL_SECS)
    };

    // 地址与远端文件名先校验：否则要等到第一次同步才报错，
    // 而那时的错误信息（reqwest 的英文）对用户毫无指引。
    let url = input.url.trim().to_string();
    if input.enabled {
        if url.is_empty() {
            return Err(AppError::new(3005, "开启同步前需要填写 WebDAV 地址"));
        }
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            return Err(AppError::with_details(
                3005,
                "WebDAV 地址格式不正确",
                "需要以 http:// 或 https:// 开头（例如 https://dav.example.com/halo）",
            ));
        }
    }
    let remote_file = if input.remote_file.trim().is_empty() {
        "halo-sync.json".to_string()
    } else {
        input.remote_file.trim().to_string()
    };
    if !crate::sync::webdav::valid_remote_file(&remote_file) {
        return Err(AppError::with_details(
            3005,
            "远端文件名不合法",
            "不能包含 .. 、完整网址、反斜杠或控制字符；子目录用 / 分隔",
        ));
    }

    // 界面已经拦了一道，后端再守一道：加密开关与口令必须在同一个真实状态上。
    // 否则会遇到「已保存、但每次同步都报口令为空」的僵尸配置。
    if input.encryption && !crate::secrets::has_sync_passphrase() {
        return Err(AppError::new(3004, "开启端到端加密前需要设置口令"));
    }

    let next = SyncConfig {
        enabled: input.enabled,
        provider: input.provider,
        credentials: SyncCredentials {
            url,
            username: input.username.trim().to_string(),
            // 明文密码永不写进 config.json。
            password: String::new(),
        },
        encryption: input.encryption,
        passphrase: String::new(),
        remote_file,
        interval_secs,
    };

    let saved = state
        .config
        .patch(serde_json::json!({ "sync": next }))
        .map_err(|e| AppError::with_details(3102, "保存同步设置失败", e.to_string()))?;

    // 定时器跟着配置走：先停旧循环，再按新间隔拉起。
    state.sync.stop();
    state.sync.start(interval_secs);

    let _ = state.app.emit(crate::events::CONFIG_CHANGED, &saved);
    Ok(saved.sync)
}
