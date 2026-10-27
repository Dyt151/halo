//! 敏感内容识别（启发式规则）测试。

use crate::core::clipboard::types::ClipContent;
use crate::core::sensitivity::SensitivityDetector;

fn detect(text: &str) -> bool {
    SensitivityDetector::new()
        .detect(&ClipContent::Text(text.to_string()))
        .sensitive
}

#[test]
fn detects_jwt_prefix() {
    assert!(detect("eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.signature"));
}

#[test]
fn detects_github_token() {
    assert!(detect("ghp_abcdefghijklmnopqrstuvwxyz123456"));
}

#[test]
fn detects_sk_api_key() {
    assert!(detect("sk-proj-abcdef123456"));
}

#[test]
fn detects_credential_pair() {
    assert!(detect("password=hunter2"));
    assert!(detect("api_key=deadbeef"));
    assert!(detect("TOKEN = secret_value"));
}

#[test]
fn detects_private_key_block() {
    assert!(detect("-----BEGIN RSA PRIVATE KEY-----"));
}

#[test]
fn clean_plain_text() {
    assert!(!detect("hello world"));
    assert!(!detect("今天天气不错，适合散步。"));
    assert!(!detect("https://example.com/path?query=1"));
}

#[test]
fn clean_for_image_and_files() {
    let det = SensitivityDetector::new();
    let image = ClipContent::Image { path: "a.png".into(), width: 1, height: 1 };
    assert!(!det.detect(&image).sensitive);
    let files = ClipContent::Files(vec!["/tmp/secret.txt".to_string()]);
    assert!(!det.detect(&files).sensitive);
}

#[test]
fn empty_string_is_clean() {
    assert!(!detect(""));
}

// 记录当前已知局限：超长文本（>400 字符）即使内含敏感词也不会被标记，
// 属「启发式规则可能漏判」，与 IMPLEMENTATION.md 已知问题 #2 一致。
#[test]
fn long_text_with_secret_is_not_flagged_by_design() {
    let mut s = "x".repeat(500);
    s.push_str(" password=hunter2");
    assert!(!detect(&s));
}
