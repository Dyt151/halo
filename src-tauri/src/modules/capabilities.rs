//! 能力（权限白名单）枚举 + `CapabilityGate`（权限门控）。

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use crate::lock_ext::LockExt;

/// 模块可声明/被授权的系统能力。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    ClipboardRead,
    ClipboardWrite,
    /// 仅模块自身目录。
    FsReadModule,
    /// 用户目录读（需运行时授权）。
    FsReadUser,
    FsWriteUser,
    Network,
}

impl Capability {
    /// 从 module.json 的权限字符串解析。
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "clipboard:read" => Some(Self::ClipboardRead),
            "clipboard:write" => Some(Self::ClipboardWrite),
            "fs:read:module" => Some(Self::FsReadModule),
            "fs:read:user" => Some(Self::FsReadUser),
            "fs:write:user" => Some(Self::FsWriteUser),
            "network:webdav" | "network" => Some(Self::Network),
            _ => None,
        }
    }

    /// 权限标识（module.json 中使用的字符串）。
    pub fn id(&self) -> &'static str {
        match self {
            Self::ClipboardRead => "clipboard:read",
            Self::ClipboardWrite => "clipboard:write",
            Self::FsReadModule => "fs:read:module",
            Self::FsReadUser => "fs:read:user",
            Self::FsWriteUser => "fs:write:user",
            Self::Network => "network:webdav",
        }
    }

    /// 面向用户的可读标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::ClipboardRead => "读取剪贴板",
            Self::ClipboardWrite => "写入剪贴板",
            Self::FsReadModule => "读取模块自身目录",
            Self::FsReadUser => "读取用户目录",
            Self::FsWriteUser => "写入用户目录",
            Self::Network => "网络访问",
        }
    }

    /// 方法名 → 所需能力（模块→核心 RPC 门控用）。
    pub fn required_for_method(method: &str) -> Option<Self> {
        match method {
            "clipboard.read" => Some(Self::ClipboardRead),
            "clipboard.write" => Some(Self::ClipboardWrite),
            "fs.read" => Some(Self::FsReadModule),
            "fs.read_user" => Some(Self::FsReadUser),
            "fs.write_user" => Some(Self::FsWriteUser),
            "network.request" => Some(Self::Network),
            _ => None,
        }
    }
}

/// 权限门控：模块与核心之间的唯一关口。
pub struct CapabilityGate {
    grants: Mutex<HashMap<String, HashSet<Capability>>>,
}

impl CapabilityGate {
    pub fn new() -> Self {
        Self {
            grants: Mutex::new(HashMap::new()),
        }
    }

    /// 记录某模块的授权能力快照。
    pub fn grant(&self, module_id: &str, caps: &[Capability]) {
        let mut grants = self.grants.lock_ok();
        grants.insert(module_id.to_string(), caps.iter().copied().collect());
    }

    /// 撤销某模块全部授权。
    pub fn revoke(&self, module_id: &str) {
        self.grants.lock_ok().remove(module_id);
    }

    /// 校验某模块是否被授权某能力。
    pub fn check(&self, module_id: &str, cap: Capability) -> bool {
        self.grants
            .lock_ok()
            .get(module_id)
            .map(|set| set.contains(&cap))
            .unwrap_or(false)
    }

    /// 获取某模块当前授权能力。
    pub fn grants_for(&self, module_id: &str) -> Vec<Capability> {
        self.grants
            .lock_ok()
            .get(module_id)
            .map(|set| set.iter().copied().collect())
            .unwrap_or_default()
    }
}

impl Default for CapabilityGate {
    fn default() -> Self {
        Self::new()
    }
}
