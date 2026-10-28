//! Embed the app icon (resource 1, which GPUI uses for the window and taskbar)
//! and version information in the Windows executable.

fn main() {
    println!("cargo:rerun-if-changed=assets/aurelius.ico");
    println!("cargo:rerun-if-changed=assets/aurelius-dev.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        // Dev builds (debug, scripts/dev.ps1) carry the icon with the amber
        // DEV strip, so the taskbar, Alt+Tab and the tray tell them apart.
        let icon = if std::env::var("PROFILE").as_deref() == Ok("debug") { "assets/aurelius-dev.ico" } else { "assets/aurelius.ico" };
        let mut res = winresource::WindowsResource::new();
        res.set_icon_with_id(icon, "1");
        let dev = std::env::var("PROFILE").as_deref() == Ok("debug");
        res.set("ProductName", if dev { "Aurelius Dev" } else { "Aurelius" });
        res.set("FileDescription", if dev { "Aurelius Dev" } else { "Aurelius" });
        if let Err(err) = res.compile() {
            println!("cargo:warning=could not embed the app icon: {err}");
        }
    }
}
