//! Phase 3 (migration 21): sessions, disabled users and passwords, Claude
//! logins, and whose login runs, tickets and parked work use.

use crate::claude_logins::{self, LoginStatus};
use crate::tickets::{self, NewTicket};
use crate::user_sessions::{self, IDLE_MS, LIFETIME_MS, NewSession};
use crate::{Store, company, conversations, events, queries, remote, users};
use aurelius_domain::remote::Capability;
use aurelius_domain::task::RunSettings;
use aurelius_domain::ticket::{ColumnKind, TicketPriority};
use aurelius_domain::{ClientId, PrincipalId, ProjectId, RunId, TicketId, UserId};
use rusqlite::params;

const PHC: &str = "$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHQ$aGFzaGhhc2hoYXNoaGFzaA";
const NEW_PHC: &str = "$argon2id$v=19$m=19456,t=2,p=1$bmV3c2FsdA$bmV3aGFzaG5ld2hhc2g";
const T0: i64 = 1_000_000_000;

struct Fixture {
    _dir: tempfile::TempDir,
    store: Store,
    alex: String,
    robin: String,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let (alex, robin) = (UserId::new().to_string(), UserId::new().to_string());
    let (a, r) = (alex.clone(), robin.clone());
    store
        .write(move |tx| {
            users::insert(tx, &users::NewUser { id: &a, username: "Alex", created_by_user: None, now_ms: 1 }, PHC)?;
            users::insert(tx, &users::NewUser { id: &r, username: "Robin", created_by_user: Some(&a), now_ms: 2 }, PHC)
        })
        .unwrap();
    Fixture { _dir: dir, store, alex, robin }
}

fn login(f: &Fixture, id: &str, user: &str, client: Option<ClientId>, verifier: &str, at: i64) {
    let (id, user, verifier) = (id.to_owned(), user.to_owned(), verifier.to_owned());
    f.store
        .write(move |tx| {
            let row = user_sessions::insert(tx, &NewSession { id: &id, user_id: &user, client_id: client, device_name: "Laptop", now_ms: at }, &verifier)?;
            assert_eq!((row.created_at_ms, row.last_seen_ms, row.expires_at_ms), (at, at, at + LIFETIME_MS));
            Ok(())
        })
        .unwrap();
}

fn found(f: &Fixture, verifier: &str, at: i64) -> Option<String> {
    let verifier = verifier.to_owned();
    f.store.read(move |c| user_sessions::by_verifier(c, &verifier, at)).unwrap().map(|s| s.id)
}

fn device(f: &Fixture) -> ClientId {
    let id = ClientId::new();
    f.store.write(move |tx| remote::create_client(tx, id, PrincipalId::new(), "Phone", "ios", "v", &Capability::DEVICE_DEFAULT)).unwrap();
    id
}

#[test]
fn a_session_is_found_by_its_verifier_until_it_expires() {
    let f = fixture();
    login(&f, "s1", &f.alex, None, "v1", T0);
    assert_eq!(found(&f, "v1", T0 + 1).as_deref(), Some("s1"));
    assert_eq!(found(&f, "other", T0 + 1), None);
    let s = f.store.read(|c| user_sessions::get(c, "s1")).unwrap().unwrap();
    assert_eq!((s.user_id.as_str(), s.client_id, s.device_name.as_str()), (f.alex.as_str(), None, "Laptop"), "no device: the local IPC");

    // 30 days without use.
    assert_eq!(found(&f, "v1", T0 + IDLE_MS), None, "idle");
    // Used now and then, it still ends 90 days after the login.
    let mut at = T0;
    while at + IDLE_MS / 2 < T0 + LIFETIME_MS {
        at += IDLE_MS / 2;
        assert_eq!(found(&f, "v1", at).as_deref(), Some("s1"), "used at {at}");
        f.store.write(move |tx| user_sessions::touch(tx, "s1", at)).unwrap();
    }
    assert_eq!(found(&f, "v1", T0 + LIFETIME_MS), None, "absolute");
    // A late touch never moves it back.
    f.store.write(|tx| user_sessions::touch(tx, "s1", T0)).unwrap();
    assert_eq!(f.store.read(|c| user_sessions::get(c, "s1")).unwrap().unwrap().last_seen_ms, at);
    // The verifier is unique.
    let alex = f.alex.clone();
    assert!(f.store.write(move |tx| user_sessions::insert(tx, &NewSession { id: "s2", user_id: &alex, client_id: None, device_name: "x", now_ms: T0 }, "v1")).is_err());
}

/// R1 M-2: what an open connection checks its session against now and then.
#[test]
fn a_session_is_valid_by_id_until_it_is_revoked_expired_or_its_board_member_disabled() {
    let f = fixture();
    login(&f, "s1", &f.alex, None, "v1", T0);
    login(&f, "s2", &f.robin, None, "v2", T0);
    let valid = |id: &'static str, at: i64| f.store.read(move |c| user_sessions::valid(c, id, at)).unwrap().map(|s| s.id);
    assert_eq!(valid("s1", T0 + 1).as_deref(), Some("s1"));
    assert_eq!(valid("nope", T0 + 1), None);
    assert_eq!(valid("s1", T0 + IDLE_MS), None, "idle");
    let alex = f.alex.clone();
    f.store.write(move |tx| user_sessions::revoke(tx, &alex, "s1", T0 + 2)).unwrap();
    assert_eq!(valid("s1", T0 + 3), None, "revoked");
    let robin = f.robin.clone();
    f.store.write(move |tx| crate::users::disable(tx, &robin, T0 + 2)).unwrap();
    assert_eq!(valid("s2", T0 + 3), None, "disabled");
}

#[test]
fn sessions_are_listed_and_revoked_one_all_or_all_but_one() {
    let f = fixture();
    login(&f, "a1", &f.alex, None, "va1", T0);
    login(&f, "a2", &f.alex, None, "va2", T0 + 10);
    login(&f, "a3", &f.alex, None, "va3", T0 + 20);
    login(&f, "r1", &f.robin, None, "vr1", T0);
    let listed = |user: &str| -> Vec<String> {
        let user = user.to_owned();
        f.store.read(move |c| user_sessions::list_for_user(c, &user, T0 + 30)).unwrap().into_iter().map(|s| s.id).collect()
    };
    assert_eq!(listed(&f.alex), ["a3", "a2", "a1"], "most recently used first");

    // Only one's own session, and only once.
    let (alex, robin) = (f.alex.clone(), f.robin.clone());
    assert!(!f.store.write(move |tx| user_sessions::revoke(tx, &robin, "a1", T0 + 30)).unwrap(), "not Robin's");
    let a = alex.clone();
    assert!(f.store.write(move |tx| user_sessions::revoke(tx, &a, "a1", T0 + 30)).unwrap());
    let a = alex.clone();
    assert!(!f.store.write(move |tx| user_sessions::revoke(tx, &a, "a1", T0 + 31)).unwrap(), "already revoked");
    assert_eq!(found(&f, "va1", T0 + 40), None);
    assert_eq!(f.store.read(|c| user_sessions::get(c, "a1")).unwrap().unwrap().revoked_at_ms, Some(T0 + 30));
    assert_eq!(listed(&f.alex), ["a3", "a2"]);

    let a = alex.clone();
    assert_eq!(f.store.write(move |tx| user_sessions::revoke_others(tx, &a, "a3", T0 + 40)).unwrap(), 1);
    assert_eq!(listed(&f.alex), ["a3"]);
    let a = alex.clone();
    assert_eq!(f.store.write(move |tx| user_sessions::revoke_for_user(tx, &a, T0 + 50)).unwrap(), 1);
    assert!(listed(&f.alex).is_empty());
    assert_eq!(listed(&f.robin), ["r1"], "Robin's session stays");
}

#[test]
fn revoking_a_device_revokes_its_sessions_and_the_device_knows_its_board_member() {
    let f = fixture();
    let (phone, tablet) = (device(&f), device(&f));
    login(&f, "p1", &f.alex, Some(phone), "vp1", T0);
    login(&f, "p2", &f.robin, Some(phone), "vp2", T0 + 1);
    login(&f, "t1", &f.alex, Some(tablet), "vt1", T0);
    login(&f, "l1", &f.alex, None, "vl1", T0);
    let client = f.store.read(move |c| remote::find_client(c, phone)).unwrap().unwrap();
    assert_eq!(client.user_id.as_deref(), Some(f.robin.as_str()), "the last who logged in on it");

    assert_eq!(f.store.write(move |tx| user_sessions::revoke_for_client(tx, tablet, T0 + 5)).unwrap(), 1);
    assert_eq!(found(&f, "vt1", T0 + 6), None);
    // Revoking the device itself ends both board members' sessions on it.
    assert!(f.store.write(move |tx| remote::revoke_client(tx, phone)).unwrap());
    assert_eq!((found(&f, "vp1", T0 + 6), found(&f, "vp2", T0 + 6)), (None, None));
    assert_eq!(found(&f, "vl1", T0 + 6).as_deref(), Some("l1"), "the local session stays");
    // A device revoked outside of `revoke_client` does not let its sessions through either.
    let laptop = device(&f);
    login(&f, "d1", &f.alex, Some(laptop), "vd1", T0);
    f.store.write(move |tx| Ok(tx.execute("UPDATE remote_clients SET revoked_at_ms = 1 WHERE id = ?1", [laptop.to_string()]).map(|_| ())?)).unwrap();
    assert_eq!(found(&f, "vd1", T0 + 6), None);
}

#[test]
fn a_disabled_board_member_has_no_sessions_and_is_not_counted() {
    let f = fixture();
    login(&f, "r1", &f.robin, None, "vr1", T0);
    login(&f, "a1", &f.alex, None, "va1", T0);
    let robin = f.robin.clone();
    f.store.write(move |tx| company::set_background_login_user(tx, Some(UserId::parse(&robin).unwrap())).map(|_| ())).unwrap();
    assert_eq!(f.store.read(users::count_active).unwrap(), 2);

    let robin = f.robin.clone();
    assert!(f.store.write(move |tx| users::disable(tx, &robin, T0 + 1)).unwrap());
    let robin = f.robin.clone();
    assert!(!f.store.write(move |tx| users::disable(tx, &robin, T0 + 2)).unwrap(), "already disabled");
    let row = f.store.read(|c| users::get(c, &f.robin)).unwrap().unwrap();
    assert_eq!((row.disabled_at_ms, row.disabled(), row.version), (Some(T0 + 1), true, 2));
    assert_eq!((f.store.read(users::count).unwrap(), f.store.read(users::count_active).unwrap()), (2, 1), "never deleted");
    assert_eq!(found(&f, "vr1", T0 + 3), None);
    assert_eq!(f.store.read(|c| user_sessions::get(c, "r1")).unwrap().unwrap().revoked_at_ms, Some(T0 + 1));
    assert_eq!(found(&f, "va1", T0 + 3).as_deref(), Some("a1"));
    assert_eq!(f.store.read(company::background_login_user).unwrap(), None, "no longer the company login");
    assert!(!f.store.write(|tx| users::disable(tx, "nobody", T0)).unwrap());
}

#[test]
fn a_new_password_revokes_the_other_sessions_and_a_reset_one_must_be_changed() {
    let f = fixture();
    login(&f, "a1", &f.alex, None, "va1", T0);
    login(&f, "a2", &f.alex, None, "va2", T0);
    let before = f.store.read(|c| users::get(c, &f.alex)).unwrap().unwrap();
    assert_eq!((before.password_changed_at_ms, before.must_change_password), (None, false));

    // Alex changes their own password in session a1: a2 ends, a1 goes on.
    let alex = f.alex.clone();
    assert!(f.store.write(move |tx| users::set_password(tx, &alex, NEW_PHC, false, Some("a1"), T0 + 5)).unwrap());
    let row = f.store.read(|c| users::get(c, &f.alex)).unwrap().unwrap();
    assert_eq!((row.password_changed_at_ms, row.must_change_password, row.version), (Some(T0 + 5), false, before.version + 1));
    assert_eq!(f.store.read(|c| users::password_hash(c, &f.alex)).unwrap().as_deref(), Some(NEW_PHC));
    assert_eq!((found(&f, "va1", T0 + 6).as_deref(), found(&f, "va2", T0 + 6)), (Some("a1"), None));

    // Robin resets it to a temporary one: every session ends, it must be changed.
    let alex = f.alex.clone();
    assert!(f.store.write(move |tx| users::set_password(tx, &alex, PHC, true, None, T0 + 7)).unwrap());
    let row = f.store.read(|c| users::get(c, &f.alex)).unwrap().unwrap();
    assert_eq!((row.password_changed_at_ms, row.must_change_password), (Some(T0 + 7), true));
    assert_eq!(found(&f, "va1", T0 + 8), None);
    assert!(!f.store.write(|tx| users::set_password(tx, "nobody", PHC, false, None, T0)).unwrap());
    let flag = f.store.write(|tx| Ok(tx.execute("UPDATE users SET must_change_password = 2", [])?));
    assert!(flag.is_err(), "must_change_password is 0 or 1");
}

#[test]
fn a_claude_login_keeps_only_its_status_email_and_plan() {
    let f = fixture();
    let alex = f.alex.clone();
    assert!(f.store.read(move |c| claude_logins::get(c, &alex)).unwrap().is_none());
    assert!(!f.store.read(|c| claude_logins::is_linked(c, &f.alex)).unwrap());
    let alex = f.alex.clone();
    f.store.write(move |tx| claude_logins::upsert(tx, &alex, LoginStatus::Linked, Some("test@example.com"), Some("max"), 10)).unwrap();
    let get = |at: &str| f.store.read(|c| claude_logins::get(c, &f.alex)).unwrap().unwrap_or_else(|| panic!("{at}"));
    let row = get("linked");
    assert_eq!(
        (row.status, row.email.as_deref(), row.plan.as_deref(), row.linked_at_ms, row.checked_at_ms),
        (LoginStatus::Linked, Some("test@example.com"), Some("max"), Some(10), 10)
    );
    assert!(f.store.read(|c| claude_logins::is_linked(c, &f.alex)).unwrap());

    // Checked again: still linked since 10.
    let alex = f.alex.clone();
    f.store.write(move |tx| claude_logins::upsert(tx, &alex, LoginStatus::Linked, Some("test@example.com"), Some("pro"), 20)).unwrap();
    let row = get("checked");
    assert_eq!((row.plan.as_deref(), row.linked_at_ms, row.checked_at_ms), (Some("pro"), Some(10), 20));
    // Expired, then linked again: a new linked_at.
    let alex = f.alex.clone();
    f.store.write(move |tx| claude_logins::upsert(tx, &alex, LoginStatus::Expired, None, None, 30)).unwrap();
    let row = get("expired");
    assert_eq!((row.status, row.email, row.linked_at_ms), (LoginStatus::Expired, None, Some(10)));
    assert!(!f.store.read(|c| claude_logins::is_linked(c, &f.alex)).unwrap());
    let alex = f.alex.clone();
    f.store.write(move |tx| claude_logins::upsert(tx, &alex, LoginStatus::Linked, Some("test@example.com"), None, 40)).unwrap();
    assert_eq!(get("relinked").linked_at_ms, Some(40));

    // A disabled board member's login does not count.
    let robin = f.robin.clone();
    f.store.write(move |tx| claude_logins::upsert(tx, &robin, LoginStatus::Linked, None, None, 1)).unwrap();
    let robin = f.robin.clone();
    f.store.write(move |tx| users::disable(tx, &robin, 2).map(|_| ())).unwrap();
    assert!(!f.store.read(|c| claude_logins::is_linked(c, &f.robin)).unwrap());

    let alex = f.alex.clone();
    assert!(f.store.write(move |tx| claude_logins::delete(tx, &alex)).unwrap());
    let alex = f.alex.clone();
    assert!(!f.store.write(move |tx| claude_logins::delete(tx, &alex)).unwrap());
    assert!(f.store.read(|c| claude_logins::get(c, &f.alex)).unwrap().is_none());
    let columns: Vec<String> = f
        .store
        .read(|c| Ok(c.prepare("SELECT name FROM pragma_table_info('claude_logins')")?.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?))
        .unwrap();
    assert_eq!(columns, ["user_id", "status", "email", "plan", "linked_at_ms", "checked_at_ms"], "no token, no path");
    let bad = f.store.write(|tx| Ok(tx.execute("UPDATE claude_logins SET status = 'unknown'", [])?));
    assert!(bad.is_err());
}

fn ticket(f: &Fixture, project: ProjectId, creator: &str) -> TicketId {
    let (id, creator) = (TicketId::new(), UserId::parse(creator).unwrap());
    f.store
        .write(move |tx| {
            let backlog = company::column_of_kind(tx, ColumnKind::Backlog)?;
            tickets::insert(
                tx,
                &NewTicket {
                    id,
                    project_id: project,
                    title: "Fix it",
                    description_blob: None,
                    column_id: backlog.id,
                    priority: TicketPriority::default(),
                    created_by_user: creator,
                    owner: None,
                    run: &RunSettings::default(),
                    labels: &[],
                    deadline_ms: None,
                    kind: Default::default(),
                },
            )
            .map(|_| ())
        })
        .unwrap();
    id
}

fn project(f: &Fixture) -> ProjectId {
    let id = ProjectId::new();
    f.store.write(move |tx| events::insert_project_projection(tx, id, "Alpha", None)).unwrap();
    id
}

#[test]
fn a_ticket_runs_on_its_creators_login_until_someone_takes_it_over() {
    let f = fixture();
    let p = project(&f);
    let t = ticket(&f, p, &f.alex);
    let (alex, robin) = (UserId::parse(&f.alex).unwrap(), UserId::parse(&f.robin).unwrap());
    assert_eq!(f.store.read(move |c| tickets::login_user(c, t)).unwrap(), Some(alex));
    assert!(!f.store.write(move |tx| tickets::use_my_login(tx, t, alex)).unwrap(), "already on Alex's login");
    assert!(f.store.write(move |tx| tickets::use_my_login(tx, t, robin)).unwrap());
    assert_eq!(f.store.read(move |c| tickets::login_user(c, t)).unwrap(), Some(robin));
    let creator: String = f.store.read(move |c| Ok(c.query_row("SELECT created_by_user FROM tickets WHERE id = ?1", [t.to_string()], |r| r.get(0))?)).unwrap();
    assert_eq!(creator, f.alex, "the creator stays");
    assert!(!f.store.write(move |tx| tickets::use_my_login(tx, TicketId::new(), robin)).unwrap());
    assert_eq!(f.store.read(|c| tickets::login_user(c, TicketId::new())).unwrap(), None);
    let nobody = UserId::new();
    assert!(f.store.write(move |tx| tickets::use_my_login(tx, t, nobody)).is_err(), "login_user references users");
}

#[test]
fn the_ticket_view_and_list_show_whose_login_it_runs_on() {
    let f = fixture();
    let p = project(&f);
    let t = ticket(&f, p, &f.alex);
    let (alex, robin) = (UserId::parse(&f.alex).unwrap(), UserId::parse(&f.robin).unwrap());
    let shown = |f: &Fixture| {
        let view = f.store.read(move |c| tickets::view(c, t, &|_| None)).unwrap().unwrap();
        let listed = f.store.read(move |c| tickets::list(c, &tickets::TicketFilter { project: Some(p), ..Default::default() }, &|_| None)).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!((listed[0].login_user, listed[0].login_username.as_str()), (view.login_user, view.login_username.as_str()), "list and view agree");
        (view.login_user, view.login_username, view.created_by_user, view.created_by_username)
    };
    assert_eq!(shown(&f), (alex, "Alex".to_owned(), alex, "Alex".to_owned()), "at first the creator's login");
    assert!(f.store.write(move |tx| tickets::use_my_login(tx, t, robin)).unwrap());
    assert_eq!(shown(&f), (robin, "Robin".to_owned(), alex, "Alex".to_owned()), "Robin's login, Alex stays the creator");
}

#[test]
fn a_disabled_board_member_shows_when_in_the_user_view() {
    let f = fixture();
    let view = |f: &Fixture| f.store.read(|c| users::get(c, &f.robin)).unwrap().unwrap().view().unwrap();
    assert_eq!(view(&f).disabled_at_ms, None);
    let robin = f.robin.clone();
    assert!(f.store.write(move |tx| users::disable(tx, &robin, T0 + 1)).unwrap());
    assert_eq!(view(&f).disabled_at_ms, Some(T0 + 1));
    assert_eq!(f.store.read(|c| users::get(c, &f.alex)).unwrap().unwrap().view().unwrap().disabled_at_ms, None, "Alex is still active");
}

struct Config(Option<String>);

impl serde::Serialize for Config {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_unit()
    }
}

impl conversations::RunConfigInfo for Config {
    fn runtime_kind(&self) -> &'static str {
        "claude_code"
    }
    fn connection(&self) -> Option<String> {
        None
    }
    fn model(&self) -> Option<String> {
        None
    }
    fn login_user(&self) -> Option<String> {
        self.0.clone()
    }
}

/// A chat run on `login` with its ready `run.chat` job (run id = job id).
fn chat_run(f: &Fixture, conversation: &str, login: Option<&str>) -> RunId {
    let (run, conversation, login) = (RunId::new(), conversation.to_owned(), login.map(str::to_owned));
    f.store
        .write(move |tx| {
            conversations::insert_run(tx, run, aurelius_domain::ConversationId::parse(&conversation).unwrap(), &Config(login))?;
            tx.execute(
                "INSERT INTO events(id, aggregate_type, aggregate_id, aggregate_version, type, schema_version, actor_kind, correlation_id,
                                    metadata_json, dedupe_key, occurred_at_ms, recorded_at_ms)
                 VALUES (?1, 'run', ?1, 1, 'project.created', 1, 'user', 'c', '{}', ?1, 1, 1)",
                [run.to_string()],
            )?;
            events::insert_job(tx, &run.to_string(), "run.chat", aurelius_domain::EventId::parse(&run.to_string()).unwrap(), &format!("d-{run}"), &serde_json::json!({ "conversation_id": conversation }))
        })
        .unwrap();
    run
}

fn conversation(f: &Fixture, project: ProjectId) -> String {
    let id = aurelius_domain::ConversationId::new().to_string();
    let c = id.clone();
    f.store
        .write(move |tx| {
            tx.execute("INSERT INTO conversations(id, kind, project_id, title, version, created_at_ms) VALUES (?1, 'project', ?2, 'Chat', 1, 1)", params![c, project.to_string()])?;
            Ok(())
        })
        .unwrap();
    id
}

fn job(f: &Fixture, run: RunId) -> (i64, Option<String>) {
    f.store
        .read(move |c| Ok(c.query_row("SELECT not_before_ms, last_error_code FROM jobs WHERE id = ?1", [run.to_string()], |r| Ok((r.get(0)?, r.get(1)?)))?))
        .unwrap()
}

#[test]
fn runs_record_their_login_and_only_that_logins_runs_wait_for_its_quota() {
    let f = fixture();
    let p = project(&f);
    let chat = conversation(&f, p);
    let (alex, robin) = (f.alex.clone(), f.robin.clone());
    let a = chat_run(&f, &chat, Some(&alex));
    let r = chat_run(&f, &chat, Some(&robin));
    let old = chat_run(&f, &chat, None);
    assert_eq!(f.store.read(move |c| conversations::run_login_user(c, a)).unwrap().as_deref(), Some(alex.as_str()));
    assert_eq!(f.store.read(move |c| conversations::run_login_user(c, old)).unwrap(), None);
    let x = alex.clone();
    f.store.write(move |tx| conversations::set_run_login_user(tx, old, Some(&x))).unwrap();
    assert_eq!(f.store.read(move |c| conversations::run_login_user(c, old)).unwrap().as_deref(), Some(alex.as_str()));
    f.store.write(move |tx| conversations::set_run_login_user(tx, old, None)).unwrap();

    // Alex's limit: only Alex's ready run waits.
    let until = i64::MAX / 4;
    let x = alex.clone();
    let parked = f.store.write(move |tx| queries::park_ready_runs_for_quota_of(tx, "claude_code", Some(&x), until)).unwrap();
    assert_eq!(parked, vec![a.to_string()]);
    assert_eq!(job(&f, a), (until, Some("quota".into())));
    assert_eq!(job(&f, r).1, None, "Robin's run goes on");
    assert_eq!(job(&f, old).1, None, "a run without a login is not Alex's");
    // Robin's limit too; then Alex's resets: only Alex's run wakes.
    let x = robin.clone();
    f.store.write(move |tx| queries::park_ready_runs_for_quota_of(tx, "claude_code", Some(&x), until).map(|_| ())).unwrap();
    let x = alex.clone();
    let woken = f.store.write(move |tx| queries::wake_parked_for_login(tx, "quota", Some(&x), 50)).unwrap();
    assert_eq!(woken, vec![a.to_string()]);
    assert_eq!((job(&f, a), job(&f, r).1), ((50, None), Some("quota".into())));
    // None: the old behaviour, every run of the provider.
    let parked = f.store.write(move |tx| queries::park_ready_runs_for_quota(tx, "claude_code", until)).unwrap();
    assert_eq!(parked.len(), 2, "Alex's and the one without a login; Robin's already waits until then");
    assert_eq!(f.store.write(|tx| queries::wake_parked(tx, "quota", 60)).unwrap(), 3);
    assert_eq!((job(&f, a).0, job(&f, r).0, job(&f, old).0), (60, 60, 60));
}

#[test]
fn work_waiting_for_a_login_is_found_and_woken_per_login() {
    let f = fixture();
    let p = project(&f);
    let chat = conversation(&f, p);
    let (alex, robin) = (f.alex.clone(), f.robin.clone());
    // A chat run of each, parked for 'login'.
    let a = chat_run(&f, &chat, Some(&alex));
    let r = chat_run(&f, &chat, Some(&robin));
    for run in [a, r] {
        f.store.write(move |tx| Ok(tx.execute("UPDATE jobs SET not_before_ms = ?2, last_error_code = 'login' WHERE id = ?1", params![run.to_string(), i64::MAX / 4]).map(|_| ())?)).unwrap();
    }
    // Tasks waiting with 'login': one on Alex's ticket, one whose last run was on Robin's login, one of Alex's that does not wait.
    let t_alex = ticket(&f, p, &alex);
    let task = |ticket: Option<TicketId>, status: &str, reason: Option<&str>| -> aurelius_domain::TaskId {
        let (id, status, reason) = (aurelius_domain::TaskId::new(), status.to_owned(), reason.map(str::to_owned));
        f.store
            .write(move |tx| {
                tx.execute("INSERT OR IGNORE INTO blobs(id, storage_key, sha256, byte_length, media_type, created_at_ms) VALUES ('b', 'k', 's', 0, 'text/plain', 1)", [])?;
                tx.execute(
                    "INSERT INTO tasks(id, project_id, title, brief_blob_id, status, waiting_reason, version, created_at_ms, updated_at_ms, ticket_id)
                     VALUES (?1, ?2, 'Work', 'b', ?3, ?4, 1, 1, 1, ?5)",
                    params![id.to_string(), p.to_string(), status, reason, ticket.map(|t| t.to_string())],
                )?;
                Ok(())
            })
            .unwrap();
        id
    };
    let on_ticket = task(Some(t_alex), "waiting", Some("login"));
    let on_run = task(None, "waiting", Some("login"));
    let running = task(Some(t_alex), "running", None);
    let quota = task(Some(t_alex), "waiting", Some("quota"));
    let run_of = |task: aurelius_domain::TaskId, attempt: i64, login: &str| {
        let login = login.to_owned();
        f.store
            .write(move |tx| {
                tx.execute(
                    "INSERT INTO runs(id, task_id, attempt, runtime_kind, model_id, resolved_config_json, status, login_user)
                     VALUES (?1, ?2, ?3, 'claude_code', '', '{}', 'failed', ?4)",
                    params![RunId::new().to_string(), task.to_string(), attempt, login],
                )?;
                Ok(())
            })
            .unwrap();
    };
    run_of(on_run, 1, &alex);
    run_of(on_run, 2, &robin);

    let waiting = |login: &str| {
        let login = login.to_owned();
        f.store.read(move |c| queries::tasks_waiting_for_login(c, &login)).unwrap()
    };
    assert_eq!(waiting(&alex), vec![on_ticket], "the latest run of the other task was on Robin's login");
    assert_eq!(waiting(&robin), vec![on_run]);
    let _ = (running, quota);
    // Robin takes the ticket over: its waiting task is Robin's now.
    let robin_id = UserId::parse(&robin).unwrap();
    f.store.write(move |tx| tickets::use_my_login(tx, t_alex, robin_id).map(|_| ())).unwrap();
    assert!(waiting(&alex).is_empty());
    assert_eq!(waiting(&robin).len(), 2);

    // Linking Robin's login wakes only Robin's parked run.
    let x = robin.clone();
    assert_eq!(f.store.write(move |tx| queries::wake_parked_for_login(tx, "login", Some(&x), 70)).unwrap(), vec![r.to_string()]);
    assert_eq!((job(&f, r), job(&f, a).1), ((70, None), Some("login".into())));
    // The task's CHECK takes 'login' and still refuses a reason it does not know.
    let bad = f.store.write(move |tx| Ok(tx.execute("UPDATE tasks SET waiting_reason = 'auth' WHERE id = ?1", [on_run.to_string()])?));
    assert!(bad.is_err());
}

#[test]
fn schedules_and_the_company_name_a_login() {
    let f = fixture();
    // The company login.
    assert_eq!(f.store.read(company::background_login_user).unwrap(), None);
    let alex = UserId::parse(&f.alex).unwrap();
    let before = f.store.read(company::get).unwrap().version;
    assert_eq!(f.store.write(move |tx| company::set_background_login_user(tx, Some(alex))).unwrap(), before + 1);
    assert_eq!(f.store.read(company::background_login_user).unwrap(), Some(alex));
    f.store.write(|tx| company::set_background_login_user(tx, None).map(|_| ())).unwrap();
    assert_eq!(f.store.read(company::background_login_user).unwrap(), None);
    assert!(f.store.write(|tx| company::set_background_login_user(tx, Some(UserId::new())).map(|_| ())).is_err(), "references users");

    // A recurring task names the login its tasks run on.
    let schedule = aurelius_domain::TaskScheduleId::new();
    assert_eq!(f.store.read(move |c| crate::task_schedules::login_user(c, schedule)).unwrap(), None);

    // All of them are plain nullable references to users.
    for table in ["recurring_tasks", "remote_clients", "runs"] {
        let (notnull, target): (i64, String) = f
            .store
            .read(move |c| {
                let notnull = c.query_row(&format!("SELECT \"notnull\" FROM pragma_table_info('{table}') WHERE name IN ('login_user', 'user_id')"), [], |r| r.get(0))?;
                let target = c.query_row(&format!("SELECT \"table\" FROM pragma_foreign_key_list('{table}') WHERE \"from\" IN ('login_user', 'user_id')"), [], |r| r.get(0))?;
                Ok((notnull, target))
            })
            .unwrap();
        assert_eq!((notnull, target.as_str()), (0, "users"), "{table}");
    }
}
