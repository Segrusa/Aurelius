//! `user.set_rail_pins`: each board member's own rail, the no-op, the refusals and
//! the event that tells that board member's other devices.

use super::*;
use aurelius_domain::project::CreateProject;
use aurelius_domain::user::SetRailPins;
use aurelius_domain::{ErrorCode, UserId};

const PHC: &str = "$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHQ$aGFzaGhhc2hoYXNoaGFzaA";

fn ctx(user: Option<UserId>) -> CommandContext {
    CommandContext { principal_id: PrincipalId::new(), client_id: ClientId::new(), origin: Origin::LocalIpc, request_id: 1, user_id: user }
}

fn board_member(service: &CommandService, name: &'static str) -> UserId {
    let id = UserId::new();
    service
        .store()
        .write(move |tx| aurelius_storage::users::insert(tx, &aurelius_storage::users::NewUser { id: &id.to_string(), username: name, created_by_user: None, now_ms: 1 }, PHC))
        .unwrap();
    id
}

fn run(service: &CommandService, user: Option<UserId>, command: Command) -> Result<Committed, AppError> {
    service.execute(&ctx(user), CommandEnvelope { command_id: CommandId::new(), expected_version: None, command })
}

fn project(service: &CommandService, user: UserId, name: &str) -> ProjectId {
    let made = run(service, Some(user), Command::CreateProject(CreateProject { name: name.into(), description: None, folder: None })).unwrap();
    match made.receipt.result {
        CommandResult::ProjectCreated { project_id, .. } => project_id,
        other => panic!("{other:?}"),
    }
}

fn pin(service: &CommandService, user: Option<UserId>, project_ids: Vec<ProjectId>) -> Result<Committed, AppError> {
    run(service, user, Command::SetRailPins(SetRailPins { project_ids }))
}

fn refused(result: Result<Committed, AppError>) -> ErrorCode {
    match result {
        Ok(_) => panic!("the rail was accepted"),
        Err(err) => err.code,
    }
}

fn rail(service: &CommandService, user: UserId) -> aurelius_domain::user::RailPinsView {
    service.store().read(move |c| aurelius_storage::rail_pins::get(c, user)).unwrap()
}

fn changed_events(service: &CommandService) -> Vec<serde_json::Value> {
    service
        .store()
        .read(|c| {
            let mut stmt = c.prepare("SELECT metadata_json FROM events WHERE type = 'user.rail_pins_changed' ORDER BY seq")?;
            let rows = stmt.query_map([], |r| r.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
            Ok(rows.into_iter().map(|m| serde_json::from_str(&m).unwrap()).collect())
        })
        .unwrap()
}

fn setup() -> (tempfile::TempDir, CommandService, UserId, UserId, ProjectId, ProjectId) {
    let dir = tempfile::tempdir().unwrap();
    let service = CommandService::new(Arc::new(Store::open(dir.path()).unwrap()));
    let alex = board_member(&service, "alex");
    let sam = board_member(&service, "sam");
    let alpha = project(&service, alex, "Alpha");
    let beta = project(&service, alex, "Beta");
    (dir, service, alex, sam, alpha, beta)
}

#[test]
fn each_board_member_keeps_their_own_rail() {
    let (_dir, service, alex, sam, alpha, beta) = setup();
    pin(&service, Some(alex), vec![beta, alpha]).unwrap();
    pin(&service, Some(sam), vec![alpha]).unwrap();
    assert_eq!(rail(&service, alex).project_ids, [beta, alpha]);
    assert_eq!(rail(&service, sam).project_ids, [alpha]);
}

#[test]
fn a_change_answers_the_rail_and_writes_one_event_for_that_board_member() {
    let (_dir, service, alex, _sam, alpha, beta) = setup();
    let committed = pin(&service, Some(alex), vec![alpha, beta, alpha]).unwrap();
    assert_eq!(committed.receipt.result, CommandResult::RailPinsSet { project_ids: vec![alpha, beta], changed: true });
    assert_eq!(committed.notices.len(), 1);
    assert_eq!(committed.notices[0].kind, EventKind::UserRailPinsChanged);
    let events = changed_events(&service);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["user_id"], alex.to_string());
    assert_eq!(events[0]["project_ids"], serde_json::json!([alpha, beta]));
}

#[test]
fn the_same_rail_again_writes_no_event() {
    let (_dir, service, alex, _sam, alpha, _beta) = setup();
    pin(&service, Some(alex), vec![alpha]).unwrap();
    let again = pin(&service, Some(alex), vec![alpha]).unwrap();
    assert_eq!(again.receipt.result, CommandResult::RailPinsSet { project_ids: vec![alpha], changed: false });
    assert!(again.notices.is_empty());
    assert_eq!(changed_events(&service).len(), 1);
}

#[test]
fn an_empty_rail_is_a_choice() {
    let (_dir, service, alex, _sam, _alpha, _beta) = setup();
    let committed = pin(&service, Some(alex), vec![]).unwrap();
    assert_eq!(committed.receipt.result, CommandResult::RailPinsSet { project_ids: vec![], changed: true });
    let stored = rail(&service, alex);
    assert!(stored.chosen && stored.project_ids.is_empty());
}

#[test]
fn a_rail_needs_a_board_member_and_real_workspaces() {
    let (_dir, service, alex, _sam, alpha, _beta) = setup();
    assert_eq!(refused(pin(&service, None, vec![alpha])), ErrorCode::BoardMemberRequired);
    assert_eq!(refused(pin(&service, Some(alex), vec![alpha, ProjectId::new()])), ErrorCode::NotFound);
    assert!(!rail(&service, alex).chosen, "a refused rail stores nothing");
}
