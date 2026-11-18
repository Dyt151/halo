//! 语音指令条：把「说一句中文」翻成一个动作。
//!
//! 为什么要有这一页：SayAll 那类语音 App 只会把话变成**文字**打进当前焦点输入框，
//! 它没有「语音指令」通道（本机 1.9.21 的 AI 部分只是用 DeepSeek 整理已上屏的文字）。
//! 所以流程是 Halo 提供一个能被听写的输入框，再把听写结果解释成动作：
//!
//! 1. 遥控器一格 → [`HotkeyAction::VoiceCmd`](crate::core::hotkey::HotkeyAction) → 弹出指令条并把光标放进输入框；
//! 2. 按住语音键说「打开抖音」，松手，字落在输入框里；
//! 3. 按确定键（回车）→ [`run`] 解释并执行。
//!
//! 解释规则全在 [`parse`]（纯函数，可单测），执行在 [`run`]。刻意不做的事：
//! 听不懂时不猜、不自动去搜索——语音转写经常多一个字少一个字，自作主张地开浏览器
//! 比什么都不做糟糕得多；没命中就把原因写清楚，让用户改口或改说「搜索 X」。

use serde::Serialize;
use tauri::{AppHandle, Manager};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as platform;

#[cfg(not(target_os = "macos"))]
mod other;
#[cfg(not(target_os = "macos"))]
use other as platform;

use crate::core::appindex;
use crate::state::AppState;
use crate::window;

/// 一句话解释出来的意图。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Intent {
    /// 打开某个 App（`String` 是用户说的那段名字，还没查过本机装没装）。
    OpenApp(String),
    QuitApp(String),
    Search(String),
    /// 已经补好 scheme 的完整网址。
    OpenUrl(String),
    ShowPanel,
    PasteStackTop,
    /// 没听懂，或者话是空的。
    Unknown,
}

/// 给指令条界面看的结果。
#[derive(Debug, Clone, Serialize)]
pub struct Outcome {
    pub ok: bool,
    /// `openApp` / `quitApp` / `search` / `openUrl` / `panel` / `paste` / `unknown`。
    pub kind: &'static str,
    /// 一句人话，直接显示在指令条下面那行。
    pub detail: String,
}

impl Outcome {
    fn done(kind: &'static str, detail: String) -> Self {
        Self {
            ok: true,
            kind,
            detail,
        }
    }

    fn failed(kind: &'static str, detail: String) -> Self {
        Self {
            ok: false,
            kind,
            detail,
        }
    }
}

/* ------------------------------------------------------------------ */
/* 解释                                                                */
/* ------------------------------------------------------------------ */

/// 客套开头：这些词在句子前面没有信息量，去掉后「帮我打开抖音」= 「打开抖音」。
const LEADING_FILLERS: &[&str] = &[
    "请帮我",
    "麻烦帮我",
    "能不能帮我",
    "可不可以",
    "帮我",
    "请问",
    "麻烦",
    "我想",
    "我要",
    "能不能",
    "可以",
    "请",
];

/// 句尾语气词：「打开抖音啊」「打开抖音吧」和「打开抖音」是同一句。
const TRAILING_PARTICLES: &[&str] = &["一下", "吧", "啊", "呀", "呢", "吗", "哦", "了", "嘛"];

/// 指代 Halo 自己的面板的词，整句就是它们时直接开面板。
const PANEL_WORDS: &[&str] = &[
    "面板",
    "剪贴板",
    "剪贴板面板",
    "剪贴板历史",
    "历史",
    "历史记录",
    "halo面板",
];

/// 粘贴栈顶的说法。
const PASTE_WORDS: &[&str] = &[
    "粘贴",
    "贴",
    "贴一下",
    "粘贴栈顶",
    "粘贴堆栈顶",
    "粘出去",
    "贴出去",
    "paste",
];

const OPEN_PREFIXES: &[&str] = &["打开", "开启", "启动", "运行", "唤起", "进入"];
const QUIT_PREFIXES: &[&str] = &["关闭", "关掉", "退出", "结束", "杀掉"];
const SEARCH_PREFIXES: &[&str] = &["搜索", "搜一下", "搜", "查询", "查一下", "查", "找找", "找"];

/// 去掉首尾的空白和标点。
fn trim_edges(raw: &str) -> String {
    raw.trim_matches(|c: char| {
        c.is_whitespace()
            || matches!(
                c,
                '。' | '，' | '、' | '！' | '？' | '；' | '.' | ',' | '!' | '?' | ';'
            )
    })
    .to_string()
}

/// 去掉首尾标点、客套话和语气词。
///
/// 三件事会互相挡路（「请打开抖音吧。」里客套话在前、语气词在后、句号在最末），
/// 所以反复剥到稳定为止，而不是排一遍。
fn clean(raw: &str) -> String {
    let mut text = trim_edges(raw);
    loop {
        let before = text.clone();
        if let Some(rest) = LEADING_FILLERS.iter().find_map(|f| text.strip_prefix(f)) {
            let next = trim_edges(rest);
            text = next;
        }
        // 「搜索一下天气」：语气词夹在动词后面，不是后缀，得单独拆。
        for prefix in OPEN_PREFIXES
            .iter()
            .chain(QUIT_PREFIXES)
            .chain(SEARCH_PREFIXES)
        {
            let joined = format!("{prefix}一下");
            if let Some(rest) = text.strip_prefix(joined.as_str()) {
                let next = format!("{prefix}{}", trim_edges(rest));
                text = next;
            }
        }
        for particle in TRAILING_PARTICLES {
            if let Some(rest) = text.strip_suffix(particle) {
                let next = trim_edges(rest);
                text = next;
            }
        }
        if text == before {
            return text;
        }
    }
}

/// 剥掉一个前缀，返回剩下的部分。
fn strip_prefix_any<'a>(text: &'a str, prefixes: &[&str]) -> Option<&'a str> {
    prefixes
        .iter()
        .find_map(|p| text.strip_prefix(p))
        .map(|rest| rest.trim_start_matches([' ', '，', '、']))
}

/// 整句匹配（已经小写、去空格）。
fn is_word(text: &str, words: &[&str]) -> bool {
    words.contains(&text)
}

/// 像不像一个域名：纯 ASCII、含点、末段是 2 个以上字母。
///
/// 要求纯 ASCII 是有意的——「抖音.app」这种中文串不能当网址开走，
/// 它该被当成 App 名。
fn looks_like_domain(text: &str) -> bool {
    if !text.is_ascii() || text.len() < 4 || !text.contains('.') {
        return false;
    }
    if text
        .chars()
        .any(|c| !(c.is_ascii_alphanumeric() || c == '-' || c == '.'))
    {
        return false;
    }
    match text.rsplit_once('.') {
        Some((_, tld)) => tld.len() >= 2 && tld.chars().all(|c| c.is_ascii_alphabetic()),
        None => false,
    }
}

/// 只去首尾标点与句内空格、转小写，**不剥任何词**。
fn compact(raw: &str) -> String {
    trim_edges(raw)
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_lowercase()
}

/// 把一句话解释成意图。
pub fn parse(raw: &str) -> Intent {
    // 整句就是「面板」「贴一下」时先按原话认一次：走 [`clean`] 会把句尾的「一下」
    // 当语气词剥掉，剩下一个孤零零的「贴」反而不在这两张词表里了。
    let spoken = compact(raw);
    if is_word(&spoken, PANEL_WORDS) {
        return Intent::ShowPanel;
    }
    if is_word(&spoken, PASTE_WORDS) {
        return Intent::PasteStackTop;
    }
    let text = clean(raw).to_lowercase();
    if text.is_empty() {
        return Intent::Unknown;
    }
    // 先认 Halo 自己的两件事，免得「打开面板」被当成一个叫「面板」的 App。
    if is_word(&text, PANEL_WORDS) {
        return Intent::ShowPanel;
    }
    if is_word(&text, PASTE_WORDS) {
        return Intent::PasteStackTop;
    }
    if text.starts_with("http://") || text.starts_with("https://") {
        return Intent::OpenUrl(text);
    }
    if let Some(rest) = strip_prefix_any(&text, QUIT_PREFIXES) {
        return target(rest, Intent::QuitApp, Intent::Unknown);
    }
    if let Some(rest) = strip_prefix_any(&text, SEARCH_PREFIXES) {
        return target(rest, Intent::Search, Intent::Unknown);
    }
    if let Some(rest) = strip_prefix_any(&text, OPEN_PREFIXES) {
        // 「打开面板」「打开剪贴板历史」还是指 Halo 的面板。
        if is_word(rest, PANEL_WORDS) {
            return Intent::ShowPanel;
        }
        return target(rest, Intent::OpenApp, Intent::Unknown);
    }
    if looks_like_domain(&text) {
        return Intent::OpenUrl(format!("https://{text}"));
    }
    // 光说一个名字（「抖音」「备忘录」）就当启动它：启动器的常规用法。
    // 认不出来时 [`run`] 会说明「没找到」，不会偷偷去搜索。
    Intent::OpenApp(text)
}

/// 前缀后面那段还得有内容，顺手也认一下「打开 youtube.com」这种。
fn target(rest: &str, app_like: impl Fn(String) -> Intent, fallback: Intent) -> Intent {
    let rest = rest.trim();
    if rest.is_empty() {
        return fallback;
    }
    if looks_like_domain(rest) {
        return Intent::OpenUrl(format!("https://{rest}"));
    }
    app_like(rest.to_string())
}

/* ------------------------------------------------------------------ */
/* 执行                                                                */
/* ------------------------------------------------------------------ */

/// 搜索引擎：`cn.bing.com` 在国内直连可用，Google 不行；这条链路的目标是
/// 「说完就能查到」，所以按能用的那个来。
const SEARCH_URL_PREFIX: &str = "https://cn.bing.com/search?q=";

/// 只保留 URL 安全字符，其余按 UTF-8 百分号编码。
///
/// 手写而不是引一个 crate：查询串里出现的只会是中文、字母数字和少量标点，
/// 未保留字符一律 `%XX` 就够，引依赖反而要多审一个包。
fn percent_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

pub fn search_url(query: &str) -> String {
    format!("{SEARCH_URL_PREFIX}{}", percent_encode(query))
}

/// 解释并执行一句话。
///
/// 需要把焦点还给用户原来那个 App 的动作（开面板、粘贴）**先收起指令条**再执行，
/// 否则粘贴会贴进自己的输入框；启动外部 App 的动作则先执行再收起。
pub fn run(app: &AppHandle, raw: &str) -> Outcome {
    let intent = parse(raw);
    match intent {
        Intent::ShowPanel => {
            window::hide_voice_cmd(app);
            window::show_panel_window(app);
            Outcome::done("panel", "已打开剪贴板面板".to_string())
        }
        Intent::PasteStackTop => {
            window::hide_voice_cmd(app);
            let state = app.state::<AppState>();
            match state.paste_stack_top() {
                Ok(Some(_)) => Outcome::done("paste", "已粘贴栈顶".to_string()),
                Ok(None) => Outcome::failed("paste", "堆栈是空的".to_string()),
                Err(e) => Outcome::failed("paste", e.message),
            }
        }
        Intent::OpenApp(query) => match appindex::find(&query) {
            Some(found) => match appindex::open(&found) {
                Ok(()) => {
                    window::hide_voice_cmd(app);
                    Outcome::done("openApp", format!("已打开 {}", found.name))
                }
                Err(message) => Outcome::failed("openApp", message),
            },
            None => Outcome::failed(
                "openApp",
                format!("没找到叫「{query}」的 App，改说「搜索 {query}」试试"),
            ),
        },
        Intent::QuitApp(query) => match appindex::quit(&query) {
            Ok(located) => {
                window::hide_voice_cmd(app);
                Outcome::done("quitApp", format!("已退出 {}", located.name))
            }
            Err(message) => Outcome::failed("quitApp", message),
        },
        Intent::Search(query) => {
            let url = search_url(&query);
            match platform::open_url(&url) {
                Ok(()) => {
                    window::hide_voice_cmd(app);
                    Outcome::done("search", format!("已搜索「{query}」"))
                }
                Err(message) => Outcome::failed("search", message),
            }
        }
        Intent::OpenUrl(url) => match platform::open_url(&url) {
            Ok(()) => {
                window::hide_voice_cmd(app);
                Outcome::done("openUrl", format!("已打开 {url}"))
            }
            Err(message) => Outcome::failed("openUrl", message),
        },
        Intent::Unknown => Outcome::failed(
            "unknown",
            "没听懂这句，可以说「打开 X」「搜索 X」".to_string(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{clean, parse, search_url, Intent};

    #[test]
    fn strips_fillers_and_particles() {
        assert_eq!(clean("  帮我打开抖音  "), "打开抖音");
        assert_eq!(clean("请打开抖音吧。"), "打开抖音");
        assert_eq!(clean("我要搜索一下 天气！"), "搜索天气");
        assert_eq!(clean("。。。"), "");
    }

    #[test]
    fn parses_open_and_quit_and_search() {
        assert_eq!(parse("打开抖音"), Intent::OpenApp("抖音".into()));
        assert_eq!(parse("启动 备忘录"), Intent::OpenApp("备忘录".into()));
        assert_eq!(parse("关闭微信"), Intent::QuitApp("微信".into()));
        assert_eq!(parse("搜索今天天气"), Intent::Search("今天天气".into()));
        // 光说名字 = 启动它。
        assert_eq!(parse("抖音"), Intent::OpenApp("抖音".into()));
    }

    #[test]
    fn halo_own_actions_win_over_app_names() {
        assert_eq!(parse("面板"), Intent::ShowPanel);
        assert_eq!(parse("打开剪贴板历史"), Intent::ShowPanel);
        assert_eq!(parse("粘贴"), Intent::PasteStackTop);
        assert_eq!(parse("贴一下"), Intent::PasteStackTop);
        assert_eq!(parse("帮我贴一下"), Intent::PasteStackTop);
    }

    #[test]
    fn urls_are_not_apps() {
        assert_eq!(
            parse("打开 youtube.com"),
            Intent::OpenUrl("https://youtube.com".into())
        );
        assert_eq!(
            parse("https://example.com/x"),
            Intent::OpenUrl("https://example.com/x".into())
        );
        // 中文名带个「.app」也不能被当成域名。
        assert_eq!(parse("打开抖音.app"), Intent::OpenApp("抖音.app".into()));
    }

    #[test]
    fn empty_and_prefix_only_are_unknown() {
        assert_eq!(parse(""), Intent::Unknown);
        assert_eq!(parse("打开"), Intent::Unknown);
        assert_eq!(parse("搜索。"), Intent::Unknown);
    }

    #[test]
    fn encodes_the_query() {
        assert_eq!(
            search_url("今天 天气"),
            "https://cn.bing.com/search?q=%E4%BB%8A%E5%A4%A9%20%E5%A4%A9%E6%B0%94"
        );
        // 引号/问号这类字符必须被编码掉，不然能改写查询串。
        assert!(search_url("a&b=c?x").contains("%26"));
    }
}
