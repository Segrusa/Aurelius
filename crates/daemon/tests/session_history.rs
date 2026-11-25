//! Work sessions in the History (feature `session_history`): after an answer the
//! session's summary waits until the session has gone quiet; every board member
//! reads every session in `work_session.history`, may ask for its summary at once
//! with `work_session.retry_summary`, and may read the session without writing in
//! it. Runs use the fake runtime, which answers a summary or title prompt by its marker.

mod common;

use aurelius_client::Client;
use aurelius_domain::employee::CreateEmployee;
use aurelius_domain::history::{HistorySummaryState, SESSION_SUMMARY_QUIET_MS, SessionHistoryEntry};
use aurelius_domain::project::CreateProject;
use aurelius_domain::work_session::{RetrySessionSummary, StartWorkSession};
use aurelius_domain::{Command, CommandResult, ConversationId};
use aurelius_protocol::Method;
use std::time::Duration;

fn everything() -> Method {
    Method::WorkSessionHistory { project_id: None, employee_id: None, from_ms: None, to_ms: None, query: None, before_last_activity_ms: None, before_conversation_id: None, limit: None }
}

async fn row(client: &Client, conversation: ConversationId) -> SessionHistoryEntry {
    let page = client.session_history(everything()).await.unwrap();
    page.entries.into_iter().find(|e| e.conversation_id == conversation).expect("the session is in the History")
}

/// Grace starts a session with Orion in project Atlas; it waits for Orion's answer.
async fn session(root: &std::path::Path, grace: &Client) -> ConversationId {
    let folder = root.join("atlas");
    common::git_repo(&folder);
    let made = grace.execute(aurelius_client::envelope(Command::CreateProject(CreateProject { name: "Atlas".into(), description: None, folder: Some(folder.to_string_lossy().into_owned()) }), None)).await.unwrap().result;
    let CommandResult::ProjectCreated { project_id, .. } = made else { panic!("{made:?}") };
    let create = CreateEmployee { access_level: None, tool_policy: None, start_profile: None, description: None, name: "Orion".into(), role: "Project Manager".into(), title: None, reports_to: None, instructions: None, folder: None };
    let CommandResult::EmployeeCreated { employee_id, .. } = grace.execute(aurelius_client::envelope(Command::CreateEmployee(create), None)).await.unwrap().result else { panic!() };
    let start = StartWorkSession { project_id, employee_id, text: "Make the export faster.".into(), images: vec![], files: vec![], worktree: false, own_branch: true, run: Default::default(), confirm_secret: false, design: false };
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

#[tokio::test(flavor = "multi_thread")]
async fn every_board_member_sees_a_session_in_the_history_and_its_summary_comes_when_asked() {
    let dir = tempfile::tempdir().unwrap();
    let _daemon = common::start(dir.path()).await;
    let grace = common::connect_as(dir.path(), "grace").await;
    let conversation = session(dir.path(), &grace).await;

    // The answer asked for a summary, but only once the session has gone quiet.
    let due: i64 = common::db(dir.path())
        .query_row("SELECT not_before_ms FROM jobs WHERE kind = 'session.summarize' AND json_extract(payload_json, '$.conversation_id') = ?1", [conversation.to_string()], |r| r.get(0))
        .unwrap();
    assert!(due > aurelius_storage::now_ms() + SESSION_SUMMARY_QUIET_MS / 2, "it waits for the quiet time");

    // Ada did not start it, but the History shows it to her, with who did, under
    // the title made from its content after the first answer (in place of the first line).
    let ada = common::connect_as(dir.path(), "ada").await;
    let mut seen = row(&ada, conversation).await;
    for _ in 0..400 {
        if seen.title != "Make the export faster." {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
        seen = row(&ada, conversation).await;
    }
    assert_eq!(seen.title, "Make the export faster", "the fake's made title, cleaned");
    let titles: i64 = common::db(dir.path()).query_row("SELECT COUNT(*) FROM jobs WHERE kind = 'session.title' AND state = 'done'", [], |r| r.get(0)).unwrap();
    assert_eq!(titles, 1, "one title job per session");
    assert_eq!(seen.employee_name, "Orion");
    assert_eq!(seen.project_name, "Atlas");
    assert_eq!(seen.started_by_name.as_deref(), Some("grace"));
    assert_eq!(seen.summary_state, HistorySummaryState::Summarizing, "the waiting job counts");

    // She asks for it now: the job runs at once and the History shows the summary.
    ada.execute(aurelius_client::envelope(Command::RetrySessionSummary(RetrySessionSummary { conversation_id: conversation }), None)).await.unwrap();
    let mut ready = None;
    for _ in 0..400 {
        let r = row(&ada, conversation).await;
        if r.summary_state == HistorySummaryState::Ready {
            ready = Some(r);
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let ready = ready.expect("the summary came");
    let summary = ready.summary.unwrap();
    assert!(summary.starts_with("Summary (test runtime): 2 messages discussed."), "both messages were read: {summary}");
    let jobs: i64 = common::db(dir.path()).query_row("SELECT COUNT(*) FROM jobs WHERE kind = 'session.summarize'", [], |r| r.get(0)).unwrap();
    assert_eq!(jobs, 1, "the waiting job ran: no second model call");

    // The search finds it by its summary; Ada reads the session but does not write in it.
    let found = ada
        .session_history(Method::WorkSessionHistory { project_id: None, employee_id: None, from_ms: None, to_ms: None, query: Some("TEST RUNTIME".into()), before_last_activity_ms: None, before_conversation_id: None, limit: None })
        .await
        .unwrap();
    assert_eq!(found.entries.len(), 1);
    assert!(!ada.messages(conversation).await.unwrap().messages.is_empty(), "a board member reads any session");
    let send = Command::SendChatMessage(aurelius_domain::conversation::SendChatMessage { conversation_id: conversation, text: "Me too".into(), images: vec![], files: vec![], interrupt: false, confirm_secret: false });
    assert!(ada.execute(aurelius_client::envelope(send, None)).await.is_err(), "only Grace writes in it");
}

/// The reader's own row of `conversation` in `work_session.list`.
async fn listed(client: &Client, conversation: ConversationId) -> aurelius_domain::work_session::WorkSessionView {
    let aurelius_protocol::ResponseBody::WorkSessions(snapshot) = client.call(Method::WorkSessionList { project_id: None }).await.unwrap() else { panic!("not a session list") };
    snapshot.sessions.into_iter().find(|s| s.conversation_id == conversation).expect("the session is listed")
}

#[tokio::test(flavor = "multi_thread")]
async fn a_finished_answer_is_unseen_until_the_session_is_opened_and_it_orders_by_its_end() {
    let dir = tempfile::tempdir().unwrap();
    let _daemon = common::start(dir.path()).await;
    let grace = common::connect_as(dir.path(), "grace").await;
    let conversation = session(dir.path(), &grace).await;
    assert!(!listed(&grace, conversation).await.unseen, "read after the answer ended: seen");

    // A second answer that nobody opens: wait for it on the list only.
    let send = aurelius_domain::conversation::SendChatMessage { confirm_secret: false, conversation_id: conversation, text: "And the import?".into(), images: vec![], files: vec![], interrupt: false };
    grace.execute(aurelius_client::envelope(Command::SendChatMessage(send), None)).await.unwrap();
    let mut row = listed(&grace, conversation).await;
    for _ in 0..400 {
        let ended: i64 = common::db(dir.path())
            .query_row("SELECT COUNT(*) FROM runs WHERE conversation_id = ?1 AND ended_at_ms IS NOT NULL", [conversation.to_string()], |r| r.get(0))
            .unwrap();
        row = listed(&grace, conversation).await;
        if ended >= 2 && !row.running {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(row.unseen, "the second answer ended after the session was last opened");
    let ended: i64 = common::db(dir.path())
        .query_row("SELECT MAX(ended_at_ms) FROM runs WHERE conversation_id = ?1", [conversation.to_string()], |r| r.get(0))
        .unwrap();
    assert!(row.last_activity_ms >= ended, "the last activity is the answer's end, not its start");

    grace.messages(conversation).await.unwrap();
    assert!(!listed(&grace, conversation).await.unseen, "opening it with conversation.get clears the mark");
}
