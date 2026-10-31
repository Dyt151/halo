//! 文本象限的纯转换函数（HTML / TXT 互转）。
//!
//! 全部无 I/O、无副作用，可单测；落盘由 `engines/text.rs` 负责。
//! 目标是「结构大致保留、正文一个字不丢」，不做规范级还原：表格拍平成竖线、
//! 嵌套列表不缩进、样式与脚本直接丢弃。真正的富文本往返要等排版引擎。

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
    fn txt_to_html_escapes_and_keeps_pre() {
        let html = text_to_html("<b>不是标签</b> & 保留");
        assert!(html.contains("&lt;b&gt;不是标签&lt;/b&gt; &amp; 保留"), "{html}");
        assert!(html.contains("<pre>"), "{html}");
    }
}
