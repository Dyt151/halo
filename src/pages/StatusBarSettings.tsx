/**
 * 设置 → 状态栏图标。
 *
 * 对应 status-trio 设置页的第三张图：菜单栏图标本身用「三区一张图」说状态
 * （外环=电量、中心=网络、底部=输出音量），面板里显示哪些模块、拖拽排序、
 * 面板滚动调音量。已接入电池 / 网络 / 蓝牙 / 音量 / 声音输入 / 输入法；VPN 与
 * 私有能力（AirPods 降噪、充电上限、iPhone 电量）在这里显式标「即将支持」，
 * 不做假开关。另外代管第三方「无线麦 SayAll」的入口：它自己只在菜单栏占一只图标，
 * Halo 这边给一个直接进它的按钮；隐藏那只图标的开关要 SayAll 2.0.0 起才有，旧版本
 * 只能退出它（图标随之消失），界面上按版本分两种给法，不给点了没反应的假开关。
 */
import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { useClipStore } from "../store/useClipStore";
import { api } from "../lib/ipc";
import { toastError } from "../lib/toast";
import type {
  GlyphAppearance,
  GlyphCenter,
  GlyphVolumeStyle,
  SayAllState,
  StatusBarConfig,
  StatusBarGlyphPreview,
  StatusBarSection,
} from "../lib/types";
import {
  Btn,
  CardGroup,
  Divider,
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
  vpn: { label: "VPN", hint: "代理接管开关、模式与节点延迟", available: true },
  bluetooth: { label: "蓝牙", hint: "蓝牙开关与已连接设备电量", available: true },
};

/** 与 Rust `STATUSBAR_AVAILABLE_SECTIONS` 同序：这一栏决定「全选」给到什么。 */
const AVAILABLE: StatusBarSection[] = [
  "battery",
  "network",
  "vpn",
  "bluetooth",
  "volume",
  "audio_input",
  "input_source",
];

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
function GlyphPreview({
  preview,
  enabled,
}: {
  preview: StatusBarGlyphPreview | null;
  enabled: boolean;
}) {
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
    <div
      className={cx(
        "rounded-[8px] border border-white/[0.06] bg-white/[0.03] p-[14px] transition-opacity duration-150",
        !enabled && "opacity-45",
      )}
    >
      <div className="flex flex-wrap items-center gap-x-[18px] gap-y-[12px]">
        <div className="flex flex-col items-center gap-[6px]">
          <div className={cx("flex h-[24px] w-[168px] items-center justify-end gap-[7px] rounded-[5px] px-[8px]", strip)}>
            <span className={cx("mr-auto text-[10px] leading-none", preview?.dark_bar ? "text-white/45" : "text-black/40")}>
              剪贴板 · 菜单栏
            </span>
            {icon(18)}
          </div>
          <span className="text-[10px]/[12px] text-white/35">真实大小</span>
        </div>
        <div className="flex flex-col items-center gap-[6px]">
          <div className={cx("flex h-[56px] w-[56px] items-center justify-center rounded-[8px]", strip)}>
            {icon(42)}
          </div>
          <span className="text-[10px]/[12px] text-white/35">放大</span>
        </div>
        <div className="min-w-[180px] flex-1">
          <div className="text-[12px]/[17px] text-white/70">
            {preview?.tooltip ?? "读取系统状态中…"}
          </div>
          <div className="mt-[3px] text-[11px]/[15px] text-white/35">
            鼠标悬停提示 · 每两秒刷新
          </div>
        </div>
      </div>
    </div>
  );
}

/** 默认折叠的高级项：只调一次的开关不该占据首屏。 */
function Disclosure({
  title,
  hint,
  children,
}: {
  title: string;
  hint: string;
  children: ReactNode;
}) {
  return (
    <details className="group rounded-[10px] bg-group">
      <summary className="flex cursor-pointer list-none items-center gap-[8px] rounded-[10px] px-[16px] py-[13px] outline-none transition-colors duration-120 hover:bg-white/[0.03] [&::-webkit-details-marker]:hidden">
        <svg
          viewBox="0 0 24 24"
          className="h-[13px] w-[13px] shrink-0 text-white/35 transition-transform duration-150 group-open:rotate-90"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
        >
          <path d="M9 6l6 6-6 6" />
        </svg>
        <span className="text-[13px]/[18px] font-semibold text-white">{title}</span>
        <span className="ml-auto text-[11px]/[14px] text-white/35">{hint}</span>
      </summary>
      <div className="border-t border-white/[0.06] px-[16px] pb-[16px] pt-[10px]">{children}</div>
    </details>
  );
}

export default function StatusBarSettings() {
  const config = useClipStore((s) => s.config);
  const patchConfig = useClipStore((s) => s.patchConfig);
  const statusbar = config?.statusbar;
  const [rows, setRows] = useState<StatusBarSection[]>([]);
  const [dragging, setDragging] = useState<StatusBarSection | null>(null);
  const [preview, setPreview] = useState<StatusBarGlyphPreview | null>(null);
  /** 无线麦 SayAll（第三方 App）：没装时整块卡片不出现。 */
  const [sayall, setSayall] = useState<SayAllState | null>(null);
  /** 改它的菜单栏开关会连带重启它，这期间不许连点。 */
  const [sayallBusy, setSayallBusy] = useState(false);
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

  // SayAll 是另一个 App，读数不在 Halo 自己的状态里：进这一页时查一次。
  useEffect(() => {
    let cancelled = false;
    api
      .sayAllState()
      .then((state) => {
        if (!cancelled) setSayall(state);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, []);

  const openSayAll = () => {
    api
      .sayAllOpen()
      .then(setSayall)
      .catch((e) => toastError(e, "打不开无线麦 SayAll"));
  };

  const setSayAllStatusBar = (visible: boolean) => {
    setSayallBusy(true);
    api
      .sayAllSetStatusBar(visible)
      .then(setSayall)
      .catch((e) => toastError(e, "改 SayAll 的菜单栏图标失败"))
      .finally(() => setSayallBusy(false));
  };

  const quitSayAll = () => {
    setSayallBusy(true);
    api
      .sayAllQuit()
      .then(setSayall)
      .catch((e) => toastError(e, "没能退出无线麦 SayAll"))
      .finally(() => setSayallBusy(false));
  };

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
      <CardGroup>
        <GlyphPreview preview={preview} enabled={statusbar.enabled} />
        <div className="mt-[14px] space-y-[2px]">
          <ToggleRow
            label="显示状态栏图标"
            checked={statusbar.enabled}
            onChange={(v) => save({ enabled: v })}
          />
          <Divider />
          <SelectRow
            label="中心图形"
            value={statusbar.glyph_center}
            options={CENTER_OPTIONS}
            onChange={(v) => save({ glyph_center: v as GlyphCenter })}
            width="w-[120px]"
          />
        </div>
      </CardGroup>

      <CardGroup title="面板显示项目">
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
              </div>
              <Switch
                checked={enabled.has(id)}
                label={`显示${MODULES[id].label}`}
                onChange={(v) => toggle(id, v)}
              />
            </div>
          ))}
        </div>
      </CardGroup>

      {sayall?.installed && (
        <CardGroup
          title="无线麦 SayAll"
          subtitle={`第三方 App${sayall.version ? ` ${sayall.version}` : ""} · ${
            sayall.running ? "正在运行" : "没在运行"
          }`}
        >
          <Row label="打开 SayAll" hint="没在运行时启动它，在跑时直接把它的窗口带到前台">
            <Btn size="sm" onClick={openSayAll}>
              打开
            </Btn>
          </Row>
          <Divider />
          {sayall.status_bar_icon_supported ? (
            <ToggleRow
              label="在菜单栏显示它的图标"
              hint="关掉后右上角不再挂它，改由 Halo 面板「声音输入」里进；改这一项会重启 SayAll"
              checked={sayall.status_bar_visible}
              disabled={sayallBusy}
              onChange={setSayAllStatusBar}
            />
          ) : (
            <Row
              label="退出 SayAll"
              hint="这个版本里没有「隐藏菜单栏图标」的设置，图标是无条件画的；退出它右上角就干净了，要用时点上面「打开」"
            >
              <Btn size="sm" onClick={quitSayAll} disabled={!sayall.running || sayallBusy}>
                退出
              </Btn>
            </Row>
          )}
        </CardGroup>
      )}

      <Disclosure title="高级设置" hint="图标细节、连接图标">
        <div className="space-y-[2px]">
          <ToggleRow
            label="外环显示电量"
            checked={statusbar.glyph_battery_ring}
            onChange={(v) => save({ glyph_battery_ring: v })}
          />
          <Divider />
          <ToggleRow
            label="缺口里写电量数字"
            checked={statusbar.glyph_battery_percent}
            onChange={(v) => save({ glyph_battery_percent: v })}
          />
          <Divider />
          <ToggleRow
            label="底部显示音量"
            checked={statusbar.glyph_volume}
            onChange={(v) => save({ glyph_volume: v })}
          />
          <SelectRow
            label="音量指示样式"
            value={statusbar.glyph_volume_style}
            options={VOLUME_STYLE_OPTIONS}
            onChange={(v) => save({ glyph_volume_style: v as GlyphVolumeStyle })}
            width="w-[120px]"
          />
          <Divider />
          <ToggleRow
            label="彩色图标"
            checked={statusbar.glyph_colored}
            onChange={(v) => save({ glyph_colored: v })}
          />
          <SelectRow
            label="菜单栏明暗"
            value={statusbar.glyph_appearance}
            options={APPEARANCE_OPTIONS}
            onChange={(v) => save({ glyph_appearance: v as GlyphAppearance })}
            width="w-[130px]"
          />
        </div>

        <div className="px-[4px] pb-[2px] pt-[16px]">
          <p className="text-[12px]/[16px] font-medium text-white/70">连接图标</p>
        </div>
        <div className="space-y-[2px]">
          <ToggleRow
            label="有线连接使用 Wi‑Fi 图标"
            checked={statusbar.glyph_wifi_for_ethernet}
            onChange={(v) => save({ glyph_wifi_for_ethernet: v })}
          />
          <Divider />
          <ToggleRow
            label="个人热点使用 Wi‑Fi 图标"
            checked={statusbar.glyph_wifi_for_hotspot}
            onChange={(v) => save({ glyph_wifi_for_hotspot: v })}
          />
          <Divider />
          <ToggleRow
            label="临时连接使用 Wi‑Fi 图标"
            checked={statusbar.glyph_wifi_for_temporary}
            onChange={(v) => save({ glyph_wifi_for_temporary: v })}
          />
          <Divider />
          <ToggleRow
            label="互联网共享使用 Wi‑Fi 图标"
            checked={statusbar.glyph_wifi_for_sharing}
            onChange={(v) => save({ glyph_wifi_for_sharing: v })}
          />
        </div>
      </Disclosure>
    </div>
  );
}
