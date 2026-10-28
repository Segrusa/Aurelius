//! Tailscale Serve: publish the loopback gateway on this PC's tailnet name
//! with a real HTTPS certificate. Never Funnel (public internet).
//!
//! `AURELIUS_NO_TAILSCALE=1` skips every call (tests, machines without it).

use aurelius_protocol::remote::TailscaleStatus;
use std::path::PathBuf;
use std::process::Command;

fn disabled() -> bool {
    std::env::var_os("AURELIUS_NO_TAILSCALE").is_some()
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

fn target(local_port: u16) -> String {
    format!("http://127.0.0.1:{local_port}")
}

pub fn status(https_port: u16, local_port: u16) -> TailscaleStatus {
    if disabled() {
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
        status.serving = serving(&serve, dns, https_port, local_port);
    }
    status
}

/// Does the Serve config forward `https://<dns>:<https_port>/` to our port?
fn serving(serve_json: &str, dns: &str, https_port: u16, local_port: u16) -> bool {
    let Ok(json) = serde_json::from_str::<serde_json::Value>(serve_json) else { return false };
    let host = format!("{dns}:{https_port}");
    json["Web"][&host]["Handlers"]["/"]["Proxy"].as_str() == Some(target(local_port).as_str())
}

pub fn serve_on(https_port: u16, local_port: u16) -> Result<(), String> {
    if disabled() {
        return Ok(());
    }
    let https = format!("--https={https_port}");
    run(&["serve", "--bg", "--yes", &https, &target(local_port)])
        .map(|_| ())
        .map_err(|e| format!("Tailscale Serve did not start: {e}"))
}

pub fn serve_off(https_port: u16) -> Result<(), String> {
    if disabled() || exe().is_none() {
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
    use super::serving;

    #[test]
    fn recognizes_our_route() {
        let json = r#"{"TCP":{"8443":{"HTTPS":true}},"Web":{"pc.tail1.ts.net:8443":{"Handlers":{"/":{"Proxy":"http://127.0.0.1:47613"}}}}}"#;
        assert!(serving(json, "pc.tail1.ts.net", 8443, 47613));
        assert!(!serving(json, "pc.tail1.ts.net", 8444, 47613));
        assert!(!serving("{}", "pc.tail1.ts.net", 8443, 47613));
    }
}
