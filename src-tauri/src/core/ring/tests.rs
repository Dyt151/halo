use super::plan::{build_actions, build_snapshot};
use super::*;
use super::*;
fn ids(actions: &[RingAction]) -> Vec<&str> {
    actions.iter().map(|a| a.id.as_str()).collect()
}

fn enabled_ids(actions: &[RingAction]) -> Vec<&str> {
    actions
        .iter()
        .filter(|a| a.enabled)
        .map(|a| a.id.as_str())
        .collect()
}

fn ready_config() -> Config {
    let mut config = Config::default();
    config.migrate();
    config
}
/// 旧安装版的混杂布局（图片 + 文本目标同盘）。
fn old_mixed_layout() -> Vec<String> {
    vec!["img.jpg", "img.png", "doc.html", "doc.txt"]
        .into_iter()
        .map(str::to_string)
        .collect()
}
/// 位置固定：选 .txt 时图片预设仍然占着原位置，只是禁用并带原因。
#[test]
fn txt_keeps_image_slices_but_disables_them() {
    let presets = preset::builtin_presets();
    let actions = build_actions(
        &presets,
        &[Some(ConvertFormat::Txt)],
        &[],
        &old_mixed_layout(),
    );
    assert_eq!(
        ids(&actions),
        vec!["img.jpg", "img.png", "doc.html", "doc.txt"]
    );
    assert_eq!(enabled_ids(&actions), vec!["doc.html"]);
    for action in actions.iter().filter(|a| !a.enabled) {
        assert!(
            action.disabled_reason.is_some(),
            "{} 缺少禁用原因",
            action.id
        );
    }
    assert_eq!(actions[2].target.as_deref(), Some("html"));
}
/// 已经是目标格式的同格式预设：禁用但仍在原位置。
#[test]
fn png_disables_identity_preset_in_place() {
    let presets = preset::builtin_presets();
    let actions = build_actions(
        &presets,
        &[Some(ConvertFormat::Png)],
        &[],
        &old_mixed_layout(),
    );
    assert_eq!(ids(&actions)[1], "img.png");
    assert!(!actions[1].enabled, "同格式且未声明重编码，应禁用");
    assert!(actions[1].disabled_reason.is_some());
    assert!(actions[0].enabled, "PNG → JPG 应可用");
}
/// 混合选择时：有路由的文件计入 convert，不认识的文件计入 skip。
#[test]
fn mixed_selection_counts_skips() {
    let presets = preset::builtin_presets();
    let per_file = [Some(ConvertFormat::Png), None];
    let actions = build_actions(&presets, &per_file, &[], &["img.jpg".to_string()]);
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].convert, 1);
    assert_eq!(actions[0].skip, 1);
    assert!(actions[0].enabled);
}
/// 预设被删掉时扇区就消失，而不是留一个点了没反应的坑。
#[test]
fn missing_preset_drops_its_slice() {
    let presets = preset::builtin_presets();
    let actions = build_actions(
        &presets,
        &[Some(ConvertFormat::Png)],
        &[],
        &["preset-gone".into()],
    );
    assert!(actions.is_empty());
}
/// 一批 .png → 选到「图片」Profile，只下发对 PNG 可用的扇区。
#[test]
fn snapshot_picks_image_profile_for_png() {
    let mut config = ready_config();
    config.ring.utility_actions = false;
    let snapshot = build_snapshot(1, &State::Ready, &["/tmp/a.png".to_string()], &config);
    assert_eq!(
        snapshot.profile.as_ref().map(|p| p.id.as_str()),
        Some("images")
    );
    assert_eq!(snapshot.category.as_deref(), Some("PNG"));
    assert!(snapshot.actions.iter().all(|a| a.enabled));
    let enabled = ids(&snapshot.actions);
    assert!(enabled.contains(&"img.jpg"));
    assert!(!enabled.contains(&"img.png"), "同格式禁用的扇区不应下发");
}
/// 文档格式走「文档」Profile（docx 不再落到 catch-all）。
#[test]
fn snapshot_picks_docs_profile_for_document_format() {
    let mut config = ready_config();
    config.ring.utility_actions = false;
    let snapshot = build_snapshot(1, &State::Ready, &["/tmp/a.docx".to_string()], &config);
    assert_eq!(
        snapshot.profile.as_ref().map(|p| p.id.as_str()),
        Some("docs")
    );
    assert_eq!(snapshot.category.as_deref(), Some("DOCX"));
    assert!(snapshot.actions.iter().all(|a| a.enabled));
    assert!(ids(&snapshot.actions).contains(&"doc.txt"));
}
/// 图片 + 文本混选 → 没有普通 Profile 能覆盖全部扩展名，落到 catch-all。
#[test]
fn snapshot_picks_catch_all_for_cross_family_batch() {
    let config = ready_config();
    let paths = vec!["/tmp/a.png".to_string(), "/tmp/b.txt".to_string()];
    let snapshot = build_snapshot(1, &State::Ready, &paths, &config);
    assert_eq!(
        snapshot.profile.as_ref().map(|p| p.id.as_str()),
        Some("mixed")
    );
    assert!(snapshot
        .profile
        .as_ref()
        .map(|p| p.fallback)
        .unwrap_or(false));
    assert_eq!(snapshot.count, 2);
    // 混合盘同时含图片与文本目标，但只下发两边都能用上的那些。
    assert!(snapshot.actions.iter().all(|a| a.enabled));
    assert!(ids(&snapshot.actions).contains(&"doc.html"));
}
/// 未知扩展名 → catch-all，不误入任何普通 Profile。
#[test]
fn snapshot_falls_back_for_unknown_extension() {
    let config = ready_config();
    let snapshot = build_snapshot(1, &State::Ready, &["/tmp/a.xyz".to_string()], &config);
    assert_eq!(
        snapshot.profile.as_ref().map(|p| p.id.as_str()),
        Some("mixed")
    );
    assert_eq!(snapshot.count, 0);
    assert!(snapshot.hint.is_some());
    assert!(snapshot.actions.iter().all(|a| a.enabled));
}
/// 工具动作固定追加在正面末尾；关闭开关后整组消失。
#[test]
fn utility_actions_appended_last_and_toggleable() {
    let config = ready_config();
    let snapshot = build_snapshot(1, &State::Ready, &["/tmp/a.png".to_string()], &config);
    // PNG 能打包；解压 / PDF 分页对 PNG 不可用，不下发。
    let zip = snapshot
        .actions
        .iter()
        .find(|a| a.id == "act.pack.zip")
        .expect("打包扇区");
    assert!(zip.enabled);
    assert!(snapshot.actions.iter().all(|a| a.enabled));
    assert!(snapshot
        .actions
        .iter()
        .all(|a| !a.id.starts_with("act.pdf")));
    assert!(snapshot.actions.iter().all(|a| a.id != "act.extract"));
    // 工具动作仍连续排在末尾。
    let utility = snapshot
        .actions
        .iter()
        .filter(|a| a.id.starts_with("act."))
        .count();
    assert!(
        snapshot
            .actions
            .iter()
            .rev()
            .take(utility)
            .all(|a| a.id.starts_with("act.")),
        "工具动作必须排在正面末尾"
    );
    let mut off = ready_config();
    off.ring.utility_actions = false;
    let plain = build_snapshot(1, &State::Ready, &["/tmp/a.png".to_string()], &off);
    assert!(plain.actions.iter().all(|a| !a.id.starts_with("act.")));
}
/// 纯文件夹批次：落兜底盘，转换预设全部不可用被滤掉，只剩打包动作。
#[test]
fn snapshot_for_folder_only_shows_pack_tools() {
    let config = ready_config();
    let dir = std::env::temp_dir().join(format!("halo-ring-dir-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("创建临时文件夹");
    let paths = vec![dir.to_string_lossy().to_string()];
    let snapshot = build_snapshot(5, &State::Ready, &paths, &config);
    assert_eq!(
        snapshot.profile.as_ref().map(|p| p.id.as_str()),
        Some("mixed")
    );
    assert_eq!(snapshot.category.as_deref(), Some("文件夹"));
    assert_eq!(snapshot.count, 1);
    assert!(
        snapshot.hint.is_none(),
        "文件夹批次不该提示「不是支持的格式」"
    );
    assert!(snapshot.actions.iter().all(|a| a.enabled));
    assert!(
        snapshot
            .actions
            .iter()
            .all(|a| a.id.starts_with("act.pack")),
        "{:?}",
        ids(&snapshot.actions)
    );
    assert!(ids(&snapshot.actions).contains(&"act.pack.zip"));
}
