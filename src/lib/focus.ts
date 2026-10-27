/**
 * 焦点判定的公共小工具。
 *
 * 为什么单独成文件：`⌘Z` 在两个地方需要「这是不是在输入框里」这个判断——
 * 面板的键盘处理（输入框里的 ⌘Z 归系统）和菜单栏转发的撤销事件（输入框里
 * 要做文本撤销而不是撤销删除）。两处各写一份很容易走偏。
 */

/** 可以被「文本撤销」的元素类型（checkbox/radio 之类没有文本可撤销）。 */
const NON_TEXT_INPUT_TYPES = new Set([
  "button",
  "checkbox",
  "color",
  "file",
  "hidden",
  "image",
  "radio",
  "range",
  "reset",
  "submit",
]);

/**
 * 给定元素是否是正在接受文本编辑的控件。
 *
 * 只读/禁用控件不算：往只读输入框里打不进字，⌘Z 自然也不该当成文本撤销。
 */
export function isTextEditingAt(el: Element | null | undefined): boolean {
  if (!el) return false;
  if (el instanceof HTMLTextAreaElement) return !el.disabled && !el.readOnly;
  if (el instanceof HTMLInputElement) {
    if (el.disabled || el.readOnly) return false;
    return !NON_TEXT_INPUT_TYPES.has(el.type);
  }
  return el instanceof HTMLElement && el.isContentEditable;
}

/** 当前焦点是否在可编辑文本里。 */
export function isTextEditing(): boolean {
  return isTextEditingAt(document.activeElement);
}
