//! 文本象限的纯转换函数（Markdown / HTML / TXT 三方互转）。
//!
//! 全部无 I/O、无副作用，可单测；落盘由 `engines/text.rs` 负责。
//! 目标是「结构大致保留、正文一个字不丢」，不做规范级还原：表格拍平成竖线、
//! 嵌套列表不缩进、样式与脚本直接丢弃。真正的富文本往返要等排版引擎。

/// Markdown → HTML（`pulldown-cmark`，CommonMark）。
pub fn markdown_to_html(md: &str) -> String {
    use pulldown_cmark::html;
    let parser = pulldown_cmark::Parser::new(md);
    let mut out = String::new();
    html::push_html(&mut out, parser);
    out
}

/// Markdown → 纯文本：提取文本事件，段落/标题后空行，列表项换行。
pub fn markdown_to_text(md: &str) -> String {
    use pulldown_cmark::{Event, Parser, Tag, TagEnd};

    let mut out = String::with_capacity(md.len());
    for event in Parser::new(md) {
        match event {
            Event::Text(text) | Event::Code(text) => out.push_str(&text),
            Event::SoftBreak | Event::HardBreak => out.push('\n'),
            Event::Start(Tag::CodeBlock(_)) => out.push('\n'),
            Event::End(TagEnd::Paragraph) | Event::End(TagEnd::Heading(_)) => {
                out.push_str("\n\n")
            }
            Event::End(TagEnd::CodeBlock) => out.push('\n'),
            Event::End(TagEnd::Item) => out.push('\n'),
            _ => {}
        }
    }
    normalize_text(&out)
}

/// 纯文本 → Markdown：内容原样保留（Markdown 兼容纯文本），只统一换行符
/// 并保证结尾换行，免得编辑器里最后一行被吞。
pub fn text_to_markdown(txt: &str) -> String {
    let mut out = txt.replace("\r\n", "\n").replace('\r', "\n");
    while out.ends_with('\n') {
        out.pop();
    }
    out.push('\n');
    out
}

/// 纯文本 → HTML：完整文档 + `<pre>` 保留空行与缩进（深色底，双击就能看）。
pub fn text_to_html(txt: &str) -> String {
    let body = escape_html(txt);
    format!(
        "<!doctype html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n<title>Document</title>\n\
         <style>\nbody {{ margin: 0; padding: 32px; background: #1b1b1f; color: #e8e8ea; }}\n\
         pre {{ margin: 0; white-space: pre-wrap; word-break: break-word;\n\
         font: 14px/1.7 ui-monospace, SFMono-Regular, Menlo, monospace; }}\n</style>\n\
         </head>
<body>
<pre>{body}</pre>
</body>
</html>
"
    )
}

/// HTML 转义（`decode_entities` 的反向）：拼 HTML 时正文必须转义。
pub fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

/// 块级标签：闭合后插入换行。
const BLOCK_TAGS: &[&str] = &[
    "p", "div", "hr", "li", "ul", "ol", "tr", "td", "th", "table", "thead", "tbody", "h1", "h2",
    "h3", "h4", "h5", "h6", "section", "article", "header", "footer", "nav", "aside", "blockquote",
    "pre", "dl", "dt", "dd", "figure", "figcaption", "form", "main", "address",
];

/// HTML → 纯文本：去标签（`script`/`style` 内容整体丢弃、注释丢弃）、
/// 块级标签转换行、解码常见字符实体。
pub fn html_to_text(html: &str) -> String {
    let lower = html.to_ascii_lowercase();
    let mut out = String::with_capacity(html.len());
    let mut i = 0usize;
    // 处于 script/style 内部时，记录等待的闭合标签名。
    let mut skip_close: Option<&'static str> = None;

    while i < html.len() {
        if let Some(close) = skip_close {
            let needle = format!("</{close}");
            match lower[i..].find(&needle) {
                Some(rel) => {
                    i += rel + needle.len();
                    if let Some(gt) = lower[i..].find('>') {
                        i += gt + 1;
                    }
                    skip_close = None;
                }
                // 未闭合：丢弃剩余内容。
                None => break,
            }
            continue;
        }

        if html.as_bytes()[i] == b'<' {
            if lower[i..].starts_with("<!--") {
                match lower[i + 4..].find("-->") {
                    Some(end) => {
                        i += 4 + end + 3;
                        continue;
                    }
                    None => break,
                }
            }
            if let Some(gt_rel) = lower[i..].find('>') {
                let inner = &lower[i + 1..i + gt_rel];
                let closing = inner.starts_with('/');
                let name = inner
                    .trim_start_matches('/')
                    .split(|c: char| c.is_whitespace() || c == '/')
                    .next()
                    .unwrap_or("");
                i += gt_rel + 1;
                match name {
                    "script" if !closing => skip_close = Some("script"),
                    "style" if !closing => skip_close = Some("style"),
                    "br" | "hr" => push_newline(&mut out),
                    n if BLOCK_TAGS.contains(&n) && closing => push_newline(&mut out),
                    _ => {}
                }
                continue;
            }
            // 孤立的 '<'：当普通文本。
            out.push('<');
            i += 1;
            continue;
        }

        // 普通文本字符（按 UTF-8 字符边界推进）。
        let ch = html[i..].chars().next().unwrap_or('\u{FFFD}');
        out.push(ch);
        i += ch.len_utf8();
    }

    normalize_text(&decode_entities(&out))
}

/// 追加换行（行首不重复追加）。
fn push_newline(out: &mut String) {
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
}

/// 保证前方正好空一行（Markdown 里块级元素之间需要）。
fn push_blank_line(out: &mut String) {
    if out.is_empty() {
        return;
    }
    push_newline(out);
    if !out.ends_with("\n\n") {
        out.push('\n');
    }
}

/// HTML → Markdown：把常见块级/行内标签翻成对应标记，其余标签丢弃。
pub fn html_to_markdown(html: &str) -> String {
    let lower = html.to_ascii_lowercase();
    let mut out = String::with_capacity(html.len());
    let mut i = 0usize;
    // 处于 script/style/head 内部时，等待对应闭合标签。
    let mut skip_close: Option<&'static str> = None;
    // `pre` 内部按代码块处理：不翻译标签、不折叠空白。
    let mut in_pre = false;
    // 非 `pre` 状态下折叠连续空白（HTML 语义），避免源码缩进变成一堆断行。
    let mut pending_space = false;
    // 当前 `<a>` 的 href（Markdown 的链接目标写在结束位置）。
    let mut href: Option<String> = None;

    while i < html.len() {
        if let Some(close) = skip_close {
            let needle = format!("</{close}");
            match lower[i..].find(&needle) {
                Some(rel) => {
                    i += rel + needle.len();
                    if let Some(gt) = lower[i..].find('>') {
                        i += gt + 1;
                    }
                    skip_close = None;
                }
                None => break,
            }
            continue;
        }

        if html.as_bytes()[i] == b'<' {
            if lower[i..].starts_with("<!--") {
                match lower[i + 4..].find("-->") {
                    Some(end) => {
                        i += 4 + end + 3;
                        continue;
                    }
                    None => break,
                }
            }
            let Some(gt_rel) = lower[i..].find('>') else {
                out.push('<');
                i += 1;
                continue;
            };
            // 属性值要用保留大小写的原串，标签名用低串。
            let raw_inner = &html[i + 1..i + gt_rel];
            let inner = &lower[i + 1..i + gt_rel];
            let closing = inner.starts_with('/');
            let name = inner
                .trim_start_matches('/')
                .split(|c: char| c.is_whitespace() || c == '/')
                .next()
                .unwrap_or("");
            i += gt_rel + 1;

            if in_pre && name != "pre" {
                continue;
            }
            match name {
                "script" | "style" | "head" => {
                    if !closing {
                        skip_close = Some(match name {
                            "script" => "script",
                            "style" => "style",
                            _ => "head",
                        });
                    }
                    continue;
                }
                _ => {}
            }
            // 标签前的空白要落到文本里（`正文 <strong>` 不能变成 `正文**粗**`）。
            if pending_space {
                pending_space = false;
                if !out.is_empty() && !out.ends_with(|c: char| c.is_whitespace()) {
                    out.push(' ');
                }
            }
            push_md_tag(&mut out, name, closing, raw_inner, &mut in_pre, &mut href);
            continue;
        }

        let ch = html[i..].chars().next().unwrap_or('\u{FFFD}');
        i += ch.len_utf8();
        if in_pre {
            out.push(ch);
            continue;
        }
        if ch.is_whitespace() {
            pending_space = true;
            continue;
        }
        if pending_space {
            pending_space = false;
            if !out.is_empty() && !out.ends_with('\n') {
                out.push(' ');
            }
        }
        out.push(ch);
    }

    normalize_markdown(&decode_entities(&out))
}

/// 单个标签 → Markdown 标记（属性值从保留大小写的标签内部取）。
fn push_md_tag(
    out: &mut String,
    name: &str,
    closing: bool,
    raw_inner: &str,
    in_pre: &mut bool,
    href: &mut Option<String>,
) {
    match name {
        "pre" => {
            if closing {
                *in_pre = false;
                push_newline(out);
                out.push_str("```\n\n");
            } else {
                *in_pre = true;
                push_blank_line(out);
                out.push_str("```\n");
            }
        }
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
            if closing {
                out.push_str("\n\n");
            } else {
                push_blank_line(out);
                let level = name.as_bytes()[1] - b'0';
                for _ in 0..level {
                    out.push('#');
                }
                out.push(' ');
            }
        }
        "p" | "div" | "section" | "article" | "header" | "footer" | "nav" | "aside"
        | "main" | "figure" | "figcaption" => {
            if closing {
                out.push_str("\n\n");
            }
        }
        "br" => out.push_str("  \n"),
        "hr" => {
            push_blank_line(out);
            out.push_str("---\n\n");
        }
        "strong" | "b" => out.push_str("**"),
        "em" | "i" => out.push('*'),
        "code" => out.push('`'),
        "del" | "s" => out.push_str("~~"),
        "blockquote" => {
            if closing {
                push_newline(out);
            } else {
                push_newline(out);
                out.push_str("> ");
            }
        }
        "a" => {
            if closing {
                out.push_str("](");
                out.push_str(&href.take().unwrap_or_default());
                out.push(')');
            } else {
                *href = attr(raw_inner, "href");
                out.push('[');
            }
        }
        "img" => {
            let src = attr(raw_inner, "src").unwrap_or_default();
            let alt = attr(raw_inner, "alt").unwrap_or_default();
            out.push_str("![");
            out.push_str(&alt);
            out.push_str("](");
            out.push_str(&src);
            out.push(')');
        }
        "li" => {
            if closing {
                push_newline(out);
            } else {
                push_newline(out);
                out.push_str("- ");
            }
        }
        "ul" | "ol" | "table" | "tr" => {
            if closing {
                push_newline(out);
            }
        }
        "td" | "th" if !closing => {
            out.push_str(" | ");
        }
        _ => {}
    }
}

/// 从标签内部文本里取属性值（key 大小写不敏感，支持引号与无引号写法）。
fn attr(tag_inner: &str, key: &str) -> Option<String> {
    let lower = tag_inner.to_ascii_lowercase();
    let mut from = 0usize;
    while from < lower.len() {
        let rel = lower[from..].find(key)?;
        let at = from + rel;
        let before_ok = at == 0
            || (!tag_inner.as_bytes()[at - 1].is_ascii_alphanumeric()
                && tag_inner.as_bytes()[at - 1] != b'-');
        let rest = tag_inner.get(at + key.len()..)?.trim_start();
        if before_ok {
            if let Some(rest) = rest.strip_prefix('=') {
                let rest = rest.trim_start();
                let mut chars = rest.chars();
                return match chars.next() {
                    Some(q @ ('"' | '\'')) => {
                        let body = &rest[1..];
                        body.find(q).map(|end| body[..end].to_string())
                    }
                    Some(_) => {
                        let end = rest
                            .find(|c: char| c.is_whitespace() || c == '>')
                            .unwrap_or(rest.len());
                        Some(rest[..end].to_string())
                    }
                    None => None,
                };
            }
        }
        from = at + key.len();
    }
    None
}

/// Markdown 收尾：折叠 3 个以上连续空行为一个空行、去掉首尾空行；
/// 行尾空格保留（`<br>` 转成的硬换行靠两个行尾空格）。
fn normalize_markdown(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut blanks = 0usize;
    for line in text.lines() {
        if line.trim().is_empty() {
            blanks += 1;
            continue;
        }
        if !out.is_empty() {
            out.push('\n');
            if blanks > 0 {
                out.push('\n');
            }
        }
        blanks = 0;
        out.push_str(line.trim_end_matches('\t'));
    }
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

/// 解码常见 HTML 字符实体（命名实体 + 十进制/十六进制数字实体）。
pub fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(idx) = rest.find('&') {
        out.push_str(&rest[..idx]);
        let after = &rest[idx + 1..];
        if let Some(semi) = after.find(';') {
            if semi <= 32 {
                if let Some(decoded) = decode_entity(&after[..semi]) {
                    out.push_str(&decoded);
                    rest = &after[semi + 1..];
                    continue;
                }
            }
        }
        out.push('&');
        rest = after;
    }
    out.push_str(rest);
    out
}

/// 解码单个实体内容（不含 `&` 与 `;`）。
fn decode_entity(entity: &str) -> Option<String> {
    match entity {
        "amp" => Some("&".to_string()),
        "lt" => Some("<".to_string()),
        "gt" => Some(">".to_string()),
        "quot" => Some("\"".to_string()),
        "apos" => Some("'".to_string()),
        "nbsp" => Some("\u{00A0}".to_string()),
        _ => {
            let num = entity.strip_prefix('#')?;
            let code = if let Some(hex) = num.strip_prefix('x').or_else(|| num.strip_prefix('X')) {
                u32::from_str_radix(hex, 16).ok()?
            } else {
                num.parse::<u32>().ok()?
            };
            let ch = char::from_u32(code)?;
            // 过滤控制字符（保留 \t \n \r）。
            if ch.is_control() && !matches!(ch, '\t' | '\n' | '\r') {
                return None;
            }
            Some(ch.to_string())
        }
    }
}

/// 规范化纯文本：保留单个空行（折叠连续空行）、去掉每行行尾空白与首尾空行。
fn normalize_text(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut pending_newlines = 0usize;
    for line in text.lines() {
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            pending_newlines += 1;
            continue;
        }
        if !result.is_empty() {
            result.push('\n');
            if pending_newlines > 0 {
                result.push('\n');
            }
        }
        pending_newlines = 0;
        result.push_str(trimmed);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_to_html_basics() {
        let html = markdown_to_html("# 标题\n\n这是**粗体**与[链接](https://example.com)。\n");
        assert!(html.contains("<h1>标题</h1>"), "{html}");
        assert!(html.contains("<p>"), "{html}");
        assert!(html.contains("<strong>粗体</strong>"), "{html}");
        assert!(
            html.contains("<a href=\"https://example.com\">链接</a>"),
            "{html}"
        );
    }

    #[test]
    fn markdown_to_text_basics() {
        let text = markdown_to_text("# 标题\n\n第一段 **粗体** 与 `代码`。\n\n- 甲\n- 乙\n");
        assert!(text.contains("标题"), "{text}");
        assert!(text.contains("第一段 粗体 与 代码。"), "{text}");
        assert!(text.contains("甲\n乙"), "{text}");
        assert!(text.contains("标题\n\n第一段"), "{text}");
        assert!(!text.contains('#'), "{text}");
        assert!(!text.contains('*'), "{text}");
    }

    #[test]
    fn html_to_text_strips_tags_and_decodes_entities() {
        let text = html_to_text(
            "<p>Hello <b>World</b></p><p>Second &amp; last &#65;&#x42;</p><script>var x = 1;</script><!-- note -->",
        );
        assert_eq!(text, "Hello World\nSecond & last AB");
        assert!(!text.contains("var x"), "{text}");
        assert!(!text.contains("note"), "{text}");
    }

    #[test]
    fn html_to_text_handles_void_and_block_tags() {
        let text = html_to_text("<div>a<br>b</div><ul><li>x</li><li>y</li></ul>");
        assert_eq!(text, "a\nb\nx\ny");
    }

    #[test]
    fn txt_to_markdown_normalizes_newlines() {
        assert_eq!(text_to_markdown("第一行\r\n第二行"), "第一行\n第二行\n");
        assert_eq!(text_to_markdown("a\n\n\n"), "a\n");
    }

    #[test]
    fn txt_to_html_escapes_and_keeps_pre() {
        let html = text_to_html("<b>不是标签</b> & 保留");
        assert!(html.contains("&lt;b&gt;不是标签&lt;/b&gt; &amp; 保留"), "{html}");
        assert!(html.contains("<pre>"), "{html}");
    }

    #[test]
    fn html_to_markdown_common_structure() {
        let out = html_to_markdown(
            "<h2>标题</h2><p>正文 <strong>粗</strong> <a href=\"https://x.test\">链接</a></p>\
             <ul><li>甲</li><li>乙</li></ul><img src=\"p.png\" alt=\"图\">\
             <pre>let a = 1;</pre>",
        );
        assert!(out.contains("## 标题"), "{out}");
        assert!(out.contains("正文 **粗**"), "{out}");
        assert!(out.contains("[链接](https://x.test)"), "{out}");
        assert!(out.contains("- 甲"), "{out}");
        assert!(out.contains("![图](p.png)"), "{out}");
        assert!(out.contains("```\nlet a = 1;\n```"), "{out}");
    }
}
