//! 配置服务（ConfigService）读写、序列化往返与部分更新测试。

use crate::config::settings::{Config, SensitivityPolicy};
use crate::config::ConfigService;
use crate::qa_tests::test_db;

#[test]
fn defaults_on_empty_db() {
    let svc = ConfigService::new(test_db());
    let c = svc.get();
    assert_eq!(c.max_items, 2000);
    assert_eq!(c.max_bytes, 1024 * 1024 * 1024);
    assert!(c.monitor_enabled);
    assert_eq!(c.poll_interval_ms, 500);
    // 默认敏感策略为「脱敏入库」：命中的密码 / token 不以明文落盘，
    // 又不会像 Ignore 那样静默丢条目。
    assert_eq!(c.sensitivity, SensitivityPolicy::Mask);
    assert_eq!(c.hotkeys.toggle, "Control+Option+C");
    assert_eq!(c.hotkeys.stack_paste, "Control+Shift+V");
    assert!(!c.sync.enabled);
    assert_eq!(c.sync.remote_file, "halo-sync.json");
    assert_eq!(c.sync.interval_secs, 300);
    assert!(!c.launch_at_login);
    assert_eq!(c.theme, "system");
}

#[test]
fn new_feature_defaults() {
    let svc = ConfigService::new(test_db());
    let c = svc.get();
    assert!(!c.paste_as_plain);
    assert!(c.paste_trim_trailing);
    assert!(c.confirm_on_delete);
    assert!(c.auto_close_after_paste);
    assert!(!c.paste_on_single_click);
    assert!(c.sound_enabled);
    assert!(c.num_hotkey_enabled);
    assert!(c.stack_enabled);
    assert_eq!(c.board_style, "list");
    // 与 settings.rs 各默认值函数对齐。
    assert_eq!(c.board_height_scale, 50);
    assert_eq!(c.board_position, "bottom");
    assert_eq!(c.text_preview, "rich");
    assert_eq!(c.history_ttl_days, 0);
    assert!(c.app_blacklist.is_empty());
}

#[test]
fn config_serde_defaults_for_new_fields() {
    // 老配置 JSON（不含新字段）反序列化后应回退到默认值。
    let json = r#"{"max_items":100}"#;
    let c: Config = serde_json::from_str(json).unwrap();
    assert_eq!(c.max_items, 100);
    assert!(c.auto_close_after_paste);
    assert!(c.num_hotkey_enabled);
    assert_eq!(c.board_style, "list");
    assert!(c.app_blacklist.is_empty());
}

#[test]
fn save_and_get_roundtrip() {
    let db = test_db();
    let svc = ConfigService::new(db.clone());
    let mut c = svc.get();
    c.max_items = 123;
    c.max_bytes = 999;
    c.monitor_enabled = false;
    c.theme = "dark".to_string();
    svc.save(c.clone()).unwrap();

    let loaded = svc.get();
    assert_eq!(loaded.max_items, 123);
    assert_eq!(loaded.max_bytes, 999);
    assert!(!loaded.monitor_enabled);
    assert_eq!(loaded.theme, "dark");
}

#[test]
fn save_persists_across_service_instances() {
    let db = test_db();
    {
        let svc = ConfigService::new(db.clone());
        let mut c = svc.get();
        c.max_items = 42;
        svc.save(c).unwrap();
    }
    // 新实例从库中重载。
    let svc2 = ConfigService::new(db);
    assert_eq!(svc2.get().max_items, 42);
}

#[test]
fn patch_merges_only_given_fields() {
    let svc = ConfigService::new(test_db());
    let c = svc.patch(serde_json::json!({ "max_items": 77 })).unwrap();
    assert_eq!(c.max_items, 77);
    // 未修改字段保持默认。
    assert_eq!(c.max_bytes, 1024 * 1024 * 1024);
    assert!(c.monitor_enabled);
}

#[test]
fn patch_rejects_invalid_value() {
    let svc = ConfigService::new(test_db());
    assert!(svc.patch(serde_json::json!({ "max_items": "not-a-number" })).is_err());
}

#[test]
fn set_sync_config_updates_subblock() {
    let svc = ConfigService::new(test_db());
    let mut sync = svc.get().sync;
    sync.enabled = true;
    sync.encryption = true;
    sync.passphrase = "secret".to_string();
    let c = svc.set_sync_config(sync).unwrap();
    assert!(c.sync.enabled);
    assert!(c.sync.encryption);
    assert_eq!(c.sync.passphrase, "secret");
}

#[test]
fn config_serde_roundtrip() {
    let c = Config::default();
    let json = serde_json::to_string(&c).unwrap();
    let back: Config = serde_json::from_str(&json).unwrap();
    assert_eq!(c, back);
}
