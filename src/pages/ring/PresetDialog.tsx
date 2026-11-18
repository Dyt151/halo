import { useEffect, useRef, useState } from "react";
import { Btn, Divider, Input, Row, Segment, SliderRow, ToggleRow } from "../../components/ui";
import { api } from "../../lib/ipc";
import type { ConversionPreset, RingProfile } from "../../lib/types";
import { IMAGE_TARGETS, presetCaption } from "./model";

/** `#RRGGBB` / `RRGGBB` / `#RGB`（与 Rust `parse_hex_rgb` 接受范围一致）。 */
const HEX_COLOR = /^#?([0-9a-fA-F]{3}|[0-9a-fA-F]{6})$/;

export /**
 * 预设编辑器：一份配方 = 名称 + 目标 + 图像选项 + 输出规则。
 *
 * 以前这里只有一个全局 JPG 质量滑块，所以「两个不同质量的 JPG 动作」根本做不出来；
 * 现在每条预设自带质量与输出规则，复制一份就能得到第二个 JPG。
 */
function PresetDialog({
  preset,
  inProfiles,
  onClose,
  onPatch,
  onDuplicate,
  onDelete,
  onRemoveFrom,
}: {
  preset: ConversionPreset;
  /** 这条预设当前出现在哪些 Profile 里。 */
  inProfiles: RingProfile[];
  onClose: () => void;
  onPatch: (update: Partial<ConversionPreset>) => void;
  onDuplicate: () => void;
  onDelete: () => void;
  onRemoveFrom: (profileId: string) => void;
}) {
  const [name, setName] = useState(preset.name);
  /**
   * 有未提交的改名时，外部回写（自己的 patch 响应也算）不得覆盖草稿。
   *
   * 早期依赖 `[preset.id, preset.name]` 无条件同步：输入到一半时只要配置
   * 回流一次，光标就会跳到末尾（与下面背景色草稿同一个坑）。
   */
  const nameDirty = useRef(false);
  useEffect(() => {
    if (!nameDirty.current) setName(preset.name);
  }, [preset.name]);

  /** 背景色草稿：合法值即时保存，非法值不落盘（后端回退成白色会让人误以为设置没生效）。 */
  const [bg, setBg] = useState(preset.image.alpha_background);
  // 只在切到另一条预设时重同步：跟着 `preset.image.alpha_background` 同步会在
  // 每次输入合法值→回写→effect 把草稿抢回去，光标跳到最后（#abc → #abcdef 打不完）。
  useEffect(() => setBg(preset.image.alpha_background), [preset.id]);
  const bgValid = HEX_COLOR.test(bg.trim());

  const targetsJpg = preset.target === "jpg";
  const isImage = preset.target !== "*" && IMAGE_TARGETS.includes(preset.target);

  /**
   * 质量滑块：拖动时只改本地草稿，停手 300ms 才提交。
   *
   * 直接绑 `preset.image.jpeg_quality` 有两个毛病：①每挑 5% 就写一次配置；
   * ②回写比拖动慢时，滑块会被拽回上一档（40→45→50 显示成 45→50→45→50）。
   */
  const [quality, setQuality] = useState(preset.image.jpeg_quality);
  const qualityTimer = useRef<number | null>(null);
  const qualityPending = useRef<number | null>(null);

  const flushQuality = () => {
    if (qualityTimer.current !== null) {
      window.clearTimeout(qualityTimer.current);
      qualityTimer.current = null;
    }
    const v = qualityPending.current;
    qualityPending.current = null;
    if (v !== null && v !== preset.image.jpeg_quality) {
      onPatch({ image: { ...preset.image, jpeg_quality: v } });
    }
  };
  // 卸载（点「完成」/ 切到别的预设）时把最后一档写回去，否则快速关闭会丢改动。
  const flushRef = useRef(flushQuality);
  flushRef.current = flushQuality;
  useEffect(() => () => flushRef.current(), []);

  const onQuality = (v: number) => {
    setQuality(v);
    qualityPending.current = v;
    if (qualityTimer.current !== null) window.clearTimeout(qualityTimer.current);
    qualityTimer.current = window.setTimeout(() => flushRef.current(), 300);
  };

  const commitName = () => {
    if (!nameDirty.current) return;
    nameDirty.current = false;
    const next = name.trim();
    if (next && next !== preset.name) onPatch({ name: next });
    else setName(preset.name);
  };

  /**
   * 关闭前先把未提交的名称写回去。
   *
   * 只有 Esc 走这条路时，直接在遮罩上点一下就会丢掉刚输入的改名；
   * 现在遮罩、Esc、「完成」按钮都走同一个出口。
   */
  const close = () => {
    commitName();
    onClose();
  };
  const closeRef = useRef(close);
  closeRef.current = close;

  // Esc 关闭（与其余浮层一致）。只绑一次：回调走 ref 取最新闭包，
  // 否则每敲一个字符就重绑一次监听。
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

  return (
    <div
      className="fixed inset-0 z-50 grid place-items-center bg-black/55 p-[24px]"
      role="dialog"
      aria-modal="true"
      aria-label={`${preset.name} 预设`}
      onClick={close}
    >
      <div
        className="dialog-surface max-h-[86vh] w-[400px] overflow-y-auto rounded-[12px] p-[16px] shadow-modal"
        onClick={(e) => e.stopPropagation()}
      >
        <Input
          aria-label="预设名称"
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
        <div className="mt-[6px] text-[12px]/[16px] text-white/50">
          {presetCaption(preset)} · {preset.id}
        </div>

        {targetsJpg && (
          <div className="mt-[12px]">
            <SliderRow
              label="输出质量"
              value={quality}
              min={40}
              max={100}
              step={5}
              format={(v) => `${v}%`}
              onChange={onQuality}
            />
          </div>
        )}

        {targetsJpg && (
          <Row label="透明背景" hint="透明图转 JPG 时的合成色；不显式指定会是黑底">
            <Input
              aria-label="透明背景色"
              className={"w-[110px]" + (bgValid ? "" : " border-danger/50")}
              value={bg}
              onChange={(e) => {
                const next = e.target.value;
                setBg(next);
                if (HEX_COLOR.test(next.trim())) {
                  onPatch({ image: { ...preset.image, alpha_background: next.trim() } });
                }
              }}
              onBlur={() => {
                if (!bgValid) setBg(preset.image.alpha_background);
              }}
              onKeyDown={(e) => {
                if (e.key === "Enter") (e.target as HTMLInputElement).blur();
              }}
              spellCheck={false}
            />
            {!bgValid && (
              <span className="text-[11px]/[14px] text-danger-soft">#RRGGBB</span>
            )}
          </Row>
        )}

        {isImage && (
          <ToggleRow
            label="同格式也重编码"
            hint="源文件已经是目标格式时仍重新编码（例如 JPG 降质量）；默认跳过"
            checked={preset.reencode_same_format}
            onChange={(v) => onPatch({ reencode_same_format: v })}
          />
        )}

        <Divider />

        <Row label="输出目录" hint="留空 = 与源文件同目录；源文件永远不会被覆盖">
          <div className="flex items-center gap-[6px]">
            <Btn
              size="sm"
              onClick={() =>
                void api
                  .pickDirectory()
                  .then((dirs) => {
                    if (dirs[0]) onPatch({ output: { ...preset.output, dir: dirs[0] } });
                  })
                  .catch(() => {})
              }
            >
              选择…
            </Btn>
            <span className="max-w-[160px] truncate text-[12px]/[16px] text-white/55">
              {preset.output.dir || "同目录"}
            </span>
            {preset.output.dir && (
              <Btn size="sm" onClick={() => onPatch({ output: { ...preset.output, dir: "" } })}>
                清空
              </Btn>
            )}
          </div>
        </Row>

        <Row label="重名处理" hint="只对输出文件生效，源文件始终受保护">
          <Segment
            items={[
              { value: "rename", label: "递增" },
              { value: "overwrite", label: "覆盖" },
              { value: "skip", label: "跳过" },
            ]}
            value={preset.output.conflict}
            onChange={(v) => onPatch({ output: { ...preset.output, conflict: v } })}
          />
        </Row>

        <ToggleRow
          label="只在变小时保存"
          hint="结果比同名输出更大时丢弃，不白白占空间"
          checked={preset.output.only_if_smaller}
          onChange={(v) => onPatch({ output: { ...preset.output, only_if_smaller: v } })}
        />

        <div className="mt-[12px] text-[12px]/[16px] text-white/45">
          当前位于：
          {inProfiles.length === 0 ? "未使用" : inProfiles.map((p) => p.name || p.id).join("、")}
        </div>

        <div className="mt-[14px] flex flex-wrap justify-end gap-[8px]">
          <Btn size="sm" onClick={onDuplicate}>
            复制一份
          </Btn>
          {!preset.builtin && (
            <Btn size="sm" variant="danger" onClick={onDelete}>
              删除预设
            </Btn>
          )}
          {inProfiles.map((p) => (
            <Btn key={p.id} size="sm" variant="danger" onClick={() => onRemoveFrom(p.id)}>
              从「{p.name || p.id}」移除
            </Btn>
          ))}
          <Btn size="sm" variant="primary" onClick={close}>
            完成
          </Btn>
        </div>
      </div>
    </div>
  );
}

/** 跟随系统「减少动态效果」：开启后所有预览直接到位，功能不变。 */
