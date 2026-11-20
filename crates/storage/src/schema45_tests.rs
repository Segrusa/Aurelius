//! Migration 45: chatting with an employee is gone. The board members' own chats with
//! employees and the Team chat (General, the project channels and the employees'
//! threads of them) are deleted with what hangs off them; the employees' main chats,
//! the tickets' discussions and the work sessions stay.

use crate::Store;
use rusqlite::Connection;

const PHC: &str = "$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHQ$aGFzaGhhc2hoYXNoaGFzaA";
const PROJECT: &str = "00000000-0000-7000-8000-000000000451";
const EMPLOYEE: &str = "00000000-0000-7000-8000-000000000452";
const USER: &str = "00000000-0000-7000-8000-000000000453";
const EVENT: &str = "00000000-0000-7000-8000-000000000454";
const MAIN: &str = "00000000-0000-7000-8000-000000000455";
const BOARD: &str = "00000000-0000-7000-8000-000000000456";
const GENERAL: &str = "00000000-0000-7000-8000-000000000457";
const CHANNEL: &str = "00000000-0000-7000-8000-000000000458";
const CHANNEL_THREAD: &str = "00000000-0000-7000-8000-000000000459";
const DISCUSSION: &str = "00000000-0000-7000-8000-00000000045a";
const TICKET_THREAD: &str = "00000000-0000-7000-8000-00000000045b";
const SESSION: &str = "00000000-0000-7000-8000-00000000045c";
const TICKET: &str = "00000000-0000-7000-8000-00000000045d";
const OTHER_TICKET: &str = "00000000-0000-7000-8000-00000000045e";
const OTHER_DISCUSSION: &str = "00000000-0000-7000-8000-00000000045f";

fn db(dir: &std::path::Path) -> Connection {
    let conn = Connection::open(dir.join("data/aurelius.sqlite")).unwrap();
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();
    conn
}

fn count(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get(0)).unwrap()
}

/// A schema 44 store: Orion with his main chat (and its settings), a board member's chat with
/// him (a run, its usage and a ticket made there), General with a thread and a reaction, a
/// channel of the project with an employee thread of it (a run, a mailbox item), a ticket
/// discussion with a thread (one post written by a run of the channel thread), a thread of
/// his on a ticket, and a work session.
fn schema_44_database(dir: &std::path::Path) {
    drop(Store::open_until(dir, 44).unwrap());
    let conn = db(dir);
    // Migration 17 made the General channel at 17; this one is the seeded one.
    conn.execute("DELETE FROM conversations WHERE kind = 'group'", []).unwrap();
    let company: String = conn.query_row("SELECT id FROM company", [], |r| r.get(0)).unwrap();
    let global: String = conn.query_row("SELECT id FROM scopes WHERE kind = 'global'", [], |r| r.get(0)).unwrap();
    let column: String = conn.query_row("SELECT id FROM board_columns WHERE kind = 'backlog'", [], |r| r.get(0)).unwrap();
    let run = |id: &str, conversation: &str| {
        format!("INSERT INTO runs(id, employee_id, conversation_id, attempt, runtime_kind, model_id, resolved_config_json, status) VALUES ('{id}', '{EMPLOYEE}', '{conversation}', 1, 'fake', 'm', '{{}}', 'succeeded');")
    };
    let message = |id: &str, conversation: &str, ordinal: u32, extra: &str| {
        format!("INSERT INTO messages(id, conversation_id, ordinal, role, source_event_id, status, input_kind, created_at_ms{}) VALUES ('{id}', '{conversation}', {ordinal}, 'user', '{EVENT}', 'complete', 'text', 1{});", if extra.is_empty() { "" } else { ", run_id, thread_parent_id" }, if extra.is_empty() { String::new() } else { format!(", {extra}") })
    };
    // A post in a ticket's discussion written by a run of the channel thread (its run goes, the post stays).
    let (r_main, r_board, r_channel) = (run("rm", MAIN), run("rb", BOARD), format!("{} {}", run("rc", CHANNEL_THREAD), run("rt", TICKET_THREAD)));
    let (m_main, m_board, g1) = (message("m1", MAIN, 1, ""), message("b1", BOARD, 1, ""), message("g1", GENERAL, 1, ""));
    // A reply in the thread of g1, and the post of the run in a ticket's discussion with a reply under it.
    let (g2, d1, d2) = (message("g2", GENERAL, 2, "NULL, 'g1'"), message("d1", DISCUSSION, 1, "'rc', NULL"), message("d2", DISCUSSION, 2, "NULL, 'd1'"));
    conn.execute_batch(&format!(
        "INSERT INTO scopes(id, kind, parent_id, owner_id) VALUES ('ps', 'project', '{global}', '{PROJECT}'), ('es', 'employee', '{global}', '{EMPLOYEE}');
         INSERT INTO projects(id, scope_id, name, status, version, created_at_ms, updated_at_ms) VALUES ('{PROJECT}', 'ps', 'Alpha', 'active', 1, 1, 1);
         INSERT INTO employees(id, scope_id, name, role, config_json, status, version, created_at_ms, updated_at_ms) VALUES ('{EMPLOYEE}', 'es', 'Orion', 'PM', '{{}}', 'active', 1, 1, 1);
         INSERT INTO users(id, company_id, username, password_hash, created_at_ms, updated_at_ms) VALUES ('{USER}', '{company}', 'grace', '{PHC}', 1, 1);
         INSERT INTO events(id, aggregate_type, aggregate_id, aggregate_version, type, schema_version, actor_kind, correlation_id, metadata_json, dedupe_key, occurred_at_ms, recorded_at_ms)
           VALUES ('{EVENT}', 'conversation', '{MAIN}', 1, 'message.user_committed', 1, 'user', 'c', '{{}}', 'k', 1, 1);

         INSERT INTO conversations(id, kind, employee_id, title, version, created_at_ms, agent_mode) VALUES ('{MAIN}', 'employee', '{EMPLOYEE}', 'Orion', 4, 7, 'auto');
         INSERT INTO conversations(id, kind, employee_id, title, version, created_at_ms, board_member_id) VALUES ('{BOARD}', 'employee', '{EMPLOYEE}', 'Orion', 1, 8, '{USER}');
         INSERT INTO conversations(id, kind, title, version, created_at_ms) VALUES ('{GENERAL}', 'group', 'Team chat', 1, 9);
         INSERT INTO conversations(id, kind, project_id, title, version, created_at_ms) VALUES ('{CHANNEL}', 'group', '{PROJECT}', 'Alpha', 1, 9);
         INSERT INTO conversations(id, kind, employee_id, title, version, created_at_ms, thread_key) VALUES ('{CHANNEL_THREAD}', 'employee', '{EMPLOYEE}', 'Orion', 1, 10, 'channel:{GENERAL}');
         INSERT INTO conversations(id, kind, project_id, title, version, created_at_ms) VALUES ('{DISCUSSION}', 'ticket', '{PROJECT}', '#1 Fix it', 1, 11), ('{OTHER_DISCUSSION}', 'ticket', '{PROJECT}', '#2 Other', 1, 11);
         INSERT INTO conversations(id, kind, employee_id, title, version, created_at_ms, thread_key) VALUES ('{TICKET_THREAD}', 'employee', '{EMPLOYEE}', 'Orion', 1, 12, 'ticket:{TICKET}');
         INSERT INTO conversations(id, kind, project_id, employee_id, title, version, created_at_ms) VALUES ('{SESSION}', 'session', '{PROJECT}', '{EMPLOYEE}', 'Fix the login', 1, 13);
         INSERT INTO work_sessions(conversation_id, started_by, created_at_ms) VALUES ('{SESSION}', '{USER}', 13);
         INSERT INTO tickets(id, company_id, project_id, number, title, column_id, position, priority, created_by_user, version, created_at_ms, updated_at_ms, conversation_id, login_user)
           VALUES ('{TICKET}', '{company}', '{PROJECT}', 1, 'Fix it', '{column}', 1.0, 'normal', '{USER}', 1, 1, 1, '{DISCUSSION}', '{USER}'),
                  ('{OTHER_TICKET}', '{company}', '{PROJECT}', 2, 'Other', '{column}', 2.0, 'normal', '{USER}', 1, 1, 1, '{OTHER_DISCUSSION}', '{USER}');
         INSERT INTO chat_tickets(ticket_id, conversation_id, created_at_ms) VALUES ('{TICKET}', '{BOARD}', 1);

         {r_main} {r_board} {r_channel}
         INSERT INTO usage_records(id, run_id, source_event_id, estimate, provider_record_key, recorded_at_ms) VALUES ('u1', 'rb', '{EVENT}', 0, 'k1', 1);
         INSERT INTO run_activity(run_id, call_id, message_id, tool, input_json, text_offset, started_at_ms) VALUES ('rb', 'c1', 'x', 'Bash', '{{}}', 0, 1);

         {m_main} {m_board}
         {g1} {g2}
         {d1} {d2}
         INSERT INTO message_reactions(message_id, employee_id, emoji, created_at_ms) VALUES ('g1', '{EMPLOYEE}', 'x', 1), ('d1', '{EMPLOYEE}', 'x', 1);
         INSERT INTO thread_reads(user_id, root_message_id, read_through, read_at_ms) VALUES ('{USER}', 'g1', 2, 1), ('{USER}', 'd1', 2, 1);
         INSERT INTO discussion_reads(user_id, conversation_id, read_through, read_at_ms) VALUES ('{USER}', '{GENERAL}', 2, 1), ('{USER}', '{DISCUSSION}', 2, 1);
         INSERT INTO channel_members(conversation_id, user_id, joined_at_ms) VALUES ('{DISCUSSION}', '{USER}', 1);
         INSERT INTO notifications(id, event_id, dedupe_key, severity, target_json, created_at_ms, user_id, conversation_id, message_id) VALUES
           ('n1', '{EVENT}', 'n1', 'info', '{{}}', 1, '{USER}', '{GENERAL}', 'g1'), ('n2', '{EVENT}', 'n2', 'info', '{{}}', 1, '{USER}', '{DISCUSSION}', 'd1');
         INSERT INTO employee_threads(employee_id, thread_key, conversation_id, created_at_ms) VALUES
           ('{EMPLOYEE}', 'channel:{GENERAL}', '{CHANNEL_THREAD}', 1), ('{EMPLOYEE}', 'ticket:{TICKET}', '{TICKET_THREAD}', 1);
         INSERT INTO employee_inbox(id, employee_id, thread_key, conversation_id, kind, source, payload_json, source_event_id, not_before_ms, run_id, created_at_ms) VALUES
           ('i1', '{EMPLOYEE}', 'channel:{GENERAL}', '{CHANNEL_THREAD}', 'message', 'board', '{{}}', '{EVENT}', 1, 'rc', 1),
           ('i2', '{EMPLOYEE}', 'ticket:{TICKET}', '{TICKET_THREAD}', 'message', 'board', '{{}}', '{EVENT}', 1, 'rt', 1);
         INSERT INTO jobs(id, kind, subject_id, source_event_id, dedupe_key, payload_json, state, not_before_ms) VALUES
           ('rc', 'run.chat', '{CHANNEL_THREAD}', '{EVENT}', 'j1', '{{\"conversation_id\":\"{CHANNEL_THREAD}\",\"group\":{{\"conversation_id\":\"{GENERAL}\"}}}}', 'done', 1),
           ('rt', 'run.chat', '{TICKET_THREAD}', '{EVENT}', 'j2', '{{\"conversation_id\":\"{TICKET_THREAD}\",\"group\":{{\"conversation_id\":\"{DISCUSSION}\"}}}}', 'done', 1);
         INSERT INTO activity_log(at_ms, category, step, thread_key) VALUES
           (1, 'mailbox', 'delivered', 'channel:{GENERAL}'), (1, 'mailbox', 'delivered', 'chat:{BOARD}'),
           (1, 'mailbox', 'delivered', 'chat:{MAIN}'), (1, 'mailbox', 'delivered', 'ticket:{TICKET}');
         INSERT INTO scheduled_messages(id, from_employee_id, to_employee_id, content, chat_id, thread_key, place, planned_by_run, due_at_ms, created_at_ms)
           VALUES ('s1', '{EMPLOYEE}', '{EMPLOYEE}', 'later', '{BOARD}', 'chat:{BOARD}', 'chat', 'rb', 1, 1);",
    ))
    .unwrap();
}

#[test]
fn a_schema_44_store_loses_the_board_chats_and_the_team_chat_and_keeps_everything_else() {
    let dir = tempfile::tempdir().unwrap();
    schema_44_database(dir.path());
    assert_eq!(count(&db(dir.path()), "SELECT COUNT(*) FROM chat_tickets"), 1);

    let store = Store::open(dir.path()).unwrap();
    let conn = db(dir.path());
    assert_eq!(count(&conn, "SELECT MAX(version) FROM schema_migrations"), crate::migrate::supported_version());
    assert!(crate::migrate::supported_version() >= 45);

    // Kept: the main chat with its settings, the tickets and their discussions, the thread of a ticket, the session.
    let main: (String, i64, String) = conn.query_row("SELECT kind, version, agent_mode FROM conversations WHERE id = ?1", [MAIN], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap();
    assert_eq!(main, ("employee".into(), 4, "auto".into()));
    for kept in [MAIN, DISCUSSION, OTHER_DISCUSSION, TICKET_THREAD, SESSION] {
        assert_eq!(count(&conn, &format!("SELECT COUNT(*) FROM conversations WHERE id = '{kept}'")), 1, "{kept}");
    }
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM tickets"), 2);
    assert_eq!(count(&conn, &format!("SELECT COUNT(*) FROM work_sessions WHERE conversation_id = '{SESSION}'")), 1);
    assert_eq!(count(&conn, &format!("SELECT COUNT(*) FROM messages WHERE conversation_id IN ('{MAIN}', '{DISCUSSION}')")), 3);
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM runs WHERE id IN ('rm', 'rt')"), 2);
    assert_eq!(count(&conn, &format!("SELECT COUNT(*) FROM employee_threads WHERE thread_key = 'ticket:{TICKET}'")), 1);
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM employee_inbox WHERE id = 'i2'"), 1);
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM jobs WHERE id = 'rt'"), 1);
    assert_eq!(count(&conn, &format!("SELECT COUNT(*) FROM activity_log WHERE thread_key IN ('chat:{MAIN}', 'ticket:{TICKET}')")), 2);
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM message_reactions WHERE message_id = 'd1'"), 1);
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM thread_reads WHERE root_message_id = 'd1'"), 1);
    assert_eq!(count(&conn, &format!("SELECT COUNT(*) FROM discussion_reads WHERE conversation_id = '{DISCUSSION}'")), 1);
    assert_eq!(count(&conn, &format!("SELECT COUNT(*) FROM channel_members WHERE conversation_id = '{DISCUSSION}'")), 1);
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM notifications WHERE id = 'n2'"), 1);
    // A post in a discussion written by a gone run keeps its place, without the run, and its reply keeps its thread.
    let post: (Option<String>, Option<String>) = conn.query_row("SELECT run_id, thread_parent_id FROM messages WHERE id = 'd1'", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    assert_eq!(post, (None, None));
    let reply: Option<String> = conn.query_row("SELECT thread_parent_id FROM messages WHERE id = 'd2'", [], |r| r.get(0)).unwrap();
    assert_eq!(reply.as_deref(), Some("d1"));

    // Gone: the board member's chat, General, the channel and the thread of it, with everything that hung off them.
    for gone in [BOARD, GENERAL, CHANNEL, CHANNEL_THREAD] {
        assert_eq!(count(&conn, &format!("SELECT COUNT(*) FROM conversations WHERE id = '{gone}'")), 0, "{gone}");
    }
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM conversations WHERE kind = 'group' OR board_member_id IS NOT NULL"), 0, "no General is made again");
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM runs WHERE id IN ('rb', 'rc')"), 0);
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM usage_records") + count(&conn, "SELECT COUNT(*) FROM run_activity"), 0);
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM messages WHERE id IN ('b1', 'g1', 'g2')"), 0);
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM message_reactions WHERE message_id = 'g1'") + count(&conn, "SELECT COUNT(*) FROM thread_reads WHERE root_message_id = 'g1'"), 0);
    assert_eq!(count(&conn, &format!("SELECT COUNT(*) FROM discussion_reads WHERE conversation_id = '{GENERAL}'")), 0);
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM notifications WHERE id = 'n1'"), 0);
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM employee_threads WHERE thread_key LIKE 'channel:%'"), 0);
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM employee_inbox WHERE id = 'i1'"), 0);
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM jobs WHERE id = 'rc'"), 0);
    assert_eq!(count(&conn, &format!("SELECT COUNT(*) FROM activity_log WHERE thread_key LIKE 'channel:%' OR thread_key = 'chat:{BOARD}'")), 0);
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM scheduled_messages"), 0);
    assert!(conn.prepare("SELECT ticket_id FROM chat_tickets").is_err(), "the table is dropped");
    // The ticket made in the chat is still a ticket.
    assert_eq!(count(&conn, &format!("SELECT COUNT(*) FROM tickets WHERE id = '{TICKET}'")), 1);

    assert_eq!(count(&conn, "SELECT COUNT(*) FROM pragma_foreign_key_check"), 0);
    // Opening again changes nothing and makes no General.
    drop(store);
    drop(Store::open(dir.path()).unwrap());
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM conversations WHERE kind = 'group'"), 0);
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM conversations"), 5);
}

#[test]
fn a_fresh_store_has_no_group_chat_and_no_board_chats() {
    let dir = tempfile::tempdir().unwrap();
    drop(Store::open(dir.path()).unwrap());
    let conn = db(dir.path());
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM conversations"), 0);
    assert!(conn.prepare("SELECT ticket_id FROM chat_tickets").is_err());
}
