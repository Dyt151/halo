//! 转换预设（Preset）：动作身份不是目标格式字符串，而是一份完整配方。
//!
//! 对照 docs/REA-Filee-Conversion.md §2.2 / §8.1：
//! - `target` 只是配方里的一项，另外还有图像选项和输出规则；
//! - 因此「两个 JPG 动作」可以有不同的质量和命名，互不影响；
//! - `SameAsSource` 用 `"*"` 表示「目标按每个源文件自己的格式解析」，
//!   它不是「无需转换」：JPG→JPG 降质量、PNG→PNG 重编码都是合法任务。
//!
//! 只放当前真实支持的字段（§8.1 约束：不先做空壳 UI）。

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::format::{ConvertFormat, ALL_FORMATS, DEFAULT_JPEG_QUALITY};

/// 输出文件名模板的默认值。
pub const DEFAULT_NAME_TEMPLATE: &str = "{name}.{ext}";
/// 透明通道合成到无 alpha 目标时的默认背景（白）。
pub const DEFAULT_ALPHA_BACKGROUND: &str = "#FFFFFF";

/// 目标规格：显式格式，或「与源格式相同」（Filee 的 `"*"`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetSpec {
    Explicit(ConvertFormat),
    SameAsSource,
}

impl TargetSpec {
    /// 解析出实际要转成的格式。
    pub fn resolve(&self, source: ConvertFormat) -> ConvertFormat {
        match self {
            Self::Explicit(f) => *f,
            Self::SameAsSource => source,
        }
    }

    /// 序列化取值（`"jpg"` / `"*"`）。
    pub fn as_str(&self) -> String {
        match self {
            Self::Explicit(f) => f.as_str().to_string(),
            Self::SameAsSource => "*".to_string(),
        }
    }
}

impl Serialize for TargetSpec {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.as_str())
    }
}

impl<'de> Deserialize<'de> for TargetSpec {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        if raw.trim() == "*" {
            return Ok(Self::SameAsSource);
        }
        ConvertFormat::from_ext(&raw)
            .map(Self::Explicit)
            .ok_or_else(|| serde::de::Error::custom(format!("未知的目标格式：{raw}")))
    }
}

/// 图像选项。目前只有真实生效的两项（质量、透明背景）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageOptions {
    /// JPEG 输出质量（1-100），只对目标为 JPG 的路由生效。
    #[serde(default = "default_jpeg_quality")]
    pub jpeg_quality: u8,
    /// 透明图转到不支持 alpha 的格式（JPG）时合成的背景色，`#RRGGBB`。
    ///
    /// 不显式给颜色就会得到「透明像素底下原本的 RGB」，肉眼是随机黑底。
    #[serde(default = "default_alpha_background")]
    pub alpha_background: String,
}

fn default_jpeg_quality() -> u8 {
    DEFAULT_JPEG_QUALITY
}

fn default_alpha_background() -> String {
    DEFAULT_ALPHA_BACKGROUND.to_string()
}

impl Default for ImageOptions {
    fn default() -> Self {
        Self {
            jpeg_quality: DEFAULT_JPEG_QUALITY,
            alpha_background: DEFAULT_ALPHA_BACKGROUND.to_string(),
        }
    }
}

impl ImageOptions {
    /// 背景色解析成 RGB；非法写法回退到白色（绝不静默用随机值）。
    pub fn background_rgb(&self) -> [u8; 3] {
        parse_hex_rgb(&self.alpha_background).unwrap_or([255, 255, 255])
    }
}

/// `#RRGGBB` / `RRGGBB` / `#RGB` → RGB。
pub fn parse_hex_rgb(raw: &str) -> Option<[u8; 3]> {
    let hex = raw.trim().trim_start_matches('#');
    let expand = |s: &str| u8::from_str_radix(s, 16).ok();
    match hex.len() {
        6 => Some([
            expand(&hex[0..2])?,
            expand(&hex[2..4])?,
            expand(&hex[4..6])?,
        ]),
        3 => {
            let r = expand(&hex[0..1])?;
            let g = expand(&hex[1..2])?;
            let b = expand(&hex[2..3])?;
            Some([r * 17, g * 17, b * 17])
        }
        _ => None,
    }
}

/// 目标文件已存在时的策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConflictPolicy {
    /// 递增编号：`name (2).ext`（默认，Finder 习惯）。
    #[default]
    Rename,
    /// 覆盖同名**输出**文件。源文件永远不因这条策略被覆盖（见 `output.rs`）。
    Overwrite,
    /// 直接跳过这个文件。
    Skip,
}

/// 输出规则（§4.1）：位置、冲突、命名、体积筛选。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputRule {
    /// 输出目录；空串 = 与源文件同目录。
    #[serde(default)]
    pub dir: String,
    /// 冲突策略。
    #[serde(default)]
    pub conflict: ConflictPolicy,
    /// 文件名模板，支持 `{name}` `{ext}` `{preset}` `{date}` `{time}` `{index}`。
    #[serde(default = "default_name_template")]
    pub name_template: String,
    /// 只在结果比同名输出更小时落盘（`OnlyIfSmaller`）。
    #[serde(default)]
    pub only_if_smaller: bool,
}

fn default_name_template() -> String {
    DEFAULT_NAME_TEMPLATE.to_string()
}

impl Default for OutputRule {
    fn default() -> Self {
        Self {
            dir: String::new(),
            conflict: ConflictPolicy::Rename,
            name_template: DEFAULT_NAME_TEMPLATE.to_string(),
            only_if_smaller: false,
        }
    }
}

impl OutputRule {
    /// 输出目录（空串回退到源文件所在目录）。
    ///
    /// 界面上的目录都是原生面板选出的**结对路径**，但配置可以被手改、旧版本
    /// 还留有 `legacy_dest_dir`。相对路径不能原样交给 `create_dir_all`：
    /// 打包后的 App 工作目录是 `/`，`"output"` 会变成 `/output`（必然失败）。
    /// 所以相对路径按源文件所在目录展开，开头的 `~` 按家目录展开。
    pub fn dir_or_source(&self, source: &std::path::Path) -> std::path::PathBuf {
        let base = match source.parent() {
            Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
            _ => std::path::PathBuf::from("."),
        };
        let trimmed = self.dir.trim();
        if trimmed.is_empty() {
            return base;
        }
        if trimmed == "~" {
            return dirs::home_dir().unwrap_or(base);
        }
        if let Some(rest) = trimmed
            .strip_prefix("~/")
            // 中文输入法打出的全角波浪号。
            .or_else(|| trimmed.strip_prefix("\u{FF5E}/"))
        {
            if let Some(home) = dirs::home_dir() {
                return home.join(rest);
            }
        }
        let path = std::path::PathBuf::from(trimmed);
        if path.is_absolute() {
            path
        } else {
            base.join(path)
        }
    }
}

/// 一份完整配方。Job 启动时深拷贝，之后改配置不影响已排队的任务（§2.2）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversionPreset {
    /// 稳定 ID（圆环动作保存的就是它）。
    pub id: String,
    /// 展示名（环形扇区标签 / 设置页标题）。
    pub name: String,
    /// 目标规格。
    pub target: TargetSpec,
    /// 图像选项。
    #[serde(default)]
    pub image: ImageOptions,
    /// 输出规则。
    #[serde(default)]
    pub output: OutputRule,
    /// 源格式 == 目标格式时是否仍重编码（JPG 降质量这类「同格式任务」）。
    ///
    /// 默认 false：已经是目标格式的文件按「无需转换」跳过。置 true 后要求
    /// 路由图里存在该格式的显式 self edge，否则明确失败而不是假装成功。
    #[serde(default)]
    pub reencode_same_format: bool,
    /// 是否为内置预设（UI 用于区分「可删除」）。
    #[serde(default)]
    pub builtin: bool,
}

impl ConversionPreset {
    /// 目标格式（`SameAsSource` 时等于源格式）。
    pub fn target_format(&self, source: ConvertFormat) -> ConvertFormat {
        self.target.resolve(source)
    }

    /// 该文件在这个预设下是否属于「源格式就是目标格式、且不需要重编码」。
    pub fn is_already_target(&self, source: ConvertFormat) -> bool {
        self.target_format(source) == source && !self.reencode_same_format
    }

    /// 圆环标签：JPG 预设把质量写进名字，否则两个 JPG 扇区长一样。
    pub fn ring_label(&self) -> String {
        if self.target == TargetSpec::Explicit(ConvertFormat::Jpg) && self.name.trim().is_empty() {
            return format!("JPG {}%", self.image.jpeg_quality);
        }
        self.name.clone()
    }
}

/// 内置预设 ID（迁移时把旧的裸格式 id 映射到这里）。
pub const BUILTIN_IDS: [&str; 10] = [
    "img.jpg",
    "img.jpg.q70",
    "img.png",
    "img.webp",
    "img.gif",
    "img.bmp",
    "img.tiff",
    "doc.md",
    "doc.html",
    "doc.txt",
];

/// 旧的「裸格式 id」→ 内置预设 ID。
pub fn builtin_id_for_format(format: ConvertFormat) -> &'static str {
    match format {
        ConvertFormat::Png => "img.png",
        ConvertFormat::Jpg => "img.jpg",
        ConvertFormat::Webp => "img.webp",
        ConvertFormat::Gif => "img.gif",
        ConvertFormat::Bmp => "img.bmp",
        ConvertFormat::Tiff => "img.tiff",
        ConvertFormat::Md => "doc.md",
        ConvertFormat::Html => "doc.html",
        ConvertFormat::Txt => "doc.txt",
    }
}

/// 内置预设库：每个格式一条，外加一条「另一个 JPG 质量」示范同目标不同参数。
pub fn builtin_presets() -> Vec<ConversionPreset> {
    let mut out: Vec<ConversionPreset> = Vec::with_capacity(ALL_FORMATS.len() + 1);
    for format in ALL_FORMATS {
        out.push(ConversionPreset {
            id: builtin_id_for_format(format).to_string(),
            name: default_preset_name(format, DEFAULT_JPEG_QUALITY),
            target: TargetSpec::Explicit(format),
            image: ImageOptions::default(),
            output: OutputRule::default(),
            reencode_same_format: false,
            builtin: true,
        });
    }
    out.push(ConversionPreset {
        id: "img.jpg.q70".to_string(),
        name: default_preset_name(ConvertFormat::Jpg, 70),
        target: TargetSpec::Explicit(ConvertFormat::Jpg),
        image: ImageOptions {
            jpeg_quality: 70,
            ..ImageOptions::default()
        },
        output: OutputRule::default(),
        reencode_same_format: true,
        builtin: true,
    });
    out
}

/// 预设展示名：JPG 带质量，其余就是格式名。
pub fn default_preset_name(format: ConvertFormat, jpeg_quality: u8) -> String {
    if format == ConvertFormat::Jpg {
        format!("JPG {jpeg_quality}%")
    } else {
        format.label().to_string()
    }
}

/// 把任意字符串解析成预设 ID：先按预设 ID 找，找不到再按旧格式名兜底。
pub fn resolve_action_id(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if BUILTIN_IDS.contains(&trimmed) {
        return Some(trimmed.to_string());
    }
    ConvertFormat::from_ext(trimmed).map(|f| builtin_id_for_format(f).to_string())
}

/// 迁移用：按 ID 查预设，找不到就按旧格式名现造一条（配置里残留的旧动作不至于丢）。
pub fn find_or_legacy(presets: &[ConversionPreset], id: &str) -> Option<ConversionPreset> {
    if let Some(found) = presets.iter().find(|p| p.id == id) {
        return Some(found.clone());
    }
    let format = ConvertFormat::from_ext(id)?;
    let quality = DEFAULT_JPEG_QUALITY;
    Some(ConversionPreset {
        id: builtin_id_for_format(format).to_string(),
        name: default_preset_name(format, quality),
        target: TargetSpec::Explicit(format),
        image: ImageOptions::default(),
        output: OutputRule::default(),
        reencode_same_format: false,
        builtin: true,
    })
}

/// 默认正面顺序（与前端 `DEFAULT_FRONT` 对齐）。
///
/// 文档格式和图片格式**混排**：旧布局把 `doc.md` 放到背面，选一个 `.txt` 时正面
/// 只剩 HTML（`.txt` 本身是目标、图片动作对它无路由），盘上看着就像只剩一个动作。
/// 现在常见文档目标（MD/HTML/TXT）都在正面，背面只放少见图片格式。
pub fn default_front_actions() -> Vec<String> {
    ["img.jpg", "img.png", "doc.md", "doc.html", "doc.txt"]
        .into_iter()
        .map(str::to_string)
        .collect()
}

/// 默认背面顺序：少见图片格式，用 Tab / 滚轮翻面（空 = 不能翻面）。
pub fn default_back_actions() -> Vec<String> {
    ["img.webp", "img.gif", "img.bmp", "img.tiff"]
        .into_iter()
        .map(str::to_string)
        .collect()
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_spec_serializes_as_string() {
        let jpg = TargetSpec::Explicit(ConvertFormat::Jpg);
        assert_eq!(serde_json::to_string(&jpg).unwrap(), "\"jpg\"");
        assert_eq!(serde_json::to_string(&TargetSpec::SameAsSource).unwrap(), "\"*\"");
        let back: TargetSpec = serde_json::from_str("\"*\"")
            .map(|v: TargetSpec| v)
            .unwrap();
        assert_eq!(back, TargetSpec::SameAsSource);
    }

    #[test]
    fn preset_json_roundtrip() {
        let preset = ConversionPreset {
            id: "img.jpg".into(),
            name: "JPG 60%".into(),
            target: TargetSpec::Explicit(ConvertFormat::Jpg),
            image: ImageOptions {
                jpeg_quality: 60,
                alpha_background: "#102030".into(),
            },
            output: OutputRule {
                dir: "/tmp/out".into(),
                conflict: ConflictPolicy::Skip,
                name_template: "{name}-{preset}.{ext}".into(),
                only_if_smaller: true,
            },
            reencode_same_format: true,
            builtin: false,
        };
        let json = serde_json::to_string(&preset).unwrap();
        let back: ConversionPreset = serde_json::from_str(&json).unwrap();
        assert_eq!(back, preset);
        assert_eq!(back.image.background_rgb(), [0x10, 0x20, 0x30]);
    }

    #[test]
    fn missing_optional_fields_fall_back_to_defaults() {
        // 旧配置里没有 image / output：反序列化必须给出可用默认值。
        let preset: ConversionPreset =
            serde_json::from_str(r#"{"id":"img.png","name":"PNG","target":"png"}"#).unwrap();
        assert_eq!(preset.image.jpeg_quality, DEFAULT_JPEG_QUALITY);
        assert_eq!(preset.image.alpha_background, DEFAULT_ALPHA_BACKGROUND);
        assert_eq!(preset.output.conflict, ConflictPolicy::Rename);
        assert_eq!(preset.output.name_template, DEFAULT_NAME_TEMPLATE);
        assert!(!preset.reencode_same_format);
    }

    #[test]
    fn two_jpg_presets_do_not_share_quality() {
        let presets = builtin_presets();
        let a = presets.iter().find(|p| p.id == "img.jpg").unwrap();
        let b = presets.iter().find(|p| p.id == "img.jpg.q70").unwrap();
        assert_eq!(a.image.jpeg_quality, DEFAULT_JPEG_QUALITY);
        assert_eq!(b.image.jpeg_quality, 70);
        // 只有一个声明了「同格式也重编码」，另一个对 JPG 输入应跳过。
        assert!(!a.reencode_same_format);
        assert!(b.reencode_same_format);
    }

    #[test]
    fn legacy_format_ids_migrate_to_preset_ids() {
        assert_eq!(resolve_action_id("jpg").as_deref(), Some("img.jpg"));
        assert_eq!(resolve_action_id("html").as_deref(), Some("doc.html"));
        // 已经是预设 ID 的原样保留。
        assert_eq!(resolve_action_id("img.jpg.q70").as_deref(), Some("img.jpg.q70"));
        assert_eq!(resolve_action_id("nope"), None);
        assert_eq!(resolve_action_id("  "), None);
    }

    #[test]
    fn dir_or_source_expands_relative_and_tilde() {
        let rule = |dir: &str| OutputRule {
            dir: dir.to_string(),
            ..OutputRule::default()
        };
        let source = std::path::Path::new("/tmp/in/photo.png");

        // 空串（以及只有空白）回落到源文件所在目录。
        assert_eq!(rule("").dir_or_source(source), std::path::PathBuf::from("/tmp/in"));
        assert_eq!(
            rule("   ").dir_or_source(source),
            std::path::PathBuf::from("/tmp/in")
        );

        // 绝对路径原样使用，不做任何拼接。
        assert_eq!(
            rule("/Volumes/外置盘/out").dir_or_source(source),
            std::path::PathBuf::from("/Volumes/外置盘/out")
        );

        // 相对路径按源目录展开：打包 App 的 cwd 是 `/`，原样交给
        // create_dir_all 会写到 `/output`。
        assert_eq!(
            rule("out").dir_or_source(source),
            std::path::PathBuf::from("/tmp/in/out")
        );
        assert_eq!(
            rule("./out/").dir_or_source(source),
            std::path::PathBuf::from("/tmp/in/./out/")
        );

        // `~` 与全角 `～`（中文输入法）都按家目录展开。
        if let Some(home) = dirs::home_dir() {
            assert_eq!(rule("~").dir_or_source(source), home);
            assert_eq!(rule("~/Pictures").dir_or_source(source), home.join("Pictures"));
            assert_eq!(
                rule("\u{FF5E}/Pictures").dir_or_source(source),
                home.join("Pictures")
            );
        }

        // 源路径只有文件名（相对路径）时，父目录是空串，回落 `.`。
        assert_eq!(
            rule("").dir_or_source(std::path::Path::new("photo.png")),
            std::path::PathBuf::from(".")
        );
    }

    #[test]
    fn hex_colors_parse_and_fall_back_to_white() {
        assert_eq!(parse_hex_rgb("#FFFFFF"), Some([255, 255, 255]));
        assert_eq!(parse_hex_rgb("000000"), Some([0, 0, 0]));
        assert_eq!(parse_hex_rgb("#abc"), Some([170, 187, 204]));
        assert_eq!(parse_hex_rgb("zzz"), None);
        let opts = ImageOptions {
            alpha_background: "not-a-color".into(),
            ..ImageOptions::default()
        };
        assert_eq!(opts.background_rgb(), [255, 255, 255]);
    }
}
