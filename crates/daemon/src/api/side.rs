//! Side questions (feature `side_question`), like Claude Code's `/btw`.
//! `conversation.side_ask` answers at once with an id; the answer is written by
//! a model without tools, on the asker's own Claude login, and streams to this
//! connection only as `ServerMessage::SideAnswer`. Nothing is stored or logged:
//! no message, event, receipt or request hash, and never the question.
//! What the model reads is built in `aurelius_application::conversation::side_question`.

use super::{Peer, app_task, internal};
use crate::Shared;
use crate::tasks::{INTERRUPTED, StepError, stream_text};
use aurelius_application::conversation::side_question::side_prompt;
use aurelius_domain::RunId;
use aurelius_protocol::{ErrorCode, ProtocolError, ResponseBody, SIDE_QUESTION_MAX_CHARS, ServerMessage, SideAnswer, SideAskParams, SideAsked};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;

/// How many side questions one connection may have answering at once.
pub const SIDE_ASKS_PER_CONN_MAX: usize = 2;
/// The shortest time between two updates of one answer, in milliseconds.
pub const SIDE_PUSH_MS: u64 = 150;

const LOGIN_TEXT: &str = "Connect your Claude login to ask a side question.";
const FAILED_TEXT: &str = "The side question failed. Try again.";
const STOPPED_TEXT: &str = "Stopped.";

/// The side questions of one connection that are still answering, by side id.
#[derive(Default)]
pub(super) struct Sides {
    open: Arc<std::sync::Mutex<HashMap<String, RunId>>>,
}

impl Sides {
    /// Stop one side question; unknown or finished ids are fine.
    pub(super) fn cancel(&self, shared: &Arc<Shared>, side_id: &str) {
        let run = self.open.lock().ok().and_then(|open| open.get(side_id).copied());
        if let Some(run) = run {
            stop(shared, run);
        }
    }

    /// Stop every side question of this connection (it ended).
    pub(super) fn cancel_all(&self, shared: &Arc<Shared>) {
        let runs: Vec<RunId> = self.open.lock().map(|open| open.values().copied().collect()).unwrap_or_default();
        for run in runs {
            stop(shared, run);
        }
    }
}

fn stop(shared: &Arc<Shared>, run: RunId) {
    if let Ok(map) = shared.cancellers.lock()
        && let Some(canceller) = map.get(&run)
    {
        canceller.cancel();
    }
}

/// A board member with a session that needs no new password; apps never ask.
fn asker(peer: &Peer) -> Result<aurelius_domain::UserId, ProtocolError> {
    match (peer.session.as_ref(), peer.user) {
        (Some(session), Some(user)) if !session.must_change_password => Ok(user),
        (Some(_), Some(_)) => Err(ProtocolError::new(ErrorCode::PasswordChangeRequired, "Change your temporary password first.")),
        _ => Err(ProtocolError::new(ErrorCode::BoardMemberRequired, "Only a signed-in board member asks side questions.")),
    }
}

/// `conversation.side_ask`: check, read the conversation, start the answer and
/// return its id at once.
pub(super) async fn ask(shared: &Arc<Shared>, peer: &Peer, sides: &Sides, params: SideAskParams, out: mpsc::Sender<ServerMessage>) -> Result<ResponseBody, ProtocolError> {
    let reader = asker(peer)?;
    let question = params.question.trim().to_owned();
    if question.is_empty() || question.chars().count() > SIDE_QUESTION_MAX_CHARS {
        return Err(ProtocolError::new(ErrorCode::Validation, format!("A side question has 1 to {SIDE_QUESTION_MAX_CHARS} characters.")));
    }
    if sides.open.lock().map_err(internal)?.len() >= SIDE_ASKS_PER_CONN_MAX {
        return Err(ProtocolError::new(ErrorCode::Validation, "Wait for the side question that is still being answered."));
    }
    let conversation = params.conversation_id;
    let live = shared.live_text.lock().map(|l| l.clone()).unwrap_or_default();
    let s = shared.clone();
    let context = app_task(move || s.service.side_question_context(conversation, reader, &live)).await?;
    let Some(context) = context else {
        return Err(ProtocolError::new(ErrorCode::NotFound, "This conversation does not exist."));
    };
    // What a board member types passes the vault redactor before it is sent.
    let question = aurelius_runtime::redact::string(question);
    let (system, prompt) = side_prompt(&context, &question);
    let side_id = RunId::new().to_string();
    let run = RunId::new();
    sides.open.lock().map_err(internal)?.insert(side_id.clone(), run);
    let (shared, open, id) = (shared.clone(), sides.open.clone(), side_id.clone());
    tokio::spawn(async move {
        let mut last = std::time::Instant::now();
        let push = |text: &str, last: &mut std::time::Instant| {
            if last.elapsed().as_millis() < u128::from(SIDE_PUSH_MS) {
                return;
            }
            *last = std::time::Instant::now();
            let text = aurelius_runtime::redact::string(text.to_owned());
            // A full outbox skips this update: the next one carries the whole text.
            let _ = out.try_send(ServerMessage::SideAnswer(SideAnswer { side_id: id.clone(), text, done: false, error: None }));
        };
        let result = stream_text(&shared, &context.config, run, system, prompt, |text| push(text, &mut last)).await;
        let (text, error) = match result {
            Ok((text, _)) => (aurelius_runtime::redact::string(text), None),
            Err(StepError::Login) => (String::new(), Some(LOGIN_TEXT)),
            Err(StepError::Failed(why)) if why == INTERRUPTED => (String::new(), Some(STOPPED_TEXT)),
            Err(e) => {
                tracing::warn!(error = %e, "side question failed");
                (String::new(), Some(FAILED_TEXT))
            }
        };
        if let Ok(mut open) = open.lock() {
            open.remove(&id);
        }
        let _ = out.send(ServerMessage::SideAnswer(SideAnswer { side_id: id, text, done: true, error: error.map(str::to_owned) })).await;
    });
    Ok(ResponseBody::SideAsked(SideAsked { side_id }))
}
