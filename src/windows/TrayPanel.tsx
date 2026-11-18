/**
 * 菜单栏图标下拉面板（`traypanel` 窗口）。
 *
 * 结构与尺寸照抄 Status Trio 的 `StatusPopoverView`：330 宽、14 内边距、
 * 12 的行间距、纯分隔线（不画卡片）、24×24 图标列、13pt 标题 + 10pt 副标题、
 * 页脚「设置…」+ 省略号菜单。面板高度由内容决定（ResizeObserver 回报给
 * Rust，窗口贴着图标下方重排）；滚轮只用来滚动面板（不画滚动条），
 * 音量靠滑条或键盘；失焦由 Rust 收起。
 */
import { Fragment, useCallback, useEffect, useRef, useState } from "react";
import type { CSSProperties, ReactNode } from "react";
import { api } from "../lib/ipc";
import { errorMessage } from "../lib/errors";
import { useAppVersion } from "../lib/version";
import { chargeToFullPresentation } from "../lib/chargeToFull";
import type { ChargeToFullPhase, ChargeToFullPresentation } from "../lib/chargeToFull";
import { onTrayHidden, onTrayShown } from "../lib/windows";
import { useClipStore } from "../store/useClipStore";
import type {
  AudioScope,
  SayAllState,
  StatusBarAudioState,
  StatusBarBatteryDetails,
  StatusBarChargeToFullState,
  StatusBarInputSource,
  StatusBarSection,
  StatusBarSnapshot,
  StatusBarWifiScan,
} from "../lib/types";
import { Spinner, cx } from "../components/ui";
import {
  BatteryGlyph,
  BluetoothGlyph,
  CableGlyph,
  EllipsisCircleGlyph,
  GearGlyph,
  ICON_COLUMN,
  KeyboardGlyph,
  MicGlyph,
  PowerGlyph,
  RefreshGlyph,
  SpeakerGlyph,
  ShieldGlyph,
  bluetoothDeviceGlyph,
  WifiGlyph,
} from "./tray/glyphs";
import {
  AudioRow,
  ChargeToFullAction,
  DeviceRow,
  DetailActionButton,
  DIVIDER,
  ExpansionButton,
  IconAction,
  SummaryRow,
} from "./tray/rows";
import {
  BatteryDetailPage,
  InputSourceDetailPage,
  WifiDetailPage,
} from "./tray/detailPages";
import { ProxyNodesPage, ProxyOverviewPage } from "./tray/proxyPage";
import { useProxyPanel } from "./tray/useProxyPanel";
import { isStaleTakeover, proxySubtitle, proxyTitle } from "../lib/proxyPanel";
import {
  MAX_VISIBLE_BLUETOOTH_DEVICES,
  batterySubtitle,
  bluetoothSubtitle,
  networkSubtitle,
  networkTitle,
} from "./tray/format";

const DEFAULT_SECTIONS: StatusBarSection[] = [
  "battery",
  "network",
  "vpn",
  "bluetooth",
  "volume",
  "audio_input",
  "input_source",
];



/** 二级页（正版的 NavigationStack：一次只压一层，Esc 先返回再收起）。 */
type DetailPage = "battery" | "input_source" | "wifi" | "vpn";

export default function TrayPanel() {
  const config = useClipStore((s) => s.config);
  const version = useAppVersion();
  const [snapshot, setSnapshot] = useState<StatusBarSnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [detail, setDetail] = useState<DetailPage | null>(null);
  const [details, setDetails] = useState<StatusBarBatteryDetails | null>(null);
  const [detailsLoading, setDetailsLoading] = useState(false);
  /** PowerUI 充电上限读数：面板可见时每 2 秒拉一次。 */
  const [chargeState, setChargeState] = useState<StatusBarChargeToFullState | null>(null);
  const [chargePhase, setChargePhase] = useState<ChargeToFullPhase>({ kind: "idle" });
  /** Wi‑Fi 二级页读数：一次全信道扫描要一秒，只在进页/点刷新时拉。 */
  const [wifiScan, setWifiScan] = useState<StatusBarWifiScan | null>(null);
  const [wifiScanning, setWifiScanning] = useState(false);
  const [bluetoothExpanded, setBluetoothExpanded] = useState(false);
  /** 无线麦 SayAll（第三方 App）：只在面板弹出时查一次，不进 150ms 的读数轮询。 */
  const [sayall, setSayall] = useState<SayAllState | null>(null);
  const [menuOpen, setMenuOpen] = useState(false);
  /** 每次弹出加一：把「弹出补读数」的时序交给代理页的 hook，不在这里堆状态。 */
  const [shownAt, setShownAt] = useState(0);
  /** 开关类操作进行中：避免连点，也让用户看到状态在等系统。 */
  const [busy, setBusy] = useState<{ network: boolean; bluetooth: boolean }>({
    network: false,
    bluetooth: false,
  });

  const rootRef = useRef<HTMLDivElement | null>(null);
  const contentRef = useRef<HTMLDivElement | null>(null);
  /**
   * 每次弹出重放一次淡入。
   *
   * 状态栏面板是 hide 而非销毁，静态动画类只在冷启动播一遍；这里移除后
   * 强制回流再加回去，让同一段动画针对每次唤出重新开始。
   */
  const replayEntrance = useCallback(() => {
    const el = rootRef.current;
    if (!el) return;
    el.classList.remove("animate-fade-in");
    void el.offsetWidth;
    el.classList.add("animate-fade-in");
  }, []);
  const snapshotRef = useRef<StatusBarSnapshot | null>(null);
  const volumeTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  /** 音量提示音：正版用 `NSSound`，这里用同一份 wav 的 `Audio` 元素。 */
  const tickSound = useRef<HTMLAudioElement | null>(null);
  const lastTickAt = useRef(0);
  const feedbackEnabled = useRef(true);
  /** 正在拖动滑条 / 写入未落：轮询不许抢走控制权（正版同规则）。 */
  const editingVolume = useRef(false);

  const statusbar = config?.statusbar;
  const sections = (statusbar?.sections ?? DEFAULT_SECTIONS).filter((id) =>
    DEFAULT_SECTIONS.includes(id),
  );

  /** 代理页的读数、轮询与四种写操作；没开 VPN 模块时一条命令都不发。 */
  const proxy = useProxyPanel(sections.includes("vpn"), shownAt, detail === "vpn");

  const refresh = useCallback(async () => {
    try {
      const next = await api.statusBarState();
      snapshotRef.current = next;
      setSnapshot(next);
      setError(null);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setLoading(false);
    }
  }, []);

  /** 无线麦 SayAll 的读数：探测要读 plist，每次弹出查一次，失败就按没装处理。 */
  const refreshSayAll = useCallback(async () => {
    try {
      setSayall(await api.sayAllState());
    } catch {
      setSayall(null);
    }
  }, []);

  /** Wi‑Fi 二级页读数：正版只在进页与点刷新时扫一次，不做轮询（扫描要一秒）。 */
  const loadWifiPage = useCallback(async () => {
    setWifiScanning(true);
    try {
      setWifiScan(await api.statusBarWifiPage());
      setError(null);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setWifiScanning(false);
    }
  }, []);

  /** 正版 `VolumeFeedback`：只在标量真的变了才响，且 50ms 内最多一声。 */
  const playVolumeTick = useCallback(() => {
    if (!feedbackEnabled.current) return;
    const now = performance.now();
    if (now - lastTickAt.current < 50) return;
    lastTickAt.current = now;
    const audio = tickSound.current;
    if (!audio) return;
    audio.currentTime = 0;
    void audio.play().catch(() => {
      // 自动播放被拒：提示音属于锦上添花，不打扰用户。
    });
  }, []);

  useEffect(() => {
    const audio = new Audio("/VolumeFeedback.wav");
    audio.preload = "auto";
    audio.volume = 1;
    tickSound.current = audio;
    void api
      .statusBarVolumeFeedbackEnabled()
      .then((enabled) => {
        feedbackEnabled.current = enabled;
      })
      .catch(() => {});
    return () => {
      audio.pause();
      tickSound.current = null;
    };
  }, []);

  useEffect(() => {
    let unsubShow: (() => void) | undefined;
    let unsubHide: (() => void) | undefined;
    void refresh();
    void refreshSayAll();
    void onTrayShown(() => {
      setMenuOpen(false);
      setShownAt((count) => count + 1);
      replayEntrance();
      void refresh();
      void refreshSayAll();
    }).then((fn) => {
      unsubShow = fn;
    });
    void onTrayHidden(() => {
      setDetail(null);
    }).then((fn) => {
      unsubHide = fn;
    });
    // 面板可见期间每 150ms 跟一次系统音量：键盘/触控栏调音量时滑条要动。
    const poll = setInterval(() => {
      if (document.hidden) return;
      if (editingVolume.current) return;
      void api
        .statusBarAudioLevels()
        .then((levels) => {
          const current = snapshotRef.current;
          if (!current) return;
          const merge = (
            state: StatusBarAudioState,
            next: StatusBarAudioState,
          ): StatusBarAudioState =>
            state.available &&
            next.available &&
            (state.volume !== next.volume || state.muted !== next.muted)
              ? { ...state, volume: next.volume, muted: next.muted }
              : state;
          const output = merge(current.audio_output, levels.output);
          const input = merge(current.audio_input, levels.input);
          if (output === current.audio_output && input === current.audio_input) return;
          const next: StatusBarSnapshot = { ...current, audio_output: output, audio_input: input };
          snapshotRef.current = next;
          setSnapshot(next);
        })
        .catch(() => {});
    }, 150);
    return () => {
      clearInterval(poll);
      unsubShow?.();
      unsubHide?.();
    };
  }, [refresh, replayEntrance]);
  // 电池详情页：正版打开时立刻读一次，之后有功率就 2 秒、没有就 15 秒。
  useEffect(() => {
    if (detail !== "battery") return;
    let cancelled = false;
    setDetailsLoading(true);
    const load = () => {
      void api
        .statusBarBatteryDetails()
        .then((next) => {
          if (!cancelled) setDetails(next);
        })
        .catch((e) => {
          if (!cancelled) setError(errorMessage(e));
        })
        .finally(() => {
          if (!cancelled) setDetailsLoading(false);
        });
    };
    load();
    const interval = setInterval(load, 2000);
    return () => {
      cancelled = true;
      clearInterval(interval);
    };
  }, [detail]);

  // 「立即充满电」读数：只在面板可见时每 2 秒读一次，打开面板立刻读。
  useEffect(() => {
    let cancelled = false;
    let unsub: (() => void) | undefined;
    const load = () => {
      if (document.hidden) return;
      void api
        .statusBarChargeToFullState()
        .then((next) => {
          if (!cancelled) setChargeState(next);
        })
        .catch(() => {});
    };
    load();
    const interval = setInterval(load, 2000);
    void onTrayShown(() => {
      setChargePhase((prev) =>
        prev.kind === "failed" || prev.kind === "cancelFailed" ? { kind: "idle" } : prev,
      );
      load();
    }).then((fn) => {
      unsub = fn;
    });
    return () => {
      cancelled = true;
      clearInterval(interval);
      unsub?.();
    };
  }, []);

  // 内容高度 → 窗口高度（Rust 会 clamp 到 720 并重新贴到图标下方）。
  useEffect(() => {
    const el = contentRef.current;
    if (!el) return;
    const report = () => {
      const height = Math.ceil(el.getBoundingClientRect().height) + 2;
      void api.statusBarResize(height).catch(() => {});
    };
    report();
    const observer = new ResizeObserver(report);
    observer.observe(el);
    return () => observer.disconnect();
  }, [snapshot, details, wifiScan, wifiScanning, sections.length, detail, bluetoothExpanded, menuOpen, proxy]);

  // Esc：正版先返回上一层，已在根页面才收起面板（窗口失焦由 Rust 处理）。
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      if (menuOpen) {
        setMenuOpen(false);
        return;
      }
      // 节点清单是代理页里的第三层：先退回概览，再退才算离开这一格。
      if (proxy.group) {
        proxy.closeGroup();
        return;
      }
      if (detail) {
        setDetail(null);
        return;
      }
      void api.statusBarHide();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [detail, menuOpen, proxy]);

  // 离开代理页时退回概览：下次进来不该还停在某个组的节点清单上。
  useEffect(() => {
    if (detail !== "vpn") proxy.closeGroup();
  }, [detail, proxy]);

  const setVolumeOptimistic = useCallback(
    (scope: AudioScope, value: number) => {
      const current = snapshotRef.current;
      if (!current) return;
      const key = scope === "input" ? "audio_input" : "audio_output";
      const state = current[key];
      if (!state.available || !state.volume_available) return;
      const volume = Math.min(1, Math.max(0, value));
      const changed = Math.abs(state.volume - volume) > 0.0001;
      const next: StatusBarSnapshot = {
        ...current,
        [key]: { ...state, volume, muted: false },
      };
      snapshotRef.current = next;
      setSnapshot(next);
      if (changed) {
        // 拖动/滚轮改音量期间暂停 500ms 轮询，否则硬件读数会把滑条拽回去。
        editingVolume.current = true;
        playVolumeTick();
      }

      if (volumeTimer.current) clearTimeout(volumeTimer.current);
      volumeTimer.current = setTimeout(() => {
        void api
          .statusBarSetVolume(scope, volume)
          .catch((e) => setError(errorMessage(e)))
          .finally(() => {
            editingVolume.current = false;
          });
      }, 50);
    },
    [playVolumeTick],
  );

  const toggleMute = async (scope: AudioScope, muted: boolean) => {
    const current = snapshotRef.current;
    if (!current) return;
    const key = scope === "input" ? "audio_input" : "audio_output";
    const next: StatusBarSnapshot = {
      ...current,
      [key]: { ...current[key], muted },
    };
    snapshotRef.current = next;
    setSnapshot(next);
    try {
      await api.statusBarSetMute(scope, muted);
    } catch (e) {
      setError(errorMessage(e));
      void refresh();
    }
  };

  const pickDevice = async (scope: AudioScope, id: number) => {
    try {
      await api.statusBarSetDefaultDevice(scope, id);
      await refresh();
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  const pickInputSource = async (id: string) => {
    try {
      await api.statusBarSelectInputSource(id);
      await refresh();
    } catch (e) {
      setError(errorMessage(e));
      // 多半是这一行刚在系统里被关掉了：重读一遍，让列表立刻只剩真能用的输入法，
      // 而不是留着一行点了只报错的死条目。
      void refresh();
    }
  };

  /** 打开输入法自带的设置 App；路径由后端在扫描时给出，这里只负责报错。 */
  const openImeSettings = async (source: StatusBarInputSource) => {
    if (source.settings_app === null) return;
    try {
      await api.statusBarOpenImeSettings(source.settings_app);
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  /** 进到无线麦 SayAll：后端 `open` 它的 App，在跑时会直接开它自己的窗口。 */
  const openSayAll = async () => {
    try {
      setSayall(await api.sayAllOpen());
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  /**
   * Wi-Fi 名称授权（正版 `requestWiFiNameAccess`）：能弹就弹，弹不出来就直接把
   * 用户送到系统「定位服务」——不能只留一句「允许定位」却没有入口。
   */
  const requestNameAccess = async () => {
    try {
      const outcome = await api.statusBarRequestLocation();
      if (outcome === "open_location_settings") {
        void api.statusBarOpenPane("location");
        return;
      }
      if (outcome === "requested") window.setTimeout(() => void refresh(), 1500);
      else await refresh();
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  /** Wi-Fi 开关：先乐观更新，再等系统生效后重读（开关链路要 ~1s）。 */
  const toggleWifi = async (on: boolean) => {
    const current = snapshotRef.current;
    if (!current) return;
    const optimistic: StatusBarSnapshot = {
      ...current,
      network: {
        ...current.network,
        powered: on,
        connected: on ? current.network.connected : false,
        ssid: on ? current.network.ssid : null,
        rssi: on ? current.network.rssi : null,
        ipv4: on ? current.network.ipv4 : null,
        link: on ? current.network.link : "none",
      },
    };
    snapshotRef.current = optimistic;
    setSnapshot(optimistic);
    setBusy((prev) => ({ ...prev, network: true }));
    try {
      await api.statusBarSetWifi(on);
      window.setTimeout(() => void refresh(), 900);
    } catch (e) {
      setError(errorMessage(e));
      void refresh();
    } finally {
      setBusy((prev) => ({ ...prev, network: false }));
    }
  };

  // 进入 Wi‑Fi 页：立刻扫一次（正版 `refreshWiFi` 挂在 `.onAppear` 与刷新按钮上）。
  useEffect(() => {
    if (detail !== "wifi") return;
    void loadWifiPage();
  }, [detail, loadWifiPage]);

  /** 页里的 Wi‑Fi 开关：先按快照回显，等系统生效再重扫（否则列表还是旧的）。 */
  const toggleWifiFromPage = async (on: boolean) => {
    await toggleWifi(on);
    window.setTimeout(() => void loadWifiPage(), 1000);
  };

  /** 页里的「允许定位以显示 Wi‑Fi 名称」：答完授权弹窗后重扫一次。 */
  const grantNameAccessFromPage = async () => {
    await requestNameAccess();
    window.setTimeout(() => void loadWifiPage(), 2000);
  };

  /** 页里 Wi‑Fi 行的落点（正版 `onOpenWiFiSettings`）：系统和它自己的按钮走同一个面板。 */
  const openWifiSettingsFromPage = () => void api.statusBarOpenPane("wifi");

  /** 蓝牙开关：系统切换比 Wi-Fi 更慢，重读就一次。 */
  const toggleBluetooth = async (on: boolean) => {
    const current = snapshotRef.current;
    if (!current) return;
    const optimistic: StatusBarSnapshot = {
      ...current,
      bluetooth: {
        ...current.bluetooth,
        powered: on,
        devices: on ? current.bluetooth.devices : [],
      },
    };
    snapshotRef.current = optimistic;
    setSnapshot(optimistic);
    setBusy((prev) => ({ ...prev, bluetooth: true }));
    try {
      await api.statusBarSetBluetooth(on);
      window.setTimeout(() => void refresh(), 1200);
    } catch (e) {
      setError(errorMessage(e));
      void refresh();
    } finally {
      setBusy((prev) => ({ ...prev, bluetooth: false }));
    }
  };

  /** 当前展示态：摘要行和详情页共用同一份判定。 */
  const chargeToFull = snapshot
    ? chargeToFullPresentation(snapshot.battery, chargeState, chargePhase, Date.now())
    : ({ kind: "hidden" } as ChargeToFullPresentation);

  const startChargeToFull = async () => {
    const limit = chargeState?.limit ?? null;
    setChargePhase({ kind: "applying" });
    try {
      await api.statusBarChargeToFull();
      setChargePhase({ kind: "accepted", limit, at: Date.now() });
    } catch {
      setChargePhase({ kind: "failed" });
    }
    void api
      .statusBarChargeToFullState()
      .then(setChargeState)
      .catch(() => {});
  };

  const cancelChargeToFull = async () => {
    setChargePhase({ kind: "cancelling" });
    try {
      const limit = await api.statusBarCancelChargeToFull();
      setChargePhase({ kind: "restored", limit, at: Date.now() });
    } catch (e) {
      setChargePhase({ kind: "cancelFailed", message: errorMessage(e) });
    }
    void api
      .statusBarChargeToFullState()
      .then(setChargeState)
      .catch(() => {});
  };

  const runChargeToFull = () => {
    switch (chargeToFull.kind) {
      case "available":
      case "failed":
        void startChargeToFull();
        break;
      case "chargingToFull":
      case "cancelFailed":
        void cancelChargeToFull();
        break;
      default:
        break;
    }
  };

  const renderSection = (id: StatusBarSection): ReactNode => {
    if (!snapshot) return null;

    if (id === "battery") {
      const { battery } = snapshot;
      if (!battery.present) return null;
      const tint =
        battery.percent < 20
          ? "text-[color:var(--danger-fill)]"
          : battery.charging || battery.plugged
            ? "text-[color:var(--success)]"
            : "text-white/90";
      return (
        <div className="flex flex-col gap-[8px]">
          <SummaryRow
            icon={<BatteryGlyph percent={battery.percent} charging={battery.charging || battery.plugged} />}
            iconClass={tint}
            title={`电池 · ${battery.percent}%`}
            subtitle={batterySubtitle(battery)}
            chevron
            onActivate={() => setDetail("battery")}
            trailing={
              <IconAction
                label="打开电池设置"
                onClick={() => void api.statusBarOpenPane("battery")}
              >
                <GearGlyph />
              </IconAction>
            }
          />
          <ChargeToFullAction presentation={chargeToFull} onAction={runChargeToFull} />
        </div>
      );
    }

    if (id === "network") {
      const network = snapshot.network;
      if (!network.available) return null;
      // 正版把有线链路路由到另一页（`EthernetLinkView`），这里先不给入口。
      const isWired = network.link === "ethernet";
      return (
        <SummaryRow
          icon={network.link === "ethernet" ? <CableGlyph /> : <WifiGlyph powered={network.powered} />}
          iconClass="text-white/60"
          title={networkTitle(network)}
          subtitle={networkSubtitle(network)}
          chevron={!isWired}
          onActivate={isWired ? undefined : () => setDetail("wifi")}
          trailing={
            <>
              <IconAction
                label={network.powered ? "关闭 Wi-Fi" : "打开 Wi-Fi"}
                disabled={busy.network}
                onClick={() => void toggleWifi(!network.powered)}
                className={
                  network.powered
                    ? "text-[color:var(--panel-accent)] hover:text-[color:var(--panel-accent)]"
                    : undefined
                }
              >
                <PowerGlyph />
              </IconAction>
              <IconAction
                label="打开 Wi-Fi 设置"
                onClick={() => void api.statusBarOpenPane("wifi")}
              >
                <GearGlyph />
              </IconAction>
            </>
          }
        />
      );
    }

    if (id === "vpn") {
      // 代理的读数不在快照里（要起子进程问 networksetup 和控制口），单独一轮拉取。
      const stale = isStaleTakeover(proxy.page);
      return (
        <SummaryRow
          icon={<ShieldGlyph checked={proxy.page?.takeover === true} />}
          iconClass={
            stale
              ? "text-[#ff9f0a]"
              : proxy.page?.takeover
                ? "text-[color:var(--panel-accent)]"
                : "text-white/60"
          }
          title={proxyTitle(proxy.page)}
          subtitle={proxySubtitle(proxy.page, proxy.speed)}
          chevron
          onActivate={() => setDetail("vpn")}
          trailing={
            <IconAction label="打开 FlClash" onClick={proxy.openApp}>
              <GearGlyph />
            </IconAction>
          }
        />
      );
    }

    if (id === "bluetooth") {
      const bluetooth = snapshot.bluetooth;
      if (!bluetooth.available) return null;
      const visibleBluetoothDevices = bluetoothExpanded
        ? bluetooth.devices
        : bluetooth.devices.slice(0, MAX_VISIBLE_BLUETOOTH_DEVICES);
      return (
        <div className="flex flex-col gap-[8px]">
          <div className="flex items-center gap-[10px]">
            <span className={cx(ICON_COLUMN, "text-white/60")} aria-hidden>
              <BluetoothGlyph />
            </span>
            <span className="flex min-w-0 flex-1 flex-col gap-[2px]">
              <span className="truncate text-[13px]/[17px] font-semibold text-white/90">蓝牙</span>
              <span className="truncate text-[10px]/[13px] text-white/50">
                {bluetoothSubtitle(bluetooth)}
              </span>
            </span>
            {/* 正版顺序：电源 → 刷新 → 设置。 */}
            <IconAction
              label={bluetooth.powered ? "关闭蓝牙" : "打开蓝牙"}
              disabled={busy.bluetooth}
              onClick={() => void toggleBluetooth(!bluetooth.powered)}
              className={
                bluetooth.powered
                  ? "text-[color:var(--panel-accent)] hover:text-[color:var(--panel-accent)]"
                  : undefined
              }
            >
              <PowerGlyph />
            </IconAction>
            <IconAction label="刷新蓝牙设备" disabled={busy.bluetooth} onClick={() => void refresh()}>
              <RefreshGlyph />
            </IconAction>
            <IconAction
              label="打开蓝牙设置"
              onClick={() => void api.statusBarOpenPane("bluetooth")}
            >
              <GearGlyph />
            </IconAction>
          </div>

          {bluetooth.powered && bluetooth.devices.length > 0 && (
            <>
              <div className="flex flex-col gap-[2px]">
                {visibleBluetoothDevices.map((device) => (
                  <DeviceRow
                    key={device.address ?? device.name}
                    icon={bluetoothDeviceGlyph(device.kind)}
                    name={device.name}
                    selected={device.connected}
                    trailing={
                      device.connected && device.battery !== null && (
                        <span className="shrink-0 text-[10px]/[13px] tabular-nums text-white/50">
                          {device.battery}%
                        </span>
                      )
                    }
                  />
                ))}
              </div>
              {bluetooth.devices.length > MAX_VISIBLE_BLUETOOTH_DEVICES && (
                <ExpansionButton
                  expanded={bluetoothExpanded}
                  onToggle={() => setBluetoothExpanded((open) => !open)}
                />
              )}
            </>
          )}
        </div>
      );
    }

    if (id === "volume" || id === "audio_input") {
      const scope: AudioScope = id === "volume" ? "output" : "input";
      const state = id === "volume" ? snapshot.audio_output : snapshot.audio_input;
      const devices = state.devices;
      const showsDevices = scope === "output" ? devices.length > 1 : devices.length > 0;
      return (
        <div className="flex flex-col gap-[8px]">
          <AudioRow
            scope={scope}
            state={state}
            onVolume={setVolumeOptimistic}
            onMute={(s, muted) => void toggleMute(s, muted)}
          />

          {showsDevices && (
            <>
              <div className={cx(DIVIDER, "mt-[2px]")} />
              <div className="flex flex-col gap-[2px]">
                {devices.map((device) => (
                  <DeviceRow
                    key={device.id}
                    icon={
                      scope === "output" ? (
                        <SpeakerGlyph muted={false} className="h-[12px] w-[12px]" />
                      ) : (
                        <MicGlyph muted={false} className="h-[12px] w-[12px]" />
                      )
                    }
                    name={device.name}
                    selected={device.is_default}
                    onSelect={
                      device.is_default ? undefined : () => void pickDevice(scope, device.id)
                    }
                  />
                ))}
              </div>
            </>
          )}

          {/* 无线麦 SayAll 是第三方 App，它自己只在菜单栏放一只鸭子，没有别的入口。
              Halo 这里接上同一个跳转（和微信输入法设置一个路子）；没装就什么都不显示。 */}
          {id === "audio_input" && sayall?.installed && (
            <>
              <div className={cx(DIVIDER, "mt-[2px]")} />
              <DetailActionButton label="打开无线麦 SayAll" onClick={() => void openSayAll()} />
            </>
          )}
        </div>
      );
    }

    if (id === "input_source") {
      const sources = snapshot.input_sources;
      if (sources.length === 0) return null;
      const current = sources.find((source) => source.selected);
      return (
        <SummaryRow
          icon={<KeyboardGlyph />}
          iconClass="text-white/60"
          title="输入法"
          subtitle={current?.name ?? "未选择输入法"}
          chevron
          onActivate={() => setDetail("input_source")}
          trailing={
            <IconAction
              label="打开键盘设置"
              onClick={() => void api.statusBarOpenPane("keyboard")}
            >
              <GearGlyph />
            </IconAction>
          }
        />
      );
    }

    return null;
  };

  const renderedSections = sections
    .map((id) => ({ id, node: renderSection(id) }))
    .filter((entry) => entry.node !== null);

  /** 正版 `StatusPopoverView`：二级页整页替换摘要（含页脚），只留 14pt 内边距。 */
  const detailPage =
    detail === "battery" && snapshot ? (
      <BatteryDetailPage
        battery={snapshot.battery}
        details={details}
        loading={detailsLoading}
        chargeToFull={chargeToFull}
        onChargeToFull={runChargeToFull}
        onBack={() => setDetail(null)}
      />
    ) : detail === "input_source" && snapshot ? (
      <InputSourceDetailPage
        sources={snapshot.input_sources}
        onBack={() => setDetail(null)}
        onSelect={(id) => void pickInputSource(id)}
        onOpenSettings={(source) => void openImeSettings(source)}
      />
    ) : detail === "wifi" && snapshot ? (
      <WifiDetailPage
        scan={wifiScan}
        powered={snapshot.network.powered}
        scanning={wifiScanning}
        busy={busy.network}
        onBack={() => setDetail(null)}
        onRefresh={() => void loadWifiPage()}
        onTogglePower={(on) => void toggleWifiFromPage(on)}
        onRequestNameAccess={() => void grantNameAccessFromPage()}
        onOpenLocationSettings={() => void api.statusBarOpenPane("location")}
        onOpenSettings={openWifiSettingsFromPage}
      />
    ) : detail === "vpn" ? (
      proxy.group ? (
        <ProxyNodesPage
          group={proxy.group}
          nodes={proxy.nodes}
          testing={proxy.testing}
          busy={proxy.busy}
          notice={proxy.notice}
          onBack={proxy.closeGroup}
          onSelect={proxy.selectNode}
          onTestLatency={proxy.testLatency}
          onDismissNotice={proxy.dismissNotice}
        />
      ) : (
        <ProxyOverviewPage
          page={proxy.page}
          loading={proxy.loading}
          busy={proxy.busy}
          notice={proxy.notice}
          autoSwitch={proxy.autoSwitch}
          autoSwitchBusy={proxy.autoSwitchBusy}
          onBack={() => setDetail(null)}
          onRefresh={proxy.reload}
          onTakeover={proxy.setTakeover}
          onMode={proxy.setMode}
          onOpenGroup={proxy.openGroup}
          onOpenApp={proxy.openApp}
          onDismissNotice={proxy.dismissNotice}
          onAutoSwitchConfig={proxy.setAutoSwitchConfig}
          onRouterSecret={proxy.setRouterSecret}
          onCheckAutoSwitch={proxy.checkAutoSwitchNow}
        />
      )
    ) : null;

  return (
    <div
      ref={rootRef}
      style={{ "--panel-accent": snapshot?.accent ?? "#007aff" } as CSSProperties}
      className="app-surface app-surface--panel h-full w-full overflow-y-auto"
    >
      <div
        ref={contentRef}
        key={detail ?? "summary"}
        className="flex w-full animate-fade-in flex-col gap-[12px] px-[14px] py-[14px] text-white"
      >
        {detailPage ? (
          <>
            {detailPage}
            {error && (
              <p className="text-[10px]/[14px] text-[#ff9f0a]">
                <span aria-hidden>⚠ </span>
                {error}
              </p>
            )}
          </>
        ) : (
          <>
            {loading && !snapshot ? (
              <div className="flex items-center justify-center gap-[8px] py-[18px] text-[12px] text-white/50">
                <Spinner />
                正在读取系统状态…
              </div>
            ) : (
              renderedSections.map(({ id, node }, index) => (
                <Fragment key={id}>
                  {index > 0 && <div className={DIVIDER} />}
                  {node}
                </Fragment>
              ))
            )}

            {error && (
              <p className="text-[10px]/[14px] text-[#ff9f0a]">
                <span aria-hidden>⚠ </span>
                {error}
              </p>
            )}

            {!loading && renderedSections.length === 0 && (
              <p className="py-[10px] text-center text-[12px] text-white/50">
                没有启用的面板项目，可在「设置 → 状态栏图标」里勾选。
              </p>
            )}

            <div className={cx(DIVIDER, "my-[2px] opacity-60")} />

            <div className="flex items-center gap-[8px] text-[12px]/[16px] text-white/60">
              <button
                type="button"
                onClick={() => void api.statusBarOpenSettings()}
                className="flex min-h-[24px] min-w-0 flex-1 items-center gap-[6px] text-left outline-none hover:text-white"
              >
                <GearGlyph className="h-[13px] w-[13px] shrink-0" />
                <span className="truncate">设置…</span>
              </button>

              {version && (
                <span className="shrink-0 tabular-nums text-white/45">{version}</span>
              )}

              <div className="relative flex shrink-0 items-center">
                <button
                  type="button"
                  aria-label="更多"
                  title="更多"
                  aria-expanded={menuOpen}
                  onClick={() => setMenuOpen((open) => !open)}
                  className="grid h-[24px] w-[24px] place-items-center rounded-[5px] outline-none hover:bg-white/[0.08] hover:text-white"
                >
                  <EllipsisCircleGlyph />
                </button>

                {menuOpen && (
                  <>
                    <div className="fixed inset-0 z-10" onClick={() => setMenuOpen(false)} />
                    <div className="menu absolute bottom-[28px] right-0 z-20 animate-pop-in !min-w-[160px]">
                      <button
                        type="button"
                        className="menu-item"
                        onClick={() => {
                          setMenuOpen(false);
                          void api.quitApp();
                        }}
                      >
                        退出 Halo
                      </button>
                    </div>
                  </>
                )}
              </div>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
