// Zustand 全局状态：快捷回复（分组 + 回复模板），独立于 clip store。

import { create } from "zustand";
import { api } from "../lib/ipc";
import { errorMessage } from "../lib/errors";
import type { PasteOutcome, Reply, ReplyGroup } from "../lib/types";

/** 回复列表查询序号（跨调用累计，用于丢弃过期响应）。 */
let loadSeq = 0;

interface ReplyStore {
  groups: ReplyGroup[];
  /** 当前筛选分组，null = 全部。 */
  activeGroupId: number | null;
  replies: Reply[];
  keyword: string;
  loading: boolean;
  error: string | null;

  loadGroups: () => Promise<void>;
  loadReplies: () => Promise<void>;
  /** 清掉一次性错误提示（管理页把 error 当 Notice 展示）。 */
  clearError: () => void;
  setActiveGroup: (id: number | null) => void;
  setKeyword: (keyword: string) => void;
  /** 回到「全部 + 无关键词」并重拉一次（选择器每次唤出都要干净状态）。 */
  resetFilters: () => void;
  addGroup: (name: string) => Promise<ReplyGroup>;
  renameGroup: (id: number, name: string) => Promise<void>;
  deleteGroup: (id: number) => Promise<void>;
  addReply: (groupId: number, title: string, content: string, files: string[]) => Promise<void>;
  updateReply: (id: number, title: string, content: string, files: string[]) => Promise<void>;
  deleteReply: (id: number) => Promise<void>;
  /** 同组内移动：正数上移、负数下移。 */
  move: (id: number, direction: number) => Promise<void>;
  /** 回贴一条：返回结果，调用方需要区分「已粘贴」与「只复制」。 */
  pasteReply: (id: number) => Promise<PasteOutcome>;
}

export const useReplyStore = create<ReplyStore>((set, get) => ({
  groups: [],
  activeGroupId: null,
  replies: [],
  keyword: "",
  loading: false,
  error: null,

  loadGroups: async () => {
    try {
      const groups = await api.listReplyGroups();
      set({ groups, error: null });
    } catch (e) {
      set({ error: errorMessage(e) });
    }
  },

  loadReplies: async () => {
    const { activeGroupId, keyword } = get();
    // 查询序号：切换分组/连续输入时，旧请求可能后到，不能让它盖掉新结果
    // （否则「点 A 组→点 B 组」会闪回 A 组的内容）。
    const seq = ++loadSeq;
    set({ loading: true });
    try {
      const replies = await api.listReplies(
        activeGroupId,
        keyword.trim() ? keyword.trim() : null,
      );
      if (seq !== loadSeq) return;
      set({ replies, error: null });
    } catch (e) {
      if (seq === loadSeq) set({ error: errorMessage(e), replies: [] });
    } finally {
      if (seq === loadSeq) set({ loading: false });
    }
  },

  setActiveGroup: (id) => {
    set({ activeGroupId: id });
    void get().loadReplies();
  },

  clearError: () => set({ error: null }),

  setKeyword: (keyword) => set({ keyword }),

  resetFilters: () => {
    set({ activeGroupId: null, keyword: "" });
    void get().loadReplies();
  },

  addGroup: async (name) => {
    try {
      const group = await api.addReplyGroup(name);
      set((s) => ({ groups: [...s.groups, group], error: null }));
      return group;
    } catch (e) {
      set({ error: errorMessage(e) });
      throw e;
    }
  },

  renameGroup: async (id, name) => {
    try {
      await api.renameReplyGroup(id, name);
      await get().loadGroups();
    } catch (e) {
      set({ error: errorMessage(e) });
      throw e;
    }
  },

  deleteGroup: async (id) => {
    try {
      await api.deleteReplyGroup(id);
      // 删除的正好是当前分组时，回退到「全部」。
      set((s) => ({ activeGroupId: s.activeGroupId === id ? null : s.activeGroupId }));
      await Promise.all([get().loadGroups(), get().loadReplies()]);
    } catch (e) {
      set({ error: errorMessage(e) });
      throw e;
    }
  },

  addReply: async (groupId, title, content, files) => {
    try {
      await api.addReply(groupId, title, content, files);
      await get().loadReplies();
    } catch (e) {
      set({ error: errorMessage(e) });
      throw e;
    }
  },

  updateReply: async (id, title, content, files) => {
    try {
      await api.updateReply(id, title, content, files);
      await get().loadReplies();
    } catch (e) {
      set({ error: errorMessage(e) });
      throw e;
    }
  },

  deleteReply: async (id) => {
    try {
      await api.deleteReply(id);
      await get().loadReplies();
    } catch (e) {
      set({ error: errorMessage(e) });
      throw e;
    }
  },

  move: async (id, direction) => {
    try {
      await api.moveReply(id, direction);
      await get().loadReplies();
    } catch (e) {
      set({ error: errorMessage(e) });
      throw e;
    }
  },

  pasteReply: async (id) => {
    try {
      const outcome = await api.replyPaste(id);
      await get().loadReplies();
      return outcome;
    } catch (e) {
      set({ error: errorMessage(e) });
      // 往上抛：选择器是浮动窗口没有 toast 宿主，需要就地显示失败原因，
      // 否则用户按 Enter 后会以为“点了没反应”。
      throw e;
    }
  },
}));
