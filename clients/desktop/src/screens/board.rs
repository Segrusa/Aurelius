//! The Board (protocol 4; ui-ontwerp §3, §5, §6): one board per company with
//! six columns, filtered per project (B4). A card opens the ticket in the right
//! panel (`screens/ticket.rs`); a drag, "Move to…" and the buttons follow the
//! same rules (§6), decided here by `drop_effect`.
//!
//! Employees move cards by themselves (the engine): those moves come in live
//! (`BoardUpdate::Changed`). "Needs you" has no category on the server: a card
//! needs a board member in Review and Plan review, when its main task failed
//! or waits for an approval, and in Todo when nobody chooses its owner (B5:
//! no dispatcher, or the server said the dispatcher chose nobody). As on
//! mobile, that last one is read from the ticket's timeline (`dispatcher_gave_up`).

use crate::app::{AppView, MenuDo, MenuEntry, MenuItems, Overlay, Panel};
use crate::backend::{self, BoardCall, BoardUpdate};
use crate::theme::{self, c};
use crate::ui::{self, ButtonKind, h, v};
use aurelius_domain::ticket::{BoardColumnCards, BoardView, ColumnKind, TicketDetail, TicketNeed, TicketPriority, TicketView};
use aurelius_domain::{Command, ConversationId, EmployeeId, ProjectId, TicketId, UserId, ticket};
use aurelius_protocol::TimelineCard;
use crate::screens::who::{FormResponsible, PREF_RESPONSIBLE, ResponsibleFilter};
use gpui_kit::component::input::{InputEvent, InputState, TextareaState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::collections::{HashMap, HashSet};

/// Done shows its newest tickets; "Show older" the rest.
pub const DONE_SHOWN: usize = 20;
/// The colour of High (and of a deadline within a day).
pub const HIGH: u32 = 0xD99A3A;
/// How much of a Todo ticket's timeline says whether its dispatcher gave up.
pub const GAVE_UP_LIMIT: u32 = 20;
/// The bounds of the card the keyboard is on ("Move to…" opens there).
const FOCUSED_CARD: &str = "board-focused-card";
/// ui.json: the project filter of the board, per client (§3.2).
const PREF_PROJECT: &str = "board_project";

// ---- Rules ------------------------------------------------------------------

/// What a drop (or "Move to…") of a card from one column on another does for a
/// board member (§6). The server decides again; this keeps the client honest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Drop {
    /// Within its column: only the order.
    Reorder,
    /// Backlog → Todo: Start, no dialog.
    Start,
    /// To Backlog: stops the work (`ticket.cancel`). `confirm`: ask first.
    BackToBacklog { confirm: bool },
    /// Plan review → Backlog: "Afwijzen", asked first.
    Reject,
    /// Plan review → Doing: the plan with its buttons.
    PlanDialog,
    /// Review → Doing: "Send back to Doing" with its required feedback (B2).
    SendBack,
    /// Review → Done: "Mark as Done?".
    MarkDone,
    /// Not allowed, with the reason in words.
    Refused(&'static str),
}

impl Drop {
    pub fn allowed(self) -> bool {
        !matches!(self, Drop::Refused(_))
    }
}

/// §6, the table "Van \ Naar". `choosing`: the dispatcher is choosing the owner
/// of this Todo ticket (back to Backlog then asks first).
pub fn drop_effect(from: ColumnKind, to: ColumnKind, choosing: bool) -> Drop {
    use ColumnKind::*;
    match (from, to) {
        (Done, _) => Drop::Refused("Done tickets are closed."),
        (a, b) if a == b => Drop::Reorder,
        (Backlog, Todo) => Drop::Start,
        (Todo, Backlog) => Drop::BackToBacklog { confirm: choosing },
        (Doing | Testing | Review, Backlog) => Drop::BackToBacklog { confirm: true },
        (PlanReview, Backlog) => Drop::Reject,
        (PlanReview, Doing) => Drop::PlanDialog,
        (Review, Doing) => Drop::SendBack,
        (Review, Done) => Drop::MarkDone,
        (Testing, _) | (_, Testing) => Drop::Refused("Testing moves on by itself when the tests have run."),
        (Backlog, _) => Drop::Refused("Start first."),
        (_, Done) => Drop::Refused("Only from Review."),
        (_, Review) => Drop::Refused("The owner hands it in."),
        (_, PlanReview) => Drop::Refused("The engine moves tickets here."),
        (Todo, Doing) => Drop::Refused("Doing starts when an owner is chosen."),
        (Doing, Todo) => Drop::Refused("Not from Doing."),
        (_, Todo) => Drop::Refused("Only from Backlog."),
        (_, Doing) => Drop::Refused("The engine moves tickets here."),
    }
}

/// What a card waits for from a board member, if anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Needs {
    Review,
    PlanReview,
    /// Todo and nobody chooses its owner (B5).
    ChooseOwner,
    /// The main task failed (B6): start again or back to Backlog.
    Failed,
    /// The main task waits for an approval (in the Inbox).
    Approval,
    /// It waits for the own Claude login (missing or expired): Connect (MU3-12 AC2).
    ConnectLogin,
}

impl Needs {
    pub fn label(self) -> &'static str {
        match self {
            Needs::Review => "Needs you · review",
            Needs::PlanReview => "Needs you · plan",
            Needs::ChooseOwner => "Choose an employee",
            Needs::Failed => "✕ Failed · Needs you",
            Needs::Approval => "Waiting for your approval",
            Needs::ConnectLogin => "Needs you · Claude login",
        }
    }

    /// The same state for a board member it does not count for (B6): they can still decide.
    pub fn others_label(self) -> &'static str {
        match self {
            Needs::Review => "Waiting for review",
            Needs::PlanReview => "Waiting for plan review",
            Needs::ChooseOwner => "Choose an employee",
            Needs::Failed => "✕ Failed",
            Needs::Approval => "Waiting for approval",
            Needs::ConnectLogin => "Waiting for a Claude login",
        }
    }

    /// What a card says, by whom it counts for.
    pub fn line(self, mine: bool) -> &'static str {
        if mine { self.label() } else { self.others_label() }
    }
}

/// Whether ✓ Done can close a ticket in Review now (B6), as `ticket.accept` checks it.
#[derive(Debug, Clone, PartialEq)]
pub enum DoneCheck {
    /// The ticket's tasks are not read yet: Done waits for them.
    Loading,
    /// Still at work: linked tasks queued, running or waiting (as the server's
    /// `working_tasks`, the main task left out), and whether a run goes in the
    /// main task's conversation (a question asked in Review).
    Working { tasks: Vec<aurelius_protocol::TaskView>, conversation: bool },
    Ready,
}

impl DoneCheck {
    /// Whether the ✓ Done button can be pressed.
    pub fn allowed(&self, online: bool) -> bool {
        online && *self == DoneCheck::Ready
    }
}

/// `live`: a run goes (or waits to start) in that conversation.
pub fn done_check(detail: Option<&TicketDetail>, live: impl Fn(ConversationId) -> bool) -> DoneCheck {
    use aurelius_domain::task::TaskStatus::{Queued, Running, Waiting};
    let Some(d) = detail else { return DoneCheck::Loading };
    let root = d.ticket.root_task_id;
    let tasks: Vec<aurelius_protocol::TaskView> = d.tasks.iter().filter(|t| Some(t.id) != root && matches!(t.status, Queued | Running | Waiting)).cloned().collect();
    let conversation = d.root_task.as_ref().and_then(|r| r.conversation_id).is_some_and(live);
    if tasks.is_empty() && !conversation { DoneCheck::Ready } else { DoneCheck::Working { tasks, conversation } }
}

/// Whether the dispatcher gave up on a ticket in Todo (B5), as mobile reads it:
/// the server says so with a `ticket.updated` the system made (`needs: owner`),
/// newer than the ticket's last start, choice of an owner or cancel.
/// `activity`: the ticket's timeline, newest first.
pub fn dispatcher_gave_up(activity: &[TimelineCard]) -> bool {
    for card in activity {
        match card.title.as_str() {
            "ticket.started" | "ticket.dispatched" | "ticket.cancelled" => return false,
            "ticket.updated" if card.actor == "system" => return true,
            _ => {}
        }
    }
    false
}

/// The dispatcher is choosing this Todo ticket's owner: there is one
/// (`has_dispatcher`) and it did not give up (`gave_up`).
pub fn choosing(t: &TicketView, has_dispatcher: bool, gave_up: bool) -> bool {
    t.column == ColumnKind::Todo && t.owner_employee_id.is_none() && has_dispatcher && !gave_up
}

/// What a Todo ticket's timeline said at a version: `Some(gave_up)`, or none
/// when it did not load (mobile's `ownerUnknown`).
pub type GaveUp = (u64, Option<bool>);

/// How long an unread timeline waits before it is asked again (as mobile).
pub const GAVE_UP_RETRY_MS: u64 = 15_000;

/// Nobody shows as choosing: the dispatcher gave up, or its timeline did not
/// load and the board member is asked rather than told "is choosing…" (Trace R-m4).
pub fn dispatcher_stopped(read: Option<&GaveUp>) -> bool {
    matches!(read, Some((_, Some(true) | None)))
}

/// The timeline of this Todo ticket did not load at its version.
pub fn owner_unknown(read: Option<&GaveUp>) -> bool {
    matches!(read, Some((_, None)))
}

/// Retry an unread timeline: forget it, so the next read asks again; false when
/// it was read (or changed) since.
pub fn forget_unknown(known: &mut HashMap<TicketId, GaveUp>, ticket: TicketId, version: u64) -> bool {
    let unread = known.get(&ticket) == Some(&(version, None));
    if unread {
        known.remove(&ticket);
    }
    unread
}

/// The Todo tickets without an owner whose timeline is not read at their version yet.
pub fn gave_up_to_read<'a>(view: &'a BoardView, known: &'a HashMap<TicketId, GaveUp>) -> impl Iterator<Item = (TicketId, u64)> + 'a {
    view.columns
        .iter()
        .flat_map(|c| c.tickets.iter())
        .filter(|t| t.column == ColumnKind::Todo && t.owner_employee_id.is_none())
        .filter(move |t| known.get(&t.id).is_none_or(|(v, _)| *v != t.version))
        .map(|t| (t.id, t.version))
}

/// "Needs you" without a server category: the columns Review and Plan review,
/// a failed main task, an approval of the main task, and B5 ("kies een eigenaar").
pub fn needs_you(t: &TicketView, choosing: bool, root_approvals: u32) -> Option<Needs> {
    use aurelius_domain::task::TaskStatus;
    match t.column {
        ColumnKind::Review => Some(Needs::Review),
        ColumnKind::PlanReview => Some(Needs::PlanReview),
        ColumnKind::Backlog | ColumnKind::Done => None,
        ColumnKind::Todo | ColumnKind::Doing if t.root_task_status == Some(TaskStatus::Failed) => Some(Needs::Failed),
        _ if root_approvals > 0 => Some(Needs::Approval),
        ColumnKind::Todo if t.owner_employee_id.is_none() && !choosing => Some(Needs::ChooseOwner),
        _ => None,
    }
}

/// Whether a ticket's need counts for this board member (B6, freeze §1): ◆, the filter, the
/// badge and Waiting on me follow this; `needs` is what the card shows, and anyone can still
/// decide. A login only for its own; anything else for the Responsible, and for everyone when
/// nobody active is responsible. A server without Responsible sends none: for everyone.
pub fn needs_me(needs: Option<Needs>, t: &TicketView, me: Option<UserId>) -> bool {
    match needs {
        None => false,
        Some(Needs::ConnectLogin) => me == Some(t.login_user),
        Some(_) => t.responsible_user_id.is_none() || t.responsible_disabled || t.responsible_user_id == me,
    }
}

/// What the review and plan bars say to someone else than the Responsible (§1.4).
pub fn decide_too(t: &TicketView) -> Option<String> {
    t.responsible_username.as_ref().filter(|_| t.responsible_user_id.is_some() && !t.responsible_disabled).map(|name| format!("{name} is responsible. You can decide too."))
}

/// Whose Claude login a ticket runs on (MU3-10 AC4): "your" or "Alex's".
pub fn login_owner(t: &TicketView, me: Option<UserId>) -> String {
    if me == Some(t.login_user) { "your".into() } else { format!("{}'s", t.login_username) }
}

/// What a ticket that waits for its Claude login says (MU3-12 AC2/AC4); none
/// when it does not. `disabled`: its login's board member was disabled.
pub fn login_wait(t: &TicketView, me: Option<UserId>, disabled: bool) -> Option<String> {
    if t.needs != Some(TicketNeed::Login) {
        return None;
    }
    Some(match (me == Some(t.login_user), disabled) {
        (true, _) => "◆ Waiting for your Claude login".into(),
        (false, true) => format!("Waiting: {} is disabled", t.login_username),
        (false, false) => format!("Waiting for {}'s Claude login", t.login_username),
    })
}

/// The own login's direct action on a card (ui-ontwerp §5): Connect, or Reconnect once it expired.
pub fn connect_label(view: Option<&aurelius_domain::user::ClaudeLoginView>) -> &'static str {
    if view.is_some_and(|v| v.status == aurelius_domain::user::ClaudeLoginStatus::Expired) { "Reconnect" } else { "Connect" }
}

/// Start's toast (P-36, ui-ontwerp §4): without the own Claude login it says only that it waits.
pub fn start_toast(number: u32, started: String, waits_for_login: bool) -> String {
    if waits_for_login { format!("#{number} started. It waits until you connect your Claude login.") } else { started }
}

/// "Run on my login" (B4, D3-3): another board member takes a ticket that waits for
/// someone else's login over on their own; never anyone else's, never silently.
pub fn take_over_offered(t: &TicketView, me: Option<UserId>) -> bool {
    t.needs == Some(TicketNeed::Login) && me.is_some_and(|me| me != t.login_user) && !matches!(t.column, ColumnKind::Done | ColumnKind::Backlog)
}

/// The priority as text and shape, never only colour (§5.1); Normal shows nothing.
pub fn priority_mark(p: TicketPriority) -> Option<(&'static str, u32)> {
    match p {
        TicketPriority::Urgent => Some(("▲▲ Urgent", theme::NEGATIVE)),
        TicketPriority::High => Some(("▲ High", HIGH)),
        TicketPriority::Normal => None,
        TicketPriority::Low => Some(("▽ Low", theme::TEXT_2)),
    }
}

pub fn priority_label(p: TicketPriority) -> &'static str {
    match p {
        TicketPriority::Urgent => "▲▲ Urgent",
        TicketPriority::High => "▲ High",
        TicketPriority::Normal => "Normal",
        TicketPriority::Low => "▽ Low",
    }
}

pub const PRIORITIES: [TicketPriority; 4] = [TicketPriority::Urgent, TicketPriority::High, TicketPriority::Normal, TicketPriority::Low];

/// The one line of an empty column (§3.3).
pub fn empty_text(kind: ColumnKind) -> &'static str {
    match kind {
        ColumnKind::Backlog => "New tickets land here.",
        ColumnKind::Todo => "Started tickets wait here for an owner.",
        ColumnKind::Doing => "Nothing in progress.",
        ColumnKind::PlanReview => "Only tickets in Plan mode stop here, with their plan.",
        ColumnKind::Testing => "Nothing being tested.",
        ColumnKind::Review => "Nothing to review.",
        ColumnKind::Done => "Closed tickets appear here.",
    }
}

/// Whether the board shows a column: Testing only on a server with `board_testing`
/// (ticket #5; an older server never sends it, nor a card in it).
pub fn column_shown(kind: ColumnKind, testing: bool) -> bool {
    kind != ColumnKind::Testing || testing
}

/// The test task of a ticket in Testing: the tester's task on it, the running one
/// before a finished one, the newest first. None when the tester or its task is unknown.
pub fn test_task<'a>(tasks: impl IntoIterator<Item = &'a aurelius_protocol::TaskView>, ticket: TicketId, tester: Option<EmployeeId>) -> Option<&'a aurelius_protocol::TaskView> {
    let tester = tester?;
    tasks.into_iter().filter(|x| x.ticket_id == Some(ticket) && x.employee_id == Some(tester)).max_by_key(|x| (!x.status.is_terminal(), x.created_at_ms))
}

/// What a card in Testing says (ticket #5): it is being tested, and by whom.
pub fn testing_line(tester: Option<&str>) -> String {
    match tester {
        Some(name) => format!("Being tested by {name}"),
        None => "Being tested".into(),
    }
}

/// The ticket panel's line in Testing (ticket #5).
pub fn testing_note(tester: Option<&str>) -> String {
    format!("◆ Testing · {} runs the full test suite. Green goes on to Review; red back to Doing.", tester.unwrap_or("The tester"))
}

/// The tester picker's empty choice (ticket #5).
pub const NO_TESTER: &str = "None: no Testing step";

/// What the toast says once the company's tester is set.
pub fn tester_set_text(name: Option<&str>) -> String {
    match name {
        Some(name) => format!("{name} is the company's tester. Reported tickets go to Testing first."),
        None => "No tester: reported tickets go from Doing straight to Review.".into(),
    }
}

/// `company.update` that only sets the tester: the name and the dispatcher stay.
pub fn company_tester_command(employee: Option<EmployeeId>) -> aurelius_domain::company::UpdateCompany {
    aurelius_domain::company::UpdateCompany { name: None, dispatcher: None, tester: Some(aurelius_domain::company::SetTester { employee_id: employee }) }
}

const DAY_MS: i64 = 86_400_000;

/// Days since 1970-01-01 of a civil date.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(day: i64) -> (i64, i64, i64) {
    let z = day + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// A deadline as typed: `YYYY-MM-DD`, optionally with ` HH:MM` (local time).
/// A day alone means the end of that day. Empty: no deadline. `offset_min`:
/// the local offset from UTC in minutes.
pub fn parse_deadline(text: &str, offset_min: i64) -> Result<Option<i64>, String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    let wrong = || "Write the date as 2026-11-03, optionally with a time: 2026-11-03 17:00.".to_string();
    let (date, time) = match text.split_once([' ', 'T']) {
        Some((d, t)) => (d, Some(t.trim())),
        None => (text, None),
    };
    let parts: Vec<i64> = date.split('-').map(|p| p.parse::<i64>()).collect::<Result<_, _>>().map_err(|_| wrong())?;
    let [y, m, d] = parts[..] else { return Err(wrong()) };
    if !(1970..=9999).contains(&y) || !(1..=12).contains(&m) || !(1..=31).contains(&d) || civil_from_days(days_from_civil(y, m, d)) != (y, m, d) {
        return Err(wrong());
    }
    let minutes = match time {
        None => 23 * 60 + 59,
        Some(t) => {
            let (hh, mm) = t.split_once(':').ok_or_else(wrong)?;
            let (hh, mm): (i64, i64) = (hh.parse().map_err(|_| wrong())?, mm.parse().map_err(|_| wrong())?);
            if !(0..24).contains(&hh) || !(0..60).contains(&mm) {
                return Err(wrong());
            }
            hh * 60 + mm
        }
    };
    Ok(Some(days_from_civil(y, m, d) * DAY_MS + (minutes - offset_min) * 60_000))
}

/// A deadline in the form `parse_deadline` reads (for editing).
pub fn deadline_input(ms: i64, offset_min: i64) -> String {
    let local = ms + offset_min * 60_000;
    let (y, m, d) = civil_from_days(local.div_euclid(DAY_MS));
    let minute = local.rem_euclid(DAY_MS) / 60_000;
    if minute == 23 * 60 + 59 {
        format!("{y:04}-{m:02}-{d:02}")
    } else {
        format!("{y:04}-{m:02}-{d:02} {:02}:{:02}", minute / 60, minute % 60)
    }
}

/// "Due Nov 3", within a day "Due today 17:00" (orange), past "Overdue · Oct 29"
/// (red, not in Done). §5.1.
pub fn deadline_text(deadline_ms: i64, now_ms: i64, done: bool, offset_min: i64) -> (String, u32) {
    let local = |ms: i64| ms + offset_min * 60_000;
    let (_, m, d) = civil_from_days(local(deadline_ms).div_euclid(DAY_MS));
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let day = format!("{} {d}", MONTHS[(m - 1) as usize]);
    if done {
        return (format!("Due {day}"), theme::TEXT_2);
    }
    if deadline_ms < now_ms {
        return (format!("Overdue · {day}"), theme::NEGATIVE);
    }
    if deadline_ms - now_ms <= DAY_MS {
        let minute = local(deadline_ms).rem_euclid(DAY_MS) / 60_000;
        let today = local(deadline_ms).div_euclid(DAY_MS) == local(now_ms).div_euclid(DAY_MS);
        let when = if today { "today" } else { "tomorrow" };
        return (format!("Due {when} {:02}:{:02}", minute / 60, minute % 60), HIGH);
    }
    (format!("Due {day}"), theme::TEXT_2)
}

/// Labels typed as one line, separated by commas.
pub fn parse_labels(text: &str) -> Vec<String> {
    text.split(',').map(str::trim).filter(|l| !l.is_empty()).map(str::to_string).collect()
}

/// The neighbour a card goes right above when it is dropped on `onto` (none: at
/// the bottom of the column). `None` when the drop changes nothing.
pub fn before_for(cards: &[TicketView], dragged: TicketId, onto: Option<TicketId>) -> Option<Option<TicketId>> {
    let at = cards.iter().position(|t| t.id == dragged);
    match onto {
        Some(target) if target == dragged => None,
        // Right above the card it already stands above: unchanged.
        Some(target) if at.and_then(|i| cards.get(i + 1)).is_some_and(|n| n.id == target) => None,
        Some(target) => Some(Some(target)),
        None if at.is_some_and(|i| i + 1 == cards.len()) => None,
        None => Some(None),
    }
}

/// Who the board shows (§3.2: "Anyone", "No owner yet", or an employee).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OwnerFilter {
    #[default]
    Anyone,
    NoOwner,
    Employee(EmployeeId),
}

/// Whose tickets the board shows (ui-ontwerp §3: "Created by: Anyone / Me / <board member>").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CreatorFilter {
    #[default]
    Anyone,
    Me,
    Member(UserId),
}

impl CreatorFilter {
    /// The creator a card must have; none: anyone (also while nobody is signed in).
    pub fn user(self, me: Option<UserId>) -> Option<UserId> {
        match self {
            CreatorFilter::Anyone => None,
            CreatorFilter::Me => me,
            CreatorFilter::Member(u) => Some(u),
        }
    }
}

/// The cards a column shows under the filters: owner, creator, Responsible, "Find" (title,
/// `#number` and labels) and "Needs you only". Done: newest first, `DONE_SHOWN` unless all.
#[allow(clippy::too_many_arguments)]
pub fn shown_cards<'a>(column: &'a BoardColumnCards, owner: OwnerFilter, creator: Option<UserId>, responsible: Option<Option<UserId>>, query: &str, needs: impl Fn(&TicketView) -> bool, needs_only: bool, done_all: bool) -> (Vec<&'a TicketView>, usize) {
    let query = query.trim().to_lowercase();
    let number = query.trim_start_matches('#');
    let mut cards: Vec<&TicketView> = column
        .tickets
        .iter()
        .filter(|t| match owner {
            OwnerFilter::Anyone => true,
            OwnerFilter::NoOwner => t.owner_employee_id.is_none(),
            OwnerFilter::Employee(e) => t.owner_employee_id == Some(e),
        })
        .filter(|t| creator.is_none_or(|u| t.created_by_user == u))
        .filter(|t| crate::screens::who::responsible_matches(t, responsible))
        .filter(|t| {
            query.is_empty()
                || t.title.to_lowercase().contains(&query)
                || (!number.is_empty() && t.number.to_string() == number)
                || t.labels.iter().any(|l| l.to_lowercase().contains(&query))
        })
        .filter(|t| !needs_only || needs(t))
        .collect();
    let mut older = 0;
    if column.column.kind == ColumnKind::Done {
        cards.sort_by_key(|t| std::cmp::Reverse(t.done_at_ms.unwrap_or(t.updated_at_ms)));
        if !done_all && cards.len() > DONE_SHOWN {
            older = cards.len() - DONE_SHOWN;
            cards.truncate(DONE_SHOWN);
        }
    }
    (cards, older)
}

/// Who chooses the owner of this project's started tickets, as the server does
/// (`dispatcher_for`): the project's own, else the company's, and that one only
/// while it is active. A paused or archived one is nobody, also when the other
/// would be active (B5 strict). With the line the ticket view shows (§7.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dispatcher {
    Project(EmployeeId, String),
    Company(EmployeeId, String),
    /// The dispatcher in charge is not active (`state`: "paused", "archived" or
    /// "not active"): nobody chooses, the board member does.
    Inactive { name: String, state: &'static str },
    None,
}

impl Dispatcher {
    pub fn line(&self) -> String {
        match self {
            Dispatcher::Project(_, n) => format!("{n} · set for this project"),
            Dispatcher::Company(_, n) => format!("{n} · company default"),
            Dispatcher::Inactive { name, state } => format!("{name} is {state}. Started tickets wait for you to choose an owner."),
            Dispatcher::None => "No dispatcher. Started tickets wait in Todo until you choose an owner.".into(),
        }
    }

    /// The one that chooses; none when nobody does (also when it is not active).
    pub fn name(&self) -> Option<&str> {
        match self {
            Dispatcher::Project(_, n) | Dispatcher::Company(_, n) => Some(n),
            _ => None,
        }
    }

    /// Why nobody chooses, as a sentence start ("Lead is paused"); none when one does.
    pub fn why_nobody(&self) -> Option<String> {
        match self {
            Dispatcher::Inactive { name, state } => Some(format!("{name} is {state}")),
            Dispatcher::None => Some("No dispatcher is set".into()),
            _ => None,
        }
    }

    /// The owner choice "the dispatcher" in a picker.
    pub fn choice(&self) -> String {
        match self {
            Dispatcher::Project(_, n) | Dispatcher::Company(_, n) => format!("Dispatcher ({n})"),
            Dispatcher::Inactive { name, state } => format!("Dispatcher ({name}, {state})"),
            Dispatcher::None => "Dispatcher (none set)".into(),
        }
    }
}

pub fn dispatcher_of(project: Option<EmployeeId>, company: Option<EmployeeId>, employees: &[aurelius_protocol::EmployeeSummary]) -> Dispatcher {
    use aurelius_domain::employee::LifecycleStatus;
    let Some(id) = project.or(company) else { return Dispatcher::None };
    let own = project.is_some();
    match employees.iter().find(|e| e.id == id) {
        Some(e) if e.status == LifecycleStatus::Active => if own { Dispatcher::Project(id, e.name.clone()) } else { Dispatcher::Company(id, e.name.clone()) },
        Some(e) => Dispatcher::Inactive { name: e.name.clone(), state: if e.status == LifecycleStatus::Paused { "paused" } else { "archived" } },
        None => Dispatcher::Inactive { name: if own { "The project's dispatcher" } else { "The company's dispatcher" }.into(), state: "not active" },
    }
}

/// The required feedback of "Send back to Doing" / "Pas aan" (B2): not empty after trimming.
pub fn feedback_ready(text: &str) -> bool {
    !text.trim().is_empty()
}

/// The New ticket form's problems (MU1-07 AC2): title, then project.
pub fn new_ticket_problems(title: &str, project: Option<ProjectId>) -> (Option<&'static str>, Option<&'static str>) {
    let title = title.trim();
    let title_problem = if title.is_empty() {
        Some("Required.")
    } else if title.chars().count() > ticket::TITLE_MAX {
        Some("At most 160 characters.")
    } else {
        None
    };
    (title_problem, project.is_none().then_some("Choose a project."))
}

/// Columns renamed or moved, as `board.columns_update` takes them (all six, left to right).
pub fn columns_edit(columns: &[ticket::BoardColumnView]) -> ticket::UpdateBoardColumns {
    let mut sorted: Vec<&ticket::BoardColumnView> = columns.iter().collect();
    sorted.sort_by_key(|c| c.position);
    ticket::UpdateBoardColumns { columns: sorted.into_iter().map(|c| ticket::BoardColumnEdit { column_id: c.id, name: c.name.trim().to_string() }).collect() }
}

/// Restore defaults (MU1-06 AC5): every column its own name and place again.
pub fn default_columns(columns: &mut [ticket::BoardColumnView]) {
    for column in columns.iter_mut() {
        column.name = ticket::column_title(column.kind).to_string();
        column.position = ColumnKind::ALL.iter().position(|k| *k == column.kind).unwrap_or(0) as u32;
    }
}

/// Swap a column with its neighbour (`left`: to the left).
pub fn move_column(columns: &mut [ticket::BoardColumnView], index: usize, left: bool) -> bool {
    let mut order: Vec<usize> = (0..columns.len()).collect();
    order.sort_by_key(|&i| columns[i].position);
    let Some(at) = order.iter().position(|&i| i == index) else { return false };
    let other = if left { at.checked_sub(1) } else { Some(at + 1).filter(|&o| o < order.len()) };
    let Some(other) = other else { return false };
    let (a, b) = (order[at], order[other]);
    let pa = columns[a].position;
    columns[a].position = columns[b].position;
    columns[b].position = pa;
    true
}

/// Who did something to a ticket, from the kind of actor a timeline card
/// carries (the card has no name): a board member, an employee or Aurelius.
pub fn activity_who(card: &TimelineCard) -> &'static str {
    match card.actor.as_str() {
        "user" => "A board member",
        "employee" => "The owner",
        _ => "Aurelius",
    }
}

/// One line of a ticket's Activity (MU1-15 AC4), worded as on mobile.
pub fn activity_text(card: &TimelineCard) -> String {
    let who = activity_who(card);
    match card.title.as_str() {
        "ticket.created" => format!("{who} created the ticket"),
        "ticket.started" => format!("{who} started it"),
        "ticket.dispatched" if card.actor == "employee" => "The dispatcher chose an owner".into(),
        "ticket.dispatched" => format!("{who} chose an owner"),
        "ticket.moved" => format!("{who} moved it"),
        "ticket.plan_review" => "A plan was handed in".into(),
        "ticket.plan_approved" => format!("{who} approved the plan"),
        "ticket.plan_revised" => format!("{who} sent the plan back"),
        "ticket.review" => "A report was handed in".into(),
        // Ticket #5: the server words it ("Tests passed" / "Tests failed"); a bare event says only that they ran.
        "ticket.tested" => match card.detail.as_str() {
            "passed" => "Tests passed".into(),
            "failed" => "Tests failed".into(),
            _ => "The tests ran".into(),
        },
        "ticket.sent_back" => format!("{who} sent it back to Doing"),
        "ticket.accepted" => format!("{who} marked it as Done"),
        "ticket.cancelled" => format!("{who} put it back in Backlog"),
        "ticket.archived" => format!("{who} archived it"),
        "ticket.updated" if card.actor == "system" => "Nobody chose an owner; a board member must".into(),
        "ticket.updated" if card.detail.contains("responsible") => format!("{who} changed who is responsible"),
        "ticket.updated" => format!("{who} changed it"),
        // Plan approved: the main task's conversation left Plan mode (D-07).
        "conversation.settings_changed" => "The main task's mode changed".into(),
        // An event the server does not word: never its raw name (Pulse m4).
        other if is_event_name(other) => match other.rsplit_once('.') {
            Some((_, what)) => format!("{who}: {}", what.replace('_', " ")),
            None => format!("{who} did something"),
        },
        // A card the server words itself (the main task's): its title.
        other => other.to_string(),
    }
}

/// A raw event type ("conversation.settings_changed"), not a sentence.
fn is_event_name(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|c| c.is_ascii_lowercase() || c == '.' || c == '_')
}

/// The second line of an Activity row: the card's detail, unless it only names
/// the kind ("ticket", "conversation": what the server puts there for an event it does not word).
pub fn activity_detail(card: &TimelineCard) -> Option<&str> {
    Some(card.detail.as_str()).filter(|d| !d.is_empty() && !(card.kind == "activity" && is_event_name(d)) && !(card.title == "ticket.tested" && matches!(*d, "passed" | "failed")))
}

/// Which feedback the Doing bar shows (§7.3), from the newest of the ticket's
/// decisions in its activity (newest first): "Sent back" only after Review.
pub fn feedback_label(activity: Option<&[TimelineCard]>) -> &'static str {
    let last = activity.and_then(|cards| cards.iter().find_map(|c| match c.title.as_str() {
        t @ ("ticket.sent_back" | "ticket.plan_approved" | "ticket.plan_revised") => Some(t),
        _ => None,
    }));
    match last {
        Some("ticket.sent_back") => "Sent back",
        Some("ticket.plan_approved") => "With the plan's approval",
        Some("ticket.plan_revised") => "New plan asked",
        _ => "Feedback",
    }
}

/// The Mode row (D-07): a ticket started in Plan shows where its main task's
/// conversation went after the approval ("Plan → Auto"); else the ticket's mode.
pub fn mode_text(ticket_mode: Option<aurelius_domain::autonomy::AgentMode>, conversation: Option<aurelius_domain::autonomy::AgentMode>) -> Option<String> {
    use aurelius_domain::autonomy::AgentMode;
    match (ticket_mode, conversation) {
        (Some(AgentMode::Plan), Some(now)) if now != AgentMode::Plan => Some(format!("Plan → {}", now.label())),
        _ => None,
    }
}

// ---- Keyboard and screen readers (§15) --------------------------------------

/// An arrow key on the Board.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Left,
    Right,
    Up,
    Down,
}

/// The Board's keys (§15) work only on the page itself: never behind a board
/// dialog ("Mark as Done?", New ticket…) or another overlay (a menu, the palette).
pub fn board_keys_live(dialog: Option<BoardDialog>, overlay: Overlay) -> bool {
    dialog.is_none() && overlay == Overlay::None
}

/// The cards as the Board shows them, column by column from left to right.
pub type Grid = Vec<(ColumnKind, Vec<TicketId>)>;

fn find_in(grid: &Grid, id: TicketId) -> Option<(usize, usize)> {
    grid.iter().enumerate().find_map(|(ci, (_, cards))| cards.iter().position(|t| *t == id).map(|ri| (ci, ri)))
}

/// The card the keyboard goes to (§15): ↑↓ within the column, ←→ to the nearest
/// column with cards, at the same height or its last card. Nothing focused yet:
/// the first card. A focused card that is gone: the first card too.
pub fn focus_step(grid: &Grid, focused: Option<TicketId>, step: Step) -> Option<TicketId> {
    let first = || grid.iter().find_map(|(_, cards)| cards.first().copied());
    let Some((ci, ri)) = focused.and_then(|id| find_in(grid, id)) else { return first() };
    let cards = &grid[ci].1;
    let side = |column: Option<usize>| column.map(|c| grid[c].1[ri.min(grid[c].1.len() - 1)]).unwrap_or(cards[ri]);
    let filled = |c: &usize| !grid[*c].1.is_empty();
    Some(match step {
        Step::Up => cards[ri.saturating_sub(1)],
        Step::Down => cards[(ri + 1).min(cards.len() - 1)],
        Step::Left => side((0..ci).rev().find(filled)),
        Step::Right => side((ci + 1..grid.len()).find(filled)),
    })
}

/// A card picked up with Space (§15): where it would go, as a column of the
/// grid and a place among that column's other cards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Held {
    pub id: TicketId,
    pub number: u32,
    pub from: ColumnKind,
    pub choosing: bool,
    pub to: usize,
    pub at: usize,
}

/// The other cards of the column `to` (the held one does not count).
fn others(grid: &Grid, held: &Held, to: usize) -> usize {
    grid.get(to).map_or(0, |(_, cards)| cards.iter().filter(|t| **t != held.id).count())
}

/// Pick up a card where it stands; none when it is not shown.
pub fn pick_up(grid: &Grid, id: TicketId, number: u32, choosing: bool) -> Option<Held> {
    let (ci, ri) = find_in(grid, id)?;
    Some(Held { id, number, from: grid[ci].0, choosing, to: ci, at: ri })
}

/// A held card follows the arrows: ←→ every column (also an empty one), ↑↓ its place.
pub fn held_step(grid: &Grid, held: Held, step: Step) -> Held {
    let to = match step {
        Step::Left => held.to.saturating_sub(1),
        Step::Right => (held.to + 1).min(grid.len().saturating_sub(1)),
        _ => held.to,
    };
    let room = others(grid, &held, to);
    let at = match step {
        Step::Up => held.at.saturating_sub(1),
        Step::Down => held.at + 1,
        _ => held.at,
    };
    Held { to, at: at.min(room), ..held }
}

/// Where a held card is put down: its column, and the card it goes right above (none: the bottom).
pub fn held_target(grid: &Grid, held: &Held) -> Option<(ColumnKind, Option<TicketId>)> {
    let (kind, cards) = grid.get(held.to)?;
    Some((*kind, cards.iter().filter(|t| **t != held.id).nth(held.at).copied()))
}

/// The one label a card has for a screen reader (§15): number, title, priority,
/// owner, state, deadline and column; `who`: the who block's sentence (AUR-FA §1.3).
pub fn card_label(t: &TicketView, state: Option<&str>, deadline: Option<&str>, column: &str, who: Option<&str>) -> String {
    let priority = match t.priority {
        TicketPriority::Urgent => "Urgent",
        TicketPriority::High => "High",
        TicketPriority::Normal => "Normal",
        TicketPriority::Low => "Low",
    };
    let mut parts = vec![format!("Ticket {}", t.number), t.title.clone(), format!("priority {priority}"), format!("owner {}", t.owner_name.as_deref().unwrap_or("none"))];
    parts.extend(state.map(str::to_string));
    parts.extend(deadline.map(|d| {
        let mut chars = d.chars();
        chars.next().map(|f| f.to_lowercase().chain(chars).collect::<String>()).unwrap_or_default()
    }));
    parts.push(format!("in {column}"));
    let label = format!("{}.", parts.join(", "));
    match who {
        Some(who) => format!("{label} {who}"),
        None => label,
    }
}

/// The cards another device or an employee moved to another column, between two
/// reads of the board, without the ones this window moved itself (`own`).
pub fn moved_elsewhere(old: &BoardView, new: &BoardView, own: &HashSet<TicketId>) -> Vec<(u32, ColumnKind, Option<String>)> {
    let before: HashMap<TicketId, ColumnKind> = old.columns.iter().flat_map(|c| c.tickets.iter()).map(|t| (t.id, t.column)).collect();
    new.columns
        .iter()
        .flat_map(|c| c.tickets.iter())
        .filter(|t| !own.contains(&t.id) && before.get(&t.id).is_some_and(|k| *k != t.column))
        .map(|t| (t.number, t.column, t.owner_name.clone()))
        .collect()
}

/// What a screen reader hears of moves made elsewhere, bundled (§15): "#42 moved
/// to Review by Forge." A hand-in to Review or Plan review is the owner's.
pub fn moves_announcement(moves: &[(u32, ColumnKind, Option<String>)], name: impl Fn(ColumnKind) -> String) -> Option<String> {
    let one = |(number, kind, owner): &(u32, ColumnKind, Option<String>)| {
        let by = owner.as_ref().filter(|_| matches!(kind, ColumnKind::Review | ColumnKind::PlanReview)).map(|o| format!(" by {o}")).unwrap_or_default();
        (format!("#{number} moved to {}{by}", name(*kind)), format!("#{number} to {}", name(*kind)))
    };
    match moves {
        [] => None,
        [m] => Some(format!("{}.", one(m).0)),
        many => Some(format!("{} tickets moved: {}.", many.len(), many.iter().map(|m| one(m).1).collect::<Vec<_>>().join(", "))),
    }
}

/// At most one announcement of others' moves per this many milliseconds (§15).
pub const ANNOUNCE_EVERY_MS: i64 = 5_000;

/// The ticket view's tabs (§7.4); the conversation opens from the action bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TicketTab {
    #[default]
    Ticket,
    /// The ticket's discussion (AUR-MU-02, ui-ontwerp fase 2 §1).
    Discussion,
    Activity,
}

// ---- State ------------------------------------------------------------------

/// A board dialog over the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoardDialog {
    NewTicket,
    /// "Send back to Doing" (after a drag or from the menu, §9.2).
    SendBack(TicketId),
    /// "Mark as Done?" (§9.3).
    MarkDone(TicketId),
    /// The plan with its buttons (a drag from Plan review to Doing, §9.1).
    Plan(TicketId),
    /// Back to Backlog stops the work: asked first.
    BackToBacklog(TicketId),
    /// "Afwijzen": the plan is rejected and the ticket goes back to Backlog.
    Reject(TicketId),
    /// Delete (archive) a ticket in Backlog.
    Delete(TicketId),
    /// "#52 was already created from this message. Create another?" (fase 2 §4).
    AnotherTicket,
    /// Settings → Board: the columns and the company's dispatcher (§12.1).
    Columns,
}

/// What the board's menus do.
#[derive(Debug, Clone)]
pub enum BoardDo {
    Open(TicketId),
    Start(TicketId, Option<EmployeeId>),
    Move(TicketId, ColumnKind),
    /// Owner of a ticket (`None`: the dispatcher, Backlog only).
    Owner(TicketId, Option<EmployeeId>),
    Priority(TicketId, TicketPriority),
    Mode(TicketId, Option<aurelius_domain::autonomy::AgentMode>),
    Model(TicketId, Option<(aurelius_domain::ProviderId, String)>),
    CopyLink(TicketId),
    Dialog(BoardDialog),
    FilterProject(Option<ProjectId>),
    FilterOwner(OwnerFilter),
    FilterCreator(CreatorFilter),
    /// The Responsible of a ticket (`None`: nobody, AUR-FA A1).
    Responsible(TicketId, Option<UserId>),
    /// "Set responsible…" on a card.
    ResponsibleMenu(TicketId),
    /// Follow (`true`) or unfollow a ticket (AUR-FA A2).
    Follow(TicketId, bool),
    FilterResponsible(ResponsibleFilter),
    FormResponsible(FormResponsible),
    /// The New ticket form's dropdowns.
    FormProject(ProjectId),
    FormPriority(TicketPriority),
    FormOwner(Option<EmployeeId>),
    FormMode(Option<aurelius_domain::autonomy::AgentMode>),
    FormModel(Option<(aurelius_domain::ProviderId, String)>),
    /// Project settings: its dispatcher (`None`: the company's).
    ProjectDispatcher(ProjectId, Option<EmployeeId>),
    /// Settings → Board: the company's dispatcher (`None`: nobody, B5).
    CompanyDispatcher(Option<EmployeeId>),
    /// Settings → Company and Board: the company's tester (`None`: no Testing step, ticket #5).
    CompanyTester(Option<EmployeeId>),
    /// "Create ticket…" on a chat message (MU2-10, fase 2 §4).
    TicketFromMessage(aurelius_domain::MessageId),
}

/// The New ticket form besides its fields (§10.1).
pub struct NewTicket {
    pub title: Entity<InputState>,
    pub description: Entity<TextareaState>,
    pub labels: Entity<InputState>,
    pub deadline: Entity<InputState>,
    pub project: Option<ProjectId>,
    pub priority: TicketPriority,
    pub owner: Option<EmployeeId>,
    /// Who follows up (feature `ticket_responsible`): You unless chosen otherwise (B9).
    pub responsible: FormResponsible,
    pub run: aurelius_domain::task::RunSettings,
    /// Files on this computer to attach (path, size), uploaded first to a remote server (MU1-07).
    pub files: Vec<(std::path::PathBuf, u64)>,
    /// Create was pressed: problems show.
    pub sent: bool,
    /// Waiting for `ticket.create`.
    pub busy: bool,
    pub error: Option<String>,
    /// Empty the fields on the next render (they need the window).
    pub reset: bool,
    /// Made from this chat message (MU2-10): sent as `from_message`.
    pub from_message: Option<aurelius_domain::MessageId>,
    /// Fill the title and description with these on the next render (MU2-10 AC1).
    pub prefill: Option<crate::screens::discussion::Prefill>,
}

/// "Create ticket" on a message that already made one: what to ask before the form opens.
#[derive(Debug, Clone)]
pub struct AnotherTicket {
    pub message: aurelius_domain::MessageId,
    pub project: Option<ProjectId>,
    pub prefill: crate::screens::discussion::Prefill,
    /// "#52": the ticket made from it before.
    pub existing: String,
}

pub struct Board {
    pub view: Option<BoardView>,
    /// Why the board did not load (shown with Try again).
    pub error: Option<String>,
    /// The filter of a `board.get` on its way.
    pub asked: Option<Option<ProjectId>>,
    /// Asked again while that one was on its way: one more read when it answers (D-m3).
    pub again: bool,
    /// The project filter (B4), remembered per client.
    pub project: Option<ProjectId>,
    pub owner: OwnerFilter,
    /// "Created by" (ui-ontwerp §3).
    pub creator: CreatorFilter,
    /// "Responsible" (AUR-FA P-77), remembered per client; counts only with `ticket_responsible`.
    pub responsible: ResponsibleFilter,
    /// Who follows each ticket read so far (`ticket.followers`), or why it did not load.
    pub followers: HashMap<TicketId, Result<Vec<ticket::FollowerView>, String>>,
    /// The names of the open ticket's followers show.
    pub followers_open: bool,
    /// The pointer is on the Follow button: it says Unfollow.
    pub follow_hover: bool,
    pub needs_only: bool,
    pub find: Entity<InputState>,
    /// The ticket in the right panel, once loaded.
    pub detail: Option<TicketDetail>,
    pub detail_error: Option<String>,
    /// The ticket of a dialog (Plan, Mark as Done) that is not the one in the
    /// panel, read for the dialog alone: the panel's stays as it is.
    pub dialog_detail: Option<TicketDetail>,
    pub dialog_error: Option<String>,
    /// The latest plan per main task's conversation (Plan review; none inside: not found),
    /// and at which ticket version it was asked (read again only for a new one, D-m3).
    pub plans: HashMap<ConversationId, Option<String>>,
    pub plans_read: HashMap<ConversationId, u64>,
    /// The agent mode of a main task's conversation, read with its plan (D-07).
    pub modes: HashMap<ConversationId, aurelius_domain::autonomy::AgentMode>,
    pub tab: TicketTab,
    /// The open ticket's activity, newest first (or why it did not load).
    pub activity: Option<(TicketId, Result<Vec<TimelineCard>, String>)>,
    /// Per Todo ticket without an owner: at which version its timeline was read
    /// and whether the dispatcher gave up (B5). Read again when the version changes.
    /// None: it did not load; asked again after `GAVE_UP_RETRY_MS`.
    pub gave_up: HashMap<TicketId, GaveUp>,
    /// Those reads on their way.
    pub gave_up_asked: HashSet<(TicketId, u64)>,
    /// The card the keyboard is on (its focus ring shows, §15).
    pub focused: Option<TicketId>,
    /// A card picked up with Space.
    pub held: Option<Held>,
    /// What the screen reader is told now (a `Role::Status` element), with a counter so a repeat is new.
    pub announce: Option<(u64, String)>,
    /// When others' moves were last told, and the ones waiting for the next turn.
    pub announced_at: i64,
    pub pending_moves: Vec<(u32, ColumnKind, Option<String>)>,
    /// Tickets this window changed: their move is told by its own toast, not again.
    pub own: HashSet<TicketId>,
    /// Tickets with a write on its way (their buttons wait).
    pub busy: HashSet<TicketId>,
    pub dialog: Option<BoardDialog>,
    /// Feedback for Send back / Pas aan; kept until another ticket opens (§9.2).
    pub feedback: Entity<TextareaState>,
    pub feedback_for: Option<TicketId>,
    /// Send back was pressed without feedback: say why under the field.
    pub feedback_missing: bool,
    pub form: NewTicket,
    pub done_all: bool,
    /// The user opened the empty Plan review column.
    pub plan_open: bool,
    /// The ticket a task in the panel was opened from ("‹ #42").
    pub back_to: Option<TicketId>,
    /// Inline edits in the ticket view: title, description, labels, deadline.
    pub edit_title: Entity<InputState>,
    pub edit_description: Entity<TextareaState>,
    pub edit_labels: Entity<InputState>,
    pub edit_deadline: Entity<InputState>,
    pub editing: Option<EditField>,
    pub edit_error: Option<String>,
    /// Files pasted into the title or description being edited (path, size), added on Save.
    pub edit_files: Vec<(std::path::PathBuf, u64)>,
    /// Settings → Board: the columns being edited, with a name field each.
    pub columns: Vec<(ticket::BoardColumnView, Entity<InputState>)>,
    pub columns_error: Option<String>,
    pub columns_reload: ColumnsReload,
    /// The board changed while it did not show: read it when it shows again.
    pub stale: bool,
    /// The open ticket's discussion (AUR-MU-02): its conversation and messages, or why they did not load.
    pub discussion: Option<(ConversationId, Option<Result<Vec<aurelius_protocol::MessageView>, String>>)>,
    /// How many discussion messages the board member saw per ticket (the tab's count).
    pub discussion_seen: HashMap<TicketId, usize>,
    /// The discussion's composer, and the ticket its draft belongs to (kept until another opens).
    pub composer: Entity<TextareaState>,
    pub composer_for: Option<TicketId>,
    /// Empty the composer on the next render (an update has no window).
    pub composer_clear: bool,
    /// The discussion's scroll position, and whether the next read follows the
    /// board member's own post to the bottom.
    pub discussion_scroll: ScrollHandle,
    pub discussion_mine: bool,
    /// A post on its way, and why the last one did not go (the draft stays).
    pub posting: bool,
    pub post_error: Option<String>,
    /// Send back / Pas aan in the discussion without text: say why.
    pub composer_missing: bool,
    /// The `@` suggestions' active one, and whether Esc closed them.
    pub mentions: crate::screens::discussion::MentionPicker,
    /// "Try again" on a failed summary on its way, and why it did not go (MU2-12 AC4).
    pub retrying: bool,
    pub retry_error: Option<String>,
    /// Reports opened inline in the discussion ("Open report").
    pub open_reports: HashSet<aurelius_domain::MessageId>,
    /// Waiting for the board member's answer to "Create another?".
    pub another: Option<AnotherTicket>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditField {
    Title,
    Description,
    Labels,
    Deadline,
}

impl Board {
    pub fn new(window: &mut Window, cx: &mut Context<AppView>) -> Self {
        let input = |placeholder: &'static str, window: &mut Window, cx: &mut Context<AppView>| cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
        let area = |placeholder: &'static str, window: &mut Window, cx: &mut Context<AppView>| cx.new(|cx| TextareaState::new(window, cx).auto_grow(3, 10).placeholder(placeholder));
        let project = crate::app::ui_prefs()[PREF_PROJECT].as_str().and_then(|s| ProjectId::parse(s).ok());
        Self {
            view: None,
            error: None,
            asked: None,
            again: false,
            project,
            owner: OwnerFilter::Anyone,
            creator: CreatorFilter::Anyone,
            responsible: crate::app::ui_prefs()[PREF_RESPONSIBLE].as_str().map_or(ResponsibleFilter::Anyone, ResponsibleFilter::from_pref),
            followers: HashMap::new(),
            followers_open: false,
            follow_hover: false,
            needs_only: false,
            find: input("Find… (title, #number, label)", window, cx),
            detail: None,
            detail_error: None,
            dialog_detail: None,
            dialog_error: None,
            plans: HashMap::new(),
            plans_read: HashMap::new(),
            modes: HashMap::new(),
            tab: TicketTab::Ticket,
            activity: None,
            gave_up: HashMap::new(),
            gave_up_asked: HashSet::new(),
            focused: None,
            held: None,
            announce: None,
            announced_at: 0,
            pending_moves: Vec::new(),
            own: HashSet::new(),
            busy: HashSet::new(),
            dialog: None,
            feedback: area("What needs to change?", window, cx),
            feedback_for: None,
            feedback_missing: false,
            form: NewTicket {
                title: input("What should happen?", window, cx),
                description: area("Details, in Markdown", window, cx),
                labels: input("backend, ui", window, cx),
                deadline: input("2026-11-03 or 2026-11-03 17:00", window, cx),
                project: None,
                priority: TicketPriority::Normal,
                owner: None,
                responsible: FormResponsible::You,
                run: Default::default(),
                files: Vec::new(),
                sent: false,
                busy: false,
                error: None,
                reset: false,
                from_message: None,
                prefill: None,
            },
            done_all: false,
            plan_open: false,
            back_to: None,
            edit_title: input("Title", window, cx),
            edit_description: area("Details, in Markdown", window, cx),
            edit_labels: input("backend, ui", window, cx),
            edit_deadline: input("2026-11-03 or 2026-11-03 17:00", window, cx),
            editing: None,
            edit_error: None,
            edit_files: Vec::new(),
            columns: Vec::new(),
            columns_error: None,
            columns_reload: ColumnsReload::Idle,
            stale: false,
            discussion: None,
            discussion_seen: HashMap::new(),
            composer: cx.new(|cx| TextareaState::new(window, cx).auto_grow(2, 8).submit_on_enter(true).placeholder("Message the discussion · @ to mention")),
            composer_for: None,
            composer_clear: false,
            discussion_scroll: ScrollHandle::new(),
            discussion_mine: false,
            posting: false,
            post_error: None,
            composer_missing: false,
            mentions: Default::default(),
            retrying: false,
            retry_error: None,
            open_reports: HashSet::new(),
            another: None,
        }
    }

    /// Re-render while the find field or a feedback field changes; Enter saves an inline edit.
    pub fn subscriptions(&self, window: &mut Window, cx: &mut Context<AppView>) -> Vec<Subscription> {
        vec![
            cx.observe(&self.find, |_, _, cx| cx.notify()),
            cx.observe(&self.feedback, |this, _, cx| {
                this.board.feedback_missing = false;
                cx.notify();
            }),
            cx.observe(&self.form.title, |_, _, cx| cx.notify()),
            // The discussion's composer: suggestions and warnings follow the text; Enter posts.
            cx.subscribe_in(&self.composer, window, |this, _, e: &InputEvent, w, cx| match e {
                InputEvent::PressEnter { shift: false, .. } => this.discussion_post(w, cx),
                InputEvent::Change => {
                    this.board.composer_missing = false;
                    let text = this.board.composer.read(cx).value().to_string();
                    this.board.mentions.changed(&text);
                    cx.notify();
                }
                _ => {}
            }),
            cx.subscribe_in(&self.edit_title, window, |this, _, e: &InputEvent, w, cx| if let InputEvent::PressEnter { .. } = e { this.ticket_save_edit(w, cx) }),
            cx.subscribe_in(&self.edit_labels, window, |this, _, e: &InputEvent, w, cx| if let InputEvent::PressEnter { .. } = e { this.ticket_save_edit(w, cx) }),
            cx.subscribe_in(&self.edit_deadline, window, |this, _, e: &InputEvent, w, cx| if let InputEvent::PressEnter { .. } = e { this.ticket_save_edit(w, cx) }),
            cx.subscribe_in(&self.form.title, window, |this, _, e: &InputEvent, w, cx| if let InputEvent::PressEnter { .. } = e { this.board_create(false, w, cx) }),
        ]
    }

    /// The ticket read with its tasks, for the panel or for a dialog.
    pub fn detail_of(&self, id: TicketId) -> Option<&TicketDetail> {
        let dialog = self.dialog_detail.as_ref().filter(|_| self.dialog.is_some());
        [self.detail.as_ref(), dialog].into_iter().flatten().find(|d| d.ticket.id == id)
    }

    pub fn ticket(&self, id: TicketId) -> Option<&TicketView> {
        if let Some(d) = self.detail_of(id) {
            return Some(&d.ticket);
        }
        self.view.as_ref()?.columns.iter().flat_map(|c| c.tickets.iter()).find(|t| t.id == id)
    }

    pub fn column(&self, kind: ColumnKind) -> Option<&ticket::BoardColumnView> {
        self.view.as_ref()?.columns.iter().map(|c| &c.column).find(|c| c.kind == kind)
    }

    /// The name a column has on this server (renamed or not).
    pub fn column_name(&self, kind: ColumnKind) -> String {
        self.column(kind).map_or_else(|| ticket::column_title(kind).to_string(), |c| c.name.clone())
    }
}

/// A card being dragged.
#[derive(Clone)]
pub struct DraggedCard {
    pub id: TicketId,
    pub from: ColumnKind,
    pub number: u32,
    pub title: String,
    pub choosing: bool,
}

/// What follows the mouse while a card is dragged.
struct CardGhost(DraggedCard);

impl Render for CardGhost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        v().w(px(240.)).p(px(10.)).gap(px(4.)).rounded(px(8.)).border_1().border_color(c(theme::FOCUS_RING)).bg(c(theme::SURFACE_2)).shadow_lg().opacity(0.92)
            .child(div().text_size(px(11.)).text_color(c(theme::TEXT_2)).child(format!("#{}", self.0.number)))
            .child(div().text_size(px(13.)).font_weight(FontWeight::MEDIUM).line_clamp(2).child(self.0.title.clone()))
    }
}

pub fn menu_entry(label: impl Into<SharedString>, run: BoardDo, checked: bool, detail: Option<String>) -> Option<MenuEntry> {
    Some(MenuEntry { label: label.into(), keys: None, run: MenuDo::Board(run), danger: false, checked, detail: detail.map(Into::into) })
}

/// Where a held card would go: a line with its number (§15).
fn held_line(number: u32) -> AnyElement {
    h().gap(px(6.))
        .child(div().flex_1().h(px(2.)).rounded(px(1.)).bg(c(theme::FOCUS_RING)))
        .child(div().text_size(px(11.)).text_color(c(theme::TEXT_2)).child(format!("#{number} here")))
        .into_any_element()
}

/// A menu row that shows why it cannot be chosen (it does nothing).
fn refused_entry(label: impl Into<SharedString>, why: &str) -> Option<MenuEntry> {
    Some(MenuEntry { label: label.into(), keys: None, run: MenuDo::Call(|_, _, _| {}), danger: false, checked: false, detail: Some(why.to_string().into()) })
}

/// The ticket a dialog reads for itself with `ticket.get` (`panel`: the ticket in the panel, read already).
pub fn dialog_reads(dialog: Option<BoardDialog>, panel: Option<TicketId>) -> Option<TicketId> {
    match dialog {
        Some(BoardDialog::Plan(id) | BoardDialog::MarkDone(id)) if panel != Some(id) => Some(id),
        _ => None,
    }
}

/// Settings → Board after a version conflict (D-m2): the copy of the columns is
/// built again from the board the server sends next, so Save can work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColumnsReload {
    #[default]
    Idle,
    /// Refused as stale: waiting for the board that is read again.
    Waiting,
    /// That board is here: rebuild the fields on the next render (they need the window).
    Rebuild,
}

impl ColumnsReload {
    /// A columns write failed: only a conflict waits for the newer board.
    pub fn failed(&mut self, message: &str) {
        if message == backend::BOARD_CONFLICT {
            *self = ColumnsReload::Waiting;
        }
    }

    pub fn loaded(&mut self) {
        if *self == ColumnsReload::Waiting {
            *self = ColumnsReload::Rebuild;
        }
    }

    /// Whether to rebuild now (once).
    pub fn take_rebuild(&mut self) -> bool {
        std::mem::take(self) == ColumnsReload::Rebuild
    }
}

/// The project the board shows: on a project's Board tab that project, else the
/// Board page's own filter (`chosen`); none when that project is gone.
pub fn board_scope(screen: crate::app::Page, chosen: Option<ProjectId>, known: impl Fn(ProjectId) -> bool) -> Option<ProjectId> {
    let page = match screen {
        crate::app::Page::Project(crate::app::ProjectRef::Real(id), crate::app::Tab::Board) => Some(id),
        _ => None,
    };
    page.or(chosen).filter(|p| known(*p))
}

/// Whether the board page reads the board as it draws: nothing on its way and
/// no board (or a stale one) yet, or the board read is another scope's.
pub fn load_on_show(asked: Option<Option<ProjectId>>, current: bool, stale: bool, scope: Option<ProjectId>) -> bool {
    match asked {
        None => !current || stale,
        Some(a) => !current && a != scope,
    }
}

/// Whether a `board.get` goes now: not while one with the same filter is on its way (D-m3).
pub fn load_now(asked: Option<Option<ProjectId>>, project: Option<ProjectId>) -> bool {
    asked != Some(project)
}

/// Whether a burst of events changes what the board shows: a ticket, the columns,
/// the dispatcher, or a task that works for a ticket. `on_ticket`: whether a known
/// task has a ticket; an unknown one (new, or an id that does not parse) counts.
pub fn board_changed(tickets: bool, settings: bool, tasks: &[String], on_ticket: impl Fn(aurelius_domain::TaskId) -> Option<bool>) -> bool {
    tickets || settings || tasks.iter().any(|id| aurelius_domain::TaskId::parse(id).ok().and_then(&on_ticket).unwrap_or(true))
}

/// Whether to read a main task's plan again: its ticket has a new version, or no plan was found yet.
pub fn plan_to_read(read_at: Option<u64>, plan: Option<&Option<String>>, version: u64) -> bool {
    read_at != Some(version) || matches!(plan, Some(None))
}

/// The newest version this window knows of a ticket (its read and its card).
pub fn newest_version(detail: Option<&TicketDetail>, view: Option<&BoardView>, id: TicketId) -> Option<u64> {
    let card = view.and_then(|v| v.columns.iter().flat_map(|c| c.tickets.iter()).find(|t| t.id == id)).map(|t| t.version);
    detail.filter(|d| d.ticket.id == id).map(|d| d.ticket.version).max(card)
}

/// The version a ticket write is checked against (D-m1): edits, moves, the owner
/// and Start, so changes from two devices do not overwrite each other (§7.2).
/// The decisions (Done, Send back, the plan, Back to Backlog) follow the column the server has.
pub fn expected_version_for(command: &Command, version: Option<u64>) -> Option<u64> {
    match command {
        Command::UpdateTicket(_) | Command::MoveTicket(_) | Command::AssignTicket(_) | Command::StartTicket(_) => version,
        _ => None,
    }
}

/// An update that changes nothing but the ticket it names.
pub fn empty_update(ticket_id: TicketId) -> ticket::UpdateTicket {
    ticket::UpdateTicket { confirm_secret: false, ticket_id, title: None, description: None, priority: None, labels: None, deadline_ms: None, clear_deadline: false, run: None, add_files: None }
}

/// The files pasted while editing a ticket's title or description, as `add_files`:
/// left out when there are none, so an update without files still goes to an
/// older server (ticket #3).
pub fn pasted_add_files(files: Vec<(std::path::PathBuf, u64)>) -> Option<Vec<String>> {
    Some(files.into_iter().map(|(p, _)| p.display().to_string()).collect::<Vec<_>>()).filter(|f| !f.is_empty())
}

// ---- Behaviour --------------------------------------------------------------

impl AppView {
    pub fn board_online(&self) -> bool {
        matches!(self.connection, backend::Connection::Online { .. })
    }

    /// The project whose page shows its Board tab: the board there is that
    /// project's only, whatever the Board page's own filter is.
    pub fn project_board(&self) -> Option<ProjectId> {
        board_scope(self.screen, None, |_| true)
    }

    /// The project filter in use: the project page's, else the Board page's;
    /// none when its project is gone.
    pub fn board_filter(&self) -> Option<ProjectId> {
        board_scope(self.screen, self.board.project, |p| self.projects.iter().any(|x| x.id == p))
    }

    /// Ask the server for the board under the current filter.
    pub fn board_load(&mut self) {
        let project = self.board_filter();
        if !load_now(self.board.asked, project) {
            self.board.again = true;
            return;
        }
        self.board.asked = Some(project);
        self.board.again = false;
        let _ = self.requests.send(backend::Request::Board(BoardCall::Load(project)));
    }

    pub(crate) fn board_send(&mut self, command: Command, done: Option<String>) {
        let mut expected_version = None;
        if let Some(ticket) = backend::ticket_of(&command) {
            self.board.busy.insert(ticket);
            self.board.own.insert(ticket);
            expected_version = expected_version_for(&command, newest_version(self.board.detail_of(ticket), self.board.view.as_ref(), ticket));
        }
        let _ = self.requests.send(backend::Request::Board(BoardCall::Write { command, expected_version, done }));
    }

    /// The connection went: a write on its way will not answer (the backend says so too).
    pub fn board_connection(&mut self, connection: &backend::Connection, cx: &mut Context<Self>) {
        let online = matches!(connection, backend::Connection::Online { .. });
        // Reads on their way are not answered: read again once back.
        if !online && self.board.asked.take().is_some() {
            self.board.again = false;
            self.board.stale = true;
        }
        if !online && (self.board.form.busy || !self.board.busy.is_empty()) {
            self.board.form.busy = false;
            self.board.busy.clear();
            cx.notify();
        }
    }

    /// Whether the board or a ticket shows (and so reloads on a change).
    pub fn board_watched(&self) -> bool {
        self.screen == crate::app::Page::Board || self.project_board().is_some() || matches!(self.panel, Panel::Ticket(_)) || self.board.dialog.is_some()
    }

    /// Read again the board and the open ticket.
    pub fn board_refresh(&mut self) {
        self.board_load();
        if let Panel::Ticket(id) = self.panel {
            let _ = self.requests.send(backend::Request::Board(BoardCall::Ticket(id)));
            if self.board.tab == TicketTab::Activity {
                let _ = self.requests.send(backend::Request::Board(BoardCall::Activity(id)));
            }
            self.ticket_read_followers(id);
        }
        if let Some(id) = self.board_dialog_reads() {
            let _ = self.requests.send(backend::Request::Board(BoardCall::Ticket(id)));
        }
    }

    /// The ticket a dialog reads for itself (Plan, Mark as Done): not the one in the panel.
    fn board_dialog_reads(&self) -> Option<TicketId> {
        let panel = match self.panel {
            Panel::Ticket(id) => Some(id),
            _ => None,
        };
        dialog_reads(self.board.dialog, panel)
    }

    /// Show a tab of the open ticket; Activity is read when it shows.
    pub fn ticket_tab(&mut self, tab: TicketTab, cx: &mut Context<Self>) {
        self.board.tab = tab;
        if let (TicketTab::Activity, Panel::Ticket(id)) = (tab, self.panel) {
            let _ = self.requests.send(backend::Request::Board(BoardCall::Activity(id)));
        }
        if let (TicketTab::Discussion, Panel::Ticket(id)) = (tab, self.panel) {
            self.discussion_mark_seen(id);
        }
        cx.notify();
    }

    pub fn apply_board(&mut self, update: BoardUpdate, cx: &mut Context<Self>) {
        match update {
            BoardUpdate::Loaded(project, result) => {
                if self.board.asked.is_some_and(|a| a != project) {
                    return; // an older filter's answer
                }
                self.board.asked = None;
                match result {
                    Ok(view) => {
                        self.board_read_gave_up(&view);
                        if let Some(old) = self.board.view.as_ref().filter(|old| old.project_id == view.project_id) {
                            let moves = moved_elsewhere(old, &view, &self.board.own);
                            self.board_tell_moves(moves, cx);
                        }
                        self.board.own.clear();
                        self.board.view = Some(view);
                        self.board.error = None;
                        self.board.columns_reload.loaded();
                    }
                    Err(e) => self.board.error = Some(e),
                }
                // Changes came in while it was on its way: one more read.
                if std::mem::take(&mut self.board.again) {
                    self.board_load();
                }
            }
            BoardUpdate::Ticket(id, result) => {
                // The open ticket channel's header shows its title and assignees (AUR-CHAT-001).
                if let Ok(detail) = &result {
                    self.channel_ticket_loaded(detail);
                }
                let panel = self.panel == Panel::Ticket(id);
                if !panel && self.board_dialog_reads() != Some(id) {
                    return;
                }
                match result {
                    Ok(detail) => {
                        // A plan to review: read it from the main task's conversation.
                        // Only for a new version or conversation, or while none was found (D-m3).
                        if detail.ticket.column == ColumnKind::PlanReview
                            && let Some(conversation) = detail.root_task.as_ref().and_then(|t| t.conversation_id)
                            && plan_to_read(self.board.plans_read.get(&conversation).copied(), self.board.plans.get(&conversation), detail.ticket.version)
                        {
                            if self.board.plans_read.insert(conversation, detail.ticket.version).is_some_and(|v| v != detail.ticket.version) {
                                self.board.plans.remove(&conversation);
                            }
                            let _ = self.requests.send(backend::Request::Board(BoardCall::Plan(conversation)));
                        }
                        // D-07: after the plan, which mode the conversation went on in (read once per version).
                        if detail.ticket.run.mode == Some(aurelius_domain::autonomy::AgentMode::Plan)
                            && matches!(detail.ticket.column, ColumnKind::Doing | ColumnKind::Review | ColumnKind::Done)
                            && let Some(conversation) = detail.root_task.as_ref().and_then(|t| t.conversation_id)
                            && self.board.plans_read.insert(conversation, detail.ticket.version) != Some(detail.ticket.version)
                        {
                            let _ = self.requests.send(backend::Request::Board(BoardCall::Plan(conversation)));
                        }
                        // "Sent back" or the plan's note (Pulse m2): the Doing bar reads the activity too.
                        let known = self.board.detail.as_ref().filter(|d| d.ticket.id == id).map(|d| d.ticket.version);
                        if panel
                            && detail.ticket.column == ColumnKind::Doing
                            && detail.ticket.review_feedback.is_some()
                            && self.board.tab != TicketTab::Activity
                            && (known != Some(detail.ticket.version) || self.ticket_activity(id).is_none())
                        {
                            let _ = self.requests.send(backend::Request::Board(BoardCall::Activity(id)));
                        }
                        // Its discussion follows the ticket (read again with each change: moves write lines there).
                        if panel {
                            self.discussion_follow(detail.ticket.conversation_id);
                        }
                        // A dialog's ticket does not replace the one in the panel.
                        if panel {
                            self.board.detail = Some(detail);
                            self.board.detail_error = None;
                        } else {
                            self.board.dialog_detail = Some(detail);
                            self.board.dialog_error = None;
                        }
                    }
                    Err(e) if panel => self.board.detail_error = Some(e),
                    Err(e) => self.board.dialog_error = Some(e),
                }
            }
            BoardUpdate::Plan(conversation, text, mode) => {
                self.board.plans.insert(conversation, text);
                if let Some(mode) = mode {
                    self.board.modes.insert(conversation, mode);
                }
            }
            BoardUpdate::Activity(id, result) => {
                if self.panel == Panel::Ticket(id) {
                    self.board.activity = Some((id, result));
                }
            }
            BoardUpdate::Discussion(conversation, result) => self.discussion_loaded(conversation, result),
            BoardUpdate::Followers(id, result) => {
                self.board.followers.insert(id, result.map(|v| v.followers));
            }
            BoardUpdate::FollowChanged => self.followers_changed(),
            BoardUpdate::Located(result) => self.link_located(result, cx),
            BoardUpdate::Posted { ticket, to } => self.discussion_posted(ticket, to, cx),
            BoardUpdate::PostFailed { ticket, message } => {
                self.board.posting = false;
                if self.board.composer_for == Some(ticket) {
                    self.board.post_error = Some(message.clone());
                }
                self.board_announce(message, cx);
            }
            BoardUpdate::SummaryRetried { ticket, failure } => {
                self.board.retrying = false;
                let number = self.board.ticket(ticket).map_or(0, |t| t.number);
                match failure {
                    None => {
                        self.board.retry_error = None;
                        self.notify_user(format!("#{number}: the summary is being made again."), false, cx);
                    }
                    Some(why) => {
                        self.board.retry_error = Some(why.clone());
                        self.board_announce(why, cx);
                    }
                }
            }
            BoardUpdate::GaveUp { ticket, version, gave_up } => {
                self.board.gave_up_asked.remove(&(ticket, version));
                self.board.gave_up.insert(ticket, (version, gave_up));
                // Unknown: Needs you (the board member chooses), and read again later (as mobile).
                if gave_up.is_none() {
                    cx.spawn(async move |this, cx| {
                        cx.background_executor().timer(std::time::Duration::from_millis(GAVE_UP_RETRY_MS)).await;
                        let _ = this.update(cx, |this, cx| {
                            if forget_unknown(&mut this.board.gave_up, ticket, version)
                                && let Some(view) = this.board.view.clone()
                            {
                                this.board_read_gave_up(&view);
                                cx.notify();
                            }
                        });
                    })
                    .detach();
                }
            }
            BoardUpdate::Done { ticket, text } => {
                if let Some(t) = ticket {
                    self.board.busy.remove(&t);
                }
                if let Some(text) = text {
                    self.board_announce(text.clone(), cx);
                    self.notify_user(text, false, cx);
                }
                self.board_refresh();
            }
            BoardUpdate::Created { number, text } => {
                self.board.form.busy = false;
                if self.board.dialog == Some(BoardDialog::NewTicket) {
                    self.board.dialog = None;
                }
                self.board_form_clear();
                self.notify_user(text.unwrap_or_else(|| format!("#{number} created in Backlog.")), false, cx);
                self.board_refresh();
            }
            BoardUpdate::Failed { ticket, message } => {
                match ticket {
                    Some(t) => {
                        self.board.busy.remove(&t);
                    }
                    None => {
                        if self.board.form.busy {
                            self.board.form.busy = false;
                            self.board.form.error = Some(message.clone());
                        }
                        if self.board.dialog == Some(BoardDialog::Columns) {
                            self.board.columns_error = Some(message.clone());
                            self.board.columns_reload.failed(&message);
                        }
                    }
                }
                // The card springs back: show what the server has.
                self.board_announce(message.clone(), cx);
                self.notify_user(message, true, cx);
                if self.board_online() {
                    self.board_refresh();
                }
            }
            BoardUpdate::Changed { tickets, settings, tasks } => {
                // A task matters only when it works for a ticket (D-m3); a task this window does not know yet may.
                let on_ticket = |id: aurelius_domain::TaskId| self.tasks.iter().find(|t| t.id == id).map(|t| t.ticket_id.is_some());
                if !board_changed(tickets, settings, &tasks, on_ticket) {
                    return;
                }
                if self.board_watched() {
                    self.board_refresh();
                } else if tickets {
                    // The sidebar's "Needs you" follows the cards.
                    self.board_load();
                } else {
                    // A task's status shows on its card: read it when the board shows again.
                    self.board.stale = true;
                }
            }
        }
        cx.notify();
    }

    /// The dispatcher is choosing this ticket's owner: the project has an active
    /// dispatcher and the ticket's timeline does not say it gave up (B5, as mobile).
    pub fn ticket_choosing(&self, t: &TicketView) -> bool {
        choosing(t, self.dispatcher_for(t.project_id).name().is_some(), dispatcher_stopped(self.board.gave_up.get(&t.id)))
    }

    /// The open ticket's activity, when it loaded.
    pub fn ticket_activity(&self, id: TicketId) -> Option<&[TimelineCard]> {
        match &self.board.activity {
            Some((t, Ok(cards))) if *t == id => Some(cards.as_slice()),
            _ => None,
        }
    }

    /// This Todo ticket's timeline did not load: whether the dispatcher chose is unknown.
    pub fn ticket_owner_unknown(&self, t: &TicketView) -> bool {
        owner_unknown(self.board.gave_up.get(&t.id).filter(|(v, _)| *v == t.version))
    }

    /// Tell the screen reader something now (§15).
    pub fn board_announce(&mut self, text: String, cx: &mut Context<Self>) {
        let n = self.board.announce.as_ref().map_or(0, |(n, _)| n + 1);
        self.board.announce = Some((n, text));
        cx.notify();
    }

    /// Others' moves: told at once, or bundled into the next turn of `ANNOUNCE_EVERY_MS`.
    fn board_tell_moves(&mut self, moves: Vec<(u32, ColumnKind, Option<String>)>, cx: &mut Context<Self>) {
        if moves.is_empty() {
            return;
        }
        let waiting = !self.board.pending_moves.is_empty();
        self.board.pending_moves.extend(moves);
        let wait = self.board.announced_at + ANNOUNCE_EVERY_MS - ui::now_ms();
        if wait <= 0 {
            self.board_flush_moves(cx);
        } else if !waiting {
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(std::time::Duration::from_millis(wait as u64)).await;
                let _ = this.update(cx, |this, cx| this.board_flush_moves(cx));
            })
            .detach();
        }
    }

    fn board_flush_moves(&mut self, cx: &mut Context<Self>) {
        let moves = std::mem::take(&mut self.board.pending_moves);
        if let Some(text) = moves_announcement(&moves, |k| self.board.column_name(k)) {
            self.board.announced_at = ui::now_ms();
            self.board_announce(text, cx);
        }
    }

    /// The cards as the Board shows them now (filters included), for the keyboard.
    pub fn board_grid(&self, cx: &App) -> Grid {
        let Some(view) = &self.board.view else { return Vec::new() };
        let query = self.board.find.read(cx).value().to_string();
        let testing = self.board_testing_supported();
        view.columns
            .iter()
            .filter(|column| column_shown(column.column.kind, testing))
            .map(|column| {
                let (cards, _) = shown_cards(column, self.board.owner, self.board.creator.user(self.active_member().map(|u| u.id)), self.board_responsible_wanted(), &query, |t| self.ticket_needs_me(t), self.board.needs_only, self.board.done_all);
                (column.column.kind, cards.into_iter().map(|t| t.id).collect())
            })
            .collect()
    }

    /// An arrow on the Board: the next card, or the held card's next place.
    pub fn board_step(&mut self, step: Step, cx: &mut Context<Self>) {
        if !board_keys_live(self.board.dialog, self.overlay) {
            return;
        }
        let grid = self.board_grid(cx);
        if let Some(held) = self.board.held {
            let held = held_step(&grid, held, step);
            self.board.held = Some(held);
            if let Some((kind, _)) = held_target(&grid, &held) {
                let text = format!("#{} to {}, place {} of {}. Enter puts it down, Escape cancels.", held.number, self.board.column_name(kind), held.at + 1, others(&grid, &held, held.to) + 1);
                self.board_announce(text, cx);
            }
            cx.notify();
            return;
        }
        self.board.focused = focus_step(&grid, self.board.focused.filter(|id| find_in(&grid, *id).is_some()), step);
        if let Some(id) = self.board.focused {
            self.ticket_know_followers(id);
        }
        cx.notify();
    }

    /// The card the keyboard is on, if the board still shows it.
    fn board_focused(&self, cx: &App) -> Option<TicketView> {
        let id = self.board.focused?;
        find_in(&self.board_grid(cx), id)?;
        self.board.ticket(id).cloned()
    }

    /// Enter: open the focused card, or put the held one down (§6 decides; a dialog may follow).
    pub fn board_key_open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !board_keys_live(self.board.dialog, self.overlay) {
            return;
        }
        if let Some(held) = self.board.held.take() {
            let grid = self.board_grid(cx);
            if let Some((to, onto)) = held_target(&grid, &held) {
                if !self.board_online() {
                    self.notify_user("Offline · reconnecting", true, cx);
                } else if self.board.ticket(held.id).map(|t| t.column) != Some(held.from) {
                    self.notify_user(format!("#{} moved on another device.", held.number), true, cx);
                } else {
                    if let Drop::Refused(why) = drop_effect(held.from, to, held.choosing) {
                        self.board_announce(format!("#{} cannot go to {}: {why}", held.number, self.board.column_name(to)), cx);
                    }
                    self.board_apply(held.id, held.from, to, onto, held.choosing, window, cx);
                }
            }
            cx.notify();
            return;
        }
        if let Some(t) = self.board_focused(cx) {
            self.open_ticket(t.id, window, cx);
        }
    }

    /// S: Start, only on a Backlog card (§15).
    pub fn board_key_start(&mut self, cx: &mut Context<Self>) {
        let live = board_keys_live(self.board.dialog, self.overlay);
        let Some(t) = self.board_focused(cx).filter(|_| live && self.board.held.is_none()) else { return };
        if t.column != ColumnKind::Backlog {
            self.notify_user("Start works on a Backlog card.", false, cx);
        } else if self.board_online() {
            self.ticket_start(t.id, None, cx);
        }
    }

    /// M: "Move to…" with the allowed columns (§15), at the card.
    pub fn board_key_move(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let live = board_keys_live(self.board.dialog, self.overlay);
        let Some(t) = self.board_focused(cx).filter(|_| live && self.board.held.is_none()) else { return };
        let at = self.dropdown_bounds.borrow().get(FOCUSED_CARD).map_or_else(|| point(px(320.), px(200.)), |b| b.bottom_left());
        let items = self.ticket_move_menu(&t);
        self.open_menu(at, items, window, cx);
    }

    /// The card the Board's letter keys act on: the keyboard's, else the open ticket's.
    fn board_key_card(&self, cx: &App) -> Option<TicketView> {
        if !board_keys_live(self.board.dialog, self.overlay) || self.board.held.is_some() {
            return None;
        }
        let open = match self.panel {
            Panel::Ticket(id) => self.board.ticket(id).cloned(),
            _ => None,
        };
        self.board_focused(cx).or(open)
    }

    /// A: "Set responsible…" (feature `ticket_responsible`), at the card.
    pub fn board_key_responsible(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(t) = self.board_key_card(cx).filter(|_| self.ticket_responsible_supported()) else { return };
        let at = self.dropdown_bounds.borrow().get(FOCUSED_CARD).map_or_else(|| point(px(320.), px(200.)), |b| b.bottom_left());
        self.open_responsible_menu(t.id, at, window, cx);
    }

    /// F: Follow or Unfollow (feature `ticket_follow`); the Responsible hears why not.
    pub fn board_key_follow(&mut self, cx: &mut Context<Self>) {
        let Some(t) = self.board_key_card(cx).filter(|_| self.ticket_follow_supported()) else { return };
        let state = self.ticket_follow_state(&t);
        if state.locked {
            self.notify_user(crate::screens::who::follow_tip(state, None, t.number, ColumnKind::Doing), false, cx);
        } else if self.board_online() {
            self.ticket_follow(t.id, !state.following, cx);
        }
    }

    /// The palette's commands on the Board's card (AUR-FA §8), only what this server does.
    pub fn board_palette_entries(&self, cx: &App) -> Vec<crate::app::PaletteEntry> {
        if self.screen != crate::app::Page::Board || self.board_key_card(cx).is_none() {
            return Vec::new();
        }
        let entry = |label: &'static str, keys: Option<&'static str>, action: fn() -> Box<dyn Action>| crate::app::PaletteEntry { label: label.into(), hint: "Board".into(), keys, target: crate::app::PaletteTarget::Action(action) };
        let mut entries = vec![entry("Copy link", Some("c"), || Box::new(crate::keymap::BoardCopyLink))];
        if self.ticket_responsible_supported() {
            entries.push(entry("Set responsible…", Some("a"), || Box::new(crate::keymap::BoardResponsible)));
            entries.push(entry("Make me responsible", None, || Box::new(crate::keymap::BoardMakeMeResponsible)));
        }
        if self.ticket_follow_supported() {
            entries.push(entry("Follow ticket", Some("f"), || Box::new(crate::keymap::BoardFollow)));
        }
        entries
    }

    /// "Make me responsible" from the palette.
    pub fn board_key_make_me_responsible(&mut self, cx: &mut Context<Self>) {
        let me = self.active_member().map(|u| u.id);
        let Some(t) = self.board_key_card(cx).filter(|t| self.ticket_responsible_supported() && me.is_some() && crate::screens::who::responsible_editable(t)) else { return };
        if self.board_online() {
            self.ticket_set_responsible(t.id, me, cx);
        }
    }

    /// C: Copy link.
    pub fn board_key_copy_link(&mut self, cx: &mut Context<Self>) {
        if let Some(t) = self.board_key_card(cx) {
            self.copy_ticket_link(t.id, cx);
        }
    }

    /// Space: pick the focused card up (not in Done), or put it back where it was.
    pub fn board_key_pick(&mut self, cx: &mut Context<Self>) {
        if !board_keys_live(self.board.dialog, self.overlay) {
            return;
        }
        if let Some(held) = self.board.held.take() {
            self.board_announce(format!("#{} stays where it was.", held.number), cx);
            return;
        }
        let Some(t) = self.board_focused(cx) else { return };
        if t.column == ColumnKind::Done || !self.board_online() {
            self.notify_user(if t.column == ColumnKind::Done { "Done tickets are closed." } else { "Offline · reconnecting" }, true, cx);
            return;
        }
        let choosing = self.ticket_choosing(&t);
        self.board.held = pick_up(&self.board_grid(cx), t.id, t.number, choosing);
        self.board_announce(format!("Picked up #{}. Arrows choose the column and place, Enter puts it down, Escape cancels.", t.number), cx);
    }

    /// Esc while a card is held: it stays where it was. Whether Esc was used.
    pub fn board_cancel_hold(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(held) = self.board.held.take() else { return false };
        self.board_announce(format!("#{} stays where it was.", held.number), cx);
        true
    }

    /// Ask the timeline of each Todo ticket without an owner that changed since it was read.
    fn board_read_gave_up(&mut self, view: &BoardView) {
        let ask: Vec<(TicketId, u64)> = gave_up_to_read(view, &self.board.gave_up).filter(|k| !self.board.gave_up_asked.contains(k)).collect();
        for (ticket, version) in ask {
            self.board.gave_up_asked.insert((ticket, version));
            let _ = self.requests.send(backend::Request::Board(BoardCall::GaveUp { ticket, version }));
        }
    }

    pub fn ticket_needs(&self, t: &TicketView) -> Option<Needs> {
        // Needs you is per board member (ui-ontwerp §3): a missing login only for its own.
        if t.needs == Some(TicketNeed::Login) && matches!(t.column, ColumnKind::Todo | ColumnKind::Doing) {
            return (self.active_member().map(|u| u.id) == Some(t.login_user)).then_some(Needs::ConnectLogin);
        }
        let approvals = t.root_task_id.and_then(|r| self.tasks.iter().find(|x| x.id == r)).map_or(0, |r| r.pending_approvals);
        needs_you(t, self.ticket_choosing(t), approvals)
    }

    /// Whether a ticket's need counts for the board member here (B6): ◆, the filter, the badge
    /// and Waiting on me; `ticket_needs` stays what the card shows.
    pub fn ticket_needs_me(&self, t: &TicketView) -> bool {
        needs_me(self.ticket_needs(t), t, self.active_member().map(|u| u.id))
    }

    /// Whether the board member whose login a ticket runs on is disabled (MU3-12 AC4).
    pub fn login_disabled(&self, t: &TicketView) -> bool {
        self.members.list.iter().any(|u| u.id == t.login_user && u.disabled_at_ms.is_some())
    }

    /// A ticket's wait, by whose login (ui-ontwerp §5): its Claude login, else its quota.
    pub fn ticket_wait(&self, t: &TicketView) -> Option<String> {
        let me = self.active_member().map(|u| u.id);
        if let Some(text) = login_wait(t, me, self.login_disabled(t)) {
            return Some(text);
        }
        let root = t.root_task_id.and_then(|r| self.tasks.iter().find(|x| x.id == r))?;
        match self.task_mark(root) {
            crate::ui::TaskMark::LimitReset(until) => Some(format!("⏸ {}", crate::ui::limit_wait_for(&login_owner(t, me), until))),
            _ => None,
        }
    }

    /// Run on my login (B4): after its confirmation.
    pub fn ticket_use_my_login(&mut self, id: TicketId, cx: &mut Context<Self>) {
        let number = self.board.ticket(id).map_or(0, |t| t.number);
        self.board_send(Command::UseMyLogin(ticket::UseMyLogin { ticket_id: id }), Some(format!("#{number} now runs on your Claude login.")));
        cx.notify();
    }

    /// Tickets that need this board member now: the sidebar badge and the board's counter (B6).
    pub fn board_needs_count(&self) -> usize {
        let Some(view) = &self.board.view else { return 0 };
        view.columns.iter().flat_map(|c| c.tickets.iter()).filter(|t| self.ticket_needs_me(t)).count()
    }

    pub fn employee_name(&self, id: EmployeeId) -> Option<String> {
        self.managers.iter().find(|m| m.id == id).map(|m| m.name.clone())
    }

    /// The company's tester (feature `board_testing`); none: no Testing step.
    pub fn company_tester(&self) -> Option<EmployeeId> {
        self.company.as_ref().and_then(|c| c.company.tester_employee_id)
    }

    /// Who tests a ticket in Testing, by name, and its test task among `tasks`
    /// when it is there (ticket #5).
    pub fn ticket_testing<'a>(&self, t: &TicketView, tasks: impl IntoIterator<Item = &'a aurelius_protocol::TaskView>) -> (Option<String>, Option<&'a aurelius_protocol::TaskView>) {
        let tester = self.company_tester();
        let task = test_task(tasks, t.id, tester);
        let name = task.and_then(|x| x.employee_name.clone()).or_else(|| tester.and_then(|e| self.employee_name(e)));
        (name, task)
    }

    /// The dispatcher of a project's started tickets.
    pub fn dispatcher_for(&self, project: ProjectId) -> Dispatcher {
        let own = self.projects.iter().find(|p| p.id == project).and_then(|p| p.dispatcher_employee_id);
        let company = self.company.as_ref().and_then(|c| c.company.dispatcher_employee_id);
        dispatcher_of(own, company, &self.managers)
    }

    pub fn set_board_project(&mut self, project: Option<ProjectId>, cx: &mut Context<Self>) {
        self.board.project = project;
        crate::app::set_ui_value(PREF_PROJECT, project.map_or(serde_json::Value::Null, |p| serde_json::Value::String(p.to_string())));
        self.board.view = None;
        self.board_load();
        cx.notify();
    }

    pub fn open_ticket(&mut self, id: TicketId, window: &mut Window, cx: &mut Context<Self>) {
        if self.panel != Panel::Ticket(id) {
            self.board.detail = None;
            self.board.detail_error = None;
            self.board.activity = None;
            self.board.editing = None;
            self.board.edit_error = None;
            self.board.edit_files.clear();
            self.board.followers_open = false;
        }
        self.ticket_read_followers(id);
        if self.board.feedback_for != Some(id) {
            self.board.feedback_for = Some(id);
            self.board.feedback.update(cx, |f, cx| f.set_value("", window, cx));
        }
        self.board.back_to = None;
        let _ = self.requests.send(backend::Request::Board(BoardCall::Ticket(id)));
        if self.board.tab == TicketTab::Activity {
            let _ = self.requests.send(backend::Request::Board(BoardCall::Activity(id)));
        }
        self.open_panel(Panel::Ticket(id), window, cx);
    }

    /// A task of the open ticket in the panel, with "‹ #42" back to the ticket (MU1-08 AC4).
    pub fn open_ticket_task(&mut self, task: aurelius_domain::TaskId, window: &mut Window, cx: &mut Context<Self>) {
        let back = match self.panel {
            Panel::Ticket(id) => Some(id),
            _ => None,
        };
        self.open_panel(Panel::Task(task), window, cx);
        self.board.back_to = back;
    }

    /// Start (Backlog → Todo, MU1-09), with an employee (MU1-12) or the dispatcher; also
    /// "Start again" after a failed main task (B6).
    pub fn ticket_start(&mut self, id: TicketId, employee: Option<EmployeeId>, cx: &mut Context<Self>) {
        let Some(t) = self.board.ticket(id).cloned() else { return };
        let who = employee.or(t.owner_employee_id).and_then(|e| self.employee_name(e));
        let done = match who {
            Some(name) => format!("#{} started with {name}.", t.number),
            None => {
                let dispatcher = self.dispatcher_for(t.project_id);
                match (dispatcher.name(), dispatcher.why_nobody()) {
                    (Some(d), _) => format!("#{} started. {d} chooses an employee.", t.number),
                    (None, why) => format!("#{} started. {}: choose an employee.", t.number, why.unwrap_or_default()),
                }
            }
        };
        // P-36: Start without the own Claude login is allowed, and says what follows.
        let mine = self.active_member().map(|u| u.id) == Some(t.login_user);
        let done = start_toast(t.number, done, mine && crate::screens::account::needs_connect(self.claude.view.as_ref(), self.claude.unsupported.is_some()));
        self.board_send(Command::StartTicket(ticket::StartTicket { ticket_id: id, employee_id: employee }), Some(done));
        cx.notify();
    }

    /// Back to Backlog (`ticket.cancel`): the main task and its delegations stop.
    pub fn ticket_cancel(&mut self, id: TicketId, cx: &mut Context<Self>) {
        let number = self.board.ticket(id).map_or(0, |t| t.number);
        self.board_send(Command::CancelTicket(ticket::CancelTicket { ticket_id: id }), Some(format!("#{number} is back in Backlog.")));
        cx.notify();
    }

    /// The owner (`ticket.assign`): free in Backlog, also back to the dispatcher there;
    /// in Todo "Choose employee" overrides the dispatcher (MU1-12 AC2).
    pub fn ticket_assign(&mut self, id: TicketId, employee: Option<EmployeeId>, cx: &mut Context<Self>) {
        let Some(t) = self.board.ticket(id).cloned() else { return };
        let done = match employee.and_then(|e| self.employee_name(e)) {
            Some(name) if t.column == ColumnKind::Todo => format!("#{} goes to {name}.", t.number),
            Some(name) => format!("Owner of #{}: {name}.", t.number),
            None => format!("#{}: the dispatcher chooses when it starts.", t.number),
        };
        self.board_send(Command::AssignTicket(ticket::AssignTicket { ticket_id: id, employee_id: employee, reason: None }), Some(done));
        cx.notify();
    }

    pub fn ticket_update(&mut self, update: ticket::UpdateTicket, cx: &mut Context<Self>) {
        self.board_send(Command::UpdateTicket(update), None);
        cx.notify();
    }

    /// A card dropped on a column (`onto`: the card it was dropped on). §6.
    pub fn board_drop(&mut self, card: &DraggedCard, to: ColumnKind, onto: Option<TicketId>, window: &mut Window, cx: &mut Context<Self>) {
        if !self.board_online() {
            self.notify_user("Offline · reconnecting", true, cx);
            return;
        }
        // The card changed on another device while it was dragged.
        let now = self.board.ticket(card.id).map(|t| t.column);
        if let Some(now) = now.filter(|c| *c != card.from) {
            let name = self.board.column_name(now);
            self.notify_user(format!("#{} moved to {name} on another device.", card.number), true, cx);
            return;
        }
        self.board_apply(card.id, card.from, to, onto, card.choosing, window, cx);
    }

    #[allow(clippy::too_many_arguments)]
    fn board_apply(&mut self, id: TicketId, from: ColumnKind, to: ColumnKind, onto: Option<TicketId>, choosing: bool, window: &mut Window, cx: &mut Context<Self>) {
        match drop_effect(from, to, choosing) {
            Drop::Refused(why) => self.notify_user(why, true, cx),
            Drop::Reorder => {
                let Some(column) = self.board.view.as_ref().and_then(|v| v.columns.iter().find(|c| c.column.kind == to)).cloned() else { return };
                if let Some(before) = before_for(&column.tickets, id, onto) {
                    self.board_send(Command::MoveTicket(ticket::MoveTicket { ticket_id: id, column_id: column.column.id, before_ticket_id: before }), None);
                }
            }
            Drop::Start => self.ticket_start(id, None, cx),
            Drop::BackToBacklog { confirm: false } => self.ticket_cancel(id, cx),
            Drop::BackToBacklog { confirm: true } => self.board_dialog(BoardDialog::BackToBacklog(id), window, cx),
            Drop::Reject => self.board_dialog(BoardDialog::Reject(id), window, cx),
            Drop::PlanDialog => self.board_dialog(BoardDialog::Plan(id), window, cx),
            Drop::SendBack => self.board_dialog(BoardDialog::SendBack(id), window, cx),
            Drop::MarkDone => self.board_dialog(BoardDialog::MarkDone(id), window, cx),
        }
        cx.notify();
    }

    pub fn board_dialog(&mut self, dialog: BoardDialog, window: &mut Window, cx: &mut Context<Self>) {
        if let BoardDialog::SendBack(id) | BoardDialog::Plan(id) = dialog
            && self.board.feedback_for != Some(id)
        {
            self.board.feedback_for = Some(id);
            self.board.feedback.update(cx, |f, cx| f.set_value("", window, cx));
        }
        // The plan and Mark as Done (also after a drag) need the ticket's tasks:
        // read them, unless the panel shows that ticket.
        self.board.dialog_detail = None;
        self.board.dialog_error = None;
        let panel = match self.panel {
            Panel::Ticket(id) => Some(id),
            _ => None,
        };
        if let Some(id) = dialog_reads(Some(dialog), panel) {
            let _ = self.requests.send(backend::Request::Board(BoardCall::Ticket(id)));
        }
        self.board.feedback_missing = false;
        if dialog == BoardDialog::Columns {
            self.board.columns_reload = ColumnsReload::Idle;
            self.board_columns_begin(window, cx);
        }
        self.board.dialog = Some(dialog);
        match dialog {
            BoardDialog::NewTicket => self.board.form.title.update(cx, |i, cx| i.focus(window, cx)),
            BoardDialog::SendBack(_) | BoardDialog::Plan(_) => self.board.feedback.update(cx, |i, cx| i.focus(window, cx)),
            _ => self.overlay_focus.focus(window, cx),
        }
        cx.notify();
    }

    pub fn board_close_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.board.dialog = None;
        self.board.dialog_detail = None;
        self.focus.focus(window, cx);
        cx.notify();
    }

    /// The columns a card may go to with "Move to…" (the others with their reason).
    pub fn ticket_move_menu(&self, t: &TicketView) -> MenuItems {
        let choosing = self.ticket_choosing(t);
        let testing = self.board_testing_supported();
        ColumnKind::ALL
            .into_iter()
            .filter(|k| *k != t.column && column_shown(*k, testing))
            .map(|k| {
                let name = format!("Move to {}", self.board.column_name(k));
                match drop_effect(t.column, k, choosing) {
                    Drop::Refused(why) => refused_entry(name, why),
                    effect => {
                        let detail = match effect {
                            Drop::Start => "Start",
                            Drop::BackToBacklog { .. } => "Stops the work",
                            Drop::Reject => "Afwijzen",
                            Drop::PlanDialog => "Review the plan",
                            Drop::SendBack => "With feedback",
                            Drop::MarkDone => "Closes it",
                            _ => "",
                        };
                        menu_entry(name, BoardDo::Move(t.id, k), false, Some(detail.to_string()).filter(|d| !d.is_empty()))
                    }
                }
            })
            .collect()
    }

    /// The card's menu (right-click): §3.4.
    pub fn card_menu(&self, t: &TicketView) -> MenuItems {
        let mut items: MenuItems = vec![menu_entry("Open", BoardDo::Open(t.id), false, None)];
        if t.column == ColumnKind::Backlog {
            items.push(menu_entry("Start", BoardDo::Start(t.id, None), false, None));
            items.extend(self.employee_choices().into_iter().map(|(id, name)| menu_entry(format!("Start with {name}"), BoardDo::Start(t.id, Some(id)), false, None)));
        }
        items.push(menu_entry("Copy link", BoardDo::CopyLink(t.id), false, None));
        if self.ticket_follow_supported() {
            let state = self.ticket_follow_state(t);
            if !state.following {
                items.push(menu_entry("Follow", BoardDo::Follow(t.id, true), false, None));
            } else if !state.locked {
                items.push(menu_entry("Unfollow", BoardDo::Follow(t.id, false), false, None));
            }
        }
        if self.ticket_responsible_supported() && crate::screens::who::responsible_editable(t) {
            let me = self.active_member().map(|u| u.id);
            if me.is_some() && t.responsible_user_id == me {
                items.push(menu_entry("Remove me as responsible", BoardDo::Responsible(t.id, None), false, None));
            } else if me.is_some() {
                items.push(menu_entry("Make me responsible", BoardDo::Responsible(t.id, me), false, None));
            }
            items.push(menu_entry("Set responsible…", BoardDo::ResponsibleMenu(t.id), false, None));
        }
        items.push(None);
        items.extend(self.ticket_move_menu(t));
        if t.column == ColumnKind::Backlog {
            items.push(None);
            items.push(crate::app::danger(menu_entry("Delete", BoardDo::Dialog(BoardDialog::Delete(t.id)), false, None)));
        }
        items
    }

    /// Active employees as the org chart orders them (MU1-12 AC3).
    pub fn employee_choices(&self) -> Vec<(EmployeeId, String)> {
        crate::screens::dashboard::assignee_order(&self.managers).into_iter().map(|m| (m.id, m.name.clone())).collect()
    }

    pub fn board_do(&mut self, run: BoardDo, window: &mut Window, cx: &mut Context<Self>) {
        match run {
            BoardDo::Open(id) => self.open_ticket(id, window, cx),
            BoardDo::Start(id, employee) => self.ticket_start(id, employee, cx),
            BoardDo::Move(id, to) => {
                if let Some(t) = self.board.ticket(id).cloned() {
                    let choosing = self.ticket_choosing(&t);
                    self.board_apply(id, t.column, to, None, choosing, window, cx);
                }
            }
            BoardDo::Owner(id, employee) => self.ticket_assign(id, employee, cx),
            BoardDo::Priority(id, priority) => self.ticket_update(ticket::UpdateTicket { priority: Some(priority), ..empty_update(id) }, cx),
            BoardDo::Mode(id, mode) => {
                let run = self.board.ticket(id).map(|t| aurelius_domain::task::RunSettings { mode, ..t.run.clone() }).unwrap_or_default();
                self.ticket_update(ticket::UpdateTicket { run: Some(run), ..empty_update(id) }, cx)
            }
            BoardDo::Model(id, model) => {
                let run = self
                    .board
                    .ticket(id)
                    .map(|t| aurelius_domain::task::RunSettings { connection_id: model.as_ref().map(|(c, _)| *c), model: model.map(|(_, m)| m), ..t.run.clone() })
                    .unwrap_or_default();
                self.ticket_update(ticket::UpdateTicket { run: Some(run), ..empty_update(id) }, cx)
            }
            BoardDo::CopyLink(id) => self.copy_ticket_link(id, cx),
            BoardDo::Responsible(id, user) => self.ticket_set_responsible(id, user, cx),
            BoardDo::ResponsibleMenu(id) => {
                let at = window.mouse_position();
                self.open_responsible_menu(id, at, window, cx);
            }
            BoardDo::Follow(id, on) => self.ticket_follow(id, on, cx),
            BoardDo::FilterResponsible(f) => {
                self.board.responsible = f;
                crate::app::set_ui_value(PREF_RESPONSIBLE, serde_json::Value::String(f.to_pref()));
            }
            BoardDo::FormResponsible(f) => self.board.form.responsible = f,
            BoardDo::Dialog(d) => self.board_dialog(d, window, cx),
            BoardDo::FilterProject(p) => self.set_board_project(p, cx),
            BoardDo::FilterOwner(o) => self.board.owner = o,
            BoardDo::FilterCreator(f) => self.board.creator = f,
            BoardDo::FormProject(p) => self.board.form.project = Some(p),
            BoardDo::FormPriority(p) => self.board.form.priority = p,
            BoardDo::FormOwner(o) => self.board.form.owner = o,
            BoardDo::FormMode(m) => self.board.form.run.mode = m,
            BoardDo::FormModel(m) => {
                self.board.form.run.connection_id = m.as_ref().map(|(c, _)| *c);
                self.board.form.run.model = m.map(|(_, m)| m);
            }
            BoardDo::ProjectDispatcher(project, employee) => self.set_project_dispatcher(project, employee, cx),
            BoardDo::CompanyDispatcher(employee) => self.set_company_dispatcher(employee, cx),
            BoardDo::CompanyTester(employee) => self.set_company_tester(employee, cx),
            BoardDo::TicketFromMessage(message) => self.ticket_from_message(message, window, cx),
        }
        cx.notify();
    }

    /// MU1-11: a project's own dispatcher, or the company's again (`None`).
    pub fn set_project_dispatcher(&mut self, project: ProjectId, employee: Option<EmployeeId>, cx: &mut Context<Self>) {
        let done = match employee.and_then(|e| self.employee_name(e)) {
            Some(name) => format!("{name} chooses the owners of this project's tickets."),
            None => "This project uses the company's dispatcher again.".into(),
        };
        let command = Command::UpdateProject(aurelius_domain::company::UpdateProject { project_id: project, dispatcher: Some(ticket::SetDispatcher { employee_id: employee }) });
        let _ = self.requests.send(backend::Request::Board(BoardCall::Write { command, expected_version: None, done: Some(done) }));
        cx.notify();
    }

    pub fn set_company_dispatcher(&mut self, employee: Option<EmployeeId>, cx: &mut Context<Self>) {
        let done = match employee.and_then(|e| self.employee_name(e)) {
            Some(name) => format!("{name} is the company's dispatcher."),
            None => "No company dispatcher: started tickets wait in Todo for an owner.".into(),
        };
        let command = Command::UpdateCompany(aurelius_domain::company::UpdateCompany { name: None, dispatcher: Some(ticket::SetDispatcher { employee_id: employee }), tester: None });
        let _ = self.requests.send(backend::Request::Board(BoardCall::Write { command, expected_version: None, done: Some(done) }));
        cx.notify();
    }

    /// Ticket #5: who runs the full test suite before Review (`None`: no Testing step).
    /// A refusal (an engine that cannot test yet) comes back as a toast.
    pub fn set_company_tester(&mut self, employee: Option<EmployeeId>, cx: &mut Context<Self>) {
        let done = tester_set_text(employee.and_then(|e| self.employee_name(e)).as_deref());
        let command = Command::UpdateCompany(company_tester_command(employee));
        let _ = self.requests.send(backend::Request::Board(BoardCall::Write { command, expected_version: None, done: Some(done) }));
        cx.notify();
    }

    fn board_filter_menu(&self, field: &'static str) -> MenuItems {
        if field == "board-project" {
            let mut items = vec![menu_entry("All projects", BoardDo::FilterProject(None), self.board.project.is_none(), None), None];
            items.extend(
                self.projects
                    .iter()
                    .filter(|p| p.status == aurelius_domain::project::ProjectStatus::Active)
                    .map(|p| menu_entry(p.name.clone(), BoardDo::FilterProject(Some(p.id)), self.board.project == Some(p.id), None)),
            );
            return items;
        }
        if field == "board-responsible" {
            return self.responsible_filter_menu();
        }
        if field == "board-creator" {
            let me = self.active_member().map(|u| u.id);
            let mut items = vec![
                menu_entry("Anyone", BoardDo::FilterCreator(CreatorFilter::Anyone), self.board.creator == CreatorFilter::Anyone, None),
                menu_entry("Me", BoardDo::FilterCreator(CreatorFilter::Me), self.board.creator == CreatorFilter::Me, None),
                None,
            ];
            items.extend(self.members.list.iter().filter(|u| Some(u.id) != me).map(|u| {
                let f = CreatorFilter::Member(u.id);
                menu_entry(u.username.clone(), BoardDo::FilterCreator(f), self.board.creator == f, None)
            }));
            return items;
        }
        let mut items = vec![
            menu_entry("Anyone", BoardDo::FilterOwner(OwnerFilter::Anyone), self.board.owner == OwnerFilter::Anyone, None),
            menu_entry("No owner yet", BoardDo::FilterOwner(OwnerFilter::NoOwner), self.board.owner == OwnerFilter::NoOwner, None),
            None,
        ];
        items.extend(self.employee_choices().into_iter().map(|(id, name)| menu_entry(name, BoardDo::FilterOwner(OwnerFilter::Employee(id)), self.board.owner == OwnerFilter::Employee(id), None)));
        items
    }

    pub fn board_new_ticket(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.board_online() {
            self.notify_user("Offline · reconnecting", true, cx);
            return;
        }
        if !self.board.form.busy {
            self.board.form.sent = false;
            self.board.form.error = None;
            // The board's project, else the last one used, else the first.
            let known = |p: &ProjectId| self.projects.iter().any(|x| x.id == *p);
            self.board.form.project = self.board_filter().filter(known).or(self.board.form.project.filter(known)).or_else(|| self.projects.first().map(|p| p.id));
        }
        self.board_dialog(BoardDialog::NewTicket, window, cx);
    }

    pub(crate) fn board_form_clear(&mut self) {
        let form = &mut self.board.form;
        form.from_message = None;
        form.prefill = None;
        form.sent = false;
        form.error = None;
        form.owner = None;
        form.responsible = FormResponsible::You;
        form.priority = TicketPriority::Normal;
        form.run = Default::default();
        form.files.clear();
        form.reset = true;
    }

    /// Pick files for the New ticket form (Ctrl+O or "Add files…").
    pub fn board_pick_files(&mut self, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, multiple: true, prompt: Some("Attach to the ticket".into()) });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = paths.await {
                let _ = this.update(cx, |this, cx| this.board_add_files(paths, cx));
            }
        })
        .detach();
    }

    /// Files picked or dropped on the New ticket form; the same limits as a task's.
    pub fn board_add_files(&mut self, paths: Vec<std::path::PathBuf>, cx: &mut Context<Self>) {
        if self.board.dialog != Some(BoardDialog::NewTicket) || self.board.form.busy {
            return;
        }
        let problems = crate::app::take_files(&mut self.board.form.files, paths, 0);
        if !problems.is_empty() {
            self.notify_user(problems.join(" "), true, cx);
        }
        cx.notify();
    }

    /// A paste into the New ticket title or description: a copied image (a screenshot) or
    /// copied files are attached to the ticket. Returns whether it took the paste; anything
    /// else is pasted as text.
    pub fn board_paste_files(&mut self, item: &ClipboardItem, cx: &mut Context<Self>) -> bool {
        if self.board.dialog != Some(BoardDialog::NewTicket) || self.board.form.busy {
            return false;
        }
        let (took, found) = self.pasted_files(item, |p| p.is_file(), cx);
        self.board_add_files(found, cx);
        took
    }

    /// A paste into the title or description of an open ticket being edited: a copied image or
    /// copied files wait beside the field and are added to the ticket on Save. Only when the
    /// server takes files on an existing ticket; anything else is pasted as text.
    pub fn ticket_paste_files(&mut self, item: &ClipboardItem, cx: &mut Context<Self>) -> bool {
        let Panel::Ticket(id) = self.panel else { return false };
        if !matches!(self.board.editing, Some(EditField::Title | EditField::Description)) || !self.ticket_add_files_supported() {
            return false;
        }
        let kept = self.board.ticket(id).map_or(0, |t| t.attachments.len());
        let (took, found) = self.pasted_files(item, |p| p.is_file(), cx);
        let problems = crate::app::take_files(&mut self.board.edit_files, found, kept);
        if !problems.is_empty() {
            self.board.edit_error = Some(problems.join(" "));
        }
        cx.notify();
        took
    }

    // ---- The page -----------------------------------------------------------

    pub fn render_board(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let scope = self.board_filter();
        // Between the Board page and a project's Board tab: never show the other one's cards.
        if self.board.view.as_ref().is_some_and(|v| v.project_id != scope) {
            self.board.view = None;
        }
        let current = self.board.view.is_some();
        if self.board_online() && self.board.error.is_none() && load_on_show(self.board.asked, current, self.board.stale, scope) {
            self.board.stale = false;
            self.board_load();
        }
        let locked = self.project_board().is_some();
        let serif = ui::serif(cx);
        let online = self.board_online();
        let needs = self.board_needs_count();
        let total = self.board.view.as_ref().map_or(0, |v| v.columns.iter().map(|c| c.tickets.len()).sum::<usize>());
        let project = self.board_filter().and_then(|id| self.projects.iter().find(|p| p.id == id)).map(|p| p.name.clone());
        let owner = match self.board.owner {
            OwnerFilter::Anyone => "Owner: Anyone".to_string(),
            OwnerFilter::NoOwner => "No owner yet".to_string(),
            OwnerFilter::Employee(e) => self.employee_name(e).unwrap_or_else(|| "Owner".into()),
        };
        let creator = match self.board.creator {
            CreatorFilter::Anyone => "Created by: Anyone".to_string(),
            CreatorFilter::Me => "Created by: Me".to_string(),
            CreatorFilter::Member(u) => format!("Created by: {}", self.member_name(u)),
        };
        let picker = |id: &'static str, label: String, set: bool, cx: &mut Context<Self>| {
            let open = self.overlay == Overlay::Menu && self.menu.as_ref().is_some_and(|m| m.field == Some(id));
            ui::dropdown_trigger(id, 28., set, open)
                .max_w(px(220.))
                .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| {
                    let items = this.board_filter_menu(id);
                    this.open_dropdown(id, items, w, cx);
                }))
                .child(div().min_w_0().truncate().child(label))
                .child(ui::caret())
                .child(ui::track_bounds(self.dropdown_bounds.clone(), id))
        };
        let header = v().flex_none().px(px(24.)).pt(px(22.)).pb(px(12.)).gap(px(10.)).child(
            h().gap(px(10.))
                .flex_wrap()
                .child(
                    v().flex_1()
                        .min_w(px(160.))
                        // In a project the project bar above already names it.
                        .when(!locked, |el| el.child(div().font_family(serif).text_size(px(26.)).child("Board")))
                        .child(div().text_size(px(12.)).text_color(c(theme::TEXT_2)).child(format!("{total} tickets · {needs} need you"))),
                )
                .when(!locked, |el| el.child(picker("board-project", format!("Project: {}", project.clone().unwrap_or_else(|| "All projects".into())), project.is_some(), cx)))
                .when(!locked && project.is_some(), |el| {
                    el.child(ui::icon_button("board-project-clear").aria_label("Clear the project filter").tooltip(ui::tip("Clear filter", None)).child("×").on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.set_board_project(None, cx))))
                })
                .when(self.ticket_responsible_supported(), |el| el.child(picker("board-responsible", self.responsible_filter_text(), self.board.responsible != ResponsibleFilter::Anyone, cx)))
                .child(picker("board-owner", owner, self.board.owner != OwnerFilter::Anyone, cx))
                .child(picker("board-creator", creator, self.board.creator != CreatorFilter::Anyone, cx))
                .child(div().w(px(200.)).child(ui::input(&self.board.find)))
                .child(
                    h().id("board-needs")
                        .h(px(28.))
                        .px(px(10.))
                        .gap(px(6.))
                        .rounded(px(14.))
                        .border_1()
                        .cursor_pointer()
                        .text_size(px(12.5))
                        .role(Role::Switch)
                        .aria_label("Needs you only")
                        .border_color(c(if self.board.needs_only { theme::ATTENTION_BORDER } else { theme::BORDER }))
                        .when(self.board.needs_only, |el| el.bg(c(theme::ATTENTION_BG)))
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                            this.board.needs_only = !this.board.needs_only;
                            cx.notify();
                        }))
                        .child(ui::diamond(7., theme::ACCENT_TEXT, self.board.needs_only))
                        .child(format!("Needs you ({needs})")),
                )
                .child(
                    ui::button("board-new", "+ New ticket", ButtonKind::Primary, Some(ui::kbd("secondary-n", cx)))
                        .when(!online, |el| el.opacity(0.5).tooltip(ui::tip("Offline · reconnecting", None)))
                        .on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.board_new_ticket(w, cx))),
                )
                .child(ui::icon_button("board-settings").aria_label("Board settings").tooltip(ui::tip("Board settings…", None)).child("⋯").on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.board_dialog(BoardDialog::Columns, w, cx)))),
        );

        // ui-ontwerp §4: the own Claude login is missing or expired.
        let header = header.when(crate::screens::account::needs_connect(self.claude.view.as_ref(), self.claude.unsupported.is_some()), |el| {
            let expired = self.claude.view.as_ref().is_some_and(|v| v.status == aurelius_domain::user::ClaudeLoginStatus::Expired);
            let text = if expired { "Your Claude login expired. Reconnect it to run your tickets." } else { "Connect your Claude login to run your tickets." };
            el.child(
                h().id("board-connect-banner").role(Role::Status).aria_label(text).gap(px(10.)).px(px(12.)).py(px(8.)).rounded(px(8.)).border_1().border_color(c(theme::ATTENTION_BORDER)).bg(c(theme::ATTENTION_BG))
                    .child(div().text_size(px(11.)).text_color(c(theme::ACCENT_TEXT)).child("ATTENTION"))
                    .child(div().flex_1().text_size(px(12.5)).child(text))
                    .child(ui::button("board-connect", if expired { "Reconnect" } else { "Connect" }, ButtonKind::Primary, None).h(px(26.)).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.claude_connect(cx)))),
            )
        });

        let body: AnyElement = if self.projects.is_empty() {
            v().flex_1()
                .items_center()
                .justify_center()
                .gap(px(12.))
                .p(px(40.))
                .child(div().text_size(px(14.)).text_color(c(theme::TEXT_2)).child("Create a project first; work is done in a project."))
                .child(ui::button("board-new-project", "New project", ButtonKind::Primary, None).on_click(|_: &ClickEvent, w, cx| w.dispatch_action(Box::new(crate::keymap::NewProject), cx)))
                .into_any_element()
        } else if let (None, Some(error)) = (&self.board.view, &self.board.error) {
            v().flex_1()
                .items_center()
                .justify_center()
                .gap(px(10.))
                .p(px(40.))
                .child(div().text_size(px(15.)).child("Could not load the Board."))
                .child(div().max_w(px(520.)).text_size(px(12.5)).text_color(c(theme::TEXT_2)).child(error.clone()))
                .child(ui::button("board-retry", "Try again", ButtonKind::Primary, None).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.board.error = None;
                    this.board_load();
                    cx.notify();
                })))
                .into_any_element()
        } else {
            self.render_columns(window, cx)
        };
        let mut keys = KeyContext::default();
        keys.add(crate::keymap::NAV);
        keys.add(crate::keymap::BOARD);
        let announce = self.board.announce.clone();
        v().id("board-page")
            .relative()
            .size_full()
            .min_w_0()
            .track_focus(&self.focus)
            .key_context(keys)
            // What a screen reader is told (moves, a held card): a status, not shown (§15).
            .children(announce.map(|(n, text)| div().id(("board-announce", n as usize)).role(Role::Status).aria_label(text).absolute().top_0().left_0().size(px(1.)).overflow_hidden()))
            .child(header)
            .when(!online && self.board.view.is_some(), |el| el.child(div().px(px(24.)).pb(px(6.)).text_size(px(12.)).text_color(c(theme::TEXT_2)).child("Offline · reconnecting. The board shows the last known state.")))
            .child(body)
            .into_any_element()
    }

    fn render_columns(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let online = self.board_online();
        let query = self.board.find.read(cx).value().to_string();
        // A project's own board is not filtered: its empty state is "No tickets yet".
        let locked = self.project_board().is_some();
        let filtered = (self.board_filter().is_some() && !locked) || self.board.owner != OwnerFilter::Anyone || self.board.creator != CreatorFilter::Anyone || self.board_responsible_wanted().is_some() || !query.trim().is_empty() || self.board.needs_only;
        let loading = self.board.view.is_none();
        let testing = self.board_testing_supported();
        // Before the first answer: the default columns with skeleton cards (§3.3).
        let columns: Vec<BoardColumnCards> = match &self.board.view {
            Some(v) => v.columns.iter().filter(|c| column_shown(c.column.kind, testing)).cloned().collect(),
            None => ColumnKind::ALL
                .iter()
                .filter(|k| column_shown(**k, testing))
                .enumerate()
                .map(|(i, k)| BoardColumnCards { column: ticket::BoardColumnView { id: aurelius_domain::BoardColumnId::new(), kind: *k, name: ticket::column_title(*k).into(), position: i as u32, version: 0 }, tickets: vec![] })
                .collect(),
        };
        let any = columns.iter().any(|c| !c.tickets.is_empty());
        let open = match self.panel {
            Panel::Ticket(id) => Some(id),
            _ => None,
        };
        let filter_project = self.board_filter();
        let held = self.board.held;
        let mut shown_any = false;
        let mut lanes: Vec<AnyElement> = Vec::new();
        for (ci, column) in columns.iter().enumerate() {
            let kind = column.column.kind;
            let (cards, older) = shown_cards(column, self.board.owner, self.board.creator.user(self.active_member().map(|u| u.id)), self.board_responsible_wanted(), &query, |t| self.ticket_needs_me(t), self.board.needs_only, self.board.done_all);
            shown_any |= !cards.is_empty();
            let folded = kind == ColumnKind::PlanReview && column.tickets.is_empty() && !self.board.plan_open && held.is_none_or(|h| h.to != ci);
            let name = column.column.name.clone();
            let count = column.tickets.len();
            let lane = v().id(("board-lane", ci))
                .flex_none()
                .h_full()
                .rounded(px(8.))
                .border_1()
                .border_color(c(theme::BORDER))
                .bg(c(theme::PANEL))
                .role(Role::List)
                .aria_label(SharedString::from(format!("{name}, {count} tickets")))
                .drag_over::<DraggedCard>(move |style, d, _, _| if drop_effect(d.from, kind, d.choosing).allowed() { style.bg(c(theme::ACCENT_HALO)).border_color(c(theme::ATTENTION_BORDER)) } else { style.opacity(0.5) })
                .on_drop(cx.listener(move |this, d: &DraggedCard, w, cx| this.board_drop(d, kind, None, w, cx)));
            if folded {
                lanes.push(
                    lane.w(px(44.))
                        .items_center()
                        .pt(px(10.))
                        .gap(px(2.))
                        .cursor_pointer()
                        .tooltip(ui::tip(empty_text(kind), None))
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                            this.board.plan_open = true;
                            cx.notify();
                        }))
                        .child(div().pb(px(6.)).text_size(px(11.)).text_color(c(theme::TEXT_2)).child("0"))
                        .children(name.chars().map(|ch| div().text_size(px(11.5)).line_height(px(13.)).text_color(c(theme::TEXT_2)).child(ch.to_string())))
                        .into_any_element(),
                );
                continue;
            }
            let head = h().flex_none()
                .h(px(40.))
                .px(px(12.))
                .gap(px(8.))
                .border_b_1()
                .border_color(c(theme::BORDER))
                .child(div().text_size(px(13.)).font_weight(FontWeight::MEDIUM).child(name.clone()))
                .child(div().text_size(px(11.5)).text_color(c(theme::TEXT_2)).child(count.to_string()))
                .child(div().flex_1())
                .when(kind == ColumnKind::Review && column.tickets.iter().any(|t| self.ticket_needs_me(t)), |el| el.child(ui::needs_you_tag("Needs you", theme::ACCENT_TEXT)))
                .when(kind == ColumnKind::PlanReview && column.tickets.is_empty(), |el| {
                    el.child(ui::icon_button(("board-fold", ci)).aria_label("Collapse").tooltip(ui::tip("Collapse", None)).child("‹").on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.board.plan_open = false;
                        cx.notify();
                    })))
                });
            let list = v().id(("board-cards", ci))
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .p(px(8.))
                .gap(px(8.))
                .when(loading, |el| el.children((0..2).map(|_| div().h(px(64.)).rounded(px(8.)).bg(c(theme::SURFACE_2)))))
                .when(!loading && cards.is_empty(), |el| {
                    if kind == ColumnKind::Backlog && !any && !filtered {
                        // The empty board after the fresh start (MU1-19 AC4).
                        el.child(
                            v().gap(px(8.))
                                .p(px(6.))
                                .child(div().text_size(px(13.)).font_weight(FontWeight::MEDIUM).child("No tickets yet"))
                                .child(div().text_size(px(12.)).text_color(c(theme::TEXT_2)).child("New tickets land in Backlog. Nothing starts until you press Start."))
                                .when(online, |el| el.child(ui::button("board-empty-new", "New ticket", ButtonKind::Primary, None).on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.board_new_ticket(w, cx))))),
                        )
                    } else {
                        el.child(div().p(px(4.)).text_size(px(12.)).text_color(c(theme::TEXT_2)).child(empty_text(kind)))
                    }
                })
                .children({
                    // A held card's place in this column shows as a line (§15).
                    let line = held.filter(|h| h.to == ci).map(|h| h.at);
                    let mut rows: Vec<AnyElement> = Vec::new();
                    let mut place = 0;
                    for (i, t) in cards.iter().enumerate() {
                        let is_held = held.is_some_and(|h| h.id == t.id);
                        if !is_held && line == Some(place) {
                            rows.push(held_line(held.map_or(0, |h| h.number)));
                        }
                        place += usize::from(!is_held);
                        rows.push(self.render_card(t, ci * 10_000 + i, open == Some(t.id), filter_project, online, cx));
                    }
                    if line == Some(place) {
                        rows.push(held_line(held.map_or(0, |h| h.number)));
                    }
                    rows
                })
                .when(older > 0, |el| {
                    el.child(
                        div().id(("board-older", ci)).p(px(6.)).cursor_pointer().text_size(px(12.)).text_color(c(theme::TEXT_2)).hover(|s| s.text_color(c(theme::TEXT)))
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.board.done_all = true;
                                cx.notify();
                            }))
                            .child(format!("Show older ({older})")),
                    )
                })
                .when(kind == ColumnKind::Backlog && online && (any || filtered), |el| {
                    el.child(div().id("board-add").p(px(6.)).cursor_pointer().text_size(px(12.5)).text_color(c(theme::TEXT_2)).hover(|s| s.text_color(c(theme::TEXT))).on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.board_new_ticket(w, cx))).child("+ Add ticket"))
                });
            lanes.push(lane.w(px(268.)).child(head).child(list).into_any_element());
        }
        let no_match = filtered && any && !shown_any;
        v().flex_1()
            .min_h_0()
            .w_full()
            .when(no_match, |el| {
                let name = self.board_filter().filter(|_| !locked).and_then(|id| self.projects.iter().find(|p| p.id == id)).map_or_else(|| "these filters".to_string(), |p| p.name.clone());
                el.child(h().px(px(24.)).pb(px(8.)).gap(px(10.)).text_size(px(12.5)).text_color(c(theme::TEXT_2)).child(format!("No tickets in {name}.")).child(
                    div().id("board-clear-filter").cursor_pointer().text_color(c(theme::ACCENT_TEXT)).child("Clear filter").on_click(cx.listener(move |this, _: &ClickEvent, w, cx| {
                        this.board.owner = OwnerFilter::Anyone;
                        this.board.creator = CreatorFilter::Anyone;
                        this.board.responsible = ResponsibleFilter::Anyone;
                        crate::app::set_ui_value(PREF_RESPONSIBLE, serde_json::Value::String(ResponsibleFilter::Anyone.to_pref()));
                        this.board.needs_only = false;
                        this.board.find.update(cx, |i, cx| i.set_value("", w, cx));
                        if locked {
                            cx.notify();
                        } else {
                            this.set_board_project(None, cx);
                        }
                    })),
                ))
            })
            .child(h().id("board-lanes").flex_1().min_h_0().items_stretch().gap(px(12.)).px(px(24.)).pb(px(18.)).overflow_x_scroll().when(!online, |el| el.opacity(0.6)).children(lanes))
            .into_any_element()
    }

    fn render_card(&self, t: &TicketView, key: usize, open: bool, filter_project: Option<ProjectId>, online: bool, cx: &mut Context<Self>) -> AnyElement {
        let id = t.id;
        let choosing = self.ticket_choosing(t);
        let needs = self.ticket_needs(t);
        // B6: what the card shows stays; ◆ and its accent only for whom it counts.
        let mine = self.ticket_needs_me(t);
        let who_block = self.ticket_responsible_supported();
        let root = t.root_task_id.and_then(|r| self.tasks.iter().find(|x| x.id == r));
        let done = t.column == ColumnKind::Done;
        let offset = aurelius_host::utc_offset_minutes_at(ui::now_ms());
        let owner = t.owner_name.clone();
        let delegations = root.map_or(0, |r| r.children.len());
        let dragged = DraggedCard { id, from: t.column, number: t.number, title: t.title.clone(), choosing };
        let failed = t.root_task_status == Some(aurelius_domain::task::TaskStatus::Failed);
        let focused = self.board.focused == Some(id);
        let held = self.board.held.is_some_and(|h| h.id == id);
        // Its Claude login or its quota (ui-ontwerp §5) before anything else the card says.
        let wait = self.ticket_wait(t).filter(|_| matches!(t.column, ColumnKind::Todo | ColumnKind::Doing | ColumnKind::PlanReview | ColumnKind::Review));
        let me = self.active_member().map(|u| u.id);
        let tester = (t.column == ColumnKind::Testing).then(|| self.ticket_testing(t, &self.tasks).0).flatten();
        let status: Option<AnyElement> = match t.column {
            _ if wait.is_some() => wait.clone().map(|text| div().id(("card-wait", key)).text_size(px(11.5)).text_color(c(if needs == Some(Needs::ConnectLogin) { theme::ACCENT_TEXT } else { theme::TEXT_2 })).child(text).into_any_element()),
            ColumnKind::Todo if failed => None,
            ColumnKind::Todo if choosing => Some(
                h().gap(px(6.))
                    .text_size(px(11.5))
                    .text_color(c(theme::TEXT_2))
                    .child(ui::pulse_dot(("card-choosing", key), 6., theme::ACCENT))
                    .child(format!("{} is choosing…", self.dispatcher_for(t.project_id).name().unwrap_or("The dispatcher")))
                    .into_any_element(),
            ),
            ColumnKind::Todo if t.owner_employee_id.is_some() => Some(div().text_size(px(11.5)).text_color(c(theme::TEXT_2)).child(format!("Queued for {}", owner.clone().unwrap_or_default())).into_any_element()),
            ColumnKind::Doing | ColumnKind::PlanReview | ColumnKind::Review => root.map(|r| ui::task_mark(self.task_mark(r), ("card-mark", key)).into_any_element()),
            // Ticket #5: the tester runs the full test suite; the card moves on by itself.
            ColumnKind::Testing => Some(
                h().gap(px(6.))
                    .text_size(px(11.5))
                    .text_color(c(theme::TEXT_2))
                    .child(ui::pulse_dot(("card-testing", key), 6., theme::ACCENT))
                    .child(testing_line(tester.as_deref()))
                    .into_any_element(),
            ),
            _ => None,
        };
        let deadline = t.deadline_ms.map(|d| deadline_text(d, ui::now_ms(), done, offset));
        let state = match (needs, t.column) {
            (Some(n), _) => Some(n.line(mine).to_string()),
            (None, ColumnKind::Todo) if choosing => Some("the dispatcher is choosing".to_string()),
            (None, ColumnKind::Todo) if t.owner_employee_id.is_some() => Some("queued".to_string()),
            (None, ColumnKind::Doing) => Some("working".to_string()),
            (None, ColumnKind::Testing) => Some(testing_line(tester.as_deref()).replacen("Being", "being", 1)),
            (None, ColumnKind::Done) => Some("done".to_string()),
            _ => None,
        };
        let label = SharedString::from(card_label(t, state.as_deref(), deadline.as_ref().map(|(d, _)| d.as_str()), &self.board.column_name(t.column), who_block.then(|| crate::screens::who::who_sentence(t, me)).as_deref()));
        let action: Option<AnyElement> = match t.column {
            // B4: someone else's login is missing; take it over on the own, explicitly.
            _ if online && take_over_offered(t, me) => Some(
                ui::button(("card-use-my-login", key), "Run on my login", ButtonKind::Outline, None).h(px(24.)).px(px(8.)).text_size(px(12.)).on_click(cx.listener(move |this, _: &ClickEvent, w, cx| {
                    cx.stop_propagation();
                    this.overlay = Overlay::Confirm(crate::app::ConfirmKind::UseMyLogin(id));
                    this.overlay_focus.focus(w, cx);
                    cx.notify();
                }))
                .into_any_element(),
            ),
            _ if needs == Some(Needs::ConnectLogin) => Some(
                ui::button(("card-connect", key), connect_label(self.claude.view.as_ref()), ButtonKind::Primary, None).h(px(24.)).px(px(10.)).text_size(px(12.)).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    this.claude_connect(cx);
                }))
                .into_any_element(),
            ),
            ColumnKind::Backlog if online => Some(
                h().child(ui::button(("card-start", key), "▸ Start", ButtonKind::Outline, None).h(px(24.)).px(px(8.)).text_size(px(12.)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    this.ticket_start(id, None, cx);
                })))
                .child(ui::button(("card-start-with", key), "⌄", ButtonKind::Outline, None).h(px(24.)).px(px(6.)).text_size(px(12.)).aria_label("Start with…").tooltip(ui::tip("Start with…", None)).on_click(cx.listener(
                    move |this, e: &ClickEvent, w, cx| {
                        cx.stop_propagation();
                        let items: MenuItems = this.employee_choices().into_iter().map(|(e, name)| menu_entry(format!("Start with {name}"), BoardDo::Start(id, Some(e)), false, None)).collect();
                        this.open_menu(e.position(), items, w, cx);
                    },
                )))
                .into_any_element(),
            ),
            ColumnKind::Review | ColumnKind::PlanReview => Some(
                ui::button(("card-review", key), "Review ›", ButtonKind::Primary, None)
                    .h(px(24.))
                    .px(px(10.))
                    .text_size(px(12.))
                    .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| {
                        cx.stop_propagation();
                        this.open_ticket(id, w, cx);
                    }))
                    .into_any_element(),
            ),
            _ => None,
        };
        v().id(("board-card", key))
            .relative()
            .p(px(10.))
            .gap(px(5.))
            .rounded(px(8.))
            .border_1()
            .cursor_pointer()
            .role(Role::ListItem)
            .aria_label(label)
            .border_color(c(if open { theme::FOCUS_RING } else { theme::BORDER }))
            .bg(c(if open { theme::SURFACE_2 } else { theme::SURFACE }))
            .hover(|s| s.bg(c(theme::SURFACE_2)))
            .when(done, |el| el.opacity(0.75))
            .when(mine, |el| el.border_l_2().border_color(c(theme::ACCENT)))
            .when(self.board.busy.contains(&id), |el| el.opacity(0.6))
            // The keyboard's card: the focus ring always shows, also while it is held (§15).
            .when(focused || held, |el| el.border_2().border_color(c(theme::FOCUS_RING)).aria_active_descendant().child(ui::track_bounds(self.dropdown_bounds.clone(), FOCUSED_CARD)))
            .when(held, |el| el.opacity(0.55))
            .when(online && !done, |el| el.on_drag(dragged, |d, _, _, cx| cx.new(|_| CardGhost(d.clone()))))
            .on_drop(cx.listener(move |this, d: &DraggedCard, w, cx| {
                cx.stop_propagation();
                if let Some(kind) = this.board.ticket(id).map(|t| t.column) {
                    this.board_drop(d, kind, Some(id), w, cx);
                }
            }))
            .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| {
                this.board.focused = Some(id);
                this.open_ticket(id, w, cx);
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, e: &MouseDownEvent, w, cx| {
                    let Some(t) = this.board.ticket(id).cloned() else { return };
                    this.ticket_know_followers(id);
                    let items = this.card_menu(&t);
                    this.open_menu(e.position, items, w, cx);
                }),
            )
            .child(
                h().gap(px(6.))
                    .child(div().font_family(ui::mono(cx)).text_size(px(11.)).text_color(c(theme::TEXT_2)).child(format!("#{}", t.number)))
                    .when(filter_project != Some(t.project_id), |el| el.child(div().min_w_0().truncate().text_size(px(11.)).text_color(c(theme::TEXT_2)).child(t.project_name.clone())))
                    .child(div().flex_1())
                    .children(priority_mark(t.priority).map(|(text, color)| div().flex_none().text_size(px(11.)).text_color(c(color)).child(text))),
            )
            .child(div().text_size(px(13.)).font_weight(FontWeight::MEDIUM).line_clamp(3).child(t.title.clone()))
            .when(!t.labels.is_empty(), |el| {
                el.child(
                    h().gap(px(4.))
                        .flex_wrap()
                        .children(t.labels.iter().take(2).map(|l| div().px(px(6.)).py(px(1.)).rounded(px(8.)).bg(c(theme::SURFACE_2)).text_size(px(10.5)).text_color(c(theme::TEXT_2)).child(l.clone())))
                        .when(t.labels.len() > 2, |el| el.child(div().text_size(px(10.5)).text_color(c(theme::TEXT_2)).child(format!("+{}", t.labels.len() - 2)))),
                )
            })
            .when(status.is_some() || delegations > 0, |el| {
                el.child(
                    h().gap(px(8.))
                        .children(status)
                        .child(div().flex_1())
                        .when(delegations > 0 && matches!(t.column, ColumnKind::Doing | ColumnKind::Review), |el| el.child(div().text_size(px(11.)).text_color(c(theme::TEXT_2)).child(format!("⑂ {delegations} tasks")))),
                )
            })
            .children(needs.map(|n| {
                let color = match (n, mine) {
                    (Needs::Failed, _) => theme::NEGATIVE,
                    (_, true) => theme::ACCENT_TEXT,
                    (_, false) => theme::TEXT_2,
                };
                h().gap(px(5.)).text_size(px(11.5)).text_color(c(color)).when(mine, |el| el.child(ui::diamond(7., theme::ACCENT_TEXT, true))).child(n.line(mine))
            }))
            .when(done, |el| el.child(h().gap(px(5.)).text_size(px(11.5)).text_color(c(theme::ACCENT_TEXT)).child(ui::check(10., theme::ACCENT_TEXT)).child(format!("Done{}", t.done_at_ms.map(|d| format!(" {}", ui::fmt_day(d))).unwrap_or_default()))))
            // B8: the who block; "by …" stays only for a server without Responsible (P-91).
            .child(if who_block {
                self.render_card_who(t, key, deadline, cx)
            } else {
                h().gap(px(6.))
                    .text_size(px(11.))
                    .text_color(c(theme::TEXT_2))
                    .children(owner.as_ref().map(|n| ui::monogram(n, 16., cx)))
                    .child(div().min_w_0().truncate().child(owner.clone().unwrap_or_else(|| "No owner".into())))
                    .child(div().flex_1())
                    .children(deadline.map(|(text, color)| div().flex_none().text_color(c(color)).child(text)))
                    .child(h().flex_none().gap(px(4.)).child(ui::member_monogram(&t.created_by_username, 14., cx)).child(format!("by {}", t.created_by_username)))
                    .into_any_element()
            })
            .children(action.map(|a| h().justify_end().child(a)))
            .into_any_element()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    // Not `super::*`: gpui's `test` attribute would shadow the standard one.
    use super::{
        BoardColumnCards, BoardDialog, CreatorFilter, ColumnsReload, DAY_MS, DONE_SHOWN, DoneCheck, dialog_reads, done_check, expected_version_for, newest_version, board_changed, board_scope, load_now, load_on_show, plan_to_read, default_columns, Grid, Step, TimelineCard, activity_detail, activity_text, feedback_label, mode_text, card_label, focus_step, held_step, held_target, moved_elsewhere, moves_announcement, pick_up, Dispatcher, Drop, HIGH, Needs, OwnerFilter, PRIORITIES, before_for, choosing, dispatcher_gave_up, dispatcher_stopped, forget_unknown, gave_up_to_read, owner_unknown, columns_edit, days_from_civil, deadline_input, deadline_text, dispatcher_of, drop_effect, empty_text, feedback_ready, move_column,
        board_keys_live, needs_you, new_ticket_problems, parse_deadline, pasted_add_files, parse_labels, priority_label, priority_mark, shown_cards, theme, ticket,
        NO_TESTER, column_shown, company_tester_command, test_task, tester_set_text, testing_line, testing_note,
    };
    use aurelius_domain::task::TaskStatus;
    use aurelius_domain::ticket::{ColumnKind, TicketPriority, TicketView};
    use aurelius_domain::{EmployeeId, ProjectId, TicketId};

    #[test]
    fn pasted_files_go_as_add_files_and_none_leaves_the_field_out() {
        assert_eq!(pasted_add_files(Vec::new()), None, "an older server refuses an unknown field");
        let files = vec![(std::path::PathBuf::from("/tmp/a.png"), 4), (std::path::PathBuf::from("/tmp/b.pdf"), 9)];
        assert_eq!(pasted_add_files(files), Some(vec!["/tmp/a.png".to_string(), "/tmp/b.pdf".to_string()]));
    }

    pub(crate) fn card(number: u32, column: ColumnKind) -> TicketView {
        serde_json::from_value(serde_json::json!({
            "id": TicketId::new(), "number": number, "project_id": ProjectId::new(), "project_name": "Atlas",
            "title": format!("Ticket {number}"), "description": null, "column_id": aurelius_domain::BoardColumnId::new(), "column": column.as_str(),
            "position": number as f64, "priority": "normal", "created_by_user": aurelius_domain::UserId::new(), "created_by_username": "ada",
            "login_user": aurelius_domain::UserId::new(), "login_username": "ada",
            "owner_employee_id": null, "owner_name": null, "run": {}, "labels": [], "attachments": [], "deadline_ms": null,
            "root_task_id": null, "root_task_status": null, "review_feedback": null, "version": 1, "created_at_ms": 0, "updated_at_ms": number,
            "started_at_ms": null, "done_at_ms": null, "archived_at_ms": null,
        }))
        .expect("a ticket view")
    }

    fn column(kind: ColumnKind) -> ticket::BoardColumnView {
        ticket::BoardColumnView { id: aurelius_domain::BoardColumnId::new(), kind, name: ticket::column_title(kind).into(), position: 0, version: 1 }
    }

    pub(crate) fn timeline(title: &str, actor: &str) -> TimelineCard {
        TimelineCard { seq: 1, at_ms: 0, kind: "activity".into(), label: "ACTIVITY".into(), title: title.into(), detail: "ticket".into(), actor: actor.into(), correlation_id: String::new(), task_id: None }
    }

    /// B6, freeze §1: needs_me = needs exists AND (login: me == login_user; else responsible
    /// missing OR responsible_disabled OR responsible == me). Each case of the rule.
    #[test]
    fn needs_me_follows_the_rule_of_the_freeze_in_each_case() {
        use super::needs_me;
        use aurelius_domain::UserId;
        let (me, other, login) = (UserId::new(), UserId::new(), UserId::new());
        let mut t = card(42, ColumnKind::Review);
        t.login_user = login;
        // No need: never, also for the Responsible.
        t.responsible_user_id = Some(me);
        assert!(!needs_me(None, &t, Some(me)));
        for need in [Needs::Review, Needs::PlanReview, Needs::ChooseOwner, Needs::Failed, Needs::Approval] {
            // Nobody responsible: for everyone, as a server without the feature behaves.
            t.responsible_user_id = None;
            t.responsible_disabled = false;
            assert!(needs_me(Some(need), &t, Some(me)), "{need:?} without a responsible");
            assert!(needs_me(Some(need), &t, None), "{need:?} without a responsible, nobody signed in");
            // The Responsible, and only the Responsible.
            t.responsible_user_id = Some(me);
            assert!(needs_me(Some(need), &t, Some(me)), "{need:?} for the responsible");
            assert!(!needs_me(Some(need), &t, Some(other)), "{need:?} for someone else");
            assert!(!needs_me(Some(need), &t, None), "{need:?} with a responsible, nobody signed in");
            // A disabled Responsible counts as nobody.
            t.responsible_disabled = true;
            assert!(needs_me(Some(need), &t, Some(other)), "{need:?} with a disabled responsible");
        }
        // The login: only its own, whoever is responsible.
        for (responsible, disabled) in [(None, false), (Some(me), false), (Some(other), false), (Some(other), true)] {
            t.responsible_user_id = responsible;
            t.responsible_disabled = disabled;
            assert!(needs_me(Some(Needs::ConnectLogin), &t, Some(login)), "the login's own");
            assert!(!needs_me(Some(Needs::ConnectLogin), &t, Some(me)), "not the responsible's");
            assert!(!needs_me(Some(Needs::ConnectLogin), &t, None));
        }
    }

    /// B6: the card still says what it waits for, without ◆ words for someone else, who can still decide.
    #[test]
    fn someone_else_than_the_responsible_still_sees_the_state_and_can_decide() {
        use super::decide_too;
        assert_eq!(Needs::Review.line(true), "Needs you · review");
        assert_eq!(Needs::Review.line(false), "Waiting for review");
        for need in [Needs::Review, Needs::PlanReview, Needs::ChooseOwner, Needs::Failed, Needs::Approval, Needs::ConnectLogin] {
            assert!(!need.others_label().contains("Needs you") && !need.others_label().contains("your"), "{need:?}");
        }
        let mut t = card(42, ColumnKind::Review);
        assert_eq!(decide_too(&t), None);
        t.responsible_user_id = Some(aurelius_domain::UserId::new());
        t.responsible_username = Some("Alexandra".into());
        assert_eq!(decide_too(&t).as_deref(), Some("Alexandra is responsible. You can decide too."));
        t.responsible_disabled = true;
        assert_eq!(decide_too(&t), None);
    }

    /// B6: the "Needs you" filter takes what needs me; the Responsible filter its own.
    #[test]
    fn the_filters_follow_needs_me_and_the_responsible() {
        use super::needs_me;
        use aurelius_domain::UserId;
        let (me, other) = (UserId::new(), UserId::new());
        let mut mine = card(1, ColumnKind::Review);
        mine.responsible_user_id = Some(me);
        let mut theirs = card(2, ColumnKind::Review);
        theirs.responsible_user_id = Some(other);
        let nobody = card(3, ColumnKind::Review);
        let column = BoardColumnCards { column: column(ColumnKind::Review), tickets: vec![mine, theirs, nobody] };
        let numbers = |responsible, needs_only, reader| shown_cards(&column, OwnerFilter::Anyone, None, responsible, "", |t| needs_me(needs_you(t, false, 0), t, reader), needs_only, false).0.iter().map(|t| t.number).collect::<Vec<_>>();
        assert_eq!(numbers(None, false, Some(me)), [1, 2, 3], "without the filter a card stays on the board");
        assert_eq!(numbers(None, true, Some(me)), [1, 3], "mine, and the one nobody is responsible for");
        assert_eq!(numbers(None, true, Some(other)), [2, 3]);
        assert_eq!(numbers(Some(Some(other)), false, Some(me)), [2]);
        assert_eq!(numbers(Some(None), false, Some(me)), [3]);
    }

    /// B8: the who sentence joins the label on a server with a Responsible.
    #[test]
    fn the_label_of_a_card_reads_the_who_block_when_there_is_one() {
        let t = card(42, ColumnKind::Doing);
        assert!(card_label(&t, None, None, "Doing", Some("Forge does the work.")).ends_with("in Doing. Forge does the work."));
    }

    /// MU3-10 AC4, MU3-12 AC2/AC4 and B4: whose login, its wait, and who may take it over.
    #[test]
    fn a_ticket_says_whose_login_it_waits_for_and_others_may_run_it_on_theirs() {
        use super::{login_owner, login_wait, take_over_offered};
        use aurelius_domain::ticket::TicketNeed;
        let alex = aurelius_domain::UserId::new();
        let robin = aurelius_domain::UserId::new();
        let mut t = card(42, ColumnKind::Todo);
        t.login_user = alex;
        t.login_username = "alex".into();
        assert_eq!(login_owner(&t, Some(alex)), "your");
        assert_eq!(login_owner(&t, Some(robin)), "alex's");
        assert_eq!(login_wait(&t, Some(alex), false), None, "nothing to wait for");
        assert!(!take_over_offered(&t, Some(robin)));
        t.needs = Some(TicketNeed::Login);
        assert_eq!(login_wait(&t, Some(alex), false).as_deref(), Some("◆ Waiting for your Claude login"));
        assert_eq!(login_wait(&t, Some(robin), false).as_deref(), Some("Waiting for alex's Claude login"));
        assert_eq!(login_wait(&t, Some(robin), true).as_deref(), Some("Waiting: alex is disabled"), "AC4");
        // Only another board member, on their own login; never the creator's own, never in Backlog or Done.
        assert!(take_over_offered(&t, Some(robin)));
        assert!(!take_over_offered(&t, Some(alex)));
        assert!(!take_over_offered(&t, None));
        t.column = ColumnKind::Backlog;
        assert!(!take_over_offered(&t, Some(robin)));
    }

    /// ui-ontwerp §5, B12: a quota wait names whose limit, and nothing more of it.
    #[test]
    fn a_quota_wait_names_whose_limit() {
        let soon = crate::ui::now_ms();
        assert!(crate::ui::limit_wait_for("alex's", Some(soon)).starts_with("Waiting for alex's limit until "));
        assert_eq!(crate::ui::limit_wait_for("your", None), "Waiting for your limit");
        assert_eq!(Needs::ConnectLogin.label(), "Needs you · Claude login");
    }

    /// P-36 / ui-ontwerp §4–§5: Start without the own login says it waits; the card's action follows the login.
    #[test]
    fn start_without_a_login_says_it_waits_and_the_card_offers_connect() {
        use super::{connect_label, start_toast};
        use aurelius_domain::user::{ClaudeLoginStatus, ClaudeLoginView};
        assert_eq!(start_toast(51, "#51 started. Orion chooses an employee.".into(), true), "#51 started. It waits until you connect your Claude login.");
        assert_eq!(start_toast(51, "#51 started. Orion chooses an employee.".into(), false), "#51 started. Orion chooses an employee.");
        let view = |status| ClaudeLoginView { user_id: aurelius_domain::UserId::new(), status, email: None, plan: None, linked_at_ms: None, checked_at_ms: None, link_id: None, background: false };
        assert_eq!(connect_label(Some(&view(ClaudeLoginStatus::Missing))), "Connect");
        assert_eq!(connect_label(Some(&view(ClaudeLoginStatus::Expired))), "Reconnect");
        assert_eq!(connect_label(None), "Connect");
    }

    /// MU1-15 AC4: each line says who (a board member, the owner or Aurelius) and what.
    #[test]
    fn activity_says_who_changed_the_ticket() {
        assert_eq!(activity_text(&timeline("ticket.created", "user")), "A board member created the ticket");
        assert_eq!(activity_text(&timeline("ticket.started", "user")), "A board member started it");
        assert_eq!(activity_text(&timeline("ticket.moved", "employee")), "The owner moved it");
        assert_eq!(activity_text(&timeline("ticket.moved", "user")), "A board member moved it");
        assert_eq!(activity_text(&timeline("ticket.review", "employee")), "A report was handed in");
        assert_eq!(activity_text(&timeline("ticket.sent_back", "user")), "A board member sent it back to Doing");
        assert_eq!(activity_text(&timeline("ticket.accepted", "user")), "A board member marked it as Done");
        assert_eq!(activity_text(&timeline("ticket.dispatched", "employee")), "The dispatcher chose an owner");
        assert_eq!(activity_text(&timeline("ticket.dispatched", "user")), "A board member chose an owner");
        assert_eq!(activity_text(&timeline("ticket.updated", "system")), "Nobody chose an owner; a board member must");
        assert_eq!(activity_text(&timeline("ticket.updated", "user")), "A board member changed it");
        // A card the server words itself (the main task) keeps its title, and its detail shows.
        let mut task = timeline("You → Columns", "You");
        assert_eq!(activity_text(&task), "You → Columns");
        assert_eq!(activity_detail(&task), None, "the kind alone is no detail");
        task.detail = "for Builder".into();
        assert_eq!(activity_detail(&task), Some("for Builder"));
    }

    /// Pulse m4: no raw event names (nor their kind as a second line) in Activity.
    #[test]
    fn activity_words_events_the_server_does_not() {
        let mode = timeline("conversation.settings_changed", "user");
        let mut mode = mode;
        mode.detail = "conversation".into();
        assert_eq!(activity_text(&mode), "The main task's mode changed");
        assert_eq!(activity_detail(&mode), None);
        let unknown = timeline("ticket.something_new", "system");
        assert_eq!(activity_text(&unknown), "Aurelius: something new");
        assert!(!activity_text(&unknown).contains('.'));
    }

    /// Pulse m2: "Sent back" only after Review; the plan's note is not a send-back.
    #[test]
    fn the_doing_feedback_says_where_it_came_from() {
        let cards = |titles: &[&str]| titles.iter().map(|t| timeline(t, "user")).collect::<Vec<_>>();
        assert_eq!(feedback_label(Some(&cards(&["ticket.moved", "ticket.sent_back", "ticket.plan_approved"]))), "Sent back");
        assert_eq!(feedback_label(Some(&cards(&["ticket.moved", "ticket.plan_approved"]))), "With the plan's approval");
        assert_eq!(feedback_label(Some(&cards(&["ticket.plan_revised", "ticket.sent_back"]))), "New plan asked");
        assert_eq!(feedback_label(None), "Feedback", "activity not read: neutral");
    }

    /// Pulse m3, D-07: the Mode row shows "Plan → Auto" once the plan was approved.
    #[test]
    fn the_mode_shows_where_the_plan_went() {
        use aurelius_domain::autonomy::AgentMode;
        assert_eq!(mode_text(Some(AgentMode::Plan), Some(AgentMode::Auto)).as_deref(), Some("Plan → Auto"));
        assert_eq!(mode_text(Some(AgentMode::Plan), Some(AgentMode::AcceptEdits)), Some(format!("Plan → {}", AgentMode::AcceptEdits.label())));
        assert_eq!(mode_text(Some(AgentMode::Plan), Some(AgentMode::Plan)), None, "Pas aan: Plan stays");
        assert_eq!(mode_text(Some(AgentMode::Plan), None), None, "not read yet");
        assert_eq!(mode_text(Some(AgentMode::Auto), Some(AgentMode::Auto)), None);
    }

    fn grid(columns: &[(ColumnKind, usize)]) -> (Grid, Vec<Vec<TicketId>>) {
        let ids: Vec<Vec<TicketId>> = columns.iter().map(|(_, n)| (0..*n).map(|_| TicketId::new()).collect()).collect();
        (columns.iter().zip(&ids).map(|((k, _), ids)| (*k, ids.clone())).collect(), ids)
    }

    /// MU1-06 AC5: the six names and the order of §3 come back; ids and versions stay.
    #[test]
    fn restore_defaults_brings_back_the_names_and_the_order() {
        let mut columns: Vec<ticket::BoardColumnView> = ColumnKind::ALL.iter().rev().enumerate().map(|(i, k)| ticket::BoardColumnView { position: i as u32, name: format!("Custom {i}"), ..column(*k) }).collect();
        let ids: Vec<_> = columns.iter().map(|c| (c.id, c.version)).collect();
        default_columns(&mut columns);
        columns.sort_by_key(|c| c.position);
        assert_eq!(columns.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ColumnKind::ALL.map(ticket::column_title));
        let mut kept: Vec<_> = columns.iter().map(|c| (c.id, c.version)).collect();
        kept.sort_by_key(|(id, _)| ids.iter().position(|(i, _)| i == id));
        assert_eq!(kept, ids, "the same columns, so the board's version still matches");
    }

    /// §15: ↑↓ within a column, ←→ to the nearest column with cards at the same height.
    #[test]
    fn arrows_go_from_card_to_card_and_skip_empty_columns() {
        use ColumnKind::*;
        let (g, ids) = grid(&[(Backlog, 3), (Todo, 0), (Doing, 2), (Review, 0)]);
        assert_eq!(focus_step(&g, None, Step::Down), Some(ids[0][0]), "nothing focused: the first card");
        assert_eq!(focus_step(&g, Some(ids[0][0]), Step::Down), Some(ids[0][1]));
        assert_eq!(focus_step(&g, Some(ids[0][2]), Step::Down), Some(ids[0][2]), "the last stays");
        assert_eq!(focus_step(&g, Some(ids[0][0]), Step::Up), Some(ids[0][0]));
        assert_eq!(focus_step(&g, Some(ids[0][1]), Step::Right), Some(ids[2][1]), "over the empty Todo, same height");
        assert_eq!(focus_step(&g, Some(ids[0][2]), Step::Right), Some(ids[2][1]), "its last card when shorter");
        assert_eq!(focus_step(&g, Some(ids[2][0]), Step::Right), Some(ids[2][0]), "nothing further right");
        assert_eq!(focus_step(&g, Some(ids[2][0]), Step::Left), Some(ids[0][0]));
        assert_eq!(focus_step(&g, Some(TicketId::new()), Step::Up), Some(ids[0][0]), "a card that left: the first");
        assert_eq!(focus_step(&Vec::new(), None, Step::Down), None);
    }

    /// §15: Space picks a card up; arrows choose every column (also an empty one) and the place;
    /// Enter puts it right above the card at that place, or at the bottom.
    #[test]
    fn a_held_card_goes_to_a_column_and_a_place() {
        use ColumnKind::*;
        let (g, ids) = grid(&[(Backlog, 3), (Todo, 0), (Doing, 2)]);
        let held = pick_up(&g, ids[0][1], 7, false).expect("shown");
        assert_eq!((held.from, held.to, held.at), (Backlog, 0, 1));
        assert_eq!(held_target(&g, &held), Some((Backlog, Some(ids[0][2]))), "where it is: above the next one");
        let up = held_step(&g, held, Step::Up);
        assert_eq!(held_target(&g, &up), Some((Backlog, Some(ids[0][0]))), "to the top");
        let down = held_step(&g, held_step(&g, held, Step::Down), Step::Down);
        assert_eq!(down.at, 2, "the column has two others: below them");
        assert_eq!(held_target(&g, &down), Some((Backlog, None)), "at the bottom");
        let todo = held_step(&g, held, Step::Right);
        assert_eq!((todo.to, todo.at), (1, 0), "an empty column: its only place");
        assert_eq!(held_target(&g, &todo), Some((Todo, None)));
        let doing = held_step(&g, todo, Step::Right);
        assert_eq!(held_target(&g, &doing), Some((Doing, Some(ids[2][0]))));
        assert_eq!(held_step(&g, doing, Step::Right).to, 2, "the last column stays");
        assert_eq!(held_step(&g, held, Step::Left).to, 0);
        assert!(pick_up(&g, TicketId::new(), 1, false).is_none(), "not shown: nothing to pick up");
    }

    /// §15: one label per card, the column as a list with its count.
    #[test]
    fn a_card_reads_as_one_sentence() {
        let mut t = card(42, ColumnKind::Doing);
        t.title = "Login per board member".into();
        t.priority = TicketPriority::Urgent;
        t.owner_name = Some("Builder".into());
        assert_eq!(card_label(&t, Some("working"), Some("Due Nov 3"), "Doing", None), "Ticket 42, Login per board member, priority Urgent, owner Builder, working, due Nov 3, in Doing.");
        t.owner_name = None;
        t.priority = TicketPriority::Normal;
        assert_eq!(card_label(&t, None, None, "Building", None), "Ticket 42, Login per board member, priority Normal, owner none, in Building.");
    }

    /// §15: moves made elsewhere are told, bundled; this window's own are not told twice.
    #[test]
    fn moves_elsewhere_are_announced_together() {
        let view = |cards: Vec<TicketView>| ticket::BoardView { columns: vec![BoardColumnCards { column: column(ColumnKind::Doing), tickets: cards }], project_id: None, as_of_seq: 0 };
        let (a, b, c) = (card(42, ColumnKind::Doing), card(43, ColumnKind::Doing), card(44, ColumnKind::Doing));
        let old = view(vec![a.clone(), b.clone(), c.clone()]);
        let mut a2 = a.clone();
        a2.column = ColumnKind::Review;
        a2.owner_name = Some("Builder".into());
        let mut b2 = b.clone();
        b2.column = ColumnKind::Backlog;
        let mut c2 = c.clone();
        c2.column = ColumnKind::Done;
        let new = view(vec![a2, b2, c2]);
        let own: std::collections::HashSet<TicketId> = [c.id].into();
        let moves = moved_elsewhere(&old, &new, &own);
        assert_eq!(moves.iter().map(|m| (m.0, m.1)).collect::<Vec<_>>(), [(42, ColumnKind::Review), (43, ColumnKind::Backlog)], "not this window's own");
        let name = |k: ColumnKind| ticket::column_title(k).to_string();
        assert_eq!(moves_announcement(&moves[..1], name).as_deref(), Some("#42 moved to Review by Builder."), "the owner hands it in");
        assert_eq!(moves_announcement(&moves[1..], name).as_deref(), Some("#43 moved to Backlog."));
        assert_eq!(moves_announcement(&moves, name).as_deref(), Some("2 tickets moved: #42 to Review, #43 to Backlog."));
        assert_eq!(moves_announcement(&[], name), None);
    }

    /// A ticket in Review with its main task (in review, with a conversation) and these delegations.
    fn in_review(delegations: &[TaskStatus]) -> (ticket::TicketDetail, aurelius_domain::ConversationId) {
        let mut t = card(7, ColumnKind::Review);
        let conversation = aurelius_domain::ConversationId::new();
        let mut root = crate::screens::tasks::tests::task(t.project_id, None, TaskStatus::Review, 0);
        root.conversation_id = Some(conversation);
        root.ticket_id = Some(t.id);
        t.root_task_id = Some(root.id);
        t.root_task_status = Some(TaskStatus::Review);
        let mut tasks = vec![root.clone()];
        tasks.extend(delegations.iter().map(|s| {
            let mut d = crate::screens::tasks::tests::task(t.project_id, None, *s, 1);
            d.parent_task_id = Some(root.id);
            d.ticket_id = Some(t.id);
            d
        }));
        (ticket::TicketDetail { as_of_seq: 1, ticket: t, root_task: Some(root), tasks }, conversation)
    }

    /// B6 (D-M1): ✓ Done is off while work goes on, counted as the server's
    /// `working_tasks` (queued, running, waiting), plus a run in the main task's conversation.
    #[test]
    fn done_is_off_while_linked_work_still_runs() {
        let quiet = |_| false;
        for s in [TaskStatus::Queued, TaskStatus::Running, TaskStatus::Waiting] {
            let (d, _) = in_review(&[TaskStatus::Succeeded, s]);
            let check = done_check(Some(&d), quiet);
            assert!(matches!(&check, DoneCheck::Working { tasks, conversation: false } if tasks.len() == 1 && tasks[0].status == s), "{s:?}: {check:?}");
            assert!(!check.allowed(true), "{s:?}: the button is off");
        }
        // The server lets these through: so does the button.
        for s in [TaskStatus::Draft, TaskStatus::Review, TaskStatus::Succeeded, TaskStatus::Failed, TaskStatus::Cancelled] {
            let (d, _) = in_review(&[s]);
            assert_eq!(done_check(Some(&d), quiet), DoneCheck::Ready, "{s:?}");
            assert!(done_check(Some(&d), quiet).allowed(true), "{s:?}: the button is on");
            assert!(!done_check(Some(&d), quiet).allowed(false), "{s:?}: offline it is off");
        }
        // A question in Review: a run goes in the main task's conversation (S4b refuses then too).
        let (d, conversation) = in_review(&[]);
        let check = done_check(Some(&d), |c| c == conversation);
        assert_eq!(check, DoneCheck::Working { tasks: vec![], conversation: true });
        assert!(!check.allowed(true));
        assert!(done_check(Some(&d), |c| c != conversation).allowed(true), "a run elsewhere does not count");
        // The main task itself is left out, as on the server.
        let (mut d, _) = in_review(&[]);
        d.tasks[0].status = TaskStatus::Running;
        assert_eq!(done_check(Some(&d), quiet), DoneCheck::Ready);
    }

    /// D-M1, the drag: Review → Done opens "Mark as Done?" without the ticket
    /// open. The dialog reads the ticket itself and Done waits for it.
    #[test]
    fn done_after_a_drag_reads_the_ticket_first() {
        let (d, _) = in_review(&[TaskStatus::Running]);
        let id = d.ticket.id;
        assert_eq!(drop_effect(ColumnKind::Review, ColumnKind::Done, false), Drop::MarkDone);
        let other = TicketId::new();
        assert_eq!(dialog_reads(Some(BoardDialog::MarkDone(id)), None), Some(id), "nothing open: read it");
        assert_eq!(dialog_reads(Some(BoardDialog::MarkDone(id)), Some(other)), Some(id), "another ticket open: read it, for the dialog alone");
        assert_eq!(dialog_reads(Some(BoardDialog::MarkDone(id)), Some(id)), None, "the panel has it");
        assert_eq!(dialog_reads(Some(BoardDialog::Plan(id)), Some(other)), Some(id));
        assert_eq!(dialog_reads(Some(BoardDialog::SendBack(id)), None), None);
        // Before the answer: off. After it: off while the delegation runs.
        assert_eq!(done_check(None, |_| false), DoneCheck::Loading);
        assert!(!done_check(None, |_| false).allowed(true));
        assert!(!done_check(Some(&d), |_| false).allowed(true));
    }

    /// D-m1: edits, moves, the owner and Start carry the newest version the window knows.
    #[test]
    fn ticket_writes_carry_the_known_version() {
        use aurelius_domain::Command;
        let (mut d, _) = in_review(&[]);
        let id = d.ticket.id;
        let mut newer = d.ticket.clone();
        newer.version = 5;
        d.ticket.version = 4;
        let view = ticket::BoardView { as_of_seq: 1, project_id: None, columns: vec![BoardColumnCards { column: column(ColumnKind::Review), tickets: vec![newer] }] };
        assert_eq!(newest_version(Some(&d), Some(&view), id), Some(5), "the card is newer than the read");
        assert_eq!(newest_version(Some(&d), None, id), Some(4));
        assert_eq!(newest_version(None, None, id), None);
        let writes = [
            Command::UpdateTicket(super::empty_update(id)),
            Command::MoveTicket(ticket::MoveTicket { ticket_id: id, column_id: aurelius_domain::BoardColumnId::new(), before_ticket_id: None }),
            Command::AssignTicket(ticket::AssignTicket { ticket_id: id, employee_id: None, reason: None }),
            Command::StartTicket(ticket::StartTicket { ticket_id: id, employee_id: None }),
        ];
        for w in &writes {
            assert_eq!(expected_version_for(w, Some(5)), Some(5), "{w:?}");
        }
        assert_eq!(expected_version_for(&Command::AcceptTicket(ticket::AcceptTicket { confirm_secret: false, ticket_id: id, note: None }), Some(5)), None, "a decision follows the column");
        assert_eq!(expected_version_for(&Command::CancelTicket(ticket::CancelTicket { ticket_id: id }), Some(5)), None);
    }

    /// D-m2: after a conflict the columns are built again from the next board, once;
    /// another refusal (a bad name) keeps what the board member typed.
    #[test]
    fn a_columns_conflict_rebuilds_from_the_next_board() {
        let mut reload = ColumnsReload::Idle;
        reload.failed("Each column needs its own name.");
        reload.loaded();
        assert!(!reload.take_rebuild(), "not a conflict: the fields stay");
        reload.failed(crate::backend::BOARD_CONFLICT);
        assert!(!reload.take_rebuild(), "the newer board is not here yet");
        reload.failed(crate::backend::BOARD_CONFLICT);
        reload.loaded();
        assert!(reload.take_rebuild(), "rebuilt from the newer board");
        assert!(!reload.take_rebuild(), "once");
        reload.loaded();
        assert!(!reload.take_rebuild());
    }

    /// D-m3: a task's events reload the board only when it works for a ticket
    /// (or is new to this window); tickets, columns and the dispatcher always do.
    #[test]
    fn only_tasks_of_tickets_reload_the_board() {
        use aurelius_domain::TaskId;
        let (plain, of_ticket, new) = (TaskId::new(), TaskId::new(), TaskId::new());
        let known = |id: TaskId| if id == plain { Some(false) } else if id == of_ticket { Some(true) } else { None };
        assert!(!board_changed(false, false, &[plain.to_string()], known), "a task without a ticket");
        assert!(board_changed(false, false, &[plain.to_string(), of_ticket.to_string()], known));
        assert!(board_changed(false, false, &[new.to_string()], known), "a new delegation may be a ticket's");
        assert!(board_changed(false, false, &["not-an-id".to_string()], known));
        assert!(board_changed(true, false, &[], known), "a ticket");
        assert!(board_changed(false, true, &[], known), "the dispatcher");
        assert!(!board_changed(false, false, &[], known));
    }

    /// D-m3: the plan is read once per ticket version and conversation (again while none was found).
    #[test]
    fn the_plan_is_read_again_only_for_a_new_version() {
        let text = Some("The plan".to_string());
        assert!(plan_to_read(None, None, 3), "never read");
        assert!(!plan_to_read(Some(3), None, 3), "on its way");
        assert!(!plan_to_read(Some(3), Some(&text), 3), "read for this version");
        assert!(plan_to_read(Some(3), Some(&text), 4), "a new version: a new plan");
        assert!(plan_to_read(Some(3), Some(&None), 3), "not found yet");
    }

    /// D-m3: no second `board.get` for the same filter while one is on its way.
    #[test]
    fn one_board_read_at_a_time_per_filter() {
        let project = Some(ProjectId::new());
        assert!(load_now(None, project), "nothing on its way");
        assert!(!load_now(Some(project), project), "the same filter: wait, then once more");
        assert!(load_now(Some(None), project), "another filter goes at once");
    }

    /// Ticket #4: a project's Board tab shows that project only, whatever the Board page's filter.
    #[test]
    fn a_project_board_tab_shows_its_own_project_and_the_board_page_keeps_its_filter() {
        use crate::app::{Page, ProjectRef, Tab};
        let (here, chosen) = (ProjectId::new(), ProjectId::new());
        let all = |_| true;
        assert_eq!(board_scope(Page::Project(ProjectRef::Real(here), Tab::Board), Some(chosen), all), Some(here));
        assert_eq!(board_scope(Page::Project(ProjectRef::Real(here), Tab::Board), None, all), Some(here));
        assert_eq!(board_scope(Page::Board, Some(chosen), all), Some(chosen), "the Board page's own filter");
        assert_eq!(board_scope(Page::Board, None, all), None);
        assert_eq!(board_scope(Page::Project(ProjectRef::Real(here), Tab::Tasks), Some(chosen), all), Some(chosen), "only the Board tab fixes the project");
        assert_eq!(board_scope(Page::Project(ProjectRef::Sample(0), Tab::Board), None, all), None, "a sample has no board");
        assert_eq!(board_scope(Page::Project(ProjectRef::Real(here), Tab::Board), None, |p| p != here), None, "a project that is gone");
    }

    /// Ticket #4: going between the Board page and a project's Board tab reads the
    /// new scope at once, also while the other scope's read is on its way.
    #[test]
    fn the_board_reads_again_when_its_scope_changes() {
        let here = Some(ProjectId::new());
        assert!(load_on_show(None, false, false, here), "nothing read yet");
        assert!(!load_on_show(None, true, false, here), "already shown");
        assert!(load_on_show(None, true, true, here), "stale");
        assert!(!load_on_show(Some(here), false, false, here), "this scope's read is on its way");
        assert!(load_on_show(Some(None), false, false, here), "only the other scope's read is on its way");
        assert!(!load_on_show(Some(None), true, false, here), "the board shown is this scope's");
    }

    /// §6, cell by cell: what a board member's drop does.
    #[test]
    fn a_drop_follows_the_table_of_section_6() {
        use ColumnKind::*;
        assert_eq!(drop_effect(Backlog, Todo, false), Drop::Start, "MU1-09 AC2: a drag to Todo is Start");
        assert_eq!(drop_effect(Backlog, Backlog, false), Drop::Reorder);
        assert_eq!(drop_effect(Backlog, Doing, false), Drop::Refused("Start first."));
        assert_eq!(drop_effect(Todo, Backlog, false), Drop::BackToBacklog { confirm: false });
        assert_eq!(drop_effect(Todo, Backlog, true), Drop::BackToBacklog { confirm: true }, "asked while the dispatcher works");
        assert_eq!(drop_effect(Todo, Doing, false), Drop::Refused("Doing starts when an owner is chosen."));
        assert_eq!(drop_effect(Doing, Backlog, false), Drop::BackToBacklog { confirm: true }, "stops the run: asked");
        assert_eq!(drop_effect(Doing, Todo, false), Drop::Refused("Not from Doing."), "§6");
        assert_eq!(drop_effect(Review, Todo, false), Drop::Refused("Only from Backlog."));
        assert_eq!(drop_effect(Doing, PlanReview, false), Drop::Refused("The engine moves tickets here."));
        assert_eq!(drop_effect(Doing, Review, false), Drop::Refused("The owner hands it in."));
        assert_eq!(drop_effect(Doing, Done, false), Drop::Refused("Only from Review."));
        assert_eq!(drop_effect(PlanReview, Backlog, false), Drop::Reject);
        assert_eq!(drop_effect(PlanReview, Doing, false), Drop::PlanDialog, "MU1-14: the plan dialog");
        assert_eq!(drop_effect(Review, Doing, false), Drop::SendBack, "MU1-16 AC2: with feedback");
        assert_eq!(drop_effect(Review, Done, false), Drop::MarkDone, "MU1-16 AC1: asked first (D-08)");
        assert_eq!(drop_effect(Review, Backlog, false), Drop::BackToBacklog { confirm: true });
        for to in ColumnKind::ALL {
            assert_eq!(drop_effect(Done, to, false), Drop::Refused("Done tickets are closed."), "OV-08: {to:?}");
        }
    }

    /// The client never offers a move the server's own rules refuse.
    #[test]
    fn what_the_board_allows_the_server_allows() {
        use ColumnKind::*;
        for from in ColumnKind::ALL {
            for to in ColumnKind::ALL {
                let server_move = ticket::decide_move(from, to).is_ok();
                match drop_effect(from, to, false) {
                    Drop::Reorder | Drop::Start | Drop::BackToBacklog { .. } | Drop::Reject => assert!(server_move, "{from:?} → {to:?}"),
                    // These go through their own commands (approve/revise, send back, accept).
                    Drop::PlanDialog => assert_eq!((from, to), (PlanReview, Doing)),
                    Drop::SendBack => assert_eq!((from, to), (Review, Doing)),
                    Drop::MarkDone => assert_eq!((from, to), (Review, Done)),
                    Drop::Refused(why) => assert!(!why.is_empty(), "a reason in words"),
                }
            }
        }
    }

    #[test]
    fn needs_you_is_review_plan_review_a_failure_an_approval_or_no_owner() {
        let mut t = card(1, ColumnKind::Review);
        assert_eq!(needs_you(&t, false, 0), Some(Needs::Review));
        t.column = ColumnKind::PlanReview;
        assert_eq!(needs_you(&t, false, 0), Some(Needs::PlanReview));
        // B5: Todo, no owner, and nobody chooses → "choose an owner".
        t.column = ColumnKind::Todo;
        t.root_task_status = Some(TaskStatus::Draft);
        assert_eq!(needs_you(&t, false, 0), Some(Needs::ChooseOwner));
        assert_eq!(needs_you(&t, true, 0), None, "the dispatcher is choosing");
        t.owner_employee_id = Some(EmployeeId::new());
        assert_eq!(needs_you(&t, false, 0), None, "queued for its owner");
        // B6: a failed main task stays in Doing and needs you.
        t.column = ColumnKind::Doing;
        t.root_task_status = Some(TaskStatus::Failed);
        assert_eq!(needs_you(&t, false, 0), Some(Needs::Failed));
        t.root_task_status = Some(TaskStatus::Running);
        assert_eq!(needs_you(&t, false, 0), None);
        assert_eq!(needs_you(&t, false, 1), Some(Needs::Approval));
        t.column = ColumnKind::Backlog;
        assert_eq!(needs_you(&t, false, 0), None, "Backlog never needs you");
        t.column = ColumnKind::Done;
        assert_eq!(needs_you(&t, false, 0), None);
    }

    /// B5 as on mobile: the newest of start, choice, cancel or the system's
    /// `ticket.updated` (needs: owner) decides.
    #[test]
    fn the_dispatcher_gave_up_when_the_server_said_so_after_the_last_start() {
        let gave = |titles: &[(&str, &str)]| dispatcher_gave_up(&titles.iter().map(|(t, a)| timeline(t, a)).collect::<Vec<_>>());
        assert!(gave(&[("ticket.updated", "system"), ("ticket.started", "user"), ("ticket.created", "user")]), "the dispatcher chose nobody");
        assert!(!gave(&[("ticket.started", "user"), ("ticket.updated", "system"), ("ticket.cancelled", "user")]), "started again after it gave up");
        assert!(!gave(&[("ticket.dispatched", "employee"), ("ticket.updated", "system")]));
        assert!(!gave(&[("ticket.updated", "user"), ("ticket.started", "user")]), "a board member's change is not the dispatcher's answer");
        assert!(gave(&[("ticket.updated", "user"), ("ticket.updated", "system")]), "a later edit does not hide it");
        assert!(!gave(&[]), "unknown: choosing");
    }

    #[test]
    fn the_dispatcher_is_choosing_until_it_gives_up_and_never_without_one() {
        let mut t = card(4, ColumnKind::Todo);
        assert!(choosing(&t, true, false));
        assert!(!choosing(&t, true, true), "it chose nobody: B5");
        assert!(!choosing(&t, false, false), "no dispatcher: B5");
        assert_eq!(needs_you(&t, choosing(&t, true, true), 0), Some(Needs::ChooseOwner));
        assert_eq!(needs_you(&t, choosing(&t, true, false), 0), None);
        t.owner_employee_id = Some(EmployeeId::new());
        assert!(!choosing(&t, true, false), "an owner was chosen");
        t.owner_employee_id = None;
        t.column = ColumnKind::Backlog;
        assert!(!choosing(&t, true, false), "only in Todo");
    }

    /// One timeline read per Todo ticket without an owner, again only when its version changed.
    #[test]
    fn the_timeline_is_read_for_owner_less_todo_tickets_whose_version_changed() {
        let (mut todo, mut owned, backlog) = (card(1, ColumnKind::Todo), card(2, ColumnKind::Todo), card(3, ColumnKind::Backlog));
        owned.owner_employee_id = Some(EmployeeId::new());
        todo.version = 3;
        let view = |todo: &TicketView| ticket::BoardView {
            columns: vec![
                BoardColumnCards { column: column(ColumnKind::Backlog), tickets: vec![backlog.clone()] },
                BoardColumnCards { column: column(ColumnKind::Todo), tickets: vec![todo.clone(), owned.clone()] },
            ],
            project_id: None,
            as_of_seq: 0,
        };
        let mut known = std::collections::HashMap::new();
        assert_eq!(gave_up_to_read(&view(&todo), &known).collect::<Vec<_>>(), [(todo.id, 3)]);
        known.insert(todo.id, (3, Some(true)));
        assert_eq!(gave_up_to_read(&view(&todo), &known).count(), 0, "read at this version");
        todo.version = 4;
        assert_eq!(gave_up_to_read(&view(&todo), &known).collect::<Vec<_>>(), [(todo.id, 4)]);
    }

    #[test]
    fn an_unread_timeline_needs_you_and_is_asked_again() {
        let t = card(1, ColumnKind::Todo);
        let mut known = std::collections::HashMap::new();
        assert!(!dispatcher_stopped(known.get(&t.id)), "not read yet: the dispatcher is choosing");
        known.insert(t.id, (1, Some(false)));
        assert!(!dispatcher_stopped(known.get(&t.id)) && !owner_unknown(known.get(&t.id)));
        known.insert(t.id, (1, Some(true)));
        assert!(dispatcher_stopped(known.get(&t.id)), "B5: it gave up");
        // Trace R-m4: the timeline did not load. Not "is choosing…" but Needs you.
        known.insert(t.id, (1, None));
        assert!(dispatcher_stopped(known.get(&t.id)) && owner_unknown(known.get(&t.id)));
        assert!(!choosing(&t, true, dispatcher_stopped(known.get(&t.id))));
        assert_eq!(needs_you(&t, false, 0), Some(Needs::ChooseOwner));
        // Not asked again at once, only after the wait.
        let one = ticket::BoardView { columns: vec![BoardColumnCards { column: ticket::BoardColumnView { id: aurelius_domain::BoardColumnId::new(), kind: ColumnKind::Todo, name: "Todo".into(), position: 1, version: 1 }, tickets: vec![t.clone()] }], project_id: None, as_of_seq: 0 };
        assert_eq!(gave_up_to_read(&one, &known).count(), 0);
        assert!(!forget_unknown(&mut known, t.id, 2), "another version: not this read");
        assert!(forget_unknown(&mut known, t.id, 1));
        assert_eq!(gave_up_to_read(&one, &known).collect::<Vec<_>>(), [(t.id, 1)], "the retry asks again");
        known.insert(t.id, (1, Some(false)));
        assert!(!forget_unknown(&mut known, t.id, 1), "read meanwhile: kept");
    }

    #[test]
    fn priorities_show_as_text_and_shape() {
        assert_eq!(priority_mark(TicketPriority::Urgent), Some(("▲▲ Urgent", theme::NEGATIVE)));
        assert_eq!(priority_mark(TicketPriority::High), Some(("▲ High", HIGH)));
        assert_eq!(priority_mark(TicketPriority::Normal), None, "Normal is not shown on a card");
        assert_eq!(priority_mark(TicketPriority::Low), Some(("▽ Low", theme::TEXT_2)));
        assert_eq!(PRIORITIES.map(priority_label), ["▲▲ Urgent", "▲ High", "Normal", "▽ Low"], "B9");
    }

    #[test]
    fn a_deadline_is_typed_as_a_day_or_a_day_and_a_time() {
        // UTC+60: 2026-11-03 17:00 local = 16:00 UTC.
        let at = parse_deadline("2026-11-03 17:00", 60).unwrap().unwrap();
        assert_eq!(at, days_from_civil(2026, 11, 3) * DAY_MS + 16 * 3_600_000);
        assert_eq!(deadline_input(at, 60), "2026-11-03 17:00");
        let day = parse_deadline("2026-11-03", 0).unwrap().unwrap();
        assert_eq!(deadline_input(day, 0), "2026-11-03", "a day alone is the end of that day");
        assert_eq!(parse_deadline("  ", 0), Ok(None));
        for bad in ["2026-02-30", "2026-13-01", "03-11-2026", "2026-11-03 25:00", "tomorrow"] {
            assert!(parse_deadline(bad, 0).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_deadline_says_due_today_or_overdue() {
        let deadline = days_from_civil(2026, 11, 3) * DAY_MS + 17 * 3_600_000;
        assert_eq!(deadline_text(deadline, deadline - 10 * DAY_MS, false, 0), ("Due Nov 3".into(), theme::TEXT_2));
        assert_eq!(deadline_text(deadline, deadline - 3_600_000, false, 0), ("Due today 17:00".into(), HIGH));
        assert_eq!(deadline_text(deadline, deadline + DAY_MS * 2, false, 0), ("Overdue · Nov 3".into(), theme::NEGATIVE));
        assert_eq!(deadline_text(deadline, deadline + DAY_MS * 2, true, 0).1, theme::TEXT_2, "in Done not red");
    }

    #[test]
    fn a_reorder_names_the_card_it_goes_above() {
        let cards = vec![card(1, ColumnKind::Backlog), card(2, ColumnKind::Backlog), card(3, ColumnKind::Backlog)];
        let (a, b, c) = (cards[0].id, cards[1].id, cards[2].id);
        assert_eq!(before_for(&cards, c, Some(a)), Some(Some(a)), "#3 to the top");
        assert_eq!(before_for(&cards, a, None), Some(None), "#1 to the bottom");
        assert_eq!(before_for(&cards, a, Some(b)), None, "already right above #2");
        assert_eq!(before_for(&cards, b, Some(b)), None, "on itself");
        assert_eq!(before_for(&cards, c, None), None, "already at the bottom");
    }

    #[test]
    fn the_filters_find_by_title_number_label_and_owner() {
        let lead = EmployeeId::new();
        let mut column = BoardColumnCards {
            column: ticket::BoardColumnView { id: aurelius_domain::BoardColumnId::new(), kind: ColumnKind::Doing, name: "Doing".into(), position: 2, version: 1 },
            tickets: vec![card(42, ColumnKind::Doing), card(7, ColumnKind::Doing)],
        };
        column.tickets[0].title = "Login per board member".into();
        column.tickets[0].owner_employee_id = Some(lead);
        column.tickets[1].labels = vec!["UI".into()];
        let numbers = |owner, query: &str, needs_only| shown_cards(&column, owner, None, None, query, |t| t.number == 7, needs_only, false).0.iter().map(|t| t.number).collect::<Vec<_>>();
        assert_eq!(numbers(OwnerFilter::Anyone, "", false), [42, 7]);
        assert_eq!(numbers(OwnerFilter::Anyone, "login", false), [42]);
        assert_eq!(numbers(OwnerFilter::Anyone, "#7", false), [7]);
        assert_eq!(numbers(OwnerFilter::Anyone, "ui", false), [7]);
        assert_eq!(numbers(OwnerFilter::Employee(lead), "", false), [42]);
        assert_eq!(numbers(OwnerFilter::NoOwner, "", false), [7]);
        assert_eq!(numbers(OwnerFilter::Anyone, "", true), [7], "Needs you only");
    }

    /// ui-ontwerp §3: "Created by: Anyone / Me / <board member>".
    #[test]
    fn the_board_filters_by_who_created_a_ticket() {
        let (alex, robin) = (aurelius_domain::UserId::new(), aurelius_domain::UserId::new());
        let mut column = BoardColumnCards {
            column: ticket::BoardColumnView { id: aurelius_domain::BoardColumnId::new(), kind: ColumnKind::Todo, name: "Todo".into(), position: 1, version: 1 },
            tickets: vec![card(1, ColumnKind::Todo), card(2, ColumnKind::Todo), card(3, ColumnKind::Todo)],
        };
        column.tickets[0].created_by_user = alex;
        column.tickets[1].created_by_user = robin;
        column.tickets[2].created_by_user = alex;
        let numbers = |filter: CreatorFilter, me| shown_cards(&column, OwnerFilter::Anyone, filter.user(me), None, "", |_| false, false, false).0.iter().map(|t| t.number).collect::<Vec<_>>();
        assert_eq!(numbers(CreatorFilter::Anyone, Some(alex)), [1, 2, 3]);
        assert_eq!(numbers(CreatorFilter::Me, Some(alex)), [1, 3]);
        assert_eq!(numbers(CreatorFilter::Me, Some(robin)), [2]);
        assert_eq!(numbers(CreatorFilter::Member(robin), Some(alex)), [2]);
        assert_eq!(numbers(CreatorFilter::Me, None), [1, 2, 3], "nobody signed in: no filter");
    }

    #[test]
    fn done_shows_the_newest_twenty_then_show_older() {
        let tickets: Vec<TicketView> = (1..=23)
            .map(|n| {
                let mut t = card(n, ColumnKind::Done);
                t.done_at_ms = Some(n as i64);
                t
            })
            .collect();
        let column = BoardColumnCards { column: ticket::BoardColumnView { id: aurelius_domain::BoardColumnId::new(), kind: ColumnKind::Done, name: "Done".into(), position: 5, version: 1 }, tickets };
        let (shown, older) = shown_cards(&column, OwnerFilter::Anyone, None, None, "", |_| false, false, false);
        assert_eq!((shown.len(), older), (DONE_SHOWN, 3));
        assert_eq!(shown[0].number, 23, "newest first");
        assert_eq!(shown_cards(&column, OwnerFilter::Anyone, None, None, "", |_| false, false, true).0.len(), 23);
    }

    #[test]
    fn the_dispatcher_is_the_projects_else_the_companys_else_nobody() {
        use aurelius_domain::employee::LifecycleStatus;
        let planner = crate::screens::org::tests::manager("Planner", None);
        let lead = crate::screens::org::tests::manager("Lead", None);
        let staff = vec![planner.clone(), lead.clone()];
        assert_eq!(dispatcher_of(None, Some(planner.id), &staff), Dispatcher::Company(planner.id, "Planner".into()), "MU1-10 AC1");
        assert_eq!(dispatcher_of(Some(lead.id), Some(planner.id), &staff), Dispatcher::Project(lead.id, "Lead".into()), "MU1-11 AC1");
        let mut archived = staff.clone();
        archived[1].status = LifecycleStatus::Archived;
        assert_eq!(dispatcher_of(None, None, &staff), Dispatcher::None, "B5");
        assert!(Dispatcher::None.line().contains("until you choose an owner"));
    }

    #[test]
    fn the_board_keys_rest_behind_a_dialog_or_overlay() {
        use crate::app::Overlay;
        assert!(board_keys_live(None, Overlay::None));
        let id = TicketId::new();
        for dialog in [BoardDialog::MarkDone(id), BoardDialog::NewTicket, BoardDialog::SendBack(id), BoardDialog::Columns] {
            assert!(!board_keys_live(Some(dialog), Overlay::None), "{dialog:?}: S would start a card behind it");
        }
        assert!(!board_keys_live(None, Overlay::Menu), "a menu has the keys");
        assert!(!board_keys_live(None, Overlay::Palette));
    }

    #[test]
    fn an_inactive_dispatcher_is_nobody_and_the_company_does_not_take_over() {
        use aurelius_domain::employee::LifecycleStatus;
        let planner = crate::screens::org::tests::manager("Planner", None);
        let lead = crate::screens::org::tests::manager("Lead", None);
        // B5 strict, as `storage::company::dispatcher_for`: the project's own is paused, the company's active.
        let mut staff = vec![planner.clone(), lead.clone()];
        staff[1].status = LifecycleStatus::Paused;
        let paused = dispatcher_of(Some(lead.id), Some(planner.id), &staff);
        assert_eq!(paused, Dispatcher::Inactive { name: "Lead".into(), state: "paused" }, "MU1-11 AC3, B5");
        assert_eq!(paused.name(), None, "nobody chooses");
        assert_eq!(paused.line(), "Lead is paused. Started tickets wait for you to choose an owner.");
        assert_eq!(paused.why_nobody().as_deref(), Some("Lead is paused"));
        assert_eq!(paused.choice(), "Dispatcher (Lead, paused)");
        staff[1].status = LifecycleStatus::Archived;
        let archived = dispatcher_of(Some(lead.id), Some(planner.id), &staff);
        assert_eq!(archived, Dispatcher::Inactive { name: "Lead".into(), state: "archived" });
        assert_eq!(archived.name(), None);
        // The company's own paused: nobody either.
        let mut staff = vec![planner.clone(), lead.clone()];
        staff[0].status = LifecycleStatus::Paused;
        assert_eq!(dispatcher_of(None, Some(planner.id), &staff).name(), None);
        assert_eq!(dispatcher_of(Some(lead.id), Some(planner.id), &staff), Dispatcher::Project(lead.id, "Lead".into()), "the project's own active is enough");
        // An unknown employee is not active either.
        assert_eq!(dispatcher_of(Some(EmployeeId::new()), Some(planner.id), &staff).name(), None);
        // Nobody chooses: the Todo ticket needs the board member, not "is choosing".
        let t = card(1, ColumnKind::Todo);
        assert!(!choosing(&t, paused.name().is_some(), false));
        assert!(needs_you(&t, false, 0).is_some());
    }

    #[test]
    fn feedback_and_the_new_ticket_form_say_what_is_missing() {
        assert!(!feedback_ready("  \n "), "B2: Send back needs feedback");
        assert!(feedback_ready("Add a test."));
        assert_eq!(new_ticket_problems(" ", None), (Some("Required."), Some("Choose a project.")), "MU1-07 AC2");
        assert_eq!(new_ticket_problems(&"x".repeat(161), Some(ProjectId::new())).0, Some("At most 160 characters."));
        assert_eq!(new_ticket_problems("Columns", Some(ProjectId::new())), (None, None));
        assert_eq!(parse_labels(" backend, ui ,, "), ["backend", "ui"]);
    }

    #[test]
    fn columns_are_renamed_and_moved_all_together() {
        let mut columns: Vec<ticket::BoardColumnView> = ColumnKind::ALL
            .iter()
            .enumerate()
            .map(|(i, k)| ticket::BoardColumnView { id: aurelius_domain::BoardColumnId::new(), kind: *k, name: ticket::column_title(*k).into(), position: i as u32, version: 3 })
            .collect();
        assert!(move_column(&mut columns, 1, true), "Todo to the left");
        assert!(!move_column(&mut columns, 1, true), "already the first");
        columns[2].name = " Building ".into();
        let edit = columns_edit(&columns);
        assert_eq!(edit.columns.len(), ColumnKind::ALL.len(), "B3: all of them, none added or dropped");
        assert_eq!(edit.columns[0].column_id, columns[1].id);
        assert_eq!(edit.columns[2].name, "Building");
        assert!(edit.decide(&columns).is_ok(), "the server takes it");
    }

    /// Ticket #5: an older server has no Testing column; the board never shows one for it.
    #[test]
    fn the_testing_column_shows_only_on_a_server_with_board_testing() {
        let shown = |testing: bool| ColumnKind::ALL.into_iter().filter(|k| column_shown(*k, testing)).collect::<Vec<_>>();
        assert_eq!(shown(true), ColumnKind::ALL, "seven columns, Testing between Plan review and Review");
        assert_eq!(shown(true)[4], ColumnKind::Testing);
        assert!(!shown(false).contains(&ColumnKind::Testing));
        assert_eq!(shown(false).len(), 6, "the other six stay");
    }

    /// Ticket #5, as `decide_move`: out of Testing only to Backlog (cancel, asked first);
    /// nothing is dragged into it.
    #[test]
    fn a_card_leaves_testing_only_for_backlog_and_nothing_is_dragged_into_it() {
        use ColumnKind::*;
        assert_eq!(drop_effect(Testing, Backlog, false), Drop::BackToBacklog { confirm: true }, "it stops the tests: asked");
        assert_eq!(drop_effect(Testing, Testing, false), Drop::Reorder);
        for to in [Todo, Doing, PlanReview, Review] {
            assert_eq!(drop_effect(Testing, to, false), Drop::Refused("Testing moves on by itself when the tests have run."), "Testing → {to:?}");
        }
        assert!(!drop_effect(Testing, Done, false).allowed());
        for from in [Backlog, Todo, Doing, PlanReview, Review, Done] {
            assert!(!drop_effect(from, Testing, false).allowed(), "{from:?} → Testing");
            assert!(ticket::decide_move(from, Testing).is_err(), "the server refuses it too");
        }
        assert!(ticket::decide_move(Testing, Backlog).is_ok());
    }

    #[test]
    fn a_card_in_testing_says_it_is_being_tested_and_by_whom() {
        assert_eq!(testing_line(Some("Pulse")), "Being tested by Pulse");
        assert_eq!(testing_line(None), "Being tested");
        assert_eq!(testing_note(Some("Pulse")), "◆ Testing · Pulse runs the full test suite. Green goes on to Review; red back to Doing.");
        assert!(testing_note(None).starts_with("◆ Testing · The tester runs"));
        let t = card(5, ColumnKind::Testing);
        assert_eq!(needs_you(&t, false, 0), None, "nobody decides in Testing: the tests do");
    }

    #[test]
    fn the_test_task_is_the_testers_task_on_the_ticket_the_running_one_first() {
        let t = card(5, ColumnKind::Testing);
        let pulse = EmployeeId::new();
        let on = |status: TaskStatus, employee: Option<EmployeeId>, created: i64, ticket: Option<TicketId>| {
            let mut x = crate::screens::tasks::tests::task(t.project_id, employee, status, created);
            x.created_at_ms = created;
            x.ticket_id = ticket;
            x
        };
        let earlier = on(TaskStatus::Failed, Some(pulse), 1, Some(t.id));
        let running = on(TaskStatus::Running, Some(pulse), 2, Some(t.id));
        let owner = on(TaskStatus::Review, Some(EmployeeId::new()), 3, Some(t.id));
        let elsewhere = on(TaskStatus::Running, Some(pulse), 4, Some(TicketId::new()));
        let tasks = [earlier.clone(), running.clone(), owner, elsewhere];
        assert_eq!(test_task(&tasks, t.id, Some(pulse)).map(|x| x.id), Some(running.id), "the run in progress, not the earlier one");
        assert_eq!(test_task(&tasks[..1], t.id, Some(pulse)).map(|x| x.id), Some(earlier.id), "else the newest finished one");
        assert_eq!(test_task(&tasks, t.id, None), None, "no tester: no test task");
        assert_eq!(test_task(&tasks[2..3], t.id, Some(pulse)), None, "the owner's task is not the test");
    }

    /// Ticket #5: `ticket.tested` reads as words, never as the event's name.
    #[test]
    fn tests_passed_or_failed_read_as_words_never_as_the_event() {
        let tested = |detail: &str| TimelineCard { detail: detail.into(), ..timeline("ticket.tested", "employee") };
        assert_eq!(activity_text(&tested("passed")), "Tests passed");
        assert_eq!(activity_text(&tested("failed")), "Tests failed");
        assert_eq!(activity_text(&tested("ticket")), "The tests ran", "a server that does not word it");
        assert_eq!(activity_detail(&TimelineCard { kind: "outcome".into(), ..tested("passed") }), None, "the result is not said twice");
        // A server that words the line itself: shown as it is.
        let worded = TimelineCard { kind: "outcome".into(), detail: "3 tests failed in storage".into(), ..timeline("Tests failed", "employee") };
        assert_eq!(activity_text(&worded), "Tests failed");
        assert_eq!(activity_detail(&worded), Some("3 tests failed in storage"));
    }

    /// Settings: the tester picker sends only the tester, so the name and the dispatcher stay.
    #[test]
    fn the_tester_picker_sets_only_the_tester_and_none_means_no_testing_step() {
        let pulse = EmployeeId::new();
        let set = company_tester_command(Some(pulse));
        assert_eq!((set.name, set.dispatcher), (None, None));
        assert_eq!(set.tester.and_then(|t| t.employee_id), Some(pulse));
        let none = company_tester_command(None);
        assert_eq!(none.tester, Some(aurelius_domain::company::SetTester { employee_id: None }), "None is sent, not left out");
        assert_eq!(serde_json::to_value(&none).unwrap(), serde_json::json!({ "name": null, "dispatcher": null, "tester": { "employee_id": null } }));
        assert_eq!(NO_TESTER, "None: no Testing step");
        assert_eq!(tester_set_text(Some("Pulse")), "Pulse is the company's tester. Reported tickets go to Testing first.");
        assert_eq!(tester_set_text(None), "No tester: reported tickets go from Doing straight to Review.");
    }

    /// The column editor (B3) with seven columns: Testing renames, moves and restores like the rest.
    #[test]
    fn the_column_editor_renames_moves_and_restores_seven_columns() {
        let mut columns: Vec<ticket::BoardColumnView> = ColumnKind::ALL.iter().rev().enumerate().map(|(i, k)| ticket::BoardColumnView { position: i as u32, ..column(*k) }).collect();
        let testing = columns.iter().position(|c| c.kind == ColumnKind::Testing).unwrap();
        columns[testing].name = "QA".into();
        assert!(move_column(&mut columns, testing, false), "Testing to the right");
        let edit = columns_edit(&columns);
        assert_eq!(edit.columns.len(), 7);
        assert!(edit.columns.iter().any(|c| c.column_id == columns[testing].id && c.name == "QA"));
        assert!(edit.decide(&columns).is_ok(), "the server takes all seven");
        default_columns(&mut columns);
        columns.sort_by_key(|c| c.position);
        assert_eq!(columns.iter().map(|c| c.kind).collect::<Vec<_>>(), ColumnKind::ALL, "Restore defaults puts Testing back between Plan review and Review");
        assert_eq!(columns[4].name, "Testing");
        assert_eq!(columns_edit(&columns).columns.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["Backlog", "Todo", "Doing", "Plan review", "Testing", "Review", "Done"]);
    }

    #[test]
    fn every_empty_column_says_what_lands_there() {
        assert_eq!(empty_text(ColumnKind::PlanReview), "Only tickets in Plan mode stop here, with their plan.");
        for kind in ColumnKind::ALL {
            assert!(!empty_text(kind).is_empty());
        }
    }
}
