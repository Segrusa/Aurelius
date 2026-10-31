//! Migration 21: phase 3's logins. A phase 2 database keeps every row; each
//! ticket runs on its creator's login; a task can wait with 'login'.

use crate::{Store, migrate};
use rusqlite::{Connection, params};

/// 0019 and 0020 were released with phase 2: their bytes never change.
const RELEASED: [(i64, &str); 2] = [
    (19, "3ac2ca10b36d3ac7e859b980322d99d47e0e6605bf61d8dd9fc7a68018410d03"),
    (20, "fc034d22c00f828f0a9d859f841b457663ecaa26ea005b16482df974666a6abb"),
];

const P: &str = "00000000-0000-7000-8000-000000000001";
const B: &str = "00000000-0000-7000-8000-000000000002";
const ALEX: &str = "00000000-0000-7000-8000-000000000003";
const ROBIN: &str = "00000000-0000-7000-8000-000000000004";
const T1: &str = "00000000-0000-7000-8000-000000000011";
const T2: &str = "00000000-0000-7000-8000-000000000012";
const TASK: &str = "00000000-0000-7000-8000-000000000021";
const DONE: &str = "00000000-0000-7000-8000-000000000022";
const RUN: &str = "00000000-0000-7000-8000-000000000031";
const PHC: &str = "$argon2id$x";

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

#[test]
fn phase_2_migrations_are_unchanged_and_21_rebuilds() {
    for (version, sum) in RELEASED {
        let m = migrate::MIGRATIONS.iter().find(|m| m.version == version).unwrap();
        assert_eq!(migrate::checksum(m.sql), sum, "migration {version} changed");
    }
    let last = migrate::MIGRATIONS.last().unwrap();
    assert_eq!((last.version, last.rebuild), (21, true));
}

/// A schema 20 database with two board members, two tickets (one each), a
/// waiting task on the first ticket, a finished one and a run.
fn phase_2_database(dir: &std::path::Path) {
    drop(Store::open_until(dir, 20).unwrap());
    let conn = db(dir);
    let company: String = conn.query_row("SELECT id FROM company", [], |r| r.get(0)).unwrap();
    let global: String = conn.query_row("SELECT id FROM scopes WHERE kind = 'global'", [], |r| r.get(0)).unwrap();
    let backlog: String = conn.query_row("SELECT id FROM board_columns WHERE kind = 'backlog'", [], |r| r.get(0)).unwrap();
    conn.execute_batch(&format!(
        "INSERT INTO scopes(id, kind, parent_id, owner_id) VALUES ('ps', 'project', '{global}', '{P}');
         INSERT INTO projects(id, scope_id, name, status, version, created_at_ms, updated_at_ms) VALUES ('{P}', 'ps', 'Alpha', 'active', 1, 1, 1);
         INSERT INTO blobs(id, storage_key, sha256, byte_length, media_type, created_at_ms) VALUES ('{B}', 'k', 's', 0, 'text/plain', 1);
         INSERT INTO users(id, company_id, username, password_hash, created_at_ms, updated_at_ms) VALUES
           ('{ALEX}', '{company}', 'Alex', '{PHC}', 1, 1), ('{ROBIN}', '{company}', 'Robin', '{PHC}', 2, 2);
         INSERT INTO conversations(id, kind, project_id, title, version, created_at_ms) VALUES
           ('c1', 'ticket', '{P}', '#1 Fix the login', 1, 10), ('c2', 'ticket', '{P}', '#2 Write the docs', 1, 20);
         INSERT INTO tickets(id, company_id, project_id, number, title, column_id, position, priority, created_by_user, labels_json,
                             version, created_at_ms, updated_at_ms, conversation_id) VALUES
           ('{T1}', '{company}', '{P}', 1, 'Fix the login', '{backlog}', 1.0, 'high', '{ALEX}', '[\"ui\"]', 3, 10, 11, 'c1'),
           ('{T2}', '{company}', '{P}', 2, 'Write the docs', '{backlog}', 2.0, 'normal', '{ROBIN}', NULL, 5, 20, 21, 'c2');
         INSERT INTO tasks(id, project_id, title, brief_blob_id, status, waiting_reason, version, created_at_ms, updated_at_ms, ticket_id) VALUES
           ('{TASK}', '{P}', 'Old work', '{B}', 'waiting', 'quota', 3, 5, 6, '{T1}'),
           ('{DONE}', '{P}', 'Done work', '{B}', 'succeeded', NULL, 2, 7, 8, NULL);
         UPDATE tickets SET root_task_id = '{TASK}' WHERE id = '{T1}';
         INSERT INTO runs(id, task_id, attempt, runtime_kind, model_id, resolved_config_json, status, error_code)
           VALUES ('{RUN}', '{TASK}', 1, 'claude_code', 'm', '{{}}', 'waiting', 'quota');"
    ))
    .unwrap();
}

const TICKET_ROW: &str = "SELECT id || ' ' || project_id || ' ' || number || ' ' || title || ' ' || column_id || ' ' || position || ' ' || priority || ' '
    || created_by_user || ' ' || COALESCE(labels_json, '-') || ' ' || COALESCE(root_task_id, '-') || ' ' || version || ' ' || created_at_ms || ' '
    || updated_at_ms || ' ' || conversation_id FROM tickets ORDER BY id";
const TASK_ROW: &str = "SELECT id || ' ' || title || ' ' || status || ' ' || COALESCE(waiting_reason, '-') || ' ' || version || ' ' || created_at_ms || ' '
    || updated_at_ms || ' ' || COALESCE(ticket_id, '-') FROM tasks ORDER BY id";

#[test]
fn a_phase_2_database_migrates_without_losing_anything() {
    let dir = tempfile::tempdir().unwrap();
    phase_2_database(dir.path());
    let (tickets_before, tasks_before) = {
        let conn = db(dir.path());
        (strings(&conn, TICKET_ROW), strings(&conn, TASK_ROW))
    };
    assert_eq!((tickets_before.len(), tasks_before.len()), (2, 2));

    drop(Store::open(dir.path()).unwrap());
    let conn = db(dir.path());
    assert_eq!(count(&conn, "SELECT MAX(version) FROM schema_migrations"), 21);
    assert!(strings(&conn, "SELECT \"table\" FROM pragma_foreign_key_check").is_empty());
    assert_eq!(strings(&conn, TICKET_ROW), tickets_before);
    assert_eq!(strings(&conn, TASK_ROW), tasks_before);
    let run: (String, Option<String>, Option<String>) =
        conn.query_row("SELECT status, error_code, login_user FROM runs WHERE id = ?1", [RUN], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap();
    assert_eq!(run, ("waiting".into(), Some("quota".into()), None), "an old run has no login");

    // Each ticket runs on its creator's login.
    let logins = strings(&conn, "SELECT id || ' ' || login_user FROM tickets ORDER BY id");
    assert_eq!(logins, [format!("{T1} {ALEX}"), format!("{T2} {ROBIN}")]);
    // The new user columns start empty: nobody disabled, no temporary password.
    let users = strings(
        &conn,
        "SELECT username || ' ' || COALESCE(disabled_at_ms, '-') || ' ' || COALESCE(password_changed_at_ms, '-') || ' ' || must_change_password
         FROM users ORDER BY created_at_ms",
    );
    assert_eq!(users, ["Alex - - 0", "Robin - - 0"]);
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM company WHERE background_login_user IS NULL"), 1);
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM user_sessions") + count(&conn, "SELECT COUNT(*) FROM claude_logins"), 0);

    // The indexes are back, plus the new ones.
    let tasks = strings(&conn, "SELECT name FROM sqlite_master WHERE type = 'index' AND tbl_name = 'tasks' AND name LIKE 'tasks_%' ORDER BY name");
    assert_eq!(tasks, ["tasks_app_key", "tasks_employee", "tasks_parent", "tasks_project_status", "tasks_schedule", "tasks_ticket"]);
    let tickets = strings(&conn, "SELECT name FROM sqlite_master WHERE type = 'index' AND tbl_name = 'tickets' AND name LIKE 'tickets_%' ORDER BY name");
    assert_eq!(tickets, ["tickets_column", "tickets_login_user", "tickets_project", "tickets_root_task"]);
    let sessions = strings(&conn, "SELECT name FROM sqlite_master WHERE type = 'index' AND tbl_name = 'user_sessions' AND name LIKE 'user_sessions_%' ORDER BY name");
    assert_eq!(sessions, ["user_sessions_client", "user_sessions_user"]);
}

#[test]
fn the_waiting_reason_login_and_the_tickets_login_are_checked() {
    let dir = tempfile::tempdir().unwrap();
    phase_2_database(dir.path());
    drop(Store::open(dir.path()).unwrap());
    let conn = db(dir.path());

    conn.execute("UPDATE tasks SET waiting_reason = 'login' WHERE id = ?1", [TASK]).unwrap();
    for reason in ["approval", "input", "dependency", "quota", "offline", "paused", "reconcile", "plan_review"] {
        conn.execute("UPDATE tasks SET waiting_reason = ?2 WHERE id = ?1", params![TASK, reason]).unwrap();
    }
    assert!(conn.execute("UPDATE tasks SET waiting_reason = 'auth' WHERE id = ?1", [TASK]).is_err());
    assert!(conn.execute("UPDATE tasks SET waiting_reason = 'login' WHERE id = ?1", [DONE]).is_err(), "only a waiting task has a reason");
    assert!(conn.execute("UPDATE tasks SET ticket_id = 'no-such-ticket' WHERE id = ?1", [TASK]).is_err(), "ticket_id references tickets");

    conn.execute("UPDATE tickets SET login_user = ?2 WHERE id = ?1", [T1, ROBIN]).unwrap();
    assert!(conn.execute("UPDATE tickets SET login_user = NULL WHERE id = ?1", [T1]).is_err(), "login_user is NOT NULL");
    assert!(conn.execute("UPDATE tickets SET login_user = 'nobody' WHERE id = ?1", [T1]).is_err(), "login_user references users");
    let company: String = conn.query_row("SELECT id FROM company", [], |r| r.get(0)).unwrap();
    let backlog: String = conn.query_row("SELECT id FROM board_columns WHERE kind = 'backlog'", [], |r| r.get(0)).unwrap();
    conn.execute("INSERT INTO conversations(id, kind, project_id, title, version, created_at_ms) VALUES ('c3', 'ticket', ?1, '#3', 1, 1)", [P]).unwrap();
    let without = conn.execute(
        "INSERT INTO tickets(id, company_id, project_id, number, title, column_id, position, created_by_user, version, created_at_ms, updated_at_ms, conversation_id)
         VALUES ('t3', ?1, ?2, 3, 'New', ?3, 3.0, ?4, 1, 1, 1, 'c3')",
        params![company, P, backlog, ALEX],
    );
    assert!(without.is_err(), "a new ticket names its login");

    // A session belongs to a board member, and to a device or the local IPC.
    conn.execute(
        "INSERT INTO user_sessions(id, user_id, device_name, token_verifier, created_at_ms, last_seen_ms, expires_at_ms) VALUES ('s1', ?1, 'Desk', 'v1', 1, 1, 2)",
        [ALEX],
    )
    .unwrap();
    assert!(conn
        .execute("INSERT INTO user_sessions(id, user_id, device_name, token_verifier, created_at_ms, last_seen_ms, expires_at_ms) VALUES ('s2', ?1, 'Desk', 'v1', 1, 1, 2)", [ROBIN])
        .is_err(), "the verifier is unique");
    assert!(conn
        .execute("INSERT INTO user_sessions(id, user_id, client_id, device_name, token_verifier, created_at_ms, last_seen_ms, expires_at_ms) VALUES ('s3', ?1, 'no-device', 'Phone', 'v3', 1, 1, 2)", [ROBIN])
        .is_err(), "client_id references remote_clients");
    assert!(conn
        .execute("INSERT INTO user_sessions(id, user_id, device_name, token_verifier, created_at_ms, last_seen_ms, expires_at_ms) VALUES ('s4', 'nobody', 'Desk', 'v4', 1, 1, 2)", [])
        .is_err(), "user_id references users");
    assert!(conn.execute("INSERT INTO claude_logins(user_id, status, checked_at_ms) VALUES (?1, 'unknown', 1)", [ALEX]).is_err());
    conn.execute("INSERT INTO claude_logins(user_id, status, email, checked_at_ms) VALUES (?1, 'linked', 'test@example.com', 1)", [ALEX]).unwrap();
    assert!(strings(&conn, "SELECT \"table\" FROM pragma_foreign_key_check").is_empty());
}
