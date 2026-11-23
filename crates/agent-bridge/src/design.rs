//! The eyes of a design session: `view_design` renders one of the session's own HTML pages in a
//! headless Chromium and shows the screenshot to the agent, so it sees what the board member
//! sees instead of guessing from the source. It asks nobody: it only shows a file in the run's own
//! work folder, never the user's screen.
//!
//! The page gets what the desktop's artifact viewer gives it and nothing more: it is served from
//! memory on loopback with the artifact policy (inline styles and scripts, data: images and fonts,
//! no requests, no frames), to a browser without a profile whose proxy goes nowhere. Served over
//! http, it cannot navigate to a `file:` URL, so it cannot show the agent files outside its folder.

use serde_json::{Value, json};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// The width a page is shown at when the agent names none: the desktop viewer's Desktop width.
pub const VIEW_WIDTH_DEFAULT: u32 = 1280;
/// The height a page is shown at when the agent names none: one screen.
pub const VIEW_HEIGHT_DEFAULT: u32 = 800;
/// The narrowest and widest width: a small phone up to a wide desktop.
pub const VIEW_WIDTH_MIN: u32 = 320;
pub const VIEW_WIDTH_MAX: u32 = 2560;
/// The lowest and highest height. Taller shots are scaled down more, so text gets hard to read.
pub const VIEW_HEIGHT_MIN: u32 = 200;
pub const VIEW_HEIGHT_MAX: u32 = 4000;
/// The largest page it renders (the desktop viewer takes 25 MB; a design page is far smaller).
pub const DESIGN_BYTES_MAX: u64 = 10 * 1024 * 1024;
/// How long the page's scripts and fonts get before the shot, in the browser's virtual time.
const SETTLE_MS: u32 = 1500;
/// How long a render may take before the browser is stopped.
const RENDER_TIMEOUT_MS: u64 = 30_000;
/// The longest request the loopback server reads (the browser's GET with its headers).
const REQUEST_BYTES_MAX: usize = 16 * 1024;

/// What the page may do: the desktop artifact viewer's policy, with inline scripts allowed.
pub const DESIGN_CSP: &str = "sandbox allow-scripts; default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data: blob:; font-src data:; media-src data: blob:; connect-src 'none'; form-action 'none'; base-uri 'none'; frame-src 'none'; object-src 'none'";

/// The tool's MCP definition (only in a design session).
pub fn tool_definition() -> Value {
    json!({
        "name": "view_design",
        "description": "Renders one of your design pages (an HTML file in your work folder) in a headless browser and shows you the screenshot, as the board member's viewer shows it. Use it after you write or change a page, check it, and fix what looks wrong before you answer; look at a phone width (390) too. No question to the user is needed. The page gets no network and no other files, like in the viewer.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "The page: a path in your work folder, e.g. design/pricing.html." },
                "width": { "type": "integer", "minimum": VIEW_WIDTH_MIN, "maximum": VIEW_WIDTH_MAX, "description": "Viewport width in CSS pixels; default 1280 (desktop). 768 is a tablet, 390 a phone." },
                "height": { "type": "integer", "minimum": VIEW_HEIGHT_MIN, "maximum": VIEW_HEIGHT_MAX, "description": "Viewport height; default 800. The shot shows this much of the page from the top; a taller one is scaled down more." }
            },
            "required": ["path"]
        }
    })
}

/// The page `raw` names, when it is an HTML file inside `folder`; otherwise what is wrong.
pub fn design_file(folder: &Path, raw: &str) -> Result<PathBuf, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err("Name the page: a path in your work folder, e.g. design/pricing.html.".into());
    }
    let folder = folder.canonicalize().map_err(|_| "Your work folder is not there.".to_string())?;
    let asked = Path::new(raw);
    let joined = if asked.is_absolute() { asked.to_path_buf() } else { folder.join(asked) };
    let file = joined.canonicalize().map_err(|_| format!("There is no file {raw} in your work folder."))?;
    if !file.starts_with(&folder) {
        return Err(format!("{raw} is outside your work folder; view_design shows only your own pages."));
    }
    let html = file
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("html") || e.eq_ignore_ascii_case("htm"));
    if !html || !file.is_file() {
        return Err(format!("{raw} is not an HTML page (.html or .htm)."));
    }
    Ok(file)
}

/// A width and height the agent asked for, within the limits.
pub fn viewport(width: Option<u64>, height: Option<u64>) -> (u32, u32) {
    let clamp = |v: Option<u64>, default: u32, min: u32, max: u32| v.map_or(default, |v| v.clamp(u64::from(min), u64::from(max)) as u32);
    (
        clamp(width, VIEW_WIDTH_DEFAULT, VIEW_WIDTH_MIN, VIEW_WIDTH_MAX),
        clamp(height, VIEW_HEIGHT_DEFAULT, VIEW_HEIGHT_MIN, VIEW_HEIGHT_MAX),
    )
}

/// The Chromium-family browser to render with: `AURELIUS_DESIGN_BROWSER`, else one on the PATH,
/// else one in its usual place, else Playwright's headless shell.
pub fn find_browser() -> Option<PathBuf> {
    if let Some(set) = std::env::var_os("AURELIUS_DESIGN_BROWSER").map(PathBuf::from).filter(|p| p.is_file()) {
        return Some(set);
    }
    const NAMES: &[&str] = &["chromium", "chromium-browser", "google-chrome", "google-chrome-stable", "microsoft-edge", "chrome", "msedge"];
    let exe = |name: &str| if cfg!(windows) { format!("{name}.exe") } else { name.to_owned() };
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            if let Some(found) = NAMES.iter().map(|n| dir.join(exe(n))).find(|p| p.is_file()) {
                return Some(found);
            }
        }
    }
    let fixed: Vec<PathBuf> = if cfg!(target_os = "macos") {
        [
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            "/Applications/Chromium.app/Contents/MacOS/Chromium",
            "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
        ]
        .iter()
        .map(PathBuf::from)
        .collect()
    } else if cfg!(windows) {
        let mut roots: Vec<PathBuf> = ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"].iter().filter_map(std::env::var_os).map(PathBuf::from).collect();
        roots.dedup();
        roots
            .iter()
            .flat_map(|r| [r.join("Google/Chrome/Application/chrome.exe"), r.join("Microsoft/Edge/Application/msedge.exe")])
            .collect()
    } else {
        ["/usr/bin/chromium", "/usr/bin/google-chrome", "/snap/bin/chromium"].iter().map(PathBuf::from).collect()
    };
    fixed.into_iter().find(|p| p.is_file()).or_else(playwright_shell)
}

/// The newest headless shell Playwright installed, if any.
fn playwright_shell() -> Option<PathBuf> {
    let cache = if let Some(set) = std::env::var_os("PLAYWRIGHT_BROWSERS_PATH") {
        PathBuf::from(set)
    } else if cfg!(windows) {
        PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("ms-playwright")
    } else if cfg!(target_os = "macos") {
        PathBuf::from(std::env::var_os("HOME")?).join("Library/Caches/ms-playwright")
    } else {
        PathBuf::from(std::env::var_os("HOME")?).join(".cache/ms-playwright")
    };
    let mut shells: Vec<PathBuf> = std::fs::read_dir(cache)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("chromium_headless_shell-")))
        .collect();
    shells.sort();
    shells.into_iter().rev().find_map(|dir| {
        [
            "chrome-linux/headless_shell",
            "chrome-headless-shell-linux64/chrome-headless-shell",
            "chrome-mac/headless_shell",
            "chrome-headless-shell-mac-arm64/chrome-headless-shell",
            "chrome-headless-shell-mac-x64/chrome-headless-shell",
            "chrome-win/headless_shell.exe",
            "chrome-headless-shell-win64/chrome-headless-shell.exe",
        ]
        .iter()
        .map(|rel| dir.join(rel))
        .find(|p| p.is_file())
    })
}

/// Serves one page from memory on loopback, at a path nobody can guess, until it is dropped.
struct OnePage {
    url: String,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    addr: std::net::SocketAddr,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl OnePage {
    fn serve(html: Vec<u8>) -> std::io::Result<Self> {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0))?;
        let addr = listener.local_addr()?;
        // v7 carries 74 random bits: enough that no other local program guesses the path.
        let token = uuid::Uuid::now_v7().simple().to_string();
        let url = format!("http://{addr}/{token}");
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stopped = stop.clone();
        let thread = std::thread::spawn(move || {
            for stream in listener.incoming() {
                if stopped.load(std::sync::atomic::Ordering::SeqCst) {
                    break;
                }
                if let Ok(mut stream) = stream {
                    let _ = answer(&mut stream, &token, &html);
                }
            }
        });
        Ok(Self {
            url,
            stop,
            addr,
            thread: Some(thread),
        })
    }
}

impl Drop for OnePage {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::SeqCst);
        // Wake the accept loop so it sees the stop.
        let _ = std::net::TcpStream::connect_timeout(&self.addr, Duration::from_millis(200));
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// One request: the page at its own path with the artifact policy, 404 for anything else.
fn answer(stream: &mut std::net::TcpStream, token: &str, html: &[u8]) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    let mut request = Vec::new();
    let mut buf = [0u8; 2048];
    while !request.windows(4).any(|w| w == b"\r\n\r\n") && request.len() < REQUEST_BYTES_MAX {
        let n = stream.read(&mut buf)?;
        if n == 0 {
            break;
        }
        request.extend_from_slice(&buf[..n]);
    }
    let line = String::from_utf8_lossy(&request);
    let ours = line
        .lines()
        .next()
        .is_some_and(|first| first.starts_with(&format!("GET /{token} ")) || first.starts_with(&format!("GET /{token}?")));
    let (status, body): (&str, &[u8]) = if ours { ("200 OK", html) } else { ("404 Not Found", b"") };
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nContent-Security-Policy: {DESIGN_CSP}\r\nX-Content-Type-Options: nosniff\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()
}

/// The browser's arguments for one shot of `url` at `width`×`height` into `shot`.
pub fn browser_args(profile: &Path, shot: &Path, url: &str, width: u32, height: u32) -> Vec<String> {
    vec![
        "--headless".into(),
        "--disable-gpu".into(),
        "--hide-scrollbars".into(),
        "--no-first-run".into(),
        "--no-default-browser-check".into(),
        "--disable-extensions".into(),
        "--disable-sync".into(),
        "--disable-background-networking".into(),
        "--mute-audio".into(),
        format!("--user-data-dir={}", profile.display()),
        // Everything but loopback (where the page is) goes to a proxy that is not there.
        "--proxy-server=http://127.0.0.1:9".into(),
        "--force-webrtc-ip-handling-policy=disable_non_proxied_udp".into(),
        format!("--window-size={width},{height}"),
        format!("--virtual-time-budget={SETTLE_MS}"),
        format!("--screenshot={}", shot.display()),
        url.into(),
    ]
}

/// Renders `file` at `width`×`height` and gives the shot as a JPEG with its caption.
pub fn render(file: &Path, shown_as: &str, width: u32, height: u32) -> Result<crate::screen::Shot, String> {
    let meta = std::fs::metadata(file).map_err(|e| e.to_string())?;
    if meta.len() > DESIGN_BYTES_MAX {
        return Err(format!("{shown_as} is larger than {} MB.", DESIGN_BYTES_MAX / (1024 * 1024)));
    }
    let browser =
        find_browser().ok_or("No Chromium, Chrome or Edge was found on this computer to render the page with. Ask the board member to install one (or set AURELIUS_DESIGN_BROWSER to its program).")?;
    let html = std::fs::read(file).map_err(|e| e.to_string())?;
    let page = OnePage::serve(html).map_err(|e| format!("Could not serve the page: {e}"))?;
    let work = tempfile::tempdir().map_err(|e| e.to_string())?;
    let shot = work.path().join("shot.png");
    let mut child = std::process::Command::new(&browser)
        .args(browser_args(&work.path().join("profile"), &shot, &page.url, width, height))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("Could not start {}: {e}", browser.display()))?;
    let started = Instant::now();
    loop {
        if child.try_wait().map_err(|e| e.to_string())?.is_some() {
            break;
        }
        if started.elapsed() > Duration::from_millis(RENDER_TIMEOUT_MS) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("The page did not render within {} s.", RENDER_TIMEOUT_MS / 1000));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    drop(page);
    let png = std::fs::read(&shot).map_err(|_| "The browser made no screenshot of the page.".to_string())?;
    let image = image::load_from_memory(&png).map_err(|e| e.to_string())?.to_rgb8();
    let (w, h) = (image.width(), image.height());
    let (tw, th) = crate::screen::fit(w, h);
    let image = if (tw, th) == (w, h) {
        image
    } else {
        image::imageops::resize(&image, tw, th, image::imageops::FilterType::Triangle)
    };
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, crate::screen::JPEG_QUALITY)
        .encode_image(&image)
        .map_err(|e| e.to_string())?;
    let scaled = if (tw, th) == (w, h) { String::new() } else { format!(", shown at {tw}x{th}") };
    Ok(crate::screen::Shot {
        caption: format!("{shown_as} rendered at {width}x{height} CSS pixels{scaled}."),
        jpeg,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_an_html_page_inside_the_work_folder_is_rendered() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("work");
        std::fs::create_dir_all(folder.join("design")).unwrap();
        std::fs::write(folder.join("design/pricing.html"), "<h1>Plans</h1>").unwrap();
        std::fs::write(folder.join("notes.md"), "notes").unwrap();
        std::fs::write(dir.path().join("secret.html"), "<h1>no</h1>").unwrap();
        assert!(design_file(&folder, "design/pricing.html").unwrap().ends_with("design/pricing.html"));
        assert!(design_file(&folder, folder.join("design/pricing.html").to_str().unwrap()).is_ok(), "an absolute path inside the folder");
        assert!(design_file(&folder, "../secret.html").unwrap_err().contains("outside"));
        assert!(design_file(&folder, dir.path().join("secret.html").to_str().unwrap()).unwrap_err().contains("outside"));
        assert!(design_file(&folder, "notes.md").unwrap_err().contains("not an HTML page"));
        assert!(design_file(&folder, "design/missing.html").unwrap_err().contains("no file"));
        assert!(design_file(&folder, " ").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn a_link_out_of_the_work_folder_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("work");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(dir.path().join("secret.html"), "<h1>no</h1>").unwrap();
        std::os::unix::fs::symlink(dir.path().join("secret.html"), folder.join("page.html")).unwrap();
        assert!(design_file(&folder, "page.html").unwrap_err().contains("outside"));
    }

    #[test]
    fn the_viewport_stays_within_its_limits() {
        assert_eq!(viewport(None, None), (VIEW_WIDTH_DEFAULT, VIEW_HEIGHT_DEFAULT));
        assert_eq!(viewport(Some(390), Some(844)), (390, 844));
        assert_eq!(viewport(Some(1), Some(u64::MAX)), (VIEW_WIDTH_MIN, VIEW_HEIGHT_MAX));
    }

    #[test]
    fn the_browser_gets_no_network_and_no_profile_of_the_user() {
        let args = browser_args(Path::new("/tmp/p"), Path::new("/tmp/s.png"), "http://127.0.0.1:1/x", 390, 844);
        assert!(args.contains(&"--proxy-server=http://127.0.0.1:9".to_string()));
        assert!(args.contains(&"--user-data-dir=/tmp/p".to_string()));
        assert!(args.contains(&"--window-size=390,844".to_string()));
        assert_eq!(args.last().map(String::as_str), Some("http://127.0.0.1:1/x"), "the page is the last argument");
    }

    #[test]
    fn the_loopback_server_gives_the_page_only_at_its_own_path_with_the_policy() {
        let page = OnePage::serve(b"<h1>Plans</h1>".to_vec()).unwrap();
        let get = |path: &str| {
            let mut s = std::net::TcpStream::connect(page.addr).unwrap();
            s.write_all(format!("GET {path} HTTP/1.1\r\nHost: x\r\n\r\n").as_bytes()).unwrap();
            let mut out = String::new();
            s.read_to_string(&mut out).unwrap();
            out
        };
        let path = page.url.trim_start_matches(&format!("http://{}", page.addr)).to_string();
        let ok = get(&path);
        assert!(ok.starts_with("HTTP/1.1 200"), "{ok}");
        assert!(ok.contains(&format!("Content-Security-Policy: {DESIGN_CSP}")));
        assert!(ok.ends_with("<h1>Plans</h1>"));
        let other = get("/etc/passwd");
        assert!(other.starts_with("HTTP/1.1 404") && !other.contains("Plans"));
        drop(page);
    }

    /// Renders a real page when a browser is installed (skipped otherwise): the shot is the page.
    #[test]
    fn a_page_renders_to_a_jpeg_of_the_asked_width_when_a_browser_is_there() {
        if find_browser().is_none() {
            eprintln!("no browser: skipped");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("page.html");
        std::fs::write(&file, "<!doctype html><title>T</title><style>body{margin:0;background:#15764c}</style><h1>Plans</h1>").unwrap();
        let shot = render(&file, "page.html", 390, 300).unwrap();
        let image = image::load_from_memory(&shot.jpeg).unwrap().to_rgb8();
        assert_eq!((image.width(), image.height()), (390, 300));
        let px = image.get_pixel(380, 290);
        assert!(px[1] > px[0] + 40 && px[1] > px[2] + 40, "the page's green background, got {px:?}");
        assert!(shot.caption.contains("390x300"));
    }

    /// A page cannot show the agent a file outside its folder by going there (skipped without a browser).
    #[test]
    fn a_page_that_navigates_to_a_local_file_does_not_show_it() {
        if find_browser().is_none() {
            eprintln!("no browser: skipped");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let outside = dir.path().join("outside.html");
        std::fs::write(&outside, "<style>html,body{margin:0;height:100%;background:#ff0000}</style>").unwrap();
        let url = format!("file://{}", outside.display().to_string().replace('\\', "/"));
        let file = dir.path().join("page.html");
        let page = format!(
            "<!doctype html><style>body{{margin:0;background:#ffffff}}</style><iframe src=\"{url}\" style=\"border:0;width:100%;height:300px\"></iframe><script>location.href = \"{url}\";</script><meta http-equiv=\"refresh\" content=\"0;url={url}\">"
        );
        std::fs::write(&file, page).unwrap();
        let shot = render(&file, "page.html", 390, 300).unwrap();
        let image = image::load_from_memory(&shot.jpeg).unwrap().to_rgb8();
        let red = image.pixels().filter(|p| p[0] > 200 && p[1] < 80 && p[2] < 80).count();
        assert_eq!(red, 0, "the outside file showed in the shot");
    }
}
