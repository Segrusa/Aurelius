//! The Markdown viewer, in the side panel: one Markdown file of a work folder
//! (a project's, a conversation's or a task's) rendered with the same Markdown
//! renderer as the messages. Needs protocol 1.16; the server confines every path
//! to the work folder.
//!
//! Opened from a `.md` link in a message or from a file chip. A relative link
//! inside a document opens that document in the same panel.

use crate::app::{AppView, Overlay};
use crate::backend::{MarkdownFailure, Request};
use crate::keymap::Back;
use crate::theme::{self, c};
use crate::ui::{self, ButtonKind, h, v};
use aurelius_protocol::{FileScope, MarkdownFileContent};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

/// The side panel: one document (`doc`) opened from a link in a message, with the
/// documents it was reached from (`back`).
pub struct MarkdownPanel {
    pub scope: FileScope,
    /// What the scope is called ("Aurelius", a task's title).
    pub title: String,
    pub doc: Doc,
    /// Documents left through a relative link, most recent last.
    pub back: Vec<String>,
}

pub struct Doc {
    /// The path as asked (relative or absolute).
    pub asked: String,
    pub state: DocState,
}

pub enum DocState {
    Loading,
    Loaded(MarkdownFileContent),
    Failed(MarkdownFailure),
}

impl MarkdownPanel {
    /// The panel's document path, as the server reported it once loaded.
    fn current_path(&self) -> Option<String> {
        Some(match &self.doc.state {
            DocState::Loaded(f) => f.path.clone(),
            _ => self.doc.asked.clone(),
        })
    }

    /// Takes a loaded document if it is the one asked for. Returns whether it was.
    pub fn apply_doc(&mut self, scope: &FileScope, asked: &str, result: Result<MarkdownFileContent, MarkdownFailure>) -> bool {
        if &self.scope != scope {
            return false;
        }
        if self.doc.asked != asked {
            return false;
        }
        self.doc.state = match result {
            Ok(file) => DocState::Loaded(file),
            Err(e) => DocState::Failed(e),
        };
        true
    }
}

impl AppView {
    /// Open one Markdown file (relative or absolute inside the work folder) of a scope.
    pub fn open_markdown(&mut self, scope: FileScope, title: String, path: String, window: &mut Window, cx: &mut Context<Self>) {
        let _ = self.requests_handle().send(Request::LoadMarkdownFile(scope.clone(), path.clone()));
        self.markdown = Some(MarkdownPanel { scope, title, doc: Doc { asked: path, state: DocState::Loading }, back: Vec::new() });
        self.show_markdown(window, cx);
    }

    fn show_markdown(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.overlay = Overlay::Markdown;
        self.overlay_focus.focus(window, cx);
        cx.notify();
    }

    /// Follow a relative link inside the open document.
    fn markdown_goto(&mut self, path: String, cx: &mut Context<Self>) {
        let tx = self.requests_handle().clone();
        let Some(panel) = self.markdown.as_mut() else { return };
        if let Some(current) = panel.current_path() {
            panel.back.push(current);
        }
        let _ = tx.send(Request::LoadMarkdownFile(panel.scope.clone(), path.clone()));
        panel.doc = Doc { asked: path, state: DocState::Loading };
        cx.notify();
    }

    /// Back to the previous document; from the first one the panel closes.
    fn markdown_back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let tx = self.requests_handle().clone();
        let Some(panel) = self.markdown.as_mut() else { return };
        match panel.back.pop() {
            Some(path) => {
                let _ = tx.send(Request::LoadMarkdownFile(panel.scope.clone(), path.clone()));
                panel.doc = Doc { asked: path, state: DocState::Loading };
                cx.notify();
            }
            None => window.dispatch_action(Box::new(Back), cx),
        }
    }

    /// Load the open document again.
    pub fn reload_markdown(&mut self, cx: &mut Context<Self>) {
        let tx = self.requests_handle().clone();
        let Some(panel) = self.markdown.as_mut() else { return };
        let _ = tx.send(Request::LoadMarkdownFile(panel.scope.clone(), panel.doc.asked.clone()));
        panel.doc.state = DocState::Loading;
        cx.notify();
    }

    pub fn markdown_loaded(&mut self, scope: FileScope, asked: String, result: Result<MarkdownFileContent, MarkdownFailure>, cx: &mut Context<Self>) {
        if let Some(panel) = self.markdown.as_mut() {
            if panel.apply_doc(&scope, &asked, result) {
                cx.notify();
            }
        }
    }

    /// Whose work folder the open conversation's files are in: the task's own
    /// workspace for a task conversation, else the conversation's.
    pub fn message_scope(&self) -> Option<(FileScope, String)> {
        let conversation = self.chip_conversation()?;
        if let Some(t) = self.tasks.iter().find(|t| t.conversation_id == Some(conversation)) {
            return Some((FileScope::Task { task_id: t.id }, t.title.clone()));
        }
        Some((FileScope::Conversation { conversation_id: conversation }, "This conversation".into()))
    }

    /// A link was clicked: `in_viewer` inside the open document (relative to it),
    /// else in a message (relative to the work folder).
    pub fn markdown_link(&mut self, url: &str, in_viewer: bool, window: &mut Window, cx: &mut Context<Self>) {
        let base = if in_viewer { self.markdown.as_ref().and_then(|p| p.current_path()) } else { None };
        match resolve_link(base.as_deref(), url) {
            Some(path) if in_viewer => self.markdown_goto(path, cx),
            Some(path) => match self.message_scope() {
                Some((scope, title)) => self.open_markdown(scope, title, path, window, cx),
                None => self.notify_user("Open a conversation first: the document is in its work folder.", true, cx),
            },
            None if is_web(url) => cx.open_url(url.trim()),
            None if url.starts_with('#') => {}
            // Model-written text: other schemes would start whatever app claims them.
            None if is_external(url) => self.notify_user("Only web and mail links open from here.", true, cx),
            None => self.notify_user("That link is not a Markdown file.", true, cx),
        }
    }

    /// Click handler for links of rendered Markdown (messages: `in_viewer` false).
    pub fn markdown_link_handler(&self, in_viewer: bool, cx: &mut Context<Self>) -> impl Fn(&SharedString, &ClickEvent, &mut Window, &mut App) + Send + Sync + 'static {
        let weak = cx.weak_entity();
        move |url, _, window, cx| {
            let (weak, url) = (weak.clone(), url.to_string());
            window.defer(cx, move |window, cx| {
                let _ = weak.update(cx, |this, cx| this.markdown_link(&url, in_viewer, window, cx));
            });
        }
    }

    /// Chips for the Markdown files an answer names; a click opens the viewer.
    pub fn markdown_chips(&self, id: impl std::fmt::Display, text: &str, cx: &mut Context<Self>) -> Option<AnyElement> {
        let paths = memo(&PATHS, text, markdown_paths);
        if paths.is_empty() {
            return None;
        }
        Some(
            h().flex_wrap().gap(px(6.)).pt(px(2.))
                .children(paths.into_iter().enumerate().map(|(i, path)| {
                    let open = path.clone();
                    h().id(SharedString::from(format!("md-chip-{id}-{i}"))).gap(px(6.)).px(px(9.)).py(px(4.)).rounded(px(6.)).cursor_pointer()
                        .border_1().border_color(c(theme::BORDER)).bg(c(theme::SURFACE)).text_size(px(12.))
                        .hover(|s| s.border_color(c(theme::BORDER_STRONG)))
                        .child(div().text_color(c(theme::ACCENT_TEXT)).child("▤"))
                        .child(path)
                        .child(div().text_color(c(theme::ACCENT_TEXT)).child("View"))
                        .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.markdown_link(&open, false, w, cx)))
                }))
                .into_any_element(),
        )
    }

    pub(crate) fn render_markdown_panel(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let mono = ui::mono(cx);
        let Some(panel) = self.markdown.as_ref() else { return div().into_any_element() };
        let message = |headline: &str, text: String| {
            v().px(px(28.)).py(px(24.)).gap(px(6.))
                .child(div().text_size(px(14.)).font_weight(FontWeight::MEDIUM).child(headline.to_string()))
                .child(div().text_size(px(13.)).text_color(c(theme::MUTED)).child(text))
                .into_any_element()
        };
        let plain = |text: &str| div().px(px(28.)).py(px(24.)).text_size(px(13.)).text_color(c(theme::MUTED)).child(text.to_string()).into_any_element();

        let (head, body, foot): (AnyElement, AnyElement, AnyElement) = {
            {
                let doc = &panel.doc;
                let back_label = match panel.back.last() {
                    Some(p) => format!("← {p}"),
                    None => "← Close".to_string(),
                };
                let shown = panel.current_path().unwrap_or_default();
                let head = v().px(px(28.)).pt(px(14.)).pb(px(12.)).gap(px(6.)).border_b_1().border_color(c(theme::BORDER))
                    .child(
                        h().id("md-back").self_start().max_w_full().px(px(6.)).py(px(2.)).rounded(px(5.)).cursor_pointer().text_size(px(12.)).text_color(c(theme::MUTED))
                            .hover(|s| s.bg(c(theme::SURFACE_2)).text_color(c(theme::TEXT)))
                            .on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.markdown_back(w, cx)))
                            .child(div().truncate().child(back_label)),
                    )
                    .child(div().font_family(mono.clone()).text_size(px(13.)).truncate().child(shown))
                    .child(div().text_size(px(11.5)).text_color(c(theme::MUTED)).child(panel.title.clone()))
                    .into_any_element();
                let (body, copy): (AnyElement, Option<String>) = match &doc.state {
                    DocState::Loading => (plain("Loading…"), None),
                    DocState::Failed(e) => (message(e.headline(), e.detail()), None),
                    DocState::Loaded(file) => {
                        let handler = self.markdown_link_handler(true, cx);
                        let view = gpui_kit::component::text::TextView::markdown(
                            SharedString::from(format!("md-viewer-{}-{}-{}", file.path, file.modified_ms, file.byte_length)),
                            file.text.clone(),
                        )
                        .plugin(crate::screens::mermaid::Mermaid)
                        .selectable(true)
                        .on_link_click(handler);
                        let body = v().id("md-body").flex_1().min_h_0().overflow_y_scrollbar().px(px(28.)).py(px(18.))
                            .child(div().text_size(px(14.)).line_height(px(22.)).child(view))
                            .into_any_element();
                        (body, Some(file.text.clone()))
                    }
                };
                let note = match &doc.state {
                    DocState::Loaded(f) => format!("{} · changed {}", crate::app::file_size(f.byte_length), stamp(f.modified_ms)),
                    _ => String::new(),
                };
                let foot = h().px(px(28.)).py(px(12.)).gap(px(10.)).border_t_1().border_color(c(theme::BORDER))
                    .child(div().flex_1().min_w_0().truncate().text_size(px(12.)).text_color(c(theme::MUTED)).child(note))
                    .when_some(copy, |el, raw| {
                        el.child(ui::button("md-copy", "Copy raw", ButtonKind::Outline, None).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(raw.clone()));
                            this.notify_user("Copied the Markdown", false, cx);
                        })))
                    })
                    .child(ui::button("md-reload", "Reload", ButtonKind::Outline, Some(ui::kbd("enter", cx))).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.reload_markdown(cx))))
                    .into_any_element();
                (head, body, foot)
            }
        };
        let side = self.side_panel("MARKDOWN", cx)
            .child(head)
            .child(div().flex_1().min_h_0().flex().flex_col().child(body))
            .child(foot);
        self.slide_in(side, ("md-scrim", "md-slide"), ("md_panel", 820.), cx)
    }
}

/// "https://…", "mailto:…" and the like: not a file of the work folder.
fn is_external(url: &str) -> bool {
    let url = url.trim();
    url.starts_with("mailto:") || url.split_once("://").is_some_and(|(scheme, _)| !scheme.is_empty() && scheme != "file" && scheme.chars().all(|ch| ch.is_ascii_alphanumeric() || "+-.".contains(ch)))
}

/// http(s) and mailto: the links that open in the default browser or mail app.
fn is_web(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    lower.starts_with("https://") || lower.starts_with("http://") || lower.starts_with("mailto:")
}

fn is_markdown_name(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.ends_with(".md") || lower.ends_with(".markdown")
}

fn is_absolute(path: &str) -> bool {
    path.starts_with('/') || path.starts_with('\\') || path.as_bytes().get(1) == Some(&b':')
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Some(b) = std::str::from_utf8(&bytes[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_string())
}

/// The Markdown file a link points at, as a path for `query/markdown_file`:
/// relative to the document `base` (else to the work folder), `..` resolved
/// (one that leaves the folder stays, so the server refuses it). `None`: not
/// a link to a Markdown file.
pub fn resolve_link(base: Option<&str>, link: &str) -> Option<String> {
    let link = link.trim();
    let link = link.split(['#', '?']).next().unwrap_or_default();
    if link.is_empty() || is_external(link) {
        return None;
    }
    let link = match link.strip_prefix("file://") {
        // file:///C:/x.md → C:/x.md, file:///home/x.md → /home/x.md
        Some(rest) => {
            let rest = rest.strip_prefix('/').filter(|r| r.as_bytes().get(1) == Some(&b':')).unwrap_or(rest);
            rest.to_string()
        }
        None => link.to_string(),
    };
    let link = percent_decode(&link).replace('\\', "/");
    if !is_markdown_name(&link) {
        return None;
    }
    if is_absolute(&link) {
        return Some(link);
    }
    let mut parts: Vec<&str> = base.map(|b| b.rsplit_once('/').map_or("", |(dir, _)| dir)).unwrap_or_default().split('/').filter(|p| !p.is_empty()).collect();
    for part in link.split('/') {
        match part {
            "" | "." => {}
            ".." if parts.last().is_some_and(|p| *p != "..") => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    Some(parts.join("/"))
}

/// A memo cache and the text bytes it holds results for.
type Memo<T> = std::cell::RefCell<(usize, std::collections::HashMap<u64, T>)>;

thread_local! {
    static LINKED: Memo<SharedString> = Default::default();
    static PATHS: Memo<Vec<String>> = Default::default();
}

/// What a memo cache may hold, in text bytes, before it starts over.
const MEMO_BYTES: usize = 4 << 20;

/// `f(text)`, kept per text: messages render again on every repaint while their text rarely changes.
fn memo<T: Clone>(cache: &'static std::thread::LocalKey<Memo<T>>, text: &str, f: impl FnOnce(&str) -> T) -> T {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    let key = h.finish();
    cache.with(|cache| {
        let (bytes, cache) = &mut *cache.borrow_mut();
        if let Some(hit) = cache.get(&key) {
            return hit.clone();
        }
        // Streaming leaves a result per delta behind, each the whole answer so far: a long answer
        // left ~40 MB at a cap of 4096 entries, so cap by size and start over now and then.
        if *bytes + text.len() > MEMO_BYTES || cache.len() >= 4096 {
            *cache = Default::default();
            *bytes = 0;
        }
        let value = f(text);
        cache.insert(key, value.clone());
        *bytes += text.len();
        value
    })
}

/// `link_paths`, memoized.
pub fn linked(text: &str) -> SharedString {
    memo(&LINKED, text, |t| link_paths(t).into())
}

/// Markdown file names an answer mentions (in prose, code or a link target), at most a few.
pub fn markdown_paths(text: &str) -> Vec<String> {
    const MAX: usize = 4;
    let mut found: Vec<String> = Vec::new();
    for token in text.split(|ch: char| ch.is_whitespace() || "()[]<>\"'`,;*".contains(ch)) {
        let token = token.trim_end_matches(['.', ':', '!', '?']);
        let token = token.split('#').next().unwrap_or_default();
        if token.len() < 4 || token.contains("://") || !is_markdown_name(token) {
            continue;
        }
        if let Some(path) = resolve_link(None, token) {
            if !found.contains(&path) {
                found.push(path);
            }
        }
        if found.len() == MAX {
            break;
        }
    }
    found
}

/// Message Markdown with bare `.md` paths in plain text turned into links, so
/// a click opens them like any other link. Code blocks, inline code and
/// existing links stay as they are; only whitespace-free tokens count.
// shortcut: indented (4-space) code blocks are not recognised, only fenced ones; upgrade if models start using them.
pub fn link_paths(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let (mut fence, mut code, mut bracket) = (false, false, 0i32);
    for line in text.split_inclusive('\n') {
        if line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~") {
            fence = !fence;
            out.push_str(line);
            continue;
        }
        if fence {
            out.push_str(line);
            continue;
        }
        let mut rest = line;
        while !rest.is_empty() {
            let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            let (token, after) = rest.split_at(end);
            let space = after.len() - after.trim_start_matches(|ch: char| ch.is_whitespace() && ch != '\n').len();
            let (space, after) = after.split_at(space);
            let plain = !code && bracket == 0 && !token.contains(['`', '[', ']', '<', '>', '*', '|']);
            code ^= token.matches('`').count() % 2 == 1;
            bracket += token.matches('[').count() as i32 - token.matches(']').count() as i32;
            bracket = bracket.max(0);
            let lead = token.len() - token.trim_start_matches(['(', '"', '\'']).len();
            let core = token[lead..].trim_end_matches(['.', ',', ';', ':', '!', '?', ')', '"', '\'']);
            let tail = &token[lead + core.len()..];
            // A closing quote without its opening one ends a quoted path with spaces: not this token alone.
            let cut = tail.contains(['"', '\'']) && !token[..lead].contains(['"', '\'']);
            if plain && !cut && !core.is_empty() && resolve_link(None, core).is_some() {
                out.push_str(&token[..lead]);
                out.push_str(&format!("[{}](<{}>)", core.replace('\\', "\\\\"), core.replace('\\', "/")));
                out.push_str(tail);
            } else {
                out.push_str(token);
            }
            out.push_str(space);
            rest = if after.starts_with('\n') { out.push('\n'); &after[1..] } else { after };
        }
    }
    out
}

/// "2026-10-07 14:03" in local time.
pub fn stamp(ms: i64) -> String {
    let local = ms.div_euclid(1000) + aurelius_host::utc_offset_minutes_at(ms) * 60;
    let days = local.div_euclid(86_400);
    let minutes = local.rem_euclid(86_400) / 60;
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02} {:02}:{:02}", minutes / 60, minutes % 60)
}

#[cfg(test)]
mod tests {
    use super::{DocState, MarkdownPanel, Doc, link_paths, markdown_paths, resolve_link};
    use crate::backend::MarkdownFailure;
    use aurelius_protocol::{FileScope, MarkdownFileContent};

    #[test]
    fn a_streamed_answer_does_not_pile_up_in_the_memo() {
        // Every version of a growing 200 kB answer, as the 40 ms repaints see it.
        let answer = "word ".repeat(40_000);
        for end in (0..answer.len()).step_by(500) {
            super::linked(&answer[..end]);
        }
        let held = super::LINKED.with(|c| c.borrow().0);
        assert!(held <= super::MEMO_BYTES, "{held} bytes held");
        assert_eq!(super::linked("see notes.md").to_string(), super::link_paths("see notes.md"));
    }

    #[test]
    fn links_resolve_against_the_document() {
        assert_eq!(resolve_link(Some("docs/a.md"), "b.md").as_deref(), Some("docs/b.md"));
        assert_eq!(resolve_link(Some("docs/a.md"), "./sub/b.md#top").as_deref(), Some("docs/sub/b.md"));
        assert_eq!(resolve_link(Some("docs/a.md"), "../README.md").as_deref(), Some("README.md"));
        assert_eq!(resolve_link(Some("a.md"), "../x.md").as_deref(), Some("../x.md"));
        assert_eq!(resolve_link(None, "docs/my%20file.md").as_deref(), Some("docs/my file.md"));
        assert_eq!(resolve_link(Some("docs/a.md"), "/srv/work/x.MD").as_deref(), Some("/srv/work/x.MD"));
        assert_eq!(resolve_link(None, "file:///C:/work/x.md").as_deref(), Some("C:/work/x.md"));
        assert_eq!(resolve_link(None, r"C:\work\x.md").as_deref(), Some("C:/work/x.md"));
    }

    #[test]
    fn other_links_are_not_documents() {
        assert_eq!(resolve_link(None, "https://example.com/a.md"), None);
        assert_eq!(resolve_link(None, "mailto:a@b.c"), None);
        assert_eq!(resolve_link(None, "#section"), None);
        assert_eq!(resolve_link(None, "image.png"), None);
        assert_eq!(resolve_link(None, ""), None);
    }

    #[test]
    fn only_web_links_leave_the_app() {
        assert!(super::is_web(" HTTPS://example.com/a.md") && super::is_web("http://x.io") && super::is_web("mailto:a@b.c"));
        assert!(!super::is_web("ms-settings://x") && !super::is_web("file:///C:/x.exe") && !super::is_web("javascript:alert(1)"));
        assert!(super::is_external("ms-settings://x"));
    }

    #[test]
    fn answers_name_their_documents() {
        let text = "See `docs/plan.md`, then [the notes](notes/today.md#x). Also https://x.io/readme.md and plan.md.";
        assert_eq!(markdown_paths(text), vec!["docs/plan.md", "notes/today.md", "plan.md"]);
        assert!(markdown_paths("nothing here, not even a .md").is_empty());
    }

    #[test]
    fn bare_paths_become_links() {
        assert_eq!(link_paths("See docs/plan.md."), "See [docs/plan.md](<docs/plan.md>).");
        assert_eq!(link_paths(r"Wrote C:\work\x.md, (and /srv/y.MD)"), r"Wrote [C:\\work\\x.md](<C:/work/x.md>), (and [/srv/y.MD](</srv/y.MD>))");
        assert_eq!(link_paths("\"C:/a b/c.md\" a.md\n\tb.md\r\n"), "\"C:/a b/c.md\" [a.md](<a.md>)\n\t[b.md](<b.md>)\r\n");
        let kept = "`docs/a.md` and `see b.md` [the c.md notes](c.md) https://x.io/d.md e.md's\n```\nf.md\n```\n";
        assert_eq!(link_paths(kept), kept);
        assert_eq!(link_paths("```\nx\n```\ng.md"), "```\nx\n```\n[g.md](<g.md>)");
        // Web URLs stay as written: the GFM parser makes them links itself.
        let web = "See https://x.io/a.md, <https://y.io> and www.z.io.";
        assert_eq!(link_paths(web), web);
    }

    #[test]
    fn memoized_links_follow_the_text() {
        assert_eq!(super::linked("See a.md").as_ref(), "See [a.md](<a.md>)");
        assert_eq!(super::linked("See a.md").as_ref(), "See [a.md](<a.md>)");
        assert_eq!(super::linked("See b.md").as_ref(), "See [b.md](<b.md>)");
    }

    #[test]
    fn a_panel_takes_only_the_document_it_asked_for() {
        let scope = FileScope::Project { project_id: aurelius_domain::ProjectId::new() };
        let mut panel = MarkdownPanel { scope: scope.clone(), title: "P".into(), doc: Doc { asked: "a.md".into(), state: DocState::Loading }, back: Vec::new() };
        let file = MarkdownFileContent { path: "a.md".into(), text: "# a".into(), byte_length: 3, modified_ms: 0 };
        assert!(!panel.apply_doc(&scope, "b.md", Ok(file.clone())));
        assert!(matches!(panel.doc.state, DocState::Loading));
        assert!(panel.apply_doc(&scope, "a.md", Ok(file)));
        assert!(matches!(panel.doc.state, DocState::Loaded(_)));
        assert!(panel.apply_doc(&scope, "a.md", Err(MarkdownFailure::NotFound("gone".into()))));
        assert!(matches!(panel.doc.state, DocState::Failed(MarkdownFailure::NotFound(_))));
    }
}
