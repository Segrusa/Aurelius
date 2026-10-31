//! R4 M-1: a daemon on a test or temp data folder never calls `tailscale`,
//! without any variable. A stand-in `tailscale` first on PATH writes a marker
//! when it runs; the real binary is never reached. One test: it changes the
//! environment (PATH) of the whole process.

#![cfg(unix)]

mod common;

use aurelius_protocol::HostWrite;
use common::*;
use std::path::Path;

fn set(name: &str, value: Option<&std::ffi::OsStr>) {
    // SAFETY (test): the only test in this binary, set before its daemon starts.
    unsafe {
        match value {
            Some(value) => std::env::set_var(name, value),
            None => std::env::remove_var(name),
        }
    }
}

/// A `tailscale` that only notes that it ran.
fn stand_in(bin: &Path, marker: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let script = bin.join("tailscale");
    std::fs::write(&script, format!("#!/bin/sh\necho \"$@\" >> '{}'\n", marker.display())).unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_daemon_on_a_temp_folder_never_calls_tailscale() {
    let dir = tempfile::tempdir().unwrap();
    let (root, bin, marker) = (dir.path().join("root"), dir.path().join("bin"), dir.path().join("tailscale-ran"));
    std::fs::create_dir_all(&bin).unwrap();
    stand_in(&bin, &marker);
    let path = std::env::join_paths(std::iter::once(bin.clone()).chain(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()))).unwrap();
    set("PATH", Some(&path));
    set("AURELIUS_NO_TAILSCALE", None);
    set("AURELIUS_TAILSCALE", None);
    set("AURELIUS_REMOTE_PORT", Some("0".as_ref()));
    set("AURELIUS_ENV", Some("dev".as_ref()));

    let _daemon = start(&root).await;
    let desktop = connect(&root).await;
    let status = desktop.remote(HostWrite::RemoteEnable).await.unwrap();
    assert!(status.enabled);
    desktop.remote_status().await.unwrap();
    desktop.pairing_start().await.unwrap();
    desktop.remote(HostWrite::RemoteDisable).await.unwrap();
    assert!(!marker.exists(), "tailscale ran: {}", std::fs::read_to_string(&marker).unwrap_or_default());
    assert!(!status.tailscale.installed && !status.tailscale.serving);

    // The stand-in is the one found: with the explicit opt-in it does run.
    set("AURELIUS_TAILSCALE", Some("1".as_ref()));
    desktop.remote_status().await.unwrap();
    set("AURELIUS_TAILSCALE", None);
    assert!(std::fs::read_to_string(&marker).unwrap().contains("status --json"));
}
