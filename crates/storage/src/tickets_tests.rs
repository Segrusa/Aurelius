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
fn the_board_has_seven_fixed_columns_and_one_company() {
    let f = fixture();
    let columns = f.store.read(company::columns).unwrap();
    assert_eq!(columns.iter().map(|c| c.kind).collect::<Vec<_>>(), ColumnKind::ALL);
    assert_eq!(columns.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["Backlog", "Todo", "Doing", "Plan review", "Testing", "Review", "Done"]);
    let company = f.store.read(company::get).unwrap();
    assert_eq!((company.name.as_str(), company.dispatcher_employee_id, company.tester_employee_id), ("Company", None, None));

    // Renamed and reordered: the kinds stay, the order follows the list.
    let mut order: Vec<_> = columns.iter().rev().map(|c| (c.id, format!("{}!", c.name))).collect();
    order.swap(0, 6);
    f.store.write(move |tx| company::update_columns(tx, &order)).unwrap();
    let after = f.store.read(company::columns).unwrap();
    assert_eq!(after[0].kind, ColumnKind::Backlog);
    assert_eq!(after[1].kind, ColumnKind::Review);
    assert_eq!(after[1].name, "Review!");
    // Each column counts one up (migration 25 already moved Review and Done one place right).
    assert!(after.iter().all(|a| columns.iter().any(|c| c.id == a.id && a.version == c.version + 1)));
    assert_eq!(columns.iter().map(|c| c.version).collect::<Vec<_>>(), [1, 1, 1, 1, 2, 2, 2]);
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
    assert_eq!(atlas.len(), 7);
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
    f.store.write(move |tx| company::update(tx, Some("Segrusa"), Some(Some(orion)), None)).unwrap();
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

/// Ticket #5: a `ticket.tested` card says in `detail` whether the tests
/// passed, which is all the clients read to word "Tests passed/failed".
#[test]
fn a_tested_card_says_passed_or_failed_in_its_detail() {
    let f = fixture();
    let (ticket, _) = add(&f, f.atlas, "Tested");
    let project = f.atlas;
    f.store
        .write(move |tx| {
            for (version, passed) in [(1, false), (2, true)] {
                events::append(
                    tx,
                    events::NewEvent {
                        aggregate_type: "ticket",
                        aggregate_id: ticket.to_string(),
                        aggregate_version: version,
                        event_type: "ticket.tested",
                        project_id: Some(project),
                        actor_kind: aurelius_domain::ActorKind::Employee,
                        actor_id: None,
                        correlation_id: ticket.to_string(),
                        causation_id: None,
                        payload_blob_id: None,
                        metadata: serde_json::json!({ "passed": passed, "task_id": TaskId::new().to_string() }),
                        dedupe_key: aurelius_domain::EventId::new().to_string(),
                    },
                )?;
            }
            Ok(())
        })
        .unwrap();
    let (cards, _) = f.store.read(|c| crate::timeline::cards(c, None, Some(&ticket.to_string()), None, 50, true)).unwrap();
    let lines: Vec<(&str, &str)> = cards.iter().filter(|c| c.label == "TESTING").map(|c| (c.title.as_str(), c.detail.as_str())).collect();
    assert_eq!(lines, [("Tests passed", "passed"), ("Tests failed", "failed")], "newest first");
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

// ---- Responsible and my tickets (AUR-FA-S1) --------------------------------

fn second_user(f: &Fixture, name: &'static str) -> UserId {
    let id = UserId::new();
    f.store.write(move |tx| users::insert(tx, &users::NewUser { id: &id.to_string(), username: name, created_by_user: None, now_ms: 1 }, PHC).map(|_| ())).unwrap();
    id
}

/// Give a ticket `priority` and put its update time on `updated`.
fn prioritize(f: &Fixture, id: TicketId, priority: &'static str, updated: i64) {
    f.store
        .write(move |tx| Ok(tx.execute("UPDATE tickets SET priority = ?2, updated_at_ms = ?3 WHERE id = ?1", rusqlite::params![id.to_string(), priority, updated])?))
        .unwrap();
}

fn make_responsible(f: &Fixture, id: TicketId, user: Option<UserId>) -> bool {
    f.store.write(move |tx| tickets::set_responsible(tx, id, user)).unwrap()
}

fn disable(f: &Fixture, user: UserId) {
    f.store.write(move |tx| Ok(tx.execute("UPDATE users SET disabled_at_ms = 5 WHERE id = ?1", [user.to_string()])?)).unwrap();
}

fn on_column(f: &Fixture, id: TicketId, kind: ColumnKind) {
    f.store
        .write(move |tx| {
            let column = company::column_of_kind(tx, kind)?;
            tickets::move_to(tx, id, column.id, None)
        })
        .unwrap();
}

fn follow(f: &Fixture, ticket: TicketId, user: UserId) {
    f.store.write(move |tx| crate::followers::follow(tx, ticket, user, crate::followers::FollowReason::Manual, 1)).unwrap();
}

fn mine(f: &Fixture, viewer: UserId, project: Option<ProjectId>, include_done: bool) -> aurelius_domain::ticket::MyTicketsView {
    f.store.read(|c| tickets::my_tickets(c, viewer, project, include_done, &read_blob)).unwrap()
}

fn numbers(view: &aurelius_domain::ticket::MyTicketsView) -> Vec<u32> {
    let mut numbers: Vec<u32> = view.tickets.iter().map(|t| t.number).collect();
    numbers.sort();
    numbers
}

#[test]
fn every_ticket_read_shows_the_responsible_with_name_and_disabled() {
    let f = fixture();
    let (id, _) = add(&f, f.atlas, "Fix");
    let bea = second_user(&f, "bea");
    assert!(make_responsible(&f, id, Some(bea)));

    let check = |disabled: bool| {
        let through_view = f.store.read(|c| tickets::view(c, id, &read_blob)).unwrap().unwrap();
        let through_list = f.store.read(|c| tickets::list(c, &TicketFilter::default(), &read_blob)).unwrap().pop().unwrap();
        let through_board = f.store.read(|c| tickets::board(c, None, &read_blob)).unwrap().into_iter().flat_map(|c| c.tickets).next().unwrap();
        let through_mine = mine(&f, bea, None, false).tickets.pop().unwrap();
        for t in [through_view, through_list, through_board, through_mine] {
            assert_eq!((t.responsible_user_id, t.responsible_username.as_deref(), t.responsible_disabled), (Some(bea), Some("bea"), disabled));
        }
    };
    check(false);
    disable(&f, bea);
    check(true);
}

#[test]
fn a_ticket_without_a_responsible_reads_as_nobody() {
    let f = fixture();
    let (id, _) = add(&f, f.atlas, "Fix");
    let t = f.store.read(|c| tickets::view(c, id, &read_blob)).unwrap().unwrap();
    assert_eq!((t.responsible_user_id, t.responsible_username, t.responsible_disabled), (None, None, false));
    assert_eq!(f.store.read(|c| tickets::responsible(c, id)).unwrap(), None);
}

#[test]
fn set_responsible_says_whether_anything_changed_and_can_clear_it_and_leaves_the_version_alone() {
    let f = fixture();
    let (id, _) = add(&f, f.atlas, "Fix");
    let bea = second_user(&f, "bea");
    assert!(make_responsible(&f, id, Some(bea)));
    assert!(!make_responsible(&f, id, Some(bea)), "already");
    assert_eq!(f.store.read(|c| tickets::responsible(c, id)).unwrap(), Some(tickets::TicketResponsible { user_id: bea, disabled: false }));
    disable(&f, bea);
    assert_eq!(f.store.read(|c| tickets::responsible(c, id)).unwrap(), Some(tickets::TicketResponsible { user_id: bea, disabled: true }));
    assert!(make_responsible(&f, id, None));
    assert!(!make_responsible(&f, id, None), "nobody already");
    assert!(!make_responsible(&f, TicketId::new(), None), "an unknown ticket");
    assert_eq!(f.store.read(|c| tickets::view(c, id, &read_blob)).unwrap().unwrap().version, 1);
}

#[test]
fn a_responsible_who_does_not_exist_is_refused() {
    let f = fixture();
    let (id, _) = add(&f, f.atlas, "Fix");
    assert!(f.store.write(move |tx| tickets::set_responsible(tx, id, Some(UserId::new()))).is_err());
}

#[test]
fn my_tickets_are_what_i_am_responsible_for_made_follow_or_on_my_login_and_what_has_no_active_responsible() {
    let f = fixture();
    let (me, other) = (second_user(&f, "bea"), second_user(&f, "cyd"));
    // #1: nobody responsible, so open without an active responsible.
    add(&f, f.atlas, "No responsible");
    // #2: only Cyd's.
    let (not_mine, _) = add(&f, f.atlas, "Cyd's");
    make_responsible(&f, not_mine, Some(other));
    // #3: I am responsible.
    let (responsible, _) = add(&f, f.atlas, "Responsible");
    make_responsible(&f, responsible, Some(me));
    // #4: I follow it, Cyd is responsible.
    let (followed, _) = add(&f, f.atlas, "Followed");
    make_responsible(&f, followed, Some(other));
    follow(&f, followed, me);
    // #5: on my login, Cyd is responsible.
    let (login, _) = add(&f, f.atlas, "On my login");
    make_responsible(&f, login, Some(other));
    f.store.write(move |tx| tickets::use_my_login(tx, login, me)).unwrap();
    // #6: made by me, Cyd is responsible.
    let (creator, project) = (TicketId::new(), f.hermes);
    f.store
        .write(move |tx| {
            let backlog = company::column_of_kind(tx, ColumnKind::Backlog)?;
            let run = RunSettings::default();
            tickets::insert(tx, &NewTicket { id: creator, project_id: project, title: "Made by me", description_blob: None, column_id: backlog.id, priority: TicketPriority::default(), created_by_user: me, owner: None, run: &run, labels: &[], deadline_ms: None })
        })
        .unwrap();
    make_responsible(&f, creator, Some(other));
    // #7: its responsible is disabled, so no active responsible.
    let gone = second_user(&f, "gone");
    let (orphan, _) = add(&f, f.atlas, "Orphan");
    make_responsible(&f, orphan, Some(gone));
    disable(&f, gone);

    let view = mine(&f, me, None, false);
    assert_eq!(numbers(&view), [1, 3, 4, 5, 6, 7]);
    assert_eq!(view.following, [followed], "only what the reader follows");
    assert_eq!(numbers(&mine(&f, other, None, false)), [1, 2, 4, 5, 6, 7], "the reader decides what is mine");
}

#[test]
fn my_tickets_filter_on_the_project() {
    let f = fixture();
    let (a, _) = add(&f, f.atlas, "A");
    let (h, _) = add(&f, f.hermes, "H");
    let ids = |project| mine(&f, f.user, project, false).tickets.iter().map(|t| t.id).collect::<Vec<_>>();
    assert_eq!(ids(Some(f.atlas)), [a]);
    assert_eq!(ids(Some(f.hermes)), [h]);
    assert_eq!(ids(None).len(), 2);
}

#[test]
fn done_and_archived_come_only_with_include_done_and_then_not_for_lacking_a_responsible() {
    let f = fixture();
    let reader = second_user(&f, "bea");
    let (done_mine, _) = add(&f, f.atlas, "Done, mine");
    make_responsible(&f, done_mine, Some(reader));
    on_column(&f, done_mine, ColumnKind::Done);
    let (archived_mine, _) = add(&f, f.atlas, "Archived, mine");
    make_responsible(&f, archived_mine, Some(reader));
    f.store.write(move |tx| tickets::mark_archived(tx, archived_mine)).unwrap();
    let (done_nobody, _) = add(&f, f.atlas, "Done, nobody");
    on_column(&f, done_nobody, ColumnKind::Done);
    let (archived_nobody, _) = add(&f, f.atlas, "Archived, nobody");
    f.store.write(move |tx| tickets::mark_archived(tx, archived_nobody)).unwrap();
    add(&f, f.atlas, "Open, nobody");

    assert_eq!(numbers(&mine(&f, reader, None, false)), [5]);
    assert_eq!(numbers(&mine(&f, reader, None, true)), [1, 2, 5], "not another's closed ticket");
    assert_eq!(numbers(&mine(&f, f.user, None, true)), [1, 2, 3, 4, 5], "what they made");
}

#[test]
fn my_tickets_sort_on_priority_then_on_the_latest_update() {
    let f = fixture();
    let (low, _) = add(&f, f.atlas, "low");
    let (normal_old, _) = add(&f, f.atlas, "normal old");
    let (normal_new, _) = add(&f, f.atlas, "normal new");
    let (high, _) = add(&f, f.atlas, "high");
    let (urgent, _) = add(&f, f.atlas, "urgent");
    prioritize(&f, low, "low", 900);
    prioritize(&f, normal_old, "normal", 100);
    prioritize(&f, normal_new, "normal", 200);
    prioritize(&f, high, "high", 50);
    prioritize(&f, urgent, "urgent", 10);
    let order: Vec<String> = mine(&f, f.user, None, false).tickets.into_iter().map(|t| t.title).collect();
    assert_eq!(order, ["urgent", "high", "normal new", "normal old", "low"]);
}

#[test]
fn my_tickets_stop_at_the_maximum_and_keep_the_most_important() {
    use aurelius_domain::ticket::MY_TICKETS_MAX;
    let f = fixture();
    let (user, project) = (f.user, f.atlas);
    f.store
        .write(move |tx| {
            let backlog = company::column_of_kind(tx, ColumnKind::Backlog)?;
            let run = RunSettings::default();
            for n in 0..=MY_TICKETS_MAX {
                let priority = if n == MY_TICKETS_MAX { TicketPriority::Urgent } else { TicketPriority::Low };
                tickets::insert(tx, &NewTicket { id: TicketId::new(), project_id: project, title: "x", description_blob: None, column_id: backlog.id, priority, created_by_user: user, owner: None, run: &run, labels: &[], deadline_ms: None })?;
            }
            Ok(())
        })
        .unwrap();
    let view = mine(&f, user, None, false);
    assert_eq!(view.tickets.len(), MY_TICKETS_MAX);
    assert_eq!(view.tickets[0].priority, TicketPriority::Urgent);
}

#[test]
fn following_names_only_the_tickets_in_the_answer() {
    let f = fixture();
    let reader = second_user(&f, "bea");
    let (open, _) = add(&f, f.atlas, "Open");
    let (done, _) = add(&f, f.atlas, "Done");
    let (elsewhere, _) = add(&f, f.hermes, "Elsewhere");
    on_column(&f, done, ColumnKind::Done);
    for ticket in [open, done, elsewhere] {
        follow(&f, ticket, reader);
    }
    assert_eq!(mine(&f, reader, Some(f.atlas), false).following, [open], "not the one on Done, nor another project's");
}
