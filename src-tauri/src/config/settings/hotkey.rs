//! 全局快捷键配置。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HotkeyConfig {
    /// 唤起/收起剪贴板面板。
    #[serde(default = "default_toggle_hotkey")]
    pub toggle: String,
    /// 粘贴堆栈顶。
    #[serde(default = "default_stack_paste_hotkey")]
    pub stack_paste: String,
    /// 弹出语音指令条。
    #[serde(default = "default_voice_cmd_hotkey")]
    pub voice_cmd: String,
}

pub(crate) fn default_toggle_hotkey() -> String {
    // 不用 Control+C：它会抢走终端的 SIGINT（评审 R03），且 macOS 不把 Fn 当
    // 普通修饰键（fn+z 无法注册），⌃⌥C 冲突概率低、单手可按。
    "Control+Option+C".to_string()
}

fn default_stack_paste_hotkey() -> String {
    "Control+Shift+V".to_string()
}

fn default_voice_cmd_hotkey() -> String {
    // K = command。和面板（⌃⌥C）同族、只差一个字母，遥控器录快捷键时好记也好按；
    // 不选 ⌃⌥K 之外的形态是因为 ⌥K 单独按会打出「⌘」符号，修饰键多一层更安全。
    "Control+Option+K".to_string()
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            toggle: default_toggle_hotkey(),
            stack_paste: default_stack_paste_hotkey(),
            voice_cmd: default_voice_cmd_hotkey(),
        }
    }
}

