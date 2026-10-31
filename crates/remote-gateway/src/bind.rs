//! Where the gateway listens (MU3-07, OV-04).
//!
//! By default only on loopback. The tailnet reaches it through Tailscale
//! Serve: tailscaled itself listens on this PC's tailnet addresses (port 8443,
//! dev 8444), ends TLS and forwards to the loopback port. Nothing of ours
//! listens on a LAN address, so a device outside the tailnet finds no port.
//!
//! `AURELIUS_REMOTE_BIND=<ip>` is the one explicit way to listen elsewhere.
//! A loopback address is quiet, one of this PC's own tailnet addresses is
//! logged, anything else (`0.0.0.0`, `::`, a LAN address, a 100.x address that
//! is not ours, e.g. carrier NAT) is logged as a warning: then devices outside
//! the tailnet can reach the gateway, and only pairing stands in the way.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

pub const ENV: &str = "AURELIUS_REMOTE_BIND";

/// Who can reach a gateway on that address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reach {
    /// This PC only (and Tailscale Serve on it).
    Loopback,
    /// This PC's own tailnet address: tailnet devices, without Serve's TLS.
    Tailnet,
    /// Every network the address is on, also outside the tailnet.
    Exposed,
}

/// The address from the setting: loopback when unset or blank.
pub fn parse(setting: Option<&str>) -> Result<IpAddr, String> {
    match setting.map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(IpAddr::V4(Ipv4Addr::LOCALHOST)),
        Some(text) => text
            .parse()
            .map_err(|_| format!("{ENV} must be an IP address, not \"{text}\". Remote access stays off.")),
    }
}

pub fn from_env() -> Result<IpAddr, String> {
    parse(std::env::var(ENV).ok().as_deref())
}

/// Tailscale's address ranges: 100.64.0.0/10 and fd7a:115c:a1e0::/48.
fn tailnet_range(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let [a, b, ..] = v4.octets();
            a == 100 && (64..128).contains(&b)
        }
        IpAddr::V6(v6) => v6.segments()[..3] == [0xfd7a, 0x115c, 0xa1e0],
    }
}

/// `tailnet`: the addresses `tailscale ip` gives for this PC (none without
/// Tailscale, so a 100.x address then counts as exposed).
pub fn reach(ip: IpAddr, tailnet: &[IpAddr]) -> Reach {
    let ip = ip.to_canonical();
    if ip.is_loopback() {
        Reach::Loopback
    } else if tailnet_range(ip) && tailnet.contains(&ip) {
        Reach::Tailnet
    } else {
        Reach::Exposed
    }
}

/// The warning to log for a gateway bound on `addr`; none for loopback.
pub fn warning(addr: SocketAddr, reach: Reach) -> Option<String> {
    match reach {
        Reach::Loopback => None,
        Reach::Tailnet => Some(format!(
            "remote gateway listens on the tailnet address {addr} ({ENV}): tailnet devices reach it without Tailscale Serve's HTTPS"
        )),
        Reach::Exposed => Some(format!(
            "remote gateway listens on {addr} ({ENV}): devices OUTSIDE the tailnet can reach it; unset {ENV} to listen on loopback only"
        )),
    }
}

/// The URL that reaches a gateway bound on `addr` from this PC (the target of
/// Tailscale Serve): loopback for a wildcard address.
pub fn local_url(addr: SocketAddr) -> String {
    let ip = match addr.ip() {
        IpAddr::V4(v4) if v4.is_unspecified() => IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V6(v6) if v6.is_unspecified() => IpAddr::V6(Ipv6Addr::LOCALHOST),
        ip => ip,
    };
    format!("http://{}", SocketAddr::new(ip, addr.port()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(text: &str) -> IpAddr {
        text.parse().unwrap()
    }

    #[test]
    fn default_is_loopback() {
        assert_eq!(parse(None), Ok(ip("127.0.0.1")));
        assert_eq!(parse(Some("")), Ok(ip("127.0.0.1")));
        assert_eq!(parse(Some("  ")), Ok(ip("127.0.0.1")));
        assert_eq!(reach(parse(None).unwrap(), &[]), Reach::Loopback);
    }

    #[test]
    fn a_setting_that_is_no_address_is_refused() {
        // Refused, not read as "anything": the gateway then does not start.
        for text in ["localhost", "0.0.0.0:47613", "*", "lan", "192.168.1"] {
            assert!(parse(Some(text)).is_err(), "{text}");
        }
        assert_eq!(parse(Some(" 0.0.0.0 ")), Ok(ip("0.0.0.0")));
    }

    #[test]
    fn only_loopback_and_our_own_tailnet_addresses_are_not_exposed() {
        let ours = [ip("100.101.102.103"), ip("fd7a:115c:a1e0::ab12:cd34")];
        for text in ["127.0.0.1", "127.0.0.2", "::1", "::ffff:127.0.0.1"] {
            assert_eq!(reach(ip(text), &ours), Reach::Loopback, "{text}");
        }
        for text in ["100.101.102.103", "fd7a:115c:a1e0::ab12:cd34"] {
            assert_eq!(reach(ip(text), &ours), Reach::Tailnet, "{text}");
        }
        for text in [
            "0.0.0.0",
            "::",
            "192.168.1.10",
            "10.0.0.5",
            "172.17.0.1",
            "fe80::1",
            "fd00::1",
            // In the tailnet range, but not ours (another device, carrier NAT).
            "100.64.0.1",
            "fd7a:115c:a1e0::1",
            // Ours by list, but outside the range: never trusted as tailnet.
            "203.0.113.7",
        ] {
            assert_eq!(reach(ip(text), &[ours[0], ours[1], ip("203.0.113.7")]), Reach::Exposed, "{text}");
        }
    }

    #[test]
    fn without_tailscale_only_loopback_is_not_exposed() {
        // Tailscale off (a test root, AURELIUS_NO_TAILSCALE): `tailscale::own_ips` gives nothing.
        assert_eq!(reach(ip("127.0.0.1"), &[]), Reach::Loopback);
        assert_eq!(reach(ip("100.101.102.103"), &[]), Reach::Exposed);
        assert_eq!(reach(ip("fd7a:115c:a1e0::ab12:cd34"), &[]), Reach::Exposed);
    }

    #[test]
    fn every_address_but_loopback_is_logged() {
        let addr = |text: &str| SocketAddr::new(ip(text), 47613);
        assert_eq!(warning(addr("127.0.0.1"), Reach::Loopback), None);
        assert!(warning(addr("100.101.102.103"), Reach::Tailnet).unwrap().contains("tailnet address 100.101.102.103:47613"));
        let exposed = warning(addr("0.0.0.0"), Reach::Exposed).unwrap();
        assert!(exposed.contains("0.0.0.0:47613") && exposed.contains("OUTSIDE the tailnet") && exposed.contains(ENV));
    }

    #[test]
    fn serve_targets_the_bound_address() {
        let url = |text: &str| local_url(text.parse().unwrap());
        assert_eq!(url("127.0.0.1:47613"), "http://127.0.0.1:47613");
        assert_eq!(url("0.0.0.0:47613"), "http://127.0.0.1:47613");
        assert_eq!(url("[::]:47613"), "http://[::1]:47613");
        assert_eq!(url("100.101.102.103:47613"), "http://100.101.102.103:47613");
        assert_eq!(url("[fd7a:115c:a1e0::1]:47613"), "http://[fd7a:115c:a1e0::1]:47613");
    }
}
