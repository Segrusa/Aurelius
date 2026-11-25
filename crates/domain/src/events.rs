//! The kinds of events in the audit log. The log is append-only history for the
//! timeline and notifications; clients refresh on them. Every event the
//! application writes has one of these kinds.

use serde::{Deserialize, Serialize};

macro_rules! event_kinds {
    ($($variant:ident => $text:literal,)*) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
        pub enum EventKind {
            $(#[serde(rename = $text)] $variant,)*
        }

        impl EventKind {
            pub const ALL: &'static [EventKind] = &[$(EventKind::$variant,)*];

            pub fn as_str(self) -> &'static str {
                match self {
                    $(EventKind::$variant => $text,)*
                }
            }

            pub fn parse(text: &str) -> Option<Self> {
                match text {
                    $($text => Some(EventKind::$variant),)*
                    _ => None,
                }
            }
        }
    };
}

event_kinds! {
    ProjectCreated => "project.created",
    ProjectUpdated => "project.updated",
    ProjectPaused => "project.paused",
    ProjectArchived => "project.archived",
    ProjectFolderSet => "project.folder_set",
    // History only: before protocol 3 "Follow the org chart" was per project.
    ProjectFollowOrgChart => "project.follow_org_chart",
    // "Follow the org chart" (server setting) changed; `metadata.follow`.
    SettingsFollowOrgChart => "settings.follow_org_chart",
    // The one-time conversion of 2.x data into the company; `metadata.report` is a `ConversionReport`.
    CompanyConverted => "company.converted",
    EmployeeCreated => "employee.created",
    EmployeeConfigChanged => "employee.config_changed",
    EmployeePaused => "employee.paused",
    EmployeeResumed => "employee.resumed",
    EmployeeArchived => "employee.archived",
    EmployeeFolderSet => "employee.folder_set",
    EmployeeReportsToSet => "employee.reports_to_set",
    EmployeeProposed => "employee.proposed",
    ProposalDecided => "employee.proposal_decided",
    // An employee proposes new instructions for itself or a direct report (Harness² B); waits for the user.
    EmployeeInstructionsProposed => "employee.instructions_proposed",
    ConversationStarted => "conversation.started",
    ConversationCreated => "conversation.created",
    ConversationSettingsChanged => "conversation.settings_changed",
    ConversationDeleted => "conversation.deleted",
    MessageUserCommitted => "message.user_committed",
    MessageAssistantCommitted => "message.assistant_committed",
    MessageInterrupted => "message.interrupted",
    // A queued message was removed (`metadata.run_id`), or a board member deleted their
    // message of a channel (feature `message_edits`; the payload is the earlier text and
    // `metadata.queued` is false). Clients read the conversation again either way.
    MessageDeleted => "message.deleted",
    MessageSentNow => "message.sent_now",
    // A board member edited their message of a channel (feature `message_edits`); aggregate:
    // the channel's conversation; the payload is the earlier text, `metadata.message_id`
    // and `by`, never text. Only to connections that accept `message_edits`.
    MessageEdited => "message.edited",
    // A board member added or took back a reaction on a message of a channel
    // (feature `chat_reactions`); aggregate: the channel's conversation;
    // `metadata.message_id`, `emoji`, `on`. Only to connections that accept `chat_reactions`.
    MessageReacted => "message.reacted",
    RunQueued => "run.queued",
    RunStarted => "run.started",
    RunWaiting => "run.waiting",
    RunCheckpointed => "run.checkpointed",
    RunUsageReported => "run.usage_reported",
    RunCompleted => "run.completed",
    RunFailed => "run.failed",
    RunCancelRequested => "run.cancel_requested",
    RunCancelled => "run.cancelled",
    // A web page was fetched for an answer (web research); `metadata.error` says if it failed.
    RunSourceFetched => "run.source_fetched",
    ApprovalRequested => "approval.requested",
    ApprovalApproved => "approval.approved",
    ApprovalRejected => "approval.rejected",
    ApprovalExpired => "approval.expired",
    ApprovalInvalidated => "approval.invalidated",
    ApprovalConsumed => "approval.consumed",
    ApprovalAnswered => "approval.answered",
    ApprovalPlanApproved => "approval.plan_approved",
    ApprovalPlanRevisionRequested => "approval.plan_revision_requested",
    ApprovalPlanRejected => "approval.plan_rejected",
    ApprovalAllowedByMode => "approval.allowed_by_mode",
    // Approved because the user chose Always allow for its MCP tool on another ask.
    ApprovalAllowedAlways => "approval.allowed_always",
    ProviderChanged => "connection.changed",
    FolderChanged => "folder.changed",
    ConfigChanged => "config.changed",
    TaskCreated => "task.created",
    TaskQueued => "task.queued",
    TaskStarted => "task.started",
    TaskWaiting => "task.waiting",
    TaskResumed => "task.resumed",
    TaskSucceeded => "task.succeeded",
    TaskFailed => "task.failed",
    TaskCancelled => "task.cancelled",
    TaskAssigned => "task.assigned",
    TaskHandedOff => "task.handed_off",
    TaskDelegated => "task.delegated",
    TaskModeChosen => "task.mode_chosen",
    TaskPosted => "task.posted",
    TaskJobRequeued => "task.job_requeued",
    // A run in the conversation of a finished task was queued, started or ended: its
    // `TaskView::live_run` may have changed (the status did not). The aggregate is the task.
    TaskRunChanged => "task.run_changed",
    // The owner of a ticket's main task reported its work ready (`report_ready`); the run's end moves the ticket.
    TaskReportReady => "task.report_ready",
    MemoryRemembered => "memory.remembered",
    MemoryProposed => "memory.proposed",
    MemorySuperseded => "memory.superseded",
    MemoryRevised => "memory.revised",
    MemoryConsolidated => "memory.consolidated",
    MemoryConsolidationRequested => "memory.consolidation_requested",
    MemorySummarized => "memory.summarized",
    MemoryExtracted => "memory.extracted",
    MemoryRejected => "memory.rejected",
    MemoryPinned => "memory.pinned",
    MemoryForgotten => "memory.forgotten",
    MemoryAccepted => "memory.accepted",
    // A later task found the same lesson again (Harness² A); the aggregate is the lesson.
    MemoryReinforced => "memory.reinforced",
    IdeaAdded => "idea.added",
    IdeaUpdated => "idea.updated",
    IdeaStatusChanged => "idea.status_changed",
    IdeaStarted => "idea.started",
    IdeaTicketCreated => "idea.ticket_created",
    IdeaDeleted => "idea.deleted",
    AttachmentAdded => "attachment.added",
    AttachmentRemoved => "attachment.removed",
    WakeRequested => "wake.requested",
    RecurringTaskChanged => "recurring_task.changed",
    // An MCP server's OAuth status changed (login, logout); `aggregate_id` is the server name.
    McpAuthChanged => "mcp.auth_changed",
    // A login of `mcp.login_start` ended (succeeded, failed, expired); `aggregate_id` is the login id.
    McpLoginFinished => "mcp.login_finished",
    // A write of the host (MCP, load, backups, …): the audit record of the write pipeline.
    HostWrite => "host.write",
    // The board (protocol 4); the aggregate is the ticket. `metadata.by` is an `Actor`.
    TicketCreated => "ticket.created",
    TicketUpdated => "ticket.updated",
    // `metadata.from` / `metadata.to` (column kinds) and `metadata.by`.
    TicketMoved => "ticket.moved",
    TicketStarted => "ticket.started",
    // An owner was chosen (create with an owner, Start with an employee, Assign;
    // the only ways): `metadata.employee_id`, `metadata.by`, `metadata.reason`.
    TicketDispatched => "ticket.dispatched",
    // The main task handed in a plan (Plan mode); waits for a board member.
    TicketPlanReview => "ticket.plan_review",
    // A board member approved the plan (`metadata.decision`) or asked for a new one.
    TicketPlanApproved => "ticket.plan_approved",
    TicketPlanRevised => "ticket.plan_revised",
    // The main task reported; waits for a board member.
    TicketReview => "ticket.review",
    // The tester ran the full test suite of a card in Testing (feature `board_testing`):
    // `metadata.passed` (green: on to Review; red: back to Doing) and `metadata.task_id`
    // (the test task).
    TicketTested => "ticket.tested",
    TicketAccepted => "ticket.accepted",
    TicketSentBack => "ticket.sent_back",
    TicketCancelled => "ticket.cancelled",
    TicketArchived => "ticket.archived",
    // A board member started or stopped following a ticket (feature `ticket_follow`):
    // `metadata.user_id`, `on`, `reason`. Only to connections that accept `ticket_follow`.
    TicketFollowChanged => "ticket.follow_changed",
    // Names or order of the columns; the aggregate is the company.
    BoardColumnsUpdated => "board.columns_updated",
    // Name, dispatcher or tester of the company.
    CompanyUpdated => "company.updated",
    // A new board member: `metadata.created_user_id`, `metadata.username`. As in every
    // event, `metadata.user_id` is the board member who made it (none in the
    // onboarding), also as `metadata.by`. Never a password or hash.
    UserCreated => "user.created",
    // A board member was disabled (`metadata.disabled_user_id`, `metadata.by`): their
    // sessions are gone and they cannot sign in. Sessions themselves have no events.
    UserDisabled => "user.disabled",
    // A board member's Claude login was connected, expired or disconnected, or became
    // the company's background login; the aggregate is the user. Never a token.
    ClaudeLoginUpdated => "claude_login.updated",
    // A vault item (feature `vault`); the aggregate is the item, `project_id` its
    // project (none: the company). `metadata` is a `vault::VaultEventMeta`: id,
    // name, kind, project, version, by, changed fields. Never a value.
    VaultItemCreated => "vault.item_created",
    VaultItemUpdated => "vault.item_updated",
    VaultItemArchived => "vault.item_archived",
    // Guidelines (feature `guidelines`); the aggregate is the guideline. `metadata` is a
    // `guideline::GuidelineEventMeta`: id, title, version, by (and the target of a link).
    GuidelineCreated => "guideline.created",
    GuidelineUpdated => "guideline.updated",
    GuidelineArchived => "guideline.archived",
    GuidelineRestored => "guideline.restored",
    GuidelineLinked => "guideline.linked",
    GuidelineUnlinked => "guideline.unlinked",
    // An employee proposed a guideline or a change (`propose_guideline`, B5); the
    // aggregate is the proposal, `metadata` a `guideline::GuidelineProposedMeta`.
    GuidelineProposed => "guideline.proposed",
    // A board member decided; `metadata` a `guideline::GuidelineProposalDecidedMeta`.
    GuidelineProposalDecided => "guideline.proposal_decided",
    // The server's name changed (feature `server_name`); the aggregate is the server
    // (`server`). `metadata`: `by`, `before` and `after` (the shown names) and `chosen`
    // (none: the computer's name). Clients read `server.get_name` again.
    ServerRenamed => "server.renamed",
    // A board member pinned, unpinned or reordered the workspaces on their rail
    // (feature `rail_pins`); the aggregate is the user, so `aggregate_id` is their id.
    // `metadata.user_id` and `project_ids`. A client whose board member it is reads
    // `user.rail_pins` again; the others ignore it.
    UserRailPinsChanged => "user.rail_pins_changed",
    // A branch's review changed (feature `code_review`): a thread or reply was added, a
    // thread resolved or opened, a file marked viewed, or a review finished. The aggregate
    // is `<project_id>:<branch>`; `metadata.project_id`, `branch` and `user_id` (who did it).
    // A client showing that branch reads `code.review` again.
    CodeReviewChanged => "code.review_changed",
}

impl EventKind {
    /// The part before the dot: what the event is about (`task`, `employee`, …).
    pub fn area(self) -> &'static str {
        self.as_str().split('.').next().unwrap_or("")
    }
}

/// An event after commit: what subscribers get to know.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct EventNotice {
    pub seq: u64,
    pub kind: EventKind,
    pub aggregate_id: String,
    pub project_id: Option<crate::ProjectId>,
    pub occurred_at_ms: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_round_trip_through_text_and_json() {
        for kind in EventKind::ALL {
            assert_eq!(EventKind::parse(kind.as_str()), Some(*kind));
            assert_eq!(serde_json::to_value(kind).unwrap(), kind.as_str());
        }
        assert_eq!(EventKind::TaskCreated.area(), "task");
        assert_eq!(EventKind::parse("task.reviewed"), None);
    }
}
