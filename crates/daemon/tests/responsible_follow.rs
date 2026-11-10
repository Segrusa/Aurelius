//! A1 Responsible and A2 following through the daemon's API (AUR-FA-F1): every rule
//! with its refusals, one transaction with events and receipt, and what an older
//! client gets (rule R1 of the freeze).
#![cfg(unix)]

mod common;
use common::*;

use aurelius_client::{Client, ClientError};
use aurelius_domain::group_chat::SendGroupMessage;
use aurelius_domain::project::CreateProject;
use aurelius_domain::ticket::{
    ArchiveTicket, ColumnKind, CreateTicket, FollowTicket, PostTicketMessage, SetTicketResponsible,
    TicketPriority, UpdateTicket,
};
use aurelius_domain::{
    Command, CommandEnvelope, CommandId, CommandResult, ConversationId, ErrorCode, EventKind,
    ProjectId, TicketId, UserId,
};
use aurelius_host::ipc::{self, BoxStream, read_frame, write_frame};
use aurelius_protocol::{
    HelloParams, Method, Outcome, Request, ResponseBody, ServerMessage, TimelineQuery,
    client_feature, feature,
};
use aurelius_storage::Store;
use serde::Deserialize;
use std::path::Path;
use std::time::Duration;

async fn project(client: &Client) -> (ProjectId, ConversationId) {
    let created = client
        .execute(aurelius_client::envelope(
            Command::CreateProject(CreateProject {
                name: "Atlas".into(),
                description: None,
                folder: None,
            }),
            None,
        ))
        .await
        .unwrap();
    let CommandResult::ProjectCreated { project_id, .. } = created.result else {
        panic!("{:?}", created.result)
    };
    let channel = client
        .channels()
        .await
        .unwrap()
        .channels
        .into_iter()
        .find(|c| c.project_id == Some(project_id))
        .expect("its channel")
        .conversation_id;
    (project_id, channel)
}

fn new_ticket(
    project_id: ProjectId,
    description: Option<&str>,
    responsible_user_id: Option<UserId>,
) -> CreateTicket {
    CreateTicket {
        confirm_secret: false,
        project_id,
        title: "Login".into(),
        description: description.map(str::to_owned),
        priority: TicketPriority::default(),
        labels: vec![],
        deadline_ms: None,
        owner_employee_id: None,
        run: Default::default(),
        files: vec![],
        start: false,
        from_message: None,
        from_idea: None,
        responsible_user_id,
    }
}

async fn ticket(client: &Client, project_id: ProjectId) -> (TicketId, u32) {
    let CommandResult::TicketCreated { ticket_id, number } = client
        .create_ticket(new_ticket(project_id, None, None))
        .await
        .unwrap()
        .result
    else {
        panic!("not created")
    };
    (ticket_id, number)
}

async fn user_id(client: &Client) -> UserId {
    client
        .user_whoami()
        .await
        .unwrap()
        .user
        .expect("signed in")
        .id
}

/// `alex` (on a new server) and `bea`, each signed in, with their ids.
async fn two_members(root: &Path) -> (Client, UserId, Client, UserId) {
    let alex = connect(root).await;
    let bea = connect_as(root, "bea").await;
    let (alex_id, bea_id) = (user_id(&alex).await, user_id(&bea).await);
    (alex, alex_id, bea, bea_id)
}

fn set_responsible(ticket_id: TicketId, responsible_user_id: Option<UserId>) -> Command {
    Command::SetTicketResponsible(SetTicketResponsible {
        ticket_id,
        responsible_user_id,
    })
}

fn follow(ticket_id: TicketId, on: bool) -> Command {
    Command::FollowTicket(FollowTicket { ticket_id, on })
}

fn code(err: ClientError) -> ErrorCode {
    match err {
        ClientError::Protocol(err) => err.code,
        other => panic!("{other:?}"),
    }
}

async fn run(client: &Client, command: Command) -> Result<CommandResult, ClientError> {
    client
        .execute(aurelius_client::envelope(command, None))
        .await
        .map(|r| r.result)
}

/// Who follows, as (username, reason).
async fn followers(client: &Client, ticket_id: TicketId) -> Vec<(String, String)> {
    client
        .ticket_followers(ticket_id)
        .await
        .unwrap()
        .followers
        .into_iter()
        .map(|f| (f.username, f.reason))
        .collect()
}

fn pair(name: &str, reason: &str) -> (String, String) {
    (name.to_owned(), reason.to_owned())
}

fn count(root: &Path, sql: &str, value: &str) -> i64 {
    db(root).query_row(sql, [value], |r| r.get(0)).unwrap()
}

/// The events of a kind about a ticket, whichever aggregate they have.
fn events_of(root: &Path, kind: EventKind, ticket_id: TicketId) -> Vec<serde_json::Value> {
    let db = db(root);
    let mut stmt = db
        .prepare(
            "SELECT metadata_json FROM events WHERE type = ?1 AND aggregate_id = ?2 ORDER BY seq",
        )
        .unwrap();
    stmt.query_map([kind.as_str().to_owned(), ticket_id.to_string()], |r| {
        r.get::<_, String>(0)
    })
    .unwrap()
    .map(|m| serde_json::from_str(&m.unwrap()).unwrap())
    .collect()
}

fn put_in_column(root: &Path, ticket_id: TicketId, kind: &'static str) {
    Store::open(root)
        .unwrap()
        .write(move |tx| {
            tx.execute("UPDATE tickets SET column_id = (SELECT id FROM board_columns WHERE kind = ?2) WHERE id = ?1", [ticket_id.to_string(), kind.to_owned()])?;
            Ok(())
        })
        .unwrap();
}

// ---- ticket.set_responsible ----

#[tokio::test(flavor = "multi_thread")]
async fn a_board_member_makes_someone_responsible_and_that_person_follows_in_one_commit() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let (alex, alex_id, _bea, bea_id) = two_members(dir.path()).await;
    let (project_id, _) = project(&alex).await;
    let (ticket_id, _) = ticket(&alex, project_id).await;
    let before = alex.ticket(ticket_id).await.unwrap().ticket;
    assert_eq!(before.responsible_user_id, None);

    let key = CommandId::new();
    let receipt = alex
        .execute(CommandEnvelope {
            command_id: key,
            expected_version: None,
            command: set_responsible(ticket_id, Some(bea_id)),
        })
        .await
        .unwrap();
    let CommandResult::TicketChanged {
        version, column, ..
    } = receipt.result
    else {
        panic!("{:?}", receipt.result)
    };
    assert_eq!((column, version), (ColumnKind::Backlog, before.version + 1));

    let after = alex.ticket(ticket_id).await.unwrap().ticket;
    assert_eq!(
        (
            after.responsible_user_id,
            after.responsible_username.as_deref(),
            after.responsible_disabled
        ),
        (Some(bea_id), Some("bea"), false)
    );
    assert_eq!(after.version, version);
    assert_eq!(
        followers(&alex, ticket_id).await,
        vec![pair("alex", "created"), pair("bea", "responsible")]
    );

    let updated = events_of(dir.path(), EventKind::TicketUpdated, ticket_id);
    assert_eq!(updated.len(), 1);
    assert_eq!(updated[0]["changed"], serde_json::json!(["responsible"]));
    assert_eq!(updated[0]["by"]["user_id"], serde_json::json!(alex_id));
    assert_eq!(updated[0]["responsible_user_id"], serde_json::json!(bea_id));
    let follows = events_of(dir.path(), EventKind::TicketFollowChanged, ticket_id);
    assert_eq!(
        follows.last().unwrap()["user_id"],
        serde_json::json!(bea_id)
    );
    assert_eq!(follows.last().unwrap()["reason"], "responsible");

    // The same command again answers from its one receipt and writes nothing more.
    let again = alex
        .execute(CommandEnvelope {
            command_id: key,
            expected_version: None,
            command: set_responsible(ticket_id, Some(bea_id)),
        })
        .await
        .unwrap();
    assert!(again.replayed);
    assert_eq!(
        count(
            dir.path(),
            "SELECT COUNT(*) FROM command_receipts WHERE command_id = ?1",
            &key.to_string()
        ),
        1
    );
    assert_eq!(
        events_of(dir.path(), EventKind::TicketUpdated, ticket_id).len(),
        1
    );

    drop((alex, _bea));
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_handover_ignores_a_stale_version_and_the_former_responsible_keeps_following() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let (alex, alex_id, _bea, bea_id) = two_members(dir.path()).await;
    let (project_id, _) = project(&alex).await;
    let CommandResult::TicketCreated { ticket_id, .. } = alex
        .create_ticket(new_ticket(project_id, None, Some(bea_id)))
        .await
        .unwrap()
        .result
    else {
        panic!()
    };

    let stale = alex
        .execute(aurelius_client::envelope(
            set_responsible(ticket_id, Some(alex_id)),
            Some(9_999),
        ))
        .await
        .unwrap();
    assert!(matches!(stale.result, CommandResult::TicketChanged { .. }));
    assert_eq!(
        alex.ticket(ticket_id)
            .await
            .unwrap()
            .ticket
            .responsible_user_id,
        Some(alex_id)
    );
    let names: Vec<String> = followers(&alex, ticket_id)
        .await
        .into_iter()
        .map(|f| f.0)
        .collect();
    assert_eq!(names, ["alex", "bea"]);

    // Nobody any more: the column is empty, and whoever followed still does.
    run(&alex, set_responsible(ticket_id, None)).await.unwrap();
    let view = alex.ticket(ticket_id).await.unwrap().ticket;
    assert_eq!(
        (view.responsible_user_id, view.responsible_username),
        (None, None)
    );
    assert_eq!(followers(&alex, ticket_id).await.len(), 2);

    drop((alex, _bea));
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn choosing_who_already_is_responsible_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let (alex, _, _bea, bea_id) = two_members(dir.path()).await;
    let (project_id, _) = project(&alex).await;
    let (ticket_id, _) = ticket(&alex, project_id).await;
    run(&alex, set_responsible(ticket_id, Some(bea_id)))
        .await
        .unwrap();
    let version = alex.ticket(ticket_id).await.unwrap().ticket.version;

    run(&alex, set_responsible(ticket_id, Some(bea_id)))
        .await
        .unwrap();
    assert_eq!(
        alex.ticket(ticket_id).await.unwrap().ticket.version,
        version
    );
    assert_eq!(
        events_of(dir.path(), EventKind::TicketUpdated, ticket_id).len(),
        1
    );

    drop((alex, _bea));
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_disabled_or_unknown_member_cannot_become_responsible_and_the_refusal_leaves_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let (alex, _, bea, bea_id) = two_members(dir.path()).await;
    let (project_id, _) = project(&alex).await;
    let (ticket_id, _) = ticket(&alex, project_id).await;
    drop(bea);
    alex.disable_user(bea_id).await.unwrap();

    for (who, expected) in [
        (bea_id, ErrorCode::Validation),
        (UserId::new(), ErrorCode::NotFound),
    ] {
        let key = CommandId::new();
        let refused = alex
            .execute(CommandEnvelope {
                command_id: key,
                expected_version: None,
                command: set_responsible(ticket_id, Some(who)),
            })
            .await
            .unwrap_err();
        assert_eq!(code(refused), expected);
        assert_eq!(
            count(
                dir.path(),
                "SELECT COUNT(*) FROM command_receipts WHERE command_id = ?1",
                &key.to_string()
            ),
            0
        );
    }
    assert_eq!(
        alex.ticket(ticket_id)
            .await
            .unwrap()
            .ticket
            .responsible_user_id,
        None
    );
    assert!(events_of(dir.path(), EventKind::TicketUpdated, ticket_id).is_empty());
    assert_eq!(
        followers(&alex, ticket_id).await,
        vec![pair("alex", "created")]
    );

    // On a new ticket too: it does not get made.
    let tickets = |root: &Path| count(root, "SELECT COUNT(*) FROM tickets WHERE title <> ?1", "");
    let before = tickets(dir.path());
    assert_eq!(
        code(
            alex.create_ticket(new_ticket(project_id, None, Some(bea_id)))
                .await
                .unwrap_err()
        ),
        ErrorCode::Validation
    );
    assert_eq!(tickets(dir.path()), before);

    drop(alex);
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_disabled_responsible_stays_on_the_ticket_until_someone_changes_it() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let (alex, alex_id, bea, bea_id) = two_members(dir.path()).await;
    let (project_id, _) = project(&alex).await;
    let (ticket_id, _) = ticket(&alex, project_id).await;
    run(&alex, set_responsible(ticket_id, Some(bea_id)))
        .await
        .unwrap();
    drop(bea);
    alex.disable_user(bea_id).await.unwrap();

    let view = alex.ticket(ticket_id).await.unwrap().ticket;
    assert_eq!(
        (view.responsible_user_id, view.responsible_disabled),
        (Some(bea_id), true)
    );
    // Sending the same choice again is not "a new Responsible": no refusal, no change.
    run(&alex, set_responsible(ticket_id, Some(bea_id)))
        .await
        .unwrap();
    let listed = alex.ticket_followers(ticket_id).await.unwrap().followers;
    assert!(
        listed.iter().any(|f| f.user_id == bea_id && f.disabled),
        "stays listed: {listed:?}"
    );

    run(&alex, set_responsible(ticket_id, Some(alex_id)))
        .await
        .unwrap();
    assert!(
        !alex
            .ticket(ticket_id)
            .await
            .unwrap()
            .ticket
            .responsible_disabled
    );

    drop(alex);
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_ticket_on_done_or_archived_keeps_its_responsible() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let (alex, _, _bea, bea_id) = two_members(dir.path()).await;
    let (project_id, _) = project(&alex).await;

    let (done, _) = ticket(&alex, project_id).await;
    put_in_column(dir.path(), done, "done");
    assert_eq!(
        code(
            run(&alex, set_responsible(done, Some(bea_id)))
                .await
                .unwrap_err()
        ),
        ErrorCode::Validation
    );
    assert_eq!(
        alex.ticket(done).await.unwrap().ticket.responsible_user_id,
        None
    );

    let (archived, _) = ticket(&alex, project_id).await;
    run(
        &alex,
        Command::ArchiveTicket(ArchiveTicket {
            ticket_id: archived,
        }),
    )
    .await
    .unwrap();
    assert_eq!(
        code(
            run(&alex, set_responsible(archived, Some(bea_id)))
                .await
                .unwrap_err()
        ),
        ErrorCode::Validation
    );

    assert_eq!(
        code(
            run(&alex, set_responsible(TicketId::new(), None))
                .await
                .unwrap_err()
        ),
        ErrorCode::NotFound
    );

    drop((alex, _bea));
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn only_a_board_member_writes_or_reads_this_and_never_an_app() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let alex = connect(dir.path()).await;
    let (project_id, _) = project(&alex).await;
    let (ticket_id, _) = ticket(&alex, project_id).await;

    let anonymous = connect_anonymous(dir.path()).await;
    for command in [set_responsible(ticket_id, None), follow(ticket_id, true)] {
        assert_eq!(
            code(run(&anonymous, command).await.unwrap_err()),
            ErrorCode::Unauthenticated
        );
    }
    assert_eq!(
        code(anonymous.ticket_followers(ticket_id).await.unwrap_err()),
        ErrorCode::Unauthenticated
    );

    assert_eq!(
        followers(&alex, ticket_id).await.len(),
        1,
        "nothing changed"
    );

    drop((alex, anonymous));
    stop(dir.path(), daemon).await;
}

// ---- ticket.create ----

#[tokio::test(flavor = "multi_thread")]
async fn a_new_ticket_without_the_field_has_no_responsible_and_its_creator_follows() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let alex = connect(dir.path()).await;
    let (project_id, _) = project(&alex).await;
    let (ticket_id, _) = ticket(&alex, project_id).await;

    assert_eq!(
        alex.ticket(ticket_id)
            .await
            .unwrap()
            .ticket
            .responsible_user_id,
        None
    );
    assert_eq!(
        followers(&alex, ticket_id).await,
        vec![pair("alex", "created")]
    );

    drop(alex);
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_new_ticket_for_someone_else_makes_both_follow_and_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let (alex, _, _bea, bea_id) = two_members(dir.path()).await;
    let (project_id, _) = project(&alex).await;
    let CommandResult::TicketCreated { ticket_id, .. } = alex
        .create_ticket(new_ticket(project_id, None, Some(bea_id)))
        .await
        .unwrap()
        .result
    else {
        panic!()
    };

    assert_eq!(
        alex.ticket(ticket_id)
            .await
            .unwrap()
            .ticket
            .responsible_user_id,
        Some(bea_id)
    );
    assert_eq!(
        followers(&alex, ticket_id).await,
        vec![pair("alex", "created"), pair("bea", "responsible")]
    );
    assert_eq!(
        events_of(dir.path(), EventKind::TicketUpdated, ticket_id).len(),
        1,
        "someone else: it is news"
    );

    drop((alex, _bea));
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_new_ticket_for_its_creator_follows_once_and_makes_no_line_of_it() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let alex = connect(dir.path()).await;
    let alex_id = user_id(&alex).await;
    let (project_id, _) = project(&alex).await;
    let CommandResult::TicketCreated { ticket_id, .. } = alex
        .create_ticket(new_ticket(project_id, None, Some(alex_id)))
        .await
        .unwrap()
        .result
    else {
        panic!()
    };

    assert_eq!(
        alex.ticket(ticket_id)
            .await
            .unwrap()
            .ticket
            .responsible_user_id,
        Some(alex_id)
    );
    assert_eq!(
        followers(&alex, ticket_id).await,
        vec![pair("alex", "created")]
    );
    assert!(events_of(dir.path(), EventKind::TicketUpdated, ticket_id).is_empty());
    assert_eq!(
        events_of(dir.path(), EventKind::TicketFollowChanged, ticket_id).len(),
        1
    );

    drop(alex);
    stop(dir.path(), daemon).await;
}

// ---- ticket.follow ----

#[tokio::test(flavor = "multi_thread")]
async fn following_is_idempotent_and_only_a_real_change_gives_an_event() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let (alex, _, bea, _) = two_members(dir.path()).await;
    let (project_id, _) = project(&alex).await;
    let (ticket_id, _) = ticket(&alex, project_id).await;
    let events = |root: &Path| events_of(root, EventKind::TicketFollowChanged, ticket_id).len();
    let start_count = events(dir.path());

    let change = |on: bool, changed: bool| CommandResult::TicketFollowChanged {
        ticket_id,
        on,
        changed,
    };
    assert_eq!(
        run(&bea, follow(ticket_id, false)).await.unwrap(),
        change(false, false)
    );
    assert_eq!(events(dir.path()), start_count);
    assert_eq!(
        run(&bea, follow(ticket_id, true)).await.unwrap(),
        change(true, true)
    );
    assert_eq!(
        run(&bea, follow(ticket_id, true)).await.unwrap(),
        change(true, false)
    );
    assert_eq!(events(dir.path()), start_count + 1);
    assert_eq!(
        followers(&alex, ticket_id).await,
        vec![pair("alex", "created"), pair("bea", "manual")]
    );
    assert_eq!(
        run(&bea, follow(ticket_id, false)).await.unwrap(),
        change(false, true)
    );
    assert_eq!(events(dir.path()), start_count + 2);
    assert_eq!(
        followers(&alex, ticket_id).await,
        vec![pair("alex", "created")]
    );

    let last = events_of(dir.path(), EventKind::TicketFollowChanged, ticket_id)
        .pop()
        .unwrap();
    assert_eq!(
        (last["on"].as_bool(), last["reason"].is_null()),
        (Some(false), true)
    );
    assert_eq!(
        code(run(&bea, follow(TicketId::new(), true)).await.unwrap_err()),
        ErrorCode::NotFound
    );

    drop((alex, bea));
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn following_leaves_the_ticket_version_alone_so_an_older_client_gets_no_conflict() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let (alex, _, bea, _) = two_members(dir.path()).await;
    let (project_id, _) = project(&alex).await;
    let (ticket_id, _) = ticket(&alex, project_id).await;
    let seen = alex.ticket(ticket_id).await.unwrap().ticket;

    run(&bea, follow(ticket_id, true)).await.unwrap();
    run(&bea, follow(ticket_id, false)).await.unwrap();

    let now = alex.ticket(ticket_id).await.unwrap().ticket;
    assert_eq!(
        (now.version, now.updated_at_ms),
        (seen.version, seen.updated_at_ms)
    );
    // A client that never hears of the follow writes with the version it has; events go on from there.
    let update = UpdateTicket { design_url: None,
        ticket_id,
        title: Some("Login v2".into()),
        description: None,
        priority: None,
        labels: None,
        deadline_ms: None,
        clear_deadline: false,
        run: None,
        add_files: None,
        confirm_secret: false,
    };
    alex.execute(aurelius_client::envelope(
        Command::UpdateTicket(update),
        Some(seen.version),
    ))
    .await
    .unwrap();

    drop((alex, bea));
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_active_responsible_cannot_unfollow_until_they_handed_over() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let (alex, _, bea, bea_id) = two_members(dir.path()).await;
    let (project_id, _) = project(&alex).await;
    let (ticket_id, _) = ticket(&alex, project_id).await;
    run(&alex, set_responsible(ticket_id, Some(bea_id)))
        .await
        .unwrap();
    let events = events_of(dir.path(), EventKind::TicketFollowChanged, ticket_id).len();

    let key = CommandId::new();
    let refused = bea
        .execute(CommandEnvelope {
            command_id: key,
            expected_version: None,
            command: follow(ticket_id, false),
        })
        .await
        .unwrap_err();
    assert_eq!(code(refused), ErrorCode::Validation);
    assert_eq!(
        count(
            dir.path(),
            "SELECT COUNT(*) FROM command_receipts WHERE command_id = ?1",
            &key.to_string()
        ),
        0
    );
    assert_eq!(
        events_of(dir.path(), EventKind::TicketFollowChanged, ticket_id).len(),
        events
    );
    assert!(
        followers(&alex, ticket_id)
            .await
            .contains(&pair("bea", "responsible"))
    );

    // Someone else, not responsible, still can.
    assert!(matches!(
        run(&alex, follow(ticket_id, false)).await.unwrap(),
        CommandResult::TicketFollowChanged { changed: true, .. }
    ));
    // After the handover so can they.
    run(&alex, set_responsible(ticket_id, None)).await.unwrap();
    assert!(matches!(
        run(&bea, follow(ticket_id, false)).await.unwrap(),
        CommandResult::TicketFollowChanged { changed: true, .. }
    ));

    drop((alex, bea));
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_closed_ticket_can_still_be_followed_and_unfollowed() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let (alex, _, bea, _) = two_members(dir.path()).await;
    let (project_id, _) = project(&alex).await;
    let (done, _) = ticket(&alex, project_id).await;
    put_in_column(dir.path(), done, "done");
    assert!(matches!(
        run(&bea, follow(done, true)).await.unwrap(),
        CommandResult::TicketFollowChanged { changed: true, .. }
    ));
    let (archived, _) = ticket(&alex, project_id).await;
    run(
        &alex,
        Command::ArchiveTicket(ArchiveTicket {
            ticket_id: archived,
        }),
    )
    .await
    .unwrap();
    assert!(matches!(
        run(&alex, follow(archived, false)).await.unwrap(),
        CommandResult::TicketFollowChanged { changed: true, .. }
    ));

    drop((alex, bea));
    stop(dir.path(), daemon).await;
}

// ---- following by a mention (US-A2) ----

async fn post(client: &Client, ticket_id: TicketId, text: &str) {
    let post = PostTicketMessage {
        ticket_id,
        text: text.into(),
        images: vec![],
        files: vec![],
        confirm_secret: false,
        thread_parent: None,
    };
    run(client, Command::PostTicketMessage(post)).await.unwrap();
}

fn describe(ticket_id: TicketId, text: &str) -> Command {
    Command::UpdateTicket(UpdateTicket { design_url: None,
        ticket_id,
        title: None,
        description: Some(text.into()),
        priority: None,
        labels: None,
        deadline_ms: None,
        clear_deadline: false,
        run: None,
        add_files: None,
        confirm_secret: false,
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn a_mention_in_the_discussion_makes_someone_follow_again_after_they_left() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let (alex, _, bea, _) = two_members(dir.path()).await;
    let (project_id, _) = project(&alex).await;
    let (ticket_id, _) = ticket(&alex, project_id).await;

    post(&alex, ticket_id, "Nobody in particular").await;
    assert_eq!(
        followers(&alex, ticket_id).await,
        vec![pair("alex", "created")]
    );
    post(&alex, ticket_id, "What do you think, @bea?").await;
    assert_eq!(
        followers(&alex, ticket_id).await,
        vec![pair("alex", "created"), pair("bea", "mentioned")]
    );

    run(&bea, follow(ticket_id, false)).await.unwrap();
    assert_eq!(
        followers(&alex, ticket_id).await,
        vec![pair("alex", "created")]
    );
    post(&alex, ticket_id, "@bea once more").await;
    assert_eq!(
        followers(&alex, ticket_id).await,
        vec![pair("alex", "created"), pair("bea", "mentioned")]
    );

    drop((alex, bea));
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn only_a_new_name_in_the_description_makes_someone_follow() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let (alex, _, bea, _) = two_members(dir.path()).await;
    let (project_id, _) = project(&alex).await;
    let CommandResult::TicketCreated { ticket_id, .. } = alex
        .create_ticket(new_ticket(project_id, Some("For @bea to look at"), None))
        .await
        .unwrap()
        .result
    else {
        panic!()
    };
    assert_eq!(
        followers(&alex, ticket_id).await,
        vec![pair("alex", "created"), pair("bea", "mentioned")]
    );

    // A typo fixed elsewhere in the text must not bring back who unfollowed.
    run(&bea, follow(ticket_id, false)).await.unwrap();
    run(&alex, describe(ticket_id, "For @bea to look at, please"))
        .await
        .unwrap();
    assert_eq!(
        followers(&alex, ticket_id).await,
        vec![pair("alex", "created")]
    );

    run(&alex, describe(ticket_id, "Nothing about anyone"))
        .await
        .unwrap();
    run(&alex, describe(ticket_id, "Now @bea again"))
        .await
        .unwrap();
    assert_eq!(
        followers(&alex, ticket_id).await,
        vec![pair("alex", "created"), pair("bea", "mentioned")]
    );

    drop((alex, bea));
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_mention_in_a_channel_or_of_a_disabled_member_makes_nobody_follow() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let (alex, _, bea, bea_id) = two_members(dir.path()).await;
    let (project_id, channel) = project(&alex).await;
    let (ticket_id, _) = ticket(&alex, project_id).await;

    let send = SendGroupMessage {
        confirm_secret: false,
        text: "@bea see the ticket".into(),
        images: vec![],
        files: vec![],
        channel: Some(channel),
        thread_parent: None,
    };
    run(&alex, Command::SendGroupMessage(send)).await.unwrap();
    assert_eq!(
        count(
            dir.path(),
            "SELECT COUNT(*) FROM ticket_followers WHERE user_id = ?1",
            &bea_id.to_string()
        ),
        0
    );

    drop(bea);
    alex.disable_user(bea_id).await.unwrap();
    post(&alex, ticket_id, "@bea are you there?").await;
    assert_eq!(
        followers(&alex, ticket_id).await,
        vec![pair("alex", "created")]
    );

    drop(alex);
    stop(dir.path(), daemon).await;
}

// ---- ticket.followers ----

#[tokio::test(flavor = "multi_thread")]
async fn the_followers_of_an_unknown_ticket_are_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let alex = connect(dir.path()).await;
    assert!(alex.supports(feature::TICKET_RESPONSIBLE) && alex.supports(feature::TICKET_FOLLOW));
    assert_eq!(
        code(alex.ticket_followers(TicketId::new()).await.unwrap_err()),
        ErrorCode::NotFound
    );

    drop(alex);
    stop(dir.path(), daemon).await;
}

// ---- Activity ----

#[tokio::test(flavor = "multi_thread")]
async fn the_activity_says_who_changed_the_responsible_and_when() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let (alex, alex_id, _bea, bea_id) = two_members(dir.path()).await;
    let (project_id, _) = project(&alex).await;
    let (ticket_id, number) = ticket(&alex, project_id).await;
    let started = aurelius_storage::now_ms();

    run(&alex, set_responsible(ticket_id, Some(bea_id)))
        .await
        .unwrap();
    run(&alex, set_responsible(ticket_id, Some(alex_id)))
        .await
        .unwrap();
    run(&alex, set_responsible(ticket_id, None)).await.unwrap();

    // Also without routine events: this is what people want to read later.
    for activity in [false, true] {
        let page = alex
            .timeline(TimelineQuery {
                project_id: None,
                correlation_id: Some(ticket_id.to_string()),
                before_seq: None,
                limit: 50,
                activity,
            })
            .await
            .unwrap();
        let mut cards: Vec<_> = page
            .cards
            .into_iter()
            .filter(|c| c.label == "RESPONSIBLE")
            .collect();
        cards.reverse();
        let words: Vec<(&str, &str, &str)> = cards
            .iter()
            .map(|c| (c.title.as_str(), c.detail.as_str(), c.actor.as_str()))
            .collect();
        let (made, handed, removed) = (
            format!("alex made bea responsible for #{number}"),
            format!("alex made alex responsible for #{number}"),
            format!("alex removed alex as responsible for #{number}"),
        );
        assert_eq!(
            words,
            vec![
                (made.as_str(), "Nobody was responsible before", "alex"),
                (handed.as_str(), "bea was responsible before", "alex"),
                (removed.as_str(), "Nobody is responsible now", "alex"),
            ]
        );
        assert!(
            cards
                .iter()
                .all(|c| c.at_ms >= started && c.detail.contains("responsible")),
            "the clients look for this word"
        );
    }

    drop((alex, _bea));
    stop(dir.path(), daemon).await;
}

// ---- what an older client gets (R1) ----

/// A raw connection signed in as `BOARD_MEMBER`, whose hello accepts `accepts`.
async fn raw(root: &Path, accepts: &[&str]) -> BoxStream {
    let mut stream = ipc::connect(&aurelius_host::endpoint(root)).await.unwrap();
    let token = sign_in(root, &connect_anonymous(root).await, BOARD_MEMBER)
        .await
        .session_token;
    let accepts = accepts.iter().map(|a| (*a).to_owned()).collect();
    let hello = Request::read(
        1,
        Method::SessionHello(HelloParams {
            client_name: "raw".into(),
            client_version: "0".into(),
            accepts,
            session_token: Some(token),
        }),
    );
    write_frame(&mut stream, &hello).await.unwrap();
    loop {
        if let ServerMessage::Response { request_id: 1, .. } =
            read_frame::<ServerMessage, _>(&mut stream)
                .await
                .unwrap()
                .unwrap()
        {
            return stream;
        }
    }
}

/// The kinds of the events that arrive until the connection has been quiet for a moment.
async fn kinds_until_quiet(stream: &mut BoxStream) -> Vec<EventKind> {
    let mut kinds = Vec::new();
    while let Ok(Ok(Some(message))) = tokio::time::timeout(
        Duration::from_millis(500),
        read_frame::<ServerMessage, _>(stream),
    )
    .await
    {
        if let ServerMessage::Event(event) = message {
            kinds.push(event.kind);
        }
    }
    kinds
}

async fn subscribe_from(stream: &mut BoxStream, after_seq: u64) -> Vec<EventKind> {
    write_frame(
        stream,
        &Request::read(2, Method::EventSubscribe { after_seq }),
    )
    .await
    .unwrap();
    kinds_until_quiet(stream).await
}

#[tokio::test(flavor = "multi_thread")]
async fn a_client_without_accepts_never_gets_a_follow_event_live_or_in_the_replay() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let alex = connect(dir.path()).await;
    let (project_id, _) = project(&alex).await;
    let (ticket_id, _) = ticket(&alex, project_id).await;

    let mut older = raw(dir.path(), &[client_feature::TEAM_CHANNELS]).await;
    let mut newer = raw(
        dir.path(),
        &[client_feature::TEAM_CHANNELS, client_feature::TICKET_FOLLOW],
    )
    .await;
    let replayed = subscribe_from(&mut older, 0).await;
    assert!(replayed.contains(&EventKind::TicketCreated), "{replayed:?}");
    assert!(
        !replayed.contains(&EventKind::TicketFollowChanged),
        "the replay: {replayed:?}"
    );
    let replayed = subscribe_from(&mut newer, 0).await;
    assert_eq!(
        replayed
            .iter()
            .filter(|k| **k == EventKind::TicketFollowChanged)
            .count(),
        1,
        "the creator: {replayed:?}"
    );

    // Live: the older client gets the handover it knows, not the follow that came with it.
    let alex_id = user_id(&alex).await;
    run(&alex, follow(ticket_id, false)).await.unwrap();
    run(&alex, set_responsible(ticket_id, Some(alex_id)))
        .await
        .unwrap();
    let live = kinds_until_quiet(&mut older).await;
    assert!(live.contains(&EventKind::TicketUpdated), "{live:?}");
    assert!(
        !live.contains(&EventKind::TicketFollowChanged),
        "live: {live:?}"
    );
    let live = kinds_until_quiet(&mut newer).await;
    assert_eq!(
        live.iter()
            .filter(|k| **k == EventKind::TicketFollowChanged)
            .count(),
        2,
        "{live:?}"
    );

    drop((alex, older, newer));
    stop(dir.path(), daemon).await;
}

/// `SystemLineKind` as a client from before phase A has it (`channels_tests.rs`).
#[derive(Debug, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum OldSystemLineKind {
    Delegated,
    Reported,
    Moved,
    Plan,
    Review,
    Done,
    CreatedFrom,
    TicketCreated,
    Summary,
    SummaryFailed,
    Limit,
}

#[derive(Debug, Deserialize)]
struct OldSystemLine {
    kind: OldSystemLineKind,
    #[serde(default)]
    by: Option<String>,
    #[serde(default)]
    to: Option<String>,
}

#[tokio::test(flavor = "multi_thread")]
async fn an_older_client_reads_the_line_of_a_handover_with_its_own_enum() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let (alex, _, _bea, bea_id) = two_members(dir.path()).await;
    let (project_id, _) = project(&alex).await;
    let (ticket_id, _) = ticket(&alex, project_id).await;
    run(&alex, set_responsible(ticket_id, Some(bea_id)))
        .await
        .unwrap();
    run(&alex, set_responsible(ticket_id, None)).await.unwrap();

    let discussion = alex
        .ticket(ticket_id)
        .await
        .unwrap()
        .ticket
        .conversation_id
        .expect("a discussion");
    let mut older = raw(dir.path(), &[client_feature::TEAM_CHANNELS]).await;
    write_frame(
        &mut older,
        &Request::read(
            40,
            Method::GroupChatGet {
                channel: Some(discussion),
                peek: false,
            },
        ),
    )
    .await
    .unwrap();
    let snapshot = loop {
        if let ServerMessage::Response {
            request_id: 40,
            outcome,
        } = read_frame::<ServerMessage, _>(&mut older)
            .await
            .unwrap()
            .unwrap()
        {
            let Outcome::Ok(ResponseBody::Messages(snapshot)) = outcome else {
                panic!("{outcome:?}")
            };
            break snapshot;
        }
    };
    let lines: Vec<(OldSystemLine, String)> = snapshot
        .messages
        .iter()
        .filter_map(|m| {
            m.system.as_ref().map(|s| {
                (
                    serde_json::from_value(serde_json::to_value(s).unwrap())
                        .expect("every line reads with the old enum"),
                    m.text.clone(),
                )
            })
        })
        .collect();
    let handovers: Vec<_> = lines
        .iter()
        .filter(|(_, text)| text.contains("responsible"))
        .collect();
    assert_eq!(handovers.len(), 2, "{lines:?}");
    assert_eq!(handovers[0].0.kind, OldSystemLineKind::Moved);
    assert_eq!(
        (
            handovers[0].0.by.as_deref(),
            handovers[0].0.to.as_deref(),
            handovers[0].1.as_str()
        ),
        (Some("alex"), Some("bea"), "alex made bea responsible")
    );
    assert_eq!(
        (handovers[1].0.to.as_deref(), handovers[1].1.as_str()),
        (None, "alex removed bea as responsible")
    );

    drop((alex, _bea, older));
    stop(dir.path(), daemon).await;
}
