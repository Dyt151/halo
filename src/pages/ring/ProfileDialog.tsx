import { useEffect, useRef, useState } from "react";
import { Btn, Divider, Input, Row, Segment } from "../../components/ui";
import type { RingProfile } from "../../lib/types";
import {
  DOC_EXTS,
  IMAGE_EXTS,
  MAX_EXTENSION_LEN,
  normalizeExtension,
  parseExtensions,
  shadowedBy,
  TEXT_EXTS,
} from "./model";

export /**
 * Profile 编辑器：名称 + 匹配扩展名 + 类型（普通 / 混合兜底）。
 *
 * 扩展名用「文本输入 + 快速填充」而不是一个个 chip 输入：手写一行
 * `png, jpg, .webp` 比连点十几次快，规范化规则与后端完全一致（非法项
 * 即时丢弃并提示），不会出现「界面上有、后端存不下」的偏差。
 */
function ProfileDialog({
  profile,
  others,
  onClose,
  onPatch,
  onDelete,
}: {
  profile: RingProfile;
  others: RingProfile[];
  onClose: () => void;
  onPatch: (update: Partial<RingProfile>) => void;
  onDelete: () => void;
}) {
  const [name, setName] = useState(profile.name);
  const nameDirty = useRef(false);
  useEffect(() => {
    if (!nameDirty.current) setName(profile.name);
  }, [profile.name]);

  const [extText, setExtText] = useState(profile.extensions.join(", "));
  const extDirty = useRef(false);
  // 只在切到另一个 Profile 时重同步：跟着回写同步会在输入过程中抢走光标。
  useEffect(() => {
    if (extDirty.current) return;
    setExtText(profile.extensions.join(", "));
  }, [profile.id]); // eslint-disable-line react-hooks/exhaustive-deps

  const parsed = parseExtensions(extText);
  const missing: string[] = [];
  for (const token of extText.split(/[\s,，、;；/]+/).filter(Boolean)) {
    if (!normalizeExtension(token)) missing.push(token);
  }

  const commitName = () => {
    if (!nameDirty.current) return;
    nameDirty.current = false;
    const next = name.trim();
    if (next && next !== profile.name) onPatch({ name: next });
    else setName(profile.name);
  };

  /** 扩展名合法就即时保存（与「每次成功编辑立即提交」一致）。 */
  const commitExt = (text: string) => {
    extDirty.current = false;
    const { valid } = parseExtensions(text);
    const same =
      valid.length === profile.extensions.length &&
      valid.every((ext, i) => ext === profile.extensions[i]);
    if (!same) onPatch({ extensions: valid });
  };

  const close = () => {
    commitName();
    commitExt(extText);
    onClose();
  };
  const closeRef = useRef(close);
  closeRef.current = close;

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      e.stopPropagation();
      closeRef.current();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, []);

  const mixed = profile.kind === "mixed_fallback";
  /** 兜底盘的空扩展名 = catch-all；普通盘的空扩展名 = 永不匹配，必须说清楚。 */
  const emptyWarning = !mixed && parsed.valid.length === 0;
  const shadow = shadowedBy([...others, profile], [...others, profile].length - 1);

  return (
    <div
      className="fixed inset-0 z-50 grid place-items-center bg-black/55 p-[24px]"
      role="dialog"
      aria-modal="true"
      aria-label={`${profile.name} Profile`}
      onClick={close}
    >
      <div
        className="dialog-surface max-h-[86vh] w-[420px] overflow-y-auto rounded-[12px] p-[16px] shadow-modal"
        onClick={(e) => e.stopPropagation()}
      >
        <Input
          aria-label="Profile 名称"
          autoFocus
          value={name}
          onChange={(e) => {
            nameDirty.current = true;
            setName(e.target.value);
          }}
          onBlur={commitName}
          onKeyDown={(e) => {
            if (e.key === "Enter") (e.target as HTMLInputElement).blur();
          }}
        />
        <div className="mt-[6px] text-[12px]/[16px] text-white/45">
          {profile.id} · {profile.preset_ids.length} 个动作
        </div>

        <Divider />

        <Row label="类型" hint="普通 Profile 优先匹配；混合兜底负责「没配置的类型」">
          <Segment
            items={[
              { value: "normal", label: "普通" },
              { value: "mixed_fallback", label: "混合兜底" },
            ]}
            value={profile.kind}
            onChange={(v) => onPatch({ kind: v })}
          />
        </Row>

        <div className="mt-[10px]">
          <div className="mb-[6px] text-[12px]/[16px] text-white/60">匹配扩展名</div>
          <Input
            aria-label="匹配扩展名"
            value={extText}
            placeholder={mixed ? "留空 = 兜住所有未匹配的类型" : "例如 png, jpg, jpeg, webp"}
            onChange={(e) => setExtText(e.target.value)}
            onBlur={() => commitExt(extText)}
            onKeyDown={(e) => {
              if (e.key === "Enter") (e.target as HTMLInputElement).blur();
            }}
            spellCheck={false}
          />
          <div className="mt-[6px] flex flex-wrap items-center gap-[6px]">
            <span className="text-[11px]/[15px] text-white/35">快速填充</span>
            <Btn size="sm" onClick={() => setExtText(IMAGE_EXTS.join(", "))}>
              图片族
            </Btn>
            <Btn size="sm" onClick={() => setExtText(TEXT_EXTS.join(", "))}>
              文本族
            </Btn>
            <Btn size="sm" onClick={() => setExtText(DOC_EXTS.join(", "))}>
              文档族
            </Btn>
            <Btn size="sm" onClick={() => setExtText("")}>
              清空
            </Btn>
          </div>
          <div className="mt-[6px] text-[11px]/[15px] text-white/40">
            已识别 {parsed.valid.length} 个：{parsed.valid.join(" · ") || "无"}
          </div>
          {missing.length > 0 && (
            <div className="mt-[4px] text-[11px]/[15px] text-amber-300/85">
              已忽略 {missing.length} 个无效项（{missing.slice(0, 4).join("、")}
              {missing.length > 4 ? "…" : ""}）：最长 {MAX_EXTENSION_LEN} 字符，只允许字母、数字与 . _ -
            </div>
          )}
          {emptyWarning && (
            <div className="mt-[4px] text-[11px]/[15px] text-amber-300/85">
              普通 Profile 没有扩展名就不会匹配任何文件；要兜住所有类型请改成「混合兜底」
            </div>
          )}
          {shadow && (
            <div className="mt-[4px] text-[11px]/[15px] text-amber-300/85">
              这些扩展名已被「{shadow.name}」完全覆盖：在当前排序下轮不到它
            </div>
          )}
        </div>

        <div className="mt-[16px] flex justify-end gap-[8px]">
          <Btn size="sm" variant="danger" onClick={onDelete}>
            删除 Profile
          </Btn>
          <Btn size="sm" variant="primary" onClick={close}>
            完成
          </Btn>
        </div>
      </div>
    </div>
  );
}
