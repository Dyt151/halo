// invoke 封装：类型安全的命令调用。

import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { toApiError } from "./errors";
import type {
  ClipContent,
  ClipItem,
  Config,
  ContributionEntry,
  ConversionJob,
  ExportResult,
  ImportResult,
  HistoryQuery,
  HistoryStats,
  ModuleManifest,
  ModuleRecord,
  PasteOutcome,
  RingSnapshot,
  GestureStatus,
  AxRepair,
  StackPasteAllResult,
  SyncStatus,
  Capability,
  FinderIntegrationStatus,
  SyncCredentials,
  StatusBarSnapshot,
  StatusBarGlyphPreview,
  StatusBarBatteryDetails,
  StatusBarChargeToFullState,
  StatusBarAudioLevels,
  StatusBarWifiScan,
  SystemPane,
  AudioScope,
} from "./types";

/** 同步设置入参（与 Rust `SyncSettingsInput` 对齐；密码留空表示不改）。 */
export interface SyncSettingsInput {
  enabled: boolean;
  provider: import("./types").SyncProviderKind;
  url: string;
  username: string;
  password?: string;
  encryption: boolean;
  passphrase?: string;
  remote_file: string;
  interval_secs: number;
}

/** 快捷键格式/保留键校验结果（对应 Rust `HotkeyCheck`）。 */
export interface HotkeyCheck {
  ok: boolean;
  message: string | null;
}

/** 单个快捷键槽位的真实注册结果（对应 Rust `HotkeySlotResult`）。 */
export interface HotkeySlotResult {
  /** 全局槽位之一：`toggle` / `stack_paste`。 */
  slot: string;
  accelerator: string;
  ok: boolean;
  message: string | null;
}

/**
 * 命令调用外壳（唯一入口）。
 *
 * 后端失败时 reject 的是 `{ code, message, details }` 这个**对象**（`AppError` 经
 * `serde_json::to_value` 序列化后以 application/json 回传），直接 `String(e)` 会得到
 * `[object Object]`。这里统一折成 `ApiError`（`message` 可读、`toString()` 同文案），
 * 上层所有 `setError(String(e))` / `toastError(e)` 才拿得到真正的原因。
 */
async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await tauriInvoke<T>(cmd, args);
  } catch (e) {
    throw toApiError(e);
  }
}

/** 供 `lib/windows.ts` 等少量直接调命令的地方复用同一套错误归一化。 */
export const invokeCommand = invoke;

export const api = {
  listClips: (query: HistoryQuery) => invoke<ClipItem[]>("list_clips", { query }),
  getClip: (id: number) => invoke<ClipItem>("get_clip", { id }),
  pasteClip: (id: number) => invoke<PasteOutcome>("paste_clip", { id }),
  copyClip: (id: number) => invoke<ClipContent>("copy_clip", { id }),
  deleteClip: (id: number) => invoke<void>("delete_clip", { id }),
  clearClips: () => invoke<void>("clear_clips"),
  undo: () => invoke<ClipItem | null>("undo"),
  redo: () => invoke<number | null>("redo"),
  listTags: () => invoke<string[]>("list_tags"),
  pinClip: (id: number, pinned: boolean) => invoke<void>("pin_clip", { id, pinned }),
  favoriteClip: (id: number, favorite: boolean) =>
    invoke<void>("favorite_clip", { id, favorite }),
  setClipTag: (id: number, tag: string | null) =>
    invoke<void>("set_clip_tag", { id, tag }),
  historyStats: () => invoke<HistoryStats>("history_stats"),
  captureNow: () => invoke<ClipItem | null>("capture_now"),

  getConfig: () => invoke<Config>("get_config"),
  patchConfig: (patch: Record<string, unknown>) =>
    invoke<Config>("patch_config", { patch }),
  /** 跳系统设置页（`accessibility` / `login-items` 等）。 */
  openSystemSettings: (target: string) => invoke<void>("open_system_settings", { pane: target }),
  /** 校验加速键：可解析、带修饰键、且不占用系统保留组合（清单 16）。 */
  validateHotkey: (hotkey: string) => invoke<HotkeyCheck>("validate_hotkey", { hotkey }),
  /** 按当前配置重新注册全部快捷键，逐槽返回是否真的注册成功（清单 16）。 */
  applyHotkeys: () => invoke<HotkeySlotResult[]>("apply_hotkeys"),

  stackPush: (id: number) => invoke<void>("stack_push", { id }),
  stackList: () => invoke<number[]>("stack_list"),
  stackPasteFirst: () => invoke<PasteOutcome | null>("stack_paste_first"),
  stackPasteAll: () => invoke<StackPasteAllResult>("stack_paste_all"),
  stackClear: () => invoke<void>("stack_clear"),
  stackSetLock: (locked: boolean) => invoke<void>("stack_set_lock", { locked }),
  stackLocked: () => invoke<boolean>("stack_locked"),

  syncNow: () => invoke<SyncStatus>("sync_now"),
  getSyncStatus: () => invoke<SyncStatus>("get_sync_status"),
  testSync: (credentials: SyncCredentials, provider?: import("./types").SyncProviderKind) =>
    invoke<string>("test_sync", { credentials, provider }),
  syncSecretStatus: () =>
    invoke<{ has_password: boolean; has_passphrase: boolean }>("sync_secret_status"),
  setSyncSettings: (input: SyncSettingsInput) =>
    invoke<import("./types").SyncConfig>("set_sync_settings", { input }),

  // ---------- 系统能力 ----------
  /** 原生「打开文件」面板（可多选）；取消返回空数组。 */
  pickFiles: (multiple = false) => invoke<string[]>("pick_files", { multiple }),
  /** 原生「选择文件夹」面板；取消返回空数组。 */
  pickDirectory: () => invoke<string[]>("pick_directory"),
  /** 辅助功能是否已授权（回贴需要，未授权时降级为仅复制）。 */
  accessibilityStatus: () => invoke<boolean>("accessibility_status"),
  /** 主动请求辅助功能授权：系统弹窗并把 Halo 加进白名单（只由按钮调用）。 */
  requestAccessibility: () => invoke<boolean>("request_accessibility"),
  /** 环形手势现场状态：开关/授权/监听/前台条件 + 最近一次没反应的原因。 */
  ringGestureStatus: () => invoke<GestureStatus>("ring_gesture_status"),
  /** 修复辅助功能授权：清掉失效的 TCC 记录后重新登记（重置后需重新勾选）。 */
  ringGestureRepair: () => invoke<AxRepair>("ring_gesture_repair"),
  loginItemStatus: () =>
    invoke<{ supported: boolean; enabled: boolean; requires_approval: boolean }>(
      "login_item_status",
    ),
  setLoginItem: (enabled: boolean) =>
    invoke<{ supported: boolean; enabled: boolean; requires_approval: boolean }>(
      "set_login_item",
      { enabled },
    ),
  appInfo: () =>
    invoke<{ version: string; arch: string; data_dir: string; db_path: string }>("app_info"),
  /** 由前端在收起动画播完后调用（评审 U9）。 */
  hidePanel: () => invoke<void>("hide_panel"),
  /** 退出 App（菜单栏图标已取消，退出入口在设置 → 关于）。 */
  quitApp: () => invoke<void>("quit_app"),

  // ---------- 本地备份 / 迁移 ----------
  /**
   * 导出整库为可迁移包（目录，含 backup.json + 可选 files/）。
   * `includeFiles` 默认 true：把图片一并复制进包，迁移到新机后用得上。
   */
  exportData: (destDir: string, includeFiles = true) =>
    invoke<ExportResult>("export_data", { destDir, includeFiles }),
  /**
   * 从导出包恢复历史（可选恢复配置）。`path` 为包目录或包内 backup.json。
   * 导入是幂等的（按 hash 去重），可重复跑补齐遗漏。
   */
  importData: (path: string, restoreConfig = false) =>
    invoke<ImportResult>("import_data", { path, restoreConfig }),

  listModules: () => invoke<ModuleRecord[]>("list_modules"),
  installModule: (path: string) => invoke<ModuleManifest>("install_module", { path }),
  uninstallModule: (id: string) => invoke<void>("uninstall_module", { id }),
  enableModule: (id: string, grants: Capability[]) =>
    invoke<void>("enable_module", { id, grants }),
  disableModule: (id: string) => invoke<void>("disable_module", { id }),
  invokeModule: (id: string, method: string, params: unknown) =>
    invoke<unknown>("invoke_module", { id, method, params }),
  listModuleContributions: () => invoke<ContributionEntry[]>("list_module_contributions"),
  readModuleUi: (id: string) => invoke<string>("read_module_ui", { id }),

  // ---------- Finder 扩展 ----------
  finderIntegrationStatus: () =>
    invoke<FinderIntegrationStatus>("finder_integration_status"),
  setFinderExtensionEnabled: (enabled: boolean) =>
    invoke<FinderIntegrationStatus>("set_finder_extension_enabled", { enabled }),

  // ---------- 格式转换 ----------
  /**
   * 按预设批量转换（动作身份是预设 id，不再是格式名）。
   *
   * 阻塞到全部文件出结果；进度另外通过 `convert://progress` 事件推送。
   */
  convertRun: (paths: string[], presetId: string) =>
    invoke<ConversionJob>("convert_run", { paths, presetId }),
  /** 取消正在运行的转换（返回是否真有任务被取消）。 */
  convertCancel: () => invoke<boolean>("convert_cancel"),
  revealPath: (path: string) => invoke<void>("reveal_path", { path }),

  // ---------- 环形转换（ring 窗口 ↔ 会话状态机） ----------
  /** ring 窗口挂载/收到更新事件时拉取当前会话快照。 */
  ringSnapshot: () => invoke<RingSnapshot>("ring_snapshot"),
  /** 用户在环形盘选定动作（动作 id = 预设 id）。 */
  ringExecute: (actionId: string) => invoke<void>("ring_execute", { actionId }),
  /** 取消环形盘（Esc / 点外部 / 失焦）。 */
  ringCancel: () => invoke<void>("ring_cancel"),
  /** 结果动画播完后收起环形盘（原地成功/失败反馈结束）。 */
  ringDismiss: () => invoke<void>("ring_dismiss"),

  // ---------- 状态栏（菜单栏图标 ↔ 下拉面板） ----------
  /** 一次拉齐：输出/输入音量、电池、输入法、未接入能力。 */
  statusBarState: () => invoke<StatusBarSnapshot>("statusbar_state"),
  /** 菜单栏图标的实时预览（后端把同一张将要贴到菜单栏的图直接给前端）。 */
  statusBarGlyphPreview: () => invoke<StatusBarGlyphPreview>("statusbar_glyph_preview"),
  statusBarSetVolume: (scope: AudioScope, volume: number) =>
    invoke<void>("statusbar_set_volume", { scope, volume }),
  statusBarSetMute: (scope: AudioScope, muted: boolean) =>
    invoke<void>("statusbar_set_mute", { scope, muted }),
  statusBarSetDefaultDevice: (scope: AudioScope, deviceId: number) =>
    invoke<void>("statusbar_set_default_device", { scope, deviceId }),
  statusBarSelectInputSource: (id: string) =>
    invoke<void>("statusbar_select_input_source", { id }),
  statusBarSetWifi: (enabled: boolean) => invoke<void>("statusbar_set_wifi", { enabled }),
  statusBarSetBluetooth: (enabled: boolean) =>
    invoke<void>("statusbar_set_bluetooth", { enabled }),
  /** 申请定位权限：授权后 macOS 才会返回 Wi-Fi 名称。正版返回三态结果。 */
  statusBarRequestLocation: () =>
    invoke<"requested" | "open_location_settings" | "not_needed">("statusbar_request_location"),
  /** 电池详情页读数（打开页面后每 2 秒拉一次）。 */
  statusBarBatteryDetails: () =>
    invoke<StatusBarBatteryDetails>("statusbar_battery_details"),
  /** 「立即充满电」的充电上限读数（面板可见时每 2 秒拉一次）。 */
  statusBarChargeToFullState: () =>
    invoke<StatusBarChargeToFullState>("statusbar_charge_to_full_state"),
  /** 发起立即充满：解除充电上限，充到 100%。 */
  statusBarChargeToFull: () => invoke<void>("statusbar_charge_to_full"),
  /** 取消立即充满：恢复发起前记下的充电上限，返回恢复到的档位。 */
  statusBarCancelChargeToFull: () =>
    invoke<number>("statusbar_cancel_charge_to_full"),
  /** Wi‑Fi 二级页读数：含一次全信道扫描（一秒上下），只在打开页面/点刷新时调。 */
  statusBarWifiPage: () => invoke<StatusBarWifiScan>("statusbar_wifi_page"),
  /** 面板可见时的轻量音量读数：让滑条跟随键盘/触控栏音量键。 */
  statusBarAudioLevels: () => invoke<StatusBarAudioLevels>("statusbar_audio_levels"),
  /** 音量提示音的系统总开关（`com.apple.sound.beep.feedback`，缺省即开）。 */
  statusBarVolumeFeedbackEnabled: () => invoke<boolean>("statusbar_volume_feedback_enabled"),
  /** 齿轮：打开 macOS 系统设置对应面板（不是 Halo 自己的设置）。 */
  statusBarOpenPane: (pane: SystemPane) => invoke<void>("statusbar_open_pane", { pane }),
  /** 收起面板（Esc / 执行动作后）。 */
  statusBarHide: () => invoke<void>("statusbar_hide"),
  /** 面板底部「设置…」：收起并打开主窗口的状态栏分区。 */
  statusBarOpenSettings: () => invoke<void>("statusbar_open_settings"),
  /** 前端按实际内容高度回报，窗口据此重新贴到图标下方。 */
  statusBarResize: (height: number) => invoke<void>("statusbar_resize", { height }),
};
