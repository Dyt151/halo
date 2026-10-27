//! 快捷回复服务测试：分组 CRUD 与级联删除、回复 CRUD、分组/关键词查询、排序交换。

use crate::core::reply::{Reply, ReplyGroup, ReplyService};
use crate::qa_tests::test_db;

fn svc() -> ReplyService {
    ReplyService::new(test_db())
}

fn all_replies(svc: &ReplyService) -> Vec<Reply> {
    svc.list_replies(None, None).unwrap()
}

#[test]
fn group_crud_roundtrip() {
    let svc = svc();
    assert!(svc.list_groups().unwrap().is_empty());

    let a = svc.add_group("工作".to_string()).unwrap();
    let b = svc.add_group("生活".to_string()).unwrap();
    assert!(a.id > 0 && b.id > a.id, "id 自增且组依次追加");
    assert_eq!(a.sort, 0);
    assert_eq!(b.sort, 1, "新分组 sort 追加到末尾");

    let groups = svc.list_groups().unwrap();
    assert_eq!(groups.len(), 2);
    assert_eq!(
        groups,
        vec![
            ReplyGroup { id: a.id, name: "工作".to_string(), sort: 0 },
            ReplyGroup { id: b.id, name: "生活".to_string(), sort: 1 },
        ]
    );

    svc.rename_group(a.id, "工作常用".to_string()).unwrap();
    let renamed = svc.list_groups().unwrap();
    assert_eq!(renamed[0].name, "工作常用");

    svc.delete_group(b.id).unwrap();
    let after = svc.list_groups().unwrap();
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].id, a.id);
}

#[test]
fn rename_missing_group_errors() {
    let svc = svc();
    assert!(svc.rename_group(999, "x".to_string()).is_err());
}

#[test]
fn delete_group_cascades_replies() {
    let svc = svc();
    let g1 = svc.add_group("g1".to_string()).unwrap();
    let g2 = svc.add_group("g2".to_string()).unwrap();
    let r1 = svc.add_reply(g1.id, "t1".into(), "c1".into(), vec![]).unwrap();
    let r2 = svc.add_reply(g1.id, "t2".into(), "c2".into(), vec![]).unwrap();
    let r3 = svc.add_reply(g2.id, "t3".into(), "c3".into(), vec![]).unwrap();

    svc.delete_group(g1.id).unwrap();

    let remaining = all_replies(&svc);
    assert_eq!(remaining.len(), 1, "删除分组应级联删除组内回复");
    assert_eq!(remaining[0].id, r3.id);
    assert!(svc.get_reply(r1.id).is_err());
    assert!(svc.get_reply(r2.id).is_err());
    // 被删分组自身也不在了。
    assert_eq!(svc.list_groups().unwrap().len(), 1);
}

#[test]
fn reply_crud_roundtrip() {
    let svc = svc();
    let g = svc.add_group("常用".to_string()).unwrap();
    let files = vec!["/tmp/a.txt".to_string(), "/tmp/b.png".to_string()];
    let r = svc
        .add_reply(g.id, "标题".into(), "正文内容".into(), files.clone())
        .unwrap();
    assert!(r.id > 0);
    assert!(r.created_at > 0.0, "created_at 应写入当前时间戳");

    let got = svc.get_reply(r.id).unwrap();
    assert_eq!(got.title, "标题");
    assert_eq!(got.content, "正文内容");
    assert_eq!(got.files, files, "files JSON 数组应原样往返");

    svc.update_reply(r.id, "新标题".into(), "新内容".into(), vec![]).unwrap();
    let updated = svc.get_reply(r.id).unwrap();
    assert_eq!(updated.title, "新标题");
    assert_eq!(updated.content, "新内容");
    assert!(updated.files.is_empty(), "更新后附件应被替换");

    svc.delete_reply(r.id).unwrap();
    assert!(svc.get_reply(r.id).is_err());
    assert!(all_replies(&svc).is_empty());
}

#[test]
fn add_reply_appends_sort_within_group() {
    let svc = svc();
    let g = svc.add_group("g".to_string()).unwrap();
    let r1 = svc.add_reply(g.id, "a".into(), "".into(), vec![]).unwrap();
    let r2 = svc.add_reply(g.id, "b".into(), "".into(), vec![]).unwrap();
    let r3 = svc.add_reply(g.id, "c".into(), "".into(), vec![]).unwrap();
    assert_eq!((r1.sort, r2.sort, r3.sort), (0, 1, 2));

    let list = all_replies(&svc);
    assert_eq!(
        list.iter().map(|r| r.id).collect::<Vec<_>>(),
        vec![r1.id, r2.id, r3.id],
        "默认按 sort 升序返回"
    );
}

#[test]
fn list_replies_filters_by_group() {
    let svc = svc();
    let g1 = svc.add_group("g1".to_string()).unwrap();
    let g2 = svc.add_group("g2".to_string()).unwrap();
    let a = svc.add_reply(g1.id, "a".into(), "".into(), vec![]).unwrap();
    svc.add_reply(g1.id, "b".into(), "".into(), vec![]).unwrap();
    let c = svc.add_reply(g2.id, "c".into(), "".into(), vec![]).unwrap();

    let in_g1 = svc.list_replies(Some(g1.id), None).unwrap();
    assert_eq!(in_g1.len(), 2);
    assert!(in_g1.iter().all(|r| r.group_id == g1.id));

    let in_g2 = svc.list_replies(Some(g2.id), None).unwrap();
    assert_eq!(in_g2.len(), 1);
    assert_eq!(in_g2[0].id, c.id);

    let all = svc.list_replies(None, None).unwrap();
    assert_eq!(all.len(), 3, "group_id 为 None 时返回全部");
    assert!(all.iter().any(|r| r.id == a.id));
}

#[test]
fn list_replies_filters_by_keyword_on_title_and_content() {
    let svc = svc();
    let g = svc.add_group("g".to_string()).unwrap();
    svc.add_reply(g.id, "周报模板".into(), "请查收附件".into(), vec![]).unwrap();
    svc.add_reply(g.id, "问候".into(), "早上好呀".into(), vec![]).unwrap();
    svc.add_reply(g.id, "会议".into(), "明天周报评审".into(), vec![]).unwrap();

    let by_title = svc.list_replies(None, Some("周报".to_string())).unwrap();
    assert_eq!(by_title.len(), 2, "标题命中一条、内容命中一条");
    assert!(by_title.iter().all(|r| r.title.contains("周报") || r.content.contains("周报")));

    let by_content = svc.list_replies(None, Some("附件".to_string())).unwrap();
    assert_eq!(by_content.len(), 1);
    assert_eq!(by_content[0].title, "周报模板");

    let none = svc.list_replies(None, Some("不存在的词".to_string())).unwrap();
    assert!(none.is_empty());

    // 关键词与分组条件可叠加。
    let both = svc
        .list_replies(Some(g.id), Some("周报".to_string()))
        .unwrap();
    assert_eq!(both.len(), 2);
}

#[test]
fn keyword_like_wildcards_are_escaped() {
    let svc = svc();
    let g = svc.add_group("g".to_string()).unwrap();
    svc.add_reply(g.id, "100%完成".into(), "".into(), vec![]).unwrap();
    svc.add_reply(g.id, "普通".into(), "".into(), vec![]).unwrap();

    let r = svc.list_replies(None, Some("100%".to_string())).unwrap();
    assert_eq!(r.len(), 1, "% 应按字面匹配而非通配符");
    assert_eq!(r[0].title, "100%完成");
}

#[test]
fn move_reply_swaps_adjacent_sort() {
    let svc = svc();
    let g = svc.add_group("g".to_string()).unwrap();
    let r1 = svc.add_reply(g.id, "1".into(), "".into(), vec![]).unwrap();
    let r2 = svc.add_reply(g.id, "2".into(), "".into(), vec![]).unwrap();
    let r3 = svc.add_reply(g.id, "3".into(), "".into(), vec![]).unwrap();

    // 中间项上移：与 r1 交换。
    svc.move_reply(r2.id, 1).unwrap();
    let ids: Vec<i64> = all_replies(&svc).iter().map(|r| r.id).collect();
    assert_eq!(ids, vec![r2.id, r1.id, r3.id], "上移应与前一项交换");

    // 再下移回来。
    svc.move_reply(r2.id, -1).unwrap();
    let ids: Vec<i64> = all_replies(&svc).iter().map(|r| r.id).collect();
    assert_eq!(ids, vec![r1.id, r2.id, r3.id], "下移应与后一项交换");

    // 首项上移、末项下移：无操作。
    svc.move_reply(r1.id, 1).unwrap();
    svc.move_reply(r3.id, -1).unwrap();
    let ids: Vec<i64> = all_replies(&svc).iter().map(|r| r.id).collect();
    assert_eq!(ids, vec![r1.id, r2.id, r3.id], "边界移动应为无操作");

    // direction = 0：无操作。
    svc.move_reply(r1.id, 0).unwrap();
    let ids: Vec<i64> = all_replies(&svc).iter().map(|r| r.id).collect();
    assert_eq!(ids, vec![r1.id, r2.id, r3.id]);
}

#[test]
fn move_reply_stays_within_group() {
    let svc = svc();
    let g1 = svc.add_group("g1".to_string()).unwrap();
    let g2 = svc.add_group("g2".to_string()).unwrap();
    let a1 = svc.add_reply(g1.id, "a1".into(), "".into(), vec![]).unwrap();
    let b1 = svc.add_reply(g2.id, "b1".into(), "".into(), vec![]).unwrap();

    // g1 首项上移：不应跨组交换到 g2。
    svc.move_reply(a1.id, 1).unwrap();
    let a = svc.get_reply(a1.id).unwrap();
    assert_eq!(a.group_id, g1.id);
    assert_eq!(a.sort, 0);

    // g2 首项上移也不应影响 g1。
    svc.move_reply(b1.id, 1).unwrap();
    let b = svc.get_reply(b1.id).unwrap();
    assert_eq!(b.group_id, g2.id);
    assert_eq!(b.sort, 0);
}

#[test]
fn move_missing_reply_errors() {
    let svc = svc();
    let err = svc.move_reply(999, 1).unwrap_err();
    assert_eq!(err.code, 2005);
    assert!(
        err.details.unwrap_or_default().contains("id=999"),
        "details 应给 id，而不是 rusqlite 的英文原文"
    );
}

#[test]
fn missing_reply_reports_2005_instead_of_silent_success() {
    let svc = svc();
    for err in [
        svc.get_reply(4242).unwrap_err(),
        svc.delete_reply(4242).unwrap_err(),
        svc.update_reply(4242, "t".into(), "c".into(), vec![])
            .unwrap_err(),
    ] {
        assert_eq!(err.code, 2005);
        assert!(err.details.unwrap_or_default().contains("id=4242"));
    }
}

#[test]
fn add_reply_rejects_blank_group_and_blank_body() {
    let svc = svc();

    // 分组不存在：不能插一条「哪都看不到」的孤儿记录。
    let err = svc
        .add_reply(9999, "标题".into(), "正文".into(), vec![])
        .unwrap_err();
    assert_eq!(err.code, 2004);

    let g = svc.add_group("工作".to_string()).unwrap();
    // 标题 / 正文 / 附件全空 = 没有任何可发送的内容。
    let err = svc.add_reply(g.id, "   ".into(), "\n\t".into(), vec![]).unwrap_err();
    assert_eq!(err.code, 2006);

    // 只挂附件（无标题）是合法用法：前端回复选择器就是这么建的。
    svc.add_reply(g.id, String::new(), String::new(), vec!["/tmp/a.pdf".into()])
        .unwrap();

    // 标题首尾空白会被裁掉，避免列表里出现「看起来是空行」的条目。
    let r = svc
        .add_reply(g.id, "  日报 ".into(), "正文".into(), vec![])
        .unwrap();
    assert_eq!(r.title, "日报");
}

#[test]
fn group_name_is_trimmed_and_bounded() {
    let svc = svc();
    let g = svc.add_group("  工作  ".to_string()).unwrap();
    assert_eq!(g.name, "工作");
    assert_eq!(svc.add_group("   ".to_string()).unwrap_err().code, 2006);
    assert_eq!(svc.add_group("组".repeat(33)).unwrap_err().code, 2006);
}

#[test]
fn reply_to_clip_content_prefers_files() {
    let svc = svc();
    let g = svc.add_group("g".to_string()).unwrap();
    let text_only = svc.add_reply(g.id, "t".into(), "hello".into(), vec![]).unwrap();
    let with_files = svc
        .add_reply(g.id, "t".into(), "hello".into(), vec!["/tmp/f.txt".into()])
        .unwrap();

    use crate::core::clipboard::types::ClipContent;
    assert_eq!(
        text_only.to_clip_content(),
        ClipContent::Text("hello".to_string()),
        "无附件时应写入纯文本"
    );
    assert_eq!(
        with_files.to_clip_content(),
        ClipContent::Files(vec!["/tmp/f.txt".to_string()]),
        "有附件时应写入文件列表"
    );
}
