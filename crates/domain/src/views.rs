//! Read models: what the storage layer returns and the protocol sends. Plain data, no behavior.
//!
//! They live in `domain` so that `storage` and `application` never depend on the wire crate.

use crate::{
    ApprovalId, AttachmentId, ConversationId, EmployeeId, MemoryId, MessageId, NoteId, ProjectId,
    ProjectStatus, ProposalId, ProviderId, RunId, TaskId,
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
    pub depends_on: Vec<TaskId>,
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
    /// The agent function whose call made this task.
    pub function_id: Option<crate::AgentFunctionId>,
    /// The app (the name of its app key) that created this task through
    /// `aurelius-mcp` (feature `app_keys`); none for the user's own tasks.
    #[serde(default)]
    pub created_by_app: Option<String>,
    /// The app key behind `created_by_app`.
    #[serde(default)]
    pub created_by_app_key_id: Option<crate::AppKeyId>,
    /// Where the task's variants are, when it has some (feature `task_variants`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant_phase: Option<crate::variants::VariantPhase>,
    /// A finished task (succeeded, failed or cancelled) in whose conversation a
    /// run goes on or waits to start, after the user (or a manager) wrote to it.
    /// The status does not change: clients show such a task as working or
    /// queued, not as done. Derived, never stored; left out when there is none
    /// and for tasks that are not finished (their `status` says it). Older
    /// servers leave it out.
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

/// Agent functions. Times are epoch milliseconds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AgentFunctionsSnapshot {
    /// The kill switch: false means every function refuses calls.
    pub all_enabled: bool,
    pub functions: Vec<AgentFunctionView>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AgentFunctionView {
    pub id: crate::AgentFunctionId,
    pub project_id: ProjectId,
    pub employee_id: EmployeeId,
    pub employee_name: String,
    pub name: String,
    pub slug: String,
    pub description: String,
    pub instruction_template: String,
    pub params: Vec<crate::agent_function::FunctionParam>,
    /// None: the employee's own default mode.
    pub mode: Option<crate::autonomy::AgentMode>,
    pub limit_per_day: Option<u32>,
    pub limit_per_week: Option<u32>,
    pub enabled: bool,
    /// Accepted calls in the last 24 hours / 7 days (rolling).
    pub used_day: u32,
    pub used_week: u32,
    pub created_at_ms: i64,
}

/// An API key; the secret is never part of it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ApiKeyView {
    pub id: crate::ApiKeyId,
    pub name: String,
    pub function_ids: Vec<crate::AgentFunctionId>,
    pub created_at_ms: i64,
    pub last_used_ms: Option<i64>,
    pub revoked_at_ms: Option<i64>,
}

/// The answer to `api_keys/create`: the secret (`ak_...`) is shown once and not kept.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ApiKeyCreated {
    pub key: ApiKeyView,
    pub secret: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FunctionCallsSnapshot {
    /// Newest first.
    pub calls: Vec<FunctionCallView>,
    pub usage: Vec<FunctionUsageView>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FunctionCallView {
    pub id: crate::FunctionCallId,
    pub function_id: crate::AgentFunctionId,
    pub function_name: String,
    pub api_key_id: crate::ApiKeyId,
    pub api_key_name: String,
    /// The task the call made (open it like any task).
    pub task_id: TaskId,
    /// queued, running, succeeded, failed or cancelled.
    pub status: crate::agent_function::CallStatus,
    pub created_at_ms: i64,
    pub finished_at_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FunctionUsageView {
    pub function_id: crate::AgentFunctionId,
    /// Accepted calls in the last 24 hours / 7 days (rolling).
    pub used_day: u32,
    pub used_week: u32,
    pub limit_per_day: Option<u32>,
    pub limit_per_week: Option<u32>,
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
    /// Opt-in heartbeat; `None` = off.
    pub heartbeat: Option<HeartbeatView>,
    /// It works on one task at a time; the others wait their turn.
    pub sequential_tasks: bool,
    /// Its chats, most recent first: the persistent one (`conversation_id`, it
    /// carries mode, heartbeat and memory summaries) and older ones from before
    /// protocol 3 (history). Task conversations are not listed.
    pub conversations: Vec<ConversationSummary>,
    /// What runs for this employee right now: its own steps,
    /// subagents and sub-tasks delegated to colleagues (derived, never stored).
    pub agents: Vec<AgentView>,
}

/// A page on claude.ai that a Claude Code run published (Claude Code artifact).
/// The URL is its identity: republishing keeps it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ArtifactView {
    /// `https://claude.ai/artifact/<id>`; private to the user's claude.ai account until shared there.
    pub url: String,
    pub title: String,
    /// The conversation whose run published it first.
    pub conversation_id: ConversationId,
    pub task_id: Option<TaskId>,
    pub project_id: Option<ProjectId>,
    /// Claude Code's version number of the latest publish (1 on the first).
    pub version: u32,
    pub first_published_at_ms: i64,
    pub last_published_at_ms: i64,
    /// The server kept the published file of at least one version, so
    /// `artifact.get_content` can show it; false for pages published before
    /// servers kept them (and from older servers): show the link instead.
    #[serde(default)]
    pub has_content: bool,
}

/// What kind of file a run published; decides how a client renders it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    Html,
    Markdown,
    /// Neither: show as plain text.
    Text,
}

/// The file one publish sent to claude.ai, as the server kept it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ArtifactSnapshot {
    pub version: u32,
    pub kind: ArtifactKind,
    /// The published file's name (no folder).
    pub file_name: String,
    pub content: String,
    pub published_at_ms: i64,
}

/// `artifact.get_content`: an artifact and the kept file of one version.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ArtifactContent {
    pub artifact: ArtifactView,
    /// Versions with a kept file, newest first (the server keeps the last 20).
    pub versions: Vec<u32>,
    /// The asked version, else the newest kept one; none when the server kept
    /// no file of it (published before servers kept them, larger than 2 MB, or
    /// not text): show the link instead.
    pub snapshot: Option<ArtifactSnapshot>,
}

/// `artifact.list`: newest publish first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ArtifactsSnapshot {
    pub artifacts: Vec<ArtifactView>,
}

/// A note of a project as `note.list` shows it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct NoteSummary {
    pub id: NoteId,
    pub project_id: ProjectId,
    /// The first non-empty line without Markdown heading marks; empty for an empty note.
    pub title: String,
    /// The text after the title on one line, at most 120 characters.
    pub preview: String,
    pub pinned: bool,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    /// Goes up by one with every save; `note.update` names the one it edited.
    pub revision: u64,
}

/// `note.list`: pinned first, then the last edited first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct NotesSnapshot {
    pub notes: Vec<NoteSummary>,
}

/// One note with its Markdown body (`note.get` and every note write).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct NoteView {
    #[serde(flatten)]
    pub summary: NoteSummary,
    pub body: String,
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
pub struct HeartbeatView {
    pub every_minutes: u32,
    pub quiet_start_min: u32,
    pub quiet_end_min: u32,
    pub daily_token_budget: u64,
    pub tokens_today: u64,
    pub next_due_ms: Option<i64>,
    /// e.g. "budget" when today's budget is used up.
    pub waiting: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ShareView {
    pub id: crate::ShareId,
    pub title: String,
    /// Where it came from, e.g. "Chat · How do we …" or "Task · Onboarding".
    pub source: String,
    /// project · <name> | employee · <name> | conversation
    pub destination: String,
    pub mode: crate::share::ShareMode,
    pub excerpt: String,
    pub created_at_ms: i64,
    pub revoked: bool,
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
    /// The reactions on the message (feature `chat_reactions`), one entry per
    /// emoji, in the order each emoji was first used. Left out when there are
    /// none and by older servers.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reactions: Vec<ReactionView>,
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
                "{} delegated a task to {}",
                or(&self.from, "An employee"),
                or(&self.to, "a colleague")
            ),
            SystemLineKind::Reported => format!("{} reported", or(&self.from, "A colleague")),
            SystemLineKind::Moved => match (&self.from, &self.to) {
                (Some(from), Some(to)) => format!("{by} moved {from} → {to}"),
                (_, Some(to)) => format!("{by} moved to {to}"),
                _ => format!("{by} moved the ticket"),
            },
            SystemLineKind::Plan => format!(
                "{} handed in a plan · waiting for a board member",
                or(&self.from, "The owner")
            ),
            SystemLineKind::Review => format!(
                "{} handed in the report · Review",
                or(&self.from, "The owner")
            ),
            SystemLineKind::Done => format!("{by} put it on Done"),
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
            SystemLineKind::Summary => "Summary saved to project memory".to_string(),
            SystemLineKind::SummaryFailed => "The summary for project memory failed".to_string(),
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
    /// Snapshots shared into this conversation (e.g. a discussed task).
    pub shares: Vec<ShareView>,
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

/// `variants.get_setting` / `variants.set_enabled`: whether tasks may be tried in
/// several ways first (feature `task_variants`; off by default).
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
pub struct VariantsSetting {
    pub enabled: bool,
    /// At most this many variants per task.
    pub max_variants: u8,
}

/// A task's variants and the judge's comparison (`task.variants`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TaskVariantsView {
    pub task_id: TaskId,
    pub phase: crate::variants::VariantPhase,
    /// How many variants.
    pub count: u8,
    /// The commit the variants started from.
    pub base_commit: Option<String>,
    /// The judge's pick, once it compared them.
    pub judge_pick: Option<u8>,
    /// What the judge advises to combine, if anything.
    pub compose: Option<String>,
    /// The chosen variant and by whom (`user` | `judge`).
    pub chosen: Option<u8>,
    pub chosen_by: Option<String>,
    /// Why it failed (phase `failed`), e.g. the chosen diff did not apply.
    pub error: Option<String>,
    pub variants: Vec<TaskVariantView>,
}

/// One variant of a task.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TaskVariantView {
    /// 1 to 3.
    pub n: u8,
    /// The approach it was told to take.
    pub hint: String,
    pub status: crate::variants::VariantStatus,
    /// Its own conversation (read it like any other).
    pub conversation_id: Option<ConversationId>,
    pub run_id: Option<RunId>,
    /// Files changed, lines added and removed.
    pub files: u32,
    pub insertions: u32,
    pub deletions: u32,
    /// The diff against `base_commit` (cut at 200 kB; `diff_truncated`).
    pub diff: String,
    pub diff_truncated: bool,
    /// Its closing answer.
    pub report: String,
    /// Tool calls that failed or were refused.
    pub errors: u32,
    /// The last test or build output it saw (cut), if any.
    pub tests: Option<String>,
    pub usage: TokenUsage,
    /// Its place in the judge's ranking (1 = best) and why.
    pub rank: Option<u8>,
    pub judge_note: Option<String>,
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
