//! Tailscale Serve: publish the loopback gateway on this PC's tailnet name
//! with a real HTTPS certificate. Never Funnel (public internet).
//!
//! Only this host's own data folder touches Tailscale. Another root (a test
//! instance, a copy, anything under the temp folder) makes no call at all,
//! unless `AURELIUS_TAILSCALE=1` opts in. `AURELIUS_NO_TAILSCALE=1` skips
//! every call, also for the own folder (machines without Tailscale).

use aurelius_protocol::remote::TailscaleStatus;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::process::Command;

pub const OFF_ENV: &str = "AURELIUS_NO_TAILSCALE";
pub const OPT_IN_ENV: &str = "AURELIUS_TAILSCALE";

/// No Tailscale call for the data folder `root`.
pub fn disabled(root: &Path) -> bool {
    let opt_in = std::env::var_os(OPT_IN_ENV).is_some_and(|v| v == "1");
    let own = aurelius_host::default_data_dir().ok();
    off(root, own.as_deref(), &std::env::temp_dir(), std::env::var_os(OFF_ENV).is_some(), opt_in)
}

fn off(root: &Path, own: Option<&Path>, temp: &Path, no_tailscale: bool, opt_in: bool) -> bool {
    if no_tailscale {
        return true;
    }
    if opt_in {
        return false;
    }
    let real = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    let root = real(root);
    // A HOME in the temp folder makes the default root a temp folder too.
    own.is_none_or(|own| real(own) != root) || root.starts_with(real(temp))
}

fn exe() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let installed = PathBuf::from(r"C:\Program Files\Tailscale\tailscale.exe");
        if installed.exists() {
            return Some(installed);
        }
    }
    #[cfg(target_os = "macos")]
    {
        let app = PathBuf::from("/Applications/Tailscale.app/Contents/MacOS/Tailscale");
        if app.exists() {
            return Some(app);
        }
    }
    let name = if cfg!(windows) { "tailscale.exe" } else { "tailscale" };
    std::env::var_os("PATH").and_then(|paths| std::env::split_paths(&paths).map(|p| p.join(name)).find(|p| p.exists()))
}

fn run(args: &[&str]) -> Result<String, String> {
    let exe = exe().ok_or_else(|| "Tailscale is not installed on this PC.".to_string())?;
    let mut command = Command::new(exe);
    command.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW: the background service has no console to flash.
        command.creation_flags(0x0800_0000);
    }
    let output = command.output().map_err(|e| format!("Could not run Tailscale: {e}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        Err(if stderr.is_empty() { stdout } else { stderr })
    }
}

/// This PC's tailnet addresses (`tailscale ip`); none when Tailscale is off,
/// missing or not connected.
pub fn own_ips(root: &Path) -> Vec<IpAddr> {
    if disabled(root) {
        return Vec::new();
    }
    run(&["ip"]).map(|text| parse_ips(&text)).unwrap_or_default()
}

fn parse_ips(text: &str) -> Vec<IpAddr> {
    text.lines().filter_map(|line| line.trim().parse().ok()).collect()
}

/// `local`: where the gateway listens (loopback unless `AURELIUS_REMOTE_BIND`).
pub fn status(root: &Path, https_port: u16, local: SocketAddr) -> TailscaleStatus {
    if disabled(root) {
        return TailscaleStatus { error: Some("Tailscale is turned off for this instance.".into()), ..Default::default() };
    }
    if exe().is_none() {
        return TailscaleStatus { error: Some("Tailscale is not installed on this PC.".into()), ..Default::default() };
    }
    let mut status = TailscaleStatus { installed: true, ..Default::default() };
    let json: serde_json::Value = match run(&["status", "--json"]).map(|t| serde_json::from_str(&t)) {
        Ok(Ok(json)) => json,
        Ok(Err(e)) => {
            status.error = Some(format!("Unexpected Tailscale status: {e}"));
            return status;
        }
        Err(e) => {
            status.error = Some(e);
            return status;
        }
    };
    status.online = json["BackendState"].as_str() == Some("Running");
    status.dns_name = json["Self"]["DNSName"].as_str().map(|d| d.trim_end_matches('.').to_owned()).filter(|d| !d.is_empty());
    let has_certs = json["CertDomains"].as_array().is_some_and(|d| !d.is_empty());
    if !status.online {
        status.error = Some("Tailscale is not connected on this PC. Sign in to Tailscale.".into());
    } else if status.dns_name.is_none() {
        status.error = Some("Turn on MagicDNS in the Tailscale admin console (DNS).".into());
    } else if !has_certs {
        status.error = Some("Turn on HTTPS Certificates in the Tailscale admin console (DNS → HTTPS Certificates).".into());
    }
    if let (Some(dns), Ok(serve)) = (&status.dns_name, run(&["serve", "status", "--json"])) {
        status.serving = serving(&serve, dns, https_port, local);
    }
    status
}

/// Does the Serve config forward `https://<dns>:<https_port>/` to our port?
fn serving(serve_json: &str, dns: &str, https_port: u16, local: SocketAddr) -> bool {
    let Ok(json) = serde_json::from_str::<serde_json::Value>(serve_json) else { return false };
    let host = format!("{dns}:{https_port}");
    json["Web"][&host]["Handlers"]["/"]["Proxy"].as_str() == Some(crate::bind::local_url(local).as_str())
}

/// Serve `https://<this PC's tailnet name>:<https_port>` → the gateway. Serve
/// listens on the tailnet addresses only; never `funnel` (public internet).
pub fn serve_on(root: &Path, https_port: u16, local: SocketAddr) -> Result<(), String> {
    if disabled(root) {
        return Ok(());
    }
    let https = format!("--https={https_port}");
    run(&["serve", "--bg", "--yes", &https, &crate::bind::local_url(local)])
        .map(|_| ())
        .map_err(|e| format!("Tailscale Serve did not start: {e}"))
}

pub fn serve_off(root: &Path, https_port: u16) -> Result<(), String> {
    if disabled(root) || exe().is_none() {
        return Ok(());
    }
    let https = format!("--https={https_port}");
    match run(&["serve", &https, "off"]) {
        Ok(_) => Ok(()),
        // Nothing was served on that port: already off.
        Err(e) if e.contains("not found") || e.contains("does not exist") || e.contains("no serve config") => Ok(()),
        Err(e) => Err(format!("Tailscale Serve did not stop: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::{off, parse_ips, serving};
    use std::path::Path;

    #[test]
    fn only_the_own_data_folder_uses_tailscale() {
        let own = Path::new("/home/me/.local/share/Aurelius");
        let temp = Path::new("/tmp");
        let call = |root: &str, own: Option<&Path>, no: bool, opt_in: bool| !off(Path::new(root), own, temp, no, opt_in);
        assert!(call("/home/me/.local/share/Aurelius", Some(own), false, false));
        // AURELIUS_DATA_DIR elsewhere, a temp folder, no known default: never.
        assert!(!call("/home/me/aurelius-test", Some(own), false, false));
        assert!(!call("/tmp/.tmpAb12", Some(own), false, false));
        assert!(!call("/tmp/home/.local/share/Aurelius", Some(Path::new("/tmp/home/.local/share/Aurelius")), false, false));
        assert!(!call("/home/me/.local/share/Aurelius", None, false, false));
        // AURELIUS_TAILSCALE=1 opts in; AURELIUS_NO_TAILSCALE wins over everything.
        assert!(call("/tmp/.tmpAb12", Some(own), false, true));
        assert!(!call("/home/me/.local/share/Aurelius", Some(own), true, false));
        assert!(!call("/tmp/.tmpAb12", Some(own), true, true));
    }

    #[test]
    fn recognizes_our_route() {
        let json = r#"{"TCP":{"8443":{"HTTPS":true}},"Web":{"pc.tail1.ts.net:8443":{"Handlers":{"/":{"Proxy":"http://127.0.0.1:47613"}}}}}"#;
        let local = "127.0.0.1:47613".parse().unwrap();
        assert!(serving(json, "pc.tail1.ts.net", 8443, local));
        assert!(serving(json, "pc.tail1.ts.net", 8443, "0.0.0.0:47613".parse().unwrap()));
        assert!(!serving(json, "pc.tail1.ts.net", 8444, local));
        assert!(!serving(json, "pc.tail1.ts.net", 8443, "127.0.0.1:47614".parse().unwrap()));
        assert!(!serving("{}", "pc.tail1.ts.net", 8443, local));
    }

    #[test]
    fn reads_tailscale_ip() {
        let ips = parse_ips("100.101.102.103\nfd7a:115c:a1e0::ab12:cd34\n\n");
        assert_eq!(ips, ["100.101.102.103".parse::<std::net::IpAddr>().unwrap(), "fd7a:115c:a1e0::ab12:cd34".parse().unwrap()]);
        assert!(parse_ips("Tailscale is stopped.").is_empty());
    }
}
