//! Who is on a ticket (AUR-FA A1, A2, ui-ontwerp §1–§3): the Responsible (a board member
//! who follows up and reviews; an employee does the work) and the followers, on the card,
//! in the panel and in New ticket. Shown only when the server has `ticket_responsible` /
//! `ticket_follow`; without them the Board is as before (P-91). The rules are pure here.

use crate::app::{AppView, MenuItems, Panel};
use crate::backend::{self, BoardCall};
use crate::screens::board::{BoardDo, menu_entry};
use crate::theme::{self, c};
use crate::ui::{self, ButtonKind, h, v};
use aurelius_domain::ticket::{self, ColumnKind, FollowerView, TicketView};
use aurelius_domain::user::UserView;
use aurelius_domain::{Command, TicketId, UserId};
use gpui_kit::base::TestSupportExt as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

/// What the Responsible does (§1.1).
pub const RESPONSIBLE_MEANS: &str = "Follows up and reviews. Doesn't do the work.";
/// What the owner does.
pub const OWNER_MEANS: &str = "Does the work.";
/// What Runs on means.
pub const RUNS_ON_MEANS: &str = "Whose Claude subscription the runs use.";
/// What Created by means.
pub const CREATED_BY_MEANS: &str = "Made the ticket. Never changes.";
/// Why a ticket in Done has no picker.
pub const DONE_CLOSED: &str = "Done tickets are closed.";
/// The followers shown as monograms; the rest is `+n`.
pub const FOLLOWERS_SHOWN: usize = 3;
/// ui.json: the Responsible filter of the board, per client (P-77).
pub const PREF_RESPONSIBLE: &str = "board_responsible";

/// One line of the who block: the name truncates first, the tail always stays (§1.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WhoLine {
    pub name: String,
    pub tail: String,
}

/// Who works on it and on whose login: "Forge · on Marie's login" (P-73).
pub fn work_line(t: &TicketView, me: Option<UserId>, login_disabled: bool) -> WhoLine {
    let login = crate::screens::board::login_owner(t, me);
    WhoLine { name: t.owner_name.clone().unwrap_or_else(|| "No owner".into()), tail: format!(" · on {login} login{}", if login_disabled { " (disabled)" } else { "" }) }
}

/// Who follows up: "Alexandra · responsible", always with the word (§1.2).
pub fn responsible_line(t: &TicketView, me: Option<UserId>) -> WhoLine {
    let Some(user) = t.responsible_user_id else { return WhoLine { name: "No one responsible".into(), tail: String::new() } };
    let name = if Some(user) == me { "You".to_string() } else { responsible_name(t) };
    WhoLine { name: if t.responsible_disabled { format!("{name} (disabled)") } else { name }, tail: " · responsible".into() }
}

/// The Responsible's name.
pub fn responsible_name(t: &TicketView) -> String {
    t.responsible_username.clone().unwrap_or_else(|| "A board member".into())
}

/// The who block as one sentence (§1.3): its tooltip and what a screen reader hears.
pub fn who_sentence(t: &TicketView, me: Option<UserId>) -> String {
    let work = t.owner_name.as_ref().map_or_else(|| "No owner yet.".to_string(), |o| format!("{o} does the work."));
    let login = crate::screens::board::login_owner(t, me);
    let responsible = match t.responsible_user_id {
        None => "No one is responsible.".to_string(),
        Some(u) if Some(u) == me => "You are responsible: follows up and reviews.".to_string(),
        Some(_) if t.responsible_disabled => format!("{} (disabled) is responsible.", responsible_name(t)),
        Some(_) => format!("{} is responsible: follows up and reviews.", responsible_name(t)),
    };
    let created = if Some(t.created_by_user) == me { "you".to_string() } else { t.created_by_username.clone() };
    format!("{work} Runs on {login} Claude login. {responsible} Created by {created}.")
}

/// One choice of the Responsible picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponsibleChoice {
    /// None: "No one".
    pub user: Option<UserId>,
    pub label: String,
    pub checked: bool,
}

/// The picker's list (§2.1): you first, then the active board members A–Z, and "No one" only
/// when someone is responsible now. A disabled board member cannot be chosen (P-78).
pub fn responsible_choices(members: &[UserView], me: Option<UserId>, current: Option<UserId>) -> Vec<ResponsibleChoice> {
    let mut others: Vec<&UserView> = members.iter().filter(|u| u.disabled_at_ms.is_none() && Some(u.id) != me).collect();
    others.sort_by_key(|u| u.username.to_lowercase());
    let mut choices: Vec<ResponsibleChoice> = me.into_iter().map(|u| ResponsibleChoice { user: Some(u), label: "You".into(), checked: current == Some(u) }).collect();
    choices.extend(others.into_iter().map(|u| ResponsibleChoice { user: Some(u.id), label: u.username.clone(), checked: current == Some(u.id) }));
    if current.is_some() {
        choices.push(ResponsibleChoice { user: None, label: "No one".into(), checked: false });
    }
    choices
}

/// The toast after a handover. There is no Undo: it can be changed again at any time.
pub fn responsible_toast(number: u32, chosen: Option<UserId>, me: Option<UserId>, name: &str) -> String {
    match chosen {
        None => format!("No one is responsible for #{number} now."),
        Some(u) if Some(u) == me => format!("You are now responsible for #{number}."),
        Some(_) => format!("{name} is now responsible for #{number}."),
    }
}

/// A `responsible` line of the discussion or the Team chat when the server gives no text (§2.1).
pub fn responsible_line_text(by: &str, from: Option<&str>, to: Option<&str>) -> String {
    match (from, to) {
        (Some(from), Some(to)) => format!("{by} changed responsible: {from} → {to}"),
        (None, Some(to)) => format!("{by} made {to} responsible"),
        (Some(from), None) => format!("{by} removed {from} as responsible"),
        (None, None) => format!("{by} changed who is responsible"),
    }
}

/// What disabling a board member says about their open tickets (§2.4); none when they have none.
pub fn disable_note(name: &str, open: usize) -> Option<String> {
    match open {
        0 => None,
        1 => Some(format!("{name} is responsible for 1 open ticket. It keeps that name until someone changes it.")),
        n => Some(format!("{name} is responsible for {n} open tickets. They keep that name until someone changes it.")),
    }
}

/// The Board's filter on the Responsible (P-77).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResponsibleFilter {
    #[default]
    Anyone,
    Me,
    NoOne,
    Member(UserId),
}

impl ResponsibleFilter {
    /// What a card must have: none: anything (also "Me" while nobody is signed in);
    /// `Some(None)`: nobody responsible.
    pub fn wanted(self, me: Option<UserId>) -> Option<Option<UserId>> {
        match self {
            ResponsibleFilter::Anyone => None,
            ResponsibleFilter::Me => me.map(Some),
            ResponsibleFilter::NoOne => Some(None),
            ResponsibleFilter::Member(u) => Some(Some(u)),
        }
    }

    /// How ui.json keeps it.
    pub fn to_pref(self) -> String {
        match self {
            ResponsibleFilter::Anyone => "anyone".into(),
            ResponsibleFilter::Me => "me".into(),
            ResponsibleFilter::NoOne => "no_one".into(),
            ResponsibleFilter::Member(u) => u.to_string(),
        }
    }

    /// Back from ui.json; anything else is Anyone.
    pub fn from_pref(text: &str) -> Self {
        match text {
            "me" => ResponsibleFilter::Me,
            "no_one" => ResponsibleFilter::NoOne,
            other => UserId::parse(other).map_or(ResponsibleFilter::Anyone, ResponsibleFilter::Member),
        }
    }
}

/// Whether a card passes the filter (`wanted` of [`ResponsibleFilter`]).
pub fn responsible_matches(t: &TicketView, wanted: Option<Option<UserId>>) -> bool {
    wanted.is_none_or(|w| t.responsible_user_id == w)
}

/// New ticket's Responsible (P-76): You unless chosen otherwise.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FormResponsible {
    #[default]
    You,
    NoOne,
    Member(UserId),
}

/// What `ticket.create` says: left out on a server without the feature (it refuses a field it
/// does not know), and for No one.
pub fn create_responsible(choice: FormResponsible, me: Option<UserId>, supported: bool) -> Option<UserId> {
    if !supported {
        return None;
    }
    match choice {
        FormResponsible::You => me,
        FormResponsible::NoOne => None,
        FormResponsible::Member(u) => Some(u),
    }
}

/// The reader's follow state on a ticket.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FollowState {
    pub following: bool,
    /// The Responsible cannot stop following: hand it over first (B10).
    pub locked: bool,
}

/// The Responsible always follows, also before the list is read. `followers`: none until read.
pub fn follow_state(t: &TicketView, followers: Option<&[FollowerView]>, me: Option<UserId>) -> FollowState {
    let locked = me.is_some() && t.responsible_user_id == me;
    let listed = followers.is_some_and(|list| list.iter().any(|f| Some(f.user_id) == me));
    FollowState { following: locked || listed, locked }
}

/// Why a board member follows, in a phrase (P-79); a reason this client does not know yet says nothing more.
pub fn follow_reason(reason: &str) -> Option<&'static str> {
    match reason {
        "created" => Some("you created it"),
        "responsible" => Some("you're responsible"),
        "mentioned" => Some("you were @mentioned"),
        "manual" => Some("you chose to follow"),
        _ => None,
    }
}

/// The same reason in a list of everyone who follows.
pub fn follower_reason_text(f: &FollowerView) -> &'static str {
    match f.reason.as_str() {
        "created" => "Created it",
        "responsible" => "Responsible",
        "mentioned" => "Was @mentioned",
        "manual" => "Chose to follow",
        _ => "Follows",
    }
}

/// The Follow button's tooltip (§3.1, §3.3).
pub fn follow_tip(state: FollowState, reason: Option<&str>, number: u32, column: ColumnKind) -> String {
    let main = if state.locked {
        format!("You're responsible for #{number}. Hand it to someone else to stop following.")
    } else if state.following {
        match reason.and_then(follow_reason) {
            Some(why) => format!("You follow because {why}."),
            None => "You follow this ticket.".to_string(),
        }
    } else {
        "Get updates on this ticket.".to_string()
    };
    if column == ColumnKind::Done { format!("{main} Done tickets send no updates.") } else { main }
}

/// What a Follow or an Unfollow says (§3.3).
pub fn follow_toast(number: u32, on: bool) -> String {
    if on { format!("You'll get updates on #{number}.") } else { format!("Unfollowed #{number}. A new @mention or becoming responsible makes you follow again.") }
}

/// A follower's name in the list.
pub fn follower_name(f: &FollowerView) -> String {
    if f.disabled { format!("{} (disabled)", f.username) } else { f.username.clone() }
}

/// The monograms a list of followers shows, and how many more there are.
pub fn followers_shown(list: &[FollowerView]) -> (&[FollowerView], usize) {
    let shown = list.len().min(FOLLOWERS_SHOWN);
    (&list[..shown], list.len() - shown)
}

/// Whether the Responsible can change on this ticket (not on Done, US-A1).
pub fn responsible_editable(t: &TicketView) -> bool {
    t.column != ColumnKind::Done
}

/// A monogram of a board member, dimmed when disabled; a ring for nobody.
fn responsible_mark(t: &TicketView, size: f32, cx: &App) -> AnyElement {
    match t.responsible_user_id {
        None => div().flex_none().size(px(size)).rounded_full().border_1().border_color(c(theme::TEXT_2)).into_any_element(),
        Some(_) => div().flex_none().when(t.responsible_disabled, |el| el.opacity(0.5)).child(ui::member_monogram(&responsible_name(t), size, cx)).into_any_element(),
    }
}

/// A who line: the name truncates first, the tail stays.
fn line_row(mark: Option<AnyElement>, line: WhoLine) -> Div {
    h().gap(px(6.)).min_w_0().overflow_hidden().children(mark).child(div().min_w_0().truncate().child(line.name)).when(!line.tail.is_empty(), |el| el.child(div().id("who-tail").test_support().flex_none().child(line.tail)))
}

/// The card's who block (§1.3): who works on whose login, who follows up, and the deadline.
pub fn card_who(t: &TicketView, me: Option<UserId>, login_disabled: bool, key: usize, deadline: Option<(String, u32)>, cx: &App) -> AnyElement {
    let work = line_row(t.owner_name.as_ref().map(|n| ui::monogram(n, 16., cx).into_any_element()), work_line(t, me, login_disabled));
    let follow_up = line_row(Some(responsible_mark(t, 16., cx)), responsible_line(t, me));
    v().id(("card-who", key)).w_full().min_w_0().gap(px(3.)).text_size(px(11.)).text_color(c(theme::TEXT_2)).tooltip(ui::tip(who_sentence(t, me), None))
        .child(work)
        .child(h().w_full().gap(px(6.)).child(follow_up.flex_1()).children(deadline.map(|(text, color)| div().flex_none().text_color(c(color)).child(text))))
        .into_any_element()
}

impl AppView {
    /// Tickets have a Responsible on this server (feature `ticket_responsible`).
    pub fn ticket_responsible_supported(&self) -> bool {
        matches!(&self.connection, backend::Connection::Online { server, .. } if server.ticket_responsible)
    }

    /// Tickets can be followed on this server (feature `ticket_follow`).
    pub fn ticket_follow_supported(&self) -> bool {
        matches!(&self.connection, backend::Connection::Online { server, .. } if server.ticket_follow)
    }

    /// The filter as it counts now: nothing on a server without Responsible.
    pub fn board_responsible_wanted(&self) -> Option<Option<UserId>> {
        if !self.ticket_responsible_supported() {
            return None;
        }
        self.board.responsible.wanted(self.active_member().map(|u| u.id))
    }

    /// Choose the Responsible: at once and without a version: a handover always goes.
    pub fn ticket_set_responsible(&mut self, id: TicketId, chosen: Option<UserId>, cx: &mut Context<Self>) {
        let number = self.board.ticket(id).map_or(0, |t| t.number);
        let me = self.active_member().map(|u| u.id);
        let name = chosen.map(|u| self.member_name(u)).unwrap_or_default();
        self.board_send(Command::SetTicketResponsible(ticket::SetTicketResponsible { ticket_id: id, responsible_user_id: chosen }), Some(responsible_toast(number, chosen, me, &name)));
        cx.notify();
    }

    /// Follow or unfollow (`ticket.follow`).
    pub fn ticket_follow(&mut self, id: TicketId, on: bool, cx: &mut Context<Self>) {
        let number = self.board.ticket(id).map_or(0, |t| t.number);
        self.ticket_follow_numbered(id, number, on, cx);
    }

    /// The same for a ticket this client may not have read (a row of the Inbox knows its number).
    pub fn ticket_follow_numbered(&mut self, id: TicketId, number: u32, on: bool, cx: &mut Context<Self>) {
        self.board_send(Command::FollowTicket(ticket::FollowTicket { ticket_id: id, on }), Some(follow_toast(number, on)));
        cx.notify();
    }

    /// The reader's state on this ticket, by what was read of its followers.
    pub fn ticket_follow_state(&self, t: &TicketView) -> FollowState {
        let list = self.board.followers.get(&t.id).and_then(|r| r.as_ref().ok()).map(Vec::as_slice);
        follow_state(t, list, self.active_member().map(|u| u.id))
    }

    /// Ask who follows this ticket (only a server with the feature knows).
    pub fn ticket_read_followers(&self, id: TicketId) {
        if self.ticket_follow_supported() {
            let _ = self.requests.send(backend::Request::Board(BoardCall::Followers(id)));
        }
    }

    /// The state of a card the keyboard or a menu is on: read once, so `f` and the menu know it.
    pub fn ticket_know_followers(&self, id: TicketId) {
        if !self.board.followers.contains_key(&id) {
            self.ticket_read_followers(id);
        }
    }

    /// Someone started or stopped following: what was read is old; the open ticket reads again.
    pub fn followers_changed(&mut self) {
        let open = match self.panel {
            Panel::Ticket(id) => Some(id),
            _ => None,
        };
        self.board.followers.retain(|id, _| Some(*id) == open);
        self.inbox_followers_asked = None;
        if let Some(id) = open {
            self.ticket_read_followers(id);
        }
    }

    /// The Responsible picker on a ticket.
    pub fn responsible_menu(&self, t: &TicketView) -> MenuItems {
        let me = self.active_member().map(|u| u.id);
        responsible_choices(&self.members.list, me, t.responsible_user_id).into_iter().map(|choice| menu_entry(choice.label, BoardDo::Responsible(t.id, choice.user), choice.checked, None)).collect()
    }

    /// The same list in New ticket: there "No one" is always there, You is the start (P-76).
    pub fn form_responsible_menu(&self) -> MenuItems {
        let form = self.board.form.responsible;
        let me = self.active_member().map(|u| u.id);
        let mut items = vec![menu_entry("You", BoardDo::FormResponsible(FormResponsible::You), form == FormResponsible::You, None)];
        items.extend(responsible_choices(&self.members.list, me, None).into_iter().filter(|c| c.user != me).filter_map(|c| c.user.map(|u| (u, c.label))).map(|(u, label)| menu_entry(label, BoardDo::FormResponsible(FormResponsible::Member(u)), form == FormResponsible::Member(u), None)));
        items.push(None);
        items.push(menu_entry("No one", BoardDo::FormResponsible(FormResponsible::NoOne), form == FormResponsible::NoOne, None));
        items
    }

    /// New ticket's field, as text.
    pub fn form_responsible_text(&self) -> String {
        match self.board.form.responsible {
            FormResponsible::You => "You".into(),
            FormResponsible::NoOne => "No one".into(),
            FormResponsible::Member(u) => self.member_name(u),
        }
    }

    /// The Board's filter: Anyone, Me, No one, the active board members A–Z and, under them,
    /// disabled board members who still have tickets (P-77).
    pub fn responsible_filter_menu(&self) -> MenuItems {
        let current = self.board.responsible;
        let me = self.active_member().map(|u| u.id);
        let entry = |label: String, f: ResponsibleFilter, detail: Option<String>| menu_entry(label, BoardDo::FilterResponsible(f), current == f, detail);
        let mut items = vec![entry("Anyone".into(), ResponsibleFilter::Anyone, None), entry("Me".into(), ResponsibleFilter::Me, None), entry("No one".into(), ResponsibleFilter::NoOne, None), None];
        items.extend(responsible_choices(&self.members.list, me, None).into_iter().filter(|c| c.user != me).filter_map(|c| c.user.map(|u| entry(c.label, ResponsibleFilter::Member(u), None))));
        let tickets: Vec<&TicketView> = self.board.view.iter().flat_map(|v| v.columns.iter()).flat_map(|c| c.tickets.iter()).collect();
        let mut disabled: Vec<&UserView> = self.members.list.iter().filter(|u| u.disabled_at_ms.is_some() && tickets.iter().any(|t| t.responsible_user_id == Some(u.id))).collect();
        disabled.sort_by_key(|u| u.username.to_lowercase());
        if !disabled.is_empty() {
            items.push(None);
            items.extend(disabled.into_iter().map(|u| entry(u.username.clone(), ResponsibleFilter::Member(u.id), Some("Disabled".into()))));
        }
        items
    }

    /// The filter's button text.
    pub fn responsible_filter_text(&self) -> String {
        match self.board.responsible {
            ResponsibleFilter::Anyone => "Responsible: Anyone".into(),
            ResponsibleFilter::Me => "Responsible: Me".into(),
            ResponsibleFilter::NoOne => "Responsible: No one".into(),
            ResponsibleFilter::Member(u) => format!("Responsible: {}", self.member_name(u)),
        }
    }

    /// A menu's "Set responsible…", at the mouse or the keyboard's card.
    pub fn open_responsible_menu(&mut self, id: TicketId, at: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(t) = self.board.ticket(id).cloned() else { return };
        if !responsible_editable(&t) {
            self.notify_user(DONE_CLOSED, false, cx);
            return;
        }
        let items = self.responsible_menu(&t);
        self.open_menu(at, items, window, cx);
    }

    /// The card's who block with what this window knows.
    pub fn render_card_who(&self, t: &TicketView, key: usize, deadline: Option<(String, u32)>, cx: &mut Context<Self>) -> AnyElement {
        card_who(t, self.active_member().map(|u| u.id), self.login_disabled(t), key, deadline, cx)
    }

    /// The panel's who strip on every tab (P-75), with Follow on the right (§3.1).
    pub fn render_who_strip(&self, t: &TicketView, can: bool, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (responsible, follow) = (self.ticket_responsible_supported(), self.ticket_follow_supported());
        if !responsible && !follow {
            return None;
        }
        let me = self.active_member().map(|u| u.id);
        let lines = v().flex_1().min_w_0().gap(px(3.)).text_size(px(12.)).text_color(c(theme::TEXT_2)).when(responsible, |el| {
            el.child(line_row(t.owner_name.as_ref().map(|n| ui::monogram(n, 18., cx).into_any_element()), work_line(t, me, self.login_disabled(t))))
                .child(line_row(Some(responsible_mark(t, 18., cx)), responsible_line(t, me)))
        });
        Some(h().gap(px(8.)).items_start().child(lines).when(follow, |el| el.child(self.render_follow_button(t, can, cx))).into_any_element())
    }

    /// `[ Follow ]` / `[ ✓ Following ]`, Unfollow on hover; off for the Responsible (B10).
    fn render_follow_button(&self, t: &TicketView, can: bool, cx: &mut Context<Self>) -> AnyElement {
        let id = t.id;
        let state = self.ticket_follow_state(t);
        let me = self.active_member().map(|u| u.id);
        let reason = self.board.followers.get(&id).and_then(|r| r.as_ref().ok()).and_then(|list| list.iter().find(|f| Some(f.user_id) == me)).map(|f| f.reason.clone());
        let tip = if can { follow_tip(state, reason.as_deref(), t.number, t.column) } else { "Offline · reconnecting".to_string() };
        let enabled = can && !state.locked;
        let label = match (state.following, self.board.follow_hover && enabled) {
            (true, true) => "Unfollow",
            (true, false) => "✓ Following",
            (false, _) => "Follow",
        };
        let on = !state.following;
        let button = ui::button("ticket-follow", label, ButtonKind::Outline, None).h(px(26.)).flex_none().tooltip(ui::tip(tip, None))
            .when(state.following, |el| el.text_color(c(theme::ACCENT_TEXT)))
            .on_hover(cx.listener(|this, on: &bool, _, cx| {
                this.board.follow_hover = *on;
                cx.notify();
            }));
        if enabled { button.on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.ticket_follow(id, on, cx))).into_any_element() } else { button.opacity(0.5).cursor_default().into_any_element() }
    }

    /// The Responsible's value in WHO: its picker, Make me responsible, or read-only on Done.
    pub fn render_responsible_value(&self, t: &TicketView, picker: AnyElement, can: bool, cx: &mut Context<Self>) -> AnyElement {
        let id = t.id;
        let me = self.active_member().map(|u| u.id);
        let disabled_note = t.responsible_disabled.then(|| format!("{} is disabled and gets no updates. Choose someone else.", responsible_name(t)));
        v().gap(px(2.))
            .child(
                h().gap(px(8.)).child(picker).when(t.responsible_user_id.is_none() && can && responsible_editable(t) && me.is_some(), |el| {
                    el.child(div().id("ticket-make-me-responsible").role(Role::Link).cursor_pointer().text_size(px(12.)).text_color(c(theme::ACCENT_TEXT)).child("Make me responsible").on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        let me = this.active_member().map(|u| u.id);
                        this.ticket_set_responsible(id, me, cx);
                    })))
                }),
            )
            .child(div().text_size(px(11.5)).text_color(c(theme::TEXT_2)).child(if responsible_editable(t) { RESPONSIBLE_MEANS } else { DONE_CLOSED }))
            .children(disabled_note.map(|n| div().text_size(px(11.5)).text_color(c(theme::TEXT_2)).child(n)))
            .into_any_element()
    }

    /// The Followers row of WHO (§3.1): monograms and `+n`; a click shows names and reasons.
    pub fn render_followers_value(&self, t: &TicketView, cx: &mut Context<Self>) -> AnyElement {
        let id = t.id;
        let note = |text: String| div().text_size(px(12.)).text_color(c(theme::TEXT_2)).child(text);
        let list = match self.board.followers.get(&id) {
            None => return note("Loading…".into()).into_any_element(),
            Some(Err(why)) => {
                return h().gap(px(8.)).child(note(why.clone())).child(div().id("ticket-followers-retry").role(Role::Button).cursor_pointer().text_size(px(12.)).text_color(c(theme::ACCENT_TEXT)).child("Try again").on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.ticket_read_followers(id);
                    cx.notify();
                }))).into_any_element();
            }
            Some(Ok(list)) => list,
        };
        if list.is_empty() {
            return note("No one follows this ticket.".into()).into_any_element();
        }
        let (shown, more) = followers_shown(list);
        let open = self.board.followers_open;
        v().gap(px(4.))
            .child(
                h().id("ticket-followers").role(Role::Button).aria_label(format!("{} followers", list.len())).gap(px(4.)).cursor_pointer()
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.board.followers_open = !this.board.followers_open;
                        cx.notify();
                    }))
                    .children(shown.iter().map(|f| div().when(f.disabled, |el| el.opacity(0.5)).child(ui::member_monogram(&f.username, 18., cx))))
                    .when(more > 0, |el| el.child(div().text_size(px(11.5)).text_color(c(theme::TEXT_2)).child(format!("+{more}")))),
            )
            .when(open, |el| {
                el.children(list.iter().map(|f| {
                    h().gap(px(6.)).text_size(px(12.)).child(div().min_w_0().truncate().child(follower_name(f))).child(div().flex_none().text_color(c(theme::TEXT_2)).child(format!("· {}", follower_reason_text(f))))
                }))
            })
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    // Not `super::*`: gpui's `test` attribute would shadow the standard one.
    use super::{FOLLOWERS_SHOWN, FollowState, FormResponsible, ResponsibleFilter, WhoLine, create_responsible, disable_note, follow_state, follow_tip, follow_toast, follower_name, follower_reason_text, followers_shown, responsible_choices, responsible_editable, responsible_line, responsible_line_text, responsible_matches, responsible_toast, who_sentence, work_line};
    use crate::screens::board::tests::card;
    use gpui_kit::base::TestSupportExt as _;
    use gpui_kit::test::TestWindowExt as _;
    use gpui_kit::{AppContext as _, Bounds, Context, InteractiveElement as _, IntoElement, ParentElement as _, Point, Render, Styled as _, TestAppContext, Window, WindowBounds, WindowOptions, div, px, size};
    use aurelius_domain::UserId;
    use aurelius_domain::ticket::{ColumnKind, FollowerView, TicketView};
    use aurelius_domain::user::UserView;

    fn user(name: &str, disabled: bool) -> UserView {
        UserView { id: UserId::new(), username: name.into(), created_at_ms: 0, created_by_user: None, disabled_at_ms: disabled.then_some(1) }
    }

    fn follower(u: UserId, reason: &str) -> FollowerView {
        FollowerView { user_id: u, username: "ada".into(), disabled: false, reason: reason.into(), followed_at_ms: 1 }
    }

    /// #42: Forge works on Marie's login, Alexandra is responsible, Bram made it.
    fn ticket() -> TicketView {
        let mut t = card(42, ColumnKind::Doing);
        t.owner_name = Some("Forge".into());
        t.login_username = "Marie".into();
        t.created_by_username = "Bram".into();
        t.responsible_user_id = Some(UserId::new());
        t.responsible_username = Some("Alexandra".into());
        t
    }

    #[test]
    fn the_card_says_who_works_on_whose_login_and_who_follows_up() {
        let t = ticket();
        let robin = Some(UserId::new());
        assert_eq!(work_line(&t, robin, false), WhoLine { name: "Forge".into(), tail: " · on Marie's login".into() });
        assert_eq!(work_line(&t, Some(t.login_user), false).tail, " · on your login");
        assert_eq!(work_line(&t, robin, true).tail, " · on Marie's login (disabled)");
        assert_eq!(work_line(&TicketView { owner_name: None, ..t.clone() }, robin, false).name, "No owner");
        assert_eq!(responsible_line(&t, robin), WhoLine { name: "Alexandra".into(), tail: " · responsible".into() });
        assert_eq!(responsible_line(&t, t.responsible_user_id).name, "You");
        assert_eq!(responsible_line(&TicketView { responsible_disabled: true, ..t.clone() }, robin).name, "Alexandra (disabled)");
        assert_eq!(responsible_line(&TicketView { responsible_user_id: None, responsible_username: None, ..t }, robin), WhoLine { name: "No one responsible".into(), tail: String::new() });
    }

    #[test]
    fn the_creator_left_the_card_but_stays_in_the_tooltip() {
        let t = ticket();
        let robin = Some(UserId::new());
        assert_eq!(who_sentence(&t, robin), "Forge does the work. Runs on Marie's Claude login. Alexandra is responsible: follows up and reviews. Created by Bram.");
        assert_eq!(who_sentence(&TicketView { responsible_user_id: None, owner_name: None, ..t.clone() }, robin), "No owner yet. Runs on Marie's Claude login. No one is responsible. Created by Bram.");
        assert!(who_sentence(&t, t.responsible_user_id).contains("You are responsible"));
        assert!(who_sentence(&t, Some(t.created_by_user)).ends_with("Created by you."));
    }

    #[test]
    fn the_picker_lists_you_first_then_active_members_and_no_one_only_when_someone_is_set() {
        let (me, zed, ada, gone) = (user("me", false), user("Zed", false), user("ada", false), user("Gone", true));
        let members = [zed.clone(), gone.clone(), me.clone(), ada.clone()];
        let labels = |current| responsible_choices(&members, Some(me.id), current).into_iter().map(|c| c.label).collect::<Vec<_>>();
        assert_eq!(labels(None), ["You", "ada", "Zed"], "a disabled board member cannot be chosen");
        assert_eq!(labels(Some(zed.id)), ["You", "ada", "Zed", "No one"]);
        let chosen = responsible_choices(&members, Some(me.id), Some(ada.id));
        assert!(chosen.iter().find(|c| c.user == Some(ada.id)).unwrap().checked);
        assert_eq!(chosen.iter().filter(|c| c.checked).count(), 1);
        assert_eq!(chosen.last().unwrap().user, None);
        assert_eq!(responsible_choices(&members, None, None).len(), 3, "nobody signed in: no You");
    }

    #[test]
    fn a_handover_says_who_it_went_to() {
        let me = UserId::new();
        assert_eq!(responsible_toast(42, Some(UserId::new()), Some(me), "Alexandra"), "Alexandra is now responsible for #42.");
        assert_eq!(responsible_toast(42, Some(me), Some(me), ""), "You are now responsible for #42.");
        assert_eq!(responsible_toast(42, None, Some(me), ""), "No one is responsible for #42 now.");
        assert_eq!(responsible_line_text("Marie", None, Some("Alexandra")), "Marie made Alexandra responsible");
        assert_eq!(responsible_line_text("Marie", Some("Alexandra"), Some("Bram")), "Marie changed responsible: Alexandra → Bram");
        assert_eq!(responsible_line_text("Marie", Some("Alexandra"), None), "Marie removed Alexandra as responsible");
    }

    #[test]
    fn the_responsible_filter_finds_me_no_one_and_a_member_and_is_remembered() {
        let t = ticket();
        let who = t.responsible_user_id.unwrap();
        let other = UserId::new();
        assert!(responsible_matches(&t, ResponsibleFilter::Anyone.wanted(Some(other))));
        assert!(responsible_matches(&t, ResponsibleFilter::Me.wanted(Some(who))));
        assert!(!responsible_matches(&t, ResponsibleFilter::Me.wanted(Some(other))));
        assert!(responsible_matches(&t, ResponsibleFilter::Me.wanted(None)), "nobody signed in: no filter");
        assert!(!responsible_matches(&t, ResponsibleFilter::NoOne.wanted(None)));
        assert!(responsible_matches(&TicketView { responsible_user_id: None, ..t.clone() }, ResponsibleFilter::NoOne.wanted(None)));
        assert!(responsible_matches(&t, ResponsibleFilter::Member(who).wanted(None)));
        assert!(!responsible_matches(&t, ResponsibleFilter::Member(other).wanted(None)));
        for f in [ResponsibleFilter::Anyone, ResponsibleFilter::Me, ResponsibleFilter::NoOne, ResponsibleFilter::Member(who)] {
            assert_eq!(ResponsibleFilter::from_pref(&f.to_pref()), f);
        }
        assert_eq!(ResponsibleFilter::from_pref("nonsense"), ResponsibleFilter::Anyone);
    }

    #[test]
    fn new_ticket_starts_on_you_and_an_older_server_never_hears_of_it() {
        let (me, other) = (UserId::new(), UserId::new());
        assert_eq!(FormResponsible::default(), FormResponsible::You);
        assert_eq!(create_responsible(FormResponsible::You, Some(me), true), Some(me));
        assert_eq!(create_responsible(FormResponsible::NoOne, Some(me), true), None);
        assert_eq!(create_responsible(FormResponsible::Member(other), Some(me), true), Some(other));
        assert_eq!(create_responsible(FormResponsible::You, None, true), None, "nobody signed in");
        for choice in [FormResponsible::You, FormResponsible::Member(other)] {
            assert_eq!(create_responsible(choice, Some(me), false), None, "the field stays out for an older server");
        }
    }

    #[test]
    fn the_responsible_cannot_unfollow_and_others_follow_by_the_list() {
        let t = ticket();
        let who = t.responsible_user_id;
        let other = UserId::new();
        assert_eq!(follow_state(&t, None, who), FollowState { following: true, locked: true }, "also before the list is read");
        assert_eq!(follow_state(&t, Some(&[]), Some(other)), FollowState { following: false, locked: false });
        assert_eq!(follow_state(&t, Some(&[follower(other, "manual")]), Some(other)), FollowState { following: true, locked: false });
        assert_eq!(follow_state(&t, None, None), FollowState { following: false, locked: false });
        // After a handover they still follow, and can stop.
        let handed = TicketView { responsible_user_id: Some(other), ..t.clone() };
        assert_eq!(follow_state(&handed, Some(&[follower(who.unwrap(), "responsible")]), who), FollowState { following: true, locked: false });
    }

    #[test]
    fn the_follow_button_says_why_you_follow() {
        let on = FollowState { following: true, locked: false };
        assert_eq!(follow_tip(on, Some("mentioned"), 42, ColumnKind::Doing), "You follow because you were @mentioned.");
        assert_eq!(follow_tip(on, Some("created"), 42, ColumnKind::Doing), "You follow because you created it.");
        assert_eq!(follow_tip(on, Some("manual"), 42, ColumnKind::Doing), "You follow because you chose to follow.");
        assert_eq!(follow_tip(on, Some("something new"), 42, ColumnKind::Doing), "You follow this ticket.", "a new reason breaks nothing");
        assert_eq!(follow_tip(FollowState { following: true, locked: true }, None, 42, ColumnKind::Doing), "You're responsible for #42. Hand it to someone else to stop following.");
        assert_eq!(follow_tip(FollowState { following: false, locked: false }, None, 42, ColumnKind::Done), "Get updates on this ticket. Done tickets send no updates.");
        assert_eq!(follow_toast(42, true), "You'll get updates on #42.");
        assert_eq!(follow_toast(42, false), "Unfollowed #42. A new @mention or becoming responsible makes you follow again.");
    }

    #[test]
    fn three_followers_show_and_the_rest_counts() {
        let list: Vec<FollowerView> = (0..5).map(|_| follower(UserId::new(), "manual")).collect();
        assert_eq!(followers_shown(&list).0.len(), FOLLOWERS_SHOWN);
        assert_eq!(followers_shown(&list).1, 2);
        assert_eq!(followers_shown(&list[..2]), (&list[..2], 0));
        assert_eq!(follower_name(&FollowerView { disabled: true, ..list[0].clone() }), "ada (disabled)");
        assert_eq!(follower_reason_text(&follower(UserId::new(), "new reason")), "Follows");
    }

    #[test]
    fn disabling_someone_says_their_open_tickets_keep_their_name() {
        assert_eq!(disable_note("Alexandra", 0), None);
        assert_eq!(disable_note("Alexandra", 1).as_deref(), Some("Alexandra is responsible for 1 open ticket. It keeps that name until someone changes it."));
        assert_eq!(disable_note("Alexandra", 3).as_deref(), Some("Alexandra is responsible for 3 open tickets. They keep that name until someone changes it."));
    }

    struct Card(TicketView);

    impl Render for Card {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            div().size_full().child(div().id("the-card").test_support().w(px(CARD_INNER)).child(super::card_who(&self.0, None, false, 0, Some(("Nov 3".into(), 0)), cx)))
        }
    }

    /// The narrowest card's inside: 232 px wide, less its padding.
    const CARD_INNER: f32 = 212.;

    /// §1.3: long names truncate first; "· on … login" and "· responsible" stay inside the card.
    #[gpui_kit::test]
    fn long_names_truncate_and_the_rest_of_the_line_stays_on_the_card(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        cx.update(|cx| cx.set_global(crate::theme::Fonts::resolve(cx)));
        let mut t = ticket();
        t.owner_name = Some("Forge the very long named backend developer".into());
        t.login_username = "Marie".into();
        t.responsible_username = Some("Alexandra with a really long family name".into());
        let (window, _) = cx.update(|cx| {
            let bounds = Bounds { origin: Point::default(), size: size(px(500.), px(300.)) };
            gpui_kit::open_window(WindowOptions { window_bounds: Some(WindowBounds::Windowed(bounds)), ..Default::default() }, cx, |_, cx| cx.new(|_| Card(t))).unwrap_or_else(|e| panic!("window: {e}"))
        });
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            let card = window.find("the-card").bounds();
            let tail = window.find("who-tail").bounds();
            assert!(tail.size.width > px(0.), "{tail:?}");
            assert!(tail.right() <= card.right(), "the tail stays on the card: {tail:?} in {card:?}");
        })
        .unwrap_or_else(|e| panic!("update: {e}"));
    }

    #[test]
    fn a_ticket_on_done_keeps_its_responsible() {
        assert!(responsible_editable(&card(1, ColumnKind::Review)));
        assert!(!responsible_editable(&card(1, ColumnKind::Done)));
    }
}
