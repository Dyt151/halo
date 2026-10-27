import { describe, it, expect } from "vitest";
import { sanitizeHtml, htmlToPlainText } from "./sanitize";

describe("sanitizeHtml", () => {
  it("空输入返回空串", () => {
    expect(sanitizeHtml("")).toBe("");
  });

  it("纯文本原样返回", () => {
    expect(sanitizeHtml("hello world")).toBe("hello world");
  });

  it("纯文本里的 < > 被转义", () => {
    expect(sanitizeHtml("a < b && c > d")).toBe("a &lt; b &amp;&amp; c &gt; d");
  });

  it("白名单排版标签保留", () => {
    expect(sanitizeHtml("<b>bold</b>")).toBe("<b>bold</b>");
  });

  it("script 整棵丢弃", () => {
    expect(sanitizeHtml('<script>alert("xss")</script>')).toBe("");
  });

  it("img / svg / iframe 等资源标签丢弃", () => {
    expect(sanitizeHtml('<img src="x" onerror="alert(1)">')).toBe("");
    expect(sanitizeHtml("<svg onload=alert(1)></svg>")).toBe("");
    expect(sanitizeHtml("<iframe src=evil></iframe>")).toBe("");
  });

  it("事件属性被剥离", () => {
    expect(sanitizeHtml('<p onclick="alert(1)">hi</p>')).toBe("<p>hi</p>");
  });

  it("a 标签丢弃 href、保留 title", () => {
    expect(sanitizeHtml('<a href="https://evil.com" title="说明">链接</a>')).toBe(
      '<a title="说明">链接</a>',
    );
  });

  it("未知标签拆掉但保留子文本", () => {
    expect(sanitizeHtml("<custom>text</custom>")).toBe("text");
  });

  it("注释 / 处理指令丢弃", () => {
    expect(sanitizeHtml("<!-- hidden --><p>x</p>")).toBe("<p>x</p>");
  });

  it("危险属性值不含任何脚本执行点", () => {
    const html =
      '<a href="javascript:alert(1)" style="position:fixed">x</a>' +
      '<table background="javascript:alert(2)"><tr><td>y</td></tr></table>';
    const clean = sanitizeHtml(html);
    expect(clean).not.toContain("javascript:");
    expect(clean).not.toContain("href=");
    expect(clean).not.toContain("style=");
  });
});

describe("htmlToPlainText", () => {
  it("空输入返回空串", () => {
    expect(htmlToPlainText("")).toBe("");
  });

  it("纯文本原样返回", () => {
    expect(htmlToPlainText("plain")).toBe("plain");
  });

  it("块级元素之间补换行", () => {
    expect(htmlToPlainText("<p>a</p><p>b</p>")).toBe("a\nb");
  });

  it("连续换行被压缩为一个空行", () => {
    expect(htmlToPlainText("<p>a</p><p></p><p></p><p></p><p>b</p>")).toBe("a\n\nb");
    expect(htmlToPlainText("<p>a</p><p></p><p></p><p>b</p>")).not.toContain("\n\n\n");
  });
});
