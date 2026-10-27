//! 权限能力枚举 + `CapabilityGate` 门控测试。

use crate::modules::capabilities::{Capability, CapabilityGate};

#[test]
fn parse_known_capabilities() {
    assert_eq!(Capability::parse("clipboard:read"), Some(Capability::ClipboardRead));
    assert_eq!(Capability::parse("clipboard:write"), Some(Capability::ClipboardWrite));
    assert_eq!(Capability::parse("fs:read:module"), Some(Capability::FsReadModule));
    assert_eq!(Capability::parse("fs:read:user"), Some(Capability::FsReadUser));
    assert_eq!(Capability::parse("fs:write:user"), Some(Capability::FsWriteUser));
    assert_eq!(Capability::parse("network:webdav"), Some(Capability::Network));
    // 别名也接受。
    assert_eq!(Capability::parse("network"), Some(Capability::Network));
}

#[test]
fn parse_unknown_capability_is_none() {
    assert_eq!(Capability::parse("clipboard:everything"), None);
    assert_eq!(Capability::parse("fs:read:all"), None);
    assert_eq!(Capability::parse(""), None);
}

#[test]
fn id_roundtrip() {
    for cap in [
        Capability::ClipboardRead,
        Capability::ClipboardWrite,
        Capability::FsReadModule,
        Capability::FsReadUser,
        Capability::FsWriteUser,
        Capability::Network,
    ] {
        assert_eq!(Capability::parse(cap.id()), Some(cap));
    }
}

#[test]
fn required_for_method_mapping() {
    assert_eq!(Capability::required_for_method("clipboard.read"), Some(Capability::ClipboardRead));
    assert_eq!(Capability::required_for_method("clipboard.write"), Some(Capability::ClipboardWrite));
    assert_eq!(Capability::required_for_method("network.request"), Some(Capability::Network));
    assert_eq!(Capability::required_for_method("fs.read"), Some(Capability::FsReadModule));
    assert_eq!(Capability::required_for_method("unknown.method"), None);
}

#[test]
fn gate_grants_and_checks() {
    let gate = CapabilityGate::new();
    assert!(!gate.check("m", Capability::ClipboardRead));

    gate.grant("m", &[Capability::ClipboardRead, Capability::ClipboardWrite]);
    assert!(gate.check("m", Capability::ClipboardRead));
    assert!(gate.check("m", Capability::ClipboardWrite));
    assert!(!gate.check("m", Capability::Network));
    assert!(!gate.check("other", Capability::ClipboardRead));
}

#[test]
fn gate_revoke_removes_all() {
    let gate = CapabilityGate::new();
    gate.grant("m", &[Capability::ClipboardRead]);
    assert!(gate.check("m", Capability::ClipboardRead));
    gate.revoke("m");
    assert!(!gate.check("m", Capability::ClipboardRead));
}

#[test]
fn gate_regrant_overwrites() {
    let gate = CapabilityGate::new();
    gate.grant("m", &[Capability::ClipboardRead, Capability::Network]);
    gate.grant("m", &[Capability::ClipboardWrite]);
    assert!(!gate.check("m", Capability::ClipboardRead));
    assert!(gate.check("m", Capability::ClipboardWrite));
}

#[test]
fn grants_for_returns_snapshot() {
    let gate = CapabilityGate::new();
    assert!(gate.grants_for("m").is_empty());
    gate.grant("m", &[Capability::Network, Capability::FsReadUser]);
    let mut got = gate.grants_for("m");
    got.sort_by_key(|c| c.id());
    assert_eq!(got, vec![Capability::FsReadUser, Capability::Network]);
}
