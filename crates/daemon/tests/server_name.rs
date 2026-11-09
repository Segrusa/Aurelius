//! The server's name over the protocol (SN-01, SN-02): a board member sets it,
//! every new hello and `server.get_name` give it, the event reaches a connected
//! client, an empty name gives the computer's name back, and apps and
//! connections without a session cannot change it.
#![cfg(unix)]

mod common;
use common::*;

use aurelius_client::{Client, ClientError, envelope};
use aurelius_domain::server::SetServerName;
use aurelius_domain::{Command, CommandResult, ErrorCode, EventKind};
use std::time::Duration;

fn rename(name: &str) -> Command {
    Command::SetServerName(SetServerName { name: name.into() })
}

fn code(result: Result<aurelius_domain::CommandReceipt, ClientError>) -> ErrorCode {
    match result {
        Err(ClientError::Protocol(err)) => err.code,
        other => panic!("{other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_new_name_reaches_every_new_hello_and_an_empty_one_gives_the_computers_name_back() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let alex = connect(dir.path()).await;
    let computer = aurelius_host::host_name();
    assert!(alex.hello.features.iter().any(|f| f == aurelius_protocol::feature::SERVER_NAME));
    assert_eq!(alex.hello.host_name, computer, "without a choice: the computer's name");
    let view = alex.server_name().await.unwrap();
    assert_eq!((view.name.as_str(), view.chosen.as_deref(), view.computer_name.as_str(), view.version), (computer.as_str(), None, computer.as_str(), 0));

    let set = alex.execute(envelope(rename(" Kantoor "), None)).await.unwrap();
    assert_eq!(set.result, CommandResult::ServerNameSet { name: "Kantoor".into(), version: 1 });
    assert_eq!(connect(dir.path()).await.hello.host_name, "Kantoor");
    assert_eq!(connect_anonymous(dir.path()).await.hello.host_name, "Kantoor", "the hello before the sign-in names it too");
    let view = alex.server_name().await.unwrap();
    assert_eq!((view.name.as_str(), view.chosen.as_deref(), view.computer_name.as_str()), ("Kantoor", Some("Kantoor"), computer.as_str()));

    alex.execute(envelope(rename("  "), None)).await.unwrap();
    assert_eq!(connect(dir.path()).await.hello.host_name, computer);
    assert_eq!(alex.server_name().await.unwrap().chosen, None);

    // A restart keeps the name: it lives in the database.
    alex.execute(envelope(rename("Thuis-PC"), None)).await.unwrap();
    stop(dir.path(), daemon).await;
    let daemon = start(dir.path()).await;
    assert_eq!(connect(dir.path()).await.hello.host_name, "Thuis-PC");
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_connected_client_gets_the_event_and_reads_the_new_name() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let (watcher, mut events) = connect_events(dir.path()).await;
    let after_seq = watcher.health().await.unwrap().last_seq;
    watcher.subscribe(after_seq).await.unwrap();
    let robin = connect_as(dir.path(), "robin").await;
    robin.execute(envelope(rename("Kantoor"), None)).await.unwrap();
    let event = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let event = events.recv().await.expect("events");
            if event.kind == EventKind::ServerRenamed {
                return event;
            }
        }
    })
    .await
    .expect("server.renamed within a few seconds");
    assert_eq!(event.aggregate_id, "server");
    assert_eq!(watcher.server_name().await.unwrap().name, "Kantoor", "without reconnecting");

    // The same name again: no event (AC6).
    robin.execute(envelope(rename("Kantoor"), None)).await.unwrap();
    robin.execute(envelope(rename("Thuis"), None)).await.unwrap();
    let next = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let event = events.recv().await.expect("events");
            if event.kind == EventKind::ServerRenamed {
                return event.seq;
            }
        }
    })
    .await
    .unwrap();
    let db = db(dir.path());
    let renamed: Vec<(i64, String)> = db
        .prepare("SELECT seq, json_extract(metadata_json, '$.after') FROM events WHERE type = 'server.renamed' ORDER BY seq")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(renamed.iter().map(|(_, a)| a.as_str()).collect::<Vec<_>>(), ["Kantoor", "Thuis"]);
    assert_eq!(next as i64, renamed[1].0, "the next event is the second change");
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_refused_name_keeps_the_old_one_with_the_reason() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let alex = connect(dir.path()).await;
    alex.execute(envelope(rename("Kantoor"), None)).await.unwrap();
    for name in ["x".repeat(41), "Kan\ntoor".into()] {
        match alex.execute(envelope(rename(&name), None)).await {
            Err(ClientError::Protocol(err)) => {
                assert_eq!(err.code, ErrorCode::Validation);
                assert!(err.message.contains("40") || err.message.contains("line breaks"), "{}", err.message);
            }
            other => panic!("{other:?}"),
        }
    }
    assert_eq!(alex.server_name().await.unwrap().name, "Kantoor");
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn connections_without_a_session_cannot_rename_the_server() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let alex = connect(dir.path()).await;
    let anonymous = connect_anonymous(dir.path()).await;
    assert_eq!(code(anonymous.execute(envelope(rename("Gekaapt"), None)).await), ErrorCode::Unauthenticated);
    assert_eq!(alex.server_name().await.unwrap().version, 0, "nothing changed");
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_name_that_holds_a_vault_secret_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let alex = connect(dir.path()).await;
    let params = aurelius_protocol::VaultCreateParams {
        project_id: None,
        kind: aurelius_domain::vault::VaultKind::Secret,
        name: "Token".into(),
        env_name: "SN_TOKEN".into(),
        description: None,
        value: "sn01-secret-value-123".into(),
    };
    alex.vault_create(params).await.unwrap();
    assert_eq!(code(alex.execute(envelope(rename("sn01-secret-value-123"), None)).await), ErrorCode::LooksLikeSecret);
    assert_eq!(alex.server_name().await.unwrap().version, 0);
    stop(dir.path(), daemon).await;
}
