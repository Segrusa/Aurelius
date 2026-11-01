//! `server.set_name` (feature `server_name`, SN-01): a board member chooses the
//! name every client shows for this server. The last write wins (OV-03); each
//! change is one `server.renamed` event with who, the old and the new name.
//! The same name as now is a no-op: no event, no new version (AC6).

use crate::ticket::require_board_member;
use crate::{CommandContext, CommandService, Committed, domain_error, notice};
use aurelius_domain::server::{ServerNameView, SetServerName};
use aurelius_domain::{ActorKind, AppError, CommandEnvelope, CommandResult, EventKind};
use aurelius_storage::events::{self, NewEvent};
use aurelius_storage::server_name as store;
use serde_json::json;

/// The aggregate of the server's name in the event log: there is one server.
pub const SERVER_AGGREGATE: &str = "server";

impl CommandService {
    pub(crate) fn set_server_name(&self, ctx: &CommandContext, envelope: &CommandEnvelope, hash: String, cmd: &SetServerName) -> Result<Committed, AppError> {
        let user = require_board_member(ctx)?;
        let chosen = cmd.valid_name().map_err(domain_error)?;
        let computer_name = aurelius_host::host_name();
        let (command_id, meta, actor) = (envelope.command_id.to_string(), ctx.metadata(), ctx.principal_id.to_string());
        self.commit(envelope.command_id, hash, move |tx| {
            let current = store::get(tx)?;
            let before = ServerNameView::of(current.chosen.clone(), computer_name.clone(), current.version);
            if current.chosen == chosen {
                return Ok(Ok((CommandResult::ServerNameSet { name: before.name, version: before.version }, Vec::new())));
            }
            let version = store::set(tx, chosen.as_deref(), user)?;
            let after = ServerNameView::of(chosen.clone(), computer_name, version);
            let mut metadata = json!({ "by": user, "before": before.name, "after": after.name, "chosen": chosen, "version": version });
            if let (Some(m), Some(ctx)) = (metadata.as_object_mut(), meta.as_object()) {
                for (k, v) in ctx {
                    m.entry(k.clone()).or_insert(v.clone());
                }
            }
            let appended = events::append(
                tx,
                NewEvent {
                    aggregate_type: SERVER_AGGREGATE,
                    aggregate_version: crate::chat_next_version(tx, SERVER_AGGREGATE, SERVER_AGGREGATE)?,
                    aggregate_id: SERVER_AGGREGATE.to_owned(),
                    event_type: EventKind::ServerRenamed.as_str(),
                    project_id: None,
                    actor_kind: ActorKind::User,
                    actor_id: Some(actor),
                    correlation_id: command_id.clone(),
                    causation_id: None,
                    payload_blob_id: None,
                    metadata,
                    dedupe_key: format!("cmd:{command_id}:0"),
                },
            )?;
            let n = notice(appended.seq, EventKind::ServerRenamed.as_str(), SERVER_AGGREGATE.to_owned(), None);
            Ok(Ok((CommandResult::ServerNameSet { name: after.name, version }, vec![n])))
        })
    }
}
