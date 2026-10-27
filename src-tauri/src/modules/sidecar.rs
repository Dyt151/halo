//! sidecar 进程管理 + JSON-RPC 2.0 over stdio。

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex as StdMutex;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;
use crate::lock_ext::LockExt;

/// 一个 sidecar 模块子进程句柄。
pub struct SidecarHandle {
    child: StdMutex<Child>,
    stdin: Mutex<ChildStdin>,
    stdout: Mutex<BufReader<ChildStdout>>,
    next_id: AtomicU64,
}

impl SidecarHandle {
    /// 启动 sidecar 进程（stdin/stdout 走 JSON-RPC）。
    pub async fn spawn(program: &Path, args: &[String]) -> Result<Self, String> {
        let mut child = Command::new(program)
            .args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| format!("failed to spawn sidecar: {e}"))?;
        let stdin = child.stdin.take().ok_or("sidecar stdin unavailable")?;
        let stdout = child.stdout.take().ok_or("sidecar stdout unavailable")?;
        Ok(Self {
            child: StdMutex::new(child),
            stdin: Mutex::new(stdin),
            stdout: Mutex::new(BufReader::new(stdout)),
            next_id: AtomicU64::new(1),
        })
    }

    /// 发送一条 JSON-RPC 请求并等待响应。
    pub async fn request(&self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let req = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        let line = format!("{}\n", serde_json::to_string(&req).map_err(|e| e.to_string())?);

        {
            let mut stdin = self.stdin.lock().await;
            stdin.write_all(line.as_bytes()).await.map_err(|e| e.to_string())?;
            stdin.flush().await.map_err(|e| e.to_string())?;
        }

        let resp_line = tokio::time::timeout(
            Duration::from_secs(10),
            async {
                let mut stdout = self.stdout.lock().await;
                let mut buf = String::new();
                stdout.read_line(&mut buf).await?;
                Ok::<String, std::io::Error>(buf)
            },
        )
        .await
        .map_err(|_| "module response timeout".to_string())?
        .map_err(|e| e.to_string())?;

        let resp: Value =
            serde_json::from_str(resp_line.trim()).map_err(|e| format!("bad module response: {e}"))?;
        if let Some(err) = resp.get("error") {
            return Err(err.to_string());
        }
        Ok(resp.get("result").cloned().unwrap_or(Value::Null))
    }

    /// 进程是否仍在运行。
    pub fn is_alive(&self) -> bool {
        matches!(self.child.lock_ok().try_wait(), Ok(None))
    }

    /// 终止进程。
    pub fn kill(&self) {
        if let Ok(mut child) = self.child.lock() {
            let _ = child.start_kill();
        }
    }
}
