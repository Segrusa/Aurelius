//! One device WebSocket ⇄ one daemon session. Same JSON as local IPC, one
//! message per text frame; full-text stream deltas become appends.

use crate::{Device, Hub};
use aurelius_domain::MessageId;
use aurelius_protocol::remote::StreamAppend;
use aurelius_protocol::{ActivityView, Request, ServerMessage, reject_unparsed};
use aurelius_storage::remote::RemoteClient;
use axum::extract::ws::{CloseFrame, Message, WebSocket};
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::mpsc;

/// Close code the app reads as "revoked or remote access turned off".
pub const CLOSE_REVOKED: u16 = 4401;

pub async fn serve(hub: Hub, client: RemoteClient, socket: WebSocket) {
    let Some(dispatch) = hub.inner.dispatch.get().cloned() else { return };
    let (req_tx, req_rx) = mpsc::channel::<Request>(16);
    let (out_tx, mut out_rx) = mpsc::channel::<ServerMessage>(256);
    // The guard keeps a device counted as connected while the socket is open.
    let (mut kill, _guard) = hub.connect(client.id);
    let inbox = hub.inbox(client.id);
    let _ = std::fs::create_dir_all(&inbox);
    dispatch.open(
        Device { client_id: client.id, name: client.name.clone(), capabilities: client.capabilities.clone(), inbox },
        req_rx,
        out_tx,
    );

    let (mut sink, mut stream) = socket.split();
    let mut appends = Appends::default();
    // Keeps NAT/Tailscale paths warm and notices dead phones.
    let mut ping = tokio::time::interval(Duration::from_secs(20));
    ping.tick().await;
    loop {
        tokio::select! {
            incoming = stream.next() => match incoming {
                Some(Ok(Message::Text(text))) => match parse(text.as_str()) {
                    Ok(request) => {
                        if req_tx.send(request).await.is_err() {
                            break;
                        }
                    }
                    Err(reply) => {
                        if send(&mut sink, &reply).await.is_err() {
                            break;
                        }
                    }
                },
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                Some(Ok(_)) => {}
            },
            outgoing = out_rx.recv() => match outgoing {
                Some(message) => {
                    if send(&mut sink, &appends.convert(message)).await.is_err() {
                        break;
                    }
                }
                None => break,
            },
            changed = kill.changed() => {
                if changed.is_err() || *kill.borrow() {
                    let _ = sink
                        .send(Message::Close(Some(CloseFrame { code: CLOSE_REVOKED, reason: "revoked".into() })))
                        .await;
                    break;
                }
            }
            _ = ping.tick() => {
                if sink.send(Message::Ping(Vec::new().into())).await.is_err() {
                    break;
                }
            }
        }
    }
    // Dropping `req_tx` ends the daemon session (and its event forwarder).
}

/// A request, or the error to answer for its own id (unknown method, bad params, not JSON).
#[allow(clippy::result_large_err)] // once per bad frame; boxing buys nothing
fn parse(text: &str) -> Result<Request, ServerMessage> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(|err| reject_unparsed(None, &err))?;
    serde_json::from_value(value.clone()).map_err(|err| reject_unparsed(Some(&value), &err))
}

async fn send<S>(sink: &mut S, message: &ServerMessage) -> Result<(), ()>
where
    S: SinkExt<Message> + Unpin,
{
    let text = serde_json::to_string(message).map_err(|_| ())?;
    sink.send(Message::Text(text.into())).await.map_err(|_| ())
}

/// Turns full-text deltas into appends. Offsets count UTF-16 code units so a
/// Dart client can splice its `String` directly. Tool activity rides along
/// only when it changed.
#[derive(Default)]
pub struct Appends {
    sent: HashMap<MessageId, (String, Vec<ActivityView>)>,
}

impl Appends {
    pub fn convert(&mut self, message: ServerMessage) -> ServerMessage {
        let ServerMessage::Stream(delta) = message else {
            if let ServerMessage::Event(event) = &message {
                // A finished message is refetched by the client; forget its text.
                if matches!(event.kind.area(), "message" | "run") {
                    self.sent.clear();
                }
            }
            return message;
        };
        let previous = self.sent.get(&delta.message_id);
        let activity = match previous {
            // Already only the changes (the session asked for them): none, or these.
            _ if delta.activity_changes => (!delta.activity.is_empty()).then(|| delta.activity.clone()),
            Some((_, prev)) if *prev == delta.activity => None,
            None if delta.activity.is_empty() => None,
            _ => Some(delta.activity.clone()),
        };
        let append = match previous {
            Some((prev, _)) if delta.text.starts_with(prev.as_str()) => StreamAppend {
                run_id: delta.run_id,
                conversation_id: delta.conversation_id,
                message_id: delta.message_id,
                offset: prev.encode_utf16().count() as u64,
                text: delta.text[prev.len()..].to_owned(),
                activity,
                progress: delta.progress,
                activity_changes: delta.activity_changes,
                mode: delta.mode,
                thread_parent: delta.thread_parent,
                draft: delta.draft.clone(),
            },
            _ => StreamAppend {
                run_id: delta.run_id,
                conversation_id: delta.conversation_id,
                message_id: delta.message_id,
                offset: 0,
                text: delta.text.clone(),
                activity,
                progress: delta.progress,
                activity_changes: delta.activity_changes,
                mode: delta.mode,
                thread_parent: delta.thread_parent,
                draft: delta.draft.clone(),
            },
        };
        if self.sent.len() > 64 {
            self.sent.clear();
        }
        self.sent.insert(delta.message_id, (delta.text, delta.activity));
        ServerMessage::StreamAppend(append)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aurelius_domain::{ConversationId, RunId};
    use aurelius_protocol::StreamDelta;

    #[test]
    fn a_bad_request_is_answered_for_its_own_id() {
        use aurelius_protocol::{ErrorCode, Outcome};
        let code_of = |text: &str| match parse(text) {
            Err(ServerMessage::Response { request_id, outcome: Outcome::Error(e) }) => (request_id, e.code),
            other => panic!("{other:?}"),
        };
        assert_eq!(code_of(r#"{"protocol":2,"request_id":7,"method":"mcp.login","params":{}}"#), (7, ErrorCode::UnknownMethod));
        assert_eq!(code_of("{oops"), (0, ErrorCode::InvalidRequest));
        assert!(parse(r#"{"protocol":2,"request_id":8,"method":"server.get_health"}"#).is_ok());
    }

    fn delta(id: MessageId, text: &str) -> ServerMessage {
        delta_with(id, text, vec![])
    }

    fn delta_with(id: MessageId, text: &str, activity: Vec<ActivityView>) -> ServerMessage {
        ServerMessage::Stream(StreamDelta { run_id: RunId::new(), conversation_id: ConversationId::new(), message_id: id, text: text.into(), activity, progress: None, activity_changes: false, mode: None, thread_parent: None, draft: None })
    }

    fn tool(call: &str, ok: Option<bool>) -> ActivityView {
        ActivityView { call_id: call.into(), tool: "Bash".into(), input: serde_json::json!({ "command": "ls" }), output: None, ok, offset: 3, truncated: false, output_bytes: None, parent_call_id: None }
    }

    fn activity(message: ServerMessage) -> Option<Vec<ActivityView>> {
        match message {
            ServerMessage::StreamAppend(a) => a.activity,
            other => panic!("{other:?}"),
        }
    }

    fn append(message: ServerMessage) -> (u64, String) {
        match message {
            ServerMessage::StreamAppend(a) => (a.offset, a.text),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn deltas_become_appends_in_utf16_units() {
        let mut a = Appends::default();
        let id = MessageId::new();
        assert_eq!(append(a.convert(delta(id, "Hi 👋"))), (0, "Hi 👋".into()));
        // "Hi 👋" is 5 UTF-16 units (the emoji is a surrogate pair).
        assert_eq!(append(a.convert(delta(id, "Hi 👋 there"))), (5, " there".into()));
        // A rewrite (not a continuation) resends the full text.
        assert_eq!(append(a.convert(delta(id, "Hello"))), (0, "Hello".into()));
    }

    #[test]
    fn activity_is_sent_only_when_it_changes() {
        let mut a = Appends::default();
        let id = MessageId::new();
        assert_eq!(activity(a.convert(delta(id, "Hi"))), None);
        assert_eq!(activity(a.convert(delta_with(id, "Hi", vec![tool("1", None)]))), Some(vec![tool("1", None)]));
        assert_eq!(activity(a.convert(delta_with(id, "Hi there", vec![tool("1", None)]))), None);
        assert_eq!(activity(a.convert(delta_with(id, "Hi there", vec![tool("1", Some(true))]))), Some(vec![tool("1", Some(true))]));
    }
}
