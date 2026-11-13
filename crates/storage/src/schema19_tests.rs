//! Migration 19: the channels of the Team chat, the tickets' discussions and
//! the board member who wrote a message.

use crate::conversations::{self, NewMessage};
use crate::events::{NewEvent, append};
use crate::tickets::{self, NewTicket, TicketFields};
use crate::{Store, company, discussions, events, migrate, users};
use aurelius_domain::conversation::{ConversationKind, MessageRole, MessageStatus};
use aurelius_domain::task::RunSettings;
use aurelius_domain::ticket::{ColumnKind, TicketPriority};
use aurelius_domain::{ActorKind, ConversationId, EmployeeId, MessageId, ProjectId, RunId, TicketId, UserId};
use rusqlite::{Connection, params};

const PHC: &str = "$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHQ$aGFzaGhhc2hoYXNoaGFzaA";
const ALPHA: &str = "00000000-0000-7000-8000-000000000001";
const BETA: &str = "00000000-0000-7000-8000-000000000002";
const USER: &str = "00000000-0000-7000-8000-000000000003";
const T1: &str = "00000000-0000-7000-8000-000000000004";
const T2: &str = "00000000-0000-7000-8000-000000000005";
const T3: &str = "00000000-0000-7000-8000-000000000006";
const EVENT: &str = "00000000-0000-7000-8000-000000000007";
const NOTE: &str = "00000000-0000-7000-8000-000000000008";
const PLANS: &str = "00000000-0000-7000-8000-000000000009";

fn db(dir: &std::path::Path) -> Connection {
    let conn = Connection::open(dir.join("data/aurelius.sqlite")).unwrap();
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();
    conn
}

fn strings(conn: &Connection, sql: &str) -> Vec<String> {
    conn.prepare(sql).unwrap().query_map([], |r| r.get(0)).unwrap().collect::<Result<_, _>>().unwrap()
}

fn count(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get(0)).unwrap()
}

fn event(n: usize) -> NewEvent<'static> {
    NewEvent {
        aggregate_type: "project",
        aggregate_id: format!("message-{n}"),
        aggregate_version: 1,
        event_type: "project.created",
        project_id: None,
        actor_kind: ActorKind::User,
        actor_id: None,
        correlation_id: "test".into(),
        causation_id: None,
        payload_blob_id: None,
        metadata: serde_json::json!({}),
        dedupe_key: format!("k-message-{n}"),
    }
}

#[test]
fn migration_18_is_released_and_19_rebuilds() {
    let board = migrate::MIGRATIONS.iter().find(|m| m.version == 18).unwrap();
    assert_eq!(migrate::checksum(board.sql), "e5b2d44d48c03983e2086e209cbb3611d56561ffcd98ac2d81d67f756c8b6d20", "migration 18 changed");
    let discussions = migrate::MIGRATIONS.iter().find(|m| m.version == discussions::DISCUSSIONS_SCHEMA).unwrap();
    assert!(discussions.rebuild);
    // 20 only adds the read markers.
    let reads = migrate::MIGRATIONS.iter().find(|m| m.version == discussions::READS_SCHEMA).unwrap();
    assert!(!reads.rebuild);
}

/// A schema 18 database with the group chat (and a message in it), a project
/// conversation, two projects (one archived) and three tickets.
fn board_database(dir: &std::path::Path) -> String {
    drop(Store::open_until(dir, 18).unwrap());
    let conn = db(dir);
    let company: String = conn.query_row("SELECT id FROM company", [], |r| r.get(0)).unwrap();
    let global: String = conn.query_row("SELECT id FROM scopes WHERE kind = 'global'", [], |r| r.get(0)).unwrap();
    let group: String = conn.query_row("SELECT id FROM conversations WHERE kind = 'group'", [], |r| r.get(0)).unwrap();
    let column = |kind: &str| -> String { conn.query_row("SELECT id FROM board_columns WHERE kind = ?1", [kind], |r| r.get(0)).unwrap() };
    let (backlog, done) = (column("backlog"), column("done"));
    conn.execute_batch(&format!(
        "INSERT INTO scopes(id, kind, parent_id, owner_id) VALUES ('sa', 'project', '{global}', '{ALPHA}'), ('sb', 'project', '{global}', '{BETA}');
         INSERT INTO projects(id, scope_id, name, status, version, created_at_ms, updated_at_ms) VALUES
           ('{ALPHA}', 'sa', 'Alpha', 'active', 1, 1, 1), ('{BETA}', 'sb', 'Beta', 'archived', 2, 1, 2);
         INSERT INTO conversations(id, kind, project_id, title, version, created_at_ms) VALUES ('{PLANS}', 'project', '{ALPHA}', 'Plans', 1, 3);
         INSERT INTO users(id, company_id, username, password_hash, created_at_ms, updated_at_ms) VALUES ('{USER}', '{company}', 'grace', '{PHC}', 1, 1);
         INSERT INTO tickets(id, company_id, project_id, number, title, column_id, position, priority, created_by_user, labels_json,
                             version, created_at_ms, updated_at_ms, done_at_ms) VALUES
           ('{T1}', '{company}', '{ALPHA}', 1, 'Fix the login', '{backlog}', 1.0, 'high', '{USER}', '[\"ui\"]', 3, 10, 11, NULL),
           ('{T2}', '{company}', '{ALPHA}', 2, 'Write the docs', '{done}', 1.0, 'normal', '{USER}', NULL, 5, 20, 21, 22),
           ('{T3}', '{company}', '{BETA}', 3, 'Old work', '{backlog}', 2.0, 'low', '{USER}', NULL, 1, 30, 30, NULL);
         INSERT INTO events(id, aggregate_type, aggregate_id, aggregate_version, type, schema_version, actor_kind, correlation_id,
                            metadata_json, dedupe_key, occurred_at_ms, recorded_at_ms)
           VALUES ('{EVENT}', 'conversation', '{group}', 1, 'message.created', 1, 'user', 'c', '{{}}', 'k', 1, 1);
         INSERT INTO messages(id, conversation_id, ordinal, role, source_event_id, status, input_kind, created_at_ms)
           VALUES ('{NOTE}', '{group}', 1, 'user', '{EVENT}', 'complete', 'text', 1);"
    ))
    .unwrap();
    group
}

#[test]
fn a_board_database_migrates_without_losing_anything() {
    let dir = tempfile::tempdir().unwrap();
    let group = board_database(dir.path());
    let tickets_before = {
        let conn = db(dir.path());
        strings(&conn, "SELECT id || ' ' || project_id || ' ' || number || ' ' || title || ' ' || column_id || ' ' || position || ' ' || priority || ' '
                        || created_by_user || ' ' || COALESCE(labels_json, '-') || ' ' || version || ' ' || created_at_ms || ' ' || updated_at_ms || ' '
                        || COALESCE(done_at_ms, '-') FROM tickets ORDER BY id")
    };
    let conversations_before = strings(&db(dir.path()), "SELECT id || ' ' || kind || ' ' || COALESCE(project_id, '-') || ' ' || title FROM conversations ORDER BY id");

    let store = Store::open(dir.path()).unwrap();
    let conn = db(dir.path());
    assert_eq!(count(&conn, "SELECT MAX(version) FROM schema_migrations"), crate::migrate::supported_version());
    assert!(strings(&conn, "SELECT \"table\" FROM pragma_foreign_key_check").is_empty());

    // Every row is still there, unchanged.
    let tickets_after = strings(&conn, "SELECT id || ' ' || project_id || ' ' || number || ' ' || title || ' ' || column_id || ' ' || position || ' ' || priority || ' '
                                        || created_by_user || ' ' || COALESCE(labels_json, '-') || ' ' || version || ' ' || created_at_ms || ' ' || updated_at_ms || ' '
                                        || COALESCE(done_at_ms, '-') FROM tickets ORDER BY id");
    assert_eq!(tickets_after, tickets_before);
    let kept = strings(&conn, "SELECT id || ' ' || kind || ' ' || COALESCE(project_id, '-') || ' ' || title FROM conversations ORDER BY id");
    for row in &conversations_before {
        assert!(kept.contains(row), "{row} is kept");
    }
    // Two channels and three discussions more, nothing else.
    assert_eq!(kept.len(), conversations_before.len() + 2 + 3);
    assert_eq!(count(&conn, &format!("SELECT COUNT(*) FROM messages WHERE id = '{NOTE}' AND conversation_id = '{group}' AND author_user_id IS NULL")), 1);

    // The group chat is the general channel.
    let group = ConversationId::parse(&group).unwrap();
    assert_eq!(store.read(|c| discussions::channel(c, None)).unwrap(), Some(group));
    assert_eq!(store.read(conversations::group_conversation).unwrap(), Some(group));

    // Every project has its channel, named after it; the archived one is read-only.
    let channels = store.read(discussions::channels).unwrap();
    let listed: Vec<(Option<String>, &str, bool)> = channels.iter().map(|c| (c.project_id.map(|p| p.to_string()), c.name.as_str(), c.read_only)).collect();
    assert_eq!(listed, vec![(None, "Team chat", false), (Some(ALPHA.into()), "Alpha", false), (Some(BETA.into()), "Beta", true)]);
    assert_eq!(channels[0].conversation_id, group);
    for channel in &channels[1..] {
        assert_eq!(store.read(|c| discussions::channel(c, channel.project_id)).unwrap(), Some(channel.conversation_id));
        assert_eq!(store.read(|c| discussions::kind_of(c, channel.conversation_id)).unwrap(), Some(ConversationKind::Group));
    }

    // Every ticket has exactly one discussion, in its project.
    for (ticket, project, title) in [(T1, ALPHA, "#1 Fix the login"), (T2, ALPHA, "#2 Write the docs"), (T3, BETA, "#3 Old work")] {
        let id = TicketId::parse(ticket).unwrap();
        let discussion = store.read(|c| discussions::ticket_conversation(c, id)).unwrap().expect("a discussion");
        assert_eq!(store.read(|c| discussions::ticket_of_conversation(c, discussion)).unwrap(), Some(id));
        let (kind, of, name): (String, String, String) = conn
            .query_row("SELECT kind, project_id, title FROM conversations WHERE id = ?1", [discussion.to_string()], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap();
        assert_eq!((kind.as_str(), of.as_str(), name.as_str()), ("ticket", project, title));
    }
    assert_eq!(count(&conn, "SELECT COUNT(DISTINCT conversation_id) FROM tickets"), 3);
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM conversations WHERE kind = 'ticket'"), 3);
}

#[test]
fn the_indexes_keep_one_general_channel_one_per_project_and_one_discussion_per_ticket() {
    let dir = tempfile::tempdir().unwrap();
    board_database(dir.path());
    drop(Store::open(dir.path()).unwrap());
    let conn = db(dir.path());
    let indexes = strings(&conn, "SELECT name FROM sqlite_master WHERE type = 'index' AND name NOT LIKE 'sqlite_%' ORDER BY name");
    for name in ["one_general_channel", "one_project_channel", "messages_waking", "conversations_project", "conversations_employee", "tickets_column", "tickets_project", "tickets_root_task"] {
        assert!(indexes.iter().any(|i| i == name), "index {name}");
    }
    assert!(!indexes.iter().any(|i| i == "one_group_conversation"), "replaced by the two channel indexes");

    let conversation = |id: &str, kind: &str, project: Option<&str>| {
        conn.execute("INSERT INTO conversations(id, kind, project_id, title, version, created_at_ms) VALUES (?1, ?2, ?3, 'x', 1, 1)", params![id, kind, project])
    };
    assert!(conversation("c1", "group", None).is_err(), "one general channel");
    assert!(conversation("c1", "group", Some(ALPHA)).is_err(), "one channel per project");
    assert!(conversation("c1", "group", Some("nowhere")).is_err(), "a channel's project exists");
    assert!(conversation("c1", "ticket", None).is_err(), "a discussion has its ticket's project");
    assert!(conversation("c1", "chat", None).is_err(), "kinds are fixed");
    conversation("c1", "ticket", Some(ALPHA)).unwrap();
    let employee_group = conn.execute(
        "INSERT INTO conversations(id, kind, project_id, employee_id, title, version, created_at_ms) VALUES ('c2', 'group', NULL, 'e', 'x', 1, 1)",
        [],
    );
    assert!(employee_group.is_err(), "a channel has no employee");

    // tickets.conversation_id: required, existing, and its own.
    let other: String = conn.query_row("SELECT conversation_id FROM tickets WHERE id = ?1", [T2], |r| r.get(0)).unwrap();
    assert!(conn.execute("UPDATE tickets SET conversation_id = NULL WHERE id = ?1", [T1]).is_err(), "NOT NULL");
    assert!(conn.execute("UPDATE tickets SET conversation_id = 'nope' WHERE id = ?1", [T1]).is_err(), "references conversations");
    assert!(conn.execute("UPDATE tickets SET conversation_id = ?2 WHERE id = ?1", params![T1, other]).is_err(), "one ticket per discussion");
    conn.execute("UPDATE tickets SET conversation_id = 'c1' WHERE id = ?1", [T1]).unwrap();

    // messages.author_user_id: optional, and a board member when given.
    assert!(conn.execute("UPDATE messages SET author_user_id = 'nobody' WHERE id = ?1", [NOTE]).is_err(), "references users");
    conn.execute("UPDATE messages SET author_user_id = ?2 WHERE id = ?1", params![NOTE, USER]).unwrap();
    conn.execute("UPDATE messages SET author_user_id = NULL WHERE id = ?1", [NOTE]).unwrap();
}

struct Fixture {
    _dir: tempfile::TempDir,
    store: Store,
    user: UserId,
    project: ProjectId,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let (user, project) = (UserId::new(), ProjectId::new());
    store
        .write(move |tx| {
            users::insert(tx, &users::NewUser { id: &user.to_string(), username: "grace", created_by_user: None, now_ms: 1 }, PHC)?;
            events::insert_project_projection(tx, project, "Atlas", None)?;
            Ok(())
        })
        .unwrap();
    Fixture { _dir: dir, store, user, project }
}

fn add_ticket(f: &Fixture, title: &str) -> TicketId {
    let (id, user, project, title) = (TicketId::new(), f.user, f.project, title.to_owned());
    f.store
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
                    labels: &[],
                    deadline_ms: None,
                    kind: Default::default(),
                },
            )
        })
        .unwrap();
    id
}

#[test]
fn a_new_ticket_gets_exactly_one_discussion() {
    let f = fixture();
    let before = f.store.read(|c| Ok(count(c, "SELECT COUNT(*) FROM conversations"))).unwrap();
    let ticket = add_ticket(&f, "Fix the login");
    let after = f.store.read(|c| Ok(count(c, "SELECT COUNT(*) FROM conversations"))).unwrap();
    assert_eq!(after, before + 1, "exactly one conversation more");

    let discussion = f.store.read(|c| discussions::ticket_conversation(c, ticket)).unwrap().unwrap();
    assert_eq!(f.store.read(|c| discussions::kind_of(c, discussion)).unwrap(), Some(ConversationKind::Ticket));
    assert_eq!(f.store.read(|c| discussions::ticket_of_conversation(c, discussion)).unwrap(), Some(ticket));
    assert_eq!(f.store.read(|c| conversations::owner_of(c, discussion)).unwrap(), (None, Some(f.project)));
    let title = |f: &Fixture| f.store.read(move |c| Ok(c.query_row("SELECT title FROM conversations WHERE id = ?1", [discussion.to_string()], |r| r.get::<_, String>(0))?)).unwrap();
    assert_eq!(title(&f), "#1 Fix the login");
    f.store.write(move |tx| tickets::update_fields(tx, ticket, &TicketFields { title: Some("Fix the sign-in"), ..Default::default() })).unwrap();
    assert_eq!(title(&f), "#1 Fix the sign-in", "the discussion follows the title");

    // The discussion takes no chat messages and no app: only its own write.
    assert!(!f.store.write(move |tx| conversations::conversation_exists(tx, discussion)).unwrap());

    // A second ticket has its own.
    let second = add_ticket(&f, "Write the docs");
    let other = f.store.read(|c| discussions::ticket_conversation(c, second)).unwrap().unwrap();
    assert_ne!(other, discussion);
    assert_eq!(f.store.read(|c| discussions::ticket_conversation(c, TicketId::new())).unwrap(), None);
}

#[test]
fn a_new_project_gets_its_channel_in_the_same_transaction() {
    let f = fixture();
    let channel = f.store.read(|c| discussions::channel(c, Some(f.project))).unwrap().expect("a channel");
    assert_eq!(f.store.read(|c| discussions::kind_of(c, channel)).unwrap(), Some(ConversationKind::Group));
    assert!(f.store.read(|c| discussions::channel(c, None)).unwrap().is_some(), "the general channel is there too");
    assert_ne!(f.store.read(|c| discussions::channel(c, None)).unwrap(), Some(channel));
    assert!(!f.store.write(move |tx| conversations::conversation_exists(tx, channel)).unwrap());

    // A project that fails to be made leaves no channel behind.
    let failed = ProjectId::new();
    let refused = f.store.write(move |tx| {
        events::insert_project_projection(tx, failed, "Hermes", None)?;
        Err::<(), _>(crate::StorageError::Rejected("stop".into()))
    });
    assert!(refused.is_err());
    assert_eq!(f.store.read(|c| discussions::channel(c, Some(failed))).unwrap(), None);

    // The channel follows the project's name; the list is general first, then by name.
    let project = f.project;
    f.store.write(move |tx| events::rename_project_projection(tx, project, "Zeta", 2)).unwrap();
    let other = ProjectId::new();
    f.store.write(move |tx| events::insert_project_projection(tx, other, "Beta", None)).unwrap();
    let names: Vec<String> = f.store.read(discussions::channels).unwrap().into_iter().map(|c| c.name).collect();
    assert_eq!(names, ["Team chat", "Beta", "Zeta"]);
}

#[test]
fn a_project_on_an_older_schema_gets_its_channel_from_the_migration() {
    let dir = tempfile::tempdir().unwrap();
    let project = ProjectId::new();
    let store = Store::open_until(dir.path(), 17).unwrap();
    store.write(move |tx| events::insert_project_projection(tx, project, "Atlas", None)).unwrap();
    assert!(!store.read(discussions::has_discussions).unwrap());
    drop(store);
    let store = Store::open(dir.path()).unwrap();
    assert!(store.read(discussions::has_discussions).unwrap());
    assert!(store.read(|c| discussions::channel(c, Some(project))).unwrap().is_some());
}

#[test]
fn waking_posts_count_over_every_channel_and_ticket_together() {
    let f = fixture();
    let ticket = add_ticket(&f, "Fix the login");
    let (run, other_run, employee, someone) = (RunId::new(), RunId::new(), EmployeeId::new(), EmployeeId::new());
    let project = f.project;
    let text = f.store.blobs().put(b"I asked Ada", "text/plain").unwrap();
    f.store
        .write(move |tx| {
            crate::blobs::register(tx, &text)?;
            let key = text.id.to_string();
            let general = discussions::channel(tx, None)?.unwrap();
            let channel = discussions::channel(tx, Some(project))?.unwrap();
            let discussion = discussions::ticket_conversation(tx, ticket)?.unwrap();
            let plans = ConversationId::new();
            conversations::insert_project_conversation(tx, plans, project, "Plans")?;
            let post = |n: usize, conversation: ConversationId, run: RunId, employee: EmployeeId, woke: u32| -> Result<(), crate::StorageError> {
                let appended = append(tx, event(n))?;
                let meta = serde_json::json!({"kind": "group", "from": "Ada", "woke": woke, "employee_id": employee.to_string(), "run_id": run.to_string()});
                conversations::insert_update(tx, MessageId::new(), conversation, &key, appended.id, &meta)?;
                Ok(())
            };
            post(1, general, run, employee, 1)?;
            post(2, channel, run, employee, 2)?;
            post(3, discussion, other_run, employee, 1)?;
            // Not counted: a post that woke nobody, and a post outside the discussions.
            post(4, discussion, run, employee, 0)?;
            post(5, plans, run, employee, 1)?;
            // Someone else's.
            post(6, channel, other_run, someone, 1)?;
            tx.execute("UPDATE messages SET created_at_ms = 0 WHERE source_event_id IN (SELECT id FROM events WHERE dedupe_key = 'k-message-1')", [])?;
            Ok(())
        })
        .unwrap();
    assert_eq!(f.store.read(move |c| discussions::waking_posts(c, run, employee, 0)).unwrap(), (2, 3));
    assert_eq!(f.store.read(move |c| discussions::waking_posts(c, run, employee, 1)).unwrap(), (2, 2), "the window leaves the old post out");
    assert_eq!(f.store.read(move |c| discussions::waking_posts(c, other_run, someone, 0)).unwrap(), (2, 1));
    // The old counter counts the same, whatever group it is given.
    let any = ConversationId::new();
    assert_eq!(f.store.read(move |c| conversations::waking_posts(c, any, run, employee, 0)).unwrap(), (2, 3));
}

#[test]
fn a_message_carries_the_board_member_who_wrote_it() {
    let f = fixture();
    let ticket = add_ticket(&f, "Fix the login");
    let user = f.user;
    let texts: Vec<_> = (0..3).map(|n| f.store.blobs().put(format!("message {n}").as_bytes(), "text/plain").unwrap()).collect();
    let (mine, theirs, update) = (MessageId::new(), MessageId::new(), MessageId::new());
    let discussion = f
        .store
        .write(move |tx| {
            let discussion = discussions::ticket_conversation(tx, ticket)?.unwrap();
            for text in &texts {
                crate::blobs::register(tx, text)?;
            }
            for (n, (id, by)) in [(mine, Some(user)), (theirs, None)].into_iter().enumerate() {
                let text = &texts[n];
                let appended = append(tx, event(n))?;
                let message = NewMessage {
                    id,
                    conversation_id: discussion,
                    role: MessageRole::User,
                    content_blob_id: Some(text.id.to_string()),
                    source_event_id: appended.id,
                    status: MessageStatus::Complete,
                    run_id: None,
                    reply_to: None,
                };
                conversations::insert_message_by(tx, message, by)?;
            }
            let appended = append(tx, event(9))?;
            conversations::insert_update_by(tx, update, discussion, &texts[2].id.to_string(), appended.id, &serde_json::json!({"system": {"kind": "moved"}}), Some(user))?;
            Ok(discussion)
        })
        .unwrap();
    assert_eq!(f.store.read(move |c| discussions::message_author_user(c, mine)).unwrap(), Some(user));
    assert_eq!(f.store.read(move |c| discussions::message_author_user(c, theirs)).unwrap(), None);
    assert_eq!(f.store.read(move |c| discussions::message_author_user(c, update)).unwrap(), Some(user));
    let authors = f.store.read(move |c| discussions::message_author_users(c, discussion)).unwrap();
    assert_eq!(authors.len(), 2);
    assert_eq!(authors.get(&mine), Some(&user));

    let lines = f.store.read(move |c| discussions::latest_messages(c, discussion, None, 10)).unwrap();
    let read: Vec<(MessageId, Option<String>, Option<String>)> = lines.into_iter().map(|l| (l.id, l.author, l.author_user.map(|(_, name)| name))).collect();
    assert_eq!(read, vec![(theirs, None, None), (mine, None, Some("grace".into()))], "newest first; the system line is not read");

    // Only a board member there is.
    let unknown = f.store.write(move |tx| {
        Ok(tx.execute("UPDATE messages SET author_user_id = ?2 WHERE id = ?1", params![theirs.to_string(), UserId::new().to_string()])?)
    });
    assert!(unknown.is_err(), "an unknown board member is refused");
}
