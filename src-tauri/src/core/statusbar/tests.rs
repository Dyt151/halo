use super::spec::{glyph_spec, glyph_tooltip};
use super::*;
fn audio(volume: f64, muted: bool, volume_available: bool) -> AudioDeviceState {
    AudioDeviceState {
        available: true,
        device_id: 1,
        device_name: "扬声器".to_string(),
        volume,
        volume_available,
        muted,
        mute_available: true,
        devices: Vec::new(),
    }
}

fn state(battery_present: bool, wifi: bool, bluetooth: Option<bool>) -> GlyphState {
    GlyphState {
        battery: BatteryState {
            present: battery_present,
            percent: 66,
            charging: false,
            charged: false,
            plugged: false,
            low_power: false,
            minutes_to_empty: None,
            minutes_to_full: None,
        },
        network: NetworkState {
            available: wifi,
            powered: wifi,
            connected: wifi,
            ssid: wifi.then(|| "Office".to_string()),
            rssi: wifi.then_some(-52),
            band: wifi.then(|| "5 GHz".to_string()),
            name_access: "authorized".to_string(),
            interface: wifi.then(|| "en0".to_string()),
            ipv4: wifi.then(|| "10.0.0.2".to_string()),
            link: if wifi { "wifi" } else { "none" }.to_string(),
        },
        bluetooth_available: bluetooth.is_some(),
        bluetooth_powered: bluetooth.unwrap_or(false),
        audio_output: audio(0.6, false, true),
        proxy: ProxyBar::Unknown,
    }
}

#[test]
fn audio_scope_parses_only_known_values() {
    assert_eq!(AudioScope::parse("output"), Some(AudioScope::Output));
    assert_eq!(AudioScope::parse("input"), Some(AudioScope::Input));
    assert_eq!(AudioScope::parse("mic"), None);
    assert_eq!(AudioScope::parse(""), None);
}

#[test]
fn auto_center_prefers_wifi_then_bluetooth_then_clipboard() {
    let options = glyph::GlyphOptions::default();
    assert_eq!(
        glyph_spec(&state(true, true, Some(true)), &options).center,
        glyph::CenterGlyph::Wifi {
            bars: 3,
            powered: true,
            connected: true
        }
    );
    assert_eq!(
        glyph_spec(&state(true, false, Some(true)), &options).center,
        glyph::CenterGlyph::Bluetooth { powered: true }
    );
    assert_eq!(
        glyph_spec(&state(true, false, None), &options).center,
        glyph::CenterGlyph::Clipboard
    );
}

#[test]
fn link_type_switches_the_center_mark() {
    let options = glyph::GlyphOptions::default();
    let mut hotspot = state(true, true, None);
    hotspot.network.ipv4 = Some("172.20.10.2".to_string());
    hotspot.network.link = "hotspot".to_string();
    assert_eq!(
        glyph_spec(&hotspot, &options).center,
        glyph::CenterGlyph::Hotspot {
            powered: true,
            connected: true
        }
    );
    let mut wired = state(true, true, None);
    wired.network.link = "ethernet".to_string();
    assert_eq!(
        glyph_spec(&wired, &options).center,
        glyph::CenterGlyph::Ethernet { connected: true }
    );
    // 共享/临时连接同样有专用字形：实心扇形再抠掉一块。
    let mut sharing = state(true, true, None);
    sharing.network.link = "sharing".to_string();
    assert_eq!(
        glyph_spec(&sharing, &options).center,
        glyph::CenterGlyph::Shared { connected: true }
    );
    let mut temporary = state(true, true, None);
    temporary.network.link = "temporary".to_string();
    assert_eq!(
        glyph_spec(&temporary, &options).center,
        glyph::CenterGlyph::Temporary { connected: true }
    );
    let wifi_arc = glyph::CenterGlyph::Wifi {
        bars: 3,
        powered: true,
        connected: true,
    };
    // 「连接图标样式」按链路逐项退回 Wi-Fi 弧，勾了哪种只影响哪种。
    let back_to_wifi = |flag: &str| {
        let mut o = glyph::GlyphOptions::default();
        match flag {
            "hotspot" => o.wifi_for_hotspot = true,
            "ethernet" => o.wifi_for_ethernet = true,
            "temporary" => o.wifi_for_temporary = true,
            _ => o.wifi_for_sharing = true,
        }
        o
    };
    let cases = [
        ("hotspot", &hotspot),
        ("ethernet", &wired),
        ("temporary", &temporary),
        ("sharing", &sharing),
    ];
    for (link, sample) in cases {
        assert_eq!(
            glyph_spec(sample, &back_to_wifi(link)).center,
            wifi_arc,
            "{link}"
        );
        // 别的链路不受影响。
        for (other, other_sample) in cases.iter().filter(|(l, _)| *l != link) {
            assert_eq!(
                glyph_spec(other_sample, &back_to_wifi(link)).center,
                glyph_spec(other_sample, &options).center,
                "{link} 不该动 {other}"
            );
        }
    }
    // 点名要 Wi-Fi 弧时不看链路类型。
    let forced = glyph::GlyphOptions {
        center: glyph::CenterMode::Wifi,
        ..glyph::GlyphOptions::default()
    };
    assert_eq!(glyph_spec(&hotspot, &forced).center, wifi_arc);
}

#[test]
fn forced_center_ignores_hardware_availability() {
    let options = glyph::GlyphOptions {
        center: glyph::CenterMode::Bluetooth,
        ..glyph::GlyphOptions::default()
    };
    assert_eq!(
        glyph_spec(&state(true, true, None), &options).center,
        glyph::CenterGlyph::Bluetooth { powered: false }
    );
}

#[test]
fn ring_and_volume_respect_options_and_device_support() {
    let options = glyph::GlyphOptions::default();
    let spec = glyph_spec(&state(true, true, None), &options);
    assert_eq!(spec.battery, Some(66));
    assert_eq!(spec.percent_label, Some(66));
    assert_eq!(spec.volume_steps(), Some(3));
    // 台式机没有电池：不画环。
    let mut desktop = state(false, true, None);
    desktop.audio_output = audio(0.6, false, false);
    let spec = glyph_spec(&desktop, &options);
    assert_eq!(spec.battery, None);
    assert_eq!(spec.percent_label, None);
    assert_eq!(spec.volume, None);
    // 静音：保留音量区，但一格都不亮。
    let mut muted = state(true, true, None);
    muted.audio_output = audio(0.6, true, true);
    assert_eq!(glyph_spec(&muted, &options).volume_steps(), Some(0));
    // 关掉开关就不画。
    let options = glyph::GlyphOptions {
        battery_ring: false,
        volume: false,
        ..glyph::GlyphOptions::default()
    };
    let spec = glyph_spec(&state(true, true, None), &options);
    assert_eq!(spec.battery, None);
    assert_eq!(spec.volume, None);
    // 数字住在环顶缺口里：环关了，不许出现数字。
    let no_label = glyph::GlyphOptions {
        battery_percent: false,
        ..glyph::GlyphOptions::default()
    };
    assert_eq!(
        glyph_spec(&state(true, true, None), &no_label).percent_label,
        None
    );
    let ring_off = glyph::GlyphOptions {
        battery_ring: false,
        ..glyph::GlyphOptions::default()
    };
    assert_eq!(
        glyph_spec(&state(true, true, None), &ring_off).percent_label,
        None
    );
}

#[test]
fn tooltip_reads_out_the_drawn_regions() {
    let options = glyph::GlyphOptions::default();
    let sample = state(true, true, None);
    let spec = glyph_spec(&sample, &options);
    assert_eq!(
        glyph_tooltip(&sample, &spec),
        "Halo · 电量 66% · Wi-Fi 3 格（-52dBm） · 音量 3/4"
    );
    let mut off = state(true, false, Some(false));
    off.audio_output = audio(0.6, true, true);
    let spec = glyph_spec(&off, &options);
    assert_eq!(
        glyph_tooltip(&off, &spec),
        "Halo · 电量 66% · 蓝牙关 · 已静音"
    );
    // 热点与有线：提示里必须点出链路类型，不能一律说 Wi-Fi。
    let mut hotspot = state(true, true, None);
    hotspot.network.link = "hotspot".to_string();
    let spec = glyph_spec(&hotspot, &options);
    assert_eq!(
        glyph_tooltip(&hotspot, &spec),
        "Halo · 电量 66% · iPhone 热点 · 音量 3/4"
    );
    // 互联网共享现在是实心扇形抠箭头，提示也跟着换名字。
    let mut sharing = state(true, true, None);
    sharing.network.link = "sharing".to_string();
    let spec = glyph_spec(&sharing, &options);
    assert_eq!(
        glyph_tooltip(&sharing, &spec),
        "Halo · 电量 66% · 互联网共享 · 音量 3/4"
    );
    let forced_wifi = glyph::GlyphOptions {
        center: glyph::CenterMode::Wifi,
        ..glyph::GlyphOptions::default()
    };
    assert_eq!(
        glyph_tooltip(&sharing, &glyph_spec(&sharing, &forced_wifi)),
        "Halo · 电量 66% · 互联网共享 3 格（-52dBm） · 音量 3/4"
    );
    // 代理：接管和失效都要占一格（失效那条是断网预警，不能省）；没接管时不占，
    // 因为绝大多数时间系统代理都是关的，提示里挂一句「代理未接管」只是噪音。
    let mut taken = state(true, true, None);
    taken.proxy = ProxyBar::On;
    assert_eq!(
        glyph_tooltip(&taken, &glyph_spec(&taken, &options)),
        "Halo · 电量 66% · Wi-Fi 3 格（-52dBm） · 代理已接管 · 音量 3/4"
    );
    taken.proxy = ProxyBar::Stale;
    assert!(
        glyph_tooltip(&taken, &glyph_spec(&taken, &options)).contains("代理已失效"),
        "核心退出后提示要转成失效"
    );
    for quiet in [ProxyBar::Off, ProxyBar::Unknown] {
        taken.proxy = quiet;
        assert!(
            !glyph_tooltip(&taken, &glyph_spec(&taken, &options)).contains("代理"),
            "{quiet:?} 不该出现在提示里"
        );
    }
}
