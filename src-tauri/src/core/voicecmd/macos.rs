//! macOS：用默认浏览器打开一个网址。
//!
//! 走 `/usr/bin/open` 而不是 `NSWorkspace openURL`：前者能拿到 stderr，失败时能把
//! 系统那句原因原样报给界面；两者在「交给默认浏览器」这件事上没有区别。

/// 只放行 http/https，且不能以 `-` 开头（否则会被 `open` 当成自己的选项）。
fn is_safe_url(url: &str) -> bool {
    (url.starts_with("http://") || url.starts_with("https://"))
        && !url.starts_with('-')
        && !url.contains('\0')
        && url.chars().all(|c| !c.is_control())
}

pub fn open_url(url: &str) -> Result<(), String> {
    if !is_safe_url(url) {
        return Err(format!("这个网址我不敢直接打开：{url}"));
    }
    let output = std::process::Command::new("/usr/bin/open")
        .arg(url)
        .output()
        .map_err(|e| format!("无法打开浏览器：{e}"))?;
    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "无法打开 {url}：{}",
        String::from_utf8_lossy(&output.stderr).trim()
    ))
}

#[cfg(test)]
mod tests {
    use super::is_safe_url;

    #[test]
    fn only_http_and_https() {
        assert!(is_safe_url("https://cn.bing.com/search?q=%E4%BD%A0"));
        assert!(is_safe_url("http://example.com"));
        assert!(!is_safe_url("file:///etc/passwd"));
        assert!(!is_safe_url("javascript:alert(1)"));
        assert!(!is_safe_url("-a /Applications"));
        assert!(!is_safe_url("https://example.com/a\nb"));
    }
}
