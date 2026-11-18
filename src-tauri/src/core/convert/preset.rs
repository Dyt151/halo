//! 转换预设（Preset）：动作身份不是目标格式字符串，而是一份完整配方。
//!
//! 对照 README「三、环形文件转换」：
//! - `target` 只是配方里的一项，另外还有图像选项和输出规则；
//! - 因此「两个 JPG 动作」可以有不同的质量和命名，互不影响；
//! - `SameAsSource` 用 `"*"` 表示「目标按每个源文件自己的格式解析」，
//!   它不是「无需转换」：JPG→JPG 降质量、PNG→PNG 重编码都是合法任务。
//!
//! 只放当前真实支持的字段（Filee 行为 约束：不先做空壳 UI）。

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

/// 输出规则（Filee 行为）：位置、冲突、命名、体积筛选。
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

/// 一份完整配方。Job 启动时深拷贝，之后改配置不影响已排队的任务（Filee 行为）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversionPreset {
    /// 稳定 ID（圆环动作保存的就是它）。
    pub id: String,
    /// 展示名（环形扇区标签 / 设置页标题）。
    pub name: String,
    /// 动作类型：格式转换（默认），或打包 / 解压 / PDF 分页这类**非格式动作**。
    ///
    /// 旧配置没有这个字段，serde 默认落到 `Convert`，无需迁移。
    #[serde(default)]
    pub kind: PresetKind,
    /// 目标规格。`PresetKind::Convert` 之外的动作不读它（占位 `SameAsSource`）。
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

/// 动作类型：`Convert` 是格式到格式的转换，其余是**整批 / 多输出**的工具动作。
///
/// 工具动作不进路由图：可用性与执行都在 `job.rs` 的动作管线上单独判定。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PresetKind {
    /// 格式转换（默认）。
    Convert,
    /// 打包：整批文件 → 一个压缩包。
    Pack { archive: ArchiveFormat },
    /// 解压：每个压缩包 → 一个文件夹（ZIP / TAR / TAR.GZ）。
    Extract,
    /// PDF 每页渲染成一张图（多输出）。
    PdfPages { to: ConvertFormat },
    /// PDF → CBZ 漫画包（每页一张图打进 zip）。
    PdfToCbz,
}

impl Default for PresetKind {
    fn default() -> Self {
        Self::Convert
    }
}

impl PresetKind {
    /// 是否为「格式转换」（走路由管线）。
    pub fn is_convert(&self) -> bool {
        matches!(self, Self::Convert)
    }
}

/// 打包 / 解压支持的压缩格式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArchiveFormat {
    Zip,
    TarGz,
    Tar,
}

impl ArchiveFormat {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Zip => "zip",
            Self::TarGz => "tar.gz",
            Self::Tar => "tar",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Zip => "ZIP",
            Self::TarGz => "TAR.GZ",
            Self::Tar => "TAR",
        }
    }

    /// 按文件扩展名识别压缩包（`tar.gz` 看最后两段）。
    pub fn from_path(path: &str) -> Option<Self> {
        let lower = path.to_ascii_lowercase();
        let name = std::path::Path::new(&lower);
        let ext = name.extension()?.to_str()?;
        let stem_ext = name
            .file_stem()
            .and_then(|s| std::path::Path::new(s).extension())
            .and_then(|e| e.to_str());
        match ext {
            "zip" => Some(Self::Zip),
            "tar" => Some(Self::Tar),
            // `.tgz` 本身就是 tar.gz 的别名；`.gz` 只有前面还有 `.tar` 才算。
            "tgz" => Some(Self::TarGz),
            "gz" if stem_ext == Some("tar") => Some(Self::TarGz),
            _ => None,
        }
    }
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
        if !self.kind.is_convert() {
            return self.name.clone();
        }
        if self.target == TargetSpec::Explicit(ConvertFormat::Jpg) && self.name.trim().is_empty() {
            return format!("JPG {}%", self.image.jpeg_quality);
        }
        self.name.clone()
    }
}

/// 内置预设 ID（迁移时把旧的裸格式 id 映射到这里）。
///
/// 本版只保留实际用得到的 11 种格式：ico/qoi/tga/pnm/jp2/avif/md/doc/odt/rtf/mhtml
/// 的动作与预设已从源码删除，旧配置里指向它们的条目由 `Config::migrate` 清掉。
pub const BUILTIN_IDS: [&str; 19] = [
    "img.jpg",
    "img.png",
    "img.webp",
    "img.gif",
    "img.bmp",
    "img.tiff",
    "img.heic",
    "img.pdf",
    "doc.html",
    "doc.txt",
    "doc.docx",
    "doc.pdf",
    "act.pack.zip",
    "act.pack.tar.gz",
    "act.pack.tar",
    "act.extract",
    "act.pdf.pages.png",
    "act.pdf.pages.jpg",
    "act.pdf.cbz",
];

/// 上一版（全部格式时代）就存在、本版继续提供的内置预设 id。
///
/// 用户预设库里缺席这些 id 视为「用户删过」，迁移不复活；只有**本版新增**的
/// 内置 id 缺席才当成「版本升级」，由 `Config::migrate` 补进预设库。
pub const LEGACY_BUILTIN_IDS: [&str; 12] = [
    "img.jpg",
    "img.png",
    "img.webp",
    "img.gif",
    "img.bmp",
    "img.tiff",
    "img.heic",
    "img.pdf",
    "doc.html",
    "doc.txt",
    "doc.docx",
    "doc.pdf",
];

/// 工具动作内置 id（打包 / 解压 / PDF 分页），环形盘按固定顺序追加在正面末尾。
pub const ACTION_BUILTIN_IDS: [&str; 7] = [
    "act.pack.zip",
    "act.pack.tar.gz",
    "act.pack.tar",
    "act.extract",
    "act.pdf.pages.png",
    "act.pdf.pages.jpg",
    "act.pdf.cbz",
];

/// 旧的「裸格式 id」→ 内置预设 ID。
pub fn builtin_id_for_format(format: ConvertFormat) -> Option<&'static str> {
    match format {
        ConvertFormat::Png => Some("img.png"),
        ConvertFormat::Jpg => Some("img.jpg"),
        ConvertFormat::Webp => Some("img.webp"),
        ConvertFormat::Gif => Some("img.gif"),
        ConvertFormat::Bmp => Some("img.bmp"),
        ConvertFormat::Tiff => Some("img.tiff"),
        ConvertFormat::Heic => Some("img.heic"),
        ConvertFormat::Html => Some("doc.html"),
        ConvertFormat::Txt => Some("doc.txt"),
        ConvertFormat::Pdf => Some("doc.pdf"),
        ConvertFormat::Docx => Some("doc.docx"),
    }
}

/// 内置预设库：每个可产出格式一条，外加「图片 → PDF」与打包 / 解压 / PDF 分页
/// 等工具动作。
pub fn builtin_presets() -> Vec<ConversionPreset> {
    let mut out: Vec<ConversionPreset> = Vec::with_capacity(ALL_FORMATS.len() + 8);
    for format in ALL_FORMATS {
        let Some(id) = builtin_id_for_format(format) else {
            continue;
        };
        out.push(ConversionPreset {
            id: id.to_string(),
            name: default_preset_name(format, DEFAULT_JPEG_QUALITY),
            kind: PresetKind::Convert,
            target: TargetSpec::Explicit(format),
            image: ImageOptions::default(),
            output: OutputRule::default(),
            reencode_same_format: false,
            builtin: true,
        });
    }
    out.push(ConversionPreset {
        id: "img.pdf".to_string(),
        name: default_preset_name(ConvertFormat::Pdf, DEFAULT_JPEG_QUALITY),
        kind: PresetKind::Convert,
        target: TargetSpec::Explicit(ConvertFormat::Pdf),
        image: ImageOptions::default(),
        output: OutputRule::default(),
        reencode_same_format: false,
        builtin: true,
    });
    for action in builtin_actions() {
        out.push(action);
    }
    out
}

/// 工具动作内置预设（固定顺序，环形盘把它们追加在每张盘的正面末尾）。
pub fn builtin_actions() -> Vec<ConversionPreset> {
    let cases: [(&str, &str, PresetKind); 7] = [
        ("act.pack.zip", "打包 ZIP", PresetKind::Pack { archive: ArchiveFormat::Zip }),
        (
            "act.pack.tar.gz",
            "打包 TAR.GZ",
            PresetKind::Pack { archive: ArchiveFormat::TarGz },
        ),
        ("act.pack.tar", "打包 TAR", PresetKind::Pack { archive: ArchiveFormat::Tar }),
        ("act.extract", "解压压缩包", PresetKind::Extract),
        ("act.pdf.pages.png", "PDF → PNG", PresetKind::PdfPages { to: ConvertFormat::Png }),
        ("act.pdf.pages.jpg", "PDF → JPG", PresetKind::PdfPages { to: ConvertFormat::Jpg }),
        ("act.pdf.cbz", "PDF → CBZ", PresetKind::PdfToCbz),
    ];
    cases
        .into_iter()
        .map(|(id, name, kind)| ConversionPreset {
            id: id.to_string(),
            name: name.to_string(),
            kind,
            // 动作不读 target，占位即可（serde 需要非缺省字段）。
            target: TargetSpec::SameAsSource,
            image: ImageOptions::default(),
            output: OutputRule::default(),
            reencode_same_format: false,
            builtin: true,
        })
        .collect()
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
    ConvertFormat::from_ext(trimmed).and_then(|f| builtin_id_for_format(f).map(str::to_string))
}

/// 迁移用：按 ID 查预设，找不到就按旧格式名现造一条（配置里残留的旧动作不至于丢）。
pub fn find_or_legacy(presets: &[ConversionPreset], id: &str) -> Option<ConversionPreset> {
    if let Some(found) = presets.iter().find(|p| p.id == id) {
        return Some(found.clone());
    }
    let format = ConvertFormat::from_ext(id)?;
    let preset_id = builtin_id_for_format(format)?;
    let quality = DEFAULT_JPEG_QUALITY;
    Some(ConversionPreset {
        id: preset_id.to_string(),
        name: default_preset_name(format, quality),
        kind: PresetKind::Convert,
        target: TargetSpec::Explicit(format),
        image: ImageOptions::default(),
        output: OutputRule::default(),
        reencode_same_format: false,
        builtin: true,
    })
}

/// 默认图片盘正面：常用图片格式 + PDF（图片 → PDF）。
pub const DEFAULT_IMAGES: [&str; 6] = [
    "img.jpg",
    "img.png",
    "img.webp",
    "img.gif",
    "img.heic",
    "img.pdf",
];

/// 默认文本盘正面：文本互转。
pub const DEFAULT_TEXT: [&str; 2] = ["doc.html", "doc.txt"];

/// 默认文档盘正面（PDF / Word 文件弹这张盘）。
pub const DEFAULT_DOCS: [&str; 6] = [
    "img.png",
    "img.jpg",
    "doc.txt",
    "doc.html",
    "doc.docx",
    "doc.pdf",
];

/// 默认兜底盘（catch-all）正面：图片目标在前，文本目标补位。
pub fn default_mixed_actions() -> Vec<String> {
    [
        "img.jpg",
        "img.png",
        "img.webp",
        "img.gif",
        "img.bmp",
        "img.tiff",
        "doc.html",
        "doc.txt",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

/// 旧「正面 + 背面」合并后的默认顺序（迁移 / 兜底重置用）。
///
/// 背面已取消：所有扇区都在这一个列表里，按类别拆进图片 / 文本 / 文档三张盘。
pub fn default_front_actions() -> Vec<String> {
    let mut merged: Vec<String> = DEFAULT_IMAGES.iter().map(|s| s.to_string()).collect();
    for id in DEFAULT_TEXT.iter().chain(DEFAULT_DOCS.iter()) {
        if !merged.contains(&id.to_string()) {
            merged.push(id.to_string());
        }
    }
    merged
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
            kind: PresetKind::Convert,
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
        assert_eq!(preset.kind, PresetKind::Convert);
    }

    #[test]
    fn action_presets_roundtrip_and_detect_archives() {
        let pack: ConversionPreset = serde_json::from_str(
            r#"{"id":"act.pack.zip","name":"打包 ZIP","kind":{"pack":{"archive":"zip"}},"target":"*"}"#,
        )
        .unwrap();
        assert_eq!(
            pack.kind,
            PresetKind::Pack { archive: ArchiveFormat::Zip }
        );
        assert_eq!(pack.ring_label(), "打包 ZIP");

        let pages: ConversionPreset = serde_json::from_str(
            r#"{"id":"act.pdf.pages.png","name":"PDF → PNG","kind":{"pdf_pages":{"to":"png"}},"target":"*"}"#,
        )
        .unwrap();
        assert_eq!(
            pages.kind,
            PresetKind::PdfPages { to: ConvertFormat::Png }
        );

        // 动作 id 都是内置 id，能解析回来。
        for id in ACTION_BUILTIN_IDS {
            assert_eq!(resolve_action_id(id).as_deref(), Some(id));
        }
        let actions = builtin_actions();
        assert_eq!(actions.len(), ACTION_BUILTIN_IDS.len());
        assert!(actions.iter().all(|a| !a.kind.is_convert()));

        // 压缩包扩展名识别（tar.gz 看两段）。
        assert_eq!(ArchiveFormat::from_path("/a/b.zip"), Some(ArchiveFormat::Zip));
        assert_eq!(ArchiveFormat::from_path("/a/b.TAR"), Some(ArchiveFormat::Tar));
        assert_eq!(ArchiveFormat::from_path("/a/b.tar.gz"), Some(ArchiveFormat::TarGz));
        assert_eq!(ArchiveFormat::from_path("/a/b.tgz"), Some(ArchiveFormat::TarGz));
        assert_eq!(ArchiveFormat::from_path("/a/b.gz"), None);
        assert_eq!(ArchiveFormat::from_path("/a/b.7z"), None);
        assert_eq!(ArchiveFormat::from_path("/a/b"), None);
    }

    #[test]
    fn builtin_library_covers_kept_formats_and_actions() {
        let presets = builtin_presets();
        let ids: Vec<&str> = presets.iter().map(|p| p.id.as_str()).collect();
        for id in BUILTIN_IDS {
            assert!(ids.contains(&id), "缺少内置预设 {id}");
        }
        assert_eq!(presets.len(), BUILTIN_IDS.len());
        // 已删除的格式不再有内置预设。
        for gone in ["img.ico", "img.avif", "doc.md", "doc.odt", "img.jpg.q70"] {
            assert!(!ids.contains(&gone), "{gone} 不该再出现");
        }
        // 图片盘默认内容都是真实存在的预设。
        for id in DEFAULT_IMAGES.iter().chain(DEFAULT_TEXT.iter()).chain(DEFAULT_DOCS.iter()) {
            assert!(ids.contains(id), "默认盘引用了不存在的预设 {id}");
        }
        for id in default_mixed_actions() {
            assert!(ids.contains(&id.as_str()), "兜底盘引用了不存在的预设 {id}");
        }
    }

    #[test]
    fn legacy_format_ids_migrate_to_preset_ids() {
        assert_eq!(resolve_action_id("jpg").as_deref(), Some("img.jpg"));
        assert_eq!(resolve_action_id("html").as_deref(), Some("doc.html"));
        // 已经是预设 ID 的原样保留。
        assert_eq!(resolve_action_id("img.png").as_deref(), Some("img.png"));
        // 已删除格式不再能解析出预设。
        assert_eq!(resolve_action_id("md"), None);
        assert_eq!(resolve_action_id("ico"), None);
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
