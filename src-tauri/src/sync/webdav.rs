//! WebDAV 同步 provider 实现（reqwest + rustls）。

use std::time::Duration;

use async_trait::async_trait;
use reqwest::StatusCode;

use super::provider::{SyncError, SyncProvider};
use crate::config::settings::{SyncCredentials, SyncProviderKind};

pub struct WebDavProvider {
    client: reqwest::Client,
}

/// 把 HTTP 状态码翻成「该去改什么」的中文提示。
///
/// 直接抛 `HTTP 401 Unauthorized` 对用户等于什么也没说：配错密码和目录只读
/// 都是 401/403，用户只能反复重试。按最常见的几种情形给出下一步动作。
fn http_hint(action: &str, status: StatusCode) -> String {
    match status.as_u16() {
        401 | 403 => format!(
            "{action}失败（HTTP {}）：账号或密码不对，或该目录没有写入权限",
            status.as_u16()
        ),
        404 => format!("{action}失败（HTTP 404）：地址或文件名不对，且目标目录必须已存在"),
        405 => format!("{action}失败（HTTP 405）：服务端不支持该请求方法，可能不是 WebDAV 地址"),
        409 => format!("{action}失败（HTTP 409）：目标目录不存在，请先在网盘里建好"),
        507 => format!("{action}失败（HTTP 507）：网盘空间不足"),
        code => format!(
            "{action}失败（HTTP {code}）：{}",
            status.canonical_reason().unwrap_or("未知错误")
        ),
    }
}

/// 远端文件名 / 子目录路径是否合法。
///
/// 这一步是**安全边界**：`target_url` 会把 `remote_file` 直接拼到用户填的
/// WebDAV 地址后面，`../` 能让快照写到同步目录之外（甚至可以写到另一个用户的
/// 共享目录），`http://` 开头则会变成另一个主机名。只允许模块目录内的相对路径。
pub fn valid_remote_file(raw: &str) -> bool {
    let s = raw.trim();
    if s.is_empty() || s.len() > 200 {
        return false;
    }
    if s.contains("://") || s.contains('\\') {
        return false;
    }
    if s.chars()
        .any(|c| c.is_control() || matches!(c, '?' | '#'))
    {
        return false;
    }
    // 逐段检查：不允许空段与「往上跳」。
    s.split('/').all(|seg| !seg.is_empty() && seg != "." && seg != "..")
}

/// WebDAV provider（reqwest + rustls）。
impl WebDavProvider {
    pub fn new() -> Result<Self, SyncError> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| SyncError::Network(e.to_string()))?;
        Ok(Self { client })
    }

    /// 构造目标 URL：基址 + 相对路径；不合法的远端路径直接拒绝。
    fn target_url(creds: &SyncCredentials, remote_file: &str) -> Result<String, SyncError> {
        if !valid_remote_file(remote_file) {
            return Err(SyncError::Provider(
                "远端文件名不合法（不能包含 ..、完整网址或控制字符）".to_string(),
            ));
        }
        let base = creds.url.trim().trim_end_matches('/');
        let file = remote_file.trim().trim_start_matches('/');
        Ok(format!("{base}/{file}"))
    }
}

#[async_trait]
impl SyncProvider for WebDavProvider {
    fn kind(&self) -> SyncProviderKind {
        SyncProviderKind::WebDav
    }

    async fn test(&self, creds: &SyncCredentials) -> Result<(), SyncError> {
        let url = creds.url.trim().trim_end_matches('/').to_string();
        let resp = self
            .client
            // PROPFIND 是合法 HTTP token，`from_bytes` 对静态常量不可能失败。
            .request(reqwest::Method::from_bytes(b"PROPFIND").unwrap(), &url)
            .header("Depth", "0")
            .basic_auth(&creds.username, Some(&creds.password))
            .send()
            .await
            .map_err(|e| SyncError::Network(e.to_string()))?;
        if resp.status().is_success() {
            Ok(())
        } else {
            Err(SyncError::Provider(http_hint("连接测试", resp.status())))
        }
    }

    async fn push(
        &self,
        creds: &SyncCredentials,
        remote_file: &str,
        payload: &[u8],
    ) -> Result<(), SyncError> {
        let url = Self::target_url(creds, remote_file)?;
        let resp = self
            .client
            .put(&url)
            .basic_auth(&creds.username, Some(&creds.password))
            .body(payload.to_vec())
            .send()
            .await
            .map_err(|e| SyncError::Network(e.to_string()))?;
        if resp.status().is_success() {
            Ok(())
        } else {
            Err(SyncError::Provider(http_hint("上传", resp.status())))
        }
    }

    async fn pull(
        &self,
        creds: &SyncCredentials,
        remote_file: &str,
    ) -> Result<Vec<u8>, SyncError> {
        let url = Self::target_url(creds, remote_file)?;
        let resp = self
            .client
            .get(&url)
            .basic_auth(&creds.username, Some(&creds.password))
            .send()
            .await
            .map_err(|e| SyncError::Network(e.to_string()))?;

        match resp.status() {
            StatusCode::NOT_FOUND => Err(SyncError::NotFound),
            s if s.is_success() => resp
                .bytes()
                .await
                .map(|b| b.to_vec())
                .map_err(|e| SyncError::Network(e.to_string())),
            s => Err(SyncError::Provider(http_hint("下载", s))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::valid_remote_file;

    #[test]
    fn remote_file_rejects_traversal_and_urls() {
        // 正常文件名与子目录都可以。
        assert!(valid_remote_file("clipbase-sync.json"));
        assert!(valid_remote_file("clipbase/sync.json"));
        // 往上跳 / 绝对网址 / 空段都会把快照写到同步目录之外。
        assert!(!valid_remote_file("../evil.json"));
        assert!(!valid_remote_file("a/../../b.json"));
        assert!(!valid_remote_file("http://evil.com/x.json"));
        assert!(!valid_remote_file("a//b.json"));
        assert!(!valid_remote_file("./a.json"));
        assert!(!valid_remote_file("  "));
        assert!(!valid_remote_file("a\\b.json"));
        assert!(!valid_remote_file("a.json?x=1"));
    }
}
