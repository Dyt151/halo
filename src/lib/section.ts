/**
 * 主窗口分区归一化（纯函数，无 React 依赖，可直接单测）。
 *
 * 深链（Rust `open_main_window` → `halo://navigate`）与旧配置
 * （`last_settings_section`）里可能带着**已经下线或已经改名**的分区值。
 * 这里把它们统一映射到当前真实存在的分区；映射不到就返回 null，
 * 让调用方保持原样，而不是跳到一个不存在的页面——那会白屏。
 *
 * 单独成文件而不是留在 `MainWindow.tsx` 里：它恰好是「删掉侧边栏入口」
 * 最容易出事的地方（存量用户的 `last_settings_section` 会命中它），
 * 必须是能单测的纯函数——主窗口里的那段过去从来没有测试覆盖。
 */
import type { MainPage } from "./windows";

export function normalizeSection(raw: string | null | undefined): MainPage | null {
  if (!raw) return null;
  switch (raw) {
    // 旧别名：最早的设置页就叫「设置」。
    case "settings":
      return "general";
    // 旧别名：转换页后来改名成「环形转换」。
    case "convert":
      return "ring";
    // 权限页已并入通用页：旧值落到通用，而不是一个已经不存在的页面。
    case "permissions":
      return "general";
    // 已下线的分区：快捷键 / 快捷回复 / 模块。旧配置（last_settings_section）
    // 或旧深链仍可能带这几个值，落到「通用」而不是白屏。
    case "hotkeys":
    case "replies":
    case "modules":
      return "general";
    case "history":
    case "general":
    case "clipboard":
    case "ring":
    case "context-menu":
    case "statusbar":
    case "sync":
    case "backup":
    case "about":
      return raw;
    default:
      return null;
  }
}
