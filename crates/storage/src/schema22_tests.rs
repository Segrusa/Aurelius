//! Migration 22: phase 4's vault. A phase 3 database keeps every row and gains
//! three empty tables.

use crate::vault::{self, AuditAction, NewAudit, NewVaultItem, Sealed, VaultKind};
use crate::{Store, migrate};
use rusqlite::{Connection, params};

/// 0021 was released with phase 3: its bytes never change.
const RELEASED: [(i64, &str); 1] = [(21, "2d90992dbe8083f2d4622d78504150675134f83d4f204fd0611bdff4e9d64979")];

const P: &str = "00000000-0000-7000-8000-000000000001";
const B: &str = "00000000-0000-7000-8000-000000000002";
const ALEX: &str = "00000000-0000-7000-8000-000000000003";
const T1: &str = "00000000-0000-7000-8000-000000000011";
const TASK: &str = "00000000-0000-7000-8000-000000000021";
const RUN: &str = "00000000-0000-7000-8000-000000000031";

fn db(dir: &std::path::Path) -> Connection {
    let conn = Connection::open(dir.join("data/aurelius.sqlite")).unwrap();
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();
    conn
}

fn strings(conn: &Connection, sql: &str) -> Vec<String> {
    conn.prepare(sql).unwrap().query_map([], |r| r.get(0)).unwrap().collect::<Result<_, _>>().unwrap()
}

#[test]
fn phase_3_migrations_are_unchanged_and_22_is_additive() {
    for (version, sum) in RELEASED {
        let m = migrate::MIGRATIONS.iter().find(|m| m.version == version).unwrap();
        assert_eq!(migrate::checksum(m.sql), sum, "migration {version} changed");
    }
    let vault = migrate::MIGRATIONS.iter().find(|m| m.version == 22).unwrap();
    assert!(!vault.rebuild);
}

/// A schema 21 database with a board member and their session and Claude
/// login, a project, a ticket with a waiting task and its run.
fn phase_3_database(dir: &std::path::Path) {
    drop(Store::open_until(dir, 21).unwrap());
    let conn = db(dir);
    let company: String = conn.query_row("SELECT id FROM company", [], |r| r.get(0)).unwrap();
    let global: String = conn.query_row("SELECT id FROM scopes WHERE kind = 'global'", [], |r| r.get(0)).unwrap();
    let backlog: String = conn.query_row("SELECT id FROM board_columns WHERE kind = 'backlog'", [], |r| r.get(0)).unwrap();
    conn.execute_batch(&format!(
        "INSERT INTO scopes(id, kind, parent_id, owner_id) VALUES ('ps', 'project', '{global}', '{P}');
         INSERT INTO projects(id, scope_id, name, status, version, created_at_ms, updated_at_ms) VALUES ('{P}', 'ps', 'Alpha', 'active', 1, 1, 1);
         INSERT INTO blobs(id, storage_key, sha256, byte_length, media_type, created_at_ms) VALUES ('{B}', 'k', 's', 0, 'text/plain', 1);
         INSERT INTO users(id, company_id, username, password_hash, created_at_ms, updated_at_ms, password_changed_at_ms)
           VALUES ('{ALEX}', '{company}', 'Alex', '$argon2id$x', 1, 1, 5);
         INSERT INTO user_sessions(id, user_id, device_name, token_verifier, created_at_ms, last_seen_ms, expires_at_ms)
           VALUES ('s1', '{ALEX}', 'Desk', 'v1', 1, 2, 3);
         INSERT INTO claude_logins(user_id, status, email, checked_at_ms) VALUES ('{ALEX}', 'linked', 'test@example.com', 4);
         INSERT INTO conversations(id, kind, project_id, title, version, created_at_ms) VALUES ('c1', 'ticket', '{P}', '#1 Fix the login', 1, 10);
         INSERT INTO tickets(id, company_id, project_id, number, title, column_id, position, priority, created_by_user, version,
                             created_at_ms, updated_at_ms, conversation_id, login_user)
           VALUES ('{T1}', '{company}', '{P}', 1, 'Fix the login', '{backlog}', 1.0, 'high', '{ALEX}', 3, 10, 11, 'c1', '{ALEX}');
         INSERT INTO tasks(id, project_id, title, brief_blob_id, status, waiting_reason, version, created_at_ms, updated_at_ms, ticket_id, login_user)
           VALUES ('{TASK}', '{P}', 'Old work', '{B}', 'waiting', 'login', 3, 5, 6, '{T1}', '{ALEX}');
         UPDATE tickets SET root_task_id = '{TASK}' WHERE id = '{T1}';
         INSERT INTO runs(id, task_id, attempt, runtime_kind, model_id, resolved_config_json, status, login_user)
           VALUES ('{RUN}', '{TASK}', 1, 'claude_code', 'm', '{{}}', 'succeeded', '{ALEX}');"
    ))
    .unwrap();
}

/// Every row of the tables phase 3 data lives in, as text.
fn snapshot(conn: &Connection) -> Vec<String> {
    let mut all = Vec::new();
    for table in ["users", "user_sessions", "claude_logins", "projects", "tickets", "tasks", "runs", "conversations", "company"] {
        let columns = strings(conn, &format!("SELECT name FROM pragma_table_info('{table}') ORDER BY cid"));
        let row = columns.iter().map(|c| format!("COALESCE(quote(\"{c}\"), 'NULL')")).collect::<Vec<_>>().join(" || '|' || ");
        all.extend(strings(conn, &format!("SELECT '{table}:' || {row} FROM {table} ORDER BY 1")));
    }
    all
}

#[test]
fn a_phase_3_database_migrates_without_losing_anything() {
    let dir = tempfile::tempdir().unwrap();
    phase_3_database(dir.path());
    let before = snapshot(&db(dir.path()));
    assert!(before.iter().any(|r| r.starts_with("tickets:")) && before.iter().any(|r| r.starts_with("user_sessions:")));

    let store = Store::open(dir.path()).unwrap();
    {
        let conn = db(dir.path());
        assert_eq!(conn.query_row("SELECT MAX(version) FROM schema_migrations", [], |r| r.get::<_, i64>(0)).unwrap(), migrate::supported_version());
        assert!(strings(&conn, "SELECT \"table\" FROM pragma_foreign_key_check").is_empty());
        assert_eq!(snapshot(&conn), before);
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM vault_items", [], |r| r.get::<_, i64>(0)).unwrap(), 0);
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM vault_audit", [], |r| r.get::<_, i64>(0)).unwrap(), 0);
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM vault_item_versions", [], |r| r.get::<_, i64>(0)).unwrap(), 0);
        assert!(!vault::any_ciphertext(&conn).unwrap());
        let indexes = strings(&conn, "SELECT name FROM sqlite_master WHERE type = 'index' AND name LIKE 'vault_%' ORDER BY name");
        assert_eq!(
            indexes,
            ["vault_audit_at", "vault_audit_employee", "vault_audit_item", "vault_audit_scope", "vault_audit_user",
             "vault_item_versions_item", "vault_item_versions_key", "vault_items_env_name", "vault_items_key", "vault_items_name",
             "vault_items_scope"]
        );
        let triggers = strings(&conn, "SELECT name FROM sqlite_master WHERE type = 'trigger' AND name LIKE 'vault_%' ORDER BY name");
        assert_eq!(
            triggers,
            ["vault_audit_no_delete", "vault_audit_no_update", "vault_item_versions_fixed", "vault_item_versions_no_delete",
             "vault_items_kind_secret", "vault_items_no_delete", "vault_items_scope_fixed"]
        );
        // E1: no vault value ever reaches full-text search: no FTS table or
        // trigger that feeds one mentions a vault table.
        let fts = strings(&conn, "SELECT sql FROM sqlite_master WHERE lower(sql) LIKE '%fts%'");
        assert!(!fts.is_empty());
        for sql in fts {
            assert!(!sql.to_lowercase().contains("vault"), "an FTS object reads the vault: {sql}");
        }
    }

    // The existing board member and project can use the vault at once.
    store
        .write(|tx| {
            vault::insert(
                tx,
                NewVaultItem {
                    id: "v1",
                    project_id: Some(P),
                    kind: VaultKind::Secret,
                    name: "Deploy key",
                    env_name: Some("DEPLOY_KEY"),
                    description: None,
                    sealed: Sealed { ciphertext: vec![1, 2, 3], nonce: vec![0; 12], key_id: "k1".into() },
                    by_user: ALEX,
                    now_ms: 20,
                },
            )?;
            vault::insert_audit(
                tx,
                &NewAudit { employee_id: Some("e1"), run_id: Some(RUN), task_id: Some(TASK), ticket_id: Some(T1), ..NewAudit::new("a1", Some("v1"), AuditAction::Deliver, 21) },
            )?;
            Ok(())
        })
        .unwrap();
    let set = store.read(|c| vault::effective(c, Some(P))).unwrap();
    assert_eq!(set.len(), 1);
    drop(store);
    let conn = db(dir.path());
    assert!(strings(&conn, "SELECT \"table\" FROM pragma_foreign_key_check").is_empty());
    assert!(conn.execute("INSERT INTO vault_items(id, company_id, kind, name, env_name, ciphertext, nonce, key_id, version, created_by_user, updated_by_user, created_at_ms, updated_at_ms)
                          SELECT 'v2', id, 'text', 'x', NULL, x'01', x'00', 'k1', 1, 'nobody', 'nobody', 1, 1 FROM company", params![]).is_err(),
            "created_by_user references users");
}
