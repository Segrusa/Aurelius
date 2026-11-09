//! Command handling:
//! validate → write blobs → one immediate transaction that re-checks the
//! receipt, appends events, updates projections, creates jobs and stores the
//! receipt → commit → caller publishes notices.

use aurelius_domain::{
    ActorKind, AppError, ClientId, Command, CommandEnvelope, CommandId, CommandReceipt, CommandResult, DomainError, ErrorCode, EventKind, EventNotice,
    PrincipalId, ProjectId,
    project::{self, ProjectEvent},
};
use aurelius_storage::{
    PreparedBlob, Store, StorageError, blobs,
    events::{self, NewEvent},
    queries,
};
use aurelius_storage::Tx as Transaction;
use sha2::{Digest, Sha256};
use std::sync::Arc;

/// Where a command came from. Set by the server-side adapter, never by the client.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    LocalIpc,
    Daemon,
    /// A paired device through the remote gateway; `client_id` is the device.
    Remote,
}

/// Authenticated command context. The local IPC adapter injects
/// the local OS user; the remote gateway injects the paired device.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CommandContext {
    pub principal_id: PrincipalId,
    pub client_id: ClientId,
    pub origin: Origin,
    pub request_id: u64,
    /// The app whose key this session authenticated with (`aurelius-mcp`);
    /// none for the user's own clients. Its scopes were checked by the session;
    /// its limits and the Auto rule are checked where tasks are made.
    pub app: Option<AppActor>,
    /// The active board member of the connection (`user.use`, `user.create`, or
    /// the hello's remembered one); none for apps and before the onboarding.
    pub user_id: Option<aurelius_domain::UserId>,
}

impl CommandContext {
    /// The board member acting, never an app: whose Claude login what they make
    /// or (re)activate runs on (B5).
    pub(crate) fn login_user(&self) -> Option<String> {
        self.user_id.filter(|_| self.app.is_none()).map(|u| u.to_string())
    }
}

/// An app acting through its app key.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct AppActor {
    pub key_id: aurelius_domain::AppKeyId,
    pub name: String,
}

impl CommandContext {
    pub fn metadata(&self) -> serde_json::Value {
        let mut meta = serde_json::json!({
            "principal_id": self.principal_id,
            "client_id": self.client_id,
            "origin": self.origin,
            "request_id": self.request_id,
        });
        if let Some(app) = &self.app {
            meta["app_key_id"] = serde_json::json!(app.key_id);
            meta["app"] = serde_json::json!(app.name);
        }
        if let Some(user) = self.user_id {
            meta["user_id"] = serde_json::json!(user);
        }
        meta
    }
}

pub const CHAT_RUN_KIND: &str = "run.chat";

pub mod conversation;
pub mod guideline;
pub mod guideline_proposal;
pub mod guideline_read;
pub mod memory;
pub mod org;
pub mod run;
pub mod task;
pub mod server_name;
pub mod ticket;
pub(crate) mod ticket_follow;
pub mod user;

pub(crate) fn chat_next_version(tx: &Transaction<'_>, aggregate_type: &str, id: &str) -> Result<u64, StorageError> {
    aurelius_storage::conversations::next_version(tx, aggregate_type, id)
}

pub(crate) fn queries_load_project(tx: &Transaction<'_>, id: ProjectId) -> Result<Option<aurelius_domain::project::Project>, StorageError> {
    queries::load_project(tx, id)
}

/// A project's name, if it exists.
pub(crate) fn queries_project_name(conn: &rusqlite::Connection, id: ProjectId) -> Option<String> {
    conn.query_row("SELECT name FROM projects WHERE id = ?1", [id.to_string()], |r| r.get(0)).ok()
}

pub struct Committed {
    pub receipt: CommandReceipt,
    /// Publish these to subscribers after the commit.
    pub notices: Vec<EventNotice>,
    /// Side work for the daemon after the commit (never on a replayed receipt).
    pub followups: Vec<Followup>,
}

/// Work the daemon must do after a command committed.
#[derive(Debug, Clone, PartialEq)]
pub enum Followup {
    /// Probe a connection (status, models) outside the command transaction.
    VerifyProvider(aurelius_domain::ProviderId),
    /// The user switched a conversation's working mode: tell its running answers now.
    AgentModeChanged(aurelius_domain::ConversationId),
}

pub struct CommandService {
    store: Arc<Store>,
    /// Where project folders are created (set by the daemon from the platform).
    projects_root: std::sync::Mutex<Option<std::path::PathBuf>>,
}

pub(crate) fn storage_error(err: StorageError) -> AppError {
    tracing::error!(error = %err, "storage failure");
    AppError::new(ErrorCode::StorageUnavailable, format!("Could not save: {err}"))
}

pub(crate) fn domain_error(err: DomainError) -> AppError {
    let code = match err {
        DomainError::ProjectNotFound(_)
        | DomainError::EmployeeNotFound(_)
        | DomainError::ConversationNotFound(_)
        | DomainError::MessageNotFound(_)
        | DomainError::RunNotFound(_)
        | DomainError::ApprovalNotFound(_)
        | DomainError::ProviderNotFound(_)
        | DomainError::AgentFunctionNotFound(_)
        | DomainError::ApiKeyNotFound(_)
        | DomainError::AppKeyNotFound(_)
        | DomainError::ProposalNotFound(_)
        | DomainError::TaskNotFound(_)
        | DomainError::TicketNotFound(_)
        | DomainError::BoardColumnNotFound(_)
        | DomainError::GuidelineNotFound(_)
        | DomainError::GuidelineProposalNotFound(_) => ErrorCode::NotFound,
        DomainError::GuidelineTitleTaken => ErrorCode::GuidelineTitleTaken,
        DomainError::VersionConflict { .. } => ErrorCode::VersionConflict,
        DomainError::BoardMemberRequired => ErrorCode::BoardMemberRequired,
        DomainError::InvalidUsername(_) => ErrorCode::InvalidUsername,
        DomainError::WeakPassword(_) => ErrorCode::WeakPassword,
        DomainError::UsernameTaken => ErrorCode::UsernameTaken,
        _ => ErrorCode::Validation,
    };
    AppError::new(code, err.to_string())
}

pub(crate) fn idempotency_conflict() -> AppError {
    AppError::new(
        ErrorCode::IdempotencyConflict,
        "This command ID was already used for a different request.",
    )
}

thread_local! {
    /// The protocol method of the command being executed on this thread, for its receipt.
    /// shortcut: `commit` reads it here instead of getting it as a parameter (65 call sites);
    /// pass it through `commit` the next time those handlers are touched.
    static COMMAND_METHOD: std::cell::Cell<&'static str> = const { std::cell::Cell::new("") };
}

/// Hash of everything except the command ID: same ID + same hash = retry.
pub fn request_hash(envelope: &CommandEnvelope) -> String {
    let canonical = serde_json::to_vec(&(&envelope.expected_version, &envelope.command)).unwrap_or_default();
    hex::encode(Sha256::digest(&canonical))
}

/// Hash of a request of the host write pipeline (MCP, plugins, ...): the same
/// key with the same parameters replays, with other parameters it conflicts.
pub fn request_hash_of<T: serde::Serialize>(value: &T) -> String {
    hex::encode(Sha256::digest(serde_json::to_vec(value).unwrap_or_default()))
}

/// Inner result: domain rejections are decided before anything is written,
/// so committing the (empty) transaction is harmless.
pub(crate) type TxResult<T> = Result<Result<T, AppError>, StorageError>;

impl CommandService {
    pub fn new(store: Arc<Store>) -> Self {
        Self { store, projects_root: std::sync::Mutex::new(None) }
    }

    pub fn set_projects_root(&self, root: std::path::PathBuf) {
        *self.projects_root.lock().unwrap_or_else(|p| p.into_inner()) = Some(root);
    }

    pub fn projects_root(&self) -> Option<std::path::PathBuf> {
        self.projects_root.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    pub fn store(&self) -> &Arc<Store> {
        &self.store
    }

    pub fn execute(&self, ctx: &CommandContext, envelope: CommandEnvelope) -> Result<Committed, AppError> {
        let hash = request_hash(&envelope);
        let command_id = envelope.command_id.to_string();
        COMMAND_METHOD.set(envelope.command.method());

        // Fast path for retries: answer from the receipt without writing blobs.
        if let Some(receipt) = self.replay(&command_id, &hash)? {
            return Ok(Committed { receipt, notices: vec![], followups: vec![] });
        }

        let result = match &envelope.command {
            Command::CreateProject(cmd) => self.create_project(ctx, &envelope, hash, cmd),
            Command::RenameProject(cmd) => self.rename_project(ctx, &envelope, hash, cmd),
            Command::SetProjectDescription(cmd) => self.set_project_description(ctx, &envelope, hash, cmd),
            Command::SendChatMessage(cmd) => self.send_chat_message(ctx, &envelope, hash, cmd),
            Command::RetryReply(cmd) => self.retry_reply(ctx, &envelope, hash, cmd),
            Command::StopRun(cmd) => self.stop_run(ctx, &envelope, hash, cmd),
            Command::CreateEmployee(cmd) => self.create_employee(ctx, &envelope, hash, cmd),
            Command::OpenEmployeeChat(cmd) => self.open_employee_chat(ctx, &envelope, hash, cmd),
            Command::CancelScheduledMessage(cmd) => self.cancel_scheduled_message(ctx, &envelope, hash, cmd),
            Command::UpdateEmployee(cmd) => self.update_employee(ctx, &envelope, hash, cmd),
            Command::SetEmployeeStatus(cmd) => self.set_employee_status(ctx, &envelope, hash, cmd),
            Command::SetProjectStatus(cmd) => self.set_project_status(ctx, &envelope, hash, cmd),
            Command::SendEmployeeMessage(cmd) => self.send_employee_message(ctx, &envelope, hash, cmd),
            Command::SendGroupMessage(cmd) => self.send_group_message(ctx, &envelope, hash, cmd),
            Command::DecideApproval(cmd) => self.decide_approval(ctx, &envelope, hash, cmd),
            Command::AddProvider(cmd) => self.add_connection(ctx, &envelope, hash, cmd),
            Command::RemoveProvider(cmd) => self.remove_connection(ctx, &envelope, hash, cmd),
            Command::VerifyProvider(cmd) => self.verify_connection(&envelope, hash, cmd),
            Command::SetRunDefaults(cmd) => self.set_run_defaults(ctx, &envelope, hash, cmd),
            Command::CreateTask(cmd) => self.create_task(ctx, &envelope, hash, cmd),
            Command::CancelTask(cmd) => self.cancel_task(&envelope, hash, cmd),
            Command::ChooseTaskVariant(cmd) => self.choose_task_variant(&envelope, hash, cmd),
            Command::AssignTask(cmd) => self.assign_task(&envelope, hash, cmd),
            Command::AddIdea(cmd) => self.add_idea(&envelope, hash, cmd),
            Command::UpdateIdea(cmd) => self.update_idea(&envelope, hash, cmd),
            Command::SetIdeaStatus(cmd) => self.set_idea_status(&envelope, hash, cmd),
            Command::StartIdea(cmd) => self.start_idea(ctx, &envelope, hash, cmd),
            Command::DeleteIdea(cmd) => self.delete_idea(&envelope, hash, cmd),
            Command::CreateTaskSchedule(cmd) => self.create_task_schedule(ctx, &envelope, hash, cmd),
            Command::UpdateTaskSchedule(cmd) => self.update_task_schedule(ctx, &envelope, hash, cmd),
            Command::DeleteTaskSchedule(cmd) => self.delete_task_schedule(&envelope, hash, cmd),
            Command::SetTaskSchedulePaused(cmd) => self.set_task_schedule_paused(ctx, &envelope, hash, cmd),
            Command::RunTaskScheduleNow(cmd) => self.run_task_schedule_now(&envelope, hash, cmd),
            Command::AnswerQuestion(cmd) => self.answer_question(ctx, &envelope, hash, cmd),
            Command::AttachFile(cmd) => self.attach_file(ctx, &envelope, hash, cmd),
            Command::RemoveAttachment(cmd) => self.remove_attachment(&envelope, hash, cmd),
            Command::Remember(cmd) => self.remember(ctx, &envelope, hash, cmd),
            Command::DecideMemory(cmd) => self.decide_memory(&envelope, hash, cmd),
            Command::ReviseMemory(cmd) => self.revise_memory(&envelope, hash, cmd),
            Command::ForgetMemory(cmd) => self.forget_memory(&envelope, hash, cmd),
            Command::PinMemory(cmd) => self.pin_memory(&envelope, hash, cmd),
            Command::ConsolidateMemory(cmd) => self.consolidate_memory(&envelope, hash, cmd),
            Command::SetAgentMode(cmd) => self.set_agent_mode(ctx, &envelope, hash, cmd),
            Command::SetConversationRun(cmd) => self.set_conversation_run(ctx, &envelope, hash, cmd),
            Command::SetBrowserAllowed(cmd) => self.set_browser_allowed(ctx, &envelope, hash, cmd),
            Command::SetScreenAllowed(cmd) => self.set_screen_allowed(ctx, &envelope, hash, cmd),
            Command::DecidePlan(cmd) => self.decide_plan(ctx, &envelope, hash, cmd),
            Command::SetProjectFolder(cmd) => self.set_project_folder(&envelope, hash, cmd),
            Command::ShareMessage(cmd) => self.share_message(ctx, &envelope, hash, cmd),
            Command::DiscussTask(cmd) => self.discuss_task(&envelope, hash, cmd),
            Command::RevokeShare(cmd) => self.revoke_share(&envelope, hash, cmd),
            Command::SetEmployeeFolder(cmd) => self.set_employee_folder(&envelope, hash, cmd),
            Command::DeleteConversation(cmd) => self.delete_conversation(ctx, &envelope, hash, cmd),
            Command::SetHeartbeat(cmd) => self.set_heartbeat(ctx, &envelope, hash, cmd),
            Command::StartProjectConversation(cmd) => self.start_project_conversation(ctx, &envelope, hash, cmd),
            Command::SendProjectMessage(cmd) => self.send_project_message(ctx, &envelope, hash, cmd),
            Command::DeleteQueuedMessage(cmd) => self.delete_queued_message(ctx, &envelope, hash, cmd),
            Command::SendQueuedNow(cmd) => self.send_queued_now(ctx, &envelope, hash, cmd),
            Command::SetFollowOrgChart(cmd) => self.set_follow_org_chart(ctx, &envelope, hash, cmd),
            Command::SetEmployeeReportsTo(cmd) => self.set_employee_reports_to(ctx, &envelope, hash, cmd),
            Command::DecideProposal(cmd) => self.decide_proposal(ctx, &envelope, hash, cmd),
            Command::RestoreEmployeeInstructions(cmd) => self.restore_employee_instructions(ctx, &envelope, hash, cmd),
            Command::CreateTicket(_)
            | Command::UpdateTicket(_)
            | Command::MoveTicket(_)
            | Command::StartTicket(_)
            | Command::AssignTicket(_)
            | Command::ApprovePlan(_)
            | Command::RevisePlan(_)
            | Command::AcceptTicket(_)
            | Command::SendBackTicket(_)
            | Command::CancelTicket(_)
            | Command::ArchiveTicket(_)
            | Command::UpdateBoardColumns(_)
            | Command::UpdateCompany(_)
            | Command::UseMyLogin(_)
            | Command::SetBackgroundLogin(_)
            | Command::SetTicketResponsible(_) => self.ticket_command(ctx, &envelope, hash),
            Command::FollowTicket(cmd) => self.follow_ticket(ctx, &envelope, hash, cmd),
            Command::UpdateProject(cmd) => self.update_project(ctx, &envelope, hash, cmd),
            Command::PostTicketMessage(cmd) => self.post_ticket_message(ctx, &envelope, hash, cmd),
            Command::RetryTicketSummary(cmd) => self.retry_ticket_summary(ctx, &envelope, hash, cmd),
            Command::CreateGuideline(_)
            | Command::UpdateGuideline(_)
            | Command::ArchiveGuideline(_)
            | Command::RestoreGuideline(_)
            | Command::RestoreGuidelineVersion(_)
            | Command::LinkGuideline(_)
            | Command::UnlinkGuideline(_)
            | Command::DecideGuidelineProposal(_)
            | Command::AddStarterGuidelines(_) => self.guideline_command(ctx, &envelope, hash),
            Command::SetServerName(cmd) => self.set_server_name(ctx, &envelope, hash, cmd),
            Command::ReactToMessage(cmd) => self.react_to_message(ctx, &envelope, hash, cmd),
            Command::EditMessage(cmd) => self.edit_message(ctx, &envelope, hash, cmd),
            Command::DeleteMessage(cmd) => self.delete_message(ctx, &envelope, hash, cmd),
        };
        COMMAND_METHOD.set("");
        result
    }

    fn replay(&self, command_id: &str, hash: &str) -> Result<Option<CommandReceipt>, AppError> {
        let command_id = command_id.to_owned();
        let hash = hash.to_owned();
        let found = self
            .store
            .write(move |tx| Ok(check_receipt(tx, &command_id, &hash)))
            .map_err(storage_error)?;
        found
    }

    fn create_project(
        &self,
        ctx: &CommandContext,
        envelope: &CommandEnvelope,
        hash: String,
        cmd: &project::CreateProject,
    ) -> Result<Committed, AppError> {
        if let Some(expected) = envelope.expected_version.filter(|v| *v != 0) {
            return Err(domain_error(DomainError::VersionConflict { expected, actual: 0 }));
        }
        let project_id = ProjectId::new();
        let event = cmd.decide(project_id).map_err(domain_error)?;
        let ProjectEvent::Created { name, description, .. } = event.clone() else { unreachable!() };

        let blobs = self.store.blobs();
        let payload = blobs.put_json(&event).map_err(storage_error)?;
        let description_blob = match &description {
            Some(text) => Some(blobs.put(text.as_bytes(), "text/plain; charset=utf-8").map_err(storage_error)?),
            None => None,
        };

        let command_id = envelope.command_id;
        let meta = ctx.metadata();
        let actor = ctx.principal_id.to_string();
        // The project's own folder, created before the transaction (idempotent).
        // The folder the user chose; otherwise a default one (tools, older clients).
        let folder = match cmd.folder.as_deref().filter(|p| !p.trim().is_empty()) {
            Some(path) => {
                let folder = self.chosen_folder(path)?;
                if self.store.read(|c| aurelius_storage::files::folder_taken(c, &folder)).map_err(storage_error)? {
                    return Err(domain_error(DomainError::File("Another project already uses this folder.".into())));
                }
                Some(folder)
            }
            None => match self.new_project_folder(&name) {
                Ok(folder) => folder,
                Err(err) => {
                    tracing::warn!(error = %err, "could not create the project folder");
                    None
                }
            },
        };
        self.commit(command_id, hash, move |tx| {
            register_all(tx, [Some(&payload), description_blob.as_ref()])?;
            let appended = events::append(
                tx,
                NewEvent {
                    aggregate_type: "project",
                    aggregate_id: project_id.to_string(),
                    aggregate_version: 1,
                    event_type: event.type_name(),
                    project_id: Some(project_id),
                    actor_kind: ActorKind::User,
                    actor_id: Some(actor),
                    correlation_id: command_id.to_string(),
                    causation_id: None,
                    payload_blob_id: Some(payload.id.to_string()),
                    metadata: meta,
                    dedupe_key: format!("cmd:{command_id}:0"),
                },
            )?;
            events::insert_project_projection(
                tx,
                project_id,
                &name,
                description_blob.as_ref().map(|b| b.id.to_string()).as_deref(),
            )?;
            if let Some(folder) = &folder {
                aurelius_storage::files::set_project_folder(tx, project_id, folder, aurelius_domain::GrantId::new(), appended.id)?;
            }
            Ok(Ok((
                CommandResult::ProjectCreated { project_id, version: 1 },
                vec![notice(appended.seq, event.type_name(), project_id.to_string(), Some(project_id))],
            )))
        })
    }

    fn rename_project(
        &self,
        ctx: &CommandContext,
        envelope: &CommandEnvelope,
        hash: String,
        cmd: &project::RenameProject,
    ) -> Result<Committed, AppError> {
        let cmd = cmd.clone();
        let expected = envelope.expected_version;
        let command_id = envelope.command_id;
        let meta = ctx.metadata();
        let actor = ctx.principal_id.to_string();
        // The payload only depends on the request, so it is written before the transaction.
        let planned = ProjectEvent::Renamed { project_id: cmd.project_id, name: cmd.name.trim().to_owned() };
        let payload = self.store.blobs().put_json(&planned).map_err(storage_error)?;
        self.commit(command_id, hash, move |tx| {
            let Some(current) = queries::load_project(tx, cmd.project_id)? else {
                return Ok(Err(domain_error(DomainError::ProjectNotFound(cmd.project_id))));
            };
            let event = match cmd.decide(&current, expected) {
                Ok(event) => event,
                Err(err) => return Ok(Err(domain_error(err))),
            };
            debug_assert_eq!(event, planned);
            let next = project::apply(Some(current), &event).expect("rename keeps project");
            blobs::register(tx, &payload)?;
            let appended = events::append(
                tx,
                NewEvent {
                    aggregate_type: "project",
                    aggregate_id: cmd.project_id.to_string(),
                    aggregate_version: next.version,
                    event_type: event.type_name(),
                    project_id: Some(cmd.project_id),
                    actor_kind: ActorKind::User,
                    actor_id: Some(actor),
                    correlation_id: command_id.to_string(),
                    causation_id: None,
                    payload_blob_id: Some(payload.id.to_string()),
                    metadata: meta,
                    dedupe_key: format!("cmd:{command_id}:0"),
                },
            )?;
            events::rename_project_projection(tx, cmd.project_id, &next.name, next.version)?;
            Ok(Ok((
                CommandResult::ProjectRenamed { project_id: cmd.project_id, version: next.version },
                vec![notice(appended.seq, event.type_name(), cmd.project_id.to_string(), Some(cmd.project_id))],
            )))
        })
    }

    fn set_project_description(
        &self,
        ctx: &CommandContext,
        envelope: &CommandEnvelope,
        hash: String,
        cmd: &project::SetProjectDescription,
    ) -> Result<Committed, AppError> {
        let cmd = cmd.clone();
        let expected = envelope.expected_version;
        let command_id = envelope.command_id;
        let meta = ctx.metadata();
        let actor = ctx.principal_id.to_string();
        // Validated up front so a too long text writes no blobs.
        let description = match cmd.description.trim() {
            "" => None,
            text if text.chars().count() > project::DESCRIPTION_MAX => {
                return Err(domain_error(DomainError::TooLong { field: "description".into(), max: project::DESCRIPTION_MAX }));
            }
            text => Some(text.to_owned()),
        };
        let planned = ProjectEvent::DescriptionSet { project_id: cmd.project_id, description: description.clone() };
        let blobs = self.store.blobs();
        let payload = blobs.put_json(&planned).map_err(storage_error)?;
        let description_blob = match &description {
            Some(text) => Some(blobs.put(text.as_bytes(), "text/plain; charset=utf-8").map_err(storage_error)?),
            None => None,
        };
        self.commit(command_id, hash, move |tx| {
            let Some(current) = queries::load_project(tx, cmd.project_id)? else {
                return Ok(Err(domain_error(DomainError::ProjectNotFound(cmd.project_id))));
            };
            let event = match cmd.decide(&current, expected) {
                Ok(event) => event,
                Err(err) => return Ok(Err(domain_error(err))),
            };
            debug_assert_eq!(event, planned);
            let next = project::apply(Some(current), &event).expect("description keeps project");
            register_all(tx, [Some(&payload), description_blob.as_ref()])?;
            let appended = events::append(
                tx,
                NewEvent {
                    aggregate_type: "project",
                    aggregate_id: cmd.project_id.to_string(),
                    aggregate_version: next.version,
                    event_type: event.type_name(),
                    project_id: Some(cmd.project_id),
                    actor_kind: ActorKind::User,
                    actor_id: Some(actor),
                    correlation_id: command_id.to_string(),
                    causation_id: None,
                    payload_blob_id: Some(payload.id.to_string()),
                    metadata: meta,
                    dedupe_key: format!("cmd:{command_id}:0"),
                },
            )?;
            events::set_project_description_projection(
                tx,
                cmd.project_id,
                description_blob.as_ref().map(|b| b.id.to_string()).as_deref(),
                next.version,
            )?;
            Ok(Ok((
                CommandResult::ProjectDescriptionSet { project_id: cmd.project_id, version: next.version },
                vec![notice(appended.seq, event.type_name(), cmd.project_id.to_string(), Some(cmd.project_id))],
            )))
        })
    }

    /// Shared transaction shell: receipt re-check, body, receipt insert.
    pub(crate) fn commit<F>(&self, command_id: CommandId, hash: String, body: F) -> Result<Committed, AppError>
    where
        F: FnOnce(&Transaction<'_>) -> TxResult<(CommandResult, Vec<EventNotice>)> + Send + 'static,
    {
        let id = command_id.to_string();
        let method = COMMAND_METHOD.get();
        let outcome = self
            .store
            .write(move |tx| {
                // A concurrent identical command may have committed since the fast path.
                match check_receipt(tx, &id, &hash) {
                    Ok(Some(receipt)) => return Ok(Ok(Committed { receipt, notices: vec![], followups: vec![] })),
                    Ok(None) => {}
                    Err(conflict) => return Ok(Err(conflict)),
                }
                let (result, notices) = match body(tx)? {
                    Ok(done) => done,
                    Err(rejected) => return Ok(Err(rejected)),
                };
                let committed_seq = match notices.last() {
                    Some(n) => n.seq,
                    None => queries::last_seq(tx)?,
                };
                events::insert_receipt(tx, &id, &hash, method, &serde_json::to_string(&result)?, committed_seq)?;
                Ok(Ok(Committed {
                    receipt: CommandReceipt { command_id, committed_seq, replayed: false, result },
                    notices,
                    followups: vec![],
                }))
            })
            .map_err(storage_error)?;
        outcome
    }

    /// The earlier answer of a host write (see `record_host_write`): the sequence it
    /// committed at and its stored result, when `key` was used with the same
    /// parameters. The same key with other parameters is a conflict.
    pub fn host_receipt(&self, key: CommandId, request_hash: &str) -> Result<Option<(u64, serde_json::Value)>, AppError> {
        let (key, hash) = (key.to_string(), request_hash.to_owned());
        let found = self.store.read(|conn| events::find_receipt(conn, &key)).map_err(storage_error)?;
        match found {
            None => Ok(None),
            Some(stored) if stored.request_hash != hash => Err(idempotency_conflict()),
            Some(stored) => Ok(Some((stored.committed_seq, serde_json::from_str(&stored.result_json).unwrap_or_default()))),
        }
    }

    /// Record something the host noticed by itself (an MCP login that finished in the
    /// browser): one system event of `kind` about `aggregate_id`, with `metadata`
    /// that must not hold secrets. Returns the notice to publish.
    pub fn record_host_event(&self, kind: EventKind, aggregate_id: String, metadata: serde_json::Value) -> Result<EventNotice, AppError> {
        self.store
            .write(move |tx| {
                let id = CommandId::new().to_string();
                let aggregate_type = kind.area();
                let appended = events::append(
                    tx,
                    NewEvent {
                        aggregate_type,
                        aggregate_id: aggregate_id.clone(),
                        aggregate_version: aurelius_storage::conversations::next_version(tx, aggregate_type, &aggregate_id)?,
                        event_type: kind.as_str(),
                        project_id: None,
                        actor_kind: ActorKind::System,
                        actor_id: None,
                        correlation_id: id.clone(),
                        causation_id: None,
                        payload_blob_id: None,
                        metadata,
                        dedupe_key: format!("host:{id}"),
                    },
                )?;
                Ok(notice(appended.seq, kind.as_str(), aggregate_id, None))
            })
            .map_err(storage_error)
    }

    /// Record a finished host write in one transaction: an audit event of kind
    /// `host.write` (the method and who asked, never the parameters or secrets)
    /// and the receipt with `result`. Returns the committed sequence and the notice.
    pub fn record_host_write(
        &self,
        ctx: &CommandContext,
        key: CommandId,
        request_hash: String,
        method: &str,
        result: &serde_json::Value,
    ) -> Result<(u64, EventNotice), AppError> {
        let (id, method, result_json) = (key.to_string(), method.to_owned(), result.to_string());
        let meta = serde_json::json!({ "method": method, "ctx": ctx.metadata() });
        let actor = ctx.principal_id.to_string();
        self.store
            .write(move |tx| {
                if let Some(stored) = events::find_receipt(tx, &id)? {
                    // A concurrent identical write got there first; a different one is refused.
                    if stored.request_hash != request_hash {
                        return Ok(Err(idempotency_conflict()));
                    }
                    let seq = stored.committed_seq;
                    let found = queries::events_after(tx, seq.saturating_sub(1), 1)?.into_iter().next();
                    return Ok(found.map(|n| (seq, n)).ok_or_else(|| AppError::new(ErrorCode::Internal, "corrupt receipt: its event is missing")));
                }
                let appended = events::append(
                    tx,
                    NewEvent {
                        aggregate_type: "host",
                        aggregate_id: id.clone(),
                        aggregate_version: 1,
                        event_type: EventKind::HostWrite.as_str(),
                        project_id: None,
                        actor_kind: ActorKind::User,
                        actor_id: Some(actor),
                        correlation_id: id.clone(),
                        causation_id: None,
                        payload_blob_id: None,
                        metadata: meta,
                        dedupe_key: format!("host:{id}"),
                    },
                )?;
                events::insert_receipt(tx, &id, &request_hash, &method, &result_json, appended.seq)?;
                Ok(Ok((appended.seq, notice(appended.seq, EventKind::HostWrite.as_str(), id, None))))
            })
            .map_err(storage_error)?
    }
}

fn check_receipt(tx: &Transaction<'_>, command_id: &str, hash: &str) -> Result<Option<CommandReceipt>, AppError> {
    let stored = events::find_receipt(tx, command_id).map_err(storage_error)?;
    let Some(stored) = stored else { return Ok(None) };
    if stored.request_hash != hash {
        return Err(idempotency_conflict());
    }
    let result: CommandResult = serde_json::from_str(&stored.result_json)
        .map_err(|e| AppError::new(ErrorCode::Internal, format!("corrupt receipt: {e}")))?;
    Ok(Some(CommandReceipt {
        command_id: CommandId::parse(command_id).map_err(|e| AppError::new(ErrorCode::Internal, e.to_string()))?,
        committed_seq: stored.committed_seq,
        replayed: true,
        result,
    }))
}

fn register_all<'a>(
    tx: &Transaction<'_>,
    items: impl IntoIterator<Item = Option<&'a PreparedBlob>>,
) -> Result<(), StorageError> {
    for blob in items.into_iter().flatten() {
        blobs::register(tx, blob)?;
    }
    Ok(())
}

pub(crate) fn notice(seq: u64, event_type: &str, aggregate_id: String, project_id: Option<ProjectId>) -> EventNotice {
    EventNotice {
        seq,
        kind: EventKind::parse(event_type).expect("known event kind"),
        aggregate_id,
        project_id,
        occurred_at_ms: aurelius_storage::now_ms(),
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod approvals_tests;

#[cfg(test)]
mod inbox_tests;

#[cfg(test)]
mod org_tests;

#[cfg(test)]
mod run_end_tests;

#[cfg(test)]
mod ticket_tests;

#[cfg(test)]
mod ticket_files_tests;

#[cfg(test)]
mod ticket_agent_tests;

#[cfg(test)]
mod discussion_tests;

#[cfg(test)]
mod login_tests;
#[cfg(test)]
mod employee_chat_tests;

#[cfg(test)]
mod guideline_tests;

#[cfg(test)]
mod guideline_run_tests;
#[cfg(test)]
mod server_name_tests;
