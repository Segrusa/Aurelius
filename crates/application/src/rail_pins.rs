//! `user.set_rail_pins` (feature `rail_pins`): the board member of the session
//! chooses which workspaces their rail shows, in which order, on every device
//! they sign in on. It changes nothing for other board members. Each change is
//! one `user.rail_pins_changed` event on the user's aggregate; the same list
//! again is a no-op without an event.

use crate::ticket::require_board_member;
use crate::{CommandContext, CommandService, Committed, domain_error, notice};
use aurelius_domain::user::SetRailPins;
use aurelius_domain::{ActorKind, AppError, CommandEnvelope, CommandResult, DomainError, EventKind};
use aurelius_storage::events::{self, NewEvent};
use aurelius_storage::rail_pins as store;
use serde_json::json;

/// The aggregate of a board member's own settings in the event log.
pub const USER_AGGREGATE: &str = "user";

impl CommandService {
    pub(crate) fn set_rail_pins(&self, ctx: &CommandContext, envelope: &CommandEnvelope, hash: String, cmd: &SetRailPins) -> Result<Committed, AppError> {
        let user = require_board_member(ctx)?;
        let pins = cmd.valid_pins().map_err(domain_error)?;
        let (command_id, meta, actor) = (envelope.command_id.to_string(), ctx.metadata(), ctx.principal_id.to_string());
        self.commit(envelope.command_id, hash, move |tx| {
            if let Some(unknown) = store::unknown_projects(tx, &pins)?.first() {
                return Ok(Err(domain_error(DomainError::ProjectNotFound(*unknown))));
            }
            if !store::set(tx, user, &pins, aurelius_storage::now_ms())? {
                return Ok(Ok((CommandResult::RailPinsSet { project_ids: pins, changed: false }, Vec::new())));
            }
            let mut metadata = json!({ "user_id": user, "project_ids": pins });
            if let (Some(m), Some(ctx)) = (metadata.as_object_mut(), meta.as_object()) {
                for (k, v) in ctx {
                    m.entry(k.clone()).or_insert(v.clone());
                }
            }
            let aggregate_id = user.to_string();
            let appended = events::append(
                tx,
                NewEvent {
                    aggregate_type: USER_AGGREGATE,
                    aggregate_version: crate::chat_next_version(tx, USER_AGGREGATE, &aggregate_id)?,
                    aggregate_id: aggregate_id.clone(),
                    event_type: EventKind::UserRailPinsChanged.as_str(),
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
            let n = notice(appended.seq, EventKind::UserRailPinsChanged.as_str(), aggregate_id, None);
            Ok(Ok((CommandResult::RailPinsSet { project_ids: pins, changed: true }, vec![n])))
        })
    }
}
