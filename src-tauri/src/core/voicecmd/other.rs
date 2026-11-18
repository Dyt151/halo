//! 非 macOS 平台：没有「用默认浏览器打开」这条链路。

pub fn open_url(_url: &str) -> Result<(), String> {
    Err("当前系统不支持用语音打开网址".to_string())
}
