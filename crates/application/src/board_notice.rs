//! What the server tells every board member about itself, in their Inbox: a vault
//! key export, a vault name runs may no longer have. It was a line in the Team chat
//! before the Team chat went away. Each is one `company.updated` event (what
//! changed: `server_notice`) and one notification row per board member.

use crate::{CommandService, notice};
use aurelius_domain::{ActorKind, AppError, EventId, EventKind, EventNotice};
use aurelius_storage::events::{self, NewEvent};
use aurelius_storage::{StorageError, timeline};
use serde_json::json;

/// What a row reads as when it carries a warning (the server's own business).
const SEVERITY: &str = "warning";

/// Why nobody could be told.
const NOBODY_TOLD: &str = "No board member could be told: there is no active board member.";

impl CommandService {
    /// Tell every board member `text` in their Inbox. The notices to publish; an error
    /// when it could not be written for at least one board member (a caller that must
    /// not go on without it, as a key export, stops there).
    pub fn board_notice(&self, text: &str) -> Result<Vec<EventNotice>, AppError> {
        let text = text.to_owned();
        self.store
            .write(move |tx| {
                let company = aurelius_storage::company::get(tx)?;
                let id = EventId::new();
                let appended = events::append(
                    tx,
                    NewEvent {
                        aggregate_type: "company",
                        aggregate_version: aurelius_storage::conversations::next_version(tx, "company", &company.id)?,
                        aggregate_id: company.id.clone(),
                        event_type: EventKind::CompanyUpdated.as_str(),
                        project_id: None,
                        actor_kind: ActorKind::System,
                        actor_id: None,
                        correlation_id: id.to_string(),
                        causation_id: None,
                        payload_blob_id: None,
                        metadata: json!({ "changed": ["server_notice"], "notice": text }),
                        dedupe_key: format!("server-notice:{id}"),
                    },
                )?;
                if timeline::notify_board_members(tx, appended.seq, SEVERITY, &text)? == 0 {
                    return Err(StorageError::Rejected(NOBODY_TOLD.into()));
                }
                Ok(vec![notice(appended.seq, EventKind::CompanyUpdated.as_str(), company.id, None)])
            })
            .map_err(crate::storage_error)
    }
}
