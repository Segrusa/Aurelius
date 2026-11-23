//! The Code tab's links from a branch to its ticket, owner and live runs (ticket #77).

use super::*;
use crate::tickets::{self, NewTicket};
use crate::{Store, company, employees, events, tasks, users, work_sessions};
use aurelius_domain::task::{RunSettings, TaskStatus};
use aurelius_domain::ticket::{ColumnKind, TicketKind, TicketPriority};
use aurelius_domain::{RunId, TaskId, UserId};

const PHC: &str = "$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHQ$aGFzaGhhc2hoYXNoaGFzaA";

struct Fixture {
    _dir: tempfile::TempDir,
    store: Store,
    user: UserId,
    atlas: ProjectId,
    hermes: ProjectId,
    ann: EmployeeId,
    bob: EmployeeId,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let (user, atlas, hermes, ann, bob) = (UserId::new(), ProjectId::new(), ProjectId::new(), EmployeeId::new(), EmployeeId::new());
    store
        .write(move |tx| {
            users::insert(tx, &users::NewUser { id: &user.to_string(), username: "grace", created_by_user: None, now_ms: 1 }, PHC)?;
            events::insert_project_projection(tx, atlas, "Atlas", None)?;
            tx.execute("UPDATE projects SET folder = '/repo/atlas' WHERE id = ?1", [atlas.to_string()])?;
            events::insert_project_projection(tx, hermes, "Hermes", None)?;
            employees::insert_employee(tx, ann, "Ann", "Dev", None, None, None, &serde_json::json!({}), ConversationId::new())?;
            employees::insert_employee(tx, bob, "Bob", "Dev", None, None, None, &serde_json::json!({}), ConversationId::new())?;
            tx.execute("INSERT INTO blobs(id, storage_key, sha256, byte_length, media_type, created_at_ms) VALUES ('g', 'g', '', 0, 'text/plain', 0)", [])?;
            Ok(())
        })
        .unwrap();
    Fixture { _dir: dir, store, user, atlas, hermes, ann, bob }
}

fn ticket(f: &Fixture, project: ProjectId, title: &str, owner: Option<EmployeeId>, kind: TicketKind) -> (TicketId, u32) {
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
                    owner,
                    run: &RunSettings::default(),
                    labels: &[],
                    deadline_ms: None,
                    kind,
                },
            )
        })
        .unwrap();
    (id, number)
}

/// A task of the ticket's tree with its own conversation.
fn task_of(f: &Fixture, ticket: TicketId, status: TaskStatus) -> ConversationId {
    let (task, conversation, project, ann) = (TaskId::new(), ConversationId::new(), f.atlas, f.ann);
    f.store
        .write(move |tx| {
            employees::insert_employee(tx, EmployeeId::new(), "Chat", "Dev", None, None, None, &serde_json::json!({}), conversation)?;
            tasks::insert_ticket_task(tx, task, project, Some(ann), "Work", "g", "g", status, None, Some(ticket))?;
            tasks::set_task_conversation(tx, task, conversation)
        })
        .unwrap();
    conversation
}

fn run_in(f: &Fixture, conversation: ConversationId, status: &'static str) {
    let run = RunId::new();
    f.store
        .write(move |tx| {
            Ok(tx.execute(
                "INSERT INTO runs(id, conversation_id, attempt, runtime_kind, model_id, resolved_config_json, status) VALUES (?1, ?2, 1, 'fake', 'm', '{}', ?3)",
                params![run.to_string(), conversation.to_string(), status],
            )?)
        })
        .unwrap();
}

fn session(f: &Fixture, employee: EmployeeId, worktree: &str, branch: &str) -> ConversationId {
    let (id, project, user, worktree, branch) = (ConversationId::new(), f.atlas, f.user, worktree.to_owned(), branch.to_owned());
    f.store.write(move |tx| work_sessions::insert(tx, id, project, employee, "Session", user, Some((&worktree, &branch)), None, false)).unwrap();
    id
}

fn link(f: &Fixture, branch: &str) -> BranchLink {
    let (project, branch) = (f.atlas, branch.to_owned());
    f.store.read(move |c| branch_link(c, project, &branch)).unwrap()
}

#[test]
fn a_branch_named_after_a_ticket_of_this_project_gets_it_and_its_owner() {
    let f = fixture();
    let (id, number) = ticket(&f, f.atlas, "Code tab", Some(f.ann), TicketKind::Normal);
    let found = link(&f, &format!("feat/ticket-{number}-code"));
    assert_eq!(found.ticket, Some(CodeTicket { ticket_id: id, number, title: "Code tab".into() }));
    assert_eq!(found.owner, Some(CodeOwner { employee_id: f.ann, name: "Ann".into() }));
    assert!(!found.working);
    assert_eq!(link(&f, &format!("fix/{number}-x")).ticket.map(|t| t.ticket_id), Some(id));
}

#[test]
fn a_ticket_number_of_another_project_or_no_number_links_nothing() {
    let f = fixture();
    let (_, number) = ticket(&f, f.hermes, "Elsewhere", Some(f.ann), TicketKind::Normal);
    assert_eq!(link(&f, &format!("feat/ticket-{number}-x")), BranchLink::default());
    assert_eq!(link(&f, "spike/nothing"), BranchLink::default());
    assert_eq!(link(&f, "main"), BranchLink::default());
}

#[test]
fn a_quick_tasks_branch_links_its_quick_ticket() {
    let f = fixture();
    let (id, number) = ticket(&f, f.atlas, "Quick fix", Some(f.bob), TicketKind::Quick);
    let found = link(&f, &format!("quick/ticket-{number}"));
    assert_eq!(found.ticket.map(|t| t.ticket_id), Some(id));
    assert_eq!(found.owner.map(|o| o.employee_id), Some(f.bob));
}

#[test]
fn a_work_sessions_branch_gets_its_employee_and_works_while_a_run_is_live_or_queued() {
    let f = fixture();
    let conversation = session(&f, f.bob, "/data/worktrees/s1", "session/1a2b3c4d");
    let found = link(&f, "session/1a2b3c4d");
    assert_eq!(found.ticket, None);
    assert_eq!(found.owner, Some(CodeOwner { employee_id: f.bob, name: "Bob".into() }));
    assert!(!found.working);
    run_in(&f, conversation, "created");
    assert!(link(&f, "session/1a2b3c4d").working, "a queued run");
    let project = f.atlas;
    let worktrees = f.store.read(move |c| session_worktrees(c, project)).unwrap();
    assert_eq!(worktrees, vec![SessionWorktree { path: "/data/worktrees/s1".into(), working: true }], "the same session found by its worktree");
}

#[test]
fn a_ticket_owner_goes_before_the_session_employee() {
    let f = fixture();
    let (_, number) = ticket(&f, f.atlas, "Owned", Some(f.ann), TicketKind::Normal);
    let branch = format!("feat/ticket-{number}-owned");
    session(&f, f.bob, "/data/worktrees/s2", &branch);
    assert_eq!(link(&f, &branch).owner.map(|o| o.employee_id), Some(f.ann));
}

#[test]
fn a_ticket_works_while_a_task_of_its_tree_runs_or_a_run_of_one_is_live() {
    let f = fixture();
    let (id, number) = ticket(&f, f.atlas, "Busy", None, TicketKind::Normal);
    let branch = format!("feat/ticket-{number}");
    let done = task_of(&f, id, TaskStatus::Succeeded);
    assert!(!link(&f, &branch).working, "only a finished task");
    run_in(&f, done, "succeeded");
    assert!(!link(&f, &branch).working, "an ended run");
    run_in(&f, done, "running");
    assert!(link(&f, &branch).working, "a follow-up run of the finished task");

    let (other, number) = ticket(&f, f.atlas, "Queued", None, TicketKind::Normal);
    task_of(&f, other, TaskStatus::Queued);
    assert!(link(&f, &format!("fix/{number}")).working, "a queued task");
}

#[test]
fn a_project_folder_tells_a_missing_project_from_one_without_a_folder() {
    let f = fixture();
    let (atlas, hermes) = (f.atlas, f.hermes);
    assert_eq!(f.store.read(move |c| project_folder(c, atlas)).unwrap(), Some(Some("/repo/atlas".into())));
    assert_eq!(f.store.read(move |c| project_folder(c, hermes)).unwrap(), Some(None));
    assert_eq!(f.store.read(|c| project_folder(c, ProjectId::new())).unwrap(), None);
}

#[test]
fn a_branch_stays_open_until_its_ticket_is_done_or_its_session_is_deleted() {
    let f = fixture();
    let (id, number) = ticket(&f, f.atlas, "Fresh", Some(f.ann), TicketKind::Normal);
    let branch = format!("feat/ticket-{number}-fresh");
    assert!(link(&f, &branch).open, "a Backlog ticket may still get work");
    let ticket_id = id.to_string();
    f.store
        .write(move |tx| {
            let done = company::column_of_kind(tx, ColumnKind::Done)?;
            Ok(tx.execute("UPDATE tickets SET column_id = ?1 WHERE id = ?2", params![done.id.to_string(), ticket_id])?)
        })
        .unwrap();
    assert!(!link(&f, &branch).open, "a Done ticket's branch may be cleaned up");

    let conversation = session(&f, f.bob, "/data/worktrees/s3", "session/0a0b0c0d");
    assert!(link(&f, "session/0a0b0c0d").open, "a session that is not deleted");
    f.store.write(move |tx| Ok(tx.execute("UPDATE conversations SET archived_at_ms = 5 WHERE id = ?1", [conversation.to_string()])?)).unwrap();
    assert!(!link(&f, "session/0a0b0c0d").open, "a deleted session");
}
