//! 输入法（Carbon TIS）：列出可切换的键盘输入源、直接切换，并在切换时让菜单栏
//! 图标闪一下新输入源的字牌。
//!
//! 全是公开 API，不需要辅助功能权限，也不做按键模拟。

use std::ffi::c_void;
use std::sync::OnceLock;

use tauri::AppHandle;

use super::cf;
use crate::core::statusbar::{InputSource, InputSourceHint, InputSources};
use crate::lock_ext::LockExt;

#[link(name = "Carbon", kind = "framework")]
extern "C" {
    fn TISCreateInputSourceList(
        properties: cf::CFTypeRef,
        include_all_installed: u8,
    ) -> cf::CFTypeRef;
    fn TISCopyCurrentKeyboardInputSource() -> cf::CFTypeRef;
    fn TISGetInputSourceProperty(
        source: cf::CFTypeRef,
        property_key: cf::CFTypeRef,
    ) -> cf::CFTypeRef;
    fn TISSelectInputSource(source: cf::CFTypeRef) -> i32;

    static kTISPropertyInputSourceID: cf::CFTypeRef;
    static kTISPropertyLocalizedName: cf::CFTypeRef;
    static kTISPropertyInputSourceIsEnabled: cf::CFTypeRef;
    static kTISPropertyInputSourceIsSelectCapable: cf::CFTypeRef;
    static kTISPropertyInputSourceCategory: cf::CFTypeRef;
    static kTISPropertyInputSourceLanguages: cf::CFTypeRef;
    static kTISCategoryKeyboardInputSource: cf::CFTypeRef;
}

unsafe fn prop_string(source: cf::CFTypeRef, key: cf::CFTypeRef) -> Option<String> {
    let value = TISGetInputSourceProperty(source, key);
    cf::string(value)
}

unsafe fn prop_bool(source: cf::CFTypeRef, key: cf::CFTypeRef) -> Option<bool> {
    let value = TISGetInputSourceProperty(source, key);
    cf::bool_value(value)
}

/// 输入源自己的语言列表（`["zh-Hans"]` / `["ja"]`）；只借读，不持有。
unsafe fn prop_languages(source: cf::CFTypeRef) -> Vec<String> {
    let value = TISGetInputSourceProperty(source, kTISPropertyInputSourceLanguages);
    (0..cf::array_count(value))
        .filter_map(|i| cf::string(cf::array_at(value, i)))
        .collect()
}

/// 无图标输入源的字标记（正版 `InputSourceShortLabel.make` 的等价实现）。
///
/// 只用稳定 ID 和输入源自带的语言元数据推导，不依赖某个第三方输入法的显示名
/// ——那种名字会被用户改名。
fn short_label(id: &str, localized_name: &str, languages: &[String]) -> Option<String> {
    let normalized = id.to_lowercase();
    if let Some(layout) = apple_layout_label(&normalized) {
        return Some(layout);
    }
    if normalized.contains("pinyin") || localized_name.contains("拼音") {
        return Some("拼".to_string());
    }
    if normalized.contains("zhuyin")
        || localized_name.contains("注音")
        || localized_name.contains("注音輸入")
    {
        return Some("注".to_string());
    }
    if normalized.contains("japanese")
        || languages.iter().any(|l| l == "ja")
        || normalized.starts_with("com.apple.inputmethod.kotoeri")
    {
        return Some("あ".to_string());
    }
    if normalized.contains("korean") || languages.iter().any(|l| l == "ko") {
        return Some("한".to_string());
    }
    if normalized.contains("wubi") || localized_name.contains("五笔") {
        return Some("五".to_string());
    }
    if normalized.contains("cangjie") || localized_name.contains("仓颉") {
        return Some("倉".to_string());
    }
    localized_name
        .trim()
        .chars()
        .next()
        .filter(|c| !c.is_whitespace())
        .map(|c| c.to_string())
}

/// ASCII 键盘布局读作单个大写字母：系统菜单栏给 ABC 显示的就是 `A`。
fn apple_layout_label(normalized_id: &str) -> Option<String> {
    let suffix = normalized_id.strip_prefix("com.apple.keylayout.")?;
    if matches!(suffix, "abc" | "us" | "usextended") {
        return Some("A".to_string());
    }
    suffix
        .chars()
        .find(char::is_ascii_alphabetic)
        .map(|c| c.to_uppercase().collect())
}

/// 正版用法：带「键盘输入源」属性过滤 + `includeAllInstalled = false`。
///
/// 传 NULL + true 会返回系统里*所有装过的*布局（Ainu、Hiragana、ABC 都进来），
/// 而且其中一部分照样报 `IsEnabled = true`，过滤不掉——这就是列表里冒出一堆
/// 莫名其妙的输入法的根因。
unsafe fn keyboard_list() -> cf::CFTypeRef {
    let Some(filter) = cf::dict(&[(
        kTISPropertyInputSourceCategory,
        kTISCategoryKeyboardInputSource,
    )]) else {
        return std::ptr::null();
    };
    let list = TISCreateInputSourceList(filter, 0);
    cf::release(filter);
    list
}

/// 按 ID 精确取一个输入源（正版切换走的就是这条路径，不做全表扫描）。
unsafe fn list_for_id(id: &str) -> cf::CFTypeRef {
    let Some(key) = cf::cfstr(id) else {
        return std::ptr::null();
    };
    let Some(filter) = cf::dict(&[(kTISPropertyInputSourceID, key)]) else {
        cf::release(key);
        return std::ptr::null();
    };
    let list = TISCreateInputSourceList(filter, 0);
    cf::release(filter);
    cf::release(key);
    list
}

/// 可切换的键盘输入源（已启用 + 可选择，按 macOS 返回的顺序，不排序）。
///
/// 返回值全部转成 Rust 字符串后在函数内释放列表：TIS 属性是借出的，
/// 调用方拿到 `(id, name)` 就够了，不必把 CF 生命周期带出去。
unsafe fn keyboard_sources() -> Vec<(String, String, Option<String>)> {
    let list = keyboard_list();
    if list.is_null() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for i in 0..cf::array_count(list) {
        let source = cf::array_at(list, i);
        if source.is_null() {
            continue;
        }
        if !prop_bool(source, kTISPropertyInputSourceIsEnabled).unwrap_or(false)
            || !prop_bool(source, kTISPropertyInputSourceIsSelectCapable).unwrap_or(false)
        {
            continue;
        }
        let Some(id) = prop_string(source, kTISPropertyInputSourceID) else {
            continue;
        };
        let name = prop_string(source, kTISPropertyLocalizedName).unwrap_or_else(|| id.clone());
        let marker = short_label(&id, &name, &prop_languages(source));
        out.push((id, name, marker));
    }
    cf::release(list);
    out
}

/// 只能在主线程调用：TSM 会在别的线程上 `dispatch_assert_queue` 然后 abort。
pub fn state() -> InputSources {
    unsafe {
        let current = current_id();

        let sources = keyboard_sources()
            .into_iter()
            .map(|(id, name, marker)| InputSource {
                selected: current.as_deref() == Some(id.as_str()),
                marker,
                id,
                name,
            })
            .collect();

        InputSources { sources, current }
    }
}

pub fn select(id: &str) -> Result<(), String> {
    unsafe {
        // 正版切换用 ID 过滤字典直接命中，不遍历全表。
        let list = list_for_id(id);
        if list.is_null() {
            return Err("读不到输入法列表".to_string());
        }
        let target = cf::array_at(list, 0);
        let status = if target.is_null() {
            -1
        } else {
            TISSelectInputSource(target)
        };
        cf::release(list);
        if status == 0 {
            Ok(())
        } else {
            Err("切换输入法失败（可能已被系统禁用）".to_string())
        }
    }
}

/* ------------------------------------------------------------------ */
/* 切换通知（输入法 HUD 用）                                            */
/* ------------------------------------------------------------------ */

/// 系统切换输入源时往分布式通知中心发的通知名（TSM 的公开约定，第三方输入法同样走它）。
const SELECTED_INPUT_SOURCE_CHANGED: &str =
    "com.apple.Carbon.TISNotifySelectedKeyboardInputSourceChanged";

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFNotificationCenterGetDistributedCenter() -> cf::CFTypeRef;
    /// `observer` 只当身份标识用（这里传回调自身的地址），CoreFoundation 不解引用它。
    fn CFNotificationCenterAddObserver(
        center: cf::CFTypeRef,
        observer: *const c_void,
        callback: extern "C" fn(
            observer: *const c_void,
            center: cf::CFTypeRef,
            name: cf::CFTypeRef,
            object: *const c_void,
            user_info: cf::CFTypeRef,
        ),
        name: cf::CFTypeRef,
        object: cf::CFTypeRef,
        suspension_behavior: isize,
    );
}

static APP: OnceLock<AppHandle> = OnceLock::new();

/// 只有 ID 和上一次不同才算一次切换。
///
/// 一次真实切换系统可能连着发两条（选中态与当前源各一条），第二条读到的还是同一个
/// 输入源；而 App 焦点变化、输入源被重新评估也会发一条内容完全相同的通知（本机实测
/// 每分钟好几条）。按时间窗去重挡不住隔几秒的重复通知，只能按 ID 比较。
static LAST_ID: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// 当前键盘输入源的 ID：通知不带「变成了谁」，只能回读。
unsafe fn current_id() -> Option<String> {
    let source = TISCopyCurrentKeyboardInputSource();
    if source.is_null() {
        return None;
    }
    let id = prop_string(source, kTISPropertyInputSourceID);
    cf::release(source);
    id
}

/// 当前输入源与它在可切换列表里的位置（HUD 的字牌、名称与圆点都要这三样）。
unsafe fn current_hint() -> Option<InputSourceHint> {
    let source = TISCopyCurrentKeyboardInputSource();
    if source.is_null() {
        return None;
    }
    let id = prop_string(source, kTISPropertyInputSourceID);
    let name = prop_string(source, kTISPropertyLocalizedName);
    let marker = match (id.as_deref(), name.as_deref()) {
        (Some(id), Some(name)) => short_label(id, name, &prop_languages(source)),
        _ => None,
    };
    cf::release(source);

    let id = id?;
    let sources = keyboard_sources();
    Some(InputSourceHint {
        index: sources
            .iter()
            .position(|(candidate, _, _)| *candidate == id)
            .map(|position| position + 1)
            .unwrap_or(0),
        total: sources.len(),
        name: name.unwrap_or_else(|| id.clone()),
        marker,
        id,
    })
}

/// 分布式通知在主线程回调——正好是 TIS 唯一允许读数的线程，不用再切一次。
extern "C" fn on_selected_input_source_changed(
    _observer: *const c_void,
    _center: cf::CFTypeRef,
    _name: cf::CFTypeRef,
    _object: *const c_void,
    _user_info: cf::CFTypeRef,
) {
    {
        let Some(id) = (unsafe { current_id() }) else {
            return;
        };
        let mut last = LAST_ID.lock_ok();
        if last.as_deref() == Some(id.as_str()) {
            return;
        }
        *last = Some(id);
    }
    let Some(hint) = (unsafe { current_hint() }) else {
        return;
    };
    tracing::info!("输入法切换：{}（{}）", hint.name, hint.id);
    // 和 Apple 的输入菜单图标一样：整枚菜单栏图标短暂换成新输入源的字牌。
    crate::core::statusbar::flash_input_source(hint.marker.as_deref());
}

/// 订阅输入法切换（只在主线程调用一次；重复调用会被 `OnceLock` 挡掉）。
pub fn watch_changes(app: &AppHandle) {
    if APP.set(app.clone()).is_err() {
        return;
    }
    // 先把启动时的输入源记下来：否则第一条通知（往往只是焦点变化）就会被当成一次切换。
    *LAST_ID.lock_ok() = unsafe { current_id() };
    let Some(name) = cf::cfstr(SELECTED_INPUT_SOURCE_CHANGED) else {
        return;
    };
    unsafe {
        CFNotificationCenterAddObserver(
            CFNotificationCenterGetDistributedCenter(),
            on_selected_input_source_changed as *const c_void,
            on_selected_input_source_changed,
            name,
            std::ptr::null(),
            // kCFNotificationSuspensionBehaviorCoalesce：App 被挂起期间的多次切换合成一条。
            2,
        );
        cf::release(name);
    }
    tracing::info!("输入法切换：已订阅");
}

#[cfg(test)]
mod tests {
    use super::short_label;

    #[test]
    fn apple_layouts_read_as_a_single_capital() {
        for id in [
            "com.apple.keylayout.ABC",
            "com.apple.keylayout.US",
            "com.apple.keylayout.USExtended",
        ] {
            assert_eq!(short_label(id, "ABC", &[]).as_deref(), Some("A"));
        }
        assert_eq!(
            short_label("com.apple.keylayout.British", "British", &[]).as_deref(),
            Some("B")
        );
    }

    #[test]
    fn ime_markers_come_from_the_id_before_the_display_name() {
        // 正版的规则只认「ID 里有 pinyin」或「显示名里有拼音」：SCIM 的 ID 不含
        // pinyin，所以英文本地化下两条都不命中，标记退回显示名首字母；一旦读到的
        // 是中文显示名（bundle 带 zh-Hans.lproj），就命中「拼音」→ 拼。
        assert_eq!(
            short_label("com.apple.inputmethod.SCIM.ITABC", "Pinyin – Simplified", &[]).as_deref(),
            Some("P")
        );
        assert_eq!(
            short_label("com.apple.inputmethod.SCIM.ITABC", "简体拼音", &[]).as_deref(),
            Some("拼")
        );
        assert_eq!(
            short_label("com.sogou.inputmethod.sogou", "搜狗拼音", &[]).as_deref(),
            Some("拼")
        );
        assert_eq!(
            short_label("com.apple.inputmethod.Kotoeri.KanaTyping", "Japanese", &[]).as_deref(),
            Some("あ")
        );
        assert_eq!(
            short_label("com.apple.inputmethod.Korean", "Korean", &[]).as_deref(),
            Some("한")
        );
    }

    #[test]
    fn unknown_sources_fall_back_to_the_first_letter() {
        assert_eq!(
            short_label("com.example.ime", "  My IME", &[]).as_deref(),
            Some("M")
        );
        assert_eq!(short_label("com.example.ime", "   ", &[]), None);
    }
}
