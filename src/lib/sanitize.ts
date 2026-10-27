/**
 * 剪贴板 HTML 的白名单消毒（评审 F1）。
 *
 * 旧实现把 `clip.content.value` 直接塞进 `dangerouslySetInnerHTML`：剪贴板内容
 * 完全由外部 App 决定，一段 `<img onerror="fetch('file:///...')">` 就能在卡片
 * 预览里执行脚本，配合曾经的 `csp: null` + `assetProtocol scope: ["**"]`
 * 直接构成任意本地文件读取 / 任意命令调用。
 *
 * 这里的策略是「白名单 + 丢弃不可信内容」，而不是「黑名单过滤」：
 * - 只保留排版类标签（p / b / ul / table …），其余整棵子树丢弃；
 * - 只保留 `a[title]` 这类无害属性，`href` 一律丢弃（点了会导航离开应用）；
 * - `<img>` / `<svg>` / `<style>` / `<iframe>` / `<script>` 全丢弃，
 *   预览里只呈现文字与结构，图片原图另有 image 分支展示。
 *
 * 输出仍然是 HTML 字符串（用于 innerHTML），但每个标签与属性都来自本文件的
 * 常量表，不再包含任何来自剪贴板的标签名或属性名。
 */

/** 允许保留的标签（只挑纯排版语义的）。 */
const ALLOWED_TAGS = new Set([
  "p",
  "br",
  "div",
  "span",
  "b",
  "strong",
  "i",
  "em",
  "u",
  "s",
  "del",
  "sub",
  "sup",
  "small",
  "code",
  "pre",
  "blockquote",
  "ul",
  "ol",
  "li",
  "dl",
  "dt",
  "dd",
  "hr",
  "h1",
  "h2",
  "h3",
  "h4",
  "h5",
  "h6",
  "table",
  "thead",
  "tbody",
  "tfoot",
  "tr",
  "td",
  "th",
  "caption",
  "a",
]);

/** 允许保留的属性（按标签）。`a` 只留 title，href 丢弃。 */
const ALLOWED_ATTRS: Record<string, string[]> = {
  a: ["title"],
  td: ["colspan", "rowspan"],
  th: ["colspan", "rowspan"],
};

/** 直接整棵丢弃的标签：即便内容也想保留语义，也不值得冒风险。 */
const DROP_SUBTREE = new Set([
  "script",
  "style",
  "iframe",
  "frame",
  "frameset",
  "object",
  "embed",
  "applet",
  "img",
  "picture",
  "video",
  "audio",
  "canvas",
  "svg",
  "math",
  "template",
  "link",
  "meta",
  "base",
  "form",
  "input",
  "button",
  "select",
  "textarea",
  "noscript",
]);

/**
 * 消毒一段 HTML，返回可安全 innerHTML 的字符串。
 *
 * 解析用独立的 inert document（DOMParser 产物不执行脚本、不加载资源），
 * 因此整个过程不会触发任何副作用。
 */
export function sanitizeHtml(html: string): string {
  if (!html) return "";
  // 先粗判：没有标签就按纯文本处理，省掉解析开销（绝大多数卡片是纯文本）。
  if (!html.includes("<")) return escapeText(html);

  const doc = new DOMParser().parseFromString(html, "text/html");
  const out = doc.createElement("div");
  for (const node of Array.from(doc.body.childNodes)) {
    const clean = cleanNode(doc, node);
    if (clean) out.appendChild(clean);
  }
  return out.innerHTML;
}

function cleanNode(doc: Document, node: Node): Node | null {
  if (node.nodeType === Node.TEXT_NODE) {
    return doc.createTextNode(node.nodeValue ?? "");
  }
  if (node.nodeType !== Node.ELEMENT_NODE) {
    // 注释 / CDATA / 处理指令一律丢弃（条件注释是历史 XSS 入口）。
    return null;
  }

  const el = node as Element;
  const tag = el.tagName.toLowerCase();
  if (DROP_SUBTREE.has(tag)) return null;

  // 不在白名单里的标签：拆掉标签本身、保留其子节点文本，
  // 避免「整段内容消失」这种更难理解的行为。
  if (!ALLOWED_TAGS.has(tag)) {
    const frag = doc.createDocumentFragment();
    for (const child of Array.from(el.childNodes)) {
      const clean = cleanNode(doc, child);
      if (clean) frag.appendChild(clean);
    }
    return frag;
  }

  const rebuilt = doc.createElement(tag);
  const allowed = ALLOWED_ATTRS[tag] ?? [];
  for (const attr of allowed) {
    const value = el.getAttribute(attr);
    if (value !== null) rebuilt.setAttribute(attr, value);
  }
  for (const child of Array.from(el.childNodes)) {
    const clean = cleanNode(doc, child);
    if (clean) rebuilt.appendChild(clean);
  }
  return rebuilt;
}

/** 纯文本 → 转义后的 HTML（保留换行）。 */
function escapeText(text: string): string {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/\n/g, "<br>");
}

/** HTML → 纯文本（`text_preview = plain` 时用）。 */
export function htmlToPlainText(html: string): string {
  if (!html) return "";
  if (!html.includes("<")) return html;
  const doc = new DOMParser().parseFromString(html, "text/html");
  // block 级元素之间补换行，否则整段文字会粘成一行。
  for (const el of Array.from(doc.body.querySelectorAll("p,div,li,tr,br,h1,h2,h3,h4,h5,h6"))) {
    el.appendChild(doc.createTextNode("\n"));
  }
  return (doc.body.textContent ?? "").replace(/\n{3,}/g, "\n\n").trim();
}
