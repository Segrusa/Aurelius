//! `event.subscribe`: replay the durable events after a cursor, then forward
//! live events, streamed text and the load and power pushes.

use super::storage_task;
use crate::Shared;
use aurelius_domain::{EventKind, MessageId};
use aurelius_protocol::{ActivityView, ErrorCode, Event, LoadSnapshot, PowerState, ProtocolError, ResponseBody, ServerMessage, StreamDelta};
use aurelius_storage::{Store, queries};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, watch};

const REPLAY_PAGE: u32 = 500;

/// What the hello's `accepts` named that changes the live updates of a connection
/// (`aurelius_protocol::client_feature`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Accepts {
    pub activity_changes: bool,
    pub team_channels: bool,
    pub server_name: bool,
    pub chat_threads: bool,
    pub chat_reactions: bool,
    pub ticket_follow: bool,
    pub message_edits: bool,
}

impl Accepts {
    /// The client features named in a hello.
    pub(super) fn of(accepts: &[String]) -> Self {
        use aurelius_protocol::client_feature as f;
        let has = |feature: &str| accepts.iter().any(|a| a == feature);
        Accepts {
            activity_changes: has(f::ACTIVITY_CHANGES),
            team_channels: has(f::TEAM_CHANNELS),
            server_name: has(f::SERVER_NAME),
            chat_threads: has(f::CHAT_THREADS),
            chat_reactions: has(f::CHAT_REACTIONS),
            ticket_follow: has(f::TICKET_FOLLOW),
            message_edits: has(f::MESSAGE_EDITS),
        }
    }
}

/// Replay durable events after `after_seq`, then forward live events.
/// A lagging subscriber re-reads from the DB instead of skipping events.
/// With `activity_changes` live updates carry only new or changed tool calls.
/// Without `team_channels` the events and streamed text of project channels are
/// left out: an older client would show them in its one Team chat. Without
/// `server_name` the `server.renamed` events are, and without `chat_reactions`
/// the `message.reacted` ones, without `ticket_follow` the `ticket.follow_changed`
/// ones and without `message_edits` the `message.edited` ones: an older client
/// cannot read an event kind it does not know. Without `chat_threads` the streamed
/// text of an answer in a thread is: an older client would show it in the
/// channel's own list.
pub(super) async fn subscribe(
    shared: &Arc<Shared>,
    after_seq: u64,
    accepts: Accepts,
    out: mpsc::Sender<ServerMessage>,
) -> Result<(ResponseBody, tokio::task::JoinHandle<()>), ProtocolError> {
    let activity_changes = accepts.activity_changes;
    // Subscribe before replaying so nothing committed in between is missed.
    let mut live = shared.events.subscribe();
    let mut streams = shared.streams.subscribe();
    let mut loads = Some(shared.admission.changes());
    let mut powers = Some(shared.power.changes());
    let mut hidden = Hidden::new(accepts.team_channels, accepts.server_name).with_chat(accepts.chat_threads, accepts.chat_reactions).with_follow_and_edits(accepts.ticket_follow, accepts.message_edits);
    let mut cursor = replay(shared, after_seq, &out, &mut hidden).await?;
    let replayed_through_seq = cursor;
    let shared = shared.clone();
    let task = tokio::spawn(async move {
        let mut sent = ActivitySent::default();
        loop {
            let received = tokio::select! {
                received = live.recv() => received,
                delta = streams.recv() => {
                    let delta = match delta {
                        Ok(delta) if hidden.hides_stream(&delta) || hidden.hides(shared.store(), &delta.conversation_id.to_string()).await => continue,
                        other => other,
                    };
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
                        match replay(&shared, cursor, &out, &mut hidden).await {
                            Ok(next) => cursor = next,
                            Err(_) => break,
                        }
                        if notice.seq <= cursor {
                            continue;
                        }
                    }
                    cursor = notice.seq;
                    if hidden.hides_kind(notice.kind) || hidden.hides(shared.store(), &notice.aggregate_id).await {
                        continue;
                    }
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
                    match replay(&shared, cursor, &out, &mut hidden).await {
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

/// The project channels a connection without `team_channels` does not get
/// (`client_feature::TEAM_CHANNELS`). A conversation never becomes or stops
/// being a project channel, so each id is looked up once, off the async
/// threads (`storage_task`): the forwarder never blocks a tokio worker on SQLite.
/// Nor does a connection without `server_name` get `server.renamed`
/// (`client_feature::SERVER_NAME`, SN-02 AC3), one without `chat_reactions`
/// `message.reacted`, one without `ticket_follow` `ticket.follow_changed`, one
/// without `message_edits` `message.edited`, or one without `chat_threads` the
/// streamed text of an answer in a thread.
struct Hidden {
    team_channels: bool,
    server_name: bool,
    chat_threads: bool,
    chat_reactions: bool,
    ticket_follow: bool,
    message_edits: bool,
    known: HashMap<String, bool>,
}

impl Hidden {
    fn new(team_channels: bool, server_name: bool) -> Self {
        Hidden { team_channels, server_name, chat_threads: false, chat_reactions: false, ticket_follow: false, message_edits: false, known: HashMap::new() }
    }

    /// The same, for a client that shows threads and/or reactions.
    fn with_chat(self, chat_threads: bool, chat_reactions: bool) -> Self {
        Hidden { chat_threads, chat_reactions, ..self }
    }

    /// The same, for a client that shows followers and/or edited messages.
    fn with_follow_and_edits(self, ticket_follow: bool, message_edits: bool) -> Self {
        Hidden { ticket_follow, message_edits, ..self }
    }

    /// Event kinds this connection's client does not know.
    fn hides_kind(&self, kind: EventKind) -> bool {
        match kind {
            EventKind::ServerRenamed => !self.server_name,
            EventKind::MessageReacted => !self.chat_reactions,
            EventKind::TicketFollowChanged => !self.ticket_follow,
            EventKind::MessageEdited => !self.message_edits,
            _ => false,
        }
    }

    /// Streamed text this connection's client would show in the wrong place:
    /// an answer in a thread, for a client without threads.
    fn hides_stream(&self, delta: &StreamDelta) -> bool {
        delta.thread_parent.is_some() && !self.chat_threads
    }

    async fn hides(&mut self, store: &Arc<Store>, aggregate_id: &str) -> bool {
        if self.team_channels {
            return false;
        }
        if let Some(hidden) = self.known.get(aggregate_id) {
            return *hidden;
        }
        let (id, store) = (aggregate_id.to_owned(), store.clone());
        let hidden = storage_task(move || {
            store.read(|c| {
                Ok(c.query_row("SELECT EXISTS(SELECT 1 FROM conversations WHERE id = ?1 AND kind = 'group' AND project_id IS NOT NULL)", [&id], |r| r.get::<_, bool>(0))?)
            })
        })
        .await
        .unwrap_or(false);
        // A long session sees many aggregates: start over rather than grow without end.
        if self.known.len() >= 10_000 {
            self.known.clear();
        }
        self.known.insert(aggregate_id.to_owned(), hidden);
        hidden
    }
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

async fn replay(shared: &Arc<Shared>, mut cursor: u64, out: &mpsc::Sender<ServerMessage>, hidden: &mut Hidden) -> Result<u64, ProtocolError> {
    loop {
        let store = shared.store().clone();
        let page = storage_task(move || store.read(|c| queries::events_after(c, cursor, REPLAY_PAGE))).await?;
        let done = page.len() < REPLAY_PAGE as usize;
        for notice in page {
            cursor = notice.seq;
            if hidden.hides_kind(notice.kind) || hidden.hides(shared.store(), &notice.aggregate_id).await {
                continue;
            }
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
        StreamDelta { run_id: RunId::new(), conversation_id: ConversationId::new(), message_id, text: "x".into(), activity, progress: None, activity_changes: false, mode: None, thread_parent: None }
    }

    fn ids(calls: &[ActivityView]) -> Vec<&str> {
        calls.iter().map(|a| a.call_id.as_str()).collect()
    }

    #[tokio::test(flavor = "current_thread")]
    async fn hidden_looks_a_channel_up_once_off_the_async_threads() {
        // Trace m6: on a current-thread runtime a blocking read would still work, so
        // this checks the answers and the cache; the read itself goes through `storage_task`.
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open(dir.path()).unwrap());
        let general = store.read(aurelius_storage::conversations::group_conversation).unwrap().expect("the general channel").to_string();
        let mut hidden = Hidden::new(false, false);
        assert!(!hidden.hides(&store, &general).await, "the general channel is not hidden");
        assert!(!hidden.hides(&store, &ConversationId::new().to_string()).await);
        assert_eq!(hidden.known.len(), 2, "each id looked up once");
        assert!(!Hidden::new(true, false).hides(&store, &general).await);
    }

    #[test]
    fn only_a_client_that_accepts_chat_reactions_gets_message_reacted() {
        assert!(Hidden::new(true, true).hides_kind(EventKind::MessageReacted));
        assert!(Hidden::new(true, true).with_chat(true, false).hides_kind(EventKind::MessageReacted));
        assert!(!Hidden::new(false, false).with_chat(false, true).hides_kind(EventKind::MessageReacted));
        // A reply in a thread is an event an older client knows.
        assert!(!Hidden::new(true, true).hides_kind(EventKind::MessageUserCommitted));
    }

    #[test]
    fn only_a_client_that_accepts_ticket_follow_gets_ticket_follow_changed() {
        assert!(Hidden::new(true, true).with_chat(true, true).hides_kind(EventKind::TicketFollowChanged));
        assert!(Hidden::new(true, true).with_follow_and_edits(false, true).hides_kind(EventKind::TicketFollowChanged));
        assert!(!Hidden::new(false, false).with_follow_and_edits(true, false).hides_kind(EventKind::TicketFollowChanged));
    }

    #[test]
    fn only_a_client_that_accepts_message_edits_gets_message_edited() {
        assert!(Hidden::new(true, true).with_chat(true, true).hides_kind(EventKind::MessageEdited));
        assert!(Hidden::new(true, true).with_follow_and_edits(true, false).hides_kind(EventKind::MessageEdited));
        assert!(!Hidden::new(false, false).with_follow_and_edits(false, true).hides_kind(EventKind::MessageEdited));
    }

    #[test]
    fn every_client_keeps_getting_message_deleted_because_it_already_knows_it() {
        assert!(!Hidden::new(false, false).hides_kind(EventKind::MessageDeleted));
    }

    #[test]
    fn a_hello_names_the_follow_and_edit_flags_it_accepts() {
        let accepts = Accepts::of(&["ticket_follow".into(), "message_edits".into()]);
        assert_eq!(accepts, Accepts { ticket_follow: true, message_edits: true, ..Accepts::default() });
    }

    #[test]
    fn only_a_client_that_accepts_chat_threads_gets_an_answer_streaming_into_a_thread() {
        let in_thread = StreamDelta { thread_parent: Some(MessageId::new()), ..delta(MessageId::new(), vec![]) };
        assert!(Hidden::new(true, true).hides_stream(&in_thread));
        assert!(Hidden::new(true, true).with_chat(false, true).hides_stream(&in_thread));
        assert!(!Hidden::new(true, true).with_chat(true, false).hides_stream(&in_thread));
        assert!(!Hidden::new(false, false).hides_stream(&delta(MessageId::new(), vec![])), "the channel's own stream as before");
    }

    #[test]
    fn a_hello_names_the_chat_flags_it_accepts() {
        let accepts = Accepts::of(&["chat_threads".into(), "server_name".into()]);
        assert_eq!(accepts, Accepts { server_name: true, chat_threads: true, ..Accepts::default() });
        assert_eq!(Accepts::of(&[]), Accepts::default());
    }

    #[test]
    fn only_a_client_that_accepts_server_name_gets_server_renamed() {
        assert!(Hidden::new(true, false).hides_kind(EventKind::ServerRenamed));
        assert!(!Hidden::new(false, true).hides_kind(EventKind::ServerRenamed));
        assert!(!Hidden::new(false, false).hides_kind(EventKind::TaskCreated));
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
