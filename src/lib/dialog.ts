// 应用内对话框：confirm / prompt / alert。
//
// Tauri 的 WKWebView 不实现 window.confirm / prompt / alert（静默返回
// false/null），因此全部对话框走本模块 + <DialogHost> 自绘 UI。
// 用法：`await confirm("确定删除？")` / `await promptDialog("名称")` / `await alertDialog("提示")`。
//
// 评审 S1：早期实现只有一个 `current` 槽位，第二个请求会直接覆盖第一个，
// 第一个 Promise 永远不 resolve —— 连续删除多条时流程就永久挂起（await 卡死）。
// 现在改为 FIFO 队列：请求排队、逐个展示，关闭一个才推进下一个。

export interface DialogOptions {
  /** 标题（可选）。有标题时正文降级为次要文本，更接近 NSAlert。 */
  title?: string;
  /** 破坏性操作：确定键显示为红色（评审 U2）。 */
  danger?: boolean;
  /** 确定键文案，默认「确定」。 */
  okLabel?: string;
  /** 取消键文案，默认「取消」。 */
  cancelLabel?: string;
}

export type DialogRequest = DialogOptions & (
  | { kind: "confirm"; message: string; resolve: (ok: boolean) => void }
  | {
      kind: "prompt";
      message: string;
      defaultValue?: string;
      placeholder?: string;
      resolve: (value: string | null) => void;
    }
  | { kind: "alert"; message: string; resolve: () => void }
);

/** 排队等待展示的请求，以及正在展示的那个。 */
const queue: DialogRequest[] = [];
let current: DialogRequest | null = null;
const listeners = new Set<() => void>();

function emit() {
  for (const l of listeners) l();
}

function enqueue(req: DialogRequest) {
  if (current) queue.push(req);
  else current = req;
  emit();
}

/** 订阅当前对话框请求（供 <DialogHost> 用）。 */
export function subscribeDialog(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** 当前正在展示的对话框请求。 */
export function getDialog(): DialogRequest | null {
  return current;
}

/** 关闭当前对话框并回传结果；队列里还有则立刻展示下一个。 */
export function closeDialog(result: boolean | string | null) {
  const req = current;
  if (!req) return;
  // 先推进队列再 resolve：否则回调里再弹一个对话框会覆盖 current。
  current = queue.shift() ?? null;
  emit();

  if (req.kind === "confirm") req.resolve(result === true);
  else if (req.kind === "prompt") req.resolve(typeof result === "string" ? result : null);
  else req.resolve();
}

/** 确认框：确定返回 true，取消返回 false。 */
export function confirm(message: string, options?: DialogOptions): Promise<boolean> {
  return new Promise((resolve) => {
    enqueue({ kind: "confirm", message, ...options, resolve });
  });
}

/** 输入框：确定返回输入值，取消返回 null。 */
export function promptDialog(
  message: string,
  defaultValue = "",
  options?: DialogOptions & { placeholder?: string },
): Promise<string | null> {
  return new Promise((resolve) => {
    enqueue({ kind: "prompt", message, defaultValue, ...options, resolve });
  });
}

/** 提示框：仅「确定」。 */
export function alertDialog(message: string, options?: DialogOptions): Promise<void> {
  return new Promise((resolve) => {
    enqueue({ kind: "alert", message, ...options, resolve });
  });
}
