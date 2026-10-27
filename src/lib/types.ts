// 与 Rust 后端 serde 契约对齐的类型定义（src-tauri/src 对应结构）。

export type ContentType = "text" | "html" | "image" | "files" | "unknown";

/** 主窗口出现位置，与 Rust 端 `board_position` 对齐。 */
export type BoardPosition = "follow_cursor" | "screen" | "bottom";

export type SyncState = "local" | "synced" | "dirty";

export type SensitivityPolicy = "ignore" | "mask" | "encrypt";

export type SyncProviderKind = "webdav" | "s3";

export type ModuleState = "stopped" | "starting" | "running" | "crashed";

export type ModuleRuntime = "iframe" | "sidecar" | "wasm";

export type Lifecycle = "demand" | "background";

export type Capability =
  | "clipboard_read"
  | "clipboard_write"
  | "fs_read_module"
  | "fs_read_user"
  | "fs_write_user"
  | "network";

export type ClipContent =
  | { kind: "text"; value: string }
  | { kind: "html"; value: string }
  | { kind: "image"; value: { path: string; width: number; height: number } }
  | { kind: "files"; value: string[] };

export interface ClipItem {
  id: number;
  content_type: ContentType;
  content: ClipContent;
  preview: string;
  source_app: string | null;
  /** 来源 App 图标 PNG 文件绝对路径（导出失败/非 macOS 为 null）。 */
  source_app_icon: string | null;
  created_at: number;
  last_used_at: number;
  pinned: boolean;
  favorite: boolean;
  sensitive: boolean;
  size_bytes: number;
  hash: string;
  sync_state: SyncState;
  /** 分组标签（null=未分组）。 */
  tag: string | null;
}

export interface HistoryQuery {
  keyword: string | null;
  content_type: ContentType | null;
  pinned_only: boolean;
  favorite_only: boolean;
  /** 分组标签筛选：null=不筛选。 */
  tag: string | null;
  limit: number;
  offset: number;
}

export type PasteOutcome =
  | { status: "pasted" }
  | { status: "copied_only"; reason: string };

/** 「发送全部」中单条的粘贴结果（对应 Rust `StackPasteItemResult`）。 */
export interface StackPasteItemResult {
  id: number;
  ok: boolean;
  error?: string;
}

/** 「发送全部」汇总（对应 Rust `StackPasteAllResult`）。 */
export interface StackPasteAllResult {
  total: number;
  pasted: number;
  failed: number;
  items: StackPasteItemResult[];
}

export interface HistoryStats {
  total: number;
  total_bytes: number;
  /** 按内容类型计数（不受当前筛选影响，用于过滤标签计数徽标）。 */
  by_type: Record<string, number>;
  /** 已收藏条数。 */
  favorite: number;
}

/** 快捷回复分组。 */
export interface ReplyGroup {
  id: number;
  name: string;
  sort: number;
}

/** 快捷回复模板（`files` 为附件文件绝对路径列表）。 */
export interface Reply {
  id: number;
  group_id: number;
  title: string;
  content: string;
  files: string[];
  sort: number;
  /** 创建时间（毫秒时间戳）。 */
  created_at: number;
}

/** Excel 导入汇总（对应 Rust `ImportSummary`）。 */
export interface ImportSummary {
  imported: number;
  groups_created: number;
  errors: string[];
}

export interface SyncCredentials {
  url: string;
  username: string;
  password: string;
}

export interface SyncConfig {
  enabled: boolean;
  provider: SyncProviderKind;
  credentials: SyncCredentials;
  encryption: boolean;
  passphrase: string;
  remote_file: string;
  interval_secs: number;
}

export interface HotkeyConfig {
  toggle: string;
  stack_paste: string;
  /** 唤起快捷回复选择器（现场回贴，不经过主窗口）。 */
  reply_picker: string;
}

/** Profile 类型：决定选盘优先级（与 Rust `RingProfileKind` 对齐）。 */
export type RingProfileKind = "normal" | "mixed_fallback";

/**
 * 文件类型 Profile：一种文件类型该用哪几条预设。
 *
 * 三层模型：预设（怎么转）→ Profile（什么文件用哪些预设）→ 扇区（本批能不能用）。
 * 迁移后与 Rust `RingProfile` 字段一一对应。
 */
export interface RingProfile {
  /** 稳定 id（默认 `images` / `text` / `mixed`，自建为 `profile-*`）。 */
  id: string;
  /** 展示名（设置页可改）。 */
  name: string;
  /** 归属本 Profile 的扩展名（小写、不带点）；空 + 兜底 = catch-all。 */
  extensions: string[];
  kind: RingProfileKind;
  /** 唯一一面的预设顺序：索引 0 在 12 点，顺时针，最多 12 条。 */
  preset_ids: string[];
}

/** 环形转换配置（`设置 → 环形转换`，与 Rust `RingConfig` 对齐）。 */
export interface RingConfig {
  /** 文件类型 Profile（顺序即匹配优先级）。 */
  profiles: RingProfile[];
  /** Finder 里 Option + 拖动文件唤起环形盘（仅 macOS，只在前台是 Finder 时生效）。 */
  option_tap: boolean;
  /** 按类别记住的最后动作（[类别, 预设 id]），只用于初始高亮。 */
  last_action: [string, string][];
}

/** 单个环形扇区（Rust `RingAction`）。 */
export interface RingAction {
  /** 动作 id = 预设 id。 */
  id: string;
  /** 展示名（预设名，JPG 带质量）。 */
  label: string;
  /** 目标格式；null = 与源格式相同（同格式重编码预设）。 */
  target: ConvertFormat | null;
  /** 将实际转换的文件数。 */
  convert: number;
  /** 不会转换的文件数（已是目标格式 / 无路由）。 */
  skip: number;
  /** 本批文件下能否执行；false 仍占原位置，只降低不透明度并显示原因。 */
  enabled: boolean;
  /** 禁用原因（enabled 为 true 时是 null）。 */
  disabled_reason: string | null;
}

/** 本次会话选中的文件类型 Profile（Rust `ProfileInfo`）。 */
export interface RingProfileInfo {
  id: string;
  name: string;
  /** 归属本 Profile 的扩展名（空 = catch-all）。 */
  extensions: string[];
  /** 是否是混合兜底（中心标注「其他文件」用）。 */
  fallback: boolean;
}

/** 环形会话快照（Rust `RingSnapshot`，ring 窗口据此渲染）。 */
export interface RingSnapshot {
  generation: number;
  /** idle/capturing/probing/ready/empty/converting。 */
  state: string;
  /** 类别标签：图片 / 文本 / 混合文件。 */
  category: string | null;
  count: number;
  hint: string | null;
  /** 本批文件选中的 Profile；没有 Profile 时为 null。 */
  profile: RingProfileInfo | null;
  /** 唯一一面的扇区（位置固定，禁用项也在这里）。 */
  actions: RingAction[];
  /** 按类别记住的最后动作：仅用于预高亮，不自动执行。 */
  suggested: string | null;
}

/** 环形手势现场状态（Rust `GestureStatus`，设置页据此解释「为什么没反应」）。 */
export interface GestureStatus {
  /** 设置里是否开启此手势。 */
  enabled: boolean;
  /** 系统「辅助功能」是否已授权。 */
  accessibility: boolean;
  /** 事件监听是否真的建好（授权了但没起来时与 accessibility 不一致）。 */
  tapAlive: boolean;
  /** 当前前台是否满足条件（Finder 或 ClipBase 自己）。 */
  frontmostOk: boolean;
  /** 环形会话状态（idle/capturing/ready/converting）。 */
  state: string;
  /** 最近一次「没反应」的原因；接管成功后为 null。 */
  lastNote: string | null;
}

/** 「修复授权」的结果（Rust `AxRepair`）。 */
export interface AxRepair {
  /** `tccutil reset` 是否成功。 */
  reset: boolean;
  /** 重置后复查到的授权状态（一般仍为 false，等用户勾选）。 */
  trusted: boolean;
  /** 给用户看的一句话。 */
  detail: string;
}

/** 转换进度（Rust `RingProgressEvent`，逐文件）。 */
export interface RingProgress {
  generation: number;
  done: number;
  total: number;
}

/** 转换结果（Rust `RingResultEvent`，原地成功/失败反馈）。 */
export interface RingResult {
  generation: number;
  converted: number;
  skipped: number;
  failed: number;
  /** 被取消、未完成的文件数。 */
  cancelled: number;
  errors: string[];
  /** 转成功但不保真的地方（动画、ICC、EXIF 等），不能静默丢掉。 */
  warnings: string[];
  output: string | null;
}

export interface Config {
  max_items: number;
  max_bytes: number;
  monitor_enabled: boolean;
  poll_interval_ms: number;
  sensitivity: SensitivityPolicy;
  hotkeys: HotkeyConfig;
  sync: SyncConfig;
  launch_at_login: boolean;
  theme: string;
  paste_as_plain: boolean;
  paste_trim_trailing: boolean;
  confirm_on_delete: boolean;
  auto_close_after_paste: boolean;
  /** 单击卡片是否直接回贴（false = 单击仅复制，双击/Enter 回贴）。 */
  paste_on_single_click: boolean;
  sound_enabled: boolean;
  num_hotkey_enabled: boolean;
  stack_enabled: boolean;
  board_style: string;
  board_height_scale: number;
  board_position: BoardPosition;
  text_preview: string;
  history_ttl_days: number;
  app_blacklist: string[];
  /** 环形转换配置（设置 → 环形转换）。 */
  ring: RingConfig;
  /** 转换预设库（环形动作引用的配方都在这里）。 */
  conversion: ConversionConfig;
  /** 上次停留的设置分区（主窗口默认打开此分区）。 */
  last_settings_section: string;
}

export interface SyncStatus {
  enabled: boolean;
  state: string;
  last_sync_at: number | null;
  last_error: string | null;
  item_count: number;
}

export interface ModuleEntry {
  backend: string | null;
  ui: string | null;
}

export type Contribution =
  | { type: "tray"; label: string; icon: string | null }
  | { type: "clipboard_action"; label: string; mime: string[] }
  | { type: "shortcut"; label: string; default: string }
  | { type: "settings"; label: string }
  | { type: "command"; id: string; label: string };

export interface ModuleManifest {
  schema: number;
  id: string;
  name: string;
  version: string;
  min_core: string;
  runtime: ModuleRuntime;
  entry: ModuleEntry;
  permissions: Capability[];
  contributions: Contribution[];
  lifecycle: Lifecycle;
}

export interface ModuleRecord {
  id: string;
  name: string;
  version: string;
  enabled: boolean;
  grants: Capability[];
  installed_at: number;
  state: ModuleState;
  manifest: ModuleManifest;
}

export interface ContributionEntry {
  module_id: string;
  module_name: string;
  contribution: Contribution;
}

/** Finder Sync 扩展状态。 */
export interface FinderIntegrationStatus {
  /** 当前平台是否支持（仅 macOS）。 */
  supported: boolean;
  /** 系统扩展数据库里是否已有 ClipBase Finder 扩展。 */
  installed: boolean;
  /** 扩展当前是否启用。 */
  enabled: boolean;
  /** 扩展的 bundle id。 */
  bundle_id: string;
  /** 当前 App 包里是否带有 .appex（开发构建通常没有）。 */
  appex_present: boolean;
}

/** 转换支持的格式（与 Rust `ConvertFormat` 对齐）。 */
export type ConvertFormat =
  | "png"
  | "jpg"
  | "webp"
  | "gif"
  | "bmp"
  | "tiff"
  | "md"
  | "html"
  | "txt";

export const CONVERT_FORMAT_LABELS: Record<ConvertFormat, string> = {
  png: "PNG 图片",
  jpg: "JPG 图片",
  webp: "WebP 图片",
  gif: "GIF 图片",
  bmp: "BMP 图片",
  tiff: "TIFF 图片",
  md: "Markdown",
  html: "HTML",
  txt: "纯文本",
};

/** 目标规格：显式格式，或 `"*"` = 与源格式相同（Rust `TargetSpec`）。 */
export type TargetSpec = ConvertFormat | "*";

/** 目标文件已存在时的策略（Rust `ConflictPolicy`）。 */
export type ConflictPolicy = "rename" | "overwrite" | "skip";

/** 图像选项（Rust `ImageOptions`）。 */
export interface ImageOptions {
  /** JPEG 输出质量 1-100（仅目标为 JPG 的路由生效）。 */
  jpeg_quality: number;
  /** 透明图转 JPG 时的合成背景色 `#RRGGBB`。 */
  alpha_background: string;
}

/** 输出规则（Rust `OutputRule`，规范 §4）。 */
export interface OutputRule {
  /** 输出目录；空串 = 与源文件同目录。 */
  dir: string;
  conflict: ConflictPolicy;
  /** 文件名模板，支持 `{name}` `{ext}` `{preset}` `{date}` `{time}` `{index}`。 */
  name_template: string;
  /** 只在结果更小时落盘。 */
  only_if_smaller: boolean;
}

/**
 * 转换预设：一份完整配方（Rust `ConversionPreset`）。
 *
 * 环形动作保存的是 `id`，因此两个 JPG 动作可以有不同质量。
 */
export interface ConversionPreset {
  id: string;
  name: string;
  target: TargetSpec;
  image: ImageOptions;
  output: OutputRule;
  /** 源格式 == 目标格式时仍重编码（JPG 降质量这类任务）。 */
  reencode_same_format: boolean;
  /** 内置预设（可以改，但不能删）。 */
  builtin: boolean;
}

/** 转换预设库（Rust `ConversionConfig`）。 */
export interface ConversionConfig {
  presets: ConversionPreset[];
}

/** 单文件转换状态（Rust `FileState`）。 */
export type ConversionFileState =
  | "pending"
  | "running"
  | "done"
  | "skipped"
  | "failed"
  | "cancelled";

/** 逐文件结果（Rust `ConversionFileResult`）。 */
export interface ConversionFileResult {
  source: string;
  detected_format: string | null;
  target_format: string | null;
  state: ConversionFileState;
  progress: number;
  outputs: string[];
  skip: { code: string; detail: string } | null;
  error: { code: string; message: string } | null;
  /** 转换成功但不保真的地方（动画只剩首帧、ICC 未保留……）。 */
  warnings: string[];
  /** 实际走过的格式链。 */
  route: string[];
}

/** 一个转换任务（Rust `ConversionJob`）。 */
export interface ConversionJob {
  id: string;
  preset: ConversionPreset;
  sources: string[];
  state: "queued" | "running" | "completed" | "completed_with_errors" | "failed" | "cancelled";
  files: ConversionFileResult[];
  created_at: number;
}

export const CAPABILITY_LABELS: Record<Capability, string> = {
  clipboard_read: "读取剪贴板",
  clipboard_write: "写入剪贴板",
  fs_read_module: "读取模块自身目录",
  fs_read_user: "读取用户目录",
  fs_write_user: "写入用户目录",
  network: "网络访问",
};
