//! 环形转换配置、转换预设库与旧配置迁移规则。

use serde::{Deserialize, Serialize};

use crate::core::convert::preset::{self, ConversionPreset};
use crate::core::ring::profile::normalize_extensions;

use super::default_true;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RingProfile {
    /// 稳定 id（默认 `images` / `text` / `mixed`，自建为 `profile-*`）。
    pub id: String,
    /// 展示名（设置页可改）。
    #[serde(default)]
    pub name: String,
    /// 归属本 Profile 的扩展名：小写、不带点、不带 `*`。
    ///
    /// 普通 Profile 为空时**不匹配**任何非空批次；混合兜底为空时表示 catch-all。
    #[serde(default)]
    pub extensions: Vec<String>,
    /// 普通 / 混合兜底（决定匹配优先级，见 README「三、环形文件转换」）。
    #[serde(default)]
    pub kind: RingProfileKind,
    /// 正面预设顺序：索引 0 在 12 点，顺时针。
    #[serde(default)]
    pub preset_ids: Vec<String>,
    /// 【已废弃】旧版背面预设顺序：背面已取消，`Config::migrate` 会并入
    /// `preset_ids` 并清空，此后恒为空。保留字段只是为了让旧配置能反序列化。
    #[serde(default)]
    pub back_ids: Vec<String>,
}

/// Profile 类型：决定选择顺序里的优先级。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RingProfileKind {
    /// 普通 Profile：扩展名必须覆盖本批全部文件的类型。
    #[default]
    Normal,
    /// 混合兜底：扩展名为空时是 catch-all。
    MixedFallback,
}

/// 每个 Profile 最多几个扇区（Filee `ToolbarProfile.MaxSlices`；不含运行时
/// 追加在末尾的工具动作，见 [`RingConfig::utility_actions`]）。
pub const MAX_PROFILE_SLICES: usize = 16;

impl RingProfile {
    /// 出厂 Profile：图片 / 文本 / 文档 / 混合兜底，全部只有正面（背面已取消）。
    ///
    /// `merged` 是旧版正背面合并后的顺序（迁移）或默认顺序；每张盘按自己的
    /// 类别表（内置 id 表 + 自建预设按目标格式归类）从 `merged` 里取交集，
    /// 顺序仍然跟随 `merged`，用户原来的偏好不会被打乱。
    pub(super) fn seeds(merged: &[String], presets: &[ConversionPreset]) -> Vec<RingProfile> {
        let pick = |table: &[&str], is_kind: fn(&ConversionPreset) -> bool| -> Vec<String> {
            merged
                .iter()
                .filter(|id| {
                    table.contains(&id.as_str())
                        || presets
                            .iter()
                            .find(|p| p.id == **id)
                            .map(is_kind)
                            .unwrap_or(false)
                })
                .cloned()
                .take(MAX_PROFILE_SLICES)
                .collect()
        };
        let mut out = Vec::new();
        let images = pick(&preset::DEFAULT_IMAGES, target_is_image);
        let texts = pick(&preset::DEFAULT_TEXT, target_is_text);
        let docs = pick(&preset::DEFAULT_DOCS, target_is_document);
        if !images.is_empty() {
            out.push(RingProfile {
                id: "images".to_string(),
                name: "图片".to_string(),
                extensions: image_extensions(),
                kind: RingProfileKind::Normal,
                preset_ids: images,
                back_ids: Vec::new(),
            });
        }
        if !texts.is_empty() {
            out.push(RingProfile {
                id: "text".to_string(),
                name: "文本".to_string(),
                extensions: text_extensions(),
                kind: RingProfileKind::Normal,
                preset_ids: texts,
                back_ids: Vec::new(),
            });
        }
        if !docs.is_empty() {
            out.push(RingProfile {
                id: "docs".to_string(),
                name: "文档".to_string(),
                extensions: docs_extensions(),
                kind: RingProfileKind::Normal,
                preset_ids: docs,
                back_ids: Vec::new(),
            });
        }
        // 至少留一个 catch-all：没配过的类型也要有盘可拖（Filee 行为）。
        out.push(RingProfile {
            id: "mixed".to_string(),
            name: "其他文件".to_string(),
            extensions: Vec::new(),
            kind: RingProfileKind::MixedFallback,
            preset_ids: merged.iter().take(MAX_PROFILE_SLICES).cloned().collect(),
            back_ids: Vec::new(),
        });
        out
    }

    /// 全新出厂盘：内容按类别表原样（不经过 `merged` 顺序）。
    pub(super) fn stock() -> Vec<RingProfile> {
        let mut out = Vec::new();
        for (id, name, exts) in [
            ("images", "图片", image_extensions()),
            ("text", "文本", text_extensions()),
            ("docs", "文档", docs_extensions()),
        ] {
            out.push(RingProfile {
                id: id.to_string(),
                name: name.to_string(),
                extensions: exts,
                kind: RingProfileKind::Normal,
                preset_ids: stock_content(id),
                back_ids: Vec::new(),
            });
        }
        out.push(RingProfile {
            id: "mixed".to_string(),
            name: "其他文件".to_string(),
            extensions: Vec::new(),
            kind: RingProfileKind::MixedFallback,
            preset_ids: stock_content("mixed"),
            back_ids: Vec::new(),
        });
        out
    }
}

/// 出厂盘正面内容（id → 预设列表）。
pub(super) fn stock_content(id: &str) -> Vec<String> {
    match id {
        "images" => preset::DEFAULT_IMAGES
            .iter()
            .map(|s| s.to_string())
            .collect(),
        "text" => preset::DEFAULT_TEXT.iter().map(|s| s.to_string()).collect(),
        "docs" => preset::DEFAULT_DOCS.iter().map(|s| s.to_string()).collect(),
        "mixed" => preset::default_mixed_actions(),
        _ => Vec::new(),
    }
}

/// 预设目标是否图片（自建预设按目标归类进图片盘；动作类预设不算）。
fn target_is_image(p: &ConversionPreset) -> bool {
    if !p.kind.is_convert() {
        return false;
    }
    match &p.target {
        preset::TargetSpec::Explicit(format) => format.is_image(),
        // SameAsSource 只会出现在图片（重编码）上，按图片归类。
        preset::TargetSpec::SameAsSource => true,
    }
}

/// 预设目标是否文本（html / txt）。
fn target_is_text(p: &ConversionPreset) -> bool {
    if !p.kind.is_convert() {
        return false;
    }
    matches!(&p.target, preset::TargetSpec::Explicit(f) if f.is_text())
}

/// 预设目标是否文档（pdf / docx）。
fn target_is_document(p: &ConversionPreset) -> bool {
    if !p.kind.is_convert() {
        return false;
    }
    matches!(&p.target, preset::TargetSpec::Explicit(f) if f.is_document())
}

/// 背面并入正面（背面已取消）：去重、截断到扇区上限、清空背面。
pub(super) fn merge_back_into_front(profile: &mut RingProfile) {
    let mut front = std::mem::take(&mut profile.preset_ids);
    for id in std::mem::take(&mut profile.back_ids) {
        if !front.contains(&id) {
            front.push(id);
        }
    }
    front.truncate(MAX_PROFILE_SLICES);
    profile.preset_ids = front;
}

/// 图片族的全部扩展名（含别名，与 `ConvertFormat::from_ext` 同一张表）。
fn image_extensions() -> Vec<String> {
    [
        "png", "jpg", "jpeg", "jpe", "webp", "gif", "bmp", "tif", "tiff", "heic", "heif", "hif",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

/// 文本族的全部扩展名。
fn text_extensions() -> Vec<String> {
    ["html", "htm", "xhtml", "txt", "text"]
        .into_iter()
        .map(str::to_string)
        .collect()
}

/// 文档族的全部扩展名（PDF / Word）。
pub(super) fn docs_extensions() -> Vec<String> {
    ["pdf", "docx", "docm", "dotx"]
        .into_iter()
        .map(str::to_string)
        .collect()
}

/// 出厂盘的扩展名表（一次性迁移用它刷新旧配置；`None` = 非出厂盘，不动）。
pub(crate) fn stock_extensions(id: &str) -> Option<Vec<String>> {
    match id {
        "images" => Some(image_extensions()),
        "text" => Some(text_extensions()),
        "docs" => Some(docs_extensions()),
        "mixed" => Some(Vec::new()),
        _ => None,
    }
}

/// 环形转换配置（`设置 → 环形转换`）。
///
/// 只有**一个运行手势**（Finder 里 Option＋左键拖动）与**一面环**：背面、
/// Tab/滚轮翻面、单击执行、快捷键与右键服务入口全部取消（见
/// README「三、环形文件转换」）。打包 / 解压 / PDF 分页等工具动作
/// 不进 Profile，运行时按固定顺序追加在正面末尾（见 `utility_actions`）。
///
/// **动作 id 是预设 id，不是目标格式名**；配方本身（目标、质量、输出规则）在
/// [`ConversionConfig`] 里，两个 JPG 动作因此可以有不同质量。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RingConfig {
    /// 文件类型 Profile 列表（顺序即匹配优先级）。
    #[serde(default)]
    pub profiles: Vec<RingProfile>,
    /// 每张盘正面末尾是否追加工具动作（打包 / 解压 / PDF 分页，固定顺序）。
    #[serde(default = "default_true")]
    pub utility_actions: bool,
    /// Finder 里 Option + 拖动文件唤起环形盘（macOS）。
    ///
    /// 只在**前台是 Finder**时生效，且位移要超过 8px 才接管（Option+单击不弹盘）：
    /// 别的 App 里 Option 是输入特殊字符的常用修饰键，抢过来会打断用户。
    /// 需要系统「辅助功能」权限（与回贴同一套授权）。
    #[serde(default = "default_true")]
    pub option_tap: bool,
    /// 按文件类别记住的最后动作（类别 → 预设 id）。
    /// 只用于环形盘初始高亮，不自动执行（任务 30）。
    #[serde(default)]
    pub last_action: Vec<(String, String)>,
    /// 【已废弃】旧版正面顺序，仅用于迁移到 `profiles`。
    #[serde(default, rename = "actions", skip_serializing_if = "Option::is_none")]
    pub legacy_actions: Option<Vec<String>>,
    /// 【已废弃】旧版背面顺序，仅用于迁移到 `profiles`（背面本身已取消）。
    #[serde(
        default,
        rename = "back_actions",
        skip_serializing_if = "Option::is_none"
    )]
    pub legacy_back_actions: Option<Vec<String>>,
    /// 【已废弃】旧版全局 JPG 质量，仅用于迁移到预设（`Config::migrate` 会清空）。
    #[serde(
        default,
        rename = "jpeg_quality",
        skip_serializing_if = "Option::is_none"
    )]
    pub legacy_jpeg_quality: Option<u8>,
    /// 【已废弃】旧版全局输出目录，仅用于迁移到预设（`Config::migrate` 会清空）。
    #[serde(default, rename = "dest_dir", skip_serializing_if = "Option::is_none")]
    pub legacy_dest_dir: Option<String>,
}

/// 转换预设库。
///
/// 独立于 `RingConfig` 存放：正面/背面只保存预设 id 顺序，改圆环顺序不会碰到
/// 配方；反过来改质量也不会重排圆环。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ConversionConfig {
    /// 全部预设（含内置）。为空时 `Config::migrate` 会补回内置预设。
    #[serde(default)]
    pub presets: Vec<ConversionPreset>,
}

impl ConversionConfig {
    /// 出厂预设库。
    pub fn with_builtins() -> Self {
        Self {
            presets: preset::builtin_presets(),
        }
    }
}

impl Default for RingConfig {
    fn default() -> Self {
        Self {
            // 默认 Profile 在 `migrate` 里按预设类别生成（需要预设库才能判断
            // 目标格式），这里留空，避免两份默认值各写一遍。
            profiles: Vec::new(),
            utility_actions: default_true(),
            option_tap: default_true(),
            last_action: Vec::new(),
            legacy_actions: None,
            legacy_back_actions: None,
            legacy_jpeg_quality: None,
            legacy_dest_dir: None,
        }
    }
}


pub(super) const LEGACY_DEFAULT_FRONT: [&str; 5] = ["img.jpg", "img.png", "img.webp", "doc.html", "doc.txt"];
pub(super) const LEGACY_DEFAULT_BACK: [&str; 4] = ["img.gif", "img.bmp", "img.tiff", "doc.md"];

/// 上一版（带背面的版本）出厂布局：正 / 背面都恰好没动过时，升级到新正面默认。
pub(super) const PREV_DEFAULT_FRONT: [&str; 5] = ["img.jpg", "img.png", "doc.md", "doc.html", "doc.txt"];
pub(super) const PREV_DEFAULT_BACK: [&str; 7] = [
    "img.webp", "img.gif", "img.bmp", "img.tiff", "img.heic", "img.avif", "img.ico",
];

/// 旧配置没有 `back_actions` 字段时按上一版默认背面处理（保持旧行为：
/// 缺字段 = 没排过背面 = 用默认，而不是把背面清空）。
pub(super) fn prev_default_back() -> Vec<String> {
    PREV_DEFAULT_BACK.iter().map(|s| s.to_string()).collect()
}

/// 把旧默认布局里的 id 过一遍「当前还能解析」的过滤：本版删掉的格式
/// （doc.md / img.avif / img.ico 等）不再出现在比较列表里，否则老配置会被
/// 误判成「用户自订过」，白白丢掉出厂盘升级。
pub(super) fn kept(ids: &[&str]) -> Vec<String> {
    ids.iter()
        .filter_map(|id| preset::resolve_action_id(id))
        .collect()
}

/// 上一版出厂盘内容：用来识别「没动过」的盘，动过的一律不升级。
const PREV_IMAGES_FRONT: [&str; 2] = ["img.jpg", "img.png"];
const PREV_TEXT_FRONT: [&str; 3] = ["doc.md", "doc.html", "doc.txt"];
const PREV_MIXED_FRONT: [&str; 12] = [
    "img.jpg", "img.png", "doc.md", "doc.html", "doc.txt", "img.webp", "img.gif", "img.bmp",
    "img.tiff", "img.heic", "img.avif", "img.ico",
];
/// 更早一版（合并单面）的图片盘正面。
const LEGACY_MERGED_IMAGES: [&str; 6] = [
    "img.jpg", "img.png", "img.webp", "img.gif", "img.bmp", "img.tiff",
];

/// 出厂盘是否「从没被用户动过」（正面 / 背面恰等于上一版默认内容）。
///
/// 混合兜底兼容两版：12 项（上一版正面+背面）与 9 项（更早一版，没有
/// heic/avif/ico）。
pub(super) fn is_untouched_stock(profile: &RingProfile) -> bool {
    match profile.id.as_str() {
        "images" => {
            (profile.preset_ids == PREV_IMAGES_FRONT && profile.back_ids == PREV_DEFAULT_BACK)
                || (profile.preset_ids == LEGACY_MERGED_IMAGES && profile.back_ids.is_empty())
        }
        "text" => profile.preset_ids == PREV_TEXT_FRONT && profile.back_ids.is_empty(),
        "mixed" => {
            profile.back_ids.is_empty()
                && (profile.preset_ids == PREV_MIXED_FRONT
                    || profile.preset_ids == PREV_MIXED_FRONT[..9])
        }
        _ => false,
    }
}

/// 把一张圆环顺序表归一到「已存在的预设 id」，并按需补回内置预设。
pub(super) fn normalize_ring_list(
    list: &mut Vec<String>,
    presets: &mut Vec<ConversionPreset>,
    fill_default: bool,
) {
    let had_items = !list.is_empty();
    let mut out: Vec<String> = Vec::with_capacity(list.len());
    for raw in list.iter() {
        // 已经是预设 id（包括用户自建的）→ 原样保留，不能再经一次格式名解析，
        // 否则 `preset-mine` 这种非格式 id 会被当成未知项丢掉。
        if presets.iter().any(|p| p.id == *raw) {
            if !out.contains(raw) {
                out.push(raw.clone());
            }
            continue;
        }
        // 旧版裸格式名（`"jpg"`）→ 内置预设 id，必要时把被裁掉的内置预设补回来。
        let Some(id) = preset::resolve_action_id(raw) else {
            continue;
        };
        ensure_preset(&id, presets);
        if !presets.iter().any(|p| p.id == id) {
            continue;
        }
        if !out.contains(&id) {
            out.push(id);
        }
    }
    // 整表都被丢弃（配置里的动作全都不认识了）时保留默认，不让圆环变空盘。
    if had_items && out.is_empty() && fill_default {
        out = preset::default_front_actions();
    }
    *list = out;
}

/// 清理 Profile 列表：id 唯一、扩展名规范化、预设引用可解析、至少一个混合兜底。
///
/// 每次落库都会跑（见 `Config::migrate`），所以必须幂等：已经合法的 Profile
/// 原样保留，不会因为再迁移一次就换顺序或改名。
pub(super) fn normalize_profiles(profiles: &mut Vec<RingProfile>, presets: &mut Vec<ConversionPreset>) {
    let mut out: Vec<RingProfile> = Vec::with_capacity(profiles.len());
    for (index, mut profile) in profiles.drain(..).enumerate() {
        let id = profile.id.trim().to_string();
        profile.id = if id.is_empty() {
            format!("profile-{}", index + 1)
        } else {
            id
        };
        // 同 id 只留第一个：否则设置页会出现两行改同一个圆环。
        if out.iter().any(|p| p.id == profile.id) {
            continue;
        }
        let name = profile.name.trim().to_string();
        profile.name = if name.is_empty() {
            profile.id.clone()
        } else {
            name
        };
        profile.extensions = normalize_extensions(&profile.extensions);
        // 与旧圆环顺序表同一套规则：预设 id 原样保留，旧格式名换成预设 id，
        // 指向已删预设的引用丢掉（Filee 行为：不允许悬空 id）。
        normalize_ring_list(&mut profile.preset_ids, presets, false);
        profile.preset_ids.truncate(MAX_PROFILE_SLICES);
        // 背面已取消：任何残留的背面顺序直接清掉（迁移阶段已并入正面）。
        profile.back_ids.clear();
        out.push(profile);
    }

    // 至少留一个混合兜底：否则没配置过的文件类型根本没有盘可拖（Filee 行为）。
    if !out.iter().any(|p| p.kind == RingProfileKind::MixedFallback) {
        let id = unique_profile_id(&out, "mixed");
        out.push(RingProfile {
            id,
            name: "其他文件".to_string(),
            extensions: Vec::new(),
            kind: RingProfileKind::MixedFallback,
            preset_ids: preset::default_mixed_actions(),
            back_ids: Vec::new(),
        });
    }

    // 所有 Profile 都空了（预设被删光 / 引用全失效）→ 用出厂盘补回，
    // 宁可回到默认盘，也不能让一处配置坏掉就把整个功能变成空盘。
    if out.iter().all(|p| p.preset_ids.is_empty()) {
        *profiles = RingProfile::stock();
        return;
    }
    *profiles = out;
}

/// 生成不与现有 Profile 冲突的 id（`mixed`、`mixed-2`……）。
fn unique_profile_id(profiles: &[RingProfile], base: &str) -> String {
    if !profiles.iter().any(|p| p.id == base) {
        return base.to_string();
    }
    (2..)
        .map(|n| format!("{base}-{n}"))
        .find(|id| !profiles.iter().any(|p| &p.id == id))
        .unwrap_or_else(|| base.to_string())
}

/// 内置预设能按 id 补回来就补，补不回来（用户自建又删掉的）就留给调用方丢掉。
fn ensure_preset(id: &str, presets: &mut Vec<ConversionPreset>) {
    if presets.iter().any(|p| p.id == id) {
        return;
    }
    if let Some(builtin) = preset::builtin_presets().into_iter().find(|p| p.id == id) {
        presets.push(builtin);
    }
}
