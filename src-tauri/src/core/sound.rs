//! 复制 / 粘贴提示音与音量调节反馈音。
//!
//! 音效随应用打包，不用系统自带音效——Tink 之类在系统各处都在响，
//! 听不出是剪贴板动作；且 macOS 26 起固定的音效 ID 已不发声，必须加载文件。

#[cfg(target_os = "macos")]
const COPY_SOUND: &[u8] = include_bytes!("../../assets/copy.wav");
/// 音量每变化一步的短促反馈音（40ms）。
#[cfg(target_os = "macos")]
const VOLUME_TICK: &[u8] = include_bytes!("../../assets/volume.wav");

/// 播放复制 / 粘贴提示音（macOS 有效，其余平台 no-op）。
pub fn play_system_sound() {
    #[cfg(target_os = "macos")]
    {
        static SOUND_ID: std::sync::OnceLock<Option<u32>> = std::sync::OnceLock::new();
        let Some(id) = *SOUND_ID.get_or_init(|| load_sound(COPY_SOUND, "sound-copy.wav")) else {
            return;
        };
        play_id(id);
    }
}

/// 播放音量步进反馈音（macOS 有效，其余平台 no-op）。
pub fn play_volume_tick() {
    #[cfg(target_os = "macos")]
    {
        static SOUND_ID: std::sync::OnceLock<Option<u32>> = std::sync::OnceLock::new();
        let Some(id) = *SOUND_ID.get_or_init(|| load_sound(VOLUME_TICK, "sound-volume.wav")) else {
            return;
        };
        play_id(id);
    }
}

#[cfg(target_os = "macos")]
fn play_id(id: u32) {
    #[link(name = "AudioToolbox", kind = "framework")]
    unsafe extern "C" {
        fn AudioServicesPlaySystemSound(in_system_sound_id: u32);
    }

    // 纯 C API，内部走系统音效服务，任意线程可安全调用。
    unsafe {
        AudioServicesPlaySystemSound(id);
    }
}

/// 把内置音效落到数据目录并创建 system sound ID。
///
/// 内置 wav 每进程重写一次，保证升级应用后不会播放旧音效；失败时退回系统 Tink。
#[cfg(target_os = "macos")]
fn load_sound(bytes: &[u8], file_name: &str) -> Option<u32> {
    let dir = crate::storage::data_dir();
    let path = dir.join(file_name);
    if std::fs::create_dir_all(&dir).is_ok() && std::fs::write(&path, bytes).is_ok() {
        if let Some(id) = create_sound_id(&path) {
            return Some(id);
        }
    }
    create_sound_id(std::path::Path::new("/System/Library/Sounds/Tink.aiff"))
}

#[cfg(target_os = "macos")]
fn create_sound_id(path: &std::path::Path) -> Option<u32> {
    use core_foundation::base::TCFType;

    #[link(name = "AudioToolbox", kind = "framework")]
    unsafe extern "C" {
        fn AudioServicesCreateSystemSoundID(
            in_file_url: *const std::ffi::c_void,
            out_system_sound_id: *mut u32,
        ) -> i32;
    }

    let url = core_foundation::url::CFURL::from_path(path, false)?;
    let mut id: u32 = 0;
    let status =
        unsafe { AudioServicesCreateSystemSoundID(url.as_concrete_TypeRef() as *const _, &mut id) };
    (status == 0 && id != 0).then_some(id)
}
