//! 文件类型 Profile 的扩展名规范化与选择规则（Filee 行为 / Filee 行为）。
//!
//! 这一层全是纯函数：输入「本批文件用到的扩展名集合」与 Profile 列表，输出唯一
//! 一个 Profile，因此设置页的遮蔽提示与运行态的选盘判断可以共用同一份逻辑，
//! 不会再各写一套匹配规则。

use std::path::Path;

use crate::config::settings::{RingProfile, RingProfileKind};
use crate::core::convert::format::ConvertFormat;

/// 单个扩展名的最大长度（对齐 Filee `ExtensionInput`）。
pub const MAX_EXTENSION_LEN: usize = 16;

/// 把一个用户输入 / 文件名里的扩展名规范成可比较的形式。
///
/// 规则（Filee 行为）：trim、小写、去前导 `*` 与 `.`、去末尾 `.`；拒绝空串、
/// 非法字符、连续 `..`、超长。复合扩展名（`tar.gz`）只接受注册表已知的写法，
/// 否则普通文件名只按最后一段匹配 —— 这里直接拒绝，避免把一个永远匹配不到的
/// 字符串存进配置。
pub fn normalize_extension(raw: &str) -> Option<String> {
    let trimmed = raw.trim().to_ascii_lowercase();
    // 连续的点在 strip 之前判断：`..png` 不能因为被当成「两个前导点」而放行。
    if trimmed.contains("..") {
        return None;
    }
    let mut text = trimmed.as_str();
    // `*.PNG` / `.png` 是常见手写法。
    while let Some(rest) = text.strip_prefix('*') {
        text = rest.trim_start();
    }
    while let Some(rest) = text.strip_prefix('.') {
        text = rest;
    }
    let text = text.trim_end_matches('.');
    if text.is_empty() || text.len() > MAX_EXTENSION_LEN {
        return None;
    }
    // 只接受字母数字与 `._-`：路径分隔符、空白、控制字符都不能进配置。
    if !text
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    {
        return None;
    }
    if text.contains('.') && ConvertFormat::from_ext(text).is_none() {
        return None;
    }
    Some(text.to_string())
}

/// 规范化一组扩展名，按出现顺序去重（同一 Profile 内不允许重复）。
pub fn normalize_extensions(raw: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(raw.len());
    for item in raw {
        let Some(ext) = normalize_extension(item) else {
            continue;
        };
        if !out.contains(&ext) {
            out.push(ext);
        }
    }
    out
}

/// 一批路径用到的、**转换器认识**的小写扩展名集合（去重、保序）。
///
/// 为什么要筛掉不认识的尾巴：`png + 备份.tar.gz` 这种批次如果带上 `gz`，
/// 图片盘就「没覆盖全部扩展名」，整批会掉到混合兜底盘——明明只有一张图
/// 要转。认不出来的文件本来就会被每个扇区跳过，不该参与选盘。
pub fn extensions_of(paths: &[String]) -> Vec<String> {
    let mut raw: Vec<String> = Vec::with_capacity(paths.len());
    for path in paths {
        let Some(ext) = Path::new(path).extension().and_then(|e| e.to_str()) else {
            continue;
        };
        // 扩展名大小写不敏感：`A.PNG` 与 `a.png` 必须命中同一张盘。
        if ConvertFormat::from_ext(ext).is_some() {
            raw.push(ext.to_string());
        }
    }
    normalize_extensions(&raw)
}

/// 按Filee 行为 的五条优先级选出一个 Profile。
///
/// 1. 第一个「包含本批全部扩展名」的普通 Profile；
/// 2. 第一个「扩展名非空且包含全部扩展名」的 MixedFallback；
/// 3. 第一个扩展名为空的 MixedFallback（catch-all）；
/// 4. 第一个 MixedFallback；
/// 5. 第一个 Profile。
///
/// 普通 Profile 优先于 MixedFallback，因此同族混选（png + jpg）也会落在图片盘；
/// 列表顺序解决竞争，且至少会有一个 MixedFallback（迁移时保证）。
pub fn select_profile<'a>(
    profiles: &'a [RingProfile],
    extensions: &[String],
) -> Option<&'a RingProfile> {
    let wanted = normalize_extensions(extensions);
    let first_mixed = || {
        profiles
            .iter()
            .find(|p| p.kind == RingProfileKind::MixedFallback)
    };
    if wanted.is_empty() {
        // 一个扩展名都识别不出来时「包含全部」恒真，先走普通 Profile 会随机命中
        // 一个有扩展名限制的盘；这里直接按兜底顺序来。
        return profiles
            .iter()
            .find(|p| p.kind == RingProfileKind::MixedFallback && p.extensions.is_empty())
            .or_else(first_mixed)
            .or_else(|| profiles.first());
    }
    let covers = |p: &RingProfile| -> bool {
        wanted
            .iter()
            .all(|ext| p.extensions.iter().any(|e| e.eq_ignore_ascii_case(ext)))
    };
    profiles
        .iter()
        .find(|p| p.kind == RingProfileKind::Normal && covers(p))
        .or_else(|| {
            profiles.iter().find(|p| {
                p.kind == RingProfileKind::MixedFallback && !p.extensions.is_empty() && covers(p)
            })
        })
        .or_else(|| {
            profiles
                .iter()
                .find(|p| p.kind == RingProfileKind::MixedFallback && p.extensions.is_empty())
        })
        .or_else(first_mixed)
        .or_else(|| profiles.first())
}

/// 设置页用：某个 Profile 的扩展名是否被列表中更靠前的 Profile 完全遮蔽。
///
/// 只提示、不阻止保存 —— 竞争由列表顺序决定，用户可能是有意排在前面的那个。
pub fn shadowed_by(profiles: &[RingProfile], index: usize) -> Option<&RingProfile> {
    let profile = profiles.get(index)?;
    if profile.kind != RingProfileKind::Normal || profile.extensions.is_empty() {
        return None;
    }
    profiles.iter().take(index).find(|p| {
        p.kind == RingProfileKind::Normal
            && profile
                .extensions
                .iter()
                .all(|ext| p.extensions.iter().any(|e| e.eq_ignore_ascii_case(ext)))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(id: &str, kind: RingProfileKind, exts: &[&str]) -> RingProfile {
        RingProfile {
            id: id.to_string(),
            name: id.to_string(),
            extensions: exts.iter().map(|e| e.to_string()).collect(),
            kind,
            preset_ids: Vec::new(),
            back_ids: Vec::new(),
        }
    }

    #[test]
    fn normalizes_common_hand_written_forms() {
        assert_eq!(normalize_extension(" *.PNG ").as_deref(), Some("png"));
        assert_eq!(normalize_extension(".jpeg").as_deref(), Some("jpeg"));
        assert_eq!(normalize_extension("JPG.").as_deref(), Some("jpg"));
        assert_eq!(normalize_extension("tif").as_deref(), Some("tif"));
    }

    #[test]
    fn rejects_empty_illegal_and_unknown_compound() {
        assert_eq!(normalize_extension(""), None);
        assert_eq!(normalize_extension("   "), None);
        assert_eq!(normalize_extension("..png"), None);
        assert_eq!(normalize_extension("png/../x"), None);
        assert_eq!(normalize_extension("a b"), None);
        // 未知的复合扩展名：文件名只会按最后一段匹配，存下来永远命中不了。
        assert_eq!(normalize_extension("tar.gz"), None);
        assert_eq!(normalize_extension(&"x".repeat(17)), None);
    }

    #[test]
    fn normalize_extensions_dedupes_in_order() {
        let raw = vec![
            "PNG".to_string(),
            " png".to_string(),
            "!!".to_string(),
            "jpeg".to_string(),
        ];
        assert_eq!(normalize_extensions(&raw), vec!["png", "jpeg"]);
    }

    #[test]
    fn extensions_of_keeps_only_convertible_last_segment() {
        let paths = vec![
            "/tmp/a.PNG".to_string(),
            "/tmp/b.png".to_string(),
            // 没有扩展名：不参与选盘。
            "/tmp/c".to_string(),
            // 只看最后一段，而且 `.gz` 转换器不认识 → 丢掉，不能拖累整批选盘。
            "/tmp/d.tar.gz".to_string(),
            "/tmp/e.jpg".to_string(),
        ];
        assert_eq!(extensions_of(&paths), vec!["png", "jpg"]);
    }

    #[test]
    fn normal_profile_beats_mixed_fallback() {
        let profiles = vec![
            profile("text", RingProfileKind::Normal, &["md", "txt"]),
            profile("mixed", RingProfileKind::MixedFallback, &[]),
            profile("images", RingProfileKind::Normal, &["png", "jpg"]),
        ];
        let picked = select_profile(&profiles, &["png".to_string(), "jpg".to_string()]);
        assert_eq!(picked.map(|p| p.id.as_str()), Some("images"));
    }

    #[test]
    fn family_profile_handles_mixed_selection_of_same_family() {
        let profiles = vec![
            profile("images", RingProfileKind::Normal, &["png", "jpg", "webp"]),
            profile("mixed", RingProfileKind::MixedFallback, &[]),
        ];
        let picked = select_profile(&profiles, &["png".to_string(), "webp".to_string()]);
        assert_eq!(picked.map(|p| p.id.as_str()), Some("images"));
    }

    #[test]
    fn cross_family_selection_falls_to_catch_all() {
        let profiles = vec![
            profile("images", RingProfileKind::Normal, &["png"]),
            profile("text", RingProfileKind::Normal, &["md"]),
            profile("mixed", RingProfileKind::MixedFallback, &[]),
        ];
        let picked = select_profile(&profiles, &["png".to_string(), "md".to_string()]);
        assert_eq!(picked.map(|p| p.id.as_str()), Some("mixed"));
    }

    #[test]
    fn limited_mixed_profile_wins_before_catch_all() {
        let profiles = vec![
            profile("text", RingProfileKind::Normal, &["md"]),
            profile("png-jpg", RingProfileKind::MixedFallback, &["png", "jpg"]),
            profile("mixed", RingProfileKind::MixedFallback, &[]),
        ];
        let picked = select_profile(&profiles, &["png".to_string(), "jpg".to_string()]);
        assert_eq!(picked.map(|p| p.id.as_str()), Some("png-jpg"));
    }

    #[test]
    fn first_profile_wins_when_list_order_competes() {
        let profiles = vec![
            profile("a", RingProfileKind::Normal, &["png"]),
            profile("b", RingProfileKind::Normal, &["png"]),
        ];
        let picked = select_profile(&profiles, &["png".to_string()]);
        assert_eq!(picked.map(|p| p.id.as_str()), Some("a"));
        assert_eq!(
            shadowed_by(&profiles, 1).map(|p| p.id.as_str()),
            Some("a"),
            "第二个同扩展名 Profile 必须提示被遮蔽"
        );
    }

    #[test]
    fn unknown_extension_falls_back_instead_of_matching_a_normal_profile() {
        let profiles = vec![
            profile("images", RingProfileKind::Normal, &["png"]),
            profile("mixed", RingProfileKind::MixedFallback, &[]),
        ];
        // 空集合：不能命中 images（否则没扩展名的文件会用图片盘）。
        assert_eq!(
            select_profile(&profiles, &[]).map(|p| p.id.as_str()),
            Some("mixed")
        );
        // 未配置的单扩展名：普通 Profile 覆盖不了 → catch-all；它不会被当成「空集合」。
        assert_eq!(
            select_profile(&profiles, &["docx".to_string()]).map(|p| p.id.as_str()),
            Some("mixed")
        );
    }

    #[test]
    fn empty_profile_list_selects_nothing() {
        assert!(select_profile(&[], &["png".to_string()]).is_none());
    }
}
