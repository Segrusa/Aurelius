//! The group chat ("Team chat"): one conversation of the user with every employee,
//! right under Dashboard. `@Name` asks that employee; without a name the top of the
//! org chart answers. Each answer shows under its employee's name and streams in live.
//!
//! Channels (AUR-MU-02, fase 2 §3): #general plus one per project, listed on the
//! left; an archived project's channel is read-only. A server without channels
//! (no `channel.list`) keeps the one general chat and no list shows.
//!
//! Ticket channels (AUR-CHAT-001, ui-ontwerp §1–4): a server with `ticket_channels`
//! also lists one channel per ticket (its discussion), under Tickets while the
//! ticket is open and under Archived (read-only) once it is closed.

use super::chat::composer;
use crate::app::AppView;
use crate::keymap::*;
use crate::theme::{self, c};
use crate::ui::{self, h, v, ButtonKind};
use crate::screens::discussion as d;
use aurelius_domain::ConversationId;
use aurelius_domain::employee::LifecycleStatus;
use aurelius_domain::views::{ChannelKind, ChannelView};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

impl AppView {
    /// `channel.list` answered (none: the server has no channels).
    pub fn channels_loaded(&mut self, channels: Option<Vec<ChannelView>>) {
        // Ticket channels only from a server that has them (an older one never sends them).
        let channels = match channels {
            Some(list) if !self.ticket_channels_supported() => Some(list.into_iter().filter(|c| d::kind_of(c) != ChannelKind::Ticket).collect()),
            list => list,
        };
        // A channel that is gone (its project or ticket was deleted): back to general.
        if let (Some(list), Some(open)) = (&channels, self.channel)
            && !list.iter().any(|c| c.conversation_id == open)
        {
            self.channel = None;
            self.group_conversation = None;
            self.channel_ticket = None;
            if self.screen == crate::app::Page::GroupChat {
                self.open(None);
                let _ = self.requests.send(crate::backend::Request::OpenGroupChat(None));
            }
        }
        // Reopened (the ticket left Done, Titan's decision): it can be written in again.
        if let (Some(list), Some(refused)) = (&channels, self.channel_refused)
            && list.iter().any(|c| c.conversation_id == refused && !c.archived)
        {
            self.channel_refused = None;
        }
        // The ticket moved: its header reads the ticket again.
        let moved = channels.as_ref().and_then(|list| list.iter().find(|c| Some(c.conversation_id) == self.channel)).and_then(|c| c.ticket_id.zip(c.ticket_column));
        if let (Some((ticket, column)), Some(shown)) = (moved, &self.channel_ticket)
            && shown.ticket.id == ticket
            && shown.ticket.column != column
        {
            let _ = self.requests.send(crate::backend::Request::Board(crate::backend::BoardCall::Ticket(ticket)));
        }
        self.channels = channels.filter(|list| !list.is_empty());
        // The channel shown is being read: it adds nothing to the sidebar badge.
        if let (Some(list), Some(open), crate::app::Page::GroupChat) = (self.channels.as_mut(), self.group_conversation, self.screen) {
            d::mark_read(list, open);
        }
    }

    /// The Team chat read a channel (`group_chat.get` marks it read on the server): it is
    /// read here at once, and the list is read again for the other channels' counts.
    pub fn channel_read(&mut self, conversation: ConversationId) {
        let Some(list) = self.channels.as_mut() else { return };
        d::mark_read(list, conversation);
        let _ = self.requests.send(crate::backend::Request::LoadChannels);
    }

    /// `channel.search` answered for `query` (none: the server could not search; then
    /// the names in the list are searched here).
    pub fn channels_found(&mut self, query: String, found: Option<Vec<ChannelView>>, cx: &mut Context<Self>) {
        if self.channel_search.read(cx).value().trim() != query {
            return;
        }
        let found = found.unwrap_or_else(|| self.local_channel_search(&query));
        self.channel_results = Some((query, found));
        cx.notify();
    }

    fn local_channel_search(&self, query: &str) -> Vec<ChannelView> {
        let query = query.to_lowercase();
        self.channels.iter().flatten().filter(|c| d::channel_title(c).to_lowercase().contains(&query)).cloned().collect()
    }

    /// The search field changed: ask the server (`channel.search`), or clear the search.
    pub fn search_channels(&mut self, cx: &mut Context<Self>) {
        let query = self.channel_search.read(cx).value().trim().to_string();
        if query.is_empty() {
            self.channel_results = None;
        } else if self.ticket_channels_supported() {
            let _ = self.requests.send(crate::backend::Request::SearchChannels(query));
        } else {
            self.channel_results = Some((query.clone(), self.local_channel_search(&query)));
        }
        cx.notify();
    }

    /// A post was refused with `channel_archived`: the channel shows its read-only bar.
    pub fn channel_archived(&mut self, channel: Option<ConversationId>) {
        let open = self.current_channel().map(|c| c.conversation_id);
        self.channel_refused = channel.or(open);
    }

    /// The open channel can only be read: the list says so, or the server refused a post in it.
    pub fn channel_read_only(&self) -> bool {
        self.current_channel().is_some_and(|c| c.archived || self.channel_refused == Some(c.conversation_id))
    }

    /// The channel the Team chat shows, when the server has channels.
    pub fn current_channel(&self) -> Option<&ChannelView> {
        let list = self.channels.as_ref()?;
        match self.channel {
            Some(id) => list.iter().find(|c| c.conversation_id == id),
            None => list.iter().find(|c| c.project_id.is_none()),
        }
    }

    /// The project of the open channel (none in #general).
    pub fn channel_project(&self) -> Option<aurelius_domain::ProjectId> {
        self.current_channel().and_then(|c| c.project_id)
    }

    /// The composer's placeholder in the Team chat (fase 2 §3).
    pub fn channel_placeholder(&self) -> String {
        match self.current_channel() {
            None => crate::app::GROUP_PLACEHOLDER.into(),
            c => d::channel_hint(c),
        }
    }

    /// Choose the channel the Team chat shows; it loads when the Team chat opens it.
    /// Returns it, or none when it is not in the list or already open.
    fn choose_channel(&mut self, conversation: ConversationId) -> Option<ChannelView> {
        let target = self.channels.as_ref().and_then(|l| l.iter().find(|c| c.conversation_id == conversation)).cloned()?;
        let channel = target.project_id.map(|_| conversation);
        if channel == self.channel && self.group_conversation == Some(conversation) {
            return None;
        }
        self.channel = channel;
        // Taken from the list before the channel is read: reading it marks it read.
        self.channel_divider = (target.unread > 0).then_some((conversation, target.unread as usize, target.read_through));
        self.channel_new_at = None;
        self.group_conversation = None;
        self.group_error = None;
        // A ticket's channel: its title and assignees for the header (ui-ontwerp §3).
        self.channel_ticket = None;
        if let Some(ticket) = target.ticket_id {
            let _ = self.requests.send(crate::backend::Request::Board(crate::backend::BoardCall::Ticket(ticket)));
        }
        Some(target)
    }

    /// `ticket.get` answered for the open ticket channel's ticket.
    pub fn channel_ticket_loaded(&mut self, detail: &aurelius_domain::ticket::TicketDetail) {
        if self.current_channel().and_then(|c| c.ticket_id) == Some(detail.ticket.id) {
            self.channel_ticket = Some(detail.clone());
        }
    }

    /// Ctrl+K "Go to #…": the Team chat on that channel, from any page. The channel is
    /// chosen before the page opens, so no other channel is read (and marked read) on the way.
    pub fn go_to_channel(&mut self, conversation: ConversationId, window: &mut Window, cx: &mut Context<Self>) {
        if self.screen == crate::app::Page::GroupChat {
            return self.open_channel(conversation, window, cx);
        }
        self.choose_channel(conversation);
        self.go_nav(crate::app::NavItem::Chat, window, cx);
    }

    /// Show a channel: its messages, with "New messages" before the ones not read yet.
    pub fn open_channel(&mut self, conversation: ConversationId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(target) = self.choose_channel(conversation) else { return };
        self.open(None);
        let placeholder = self.channel_placeholder();
        self.pending_placeholder_set(placeholder);
        let _ = self.requests.send(crate::backend::Request::OpenGroupChat(self.channel));
        if !target.archived {
            self.composer.update(cx, |input, cx| input.focus(window, cx));
        }
        cx.notify();
    }

    /// The channels the list shows, in its order (Alt+↑/↓ walks these).
    fn visible_channels(&self) -> Vec<ConversationId> {
        let Some(list) = self.channels.as_deref() else { return Vec::new() };
        let mut s = d::sections(list);
        if let Some(frozen) = &self.channel_order {
            s.tickets = d::keep_order(s.tickets, frozen);
        }
        d::visible(&s, self.channels_tickets_all, self.channels_archived_open, self.channels_archived_all)
    }

    /// Alt+↑ / Alt+↓: the channel above or below in the list.
    pub fn step_channel(&mut self, down: bool, window: &mut Window, cx: &mut Context<Self>) {
        let visible = self.visible_channels();
        let current = self.current_channel().map(|c| c.conversation_id);
        if let Some(next) = d::step_channel(&visible, current, down) {
            self.open_channel(next, window, cx);
        }
    }

    /// The Team chat's sidebar badge (B8): mentions, and a dot while anything is unread.
    pub fn team_chat_badge(&self) -> (u32, bool) {
        self.channels.as_deref().map_or((0, false), d::badge)
    }

    // ---- The @ suggestions of the composer (ui-ontwerp §4) ------------------------

    /// The `@` suggestions above the Team chat composer: where the `@` is, and whom.
    pub fn group_suggestions(&self, cx: &App) -> Vec<(usize, d::Mentionable)> {
        let text = self.composer.read(cx).value().to_string();
        if !self.group_mentions.shows(&text) {
            return Vec::new();
        }
        let people = self.mentionables();
        d::mention_suggestions(&text, &people).map(|(at, found)| found.into_iter().map(|p| (at, p.clone())).collect()).unwrap_or_default()
    }

    /// ↑ / ↓ in the suggestions.
    pub fn group_step_mention(&mut self, down: bool, cx: &mut Context<Self>) {
        let of = self.group_suggestions(cx).len();
        self.group_mentions.step(down, of);
        cx.notify();
    }

    /// Enter / Tab, or a click: complete the `@word` to that name.
    pub fn group_complete(&mut self, at: usize, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.composer.read(cx).value().to_string();
        if at > text.len() {
            return;
        }
        let mention = format!("@{name} ");
        let end = text.len();
        self.composer.update(cx, |input, cx| {
            input.set_selected_range(at..end, cx);
            input.replace(mention, window, cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    /// Enter / Tab: the active suggestion.
    pub fn group_pick_mention(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let suggestions = self.group_suggestions(cx);
        if let Some((at, p)) = self.group_mentions.active(suggestions.len()).and_then(|i| suggestions.into_iter().nth(i)) {
            self.group_complete(at, &p.name, window, cx);
        }
    }

    /// Esc: close the suggestions (they open again when the text changes).
    pub fn group_close_mentions(&mut self, cx: &mut Context<Self>) {
        let text = self.composer.read(cx).value().to_string();
        self.group_mentions.close(&text);
        cx.notify();
    }

    // ---- The channel list ----------------------------------------------------------

    /// One row of the channel list.
    fn channel_row(&self, ch: &ChannelView, i: usize, current: Option<ConversationId>, cx: &mut Context<Self>) -> AnyElement {
        let on = current == Some(ch.conversation_id);
        let id = ch.conversation_id;
        let bold = ch.unread > 0 && !on;
        h().id(SharedString::from(format!("channel-{i}"))).role(Role::Tab).aria_label(if on { format!("{}, selected", d::channel_label(ch)) } else { d::channel_label(ch) })
            .gap(px(6.)).px(px(10.)).py(px(4.)).rounded(px(6.)).cursor_pointer()
            .text_size(px(12.5))
            .text_color(c(if on || bold { theme::TEXT } else { theme::TEXT_2 }))
            .when(on, |el| el.bg(c(theme::SURFACE_2)))
            .when(bold, |el| el.font_weight(FontWeight::SEMIBOLD))
            .hover(|s| s.bg(c(theme::SURFACE_2)))
            .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.open_channel(id, w, cx)))
            .child(div().flex_1().min_w_0().truncate().child(d::channel_title(ch)))
            // A mention of a board member: an "@" badge, never colour alone (P-24); the open channel shows none.
            .when(ch.mentions > 0 && !on, |el| el.child(div().px(px(5.)).rounded(px(8.)).bg(c(theme::ACCENT)).text_color(c(theme::BG)).text_size(px(11.)).child(format!("@{}", ch.mentions))))
            .when(ch.unread > 0 && ch.mentions == 0 && !on, |el| el.child(div().px(px(5.)).rounded(px(8.)).bg(c(theme::SURFACE_2)).text_color(c(theme::TEXT_2)).text_size(px(11.)).child(ch.unread.to_string())))
            .when(ch.archived, |el| el.child(div().text_size(px(11.)).text_color(c(theme::TEXT_2)).child("read-only")))
            .into_any_element()
    }

    /// "Show more (n)" under a section cut at `SHOW_MORE_AFTER` rows.
    fn show_more(&self, key: &'static str, hidden: usize, cx: &mut Context<Self>, more: fn(&mut AppView)) -> Option<AnyElement> {
        (hidden > 0).then(|| {
            div().id(key).role(Role::Button).aria_label(format!("Show {hidden} more")).px(px(10.)).py(px(4.)).cursor_pointer().text_size(px(11.5)).text_color(c(theme::ACCENT_TEXT))
                .child(format!("Show more ({hidden})"))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    more(this);
                    cx.notify();
                }))
                .into_any_element()
        })
    }

    /// The channel list on the left of the Team chat (ui-ontwerp §1): the search
    /// field, then Channels, Tickets and Archived, or what the search found.
    fn render_channel_list(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let list = self.channels.as_ref()?;
        let current = self.current_channel().map(|c| c.conversation_id);
        let eyebrow = |label: &'static str, first: bool| div().px(px(10.)).pt(px(if first { 0. } else { 12. })).pb(px(4.)).child(ui::eyebrow(label));
        let mut i = 0;
        let mut rows: Vec<AnyElement> = Vec::new();
        if let Some((_, found)) = &self.channel_results {
            rows.push(eyebrow("RESULTS", true).into_any_element());
            if found.is_empty() {
                rows.push(div().px(px(10.)).text_size(px(12.)).text_color(c(theme::TEXT_2)).child("No channel matches.").into_any_element());
            }
            for ch in found {
                i += 1;
                rows.push(self.channel_row(ch, i, current, cx));
            }
        } else {
            let mut s = d::sections(list);
            if let Some(frozen) = &self.channel_order {
                s.tickets = d::keep_order(s.tickets, frozen);
            }
            rows.push(eyebrow("CHANNELS", true).into_any_element());
            for ch in s.general.into_iter().chain(s.projects.iter().copied()) {
                i += 1;
                rows.push(self.channel_row(ch, i, current, cx));
            }
            if !s.tickets.is_empty() {
                rows.push(eyebrow("TICKETS", false).into_any_element());
                let shown = d::shown(s.tickets.len(), self.channels_tickets_all);
                for ch in s.tickets.iter().take(shown) {
                    i += 1;
                    rows.push(self.channel_row(ch, i, current, cx));
                }
                rows.extend(self.show_more("channels-tickets-more", s.tickets.len() - shown, cx, |this| this.channels_tickets_all = true));
            }
            if !s.archived.is_empty() {
                let open = self.channels_archived_open;
                rows.push(
                    div().id("channels-archived").role(Role::Button).aria_label(if open { "Archived, expanded" } else { "Archived, collapsed" }).px(px(10.)).pt(px(12.)).pb(px(4.)).cursor_pointer().text_size(px(11.5)).text_color(c(theme::TEXT_2))
                        .child(format!("{} Archived ({})", if open { "▾" } else { "▸" }, s.archived.len()))
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                            this.channels_archived_open = !this.channels_archived_open;
                            cx.notify();
                        }))
                        .into_any_element(),
                );
                if open {
                    let shown = d::shown(s.archived.len(), self.channels_archived_all);
                    for ch in s.archived.iter().take(shown) {
                        i += 1;
                        rows.push(self.channel_row(ch, i, current, cx));
                    }
                    rows.extend(self.show_more("channels-archived-more", s.archived.len() - shown, cx, |this| this.channels_archived_all = true));
                }
            }
        }
        let order: Vec<ConversationId> = d::sections(list).tickets.iter().map(|c| c.conversation_id).collect();
        Some(
            v().id("channel-list").role(Role::TabList).aria_label("Channels").w(px(180.)).flex_none().h_full().overflow_y_scroll().gap(px(2.)).px(px(8.)).py(px(14.)).border_r_1().border_color(c(theme::BORDER))
                // While the pointer is over the list, the tickets keep their order (ui-ontwerp §1).
                .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                    this.channel_order = hovered.then(|| this.channel_order.clone().unwrap_or_else(|| order.clone()));
                    cx.notify();
                }))
                .child(div().pb(px(10.)).child(ui::input(&self.channel_search).h(px(28.)).text_size(px(12.5))))
                .children(rows)
                .into_any_element(),
        )
    }

    /// The header's second line in a ticket's channel (ui-ontwerp §3): the status chip,
    /// the title and the assignees (owner first; more than two fold into "+n").
    fn ticket_line(&self, ch: &ChannelView, cx: &mut Context<Self>) -> AnyElement {
        let status = ch.ticket_column.map(|k| self.board.column_name(k));
        let detail = self.channel_ticket.as_ref().filter(|d| Some(d.ticket.id) == ch.ticket_id);
        let names = detail.map(d::assignees).unwrap_or_default();
        let (shown, rest) = names.split_at(names.len().min(d::ASSIGNEES_SHOWN));
        let rest = rest.to_vec();
        h().min_w_0().gap(px(8.)).text_size(px(12.5)).text_color(c(theme::TEXT_2))
            .children(status.map(|s| div().flex_none().px(px(8.)).py(px(1.)).rounded(px(10.)).border_1().border_color(c(theme::BORDER_STRONG)).text_size(px(11.5)).text_color(c(theme::TEXT)).child(s)))
            .children(detail.map(|d| div().min_w_0().truncate().child(format!("· {}", d.ticket.title))))
            .children(shown.iter().map(|name| h().flex_none().gap(px(5.)).child(ui::monogram(name, 16., cx)).child(name.clone())))
            .when(!rest.is_empty(), |el| {
                el.child(div().id("ticket-assignees-more").flex_none().child(format!("+{}", rest.len())).tooltip(ui::tip(rest.join(", "), None)))
            })
            .into_any_element()
    }

    pub fn render_group_chat(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let team = self.managers.iter().filter(|m| m.status == LifecycleStatus::Active).count();
        // Who is writing an answer here right now.
        let answering: Vec<String> = self.group_live.iter().filter_map(|l| l.author.clone().or_else(|| self.run_author(l.run))).collect();
        let current = self.current_channel().cloned();
        let ticket = current.as_ref().and_then(|c| c.ticket_id.filter(|_| d::kind_of(c) == ChannelKind::Ticket));
        let subtitle = match ticket.and(current.as_ref()) {
            Some(ch) => self.ticket_line(ch, cx),
            None => div().truncate().text_size(px(12.5)).text_color(c(theme::TEXT_2)).child(match (current.as_ref(), team) {
                (Some(ch), _) if ch.archived => "Archived project · read-only".to_string(),
                (Some(ch), 1) if ch.project_id.is_some() => "Project channel · you and 1 employee".to_string(),
                (Some(ch), n) if ch.project_id.is_some() => format!("Project channel · you and {n} employees"),
                (_, 1) => "You and 1 employee".to_string(),
                (_, n) => format!("You and {n} employees"),
            }).into_any_element(),
        };
        let header = h()
            .h(px(68.))
            .flex_none()
            .px(px(28.))
            .gap(px(14.))
            .border_b_1()
            .border_color(c(theme::BORDER))
            .child(
                v().flex_1().min_w_0()
                    .child(div().truncate().text_size(px(15.)).font_weight(FontWeight::MEDIUM).child(current.as_ref().map_or_else(|| crate::app::NavItem::Chat.label().to_string(), d::channel_title)))
                    .child(subtitle),
            )
            .when(!answering.is_empty(), |el| {
                el.child(h().gap(px(7.)).text_size(px(12.5)).child(ui::pulse_dot("group-answering", 6., theme::ACCENT)).child(format!("{} answering", answering.join(", "))))
            })
            .when_some(ticket, |el, id| {
                el.child(ui::button("channel-open-ticket", "Open ticket ↗", ButtonKind::Outline, None).h(px(28.)).on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.open_ticket(id, w, cx))))
            });
        let read_only = self.channel_read_only();
        let list = self.render_channel_list(cx);
        let chat = v().flex_1()
            .min_w_0()
            .h_full()
            // Files dropped anywhere on the conversation go with the next message.
            .drag_over::<ExternalPaths>(|style, _, _, _| style.bg(c(theme::SURFACE)))
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| this.drop_files(paths.paths(), cx)))
            .child(header)
            .child(self.conversation_pane(|this, _, cx| this.group_scroll(cx), cx))
            .map(|el| match current.as_ref().filter(|_| read_only) {
                // A closed ticket's or an archived project's channel: a read-only bar, no composer (ui-ontwerp §3).
                Some(ch) => el.child(
                    h().id("channel-read-only").role(Role::Status).flex_none().gap(px(10.)).px(px(28.)).py(px(14.)).border_t_1().border_color(c(theme::BORDER)).bg(c(theme::SURFACE)).text_size(px(12.5)).text_color(c(theme::TEXT_2))
                        .child(d::read_only_bar(ch))
                        .when_some(ticket, |el, id| {
                            el.child(div().id("channel-read-only-open").role(Role::Link).cursor_pointer().text_color(c(theme::ACCENT_TEXT)).child("Open ticket").on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.open_ticket(id, w, cx))))
                        }),
                ),
                None => el.child(composer(self, "", false, cx)),
            });
        match list {
            Some(list) => h().flex_1().min_w_0().h_full().items_start().child(list).child(chat).into_any_element(),
            None => chat.into_any_element(),
        }
    }

    /// What employees' runs ask from their main chat (where group chat answers are written)
    /// with no answer streaming here to show it under: decided right here, under the asker's name.
    pub fn group_approvals(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let asks: Vec<_> = self.pending_approvals()
            .filter(|a| a.conversation_id.is_some_and(|id| self.is_main_chat(id)) && !self.group_live.iter().any(|l| l.run == a.run_id))
            .cloned()
            .collect();
        asks.into_iter().map(|a| {
            let (label, body): (&str, AnyElement) = if a.tool == crate::app::QUESTION_TOOL {
                ("QUESTION · WAITING FOR YOU", self.question_form(&a, None, cx).into_any_element())
            } else if a.tool == crate::app::PLAN_TOOL {
                ("PLAN · WAITING FOR YOU", div().text_size(px(13.5)).text_color(c(theme::TEXT_2)).child("A plan to decide on.").into_any_element())
            } else {
                let summary = crate::screens::approval::approval_summary(&a, &self.approval_folders(&a));
                ("WAITING FOR YOU", v().gap(px(8.))
                    .child(div().text_size(px(13.5)).text_color(c(theme::TEXT)).child(summary.title))
                    .child(self.approval_detail(&a, "group", cx))
                    .into_any_element())
            };
            let id = a.id;
            v().gap(px(6.)).px(px(14.)).py(px(11.)).rounded(px(8.)).border_1()
                .border_color(c(theme::ATTENTION_BORDER)).bg(c(theme::ATTENTION_BG))
                .child(h().gap(px(8.)).text_size(px(11.5)).text_color(c(theme::MUTED)).child(ui::eyebrow(label)).child(div().flex_1().min_w_0().text_color(c(theme::TEXT_2)).child(a.requester.clone())))
                .child(body)
                .child(self.approval_actions(a, "group", cx))
                .id(SharedString::from(format!("group-ask-{id}")))
                .into_any_element()
        }).collect()
    }

    /// The group chat's scroll area, in the conversation pane.
    fn group_scroll(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let empty = self.messages.as_ref().is_some_and(|s| s.messages.is_empty()) && self.group_live.is_empty();
        // It did not load: say why, instead of "Loading…" for good.
        let failed = self.group_conversation.is_none().then(|| self.group_error.clone()).flatten();
        let body = v().w_full().max_w(px(700.)).gap(px(16.))
            .when_some(failed.clone(), |el, why| {
                el.child(
                    v().gap(px(10.)).text_size(px(13.5)).text_color(c(theme::TEXT_2))
                        .child(div().text_size(px(15.)).text_color(c(theme::TEXT)).child("The Team chat did not load."))
                        .child(why)
                        .child(h().child(ui::button("group-retry", "Try again", ButtonKind::Outline, None).h(px(28.))
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.retry_group_chat(cx))))),
                )
            })
            .when(empty, |el| {
                let first = self.group_answerer().map_or_else(|| "the top of the org chart".to_string(), str::to_owned);
                el.child(
                    v().gap(px(6.)).text_size(px(13.5)).text_color(c(theme::TEXT_2))
                        .child(div().text_size(px(15.)).text_color(c(theme::TEXT)).child("Talk with your whole team here."))
                        .child(format!("Write @Name to ask an employee; without a name, {first} answers and brings in whoever is needed.")),
                )
            })
            .when(failed.is_none(), |el| el.child(self.render_messages("Aurelius".into(), cx)));
        ui::scroll_frame(&self.scroll,
            div()
                .id("group-scroll")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .track_scroll(&self.scroll)
                .track_focus(&self.focus)
                .key_context(NAV)
                .flex()
                .flex_col()
                .items_center()
                .px(px(36.))
                .pt(px(26.))
                .pb(px(16.))
                .child(ui::rise(div().flex_none().w_full().flex().justify_center().child(body), ("page", self.page_key()), 0)),
        ).flex_1().into_any_element()
    }
}
