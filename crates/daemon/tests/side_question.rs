//! Side questions (feature `side_question`), like Claude Code's `/btw`: a board
//! member asks about a session while it works; the answer streams to that
//! connection only and nothing is stored. Runs use the fake runtime, which
//! echoes the prompt it was given.

mod common;

use aurelius_client::{Client, ClientError, SessionPush};
use aurelius_domain::employee::CreateEmployee;
use aurelius_domain::project::CreateProject;
use aurelius_domain::work_session::StartWorkSession;
use aurelius_domain::{Command, CommandResult, ConversationId, ErrorCode};
use aurelius_protocol::{Method, ResponseBody, SIDE_QUESTION_MAX_CHARS, SideAnswer, SideAskParams};
use std::path::Path;
use std::time::Duration;
use tokio::sync::mpsc::UnboundedReceiver;

/// A word that is in the question only: it must end up nowhere on disk.
const MARKER: &str = "PINEAPPLE-SIDE-QUESTION";

fn code<T: std::fmt::Debug>(result: Result<T, ClientError>) -> ErrorCode {
    match result {
        Err(ClientError::Protocol(err)) => err.code,
        other => panic!("expected a protocol error, got {other:?}"),
    }
}

/// Grace starts a session with Orion in project Atlas and waits for his answer.
async fn session(root: &Path, grace: &Client) -> ConversationId {
    let folder = root.join("atlas");
    std::fs::create_dir_all(&folder).unwrap();
    let made = grace
        .execute(aurelius_client::envelope(Command::CreateProject(CreateProject { name: "Atlas".into(), description: None, folder: Some(folder.to_string_lossy().into_owned()) }), None))
        .await
        .unwrap()
        .result;
    let CommandResult::ProjectCreated { project_id, .. } = made else { panic!("{made:?}") };
    let create = CreateEmployee {
        access_level: None,
        tool_policy: None,
        start_profile: None,
        description: None,
        name: "Orion".into(),
        role: "Project Manager".into(),
        title: None,
        reports_to: None,
        instructions: None,
        folder: None,
    };
    let CommandResult::EmployeeCreated { employee_id, .. } = grace.execute(aurelius_client::envelope(Command::CreateEmployee(create), None)).await.unwrap().result else { panic!() };
    let start = StartWorkSession {
        project_id,
        employee_id,
        text: "Make the export faster.".into(),
        images: vec![],
        files: vec![],
        worktree: false,
        own_branch: false,
        run: Default::default(),
        confirm_secret: false,
        design: false,
    };
    let made = grace.execute(aurelius_client::envelope(Command::StartWorkSession(start), None)).await.unwrap().result;
    let CommandResult::WorkSessionStarted { conversation_id, .. } = made else { panic!("{made:?}") };
    for _ in 0..400 {
        let snapshot = grace.messages(conversation_id).await.unwrap();
        if snapshot.messages.iter().any(|m| m.role == aurelius_domain::conversation::MessageRole::Assistant && m.status == aurelius_domain::conversation::MessageStatus::Complete) {
            return conversation_id;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("Orion never answered");
}

async fn ask(client: &Client, conversation_id: ConversationId, question: &str) -> Result<String, ClientError> {
    match client.call(Method::ConversationSideAsk(SideAskParams { conversation_id, question: question.into() })).await? {
        ResponseBody::SideAsked(asked) => Ok(asked.side_id),
        other => panic!("not a side id: {other:?}"),
    }
}

/// Every update of `side_id` until the one that is done.
async fn answers(pushes: &mut UnboundedReceiver<SessionPush>, side_id: &str) -> Vec<SideAnswer> {
    let mut seen = Vec::new();
    loop {
        let push = tokio::time::timeout(Duration::from_secs(10), pushes.recv()).await.expect("the answer in time").expect("pushes");
        if let SessionPush::SideAnswer(answer) = push {
            assert_eq!(answer.side_id, side_id);
            let done = answer.done;
            seen.push(answer);
            if done {
                return seen;
            }
        }
    }
}

/// Whether any file under `dir` holds `needle` (the database, its WAL, the blobs, logs).
fn on_disk(dir: &Path, needle: &[u8]) -> bool {
    std::fs::read_dir(dir).unwrap().flatten().any(|entry| {
        let path = entry.path();
        if path.is_dir() {
            return on_disk(&path, needle);
        }
        std::fs::read(&path).map(|bytes| bytes.windows(needle.len()).any(|w| w == needle)).unwrap_or(false)
    })
}

fn message_count(root: &Path, conversation: ConversationId) -> i64 {
    common::db(root).query_row("SELECT COUNT(*) FROM messages WHERE conversation_id = ?1", [conversation.to_string()], |r| r.get(0)).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_side_question_streams_its_answer_from_the_conversation_and_stores_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let _daemon = common::start_with_delay(dir.path(), 20).await;
    let grace = common::connect_as(dir.path(), "grace").await;
    let conversation = session(dir.path(), &grace).await;
    let before = message_count(dir.path(), conversation);
    let mut pushes = grace.session_pushes();

    let side_id = ask(&grace, conversation, &format!("What is {MARKER} about?")).await.unwrap();
    let seen = answers(&mut pushes, &side_id).await;
    let last = seen.last().unwrap();
    assert!(last.error.is_none(), "{last:?}");
    // The fake answers with the prompt it got: the conversation, then the question.
    assert!(last.text.contains("local test runtime"), "{}", last.text);
    assert!(last.text.contains("Project: Atlas"), "{}", last.text);
    assert!(last.text.contains("Make the export faster."), "{}", last.text);
    assert!(seen.len() > 1, "the answer came while it was written: {} updates", seen.len());
    assert!(seen.windows(2).all(|w| w[1].text.starts_with(&w[0].text) || w[0].text.is_empty()), "each update holds the whole text so far");

    assert_eq!(message_count(dir.path(), conversation), before, "no message in the conversation");
    assert!(!on_disk(&dir.path().join("data"), MARKER.as_bytes()), "the question is nowhere on disk");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_side_question_must_be_short_not_blank_and_about_a_conversation_that_exists() {
    let dir = tempfile::tempdir().unwrap();
    let _daemon = common::start(dir.path()).await;
    let grace = common::connect_as(dir.path(), "grace").await;
    let conversation = session(dir.path(), &grace).await;
    assert_eq!(code(ask(&grace, conversation, "   ").await), ErrorCode::Validation);
    assert_eq!(code(ask(&grace, conversation, &"x".repeat(SIDE_QUESTION_MAX_CHARS + 1)).await), ErrorCode::Validation);
    assert_eq!(code(ask(&grace, ConversationId::new(), "What now?").await), ErrorCode::NotFound);
}

#[tokio::test(flavor = "multi_thread")]
async fn only_a_signed_in_board_member_asks() {
    let dir = tempfile::tempdir().unwrap();
    let _daemon = common::start(dir.path()).await;
    let grace = common::connect_as(dir.path(), "grace").await;
    let conversation = session(dir.path(), &grace).await;
    let anonymous = common::connect_anonymous(dir.path()).await;
    let refused = code(ask(&anonymous, conversation, "What now?").await);
    assert_ne!(refused, ErrorCode::Validation, "refused before anything is read: {refused:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_side_question_can_be_stopped() {
    let dir = tempfile::tempdir().unwrap();
    // Slow steps: the answer is still being written when it is stopped.
    let _daemon = common::start_with_delay(dir.path(), 200).await;
    let grace = common::connect_as(dir.path(), "grace").await;
    let conversation = session(dir.path(), &grace).await;
    let mut pushes = grace.session_pushes();
    let side_id = ask(&grace, conversation, "What now?").await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    let stopped = grace.call(Method::ConversationSideCancel { side_id: side_id.clone() }).await.unwrap();
    assert!(matches!(stopped, ResponseBody::SideAsked(ref a) if a.side_id == side_id), "{stopped:?}");
    let seen = answers(&mut pushes, &side_id).await;
    assert_eq!(seen.last().unwrap().error.as_deref(), Some("Stopped."));
    // An unknown id is fine.
    grace.call(Method::ConversationSideCancel { side_id: "gone".into() }).await.unwrap();
}
