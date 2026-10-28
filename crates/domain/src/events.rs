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
    DepartmentCreated => "department.created",
    DepartmentUpdated => "department.updated",
    DepartmentArchived => "department.archived",
    // An employee moved to another department (or out of any); the aggregate is the employee.
    EmployeeDepartmentSet => "employee.department_set",
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
    MessageDeleted => "message.deleted",
    MessageSentNow => "message.sent_now",
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
    // Task variants (Harness² C); the aggregate is the task.
    TaskVariantsStarted => "task.variants_started",
    TaskVariantProbed => "task.variant_probed",
    TaskVariantsReady => "task.variants_ready",
    TaskVariantChosen => "task.variant_chosen",
    TaskVariantsStopped => "task.variants_stopped",
    TaskVariantsFailed => "task.variants_failed",
    // A later task found the same lesson again (Harness² A); the aggregate is the lesson.
    MemoryReinforced => "memory.reinforced",
    IdeaAdded => "idea.added",
    IdeaUpdated => "idea.updated",
    IdeaStatusChanged => "idea.status_changed",
    IdeaStarted => "idea.started",
    IdeaDeleted => "idea.deleted",
    ShareGranted => "share.granted",
    ShareRevoked => "share.revoked",
    AttachmentAdded => "attachment.added",
    AttachmentRemoved => "attachment.removed",
    HeartbeatChanged => "heartbeat.changed",
    HeartbeatReported => "heartbeat.reported",
    WakeRequested => "wake.requested",
    RecurringTaskChanged => "recurring_task.changed",
    AgentFunctionChanged => "agent_function.changed",
    ApiKeyChanged => "api_key.changed",
    // An MCP server's OAuth status changed (login, logout); `aggregate_id` is the server name.
    McpAuthChanged => "mcp.auth_changed",
    // A login of `mcp.login_start` ended (succeeded, failed, expired); `aggregate_id` is the login id.
    McpLoginFinished => "mcp.login_finished",
    // A run published (or republished) a Claude Code artifact; the aggregate is its conversation.
    ArtifactPublished => "artifact.published",
    // A note of a project was created, saved (`metadata.revision`), pinned or deleted; the aggregate is the note.
    NoteCreated => "note.created",
    NoteUpdated => "note.updated",
    NoteDeleted => "note.deleted",
    // A write of the host (MCP, plugins, load, backups, …): the audit record of the write pipeline.
    HostWrite => "host.write",
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
