//! Every read method (`<area>.list` / `<area>.get`) as a `ResponseBody`.
//! Session methods (hello, subscribe, terminal open and detach) are answered by the session itself.

use super::{app_task, git, internal, sql, storage_task};
use crate::{Shared, VERSION};
use aurelius_domain::{ConversationId, EmployeeId, ProjectId};
use aurelius_protocol::{
    ApprovalsSnapshot, ProvidersSnapshot, ErrorCode, HealthResult, JobSummary, JobsSnapshot, EmployeesSnapshot, MemorySnapshot,
    Method, MessagesSnapshot, NotificationsSnapshot, ProjectConversationsSnapshot, ProjectsSnapshot, ProtocolError, ResponseBody, TasksSnapshot,
    TimelinePage,
};
use aurelius_storage::queries;
use base64::Engine;
use std::sync::Arc;

/// `reader`: the board member of the connection (none for an app): reading a
/// discussion (a channel, a ticket's) marks it read for them.
pub(super) async fn handle(shared: &Arc<Shared>, method: Method, reader: Option<aurelius_domain::UserId>) -> Result<ResponseBody, ProtocolError> {
    match method {
        Method::SessionHello(_)
        | Method::EventSubscribe { .. }
        | Method::TerminalOpen { .. }
        | Method::TerminalDetach { .. }
        | Method::UserLogin(_)
        | Method::UserLogout
        | Method::SessionList { .. }
        | Method::ClaudeLoginStatus
        | Method::ClaudeLoginInput(_)
        | Method::VaultList { .. }
        | Method::VaultReveal { .. }
        | Method::VaultAudit { .. }
        | Method::VaultKeyExport(_)
        | Method::UserWhoami => {
            Err(internal("a session method reached the reads"))
        }
        Method::GuidelineList { .. }
        | Method::GuidelineGet { .. }
        | Method::GuidelineVersions { .. }
        | Method::GuidelineEffective { .. }
        | Method::GuidelineProposals { .. }
        | Method::GuidelineAffected { .. }
        | Method::GuidelineStarterSet
        | Method::RunGuidelines { .. } => guidelines(shared, method).await,
        Method::ServerGetHealth => health(shared).await,
        Method::ServerGetName => Ok(ResponseBody::ServerName(super::server_name(shared).await)),
        Method::ProjectList => projects(shared).await,
        Method::JobList => jobs(shared).await,
        Method::ConversationGet { conversation_id } => {
            let body = messages(shared, conversation_id).await?;
            mark_read(shared, reader, conversation_id).await;
            Ok(body)
        }
        Method::EmployeeList => employees(shared).await,
        Method::EmployeeChat { employee_id } => {
            let store = shared.store().clone();
            let chat = storage_task(move || store.read(|c| aurelius_storage::employees::main_chat(c, employee_id))).await?;
            match chat {
                Some(conversation_id) => messages(shared, conversation_id).await,
                None => Err(ProtocolError::new(ErrorCode::NotFound, format!("employee {employee_id} does not exist"))),
            }
        }
        Method::GroupChatGet { channel } => {
            let store = shared.store().clone();
            let found = storage_task(move || {
                store.read(|c| match channel {
                    None => aurelius_storage::conversations::group_conversation(c),
                    // A channel, or a ticket's (feature `ticket_channels`; also read with `conversation.get`).
                    Some(id) => Ok(match aurelius_storage::discussions::channels(c)?.into_iter().find(|ch| ch.conversation_id == id) {
                        Some(ch) => Some(ch.conversation_id),
                        None => aurelius_storage::discussions::ticket_of_conversation(c, id)?.map(|_| id),
                    }),
                })
            })
            .await?;
            match (found, channel) {
                (Some(conversation_id), _) => {
                    let body = messages(shared, conversation_id).await?;
                    mark_read(shared, reader, conversation_id).await;
                    Ok(body)
                }
                (None, Some(_)) => Err(ProtocolError::new(ErrorCode::NotFound, "this channel does not exist")),
                (None, None) => Err(ProtocolError::new(ErrorCode::NotFound, "the group chat does not exist")),
            }
        }
        Method::ChannelList => channels(shared, reader, false, None).await,
        Method::ChannelSearch { query } => channels(shared, reader, false, Some(query)).await,
        Method::CompanyGet => company(shared).await,
        Method::BoardGet { project_id } => board(shared, project_id).await,
        Method::UserList => {
            let store = shared.store().clone();
            storage_task(move || store.read(aurelius_application::user::list)).await.map(ResponseBody::Users)
        }
        Method::TicketGet { ticket_id } => ticket(shared, ticket_id).await,
        Method::TicketList { project_id, column, query } => tickets(shared, project_id, column, query).await,
        Method::ProjectConversationList { project_id } => project_conversations(shared, project_id).await,
        Method::ApprovalList => approvals(shared).await,
        Method::ProviderList => connections(shared).await,
        Method::TaskList { project_id, employee_id, state } => tasks(shared, project_id, employee_id, state).await,
        Method::TimelineList(query) => timeline(shared, query).await,
        Method::NotificationList => notifications(shared, reader).await,
        Method::ShareList { project_id } => {
            let store = shared.store().clone();
            storage_task(move || {
                store.read(|c| {
                    let read = |key: &str| store.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
                    aurelius_storage::shares::for_project(c, project_id, &read)
                })
            })
            .await
            .map(ResponseBody::Shares)
        }
        Method::UpdateGet { check } => Ok(ResponseBody::Update(crate::updates::status(shared, check).await)),
        Method::BackupList => Ok(ResponseBody::Backups(aurelius_host::backups::snapshot(&shared.root))),
        // Only the reader's own login (B12); an app has none.
        Method::SubscriptionGet { refresh } => Ok(ResponseBody::Subscription(crate::runtimes::subscription_usage(shared, reader, refresh).await)),
        Method::UsageGet => {
            let store = shared.store().clone();
            storage_task(move || store.read(|c| aurelius_storage::usage::snapshot(c, aurelius_host::utc_offset_minutes_at))).await.map(ResponseBody::Usage)
        }
        Method::UsageGetHours { day_start_ms } => {
            let store = shared.store().clone();
            storage_task(move || store.read(|c| aurelius_storage::usage::hours(c, day_start_ms, aurelius_host::utc_offset_minutes_at))).await.map(ResponseBody::UsageHours)
        }
        Method::UsageGetTotals { conversation_id, task_id } => {
            let store = shared.store().clone();
            let totals = storage_task(move || {
                store.read(|c| match (task_id, conversation_id) {
                    (Some(task), _) => aurelius_storage::usage::task_totals(c, task).map(Some),
                    (None, Some(conversation)) => aurelius_storage::usage::conversation_totals(c, conversation).map(Some),
                    (None, None) => Ok(None),
                })
            })
            .await?;
            totals.map(ResponseBody::UsageTotals).ok_or_else(|| ProtocolError::new(ErrorCode::InvalidRequest, "Give a conversation or a task."))
        }
        Method::RunGetTrace { run_id } => {
            let s = shared.clone();
            app_task(move || s.service.run_trace(run_id))
                .await?
                .map(ResponseBody::RunTrace)
                .ok_or_else(|| ProtocolError::new(ErrorCode::NotFound, "No trace was kept for this answer: trace mode was off, or it is no longer among the latest runs."))
        }
        Method::TraceGetSetting => {
            let store = shared.store().clone();
            storage_task(move || store.read(aurelius_storage::traces::setting)).await.map(ResponseBody::TraceSetting)
        }
        Method::VariantsGetSetting => {
            let store = shared.store().clone();
            storage_task(move || store.read(aurelius_storage::variants::setting)).await.map(ResponseBody::VariantsSetting)
        }
        Method::TaskVariants { task_id } => {
            let s = shared.clone();
            app_task(move || s.service.task_variants(task_id)).await.map(ResponseBody::TaskVariants)
        }
        Method::PowerGet => Ok(ResponseBody::Power(shared.power.state())),
        Method::LoadGet => Ok(ResponseBody::Load(crate::load::snapshot(shared))),
        Method::BrowserGet => Ok(ResponseBody::Browser(aurelius_host::chrome::setting())),
        Method::ArtifactGetSetting => {
            let store = shared.store().clone();
            storage_task(move || store.read(aurelius_storage::artifacts::enabled)).await.map(|enabled| ResponseBody::ArtifactSetting(aurelius_protocol::ArtifactSetting { enabled }))
        }
        Method::AutoAllowGetSetting => {
            let store = shared.store().clone();
            storage_task(move || store.read(aurelius_storage::auto_allow::enabled)).await.map(|enabled| ResponseBody::AutoAllowSetting(aurelius_protocol::AutoAllowSetting::new(enabled)))
        }
        Method::ManagementModeGetSetting => {
            let store = shared.store().clone();
            storage_task(move || store.read(aurelius_storage::management_mode::enabled)).await.map(|enabled| ResponseBody::ManagementModeSetting(aurelius_protocol::ManagementModeSetting { enabled }))
        }
        Method::ArtifactList { conversation_id, task_id, project_id } => {
            let s = shared.clone();
            let filter = aurelius_storage::artifacts::Filter { conversation_id, task_id, project_id };
            app_task(move || s.service.artifacts(filter)).await.map(|artifacts| ResponseBody::Artifacts(aurelius_domain::views::ArtifactsSnapshot { artifacts }))
        }
        Method::ArtifactGetContent { url, version } => {
            let s = shared.clone();
            app_task(move || s.service.artifact_content(url, version)).await.map(ResponseBody::ArtifactContent)
        }
        Method::McpList => crate::mcp_servers::list(shared, reader).await.map(ResponseBody::McpServers),
        Method::McpLoginGet { login_id } => crate::mcp_oauth::get(&login_id, reader).map(ResponseBody::McpLogin),
        Method::McpAlwaysAllowList => crate::mcp_servers::always_allow_list(shared).await.map(ResponseBody::McpAlwaysAllow),
        Method::PluginList => aurelius_host::plugins::list(shared).await.map(ResponseBody::Plugins),
        Method::PluginListAvailable => aurelius_host::plugins::available().await.map(ResponseBody::PluginCatalog),
        Method::PluginGetForEmployee { employee_id } => aurelius_host::plugins::employee_get(shared, employee_id).await.map(ResponseBody::EmployeePlugins),
        Method::PluginGetForProject { project_id } => aurelius_host::plugins::project_get(shared, project_id).await.map(ResponseBody::ProjectPlugins),
        Method::MemoryList { project_id, employee_id } => {
            let store = shared.store().clone();
            storage_task(move || {
                store.read(|c| {
                    let read = |key: &str| store.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
                    let wanted = aurelius_storage::memory::Wanted { project: project_id, employee: employee_id };
                    Ok(MemorySnapshot { items: aurelius_storage::memory::list(c, wanted, &read)? })
                })
            })
            .await
            .map(ResponseBody::Memory)
        }
        Method::MemoryGetContext { project_id, employee_id } => {
            if project_id.is_none() && employee_id.is_none() {
                return Err(ProtocolError::new(ErrorCode::Validation, "Give a project or an employee."));
            }
            let store = shared.store().clone();
            storage_task(move || {
                store.read(|c| {
                    let read = |key: &str| store.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
                    aurelius_application::memory::memory_context::memory_context(c, project_id, employee_id, &read)
                })
            })
            .await
            .map(ResponseBody::MemoryContext)
        }
        Method::ScreenshotGet { id } => {
            let path = shared.root.join(aurelius_protocol::SCREENSHOT_DIR).join(format!("{id}.jpg"));
            // The id is a UUID, never a path.
            let bytes = if aurelius_protocol::valid_screenshot_id(&id) { tokio::fs::read(&path).await.ok() } else { None };
            match bytes {
                Some(bytes) => Ok(ResponseBody::Screenshot { id, jpeg_base64: base64::engine::general_purpose::STANDARD.encode(bytes) }),
                None => Err(ProtocolError::new(ErrorCode::NotFound, "This screenshot is no longer kept.")),
            }
        }
        Method::IdeaList { project_id } => {
            let s = shared.clone();
            app_task(move || s.service.ideas(project_id)).await.map(ResponseBody::Ideas)
        }
        Method::IdeaListAll {} => {
            let s = shared.clone();
            app_task(move || s.service.all_ideas()).await.map(ResponseBody::AllIdeas)
        }
        Method::NoteList { project_id, query } => {
            let s = shared.clone();
            app_task(move || s.service.notes(project_id, query)).await.map(ResponseBody::Notes)
        }
        Method::NoteGet { note_id } => {
            let s = shared.clone();
            app_task(move || s.service.note(note_id)).await.map(ResponseBody::Note)
        }
        Method::EmployeeInstructionVersions { employee_id } => {
            let s = shared.clone();
            app_task(move || s.service.employee_instruction_versions(employee_id)).await.map(ResponseBody::EmployeeInstructionVersions)
        }
        Method::ProposalList => {
            let s = shared.clone();
            app_task(move || s.service.proposals()).await.map(ResponseBody::Proposals)
        }
        Method::RecurringTaskList { project_id } => {
            let s = shared.clone();
            app_task(move || s.service.task_schedules(project_id)).await.map(ResponseBody::TaskSchedules)
        }
        Method::AgentFunctionList { project_id } => {
            let s = shared.clone();
            app_task(move || s.service.agent_functions(project_id)).await.map(ResponseBody::AgentFunctions)
        }
        Method::FunctionCallList { function_id, limit } => {
            let s = shared.clone();
            app_task(move || s.service.function_calls(function_id, limit)).await.map(ResponseBody::FunctionCalls)
        }
        Method::AppKeyList => {
            let s = shared.clone();
            app_task(move || s.service.app_keys()).await.map(ResponseBody::AppKeys)
        }
        Method::ApiKeyList => {
            let s = shared.clone();
            app_task(move || s.service.api_keys()).await.map(ResponseBody::ApiKeys)
        }
        Method::FileGet { id } => {
            let store = shared.store().clone();
            let found = storage_task(move || {
                store.read(|c| {
                    let Some((name, media_type, key)) = aurelius_storage::conversations::message_file(c, &id)? else { return Ok(None) };
                    Ok(store.blobs().read(&key).ok().map(|bytes| (id, name, media_type, bytes)))
                })
            })
            .await?;
            match found {
                Some((id, name, media_type, bytes)) => {
                    Ok(ResponseBody::File { id, name, media_type, data_base64: base64::engine::general_purpose::STANDARD.encode(bytes) })
                }
                None => Err(ProtocolError::new(ErrorCode::NotFound, "This file is no longer kept.")),
            }
        }
        Method::UploadGet { id, offset, max_side } => crate::uploads::get(shared, id, offset, max_side).await.map(ResponseBody::Upload),
        Method::RunGetContext { run_id } => {
            let store = shared.store().clone();
            let found = storage_task(move || {
                store.read(|c| {
                    Ok(sql::context_key(c, run_id)
                        .and_then(|k| store.blobs().read(&k).ok())
                        .and_then(|b| serde_json::from_slice::<aurelius_protocol::ContextView>(&b).ok()))
                })
            })
            .await?;
            found.map(ResponseBody::Context).ok_or_else(|| ProtocolError::new(ErrorCode::NotFound, "No context overview was kept for this answer."))
        }
        Method::GitGetStatus { conversation_id } => git::status(shared, conversation_id).await.map(ResponseBody::GitStatus),
        Method::GitGetDiff { conversation_id } => git::diff(shared, conversation_id).await.map(ResponseBody::GitDiff),
        Method::MarkdownList { scope } => crate::markdown::list(shared, scope).await.map(ResponseBody::MarkdownFiles),
        Method::MarkdownGet { scope, path } => crate::markdown::read(shared, scope, path).await.map(ResponseBody::MarkdownFile),
        Method::SubagentList { conversation_id } => {
            let store = shared.store().clone();
            storage_task(move || store.read(|c| aurelius_storage::employees::running_subagents(c, conversation_id))).await.map(ResponseBody::Subagents)
        }
        Method::ActivityGet { message_id, call_id } => {
            let store = shared.store().clone();
            storage_task(move || store.read(|c| aurelius_storage::conversations::activity_call(c, message_id, &call_id)))
                .await?
                .map(ResponseBody::Activity)
                .ok_or_else(|| ProtocolError::new(ErrorCode::NotFound, "This tool call is no longer kept."))
        }
        Method::RemoteGetStatus => Ok(ResponseBody::Remote(crate::remote::status(shared).await)),
        Method::FsListDir { path } => crate::browse::list(shared, path).await.map(ResponseBody::DirListing),
        Method::ProviderDetectRuntime => Ok(ResponseBody::RuntimeDetected {
            claude_code: tokio::task::spawn_blocking(aurelius_runtime::claude_code::locate).await.ok().flatten().map(|p| p.display().to_string()),
        }),
        Method::TerminalInput { terminal_id, data_base64 } => shared.terminals.input(&terminal_id, &data_base64).await.map(|()| ResponseBody::TerminalDone),
        Method::TerminalList { conversation_id } => aurelius_host::terminal::list(shared, conversation_id).await.map(ResponseBody::Terminals),
        Method::TerminalResize { terminal_id, cols, rows } => shared.terminals.resize(&terminal_id, cols, rows).map(|()| ResponseBody::TerminalDone),
        // The attachment stays until the exit has reached this session too.
        Method::TerminalClose { terminal_id } => shared.terminals.close(&terminal_id).map(|()| ResponseBody::TerminalDone),
    }
}

async fn health(shared: &Arc<Shared>) -> Result<ResponseBody, ProtocolError> {
    let store = shared.store().clone();
    let (last_seq, active_jobs) = storage_task(move || store.read(|c| Ok((queries::last_seq(c)?, queries::running_runs(c)?)))).await?;
    Ok(ResponseBody::Health(HealthResult {
        host_id: shared.host_id,
        daemon_version: VERSION.into(),
        pid: std::process::id(),
        started_at_ms: shared.started_at_ms,
        last_seq,
        active_jobs,
    }))
}

/// Snapshot read in one read transaction so `as_of_seq` matches the rows.
async fn projects(shared: &Arc<Shared>) -> Result<ResponseBody, ProtocolError> {
    let store = shared.store().clone();
    storage_task(move || {
        store.read(|conn| {
            conn.execute_batch("BEGIN DEFERRED")?;
            let result = (|| {
                let as_of_seq = queries::last_seq(conn)?;
                let read_blob = |key: &str| store.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
                let projects = queries::list_projects(conn, &read_blob)?;
                Ok(ProjectsSnapshot { as_of_seq, projects })
            })();
            conn.execute_batch("COMMIT")?;
            result
        })
    })
    .await
    .map(ResponseBody::Projects)
}

async fn jobs(shared: &Arc<Shared>) -> Result<ResponseBody, ProtocolError> {
    let store = shared.store().clone();
    let (as_of_seq, rows, names) = storage_task(move || {
        store.read(|c| {
            let rows = queries::list_jobs(c, 50)?;
            let names = sql::job_names(c, &rows);
            Ok((queries::last_seq(c)?, rows, names))
        })
    })
    .await?;
    let now = aurelius_storage::now_ms();
    let jobs = rows
        .into_iter()
        .map(|row| JobSummary {
            not_before_ms: (row.state == aurelius_domain::job::JobState::Ready && row.not_before_ms > now && row.not_before_ms < i64::MAX / 4)
                .then_some(row.not_before_ms),
            reason: (row.state == aurelius_domain::job::JobState::Ready).then(|| row.last_error_code.clone()).flatten(),
            progress_permille: match row.state {
                aurelius_domain::job::JobState::Done => 1000,
                _ => 0,
            },
            label: names.get(&row.id).cloned().unwrap_or_else(|| row.payload["label"].as_str().unwrap_or("").to_owned()),
            conversation_id: row.payload["conversation_id"].as_str().and_then(|s| ConversationId::parse(s).ok()),
            task_id: row.payload["task_id"].as_str().and_then(|s| aurelius_domain::TaskId::parse(s).ok()),
            live: (row.state == aurelius_domain::job::JobState::Leased)
                .then(|| shared.live.job(&row.id))
                .flatten()
                .map(|(started_at_ms, output_tokens)| aurelius_protocol::RunProgress { started_at_ms, output_tokens, compacting: shared.live.is_compacting(&row.id) }),
            id: row.id,
            kind: row.kind,
            state: row.state,
            created_at_ms: row.not_before_ms,
        })
        .collect();
    Ok(ResponseBody::Jobs(JobsSnapshot { as_of_seq, jobs }))
}

/// `reader` read `conversation` (when it is a discussion): it counts as read up
/// to its latest message. A failure only costs the unread count; the read still answers.
async fn mark_read(shared: &Arc<Shared>, reader: Option<aurelius_domain::UserId>, conversation: ConversationId) {
    let Some(user) = reader else { return };
    let store = shared.store().clone();
    if let Err(err) = storage_task(move || store.write(move |tx| aurelius_storage::discussions::mark_read(tx, user, conversation))).await {
        tracing::warn!(error = %err.message, %conversation, "could not mark the discussion read");
    }
}

/// Messages plus the live text of runs still streaming, so a reconnecting
/// client sees everything produced so far. Tool use comes shortened
/// (`activity.get` has a call whole), so hundreds of calls stay small.
async fn messages(shared: &Arc<Shared>, conversation_id: ConversationId) -> Result<ResponseBody, ProtocolError> {
    let store = shared.store().clone();
    let (as_of_seq, mut messages) = storage_task(move || {
        store.read(|c| {
            let read_blob = |key: &str| store.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
            let as_of_seq = queries::last_seq(c)?;
            let mut messages = aurelius_storage::conversations::list_messages(c, conversation_id, &read_blob)?;
            for message in messages.iter_mut() {
                message.activity = std::mem::take(&mut message.activity).into_iter().map(aurelius_domain::activity::preview).collect();
            }
            Ok((as_of_seq, messages))
        })
    })
    .await?;
    if let Ok(live) = shared.live_text.lock() {
        for message in messages.iter_mut() {
            if message.status == aurelius_domain::conversation::MessageStatus::Partial {
                if let Some(text) = message.run_id.and_then(|r| live.get(&r)) {
                    message.text = text.clone();
                }
            }
        }
    }
    let store = shared.store().clone();
    let (attachments, agent_mode, browser_allowed, screen_allowed, shares, run, (employee_id, project_id), read_only) = storage_task(move || {
        store.read(|c| {
            let read = |key: &str| store.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
            Ok((
                aurelius_storage::files::list_attachments(c, conversation_id)?,
                aurelius_storage::conversations::agent_mode(c, conversation_id)?,
                aurelius_storage::conversations::browser_allowed(c, conversation_id)?,
                aurelius_storage::conversations::screen_allowed(c, conversation_id)?,
                aurelius_storage::shares::for_conversation(c, conversation_id, &read)?,
                aurelius_storage::conversations::conversation_run(c, conversation_id)?,
                aurelius_storage::conversations::owner_of(c, conversation_id)?,
                aurelius_storage::conversations::is_older_employee_chat(c, conversation_id)?,
            ))
        })
    })
    .await?;
    Ok(ResponseBody::Messages(MessagesSnapshot {
        as_of_seq,
        conversation_id,
        messages,
        attachments,
        agent_mode,
        browser_allowed,
        screen_allowed,
        shares,
        run,
        employee_id,
        project_id,
        read_only,
    }))
}

/// The employees (not archived) with their heartbeats.
fn employee_list(
    c: &aurelius_storage::Conn,
    read_blob: &dyn Fn(&str) -> Option<String>,
) -> Result<Vec<aurelius_domain::views::EmployeeSummary>, aurelius_storage::StorageError> {
    let mut employees = aurelius_storage::employees::list_employees(c, read_blob)?;
    let day_start = aurelius_domain::scheduler::heartbeat::local_day_start(aurelius_storage::now_ms(), aurelius_host::utc_offset_minutes_at);
    for m in employees.iter_mut() {
        m.heartbeat = aurelius_storage::heartbeats::view(c, m.id, day_start)?;
    }
    Ok(employees)
}

async fn employees(shared: &Arc<Shared>) -> Result<ResponseBody, ProtocolError> {
    let store = shared.store().clone();
    storage_task(move || {
        store.read(|c| {
            let read_blob = |key: &str| store.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
            let employees = employee_list(c, &read_blob)?;
            Ok(EmployeesSnapshot { as_of_seq: queries::last_seq(c)?, employees })
        })
    })
    .await
    .map(ResponseBody::Employees)
}

/// The company in one read transaction, so `as_of_seq` matches the rows.
async fn company(shared: &Arc<Shared>) -> Result<ResponseBody, ProtocolError> {
    let store = shared.store().clone();
    storage_task(move || {
        store.read(|c| {
            c.execute_batch("BEGIN DEFERRED")?;
            let result = (|| {
                let read_blob = |key: &str| store.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
                let employees = employee_list(c, &read_blob)?;
                let roots = employees
                    .iter()
                    .filter(|e| e.status == aurelius_domain::employee::LifecycleStatus::Active && e.reports_to.is_none_or(|b| !employees.iter().any(|o| o.id == b)))
                    .map(|e| e.id)
                    .collect();
                Ok(aurelius_protocol::CompanySnapshot {
                    as_of_seq: queries::last_seq(c)?,
                    follow_org_chart: aurelius_storage::employees::follow_org_chart(c)?,
                    employees,
                    roots,
                    company: aurelius_storage::company::get(c)?,
                    // B6: whose Claude login runs the background jobs (memory consolidation).
                    background_login: match aurelius_storage::company::background_login_user(c)? {
                        Some(user) => aurelius_storage::users::get(c, &user.to_string())?.and_then(|u| u.view()),
                        None => None,
                    },
                })
            })();
            c.execute_batch("COMMIT")?;
            result
        })
    })
    .await
    .map(ResponseBody::Company)
}

/// The board in one read transaction, so `as_of_seq` matches the cards.
async fn board(shared: &Arc<Shared>, project_id: Option<ProjectId>) -> Result<ResponseBody, ProtocolError> {
    let store = shared.store().clone();
    storage_task(move || {
        store.read(|c| {
            c.execute_batch("BEGIN DEFERRED")?;
            let result = (|| {
                let read_blob = |key: &str| store.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
                Ok(aurelius_domain::ticket::BoardView {
                    as_of_seq: queries::last_seq(c)?,
                    project_id,
                    columns: aurelius_storage::tickets::board(c, project_id, &read_blob)?,
                })
            })();
            c.execute_batch("COMMIT")?;
            result
        })
    })
    .await
    .map(ResponseBody::Board)
}

/// The ticket and its tasks in one read transaction (as `board`): the card's
/// column and its main task's status come from the same commit, never a card
/// still in Todo with a main task that already runs.
async fn ticket(shared: &Arc<Shared>, ticket_id: aurelius_domain::TicketId) -> Result<ResponseBody, ProtocolError> {
    let store = shared.store().clone();
    let found = storage_task(move || {
        store.read(|c| {
            c.execute_batch("BEGIN DEFERRED")?;
            let result = (|| {
                let read_blob = |key: &str| store.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
                let Some(ticket) = aurelius_storage::tickets::view(c, ticket_id, &read_blob)? else { return Ok(None) };
                let mut tasks = Vec::new();
                for id in aurelius_storage::tickets::tasks_of_ticket(c, ticket_id)? {
                    tasks.extend(aurelius_storage::tasks::task_view(c, id, &read_blob)?);
                }
                let root_task = ticket.root_task_id.and_then(|root| tasks.iter().find(|t| t.id == root).cloned());
                Ok(Some(aurelius_domain::ticket::TicketDetail { as_of_seq: queries::last_seq(c)?, ticket, root_task, tasks }))
            })();
            c.execute_batch("COMMIT")?;
            result
        })
    })
    .await?;
    found
        .map(ResponseBody::Ticket)
        .ok_or_else(|| ProtocolError::new(ErrorCode::NotFound, format!("ticket {ticket_id} does not exist")))
}

async fn tickets(
    shared: &Arc<Shared>,
    project: Option<ProjectId>,
    column: Option<aurelius_domain::ticket::ColumnKind>,
    query: Option<String>,
) -> Result<ResponseBody, ProtocolError> {
    let store = shared.store().clone();
    storage_task(move || {
        store.read(|c| {
            let read_blob = |key: &str| store.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
            let filter = aurelius_storage::tickets::TicketFilter { project, column, query, ..Default::default() };
            Ok(aurelius_domain::ticket::TicketsSnapshot { as_of_seq: queries::last_seq(c)?, tickets: aurelius_storage::tickets::list(c, &filter, &read_blob)? })
        })
    })
    .await
    .map(ResponseBody::Tickets)
}

async fn project_conversations(shared: &Arc<Shared>, project_id: ProjectId) -> Result<ResponseBody, ProtocolError> {
    let store = shared.store().clone();
    storage_task(move || {
        store.read(|c| {
            Ok(ProjectConversationsSnapshot {
                as_of_seq: queries::last_seq(c)?,
                project_id,
                conversations: aurelius_storage::conversations::project_conversations(c, project_id)?,
            })
        })
    })
    .await
    .map(ResponseBody::ProjectConversations)
}

async fn approvals(shared: &Arc<Shared>) -> Result<ResponseBody, ProtocolError> {
    let store = shared.store().clone();
    storage_task(move || {
        store.read(|c| {
            let read_blob = |key: &str| store.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
            Ok(ApprovalsSnapshot { as_of_seq: queries::last_seq(c)?, approvals: aurelius_storage::approvals::list_approvals(c, &read_blob)? })
        })
    })
    .await
    .map(ResponseBody::Approvals)
}

async fn connections(shared: &Arc<Shared>) -> Result<ResponseBody, ProtocolError> {
    let store = shared.store().clone();
    storage_task(move || {
        store.read(|c| {
            let has_secret = |r: &str| matches!(aurelius_host::secrets::get(r), Ok(Some(_)));
            Ok(ProvidersSnapshot {
                as_of_seq: queries::last_seq(c)?,
                connections: aurelius_storage::connections::list(c, &has_secret)?,
                defaults: aurelius_storage::connections::list_defaults(c)?,
            })
        })
    })
    .await
    .map(ResponseBody::Connections)
}

async fn timeline(shared: &Arc<Shared>, query: aurelius_protocol::TimelineQuery) -> Result<ResponseBody, ProtocolError> {
    let store = shared.store().clone();
    let limit = query.limit.clamp(1, 200);
    storage_task(move || {
        store.read(|c| {
            let (cards, next_before_seq) =
                aurelius_storage::timeline::cards(c, query.project_id, query.correlation_id.as_deref(), query.before_seq, limit, query.activity)?;
            Ok(TimelinePage { cards, next_before_seq })
        })
    })
    .await
    .map(ResponseBody::Timeline)
}

/// The Inbox of `reader`: what is for everyone and their own (e.g. "connect your Claude login").
async fn notifications(shared: &Arc<Shared>, reader: Option<aurelius_domain::UserId>) -> Result<ResponseBody, ProtocolError> {
    let store = shared.store().clone();
    let user = reader.map(|u| u.to_string());
    storage_task(move || {
        store.read(|c| {
            Ok(NotificationsSnapshot {
                notifications: aurelius_storage::timeline::list_notifications_for(c, 100, user.as_deref())?,
                unread: aurelius_storage::timeline::unread_count_for(c, user.as_deref())?,
            })
        })
    })
    .await
    .map(ResponseBody::Notifications)
}

async fn tasks(
    shared: &Arc<Shared>,
    project_id: Option<ProjectId>,
    employee_id: Option<EmployeeId>,
    state: Option<aurelius_protocol::method::TaskListState>,
) -> Result<ResponseBody, ProtocolError> {
    let store = shared.store().clone();
    storage_task(move || {
        store.read(|c| {
            let read_blob = |key: &str| store.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
            let tasks = aurelius_storage::tasks::list_tasks(c, project_id, &read_blob)?
                .into_iter()
                .filter(|t| employee_id.is_none() || t.employee_id == employee_id)
                .filter(|t| state.is_none_or(|s| s.matches(t.status, t.waiting_reason.as_deref())))
                .collect();
            Ok(TasksSnapshot { as_of_seq: queries::last_seq(c)?, tasks })
        })
    })
    .await
    .map(ResponseBody::Tasks)
}

/// One guideline read on a read connection: its answer, or a refusal (not found).
type GuidelineRead = Box<dyn FnOnce(&rusqlite::Connection) -> Result<Result<ResponseBody, ProtocolError>, aurelius_storage::StorageError> + Send>;

/// The guideline reads (feature `guidelines`): board members' clients only, never apps (SEC0 R-d).
async fn guidelines(shared: &Arc<Shared>, method: Method) -> Result<ResponseBody, ProtocolError> {
    use aurelius_domain::guideline as gl;
    use aurelius_storage::guidelines as store;
    let not_found = |id: aurelius_domain::GuidelineId| ProtocolError::new(ErrorCode::NotFound, format!("guideline {id} does not exist"));
    let store_handle = shared.store().clone();
    let read = move |f: GuidelineRead| async move {
        storage_task(move || store_handle.read(f)).await?
    };
    match method {
        Method::GuidelineList { archived } => {
            read(Box::new(move |c| {
                let guidelines = store::list(c, archived)?.into_iter().filter(|g| g.archived == archived).collect();
                Ok(Ok(ResponseBody::Guidelines(gl::GuidelinesSnapshot { guidelines })))
            }))
            .await
        }
        Method::GuidelineGet { guideline_id, version } => {
            read(Box::new(move |c| {
                let (Some(guideline), Some(found)) = (store::view(c, guideline_id)?, store::get(c, guideline_id, version)?) else {
                    return Ok(Err(match version {
                        Some(v) => ProtocolError::new(ErrorCode::NotFound, format!("guideline {guideline_id} has no version {v}")),
                        None => not_found(guideline_id),
                    }));
                };
                Ok(Ok(ResponseBody::Guideline(gl::GuidelineDetail { guideline, version: found })))
            }))
            .await
        }
        Method::GuidelineVersions { guideline_id } => {
            read(Box::new(move |c| {
                if store::view(c, guideline_id)?.is_none() {
                    return Ok(Err(not_found(guideline_id)));
                }
                Ok(Ok(ResponseBody::GuidelineVersions(gl::GuidelineVersions { guideline_id, versions: store::versions(c, guideline_id)? })))
            }))
            .await
        }
        // What a run of this employee in this project gets, after the safety net (B4).
        Method::GuidelineEffective { employee_id, project_id } => {
            read(Box::new(move |c| Ok(Ok(ResponseBody::EffectiveGuidelines(gl::apply_safety_net(store::effective(c, employee_id, project_id)?)))))).await
        }
        Method::GuidelineProposals { status } => {
            read(Box::new(move |c| Ok(Ok(ResponseBody::GuidelineProposals(gl::GuidelineProposals { proposals: store::proposals(c, status)? }))))).await
        }
        Method::GuidelineAffected { guideline_id } => {
            read(Box::new(move |c| {
                let Some(view) = store::view(c, guideline_id)? else { return Ok(Err(not_found(guideline_id))) };
                let employees = store::affected_employees(c, guideline_id)?.len() as u32;
                let mut projects = 0;
                for target in &view.links {
                    if let gl::GuidelineTarget::Project(p) = target {
                        let status: Option<String> = c.query_row("SELECT status FROM projects WHERE id = ?1", [p.to_string()], |r| r.get(0)).ok();
                        projects += u32::from(status.is_some_and(|s| s != "archived"));
                    }
                }
                let conversations = match aurelius_application::guideline::sessions_holding(c, guideline_id) {
                    Ok(n) => n,
                    Err(err) => return Ok(Err(err)),
                };
                Ok(Ok(ResponseBody::GuidelineAffected(gl::GuidelineAffected { guideline_id, employees, projects, conversations })))
            }))
            .await
        }
        Method::GuidelineStarterSet => {
            read(Box::new(move |c| {
                let mut templates = Vec::new();
                for starter in store::starter_set() {
                    templates.push(gl::StarterTemplate {
                        key: starter.key.to_owned(),
                        title: starter.title.to_owned(),
                        text: starter.text.to_owned(),
                        chars: starter.text.chars().count() as u32,
                        level: starter.level,
                        in_library: store::title_taken(c, starter.title, None)?,
                    });
                }
                Ok(Ok(ResponseBody::StarterSet(gl::StarterSet { templates })))
            }))
            .await
        }
        Method::RunGuidelines { run_id } => {
            read(Box::new(move |c| {
                let known: bool = c.query_row("SELECT EXISTS(SELECT 1 FROM runs WHERE id = ?1)", [run_id.to_string()], |r| r.get(0))?;
                if !known {
                    return Ok(Err(ProtocolError::new(ErrorCode::NotFound, format!("run {run_id} does not exist"))));
                }
                Ok(Ok(ResponseBody::RunGuidelines(gl::RunGuidelines { run_id, guidelines: store::for_run(c, run_id)? })))
            }))
            .await
        }
        _ => Err(internal("not a guideline read")),
    }
}

/// `channel.list` and `channel.search { query }`: General, the project channels
/// and (`ticket_channels`: the connection accepts them) the ticket channels,
/// open ones first, newest ticket first. A query keeps the channels whose name,
/// ticket title or ticket number matches it. A connection without `ticket_channels` gets exactly
/// the list of before: no ticket channels and none of the new fields.
pub(super) async fn channels(shared: &Arc<Shared>, reader: Option<aurelius_domain::UserId>, ticket_channels: bool, query: Option<String>) -> Result<ResponseBody, ProtocolError> {
    use aurelius_domain::views::{ChannelKind, ChannelView, ChannelsSnapshot, ticket_channel_name};
    use aurelius_storage::discussions::{self, Member};
    let store = shared.store().clone();
    let snapshot = storage_task(move || {
        store.read(|c| {
            let as_of_seq = queries::last_seq(c)?;
            let read_text = |key: &str| store.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
            // Unread and mentions are the reader's own; none without a board member.
            let reader = match reader {
                Some(user) => aurelius_storage::users::get(c, &user.to_string())?.map(|u| (user, u.username)),
                None => None,
            };
            let last = |id: ConversationId| c.query_row("SELECT MAX(created_at_ms) FROM messages WHERE conversation_id = ?1", [id.to_string()], |r| r.get::<_, Option<i64>>(0));
            let mut channels = Vec::new();
            for ch in discussions::channels(c)? {
                let name = if ch.project_id.is_none() { "General".to_owned() } else { ch.name };
                if !query.as_deref().is_none_or(|q| channel_matches(&name, None, None, q)) {
                    continue;
                }
                let (unread, mentions) = match &reader {
                    Some((user, username)) => discussions::unread(c, *user, username, ch.conversation_id, &read_text)?,
                    None => (0, 0),
                };
                let kind = if ch.project_id.is_none() { ChannelKind::General } else { ChannelKind::Project };
                let read_through = match &reader {
                    Some((user, _)) => discussions::read_through(c, *user, ch.conversation_id)?,
                    None => None,
                };
                // Every board member is in General and the project channels.
                channels.push(ChannelView {
                    conversation_id: ch.conversation_id, project_id: ch.project_id, name, unread, mentions, archived: ch.read_only, last_activity_ms: last(ch.conversation_id)?,
                    kind, ticket_id: None, ticket_number: None, ticket_column: None, member: reader.is_some(), member_count: None, read_through,
                });
            }
            if ticket_channels {
                let mut tickets = discussions::ticket_channels(c)?;
                // Open channels first; within each, the newest ticket first (as listed).
                tickets.sort_by_key(|t| t.archived);
                for t in tickets {
                    let name = ticket_channel_name(t.number, &t.title);
                    if !query.as_deref().is_none_or(|q| channel_matches(&name, Some(&t.title), Some(t.number), q)) {
                        continue;
                    }
                    let (unread, mentions, read_through, member) = match &reader {
                        Some((user, username)) => {
                            let (unread, mentions) = discussions::unread(c, *user, username, t.conversation_id, &read_text)?;
                            let member = discussions::members(c, t.conversation_id)?.contains(&Member::User(*user));
                            (unread, mentions, discussions::read_through(c, *user, t.conversation_id)?, member)
                        }
                        None => (0, 0, None, false),
                    };
                    channels.push(ChannelView {
                        conversation_id: t.conversation_id, project_id: Some(t.project_id), name, unread, mentions,
                        archived: t.archived, last_activity_ms: last(t.conversation_id)?, kind: ChannelKind::Ticket, ticket_id: Some(t.ticket_id),
                        ticket_number: Some(t.number), ticket_column: Some(t.column), member, member_count: Some(t.member_count), read_through,
                    });
                }
            }
            Ok(ChannelsSnapshot { as_of_seq, channels })
        })
    })
    .await?;
    let channels = snapshot.channels.into_iter();
    let channels = if ticket_channels { channels.collect() } else { channels.map(as_before).collect() };
    Ok(ResponseBody::Channels(ChannelsSnapshot { channels, ..snapshot }))
}

/// A channel as a connection without `ticket_channels` gets it: without the
/// fields that feature adds, so its list is exactly the one of before.
fn as_before(channel: aurelius_domain::views::ChannelView) -> aurelius_domain::views::ChannelView {
    aurelius_domain::views::ChannelView { kind: Default::default(), ticket_id: None, ticket_number: None, ticket_column: None, member: false, member_count: None, read_through: None, ..channel }
}

/// Whether `query` finds a channel (`channel.search`): `#12` or `12` its ticket
/// number, other text its name or its ticket's whole title (case not counting,
/// and a space the same as the `-` between the words of a name, so "fix the"
/// finds `#12-fix-the-login` and words past the name's cut are still found).
/// An empty query finds all.
fn channel_matches(name: &str, title: Option<&str>, number: Option<u32>, query: &str) -> bool {
    let query = query.trim();
    if query.is_empty() {
        return true;
    }
    let digits = query.strip_prefix('#').unwrap_or(query);
    if let (Ok(n), Some(own)) = (digits.parse::<u32>(), number)
        && n == own
    {
        return true;
    }
    let query = search_words(query);
    search_words(name).contains(&query) || title.is_some_and(|t| search_words(t).contains(&query))
}

/// `text` in lowercase with every run of spaces and `-` as one space, for `channel_matches`.
fn search_words(text: &str) -> String {
    text.to_lowercase().split(|c: char| c.is_whitespace() || c == '-').filter(|w| !w.is_empty()).collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod channel_search_tests {
    use super::channel_matches;

    #[test]
    fn several_words_find_a_ticket_channel_by_its_name_or_its_whole_title() {
        let title = "Fix the login page after the password reset fails on Safari";
        let name = aurelius_domain::views::ticket_channel_name(12, title);
        assert_eq!(name, "#12-fix-the-login-page-after-the");
        assert!(channel_matches(&name, Some(title), Some(12), "fix the"));
        assert!(channel_matches(&name, Some(title), Some(12), "Login  Page"));
        assert!(channel_matches(&name, Some(title), Some(12), "fix-the-login"));
        // Past the 32-character cut of the name: only the title has these words.
        assert!(channel_matches(&name, Some(title), Some(12), "password reset fails"));
        assert!(channel_matches(&name, Some(title), Some(12), "ON SAFARI"));
        assert!(!channel_matches(&name, None, Some(12), "password reset"));
        assert!(!channel_matches(&name, Some(title), Some(12), "reset the password"));
        assert!(channel_matches(&name, Some(title), Some(12), "#12"));
        assert!(!channel_matches("General", None, None, "fix the"));
        assert!(channel_matches("General", None, None, "  "));
    }
}
