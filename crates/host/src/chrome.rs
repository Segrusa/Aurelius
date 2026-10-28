//! Claude in Chrome needs a running Chrome with its extension. When the user
//! allowed the browser for a run, make sure Chrome is up: if it isn't, start
//! it in the background (`--no-startup-window`, so no window pops up) and
//! wait up to 15 seconds for the process. Never fatal: without Chrome the
//! browser tools simply fail and the run goes on.

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;
use aurelius_protocol::{BrowserChoice, BrowserSetting, ErrorCode, ProtocolError};
use tokio::process::Command;

/// How long to wait for a freshly started Chrome.
pub const START_TIMEOUT: Duration = Duration::from_secs(15);
const POLL: Duration = Duration::from_millis(500);

#[derive(Debug, PartialEq, Eq)]
pub enum ChromeState {
    /// Chrome was already running.
    Running,
    /// Chrome was started now (windowless).
    Started,
    /// Chrome could not be started or did not come up in time.
    Unavailable(String),
}

pub async fn ensure_running() -> ChromeState {
    if is_running().await {
        return ChromeState::Running;
    }
    let Some(exe) = find_executable() else {
        return ChromeState::Unavailable("Chrome or Edge is not installed in a known place".into());
    };
    let mut command = Command::new(&exe);
    no_console(&mut command);
    command.arg("--no-startup-window").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    if let Err(e) = command.spawn() {
        return ChromeState::Unavailable(format!("could not start Chrome: {e}"));
    }
    // The spawned handle is dropped on purpose: Chrome outlives the run.
    let deadline = tokio::time::Instant::now() + START_TIMEOUT;
    while tokio::time::Instant::now() < deadline {
        tokio::time::sleep(POLL).await;
        if is_running().await {
            return ChromeState::Started;
        }
    }
    ChromeState::Unavailable(format!("Chrome did not start within {} seconds", START_TIMEOUT.as_secs()))
}

/// Is a Chrome process running for this user?
pub async fn is_running() -> bool {
    if cfg!(windows) {
        for browser in wanted_browsers() {
            let mut command = Command::new("tasklist");
            no_console(&mut command);
            let out = command.args(["/FI", &format!("IMAGENAME eq {}", browser.image), "/NH", "/FO", "CSV"]).stdin(Stdio::null()).output().await;
            if out.map(|o| listed_in_tasklist(&String::from_utf8_lossy(&o.stdout), browser.image)).unwrap_or(false) {
                return true;
            }
        }
        false
    } else {
        for name in ["Google Chrome", "chrome", "google-chrome", "google-chrome-stable"] {
            let found = Command::new("pgrep").args(["-x", name]).stdin(Stdio::null()).stdout(Stdio::null()).status().await;
            if found.map(|s| s.success()).unwrap_or(false) {
                return true;
            }
        }
        false
    }
}

/// The daemon runs without a console, so every console program it starts (here
/// `tasklist`, polled every 500 ms while Chrome starts) would open and close its own
/// command prompt window. CREATE_NO_WINDOW prevents that.
pub(crate) fn no_console(command: &mut Command) {
    #[cfg(windows)]
    command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    #[cfg(not(windows))]
    let _ = command;
}

/// `tasklist /FO CSV /NH` lists `"chrome.exe","1234",...` per process; with no match
/// it prints an "INFO: No tasks..." line.
pub(crate) fn listed_in_tasklist(output: &str, image: &str) -> bool {
    output.lines().any(|line| line.trim_start().trim_start_matches('"').to_ascii_lowercase().starts_with(&image.to_ascii_lowercase()))
}

/// The Claude in Chrome extension's id in the Chrome Web Store.
const EXTENSION_ID: &str = "fcoeoabgfenejglbffodgkkbkcdhcgfn";

/// A Chromium browser on Windows that can carry the extension (Chrome or Edge).
struct WindowsBrowser {
    image: &'static str,
    /// Install folder under a Program Files / LocalAppData root.
    app: &'static [&'static str],
    /// Profile root under LocalAppData.
    user_data: &'static [&'static str],
}

const WINDOWS_BROWSERS: &[WindowsBrowser] = &[
    WindowsBrowser { image: "chrome.exe", app: &["Google", "Chrome", "Application", "chrome.exe"], user_data: &["Google", "Chrome", "User Data"] },
    WindowsBrowser { image: "msedge.exe", app: &["Microsoft", "Edge", "Application", "msedge.exe"], user_data: &["Microsoft", "Edge", "User Data"] },
];

const SETTINGS_KEY: &str = "browser";
static CHOICE: AtomicU8 = AtomicU8::new(0);

fn choice_code(choice: BrowserChoice) -> u8 {
    match choice {
        BrowserChoice::Automatic => 0,
        BrowserChoice::Chrome => 1,
        BrowserChoice::Edge => 2,
    }
}

fn stored_choice() -> BrowserChoice {
    match CHOICE.load(Ordering::Relaxed) {
        1 => BrowserChoice::Chrome,
        2 => BrowserChoice::Edge,
        _ => BrowserChoice::Automatic,
    }
}

/// `AURELIUS_BROWSER=chrome|edge`, when set, wins over the stored choice.
fn env_override() -> Option<BrowserChoice> {
    match std::env::var("AURELIUS_BROWSER").unwrap_or_default().trim().to_ascii_lowercase().as_str() {
        "chrome" => Some(BrowserChoice::Chrome),
        "edge" => Some(BrowserChoice::Edge),
        _ => None,
    }
}

/// `browser/get`.
pub fn setting() -> BrowserSetting {
    BrowserSetting { choice: stored_choice(), env_override: env_override(), backend: Some(crate::playwright::backend()) }
}

/// Read the stored choice at daemon start (unreadable means automatic).
pub fn load_stored(store: &aurelius_storage::Store) {
    if let Ok(Some(json)) = store.read(|c| aurelius_storage::settings::get(c, SETTINGS_KEY)) {
        if let Ok(choice) = serde_json::from_str::<BrowserChoice>(&json) {
            CHOICE.store(choice_code(choice), Ordering::Relaxed);
        }
    }
    crate::playwright::load_stored(store);
}

/// `browser/set`: keep the choice and use it from the next run on.
pub async fn set_choice(shared: &std::sync::Arc<impl crate::Service>, choice: BrowserChoice) -> Result<BrowserSetting, ProtocolError> {
    let json = serde_json::to_string(&choice).map_err(|e| ProtocolError::new(ErrorCode::Internal, e.to_string()))?;
    let store = shared.store().clone();
    tokio::task::spawn_blocking(move || store.write(move |tx| aurelius_storage::settings::put(tx, SETTINGS_KEY, &json, aurelius_storage::now_ms())))
        .await
        .map_err(|e| ProtocolError::new(ErrorCode::Internal, e.to_string()))?
        .map_err(|e| ProtocolError::new(ErrorCode::Internal, e.to_string()))?;
    CHOICE.store(choice_code(choice), Ordering::Relaxed);
    Ok(setting())
}

fn wanted_browsers() -> impl Iterator<Item = &'static WindowsBrowser> {
    let image = match env_override().unwrap_or_else(stored_choice) {
        BrowserChoice::Chrome => Some("chrome.exe"),
        BrowserChoice::Edge => Some("msedge.exe"),
        BrowserChoice::Automatic => None,
    };
    WINDOWS_BROWSERS.iter().filter(move |b| image.is_none_or(|i| b.image == i))
}

impl WindowsBrowser {
    fn executable(&self) -> Option<PathBuf> {
        [("PROGRAMFILES", r"C:\Program Files"), ("PROGRAMFILES(X86)", r"C:\Program Files (x86)"), ("LOCALAPPDATA", "")]
            .iter()
            .filter_map(|(var, default)| std::env::var_os(var).or_else(|| (!default.is_empty()).then(|| default.into())))
            .map(|base| self.app.iter().fold(PathBuf::from(base), |p, part| p.join(part))).find(|p| p.is_file())
    }

    /// Is the extension installed in any profile of this browser?
    fn has_extension(&self) -> bool {
        let Some(base) = std::env::var_os("LOCALAPPDATA") else { return false };
        let root = self.user_data.iter().fold(PathBuf::from(base), |p, part| p.join(part));
        std::fs::read_dir(root).map(|entries| entries.flatten().any(|e| e.path().join("Extensions").join(EXTENSION_ID).is_dir())).unwrap_or(false)
    }
}

/// Where the browser with the extension lives on this computer: on Windows the
/// one that has the extension (Chrome or Edge), else any installed one.
pub fn find_executable() -> Option<PathBuf> {
    if cfg!(windows) {
        let installed: Vec<(&WindowsBrowser, PathBuf)> = wanted_browsers().filter_map(|b| b.executable().map(|e| (b, e))).collect();
        let chosen = installed.iter().find(|(b, _)| b.has_extension()).or_else(|| installed.first());
        return chosen.map(|(_, e)| e.clone());
    }
    candidates().into_iter().find(|p| p.is_file())
}

fn candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if cfg!(target_os = "macos") {
        out.push(PathBuf::from("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"));
    } else {
        if let Some(paths) = std::env::var_os("PATH") {
            for dir in std::env::split_paths(&paths) {
                for name in ["google-chrome", "google-chrome-stable", "chromium"] {
                    out.push(dir.join(name));
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tasklist_output_is_read_per_process() {
        assert!(listed_in_tasklist("\"chrome.exe\",\"1234\",\"Console\",\"1\",\"80,000 K\"\r\n", "chrome.exe"));
        assert!(listed_in_tasklist("\"Chrome.exe\",\"1\",\"Console\"\n\"chrome.exe\",\"2\",\"Console\"\n", "chrome.exe"));
        assert!(!listed_in_tasklist("INFO: No tasks are running which match the specified criteria.\r\n", "chrome.exe"));
        assert!(!listed_in_tasklist("\"chromedriver.exe\",\"7\",\"Console\"\n", "chrome.exe"), "only the exact image counts");
        assert!(!listed_in_tasklist("", "chrome.exe"));
        assert!(listed_in_tasklist("\"msedge.exe\",\"9\",\"Console\",\"1\",\"80,000 K\"
", "msedge.exe"));
    }
}
