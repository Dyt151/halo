//! CoreFoundation 最小 FFI：只覆盖状态栏读取用到的字典 / 数组 / 字符串访问。
//!
//! 不引入额外 crate：CoreAudio 与 IOKit 的属性查询全部返回 CF 对象，
//! 这里的函数只做「借出 → 转 Rust 值」和必要的引用计数释放。

use std::ffi::{c_void, CStr, CString};

pub type CFTypeRef = *const c_void;

/// kCFStringEncodingUTF8
pub const ENCODING_UTF8: u32 = 0x0800_0100;
/// kCFNumberSInt32Type
pub const NUMBER_SINT32: i32 = 3;
/// kCFNumberFloat64Type
pub const NUMBER_FLOAT64: i32 = 13;
/// kCFNumberSInt64Type
pub const NUMBER_SINT64: i32 = 4;

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    pub fn CFRelease(cf: CFTypeRef);
    pub fn CFGetTypeID(cf: CFTypeRef) -> u64;
    pub fn CFStringGetTypeID() -> u64;
    pub fn CFNumberGetTypeID() -> u64;
    pub fn CFBooleanGetTypeID() -> u64;
    pub fn CFArrayGetTypeID() -> u64;
    pub fn CFDictionaryGetTypeID() -> u64;

    pub fn CFStringCreateWithCString(
        alloc: CFTypeRef,
        c_str: *const i8,
        encoding: u32,
    ) -> CFTypeRef;
    pub fn CFStringGetLength(s: CFTypeRef) -> isize;
    pub fn CFStringGetMaximumSizeForEncoding(length: isize, encoding: u32) -> isize;
    pub fn CFStringGetCString(
        s: CFTypeRef,
        buffer: *mut i8,
        buffer_size: isize,
        encoding: u32,
    ) -> u8;

    pub fn CFNumberGetValue(num: CFTypeRef, the_type: i32, value_ptr: *mut c_void) -> u8;
    pub fn CFBooleanGetValue(b: CFTypeRef) -> u8;

    pub fn CFArrayGetCount(arr: CFTypeRef) -> isize;
    pub fn CFArrayGetValueAtIndex(arr: CFTypeRef, idx: isize) -> CFTypeRef;
    pub fn CFDictionaryGetValue(dict: CFTypeRef, key: CFTypeRef) -> CFTypeRef;

    /// 传 NULL 回调即等价于 `kCFTypeDictionary{Key,Value}CallBacks`（Apple 文档明确允许）。
    pub fn CFDictionaryCreate(
        alloc: CFTypeRef,
        keys: *const CFTypeRef,
        values: *const CFTypeRef,
        num_values: isize,
        key_call_backs: *const c_void,
        value_call_backs: *const c_void,
    ) -> CFTypeRef;

    /// 用户偏好读取（线程安全，不需要主线程）；调用方负责释放返回值。
    pub fn CFPreferencesCopyAppValue(key: CFTypeRef, domain: CFTypeRef) -> CFTypeRef;
}

/// 创建并持有一个 CFString（调用方负责 `release`）。
pub fn cfstr(value: &str) -> Option<CFTypeRef> {
    let c = CString::new(value).ok()?;
    let s = unsafe { CFStringCreateWithCString(std::ptr::null(), c.as_ptr(), ENCODING_UTF8) };
    (!s.is_null()).then_some(s)
}

/// 由若干「已有 CF 值」组成一个字典（调用方负责 `release`）。
///
/// 传入的 key/value 都是调用方自持有的引用，字典本身会再 retain 一次，
/// 所以调用方释放自己那份之后字典仍然有效。
pub unsafe fn dict(pairs: &[(CFTypeRef, CFTypeRef)]) -> Option<CFTypeRef> {
    if pairs.is_empty() || pairs.iter().any(|(k, v)| k.is_null() || v.is_null()) {
        return None;
    }
    let keys: Vec<CFTypeRef> = pairs.iter().map(|(k, _)| *k).collect();
    let values: Vec<CFTypeRef> = pairs.iter().map(|(_, v)| *v).collect();
    let d = CFDictionaryCreate(
        std::ptr::null(),
        keys.as_ptr(),
        values.as_ptr(),
        pairs.len() as isize,
        std::ptr::null(),
        std::ptr::null(),
    );
    (!d.is_null()).then_some(d)
}

pub unsafe fn release(cf: CFTypeRef) {
    if !cf.is_null() {
        CFRelease(cf);
    }
}

pub unsafe fn string(cf: CFTypeRef) -> Option<String> {
    if cf.is_null() || CFGetTypeID(cf) != CFStringGetTypeID() {
        return None;
    }
    let len = CFStringGetLength(cf);
    let max = CFStringGetMaximumSizeForEncoding(len, ENCODING_UTF8) + 1;
    if max <= 0 || max > 4096 {
        return None;
    }
    let mut buf = vec![0i8; max as usize];
    if CFStringGetCString(cf, buf.as_mut_ptr(), max, ENCODING_UTF8) == 0 {
        return None;
    }
    Some(CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned())
}

pub unsafe fn i32_value(cf: CFTypeRef) -> Option<i32> {
    if cf.is_null() || CFGetTypeID(cf) != CFNumberGetTypeID() {
        return None;
    }
    let mut out: i32 = 0;
    let ok = CFNumberGetValue(cf, NUMBER_SINT32, (&mut out as *mut i32).cast());
    (ok != 0).then_some(out)
}

pub unsafe fn f64_value(cf: CFTypeRef) -> Option<f64> {
    if cf.is_null() || CFGetTypeID(cf) != CFNumberGetTypeID() {
        return None;
    }
    let mut out: f64 = 0.0;
    let ok = CFNumberGetValue(cf, NUMBER_FLOAT64, (&mut out as *mut f64).cast());
    (ok != 0).then_some(out)
}

/// 有符号 64 位读数：`Amperage` 这类键用补码表示负值，走 f64 会丢掉符号。
pub unsafe fn i64_value(cf: CFTypeRef) -> Option<i64> {
    if cf.is_null() || CFGetTypeID(cf) != CFNumberGetTypeID() {
        return None;
    }
    let mut out: i64 = 0;
    let ok = CFNumberGetValue(cf, NUMBER_SINT64, (&mut out as *mut i64).cast());
    (ok != 0).then_some(out)
}

pub unsafe fn bool_value(cf: CFTypeRef) -> Option<bool> {
    if cf.is_null() || CFGetTypeID(cf) != CFBooleanGetTypeID() {
        return None;
    }
    Some(CFBooleanGetValue(cf) != 0)
}

pub unsafe fn array_count(arr: CFTypeRef) -> isize {
    if arr.is_null() || CFGetTypeID(arr) != CFArrayGetTypeID() {
        return 0;
    }
    CFArrayGetCount(arr)
}

pub unsafe fn array_at(arr: CFTypeRef, index: isize) -> CFTypeRef {
    CFArrayGetValueAtIndex(arr, index)
}

pub unsafe fn dict_get(dict: CFTypeRef, key: CFTypeRef) -> CFTypeRef {
    if dict.is_null() || CFGetTypeID(dict) != CFDictionaryGetTypeID() {
        return std::ptr::null();
    }
    CFDictionaryGetValue(dict, key)
}

/// 按字符串键取子字典（借出值，随父字典一起释放，调用方不要 release）。
pub unsafe fn sub_dict(dict: CFTypeRef, key: &str) -> Option<CFTypeRef> {
    let k = cfstr(key)?;
    let value = dict_get(dict, k);
    release(k);
    if value.is_null() || CFGetTypeID(value) != CFDictionaryGetTypeID() {
        return None;
    }
    Some(value)
}
