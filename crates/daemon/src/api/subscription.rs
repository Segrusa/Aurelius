//! `event.subscribe`: replay the durable events after a cursor, then forward
//! live events, streamed text and the load and power pushes.

use super::storage_task;
use crate::Shared;
use aurelius_domain::MessageId;
use aurelius_protocol::{ActivityView, ErrorCode, Event, LoadSnapshot, PowerState, ProtocolError, ResponseBody, ServerMessage, StreamDelta};
use aurelius_storage::queries;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, watch};

const REPLAY_PAGE: u32 = 500;

/// Replay durable events after `after_seq`, then forward live events.
/// A lagging subscriber re-reads from the DB instead of skipping events.
/// With `activity_changes` live updates carry only new or changed tool calls.
pub(super) async fn subscribe(
    shared: &Arc<Shared>,
    after_seq: u64,
    activity_changes: bool,
    out: mpsc::Sender<ServerMessage>,
) -> Result<(ResponseBody, tokio::task::JoinHandle<()>), ProtocolError> {
    // Subscribe before replaying so nothing committed in between is missed.
    let mut live = shared.events.subscribe();
    let mut streams = shared.streams.subscribe();
    let mut loads = Some(shared.admission.changes());
    let mut powers = Some(shared.power.changes());
    let mut cursor = replay(shared, after_seq, &out).await?;
    let replayed_through_seq = cursor;
    let shared = shared.clone();
    let task = tokio::spawn(async move {
        let mut sent = ActivitySent::default();
        loop {
            let received = tokio::select! {
                received = live.recv() => received,
                delta = streams.recv() => {
                    match delta {
                        // Ephemeral: never block durable events for a delta.
                        Ok(delta) if activity_changes => {
                            // The first update of a message in this session carries every call, as a full list.
                            let (message_id, known, changed) = (delta.message_id, sent.knows(delta.message_id), sent.changes(&delta));
                            let message = ServerMessage::Stream(StreamDelta { activity: changed.clone(), activity_changes: known, ..delta });
                            // Only what reached the outbox counts as sent; the rest goes again next time.
                            if out.try_send(message).is_ok() {
                                sent.record(message_id, changed);
                            }
                        }
                        Ok(delta) => { let _ = out.try_send(ServerMessage::Stream(delta)); }
                        Err(broadcast::error::RecvError::Closed) => break,
                        Err(broadcast::error::RecvError::Lagged(_)) => {}
                    }
                    continue;
                }
                snapshot = next_load(&mut loads) => {
                    // Ephemeral like a delta: only the latest counts.
                    if let Some(snapshot) = snapshot {
                        let _ = out.try_send(ServerMessage::LoadChanged(snapshot));
                    }
                    continue;
                }
                state = next_power(&mut powers) => {
                    if let Some(state) = state {
                        let _ = out.try_send(ServerMessage::PowerChanged(state));
                    }
                    continue;
                }
            };
            match received {
                Ok(notice) => {
                    if notice.seq <= cursor {
                        continue;
                    }
                    if notice.seq > cursor + 1 {
                        // Gap (e.g. concurrent commits published out of order): fill from DB.
                        sent.forget();
                        match replay(&shared, cursor, &out).await {
                            Ok(next) => cursor = next,
                            Err(_) => break,
                        }
                        if notice.seq <= cursor {
                            continue;
                        }
                    }
                    cursor = notice.seq;
                    let event = Event::from(notice);
                    // The client forgets its merged tool lists on these (`aurelius_client::stream::Calls`), so do we.
                    if matches!(event.kind.area(), "message" | "run") {
                        sent.forget();
                    }
                    if out.send(ServerMessage::Event(event)).await.is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    sent.forget();
                    match replay(&shared, cursor, &out).await {
                        Ok(next) => cursor = next,
                        Err(_) => break,
                    }
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    });
    Ok((ResponseBody::Subscribed { replayed_through_seq }, task))
}

/// The tool calls this session got per message, and whether each had finished,
/// so a client that merges changes gets only new or changed calls. The client
/// forgets its lists on every `message.*` / `run.*` event; this forgets at
/// least as often (more is safe: the next update is a full list again).
#[derive(Default)]
struct ActivitySent {
    messages: HashMap<MessageId, HashMap<String, bool>>,
}

impl ActivitySent {
    fn knows(&self, message: MessageId) -> bool {
        self.messages.contains_key(&message)
    }

    fn forget(&mut self) {
        self.messages.clear();
    }

    /// The calls of `delta` this session has not got as they are now (all of
    /// them for a message it got nothing of yet).
    fn changes(&self, delta: &StreamDelta) -> Vec<ActivityView> {
        let known = self.messages.get(&delta.message_id);
        delta
            .activity
            .iter()
            .filter(|a| known.is_none_or(|known| known.get(&a.call_id) != Some(&a.ok.is_some())))
            .cloned()
            .collect()
    }

    fn record(&mut self, message: MessageId, changed: Vec<ActivityView>) {
        if !self.messages.contains_key(&message) && self.messages.len() >= 64 {
            // Many answers at once: start over with full lists.
            self.forget();
        }
        let known = self.messages.entry(message).or_default();
        for a in changed {
            known.insert(a.call_id, a.ok.is_some());
        }
    }
}

/// The next `load_changed` snapshot; never once the sender is gone.
async fn next_load(loads: &mut Option<watch::Receiver<Option<LoadSnapshot>>>) -> Option<LoadSnapshot> {
    let Some(rx) = loads.as_mut() else { return std::future::pending().await };
    if rx.changed().await.is_err() {
        *loads = None;
        return None;
    }
    rx.borrow_and_update().clone()
}

/// The next `power_changed` state; never once the sender is gone.
async fn next_power(powers: &mut Option<watch::Receiver<PowerState>>) -> Option<PowerState> {
    let Some(rx) = powers.as_mut() else { return std::future::pending().await };
    if rx.changed().await.is_err() {
        *powers = None;
        return None;
    }
    Some(rx.borrow_and_update().clone())
}

async fn replay(shared: &Arc<Shared>, mut cursor: u64, out: &mpsc::Sender<ServerMessage>) -> Result<u64, ProtocolError> {
    loop {
        let store = shared.store().clone();
        let page = storage_task(move || store.read(|c| queries::events_after(c, cursor, REPLAY_PAGE))).await?;
        let done = page.len() < REPLAY_PAGE as usize;
        for notice in page {
            cursor = notice.seq;
            out.send(ServerMessage::Event(Event::from(notice)))
                .await
                .map_err(|_| ProtocolError::new(ErrorCode::Internal, "client went away"))?;
        }
        if done {
            return Ok(cursor);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aurelius_domain::{ConversationId, RunId};

    fn call(id: &str, ok: Option<bool>) -> ActivityView {
        ActivityView { call_id: id.into(), tool: "Bash".into(), input: serde_json::json!({}), output: ok.map(|_| "done".into()), ok, offset: 0, truncated: false, output_bytes: None }
    }

    fn delta(message_id: MessageId, activity: Vec<ActivityView>) -> StreamDelta {
        StreamDelta { run_id: RunId::new(), conversation_id: ConversationId::new(), message_id, text: "x".into(), activity, progress: None, activity_changes: false }
    }

    fn ids(calls: &[ActivityView]) -> Vec<&str> {
        calls.iter().map(|a| a.call_id.as_str()).collect()
    }

    #[test]
    fn only_new_or_finished_calls_go_again() {
        let mut sent = ActivitySent::default();
        let message = MessageId::new();
        let first = delta(message, vec![call("a", Some(true)), call("b", None)]);
        assert!(!sent.knows(message));
        let changed = sent.changes(&first);
        assert_eq!(ids(&changed), ["a", "b"]);
        sent.record(message, changed);
        assert!(sent.knows(message));
        // Text only: nothing changed.
        assert!(sent.changes(&first).is_empty());
        // "b" finished and "c" started.
        let next = delta(message, vec![call("a", Some(true)), call("b", Some(false)), call("c", None)]);
        let changed = sent.changes(&next);
        assert_eq!(ids(&changed), ["b", "c"]);
        // Not recorded (the outbox was full): the same changes go again.
        assert_eq!(ids(&sent.changes(&next)), ["b", "c"]);
        sent.record(message, changed);
        assert!(sent.changes(&next).is_empty());
        // Another message starts with everything.
        assert_eq!(ids(&sent.changes(&delta(MessageId::new(), vec![call("z", None)]))), ["z"]);
        // Forgotten (a message or run event went out): a full list again.
        sent.forget();
        assert!(!sent.knows(message));
        assert_eq!(ids(&sent.changes(&next)), ["a", "b", "c"]);
    }
}
