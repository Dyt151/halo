/**
 * 快捷回复选择器（评审 A1/C2、任务 6–8）。
 *
 * 独立快捷键唤出的紧凑浮动面板：搜索 + 分组筛选 + 方向键选择 + Enter 回贴 + Esc 关闭。
 * 只负责「使用」，不出现新建、排序、导入导出等管理控件——设置里的
 * 「快捷回复」分区已下线，数据层（useReplyStore + 后端命令）仍保留。
 *
 * 与回复库共享 useReplyStore 数据层；回贴后调用 close_reply_picker 收起，
 * 目标 App 立刻拿回焦点（失焦收起亦由 Rust 侧兜底）。
 */
import { useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { errorMessage } from "../lib/errors";
import { api } from "../lib/ipc";
import { useReplyStore } from "../store/useReplyStore";

export default function ReplyPicker() {
  const groups = useReplyStore((s) => s.groups);
  const replies = useReplyStore((s) => s.replies);
  const activeGroupId = useReplyStore((s) => s.activeGroupId);
  const loadGroups = useReplyStore((s) => s.loadGroups);
  const loadReplies = useReplyStore((s) => s.loadReplies);
  const setActiveGroup = useReplyStore((s) => s.setActiveGroup);
  const setKeyword = useReplyStore((s) => s.setKeyword);
  const resetFilters = useReplyStore((s) => s.resetFilters);
  const pasteReply = useReplyStore((s) => s.pasteReply);

  const [input, setInput] = useState("");
  const [index, setIndex] = useState(0);
  /** 回贴失败（多半是没给辅助功能权限）：选择器没有 toast 宿主，只能就地提示。 */
  const [fail, setFail] = useState<string | null>(null);
  const listRef = useRef<HTMLDivElement | null>(null);

  // 每次唤出重新拉取分组与回复，保证管理页刚编辑的内容即时可见。
  // 过滤条件必须一起复位：管理页停在某个分组时，选择器不能跟着只看那一组。
  const reload = () => {
    setInput("");
    setIndex(0);
    setFail(null);
    void loadGroups();
    resetFilters();
  };

  useEffect(() => {
    reload();
    const unshown = listen("clipbase://picker-shown", reload);
    return () => void unshown.then((fn) => fn());
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // 输入防抖：跟随关键词重新筛选（复用后端 LIKE 过滤）。
  // 与 store 里的关键词相同时跳过：唤出面板时 `resetFilters` 已经查过一次，
  // 再跟着空关键词发一次就是白打一条 IPC（切回空搜索时仍会正常重查）。
  useEffect(() => {
    const t = setTimeout(() => {
      const next = input.trim();
      if (useReplyStore.getState().keyword === next) return;
      setKeyword(next);
      setIndex(0);
      void loadReplies();
    }, 180);
    return () => clearTimeout(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [input]);

  const close = () => void api.closeReplyPicker().catch(() => {});

  const paste = (id: number) => {
    setFail(null);
    void pasteReply(id).then(
      (outcome) => {
        if (outcome.status === "pasted") {
          close();
          return;
        }
        // 「只复制、没贴上」不是失败，但也不能假装成功：留着窗口告诉用户
        // 内容已在剪贴板、可以直接 ⌘V，或先去授权。
        setFail(outcome.reason || "已复制到剪贴板，但没能自动粘贴");
      },
      (e: unknown) => {
        // 保留窗口让用户重试或改选；错误文案直接来自后端（已带可读中文）。
        const raw = errorMessage(e, "回贴失败");
        setFail(
          raw.includes("辅助功能")
            ? "回贴失败：未授予「辅助功能」权限，已可在设置 → 权限 中查看"
            : `回贴失败：${raw}`,
        );
      },
    );
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    // 输入法用 Enter 上屏：那是「确认候选词」，不是「回贴这一条」。
    // 不拦住的话，中文搜索时选词回车会把内容直接灌进前台 App。
    if (e.nativeEvent.isComposing || e.keyCode === 229) return;
    const n = replies.length;
    if (e.key === "Escape") {
      e.preventDefault();
      close();
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      if (n) setIndex((i) => (i + 1) % n);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      if (n) setIndex((i) => (i - 1 + n) % n);
    } else if (e.key === "Enter") {
      e.preventDefault();
      const target = replies[Math.min(index, n - 1)];
      if (target) paste(target.id);
    }
  };

  // 选中项滚动可见。
  useEffect(() => {
    const el = listRef.current?.querySelector<HTMLElement>(`[data-idx="${index}"]`);
    el?.scrollIntoView({ block: "nearest" });
  }, [index]);

  const chips = useMemo(
    () => [{ id: null as number | null, name: "全部" }, ...groups.map((g) => ({ id: g.id as number | null, name: g.name }))],
    [groups],
  );

  return (
    <div
      className="flex h-screen w-screen flex-col overflow-hidden text-white"
      onKeyDown={onKeyDown}
    >
      <div className="px-[12px] pt-[12px]">
        <input
          autoFocus
          value={input}
          onChange={(e) => setInput(e.target.value)}
          placeholder="搜索快捷回复…"
          className="input w-full"
        />
      </div>

      {chips.length > 1 && (
        <div className="flex flex-wrap gap-[6px] px-[12px] pt-[10px]">
          {chips.map((c) => (
            <button
              key={c.id ?? "all"}
              type="button"
              onClick={() => {
                setIndex(0);
                setActiveGroup(c.id);
              }}
              className={
                "rounded-full px-[10px] py-[3px] text-[12px] transition " +
                (activeGroupId === c.id
                  ? "bg-accent-500/90 text-accent-on"
                  : "bg-white/[0.06] text-white/60 hover:bg-white/[0.12]")
              }
            >
              {c.name}
            </button>
          ))}
        </div>
      )}

      <div ref={listRef} className="mt-[8px] min-h-0 flex-1 overflow-y-auto px-[8px] pb-[8px]">
        {replies.length === 0 ? (
          <p className="py-[24px] text-center text-[12px]/[16px] text-white/45">
            没有匹配的回复。
          </p>
        ) : (
          <ul className="space-y-[2px]">
            {replies.map((r, i) => (
              <li key={r.id}>
                <button
                  type="button"
                  data-idx={i}
                  onMouseEnter={() => setIndex(i)}
                  onClick={() => paste(r.id)}
                  className={
                    "w-full rounded-[8px] px-[10px] py-[7px] text-left transition " +
                    (i === index ? "bg-white/[0.10]" : "hover:bg-white/[0.06]")
                  }
                >
                  <div className="truncate text-[13px]/[18px] font-medium text-white">{r.title}</div>
                  <div className="truncate text-[12px]/[16px] text-white/50">
                    {r.content.replace(/\s+/g, " ").trim() || "（仅附件）"}
                  </div>
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>

      <div className="border-t border-white/[0.08] px-[12px] py-[7px] text-[11px]/[15px] text-white/40">
        {fail ? (
          <span className="text-danger-soft">{fail}</span>
        ) : (
          "↑↓ 选择 · Enter 回贴 · Esc 关闭"
        )}
      </div>
    </div>
  );
}
