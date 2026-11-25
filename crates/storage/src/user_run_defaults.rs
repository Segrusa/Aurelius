//! Each board member's own run defaults (migration 52, feature
//! `user_run_defaults`): the model, effort, mode and subagent model of the runs
//! on their login. A board member without a row chose nothing.

use crate::StorageError;
use aurelius_domain::autonomy::AgentMode;
use aurelius_domain::user::UserRunDefaults;
use aurelius_domain::{EventId, UserId};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

/// model, effort, mode, subagent_model.
type DefaultsRow = (Option<String>, Option<String>, Option<String>, Option<String>);

/// The defaults of `user`; all unset when they chose none.
pub fn get(conn: &Connection, user: UserId) -> Result<UserRunDefaults, StorageError> {
    let row: Option<DefaultsRow> = conn
        .query_row("SELECT model, effort, mode, subagent_model FROM user_run_defaults WHERE user_id = ?1", [user.to_string()], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })
        .optional()?;
    let Some((model, effort, mode, subagent_model)) = row else { return Ok(UserRunDefaults::default()) };
    Ok(UserRunDefaults { model, effort, mode: mode.as_deref().map(AgentMode::parse), subagent_model })
}

/// Store `defaults` (validated) as those of `user`, written by the event
/// `source_event`. The caller compares with `get` first, so the same defaults
/// a second time make no event.
pub fn set(tx: &Transaction<'_>, user: UserId, defaults: &UserRunDefaults, source_event: EventId, now_ms: i64) -> Result<(), StorageError> {
    tx.execute(
        "INSERT INTO user_run_defaults(user_id, model, effort, mode, subagent_model, source_event_id, updated_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(user_id) DO UPDATE SET model = excluded.model, effort = excluded.effort, mode = excluded.mode,
           subagent_model = excluded.subagent_model, source_event_id = excluded.source_event_id, updated_at_ms = excluded.updated_at_ms",
        params![user.to_string(), defaults.model, defaults.effort, defaults.mode.map(AgentMode::as_str), defaults.subagent_model, source_event.to_string(), now_ms],
    )?;
    Ok(())
}
