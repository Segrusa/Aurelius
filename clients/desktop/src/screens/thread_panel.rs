//! Threads and reactions in the Team chat (AUR-CHAT-002, ui-ontwerp §5): the thread
//! panel beside a channel (its root, its replies, a composer with `@`), the hover
//! actions of a message row, its reactions and the "↳ n replies" line under a root.
//! Shown only when the server has `chat_threads` / `chat_reactions`; without them the
//! Team chat is as in phase 1. The decisions are pure helpers in `threads.rs`.

use crate::app::{AppView, GroupLive, MenuDo, MenuEntry, MenuItems, Panel};
use crate::backend::{self, Request};
use crate::keymap::*;
use crate::screens::discussion::{self as d, Mentionable};
use crate::screens::threads::{self, Caps, RowActions};
use crate::theme::{self, c};
use crate::ui::{self, h, v, ButtonKind};
use aurelius_domain::MessageId;
use aurelius_domain::views::ThreadSnapshot;
use aurelius_protocol::MessageView;
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::collections::HashMap;

/// What a menu entry on a channel message does (right-click, ⋯, the reaction picker).
#[derive(Debug, Clone, Copy)]
pub enum ChatDo {
    /// Open the picker of the eight reactions at this point.
    Picker(MessageId, Point<Pixels>),
    /// Add (`true`) or take back the reader's reaction.
    React(MessageId, &'static str, bool),
    /// Open the message's thread in the panel.
    Reply(MessageId),
    /// Copy the message's link (AUR-FA A5a).
    CopyLink(MessageId),
}

/// The thread panel's state in a window.
pub struct ThreadPanel {
    /// `thread.read` for the thread shown (none while it loads).
    pub snapshot: Option<ThreadSnapshot>,
    /// Why it did not load (Try again asks again).
    pub error: Option<String>,
    pub composer: Entity<TextareaState>,
    /// The thread the composer's text was written for.
    pub composer_for: Option<MessageId>,
    pub mentions: d::MentionPicker,
    /// The thread the backend reads on every chat event (`Request::FollowThread`); this window last asked for that.
    pub followed: Option<MessageId>,
    /// Answers streaming into the shown thread, before they are replies.
    pub live: Vec<GroupLive>,
    pub scroll: ScrollHandle,
    /// Each message row's action bar, reachable with Tab (ui-ontwerp §5: also on keyboard focus).
    pub row_focus: std::cell::RefCell<HashMap<MessageId, FocusHandle>>,
}

impl ThreadPanel {
    pub fn new(window: &mut Window, cx: &mut Context<AppView>) -> Self {
        Self {
            snapshot: None,
            error: None,
            composer: cx.new(|cx| TextareaState::new(window, cx).auto_grow(1, 6).submit_on_enter(true).placeholder(threads::THREAD_PLACEHOLDER)),
            composer_for: None,
            mentions: d::MentionPicker::default(),
            followed: None,
            live: Vec::new(),
            scroll: ScrollHandle::new(),
            row_focus: Default::default(),
        }
    }

    /// Enter sends (Shift+Enter is a new line, as in the Team chat); the `@` suggestions follow the text.
    pub fn subscriptions(&self, window: &mut Window, cx: &mut Context<AppView>) -> Vec<Subscription> {
        vec![cx.subscribe_in(&self.composer, window, |this, _, e: &InputEvent, w, cx| match e {
            InputEvent::PressEnter { shift: false, .. } => this.thread_send(w, cx),
            InputEvent::Change => {
                let text = this.thread.composer.read(cx).value().to_string();
                this.thread.mentions.changed(&text);
                cx.notify();
            }
            _ => {}
        })]
    }
}

impl AppView {
    /// Threads in channels (feature `chat_threads`).
    pub fn chat_threads_supported(&self) -> bool {
        matches!(&self.connection, backend::Connection::Online { server, .. } if server.chat_threads)
    }

    /// Reactions on channel messages (feature `chat_reactions`).
    pub fn chat_reactions_supported(&self) -> bool {
        matches!(&self.connection, backend::Connection::Online { server, .. } if server.chat_reactions)
    }

    /// The thread the panel shows beside the Team chat.
    pub fn thread_root(&self) -> Option<MessageId> {
        match self.panel {
            Panel::Thread(root) if self.screen == crate::app::Page::GroupChat => Some(root),
            _ => None,
        }
    }

    /// What a message of the open channel (or of the open thread) allows.
    pub fn message_caps(&self, m: &MessageView) -> Caps {
        let in_thread = m.thread_parent.is_some() || self.thread.snapshot.as_ref().is_some_and(|s| s.root.id == m.id);
        let snapshot = self.thread.snapshot.as_ref().filter(|_| in_thread);
        Caps { threads: self.chat_threads_supported(), reactions: self.chat_reactions_supported(), read_only: threads::thread_read_only(snapshot, self.channel_read_only()) }
    }

    /// A message row has the action bar of phase 2 (else phase 1's inline actions).
    pub fn row_toolbar_on(&self) -> bool {
        self.screen == crate::app::Page::GroupChat && (self.chat_threads_supported() || self.chat_reactions_supported())
    }

    // ---- Opening, following and reading a thread ----------------------------------

    /// Show the thread of `root` in the panel beside the channel; its composer keeps
    /// what was typed for this thread.
    pub fn open_thread_panel(&mut self, root: MessageId, window: &mut Window, cx: &mut Context<Self>) {
        if !self.chat_threads_supported() {
            return;
        }
        if self.panel != Panel::Thread(root) {
            self.thread.snapshot = None;
            self.thread.error = None;
            self.thread.live.clear();
        }
        if self.thread.composer_for != Some(root) {
            self.thread.composer_for = Some(root);
            self.thread.composer.update(cx, |i, cx| i.set_value("", window, cx));
        }
        self.open_panel(Panel::Thread(root), window, cx);
        self.thread.composer.update(cx, |i, cx| i.focus(window, cx));
    }

    /// The backend follows the thread the panel shows, and stops when it closes or the
    /// Team chat is left (called on every render, like `FollowChannels`).
    pub fn follow_thread(&mut self) {
        let shown = self.thread_root();
        if shown != self.thread.followed {
            self.thread.followed = shown;
            let _ = self.requests.send(Request::FollowThread(shown));
        }
    }

    /// Try again after the thread did not load.
    pub fn thread_retry(&mut self, cx: &mut Context<Self>) {
        self.thread.error = None;
        let _ = self.requests.send(Request::FollowThread(self.thread_root()));
        cx.notify();
    }

    /// `thread.read` answered: the panel shows it, and its root in the channel has no unread replies any more.
    pub fn thread_loaded(&mut self, root: MessageId, result: Result<ThreadSnapshot, String>) {
        if self.thread_root() != Some(root) {
            return;
        }
        match result {
            Ok(snapshot) => {
                if let Some(channel) = self.messages.as_mut().filter(|s| s.conversation_id == snapshot.conversation_id) {
                    threads::mark_thread_read(&mut channel.messages, &snapshot);
                }
                // A streamed answer is a reply now.
                for reply in &snapshot.replies {
                    self.live.remove(&reply.id);
                    self.live_activity.remove(&reply.id);
                }
                self.thread.live.retain(|l| !snapshot.replies.iter().any(|r| r.id == l.message));
                let grew = self.thread.snapshot.as_ref().is_none_or(|s| s.replies.len() != snapshot.replies.len());
                if grew {
                    self.thread.scroll.scroll_to_bottom();
                }
                self.thread.snapshot = Some(snapshot);
                self.thread.error = None;
            }
            Err(why) => self.thread.error = Some(why),
        }
    }

    /// An answer streaming into the shown thread (`StreamDelta::thread_parent`).
    pub fn thread_streamed(&mut self, delta: &aurelius_protocol::StreamDelta) {
        let author = self.run_author(delta.run_id);
        let at = std::time::Instant::now();
        match self.thread.live.iter_mut().find(|l| l.message == delta.message_id) {
            Some(entry) => {
                entry.at = at;
                entry.author = entry.author.take().or(author);
                entry.mode = entry.mode.or(delta.mode);
            }
            None => self.thread.live.push(GroupLive { message: delta.message_id, run: delta.run_id, at, author, mode: delta.mode }),
        }
        self.expanded_messages.insert(delta.message_id);
        self.live.insert(delta.message_id, delta.text.clone());
        self.live_activity.insert(delta.message_id, delta.activity.clone());
        self.thread.scroll.scroll_to_bottom();
    }

    // ---- Replying ----------------------------------------------------------------

    /// Send the thread composer's text as a reply (`group_chat.send` with `thread_parent`).
    pub fn thread_send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(root) = self.thread_root() else { return };
        if self.message_caps_of_root().read_only {
            return;
        }
        let text = self.thread.composer.read(cx).value().trim().to_string();
        if text.is_empty() {
            return;
        }
        let _ = self.requests.send(Request::SendThreadReply { text, channel: self.channel, thread_parent: root });
        self.thread.composer.update(cx, |i, cx| i.set_value("", window, cx));
        cx.notify();
    }

    fn message_caps_of_root(&self) -> Caps {
        Caps { threads: self.chat_threads_supported(), reactions: self.chat_reactions_supported(), read_only: threads::thread_read_only(self.thread.snapshot.as_ref(), self.channel_read_only()) }
    }

    /// The `@` suggestions above the thread composer: where the `@` is, and whom.
    pub fn thread_suggestions(&self, cx: &App) -> Vec<(usize, Mentionable)> {
        let text = self.thread.composer.read(cx).value().to_string();
        if !self.thread.mentions.shows(&text) {
            return Vec::new();
        }
        let people = self.mentionables();
        d::mention_suggestions(&text, &people).map(|(at, found)| found.into_iter().map(|p| (at, p.clone())).collect()).unwrap_or_default()
    }

    /// Complete the `@word` being typed to a whole name.
    pub fn thread_complete(&mut self, at: usize, name: String, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.thread.composer.read(cx).value().to_string();
        if at > text.len() {
            return;
        }
        let done = d::complete(&text, at, &name);
        self.thread.composer.update(cx, |i, cx| {
            i.set_value(done, window, cx);
            i.focus(window, cx);
        });
        cx.notify();
    }

    /// ↑ / ↓ in the suggestions.
    pub fn thread_step_mention(&mut self, down: bool, cx: &mut Context<Self>) {
        let of = self.thread_suggestions(cx).len();
        self.thread.mentions.step(down, of);
        cx.notify();
    }

    /// Enter / Tab: the active suggestion.
    pub fn thread_pick_mention(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let suggestions = self.thread_suggestions(cx);
        if let Some((at, p)) = self.thread.mentions.active(suggestions.len()).and_then(|i| suggestions.into_iter().nth(i)) {
            self.thread_complete(at, p.name, window, cx);
        }
    }

    /// Esc: close the suggestions (they open again when the text changes).
    pub fn thread_close_mentions(&mut self, cx: &mut Context<Self>) {
        let text = self.thread.composer.read(cx).value().to_string();
        self.thread.mentions.close(&text);
        cx.notify();
    }

    // ---- Reactions -----------------------------------------------------------------

    /// A message of the channel or of the shown thread.
    fn shown_message(&self, id: MessageId) -> Option<&MessageView> {
        let channel = self.messages.iter().flat_map(|s| s.messages.iter());
        let thread = self.thread.snapshot.iter().flat_map(|s| std::iter::once(&s.root).chain(s.replies.iter()));
        channel.chain(thread).find(|m| m.id == id)
    }

    /// Add or take back the reader's reaction (`message.react`); shown at once, the
    /// server's `message.reacted` reads the list again.
    pub fn react(&mut self, message: MessageId, emoji: &str, on: bool, cx: &mut Context<Self>) {
        if !self.shown_message(message).is_some_and(|m| self.message_caps(m).reactions && !self.message_caps(m).read_only) {
            return;
        }
        let _ = self.requests.send(Request::React { message_id: message, emoji: emoji.to_string(), on });
        let me = self.me_name();
        let lists = self.messages.iter_mut().flat_map(|s| s.messages.iter_mut()).chain(self.thread.snapshot.iter_mut().flat_map(|s| std::iter::once(&mut s.root).chain(s.replies.iter_mut())));
        for m in lists.filter(|m| m.id == message) {
            threads::apply_reaction(&mut m.reactions, emoji, on, me.as_deref());
        }
        cx.notify();
    }

    /// The picker of the eight reactions, as a menu at `anchor` (yours are checked).
    pub fn open_reaction_picker(&mut self, message: MessageId, anchor: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(m) = self.shown_message(message) else { return };
        let entries: MenuItems = threads::picker(&m.reactions)
            .into_iter()
            .map(|(emoji, mine)| Some(MenuEntry { label: emoji.into(), keys: None, run: MenuDo::Chat(ChatDo::React(message, emoji, !mine)), danger: false, checked: mine, detail: mine.then(|| "Take back".into()) }))
            .collect();
        self.open_menu(anchor, entries, window, cx);
    }

    /// A menu entry of a message ran.
    pub fn chat_do(&mut self, run: ChatDo, window: &mut Window, cx: &mut Context<Self>) {
        match run {
            ChatDo::Picker(message, anchor) => self.open_reaction_picker(message, anchor, window, cx),
            ChatDo::React(message, emoji, on) => self.react(message, emoji, on, cx),
            ChatDo::Reply(message) => self.open_thread_panel(message, window, cx),
            ChatDo::CopyLink(message) => self.copy_message_link(message, cx),
        }
    }

    /// The actions of a message as a menu (right-click, ⋯, Enter on its action bar):
    /// React…, Reply in thread, Create ticket…, Copy link. Empty when it has none.
    pub fn message_menu(&self, m: &MessageView, anchor: Point<Pixels>) -> MenuItems {
        let RowActions { react, reply } = if self.row_toolbar_on() { threads::row_actions(m, self.message_caps(m)) } else { RowActions::default() };
        let chat = |label: &str, run| Some(MenuEntry { label: label.to_string().into(), keys: None, run: MenuDo::Chat(run), danger: false, checked: false, detail: None });
        let mut items = Vec::new();
        if react {
            items.push(chat("React…", ChatDo::Picker(m.id, anchor)));
        }
        if reply {
            items.push(chat("Reply in thread", ChatDo::Reply(m.id)));
        }
        if self.make_ticket_action_allowed(m) {
            items.push(crate::screens::board::menu_entry("Create ticket…", crate::screens::board::BoardDo::TicketFromMessage(m.id), false, None));
        }
        if self.message_link_allowed(m) {
            items.push(chat("Copy link", ChatDo::CopyLink(m.id)));
        }
        items
    }

    // ---- In a message row ----------------------------------------------------------

    /// The action bar at the top right of a row (ui-ontwerp §5): [☺ React] [↳ Reply in
    /// thread] then the row's own actions (⌁ Ticket, …) and [⋯]. It shows on hover and
    /// while it has the keyboard focus (Tab); Enter there opens the actions as a menu.
    pub fn row_toolbar(&self, m: &MessageView, hover: SharedString, actions: Vec<AnyElement>, cx: &mut Context<Self>) -> AnyElement {
        let RowActions { react, reply } = threads::row_actions(m, self.message_caps(m));
        let id = m.id;
        let focus = self.thread.row_focus.borrow_mut().entry(id).or_insert_with(|| cx.focus_handle().tab_stop(true)).clone();
        let menu_items = !self.message_menu(m, Point::default()).is_empty();
        h().id(SharedString::from(format!("row-actions-{id}"))).role(Role::Toolbar).aria_label("Message actions")
            .absolute().top(px(-12.)).right(px(4.)).gap(px(2.)).p(px(2.)).rounded(px(6.)).border_1().border_color(c(theme::BORDER)).bg(c(theme::SURFACE))
            .opacity(0.).group_hover(hover, |s| s.opacity(1.))
            .track_focus(&focus).key_context(MESSAGE_ROW).focus(|s| s.opacity(1.).border_color(c(theme::FOCUS_RING))).in_focus(|s| s.opacity(1.))
            .on_action(|_: &RowNext, w, cx| w.focus_next(cx))
            .on_action(|_: &RowPrev, w, cx| w.focus_prev(cx))
            .on_action(cx.listener(move |this, _: &MessageActions, w, cx| {
                let at = point(w.viewport_size().width / 2. - px(160.), px(180.));
                let Some(m) = this.shown_message(id).cloned() else { return };
                let items = this.message_menu(&m, at);
                if !items.is_empty() {
                    this.open_menu(at, items, w, cx);
                }
            }))
            .when(react, |el| {
                el.child(ui::action_icon(SharedString::from(format!("react-{id}")), "☺", "React", None, None).on_click(cx.listener(move |this, e: &ClickEvent, w, cx| {
                    cx.stop_propagation();
                    this.open_reaction_picker(id, e.position(), w, cx);
                })))
            })
            .when(reply, |el| {
                el.child(ui::action_icon(SharedString::from(format!("reply-{id}")), "↳", "Reply in thread", None, None).on_click(cx.listener(move |this, _: &ClickEvent, w, cx| {
                    cx.stop_propagation();
                    this.open_thread_panel(id, w, cx);
                })))
            })
            .children(actions)
            .when(self.message_link_allowed(m), |el| {
                el.child(ui::action_icon(SharedString::from(format!("copy-link-{id}")), "🔗", "Copy link", None, None).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    this.copy_message_link(id, cx);
                })))
            })
            .when(menu_items, |el| {
                el.child(ui::action_icon(SharedString::from(format!("more-{id}")), "⋯", "More actions", None, None).on_click(cx.listener(move |this, e: &ClickEvent, w, cx| {
                    cx.stop_propagation();
                    let Some(m) = this.shown_message(id).cloned() else { return };
                    let items = this.message_menu(&m, e.position());
                    this.open_menu(e.position(), items, w, cx);
                })))
            })
            .into_any_element()
    }

    /// The reactions under a message's text (ui-ontwerp §5): `[✓ 2]` pills, yours in
    /// the accent; a click toggles yours, the tooltip says who; `[☺+]` opens the picker.
    pub fn reactions_row(&self, m: &MessageView, cx: &mut Context<Self>) -> Option<AnyElement> {
        let caps = self.message_caps(m);
        if !caps.reactions || m.reactions.is_empty() {
            return None;
        }
        let can = threads::row_actions(m, caps).react;
        let me = self.me_name();
        let id = m.id;
        let pills = m.reactions.iter().enumerate().map(|(i, r)| {
            let (emoji, on) = (r.emoji.clone(), !r.mine);
            h().id(SharedString::from(format!("reaction-{id}-{i}"))).role(Role::Button).aria_label(threads::reaction_label(r)).aria_selected(r.mine)
                .gap(px(4.)).px(px(7.)).py(px(1.)).rounded(px(10.)).border_1().text_size(px(11.5))
                .bg(c(if r.mine { theme::ACCENT_HALO } else { theme::SURFACE_2 }))
                .border_color(c(if r.mine { theme::ACCENT } else { theme::SURFACE_2 }))
                .tooltip(ui::tip(threads::reaction_tip(r, me.as_deref()), None))
                .child(r.emoji.clone())
                .child(div().text_color(c(if r.mine { theme::ACCENT_TEXT } else { theme::TEXT_2 })).child(r.count.to_string()))
                .when(can, |el| {
                    el.cursor_pointer().on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        this.react(id, &emoji, on, cx);
                    }))
                })
        });
        Some(
            h().flex_wrap().gap(px(4.))
                .children(pills)
                .when(can, |el| {
                    el.child(
                        div().id(SharedString::from(format!("reaction-add-{id}"))).role(Role::Button).aria_label("Add a reaction").px(px(7.)).py(px(1.)).rounded(px(10.)).border_1().border_color(c(theme::BORDER)).cursor_pointer().text_size(px(11.5)).text_color(c(theme::TEXT_2))
                            .hover(|s| s.bg(c(theme::SURFACE_2)))
                            .child("☺+")
                            .on_click(cx.listener(move |this, e: &ClickEvent, w, cx| {
                                cx.stop_propagation();
                                this.open_reaction_picker(id, e.position(), w, cx);
                            })),
                    )
                })
                .into_any_element(),
        )
    }

    /// Under a root in the channel (ui-ontwerp §5): `↳ 3 replies · last 11:02` with up to
    /// three monograms of who replied; bold with an accent mark while replies are unread.
    pub fn thread_summary(&self, m: &MessageView, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.chat_threads_supported() || m.thread_parent.is_some() {
            return None;
        }
        let today = crate::screens::team_feed::local_day(crate::screens::chat::now_ms(), aurelius_host::local_utc_offset_minutes());
        let offset = m.thread_last_reply_ms.map_or(0, aurelius_host::utc_offset_minutes_at);
        let line = threads::thread_line(m, offset, today)?;
        let unread = threads::unread(m);
        let id = m.id;
        let employees: Vec<&str> = self.managers.iter().map(|m| m.name.as_str()).collect();
        let people = threads::repliers(m, &employees);
        Some(
            h().id(SharedString::from(format!("thread-{id}"))).role(Role::Link).aria_label(threads::thread_label(m, offset, today)).gap(px(6.)).pt(px(2.)).cursor_pointer()
                .children(people.iter().map(|(name, human)| if *human { ui::member_monogram(name, 16., cx) } else { ui::monogram(name, 16., cx) }))
                .child(div().text_size(px(12.)).text_color(c(theme::ACCENT_TEXT)).when(unread > 0, |el| el.font_weight(FontWeight::SEMIBOLD)).child(line))
                .when(unread > 0, |el| el.child(div().px(px(5.)).rounded(px(8.)).bg(c(theme::ACCENT)).text_color(c(theme::BG)).text_size(px(11.)).child(format!("{unread} new"))))
                .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| {
                    cx.stop_propagation();
                    this.open_thread_panel(id, w, cx);
                }))
                .into_any_element(),
        )
    }

    // ---- The panel -----------------------------------------------------------------

    /// The thread panel beside the channel (ui-ontwerp §5): "Thread · #channel" and ×,
    /// the root, "── n replies ──", the replies and a composer (none when read-only).
    pub fn render_thread_panel(&mut self, root: MessageId, cx: &mut Context<Self>) -> AnyElement {
        let title = threads::thread_title(&self.current_channel().map_or_else(|| "# general".to_string(), d::channel_title));
        let header = h().h(px(48.)).flex_none().px(px(16.)).gap(px(10.)).border_b_1().border_color(c(theme::BORDER))
            .child(div().flex_1().min_w_0().truncate().text_size(px(13.5)).font_weight(FontWeight::MEDIUM).child(title))
            .child(ui::icon_button("thread-close").aria_label("Close the thread").tooltip(ui::tip("Close", Some("escape"))).child("×").on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.close_panel(cx))));
        let author: SharedString = "Aurelius".into();
        let mut rows: Vec<AnyElement> = Vec::new();
        match self.thread.snapshot.clone().filter(|s| s.root.id == root) {
            Some(s) => {
                // The root as in the channel, without its thread line (this is the thread).
                let shown_root = MessageView { thread_reply_count: None, thread_unread: None, ..s.root.clone() };
                rows.push(div().w_full().relative().child(self.render_message(&shown_root, &author, cx)).into_any_element());
                if let Some(label) = threads::replies_divider(s.replies.len()) {
                    rows.push(crate::screens::chat::feed_separator("thread-replies".into(), label.into(), theme::BORDER, theme::TEXT_2).pt(px(14.)).into_any_element());
                }
                for reply in &s.replies {
                    rows.push(div().w_full().relative().pt(px(12.)).when(self.link_flashes(reply.id), |el| el.rounded(px(4.)).bg(c(theme::ACCENT_HALO))).child(self.render_message(reply, &author, cx)).into_any_element());
                }
                for l in self.thread.live.iter().filter(|l| !s.replies.iter().any(|r| r.id == l.message)) {
                    let mut streaming = crate::screens::chat::streaming_answer(l.message, l.run, l.author.clone().or_else(|| self.run_author(l.run)), l.mode);
                    streaming.thread_parent = Some(root);
                    rows.push(div().w_full().pt(px(12.)).child(self.render_message(&streaming, &author, cx)).into_any_element());
                }
            }
            None => match self.thread.error.clone() {
                Some(why) => rows.push(
                    v().gap(px(10.)).text_size(px(13.)).text_color(c(theme::TEXT_2))
                        .child(div().text_color(c(theme::TEXT)).child("The thread did not load."))
                        .child(why)
                        .child(h().child(ui::button("thread-retry", "Try again", ButtonKind::Outline, None).h(px(28.)).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.thread_retry(cx)))))
                        .into_any_element(),
                ),
                None => rows.push(div().text_color(c(theme::MUTED)).child("Loading the thread…").into_any_element()),
            },
        }
        let body = ui::scroll_frame(&self.thread.scroll, div().id("thread-scroll").flex_1().min_h_0().overflow_y_scroll().track_scroll(&self.thread.scroll).px(px(16.)).pt(px(18.)).pb(px(12.)).child(v().w_full().children(rows))).flex_1();
        v().size_full().bg(c(theme::BG)).border_l_1().border_color(c(theme::BORDER))
            .child(header)
            .child(body)
            .child(self.thread_footer(cx))
            .into_any_element()
    }

    /// The thread's composer, or a read-only note in a closed channel.
    fn thread_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let frame = v().flex_none().gap(px(8.)).px(px(16.)).py(px(10.)).border_t_1().border_color(c(theme::BORDER)).bg(c(theme::PANEL));
        if self.message_caps_of_root().read_only {
            return frame.id("thread-read-only").role(Role::Status).child(div().text_size(px(12.5)).text_color(c(theme::TEXT_2)).child("This channel is closed · the thread is read-only.")).into_any_element();
        }
        let text = self.thread.composer.read(cx).value().to_string();
        let people = self.mentionables();
        let warning = d::unknown_warning(&d::unknown_mentions(&text, &people, &self.archived_names()));
        let suggestions = self.thread_suggestions(cx);
        let of = suggestions.len();
        let active = self.thread.mentions.active(of);
        let list = (of > 0).then(|| {
            v().id("thread-mentions").role(Role::ListBox).aria_label("Mention someone").gap(px(2.)).p(px(4.)).rounded(px(6.)).border_1().border_color(c(theme::BORDER)).bg(c(theme::SURFACE))
                .children(suggestions.into_iter().enumerate().map(|(i, (at, p))| {
                    let name = p.name.clone();
                    let on = active == Some(i);
                    h().id(("thread-mention", i)).role(Role::ListBoxOption).aria_label(d::suggestion_label(&p, i, of)).aria_selected(on).gap(px(8.)).px(px(6.)).py(px(3.)).rounded(px(4.)).cursor_pointer()
                        .when(on, |el| el.bg(c(theme::SURFACE_2)))
                        .hover(|s| s.bg(c(theme::SURFACE_2)))
                        .child(if p.employee { ui::monogram(&p.name, 16., cx) } else { ui::member_monogram(&p.name, 16., cx) })
                        .child(div().text_size(px(12.5)).child(p.name.clone()))
                        .child(div().text_size(px(11.5)).text_color(c(theme::TEXT_2)).child(p.kind.clone()))
                        .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.thread_complete(at, name.clone(), w, cx)))
                }))
        });
        let ready = !text.trim().is_empty();
        frame
            .children(list)
            .child(
                h().gap(px(8.)).px(px(10.)).rounded(px(10.)).border_1().border_color(c(theme::BORDER)).bg(c(theme::SURFACE)).key_context("Composer")
                    // While suggestions show, ↑↓ / Enter / Tab / Esc are theirs (ui-ontwerp §4).
                    .child(div().flex_1().when(of > 0, |el| {
                        el.key_context(MENTIONS)
                            .on_action(cx.listener(|this, _: &MentionUp, _, cx| this.thread_step_mention(false, cx)))
                            .on_action(cx.listener(|this, _: &MentionDown, _, cx| this.thread_step_mention(true, cx)))
                            .on_action(cx.listener(|this, _: &MentionPick, w, cx| this.thread_pick_mention(w, cx)))
                            .on_action(cx.listener(|this, _: &MentionClose, _, cx| this.thread_close_mentions(cx)))
                    }).child(Textarea::new(&self.thread.composer).py(px(7.)).line_height(px(20.)).appearance(false)))
                    .child(
                        div().id("thread-send").size(px(26.)).flex_none().rounded(px(7.)).flex().items_center().justify_center().font_weight(FontWeight::SEMIBOLD)
                            .bg(c(if ready { theme::ACCENT } else { theme::SURFACE_2 })).text_color(c(if ready { theme::BG } else { theme::MUTED }))
                            .tooltip(ui::tip("Send", Some("enter")))
                            .when(ready, |el| el.cursor_pointer().on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.thread_send(w, cx))))
                            .child("↵"),
                    ),
            )
            .children(warning.map(|w| div().id("thread-mention-warning").role(Role::Status).text_size(px(12.)).text_color(c(crate::screens::board::HIGH)).child(w)))
            .into_any_element()
    }
}
