//! Pure domain model for Aurelius.
//!
//! No IO: serde and primitive types only. It validates commands, holds the
//! rules (policy intersection, the org chart, heartbeat schedule) and the
//! read models that storage returns and the protocol sends. Persistence,
//! transport and UI live elsewhere.

pub mod activity;
pub mod agent_function;
pub mod app_key;
pub mod approval;
pub mod autonomy;
pub mod command;
pub mod company;
pub mod connection;
pub mod conversation;
pub mod error;
pub mod events;
pub mod files;
pub mod group_chat;
pub mod idea;
pub mod ids;
pub mod job;
pub mod employee;
pub mod memory;
pub mod org;
pub mod project;
pub mod project_conversation;
pub mod proposal;
pub mod remote;
pub mod schedule;
pub mod scheduler;
pub mod share;
pub mod task;
pub mod ticket;
pub mod user;
pub mod variants;
pub mod views;
pub mod webhook;

pub use command::{CommandEnvelope, CommandReceipt, CommandResult};
pub use error::{AppError, ErrorCode};
pub use events::{EventKind, EventNotice};
pub use ids::*;
pub use project::ProjectStatus;

/// UUID parse errors for callers that parse IDs from payloads.
pub mod uuid_error {
    pub use uuid::Error;
}

use serde::{Deserialize, Serialize};

/// Who caused an event. Mirrors `events.actor_kind` in the schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ActorKind {
    User,
    Employee,
    System,
    Runtime,
}

impl ActorKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ActorKind::User => "user",
            ActorKind::Employee => "employee",
            ActorKind::System => "system",
            ActorKind::Runtime => "runtime",
        }
    }
}

/// Every business mutation of the domain enters the system as one of these
/// commands. `Command::method` gives its protocol method name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Command {
    CreateProject(project::CreateProject),
    RenameProject(project::RenameProject),
    SetProjectDescription(project::SetProjectDescription),
    SetProjectStatus(employee::SetProjectStatus),
    SetProjectFolder(project::SetProjectFolder),
    SendChatMessage(conversation::SendChatMessage),
    RetryReply(conversation::RetryReply),
    StopRun(conversation::StopRun),
    DeleteConversation(conversation::DeleteConversation),
    DeleteQueuedMessage(conversation::DeleteQueuedMessage),
    SendQueuedNow(conversation::SendQueuedNow),
    SetAgentMode(autonomy::SetAgentMode),
    SetConversationRun(autonomy::SetConversationRun),
    SetBrowserAllowed(autonomy::SetBrowserAllowed),
    SetScreenAllowed(autonomy::SetScreenAllowed),
    AttachFile(files::AttachFile),
    RemoveAttachment(files::RemoveAttachment),
    StartProjectConversation(project_conversation::StartProjectConversation),
    SendProjectMessage(project_conversation::SendProjectMessage),
    CreateEmployee(employee::CreateEmployee),
    UpdateEmployee(employee::UpdateEmployee),
    SetEmployeeStatus(employee::SetEmployeeStatus),
    SetEmployeeFolder(employee::SetEmployeeFolder),
    SetEmployeeReportsTo(employee::SetEmployeeReportsTo),
    SetFollowOrgChart(employee::SetFollowOrgChart),
    SetHeartbeat(employee::SetHeartbeat),
    SendEmployeeMessage(employee::SendEmployeeMessage),
    SendGroupMessage(group_chat::SendGroupMessage),
    DecideProposal(proposal::DecideProposal),
    RestoreEmployeeInstructions(proposal::RestoreEmployeeInstructions),
    DecideApproval(approval::DecideApproval),
    AnswerQuestion(approval::AnswerQuestion),
    DecidePlan(approval::DecidePlan),
    #[serde(rename = "add_connection")]
    AddProvider(connection::AddProvider),
    #[serde(rename = "remove_connection")]
    RemoveProvider(connection::RemoveProvider),
    #[serde(rename = "verify_connection")]
    VerifyProvider(connection::VerifyProvider),
    SetRunDefaults(connection::SetRunDefaults),
    CreateTask(task::CreateTask),
    CancelTask(task::CancelTask),
    ChooseTaskVariant(variants::ChooseTaskVariant),
    AssignTask(task::AssignTask),
    DiscussTask(share::DiscussTask),
    AddIdea(idea::AddIdea),
    UpdateIdea(idea::UpdateIdea),
    SetIdeaStatus(idea::SetIdeaStatus),
    StartIdea(idea::StartIdea),
    DeleteIdea(idea::DeleteIdea),
    Remember(memory::Remember),
    DecideMemory(memory::DecideMemory),
    ReviseMemory(memory::ReviseMemory),
    ForgetMemory(memory::ForgetMemory),
    PinMemory(memory::PinMemory),
    ConsolidateMemory(memory::ConsolidateMemory),
    ShareMessage(share::ShareMessage),
    RevokeShare(share::RevokeShare),
    CreateTaskSchedule(schedule::CreateTaskSchedule),
    UpdateTaskSchedule(schedule::UpdateTaskSchedule),
    DeleteTaskSchedule(schedule::DeleteTaskSchedule),
    SetTaskSchedulePaused(schedule::SetTaskSchedulePaused),
    RunTaskScheduleNow(schedule::RunTaskScheduleNow),
    CreateTicket(ticket::CreateTicket),
    UpdateTicket(ticket::UpdateTicket),
    MoveTicket(ticket::MoveTicket),
    StartTicket(ticket::StartTicket),
    AssignTicket(ticket::AssignTicket),
    ApprovePlan(ticket::ApprovePlan),
    RevisePlan(ticket::RevisePlan),
    AcceptTicket(ticket::AcceptTicket),
    SendBackTicket(ticket::SendBackTicket),
    CancelTicket(ticket::CancelTicket),
    ArchiveTicket(ticket::ArchiveTicket),
    PostTicketMessage(ticket::PostTicketMessage),
    RetryTicketSummary(ticket::RetryTicketSummary),
    UpdateBoardColumns(ticket::UpdateBoardColumns),
    UpdateCompany(company::UpdateCompany),
    UpdateProject(company::UpdateProject),
}

/// `(type, method)` for every command: the tag in the JSON of a `Command` and
/// the `<area>.<verb>` name it has on the wire. One table, checked by a test
/// against the JSON schema of `Command`.
pub const COMMAND_METHODS: &[(&str, &str)] = &[
    ("create_project", "project.create"),
    ("rename_project", "project.rename"),
    ("set_project_description", "project.set_description"),
    ("set_project_status", "project.set_status"),
    ("set_project_folder", "project.set_folder"),
    ("send_chat_message", "chat.send"),
    ("retry_reply", "chat.retry"),
    ("stop_run", "run.stop"),
    ("delete_conversation", "conversation.delete"),
    ("delete_queued_message", "conversation.delete_queued"),
    ("send_queued_now", "conversation.send_queued_now"),
    ("set_agent_mode", "conversation.set_agent_mode"),
    ("set_conversation_run", "conversation.set_run"),
    ("set_browser_allowed", "conversation.set_browser_allowed"),
    ("set_screen_allowed", "conversation.set_screen_allowed"),
    ("attach_file", "attachment.add"),
    ("remove_attachment", "attachment.remove"),
    ("start_project_conversation", "project_conversation.start"),
    ("send_project_message", "project_conversation.send"),
    ("create_employee", "employee.create"),
    ("update_employee", "employee.update"),
    ("set_employee_status", "employee.set_status"),
    ("set_employee_folder", "employee.set_folder"),
    ("set_employee_reports_to", "employee.set_reports_to"),
    ("set_follow_org_chart", "company.set_follow_org_chart"),
    ("set_heartbeat", "heartbeat.set"),
    ("send_employee_message", "employee.send_message"),
    ("send_group_message", "group_chat.send"),
    ("decide_proposal", "proposal.decide"),
    ("restore_employee_instructions", "employee.restore_instructions"),
    ("decide_approval", "approval.decide"),
    ("answer_question", "approval.answer"),
    ("decide_plan", "approval.decide_plan"),
    ("add_connection", "connection.add"),
    ("remove_connection", "connection.remove"),
    ("verify_connection", "connection.verify"),
    ("set_run_defaults", "connection.set_defaults"),
    ("create_task", "task.create"),
    ("cancel_task", "task.cancel"),
    ("choose_task_variant", "task.choose_variant"),
    ("assign_task", "task.assign"),
    ("discuss_task", "task.discuss"),
    ("add_idea", "idea.add"),
    ("update_idea", "idea.update"),
    ("set_idea_status", "idea.set_status"),
    ("start_idea", "idea.start"),
    ("delete_idea", "idea.delete"),
    ("remember", "memory.remember"),
    ("decide_memory", "memory.decide"),
    ("revise_memory", "memory.revise"),
    ("forget_memory", "memory.forget"),
    ("pin_memory", "memory.pin"),
    ("consolidate_memory", "memory.consolidate"),
    ("share_message", "share.create"),
    ("revoke_share", "share.revoke"),
    ("create_task_schedule", "recurring_task.create"),
    ("update_task_schedule", "recurring_task.update"),
    ("delete_task_schedule", "recurring_task.delete"),
    ("set_task_schedule_paused", "recurring_task.set_paused"),
    ("run_task_schedule_now", "recurring_task.run_now"),
    ("create_ticket", "ticket.create"),
    ("update_ticket", "ticket.update"),
    ("move_ticket", "ticket.move"),
    ("start_ticket", "ticket.start"),
    ("assign_ticket", "ticket.assign"),
    ("approve_plan", "ticket.approve_plan"),
    ("revise_plan", "ticket.revise_plan"),
    ("accept_ticket", "ticket.accept"),
    ("send_back_ticket", "ticket.send_back"),
    ("cancel_ticket", "ticket.cancel"),
    ("archive_ticket", "ticket.archive"),
    ("post_ticket_message", "ticket.post"),
    ("retry_ticket_summary", "ticket.retry_summary"),
    ("update_board_columns", "board.columns_update"),
    ("update_company", "company.update"),
    ("update_project", "project.update"),
];

impl Command {
    /// The tag of this command in JSON (`create_project`, …); used in audit records.
    pub fn kind(&self) -> String {
        match serde_json::to_value(self) {
            Ok(serde_json::Value::Object(map)) => map.get("type").and_then(|t| t.as_str()).unwrap_or("").to_owned(),
            _ => String::new(),
        }
    }

    /// The `<area>.<verb>` method name of this command.
    pub fn method(&self) -> &'static str {
        let kind = self.kind();
        COMMAND_METHODS.iter().find(|(t, _)| *t == kind).map(|(_, m)| *m).unwrap_or("")
    }
}

/// Validation failures carry a user-facing meaning (definition of done).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
pub enum DomainError {
    #[error("{field} is required")]
    Required { field: String },
    #[error("{field} is too long (max {max} characters)")]
    TooLong { field: String, max: usize },
    #[error("{field} is out of range: {detail}")]
    OutOfRange { field: String, detail: String },
    #[error("project {0} does not exist")]
    ProjectNotFound(ProjectId),
    #[error("conversation {0} does not exist")]
    ConversationNotFound(ConversationId),
    #[error("message {0} does not exist in this conversation")]
    MessageNotFound(MessageId),
    #[error("this message is not waiting for an answer")]
    MessageNotQueued,
    #[error("the answer to this message has already started; stop it instead")]
    MessageAlreadyStarted,
    #[error("employee {0} does not exist")]
    EmployeeNotFound(EmployeeId),
    #[error("this employee is paused; resume it first to start new work")]
    EmployeePaused,
    #[error("this employee is archived; its history is kept but it cannot act")]
    EmployeeArchived,
    #[error("approval {0} does not exist")]
    ApprovalNotFound(ApprovalId),
    #[error("this request was changed or has already been handled")]
    ApprovalNotPending,
    #[error("this request has expired; expiry never counts as permission")]
    ApprovalExpired,
    #[error("the action or policy changed after the request; a new approval is needed")]
    ApprovalInvalidated,
    #[error("connection {0} does not exist")]
    ProviderNotFound(ProviderId),
    #[error("{0}")]
    File(String),
    #[error("task {0} does not exist")]
    TaskNotFound(TaskId),
    #[error("this dependency would create a cycle")]
    DependencyCycle,
    #[error("run {0} does not exist")]
    RunNotFound(RunId),
    #[error("project is archived and can no longer be changed")]
    ProjectArchived,
    #[error("memory {0} does not exist")]
    MemoryNotFound(MemoryId),
    #[error("idea {0} does not exist")]
    IdeaNotFound(IdeaId),
    #[error("recurring task {0} does not exist")]
    TaskScheduleNotFound(TaskScheduleId),
    #[error("agent function {0} does not exist")]
    AgentFunctionNotFound(AgentFunctionId),
    #[error("API key {0} does not exist")]
    ApiKeyNotFound(ApiKeyId),
    #[error("app key {0} does not exist")]
    AppKeyNotFound(AppKeyId),
    #[error("{0}")]
    Memory(String),
    #[error("proposal {0} does not exist")]
    ProposalNotFound(ProposalId),
    #[error("this would make the employees report to each other in a circle")]
    OrgCycle,
    #[error("{0}")]
    Org(String),
    #[error("expected version {expected}, but the current version is {actual}")]
    VersionConflict { expected: u64, actual: u64 },
    #[error("ticket {0} does not exist")]
    TicketNotFound(TicketId),
    #[error("column {0} is not on the board")]
    BoardColumnNotFound(BoardColumnId),
    /// A workflow rule of the board refused it; the text says what to do instead.
    #[error("{0}")]
    Ticket(String),
    #[error("choose who you are first: this needs an active board member")]
    BoardMemberRequired,
    #[error("{0}")]
    InvalidUsername(String),
    #[error("{0}")]
    WeakPassword(String),
    #[error("another board member already has this name")]
    UsernameTaken,
}

pub(crate) fn require_text(field: &str, value: &str, max: usize) -> Result<String, DomainError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(DomainError::Required { field: field.into() });
    }
    if trimmed.chars().count() > max {
        return Err(DomainError::TooLong { field: field.into(), max });
    }
    Ok(trimmed.to_owned())
}

pub(crate) fn optional_text(field: &str, value: Option<&str>, max: usize) -> Result<Option<String>, DomainError> {
    match value.map(str::trim) {
        None | Some("") => Ok(None),
        Some(text) if text.chars().count() > max => Err(DomainError::TooLong { field: field.into(), max }),
        Some(text) => Ok(Some(text.to_owned())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// Every command has exactly one wire name, and the names are `<area>.<verb>`.
    #[test]
    fn every_command_has_a_method_name() {
        let schema = serde_json::to_value(schemars::schema_for!(Command)).unwrap();
        let variants = schema["oneOf"].as_array().expect("Command is a tagged enum");
        let types: BTreeSet<&str> = variants.iter().filter_map(|v| v["properties"]["type"]["const"].as_str()).collect();
        assert_eq!(types.len(), variants.len(), "every variant carries a type tag");
        let table: BTreeSet<&str> = COMMAND_METHODS.iter().map(|(t, _)| *t).collect();
        assert_eq!(types, table, "COMMAND_METHODS and Command list the same commands");
        let methods: BTreeSet<&str> = COMMAND_METHODS.iter().map(|(_, m)| *m).collect();
        assert_eq!(methods.len(), COMMAND_METHODS.len(), "method names are unique");
        for method in methods {
            let (area, verb) = method.split_once('.').expect("area.verb");
            assert!(!area.is_empty() && !verb.is_empty() && !verb.contains('.'), "{method}");
        }
    }
}
