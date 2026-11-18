//! 配置迁移与默认值回归测试。

use super::*;
use crate::core::convert::preset::ConversionPreset;

fn list(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

/// 状态栏模块顺序：未知/重复 id 清掉。
#[test]
fn statusbar_normalize_filters_unknown_and_duplicates() {
    let mut sb = StatusBarConfig {
        sections: list(&["volume", "ghost", "battery", "volume", "network"]),
        ..StatusBarConfig::default()
    };
    sb.normalize();
    assert_eq!(sb.sections, list(&["volume", "battery", "network"]));
}

/// 旧配置没有 `statusbar` 字段时用默认值（图标开、七个已实现模块）。
#[test]
fn statusbar_defaults_when_field_missing() {
    let config: Config = serde_json::from_str("{}").expect("空配置可解析");
    assert!(config.statusbar.enabled);
    assert_eq!(
        config.statusbar.sections,
        list(&[
            "battery",
            "network",
            "vpn",
            "bluetooth",
            "volume",
            "audio_input",
            "input_source"
        ])
    );
    assert_eq!(config.statusbar.glyph_center, "auto");
    assert!(config.statusbar.glyph_battery_ring);
    assert!(config.statusbar.glyph_battery_percent);
    assert!(config.statusbar.glyph_volume);
    assert_eq!(config.statusbar.glyph_volume_style, "arc");
    assert!(config.statusbar.glyph_colored);
    assert_eq!(config.statusbar.glyph_appearance, "auto");
    // 专用标记才是正版的默认长相，四个退回开关默认全关。
    assert!(!config.statusbar.glyph_wifi_for_hotspot);
    assert!(!config.statusbar.glyph_wifi_for_ethernet);
    assert_eq!(
        config.statusbar.glyph_options().center,
        crate::core::statusbar::glyph::CenterMode::Auto
    );
}

/// 旧配置里只有 `glyph_volume_dots`：那个开关管的是「画不画音量区」，
/// 当时画法只有点一种，所以照样映射到新的 `glyph_volume`。
#[test]
fn statusbar_accepts_the_legacy_volume_dots_key() {
    let config: Config = serde_json::from_str(r#"{"statusbar":{"glyph_volume_dots":false}}"#)
        .expect("旧字段可解析");
    assert!(!config.statusbar.glyph_volume);
    assert!(!config.statusbar.glyph_options().volume);
}

#[test]
fn statusbar_normalize_fixes_unknown_glyph_center() {
    let mut config = StatusBarConfig {
        glyph_center: "vpn".to_string(),
        glyph_appearance: "midnight".to_string(),
        glyph_volume_style: "waves".to_string(),
        ..StatusBarConfig::default()
    };
    config.normalize();
    assert_eq!(config.glyph_center, "auto");
    assert_eq!(config.glyph_appearance, "auto");
    assert_eq!(config.glyph_volume_style, "arc");

    let mut config = StatusBarConfig {
        glyph_center: "bluetooth".to_string(),
        glyph_battery_ring: false,
        glyph_battery_percent: false,
        glyph_volume: false,
        glyph_volume_style: "dots".to_string(),
        glyph_wifi_for_hotspot: true,
        glyph_colored: false,
        glyph_appearance: "dark".to_string(),
        ..StatusBarConfig::default()
    };
    config.normalize();
    let options = config.glyph_options();
    assert_eq!(
        options.center,
        crate::core::statusbar::glyph::CenterMode::Bluetooth
    );
    assert!(!options.battery_ring);
    assert!(!options.battery_percent);
    assert!(!options.volume);
    assert_eq!(
        options.volume_style,
        crate::core::statusbar::glyph::VolumeStyle::Dots
    );
    assert!(options.wifi_for_hotspot);
    assert!(!options.wifi_for_sharing);
    assert!(!options.colored);
    assert_eq!(
        options.appearance,
        crate::core::statusbar::glyph::Appearance::Dark
    );
}

/// 出厂盘共同断言：图片 6 / 文本 2 / 文档 6 / 兜底，全部没有背面。
fn assert_stock_profiles(config: &Config) {
    let images = profile_by_id(config, "images").expect("图片 Profile");
    assert_eq!(
        images.preset_ids,
        preset::DEFAULT_IMAGES
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
    );
    assert!(images.back_ids.is_empty());
    assert!(images.preset_ids.len() <= MAX_PROFILE_SLICES);

    let text = profile_by_id(config, "text").expect("文本 Profile");
    assert_eq!(
        text.preset_ids,
        preset::DEFAULT_TEXT
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
    );
    assert!(text.back_ids.is_empty());

    let docs = profile_by_id(config, "docs").expect("文档 Profile");
    assert_eq!(
        docs.preset_ids,
        preset::DEFAULT_DOCS
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
    );
    assert_eq!(docs.extensions, docs_extensions());
    assert!(docs.back_ids.is_empty());

    let mixed = profile_by_id(config, "mixed").expect("混合兜底");
    assert_eq!(mixed.kind, RingProfileKind::MixedFallback);
    assert_eq!(mixed.preset_ids, preset::default_mixed_actions());
    assert!(mixed.back_ids.is_empty());
    // 兜底必须排在最后（匹配优先级）。
    assert_eq!(
        config.ring.profiles.last().map(|p| p.id.as_str()),
        Some("mixed")
    );
}

/// 旧版配置（格式名动作 + 全局质量/目录）能完整升级到 Profile 模型。
#[test]
fn migrate_maps_legacy_ids_quality_and_dir() {
    let mut config: Config = serde_json::from_value(serde_json::json!({
        "ring": {
            "actions": ["jpg", "png", "md"],
            "back_actions": ["gif"],
            "jpeg_quality": 62,
            "dest_dir": "/tmp/out",
            "last_action": [["图片", "jpg"]]
        }
    }))
    .expect("旧配置应能反序列化");
    assert!(config.conversion.presets.is_empty());

    config.migrate();

    // 旧背面取消：`gif` 追加到合并顺序末尾，并按类别拆进图片盘。
    // `md` 已从本版删除，解析不出预设 → 被丢弃，文本盘因此为空、不创建。
    let images = profile_by_id(&config, "images").expect("图片 Profile");
    assert_eq!(images.preset_ids, list(&["img.jpg", "img.png", "img.gif"]));
    assert!(images.back_ids.is_empty());
    assert!(profile_by_id(&config, "text").is_none());
    // 最后一个必须是 catch-all，否则没配过的类型没有盘可拖。
    let mixed = config.ring.profiles.last().expect("至少一个 Profile");
    assert_eq!(mixed.kind, RingProfileKind::MixedFallback);
    assert!(mixed.extensions.is_empty());
    assert_eq!(mixed.preset_ids.len(), 3);

    assert_eq!(
        config.ring.last_action,
        vec![("图片".to_string(), "img.jpg".to_string())]
    );
    assert!(config.ring.legacy_actions.is_none());
    assert!(config.ring.legacy_back_actions.is_none());
    assert!(config.ring.legacy_jpeg_quality.is_none());
    assert!(config.ring.legacy_dest_dir.is_none());

    let jpg = config
        .conversion
        .presets
        .iter()
        .find(|p| p.id == "img.jpg")
        .expect("img.jpg 应存在");
    assert_eq!(jpg.image.jpeg_quality, 62);
    assert_eq!(jpg.name, "JPG 62%");
    assert_eq!(jpg.output.dir, "/tmp/out");

    // 已删除格式的内置预设不再留在库里，避免 UI 出现选不了的扇区。
    assert!(!config.conversion.presets.iter().any(|p| p.id == "doc.md"));
}

/// 迁移可以重复跑：第二遍不再改变任何东西，旧字段也不会重新出现在 JSON 里。
#[test]
fn migrate_is_idempotent_and_drops_legacy_fields() {
    let mut config = Config::default();
    config.ring.legacy_actions = Some(vec!["jpg".to_string(), "preset-mine".to_string()]);
    config.ring.legacy_jpeg_quality = Some(90);
    config.ring.legacy_dest_dir = Some("  /tmp/once  ".to_string());
    config.conversion.presets.push(ConversionPreset {
        id: "preset-mine".to_string(),
        name: "我的配方".to_string(),
        target: preset::TargetSpec::Explicit(ConvertFormat::Webp),
        builtin: false,
        ..preset::builtin_presets()
            .into_iter()
            .next()
            .expect("内置预设不为空")
    });

    config.migrate();
    // 未知的自定义预设 id 必须保住；旧格式名换成预设 id；背面缺省时取上一版
    // 默认背面，拼在正面之后（Filee 行为：正面在前 + 背面追加去重）。已删除格式
    // 的背面项（avif / ico）被丢掉。
    let images = profile_by_id(&config, "images").expect("图片 Profile");
    assert_eq!(
        images.preset_ids,
        list(&[
            "img.jpg",
            "preset-mine",
            "img.webp",
            "img.gif",
            "img.bmp",
            "img.tiff",
            "img.heic",
        ])
    );
    assert!(images.back_ids.is_empty());
    assert_eq!(
        config
            .conversion
            .presets
            .iter()
            .find(|p| p.id == "preset-mine")
            .map(|p| p.name.as_str()),
        Some("我的配方")
    );

    let once = serde_json::to_value(&config).expect("序列化");
    config.migrate();
    let twice = serde_json::to_value(&config).expect("序列化");
    assert_eq!(once, twice, "迁移必须是幂等的");
    assert!(once["ring"].get("jpeg_quality").is_none());
    assert!(once["ring"].get("dest_dir").is_none());
    assert!(once["ring"].get("actions").is_none());
    assert!(once["ring"].get("back_actions").is_none());
}

/// 从未自订过的旧默认布局（格式名）会升级成全新出厂盘。
#[test]
fn migrate_upgrades_untouched_default_layout() {
    let mut config: Config = serde_json::from_value(serde_json::json!({
        "ring": {
            "actions": ["jpg", "png", "webp", "html", "txt"],
            "back_actions": ["gif", "bmp", "tiff", "md"]
        }
    }))
    .expect("旧配置应能反序列化");

    config.migrate();

    // 升级后直接是新出厂盘：新增格式、文档目标全部开箱可见。
    assert_stock_profiles(&config);
}

/// 上一版（预设 id）的默认布局同样升级成出厂盘。
#[test]
fn migrate_upgrades_untouched_preset_id_default() {
    let mut config: Config = serde_json::from_value(serde_json::json!({
        "ring": {
            "actions": ["img.jpg", "img.png", "img.webp", "doc.html", "doc.txt"],
            "back_actions": ["img.gif", "img.bmp", "img.tiff", "doc.md"]
        }
    }))
    .expect("默认布局应能反序列化");

    config.migrate();

    assert_stock_profiles(&config);
}

/// 全新配置（没有任何旧字段）直接得到出厂盘，而不是「自定义」降级盘。
#[test]
fn migrate_builds_stock_profiles_for_fresh_config() {
    let mut config = Config::default();
    config.migrate();
    assert_stock_profiles(&config);
}

/// 用户自己排过的顺序不能被「默认升级」冲掉。
#[test]
fn migrate_keeps_custom_order() {
    let mut config: Config = serde_json::from_value(serde_json::json!({
        "ring": { "actions": ["txt", "html"], "back_actions": [] }
    }))
    .expect("旧配置应能反序列化");

    config.migrate();

    let text = profile_by_id(&config, "text").expect("文本 Profile");
    assert_eq!(text.preset_ids, list(&["doc.txt", "doc.html"]));
    // 用户没配图片：不该凭空多出一个空的图片 Profile。
    assert!(profile_by_id(&config, "images").is_none());
    // 文档盘：有文档目标动作就建（PDF → TXT / HTML）。
    let docs = profile_by_id(&config, "docs").expect("文档 Profile");
    assert_eq!(docs.preset_ids, list(&["doc.txt", "doc.html"]));
}

/// 指向已删预设的引用、重复 id、非法扩展名都在迁移时清掉；背面并入正面。
#[test]
fn migrate_repairs_dangling_profile_references() {
    let mut config = Config::default();
    config.ring.profiles = vec![
        RingProfile {
            id: "custom".to_string(),
            name: "  ".to_string(),
            extensions: vec!["*.PNG".to_string(), "png".to_string(), "a/b".to_string()],
            kind: RingProfileKind::Normal,
            preset_ids: vec![
                "img.jpg".to_string(),
                "preset-gone".to_string(),
                "img.jpg".to_string(),
            ],
            back_ids: vec!["preset-gone".to_string(), "img.jpg".to_string()],
        },
        RingProfile {
            id: "custom".to_string(),
            name: "重复 id".to_string(),
            extensions: Vec::new(),
            kind: RingProfileKind::MixedFallback,
            preset_ids: vec!["doc.md".to_string()],
            back_ids: Vec::new(),
        },
    ];

    config.migrate();

    assert_eq!(config.ring.profiles.len(), 2, "重复 id 的那条被丢弃");
    let first = &config.ring.profiles[0];
    assert_eq!(first.name, "custom", "空名字回落到 id");
    assert_eq!(first.extensions, vec!["png".to_string()]);
    // 背面已取消：`img.jpg` 已在正面，不重复出现；悬空引用清掉。
    assert_eq!(first.preset_ids, vec!["img.jpg".to_string()]);
    assert_eq!(first.back_ids, Vec::<String>::new());
    // 原有的 MixedFallback 被同 id 丢掉 → 自动补一个 catch-all。
    let mixed = config.ring.profiles.last().expect("兜底 Profile");
    assert_eq!(mixed.kind, RingProfileKind::MixedFallback);
    assert!(mixed.extensions.is_empty());
    assert!(!mixed.preset_ids.is_empty());
}

/// 用户定制过的旧布局：正面保持合并顺序，不升级、不臆测内容。
#[test]
fn migrate_leaves_custom_legacy_layout() {
    let mut config: Config = serde_json::from_value(serde_json::json!({
        "ring": {
            "actions": ["img.jpg", "img.gif", "doc.html"],
            "back_actions": ["img.png", "doc.md"]
        }
    }))
    .expect("定制布局应能反序列化");

    config.migrate();

    let images = profile_by_id(&config, "images").expect("图片 Profile");
    assert_eq!(images.preset_ids, list(&["img.jpg", "img.gif", "img.png"]));
    assert!(images.back_ids.is_empty(), "定制布局不臆测背面");
    let text = profile_by_id(&config, "text").expect("文本 Profile");
    assert_eq!(text.preset_ids, list(&["doc.html"]));
    // 文档盘收文档目标（含 PDF → 图片的 img.jpg / img.png，顺序跟随合并表）。
    let docs = profile_by_id(&config, "docs").expect("文档 Profile");
    assert_eq!(docs.preset_ids, list(&["img.jpg", "doc.html", "img.png"]));
}

/// 上一版拆成两面的默认盘：没动过的盘升级到新正面默认，动过的盘保持原样；
/// 恰好是出厂三盘时补上「文档」盘。迁移幂等。
#[test]
fn migrate_upgrades_untouched_profiles_and_adds_docs() {
    let mut config: Config = serde_json::from_value(serde_json::json!({
        "ring": {
            "profiles": [
                {
                    "id": "images",
                    "name": "图片",
                    "extensions": ["png", "jpg", "jpeg", "webp", "gif", "bmp", "tiff"],
                    "kind": "normal",
                    "preset_ids": ["img.jpg", "img.png"],
                    "back_ids": ["img.webp", "img.gif", "img.bmp", "img.tiff", "img.heic", "img.avif", "img.ico"]
                },
                {
                    "id": "text",
                    "name": "文本",
                    "extensions": ["md", "html", "txt"],
                    "kind": "normal",
                    "preset_ids": ["doc.html", "doc.txt"]
                }
            ]
        }
    }))
    .expect("上一版两面配置应能反序列化");

    config.migrate();

    // 图片盘没动过 → 升级到 14 项新默认（含新增格式与 img.pdf）。
    let images = profile_by_id(&config, "images").expect("图片 Profile");
    assert_eq!(
        images.preset_ids,
        preset::DEFAULT_IMAGES
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
    );
    assert!(images.back_ids.is_empty());

    // 用户动过的文本盘（删过 doc.md）：保持原样，不补不升级。
    let text = profile_by_id(&config, "text").expect("文本 Profile");
    assert_eq!(text.preset_ids, list(&["doc.html", "doc.txt"]));
    assert!(text.back_ids.is_empty());

    // 三张出厂盘齐全 → 补「文档」盘，且排在兜底之前。
    let docs = profile_by_id(&config, "docs").expect("文档 Profile");
    assert_eq!(
        docs.preset_ids,
        preset::DEFAULT_DOCS
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        config.ring.profiles.last().map(|p| p.id.as_str()),
        Some("mixed")
    );

    // 幂等：再跑一次不重复升级、不重复补盘。
    let once = serde_json::to_value(&config).expect("序列化");
    config.migrate();
    let twice = serde_json::to_value(&config).expect("序列化");
    assert_eq!(once, twice);
    assert_eq!(
        profile_by_id(&config, "docs")
            .expect("文档 Profile")
            .preset_ids
            .len(),
        preset::DEFAULT_DOCS.len()
    );
}

/// 用户动过的两面布局（删过背面一项）：背面并入正面，不自动补回新增格式。
#[test]
fn migrate_merges_custom_split_without_upgrade() {
    let mut config: Config = serde_json::from_value(serde_json::json!({
        "ring": {
            "profiles": [
                {
                    "id": "images",
                    "name": "图片",
                    "extensions": ["png", "jpg", "jpeg"],
                    "kind": "normal",
                    "preset_ids": ["img.jpg", "img.png"],
                    "back_ids": ["img.webp", "img.gif", "img.bmp"]
                }
            ]
        }
    }))
    .expect("定制两面配置应能反序列化");

    config.migrate();

    let images = profile_by_id(&config, "images").expect("图片 Profile");
    assert_eq!(
        images.preset_ids,
        list(&["img.jpg", "img.png", "img.webp", "img.gif", "img.bmp"])
    );
    assert!(images.back_ids.is_empty());
    // 只有一张图片盘（不是出厂三盘）→ 不补文档盘。
    assert!(profile_by_id(&config, "docs").is_none());
}

fn profile_by_id<'a>(config: &'a Config, id: &str) -> Option<&'a RingProfile> {
    config.ring.profiles.iter().find(|p| p.id == id)
}

/// 版本升级：用户库里已删掉的旧内置预设不复活；本版删掉的格式预设会被
/// 清掉；缺失的工具动作补回来；自建预设原样保留。迁移幂等。
#[test]
fn migrate_prunes_removed_builtins_and_keeps_user_presets() {
    let mut config = Config::default();
    let mut presets = preset::builtin_presets();
    // 旧库里指向已删格式的内置预设（配置反序列化前会被 JSON 层剪掉，
    // 这里直接构造出等价状态验证迁移的兜底清理）。
    let mut stale = presets
        .iter()
        .find(|p| p.id == "img.png")
        .expect("img.png")
        .clone();
    stale.id = "doc.md".to_string();
    stale.name = "Markdown".to_string();
    presets.push(stale);
    // 旧工具动作缺失 → 版本升级时补回。
    presets.retain(|p| p.id != "act.extract");
    // 用户删过的旧内置（doc.pdf）不复活。
    presets.retain(|p| p.id != "doc.pdf");
    presets.push(ConversionPreset {
        id: "preset-mine".to_string(),
        name: "我的配方".to_string(),
        target: preset::TargetSpec::Explicit(ConvertFormat::Webp),
        builtin: false,
        ..preset::builtin_presets()
            .into_iter()
            .next()
            .expect("内置预设不为空")
    });
    config.conversion.presets = presets;
    // 自定义盘只引用 img.jpg：不能让出厂盘的 doc.pdf 引用把「用户删过的
    // 预设」补回来（`normalize_profiles` 会 ensure 盘上引用的预设）。
    config.ring.profiles = vec![RingProfile {
        id: "images".to_string(),
        name: "图片".to_string(),
        extensions: vec!["png".to_string()],
        kind: RingProfileKind::Normal,
        preset_ids: vec!["img.jpg".to_string()],
        back_ids: Vec::new(),
    }];

    config.migrate();

    let ids: Vec<&str> = config
        .conversion
        .presets
        .iter()
        .map(|p| p.id.as_str())
        .collect();
    assert!(!ids.contains(&"doc.md"), "已删除格式的内置预设必须清掉");
    assert!(!ids.contains(&"img.jpg.q70"), "示范预设已从源码删除");
    assert!(ids.contains(&"preset-mine"), "用户自建预设不能被连带删掉");
    assert!(ids.contains(&"act.extract"), "缺失的新工具动作应补回");
    assert!(!ids.contains(&"doc.pdf"), "用户删过的旧内置不复活");

    // 幂等：再跑一次，数量不变。
    let count = config.conversion.presets.len();
    config.migrate();
    assert_eq!(config.conversion.presets.len(), count);
}
