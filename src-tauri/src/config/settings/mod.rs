//! 配置数据结构：`Config` / `SyncConfig` / `HotkeyConfig` / `SyncCredentials` 等。

use serde::{Deserialize, Serialize};

use crate::core::convert::format::ConvertFormat;
use crate::core::convert::preset;

use ring::{
    docs_extensions, is_untouched_stock, kept, merge_back_into_front, normalize_profiles,
    normalize_ring_list, prev_default_back, stock_content, LEGACY_DEFAULT_BACK,
    LEGACY_DEFAULT_FRONT, PREV_DEFAULT_BACK, PREV_DEFAULT_FRONT,
};

/// 历史条数默认上限。
pub const DEFAULT_MAX_ITEMS: u32 = 2000;
/// 历史容量默认上限（字节）。
pub const DEFAULT_MAX_BYTES: u64 = 1024 * 1024 * 1024;

/// 敏感内容处理策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SensitivityPolicy {
    /// 脱敏后入库（默认）。
    ///
    /// 兼顾两条底线：① 不静默丢弃 —— 启发式规则有误报，旧 `Ignore` 默认会把
    /// 命中条目悄悄丢掉（复制一句「把 token 放进 header」就没了），代价大于标记，
    /// 脱敏至少保留「这里有敏感内容」的占位；② 不明文落盘 —— 命中的密码 / token
    /// 写成 `[敏感内容已脱敏 <hash 前 8 位>]`，同用户进程、Time Machine、备份都读不到原文。
    /// 需要明文时由用户在设置里显式切回 [`Self::Flag`]。
    #[default]
    Mask,
    /// 识别为敏感内容后照常入库，只打 `sensitive` 标记（明文留存，仅作提示）。
    Flag,
    /// 识别为敏感内容后不入库。
    Ignore,
    /// 加密后入库（应用层 AEAD）。
    ///
    /// **尚未实现**：本地字段级加密需要 Keychain 主密钥（见 README「已知限制」）。
    /// 在它落地前行为与 [`Self::Mask`] 一致 —— 宁可降级为脱敏，也不能按明文入库
    /// 让这个选项变成谎话（见 `state.rs::ingest_clip_event`）。
    Encrypt,
}

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
    /// 状态栏图标与下拉面板。
    #[serde(default)]
    pub statusbar: StatusBarConfig,
    /// 上次停留的设置分区（任务 1：主窗口默认打开上次分区）。
    #[serde(default = "default_settings_section")]
    pub last_settings_section: String,
}

pub(super) fn default_max_items() -> u32 {
    DEFAULT_MAX_ITEMS
}

pub(super) fn default_max_bytes() -> u64 {
    // 对标 iCopy 默认 `pasteboardDiskLimit = 1024`（MB）。
    DEFAULT_MAX_BYTES
}

pub(super) fn default_poll_interval() -> u64 {
    500
}

pub(super) fn default_true() -> bool {
    true
}

fn default_theme() -> String {
    "system".to_string()
}

fn default_board_style() -> String {
    "list".to_string()
}

fn default_board_height_scale() -> u32 {
    50
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
            sensitivity: SensitivityPolicy::Mask,
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
            statusbar: StatusBarConfig::default(),
            last_settings_section: default_settings_section(),
        }
    }
}

impl Config {
    /// 迁移旧配置到预设模型（README「三、环形文件转换」）。
    ///
    /// 处理三件事，处理完把旧字段清空，因此**可重复调用**：
    /// 1. 预设库为空 → 补内置预设；
    /// 2. 旧的全局 `ring.jpeg_quality` → 只写进 `img.jpg` 这一条配方；
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

        // 本版删掉了用不到的格式：内置预设只保留当前仍提供的 id，用户自建的
        // （builtin = false）一律保留。删掉的内置 id 不会再被下面的补全逻辑复活。
        presets.retain(|p| !p.builtin || preset::BUILTIN_IDS.contains(&p.id.as_str()));

        // 版本升级：本版新增的内置预设补进用户库（新增格式的动作才可选）。
        // 旧版内置 id 缺席视为用户删过，不复活。
        for builtin in preset::builtin_presets() {
            if seen.contains(&builtin.id)
                || preset::LEGACY_BUILTIN_IDS.contains(&builtin.id.as_str())
            {
                continue;
            }
            seen.insert(builtin.id.clone());
            presets.push(builtin);
        }

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

        // 旧「正 / 背面」→ 文件类型 Profile（Filee 行为）。
        //
        // 背面已取消：从未自订过的旧默认布局（含上一版正面 5 项 + 背面 7 项）
        // 直接升级到新的正面默认（新增格式、文档目标全部开箱可见）；用户自己
        // 排过的旧配置保持合并后的顺序，只做「背面追加到正面」的去重合并。
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
                .unwrap_or_else(|| prev_default_back().to_vec());
            normalize_ring_list(&mut front, &mut presets, true);
            normalize_ring_list(&mut back, &mut presets, false);

            // 旧默认布局把 `doc.md` 放到了背面：选一个 `.txt` 时正面只剩 HTML（图片
            // 预设对 txt 无路由，txt 本身又是目标），盘上看着就像只剩一个动作。只在
            // 顺序「完全等于旧默认（或上一版默认）」时升级，用户自己排过的顺序一律不动。
            let untouched_default = (front == kept(&LEGACY_DEFAULT_FRONT)
                && back == kept(&LEGACY_DEFAULT_BACK))
                || (front == kept(&PREV_DEFAULT_FRONT) && back == kept(&PREV_DEFAULT_BACK))
                // 全新配置（旧字段缺省）：正面/背面恰好是两版默认值，按出厂盘建。
                || (front == preset::default_front_actions()
                    && back == kept(&PREV_DEFAULT_BACK));
            if untouched_default {
                self.ring.profiles = RingProfile::stock();
            } else {
                let mut merged = front;
                for id in &back {
                    if !merged.contains(id) {
                        merged.push(id.clone());
                    }
                }
                merged.truncate(MAX_PROFILE_SLICES);
                self.ring.profiles = RingProfile::seeds(&merged, &presets);
            }
        } else {
            // 已经建过 Profile 的旧配置（上一版曾把正/背面拆成两张盘）。
            //
            // 背面并入正面（去重、截断），没动过的出厂盘升级到新的正面默认，
            // 让新增格式开箱可见；用户自己排过的盘一律不动。
            for profile in &mut self.ring.profiles {
                if is_untouched_stock(profile) {
                    profile.preset_ids = stock_content(&profile.id);
                    profile.back_ids.clear();
                } else {
                    merge_back_into_front(profile);
                }
            }
            self.ring.legacy_actions = None;
            self.ring.legacy_back_actions = None;
        }
        normalize_profiles(&mut self.ring.profiles, &mut presets);

        // 恰好是出厂三盘（没有自建盘）→ 补「文档」盘：新盘没有旧数据可保留，
        // 直接按默认内容建。位置放在混合兜底之前，保证匹配优先级正确。
        let ids: std::collections::HashSet<&str> =
            self.ring.profiles.iter().map(|p| p.id.as_str()).collect();
        if ids.len() == 3 && ids.contains("images") && ids.contains("text") && ids.contains("mixed")
        {
            let docs = RingProfile {
                id: "docs".to_string(),
                name: "文档".to_string(),
                extensions: docs_extensions(),
                kind: RingProfileKind::Normal,
                preset_ids: stock_content("docs"),
                back_ids: Vec::new(),
            };
            let pos = self
                .ring
                .profiles
                .iter()
                .position(|p| p.kind == RingProfileKind::MixedFallback)
                .unwrap_or(self.ring.profiles.len());
            self.ring.profiles.insert(pos, docs);
        }

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

        self.statusbar.normalize();
    }
}

mod hotkey;
mod ring;
mod statusbar;
mod sync;

#[cfg(test)]
mod tests;

pub use hotkey::HotkeyConfig;
pub(crate) use hotkey::default_toggle_hotkey;
pub use ring::{ConversionConfig, RingConfig, RingProfile, RingProfileKind, MAX_PROFILE_SLICES};
pub(crate) use ring::stock_extensions;
pub use statusbar::StatusBarConfig;
pub use sync::{SyncConfig, SyncCredentials, SyncProviderKind};
