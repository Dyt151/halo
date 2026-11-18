import type { ConversionPreset, ConvertFormat, RingProfile } from "../../lib/types";

export type Category = "image" | "doc" | "tool";
export type Filter = "all" | Category;

/** 图片格式（候选面板的「图片 / 文档」分类用）。 */
export const IMAGE_TARGETS: ConvertFormat[] = [
  "png",
  "jpg",
  "webp",
  "gif",
  "bmp",
  "tiff",
  "heic",
];

/**
 * 扩展名别名的**前端镜像**（与 Rust `ConvertFormat::from_ext` 同一张表）。
 *
 * 两边各写一份是不可避免的（前端不能调 Rust 纯函数），所以这里用「同一个
 * 校验口径」而不是「同一个字符串列表」：复合扩展名（`tar.gz`）只有在表里
 * 出现过才被接受，否则用户会存下一个永远匹配不到的字符串。
 */
export const KNOWN_EXTENSIONS = new Set([
  "png",
  "jpg",
  "jpeg",
  "jpe",
  "webp",
  "gif",
  "bmp",
  "tif",
  "tiff",
  "heic",
  "heif",
  "hif",
  "html",
  "htm",
  "xhtml",
  "txt",
  "text",
  "pdf",
  "docx",
  "docm",
  "dotx",
]);
/** 单个扩展名最长多少字符（Rust `MAX_EXTENSION_LEN`）。 */
export const MAX_EXTENSION_LEN = 16;
/** 扩展名快速填充：图片族 / 文本族 / 文档族。 */
export const IMAGE_EXTS = [
  "png",
  "jpg",
  "jpeg",
  "jpe",
  "webp",
  "gif",
  "bmp",
  "tif",
  "tiff",
  "heic",
  "heif",
  "hif",
];
export const TEXT_EXTS = ["html", "htm", "xhtml", "txt", "text"];
export const DOC_EXTS = ["pdf", "docx", "docm", "dotx"];

/**
 * 内置 Profile 的「恢复默认」顺序（与 Rust `preset::DEFAULT_*` / `stock()` 对齐）。
 *
 * 每张盘只有一面；打包 / 解压 / PDF 分页由「工具动作」开关统一追加。
 */
export const DEFAULT_SLICES: Record<string, string[]> = {
  images: ["img.jpg", "img.png", "img.webp", "img.gif", "img.heic", "img.pdf"],
  text: ["doc.html", "doc.txt"],
  docs: ["img.png", "img.jpg", "doc.txt", "doc.html", "doc.docx", "doc.pdf"],
  mixed: [
    "img.jpg",
    "img.png",
    "img.webp",
    "img.gif",
    "img.bmp",
    "img.tiff",
    "doc.html",
    "doc.txt",
  ],
};

export const ARCHIVE_LABELS: Record<string, string> = { zip: "ZIP", tar_gz: "TAR.GZ", tar: "TAR" };

/** 工具动作的短说明（打包 / 解压 / PDF 分页）。 */
export function kindCaption(preset: ConversionPreset): string | null {
  const kind = preset.kind ?? "convert";
  if (kind === "convert") return null;
  if (kind === "extract") return "解压成文件夹";
  if (kind === "pdf_to_cbz") return "PDF → CBZ 漫画包";
  if ("pack" in kind) return `整批 → 一个 ${ARCHIVE_LABELS[kind.pack.archive] ?? kind.pack.archive}`;
  if ("pdf_pages" in kind) return `每页一张 ${kind.pdf_pages.to.toUpperCase()}`;
  return "工具动作";
}

/** 预设副标题：目标 + 关键参数，让同目标不同质量的预设能一眼分开。 */
export function presetCaption(preset: ConversionPreset): string {
  const kind = kindCaption(preset);
  if (kind) return kind;
  if (preset.target === "*") return "同格式重编码";
  const label = preset.target.toUpperCase();
  if (preset.target === "jpg") return `${label} ${preset.image.jpeg_quality}% · 背景 ${preset.image.alpha_background}`;
  if (preset.target === "pdf") return `转成 PDF${preset.output.dir ? " · 自定义目录" : ""}`;
  return preset.output.dir ? `${label} · 自定义目录` : label;
}

export function presetCategory(preset: ConversionPreset): Category {
  if ((preset.kind ?? "convert") !== "convert") return "tool";
  if (preset.target === "*") return "image";
  return IMAGE_TARGETS.includes(preset.target) ? "image" : "doc";
}

/**
 * 扩展名规范化（前端镜像 Rust `core::ring::profile::normalize_extension`）。
 *
 * 返回 `null` 表示这个 token 会被后端丢掉，界面上要立刻提示，而不是等
 * 保存后发现「我写的扩展名怎么没了」。
 */
export function normalizeExtension(raw: string): string | null {
  const trimmed = raw.trim().toLowerCase();
  if (trimmed.includes("..")) return null;
  let text = trimmed.replace(/^\*+\s*/, "");
  text = text.replace(/^\.+/, "").replace(/\.+$/, "");
  if (!text || text.length > MAX_EXTENSION_LEN) return null;
  if (!/^[a-z0-9._-]+$/.test(text)) return null;
  if (text.includes(".") && !KNOWN_EXTENSIONS.has(text)) return null;
  return text;
}

/** 把输入框里的内容拆成扩展名（支持逗号、顿号、空格分隔），并按序去重。 */
export function parseExtensions(text: string): { valid: string[]; dropped: number } {
  const tokens = text.split(/[\s,，、;；/]+/).filter(Boolean);
  const valid: string[] = [];
  let dropped = 0;
  for (const token of tokens) {
    const ext = normalizeExtension(token);
    if (!ext) {
      dropped += 1;
      continue;
    }
    if (!valid.includes(ext)) valid.push(ext);
  }
  return { valid, dropped };
}

/** 某个 Profile 的扩展名是否被列表里更靠前的普通 Profile 完全覆盖（Filee 行为）。 */
export function shadowedBy(profiles: RingProfile[], index: number): RingProfile | null {
  const profile = profiles[index];
  if (!profile || profile.kind !== "normal" || profile.extensions.length === 0) return null;
  return (
    profiles.slice(0, index).find(
      (earlier) =>
        earlier.kind === "normal" &&
        profile.extensions.every((ext) =>
          earlier.extensions.some((own) => own.toLowerCase() === ext.toLowerCase()),
        ),
    ) ?? null
  );
}

/** 拖动会话：chip 来自候选面板，slice 来自圆环。 */
export interface DragState {
  source: "chip" | "slice";
  id: string;
  /** slice 的原索引；chip 为 -1。 */
  from: number;
  label: string;
  startX: number;
  startY: number;
  x: number;
  y: number;
  /** 是否已越过阈值。 */
  active: boolean;
  /** 当前落点 slot；-1 = 不在有效位置。 */
  slot: number;
  /** 是否位于候选面板上方（slice → 移除）。 */
  overPalette: boolean;
  /** 拖动开始时的添加资格快照（拖动中刷新搜索不能改变它）。 */
  canAdd: boolean;
}

/** 只保留还存在的预设：配置里若有已删预设的残留 id，读写前就地清掉。 */
export function clean(list: string[], known: ReadonlySet<string>): string[] {
  return list.filter((id) => known.has(id));
}

/** 生成不冲突的 Profile id。 */
export function uniqueProfileId(profiles: RingProfile[]): string {
  const used = new Set(profiles.map((p) => p.id));
  let n = profiles.length + 1;
  let id = `profile-${n}`;
  while (used.has(id)) {
    n += 1;
    id = `profile-${n}`;
  }
  return id;
}
