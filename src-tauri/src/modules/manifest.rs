//! 模块清单（module.json）的类型定义与解析/校验。

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{AppError, CommandResult};

/// 模块唯一标识（反域名格式，如 `com.clipbase.filee`）。
pub type ModuleId = String;

/// 模块运行时级别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModuleRuntime {
    /// 纯 UI 模块（sandboxed iframe）。
    Iframe,
    /// 独立 sidecar 进程。
    Sidecar,
    /// WASM 沙箱（P1 预留）。
    Wasm,
}

/// 模块入口。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModuleEntry {
    /// sidecar 可执行文件相对路径。
    #[serde(default)]
    pub backend: Option<String>,
    /// UI 入口相对路径。
    #[serde(default)]
    pub ui: Option<String>,
}

/// 生命周期策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lifecycle {
    /// 按需启动。
    Demand,
    /// 常驻后台。
    Background,
}

/// 贡献点。
///
/// 标签必须是 `snake_case`：前端按 `"clipboard_action"` 过滤（见 `useClipActions`），
/// 而 `lowercase` 会把它序列化成 `"clipboardaction"`，导致右键菜单永远看不到模块动作。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Contribution {
    /// 菜单栏贡献。
    ///
    /// 只保留解析：ClipBase 已取消菜单栏图标（纯后台 + 快捷键唤出面板），
    /// 这类贡献现在不会渲染到任何地方。保留变体是为了不把旧模块清单一律
    /// 判成非法——模块仍可安装、其它贡献点照常生效。
    Tray {
        label: String,
        #[serde(default)]
        icon: Option<String>,
    },
    ClipboardAction {
        label: String,
        #[serde(default)]
        mime: Vec<String>,
    },
    Shortcut {
        label: String,
        #[serde(default)]
        default: String,
    },
    Settings {
        label: String,
    },
    Command {
        id: String,
        label: String,
    },
}

/// 模块清单。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleManifest {
    pub schema: u32,
    pub id: ModuleId,
    pub name: String,
    pub version: semver::Version,
    pub min_core: semver::Version,
    pub runtime: ModuleRuntime,
    pub entry: ModuleEntry,
    pub permissions: Vec<crate::modules::capabilities::Capability>,
    pub contributions: Vec<Contribution>,
    pub lifecycle: Lifecycle,
}

/// 当前支持的最新 manifest schema 版本。
pub const MANIFEST_SCHEMA: u32 = 1;

/// 模块 ID 是否可以直接当作 `modules/<id>/` 的目录名。
///
/// 反域名格式（`com.clipbase.filee`），只允许 ASCII 字母数字与 `-` `_` `.`，
/// 不得以 `.` 开头、不得包含 `..`。这条校验是**安全边界**而不是整洁癖：
/// `install` 会先 `remove_dir_all(modules_dir.join(id))` 再复制，清单里写
/// `id: "../../Documents"` 就会删掉模块目录之外的内容；同理 `uninstall`
/// 与 `read_module_ui` 都拿它拼路径。
pub fn valid_module_id(id: &str) -> bool {
    if id.is_empty() || id.len() > 128 || !id.contains('.') {
        return false;
    }
    if id.starts_with('.') || id.contains("..") {
        return false;
    }
    id.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// 入口路径必须是模块目录内的相对路径。
///
/// `entry.backend: "/bin/sh"` 或 `entry.ui: "../../secret"` 都会跳出模块目录：
/// 前者让 sidecar 启动时执行系统任意可执行文件，后者能把任意文件读进 iframe。
pub fn safe_relative_path(raw: &str) -> bool {
    if raw.is_empty() || raw.starts_with('/') || raw.contains('\\') {
        return false;
    }
    // Windows 盘符与 `~` 展开都不属于「模块目录内的相对路径」。
    if raw.starts_with('~') || raw.as_bytes().get(1) == Some(&b':') {
        return false;
    }
    !Path::new(raw).components().any(|c| {
        // `./index.html` 这种写法是合法的，只拦真正的「往上跳」与绝对路径。
        matches!(
            c,
            std::path::Component::ParentDir
                | std::path::Component::RootDir
                | std::path::Component::Prefix(_)
        )
    })
}

// ---------------------------------------------------------------------------
// 原始 JSON 反序列化结构（字符串字段，解析后校验）
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct RawManifest {
    schema: u32,
    id: String,
    name: String,
    version: String,
    min_core: String,
    runtime: String,
    #[serde(default)]
    entry: RawEntry,
    #[serde(default)]
    permissions: Vec<String>,
    #[serde(default)]
    contributions: Vec<RawContribution>,
    #[serde(default)]
    lifecycle: RawLifecycle,
}

#[derive(Debug, Deserialize)]
struct RawLifecycle {
    #[serde(default = "default_run_on")]
    run_on: String,
}

impl Default for RawLifecycle {
    fn default() -> Self {
        Self {
            run_on: default_run_on(),
        }
    }
}

fn default_run_on() -> String {
    "demand".to_string()
}

#[derive(Debug, Default, Deserialize)]
struct RawEntry {
    #[serde(default)]
    backend: Option<String>,
    #[serde(default)]
    ui: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawContribution {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    label: String,
    #[serde(default)]
    icon: Option<String>,
    #[serde(default)]
    mime: Vec<String>,
    #[serde(default)]
    default: Option<String>,
    #[serde(default)]
    id: Option<String>,
}

/// 从 JSON 文本解析并校验 module.json。
pub fn parse_manifest(json: &str) -> CommandResult<ModuleManifest> {
    let raw: RawManifest = serde_json::from_str(json)
        .map_err(|e| AppError::with_details(4001, "模块清单（module.json）格式不正确", e.to_string()))?;
    validate_raw(&raw)
}

/// 从文件路径读取并解析 module.json。
pub fn load_manifest(path: &Path) -> CommandResult<ModuleManifest> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| AppError::with_details(4001, "读取模块清单（module.json）失败", e.to_string()))?;
    parse_manifest(&content)
}

fn validate_raw(raw: &RawManifest) -> CommandResult<ModuleManifest> {
    if raw.schema != MANIFEST_SCHEMA {
        return Err(AppError::with_details(
            4001,
            "不支持的模块清单版本",
            format!("需要 schema {MANIFEST_SCHEMA}，实际是 {}", raw.schema),
        ));
    }
    // id 会被拼成目录名：除了字符集，还要是反域名形式（两处判断合成一条中文提示，
    // 否则用户会先后收到两种相似但措辞不同的报错）。
    if !valid_module_id(&raw.id) {
        return Err(AppError::with_details(
            4001,
            "模块 ID 不合法",
            format!(
                "需要反域名形式（如 com.clipbase.foo），只能用字母、数字与 - _ .，且不能以 . 开头：'{}'",
                raw.id
            ),
        ));
    }
    for (field, value) in [
        ("entry.backend", raw.entry.backend.as_deref()),
        ("entry.ui", raw.entry.ui.as_deref()),
    ] {
        if let Some(value) = value.filter(|v| !v.trim().is_empty()) {
            if !safe_relative_path(value) {
                return Err(AppError::with_details(
                    4001,
                    "模块入口路径不合法",
                    format!("{field} 必须是模块目录内的相对路径：'{value}'"),
                ));
            }
        }
    }
    if raw.name.trim().is_empty() {
        return Err(AppError::new(4001, "模块名不能为空"));
    }

    let version = semver::Version::parse(&raw.version)
        .map_err(|e| AppError::with_details(4001, "模块版本号不合法", e.to_string()))?;
    let min_core = semver::Version::parse(&raw.min_core)
        .map_err(|e| AppError::with_details(4001, "min_core 版本号不合法", e.to_string()))?;

    let runtime = match raw.runtime.as_str() {
        "iframe" => ModuleRuntime::Iframe,
        "sidecar" => ModuleRuntime::Sidecar,
        "wasm" => ModuleRuntime::Wasm,
        other => {
            return Err(AppError::with_details(
                4001,
                "不支持的模块运行时",
                format!("未知 runtime '{other}'（可选 iframe / sidecar / wasm）"),
            ))
        }
    };

    // sidecar 必须提供后端入口。
    if runtime == ModuleRuntime::Sidecar && raw.entry.backend.as_deref().unwrap_or("").is_empty() {
        return Err(AppError::new(
            4001,
            "sidecar 模块必须声明 entry.backend",
        ));
    }

    let mut permissions = Vec::new();
    for p in &raw.permissions {
        let cap = crate::modules::capabilities::Capability::parse(p)
            .ok_or_else(|| AppError::with_details(4001, "未知权限", format!("'{p}'")))?;
        permissions.push(cap);
    }

    let mut contributions = Vec::new();
    for c in &raw.contributions {
        contributions.push(parse_contribution(c)?);
    }

    Ok(ModuleManifest {
        schema: raw.schema,
        id: raw.id.clone(),
        name: raw.name.clone(),
        version,
        min_core,
        runtime,
        entry: ModuleEntry {
            backend: raw.entry.backend.clone(),
            ui: raw.entry.ui.clone(),
        },
        permissions,
        contributions,
        lifecycle: match raw.lifecycle.run_on.as_str() {
            "background" => Lifecycle::Background,
            _ => Lifecycle::Demand,
        },
    })
}

fn parse_contribution(c: &RawContribution) -> CommandResult<Contribution> {
    match c.kind.as_str() {
        "tray" => Ok(Contribution::Tray {
            label: c.label.clone(),
            icon: c.icon.clone(),
        }),
        "clipboard_action" => Ok(Contribution::ClipboardAction {
            label: c.label.clone(),
            mime: c.mime.clone(),
        }),
        "shortcut" => Ok(Contribution::Shortcut {
            label: c.label.clone(),
            default: c.default.clone().unwrap_or_default(),
        }),
        "settings" => Ok(Contribution::Settings {
            label: c.label.clone(),
        }),
        "command" => Ok(Contribution::Command {
            id: c.id.clone().unwrap_or_default(),
            label: c.label.clone(),
        }),
        other => Err(AppError::with_details(
            4001,
            "未知的贡献点类型",
            format!("'{other}'（可选 tray / clipboard_action / shortcut / settings / command）"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 序列化出去的 `type` 必须与前端枚举、module.json 一致（都是 snake_case）。
    #[test]
    fn contribution_tags_round_trip_as_snake_case() {
        let cases = [
            (Contribution::Tray { label: "a".into(), icon: None }, "tray"),
            (
                Contribution::ClipboardAction {
                    label: "b".into(),
                    mime: vec!["image/*".into()],
                },
                "clipboard_action",
            ),
            (
                Contribution::Shortcut {
                    label: "c".into(),
                    default: "Control+Shift+X".into(),
                },
                "shortcut",
            ),
            (Contribution::Settings { label: "d".into() }, "settings"),
            (
                Contribution::Command {
                    id: "e".into(),
                    label: "e".into(),
                },
                "command",
            ),
        ];

        for (value, tag) in cases {
            let json = serde_json::to_value(&value).expect("serialize");
            assert_eq!(json["type"], tag, "{json}");
            // 反序列化也要能读回同一个值（前端/托盘会原样回传）。
            let back: Contribution = serde_json::from_value(json).expect("deserialize");
            assert_eq!(back, value);
        }
    }
}
