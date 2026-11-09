//! The host writes: what only the daemon can do (MCP, load limits,
//! browser, backups, power, updates, remote access,
//! notifications, secrets, folders). `writes` wraps each in the receipt and audit event.

use super::storage_task;
use crate::Shared;
use aurelius_protocol::{ErrorCode, HostResult, HostWrite, ProtocolError};
use std::sync::Arc;

/// `user`: the board member of the connection (their own notifications).
pub(super) async fn perform(shared: &Arc<Shared>, write: HostWrite, user: Option<aurelius_domain::UserId>) -> Result<HostResult, ProtocolError> {
    let user_id = user;
    let user = user.map(|u| u.to_string());
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
        HostWrite::LoadSetLimits(update) => HostResult::Load(crate::load::set_limits(shared, update).await?),
        HostWrite::BrowserSet { choice } => HostResult::Browser(aurelius_host::chrome::set_choice(shared, choice).await?),
        HostWrite::BrowserSetBackend { backend } => HostResult::Browser(aurelius_host::playwright::set_backend(shared, backend).await?),
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
