//! 同步合并策略测试：墓碑、时间戳保真、敏感内容不落地。
//!
//! 这些用例只覆盖「快照 → 本地库」的合并（`SyncManager::merge_remote`），
//! 不碰网络：WebDAV 只是快照的搬运工，真正容易写错的是这块合并逻辑。

use crate::config::ConfigService;
use crate::core::clipboard::types::{hash_content, ClipContent, ClipItem, ContentType, SyncState};
use crate::core::history::HistoryService;
use crate::qa_tests::test_db;
use crate::sync::manager::SyncManager;
use crate::sync::provider::{SyncCursor, SyncSnapshot, SyncTombstone};

fn setup() -> (SyncManager, HistoryService) {
    let db = test_db();
    let config = ConfigService::new(db.clone());
    let history = HistoryService::new(db.clone(), config.clone());
    let manager = SyncManager::new(db, history.clone(), config);
    (manager, history)
}

fn item(text: &str, created_at: i64) -> ClipItem {
    let content = ClipContent::Text(text.to_string());
    ClipItem {
        id: 0,
        content_type: ContentType::Text,
        content: content.clone(),
        preview: String::new(),
        source_app: Some("RemoteApp".to_string()),
        source_app_icon: None,
        tag: None,
        created_at,
        last_used_at: created_at,
        pinned: false,
        favorite: false,
        sensitive: false,
        size_bytes: content.size_bytes(),
        hash: hash_content(&content),
        sync_state: SyncState::Synced,
    }
}

fn snapshot(items: Vec<ClipItem>, deleted: Vec<SyncTombstone>) -> SyncSnapshot {
    SyncSnapshot {
        items,
        deleted,
        cursor: SyncCursor::default(),
        updated_at: 1,
    }
}

#[test]
fn merge_preserves_remote_created_at() {
    let (manager, history) = setup();
    // 三天前在另一台设备上复制的内容。
    let created = 1_600_000_000_000;
    let remote = snapshot(vec![item("来自另一台设备", created)], Vec::new());

    assert_eq!(manager.merge_remote(&remote).unwrap(), 1);

    let stored = history.all().unwrap();
    assert_eq!(stored.len(), 1);
    // 关键：不能变成「同步这一刻」的时间，否则所有远端条目都会被顶到最前面。
    assert_eq!(stored[0].created_at, created);
}

#[test]
fn merge_does_not_touch_existing_local_items() {
    let (manager, history) = setup();
    let local_created = 1_500_000_000_000;
    history
        .insert_with_time(item("两边都有", local_created), local_created)
        .unwrap();

    // 远端同内容、更新时间更晚。
    let remote = snapshot(vec![item("两边都有", 1_700_000_000_000)], Vec::new());
    assert_eq!(manager.merge_remote(&remote).unwrap(), 0);

    let stored = history.all().unwrap();
    assert_eq!(stored.len(), 1);
    // 本地记录的时间与用户操作（置顶/收藏/标签）都不能被一次同步改掉。
    assert_eq!(stored[0].created_at, local_created);
}

#[test]
fn remote_tombstone_deletes_older_local_item() {
    let (manager, history) = setup();
    let created = 1_500_000_000_000;
    history.insert_with_time(item("被别的设备删了", created), created).unwrap();
    let hash = item("被别的设备删了", created).hash;

    let remote = snapshot(
        Vec::new(),
        vec![SyncTombstone {
            hash,
            deleted_at: created + 1_000,
        }],
    );
    assert_eq!(manager.merge_remote(&remote).unwrap(), 0);
    assert!(history.all().unwrap().is_empty());
}

#[test]
fn remote_tombstone_keeps_item_copied_again_after_delete() {
    let (manager, history) = setup();
    let deleted_at = 1_500_000_000_000i64;
    // 删除之后用户又复制了一次同一条内容。
    let recreated = deleted_at + 5_000;
    history
        .insert_with_time(item("删掉后又复制", recreated), recreated)
        .unwrap();
    let hash = item("删掉后又复制", recreated).hash;

    let remote = snapshot(Vec::new(), vec![SyncTombstone { hash, deleted_at }]);
    manager.merge_remote(&remote).unwrap();

    // 后复制的内容比删除更晚：不能被墓碑误删。
    assert_eq!(history.all().unwrap().len(), 1);
}

#[test]
fn tombstoned_item_is_not_resurrected_by_remote_snapshot() {
    let (manager, history) = setup();
    let created = 1_500_000_000_000;
    let id = history.insert_with_time(item("删了别再回来", created), created).unwrap();
    history.delete(id).unwrap();

    // 远端快照里还留着这条（另一台设备还没同步过）。
    let remote = snapshot(vec![item("删了别再回来", created)], Vec::new());
    assert_eq!(manager.merge_remote(&remote).unwrap(), 0);
    assert!(history.all().unwrap().is_empty());
}

#[test]
fn copying_again_clears_the_tombstone() {
    let (manager, history) = setup();
    let created = 1_500_000_000_000;
    let id = history.insert_with_time(item("还要", created), created).unwrap();
    history.delete(id).unwrap();

    // 用户又复制了一次同样的内容（created_at 刷新为当前）。
    let again = created + 10_000;
    history.insert_with_time(item("还要", again), again).unwrap();

    let remote = snapshot(vec![], vec![SyncTombstone {
        hash: item("还要", again).hash,
        deleted_at: created,
    }]);
    manager.merge_remote(&remote).unwrap();
    assert_eq!(history.all().unwrap().len(), 1);
}

#[test]
fn sensitive_items_never_leave_the_device() {
    let (manager, history) = setup();
    let mut secret = item("password=hunter2", 1_500_000_000_000);
    secret.sensitive = true;
    history
        .insert_with_time(secret.clone(), secret.created_at)
        .unwrap();

    // 推送侧：快照里不能出现这条。
    assert!(manager.build_snapshot().unwrap().items.is_empty());

    // 拉取侧：即使远端快照带着它，本机也不收。
    let (other, other_history) = setup();
    let remote = snapshot(vec![secret], Vec::new());
    assert_eq!(other.merge_remote(&remote).unwrap(), 0);
    assert!(other_history.all().unwrap().is_empty());
}

#[test]
fn image_items_without_local_file_are_skipped() {
    let (manager, history) = setup();
    let content = ClipContent::Image {
        path: "/another-machine/data/files/clip_deadbeef.png".to_string(),
        width: 10,
        height: 10,
    };
    let mut image = item("", 1_500_000_000_000);
    image.content = content.clone();
    image.content_type = ContentType::Image;
    image.hash = hash_content(&content);
    history
        .insert_with_time(image.clone(), image.created_at)
        .unwrap();

    // 推送侧：本机没有这张图，不要上传一条指向别人磁盘的空记录。
    assert!(manager.build_snapshot().unwrap().items.is_empty());

    // 拉取侧：另一台机器上也不应该凭空多出一条打不开的图片记录。
    let (other, other_history) = setup();
    let remote = snapshot(vec![image], Vec::new());
    assert_eq!(other.merge_remote(&remote).unwrap(), 0);
    assert!(other_history.all().unwrap().is_empty());
}
