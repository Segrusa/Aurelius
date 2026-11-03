//! The rows per ticket or channel (AUR-FA A3, feature `personal_inbox`; freeze §2 and §4,
//! ui-ontwerp §4): the heading from the row's `reason`, `+n` for what it merged (it opens the
//! ticket's Activity, there is no list of events per row), ◆ on `needs_you`.
//! A reason this client does not know is a plain row with the server's sentence.
//! The rules are pure here; the list itself is in `notifications.rs`.

use crate::app::{AppView, NavItem};
use crate::backend::{self, BoardCall};
use crate::screens::board::TicketTab;
use aurelius_domain::{ConversationId, MessageId, TicketId};
use aurelius_protocol::NotificationView;
use gpui_kit::*;
use std::collections::HashMap;

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

/// Whether a row counts as unread: not read on the server, and not opened here since its last
/// event. `opened`: the rows opened here, with their `created_at_ms` then, so the row does not
/// flicker until the server's answer is in (a new event moves it).
pub fn row_unread(n: &NotificationView, opened: &HashMap<String, i64>) -> bool {
    !n.read && opened.get(&n.id).is_none_or(|at| n.created_at_ms > *at)
}

/// The row to read on the server when it is opened: `notification.mark_read`, never a dismiss, so
/// the row stays listed (freeze §4). Only with the personal Inbox, and only while it is unread.
pub fn read_on_open(n: &NotificationView, supported: bool, opened: &HashMap<String, i64>) -> Option<String> {
    (supported && is_update(n) && row_unread(n, opened)).then(|| n.id.clone())
}

/// The mark in front of a row. ◆ goes first: `needs_you` is worked out on reading, so a row that
/// was read keeps it until the work is done.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowMark {
    NeedsYou,
    Unread,
    Read,
}

/// See [`RowMark`].
pub fn row_mark(n: &NotificationView, unread: bool) -> RowMark {
    match (n.needs_you, unread) {
        (true, _) => RowMark::NeedsYou,
        (false, true) => RowMark::Unread,
        (false, false) => RowMark::Read,
    }
}

impl AppView {
    /// The personal Inbox is on this server (feature `personal_inbox`).
    pub fn personal_inbox_supported(&self) -> bool {
        matches!(&self.connection, backend::Connection::Online { server, .. } if server.personal_inbox)
    }

    /// Read a row: at once here, and for this reader on the server.
    fn read_row(&mut self, n: &NotificationView) {
        if let Some(id) = read_on_open(n, self.personal_inbox_supported(), &self.bell.opened) {
            let _ = self.requests.send(backend::Request::MarkRowRead(id));
        }
        self.bell.opened.insert(n.id.clone(), n.created_at_ms);
    }

    /// Open a row on the right place; that reads it (§4.4).
    pub fn open_update_row(&mut self, n: &NotificationView, window: &mut Window, cx: &mut Context<Self>) {
        self.read_row(n);
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
        self.read_row(n);
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

}

#[cfg(test)]
pub mod tests {
    use super::{RowMark, RowTarget, heading, is_update, more, place, read_on_open, row_mark, row_target, row_unread};
    use crate::screens::board::TicketTab;
    use aurelius_domain::{ConversationId, MessageId, TicketId};
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
    fn merged_events_show_as_plus_n() {
        let mut r = row("a", Some("review"));
        assert_eq!(more(&r), None);
        r.merged_count = 2;
        assert_eq!(more(&r).as_deref(), Some("+2"));
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
    fn opening_reads_a_row_once_and_only_with_the_personal_inbox() {
        let mut r = row("a", Some("mention"));
        let mut opened = HashMap::new();
        assert_eq!(read_on_open(&r, true, &opened).as_deref(), Some("a"));
        assert_eq!(read_on_open(&r, false, &opened), None, "a server without personal_inbox has no such write");
        assert_eq!(read_on_open(&row("b", None), true, &opened), None, "a notification without a reason is not a row");
        opened.insert("a".to_string(), r.created_at_ms);
        assert_eq!(read_on_open(&r, true, &opened), None, "already asked");
        r.created_at_ms += 1;
        assert_eq!(read_on_open(&r, true, &opened).as_deref(), Some("a"), "a new event is unread again");
        r.read = true;
        assert_eq!(read_on_open(&r, true, &HashMap::new()), None, "read on the server");
    }

    #[test]
    fn a_read_row_keeps_its_mark_until_the_work_is_done() {
        let mine = NotificationView { read: true, needs_you: true, ..row("a", Some("review")) };
        assert_eq!(row_mark(&mine, row_unread(&mine, &HashMap::new())), RowMark::NeedsYou);
        let done = NotificationView { needs_you: false, ..mine };
        assert_eq!(row_mark(&done, row_unread(&done, &HashMap::new())), RowMark::Read, "the read after the work is done");
        assert_eq!(row_mark(&row("b", Some("mention")), true), RowMark::Unread);
    }
}
