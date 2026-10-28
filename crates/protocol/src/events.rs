//! Events on the wire: what a subscriber gets after `event.subscribe`.
//! The kind is a typed enum; clients refresh what the kind says changed.

use aurelius_domain::{EventKind, EventNotice, ProjectId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Event {
    /// Position in the log; replay with `event.subscribe { after_seq }`.
    pub seq: u64,
    pub kind: EventKind,
    /// The task, employee, conversation, … the event is about.
    pub aggregate_id: String,
    pub project_id: Option<ProjectId>,
    pub at_ms: i64,
}

impl From<EventNotice> for Event {
    fn from(notice: EventNotice) -> Self {
        Self { seq: notice.seq, kind: notice.kind, aggregate_id: notice.aggregate_id, project_id: notice.project_id, at_ms: notice.occurred_at_ms }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_event_is_typed_and_snake_case() {
        let event = Event::from(EventNotice { seq: 3, kind: EventKind::TaskCreated, aggregate_id: "t".into(), project_id: None, occurred_at_ms: 9 });
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json, serde_json::json!({ "seq": 3, "kind": "task.created", "aggregate_id": "t", "project_id": null, "at_ms": 9 }));
        assert_eq!(serde_json::from_value::<Event>(json).unwrap(), event);
        assert!(serde_json::from_value::<Event>(serde_json::json!({ "seq": 1, "kind": "task.reviewed", "aggregate_id": "t", "project_id": null, "at_ms": 1 })).is_err());
    }
}
