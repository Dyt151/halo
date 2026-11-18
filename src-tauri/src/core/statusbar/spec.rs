//! 图标规格与悬停提示的纯函数推导。

use super::*;

/// 由读数 + 用户开关推出图标规格（纯函数，两平台与单测共用）。
pub fn glyph_spec(state: &GlyphState, options: &glyph::GlyphOptions) -> glyph::GlyphSpec {
    // 「自动」是正版默认：有网络读数就画网络符号，否则退回蓝牙，再否则剪贴板。
    // 点名要某一种时，读不到也照画（用户想看到那个形状，而不是被静默换掉）。
    let center = match options.center {
        glyph::CenterMode::Clipboard => glyph::CenterGlyph::Clipboard,
        glyph::CenterMode::Bluetooth => glyph::CenterGlyph::Bluetooth {
            powered: state.bluetooth_powered,
        },
        glyph::CenterMode::Wifi | glyph::CenterMode::Auto => {
            if state.network.available || options.center == glyph::CenterMode::Wifi {
                network_center(state, options)
            } else if state.bluetooth_available {
                glyph::CenterGlyph::Bluetooth {
                    powered: state.bluetooth_powered,
                }
            } else {
                glyph::CenterGlyph::Clipboard
            }
        }
    };

    let volume =
        if options.volume && state.audio_output.available && state.audio_output.volume_available {
            // 静音在两种画法里都是「空」：点全暗、弧不画。
            Some(if state.audio_output.muted {
                0.0
            } else {
                state.audio_output.volume
            })
        } else {
            None
        };

    let battery = (options.battery_ring && state.battery.present).then_some(state.battery.percent);
    // 数字住在环顶缺口里，没有环就无处安放。
    let percent_label = options.battery_percent.then_some(battery).flatten();

    glyph::GlyphSpec {
        battery,
        charging: state.battery.charging,
        plugged: state.battery.plugged,
        low_power: state.battery.low_power,
        percent_label,
        center,
        volume,
        volume_style: options.volume_style,
    }
}

/// 网络符号：热点/有线/临时/共享各有专属字形，用户逐项勾选后才退回普通 Wi-Fi 弧。
fn network_center(state: &GlyphState, options: &glyph::GlyphOptions) -> glyph::CenterGlyph {
    // 用户点名要 Wi-Fi 弧（center=Wifi）时不看链路类型。
    let by_link = options.center == glyph::CenterMode::Auto;
    let wifi = || glyph::CenterGlyph::Wifi {
        bars: glyph::wifi_bars(state.network.rssi),
        powered: state.network.powered,
        connected: state.network.connected,
    };
    if !by_link {
        return wifi();
    }
    match state.network.link.as_str() {
        "hotspot" if !options.wifi_for_hotspot => glyph::CenterGlyph::Hotspot {
            powered: state.network.powered,
            connected: state.network.connected,
        },
        "ethernet" if !options.wifi_for_ethernet => {
            glyph::CenterGlyph::Ethernet { connected: true }
        }
        "temporary" if !options.wifi_for_temporary => glyph::CenterGlyph::Temporary {
            connected: state.network.connected,
        },
        "sharing" if !options.wifi_for_sharing => glyph::CenterGlyph::Shared {
            connected: state.network.connected,
        },
        _ => wifi(),
    }
}

/// 悬停提示：把图标里三区读出来的数值写成一句话。
pub fn glyph_tooltip(state: &GlyphState, spec: &glyph::GlyphSpec) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(percent) = spec.battery {
        let mut label = format!("电量 {percent}%");
        if spec.charging {
            label.push_str(" 充电中");
        } else if spec.plugged {
            label.push_str(" 已接电源");
        }
        if spec.low_power {
            label.push_str(" · 低电量模式");
        }
        parts.push(label);
    }
    match spec.center {
        glyph::CenterGlyph::Wifi {
            bars,
            powered,
            connected,
        } => {
            if !powered {
                parts.push("Wi-Fi 关闭".to_string());
            } else if !connected {
                parts.push("Wi-Fi 未连接".to_string());
            } else {
                // 互联网共享、临时连接仍画 Wi-Fi 弧，但提示里要说清是哪一种。
                let kind = match state.network.link.as_str() {
                    "sharing" => "互联网共享",
                    "temporary" => "临时网络",
                    _ => "Wi-Fi",
                };
                let signal = state
                    .network
                    .rssi
                    .map(|rssi| format!("（{rssi}dBm）"))
                    .unwrap_or_default();
                parts.push(format!("{kind} {bars} 格{signal}"));
            }
        }
        glyph::CenterGlyph::Hotspot { connected, .. } => parts.push(
            if connected {
                "iPhone 热点"
            } else {
                "热点未连接"
            }
            .to_string(),
        ),
        glyph::CenterGlyph::Ethernet { connected } => parts.push(
            if connected {
                "有线网络"
            } else {
                "有线未连接"
            }
            .to_string(),
        ),
        glyph::CenterGlyph::Temporary { connected } => parts.push(
            if connected {
                "临时网络"
            } else {
                "临时网络未连接"
            }
            .to_string(),
        ),
        glyph::CenterGlyph::Shared { connected } => parts.push(
            if connected {
                "互联网共享"
            } else {
                "互联网共享未连接"
            }
            .to_string(),
        ),
        glyph::CenterGlyph::Bluetooth { powered } => {
            parts.push(if powered { "蓝牙开" } else { "蓝牙关" }.to_string())
        }
        glyph::CenterGlyph::Clipboard => {}
    }
    // 代理紧跟网络：它说的是「这些流量现在走哪儿」，而且失效那一档必须比音量显眼。
    match state.proxy {
        ProxyBar::On => parts.push("代理已接管".to_string()),
        ProxyBar::Stale => parts.push("代理已失效（去面板关掉）".to_string()),
        // 没接管和还没读过都不占提示语：机器上绝大多数时间都是这两种。
        ProxyBar::Off | ProxyBar::Unknown => {}
    }
    if let Some(steps) = spec.volume_steps() {
        parts.push(if steps == 0 {
            "已静音".to_string()
        } else {
            format!("音量 {steps}/4")
        });
    }
    if parts.is_empty() {
        return "Halo".to_string();
    }
    format!("Halo · {}", parts.join(" · "))
}
