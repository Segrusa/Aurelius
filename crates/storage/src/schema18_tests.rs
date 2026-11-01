//! Migration 18: the company, its board members, the board and its tickets.

use crate::{Store, migrate};
use rusqlite::{Connection, params};

/// 0001–0017 were released: their bytes (and so their checksums) never change.
const RELEASED: [&str; 17] = [
    "bd66e2ba0366c467937abfe71cfde9fd726b31f24632318519c4fe55ea40a088",
    "d0ee867cd1dab9dae84c2aee4c936d666122065d727593e0e4a25fbaa3e7892f",
    "ba08220be6ead577a2f8bb2499e3b6adc114161ac175cdf1abe991ab9887702f",
    "cd19c38f658a2bc62181522068ea8fb8d1c02d853a4023d96997fb3f4722f02b",
    "f41d9c9fa3c4ebc46c9b6587faf866c9dd77cadae7ebee0cf34727b8ae7738dd",
    "c76814b64735ddbc431b080402d879f00c3272ccb0143c644d171d23c480023a",
    "d13598b0cd40d664f4e16a99cd53fffb6aff9f7081bf1acb56aca150aecf5f86",
    "c59e5024dd9a2a85b761fbafe97c8354fa93bed6cf76b53a3ae7b6ef8ec989a9",
    "6187b2aa33c0d81350326710078be9f1ac9e9324556c447282d622a68a0a1f7c",
    "c7a10b85f8663b93a44507c9d982ba63b1547dcdecc40475bafb9a79eb092c58",
    "50213cdac29d85bd3d3a38bd7c7252a41df298170d87e990f571f06d2863a8ab",
    "31ba2d8a5b4eab4c0d23709b958f248528fc781a85a521afc179100ebcf756c5",
    "4d21e8f8d99f3bf6f0822d79bc958b5126c683b47effc472a46da340fc2ace6c",
    "be5456228e31b9efa31ca2f7ffd1c90c0f88bf14ba352ac99677c6eaa49dc189",
    "416c7353b3d31cf0aaccb3f2ac1ddb16f9c7fad088e3dd950e25e6eb9e26ce07",
    "57f809626ad9b697fdba44df3fdeb5668c11946910cd5c866db5f18aa53178c4",
    "7bdfdaae2c7a1e00a5024145da79ed12dd4570d69c5b8ce09dfde91863433af9",
];

const P: &str = "00000000-0000-7000-8000-000000000001";
const T: &str = "00000000-0000-7000-8000-000000000002";
const B: &str = "00000000-0000-7000-8000-000000000003";
const U: &str = "00000000-0000-7000-8000-000000000004";

fn db(dir: &std::path::Path) -> Connection {
    let conn = Connection::open(dir.join("data/aurelius.sqlite")).unwrap();
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();
    conn
}

#[test]
fn the_released_migrations_are_unchanged() {
    for (m, sum) in migrate::MIGRATIONS.iter().zip(RELEASED) {
        assert_eq!(migrate::checksum(m.sql), sum, "migration {} changed", m.version);
    }
    let board = migrate::MIGRATIONS.iter().find(|m| m.version == 18).unwrap();
    assert!(board.rebuild);
    assert_eq!(migrate::LAST_PROTOCOL_2_VERSION, 13);
}

#[test]
fn a_new_database_has_one_company_with_six_columns() {
    let dir = tempfile::tempdir().unwrap();
    // Schema 18's own board; migration 25 adds Testing (schema25_tests).
    drop(Store::open_until(dir.path(), 18).unwrap());
    let conn = db(dir.path());
    let (company, scope_kind, name, dispatcher): (String, String, String, Option<String>) = conn
        .query_row(
            "SELECT c.id, s.kind, c.name, c.dispatcher_employee_id FROM company c JOIN scopes s ON s.id = c.scope_id",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!((scope_kind.as_str(), name.as_str(), dispatcher), ("global", "Company", None));
    let columns: Vec<(String, String, i64)> = conn
        .prepare("SELECT kind, name, position FROM board_columns WHERE company_id = ?1 ORDER BY position")
        .unwrap()
        .query_map([&company], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    let expected = [("backlog", "Backlog"), ("todo", "Todo"), ("doing", "Doing"), ("plan_review", "Plan review"), ("review", "Review"), ("done", "Done")];
    assert_eq!(columns.len(), 6);
    for (i, ((kind, name, position), (k, n))) in columns.iter().zip(expected).enumerate() {
        assert_eq!((kind.as_str(), name.as_str(), *position), (k, n, i as i64));
    }
    // One company per server; the kinds are fixed.
    let now = 1i64;
    let scope: String = conn.query_row("SELECT scope_id FROM company", [], |r| r.get(0)).unwrap();
    assert!(conn
        .execute("INSERT INTO company(id, scope_id, name, version, created_at_ms, updated_at_ms) VALUES ('c2', ?1, 'B', 1, ?2, ?2)", params![scope, now])
        .is_err());
    assert!(conn.execute("INSERT INTO board_columns VALUES ('x', ?1, 'blocked', 'Blocked', 6, 1)", [&company]).is_err());
    assert!(conn.execute("INSERT INTO board_columns VALUES ('x', ?1, 'todo', 'Again', 6, 1)", [&company]).is_err(), "one column per kind");
}

#[test]
fn a_ticket_needs_its_board_member() {
    let dir = tempfile::tempdir().unwrap();
    drop(Store::open_until(dir.path(), 18).unwrap());
    let conn = db(dir.path());
    let company: String = conn.query_row("SELECT id FROM company", [], |r| r.get(0)).unwrap();
    let backlog: String = conn.query_row("SELECT id FROM board_columns WHERE kind = 'backlog'", [], |r| r.get(0)).unwrap();
    let global: String = conn.query_row("SELECT id FROM scopes WHERE kind = 'global'", [], |r| r.get(0)).unwrap();
    conn.execute("INSERT INTO scopes(id, kind, parent_id, owner_id) VALUES ('ps', 'project', ?1, ?2)", params![global, P]).unwrap();
    conn.execute("INSERT INTO projects(id, scope_id, name, status, version, created_at_ms, updated_at_ms) VALUES (?1, 'ps', 'Alpha', 'active', 1, 1, 1)", [P]).unwrap();
    conn.execute(
        "INSERT INTO users(id, company_id, username, password_hash, created_at_ms, updated_at_ms) VALUES (?1, ?2, 'grace', '$argon2id$x', 1, 1)",
        params![U, company],
    )
    .unwrap();
    let ticket = |id: &str, by: Option<&str>, priority: Option<&str>| {
        conn.execute(
            &format!(
                "INSERT INTO tickets(id, company_id, project_id, number, title, column_id, position, created_by_user, version, created_at_ms, updated_at_ms{})
                 VALUES (?1, ?2, ?3, ?4, 'Fix it', ?5, 1.0, ?6, 1, 1, 1{})",
                if priority.is_some() { ", priority" } else { "" },
                if priority.is_some() { ", ?7" } else { "" }
            ),
            rusqlite::params_from_iter(
                [Some(id), Some(company.as_str()), Some(P), Some(if id == "t1" { "1" } else { "2" }), Some(backlog.as_str()), by]
                    .into_iter()
                    .chain(priority.map(Some)),
            ),
        )
    };
    assert!(ticket("t0", None, None).is_err(), "created_by_user is NOT NULL");
    assert!(ticket("t0", Some("nobody"), None).is_err(), "created_by_user references users");
    assert!(ticket("t0", Some(U), Some("asap")).is_err(), "priority is low, normal, high or urgent");
    ticket("t1", Some(U), None).unwrap();
    ticket("t2", Some(U), Some("urgent")).unwrap();
    let priority: String = conn.query_row("SELECT priority FROM tickets WHERE id = 't1'", [], |r| r.get(0)).unwrap();
    assert_eq!(priority, "normal");
    assert!(conn.execute("UPDATE tickets SET number = 1 WHERE id = 't2'", []).is_err(), "numbers are unique per company");
}

#[test]
fn tasks_keep_their_rows_and_gain_a_ticket_and_plan_review() {
    let dir = tempfile::tempdir().unwrap();
    drop(Store::open_until(dir.path(), 17).unwrap());
    {
        let conn = db(dir.path());
        let global: String = conn.query_row("SELECT id FROM scopes WHERE kind = 'global'", [], |r| r.get(0)).unwrap_or_else(|_| {
            conn.execute("INSERT INTO scopes(id, kind) VALUES ('gs', 'global')", []).unwrap();
            "gs".into()
        });
        conn.execute("INSERT INTO scopes(id, kind, parent_id, owner_id) VALUES ('ps', 'project', ?1, ?2)", params![global, P]).unwrap();
        conn.execute("INSERT INTO projects(id, scope_id, name, status, version, created_at_ms, updated_at_ms) VALUES (?1, 'ps', 'Alpha', 'active', 1, 1, 1)", [P]).unwrap();
        conn.execute("INSERT INTO blobs(id, storage_key, sha256, byte_length, media_type, created_at_ms) VALUES (?1, 'k', 's', 0, 'text/plain', 1)", [B]).unwrap();
        conn.execute(
            "INSERT INTO tasks(id, project_id, title, brief_blob_id, status, waiting_reason, version, created_at_ms, updated_at_ms)
             VALUES (?1, ?2, 'Old work', ?3, 'waiting', 'input', 3, 5, 6)",
            params![T, P, B],
        )
        .unwrap();
    }
    drop(Store::open(dir.path()).unwrap());
    let conn = db(dir.path());
    let (title, reason, version, ticket): (String, String, i64, Option<String>) = conn
        .query_row("SELECT title, waiting_reason, version, ticket_id FROM tasks WHERE id = ?1", [T], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap();
    assert_eq!((title.as_str(), reason.as_str(), version, ticket), ("Old work", "input", 3, None));
    conn.execute("UPDATE tasks SET waiting_reason = 'plan_review' WHERE id = ?1", [T]).unwrap();
    assert!(conn.execute("UPDATE tasks SET waiting_reason = 'thinking' WHERE id = ?1", [T]).is_err());
    assert!(conn.execute("UPDATE tasks SET ticket_id = 'no-such-ticket' WHERE id = ?1", [T]).is_err(), "ticket_id references tickets");
    let dispatcher: Option<String> = conn.query_row("SELECT dispatcher_employee_id FROM projects WHERE id = ?1", [P], |r| r.get(0)).unwrap();
    assert!(dispatcher.is_none());
    let indexes: i64 = conn
        .query_row("SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND tbl_name = 'tasks' AND name LIKE 'tasks_%'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(indexes, 6, "the five old indexes plus tasks_ticket");
    let broken: i64 = conn.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| r.get(0)).unwrap();
    assert_eq!(broken, 0);
}

#[test]
fn departments_are_gone_and_their_memory_is_the_companys() {
    let dir = tempfile::tempdir().unwrap();
    drop(Store::open_until(dir.path(), 17).unwrap());
    {
        let conn = db(dir.path());
        conn.execute_batch(
            "BEGIN; PRAGMA defer_foreign_keys = ON;
             INSERT INTO scopes(id, kind) SELECT 'gs', 'global' WHERE NOT EXISTS (SELECT 1 FROM scopes WHERE kind = 'global');
             INSERT INTO scopes(id, kind, parent_id, owner_id) VALUES ('ds', 'department', (SELECT id FROM scopes WHERE kind = 'global'), 'd1');
             INSERT INTO scopes(id, kind, parent_id, owner_id) VALUES ('es', 'employee', (SELECT id FROM scopes WHERE kind = 'global'), 'e1');
             INSERT INTO departments(id, scope_id, name, status, version, created_at_ms, updated_at_ms) VALUES ('d1', 'ds', 'Engineering', 'active', 1, 1, 1);
             INSERT INTO employees(id, scope_id, name, role, config_json, status, version, created_at_ms, updated_at_ms, department_id)
               VALUES ('e1', 'es', 'Ada', 'engineer', '{}', 'active', 1, 1, 1, 'd1');
             INSERT INTO blobs(id, storage_key, sha256, byte_length, media_type, created_at_ms) VALUES ('b1', 'k', 's', 0, 'text/plain', 1);
             INSERT INTO memory_items(id, scope_id, kind, content_blob_id, status, confidence, importance, valid_from_ms, extraction_version, created_at_ms, locator_json)
               VALUES ('m1', 'ds', 'decision', 'b1', 'active', 0.5, 0.5, 1, 'v1', 1, '{\"task\":\"t\"}');
             INSERT INTO events(id, aggregate_type, aggregate_id, aggregate_version, type, schema_version, actor_kind, correlation_id, occurred_at_ms, recorded_at_ms, dedupe_key)
               VALUES ('ev', 'x', 'x', 1, 'project.created', 1, 'user', 'c', 1, 1, 'd');
             INSERT INTO config_overrides(scope_id, purpose, version, config_json, source_event_id) VALUES ('ds', 'run', 1, '{}', 'ev');
             COMMIT;",
        )
        .unwrap();
    }
    drop(Store::open(dir.path()).unwrap());
    let conn = db(dir.path());
    let objects: i64 = conn.query_row("SELECT COUNT(*) FROM sqlite_master WHERE name LIKE '%department%'", [], |r| r.get(0)).unwrap();
    assert_eq!(objects, 0, "no departments table or index");
    for table in ["employees", "proposals"] {
        let has: i64 = conn.query_row(&format!("SELECT COUNT(*) FROM pragma_table_info('{table}') WHERE name = 'department_id'"), [], |r| r.get(0)).unwrap();
        assert_eq!(has, 0, "{table}.department_id");
    }
    let name: String = conn.query_row("SELECT name FROM employees WHERE id = 'e1'", [], |r| r.get(0)).unwrap();
    assert_eq!(name, "Ada");
    let (kind, locator): (String, String) =
        conn.query_row("SELECT s.kind, m.locator_json FROM memory_items m JOIN scopes s ON s.id = m.scope_id WHERE m.id = 'm1'", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    let locator: serde_json::Value = serde_json::from_str(&locator).unwrap();
    assert_eq!((kind.as_str(), locator["from_department"].as_str(), locator["task"].as_str()), ("global", Some("Engineering"), Some("t")));
    let overrides: i64 = conn.query_row("SELECT COUNT(*) FROM config_overrides", [], |r| r.get(0)).unwrap();
    assert_eq!(overrides, 0, "a department's run defaults go");
    assert!(conn.execute("INSERT INTO scopes(id, kind, parent_id, owner_id) VALUES ('x', 'department', 'es', 'd2')", []).is_err());
    let broken: i64 = conn.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| r.get(0)).unwrap();
    assert_eq!(broken, 0);
}
