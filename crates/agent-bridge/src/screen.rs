//! The agent's eyes: screenshots of a monitor or one window, and the list of
//! open windows. Runs inside the agent bridge, so on the computer (and in
//! the desktop session) where the Aurelius server runs. Whether the agent may
//! look is decided before these are called (see `mcp.rs`).

use serde_json::{Value, json};

/// Longest edge and pixel budget sent to the model (larger images are scaled
/// down by Claude anyway; this keeps them small and fast).
const MAX_EDGE: u32 = 1568;
const MAX_PIXELS: u32 = 1_150_000;
const JPEG_QUALITY: u8 = 80;

/// Where screenshots are kept under the data folder; the daemon serves them
/// from here (`screenshot.get`), so it must match the protocol's `SCREENSHOT_DIR`.
const SCREENSHOT_DIR: &str = "screenshots";

/// A screenshot ready for MCP: a caption plus the JPEG.
pub struct Shot {
    pub caption: String,
    pub jpeg: Vec<u8>,
}

/// Size after scaling (width, height) to fit `MAX_EDGE` and `MAX_PIXELS`, never up.
pub fn fit(width: u32, height: u32) -> (u32, u32) {
    if width == 0 || height == 0 {
        return (width, height);
    }
    let (w, h) = (width as f64, height as f64);
    let edge = MAX_EDGE as f64 / w.max(h);
    let area = (MAX_PIXELS as f64 / (w * h)).sqrt();
    let scale = edge.min(area).min(1.0);
    (((w * scale).round() as u32).max(1), ((h * scale).round() as u32).max(1))
}

/// Screenshots kept for the user; the oldest go beyond this many.
const KEEP: usize = 300;

/// Keep a screenshot under the data folder so the user can open it; returns its id.
pub fn keep(root: &std::path::Path, jpeg: &[u8]) -> Option<String> {
    let dir = root.join(SCREENSHOT_DIR);
    std::fs::create_dir_all(&dir).ok()?;
    let id = uuid::Uuid::now_v7().to_string();
    std::fs::write(dir.join(format!("{id}.jpg")), jpeg).ok()?;
    // v7 ids sort by time: drop the oldest beyond KEEP.
    if let Ok(entries) = std::fs::read_dir(&dir) {
        let mut names: Vec<_> = entries.filter_map(|e| e.ok()).map(|e| e.file_name()).filter(|n| n.to_string_lossy().ends_with(".jpg")).collect();
        if names.len() > KEEP {
            names.sort();
            for name in &names[..names.len() - KEEP] {
                let _ = std::fs::remove_file(dir.join(name));
            }
        }
    }
    Some(id)
}

/// The tools' MCP definitions.
pub fn tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "list_windows",
            "description": "Lists the open windows on the user's screen (title, app, size, which one has focus) and the monitors. Use it to pick a window for screenshot.",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "screenshot",
            "description": "Takes a screenshot and shows it to you. Without arguments: the main monitor. `window`: part of a window title or app name (see list_windows); `monitor`: a monitor number from list_windows. The user may be asked first.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "window": { "type": "string", "description": "Part of the window title or app name." },
                    "monitor": { "type": "integer", "minimum": 1, "description": "Monitor number (1 = first)." }
                }
            }
        }),
    ]
}

#[cfg(any(windows, target_os = "macos"))]
mod imp {
    use super::*;
    use xcap::{Monitor, Window};

    fn err(e: impl std::fmt::Display) -> String {
        e.to_string()
    }

    pub fn list_windows() -> Result<String, String> {
        let mut out = String::from("Monitors:\n");
        for (i, m) in Monitor::all().map_err(err)?.iter().enumerate() {
            out.push_str(&format!(
                "  {}. {} {}x{}{}\n",
                i + 1,
                m.name().unwrap_or_default(),
                m.width().unwrap_or(0),
                m.height().unwrap_or(0),
                if m.is_primary().unwrap_or(false) { " (main)" } else { "" }
            ));
        }
        out.push_str("Windows (front to back):\n");
        let mut windows: Vec<Window> = Window::all().map_err(err)?.into_iter().filter(|w| !w.title().unwrap_or_default().trim().is_empty()).collect();
        windows.sort_by_key(|w| std::cmp::Reverse(w.z().unwrap_or(0)));
        for w in windows {
            out.push_str(&format!(
                "  - \"{}\" ({}) {}x{}{}{}\n",
                w.title().unwrap_or_default(),
                w.app_name().unwrap_or_default(),
                w.width().unwrap_or(0),
                w.height().unwrap_or(0),
                if w.is_focused().unwrap_or(false) { ", focused" } else { "" },
                if w.is_minimized().unwrap_or(false) { ", minimized (cannot be captured)" } else { "" },
            ));
        }
        Ok(out)
    }

    pub fn screenshot(window: Option<&str>, monitor: Option<usize>) -> Result<Shot, String> {
        let (image, what) = if let Some(query) = window.map(str::trim).filter(|q| !q.is_empty()) {
            let q = query.to_lowercase();
            let mut matches: Vec<Window> = Window::all()
                .map_err(err)?
                .into_iter()
                .filter(|w| !w.is_minimized().unwrap_or(false))
                .filter(|w| w.title().unwrap_or_default().to_lowercase().contains(&q) || w.app_name().unwrap_or_default().to_lowercase().contains(&q))
                .collect();
            // Frontmost first: the window the user most likely means.
            matches.sort_by_key(|w| std::cmp::Reverse(w.z().unwrap_or(0)));
            let w = matches.into_iter().next().ok_or_else(|| format!("No open (not minimized) window matches \"{query}\". Use list_windows."))?;
            let title = w.title().unwrap_or_default();
            (w.capture_image().map_err(err)?, format!("window \"{title}\""))
        } else {
            let monitors = Monitor::all().map_err(err)?;
            let index = match monitor {
                Some(n) => n.checked_sub(1).filter(|i| *i < monitors.len()).ok_or_else(|| format!("There are {} monitor(s); pick 1 to {}.", monitors.len(), monitors.len()))?,
                None => monitors.iter().position(|m| m.is_primary().unwrap_or(false)).unwrap_or(0),
            };
            let m = monitors.get(index).ok_or("No monitor found.")?;
            (m.capture_image().map_err(err)?, format!("monitor {} of {}", index + 1, monitors.len()))
        };
        let (w, h) = (image.width(), image.height());
        let (tw, th) = fit(w, h);
        let rgb = xcap::image::DynamicImage::ImageRgba8(image).to_rgb8();
        let rgb = if (tw, th) == (w, h) { rgb } else { xcap::image::imageops::resize(&rgb, tw, th, xcap::image::imageops::FilterType::Triangle) };
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, JPEG_QUALITY).encode_image(&rgb).map_err(err)?;
        let caption = if (tw, th) == (w, h) { format!("Screenshot of {what}, {w}x{h}.") } else { format!("Screenshot of {what}, {w}x{h} shown at {tw}x{th}.") };
        Ok(Shot { caption, jpeg })
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod imp {
    use super::*;
    const UNSUPPORTED: &str = "Screenshots are not available on this computer's operating system.";
    pub fn list_windows() -> Result<String, String> {
        Err(UNSUPPORTED.into())
    }
    pub fn screenshot(_window: Option<&str>, _monitor: Option<usize>) -> Result<Shot, String> {
        Err(UNSUPPORTED.into())
    }
}

pub use imp::{list_windows, screenshot};

#[cfg(test)]
mod tests {
    use super::fit;

    #[test]
    fn kept_screenshots_are_found_back_from_the_tool_output() {
        let dir = tempfile::tempdir().unwrap();
        let id = super::keep(dir.path(), b"jpeg").unwrap();
        assert_eq!(std::fs::read(dir.path().join("screenshots").join(format!("{id}.jpg"))).unwrap(), b"jpeg");
        assert_eq!(id.len(), 36, "a UUID, never a path");
    }

    #[test]
    fn screenshots_are_scaled_down_never_up() {
        assert_eq!(fit(800, 600), (800, 600));
        let (w, h) = fit(2560, 1440);
        assert!(w <= 1568 && h <= 1568 && w * h <= 1_150_000 + 2000);
        assert!((w as f64 / h as f64 - 2560.0 / 1440.0).abs() < 0.01, "keeps the aspect ratio");
        let (w, h) = fit(3840, 400);
        assert_eq!(w, 1568);
        assert!(h > 0);
    }
}
