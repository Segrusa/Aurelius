//! MU3-07 AC2 / OV-04: a real gateway listens on loopback only unless
//! `AURELIUS_REMOTE_BIND` says otherwise. One test: it changes the environment.

use aurelius_domain::{HostId, PrincipalId};
use aurelius_remote_gateway::Hub;
use aurelius_storage::Store;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

fn set(name: &str, value: Option<&str>) {
    // SAFETY (test): the only test in this binary; nothing else reads the environment meanwhile.
    unsafe {
        match value {
            Some(value) => std::env::set_var(name, value),
            None => std::env::remove_var(name),
        }
    }
}

/// This PC's address on its default route (LAN, Wi-Fi), if it has one. A
/// UDP "connect" picks the source address without sending anything.
fn lan_ip() -> Option<IpAddr> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("192.0.2.1:9").ok()?;
    Some(socket.local_addr().ok()?.ip()).filter(|ip| !ip.is_loopback() && !ip.is_unspecified())
}

async fn health(addr: SocketAddr) -> bool {
    let url = format!("{}/v1/health", aurelius_remote_gateway::bind::local_url(addr));
    reqwest::get(url).await.is_ok_and(|r| r.status().is_success())
}

#[tokio::test]
async fn listens_on_loopback_only_unless_set_explicitly() {
    set("AURELIUS_REMOTE_PORT", Some("0"));
    set("AURELIUS_NO_TAILSCALE", Some("1"));
    set(aurelius_remote_gateway::bind::ENV, None);
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(dir.path()).unwrap());
    let hub = Hub::new(store, HostId::new(), PrincipalId::new(), dir.path().to_path_buf(), "test");

    // Default, without Tailscale: loopback, reachable there and nowhere else.
    let status = hub.enable().await.unwrap();
    let addr = hub.local_addr().await.unwrap();
    assert_eq!(addr.ip(), IpAddr::from([127, 0, 0, 1]));
    assert_eq!(status.port, addr.port());
    assert!(!status.tailscale.serving);
    assert!(health(addr).await);
    match lan_ip() {
        Some(lan) => {
            let refused = tokio::net::TcpStream::connect(SocketAddr::new(lan, addr.port())).await;
            assert!(refused.is_err(), "the gateway answers on the LAN address {lan}");
        }
        None => eprintln!("no default route: LAN probe skipped"),
    }
    hub.disable().await.unwrap();
    assert!(hub.local_addr().await.is_none());

    // A setting that is no address: refused, nothing listens.
    set(aurelius_remote_gateway::bind::ENV, Some("everywhere"));
    let refused = hub.enable().await.expect_err("a bad bind setting is refused");
    assert!(refused.message.contains("AURELIUS_REMOTE_BIND"), "{}", refused.message);
    assert!(hub.local_addr().await.is_none());

    // Explicitly every interface: allowed (with a warning in the log), and
    // Tailscale Serve would still target loopback.
    set(aurelius_remote_gateway::bind::ENV, Some("0.0.0.0"));
    hub.enable().await.unwrap();
    let addr = hub.local_addr().await.unwrap();
    assert!(addr.ip().is_unspecified());
    assert_eq!(aurelius_remote_gateway::bind::local_url(addr), format!("http://127.0.0.1:{}", addr.port()));
    assert!(health(addr).await);
    hub.disable().await.unwrap();

    // Unset again: back to loopback.
    set(aurelius_remote_gateway::bind::ENV, None);
    hub.enable().await.unwrap();
    assert!(hub.local_addr().await.unwrap().ip().is_loopback());
    hub.disable().await.unwrap();
}
