//! Command envelope, receipt and result: the one write pipeline.

use crate::{
    ApprovalId, AttachmentId, Command, CommandId, ConversationId, EmployeeId, MemoryId, MessageId,
    ProjectId, ProviderId, RunId, TaskId, approval::ApprovalStatus, employee::LifecycleStatus,
};
use serde::{Deserialize, Serialize};

/// `command_id` makes retries idempotent. The daemon, not the client,
/// fixes principal/client/origin.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CommandEnvelope {
    pub command_id: CommandId,
    #[serde(default)]
    pub expected_version: Option<u64>,
    pub command: Command,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CommandReceipt {
    pub command_id: CommandId,
    pub committed_seq: u64,
    /// True when this receipt was returned from an earlier identical command.
    pub replayed: bool,
    pub result: CommandResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CommandResult {
    ProjectCreated {
        project_id: ProjectId,
        version: u64,
    },
    ProjectRenamed {
        project_id: ProjectId,
        version: u64,
    },
    ProjectDescriptionSet {
        project_id: ProjectId,
        version: u64,
    },
    ChatMessageSent {
        conversation_id: ConversationId,
        message_id: MessageId,
        run_id: RunId,
    },
    ReplyRetried {
        run_id: RunId,
    },
    StopRequested {
        run_id: RunId,
        already_finished: bool,
    },
    EmployeeCreated {
        employee_id: EmployeeId,
        conversation_id: ConversationId,
    },
    ScheduledMessageCancelled {
        message_id: crate::ScheduledMessageId,
    },
    EmployeeChatOpened {
        conversation_id: ConversationId,
    },
    EmployeeUpdated {
        employee_id: EmployeeId,
        version: u64,
    },
    EmployeeStatusChanged {
        employee_id: EmployeeId,
        status: LifecycleStatus,
    },
    ProjectStatusChanged {
        project_id: ProjectId,
        status: LifecycleStatus,
    },
    EmployeeMessageSent {
        conversation_id: ConversationId,
        message_id: MessageId,
        run_id: RunId,
    },
    /// A message in the group chat, with the employees it woke (each answers in a run of its own).
    GroupMessageSent {
        conversation_id: ConversationId,
        message_id: MessageId,
        to: Vec<EmployeeId>,
        run_ids: Vec<RunId>,
    },
    ApprovalDecided {
        approval_id: ApprovalId,
        status: ApprovalStatus,
        ok: bool,
        output: String,
    },
    #[serde(rename = "connection_added")]
    ProviderAdded {
        connection_id: ProviderId,
    },
    #[serde(rename = "connection_removed")]
    ProviderRemoved {
        connection_id: ProviderId,
    },
    VerificationStarted {
        connection_id: ProviderId,
    },
    RunDefaultsSet,
    TaskCreated {
        task_id: TaskId,
    },
    TaskCancelled {
        task_id: TaskId,
    },
    TaskAssigned {
        task_id: TaskId,
    },
    /// Also the answer to update, pause and resume.
    TaskScheduleCreated {
        schedule_id: crate::TaskScheduleId,
    },
    TaskScheduleUpdated {
        schedule_id: crate::TaskScheduleId,
    },
    TaskScheduleDeleted {
        schedule_id: crate::TaskScheduleId,
    },
    TaskScheduleRan {
        schedule_id: crate::TaskScheduleId,
        task_id: TaskId,
    },
    IdeaAdded {
        idea_id: crate::IdeaId,
    },
    IdeaUpdated {
        idea_id: crate::IdeaId,
    },
    IdeaStarted {
        idea_id: crate::IdeaId,
        task_id: TaskId,
    },
    IdeaDeleted {
        idea_id: crate::IdeaId,
    },
    AttachmentAdded {
        attachment_id: AttachmentId,
        conversation_id: ConversationId,
    },
    AttachmentRemoved {
        attachment_id: AttachmentId,
    },
    MemoryStored {
        memory_id: MemoryId,
    },
    MemoryDecided {
        memory_id: MemoryId,
        status: crate::memory::MemoryStatus,
    },
    MemoryForgotten {
        memory_id: MemoryId,
    },
    /// `queued` is false when a consolidation of that memory is already waiting or running.
    MemoryConsolidationQueued {
        queued: bool,
    },
    AgentModeSet {
        conversation_id: ConversationId,
        mode: crate::autonomy::AgentMode,
    },
    ConversationRunSet {
        conversation_id: ConversationId,
    },
    BrowserAllowedSet {
        conversation_id: ConversationId,
        allowed: bool,
    },
    ScreenAllowedSet {
        conversation_id: ConversationId,
        allowed: bool,
    },
    ProjectFolderSet {
        project_id: ProjectId,
        folder: String,
    },
    EmployeeFolderSet {
        employee_id: EmployeeId,
        folder: Option<String>,
    },
    ConversationDeleted {
        conversation_id: ConversationId,
    },
    /// A new, empty project conversation.
    ProjectConversationStarted {
        project_id: ProjectId,
        conversation_id: ConversationId,
    },
    /// A work session was made and its first message is being answered.
    WorkSessionStarted {
        project_id: ProjectId,
        conversation_id: ConversationId,
        message_id: MessageId,
        run_id: RunId,
    },
    /// A work session's summary for the History was asked for (`work_session.retry_summary`).
    SessionSummaryAsked {
        conversation_id: ConversationId,
    },
    /// A work session was made; the dispatcher chooses who answers its first message.
    WorkSessionDispatching {
        project_id: ProjectId,
        conversation_id: ConversationId,
        message_id: MessageId,
    },
    /// A message in a project conversation was accepted and is being answered.
    ProjectMessageSent {
        project_id: ProjectId,
        conversation_id: ConversationId,
        message_id: MessageId,
        run_id: RunId,
    },
    /// A waiting message was removed and its queued run cancelled.
    QueuedMessageDeleted {
        conversation_id: ConversationId,
        message_id: MessageId,
        run_id: RunId,
    },
    /// A waiting message's run starts now; the answers ahead of it were stopped.
    QueuedMessageSentNow {
        conversation_id: ConversationId,
        message_id: MessageId,
        run_id: RunId,
    },
    EmployeeReportsToSet {
        employee_id: EmployeeId,
        reports_to: Option<EmployeeId>,
    },
    FollowOrgChartSet {
        follow: bool,
    },
    /// `employee_id` is the employee that was created when the proposal was approved.
    ProposalDecided {
        proposal_id: crate::ProposalId,
        status: crate::proposal::ProposalStatus,
        employee_id: Option<EmployeeId>,
    },
    /// An earlier version of an employee's instructions was put back as `version` (the new current one).
    EmployeeInstructionsRestored {
        employee_id: EmployeeId,
        version: u32,
    },
    TicketCreated {
        ticket_id: crate::TicketId,
        number: u32,
    },
    /// The answer to every other ticket write: where the ticket is now.
    TicketChanged {
        ticket_id: crate::TicketId,
        column: crate::ticket::ColumnKind,
        version: u64,
    },
    /// A message in a ticket's discussion, with the employees it woke (or the
    /// owner it went to: `run_ids` is empty when the main task already runs and takes it).
    TicketMessagePosted {
        ticket_id: crate::TicketId,
        conversation_id: ConversationId,
        message_id: MessageId,
        to: Vec<EmployeeId>,
        run_ids: Vec<RunId>,
    },
    /// The board's new version (`ticket::board_version`).
    BoardColumnsUpdated {
        version: u64,
    },
    CompanyUpdated {
        version: u64,
    },
    ProjectUpdated {
        project_id: ProjectId,
        version: u64,
    },
    /// A new board member (`user.create`).
    UserCreated {
        user_id: crate::UserId,
    },
    /// Every guideline write but a proposal's decision and the starter set: the
    /// guideline and its current version now.
    GuidelineChanged {
        guideline_id: crate::GuidelineId,
        version: u32,
    },
    /// `guideline.decide_proposal`; `guideline_id`: the guideline that got the
    /// new version (or was made), none for a rejection.
    GuidelineProposalDecided {
        proposal_id: crate::GuidelineProposalId,
        status: crate::proposal::ProposalStatus,
        guideline_id: Option<crate::GuidelineId>,
    },
    /// `guideline.add_starter_set`: the guidelines made (keys already in the library are skipped).
    StarterGuidelinesAdded {
        guideline_ids: Vec<crate::GuidelineId>,
    },
    /// `server.set_name`: the name clients show now and its version (unchanged
    /// when the name was the same).
    ServerNameSet {
        name: String,
        version: u64,
    },
    /// `message.react` (feature `chat_reactions`): the reaction is as asked now.
    /// `changed` is false when it already was (on twice, off when not there).
    MessageReacted {
        conversation_id: ConversationId,
        message_id: MessageId,
        emoji: String,
        on: bool,
        changed: bool,
    },
    /// `ticket.follow` (feature `ticket_follow`): the reader follows it as asked
    /// now. `changed` is false when they already did (or did not).
    TicketFollowChanged {
        ticket_id: crate::TicketId,
        on: bool,
        changed: bool,
    },
    /// `message.edit` (feature `message_edits`).
    MessageEdited {
        conversation_id: ConversationId,
        message_id: MessageId,
        edited_at_ms: i64,
    },
    /// `message.delete` (feature `message_edits`).
    MessageDeleted {
        conversation_id: ConversationId,
        message_id: MessageId,
        deleted_at_ms: i64,
    },
}
