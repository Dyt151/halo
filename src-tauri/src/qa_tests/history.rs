//! 历史服务测试：CRUD、去重、容量策略、FTS5 全文搜索。

use std::sync::{Arc, Mutex};

use crate::config::ConfigService;
use crate::core::clipboard::types::{hash_content, ClipContent, ClipItem, ContentType, HistoryQuery, SyncState};
use crate::core::history::HistoryService;
use crate::qa_tests::test_db;

const MB: u64 = 1024 * 1024;

fn svc(max_items: u32, max_bytes: u64) -> HistoryService {
    let db = test_db();
    let config = ConfigService::new(db.clone());
    let mut cfg = config.get();
    cfg.max_items = max_items;
    cfg.max_bytes = max_bytes;
    config.save(cfg).unwrap();
    HistoryService::new(db, config)
}

fn item(text: &str) -> ClipItem {
    let content = ClipContent::Text(text.to_string());
    ClipItem {
        id: 0,
        content_type: ContentType::Text,
        content: content.clone(),
        preview: String::new(),
        source_app: Some("TestApp".to_string()),
        source_app_icon: None,
        tag: None,
        created_at: 0,
        last_used_at: 0,
        pinned: false,
        favorite: false,
        sensitive: false,
        size_bytes: content.size_bytes(),
        hash: hash_content(&content),
        sync_state: SyncState::Local,
    }
}

fn query_all(svc: &HistoryService) -> Vec<ClipItem> {
    svc.query(HistoryQuery {
        keyword: None,
        content_type: None,
        pinned_only: false,
        favorite_only: false,
        tag: None,
        limit: 1000,
        offset: 0,
    })
    .unwrap()
}

fn item_with_tag(text: &str, tag: Option<String>) -> ClipItem {
    let mut it = item(text);
    it.tag = tag;
    it
}

#[test]
fn insert_and_get_roundtrip() {
    let svc = svc(100, MB);
    let id = svc.insert(item("hello world")).unwrap();
    assert!(id > 0);

    let got = svc.get(id).unwrap();
    assert_eq!(got.id, id);
    assert_eq!(got.content, ClipContent::Text("hello world".to_string()));
    assert_eq!(got.preview, "hello world");
    assert_eq!(got.source_app.as_deref(), Some("TestApp"));
    assert_eq!(got.source_app_icon, None, "未导出图标时应为 None");
    assert_eq!(got.content_type, ContentType::Text);
    assert_eq!(got.size_bytes, 11);
    assert_eq!(got.sync_state, SyncState::Local);
}

#[test]
fn insert_and_get_roundtrip_with_app_icon() {
    let svc = svc(100, MB);
    let mut it = item("with icon");
    it.source_app_icon = Some("/tmp/icons/Safari.png".to_string());
    let id = svc.insert(it).unwrap();

    let got = svc.get(id).unwrap();
    assert_eq!(
        got.source_app_icon.as_deref(),
        Some("/tmp/icons/Safari.png"),
        "source_app_icon 应原样往返"
    );

    let queried = query_all(&svc);
    assert_eq!(queried.len(), 1);
    assert_eq!(
        queried[0].source_app_icon.as_deref(),
        Some("/tmp/icons/Safari.png"),
        "query 路径也应带上 source_app_icon"
    );
}

#[test]
fn insert_normalizes_preview_whitespace() {
    let svc = svc(100, MB);
    let id = svc.insert(item("line1\nline2\ttab")).unwrap();
    let got = svc.get(id).unwrap();
    assert_eq!(got.preview, "line1 line2 tab");
}

#[test]
fn insert_dedups_same_hash() {
    let svc = svc(100, MB);
    let id1 = svc.insert(item("same content")).unwrap();
    let id2 = svc.insert(item("same content")).unwrap();
    assert_eq!(id1, id2, "重复内容应复用同一条记录");
    assert_eq!(svc.stats().unwrap().total, 1);
}

#[test]
fn query_orders_pinned_first() {
    let svc = svc(100, MB);
    let a = svc.insert(item("unpinned")).unwrap();
    let b = svc.insert(item("pinned")).unwrap();
    svc.pin(b, true).unwrap();

    let items = query_all(&svc);
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].id, b, "置顶项应排最前");
    assert_eq!(items[1].id, a);
}

#[test]
fn delete_and_get_missing() {
    let svc = svc(100, MB);
    let id = svc.insert(item("to delete")).unwrap();
    svc.delete(id).unwrap();
    assert!(svc.get(id).is_err());
    assert_eq!(svc.stats().unwrap().total, 0);
}

#[test]
fn clear_removes_all() {
    let svc = svc(100, MB);
    svc.insert(item("a")).unwrap();
    svc.insert(item("b")).unwrap();
    svc.clear().unwrap();
    assert_eq!(svc.stats().unwrap().total, 0);
}

#[test]
fn pin_favorite_touch_work() {
    let svc = svc(100, MB);
    let id = svc.insert(item("x")).unwrap();
    svc.pin(id, true).unwrap();
    svc.favorite(id, true).unwrap();
    let got = svc.get(id).unwrap();
    assert!(got.pinned);
    assert!(got.favorite);

    let before = got.last_used_at;
    svc.touch(id).unwrap();
    let after = svc.get(id).unwrap().last_used_at;
    assert!(after >= before);
}

#[test]
fn favorite_only_filters_favorited_items() {
    let svc = svc(100, MB);
    let a = svc.insert(item("plain")).unwrap();
    let b = svc.insert(item("starred")).unwrap();
    svc.favorite(b, true).unwrap();

    let all = query_all(&svc);
    assert_eq!(all.len(), 2);

    let fav = svc
        .query(HistoryQuery {
            keyword: None,
            content_type: None,
            pinned_only: false,
            favorite_only: true,
            tag: None,
            limit: 10,
            offset: 0,
        })
        .unwrap();
    assert_eq!(fav.len(), 1);
    assert_eq!(fav[0].id, b);
    assert_ne!(fav[0].id, a);
}

#[test]
fn tag_filter_returns_only_matching_tag() {
    let svc = svc(100, MB);
    let a = svc
        .insert(item_with_tag("work one", Some("work".to_string())))
        .unwrap();
    let b = svc
        .insert(item_with_tag("work two", Some("work".to_string())))
        .unwrap();
    let c = svc
        .insert(item_with_tag("personal stuff", Some("personal".to_string())))
        .unwrap();
    svc.insert(item("untagged")).unwrap();

    let work = svc
        .query(HistoryQuery {
            keyword: None,
            content_type: None,
            pinned_only: false,
            favorite_only: false,
            tag: Some("work".to_string()),
            limit: 10,
            offset: 0,
        })
        .unwrap();
    assert_eq!(work.len(), 2, "按 tag=work 只应命中两条");
    assert!(work.iter().all(|i| i.tag.as_deref() == Some("work")));
    assert!(work.iter().any(|i| i.id == a));
    assert!(work.iter().any(|i| i.id == b));

    let personal = svc
        .query(HistoryQuery {
            keyword: None,
            content_type: None,
            pinned_only: false,
            favorite_only: false,
            tag: Some("personal".to_string()),
            limit: 10,
            offset: 0,
        })
        .unwrap();
    assert_eq!(personal.len(), 1);
    assert_eq!(personal[0].id, c);

    let other = svc
        .query(HistoryQuery {
            keyword: None,
            content_type: None,
            pinned_only: false,
            favorite_only: false,
            tag: Some("nonexistent".to_string()),
            limit: 10,
            offset: 0,
        })
        .unwrap();
    assert!(other.is_empty(), "按其他 tag 应查不到");

    // None = 不按标签筛选，应返回全部（含未分组）。
    assert_eq!(query_all(&svc).len(), 4);
}

#[test]
fn set_tag_updates_and_clears() {
    let svc = svc(100, MB);
    let id = svc.insert(item("taggable")).unwrap();
    assert_eq!(svc.get(id).unwrap().tag, None, "插入时未带标签应为 None");

    svc.set_tag(id, Some("工作".to_string())).unwrap();
    let got = svc.get(id).unwrap();
    assert_eq!(got.tag.as_deref(), Some("工作"), "set_tag 应写入标签");

    let by_tag = svc
        .query(HistoryQuery {
            keyword: None,
            content_type: None,
            pinned_only: false,
            favorite_only: false,
            tag: Some("工作".to_string()),
            limit: 10,
            offset: 0,
        })
        .unwrap();
    assert_eq!(by_tag.len(), 1, "按新标签应能查到");
    assert_eq!(by_tag[0].id, id);

    svc.set_tag(id, None).unwrap();
    assert_eq!(svc.get(id).unwrap().tag, None, "None 应清除标签");
    let by_tag_after = svc
        .query(HistoryQuery {
            keyword: None,
            content_type: None,
            pinned_only: false,
            favorite_only: false,
            tag: Some("工作".to_string()),
            limit: 10,
            offset: 0,
        })
        .unwrap();
    assert!(by_tag_after.is_empty(), "清除后按原标签应查不到");
}

#[test]
fn stats_counts_items_and_bytes() {
    let svc = svc(100, MB);
    svc.insert(item("abc")).unwrap(); // 3 bytes
    svc.insert(item("de")).unwrap(); // 2 bytes
    let stats = svc.stats().unwrap();
    assert_eq!(stats.total, 2);
    assert_eq!(stats.total_bytes, 5);
}

#[test]
fn exists_hash_checks_presence() {
    let svc = svc(100, MB);
    let it = item("find me");
    let hash = it.hash.clone();
    assert!(!svc.exists_hash(&hash).unwrap());
    svc.insert(it).unwrap();
    assert!(svc.exists_hash(&hash).unwrap());
}

#[test]
fn capacity_limits_by_item_count() {
    let svc = svc(2, MB);
    svc.insert(item("first")).unwrap();
    svc.insert(item("second")).unwrap();
    svc.insert(item("third")).unwrap();
    assert_eq!(svc.stats().unwrap().total, 2, "超出 max_items 应淘汰最旧条目");
}

#[test]
fn zero_capacity_falls_back_to_defaults() {
    // 0 是「没设过」而不是「一条不留」：手改配置/旧版本遗留的 0 不该让
    // 下一次入库把历史删空（删除对用户不可见，也无从恢复）。
    let svc = svc(0, 0);
    svc.insert(item("keep first")).unwrap();
    svc.insert(item("keep second")).unwrap();
    assert_eq!(
        svc.stats().unwrap().total,
        2,
        "max_items / max_bytes 为 0 时应回落到默认上限，而不是清空历史"
    );
}

#[test]
fn capacity_limits_by_total_bytes() {
    let svc = svc(1000, 30);
    svc.insert(item("aaaaaaaaaaaaaaaaaaaaa")).unwrap(); // 21 bytes
    svc.insert(item("bbbbbbbbbbbbbbbbbbbbb")).unwrap(); // 21 bytes
    // 总量 42 > 30，应淘汰最旧的 1 条，保留 1 条。
    let stats = svc.stats().unwrap();
    assert_eq!(stats.total, 1, "超出 max_bytes 应淘汰最旧条目");
}

#[test]
fn oversized_single_item_is_evicted() {
    let svc = svc(1000, 5);
    svc.insert(item("this is way over five bytes")).unwrap();
    assert_eq!(svc.stats().unwrap().total, 0);
}

#[test]
fn fts_search_english_keyword() {
    let svc = svc(100, MB);
    svc.insert(item("hello world")).unwrap();
    svc.insert(item("goodbye moon")).unwrap();

    let r = svc
        .query(HistoryQuery {
            keyword: Some("hello".to_string()),
            content_type: None,
            pinned_only: false,
            favorite_only: false,
            tag: None,
            limit: 10,
            offset: 0,
        })
        .unwrap();
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].preview, "hello world");
}

#[test]
fn fts_search_no_match_returns_empty() {
    let svc = svc(100, MB);
    svc.insert(item("hello world")).unwrap();
    let r = svc
        .query(HistoryQuery {
            keyword: Some("zzzzz".to_string()),
            content_type: None,
            pinned_only: false,
            favorite_only: false,
            tag: None,
            limit: 10,
            offset: 0,
        })
        .unwrap();
    assert!(r.is_empty());
}

#[test]
fn fts_search_chinese_full_token() {
    let svc = svc(100, MB);
    svc.insert(item("你好世界，今天天气不错")).unwrap();
    svc.insert(item("普通英文 plain text")).unwrap();

    let r = svc
        .query(HistoryQuery {
            keyword: Some("你好世界".to_string()),
            content_type: None,
            pinned_only: false,
            favorite_only: false,
            tag: None,
            limit: 10,
            offset: 0,
        })
        .unwrap();
    assert_eq!(r.len(), 1, "完整中文词组应可命中");
}

// 记录当前已知局限：FTS5 使用 unicode61 分词器，连续中文被视为单个 token，
// 子串搜索（如「你好」无法命中「你好世界」）不生效。若需中文分词需引入
// 专用 tokenizer（如 simple/trigram 或外部分词库）。属 IMPLEMENTATION.md
// 未明确列出的搜索体验局限，供团队决策。
#[test]
fn fts_search_chinese_substring_is_not_matched() {
    let svc = svc(100, MB);
    svc.insert(item("你好世界")).unwrap();

    let r = svc
        .query(HistoryQuery {
            keyword: Some("你好".to_string()),
            content_type: None,
            pinned_only: false,
            favorite_only: false,
            tag: None,
            limit: 10,
            offset: 0,
        })
        .unwrap();
    assert_eq!(r.len(), 0, "unicode61 分词器下子串无法命中（已知局限）");
}

#[test]
fn ttl_evicts_items_older_than_days() {
    use std::time::{SystemTime, UNIX_EPOCH};

    let db = test_db();
    let config = ConfigService::new(db.clone());
    let mut cfg = config.get();
    cfg.history_ttl_days = 1;
    config.save(cfg).unwrap();
    let svc = HistoryService::new(db.clone(), config);

    let old_id = svc.insert(item("old entry")).unwrap();
    let two_days_ago =
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as i64 - 2 * 86_400_000;
    {
        let conn = db.lock().unwrap();
        conn.execute(
            "UPDATE clips SET created_at = ?1 WHERE id = ?2",
            rusqlite::params![two_days_ago, old_id],
        )
        .unwrap();
    }

    // 插入新条目触发清理。
    svc.insert(item("fresh entry")).unwrap();

    assert!(svc.get(old_id).is_err(), "超过 TTL 的旧条目应被淘汰");
    assert_eq!(svc.stats().unwrap().total, 1);
}

#[test]
fn ttl_keeps_pinned_items() {
    use std::time::{SystemTime, UNIX_EPOCH};

    let db = test_db();
    let config = ConfigService::new(db.clone());
    let mut cfg = config.get();
    cfg.history_ttl_days = 1;
    config.save(cfg).unwrap();
    let svc = HistoryService::new(db.clone(), config);

    let pinned_id = svc.insert(item("pinned old")).unwrap();
    svc.pin(pinned_id, true).unwrap();
    let two_days_ago =
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as i64 - 2 * 86_400_000;
    {
        let conn = db.lock().unwrap();
        conn.execute(
            "UPDATE clips SET created_at = ?1 WHERE id = ?2",
            rusqlite::params![two_days_ago, pinned_id],
        )
        .unwrap();
    }

    svc.insert(item("fresh entry")).unwrap();

    assert!(svc.get(pinned_id).is_ok(), "置顶条目不受 TTL 淘汰");
    assert_eq!(svc.stats().unwrap().total, 2);
}

#[test]
fn fts_search_html_content_is_indexed() {
    let svc = svc(100, MB);
    let content = ClipContent::Html("<p>重要 <b>文档</b></p>".to_string());
    let mut it = item("");
    it.content_type = ContentType::Html;
    it.content = content.clone();
    it.hash = hash_content(&content);
    it.size_bytes = content.size_bytes();
    svc.insert(it).unwrap();

    let r = svc
        .query(HistoryQuery {
            keyword: Some("文档".to_string()),
            content_type: None,
            pinned_only: false,
            favorite_only: false,
            tag: None,
            limit: 10,
            offset: 0,
        })
        .unwrap();
    assert_eq!(r.len(), 1, "HTML 去标签后的文本应可检索");
}

// —— 图片文件生命周期与迁移维护 ——

/// 构造引用指定路径的图片条目（width/height 可区分 hash）。
fn image_item(path: &str, width: u32, height: u32, created_at: i64) -> ClipItem {
    let content = ClipContent::Image {
        path: path.to_string(),
        width,
        height,
    };
    ClipItem {
        id: 0,
        content_type: ContentType::Image,
        content: content.clone(),
        preview: String::new(),
        source_app: Some("TestApp".to_string()),
        source_app_icon: None,
        tag: None,
        created_at,
        last_used_at: 0,
        pinned: false,
        favorite: false,
        sensitive: false,
        size_bytes: content.size_bytes(),
        hash: hash_content(&content),
        sync_state: SyncState::Local,
    }
}

/// 生成一个临时 PNG 文件，返回其路径。
fn temp_png() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("halo_qa_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("img.png");
    image::RgbaImage::from_pixel(2, 3, image::Rgba([1, 2, 3, 255]))
        .save(&path)
        .unwrap();
    path
}

#[test]
fn replace_by_time_updates_row_in_place_and_preserves_user_flags() {
    let svc = svc(100, 100 * MB);
    let mut it = item("第一版内容");
    it.created_at = 42_000;
    svc.insert_with_time(it, 42_000).unwrap();

    let id = svc.all().unwrap()[0].id;
    svc.pin(id, true).unwrap();
    svc.favorite(id, true).unwrap();
    svc.set_tag(id, Some("组A".to_string())).unwrap();

    let mut it = item("第二版内容（重导入）");
    it.created_at = 42_000;
    let inserted = svc.replace_by_time(it).unwrap();
    assert!(!inserted, "同时间戳+来源应原位更新而非新增");

    let updated = svc.get(id).unwrap();
    assert_eq!(updated.content, ClipContent::Text("第二版内容（重导入）".to_string()));
    assert!(updated.pinned, "置顶状态应保留");
    assert!(updated.favorite, "收藏状态应保留");
    assert_eq!(updated.tag.as_deref(), Some("组A"), "标签应保留");
    assert_eq!(svc.all().unwrap().len(), 1, "不得产生重复行");

    // 不同时间戳应正常插入新行。
    let mut it = item("另一条");
    it.created_at = 43_000;
    assert!(svc.replace_by_time(it).unwrap());
}

#[test]
fn backfill_image_sizes_fills_size_and_missing_dims() {
    let svc = svc(100, 100 * MB);
    let path = temp_png();
    let p = path.to_string_lossy().into_owned();
    let mut it = image_item(&p, 0, 0, 1_000);
    it.size_bytes = 0;
    let id = svc.insert_with_time(it, 1_000).unwrap();

    let (fixed_sizes, fixed_dims) = svc.backfill_image_sizes().unwrap();
    assert_eq!((fixed_sizes, fixed_dims), (1, 1));

    let item = svc.get(id).unwrap();
    assert_eq!(item.size_bytes, std::fs::metadata(&path).unwrap().len());
    match item.content {
        ClipContent::Image { width, height, .. } => {
            assert_eq!((width, height), (2, 3), "宽高应从真实像素回填");
        }
        other => panic!("expected image, got {other:?}"),
    }
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

#[test]
fn delete_keeps_image_file_for_undo_and_restores_row() {
    let svc = svc(100, 100 * MB);
    let path = temp_png();
    let p = path.to_string_lossy().into_owned();

    let id1 = svc.insert_with_time(image_item(&p, 2, 3, 1_000), 1_000).unwrap();
    let id2 = svc.insert_with_time(image_item(&p, 2, 4, 1_001), 1_001).unwrap();

    svc.delete(id1).unwrap();
    assert!(path.exists(), "仍被 id2 引用，文件不得删除");

    svc.delete(id2).unwrap();
    assert!(path.exists(), "撤销栈仍持有记录，文件不得删除");

    assert!(svc.undo().unwrap().is_some());
    assert!(svc.undo().unwrap().is_some());
    assert_eq!(svc.all().unwrap().len(), 2, "撤销应恢复两条记录");
    assert!(path.exists());
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

#[test]
fn clear_removes_all_image_files() {
    let svc = svc(100, 100 * MB);
    let path = temp_png();
    let p = path.to_string_lossy().into_owned();
    svc.insert_with_time(image_item(&p, 2, 3, 1_000), 1_000).unwrap();
    svc.clear().unwrap();
    assert!(!path.exists());
}

/// 直接往库里塞一条旧版本遗留的坏行（`insert` 只会写合法 JSON）。
fn insert_raw(db: &Arc<Mutex<rusqlite::Connection>>, content_type: &str, content: &str, at: i64) {
    let conn = db.lock().unwrap();
    conn.execute(
        "INSERT INTO clips \
         (content_type, content, search_text, preview, created_at, last_used_at, hash, sync_state) \
         VALUES (?1, ?2, '', 'x', ?3, ?3, ?4, 'local')",
        rusqlite::params![content_type, content, at, format!("h{at}")],
    )
    .unwrap();
}

fn svc_over(db: &Arc<Mutex<rusqlite::Connection>>) -> HistoryService {
    HistoryService::new(db.clone(), ConfigService::new(db.clone()))
}

#[test]
fn query_skips_corrupt_rows_instead_of_failing() {
    let db = test_db();
    let service = svc_over(&db);
    service.insert_with_time(item("正常条目"), 2_000).unwrap();
    // 旧版本写坏的形态：字符串值少了引号。
    insert_raw(&db, "text", "{\"kind\":\"text\",\"value\":少了一对引号}", 3_000);

    // 以前这里整份查询直接报错（面板「一条都没有 + 看不懂的报错」），
    // 现在只跳过坏行，其它条目照常返回。
    let items = query_all(&service);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].content, ClipContent::Text("正常条目".to_string()));
}

#[test]
fn query_reports_error_when_every_row_is_corrupt() {
    let db = test_db();
    let service = svc_over(&db);
    insert_raw(&db, "text", "{ not json at all", 1_000);

    // 全坏时列表本来就空：必须报错，不能伪装成「暂无历史」。
    let err = service
        .query(HistoryQuery {
            keyword: None,
            content_type: None,
            pinned_only: false,
            favorite_only: false,
            tag: None,
            limit: 100,
            offset: 0,
        })
        .unwrap_err();
    assert_eq!(err.code, 2002);
}

#[test]
fn repair_corrupt_content_salvages_unquoted_json() {
    let db = test_db();
    let service = svc_over(&db);
    insert_raw(&db, "text", "{\"kind\":\"text\",\"value\":少了一对引号}", 1_000);
    insert_raw(&db, "html", "{\"kind\":\"html\",\"value\":\"<b>ok</b>\"}", 1_001);
    insert_raw(&db, "image", "{\"kind\":\"image\",\"value\":oops}", 1_002);

    // 只能修 text/html 这种「正文直接裸在 value 后」的形态；图片坏行救不回来，
    // 保持原样交给查询层跳过（绝不删用户数据）。
    assert_eq!(service.repair_corrupt_content(), 1);
    assert_eq!(service.repair_corrupt_content(), 0, "修复应当幂等");

    let items = query_all(&service);
    assert_eq!(items.len(), 2, "修好一条 + 本来就正常一条 + 无法修复的一条被跳过");
    assert!(items
        .iter()
        .any(|it| it.content == ClipContent::Text("少了一对引号".to_string())));
    assert!(items.iter().any(|it| it.content == ClipContent::Html("<b>ok</b>".to_string())));
}

#[test]
fn exists_tracks_insert_and_delete() {
    let svc = svc(100, MB);
    assert!(!svc.exists(4242).unwrap(), "不存在的 id 应为 false");
    let id = svc.insert(item("exists-check")).unwrap();
    assert!(svc.exists(id).unwrap());
    svc.delete(id).unwrap();
    assert!(!svc.exists(id).unwrap(), "删除后不应再能被压入粘贴堆栈");
}

#[test]
fn mutating_missing_clip_reports_2001() {
    let svc = svc(100, MB);
    // 面板/历史页是「渲染后再操作」的，条目可能已被同步侧删掉：
    // UPDATE 0 行不能静默当成成功。
    assert_eq!(svc.pin(4242, true).unwrap_err().code, 2001);
    assert_eq!(svc.favorite(4242, true).unwrap_err().code, 2001);
    assert_eq!(svc.set_tag(4242, Some("工作".to_string())).unwrap_err().code, 2001);
    assert_eq!(svc.set_tag(4242, Some("   ".to_string())).unwrap_err().code, 2001);
}

#[test]
fn set_tag_trims_and_limits_length() {
    let svc = svc(100, MB);
    let id = svc.insert(item("tag-limits")).unwrap();

    svc.set_tag(id, Some("  工作  ".to_string())).unwrap();
    assert_eq!(svc.get(id).unwrap().tag.as_deref(), Some("工作"), "首尾空白应被裁掉");

    svc.set_tag(id, Some("   ".to_string())).unwrap();
    assert_eq!(svc.get(id).unwrap().tag, None, "纯空白等同清除标签");

    let err = svc.set_tag(id, Some("标".repeat(65))).unwrap_err();
    assert_eq!(err.code, 2003);
    assert!(
        err.details.unwrap_or_default().contains("64"),
        "超长提示要说出上限"
    );
}
