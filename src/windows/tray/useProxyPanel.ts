/**
 * 代理（VPN）页的状态与轮询集中在这里。
 *
 * TrayPanel 已经有近千行，把「读页面、读网速、四种写操作、三级页的当前组」这套
 * 状态机单独放出来，一是让面板本体只负责画，二是这些时序（写操作期间禁掉整页、
 * 延迟测试覆盖读数、面板收起清通知）本身就有规则可讲。
 */
import { useCallback, useEffect, useMemo, useState } from "react";
import { api } from "../../lib/ipc";
import { errorMessage } from "../../lib/errors";
import { outcomeNotice } from "../../lib/proxyPanel";
import type {
  AutoSwitchConfig,
  AutoSwitchState,
  ProxyMode,
  StatusBarProxyGroup,
  StatusBarProxyNode,
  StatusBarProxyOutcome,
  StatusBarProxyPage,
  StatusBarProxySpeed,
} from "../../lib/types";

export interface ProxyPanelState {
  page: StatusBarProxyPage | null;
  speed: StatusBarProxySpeed | null;
  /** 整页读数在飞（页面打开、点刷新）。 */
  loading: boolean;
  /** 某一次写操作在飞：这段时间整页控件禁用，避免连点把系统代理写成半截状态。 */
  busy: boolean;
  /** 上一次写操作的失败原因；null 表示没有要提醒的。 */
  notice: string | null;
  /** 当前打开的三级组；null 表示停在概览。 */
  group: StatusBarProxyGroup | null;
  /** 一轮延迟测试的节点结果；null 表示沿用读数里的延迟。 */
  nodes: StatusBarProxyNode[] | null;
  testing: boolean;
  /** 自动换节点卡片读数；null 表示还没读到。 */
  autoSwitch: AutoSwitchState | null;
  /** 自动换节点的写操作/立即测速在飞。 */
  autoSwitchBusy: boolean;
  reload: () => void;
  dismissNotice: () => void;
  openGroup: (group: StatusBarProxyGroup) => void;
  closeGroup: () => void;
  setTakeover: (enabled: boolean) => void;
  setMode: (mode: ProxyMode) => void;
  selectNode: (node: string) => void;
  testLatency: () => void;
  openApp: () => void;
  setAutoSwitchConfig: (config: AutoSwitchConfig) => void;
  setRouterSecret: (secret: string) => void;
  checkAutoSwitchNow: () => void;
}

/**
 * @param enabled 用户是否在设置里开了 VPN 模块；没开就不起任何子进程。
 * @param shownAt 每次面板弹出的计数，用来在弹出瞬间补一次读数。
 * @param detailOpen 代理页是否正显示；只有显示时才做 2 秒一轮的网速轮询。
 */
export function useProxyPanel(
  enabled: boolean,
  shownAt: number,
  detailOpen: boolean,
): ProxyPanelState {
  const [page, setPage] = useState<StatusBarProxyPage | null>(null);
  const [speed, setSpeed] = useState<StatusBarProxySpeed | null>(null);
  const [loading, setLoading] = useState(false);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [group, setGroup] = useState<StatusBarProxyGroup | null>(null);
  const [nodes, setNodes] = useState<StatusBarProxyNode[] | null>(null);
  const [testing, setTesting] = useState(false);
  const [autoSwitch, setAutoSwitch] = useState<AutoSwitchState | null>(null);
  const [autoSwitchBusy, setAutoSwitchBusy] = useState(false);

  const apply = useCallback((next: StatusBarProxyPage | null) => {
    setPage(next);
    // 组对象来自旧读数：换了新读数要把三级页跟着换，否则切完节点还在看老名单。
    setGroup((current) => {
      if (!current) return null;
      return next?.groups.find((item) => item.name === current.name) ?? null;
    });
    setNodes(null);
  }, []);

  const reload = useCallback(() => {
    if (!enabled) return;
    setLoading(true);
    void api
      .statusBarProxyPage()
      .then(apply)
      .catch((e) => setNotice(errorMessage(e)))
      .finally(() => setLoading(false));
  }, [apply, enabled]);

  // 弹出补一次读数：接管状态在 FlClash 里也可能被改，不能只靠缓存。
  // 窗口是开机就挂在后台的，没弹出过就别读——那一串 networksetup 子进程不该出现在启动路径上。
  useEffect(() => {
    if (!enabled) {
      apply(null);
      setSpeed(null);
      return;
    }
    if (shownAt > 0) reload();
  }, [enabled, shownAt, reload, apply]);

  // 网速：页面开着才轮询，一次只是一条控制口请求或一次 netstat。
  useEffect(() => {
    if (!enabled || !detailOpen) {
      setSpeed(null);
      return;
    }
    const load = () => {
      void api
        .statusBarProxySpeed()
        .then(setSpeed)
        .catch(() => {});
    };
    load();
    const timer = setInterval(() => {
      if (!document.hidden) load();
    }, 2000);
    return () => clearInterval(timer);
  }, [enabled, detailOpen]);

  // 自动换节点卡片：页面开着才轮询（一次只是读内存状态 + 一次钥匙串查询）。
  useEffect(() => {
    if (!enabled || !detailOpen) return;
    const load = () => {
      void api
        .statusBarAutoSwitchState()
        .then(setAutoSwitch)
        .catch(() => {});
    };
    load();
    const timer = setInterval(() => {
      if (!document.hidden) load();
    }, 10000);
    return () => clearInterval(timer);
  }, [enabled, detailOpen]);

  /** 四种写操作的公共外壳：禁用整页、把失败原因写成页面上的话、无论成败都重读。 */
  const run = useCallback(
    async (task: () => Promise<StatusBarProxyOutcome>) => {
      setBusy(true);
      setNotice(null);
      try {
        const text = outcomeNotice(await task());
        if (text) setNotice(text);
        // 失败也可能改了一半系统代理（回滚过），重读一次让用户看到真实状态。
        reload();
      } catch (e) {
        setNotice(errorMessage(e));
      } finally {
        setBusy(false);
      }
    },
    [reload],
  );

  const selectNode = useCallback(
    (node: string) => {
      const current = group;
      if (!current) return;
      void run(() => api.statusBarProxySelectNode(current.name, node));
    },
    [group, run],
  );

  const openGroup = useCallback((next: StatusBarProxyGroup) => {
    setNodes(null);
    setGroup(next);
  }, []);
  const closeGroup = useCallback(() => setGroup(null), []);
  const dismissNotice = useCallback(() => setNotice(null), []);

  const testLatency = useCallback(() => {
    if (!group) return;
    setTesting(true);
    setNotice(null);
    void api
      .statusBarProxyLatency(group.name)
      .then((result) => {
        // 全组超时（空数组）说明没有控制口，不能画成「都没测过」。
        if (result.length === 0) {
          setNotice("没有控制口，测不了延迟。");
          return;
        }
        setNodes(result);
        // 组自己的延迟来自成员的最优值：测完顺手把概览里的数字也更新掉。
        setPage((prev) =>
          prev
            ? {
                ...prev,
                groups: prev.groups.map((item) =>
                  item.name === group.name ? { ...item, nodes: result } : item,
                ),
              }
            : prev,
        );
      })
      .catch((e) => setNotice(errorMessage(e)))
      .finally(() => setTesting(false));
  }, [group]);

  const setTakeover = useCallback(
    (next: boolean) => void run(() => api.statusBarProxySetTakeover(next)),
    [run],
  );
  const setMode = useCallback(
    (mode: ProxyMode) => void run(() => api.statusBarProxySetMode(mode)),
    [run],
  );
  const openApp = useCallback(() => void run(() => api.statusBarProxyOpenApp()), [run]);

  const refreshAutoSwitch = useCallback(() => {
    void api
      .statusBarAutoSwitchState()
      .then(setAutoSwitch)
      .catch(() => {});
  }, []);

  const setAutoSwitchConfig = useCallback(
    (config: AutoSwitchConfig) => {
      setAutoSwitchBusy(true);
      void api
        .statusBarAutoSwitchSetConfig(config)
        .then((outcome) => {
          const text = outcomeNotice(outcome);
          if (text) setNotice(text);
          refreshAutoSwitch();
        })
        .catch((e) => setNotice(errorMessage(e)))
        .finally(() => setAutoSwitchBusy(false));
    },
    [refreshAutoSwitch],
  );

  const setRouterSecret = useCallback(
    (secret: string) => {
      setAutoSwitchBusy(true);
      void api
        .statusBarAutoSwitchSetRouterSecret(secret)
        .then((outcome) => {
          const text = outcomeNotice(outcome);
          if (text) setNotice(text);
          refreshAutoSwitch();
        })
        .catch((e) => setNotice(errorMessage(e)))
        .finally(() => setAutoSwitchBusy(false));
    },
    [refreshAutoSwitch],
  );

  const checkAutoSwitchNow = useCallback(() => {
    setAutoSwitchBusy(true);
    void api
      .statusBarAutoSwitchCheckNow()
      .then(setAutoSwitch)
      .catch((e) => setNotice(errorMessage(e)))
      .finally(() => setAutoSwitchBusy(false));
  }, []);

  // 返回对象保持稳定身份：Esc 处理器把它放进依赖表时不该每次渲染都重挂监听。
  return useMemo(
    () => ({
      page,
      speed,
      loading,
      busy,
      notice,
      group,
      nodes,
      testing,
      autoSwitch,
      autoSwitchBusy,
      reload,
      dismissNotice,
      openGroup,
      closeGroup,
      setTakeover,
      setMode,
      selectNode,
      testLatency,
      openApp,
      setAutoSwitchConfig,
      setRouterSecret,
      checkAutoSwitchNow,
    }),
    [
      page,
      speed,
      loading,
      busy,
      notice,
      group,
      nodes,
      testing,
      autoSwitch,
      autoSwitchBusy,
      reload,
      dismissNotice,
      openGroup,
      closeGroup,
      setTakeover,
      setMode,
      selectNode,
      testLatency,
      openApp,
      setAutoSwitchConfig,
      setRouterSecret,
      checkAutoSwitchNow,
    ],
  );
}
