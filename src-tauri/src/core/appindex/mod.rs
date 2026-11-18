//! 本机 App 索引：按名字找回某个已安装的 App，供语音指令「打开 X / 关闭 X」用。
//!
//! 为什么不能直接 `open -a X`：`-a` 走 LaunchServices 的名字解析，同名、带本地化
//! 显示名、或者装在 `/Applications/开源软件/` 这类子目录里的 App 经常解析不到，
//! 失败还是一句英文。这里改成自己扫一遍安装目录、读每个 `Info.plist` 的显示名，
//! 命中后拿**真实路径**去 `open`——路径是我们自己扫出来的，不存在把用户说的话
//! 当命令拼进去的风险。
//!
//! 别名取四个：`CFBundleDisplayName`、`CFBundleName`、`.app` 文件名去掉后缀、
//! `CFBundleExecutable`。中文界面名和前三个一致，英文名和最后一个一致，
//! 所以「打开抖音」和「打开 douyin」都能落到同一个 App。

use serde::Serialize;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as platform;

#[cfg(not(target_os = "macos"))]
mod other;
#[cfg(not(target_os = "macos"))]
use other as platform;

/// 一个已安装 App 的定位信息。
#[derive(Debug, Clone)]
pub struct AppInfo {
    /// 展示名（优先 `CFBundleDisplayName`，退到文件名）；界面回显用它。
    pub name: String,
    /// `.app` 的真实绝对路径，已经过 [`is_plausible_app_path`] 校验。
    pub path: String,
    pub bundle_id: Option<String>,
    /// 可用来匹配的名字别名（已小写、去空格）。
    pub aliases: Vec<String>,
}

/// 指令执行结果里给界面看的那部分。
#[derive(Debug, Clone, Serialize)]
pub struct Located {
    pub name: String,
    pub bundle_id: Option<String>,
}

impl AppInfo {
    fn located(&self) -> Located {
        Located {
            name: self.name.clone(),
            bundle_id: self.bundle_id.clone(),
        }
    }
}

/// 把用户说的名字归一化：去空格、转小写、去掉结尾的 `.app`。
///
/// 中文不受 `lowercase` 影响，等于只做去空格；这一步决定「打开 抖音」和
/// 「打开抖音」是同一个查询。
pub fn normalize_name(raw: &str) -> String {
    let compact: String = raw
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_ascii_lowercase();
    // 用 `strip_suffix` 而不是自己按字节切：中文别名里字节边界会切在字符中间。
    match compact.strip_suffix(".app") {
        Some(stripped) => stripped.to_string(),
        None => compact,
    }
}

/// 在候选里挑一个最合适的：完全同名 > 名字以它开头 > 它包含名字；同级取名字最短的。
///
/// 「最短优先」是有意的：说「备忘录」时 `备忘录` 要比 `备忘录提醒事项` 之类的长名字
/// 先命中，说「抖音」时不该被某个名字里带「抖音」的第三方壳抢走。
pub fn pick<'a>(candidates: &'a [AppInfo], query: &str) -> Option<&'a AppInfo> {
    let query = normalize_name(query);
    if query.is_empty() {
        return None;
    }
    let mut best: Option<(u8, usize, &'a AppInfo)> = None;
    for app in candidates {
        let Some(rank) = app
            .aliases
            .iter()
            .filter_map(|alias| match_name(alias, &query))
            .min()
        else {
            continue;
        };
        let score = (rank, alias_len(app));
        let replace = match best {
            None => true,
            Some(current) => current.0 > score.0 || (current.0 == score.0 && current.1 > score.1),
        };
        if replace {
            best = Some((rank, alias_len(app), app));
        }
    }
    best.map(|(_, _, app)| app)
}

/// 别名和查询的匹配等级：0 完全同名，1 别名以查询开头，2 别名包含查询。
fn match_name(alias: &str, query: &str) -> Option<u8> {
    if alias == query {
        Some(0)
    } else if alias.starts_with(query) {
        Some(1)
    } else if alias.contains(query) {
        Some(2)
    } else {
        None
    }
}

fn alias_len(app: &AppInfo) -> usize {
    app.aliases
        .iter()
        .map(String::len)
        .min()
        .unwrap_or(usize::MAX)
}

/// 苹果自家 App 的中文名不在 App 包里（访达是从系统语言表取的），读文件的这条路拿不到，
/// 所以按英文别名补一张表。只收日常真会用语音说出来的那几个。
const ZH_SYSTEM_NAMES: &[(&str, &[&str])] = &[
    ("notes", &["备忘录"]),
    ("reminders", &["提醒事项"]),
    ("stickynotes", &["便笺"]),
    ("calendar", &["日历"]),
    ("photos", &["照片"]),
    ("maps", &["地图"]),
    ("mail", &["邮件"]),
    ("messages", &["信息"]),
    ("preview", &["预览"]),
    ("textedit", &["文本编辑"]),
    ("terminal", &["终端"]),
    ("calculator", &["计算器"]),
    ("settings", &["系统设置", "系统偏好设置"]),
    ("activitymonitor", &["活动监视器"]),
    ("diskutility", &["磁盘工具"]),
    ("finder", &["访达"]),
    ("shortcuts", &["快捷指令"]),
    ("music", &["音乐"]),
    ("podcasts", &["播客"]),
    ("tv", &["电视"]),
    ("weather", &["天气"]),
    ("home", &["家庭"]),
    ("freeform", &["无边记"]),
];

/// 按已有的英文别名补中文名（原地去重）。
pub fn add_zh_system_names(aliases: &mut Vec<String>) {
    for (english, chinese) in ZH_SYSTEM_NAMES {
        if !aliases.iter().any(|alias| alias == english) {
            continue;
        }
        for name in *chinese {
            let name = normalize_name(name);
            if !aliases.contains(&name) {
                aliases.push(name);
            }
        }
    }
}

/// 回显给用户看的名字：有非 ASCII（中文）别名时优先用它。
///
/// 语音场景里这条尤其重要：用户说的是「备忘录」，回应里出现 `Notes` 会让人以为
/// 打开的不是同一个东西。
pub fn display_name(aliases: &[String], fallback: &str) -> String {
    aliases
        .iter()
        .find(|alias| !alias.is_ascii())
        .cloned()
        .unwrap_or_else(|| fallback.to_string())
}

/// 扫一遍安装目录（结果有缓存，见平台实现）。
pub fn installed() -> Vec<AppInfo> {
    platform::installed()
}

/// 按名字找已安装的 App。
pub fn find(query: &str) -> Option<AppInfo> {
    // 索引是每次现取的一张表，不能把借用它的引用带出去。
    let apps = installed();
    pick(&apps, query).cloned()
}

/// 用路径启动；路径必须来自 [`installed`]。
pub fn open(app: &AppInfo) -> Result<(), String> {
    platform::open(app)
}

/// 退出某个正在跑的 App（按 bundle id 现查，不等它退完）。
pub fn quit(query: &str) -> Result<Located, String> {
    platform::quit(query)
}

#[cfg(test)]
mod tests {
    use super::{add_zh_system_names, display_name, normalize_name, pick, AppInfo};

    fn app(name: &str, aliases: &[&str]) -> AppInfo {
        AppInfo {
            name: name.into(),
            path: format!("/Applications/{name}.app"),
            bundle_id: Some(format!("com.example.{}", name.len())),
            aliases: aliases.iter().map(|s| normalize_name(s)).collect(),
        }
    }

    #[test]
    fn normalizes_spaces_and_app_suffix() {
        assert_eq!(normalize_name(" iPhone Mirroring "), "iphonemirroring");
        assert_eq!(normalize_name("抖音.app"), "抖音");
        assert_eq!(normalize_name("Safari.APP"), "safari");
    }

    #[test]
    fn prefers_exact_then_prefix_then_contains() {
        let apps = vec![
            app("备忘录提醒事项", &["备忘录提醒事项", "reminders"]),
            app("备忘录", &["备忘录", "notes"]),
            app("抖音", &["抖音", "douyin"]),
        ];
        assert_eq!(pick(&apps, "备忘录").unwrap().name, "备忘录");
        assert_eq!(pick(&apps, "douyin").unwrap().name, "抖音");
        assert_eq!(pick(&apps, " 抖音 .app").unwrap().name, "抖音");
        assert_eq!(pick(&apps, "提醒").unwrap().name, "备忘录提醒事项");
        assert!(pick(&apps, "微信").is_none());
        assert!(pick(&apps, "").is_none());
    }

    #[test]
    fn shorter_name_wins_ties() {
        let apps = vec![
            app("抖音直播伴侣", &["抖音直播伴侣"]),
            app("抖音", &["抖音"]),
        ];
        assert_eq!(pick(&apps, "抖音").unwrap().name, "抖音");
    }
    #[test]
    fn adds_chinese_names_for_system_apps() {
        let mut aliases = vec!["notes".to_string()];
        add_zh_system_names(&mut aliases);
        assert_eq!(aliases, vec!["notes".to_string(), "备忘录".to_string()]);
        // 已经有中文名的不重复补。
        let mut with_zh = vec!["微信".to_string(), "wechat".to_string()];
        add_zh_system_names(&mut with_zh);
        assert_eq!(with_zh.len(), 2);
    }

    #[test]
    fn prefers_the_chinese_name_when_naming_the_result() {
        let aliases = vec!["notes".to_string(), "备忘录".to_string()];
        assert_eq!(display_name(&aliases, "Notes"), "备忘录");
        let english_only = vec!["chrome".to_string()];
        assert_eq!(display_name(&english_only, "Chrome"), "Chrome");
    }
}
