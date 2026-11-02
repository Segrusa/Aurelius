//! The host writes: what only the daemon can do (MCP, plugins, load limits,
//! browser, backups, power, updates, remote access, agent functions, API keys,
//! notifications, secrets, folders, project notes). `writes` wraps each in the receipt and audit event.

use super::{app_task, storage_task};
use crate::Shared;
use aurelius_protocol::{ErrorCode, HostResult, HostWrite, ProtocolError};
use std::sync::Arc;

/// `user`: the board member of the connection (their own notifications).
pub(super) async fn perform(shared: &Arc<Shared>, write: HostWrite, user: Option<aurelius_domain::UserId>) -> Result<HostResult, ProtocolError> {
    let user_id = user;
    let user = user.map(|u| u.to_string());
    let s = shared.clone();
    Ok(match write {
        // Answered by `users::create` before the host pipeline (which would keep the request).
        HostWrite::UserCreate(_) => return Err(super::internal("user.create reached the host pipeline")),
        // Sessions, passwords and Claude logins: answered by `users` before the pipeline.
        HostWrite::SessionRevoke { .. }
        | HostWrite::UserSetPassword(_)
        | HostWrite::UserDisable { .. }
        | HostWrite::ClaudeLoginLink
        | HostWrite::ClaudeLoginCancel { .. }
        | HostWrite::ClaudeLoginUnlink => return Err(super::internal("a session write reached the host pipeline")),
        // Answered by `vault` before the pipeline (which would keep the value).
        HostWrite::VaultCreate(_) | HostWrite::VaultUpdate(_) | HostWrite::VaultArchive { .. } => return Err(super::internal("a vault write reached the host pipeline")),
        HostWrite::SecretSet { connection_id, secret } => {
            crate::runtimes::store_secret(shared, connection_id, secret).await?;
            HostResult::Done
        }
        HostWrite::McpSet { enabled } => HostResult::McpServers(crate::mcp_servers::set(shared, enabled, user_id).await?),
        HostWrite::McpAdd(params) => HostResult::McpServers(crate::mcp_servers::add(shared, params, user_id).await?),
        HostWrite::McpRemove { name } => HostResult::McpServers(crate::mcp_servers::remove(shared, name, user_id).await?),
        HostWrite::McpLoginStart { name } => HostResult::McpLogin(crate::mcp_oauth::start(shared, name, user_id).await?),
        HostWrite::McpLoginComplete { login_id, redirect_url } => HostResult::McpLogin(crate::mcp_oauth::complete_pasted(shared, login_id, redirect_url, user_id).await?),
        HostWrite::McpLogout { name } => HostResult::McpServers(crate::mcp_oauth::logout(shared, name, user_id).await?),
        HostWrite::McpAlwaysAllowRemove { tool } => HostResult::McpAlwaysAllow(crate::mcp_servers::always_allow_remove(shared, tool).await?),
        HostWrite::PluginPreview { source } => HostResult::PluginPreview(aurelius_host::plugins::preview(source).await?),
        HostWrite::PluginInstall { ids, employees, projects } => HostResult::Plugins(aurelius_host::plugins::install(shared, ids, employees, projects).await?),
        HostWrite::PluginUpdate { id } => HostResult::Plugins(aurelius_host::plugins::update(shared, id).await?),
        HostWrite::PluginRemove { id } => HostResult::Plugins(aurelius_host::plugins::remove(shared, id).await?),
        HostWrite::PluginSetForEmployee { employee_id, enabled } => HostResult::EmployeePlugins(aurelius_host::plugins::employee_set(shared, employee_id, enabled).await?),
        HostWrite::PluginSetForProject { project_id, enabled } => HostResult::ProjectPlugins(aurelius_host::plugins::project_set(shared, project_id, enabled).await?),
        HostWrite::LoadSetLimits(update) => HostResult::Load(crate::load::set_limits(shared, update).await?),
        HostWrite::BrowserSet { choice } => HostResult::Browser(aurelius_host::chrome::set_choice(shared, choice).await?),
        HostWrite::BrowserSetBackend { backend } => HostResult::Browser(aurelius_host::playwright::set_backend(shared, backend).await?),
        // Read at each run's start: applies from the next run on.
        HostWrite::ArtifactSetEnabled { enabled } => {
            let store = shared.store().clone();
            storage_task(move || store.write(move |tx| aurelius_storage::artifacts::set_enabled(tx, enabled, aurelius_storage::now_ms()))).await?;
            HostResult::ArtifactSetting(aurelius_protocol::ArtifactSetting { enabled })
        }
        // Read on every tool call: applies at once.
        HostWrite::AutoAllowSetEnabled { enabled } => {
            let store = shared.store().clone();
            storage_task(move || store.write(move |tx| aurelius_storage::auto_allow::set_enabled(tx, enabled, aurelius_storage::now_ms()))).await?;
            HostResult::AutoAllowSetting(aurelius_protocol::AutoAllowSetting::new(enabled))
        }
        // Checked when a task conversation opens and when a run starts: applies from then on.
        HostWrite::ManagementModeSetEnabled { enabled } => {
            let store = shared.store().clone();
            storage_task(move || store.write(move |tx| aurelius_storage::management_mode::set_enabled(tx, enabled, aurelius_storage::now_ms()))).await?;
            HostResult::ManagementModeSetting(aurelius_protocol::ManagementModeSetting { enabled })
        }
        // Read when each run starts: applies from the next run on.
        HostWrite::TraceSetEnabled { enabled } => {
            let store = shared.store().clone();
            HostResult::TraceSetting(storage_task(move || store.write(move |tx| aurelius_storage::traces::set_enabled(tx, enabled, aurelius_storage::now_ms()))).await?)
        }
        // Checked when a task asks for variants; tasks that already have them go on.
        HostWrite::VariantsSetEnabled { enabled } => {
            let store = shared.store().clone();
            HostResult::VariantsSetting(storage_task(move || store.write(move |tx| aurelius_storage::variants::set_enabled(tx, enabled, aurelius_storage::now_ms()))).await?)
        }
        // Runs read the notes as context: a vault value in one is refused, before anything is stored (R1 m7).
        HostWrite::NoteCreate { project_id, body } => {
            crate::vault::refuse_secret_text(&[Some(&body)])?;
            let (note, notices) = app_task(move || s.service.create_note(project_id, body)).await?;
            shared.publish(notices);
            HostResult::Note(note)
        }
        HostWrite::NoteUpdate { note_id, body, base_revision } => {
            crate::vault::refuse_secret_text(&[Some(&body)])?;
            let (note, notices) = app_task(move || s.service.update_note(note_id, body, base_revision)).await?;
            shared.publish(notices);
            HostResult::Note(note)
        }
        HostWrite::NoteSetPinned { note_id, pinned } => {
            let (note, notices) = app_task(move || s.service.set_note_pinned(note_id, pinned)).await?;
            shared.publish(notices);
            HostResult::Note(note)
        }
        HostWrite::NoteDelete { note_id } => {
            shared.publish(app_task(move || s.service.delete_note(note_id)).await?);
            HostResult::Done
        }
        HostWrite::AgentFunctionCreate(cmd) => HostResult::AgentFunction(app_task(move || s.service.create_agent_function(&cmd)).await?),
        HostWrite::AgentFunctionUpdate(cmd) => HostResult::AgentFunction(app_task(move || s.service.update_agent_function(&cmd)).await?),
        HostWrite::AgentFunctionDelete { function_id } => {
            app_task(move || s.service.delete_agent_function(function_id)).await?;
            HostResult::Done
        }
        HostWrite::AgentFunctionSetEnabled { function_id, enabled } => {
            app_task(move || s.service.set_agent_function_enabled(function_id, enabled)).await?;
            HostResult::Done
        }
        HostWrite::AgentFunctionSetAllEnabled { enabled } => {
            app_task(move || s.service.set_all_agent_functions_enabled(enabled)).await?;
            HostResult::Done
        }
        HostWrite::ApiKeyCreate { name, function_ids } => HostResult::ApiKeyCreated(app_task(move || s.service.create_api_key(&name, &function_ids)).await?),
        HostWrite::ApiKeyRevoke { key_id } => {
            app_task(move || s.service.revoke_api_key(key_id)).await?;
            HostResult::Done
        }
        HostWrite::ApiKeySetFunctions { key_id, function_ids } => {
            app_task(move || s.service.set_api_key_functions(key_id, &function_ids)).await?;
            HostResult::Done
        }
        HostWrite::AppKeyCreate(cmd) => HostResult::AppKeyCreated(app_task(move || s.service.create_app_key(&cmd)).await?),
        HostWrite::AppKeyRevoke { key_id } => {
            let view = app_task(move || s.service.revoke_app_key(key_id)).await?;
            // Every request of its sessions already fails; this also closes its open WebSockets.
            shared.remote.revoke_app(key_id);
            HostResult::AppKey(view)
        }
        HostWrite::NotificationMarkAllRead => {
            let store = shared.store().clone();
            HostResult::Count { count: storage_task(move || store.write(move |tx| aurelius_storage::timeline::mark_all_read_for(tx, user.as_deref()))).await? as u64 }
        }
        HostWrite::NotificationMarkRead { id } => {
            let store = shared.store().clone();
            HostResult::Count { count: storage_task(move || store.write(move |tx| aurelius_storage::timeline::mark_read_for(tx, &id, user.as_deref()))).await? as u64 }
        }
        HostWrite::NotificationDismiss { id } => {
            let store = shared.store().clone();
            HostResult::Count { count: storage_task(move || store.write(move |tx| aurelius_storage::timeline::dismiss_for(tx, &id, user.as_deref()))).await? as u64 }
        }
        HostWrite::NotificationDismissAll => {
            let store = shared.store().clone();
            HostResult::Count { count: storage_task(move || store.write(move |tx| aurelius_storage::timeline::dismiss_all_for(tx, user.as_deref()))).await? as u64 }
        }
        HostWrite::BackupCreate => backups(aurelius_host::backups::create(shared, "manual").await)?,
        HostWrite::BackupRestore { id } => backups(aurelius_host::backups::restore(shared, &id).await)?,
        HostWrite::BackupResume => backups(aurelius_host::backups::resume(shared))?,
        HostWrite::ProjectExport { project_id } => {
            let (markdown_path, json_path) =
                aurelius_host::backups::export_project(shared, project_id).await.map_err(|e| ProtocolError::new(ErrorCode::Validation, e))?;
            HostResult::Exported { markdown_path, json_path }
        }
        HostWrite::PowerShutdown { when } => HostResult::Power(aurelius_host::power::request(&shared.power, when)),
        HostWrite::PowerCancel => HostResult::Power(aurelius_host::power::cancel(&shared.power)),
        HostWrite::ServerRestart => {
            tracing::info!("restart requested by a client");
            shared.restart_after_reply();
            HostResult::ShuttingDown
        }
        HostWrite::UpdatePrepare => HostResult::Update(crate::updates::prepare(shared).await),
        HostWrite::UpdateApply => HostResult::Update(crate::updates::apply(shared).await.map_err(|e| ProtocolError::new(ErrorCode::Validation, e))?),
        HostWrite::UpdatePrepareApp => HostResult::Update(crate::updates::prepare_app(shared).await),
        HostWrite::UpdateApplyApp => HostResult::Update(crate::updates::apply_app(shared).await.map_err(|e| ProtocolError::new(ErrorCode::Validation, e))?),
        HostWrite::RemoteEnable => HostResult::Remote(crate::remote::enable(shared).await?),
        HostWrite::RemoteDisable => HostResult::Remote(crate::remote::disable(shared).await?),
        HostWrite::RemoteStartPairing => HostResult::PairingCode(crate::remote::pairing_start(shared).await?),
        HostWrite::RemoteDecidePairing { pairing_id, approve, owner } => HostResult::Remote(crate::remote::pairing_decide(shared, &pairing_id, approve, owner).await?),
        HostWrite::RemoteRevoke { client_id } => {
            let status = crate::remote::revoke(shared, client_id).await?;
            // Its board members' sessions ended with it (storage); their connections close now.
            shared.sessions.end_client(client_id, aurelius_domain::user::SignOutReason::Revoked);
            HostResult::Remote(status)
        }
        HostWrite::PushConfigure(config) => HostResult::Remote(crate::remote::push_configure(shared, config).await?),
        HostWrite::PushTest { client_id } => {
            crate::remote::push_test(shared, client_id).await?;
            HostResult::Done
        }
        HostWrite::FsCreateDir { parent, name } => HostResult::DirListing(crate::browse::create(shared, parent, name).await?),
    })
}

/// A backup action that failed says so in its snapshot; that is an error, not a receipt.
fn backups(snapshot: aurelius_protocol::BackupsSnapshot) -> Result<HostResult, ProtocolError> {
    match snapshot.error.clone() {
        Some(message) => Err(ProtocolError::new(ErrorCode::Validation, message)),
        None => Ok(HostResult::Backups(snapshot)),
    }
}
