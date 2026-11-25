//! The envelope of protocol 3.0: one JSON object per message. Frames on local IPC
//! are a 4-byte big-endian length plus this JSON; the gateway sends it as one
//! WebSocket text frame.

use crate::events::Event;
use crate::method::Method;
use crate::remote::RemoteStatus;
use crate::views::*;
use crate::write::{Receipt, WriteMethod};
use aurelius_domain::views::*;
use aurelius_domain::{AppError, ClientId, CommandId, ErrorCode, HostId};
use schemars::JsonSchema;
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// The protocol major. The handshake checks only this; a mismatch is a hard
/// "update the app or the server" (`incompatible_protocol`, "Update needed").
/// There are no minors: additions within a major are announced in
/// `HelloResult::features` (see `FEATURES`). 3: the company (employees, one
/// org chart). 4: departments are gone (methods, fields and commands).
/// 5: sign-in and sessions (`user.login`, `HelloParams::session_token`; `user.use`
/// and the remembered `user_id` are gone) and the gate: nothing without a session
/// but the hello, `user.whoami`, `user.login`, `user.list` and the first `user.create`.
pub const PROTOCOL_MAJOR: u16 = 5;

/// The message of `incompatible_protocol`: what the user must do.
pub fn incompatible_message(client: u16) -> String {
    if client < PROTOCOL_MAJOR {
        format!("Update needed: this app speaks protocol {client}, the server {PROTOCOL_MAJOR}. Update the app.")
    } else {
        format!("Update needed: this app speaks protocol {client}, the server {PROTOCOL_MAJOR}. Update the server.")
    }
}

/// Client → daemon.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    /// The client's protocol major; must equal the server's.
    pub protocol: u16,
    pub request_id: u64,
    /// Required for writes: the same key with the same request is answered from
    /// the receipt; the same key with another request is a conflict.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<CommandId>,
    /// Optimistic concurrency for writes that change a versioned entity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_version: Option<u64>,
    #[serde(flatten)]
    pub call: Call,
}

impl Request {
    pub fn read(request_id: u64, method: Method) -> Self {
        Self { protocol: PROTOCOL_MAJOR, request_id, idempotency_key: None, expected_version: None, call: Call::Read(method) }
    }

    pub fn write(request_id: u64, key: CommandId, write: WriteMethod) -> Self {
        Self { protocol: PROTOCOL_MAJOR, request_id, idempotency_key: Some(key), expected_version: None, call: Call::Write(write) }
    }
}

/// A read or session method, or a write. On the wire both are `{ method, params }`;
/// the method name says which.
#[derive(Debug, Clone, PartialEq)]
pub enum Call {
    Read(Method),
    Write(WriteMethod),
}

impl Serialize for Call {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Call::Read(method) => method.serialize(serializer),
            Call::Write(write) => write.serialize(serializer),
        }
    }
}

/// The answer to a message that is not a valid `Request`. The request id is read
/// leniently so the client can match the error; a method this server does not
/// know is `unknown_method` (how newer clients detect an older server), anything
/// else `invalid_request`. The connection stays open either way.
pub fn reject_unparsed(value: Option<&serde_json::Value>, err: &dyn std::fmt::Display) -> ServerMessage {
    let request_id = value.and_then(|v| v.get("request_id")).and_then(|id| id.as_u64()).unwrap_or(0);
    let method = value.and_then(|v| v.get("method")).and_then(|m| m.as_str());
    let error = match method {
        Some(name) if !Method::knows(name) && !WriteMethod::knows(name) => {
            AppError::new(ErrorCode::UnknownMethod, format!("This server does not know the method {name}."))
        }
        // serde quotes values it could not take: never for a method with a secret in it (SEC0 E5).
        Some(name) if quotes_no_params(name) => AppError::new(ErrorCode::InvalidRequest, format!("The request for {name} is not valid.")),
        _ => AppError::new(ErrorCode::InvalidRequest, err.to_string()),
    };
    ServerMessage::Response { request_id, outcome: Outcome::Error(error) }
}

/// Methods whose params may hold a value, a password or a code: an error never
/// repeats what serde said about them.
pub fn quotes_no_params(name: &str) -> bool {
    name.starts_with("vault.") || matches!(name, "user.login" | "user.create" | "user.set_password" | "claude_login.input" | "session.hello" | "message.edit" | "conversation.side_ask")
}

impl<'de> Deserialize<'de> for Call {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        let name = value.get("method").and_then(|m| m.as_str()).unwrap_or("").to_owned();
        if WriteMethod::knows(&name) {
            from_method_value(value).map(Call::Write).map_err(D::Error::custom)
        } else {
            from_method_value(value).map(Call::Read).map_err(|e| D::Error::custom(format!("{name}: {e}")))
        }
    }
}

/// A `{ method, params }` object into a method enum. Params whose fields are
/// all optional may be left out or `null` (section 4 of the protocol): a method
/// that needs an object there is tried again with `{}`. The other way round, a
/// method without params also takes `"params": {}`.
pub(crate) fn from_method_value<T: serde::de::DeserializeOwned>(value: serde_json::Value) -> Result<T, serde_json::Error> {
    let fallback = match value.get("params") {
        None | Some(serde_json::Value::Null) => Some(serde_json::json!({})),
        Some(serde_json::Value::Object(o)) if o.is_empty() => None,
        Some(_) => return serde_json::from_value(value),
    };
    serde_json::from_value(value.clone()).or_else(|err| {
        let mut retry = value;
        let Some(object) = retry.as_object_mut() else { return Err(err) };
        match fallback {
            Some(params) => {
                object.insert("params".into(), params);
                serde_json::from_value(retry)
            }
            // A method with params says best what is missing in them.
            None => {
                object.remove("params");
                serde_json::from_value(retry).map_err(|_| err)
            }
        }
    })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HelloResult {
    pub host_id: HostId,
    pub daemon_version: String,
    pub protocol: u16,
    /// `dev` or `production`; a client never talks to the other environment.
    pub environment: String,
    /// The server's computer name.
    pub host_name: String,
    /// The server's OS: `windows`, `macos` or `linux`.
    pub os: String,
    /// This connection came in over the network.
    pub remote: bool,
    /// The paired device behind a remote connection.
    pub client_id: Option<ClientId>,
    pub capabilities: Vec<aurelius_domain::remote::Capability>,
    /// Additions to protocol 3.0 this server has (see `FEATURES`); a client hides
    /// what is missing, since an unknown method is not a clean error. Older servers leave it out.
    #[serde(default)]
    pub features: Vec<String>,
    /// The server has at least one board member; false: the onboarding
    /// ("Create the first board member") is all a client shows.
    #[serde(default)]
    pub board_members_exist: bool,
    /// The board member of this connection's session (`HelloParams::session_token`
    /// when it is valid); none: the client shows its sign-in screen (or the
    /// onboarding without board members).
    #[serde(default)]
    pub user: Option<aurelius_domain::user::UserView>,
    /// The session's board member must change a temporary password first.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub must_change_password: bool,
    /// The session's own Claude login; none without a session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude_login: Option<aurelius_domain::user::ClaudeLoginView>,
}

/// What `HelloResult::features` names.
pub mod feature {
    /// Uploaded files carry an id (`MessageFileView::id`) and `upload.get` fetches
    /// them, whole or as a thumbnail.
    pub const UPLOAD_FETCH: &str = "upload_fetch";
    /// `project.set_description` (`set_project_description`): change or clear a
    /// project's description after creation.
    pub const PROJECT_DESCRIPTION: &str = "project_description";
    /// Ideas have files: `IdeaView::attachments` (fetched with `upload.get`),
    /// `AddIdea::files`, `UpdateIdea::add_files` / `remove_files`, and
    /// `idea.delete`. Starting an idea gives the task copies of them.
    pub const IDEA_ATTACHMENTS: &str = "idea_attachments";
    /// A ticket made from an idea: `ticket.create { from_idea }`, `IdeaView::ticket_id`
    /// and `ticket_number`, and the event `idea.ticket_created`. A client offers
    /// "Create ticket" on an idea only when the server has this.
    pub const TICKET_FROM_IDEA: &str = "ticket_from_idea";
    /// Tokens per answer (`MessageView::usage`), `usage.get_totals` per conversation
    /// or task, and cache writes apart (`cache_creation_tokens` in the usage views).
    pub const RUN_USAGE: &str = "run_usage";
    /// Trace mode: `trace.get_setting`, `trace.set_enabled` and `run.get_trace`.
    pub const RUN_TRACE: &str = "run_trace";
    /// Lessons learned from tasks (Harness² A): `MemoryView::lesson` (when it
    /// applies, evidence S1..S8, times confirmed, tasks) on episodic memories,
    /// event `memory.reinforced`, and failed tasks are extracted too.
    pub const TASK_LESSONS: &str = "task_lessons";
    /// Instruction proposals (Harness² B): `ProposalView::kind` / `target_employee_id` /
    /// `base_version` / `current_instructions` / `current_version` / `evidence`,
    /// `DecideProposal::instructions`, `employee.restore_instructions`,
    /// `employee.instruction_versions` and event `employee.instructions_proposed`.
    pub const INSTRUCTION_PROPOSALS: &str = "instruction_proposals";
    /// A conversation's runtime session whose context grew past the server's
    /// threshold is compacted before its next turn: a `compacted` update ("Context
    /// compacted (≈X → Y tokens)") in the conversation, and a `session` part in the run's trace.
    pub const SESSION_COMPACTION: &str = "session_compaction";
    /// Short tool use: `conversation.get` and live updates shorten each tool
    /// call (`ActivityView::truncated` / `output_bytes`), `activity.get` returns
    /// one whole, and a client that sends `HelloParams::accepts`
    /// `activity_changes` gets only new or changed calls in live updates.
    pub const ACTIVITY_PREVIEW: &str = "activity_preview";
    /// Quick replies: an answer can end with a `suggest_replies` tool use
    /// (`ActivityView::tool` `mcp__aurelius__suggest_replies`, input `question` and
    /// `options`); clients show the options as buttons that send one as the user's
    /// message. No new methods; older clients show it as a plain tool step.
    pub const QUICK_REPLIES: &str = "quick_replies";
    /// While a run compacts the conversation's context before its turn, its `RunProgress`
    /// (stream updates and `JobSummary::live`) has `compacting: true`; older servers
    /// never set it and clients show plain work.
    pub const COMPACT_STATUS: &str = "compact_status";
    /// Management (the CEO and those who report to them) chooses its own task mode, Auto or
    /// Plan, when the user picked none: `management_mode.get_setting` /
    /// `management_mode.set_enabled` (`ManagementModeSetting`), the `mode_chosen` task
    /// update ("<name> chose Auto: <reason>") and the event `task.mode_chosen`.
    pub const MANAGEMENT_CHOOSES_MODE: &str = "management_chooses_mode";
    /// Auto-allow of the safe read-only tools (Read, Grep, Glob, WebSearch, list_windows) in
    /// every mode but Plan: `auto_allow.get_setting` / `auto_allow.set_enabled`
    /// (`AutoAllowSetting`, on by default, Approve right).
    pub const AUTO_ALLOW: &str = "auto_allow";
    /// One discussion per ticket (protocol 4, additive): `TicketView::conversation_id`
    /// (read with `conversation.get`), the write `ticket.post`, `MessageView::author_user`
    /// and `MessageView::system` (system lines, never a new `MessageRole`),
    /// `ticket.create { from_message }` and `ticket.retry_summary`. A discussion
    /// of a ticket on Done is read-only.
    pub const TICKET_DISCUSSION: &str = "ticket_discussion";
    /// Channels in the Team chat: `channel.list`, `group_chat.get { channel }`,
    /// `group_chat.send { channel }`. The Team chat is gone (migration 45): no longer
    /// advertised in `FEATURES`; the methods only answer for a ticket's channel.
    pub const TEAM_CHANNELS: &str = "team_channels";
    /// The Testing column (protocol 5, additive): `ColumnKind::Testing` between
    /// Plan review and Review, the company tester (`company.update { tester }`,
    /// `CompanyView::tester_employee_id`) and the event `ticket.tested`. A client
    /// shows Testing and the tester picker only when the server has this.
    pub const BOARD_TESTING: &str = "board_testing";
    /// The vault (protocol 5, additive): `vault.list`, `vault.reveal`,
    /// `vault.audit`, `vault.key_export`, the writes `vault.create`,
    /// `vault.update`, `vault.archive`, the events `vault.item_created` /
    /// `_updated` / `_archived` (never a value), the error codes
    /// `vault_kind_locked`, `vault_undecryptable` and `looks_like_secret`
    /// (`AppError::matched_items`), and `confirm_secret` on `employee.send_message`,
    /// `ticket.post` and `group_chat.send` (B10). Board members only; never apps.
    pub const VAULT: &str = "vault";
    /// Guidelines (protocol 5, additive; docs/product/guidelines): the reads
    /// `guideline.list`, `.get`, `.versions`, `.effective`, `.proposals`,
    /// `.affected`, `.starter_set` and `run.guidelines`; the writes
    /// `guideline.create`, `.update`, `.archive`, `.restore`, `.restore_version`,
    /// `.link`, `.unlink`, `.decide_proposal`, `.add_starter_set` (board members
    /// only; never apps); the `guideline.*` events; the error codes
    /// `guideline_too_large` (`AppError::too_large`) and `guideline_title_taken`;
    /// context items with section `guideline`.
    pub const GUIDELINES: &str = "guidelines";
    /// The server's name (protocol 5, additive; docs/product/servernaam): the
    /// read `server.get_name`, the write `server.set_name` (board members only;
    /// never apps; remote: owner) and the event `server.renamed`, only to
    /// connections whose `HelloParams::accepts` names `server_name`. The hello's
    /// `host_name`, the pairing text and the pairing poll give the chosen name.
    pub const SERVER_NAME: &str = "server_name";
    /// `@Everyone` (protocol 5, additive; ticket #40): a board member's message
    /// with `@Everyone` in a channel or a ticket's discussion wakes one employee,
    /// the coordinator (ticket #45; before that, every active employee in it), and
    /// tells the board members in it, once each; the other employees read it when
    /// next woken there. The read `discussion.audience { channel }` names and counts
    /// them before sending (`DiscussionAudience`; `coordinator` absent on an older
    /// server). The text is plain text on the wire: a client
    /// marks `@Everyone` itself. An employee's `@Everyone` is only text.
    pub const EVERYONE_MENTION: &str = "everyone_mention";
    /// A channel per ticket (protocol 5, additive; AUR-CHAT-001): the ticket's
    /// discussion is its channel, `#<number>-<slug>` (`ticket_channel_name`).
    /// `channel.list` names ticket channels only to connections whose
    /// `HelloParams::accepts` names `ticket_channels` (any other sees the list of
    /// before), with `ChannelView::kind`, `ticket_id`, `ticket_number`,
    /// `ticket_column`, `member`, `member_count` and `read_through`; a closed
    /// ticket's channel is `archived`. The read `channel.search { query }`; the
    /// writes `group_chat.send { channel }` to a ticket's channel; the error code
    /// `channel_archived` (only to such connections); `SystemLine::action` and
    /// `SystemLine::channel_id`, with one line in General when a ticket opens and
    /// one when it closes.
    pub const TICKET_CHANNELS: &str = "ticket_channels";
    /// Threads in channels (protocol 5, additive; AUR-CHAT-002): `thread_parent`
    /// on `group_chat.send` / `ticket.post` (a message of the same channel that is
    /// not itself a reply); the read `thread.read { message_id }` (`ThreadSnapshot`,
    /// marks the thread read); `MessageView::thread_parent`, `thread_reply_count`,
    /// `thread_last_reply_ms`, `thread_unread` and `thread_repliers`. Replies are never in the
    /// channel's own list and not in its `unread`; unread replies that name the
    /// reader count in its `mentions`. A reply gives the events a message of the
    /// channel gives (`message.user_committed`, `task.posted` on the channel's
    /// conversation); an employee's answer streams with `StreamDelta::thread_parent`,
    /// only to connections whose `HelloParams::accepts` names `chat_threads`.
    pub const CHAT_THREADS: &str = "chat_threads";
    /// Reactions on messages (protocol 5, additive; AUR-CHAT-002): the write
    /// `message.react { message_id, emoji, on }` (board members, on a message of a
    /// channel or a ticket's discussion that is not closed; idempotent),
    /// `MessageView::reactions`, and the event `message.reacted` (aggregate: the
    /// channel's conversation), only to connections whose `HelloParams::accepts`
    /// names `chat_reactions`.
    pub const CHAT_REACTIONS: &str = "chat_reactions";
    /// `ticket.update { add_files }` (protocol 5, additive): files added to an
    /// existing ticket, as `files` on `ticket.create`. A main task that still
    /// works gets them too, as an update in its conversation that names them
    /// (with a run if it is Running), or through its brief if it has not opened
    /// its conversation yet. It gives `ticket.updated`, and the timeline says who
    /// added how many files, with their names only.
    pub const TICKET_ADD_FILES: &str = "ticket_add_files";
    /// A board member responsible for a ticket (protocol 5, additive; AUR-FA A1):
    /// the write `ticket.set_responsible`, `CreateTicket::responsible_user_id`,
    /// `TicketView::responsible_user_id`, `responsible_username` and
    /// `responsible_disabled`, and `ticket.updated` with `changed: ["responsible"]`.
    pub const TICKET_RESPONSIBLE: &str = "ticket_responsible";
    /// Following a ticket (protocol 5, additive; AUR-FA A2): the write
    /// `ticket.follow`, the read `ticket.followers`, and the event
    /// `ticket.follow_changed`, only to connections whose `HelloParams::accepts`
    /// names `ticket_follow`.
    pub const TICKET_FOLLOW: &str = "ticket_follow";
    /// A personal Inbox (protocol 5, additive; AUR-FA A3): `NotificationView`
    /// rows per person, with `ticket_id`, `ticket_number`, `conversation_id`,
    /// `message_id`, `reason`, `merged_count` and `needs_you`.
    pub const PERSONAL_INBOX: &str = "personal_inbox";
    /// My tickets (protocol 5, additive; AUR-FA A4): the read `ticket.my_tickets`.
    pub const MY_TICKETS: &str = "my_tickets";
    /// Links to a message (protocol 5, additive; AUR-FA A5a): the read
    /// `message.locate { message_id }` answers `MessageLocation`, for a message
    /// of a channel or a ticket's discussion only.
    pub const MESSAGE_LOCATE: &str = "message_locate";
    /// Editing and deleting one's own message (protocol 5, additive; AUR-FA A6):
    /// the writes `message.edit` and `message.delete`, the read `message.history`,
    /// `MessageView::edited_at_ms`, `deleted_at_ms` and `editable`, and the event
    /// `message.edited`, only to connections whose `HelloParams::accepts` names
    /// `message_edits`. A deletion gives the event `message.deleted` every client knows.
    pub const MESSAGE_EDITS: &str = "message_edits";
    /// Quick answers (protocol 5, additive; ticket #15): a board member's message to
    /// an employee who is in the middle of a run is also answered at once by a
    /// short, text-only run of its own, besides the busy line. No new method, field
    /// or event: the answer is an ordinary employee message in the discussion.
    pub const QUICK_ANSWERS: &str = "quick_answers";
    /// An employee has an access level (`EmployeeSummary::access_level`, and the
    /// field on `employee.create` and `employee.update`) that limits its runs' tools.
    pub const EMPLOYEE_ACCESS: &str = "employee_access";
    /// Run cards and interrupt cards in a discussion, and the odometer (protocol 5,
    /// additive): `MessageView::run` and `awaiting_user`, `ApprovalView::ticket_id` and
    /// `discussion_id`, `run.get_activity`, `usage.get_totals` with `ticket_id`,
    /// `TokenUsage::model_calls`, `UsageSnapshot::by_user`.
    pub const RUN_CARDS: &str = "run_cards";
    /// A board member's lasting chat with an employee: `employee.open_chat`, `employee.chats`
    /// and `chat.send` to such a chat. Gone (migration 45): no longer advertised in `FEATURES`;
    /// `employee.chats` answers an empty list and `employee.open_chat` is refused.
    pub const EMPLOYEE_CHATS: &str = "employee_chats";
    /// `schedule_message` on the bridge shows to board members: `scheduled_message.list`,
    /// `scheduled_message.cancel`, and who waits for a thread (`thread.waiting`).
    pub const SCHEDULED_MESSAGES: &str = "scheduled_messages";
    /// An employee has a tool list, what asks first and its own MCP servers
    /// (`EmployeeSummary::tool_policy`, `employee.tool_options`, C2).
    pub const EMPLOYEE_TOOLS: &str = "employee_tools";
    /// `employee.create` takes a start profile (`employee.start_profiles`, C6).
    pub const EMPLOYEE_PROFILES: &str = "employee_profiles";
    /// The model and effort of every employee can be read (`employee.run_overview`),
    /// and `connection.set_defaults` refuses an effort the model lacks.
    pub const EMPLOYEE_RUN_OVERVIEW: &str = "employee_run_overview";
    /// The activity log can be read (`activity_log.list`, R8).
    pub const ACTIVITY_LOG: &str = "activity_log";
    /// Who is busy and what waits for a slot can be read (`activity.overview`, ticket #50).
    pub const ACTIVITY_OVERVIEW: &str = "activity_overview";
    /// An employee's mailbox can be read (`employee.queue`, U6).
    pub const EMPLOYEE_QUEUE: &str = "employee_queue";
    /// An employee's scratchpad can be read (`employee.scratchpad`, `RunBrief::employee_id`).
    pub const EMPLOYEE_SCRATCHPAD: &str = "employee_scratchpad";
    /// The reads a workspace narrows take a `project_id` (`notification.list`,
    /// `approval.list`, `scheduled_message.list`, `activity_log.list`). Older servers
    /// refuse or ignore it, so a client without this feature does not offer workspaces.
    pub const WORKSPACE_FILTERS: &str = "workspace_filters";
    /// Links to Claude Design artifacts (protocol 5, additive): `ProjectSummary::styleguide_url`
    /// and `project.update { styleguide_url }`, `TicketView::design_url` and
    /// `ticket.update { design_url }`. `ticket.updated` says `changed: ["design"]`. A client
    /// without this feature does not offer the fields (a project or ticket update refuses them).
    pub const DESIGN_LINKS: &str = "design_links";
    /// Quick tasks (protocol 5, additive, ticket #53): `ticket.create { kind: "quick" }` with an
    /// owner starts at once in Doing, in Auto, without triage, Plan review or Testing, on the
    /// branch `quick/ticket-<N>`; `ticket.accept` merges that branch into the project's local main
    /// first (the card stays in Review until the merge worked). `TicketView::kind` and
    /// `merge_commit`. A client without this feature does not offer quick tasks.
    pub const QUICK_TASK: &str = "quick_task";
    /// The History (protocol 5, additive, ticket #57): the read `ticket.history` of the
    /// tickets on Done with their summary, kept on the ticket at Done; `ticket.updated`
    /// says `changed: ["summary"]` when it comes, fails or is made again. A client
    /// without this feature does not show the History.
    pub const TICKET_HISTORY: &str = "ticket_history";
    /// Work sessions in the History (protocol 5, additive): the read
    /// `work_session.history` of every board member's sessions on the day of their last
    /// activity, each with a short AI summary made once the session has been quiet a
    /// while (`conversation.settings_changed` says `changed: ["summary"]` when it comes
    /// or fails), and the write `work_session.retry_summary`. A client without this
    /// feature shows only tickets in the History.
    pub const SESSION_HISTORY: &str = "session_history";
    /// The Code tab (protocol 5, additive, ticket #77): the read `code.get` of a
    /// project's git state and the host writes `code.fetch`, `code.merge`,
    /// `code.remove_worktree`, `code.delete_branch` and `code.prune_merged`, each
    /// answering the view as it is after it. Never a push. A client without this
    /// feature does not show the Code tab.
    pub const CODE_VIEW: &str = "code_view";
    /// Work sessions (protocol 5, additive): `work_session.start` starts one employee in one
    /// project's folder without a ticket (optionally in a worktree of its own), later messages
    /// go in with `chat.send`, and `work_session.list` lists the reader's sessions. A client
    /// without this feature offers quick tasks instead.
    pub const WORK_SESSIONS: &str = "work_sessions";
    /// `chat.send` and `work_session.start` take `files` (additive): files of any type,
    /// copied into the conversation's work folder as with an employee message. A client
    /// without this feature sends only images in a chat or session.
    pub const CHAT_FILES: &str = "chat_files";
    /// The dispatcher chooses who takes a work session (protocol 5, additive):
    /// `work_session.dispatch` starts one without an employee, `work_session.assign` lets
    /// the board member choose while it chooses or after it chose nobody, and
    /// `WorkSessionView::dispatch` says where it is. A client without this feature
    /// asks for an employee when a session starts.
    pub const SESSION_DISPATCH: &str = "session_dispatch";
    /// Every step of a running subagent (protocol 5, additive): `AgentView::call_id`
    /// names its `Task`/`Agent` call and `ActivityView::parent_call_id` the subagent
    /// behind a call, so `run.get_activity` shows what one subagent did. A client
    /// without this feature shows only its latest step.
    pub const SUBAGENT_STEPS: &str = "subagent_steps";
    /// A board member chooses above how many tokens a session is compacted:
    /// `compaction.get_setting` / `compaction.set_threshold` (`CompactionSetting`).
    /// Claude Code compacting by itself during a turn shows too: `RunProgress::compacting`
    /// while it works and a `compacted` update afterwards. Older servers keep 120k.
    pub const COMPACTION_SETTING: &str = "compaction_setting";
    /// A board member's answer to a run's questions or decision on its plan comes with
    /// `MessageView::decision` (protocol 5, additive), so a client shows it as a card.
    /// Decisions noted earlier have only `text`; a client keeps showing those as before.
    pub const RUN_DECISIONS: &str = "run_decisions";
    /// The read `command.applied { idempotency_key }` (protocol 5, additive) says
    /// whether a write with that key committed (`CommandApplied`). A client asks it
    /// when a write it does not resend lost its answer to a dropped link or a
    /// timeout; without this feature it can only say the write may have arrived.
    pub const COMMAND_APPLIED: &str = "command_applied";
    /// A ticket's title made from its description (protocol 5, additive): `ticket.create`
    /// with an empty title and a description starts with the description's first line as
    /// its title, and a model replaces it with a short title shortly after
    /// (`ticket.updated { changed: ["title"] }`), unless someone renamed it meanwhile.
    /// Older servers refuse an empty title, so a client without this feature sends one.
    pub const TICKET_MADE_TITLE: &str = "ticket_made_title";
    /// A design written live (protocol 5, additive): while an employee's `Write` of an
    /// HTML file streams, live updates carry the content so far (`StreamDelta::draft`),
    /// but only to a connection whose hello accepts `artifact_drafts`. The whole call
    /// follows as ever in `activity`.
    pub const ARTIFACT_DRAFTS: &str = "artifact_drafts";
    /// A workspace's designs (protocol 5, additive): `design.list` gives the kept HTML
    /// pages of each conversation of a project. A client without this feature shows no
    /// Designs page.
    pub const DESIGNS: &str = "designs";
    /// Design sessions (protocol 5, additive): `work_session.start` and `work_session.dispatch`
    /// take `design`, the employee is told to make HTML pages, `work_session.list` and
    /// `design.list` say which sessions are designs. A client without this feature offers no
    /// Design in New session.
    pub const DESIGN_SESSIONS: &str = "design_sessions";
    /// A work session on a branch of its own in the project's folder (protocol 5,
    /// additive): `work_session.start` and `work_session.dispatch` take `own_branch`,
    /// and `WorkSessionView::own_branch` says a session works so. The daemon checks the
    /// branch out before each run, never while another run works in the folder or the
    /// folder has uncommitted changes. A client without this feature does not offer it.
    pub const SESSION_OWN_BRANCH: &str = "session_own_branch";
    /// A paired device belongs to the board member who paired it (protocol 5, additive):
    /// `device.list` and `device.revoke` for the session's own devices,
    /// `RemoteClientView::paired_by`, and `remote.get_status` shows only the reader's
    /// own devices and those of nobody. A client without this feature keeps its
    /// device list in the Remote access settings.
    pub const OWN_DEVICES: &str = "own_devices";
    /// Each board member's own rail (protocol 5, additive): the read `user.rail_pins`,
    /// the write `user.set_rail_pins` and the event `user.rail_pins_changed`. A client
    /// without it keeps the pins on the device.
    pub const RAIL_PINS: &str = "rail_pins";
    /// Side questions (protocol 5, additive), like Claude Code's `/btw`:
    /// `conversation.side_ask` and `conversation.side_cancel`, the answer streamed
    /// to the asking connection only as `ServerMessage::SideAnswer`. Nothing is
    /// stored. A client without this feature shows no side-question button.
    pub const SIDE_QUESTION: &str = "side_question";
}

/// Every feature this build has.
pub const FEATURES: &[&str] = &[
    feature::UPLOAD_FETCH,
    feature::PROJECT_DESCRIPTION,
    feature::IDEA_ATTACHMENTS,
    feature::RUN_USAGE,
    feature::RUN_TRACE,
    feature::TASK_LESSONS,
    feature::INSTRUCTION_PROPOSALS,
    feature::SESSION_COMPACTION,
    feature::ACTIVITY_PREVIEW,
    feature::QUICK_REPLIES,
    feature::COMPACT_STATUS,
    feature::MANAGEMENT_CHOOSES_MODE,
    feature::AUTO_ALLOW,
    feature::TICKET_DISCUSSION,
    feature::TICKET_CHANNELS,
    feature::VAULT,
    feature::GUIDELINES,
    feature::BOARD_TESTING,
    feature::SERVER_NAME,
    feature::TICKET_ADD_FILES,
    feature::CHAT_THREADS,
    feature::CHAT_REACTIONS,
    feature::MESSAGE_LOCATE,
    feature::MY_TICKETS,
    feature::MESSAGE_EDITS,
    feature::TICKET_RESPONSIBLE,
    feature::TICKET_FOLLOW,
    feature::PERSONAL_INBOX,
    feature::QUICK_ANSWERS,
    feature::TICKET_FROM_IDEA,
    feature::EMPLOYEE_ACCESS,
    feature::RUN_CARDS,
    feature::SCHEDULED_MESSAGES,
    feature::EMPLOYEE_SCRATCHPAD,
    feature::EMPLOYEE_TOOLS,
    feature::EMPLOYEE_PROFILES,
    feature::EMPLOYEE_RUN_OVERVIEW,
    feature::ACTIVITY_LOG,
    feature::EMPLOYEE_QUEUE,
    feature::WORKSPACE_FILTERS,
    feature::DESIGN_LINKS,
    feature::EVERYONE_MENTION,
    feature::ACTIVITY_OVERVIEW,
    feature::QUICK_TASK,
    feature::WORK_SESSIONS,
    feature::SESSION_DISPATCH,
    feature::TICKET_HISTORY,
    feature::SESSION_HISTORY,
    feature::CHAT_FILES,
    feature::SUBAGENT_STEPS,
    feature::COMPACTION_SETTING,
    feature::RUN_DECISIONS,
    feature::COMMAND_APPLIED,
    feature::TICKET_MADE_TITLE,
    feature::CODE_VIEW,
    feature::ARTIFACT_DRAFTS,
    feature::DESIGNS,
    feature::DESIGN_SESSIONS,
    feature::SESSION_OWN_BRANCH,
    feature::OWN_DEVICES,
    feature::RAIL_PINS,
    feature::SIDE_QUESTION,
];

/// The answer of `command.applied`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CommandApplied {
    /// True: a write with this key committed. False: none did (yet).
    pub applied: bool,
}

/// Daemon → client.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    Response {
        request_id: u64,
        #[serde(flatten)]
        outcome: Outcome,
    },
    /// A committed event (durable); replayed from the log after `event.subscribe`.
    Event(Event),
    /// Ephemeral streamed text; after a reconnect the client refetches instead.
    Stream(StreamDelta),
    /// Remote clients only: streamed text as an append.
    StreamAppend(crate::remote::StreamAppend),
    /// The machine's load or the admission changed. While runs wait, at most every 2 s.
    LoadChanged(LoadSnapshot),
    /// The pending shutdown changed.
    PowerChanged(PowerState),
    /// What `claude auth login` asks, only to the connection that started the
    /// link (`claude_login.link`); never an event, never logged.
    ClaudeLoginPrompt(ClaudeLoginPrompt),
    /// The session of this connection ended (`user.logout`, `session.revoke`, a
    /// new password, `user.disable`, expiry); the server closes the connection
    /// right after. The client shows "You were signed out on this device." and
    /// its sign-in screen (a logout of its own needs no message).
    SignedOut { reason: aurelius_domain::user::SignOutReason },
    /// The answer to a side question so far (`conversation.side_ask`), only to the
    /// connection that asked; never an event, never stored.
    SideAnswer(SideAnswer),
}

/// `ServerMessage::SideAnswer`: one side question's answer as it grows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SideAnswer {
    pub side_id: String,
    /// The whole answer so far (not a delta), through the vault redactor.
    pub text: String,
    /// The answer is complete, failed or was stopped: no more messages for this id.
    pub done: bool,
    /// Why there is no (whole) answer: a fixed text of Aurelius, never the
    /// model's or the CLI's output. Comes with `done`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// `ResponseBody::SideAsked`: the id the answer of a side question will carry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SideAsked {
    pub side_id: String,
}

/// `ServerMessage::ClaudeLoginPrompt`: the sign-in link to open and/or the
/// question for the code, as `claude` printed them.
#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ClaudeLoginPrompt {
    pub link_id: String,
    /// The Anthropic sign-in page to open (`https://claude.com/…/oauth/authorize?…`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Claude waits for the code: send it with `claude_login.input`.
    pub asks_code: bool,
    /// The link ended without a login: a fixed text of Aurelius (never Claude's
    /// output), shown with "Try again". A link that worked ends with the event
    /// `claude_login.updated` instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Hides the link (it carries the sign-in's state): never in a log (C6).
impl std::fmt::Debug for ClaudeLoginPrompt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClaudeLoginPrompt")
            .field("link_id", &self.link_id)
            .field("url", &self.url.as_ref().map(|_| "<redacted>"))
            .field("asks_code", &self.asks_code)
            .field("error", &self.error)
            .finish()
    }
}

#[cfg(test)]
mod claude_login_prompt_tests {
    #[test]
    fn the_sign_in_link_and_the_code_never_show_in_debug_output() {
        let prompt = super::ClaudeLoginPrompt { link_id: "l".into(), url: Some("https://claude.com/cai/oauth/authorize?state=SECRET-STATE".into()), asks_code: true, error: None };
        assert!(!format!("{prompt:?}").contains("SECRET-STATE"));
        let input = crate::ClaudeLoginInputParams { link_id: "l".into(), text: "SECRET-CODE".into() };
        assert!(!format!("{input:?}").contains("SECRET-CODE"));
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Ok(ResponseBody),
    Error(AppError),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum ResponseBody {
    Hello(HelloResult),
    Health(HealthResult),
    /// The answer to every write.
    Receipt(Receipt),
    Projects(ProjectsSnapshot),
    Jobs(JobsSnapshot),
    Messages(MessagesSnapshot),
    Employees(EmployeesSnapshot),
    Company(CompanySnapshot),
    Board(aurelius_domain::ticket::BoardView),
    Ticket(aurelius_domain::ticket::TicketDetail),
    Tickets(aurelius_domain::ticket::TicketsSnapshot),
    Users(aurelius_domain::user::UsersSnapshot),
    Channels(aurelius_domain::views::ChannelsSnapshot),
    EmployeeChats(aurelius_domain::views::EmployeeChatsSnapshot),
    /// `work_session.list`.
    WorkSessions(aurelius_domain::work_session::WorkSessionsSnapshot),
    ScheduledMessages(aurelius_domain::views::ScheduledMessagesSnapshot),
    ThreadWaiting(aurelius_domain::views::ThreadWaitingSnapshot),
    EmployeeScratchpad(aurelius_domain::views::ScratchpadView),
    /// `activity.overview`.
    ActivityOverview(aurelius_domain::views::ActivityOverview),
    /// `employee.queue`.
    EmployeeQueue(aurelius_domain::views::EmployeeQueue),
    /// `activity_log.list`.
    ActivityLog(aurelius_domain::views::ActivityLogPage),
    /// `employee.tool_options`.
    EmployeeToolOptions(aurelius_domain::employee_tools::ToolOptions),
    /// `employee.start_profiles`.
    EmployeeStartProfiles(aurelius_domain::employee_tools::StartProfiles),
    EmployeeRuns(aurelius_domain::views::EmployeeRunsView),
    /// `command.applied`.
    CommandApplied(CommandApplied),
    /// `discussion.audience`.
    DiscussionAudience(aurelius_domain::views::DiscussionAudience),
    /// `thread.read`.
    Thread(aurelius_domain::views::ThreadSnapshot),
    /// `ticket.followers`.
    TicketFollowers(aurelius_domain::ticket::TicketFollowersView),
    /// `ticket.my_tickets`.
    MyTickets(aurelius_domain::ticket::MyTicketsView),
    /// `user.rail_pins`.
    RailPins(aurelius_domain::user::RailPinsView),
    /// `ticket.history`.
    History(aurelius_domain::history::HistoryPage),
    /// `work_session.history`.
    SessionHistory(aurelius_domain::history::SessionHistoryPage),
    /// `code.get`.
    Code(aurelius_domain::code::CodeView),
    /// `message.locate`.
    MessageLocation(aurelius_domain::views::MessageLocation),
    /// `message.history`.
    MessageHistory(aurelius_domain::views::MessageHistoryView),
    WhoAmI(aurelius_domain::user::WhoAmI),
    /// `user.login`: the session token, once.
    Login(aurelius_domain::user::LoginResult),
    Sessions(aurelius_domain::user::SessionsSnapshot),
    ClaudeLogin(aurelius_domain::user::ClaudeLoginView),
    /// `conversation.side_ask` (the answer follows as `ServerMessage::SideAnswer`)
    /// and `conversation.side_cancel`.
    SideAsked(SideAsked),
    /// `vault.list`.
    Vault(aurelius_domain::vault::VaultList),
    /// `vault.reveal`: never printed.
    VaultValue(aurelius_domain::vault::VaultValue),
    /// `vault.audit`.
    VaultAudit(aurelius_domain::vault::VaultAuditPage),
    /// `vault.key_export`: never printed.
    VaultKey(aurelius_domain::vault::VaultKeyExport),
    /// `guideline.list`.
    Guidelines(aurelius_domain::guideline::GuidelinesSnapshot),
    /// `guideline.get`.
    Guideline(aurelius_domain::guideline::GuidelineDetail),
    /// `guideline.versions`.
    GuidelineVersions(aurelius_domain::guideline::GuidelineVersions),
    /// `guideline.effective`.
    EffectiveGuidelines(aurelius_domain::guideline::EffectiveGuidelines),
    /// `guideline.proposals`.
    GuidelineProposals(aurelius_domain::guideline::GuidelineProposals),
    /// `guideline.affected`.
    GuidelineAffected(aurelius_domain::guideline::GuidelineAffected),
    /// `guideline.starter_set`.
    StarterSet(aurelius_domain::guideline::StarterSet),
    /// `run.guidelines`.
    RunGuidelines(aurelius_domain::guideline::RunGuidelines),
    /// `server.get_name`.
    ServerName(aurelius_domain::server::ServerNameView),
    /// `design.list`.
    Designs(aurelius_domain::views::DesignsSnapshot),
    ProjectConversations(ProjectConversationsSnapshot),
    Approvals(ApprovalsSnapshot),
    Connections(ProvidersSnapshot),
    Tasks(TasksSnapshot),
    Timeline(TimelinePage),
    Memory(MemorySnapshot),
    MemoryContext(MemoryContextView),
    MarkdownFiles(MarkdownFileList),
    MarkdownFile(MarkdownFileContent),
    Usage(UsageSnapshot),
    UsageHours(UsageHours),
    UsageTotals(UsageTotals),
    RunTrace(RunTrace),
    RunActivity(RunActivity),
    TraceSetting(TraceSetting),
    Load(LoadSnapshot),
    Browser(BrowserSetting),
    ManagementModeSetting(ManagementModeSetting),
    AutoAllowSetting(AutoAllowSetting),
    CompactionSetting(CompactionSetting),
    McpServers(McpServers),
    McpLogin(McpLogin),
    McpAlwaysAllow(McpAlwaysAllow),
    Power(PowerState),
    Subscription(SubscriptionUsage),
    Update(UpdateStatus),
    Backups(BackupsSnapshot),
    Context(ContextView),
    /// A screenshot the agent took, as JPEG.
    Screenshot { id: String, jpeg_base64: String },
    /// A file kept with a message.
    File { id: String, name: String, media_type: String, data_base64: String },
    /// A file the user uploaded (a part of it, or its thumbnail).
    Upload(UploadChunk),
    Ideas(IdeasSnapshot),
    AllIdeas(aurelius_domain::views::AllIdeasSnapshot),
    Proposals(ProposalsSnapshot),
    EmployeeInstructionVersions(EmployeeInstructionVersionsSnapshot),
    TaskSchedules(TaskSchedulesSnapshot),
    Notifications(NotificationsSnapshot),
    Subscribed { replayed_through_seq: u64 },
    Remote(RemoteStatus),
    /// `device.list`.
    Devices(crate::remote::DevicesSnapshot),
    DirListing(DirListing),
    /// `None`: Claude Code was not found on the server.
    RuntimeDetected { claude_code: Option<String> },
    Subagents(Vec<AgentView>),
    /// `activity.get`: one tool call whole.
    Activity(ActivityView),
}

#[cfg(test)]
mod tests {
    use super::*;
    use aurelius_domain::project::CreateProject;

    fn write_request(key: CommandId) -> Request {
        Request::write(7, key, WriteMethod::Domain(aurelius_domain::Command::CreateProject(CreateProject { name: "Atlas".into(), description: None, folder: None })))
    }

    #[test]
    fn a_write_request_is_one_flat_object() {
        let key = CommandId::new();
        let request = write_request(key);
        let json = serde_json::to_value(&request).unwrap();
        assert_eq!((json["protocol"].as_u64(), json["request_id"].as_u64()), (Some(5), Some(7)));
        assert_eq!(json["idempotency_key"], serde_json::json!(key));
        assert_eq!(json["method"], "project.create");
        assert_eq!(json["params"]["name"], "Atlas");
        assert!(json.get("expected_version").is_none());
        assert_eq!(serde_json::from_value::<Request>(json).unwrap(), request);
    }

    #[test]
    fn reads_and_unit_methods_roundtrip() {
        for method in [Method::ServerGetHealth, Method::ProjectList, Method::EmployeeList, Method::CompanyGet, Method::LoadGet] {
            let request = Request::read(1, method);
            let text = serde_json::to_string(&request).unwrap();
            assert_eq!(serde_json::from_str::<Request>(&text).unwrap(), request, "{text}");
        }
        let json = serde_json::to_value(Request::read(1, Method::TaskList { project_id: None, employee_id: None, state: None })).unwrap();
        assert_eq!(json["method"], "task.list");
        assert_eq!(json["params"], serde_json::json!({}));
        // Every filter may be left out.
        let parsed: Request = serde_json::from_value(serde_json::json!({ "protocol": 3, "request_id": 2, "method": "task.list", "params": { "state": "needs_you" } })).unwrap();
        assert_eq!(parsed.call, Call::Read(Method::TaskList { project_id: None, employee_id: None, state: Some(crate::method::TaskListState::NeedsYou) }));
        // A unit method also takes an empty params object (or null).
        for name in ["employee.list", "proposal.list", "company.get"] {
            for params in [serde_json::json!({}), serde_json::Value::Null] {
                let parsed: Request = serde_json::from_value(serde_json::json!({ "protocol": 4, "request_id": 4, "method": name, "params": params })).unwrap();
                assert_eq!(serde_json::to_value(&parsed).unwrap()["method"], name);
            }
        }
        let err = serde_json::from_value::<Request>(serde_json::json!({ "protocol": 4, "request_id": 5, "method": "employee.list", "params": { "x": 1 } }));
        assert!(err.is_err(), "a unit method takes no params");
        let chat = serde_json::to_value(Request::read(3, Method::EmployeeChat { employee_id: aurelius_domain::EmployeeId::new() })).unwrap();
        assert_eq!(chat["method"], "employee.chat");
    }

    #[test]
    fn the_instruction_methods_roundtrip_under_their_names() {
        use aurelius_domain::{EmployeeId, ProposalId, proposal::{DecideProposal, RestoreEmployeeInstructions}};
        let employee_id = EmployeeId::new();
        let decide = DecideProposal { proposal_id: ProposalId::new(), approve: true, edits: None, note: None, instructions: Some("Work test-first.".into()) };
        let cases = [
            (Request::read(1, Method::EmployeeInstructionVersions { employee_id }), "employee.instruction_versions"),
            (Request::write(2, CommandId::new(), WriteMethod::Domain(aurelius_domain::Command::RestoreEmployeeInstructions(RestoreEmployeeInstructions { employee_id, version: 1 }))), "employee.restore_instructions"),
            (Request::write(3, CommandId::new(), WriteMethod::Domain(aurelius_domain::Command::DecideProposal(decide))), "proposal.decide"),
        ];
        for (request, name) in cases {
            let json = serde_json::to_value(&request).unwrap();
            assert_eq!(json["method"], name);
            assert_eq!(serde_json::from_value::<Request>(json).unwrap(), request, "{name}");
        }
        assert!(FEATURES.contains(&feature::INSTRUCTION_PROPOSALS) && FEATURES.contains(&feature::QUICK_REPLIES));
    }

    #[test]
    fn optional_params_may_be_left_out_or_null() {
        let parse = |method: &str, params: Option<serde_json::Value>| {
            let mut json = serde_json::json!({ "protocol": 3, "request_id": 2, "method": method });
            if let Some(params) = params {
                json["params"] = params;
            }
            serde_json::from_value::<Request>(json).map(|r| r.call)
        };
        let all_optional = [
            Method::EmployeeList,
            Method::TaskList { project_id: None, employee_id: None, state: None },
            Method::MemoryList { project_id: None, employee_id: None },
            Method::CompanyGet,
            Method::BackupList,
        ];
        for method in all_optional {
            let name = serde_json::to_value(&method).unwrap()["method"].as_str().unwrap().to_owned();
            for params in [None, Some(serde_json::Value::Null)] {
                assert_eq!(parse(&name, params.clone()).unwrap(), Call::Read(method.clone()), "{name} {params:?}");
            }
        }
        // A required field is still required, and named.
        let err = parse("employee.chat", None).unwrap_err().to_string();
        assert!(err.contains("employee_id"), "{err}");
    }

    #[test]
    fn an_unknown_method_is_a_clear_error() {
        let bad = serde_json::json!({ "protocol": 3, "request_id": 1, "method": "task.reviews", "params": {} });
        let err = serde_json::from_value::<Request>(bad).unwrap_err().to_string();
        assert!(err.contains("task.reviews"), "{err}");
    }

    #[test]
    fn an_unparsed_request_is_answered_for_its_own_id() {
        let error_of = |value: serde_json::Value| {
            let err = serde_json::from_value::<Request>(value.clone()).unwrap_err();
            match reject_unparsed(Some(&value), &err) {
                ServerMessage::Response { request_id, outcome: Outcome::Error(e) } => (request_id, e.code),
                other => panic!("{other:?}"),
            }
        };
        let unknown = serde_json::json!({ "protocol": 3, "request_id": 42, "method": "mcp.login", "params": {} });
        assert_eq!(error_of(unknown), (42, ErrorCode::UnknownMethod));
        // A known method with bad params is still invalid_request.
        let bad_params = serde_json::json!({ "protocol": 3, "request_id": 43, "method": "task.list", "params": { "project_id": 5 } });
        assert_eq!(error_of(bad_params), (43, ErrorCode::InvalidRequest));
        let bad_write = serde_json::json!({ "protocol": 3, "request_id": 44, "method": "project.create", "params": {} });
        assert_eq!(error_of(bad_write), (44, ErrorCode::InvalidRequest));
        let no_id = serde_json::json!({ "method": "task.list" });
        assert_eq!(error_of(no_id).0, 0);
        assert!(Method::knows("session.hello") && Method::knows("task.list") && !Method::knows("project.create"));
    }

    #[test]
    fn the_server_messages_use_snake_case_kinds() {
        let json = serde_json::to_value(ServerMessage::PowerChanged(PowerState { pending: None })).unwrap();
        assert_eq!(json["type"], "power_changed");
        let json = serde_json::to_value(ServerMessage::LoadChanged(LoadSnapshot {
            cpu_pct: 1.0,
            mem_free_mb: 1,
            mem_total_mb: 2,
            running: 0,
            held: 0,
            gate: LoadGate::Open,
            hold: None,
            limits: LoadLimits { max_runs: 1, cpu_high: 80, cpu_low: 70, mem_min_mb: 1, enabled: true },
            checks_active: true,
            sampled_at_ms: 1,
        }))
        .unwrap();
        assert_eq!(json["type"], "load_changed");
        assert_eq!(json["gate"], "open");
    }
}
