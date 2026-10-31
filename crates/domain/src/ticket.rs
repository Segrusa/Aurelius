//! The board and its tickets (protocol 4). One board per company with six
//! fixed columns: behaviour hangs on the column's kind, a board member may only
//! rename them and change their order (B3). A ticket sits on top of tasks: its
//! main task (and every delegation below it) carries the ticket's id.
//!
//! The rules here are the board member's side of the workflow: which column a
//! hand may move a card to, and which button works in which column. The engine
//! (starting the main task, the dispatcher, report and accept, plan review)
//! moves cards by itself and is not decided here.

use crate::{BoardColumnId, ConversationId, DomainError, EmployeeId, MessageId, ProjectId, TaskId, TicketId, UserId, optional_text, require_text};
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
/// The last deadline taken: 9999-12-31 (ms since the epoch).
pub const DEADLINE_MAX_MS: i64 = 253_402_300_799_999;

/// What a column does. The six kinds always exist, once each.
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
    /// The main task reported; a board member accepts it or sends it back.
    Review,
    /// Accepted by a board member; read-only.
    Done,
}

impl ColumnKind {
    pub const ALL: [ColumnKind; 6] = [Self::Backlog, Self::Todo, Self::Doing, Self::PlanReview, Self::Review, Self::Done];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Backlog => "backlog",
            Self::Todo => "todo",
            Self::Doing => "doing",
            Self::PlanReview => "plan_review",
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

/// `board.get`: the six columns left to right, each with its cards (not archived).
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
    /// A board member confirmed that the text may hold a vault secret's value
    /// (after `looks_like_secret`, B10): it is stored with the value redacted.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub confirm_secret: bool,
}

/// Drag a card. Within its column: only the order. To Todo: start it. To
/// Backlog: cancel it. Done only through `ticket.accept`, and Review back to
/// Doing only through `ticket.send_back`; other moves follow the work and are refused.
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
/// owner), Doing or Plan review; else it wakes nobody. `@Name` wakes those
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

/// Rename the columns and set their order (B3): all six, left to right.
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
        ColumnKind::Todo | ColumnKind::Doing | ColumnKind::PlanReview | ColumnKind::Review => Ok(()),
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
        for from in [Backlog, Todo, Doing, PlanReview, Review] {
            assert!(decide_move(from, Done).is_err(), "{from:?} → Done");
        }
        assert!(decide_move(Review, Doing).is_err());
        assert!(decide_move(Doing, Review).is_err());
        assert!(decide_move(Backlog, Doing).is_err());
        assert!(decide_move(PlanReview, Doing).is_err());
        assert!(decide_move(Done, Backlog).is_err() && decide_move(Done, Done).is_err());
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
        for column in [ColumnKind::Todo, ColumnKind::Doing, ColumnKind::PlanReview, ColumnKind::Review, ColumnKind::Done] {
            assert!(check_unassign(column).is_err(), "{column:?}");
        }
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
        assert_eq!(decided[0], (current[5].id, "Done".into()));
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
