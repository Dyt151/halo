//! 配置数据结构：`Config` / `SyncConfig` / `HotkeyConfig` / `SyncCredentials` 等。

use serde::{Deserialize, Serialize};

use crate::core::convert::format::ConvertFormat;
use crate::core::convert::preset::{self, ConversionPreset};
use crate::core::ring::profile::normalize_extensions;

/// 敏感内容处理策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SensitivityPolicy {
    /// 默认：识别为敏感内容后不入库。
    #[default]
    Ignore,
    /// 脱敏后入库。
    Mask,
    /// 加密后入库（应用层 AEAD）。
    ///
    /// **尚未实现**：本地字段级加密需要 Keychain 主密钥（见 `docs/IMPLEMENTATION.md` §4.1）。
    /// 在它落地前行为与 [`Self::Mask`] 一致 —— 宁可降级为脱敏，也不能按明文入库
    /// 让这个选项变成谎话（见 `state.rs::ingest_clip_event`）。
    Encrypt,
}

/// 同步后端类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SyncProviderKind {
    #[default]
    WebDav,
    /// 预留。
    S3,
}

/// WebDAV 凭据（序列化存储；`password` 属敏感字段，运行时从配置读取）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SyncCredentials {
    pub url: String,
    pub username: String,
    pub password: String,
}

/// 同步配置。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyncConfig {
    /// 同步开关，默认关闭（本地优先）。
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub provider: SyncProviderKind,
    #[serde(default)]
    pub credentials: SyncCredentials,
    /// 端到端加密开关。
    #[serde(default)]
    pub encryption: bool,
    /// E2E 加密口令（可选，为空表示未设置）。
    #[serde(default)]
    pub passphrase: String,
    /// 同步远端文件名。
    #[serde(default = "default_sync_file")]
    pub remote_file: String,
    /// 自动同步间隔（秒），0 表示关闭定时。
    #[serde(default = "default_sync_interval")]
    pub interval_secs: u64,
}

fn default_sync_file() -> String {
    "clipbase-sync.json".to_string()
}

fn default_sync_interval() -> u64 {
    300
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: SyncProviderKind::WebDav,
            credentials: SyncCredentials::default(),
            encryption: false,
            passphrase: String::new(),
            remote_file: default_sync_file(),
            interval_secs: default_sync_interval(),
        }
    }
}

/// 全局快捷键配置。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HotkeyConfig {
    /// 唤起/收起剪贴板面板。
    #[serde(default = "default_toggle_hotkey")]
    pub toggle: String,
    /// 粘贴堆栈顶。
    #[serde(default = "default_stack_paste_hotkey")]
    pub stack_paste: String,
    /// 唤起快捷回复选择器（现场回贴，不经过主窗口）。
    #[serde(default = "default_reply_hotkey")]
    pub reply_picker: String,
}

fn default_toggle_hotkey() -> String {
    "Control+C".to_string()
}

fn default_stack_paste_hotkey() -> String {
    "Control+Shift+V".to_string()
}

fn default_reply_hotkey() -> String {
    "Control+Shift+R".to_string()
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            toggle: default_toggle_hotkey(),
            stack_paste: default_stack_paste_hotkey(),
            reply_picker: default_reply_hotkey(),
        }
    }
}

/// 一个文件类型 Profile：某类文件出现时，圆盘上放哪些预设。
///
/// 这一层只回答「这批文件用哪个圆环」，不复制配方：里面的 id 全部指向
/// [`ConversionConfig::presets`]。删预设时必须在同一次配置提交里清掉所有
/// Profile 的引用，否则会出现点了没反应的扇区。
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
    /// 普通 / 混合兜底（决定匹配优先级，见 `docs/REA-Filee-Ring-Integrated.md` §2.2）。
    #[serde(default)]
    pub kind: RingProfileKind,
    /// 唯一一面的预设顺序：索引 0 在 12 点，顺时针，最多 12 条。
    #[serde(default)]
    pub preset_ids: Vec<String>,
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

/// 每个 Profile 最多几个扇区（Filee `ToolbarProfile.MaxSlices`）。
pub const MAX_PROFILE_SLICES: usize = 12;

impl RingProfile {
    /// 出厂 Profile：图片 / 文本 / 混合兜底。
    ///
    /// `merged` 是旧版正背面合并后的顺序（迁移）或默认顺序；每个 Profile 只拿
    /// 自己类别的那几条，顺序仍然跟随 `merged`，用户原来的偏好不会被打乱。
    fn seeds(merged: &[String], presets: &[ConversionPreset]) -> Vec<RingProfile> {
        let images: Vec<String> = merged
            .iter()
            .filter(|id| is_image_preset(presets, id))
            .cloned()
            .collect();
        let texts: Vec<String> = merged
            .iter()
            .filter(|id| !is_image_preset(presets, id))
            .cloned()
            .collect();

        let mut out = Vec::new();
        if !images.is_empty() {
            out.push(RingProfile {
                id: "images".to_string(),
                name: "图片".to_string(),
                extensions: image_extensions(),
                kind: RingProfileKind::Normal,
                preset_ids: images,
            });
        }
        if !texts.is_empty() {
            out.push(RingProfile {
                id: "text".to_string(),
                name: "文本".to_string(),
                extensions: text_extensions(),
                kind: RingProfileKind::Normal,
                preset_ids: texts,
            });
        }
        // 至少留一个 catch-all：没配过的类型也要有盘可拖（§2.2 末条）。
        out.push(RingProfile {
            id: "mixed".to_string(),
            name: "其他文件".to_string(),
            extensions: Vec::new(),
            kind: RingProfileKind::MixedFallback,
            preset_ids: merged.iter().take(MAX_PROFILE_SLICES).cloned().collect(),
        });
        out
    }
}

/// 预设是否产出图片（目标为图片格式，或「同格式重编码」）。
fn is_image_preset(presets: &[ConversionPreset], id: &str) -> bool {
    match presets.iter().find(|p| p.id == id).map(|p| &p.target) {
        Some(preset::TargetSpec::Explicit(format)) => format.is_image(),
        // SameAsSource 只会出现在图片（重编码）上，按图片归类。
        Some(preset::TargetSpec::SameAsSource) => true,
        None => false,
    }
}

/// 图片族的全部扩展名（含别名，与 `ConvertFormat::from_ext` 同一张表）。
fn image_extensions() -> Vec<String> {
    ["png", "jpg", "jpeg", "jpe", "webp", "gif", "bmp", "tif", "tiff"]
        .into_iter()
        .map(str::to_string)
        .collect()
}

/// 文本族的全部扩展名。
fn text_extensions() -> Vec<String> {
    [
        "md", "markdown", "mdown", "mkd", "html", "htm", "xhtml", "txt", "text",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

/// 环形转换配置（`设置 → 环形转换`）。
///
/// 只有**一个运行手势**（Finder 里 Option＋左键拖动）与**一面环**：正背面、
/// Tab/滚轮翻面、单击执行、快捷键与右键服务入口全部取消（见
/// `docs/REA-Filee-Ring-Integrated.md`）。
///
/// **动作 id 是预设 id，不是目标格式名**；配方本身（目标、质量、输出规则）在
/// [`ConversionConfig`] 里，两个 JPG 动作因此可以有不同质量。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RingConfig {
    /// 文件类型 Profile 列表（顺序即匹配优先级）。
    #[serde(default)]
    pub profiles: Vec<RingProfile>,
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
    #[serde(default, rename = "back_actions", skip_serializing_if = "Option::is_none")]
    pub legacy_back_actions: Option<Vec<String>>,
    /// 【已废弃】旧版全局 JPG 质量，仅用于迁移到预设（`Config::migrate` 会清空）。
    #[serde(default, rename = "jpeg_quality", skip_serializing_if = "Option::is_none")]
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

/// 默认正面顺序：常用目标在前（每面最多 12 个，Filee `MaxSlices`）。
#[allow(dead_code)]
fn default_ring_actions() -> Vec<String> {
    preset::default_front_actions()
}

/// 默认背面顺序：只用于迁移旧配置（背面本身已取消）。
#[allow(dead_code)]
fn default_ring_back_actions() -> Vec<String> {
    preset::default_back_actions()
}

impl Default for RingConfig {
    fn default() -> Self {
        Self {
            // 默认 Profile 在 `migrate` 里按预设类别生成（需要预设库才能判断
            // 目标格式），这里留空，避免两份默认值各写一遍。
            profiles: Vec::new(),
            option_tap: default_true(),
            last_action: Vec::new(),
            legacy_actions: None,
            legacy_back_actions: None,
            legacy_jpeg_quality: None,
            legacy_dest_dir: None,
        }
    }
}

/// 应用总配置。
///
/// 容量默认值对外公开：容量策略把「0」当作「没设过」处理时需要回落值，
/// 不能再写一份字面量（两处数值一定会走偏）。
pub const DEFAULT_MAX_ITEMS: u32 = 2000;
pub const DEFAULT_MAX_BYTES: u64 = 1024 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    /// 历史条数上限。
    #[serde(default = "default_max_items")]
    pub max_items: u32,
    /// 历史容量上限（字节）。
    #[serde(default = "default_max_bytes")]
    pub max_bytes: u64,
    /// 剪贴板监控开关。
    #[serde(default = "default_true")]
    pub monitor_enabled: bool,
    /// 轮询间隔（毫秒）。
    #[serde(default = "default_poll_interval")]
    pub poll_interval_ms: u64,
    /// 敏感内容策略。
    #[serde(default)]
    pub sensitivity: SensitivityPolicy,
    /// 快捷键。
    #[serde(default)]
    pub hotkeys: HotkeyConfig,
    /// 同步。
    #[serde(default)]
    pub sync: SyncConfig,
    /// 开机启动（P0 仅记录，实际注册延后）。
    #[serde(default)]
    pub launch_at_login: bool,
    /// 外观主题。
    #[serde(default = "default_theme")]
    pub theme: String,
    /// 粘贴为纯文本。
    #[serde(default)]
    pub paste_as_plain: bool,
    /// 粘贴前去除文本尾部空白与换行（对标 iCopy 的
    /// `writeToPasteBoardWithTxtWithoutNewlineAndWhitespace:`，默认开启）。
    #[serde(default = "default_true")]
    pub paste_trim_trailing: bool,
    /// 删除 / 清空历史前弹出确认（对标 iCopy `confirmationOnDeletion`，默认开启）。
    #[serde(default = "default_true")]
    pub confirm_on_delete: bool,
    /// 操作完成后自动关闭窗口。
    #[serde(default = "default_true")]
    pub auto_close_after_paste: bool,
    /// 单击卡片是否直接回贴（false = 单击仅复制，双击/Enter 回贴）。
    /// 回贴是不可逆的对外操作，macOS 习惯默认不把单击绑到它上。
    #[serde(default)]
    pub paste_on_single_click: bool,
    /// 音效。
    #[serde(default)]
    pub sound_enabled: bool,
    /// 面板内数字快捷键（⌘1~9 回贴第 N 条）。
    ///
    /// **不注册为全局快捷键**：系统级接管会抢走所有 App 的 ⌘1~9。
    #[serde(default = "default_true")]
    pub num_hotkey_enabled: bool,
    /// 堆栈快捷键开关。
    #[serde(default = "default_true")]
    pub stack_enabled: bool,
    /// 面板样式（horizontal/vertical）。
    #[serde(default = "default_board_style")]
    pub board_style: String,
    /// 面板高度百分比。
    #[serde(default = "default_board_height_scale")]
    pub board_height_scale: u32,
    /// 窗口出现位置（follow_cursor/screen/bottom）。
    #[serde(default = "default_board_position")]
    pub board_position: String,
    /// 文本预览（plain/rich）。
    #[serde(default = "default_text_preview")]
    pub text_preview: String,
    /// 历史时间限制（天），0 = 无限制。
    #[serde(default)]
    pub history_ttl_days: u32,
    /// 按 App 黑名单（来源 App 名命中则跳过入库）。
    #[serde(default)]
    pub app_blacklist: Vec<String>,
    /// 环形转换（Finder 现场转换）配置。
    #[serde(default)]
    pub ring: RingConfig,
    /// 转换预设库（环形动作引用的配方都在这里）。
    #[serde(default)]
    pub conversion: ConversionConfig,
    /// 上次停留的设置分区（任务 1：主窗口默认打开上次分区）。
    #[serde(default = "default_settings_section")]
    pub last_settings_section: String,
}

fn default_max_items() -> u32 {
    DEFAULT_MAX_ITEMS
}

fn default_max_bytes() -> u64 {
    // 对标 iCopy 默认 `pasteboardDiskLimit = 1024`（MB）。
    DEFAULT_MAX_BYTES
}

fn default_poll_interval() -> u64 {
    500
}

fn default_true() -> bool {
    true
}

fn default_theme() -> String {
    "system".to_string()
}

fn default_board_style() -> String {
    "horizontal".to_string()
}

fn default_board_height_scale() -> u32 {
    30
}

fn default_board_position() -> String {
    "bottom".to_string()
}

fn default_text_preview() -> String {
    "rich".to_string()
}

fn default_settings_section() -> String {
    "general".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_items: default_max_items(),
            max_bytes: default_max_bytes(),
            monitor_enabled: true,
            poll_interval_ms: default_poll_interval(),
            sensitivity: SensitivityPolicy::Ignore,
            hotkeys: HotkeyConfig::default(),
            sync: SyncConfig::default(),
            launch_at_login: false,
            theme: default_theme(),
            paste_as_plain: false,
            paste_trim_trailing: true,
            confirm_on_delete: true,
            auto_close_after_paste: true,
            paste_on_single_click: false,
            sound_enabled: true,
            num_hotkey_enabled: true,
            stack_enabled: true,
            board_style: default_board_style(),
            board_height_scale: default_board_height_scale(),
            board_position: default_board_position(),
            text_preview: default_text_preview(),
            history_ttl_days: 0,
            app_blacklist: Vec::new(),
            ring: RingConfig::default(),
            conversion: ConversionConfig::with_builtins(),
            last_settings_section: default_settings_section(),
        }
    }
}

impl Config {
    /// 迁移旧配置到预设模型（docs/REA-Filee-Conversion.md §8.1）。
    ///
    /// 处理三件事，处理完把旧字段清空，因此**可重复调用**：
    /// 1. 预设库为空 → 补内置预设；
    /// 2. 旧的全局 `ring.jpeg_quality` → 只写进 `img.jpg` 这一条配方
    ///    （`img.jpg.q70` 是另一条配方，不能被顺手改成全局质量）；
    /// 3. 旧的全局 `ring.dest_dir` → 所有还没设输出目录的预设。
    ///
    /// 另外把旧的裸格式 id（`"jpg"`）重写成预设 id（`"img.jpg"`）：圆环顺序表
    /// 从此只存预设 id，找不到的预设按内置补回来，保证扇区不会「点了没反应」。
    pub fn migrate(&mut self) {
        let mut presets = if self.conversion.presets.is_empty() {
            preset::builtin_presets()
        } else {
            std::mem::take(&mut self.conversion.presets)
        };
        // 去重：以先出现的为准，避免 UI 里两条同 id 的配方互相覆盖。
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        presets.retain(|p| !p.id.trim().is_empty() && seen.insert(p.id.clone()));

        if let Some(quality) = self.ring.legacy_jpeg_quality.take() {
            let quality = quality.clamp(1, 100);
            if let Some(p) = presets.iter_mut().find(|p| p.id == "img.jpg") {
                p.image.jpeg_quality = quality;
                if p.builtin {
                    p.name = preset::default_preset_name(ConvertFormat::Jpg, quality);
                }
            }
        }

        let legacy_dir = self.ring.legacy_dest_dir.take().unwrap_or_default();
        let legacy_dir = legacy_dir.trim().to_string();
        if !legacy_dir.is_empty() {
            for p in presets.iter_mut() {
                if p.output.dir.trim().is_empty() {
                    p.output.dir = legacy_dir.clone();
                }
            }
        }

        // 旧「正 / 背面」→ 文件类型 Profile（总规范 §8.1）：正面保持原顺序作为主环，
        // 背面追加在后并去重，超过 12 项的部分**留在预设库**（不删配方，只是不上环）。
        if self.ring.profiles.is_empty() {
            let mut front = self
                .ring
                .legacy_actions
                .take()
                .unwrap_or_else(preset::default_front_actions);
            let mut back = self
                .ring
                .legacy_back_actions
                .take()
                .unwrap_or_else(preset::default_back_actions);
            normalize_ring_list(&mut front, &mut presets, true);
            normalize_ring_list(&mut back, &mut presets, false);

            // 旧默认布局把 `doc.md` 放到了背面：选一个 `.txt` 时正面只剩 HTML（图片
            // 预设对 txt 无路由，txt 本身又是目标），盘上看着就像只剩一个动作。只在
            // 顺序「完全等于旧默认」时升级，用户自己排过的顺序一律不动。
            if front == LEGACY_DEFAULT_FRONT && back == LEGACY_DEFAULT_BACK {
                front = preset::default_front_actions();
                back = preset::default_back_actions();
            }

            let mut merged = front;
            for id in back {
                if !merged.contains(&id) {
                    merged.push(id);
                }
            }
            merged.truncate(MAX_PROFILE_SLICES);
            self.ring.profiles = RingProfile::seeds(&merged, &presets);
        } else {
            // 已经有 Profile（部分迁移过、或用户自建）：旧字段只清不读。
            self.ring.legacy_actions = None;
            self.ring.legacy_back_actions = None;
        }
        normalize_profiles(&mut self.ring.profiles, &mut presets);

        let mut last = Vec::with_capacity(self.ring.last_action.len());
        for (category, raw) in self.ring.last_action.iter() {
            // 与顺序表同理：自建预设 id 必须原样保住，只有旧格式名才映射。
            let id = if presets.iter().any(|p| p.id == *raw) {
                raw.clone()
            } else {
                let Some(mapped) = preset::resolve_action_id(raw) else {
                    continue;
                };
                if !presets.iter().any(|p| p.id == mapped) {
                    continue;
                }
                mapped
            };
            if last
                .iter()
                .any(|(c, i): &(String, String)| c == category && i == &id)
            {
                continue;
            }
            last.push((category.clone(), id));
        }
        self.ring.last_action = last;

        self.conversion.presets = presets;
    }
}

/// 迁移前（按格式名映射后）的默认布局：用来识别「从未自订过」的配置。
const LEGACY_DEFAULT_FRONT: [&str; 5] = ["img.jpg", "img.png", "img.webp", "doc.html", "doc.txt"];
const LEGACY_DEFAULT_BACK: [&str; 4] = ["img.gif", "img.bmp", "img.tiff", "doc.md"];

/// 把一张圆环顺序表归一到「已存在的预设 id」，并按需补回内置预设。
fn normalize_ring_list(
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
fn normalize_profiles(profiles: &mut Vec<RingProfile>, presets: &mut Vec<ConversionPreset>) {
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
        // 指向已删预设的引用丢掉（总规范 §1.1：不允许悬空 id）。
        normalize_ring_list(&mut profile.preset_ids, presets, false);
        profile.preset_ids.truncate(MAX_PROFILE_SLICES);
        out.push(profile);
    }

    // 至少留一个混合兜底：否则没配置过的文件类型根本没有盘可拖（§2.2）。
    if !out
        .iter()
        .any(|p| p.kind == RingProfileKind::MixedFallback)
    {
        let id = unique_profile_id(&out, "mixed");
        out.push(RingProfile {
            id,
            name: "其他文件".to_string(),
            extensions: Vec::new(),
            kind: RingProfileKind::MixedFallback,
            preset_ids: preset::default_front_actions(),
        });
    }

    // 所有 Profile 都空了（预设被删光 / 引用全失效）→ 用出厂顺序补回，
    // 宁可回到默认盘，也不能让一处配置坏掉就把整个功能变成空盘。
    if out.iter().all(|p| p.preset_ids.is_empty()) {
        let mut merged = preset::default_front_actions();
        for id in preset::default_back_actions() {
            if !merged.contains(&id) {
                merged.push(id);
            }
        }
        merged.truncate(MAX_PROFILE_SLICES);
        *profiles = RingProfile::seeds(&merged, presets);
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

#[cfg(test)]
mod tests {
    use super::*;

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
        let images = profile_by_id(&config, "images").expect("图片 Profile");
        assert_eq!(
            images.preset_ids,
            vec!["img.jpg", "img.png", "img.gif"]
                .into_iter()
                .map(str::to_string)
                .collect::<Vec<_>>()
        );
        let text = profile_by_id(&config, "text").expect("文本 Profile");
        assert_eq!(text.preset_ids, vec!["doc.md".to_string()]);
        // 最后一个必须是 catch-all，否则没配过的类型没有盘可拖。
        let mixed = config.ring.profiles.last().expect("至少一个 Profile");
        assert_eq!(mixed.kind, RingProfileKind::MixedFallback);
        assert!(mixed.extensions.is_empty());
        assert_eq!(mixed.preset_ids.len(), 4);

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

        // 另一条 JPG 配方（70%）不能被全局质量顺手改掉——这正是旧模型做不到的事。
        let q70 = config
            .conversion
            .presets
            .iter()
            .find(|p| p.id == "img.jpg.q70")
            .expect("img.jpg.q70 应存在");
        assert_eq!(q70.image.jpeg_quality, 70);
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
            ..preset::builtin_presets()
                .into_iter()
                .next()
                .expect("内置预设不为空")
        });

        config.migrate();
        // 未知的自定义预设 id 必须保住；旧格式名换成预设 id；背面缺省时取旧默认
        // 背面，拼在正面之后（§8.1：正面在前 + 背面追加去重）。
        let images = profile_by_id(&config, "images").expect("图片 Profile");
        assert_eq!(
            images.preset_ids,
            vec!["img.jpg", "preset-mine", "img.webp", "img.gif", "img.bmp", "img.tiff"]
                .into_iter()
                .map(str::to_string)
                .collect::<Vec<_>>()
        );
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

    /// 从未自订过的旧默认布局会被升级（否则选 .txt 时正面只剩 HTML）。
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

        // 升级后文档目标全在文本盘：txt / md 都能看到两个以上动作。
        let text = profile_by_id(&config, "text").expect("文本 Profile");
        assert_eq!(
            text.preset_ids,
            vec!["doc.md", "doc.html", "doc.txt"]
                .into_iter()
                .map(str::to_string)
                .collect::<Vec<_>>()
        );
        let images = profile_by_id(&config, "images").expect("图片 Profile");
        assert_eq!(
            images.preset_ids,
            vec!["img.jpg", "img.png", "img.webp", "img.gif", "img.bmp", "img.tiff"]
                .into_iter()
                .map(str::to_string)
                .collect::<Vec<_>>()
        );
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
        assert_eq!(
            text.preset_ids,
            vec!["doc.txt", "doc.html"]
                .into_iter()
                .map(str::to_string)
                .collect::<Vec<_>>()
        );
        // 用户没配图片：不该凭空多出一个空的图片 Profile。
        assert!(profile_by_id(&config, "images").is_none());
    }

    /// 指向已删预设的引用、重复 id、非法扩展名都在迁移时清掉。
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
            },
            RingProfile {
                id: "custom".to_string(),
                name: "重复 id".to_string(),
                extensions: Vec::new(),
                kind: RingProfileKind::MixedFallback,
                preset_ids: vec!["doc.md".to_string()],
            },
        ];

        config.migrate();

        assert_eq!(config.ring.profiles.len(), 2, "重复 id 的那条被丢弃");
        let first = &config.ring.profiles[0];
        assert_eq!(first.name, "custom", "空名字回落到 id");
        assert_eq!(first.extensions, vec!["png".to_string()]);
        assert_eq!(first.preset_ids, vec!["img.jpg".to_string()]);
        // 原有的 MixedFallback 被同 id 丢掉 → 自动补一个 catch-all。
        let mixed = config.ring.profiles.last().expect("兜底 Profile");
        assert_eq!(mixed.kind, RingProfileKind::MixedFallback);
        assert!(mixed.extensions.is_empty());
        assert!(!mixed.preset_ids.is_empty());
    }

    fn profile_by_id<'a>(config: &'a Config, id: &str) -> Option<&'a RingProfile> {
        config.ring.profiles.iter().find(|p| p.id == id)
    }
}
