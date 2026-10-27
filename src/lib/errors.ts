/**
 * 错误对象的统一形态（前端唯一入口）。
 *
 * Tauri 2 的命令失败时，前端拿到的**不是字符串**：`AppError` 先经
 * `serde_json::to_value` 变成 `{ code, message, details }`，再以 `application/json`
 * 回传，`invoke` 的 Promise 就用这个对象 reject。所以：
 *
 * - `String(e)` → `"[object Object]"`；
 * - `e instanceof Error` → false，`e.message` 取不到；
 * - 各处 `toastError(e)` 会退化成兜底文案。
 *
 * 这里把它折成 `ApiError`：`message` 是可读文案，`toString()` 也返回同一句
 * （旧的 `String(e)` 调用点不用逐个改），同时保留 `code` / `details` 供将来分支。
 */

/** 后端错误码：与 `src-tauri/src/error.rs` 的分段约定一致（1xxx 剪贴板 … 9xxx 通用）。 */
export interface AppErrorPayload {
  code?: number;
  message?: string;
  details?: string;
}

/** 归一化后的接口错误：可读文案 + 原始错误码。 */
export class ApiError extends Error {
  readonly code: number | null;
  readonly details: string | null;

  constructor(message: string, code: number | null = null, details: string | null = null) {
    super(message);
    this.name = "ApiError";
    this.code = code;
    this.details = details;
  }

  /** 默认 `Error#toString()` 会加 `Error: ` 前缀，这里保持纯文案。 */
  toString(): string {
    return this.message;
  }
}

/** 尝试把「长得像 JSON 的字符串」解成错误对象（兼容早期把错误 stringify 过的路径）。 */
function parseJsonish(text: string): AppErrorPayload | null {
  const trimmed = text.trim();
  if (!trimmed.startsWith("{")) return null;
  try {
    const parsed = JSON.parse(trimmed) as unknown;
    if (parsed && typeof parsed === "object") return parsed as AppErrorPayload;
  } catch {
    // 不是合法 JSON：按普通文本处理。
  }
  return null;
}

/** 从后端错误对象/字符串里取出可读文案；`message` 与 `details` 都有时用「：」拼接。 */
function messageOf(payload: AppErrorPayload): string {
  const parts = [payload.message, payload.details].filter(
    (v): v is string => typeof v === "string" && v.trim().length > 0,
  );
  return parts.join("：").trim();
}

/**
 * 把任意异常折成 `ApiError`。
 *
 * - 已经是 `ApiError` / `Error`：原样（或取其 message）；
 * - 后端对象 `{ code, message, details }`：拼成 `message：details`；
 * - JSON 字符串：先解析再走上面那条；
 * - 其余：`String(e)`，空串时用 `fallback`。
 */
export function toApiError(e: unknown, fallback = "操作失败"): ApiError {
  if (e instanceof ApiError) return e;
  if (e instanceof Error) return new ApiError(e.message || fallback);

  if (typeof e === "string") {
    const parsed = parseJsonish(e);
    const text = parsed ? messageOf(parsed) : e.trim();
    return new ApiError(
      text || fallback,
      parsed?.code ?? null,
      parsed?.details ?? null,
    );
  }

  if (e && typeof e === "object") {
    const payload = e as AppErrorPayload;
    const text = messageOf(payload);
    if (text) {
      return new ApiError(
        text,
        typeof payload.code === "number" ? payload.code : null,
        typeof payload.details === "string" && payload.details.trim() ? payload.details : null,
      );
    }
  }

  const text = e === null || e === undefined ? "" : String(e);
  return new ApiError(text && text !== "[object Object]" ? text : fallback);
}

/** 直接取可读文案（`setError(...)`、toast、就地提示都用它）。 */
export function errorMessage(e: unknown, fallback = "操作失败"): string {
  return toApiError(e, fallback).message;
}
