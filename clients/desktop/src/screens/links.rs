//! Links to a ticket or a message inside the app (AUR-FA A5a, freeze §2, ui-ontwerp §6):
//! `aurelius://<host_id>/ticket/<ticket_id>` and `aurelius://<host_id>/message/<message_id>`.
//! Parsed strictly: scheme, host id, exactly one kind, a valid id; anything else is not a
//! link. A link only navigates, and holds no token. In the UI it stays `#42`.

use crate::app::{AppView, Page, Panel};
use crate::backend::{self, BoardCall};
use crate::screens::board::TicketTab;
use aurelius_domain::error::ErrorCode;
use aurelius_domain::views::MessageLocation;
use aurelius_domain::{HostId, MessageId, TicketId};
use aurelius_protocol::MessageView;
use gpui_kit::*;

const SCHEME: &str = "aurelius://";
/// What a link that does not parse says.
pub const INVALID_LINK: &str = "This isn't a valid Aurelius link.";
/// A link on a server without `message_locate`.
pub const OLD_SERVER: &str = "This server can't open links yet. Update the server.";
/// `message.locate` answered that the message is not there (any more).
pub const MESSAGE_GONE: &str = "That message no longer exists.";
/// A link to a server this device does not know.
pub const UNKNOWN_SERVER: &str = "This link is for a server this device isn't paired with.";
/// A link that waits for the sign-in.
pub const SIGN_IN_FIRST: &str = "Sign in to open the link.";
/// How long a message opened by a link shows its halo.
pub const FLASH_MS: u64 = 1_500;
/// The bounds of the row a link opened (the list scrolls to it).
pub const LINK_TARGET: &str = "link-target-row";
/// Space above the row a link opened.
const LINK_TARGET_MARGIN: f32 = 24.;

/// What a link points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkTarget {
    Ticket(TicketId),
    Message(MessageId),
}

/// A parsed link.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AureliusLink {
    pub host: HostId,
    pub target: LinkTarget,
}

/// Whether a link's address is one of ours, valid or not: it never goes to another app.
pub fn is_aurelius(url: &str) -> bool {
    url.trim_start().to_ascii_lowercase().starts_with(SCHEME)
}

/// The link in `text`, or none: nothing is trimmed, guessed or decoded. An id must be in the
/// exact form this client writes, so one link has one spelling.
pub fn parse_link(text: &str) -> Option<AureliusLink> {
    let rest = text.strip_prefix(SCHEME)?;
    let mut parts = rest.split('/');
    let (host, kind, id) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() {
        return None;
    }
    let host = HostId::parse(host).ok().filter(|h| h.to_string() == host)?;
    let target = match kind {
        "ticket" => LinkTarget::Ticket(TicketId::parse(id).ok().filter(|t| t.to_string() == id)?),
        "message" => LinkTarget::Message(MessageId::parse(id).ok().filter(|m| m.to_string() == id)?),
        _ => return None,
    };
    Some(AureliusLink { host, target })
}

/// A ticket's link.
pub fn ticket_link(host: HostId, ticket: TicketId) -> String {
    format!("{SCHEME}{host}/ticket/{ticket}")
}

/// A message's link.
pub fn message_link(host: HostId, message: MessageId) -> String {
    format!("{SCHEME}{host}/message/{message}")
}

/// Whether a message can be linked to (ui-ontwerp §6.1): not a system line, not while it
/// waits in the queue or still streams, and not a deleted one.
pub fn linkable(m: &MessageView) -> bool {
    m.system.is_none() && !m.queued && m.status != aurelius_domain::conversation::MessageStatus::Partial && m.deleted_at_ms.is_none()
}

/// Message text with its bare links as Markdown links with a readable label (`#42 Title`),
/// so a click opens them and the UI never shows the address. Code and existing links stay.
pub fn link_pills(text: &str, label: impl Fn(&AureliusLink) -> String) -> String {
    let mut out = String::with_capacity(text.len());
    let mut fence = false;
    for line in text.split_inclusive('\n') {
        if line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~") {
            fence = !fence;
            out.push_str(line);
            continue;
        }
        if fence || !line.contains(SCHEME) {
            out.push_str(line);
            continue;
        }
        let mut rest = line;
        let mut code = false;
        while let Some(at) = rest.find(SCHEME) {
            let (before, tail) = rest.split_at(at);
            code ^= before.matches('`').count() % 2 == 1;
            out.push_str(before);
            let end = tail.find(|ch: char| !(ch.is_ascii_alphanumeric() || matches!(ch, '-' | '/' | ':'))).unwrap_or(tail.len());
            let (token, after) = tail.split_at(end);
            match parse_link(token).filter(|_| !code && !out.ends_with("](") && !out.ends_with('<')) {
                Some(link) => out.push_str(&format!("[{}]({token})", label(&link).replace('[', "\\[").replace(']', "\\]"))),
                None => out.push_str(token),
            }
            rest = after;
        }
        out.push_str(rest);
    }
    out
}

/// Where a link goes from the server in use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkRoute {
    /// The server in use (or nobody knows yet which one it is).
    Here,
    /// Another server this device is paired with, by name.
    Known(String),
    /// A server this device does not know.
    Unknown,
}

/// `current`: the id of the server in use; `known`: the servers this device is paired with.
pub fn link_route(link: HostId, current: Option<HostId>, known: &[(HostId, String)]) -> LinkRoute {
    if current.is_none_or(|c| c == link) {
        return LinkRoute::Here;
    }
    known.iter().find(|(id, _)| *id == link).map_or(LinkRoute::Unknown, |(_, name)| LinkRoute::Known(name.clone()))
}

/// The question before a link to another server opens.
pub fn other_server_question(name: &str, current: &str) -> String {
    format!("This link is for {name}. You're on {current}.")
}

/// What a failed `message.locate` says.
pub fn locate_failure(e: &aurelius_client::ClientError) -> String {
    match e {
        aurelius_client::ClientError::Protocol(p) if p.code == ErrorCode::NotFound => MESSAGE_GONE.into(),
        aurelius_client::ClientError::Protocol(p) if p.code == ErrorCode::UnknownMethod => OLD_SERVER.into(),
        aurelius_client::ClientError::Protocol(p) => p.message.clone(),
        e => e.to_string(),
    }
}

/// A link's way to its place; done on the next render, which has the window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PendingLink {
    /// Waits for the connection to the right server and the sign-in. Only in memory.
    Link(AureliusLink),
    /// `message.locate` answered.
    Located(MessageLocation),
}

/// The message a link opened: its halo, and whether the list still has to scroll to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinkFlash {
    pub message: MessageId,
    pub scroll: bool,
}

impl AppView {
    /// `aurelius://` links work on this server (feature `message_locate`).
    pub fn message_locate_supported(&self) -> bool {
        matches!(&self.connection, backend::Connection::Online { server, .. } if server.message_locate)
    }

    /// The id of the server in use.
    fn link_host(&self) -> Option<HostId> {
        match &self.connection {
            backend::Connection::Online { server, .. } => server.host_id,
            _ => None,
        }
    }

    /// Copy a ticket's link; a server without links keeps the text it always had.
    pub fn copy_ticket_link(&mut self, id: TicketId, cx: &mut Context<Self>) {
        let number = self.board.ticket(id).map_or(0, |t| t.number);
        self.copy_ticket_link_numbered(id, number, cx);
    }

    /// The same for a ticket this client may not have read (a row of the Inbox knows its number).
    pub fn copy_ticket_link_numbered(&mut self, id: TicketId, number: u32, cx: &mut Context<Self>) {
        let text = match self.link_host().filter(|_| self.message_locate_supported()) {
            Some(host) => ticket_link(host, id),
            None => format!("#{number} (ticket {id})"),
        };
        self.copy_text(format!("ticket-{id}"), text, cx);
        self.notify_user(format!("Link to #{number} copied."), false, cx);
    }

    /// Copy a message's link (only offered on a server with `message_locate`).
    pub fn copy_message_link(&mut self, id: MessageId, cx: &mut Context<Self>) {
        let Some(host) = self.link_host().filter(|_| self.message_locate_supported()) else {
            self.notify_user("Offline · reconnecting", true, cx);
            return;
        };
        self.copy_text(format!("message-{id}"), message_link(host, id), cx);
        self.notify_user("Link to message copied.", false, cx);
    }

    /// Whether this chat message offers Copy link: only a channel's can be found again
    /// (`message.locate`); a ticket's discussion has its own row.
    pub fn message_link_allowed(&self, m: &MessageView) -> bool {
        self.message_locate_supported() && linkable(m) && self.screen == Page::GroupChat
    }

    /// The servers this device is paired with, by id.
    fn paired_servers(&self) -> Vec<(HostId, String)> {
        let Some(root) = self.data_root.as_ref() else { return Vec::new() };
        aurelius_client::servers::Servers::load(root).servers.into_iter().map(|s| (s.host_id, s.host_name)).collect()
    }

    /// A clicked `aurelius://` link: to its place, or the reason it does not go there.
    pub fn open_link(&mut self, url: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(link) = parse_link(url) else {
            self.notify_user(INVALID_LINK, true, cx);
            return;
        };
        match link_route(link.host, self.link_host(), &self.paired_servers()) {
            LinkRoute::Here => self.link_go(link, window, cx),
            LinkRoute::Known(_) => {
                self.overlay = crate::app::Overlay::Confirm(crate::app::ConfirmKind::LinkOtherServer(link));
                self.overlay_focus.focus(window, cx);
                cx.notify();
            }
            LinkRoute::Unknown => self.notify_user(UNKNOWN_SERVER, true, cx),
        }
    }

    /// "Switch to Thuis" under a link: the link waits, in memory, for that server.
    pub fn link_switch_server(&mut self, link: AureliusLink, cx: &mut Context<Self>) {
        self.pending_link = Some(PendingLink::Link(link));
        self.switch_server(Some(link.host), cx);
    }

    /// The question of a link to another server, and that server's name.
    pub fn link_question(&self, link: &AureliusLink) -> (String, String) {
        let name = self.paired_servers().into_iter().find(|(id, _)| *id == link.host).map_or_else(|| "another server".to_string(), |(_, n)| n);
        let current = match &self.connection {
            backend::Connection::Online { server, .. } => server.host_name.clone(),
            _ => "this server".to_string(),
        };
        (other_server_question(&name, &current), name)
    }

    /// On the right server: open it, or let it wait for the sign-in.
    fn link_go(&mut self, link: AureliusLink, window: &mut Window, cx: &mut Context<Self>) {
        if !self.board_online() || self.active_member().is_none() {
            self.pending_link = Some(PendingLink::Link(link));
            if self.board_online() {
                self.notify_user(SIGN_IN_FIRST, false, cx);
            }
            return;
        }
        match link.target {
            LinkTarget::Ticket(id) => {
                self.go_nav(crate::app::NavItem::Board, window, cx);
                self.board.tab = TicketTab::Ticket;
                self.open_ticket(id, window, cx);
            }
            LinkTarget::Message(_) if !self.message_locate_supported() => self.notify_user(OLD_SERVER, true, cx),
            LinkTarget::Message(id) => {
                let _ = self.requests.send(backend::Request::Board(BoardCall::Locate(id)));
            }
        }
        cx.notify();
    }

    /// `message.locate` answered: its place opens on the next render.
    pub fn link_located(&mut self, result: Result<MessageLocation, String>, cx: &mut Context<Self>) {
        match result {
            Ok(location) => self.pending_link = Some(PendingLink::Located(location)),
            Err(why) => self.notify_user(why, true, cx),
        }
    }

    /// Every render: what waits for the window, the connection or the sign-in.
    pub fn link_step(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.pending_link.clone() {
            Some(PendingLink::Link(link)) if self.board_online() && self.active_member().is_some() && self.link_host() == Some(link.host) => {
                self.pending_link = None;
                self.link_go(link, window, cx);
            }
            Some(PendingLink::Located(place)) => {
                self.pending_link = None;
                self.link_open_place(place, window, cx);
            }
            _ => {}
        }
        self.link_scroll();
    }

    /// The right tab or channel, its thread when the message replies in one, and the halo.
    fn link_open_place(&mut self, place: MessageLocation, window: &mut Window, cx: &mut Context<Self>) {
        self.link_flash = Some(LinkFlash { message: place.message_id, scroll: true });
        match place.ticket_id {
            Some(ticket) => {
                self.go_nav(crate::app::NavItem::Board, window, cx);
                self.open_ticket(ticket, window, cx);
                self.ticket_tab(TicketTab::Discussion, cx);
            }
            None => {
                self.go_to_channel(place.conversation_id, window, cx);
                if let Some(root) = place.thread_root {
                    self.open_thread_panel(root, window, cx);
                }
            }
        }
        let message = place.message_id;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(std::time::Duration::from_millis(FLASH_MS)).await;
            let _ = this.update(cx, |this, cx| {
                if this.link_flash.is_some_and(|f| f.message == message) {
                    this.link_flash = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// The row a link opened, once it is laid out: into view, once.
    fn link_scroll(&mut self) {
        let Some(flash) = self.link_flash.filter(|f| f.scroll) else { return };
        let Some(row) = self.dropdown_bounds.borrow_mut().remove(LINK_TARGET) else { return };
        let list = if matches!(self.panel, Panel::Ticket(_)) && self.board.tab == TicketTab::Discussion { self.board.discussion_scroll.clone() } else { self.scroll.clone() };
        let (bounds, offset) = (list.bounds(), list.offset());
        list.set_offset(point(offset.x, offset.y - (row.top() - bounds.top()) + px(LINK_TARGET_MARGIN)));
        self.link_flash = Some(LinkFlash { scroll: false, ..flash });
    }

    /// Whether this row is the message a link opened.
    pub fn link_flashes(&self, message: MessageId) -> bool {
        self.link_flash.is_some_and(|f| f.message == message)
    }

    /// Message text with its links readable and clickable; `#42` when the board knows the ticket.
    pub fn with_link_pills(&self, text: &str) -> String {
        if !text.contains(SCHEME) {
            return text.to_string();
        }
        let here = self.link_host();
        link_pills(text, |link| match link.target {
            _ if here.is_some_and(|h| h != link.host) => "Link to another server".to_string(),
            LinkTarget::Ticket(id) => self.board.ticket(id).map_or_else(|| "Ticket link".to_string(), |t| format!("#{} {}", t.number, t.title)),
            LinkTarget::Message(_) => "Message link".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    // Not `super::*`: gpui's `test` attribute would shadow the standard one.
    use super::{AureliusLink, LinkRoute, LinkTarget, MESSAGE_GONE, OLD_SERVER, is_aurelius, link_pills, link_route, linkable, locate_failure, message_link, other_server_question, parse_link, ticket_link};
    use aurelius_client::ClientError;
    use aurelius_domain::AppError;
    use aurelius_domain::error::ErrorCode;
    use aurelius_domain::conversation::MessageStatus;
    use aurelius_domain::{HostId, MessageId, TicketId};
    use aurelius_protocol::MessageView;

    fn host() -> HostId {
        HostId::parse("11111111-1111-4111-8111-111111111111").unwrap()
    }

    fn ticket() -> TicketId {
        TicketId::parse("aaaaaaaa-2222-4222-8222-222222222222").unwrap()
    }

    fn message() -> MessageId {
        MessageId::parse("33333333-3333-4333-8333-333333333333").unwrap()
    }

    #[test]
    fn a_link_this_client_writes_parses_back_to_the_same_place() {
        let t = ticket_link(host(), ticket());
        assert_eq!(t, "aurelius://11111111-1111-4111-8111-111111111111/ticket/aaaaaaaa-2222-4222-8222-222222222222");
        assert_eq!(parse_link(&t), Some(AureliusLink { host: host(), target: LinkTarget::Ticket(ticket()) }));
        let m = message_link(host(), message());
        assert_eq!(m, "aurelius://11111111-1111-4111-8111-111111111111/message/33333333-3333-4333-8333-333333333333");
        assert_eq!(parse_link(&m), Some(AureliusLink { host: host(), target: LinkTarget::Message(message()) }));
    }

    #[test]
    fn anything_but_the_exact_form_is_not_a_link() {
        let h = host();
        let t = ticket();
        for bad in [
            String::new(),
            "aurelius://".into(),
            format!("https://{h}/ticket/{t}"),
            format!("AURELIUS://{h}/ticket/{t}"),
            format!(" aurelius://{h}/ticket/{t}"),
            format!("aurelius://{h}/ticket/{t} "),
            format!("aurelius://{h}/ticket/{t}/"),
            format!("aurelius://{h}/ticket/{t}?token=x"),
            format!("aurelius://{h}/ticket/{t}#top"),
            format!("aurelius://{h}/ticket/42"),
            format!("aurelius://{h}/ticket/"),
            format!("aurelius://{h}/ticket"),
            format!("aurelius://{h}/task/{t}"),
            format!("aurelius://{h}/ticket/{t}/message/{}", message()),
            format!("aurelius://server/ticket/{t}"),
            format!("aurelius:///ticket/{t}"),
            format!("aurelius://user@{h}/ticket/{t}"),
            format!("aurelius://{h}/ticket/{}", t.to_string().to_uppercase()),
            format!("aurelius://{h}/ticket/{}", t.to_string().replace('-', "")),
            format!("aurelius://{h}/ticket/{{{t}}}"),
            format!("aurelius://{h}/ticket/%32{}", &t.to_string()[1..]),
        ] {
            assert_eq!(parse_link(&bad), None, "{bad}");
        }
    }

    #[test]
    fn an_invalid_address_of_our_scheme_never_goes_to_another_app() {
        assert!(is_aurelius("aurelius://nonsense"));
        assert!(is_aurelius(" AURELIUS://x"));
        assert!(!is_aurelius("https://example.com"));
    }

    #[test]
    fn a_link_stays_here_on_its_own_server_and_asks_for_another() {
        let (a, b, c) = (HostId::new(), HostId::new(), HostId::new());
        let known = vec![(b, "Thuis".to_string())];
        assert_eq!(link_route(a, Some(a), &known), LinkRoute::Here);
        assert_eq!(link_route(b, Some(a), &known), LinkRoute::Known("Thuis".into()));
        assert_eq!(link_route(c, Some(a), &known), LinkRoute::Unknown);
        assert_eq!(link_route(c, None, &known), LinkRoute::Here, "not connected yet: it waits");
        assert_eq!(other_server_question("Thuis", "Segrusa"), "This link is for Thuis. You're on Segrusa.");
    }

    fn view(status: MessageStatus) -> MessageView {
        MessageView { status, ..crate::screens::chat::streaming_answer(MessageId::new(), aurelius_domain::RunId::new(), None, None) }
    }

    #[test]
    fn only_a_finished_message_of_someone_has_a_link() {
        let done = view(MessageStatus::Complete);
        assert!(linkable(&done));
        assert!(linkable(&view(MessageStatus::Interrupted)), "it stays where it is");
        assert!(!linkable(&view(MessageStatus::Partial)), "still streaming");
        assert!(!linkable(&MessageView { queued: true, ..done.clone() }), "in the queue");
        assert!(!linkable(&MessageView { deleted_at_ms: Some(1), ..done.clone() }), "deleted");
        let line = aurelius_domain::views::SystemLine::new(aurelius_domain::views::SystemLineKind::Moved);
        assert!(!linkable(&MessageView { system: Some(line), ..done }), "a system line");
    }

    #[test]
    fn a_bare_link_becomes_a_readable_link_and_code_stays() {
        let url = ticket_link(host(), ticket());
        let label = |_: &AureliusLink| "#42 Title".to_string();
        assert_eq!(link_pills(&format!("See {url}."), label), format!("See [#42 Title]({url})."));
        assert_eq!(link_pills(&format!("`{url}`"), label), format!("`{url}`"), "inline code");
        assert_eq!(link_pills(&format!("```\n{url}\n```\n"), label), format!("```\n{url}\n```\n"), "a code block");
        assert_eq!(link_pills(&format!("[mine]({url})"), label), format!("[mine]({url})"), "an existing link");
        assert_eq!(link_pills(&format!("<{url}>"), label), format!("<{url}>"), "an autolink");
        assert_eq!(link_pills("aurelius://nonsense stays", label), "aurelius://nonsense stays");
        assert_eq!(link_pills(&format!("{url} and {url}"), label), format!("[#42 Title]({url}) and [#42 Title]({url})"));
        assert_eq!(link_pills(&url, |_| "#1 a[b]".to_string()), format!("[#1 a\\[b\\]]({url})"), "a title cannot break out");
    }

    #[test]
    fn a_failed_locate_says_what_a_person_can_do() {
        let failure = |code, text: &str| locate_failure(&ClientError::Protocol(AppError::new(code, text)));
        assert_eq!(failure(ErrorCode::NotFound, "no such message"), MESSAGE_GONE);
        assert_eq!(failure(ErrorCode::UnknownMethod, "unknown"), OLD_SERVER);
        assert_eq!(failure(ErrorCode::Validation, "Nope"), "Nope");
    }

    #[test]
    fn a_linked_text_survives_the_markdown_path_pass() {
        let url = ticket_link(host(), ticket());
        let pills = link_pills(&format!("See {url} and notes.md"), |_| "#42".to_string());
        assert_eq!(crate::screens::markdown::link_paths(&pills), format!("See [#42]({url}) and [notes.md](<notes.md>)"));
    }
}
