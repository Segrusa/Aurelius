//! Playwright MCP as the browser backend (instead of Claude in Chrome). Runs
//! do not drive the user's own browser profile: the daemon keeps one Chrome or
//! Edge window of its own (an Aurelius profile under the data dir, so logins
//! made there stay between runs) with a DevTools port, and every run starts
//! `@playwright/mcp` attached to it over CDP. Runs in parallel share the
//! window and each opens its own tab (the run prompt says so).
//!
//! The package is installed once with npm under the data dir (pinned), so a
//! run starts `node cli.js` directly instead of resolving `npx` every time.
//! Needs Node.js on the server, as Claude Code itself does.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use aurelius_protocol::{BrowserBackend, BrowserSetting, ErrorCode, ProtocolError};
use tokio::process::Command;

/// The pinned `@playwright/mcp` release (checked against CLI 2.1.289 and Edge 154).
pub const PACKAGE_VERSION: &str = "0.0.83";
const SETTINGS_KEY: &str = aurelius_storage::settings::BROWSER_BACKEND;
static PLAYWRIGHT: AtomicBool = AtomicBool::new(false);
/// How long npm may take for the first install.
const INSTALL_TIMEOUT: Duration = Duration::from_secs(300);
/// How long to wait for the browser to write its DevTools port.
const START_TIMEOUT: Duration = Duration::from_secs(15);
const POLL: Duration = Duration::from_millis(300);

pub fn backend() -> BrowserBackend {
    if PLAYWRIGHT.load(Ordering::Relaxed) { BrowserBackend::Playwright } else { BrowserBackend::ClaudeInChrome }
}

pub(crate) fn load_stored(store: &aurelius_storage::Store) {
    if let Ok(Some(json)) = store.read(|c| aurelius_storage::settings::get(c, SETTINGS_KEY)) {
        if let Ok(backend) = serde_json::from_str::<BrowserBackend>(&json) {
            PLAYWRIGHT.store(backend == BrowserBackend::Playwright, Ordering::Relaxed);
        }
    }
}

/// `browser.set_backend`: keep the choice and use it from the next run on.
/// The first Playwright run installs the package (npm, under a minute).
pub async fn set_backend(shared: &std::sync::Arc<impl crate::Service>, backend: BrowserBackend) -> Result<BrowserSetting, ProtocolError> {
    let internal = |e: String| ProtocolError::new(ErrorCode::Internal, e);
    let json = serde_json::to_string(&backend).map_err(|e| internal(e.to_string()))?;
    let store = shared.store().clone();
    tokio::task::spawn_blocking(move || store.write(move |tx| aurelius_storage::settings::put(tx, SETTINGS_KEY, &json, aurelius_storage::now_ms())))
        .await
        .map_err(|e| internal(e.to_string()))?
        .map_err(|e| internal(e.to_string()))?;
    PLAYWRIGHT.store(backend == BrowserBackend::Playwright, Ordering::Relaxed);
    Ok(crate::chrome::setting())
}

/// Everything Playwright keeps, under the server's data folder.
fn root(data: &Path) -> PathBuf {
    data.join("playwright")
}

fn cli_path(root: &Path) -> PathBuf {
    root.join("node_modules").join("@playwright").join("mcp").join("cli.js")
}

/// The installed package's version, from its package.json.
fn installed_version(root: &Path) -> Option<String> {
    let json = std::fs::read_to_string(root.join("node_modules").join("@playwright").join("mcp").join("package.json")).ok()?;
    serde_json::from_str::<serde_json::Value>(&json).ok()?["version"].as_str().map(str::to_owned)
}

/// Install (or move to) the pinned package under the data dir; a no-op when it is there.
/// One install at a time: parallel runs wait for the first.
async fn ensure_installed(root: &Path) -> Result<PathBuf, String> {
    static INSTALL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _guard = INSTALL.lock().await;
    if cli_path(root).is_file() && installed_version(root).as_deref() == Some(PACKAGE_VERSION) {
        return Ok(cli_path(root));
    }
    std::fs::create_dir_all(root).map_err(|e| format!("could not create {}: {e}", root.display()))?;
    if !root.join("package.json").is_file() {
        std::fs::write(root.join("package.json"), "{\"name\":\"aurelius-playwright\",\"private\":true}\n").map_err(|e| e.to_string())?;
    }
    let package = format!("@playwright/mcp@{PACKAGE_VERSION}");
    // npm is a .cmd script on Windows, which only cmd starts.
    let mut command = if cfg!(windows) { let mut c = Command::new("cmd"); c.args(["/C", "npm"]); c } else { Command::new("npm") };
    crate::chrome::no_console(&mut command);
    command.args(["install", "--no-audit", "--no-fund", "--save-exact", &package]).current_dir(root).stdin(Stdio::null()).kill_on_drop(true);
    let out = tokio::time::timeout(INSTALL_TIMEOUT, command.output())
        .await
        .map_err(|_| format!("npm install {package} took longer than {} seconds", INSTALL_TIMEOUT.as_secs()))?
        .map_err(|e| format!("could not run npm (is Node.js installed?): {e}"))?;
    if !out.status.success() || !cli_path(root).is_file() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!("npm install {package} failed: {}", err.trim().lines().last().unwrap_or("no output")));
    }
    tracing::info!(version = PACKAGE_VERSION, "installed Playwright MCP");
    Ok(cli_path(root))
}


/// Chrome writes `DevToolsActivePort` (port on the first line) into the profile
/// when it starts with `--remote-debugging-port=0`; a stale file after a crash
/// is caught by the connect check.
pub(crate) fn port_from_file(text: &str) -> Option<u16> {
    text.lines().next()?.trim().parse().ok().filter(|p| *p > 0)
}

async fn live_port(profile: &Path) -> Option<u16> {
    let port = port_from_file(&std::fs::read_to_string(profile.join("DevToolsActivePort")).ok()?)?;
    let connect = tokio::net::TcpStream::connect(("127.0.0.1", port));
    matches!(tokio::time::timeout(Duration::from_secs(2), connect).await, Ok(Ok(_))).then_some(port)
}

/// The Aurelius browser's DevTools port: the running one, else start it (no
/// window until a run opens a tab; the window stays for the user to log in
/// and to watch).
async fn ensure_browser(root: &Path) -> Result<u16, String> {
    // The browser profile Playwright runs use, kept between runs.
    let profile = root.join("browser-profile");
    if let Some(port) = live_port(&profile).await {
        return Ok(port);
    }
    let exe = crate::chrome::find_executable().ok_or("Chrome or Edge is not installed in a known place")?;
    std::fs::create_dir_all(&profile).map_err(|e| format!("could not create {}: {e}", profile.display()))?;
    let _ = std::fs::remove_file(profile.join("DevToolsActivePort"));
    let mut command = Command::new(&exe);
    crate::chrome::no_console(&mut command);
    command
        .arg(format!("--user-data-dir={}", profile.display()))
        .args(["--remote-debugging-port=0", "--remote-debugging-address=127.0.0.1", "--no-first-run", "--no-default-browser-check", "--no-startup-window"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // The handle is dropped on purpose: the browser outlives the run.
    command.spawn().map_err(|e| format!("could not start {}: {e}", exe.display()))?;
    let deadline = tokio::time::Instant::now() + START_TIMEOUT;
    while tokio::time::Instant::now() < deadline {
        tokio::time::sleep(POLL).await;
        if let Some(port) = live_port(&profile).await {
            return Ok(port);
        }
    }
    Err(format!("the browser did not open its DevTools port within {} seconds", START_TIMEOUT.as_secs()))
}

/// What a run needs for the Playwright backend: the package installed and the
/// Aurelius browser up. Gives the stdio MCP server config for the run.
pub async fn prepare_run(data: &Path) -> Result<serde_json::Value, String> {
    let root = root(data);
    let cli = ensure_installed(&root).await?;
    let port = ensure_browser(&root).await?;
    Ok(server_config(&cli, port, &root.join("output")))
}

pub(crate) fn server_config(cli: &Path, port: u16, output: &Path) -> serde_json::Value {
    serde_json::json!({
        "type": "stdio",
        "command": "node",
        "args": [
            cli.to_string_lossy(),
            "--cdp-endpoint", format!("http://127.0.0.1:{port}"),
            "--output-dir", output.to_string_lossy(),
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn devtools_port_file_is_read() {
        assert_eq!(port_from_file("53211\n/devtools/browser/abc\n"), Some(53211));
        assert_eq!(port_from_file("0\n"), None);
        assert_eq!(port_from_file(""), None);
        assert_eq!(port_from_file("x"), None);
    }

    #[test]
    fn server_config_attaches_over_cdp() {
        let config = server_config(Path::new("C:/d/cli.js"), 9333, Path::new("C:/d/out"));
        assert_eq!(config["command"], "node");
        let args: Vec<&str> = config["args"].as_array().unwrap().iter().filter_map(|a| a.as_str()).collect();
        assert_eq!(args, ["C:/d/cli.js", "--cdp-endpoint", "http://127.0.0.1:9333", "--output-dir", "C:/d/out"]);
    }
}
