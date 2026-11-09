//! `server.set_name` (SN-01): the stored choice, the default, the refusals, the
//! no-op and the event with who, the old and the new name.

use super::*;
use aurelius_domain::server::{SERVER_NAME_MAX, SetServerName};
use aurelius_domain::{ErrorCode, UserId};

const PHC: &str = "$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHQ$aGFzaGhhc2hoYXNoaGFzaA";

fn ctx(user: Option<UserId>) -> CommandContext {
    CommandContext { principal_id: PrincipalId::new(), client_id: ClientId::new(), origin: Origin::LocalIpc, request_id: 1, user_id: user }
}

fn setup() -> (tempfile::TempDir, CommandService, UserId) {
    let dir = tempfile::tempdir().unwrap();
    let service = CommandService::new(Arc::new(Store::open(dir.path()).unwrap()));
    let alex = UserId::new();
    service
        .store()
        .write(move |tx| aurelius_storage::users::insert(tx, &aurelius_storage::users::NewUser { id: &alex.to_string(), username: "alex", created_by_user: None, now_ms: 1 }, PHC))
        .unwrap();
    // A real log is never empty: making the first board member writes an event.
    let follow = Command::SetFollowOrgChart(aurelius_domain::employee::SetFollowOrgChart { follow: false });
    service.execute(&ctx(Some(alex)), CommandEnvelope { command_id: CommandId::new(), expected_version: None, command: follow }).unwrap();
    (dir, service, alex)
}

fn rename(service: &CommandService, user: Option<UserId>, name: &str) -> Result<Committed, AppError> {
    service.execute(&ctx(user), CommandEnvelope { command_id: CommandId::new(), expected_version: None, command: Command::SetServerName(SetServerName { name: name.into() }) })
}

fn refused(service: &CommandService, user: Option<UserId>, name: &str) -> AppError {
    match rename(service, user, name) {
        Ok(_) => panic!("{name:?} was accepted"),
        Err(err) => err,
    }
}

fn stored(service: &CommandService) -> aurelius_storage::server_name::StoredServerName {
    service.store().read(aurelius_storage::server_name::get).unwrap()
}

/// The `server.renamed` events' metadata, oldest first.
fn renamed(service: &CommandService) -> Vec<serde_json::Value> {
    service
        .store()
        .read(|c| {
            let mut stmt = c.prepare("SELECT metadata_json FROM events WHERE type = 'server.renamed' ORDER BY seq")?;
            let rows = stmt.query_map([], |r| r.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
            Ok(rows.into_iter().map(|m| serde_json::from_str(&m).unwrap()).collect())
        })
        .unwrap()
}

#[test]
fn a_valid_name_is_stored_trimmed_and_answered() {
    let (_dir, service, alex) = setup();
    let committed = rename(&service, Some(alex), "  Kantoor 🏠 ").unwrap();
    assert_eq!(committed.receipt.result, CommandResult::ServerNameSet { name: "Kantoor 🏠".into(), version: 1 });
    assert_eq!(committed.notices.len(), 1);
    assert_eq!((committed.notices[0].kind, committed.notices[0].aggregate_id.as_str()), (EventKind::ServerRenamed, "server"));
    assert_eq!(stored(&service).chosen.as_deref(), Some("Kantoor 🏠"));
    assert_eq!(aurelius_host::server_name(service.store()).name, "Kantoor 🏠");
}

#[test]
fn an_empty_name_puts_the_computers_name_back() {
    let (_dir, service, alex) = setup();
    rename(&service, Some(alex), "Kantoor").unwrap();
    let committed = rename(&service, Some(alex), "   ").unwrap();
    assert_eq!(committed.receipt.result, CommandResult::ServerNameSet { name: aurelius_host::host_name(), version: 2 });
    assert_eq!(stored(&service).chosen, None);
    let view = aurelius_host::server_name(service.store());
    assert_eq!((view.name, view.chosen), (aurelius_host::host_name(), None));
}

#[test]
fn a_refused_name_keeps_the_old_one_and_says_why() {
    let (_dir, service, alex) = setup();
    rename(&service, Some(alex), "Kantoor").unwrap();
    let long = refused(&service, Some(alex), &"x".repeat(SERVER_NAME_MAX + 1));
    assert_eq!(long.code, ErrorCode::Validation);
    assert!(long.message.contains("40"), "{}", long.message);
    for name in ["Kan\ntoor", "Kan\u{7}toor"] {
        let err = refused(&service, Some(alex), name);
        assert_eq!(err.code, ErrorCode::Validation);
        assert!(err.message.contains("line breaks"), "{}", err.message);
    }
    assert_eq!(stored(&service), aurelius_storage::server_name::StoredServerName { chosen: Some("Kantoor".into()), version: 1 });
    assert_eq!(renamed(&service).len(), 1);
}

#[test]
fn without_a_board_member_the_name_is_refused() {
    let (_dir, service, _alex) = setup();
    let err = refused(&service, None, "Kantoor");
    assert_eq!(err.code, ErrorCode::BoardMemberRequired);
    assert_eq!(stored(&service).version, 0);
    assert!(renamed(&service).is_empty());
}

#[test]
fn the_same_name_again_writes_no_event_and_no_version() {
    let (_dir, service, alex) = setup();
    rename(&service, Some(alex), "Kantoor").unwrap();
    let again = rename(&service, Some(alex), " Kantoor ").unwrap();
    assert_eq!(again.receipt.result, CommandResult::ServerNameSet { name: "Kantoor".into(), version: 1 });
    assert!(again.notices.is_empty());
    // The default again while it is the default: no-op as well.
    let (_dir2, fresh, robin) = setup();
    let empty = rename(&fresh, Some(robin), "").unwrap();
    assert!(empty.notices.is_empty());
    assert_eq!(stored(&fresh).version, 0);
    assert_eq!(renamed(&service).len(), 1);
}

#[test]
fn every_change_is_in_the_audit_with_who_the_old_and_the_new_name() {
    let (_dir, service, alex) = setup();
    rename(&service, Some(alex), "Kantoor").unwrap();
    rename(&service, Some(alex), "Thuis-PC").unwrap();
    rename(&service, Some(alex), "").unwrap();
    let events = renamed(&service);
    let host = aurelius_host::host_name();
    let seen: Vec<(String, String, String)> = events.iter().map(|m| (m["by"].as_str().unwrap().to_owned(), m["before"].as_str().unwrap().to_owned(), m["after"].as_str().unwrap().to_owned())).collect();
    let me = alex.to_string();
    assert_eq!(seen, [(me.clone(), host.clone(), "Kantoor".into()), (me.clone(), "Kantoor".into(), "Thuis-PC".into()), (me, "Thuis-PC".into(), host)]);
    assert_eq!(events[2]["chosen"], serde_json::Value::Null);
    // The timeline names the board member, the new and the old name (the card has the time).
    let (cards, _) = service.store().read(|c| aurelius_storage::timeline::cards(c, None, None, None, 10, false)).unwrap();
    let card = cards.iter().find(|c| c.title.contains("Thuis-PC") && c.title.starts_with("alex renamed")).expect("a timeline card");
    assert_eq!(card.detail, "It was “Kantoor”");
    assert!(card.at_ms > 0);
}

#[test]
fn the_last_of_two_writes_wins_and_both_are_in_the_audit() {
    let (_dir, service, alex) = setup();
    let robin = UserId::new();
    service
        .store()
        .write(move |tx| aurelius_storage::users::insert(tx, &aurelius_storage::users::NewUser { id: &robin.to_string(), username: "robin", created_by_user: None, now_ms: 1 }, PHC))
        .unwrap();
    rename(&service, Some(alex), "Kantoor").unwrap();
    rename(&service, Some(robin), "Thuis").unwrap();
    assert_eq!(stored(&service).chosen.as_deref(), Some("Thuis"));
    let by: Vec<String> = renamed(&service).iter().map(|m| m["by"].as_str().unwrap().to_owned()).collect();
    assert_eq!(by, [alex.to_string(), robin.to_string()]);
}

#[test]
fn the_same_command_twice_is_replayed_from_its_receipt() {
    let (_dir, service, alex) = setup();
    let envelope = CommandEnvelope { command_id: CommandId::new(), expected_version: None, command: Command::SetServerName(SetServerName { name: "Kantoor".into() }) };
    let first = service.execute(&ctx(Some(alex)), envelope.clone()).unwrap();
    let second = service.execute(&ctx(Some(alex)), envelope).unwrap();
    assert!(second.receipt.replayed);
    assert_eq!(second.receipt.result, first.receipt.result);
    assert_eq!(renamed(&service).len(), 1);
}
