//! Every read method (`<area>.list` / `<area>.get`) as a `ResponseBody`.
//! Session methods (hello, subscribe) are answered by the session itself.

use super::{app_task, internal, sql, storage_task};
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

/// `reader`: the board member of the connection : reading a
/// discussion (a channel, a ticket's) marks it read for them.
pub(super) async fn handle(shared: &Arc<Shared>, method: Method, reader: Option<aurelius_domain::UserId>) -> Result<ResponseBody, ProtocolError> {
    match method {
        Method::SessionHello(_)
        | Method::EventSubscribe { .. }
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
            let store = shared.store().clone();
            if storage_task(move || store.read(|c| aurelius_storage::employee_chats::hidden_from(c, conversation_id, reader))).await? {
                return Err(ProtocolError::new(ErrorCode::NotFound, format!("conversation {conversation_id} does not exist")));
            }
            let body = messages(shared, conversation_id, reader).await?;
            mark_read(shared, reader, conversation_id).await;
            Ok(body)
        }
        Method::EmployeeList => employees(shared).await,
        Method::EmployeeChat { employee_id } => {
            let store = shared.store().clone();
            let chat = storage_task(move || store.read(|c| aurelius_storage::employees::main_chat(c, employee_id))).await?;
            match chat {
                Some(conversation_id) => messages(shared, conversation_id, reader).await,
                None => Err(ProtocolError::new(ErrorCode::NotFound, format!("employee {employee_id} does not exist"))),
            }
        }
        Method::EmployeeChats => employee_chats(shared, reader).await,
        Method::ScheduledMessageList { project_id } => scheduled_messages(shared, reader, project_id).await,
        Method::ThreadWaiting { ticket_id, conversation_id } => thread_waiting(shared, reader, ticket_id, conversation_id).await,
        Method::EmployeeScratchpad { employee_id } => employee_scratchpad(shared, employee_id).await,
        Method::EmployeeQueue { employee_id } => employee_queue(shared, reader, employee_id).await,
        Method::ActivityOverview => activity_overview(shared, reader).await,
        Method::ActivityLogList { project_id, employee_id, thread_key, ticket_number, run_id, category, before_seq, limit } => {
            let filter = aurelius_storage::activity_log::Filter { project: project_id, employee: employee_id, thread: thread_key, ticket_number, run: run_id, category, before_seq };
            activity_log(shared, reader, filter, limit).await
        }
        Method::EmployeeToolOptions => Ok(ResponseBody::EmployeeToolOptions(employee_tool_options())),
        Method::EmployeeStartProfiles => {
            let store = shared.store().clone();
            let profiles = storage_task(move || store.read(aurelius_storage::employee_profiles::list)).await?;
            Ok(ResponseBody::EmployeeStartProfiles(aurelius_domain::employee_tools::StartProfiles { profiles }))
        }
        Method::EmployeeRunOverview => {
            let store = shared.store().clone();
            Ok(ResponseBody::EmployeeRuns(storage_task(move || store.read(aurelius_application::run::connections::employee_runs)).await?))
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
                    let body = messages(shared, conversation_id, reader).await?;
                    mark_read(shared, reader, conversation_id).await;
                    Ok(body)
                }
                (None, Some(_)) => Err(ProtocolError::new(ErrorCode::NotFound, "this channel does not exist")),
                (None, None) => Err(ProtocolError::new(ErrorCode::NotFound, "the group chat does not exist")),
            }
        }
        Method::ChannelList => channels(shared, reader, false, None).await,
        Method::ChannelSearch { query } => channels(shared, reader, false, Some(query)).await,
        Method::ThreadRead { message_id } => thread(shared, reader, message_id).await,
        Method::DiscussionAudience { channel } => {
            let store = shared.store().clone();
            let found = storage_task(move || store.read(|c| aurelius_application::conversation::discussion::discussion_audience(c, channel, reader))).await?;
            found.map(ResponseBody::DiscussionAudience).ok_or_else(|| ProtocolError::new(ErrorCode::NotFound, "this channel does not exist"))
        }
        Method::CompanyGet => company(shared).await,
        Method::BoardGet { project_id } => board(shared, project_id).await,
        Method::UserList => {
            let store = shared.store().clone();
            storage_task(move || store.read(aurelius_application::user::list)).await.map(ResponseBody::Users)
        }
        Method::TicketGet { ticket_id } => ticket(shared, ticket_id).await,
        Method::TicketList { project_id, column, query } => tickets(shared, project_id, column, query).await,
        Method::MessageLocate { message_id } => message_location(shared, message_id).await,
        Method::TicketFollowers { ticket_id } => ticket_followers(shared, ticket_id).await,
        Method::TicketMyTickets { project_id, include_done } => my_tickets(shared, reader, project_id, include_done).await,
        Method::MessageHistory { message_id } => message_history(shared, reader, message_id).await,
        Method::ProjectConversationList { project_id } => project_conversations(shared, project_id).await,
        Method::ApprovalList { project_id } => approvals(shared, project_id).await,
        Method::ProviderList => connections(shared).await,
        Method::TaskList { project_id, employee_id, state } => tasks(shared, project_id, employee_id, state).await,
        Method::TimelineList(query) => timeline(shared, query).await,
        Method::NotificationList { project_id } => notifications(shared, reader, project_id).await,
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
        Method::UsageGetTotals { conversation_id, task_id, ticket_id } => {
            let store = shared.store().clone();
            let totals = storage_task(move || {
                store.read(|c| match (ticket_id, task_id, conversation_id) {
                    (Some(ticket), _, _) => aurelius_storage::usage::ticket_totals(c, ticket).map(Some),
                    (None, Some(task), _) => aurelius_storage::usage::task_totals(c, task).map(Some),
                    (None, None, Some(conversation)) => aurelius_storage::usage::conversation_totals(c, conversation).map(Some),
                    (None, None, None) => Ok(None),
                })
            })
            .await?;
            totals.map(ResponseBody::UsageTotals).ok_or_else(|| ProtocolError::new(ErrorCode::InvalidRequest, "Give a ticket, a conversation or a task."))
        }
        Method::RunGetActivity { run_id } => {
            let store = shared.store().clone();
            storage_task(move || store.read(|c| aurelius_storage::conversations::activity_for_run(c, run_id))).await.map(ResponseBody::RunActivity)
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
        Method::PowerGet => Ok(ResponseBody::Power(shared.power.state())),
        Method::LoadGet => Ok(ResponseBody::Load(crate::load::snapshot(shared))),
        Method::BrowserGet => Ok(ResponseBody::Browser(aurelius_host::chrome::setting())),
        Method::AutoAllowGetSetting => {
            let store = shared.store().clone();
            storage_task(move || store.read(aurelius_storage::auto_allow::enabled)).await.map(|enabled| ResponseBody::AutoAllowSetting(aurelius_protocol::AutoAllowSetting::new(enabled)))
        }
        Method::ManagementModeGetSetting => {
            let store = shared.store().clone();
            storage_task(move || store.read(aurelius_storage::management_mode::enabled)).await.map(|enabled| ResponseBody::ManagementModeSetting(aurelius_protocol::ManagementModeSetting { enabled }))
        }
        Method::McpList => crate::mcp_servers::list(shared, reader).await.map(ResponseBody::McpServers),
        Method::McpLoginGet { login_id } => crate::mcp_oauth::get(&login_id, reader).map(ResponseBody::McpLogin),
        Method::McpAlwaysAllowList => crate::mcp_servers::always_allow_list(shared).await.map(ResponseBody::McpAlwaysAllow),
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

/// The chats of `reader` with employees; none without a board member.
async fn employee_chats(shared: &Arc<Shared>, reader: Option<aurelius_domain::UserId>) -> Result<ResponseBody, ProtocolError> {
    let store = shared.store().clone();
    let (as_of_seq, chats) = storage_task(move || {
        store.read(|c| {
            let chats = match reader {
                Some(user) => aurelius_storage::employee_chats::list(c, user, |key| store.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok()))?,
                None => Vec::new(),
            };
            Ok((queries::last_seq(c)?, chats))
        })
    })
    .await?;
    Ok(ResponseBody::EmployeeChats(aurelius_domain::views::EmployeeChatsSnapshot { as_of_seq, chats }))
}

/// What employees scheduled, as far as `reader` may see it.
async fn scheduled_messages(shared: &Arc<Shared>, reader: Option<aurelius_domain::UserId>, project: Option<ProjectId>) -> Result<ResponseBody, ProtocolError> {
    let store = shared.store().clone();
    let (as_of_seq, messages) = storage_task(move || store.read(|c| Ok((queries::last_seq(c)?, aurelius_storage::scheduled::list(c, reader, project, 100)?)))).await?;
    Ok(ResponseBody::ScheduledMessages(aurelius_domain::views::ScheduledMessagesSnapshot { as_of_seq, messages }))
}

/// Who waits to pick up a thread and what is scheduled in it (U3); a private chat
/// of someone else does not exist for `reader`.
async fn thread_waiting(shared: &Arc<Shared>, reader: Option<aurelius_domain::UserId>, ticket: Option<aurelius_domain::TicketId>, conversation: Option<ConversationId>) -> Result<ResponseBody, ProtocolError> {
    let store = shared.store().clone();
    let found = storage_task(move || {
        store.read(|c| {
            if let Some(conversation) = conversation
                && aurelius_storage::employee_chats::hidden_from(c, conversation, reader)?
            {
                return Ok(None);
            }
            let Some(key) = aurelius_storage::inbox::thread_of_place(c, ticket, conversation)? else { return Ok(None) };
            Ok(Some((queries::last_seq(c)?, aurelius_storage::inbox::waiters(c, &key, aurelius_storage::now_ms())?, aurelius_storage::scheduled::of_thread(c, &key, reader)?)))
        })
    })
    .await?;
    let Some((as_of_seq, waiters, scheduled)) = found else {
        return Err(ProtocolError::new(ErrorCode::NotFound, "this thread does not exist"));
    };
    Ok(ResponseBody::ThreadWaiting(aurelius_domain::views::ThreadWaitingSnapshot { as_of_seq, waiters, scheduled }))
}

/// The switchable tools, with the access levels that have them (C2).
fn employee_tool_options() -> aurelius_domain::employee_tools::ToolOptions {
    use aurelius_domain::employee::AccessLevel;
    use aurelius_domain::employee_tools::{BRIDGE_TOOL_PREFIX, SWITCHABLE_TOOLS, ToolOption, ToolOptions};
    let levels = [AccessLevel::Inherit, AccessLevel::ReadEdit, AccessLevel::ReadOnly];
    let tools = SWITCHABLE_TOOLS
        .iter()
        .map(|t| ToolOption {
            name: t.name.to_owned(),
            label: t.label.to_owned(),
            group: t.group.to_owned(),
            // The bridge's tools stay at every level.
            levels: levels.into_iter().filter(|l| t.name.starts_with(BRIDGE_TOOL_PREFIX) || aurelius_runtime::claude_code::tools_for(*l).contains(&t.name)).collect(),
        })
        .collect();
    ToolOptions { tools }
}

async fn employee_scratchpad(shared: &Arc<Shared>, employee: aurelius_domain::EmployeeId) -> Result<ResponseBody, ProtocolError> {
    let store = shared.store().clone();
    let row = storage_task(move || store.read(|c| aurelius_storage::scratchpads::get(c, employee))).await?;
    let (text, updated_at_ms) = row.map_or((String::new(), None), |(text, at)| (text, Some(at)));
    Ok(ResponseBody::EmployeeScratchpad(aurelius_domain::views::ScratchpadView { employee_id: employee, text, updated_at_ms }))
}

/// What is in an employee's mailbox now, as far as `reader` may see it (U6).
async fn employee_queue(shared: &Arc<Shared>, reader: Option<aurelius_domain::UserId>, employee_id: aurelius_domain::EmployeeId) -> Result<ResponseBody, ProtocolError> {
    let store = shared.store().clone();
    let (as_of_seq, items) = storage_task(move || store.read(|c| Ok((queries::last_seq(c)?, aurelius_storage::inbox::queue_of(c, employee_id, reader, aurelius_storage::now_ms())?)))).await?;
    Ok(ResponseBody::EmployeeQueue(aurelius_domain::views::EmployeeQueue { as_of_seq, employee_id, items }))
}

/// Who is busy and what waits; another board member's private chat stays a count (ticket #50).
async fn activity_overview(shared: &Arc<Shared>, reader: Option<aurelius_domain::UserId>) -> Result<ResponseBody, ProtocolError> {
    let store = shared.store().clone();
    let overview = storage_task(move || store.read(|c| aurelius_storage::activity::overview(c, reader, aurelius_storage::now_ms()))).await?;
    Ok(ResponseBody::ActivityOverview(overview))
}

/// How many rows of the activity log one page has at most, and without asking.
const ACTIVITY_LOG_MAX: u32 = 200;
const ACTIVITY_LOG_PAGE: u32 = 100;

/// The activity log, as far as `reader` may see it (R8).
async fn activity_log(shared: &Arc<Shared>, reader: Option<aurelius_domain::UserId>, filter: aurelius_storage::activity_log::Filter, limit: Option<u32>) -> Result<ResponseBody, ProtocolError> {
    let store = shared.store().clone();
    let limit = limit.unwrap_or(ACTIVITY_LOG_PAGE).clamp(1, ACTIVITY_LOG_MAX);
    let (as_of_seq, (entries, has_more)) = storage_task(move || store.read(|c| Ok((queries::last_seq(c)?, aurelius_storage::activity_log::list(c, reader, &filter, limit)?)))).await?;
    Ok(ResponseBody::ActivityLog(aurelius_domain::views::ActivityLogPage { as_of_seq, entries, has_more }))
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
/// (`activity.get` has a call whole), so hundreds of calls stay small. The
/// threads' counters and the reactions are as `reader` sees them (feature
/// `chat_threads` / `chat_reactions`); the replies are only in `thread.read`.
async fn messages(shared: &Arc<Shared>, conversation_id: ConversationId, reader: Option<aurelius_domain::UserId>) -> Result<ResponseBody, ProtocolError> {
    let store = shared.store().clone();
    let (as_of_seq, mut messages) = storage_task(move || {
        store.read(|c| {
            let read_blob = |key: &str| store.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
            let as_of_seq = queries::last_seq(c)?;
            let mut messages = aurelius_storage::conversations::list_messages(c, conversation_id, &read_blob)?;
            aurelius_storage::threads::decorate(c, conversation_id, reader, &mut messages)?;
            aurelius_storage::message_edits::mark_editable(c, conversation_id, reader, &mut messages)?;
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
    // What wakes an employee about a ticket is for the employee; the board member reads its update.
    messages.retain(|m| m.update.as_deref() != Some(aurelius_domain::conversation::TICKET_UPDATE_KIND));
    let store = shared.store().clone();
    let (attachments, agent_mode, browser_allowed, screen_allowed, run, (employee_id, project_id), read_only) = storage_task(move || {
        store.read(|c| {
            Ok((
                aurelius_storage::files::list_attachments(c, conversation_id)?,
                aurelius_storage::conversations::agent_mode(c, conversation_id)?,
                aurelius_storage::conversations::browser_allowed(c, conversation_id)?,
                aurelius_storage::conversations::screen_allowed(c, conversation_id)?,
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
        run,
        employee_id,
        project_id,
        read_only,
    }))
}

async fn employees(shared: &Arc<Shared>) -> Result<ResponseBody, ProtocolError> {
    let store = shared.store().clone();
    storage_task(move || {
        store.read(|c| {
            let read_blob = |key: &str| store.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
            let employees = aurelius_storage::employees::list_employees(c, &read_blob)?;
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
                let employees = aurelius_storage::employees::list_employees(c, &read_blob)?;
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

/// `ticket.followers`: who follows a ticket, also when it is closed or archived.
async fn ticket_followers(shared: &Arc<Shared>, ticket_id: aurelius_domain::TicketId) -> Result<ResponseBody, ProtocolError> {
    let store = shared.store().clone();
    let followers = storage_task(move || {
        store.read(|c| match aurelius_storage::tickets::state(c, ticket_id)? {
            Some(_) => Ok(Some(aurelius_storage::followers::list(c, ticket_id)?)),
            None => Ok(None),
        })
    })
    .await?;
    followers
        .map(|followers| ResponseBody::TicketFollowers(aurelius_domain::ticket::TicketFollowersView { followers }))
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

async fn approvals(shared: &Arc<Shared>, project: Option<ProjectId>) -> Result<ResponseBody, ProtocolError> {
    let store = shared.store().clone();
    storage_task(move || {
        store.read(|c| {
            let read_blob = |key: &str| store.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
            Ok(ApprovalsSnapshot { as_of_seq: queries::last_seq(c)?, approvals: aurelius_storage::approvals::list_approvals(c, project, &read_blob)? })
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
async fn notifications(shared: &Arc<Shared>, reader: Option<aurelius_domain::UserId>, project: Option<ProjectId>) -> Result<ResponseBody, ProtocolError> {
    let store = shared.store().clone();
    let user = reader.map(|u| u.to_string());
    storage_task(move || {
        store.read(|c| {
            Ok(NotificationsSnapshot {
                notifications: aurelius_storage::timeline::list_notifications_for(c, 100, user.as_deref(), project)?,
                unread: aurelius_storage::timeline::unread_count_for(c, user.as_deref(), project)?,
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

/// The guideline reads (feature `guidelines`): board members' clients only (SEC0 R-d).
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

/// `ticket.my_tickets`: the tickets of the board member of the session, never of
/// one named in a param. An app has none, and so no tickets of its own.
async fn my_tickets(shared: &Arc<Shared>, reader: Option<aurelius_domain::UserId>, project: Option<ProjectId>, include_done: bool) -> Result<ResponseBody, ProtocolError> {
    let Some(viewer) = reader else { return Err(ProtocolError::new(ErrorCode::BoardMemberRequired, "My tickets are those of a board member.")) };
    let store = shared.store().clone();
    storage_task(move || {
        store.read(|c| {
            let read_blob = |key: &str| store.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
            aurelius_storage::tickets::my_tickets(c, viewer, project, include_done, &read_blob)
        })
    })
    .await
    .map(ResponseBody::MyTickets)
}

/// `message.history { message_id }`: the earlier texts of an edited message of a
/// channel or a ticket's discussion, newest first, each through the vault
/// redactor. A deleted message answers `deleted` with no versions at all: its
/// text stays in the audit log and backups, not in any read.
async fn message_history(shared: &Arc<Shared>, reader: Option<aurelius_domain::UserId>, message: aurelius_domain::MessageId) -> Result<ResponseBody, ProtocolError> {
    use aurelius_domain::views::{MessageHistoryView, MessageVersionView};
    if reader.is_none() {
        return Err(ProtocolError::new(ErrorCode::BoardMemberRequired, "The history of a message is for a board member."));
    }
    let store = shared.store().clone();
    let found = storage_task(move || {
        store.read(|c| {
            let Some(deleted) = aurelius_storage::message_edits::deleted(c, message)? else { return Ok(None) };
            if deleted {
                return Ok(Some(MessageHistoryView { versions: Vec::new(), deleted }));
            }
            let versions = aurelius_storage::message_edits::earlier_texts(c, message)?
                .into_iter()
                .filter_map(|(key, replaced_at_ms)| {
                    let text = String::from_utf8(store.blobs().read(&key).ok()?).ok()?;
                    Some(MessageVersionView { text: aurelius_runtime::redact::text(&text).into_owned(), replaced_at_ms })
                })
                .collect();
            Ok(Some(MessageHistoryView { versions, deleted }))
        })
    })
    .await?;
    found.map(ResponseBody::MessageHistory).ok_or_else(|| ProtocolError::new(ErrorCode::NotFound, "This message does not exist."))
}

/// `message.locate { message_id }`: where a message of a channel or a ticket's
/// discussion is. Any other message answers as an unknown one, so a link never
/// tells that a private chat holds it.
async fn message_location(shared: &Arc<Shared>, message: aurelius_domain::MessageId) -> Result<ResponseBody, ProtocolError> {
    let store = shared.store().clone();
    storage_task(move || store.read(|c| aurelius_storage::threads::locate(c, message)))
        .await?
        .map(ResponseBody::MessageLocation)
        .ok_or_else(|| ProtocolError::new(ErrorCode::NotFound, "This message does not exist."))
}

/// `thread.read { message_id }`: the thread `message_id` starts (a message of a
/// channel or a ticket's discussion that is not itself a reply), its replies
/// oldest first. The reader's `thread_unread` is as it was before; then the
/// thread counts as read for them (not for an app).
async fn thread(shared: &Arc<Shared>, reader: Option<aurelius_domain::UserId>, root: aurelius_domain::MessageId) -> Result<ResponseBody, ProtocolError> {
    use aurelius_storage::threads;
    let store = shared.store().clone();
    let found = storage_task(move || {
        store.read(|c| {
            let Some(place) = threads::place(c, root)? else { return Ok(Err((ErrorCode::NotFound, "This message does not exist."))) };
            let Some(read_only) = threads::discussion_closed(c, place.conversation_id)? else {
                return Ok(Err((ErrorCode::Validation, "Only a message of a channel has a thread.")));
            };
            if place.thread_parent.is_some() {
                return Ok(Err((ErrorCode::Validation, "This message is a reply: read the thread of the message it answers.")));
            }
            let read_blob = |key: &str| store.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
            let as_of_seq = queries::last_seq(c)?;
            let mut messages = aurelius_storage::conversations::list_thread(c, place.conversation_id, root, &read_blob)?;
            threads::decorate(c, place.conversation_id, reader, &mut messages)?;
            aurelius_storage::message_edits::mark_editable(c, place.conversation_id, reader, &mut messages)?;
            for message in messages.iter_mut() {
                message.activity = std::mem::take(&mut message.activity).into_iter().map(aurelius_domain::activity::preview).collect();
            }
            Ok(Ok((as_of_seq, place.conversation_id, messages, read_only)))
        })
    })
    .await?;
    let (as_of_seq, conversation_id, mut messages, read_only) = found.map_err(|(code, why)| ProtocolError::new(code, why))?;
    if messages.first().is_none_or(|m| m.id != root) {
        return Err(internal("a thread without its root"));
    }
    let root_view = messages.remove(0);
    if let Some(user) = reader {
        let store = shared.store().clone();
        if let Err(err) = storage_task(move || store.write(move |tx| threads::mark_read(tx, user, root))).await {
            tracing::warn!(error = %err.message, %root, "could not mark the thread read");
        }
    }
    Ok(ResponseBody::Thread(aurelius_domain::views::ThreadSnapshot { as_of_seq, conversation_id, root: root_view, replies: messages, read_only }))
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
                            // A closed ticket's channel counts toward no badge (archived project channels still do).
                            let (unread, mentions) = if t.archived { (0, 0) } else { discussions::unread(c, *user, username, t.conversation_id, &read_text)? };
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
