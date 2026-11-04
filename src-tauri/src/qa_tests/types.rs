//! 剪贴板类型工具函数测试：hash 去重、预览截断、HTML 去标签、字节统计。

use crate::core::clipboard::types::{hash_content, ClipContent, ContentType, SyncState};

#[test]
fn hash_content_is_deterministic_and_distinct() {
    let a = ClipContent::Text("hello".to_string());
    let b = ClipContent::Text("hello".to_string());
    let c = ClipContent::Text("world".to_string());
    assert_eq!(hash_content(&a), hash_content(&b));
    assert_ne!(hash_content(&a), hash_content(&c));
}

#[test]
fn hash_content_differs_across_types() {
    let text = ClipContent::Text("x".to_string());
    let html = ClipContent::Html("x".to_string());
    assert_ne!(hash_content(&text), hash_content(&html));
}

#[test]
fn preview_truncates_and_adds_ellipsis() {
    let content = ClipContent::Text("abcdefghij".to_string());
    assert_eq!(content.preview(3), "abc…");
    // 未超长时不加省略号。
    assert_eq!(content.preview(20), "abcdefghij");
}

#[test]
fn preview_normalizes_whitespace() {
    let content = ClipContent::Text("line1\nline2\ttab".to_string());
    assert_eq!(content.preview(200), "line1 line2 tab");
}

#[test]
fn preview_handles_emoji_without_panicking() {
    let content = ClipContent::Text("🔥🔥🔥🔥".to_string());
    // 4 个 emoji 字符，截 2 个。
    assert_eq!(content.preview(2), "🔥🔥…");
}

#[test]
fn preview_for_image_and_files() {
    let image = ClipContent::Image {
        path: "/tmp/a.png".to_string(),
        width: 10,
        height: 20,
    };
    assert_eq!(image.preview(200), "🖼 /tmp/a.png");

    let files = ClipContent::Files(vec!["/a.txt".to_string(), "/b.txt".to_string()]);
    assert_eq!(files.preview(200), "/a.txt, /b.txt");
}

#[test]
fn size_bytes_per_variant() {
    assert_eq!(ClipContent::Text("abc".to_string()).size_bytes(), 3);
    assert_eq!(ClipContent::Html("<b>x</b>".to_string()).size_bytes(), 8);
    assert_eq!(
        ClipContent::Image { path: String::new(), width: 0, height: 0 }.size_bytes(),
        0
    );
    assert_eq!(
        ClipContent::Files(vec!["/abc".to_string(), "/de".to_string()]).size_bytes(),
        7
    );
}

#[test]
fn size_bytes_for_image_reads_file_length() {
    let dir = std::env::temp_dir().join(format!("halo_qa_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("img.png");
    std::fs::write(&path, [0u8; 42]).unwrap();
    let content = ClipContent::Image {
        path: path.to_string_lossy().into_owned(),
        width: 1,
        height: 1,
    };
    assert_eq!(content.size_bytes(), 42);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn search_text_strips_html_and_joins_files() {
    assert_eq!(ClipContent::Text("hi".to_string()).search_text(), "hi");
    assert_eq!(ClipContent::Html("<p>hello <b>world</b></p>".to_string()).search_text(), "hello world");
    assert_eq!(
        ClipContent::Files(vec!["/a.txt".to_string(), "/b.txt".to_string()]).search_text(),
        "/a.txt /b.txt"
    );
}

#[test]
fn content_type_as_str_roundtrip() {
    assert_eq!(ContentType::Text.as_str(), "text");
    assert_eq!(ContentType::Html.as_str(), "html");
    assert_eq!(ContentType::Image.as_str(), "image");
    assert_eq!(ContentType::Files.as_str(), "files");
    assert_eq!(ContentType::Unknown.as_str(), "unknown");
}

#[test]
fn clip_content_serde_roundtrip() {
    let content = ClipContent::Text("hello 世界".to_string());
    let json = serde_json::to_string(&content).unwrap();
    let back: ClipContent = serde_json::from_str(&json).unwrap();
    assert_eq!(content, back);
}

#[test]
fn sync_state_serde_roundtrip() {
    for state in [SyncState::Local, SyncState::Synced, SyncState::Dirty] {
        let json = serde_json::to_string(&state).unwrap();
        let back: SyncState = serde_json::from_str(&json).unwrap();
        assert_eq!(state, back);
    }
}
