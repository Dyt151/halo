/**
 * 剪贴板条目的共享交互逻辑。
 *
 * 面板窗口与管理窗口的「历史」页都要用到同一套动作（回贴/复制/删除/置顶/收藏/
 * 标签/堆栈/模块动作）与右键菜单状态，抽成 hook 避免两处各写一份、行为漂移。
 */
import { useEffect, useState } from "react";
import { useClipStore } from "../store/useClipStore";
import { api } from "../lib/ipc";
import { onModuleStateChanged } from "../lib/events";
import { confirm } from "../lib/dialog";
import { errorMessage } from "../lib/errors";
import { toast, toastError } from "../lib/toast";
import type { ClipItem as ClipItemType, ContributionEntry } from "../lib/types";
import type { ContextMenuState, ModuleAction } from "../components/ClipContextMenu";

/** 判断剪贴板条目是否满足模块声明的 mime 过滤条件。 */
export function matchesMime(contentType: ClipItemType["content_type"], mime: string[]): boolean {
  if (mime.length === 0) return true;
  const kind = contentType;
  return mime.some((m) => {
    if (m === "*" || m === "*/*") return true;
    const [group, sub] = m.split("/");
    if (sub === "*") return group === kind;
    if (kind === "text" && group === "text") return true;
    if (kind === "image" && group === "image") return true;
    if (kind === "html" && (m === "text/html" || group === "text")) return true;
    if (kind === "files" && m === "application/pdf") return true;
    return false;
  });
}

export function useClipActions() {
  const paste = useClipStore((s) => s.paste);
  const copy = useClipStore((s) => s.copy);
  const remove = useClipStore((s) => s.remove);
  const togglePin = useClipStore((s) => s.togglePin);
  const toggleFavorite = useClipStore((s) => s.toggleFavorite);
  const setClipTag = useClipStore((s) => s.setClipTag);
  const setError = useClipStore((s) => s.setError);
  const refresh = useClipStore((s) => s.refresh);
  // 标签全集来自后端 DISTINCT（评审 G2），不再从「当前已过滤的列表」里收集。
  const availableTags = useClipStore((s) => s.tags);
  const confirmOnDelete = useClipStore((s) => s.config?.confirm_on_delete) ?? true;

  const [menu, setMenu] = useState<ContextMenuState | null>(null);
  const [contributions, setContributions] = useState<ContributionEntry[]>([]);

  useEffect(() => {
    const load = () => {
      void api
        .listModuleContributions()
        .then(setContributions)
        .catch(() => setContributions([]));
    };
    load();
    // 模块启用/停用会改变右键菜单里的「模块动作」：跟着刷，否则要重开窗口才生效。
    const un = onModuleStateChanged(load);
    return () => void un.then((fn) => fn());
  }, []);

  const moduleActions: ModuleAction[] = menu
    ? contributions
        .filter((c) => c.contribution.type === "clipboard_action")
        .filter((c) =>
          matchesMime(
            menu.clip.content_type,
            c.contribution.type === "clipboard_action" ? c.contribution.mime : [],
          ),
        )
        .map((c) => ({
          module_id: c.module_id,
          module_name: c.module_name,
          label: c.contribution.type === "clipboard_action" ? c.contribution.label : c.module_name,
        }))
    : [];

  return {
    menu,
    setMenu,
    moduleActions,
    availableTags,
    openMenu: (e: React.MouseEvent, clip: ClipItemType) => {
      e.preventDefault();
      setMenu({ x: e.clientX, y: e.clientY, clip });
    },
    handlers: {
      onPaste: (clip: ClipItemType) => void paste(clip.id),
      onCopy: (clip: ClipItemType) => void copy(clip.id),
      onDelete: async (clip: ClipItemType) => {
        // 删除是破坏性操作：确认键必须红色（评审 U2）。
        if (confirmOnDelete && !(await confirm("确定删除这条记录？", { danger: true }))) return;
        void remove(clip.id);
      },
      onTogglePin: (clip: ClipItemType, v: boolean) => void togglePin(clip.id, v),
      onToggleFavorite: (clip: ClipItemType, v: boolean) => void toggleFavorite(clip.id, v),
      onPushToStack: (clip: ClipItemType) => {
        void api
          .stackPush(clip.id)
          .then(() => toast("已加入堆栈"))
          .catch((e) => {
            setError(errorMessage(e, "加入堆栈失败"));
            toastError(e, "加入堆栈失败");
          });
      },
      onSetTag: (clip: ClipItemType, tag: string | null) => void setClipTag(clip.id, tag),
      onModuleAction: (moduleId: string, clip: ClipItemType) => {
        void api
          .invokeModule(moduleId, "module.clipboard_action", { clip })
          .then(() => refresh())
          .catch((e) => {
            setError(errorMessage(e, "模块执行失败"));
            toastError(e, "模块执行失败");
          });
      },
    },
  };
}
