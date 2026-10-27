/**
 * 设置中心「快捷回复」分区：分组 + 条目的增删改、排序与 Excel 导入导出。
 *
 * 后端的 `add_reply` / `update_reply` / `delete_reply` / `move_reply` /
 * `import_replies_xlsx` 一直齐全，选择器也在，但没有任何管理入口——选择器
 * 里甚至写着「可在设置 → 快捷回复 中管理」，点过去是空的。这里把这一页补上：
 *
 * - 左侧分组（全部 / 各分组），右侧当前分组的条目；
 * - 新建 / 编辑走同一个对话框（标题 + 正文 + 附件），Esc 取消、⌘↵ 保存；
 * - 分组只有一个「重命名 / 删除」，删除前明确告知会连带删掉组内条目；
 * - 导入按后端约定汇总：成功条数、新建分组数、逐行错误（超过 10 条只列前 10）。
 *
 * 数据层复用 `useReplyStore`，与选择器共享同一份缓存，管理完立刻能在选择器里看到。
 */
import { useEffect, useMemo, useRef, useState } from "react";
import {
  Btn,
  CardGroup,
  Divider,
  EmptyState,
  Input,
  Notice,
  Pill,
  SearchField,
  Select,
  Spinner,
} from "../components/ui";
import { alertDialog, confirm, promptDialog } from "../lib/dialog";
import { fileBasename } from "../lib/format";
import { api } from "../lib/ipc";
import { toast, toastError } from "../lib/toast";
import type { Reply } from "../lib/types";
import { useReplyStore } from "../store/useReplyStore";

/** 编辑中的条目；`reply` 为 null 表示新建。 */
interface Editing {
  reply: Reply | null;
  groupId: number;
}

export default function ReplySettings() {
  const groups = useReplyStore((s) => s.groups);
  const replies = useReplyStore((s) => s.replies);
  const loading = useReplyStore((s) => s.loading);
  const error = useReplyStore((s) => s.error);
  const activeGroupId = useReplyStore((s) => s.activeGroupId);
  const loadGroups = useReplyStore((s) => s.loadGroups);
  const loadReplies = useReplyStore((s) => s.loadReplies);
  const setActiveGroup = useReplyStore((s) => s.setActiveGroup);
  const setKeyword = useReplyStore((s) => s.setKeyword);
  const addGroup = useReplyStore((s) => s.addGroup);
  const renameGroup = useReplyStore((s) => s.renameGroup);
  const deleteGroup = useReplyStore((s) => s.deleteGroup);
  const deleteReply = useReplyStore((s) => s.deleteReply);
  const move = useReplyStore((s) => s.move);
  const clearError = useReplyStore((s) => s.clearError);

  const [search, setSearch] = useState("");
  const [editing, setEditing] = useState<Editing | null>(null);
  /** 导入 / 导出进行中（两件事都走系统面板，禁止并发）。 */
  const [transferring, setTransferring] = useState(false);
  /** 正在删除 / 移动的条目 id，行内按钮临时禁用，防连点。 */
  const [busyId, setBusyId] = useState<number | null>(null);

  useEffect(() => {
    void loadGroups();
    void loadReplies();
  }, [loadGroups, loadReplies]);

  // 搜索防抖：挂载首帧已经拉过一次，不必再打一次同样的查询。
  const firstRun = useRef(true);
  useEffect(() => {
    if (firstRun.current) {
      firstRun.current = false;
      return;
    }
    const timer = window.setTimeout(() => {
      setKeyword(search);
      void loadReplies();
    }, 200);
    return () => window.clearTimeout(timer);
  }, [search, setKeyword, loadReplies]);

  const activeGroup = useMemo(
    () => groups.find((g) => g.id === activeGroupId) ?? null,
    [groups, activeGroupId],
  );

  /* ---------------- 分组 ---------------- */

  const createGroup = async () => {
    const name = await promptDialog("新分组名称", "", {
      title: "新建分组",
      placeholder: "例如：客服话术",
      okLabel: "创建",
    });
    const trimmed = name?.trim();
    if (!trimmed) return;
    try {
      const group = await addGroup(trimmed);
      toast(`已创建分组「${group.name}」`, { tone: "success" });
      setActiveGroup(group.id);
    } catch (e) {
      toastError(e, "新建分组失败");
    }
  };

  const renameActiveGroup = async () => {
    if (!activeGroup) return;
    const name = await promptDialog("分组名称", activeGroup.name, {
      title: "重命名分组",
      okLabel: "保存",
    });
    const trimmed = name?.trim();
    if (!trimmed || trimmed === activeGroup.name) return;
    try {
      await renameGroup(activeGroup.id, trimmed);
      toast("分组已重命名", { tone: "success" });
    } catch (e) {
      toastError(e, "重命名失败");
    }
  };

  const removeActiveGroup = async () => {
    if (!activeGroup) return;
    const ok = await confirm(
      `确定删除分组「${activeGroup.name}」？组内的快捷回复会一并删除，此操作不可撤销。`,
      { danger: true, title: "删除分组", okLabel: "删除" },
    );
    if (!ok) return;
    try {
      await deleteGroup(activeGroup.id);
      toast(`已删除分组「${activeGroup.name}」`, { tone: "info" });
    } catch (e) {
      toastError(e, "删除分组失败");
    }
  };

  /**
   * 新建条目需要一个落地分组：优先当前分组 → 第一个分组 → 自动建「默认」。
   * 后端 `add_reply` 必须带真实 group_id，没分组时不该把用户卡在白屏上。
   */
  const resolveGroupForNew = async (): Promise<number | null> => {
    if (activeGroupId !== null) return activeGroupId;
    if (groups.length > 0) return groups[0].id;
    try {
      const group = await addGroup("默认");
      setActiveGroup(group.id);
      toast("已自动创建「默认」分组", { tone: "info" });
      return group.id;
    } catch (e) {
      toastError(e, "新建分组失败");
      return null;
    }
  };

  const openNew = async () => {
    const groupId = await resolveGroupForNew();
    if (groupId === null) return;
    setEditing({ reply: null, groupId });
  };

  /* ---------------- 条目 ---------------- */

  const removeReply = async (reply: Reply) => {
    const ok = await confirm(`确定删除「${reply.title || "这条快捷回复"}」？`, {
      danger: true,
      title: "删除快捷回复",
      okLabel: "删除",
    });
    if (!ok) return;
    setBusyId(reply.id);
    try {
      await deleteReply(reply.id);
      toast("已删除", { tone: "info" });
    } catch (e) {
      toastError(e, "删除失败");
    } finally {
      setBusyId(null);
    }
  };

  const moveReply = async (reply: Reply, direction: number) => {
    setBusyId(reply.id);
    try {
      await move(reply.id, direction);
    } catch (e) {
      toastError(e, "移动失败");
    } finally {
      setBusyId(null);
    }
  };

  /* ---------------- 导入 / 导出 ---------------- */

  const exportXlsx = async () => {
    setTransferring(true);
    try {
      const path = await api.exportRepliesXlsx(activeGroupId);
      toast(`已导出 ${fileBasename(path)}`, { tone: "success", duration: 4000 });
      // 顺手在 Finder 里定位文件：用户下一步多半就是打开它。
      void api.revealPath(path).catch(() => {});
    } catch (e) {
      toastError(e, "导出失败");
    } finally {
      setTransferring(false);
    }
  };

  const importXlsx = async () => {
    const picked = await api.pickFiles(false).catch((e) => {
      toastError(e, "无法打开选择面板");
      return [] as string[];
    });
    const path = picked[0];
    if (!path) return;
    setTransferring(true);
    try {
      const summary = await api.importRepliesXlsx(path);
      await Promise.all([loadGroups(), loadReplies()]);
      if (summary.imported === 0) {
        toast("没有可导入的行（表头需含「标题」「内容」）", { tone: "info", duration: 5000 });
      } else {
        toast(
          `已导入 ${summary.imported} 条${summary.groups_created > 0 ? `，新建 ${summary.groups_created} 个分组` : ""}`,
          { tone: "success" },
        );
      }
      if (summary.errors.length > 0) {
        const shown = summary.errors.slice(0, 10);
        const more =
          summary.errors.length > shown.length
            ? `\n…共 ${summary.errors.length} 条问题，其余请检查原表格。`
            : "";
        await alertDialog(`${shown.join("\n")}${more}`, { title: "部分内容没有导入" });
      }
    } catch (e) {
      toastError(e, "导入失败");
    } finally {
      setTransferring(false);
    }
  };

  /* ---------------- 渲染 ---------------- */

  const chips = useMemo(
    () => [
      { id: null as number | null, name: "全部" },
      ...groups.map((g) => ({ id: g.id as number | null, name: g.name })),
    ],
    [groups],
  );

  const scoped = activeGroup?.name ?? "全部";
  // 上下移只能是「组内」语义：看「全部」或带搜索词时，邻居不一定是屏幕上的下一条，
  // 点下去会像「没反应」，宁可置灰。
  const canReorder = activeGroupId !== null && search.trim().length === 0;

  return (
    <div className="space-y-[16px]">
      <CardGroup
        title="快捷回复"
        subtitle="常用话术按分组管理；在任意 App 里按快捷键唤出选择器，↑↓ 选择、Enter 直接回贴。"
      >
        <div className="mb-[12px] flex flex-wrap items-center gap-[8px]">
          <Btn size="sm" variant="primary" onClick={() => void openNew()}>
            新建回复
          </Btn>
          <Btn size="sm" variant="ghost" onClick={() => void importXlsx()} disabled={transferring}>
            导入 Excel…
          </Btn>
          <Btn size="sm" variant="ghost" onClick={() => void exportXlsx()} disabled={transferring}>
            {activeGroup ? `导出「${activeGroup.name}」` : "导出全部"}
          </Btn>
          {transferring && <Spinner size={13} />}
          <span className="ml-auto text-[12px]/[16px] text-white/40">
            {replies.length} 条 · {scoped}
          </span>
        </div>

        <div className="flex gap-[16px]">
          {/* 左：分组 */}
          <div className="w-[148px] shrink-0">
            <div className="space-y-[2px]">
              {chips.map((c) => {
                const active = activeGroupId === c.id;
                return (
                  <button
                    key={c.id ?? "all"}
                    type="button"
                    onClick={() => setActiveGroup(c.id)}
                    aria-current={active ? "true" : undefined}
                    className={`flex h-[28px] w-full items-center rounded-[6px] px-[8px] text-left text-[12px]/[16px] transition-colors duration-120 ${
                      active
                        ? "bg-white/[0.10] font-medium text-white"
                        : "text-white/70 hover:bg-white/[0.06] hover:text-white/90"
                    }`}
                  >
                    <span className="truncate">{c.name}</span>
                  </button>
                );
              })}
            </div>
            <Divider className="my-[8px]" />
            <div className="flex flex-col gap-[2px]">
              <button
                type="button"
                onClick={() => void createGroup()}
                className="h-[26px] rounded-[6px] px-[8px] text-left text-[12px]/[16px] text-accent-500 hover:bg-white/[0.06]"
              >
                ＋ 新建分组
              </button>
              {activeGroup && (
                <>
                  <button
                    type="button"
                    onClick={() => void renameActiveGroup()}
                    className="h-[26px] rounded-[6px] px-[8px] text-left text-[12px]/[16px] text-white/55 hover:bg-white/[0.06] hover:text-white/85"
                  >
                    重命名分组
                  </button>
                  <button
                    type="button"
                    onClick={() => void removeActiveGroup()}
                    className="h-[26px] rounded-[6px] px-[8px] text-left text-[12px]/[16px] text-danger-soft hover:bg-danger/[0.12]"
                  >
                    删除分组
                  </button>
                </>
              )}
            </div>
          </div>

          {/* 右：条目 */}
          <div className="min-w-0 flex-1">
            <SearchField
              value={search}
              onChange={setSearch}
              placeholder="搜索标题或内容"
              width="w-full"
            />

            {error && (
              <div className="mt-[8px]">
                <Notice tone="danger" onClose={clearError}>
                  {error}
                </Notice>
              </div>
            )}

            <div className="mt-[6px]">
              {loading && replies.length === 0 ? (
                <div className="grid place-items-center py-[28px] text-white/40">
                  <Spinner size={16} />
                </div>
              ) : replies.length === 0 ? (
                <EmptyState
                  compact
                  title={search.trim() ? "没有匹配的回复" : "还没有快捷回复"}
                  hint={
                    search.trim()
                      ? "换个关键词，或切换到其它分组"
                      : "新建一条之后，在任何 App 里按快捷键就能挑出来回贴"
                  }
                />
              ) : (
                <ul className="divide-y divide-white/[0.06]">
                  {replies.map((r, i) => {
                    const busy = busyId === r.id;
                    return (
                      <li key={r.id} className="flex items-start gap-[10px] py-[10px]">
                        <div className="min-w-0 flex-1">
                          <div className="truncate text-[13px]/[18px] text-white/85">
                            {r.title || "（无标题）"}
                          </div>
                          <div className="mt-[1px] truncate text-[12px]/[16px] text-white/45">
                            {r.content.replace(/\s+/g, " ").trim() || "（仅附件）"}
                          </div>
                          {r.files.length > 0 && (
                            <div className="mt-[4px] flex flex-wrap items-center gap-[4px]">
                              {r.files.slice(0, 3).map((f) => (
                                <Pill key={f} selected={false} className="pointer-events-none">
                                  {fileBasename(f)}
                                </Pill>
                              ))}
                              {r.files.length > 3 && (
                                <span className="text-[11px]/[14px] text-white/35">
                                  等 {r.files.length} 个文件
                                </span>
                              )}
                            </div>
                          )}
                        </div>
                        <div className="flex shrink-0 items-center gap-[2px]">
                          <Btn
                            size="sm"
                            variant="ghost"
                            disabled={busy || !canReorder || i === 0}
                            title={canReorder ? "上移一位" : "先选中分组并清空搜索再排序"}
                            onClick={() => void moveReply(r, 1)}
                          >
                            ↑
                          </Btn>
                          <Btn
                            size="sm"
                            variant="ghost"
                            disabled={busy || !canReorder || i === replies.length - 1}
                            title={canReorder ? "下移一位" : "先选中分组并清空搜索再排序"}
                            onClick={() => void moveReply(r, -1)}
                          >
                            ↓
                          </Btn>
                          <Btn
                            size="sm"
                            variant="ghost"
                            disabled={busy}
                            onClick={() => setEditing({ reply: r, groupId: r.group_id })}
                          >
                            编辑
                          </Btn>
                          <Btn
                            size="sm"
                            variant="ghost"
                            disabled={busy}
                            className="text-danger-soft hover:bg-danger/[0.12]"
                            onClick={() => void removeReply(r)}
                          >
                            删除
                          </Btn>
                        </div>
                      </li>
                    );
                  })}
                </ul>
              )}
            </div>
          </div>
        </div>
      </CardGroup>

      <p className="text-[12px]/[16px] text-white/45">
        Excel 表头需含「标题」「内容」两列，「分组」列可选（没有则落入「默认」分组，不存在会自动创建）。
        附件按绝对路径保存，回贴时与正文一起写入剪贴板。
      </p>

      {editing && (
        <ReplyEditor
          groups={groups}
          editing={editing}
          onClose={() => setEditing(null)}
          onSaved={(created) => {
            setEditing(null);
            toast(created ? "已新建快捷回复" : "已保存修改", { tone: "success" });
          }}
        />
      )}
    </div>
  );
}

/** 新建 / 编辑对话框：标题、正文、附件；分组只在新建时可选（后端不支持改归属）。 */
function ReplyEditor({
  groups,
  editing,
  onClose,
  onSaved,
}: {
  groups: { id: number; name: string }[];
  editing: Editing;
  onClose: () => void;
  onSaved: (created: boolean) => void;
}) {
  const addReply = useReplyStore((s) => s.addReply);
  const updateReply = useReplyStore((s) => s.updateReply);

  const [groupId, setGroupId] = useState(editing.groupId);
  const [title, setTitle] = useState(editing.reply?.title ?? "");
  const [content, setContent] = useState(editing.reply?.content ?? "");
  const [files, setFiles] = useState<string[]>(editing.reply?.files ?? []);
  const [saving, setSaving] = useState(false);

  const isNew = editing.reply === null;
  // 标题留空时用正文首行兜底：列表里不能出现一排空白行。
  const effectiveTitle = title.trim() || content.trim().split("\n")[0]?.slice(0, 40) || "";
  const canSave = !saving && (title.trim().length > 0 || content.trim().length > 0 || files.length > 0);

  const save = async () => {
    if (!canSave) return;
    setSaving(true);
    try {
      if (isNew) {
        await addReply(groupId, effectiveTitle, content, files);
        onSaved(true);
      } else {
        await updateReply(editing.reply!.id, effectiveTitle, content, files);
        onSaved(false);
      }
    } catch (e) {
      toastError(e, isNew ? "新建失败" : "保存失败");
    } finally {
      setSaving(false);
    }
  };

  // Esc 关闭 / ⌘↵ 保存：capture + preventDefault，别再让主窗口热键跟着响应。
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        onClose();
        return;
      }
      if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        e.stopPropagation();
        void save();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [title, content, files, groupId, saving]);

  const addFiles = async () => {
    const picked = await api.pickFiles(true).catch((e) => {
      toastError(e, "无法打开选择面板");
      return [] as string[];
    });
    if (picked.length === 0) return;
    setFiles((prev) => [...new Set([...prev, ...picked])]);
  };

  return (
    <div
      className="fixed inset-0 z-50 grid place-items-center bg-black/55 p-[24px]"
      role="dialog"
      aria-modal="true"
      aria-label={isNew ? "新建快捷回复" : "编辑快捷回复"}
      onClick={onClose}
    >
      <div
        className="max-h-[86vh] w-[440px] overflow-y-auto rounded-[12px] border border-white/12 bg-[#2C2C2E] p-[16px] shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="mb-[12px] text-[13px]/[18px] font-semibold text-white">
          {isNew ? "新建快捷回复" : "编辑快捷回复"}
        </div>

        {isNew ? (
          <div className="mb-[10px]">
            <Select
              aria-label="所属分组"
              value={String(groupId)}
              onChange={(e) => setGroupId(Number(e.target.value))}
              className="w-full"
            >
              {groups.map((g) => (
                <option key={g.id} value={g.id} className="bg-group">
                  {g.name}
                </option>
              ))}
            </Select>
          </div>
        ) : (
          <div className="mb-[10px] flex items-center gap-[6px] text-[12px]/[16px] text-white/45">
            <span>分组</span>
            <Pill selected className="pointer-events-none">
              {groups.find((g) => g.id === groupId)?.name ?? "未知分组"}
            </Pill>
            <span className="text-white/30">（归属不可改，可删除后重建）</span>
          </div>
        )}

        <Input
          aria-label="标题"
          autoFocus
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          placeholder="标题，例如：退款说明"
        />

        <textarea
          aria-label="内容"
          value={content}
          onChange={(e) => setContent(e.target.value)}
          placeholder="正文内容，回贴时原样写入剪贴板"
          className="input mt-[8px] h-auto min-h-[120px] resize-y py-[6px] leading-[20px]"
        />

        <div className="mt-[10px]">
          <div className="flex items-center gap-[8px]">
            <span className="text-[12px]/[16px] text-white/45">附件</span>
            <Btn size="sm" variant="ghost" onClick={() => void addFiles()}>
              添加文件…
            </Btn>
            {files.length > 0 && (
              <Btn size="sm" variant="ghost" onClick={() => setFiles([])}>
                清空
              </Btn>
            )}
          </div>
          {files.length > 0 && (
            <div className="mt-[6px] flex flex-wrap gap-[4px]">
              {files.map((f) => (
                <span
                  key={f}
                  className="inline-flex max-w-[240px] items-center gap-[4px] rounded-[5px] bg-white/[0.06] px-[6px] py-[2px] text-[11px]/[15px] text-white/65"
                  title={f}
                >
                  <span className="truncate">{fileBasename(f)}</span>
                  <button
                    type="button"
                    aria-label={`移除 ${fileBasename(f)}`}
                    onClick={() => setFiles((prev) => prev.filter((x) => x !== f))}
                    className="text-white/35 hover:text-white"
                  >
                    ×
                  </button>
                </span>
              ))}
            </div>
          )}
        </div>

        <div className="mt-[14px] flex items-center justify-end gap-[8px]">
          <span className="mr-auto text-[11px]/[15px] text-white/30">⌘↵ 保存 · Esc 取消</span>
          <Btn size="sm" onClick={onClose} disabled={saving}>
            取消
          </Btn>
          <Btn size="sm" variant="primary" onClick={() => void save()} disabled={!canSave}>
            {saving ? "保存中…" : "保存"}
          </Btn>
        </div>
      </div>
    </div>
  );
}
