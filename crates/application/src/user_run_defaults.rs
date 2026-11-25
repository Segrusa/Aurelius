//! `user.set_run_defaults` (feature `user_run_defaults`): the board member of the
//! session sets the model, effort, mode and subagent model of the runs on their
//! login. It changes nothing for other board members. Each change is one
//! `user.run_defaults_changed` event on the user's aggregate; the same defaults
//! again are a no-op without an event.

use crate::rail_pins::USER_AGGREGATE;
use crate::ticket::require_board_member;
use crate::{CommandContext, CommandService, Committed, domain_error, notice};
use aurelius_domain::user::SetUserRunDefaults;
use aurelius_domain::{ActorKind, AppError, CommandEnvelope, CommandResult, DomainError, EventKind};
use aurelius_storage::events::{self, NewEvent};
use aurelius_storage::user_run_defaults as store;
use serde_json::json;

impl CommandService {
    pub(crate) fn set_user_run_defaults(&self, ctx: &CommandContext, envelope: &CommandEnvelope, hash: String, cmd: &SetUserRunDefaults) -> Result<Committed, AppError> {
        let user = require_board_member(ctx)?;
        let defaults = cmd.defaults.validated().map_err(domain_error)?;
        let (command_id, meta, actor) = (envelope.command_id.to_string(), ctx.metadata(), ctx.principal_id.to_string());
        self.commit(envelope.command_id, hash, move |tx| {
            if store::get(tx, user)? == defaults {
                return Ok(Ok((CommandResult::UserRunDefaultsSet { defaults, changed: false }, Vec::new())));
            }
            if let Some(detail) = crate::run::connections::refused_user_effort(tx, user, &defaults)? {
                return Ok(Err(domain_error(DomainError::OutOfRange { field: "effort".into(), detail })));
            }
            let mut metadata = json!({ "user_id": user, "defaults": defaults });
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
                    event_type: EventKind::UserRunDefaultsChanged.as_str(),
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
            store::set(tx, user, &defaults, appended.id, aurelius_storage::now_ms())?;
            let n = notice(appended.seq, EventKind::UserRunDefaultsChanged.as_str(), aggregate_id, None);
            Ok(Ok((CommandResult::UserRunDefaultsSet { defaults, changed: true }, vec![n])))
        })
    }
}
