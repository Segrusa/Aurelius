//! Remote access: what a paired device may do, and the
//! read models Settings → Remote access shows.
//!
//! The gateway speaks the same `Request`/`ServerMessage` JSON as local IPC, one
//! message per WebSocket text frame. Every call a remote client sends passes
//! `required` first: phones hold the four device capabilities, a paired
//! computer can hold `Owner` (everything a local window may do, except what
//! must stay on the server itself).

use crate::method::Method;
use crate::rpc::Call;
use crate::write::{HostWrite, WriteMethod};
use aurelius_domain::autonomy::AgentMode;
use aurelius_domain::connection::{ConfigScope, Purpose};
use aurelius_domain::{Command, HostId};
pub use aurelius_domain::remote::{Capability, RemoteClientView};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// What a remote client needs to call a method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Needs {
    Cap(Capability),
    /// Credentials, folders on the server, updates, backups, remote access.
    Owner,
}

/// Whether `caps` allow what `needs` asks for. `Owner` allows every capability.
pub fn grants(caps: &[Capability], needs: Needs) -> bool {
    let owner = caps.contains(&Capability::Owner);
    match needs {
        Needs::Cap(cap) => owner || caps.contains(&cap),
        Needs::Owner => owner,
    }
}

/// The right a call requires. Host paths in commands are checked separately
/// (a phone may only attach its own uploads).
pub fn required(call: &Call) -> Needs {
    match call {
        Call::Read(method) => method_needs(method),
        Call::Write(WriteMethod::Domain(command)) => command_needs(command),
        Call::Write(WriteMethod::Host(host)) => host_needs(host),
    }
}

fn method_needs(method: &Method) -> Needs {
    use Capability::*;
    match method {
        Method::SessionHello(_)
        | Method::ServerGetHealth
        | Method::ProjectList
        | Method::JobList
        | Method::ConversationGet { .. }
        | Method::EmployeeList { .. }
        | Method::EmployeeChat { .. }
        | Method::GroupChatGet
        | Method::DepartmentList { .. }
        | Method::CompanyGet
        | Method::ProjectConversationList { .. }
        | Method::ApprovalList
        | Method::ProviderList
        | Method::TaskList { .. }
        | Method::TimelineList(_)
        | Method::MemoryList { .. }
        | Method::MemoryGetContext { .. }
        | Method::RunGetContext { .. }
        | Method::ScreenshotGet { .. }
        | Method::FileGet { .. }
        | Method::UploadGet { .. }
        | Method::IdeaList { .. }
        | Method::IdeaListAll {}
        | Method::NoteList { .. }
        | Method::NoteGet { .. }
        | Method::ProposalList { .. }
        | Method::EmployeeInstructionVersions { .. }
        | Method::RecurringTaskList { .. }
        | Method::ShareList { .. }
        | Method::UsageGet
        | Method::UsageGetHours { .. }
        | Method::UsageGetTotals { .. }
        | Method::RunGetTrace { .. }
        | Method::TraceGetSetting
        | Method::VariantsGetSetting
        | Method::TaskVariants { .. }
        | Method::LoadGet
        | Method::BrowserGet
        | Method::ArtifactGetSetting
        | Method::ManagementModeGetSetting
        | Method::AutoAllowGetSetting
        | Method::ArtifactList { .. }
        | Method::ArtifactGetContent { .. }
        | Method::McpList
        | Method::McpLoginGet { .. }
        | Method::McpAlwaysAllowList
        | Method::PluginList
        | Method::PluginListAvailable
        | Method::PluginGetForEmployee { .. }
        | Method::PluginGetForProject { .. }
        | Method::SubscriptionGet { .. }
        | Method::NotificationList
        | Method::EventSubscribe { .. }
        | Method::GitGetStatus { .. }
        | Method::GitGetDiff { .. }
        | Method::MarkdownList { .. }
        | Method::MarkdownGet { .. }
        | Method::SubagentList { .. }
        | Method::ActivityGet { .. }
        | Method::UpdateGet { .. } => Needs::Cap(Read),
        // Functions and API keys let outside apps start work on the server.
        Method::AgentFunctionList { .. }
        | Method::FunctionCallList { .. }
        | Method::ApiKeyList
        | Method::AppKeyList
        | Method::BackupList
        | Method::PowerGet
        | Method::RemoteGetStatus
        | Method::FsListDir { .. }
        | Method::ProviderDetectRuntime
        // A shell on the server can do anything the server can.
        | Method::TerminalOpen { .. }
        | Method::TerminalInput { .. }
        | Method::TerminalResize { .. }
        | Method::TerminalDetach { .. }
        | Method::TerminalList { .. }
        | Method::TerminalClose { .. } => Needs::Owner,
    }
}

fn host_needs(host: &HostWrite) -> Needs {
    use Capability::*;
    match host {
        // Updating the server from any client, the phone too: only a release
        // signed with the Aurelius key is ever installed, so it is a decision
        // like an approval, not server management. Restarting is what an update does anyway.
        HostWrite::UpdatePrepare | HostWrite::UpdateApply | HostWrite::UpdatePrepareApp | HostWrite::UpdateApplyApp | HostWrite::ServerRestart => Needs::Cap(Approve),
        // How busy the computer may get decides what runs, like an approval.
        HostWrite::LoadSetLimits(_) => Needs::Cap(Approve),
        // Which browser runs open pages for the user decides what the agent can reach.
        HostWrite::BrowserSet { .. } | HostWrite::BrowserSetBackend { .. } => Needs::Cap(Approve),
        // Artifacts let the agent publish to the user's claude.ai account: like the browser, an Approve decision.
        HostWrite::ArtifactSetEnabled { .. } => Needs::Cap(Approve),
        // Whether management may switch its own tasks to Auto decides what runs without asking.
        HostWrite::ManagementModeSetEnabled { .. } => Needs::Cap(Approve),
        HostWrite::AutoAllowSetEnabled { .. } => Needs::Cap(Approve),
        // A server-wide setting like the others in Settings; it only keeps more of what runs send.
        HostWrite::TraceSetEnabled { .. } => Needs::Cap(Approve),
        // Variants cost k times the tokens: turning them on is the Approve right's.
        HostWrite::VariantsSetEnabled { .. } => Needs::Cap(Approve),
        // An MCP server is a program or a service the agent can drive: like the browser, an Approve decision.
        HostWrite::McpSet { .. } => Needs::Cap(Approve),
        // Logging in to a server the owner already added is consent to use it, like `mcp.set`;
        // the phone must reach it, since its browser pastes the redirect back. Tokens stay on the server.
        HostWrite::McpLoginStart { .. } | HostWrite::McpLoginComplete { .. } | HostWrite::McpLogout { .. } => Needs::Cap(Approve),
        // The always-allow list is managed with the same right that fills it (answering approvals).
        HostWrite::McpAlwaysAllowRemove { .. } => Needs::Cap(Approve),
        // A plugin can bring hooks, which are commands on the server's computer: Approve, like MCP.
        HostWrite::PluginPreview { .. }
        | HostWrite::PluginInstall { .. }
        | HostWrite::PluginUpdate { .. }
        | HostWrite::PluginRemove { .. }
        | HostWrite::PluginSetForEmployee { .. }
        | HostWrite::PluginSetForProject { .. } => Needs::Cap(Approve),
        // Notes are project content, like ideas.
        HostWrite::NoteCreate { .. } | HostWrite::NoteUpdate { .. } | HostWrite::NoteSetPinned { .. } | HostWrite::NoteDelete { .. } => Needs::Cap(TaskControl),
        // The inbox is the user's own: reading, dismissing.
        HostWrite::NotificationMarkAllRead | HostWrite::NotificationDismiss { .. } | HostWrite::NotificationDismissAll => Needs::Cap(Read),
        HostWrite::SecretSet { .. }
        | HostWrite::McpAdd(_)
        | HostWrite::McpRemove { .. }
        | HostWrite::AgentFunctionCreate(_)
        | HostWrite::AgentFunctionUpdate(_)
        | HostWrite::AgentFunctionDelete { .. }
        | HostWrite::AgentFunctionSetEnabled { .. }
        | HostWrite::AgentFunctionSetAllEnabled { .. }
        | HostWrite::ApiKeyCreate { .. }
        | HostWrite::ApiKeyRevoke { .. }
        | HostWrite::ApiKeySetFunctions { .. }
        | HostWrite::AppKeyCreate(_)
        | HostWrite::AppKeyRevoke { .. }
        | HostWrite::BackupCreate
        | HostWrite::BackupRestore { .. }
        | HostWrite::BackupResume
        | HostWrite::ProjectExport { .. }
        | HostWrite::PowerShutdown { .. }
        | HostWrite::PowerCancel
        | HostWrite::RemoteEnable
        | HostWrite::RemoteDisable
        | HostWrite::RemoteStartPairing
        | HostWrite::RemoteDecidePairing { .. }
        | HostWrite::RemoteRevoke { .. }
        | HostWrite::PushConfigure(_)
        | HostWrite::PushTest { .. }
        | HostWrite::FsCreateDir { .. } => Needs::Owner,
    }
}

fn command_needs(command: &Command) -> Needs {
    use Capability::*;
    match command {
        // A new project conversation that starts in Auto mode or with the browser
        // or screen on needs what turning those on later needs.
        Command::SendProjectMessage(m) if m.conversation_id.is_none() && m.start.needs_approve() => Needs::Cap(Approve),
        Command::StartProjectConversation(m) if m.start.needs_approve() => Needs::Cap(Approve),
        Command::SendChatMessage(_)
        | Command::RetryReply(_)
        | Command::StopRun(_)
        | Command::DeleteQueuedMessage(_)
        | Command::SendQueuedNow(_)
        | Command::DeleteConversation(_)
        | Command::AttachFile(_)
        | Command::RemoveAttachment(_)
        | Command::SendEmployeeMessage(_)
        | Command::SendGroupMessage(_)
        | Command::StartProjectConversation(_)
        | Command::SendProjectMessage(_)
        | Command::ShareMessage(_)
        | Command::DiscussTask(_) => Needs::Cap(Chat),
        Command::SetAgentMode(m) if m.mode == AgentMode::Auto => Needs::Cap(Approve),
        Command::SetAgentMode(_) => Needs::Cap(Chat),
        Command::SetConversationRun(_) => Needs::Cap(Chat),
        // A task that starts in Auto mode needs what switching to Auto needs.
        Command::CreateTask(t) if t.run.mode == Some(AgentMode::Auto) => Needs::Cap(Approve),
        Command::CreateTaskSchedule(t) if t.run.mode == Some(AgentMode::Auto) => Needs::Cap(Approve),
        Command::UpdateTaskSchedule(t) if t.run.mode == Some(AgentMode::Auto) => Needs::Cap(Approve),
        Command::StartIdea(i) if i.run.mode == Some(AgentMode::Auto) => Needs::Cap(Approve),
        // The browser carries the user's logins: turning it on needs Approve (like Auto mode); anyone with Chat may turn it off.
        Command::SetBrowserAllowed(b) if b.allowed => Needs::Cap(Approve),
        Command::SetBrowserAllowed(_) => Needs::Cap(Chat),
        // Screenshots can show anything on screen: same rule as the browser.
        Command::SetScreenAllowed(s) if s.allowed => Needs::Cap(Approve),
        Command::SetScreenAllowed(_) => Needs::Cap(Chat),
        // Only an owner picks a folder on the server.
        Command::CreateProject(p) if p.folder.is_some() => Needs::Owner,
        Command::CreateEmployee(m) if m.folder.is_some() => Needs::Owner,
        Command::CreateProject(_)
        | Command::RenameProject(_)
        | Command::SetProjectDescription(_)
        | Command::SetProjectStatus(_)
        | Command::SetFollowOrgChart(_)
        | Command::CreateEmployee(_)
        | Command::UpdateEmployee(_)
        | Command::RestoreEmployeeInstructions(_)
        | Command::SetEmployeeStatus(_)
        | Command::SetEmployeeReportsTo(_)
        | Command::SetEmployeeDepartment(_)
        | Command::CreateDepartment(_)
        | Command::UpdateDepartment(_)
        | Command::ArchiveDepartment(_)
        | Command::SetHeartbeat(_)
        | Command::CreateTask(_)
        | Command::CreateTaskSchedule(_)
        | Command::UpdateTaskSchedule(_)
        | Command::DeleteTaskSchedule(_)
        | Command::SetTaskSchedulePaused(_)
        | Command::RunTaskScheduleNow(_)
        | Command::CancelTask(_)
        | Command::AssignTask(_)
        | Command::AddIdea(_)
        | Command::UpdateIdea(_)
        | Command::SetIdeaStatus(_)
        | Command::StartIdea(_)
        | Command::DeleteIdea(_)
        | Command::Remember(_)
        | Command::DecideMemory(_)
        | Command::ReviseMemory(_)
        | Command::ForgetMemory(_)
        | Command::PinMemory(_)
        | Command::ConsolidateMemory(_)
        | Command::RevokeShare(_) => Needs::Cap(TaskControl),
        // Creates an employee that works on the host: like approving an action.
        Command::DecideApproval(_) | Command::AnswerQuestion(_) | Command::DecidePlan(_) | Command::DecideProposal(_) => Needs::Cap(Approve),
        // Puts the chosen diff into the work folder: like approving an action.
        Command::ChooseTaskVariant(_) => Needs::Cap(Approve),
        // Model and effort for the next employee runs (the phone's model
        // picker) and for subagents. Only the global default and an employee's own
        // default; other scoped overrides and memory settings need an owner.
        Command::SetRunDefaults(d)
            if (d.scope == ConfigScope::Global && matches!(d.purpose, Purpose::Employee | Purpose::Subagent))
                || (matches!(d.scope, ConfigScope::Employee(_)) && matches!(d.purpose, Purpose::Employee | Purpose::Subagent)) =>
        {
            Needs::Cap(Chat)
        }
        Command::AddProvider(_)
        | Command::RemoveProvider(_)
        | Command::VerifyProvider(_)
        | Command::SetRunDefaults(_)
        | Command::SetProjectFolder(_)
        | Command::SetEmployeeFolder(_) => Needs::Owner,
    }
}

/// Settings → Remote access.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RemoteStatus {
    pub enabled: bool,
    /// Loopback port of the gateway (Tailscale Serve forwards to it).
    pub port: u16,
    /// What a phone connects to, e.g. `https://desktop.tail1234.ts.net:8443`.
    pub url: Option<String>,
    pub tailscale: TailscaleStatus,
    pub clients: Vec<RemoteClientView>,
    /// Devices that entered a valid code and wait for "Allow" on the desktop.
    pub pending: Vec<PairingRequestView>,
    pub push: PushStatus,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TailscaleStatus {
    pub installed: bool,
    pub online: bool,
    /// MagicDNS name of this PC, without the trailing dot.
    pub dns_name: Option<String>,
    /// Tailscale Serve forwards the remote port to the gateway.
    pub serving: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PairingRequestView {
    pub pairing_id: String,
    pub device_name: String,
    pub platform: String,
    pub requested_at_ms: i64,
    /// A computer asking for full access (owner).
    pub owner: bool,
}

/// A fresh one-time pairing code; `qr` is the JSON the desktop renders as a QR code.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PairingCode {
    pub code: String,
    pub qr: String,
    pub expires_at_ms: i64,
    /// The same pairing as text to paste into Aurelius on another computer
    /// (`aurelius-pair:` + base64url of the payload JSON).
    pub pairing_text: String,
}

/// What the QR code holds (`v` = 1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PairingPayload {
    pub v: u32,
    pub url: String,
    pub code: String,
    pub host_id: HostId,
    pub host_name: String,
    pub env: String,
}

/// Prefix of the text form of a pairing payload (for computers, which have no camera to scan the QR).
pub const PAIRING_TEXT_PREFIX: &str = "aurelius-pair:";

impl PairingPayload {
    pub fn to_text(&self) -> String {
        use base64::Engine;
        let json = serde_json::to_vec(self).unwrap_or_default();
        format!("{PAIRING_TEXT_PREFIX}{}", base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json))
    }

    /// Accepts the text form or the QR's JSON; surrounding whitespace is ignored.
    pub fn from_text(text: &str) -> Option<Self> {
        use base64::Engine;
        let text = text.trim();
        if let Some(encoded) = text.strip_prefix(PAIRING_TEXT_PREFIX) {
            let json = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(encoded.trim().trim_end_matches('=')).ok()?;
            return serde_json::from_slice(&json).ok();
        }
        serde_json::from_str(text).ok()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PushStatus {
    /// An APNs key is stored and the ids are set.
    pub apns: bool,
    pub key_id: Option<String>,
    pub team_id: Option<String>,
    pub bundle_id: Option<String>,
    pub last_error: Option<String>,
}

/// `push.configure`. `key_p8` is the APNs auth key file content; none keeps
/// the stored key. The key goes to the OS credential store, never to SQLite.
#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PushConfig {
    pub key_id: String,
    pub team_id: String,
    pub bundle_id: String,
    #[serde(default)]
    pub key_p8: Option<String>,
}

impl std::fmt::Debug for PushConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PushConfig").field("key_id", &self.key_id).field("team_id", &self.team_id).field("bundle_id", &self.bundle_id).field("key_p8", &self.key_p8.as_ref().map(|_| "<hidden>")).finish()
    }
}

/// Ephemeral streamed text for remote clients: `text` continues the message
/// at character `offset`. `offset == 0` carries the full text so far.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StreamAppend {
    pub run_id: aurelius_domain::RunId,
    pub conversation_id: aurelius_domain::ConversationId,
    pub message_id: aurelius_domain::MessageId,
    pub offset: u64,
    pub text: String,
    /// The tools used so far, sent only when they changed since the last
    /// append for this message (offsets are UTF-8 bytes, as in `MessageView`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activity: Option<Vec<crate::views::ActivityView>>,
    /// How long it runs and the tokens so far (see `StreamDelta`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<crate::views::RunProgress>,
    /// `activity` holds only new or changed calls (see `StreamDelta::activity_changes`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub activity_changes: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use aurelius_domain::{CommandId, ConversationId, autonomy::SetAgentMode, project::CreateProject};

    fn exec(command: Command) -> Call {
        Call::Write(WriteMethod::Domain(command))
    }

    fn read(method: Method) -> Call {
        Call::Read(method)
    }

    fn host(write: HostWrite) -> Call {
        Call::Write(WriteMethod::Host(write))
    }

    fn cap(c: Capability) -> Needs {
        Needs::Cap(c)
    }

    #[test]
    fn the_always_allow_list_is_read_with_read_and_managed_with_approve() {
        assert_eq!(required(&read(Method::McpAlwaysAllowList)), cap(Capability::Read));
        assert_eq!(required(&host(HostWrite::McpAlwaysAllowRemove { tool: "mcp__x__y".into() })), cap(Capability::Approve));
    }

    #[test]
    fn turning_artifacts_on_needs_approve_and_listing_them_read() {
        assert_eq!(required(&host(HostWrite::ArtifactSetEnabled { enabled: true })), cap(Capability::Approve));
        assert_eq!(required(&read(Method::ArtifactGetSetting)), cap(Capability::Read));
        assert_eq!(required(&host(HostWrite::TraceSetEnabled { enabled: true })), cap(Capability::Approve));
        assert_eq!(required(&read(Method::RunGetTrace { run_id: aurelius_domain::RunId::new() })), cap(Capability::Read));
        assert_eq!(required(&read(Method::ArtifactList { conversation_id: None, task_id: None, project_id: None })), cap(Capability::Read));
        assert_eq!(required(&read(Method::ArtifactGetContent { url: "https://claude.ai/artifact/x".into(), version: None })), cap(Capability::Read));
    }

    #[test]
    fn server_management_needs_an_owner() {
        use crate::views::ShutdownWhen;
        for call in [
            host(HostWrite::SecretSet { connection_id: aurelius_domain::ProviderId::new(), secret: "x".into() }),
            host(HostWrite::BackupCreate),
            host(HostWrite::RemoteEnable),
            host(HostWrite::FsCreateDir { parent: "C:/".into(), name: "New".into() }),
            host(HostWrite::PowerShutdown { when: ShutdownWhen::Now }),
            host(HostWrite::PowerCancel),
            host(HostWrite::McpRemove { name: "x".into() }),
            read(Method::FsListDir { path: None }),
            read(Method::PowerGet),
            read(Method::ProviderDetectRuntime),
            read(Method::TerminalOpen { conversation_id: ConversationId::new(), cols: 80, rows: 24, fresh: false, terminal_id: None, additional: true }),
            read(Method::TerminalInput { terminal_id: "t".into(), data_base64: String::new() }),
            read(Method::TerminalDetach { terminal_id: "t".into() }),
            read(Method::TerminalList { conversation_id: ConversationId::new() }),
        ] {
            assert_eq!(required(&call), Needs::Owner, "{call:?}");
        }
    }

    #[test]
    fn project_conversations_need_chat_and_auto_needs_approve() {
        use aurelius_domain::{conversation::ChatStart, project_conversation::{SendProjectMessage, StartProjectConversation}};
        let project_id = aurelius_domain::ProjectId::new();
        let send = |start: ChatStart, conversation_id| exec(Command::SendProjectMessage(SendProjectMessage { project_id, text: "x".into(), conversation_id, images: vec![], files: vec![], interrupt: false, start }));
        let auto = ChatStart { agent_mode: Some(AgentMode::Auto), ..Default::default() };
        assert_eq!(required(&send(ChatStart::default(), None)), cap(Capability::Chat));
        assert_eq!(required(&send(auto.clone(), None)), cap(Capability::Approve));
        assert_eq!(required(&send(auto.clone(), Some(ConversationId::new()))), cap(Capability::Chat), "an existing conversation keeps its mode");
        assert_eq!(required(&exec(Command::StartProjectConversation(StartProjectConversation { project_id, start: ChatStart::default() }))), cap(Capability::Chat));
        assert_eq!(required(&exec(Command::StartProjectConversation(StartProjectConversation { project_id, start: auto }))), cap(Capability::Approve));
        assert_eq!(required(&read(Method::ProjectConversationList { project_id })), cap(Capability::Read));
    }

    #[test]
    fn a_phone_with_chat_may_change_waiting_messages() {
        let (conversation_id, message_id) = (ConversationId::new(), aurelius_domain::MessageId::new());
        let delete = exec(Command::DeleteQueuedMessage(aurelius_domain::conversation::DeleteQueuedMessage { conversation_id, message_id }));
        let now = exec(Command::SendQueuedNow(aurelius_domain::conversation::SendQueuedNow { conversation_id, message_id }));
        for call in [delete, now] {
            assert_eq!(required(&call), cap(Capability::Chat), "{call:?}");
            assert!(grants(&[Capability::Read, Capability::Chat], required(&call)));
            assert!(!grants(&[Capability::Read], required(&call)));
        }
    }

    #[test]
    fn a_phone_with_approve_may_log_in_to_mcp_servers() {
        for call in [
            host(HostWrite::McpLoginStart { name: "x".into() }),
            host(HostWrite::McpLoginComplete { login_id: "l".into(), redirect_url: "http://localhost:1/callback?code=c&state=s".into() }),
            host(HostWrite::McpLogout { name: "x".into() }),
        ] {
            assert_eq!(required(&call), cap(Capability::Approve), "{call:?}");
            assert!(grants(&Capability::DEVICE_DEFAULT, required(&call)) == Capability::DEVICE_DEFAULT.contains(&Capability::Approve));
        }
        assert_eq!(required(&read(Method::McpLoginGet { login_id: "l".into() })), cap(Capability::Read));
    }

    #[test]
    fn agent_functions_and_api_keys_are_for_owners_only() {
        let id = aurelius_domain::AgentFunctionId::new();
        for call in [
            read(Method::AgentFunctionList { project_id: None }),
            read(Method::FunctionCallList { function_id: None, limit: None }),
            read(Method::ApiKeyList),
            host(HostWrite::AgentFunctionDelete { function_id: id }),
            host(HostWrite::AgentFunctionSetEnabled { function_id: id, enabled: false }),
            host(HostWrite::AgentFunctionSetAllEnabled { enabled: false }),
            host(HostWrite::ApiKeyCreate { name: "demo".into(), function_ids: vec![id] }),
            host(HostWrite::ApiKeyRevoke { key_id: aurelius_domain::ApiKeyId::new() }),
            read(Method::AppKeyList),
            host(HostWrite::AppKeyCreate(aurelius_domain::app_key::CreateAppKey {
                name: "assistant".into(),
                scopes: vec![aurelius_domain::app_key::AppScope::Read],
                limit_per_hour: None,
                limit_concurrent: None,
                audience: Default::default(),
                webhook_url: None,
            })),
            host(HostWrite::AppKeyRevoke { key_id: aurelius_domain::AppKeyId::new() }),
        ] {
            assert_eq!(required(&call), Needs::Owner, "{call:?}");
            assert!(!grants(&[Capability::Read, Capability::Approve, Capability::Chat], required(&call)));
            assert!(grants(&[Capability::Owner], required(&call)));
        }
    }

    #[test]
    fn any_paired_device_may_update_the_server() {
        assert_eq!(required(&read(Method::UpdateGet { check: true })), cap(Capability::Read));
        for call in [host(HostWrite::UpdatePrepare), host(HostWrite::UpdateApply), host(HostWrite::ServerRestart)] {
            assert_eq!(required(&call), cap(Capability::Approve));
            assert!(grants(&Capability::DEVICE_DEFAULT, required(&call)));
        }
    }

    #[test]
    fn a_device_reads_the_load_and_changes_its_limits_like_an_approval() {
        assert_eq!(required(&read(Method::LoadGet)), cap(Capability::Read));
        assert_eq!(required(&host(HostWrite::LoadSetLimits(Default::default()))), cap(Capability::Approve));
    }

    #[test]
    fn owner_grants_everything() {
        for needs in [cap(Capability::Read), cap(Capability::Chat), cap(Capability::TaskControl), cap(Capability::Approve), Needs::Owner] {
            assert!(grants(&[Capability::Owner], needs), "{needs:?}");
        }
    }

    #[test]
    fn a_phone_is_never_an_owner() {
        assert!(!Capability::DEVICE_DEFAULT.contains(&Capability::Owner));
        assert!(!grants(&Capability::DEVICE_DEFAULT, Needs::Owner));
        assert!(grants(&Capability::DEVICE_DEFAULT, cap(Capability::Approve)));
        assert!(!grants(&[Capability::Read], cap(Capability::Chat)));
    }

    #[test]
    fn stored_owner_parses() {
        assert_eq!(Capability::parse("owner"), Some(Capability::Owner));
        for c in Capability::ALL {
            assert_eq!(Capability::parse(c.as_str()), Some(c));
        }
    }

    #[test]
    fn host_folders_need_an_owner() {
        let with = exec(Command::CreateProject(CreateProject { name: "A".into(), description: None, folder: Some(r"C:\x".into()) }));
        let without = exec(Command::CreateProject(CreateProject { name: "A".into(), description: None, folder: None }));
        assert_eq!(required(&with), Needs::Owner);
        assert_eq!(required(&without), cap(Capability::TaskControl));
    }

    #[test]
    fn phone_sets_only_the_global_and_employee_models() {
        use aurelius_domain::connection::{RunDefaults, SetRunDefaults};
        let set = |scope, purpose| exec(Command::SetRunDefaults(SetRunDefaults { scope, purpose, settings: RunDefaults::default() }));
        assert_eq!(required(&set(ConfigScope::Global, Purpose::Employee)), cap(Capability::Chat));
        assert_eq!(required(&set(ConfigScope::Global, Purpose::Subagent)), cap(Capability::Chat));
        assert_eq!(required(&set(ConfigScope::Global, Purpose::Memory)), Needs::Owner);
        assert_eq!(required(&set(ConfigScope::Project(aurelius_domain::ProjectId::new()), Purpose::Employee)), Needs::Owner);
        let employee = ConfigScope::Employee(aurelius_domain::EmployeeId::new());
        assert_eq!(required(&set(employee, Purpose::Employee)), cap(Capability::Chat));
        assert_eq!(required(&set(employee, Purpose::Subagent)), cap(Capability::Chat));
        assert_eq!(required(&set(employee, Purpose::Memory)), Needs::Owner);
    }

    #[test]
    fn auto_mode_needs_approve() {
        let id = ConversationId::new();
        let auto = exec(Command::SetAgentMode(SetAgentMode { conversation_id: id, mode: AgentMode::Auto }));
        let plan = exec(Command::SetAgentMode(SetAgentMode { conversation_id: id, mode: AgentMode::Plan }));
        assert_eq!(required(&auto), cap(Capability::Approve));
        assert_eq!(required(&plan), cap(Capability::Chat));
    }

    #[test]
    fn org_chart_changes_need_task_control_and_proposals_need_approve() {
        use aurelius_domain::{DepartmentId, EmployeeId, ProposalId, department::{ArchiveDepartment, CreateDepartment}, employee::{SetEmployeeDepartment, SetEmployeeReportsTo, SetFollowOrgChart}, proposal::DecideProposal};
        let set = exec(Command::SetEmployeeReportsTo(SetEmployeeReportsTo { employee_id: EmployeeId::new(), reports_to: None }));
        let follow = exec(Command::SetFollowOrgChart(SetFollowOrgChart { follow: true }));
        let decide = exec(Command::DecideProposal(DecideProposal { proposal_id: ProposalId::new(), approve: true, edits: None, note: None, instructions: None }));
        assert_eq!(required(&set), cap(Capability::TaskControl));
        assert_eq!(required(&follow), cap(Capability::TaskControl));
        assert_eq!(required(&decide), cap(Capability::Approve));
        assert_eq!(required(&read(Method::ProposalList { department_id: None })), cap(Capability::Read));
        // Departments and the company are org chart changes and reads too.
        for command in [
            Command::CreateDepartment(CreateDepartment { name: "Engineering".into(), description: None, head_id: None }),
            Command::ArchiveDepartment(ArchiveDepartment { department_id: DepartmentId::new() }),
            Command::SetEmployeeDepartment(SetEmployeeDepartment { employee_id: EmployeeId::new(), department_id: None }),
        ] {
            assert_eq!(required(&exec(command)), cap(Capability::TaskControl));
        }
        for method in [Method::CompanyGet, Method::DepartmentList { archived: true }, Method::EmployeeChat { employee_id: EmployeeId::new() }, Method::GroupChatGet] {
            assert_eq!(required(&read(method)), cap(Capability::Read));
        }
        // Writing in the group chat is chatting.
        let group = Command::SendGroupMessage(aurelius_domain::group_chat::SendGroupMessage { text: "Hi".into(), images: vec![], files: vec![] });
        assert_eq!(required(&exec(group)), cap(Capability::Chat));
    }

    #[test]
    fn recurring_tasks_need_what_creating_tasks_needs() {
        use aurelius_domain::schedule::*;
        use aurelius_domain::{EmployeeId, ProjectId, TaskScheduleId, task::RunSettings};
        let create = |mode| {
            exec(Command::CreateTaskSchedule(CreateTaskSchedule {
                project_id: ProjectId::new(),
                employee_id: EmployeeId::new(),
                title: "T".into(),
                goal: "G".into(),
                acceptance_criteria: vec![],
                files: vec![],
                run: RunSettings { mode, ..RunSettings::default() },
                recurrence: Recurrence::Daily { at_minute: 0 },
                paused: false,
            }))
        };
        assert_eq!(required(&create(None)), cap(Capability::TaskControl));
        assert_eq!(required(&create(Some(AgentMode::Plan))), cap(Capability::TaskControl));
        assert_eq!(required(&create(Some(AgentMode::Auto))), cap(Capability::Approve));
        let id = TaskScheduleId::new();
        for command in [
            Command::DeleteTaskSchedule(DeleteTaskSchedule { schedule_id: id }),
            Command::SetTaskSchedulePaused(SetTaskSchedulePaused { schedule_id: id, paused: true }),
            Command::RunTaskScheduleNow(RunTaskScheduleNow { schedule_id: id }),
        ] {
            assert_eq!(required(&exec(command)), cap(Capability::TaskControl));
        }
        assert_eq!(required(&read(Method::RecurringTaskList { project_id: None })), cap(Capability::Read));
    }

    #[test]
    fn a_task_or_idea_that_starts_in_auto_needs_approve() {
        use aurelius_domain::{idea::StartIdea, task::{CreateTask, RunSettings}};
        let run = |mode| RunSettings { mode, ..RunSettings::default() };
        let start = |mode| exec(Command::StartIdea(StartIdea { idea_id: aurelius_domain::IdeaId::new(), employee_id: aurelius_domain::EmployeeId::new(), run: run(mode) }));
        let create = |mode| {
            let task: CreateTask = serde_json::from_value(serde_json::json!({ "project_id": aurelius_domain::ProjectId::new(), "title": "T", "goal": "G" })).unwrap();
            exec(Command::CreateTask(CreateTask { files: vec![], run: run(mode), ..task }))
        };
        for call in [start(Some(AgentMode::Auto)), create(Some(AgentMode::Auto))] {
            assert_eq!(required(&call), cap(Capability::Approve), "{call:?}");
        }
        for mode in [None, Some(AgentMode::Plan), Some(AgentMode::AcceptEdits), Some(AgentMode::Ask)] {
            assert_eq!(required(&start(mode)), cap(Capability::TaskControl));
            assert_eq!(required(&create(mode)), cap(Capability::TaskControl));
        }
    }

    #[test]
    fn browser_on_needs_approve_and_plans_are_decided_with_approve() {
        use aurelius_domain::{ApprovalId, approval::{DecidePlan, PlanDecision}, autonomy::SetBrowserAllowed};
        let id = ConversationId::new();
        let on = exec(Command::SetBrowserAllowed(SetBrowserAllowed { conversation_id: id, allowed: true }));
        let off = exec(Command::SetBrowserAllowed(SetBrowserAllowed { conversation_id: id, allowed: false }));
        assert_eq!(required(&on), cap(Capability::Approve));
        assert_eq!(required(&off), cap(Capability::Chat));
        let plan = exec(Command::DecidePlan(DecidePlan { approval_id: ApprovalId::new(), decision: PlanDecision::Approve, feedback: None }));
        assert_eq!(required(&plan), cap(Capability::Approve));
    }

    #[test]
    fn pairing_text_roundtrips() {
        let payload = PairingPayload {
            v: 1,
            url: "https://desktop.tail1.ts.net:8443".into(),
            code: "ABCD2345".into(),
            host_id: HostId::new(),
            host_name: "my-pc".into(),
            env: "production".into(),
        };
        let text = payload.to_text();
        assert!(text.starts_with(PAIRING_TEXT_PREFIX));
        assert_eq!(PairingPayload::from_text(&format!("  {text}\n")), Some(payload.clone()));
        assert_eq!(PairingPayload::from_text(&serde_json::to_string(&payload).unwrap()), Some(payload));
        assert_eq!(PairingPayload::from_text("aurelius-pair:nonsense"), None);
    }

    #[test]
    fn every_write_has_a_right() {
        // The keys are only needed to build requests.
        let _ = CommandId::new();
        for name in HostWrite::NAMES {
            assert!(WriteMethod::knows(name));
        }
    }
}
