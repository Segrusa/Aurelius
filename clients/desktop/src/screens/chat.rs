//! Shared conversation pieces: messages, composer and status row.

use crate::app::AppView;
use crate::screens::team_feed as feed;
use aurelius_domain::{MessageId, conversation::{MessageRole, MessageStatus}};
use crate::keymap::*;
use crate::theme::{self, c};
use crate::ui::{self, h, v, ButtonKind};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

/// Who wrote a message, as the conversation shows it: another board member as the discussion
/// names them ("ada · board member"), "You" for your own posts (`me` is this connection's
/// board member) and those without a board member, else its author (an employee in the group
/// chat, a persona), else `fallback` (the screen's employee).
pub fn message_author(m: &aurelius_protocol::MessageView, fallback: &SharedString, me: Option<&aurelius_domain::user::UserView>) -> SharedString {
    match m.role {
        MessageRole::User if other_member(m, me) => {
            let a = crate::screens::discussion::author(m, |_| None);
            format!("{} · {}", a.name, a.kind).into()
        }
        MessageRole::User => "You".into(),
        // A line the server writes itself (nobody was woken, say) may come without a name.
        MessageRole::Assistant => m.author.as_deref().map(str::trim).filter(|a| !a.is_empty()).map(|a| SharedString::from(a.to_string())).unwrap_or_else(|| fallback.clone()),
    }
}

/// A post by a board member other than `me`: it reads with their name and stands on the left.
pub fn other_member(m: &aurelius_protocol::MessageView, me: Option<&aurelius_domain::user::UserView>) -> bool {
    m.author_user.as_ref().is_some_and(|u| me.is_none_or(|me| me.id != u.id))
}

/// An employee's name with their role, as the group chat shows senders and `@` suggestions:
/// "Orion — Project Manager". Without a role, the name alone.
pub fn with_role(name: &str, role: Option<&str>) -> String {
    match role.map(str::trim).filter(|r| !r.is_empty()) {
        Some(role) => format!("{name} — {role}"),
        None => name.to_owned(),
    }
}

/// An employee's answer streaming into the group chat before it is a message there
/// (its text and steps come from the stream); marked as a group post like it will be.
/// Its run is what its Stop stops and what its approvals wait on; `mode` is the one it started in.
pub(crate) fn streaming_answer(id: MessageId, run: aurelius_domain::RunId, author: Option<String>, mode: Option<aurelius_domain::autonomy::AgentMode>) -> aurelius_protocol::MessageView {
    aurelius_protocol::MessageView {
        edited_at_ms: None, deleted_at_ms: None, editable: false,
        id,
        ordinal: 0,
        role: MessageRole::Assistant,
        status: MessageStatus::Partial,
        text: String::new(),
        created_at_ms: 0,
        run_id: Some(run),
        reply_to: None,
        produced_by: None,
        author,
        sources: Vec::new(),
        activity: Vec::new(),
        update: Some(aurelius_domain::group_chat::GROUP_KIND.into()),
        approval_id: None,
        files: Vec::new(),
        uploads: Vec::new(),
        queued: false,
        usage: None,
        mode,
        author_user: None,
        system: None,
        thread_parent: None,
        thread_reply_count: None,
        thread_last_reply_ms: None,
        thread_unread: None,
        thread_repliers: Vec::new(),
        reactions: Vec::new(),
    }
}

/// A note the server writes in the group chat itself: kind `status` there (nobody was woken),
/// kind `system`, or a finished group post without an author. A streaming answer whose
/// employee is not known yet is no note; in a task, `status` stays an update card.
fn is_system_line(m: &aurelius_protocol::MessageView, in_group: bool) -> bool {
    let unnamed = m.author.as_deref().is_none_or(|a| a.trim().is_empty());
    m.role == MessageRole::Assistant
        && match m.update.as_deref() {
            Some("system") => true,
            Some("status") => in_group,
            Some(kind) => kind == aurelius_domain::group_chat::GROUP_KIND && unnamed && m.status != MessageStatus::Partial,
            None => false,
        }
}

/// Messages longer than this many lines show only these until "Show more".
const FOLD_LINES: usize = 20;

/// `text` cut to its first FOLD_LINES lines (every line of the raw markdown counts, blank ones too,
/// trailing blank lines not; as on mobile), with a code block cut open closed again:
/// (shown text, byte offset of the cut, lines hidden).
fn fold(text: &str) -> Option<(String, usize, usize)> {
    let text = text.trim_end();
    // The open code block's fence: (indent, char, length).
    let (mut lines, mut pos, mut open, mut cut) = (0, 0, None::<(&str, char, usize)>, None);
    for line in text.split_inclusive('\n') {
        pos += line.len();
        lines += 1;
        if let Some((indent, ch, len, rest)) = fence(line.trim_end_matches(['\n', '\r'])) {
            match open {
                // A backtick fence's info string has no backticks.
                None if !(ch == '`' && rest.contains('`')) => open = Some((indent, ch, len)),
                Some((_, c, l)) if ch == c && len >= l && rest.trim().is_empty() => open = None,
                _ => {}
            }
        }
        if lines == FOLD_LINES {
            cut = Some((pos, open));
        }
    }
    let (cut, open) = cut.filter(|_| lines > FOLD_LINES)?;
    let mut shown = text[..cut].trim_end().to_string();
    if let Some((indent, ch, len)) = open {
        shown.push('\n');
        shown.push_str(indent);
        shown.extend(std::iter::repeat_n(ch, len));
    }
    Some((shown, cut, lines - FOLD_LINES))
}

/// A code fence line (``` or ~~~, 3 or more, as on mobile): (indent, char, length, rest of the line).
fn fence(line: &str) -> Option<(&str, char, usize, &str)> {
    let body = line.trim_start();
    let indent = &line[..line.len() - body.len()];
    let ch = body.chars().next().filter(|c| *c == '`' || *c == '~')?;
    let len = body.len() - body.trim_start_matches(ch).len();
    (len >= 3).then(|| (indent, ch, len, &body[len..]))
}

/// A plan's title for its folded card: its first line, without Markdown heading marks.
fn plan_title(text: &str) -> String {
    text.lines().map(|l| l.trim().trim_start_matches('#').trim()).find(|l| !l.is_empty()).unwrap_or("Plan").to_owned()
}

/// The open conversation's scroll area as its own cached view. Typing and the cursor
/// blink notify only the composer, so AppView re-renders but this pane is reused instead
/// of rebuilding every message; it observes AppView, so any AppView notify re-renders it.
/// Inside it each finished message is a cached `MessageCell`: stream repaints (which notify
/// only this pane) and quiet AppView notifies (`AppView::quiet_notify`: the 1 s tick, streams
/// shown beside) rebuild just the streaming message.
pub struct ConversationPane {
    app: WeakEntity<AppView>,
    /// What the current screen shows here; set on each AppView render.
    pub body: std::rc::Rc<dyn Fn(&mut AppView, &mut Window, &mut Context<AppView>) -> AnyElement>,
    /// AppView changed more than streamed text or elapsed times since the last render:
    /// the finished messages render again instead of from their cached views.
    full: bool,
}

impl ConversationPane {
    pub fn new(app: &Entity<AppView>, cx: &mut Context<Self>) -> Self {
        cx.observe(app, |pane, app, cx| {
            pane.full |= !app.read(cx).quiet_notify;
            cx.notify();
        })
        .detach();
        Self { app: app.downgrade(), body: std::rc::Rc::new(|_, _, _| div().into_any_element()), full: true }
    }
}

impl Render for ConversationPane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (body, full) = (self.body.clone(), std::mem::take(&mut self.full));
        let body = self
            .app
            .update(cx, |app, cx| {
                app.message_cache.get_mut().full = full;
                body(app, window, cx)
            })
            .unwrap_or_else(|_| div().into_any_element());
        // Laid out as a root at the pane's bounds: fill them like the scroll frame did.
        div().size_full().flex().flex_col().child(body)
    }
}

/// One finished message of the open conversation as its own view. The pane shows it
/// cached at its measured height, so a repaint reuses it while it stays in place, and
/// only as a blank box of that height while it is far out of view.
pub struct MessageCell {
    app: WeakEntity<AppView>,
    id: MessageId,
}

impl Render for MessageCell {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let id = self.id;
        self.app.update(cx, |app, cx| app.render_cell(id, cx)).unwrap_or_else(|_| div().into_any_element())
    }
}

/// Where a message was drawn last, measured by `AppView::measured`.
#[derive(Clone, Copy, PartialEq)]
pub struct MessageBox {
    /// Width of the scroll area then: a box measured at another width is stale.
    width: Pixels,
    height: Pixels,
    /// Within a screen of the visible part of the scroll area.
    near: bool,
}

/// The open conversation's per-message render state, dropped with the conversation.
#[derive(Default)]
pub struct MessageCache {
    /// Finished messages as cached views.
    cells: std::collections::HashMap<MessageId, Entity<MessageCell>>,
    /// Markdown states by element key, kept while the message is out of view: (message, text shown, state).
    texts: std::collections::HashMap<String, (MessageId, SharedString, Entity<gpui_kit::component::text::TextViewState>)>,
    /// Shared with the measuring canvases.
    boxes: std::rc::Rc<std::cell::RefCell<std::collections::HashMap<MessageId, MessageBox>>>,
    /// Who the assistant's messages are from on the current screen.
    author: SharedString,
    /// This render follows a full AppView change (`ConversationPane::full`).
    full: bool,
}

impl MessageCache {
    /// Forget messages that left the conversation.
    pub fn retain(&mut self, messages: &[aurelius_protocol::MessageView]) {
        let ids: std::collections::HashSet<MessageId> = messages.iter().map(|m| m.id).collect();
        self.cells.retain(|id, _| ids.contains(id));
        self.texts.retain(|_, (id, _, _)| ids.contains(id));
        self.boxes.borrow_mut().retain(|id, _| ids.contains(id));
    }
}

impl AppView {
    /// Whether a plan in the conversation shows in full: as the user folded it, else open while it waits.
    pub fn plan_shown(&self, message: MessageId, waiting: bool) -> bool {
        self.plan_open.get(&message).copied().unwrap_or(waiting)
    }

    /// Fold or unfold the latest plan in the open conversation (Alt+Shift+P).
    pub fn toggle_latest_plan(&mut self, cx: &mut Context<Self>) {
        let latest = self.messages.as_ref().and_then(|s| s.messages.iter().rev().find(|m| m.update.as_deref() == Some("plan")).map(|m| (m.id, m.approval_id)));
        let Some((id, approval)) = latest else {
            self.notify_user("There is no plan in this conversation.", false, cx);
            return;
        };
        let waiting = approval.is_some_and(|a| self.pending_approvals().any(|p| p.id == a));
        let shown = self.plan_shown(id, waiting);
        self.plan_open.insert(id, !shown);
        cx.notify();
    }

    /// `body` (a conversation's scroll area) in the cached conversation pane, which takes the
    /// room a `flex_1` scroll frame would.
    pub fn conversation_pane(&self, body: impl Fn(&mut AppView, &mut Window, &mut Context<AppView>) -> AnyElement + 'static, cx: &mut Context<Self>) -> AnyElement {
        // No notify: the pane re-renders on AppView's own notify, which every state change makes.
        self.conversation_pane.update(cx, |pane, _| pane.body = std::rc::Rc::new(body));
        self.conversation_pane.clone().cached(StyleRefinement::default().flex_1().min_h_0().w_full()).into_any_element()
    }

    /// Messages of the open conversation (chat or manager), with live streaming text.
    /// The streaming message renders here on every repaint; finished ones are `MessageCell`s.
    pub fn render_messages(&self, author: SharedString, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(snapshot) = self.messages.as_ref() else {
            return v().w_full().max_w(px(700.)).child(div().text_color(c(theme::MUTED)).child("Loading…"));
        };
        let full = {
            let mut cache = self.message_cache.borrow_mut();
            cache.author = author.clone();
            cache.full
        };
        let mut rows: Vec<AnyElement> = Vec::new();
        // The Team chat is a flat feed (ui-ontwerp §2): day separators, grouped rows.
        let group = self.screen == crate::app::Page::GroupChat;
        // A channel opened with messages not read yet: "New messages" before the first of them (fase 2 §3).
        let divider = self.channel_new_at.filter(|_| group);
        let today = feed::local_day(now_ms(), aurelius_host::local_utc_offset_minutes());
        for (i, m) in snapshot.messages.iter().enumerate().filter(|(_, m)| !self.deleted_queued.contains(&m.id)) {
            let before = if group { feed::before(&snapshot.messages, i, divider, feed_day) } else { feed::Before::default() };
            if before.day {
                let label = feed::day_label(feed_day(m.created_at_ms), today);
                rows.push(feed_separator(SharedString::from(format!("day-{}", m.id)), label.into(), theme::BORDER, theme::TEXT_2).pt(px(18.)).pb(px(4.)).into_any_element());
            }
            if before.new {
                rows.push(feed_separator("channel-new".into(), "New messages".into(), theme::ACCENT, theme::ACCENT_TEXT).pt(px(14.)).into_any_element());
            }
            let el = if m.status == MessageStatus::Partial || self.live.contains_key(&m.id) {
                self.render_message(m, &author, cx)
            } else {
                self.message_cell(m.id, full, cx)
            };
            // Right-click: "Create ticket…" (fase 2 §4), and React… / Reply in thread (ui-ontwerp §5), as the hover actions do.
            // The message a link opened: its halo, and the row the list scrolls to (AUR-FA A5a).
            let row = v().w_full().relative().child(el).when(self.link_flashes(m.id), |el| el.rounded(px(4.)).bg(c(theme::ACCENT_HALO)).child(ui::track_bounds(self.dropdown_bounds.clone(), crate::screens::links::LINK_TARGET)));
            let row = if !self.message_menu(m, Point::default()).is_empty() {
                let message = m.clone();
                row.on_mouse_down(MouseButton::Right, cx.listener(move |this, e: &MouseDownEvent, w, cx| {
                    cx.stop_propagation();
                    let items = this.message_menu(&message, e.position);
                    this.open_menu(e.position, items, w, cx);
                }))
            } else {
                row
            };
            let row = row.when(group, |el| el.pt(px(if before.grouped { 2. } else { 14. })));
            rows.push(ui::rise(row, SharedString::from(format!("msg-{}", m.id)), 0).into_any_element());
        }
        // Group chat: answers still streaming in, before they are messages there.
        for l in self.group_live.iter().filter(|l| !snapshot.messages.iter().any(|m| m.id == l.message)) {
            let streaming = streaming_answer(l.message, l.run, l.author.clone().or_else(|| self.run_author(l.run)), l.mode);
            rows.push(ui::rise(v().w_full().when(group, |el| el.pt(px(14.))).child(self.render_message(&streaming, &author, cx)), SharedString::from(format!("msg-{}", l.message)), 0).into_any_element());
        }
        // Group chat: what an employee's run asks that has no answer streaming here to show it under.
        if self.open_conversation.is_some() && self.open_conversation == self.group_conversation {
            rows.extend(self.group_approvals(cx).into_iter().map(|a| div().w_full().pt(px(14.)).child(a).into_any_element()));
        }
        // The feed spaces its rows itself (a grouped row sits close under its header).
        v().w_full().max_w(px(700.)).gap(px(if group { 0. } else { 26. }))
            .children(rows)
            // The answer waits for the server's load ("Server busy: CPU 82%, waiting"), live.
            .children(self.load_banner(cx).map(|el| div().w_full().when(group, |el| el.pt(px(14.))).child(el)))
            // The suggested replies under the last answer: not a message, nothing waits for them.
            .children(self.quick_replies_card(cx).map(|el| div().w_full().when(group, |el| el.pt(px(14.))).child(el)))
    }

    /// A finished message: its cached view at the height it was measured at (rendered again
    /// after a full AppView change), or a blank box of that height while it is far out of view.
    fn message_cell(&self, id: MessageId, full: bool, cx: &mut Context<Self>) -> AnyElement {
        let width = self.scroll.bounds().size.width;
        let known = self.message_cache.borrow().boxes.borrow().get(&id).copied().filter(|b| b.width == width);
        if let Some(b) = known.filter(|b| !b.near) {
            return self.measured(id, true, div().w_full().h(b.height).into_any_element());
        }
        let app = cx.weak_entity();
        let cell = self.message_cache.borrow_mut().cells.entry(id).or_insert_with(|| cx.new(|_| MessageCell { app, id })).clone();
        match known {
            Some(b) if !full => cell.cached(StyleRefinement::default().w_full().h(b.height)).into_any_element(),
            _ => cell.into_any_element(),
        }
    }

    /// What a `MessageCell` shows.
    pub fn render_cell(&self, id: MessageId, cx: &mut Context<Self>) -> AnyElement {
        let Some(m) = self.messages.as_ref().and_then(|s| s.messages.iter().find(|m| m.id == id)) else {
            return div().into_any_element();
        };
        let author = self.message_cache.borrow().author.clone();
        let el = self.render_message(m, &author, cx);
        self.measured(id, false, el)
    }

    /// `el` with a probe that records its box (`MessageBox`) on every prepaint. The pane renders
    /// again at once when a blank box came near the view or a measured height changed.
    fn measured(&self, id: MessageId, placeholder: bool, el: AnyElement) -> AnyElement {
        let (boxes, scroll, pane) = (self.message_cache.borrow().boxes.clone(), self.scroll.clone(), self.conversation_pane.entity_id());
        let probe = canvas(
            move |bounds, window, _| {
                let view = scroll.bounds();
                let near = bounds.bottom() >= view.top() - view.size.height && bounds.top() <= view.bottom() + view.size.height;
                let new = MessageBox { width: view.size.width, height: bounds.size.height, near };
                let old = boxes.borrow_mut().insert(id, new);
                if old.is_some_and(|old| old.width == new.width && (old.height != new.height || (placeholder && near))) {
                    // A notify while drawing is dropped: ask for the next frame, like an animation does.
                    window.on_next_frame(move |_, cx| cx.notify(pane));
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full();
        // The inner column keeps its natural height inside a cached cell laid out at the old one.
        v().w_full().child(v().w_full().relative().child(el).child(probe)).into_any_element()
    }

    /// `text` as Markdown whose state lives with the message (`MessageCache::texts`), so a message
    /// scrolled back into view keeps its parsed text, and a long streamed text is appended instead
    /// of replacing the whole document on every repaint.
    fn message_markdown(&self, message: MessageId, key: String, text: SharedString, cx: &mut Context<Self>) -> gpui_kit::component::text::TextView {
        use gpui_kit::component::text::{TextView, TextViewState};
        let mut cache = self.message_cache.borrow_mut();
        let texts = &mut cache.texts;
        let state = match texts.get_mut(&key) {
            Some((_, shown, state)) => {
                if *shown != text {
                    // Up to 4 KB set_text parses at once (no second render for the result); beyond, both parse
                    // in the background, and appending parses only the last block again.
                    let rest = text.strip_prefix(&**shown).filter(|_| text.len() > 4 * 1024).map(str::to_owned);
                    let replace = text.clone();
                    state.update(cx, |state, cx| match rest {
                        Some(rest) => state.push_str(&rest, cx),
                        None => state.set_text(&replace, cx),
                    });
                    *shown = text;
                }
                state.clone()
            }
            None => {
                let state = cx.new(|cx| TextViewState::markdown(&text, cx));
                texts.insert(key, (message, text, state.clone()));
                state
            }
        };
        TextView::new(&state).selectable(true).plugin(crate::screens::mermaid::Mermaid)
    }

    /// One message of the open conversation.
    pub(crate) fn render_message(&self, m: &aurelius_protocol::MessageView, author: &SharedString, cx: &mut Context<Self>) -> AnyElement {
        let all = self.messages.as_ref().map_or(&[][..], |s| &s.messages[..]);
        let group = self.screen == crate::app::Page::GroupChat;
        // The same author's message just before (ui-ontwerp §2): no header of its own.
        let grouped = group && all.iter().position(|x| x.id == m.id).is_some_and(|i| feed::before(all, i, self.channel_new_at, feed_day).grouped);
        if m.role == MessageRole::User && group {
            return self.feed_post(m, grouped, cx);
        }
        if m.role == MessageRole::User {
            let waiting = m.queued;
            // Another board member's post stands on the left; yours on the right.
            let mine = !other_member(m, self.active_member());
            let side = |el: Div| if mine { el.self_end().items_end() } else { el.self_start().items_start() };
            // Images sent with it show under it (the text may be empty then).
            let (folded, fold_toggle) = self.fold_view(m.id, &m.text, cx);
            let shown = folded.map_or_else(|| m.text.clone(), |(s, _)| s);
            let sent = side(v()).gap(px(6.))
                .when(!m.text.trim().is_empty() || (m.files.is_empty() && m.uploads.is_empty()), |el| el.child(bubble(self.message_markdown(m.id, format!("user-{}", m.id), crate::screens::markdown::linked(&self.with_link_pills(&shown)), cx).on_link_click(self.markdown_link_handler(false, cx)), mine)))
                .children(fold_toggle)
                .when(!m.files.is_empty(), |el| el.child(self.message_files(m.id, &m.files, cx)))
                .when(!m.uploads.is_empty(), |el| el.child(self.message_uploads(m.id, &m.uploads, cx)))
                .children(self.markdown_chips(m.id, &m.text, cx))
                .child(h().gap(px(4.)).children(self.message_ticket_pill(m.id, cx)).children(self.make_ticket_action(m, cx)).when(!m.text.trim().is_empty(), |el| el.child(self.copy_button(format!("msg-{}", m.id), m.text.clone(), cx))));
            // In the group chat every message says who wrote it, yours too; elsewhere another board member's.
            let sent = if self.screen == crate::app::Page::GroupChat || !mine {
                side(v()).gap(px(6.)).child(div().text_size(px(12.)).text_color(c(theme::MUTED)).child(message_author(m, author, self.active_member()))).child(sent)
            } else {
                sent
            };
            // Sent while an answer runs: it waits for that answer, unless the user sends it now or deletes it.
            let sent = if waiting {
                let id = m.id;
                v().self_end().items_end().gap(px(7.))
                    .child(sent.opacity(0.55))
                    .child(h().gap(px(8.)).items_center().text_size(px(12.)).text_color(c(theme::MUTED))
                        .child(ui::diamond(7., theme::MUTED, false))
                        .child("Waiting · sends after the current answer"))
                    .child(
                        h().gap(px(8.))
                            .child(send_now_button(SharedString::from(format!("queued-delete-{id}")), "Delete")
                                .text_color(c(theme::TEXT_2))
                                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.delete_queued(id, cx))))
                            .child(send_now_button(SharedString::from(format!("queued-send-{id}")), "Send now")
                                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.send_queued_now(id, cx)))),
                    )
            } else {
                sent
            };
            return sent.into_any_element();
        }
        // A line the server writes in the group chat itself (nobody could answer, an answer
        // was cut off): a quiet note, not an answer by somebody.
        // A system line of phase 2 (a ticket made from a message here, a loop limit): a quiet note.
        if let Some(line) = m.system.as_ref().filter(|_| group) {
            return self.feed_line(m, line, cx);
        }
        if let Some(line) = &m.system {
            let text = crate::screens::discussion::system_text(line, &m.text);
            let when = ui::fmt_when(m.created_at_ms);
            let ticket = line.ticket_id.filter(|_| line.kind == aurelius_domain::views::SystemLineKind::TicketCreated);
            // ui-ontwerp §5: a run without a ticket waits for the sender's Claude login.
            let login = crate::screens::discussion::login_line(line, &text);
            let connect = login == Some(crate::screens::discussion::LoginLine::Connect) && crate::screens::account::needs_connect(self.claude.view.as_ref(), self.claude.unsupported.is_some());
            return h().id(SharedString::from(format!("system-{}", m.id))).w_full().justify_center().gap(px(8.)).px(px(24.)).role(Role::Note).aria_label(crate::screens::discussion::system_label(&text, &when))
                .child(div().text_size(px(12.5)).line_height(px(18.)).text_color(c(theme::MUTED)).text_center().child(format!("┄ {}{text} ┄", crate::screens::discussion::login_mark(login))))
                .when(connect, |el| {
                    el.child(div().id(SharedString::from(format!("system-connect-{}", m.id))).role(Role::Button).cursor_pointer().text_size(px(12.)).text_color(c(theme::ACCENT_TEXT)).child(crate::screens::board::connect_label(self.claude.view.as_ref())).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.claude_connect(cx))))
                })
                .when_some(ticket, |el, id| {
                    el.child(div().id(SharedString::from(format!("system-open-{}", m.id))).role(Role::Link).cursor_pointer().text_size(px(12.)).text_color(c(theme::ACCENT_TEXT)).child("Open ticket").on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.open_ticket(id, w, cx))))
                })
                .into_any_element();
        }
        if is_system_line(m, self.open_conversation.is_some() && self.open_conversation == self.group_conversation) {
            let text = m.text.trim().to_string();
            return h().w_full().justify_center().px(px(24.))
                .child(div().text_size(px(12.5)).line_height(px(18.)).text_color(c(theme::MUTED)).text_center().child(if text.is_empty() { "…".to_string() } else { text }))
                .into_any_element();
        }
        // An employee's message in the group chat reads as an answer under its name.
        if let Some(kind) = m.update.clone().filter(|k| k != aurelius_domain::group_chat::GROUP_KIND) {
            // A task update: the task itself, or what a worker, subagent or colleague reports back.
            // Something a worker asks: decided right here (or in the inbox) while it is pending.
            let pending = m.approval_id.and_then(|id| self.pending_approvals().find(|a| a.id == id).cloned());
            // Claude's question: answered right here (or in the inbox) while it is pending.
            let question = pending.as_ref().filter(|a| a.tool == crate::app::QUESTION_TOOL);
            let asked_question = m.approval_id.is_some_and(|id| self.approvals.iter().any(|a| a.id == id && a.tool == crate::app::QUESTION_TOOL));
            let (label, accent) = match kind.as_str() {
                "brief" => ("TASK", true),
                "finding" => ("FINDING", false),
                "question" => ("NEEDS A DECISION", true),
                "approval" if question.is_some() => ("QUESTION · WAITING FOR YOU", true),
                "approval" if asked_question => ("QUESTION", false),
                "approval" if pending.is_some() => ("WAITING FOR YOU", true),
                "approval" => ("ASKED", false),
                "plan" if pending.is_some() => ("PLAN · WAITING FOR YOU", true),
                "plan" => ("PLAN", false),
                // message_task: the delegating manager writes to this task; the name follows the label.
                "message" => ("MESSAGE FROM", false),
                // set_mode: a management employee chose Auto or Plan for this task (text: "<name> chose Auto: <reason>").
                "mode_chosen" => ("MODE CHOSEN", false),
                _ => ("UPDATE", false),
            };
            let from = m.author.clone().unwrap_or_else(|| "Aurelius".into());
            // A plan folds: open while it waits for you, shut once decided (Alt+Shift+P toggles the latest).
            let is_plan = kind == "plan";
            let open = !is_plan || self.plan_shown(m.id, pending.is_some());
            let fold = is_plan.then(|| {
                let id = m.id;
                h().id(SharedString::from(format!("plan-fold-{}", m.id))).gap(px(6.)).cursor_pointer()
                    .text_color(c(theme::TEXT_2)).hover(|s| s.text_color(c(theme::TEXT)))
                    .child(if open { "▾ Hide plan" } else { "▸ Show plan" })
                    .child(ui::kbd("alt-shift-p", cx))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.plan_open.insert(id, !open);
                        cx.notify();
                    }))
            });
            let mut fold_toggle = None;
            let body: AnyElement = if let Some(a) = question {
                self.question_form(a, None, cx).into_any_element()
            } else if open {
                let folded;
                (folded, fold_toggle) = self.fold_view(m.id, &m.text, cx);
                let shown = folded.map_or_else(|| m.text.clone(), |(s, _)| s);
                div().text_size(px(13.5)).line_height(px(21.)).child(
                    self.message_markdown(m.id, format!("upd-{}", m.id), crate::screens::markdown::linked(&self.with_link_pills(&shown)), cx)
                        .on_link_click(self.markdown_link_handler(false, cx)),
                ).into_any_element()
            } else {
                div().text_size(px(13.5)).text_color(c(theme::TEXT_2)).truncate().child(plan_title(&m.text)).into_any_element()
            };
            // An action a worker asks for: what it would do, readable (also once decided).
            let asked = (kind == "approval").then(|| m.approval_id.and_then(|id| self.approvals.iter().find(|a| a.id == id)).cloned()).flatten()
                .filter(|a| a.tool != crate::app::QUESTION_TOOL && a.tool != crate::app::PLAN_TOOL);
            // Quick revision right under the plan: the shared feedback field and 'Pas aan'.
            let plan_pending = pending.as_ref().filter(|a| a.tool == crate::app::PLAN_TOOL).map(|a| a.id);
            let revise = plan_pending.map(|id| {
                h().gap(px(8.)).items_center().pt(px(4.))
                    .child(div().flex_1().min_w_0().child(ui::input(&self.plan_input)))
                    .child(ui::button(SharedString::from(format!("plan-revise-{id}")), "Pas aan", ButtonKind::Outline, None)
                        .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.decide_plan_for(id, aurelius_domain::approval::PlanDecision::Revise, w, cx))))
            });
            let actions = pending.map(|a| self.approval_actions(a, "upd", cx));
            return v().gap(px(6.)).px(px(14.)).py(px(11.)).rounded(px(8.)).border_1()
                .border_color(c(if accent { theme::ATTENTION_BORDER } else { theme::BORDER }))
                .bg(c(if accent { theme::ATTENTION_BG } else { theme::SURFACE }))
                .child(h().gap(px(8.)).text_size(px(11.5)).text_color(c(theme::MUTED)).child(ui::eyebrow(label)).child(div().flex_1().min_w_0().text_color(c(theme::TEXT_2)).child(from)).children(fold).child(self.copy_button(format!("msg-{}", m.id), m.text.clone(), cx)))
                .child(body)
                .children(fold_toggle)
                .children(asked.map(|a| self.approval_detail(&a, "card", cx)))
                .when(open && !m.files.is_empty(), |el| el.child(self.message_files(m.id, &m.files, cx)))
                .children(revise)
                .children(actions)
                .into_any_element();
        }
        let text = self.live.get(&m.id).cloned().unwrap_or_else(|| m.text.clone());
        let last_answer = all.iter().rev().find(|x| x.role == MessageRole::Assistant && x.update.is_none()).map(|x| x.id);
        // Versions: the messages answering the same one, and this answer's place among them.
        let total = m.reply_to.map_or(1, |r| all.iter().filter(|x| x.reply_to == Some(r)).count());
        let index = m.reply_to.map(|r| {
            all.iter().take_while(|x| x.id != m.id).filter(|x| x.role != MessageRole::User && x.update.is_none() && x.reply_to == Some(r)).count() + 1
        });
        let status: AnyElement = match m.status {
            MessageStatus::Partial if self.pending_approvals().any(|a| Some(a.run_id) == m.run_id) => v().gap(px(8.)).child(h()
                .gap(px(7.))
                .text_color(c(theme::ACCENT_TEXT))
                .child(ui::diamond(8., theme::ACCENT_TEXT, false))
                .child(if self.pending_approvals().any(|a| Some(a.run_id) == m.run_id && a.tool == crate::app::QUESTION_TOOL) { "Waiting for your answer above" } else if self.pending_approvals().any(|a| Some(a.run_id) == m.run_id && a.tool == crate::app::PLAN_TOOL) { "Waiting for your decision on the plan" } else { "Waiting for your approval" })
                .child(ui::hint(SharedString::from(format!("inbox-{}", m.id)), "secondary-i", "Open inbox", || Box::new(OpenInbox), cx))
                .children(self.pending_approvals().filter(|a| Some(a.run_id) == m.run_id && a.tool == crate::app::PLAN_TOOL).cloned().map(|a| {
                    ui::button(SharedString::from(format!("review-plan-{}", a.id)), "Review plan", ButtonKind::Primary, None).ml(px(8.)).on_click(cx.listener(move |this, _: &ClickEvent, w, cx| {
                        this.open_plan_approval(a.clone(), w, cx);
                    }))
                })))
                // What it asks, readable, with the buttons on top so they stay in view when unfolded.
                .children(self.pending_approvals().filter(|a| Some(a.run_id) == m.run_id && a.tool != crate::app::QUESTION_TOOL && a.tool != crate::app::PLAN_TOOL).cloned().collect::<Vec<_>>().into_iter().map(|a| {
                    let (id, digest) = (a.id, a.action_digest.clone());
                    let reject_digest = digest.clone();
                    let summary = crate::screens::approval::approval_summary(&a, &self.approval_folders(&a));
                    let always = self.always_allow_button(&a, "inline", cx);
                    v().gap(px(8.)).w_full().px(px(12.)).py(px(10.)).rounded(px(8.)).border_1()
                        .border_color(c(theme::ATTENTION_BORDER)).bg(c(theme::ATTENTION_BG))
                        .child(h().gap(px(8.))
                            .child(div().flex_1().min_w_0().text_size(px(13.)).text_color(c(theme::TEXT)).child(summary.title))
                            .child(ui::button(SharedString::from(format!("reject-{}", a.id)), "Reject", ButtonKind::Outline, None).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.decide_approval(id, reject_digest.clone(), false, cx);
                            })))
                            .children(always)
                            .child(ui::button(SharedString::from(format!("approve-{}", a.id)), "Approve once", ButtonKind::Primary, None).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.decide_approval(id, digest.clone(), true, cx);
                            }))))
                        .child(self.approval_detail(&a, "inline", cx))
                }))
                .into_any_element(),
            MessageStatus::Partial => h().gap(px(7.)).child(ui::blink_dot(6., theme::ACCENT)).child({
                // The step in progress, like "Running a command 4s · 333 tokens".
                let live = self.live_activity.get(&m.id).map(|a| &a[..]).unwrap_or(&m.activity[..]);
                match self.live_progress.get(&m.id) {
                    Some(p) if p.compacting => ui::COMPACTING,
                    _ => crate::screens::progress::current_step(live).unwrap_or(if text.is_empty() { "Thinking…" } else { "Writing…" }),
                }
            })
                .when_some(self.live_progress.get(&m.id), |el, p| el.child(div().text_color(c(theme::MUTED)).child(ui::fmt_progress(p.started_at_ms, p.output_tokens))))
                .when(m.update.is_none(), |el| el.child(ui::action_icon(SharedString::from(format!("stop-{}", m.id)), "■", "Stop", Some("secondary-."), Some(|| Box::new(StopGeneration)))))
                // An answer streaming into the group chat runs in its employee's own conversation: stop that run.
                .when_some(m.run_id.filter(|_| m.update.as_deref() == Some(aurelius_domain::group_chat::GROUP_KIND)), |el, run_id| {
                    el.child(ui::action_icon(SharedString::from(format!("stop-{}", m.id)), "■", "Stop", None, None).on_click(cx.listener(move |this, _: &ClickEvent, _, _| {
                        let _ = this.requests.send(crate::backend::Request::Stop { run_id });
                    })))
                })
                .into_any_element(),
            MessageStatus::Interrupted => h().gap(px(7.)).child("Stopped; text up to that point is kept.")
                .when(Some(m.id) == last_answer, |el| el.child(ui::action_icon(SharedString::from(format!("retry-{}", m.id)), "↻", "Retry", Some("secondary-r"), Some(|| Box::new(RetryAnswer)))))
                .into_any_element(),
            MessageStatus::Failed => h().gap(px(7.)).text_color(c(theme::NEGATIVE)).child("Failed; this doesn't count as an answer.")
                .when(Some(m.id) == last_answer, |el| el.child(ui::action_icon(SharedString::from(format!("retry-{}", m.id)), "↻", "Retry", Some("secondary-r"), Some(|| Box::new(RetryAnswer)))))
                .into_any_element(),
            MessageStatus::Complete if Some(m.id) == last_answer => h().gap(px(2.)).ml(px(-6.))
                .child(ui::action_icon(SharedString::from(format!("retry-{}", m.id)), "↻", "New version", Some("secondary-r"), Some(|| Box::new(RetryAnswer))))
                .child(ui::action_icon(SharedString::from(format!("context-{}", m.id)), "≡", "Context", Some("secondary-shift-e"), Some(|| Box::new(ShowContext))))
                .into_any_element(),
            MessageStatus::Complete => div().into_any_element(),
        };
        let sources = m.sources.clone();
        let copy = (!text.trim().is_empty()).then(|| self.copy_button(format!("msg-{}", m.id), text.clone(), cx));
        // What the run sent: only kept while trace mode is on, once the answer ended.
        let trace = m.run_id.filter(|_| m.status != MessageStatus::Partial && self.trace_on()).map(|run_id| {
            // action_icon has its own tooltip: a second one panics in debug builds.
            ui::action_icon(SharedString::from(format!("trace-{}", m.id)), "⌁", "Trace", None, None)
                .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| {
                    cx.stop_propagation();
                    this.open_trace(run_id, w, cx);
                }))
        });
        let usage = m.usage.as_ref().filter(|_| self.run_usage_on()).map(|u| self.usage_row(m.id.to_string(), u));
        // Not while it streams: no extra work per delta, and it never folds under the reader.
        let (folded, fold_toggle) = if m.status == MessageStatus::Partial { (None, None) } else { self.fold_view(m.id, &text, cx) };
        let sources = (!sources.is_empty()).then(|| {
            let ok = sources.iter().filter(|s| s.error.is_none()).count();
            v().gap(px(10.))
                .child(div().text_size(px(12.)).text_color(c(theme::MUTED)).child(format!("{ok} of {} sources fetched", sources.len())))
                .child(h().gap(px(8.)).items_start().flex_wrap().children(sources.iter().map(|s| {
                    let host = s.url.split('/').nth(2).unwrap_or(&s.url).to_string();
                    v().w(px(220.)).gap(px(5.)).px(px(12.)).py(px(10.)).rounded(px(8.)).border_1().border_color(c(theme::BORDER)).bg(c(theme::SURFACE))
                        .child(div().text_size(px(11.)).text_color(c(theme::MUTED)).font_family(ui::mono(cx)).child(format!("{} · {host}", s.n)))
                        .child(div().text_size(px(12.5)).line_height(px(17.)).text_color(c(if s.error.is_some() { theme::MUTED } else { theme::TEXT })).child(
                            s.error.clone().map(|e| format!("Not fetched: {e}")).or(s.title.clone()).unwrap_or_else(|| s.url.clone()),
                        ))
                })))
        });
        let mut actions: Vec<AnyElement> = Vec::new();
        // The feed row places the pill itself.
        if !group {
            actions.extend(self.message_ticket_pill(m.id, cx));
        }
        actions.extend((m.status != MessageStatus::Partial).then(|| self.make_ticket_action(m, cx)).flatten());
        actions.extend(trace.map(IntoElement::into_any_element));
        actions.extend(copy.map(IntoElement::into_any_element));
        let body = if text.is_empty() && m.status == MessageStatus::Partial {
            ui::thinking_steps().into_any_element()
        } else {
            let mut activity = self.live_activity.get(&m.id).cloned().unwrap_or_else(|| m.activity.clone());
            let text = match folded {
                // Only the steps taken within the shown lines.
                Some((shown, cut)) => {
                    activity.retain(|a| (a.offset as usize) < cut);
                    shown
                }
                None => text,
            };
            self.answer_body(m.id, if text.is_empty() && activity.is_empty() { "…".to_string() } else { text }, activity, cx)
        };
        let files = (!m.files.is_empty()).then(|| self.message_files(m.id, &m.files, cx));
        let status = div().text_size(px(11.5)).text_color(c(theme::MUTED)).child(status);
        // The Team chat: the flat row of the feed (ui-ontwerp §2), the answer under its employee's name.
        if group {
            let who = crate::screens::discussion::author(m, |name| self.employee_role(name).map(str::to_owned));
            let mentions_me = crate::screens::discussion::names_me(&text_of(m, &self.live), self.me_name().as_deref());
            let tags = h().gap(px(6.)).text_size(px(11.5)).text_color(c(theme::TEXT_2))
                .when_some(m.mode, |el, mode| el.child(mode_badge(mode)))
                .when_some(m.produced_by.clone(), |el, label| el.child(format!("· {label}")))
                .when(total > 1, |el| el.child(format!("· version {}/{}", index.unwrap_or(1), total)));
            let content: Vec<AnyElement> = [sources.map(IntoElement::into_any_element), Some(body), fold_toggle.map(IntoElement::into_any_element), files, Some(status.into_any_element()), usage.map(IntoElement::into_any_element)].into_iter().flatten().collect();
            return self.feed_row(m, &who, grouped, mentions_me, tags.into_any_element(), actions, content, cx);
        }
        v().gap(px(10.))
            .children(sources)
            .child(
                h().gap(px(8.)).text_size(px(12.)).text_color(c(theme::MUTED))
                    .child(self.author_label(m, author))
                    // A group chat answer: the mode the employee answered in.
                    .when_some(m.mode, |el, mode| el.child(mode_badge(mode)))
                    .when_some(m.produced_by.clone(), |el, label| el.child(div().text_color(c(theme::TEXT_2)).child(format!("· {label}"))))
                    .when(total > 1, |el| el.child(format!("· version {}/{}", index.unwrap_or(1), total)))
                    .child(div().flex_1())
                    .children(actions),
            )
            .child(body)
            .children(fold_toggle)
            .children(files)
            .child(status)
            .children(usage)
            .into_any_element()
    }

    // ---- The Team chat's feed (AUR-CHAT-001, ui-ontwerp §2) ------------------------

    /// One row of the feed: monogram, name, what they are and the time over the body,
    /// which aligns with the name; a grouped row only the body, its time and actions
    /// in the gutter on hover. A message naming the reader: "@ you", an ACCENT bar and
    /// the halo behind the row. With threads or reactions (ui-ontwerp §5) the actions
    /// sit in a bar at the top right (hover, keyboard focus), the reactions and the
    /// thread line under the text.
    #[allow(clippy::too_many_arguments)]
    fn feed_row(&self, m: &aurelius_protocol::MessageView, who: &crate::screens::discussion::Author, grouped: bool, mentions_me: bool, tags: AnyElement, actions: Vec<AnyElement>, content: Vec<AnyElement>, cx: &mut Context<Self>) -> AnyElement {
        let clock = feed::clock(m.created_at_ms, aurelius_host::utc_offset_minutes_at(m.created_at_ms));
        let label = format!("{}, {}, {}{}", who.name, who.kind, ui::fmt_when(m.created_at_ms), if mentions_me { ", mentions you" } else { "" });
        let hover = SharedString::from(format!("feed-{}", m.id));
        let kind = if who.human { "· board member".to_string() } else { format!("— {}", who.kind) };
        let pill = self.message_ticket_pill(m.id, cx);
        let toolbar = self.row_toolbar_on();
        let (actions, bar) = if toolbar {
            (pill.into_iter().collect::<Vec<_>>(), Some(self.row_toolbar(m, hover.clone(), actions, cx)))
        } else {
            (pill.into_iter().chain(actions).collect(), None)
        };
        let mut content = content;
        if toolbar {
            content.extend(self.reactions_row(m, cx));
            content.extend(self.thread_summary(m, cx));
        }
        let row = v().id(SharedString::from(format!("feed-row-{}", m.id))).group(hover.clone()).role(Role::Article).aria_label(label).w_full().relative().gap(px(4.)).rounded(px(4.))
            .when(mentions_me, |el| el.pl(px(8.)).py(px(4.)).border_l_2().border_color(c(theme::ACCENT)).bg(c(theme::ACCENT_HALO)));
        if grouped {
            return row
                .child(
                    h().items_start()
                        .child(div().w(px(28.)).flex_none().pt(px(3.)).text_size(px(10.5)).text_color(c(theme::MUTED)).opacity(0.).group_hover(hover.clone(), |s| s.opacity(1.)).child(clock))
                        .child(v().flex_1().min_w_0().gap(px(6.)).children(content))
                        .child(h().flex_none().gap(px(2.)).when(!toolbar, |el| el.opacity(0.).group_hover(hover, |s| s.opacity(1.))).children(actions)),
                )
                .children(bar)
                .into_any_element();
        }
        row.child(
            h().gap(px(8.))
                .child(if who.human { ui::member_monogram(&who.name, 20., cx) } else { ui::monogram(&who.name, 20., cx) })
                .child(div().text_size(px(12.5)).font_weight(FontWeight::MEDIUM).child(who.name.clone()))
                .child(div().text_size(px(11.5)).text_color(c(theme::TEXT_2)).child(kind))
                .child(tags)
                .when(mentions_me, |el| el.child(div().text_size(px(11.)).text_color(c(theme::ACCENT_TEXT)).child("@ you")))
                .child(div().flex_1())
                .children(actions)
                .child(div().text_size(px(11.)).text_color(c(theme::TEXT_2)).child(clock)),
        )
        .child(v().pl(px(28.)).gap(px(6.)).children(content))
        .children(bar)
        .into_any_element()
    }

    /// A board member's post in the feed: everyone left-aligned, yours too (ui-ontwerp §2).
    fn feed_post(&self, m: &aurelius_protocol::MessageView, grouped: bool, cx: &mut Context<Self>) -> AnyElement {
        let who = crate::screens::discussion::author(m, |_| None);
        let mentions_me = crate::screens::discussion::names_me(&m.text, self.me_name().as_deref());
        let (folded, fold_toggle) = self.fold_view(m.id, &m.text, cx);
        let shown = folded.map_or_else(|| m.text.clone(), |(s, _)| s);
        let mut content: Vec<AnyElement> = Vec::new();
        if !m.text.trim().is_empty() {
            content.push(div().text_size(px(14.)).line_height(px(21.)).text_color(c(theme::TEXT)).child(self.feed_markdown(m.id, format!("user-{}", m.id), &shown, cx)).into_any_element());
        }
        content.extend(fold_toggle.map(IntoElement::into_any_element));
        if !m.files.is_empty() {
            content.push(self.message_files(m.id, &m.files, cx));
        }
        if !m.uploads.is_empty() {
            content.push(self.message_uploads(m.id, &m.uploads, cx));
        }
        content.extend(self.markdown_chips(m.id, &m.text, cx));
        // Sent while an answer runs and still waiting behind it (an older server queues it).
        if m.queued {
            let id = m.id;
            content.push(
                h().gap(px(8.)).items_center().text_size(px(12.)).text_color(c(theme::MUTED))
                    .child(ui::diamond(7., theme::MUTED, false))
                    .child("Waiting · sends after the current answer")
                    .child(send_now_button(SharedString::from(format!("queued-delete-{id}")), "Delete").text_color(c(theme::TEXT_2)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.delete_queued(id, cx))))
                    .child(send_now_button(SharedString::from(format!("queued-send-{id}")), "Send now").on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.send_queued_now(id, cx))))
                    .into_any_element(),
            );
        }
        // The feed row places the "→ #52" pill itself.
        let mut actions: Vec<AnyElement> = Vec::new();
        actions.extend(self.make_ticket_action(m, cx));
        if !m.text.trim().is_empty() {
            actions.push(self.copy_button(format!("msg-{}", m.id), m.text.clone(), cx).into_any_element());
        }
        self.feed_row(m, &who, grouped, mentions_me, div().into_any_element(), actions, content, cx)
    }

    /// Markdown of the feed with its `@names` marked (`feed::mark_mentions`).
    fn feed_markdown(&self, message: MessageId, key: String, text: &str, cx: &mut Context<Self>) -> gpui_kit::component::text::TextView {
        let marked = self.mark_mentions(text);
        self.message_markdown(message, key, crate::screens::markdown::linked(&self.with_link_pills(&marked)), cx).on_link_click(self.markdown_link_handler(false, cx))
    }

    /// `text` with the `@names` of the people here marked, the reader's own stronger.
    pub fn mark_mentions(&self, text: &str) -> String {
        if !text.contains('@') {
            return text.to_string();
        }
        let people = self.mentionables();
        let names: Vec<&str> = people.iter().map(|p| p.name.as_str()).collect();
        feed::mark_mentions(text, &names, self.me_name().as_deref())
    }

    /// A system line of the feed (ui-ontwerp §2, O-4): one compact line with a shape
    /// by its action, else its kind; a ticket's line in General links to its channel.
    fn feed_line(&self, m: &aurelius_protocol::MessageView, line: &aurelius_domain::views::SystemLine, cx: &mut Context<Self>) -> AnyElement {
        let number = line.ticket_id.and_then(|t| self.channels.iter().flatten().find(|c| c.ticket_id == Some(t))).and_then(|c| c.ticket_number);
        let text = feed::line_text(line, &m.text, number);
        let mark = match crate::screens::discussion::login_line(line, &text) {
            Some(login) => feed::LineMark { mark: crate::screens::discussion::login_mark(Some(login)), failed: false },
            None => feed::line_mark(line, &text),
        };
        let clock = feed::clock(m.created_at_ms, aurelius_host::utc_offset_minutes_at(m.created_at_ms));
        let connect = crate::screens::discussion::login_line(line, &text) == Some(crate::screens::discussion::LoginLine::Connect) && crate::screens::account::needs_connect(self.claude.view.as_ref(), self.claude.unsupported.is_some());
        let ticket = line.ticket_id.filter(|_| line.kind == aurelius_domain::views::SystemLineKind::TicketCreated);
        // The ticket's channel, from another channel (General's opened / closed lines).
        let open = self.current_channel().map(|c| c.conversation_id);
        let channel = line.channel_id.filter(|id| Some(*id) != open && self.channels.iter().flatten().any(|c| c.conversation_id == *id));
        let task = crate::screens::discussion::system_task(line);
        let color = if mark.failed { theme::NEGATIVE } else { theme::TEXT_2 };
        h().id(SharedString::from(format!("system-{}", m.id))).w_full().justify_center().flex_wrap().gap(px(8.)).px(px(24.)).role(Role::Note).aria_label(crate::screens::discussion::system_label(&text, &ui::fmt_when(m.created_at_ms)))
            .child(div().text_size(px(11.5)).line_height(px(17.)).text_color(c(color)).text_center().child(format!("┄ {}{} · {clock} ┄", mark.mark, text.lines().next().unwrap_or_default())))
            .when(connect, |el| {
                el.child(div().id(SharedString::from(format!("system-connect-{}", m.id))).role(Role::Button).cursor_pointer().text_size(px(11.5)).text_color(c(theme::ACCENT_TEXT)).child(crate::screens::board::connect_label(self.claude.view.as_ref())).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.claude_connect(cx))))
            })
            .when_some(ticket, |el, id| {
                el.child(div().id(SharedString::from(format!("system-open-{}", m.id))).role(Role::Link).cursor_pointer().text_size(px(11.5)).text_color(c(theme::ACCENT_TEXT)).child("Open ticket").on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.open_ticket(id, w, cx))))
            })
            .when_some(channel, |el, id| {
                el.child(div().id(SharedString::from(format!("system-channel-{}", m.id))).role(Role::Link).cursor_pointer().text_size(px(11.5)).text_color(c(theme::ACCENT_TEXT)).child("Open channel").on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.go_to_channel(id, w, cx))))
            })
            .when_some(task, |el, task| {
                el.child(div().id(SharedString::from(format!("system-task-{}", m.id))).role(Role::Link).cursor_pointer().text_size(px(11.5)).text_color(c(theme::ACCENT_TEXT)).child("Open task").on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.open_ticket_task(task, w, cx))))
            })
            .into_any_element()
    }
}

/// A moment's local day, for the feed's day separators and grouping.
fn feed_day(ms: i64) -> i64 {
    feed::local_day(ms, aurelius_host::utc_offset_minutes_at(ms))
}

/// A message's text as shown: the streamed text while it streams.
fn text_of(m: &aurelius_protocol::MessageView, live: &std::collections::HashMap<MessageId, String>) -> String {
    live.get(&m.id).cloned().unwrap_or_else(|| m.text.clone())
}

/// A line across the feed with a label in the middle: a day, or "New messages".
pub(crate) fn feed_separator(id: SharedString, label: SharedString, line: u32, text: u32) -> Stateful<Div> {
    h().id(id.clone()).role(Role::Note).aria_label(label.clone()).w_full().gap(px(10.)).text_size(px(11.)).text_color(c(text))
        .child(div().flex_1().h(px(1.)).bg(c(line)))
        .child(label)
        .child(div().flex_1().h(px(1.)).bg(c(line)))
}

impl AppView {
    /// The buttons that decide what a worker asks: answer a question, review a plan, or
    /// approve an action (`key` keeps their ids apart where the same view shows two).
    pub fn approval_actions(&self, a: aurelius_protocol::ApprovalView, key: &str, cx: &mut Context<Self>) -> Div {
        let (id, digest) = (a.id, a.action_digest.clone());
        let reject_digest = digest.clone();
        let el = h().gap(px(8.)).pt(px(4.));
        if a.tool == crate::app::QUESTION_TOOL {
            el.child(ui::button(SharedString::from(format!("skip-{}", a.id)), "Skip", ButtonKind::Outline, None).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.decide_approval(id, reject_digest.clone(), false, cx);
            })))
            .child(ui::button(SharedString::from(format!("send-{}", a.id)), "Send answer", ButtonKind::Primary, None).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                if !this.send_answers(id) {
                    this.notify_user("Answer every question first, or skip.", true, cx);
                }
                cx.notify();
            })))
        } else if a.tool == crate::app::PLAN_TOOL {
            el.child(ui::button(SharedString::from(format!("plan-{}", a.id)), "Review plan", ButtonKind::Outline, None)
                .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.open_plan_approval(a.clone(), w, cx))))
            .child(ui::button(SharedString::from(format!("plan-auto-{id}")), "Goedkeuren + Auto", ButtonKind::Primary, None)
                .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.decide_plan_for(id, aurelius_domain::approval::PlanDecision::Approve, w, cx))))
        } else {
            let always = self.always_allow_button(&a, key, cx);
            el.child(ui::button(SharedString::from(format!("reject-{key}-{}", a.id)), "Reject", ButtonKind::Outline, None).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.decide_approval(id, reject_digest.clone(), false, cx);
            })))
            .children(always)
            .child(ui::button(SharedString::from(format!("approve-{key}-{}", a.id)), "Approve once", ButtonKind::Primary, None).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.decide_approval(id, digest.clone(), true, cx);
            })))
        }
    }

    /// A long message as shown: its first lines and where they end in `text` while folded (None
    /// when shown whole), and the Show more / Show less toggle (None for a short message).
    fn fold_view(&self, id: MessageId, text: &str, cx: &mut Context<Self>) -> (Option<(String, usize)>, Option<Stateful<Div>>) {
        let Some((shown, cut, hidden)) = fold(text) else { return (None, None) };
        let open = self.expanded_messages.contains(&id);
        let toggle = h().id(SharedString::from(format!("fold-{id}"))).role(Role::Button).flex_none().cursor_pointer()
            .text_size(px(12.)).text_color(c(theme::TEXT_2)).hover(|s| s.text_color(c(theme::TEXT)))
            .child(if open { "▴ Show less".to_string() } else { format!("▾ Show more ({hidden} lines)") })
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                if !this.expanded_messages.remove(&id) {
                    this.expanded_messages.insert(id);
                }
                cx.notify();
            }));
        ((!open).then_some((shown, cut)), Some(toggle))
    }

    /// Small copy icon; swaps to a ✓ (named "Copied") for a moment. `text` is copied as is (raw markdown).
    pub fn copy_button(&self, key: String, text: String, cx: &mut Context<Self>) -> Stateful<Div> {
        let done = self.copied.as_deref() == Some(key.as_str());
        let id = SharedString::from(format!("copy-{key}"));
        ui::action_icon(id, if done { "✓" } else { "⧉" }, if done { "Copied" } else { "Copy" }, None, None)
            .when(done, |el| el.text_color(c(theme::ACCENT_TEXT)))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                this.copy_text(key.clone(), text.clone(), cx);
            }))
    }

    /// Files kept with a message: click to show (images) or open (anything else).
    fn message_files(&self, message: MessageId, files: &[aurelius_protocol::FileRef], cx: &mut Context<Self>) -> AnyElement {
        h().flex_wrap().gap(px(8.)).pt(px(2.))
            .children(files.iter().enumerate().map(|(i, f)| {
                let file = f.clone();
                let image = f.media_type.starts_with("image/");
                let size = if f.size >= 1_048_576 { format!("{:.1} MB", f.size as f64 / 1_048_576.0) } else { format!("{} KB", (f.size / 1024).max(1)) };
                h().id(SharedString::from(format!("file-{message}-{i}"))).gap(px(8.)).px(px(10.)).py(px(6.)).rounded(px(7.)).border_1().border_color(c(theme::BORDER)).bg(c(theme::SURFACE))
                    .cursor_pointer().hover(|s| s.border_color(c(theme::BORDER_STRONG)))
                    .child(div().text_size(px(13.)).text_color(c(theme::ACCENT_TEXT)).child(if image { "▣" } else { "▤" }))
                    .child(div().text_size(px(12.5)).child(f.name.clone()))
                    .child(div().text_size(px(11.)).text_color(c(theme::MUTED)).child(size))
                    .child(div().text_size(px(11.5)).text_color(c(theme::ACCENT_TEXT)).child(if image { "View" } else { "Open" }))
                    .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.open_message_file(file.clone(), w, cx)))
            }))
            .into_any_element()
    }

    /// Files the user sent to a manager: small chips (name, size), images with a
    /// thumbnail. A click opens the copy in the work folder on a local server,
    /// else the file fetched from the server (when it can).
    fn message_uploads(&self, message: MessageId, files: &[aurelius_protocol::MessageFileView], cx: &mut Context<Self>) -> AnyElement {
        let local = self.server_is_local();
        let fetchable = self.uploads_fetchable();
        h().flex_wrap().justify_end().gap(px(6.))
            .children(files.iter().enumerate().map(|(i, f)| {
                let path = f.path.clone();
                let markdown = crate::screens::markdown::resolve_link(None, &f.path).is_some();
                let thumb = f.id.as_ref().and_then(|id| self.upload_thumbs.get(id)).cloned().flatten();
                let remote = (!local && !markdown && fetchable && f.id.is_some()).then(|| f.clone());
                h().id(SharedString::from(format!("upload-{message}-{i}"))).gap(px(6.)).px(px(9.)).py(px(4.)).rounded(px(6.)).border_1().border_color(c(theme::BORDER)).bg(c(theme::SURFACE)).text_size(px(12.))
                    .tooltip(ui::tip(f.path.clone(), None))
                    .child(match thumb {
                        Some(image) => img(ImageSource::from(image)).w(px(36.)).h(px(28.)).object_fit(ObjectFit::Cover).rounded(px(4.)).into_any_element(),
                        None => div().text_color(c(theme::ACCENT_TEXT)).child("▤").into_any_element(),
                    })
                    .child(f.name.clone())
                    .child(div().text_color(c(theme::MUTED)).child(crate::app::file_size(f.byte_length)))
                    .when(markdown, |el| {
                        // A Markdown file opens in the viewer, also on a remote server.
                        let path = path.clone();
                        el.cursor_pointer().hover(|s| s.border_color(c(theme::BORDER_STRONG)))
                            .child(div().text_color(c(theme::ACCENT_TEXT)).child("View"))
                            .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.markdown_link(&path, false, w, cx)))
                    })
                    .when(local && !markdown, |el| {
                        el.cursor_pointer().hover(|s| s.border_color(c(theme::BORDER_STRONG)))
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.open_upload(&path, cx)))
                    })
                    .when_some(remote, |el, file| {
                        el.cursor_pointer().hover(|s| s.border_color(c(theme::BORDER_STRONG)))
                            .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.open_upload_file(&file, w, cx)))
                    })
            }))
            .into_any_element()
    }

    /// An answer: Markdown text with the tools it used shown in place, as
    /// separate blocks (commands, edits, web searches), so they stand apart
    /// from what the assistant writes.
    fn answer_body(&self, id: MessageId, text: String, mut activity: Vec<aurelius_protocol::ActivityView>, cx: &mut Context<Self>) -> AnyElement {
        activity.sort_by_key(|a| a.offset);
        // The checklist and the quick replies show as cards of their own, not as steps.
        let todo = crate::screens::progress::todos(&activity);
        activity.retain(|a| !crate::screens::progress::is_progress_tool(&a.tool));
        // The Team chat's feed: smaller type, and the `@names` marked (ui-ontwerp §2).
        let group = self.screen == crate::app::Page::GroupChat;
        let md = |this: &Self, segment: &str, n: usize, cx: &mut Context<Self>| {
            let segment = if group { this.mark_mentions(segment) } else { segment.to_string() };
            div().text_size(px(if group { 14. } else { 15. })).line_height(px(if group { 21. } else { 25.5 })).child(
                this.message_markdown(id, format!("md-{id}-{n}"), crate::screens::markdown::linked(&this.with_link_pills(&segment)), cx)
                    .on_link_click(this.markdown_link_handler(false, cx)),
            )
        };
        let mut parts: Vec<AnyElement> = Vec::new();
        let mut pos = 0usize;
        // Steps with nothing written between them share one line.
        let mut run: Vec<&aurelius_protocol::ActivityView> = Vec::new();
        for (n, a) in activity.iter().enumerate() {
            let mut at = (a.offset as usize).min(text.len());
            while !text.is_char_boundary(at) {
                at -= 1;
            }
            if at > pos && !text[pos..at].trim().is_empty() {
                if !run.is_empty() {
                    parts.push(self.activity_run(id, &std::mem::take(&mut run), cx));
                }
                parts.push(md(self, &text[pos..at], n, cx).into_any_element());
            }
            pos = pos.max(at);
            run.push(a);
        }
        if !run.is_empty() {
            parts.push(self.activity_run(id, &run, cx));
        }
        if pos < text.len() && !text[pos..].trim().is_empty() {
            parts.push(md(self, &text[pos..], activity.len(), cx).into_any_element());
        }
        let chips = self.markdown_chips(id, &text, cx);
        let todo = todo.map(|list| self.todo_card(id, &list, cx));
        v().gap(px(10.)).children(parts).children(chips).children(todo).into_any_element()
    }

    /// Steps taken one after another without a word in between: one line
    /// that shows the latest, with a count to unfold them all.
    fn activity_run(&self, id: MessageId, run: &[&aurelius_protocol::ActivityView], cx: &mut Context<Self>) -> AnyElement {
        let [.., last] = run else { return div().into_any_element() };
        if run.len() == 1 {
            return self.activity_card(id, last, cx);
        }
        let key = format!("run-{}", run[0].call_id);
        let expanded = self.expanded_activity.contains(&key);
        let failed = run.iter().filter(|a| a.ok == Some(false)).count();
        let toggle = h().id(SharedString::from(key.clone()))
            .flex_none().gap(px(4.)).px(px(7.)).h(px(20.)).rounded(px(5.)).border_1().border_color(c(theme::BORDER))
            .text_size(px(11.5)).text_color(c(if failed > 0 { theme::NEGATIVE } else { theme::MUTED }))
            .cursor_pointer().hover(|s| s.border_color(c(theme::BORDER_STRONG)).text_color(c(theme::TEXT)))
            .child(crate::screens::progress::run_label(run))
            .child(if expanded { "⌄" } else { "›" })
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                if !this.expanded_activity.remove(&key) {
                    this.expanded_activity.insert(key.clone());
                }
                cx.notify();
            }));
        if expanded {
            v().gap(px(10.)).child(toggle).children(run.iter().map(|a| self.activity_card(id, a, cx))).into_any_element()
        } else {
            h().gap(px(10.)).items_start().child(toggle).child(div().flex_1().min_w_0().child(self.activity_card(id, last, cx))).into_any_element()
        }
    }

    /// One tool use in an answer, like Claude Code shows it: what it did, in
    /// words, on top; click to unfold the tool, its command and its output.
    pub(crate) fn activity_card(&self, message: MessageId, a: &aurelius_protocol::ActivityView, cx: &mut Context<Self>) -> AnyElement {
        // A shortened call (feature `activity_preview`) shows whole once fetched; older servers never shorten.
        let full = self.full_activity.get(&a.call_id);
        let loading = matches!(full, Some(None));
        let a = full.and_then(Option::as_ref).unwrap_or(a);
        let field = |k: &str| a.input[k].as_str().unwrap_or("").trim().to_string();
        let short = |path: String| std::path::Path::new(&path).file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or(path);
        let first_line = |t: String| t.lines().next().unwrap_or("").chars().take(140).collect::<String>();
        let described = |fallback: String| Some(field("description")).filter(|d| !d.is_empty()).unwrap_or(fallback);
        // (what it did, the tool's name, what it ran on)
        let (summary, tool, code): (String, &str, String) = match a.tool.as_str() {
            "Bash" => (described(first_line(field("command"))), "Bash", field("command")),
            "PowerShell" => (described(first_line(field("command"))), "PowerShell", field("command")),
            "Edit" | "MultiEdit" => (format!("Edited {}", short(field("file_path"))), "Edit", field("file_path")),
            "Write" => (format!("Wrote {}", short(field("file_path"))), "Write", field("file_path")),
            "NotebookEdit" => (format!("Edited {}", short(field("notebook_path"))), "Edit", field("notebook_path")),
            "Read" => (format!("Read {}", short(field("file_path"))), "Read", field("file_path")),
            "Glob" => (format!("Looked for files: {}", field("pattern")), "Glob", field("pattern")),
            "Grep" => (format!("Searched in files: {}", field("pattern")), "Grep", field("pattern")),
            "WebSearch" => (format!("Searched the web: {}", field("query")), "Web search", field("query")),
            "WebFetch" => (format!("Opened {}", field("url")), "Web fetch", field("url")),
            "Task" | "Agent" => (described("A subagent".into()), "Subagent", field("prompt")),
            "Artifact" => {
                let action = field("action");
                let name = Some(field("description")).filter(|d| !d.is_empty()).unwrap_or_else(|| short(field("file_path")));
                let summary = match (action.as_str(), a.ok) {
                    ("" | "publish", Some(true)) => {
                        let title = artifact_url(a.output.as_deref().unwrap_or_default()).and_then(|u| self.artifacts.iter().flatten().find(|x| x.url == u)).map(|x| x.title.clone());
                        format!("Published the artifact \"{}\"", title.unwrap_or(name))
                    }
                    ("" | "publish", _) if !field("url").is_empty() => format!("Publishing a new version of the artifact: {name}"),
                    ("" | "publish", _) => format!("Publishing an artifact to claude.ai: {name}"),
                    ("list", _) => "Looked at the published artifacts".to_string(),
                    (other, _) => format!("Artifact: {other} {}", field("url")).trim_end().to_string(),
                };
                (summary, "Artifact", field("file_path"))
            }
            "ArtifactData" => (format!("Artifact data: {}", field("action")), "Artifact data", String::new()),
            "ArtifactComments" => (format!("Artifact comments: {}", field("action")), "Artifact comments", String::new()),
            "mcp__aurelius__screenshot" => (
                match (a.input["window"].as_str(), a.input["monitor"].as_u64()) {
                    (Some(w), _) if !w.is_empty() => format!("Screenshot of window \"{w}\""),
                    (_, Some(n)) => format!("Screenshot of monitor {n}"),
                    _ => "Screenshot of the screen".to_string(),
                },
                "Screenshot",
                String::new(),
            ),
            "mcp__aurelius__list_windows" => ("Looked at the open windows".to_string(), "Windows", String::new()),
            "mcp__aurelius__remember" => ("Noted in memory".to_string(), "Memory", field("content")),
            "mcp__aurelius__recall" => (format!("Looked in memory: {}", field("query")), "Memory", field("query")),
            // `employee` since protocol 3; older history says `manager`.
            "mcp__aurelius__delegate_task" => (format!("Gave \"{}\" to {}", field("title"), Some(field("employee")).filter(|e| !e.is_empty()).unwrap_or_else(|| field("manager"))), "Delegate", field("goal")),
            "mcp__aurelius__hand_over_task" => (format!("Handed the task to {}", Some(field("employee")).filter(|e| !e.is_empty()).unwrap_or_else(|| field("manager"))), "Hand over", field("reason")),
            "mcp__aurelius__add_idea" => (format!("Noted an idea: {}", field("title")), "Idea", field("note")),
            "mcp__aurelius__list_tasks" => ("Looked at the project's tasks".to_string(), "Tasks", String::new()),
            "mcp__aurelius__read_task" => (format!("Read the task {}", field("task")), "Task", String::new()),
            "mcp__aurelius__list_conversations" => ("Looked at its other conversations".to_string(), "Conversations", String::new()),
            "mcp__aurelius__read_conversation" => (format!("Read the conversation {}", field("conversation")), "Conversation", String::new()),
            other => {
                let name = other.strip_prefix("mcp__aurelius__").or_else(|| other.strip_prefix("mcp__claude-in-chrome__")).unwrap_or(other);
                (described(name.replace('_', " ")), name, if a.input.is_null() { String::new() } else { serde_json::to_string_pretty(&a.input).unwrap_or_default() })
            }
        };
        let mono = ui::mono(cx);
        let expanded = self.expanded_activity.contains(&a.call_id);
        let output = match a.tool.as_str() {
            "WebSearch" => web_search_summary(a.output.as_deref().unwrap_or_default()),
            _ => a.output.clone().unwrap_or_default(),
        };
        let lines: Vec<&str> = output.lines().collect();
        let shown = lines.len().min(400);
        let more = lines.len() - shown;
        let screenshot = (a.tool == "mcp__aurelius__screenshot").then(|| a.output.as_deref().and_then(aurelius_protocol::screenshot_id)).flatten().map(str::to_owned);
        let artifact = (a.tool == "Artifact" && a.ok == Some(true)).then(|| artifact_url(&output)).flatten().map(str::to_owned);
        let show_output = !output.trim().is_empty() && !matches!(a.tool.as_str(), "Read" | "Edit" | "MultiEdit" | "Write" | "mcp__aurelius__screenshot");
        // Only offer the whole call when a cut part is on screen: a cut input string is at least ~1 KB.
        let shortened = a.truncated && ((show_output && a.output_bytes.is_some()) || code.len() + 4 >= aurelius_domain::activity::INPUT_STRING_PREVIEW_BYTES);
        let size_note = match a.output_bytes.filter(|_| show_output) {
            Some(total) => format!("Showing {} of {}", crate::app::file_size(a.output.as_deref().map_or(0, str::len) as u64), crate::app::file_size(total.into())),
            None => "Shortened".to_string(),
        };
        let status: AnyElement = match a.ok {
            None => h().gap(px(6.)).text_size(px(11.5)).text_color(c(theme::MUTED)).child(ui::blink_dot(6., theme::ACCENT)).child("running").into_any_element(),
            Some(true) => div().text_size(px(12.)).text_color(c(theme::ACCENT_TEXT)).child("✓").into_any_element(),
            Some(false) => div().text_size(px(12.)).text_color(c(theme::NEGATIVE)).child("✗ failed").into_any_element(),
        };
        let call = a.call_id.clone();
        v().id(SharedString::from(format!("activity-{}", a.call_id)))
            .gap(px(8.))
            .child(
                h().id(SharedString::from(format!("activity-head-{}", a.call_id)))
                    .gap(px(8.))
                    .cursor_pointer()
                    .text_color(c(if a.ok == Some(false) { theme::NEGATIVE } else { theme::TEXT_2 }))
                    .hover(|s| s.text_color(c(theme::TEXT)))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        if !this.expanded_activity.remove(&call) {
                            this.expanded_activity.insert(call.clone());
                        }
                        cx.notify();
                    }))
                    .child(div().min_w_0().text_size(px(14.)).line_height(px(20.)).truncate().child(summary))
                    .child(div().flex_none().text_size(px(12.)).text_color(c(theme::MUTED)).child(if expanded { "⌄" } else { "›" }))
                    .when_some(screenshot, |el, id| {
                        el.child(ui::button(SharedString::from(format!("view-shot-{}", a.call_id)), "View", ButtonKind::Outline, None).h(px(24.)).on_click(cx.listener(move |this, _: &ClickEvent, w, cx| {
                            cx.stop_propagation();
                            this.open_screenshot(id.clone(), w, cx);
                        })))
                    })
                    .when_some(artifact, |el, url| {
                        let open = url.clone();
                        let kept = self.artifacts.iter().flatten().any(|x| x.url == url && crate::screens::artifact::can_view(x));
                        let view = url.clone();
                        el.when(kept, |el| {
                            el.child(ui::button(SharedString::from(format!("view-artifact-{}", a.call_id)), "View", ButtonKind::Outline, None).h(px(24.)).on_click(cx.listener(move |this, _: &ClickEvent, w, cx| {
                                cx.stop_propagation();
                                this.open_artifact(&view, w, cx);
                            })))
                        })
                        .child(ui::button(SharedString::from(format!("open-artifact-{}", a.call_id)), "Open", ButtonKind::Outline, None).h(px(24.)).tooltip(ui::tip("Open in browser", None)).on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                            cx.stop_propagation();
                            cx.open_url(&open);
                        })))
                        .child(self.copy_button(format!("artifact-{}", a.call_id), url, cx).tooltip(ui::tip("Copy link", None)))
                    })
                    .child(div().flex_1())
                    .child(status),
            )
            .when(expanded, |el| {
                el.child(
                    v().gap(px(8.)).px(px(14.)).py(px(12.)).rounded(px(8.)).border_1()
                        .border_color(c(if a.ok == Some(false) { theme::NEGATIVE } else { theme::BORDER }))
                        .bg(c(theme::SURFACE))
                        .child(h().gap(px(8.)).items_center()
                            .child(div().flex_1().text_size(px(12.5)).text_color(c(theme::TEXT_2)).child(tool.to_string()))
                            .when(shortened, |el| {
                                let (call, fetch) = (a.call_id.clone(), a.call_id.clone());
                                el.child(div().text_size(px(11.5)).text_color(c(theme::MUTED)).child(size_note))
                                    .child(ui::button(SharedString::from(format!("full-{call}")), if loading { "Loading…" } else { "Show full output" }, ButtonKind::Outline, None).h(px(24.)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                        if !this.full_activity.contains_key(&fetch) {
                                            this.full_activity.insert(fetch.clone(), None);
                                            let _ = this.requests.send(crate::backend::Request::LoadFullActivity(message, fetch.clone()));
                                            cx.notify();
                                        }
                                    })))
                            })
                            .when(!code.is_empty() && !shortened, |el| el.child(self.copy_button(format!("code-{}", a.call_id), code.clone(), cx)))
                            // A shortened output is copied whole, so only once it is fetched.
                            .when(show_output && !shortened, |el| el.child(self.copy_button(format!("out-{}", a.call_id), output.clone(), cx))))
                        .when(!code.is_empty(), |el| {
                            el.child(
                                div().px(px(12.)).py(px(9.)).rounded(px(6.)).bg(c(theme::BG)).font_family(mono.clone()).text_size(px(12.)).line_height(px(18.)).text_color(c(theme::TEXT))
                                    .child(format!("> {code}")),
                            )
                        })
                        // Web search results are a Markdown list of links; other output stays raw text.
                        .when(show_output && a.tool == "WebSearch", |el| {
                            el.child(div().text_size(px(12.)).text_color(c(theme::TEXT_2)).child(
                                gpui_kit::component::text::TextView::markdown(SharedString::from(format!("out-md-{}", a.call_id)), output.clone()).selectable(true)
                                    .on_link_click(self.markdown_link_handler(false, cx)),
                            ))
                        })
                        .when(show_output && a.tool != "WebSearch", |el| {
                            el.child(div().font_family(mono.clone()).text_size(px(11.5)).line_height(px(16.)).text_color(c(theme::TEXT_2)).child(lines[..shown].join("\n")))
                                .when(more > 0, |el| el.child(div().text_size(px(11.)).text_color(c(theme::MUTED)).child(format!("… {more} more lines"))))
                        }),
                )
            })
            .into_any_element()
    }
}

/// The claude.ai link in an Artifact publish result ("Published … at https://claude.ai/artifact/<id> (Version 1, …").
pub fn artifact_url(output: &str) -> Option<&str> {
    let start = output.find("https://claude.ai/artifact/")?;
    let rest = &output[start..];
    Some(&rest[..rest.find(|ch: char| ch.is_whitespace() || matches!(ch, ')' | ',' | '"' | '\'')).unwrap_or(rest.len())])
}

/// "Links: [{title, url}, …]" from a web search result as "title — site" lines.
fn web_search_summary(output: &str) -> String {
    let links = output.find("Links: [").and_then(|start| {
        let json = &output[start + "Links: ".len()..];
        let end = json.find("}]")? + 2;
        serde_json::from_str::<Vec<serde_json::Value>>(&json[..end]).ok()
    });
    match links {
        Some(links) if !links.is_empty() => links
            .iter()
            .map(|l| {
                let url = l["url"].as_str().unwrap_or_default();
                let site = url.split('/').nth(2).unwrap_or(url).trim_start_matches("www.");
                format!("- [{}](<{url}>) — {site}", l["title"].as_str().unwrap_or(url).replace(['[', ']'], ""))
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => output.to_string(),
    }
}

/// How many calendar days ago `ms` was (0 = today).
pub(crate) fn days_ago(ms: i64, now_ms: i64) -> i64 {
    let day = |t: i64| (t + 2 * 3_600_000).div_euclid(86_400_000);
    day(now_ms) - day(ms)
}

pub(crate) fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// The composer's "Send now" look, also used on a waiting message.
fn send_now_button(id: SharedString, label: &'static str) -> Stateful<Div> {
    h().id(id).flex_none().gap(px(6.)).px(px(9.)).h(px(28.)).items_center().rounded(px(7.)).cursor_pointer()
        .border_1().border_color(c(theme::BORDER)).text_size(px(12.5)).text_color(c(theme::TEXT))
        .hover(|s| s.bg(c(theme::SURFACE_2)))
        .child(label)
}

pub fn user_bubble(text: impl Into<SharedString>) -> Div {
    bubble(user_text(text.into()), true)
}

fn user_text(text: SharedString) -> gpui_kit::component::text::TextView {
    let id = {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        text.hash(&mut h);
        h.finish() as usize
    };
    gpui_kit::component::text::TextView::markdown(("user-md", id), text).selectable(true).plugin(crate::screens::mermaid::Mermaid)
}

/// A posted message's bubble: yours (`mine`) on the right, another board member's on the left.
fn bubble(text: gpui_kit::component::text::TextView, mine: bool) -> Div {
    let el = div();
    let el = if mine { el.self_end() } else { el.self_start() };
    el.max_w(px(520.))
        .bg(c(theme::SURFACE_2))
        .border_1()
        .border_color(c(theme::BORDER))
        .rounded_tl(px(14.))
        .rounded_tr(px(14.))
        .rounded_bl(px(if mine { 14. } else { 4. }))
        .rounded_br(px(if mine { 4. } else { 14. }))
        .px(px(15.))
        .py(px(11.))
        .text_size(px(14.))
        .line_height(px(21.7))
        .child(text)
}

/// Message composer. Enter sends, Shift+Enter adds a line, Esc returns to keyboard navigation; a
/// pasted image (or copied image file) goes with the message.
pub fn composer(view: &AppView, _placeholder: &str, tall: bool, cx: &mut Context<AppView>) -> impl IntoElement {
    let this = cx.entity().downgrade();
    // The Team chat (ui-ontwerp §4, O-2): Enter sends, Shift+Enter is a new line, Alt+Enter is Send now.
    let group = view.screen == crate::app::Page::GroupChat;
    let suggesting = group && !view.group_suggestions(cx).is_empty();
    // Same type metrics as ui::input (22 px lines), but it grows with the text.
    let input = gpui_kit::component::input::Textarea::new(&view.composer).py(px(7.)).line_height(px(22.)).appearance(false).on_paste(move |item, _, cx| {
        this.update(cx, |this, cx| this.paste_into_composer(item, cx)).unwrap_or(false)
    });
    let input_box = h()
        .w_full()
        .gap(px(10.))
        .border_1()
        .border_color(c(theme::BORDER))
        .bg(c(theme::SURFACE))
        .rounded(px(12.))
        .pl(px(16.))
        .pr(px(10.))
        .py(px(if tall { 10. } else { 6. }))
        .key_context("Composer")
        // While the `@` suggestions show, ↑↓ / Enter / Tab / Esc are theirs (ui-ontwerp §4).
        .child(div().flex_1().when(suggesting, |el| {
            el.key_context(MENTIONS)
                .on_action(cx.listener(|this, _: &MentionUp, _, cx| this.group_step_mention(false, cx)))
                .on_action(cx.listener(|this, _: &MentionDown, _, cx| this.group_step_mention(true, cx)))
                .on_action(cx.listener(|this, _: &MentionPick, w, cx| this.group_pick_mention(w, cx)))
                .on_action(cx.listener(|this, _: &MentionClose, _, cx| this.group_close_mentions(cx)))
        }).child(input))
        .child(
            h().id("add-image").flex_none().px(px(7.)).h(px(28.)).items_center().rounded(px(7.)).cursor_pointer()
                .text_size(px(12.5)).text_color(c(theme::MUTED))
                .hover(|s| s.bg(c(theme::SURFACE_2)).text_color(c(theme::TEXT)))
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.pick_images(cx)))
                .child("▣ Image"),
        )
        // While an answer runs, Enter interrupts it with the message (stop and send in one go);
        // the arrow button and Shift+Enter queue the message behind it instead.
        .when(view.active_run().is_some(), |el| {
            el.child(
                send_now_button("send-now".into(), "Send now")
                    .on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.send_now(w, cx)))
                    .child(ui::kbd(if group { "alt-enter" } else { "enter" }, cx).opacity(0.55)),
            )
        })
        .child(
            div()
                .id("send")
                .size(px(28.))
                .flex_none()
                .rounded(px(7.))
                .bg(c(theme::ACCENT))
                .flex()
                .items_center()
                .justify_center()
                .text_color(c(theme::BG))
                .font_weight(FontWeight::SEMIBOLD)
                .cursor_pointer()
                .tooltip(if group { ui::tip("Send", Some("enter")) } else { ui::tip(if view.active_run().is_some() { "Queue behind the running answer" } else { "Send" }, Some("shift-enter")) })
                .on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.send_message(w, cx)))
                .child("↵"),
        );
    // The conversation screens take drops on the whole conversation.
    h().w_full().justify_center().px(px(40.)).pt(px(14.)).pb(px(18.))
        .child(
        v().w_full()
            .max_w(px(700.))
            .when(!view.pending_images.is_empty(), |el| {
                el.child(h().flex_wrap().gap(px(6.)).pb(px(8.)).children(view.pending_images.iter().enumerate().map(|(i, path)| {
                    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    h().id(SharedString::from(format!("img-{i}"))).gap(px(6.)).px(px(9.)).py(px(4.)).rounded(px(6.)).border_1().border_color(c(theme::BORDER)).bg(c(theme::SURFACE)).text_size(px(12.))
                        .child(div().text_color(c(theme::ACCENT_TEXT)).child("▣"))
                        .child(name)
                        .child(
                            div().id(SharedString::from(format!("img-x-{i}"))).cursor_pointer().text_color(c(theme::TEXT)).child("×")
                                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                    if i < this.pending_images.len() {
                                        this.pending_images.remove(i);
                                    }
                                    cx.notify();
                                })),
                        )
                })))
            })
            // Files for the manager: copied into its work folder when the message goes.
            .when(!view.pending_files.is_empty(), |el| {
                el.child(h().flex_wrap().gap(px(6.)).pb(px(8.)).children(view.pending_files.iter().enumerate().map(|(i, (path, size))| {
                    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    h().id(SharedString::from(format!("upload-{i}"))).gap(px(6.)).px(px(9.)).py(px(4.)).rounded(px(6.)).border_1().border_color(c(theme::BORDER)).bg(c(theme::SURFACE)).text_size(px(12.))
                        .child(div().text_color(c(theme::ACCENT_TEXT)).child("▤"))
                        .child(name)
                        .child(div().text_color(c(theme::MUTED)).child(crate::app::file_size(*size)))
                        .child(
                            div().id(SharedString::from(format!("upload-x-{i}"))).cursor_pointer().text_color(c(theme::TEXT)).child("×")
                                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                    if i < this.pending_files.len() {
                                        this.pending_files.remove(i);
                                    }
                                    cx.notify();
                                })),
                        )
                })))
            })
            .map(|el| {
                let Some(subagents) = subagent_chip(view, cx) else { return el };
                el.child(h().pb(px(8.)).justify_end().child(subagents))
            })
            .children(mention_row(view, cx))
            .child(input_box)
            .children(group.then(|| mention_warning(view, cx)).flatten())
            .child(if group { group_row(view).into_any_element() } else { status_row(view, cx).into_any_element() }),
    )
}

/// Team chat: whom the `@name` being typed can be, above the composer (ui-ontwerp §4):
/// employees in org chart order, then board members; ↑↓ move, Enter / Tab or a click picks.
fn mention_row(view: &AppView, cx: &mut Context<AppView>) -> Option<impl IntoElement> {
    if view.screen != crate::app::Page::GroupChat {
        return None;
    }
    let suggestions = view.group_suggestions(cx);
    let of = suggestions.len();
    let active = view.group_mentions.active(of);
    (of > 0).then(|| {
        v().id("mentions").role(Role::ListBox).aria_label("Mention someone").mb(px(8.)).gap(px(2.)).p(px(4.)).rounded(px(6.)).border_1().border_color(c(theme::BORDER)).bg(c(theme::SURFACE))
            .children(suggestions.into_iter().enumerate().map(|(i, (at, p))| {
                let name = p.name.clone();
                let on = active == Some(i);
                h().id(("mention", i)).role(Role::ListBoxOption).aria_label(crate::screens::discussion::suggestion_label(&p, i, of)).aria_selected(on).gap(px(8.)).px(px(6.)).py(px(3.)).rounded(px(4.)).cursor_pointer()
                    .when(on, |el| el.bg(c(theme::SURFACE_2)))
                    .hover(|s| s.bg(c(theme::SURFACE_2)))
                    .child(if p.employee { ui::monogram(&p.name, 16., cx) } else { ui::member_monogram(&p.name, 16., cx) })
                    .child(div().text_size(px(12.5)).child(p.name.clone()))
                    .child(div().text_size(px(11.5)).text_color(c(theme::TEXT_2)).child(p.kind.clone()))
                    .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.group_complete(at, &name, w, cx)))
            }))
    })
}

/// Team chat, under the composer: the `@names` that wake nobody (MU2-04 AC5).
fn mention_warning(view: &AppView, cx: &mut Context<AppView>) -> Option<AnyElement> {
    let text = view.composer.read(cx).value().to_string();
    let people = view.mentionables();
    let names = crate::screens::discussion::unknown_mentions(&text, &people, &view.archived_names());
    crate::screens::discussion::unknown_warning(&names).map(|w| div().id("group-mention-warning").role(Role::Status).pt(px(6.)).px(px(4.)).text_size(px(12.)).text_color(c(crate::screens::board::HIGH)).child(w).into_any_element())
}

/// Group chat, under the composer: who answers (in a ticket's channel its owner, P-21).
fn group_row(view: &AppView) -> impl IntoElement {
    let text = match view.ticket_channel_open() {
        Some(ticket) => crate::screens::discussion::ticket_channel_row(ticket),
        None => {
            let first = view.group_answerer().map_or_else(|| "the top of the org chart".to_string(), |name| name.to_string());
            format!("@Name asks that employee; without a name, {first} answers.")
        }
    };
    h().pt(px(8.)).px(px(4.)).text_size(px(12.)).text_color(c(theme::MUTED)).child(text)
}

/// How many subagents the open manager conversation runs, as a chip above the
/// composer; a click shows what each one is doing.
fn subagent_chip(view: &AppView, cx: &mut Context<AppView>) -> Option<impl IntoElement> {
    let agents = view.subagents_here();
    if agents.is_empty() {
        return None;
    }
    let open = view.subagents_open;
    let label = if agents.len() == 1 { "1 subagent".to_string() } else { format!("{} subagents", agents.len()) };
    Some(
        div().relative()
            .child(
                h().id("subagent-chip").gap(px(7.)).px(px(8.)).py(px(3.)).rounded(px(6.)).cursor_pointer()
                    .border_1().border_color(c(if open { theme::ACCENT } else { theme::BORDER })).bg(c(theme::SURFACE))
                    .text_size(px(12.)).text_color(c(theme::TEXT_2))
                    .hover(|s| s.bg(c(theme::SURFACE_2)).text_color(c(theme::TEXT)))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_subagents(cx)))
                    .child(ui::pulse_dot("subagent-dot", 6., theme::ACCENT))
                    .child(label),
            )
            .when(open, |el| el.child(div().absolute().top(px(-6.)).right_0().child(deferred(anchored().anchor(Anchor::BottomRight).snap_to_window().child(subagent_panel(view, &agents, cx))).with_priority(1)))),
    )
}

/// What the running subagents were asked and their latest step.
fn subagent_panel(view: &AppView, agents: &[aurelius_protocol::AgentView], cx: &mut Context<AppView>) -> impl IntoElement {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0);
    let rows = agents.iter().enumerate().map(|(i, a)| {
        let key = format!("{}-{}", a.since_ms, a.name);
        let expanded = view.subagent_expanded.as_deref() == Some(key.as_str());
        let prompt = a.prompt.clone().unwrap_or_default();
        let folded = prompt.lines().filter(|l| !l.trim().is_empty()).take(3).collect::<Vec<_>>().join("\n");
        let more = folded.len() < prompt.trim().len();
        v().id(("subagent", i)).gap(px(3.)).px(px(10.)).py(px(8.)).rounded(px(6.))
            .when(i > 0, |el| el.border_t_1().border_color(c(theme::BORDER)))
            .when(more, |el| {
                el.cursor_pointer().hover(|s| s.bg(c(theme::SURFACE_2))).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.subagent_expanded = if this.subagent_expanded.as_deref() == Some(key.as_str()) { None } else { Some(key.clone()) };
                    cx.notify();
                }))
            })
            .child(
                h().gap(px(8.)).text_size(px(12.5))
                    .child(div().flex_none().max_w(px(220.)).truncate().font_weight(FontWeight::MEDIUM).text_color(c(theme::TEXT)).child(a.name.clone()))
                    .child(div().flex_1().min_w_0().truncate().text_color(c(theme::MUTED)).child(a.detail.clone()))
                    .child(div().flex_none().text_size(px(11.)).text_color(c(theme::MUTED)).child(elapsed(now - a.since_ms))),
            )
            .child(div().text_size(px(12.)).text_color(c(theme::ACCENT_TEXT)).truncate().child(match (&a.doing, a.steps) {
                (Some(doing), n) => format!("↳ {doing} · {n} {}", if n == 1 { "step" } else { "steps" }),
                (None, _) => "↳ starting…".to_string(),
            }))
            .when(!prompt.is_empty(), |el| {
                el.child(div().text_size(px(11.5)).text_color(c(theme::TEXT_2)).child(if expanded { prompt.clone() } else if more { format!("{folded} …") } else { folded }))
            })
    });
    v().id("subagent-panel").w(px(440.)).max_h(px(380.)).occlude()
        .rounded(px(9.)).border_1().border_color(c(theme::BORDER)).bg(c(theme::SURFACE)).shadow_lg()
        .on_mouse_down_out(cx.listener(|this, _, _, cx| this.close_subagents(cx)))
        .child(div().flex_none().px(px(12.)).pt(px(10.)).pb(px(6.)).text_size(px(11.5)).text_color(c(theme::MUTED)).child("Subagents at work"))
        .child(v().id("subagent-rows").flex_1().min_h_0().overflow_y_scroll().px(px(2.)).pb(px(4.)).children(rows))
}

/// "12s", "1m 12s", "1h 03m".
fn elapsed(ms: i64) -> String {
    let s = (ms.max(0) / 1000) as u64;
    match s {
        0..60 => format!("{s}s"),
        60..3600 => format!("{}m {:02}s", s / 60, s % 60),
        _ => format!("{}h {:02}m", s / 3600, s / 60 % 60),
    }
}

/// The mode chip's sign for a mode (also in menus that pick one).
pub fn mode_icon(mode: aurelius_domain::autonomy::AgentMode) -> &'static str {
    use aurelius_domain::autonomy::AgentMode;
    match mode {
        AgentMode::Plan => "◇",
        AgentMode::Ask => "○",
        AgentMode::AcceptEdits => "◐",
        AgentMode::Auto => "●",
    }
}

/// The mode an employee answered in (group chat), next to its name; not clickable.
fn mode_badge(mode: aurelius_domain::autonomy::AgentMode) -> Div {
    h().flex_none().gap(px(4.)).px(px(6.)).py(px(1.)).rounded(px(4.)).border_1().border_color(c(theme::BORDER))
        .text_size(px(11.)).text_color(c(theme::TEXT_2))
        .child(mode_icon(mode))
        .child(mode.label())
}

/// Working mode of this agent; a click (or `keys`) switches it.
pub fn mode_chip(id: &'static str, mode: aurelius_domain::autonomy::AgentMode, keys: Option<&'static str>, cx: &mut Context<AppView>, switch: impl Fn(&mut AppView, &mut Window, &mut Context<AppView>) + 'static) -> impl IntoElement {
    use aurelius_domain::autonomy::AgentMode;
    let color = match mode {
        AgentMode::Plan => theme::TEXT_2,
        AgentMode::Ask => theme::MUTED,
        AgentMode::AcceptEdits => theme::ACCENT_TEXT,
        AgentMode::Auto => 0xd99a3a,
    };
    let icon = mode_icon(mode);
    h().id(id).gap(px(6.)).px(px(7.)).py(px(2.)).rounded(px(5.)).cursor_pointer()
        .border_1().border_color(c(if mode == AgentMode::Ask { theme::BG } else { theme::BORDER }))
        .text_color(c(color))
        .hover(|s| s.bg(c(theme::SURFACE_2)))
        .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| switch(this, w, cx)))
        .child(icon)
        .child(mode.label())
        .children(keys.map(|keys| ui::kbd(keys, cx).opacity(0.55)))
}

/// "Browser off / on": lets the agent use the user's own Chrome (asks first when turning it on).
pub fn browser_chip(allowed: bool, cx: &mut Context<AppView>) -> impl IntoElement {
    h().id("browser-chip").gap(px(6.)).px(px(7.)).py(px(2.)).rounded(px(5.)).cursor_pointer()
        .border_1().border_color(c(if allowed { theme::BORDER } else { theme::BG }))
        .text_color(c(if allowed { 0xd99a3a } else { theme::MUTED }))
        .hover(|s| s.bg(c(theme::SURFACE_2)))
        .on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.toggle_browser(w, cx)))
        .child(if allowed { "◉" } else { "○" })
        .child(if allowed { "Browser on" } else { "Browser off" })
        .child(ui::kbd("alt-b", cx).opacity(0.55))
}

/// "Screen on / asks": on, the agent takes screenshots without asking; off, it asks each time.
pub fn screen_chip(allowed: bool, cx: &mut Context<AppView>) -> impl IntoElement {
    h().id("screen-chip").gap(px(6.)).px(px(7.)).py(px(2.)).rounded(px(5.)).cursor_pointer()
        .border_1().border_color(c(if allowed { theme::BORDER } else { theme::BG }))
        .text_color(c(if allowed { 0xd99a3a } else { theme::MUTED }))
        .hover(|s| s.bg(c(theme::SURFACE_2)))
        .on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.toggle_screen(w, cx)))
        .child(if allowed { "◉" } else { "○" })
        .child(if allowed { "Screen on" } else { "Screen: asks" })
        .child(ui::kbd("alt-s", cx).opacity(0.55))
}

/// Compact line under the composer: persona, model, connection and activity.
fn status_row(view: &AppView, cx: &mut Context<AppView>) -> impl IntoElement {
    let next = view.next_runtime();
    let busy = view.active_run().is_some();
    let chip = |id: &'static str| {
        h().id(id).px(px(6.)).py(px(2.)).rounded(px(5.)).cursor_pointer().hover(|s| s.bg(c(theme::SURFACE_2)))
    };
    // The open task's mode; the employee's own settings (mode, browser, screen) are on its page.
    let mode = view.mode_target().map(|(_, m, _)| m);
    v().w_full().child(
        h().pt(px(8.)).px(px(4.)).gap(px(10.)).gap_y(px(4.)).flex_wrap().min_w_0().text_size(px(12.5)).text_color(c(theme::TEXT_2))
            .when_some(mode, |el, mode| el.child(mode_chip("mode-chip", mode, Some("shift-tab"), cx, |_, w, cx| w.dispatch_action(Box::new(CycleAgentMode), cx))))
            .when(view.takes_manager_files(), |el| {
                el.child(
                    chip("upload-chip").text_color(c(theme::MUTED))
                        .tooltip(ui::tip("Add files for the employee (or drop them here)", Some("secondary-o")))
                        .on_click(cx.listener(|_, _: &ClickEvent, w, cx| w.dispatch_action(Box::new(AttachFiles), cx)))
                        .child("+ File"),
                )
            })
            .when_some(next.persona.clone(), |el, name| el.child(div().text_color(c(theme::MUTED)).child(name)))
            .child(div().flex_1())
            .when_some(view.thread_override(), |el, run| {
                el.child(
                    chip("override-chip")
                        .gap(px(6.))
                        .bg(c(theme::SURFACE_2))
                        .text_color(c(theme::ACCENT_TEXT))
                        .tooltip(ui::tip(format!("{} runs on another model than the employee's default; click to go back", view.thread_scope()), None))
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.reset_thread_run(cx)))
                        .child(format!("Override: {run}"))
                        .child(div().text_color(c(theme::MUTED)).child("Back to employee default")),
                )
            })
            .child(
                ui::dropdown_trigger("model-chip", 26., true, view.overlay == crate::app::Overlay::Menu && view.menu.as_ref().is_some_and(|m| m.field == Some("model-chip")))
                    .tooltip(ui::tip(next.model.clone(), Some("secondary-m")))
                    .on_click(cx.listener(|this, _: &ClickEvent, w, cx| {
                        let items = this.model_menu();
                        this.open_dropdown("model-chip", items, w, cx);
                    }))
                    .child(div().min_w_0().truncate().child(ui::short_default(&next.model)))
                    .when_some(view.current_effort().and_then(|e| crate::app::EFFORTS.iter().find(|(l, _, _)| *l == e)).map(|(_, label, _)| *label), |el, label| {
                        el.child(div().text_color(c(theme::MUTED)).child(format!(" · {label}")))
                    })
                    .child(ui::caret())
                    .child(ui::track_bounds(view.dropdown_bounds.clone(), "model-chip")),
            )
            .child(
                chip("connection-chip")
                    .on_click(cx.listener(|_, _: &ClickEvent, w, cx| w.dispatch_action(Box::new(OpenSettings), cx)))
                    .child(next.connection.clone()),
            )
            .child(activity_ring(busy)),
    )
}

/// Ring that pulses while an answer is being written; still when idle.
fn activity_ring(busy: bool) -> AnyElement {
    let ring = move || div().size(px(14.)).rounded_full().border(px(2.)).border_color(c(if busy { theme::ACCENT_TEXT } else { theme::BORDER_STRONG }));
    if busy {
        ui::looping("activity", 0, 14., move || {
            ring().with_animation(
                "activity",
                Animation::new(std::time::Duration::from_millis(1200)).repeat(),
                |el, t| el.opacity(0.35 + 0.65 * (1.0 - (2.0 * t - 1.0).abs())),
            )
            .into_any_element()
        })
    } else {
        ring().into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{artifact_url, fold, is_system_line, message_author, other_member, streaming_answer, web_search_summary, with_role};
    use aurelius_domain::conversation::{MessageRole, MessageStatus};
    use gpui_kit::SharedString;

    #[test]
    fn every_group_message_names_its_author() {
        let fallback = SharedString::from("Aurelius");
        let run = aurelius_domain::RunId::new();
        let mut atlas = streaming_answer(aurelius_domain::MessageId::new(), run, Some("Atlas".into()), None);
        atlas.status = MessageStatus::Complete;
        let orion = aurelius_protocol::MessageView { author: Some("Orion".into()), ..atlas.clone() };
        let you = aurelius_protocol::MessageView { role: MessageRole::User, author: None, update: None, ..atlas.clone() };
        let unnamed = aurelius_protocol::MessageView { author: None, update: None, ..atlas.clone() };
        let names: Vec<String> = [&atlas, &orion, &you, &unnamed].iter().map(|m| message_author(m, &fallback, None).to_string()).collect();
        assert_eq!(names, ["Atlas", "Orion", "You", "Aurelius"], "each message its own author, the user is You");
        // Streaming into the group chat it reads as that employee's group post.
        assert_eq!(atlas.update.as_deref(), Some(aurelius_domain::group_chat::GROUP_KIND));
        // Its run is what its Stop stops and its approvals wait on.
        assert_eq!(atlas.run_id, Some(run));
        // The mode its run started in shows with it, streaming and once copied in.
        let planning = streaming_answer(aurelius_domain::MessageId::new(), run, Some("Atlas".into()), Some(aurelius_domain::autonomy::AgentMode::Plan));
        assert_eq!(planning.mode, Some(aurelius_domain::autonomy::AgentMode::Plan));
        assert_eq!(atlas.mode, None, "no badge without a known mode");
    }

    #[test]
    fn another_board_members_post_names_them_and_yours_is_you() {
        let fallback = SharedString::from("Aurelius");
        let member = |name: &str| aurelius_domain::user::UserView { id: aurelius_domain::UserId::new(), username: name.into(), created_at_ms: 0, created_by_user: None, disabled_at_ms: None };
        let (one, two) = (member("board-one"), member("board-two"));
        let post = |by: Option<&aurelius_domain::user::UserView>| aurelius_protocol::MessageView { role: MessageRole::User, author: None, update: None, author_user: by.cloned(), ..streaming_answer(aurelius_domain::MessageId::new(), aurelius_domain::RunId::new(), None, None) };
        let (mine, theirs, unnamed) = (post(Some(&one)), post(Some(&two)), post(None));
        // Signed in as board-one: board-two's post reads as the discussion names it, on the left.
        assert_eq!(message_author(&theirs, &fallback, Some(&one)).as_ref(), "board-two · board member");
        assert!(other_member(&theirs, Some(&one)));
        // Your own post, and one without a board member (an older server, the local owner), are You on the right.
        assert_eq!(message_author(&mine, &fallback, Some(&one)).as_ref(), "You");
        assert!(!other_member(&mine, Some(&one)));
        assert_eq!(message_author(&unnamed, &fallback, Some(&one)).as_ref(), "You");
        assert!(!other_member(&unnamed, Some(&one)));
        // The same name is no proof: only the same board member is you.
        let namesake = member("board-one");
        assert!(other_member(&mine, Some(&namesake)));
        // A connection without a board member: every board member's post is somebody else's.
        assert_eq!(message_author(&mine, &fallback, None).as_ref(), "board-one · board member");
        assert_eq!(message_author(&unnamed, &fallback, None).as_ref(), "You");
    }

    #[test]
    fn a_line_the_server_writes_is_a_note_not_a_nameless_answer() {
        let fallback = SharedString::from("Aurelius");
        let mut line = streaming_answer(aurelius_domain::MessageId::new(), aurelius_domain::RunId::new(), None, None);
        line.status = MessageStatus::Complete;
        assert!(is_system_line(&line, true), "a finished group post without an author");
        let blank = aurelius_protocol::MessageView { author: Some("  ".into()), ..line.clone() };
        assert!(is_system_line(&blank, true), "a blank author is none");
        assert_eq!(message_author(&blank, &fallback, None).as_ref(), "Aurelius", "never an empty name");
        let system = aurelius_protocol::MessageView { author: Some("Aurelius".into()), update: Some("system".into()), ..line.clone() };
        assert!(is_system_line(&system, true), "kind system");
        // Nobody was woken: Aurelius' status note in the group; in a task a status stays a card.
        let status = aurelius_protocol::MessageView { update: Some("status".into()), ..system.clone() };
        assert!(is_system_line(&status, true));
        assert!(!is_system_line(&status, false));
        // Still streaming with its employee not known yet: an answer, not a note.
        let streaming = aurelius_protocol::MessageView { status: MessageStatus::Partial, ..line.clone() };
        assert!(!is_system_line(&streaming, true));
        let named = aurelius_protocol::MessageView { author: Some("Atlas".into()), ..line.clone() };
        assert!(!is_system_line(&named, true));
        // Cut off: the post keeps its author and stays an answer.
        let cut = aurelius_protocol::MessageView { status: MessageStatus::Interrupted, ..named.clone() };
        assert!(!is_system_line(&cut, true));
        let chat = aurelius_protocol::MessageView { update: None, ..line };
        assert!(!is_system_line(&chat, true), "a plain answer is never a note");
    }

    #[test]
    fn each_sender_in_the_group_chat_carries_its_role() {
        assert_eq!(with_role("Orion", Some("Project Manager")), "Orion — Project Manager");
        assert_eq!(with_role("Orion", Some("  ")), "Orion", "no dangling dash without a role");
        assert_eq!(with_role("Aurelius", None), "Aurelius");
    }

    #[test]
    fn long_messages_fold_after_twenty_lines() {
        let short: String = (1..=20).map(|n| format!("line {n}\n")).collect();
        assert!(fold(&format!("{short}\n\n\n")).is_none(), "trailing blank lines don't count");
        // Blank lines count like any other line.
        let spaced: String = (1..=11).map(|n| format!("line {n}\n\n")).collect();
        let (shown, _, hidden) = fold(&spaced).unwrap();
        assert!(shown.ends_with("line 10"));
        assert_eq!(hidden, 1);
        let long: String = (1..=25).map(|n| format!("line {n}\n")).collect();
        let (shown, cut, hidden) = fold(&long).unwrap();
        assert_eq!((shown.lines().count(), hidden), (20, 5));
        assert!(shown.ends_with("line 20") && long[cut..].starts_with("line 21"));
        // Cut inside a code block: closed again so the rest doesn't render as code.
        let code = format!("intro\n```rust\n{}```\n", "x\n".repeat(30));
        let (shown, _, hidden) = fold(&code).unwrap();
        assert!(shown.ends_with("x\n```"));
        assert_eq!(hidden, 13);
        // Same fence rule as mobile: closing fence keeps the opener's char, length and indent.
        let x = "x\n".repeat(30);
        assert!(fold(&format!("  ~~~~\n{x}~~~~\n")).unwrap().0.ends_with("x\n  ~~~~"));
        // A shorter fence, another char or text after it doesn't close the block.
        assert!(fold(&format!("````\n```\n~~~\n```` x\n{x}")).unwrap().0.ends_with("x\n````"));
        // Backticks after a backtick fence: not a fence; a closed block before the cut: nothing added.
        assert!(fold(&format!("``` a`b\n```\nx\n```\n{x}")).unwrap().0.ends_with("x\nx"));
    }

    #[test]
    fn web_search_results_are_links() {
        let out = r#"Web search results for query: "x"

Links: [{"title":"A [b] c","url":"https://www.example.com/p?q=1"}]"#;
        assert_eq!(web_search_summary(out), "- [A b c](<https://www.example.com/p?q=1>) — example.com");
    }

    #[test]
    fn artifact_url_from_publish_result() {
        let out = r#"Published C:\x\spike.html at https://claude.ai/artifact/2Jwgr8SC4Y55s2rme4hyfb (Version 1, version id 1791545049-8abb) Icon: "page"."#;
        assert_eq!(artifact_url(out), Some("https://claude.ai/artifact/2Jwgr8SC4Y55s2rme4hyfb"));
        assert_eq!(artifact_url("https://claude.ai/artifact/abc"), Some("https://claude.ai/artifact/abc"));
        assert_eq!(artifact_url("Permission denied"), None);
    }
}
