//! 渲染入口：把菜单栏状态交给 CoreGraphics 光栅化。

use super::*;

/// 渲染成 52×52 的 RGBA（模板模式下只有 alpha 承载信息）。
#[cfg(target_os = "macos")]
pub fn render(spec: &GlyphSpec, palette: &Palette) -> Vec<u8> {
    super::draw::paint(spec, palette, None, None)
}

/// 带充电扫光帧的渲染；`phase = None` 时与 [`render`] 完全一致。
#[cfg(target_os = "macos")]
pub fn render_frame(spec: &GlyphSpec, palette: &Palette, phase: Option<ChargingPhase>) -> Vec<u8> {
    super::draw::paint(spec, palette, phase, None)
}

/// 输入法切换瞬间的整格字牌：`ime` 有值时整枚图标换成字牌，不出电池环与音量。
#[cfg(target_os = "macos")]
pub fn render_with_ime(
    spec: &GlyphSpec,
    palette: &Palette,
    phase: Option<ChargingPhase>,
    ime: Option<ImeFlash>,
) -> Vec<u8> {
    super::draw::paint(spec, palette, phase, ime)
}

/// 非 macOS：保持可编译，交给一张全空的图（本项目只在 macOS 上真跑）。
#[cfg(not(target_os = "macos"))]
pub fn render(spec: &GlyphSpec, _palette: &Palette) -> Vec<u8> {
    let _ = spec;
    vec![0u8; IMAGE_SIZE * IMAGE_SIZE * 4]
}

#[cfg(not(target_os = "macos"))]
pub fn render_frame(
    spec: &GlyphSpec,
    _palette: &Palette,
    _phase: Option<ChargingPhase>,
) -> Vec<u8> {
    let _ = spec;
    vec![0u8; IMAGE_SIZE * IMAGE_SIZE * 4]
}

#[cfg(not(target_os = "macos"))]
pub fn render_with_ime(
    spec: &GlyphSpec,
    _palette: &Palette,
    _phase: Option<ChargingPhase>,
    _ime: Option<ImeFlash>,
) -> Vec<u8> {
    let _ = spec;
    vec![0u8; IMAGE_SIZE * IMAGE_SIZE * 4]
}

