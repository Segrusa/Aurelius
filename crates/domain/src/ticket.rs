//! The board and its tickets (protocol 4). One board per company with seven
//! fixed columns (Testing since protocol 5, feature `board_testing`): behaviour hangs on the column's kind, a board member may only
//! rename them and change their order (B3). A ticket sits on top of tasks: its
//! main task (and every delegation below it) carries the ticket's id.
//!
//! The rules here are the board member's side of the workflow: which column a
//! hand may move a card to, and which button works in which column. The engine
//! (starting the main task, the dispatcher, report and accept, plan review,
//! the tester's run of the full test suite) moves cards by itself and is not decided here.

use crate::{BoardColumnId, ConversationId, DomainError, EmployeeId, IdeaId, MessageId, ProjectId, TaskId, TicketId, UserId, optional_text, require_text};
use crate::task::{RunSettings, TaskStatus};
use crate::views::{MessageFileView, TaskView};
use serde::{Deserialize, Serialize};

pub const TITLE_MAX: usize = 160;
pub const DESCRIPTION_MAX: usize = 20_000;
pub const COLUMN_NAME_MAX: usize = 40;
pub const LABELS_MAX: usize = 12;
pub const LABEL_MAX: usize = 40;
pub const FEEDBACK_MAX: usize = 10_000;
pub const REASON_MAX: usize = 500;
/// At most this many files on one ticket, as on a message or task.
pub const ATTACHMENTS_MAX: usize = crate::conversation::UPLOADS_MAX;
/// The last deadline taken: 9999-12-31 (ms since the epoch).
pub const DEADLINE_MAX_MS: i64 = 253_402_300_799_999;
/// Longest link to a Claude Design artifact (a project's styleguide, a ticket's design).
pub const ARTIFACT_URL_MAX: usize = 300;

/// What a column does. The seven kinds always exist, once each.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ColumnKind {
    /// New tickets; nothing starts from here by itself.
    Backlog,
    /// Started: waiting for the dispatcher or for the owner's turn.
    Todo,
    /// The main task runs or waits (for a colleague, an approval, input).
    Doing,
    /// A plan waits for a board member (Plan mode only).
    PlanReview,
    /// The main task reported; the tester runs the full test suite once before
    /// Review (protocol 5, feature `board_testing`). Only with a company tester:
    /// without one, cards go from Doing to Review as before.
    Testing,
    /// The main task reported; a board member accepts it or sends it back.
    Review,
    /// Accepted by a board member; read-only.
    Done,
}

impl ColumnKind {
    /// Every kind, in board order (left to right on a new board).
    pub const ALL: [ColumnKind; 7] = [Self::Backlog, Self::Todo, Self::Doing, Self::PlanReview, Self::Testing, Self::Review, Self::Done];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Backlog => "backlog",
            Self::Todo => "todo",
            Self::Doing => "doing",
            Self::PlanReview => "plan_review",
            Self::Testing => "testing",
            Self::Review => "review",
            Self::Done => "done",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.as_str() == text)
    }
}

/// A ticket's priority (B9). Normal unless chosen.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TicketPriority {
    Low,
    #[default]
    Normal,
    High,
    Urgent,
}

impl TicketPriority {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Normal => "normal",
            Self::High => "high",
            Self::Urgent => "urgent",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        [Self::Low, Self::Normal, Self::High, Self::Urgent].into_iter().find(|p| p.as_str() == text)
    }
}

/// Who changed a ticket's column: a board member (by hand or a button) or an
/// employee (the engine, the dispatcher). In the metadata of `ticket.moved` as `by`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Actor {
    BoardMember { user_id: UserId },
    Employee { employee_id: EmployeeId },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct BoardColumnView {
    pub id: BoardColumnId,
    pub kind: ColumnKind,
    pub name: String,
    /// 0 is the leftmost column.
    pub position: u32,
    pub version: u64,
}

/// A card on the board, and the fields of the ticket view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TicketView {
    pub id: TicketId,
    /// Per company, from 1: what people call it (`#12`).
    pub number: u32,
    pub project_id: ProjectId,
    pub project_name: String,
    pub title: String,
    /// Markdown; none when empty.
    pub description: Option<String>,
    pub column_id: BoardColumnId,
    pub column: ColumnKind,
    /// The order within its column, smallest first.
    pub position: f64,
    pub priority: TicketPriority,
    /// The board member who made it; never changes.
    pub created_by_user: UserId,
    pub created_by_username: String,
    /// The employee whose main task carries the ticket; none until chosen (by
    /// a board member or the dispatcher).
    pub owner_employee_id: Option<EmployeeId>,
    pub owner_name: Option<String>,
    /// Mode and model of the main task (unset: the owner's own).
    pub run: RunSettings,
    pub labels: Vec<String>,
    /// Files attached to the ticket (fetched with `upload.get`).
    pub attachments: Vec<MessageFileView>,
    pub deadline_ms: Option<i64>,
    /// The current main task; earlier ones stay linked through their `ticket_id`.
    pub root_task_id: Option<TaskId>,
    pub root_task_status: Option<TaskStatus>,
    /// The last feedback of a board member (send back, revise plan).
    pub review_feedback: Option<String>,
    pub version: u64,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub started_at_ms: Option<i64>,
    pub done_at_ms: Option<i64>,
    pub archived_at_ms: Option<i64>,
    /// The ticket's discussion (feature `ticket_discussion`): read it with
    /// `conversation.get`, write in it with `ticket.post`. Left out by older servers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<ConversationId>,
    /// Whose Claude login its runs use (protocol 5): the creator, unless a
    /// board member took it over on their own login (`ticket.use_my_login`).
    pub login_user: UserId,
    pub login_username: String,
    /// What the ticket waits for before it can run; none: nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub needs: Option<TicketNeed>,
    /// The board member who is responsible for the ticket (feature
    /// `ticket_responsible`), next to the employee who owns its main task. None:
    /// nobody. Left out then and by older servers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub responsible_user_id: Option<UserId>,
    /// The name of `responsible_user_id`, so a card needs no list of board members.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub responsible_username: Option<String>,
    /// The responsible board member is disabled: a client treats the ticket as
    /// having no active responsible, without a list of board members. Left out when false.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub responsible_disabled: bool,
    /// The ticket's design, a Claude Design artifact (feature `design_links`). A link only:
    /// its runs read the artifact themselves. Left out when none and by older servers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub design_url: Option<String>,
    /// What the ticket waits for from a board member, and from whom (feature
    /// `ticket_state`): the server's one answer, so every client shows the same
    /// "Needs you". None: nothing. Left out then and by older servers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attention: Option<TicketAttention>,
    /// What still works on the ticket (feature `ticket_state`): `ticket.accept`
    /// refuses while this is not empty. Left out when empty and by older servers.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub busy: Vec<String>,
}

/// What a ticket waits for from a board member (`TicketView::attention`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TicketAttention {
    /// One of `TicketAttentionKind`'s names (`login`, `review`, `plan_review`,
    /// `failed`, `approval`, `input`, `choose_owner`, `stalled`). Text, not an enum,
    /// so a new kind never breaks an older client: it shows the ticket as needing someone.
    pub kind: String,
    /// Whom it waits for: the board member whose Claude login is missing, else the
    /// ticket's active Responsible. None: every board member. Anyone may still decide.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_id: Option<UserId>,
}

/// The kinds of `TicketAttention`, on the wire by `as_str`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TicketAttentionKind {
    /// The ticket's login user must connect their Claude login (`TicketNeed::Login`).
    Login,
    /// The report waits for accept or send back.
    Review,
    /// The plan waits for approve or revise.
    PlanReview,
    /// The main task failed: Start begins a new attempt.
    Failed,
    /// A run of the ticket asks permission for an action.
    Approval,
    /// The main task (or the test task) stopped on a question.
    Input,
    /// In Todo without an owner, and no dispatcher is choosing one.
    ChooseOwner,
    /// In Doing or Testing while nothing works on it and it waits for nothing a
    /// board member could give: it would stay there for good. Start again, or cancel.
    Stalled,
}

impl TicketAttentionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Login => "login",
            Self::Review => "review",
            Self::PlanReview => "plan_review",
            Self::Failed => "failed",
            Self::Approval => "approval",
            Self::Input => "input",
            Self::ChooseOwner => "choose_owner",
            Self::Stalled => "stalled",
        }
    }
}

/// What still works on a ticket (`TicketView::busy`, by `as_str`: text, like the kinds).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TicketBusy {
    /// A run of the main task goes, or waits its turn.
    MainTask,
    /// Tasks given to colleagues below the ticket have not ended.
    LinkedTasks,
    /// A handoff or a message of the ticket waits in a colleague's mailbox, or a run that took one goes.
    Colleagues,
    /// The tester's test task has not ended.
    Tests,
}

impl TicketBusy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MainTask => "main_task",
            Self::LinkedTasks => "linked_tasks",
            Self::Colleagues => "colleagues",
            Self::Tests => "tests",
        }
    }
}

/// What is known of a ticket, to say what it needs and what still works on it
/// (`ticket_work`). Every reader and every write that asks "is it ready?" or
/// "who must act?" fills this in and asks there, so there is one answer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TicketFacts {
    pub column: Option<ColumnKind>,
    /// The main task's status and why it waits.
    pub root: Option<crate::task::TaskStatus>,
    pub root_waiting: Option<String>,
    /// A run of the main task goes or is queued, or its job is ready.
    pub root_live: bool,
    pub has_owner: bool,
    /// The dispatcher's job for this ticket is still open.
    pub choosing: bool,
    /// A task of it, or its dispatcher, waits for the login user's Claude login.
    pub login_missing: bool,
    /// Approvals a run of the main task or the test task waits for.
    pub approvals: u32,
    /// Linked tasks other than the main task and the test task that have not ended.
    pub linked_working: u32,
    /// `inbox::open_on_ticket`: handoffs and messages of the ticket still open.
    pub colleagues: bool,
    /// The test task's status while it has not ended, and why it waits.
    pub tests: Option<crate::task::TaskStatus>,
    pub tests_waiting: Option<String>,
}

/// The column a ticket's card belongs in, from its main task: the one table the
/// engine moves the card by and every card is checked against.
///
/// | Main task                                   | Column      |
/// |---------------------------------------------|-------------|
/// | none, or cancelled                          | Backlog     |
/// | Draft (no owner yet)                        | Todo        |
/// | Running, Waiting (not for a plan), Failed   | Doing       |
/// | Waiting for its plan's review               | Plan review |
/// | Review, while its tests are open            | Testing     |
/// | Review                                      | Review      |
/// | Succeeded                                   | Done        |
///
/// None: the status does not say. A queued task waits for the dispatcher or its
/// owner's turn in Todo, and passes through Queued again in Doing when it resumes;
/// one that waits for a Claude login waits wherever it was. The card stays then.
/// `tests_open`: the ticket's test task has not ended, or is about to be made.
pub fn column_of(root: Option<crate::task::TaskStatus>, root_waiting: Option<&str>, tests_open: bool) -> Option<ColumnKind> {
    use crate::task::{TaskStatus as S, WAITING_LOGIN, WAITING_PLAN_REVIEW};
    Some(match root {
        None | Some(S::Cancelled) => ColumnKind::Backlog,
        Some(S::Draft) => ColumnKind::Todo,
        Some(S::Queued) => return None,
        Some(S::Waiting) if root_waiting == Some(WAITING_LOGIN) => return None,
        Some(S::Waiting) if root_waiting == Some(WAITING_PLAN_REVIEW) => ColumnKind::PlanReview,
        Some(S::Running | S::Waiting | S::Failed) => ColumnKind::Doing,
        Some(S::Review) if tests_open => ColumnKind::Testing,
        Some(S::Review) => ColumnKind::Review,
        Some(S::Succeeded) => ColumnKind::Done,
    })
}

impl TicketFacts {
    /// Where the card belongs by `column_of`; none: where it is.
    pub fn belongs_in(&self) -> Option<ColumnKind> {
        column_of(self.root, self.root_waiting.as_deref(), self.tests.is_some())
    }

    /// The card is in another column than its main task says: (where it is, where it belongs).
    /// A cancelled ticket is in Backlog whatever its main task ended as: a failed one
    /// stays failed, it is not cancelled again.
    pub fn misplaced(&self) -> Option<(ColumnKind, ColumnKind)> {
        let (at, to) = (self.column?, self.belongs_in()?);
        if at == ColumnKind::Backlog && self.root == Some(crate::task::TaskStatus::Failed) {
            return None;
        }
        (at != to).then_some((at, to))
    }
}

/// What a ticket needs from a board member and what still works on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TicketWork {
    pub attention: Option<TicketAttentionKind>,
    pub busy: Vec<TicketBusy>,
}

/// The one place that says so. Order matters: a missing login hides everything
/// else (nothing can run), a decision that waits comes before a question.
pub fn ticket_work(f: &TicketFacts) -> TicketWork {
    use crate::task::TaskStatus as S;
    let column = f.column.unwrap_or(ColumnKind::Backlog);
    let mut busy = Vec::new();
    if f.root_live {
        busy.push(TicketBusy::MainTask);
    }
    if f.linked_working > 0 {
        busy.push(TicketBusy::LinkedTasks);
    }
    if f.colleagues {
        busy.push(TicketBusy::Colleagues);
    }
    if f.tests.is_some() {
        busy.push(TicketBusy::Tests);
    }
    let input = |reason: &Option<String>| reason.as_deref() == Some("input");
    let attention = match column {
        ColumnKind::Backlog | ColumnKind::Done => None,
        ColumnKind::Todo | ColumnKind::Doing if f.login_missing => Some(TicketAttentionKind::Login),
        ColumnKind::Review => Some(TicketAttentionKind::Review),
        ColumnKind::PlanReview => Some(TicketAttentionKind::PlanReview),
        ColumnKind::Todo | ColumnKind::Doing if f.root == Some(S::Failed) => Some(TicketAttentionKind::Failed),
        _ if f.approvals > 0 => Some(TicketAttentionKind::Approval),
        ColumnKind::Doing if f.root == Some(S::Waiting) && input(&f.root_waiting) => Some(TicketAttentionKind::Input),
        ColumnKind::Testing if f.tests == Some(S::Waiting) && input(&f.tests_waiting) => Some(TicketAttentionKind::Input),
        ColumnKind::Todo if !f.has_owner && !f.choosing => Some(TicketAttentionKind::ChooseOwner),
        // Nothing works on it and it waits for nothing a board member, the quota or a
        // login could end: without this it stays in its column for good.
        ColumnKind::Doing if busy.is_empty() && matches!((f.root, f.root_waiting.as_deref()), (None | Some(S::Running | S::Succeeded | S::Cancelled), _) | (Some(S::Waiting), Some("dependency"))) => Some(TicketAttentionKind::Stalled),
        ColumnKind::Testing if busy.is_empty() => Some(TicketAttentionKind::Stalled),
        _ => None,
    };
    TicketWork { attention, busy }
}

/// Why a ticket cannot run yet (`TicketView::needs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TicketNeed {
    /// `login_user` has no working Claude login (none, expired, or disabled):
    /// "<name> must connect their Claude login". Another board member may take
    /// it over with `ticket.use_my_login`.
    Login,
}

/// One column with its cards, in order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct BoardColumnCards {
    pub column: BoardColumnView,
    pub tickets: Vec<TicketView>,
}

/// `board.get`: the seven columns left to right, each with its cards (not archived).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct BoardView {
    pub as_of_seq: u64,
    /// The project filter it was read with; none: every project (B4).
    pub project_id: Option<ProjectId>,
    pub columns: Vec<BoardColumnCards>,
}

/// `ticket.get`: the ticket, its main task and every task linked to it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TicketDetail {
    pub as_of_seq: u64,
    pub ticket: TicketView,
    pub root_task: Option<TaskView>,
    /// Every task with this ticket's id (main tasks and delegations), oldest first.
    pub tasks: Vec<TaskView>,
}

/// `ticket.list`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TicketsSnapshot {
    pub as_of_seq: u64,
    pub tickets: Vec<TicketView>,
}

/// Most tickets `ticket.my_tickets` answers with, so one read stays bounded.
pub const MY_TICKETS_MAX: usize = 500;

/// One board member who follows a ticket (`ticket.followers`, feature `ticket_follow`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FollowerView {
    pub user_id: UserId,
    pub username: String,
    /// A disabled board member stays listed but gets no notifications.
    pub disabled: bool,
    /// Why they follow: `created`, `responsible`, `mentioned` or `manual`. Text,
    /// not an enum, so a new reason never breaks an older client.
    pub reason: String,
    pub followed_at_ms: i64,
}

/// `ticket.followers` (feature `ticket_follow`): who follows a ticket.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TicketFollowersView {
    pub followers: Vec<FollowerView>,
}

/// `ticket.my_tickets` (feature `my_tickets`): the tickets of the board member
/// of the session, at most `MY_TICKETS_MAX`. The client makes its tabs and
/// counters from this one answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MyTicketsView {
    pub tickets: Vec<TicketView>,
    /// Which of them the reader follows: `TicketView` is the same for every reader.
    pub following: Vec<TicketId>,
}

// ---- Commands -------------------------------------------------------------

/// A new ticket in Backlog (`start`: straight to Todo, as `ticket.start` would).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateTicket {
    pub project_id: ProjectId,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub priority: TicketPriority,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,
    #[serde(default)]
    pub deadline_ms: Option<i64>,
    /// Chosen now: the dispatcher is skipped when it starts.
    #[serde(default)]
    pub owner_employee_id: Option<EmployeeId>,
    /// Mode and model of the main task; unset fields follow the owner.
    #[serde(default, skip_serializing_if = "RunSettings::is_empty")]
    pub run: RunSettings,
    /// Files to copy into the project's uploads area, as for a task.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub start: bool,
    /// Made from this chat message ("Make ticket", feature `ticket_discussion`):
    /// the discussion starts with a link to it, and its chat gets a link to the ticket.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_message: Option<MessageId>,
    /// A board member confirmed that the text may hold a vault secret's value
    /// (after `looks_like_secret`, B10): it is stored with the value redacted.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub confirm_secret: bool,
    /// The board member responsible from the start (feature `ticket_responsible`).
    /// Left out: nobody. Optional, so a client leaves it out unless it chooses
    /// someone: older servers refuse fields they do not know.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub responsible_user_id: Option<UserId>,
    /// Made from this idea (feature `ticket_from_idea`): the ticket gets copies of
    /// the idea's files, and the idea is done and points at the ticket. Optional,
    /// as above: older servers refuse fields they do not know.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_idea: Option<IdeaId>,
}

/// Choose the board member responsible for a ticket (`ticket.set_responsible`,
/// feature `ticket_responsible`). Its own command rather than a field of
/// `UpdateTicket`, where "nobody" and "unchanged" would need two kinds of none.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetTicketResponsible {
    pub ticket_id: TicketId,
    /// None: nobody is responsible any more.
    pub responsible_user_id: Option<UserId>,
}

/// The board member of the session starts (`on`) or stops following a ticket
/// (`ticket.follow`, feature `ticket_follow`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FollowTicket {
    pub ticket_id: TicketId,
    pub on: bool,
}

/// Change the fields of a ticket. Left out: unchanged. The creator never
/// changes; the owner only through `ticket.assign`; mode and model only while
/// the main task does not run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateTicket {
    pub ticket_id: TicketId,
    #[serde(default)]
    pub title: Option<String>,
    /// An empty text clears it.
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub priority: Option<TicketPriority>,
    /// The whole new list.
    #[serde(default)]
    pub labels: Option<Vec<String>>,
    #[serde(default)]
    pub deadline_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub clear_deadline: bool,
    #[serde(default)]
    pub run: Option<RunSettings>,
    /// Files to add to the ticket's attachments, as `CreateTicket::files`
    /// (feature `ticket_add_files`): the ones it has stay. A main task that
    /// still works gets them too: once its conversation is open, as an update
    /// there that names them, and a Running one gets a run that reads it (a live
    /// run is not interrupted); before that, through its brief. At most
    /// `ATTACHMENTS_MAX` in all.
    /// Optional rather than a list with a default, so a client leaves it out
    /// unless it adds files: older servers refuse fields they do not know.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub add_files: Option<Vec<String>>,
    /// The ticket's design: a Claude Design artifact link (`valid_artifact_url`,
    /// feature `design_links`); an empty text clears it. Left out unless it changes,
    /// so older servers, which refuse unknown fields here, keep working.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub design_url: Option<String>,
    /// A board member confirmed that the text may hold a vault secret's value
    /// (after `looks_like_secret`, B10): it is stored with the value redacted.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub confirm_secret: bool,
}

/// Drag a card. Within its column: only the order. To Todo: start it. To
/// Backlog: cancel it. Done only through `ticket.accept`, and Review back to
/// Doing only through `ticket.send_back`; out of Testing only to Backlog (cancel);
/// other moves follow the work and are refused.
/// The place is relative to a neighbour, so it means the same on a board
/// filtered by project as on the whole board. A move within the same column
/// also gives `ticket.moved` (with `from` = `to`). A card put above itself in
/// its own column stays where it is: no event, the version is unchanged.
/// Unknown fields (such as the old `position`) are refused.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MoveTicket {
    pub ticket_id: TicketId,
    pub column_id: BoardColumnId,
    /// The card it goes right above, in the target column; none: at the bottom.
    #[serde(default)]
    pub before_ticket_id: Option<TicketId>,
}

/// The Start button: Backlog to Todo. With an employee the dispatcher is skipped.
/// Also starts again a ticket whose main task failed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StartTicket {
    pub ticket_id: TicketId,
    #[serde(default)]
    pub employee_id: Option<EmployeeId>,
}

/// Choose the owner of a ticket before its main task exists: a board member
/// (instead of the dispatcher) or the dispatcher itself. Gives `ticket.dispatched`.
///
/// `employee_id` none: no owner again, so the dispatcher chooses when it
/// starts. Only in Backlog (before the start); gives `ticket.updated` with
/// `changed: ["owner"]`, not `ticket.dispatched` (nobody was chosen).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssignTicket {
    pub ticket_id: TicketId,
    pub employee_id: Option<EmployeeId>,
    #[serde(default)]
    pub reason: Option<String>,
}

/// How a plan is approved: carried out in Auto, or with edits accepted one by one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PlanApproval {
    Approve,
    ApproveEdits,
}

/// Plan review → Doing: the owner carries the plan out.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ApprovePlan {
    pub ticket_id: TicketId,
    pub decision: PlanApproval,
    #[serde(default)]
    pub feedback: Option<String>,
}

/// Plan review → Doing in Plan mode: the owner makes a new plan with the feedback.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RevisePlan {
    pub ticket_id: TicketId,
    pub feedback: String,
    /// A board member confirmed that the text may hold a vault secret's value
    /// (after `looks_like_secret`, B10): it is stored with the value redacted.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub confirm_secret: bool,
}

/// Review → Done. Only a board member; only when no linked task still works (B6).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AcceptTicket {
    pub ticket_id: TicketId,
    #[serde(default)]
    pub note: Option<String>,
    /// A board member confirmed that the text may hold a vault secret's value
    /// (after `looks_like_secret`, B10): it is stored with the value redacted.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub confirm_secret: bool,
}

/// Review → Doing with feedback (required) for the owner.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SendBackTicket {
    pub ticket_id: TicketId,
    pub feedback: String,
    /// A board member confirmed that the text may hold a vault secret's value
    /// (after `looks_like_secret`, B10): it is stored with the value redacted.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub confirm_secret: bool,
}

/// Stop the work on a ticket: its main task is cancelled and it goes back to Backlog.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CancelTicket {
    pub ticket_id: TicketId,
}

/// A board member writes in a ticket's discussion (feature `ticket_discussion`).
/// Without `@Name` it goes to the owner while the ticket is in Todo (with an
/// owner), Doing, Plan review, Testing or Review; else it wakes nobody. `@Name` wakes those
/// employees. Refused on Done (the discussion is read-only there). Not for apps.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PostTicketMessage {
    pub ticket_id: TicketId,
    pub text: String,
    /// Images that belong to this message (see `SendChatMessage::images`).
    #[serde(default)]
    pub images: Vec<String>,
    /// Files of any type for the employees (host paths, or this device's upload inbox).
    #[serde(default)]
    pub files: Vec<String>,
    /// A board member confirmed that the text may hold a vault secret's value
    /// (after `looks_like_secret`, B10): it is stored with the value redacted.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub confirm_secret: bool,
    /// A reply in the thread of this message of the discussion (feature
    /// `chat_threads`): a message of the same discussion that is not itself a reply.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_parent: Option<crate::MessageId>,
}

impl PostTicketMessage {
    /// The text as stored (empty only with files).
    pub fn validate(&self) -> Result<String, DomainError> {
        crate::conversation::validate_employee_message(&self.text, &self.images, &self.files)
    }
}

/// Try the summary of a ticket on Done again after it failed (`SystemLineKind::SummaryFailed`).
/// Only a board member; refused when the summary is already there or still being made.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RetryTicketSummary {
    pub ticket_id: TicketId,
}

/// Run a ticket on the Claude login of the board member who asks (B4, D3-3):
/// `TicketView::login_user` becomes that board member, never anyone else. The
/// creator stays. Only a board member; answers `TicketChanged`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UseMyLogin {
    pub ticket_id: TicketId,
}

/// Take a ticket off the board (from Done or Backlog); it stays readable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ArchiveTicket {
    pub ticket_id: TicketId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BoardColumnEdit {
    pub column_id: BoardColumnId,
    pub name: String,
}

/// Rename the columns and set their order (B3): all of them, left to right.
/// `expected_version` is the board's (`board_version`: the columns change
/// together); the answer gives the new one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateBoardColumns {
    pub columns: Vec<BoardColumnEdit>,
}

/// Who chooses the owner of a started ticket. `employee_id` none: for a project,
/// the company's dispatcher; for the company, no dispatcher (tickets wait in
/// Todo for a board member, B5).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SetDispatcher {
    #[serde(default)]
    pub employee_id: Option<EmployeeId>,
}

// ---- Rules ----------------------------------------------------------------

/// Why a hand cannot move a card into or out of Testing (other than to Backlog).
pub const TESTING_MOVES_BY_ITSELF: &str = "Testing moves on by itself when the tests have run.";

fn refused(text: &str) -> DomainError {
    DomainError::Ticket(text.into())
}

/// What a board member's drag does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveEffect {
    /// Within its column: only the order.
    Reorder,
    /// To Todo: as the Start button.
    Start,
    /// To Backlog: as `ticket.cancel`.
    Cancel,
}

/// Decide a drag from `from` to `to` by a board member.
pub fn decide_move(from: ColumnKind, to: ColumnKind) -> Result<MoveEffect, DomainError> {
    use ColumnKind::*;
    match (from, to) {
        (Done, _) => Err(refused("A ticket in Done is closed; it cannot move.")),
        (a, b) if a == b => Ok(MoveEffect::Reorder),
        (_, Done) => Err(refused("Only Accept puts a ticket in Done (from Review).")),
        (Testing, Backlog) => Ok(MoveEffect::Cancel),
        (Testing, _) | (_, Testing) => Err(refused(TESTING_MOVES_BY_ITSELF)),
        (Review, Doing) => Err(refused("Send it back with feedback to put it in Doing again.")),
        (Backlog, Todo) => Ok(MoveEffect::Start),
        (_, Todo) => Err(refused("Only a ticket in Backlog can be started; cancel it first.")),
        (Todo | Doing | PlanReview | Review, Backlog) => Ok(MoveEffect::Cancel),
        (PlanReview, Doing) => Err(refused("Approve the plan or ask for a new one to put it in Doing.")),
        _ => Err(refused("The work moves this ticket by itself; it cannot be put there by hand.")),
    }
}

/// Where a button may be used. `root` is the status of the main task, if any.
pub fn check_start(column: ColumnKind, root: Option<TaskStatus>) -> Result<(), DomainError> {
    match column {
        ColumnKind::Backlog => Ok(()),
        // Start again after a failed main task (B6).
        ColumnKind::Doing | ColumnKind::Todo if root == Some(TaskStatus::Failed) => Ok(()),
        ColumnKind::Todo => Err(refused("This ticket is already started; choose an employee with Assign.")),
        _ => Err(refused("Only a ticket in Backlog (or one whose main task failed) can be started.")),
    }
}

/// In Backlog the work has not started (or its main task was cancelled), so
/// the owner can always be chosen there; in Todo until the main task leaves Draft.
pub fn check_assign(column: ColumnKind, root: Option<TaskStatus>) -> Result<(), DomainError> {
    match (column, root) {
        (ColumnKind::Backlog, _) | (ColumnKind::Todo, None | Some(TaskStatus::Draft)) => Ok(()),
        _ => Err(refused("The owner can only be chosen before the work starts.")),
    }
}

/// No owner again ("the dispatcher chooses"): only before the start (spec §2.2).
/// In Todo the dispatcher may already be choosing, so it is too late there.
pub fn check_unassign(column: ColumnKind) -> Result<(), DomainError> {
    match column {
        ColumnKind::Backlog => Ok(()),
        _ => Err(refused("The owner can only be left to the dispatcher in Backlog, before the start.")),
    }
}

pub fn check_in(column: ColumnKind, wanted: ColumnKind, action: &str) -> Result<(), DomainError> {
    if column == wanted {
        Ok(())
    } else {
        Err(DomainError::Ticket(format!("{action} works on a ticket in {}.", column_title(wanted))))
    }
}

pub fn check_cancel(column: ColumnKind) -> Result<(), DomainError> {
    match column {
        ColumnKind::Todo | ColumnKind::Doing | ColumnKind::PlanReview | ColumnKind::Testing | ColumnKind::Review => Ok(()),
        ColumnKind::Backlog => Err(refused("This ticket has not started.")),
        ColumnKind::Done => Err(refused("A ticket in Done is closed.")),
    }
}

pub fn check_archive(column: ColumnKind) -> Result<(), DomainError> {
    match column {
        ColumnKind::Done | ColumnKind::Backlog => Ok(()),
        _ => Err(refused("Only a ticket in Done or Backlog can be archived; cancel it first.")),
    }
}

/// The default name of a column kind.
pub fn column_title(kind: ColumnKind) -> &'static str {
    match kind {
        ColumnKind::Backlog => "Backlog",
        ColumnKind::Todo => "Todo",
        ColumnKind::Doing => "Doing",
        ColumnKind::PlanReview => "Plan review",
        ColumnKind::Testing => "Testing",
        ColumnKind::Review => "Review",
        ColumnKind::Done => "Done",
    }
}

pub fn valid_title(title: &str) -> Result<String, DomainError> {
    require_text("title", title, TITLE_MAX)
}

pub fn valid_description(text: Option<&str>) -> Result<Option<String>, DomainError> {
    optional_text("description", text, DESCRIPTION_MAX)
}

/// A link to a Claude Design artifact (`https://claude.ai/artifact/<id>` or
/// `https://claude.ai/code/artifact/<uuid>`, nothing behind the id), trimmed; an empty
/// text clears it (`None`). The id's characters are narrow on purpose: the link goes
/// into a run's prompt, and an employee can set a ticket's.
pub fn valid_artifact_url(field: &str, url: &str) -> Result<Option<String>, DomainError> {
    let url = url.trim();
    if url.is_empty() {
        return Ok(None);
    }
    if url.chars().count() > ARTIFACT_URL_MAX {
        return Err(DomainError::TooLong { field: field.into(), max: ARTIFACT_URL_MAX });
    }
    match url.strip_prefix("https://claude.ai/artifact/").or_else(|| url.strip_prefix("https://claude.ai/code/artifact/")) {
        Some(id) if !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_')) => Ok(Some(url.to_owned())),
        _ => Err(DomainError::ArtifactLink(format!("{field} must be a Claude artifact link: https://claude.ai/artifact/<id> or https://claude.ai/code/artifact/<uuid>"))),
    }
}

pub fn valid_feedback(text: &str) -> Result<String, DomainError> {
    require_text("feedback", text, FEEDBACK_MAX)
}

pub fn valid_note(field: &str, text: Option<&str>, max: usize) -> Result<Option<String>, DomainError> {
    optional_text(field, text, max)
}

/// A moment in ms since the epoch, after 1970 and before the year 10000.
pub fn valid_deadline(deadline_ms: Option<i64>) -> Result<Option<i64>, DomainError> {
    match deadline_ms {
        Some(ms) if !(1..=DEADLINE_MAX_MS).contains(&ms) => {
            Err(DomainError::OutOfRange { field: "deadline_ms".into(), detail: "a moment in ms since 1970, before the year 10000".into() })
        }
        other => Ok(other),
    }
}

/// The board's version for `board.columns_update`: the columns change together,
/// so the highest of theirs.
pub fn board_version(columns: &[BoardColumnView]) -> u64 {
    columns.iter().map(|c| c.version).max().unwrap_or(0)
}

/// Trimmed, empty ones dropped, duplicates (case-insensitive) dropped.
pub fn valid_labels(labels: &[String]) -> Result<Vec<String>, DomainError> {
    let mut out: Vec<String> = Vec::new();
    for label in labels {
        let Some(label) = optional_text("labels", Some(label), LABEL_MAX)? else { continue };
        if !out.iter().any(|l| l.eq_ignore_ascii_case(&label)) {
            out.push(label);
        }
    }
    if out.len() > LABELS_MAX {
        return Err(DomainError::OutOfRange { field: "labels".into(), detail: format!("at most {LABELS_MAX}") });
    }
    Ok(out)
}

/// A ticket may hold `count` files in all (the ones it keeps and the new ones).
pub fn check_file_count(count: usize) -> Result<(), DomainError> {
    if count > ATTACHMENTS_MAX {
        return Err(DomainError::File(format!("Too many files: a ticket has at most {ATTACHMENTS_MAX} attachments.")));
    }
    Ok(())
}

impl UpdateBoardColumns {
    /// Every one of the `current` columns once, with valid names; the list order is the new order.
    pub fn decide(&self, current: &[BoardColumnView]) -> Result<Vec<(BoardColumnId, String)>, DomainError> {
        if self.columns.len() != current.len() {
            return Err(refused("List every column of the board once: columns cannot be added or removed."));
        }
        let mut out = Vec::new();
        for edit in &self.columns {
            if !current.iter().any(|c| c.id == edit.column_id) {
                return Err(DomainError::Ticket(format!("Column {} is not on the board.", edit.column_id)));
            }
            if out.iter().any(|(id, _)| *id == edit.column_id) {
                return Err(refused("Each column may appear only once."));
            }
            out.push((edit.column_id, require_text("name", &edit.name, COLUMN_NAME_MAX)?));
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ticket_holds_at_most_eight_files_and_add_files_is_optional_on_the_wire() {
        assert!(check_file_count(ATTACHMENTS_MAX).is_ok());
        assert!(matches!(check_file_count(ATTACHMENTS_MAX + 1), Err(DomainError::File(m)) if m.starts_with("Too many files")));
        let id = TicketId::new();
        let old: UpdateTicket = serde_json::from_value(serde_json::json!({ "ticket_id": id, "title": "x" })).unwrap();
        assert!(old.add_files.is_none());
        assert!(serde_json::to_value(&old).unwrap().get("add_files").is_none(), "an update without files looks as before");
    }

    #[test]
    fn kinds_and_priorities_round_trip() {
        for kind in ColumnKind::ALL {
            assert_eq!(ColumnKind::parse(kind.as_str()), Some(kind));
            assert_eq!(serde_json::to_value(kind).unwrap(), kind.as_str());
        }
        assert_eq!(TicketPriority::default(), TicketPriority::Normal);
        for p in [TicketPriority::Low, TicketPriority::Normal, TicketPriority::High, TicketPriority::Urgent] {
            assert_eq!(TicketPriority::parse(p.as_str()), Some(p));
        }
        assert_eq!(TicketPriority::parse("critical"), None);
    }

    #[test]
    fn a_hand_starts_cancels_or_reorders_and_nothing_else() {
        use ColumnKind::*;
        assert_eq!(decide_move(Doing, Doing), Ok(MoveEffect::Reorder));
        assert_eq!(decide_move(Backlog, Todo), Ok(MoveEffect::Start));
        assert_eq!(decide_move(Review, Backlog), Ok(MoveEffect::Cancel));
        for from in [Backlog, Todo, Doing, PlanReview, Testing, Review] {
            assert!(decide_move(from, Done).is_err(), "{from:?} → Done");
        }
        assert!(decide_move(Review, Doing).is_err());
        assert!(decide_move(Doing, Review).is_err());
        assert!(decide_move(Backlog, Doing).is_err());
        assert!(decide_move(PlanReview, Doing).is_err());
        assert!(decide_move(Done, Backlog).is_err() && decide_move(Done, Done).is_err());
    }

    #[test]
    fn the_board_has_seven_kinds_with_testing_between_plan_review_and_review() {
        use ColumnKind::*;
        assert_eq!(ColumnKind::ALL, [Backlog, Todo, Doing, PlanReview, Testing, Review, Done]);
        assert_eq!(serde_json::to_value(Testing).unwrap(), "testing");
        assert_eq!(serde_json::from_value::<ColumnKind>(serde_json::json!("testing")).unwrap(), Testing);
        assert_eq!(column_title(Testing), "Testing");
    }

    #[test]
    fn a_hand_only_takes_a_card_out_of_testing_to_cancel_it() {
        use ColumnKind::*;
        assert_eq!(decide_move(Testing, Backlog), Ok(MoveEffect::Cancel));
        assert_eq!(decide_move(Testing, Testing), Ok(MoveEffect::Reorder));
        for to in [Todo, Doing, PlanReview, Review] {
            assert_eq!(decide_move(Testing, to), Err(DomainError::Ticket(TESTING_MOVES_BY_ITSELF.into())), "Testing → {to:?}");
        }
        assert!(decide_move(Testing, Done).is_err(), "only Accept puts a ticket in Done");
        for from in [Backlog, Todo, Doing, PlanReview, Review] {
            assert_eq!(decide_move(from, Testing), Err(DomainError::Ticket(TESTING_MOVES_BY_ITSELF.into())), "{from:?} → Testing");
        }
    }

    #[test]
    fn a_ticket_in_testing_can_be_cancelled_but_not_started_assigned_or_archived() {
        assert!(check_cancel(ColumnKind::Testing).is_ok());
        assert!(check_start(ColumnKind::Testing, Some(TaskStatus::Review)).is_err());
        assert!(check_assign(ColumnKind::Testing, Some(TaskStatus::Review)).is_err());
        assert!(check_archive(ColumnKind::Testing).is_err());
    }

    #[test]
    fn buttons_work_in_their_columns() {
        assert!(check_start(ColumnKind::Backlog, None).is_ok());
        assert!(check_start(ColumnKind::Doing, Some(TaskStatus::Failed)).is_ok());
        assert!(check_start(ColumnKind::Doing, Some(TaskStatus::Running)).is_err());
        assert!(check_assign(ColumnKind::Todo, Some(TaskStatus::Draft)).is_ok());
        assert!(check_assign(ColumnKind::Doing, Some(TaskStatus::Running)).is_err());
        // Back in Backlog after a cancel: before the work, so the owner can change (m5).
        assert!(check_assign(ColumnKind::Backlog, Some(TaskStatus::Cancelled)).is_ok());
        assert!(check_assign(ColumnKind::Todo, Some(TaskStatus::Cancelled)).is_err());
        assert!(check_in(ColumnKind::Review, ColumnKind::Review, "Accept").is_ok());
        assert!(check_in(ColumnKind::Doing, ColumnKind::Review, "Accept").is_err());
        assert!(check_cancel(ColumnKind::Backlog).is_err() && check_cancel(ColumnKind::Done).is_err());
        assert!(check_archive(ColumnKind::Done).is_ok() && check_archive(ColumnKind::Doing).is_err());
    }

    #[test]
    fn the_dispatcher_chooses_again_only_in_backlog() {
        assert!(check_unassign(ColumnKind::Backlog).is_ok());
        for column in [ColumnKind::Todo, ColumnKind::Doing, ColumnKind::PlanReview, ColumnKind::Testing, ColumnKind::Review, ColumnKind::Done] {
            assert!(check_unassign(column).is_err(), "{column:?}");
        }
    }

    #[test]
    fn only_a_claude_artifact_link_is_taken_and_empty_clears() {
        assert_eq!(valid_artifact_url("design_url", "  "), Ok(None));
        for good in ["https://claude.ai/artifact/9sfwWLbDbQerBZ3jmBSkXS", "https://claude.ai/code/artifact/8f14e45f-ceea-467f-a22a-2b7a2c55e3b1", " https://claude.ai/artifact/a_b-1 "] {
            assert_eq!(valid_artifact_url("design_url", good), Ok(Some(good.trim().to_owned())), "{good}");
        }
        for bad in [
            "http://claude.ai/artifact/abc",
            "https://claude.ai/artifact/",
            "https://claude.ai/artifact/abc/x",
            "https://claude.ai/artifact/abc?share=1",
            "https://claude.ai/artifact/abc def",
            "https://claude.ai/chat/abc",
            "https://claude.ai/code/artifact/",
            "https://claude.ai.evil.com/artifact/abc",
            "javascript:alert(1)",
            "claude.ai/artifact/abc",
        ] {
            assert!(matches!(valid_artifact_url("design_url", bad), Err(DomainError::ArtifactLink(m)) if m.contains("https://claude.ai/artifact/<id>")), "{bad}");
        }
        let long = format!("https://claude.ai/artifact/{}", "a".repeat(ARTIFACT_URL_MAX));
        assert_eq!(valid_artifact_url("design_url", &long), Err(DomainError::TooLong { field: "design_url".into(), max: ARTIFACT_URL_MAX }));
    }

    #[test]
    fn a_deadline_is_a_moment_after_1970_and_before_10000() {
        assert_eq!(valid_deadline(None), Ok(None));
        assert_eq!(valid_deadline(Some(1_800_000_000_000)), Ok(Some(1_800_000_000_000)));
        assert_eq!(valid_deadline(Some(DEADLINE_MAX_MS)), Ok(Some(DEADLINE_MAX_MS)));
        for bad in [-5, 0, DEADLINE_MAX_MS + 1, i64::MAX] {
            assert!(matches!(valid_deadline(Some(bad)), Err(DomainError::OutOfRange { .. })), "{bad}");
        }
    }

    #[test]
    fn commands_refuse_unknown_fields() {
        let t = TicketId::new();
        let c = BoardColumnId::new();
        let old = serde_json::json!({ "ticket_id": t, "column_id": c, "position": 0 });
        assert!(serde_json::from_value::<MoveTicket>(old).unwrap_err().to_string().contains("unknown field `position`"));
        let now = serde_json::json!({ "ticket_id": t, "column_id": c, "before_ticket_id": null });
        assert!(serde_json::from_value::<MoveTicket>(now).is_ok());
        assert!(serde_json::from_value::<UpdateTicket>(serde_json::json!({ "ticket_id": t, "owner_employee_id": null })).is_err());
        assert!(serde_json::from_value::<UpdateBoardColumns>(serde_json::json!({ "columns": [], "version": 1 })).is_err());
        // The owner left to the dispatcher: `null` or left out.
        let none: AssignTicket = serde_json::from_value(serde_json::json!({ "ticket_id": t, "employee_id": null })).unwrap();
        assert_eq!(none.employee_id, None);
    }

    #[test]
    fn the_board_version_is_the_highest_of_its_columns() {
        let column = |version| BoardColumnView { id: BoardColumnId::new(), kind: ColumnKind::Todo, name: "Todo".into(), position: 0, version };
        assert_eq!(board_version(&[column(3), column(4), column(4)]), 4);
        assert_eq!(board_version(&[]), 0);
    }

    #[test]
    fn labels_are_trimmed_and_unique() {
        assert_eq!(valid_labels(&[" ui ".into(), "UI".into(), "".into(), "api".into()]).unwrap(), vec!["ui", "api"]);
        assert!(valid_labels(&(0..13).map(|i| format!("l{i}")).collect::<Vec<_>>()).is_err());
    }

    #[test]
    fn columns_can_be_renamed_and_reordered_but_not_added_or_dropped() {
        let current: Vec<BoardColumnView> = ColumnKind::ALL
            .iter()
            .enumerate()
            .map(|(i, k)| BoardColumnView { id: BoardColumnId::new(), kind: *k, name: column_title(*k).into(), position: i as u32, version: 1 })
            .collect();
        let mut edits: Vec<BoardColumnEdit> = current.iter().rev().map(|c| BoardColumnEdit { column_id: c.id, name: format!(" {} ", c.name) }).collect();
        let decided = UpdateBoardColumns { columns: edits.clone() }.decide(&current).unwrap();
        assert_eq!(decided[0], (current[current.len() - 1].id, "Done".into()));
        edits.pop();
        assert!(UpdateBoardColumns { columns: edits.clone() }.decide(&current).is_err(), "one missing");
        edits.push(edits[0].clone());
        assert!(UpdateBoardColumns { columns: edits.clone() }.decide(&current).is_err(), "one twice");
        edits.pop();
        edits.push(BoardColumnEdit { column_id: BoardColumnId::new(), name: "Extra".into() });
        assert!(UpdateBoardColumns { columns: edits }.decide(&current).is_err(), "an unknown one");
        let blank = current.iter().map(|c| BoardColumnEdit { column_id: c.id, name: " ".into() }).collect();
        assert!(UpdateBoardColumns { columns: blank }.decide(&current).is_err());
    }
}

#[cfg(test)]
mod work_tests {
    use super::*;
    use crate::task::TaskStatus as S;

    fn facts(column: ColumnKind, root: Option<S>) -> TicketFacts {
        TicketFacts { column: Some(column), root, has_owner: true, ..Default::default() }
    }

    #[test]
    fn a_decision_that_waits_is_what_the_ticket_needs() {
        assert_eq!(ticket_work(&facts(ColumnKind::Review, Some(S::Review))).attention, Some(TicketAttentionKind::Review));
        assert_eq!(ticket_work(&facts(ColumnKind::PlanReview, Some(S::Waiting))).attention, Some(TicketAttentionKind::PlanReview));
        assert_eq!(ticket_work(&facts(ColumnKind::Doing, Some(S::Failed))).attention, Some(TicketAttentionKind::Failed));
        let asked = TicketFacts { root_waiting: Some("input".into()), ..facts(ColumnKind::Doing, Some(S::Waiting)) };
        assert_eq!(ticket_work(&asked).attention, Some(TicketAttentionKind::Input));
        let approval = TicketFacts { approvals: 1, root_live: true, ..facts(ColumnKind::Doing, Some(S::Running)) };
        assert_eq!(ticket_work(&approval).attention, Some(TicketAttentionKind::Approval));
        // Closed or not started: nothing.
        assert_eq!(ticket_work(&facts(ColumnKind::Backlog, None)).attention, None);
        assert_eq!(ticket_work(&facts(ColumnKind::Done, Some(S::Succeeded))).attention, None);
    }

    #[test]
    fn a_missing_login_comes_first_and_an_owner_is_asked_only_when_nobody_chooses() {
        let login = TicketFacts { login_missing: true, ..facts(ColumnKind::Doing, Some(S::Failed)) };
        assert_eq!(ticket_work(&login).attention, Some(TicketAttentionKind::Login));
        let unowned = TicketFacts { has_owner: false, ..facts(ColumnKind::Todo, Some(S::Draft)) };
        assert_eq!(ticket_work(&unowned).attention, Some(TicketAttentionKind::ChooseOwner));
        assert_eq!(ticket_work(&TicketFacts { choosing: true, ..unowned }).attention, None);
    }

    #[test]
    fn work_that_goes_is_busy_and_work_that_stopped_for_no_reason_is_stalled() {
        let running = TicketFacts { root_live: true, ..facts(ColumnKind::Doing, Some(S::Running)) };
        assert_eq!(ticket_work(&running), TicketWork { attention: None, busy: vec![TicketBusy::MainTask] });
        // Running with no run behind it, or waiting for colleagues who are all done.
        assert_eq!(ticket_work(&facts(ColumnKind::Doing, Some(S::Running))).attention, Some(TicketAttentionKind::Stalled));
        let waits = TicketFacts { root_waiting: Some("dependency".into()), ..facts(ColumnKind::Doing, Some(S::Waiting)) };
        assert_eq!(ticket_work(&waits).attention, Some(TicketAttentionKind::Stalled));
        assert_eq!(ticket_work(&TicketFacts { colleagues: true, ..waits.clone() }), TicketWork { attention: None, busy: vec![TicketBusy::Colleagues] });
        assert_eq!(ticket_work(&TicketFacts { linked_working: 2, ..waits }).busy, vec![TicketBusy::LinkedTasks]);
        // The quota, a pause or its turn end by themselves.
        for reason in ["quota", "paused"] {
            let waiting = TicketFacts { root_waiting: Some(reason.into()), ..facts(ColumnKind::Doing, Some(S::Waiting)) };
            assert_eq!(ticket_work(&waiting).attention, None, "{reason}");
        }
        assert_eq!(ticket_work(&facts(ColumnKind::Doing, Some(S::Queued))).attention, None);
    }

    #[test]
    fn testing_is_busy_while_the_tests_run_and_asks_when_they_stop() {
        let testing = TicketFacts { tests: Some(S::Running), ..facts(ColumnKind::Testing, Some(S::Review)) };
        assert_eq!(ticket_work(&testing), TicketWork { attention: None, busy: vec![TicketBusy::Tests] });
        let asked = TicketFacts { tests: Some(S::Waiting), tests_waiting: Some("input".into()), ..facts(ColumnKind::Testing, Some(S::Review)) };
        assert_eq!(ticket_work(&asked).attention, Some(TicketAttentionKind::Input));
        assert_eq!(ticket_work(&facts(ColumnKind::Testing, Some(S::Review))).attention, Some(TicketAttentionKind::Stalled));
    }

    #[test]
    fn the_card_belongs_where_its_main_task_says() {
        assert_eq!(column_of(None, None, false), Some(ColumnKind::Backlog));
        assert_eq!(column_of(Some(S::Cancelled), None, false), Some(ColumnKind::Backlog));
        assert_eq!(column_of(Some(S::Draft), None, false), Some(ColumnKind::Todo));
        assert_eq!(column_of(Some(S::Running), None, false), Some(ColumnKind::Doing));
        assert_eq!(column_of(Some(S::Failed), None, false), Some(ColumnKind::Doing));
        assert_eq!(column_of(Some(S::Waiting), Some("input"), false), Some(ColumnKind::Doing));
        assert_eq!(column_of(Some(S::Waiting), Some("plan_review"), false), Some(ColumnKind::PlanReview));
        assert_eq!(column_of(Some(S::Review), None, true), Some(ColumnKind::Testing));
        assert_eq!(column_of(Some(S::Review), None, false), Some(ColumnKind::Review));
        assert_eq!(column_of(Some(S::Succeeded), None, false), Some(ColumnKind::Done));
        // Queued and waiting for a login do not say: the card stays.
        assert_eq!(column_of(Some(S::Queued), None, false), None);
        assert_eq!(column_of(Some(S::Waiting), Some("login"), false), None);
        let stuck = TicketFacts { tests: None, ..facts(ColumnKind::Testing, Some(S::Review)) };
        assert_eq!(stuck.misplaced(), Some((ColumnKind::Testing, ColumnKind::Review)));
        assert_eq!(facts(ColumnKind::Doing, Some(S::Queued)).misplaced(), None);
        assert_eq!(facts(ColumnKind::Review, Some(S::Review)).misplaced(), None);
    }

    #[test]
    fn a_cancelled_ticket_stays_in_backlog_with_its_failed_main_task() {
        assert_eq!(facts(ColumnKind::Backlog, Some(S::Failed)).misplaced(), None);
        assert_eq!(facts(ColumnKind::Todo, Some(S::Failed)).misplaced(), Some((ColumnKind::Todo, ColumnKind::Doing)));
    }
}
