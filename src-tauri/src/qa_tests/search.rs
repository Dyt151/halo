//! FTS5 查询构造测试。

use crate::core::clipboard::types::{ContentType, HistoryQuery};
use crate::core::history::search::{build_query, fts_query};

#[test]
fn fts_query_wraps_each_term_in_quotes() {
    assert_eq!(fts_query("hello world"), "\"hello\" \"world\"");
    assert_eq!(fts_query("foo"), "\"foo\"");
}

#[test]
fn fts_query_escapes_inner_quotes() {
    assert_eq!(fts_query("a\"b"), "\"a\"\"b\"");
}

#[test]
fn fts_query_empty_input() {
    assert_eq!(fts_query(""), "");
    assert_eq!(fts_query("   "), "");
}

#[test]
fn build_query_with_keyword() {
    let q = HistoryQuery {
        keyword: Some("hello".to_string()),
        content_type: None,
        pinned_only: false,
        favorite_only: false,
        tag: None,
        limit: 10,
        offset: 0,
    };
    let (sql, params) = build_query(&q);
    assert!(sql.contains("clips_fts MATCH ?"), "sql = {sql}");
    assert_eq!(params.len(), 1);
}

#[test]
fn build_query_with_type_and_pinned() {
    let q = HistoryQuery {
        keyword: None,
        content_type: Some(ContentType::Text),
        pinned_only: true,
        favorite_only: false,
        tag: None,
        limit: 10,
        offset: 0,
    };
    let (sql, params) = build_query(&q);
    assert!(sql.contains("c.content_type = ?"));
    assert!(sql.contains("c.pinned = 1"));
    // pinned 过滤是字面量，不占参数位；仅 content_type 占 1 个参数。
    assert_eq!(params.len(), 1);
}

#[test]
fn build_query_empty_has_no_where() {
    let q = HistoryQuery {
        keyword: None,
        content_type: None,
        pinned_only: false,
        favorite_only: false,
        tag: None,
        limit: 10,
        offset: 0,
    };
    let (sql, params) = build_query(&q);
    assert!(!sql.contains("WHERE"));
    assert!(params.is_empty());
}

#[test]
fn build_query_with_tag() {
    let q = HistoryQuery {
        keyword: None,
        content_type: None,
        pinned_only: false,
        favorite_only: false,
        tag: Some("work".to_string()),
        limit: 10,
        offset: 0,
    };
    let (sql, params) = build_query(&q);
    assert!(sql.contains("c.tag = ?"), "sql = {sql}");
    assert_eq!(params.len(), 1, "tag 过滤应占 1 个参数位");
}
