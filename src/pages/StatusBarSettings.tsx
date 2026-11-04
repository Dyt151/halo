/**
 * 设置 → 状态栏图标。
 *
 * 对应 status-trio 设置页的第三张图：菜单栏图标本身用「三区一张图」说状态
 * （外环=电量、中心=网络、底部=输出音量），面板里显示哪些模块、拖拽排序、
 * 面板滚动调音量。已接入电池 / 网络 / 蓝牙 / 音量 / 声音输入 / 输入法；VPN 与
 * 私有能力（AirPods 降噪、充电上限、iPhone 电量）在这里显式标「即将支持」，
 * 不做假开关。
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useClipStore } from "../store/useClipStore";
import { api } from "../lib/ipc";
import { toastError } from "../lib/toast";
import type {
  GlyphAppearance,
  GlyphCenter,
  GlyphVolumeStyle,
  StatusBarCapability,
  StatusBarConfig,
  StatusBarGlyphPreview,
  StatusBarSection,
} from "../lib/types";
import {
  CardGroup,
  Divider,
  Notice,
  Pill,
  Row,
  SelectRow,
  Switch,
  ToggleRow,
  cx,
} from "../components/ui";

interface ModuleMeta {
  label: string;
  hint: string;
  available: boolean;
}

/** 面板模块的展示信息（顺序不作为面板顺序，顺序完全由配置决定）。 */
const MODULES: Record<StatusBarSection, ModuleMeta> = {
  battery: { label: "电池", hint: "电量、充电状态与剩余时间", available: true },
  volume: { label: "音量", hint: "输出音量滑条与静音", available: true },
  audio_input: { label: "声音输入", hint: "麦克风音量与静音", available: true },
  input_source: { label: "输入法", hint: "当前输入法，点击切换", available: true },
  network: { label: "网络", hint: "Wi-Fi 开关、连接状态与 IP", available: true },
  vpn: { label: "VPN", hint: "VPN 连接状态（只读）", available: false },
  bluetooth: { label: "蓝牙", hint: "蓝牙开关与已连接设备电量", available: true },
};

const AVAILABLE: StatusBarSection[] = ["battery", "network", "bluetooth", "volume", "audio_input", "input_source"];
const UPCOMING: StatusBarSection[] = ["vpn"];

const CENTER_OPTIONS: { value: GlyphCenter; label: string }[] = [
  { value: "auto", label: "自动" },
  { value: "wifi", label: "Wi-Fi" },
  { value: "bluetooth", label: "蓝牙" },
  { value: "clipboard", label: "剪贴板" },
];

const APPEARANCE_OPTIONS: { value: GlyphAppearance; label: string }[] = [
  { value: "auto", label: "跟随系统" },
  { value: "dark", label: "深色菜单栏" },
  { value: "light", label: "浅色菜单栏" },
];

const VOLUME_STYLE_OPTIONS: { value: GlyphVolumeStyle; label: string }[] = [
  { value: "arc", label: "圆弧条" },
  { value: "dots", label: "四个点" },
];

/**
 * 菜单栏图标的实时预览。
 *
 * 图不是这里画的：后端把真正要贴进菜单栏的那张图（RGBA）原样送来，预览与实机
 * 因此不可能长得不一样。彩色模式下 RGB 已经带好电池色与墨色，直接贴；模板模式
 * 下只有 alpha 承载信息，系统会按菜单栏明暗反色，预览就照 `dark_bar` 补一次墨色。
 */
function GlyphPreview({ preview }: { preview: StatusBarGlyphPreview | null }) {
  const [url, setUrl] = useState<string | null>(null);

  useEffect(() => {
    if (!preview) return;
    const raw = Uint8Array.from(atob(preview.image), (c) => c.charCodeAt(0));
    const size = preview.size;
    const canvas = document.createElement("canvas");
    canvas.width = size;
    canvas.height = size;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    // RGB 全 0 = 模板图，需要预览自己下墨；否则原样显示后端算好的颜色。
    const template = raw.every((v, i) => i % 4 === 3 || v === 0);
    const ink = preview.dark_bar ? 255 : 0;
    const out = new ImageData(size, size);
    for (let i = 0; i < raw.length; i += 4) {
      out.data[i] = template ? ink : raw[i];
      out.data[i + 1] = template ? ink : raw[i + 1];
      out.data[i + 2] = template ? ink : raw[i + 2];
      out.data[i + 3] = raw[i + 3];
    }
    ctx.putImageData(out, 0, 0);
    setUrl(canvas.toDataURL());
  }, [preview]);

  const strip = preview?.dark_bar ? "bg-[#28282a]" : "bg-[#f4f4f6]";
  const icon = (px: number) =>
    url ? (
      <img src={url} width={px} height={px} alt="" />
    ) : (
      <span
        className="inline-block rounded-[4px] border border-dashed border-white/25"
        style={{ width: px, height: px }}
      />
    );

  return (
    <div className="mb-[14px] flex items-end gap-[14px]">
      <div className="flex flex-col items-center gap-[5px]">
        <div className={cx("flex h-[24px] w-[168px] items-center justify-end gap-[7px] rounded-t-[6px] px-[7px]", strip)}>
          <span className={cx("mr-auto text-[10px] leading-none", preview?.dark_bar ? "text-white/45" : "text-black/40")}>
            剪贴板 · 菜单栏
          </span>
          {icon(18)}
        </div>
        <span className="text-[11px]/[14px] text-white/35">真实大小</span>
      </div>
      <div className="flex flex-col items-center gap-[5px]">
        <div className={cx("flex h-[64px] w-[64px] items-center justify-center rounded-[8px]", strip)}>
          {icon(48)}
        </div>
        <span className="text-[11px]/[14px] text-white/35">放大</span>
      </div>
      <div className="min-w-0 flex-1 pb-[19px]">
        <div className="truncate text-[12px]/[16px] text-white/70">
          {preview?.tooltip ?? "读取系统状态中…"}
        </div>
        <div className="mt-[2px] text-[11px]/[15px] text-white/35">
          鼠标停在菜单栏图标上就是这句话；数值每两秒刷新一次。
        </div>
      </div>
    </div>
  );
}

export default function StatusBarSettings() {
  const config = useClipStore((s) => s.config);
  const patchConfig = useClipStore((s) => s.patchConfig);
  const statusbar = config?.statusbar;
  const [rows, setRows] = useState<StatusBarSection[]>([]);
  const [dragging, setDragging] = useState<StatusBarSection | null>(null);
  const [capabilities, setCapabilities] = useState<StatusBarCapability[]>([]);
  const [preview, setPreview] = useState<StatusBarGlyphPreview | null>(null);
  /** 每个模块最后一次出现的位置，用于关掉再打开时插回原位。 */
  const lastIndex = useRef<Partial<Record<StatusBarSection, number>>>({});

  const loadPreview = useCallback(() => {
    api
      .statusBarGlyphPreview()
      .then(setPreview)
      .catch(() => {});
  }, []);

  // 图标是每两秒重画的，预览跟着同一个节奏走。
  useEffect(() => {
    loadPreview();
    const timer = window.setInterval(loadPreview, 2000);
    return () => window.clearInterval(timer);
  }, [loadPreview]);

  const enabled = useMemo(
    () => new Set(statusbar?.sections.filter((id) => AVAILABLE.includes(id))),
    [statusbar],
  );

  // 本地顺序只在拖动时领先于配置；其它窗口改配置后以配置为准。
  // 未勾选的模块也留在列表里（排到已勾选项之后），否则取消勾选后就再也点不回来。
  useEffect(() => {
    if (!statusbar || dragging) return;
    const on = statusbar.sections.filter((id) => AVAILABLE.includes(id));
    const next = [...on, ...AVAILABLE.filter((id) => !on.includes(id))];
    next.forEach((id, index) => {
      if (on.includes(id)) lastIndex.current[id] = index;
    });
    setRows(next);
  }, [statusbar, dragging]);

  useEffect(() => {
    void api
      .statusBarState()
      .then((s) => setCapabilities(s.capabilities))
      .catch(() => setCapabilities([]));
  }, []);

  const save = (next: Partial<StatusBarConfig>) => {
    if (!statusbar) return;
    // 嵌套子块是整块替换（Rust `ConfigService::patch` 只做顶层合并），
    // 必须带全字段，否则其余项会被重置成默认值。
    void patchConfig({ statusbar: { ...statusbar, ...next } })
      .then(loadPreview)
      .catch((e) => toastError(e, "保存状态栏设置失败"));
  };

  /** 落库的只有勾选中的模块，顺序按列表里的上下位置。 */
  const persist = (nextRows: StatusBarSection[], nextEnabled: Set<StatusBarSection>) => {
    setRows(nextRows);
    save({ sections: nextRows.filter((id) => nextEnabled.has(id)) });
  };

  const toggle = (id: StatusBarSection, on: boolean) => {
    if (!statusbar) return;
    // macOS 14.4+ 没有定位授权就拿不到 Wi-Fi 名称，开关网络模块时顺手申请。
    if (on && id === "network") void api.statusBarRequestLocation();
    const nextEnabled = new Set(enabled);
    const nextRows = [...rows];
    if (on) {
      nextEnabled.add(id);
      const at = Math.min(lastIndex.current[id] ?? nextRows.length, nextRows.length - 1);
      nextRows.splice(nextRows.indexOf(id), 1);
      nextRows.splice(at, 0, id);
    } else {
      nextEnabled.delete(id);
      lastIndex.current[id] = nextRows.indexOf(id);
    }
    persist(nextRows, nextEnabled);
  };

  /** 拖动经过目标行时先只改本地顺序，松手才落库（避免每个像素一次 patch）。 */
  const moveOver = (target: StatusBarSection) => {
    if (!dragging || dragging === target) return;
    const from = rows.indexOf(dragging);
    const to = rows.indexOf(target);
    if (from < 0 || to < 0) return;
    const next = [...rows];
    next.splice(from, 1);
    next.splice(to, 0, dragging);
    setRows(next);
  };

  if (!statusbar) {
    return <p className="py-[32px] text-center text-[13px] text-white/45">加载中…</p>;
  }

  return (
    <div className="space-y-[16px]">
      <CardGroup title="菜单栏图标" subtitle="点图标弹出系统状态面板，右键可直达设置与退出">
        <ToggleRow
          label="显示状态栏图标"
          hint="关闭后菜单栏图标立即消失；剪贴板快捷键不受影响"
          checked={statusbar.enabled}
          onChange={(v) => save({ enabled: v })}
        />
      </CardGroup>

      <CardGroup
        title="图标内容"
        subtitle="一张图标说三件事：外环=电量，中心=网络，底部=输出音量"
      >
        <GlyphPreview preview={preview} />
        <div className="space-y-[2px]">
          <SelectRow
            label="中心图形"
            hint="自动：有网络就按类型画（iPhone 热点 / 有线 / 临时 / 共享 / Wi-Fi），其次蓝牙，都没有用剪贴板标识"
            value={statusbar.glyph_center}
            options={CENTER_OPTIONS}
            onChange={(v) => save({ glyph_center: v as GlyphCenter })}
            width="w-[120px]"
          />
          <Divider />
          <ToggleRow
            label="外环显示电量"
            hint="按电量填充，低于 20% 换成红色；台式机没有电池会自动不画"
            checked={statusbar.glyph_battery_ring}
            onChange={(v) => save({ glyph_battery_ring: v })}
          />
          <Divider />
          <ToggleRow
            label="缺口里写电量数字"
            hint="数字坐在环顶缺口；充电时缺口只留一枚闪电，接电源没在充时换成插头，两种都不写数字"
            checked={statusbar.glyph_battery_percent}
            onChange={(v) => save({ glyph_battery_percent: v })}
          />
          <Divider />
          <ToggleRow
            label="底部显示音量"
            hint="4 档对应输出音量，静音时全部转淡"
            checked={statusbar.glyph_volume}
            onChange={(v) => save({ glyph_volume: v })}
          />
          <SelectRow
            label="音量指示样式"
            hint="选择在状态环底部显示为四个离散点还是连续圆弧条"
            value={statusbar.glyph_volume_style}
            options={VOLUME_STYLE_OPTIONS}
            onChange={(v) => save({ glyph_volume_style: v as GlyphVolumeStyle })}
            width="w-[120px]"
          />
          <Divider />
          <ToggleRow
            label="彩色图标"
            hint="只给电量弧上色：绿=充电、红=低于 20%，其余情况和数字、中心图形一样都是墨色"
            checked={statusbar.glyph_colored}
            onChange={(v) => save({ glyph_colored: v })}
          />
          <SelectRow
            label="菜单栏明暗"
            hint="仅彩色图标生效：决定下白墨还是黑墨；不确定就选跟随系统"
            value={statusbar.glyph_appearance}
            options={APPEARANCE_OPTIONS}
            onChange={(v) => save({ glyph_appearance: v as GlyphAppearance })}
            width="w-[130px]"
          />
        </div>
        <div className="px-[4px] pb-[2px] pt-[14px]">
          <p className="text-[12px]/[16px] font-medium text-white/70">连接图标样式</p>
          <p className="mt-[2px] text-[11px]/[15px] text-white/40">
            启用后，对应状态改用普通 Wi‑Fi 信号图标，不再显示其专用标记。
          </p>
        </div>
        <div className="space-y-[2px]">
          <ToggleRow
            label="有线连接使用 Wi‑Fi 图标"
            hint="默认画双折线加三个触点的 RJ45 插头"
            checked={statusbar.glyph_wifi_for_ethernet}
            onChange={(v) => save({ glyph_wifi_for_ethernet: v })}
          />
          <Divider />
          <ToggleRow
            label="个人热点使用 Wi‑Fi 图标"
            hint="默认画两枚互扣的链环，一眼看出是手机热点"
            checked={statusbar.glyph_wifi_for_hotspot}
            onChange={(v) => save({ glyph_wifi_for_hotspot: v })}
          />
          <Divider />
          <ToggleRow
            label="临时连接使用 Wi‑Fi 图标"
            hint="默认把 Wi‑Fi 外弧收成实心扇形，再抠掉一台小显示器"
            checked={statusbar.glyph_wifi_for_temporary}
            onChange={(v) => save({ glyph_wifi_for_temporary: v })}
          />
          <Divider />
          <ToggleRow
            label="互联网共享使用 Wi‑Fi 图标"
            hint="默认把 Wi‑Fi 外弧收成实心扇形，再抠掉一支向下的箭头"
            checked={statusbar.glyph_wifi_for_sharing}
            onChange={(v) => save({ glyph_wifi_for_sharing: v })}
          />
        </div>
        <Notice tone="info" className="mt-[12px]">
          彩色模式下图标自己按菜单栏明暗选墨色；关掉彩色就退回模板图，由系统反色。
          两种模式下，转淡的弧线/圆点都表示「这一档没有」，不是没画。
        </Notice>
      </CardGroup>

      <CardGroup title="面板显示项目" subtitle="勾选要显示的模块，拖动调整上下顺序">
        <div className="space-y-[2px]">
          {enabled.size === 0 && (
            <p className="py-[8px] text-[12px]/[16px] text-white/45">
              当前没有勾选任何项目，面板只会保留底部的设置与退出。
            </p>
          )}
          {rows.map((id) => (
            <div
              key={id}
              draggable={enabled.has(id)}
              onDragStart={() => setDragging(id)}
              onDragOver={(e) => {
                e.preventDefault();
                moveOver(id);
              }}
              onDragEnd={() => {
                setDragging(null);
                persist(rows, enabled);
              }}
              className={cx(
                "flex min-h-[40px] items-center gap-[8px] rounded-[6px] px-[4px]",
                dragging === id && "bg-white/[0.06] opacity-70",
              )}
            >
              <span
                aria-hidden
                className={cx(
                  "select-none text-[13px] leading-none text-white/30",
                  enabled.has(id) ? "cursor-grab" : "cursor-default opacity-30",
                )}
                title="拖动排序"
              >
                ⋮⋮
              </span>
              <div className="min-w-0 flex-1">
                <div
                  className={cx(
                    "text-[13px]/[18px]",
                    enabled.has(id) ? "text-white/85" : "text-white/45",
                  )}
                >
                  {MODULES[id].label}
                </div>
                <div className="mt-[1px] text-[12px]/[16px] text-white/45">
                  {MODULES[id].hint}
                </div>
              </div>
              <Switch
                checked={enabled.has(id)}
                label={`显示${MODULES[id].label}`}
                onChange={(v) => toggle(id, v)}
              />
            </div>
          ))}
        </div>

        <Divider className="my-[10px]" />

        <p className="mb-[6px] text-[11px]/[14px] font-semibold uppercase tracking-wide text-white/35">
          即将支持
        </p>
        <div className="space-y-[2px]">
          {UPCOMING.map((id) => (
            <Row
              key={id}
              label={
                <span className="flex items-center gap-[6px]">
                  {MODULES[id].label}
                  <Pill className="pointer-events-none h-[18px] px-[6px] text-[10px]">
                    后续
                  </Pill>
                </span>
              }
              hint={MODULES[id].hint}
            >
              <Switch checked={false} disabled onChange={() => {}} label={MODULES[id].label} />
            </Row>
          ))}
        </div>
      </CardGroup>

      <CardGroup title="面板交互" subtitle="在面板空白处滚动滚轮即可调音量">
        <div className="space-y-[2px]">
          <ToggleRow
            label="滚轮调节音量"
            hint="光标在面板上滚动时直接调节音量，不滚动列表"
            checked={statusbar.scroll_adjusts_volume}
            onChange={(v) => save({ scroll_adjusts_volume: v })}
          />
          <Divider />
          <SelectRow
            label="调节方向"
            hint="滚轮作用于哪个设备的音量"
            value={statusbar.scroll_scope}
            options={[
              { value: "output", label: "输出音量" },
              { value: "input", label: "输入音量" },
            ]}
            onChange={(v) => save({ scroll_scope: v as "output" | "input" })}
            width="w-[120px]"
          />
          <Divider />
          <ToggleRow
            label="反转滚轮方向"
            hint="开启后向上滚动为降低音量（自然滚动手感）"
            checked={statusbar.scroll_natural}
            onChange={(v) => save({ scroll_natural: v })}
          />
        </div>
      </CardGroup>

      <CardGroup title="尚未接入" subtitle="需要私有框架或 root 守护进程，先保留接口">
        {capabilities.length === 0 ? (
          <p className="text-[12px]/[16px] text-white/45">当前版本没有额外能力待接入。</p>
        ) : (
          <div className="space-y-[8px]">
            {capabilities.map((cap) => (
              <div
                key={cap.id}
                className="flex items-start justify-between gap-[12px] rounded-[6px] bg-black/[0.12] px-[10px] py-[8px]"
              >
                <div className="min-w-0">
                  <div className="text-[13px]/[18px] text-white/80">{cap.name}</div>
                  {cap.reason && (
                    <div className="mt-[2px] text-[12px]/[16px] text-white/40">{cap.reason}</div>
                  )}
                </div>
                <Pill className="pointer-events-none h-[20px] shrink-0 px-[7px] text-[10px]">
                  后续版本
                </Pill>
              </div>
            ))}
          </div>
        )}
        <Notice tone="info" className="mt-[12px]">
          这些能力依赖 macOS 私有接口或 root 权限，不影响上面已接入的模块。
        </Notice>
      </CardGroup>
    </div>
  );
}
