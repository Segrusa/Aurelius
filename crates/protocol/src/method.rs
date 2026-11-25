//! Methods that read or open a session. Every method is `<area>.<verb>`; reads
//! are `list` or `get`. Writes are in `write.rs`; they all go through the same
//! pipeline (idempotency key, receipt, audit event, required right).

use crate::views::{FileScope, TimelineQuery};
use aurelius_domain::{CommandId, ConversationId, EmployeeId, MessageId, ProjectId, RunId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "method", content = "params")]
pub enum Method {
    /// The first request of a session: who is calling. The protocol major in
    /// every request must equal the server's, else the answer is a hard error.
    #[serde(rename = "session.hello")]
    SessionHello(HelloParams),
    #[serde(rename = "server.get_health")]
    ServerGetHealth,
    /// The server's name, its source and the computer's name (feature `server_name`).
    #[serde(rename = "server.get_name")]
    ServerGetName,

    #[serde(rename = "project.list")]
    ProjectList,
    #[serde(rename = "job.list")]
    JobList,
    /// A conversation's messages and settings.
    #[serde(rename = "conversation.get")]
    ConversationGet { conversation_id: ConversationId },
    /// The company's employees (not archived), oldest first.
    #[serde(rename = "employee.list")]
    EmployeeList,
    /// An employee's persistent chat with the user (company scope, no project):
    /// its messages and settings, as `conversation.get` answers them.
    #[serde(rename = "employee.chat")]
    EmployeeChat { employee_id: EmployeeId },
    /// Messages employees scheduled: what waits, then what went out (feature `scheduled_messages`).
    /// `project_id` (feature `workspace_filters`): only those in a ticket or channel of that project.
    #[serde(rename = "scheduled_message.list")]
    ScheduledMessageList {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project_id: Option<ProjectId>,
    },
    /// Who waits to pick up a thread and what is scheduled in it (U3, feature
    /// `scheduled_messages`): a ticket (`ticket_id`) or a channel or chat (`conversation_id`).
    #[serde(rename = "thread.waiting")]
    ThreadWaiting {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ticket_id: Option<aurelius_domain::TicketId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        conversation_id: Option<ConversationId>,
    },
    /// The tools that can be switched per employee (feature `employee_tools`).
    #[serde(rename = "employee.tool_options")]
    EmployeeToolOptions,
    /// The catalogue of start profiles (feature `employee_profiles`).
    #[serde(rename = "employee.start_profiles")]
    EmployeeStartProfiles,
    /// The model and effort a new run of each employee uses (feature `employee_run_overview`).
    #[serde(rename = "employee.run_overview")]
    EmployeeRunOverview,
    /// An employee's scratchpad (feature `employee_scratchpad`).
    #[serde(rename = "employee.scratchpad")]
    EmployeeScratchpad { employee_id: EmployeeId },
    /// What is in an employee's mailbox now (feature `employee_queue`).
    #[serde(rename = "employee.queue")]
    EmployeeQueue { employee_id: EmployeeId },
    /// Who is busy and what waits, on what (feature `activity_overview`). Another
    /// board member's private chat is only counted, with its employee.
    #[serde(rename = "activity.overview")]
    ActivityOverview,
    /// The activity log, newest first (R8, feature `activity_log`): every delivery, queue
    /// step and run step, narrowed by what is given. `limit`: at most 200, default 100.
    /// `project_id` (feature `workspace_filters`): only threads of that project's tickets,
    /// channels and tasks; a board member's private chat has no project.
    #[serde(rename = "activity_log.list")]
    ActivityLogList {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project_id: Option<ProjectId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        employee_id: Option<EmployeeId>,
        /// `ticket:<id>`, `channel:<conversation>` or `chat:<conversation>`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        thread_key: Option<String>,
        /// A ticket's thread, by its number.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ticket_number: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run_id: Option<aurelius_domain::RunId>,
        /// delivery, queue or run.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        category: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        before_seq: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        limit: Option<u32>,
    },
    /// The reader's lasting chats with employees, the latest first (feature `employee_chats`).
    #[serde(rename = "employee.chats")]
    EmployeeChats,
    /// The reader's work sessions, the latest activity first (feature `work_sessions`);
    /// `project_id`: only those of that project.
    #[serde(rename = "work_session.list")]
    WorkSessionList {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project_id: Option<aurelius_domain::ProjectId>,
    },
    /// The designs of a project, or of all without one: the kept HTML pages of each
    /// conversation, the latest first (feature `designs`).
    #[serde(rename = "design.list")]
    DesignList {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project_id: Option<aurelius_domain::ProjectId>,
    },
    /// The company's group chat (Team chat): the user and every employee. Its
    /// messages and settings, as `conversation.get` answers them; an employee's
    /// messages carry its name as `author`. Write with `group_chat.send`.
    /// `channel` (feature `team_channels`): a project's channel from
    /// `channel.list`; none: the general channel, the one older servers have.
    #[serde(rename = "group_chat.get")]
    GroupChatGet {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        channel: Option<ConversationId>,
    },
    /// The channels of the Team chat (feature `team_channels`): the general
    /// one first, then one per project (archived projects too: read-only).
    #[serde(rename = "channel.list")]
    ChannelList,
    /// The channels whose name or ticket number matches `query` (feature
    /// `ticket_channels`), in the order of `channel.list`, archived ones too: a
    /// closed ticket's channel stays findable. `#12` and `12` match ticket 12;
    /// other text matches the name, case not counting. A connection that does not
    /// accept `ticket_channels` gets no ticket channels here either.
    #[serde(rename = "channel.search")]
    ChannelSearch { query: String },
    /// Whether the server committed the write sent with `idempotency_key` (feature
    /// `command_applied`), answered with `CommandApplied`. A client asks this after
    /// the connection dropped or timed out under a write it will not resend, so it
    /// can tell "it arrived, the answer was lost" from "it never arrived". Only yes
    /// or no: the receipt's result stays with the connection that wrote it.
    #[serde(rename = "command.applied")]
    CommandApplied { idempotency_key: CommandId },
    /// Whom `@Everyone` would reach in a discussion (feature `everyone_mention`),
    /// answered with `DiscussionAudience`, for the reader's own message (the
    /// reader is never counted). `channel`: a channel or a ticket's discussion
    /// (its conversation id, as in `channel.list`); none: the general channel.
    /// Read-only; `@Everyone` itself is plain text in `group_chat.send` / `ticket.post`.
    #[serde(rename = "discussion.audience")]
    DiscussionAudience {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        channel: Option<ConversationId>,
    },
    /// One thread of a channel (feature `chat_threads`): `message_id` is the
    /// message that starts it (a message of a channel or a ticket's discussion
    /// that is not itself a reply), answered with the message and its replies,
    /// oldest first (`ThreadSnapshot`). The board member who reads it has read
    /// the thread (its `thread_unread` goes to none); reading the channel never
    /// does. A message without replies is a thread of none.
    #[serde(rename = "thread.read")]
    ThreadRead { message_id: MessageId },
    /// The company in one read: employees, the org chart setting and the
    /// management team (roots).
    #[serde(rename = "company.get")]
    CompanyGet,
    /// The board: its six columns left to right, each with its cards (not
    /// archived), for one project or every project.
    #[serde(rename = "board.get")]
    BoardGet {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project_id: Option<ProjectId>,
    },
    /// One ticket with its main task and every task linked to it.
    #[serde(rename = "ticket.get")]
    TicketGet { ticket_id: aurelius_domain::TicketId },
    /// Tickets in board order (not archived); every given filter applies. `query`
    /// searches title, description and `#number`.
    #[serde(rename = "ticket.list")]
    TicketList {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project_id: Option<ProjectId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        column: Option<aurelius_domain::ticket::ColumnKind>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        query: Option<String>,
    },
    /// The History (feature `ticket_history`, ticket #57): tickets on Done, the newest
    /// Done first, each with the summary of what it delivered. Every given filter
    /// applies: `employee_id` is the ticket's owner, `from_ms`/`to_ms` the period of its
    /// Done (`to_ms` excluded), `query` looks in the title and the summary. Keyset
    /// paging: the next page passes the last row's `done_at_ms` and `number` as
    /// `before_done_at_ms`/`before_number`. `limit`: at most 100, default 50. The same
    /// rights as `board.get` (B3); an archived ticket or project stays in it.
    #[serde(rename = "ticket.history")]
    TicketHistory {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project_id: Option<ProjectId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        employee_id: Option<EmployeeId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from_ms: Option<i64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        to_ms: Option<i64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        query: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        before_done_at_ms: Option<i64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        before_number: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        limit: Option<u32>,
    },
    /// Work sessions in the History (feature `session_history`): every board member's
    /// sessions that were not deleted, the latest activity first, each with the summary
    /// of what was done in it. Every given filter applies: `employee_id` is the employee
    /// who works in it, `from_ms`/`to_ms` the period of its last activity (`to_ms`
    /// excluded), `query` looks in the title and the summary. Keyset paging: the next
    /// page passes the last row's `last_activity_ms` and `conversation_id` as
    /// `before_last_activity_ms`/`before_conversation_id`. `limit`: at most 100, default
    /// 50. The same rights as `ticket.history`.
    #[serde(rename = "work_session.history")]
    WorkSessionHistory {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project_id: Option<ProjectId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        employee_id: Option<EmployeeId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from_ms: Option<i64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        to_ms: Option<i64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        query: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        before_last_activity_ms: Option<i64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        before_conversation_id: Option<aurelius_domain::ConversationId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        limit: Option<u32>,
    },
    /// Who follows a ticket (feature `ticket_follow`). Not for apps.
    #[serde(rename = "ticket.followers")]
    TicketFollowers { ticket_id: aurelius_domain::TicketId },
    /// The tickets of the board member of the session (feature `my_tickets`),
    /// never of one named in a param: responsible, created or followed by them,
    /// and open ones on their login or without an active responsible. Done and
    /// archived ones only with `include_done`. By priority, then the latest
    /// update; at most `MY_TICKETS_MAX`. Not for apps.
    #[serde(rename = "ticket.my_tickets")]
    TicketMyTickets {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project_id: Option<ProjectId>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        include_done: bool,
    },
    /// Where a message is (feature `message_locate`), for a link to it: its
    /// conversation, ticket and thread. Only a message of a channel or a ticket's
    /// discussion; any other is `not_found`, as an unknown one. Not for apps.
    #[serde(rename = "message.locate")]
    MessageLocate { message_id: MessageId },
    /// The earlier texts of a message, newest first (feature `message_edits`);
    /// none for a deleted one. Not for apps.
    #[serde(rename = "message.history")]
    MessageHistory { message_id: MessageId },
    /// The board members, oldest first (never a password's hash), disabled ones
    /// too (`UserView::disabled_at_ms`). Works without a session (the sign-in
    /// screen may offer the names). Apps may read it too.
    #[serde(rename = "user.list")]
    UserList,
    /// Whether the server has board members, and this connection's session's one.
    /// Works without a session.
    #[serde(rename = "user.whoami")]
    UserWhoami,
    /// The workspaces the board member of the session pinned to their rail
    /// (feature `rail_pins`), never another's. Not for apps.
    #[serde(rename = "user.rail_pins")]
    UserRailPins,
    /// Sign in (protocol 5): a right username and password give this connection a
    /// session and answer `Login` with its token, shown once (keep it for the
    /// server and give it in every hello). A wrong password, an unknown username
    /// and a disabled board member all answer `wrong_password`; after 5 failed tries
    /// for one username the server waits longer each time, up to 15 minutes
    /// (`too_many_attempts` with `retry_after_ms`). A session method: it changes
    /// only the connection and the sessions, so it has no receipt and no event.
    /// Not for apps.
    #[serde(rename = "user.login")]
    UserLogin(LoginParams),
    /// End this connection's session (the device must sign in again); answers
    /// `WhoAmI` without a board member. A session method.
    #[serde(rename = "user.logout")]
    UserLogout,
    /// The signed-in devices of a board member (none: the session's own),
    /// newest activity first (`SessionView::current` marks this one). Any board
    /// member may list another's (one role). Never a token or its hash.
    #[serde(rename = "session.list")]
    SessionList {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        user_id: Option<aurelius_domain::UserId>,
    },
    /// The session's own Claude login (`ClaudeLoginView`). Feature of protocol 5.
    #[serde(rename = "claude_login.status")]
    ClaudeLoginStatus,
    /// What a running `claude_login.link` asks for: the code Claude showed after
    /// the sign-in at Anthropic, written to `claude auth login` as typed. Only
    /// the connection that started the link. A session method: no receipt, no
    /// request hash, no event, never printed or logged. Answers `ClaudeLogin`.
    #[serde(rename = "claude_login.input")]
    ClaudeLoginInput(ClaudeLoginInputParams),
    /// The vault of a scope (feature `vault`): none, the company's items; a
    /// project, its items plus the company's apart (`VaultList::company_items`),
    /// with `overrides_company` on a project item whose env name the company
    /// has too, and the item count of every scope. Never a secret's value.
    /// Archived items are left out. Board members only; not for apps (B5).
    #[serde(rename = "vault.list")]
    VaultList {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project_id: Option<ProjectId>,
    },
    /// One item's value (`VaultValue`, never printed), for a board member with a
    /// session. Writes an audit row `reveal` (show) or `copy` with the session's
    /// device. `vault_undecryptable` when the key does not fit. Not for apps.
    #[serde(rename = "vault.reveal")]
    VaultReveal { item_id: aurelius_domain::VaultItemId, purpose: aurelius_domain::vault::RevealPurpose },
    /// The audit log, newest first (`VaultAuditPage`): the company's (nothing
    /// given), a project's, or every scope's (`all`); one item's with `item_id`.
    /// `before`: rows older than that row id; `limit` at most 200 (default 50).
    /// Never a value. Not for apps.
    #[serde(rename = "vault.audit")]
    VaultAudit {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project_id: Option<ProjectId>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        all: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        item_id: Option<aurelius_domain::VaultItemId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        before: Option<i64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        limit: Option<u32>,
    },
    /// The server's vault key (`VaultKeyExport`, never printed), for a restore on
    /// another machine (B8): a board member with a session that needs no new
    /// password, who gives their password again (`wrong_password`; it counts
    /// for the sign-in limits, `too_many_attempts`). Writes an audit row
    /// `key_export` and tells every board member. Not found before the first
    /// item. Not for apps.
    #[serde(rename = "vault.key_export")]
    VaultKeyExport(KeyExportParams),
    /// The guideline library (feature `guidelines`): every guideline without its
    /// text, alphabetical; archived ones only with `archived` (then only those).
    #[serde(rename = "guideline.list")]
    GuidelineList {
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        archived: bool,
    },
    /// One guideline with the text of its current version, or of `version`.
    #[serde(rename = "guideline.get")]
    GuidelineGet {
        guideline_id: aurelius_domain::GuidelineId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        version: Option<u32>,
    },
    /// Every version of a guideline, newest first, with its text and source.
    #[serde(rename = "guideline.versions")]
    GuidelineVersions { guideline_id: aurelius_domain::GuidelineId },
    /// The set a run of this employee in this project would get now, in block
    /// order, with `total_chars` against `limit_chars` (B4). Only an employee:
    /// its own chat (employee guidelines); only a project: its guidelines alone.
    #[serde(rename = "guideline.effective")]
    GuidelineEffective {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        employee_id: Option<EmployeeId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project_id: Option<ProjectId>,
    },
    /// Proposals of `propose_guideline` (B5), pending first, then the latest
    /// decisions; `status` filters. Apart from `proposal.list`.
    #[serde(rename = "guideline.proposals")]
    GuidelineProposals {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        status: Option<aurelius_domain::proposal::ProposalStatus>,
    },
    /// Who a change of this guideline reaches from their next run (B6): employees,
    /// projects and conversations that start a new session.
    #[serde(rename = "guideline.affected")]
    GuidelineAffected { guideline_id: aurelius_domain::GuidelineId },
    /// The templates of the starter set (B7), for "Add starter set…".
    #[serde(rename = "guideline.starter_set")]
    GuidelineStarterSet,
    /// The guideline versions a run had in its system text (B6), kept for every
    /// run (the trace's "Guidelines: Rust v3, …"); also without trace mode.
    #[serde(rename = "run.guidelines")]
    RunGuidelines { run_id: RunId },
    /// The Code tab of a workspace (feature `code_view`, ticket #77): the git state of
    /// the project's folder (worktrees, local branches, recent commits). A folder that is
    /// no git repository answers a view with `problem` set, not an error. The same
    /// rights as `board.get`.
    #[serde(rename = "code.get")]
    CodeGet { project_id: ProjectId },
    /// What merging a branch would bring into main, file by file with its hunks, plus what
    /// its worktree has not committed (feature `code_review`, ticket #84). Redacted.
    #[serde(rename = "code.diff")]
    CodeDiff { project_id: ProjectId, branch: String },
    /// A branch's review as the board member of the session sees it: line threads (with
    /// only their own pending comments), finished reviews, and the files they viewed at
    /// `head` (feature `code_review`, ticket #84). Not for apps.
    #[serde(rename = "code.review")]
    CodeReview {
        project_id: ProjectId,
        branch: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        head: Option<String>,
    },
    /// The conversations of a project, most recently active first.
    #[serde(rename = "project_conversation.list")]
    ProjectConversationList { project_id: ProjectId },
    /// Pending approvals, then the latest decisions. `project_id` (feature
    /// `workspace_filters`): only those of runs in that project.
    #[serde(rename = "approval.list")]
    ApprovalList {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project_id: Option<ProjectId>,
    },
    #[serde(rename = "connection.list")]
    ProviderList,
    /// Tasks, newest first, company-wide; every given filter applies.
    #[serde(rename = "task.list")]
    TaskList {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project_id: Option<ProjectId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        employee_id: Option<EmployeeId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        state: Option<TaskListState>,
    },
    /// Human-readable timeline cards, newest first (keyset on `seq`).
    #[serde(rename = "timeline.list")]
    TimelineList(TimelineQuery),
    /// Memories, newest first: global always, plus the scopes asked for (a
    /// project, an employee). Nothing asked: global and every employee.
    #[serde(rename = "memory.list")]
    MemoryList {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project_id: Option<ProjectId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        employee_id: Option<EmployeeId>,
    },
    /// What goes into a context: an employee's (own, the project if given,
    /// global) or a project conversation's (project, global; no employee). At
    /// least one of the two.
    #[serde(rename = "memory.get_context")]
    MemoryGetContext {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project_id: Option<ProjectId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        employee_id: Option<EmployeeId>,
    },
    #[serde(rename = "recurring_task.list")]
    RecurringTaskList { project_id: Option<ProjectId> },
    /// The context manifest of one run (context inspector).
    #[serde(rename = "run.get_context")]
    RunGetContext { run_id: RunId },
    /// A screenshot the agent took (the id is in its screenshot tool's output).
    #[serde(rename = "screenshot.get")]
    ScreenshotGet { id: String },
    /// A file kept with a message (see `MessageView::files`).
    #[serde(rename = "file.get")]
    FileGet { id: String },
    /// A file the user uploaded with a message, task, recurring task or idea (see
    /// `MessageFileView::id`), in parts of at most `UPLOAD_CHUNK_BYTES` from
    /// `offset` (none: 0). With `max_side` an image comes back whole as a JPEG
    /// that fits that many pixels (clamped to `UPLOAD_THUMB_MAX_SIDE`); other files are refused.
    #[serde(rename = "upload.get")]
    UploadGet {
        id: String,
        #[serde(default)]
        offset: Option<u64>,
        #[serde(default)]
        max_side: Option<u32>,
    },
    #[serde(rename = "idea.list")]
    IdeaList { project_id: ProjectId },
    /// The ideas of every project (each keeps its own project).
    #[serde(rename = "idea.list_all")]
    IdeaListAll {},
    /// Proposals of the whole company (colleagues and instructions): pending
    /// first, then the latest decisions.
    #[serde(rename = "proposal.list")]
    ProposalList,
    /// The versions of an employee's instructions, newest first (feature `instruction_proposals`).
    #[serde(rename = "employee.instruction_versions")]
    EmployeeInstructionVersions { employee_id: aurelius_domain::EmployeeId },
    #[serde(rename = "usage.get")]
    UsageGet,
    /// Usage per hour of one server-local day (any moment in it).
    #[serde(rename = "usage.get_hours")]
    UsageGetHours { day_start_ms: i64 },
    /// What a conversation or a task used, with the tasks given away from it apart
    /// (feature `run_usage`). Give one of the two; a task wins.
    #[serde(rename = "usage.get_totals")]
    UsageGetTotals {
        #[serde(default)]
        conversation_id: Option<ConversationId>,
        #[serde(default)]
        task_id: Option<aurelius_domain::TaskId>,
        /// What a ticket used: its tasks and the runs of its discussion (feature `run_cards`).
        /// Wins over the other two.
        #[serde(default)]
        ticket_id: Option<aurelius_domain::TicketId>,
    },
    /// What one run sent to the runtime and what each model call cost; not found
    /// unless trace mode was on for it and it is among the latest (feature `run_trace`).
    #[serde(rename = "run.get_trace")]
    RunGetTrace { run_id: RunId },
    /// Every tool call of a run, for an opened run card (feature `run_cards`).
    #[serde(rename = "run.get_activity")]
    RunGetActivity { run_id: RunId },
    /// Whether runs keep a trace (feature `run_trace`).
    #[serde(rename = "trace.get_setting")]
    TraceGetSetting,
    /// Update status; `check` asks GitHub for the latest release first.
    #[serde(rename = "update.get")]
    UpdateGet { check: bool },
    #[serde(rename = "power.get")]
    PowerGet,
    #[serde(rename = "load.get")]
    LoadGet,
    #[serde(rename = "browser.get")]
    BrowserGet,
    /// Whether management chooses its own task mode (feature `management_chooses_mode`).
    #[serde(rename = "management_mode.get_setting")]
    ManagementModeGetSetting,
    /// Above how many tokens a session is compacted (feature `compaction_setting`).
    #[serde(rename = "compaction.get_setting")]
    CompactionGetSetting,
    /// Whether the safe read-only tools pass without a question (the auto-allow list).
    #[serde(rename = "auto_allow.get_setting")]
    AutoAllowGetSetting,
    /// The user-scope MCP servers of Claude Code and which of them every run uses.
    #[serde(rename = "mcp.list")]
    McpList,
    /// A login started with `mcp.login_start`; kept for a while after it ends.
    #[serde(rename = "mcp.login_get")]
    McpLoginGet { login_id: String },
    /// The MCP tools the user always allows (answered an approval with 'Always allow').
    #[serde(rename = "mcp_always_allow.list")]
    McpAlwaysAllowList,
    #[serde(rename = "subscription.get")]
    SubscriptionGet { refresh: bool },
    #[serde(rename = "backup.list")]
    BackupList,
    /// The notifications of the board member, newest first. `project_id` (feature
    /// `workspace_filters`): only those about that project's tickets, tasks or
    /// conversations; `unread` then counts only those too.
    #[serde(rename = "notification.list")]
    NotificationList {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project_id: Option<ProjectId>,
    },
    /// The Markdown files in a work folder, for the in-app viewer.
    #[serde(rename = "markdown.list")]
    MarkdownList { scope: FileScope },
    /// One Markdown file as UTF-8 text. `path` is relative to the work folder.
    #[serde(rename = "markdown.get")]
    MarkdownGet { scope: FileScope, path: String },
    /// The subagents running in the answers of one conversation.
    #[serde(rename = "subagent.list")]
    SubagentList { conversation_id: ConversationId },
    /// One tool call of an answer whole: the input and output that
    /// `conversation.get` and live updates shorten (feature `activity_preview`).
    #[serde(rename = "activity.get")]
    ActivityGet { message_id: MessageId, call_id: String },
    #[serde(rename = "remote.get_status")]
    RemoteGetStatus,
    /// The devices the session's board member paired, newest first (feature
    /// `own_devices`). Never another board member's. A session method.
    #[serde(rename = "device.list")]
    DeviceList,
    /// Folders on the server: none lists the starting points.
    #[serde(rename = "fs.list_dir")]
    FsListDir { path: Option<String> },
    /// Where Claude Code is installed on the server.
    #[serde(rename = "connection.detect_runtime")]
    ProviderDetectRuntime,

    /// A side question about a conversation (feature `side_question`), like
    /// Claude Code's `/btw`: answered from what the conversation holds, without
    /// tools, on the asker's own Claude login, and never stored: no message, no
    /// event, the employee never sees it and its run goes on. Answers at once with
    /// `SideAsked`; the text follows to this connection only as
    /// `ServerMessage::SideAnswer`. Board members only; not for apps.
    #[serde(rename = "conversation.side_ask")]
    ConversationSideAsk(SideAskParams),
    /// Stop a side question of this connection that is still answering. Unknown
    /// or finished ids are fine (nothing to stop). Answers `SideAsked` with the
    /// same id; the answer ends with a `SideAnswer` that is `done`.
    #[serde(rename = "conversation.side_cancel")]
    ConversationSideCancel { side_id: String },

    /// Replay events after `after_seq`, then stream live events (and load/power pushes).
    #[serde(rename = "event.subscribe")]
    EventSubscribe { after_seq: u64 },
}

/// Which tasks `task.list` gives by state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TaskListState {
    /// Not ended: draft, queued, running, waiting, review.
    Open,
    /// Waiting for the user (input, an approval) or in review.
    NeedsYou,
    /// Succeeded, failed or cancelled.
    Ended,
}

impl TaskListState {
    pub fn matches(self, status: aurelius_domain::task::TaskStatus, waiting_reason: Option<&str>) -> bool {
        use aurelius_domain::task::TaskStatus::*;
        match self {
            TaskListState::Open => !matches!(status, Succeeded | Failed | Cancelled),
            TaskListState::NeedsYou => status == Review || (status == Waiting && matches!(waiting_reason, Some("input") | Some("approval"))),
            TaskListState::Ended => matches!(status, Succeeded | Failed | Cancelled),
        }
    }
}

impl Method {
    /// Whether `name` is a read or session method of this server (taken from the schema once).
    pub fn knows(name: &str) -> bool {
        static NAMES: std::sync::LazyLock<Vec<String>> = std::sync::LazyLock::new(|| {
            let schema = serde_json::to_value(schemars::schema_for!(Method)).unwrap_or_default();
            schema["oneOf"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| v["properties"]["method"]["const"].as_str().map(str::to_owned))
                .collect()
        });
        NAMES.iter().any(|n| n == name)
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HelloParams {
    pub client_name: String,
    pub client_version: String,
    /// What this client can take that older clients cannot (`client_feature`).
    /// Servers without such a feature ignore the name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub accepts: Vec<String>,
    /// The session this device got from `user.login` (or the first
    /// `user.create`) for this server: valid, it makes its board member this
    /// connection's (`HelloResult::user`); unknown, revoked or expired, the
    /// hello still works without a board member and the client shows its
    /// sign-in screen. Ignored for apps. Never printed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_token: Option<String>,
}

/// `user.login`. The password is never printed.
#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LoginParams {
    pub username: String,
    pub password: String,
    /// How the device shows in `session.list` ("Desktop · dev-laptop").
    pub device_name: String,
}

impl std::fmt::Debug for LoginParams {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoginParams").field("username", &self.username).field("password", &"<redacted>").field("device_name", &self.device_name).finish()
    }
}

/// `vault.key_export`. The password is never printed.
#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct KeyExportParams {
    /// The signed-in board member's own password, asked again.
    pub password: String,
}

impl std::fmt::Debug for KeyExportParams {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeyExportParams").field("password", &"<redacted>").finish()
    }
}

/// `claude_login.input`. The text (a one-time code) is never printed.
#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ClaudeLoginInputParams {
    pub link_id: String,
    pub text: String,
}

impl std::fmt::Debug for ClaudeLoginInputParams {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClaudeLoginInputParams").field("link_id", &self.link_id).field("text", &"<redacted>").finish()
    }
}

/// `conversation.side_ask`. The question is never stored, so it is not printed either.
#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SideAskParams {
    pub conversation_id: ConversationId,
    /// At most `SIDE_QUESTION_MAX_CHARS` characters, not blank.
    pub question: String,
}

impl std::fmt::Debug for SideAskParams {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SideAskParams").field("conversation_id", &self.conversation_id).field("question", &"<not printed>").finish()
    }
}

/// The longest side question, in characters.
pub const SIDE_QUESTION_MAX_CHARS: usize = 2_000;

/// What `HelloParams::accepts` names.
pub mod client_feature {
    /// Live updates may carry only the tool calls that are new or changed
    /// (`StreamDelta::activity_changes`); the client merges them by `call_id`.
    pub const ACTIVITY_CHANGES: &str = "activity_changes";
    /// The client shows the Team chat's channels (feature `team_channels`):
    /// only such a connection gets the live events of project channels, so an
    /// older client never shows them in its one Team chat.
    pub const TEAM_CHANNELS: &str = "team_channels";
    /// The client knows the event `server.renamed` (feature `server_name`): only
    /// such a connection gets it, so an older client never reads an event kind
    /// it does not know (SN-02 AC3).
    pub const SERVER_NAME: &str = "server_name";
    /// The client shows ticket channels (feature `ticket_channels`): only such a
    /// connection gets them in `channel.list` and `channel.search`, and gets the
    /// error code `channel_archived`, so an older client sees the list of before
    /// and never reads a code it does not know.
    pub const TICKET_CHANNELS: &str = "ticket_channels";
    /// The client shows threads (feature `chat_threads`): only such a connection
    /// gets the live text of an answer streaming into a thread
    /// (`StreamDelta::thread_parent`), so an older client never shows a reply
    /// in the channel's own list, even for a moment.
    pub const CHAT_THREADS: &str = "chat_threads";
    /// The client shows reactions (feature `chat_reactions`): only such a
    /// connection gets the event `message.reacted`, so an older client never
    /// reads an event kind it does not know.
    pub const CHAT_REACTIONS: &str = "chat_reactions";
    /// The client shows who follows a ticket (feature `ticket_follow`): only such
    /// a connection gets the event `ticket.follow_changed`, so an older client
    /// never reads an event kind it does not know.
    pub const TICKET_FOLLOW: &str = "ticket_follow";
    /// The client shows edited messages (feature `message_edits`): only such a
    /// connection gets the event `message.edited`. An older client sees the new
    /// text at its next `conversation.get`.
    pub const MESSAGE_EDITS: &str = "message_edits";
    /// The client shows a design while it is written (feature `artifact_drafts`):
    /// only such a connection gets `StreamDelta::draft`, so no other client is
    /// sent a growing HTML file it does not show.
    pub const ARTIFACT_DRAFTS: &str = "artifact_drafts";
}

impl std::fmt::Debug for HelloParams {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HelloParams")
            .field("client_name", &self.client_name)
            .field("client_version", &self.client_version)
            .field("accepts", &self.accepts)
            .field("session_token", &self.session_token.as_ref().map(|_| "<hidden>"))
            .finish()
    }
}
