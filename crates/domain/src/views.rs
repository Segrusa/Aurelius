//! Read models: what the storage layer returns and the protocol sends. Plain data, no behavior.
//!
//! They live in `domain` so that `storage` and `application` never depend on the wire crate.

use crate::{
    ApprovalId, AttachmentId, ConversationId, EmployeeId, MemoryId, MessageId, ProjectId,
    ProjectStatus, ProposalId, ProviderId, RunId, TaskId, TicketId,
    approval::ApprovalStatus,
    connection::{ConfigScope, Purpose, RunDefaults},
    conversation::{MessageRole, MessageStatus},
    employee::LifecycleStatus,
    proposal::ProposalStatus,
    task::TaskStatus,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ProjectSummary {
    pub id: ProjectId,
    pub name: String,
    pub description: Option<String>,
    pub status: ProjectStatus,
    pub version: u64,
    pub created_at_ms: i64,
    /// Employees with an open task in this project (derived; employees are company-wide).
    pub employees_active: u32,
    /// The project's own folder on disk.
    pub folder: Option<String>,
    /// The project's own dispatcher for tickets; none: the company's (B5).
    #[serde(default)]
    pub dispatcher_employee_id: Option<EmployeeId>,
    /// The workspace's styleguide, a Claude Design artifact (feature `design_links`). A
    /// link only: runs read the artifact themselves. Left out when none and by older servers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub styleguide_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ProjectsSnapshot {
    /// Subscribe after this cursor to stay consistent.
    pub as_of_seq: u64,
    pub projects: Vec<ProjectSummary>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TaskView {
    pub id: TaskId,
    pub project_id: ProjectId,
    pub employee_id: Option<EmployeeId>,
    pub employee_name: Option<String>,
    pub title: String,
    pub goal: String,
    pub criteria: Vec<String>,
    pub status: TaskStatus,
    /// Why a waiting task waits. A queued task that does not start now has one
    /// too (derived, never stored): `in_line` (its employee works on one task at
    /// a time and another goes first) or `scheduled` (its start is planned for
    /// later, see `starts_at_ms`). Such a task is not at work: leave it out of
    /// active and at-work counts. Older servers leave it out for queued tasks.
    pub waiting_reason: Option<String>,
    /// While it waits for the usage limit ("quota"): when its run goes again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waiting_until_ms: Option<i64>,
    /// A `scheduled` queued task: when it may start (none when not known).
    #[serde(default)]
    pub starts_at_ms: Option<i64>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub result: Option<String>,
    /// Actions (and questions) of this task waiting for the user.
    pub pending_approvals: u32,
    /// The task this one was delegated from (a sub-task), and its employee.
    pub parent_task_id: Option<TaskId>,
    pub delegated_by: Option<String>,
    /// Sub-tasks this task's employee delegated to colleagues.
    pub children: Vec<TaskId>,
    /// Hand-overs between employees, oldest first.
    pub handoffs: Vec<TaskHandoff>,
    /// The conversation of the task with its current employee (the task is its
    /// context; plan, findings and results come back there).
    pub conversation_id: Option<ConversationId>,
    /// Mode, connection, model and effort chosen for the task (unset: the employee's).
    pub run: crate::task::RunSettings,
    /// Files attached when the task was made, in the work folder's uploads
    /// area. Left out when there are none.
    pub attachments: Vec<MessageFileView>,
    /// The recurring task this one was made from.
    pub schedule_id: Option<crate::TaskScheduleId>,
    /// A finished task (succeeded, failed or cancelled) in whose conversation a
    /// run goes on or waits to start, after the user (or a manager) wrote to it.
    /// The status does not change: clients show such a task as working or
    /// queued, not as done. A task in Review has it too: Review only says the
    /// report is in, this says the employee is answering a message about it.
    /// Derived, never stored; left out when there is none and for other tasks
    /// that are not finished (their `status` says it). Older servers leave it out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live_run: Option<LiveRun>,
    /// The ticket this task works for (a main task or a delegation below it), and its number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticket_id: Option<crate::TicketId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticket_number: Option<u32>,
}

/// What a finished task's follow-up run does (`TaskView::live_run`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LiveRun {
    /// A run is going (created, starting, running, waiting or cancelling).
    Running,
    /// Its run waits in the queue and has not been claimed yet.
    Queued,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TasksSnapshot {
    pub as_of_seq: u64,
    pub tasks: Vec<TaskView>,
}

/// One hand-over of a task to a colleague who suits it better.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TaskHandoff {
    pub from: String,
    pub to: String,
    pub reason: String,
    pub from_id: Option<EmployeeId>,
    pub to_id: Option<EmployeeId>,
}

/// Recurring tasks. Times are epoch milliseconds; the recurrence is
/// evaluated in the server's local time zone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TaskSchedulesSnapshot {
    pub schedules: Vec<TaskScheduleView>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TaskScheduleView {
    pub id: crate::TaskScheduleId,
    pub project_id: ProjectId,
    pub employee_id: EmployeeId,
    pub employee_name: String,
    pub title: String,
    pub goal: String,
    pub criteria: Vec<String>,
    /// Copies in the work folder, attached to every task made from it.
    pub attachments: Vec<MessageFileView>,
    pub run: crate::task::RunSettings,
    pub recurrence: crate::schedule::Recurrence,
    pub paused: bool,
    /// When it is due next; none while paused.
    pub next_run_ms: Option<i64>,
    /// When it last made a task, and which one.
    pub last_run_ms: Option<i64>,
    pub last_task_id: Option<TaskId>,
    /// When a due run was skipped because the previous task was still open.
    pub last_skipped_ms: Option<i64>,
    /// The task from this schedule that is still open, if any.
    pub open_task_id: Option<TaskId>,
    pub created_at_ms: i64,
}

/// A project's ideas.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct IdeasSnapshot {
    pub project_id: ProjectId,
    pub ideas: Vec<IdeaView>,
}

/// The ideas of every project; each idea carries its own `project_id`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AllIdeasSnapshot {
    pub ideas: Vec<IdeaView>,
}

/// One idea: noted by the user or an employee; it may have become a task.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct IdeaView {
    pub id: crate::IdeaId,
    pub project_id: ProjectId,
    pub title: String,
    pub note: Option<String>,
    pub status: crate::idea::IdeaStatus,
    /// Feature or bug;
    pub kind: crate::idea::IdeaKind,
    /// "You", or the employee who noted it.
    pub author: String,
    /// The employee who noted it; None when the user did.
    pub employee_id: Option<EmployeeId>,
    /// The task it became, and that task's title and status.
    pub task_id: Option<TaskId>,
    pub task_status: Option<TaskStatus>,
    /// The ticket made from it and that ticket's number (feature `ticket_from_idea`);
    /// left out when there is none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticket_id: Option<crate::TicketId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticket_number: Option<u32>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    /// Files attached to the idea, each with an `id` for `upload.get` (feature
    /// `idea_attachments`); empty, and left out on older servers.
    #[serde(default)]
    pub attachments: Vec<MessageFileView>,
}

impl IdeaView {
    /// Noted by an employee rather than the user.
    pub fn by_employee(&self) -> bool {
        self.employee_id.is_some() || self.author != "You"
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[schemars(rename = "ConnectionView")]
pub struct ProviderView {
    pub id: ProviderId,
    pub provider: String,
    pub display_name: String,
    pub auth_kind: String,
    pub billing: String,
    /// disconnected | ready | needs_auth | unavailable
    pub status: String,
    pub has_secret: bool,
    pub runtime_path: Option<String>,
    pub default_model: Option<String>,
    pub models: Vec<String>,
    pub detail: Option<String>,
    pub last_verified_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DefaultsView {
    pub scope: ConfigScope,
    pub purpose: Purpose,
    pub settings: RunDefaults,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[schemars(rename = "ConnectionsSnapshot")]
pub struct ProvidersSnapshot {
    pub as_of_seq: u64,
    pub connections: Vec<ProviderView>,
    pub defaults: Vec<DefaultsView>,
}

/// Everything the user needs to judge one exact action.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ApprovalView {
    pub id: ApprovalId,
    pub status: ApprovalStatus,
    pub tool: String,
    pub effect: String,
    pub arguments: serde_json::Value,
    pub resources: Vec<String>,
    pub account: String,
    pub reasons: Vec<String>,
    pub action_digest: String,
    pub requested_at_ms: i64,
    pub expires_at_ms: i64,
    pub run_id: RunId,
    pub conversation_id: Option<ConversationId>,
    /// Who asked: the employee's name, or "Chat".
    pub requester: String,
    /// The task, when a task's conversation asked.
    pub task_id: Option<TaskId>,
    /// Whether clients may offer 'Always allow' (an MCP tool of the user's
    /// servers or Claude in Chrome); answered with `DecideApproval::always_allow`.
    #[serde(default)]
    pub always_allow_eligible: bool,
    /// The ticket it waits on: of the run's task, or of the discussion that woke
    /// the run (feature `run_cards`). Left out for other runs and by older servers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticket_id: Option<crate::TicketId>,
    /// The discussion (a channel, a ticket's) that woke the run (feature `run_cards`).
    /// Left out for other runs and by older servers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discussion_id: Option<ConversationId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ApprovalsSnapshot {
    pub as_of_seq: u64,
    pub approvals: Vec<ApprovalView>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct EmployeeSummary {
    pub id: EmployeeId,
    pub name: String,
    pub role: String,
    pub title: Option<String>,
    /// Who this employee reports to; none: a root of the org chart (the management team).
    pub reports_to: Option<EmployeeId>,
    /// What other employees read about it (P7). Left out when it has none: their
    /// prompt then shows the first line of its instructions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// What its runs may do with files and commands (C3); left out when `Inherit`.
    #[serde(default, skip_serializing_if = "crate::employee::AccessLevel::is_inherit")]
    pub access_level: crate::employee::AccessLevel,
    /// Its tools, MCP servers and what asks first (C2); left out when it has all of its level.
    #[serde(default, skip_serializing_if = "crate::employee_tools::ToolPolicy::is_default")]
    pub tool_policy: crate::employee_tools::ToolPolicy,
    /// Its own provider, model and effort (C1); left out when it inherits them all.
    /// The same as `connection.list` has for its scope, without needing that read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<crate::connection::RunDefaults>,
    /// What it is doing now (U6, derived, never stored); left out when idle.
    #[serde(default, skip_serializing_if = "crate::employee::ActivityState::is_idle")]
    pub activity_state: crate::employee::ActivityState,
    /// Projects it has open tasks in (derived, never stored).
    pub projects_active: Vec<ProjectId>,
    pub instructions: Option<String>,
    pub status: LifecycleStatus,
    pub version: u64,
    /// Its persistent chat with the user (company scope, no project; `employee.chat`).
    pub conversation_id: ConversationId,
    /// Derived presence: a run of this employee is active (never stored).
    pub working: bool,
    pub agent_mode: crate::autonomy::AgentMode,
    /// The agent may use the user's browser (shares their logins); off by default.
    pub browser_allowed: bool,
    /// The agent may take screenshots without asking each time; off by default (it then asks).
    pub screen_allowed: bool,
    /// Own workspace folder, if any: the work folder of its chat (in a task the project folder applies).
    pub folder: Option<String>,
    /// It works on one task at a time; the others wait their turn.
    pub sequential_tasks: bool,
    /// Its chats, most recent first: the persistent one (`conversation_id`, it
    /// carries mode and memory summaries) and older ones from before
    /// protocol 3 (history). Task conversations are not listed.
    pub conversations: Vec<ConversationSummary>,
    /// What runs for this employee right now: its own steps,
    /// subagents and sub-tasks delegated to colleagues (derived, never stored).
    pub agents: Vec<AgentView>,
}

/// One agent at work for an employee.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AgentView {
    /// conversation | subagent | delegated
    pub kind: String,
    pub name: String,
    pub detail: String,
    pub task_id: Option<TaskId>,
    pub run_id: Option<RunId>,
    /// The employee's conversation this work shows in (a chat, or the task's).
    pub conversation_id: Option<ConversationId>,
    pub since_ms: i64,
    /// A subagent: what it was asked (shortened).
    pub prompt: Option<String>,
    /// A subagent: its latest step, e.g. "Grep · poll_git_status".
    pub doing: Option<String>,
    /// A subagent: how many tools it used so far.
    pub steps: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct EmployeesSnapshot {
    pub as_of_seq: u64,
    pub employees: Vec<EmployeeSummary>,
}

/// The company in one read (`company.get`): employees, the org chart setting
/// and its roots (the management team).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CompanySnapshot {
    pub as_of_seq: u64,
    /// Employees that are not archived, oldest first.
    pub employees: Vec<EmployeeSummary>,
    /// "Follow the org chart" (server setting, on by default).
    pub follow_org_chart: bool,
    /// Active employees without a boss, oldest first: the management team.
    pub roots: Vec<EmployeeId>,
    /// The company itself: its name and default dispatcher.
    pub company: crate::company::CompanyView,
    /// Whose Claude login runs the background jobs (B6); none: they do not run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background_login: Option<crate::user::UserView>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ConversationSummary {
    pub id: ConversationId,
    pub title: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub message_count: u32,
}

/// A conversation in a project's Conversations tab.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ProjectConversationSummary {
    pub id: ConversationId,
    pub title: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub message_count: u32,
    /// An answer is queued or running in it.
    pub working: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ProjectConversationsSnapshot {
    pub as_of_seq: u64,
    pub project_id: ProjectId,
    /// Most recently active first.
    pub conversations: Vec<ProjectConversationSummary>,
}

/// How many of a thread's repliers its root names (`MessageView::thread_repliers`).
pub const THREAD_REPLIERS_MAX: usize = 3;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MessageView {
    pub id: MessageId,
    pub ordinal: u32,
    pub role: MessageRole,
    pub status: MessageStatus,
    pub text: String,
    pub created_at_ms: i64,
    /// Run that produced an assistant message.
    pub run_id: Option<RunId>,
    /// For retried answers: the user message this answers again.
    pub reply_to: Option<MessageId>,
    /// "Connection · model" that produced an assistant message.
    pub produced_by: Option<String>,
    /// Persona that answered (e.g. "Marcus"), if any.
    pub author: Option<String>,
    /// Web sources fetched for this answer, cited as [n].
    pub sources: Vec<SourceView>,
    /// Tools the runtime used while answering (commands, edits, web searches).
    pub activity: Vec<ActivityView>,
    /// A task update in the task's conversation, not an answer: brief |
    /// finding | question | status. `author` says from whom (a
    /// subagent, a colleague).
    pub update: Option<String>,
    /// For an `approval` update: the approval it asks for (decided here or in the inbox).
    pub approval_id: Option<ApprovalId>,
    /// Files the message mentions, kept with it (open them with `query/file`).
    pub files: Vec<FileRef>,
    /// Files the user uploaded with this message into the work folder.
    pub uploads: Vec<MessageFileView>,
    /// A user message whose answer has not started yet: it waits behind the
    /// answer running in its conversation. Such a message can be deleted
    /// (`DeleteQueuedMessage`) or sent now (`SendQueuedNow`). Left out when false.
    pub queued: bool,
    /// The tokens the answer's run used, once its runtime reported them (feature
    /// `run_usage`). Left out for user messages and by older servers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<TokenUsage>,
    /// An employee's answer in the group chat: the mode its run started in.
    /// Left out for other messages and by older servers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<crate::autonomy::AgentMode>,
    /// The board member who wrote a user message in a discussion (a ticket's,
    /// a channel's; feature `ticket_discussion`). Left out for other messages
    /// and by older servers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author_user: Option<crate::user::UserView>,
    /// A system line of Aurelius in a discussion (feature `ticket_discussion`):
    /// what happened, shown as a compact grey line; `text` is its readable form.
    /// Such a line never wakes anyone. Left out for other messages and by older servers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system: Option<SystemLine>,
    /// A reply in a thread (feature `chat_threads`): the message of the channel
    /// it answers (the thread's root, never itself a reply). Replies are only in
    /// `thread.read`, never in the channel's own list. Left out for a message of
    /// the channel itself and by older servers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_parent: Option<MessageId>,
    /// A message that starts a thread (feature `chat_threads`): how many replies it has.
    /// Left out when it has none and by older servers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_reply_count: Option<u32>,
    /// A message that starts a thread (feature `chat_threads`): when its latest reply was written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_last_reply_ms: Option<i64>,
    /// A message that starts a thread (feature `chat_threads`): how many of its
    /// replies the reading board member has not read yet (others' replies since
    /// their last `thread.read` of it). Reading the channel does not change it.
    /// Left out when none are unread, for an app, and by older servers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_unread: Option<u32>,
    /// A message that starts a thread (feature `chat_threads`): who replied, so
    /// a client can show them under the root. The names (board members'
    /// usernames, employees' names, as in `ReactionView::by`) of at most
    /// `THREAD_REPLIERS_MAX` (3) distinct repliers, most recent first. Left out when
    /// it has no replies and by older servers.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub thread_repliers: Vec<String>,
    /// The reactions on the message (feature `chat_reactions`), one entry per
    /// emoji, in the order each emoji was first used. Left out when there are
    /// none and by older servers.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reactions: Vec<ReactionView>,
    /// When the message was last edited (feature `message_edits`). Left out when
    /// it never was and by older servers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edited_at_ms: Option<i64>,
    /// When the message was deleted (feature `message_edits`): `text` is the
    /// line that says so. Left out when it was not and by older servers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deleted_at_ms: Option<i64>,
    /// The reading board member may edit and delete this message (feature
    /// `message_edits`), per reader like `ReactionView::mine`. Left out when false.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub editable: bool,
    /// The run behind this message waits for a board member (feature `run_cards`):
    /// a question or an approval is open, or the post ended a ticket's main task on
    /// a question (the rule of `group_chat::asks_question`). Only on the run's last
    /// message in the list. Left out when false and by older servers.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub awaiting_user: bool,
    /// What a run card under this message shows (feature `run_cards`): on a post in a
    /// discussion and on a `Reported` line, the last one of its run in the list. With
    /// `usage` for the tokens. Left out for other messages and by older servers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<RunBrief>,
}

/// What the run card of a post shows while folded (`MessageView::run`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RunBrief {
    /// `runs.status`: created | starting | running | waiting | cancelling | succeeded |
    /// failed | cancelled | unknown. A string, so a new state never breaks a client.
    pub status: String,
    pub tool_calls: u32,
    pub failed_calls: u32,
    /// The employee whose run it was, for its scratchpad (feature `employee_scratchpad`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub employee_id: Option<EmployeeId>,
}

/// `run.get_activity`: every tool call of a run, for an opened run card (feature `run_cards`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RunActivity {
    pub run_id: RunId,
    /// The answer the calls are kept with (`activity.get` reads one whole); none without calls.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<MessageId>,
    pub calls: Vec<ActivityView>,
}

/// `message.locate` (feature `message_locate`): where a message of a channel or
/// a ticket's discussion is, so a link to it can open the right place.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MessageLocation {
    pub message_id: MessageId,
    pub conversation_id: ConversationId,
    /// The ticket whose discussion it is in; none for a channel.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticket_id: Option<crate::TicketId>,
    /// The number of `ticket_id` (`#12`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticket_number: Option<u32>,
    /// The message that starts the thread it replies in; none for a message of
    /// the channel itself (also for a thread's root).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_root: Option<MessageId>,
}

/// One earlier text of an edited message (`MessageHistoryView::versions`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MessageVersionView {
    pub text: String,
    /// When an edit replaced this text.
    pub replaced_at_ms: i64,
}

/// `message.history` (feature `message_edits`): the earlier texts of a message,
/// newest first, each through the vault redactor when it goes out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MessageHistoryView {
    /// Always empty when `deleted`: a deleted text is gone in every client, even
    /// though the audit log keeps it.
    pub versions: Vec<MessageVersionView>,
    pub deleted: bool,
}

/// `thread.read` (feature `chat_threads`): one thread of a channel, its root
/// message and its replies oldest first, as the reading board member sees them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ThreadSnapshot {
    pub as_of_seq: u64,
    /// The channel (or ticket's discussion) the thread is in.
    pub conversation_id: ConversationId,
    /// The message that starts the thread, with its counters (`thread_unread`
    /// as it was before this read marked the thread read).
    pub root: MessageView,
    /// Its replies, oldest first (each with `thread_parent` set to the root).
    #[serde(default)]
    pub replies: Vec<MessageView>,
    /// The channel is closed (an archived project's, a ticket's on Done or
    /// archived): the thread can be read, not answered.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub read_only: bool,
}

/// One emoji on a message (`MessageView::reactions`, feature `chat_reactions`):
/// how many reacted with it, whether the reader did, and who.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ReactionView {
    pub emoji: String,
    pub count: u32,
    /// The board member who reads the message reacted with it.
    pub mine: bool,
    /// The names of who reacted (board members' usernames, employees' names), oldest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub by: Vec<String>,
}

/// What a system line in a discussion says happened (`MessageView::system`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SystemLineKind {
    /// `from` gave the task `task_id` to `to`.
    Delegated,
    /// The task `task_id` of `from` reported (or failed: the text says so).
    Reported,
    /// `by` moved the ticket from the column `from` to `to`.
    Moved,
    /// The owner's plan for `task_id` waits for a board member (approve or send back).
    Plan,
    /// The ticket is in Review: the report of `task_id` waits for a board member.
    Review,
    /// `by` put the ticket on Done.
    Done,
    /// The ticket was made from the message `message_id` in `conversation_id`.
    CreatedFrom,
    /// In a chat: the ticket `ticket_id` was made from this conversation's message `message_id`.
    TicketCreated,
    /// The summary of the ticket went to the project's memory.
    Summary,
    /// The summary failed (the text says why); a board member can try again (`ticket.retry_summary`).
    SummaryFailed,
    /// A mention woke nobody: a loop limit was reached (`to`: who was not woken).
    Limit,
}

/// A system line (`MessageView::system`). Every field but `kind` is optional and
/// only set where `SystemLineKind` names it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SystemLine {
    pub kind: SystemLineKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<TaskId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    /// Who did it: a board member's name, an employee's, or "Aurelius".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticket_id: Option<crate::TicketId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<ConversationId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<MessageId>,
    /// What exactly happened within `kind` (feature `ticket_channels`), one of
    /// `system_action`: e.g. `opened` or `closed` on a ticket's line in General,
    /// `approved` on a `plan` line. A client that knows none of them shows `text`,
    /// which stays the readable sentence. A string, not an enum, so a new action
    /// never breaks an older client. Left out by older servers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    /// The ticket's channel (its discussion) the line links to (feature
    /// `ticket_channels`), e.g. on the line in General when a ticket opens or closes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<ConversationId>,
}

/// The values of `SystemLine::action` (feature `ticket_channels`).
pub mod system_action {
    /// A ticket was made (its channel opened); in General, with the channel's link.
    pub const OPENED: &str = "opened";
    /// The ticket went to Done or was archived: its channel is read-only now.
    pub const CLOSED: &str = "closed";
    /// The ticket left Done or was restored: its channel can be written in again.
    pub const REOPENED: &str = "reopened";
    /// A board member approved the owner's plan or accepted the report.
    pub const APPROVED: &str = "approved";
    /// A board member sent the ticket back (a plan or a report).
    pub const SENT_BACK: &str = "sent_back";
    /// A board member asked the owner to adjust the plan.
    pub const ADJUSTED: &str = "adjusted";
    /// The test run of the Testing column was green.
    pub const TESTED_PASSED: &str = "tested_passed";
    /// The test run of the Testing column was red.
    pub const TESTED_FAILED: &str = "tested_failed";
    /// Someone joined the channel (`by`: who).
    pub const JOINED: &str = "joined";
    /// `by` chose who is responsible for the ticket (`to`: who; on a `moved` line,
    /// so an older client shows `text` and never reads a new `SystemLineKind`).
    pub const RESPONSIBLE: &str = "responsible";
    /// A handoff waits until its sender has caught up with the thread (R5; `from`
    /// who holds it, `to` who gets it; on a `limit` line, so `text` says it all).
    pub const HANDOFF_HELD: &str = "handoff_held";
}

impl SystemLine {
    /// A line of `kind` with every other field empty.
    pub fn new(kind: SystemLineKind) -> Self {
        SystemLine {
            kind,
            task_id: None,
            from: None,
            to: None,
            by: None,
            ticket_id: None,
            conversation_id: None,
            message_id: None,
            action: None,
            channel_id: None,
        }
    }

    /// The same line with this action (one of `system_action`).
    pub fn with_action(mut self, action: &str) -> Self {
        self.action = Some(action.to_owned());
        self
    }

    /// The line in words, as the server stores it as the message's `text` and the
    /// clients show it. `ticket`: the number of `ticket_id` (`#N`, MU2-10).
    pub fn sentence(&self, ticket: Option<u32>) -> String {
        let or = |v: &Option<String>, d: &str| {
            v.as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or(d)
                .to_string()
        };
        let by = or(&self.by, "Aurelius");
        let number = ticket.map(|n| format!("#{n}"));
        match self.kind {
            SystemLineKind::Delegated => format!(
                "{} gave a task to {}",
                or(&self.from, "An employee"),
                or(&self.to, "a colleague")
            ),
            SystemLineKind::Reported => format!("{} reported back on a task", or(&self.from, "A colleague")),
            SystemLineKind::Moved => match (&self.from, &self.to) {
                (Some(from), Some(to)) => format!("{by} moved the ticket from {from} to {to}"),
                (_, Some(to)) => format!("{by} moved the ticket to {to}"),
                _ => format!("{by} moved the ticket"),
            },
            SystemLineKind::Plan => format!(
                "{} handed in a plan. It waits for a board member to approve it",
                or(&self.from, "The owner")
            ),
            SystemLineKind::Review => format!(
                "{} finished the work and handed in the report. It is ready for your review",
                or(&self.from, "The owner")
            ),
            SystemLineKind::Done => format!("{by} accepted the report and moved the ticket to Done"),
            SystemLineKind::CreatedFrom => match &self.by {
                Some(b) => format!(
                    "{} created from a message · {b}",
                    number.as_deref().unwrap_or("Ticket")
                ),
                None => format!(
                    "{} created from a message",
                    number.as_deref().unwrap_or("Ticket")
                ),
            },
            SystemLineKind::TicketCreated => format!(
                "{by} created {} from a message",
                number.as_deref().unwrap_or("a ticket")
            ),
            SystemLineKind::Summary => "A summary of this ticket was saved to project memory".to_string(),
            SystemLineKind::SummaryFailed => "The summary of this ticket could not be saved to project memory".to_string(),
            SystemLineKind::Limit => {
                format!("Limit reached: {} was not woken.", or(&self.to, "nobody"))
            }
        }
    }
}

/// What a channel is (`ChannelView::kind`, feature `ticket_channels`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChannelKind {
    /// The general channel (no project).
    #[default]
    General,
    /// A project's channel.
    Project,
    /// A ticket's channel: its discussion.
    Ticket,
    /// A kind of a newer server this client does not know.
    #[serde(other)]
    Unknown,
}

impl ChannelKind {
    /// Whether this is the default (`General`), which the JSON leaves out.
    pub fn is_general(&self) -> bool {
        *self == ChannelKind::General
    }
}

/// The longest slug of a ticket's title in its channel's name (`ticket_channel_name`).
pub const TICKET_CHANNEL_SLUG_MAX: usize = 32;

/// The name of a ticket's channel: `#<number>-<slug>`, the slug being the title
/// in lowercase ASCII words joined by `-` (any other character separates words),
/// at most `TICKET_CHANNEL_SLUG_MAX` characters, cut after a whole word (a first
/// word that alone is too long is cut). Computed when read, never stored, so a
/// renamed ticket's channel follows. Only `#<number>` without an ASCII letter or digit.
pub fn ticket_channel_name(number: u32, title: &str) -> String {
    let mut slug = String::new();
    for word in title.split(|c: char| !c.is_ascii_alphanumeric()).filter(|w| !w.is_empty()) {
        let word = word.to_ascii_lowercase();
        let extra = if slug.is_empty() { word.len() } else { word.len() + 1 };
        if slug.len() + extra <= TICKET_CHANNEL_SLUG_MAX {
            if !slug.is_empty() {
                slug.push('-');
            }
            slug.push_str(&word);
            continue;
        }
        if slug.is_empty() {
            // ASCII only, so every index is a character boundary.
            slug.push_str(&word[..TICKET_CHANNEL_SLUG_MAX]);
        }
        break;
    }
    if slug.is_empty() { format!("#{number}") } else { format!("#{number}-{slug}") }
}

/// A channel of the Team chat (`channel.list`, feature `team_channels`): the
/// general one (no project), one per project, and (feature `ticket_channels`,
/// only for a connection that accepts it) one per ticket. A closed ticket's
/// channel (ticket on Done or archived) reports `unread` and `mentions` 0, so old
/// closed tickets never light up the Team badge; an archived project's channel
/// still counts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ChannelView {
    pub conversation_id: ConversationId,
    /// None: the general channel.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<ProjectId>,
    /// "General" or the project's name.
    pub name: String,
    /// Messages since the last one read; 0 for a closed ticket's channel.
    pub unread: u32,
    /// Of those, the ones that name a board member with `@Name` (B8: the badge counts these).
    pub mentions: u32,
    /// The channel can only be read: its project is archived, or (a ticket's
    /// channel) the ticket is on Done or archived.
    pub archived: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_activity_ms: Option<i64>,
    /// What the channel is (feature `ticket_channels`); left out for General and by older servers.
    #[serde(default, skip_serializing_if = "ChannelKind::is_general")]
    pub kind: ChannelKind,
    /// A ticket's channel: the ticket.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticket_id: Option<crate::TicketId>,
    /// A ticket's channel: the ticket's number (`#N`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticket_number: Option<u32>,
    /// A ticket's channel: the kind of the column the ticket is in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticket_column: Option<crate::ticket::ColumnKind>,
    /// The board member who reads the list is a member of the channel (a
    /// ticket's channel: its creator, owner, the Owner, or who joined later).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub member: bool,
    /// How many board members and employees are members (a ticket's channel).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub member_count: Option<u32>,
    /// The highest message ordinal the reader had read before this read (the
    /// line "New messages" goes after it); none: never read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_through: Option<u32>,
}

/// `channel.list`: the general channel first, then one per project by name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ChannelsSnapshot {
    pub as_of_seq: u64,
    pub channels: Vec<ChannelView>,
}

/// `discussion.audience` (feature `everyone_mention`): whom a board member's
/// `@Everyone` in one discussion would reach, so a client can say so before it
/// sends: it wakes the `coordinator`, the others only know of it. Never the asking
/// board member: they are not told of their own message.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DiscussionAudience {
    /// Active employees in the discussion, who know of an `@Everyone`: it is in what
    /// they read when next woken there. It does not wake them (see `coordinator`).
    pub employees: u32,
    /// Their names, in the order of the company.
    pub employee_names: Vec<String>,
    /// Employees in the discussion who are paused.
    pub paused: u32,
    /// Their names.
    pub paused_names: Vec<String>,
    /// Board members `@Everyone` tells in the Inbox (and makes follow a ticket).
    pub board_members: u32,
    /// Their usernames.
    pub board_usernames: Vec<String>,
    /// The one employee `@Everyone` wakes, who answers or hands over: the ticket's
    /// owner, else the dispatcher, else the CEO, the first that is active. None
    /// when there is nobody, or from a server that still wakes all `employees`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coordinator: Option<String>,
}

/// A message an employee scheduled (`schedule_message`, R7; feature `scheduled_messages`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ScheduledMessageView {
    pub id: crate::ScheduledMessageId,
    pub from_employee: EmployeeId,
    pub from_name: String,
    pub to_employee: EmployeeId,
    pub to_name: String,
    pub text: String,
    pub created_at_ms: i64,
    pub due_at_ms: i64,
    /// scheduled | delivered | cancelled | not_delivered. A string, so a new state never breaks a client.
    pub state: String,
    /// Where it goes, in words ("the discussion of ticket #21").
    pub place: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticket_id: Option<TicketId>,
    /// The channel of the Team chat it goes to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<ConversationId>,
    /// Whose Claude login its run uses: the one of the run that planned it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub login_name: Option<String>,
    /// Why it did not go out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
    /// The run it started.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<RunId>,
}

/// `scheduled_message.list`: what waits, soonest first, then the latest that went out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ScheduledMessagesSnapshot {
    pub as_of_seq: u64,
    pub messages: Vec<ScheduledMessageView>,
}

/// An employee that waits to pick up a thread (U3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ThreadWaiter {
    pub employee_id: EmployeeId,
    pub employee_name: String,
    /// How many threads of this employee go first, counting what it is doing now.
    pub behind: u32,
    /// What holds it back besides its turn: `limit` (a usage limit), `question`
    /// (the thread waits for a board member's answer). A string, so a new one never breaks a client.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// `thread.waiting`: who waits to pick up a thread and what is scheduled in it (U3, R7).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ThreadWaitingSnapshot {
    pub as_of_seq: u64,
    pub waiters: Vec<ThreadWaiter>,
    pub scheduled: Vec<ScheduledMessageView>,
}

/// Where a thread is, as `activity_log.list` and `employee.queue` name it (R8, U6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ThreadPlace {
    /// "#21 Fix login", a channel's name, "Chat with sam", or what is left of it.
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticket_id: Option<TicketId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<ConversationId>,
    /// A board member's chat with an employee, when it is the reader's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat_id: Option<ConversationId>,
}

/// One delivery, queue step or run step (R8; feature `activity_log`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActivityLogEntry {
    pub seq: u64,
    pub at_ms: i64,
    /// delivery, queue or run. Text, not an enum: a later value only shows as it is.
    pub category: String,
    /// What happened (delivered, claimed, started, tool, ...). Text for the same reason.
    pub step: String,
    /// A short code or a tool's name; never the text of a message or a tool's input.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub employee_id: Option<EmployeeId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub employee_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub place: Option<ThreadPlace>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<RunId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hop: Option<u32>,
}

/// `activity_log.list`: the newest rows that match, newest first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActivityLogPage {
    pub as_of_seq: u64,
    pub entries: Vec<ActivityLogEntry>,
    /// There are older rows: ask again with `before_seq` as the last row's `seq`.
    #[serde(default)]
    pub has_more: bool,
}

/// One item of an employee's mailbox (U6; feature `employee_queue`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct EmployeeQueueItem {
    pub id: String,
    /// message, task, resume, scheduled. Text, not an enum.
    pub kind: String,
    /// board, employee or system.
    pub source: String,
    /// claimed (a run works on it), queued or held.
    pub state: String,
    pub thread_key: String,
    pub place: ThreadPlace,
    /// The run that works on it; none while it waits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<RunId>,
    pub hop: u32,
    pub created_at_ms: i64,
    pub not_before_ms: i64,
    /// What holds it back besides its turn: paused, question, limit, later, machine or handoff.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waits_for: Option<String>,
}

/// `employee.queue`: what is in an employee's mailbox, what runs first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct EmployeeQueue {
    pub as_of_seq: u64,
    pub employee_id: EmployeeId,
    pub items: Vec<EmployeeQueueItem>,
}

/// `employee.scratchpad`: an employee's own note (C7; feature `employee_scratchpad`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ScratchpadView {
    pub employee_id: EmployeeId,
    pub text: String,
    /// When it last changed; none when it was never written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at_ms: Option<i64>,
}

/// One of a board member's lasting chats with an employee (feature `employee_chats`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct EmployeeChatView {
    pub conversation_id: ConversationId,
    pub employee_id: EmployeeId,
    pub employee_name: String,
    /// What the employee wrote since the board member last read it.
    pub unread: u32,
    pub last_activity_ms: i64,
    /// The start of the latest message, for a list of chats.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<String>,
}

/// `employee.chats`: the reader's chats that have something in them, the latest first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct EmployeeChatsSnapshot {
    pub as_of_seq: u64,
    pub chats: Vec<EmployeeChatView>,
}

/// A file the user sent to an employee, copied into its work folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MessageFileView {
    /// Fetch it with `upload.get`. Given by the server when it reads the file's
    /// message, task or recurring task (never stored); older servers leave it out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub name: String,
    /// Relative to the work folder, with `/`, e.g. `.aurelius/uploads/report.pdf`.
    pub path: String,
    pub byte_length: u64,
}

/// A file kept with a message: what an agent made or mentioned (a screenshot,
/// a report, a log). Images show in the app; other files open on the device.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FileRef {
    pub id: String,
    pub name: String,
    pub media_type: String,
    pub size: u64,
}

/// One tool use during an answer, shown at its place in the text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActivityView {
    pub call_id: String,
    /// The runtime's tool name, e.g. "Bash", "Edit", "WebSearch".
    pub tool: String,
    pub input: serde_json::Value,
    /// Result text (capped); `None` while running.
    pub output: Option<String>,
    /// `None` while running.
    pub ok: Option<bool>,
    /// Byte position in the answer text where it happened.
    pub offset: u32,
    /// `conversation.get` and live updates shorten the output and long strings
    /// in the input (`crate::activity`); `activity.get` has all of it
    /// (feature `activity_preview`). Older servers leave it out.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub truncated: bool,
    /// The whole output's length in bytes, when `output` was shortened.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_bytes: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SourceView {
    pub n: u32,
    pub url: String,
    pub title: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AttachmentView {
    pub id: AttachmentId,
    pub name: String,
    pub kind: String,
    pub byte_length: u64,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MessagesSnapshot {
    pub as_of_seq: u64,
    pub conversation_id: ConversationId,
    pub messages: Vec<MessageView>,
    pub attachments: Vec<AttachmentView>,
    pub agent_mode: crate::autonomy::AgentMode,
    pub browser_allowed: bool,
    /// The agent may take screenshots without asking each time; off by default (it then asks).
    pub screen_allowed: bool,
    /// Model and effort chosen for this conversation (unset: the employee's).
    pub run: crate::task::RunSettings,
    /// The employee of a task conversation or an employee chat.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub employee_id: Option<EmployeeId>,
    /// The project of a task or project conversation; none for an employee chat.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<ProjectId>,
    /// An older employee chat (converted from protocol 2): history, it takes no messages.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub read_only: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MemorySourceView {
    pub event_id: String,
    pub label: String,
    pub at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MemoryView {
    pub id: MemoryId,
    /// global | project | employee
    pub scope: String,
    pub employee_id: Option<EmployeeId>,
    pub employee_name: Option<String>,
    pub kind: crate::memory::MemoryKind,
    pub status: crate::memory::MemoryStatus,
    /// Empty when forgotten (content erased).
    pub content: String,
    pub pinned: bool,
    pub importance: f64,
    pub confidence: f64,
    pub created_at_ms: i64,
    pub supersedes_id: Option<MemoryId>,
    pub conflicts_with_id: Option<MemoryId>,
    /// user | policy | None (still proposed)
    pub decided_by: Option<String>,
    pub sources: Vec<MemorySourceView>,
    /// Set when this memory is a lesson learned from a task (feature
    /// `task_lessons`): when it applies, its evidence and how often it was
    /// confirmed. The kind is then `episodic`; show it as "Lesson".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lesson: Option<crate::memory::LessonLocator>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MemoryContextItem {
    pub memory_id: MemoryId,
    pub kind: crate::memory::MemoryKind,
    /// global | project | employee
    pub scope: String,
    pub employee_name: Option<String>,
    pub pinned: bool,
    /// The label the context builder writes, e.g. "Decision · project".
    pub label: String,
    pub content: String,
    /// Tokens of the line as it goes into the system text.
    pub tokens: u32,
    /// always | when_relevant | over_budget | duplicate
    pub placement: String,
}

/// What goes into an employee's context without a specific question.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MemoryContextView {
    /// None: an employee's chat outside projects.
    pub project_id: Option<ProjectId>,
    pub employee_id: Option<EmployeeId>,
    /// Estimate of the fixed system text without memory; 0 when not known.
    pub system_tokens: u32,
    pub memory_budget_tokens: u32,
    pub always_tokens: u32,
    pub items: Vec<MemoryContextItem>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MemorySnapshot {
    pub items: Vec<MemoryView>,
}

/// One line of a context manifest: what was considered and why it was (not) used.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ContextItemView {
    /// rules | identity | request | turn | summary | memory | source | guideline
    /// (one item per guideline in the run's block, feature `guidelines`; `id` is
    /// `g:<id>@<version>`, never left out by the budget: `included` false only
    /// when B4's safety net dropped it).
    pub section: String,
    pub id: String,
    pub label: String,
    pub hash: String,
    pub tokens: u32,
    pub score: Option<f64>,
    pub included: bool,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ContextView {
    pub run_id: RunId,
    pub budget_tokens: u32,
    pub used_tokens: u32,
    pub scopes: Vec<String>,
    /// The runtime keeps its own history (resume); Aurelius does not see every token.
    pub runtime_managed: bool,
    pub items: Vec<ContextItemView>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UsageWindow {
    pub label: String,
    pub runs: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_tokens: u64,
    /// Part of `input_tokens` written to the prompt cache (feature `run_usage`).
    #[serde(default)]
    pub cache_creation_tokens: u64,
    /// Only when the provider reported a cost (micro-USD).
    pub cost_micro_usd: Option<i64>,
    /// Some values are not a bill (e.g. subscription: API-equivalent value).
    pub estimate: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UsageRow {
    pub connection: String,
    pub provider: String,
    pub model: String,
    pub runs: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_tokens: u64,
    /// Part of `input_tokens` written to the prompt cache (feature `run_usage`).
    #[serde(default)]
    pub cache_creation_tokens: u64,
    pub cost_micro_usd: Option<i64>,
    pub estimate: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UsageDay {
    pub day_start_ms: i64,
    pub tokens: u64,
}

/// One server-local day in hours: always 24 entries, oldest first.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UsageHours {
    pub day_start_ms: i64,
    pub hours: Vec<UsageHour>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UsageHour {
    pub hour_start_ms: i64,
    pub runs: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_tokens: u64,
    /// Part of `input_tokens` written to the prompt cache (feature `run_usage`).
    #[serde(default)]
    pub cache_creation_tokens: u64,
    pub cost_micro_usd: Option<i64>,
    pub estimate: bool,
}

/// Tokens of one run or a sum of runs (feature `run_usage`). Every model call
/// sends the whole context again: `cached_tokens` is the part read from the prompt
/// cache (cheap), `input_tokens` the rest, of which `cache_creation_tokens` was
/// written to the cache (dearer than plain input).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TokenUsage {
    pub runs: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_tokens: u64,
    pub cache_creation_tokens: u64,
    /// Only when the provider reported a cost (micro-USD).
    pub cost_micro_usd: Option<i64>,
    /// Some values are not a bill (e.g. subscription: API-equivalent value).
    pub estimate: bool,
    /// How many model calls those runs made (feature `run_cards`). 0: not known,
    /// for usage from before the calls were counted and by older servers.
    #[serde(default)]
    pub model_calls: u64,
}

/// `usage.get_totals`: what a conversation or a task used (feature `run_usage`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UsageTotals {
    /// The runs of the conversation itself (a task: of its conversation).
    pub own: TokenUsage,
    /// Tasks given away from it (delegated, handed over, started from it), each
    /// with its own sub-tasks included.
    pub subtasks: Vec<SubtaskUsage>,
    /// `own` plus every subtask.
    pub total: TokenUsage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SubtaskUsage {
    pub task_id: TaskId,
    pub title: String,
    /// Who works on it.
    pub employee_name: Option<String>,
    pub usage: TokenUsage,
}

/// `trace.get_setting` / `trace.set_enabled`: whether runs keep a trace (feature `run_trace`).
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
pub struct TraceSetting {
    pub enabled: bool,
    /// Traces kept at most (the newest).
    pub keep_runs: u32,
    /// Traces older than this go.
    pub keep_days: u32,
}

/// Where an employee's effective model or effort comes from (`employee.run_overview`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunValueSource {
    /// Chosen for this employee.
    Employee,
    /// The company-wide default.
    Company,
    /// The default model of the connection.
    Connection,
    /// Nothing is chosen: the runtime decides (the model's own effort).
    Runtime,
}

/// What a new run of one employee uses, as the run itself would resolve it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct EmployeeRunLine {
    pub employee_id: EmployeeId,
    pub name: String,
    pub status: LifecycleStatus,
    /// The display name of the connection, if there is one.
    pub connection: Option<String>,
    pub model: Option<String>,
    pub model_source: RunValueSource,
    /// The effort passed to the run; none: the run gets no effort and the model's own applies.
    pub effort: Option<String>,
    pub effort_source: RunValueSource,
    /// The model's own effort when none is passed, where that is known.
    pub model_default_effort: Option<String>,
    /// The levels this employee's model can use; empty: it has no effort setting.
    pub supported_efforts: Vec<String>,
    /// A chosen effort the model cannot use, so runs leave it out.
    pub unsupported_effort: Option<String>,
    /// What is stored for the employee itself: a change sends it back with only the effort changed.
    pub own: crate::connection::RunDefaults,
    /// The model chosen in the employee's chat; it goes before the rest in its chats.
    pub chat_model: Option<String>,
    /// The effort chosen in the employee's chat; it goes before the rest in its chats.
    pub chat_effort: Option<String>,
}

/// `employee.run_overview` (feature `employee_run_overview`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct EmployeeRunsView {
    pub employees: Vec<EmployeeRunLine>,
    /// The stored company-wide default for employee runs.
    pub company: crate::connection::RunDefaults,
    pub company_model: Option<String>,
    /// The levels the company's default model can use.
    pub company_supported_efforts: Vec<String>,
    pub company_model_default_effort: Option<String>,
}

/// `run.get_trace`: what one run sent to the runtime and what each of its model calls cost
/// (feature `run_trace`). Kept only while trace mode is on, for the latest runs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RunTrace {
    pub run_id: RunId,
    pub conversation_id: Option<ConversationId>,
    pub created_at_ms: i64,
    pub model: String,
    pub effort: Option<String>,
    /// text | web | work: what the runtime's own tools may do.
    pub profile: String,
    /// The employee's mode at work (manual | acceptEdits | auto | plan).
    pub mode: Option<String>,
    /// none | new | continue | resume: `continue` goes on in the conversation's session
    /// with only the new messages; `new` sends the whole conversation (a rebuild).
    pub session: String,
    pub mcp_servers: Vec<String>,
    /// The command line the runtime was started with (secrets left out); empty when it did not say.
    pub args: Vec<String>,
    /// The full system text (appended to the runtime's own on every run, also a continued one).
    pub system_text: String,
    /// The prompt as it went in.
    pub user_turn: String,
    pub images: Vec<TraceImage>,
    /// Sizes of the parts of the system text and the prompt.
    pub parts: Vec<TracePart>,
    /// The tools the runtime loaded (their definitions are input of every model call).
    pub tools: Vec<String>,
    /// Every model call of the run, in order.
    pub steps: Vec<TraceStep>,
    /// The run's own report (none until it ended).
    pub usage: Option<TokenUsage>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TraceImage {
    pub name: String,
    pub media_type: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TracePart {
    /// system | prompt
    pub area: String,
    /// E.g. "Instructions", "Memory", "Earlier conversation", "New message".
    pub label: String,
    pub chars: u64,
    /// Estimated (characters / 4).
    pub tokens: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TraceStep {
    /// Fresh input: neither read from nor written to the prompt cache.
    pub input_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cache_read_tokens: u64,
    pub output_tokens: u64,
    /// The tools this call asked for.
    pub tools: Vec<String>,
    pub subagent: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UsageSnapshot {
    /// Today, 7 days, 30 days.
    pub windows: Vec<UsageWindow>,
    /// Per connection and model, last 30 days.
    pub by_source: Vec<UsageRow>,
    /// Tokens per day, last 14 days (oldest first).
    pub by_day: Vec<UsageDay>,
    /// Per board member whose Claude login ran it, last 30 days (feature `run_cards`).
    /// Left out by older servers.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub by_user: Vec<UsageUserRow>,
}

/// One board member's share of the usage (`UsageSnapshot::by_user`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UsageUserRow {
    /// `runs.login_user`; none: runs on no one's login (an API key, a background run).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub login_user: Option<String>,
    /// That board member's name; none when there is none or they are gone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    pub runs: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cost_micro_usd: Option<i64>,
    pub estimate: bool,
    #[serde(default)]
    pub model_calls: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TimelineCard {
    pub seq: u64,
    pub at_ms: i64,
    /// start | minor | decision | outcome | activity
    pub kind: String,
    pub label: String,
    pub title: String,
    pub detail: String,
    pub actor: String,
    pub correlation_id: String,
    pub task_id: Option<TaskId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct NotificationView {
    pub id: String,
    /// action | warning | info
    pub severity: String,
    pub title: String,
    pub subject: String,
    pub created_at_ms: i64,
    pub read: bool,
    pub task_id: Option<TaskId>,
    pub target_type: String,
    /// The ticket the row is about (feature `personal_inbox`). Left out otherwise and by older servers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticket_id: Option<crate::TicketId>,
    /// The number of `ticket_id` (`#12`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticket_number: Option<u32>,
    /// The channel or discussion the row is about (feature `personal_inbox`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<ConversationId>,
    /// The message the row is about (feature `personal_inbox`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<MessageId>,
    /// Why the reader gets the row (feature `personal_inbox`): `responsible`,
    /// `mention`, `plan_review`, `review`, `sent_back`, `failed` or `login`. Text,
    /// not an enum: a client shows an unknown reason as a plain row, so a new one
    /// never breaks `notification.list` on an older client.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// How many later events this one row stands for (feature `personal_inbox`).
    /// Left out when none.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub merged_count: u32,
    /// The row waits for the reader, worked out when it is read so that it follows
    /// a handover (feature `personal_inbox`). Left out when false.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub needs_you: bool,
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

/// A colleague an employee proposed; the user decides.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ProposalView {
    pub id: ProposalId,
    pub proposed_by: EmployeeId,
    pub proposed_by_name: String,
    pub status: ProposalStatus,
    pub name: String,
    pub role: String,
    pub title: Option<String>,
    pub instructions: String,
    pub reports_to: Option<EmployeeId>,
    pub reports_to_name: Option<String>,
    /// Why the proposer wants this colleague.
    pub reason: String,
    /// The user's note on the decision (it goes back to the proposer).
    pub decision_note: Option<String>,
    pub created_at_ms: i64,
    pub decided_at_ms: Option<i64>,
    /// The employee created when the proposal was approved.
    pub created_employee_id: Option<EmployeeId>,
    /// What it proposes (feature `instruction_proposals`; older servers: a colleague).
    #[serde(default)]
    pub kind: crate::proposal::ProposalKind,
    /// Instruction proposals: the employee whose instructions would change (the
    /// proposer itself or a direct report). `name`/`role` are that employee's,
    /// `instructions` the proposed text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_employee_id: Option<EmployeeId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_employee_name: Option<String>,
    /// The version of the target's instructions the proposal was made on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_version: Option<u32>,
    /// The target's instructions now (show the diff against `instructions`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_instructions: Option<String>,
    /// The target's current version; when it differs from `base_version`, the
    /// instructions changed since and approving is refused (`version_conflict`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_version: Option<u32>,
    /// The tasks and memories (lessons) the proposal rests on.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<crate::proposal::ProposalEvidence>,
}

/// One version of an employee's instructions (feature `instruction_proposals`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct EmployeeInstructionVersionView {
    pub version: u32,
    /// Empty: the employee had no instructions.
    pub instructions: String,
    /// `initial` (as it was when versions began), `created`, `edited` (employee.update),
    /// `proposal` (an approved proposal), `restored`.
    pub source: String,
    /// The approved proposal this version came from.
    pub proposal_id: Option<ProposalId>,
    /// The version a restore put back.
    pub restored_from: Option<u32>,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct EmployeeInstructionVersionsSnapshot {
    pub employee_id: EmployeeId,
    /// The version in use now.
    pub current_version: u32,
    /// Newest first.
    pub versions: Vec<EmployeeInstructionVersionView>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ProposalsSnapshot {
    pub as_of_seq: u64,
    /// Pending first, then the latest decisions.
    pub proposals: Vec<ProposalView>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ticket_channel_is_named_by_number_and_a_lowercase_slug() {
        assert_eq!(ticket_channel_name(12, "Fix the Login page!"), "#12-fix-the-login-page");
        assert_eq!(ticket_channel_name(3, "  API: v2 / rate-limits  "), "#3-api-v2-rate-limits");
    }

    #[test]
    fn a_long_title_is_cut_after_the_last_whole_word_that_fits() {
        let name = ticket_channel_name(7, "Migrate the payments service to the new queue and retry");
        assert_eq!(name, "#7-migrate-the-payments-service-to");
        let slug = name.split_once('-').map(|(_, s)| s).unwrap_or_default();
        assert!(slug.len() <= TICKET_CHANNEL_SLUG_MAX);
        // Exactly the limit is kept whole.
        let exact = "a".repeat(TICKET_CHANNEL_SLUG_MAX);
        assert_eq!(ticket_channel_name(1, &exact), format!("#1-{exact}"));
    }

    #[test]
    fn a_first_word_longer_than_the_limit_is_cut_at_the_limit() {
        let word = "x".repeat(TICKET_CHANNEL_SLUG_MAX + 10);
        assert_eq!(ticket_channel_name(5, &word), format!("#5-{}", "x".repeat(TICKET_CHANNEL_SLUG_MAX)));
    }

    #[test]
    fn a_title_without_ascii_letters_or_digits_gives_only_the_number() {
        assert_eq!(ticket_channel_name(9, "Überprüfung → ✓"), "#9-berpr-fung");
        assert_eq!(ticket_channel_name(9, "→ ✓ …"), "#9");
    }

    #[test]
    fn a_renamed_ticket_gives_the_new_name() {
        assert_ne!(ticket_channel_name(4, "Old title"), ticket_channel_name(4, "New title"));
    }
}

/// `TicketActivity::state`: an employee works on the ticket now.
pub const ACTIVITY_RUNNING: &str = "running";
/// `TicketActivity::state`: work is queued and only waits for a free slot.
pub const ACTIVITY_WAITING_SLOT: &str = "waiting_slot";
/// `TicketActivity::state`: its employee is paused, so it does not start.
pub const ACTIVITY_PAUSED: &str = "paused";
/// `TicketActivity::state`: waits for something else (see `waits_for`).
pub const ACTIVITY_WAITING_OTHER: &str = "waiting_other";

/// What an employee works on or waits with (feature `activity_overview`). With
/// `private` set it is another board member's chat: every other field is empty,
/// so neither its title nor where it is leaves the server.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActivityWork {
    /// "#21 Fix login", a task's title, a channel's name; empty when `private`.
    #[serde(default)]
    pub label: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub private: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticket_id: Option<TicketId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<TaskId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<ConversationId>,
    /// A chat of the reader's own with an employee.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat_id: Option<ConversationId>,
}

/// One run that really runs now (feature `activity_overview`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActivityBusy {
    /// None: the project assistant, which is no employee.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub employee_id: Option<EmployeeId>,
    pub employee_name: String,
    /// It was paused during this run: the run finishes, nothing new starts.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub employee_paused: bool,
    pub work: ActivityWork,
    pub since_ms: i64,
}

/// One piece of work in line (feature `activity_overview`): one per employee and
/// thread, since what waits in one thread becomes one run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActivityWaiting {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub employee_id: Option<EmployeeId>,
    pub employee_name: String,
    pub work: ActivityWork,
    /// slot (its employee is busy), machine (a free slot on this machine), paused,
    /// question, approval, limit, login, later or handoff. Text, not an enum.
    pub waits_for: String,
    pub since_ms: i64,
}

/// Where one ticket stands right now (feature `activity_overview`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TicketActivity {
    pub ticket_id: TicketId,
    /// running, waiting_slot, paused or waiting_other. Text, not an enum.
    pub state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub employee_id: Option<EmployeeId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub employee_name: Option<String>,
    /// With waiting_other: board_member, colleague, approval, question, limit,
    /// login, later or handoff. Also slot or machine with waiting_slot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waits_for: Option<String>,
    pub since_ms: i64,
}

/// `activity.overview` (feature `activity_overview`): who is busy, what waits and
/// on what, and the state of each ticket with work going or held up. A ticket
/// that is not listed has nothing going on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActivityOverview {
    pub as_of_seq: u64,
    pub busy: Vec<ActivityBusy>,
    pub waiting: Vec<ActivityWaiting>,
    pub tickets: Vec<TicketActivity>,
}
