//! Memory items, provenance, and the FTS5 retrieval index.
//! Retrieval is ACL-first: chunk IDs are restricted to permitted scopes
//! before any text is matched or scored.

use crate::{StorageError, now_ms};
use aurelius_domain::{
    EventId, EmployeeId, MemoryId,
    memory::{Existing, MemoryKind, MemoryScope, MemoryStatus},
};
use aurelius_domain::views::{MemorySourceView, MemoryView};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

pub fn scope_id(tx: &Connection, scope: MemoryScope) -> Result<Option<String>, StorageError> {
    Ok(match scope {
        MemoryScope::Global => tx.query_row("SELECT id FROM scopes WHERE kind = 'global'", [], |r| r.get(0)).optional()?,
        MemoryScope::Project(p) => tx.query_row("SELECT scope_id FROM projects WHERE id = ?1", [p.to_string()], |r| r.get(0)).optional()?,
        MemoryScope::Employee(m) => tx.query_row("SELECT scope_id FROM employees WHERE id = ?1", [m.to_string()], |r| r.get(0)).optional()?,
        MemoryScope::Department(d) => tx.query_row("SELECT scope_id FROM departments WHERE id = ?1", [d.to_string()], |r| r.get(0)).optional()?,
    })
}

/// The scopes a run may read, each a scope id: an employee's own,
/// its department's, the project of its task (none in its chat) and global.
/// Never another employee's private scope.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReadScopes {
    pub own: Option<String>,
    pub department: Option<String>,
    pub project: Option<String>,
    pub global: Option<String>,
}

impl ReadScopes {
    /// Own, department, project, global: the order the context lists them in.
    pub fn all(&self) -> Vec<String> {
        [&self.own, &self.department, &self.project, &self.global].into_iter().flatten().cloned().collect()
    }

    /// `employee | department | project | global` for one of these scope ids.
    pub fn label(&self, scope: &str) -> &'static str {
        let is = |s: &Option<String>| s.as_deref() == Some(scope);
        if is(&self.own) {
            "employee"
        } else if is(&self.department) {
            "department"
        } else if is(&self.project) {
            "project"
        } else {
            "global"
        }
    }
}

/// What an employee reads: own, department, the project (of the task or
/// conversation; none in its chat) and global.
pub fn employee_read_scopes(conn: &Connection, employee: EmployeeId, project: Option<aurelius_domain::ProjectId>) -> Result<ReadScopes, StorageError> {
    let row: Option<(String, Option<String>)> = conn
        .query_row(
            "SELECT m.scope_id, d.scope_id FROM employees m LEFT JOIN departments d ON d.id = m.department_id AND d.status = 'active' WHERE m.id = ?1",
            [employee.to_string()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let Some((own, department)) = row else { return Ok(ReadScopes::default()) };
    Ok(ReadScopes {
        own: Some(own),
        department,
        project: match project {
            Some(p) => scope_id(conn, MemoryScope::Project(p))?,
            None => None,
        },
        global: scope_id(conn, MemoryScope::Global)?,
    })
}

/// Scopes the assistant of a project conversation may read: the project's and
/// global, never an employee's private one.
pub fn project_read_scopes(conn: &Connection, project: aurelius_domain::ProjectId) -> Result<ReadScopes, StorageError> {
    Ok(ReadScopes { project: scope_id(conn, MemoryScope::Project(project))?, global: scope_id(conn, MemoryScope::Global)?, ..ReadScopes::default() })
}

/// A department's own scope and global.
pub fn department_read_scopes(conn: &Connection, department: aurelius_domain::DepartmentId) -> Result<ReadScopes, StorageError> {
    Ok(ReadScopes { department: scope_id(conn, MemoryScope::Department(department))?, global: scope_id(conn, MemoryScope::Global)?, ..ReadScopes::default() })
}

pub fn hash(text: &str) -> String {
    use sha2::Digest;
    hex::encode(sha2::Sha256::digest(aurelius_domain::memory::normalize(text).as_bytes()))
}

pub fn token_estimate(text: &str) -> u32 {
    (text.chars().count() as u32).div_ceil(4)
}

pub struct NewMemory<'a> {
    pub id: MemoryId,
    pub scope_id: &'a str,
    pub kind: MemoryKind,
    pub blob_id: &'a str,
    pub text: &'a str,
    pub status: MemoryStatus,
    pub confidence: f64,
    pub importance: f64,
    pub pinned: bool,
    pub supersedes: Option<MemoryId>,
    pub conflicts_with: Option<MemoryId>,
    pub decided_by: Option<&'a str>,
    pub locator: serde_json::Value,
    pub sources: Vec<(EventId, serde_json::Value)>,
    pub extraction_version: &'a str,
}

pub fn insert(tx: &Transaction<'_>, m: NewMemory<'_>) -> Result<(), StorageError> {
    let now = now_ms();
    tx.execute(
        "INSERT INTO memory_items(id, scope_id, kind, content_blob_id, status, confidence, importance, pinned, valid_from_ms,
             supersedes_id, extraction_version, created_at_ms, content_hash, conflicts_with_id, decided_by, locator_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?9, ?12, ?13, ?14, ?15)",
        params![
            m.id.to_string(),
            m.scope_id,
            m.kind.as_str(),
            m.blob_id,
            m.status.as_str(),
            m.confidence,
            m.importance,
            m.pinned as i64,
            now,
            m.supersedes.map(|s| s.to_string()),
            m.extraction_version,
            hash(m.text),
            m.conflicts_with.map(|s| s.to_string()),
            m.decided_by,
            m.locator.to_string(),
        ],
    )?;
    for (event, locator) in &m.sources {
        tx.execute(
            "INSERT OR IGNORE INTO memory_sources(memory_id, event_id, locator_json) VALUES (?1, ?2, ?3)",
            params![m.id.to_string(), event.to_string(), locator.to_string()],
        )?;
    }
    if m.status == MemoryStatus::Active {
        index(tx, m.id, m.text)?;
    }
    Ok(())
}

/// Add an active memory to the retrieval index (one chunk per memory in v0.1).
pub fn index(tx: &Transaction<'_>, id: MemoryId, text: &str) -> Result<(), StorageError> {
    let row: Option<(String, String, String)> = tx
        .query_row(
            "SELECT m.scope_id, m.content_blob_id, (SELECT event_id FROM memory_sources WHERE memory_id = m.id ORDER BY rowid LIMIT 1)
             FROM memory_items m WHERE m.id = ?1",
            [id.to_string()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    let Some((scope, blob, source)) = row else { return Ok(()) };
    let chunk = EventId::new().to_string();
    tx.execute(
        "INSERT INTO retrieval_chunks(id, scope_id, source_event_id, memory_id, blob_id, locator_json, content_hash, token_count)
         VALUES (?1, ?2, ?3, ?4, ?5, '{}', ?6, ?7)",
        params![chunk, scope, source, id.to_string(), blob, hash(text), token_estimate(text)],
    )?;
    tx.execute("INSERT INTO chunk_fts(chunk_id, text) VALUES (?1, ?2)", params![chunk, text])?;
    Ok(())
}

/// Remove a memory from retrieval: chunk marked deleted, FTS text removed.
pub fn deindex(tx: &Transaction<'_>, id: MemoryId) -> Result<(), StorageError> {
    tx.execute(
        "DELETE FROM chunk_fts WHERE chunk_id IN (SELECT id FROM retrieval_chunks WHERE memory_id = ?1)",
        [id.to_string()],
    )?;
    tx.execute(
        "UPDATE retrieval_chunks SET deleted_at_ms = ?2 WHERE memory_id = ?1 AND deleted_at_ms IS NULL",
        params![id.to_string(), now_ms()],
    )?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct MemoryRow {
    pub id: MemoryId,
    pub scope_id: String,
    pub kind: MemoryKind,
    pub status: MemoryStatus,
    pub blob_key: String,
    pub pinned: bool,
    pub importance: f64,
    pub confidence: f64,
    pub created_at_ms: i64,
    pub hash: String,
    pub locator: serde_json::Value,
}

const ROW_SQL: &str = "SELECT m.id, m.scope_id, m.kind, m.status, b.storage_key, m.pinned, m.importance, m.confidence, m.created_at_ms, m.content_hash, m.locator_json
     FROM memory_items m JOIN blobs b ON b.id = m.content_blob_id";

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<MemoryRow> {
    let locator: String = r.get(10)?;
    Ok(MemoryRow {
        id: MemoryId::parse(&r.get::<_, String>(0)?).unwrap_or_else(|_| MemoryId::new()),
        scope_id: r.get(1)?,
        kind: MemoryKind::parse(&r.get::<_, String>(2)?).unwrap_or(MemoryKind::Episodic),
        status: MemoryStatus::parse(&r.get::<_, String>(3)?).unwrap_or(MemoryStatus::Rejected),
        blob_key: r.get(4)?,
        pinned: r.get::<_, i64>(5)? == 1,
        importance: r.get(6)?,
        confidence: r.get(7)?,
        created_at_ms: r.get(8)?,
        hash: r.get(9)?,
        locator: serde_json::from_str(&locator).unwrap_or_default(),
    })
}

pub fn load(conn: &Connection, id: MemoryId) -> Result<Option<MemoryRow>, StorageError> {
    Ok(conn.query_row(&format!("{ROW_SQL} WHERE m.id = ?1"), [id.to_string()], row).optional()?)
}

pub fn set_status(tx: &Transaction<'_>, id: MemoryId, status: MemoryStatus, decided_by: Option<&str>) -> Result<(), StorageError> {
    tx.execute(
        "UPDATE memory_items SET status = ?2, decided_by = COALESCE(?3, decided_by),
             valid_until_ms = CASE WHEN ?2 IN ('superseded','forgotten','rejected') THEN ?4 ELSE valid_until_ms END
         WHERE id = ?1",
        params![id.to_string(), status.as_str(), decided_by, now_ms()],
    )?;
    Ok(())
}

pub fn set_pinned(tx: &Transaction<'_>, id: MemoryId, pinned: bool) -> Result<(), StorageError> {
    tx.execute("UPDATE memory_items SET pinned = ?2 WHERE id = ?1", params![id.to_string(), pinned as i64])?;
    Ok(())
}

fn in_list(scopes: &[String]) -> String {
    // Scope IDs are UUIDs generated by us; still bind-safe by quoting.
    scopes.iter().map(|s| format!("'{}'", s.replace('\'', ""))).collect::<Vec<_>>().join(",")
}

/// Active and proposed memories in these scopes (deduplication input).
pub fn existing(conn: &Connection, scopes: &[String], read_blob: &dyn Fn(&str) -> Option<String>) -> Result<Vec<Existing>, StorageError> {
    if scopes.is_empty() {
        return Ok(Vec::new());
    }
    let sql = format!("{ROW_SQL} WHERE m.scope_id IN ({}) AND m.status IN ('active','proposed')", in_list(scopes));
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], row)?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows.into_iter().map(|r| Existing { id: r.id, kind: r.kind, content: read_blob(&r.blob_key).unwrap_or_default() }).collect())
}

/// Active and proposed lessons (`locator_json.lesson` set) in these scopes, with their text.
pub fn lessons(conn: &Connection, scopes: &[String], read_blob: &dyn Fn(&str) -> Option<String>) -> Result<Vec<(MemoryRow, String)>, StorageError> {
    if scopes.is_empty() {
        return Ok(Vec::new());
    }
    let sql = format!(
        "{ROW_SQL} WHERE m.scope_id IN ({}) AND m.status IN ('active','proposed') AND json_extract(m.locator_json, '$.lesson') IS NOT NULL ORDER BY m.created_at_ms, m.id",
        in_list(scopes)
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], row)?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows.into_iter().filter_map(|r| read_blob(&r.blob_key).map(|t| (r, t))).collect())
}

/// A later task found the same lesson again: its locator (evidence, count) and
/// importance change, and the new evidence is added to its provenance.
pub fn reinforce(tx: &Transaction<'_>, id: MemoryId, locator: &serde_json::Value, importance: f64, sources: &[(EventId, serde_json::Value)]) -> Result<(), StorageError> {
    tx.execute("UPDATE memory_items SET locator_json = ?2, importance = ?3 WHERE id = ?1", params![id.to_string(), locator.to_string(), importance.clamp(0.0, 1.0)])?;
    for (event, locator) in sources {
        tx.execute("INSERT OR IGNORE INTO memory_sources(memory_id, event_id, locator_json) VALUES (?1, ?2, ?3)", params![id.to_string(), event.to_string(), locator.to_string()])?;
    }
    Ok(())
}

/// Memories a consolidation may merge: active, not pinned, not decided by
/// the user, and no identity or conversation summary. Lessons keep their
/// evidence and count, so they are never merged either.
const MERGEABLE: &str = "m.status = 'active' AND m.pinned = 0 AND COALESCE(m.decided_by, '') <> 'user'
     AND m.kind IN ('decision','project_knowledge','episodic') AND json_extract(m.locator_json, '$.lesson') IS NULL";

/// How many mergeable memories a scope holds and their size in tokens.
pub fn mergeable_stats(conn: &Connection, scope: &str) -> Result<(u32, u32), StorageError> {
    Ok(conn.query_row(
        &format!(
            "SELECT COUNT(*), COALESCE(SUM((SELECT SUM(rc.token_count) FROM retrieval_chunks rc WHERE rc.memory_id = m.id AND rc.deleted_at_ms IS NULL)), 0)
             FROM memory_items m WHERE m.scope_id = ?1 AND {MERGEABLE}"
        ),
        [scope],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?)
}

/// The mergeable memories of a scope, oldest first.
pub fn mergeable(conn: &Connection, scope: &str) -> Result<Vec<MemoryRow>, StorageError> {
    let mut stmt = conn.prepare(&format!("{ROW_SQL} WHERE m.scope_id = ?1 AND {MERGEABLE} ORDER BY m.created_at_ms, m.id"))?;
    let rows = stmt.query_map([scope], row)?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Candidates without a lexical hit: pinned, decisions, important, recent.
const BACKGROUND_LIMIT: u32 = 300;

/// Active memories in permitted scopes, with their lexical score for `query`
/// (0 when not matched). ACL first: the FTS match is restricted to chunk IDs
/// of permitted scopes before any text is matched. The set is bounded (spec
/// §16.3): the top 200 lexical hits plus at most 300 pinned, decision,
/// important or recent memories, so a large memory never means reading
/// every memory on each run.
pub fn retrieve(conn: &Connection, scopes: &[String], query: &str) -> Result<Vec<(MemoryRow, f64)>, StorageError> {
    if scopes.is_empty() {
        return Ok(Vec::new());
    }
    let list = in_list(scopes);
    let mut lexical: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
    if let Some(fts) = fts_query(query) {
        let sql = format!(
            "SELECT rc.memory_id, bm25(chunk_fts) FROM chunk_fts
             JOIN retrieval_chunks rc ON rc.id = chunk_fts.chunk_id
             WHERE chunk_fts.chunk_id IN (SELECT id FROM retrieval_chunks WHERE scope_id IN ({list}) AND deleted_at_ms IS NULL)
               AND chunk_fts MATCH ?1
             ORDER BY bm25(chunk_fts) LIMIT 200"
        );
        let mut stmt = conn.prepare(&sql)?;
        let hits = stmt.query_map([fts], |r| Ok((r.get::<_, String>(0)?, r.get::<_, f64>(1)?)))?;
        for hit in hits {
            let (id, bm25) = hit?;
            // bm25: lower is better (negative). Normalize to 0..1.
            let score = (-bm25).max(0.0);
            lexical.insert(id, score / (1.0 + score));
        }
    }
    let hits: Vec<String> = lexical.keys().cloned().collect();
    let hit_clause = if hits.is_empty() { "0".to_string() } else { format!("m.id IN ({})", in_list(&hits)) };
    let sql = format!(
        "{ROW_SQL} WHERE m.status = 'active' AND m.scope_id IN ({list})
           AND EXISTS (SELECT 1 FROM retrieval_chunks rc WHERE rc.memory_id = m.id AND rc.deleted_at_ms IS NULL)
           AND ({hit_clause} OR m.id IN (
               SELECT id FROM memory_items WHERE status = 'active' AND scope_id IN ({list})
               ORDER BY pinned DESC, kind = 'decision' DESC, importance DESC, created_at_ms DESC LIMIT {BACKGROUND_LIMIT}))"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], row)?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows.into_iter().map(|r| {
        let score = lexical.get(&r.id.to_string()).copied().unwrap_or(0.0);
        (r, score)
    }).collect())
}

/// Build a safe FTS5 query: quoted words of 3+ characters joined with OR.
pub fn fts_query(text: &str) -> Option<String> {
    let words: Vec<String> = aurelius_domain::memory::normalize(text)
        .split(' ')
        .filter(|w| w.chars().count() >= 3)
        .take(24)
        .map(|w| format!("\"{w}\""))
        .collect();
    (!words.is_empty()).then(|| words.join(" OR "))
}

/// Latest active summary of a conversation and the ordinal it covers through.
pub fn latest_summary(conn: &Connection, conversation: aurelius_domain::ConversationId) -> Result<Option<(MemoryRow, u32)>, StorageError> {
    let sql = format!(
        "{ROW_SQL} WHERE m.kind = 'conversation_summary' AND m.status = 'active'
           AND json_extract(m.locator_json, '$.conversation_id') = ?1
         ORDER BY json_extract(m.locator_json, '$.covers_through_ordinal') DESC LIMIT 1"
    );
    let found = conn.query_row(&sql, [conversation.to_string()], row).optional()?;
    Ok(found.map(|r| {
        let through = r.locator["covers_through_ordinal"].as_u64().unwrap_or(0) as u32;
        (r, through)
    }))
}

/// The lesson part of a memory's locator (Harness² A), if it is a lesson.
pub fn lesson_of(locator_json: &str) -> Option<aurelius_domain::memory::LessonLocator> {
    let locator: serde_json::Value = serde_json::from_str(locator_json).ok()?;
    serde_json::from_value(locator.get("lesson")?.clone()).ok()
}

fn source_label(event_type: &str, title: Option<String>) -> String {
    let base = match event_type {
        "task.succeeded" => "Task completed",
        "task.failed" => "Task failed",
        "memory.reinforced" => "Lesson confirmed",
        "memory.extracted" => "Lesson learned",
        "run.failed" => "Run failed",
        "run.cancelled" => "Run stopped",
        "run.waiting" => "Run retried",
        "approval.rejected" => "Action rejected",
        "approval.plan_rejected" | "approval.plan_revision_requested" => "Plan sent back",
        "task.cancelled" => "Part cancelled",
        "task.waiting" => "Part came back",
        "task.variant_chosen" => "Variant chosen",
        "task.created" => "Assignment",
        "message.user_committed" => "Your message",
        "message.assistant_committed" => "Answer",
        "memory.remembered" => "Recorded by you",
        "memory.revised" => "Revised by you",
        "memory.consolidated" => "Consolidated",
        other => other,
    };
    match title {
        Some(t) if !t.is_empty() => format!("{base} · {t}"),
        _ => base.to_owned(),
    }
}

/// Which scopes `memory.list` shows (global always).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Wanted {
    pub project: Option<aurelius_domain::ProjectId>,
    pub employee: Option<EmployeeId>,
    pub department: Option<aurelius_domain::DepartmentId>,
}

/// One scope in the Memory view: its id, kind and owner (an employee or a department).
struct Listed {
    scope: String,
    label: &'static str,
    employee: Option<(EmployeeId, String)>,
    department: Option<(aurelius_domain::DepartmentId, String)>,
}

/// Memories for the Memory view, newest first: global, plus the project's
/// scope, the employee's own and its department's, the department's; nothing
/// asked: every department and employee scope too. Forgotten items keep no content.
pub fn list(conn: &Connection, wanted: Wanted, read_blob: &dyn Fn(&str) -> Option<String>) -> Result<Vec<MemoryView>, StorageError> {
    let mut scopes: Vec<Listed> = Vec::new();
    if let Some(global) = scope_id(conn, MemoryScope::Global)? {
        scopes.push(Listed { scope: global, label: "global", employee: None, department: None });
    }
    if let Some(p) = wanted.project {
        if let Some(ps) = scope_id(conn, MemoryScope::Project(p))? {
            scopes.push(Listed { scope: ps, label: "project", employee: None, department: None });
        }
    }
    let everything = wanted == Wanted::default();
    let mut departments: Vec<aurelius_domain::DepartmentId> = wanted.department.into_iter().collect();
    {
        let mut stmt = conn.prepare("SELECT id, name, scope_id, department_id FROM employees WHERE (?1 IS NULL AND ?2) OR id = ?1 ORDER BY created_at_ms, id")?;
        let rows = stmt.query_map(params![wanted.employee.map(|e| e.to_string()), everything], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, Option<String>>(3)?))
        })?;
        for row in rows {
            let (id, name, scope, department) = row?;
            if let Ok(id) = EmployeeId::parse(&id) {
                scopes.push(Listed { scope, label: "employee", employee: Some((id, name)), department: None });
            }
            if let Some(d) = department.and_then(|d| aurelius_domain::DepartmentId::parse(&d).ok()) {
                if wanted.employee.is_some() && !departments.contains(&d) {
                    departments.push(d);
                }
            }
        }
    }
    {
        let mut stmt = conn.prepare("SELECT id, name, scope_id FROM departments WHERE ?1 ORDER BY lower(name), id")?;
        let rows = stmt.query_map([everything], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))?;
        for row in rows {
            let (id, _, _) = row?;
            if let Ok(id) = aurelius_domain::DepartmentId::parse(&id) {
                departments.push(id);
            }
        }
    }
    for d in departments {
        let found: Option<(String, String)> =
            conn.query_row("SELECT scope_id, name FROM departments WHERE id = ?1", [d.to_string()], |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
        if let Some((scope, name)) = found {
            scopes.push(Listed { scope, label: "department", employee: None, department: Some((d, name)) });
        }
    }
    let mut out = Vec::new();
    for Listed { scope, label, employee, department } in scopes {
        let sql = format!(
            "SELECT m.id, m.kind, m.status, b.storage_key, m.pinned, m.importance, m.confidence, m.created_at_ms, m.supersedes_id, m.conflicts_with_id, m.decided_by, m.locator_json
             FROM memory_items m JOIN blobs b ON b.id = m.content_blob_id
             WHERE m.scope_id = ?1 ORDER BY m.created_at_ms DESC"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt
            .query_map([&scope], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, i64>(4)?,
                    r.get::<_, f64>(5)?,
                    r.get::<_, f64>(6)?,
                    r.get::<_, i64>(7)?,
                    r.get::<_, Option<String>>(8)?,
                    r.get::<_, Option<String>>(9)?,
                    r.get::<_, Option<String>>(10)?,
                    r.get::<_, String>(11)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for (id, kind, status, key, pinned, importance, confidence, created, supersedes, conflicts, decided_by, locator) in rows {
            let status = MemoryStatus::parse(&status).unwrap_or(MemoryStatus::Rejected);
            let mut src = conn.prepare_cached(
                "SELECT e.id, e.type, e.occurred_at_ms, (SELECT title FROM tasks WHERE id = e.correlation_id OR (e.aggregate_type = 'task' AND id = e.aggregate_id)),
                        CASE WHEN e.actor_kind = 'employee' THEN (SELECT name FROM employees WHERE id = e.actor_id) END
                 FROM memory_sources s JOIN events e ON e.id = s.event_id WHERE s.memory_id = ?1 ORDER BY e.seq",
            )?;
            let sources = src
                .query_map([&id], |r| {
                    let event_type: String = r.get(1)?;
                    // An employee's own note names the employee.
                    let label = match r.get::<_, Option<String>>(4)? {
                        Some(employee) if matches!(event_type.as_str(), "memory.remembered" | "memory.proposed") => format!("Noted by {employee}"),
                        _ => source_label(&event_type, r.get(3)?),
                    };
                    Ok(MemorySourceView { event_id: r.get(0)?, label, at_ms: r.get(2)? })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            out.push(MemoryView {
                id: MemoryId::parse(&id).unwrap_or_else(|_| MemoryId::new()),
                scope: label.to_owned(),
                employee_id: employee.as_ref().map(|(m, _)| *m),
                employee_name: employee.as_ref().map(|(_, n)| n.clone()),
                department_id: department.as_ref().map(|(d, _)| *d),
                department_name: department.as_ref().map(|(_, n)| n.clone()),
                kind: MemoryKind::parse(&kind).unwrap_or(MemoryKind::Episodic),
                status,
                content: if status == MemoryStatus::Forgotten { String::new() } else { read_blob(&key).unwrap_or_default() },
                pinned: pinned == 1,
                importance,
                confidence,
                created_at_ms: created,
                supersedes_id: supersedes.and_then(|s| MemoryId::parse(&s).ok()),
                conflicts_with_id: conflicts.and_then(|s| MemoryId::parse(&s).ok()),
                decided_by,
                sources,
                lesson: lesson_of(&locator),
            });
        }
    }
    out.sort_by(|a, b| b.created_at_ms.cmp(&a.created_at_ms));
    Ok(out)
}
