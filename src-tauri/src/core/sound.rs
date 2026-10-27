//! 系统提示音播放：复制入库 / 粘贴成功时按 `sound_enabled` 触发。

/// 播放系统提示音。
///
/// macOS 用 AudioToolbox 的 C 函数 `AudioServicesPlaySystemSound(1104)`
/// 播放系统音效 **Tink**（清脆短促的「叮」，iCopy 等剪贴板工具同款选择，
/// 由系统「声音」偏好设置统一控制音量）。
///
/// 为什么不用其它方案（都有坑，逐一试过）：
/// - `msg_send![class!(NSSound), beep]`：ObjC 消息在非主线程抛 foreign
///   exception 直接 abort 整个进程（监控线程即因此死过一次）；
/// - `NSBeep()`：虽线程安全，但播的是刺耳的警报声，不符合剪贴板工具的听感；
/// - `AudioServicesPlaySystemSound` 是纯 C API，任意线程可安全调用。
///
/// 其余平台当前为 no-op。
pub fn play_system_sound() {
    #[cfg(target_os = "macos")]
    play_macos_sound();
    #[cfg(not(target_os = "macos"))]
    {
        // no-op
    }
}

#[cfg(target_os = "macos")]
fn play_macos_sound() {
    /// 系统音效 ID：1104 = Tink（macOS 固定映射，/System/Library/Sounds/Tink.aiff）。
    const SOUND_ID_TINK: u32 = 1104;

    #[link(name = "AudioToolbox", kind = "framework")]
    unsafe extern "C" {
        fn AudioServicesPlaySystemSound(inSystemSoundID: u32);
    }

    // 该函数为异步派发（内部走系统音效服务），文档保证任意线程可调用。
    unsafe {
        AudioServicesPlaySystemSound(SOUND_ID_TINK);
    }
}
