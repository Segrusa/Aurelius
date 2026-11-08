//! The built-in terminal: a shell in the folder an employee's conversation
//! works in, its output streamed to the attached client only.

mod common;

use aurelius_client::{Client, TerminalEvent, envelope};
use aurelius_domain::{Command, ConversationId, employee::CreateEmployee, project::CreateProject};
use aurelius_protocol::CommandResult;
use common::*;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::mpsc;

async fn employee_conversation(client: &Client, name: &str) -> (ConversationId, PathBuf) {
    let CommandResult::ProjectCreated { project_id, .. } =
        client.execute(envelope(Command::CreateProject(CreateProject { name: name.into(), description: None, folder: None }), None)).await.unwrap().result
    else {
        panic!()
    };
    let folder = client.projects().await.unwrap().projects.into_iter().find(|p| p.id == project_id).unwrap().folder.expect("folder");
    // The employee's chat works in the employee's own folder (here the project's).
    let CommandResult::EmployeeCreated { conversation_id, .. } = client
        .execute(envelope(Command::CreateEmployee(CreateEmployee { description: None, name: format!("Ada of {name}"), role: "Dev".into(), title: None, reports_to: None, instructions: None, folder: Some(folder.clone()) }), None))
        .await
        .unwrap()
        .result
    else {
        panic!()
    };
    (conversation_id, PathBuf::from(folder))
}

/// Reads output until `needle` shows up (or the shell ends). Answers the
/// cursor position query a terminal emulator would (ConPTY waits for it).
async fn read_until(client: &Client, id: &str, output: &mut mpsc::UnboundedReceiver<TerminalEvent>, needle: &str) -> String {
    let mut seen = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        let text = String::from_utf8_lossy(&seen).into_owned();
        if text.contains(needle) {
            return text;
        }
        match tokio::time::timeout_at(deadline, output.recv()).await {
            Ok(Some(TerminalEvent::Output(bytes))) => {
                if bytes.windows(4).any(|w| w == b"[6n") {
                    client.terminal_input(id, b"[1;1R").await.unwrap();
                }
                seen.extend(bytes);
            }
            Ok(Some(TerminalEvent::Exit(code))) => panic!("shell ended ({code:?}) before {needle:?}:\n{text}"),
            Ok(None) => panic!("output closed before {needle:?}:\n{text}"),
            Err(_) => panic!("no {needle:?} in time:\n{text}"),
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_terminal_runs_a_shell_in_the_employees_folder() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let (conversation, folder) = employee_conversation(&client, "Shell").await;
    std::fs::write(folder.join("marker-file.txt"), "x").unwrap();

    let (view, mut output) = client.open_terminal(conversation, 120, 30, false).await.unwrap();
    assert!(!view.reattached);
    assert_eq!(Path::new(&view.cwd), folder);

    // The shell starts in the folder: it lists the file and echoes.
    let list = if cfg!(windows) { "dir\r" } else { "ls\n" };
    client.terminal_input(&view.terminal_id, list.as_bytes()).await.unwrap();
    read_until(&client, &view.terminal_id, &mut output, "marker-file.txt").await;
    client.terminal_input(&view.terminal_id, b"echo hallo-terminal\r").await.unwrap();
    read_until(&client, &view.terminal_id, &mut output, "hallo-terminal").await;
    client.resize_terminal(&view.terminal_id, 100, 40).await.unwrap();

    // Another client opening the same conversation reattaches and gets the scrollback.
    let other = connect(dir.path()).await;
    let (again, mut replay) = other.open_terminal(conversation, 100, 40, false).await.unwrap();
    assert!(again.reattached);
    assert_eq!(again.terminal_id, view.terminal_id);
    read_until(&other, &view.terminal_id, &mut replay, "hallo-terminal").await;

    // Closing ends the shell for everyone attached.
    client.close_terminal(&view.terminal_id).await.unwrap();
    for rx in [&mut output, &mut replay] {
        let ended = tokio::time::timeout(Duration::from_secs(15), async {
            while let Some(event) = rx.recv().await {
                if let TerminalEvent::Exit(_) = event {
                    return true;
                }
            }
            false
        })
        .await;
        assert_eq!(ended, Ok(true));
    }
    assert!(client.terminal_input(&view.terminal_id, b"x").await.is_err());

    // A fresh one is a new shell.
    let (fresh, _) = client.open_terminal(conversation, 80, 24, false).await.unwrap();
    assert_ne!(fresh.terminal_id, view.terminal_id);
    assert!(!fresh.reattached);
    daemon.abort();
}

/// Feature `terminal.multi`: several terminals per conversation, detach and list.
#[tokio::test(flavor = "multi_thread")]
async fn a_conversation_runs_several_terminals_and_sessions_detach() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    assert!(client.supports(aurelius_protocol::FEATURE_TERMINAL_MULTI));
    let (conversation, _folder) = employee_conversation(&client, "Shells").await;

    let (first, mut first_out) = client.open_terminal(conversation, 80, 24, false).await.unwrap();
    let (second, mut second_out) = client.open_additional_terminal(conversation, 80, 24).await.unwrap();
    assert_ne!(first.terminal_id, second.terminal_id);
    assert!(!second.reattached);

    // Both stay attached in this session: each gets its own output.
    client.terminal_input(&first.terminal_id, b"echo in-first\r").await.unwrap();
    read_until(&client, &first.terminal_id, &mut first_out, "in-first").await;
    client.terminal_input(&second.terminal_id, b"echo in-second\r").await.unwrap();
    read_until(&client, &second.terminal_id, &mut second_out, "in-second").await;

    let list = client.list_terminals(conversation).await.unwrap();
    let ids: Vec<_> = list.iter().map(|t| t.terminal_id.as_str()).collect();
    assert_eq!(ids, [first.terminal_id.as_str(), second.terminal_id.as_str()], "oldest first");
    let titles: Vec<_> = list.iter().map(|t| t.title.as_str()).collect();
    assert_eq!(titles, ["Terminal 1", "Terminal 2"]);
    assert!(list.iter().all(|t| t.alive && t.attached == 1 && t.created_at_ms > 0));

    // A plain open still attaches to the oldest one, as before.
    let other = connect(dir.path()).await;
    let (plain, _plain_out) = other.open_terminal(conversation, 80, 24, false).await.unwrap();
    assert!(plain.reattached);
    assert_eq!(plain.terminal_id, first.terminal_id);
    drop(other);

    // Detached: the shell runs on; reattaching by id replays what came meanwhile.
    client.detach_terminal(&second.terminal_id).await.unwrap();
    assert_eq!(tokio::time::timeout(Duration::from_secs(5), async { while second_out.recv().await.is_some() {} }).await, Ok(()));
    client.terminal_input(&second.terminal_id, b"echo while-detached\r").await.unwrap();
    let detached = client.list_terminals(conversation).await.unwrap().into_iter().find(|t| t.terminal_id == second.terminal_id).unwrap();
    assert!(detached.alive);
    assert_eq!(detached.attached, 0);
    let (back, mut back_out) = client.attach_terminal(conversation, &second.terminal_id, 80, 24).await.unwrap();
    assert!(back.reattached);
    assert_eq!(back.terminal_id, second.terminal_id);
    let replay = read_until(&client, &second.terminal_id, &mut back_out, "while-detached").await;
    assert!(replay.contains("in-second"), "the scrollback is replayed:\n{replay}");

    // The first terminal was not touched by any of this.
    client.terminal_input(&first.terminal_id, b"echo still-first\r").await.unwrap();
    read_until(&client, &first.terminal_id, &mut first_out, "still-first").await;

    // Four per conversation, with a clear error for the fifth.
    let (_third, _t3) = client.open_additional_terminal(conversation, 80, 24).await.unwrap();
    let (fourth, _t4) = client.open_additional_terminal(conversation, 80, 24).await.unwrap();
    assert_eq!(fourth.terminal_id, client.list_terminals(conversation).await.unwrap()[3].terminal_id);
    match client.open_additional_terminal(conversation, 80, 24).await {
        Err(aurelius_client::ClientError::Protocol(err)) => {
            assert_eq!(err.code, aurelius_protocol::ErrorCode::Validation);
            assert!(err.message.contains("At most 4 terminals"), "{}", err.message);
        }
        other => panic!("a fifth terminal: {other:?}"),
    }

    // Unknown ids and other conversations' terminals are refused.
    assert!(client.detach_terminal("no-such-terminal").await.is_err());
    let (elsewhere, _) = employee_conversation(&client, "Elsewhere").await;
    assert!(client.attach_terminal(elsewhere, &first.terminal_id, 80, 24).await.is_err());
    assert!(client.list_terminals(elsewhere).await.unwrap().is_empty());
    daemon.abort();
}
