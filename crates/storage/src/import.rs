//! One-time import of a v0.6 data folder into a fresh v0.7 one (`aurelius-import`).
//!
//! Copies projects, managers, memories, ideas and recurring tasks. Everything is read
//! first (the v0.6 database is opened read-only), then written in ONE transaction,
//! into the last protocol 2 schema (managers per project); migration 14 then makes
//! the company of it like for any 2.x database.

use crate::{StorageError, Store, blobs, employees, events, memory, migrate, PreparedBlob};
use aurelius_domain::{ActorKind, ConversationId, EmployeeId, MemoryId, ProjectId};
use rusqlite::{Connection, OpenFlags, OptionalExtension, Transaction, params};
use serde_json::{Value, json};
use std::collections::HashSet;
use std::path::Path;

/// The schema version of the final v0.6 database. Released v0.6 servers stopped at 27 (no `ideas.kind`);
/// 28 only exists on dev builds made between v0.6.13 and v0.7. Both are read.
const V06_VERSION: i64 = 28;
const V06_RELEASED: i64 = 27;

fn is_v06(version: Option<i64>) -> bool {
    matches!(version, Some(v) if v == V06_RELEASED || v == V06_VERSION)
}

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("{0}")]
    Storage(#[from] StorageError),
    #[error("file error: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Refused(String),
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct ImportReport {
    pub projects: usize,
    pub managers: usize,
    pub memories: usize,
    pub ideas: usize,
    pub recurring_tasks: usize,
    pub skipped: Vec<String>,
}

type Put<'a> = &'a mut dyn FnMut(&[u8], &str) -> Result<Option<PreparedBlob>, StorageError>;

struct ProjectPlan {
    id: String,
    name: String,
    description: Option<PreparedBlob>,
    status: String,
    folder: Option<String>,
    created_at_ms: i64,
    updated_at_ms: i64,
}

struct ManagerPlan {
    id: String,
    project_id: String,
    name: String,
    role: String,
    instructions: Option<PreparedBlob>,
    identity: Option<PreparedBlob>,
    config: Value,
    status: String,
    folder: Option<String>,
    created_at_ms: i64,
    updated_at_ms: i64,
}

/// Where a memory lives: the owner id of its v0.6 scope.
enum ScopeRef {
    Global,
    Project(String),
    Manager(String),
}

struct MemoryPlan {
    id: String,
    scope: ScopeRef,
    kind: String,
    text: String,
    blob: Option<PreparedBlob>,
    status: String,
    confidence: f64,
    importance: f64,
    pinned: i64,
    valid_from_ms: i64,
    valid_until_ms: Option<i64>,
    supersedes_id: Option<String>,
    conflicts_with_id: Option<String>,
    extraction_version: String,
    created_at_ms: i64,
    decided_by: Option<String>,
    locator_json: String,
}

struct IdeaPlan {
    id: String,
    project_id: String,
    title: String,
    note: Option<String>,
    status: String,
    manager_id: Option<String>,
    version: i64,
    created_at_ms: i64,
    updated_at_ms: i64,
    kind: String,
}

struct SchedulePlan {
    id: String,
    project_id: String,
    manager_id: String,
    title: String,
    goal: String,
    criteria_json: String,
    run_json: Option<String>,
    recurrence_json: String,
    paused: i64,
    anchor_ms: i64,
    next_run_ms: Option<i64>,
    last_run_ms: Option<i64>,
    last_skipped_ms: Option<i64>,
    created_at_ms: i64,
    updated_at_ms: i64,
}

#[derive(Default)]
struct Plan {
    projects: Vec<ProjectPlan>,
    managers: Vec<ManagerPlan>,
    memories: Vec<MemoryPlan>,
    ideas: Vec<IdeaPlan>,
    schedules: Vec<SchedulePlan>,
    skipped: Vec<String>,
}

impl Plan {
    fn report(&self) -> ImportReport {
        ImportReport {
            projects: self.projects.len(),
            managers: self.managers.len(),
            memories: self.memories.len(),
            ideas: self.ideas.len(),
            recurring_tasks: self.schedules.len(),
            skipped: self.skipped.clone(),
        }
    }
}

/// v0.6 `ManagerConfig` JSON to the v0.7 one: `worker_policy` becomes `task_policy`
/// (delegation, depth, sequential only), `voice_config` is dropped.
pub fn map_manager_config(old: &Value) -> Value {
    let inherit = || json!({ "mode": "inherit" });
    let field = |name: &str| old.get(name).filter(|v| !v.is_null()).cloned().unwrap_or_else(inherit);
    let wp = &old["worker_policy"];
    json!({
        "identity": old.get("identity").and_then(Value::as_str).unwrap_or(""),
        "memory_policy": field("memory_policy"),
        "tool_policy": field("tool_policy"),
        "autonomy_policy": field("autonomy_policy"),
        "runtime_selection": field("runtime_selection"),
        "task_policy": {
            "delegation_allowed": wp["delegation_allowed"].as_bool().unwrap_or(true),
            "max_depth": wp["max_depth"].as_u64().unwrap_or(2),
            "sequential_tasks": wp["sequential_tasks"].as_bool().unwrap_or(false),
        },
    })
}

/// Idea kinds are unchecked in v0.6; v0.7 knows `feature` and `bug`.
fn map_idea_kind(kind: &str) -> &'static str {
    if kind == "bug" { "bug" } else { "feature" }
}

fn load_blob(conn: &Connection, from: &Path, id: &str) -> Result<Option<(Vec<u8>, String)>, ImportError> {
    let row: Option<(String, String)> = conn
        .query_row("SELECT storage_key, media_type FROM blobs WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?;
    let Some((key, media_type)) = row else { return Ok(None) };
    match std::fs::read(from.join("blobs").join(&key)) {
        Ok(bytes) => Ok(Some((bytes, media_type))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn copy_blob(
    conn: &Connection,
    from: &Path,
    put: Put<'_>,
    id: Option<String>,
    what: &str,
    skipped: &mut Vec<String>,
) -> Result<Option<PreparedBlob>, ImportError> {
    let Some(id) = id else { return Ok(None) };
    match load_blob(conn, from, &id)? {
        Some((bytes, media_type)) => Ok(put(&bytes, &media_type)?),
        None => {
            skipped.push(format!("{what}: the stored text is missing, copied without it"));
            Ok(None)
        }
    }
}

fn read_v06(from: &Path, put: Put<'_>) -> Result<Plan, ImportError> {
    let db = from.join("data").join("aurelius.sqlite");
    if !db.is_file() {
        return Err(ImportError::Refused(format!("no v0.6 database at {}", db.display())));
    }
    let conn = Connection::open_with_flags(&db, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)?;
    let version: Option<i64> = conn
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |r| r.get(0))
        .map_err(|_| ImportError::Refused(format!("{} is not an Aurelius database", db.display())))?;
    if !is_v06(version) {
        return Err(ImportError::Refused(format!(
            "the database has schema version {version:?}; open it with Aurelius v0.6 first so it reaches version {V06_RELEASED}"
        )));
    }

    let mut plan = Plan::default();

    let rows: Vec<(String, String, Option<String>, String, Option<String>, i64, i64)> = conn
        .prepare("SELECT id, name, description_blob_id, status, folder, created_at_ms, updated_at_ms FROM projects ORDER BY created_at_ms, id")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)))?
        .collect::<Result<_, _>>()?;
    for (id, name, blob, status, folder, created_at_ms, updated_at_ms) in rows {
        let description = copy_blob(&conn, from, put, blob, &format!("project {name}"), &mut plan.skipped)?;
        plan.projects.push(ProjectPlan { id, name, description, status, folder, created_at_ms, updated_at_ms });
    }
    let project_ids: HashSet<String> = plan.projects.iter().map(|p| p.id.clone()).collect();

    #[allow(clippy::type_complexity)]
    let rows: Vec<(String, String, String, String, Option<String>, Option<String>, String, String, Option<String>, i64, i64)> = conn
        .prepare(
            "SELECT id, project_id, name, role, identity_blob_id, instructions_blob_id, config_json, status, folder, created_at_ms, updated_at_ms
             FROM managers ORDER BY created_at_ms, id",
        )?
        .query_map([], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?, r.get(9)?, r.get(10)?))
        })?
        .collect::<Result<_, _>>()?;
    for (id, project_id, name, role, identity, instructions, config, status, folder, created_at_ms, updated_at_ms) in rows {
        if !project_ids.contains(&project_id) {
            plan.skipped.push(format!("manager {name}: its project is missing"));
            continue;
        }
        let identity = copy_blob(&conn, from, put, identity, &format!("manager {name} identity"), &mut plan.skipped)?;
        let instructions = copy_blob(&conn, from, put, instructions, &format!("manager {name} instructions"), &mut plan.skipped)?;
        let config = map_manager_config(&serde_json::from_str(&config).unwrap_or(Value::Null));
        plan.managers.push(ManagerPlan { id, project_id, name, role, instructions, identity, config, status, folder, created_at_ms, updated_at_ms });
    }
    let manager_ids: HashSet<String> = plan.managers.iter().map(|m| m.id.clone()).collect();

    // Memories: the scope is found through the v0.6 scope row (kind, owner id).
    #[allow(clippy::type_complexity)]
    let rows: Vec<(String, String, Option<String>, String, String, String, f64, f64, i64, i64, Option<i64>, Option<String>, Option<String>, String, i64, Option<String>, String, String)> = conn
        .prepare(
            "SELECT m.id, s.kind, s.owner_id, m.kind, m.content_blob_id, m.status, m.confidence, m.importance, m.pinned,
                    m.valid_from_ms, m.valid_until_ms, m.supersedes_id, m.conflicts_with_id, m.extraction_version, m.created_at_ms,
                    m.decided_by, m.locator_json, m.status
             FROM memory_items m JOIN scopes s ON s.id = m.scope_id ORDER BY m.created_at_ms, m.id",
        )?
        .query_map([], |r| {
            Ok((
                r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?, r.get(9)?, r.get(10)?,
                r.get(11)?, r.get(12)?, r.get(13)?, r.get(14)?, r.get(15)?, r.get(16)?, r.get(17)?,
            ))
        })?
        .collect::<Result<_, _>>()?;
    let (mut forgotten, mut no_owner, mut no_text) = (0, 0, 0);
    for (id, scope_kind, owner, kind, blob_id, status, confidence, importance, pinned, valid_from_ms, valid_until_ms, supersedes_id, conflicts_with_id, extraction_version, created_at_ms, decided_by, locator_json, _) in rows {
        if status == "forgotten" {
            forgotten += 1;
            continue;
        }
        let scope = match (scope_kind.as_str(), owner) {
            ("global", _) => ScopeRef::Global,
            ("project", Some(o)) if project_ids.contains(&o) => ScopeRef::Project(o),
            ("manager", Some(o)) if manager_ids.contains(&o) => ScopeRef::Manager(o),
            _ => {
                no_owner += 1;
                continue;
            }
        };
        let Some((bytes, media_type)) = load_blob(&conn, from, &blob_id)? else {
            no_text += 1;
            continue;
        };
        let text = String::from_utf8_lossy(&bytes).into_owned();
        let blob = put(&bytes, &media_type)?;
        plan.memories.push(MemoryPlan {
            id, scope, kind, text, blob, status, confidence, importance, pinned, valid_from_ms, valid_until_ms, supersedes_id,
            conflicts_with_id, extraction_version, created_at_ms, decided_by, locator_json,
        });
    }
    if forgotten > 0 {
        plan.skipped.push(format!("{forgotten} forgotten memories (their content was erased)"));
    }
    if no_owner > 0 {
        plan.skipped.push(format!("{no_owner} memories whose project or manager was not copied"));
    }
    if no_text > 0 {
        plan.skipped.push(format!("{no_text} memories whose stored text is missing"));
    }

    let rows: Vec<(String, String, String, Option<String>, String, Option<String>, i64, i64, i64, String)> = conn
        .prepare(if version == Some(V06_RELEASED) {
            "SELECT id, project_id, title, note, status, manager_id, version, created_at_ms, updated_at_ms, 'feature' FROM ideas ORDER BY created_at_ms, id"
        } else {
            "SELECT id, project_id, title, note, status, manager_id, version, created_at_ms, updated_at_ms, kind FROM ideas ORDER BY created_at_ms, id"
        })?
        .query_map([], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?, r.get(9)?))
        })?
        .collect::<Result<_, _>>()?;
    let mut idea_skips = 0;
    for (id, project_id, title, note, status, manager_id, version, created_at_ms, updated_at_ms, kind) in rows {
        if !project_ids.contains(&project_id) {
            idea_skips += 1;
            continue;
        }
        let manager_id = manager_id.filter(|m| manager_ids.contains(m));
        plan.ideas.push(IdeaPlan { id, project_id, title, note, status, manager_id, version, created_at_ms, updated_at_ms, kind: map_idea_kind(&kind).into() });
    }
    if idea_skips > 0 {
        plan.skipped.push(format!("{idea_skips} ideas whose project was not copied"));
    }

    #[allow(clippy::type_complexity)]
    let rows: Vec<(String, String, String, String, String, String, Option<String>, String, i64, i64, Option<i64>, Option<i64>, Option<i64>, i64, i64)> = conn
        .prepare(
            "SELECT id, project_id, manager_id, title, goal, criteria_json, run_json, recurrence_json, paused, anchor_ms,
                    next_run_ms, last_run_ms, last_skipped_ms, created_at_ms, updated_at_ms
             FROM task_schedules ORDER BY created_at_ms, id",
        )?
        .query_map([], |r| {
            Ok((
                r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?, r.get(9)?, r.get(10)?,
                r.get(11)?, r.get(12)?, r.get(13)?, r.get(14)?,
            ))
        })?
        .collect::<Result<_, _>>()?;
    let mut schedule_skips = 0;
    for (id, project_id, manager_id, title, goal, criteria_json, run_json, recurrence_json, paused, anchor_ms, next_run_ms, last_run_ms, last_skipped_ms, created_at_ms, updated_at_ms) in rows {
        if !project_ids.contains(&project_id) || !manager_ids.contains(&manager_id) {
            schedule_skips += 1;
            continue;
        }
        plan.schedules.push(SchedulePlan {
            id, project_id, manager_id, title, goal, criteria_json, run_json, recurrence_json, paused, anchor_ms, next_run_ms, last_run_ms,
            last_skipped_ms, created_at_ms, updated_at_ms,
        });
    }
    if schedule_skips > 0 {
        plan.skipped.push(format!("{schedule_skips} recurring tasks whose project or manager was not copied"));
    }
    Ok(plan)
}

fn rejected(what: &str, id: &str) -> StorageError {
    StorageError::Rejected(format!("{what} id {id} is not a valid id"))
}

fn apply(tx: &Transaction<'_>, plan: &Plan) -> Result<(), StorageError> {
    for p in &plan.projects {
        let id = ProjectId::parse(&p.id).map_err(|_| rejected("project", &p.id))?;
        if let Some(b) = &p.description {
            blobs::register(tx, b)?;
        }
        events::insert_project_projection(tx, id, &p.name, p.description.as_ref().map(|b| b.id.to_string()).as_deref())?;
        tx.execute(
            "UPDATE projects SET status = ?2, folder = ?3, created_at_ms = ?4, updated_at_ms = ?5, follow_org_chart = 0 WHERE id = ?1",
            params![p.id, p.status, p.folder, p.created_at_ms, p.updated_at_ms],
        )?;
    }

    for m in &plan.managers {
        let id = EmployeeId::parse(&m.id).map_err(|_| rejected("manager", &m.id))?;
        let project = ProjectId::parse(&m.project_id).map_err(|_| rejected("project", &m.project_id))?;
        let (project_scope, _) = employees::project_scope(tx, project)?.ok_or_else(|| rejected("project", &m.project_id))?;
        for b in [&m.instructions, &m.identity].into_iter().flatten() {
            blobs::register(tx, b)?;
        }
        insert_legacy_manager(tx, id, project, &project_scope, &m.name, &m.role, m.instructions.as_ref().map(|b| b.id.to_string()).as_deref(), &m.config)?;
        tx.execute(
            "UPDATE managers SET status = ?2, identity_blob_id = ?3, folder = ?4, created_at_ms = ?5, updated_at_ms = ?6 WHERE id = ?1",
            params![m.id, m.status, m.identity.as_ref().map(|b| b.id.to_string()), m.folder, m.created_at_ms, m.updated_at_ms],
        )?;
        tx.execute("UPDATE conversations SET created_at_ms = ?2 WHERE manager_id = ?1", params![m.id, m.created_at_ms])?;
    }

    if !plan.memories.is_empty() {
        // memory_sources.event_id is NOT NULL: one audit event stands for the whole import.
        let event = events::append(
            tx,
            events::NewEvent {
                aggregate_type: "import",
                aggregate_id: "v0.6".into(),
                aggregate_version: 1,
                event_type: "memory.accepted",
                project_id: None,
                actor_kind: ActorKind::System,
                actor_id: None,
                correlation_id: "import-v0.6".into(),
                causation_id: None,
                payload_blob_id: None,
                metadata: json!({ "imported_from": "v0.6" }),
                dedupe_key: "import-v0.6".into(),
            },
        )?;
        let global = events::ensure_global_scope(tx)?;
        let copied: HashSet<&str> = plan.memories.iter().map(|m| m.id.as_str()).collect();
        for m in &plan.memories {
            let id = MemoryId::parse(&m.id).map_err(|_| rejected("memory", &m.id))?;
            let scope = match &m.scope {
                ScopeRef::Global => global.clone(),
                ScopeRef::Project(o) => tx.query_row("SELECT scope_id FROM projects WHERE id = ?1", [o], |r| r.get(0))?,
                ScopeRef::Manager(o) => tx.query_row("SELECT scope_id FROM managers WHERE id = ?1", [o], |r| r.get(0))?,
            };
            let blob = m.blob.as_ref().ok_or_else(|| StorageError::Rejected("memory text was not stored".into()))?;
            blobs::register(tx, blob)?;
            // supersedes / conflicts are linked below, once every memory row exists.
            tx.execute(
                "INSERT INTO memory_items(id, scope_id, kind, content_blob_id, status, confidence, importance, pinned, valid_from_ms, valid_until_ms,
                     extraction_version, created_at_ms, content_hash, decided_by, locator_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
                params![
                    m.id, scope, m.kind, blob.id.to_string(), m.status, m.confidence, m.importance, m.pinned, m.valid_from_ms, m.valid_until_ms,
                    m.extraction_version, m.created_at_ms, memory::hash(&m.text), m.decided_by, m.locator_json
                ],
            )?;
            tx.execute(
                "INSERT INTO memory_sources(memory_id, event_id, locator_json) VALUES (?1, ?2, '{}')",
                params![m.id, event.id.to_string()],
            )?;
            if m.status == "active" {
                memory::index(tx, id, &m.text)?;
            }
        }
        for m in &plan.memories {
            let supersedes = m.supersedes_id.as_deref().filter(|s| copied.contains(s));
            let conflicts = m.conflicts_with_id.as_deref().filter(|s| copied.contains(s));
            if supersedes.is_some() || conflicts.is_some() {
                tx.execute("UPDATE memory_items SET supersedes_id = ?2, conflicts_with_id = ?3 WHERE id = ?1", params![m.id, supersedes, conflicts])?;
            }
        }
    }

    for i in &plan.ideas {
        tx.execute(
            "INSERT INTO ideas(id, project_id, title, note, status, manager_id, task_id, version, created_at_ms, updated_at_ms, kind)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, ?7, ?8, ?9, ?10)",
            params![i.id, i.project_id, i.title, i.note, i.status, i.manager_id, i.version, i.created_at_ms, i.updated_at_ms, i.kind],
        )?;
    }

    // A due `next_run_ms` in the past stays: the server catches up once.
    for s in &plan.schedules {
        tx.execute(
            "INSERT INTO recurring_tasks(id, project_id, manager_id, title, goal, criteria_json, attachments_json, run_json, recurrence_json, paused,
                 anchor_ms, next_run_ms, last_run_ms, last_task_id, last_skipped_ms, created_at_ms, updated_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, ?7, ?8, ?9, ?10, ?11, ?12, NULL, ?13, ?14, ?15)",
            params![
                s.id, s.project_id, s.manager_id, s.title, s.goal, s.criteria_json, s.run_json, s.recurrence_json, s.paused, s.anchor_ms,
                s.next_run_ms, s.last_run_ms, s.last_skipped_ms, s.created_at_ms, s.updated_at_ms
            ],
        )?;
    }
    Ok(())
}

/// A manager as the last protocol 2 schema keeps it: its own scope below the
/// project's and its main conversation (migration 14 makes it an employee).
#[allow(clippy::too_many_arguments)]
fn insert_legacy_manager(
    tx: &Transaction<'_>,
    id: EmployeeId,
    project: ProjectId,
    project_scope: &str,
    name: &str,
    role: &str,
    instructions_blob: Option<&str>,
    config: &Value,
) -> Result<(), StorageError> {
    let scope = aurelius_domain::ScopeId::new().to_string();
    let now = crate::now_ms();
    tx.execute("INSERT INTO scopes(id, kind, parent_id, owner_id) VALUES (?1, 'manager', ?2, ?3)", params![scope, project_scope, id.to_string()])?;
    tx.execute(
        "INSERT INTO managers(id, project_id, scope_id, name, role, title, reports_to, instructions_blob_id, config_json, status, version, created_at_ms, updated_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, NULL, NULL, ?6, ?7, 'active', 1, ?8, ?8)",
        params![id.to_string(), project.to_string(), scope, name, role, instructions_blob, serde_json::to_string(config)?, now],
    )?;
    tx.execute(
        "INSERT INTO conversations(id, kind, project_id, manager_id, title, version, created_at_ms) VALUES (?1, 'project', ?2, ?3, ?4, 1, ?5)",
        params![ConversationId::new().to_string(), project.to_string(), id.to_string(), name, now],
    )?;
    Ok(())
}

/// Copy a v0.6 data folder (`from`) into a fresh v0.7 one (`to`). With `dry_run` only count.
pub fn import(from: &Path, to: &Path, dry_run: bool) -> Result<ImportReport, ImportError> {
    if to.join("data").join("aurelius.sqlite").exists() {
        return Err(ImportError::Refused(format!("{} already holds a database; import needs a fresh folder", to.display())));
    }
    // Read once without writing anything: a bad source must not leave a database behind.
    let plan = read_v06(from, &mut |_, _| Ok(None))?;
    if dry_run {
        return Ok(plan.report());
    }
    // The last protocol 2 schema: the next `Store::open` converts it into the company.
    let store = Store::open_until(to, migrate::LAST_PROTOCOL_2_VERSION)?;
    let plan = read_v06(from, &mut |bytes, media_type| store.blobs().put(bytes, media_type).map(Some))?;
    let report = plan.report();
    store.write(move |tx| apply(tx, &plan))?;
    Ok(report)
}

/// Startup step for an auto-update: if `root` still holds a v0.6 database, move it (with `blobs/`) to
/// `root/v0.6-backup/` and import it into a fresh v0.7 database, so the daemon can start.
/// Returns the report when an import ran, `None` when there was nothing to upgrade.
pub fn upgrade_legacy(root: &Path) -> Result<Option<ImportReport>, ImportError> {
    let db = root.join("data").join("aurelius.sqlite");
    if !db.exists() {
        return Ok(None);
    }
    let version: Option<i64> = {
        let conn = Connection::open_with_flags(&db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let has: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='schema_migrations')", [], |r| r.get(0))?;
        if has { conn.query_row("SELECT MAX(version) FROM schema_migrations", [], |r| r.get(0))? } else { None }
    };
    if !is_v06(version) {
        return Ok(None);
    }
    let backup = root.join("v0.6-backup");
    if backup.exists() {
        return Err(ImportError::Refused(format!("{} already exists; move it away so the v0.6 data can be imported", backup.display())));
    }
    std::fs::create_dir_all(&backup)?;
    std::fs::rename(root.join("data"), backup.join("data"))?;
    if root.join("blobs").exists() {
        std::fs::rename(root.join("blobs"), backup.join("blobs"))?;
    }
    import(&backup, root, false).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    const V06_SCHEMA: &str = include_str!("v0.6-final-schema.sql");
    const P: &str = "00000000-0000-7000-8000-000000000001";
    const M: &str = "00000000-0000-7000-8000-000000000002";
    const GS: &str = "00000000-0000-7000-8000-0000000000a1";
    const PS: &str = "00000000-0000-7000-8000-0000000000a2";
    const MS: &str = "00000000-0000-7000-8000-0000000000a3";

    fn mem_id(n: u32) -> String {
        format!("00000000-0000-7000-8000-0000000001{n:02}")
    }

    /// Write a v0.6-style blob (file under `blobs/<key>` and its row) and return its id.
    fn old_blob(conn: &Connection, root: &Path, n: u32, text: &str) -> String {
        let id = format!("00000000-0000-7000-8000-0000000002{n:02}");
        let key = format!("{:02}/blob{n}", n % 100);
        let path = root.join("blobs").join(&key);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, text).unwrap();
        conn.execute(
            "INSERT INTO blobs(id, storage_key, sha256, byte_length, media_type, created_at_ms) VALUES (?1, ?2, ?3, ?4, 'text/plain', 1)",
            params![id, key, hex::encode(Sha256::digest(text.as_bytes())), text.len() as i64],
        )
        .unwrap();
        id
    }

    fn make_v06(root: &Path) {
        std::fs::create_dir_all(root.join("data")).unwrap();
        let conn = Connection::open(root.join("data").join("aurelius.sqlite")).unwrap();
        conn.execute_batch(V06_SCHEMA).unwrap();
        conn.execute("INSERT INTO schema_migrations(version, checksum, applied_at_ms) VALUES (28, 'x', 1)", []).unwrap();
        let desc = old_blob(&conn, root, 1, "A test project");
        let instr = old_blob(&conn, root, 2, "Be helpful");
        conn.execute("INSERT INTO scopes(id, kind) VALUES (?1, 'global')", [GS]).unwrap();
        conn.execute("INSERT INTO scopes(id, kind, parent_id, owner_id) VALUES (?1, 'project', ?2, ?3)", params![PS, GS, P]).unwrap();
        conn.execute("INSERT INTO scopes(id, kind, parent_id, owner_id) VALUES (?1, 'manager', ?2, ?3)", params![MS, PS, M]).unwrap();
        conn.execute(
            "INSERT INTO projects(id, scope_id, name, description_blob_id, status, version, created_at_ms, updated_at_ms, folder)
             VALUES (?1, ?2, 'Alpha', ?3, 'paused', 3, 100, 200, 'C:/work/alpha')",
            params![P, PS, desc],
        )
        .unwrap();
        let config = json!({
            "identity": "ident", "memory_policy": {"mode": "inherit"}, "tool_policy": {"mode": "value", "value": "x"},
            "autonomy_policy": {"mode": "inherit"}, "runtime_selection": {"mode": "inherit"}, "voice_config": {"mode": "inherit"},
            "worker_policy": {"delegation_allowed": false, "parallel_allowed": true, "max_concurrent": 2, "max_per_task": 6,
                              "max_revisions": 2, "max_depth": 4, "sequential_tasks": true}
        });
        conn.execute(
            "INSERT INTO managers(id, project_id, scope_id, name, role, instructions_blob_id, config_json, status, version, created_at_ms, updated_at_ms, folder)
             VALUES (?1, ?2, ?3, 'Ada', 'engineer', ?4, ?5, 'active', 2, 110, 210, 'C:/work/ada')",
            params![M, P, MS, instr, config.to_string()],
        )
        .unwrap();
        // memories: active (global), active (manager, supersedes the first), forgotten.
        for (n, scope, status, text, supersedes) in [
            (10u32, GS, "active", "global fact", None),
            (11, MS, "active", "manager fact", Some(mem_id(10))),
            (12, PS, "forgotten", "erased", None),
        ] {
            let blob = old_blob(&conn, root, n, text);
            conn.execute(
                "INSERT INTO memory_items(id, scope_id, kind, content_blob_id, status, confidence, importance, pinned, valid_from_ms,
                     supersedes_id, extraction_version, created_at_ms)
                 VALUES (?1, ?2, 'decision', ?3, ?4, 0.8, 0.6, 1, 50, ?5, 'v1', 50)",
                params![mem_id(n), scope, blob, status, supersedes],
            )
            .unwrap();
        }
        for (id, kind) in [("00000000-0000-7000-8000-000000000301", "bug"), ("00000000-0000-7000-8000-000000000302", "feature")] {
            conn.execute(
                "INSERT INTO ideas(id, project_id, title, note, status, manager_id, task_id, version, created_at_ms, updated_at_ms, kind)
                 VALUES (?1, ?2, ?3, NULL, 'open', NULL, NULL, 1, 10, 10, ?4)",
                params![id, P, format!("idea {kind}"), kind],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO task_schedules(id, project_id, manager_id, title, goal, recurrence_json, anchor_ms, next_run_ms, last_task_id, attachments_json, created_at_ms, updated_at_ms)
             VALUES ('00000000-0000-7000-8000-000000000401', ?1, ?2, 'Weekly', 'Report', '{}', 5, 6, 'some-task', '[1]', 7, 8)",
            params![P, M],
        )
        .unwrap();
    }

    #[test]
    fn config_maps_worker_policy_to_task_policy() {
        let old = json!({"identity": "i", "voice_config": {"mode": "inherit"},
            "worker_policy": {"delegation_allowed": false, "max_concurrent": 9, "max_depth": 4, "sequential_tasks": true}});
        let new = map_manager_config(&old);
        assert_eq!(new["task_policy"], json!({"delegation_allowed": false, "max_depth": 4, "sequential_tasks": true}));
        assert!(new.get("voice_config").is_none() && new.get("worker_policy").is_none());
        assert_eq!(new["identity"], "i");
        assert_eq!(new["tool_policy"], json!({"mode": "inherit"}));
        assert_eq!(map_idea_kind("bug"), "bug");
        assert_eq!(map_idea_kind("whatever"), "feature");
    }

    #[test]
    fn imports_and_refuses() {
        let dir = tempfile::tempdir().unwrap();
        let (from, to) = (dir.path().join("old"), dir.path().join("new"));
        make_v06(&from);

        // Dry run counts and writes nothing.
        let dry = import(&from, &to, true).unwrap();
        assert_eq!((dry.projects, dry.managers, dry.memories, dry.ideas, dry.recurring_tasks), (1, 1, 2, 2, 1));
        assert!(!to.join("data").join("aurelius.sqlite").exists());

        let report = import(&from, &to, false).unwrap();
        assert_eq!(report, dry);
        assert!(report.skipped.iter().any(|s| s.contains("forgotten")));

        // Opening it converts the imported managers into the company (protocol 3).
        let store = Store::open(&to).unwrap();
        store
            .read(|c| {
                let (name, status, folder): (String, String, String) =
                    c.query_row("SELECT name, status, folder FROM projects WHERE id = ?1", [P], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
                assert_eq!((name.as_str(), status.as_str(), folder.as_str()), ("Alpha", "paused", "C:/work/alpha"));
                let (mname, config, title, reports, department): (String, String, Option<String>, Option<String>, Option<String>) = c.query_row(
                    "SELECT e.name, e.config_json, e.title, e.reports_to, d.name FROM employees e LEFT JOIN departments d ON d.id = e.department_id WHERE e.id = ?1",
                    [M],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
                )?;
                let config: Value = serde_json::from_str(&config).unwrap();
                assert_eq!(mname, "Ada");
                assert_eq!(config["task_policy"]["max_depth"], 4);
                assert_eq!(config["task_policy"]["sequential_tasks"], true);
                assert!(title.is_none() && reports.is_none());
                assert_eq!(department.as_deref(), Some("Alpha"), "the project seeded a department");
                let convs: i64 = c.query_row("SELECT COUNT(*) FROM conversations WHERE employee_id = ?1 AND kind = 'employee' AND project_id IS NULL", [M], |r| r.get(0))?;
                assert_eq!(convs, 1);
                let kinds: Vec<String> = c.prepare("SELECT kind FROM ideas ORDER BY kind")?.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?;
                assert_eq!(kinds, ["bug", "feature"]);
                let (last_task, attachments, next): (Option<String>, Option<String>, Option<i64>) =
                    c.query_row("SELECT last_task_id, attachments_json, next_run_ms FROM recurring_tasks", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
                assert!(last_task.is_none() && attachments.is_none());
                assert_eq!(next, Some(6));
                let memories: i64 = c.query_row("SELECT COUNT(*) FROM memory_items", [], |r| r.get(0))?;
                assert_eq!(memories, 2);
                let supersedes: Option<String> = c.query_row("SELECT supersedes_id FROM memory_items WHERE id = ?1", [mem_id(11)], |r| r.get(0))?;
                assert_eq!(supersedes, Some(mem_id(10)));
                let hits: i64 = c.query_row("SELECT COUNT(*) FROM chunk_fts WHERE chunk_fts MATCH 'manager'", [], |r| r.get(0))?;
                assert_eq!(hits, 1);
                let text_key: String = c.query_row(
                    "SELECT b.storage_key FROM memory_items m JOIN blobs b ON b.id = m.content_blob_id WHERE m.id = ?1",
                    [mem_id(10)],
                    |r| r.get(0),
                )?;
                assert_eq!(store.blobs().read(&text_key).unwrap(), b"global fact");
                Ok(())
            })
            .unwrap();
        drop(store);

        // A folder that already has a database is refused.
        assert!(matches!(import(&from, &to, false), Err(ImportError::Refused(_))));
        assert!(matches!(import(&from, &to, true), Err(ImportError::Refused(_))));
    }

    #[test]
    fn upgrade_legacy_moves_and_imports() {
        let dir = tempfile::tempdir().unwrap();
        make_v06(dir.path());
        let report = upgrade_legacy(dir.path()).unwrap().unwrap();
        assert_eq!(report.projects, 1);
        assert!(dir.path().join("v0.6-backup").join("data").join("aurelius.sqlite").exists());
        Store::open(dir.path()).unwrap();
        assert!(upgrade_legacy(dir.path()).unwrap().is_none());
    }

    #[test]
    fn upgrade_legacy_reads_released_v06_schema() {
        let dir = tempfile::tempdir().unwrap();
        make_v06(dir.path());
        {
            let conn = Connection::open(dir.path().join("data").join("aurelius.sqlite")).unwrap();
            conn.execute_batch("ALTER TABLE ideas DROP COLUMN kind; UPDATE schema_migrations SET version = 27;").unwrap();
        }
        let report = upgrade_legacy(dir.path()).unwrap().unwrap();
        assert_eq!(report.projects, 1);
        let store = Store::open(dir.path()).unwrap();
        drop(store);
        let conn = Connection::open(dir.path().join("data").join("aurelius.sqlite")).unwrap();
        let kinds: Vec<String> = conn.prepare("SELECT kind FROM ideas").unwrap().query_map([], |r| r.get(0)).unwrap().collect::<Result<_, _>>().unwrap();
        assert_eq!(kinds, ["feature", "feature"]);
    }
}
