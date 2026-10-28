//! What goes into an employee's context without a specific question (the
//! project Memory view). Candidates come from the same scopes and retrieval
//! as a run's; placement uses the Context Builder's own rules.

use crate::memory::context::{self, Candidate};
use aurelius_domain::memory::{MemoryKind, MemoryScope};
use aurelius_domain::{DepartmentId, EmployeeId, ProjectId};
use aurelius_domain::views::{MemoryContextItem, MemoryContextView};
use aurelius_storage::StorageError;
use aurelius_storage::memory::token_estimate;
use rusqlite::{Connection, OptionalExtension};
use std::collections::HashMap;

/// What goes into a context: an employee's (own, its department, the project if
/// given, global), a project conversation's (project, global) or a department's
/// alone (its scope, global).
pub fn memory_context(
    conn: &Connection,
    project_id: Option<ProjectId>,
    employee_id: Option<EmployeeId>,
    department_id: Option<DepartmentId>,
    read_blob: &dyn Fn(&str) -> Option<String>,
) -> Result<MemoryContextView, StorageError> {
    let read_scopes = match (employee_id, project_id, department_id) {
        (Some(m), project, _) => aurelius_storage::memory::employee_read_scopes(conn, m, project)?,
        (None, Some(p), _) => aurelius_storage::memory::project_read_scopes(conn, p)?,
        (None, None, Some(d)) => aurelius_storage::memory::department_read_scopes(conn, d)?,
        (None, None, None) => aurelius_storage::memory::ReadScopes { global: aurelius_storage::memory::scope_id(conn, MemoryScope::Global)?, ..Default::default() },
    };
    let scopes = read_scopes.all();
    let employee_name: Option<String> = match employee_id {
        Some(m) => conn.query_row("SELECT name FROM employees WHERE id = ?1", [m.to_string()], |r| r.get(0)).optional()?,
        None => None,
    };
    let department_name: Option<String> = match &read_scopes.department {
        Some(scope) => conn.query_row("SELECT name FROM departments WHERE scope_id = ?1", [scope], |r| r.get(0)).optional()?,
        None => None,
    };
    let department_id = match (department_id, &read_scopes.department) {
        (Some(d), _) => Some(d),
        (None, Some(scope)) => conn
            .query_row("SELECT id FROM departments WHERE scope_id = ?1", [scope], |r| r.get::<_, String>(0))
            .optional()?
            .and_then(|d| DepartmentId::parse(&d).ok()),
        (None, None) => None,
    };

    // memory id -> (scope label, employee name, department name, kind, pinned)
    let mut meta: HashMap<String, (&'static str, Option<String>, Option<String>, MemoryKind, bool)> = HashMap::new();
    let candidates: Vec<Candidate> = aurelius_storage::memory::retrieve(conn, &scopes, "")?
        .into_iter()
        .filter(|(row, _)| row.kind != MemoryKind::ConversationSummary)
        .filter_map(|(row, lexical)| {
            let text = read_blob(&row.blob_key)?;
            let scope = read_scopes.label(&row.scope_id);
            let name = (scope == "employee").then(|| employee_name.clone()).flatten();
            let department = (scope == "department").then(|| department_name.clone()).flatten();
            meta.insert(row.id.to_string(), (scope, name, department, row.kind, row.pinned));
            Some(Candidate {
                id: row.id.to_string(),
                kind: row.kind,
                text,
                lexical,
                importance: row.importance,
                created_at_ms: row.created_at_ms,
                pinned: row.pinned,
                source: row.scope_id.clone(),
                label: format!("{} · {scope}", context::kind_label(row.kind, &row.locator)),
                lesson: context::is_lesson(&row.locator),
            })
        })
        .collect();

    let mut always_tokens = 0;
    let mut items = Vec::new();
    for (c, tokens, placement) in context::place_without_question(candidates, aurelius_storage::now_ms()) {
        let Ok(memory_id) = aurelius_domain::MemoryId::parse(&c.id) else { continue };
        let Some((scope, employee_name, department_name, kind, pinned)) = meta.remove(&c.id) else { continue };
        if placement == context::Placement::Always {
            always_tokens += tokens;
        }
        items.push(MemoryContextItem {
            memory_id,
            kind,
            scope: scope.into(),
            employee_name,
            department_name,
            pinned,
            label: c.label,
            content: c.text,
            tokens,
            placement: placement.as_str().into(),
        });
    }

    Ok(MemoryContextView {
        project_id,
        employee_id,
        department_id,
        system_tokens: match employee_id {
            Some(m) => system_tokens(conn, m, project_id, read_blob)?,
            None => 0,
        },
        memory_budget_tokens: context::MEMORY_BUDGET_TOKENS,
        always_tokens,
        items,
    })
}

/// The fixed system text of an employee's conversation (its chat, or a task's in
/// `project`), without memory.
fn system_tokens(conn: &Connection, employee: EmployeeId, project: Option<ProjectId>, read_blob: &dyn Fn(&str) -> Option<String>) -> Result<u32, StorageError> {
    let row: Option<(String, String, Option<String>)> = conn
        .query_row(
            "SELECT m.name, m.role, b.storage_key FROM employees m LEFT JOIN blobs b ON b.id = m.instructions_blob_id WHERE m.id = ?1",
            [employee.to_string()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    let Some((name, role, key)) = row else { return Ok(0) };
    let instructions = key.as_deref().and_then(read_blob).unwrap_or_default();
    let guidance = if project.is_some() { crate::conversation::overview::TOOLS_GUIDANCE } else { crate::run::plan::CHAT_TOOLS_GUIDANCE };
    let mut text = format!("You are {name}, {role}. {instructions}\n\n{}\n\n{guidance}", crate::run::plan::AUTONOMY_GUIDANCE);
    let colleagues = crate::task::tasks::colleagues(conn, read_blob)?;
    let others: Vec<&crate::task::tasks::Colleague> = colleagues.iter().filter(|c| c.id != employee).collect();
    text.push_str(&format!("\n{}", crate::task::tasks::colleague_lines(&others)));
    if let Some(project) = project {
        if let Some(overview) = crate::conversation::overview::project_overview(conn, project, read_blob, aurelius_storage::now_ms())? {
            text.push_str(&format!("\n\n{overview}"));
        }
    }
    Ok(token_estimate(&text))
}
