//! The Inbox's rows per ticket or channel (AUR-FA A3, feature `personal_inbox`; freeze §2 and §4,
//! ui-ontwerp §4): the heading from the row's `reason`, `+n` for what it merged (it opens the
//! ticket's Activity, there is no list of events per row), ◆ on `needs_you`, Copy link and
//! Unfollow. A reason this client does not know is a plain row with the server's sentence.
//! The rules are pure here; the list itself is in `overlays.rs`.

use crate::app::{AppView, NavItem};
use crate::backend::{self, BoardCall};
use crate::screens::board::TicketTab;
use crate::screens::links::LinkTarget;
use crate::screens::who::{self, FollowState};
use crate::theme::{self, c};
use crate::ui::{self, ButtonKind, h, v};
use aurelius_domain::ticket::{FollowerView, TicketView};
use aurelius_domain::{ConversationId, MessageId, TicketId, UserId};
use aurelius_protocol::NotificationView;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::collections::HashMap;

/// What a tab without updates says (§4.6).
pub const NO_UPDATES: &str = "No updates.";
/// And why there can be none.
pub const NO_UPDATES_ABOUT: &str = "You get one when you're made responsible or @mentioned, and when a ticket you follow needs review, is sent back or fails.";
/// `Mark all read` and `Clear all` are per person (§4.4).
pub const OTHERS_KEEP_THEIRS: &str = "Others keep theirs.";

/// A row of the personal Inbox: only those have a reason, also one this client does not know.
pub fn is_update(n: &NotificationView) -> bool {
    n.reason.is_some()
}

/// The row's heading (§4.1). The sentences that depend on who reads them follow `needs_you`,
/// which the server works out on reading, so they follow a handover; those that name someone
/// come from the server. Any other reason is a plain row: the server's sentence.
pub fn heading(n: &NotificationView) -> String {
    match (n.reason.as_deref(), n.needs_you) {
        (Some("plan_review"), true) => "Plan ready for your review".into(),
        (Some("plan_review"), false) => "Plan ready for review".into(),
        (Some("review"), true) => "Ready for your review".into(),
        (Some("review"), false) => "Ready for review".into(),
        (Some("failed"), _) => "The main task failed".into(),
        (Some("login"), _) => "Waiting for your Claude login".into(),
        _ => n.title.clone(),
    }
}

/// Where the row is: `#42 Title`, or `# general` outside a ticket. `title`: the ticket's as this
/// client knows it, for a row whose subject is empty.
pub fn place(n: &NotificationView, title: Option<&str>) -> String {
    let subject = Some(n.subject.as_str()).filter(|s| !s.is_empty()).or(title);
    match (n.ticket_number, subject) {
        (Some(number), Some(subject)) => format!("#{number} {subject}"),
        (Some(number), None) => format!("#{number}"),
        (None, Some(subject)) if subject.starts_with('#') => subject.to_string(),
        (None, Some(subject)) if n.ticket_id.is_none() => format!("# {subject}"),
        (None, Some(subject)) => subject.to_string(),
        (None, None) => "Update".into(),
    }
}

/// `+2`: how many more events the row stands for.
pub fn more(n: &NotificationView) -> Option<String> {
    (n.merged_count > 0).then(|| format!("+{}", n.merged_count))
}

/// The eyebrow of the row's detail: `TICKET · 3 UPDATES`.
pub fn eyebrow(n: &NotificationView) -> String {
    let kind = if n.ticket_id.is_some() { "TICKET" } else { "CHANNEL" };
    match n.merged_count {
        0 => kind.to_string(),
        more => format!("{kind} · {} UPDATES", more + 1),
    }
}

/// What a screen reader hears (§9).
pub fn spoken(n: &NotificationView, title: Option<&str>, unread: bool, age: &str) -> String {
    let mut parts = vec![place(n, title).replacen('#', if n.ticket_number.is_some() { "Ticket " } else { "Channel " }, 1), heading(n)];
    if n.needs_you {
        parts.push("needs you".into());
    }
    match n.merged_count {
        0 => {}
        1 => parts.push("1 more update".into()),
        more => parts.push(format!("{more} more updates")),
    }
    if unread {
        parts.push("unread".into());
    }
    parts.push(age.to_string());
    parts.join(", ")
}

/// Where opening a row goes (§4.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowTarget {
    Ticket(TicketId, TicketTab),
    Message(MessageId),
    Conversation(ConversationId),
    /// A row with only a task (or nothing): as a notification without a reason.
    Task,
}

/// The row's main reason on the right place. `locate`: this server finds a message's place.
pub fn row_target(n: &NotificationView, locate: bool) -> RowTarget {
    let message = n.message_id.filter(|_| locate);
    let in_discussion = matches!(n.reason.as_deref(), Some("mention" | "sent_back"));
    match (n.ticket_id, message, n.conversation_id) {
        (Some(_), Some(m), _) if in_discussion => RowTarget::Message(m),
        (Some(t), None, _) if in_discussion => RowTarget::Ticket(t, TicketTab::Discussion),
        (Some(t), ..) => RowTarget::Ticket(t, TicketTab::Ticket),
        (None, Some(m), _) => RowTarget::Message(m),
        (None, None, Some(conversation)) => RowTarget::Conversation(conversation),
        (None, None, None) => RowTarget::Task,
    }
}

/// The main button of a row's detail.
pub fn open_label(n: &NotificationView) -> &'static str {
    match (n.reason.as_deref(), n.ticket_id, n.message_id) {
        (Some("review"), Some(_), _) => "Review",
        (Some("plan_review"), Some(_), _) => "Review plan",
        (Some("mention"), _, Some(_)) => "Open message",
        (_, Some(_), _) => "Open ticket",
        (_, None, _) => "Open channel",
    }
}

/// What Copy link on a row copies: the ticket, or outside a ticket the message; a channel row
/// without a message has no place a link can name.
pub fn row_link(n: &NotificationView) -> Option<LinkTarget> {
    n.ticket_id.map(LinkTarget::Ticket).or(n.message_id.map(LinkTarget::Message))
}

/// Whether a row counts as unread: not read on the server, and not opened here since its last
/// event. `opened`: the rows opened here, with their `created_at_ms` then (a new event moves it).
pub fn row_unread(n: &NotificationView, opened: &HashMap<String, i64>) -> bool {
    !n.read && opened.get(&n.id).is_none_or(|at| n.created_at_ms > *at)
}

/// The Unfollow of a row (B10).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowUnfollow {
    /// A channel's row, a server without following, or the reader does not follow.
    Hidden,
    /// The Responsible cannot stop following.
    Locked,
    Shown,
}

/// Unfollow on a row. Until the followers are read it shows: `ticket.follow` is idempotent and
/// the server refuses it for the Responsible, in words.
pub fn row_unfollow(n: &NotificationView, supported: bool, ticket: Option<&TicketView>, followers: Option<&[FollowerView]>, me: Option<UserId>) -> RowUnfollow {
    if !supported || n.ticket_id.is_none() {
        return RowUnfollow::Hidden;
    }
    let locked = ticket.is_some_and(|t| me.is_some() && t.responsible_user_id == me);
    let listed = followers.map(|list| list.iter().any(|f| Some(f.user_id) == me));
    match (locked, listed) {
        (true, _) => RowUnfollow::Locked,
        (false, Some(false)) => RowUnfollow::Hidden,
        (false, _) => RowUnfollow::Shown,
    }
}

impl AppView {
    /// The personal Inbox is on this server (feature `personal_inbox`).
    pub fn personal_inbox_supported(&self) -> bool {
        matches!(&self.connection, backend::Connection::Online { server, .. } if server.personal_inbox)
    }

    /// The ticket's title as this client knows it.
    fn row_ticket(&self, n: &NotificationView) -> Option<&TicketView> {
        n.ticket_id.and_then(|id| self.board.ticket(id))
    }

    /// Open a row on the right place; that reads it (§4.4).
    pub fn open_update_row(&mut self, n: &NotificationView, window: &mut Window, cx: &mut Context<Self>) {
        self.inbox_opened.insert(n.id.clone(), n.created_at_ms);
        match row_target(n, self.message_locate_supported()) {
            RowTarget::Ticket(id, tab) => self.open_row_ticket(id, tab, window, cx),
            RowTarget::Message(id) => {
                let _ = self.requests.send(backend::Request::Board(BoardCall::Locate(id)));
            }
            RowTarget::Conversation(id) => self.go_to_channel(id, window, cx),
            RowTarget::Task => self.notify_user("This notification has nothing to open.", false, cx),
        }
        cx.notify();
    }

    /// `+n` opens the ticket's Activity (freeze §4: no list of events per row).
    pub fn open_row_activity(&mut self, n: &NotificationView, window: &mut Window, cx: &mut Context<Self>) {
        self.inbox_opened.insert(n.id.clone(), n.created_at_ms);
        if let Some(id) = n.ticket_id {
            self.open_row_ticket(id, TicketTab::Activity, window, cx);
        }
        cx.notify();
    }

    fn open_row_ticket(&mut self, id: TicketId, tab: TicketTab, window: &mut Window, cx: &mut Context<Self>) {
        self.go_nav(NavItem::Board, window, cx);
        self.open_ticket(id, window, cx);
        self.ticket_tab(tab, cx);
    }

    /// Copy link on a row; a server without links has no such button.
    pub fn copy_row_link(&mut self, n: &NotificationView, cx: &mut Context<Self>) {
        match row_link(n) {
            Some(LinkTarget::Ticket(id)) => self.copy_ticket_link_numbered(id, n.ticket_number.unwrap_or(0), cx),
            Some(LinkTarget::Message(id)) => self.copy_message_link(id, cx),
            None => {}
        }
    }

    /// Read who follows the selected row's ticket once, so its Unfollow knows.
    pub fn inbox_ask_followers(&mut self, id: TicketId) {
        if self.inbox_followers_asked != Some(id) && !self.board.followers.contains_key(&id) {
            self.inbox_followers_asked = Some(id);
            self.ticket_read_followers(id);
        }
    }

    /// One row of the list: where, then the heading with its `+n`.
    pub fn update_row(&self, n: &NotificationView, j: usize, index: usize, selected: bool, now: i64, cx: &Context<Self>) -> AnyElement {
        let unread = row_unread(n, &self.inbox_opened);
        let title = self.row_ticket(n).map(|t| t.title.as_str());
        let age = ui::relative_age(n.created_at_ms, now);
        let mark = if n.needs_you { ui::diamond(8., theme::ACCENT_TEXT, true).into_any_element() } else if unread { ui::dot(7., theme::ACCENT).into_any_element() } else { ui::ring(7., theme::MUTED).into_any_element() };
        let row = n.clone();
        let more_button = more(n).map(|text| {
            let opens = n.ticket_id.is_some();
            div().id(("inbox-more", j)).flex_none().px(px(4.)).rounded(px(4.)).text_size(px(11.5)).text_color(c(theme::ACCENT_TEXT))
                .when(opens, |el| {
                    el.cursor_pointer().hover(|s| s.bg(c(theme::BORDER))).tooltip(ui::tip("Open the Activity", None)).on_click(cx.listener(move |this, _: &ClickEvent, w, cx| {
                        cx.stop_propagation();
                        this.open_row_activity(&row, w, cx);
                    }))
                })
                .child(text)
        });
        h().id(("inbox-update", j)).gap(px(8.)).items_start().px(px(10.)).py(px(8.)).rounded(px(6.)).cursor_pointer()
            .aria_label(spoken(n, title, unread, &age))
            .when(selected, |el| el.bg(c(theme::SURFACE_2)))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.inbox_selected = index;
                this.question_focus = 0;
                cx.notify();
            }))
            .child(h().w(px(10.)).h(px(18.)).flex_none().justify_center().items_center().child(mark))
            .child(
                v().flex_1().min_w_0().gap(px(1.))
                    .child(h().gap(px(6.))
                        .child(div().flex_1().min_w_0().truncate().text_size(px(13.)).when(unread, |el| el.font_weight(FontWeight::BOLD)).child(place(n, title)))
                        .child(div().flex_none().text_size(px(11.)).text_color(c(theme::MUTED)).child(age)))
                    .child(h().gap(px(4.)).min_w_0()
                        .child(div().flex_shrink(1.).min_w_0().truncate().text_size(px(12.)).text_color(c(theme::TEXT_2)).child(heading(n)))
                        .children(more_button)),
            )
            .into_any_element()
    }

    /// The detail of a row: what it is about and who has it; no list of events (freeze §4).
    pub fn update_detail(&self, n: &NotificationView, now: i64, cx: &Context<Self>) -> AnyElement {
        let ticket = self.row_ticket(n);
        let me = self.active_member().map(|u| u.id);
        let row = n.clone();
        v().p(px(22.)).gap(px(12.))
            .child(ui::eyebrow(eyebrow(n)))
            .child(div().font_family(ui::serif(cx)).text_size(px(21.)).child(place(n, ticket.map(|t| t.title.as_str()))))
            .children(ticket.map(|t| {
                let work = who::work_line(t, me, self.login_disabled(t));
                let responsible = who::responsible_line(t, me);
                v().gap(px(2.)).text_size(px(12.5)).text_color(c(theme::TEXT_2))
                    .child(format!("{}{}", work.name, work.tail))
                    .when(self.ticket_responsible_supported(), |el| el.child(format!("{}{}", responsible.name, responsible.tail)))
            }))
            .child(h().gap(px(8.)).items_center().text_size(px(14.))
                .when(n.needs_you, |el| el.child(ui::diamond(9., theme::ACCENT_TEXT, true)))
                .child(heading(n))
                .child(div().text_size(px(11.5)).text_color(c(theme::MUTED)).child(ui::relative_age(n.created_at_ms, now))))
            .children(more(n).filter(|_| n.ticket_id.is_some()).map(|text| {
                div().id("inbox-detail-more").text_size(px(12.5)).text_color(c(theme::ACCENT_TEXT)).cursor_pointer()
                    .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.open_row_activity(&row, w, cx)))
                    .child(format!("{text} more in the Activity"))
            }))
            .into_any_element()
    }

    /// The bar under a row's detail: Copy link, Unfollow, Read, and the main button.
    pub fn update_actions(&self, n: &NotificationView, cx: &Context<Self>) -> Vec<AnyElement> {
        let mut out = Vec::new();
        if self.message_locate_supported() && row_link(n).is_some() {
            let row = n.clone();
            out.push(ui::button("inbox-copy-link", "Copy link", ButtonKind::Outline, None).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.copy_row_link(&row, cx))).into_any_element());
        }
        let ticket = self.row_ticket(n);
        let followers = n.ticket_id.and_then(|id| self.board.followers.get(&id)).and_then(|r| r.as_ref().ok()).map(Vec::as_slice);
        match (row_unfollow(n, self.ticket_follow_supported(), ticket, followers, self.active_member().map(|u| u.id)), n.ticket_id) {
            (RowUnfollow::Shown, Some(id)) => {
                let number = n.ticket_number.unwrap_or(0);
                out.push(ui::button("inbox-unfollow", "Unfollow", ButtonKind::Outline, None).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.ticket_follow_numbered(id, number, false, cx))).into_any_element());
            }
            (RowUnfollow::Locked, Some(_)) => {
                let state = FollowState { following: true, locked: true };
                let tip = who::follow_tip(state, None, n.ticket_number.unwrap_or(0), ticket.map_or(aurelius_domain::ticket::ColumnKind::Doing, |t| t.column));
                out.push(ui::button("inbox-unfollow", "Unfollow", ButtonKind::Outline, None).opacity(0.5).tooltip(ui::tip(tip, None)).into_any_element());
            }
            _ => {}
        }
        if row_unread(n, &self.inbox_opened) {
            let (id, at) = (n.id.clone(), n.created_at_ms);
            out.push(ui::button("inbox-read", "Read", ButtonKind::Outline, None).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.inbox_opened.insert(id.clone(), at);
                cx.notify();
            })).into_any_element());
        }
        out.push(ui::button("open-update", open_label(n), ButtonKind::Primary, Some(ui::kbd("enter", cx))).on_click(cx.listener(|_, _: &ClickEvent, w, cx| w.dispatch_action(Box::new(crate::keymap::OpenSelected), cx))).into_any_element());
        out
    }
}

#[cfg(test)]
pub mod tests {
    use super::{RowTarget, RowUnfollow, eyebrow, heading, is_update, more, open_label, place, row_link, row_target, row_unfollow, row_unread, spoken};
    use crate::screens::board::TicketTab;
    use crate::screens::links::LinkTarget;
    use aurelius_domain::ticket::FollowerView;
    use aurelius_domain::{ConversationId, MessageId, TicketId, UserId};
    use aurelius_protocol::NotificationView;
    use std::collections::HashMap;

    /// A row as the server sends it for a ticket.
    pub fn row(id: &str, reason: Option<&str>) -> NotificationView {
        NotificationView {
            id: id.into(),
            severity: "info".into(),
            title: "Marie made you responsible".into(),
            subject: "Login per board member".into(),
            created_at_ms: 1_000,
            read: false,
            task_id: None,
            target_type: "ticket.updated".into(),
            ticket_id: Some(TicketId::new()),
            ticket_number: Some(42),
            conversation_id: Some(ConversationId::new()),
            message_id: None,
            reason: reason.map(str::to_string),
            merged_count: 0,
            needs_you: false,
        }
    }

    /// A mention outside a ticket: a channel's row.
    fn channel_row(id: &str) -> NotificationView {
        NotificationView { ticket_id: None, ticket_number: None, subject: "general".into(), title: "Atlas mentioned you".into(), message_id: Some(MessageId::new()), ..row(id, Some("mention")) }
    }

    #[test]
    fn the_heading_comes_from_the_reason_and_follows_who_reads_it() {
        let mut r = row("a", Some("review"));
        r.title = "Ready for your review".into();
        assert_eq!(heading(&r), "Ready for review", "handed over: the stored sentence is old, needs_you is not");
        r.needs_you = true;
        assert_eq!(heading(&r), "Ready for your review");
        assert_eq!(heading(&NotificationView { needs_you: true, ..row("b", Some("plan_review")) }), "Plan ready for your review");
        assert_eq!(heading(&row("c", Some("plan_review"))), "Plan ready for review");
        assert_eq!(heading(&row("d", Some("failed"))), "The main task failed");
        assert_eq!(heading(&row("e", Some("login"))), "Waiting for your Claude login");
        for named in ["responsible", "mention", "sent_back"] {
            assert_eq!(heading(&row("f", Some(named))), "Marie made you responsible", "{named}: the server's sentence names who");
        }
    }

    #[test]
    fn an_unknown_reason_is_a_plain_row_with_the_servers_sentence() {
        let r = row("a", Some("reason_of_a_newer_server"));
        assert!(is_update(&r));
        assert_eq!(heading(&r), "Marie made you responsible");
        assert_eq!(row_target(&r, true), RowTarget::Ticket(r.ticket_id.unwrap(), TicketTab::Ticket));
        assert_eq!(open_label(&r), "Open ticket");
        assert!(!is_update(&row("b", None)), "without a reason it is a notification as before");
    }

    #[test]
    fn a_row_says_where_it_is() {
        assert_eq!(place(&row("a", None), None), "#42 Login per board member");
        assert_eq!(place(&NotificationView { subject: String::new(), ..row("a", None) }, Some("From the board")), "#42 From the board");
        assert_eq!(place(&NotificationView { subject: String::new(), ..row("a", None) }, None), "#42");
        assert_eq!(place(&channel_row("b"), None), "# general");
        assert_eq!(place(&NotificationView { subject: "# general".into(), ..channel_row("b") }, None), "# general");
    }

    #[test]
    fn merged_events_show_as_plus_n_and_in_the_eyebrow() {
        let mut r = row("a", Some("review"));
        assert_eq!((more(&r), eyebrow(&r)), (None, "TICKET".to_string()));
        r.merged_count = 2;
        assert_eq!((more(&r).as_deref(), eyebrow(&r).as_str()), (Some("+2"), "TICKET · 3 UPDATES"));
        assert_eq!(eyebrow(&channel_row("b")), "CHANNEL");
    }

    #[test]
    fn a_row_reads_as_one_sentence() {
        let mut r = row("a", Some("review"));
        r.needs_you = true;
        r.merged_count = 2;
        assert_eq!(spoken(&r, None, true, "2m ago"), "Ticket 42 Login per board member, Ready for your review, needs you, 2 more updates, unread, 2m ago");
    }

    #[test]
    fn opening_a_row_goes_to_the_place_of_its_reason() {
        let mention = NotificationView { message_id: Some(MessageId::new()), ..row("a", Some("mention")) };
        assert_eq!(row_target(&mention, true), RowTarget::Message(mention.message_id.unwrap()));
        assert_eq!(row_target(&mention, false), RowTarget::Ticket(mention.ticket_id.unwrap(), TicketTab::Discussion), "no message.locate: at least the discussion");
        let review = NotificationView { message_id: Some(MessageId::new()), ..row("b", Some("review")) };
        assert_eq!(row_target(&review, true), RowTarget::Ticket(review.ticket_id.unwrap(), TicketTab::Ticket));
        let channel = channel_row("c");
        assert_eq!(row_target(&channel, true), RowTarget::Message(channel.message_id.unwrap()));
        assert_eq!(row_target(&channel, false), RowTarget::Conversation(channel.conversation_id.unwrap()));
        assert_eq!(row_target(&NotificationView { ticket_id: None, conversation_id: None, ..row("d", Some("zzz")) }, true), RowTarget::Task);
    }

    #[test]
    fn copy_link_names_the_ticket_or_outside_a_ticket_the_message() {
        let ticket = row("a", Some("mention"));
        assert_eq!(row_link(&ticket), Some(LinkTarget::Ticket(ticket.ticket_id.unwrap())));
        let channel = channel_row("b");
        assert_eq!(row_link(&channel), Some(LinkTarget::Message(channel.message_id.unwrap())));
        assert_eq!(row_link(&NotificationView { message_id: None, ..channel }), None);
    }

    #[test]
    fn a_row_opened_here_is_read_until_its_next_event() {
        let mut r = row("a", Some("mention"));
        let mut opened = HashMap::new();
        assert!(row_unread(&r, &opened));
        opened.insert("a".to_string(), r.created_at_ms);
        assert!(!row_unread(&r, &opened));
        r.created_at_ms += 1;
        assert!(row_unread(&r, &opened), "a new event puts it on unread again");
        r.read = true;
        assert!(!row_unread(&r, &HashMap::new()));
    }

    #[test]
    fn unfollow_shows_on_a_ticket_row_the_reader_follows_and_not_for_the_responsible() {
        let me = UserId::new();
        let r = row("a", Some("mention"));
        let mine = FollowerView { user_id: me, username: "ada".into(), disabled: false, reason: "mentioned".into(), followed_at_ms: 1 };
        assert_eq!(row_unfollow(&r, true, None, None, Some(me)), RowUnfollow::Shown, "not read yet: the server decides");
        assert_eq!(row_unfollow(&r, true, None, Some(std::slice::from_ref(&mine)), Some(me)), RowUnfollow::Shown);
        assert_eq!(row_unfollow(&r, true, None, Some(&[]), Some(me)), RowUnfollow::Hidden, "does not follow");
        assert_eq!(row_unfollow(&r, false, None, None, Some(me)), RowUnfollow::Hidden, "a server without following");
        assert_eq!(row_unfollow(&channel_row("b"), true, None, None, Some(me)), RowUnfollow::Hidden, "a channel has nothing to unfollow");
        let mut ticket = crate::screens::board::tests::card(1, aurelius_domain::ticket::ColumnKind::Review);
        ticket.responsible_user_id = Some(me);
        assert_eq!(row_unfollow(&r, true, Some(&ticket), Some(std::slice::from_ref(&mine)), Some(me)), RowUnfollow::Locked, "B10");
    }
}
