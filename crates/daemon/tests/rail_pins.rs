//! Each board member's own rail through the daemon's API (feature `rail_pins`):
//! what one board member pins is on every device they sign in on, never on another
//! board member's rail, and their other devices hear of the change.
#![cfg(unix)]

mod common;
use common::*;

use aurelius_client::{Client, ClientError};
use aurelius_domain::project::CreateProject;
use aurelius_domain::{Command, CommandResult, ErrorCode, EventKind, ProjectId};
use std::time::Duration;

async fn project(client: &Client, name: &str) -> ProjectId {
    let made = client.execute(aurelius_client::envelope(Command::CreateProject(CreateProject { name: name.into(), description: None, folder: None }), None)).await.unwrap();
    let CommandResult::ProjectCreated { project_id, .. } = made.result else { panic!("{:?}", made.result) };
    project_id
}

#[tokio::test(flavor = "multi_thread")]
async fn a_rail_follows_its_board_member_to_another_device_and_no_one_else() {
    let dir = tempfile::tempdir().unwrap();
    let _daemon = start(dir.path()).await;
    let laptop = connect(dir.path()).await;
    let (alpha, beta) = (project(&laptop, "Alpha").await, project(&laptop, "Beta").await);
    let (desktop, mut events) = connect_events(dir.path()).await;
    let after_seq = desktop.health().await.unwrap().last_seq;
    desktop.subscribe(after_seq).await.unwrap();
    let bea = connect_as(dir.path(), "bea").await;

    let before = desktop.rail_pins().await.unwrap();
    assert!(!before.chosen && before.project_ids.is_empty(), "nobody chose yet");

    let receipt = laptop.set_rail_pins(vec![beta, alpha]).await.unwrap();
    assert_eq!(receipt.result, CommandResult::RailPinsSet { project_ids: vec![beta, alpha], changed: true });

    let alex_id = desktop.user_whoami().await.unwrap().user.expect("signed in").id;
    let event = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let event = events.recv().await.expect("events");
            if event.kind == EventKind::UserRailPinsChanged {
                return event;
            }
        }
    })
    .await
    .expect("user.rail_pins_changed within a few seconds");
    assert_eq!(event.aggregate_id, alex_id.to_string(), "the event names whose rail changed");

    let on_desktop = desktop.rail_pins().await.unwrap();
    assert!(on_desktop.chosen);
    assert_eq!(on_desktop.project_ids, [beta, alpha]);
    let of_bea = bea.rail_pins().await.unwrap();
    assert!(!of_bea.chosen && of_bea.project_ids.is_empty(), "another board member's rail is untouched");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_connection_without_a_board_member_has_no_rail() {
    let dir = tempfile::tempdir().unwrap();
    let _daemon = start(dir.path()).await;
    drop(connect(dir.path()).await);
    let anonymous = connect_anonymous(dir.path()).await;
    match anonymous.rail_pins().await {
        Err(ClientError::Protocol(err)) => assert!(matches!(err.code, ErrorCode::Unauthenticated | ErrorCode::BoardMemberRequired), "{err:?}"),
        other => panic!("{other:?}"),
    }
}
