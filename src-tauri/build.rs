fn main() {
    // 「立即充满电」要调 PowerUI 私有框架，走 Status Trio 的 Objective-C 桥：
    // Rust 直接向私有类发消息太容易踩 ABI，编成静态库由 C 门面统一收口。
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rerun-if-changed=native/powerui");
        cc::Build::new()
            .file("native/powerui/PowerUIPrivateAPI.m")
            .file("native/powerui/halo_powerui_shim.m")
            .include("native/powerui")
            .flag("-fobjc-arc")
            .flag("-Wno-nullability-completeness")
            .compile("halo_powerui");
        println!("cargo:rustc-link-lib=framework=Foundation");
    }

    tauri_build::build()
}
