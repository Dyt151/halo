// 与 Rust 后端 serde 契约对齐的类型定义（src-tauri/src 对应结构）。

export type ContentType = "text" | "html" | "image" | "files" | "unknown";

/** 主窗口出现位置，与 Rust 端 `board_position` 对齐。 */
export type BoardPosition = "follow_cursor" | "screen" | "bottom";

export type SyncState = "local" | "synced" | "dirty";

export type SensitivityPolicy = "flag" | "ignore" | "mask" | "encrypt";

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
  /** 语音指令条：弹出的同时把光标放进输入框，听写落在里面。 */
  voice_cmd: string;
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
  /** 预设顺序：索引 0 在 12 点，顺时针，最多 16 条（不含工具动作）。 */
  preset_ids: string[];
  /**
   * 【已废弃】旧版背面预设顺序：背面已取消，Rust 迁移会并入 `preset_ids`
   * 并清空。保留字段只为旧配置能反序列化，前端不再读写。
   */
  back_ids?: string[];
}

/** 环形转换配置（`设置 → 环形转换`，与 Rust `RingConfig` 对齐）。 */
export interface RingConfig {
  /** 文件类型 Profile（顺序即匹配优先级）。 */
  profiles: RingProfile[];
  /** Finder 里 Option + 拖动文件唤起环形盘（仅 macOS，只在前台是 Finder 时生效）。 */
  option_tap: boolean;
  /** 每张盘末尾是否追加打包 / 解压 / PDF 分页等工具动作。 */
  utility_actions: boolean;
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
  /** 当前显示的扇区（位置固定，禁用项也在这里）。 */
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
  /** 当前前台是否满足条件（Finder 或 Halo 自己）。 */
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
  /** 状态栏图标与下拉面板。 */
  statusbar: StatusBarConfig;
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
  /** 系统扩展数据库里是否已有 Halo Finder 扩展。 */
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
  | "heic"
  | "html"
  | "txt"
  | "pdf"
  | "docx";

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

/** 输出规则（Rust `OutputRule`，Filee 行为）。 */
export interface OutputRule {
  /** 输出目录；空串 = 与源文件同目录。 */
  dir: string;
  conflict: ConflictPolicy;
  /** 文件名模板，支持 `{name}` `{ext}` `{preset}` `{date}` `{time}` `{index}`。 */
  name_template: string;
  /** 只在结果更小时落盘。 */
  only_if_smaller: boolean;
}

/** 压缩包格式（Rust `ArchiveFormat`）。 */
export type ArchiveFormat = "zip" | "tar_gz" | "tar";

/**
 * 预设类型（Rust `PresetKind`）：`convert` 走路由图；其余是工具动作，
 * 产物是一个压缩包 / 一批分页图。
 */
export type PresetKind =
  | "convert"
  | { pack: { archive: ArchiveFormat } }
  | "extract"
  | { pdf_pages: { to: ConvertFormat } }
  | "pdf_to_cbz";

/**
 * 转换预设：一份完整配方（Rust `ConversionPreset`）。
 *
 * 环形动作保存的是 `id`，因此两个 JPG 动作可以有不同质量。
 */
export interface ConversionPreset {
  id: string;
  name: string;
  /** 预设类型（缺省 = 格式转换，旧配置反序列化时 Rust 会补默认值）。 */
  kind?: PresetKind;
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

/** 状态栏面板模块 id（与 Rust `STATUSBAR_SECTIONS` 对齐）。 */
export type StatusBarSection =
  | "battery"
  | "volume"
  | "audio_input"
  | "input_source"
  | "network"
  | "vpn"
  | "bluetooth";

/** 菜单栏图标中心图形（与 Rust `GLYPH_CENTERS` 对齐）。 */
export type GlyphCenter = "auto" | "wifi" | "bluetooth" | "clipboard";

/** 音量区的画法（与 Rust `GLYPH_VOLUME_STYLES` 对齐）。 */
export type GlyphVolumeStyle = "arc" | "dots";

/** 彩色图标的墨色取向（与 Rust `GLYPH_APPEARANCES` 对齐）。 */
export type GlyphAppearance = "auto" | "light" | "dark";

/** 状态栏图标配置（`设置 → 状态栏图标`，与 Rust `StatusBarConfig` 对齐）。 */
export interface StatusBarConfig {
  /** 显示菜单栏图标。 */
  enabled: boolean;
  /** 面板显示的模块及其顺序。 */
  sections: StatusBarSection[];
  /** 图标中心画什么。 */
  glyph_center: GlyphCenter;
  /** 外环画电量。 */
  glyph_battery_ring: boolean;
  /** 环顶缺口里写电量数字。 */
  glyph_battery_percent: boolean;
  /** 底部画输出音量。 */
  glyph_volume: boolean;
  /** 音量画成圆弧条还是四个点。 */
  glyph_volume_style: GlyphVolumeStyle;
  /** 有线连接改用普通 Wi-Fi 图标。 */
  glyph_wifi_for_ethernet: boolean;
  /** iPhone 个人热点改用普通 Wi-Fi 图标。 */
  glyph_wifi_for_hotspot: boolean;
  /** 临时网络改用普通 Wi-Fi 图标。 */
  glyph_wifi_for_temporary: boolean;
  /** 互联网共享改用普通 Wi-Fi 图标。 */
  glyph_wifi_for_sharing: boolean;
  /** 彩色图标；关闭则交系统按菜单栏明暗反色。 */
  glyph_colored: boolean;
  /** 彩色图标的墨色取向。 */
  glyph_appearance: GlyphAppearance;
}

/** 音频方向：输出 / 输入。 */
export type AudioScope = "output" | "input";

/**
 * 菜单栏图标的实时预览（Rust `statusbar::tray::Preview`）。
 *
 * `image` 是 base64 的 `size × size` RGBA，与贴进菜单栏的那张图字节一致：
 * 彩色模式下 RGB 已经带好电池色与墨色，模板模式下只有 alpha 承载信息。
 */
export interface StatusBarGlyphPreview {
  image: string;
  size: number;
  tooltip: string;
  center:
    | "wifi"
    | "hotspot"
    | "ethernet"
    | "temporary"
    | "shared"
    | "bluetooth"
    | "clipboard";
  /** 彩色图标按深色菜单栏下的白墨；预览条底色要跟着走。 */
  dark_bar: boolean;
}

/** 可切换的音频设备（Rust `AudioDevice`）。 */
export interface StatusBarAudioDevice {
  id: number;
  name: string;
  is_default: boolean;
}

/** 默认音频设备状态（Rust `AudioDeviceState`）。 */
export interface StatusBarAudioState {
  available: boolean;
  device_id: number;
  device_name: string;
  /** 0 ~ 1。 */
  volume: number;
  /** 设备是否暴露音量属性（HDMI / 虚拟设备可能没有）。 */
  volume_available: boolean;
  muted: boolean;
  mute_available: boolean;
  devices: StatusBarAudioDevice[];
}

/** 电池状态（Rust `BatteryState`）。 */
export interface StatusBarBattery {
  present: boolean;
  percent: number;
  charging: boolean;
  /** IOPS `Is Charged`：系统判定真的充满了（副标题第一优先级）。 */
  charged: boolean;
  plugged: boolean;
  /** 系统「低电量模式」：正版副标题在「充电中」之后、`已连接电源` 之前判它。 */
  low_power: boolean;
  minutes_to_empty: number | null;
  minutes_to_full: number | null;
}

/** 电池详情页读数（Rust `BatteryDetails`，单位已换算为 W / V / A）。 */
export interface StatusBarBatteryDetails {
  battery_watts: number | null;
  system_watts: number | null;
  input_watts: number | null;
  adapter_watts: number | null;
  cycle_count: number | null;
  voltage_volts: number | null;
  current_amps: number | null;
  sampled_at: string | null;
}

/** 「立即充满电」的 PowerUI 读数（Rust `ChargeToFullState`）。 */
export interface StatusBarChargeToFullState {
  /** 这台 Mac + macOS 版本是否支持 PowerUI 充电上限接口。 */
  available: boolean;
  valid: boolean;
  /** 充电上限已启用且档位在 80/85/90/95 之一。 */
  has_active_limit: boolean;
  /** 临时解除上限（立即充满）正在进行。 */
  override_active: boolean;
  limit: number | null;
  error: string | null;
}

/** 面板可见时的轻量音量轮询（Rust `AudioLevels`）：只用来让滑条跟上硬件音量键。 */
export interface StatusBarAudioLevels {
  output: StatusBarAudioState;
  input: StatusBarAudioState;
}

/** 输入法（Rust `InputSource`）。 */
export interface StatusBarInputSource {
  id: string;
  name: string;
  selected: boolean;
  /** 无图标输入源的 1~2 字标记（正版 `InputSourceShortLabel`），读不出时为 null。 */
  marker: string | null;
  /** 输入法自带的设置 App 路径（微信输入法 → WeTypeSettings.app）；Apple 自带为 null。 */
  settings_app: string | null;
}

/** Wi-Fi / 首选链路状态（Rust `NetworkState`）。 */
export interface StatusBarNetwork {
  available: boolean;
  powered: boolean;
  connected: boolean;
  /** macOS 14.4+ 未授权定位时为 null。 */
  ssid: string | null;
  rssi: number | null;
  /** 频段文案（「2.4 GHz」/「5 GHz」/「6 GHz」）：正版只在 Wi-Fi/热点链路时给。 */
  band: string | null;
  /** Wi‑Fi 名称授权态：`authorized` / `not_determined` / `denied` / `restricted`。 */
  name_access: "authorized" | "not_determined" | "denied" | "restricted";
  interface: string | null;
  ipv4: string | null;
  link: "wifi" | "hotspot" | "sharing" | "temporary" | "ethernet" | "none";
}

/** Wi‑Fi 二级页：一条扫描到的 AP（Rust `WifiCandidate`）。 */
export interface StatusBarWifiCandidate {
  /** CoreWLAN 原样返回的 SSID，首尾空格也是身份的一部分。 */
  ssid: string;
  bssid: string | null;
  /** dBm；非负值按正版视为「读不到」。 */
  rssi: number | null;
  channel: number | null;
  /** `CWSecurity` 原始值（-1 = 未知）。 */
  security: number;
}

/** Wi‑Fi 二级页「显示详情」的原料（Rust `WifiDetails` = 正版 `WiFiConnectionDetails`）。 */
export interface StatusBarWifiDetails {
  ssid: string | null;
  bssid: string | null;
  band: string | null;
  channel: number | null;
  channel_width: string | null;
  rssi: number | null;
  noise: number | null;
  phy_mode: string | null;
  transmit_rate: number | null;
  security: number;
  country_code: string | null;
  interface_name: string | null;
  ipv4_addresses: string[];
  ipv6_addresses: string[];
  router: string | null;
  dns_servers: string[];
}

/** Wi‑Fi 二级页一次读数（Rust `WifiScan`）。 */
export interface StatusBarWifiScan {
  available: boolean;
  powered: boolean;
  name_access: "authorized" | "not_determined" | "denied" | "restricted";
  /** `ready` / `poweredOff` / `noInterface` / `failed`。 */
  list_state: "ready" | "poweredOff" | "noInterface" | "failed";
  candidates: StatusBarWifiCandidate[];
  connected_bssid: string | null;
  known_ssids: string[];
  details: StatusBarWifiDetails;
  hotspot: boolean;
}

/** 代理模式三档（Rust `ProxyPage::mode`，Clash `PATCH /configs` 的三个取值）。 */
export type ProxyMode = "rule" | "global" | "direct";

/** 代理策略组的一个成员（Rust `ProxyNode`）。信息行（「剩余流量…」）也是成员。 */
export interface StatusBarProxyNode {
  name: string;
  /** `ss` / `vmess` / `select` / `url-test`…；读不到为空串。 */
  kind: string;
  /** 最近一次延迟（毫秒）；超时或未测为 null。 */
  delay: number | null;
}

/** 代理策略组（Rust `ProxyGroup`，Clash `/proxies` 的一项）。 */
export interface StatusBarProxyGroup {
  name: string;
  /** `select` / `url-test` / `fallback` / `load-balance`… */
  kind: string;
  current: string | null;
  /** Clash 的 `hidden`：正版也不在菜单里显示这类组。 */
  hidden: boolean;
  delay: number | null;
  nodes: StatusBarProxyNode[];
}

/** 代理（VPN）二级页一次完整读数（Rust `ProxyPage`）。 */
export interface StatusBarProxyPage {
  available: boolean;
  /** `clash` = 有控制口（全能控）；`probe` = 只探到端口；`none` = 没核心（配合 `available` 表示失效的接管）。 */
  source: "clash" | "probe" | "none";
  core_running: boolean;
  mixed_port: number;
  /** 实际控制口地址（如 `127.0.0.1:9097`）；没有则空串。 */
  controller: string;
  /** 系统代理三样里任一开着且指向本机 = 已在接管。 */
  takeover: boolean;
  takeover_web: boolean;
  takeover_https: boolean;
  takeover_socks: boolean;
  /** 系统代理实际指向的本机端口；没接管时 0。核心换过端口后它和 `mixed_port` 可以不同。 */
  proxy_port: number;
  /** `proxy_port` 上有没有人监听。接管开着而这里是 false，网页就打不开。 */
  takeover_alive: boolean;
  /** `networksetup` 的网络服务名（如 `Wi-Fi`）；空串 = 认不出当前服务。 */
  service: string;
  mode: ProxyMode | null;
  groups: StatusBarProxyGroup[];
  /** 虚拟网卡（TUN）状态：第一版只读，不给开关。 */
  tun_enabled: boolean | null;
  up_bps: number | null;
  down_bps: number | null;
  /** `proxy` = 走代理的收发；`interface` = 本机网卡计数（口径更宽）。 */
  speed_scope: "proxy" | "interface";
  test_url: string;
  /** 可直接展示的中文说明；正常时为空串，绝不含订阅密钥。 */
  message: string;
}

/** 代理页可见时的网速轮询（Rust `ProxySpeed`）。 */
export interface StatusBarProxySpeed {
  up_bps: number | null;
  down_bps: number | null;
  speed_scope: "proxy" | "interface";
}

/** 代理写操作的结果（Rust `ProxyOutcome`）。失败原因要显示在页面上，不弹通用错误。 */
export interface StatusBarProxyOutcome {
  code: "ok" | "failed" | "unsupported";
  message: string;
}

/** 自动换节点配置（Rust `AutoSwitchConfig`，存 ~/.halo/autoswitch.json）。 */
export interface AutoSwitchConfig {
  enabled: boolean;
  router_group: string;
  local_group: string;
  min_speed_kbps: number;
  probe_interval_secs: number;
  daily_cap: number;
  /** 只读：钥匙串里是否已存路由器 API 密钥。 */
  has_router_secret: boolean;
}

/** 自动换节点卡片读数（Rust `AutoSwitchState`）。 */
export interface AutoSwitchState {
  config: AutoSwitchConfig;
  backend: "router" | "local" | "none";
  backend_label: string;
  current_node: string;
  last_speed_kbps: number | null;
  last_probe_at: number | null;
  last_switch_at: number | null;
  last_switch_from: string;
  last_switch_to: string;
  switches_today: number;
  consecutive_bad: number;
  status: string;
  /** 最近 20 条决策日志（新→旧）。 */
  log: string[];
}

/** 蓝牙设备（Rust `BluetoothDevice`）：已连接与已配对的都会列出。 */
export interface StatusBarBluetoothDevice {
  name: string;
  address: string | null;
  battery: number | null;
  connected: boolean;
  /** `device_minorType`（Headphones / Keyboard / Mouse…）。 */
  kind: string | null;
}

export interface StatusBarBluetooth {
  available: boolean;
  powered: boolean;
  devices: StatusBarBluetoothDevice[];
}

/** 无线麦 SayAll（Rust `sayall::State`）：第三方 App，Halo 只做入口与开关。 */
export interface SayAllState {
  /** 这台 Mac 上装没装；没装时 Halo 里这一项整个不出现。 */
  installed: boolean;
  version: string | null;
  running: boolean;
  /** 这个版本的 SayAll 认不认「菜单栏显示图标」这个键；不认时不能给开关。 */
  status_bar_icon_supported: boolean;
  /** SayAll 自己的「菜单栏显示图标」偏好（它只在启动时读一次）。 */
  status_bar_visible: boolean;
}

/** 齿轮可打开的系统设置面板（Rust `SYSTEM_PANES` 白名单键）。 */
export type SystemPane =
  | "battery"
  | "wifi"
  | "bluetooth"
  | "sound_output"
  | "sound_input"
  | "keyboard"
  | "location"
  | "sharing"
  | "bluetooth_permission";

/** 尚未接入的能力（Rust `future::Capability`）。 */
export interface StatusBarCapability {
  id: string;
  name: string;
  supported: boolean;
  /** 计划批次（3 = 后续接入）。 */
  phase: number;
  reason: string | null;
}

/** 状态栏面板一次拉齐的快照（Rust `StatusBarState`）。 */
export interface StatusBarSnapshot {
  /** 系统强调色（`#RRGGBB`），面板所有强调元素都用它。 */
  accent: string;
  audio_output: StatusBarAudioState;
  audio_input: StatusBarAudioState;
  battery: StatusBarBattery;
  input_sources: StatusBarInputSource[];
  current_input_source: string | null;
  network: StatusBarNetwork;
  bluetooth: StatusBarBluetooth;
  capabilities: StatusBarCapability[];
}

/** 本地备份导出结果（Rust `ExportResult`）。 */
export interface ExportResult {
  /** 导出的包目录绝对路径。 */
  path: string;
  /** 导出的历史条数。 */
  clips: number;
  /** 随包复制的图片文件数。 */
  files_copied: number;
  /** 随包复制的图片总字节数。 */
  bytes: number;
}

/** 本地备份导入结果（Rust `ImportSummary`）。 */
export interface ImportResult {
  /** 成功写入的历史条数。 */
  clips: number;
  /** 与既有记录合并的条数。 */
  merged: number;
  /** 失败/跳过的条数。 */
  skipped: number;
  /** 失败明细。 */
  errors: string[];
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
