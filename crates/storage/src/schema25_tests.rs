//! Migration 25: the Testing column (ticket #5). A schema 23 board gains a
//! seventh column just before Review; every column keeps its id and name, every
//! card its column, and the company gains an empty tester.

use crate::{Store, migrate};
use rusqlite::{Connection, params};

/// 0023 is on main: its bytes never change.
const RELEASED: [(i64, &str); 1] = [(23, "1ca8661be176db94067c6b766895e81b26d96743ab997fccec152dd47a1a173b")];

const P: &str = "00000000-0000-7000-8000-000000000001";
const B: &str = "00000000-0000-7000-8000-000000000002";
const ALEX: &str = "00000000-0000-7000-8000-000000000003";
const E1: &str = "00000000-0000-7000-8000-000000000004";
const T1: &str = "00000000-0000-7000-8000-000000000011";
const T2: &str = "00000000-0000-7000-8000-000000000012";
const TASK: &str = "00000000-0000-7000-8000-000000000021";

fn db(dir: &std::path::Path) -> Connection {
    let conn = Connection::open(dir.join("data/aurelius.sqlite")).unwrap();
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();
    conn
}

fn strings(conn: &Connection, sql: &str) -> Vec<String> {
    conn.prepare(sql).unwrap().query_map([], |r| r.get(0)).unwrap().collect::<Result<_, _>>().unwrap()
}

fn column(conn: &Connection, kind: &str) -> String {
    conn.query_row("SELECT id FROM board_columns WHERE kind = ?1", [kind], |r| r.get(0)).unwrap()
}

#[test]
fn migration_23_is_unchanged_and_25_rebuilds() {
    for (version, sum) in RELEASED {
        let m = migrate::MIGRATIONS.iter().find(|m| m.version == version).unwrap();
        assert_eq!(migrate::checksum(m.sql), sum, "migration {version} changed");
    }
    let m25 = migrate::MIGRATIONS.iter().find(|m| m.version == 25).unwrap();
    assert!(m25.rebuild && m25.then.is_none());
    assert_eq!(migrate::supported_version(), 25);
}

/// A schema 23 database with a board member, a project, an employee, a renamed
/// Review column and two cards: one in Review with its main task, one in Doing.
fn schema_23_database(dir: &std::path::Path) {
    drop(Store::open_until(dir, 23).unwrap());
    let conn = db(dir);
    let company: String = conn.query_row("SELECT id FROM company", [], |r| r.get(0)).unwrap();
    let global: String = conn.query_row("SELECT id FROM scopes WHERE kind = 'global'", [], |r| r.get(0)).unwrap();
    let (review, doing) = (column(&conn, "review"), column(&conn, "doing"));
    conn.execute_batch(&format!(
        "INSERT INTO scopes(id, kind, parent_id, owner_id) VALUES ('ps', 'project', '{global}', '{P}');
         INSERT INTO projects(id, scope_id, name, status, version, created_at_ms, updated_at_ms) VALUES ('{P}', 'ps', 'Alpha', 'active', 1, 1, 1);
         INSERT INTO scopes(id, kind, parent_id, owner_id) VALUES ('es', 'employee', '{global}', '{E1}');
         INSERT INTO employees(id, scope_id, name, role, config_json, status, version, created_at_ms, updated_at_ms)
           VALUES ('{E1}', 'es', 'Pulse', 'QA', '{{}}', 'active', 1, 1, 1);
         INSERT INTO blobs(id, storage_key, sha256, byte_length, media_type, created_at_ms) VALUES ('{B}', 'k', 's', 0, 'text/plain', 1);
         INSERT INTO users(id, company_id, username, password_hash, created_at_ms, updated_at_ms) VALUES ('{ALEX}', '{company}', 'Alex', '$argon2id$x', 1, 1);
         UPDATE board_columns SET name = 'Check', version = 3 WHERE kind = 'review';
         INSERT INTO conversations(id, kind, project_id, title, version, created_at_ms) VALUES
           ('c1', 'ticket', '{P}', '#1 Fix the login', 1, 10), ('c2', 'ticket', '{P}', '#2 Write the docs', 1, 20);
         INSERT INTO tickets(id, company_id, project_id, number, title, column_id, position, priority, created_by_user,
                             version, created_at_ms, updated_at_ms, conversation_id, login_user) VALUES
           ('{T1}', '{company}', '{P}', 1, 'Fix the login', '{review}', 1.0, 'high', '{ALEX}', 4, 10, 11, 'c1', '{ALEX}'),
           ('{T2}', '{company}', '{P}', 2, 'Write the docs', '{doing}', 1.0, 'normal', '{ALEX}', 2, 20, 21, 'c2', '{ALEX}');
         INSERT INTO tasks(id, project_id, employee_id, title, brief_blob_id, status, version, created_at_ms, updated_at_ms, ticket_id)
           VALUES ('{TASK}', '{P}', '{E1}', 'Fix the login', '{B}', 'review', 3, 5, 6, '{T1}');
         UPDATE tickets SET root_task_id = '{TASK}' WHERE id = '{T1}';"
    ))
    .unwrap();
}

const COLUMNS: &str = "SELECT kind || ' ' || name || ' ' || position FROM board_columns ORDER BY position";
const TICKET_ROW: &str = "SELECT k.id || ' ' || k.column_id || ' ' || c.kind || ' ' || k.position || ' ' || k.title || ' ' || COALESCE(k.root_task_id, '-') || ' ' || k.version
    FROM tickets k JOIN board_columns c ON c.id = k.column_id ORDER BY k.id";

#[test]
fn a_schema_23_board_gains_testing_before_review_and_keeps_its_cards_and_names() {
    let dir = tempfile::tempdir().unwrap();
    schema_23_database(dir.path());
    let (ids_before, tickets_before) = {
        let conn = db(dir.path());
        (strings(&conn, "SELECT kind || ' ' || id FROM board_columns ORDER BY kind"), strings(&conn, TICKET_ROW))
    };
    assert_eq!(ids_before.len(), 6);

    drop(Store::open(dir.path()).unwrap());
    let conn = db(dir.path());
    assert_eq!(conn.query_row("SELECT MAX(version) FROM schema_migrations", [], |r| r.get::<_, i64>(0)).unwrap(), 25);
    assert!(strings(&conn, "SELECT \"table\" FROM pragma_foreign_key_check").is_empty());
    assert_eq!(
        strings(&conn, COLUMNS),
        ["backlog Backlog 0", "todo Todo 1", "doing Doing 2", "plan_review Plan review 3", "testing Testing 4", "review Check 5", "done Done 6"]
    );
    // Every old column keeps its id (so every card its column); Testing is new.
    let ids_after = strings(&conn, "SELECT kind || ' ' || id FROM board_columns WHERE kind <> 'testing' ORDER BY kind");
    assert_eq!(ids_after, ids_before);
    assert_eq!(strings(&conn, TICKET_ROW), tickets_before);
    // The moved columns count up; Testing takes the board version (the highest), so a client's next edit conflicts.
    let versions = strings(&conn, "SELECT kind || ' ' || version FROM board_columns WHERE kind IN ('review', 'done', 'testing', 'backlog') ORDER BY position");
    assert_eq!(versions, ["backlog 1", "testing 4", "review 4", "done 2"]);
    // No tester yet, no test sub-task yet.
    let tester: Option<String> = conn.query_row("SELECT tester_employee_id FROM company", [], |r| r.get(0)).unwrap();
    assert_eq!(tester, None);
    assert_eq!(strings(&conn, "SELECT id FROM tickets WHERE test_task_id IS NOT NULL"), Vec::<String>::new());
    let strict: i64 = conn.query_row("SELECT strict FROM pragma_table_list WHERE name = 'board_columns'", [], |r| r.get(0)).unwrap();
    assert_eq!(strict, 1);
}

#[test]
fn a_reordered_board_gets_testing_right_before_review() {
    let dir = tempfile::tempdir().unwrap();
    schema_23_database(dir.path());
    // Review was put before Plan review by a board member.
    db(dir.path())
        .execute_batch("UPDATE board_columns SET position = 3 WHERE kind = 'review'; UPDATE board_columns SET position = 4 WHERE kind = 'plan_review';")
        .unwrap();
    drop(Store::open(dir.path()).unwrap());
    let conn = db(dir.path());
    assert_eq!(
        strings(&conn, COLUMNS),
        ["backlog Backlog 0", "todo Todo 1", "doing Doing 2", "testing Testing 3", "review Check 4", "plan_review Plan review 5", "done Done 6"]
    );
}

#[test]
fn the_kind_testing_once_and_the_tester_and_test_task_references_are_checked() {
    let dir = tempfile::tempdir().unwrap();
    schema_23_database(dir.path());
    drop(Store::open(dir.path()).unwrap());
    let conn = db(dir.path());
    let company: String = conn.query_row("SELECT id FROM company", [], |r| r.get(0)).unwrap();
    let insert = |kind: &str| {
        conn.execute("INSERT INTO board_columns(id, company_id, kind, name, position, version) VALUES (?1, ?2, ?3, 'X', 9, 1)", params![format!("x-{kind}"), company, kind])
    };
    assert!(insert("testing").is_err(), "Testing exists once");
    assert!(insert("qa").is_err(), "an unknown kind");

    conn.execute("UPDATE company SET tester_employee_id = ?1", [E1]).unwrap();
    assert!(conn.execute("UPDATE company SET tester_employee_id = 'nobody'", []).is_err(), "the tester is an employee");
    conn.execute("UPDATE tickets SET test_task_id = ?1 WHERE id = ?2", [TASK, T1]).unwrap();
    assert!(conn.execute("UPDATE tickets SET test_task_id = 'no-task' WHERE id = ?1", [T2]).is_err(), "the test task is a task");
}
