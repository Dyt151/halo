//! 按 App 黑名单匹配测试。

use crate::state::is_blacklisted;

#[test]
fn blacklist_matches_case_insensitive() {
    let blacklist = vec!["safari".to_string()];
    assert!(is_blacklisted("Safari", &blacklist));
    assert!(is_blacklisted("SAFARI", &blacklist));
}

#[test]
fn blacklist_matches_substring() {
    let blacklist = vec!["Chrome".to_string()];
    assert!(is_blacklisted("Google Chrome", &blacklist));
}

#[test]
fn blacklist_no_match() {
    let blacklist = vec!["Safari".to_string()];
    assert!(!is_blacklisted("TextEdit", &blacklist));
}

#[test]
fn empty_blacklist_matches_nothing() {
    assert!(!is_blacklisted("Safari", &[]));
}

#[test]
fn empty_entries_are_ignored() {
    let blacklist = vec!["".to_string(), "   ".to_string()];
    assert!(!is_blacklisted("Safari", &blacklist));
}
