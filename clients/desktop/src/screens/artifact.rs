//! A Claude Code artifact shown in the side panel: the file the server kept of
//! a publish (`artifact.get_content`), so the user needn't log in to claude.ai.
//! Markdown renders with the message renderer, HTML in an isolated webview,
//! anything else as plain text. Open in browser and Copy link stay.
//!
//! Opened from the artifact card (or a link) in a conversation, for artifacts with `has_content`. The others on
//! claude.ai (Docs, Slides, Design, Dashboards, or published before servers kept
//! files) open live from claude.ai in the same panel, in a webview with its own
//! persistent profile where the user signs in once (Settings › MCP & tools ›
//! claude.ai account). Aurelius never reads that profile's cookies or content.

use crate::app::{AppView, Overlay};
use crate::backend::Request;
use crate::theme::{self, c};
use crate::ui::{self, ButtonKind, h, v};
use aurelius_protocol::{ArtifactContent, ArtifactKind, ArtifactView};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::rc::Rc;

pub struct ArtifactPanel {
    pub url: String,
    /// The list's title until the content comes.
    pub title: String,
    /// The version asked for; `None` follows the newest (and reloads on a republish).
    pub version: Option<u32>,
    pub state: State,
    /// The webview of the shown HTML version, while the panel is visible.
    web: Option<(u32, Rc<html::HtmlView>)>,
    /// The webview could not start (no WebView2, another OS): the source is shown.
    web_failed: Option<String>,
}

pub enum State {
    Loading,
    Loaded(ArtifactContent),
    Failed(String),
    /// Shown live from claude.ai in the claude.ai webview (no kept file).
    Live,
}

/// claude.ai's sign-in page, for Settings › claude.ai account › Sign in.
pub const CLAUDE_LOGIN: &str = "https://claude.ai/login";

pub use claude::ClaudeView;

/// Whether View has something to show: a kept file, or the page on claude.ai.
pub fn can_view(a: &ArtifactView) -> bool {
    a.has_content || claude::is_claude(&a.url)
}

impl ArtifactPanel {
    fn new(url: String, title: String) -> Self {
        Self { url, title, version: None, state: State::Loading, web: None, web_failed: None }
    }

    /// Takes a loaded content if it is the one asked for. Returns whether it was.
    pub fn apply(&mut self, url: &str, version: Option<u32>, result: Result<ArtifactContent, String>) -> bool {
        if self.url != url || self.version != version {
            return false;
        }
        self.state = match result {
            Ok(content) => {
                self.title = content.artifact.title.clone();
                State::Loaded(content)
            }
            Err(e) => State::Failed(e),
        };
        true
    }

    /// The list changed (`artifact.published`): whether the newest shown version is now older.
    pub fn outdated_by(&self, list: &[ArtifactView]) -> bool {
        let Some(newest) = list.iter().find(|a| a.url == self.url) else { return false };
        match &self.state {
            State::Loaded(c) if self.version.is_none() => newest.has_content && newest.version > c.snapshot.as_ref().map_or(c.artifact.version, |s| s.version),
            _ => false,
        }
    }
}

impl AppView {
    /// Open an artifact in the side panel when the server kept its file, else in the browser.
    pub fn open_artifact(&mut self, url: &str, window: &mut Window, cx: &mut Context<Self>) {
        let known = self.artifacts.iter().flatten().find(|a| a.url == url).cloned();
        match known {
            Some(a) if a.has_content => {
                let _ = self.requests_handle().send(Request::LoadArtifactContent { url: a.url.clone(), version: None });
                self.show_artifact_panel(ArtifactPanel::new(a.url, a.title), window, cx);
            }
            known if claude::is_claude(url) => {
                let title = known.map_or_else(|| "claude.ai".to_string(), |a| a.title);
                self.open_claude_ai(url.to_string(), title, window, cx);
            }
            _ => cx.open_url(url),
        }
    }

    /// A claude.ai page in the panel's claude.ai webview (signed in there once).
    pub fn open_claude_ai(&mut self, url: String, title: String, window: &mut Window, cx: &mut Context<Self>) {
        self.show_artifact_panel(ArtifactPanel { state: State::Live, ..ArtifactPanel::new(url, title) }, window, cx);
    }

    fn show_artifact_panel(&mut self, panel: ArtifactPanel, window: &mut Window, cx: &mut Context<Self>) {
        self.artifact = Some(panel);
        self.overlay = Overlay::Artifact;
        self.overlay_focus.focus(window, cx);
        cx.notify();
    }

    /// Settings › claude.ai account › Sign out: clears the claude.ai webview's profile
    /// (cookies, storage, cache). The view stays alive so the clearing can finish.
    pub fn claude_sign_out(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.claude_view(window).and_then(|web| web.sign_out()) {
            Ok(()) => self.notify_user("Signed out of claude.ai in Aurelius", false, cx),
            Err(e) => self.notify_user(format!("Couldn't sign out of claude.ai: {e}"), true, cx),
        }
    }

    /// The one claude.ai webview, made on first use and kept (hidden) for the app's life.
    fn claude_view(&mut self, window: &Window) -> Result<Rc<ClaudeView>, String> {
        if let Some(web) = &self.claude_web {
            return Ok(web.clone());
        }
        let web = Rc::new(ClaudeView::new(window)?);
        self.claude_web = Some(web.clone());
        Ok(web)
    }

    /// Show another kept version (`None`: the newest).
    fn artifact_version(&mut self, version: Option<u32>, cx: &mut Context<Self>) {
        let tx = self.requests_handle().clone();
        let Some(panel) = self.artifact.as_mut() else { return };
        panel.version = version;
        panel.state = State::Loading;
        let _ = tx.send(Request::LoadArtifactContent { url: panel.url.clone(), version });
        cx.notify();
    }

    pub fn reload_artifact(&mut self, cx: &mut Context<Self>) {
        // A live claude.ai page has no kept content to load again.
        let version = self.artifact.as_ref().filter(|p| !matches!(p.state, State::Live)).map(|p| p.version);
        if let Some(version) = version {
            self.artifact_version(version, cx);
        }
    }

    pub fn artifact_loaded(&mut self, url: String, version: Option<u32>, result: Result<ArtifactContent, String>, cx: &mut Context<Self>) {
        if self.artifact.as_mut().is_some_and(|p| p.apply(&url, version, result)) {
            cx.notify();
        }
    }

    /// A republish: an open panel that follows the newest version loads it.
    pub fn artifacts_changed(&mut self, cx: &mut Context<Self>) {
        let outdated = match (self.artifact.as_ref(), self.artifacts.as_ref()) {
            (Some(panel), Some(list)) => panel.outdated_by(list),
            _ => false,
        };
        if outdated {
            self.reload_artifact(cx);
        }
    }

    /// The webview is a native window on top of the app: only while the panel shows.
    pub fn sync_artifact_webview(&mut self) {
        let shown = self.overlay == Overlay::Artifact;
        if !shown {
            if let Some(panel) = self.artifact.as_mut() {
                panel.web = None;
            }
        }
        if !(shown && self.artifact.as_ref().is_some_and(|p| matches!(p.state, State::Live))) {
            if let Some(web) = &self.claude_web {
                web.hide();
            }
        }
    }

    pub(crate) fn render_artifact_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let mono = ui::mono(cx);
        let live = match self.artifact.as_ref() {
            Some(ArtifactPanel { state: State::Live, url, .. }) => Some(url.clone()),
            _ => None,
        };
        // The claude.ai webview shows the panel's page (made on first use).
        let claude_web = live.map(|url| self.claude_view(window).inspect(|web| web.show(&url)));
        let Some(panel) = self.artifact.as_mut() else { return div().into_any_element() };
        let plain = |text: String| div().px(px(28.)).py(px(24.)).text_size(px(13.)).text_color(c(theme::MUTED)).child(text).into_any_element();
        let url = panel.url.clone();

        // The HTML of the shown version gets its webview; another version or kind drops it.
        let page = match &panel.state {
            State::Loaded(ArtifactContent { snapshot: Some(s), .. }) if s.kind == ArtifactKind::Html => Some((s.version, s.content.clone())),
            _ => None,
        };
        match page {
            Some((version, content)) if panel.web.as_ref().is_none_or(|(shown, _)| *shown != version) && panel.web_failed.is_none() => {
                panel.web = None;
                match html::HtmlView::new(window, content) {
                    Ok(view) => panel.web = Some((version, Rc::new(view))),
                    Err(e) => {
                        tracing::warn!(error = %e, "artifact webview");
                        panel.web_failed = Some(e);
                    }
                }
            }
            Some(_) => {}
            None => panel.web = None,
        }

        let (body, note, copy): (AnyElement, String, Option<String>) = match &panel.state {
            State::Loading => (plain("Loading…".into()), String::new(), None),
            State::Live => match claude_web {
                Some(Ok(web)) => (
                    div().flex_1().min_h_0().size_full().bg(c(0xffffff))
                        .child(canvas(move |bounds, _, _| web.place(bounds), |_, _, _, _| {}).size_full())
                        .into_any_element(),
                    "Live from claude.ai · signed in within Aurelius".into(),
                    None,
                ),
                Some(Err(e)) => (plain(format!("claude.ai can't be shown here ({e}). Open it in the browser.")), String::new(), None),
                None => (plain(String::new()), String::new(), None),
            },
            State::Failed(e) => (
                v().px(px(28.)).py(px(24.)).gap(px(6.))
                    .child(div().text_size(px(14.)).font_weight(FontWeight::MEDIUM).child("Couldn't load"))
                    .child(div().text_size(px(13.)).text_color(c(theme::MUTED)).child(e.clone()))
                    .into_any_element(),
                String::new(),
                None,
            ),
            State::Loaded(ArtifactContent { snapshot: None, .. }) => (
                plain("The server kept no copy of this version, so it can only be shown on claude.ai. Open it in the browser.".into()),
                String::new(),
                None,
            ),
            State::Loaded(ArtifactContent { snapshot: Some(s), .. }) => {
                let note = format!("{} · v{} · published {}", s.file_name, s.version, ui::fmt_when(s.published_at_ms));
                let body = match (s.kind, &panel.web, &panel.web_failed) {
                    (ArtifactKind::Html, Some((_, web)), _) => {
                        let web = web.clone();
                        // Keeps the native webview on this element's place, frame by frame
                        // (the panel slides in and can be resized).
                        div().flex_1().min_h_0().size_full().bg(c(0xffffff))
                            .child(canvas(move |bounds, _, _| web.place(bounds), |_, _, _, _| {}).size_full())
                            .into_any_element()
                    }
                    (ArtifactKind::Markdown, ..) => {
                        let handler = link_handler();
                        let view = gpui_kit::component::text::TextView::markdown(SharedString::from(format!("artifact-md-{}-{}", panel.url, s.version)), s.content.clone()).plugin(crate::screens::mermaid::Mermaid)
                            .selectable(true)
                            .on_link_click(handler);
                        v().id("artifact-md").flex_1().min_h_0().overflow_y_scrollbar().px(px(28.)).py(px(18.))
                            .child(div().text_size(px(14.)).line_height(px(22.)).child(view))
                            .into_any_element()
                    }
                    // Text, or HTML without a webview: the source.
                    (kind, _, failed) => v().id("artifact-src").flex_1().min_h_0().overflow_y_scrollbar().px(px(28.)).py(px(18.)).gap(px(10.))
                        .when(kind == ArtifactKind::Html, |el| {
                            el.child(div().text_size(px(12.5)).text_color(c(theme::MUTED)).child(format!(
                                "This page can't be shown here ({}); its source is below. Open it in the browser to see it.",
                                failed.as_deref().unwrap_or("no webview")
                            )))
                        })
                        .child(div().font_family(mono.clone()).text_size(px(12.5)).line_height(px(19.)).whitespace_normal().child(s.content.clone()))
                        .into_any_element(),
                };
                (body, note, Some(s.content.clone()))
            }
        };

        let versions: Vec<u32> = match &panel.state {
            State::Loaded(c) if c.versions.len() > 1 => c.versions.clone(),
            _ => Vec::new(),
        };
        let shown = match &panel.state {
            State::Loaded(c) => c.snapshot.as_ref().map(|s| s.version).or(panel.version),
            _ => panel.version,
        };
        let newest = versions.first().copied();
        let head = v().px(px(28.)).pt(px(14.)).pb(px(12.)).gap(px(8.)).border_b_1().border_color(c(theme::BORDER))
            .child(div().font_family(ui::serif(cx)).text_size(px(20.)).truncate().child(panel.title.clone()))
            .child(div().font_family(mono.clone()).text_size(px(11.5)).text_color(c(theme::MUTED)).truncate().child(url.clone()))
            .when(!versions.is_empty(), |el| {
                el.child(h().flex_wrap().gap(px(4.)).items_center()
                    .child(div().text_size(px(11.5)).text_color(c(theme::MUTED)).mr(px(4.)).child("Version"))
                    .children(versions.iter().map(|&n| {
                        let selected = shown == Some(n);
                        h().id(SharedString::from(format!("artifact-v-{n}"))).px(px(7.)).py(px(2.)).rounded(px(5.)).cursor_pointer().text_size(px(11.5))
                            .border_1().border_color(c(if selected { theme::ACCENT } else { theme::BORDER }))
                            .text_color(c(if selected { theme::TEXT } else { theme::TEXT_2 }))
                            .hover(|s| s.bg(c(theme::SURFACE_2)))
                            .child(format!("v{n}"))
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.artifact_version(if Some(n) == newest { None } else { Some(n) }, cx)))
                    })))
            });
        let (open, link) = (url.clone(), url.clone());
        let foot = h().px(px(28.)).py(px(12.)).gap(px(10.)).border_t_1().border_color(c(theme::BORDER))
            .child(div().flex_1().min_w_0().truncate().text_size(px(12.)).text_color(c(theme::MUTED)).child(note))
            .when_some(copy, |el, raw| {
                el.child(ui::button("artifact-copy-src", "Copy source", ButtonKind::Outline, None).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(raw.clone()));
                    this.notify_user("Copied the source", false, cx);
                })))
            })
            .child(ui::button("artifact-copy-link", "Copy link", ButtonKind::Outline, None).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(link.clone()));
                this.notify_user("Copied the link", false, cx);
            })))
            .child(ui::button("artifact-open", "Open in browser", ButtonKind::Primary, None).on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.open_url(&open))));
        let side = self.side_panel("ARTIFACT", cx)
            .child(head)
            .child(div().flex_1().min_h_0().flex().flex_col().child(body))
            .child(foot);
        self.slide_in(side, ("artifact-scrim", "artifact-slide"), ("artifact_panel", 820.), cx)
    }
}

/// Links in a Markdown artifact leave the app.
fn link_handler() -> impl Fn(&SharedString, &ClickEvent, &mut Window, &mut App) + Send + Sync + 'static {
    |url, _, _, cx| {
        if html::is_external(url) {
            cx.open_url(url);
        }
    }
}

/// The webview for an HTML artifact. Isolated: its own origin (a custom
/// protocol that serves only this page), incognito (no cookies or storage of
/// anything else), no IPC to the app, no connections (CSP `connect-src 'none'`,
/// so no daemon, gateway or local server), no popups; every navigation away
/// from the page opens in the system browser instead.
mod html {
    use gpui_kit::{Bounds, Pixels, Window};

    /// http(s) and mailto: what may go to the system browser.
    pub fn is_external(url: &str) -> bool {
        let lower = url.trim().to_ascii_lowercase();
        lower.starts_with("https://") || lower.starts_with("http://") || lower.starts_with("mailto:")
    }

    #[cfg(windows)]
    pub struct HtmlView {
        web: wry::WebView,
        placed: std::cell::Cell<Option<Bounds<Pixels>>>,
    }

    #[cfg(windows)]
    const SCHEME: &str = "aurelius-artifact";

    /// Whether a navigation stays on the page (its own origin, about:blank, data:).
    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn is_own(uri: &str) -> bool {
        let own = ["http://aurelius-artifact.localhost/", "https://aurelius-artifact.localhost/", "aurelius-artifact://localhost/"];
        own.iter().any(|p| uri.starts_with(p)) || uri.starts_with("about:") || uri.starts_with("data:")
    }

    #[cfg(windows)]
    impl HtmlView {
        pub fn new(window: &Window, content: String) -> Result<Self, String> {
            use std::borrow::Cow;
            let page = std::sync::Arc::new(content.into_bytes());
            // WebView2 needs a writable data folder (not beside an installed exe); incognito keeps nothing in it.
            let mut context = wry::WebContext::new(Some(std::env::temp_dir().join("aurelius-artifact-webview")));
            let web = wry::WebViewBuilder::new_with_web_context(&mut context)
                .with_incognito(true)
                .with_devtools(false)
                .with_clipboard(false)
                .with_focused(false)
                .with_visible(false)
                .with_custom_protocol(SCHEME.into(), move |_, request| {
                    let ok = request.uri().path() == "/" || request.uri().path().is_empty();
                    let builder = wry::http::Response::builder()
                        .header("Content-Security-Policy", "connect-src 'none'; form-action 'none'; frame-ancestors 'none'")
                        .header("Cache-Control", "no-store");
                    let response = if ok {
                        builder.status(200).header("Content-Type", "text/html; charset=utf-8").body(Cow::Owned(page.as_ref().clone()))
                    } else {
                        builder.status(404).body(Cow::Borrowed(&b""[..]))
                    };
                    response.unwrap_or_else(|_| wry::http::Response::new(Cow::Borrowed(&b""[..])))
                })
                .with_navigation_handler(|uri| {
                    if is_own(&uri) {
                        return true;
                    }
                    open_outside(&uri);
                    false
                })
                .with_new_window_req_handler(|uri, _| {
                    open_outside(&uri);
                    wry::NewWindowResponse::Deny
                })
                .with_url(format!("{SCHEME}://localhost/"))
                .build_as_child(window)
                .map_err(|e| e.to_string())?;
            Ok(Self { web, placed: std::cell::Cell::new(None) })
        }

        /// Put the webview on `bounds` (window coordinates, logical pixels) and show it.
        pub fn place(&self, bounds: Bounds<Pixels>) {
            if self.placed.get() == Some(bounds) {
                return;
            }
            let first = self.placed.replace(Some(bounds)).is_none();
            let rect = wry::Rect {
                position: wry::dpi::LogicalPosition::new(f32::from(bounds.origin.x) as f64, f32::from(bounds.origin.y) as f64).into(),
                size: wry::dpi::LogicalSize::new(f32::from(bounds.size.width).max(0.) as f64, f32::from(bounds.size.height).max(0.) as f64).into(),
            };
            let _ = self.web.set_bounds(rect);
            if first {
                let _ = self.web.set_visible(true);
            }
        }
    }

    /// A link the page opened: to the system browser, only http(s) and mailto.
    #[cfg(windows)]
    pub(super) fn open_outside(uri: &str) {
        if is_external(uri) {
            let _ = std::process::Command::new("rundll32").args(["url.dll,FileProtocolHandler", uri]).spawn();
        }
    }

    /// Other systems: no embedded webview yet; the panel shows the source.
    #[cfg(not(windows))]
    pub struct HtmlView;

    #[cfg(not(windows))]
    impl HtmlView {
        pub fn new(_: &Window, _: String) -> Result<Self, String> {
            Err("no webview on this system".into())
        }
        pub fn place(&self, _: Bounds<Pixels>) {}
    }
}

/// The claude.ai webview: a persistent WebView2 profile of its own in the app's
/// data folder (apart from the snapshot webview and the user's browser), so a
/// claude.ai sign-in (Google, Apple, email) is kept across restarts. At top level
/// it stays on claude.ai and the Anthropic, Google and Apple sign-in pages; their
/// popups (Google's sign-in) open as WebView2 windows of the same profile, and
/// every other link goes to the system browser. No IPC, no injected scripts.
mod claude {
    use gpui_kit::{Bounds, Pixels, Window};

    /// The host of an https URL; none with a user part (`https://claude.ai@evil.com`).
    fn host_of(url: &str) -> Option<String> {
        let lower = url.trim().to_ascii_lowercase();
        let authority = lower.strip_prefix("https://")?.split(['/', '\\', '?', '#']).next()?;
        if authority.contains('@') {
            return None;
        }
        authority.split(':').next().filter(|h| !h.is_empty()).map(str::to_string)
    }

    fn on(host: &str, domain: &str) -> bool {
        host == domain || host.strip_suffix(domain).is_some_and(|sub| sub.ends_with('.'))
    }

    /// An https page on claude.ai.
    pub fn is_claude(url: &str) -> bool {
        host_of(url).is_some_and(|h| on(&h, "claude.ai"))
    }

    /// Where the claude.ai webview may go at top level, and which popups it may open.
    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn is_allowed(url: &str) -> bool {
        host_of(url).is_some_and(|h| on(&h, "claude.ai") || on(&h, "anthropic.com") || h == "accounts.google.com" || h == "appleid.apple.com")
    }

    #[cfg(windows)]
    pub struct ClaudeView {
        web: wry::WebView,
        placed: std::cell::Cell<Option<Bounds<Pixels>>>,
        /// The page last asked for (the user may have moved on from it inside the view).
        asked: std::cell::RefCell<String>,
    }

    #[cfg(windows)]
    impl ClaudeView {
        pub fn new(window: &Window) -> Result<Self, String> {
            let dir = aurelius_host::data_dir().map_err(|e| e.to_string())?.join("claude-ai-webview");
            let mut context = wry::WebContext::new(Some(dir));
            let web = wry::WebViewBuilder::new_with_web_context(&mut context)
                .with_devtools(false)
                .with_focused(false)
                .with_visible(false)
                .with_navigation_handler(|uri| {
                    if is_allowed(&uri) || uri.starts_with("about:") {
                        return true;
                    }
                    super::html::open_outside(&uri);
                    false
                })
                .with_new_window_req_handler(|uri, _| {
                    if is_allowed(&uri) {
                        return wry::NewWindowResponse::Allow;
                    }
                    super::html::open_outside(&uri);
                    wry::NewWindowResponse::Deny
                })
                .with_url("about:blank")
                .build_as_child(window)
                .map_err(|e| e.to_string())?;
            Ok(Self { web, placed: std::cell::Cell::new(None), asked: std::cell::RefCell::new(String::new()) })
        }

        /// Load `url` unless it is the page last asked for.
        pub fn show(&self, url: &str) {
            if *self.asked.borrow() != url {
                *self.asked.borrow_mut() = url.to_string();
                let _ = self.web.load_url(url);
            }
        }

        /// Put the webview on `bounds` (window coordinates, logical pixels) and show it.
        pub fn place(&self, bounds: Bounds<Pixels>) {
            if self.placed.get() == Some(bounds) {
                return;
            }
            let first = self.placed.replace(Some(bounds)).is_none();
            let rect = wry::Rect {
                position: wry::dpi::LogicalPosition::new(f32::from(bounds.origin.x) as f64, f32::from(bounds.origin.y) as f64).into(),
                size: wry::dpi::LogicalSize::new(f32::from(bounds.size.width).max(0.) as f64, f32::from(bounds.size.height).max(0.) as f64).into(),
            };
            let _ = self.web.set_bounds(rect);
            if first {
                let _ = self.web.set_visible(true);
            }
        }

        pub fn hide(&self) {
            if self.placed.take().is_some() {
                let _ = self.web.set_visible(false);
            }
        }

        /// Clear the whole profile (cookies, storage, cache); a shown view starts again at the sign-in page.
        pub fn sign_out(&self) -> Result<(), String> {
            self.web.clear_all_browsing_data().map_err(|e| e.to_string())?;
            self.asked.borrow_mut().clear();
            if self.placed.get().is_some() {
                self.show(super::CLAUDE_LOGIN);
            }
            Ok(())
        }
    }

    /// Other systems: no embedded webview yet; the panel says so.
    #[cfg(not(windows))]
    pub struct ClaudeView;

    #[cfg(not(windows))]
    impl ClaudeView {
        pub fn new(_: &Window) -> Result<Self, String> {
            Err("no webview on this system".into())
        }
        pub fn show(&self, _: &str) {}
        pub fn place(&self, _: Bounds<Pixels>) {}
        pub fn hide(&self) {}
        pub fn sign_out(&self) -> Result<(), String> {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ArtifactPanel, State, claude, html};
    use aurelius_protocol::{ArtifactContent, ArtifactKind, ArtifactSnapshot, ArtifactView};

    fn view(version: u32, has_content: bool) -> ArtifactView {
        ArtifactView {
            url: "https://claude.ai/artifact/a".into(),
            title: "Report".into(),
            conversation_id: aurelius_domain::ConversationId::new(),
            task_id: None,
            project_id: None,
            version,
            first_published_at_ms: 0,
            last_published_at_ms: 0,
            has_content,
        }
    }

    fn content(version: u32) -> ArtifactContent {
        ArtifactContent {
            artifact: ArtifactView { title: "Report v".into(), ..view(version, true) },
            versions: (1..=version).rev().collect(),
            snapshot: Some(ArtifactSnapshot { version, kind: ArtifactKind::Html, file_name: "r.html".into(), content: "<p>x</p>".into(), published_at_ms: 0 }),
        }
    }

    #[test]
    fn a_panel_takes_only_what_it_asked_for() {
        let mut panel = ArtifactPanel::new("https://claude.ai/artifact/a".into(), "Report".into());
        assert!(!panel.apply("https://claude.ai/artifact/b", None, Ok(content(1))));
        assert!(!panel.apply("https://claude.ai/artifact/a", Some(1), Ok(content(1))), "another version");
        assert!(panel.apply("https://claude.ai/artifact/a", None, Ok(content(2))));
        assert!(matches!(panel.state, State::Loaded(_)));
        assert_eq!(panel.title, "Report v");
    }

    #[test]
    fn only_a_newer_publish_reloads_a_panel_on_the_newest() {
        let mut panel = ArtifactPanel::new("https://claude.ai/artifact/a".into(), "Report".into());
        panel.apply("https://claude.ai/artifact/a", None, Ok(content(2)));
        assert!(!panel.outdated_by(&[view(2, true)]));
        assert!(panel.outdated_by(&[view(3, true)]));
        assert!(!panel.outdated_by(&[view(3, false)]), "nothing kept to show");
        panel.version = Some(2);
        assert!(!panel.outdated_by(&[view(3, true)]), "an older version stays as picked");
    }

    #[test]
    fn the_page_stays_on_its_own_origin_and_links_go_out() {
        assert!(html::is_own("http://aurelius-artifact.localhost/"));
        assert!(html::is_own("about:blank"));
        assert!(!html::is_own("https://example.com/"));
        assert!(!html::is_own("http://aurelius-artifact.localhost.evil.com/"));
        assert!(html::is_external("https://example.com/x") && html::is_external("mailto:a@b.c"));
        assert!(!html::is_external("file:///C:/Windows/system32/calc.exe") && !html::is_external("javascript:alert(1)"));
    }

    #[test]
    fn the_claude_view_stays_on_claude_and_sign_in_pages() {
        assert!(claude::is_claude("https://claude.ai/artifact/a") && claude::is_claude("https://www.claude.ai/x"));
        assert!(!claude::is_claude("http://claude.ai/") && !claude::is_claude("https://claude.ai.evil.com/") && !claude::is_claude("https://evilclaude.ai/"));
        assert!(!claude::is_claude("https://claude.ai@evil.com/") && claude::is_claude("HTTPS://Claude.AI:443/x"));
        assert!(claude::is_allowed("https://accounts.google.com/o/oauth2/v2/auth?x") && claude::is_allowed("https://console.anthropic.com/"));
        assert!(!claude::is_allowed("https://www.google.com/search?q=x") && !claude::is_allowed("https://example.com/"));
        assert!(super::can_view(&view(1, false)), "a claude.ai page without a kept file opens live");
        assert!(!super::can_view(&ArtifactView { url: "https://example.com/a".into(), ..view(1, false) }));
    }
}
