//! The one-time conversion of a protocol 2 database into the company of
//! protocol 3. Runs right after the SQL of migration 14, in its transaction
//! (foreign keys off, checked before the commit), while `_company_convert`
//! still says which project each employee came from:
//!
//! * every old project seeds a department (its name and description; the head is
//!   the project's root employee when it had exactly one);
//! * employees with the same name (case-insensitive, any role) are one person:
//!   the one of an active project with the most tasks (the oldest on a tie)
//!   stays with its plain name, and its project is its department; it takes over
//!   the others' work, memories, plugins, instruction versions and proposals,
//!   also those from archived projects; the others are archived (their ids and
//!   history stay);
//! * a name found only in archived projects becomes archived employees, not roots;
//! * a boss is kept when it survives the merge without a loop, else cleared;
//! * one `company.converted` event holds the report (`ConversionReport`),
//!   including how many roots are left.

use crate::{BlobStore, StorageError, events, now_ms};
use aurelius_domain::{ActorKind, EmployeeId, ScopeId};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

struct Old {
    id: String,
    project: String,
    name: String,
    archived: bool,
    scope: String,
    reports_to: Option<String>,
}

struct OldProject {
    id: String,
    name: String,
    description_key: Option<String>,
    archived: bool,
    created_at_ms: i64,
}

pub fn convert(tx: &Transaction<'_>, blobs: Option<&BlobStore>) -> Result<(), StorageError> {
    let olds: Vec<Old> = {
        let mut stmt = tx.prepare(
            "SELECT e.id, c.project_id, e.name, e.status, e.scope_id, e.reports_to
             FROM employees e JOIN _company_convert c ON c.employee_id = e.id ORDER BY e.created_at_ms, e.id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(Old {
                id: r.get(0)?,
                project: r.get(1)?,
                name: r.get(2)?,
                archived: r.get::<_, String>(3)? == "archived",
                scope: r.get(4)?,
                reports_to: r.get(5)?,
            })
        })?;
        rows.collect::<Result<_, _>>()?
    };
    let projects: Vec<OldProject> = {
        let mut stmt = tx.prepare(
            "SELECT p.id, p.name, b.storage_key, p.status, p.created_at_ms FROM projects p LEFT JOIN blobs b ON b.id = p.description_blob_id
             ORDER BY p.created_at_ms, p.id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(OldProject { id: r.get(0)?, name: r.get(1)?, description_key: r.get(2)?, archived: r.get::<_, String>(3)? == "archived", created_at_ms: r.get(4)? })
        })?;
        rows.collect::<Result<_, _>>()?
    };

    let mut report = ConversionReport::default();
    if !projects.is_empty() {
        let global = events::ensure_global_scope(tx)?;
        let now = now_ms();
        for p in &projects {
            let id = Uuid::now_v7();
            let scope = ScopeId::new().to_string();
            tx.execute("INSERT INTO scopes(id, kind, parent_id, owner_id) VALUES (?1, 'department', ?2, ?3)", params![scope, global, id.to_string()])?;
            let members: Vec<&Old> = olds.iter().filter(|m| m.project == p.id && !m.archived).collect();
            let roots: Vec<&&Old> = members.iter().filter(|m| m.reports_to.as_ref().is_none_or(|b| !members.iter().any(|o| &o.id == b))).collect();
            let head = if roots.len() == 1 { Some(roots[0].id.clone()) } else { None };
            let description = p
                .description_key
                .as_deref()
                .zip(blobs)
                .and_then(|(key, blobs)| blobs.read(key).ok())
                .and_then(|bytes| String::from_utf8(bytes).ok())
                .map(|text| text.trim().to_string())
                .filter(|text| !text.is_empty());
            tx.execute(
                "INSERT INTO departments(id, scope_id, name, description, head_id, status, version, created_at_ms, updated_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7, ?8)",
                params![id.to_string(), scope, p.name, description, head, if p.archived { "archived" } else { "active" }, p.created_at_ms, now],
            )?;
            if !p.archived {
                tx.execute(
                    "UPDATE employees SET department_id = ?1 WHERE status <> 'archived' AND id IN (SELECT employee_id FROM _company_convert WHERE project_id = ?2)",
                    params![id.to_string(), p.id],
                )?;
            }
            tx.execute(
                "UPDATE proposals SET department_id = ?1 WHERE id IN (SELECT proposal_id FROM _company_convert_proposals WHERE project_id = ?2)",
                params![id.to_string(), p.id],
            )?;
            report.departments.push(ConvertedDepartment {
                department_id: id,
                name: p.name.clone(),
                head_id: head.as_deref().and_then(|h| EmployeeId::parse(h).ok()),
                members: if p.archived { 0 } else { members.len() as u32 },
            });
        }

        // Same names (any role) are one employee: the one of an active project
        // with the most tasks stays (the oldest on a tie), so its project is its
        // department. Counted before anything moves.
        let project_name = |id: &str| projects.iter().find(|p| p.id == id).map(|p| p.name.clone()).unwrap_or_default();
        let project_archived = |id: &str| projects.iter().any(|p| p.id == id && p.archived);
        let task_counts: HashMap<String, i64> = {
            let mut stmt = tx.prepare("SELECT employee_id, COUNT(*) FROM tasks WHERE employee_id IS NOT NULL GROUP BY employee_id")?;
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
            rows.collect::<Result<_, _>>()?
        };
        let mut groups: Vec<(String, Vec<&Old>)> = Vec::new();
        for m in olds.iter().filter(|m| !m.archived) {
            match groups.iter_mut().find(|(k, _)| *k == key(&m.name)) {
                Some((_, group)) => group.push(m),
                None => groups.push((key(&m.name), vec![m])),
            }
        }
        for (_, group) in &groups {
            let tasks = |m: &Old| task_counts.get(&m.id).copied().unwrap_or(0);
            // `olds` is oldest first, so the first with the most tasks wins a tie.
            let survivor = group.iter().copied().filter(|m| !project_archived(&m.project)).fold(None::<&Old>, |best, m| match best {
                Some(b) if tasks(b) >= tasks(m) => Some(b),
                _ => Some(m),
            });
            let Some(survivor) = survivor else {
                // Only in archived projects: archived employees, off the chart.
                for m in group {
                    tx.execute(
                        "UPDATE employees SET status = 'archived', reports_to = NULL, department_id = NULL, version = version + 1, updated_at_ms = ?2 WHERE id = ?1",
                        params![m.id, now],
                    )?;
                    tx.execute("UPDATE departments SET head_id = NULL WHERE head_id = ?1", [&m.id])?;
                    report.archived.push(ArchivedEmployee { employee_id: parse_id(&m.id), name: m.name.clone(), project: project_name(&m.project) });
                }
                continue;
            };
            if group.len() == 1 {
                continue;
            }
            let mut merged = MergedEmployee {
                employee_id: parse_id(&survivor.id),
                name: survivor.name.clone(),
                merged_ids: Vec::new(),
                projects: vec![project_name(&survivor.project)],
            };
            for other in group.iter().filter(|m| m.id != survivor.id) {
                merge(tx, survivor, other, &mut report)?;
                merged.merged_ids.push(parse_id(&other.id));
                merged.projects.push(project_name(&other.project));
            }
            report.merged.push(merged);
        }

        clear_bad_bosses(tx, &mut report)?;

        // The departments as they ended up (heads moved to survivors, members merged).
        for d in &mut report.departments {
            let (head, members): (Option<String>, i64) = tx.query_row(
                "SELECT d.head_id, (SELECT COUNT(*) FROM employees e WHERE e.department_id = d.id AND e.status <> 'archived') FROM departments d WHERE d.id = ?1",
                [d.department_id.to_string()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            d.head_id = head.as_deref().and_then(|h| EmployeeId::parse(h).ok());
            d.members = members as u32;
        }
        report.roots = tx.query_row("SELECT COUNT(*) FROM employees WHERE status <> 'archived' AND reports_to IS NULL", [], |r| r.get::<_, i64>(0))? as u32;
    }

    if !olds.is_empty() || !projects.is_empty() {
        let version: i64 = tx.query_row("SELECT COALESCE(MAX(aggregate_version), 0) FROM events WHERE aggregate_type = 'company' AND aggregate_id = 'company'", [], |r| r.get(0))?;
        events::append(
            tx,
            events::NewEvent {
                aggregate_type: "company",
                aggregate_id: "company".into(),
                aggregate_version: version as u64 + 1,
                event_type: "company.converted",
                project_id: None,
                actor_kind: ActorKind::System,
                actor_id: None,
                correlation_id: "company-convert".into(),
                causation_id: None,
                payload_blob_id: None,
                metadata: serde_json::json!({ "subject": report.summary(), "report": report }),
                dedupe_key: "company-convert-v3".into(),
            },
        )?;
    }
    tx.execute_batch("DROP TABLE _company_convert; DROP TABLE _company_convert_proposals;")?;
    Ok(())
}

fn key(text: &str) -> String {
    text.trim().to_lowercase()
}

fn parse_id(id: &str) -> EmployeeId {
    EmployeeId::parse(id).unwrap_or_else(|_| EmployeeId::new())
}

/// `other` becomes part of `survivor`: everything that pointed at it points at
/// the survivor, and it is archived (its row and id stay for the history).
fn merge(tx: &Transaction<'_>, survivor: &Old, other: &Old, report: &mut ConversionReport) -> Result<(), StorageError> {
    let (s, o) = (survivor.id.as_str(), other.id.as_str());
    for (table, column) in [
        ("conversations", "employee_id"),
        ("tasks", "employee_id"),
        ("tasks", "requested_by"),
        ("runs", "employee_id"),
        ("recurring_tasks", "employee_id"),
        ("ideas", "employee_id"),
        ("agent_functions", "employee_id"),
        ("proposals", "proposed_by"),
        ("proposals", "reports_to"),
        ("proposals", "created_employee_id"),
        ("proposals", "target_employee_id"),
        ("departments", "head_id"),
    ] {
        tx.execute(&format!("UPDATE {table} SET {column} = ?1 WHERE {column} = ?2"), params![s, o])?;
    }
    // Its reports now report to the survivor; a survivor that reported to it becomes a root.
    tx.execute("UPDATE employees SET reports_to = ?1 WHERE reports_to = ?2 AND id <> ?1", params![s, o])?;
    if tx.execute("UPDATE employees SET reports_to = NULL WHERE id = ?1 AND reports_to = ?2", params![s, o])? == 1 {
        report.cleared_bosses.push(ClearedBoss { employee_id: parse_id(s), name: survivor.name.clone(), was: parse_id(o) });
    }

    // One heartbeat per employee: the survivor's wins.
    let mine: Option<String> = tx.query_row("SELECT id FROM heartbeats WHERE employee_id = ?1", [s], |r| r.get(0)).optional()?;
    let theirs: Option<String> = tx.query_row("SELECT id FROM heartbeats WHERE employee_id = ?1", [o], |r| r.get(0)).optional()?;
    match (mine, theirs) {
        (Some(mine), Some(theirs)) => {
            tx.execute("UPDATE heartbeat_wakes SET schedule_id = ?1 WHERE schedule_id = ?2", params![mine, theirs])?;
            tx.execute("DELETE FROM heartbeats WHERE id = ?1", [theirs])?;
        }
        (None, Some(theirs)) => {
            tx.execute("UPDATE heartbeats SET employee_id = ?1 WHERE id = ?2", params![s, theirs])?;
        }
        _ => {}
    }

    // Its own memories (and what else hangs on its scope) move to the survivor's scope.
    for (table, column) in [("memory_items", "scope_id"), ("retrieval_chunks", "scope_id"), ("file_grants", "scope_id"), ("shares", "destination_scope_id")] {
        tx.execute(&format!("UPDATE {table} SET {column} = ?1 WHERE {column} = ?2"), params![survivor.scope, other.scope])?;
    }
    tx.execute("UPDATE OR IGNORE config_overrides SET scope_id = ?1 WHERE scope_id = ?2", params![survivor.scope, other.scope])?;
    tx.execute("DELETE FROM config_overrides WHERE scope_id = ?1", [&other.scope])?;

    // Plugins: the union.
    tx.execute(
        "INSERT OR IGNORE INTO employee_plugins(employee_id, plugin_id, enabled_at_ms) SELECT ?1, plugin_id, enabled_at_ms FROM employee_plugins WHERE employee_id = ?2",
        params![s, o],
    )?;
    tx.execute("DELETE FROM employee_plugins WHERE employee_id = ?1", [o])?;

    // Instruction versions: the other's are appended, then the survivor's
    // current instructions again, so the newest version is what is in effect.
    let last: i64 = tx.query_row("SELECT COALESCE(MAX(version), 0) FROM employee_instruction_versions WHERE employee_id = ?1", [s], |r| r.get(0))?;
    let theirs: Vec<(Option<String>, String, Option<String>, i64)> = {
        let mut stmt = tx.prepare(
            "SELECT instructions_blob_id, source, proposal_id, created_at_ms FROM employee_instruction_versions WHERE employee_id = ?1 ORDER BY version",
        )?;
        let rows = stmt.query_map([o], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
        rows.collect::<Result<_, _>>()?
    };
    if !theirs.is_empty() {
        let mut version = last;
        for (blob, source, proposal, at) in theirs {
            version += 1;
            tx.execute(
                "INSERT INTO employee_instruction_versions(employee_id, version, instructions_blob_id, source, proposal_id, restored_from, created_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6)",
                params![s, version, blob, source, proposal, at],
            )?;
        }
        tx.execute(
            "INSERT INTO employee_instruction_versions(employee_id, version, instructions_blob_id, source, proposal_id, restored_from, created_at_ms)
             SELECT ?1, ?2, instructions_blob_id, 'restored', NULL, ?3, ?4 FROM employees WHERE id = ?1",
            params![s, version + 1, (last > 0).then_some(last), now_ms()],
        )?;
        tx.execute("DELETE FROM employee_instruction_versions WHERE employee_id = ?1", [o])?;
    }

    tx.execute(
        "UPDATE employees SET status = 'archived', reports_to = NULL, department_id = NULL, version = version + 1, updated_at_ms = ?2 WHERE id = ?1",
        params![o, now_ms()],
    )?;
    Ok(())
}

/// A boss must be on the chart (not archived) and must not close a loop; the
/// oldest employee of a loop becomes a root.
fn clear_bad_bosses(tx: &Transaction<'_>, report: &mut ConversionReport) -> Result<(), StorageError> {
    let rows: Vec<(String, Option<String>, String)> = {
        let mut stmt = tx.prepare("SELECT id, reports_to, name FROM employees WHERE status <> 'archived' ORDER BY created_at_ms, id")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        rows.collect::<Result<_, _>>()?
    };
    let mut boss: HashMap<String, Option<String>> = rows.iter().map(|(id, b, _)| (id.clone(), b.clone())).collect();
    let mut clear = |id: &str, name: &str, was: &str, boss: &mut HashMap<String, Option<String>>| -> Result<(), StorageError> {
        tx.execute("UPDATE employees SET reports_to = NULL WHERE id = ?1", [id])?;
        boss.insert(id.to_string(), None);
        report.cleared_bosses.push(ClearedBoss { employee_id: parse_id(id), name: name.to_string(), was: parse_id(was) });
        Ok(())
    };
    for (id, _, name) in &rows {
        let Some(Some(b)) = boss.get(id).cloned() else { continue };
        if !boss.contains_key(&b) || &b == id {
            clear(id, name, &b, &mut boss)?;
            continue;
        }
        // Walk up: back at `id` is a loop.
        let mut seen: HashSet<String> = HashSet::from([id.clone()]);
        let mut at = b.clone();
        let mut looped = false;
        while let Some(Some(next)) = boss.get(&at).cloned().map(|n| n.filter(|n| boss.contains_key(n))) {
            if next == *id {
                looped = true;
                break;
            }
            if !seen.insert(at.clone()) {
                break;
            }
            at = next;
        }
        if looped {
            clear(id, name, &b, &mut boss)?;
        }
    }
    Ok(())
}

/// What the one-time conversion of 2.x data did (event `company.converted`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ConversionReport {
    /// One per old project.
    pub departments: Vec<ConvertedDepartment>,
    /// Employees with the same name in several projects, now one.
    pub merged: Vec<MergedEmployee>,
    /// Only from conversions before 3.0 merged by name alone (renamed
    /// "Name (Project)"); empty now.
    pub suffixed: Vec<SuffixedEmployee>,
    /// Bosses that were cleared (gone in the merge, or a loop).
    pub cleared_bosses: Vec<ClearedBoss>,
    /// Managers found only in archived projects, now archived employees.
    #[serde(default)]
    pub archived: Vec<ArchivedEmployee>,
    /// Active employees without a boss after the conversion (the management team).
    #[serde(default)]
    pub roots: u32,
}

impl ConversionReport {
    /// One line for the timeline.
    pub fn summary(&self) -> String {
        let mut line = format!(
            "{} department(s) from the projects, {} employee(s) merged, {} archived, {} boss(es) cleared, {} root(s) left",
            self.departments.len(),
            self.merged.iter().map(|m| m.merged_ids.len()).sum::<usize>(),
            self.archived.len(),
            self.cleared_bosses.len(),
            self.roots
        );
        if !self.suffixed.is_empty() {
            line.push_str(&format!(", {} renamed", self.suffixed.len()));
        }
        line
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArchivedEmployee {
    pub employee_id: EmployeeId,
    pub name: String,
    /// The archived project it came from.
    pub project: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConvertedDepartment {
    pub department_id: Uuid,
    /// The old project's name.
    pub name: String,
    pub head_id: Option<EmployeeId>,
    pub members: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MergedEmployee {
    /// The one that stays (of an active project, the most tasks).
    pub employee_id: EmployeeId,
    pub name: String,
    /// The others, now archived; their work belongs to `employee_id`.
    pub merged_ids: Vec<EmployeeId>,
    /// The projects they came from, the survivor's first.
    pub projects: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SuffixedEmployee {
    pub employee_id: EmployeeId,
    pub old_name: String,
    pub new_name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClearedBoss {
    pub employee_id: EmployeeId,
    pub name: String,
    /// The boss it had.
    pub was: EmployeeId,
}

/// The conversion report, when this server converted 2.x data.
pub fn conversion_report(conn: &Connection) -> Result<Option<ConversionReport>, StorageError> {
    let metadata: Option<String> = conn
        .query_row("SELECT metadata_json FROM events WHERE type = 'company.converted' ORDER BY seq DESC LIMIT 1", [], |r| r.get(0))
        .optional()?;
    Ok(metadata
        .and_then(|m| serde_json::from_str::<serde_json::Value>(&m).ok())
        .and_then(|m| serde_json::from_value(m["report"].clone()).ok()))
}
