//! Tickets, the board's columns and the company row (migration 18).

use crate::tickets::{self, NewTicket, TicketFilter};
use crate::{StorageError, Store, company, events, tasks, users};
use aurelius_domain::task::{RunSettings, TaskStatus};
use aurelius_domain::ticket::{ColumnKind, TicketPriority};
use aurelius_domain::{EmployeeId, ProjectId, TaskId, TicketId, UserId};

const PHC: &str = "$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHQ$aGFzaGhhc2hoYXNoaGFzaA";

struct Fixture {
    _dir: tempfile::TempDir,
    store: Store,
    user: UserId,
    atlas: ProjectId,
    hermes: ProjectId,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let (user, atlas, hermes) = (UserId::new(), ProjectId::new(), ProjectId::new());
    store
        .write(move |tx| {
            users::insert(tx, &users::NewUser { id: &user.to_string(), username: "grace", created_by_user: None, now_ms: 1 }, PHC)?;
            events::insert_project_projection(tx, atlas, "Atlas", None)?;
            events::insert_project_projection(tx, hermes, "Hermes", None)?;
            Ok(())
        })
        .unwrap();
    Fixture { _dir: dir, store, user, atlas, hermes }
}

fn add(f: &Fixture, project: ProjectId, title: &str) -> (TicketId, u32) {
    let (id, user, title) = (TicketId::new(), f.user, title.to_owned());
    let number = f
        .store
        .write(move |tx| {
            let backlog = company::column_of_kind(tx, ColumnKind::Backlog)?;
            tickets::insert(
                tx,
                &NewTicket {
                    id,
                    project_id: project,
                    title: &title,
                    description_blob: None,
                    column_id: backlog.id,
                    priority: TicketPriority::default(),
                    created_by_user: user,
                    owner: None,
                    run: &RunSettings::default(),
                    labels: &["ui".to_owned()],
                    deadline_ms: None,
                },
            )
        })
        .unwrap();
    (id, number)
}

fn read_blob(_: &str) -> Option<String> {
    None
}

fn titles_in(f: &Fixture, kind: ColumnKind) -> Vec<String> {
    f.store
        .read(|c| Ok(tickets::list(c, &TicketFilter { column: Some(kind), ..Default::default() }, &read_blob)?))
        .unwrap()
        .into_iter()
        .map(|t| t.title)
        .collect()
}

#[test]
fn the_board_has_six_fixed_columns_and_one_company() {
    let f = fixture();
    let columns = f.store.read(company::columns).unwrap();
    assert_eq!(columns.iter().map(|c| c.kind).collect::<Vec<_>>(), ColumnKind::ALL);
    assert_eq!(columns.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["Backlog", "Todo", "Doing", "Plan review", "Review", "Done"]);
    let company = f.store.read(company::get).unwrap();
    assert_eq!((company.name.as_str(), company.dispatcher_employee_id), ("Company", None));

    // Renamed and reordered: the kinds stay, the order follows the list.
    let mut order: Vec<_> = columns.iter().rev().map(|c| (c.id, format!("{}!", c.name))).collect();
    order.swap(0, 5);
    f.store.write(move |tx| company::update_columns(tx, &order)).unwrap();
    let after = f.store.read(company::columns).unwrap();
    assert_eq!(after[0].kind, ColumnKind::Backlog);
    assert_eq!(after[1].kind, ColumnKind::Review);
    assert_eq!(after[1].name, "Review!");
    assert!(after.iter().all(|c| c.version == 2));
}

#[test]
fn tickets_are_numbered_per_company_and_start_in_backlog() {
    let f = fixture();
    let (first, one) = add(&f, f.atlas, "First");
    let (_, two) = add(&f, f.hermes, "Second");
    assert_eq!((one, two), (1, 2), "one counter for every project");
    let view = f.store.read(|c| tickets::view(c, first, &read_blob)).unwrap().unwrap();
    assert_eq!(view.column, ColumnKind::Backlog);
    assert_eq!(view.priority, TicketPriority::Normal);
    assert_eq!((view.created_by_user, view.created_by_username.as_str()), (f.user, "grace"));
    assert_eq!((view.project_name.as_str(), view.labels.clone()), ("Atlas", vec!["ui".to_owned()]));
    assert_eq!(view.version, 1);
    let state = f.store.read(|c| tickets::state(c, first)).unwrap().unwrap();
    assert_eq!((state.column, state.root_task_id, state.archived), (ColumnKind::Backlog, None, false));
}

#[test]
fn created_by_user_must_be_a_board_member() {
    let f = fixture();
    let stranger = UserId::new();
    let project = f.atlas;
    let result = f.store.write(move |tx| {
        let backlog = company::column_of_kind(tx, ColumnKind::Backlog)?;
        tickets::insert(
            tx,
            &NewTicket {
                id: TicketId::new(),
                project_id: project,
                title: "Nobody's",
                description_blob: None,
                column_id: backlog.id,
                priority: TicketPriority::High,
                created_by_user: stranger,
                owner: None,
                run: &RunSettings::default(),
                labels: &[],
                deadline_ms: None,
            },
        )
    });
    assert!(result.is_err(), "the foreign key on users refuses an unknown author");
}

#[test]
fn move_to_keeps_an_exact_order_and_the_board_filters_per_project() {
    let f = fixture();
    let (a, _) = add(&f, f.atlas, "A");
    let (b, _) = add(&f, f.atlas, "B");
    let (c, _) = add(&f, f.hermes, "C");
    assert_eq!(titles_in(&f, ColumnKind::Backlog), ["A", "B", "C"]);
    let backlog = f.store.read(|conn| company::column_of_kind(conn, ColumnKind::Backlog)).unwrap();
    let todo = f.store.read(|conn| company::column_of_kind(conn, ColumnKind::Todo)).unwrap();
    f.store.write(move |tx| tickets::move_to(tx, c, backlog.id, Some(a))).unwrap();
    assert_eq!(titles_in(&f, ColumnKind::Backlog), ["C", "A", "B"]);
    f.store.write(move |tx| tickets::move_to(tx, a, todo.id, None)).unwrap();
    f.store.write(move |tx| tickets::move_to(tx, b, todo.id, Some(a))).unwrap();
    // Above a card that is not in the target column: refused.
    let wrong = f.store.write(move |tx| tickets::move_to(tx, c, todo.id, Some(c)));
    assert!(matches!(wrong, Err(StorageError::Rejected(why)) if why == tickets::NOT_IN_COLUMN));
    assert_eq!(titles_in(&f, ColumnKind::Todo), ["B", "A"]);
    assert_eq!(titles_in(&f, ColumnKind::Backlog), ["C"]);

    let atlas = f.store.read(|conn| tickets::board(conn, Some(f.atlas), &read_blob)).unwrap();
    assert_eq!(atlas.len(), 6);
    assert!(atlas[0].tickets.is_empty(), "C is Hermes'");
    assert_eq!(atlas[1].tickets.iter().map(|t| t.title.as_str()).collect::<Vec<_>>(), ["B", "A"]);
    let all = f.store.read(|conn| tickets::board(conn, None, &read_blob)).unwrap();
    assert_eq!(all.iter().map(|col| col.tickets.len()).sum::<usize>(), 3);

    // Archived cards leave the board but stay readable.
    f.store.write(move |tx| tickets::mark_archived(tx, c)).unwrap();
    let all = f.store.read(|conn| tickets::board(conn, None, &read_blob)).unwrap();
    assert_eq!(all.iter().map(|col| col.tickets.len()).sum::<usize>(), 2);
    assert!(f.store.read(|conn| tickets::view(conn, c, &read_blob)).unwrap().unwrap().archived_at_ms.is_some());
}

#[test]
fn the_list_searches_title_and_number() {
    let f = fixture();
    add(&f, f.atlas, "Fix the login");
    add(&f, f.atlas, "Board drag");
    let search = |q: &str| {
        let filter = TicketFilter { query: Some(q.to_owned()), ..Default::default() };
        f.store.read(|c| tickets::list(c, &filter, &read_blob)).unwrap().into_iter().map(|t| t.title).collect::<Vec<_>>()
    };
    assert_eq!(search("LOGIN"), ["Fix the login"]);
    assert_eq!(search("#2"), ["Board drag"]);
    assert!(search("nothing").is_empty());
}

#[test]
fn tasks_carry_their_ticket_and_delegations_inherit_it() {
    let f = fixture();
    let (ticket, number) = add(&f, f.atlas, "With tasks");
    let (root, child, loose) = (TaskId::new(), TaskId::new(), TaskId::new());
    let project = f.atlas;
    f.store
        .write(move |tx| {
            let blob = f_blob(tx)?;
            tasks::insert_ticket_task(tx, root, project, None, "Main", &blob, &blob, TaskStatus::Draft, None, Some(ticket))?;
            tasks::insert_task(tx, child, project, None, "Part", &blob, &blob, TaskStatus::Queued, Some(root))?;
            tasks::insert_task(tx, loose, project, None, "Loose", &blob, &blob, TaskStatus::Queued, None)?;
            tickets::set_root_task(tx, ticket, Some(root))?;
            Ok(())
        })
        .unwrap();
    f.store
        .read(|c| {
            assert_eq!(tickets::ticket_of_task(c, root)?, Some(ticket));
            assert_eq!(tickets::ticket_of_task(c, child)?, Some(ticket), "a delegation stays on the ticket");
            assert_eq!(tickets::ticket_of_task(c, loose)?, None);
            assert_eq!(tickets::tasks_of_ticket(c, ticket)?, vec![root, child]);
            assert_eq!(tickets::working_tasks(c, ticket, Some(root))?, 1);
            let view = tasks::task_view(c, child, &read_blob)?.unwrap();
            assert_eq!((view.ticket_id, view.ticket_number), (Some(ticket), Some(number)));
            assert_eq!(tasks::task_view(c, loose, &read_blob)?.unwrap().ticket_id, None);
            let state = tickets::state(c, ticket)?.unwrap();
            assert_eq!((state.root_task_id, state.root_status), (Some(root), Some(TaskStatus::Draft)));
            Ok(())
        })
        .unwrap();
}

fn f_blob(tx: &crate::Tx<'_>) -> Result<String, crate::StorageError> {
    let id = aurelius_domain::BlobId::new().to_string();
    tx.execute(
        "INSERT INTO blobs(id, sha256, media_type, byte_length, storage_key, created_at_ms) VALUES (?1, ?1, 'text/plain', 1, ?1, 1)",
        [&id],
    )?;
    Ok(id)
}

#[test]
fn the_dispatcher_is_the_projects_else_the_companys_while_active() {
    let f = fixture();
    let (orion, forge) = (EmployeeId::new(), EmployeeId::new());
    f.store
        .write(move |tx| {
            for (id, name) in [(orion, "Orion"), (forge, "Forge")] {
                tx.execute(
                    "INSERT INTO scopes(id, kind, parent_id, owner_id) VALUES (?1, 'employee', (SELECT id FROM scopes WHERE kind = 'global'), ?2)",
                    [aurelius_domain::ScopeId::new().to_string(), id.to_string()],
                )?;
                tx.execute(
                    "INSERT INTO employees(id, scope_id, name, role, config_json, status, version, created_at_ms, updated_at_ms)
                     VALUES (?1, (SELECT id FROM scopes WHERE owner_id = ?1), ?2, 'r', '{}', 'active', 1, 1, 1)",
                    [id.to_string(), name.to_owned()],
                )?;
            }
            Ok(())
        })
        .unwrap();
    let atlas = f.atlas;
    assert_eq!(f.store.read(|c| company::dispatcher_for(c, atlas)).unwrap(), None, "B5: none yet");
    f.store.write(move |tx| company::update(tx, Some("Segrusa"), Some(Some(orion)))).unwrap();
    assert_eq!(f.store.read(|c| company::dispatcher_for(c, atlas)).unwrap(), Some(orion));
    let version = f.store.write(move |tx| company::set_project_dispatcher(tx, atlas, Some(forge))).unwrap();
    assert_eq!(version, 2);
    assert_eq!(f.store.read(|c| company::dispatcher_for(c, atlas)).unwrap(), Some(forge));
    f.store.write(move |tx| Ok(tx.execute("UPDATE employees SET status = 'archived' WHERE id = ?1", [forge.to_string()])?)).unwrap();
    assert_eq!(f.store.read(|c| company::dispatcher_for(c, atlas)).unwrap(), None, "B5: an archived project dispatcher is none, no fall back to the company's");
    let company = f.store.read(company::get).unwrap();
    assert_eq!((company.name.as_str(), company.version), ("Segrusa", 2));
    let projects = f.store.read(|c| crate::queries::list_projects(c, &read_blob)).unwrap();
    assert_eq!(projects.iter().find(|p| p.id == atlas).unwrap().dispatcher_employee_id, Some(forge));
    f.store.write(move |tx| company::set_project_dispatcher(tx, atlas, None).map(drop)).unwrap();
    assert_eq!(f.store.read(|c| company::dispatcher_for(c, atlas)).unwrap(), Some(orion), "without its own, the company's");
}

/// A ticket's Activity (`timeline.list` on its id): the board member's writes
/// (correlated by their command) together with the engine's (correlated by
/// the ticket), once each, newest first.
#[test]
fn a_tickets_activity_has_the_board_writes_and_the_engines_once_in_order() {
    let f = fixture();
    let (ticket, _) = add(&f, f.atlas, "Activity");
    let (other, _) = add(&f, f.atlas, "Someone else's");
    let (main, project) = (TaskId::new(), f.atlas);
    f.store
        .write(move |tx| {
            let mut versions = std::collections::HashMap::<String, u64>::new();
            let mut push = |aggregate_type: &'static str, id: String, kind: &'static str, actor: aurelius_domain::ActorKind, correlation: String| {
                let version = versions.entry(format!("{aggregate_type}:{id}")).or_default();
                *version += 1;
                events::append(
                    tx,
                    events::NewEvent {
                        aggregate_type,
                        aggregate_id: id,
                        aggregate_version: *version,
                        event_type: kind,
                        project_id: Some(project),
                        actor_kind: actor,
                        actor_id: None,
                        correlation_id: correlation,
                        causation_id: None,
                        payload_blob_id: None,
                        metadata: serde_json::json!({}),
                        dedupe_key: aurelius_domain::EventId::new().to_string(),
                    },
                )
                .map(|_| ())
            };
            use aurelius_domain::ActorKind::{System, User};
            let (t, cmd) = (ticket.to_string(), || aurelius_domain::CommandId::new().to_string());
            push("ticket", t.clone(), "ticket.created", User, cmd())?;
            push("ticket", other.to_string(), "ticket.created", User, cmd())?;
            let start = cmd();
            push("ticket", t.clone(), "ticket.moved", User, start.clone())?;
            push("ticket", t.clone(), "ticket.started", User, start)?;
            push("task", main.to_string(), "task.created", System, t.clone())?;
            push("ticket", t.clone(), "ticket.updated", System, t.clone())?;
            let cancel = cmd();
            push("ticket", t.clone(), "ticket.moved", User, cancel.clone())?;
            push("ticket", t.clone(), "ticket.cancelled", User, cancel)?;
            push("task", main.to_string(), "task.cancelled", System, t.clone())?;
            let again = cmd();
            push("ticket", t.clone(), "ticket.moved", User, again.clone())?;
            push("ticket", t.clone(), "ticket.started", User, again)?;
            Ok(())
        })
        .unwrap();
    let (cards, next) = f.store.read(|c| crate::timeline::cards(c, None, Some(&ticket.to_string()), None, 50, true)).unwrap();
    assert_eq!(next, None);
    let titles: Vec<&str> = cards.iter().map(|c| c.title.as_str()).collect();
    assert_eq!(
        titles,
        [
            "ticket.started",
            "ticket.moved",
            " cancelled",
            "ticket.cancelled",
            "ticket.moved",
            "ticket.updated",
            "You → ",
            "ticket.started",
            "ticket.moved",
            "ticket.created",
        ],
        "newest first, the other ticket left out"
    );
    let mut seqs: Vec<u64> = cards.iter().map(|c| c.seq).collect();
    assert!(seqs.windows(2).all(|w| w[0] > w[1]), "strictly newest first: {seqs:?}");
    seqs.dedup();
    assert_eq!(seqs.len(), cards.len(), "no event twice");
    // Paging keeps the same order without gaps or repeats.
    let (first, next) = f.store.read(|c| crate::timeline::cards(c, None, Some(&ticket.to_string()), None, 4, true)).unwrap();
    let (rest, _) = f.store.read(move |c| crate::timeline::cards(c, None, Some(&ticket.to_string()), next, 50, true)).unwrap();
    assert_eq!(first.iter().chain(&rest).map(|c| c.seq).collect::<Vec<_>>(), cards.iter().map(|c| c.seq).collect::<Vec<_>>());
}
