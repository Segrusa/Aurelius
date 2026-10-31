//! Phase 4 (migration 22): the vault's items and audit.

use crate::vault::{
    self, AuditAction, AuditActor, AuditFilter, NewAudit, NewVaultItem, Sealed, ValueSource, VaultKind, VaultScope, VaultUpdate,
};
use crate::{StorageError, Store};
use rusqlite::{Connection, params};

const ALEX: &str = "00000000-0000-7000-8000-0000000000a1";
const ROBIN: &str = "00000000-0000-7000-8000-0000000000a2";
const ALPHA: &str = "00000000-0000-7000-8000-0000000000b1";
const BETA: &str = "00000000-0000-7000-8000-0000000000b2";
const OLD: &str = "00000000-0000-7000-8000-0000000000b3";

/// A store with two board members, two active projects and an archived one.
fn store() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    store
        .write(|tx| {
            let company: String = tx.query_row("SELECT id FROM company", [], |r| r.get(0))?;
            let global: String = tx.query_row("SELECT id FROM scopes WHERE kind = 'global'", [], |r| r.get(0))?;
            for (id, status) in [(ALPHA, "active"), (BETA, "active"), (OLD, "archived")] {
                tx.execute("INSERT INTO scopes(id, kind, parent_id, owner_id) VALUES (?1, 'project', ?2, ?3)", params![format!("s-{id}"), global, id])?;
                tx.execute(
                    "INSERT INTO projects(id, scope_id, name, status, version, created_at_ms, updated_at_ms) VALUES (?1, ?2, ?1, ?3, 1, 1, 1)",
                    params![id, format!("s-{id}"), status],
                )?;
            }
            for (id, name) in [(ALEX, "Alex"), (ROBIN, "Robin")] {
                tx.execute(
                    "INSERT INTO users(id, company_id, username, password_hash, created_at_ms, updated_at_ms) VALUES (?1, ?2, ?3, '$argon2id$x', 1, 1)",
                    params![id, company, name],
                )?;
            }
            Ok(())
        })
        .unwrap();
    (dir, store)
}

/// A stand-in for the daemon's cipher: the value XOR 0xA5. Never in the clear.
fn seal(value: &str, key_id: &str) -> Sealed {
    Sealed { ciphertext: value.bytes().map(|b| b ^ 0xA5).collect(), nonce: vec![7; 12], key_id: key_id.into() }
}

fn new<'a>(id: &'a str, project: Option<&'a str>, kind: VaultKind, name: &'a str, env: Option<&'a str>) -> NewVaultItem<'a> {
    NewVaultItem {
        id,
        project_id: project,
        kind,
        name,
        env_name: env,
        description: None,
        sealed: seal(name, "k1"),
        by_user: ALEX,
        now_ms: 100,
    }
}

fn add(store: &Store, id: &'static str, project: Option<&'static str>, kind: VaultKind, name: &'static str, env: Option<&'static str>) -> Result<vault::VaultItemRow, StorageError> {
    store.write(move |tx| vault::insert(tx, new(id, project, kind, name, env)))
}

fn no_change(by_user: &str) -> VaultUpdate<'_> {
    VaultUpdate { name: None, env_name: None, description: None, kind: None, sealed: None, expected_version: None, by_user, now_ms: 200 }
}

fn rejected(result: Result<impl std::fmt::Debug, StorageError>, why: &str) {
    match result {
        Err(StorageError::Rejected(r)) => assert_eq!(r, why),
        other => panic!("expected Rejected({why}), got {other:?}"),
    }
}

fn raw(store: &Store) -> Connection {
    let conn = Connection::open(store.db_path()).unwrap();
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();
    conn
}

#[test]
fn the_kind_changes_between_text_and_url_only() {
    let (_dir, store) = store();
    add(&store, "t", None, VaultKind::Text, "Docs", None).unwrap();
    add(&store, "s", None, VaultKind::Secret, "Token", Some("TOKEN")).unwrap();
    let conn = raw(&store);

    // In SQL: text ↔ url passes, to or from secret is refused by the trigger.
    conn.execute("UPDATE vault_items SET kind = 'url' WHERE id = 't'", []).unwrap();
    conn.execute("UPDATE vault_items SET kind = 'text' WHERE id = 't'", []).unwrap();
    for (id, kind) in [("t", "secret"), ("s", "text"), ("s", "url")] {
        let err = conn.execute("UPDATE vault_items SET kind = ?2 WHERE id = ?1", params![id, kind]).unwrap_err();
        assert!(err.to_string().contains("cannot change to or from secret"), "{id} → {kind}: {err}");
    }
    // Setting the same kind is no change.
    conn.execute("UPDATE vault_items SET kind = 'secret' WHERE id = 's'", []).unwrap();
    assert!(conn.execute("UPDATE vault_items SET kind = 'note' WHERE id = 't'", []).is_err(), "kind CHECK");
    assert!(conn.execute("UPDATE vault_items SET project_id = ?1 WHERE id = 't'", [ALPHA]).is_err(), "an item never moves scope");
    assert!(conn.execute("DELETE FROM vault_items WHERE id = 't'", []).is_err(), "items are archived, not deleted");
    drop(conn);

    // Through the API: the same rule, with a reason.
    let to_secret = VaultUpdate { kind: Some(VaultKind::Secret), sealed: Some(seal("x", "k1")), env_name: Some(Some("DOCS")), ..no_change(ROBIN) };
    rejected(store.write(|tx| vault::update(tx, "t", to_secret)), vault::KIND_SECRET);
    let to_text = VaultUpdate { kind: Some(VaultKind::Text), sealed: Some(seal("x", "k1")), ..no_change(ROBIN) };
    rejected(store.write(|tx| vault::update(tx, "s", to_text)), vault::KIND_SECRET);
    let without_value = VaultUpdate { kind: Some(VaultKind::Url), ..no_change(ROBIN) };
    rejected(store.write(|tx| vault::update(tx, "t", without_value)), vault::KIND_NEEDS_VALUE);
    let to_url = VaultUpdate { kind: Some(VaultKind::Url), sealed: Some(seal("https://example.com", "k1")), ..no_change(ROBIN) };
    let updated = store.write(|tx| vault::update(tx, "t", to_url)).unwrap();
    assert_eq!((updated.item.kind, updated.item.version, updated.changed.as_slice()), (VaultKind::Url, 2, ["kind", "value"].as_slice()));
}

#[test]
fn the_columns_are_checked() {
    let (_dir, store) = store();
    let conn = raw(&store);
    let company: String = conn.query_row("SELECT id FROM company", [], |r| r.get(0)).unwrap();
    let insert = |id: &str, kind: &str, env: Option<&str>, ciphertext: &[u8], key: &str| {
        conn.execute(
            "INSERT INTO vault_items(id, company_id, kind, name, env_name, ciphertext, nonce, key_id, version, created_by_user, updated_by_user, created_at_ms, updated_at_ms)
             VALUES (?1, ?2, ?3, ?1, ?4, ?5, x'00', ?6, 1, ?7, ?7, 1, 1)",
            params![id, company, kind, env, ciphertext, key, ALEX],
        )
    };
    insert("ok", "secret", Some("API_KEY_2"), b"c", "k1").unwrap();
    insert("ok2", "text", None, b"c", "k1").unwrap();
    assert!(insert("a", "note", None, b"c", "k1").is_err(), "unknown kind");
    assert!(insert("b", "secret", None, b"c", "k1").is_err(), "a secret has an env name");
    for env in ["api_key", "API-KEY", "2FA", "", "API KEY"] {
        assert!(insert(&format!("e{env}"), "text", Some(env), b"c", "k1").is_err(), "env name {env:?} is not UPPER_SNAKE");
    }
    assert!(insert("c", "text", None, b"", "k1").is_err(), "a value is never empty");
    assert!(insert("d", "text", None, b"c", "").is_err(), "a key id");
    assert!(
        conn.execute("INSERT INTO vault_items(id, company_id, kind, name, nonce, key_id, version, created_by_user, updated_by_user, created_at_ms, updated_at_ms)
                      VALUES ('f', ?1, 'text', 'f', x'00', 'k1', 1, ?2, ?2, 1, 1)", params![company, ALEX]).is_err(),
        "ciphertext is NOT NULL"
    );
    // There is no plaintext column at all.
    let columns: Vec<String> =
        conn.prepare("SELECT name FROM pragma_table_info('vault_items')").unwrap().query_map([], |r| r.get(0)).unwrap().collect::<Result<_, _>>().unwrap();
    assert!(!columns.iter().any(|c| c.contains("value") || c.contains("plain")), "{columns:?}");
    let audit: Vec<String> =
        conn.prepare("SELECT name FROM pragma_table_info('vault_audit')").unwrap().query_map([], |r| r.get(0)).unwrap().collect::<Result<_, _>>().unwrap();
    assert!(!audit.iter().any(|c| c.contains("value") || c.contains("cipher")), "{audit:?}");
    drop(conn);

    // The API refuses the same before SQL does.
    rejected(add(&store, "g", None, VaultKind::Secret, "No env", None), vault::ENV_NAME_REQUIRED);
    rejected(add(&store, "h", None, VaultKind::Text, "Bad env", Some("bad_env")), vault::INVALID_ENV_NAME);
    rejected(add(&store, "i", None, VaultKind::Text, "  ", None), vault::INVALID_NAME);
    rejected(store.write(|tx| vault::update(tx, "ok", VaultUpdate { env_name: Some(None), ..no_change(ALEX) })), vault::ENV_NAME_REQUIRED);
    assert!(vault::valid_env_name("GITHUB_TOKEN") && vault::valid_env_name("_X1") && !vault::valid_env_name("1X"));
}

#[test]
fn names_and_env_names_are_unique_per_scope_until_archived() {
    let (_dir, store) = store();
    add(&store, "c1", None, VaultKind::Secret, "GitHub token", Some("GITHUB_TOKEN")).unwrap();
    rejected(add(&store, "c2", None, VaultKind::Secret, "github TOKEN", Some("OTHER")), vault::NAME_TAKEN);
    rejected(add(&store, "c3", None, VaultKind::Secret, "Another", Some("GITHUB_TOKEN")), vault::ENV_NAME_TAKEN);
    // Items without an env name do not collide on it.
    add(&store, "c4", None, VaultKind::Text, "Notes", None).unwrap();
    add(&store, "c5", None, VaultKind::Url, "Staging", None).unwrap();

    // Another scope may use the same name and env name.
    add(&store, "p1", Some(ALPHA), VaultKind::Secret, "GitHub token", Some("GITHUB_TOKEN")).unwrap();
    add(&store, "p2", Some(BETA), VaultKind::Secret, "GitHub token", Some("GITHUB_TOKEN")).unwrap();
    rejected(add(&store, "p3", Some(ALPHA), VaultKind::Text, "GITHUB TOKEN", None), vault::NAME_TAKEN);

    // An update into a taken name is refused too.
    rejected(store.write(|tx| vault::update(tx, "c4", VaultUpdate { name: Some("Staging"), ..no_change(ALEX) })), vault::NAME_TAKEN);
    rejected(store.write(|tx| vault::update(tx, "c4", VaultUpdate { env_name: Some(Some("GITHUB_TOKEN")), ..no_change(ALEX) })), vault::ENV_NAME_TAKEN);

    // Archived: the name and env name are free again, and an item is archived once.
    store.write(|tx| vault::archive(tx, "c1", ROBIN, 300)).unwrap();
    rejected(store.write(|tx| vault::archive(tx, "c1", ROBIN, 301)), vault::ARCHIVED);
    let again = add(&store, "c6", None, VaultKind::Secret, "GitHub token", Some("GITHUB_TOKEN")).unwrap();
    assert_eq!(again.version, 1);
    // Archived twice under the same name is fine; only the live ones are unique.
    store.write(|tx| vault::archive(tx, "c6", ROBIN, 302)).unwrap();
    add(&store, "c7", None, VaultKind::Secret, "GitHub token", Some("GITHUB_TOKEN")).unwrap();
    let archived = store.read(|c| vault::get(c, "c1")).unwrap().unwrap();
    assert_eq!((archived.archived_at_ms, archived.updated_by_user.as_str()), (Some(300), ROBIN));
    rejected(store.write(|tx| vault::update(tx, "c1", VaultUpdate { name: Some("x"), ..no_change(ALEX) })), vault::ARCHIVED);
}

#[test]
fn a_project_item_overrides_a_company_item_with_its_name_or_env_name() {
    let (_dir, store) = store();
    add(&store, "c-gh", None, VaultKind::Secret, "GitHub token", Some("GITHUB_TOKEN")).unwrap();
    add(&store, "c-stage", None, VaultKind::Url, "Staging URL", Some("STAGING_URL")).unwrap();
    add(&store, "c-sentry", None, VaultKind::Secret, "Sentry", Some("SENTRY_DSN")).unwrap();
    add(&store, "c-gone", None, VaultKind::Text, "Old notes", None).unwrap();
    store.write(|tx| vault::archive(tx, "c-gone", ALEX, 150)).unwrap();
    // Same name (without case), another env name.
    add(&store, "p-gh", Some(ALPHA), VaultKind::Secret, "github token", Some("GH_TOKEN")).unwrap();
    // Same env name, another name.
    add(&store, "p-dsn", Some(ALPHA), VaultKind::Secret, "Alpha DSN", Some("SENTRY_DSN")).unwrap();
    add(&store, "p-docs", Some(ALPHA), VaultKind::Text, "Docs", None).unwrap();
    // Archived: it overrides nothing any more.
    add(&store, "p-stage", Some(ALPHA), VaultKind::Url, "Staging URL", Some("STAGING_URL")).unwrap();
    store.write(|tx| vault::archive(tx, "p-stage", ALEX, 160)).unwrap();
    // Another project's item never shows in Alpha.
    add(&store, "b-gh", Some(BETA), VaultKind::Secret, "GitHub token", Some("GITHUB_TOKEN")).unwrap();

    let set = |project: Option<&'static str>| {
        store
            .read(move |c| vault::effective(c, project))
            .unwrap()
            .into_iter()
            .map(|e| (e.item.id, e.overrides_company))
            .collect::<Vec<_>>()
    };
    let s = |v: &[(&str, bool)]| v.iter().map(|(id, o)| (id.to_string(), *o)).collect::<Vec<_>>();
    assert_eq!(set(Some(ALPHA)), s(&[("p-dsn", true), ("p-docs", false), ("p-gh", true), ("c-stage", false)]));
    assert_eq!(set(Some(BETA)), s(&[("b-gh", true), ("c-sentry", false), ("c-stage", false)]));
    assert_eq!(set(None), s(&[("c-gh", false), ("c-sentry", false), ("c-stage", false)]));
    // No env name twice in a run.
    let alpha = store.read(|c| vault::effective(c, Some(ALPHA))).unwrap();
    let mut envs: Vec<_> = alpha.iter().filter_map(|e| e.item.env_name.clone()).collect();
    envs.sort();
    envs.dedup();
    assert_eq!(envs.len(), alpha.iter().filter(|e| e.item.env_name.is_some()).count());

    // The lists per scope.
    let ids = |scope: VaultScope<'static>, archived: bool| {
        store.read(move |c| vault::list(c, scope, archived)).unwrap().into_iter().map(|i| i.id).collect::<Vec<_>>()
    };
    assert_eq!(ids(VaultScope::Company, false), ["c-gh", "c-sentry", "c-stage"]);
    assert_eq!(ids(VaultScope::Company, true), ["c-gh", "c-gone", "c-sentry", "c-stage"]);
    assert_eq!(ids(VaultScope::Project(ALPHA), false), ["p-dsn", "p-docs", "p-gh"]);
    assert_eq!(ids(VaultScope::ProjectAndCompany(ALPHA), false), ["c-gh", "c-sentry", "c-stage", "p-dsn", "p-docs", "p-gh"]);

    // The redactor's set: every live secret in every scope.
    let secrets: Vec<_> = store.read(vault::active_secrets).unwrap().into_iter().map(|i| i.id).collect();
    assert_eq!(secrets, ["b-gh", "c-gh", "c-sentry", "p-dsn", "p-gh"]);
}

#[test]
fn an_update_raises_the_version_only_with_a_new_value() {
    let (_dir, store) = store();
    add(&store, "s", None, VaultKind::Secret, "Token", Some("TOKEN")).unwrap();
    let edit = VaultUpdate { name: Some("API token"), env_name: Some(Some("API_TOKEN")), description: Some(Some("for CI")), ..no_change(ROBIN) };
    let edited = store.write(|tx| vault::update(tx, "s", edit)).unwrap();
    assert_eq!(edited.changed, ["name", "env_name", "description"]);
    assert_eq!((edited.action(), edited.item.version, edited.item.updated_by_user.as_str(), edited.item.updated_at_ms), (Some(AuditAction::Edit), 1, ROBIN, 200));

    let replace = VaultUpdate { sealed: Some(seal("new", "k1")), expected_version: Some(1), ..no_change(ALEX) };
    let replaced = store.write(|tx| vault::update(tx, "s", replace)).unwrap();
    assert_eq!((replaced.action(), replaced.item.version, replaced.changed.as_slice()), (Some(AuditAction::Replace), 2, ["value"].as_slice()));
    rejected(store.write(|tx| vault::update(tx, "s", VaultUpdate { sealed: Some(seal("x", "k1")), expected_version: Some(1), ..no_change(ALEX) })), vault::VERSION_MISMATCH);

    // The same values: nothing is written.
    let same = VaultUpdate { name: Some("API token"), description: Some(Some("for CI")), ..no_change(ALEX) };
    let unchanged = store.write(|tx| vault::update(tx, "s", same)).unwrap();
    assert_eq!((unchanged.action(), unchanged.item.updated_by_user.as_str()), (None, ALEX));
    let stored = store.read(|c| vault::get(c, "s")).unwrap().unwrap();
    assert_eq!((stored.name.as_str(), stored.version, stored.updated_at_ms, stored.sealed.clone()), ("API token", 2, 200, seal("new", "k1")));
    assert!(!format!("{stored:?}").contains("[") , "Debug shows no ciphertext bytes: {stored:?}");
    rejected(store.write(|tx| vault::update(tx, "nope", no_change(ALEX))), vault::NOT_FOUND);
}

#[test]
fn an_archived_projects_items_stay_readable_and_can_be_archived() {
    let (_dir, store) = store();
    // Made while the project was active.
    store.write(|tx| {
        tx.execute("UPDATE projects SET status = 'active' WHERE id = ?1", [OLD])?;
        vault::insert(tx, new("o1", Some(OLD), VaultKind::Url, "Old site", None))?;
        vault::insert(tx, new("o2", Some(OLD), VaultKind::Secret, "Old key", Some("OLD_KEY")))?;
        tx.execute("UPDATE projects SET status = 'archived' WHERE id = ?1", [OLD])?;
        Ok(())
    })
    .unwrap();
    assert_eq!(store.read(|c| vault::list(c, VaultScope::Project(OLD), false)).unwrap().len(), 2);
    assert_eq!(store.read(|c| vault::get(c, "o1")).unwrap().unwrap().name, "Old site");
    rejected(add(&store, "o3", Some(OLD), VaultKind::Text, "New", None), vault::PROJECT_ARCHIVED);
    rejected(store.write(|tx| vault::update(tx, "o1", VaultUpdate { name: Some("Renamed"), ..no_change(ALEX) })), vault::PROJECT_ARCHIVED);
    store.write(|tx| vault::archive(tx, "o1", ALEX, 400)).unwrap();
    assert_eq!(store.read(|c| vault::list(c, VaultScope::Project(OLD), false)).unwrap().len(), 1);
    // The audit of such an item is written as for any other.
    store.write(|tx| vault::insert_audit(tx, &NewAudit { user_id: Some(ALEX), ..NewAudit::new("a1", Some("o1"), AuditAction::Archive, 400) })).unwrap();
}

#[test]
fn the_audit_is_append_only_and_never_holds_a_value() {
    let (_dir, store) = store();
    add(&store, "s", Some(ALPHA), VaultKind::Secret, "Token", Some("TOKEN")).unwrap();
    let row = store
        .write(|tx| {
            vault::insert_audit(
                tx,
                &NewAudit {
                    changed_fields: &["name", "env_name"],
                    user_id: Some(ALEX),
                    session_id: Some("sess-1"),
                    via: Some("dev-laptop"),
                    ..NewAudit::new("a1", Some("s"), AuditAction::Edit, 10)
                },
            )
        })
        .unwrap();
    assert_eq!((row.project_id.as_deref(), row.item_name.as_deref(), row.changed_fields.as_slice()), (Some(ALPHA), Some("Token"), ["name", "env_name"].map(String::from).as_slice()));
    store.write(|tx| vault::insert_audit(tx, &NewAudit { user_id: Some(ALEX), ..NewAudit::new("a2", None, AuditAction::KeyExport, 11) })).unwrap();

    let conn = raw(&store);
    for sql in ["UPDATE vault_audit SET action = 'copy' WHERE id = 'a1'", "UPDATE vault_audit SET changed_fields = NULL", "DELETE FROM vault_audit WHERE id = 'a1'", "DELETE FROM vault_audit"] {
        let err = conn.execute(sql, []).unwrap_err();
        assert!(err.to_string().contains("append-only"), "{sql}: {err}");
    }
    assert_eq!(conn.query_row("SELECT COUNT(*) FROM vault_audit", [], |r| r.get::<_, i64>(0)).unwrap(), 2);
    let company: String = conn.query_row("SELECT id FROM company", [], |r| r.get(0)).unwrap();
    let audit = |id: &str, item: Option<&str>, action: &str, fields: Option<&str>, user: Option<&str>| {
        conn.execute(
            "INSERT INTO vault_audit(id, item_id, company_id, action, changed_fields, user_id, at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1)",
            params![id, item, company, action, fields, user],
        )
    };
    assert!(audit("x1", Some("s"), "update", None, Some(ALEX)).is_err(), "unknown action");
    assert!(audit("x2", Some("s"), "edit", Some("sk-test-FAKE-0000"), Some(ALEX)).is_err(), "changed_fields holds names only");
    assert!(audit("x3", Some("s"), "edit", Some("Name"), Some(ALEX)).is_err(), "changed_fields holds names only");
    assert!(audit("x4", None, "reveal", None, Some(ALEX)).is_err(), "only a key export has no item");
    assert!(audit("x5", Some("s"), "key_export", None, Some(ALEX)).is_err(), "a key export has no item");
    assert!(audit("x6", Some("s"), "reveal", None, None).is_err(), "someone did it");
    assert!(audit("x7", Some("nope"), "reveal", None, Some(ALEX)).is_err(), "item_id references vault_items");
    audit("x8", Some("s"), "copy", None, Some(ALEX)).unwrap();
    drop(conn);

    rejected(store.write(|tx| vault::insert_audit(tx, &NewAudit { changed_fields: &["secret"], user_id: Some(ALEX), ..NewAudit::new("b1", Some("s"), AuditAction::Edit, 1) })), vault::INVALID_AUDIT);
    rejected(store.write(|tx| vault::insert_audit(tx, &NewAudit::new("b2", Some("s"), AuditAction::Reveal, 1))), vault::INVALID_AUDIT);
    rejected(store.write(|tx| vault::insert_audit(tx, &NewAudit { user_id: Some(ALEX), ..NewAudit::new("b3", None, AuditAction::Reveal, 1) })), vault::INVALID_AUDIT);
    rejected(store.write(|tx| vault::insert_audit(tx, &NewAudit { user_id: Some(ALEX), ..NewAudit::new("b4", Some("nope"), AuditAction::Reveal, 1) })), vault::NOT_FOUND);
}

#[test]
fn the_audit_filters_and_pages_with_a_cursor() {
    let (_dir, store) = store();
    add(&store, "c", None, VaultKind::Secret, "Company key", Some("COMPANY_KEY")).unwrap();
    add(&store, "p", Some(ALPHA), VaultKind::Secret, "Alpha key", Some("ALPHA_KEY")).unwrap();
    add(&store, "q", Some(BETA), VaultKind::Url, "Beta site", None).unwrap();
    store
        .write(|tx| {
            // Five rows at the same time, to page through a tie.
            let rows: [(&str, &str, AuditAction, i64, bool); 9] = [
                ("01", "c", AuditAction::Create, 10, true),
                ("02", "p", AuditAction::Create, 20, true),
                ("03", "q", AuditAction::Create, 30, true),
                ("04", "p", AuditAction::Deliver, 40, false),
                ("05", "c", AuditAction::Deliver, 40, false),
                ("06", "p", AuditAction::Reveal, 40, true),
                ("07", "c", AuditAction::Copy, 40, true),
                ("08", "q", AuditAction::ToolRead, 40, false),
                ("09", "p", AuditAction::Replace, 50, true),
            ];
            for (id, item, action, at, by_user) in rows {
                let audit = if by_user {
                    NewAudit { user_id: Some(ALEX), via: Some("Desktop"), ..NewAudit::new(id, Some(item), action, at) }
                } else {
                    NewAudit { employee_id: Some("forge"), run_id: Some("run-1"), task_id: Some("task-1"), ticket_id: Some("ticket-42"), ..NewAudit::new(id, Some(item), action, at) }
                };
                vault::insert_audit(tx, &audit)?;
            }
            Ok(())
        })
        .unwrap();
    let ids = |filter: AuditFilter<'static>| store.read(move |c| vault::list_audit(c, &filter)).unwrap().into_iter().map(|a| a.id).collect::<Vec<_>>();

    assert_eq!(ids(AuditFilter::default()), ["09", "08", "07", "06", "05", "04", "03", "02", "01"]);
    assert_eq!(ids(AuditFilter { item_id: Some("p"), ..Default::default() }), ["09", "06", "04", "02"]);
    assert_eq!(ids(AuditFilter { scope: Some(VaultScope::Company), ..Default::default() }), ["07", "05", "01"]);
    assert_eq!(ids(AuditFilter { scope: Some(VaultScope::Project(BETA)), ..Default::default() }), ["08", "03"]);
    assert_eq!(ids(AuditFilter { scope: Some(VaultScope::ProjectAndCompany(ALPHA)), ..Default::default() }), ["09", "07", "06", "05", "04", "02", "01"]);
    assert_eq!(ids(AuditFilter { actor: Some(AuditActor::Employee("forge")), ..Default::default() }), ["08", "05", "04"]);
    assert_eq!(ids(AuditFilter { actor: Some(AuditActor::User(ALEX)), action: Some(AuditAction::Create), ..Default::default() }), ["03", "02", "01"]);
    assert_eq!(ids(AuditFilter { since_ms: Some(40), item_id: Some("c"), ..Default::default() }), ["07", "05"]);

    // "Load older": three per page, through the tie at 40.
    let mut pages = Vec::new();
    let mut before = None;
    loop {
        let filter = AuditFilter { before: before.clone(), limit: 3, ..Default::default() };
        let page = store.read(move |c| vault::list_audit(c, &filter)).unwrap();
        if page.is_empty() {
            break;
        }
        before = page.last().map(|a| a.cursor());
        pages.push(page.into_iter().map(|a| a.id).collect::<Vec<_>>());
    }
    assert_eq!(pages, [["09", "08", "07"], ["06", "05", "04"], ["03", "02", "01"]]);

    let delivered = store.read(|c| vault::list_audit(c, &AuditFilter { item_id: Some("p"), action: Some(AuditAction::Deliver), ..Default::default() })).unwrap();
    let d = &delivered[0];
    assert_eq!(
        (d.employee_id.as_deref(), d.run_id.as_deref(), d.task_id.as_deref(), d.ticket_id.as_deref(), d.user_id.as_deref(), d.item_kind),
        (Some("forge"), Some("run-1"), Some("task-1"), Some("ticket-42"), None, Some(VaultKind::Secret))
    );
}

#[test]
fn the_audit_pages_by_seq_newest_first_without_overlap() {
    let (_dir, store) = store();
    add(&store, "c", None, VaultKind::Secret, "Company key", Some("COMPANY_KEY")).unwrap();
    add(&store, "p", Some(ALPHA), VaultKind::Text, "Alpha note", None).unwrap();
    // Seven rows; their times are not in the order they were written (a clock
    // that went back), so only seq gives the order.
    let rows = store
        .write(|tx| {
            let mut rows = Vec::new();
            for (n, at) in [50, 10, 40, 40, 20, 60, 30].into_iter().enumerate() {
                let id = format!("r{n}");
                let item = if n % 2 == 0 { "c" } else { "p" };
                rows.push(vault::insert_audit(tx, &NewAudit { user_id: Some(ALEX), ..NewAudit::new(&id, Some(item), AuditAction::Reveal, at) })?);
            }
            Ok(rows)
        })
        .unwrap();
    let seqs = rows.iter().map(|r| r.seq).collect::<Vec<_>>();
    assert!(seqs.windows(2).all(|w| w[0] < w[1]), "seq follows the writes: {seqs:?}");
    assert_eq!(store.read(|c| vault::list_audit(c, &AuditFilter::default())).unwrap()[0].id, "r6", "newest written first");

    let page = |before_seq: Option<i64>, item: Option<&'static str>| {
        let filter = AuditFilter { before_seq, item_id: item, limit: 3, ..Default::default() };
        store.read(move |c| vault::list_audit(c, &filter)).unwrap()
    };
    let mut pages = Vec::new();
    let mut before = None;
    loop {
        let rows = page(before, None);
        if rows.is_empty() {
            break;
        }
        assert!(rows.windows(2).all(|w| w[0].seq > w[1].seq));
        before = rows.last().map(|r| r.seq);
        pages.push(rows.into_iter().map(|r| r.id).collect::<Vec<_>>());
    }
    assert_eq!(pages, [vec!["r6", "r5", "r4"], vec!["r3", "r2", "r1"], vec!["r0"]]);

    // With a filter: the item's rows only, still by seq.
    let first = page(None, Some("c"));
    assert_eq!(first.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), ["r6", "r4", "r2"]);
    let next = page(first.last().map(|r| r.seq), Some("c"));
    assert_eq!(next.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), ["r0"]);

    // The text id stays unique next to the seq.
    let conn = raw(&store);
    let company: String = conn.query_row("SELECT id FROM company", [], |r| r.get(0)).unwrap();
    assert!(conn.execute("INSERT INTO vault_audit(id, item_id, company_id, action, user_id, at_ms) VALUES ('r0', 'c', ?1, 'copy', ?2, 1)", params![company, ALEX]).is_err());
}

#[test]
fn a_new_value_keeps_the_previous_one_encrypted_for_the_redactor() {
    let (_dir, store) = store();
    assert!(!store.read(vault::any_ciphertext).unwrap());
    add(&store, "s", Some(ALPHA), VaultKind::Secret, "Token", Some("TOKEN")).unwrap();
    add(&store, "t", None, VaultKind::Text, "Note", None).unwrap();
    assert!(store.read(vault::any_ciphertext).unwrap());
    // A new name keeps no version; a new value does, with the kind it had.
    store.write(|tx| vault::update(tx, "s", VaultUpdate { name: Some("API token"), ..no_change(ALEX) })).unwrap();
    store.write(|tx| vault::update(tx, "s", VaultUpdate { sealed: Some(seal("v2", "k1")), ..no_change(ALEX) })).unwrap();
    store.write(|tx| vault::update(tx, "s", VaultUpdate { sealed: Some(seal("v3", "k2")), ..no_change(ROBIN) })).unwrap();
    store.write(|tx| vault::update(tx, "t", VaultUpdate { kind: Some(VaultKind::Url), sealed: Some(seal("https://x", "k1")), ..no_change(ALEX) })).unwrap();
    store.write(|tx| vault::archive(tx, "t", ALEX, 300)).unwrap();

    let previous = store.read(|c| vault::previous_values(c, "s")).unwrap();
    let summary = previous.iter().map(|v| (v.version, v.sealed.clone(), v.kind)).collect::<Vec<_>>();
    assert_eq!(summary, [(2, seal("v2", "k1"), VaultKind::Secret), (1, seal("Token", "k1"), VaultKind::Secret)], "newest first");
    assert!(previous.iter().all(|v| matches!(v.source, ValueSource::Previous(_)) && v.name == "API token" && v.project_id.as_deref() == Some(ALPHA)));

    // Every value of every item, archived ones too.
    let all = store.read(vault::redaction_values).unwrap();
    let all = all.iter().map(|v| (v.item_id.as_str(), v.version, v.kind, v.source == ValueSource::Current, v.archived)).collect::<Vec<_>>();
    assert_eq!(
        all,
        [
            ("s", 3, VaultKind::Secret, true, false),
            ("s", 2, VaultKind::Secret, false, false),
            ("s", 1, VaultKind::Secret, false, false),
            ("t", 2, VaultKind::Url, true, true),
            ("t", 1, VaultKind::Text, false, true),
        ]
    );
    assert_eq!(store.read(vault::key_ids).unwrap(), [("k1".to_string(), 4), ("k2".to_string(), 1)]);

    // A previous value only ever changes its key, and is never deleted.
    let conn = raw(&store);
    for sql in [
        "UPDATE vault_item_versions SET version = 9",
        "UPDATE vault_item_versions SET kind = 'text'",
        "UPDATE vault_item_versions SET item_id = 't'",
        "DELETE FROM vault_item_versions",
    ] {
        assert!(conn.execute(sql, []).is_err(), "{sql}");
    }
    conn.execute("UPDATE vault_item_versions SET key_id = 'k9' WHERE key_id = 'k9'", []).unwrap();
    // Nothing assumes a version is unique: a restore can bring an old number back.
    conn.execute(
        "INSERT INTO vault_item_versions(item_id, kind, version, ciphertext, nonce, key_id, replaced_at_ms) VALUES ('s', 'secret', 2, x'01', x'02', 'k0', 1)",
        [],
    )
    .unwrap();
    assert!(conn.execute("INSERT INTO vault_item_versions(item_id, kind, version, ciphertext, nonce, key_id, replaced_at_ms) VALUES ('nope', 'secret', 1, x'01', x'02', 'k0', 1)", []).is_err());
    for (why, values) in [
        ("no ciphertext", "'secret', 1, x'', x'02', 'k0'"),
        ("no nonce", "'secret', 1, x'01', x'', 'k0'"),
        ("no key id", "'secret', 1, x'01', x'02', ''"),
        ("unknown kind", "'note', 1, x'01', x'02', 'k0'"),
        ("version 0", "'secret', 0, x'01', x'02', 'k0'"),
    ] {
        let sql = format!("INSERT INTO vault_item_versions(item_id, kind, version, ciphertext, nonce, key_id, replaced_at_ms) VALUES ('s', {values}, 1)");
        assert!(conn.execute(&sql, []).is_err(), "{why}");
    }
    drop(conn);
    assert_eq!(store.read(|c| vault::previous_values(c, "s")).unwrap().len(), 3);
}

#[test]
fn reencrypting_a_key_changes_all_its_rows_in_one_transaction() {
    let (_dir, store) = store();
    add(&store, "a", None, VaultKind::Secret, "A", Some("A")).unwrap();
    add(&store, "b", Some(ALPHA), VaultKind::Text, "B", None).unwrap();
    add(&store, "c", None, VaultKind::Url, "C", None).unwrap();
    // A previous value of "a" under k1, its current one under k0.
    store.write(|tx| vault::update(tx, "a", VaultUpdate { sealed: Some(seal("A2", "k0")), ..no_change(ALEX) })).unwrap();
    store.write(|tx| vault::archive(tx, "c", ALEX, 150)).unwrap();
    store
        .write(|tx| vault::insert(tx, NewVaultItem { sealed: seal("D", "k0"), ..new("d", None, VaultKind::Text, "D", None) }))
        .unwrap();
    let before = store.read(|c| vault::get(c, "b")).unwrap().unwrap();

    // A failing seal halfway leaves every row as it was.
    let failed = store.write(|tx| {
        let mut calls = 0;
        vault::reencrypt(tx, "k1", |_| {
            calls += 1;
            if calls == 2 { Err(StorageError::Rejected("cipher failed".into())) } else { Ok(seal("x", "k2")) }
        })
    });
    assert!(failed.is_err());
    assert_eq!(store.read(vault::key_ids).unwrap(), [("k0".to_string(), 2), ("k1".to_string(), 3)]);

    let (changed, seen) = store
        .write(|tx| {
            let mut seen = Vec::new();
            let changed = vault::reencrypt(tx, "k1", |value| {
                seen.push((value.item_id.clone(), value.version, value.source == ValueSource::Current));
                Ok(Sealed { nonce: vec![9; 12], ..seal(&value.name, "k2") })
            })?;
            Ok((changed, seen))
        })
        .unwrap();
    assert_eq!(changed, 3, "archived and previous values too");
    assert_eq!(seen, [("a".to_string(), 1, false), ("b".to_string(), 1, true), ("c".to_string(), 1, true)]);
    assert_eq!(store.read(vault::key_ids).unwrap(), [("k0".to_string(), 2), ("k2".to_string(), 3)]);
    let after = store.read(|c| vault::get(c, "b")).unwrap().unwrap();
    assert_eq!((after.sealed.key_id.as_str(), after.sealed.nonce.clone()), ("k2", vec![9; 12]));
    assert_eq!((after.version, after.updated_at_ms, after.kind), (before.version, before.updated_at_ms, before.kind), "a rotation is no edit");
    let old = store.read(|c| vault::previous_values(c, "a")).unwrap();
    assert_eq!((old.len(), old[0].sealed.key_id.as_str(), old[0].version), (1, "k2", 1));
    assert_eq!(store.read(|c| vault::get(c, "a")).unwrap().unwrap().sealed, seal("A2", "k0"), "other keys stay");
}

#[test]
fn the_database_files_hold_no_plaintext_value() {
    const VALUE: &str = "sk-test-FAKE-0000";
    const NEXT: &str = "sk-test-FAKE-0001";
    const NOTE: &str = "plain note FAKE-0002";
    let (dir, store) = store();
    let sealed = seal(VALUE, "k1");
    let ciphertext = sealed.ciphertext.clone();
    store
        .write(move |tx| {
            vault::insert(tx, NewVaultItem { sealed, description: Some("CI token"), ..new("s", Some(ALPHA), VaultKind::Secret, "Token", Some("API_TOKEN")) })?;
            vault::insert_audit(tx, &NewAudit { user_id: Some(ALEX), ..NewAudit::new("a1", Some("s"), AuditAction::Create, 1) })?;
            vault::insert_audit(tx, &NewAudit { user_id: Some(ALEX), ..NewAudit::new("a2", Some("s"), AuditAction::Reveal, 2) })?;
            // The fake value becomes a previous value; a text item is encrypted too.
            vault::update(tx, "s", VaultUpdate { sealed: Some(seal(NEXT, "k1")), ..no_change(ALEX) })?;
            vault::insert(tx, NewVaultItem { sealed: seal(NOTE, "k1"), ..new("t", None, VaultKind::Text, "Note", None) })?;
            Ok(())
        })
        .unwrap();
    // Once in the WAL and once checkpointed into the database file.
    let files = |dir: &std::path::Path| {
        let mut all = Vec::new();
        for entry in std::fs::read_dir(dir.join("data")).unwrap() {
            let path = entry.unwrap().path();
            if path.is_file() {
                all.push((path.clone(), std::fs::read(&path).unwrap()));
            }
        }
        all
    };
    let contains = |hay: &[u8], needle: &[u8]| hay.windows(needle.len()).any(|w| w == needle);
    let check = |stage: &str| {
        let all = files(dir.path());
        assert!(all.iter().any(|(_, bytes)| contains(bytes, &ciphertext)), "{stage}: the ciphertext is stored");
        for (path, bytes) in &all {
            for plain in [VALUE, NEXT, NOTE] {
                assert!(!contains(bytes, plain.as_bytes()), "{stage}: plaintext {plain:?} in {}", path.display());
            }
        }
    };
    check("in the WAL");
    drop(store);
    let conn = Connection::open(dir.path().join("data/aurelius.sqlite")).unwrap();
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)").unwrap();
    drop(conn);
    check("checkpointed");
}

/// How many previous values are stored, of any item.
fn previous_count(store: &Store) -> i64 {
    store.read(|c| Ok(c.query_row("SELECT COUNT(*) FROM vault_item_versions", [], |r| r.get(0))?)).unwrap()
}

#[test]
fn a_refused_update_writes_nothing_even_when_the_caller_commits() {
    let (_dir, store) = store();
    add(&store, "a", None, VaultKind::Secret, "First token", Some("FIRST_TOKEN")).unwrap();
    add(&store, "b", None, VaultKind::Secret, "Second token", Some("SECOND_TOKEN")).unwrap();
    add(&store, "t", None, VaultKind::Text, "Docs", None).unwrap();
    add(&store, "u", None, VaultKind::Text, "Notes", None).unwrap();
    // As the daemon does: a `Rejected` is an answer, so the transaction commits.
    let committed = |id: &'static str, change: VaultUpdate<'static>| store.write(move |tx| Ok(vault::update(tx, id, change))).unwrap();
    let cases: [(&str, VaultUpdate<'static>, &str); 4] = [
        ("b", VaultUpdate { env_name: Some(Some("FIRST_TOKEN")), sealed: Some(seal("sk-test-FAKE-0003", "k1")), ..no_change(ROBIN) }, vault::ENV_NAME_TAKEN),
        ("b", VaultUpdate { name: Some("first TOKEN"), sealed: Some(seal("sk-test-FAKE-0004", "k1")), ..no_change(ROBIN) }, vault::NAME_TAKEN),
        ("t", VaultUpdate { name: Some("Notes"), kind: Some(VaultKind::Url), sealed: Some(seal("https://example.com", "k1")), ..no_change(ROBIN) }, vault::NAME_TAKEN),
        ("b", VaultUpdate { sealed: Some(seal("sk-test-FAKE-0005", "k1")), expected_version: Some(7), ..no_change(ROBIN) }, vault::VERSION_MISMATCH),
    ];
    for (id, change, why) in cases {
        rejected(committed(id, change), why);
    }
    assert_eq!(previous_count(&store), 0, "no previous value is left behind");
    let b = store.read(|c| vault::get(c, "b")).unwrap().unwrap();
    assert_eq!((b.name.as_str(), b.env_name.as_deref(), b.version, b.sealed.clone(), b.updated_by_user.as_str()), ("Second token", Some("SECOND_TOKEN"), 1, seal("Second token", "k1"), ALEX));
    assert_eq!(store.read(|c| vault::get(c, "t")).unwrap().unwrap().kind, VaultKind::Text);

    // The same change with a free env name goes through and keeps the old value once.
    let ok = committed("b", VaultUpdate { env_name: Some(Some("OTHER_TOKEN")), sealed: Some(seal("sk-test-FAKE-0003", "k1")), ..no_change(ROBIN) }).unwrap();
    assert_eq!((ok.item.version, ok.changed.as_slice()), (2, ["env_name", "value"].as_slice()));
    assert_eq!(previous_count(&store), 1);
    let previous = store.read(|c| vault::previous_values(c, "b")).unwrap();
    assert_eq!((previous[0].version, previous[0].sealed.clone()), (1, seal("Second token", "k1")));
}

#[test]
fn env_names_lists_the_live_ones_once_per_scope() {
    let (_dir, store) = store();
    add(&store, "c1", None, VaultKind::Secret, "GitHub token", Some("GITHUB_TOKEN")).unwrap();
    add(&store, "c2", None, VaultKind::Url, "Staging", Some("STAGING_URL")).unwrap();
    add(&store, "c3", None, VaultKind::Text, "Notes", None).unwrap();
    add(&store, "c4", None, VaultKind::Secret, "Gone", Some("GONE_KEY")).unwrap();
    store.write(|tx| vault::archive(tx, "c4", ALEX, 150)).unwrap();
    add(&store, "p1", Some(ALPHA), VaultKind::Secret, "GitHub token", Some("GITHUB_TOKEN")).unwrap();
    add(&store, "p2", Some(ALPHA), VaultKind::Secret, "Alpha DSN", Some("ALPHA_DSN")).unwrap();
    add(&store, "q1", Some(BETA), VaultKind::Secret, "Beta key", Some("BETA_KEY")).unwrap();

    let names = |scope: Option<VaultScope<'static>>| store.read(move |c| vault::env_names(c, scope)).unwrap();
    assert_eq!(names(None), ["ALPHA_DSN", "BETA_KEY", "GITHUB_TOKEN", "STAGING_URL"], "every scope, archived ones out, each once");
    assert_eq!(names(Some(VaultScope::Company)), ["GITHUB_TOKEN", "STAGING_URL"]);
    assert_eq!(names(Some(VaultScope::Project(ALPHA))), ["ALPHA_DSN", "GITHUB_TOKEN"]);
    assert_eq!(names(Some(VaultScope::ProjectAndCompany(ALPHA))), ["ALPHA_DSN", "GITHUB_TOKEN", "STAGING_URL"]);
    assert!(names(Some(VaultScope::Project(OLD))).is_empty());
}

#[test]
fn the_audit_keeps_the_version_of_the_value() {
    let (_dir, store) = store();
    add(&store, "s", Some(ALPHA), VaultKind::Secret, "Token", Some("TOKEN")).unwrap();
    store
        .write(|tx| {
            let by = |id, action, at| NewAudit { user_id: Some(ALEX), ..NewAudit::new(id, Some("s"), action, at) };
            vault::insert_audit(tx, &by("a1", AuditAction::Create, 1))?;
            vault::insert_audit(tx, &by("a2", AuditAction::Reveal, 2))?;
            let updated = vault::update(tx, "s", VaultUpdate { sealed: Some(seal("sk-test-FAKE-0006", "k1")), ..no_change(ALEX) })?;
            vault::insert_audit(tx, &NewAudit { changed_fields: &updated.changed, ..by("a3", updated.action().unwrap(), 3) })?;
            vault::insert_audit(tx, &by("a4", AuditAction::Copy, 4))?;
            // A run that got version 1 before the replace says so.
            let run = NewAudit { employee_id: Some("forge"), run_id: Some("run-1"), version: Some(1), ..NewAudit::new("a5", Some("s"), AuditAction::Deliver, 5) };
            vault::insert_audit(tx, &run)?;
            vault::insert_audit(tx, &NewAudit { action: AuditAction::ToolRead, id: "a6", version: None, ..run })?;
            vault::insert_audit(tx, &NewAudit { user_id: Some(ALEX), ..NewAudit::new("a7", None, AuditAction::KeyExport, 7) })?;
            Ok(())
        })
        .unwrap();
    let rows = store.read(|c| vault::list_audit(c, &AuditFilter::default())).unwrap();
    let got = rows.iter().map(|r| (r.id.as_str(), r.version)).collect::<Vec<_>>();
    assert_eq!(got, [("a7", None), ("a6", Some(2)), ("a5", Some(1)), ("a4", Some(2)), ("a3", Some(2)), ("a2", Some(1)), ("a1", Some(1))]);

    rejected(store.write(|tx| vault::insert_audit(tx, &NewAudit { user_id: Some(ALEX), version: Some(1), ..NewAudit::new("b1", None, AuditAction::KeyExport, 8) })), vault::INVALID_AUDIT);
    rejected(store.write(|tx| vault::insert_audit(tx, &NewAudit { user_id: Some(ALEX), version: Some(0), ..NewAudit::new("b2", Some("s"), AuditAction::Reveal, 8) })), vault::INVALID_AUDIT);
    let conn = raw(&store);
    let company: String = conn.query_row("SELECT id FROM company", [], |r| r.get(0)).unwrap();
    let insert = |id: &str, item: Option<&str>, action: &str, version: Option<i64>| {
        conn.execute(
            "INSERT INTO vault_audit(id, item_id, company_id, action, user_id, version, at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1)",
            params![id, item, company, action, ALEX, version],
        )
    };
    assert!(insert("x1", None, "key_export", Some(1)).is_err(), "a key export has no version");
    assert!(insert("x2", Some("s"), "reveal", Some(0)).is_err(), "a version is positive");
    insert("x3", Some("s"), "reveal", None).unwrap();
}

#[test]
fn an_audit_page_carries_the_actor_names_and_pages_by_seq_within_a_scope() {
    let (_dir, store) = store();
    add(&store, "c", None, VaultKind::Secret, "Company key", Some("COMPANY_KEY")).unwrap();
    add(&store, "p", Some(ALPHA), VaultKind::Secret, "Alpha key", Some("ALPHA_KEY")).unwrap();
    add(&store, "q", Some(BETA), VaultKind::Text, "Beta note", None).unwrap();
    let employee = store
        .write(|tx| {
            let (id, name) = ("00000000-0000-7000-8000-0000000000e1".to_string(), "Builder".to_string());
            tx.execute("INSERT INTO scopes(id, kind, parent_id, owner_id) VALUES ('s-e1', 'employee', (SELECT id FROM scopes WHERE kind = 'global'), ?1)", [&id])?;
            tx.execute(
                "INSERT INTO employees(id, scope_id, name, role, config_json, status, version, created_at_ms, updated_at_ms)
                 VALUES (?1, 's-e1', ?2, 'r', '{}', 'active', 1, 1, 1)",
                [&id, &name],
            )?;
            // Eight rows over three scopes, by a board member and an employee
            // (one no longer in the company).
            for (n, item) in ["p", "c", "p", "q", "p", "c", "p", "p"].into_iter().enumerate() {
                let audit_id = format!("r{n}");
                let audit = match n % 3 {
                    0 => NewAudit { user_id: Some(ROBIN), via: Some("Desktop"), ..NewAudit::new(&audit_id, Some(item), AuditAction::Reveal, 10) },
                    1 => NewAudit { employee_id: Some(&id), run_id: Some("run-1"), ..NewAudit::new(&audit_id, Some(item), AuditAction::Deliver, 10) },
                    _ => NewAudit { employee_id: Some("gone-employee"), run_id: Some("run-2"), ..NewAudit::new(&audit_id, Some(item), AuditAction::ToolRead, 10) },
                };
                vault::insert_audit(tx, &audit)?;
            }
            Ok((id, name))
        })
        .unwrap();

    // Alpha's rows only, two per page, as `vault.audit` asks (limit + 1 for `more`).
    let page = |before_seq: Option<i64>| {
        let filter = AuditFilter { scope: Some(VaultScope::Project(ALPHA)), before_seq, limit: 2, ..Default::default() };
        store.read(move |c| vault::list_audit(c, &filter)).unwrap()
    };
    let mut pages = Vec::new();
    let mut before = None;
    loop {
        let rows = page(before);
        if rows.is_empty() {
            break;
        }
        assert!(rows.iter().all(|r| r.project_id.as_deref() == Some(ALPHA) && r.item_name.as_deref() == Some("Alpha key")));
        before = rows.last().map(|r| r.seq);
        pages.push(rows.iter().map(|r| r.id.clone()).collect::<Vec<_>>());
    }
    assert_eq!(pages, [vec!["r7", "r6"], vec!["r4", "r2"], vec!["r0"]], "no overlap, no row of another scope");
    let company = |before_seq| {
        let filter = AuditFilter { scope: Some(VaultScope::Company), before_seq, ..Default::default() };
        store.read(move |c| vault::list_audit(c, &filter)).unwrap().into_iter().map(|r| r.id).collect::<Vec<_>>()
    };
    assert_eq!(company(None), ["r5", "r1"]);
    let r5 = store.read(|c| vault::list_audit(c, &AuditFilter { item_id: Some("c"), limit: 1, ..Default::default() })).unwrap();
    assert_eq!(r5[0].id, "r5");
    assert_eq!(company(Some(r5[0].seq)), ["r1"], "before_seq is strict");

    // Every row names its actor without another query.
    let rows = store.read(|c| vault::list_audit(c, &AuditFilter::default())).unwrap();
    let actor = |id: &str| {
        let r = rows.iter().find(|r| r.id == id).unwrap();
        (r.username.clone(), r.employee_name.clone())
    };
    assert_eq!(actor("r6"), (Some("Robin".to_string()), None));
    assert_eq!(actor("r7"), (None, Some(employee.1.clone())));
    assert_eq!(actor("r5"), (None, None), "an employee that no longer exists");
}
