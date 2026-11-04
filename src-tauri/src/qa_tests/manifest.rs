//! 模块清单（module.json）解析与校验测试。

use crate::modules::capabilities::Capability;
use crate::modules::manifest::{parse_manifest, Contribution, Lifecycle, ModuleRuntime};

const VALID: &str = r#"{
  "schema": 1,
  "id": "com.halo.filee",
  "name": "文件转换",
  "version": "0.1.0",
  "min_core": "0.1.0",
  "description": "示例模块",
  "author": "halo-community",
  "runtime": "sidecar",
  "entry": { "backend": "bin/filee-cli", "ui": "index.html" },
  "permissions": ["clipboard:read", "clipboard:write"],
  "contributions": [
    { "type": "tray", "label": "文件转换", "icon": "icon.png" },
    { "type": "clipboard_action", "label": "转换此文件", "mime": ["image/*", "application/pdf"] },
    { "type": "shortcut", "label": "打开文件转换", "default": "Cmd+Shift+F" },
    { "type": "settings", "label": "文件转换设置" }
  ],
  "lifecycle": { "run_on": "demand" }
}"#;

#[test]
fn parses_valid_manifest() {
    let m = parse_manifest(VALID).unwrap();
    assert_eq!(m.schema, 1);
    assert_eq!(m.id, "com.halo.filee");
    assert_eq!(m.name, "文件转换");
    assert_eq!(m.version.to_string(), "0.1.0");
    assert_eq!(m.min_core.to_string(), "0.1.0");
    assert_eq!(m.runtime, ModuleRuntime::Sidecar);
    assert_eq!(m.entry.backend.as_deref(), Some("bin/filee-cli"));
    assert_eq!(m.entry.ui.as_deref(), Some("index.html"));
    assert_eq!(m.permissions, vec![Capability::ClipboardRead, Capability::ClipboardWrite]);
    assert_eq!(m.contributions.len(), 4);
    assert_eq!(m.lifecycle, Lifecycle::Demand);
}

#[test]
fn contribution_types_are_parsed() {
    let m = parse_manifest(VALID).unwrap();
    assert!(matches!(m.contributions[0], Contribution::Tray { .. }));
    assert!(matches!(m.contributions[1], Contribution::ClipboardAction { .. }));
    assert!(matches!(m.contributions[2], Contribution::Shortcut { .. }));
    assert!(matches!(m.contributions[3], Contribution::Settings { .. }));
}

#[test]
fn rejects_missing_required_field() {
    // 缺 name 字段。
    let json = r#"{"schema":1,"id":"com.x.y","version":"1.0.0","min_core":"0.1.0","runtime":"iframe"}"#;
    assert!(parse_manifest(json).is_err());
}

#[test]
fn rejects_invalid_json() {
    assert!(parse_manifest("{ not json").is_err());
}

#[test]
fn rejects_unsupported_schema() {
    let json = VALID.replace("\"schema\": 1", "\"schema\": 2");
    assert!(parse_manifest(&json).is_err());
}

#[test]
fn rejects_empty_id() {
    let json = VALID.replace("com.halo.filee", "");
    assert!(parse_manifest(&json).is_err());
}

#[test]
fn rejects_id_without_dot() {
    let json = VALID.replace("com.halo.filee", "filee");
    assert!(parse_manifest(&json).is_err());
}

/// id 会被拼成 `modules/<id>/` 的目录名，安装时还会先 `remove_dir_all`：
/// 路径穿越式的 id 必须被挡在解析阶段。
#[test]
fn rejects_path_traversal_module_id() {
    for evil in [
        "../../Documents",
        "com.halo/../../x",
        ".hidden.module",
        "/abs/path.module",
        "com.halo.filee ",
        "com..halo",
    ] {
        let json = VALID.replace("com.halo.filee", evil);
        assert!(
            parse_manifest(&json).is_err(),
            "id 应被拒绝：{evil}"
        );
    }
}

/// 入口路径必须留在模块目录内：绝对路径能启动任意可执行文件 /
/// 读到模块目录之外的任意文件。
#[test]
fn rejects_entry_path_outside_module_dir() {
    let bad_backends = ["/bin/sh", "../../sh", "bin/../../sh", "~/sh", "bin\\sh"];
    for bad in bad_backends {
        let json = VALID.replace("bin/filee-cli", bad);
        assert!(
            parse_manifest(&json).is_err(),
            "entry.backend 应被拒绝：{bad}"
        );
    }
    let json = VALID.replace("\"index.html\"", "\"../../../etc/passwd\"");
    assert!(parse_manifest(&json).is_err(), "entry.ui 应被拒绝");

    // 子目录形式仍然是合法的（示例模块的 bin/filee-cli 就靠这个）。
    let json = VALID.replace("index.html", "./ui/index.html");
    assert!(parse_manifest(&json).is_ok());
}

#[test]
fn rejects_empty_name() {
    let json = VALID.replace("文件转换", "");
    assert!(parse_manifest(&json).is_err());
}

#[test]
fn rejects_invalid_semver() {
    let json = VALID.replace("\"version\": \"0.1.0\"", "\"version\": \"not-a-version\"");
    assert!(parse_manifest(&json).is_err());
}

#[test]
fn rejects_invalid_runtime() {
    let json = VALID.replace("\"runtime\": \"sidecar\"", "\"runtime\": \"native\"");
    assert!(parse_manifest(&json).is_err());
}

#[test]
fn rejects_sidecar_without_backend() {
    let json = VALID.replace("bin/filee-cli", "");
    assert!(parse_manifest(&json).is_err());
}

#[test]
fn rejects_unknown_permission() {
    let json = VALID.replace("\"clipboard:write\"", "\"fs:read:everything\"");
    assert!(parse_manifest(&json).is_err());
}

#[test]
fn rejects_unknown_contribution_type() {
    let json = VALID.replace("\"type\": \"settings\"", "\"type\": \"magic\"");
    assert!(parse_manifest(&json).is_err());
}

#[test]
fn iframe_runtime_defaults_demand_lifecycle() {
    let json = r#"{"schema":1,"id":"com.x.y","name":"Y","version":"1.0.0","min_core":"0.1.0","runtime":"iframe"}"#;
    let m = parse_manifest(json).unwrap();
    assert_eq!(m.runtime, ModuleRuntime::Iframe);
    assert_eq!(m.lifecycle, Lifecycle::Demand);
    assert!(m.permissions.is_empty());
    assert!(m.contributions.is_empty());
}
