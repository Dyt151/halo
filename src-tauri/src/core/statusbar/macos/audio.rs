//! CoreAudio HAL：输出 / 输入设备的音量、静音与默认设备切换。
//!
//! 只用了公开的 `AudioObject*` 接口（CoreAudio.framework），不碰私有框架：
//! 系统升级换掉私有符号的风险与这里无关。

use std::ffi::c_void;

use super::cf;
use crate::core::statusbar::{AudioDevice, AudioDeviceState, AudioScope};

type AudioObjectID = u32;
type OSStatus = i32;

#[repr(C)]
#[derive(Clone, Copy)]
struct PropertyAddress {
    selector: u32,
    scope: u32,
    element: u32,
}

const fn fourcc(s: &[u8; 4]) -> u32 {
    ((s[0] as u32) << 24) | ((s[1] as u32) << 16) | ((s[2] as u32) << 8) | s[3] as u32
}

/// kAudioObjectSystemObject
const SYSTEM_OBJECT: AudioObjectID = 1;
const SCOPE_GLOBAL: u32 = fourcc(b"glob");
const SCOPE_OUTPUT: u32 = fourcc(b"outp");
const SCOPE_INPUT: u32 = fourcc(b"inpt");
const ELEMENT_MAIN: u32 = 0;

const PROP_DEVICES: u32 = fourcc(b"dev#");
const PROP_DEFAULT_OUTPUT: u32 = fourcc(b"dOut");
const PROP_DEFAULT_INPUT: u32 = fourcc(b"dIn ");
/// kAudioObjectPropertyName
const PROP_NAME: u32 = fourcc(b"lnam");
/// kAudioDevicePropertyStreamConfiguration
const PROP_STREAM_CONFIG: u32 = fourcc(b"slay");
const PROP_VOLUME: u32 = fourcc(b"pvol");
/// kAudioDevicePropertyVolumeScalar：输入设备多数只暴露这个，输出作为后备。
const PROP_VOLUME_SCALAR: u32 = fourcc(b"volm");
const PROP_MUTE: u32 = fourcc(b"mute");

/// 主元素读不到时逐声道尝试的上限（立体声通常是 1/2）。
const MAX_CHANNELS: u32 = 8;

#[link(name = "CoreAudio", kind = "framework")]
extern "C" {
    fn AudioObjectGetPropertyData(
        obj: AudioObjectID,
        addr: *const PropertyAddress,
        qualifier_size: u32,
        qualifier: *const c_void,
        data_size: *mut u32,
        out: *mut c_void,
    ) -> OSStatus;
    fn AudioObjectGetPropertyDataSize(
        obj: AudioObjectID,
        addr: *const PropertyAddress,
        qualifier_size: u32,
        qualifier: *const c_void,
        out_size: *mut u32,
    ) -> OSStatus;
    fn AudioObjectSetPropertyData(
        obj: AudioObjectID,
        addr: *const PropertyAddress,
        qualifier_size: u32,
        qualifier: *const c_void,
        data_size: u32,
        data: *const c_void,
    ) -> OSStatus;
    fn AudioObjectHasProperty(obj: AudioObjectID, addr: *const PropertyAddress) -> u8;
}

fn addr(selector: u32, scope: u32, element: u32) -> PropertyAddress {
    PropertyAddress {
        selector,
        scope,
        element,
    }
}

fn scope_of(scope: AudioScope) -> u32 {
    match scope {
        AudioScope::Output => SCOPE_OUTPUT,
        AudioScope::Input => SCOPE_INPUT,
    }
}

fn default_selector(scope: AudioScope) -> u32 {
    match scope {
        AudioScope::Output => PROP_DEFAULT_OUTPUT,
        AudioScope::Input => PROP_DEFAULT_INPUT,
    }
}

fn system_uint32(selector: u32) -> Option<u32> {
    let a = addr(selector, SCOPE_GLOBAL, ELEMENT_MAIN);
    let mut value: u32 = 0;
    let mut size = std::mem::size_of::<u32>() as u32;
    let status = unsafe {
        AudioObjectGetPropertyData(
            SYSTEM_OBJECT,
            &a,
            0,
            std::ptr::null(),
            &mut size,
            (&mut value as *mut u32).cast(),
        )
    };
    (status == 0).then_some(value)
}

/// 有权限访问的音视频设备 id 列表。
fn device_ids() -> Vec<u32> {
    let a = addr(PROP_DEVICES, SCOPE_GLOBAL, ELEMENT_MAIN);
    let mut size: u32 = 0;
    let status = unsafe {
        AudioObjectGetPropertyDataSize(SYSTEM_OBJECT, &a, 0, std::ptr::null(), &mut size)
    };
    if status != 0 || size == 0 {
        return Vec::new();
    }
    let mut ids = vec![0u32; size as usize / std::mem::size_of::<u32>()];
    let status = unsafe {
        AudioObjectGetPropertyData(
            SYSTEM_OBJECT,
            &a,
            0,
            std::ptr::null(),
            &mut size,
            ids.as_mut_ptr().cast(),
        )
    };
    if status != 0 {
        return Vec::new();
    }
    ids.retain(|id| *id != 0);
    ids
}

/// 设备在该方向上是否真的有流（扬声器不会出现在输入设备列表里）。
///
/// `AudioBufferList` 的首字段是 `mNumberBuffers`；没有流时 CoreAudio 仍会
/// 返回一块非空缓冲（实测 8 字节：`mNumberBuffers = 0` + 对齐填充），所以
/// 不能拿 `size > 4` 当判据——那会把纯输出设备的输入方向也当成有流。
fn has_streams(id: AudioObjectID, scope: u32) -> bool {
    let a = addr(PROP_STREAM_CONFIG, scope, ELEMENT_MAIN);
    let mut size: u32 = 0;
    let status = unsafe { AudioObjectGetPropertyDataSize(id, &a, 0, std::ptr::null(), &mut size) };
    if status != 0 || size < 4 || size > 4096 {
        return false;
    }
    let mut raw = vec![0u8; size as usize];
    let status = unsafe {
        AudioObjectGetPropertyData(
            id,
            &a,
            0,
            std::ptr::null(),
            &mut size,
            raw.as_mut_ptr().cast(),
        )
    };
    if status != 0 || size < 4 {
        return false;
    }
    u32::from_ne_bytes([raw[0], raw[1], raw[2], raw[3]]) > 0
}

fn device_name(id: AudioObjectID) -> String {
    let a = addr(PROP_NAME, SCOPE_GLOBAL, ELEMENT_MAIN);
    let mut raw: cf::CFTypeRef = std::ptr::null();
    let mut size = std::mem::size_of::<cf::CFTypeRef>() as u32;
    let status = unsafe {
        AudioObjectGetPropertyData(
            id,
            &a,
            0,
            std::ptr::null(),
            &mut size,
            (&mut raw as *mut cf::CFTypeRef).cast(),
        )
    };
    if status != 0 || raw.is_null() {
        return String::new();
    }
    let name = unsafe { cf::string(raw) };
    unsafe { cf::release(raw) };
    name.map(|n| n.trim().to_string()).unwrap_or_default()
}

fn scalar(id: AudioObjectID, selector: u32, scope: u32, element: u32) -> Option<f32> {
    let a = addr(selector, scope, element);
    if unsafe { AudioObjectHasProperty(id, &a) } == 0 {
        return None;
    }
    let mut value: f32 = 0.0;
    let mut size = std::mem::size_of::<f32>() as u32;
    let status = unsafe {
        AudioObjectGetPropertyData(
            id,
            &a,
            0,
            std::ptr::null(),
            &mut size,
            (&mut value as *mut f32).cast(),
        )
    };
    (status == 0 && size == 4).then_some(value)
}

fn uint32_prop(id: AudioObjectID, selector: u32, scope: u32, element: u32) -> Option<u32> {
    let a = addr(selector, scope, element);
    if unsafe { AudioObjectHasProperty(id, &a) } == 0 {
        return None;
    }
    let mut value: u32 = 0;
    let mut size = std::mem::size_of::<u32>() as u32;
    let status = unsafe {
        AudioObjectGetPropertyData(
            id,
            &a,
            0,
            std::ptr::null(),
            &mut size,
            (&mut value as *mut u32).cast(),
        )
    };
    (status == 0).then_some(value)
}

fn read_volume(id: AudioObjectID, scope: u32) -> Option<f32> {
    // 先试虚拟主音量（pvol），再试设备音量标量（volm，麦克风基本只认这个）。
    for selector in [PROP_VOLUME, PROP_VOLUME_SCALAR] {
        if let Some(v) = scalar(id, selector, scope, ELEMENT_MAIN) {
            return Some(v);
        }
    }
    for ch in 1..=MAX_CHANNELS {
        for selector in [PROP_VOLUME, PROP_VOLUME_SCALAR] {
            if let Some(v) = scalar(id, selector, scope, ch) {
                return Some(v);
            }
        }
    }
    None
}

fn read_mute(id: AudioObjectID, scope: u32) -> Option<bool> {
    if let Some(v) = uint32_prop(id, PROP_MUTE, scope, ELEMENT_MAIN) {
        return Some(v != 0);
    }
    (1..=MAX_CHANNELS).find_map(|ch| uint32_prop(id, PROP_MUTE, scope, ch).map(|v| v != 0))
}

fn write_scalar(id: AudioObjectID, selector: u32, scope: u32, element: u32, value: f32) -> bool {
    let a = addr(selector, scope, element);
    if unsafe { AudioObjectHasProperty(id, &a) } == 0 {
        return false;
    }
    let status = unsafe {
        AudioObjectSetPropertyData(
            id,
            &a,
            0,
            std::ptr::null(),
            std::mem::size_of::<f32>() as u32,
            (&value as *const f32).cast(),
        )
    };
    status == 0
}

fn write_volume(id: AudioObjectID, scope: u32, value: f32) -> bool {
    for selector in [PROP_VOLUME, PROP_VOLUME_SCALAR] {
        if write_scalar(id, selector, scope, ELEMENT_MAIN, value) {
            return true;
        }
    }
    let mut touched = false;
    for ch in 1..=MAX_CHANNELS {
        for selector in [PROP_VOLUME, PROP_VOLUME_SCALAR] {
            if write_scalar(id, selector, scope, ch, value) {
                touched = true;
            }
        }
    }
    touched
}

fn write_mute(id: AudioObjectID, scope: u32, muted: bool) -> bool {
    let value: u32 = muted as u32;
    let main = addr(PROP_MUTE, scope, ELEMENT_MAIN);
    if unsafe { AudioObjectHasProperty(id, &main) } != 0 {
        let status = unsafe {
            AudioObjectSetPropertyData(
                id,
                &main,
                0,
                std::ptr::null(),
                std::mem::size_of::<u32>() as u32,
                (&value as *const u32).cast(),
            )
        };
        if status == 0 {
            return true;
        }
    }
    let mut touched = false;
    for ch in 1..=MAX_CHANNELS {
        let a = addr(PROP_MUTE, scope, ch);
        if unsafe { AudioObjectHasProperty(id, &a) } == 0 {
            continue;
        }
        let status = unsafe {
            AudioObjectSetPropertyData(
                id,
                &a,
                0,
                std::ptr::null(),
                std::mem::size_of::<u32>() as u32,
                (&value as *const u32).cast(),
            )
        };
        if status == 0 {
            touched = true;
        }
    }
    touched
}

fn read_state(scope: AudioScope) -> AudioDeviceState {
    let scope_code = scope_of(scope);
    let default_id = system_uint32(default_selector(scope)).unwrap_or(0);
    let devices: Vec<AudioDevice> = device_ids()
        .into_iter()
        .filter(|id| has_streams(*id, scope_code))
        .map(|id| {
            let mut name = device_name(id);
            if name.is_empty() {
                name = "未命名设备".to_string();
            }
            AudioDevice {
                id,
                name,
                is_default: id == default_id,
            }
        })
        .collect();

    // 默认设备的名称可能不在列表里（设备被热插拔）：统一按同一个函数兜底。
    let device_name = devices
        .iter()
        .find(|d| d.id == default_id)
        .map(|d| d.name.clone())
        .unwrap_or_else(|| {
            if default_id == 0 {
                "无可用设备".to_string()
            } else {
                let name = self::device_name(default_id);
                if name.is_empty() {
                    "未命名设备".to_string()
                } else {
                    name
                }
            }
        });

    let volume = read_volume(default_id, scope_code);
    let muted = read_mute(default_id, scope_code);

    AudioDeviceState {
        available: default_id != 0,
        device_id: default_id,
        device_name,
        volume: volume.unwrap_or(0.0).clamp(0.0, 1.0) as f64,
        volume_available: volume.is_some(),
        muted: muted.unwrap_or(false),
        mute_available: muted.is_some(),
        devices,
    }
}

pub fn output_state() -> AudioDeviceState {
    read_state(AudioScope::Output)
}

/// 轻量读默认输出音量：只取一个标量，供音量反馈轮询（不枚举设备）。
pub fn output_volume() -> Option<f32> {
    let id = system_uint32(PROP_DEFAULT_OUTPUT)?;
    read_volume(id, SCOPE_OUTPUT)
}

pub fn input_state() -> AudioDeviceState {
    read_state(AudioScope::Input)
}

pub fn set_volume(scope: AudioScope, volume: f64) -> Result<(), String> {
    let id = system_uint32(default_selector(scope)).ok_or("没有可用的默认音频设备")?;
    let value = (volume.clamp(0.0, 1.0)) as f32;
    if write_volume(id, scope_of(scope), value) {
        Ok(())
    } else {
        Err("该设备的音量不可调".to_string())
    }
}

pub fn set_muted(scope: AudioScope, muted: bool) -> Result<(), String> {
    let id = system_uint32(default_selector(scope)).ok_or("没有可用的默认音频设备")?;
    if write_mute(id, scope_of(scope), muted) {
        Ok(())
    } else {
        Err("该设备不支持静音切换".to_string())
    }
}

pub fn set_default(scope: AudioScope, device_id: u32) -> Result<(), String> {
    let a = addr(default_selector(scope), SCOPE_GLOBAL, ELEMENT_MAIN);
    let status = unsafe {
        AudioObjectSetPropertyData(
            SYSTEM_OBJECT,
            &a,
            0,
            std::ptr::null(),
            std::mem::size_of::<u32>() as u32,
            (&device_id as *const u32).cast(),
        )
    };
    if status == 0 {
        Ok(())
    } else {
        Err("切换默认设备失败".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fourcc_matches_coreaudio_constants() {
        // 这些值直接抄自 AudioHardwareBase.h，写错会让所有属性查询静默失败。
        assert_eq!(fourcc(b"dOut"), 0x644F_7574);
        assert_eq!(fourcc(b"pvol"), 0x7076_6F6C);
        assert_eq!(fourcc(b"glob"), 0x676C_6F62);
    }
}
