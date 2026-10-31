//! The fresh start of v0.11, on a protocol 3 workspace with a row in every table.

use crate::fresh_start::{self, BACKUP_DIR, CONFIG_FILES, DECISIONS, DRY_RUN_PREFIX, Decision, FreshStartError, IDENTITY_FILES, SCRATCH_DIR};
use crate::{Store, migrate};
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::path::Path;

const FIXTURE: &str = include_str!("fresh_start_fixture.sql");
const OLD_GLOBAL: &str = "0a000000-0000-4000-8000-000000000001";
const PROJECT: &str = "0c000000-0000-4000-8000-000000000001";
const ORION: &str = "0c000000-0000-4000-8000-000000000002";
const ADA: &str = "0c000000-0000-4000-8000-000000000003";

fn memory(n: u32) -> String {
    format!("0d000000-0000-4000-8000-{n:012}")
}

fn blob_text(n: u32) -> String {
    match n {
        4 | 5 => "The company ships on Fridays.".into(),
        6 => "Engineering reviews every department change.".into(),
        _ => format!("fixture text {n}"),
    }
}

/// Remote access as the hub reads it (synthetic ids).
const REMOTE_JSON: &str = r#"{"enabled":true,"key_id":"KEY0000000","team_id":"TEAM000000","bundle_id":"invalid.example.aurelius"}"#;

/// A protocol 3 workspace below `root`: schema 17, every table filled, blob
/// files, identity files and `remote.json` on disk.
fn protocol3(root: &Path) {
    drop(Store::open_until(root, fresh_start::LAST_PROTOCOL_3_VERSION).unwrap());
    let conn = Connection::open(root.join("data/aurelius.sqlite")).unwrap();
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();
    conn.execute_batch(&format!("BEGIN; PRAGMA defer_foreign_keys = ON; {FIXTURE} COMMIT;")).unwrap();
    let broken: i64 = conn.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| r.get(0)).unwrap();
    assert_eq!(broken, 0, "the fixture itself is consistent");
    drop(conn);
    std::fs::create_dir_all(root.join("blobs/fx")).unwrap();
    for n in 1..=14 {
        std::fs::write(root.join(format!("blobs/fx/blob{n:02}")), blob_text(n)).unwrap();
    }
    for name in IDENTITY_FILES {
        std::fs::write(root.join("data").join(name), format!("{name}-of-this-install")).unwrap();
    }
    std::fs::write(root.join("data/remote.json"), REMOTE_JSON).unwrap();
}

fn sha256(path: &Path) -> String {
    hex::encode(Sha256::digest(std::fs::read(path).unwrap()))
}

/// Every file below `dir` with its hash: a dry run must leave this unchanged.
fn listing(dir: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.push((path.strip_prefix(dir).unwrap().display().to_string(), sha256(&path)));
            }
        }
    }
    out.sort();
    out
}

fn one<T: rusqlite::types::FromSql>(conn: &Connection, sql: &str) -> T {
    conn.query_row(sql, [], |r| r.get(0)).unwrap()
}

#[test]
fn every_table_of_the_protocol_3_schema_has_a_decision() {
    let dir = tempfile::tempdir().unwrap();
    drop(Store::open_until(dir.path(), fresh_start::LAST_PROTOCOL_3_VERSION).unwrap());
    let conn = Connection::open(dir.path().join("data/aurelius.sqlite")).unwrap();
    let undecided = fresh_start::undecided_tables(&conn).unwrap();
    assert!(undecided.is_empty(), "tables without a fresh start decision (add them to fresh_start::DECISIONS): {undecided:?}");
    // And the list names no table that does not exist (except migration 14's own).
    let tables: Vec<String> = conn.prepare("SELECT name FROM sqlite_master WHERE type = 'table'").unwrap().query_map([], |r| r.get(0)).unwrap().collect::<Result<_, _>>().unwrap();
    for (table, _) in DECISIONS {
        assert!(tables.iter().any(|t| t == table) || table.starts_with("_company_convert"), "{table} is not in the schema");
    }
    // A new table is noticed.
    conn.execute("CREATE TABLE later_feature(id TEXT PRIMARY KEY)", []).unwrap();
    assert_eq!(fresh_start::undecided_tables(&conn).unwrap(), ["later_feature"]);
}

#[test]
fn the_fixture_fills_every_table() {
    let dir = tempfile::tempdir().unwrap();
    protocol3(dir.path());
    let conn = Connection::open(dir.path().join("data/aurelius.sqlite")).unwrap();
    for (table, decision) in DECISIONS {
        if *decision == Decision::Own {
            continue;
        }
        let n: i64 = one(&conn, &format!("SELECT COUNT(*) FROM \"{table}\""));
        assert!(n > 0, "the fixture has no row in {table}");
    }
}

#[test]
fn a_new_table_without_a_decision_stops_the_fresh_start() {
    let dir = tempfile::tempdir().unwrap();
    protocol3(dir.path());
    Connection::open(dir.path().join("data/aurelius.sqlite")).unwrap().execute("CREATE TABLE later_feature(id TEXT)", []).unwrap();
    let before = listing(dir.path());
    assert!(matches!(fresh_start::dry_run(dir.path()), Err(FreshStartError::Refused(m)) if m.contains("later_feature")), "the dry run predicts the refusal");
    assert!(matches!(fresh_start::upgrade(dir.path(), None), Err(FreshStartError::Refused(m)) if m.contains("later_feature")));
    assert_eq!(listing(dir.path()), before, "nothing moved");
}

#[test]
fn a_dry_run_counts_and_writes_nothing_in_the_source() {
    let dir = tempfile::tempdir().unwrap();
    protocol3(dir.path());
    let before = listing(dir.path());

    let report = fresh_start::dry_run(dir.path()).unwrap().unwrap();

    assert_eq!(listing(dir.path()), before, "the source folder is byte for byte the same");
    assert!(!dir.path().join(BACKUP_DIR).exists());
    assert_eq!(report.from_version, 17);
    assert_eq!(report.backup_dir, None);
    let copied = |t: &str| report.copied.get(t).copied().unwrap_or(0);
    assert_eq!((copied("projects"), copied("employees"), copied("memory_items")), (1, 2, 5));
    assert_eq!((copied("api_keys"), copied("api_key_functions"), copied("remote_sessions")), (1, 1, 1));
    assert_eq!((copied("heartbeats"), copied("recurring_tasks"), copied("agent_functions")), (1, 1, 1));
    assert_eq!(report.paused.values().sum::<u64>(), 3);
    assert_eq!((report.from_department, report.superseded_duplicates), (2, 1));
    assert_eq!(report.per_department.get("Engineering"), Some(&2));
    assert!(report.dispatcher.is_some());
    for t in ["tasks", "messages", "runs", "users", "tickets"] {
        assert_eq!(report.new_database[t], 0, "{t}");
    }
    // The new conversations are the empty Team chat and each employee's main chat.
    assert_eq!(report.new_database["conversations"], 3);
    for t in ["tasks", "events", "messages", "conversations", "departments", "function_calls", "command_receipts", "heartbeat_wakes", "file_grants", "message_sources", "run_activity", "task_dependencies", "task_variants", "task_variant_sets"] {
        assert!(report.not_copied.contains_key(t), "{t} is reported as not copied");
    }
    for t in ["blobs", "memory_sources", "retrieval_chunks", "chunk_fts"] {
        assert!(report.rebuilt.contains_key(t) && !report.not_copied.contains_key(t), "{t} is reported as rebuilt");
    }
    // Without names the report shows counts only.
    let text = report.lines(false).join("\n");
    assert!(!text.contains("Engineering") && !text.contains("Orion") && !text.contains("Alpha"), "{text}");
    assert!(report.lines(true).join("\n").contains("Engineering: 2"));
}

#[test]
fn the_fresh_start_sets_the_old_workspace_aside_and_imports_it() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    protocol3(root);
    let old_db = sha256(&root.join("data/aurelius.sqlite"));

    let report = fresh_start::upgrade(root, None).unwrap().expect("a protocol 3 database needs a fresh start");

    // The old folders are aside, untouched; the identity and configuration files are copied back.
    let aside = root.join(BACKUP_DIR);
    assert_eq!(report.backup_dir.as_deref(), Some(aside.as_path()));
    assert_eq!(sha256(&aside.join("data/aurelius.sqlite")), old_db);
    assert!(aside.join("blobs/fx/blob01").is_file());
    assert!(!aside.join("import-pending").exists());
    for name in IDENTITY_FILES {
        assert_eq!(std::fs::read_to_string(root.join("data").join(name)).unwrap(), format!("{name}-of-this-install"));
    }
    assert_eq!(CONFIG_FILES, ["remote.json"]);
    assert_eq!(std::fs::read_to_string(root.join("data/remote.json")).unwrap(), REMOTE_JSON, "remote access stays on, with its push ids");
    assert!(!root.join(SCRATCH_DIR).exists(), "the scratch folder is gone");

    let store = Store::open(root).unwrap();
    store
        .read(|c| {
            let version: i64 = one(c, "SELECT MAX(version) FROM schema_migrations");
            assert_eq!(version, migrate::supported_version());
            // Projects and employees with the same ids and settings.
            let (status, folder, description): (String, String, String) =
                c.query_row("SELECT p.status, p.folder, b.storage_key FROM projects p JOIN blobs b ON b.id = p.description_blob_id WHERE p.id = ?1", [PROJECT], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
            assert_eq!((status.as_str(), folder.as_str()), ("paused", "/work/alpha"));
            assert_eq!(store.blobs().read(&description).unwrap(), blob_text(1).as_bytes());
            let (reports_to, title): (Option<String>, String) = c.query_row("SELECT reports_to, title FROM employees WHERE id = ?1", [ADA], |r| Ok((r.get(0)?, r.get(1)?)))?;
            assert_eq!((reports_to.as_deref(), title.as_str()), (Some(ORION), "Engineer"));
            let main_chats: i64 = one(c, "SELECT COUNT(*) FROM conversations WHERE kind = 'employee'");
            assert_eq!(main_chats, 2);
            let versions: Vec<Option<String>> = c.prepare("SELECT proposal_id FROM employee_instruction_versions ORDER BY version")?.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?;
            assert_eq!(versions, [None, None], "proposals stay behind");

            // The company keeps the old global scope; Orion dispatches.
            let (scope, dispatcher): (String, Option<String>) = c.query_row("SELECT scope_id, dispatcher_employee_id FROM company", [], |r| Ok((r.get(0)?, r.get(1)?)))?;
            assert_eq!((scope.as_str(), dispatcher.as_deref()), (OLD_GLOBAL, Some(ORION)));
            assert_eq!(one::<i64>(c, "SELECT COUNT(*) FROM board_columns"), 6);
            assert_eq!(one::<i64>(c, "SELECT COUNT(*) FROM users"), 0, "no users: every client starts in the onboarding");

            // Memory: department memory is company memory now, the duplicate superseded.
            let mem = |n: u32| -> rusqlite::Result<Option<(String, String, String, Option<String>)>> {
                c.query_row("SELECT scope_id, status, locator_json, supersedes_id FROM memory_items WHERE id = ?1", [memory(n)], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
                    .map(Some)
                    .or_else(|e| if e == rusqlite::Error::QueryReturnedNoRows { Ok(None) } else { Err(e) })
            };
            let (scope1, status1, _, supersedes1) = mem(1)?.unwrap();
            assert_eq!((scope1.as_str(), status1.as_str(), supersedes1), (OLD_GLOBAL, "active", Some(memory(2))), "the oldest stays");
            let (scope2, status2, locator2, _) = mem(2)?.unwrap();
            assert_eq!((scope2.as_str(), status2.as_str()), (OLD_GLOBAL, "superseded"));
            let locator2: serde_json::Value = serde_json::from_str(&locator2).unwrap();
            assert_eq!((locator2["from_department"].as_str(), locator2["task"].as_str()), (Some("Engineering"), Some("t")));
            let (scope3, status3, locator3, _) = mem(3)?.unwrap();
            assert_eq!((scope3.as_str(), status3.as_str()), (OLD_GLOBAL, "active"));
            assert!(locator3.contains("\"from_department\":\"Engineering\""));
            assert!(mem(4)?.is_none() && mem(7)?.is_none(), "forgotten and rejected memories stay behind");
            let (_, status6, _, supersedes6) = mem(6)?.unwrap();
            assert_eq!((status6.as_str(), supersedes6), ("superseded", None), "a link to a memory that stayed behind is dropped");
            let sources: i64 = one(c, "SELECT COUNT(*) FROM memory_sources ms JOIN events e ON e.id = ms.event_id WHERE e.aggregate_type = 'import'");
            assert_eq!(sources, 5);
            assert_eq!(one::<i64>(c, "SELECT COUNT(*) FROM retrieval_chunks"), 3, "the active memories are indexed anew");
            assert_eq!(one::<i64>(c, "SELECT COUNT(*) FROM chunk_fts WHERE chunk_fts MATCH 'reviews'"), 1);
            assert_eq!(one::<i64>(c, "SELECT COUNT(*) FROM chunk_fts WHERE chunk_fts MATCH 'old'"), 0);

            // Run defaults, but not the department's; settings without the old log's cursors.
            let (overrides, source_type): (i64, String) =
                c.query_row("SELECT COUNT(*), MIN(e.aggregate_type) FROM config_overrides o JOIN events e ON e.id = o.source_event_id", [], |r| Ok((r.get(0)?, r.get(1)?)))?;
            assert_eq!((overrides, source_type.as_str()), (1, "import"));
            let keys: Vec<String> = c.prepare("SELECT key FROM settings ORDER BY key")?.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?;
            assert_eq!(keys, ["follow_org_chart", "sessions_owned_since_ms", "trace_mode"]);

            // Configuration and pairings come over as they were.
            for (table, n) in [("connections", 1), ("employee_plugins", 1), ("project_plugins", 1), ("remote_clients", 1), ("remote_sessions", 1), ("push_registrations", 1), ("app_keys", 1), ("api_keys", 1), ("api_key_functions", 1), ("notes", 1)] {
                assert_eq!(one::<i64>(c, &format!("SELECT COUNT(*) FROM {table}")), n, "{table}");
            }
            assert_eq!(one::<String>(c, "SELECT webhook_secret FROM app_keys"), "secret");
            let (idea, task): (String, Option<String>) = c.query_row("SELECT title, task_id FROM ideas", [], |r| Ok((r.get(0)?, r.get(1)?)))?;
            assert_eq!((idea.as_str(), task), ("Open idea", None), "open ideas only, without their task");

            // Paused: nothing makes work outside the board.
            let (enabled, cursor): (i64, i64) = c.query_row("SELECT enabled, cursor_seq FROM heartbeats", [], |r| Ok((r.get(0)?, r.get(1)?)))?;
            assert_eq!((enabled, cursor), (0, 0));
            let (paused, last_task): (i64, Option<String>) = c.query_row("SELECT paused, last_task_id FROM recurring_tasks", [], |r| Ok((r.get(0)?, r.get(1)?)))?;
            assert_eq!((paused, last_task), (1, None));
            assert_eq!(one::<i64>(c, "SELECT enabled FROM agent_functions"), 0);

            // Work and history stay behind; the Team chat is new and empty.
            for table in ["tasks", "runs", "messages", "approvals", "artifacts", "attachments", "shares", "usage_records", "jobs", "notifications", "proposals", "command_receipts", "function_calls", "heartbeat_wakes", "file_grants", "message_sources", "run_activity", "run_traces", "task_dependencies", "task_variant_sets", "task_variants", "conversation_sessions", "app_key_threads", "webhook_outbox"] {
                assert_eq!(one::<i64>(c, &format!("SELECT COUNT(*) FROM {table}")), 0, "{table}");
            }
            assert_eq!(one::<i64>(c, "SELECT COUNT(*) FROM conversations WHERE kind = 'group'"), 1);
            assert_eq!(one::<i64>(c, "SELECT COUNT(*) FROM events"), 1, "only the import's audit event");
            assert_eq!(one::<i64>(c, "SELECT COUNT(*) FROM pragma_foreign_key_check"), 0);
            Ok(())
        })
        .unwrap_or_else(|e| panic!("{e}"));
    drop(store);
    assert_eq!((report.from_version, report.from_department, report.superseded_duplicates), (17, 2, 1));

    // Once done, there is nothing more to do.
    assert_eq!(fresh_start::detect(root).unwrap(), None);
    assert!(fresh_start::upgrade(root, None).unwrap().is_none());
}

#[test]
fn an_existing_pre_v011_folder_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    protocol3(dir.path());
    // Not empty: an empty one is left by a crash and does not count.
    std::fs::create_dir_all(dir.path().join(BACKUP_DIR)).unwrap();
    std::fs::write(dir.path().join(BACKUP_DIR).join("kept.txt"), b"kept").unwrap();
    let before = listing(dir.path());
    assert!(matches!(fresh_start::upgrade(dir.path(), None), Err(FreshStartError::Refused(m)) if m.contains(BACKUP_DIR)));
    assert_eq!(listing(dir.path()), before);
}

#[test]
fn the_pre_update_backup_is_made_when_asked() {
    let dir = tempfile::tempdir().unwrap();
    protocol3(dir.path());
    fresh_start::upgrade(dir.path(), Some(("0.11.0", "production"))).unwrap().unwrap();
    let backups = crate::backup::list(dir.path());
    assert_eq!(backups.len(), 1);
    assert_eq!((backups[0].1.kind.as_str(), backups[0].1.schema_version), ("pre-update", 17));
}

#[test]
fn a_failed_import_puts_the_old_workspace_back() {
    let dir = tempfile::tempdir().unwrap();
    protocol3(dir.path());
    // A memory id that is not an id: the import fails inside its transaction.
    Connection::open(dir.path().join("data/aurelius.sqlite"))
        .unwrap()
        .execute("UPDATE memory_items SET id = 'not-an-id' WHERE id = ?1", [memory(3)])
        .unwrap();
    let before = listing(dir.path());
    assert!(fresh_start::upgrade(dir.path(), None).is_err());
    assert_eq!(listing(dir.path()), before, "data/ and blobs/ are back, byte for byte");
    assert!(!dir.path().join(BACKUP_DIR).exists());
}

#[test]
fn an_interrupted_fresh_start_is_undone_and_done_again() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    protocol3(root);
    let old_db = sha256(&root.join("data/aurelius.sqlite"));
    // As if the power went out after the move: the old folders aside, a new
    // database that never got its import, the marker still there.
    let aside = root.join(BACKUP_DIR);
    std::fs::create_dir_all(&aside).unwrap();
    std::fs::write(aside.join("import-pending"), b"").unwrap();
    std::fs::rename(root.join("data"), aside.join("data")).unwrap();
    std::fs::rename(root.join("blobs"), aside.join("blobs")).unwrap();
    drop(Store::open(root).unwrap());

    let report = fresh_start::upgrade(root, None).unwrap().expect("done again");
    assert_eq!(report.copied["employees"], 2);
    assert_eq!(sha256(&aside.join("data/aurelius.sqlite")), old_db);
    assert!(!aside.join("import-pending").exists());
}

#[test]
fn an_older_database_is_migrated_on_a_copy_first() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    drop(Store::open_until(root, migrate::LAST_PROTOCOL_2_VERSION).unwrap());
    {
        let conn = Connection::open(root.join("data/aurelius.sqlite")).unwrap();
        conn.execute_batch(&format!(
            "INSERT INTO scopes(id, kind) VALUES ('{OLD_GLOBAL}', 'global');
             INSERT INTO scopes(id, kind, parent_id, owner_id) VALUES ('0a000000-0000-4000-8000-000000000002', 'project', '{OLD_GLOBAL}', '{PROJECT}');
             INSERT INTO projects(id, scope_id, name, status, version, created_at_ms, updated_at_ms) VALUES ('{PROJECT}', '0a000000-0000-4000-8000-000000000002', 'Old', 'active', 1, 1, 1);"
        ))
        .unwrap();
    }
    let before = listing(root);
    let dry = fresh_start::dry_run(root).unwrap().unwrap();
    assert_eq!(listing(root), before, "the migration ran on a copy");
    assert_eq!((dry.from_version, dry.copied["projects"]), (13, 1));
    let report = fresh_start::upgrade(root, None).unwrap().unwrap();
    assert_eq!(report.from_version, 13);
    let conn = Connection::open(root.join("data/aurelius.sqlite")).unwrap();
    assert_eq!(one::<String>(&conn, "SELECT name FROM projects"), "Old");
}

#[test]
fn nothing_to_do_for_a_new_or_current_workspace() {
    let dir = tempfile::tempdir().unwrap();
    assert!(fresh_start::upgrade(dir.path(), None).unwrap().is_none(), "no database");
    drop(Store::open(dir.path()).unwrap());
    assert!(fresh_start::upgrade(dir.path(), None).unwrap().is_none(), "already v0.11");
    assert!(!dir.path().join(BACKUP_DIR).exists());
}

#[test]
fn a_move_that_stopped_between_data_and_blobs_keeps_the_old_blobs() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    protocol3(root);
    let blobs_before = listing(&root.join("blobs"));
    // data/ was moved, blobs/ (still the old one) was not.
    let aside = root.join(BACKUP_DIR);
    std::fs::create_dir_all(&aside).unwrap();
    std::fs::write(aside.join("import-pending"), b"").unwrap();
    std::fs::rename(root.join("data"), aside.join("data")).unwrap();

    let report = fresh_start::upgrade(root, None).unwrap().expect("done again");
    assert_eq!(report.copied["memory_items"], 5, "every memory text was found");
    assert_eq!(listing(&aside.join("blobs")), blobs_before);
}

#[test]
fn restoring_a_backup_from_before_v011_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    protocol3(root);
    let old = crate::backup::before_migration(root, "0.10.0", "production").unwrap().unwrap();
    let manifest = crate::backup::verify(&old).unwrap();
    assert_eq!(manifest.schema_version, 17);
    let refusal = crate::backup::restore_refusal(&manifest).expect("a schema 17 backup is refused");
    assert!(refusal.contains("before v0.11") && refusal.contains(BACKUP_DIR) && refusal.contains("previous build"), "{refusal}");

    fresh_start::upgrade(root, None).unwrap().unwrap();
    let before = listing(&root.join("data"));
    let err = crate::backup::request_restore(root, &old).unwrap_err();
    assert!(err.to_string().contains("before v0.11"), "{err}");
    assert!(!root.join("data/restore-pending").exists());
    // A marker an older build left is refused at the start too, before anything moves.
    std::fs::write(root.join("data/restore-pending"), old.to_string_lossy().as_bytes()).unwrap();
    assert!(crate::backup::perform_pending_restore(root).is_err());
    std::fs::remove_file(root.join("data/restore-pending")).unwrap();
    assert_eq!(listing(&root.join("data")), before, "data/ is unchanged");
    assert!(!std::fs::read_dir(root).unwrap().flatten().any(|e| e.file_name().to_string_lossy().starts_with("before-restore")));

    let new = tempfile::tempdir().unwrap();
    let store = Store::open(new.path()).unwrap();
    let (_, manifest) = crate::backup::create(&store, new.path(), "manual", "0.11.0", "production").unwrap();
    assert_eq!(manifest.schema_version, migrate::supported_version());
    assert_eq!(crate::backup::restore_refusal(&manifest), None);
}

#[test]
fn a_restore_keeps_the_configuration_of_this_installation() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let store = Store::open(root).unwrap();
    let (backup, _) = crate::backup::create(&store, root, "manual", "0.11.0", "production").unwrap();
    drop(store);
    std::fs::write(root.join("data/remote.json"), REMOTE_JSON).unwrap();
    crate::backup::request_restore(root, &backup).unwrap();
    crate::backup::perform_pending_restore(root).unwrap().unwrap();
    assert_eq!(std::fs::read_to_string(root.join("data/remote.json")).unwrap(), REMOTE_JSON);
}

#[test]
fn a_dry_run_reads_a_consistent_copy_while_the_database_is_open() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    protocol3(root);
    // A live writer: a row only in the WAL (no checkpoint) while the dry run reads.
    let writer = Connection::open(root.join("data/aurelius.sqlite")).unwrap();
    writer.pragma_update(None, "journal_mode", "WAL").unwrap();
    writer.pragma_update(None, "wal_autocheckpoint", 0).unwrap();
    writer
        .execute(
            "INSERT INTO notes(id, project_id, body, pinned, revision, created_at_ms, updated_at_ms) SELECT '0f000000-0000-4000-8000-000000000009', project_id, 'Second', 0, 1, 2, 2 FROM notes LIMIT 1",
            [],
        )
        .unwrap();
    assert!(root.join("data/aurelius.sqlite-wal").is_file() && root.join("data/aurelius.sqlite-shm").is_file());
    let files = |root: &Path| listing(&root.join("data")).into_iter().filter(|(f, _)| !f.ends_with("-shm")).collect::<Vec<_>>();
    let before = files(root);

    let report = fresh_start::dry_run(root).unwrap().unwrap();

    assert_eq!(report.copied["notes"], 2, "the row in the WAL is read");
    assert_eq!(files(root), before, "the database and its WAL are unchanged");
    assert!(!root.join(SCRATCH_DIR).exists());
}

#[test]
fn a_scratch_folder_left_by_a_crash_is_removed_by_the_next_run() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let stale = root.join(SCRATCH_DIR).join("crashed");
    std::fs::create_dir_all(&stale).unwrap();
    std::fs::write(stale.join(".lock"), b"").unwrap();
    std::fs::write(stale.join("old.sqlite"), b"secret").unwrap();

    let scratch = fresh_start::Scratch::new(root).unwrap();
    assert!(!stale.exists(), "nobody held its lock");
    #[cfg(unix)]
    for path in [root.join(SCRATCH_DIR), scratch.path().to_path_buf()] {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o700, "{}", path.display());
    }
    // A running one is left alone.
    let other = fresh_start::Scratch::new(root).unwrap();
    assert!(scratch.path().is_dir() && other.path().is_dir());
    drop(other);
    assert!(scratch.path().is_dir());
    drop(scratch);
    assert!(!root.join(SCRATCH_DIR).exists());
}

/// Every folder and file below `root` read-only (`writable`: back again).
#[cfg(unix)]
fn set_writable(root: &Path, writable: bool) {
    use std::os::unix::fs::PermissionsExt;
    let mut stack = vec![root.to_path_buf()];
    let mut dirs = Vec::new();
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else {
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(if writable { 0o644 } else { 0o444 })).unwrap();
            }
        }
        dirs.push(d);
    }
    for d in dirs {
        std::fs::set_permissions(&d, std::fs::Permissions::from_mode(if writable { 0o755 } else { 0o555 })).unwrap();
    }
}

#[cfg(unix)]
#[test]
fn a_dry_run_works_on_a_read_only_folder() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("snapshot");
    protocol3(&root);
    let before = listing(&root);
    set_writable(&root, false);
    if std::fs::write(root.join("probe"), b"").is_ok() {
        // Running as a user that writes anyway (root): nothing to prove here.
        set_writable(&root, true);
        return;
    }

    let report = fresh_start::dry_run(&root);

    set_writable(&root, true);
    let report = report.expect("a dry run needs no write access").expect("there is something to do");
    assert_eq!(report.copied["employees"], 2);
    assert_eq!(listing(&root), before, "nothing written below the root");
    assert!(!root.join(SCRATCH_DIR).exists());
}

#[test]
fn the_dry_run_predicts_the_outcome_of_the_real_run() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    protocol3(root);

    // An existing pre-v0.11/ with something in it: both refuse, with the same reason.
    std::fs::create_dir_all(root.join(BACKUP_DIR).join("data")).unwrap();
    let before = listing(root);
    let refused = |r: Result<Option<_>, FreshStartError>| matches!(r, Err(FreshStartError::Refused(m)) if m.contains("pre-v0.11 already exists"));
    assert!(refused(fresh_start::dry_run(root)), "the dry run refuses");
    assert!(refused(fresh_start::upgrade(root, None)), "like the real run");
    assert_eq!(listing(root), before);
    std::fs::remove_dir(root.join(BACKUP_DIR).join("data")).unwrap();

    // Once done (schema 18): nothing to do, for both.
    fresh_start::upgrade(root, None).unwrap().unwrap();
    let before = listing(root);
    assert!(fresh_start::dry_run(root).unwrap().is_none(), "nothing to do");
    assert!(fresh_start::upgrade(root, None).unwrap().is_none());
    assert_eq!(listing(root), before);

    // No database at all: nothing to do either.
    let empty = tempfile::tempdir().unwrap();
    assert!(fresh_start::dry_run(empty.path()).unwrap().is_none());
}

#[test]
fn a_dry_run_after_an_interrupted_fresh_start_reads_what_comes_back() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    protocol3(root);
    let aside = root.join(BACKUP_DIR);
    std::fs::create_dir_all(&aside).unwrap();
    std::fs::write(aside.join("import-pending"), b"").unwrap();
    std::fs::rename(root.join("data"), aside.join("data")).unwrap();
    std::fs::rename(root.join("blobs"), aside.join("blobs")).unwrap();
    drop(Store::open(root).unwrap());
    let before = listing(root);

    let dry = fresh_start::dry_run(root).unwrap().expect("the real run would import the old database again");

    assert_eq!((dry.from_version, dry.copied["employees"], dry.copied["memory_items"]), (17, 2, 5));
    assert_eq!(listing(root), before, "nothing put back or written");
    let real = fresh_start::upgrade(root, None).unwrap().unwrap();
    assert_eq!(real.copied, dry.copied);
}

#[test]
fn an_empty_pre_v011_folder_left_by_a_crash_does_not_block_the_start() {
    // A crash between creating pre-v0.11/ and writing its marker, or (when an
    // interrupted run was undone) between removing the marker and the folder.
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    protocol3(root);
    std::fs::create_dir(root.join(BACKUP_DIR)).unwrap();
    assert!(fresh_start::dry_run(root).unwrap().is_some(), "the dry run does not refuse either");

    let report = fresh_start::upgrade(root, None).unwrap().expect("the next start does the fresh start");
    assert_eq!(report.copied["employees"], 2);
    assert!(root.join(BACKUP_DIR).join("data/aurelius.sqlite").is_file());
    assert!(!root.join(BACKUP_DIR).join("import-pending").exists());

    // On a database that is already v0.11 an empty one goes too.
    let current = tempfile::tempdir().unwrap();
    drop(Store::open(current.path()).unwrap());
    std::fs::create_dir(current.path().join(BACKUP_DIR)).unwrap();
    assert!(fresh_start::upgrade(current.path(), None).unwrap().is_none());
    assert!(!current.path().join(BACKUP_DIR).exists());
}

#[test]
fn a_scratch_folder_still_being_made_is_not_taken_for_a_crashed_one() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let parent = root.join(SCRATCH_DIR);
    // Another run made its folder and has not created its lock yet.
    let young = parent.join("starting");
    std::fs::create_dir_all(&young).unwrap();
    // A crash right after making the folder, long ago.
    let old = parent.join("crashed-before-its-lock");
    std::fs::create_dir_all(&old).unwrap();
    std::fs::File::open(&old).unwrap().set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(120)).unwrap();

    let scratch = fresh_start::Scratch::new(root).unwrap();

    assert!(young.is_dir(), "a run that is starting keeps its folder");
    assert!(!old.exists(), "an old folder without a lock is removed");
    drop(scratch);
    assert!(young.is_dir());
}

#[test]
fn a_dry_run_keeps_its_scratch_folder_private_and_outside_the_root() {
    let temp = std::env::temp_dir();
    // One a crash left behind (lock free): removed by the next dry run.
    let stale = temp.join(format!("{DRY_RUN_PREFIX}crashed-{}", uuid::Uuid::now_v7().simple()));
    std::fs::create_dir(&stale).unwrap();
    std::fs::write(stale.join(".lock"), b"").unwrap();
    std::fs::write(stale.join("old.sqlite"), b"secret").unwrap();

    let scratch = fresh_start::Scratch::in_temp().unwrap();

    assert!(scratch.path().starts_with(&temp) && scratch.path().file_name().unwrap().to_string_lossy().starts_with(DRY_RUN_PREFIX));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(scratch.path()).unwrap().permissions().mode() & 0o777, 0o700);
    }
    assert!(!stale.exists(), "nobody held its lock");
    let path = scratch.path().to_path_buf();
    drop(scratch);
    assert!(!path.exists());
    assert!(temp.is_dir(), "the temp folder itself stays");
}

#[test]
fn reading_the_version_at_the_start_leaves_no_wal_or_shm_next_to_the_database() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    protocol3(root);
    let conn = Connection::open(root.join("data/aurelius.sqlite")).unwrap();
    assert_eq!(one::<String>(&conn, "PRAGMA journal_mode"), "wal", "a WAL database, closed cleanly");
    drop(conn);
    assert!(!root.join("data/aurelius.sqlite-wal").exists());
    let before = listing(&root.join("data"));

    // The daemon's start: first the v0.6 check, then the fresh start.
    assert!(crate::import::upgrade_legacy(root).unwrap().is_none());
    assert_eq!(listing(&root.join("data")), before, "no empty -wal or -shm appeared");
    fresh_start::upgrade(root, None).unwrap().unwrap();
    assert_eq!(listing(&root.join(BACKUP_DIR).join("data")), before, "pre-v0.11/data/ holds exactly the old files");
}
