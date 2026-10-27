// Zustand 全局状态：历史列表 / 搜索 / 配置 / 同步 / 模块。

import { create } from "zustand";
import { api } from "../lib/ipc";
import { errorMessage } from "../lib/errors";
import { toast, toastError } from "../lib/toast";
import type {
  ClipItem,
  Config,
  ContentType,
  HistoryQuery,
  HistoryStats,
  ModuleRecord,
  PasteOutcome,
  SyncConfig,
  SyncStatus,
} from "../lib/types";

interface ClipStore {
  items: ClipItem[];
  loading: boolean;
  /** 模块列表自己的加载态（评审 G5：此前误用剪贴板列表的 loading）。 */
  modulesLoading: boolean;
  keyword: string;
  contentType: ContentType | null;
  favoriteOnly: boolean;
  activeTag: string | null;
  /** 全部出现过的标签（后端 DISTINCT，不随筛选收窄，评审 G2）。 */
  tags: string[];
  stats: HistoryStats;
  config: Config | null;
  syncStatus: SyncStatus | null;
  /** 同步进行中（评审 G9：防止连点并发 PUT）。 */
  syncBusy: boolean;
  /** 钥匙串里是否已存 WebDAV 密码（评审 S6）。 */
  syncHasPassword: boolean;
  /** 钥匙串里是否已存加密口令（清单 8）。 */
  syncHasPassphrase: boolean;
  modules: ModuleRecord[];
  error: string | null;

  refresh: () => Promise<void>;
  /** 新剪贴板条目局部插入（无筛选时）；有筛选条件时退回全量刷新。 */
  applyClipboardChanged: (item: ClipItem) => void;
  setKeyword: (keyword: string) => void;
  /**
   * 立即执行待发的搜索（Enter 回贴前用）。
   *
   * 搜索有 250ms 防抖：用户打完字马上按回车时，列表还是上一个关键词的结果。
   * 不回「贴错条目」的唯一办法是先把这次查询落地再取列表。
   */
  flushSearch: () => Promise<void>;
  setActiveTag: (tag: string | null) => void;
  /** 同时改「类型 + 收藏」：工具栏的一个分段同时含这两维，分两次设会多查一次。 */
  setFilter: (contentType: ContentType | null, favoriteOnly: boolean) => void;
  /** 一次清掉搜索 + 类型 + 收藏 + 标签，只发一次查询（否则四个 set 会连打四次）。 */
  clearFilters: () => void;
  setClipTag: (id: number, tag: string | null) => Promise<void>;
  /** 回贴；返回值让调用方决定是否收起面板。 */
  paste: (id: number) => Promise<PasteOutcome | null>;
  copy: (id: number) => Promise<void>;
  remove: (id: number) => Promise<void>;
  removeMany: (ids: number[]) => Promise<void>;
  clearAll: () => Promise<void>;
  togglePin: (id: number, pinned: boolean) => Promise<void>;
  toggleFavorite: (id: number, favorite: boolean) => Promise<void>;
  /** 撤销上一次删除（评审 M8）。 */
  undo: () => Promise<void>;
  redo: () => Promise<void>;
  /** 立即捕获一次剪贴板（空状态引导用）。 */
  captureNow: () => Promise<void>;
  loadConfig: () => Promise<void>;
  /** 增量保存配置：只提交改动字段，避免整份对象覆盖别人的改动（评审 F3）。 */
  patchConfig: (patch: Partial<Config>) => Promise<void>;
  loadSyncStatus: () => Promise<void>;
  loadSyncSecret: () => Promise<void>;
  syncNow: () => Promise<void>;
  testSync: (
    credentials: { url: string; username: string; password: string },
    provider?: SyncConfig["provider"],
  ) => Promise<void>;
  setSyncSettings: (input: {
    enabled: boolean;
    provider: SyncConfig["provider"];
    url: string;
    username: string;
    password?: string;
    encryption: boolean;
    passphrase?: string;
    remote_file: string;
    interval_secs: number;
  }) => Promise<void>;
  loadModules: () => Promise<void>;
  /** 写入状态栏错误。接受任意异常形态（内部统一折成可读文案，见 `lib/errors`）。 */
  setError: (error: unknown) => void;
}

const DEFAULT_STATS: HistoryStats = { total: 0, total_bytes: 0, by_type: {}, favorite: 0 };

/** 搜索防抖窗口（trailing）：连续键入只发一次 FTS 查询。 */
const SEARCH_DEBOUNCE_MS = 250;
let keywordTimer: ReturnType<typeof setTimeout> | null = null;
function clearKeywordTimer() {
  if (keywordTimer !== null) {
    clearTimeout(keywordTimer);
    keywordTimer = null;
  }
}

/**
 * 查询序号：只让「最后一次」refresh 的结果落地。
 *
 * 防抖只能减少请求数，不能消除竞态：先敲 `a` 再敲 `ab`，若 `a` 的响应后到，
 * 列表会停在旧关键词上（看起来像「搜索框里是 ab、结果却是 a」）。
 */
let refreshSeq = 0;

/**
 * 配置写入的串行链。
 *
 * `patch_config` 返回的是「应用完这次 patch 后的整份配置」，store 拿它整份覆盖。
 * 两笔并发 patch 若响应乱序（IPC 不保证并发调用的完成顺序），后到的旧快照会把
 * 后发那笔改回去 —— 界面上就是「刚拖好的旋钮自己弹回去」。这里让写请求排队，
 * 保证「应用顺序 = 发起顺序 = 落地顺序」。
 */
let patchChain: Promise<unknown> = Promise.resolve();

/**
 * 把「查询发出后新入库」的条目合并回结果（只用于无筛选的全量列表）。
 *
 * 只保以 `since` 之后创建的条目：它们不可能是「已被删掉但本地还留着」的幽灵，
 * 也不可能是“不匹配筛选”的条目（筛选条件此时为空）。
 */
function mergeLateArrivals(
  fetched: ClipItem[],
  current: ClipItem[],
  since: number,
  unfiltered: boolean,
): ClipItem[] {
  if (!unfiltered || current.length === 0) return fetched;
  const seen = new Set(fetched.map((it) => it.id));
  const late = current.filter((it) => !seen.has(it.id) && it.created_at >= since);
  if (late.length === 0) return fetched;
  return [...late, ...fetched].slice(0, 500);
}

/** 新条目入库后就地更新统计（字段语义与后端 `history_stats` 一致）。 */
function bumpStats(stats: HistoryStats, item: ClipItem): HistoryStats {
  const type = item.content_type;
  return {
    total: stats.total + 1,
    total_bytes: stats.total_bytes + (item.size_bytes ?? 0),
    by_type: { ...stats.by_type, [type]: (stats.by_type[type] ?? 0) + 1 },
    favorite: stats.favorite + (item.favorite ? 1 : 0),
  };
}

/**
 * 连续撤销 n 条。
 *
 * 后端的撤销栈是「一条一条推进」的，批量删除后只调一次 undo 只能回来一条 ——
 * 而 toast 上写的是「已删除 N 条」。这里循环调到恢复完，最后统一刷新一次。
 */
async function undoMany(count: number): Promise<void> {
  let restored = 0;
  for (let i = 0; i < count; i += 1) {
    try {
      const item = await api.undo();
      if (!item) break;
      restored += 1;
    } catch {
      // 栈空或后端报错都停下来：已经恢复的条目仍然要显示出来。
      break;
    }
  }
  await useClipStore.getState().refresh();
  if (restored === 0) {
    toast("没有可撤销的操作");
    return;
  }
  toast(`已恢复 ${restored} 条记录`, { tone: "success" });
}

export const useClipStore = create<ClipStore>((set, get) => ({
  items: [],
  loading: false,
  modulesLoading: false,
  keyword: "",
  contentType: null,
  favoriteOnly: false,
  /**
   * 分组筛选不再跨会话/跨窗口持久化（评审 G1）。
   *
   * 旧实现把 activeTag 写进 localStorage：在面板里选了一个标签，管理窗口的
   * 历史页就带着这个「看不见的筛选」打开 —— 用户既看不到标签栏，也去不掉。
   */
  activeTag: null,
  tags: [],
  stats: DEFAULT_STATS,
  config: null,
  syncStatus: null,
  syncBusy: false,
  syncHasPassword: false,
  syncHasPassphrase: false,
  modules: [],
  error: null,

  refresh: async () => {
    const { keyword, contentType, favoriteOnly, activeTag } = get();
    const seq = ++refreshSeq;
    // 本次查询发出的时间：用来识别「查询在路上时刚入库的新条目」。
    // 不合并的话会出现「复制一下、卡片闪一下就没了」——请求早于入库发出，
    // 回来的快照里自然没有这条，而它会用整份快照盖掉刚插入的卡片。
    const since = Date.now();
    set({ loading: true });
    try {
      const query: HistoryQuery = {
        keyword: keyword.trim() ? keyword.trim() : null,
        content_type: contentType,
        pinned_only: false,
        favorite_only: favoriteOnly,
        tag: activeTag,
        limit: 500,
        offset: 0,
      };
      const [fetched, stats, tags] = await Promise.all([
        api.listClips(query),
        api.historyStats(),
        // 标签全集来自后端而不是当前列表：否则选中标签后其它标签会消失（G2）。
        api.listTags().catch(() => [] as string[]),
      ]);
      // 已有更新的查询在路上：这批结果已经不代表当前筛选，直接丢弃。
      if (seq !== refreshSeq) return;
      const items = mergeLateArrivals(fetched, get().items, since, !query.keyword && !contentType && !favoriteOnly && !activeTag);
      set({ items, stats, tags, error: null });
    } catch (e) {
      if (seq !== refreshSeq) return;
      set({ error: errorMessage(e) });
    } finally {
      if (seq === refreshSeq) set({ loading: false });
    }
  },

  /**
   * 新条目局部插入：无筛选条件时直接置顶/前插，避免每次复制都全量重拉
   * （重拉会重新渲染 200+ 卡片，是面板更新慢的主因）；有筛选条件时退回全量刷新。
   */
  applyClipboardChanged: (item) => {
    const { items, keyword, contentType, favoriteOnly, activeTag, stats } = get();
    if (keyword || contentType || favoriteOnly || activeTag) {
      void get().refresh();
      return;
    }
    const existing = items.some((i) => i.id === item.id);
    const next = existing
      ? [item, ...items.filter((i) => i.id !== item.id)]
      : [item, ...items].slice(0, 500);
    set({
      items: next,
      // 统计颗也要跟着走：只改 total 会让「文本 12 / 图片 3」与列表对不上，
      // 直到下次切换筛选才自愈。
      stats: existing ? stats : bumpStats(stats, item),
    });
  },

  /**
   * 筛选条件一律「改状态 + 立刻重新查询」。
   * 放在 store 内部而不是交给调用方补一次 refresh，避免某个页面忘了刷新、
   * 出现「点了筛选但列表没变」的假 bug。
   */
  setKeyword: (keyword) => {
    set({ keyword });
    // 250ms trailing 防抖：中文输入法逐字母上屏时不再每个字符打一次 FTS。
    // 输入法组合期间（composition）的中间值由 SearchField 拦截，不会到达这里。
    clearKeywordTimer();
    if (!keyword.trim()) {
      void get().refresh();
      return;
    }
    keywordTimer = setTimeout(() => {
      keywordTimer = null;
      void get().refresh();
    }, SEARCH_DEBOUNCE_MS);
  },
  setActiveTag: (activeTag) => {
    set({ activeTag });
    void get().refresh();
  },
  flushSearch: async () => {
    // 没有待发查询时直接返回：避免多余的一次 FTS（Enter 也会走到这里）。
    if (keywordTimer === null) return;
    clearKeywordTimer();
    await get().refresh();
  },
  clearFilters: () => {
    clearKeywordTimer();
    set({ keyword: "", contentType: null, favoriteOnly: false, activeTag: null });
    void get().refresh();
  },
  setFilter: (contentType, favoriteOnly) => {
    set({ contentType, favoriteOnly });
    void get().refresh();
  },

  setClipTag: async (id, tag) => {
    try {
      await api.setClipTag(id, tag);
      await get().refresh();
    } catch (e) {
      set({ error: errorMessage(e) });
      toastError(e);
    }
  },

  paste: async (id) => {
    try {
      const outcome = await api.pasteClip(id);
      if (outcome.status === "pasted") {
        toast("已回贴到前台 App", { tone: "success" });
      } else {
        // 评审 S5：CopiedOnly 曾经被完全丢弃 —— 用户点了回贴却什么都没发生。
        toast(outcome.reason || "已复制到剪贴板（未获得辅助功能权限，无法自动粘贴）", {
          tone: "info",
          duration: 4200,
        });
      }
      await get().refresh();
      return outcome;
    } catch (e) {
      set({ error: errorMessage(e) });
      toastError(e, "回贴失败");
      return null;
    }
  },

  copy: async (id) => {
    try {
      await api.copyClip(id);
      toast("已复制", { tone: "success" });
    } catch (e) {
      set({ error: errorMessage(e) });
      toastError(e, "复制失败");
    }
  },

  remove: async (id) => {
    try {
      await api.deleteClip(id);
      await get().refresh();
      // 删除可撤销（⌘Z / toast 上的「撤销」）。
      toast("已删除 1 条记录", {
        actionLabel: "撤销",
        onAction: () => void get().undo(),
      });
    } catch (e) {
      set({ error: errorMessage(e) });
      toastError(e, "删除失败");
    }
  },

  removeMany: async (ids) => {
    if (ids.length === 0) return;
    let removed = 0;
    try {
      // 后端无批量删除命令；串行删除可以让 undo 栈按用户直觉逐条回收。
      for (const id of ids) {
        await api.deleteClip(id);
        removed += 1;
      }
      await get().refresh();
      toast(`已删除 ${removed} 条记录`, {
        actionLabel: "撤销",
        // 撤销栈是逐条推进的：必须循环调用，否则「撤销」只回来一条。
        onAction: () => void undoMany(removed),
      });
    } catch (e) {
      // 中途失败：前面几条已经删了，不能假装整批都没发生。
      await get().refresh();
      set({ error: errorMessage(e) });
      toastError(e, removed > 0 ? `已删除 ${removed} 条，其余删除失败` : "批量删除失败");
    }
  },

  clearAll: async () => {
    try {
      await api.clearClips();
      await get().refresh();
      toast("已清空全部历史", { tone: "info" });
    } catch (e) {
      set({ error: errorMessage(e) });
      toastError(e, "清空失败");
    }
  },

  togglePin: async (id, pinned) => {
    try {
      await api.pinClip(id, pinned);
      await get().refresh();
      toast(pinned ? "已置顶" : "已取消置顶");
    } catch (e) {
      set({ error: errorMessage(e) });
      toastError(e);
    }
  },

  toggleFavorite: async (id, favorite) => {
    try {
      await api.favoriteClip(id, favorite);
      await get().refresh();
      toast(favorite ? "已收藏" : "已取消收藏");
    } catch (e) {
      set({ error: errorMessage(e) });
      toastError(e);
    }
  },

  undo: async () => {
    try {
      const restored = await api.undo();
      if (!restored) {
        toast("没有可撤销的操作");
        return;
      }
      await get().refresh();
      toast(`已恢复「${restored.preview?.slice(0, 20) || "1 条记录"}」`, { tone: "success" });
    } catch (e) {
      set({ error: errorMessage(e) });
      toastError(e, "撤销失败");
    }
  },

  redo: async () => {
    try {
      const removed = await api.redo();
      if (removed === null) {
        toast("没有可重做的操作");
        return;
      }
      await get().refresh();
      toast("已重做删除");
    } catch (e) {
      set({ error: errorMessage(e) });
      toastError(e, "重做失败");
    }
  },

  captureNow: async () => {
    try {
      const item = await api.captureNow();
      await get().refresh();
      toast(item ? "已捕获当前剪贴板" : "剪贴板没有新内容", {
        tone: item ? "success" : "info",
      });
    } catch (e) {
      set({ error: errorMessage(e) });
      toastError(e, "捕获失败");
    }
  },

  loadConfig: async () => {
    try {
      const config = await api.getConfig();
      set({ config });
    } catch (e) {
      set({ error: errorMessage(e) });
    }
  },

  patchConfig: async (patch) => {
    const run = async () => {
      try {
        const saved = await api.patchConfig(patch as Record<string, unknown>);
        set({ config: saved, error: null });
      } catch (e) {
        // 向上抛：调用方（自动保存）需要区分「已保存 / 保存失败」。
        set({ error: errorMessage(e) });
        toastError(e, "保存设置失败");
        throw e;
      }
    };
    // 前一笔结束（无论成败）再发下一笔。
    const task = patchChain.then(run, run);
    patchChain = task.catch(() => {});
    await task;
  },

  loadSyncStatus: async () => {
    try {
      const syncStatus = await api.getSyncStatus();
      set({ syncStatus });
    } catch (e) {
      set({ error: errorMessage(e) });
    }
  },

  loadSyncSecret: async () => {
    try {
      const status = await api.syncSecretStatus();
      set({ syncHasPassword: status.has_password, syncHasPassphrase: status.has_passphrase });
    } catch {
      // 钥匙串不可用时静默降级（不影响其它设置）。
    }
  },

  syncNow: async () => {
    // 评审 G9：连点「立即同步」不再并发两轮 PUT。
    if (get().syncBusy) return;
    set({ syncBusy: true });
    try {
      const syncStatus = await api.syncNow();
      set({ syncStatus });
      toast("同步完成", { tone: "success" });
    } catch (e) {
      set({ error: errorMessage(e) });
      toastError(e, "同步失败");
    } finally {
      set({ syncBusy: false });
    }
  },

  testSync: async (credentials, provider) => {
    try {
      const message = await api.testSync(credentials, provider);
      toast(message, { tone: "success" });
    } catch (e) {
      set({ error: errorMessage(e) });
      toastError(e, "连接失败");
      throw e;
    }
  },

  setSyncSettings: async (input) => {
    try {
      const sync = await api.setSyncSettings(input);
      const config = get().config;
      if (config) set({ config: { ...config, sync } });
      await get().loadSyncSecret();
      toast("同步设置已保存", { tone: "success" });
    } catch (e) {
      set({ error: errorMessage(e) });
      toastError(e, "保存同步设置失败");
      throw e;
    }
  },

  loadModules: async () => {
    set({ modulesLoading: true });
    try {
      const modules = await api.listModules();
      set({ modules });
    } catch (e) {
      set({ error: errorMessage(e) });
    } finally {
      set({ modulesLoading: false });
    }
  },

  // 兜底收敛：调用方万一传进来的是后端错误对象（`{code,message,details}`），
  // 直接存进 store 会让 `<span>⚠ {error}</span>` 渲染成「[object Object]」。
  setError: (error) => set({ error: error ? errorMessage(error) : null }),
}));
