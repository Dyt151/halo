//! 无线麦 SayAll 的探测与入口。
//!
//! SayAll（bundle id [`APP_BUNDLE_ID`]，窗口标题「无线麦」）把 iPhone 变成 Mac 的
//! 无线麦克风 + 遥控器。它自己常驻菜单栏，而 Halo 已经有菜单栏面板了，两只图标在
//! 右上角抢同一个位置。这一页做的是它缺的那半件事：
//!
//! - 探测装没装、版本、在不在跑；
//! - 一个按钮直接进到它的窗口（Halo 不重做它的界面）；
//! - 它支持时读写自己的「菜单栏显示图标」开关，关掉后由 Halo 面板接手这个入口；
//! - 它不支持时（1.9.21 及更早）改由 Halo 退出它——图标是进程画的，进程走了图标就没了。
//!
//! 一行 SayAll 的代码都没有搬过来：它是 GPLv3，Halo 是 MIT，两者不能混。这里的
//! 探测、拉起、跨进程读写偏好都是 Halo 自己写的。
//!
//! 两个必须记住的前提：SayAll 只在**启动那一刻**按 `showStatusBarIcon` 决定画不画
//! 菜单栏图标（`applyStatusBarIconVisibility(source: "launch")`），运行中改偏好域不生效，
//! 所以 [`set_status_bar_visible`] 在它在跑时会退出再拉起；而这个键本身要
//! [`STATUS_ICON_MIN_VERSION`] 起才有，见那里的说明。

use serde::Serialize;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as platform;

#[cfg(not(target_os = "macos"))]
mod other;
#[cfg(not(target_os = "macos"))]
use other as platform;

/// SayAll 的 bundle id，同时也是它存设置用的偏好域名。
pub const APP_BUNDLE_ID: &str = "com.hd838a.RemoteMic";

/// 它自己那个「菜单栏显示图标」开关。
///
/// 键名必须与 SayAll 源码里 `UserDefaults` 的键逐字一致（含大小写）：拼错的结果不是
/// 报错而是静默无效。它读不到这条记录时按「显示」处理，所以缺省值不能写成 false。
pub const STATUS_ICON_KEY: &str = "showStatusBarIcon";

/// 从哪个 SayAll 版本起才认 [`STATUS_ICON_KEY`]。
///
/// 本机装的 1.9.21 里搜不到这个键，而且它的菜单栏图标是无条件建的（二进制里只有
/// `systemStatusBar` + `statusItemWithLength:`，没有任何可见性判断）——写偏好域给
/// 它是纯空转。官方 stable 频道（`download.sayall.app`）当时最新也只到 1.9.21，
/// 所以这个开关必须有版本门，否则界面上就是一个骗人的按钮。
pub const STATUS_ICON_MIN_VERSION: &str = "2.0.0";

/// 界面上这一项需要的全部读数。
#[derive(Debug, Default, Clone, Serialize)]
pub struct State {
    /// 在这台 Mac 上找到了 SayAll.app。没装时整块入口不显示，不留假按钮。
    pub installed: bool,
    /// `CFBundleShortVersionString`，读不到为 null。
    pub version: Option<String>,
    pub running: bool,
    /// SayAll 自己记录的「菜单栏要不要画图标」（缺省 = 画）。
    ///
    /// [`Self::status_bar_icon_supported`] 为 false 时这条读数没有意义——低版本根本不读它。
    pub status_bar_visible: bool,
    /// 当前这个 SayAll 版本认不认 [`STATUS_ICON_KEY`]。
    pub status_bar_icon_supported: bool,
}

/// 按点分数字段比较，够用来判 `1.9.21 < 2.0.0`。缺的段和非法段都按 0 算。
fn version_at_least(version: &str, min: &str) -> bool {
    let actual = version_parts(version);
    let required = version_parts(min);
    for i in 0..actual.len().max(required.len()) {
        let a = *actual.get(i).unwrap_or(&0);
        let b = *required.get(i).unwrap_or(&0);
        if a != b {
            return a > b;
        }
    }
    true
}

fn version_parts(version: &str) -> Vec<u64> {
    version
        .split('.')
        .map(|segment| {
            let digits: String = segment.chars().take_while(char::is_ascii_digit).collect();
            digits.parse().unwrap_or(0)
        })
        .collect()
}

/// 这个版本能不能自己隐藏菜单栏图标。
pub fn supports_status_icon_toggle(version: &Option<String>) -> bool {
    version
        .as_deref()
        .is_some_and(|v| version_at_least(v, STATUS_ICON_MIN_VERSION))
}

pub fn state() -> State {
    platform::state()
}

/// 进到 SayAll：没跑就启动，在跑就把它自己的窗口带到前台。
pub fn open() -> Result<(), String> {
    platform::open()
}

/// 退出 SayAll，返回退出后的读数。
///
/// 这是低版本 SayAll 唯一真的能让右上角图标消失的动作：图标活在进程里，进程走了图标就没了。
pub fn quit() -> Result<State, String> {
    platform::quit()
}

/// 改「菜单栏显示图标」，返回改动后的读数。
pub fn set_status_bar_visible(visible: bool) -> Result<State, String> {
    platform::set_status_bar_visible(visible)
}

#[cfg(test)]
mod tests {
    use super::{supports_status_icon_toggle, version_at_least};

    #[test]
    fn compares_dot_versions() {
        assert!(version_at_least("2.0.0", "2.0.0"));
        assert!(version_at_least("2.1", "2.0.0"));
        assert!(!version_at_least("1.9.21", "2.0.0"));
        assert!(!version_at_least("1.10.0", "2.0.0"));
        // 版本尾巴上的预发布标记不该把主版本号带跑偏。
        assert!(version_at_least("2.0.0-beta1", "2.0.0"));
        // 读不到版本一律按「不支持」处理，宁可少给一个开关。
        assert!(!version_at_least("", "2.0.0"));
    }

    #[test]
    fn gates_the_toggle_on_version() {
        assert!(!supports_status_icon_toggle(&None));
        assert!(!supports_status_icon_toggle(&Some("1.9.21".into())));
        assert!(supports_status_icon_toggle(&Some("2.0.0".into())));
    }
}
