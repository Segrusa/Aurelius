//! Appending an event with the next version of its aggregate.

use crate::{notice};
use aurelius_domain::{ActorKind, EventNotice};
use aurelius_storage::{StorageError, Tx};
use aurelius_storage::conversations::{self as conv};
use aurelius_storage::events::{self, NewEvent};

pub(crate) struct Ev<'a> {
    pub(crate) aggregate_type: &'a str,
    pub(crate) aggregate_id: String,
    pub(crate) event_type: &'a str,
    pub(crate) actor: ActorKind,
    pub(crate) actor_id: Option<String>,
    pub(crate) correlation: String,
    pub(crate) payload: Option<String>,
    pub(crate) metadata: serde_json::Value,
    pub(crate) dedupe: String,
}

/// Append with the next gap-free version and return a publishable notice.
pub(crate) fn emit(tx: &Tx<'_>, ev: Ev<'_>) -> Result<(aurelius_domain::EventId, EventNotice), StorageError> {
    let version = conv::next_version(tx, ev.aggregate_type, &ev.aggregate_id)?;
    let appended = events::append(
        tx,
        NewEvent {
            aggregate_type: ev.aggregate_type,
            aggregate_id: ev.aggregate_id.clone(),
            aggregate_version: version,
            event_type: ev.event_type,
            project_id: None,
            actor_kind: ev.actor,
            actor_id: ev.actor_id,
            correlation_id: ev.correlation,
            causation_id: None,
            payload_blob_id: ev.payload,
            metadata: ev.metadata,
            dedupe_key: ev.dedupe,
        },
    )?;
    Ok((appended.id, notice(appended.seq, ev.event_type, ev.aggregate_id, None)))
}

