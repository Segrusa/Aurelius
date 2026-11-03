//! A conversation goes on in its own runtime session (Claude Code's `--resume`):
//! the next run sends only the new messages (no earlier images), with a note
//! when the context changed. Anything else starts a new session from the whole
//! conversation, and a session goes when its conversation is over.

mod common;
use common::*;

use aurelius_client::{Client, bridge::BridgeClient, envelope};
use aurelius_domain::{
    Command, ConversationId, MessageId, RunId,
    connection::{AddProvider, ConfigScope, ProviderKind, Purpose, RunDefaults, SetRunDefaults},
    conversation::{DeleteConversation, MessageStatus, RetryReply, SendChatMessage, StopRun},
    approval::{DecidePlan, PlanDecision},
    employee::{CreateEmployee, SendEmployeeMessage, Setting},
    memory::{MemoryKind, MemoryScope, Remember},
    project::CreateProject,
    task::{CreateTask, TaskStatus},
};
use aurelius_protocol::{CommandResult, EmployeeTaskOp};
use aurelius_runtime::{AgentRuntime, RunHandle, RunRequest, RuntimeCapabilities, RuntimeError, RuntimeEvent, SessionUse, ToolProfile, fake};
use std::collections::HashSet;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;

/// What one run asked of the runtime.
#[derive(Debug, Clone)]
struct Seen {
    run: RunId,
    session: SessionUse,
    session_id: Option<String>,
    keep: bool,
    text: String,
    images: usize,
    /// The system text it was given.
    system: String,
}

/// A runtime that keeps sessions like Claude Code does (in memory), answers like
/// the fake, and holds a run whose last turn ends with `[hold]` until it is stopped.
#[derive(Clone, Default)]
struct Sessions {
    seen: Arc<Mutex<Vec<Seen>>>,
    kept: Arc<Mutex<HashSet<String>>>,
    removed: Arc<Mutex<Vec<String>>>,
    limited: Arc<Mutex<HashSet<String>>>,
    /// The first run whose turns hold `[wait]` waits for this (the test acts during it).
    release: Arc<tokio::sync::Notify>,
    waited: Arc<std::sync::atomic::AtomicBool>,
}

impl Sessions {
    fn seen(&self) -> Vec<Seen> {
        self.seen.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl AgentRuntime for Sessions {
    async fn probe(&self) -> RuntimeCapabilities {
        fake::FakeRuntime::default().probe().await
    }

    async fn start(&self, request: RunRequest) -> Result<RunHandle, RuntimeError> {
        let id = request.session_id.clone().unwrap_or_else(|| request.run_id.to_string());
        if matches!(request.profile, ToolProfile::Work { .. }) {
            self.seen.lock().unwrap().push(Seen {
                run: request.run_id,
                session: request.session,
                session_id: request.session_id.clone(),
                keep: request.keep_session,
                text: request.turns.iter().map(|t| t.text.as_str()).collect::<Vec<_>>().join("\n---\n"),
                images: request.turns.iter().map(|t| t.images.len()).sum(),
                system: request.system.clone().unwrap_or_default(),
            });
        }
        if request.session == SessionUse::Continue && !self.kept.lock().unwrap().contains(&id) {
            return Err(RuntimeError::Unavailable("session gone".into()));
        }
        if request.turns.last().is_some_and(|t| t.text.trim() == "/compact") {
            // Compacting the session, as Claude Code reports it; the session stays.
            let (tx, rx) = mpsc::channel(8);
            let (input_tx, _input_rx) = mpsc::channel(1);
            let (cancel_tx, _cancel_rx) = tokio::sync::watch::channel(false);
            tokio::spawn(async move {
                let _ = tx.send(RuntimeEvent::Started).await;
                let _ = tx.send(RuntimeEvent::Compacted { pre_tokens: 170_258, post_tokens: 6_036 }).await;
                let _ = tx.send(RuntimeEvent::Completed).await;
            });
            return Ok(RunHandle::new(request.run_id, rx, input_tx, cancel_tx));
        }
        let hold = request.turns.last().is_some_and(|t| t.text.trim_end().ends_with("[hold]"));
        // The first attempt of a run asking for it reaches the usage limit; Claude Code keeps its session.
        let limited = request.turns.iter().any(|t| t.text.contains("[limit]")) && self.limited.lock().unwrap().insert(request.run_id.to_string());
        let wait = matches!(request.profile, ToolProfile::Work { .. })
            && request.turns.iter().any(|t| t.text.contains("[wait]")) && !self.waited.swap(true, std::sync::atomic::Ordering::SeqCst);
        let release = self.release.clone();
        let text = fake::answer_for(&request);
        let (keep, kept) = (request.keep_session && request.session != SessionUse::None, self.kept.clone());
        let (tx, rx) = mpsc::channel(8);
        let (input_tx, _input_rx) = mpsc::channel(1);
        let (cancel_tx, mut cancel_rx) = tokio::sync::watch::channel(false);
        tokio::spawn(async move {
            let _ = tx.send(RuntimeEvent::Started).await;
            if limited {
                if keep {
                    kept.lock().unwrap().insert(id);
                }
                let _ = tx.send(RuntimeEvent::TextDelta("Half an answ".into())).await;
                let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;
                let _ = tx.send(RuntimeEvent::Quota { reset_at_ms: Some(now + 3_600_000) }).await;
                return;
            }
            if hold {
                let _ = cancel_rx.changed().await;
                // Stopped: Claude Code's session may hold half a turn; it is removed.
                kept.lock().unwrap().remove(&id);
                let _ = tx.send(RuntimeEvent::Interrupted).await;
                return;
            }
            if wait {
                release.notified().await;
            }
            if keep {
                kept.lock().unwrap().insert(id);
            }
            let _ = tx.send(RuntimeEvent::TextDelta(text)).await;
            let _ = tx.send(RuntimeEvent::Completed).await;
        });
        Ok(RunHandle::new(request.run_id, rx, input_tx, cancel_tx))
    }

    fn keeps_sessions(&self) -> bool {
        true
    }

    fn has_session(&self, id: &str) -> bool {
        self.kept.lock().unwrap().contains(id)
    }

    fn remove_session(&self, id: &str) {
        self.kept.lock().unwrap().remove(id);
        self.removed.lock().unwrap().push(id.to_string());
    }
}

/// Conversations and employees run on Claude Code (whose program is never started: the
/// test runtime answers), so they work in a folder like a real one.
async fn claude_code(client: &Client, root: &Path) {
    let add = AddProvider { provider: ProviderKind::ClaudeCode, display_name: "Claude Code".into(), runtime_path: Some(root.join("no-claude.exe").display().to_string()), default_model: None };
    let CommandResult::ProviderAdded { connection_id } = client.execute(envelope(Command::AddProvider(add), None)).await.unwrap().result else { panic!("not added") };
    for purpose in [Purpose::Employee] {
        let settings = RunDefaults { connection: Setting::Value(connection_id), model: Setting::Value("sonnet".into()), persona: Setting::Inherit, effort: Setting::Inherit };
        client.execute(envelope(Command::SetRunDefaults(SetRunDefaults { scope: ConfigScope::Global, purpose, settings }), None)).await.unwrap();
    }
}

async fn send(client: &Client, conversation_id: Option<ConversationId>, text: &str, images: Vec<String>) -> (ConversationId, MessageId, RunId) {
    let send = SendChatMessage { confirm_secret: false, conversation_id: match conversation_id { Some(id) => id, None => common::conversation(client).await }, text: text.into(), images, interrupt: false };
    match client.execute(envelope(Command::SendChatMessage(send), None)).await.unwrap().result {
        CommandResult::ChatMessageSent { conversation_id, message_id, run_id, .. } => (conversation_id, message_id, run_id),
        other => panic!("{other:?}"),
    }
}

/// Wait for the answer to `message` to end with `status`.
async fn answered(client: &Client, conversation: ConversationId, message: MessageId, status: MessageStatus) {
    for _ in 0..400 {
        let messages = client.messages(conversation).await.unwrap().messages;
        if messages.iter().any(|m| m.reply_to == Some(message) && m.status == status) {
            // The session is given back in the same transaction; the run's job ends just after.
            tokio::time::sleep(Duration::from_millis(50)).await;
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("no {status:?} answer to {message}");
}

/// Send and wait for the complete answer.
async fn ask(client: &Client, conversation: Option<ConversationId>, text: &str, images: Vec<String>) -> (ConversationId, RunId) {
    let (conversation, message, run) = send(client, conversation, text, images).await;
    answered(client, conversation, message, MessageStatus::Complete).await;
    (conversation, run)
}

/// (session id, busy run, seen count) of the conversation's session.
fn stored(root: &Path, conversation: ConversationId) -> Option<(String, Option<String>, i64)> {
    db(root)
        .query_row("SELECT session_id, busy_run_id, seen_count FROM conversation_sessions WHERE conversation_id = ?1", [conversation.to_string()], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .ok()
}

fn write(root: &Path, sql: &str, params: impl rusqlite::Params) {
    let conn = rusqlite::Connection::open(root.join("data/aurelius.sqlite")).unwrap();
    conn.busy_timeout(Duration::from_secs(5)).unwrap();
    conn.execute(sql, params).unwrap();
}

/// A 1×1 PNG.
fn screenshot(root: &Path) -> String {
    use base64::Engine;
    let png = base64::engine::general_purpose::STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==").unwrap();
    let path = root.join("screenshot.png");
    std::fs::write(&path, png).unwrap();
    path.display().to_string()
}

#[tokio::test(flavor = "multi_thread")]
async fn the_next_message_goes_on_in_the_session_without_the_earlier_screenshot() {
    let dir = tempfile::tempdir().unwrap();
    let runtime = Sessions::default();
    let daemon = start_with(dir.path(), Arc::new(runtime.clone())).await;
    let client = connect(dir.path()).await;
    claude_code(&client, dir.path()).await;

    let (conversation, first) = ask(&client, None, "What is wrong on this screen?", vec![screenshot(dir.path())]).await;
    let (_, second) = ask(&client, Some(conversation), "And how do I fix the login page?", vec![]).await;
    let (_, third) = ask(&client, Some(conversation), "Thanks, one more thing.", vec![]).await;

    let seen = runtime.seen();
    assert_eq!(seen.len(), 3, "{seen:?}");
    // The first run starts the conversation's session, named after itself, with the screenshot.
    assert_eq!((seen[0].run, seen[0].session, seen[0].session_id.clone(), seen[0].keep), (first, SessionUse::Keep, Some(first.to_string()), true));
    assert_eq!(seen[0].images, 1);
    // The next ones go on in it with only their own message: no earlier image, no earlier text.
    for (s, run, text) in [(&seen[1], second, "And how do I fix the login page?"), (&seen[2], third, "Thanks, one more thing.")] {
        assert_eq!((s.run, s.session, s.session_id.clone(), s.keep), (run, SessionUse::Continue, Some(first.to_string()), true), "{s:?}");
        assert_eq!(s.images, 0, "the screenshot is not sent again");
        // Only the new message (a memory job between the runs may add a note in front).
        assert!(s.text.ends_with(text) && !s.text.contains("What is wrong") && !s.text.contains("Earlier in this conversation"), "{s:?}");
    }
    // The session has the three questions and the first two answers before its last answer.
    assert_eq!(stored(dir.path(), conversation), Some((first.to_string(), None, 5)));
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_changed_history_a_lost_session_or_another_model_starts_a_new_session() {
    let dir = tempfile::tempdir().unwrap();
    let runtime = Sessions::default();
    let daemon = start_with(dir.path(), Arc::new(runtime.clone())).await;
    let client = connect(dir.path()).await;
    claude_code(&client, dir.path()).await;
    let (conversation, first) = ask(&client, None, "First question", vec![]).await;

    // The runtime lost the session (e.g. the user cleared Claude Code's history).
    runtime.kept.lock().unwrap().clear();
    let (_, lost) = ask(&client, Some(conversation), "Second question", vec![]).await;
    // An earlier answer is no longer what the session gave (here: marked interrupted).
    write(dir.path(), "UPDATE messages SET status = 'interrupted' WHERE run_id = ?1", [first.to_string()]);
    let (_, edited) = ask(&client, Some(conversation), "Third question", vec![]).await;
    // The session was made with another model.
    write(dir.path(), "UPDATE conversation_sessions SET model = 'opus' WHERE conversation_id = ?1", [conversation.to_string()]);
    let (_, model) = ask(&client, Some(conversation), "Fourth question", vec![]).await;
    // And with nothing changed it goes on again.
    let (_, goes_on) = ask(&client, Some(conversation), "Fifth question", vec![]).await;

    let seen = runtime.seen();
    assert_eq!(seen.len(), 5, "{seen:?}");
    for (s, run, earlier) in [(&seen[1], lost, "Second question"), (&seen[2], edited, "Second question"), (&seen[3], model, "Third question")] {
        assert_eq!((s.run, s.session, s.session_id.clone(), s.keep), (run, SessionUse::Keep, Some(run.to_string()), true), "a new session: {s:?}");
        assert!(s.text.contains("First question") && s.text.contains(earlier), "the whole conversation: {s:?}");
    }
    assert_eq!((seen[4].run, seen[4].session, seen[4].session_id.clone()), (goes_on, SessionUse::Continue, Some(model.to_string())));
    assert!(seen[4].text.ends_with("Fifth question") && !seen[4].text.contains("First question"), "{:?}", seen[4]);
    // Each new session is the conversation's; the ones it replaced went.
    assert_eq!(stored(dir.path(), conversation).map(|s| s.0), Some(model.to_string()));
    let removed = runtime.removed.lock().unwrap().clone();
    assert!(removed.contains(&first.to_string()) && removed.contains(&lost.to_string()) && removed.contains(&edited.to_string()), "{removed:?}");
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_changed_mode_is_told_in_front_of_the_new_message() {
    let dir = tempfile::tempdir().unwrap();
    let runtime = Sessions::default();
    let daemon = start_with(dir.path(), Arc::new(runtime.clone())).await;
    let client = connect(dir.path()).await;
    claude_code(&client, dir.path()).await;
    let (conversation, _) = ask(&client, None, "Plan the work", vec![]).await;

    write(dir.path(), "UPDATE conversations SET agent_mode = 'auto' WHERE id = ?1", [conversation.to_string()]);
    ask(&client, Some(conversation), "Go ahead", vec![]).await;
    // Told once: the session knows it from then on.
    ask(&client, Some(conversation), "And the tests?", vec![]).await;

    let seen = runtime.seen();
    assert_eq!(seen[1].session, SessionUse::Continue);
    assert!(seen[1].text.starts_with("(Since your last answer: Your working mode is now Auto.") && seen[1].text.ends_with(")\n\nGo ahead"), "{:?}", seen[1]);
    assert_eq!(seen[2].session, SessionUse::Continue);
    assert!(seen[2].text.ends_with("And the tests?") && !seen[2].text.contains("working mode"), "{:?}", seen[2]);
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn after_a_stopped_answer_the_next_run_starts_over() {
    let dir = tempfile::tempdir().unwrap();
    let runtime = Sessions::default();
    let daemon = start_with(dir.path(), Arc::new(runtime.clone())).await;
    let client = connect(dir.path()).await;
    claude_code(&client, dir.path()).await;
    let (conversation, first) = ask(&client, None, "First question", vec![]).await;

    let (_, message, held) = send(&client, Some(conversation), "Take your time [hold]", vec![]).await;
    for _ in 0..400 {
        if runtime.seen().len() == 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert_eq!(stored(dir.path(), conversation).and_then(|s| s.1), Some(held.to_string()), "the run holds the session while it goes on");
    client.execute(envelope(Command::StopRun(StopRun { run_id: held }), None)).await.unwrap();
    answered(&client, conversation, message, MessageStatus::Interrupted).await;
    assert_eq!(stored(dir.path(), conversation), None, "a stopped run's session no longer matches the conversation");

    let (_, next) = ask(&client, Some(conversation), "Never mind", vec![]).await;
    let seen = runtime.seen();
    assert_eq!((seen[1].session, seen[1].session_id.clone()), (SessionUse::Continue, Some(first.to_string())));
    assert_eq!((seen[2].session, seen[2].session_id.clone()), (SessionUse::Keep, Some(next.to_string())), "{:?}", seen[2]);
    assert!(seen[2].text.contains("First question"));
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_deleted_conversation_keeps_no_session() {
    let dir = tempfile::tempdir().unwrap();
    let runtime = Sessions::default();
    let daemon = start_with(dir.path(), Arc::new(runtime.clone())).await;
    let client = connect(dir.path()).await;
    claude_code(&client, dir.path()).await;
    let (conversation, first) = ask(&client, None, "Hello", vec![]).await;
    assert_eq!(stored(dir.path(), conversation).map(|s| s.0), Some(first.to_string()));

    let result = client.execute(envelope(Command::DeleteConversation(DeleteConversation { conversation_id: conversation }), None)).await.unwrap().result;
    assert!(matches!(result, CommandResult::ConversationDeleted { .. }), "{result:?}");
    // The sweep runs in the background.
    for _ in 0..200 {
        if stored(dir.path(), conversation).is_none() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert_eq!(stored(dir.path(), conversation), None);
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_task_conversation_goes_on_in_its_session_after_the_task_succeeded() {
    let dir = tempfile::tempdir().unwrap();
    let runtime = Sessions::default();
    let daemon = start_with(dir.path(), Arc::new(runtime.clone())).await;
    let client = connect(dir.path()).await;
    claude_code(&client, dir.path()).await;
    let CommandResult::ProjectCreated { project_id, .. } =
        client.execute(envelope(Command::CreateProject(CreateProject { name: "Atlas".into(), description: None, folder: None }), None)).await.unwrap().result
    else {
        panic!()
    };
    let create = CreateEmployee { name: "Alex".into(), role: "Developer".into(), title: None, reports_to: None, instructions: None, folder: None };
    let CommandResult::EmployeeCreated { employee_id, .. } = client.execute(envelope(Command::CreateEmployee(create), None)).await.unwrap().result else { panic!() };
    let cmd = CreateTask { confirm_secret: false, project_id, employee_id: Some(employee_id), title: "Login page".into(), goal: "Fix the login page".into(), acceptance_criteria: vec![], depends_on: vec![], run: Default::default(), files: vec![] };
    let CommandResult::TaskCreated { task_id } = client.execute(envelope(Command::CreateTask(cmd), None)).await.unwrap().result else { panic!() };
    // The task succeeds at its first answer.
    let mut task = None;
    for _ in 0..400 {
        task = client.tasks(None).await.unwrap().tasks.into_iter().find(|t| t.id == task_id && t.status == TaskStatus::Succeeded);
        if task.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let conversation = task.expect("the task succeeded").conversation_id.expect("the task's conversation");
    tokio::time::sleep(Duration::from_millis(100)).await;
    let first = runtime.seen()[0].run;
    assert_eq!(stored(dir.path(), conversation).map(|s| s.0), Some(first.to_string()), "an ended task keeps its conversation's session");

    // The user goes on talking in the task's conversation.
    let say = |text: &'static str, images: Vec<String>| {
        let client = &client;
        async move {
            let send = SendEmployeeMessage { confirm_secret: false, employee_id, text: text.into(), conversation_id: conversation, images, files: Vec::new(), interrupt: false };
            let CommandResult::EmployeeMessageSent { message_id, run_id, .. } = client.execute(envelope(Command::SendEmployeeMessage(send), None)).await.unwrap().result else { panic!() };
            answered(client, conversation, message_id, MessageStatus::Complete).await;
            run_id
        }
    };
    let second = say("Also check the logout button", vec![screenshot(dir.path())]).await;
    let third = say("Thanks", vec![]).await;
    let seen = runtime.seen();
    assert_eq!(seen.len(), 3, "{seen:?}");
    assert_eq!((seen[0].session, seen[0].session_id.clone()), (SessionUse::Keep, Some(first.to_string())));
    for (s, run, text) in [(&seen[1], second, "Also check the logout button"), (&seen[2], third, "Thanks")] {
        assert_eq!((s.run, s.session, s.session_id.clone()), (run, SessionUse::Continue, Some(first.to_string())), "{s:?}");
        assert!(s.text.ends_with(text) && !s.text.contains("Fix the login page"), "only the new message: {s:?}");
    }
    assert_eq!(seen[2].images, 0, "the earlier screenshot is not sent again");
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_first_message_after_a_retry_goes_on_in_the_retrys_session() {
    let dir = tempfile::tempdir().unwrap();
    let runtime = Sessions::default();
    let daemon = start_with(dir.path(), Arc::new(runtime.clone())).await;
    let client = connect(dir.path()).await;
    claude_code(&client, dir.path()).await;
    let (conversation, question, _) = send(&client, None, "First question", vec![]).await;
    answered(&client, conversation, question, MessageStatus::Complete).await;

    let retry = RetryReply { conversation_id: conversation, user_message_id: question };
    let CommandResult::ReplyRetried { run_id: retried } = client.execute(envelope(Command::RetryReply(retry), None)).await.unwrap().result else { panic!() };
    for _ in 0..400 {
        if stored(dir.path(), conversation).is_some_and(|s| s.0 == retried.to_string() && s.1.is_none()) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    tokio::time::sleep(Duration::from_millis(50)).await;
    let (_, next) = ask(&client, Some(conversation), "Second question", vec![]).await;

    let seen = runtime.seen();
    assert_eq!(seen.len(), 3, "{seen:?}");
    // The retry starts over (the session had the old answer) ...
    assert_eq!((seen[1].run, seen[1].session, seen[1].session_id.clone()), (retried, SessionUse::Keep, Some(retried.to_string())));
    // ... and the next message goes on in the retry's session.
    assert_eq!((seen[2].run, seen[2].session, seen[2].session_id.clone()), (next, SessionUse::Continue, Some(retried.to_string())), "{:?}", seen[2]);
    assert!(seen[2].text.ends_with("Second question") && !seen[2].text.contains("First question"), "{:?}", seen[2]);
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn after_the_usage_limit_the_run_carries_on_in_the_conversations_session() {
    let dir = tempfile::tempdir().unwrap();
    let runtime = Sessions::default();
    let daemon = start_with(dir.path(), Arc::new(runtime.clone())).await;
    let client = connect(dir.path()).await;
    claude_code(&client, dir.path()).await;
    let (conversation, message, run) = send(&client, None, "A big job [limit]", vec![]).await;
    for _ in 0..400 {
        let status: Option<String> = db(dir.path()).query_row("SELECT status FROM runs WHERE id = ?1", [run.to_string()], |r| r.get(0)).ok();
        if status.as_deref() == Some("waiting") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert_eq!(stored(dir.path(), conversation).and_then(|s| s.1), Some(run.to_string()), "the parked run keeps holding the session");

    // The limit resets.
    write(dir.path(), "UPDATE jobs SET not_before_ms = 0 WHERE state = 'ready'", ());
    answered(&client, conversation, message, MessageStatus::Complete).await;
    let (_, next) = ask(&client, Some(conversation), "Done?", vec![]).await;

    let seen = runtime.seen();
    assert_eq!((seen[0].session, seen[0].session_id.clone(), seen[0].keep), (SessionUse::Keep, Some(run.to_string()), true));
    // The same session, carried on where the limit stopped it (the note is there for when it is gone).
    assert_eq!((seen[1].run, seen[1].session, seen[1].session_id.clone(), seen[1].keep), (run, SessionUse::Resume, Some(run.to_string()), true), "{:?}", seen[1]);
    assert!(seen[1].text.contains("usage limit"));
    // And the conversation goes on in it.
    assert_eq!((seen[2].run, seen[2].session, seen[2].session_id.clone()), (next, SessionUse::Continue, Some(run.to_string())), "{:?}", seen[2]);
    assert_eq!(stored(dir.path(), conversation).map(|s| (s.0, s.1)), Some((run.to_string(), None)));
    stop(dir.path(), daemon).await;
}

/// The runs of `conversation` the runtime saw, in order.
fn seen_in(root: &Path, runtime: &Sessions, conversation: ConversationId) -> Vec<Seen> {
    let conn = db(root);
    let mut stmt = conn.prepare("SELECT id FROM runs WHERE conversation_id = ?1").unwrap();
    let runs: HashSet<String> = stmt.query_map([conversation.to_string()], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
    runtime.seen().into_iter().filter(|s| runs.contains(&s.run.to_string())).collect()
}

/// The run of `conversation` (or of `task`'s) that is going on.
async fn running(root: &Path, column: &str, id: String) -> RunId {
    let conn = db(root);
    for _ in 0..400 {
        if let Ok(run) = conn.query_row(&format!("SELECT id FROM runs WHERE {column} = ?1 AND conversation_id IS NOT NULL AND status = 'running'"), [&id], |r| r.get::<_, String>(0)) {
            return RunId::parse(&run).unwrap();
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("no running run for {column} {id}");
}

async fn delegate(root: &Path, run: RunId, to: &str, title: &str) {
    let op = EmployeeTaskOp::Delegate { employee: to.into(), title: title.into(), goal: format!("Werk aan {title}"), criteria: vec![], project: None, ticket: Some("#1".into()) };
    let (ok, text) = BridgeClient::connect(root).await.unwrap().employee_act(run, op).await.unwrap();
    assert!(ok, "{text}");
}

#[tokio::test(flavor = "multi_thread")]
async fn after_a_colleagues_result_the_employee_goes_on_in_its_session() {
    use aurelius_domain::{conversation::ChatStart, project_conversation::SendProjectMessage};
    let dir = tempfile::tempdir().unwrap();
    let runtime = Sessions::default();
    let daemon = start_with(dir.path(), Arc::new(runtime.clone())).await;
    let client = connect(dir.path()).await;
    claude_code(&client, dir.path()).await;
    free_chart(&client).await;
    let CommandResult::ProjectCreated { project_id, .. } =
        client.execute(envelope(Command::CreateProject(CreateProject { name: "Atlas".into(), description: None, folder: None }), None)).await.unwrap().result
    else {
        panic!()
    };
    backlog_ticket(&client, project_id, "Shared work").await;
    let mut alex = None;
    for name in ["Alex", "Iris"] {
        let create = CreateEmployee { name: name.into(), role: "Developer".into(), title: None, reports_to: None, instructions: None, folder: None };
        let CommandResult::EmployeeCreated { employee_id, .. } = client.execute(envelope(Command::CreateEmployee(create), None)).await.unwrap().result else { panic!() };
        alex = alex.or(Some(employee_id));
    }

    // A task conversation: Alex gives a part to Iris while he works, and goes on when it is done.
    let cmd = CreateTask { confirm_secret: false, project_id, employee_id: alex, title: "Dark mode".into(), goal: "Fix dark mode [wait]".into(), acceptance_criteria: vec![], depends_on: vec![], run: Default::default(), files: vec![] };
    let CommandResult::TaskCreated { task_id } = client.execute(envelope(Command::CreateTask(cmd), None)).await.unwrap().result else { panic!() };
    let first = running(dir.path(), "task_id", task_id.to_string()).await;
    delegate(dir.path(), first, "Iris", "Dark mode UI").await;
    runtime.release.notify_one();
    let mut task = None;
    for _ in 0..600 {
        task = client.tasks(None).await.unwrap().tasks.into_iter().find(|t| t.id == task_id && t.status == TaskStatus::Succeeded);
        if task.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let conversation = task.expect("the task succeeded").conversation_id.expect("the task's conversation");
    let seen = seen_in(dir.path(), &runtime, conversation);
    assert_eq!(seen.len(), 2, "{seen:?}");
    assert_eq!((seen[0].run, seen[0].session), (first, SessionUse::Keep));
    assert_eq!((seen[1].session, seen[1].session_id.clone()), (SessionUse::Continue, Some(first.to_string())), "{:?}", seen[1]);
    assert!(seen[1].text.contains("Finished “Dark mode UI”") && !seen[1].text.contains("[wait]"), "only the result and what follows: {:?}", seen[1]);

    // A project conversation: the assistant gives work to Iris; her result is answered in its session.
    runtime.waited.store(false, std::sync::atomic::Ordering::SeqCst);
    let send = SendProjectMessage { confirm_secret: false, project_id, text: "A logo please [wait]".into(), conversation_id: None, images: Vec::new(), files: Vec::new(), interrupt: false, start: ChatStart::default() };
    let CommandResult::ProjectMessageSent { conversation_id, run_id: asked, .. } = client.send_project_message(send).await.unwrap().result else { panic!() };
    running(dir.path(), "id", asked.to_string()).await;
    delegate(dir.path(), asked, "Iris", "Logo").await;
    runtime.release.notify_one();
    let conn = db(dir.path());
    for _ in 0..600 {
        let done: i64 = conn.query_row("SELECT COUNT(*) FROM runs WHERE conversation_id = ?1 AND status = 'succeeded'", [conversation_id.to_string()], |r| r.get(0)).unwrap();
        if done == 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let seen = seen_in(dir.path(), &runtime, conversation_id);
    assert_eq!(seen.len(), 2, "{seen:?}");
    assert_eq!((seen[0].run, seen[0].session), (asked, SessionUse::Keep));
    assert_eq!((seen[1].session, seen[1].session_id.clone()), (SessionUse::Continue, Some(asked.to_string())), "{:?}", seen[1]);
    assert!(seen[1].text.contains("Finished “Logo”") && !seen[1].text.contains("A logo please"), "only the result: {:?}", seen[1]);
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_session_keeps_its_system_text_and_is_told_what_is_new() {
    let dir = tempfile::tempdir().unwrap();
    let runtime = Sessions::default();
    let daemon = start_with(dir.path(), Arc::new(runtime.clone())).await;
    let client = connect(dir.path()).await;
    claude_code(&client, dir.path()).await;
    let (conversation, _) = ask(&client, None, "First question", vec![]).await;

    // A new decision: not in the session's system text, so told once with the next message.
    let remember = Remember { scope: MemoryScope::Global, kind: MemoryKind::Decision, content: "Always answer in metric units".into(), pinned: false };
    client.execute(envelope(Command::Remember(remember), None)).await.unwrap();
    ask(&client, Some(conversation), "Second question", vec![]).await;
    ask(&client, Some(conversation), "Third question", vec![]).await;
    // A day later the session starts over, with the current system text.
    write(dir.path(), "UPDATE conversation_sessions SET system_at_ms = system_at_ms - 25 * 3600000 WHERE conversation_id = ?1", [conversation.to_string()]);
    ask(&client, Some(conversation), "Fourth question", vec![]).await;

    let seen = runtime.seen();
    assert_eq!(seen.len(), 4, "{seen:?}");
    assert!(!seen[0].system.contains("metric units"));
    for s in &seen[1..3] {
        assert_eq!(s.session, SessionUse::Continue, "{s:?}");
        assert_eq!(s.system, seen[0].system, "byte for byte the session's own system text");
    }
    assert!(seen[1].text.contains("New or now relevant memories") && seen[1].text.contains("Always answer in metric units") && seen[1].text.ends_with("Second question"), "{:?}", seen[1]);
    assert!(!seen[2].text.contains("metric units") && seen[2].text.ends_with("Third question"), "told once: {:?}", seen[2]);
    assert_eq!(seen[3].session, SessionUse::Keep, "{:?}", seen[3]);
    assert!(seen[3].system.contains("Always answer in metric units") && seen[3].text.contains("First question"), "{:?}", seen[3]);
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn after_an_answer_that_posted_a_plan_the_session_goes_on() {
    let dir = tempfile::tempdir().unwrap();
    let runtime = Sessions::default();
    let daemon = start_with(dir.path(), Arc::new(runtime.clone())).await;
    let client = connect(dir.path()).await;
    claude_code(&client, dir.path()).await;
    // The run hands in a plan while it answers; the user approves it; then it finishes.
    let (conversation, message, run) = send(&client, None, "Plan it [wait]", vec![]).await;
    for _ in 0..400 {
        let status: String = db(dir.path()).query_row("SELECT status FROM runs WHERE id = ?1", [run.to_string()], |r| r.get(0)).unwrap();
        if status == "running" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let bridge = BridgeClient::connect(dir.path()).await.unwrap();
    let asked = tokio::spawn(async move {
        bridge.permission_ask(run, Some("plan-1".into()), "ExitPlanMode".into(), serde_json::json!({ "plan": "# Plan\n\n1. Do it" })).await.unwrap()
    });
    let approval = loop {
        if let Some(a) = client.approvals().await.unwrap().approvals.into_iter().find(|a| a.tool == "claude.ExitPlanMode") {
            break a;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    };
    client.execute(envelope(Command::DecidePlan(DecidePlan { confirm_secret: false, approval_id: approval.id, decision: PlanDecision::Approve, feedback: None }), None)).await.unwrap();
    assert!(asked.await.unwrap().allow);
    runtime.release.notify_one();
    answered(&client, conversation, message, MessageStatus::Complete).await;
    // The answer moved below the plan and the decision.
    let messages = client.messages(conversation).await.unwrap().messages;
    assert!(messages.iter().position(|m| m.update.as_deref() == Some("plan")) < messages.iter().position(|m| m.reply_to == Some(message)), "{messages:?}");

    ask(&client, Some(conversation), "Next question", vec![]).await;
    let seen = runtime.seen();
    assert_eq!(seen.len(), 2, "{seen:?}");
    assert_eq!((seen[1].session, seen[1].session_id.clone()), (SessionUse::Continue, Some(run.to_string())), "{:?}", seen[1]);
    assert!(seen[1].text.ends_with("Next question") && !seen[1].text.contains("# Plan"), "the plan and decision are in the session: {:?}", seen[1]);
    assert_eq!(seen[1].system, seen[0].system);
    stop(dir.path(), daemon).await;
}

/// AUR-BUG-05: after compacting the session first, the run's own turn must still start
/// and answer (its start was recorded twice under one lease, which failed and left
/// the run "running" without anyone driving it), and the next message must go.
#[tokio::test(flavor = "multi_thread")]
async fn after_compacting_the_session_the_run_answers_and_the_next_message_goes() {
    let dir = tempfile::tempdir().unwrap();
    let runtime = Sessions::default();
    let daemon = start_with(dir.path(), Arc::new(runtime.clone())).await;
    let client = connect(dir.path()).await;
    claude_code(&client, dir.path()).await;

    let (conversation, first) = ask(&client, None, "Fix the team chat.", vec![]).await;
    // Its context grew past the threshold: the next run compacts it first.
    write(dir.path(), "UPDATE conversation_sessions SET context_tokens = 170258 WHERE conversation_id = ?1", [conversation.to_string()]);
    let (_, second) = ask(&client, Some(conversation), "Your colleagues finished their parts.", vec![]).await;
    let (_, third) = ask(&client, Some(conversation), "Hello?", vec![]).await;

    let seen = seen_in(dir.path(), &runtime, conversation);
    let of = |run: RunId| seen.iter().filter(|s| s.run == run).map(|s| s.text.clone()).collect::<Vec<_>>();
    let second_seen = of(second);
    assert_eq!(second_seen.len(), 2, "{seen:?}");
    assert_eq!(second_seen[0], "/compact");
    assert!(second_seen[1].ends_with("Your colleagues finished their parts."), "{second_seen:?}");
    assert!(seen.iter().filter(|s| s.run != first).all(|s| s.session_id.as_deref() == Some(first.to_string().as_str())), "it went on in the compacted session: {seen:?}");
    let conn = db(dir.path());
    let compacted: i64 = conn
        .query_row("SELECT COUNT(*) FROM messages WHERE conversation_id = ?1 AND json_extract(meta_json, '$.kind') = 'compacted'", [conversation.to_string()], |r| r.get(0))
        .unwrap();
    // (The test runtime reports no context size, so the third compacts again.)
    let compacts = seen.iter().filter(|s| s.text == "/compact").count() as i64;
    assert!(compacts >= 1);
    assert_eq!(compacted, compacts, "the conversation says each time it was compacted");
    for run in [second, third] {
        let (status, started): (String, i64) = conn
            .query_row("SELECT status, (SELECT COUNT(*) FROM events WHERE aggregate_id = ?1 AND type = 'run.started') FROM runs WHERE id = ?1", [run.to_string()], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert_eq!((status.as_str(), started), ("succeeded", 1), "{run}");
    }
    stop(dir.path(), daemon).await;
}
