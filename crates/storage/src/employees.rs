//! Employee projections. Employees are company-wide: each has its own scope
//! (below the global one) and one persistent chat with
//! the user (`kind = 'employee'`, no project; the oldest such conversation).
//! Task conversations carry the task's project.

use crate::{StorageError, now_ms};
use aurelius_domain::{ConversationId, EmployeeId, ProjectId, employee::LifecycleStatus, views::{AgentView, EmployeeSummary}};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

pub struct EmployeeRow {
    pub name: String,
    pub role: String,
    pub title: Option<String>,
    pub reports_to: Option<EmployeeId>,
    pub status: LifecycleStatus,
    pub version: u64,
    /// Its persistent chat.
    pub conversation_id: ConversationId,
}

/// The persistent chat of employee `?` (the column or parameter it is compared with).
pub(crate) const MAIN_CHAT: &str = "(SELECT id FROM conversations WHERE employee_id = {} AND kind = 'employee' ORDER BY created_at_ms, id LIMIT 1)";

pub(crate) fn main_chat_of(employee: &str) -> String {
    MAIN_CHAT.replace("{}", employee)
}

fn parse<T, E: std::error::Error + Send + Sync + 'static>(r: Result<T, E>) -> rusqlite::Result<T> {
    r.map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))
}

fn status(text: String) -> rusqlite::Result<LifecycleStatus> {
    LifecycleStatus::parse(&text).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, format!("bad status {text}").into())
    })
}

pub fn project_scope(tx: &Transaction<'_>, project: ProjectId) -> Result<Option<(String, LifecycleStatus)>, StorageError> {
    Ok(tx
        .query_row("SELECT scope_id, status FROM projects WHERE id = ?1", [project.to_string()], |r| {
            Ok((r.get::<_, String>(0)?, status(r.get(1)?)?))
        })
        .optional()?)
}

/// A new employee with its scope (below the global one) and its persistent chat.
#[allow(clippy::too_many_arguments)]
pub fn insert_employee(
    tx: &Transaction<'_>,
    id: EmployeeId,
    name: &str,
    role: &str,
    title: Option<&str>,
    reports_to: Option<EmployeeId>,
    instructions_blob: Option<&str>,
    config: &serde_json::Value,
    conversation: ConversationId,
) -> Result<(), StorageError> {
    let global = crate::events::ensure_global_scope(tx)?;
    let scope = aurelius_domain::ScopeId::new().to_string();
    let now = now_ms();
    tx.execute(
        "INSERT INTO scopes(id, kind, parent_id, owner_id) VALUES (?1, 'employee', ?2, ?3)",
        params![scope, global, id.to_string()],
    )?;
    tx.execute(
        "INSERT INTO employees(id, scope_id, name, role, title, reports_to, instructions_blob_id, config_json, status, version, created_at_ms, updated_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'active', 1, ?9, ?9)",
        params![
            id.to_string(),
            scope,
            name,
            role,
            title,
            reports_to.map(|m| m.to_string()),
            instructions_blob,
            serde_json::to_string(config)?,
            now
        ],
    )?;
    tx.execute(
        "INSERT INTO conversations(id, kind, project_id, employee_id, title, version, created_at_ms) VALUES (?1, 'employee', NULL, ?2, ?3, 1, ?4)",
        params![conversation.to_string(), id.to_string(), name, now],
    )?;
    Ok(())
}

pub fn load_employee(tx: &Transaction<'_>, id: EmployeeId) -> Result<Option<EmployeeRow>, StorageError> {
    let sql = format!(
        "SELECT m.name, m.role, m.status, m.version, c.id, m.title, m.reports_to
         FROM employees m JOIN conversations c ON c.id = {}
         WHERE m.id = ?1",
        main_chat_of("m.id")
    );
    Ok(tx
        .query_row(&sql, [id.to_string()], |r| {
            Ok(EmployeeRow {
                name: r.get(0)?,
                role: r.get(1)?,
                status: status(r.get(2)?)?,
                version: r.get::<_, i64>(3)? as u64,
                conversation_id: parse(ConversationId::parse(&r.get::<_, String>(4)?))?,
                title: r.get(5)?,
                reports_to: r.get::<_, Option<String>>(6)?.map(|m| parse(EmployeeId::parse(&m))).transpose()?,
            })
        })
        .optional()?)
}

/// Another employee (not archived, not `except`) already has this name (trimmed, case-insensitive).
pub fn name_taken(conn: &Connection, name: &str, except: Option<EmployeeId>) -> Result<bool, StorageError> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM employees WHERE status <> 'archived' AND lower(trim(name)) = lower(trim(?1)) AND (?2 IS NULL OR id <> ?2))",
        params![name, except.map(|e| e.to_string())],
        |r| r.get(0),
    )?)
}

/// The employee with this name (trimmed, case-insensitive) that is not archived.
pub fn by_name(conn: &Connection, name: &str) -> Result<Option<EmployeeId>, StorageError> {
    let id: Option<String> = conn
        .query_row(
            "SELECT id FROM employees WHERE status <> 'archived' AND lower(trim(name)) = lower(trim(?1)) ORDER BY created_at_ms, id LIMIT 1",
            [name],
            |r| r.get(0),
        )
        .optional()?;
    Ok(id.and_then(|i| EmployeeId::parse(&i).ok()))
}

/// What other employees read about it (P7); `None` clears it. In the transaction
/// of the create or update that carries it, so it needs no version of its own.
pub fn set_description(tx: &Transaction<'_>, id: EmployeeId, description: Option<&str>) -> Result<(), StorageError> {
    tx.execute("UPDATE employees SET description = ?2 WHERE id = ?1", params![id.to_string(), description.unwrap_or("")])?;
    Ok(())
}

pub fn update_employee(
    tx: &Transaction<'_>,
    id: EmployeeId,
    name: &str,
    role: &str,
    title: Option<&str>,
    instructions_blob: Option<&str>,
    new_version: u64,
) -> Result<(), StorageError> {
    let changed = tx.execute(
        "UPDATE employees SET name = ?2, role = ?3, title = ?4, instructions_blob_id = ?5, version = ?6, updated_at_ms = ?7
         WHERE id = ?1 AND version = ?6 - 1",
        params![id.to_string(), name, role, title, instructions_blob, new_version as i64, now_ms()],
    )?;
    if changed != 1 {
        return Err(StorageError::Rejected("employee projection is out of date".into()));
    }
    // The persistent chat carries the employee's name; other conversations keep theirs.
    tx.execute(&format!("UPDATE conversations SET title = ?2 WHERE id = {}", main_chat_of("?1")), params![id.to_string(), name])?;
    Ok(())
}

/// Let the employee work on one task at a time, or not (its task policy).
pub fn set_sequential_tasks(tx: &Transaction<'_>, id: EmployeeId, on: bool) -> Result<(), StorageError> {
    tx.execute(
        "UPDATE employees SET config_json = json_set(COALESCE(NULLIF(config_json, ''), '{}'), '$.task_policy.sequential_tasks', json(?2)) WHERE id = ?1",
        params![id.to_string(), if on { "true" } else { "false" }],
    )?;
    Ok(())
}

pub fn set_employee_status(tx: &Transaction<'_>, id: EmployeeId, status: LifecycleStatus, new_version: u64) -> Result<(), StorageError> {
    tx.execute(
        "UPDATE employees SET status = ?2, version = ?3, updated_at_ms = ?4 WHERE id = ?1",
        params![id.to_string(), status.as_str(), new_version as i64, now_ms()],
    )?;
    Ok(())
}

pub fn set_project_status(tx: &Transaction<'_>, id: ProjectId, status: LifecycleStatus) -> Result<u64, StorageError> {
    tx.execute(
        "UPDATE projects SET status = ?2, version = version + 1, updated_at_ms = ?3 WHERE id = ?1",
        params![id.to_string(), status.as_str(), now_ms()],
    )?;
    Ok(tx.query_row("SELECT version FROM projects WHERE id = ?1", [id.to_string()], |r| r.get::<_, i64>(0))? as u64)
}

/// The projects an employee has open tasks in, oldest project first.
pub fn projects_active(conn: &Connection, employee: EmployeeId) -> Result<Vec<ProjectId>, StorageError> {
    let mut stmt = conn.prepare(
        "SELECT p.id FROM projects p WHERE EXISTS(SELECT 1 FROM tasks t WHERE t.project_id = p.id AND t.employee_id = ?1
                AND t.status NOT IN ('succeeded','failed','cancelled'))
         ORDER BY p.created_at_ms, p.id",
    )?;
    let rows = stmt.query_map([employee.to_string()], |r| parse(ProjectId::parse(&r.get::<_, String>(0)?)))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Employees (not archived), oldest first.
pub fn list_employees(conn: &Connection, read_blob: &dyn Fn(&str) -> Option<String>) -> Result<Vec<EmployeeSummary>, StorageError> {
    let sql = format!(
        "SELECT m.id, m.name, m.role, m.title, m.reports_to, b.storage_key, m.status, m.version, c.id,
                EXISTS(SELECT 1 FROM runs r WHERE r.conversation_id IN (SELECT id FROM conversations WHERE employee_id = m.id) AND r.status IN ('created','starting','running','cancelling')),
                c.agent_mode, m.folder, c.browser_allowed, c.screen_allowed,
                COALESCE(json_extract(m.config_json, '$.task_policy.sequential_tasks'), 0), m.description
         FROM employees m
         JOIN conversations c ON c.id = {}
         LEFT JOIN blobs b ON b.id = m.instructions_blob_id
         WHERE m.status <> 'archived'
         ORDER BY m.created_at_ms, m.id",
        main_chat_of("m.id")
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], |r| {
        Ok((
            parse(EmployeeId::parse(&r.get::<_, String>(0)?))?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, Option<String>>(3)?,
            r.get::<_, Option<String>>(4)?.map(|m| parse(EmployeeId::parse(&m))).transpose()?,
            r.get::<_, Option<String>>(5)?,
            status(r.get(6)?)?,
            r.get::<_, i64>(7)? as u64,
            parse(ConversationId::parse(&r.get::<_, String>(8)?))?,
            r.get::<_, bool>(9)?,
            r.get::<_, String>(10)?,
            r.get::<_, Option<String>>(11)?,
            r.get::<_, bool>(12)?,
            r.get::<_, bool>(13)?,
            r.get::<_, bool>(14)?,
            r.get::<_, String>(15)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, name, role, title, reports_to, key, status, version, conversation_id, working, mode, folder, browser_allowed, screen_allowed, sequential_tasks, description) = row?;
        out.push(EmployeeSummary {
            id,
            name,
            role,
            title,
            reports_to,
            description: (!description.is_empty()).then_some(description),
            projects_active: Vec::new(),
            instructions: key.as_deref().and_then(read_blob),
            status,
            version,
            conversation_id,
            working,
            agent_mode: aurelius_domain::autonomy::AgentMode::parse(&mode),
            browser_allowed,
            screen_allowed,
            folder,
            heartbeat: None,
            sequential_tasks,
            conversations: Vec::new(),
            agents: Vec::new(),
        });
    }
    for m in out.iter_mut() {
        m.projects_active = projects_active(conn, m.id)?;
        m.conversations = crate::conversations::employee_conversations(conn, m.id)?;
        m.agents = running_agents(conn, m.id)?;
    }
    Ok(out)
}

/// Start a task conversation with an employee (`project`: the task's); it
/// inherits the persistent chat's mode. Another chat (`project` none) is an
/// employee chat.
pub fn insert_thread(
    tx: &Transaction<'_>,
    employee: EmployeeId,
    conversation: ConversationId,
    kind: aurelius_domain::conversation::ConversationKind,
    project: Option<ProjectId>,
) -> Result<bool, StorageError> {
    let sql = format!(
        "INSERT INTO conversations(id, kind, project_id, employee_id, title, version, created_at_ms, agent_mode)
         SELECT ?2, ?4, ?5, m.id, '', 1, ?3, c.agent_mode
         FROM employees m JOIN conversations c ON c.id = {}
         WHERE m.id = ?1",
        main_chat_of("m.id")
    );
    Ok(tx.execute(&sql, params![employee.to_string(), conversation.to_string(), now_ms(), kind.as_str(), project.map(|p| p.to_string())])? == 1)
}

/// Employee behind a conversation (for run context).
pub fn employee_for_conversation(conn: &Connection, conversation: ConversationId) -> Result<Option<(String, String, Option<String>)>, StorageError> {
    Ok(conn
        .query_row(
            "SELECT m.name, m.role, b.storage_key FROM conversations c JOIN employees m ON m.id = c.employee_id
             LEFT JOIN blobs b ON b.id = m.instructions_blob_id WHERE c.id = ?1",
            [conversation.to_string()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?)
}

/// The employee of a conversation and its project: a task conversation has
/// both, an employee chat no project. None: no employee's conversation.
pub fn employee_project(conn: &Connection, conversation: ConversationId) -> Result<Option<(EmployeeId, Option<ProjectId>)>, StorageError> {
    let row: Option<(String, Option<String>)> = conn
        .query_row("SELECT c.employee_id, c.project_id FROM conversations c WHERE c.id = ?1 AND c.employee_id IS NOT NULL", [conversation.to_string()], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .optional()?;
    Ok(row.and_then(|(m, p)| Some((EmployeeId::parse(&m).ok()?, p.and_then(|p| ProjectId::parse(&p).ok())))))
}

/// The subagents (unfinished `Task`/`Agent` calls) a run has started, oldest first.
fn subagents_of_run(conn: &Connection, run: &str, conversation_id: Option<ConversationId>) -> Result<Vec<AgentView>, StorageError> {
    use aurelius_domain::RunId;
    use AgentView;
    let mut sub = conn.prepare(
        "SELECT call_id, input_json, started_at_ms FROM run_activity WHERE run_id = ?1 AND tool IN ('Task','Agent') AND ended_at_ms IS NULL ORDER BY started_at_ms",
    )?;
    let rows: Vec<(String, String, i64)> = sub.query_map([run], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<Result<_, _>>()?;
    let mut out = Vec::new();
    for (call, input, started) in rows {
        let input: serde_json::Value = serde_json::from_str(&input).unwrap_or_default();
        // What it is doing: its latest tool use, and how many so far.
        let (steps, latest): (u32, Option<(String, String)>) = conn.query_row(
            "SELECT (SELECT COUNT(*) FROM run_activity WHERE run_id = ?1 AND parent_call_id = ?2),
                    tool, input_json FROM run_activity WHERE run_id = ?1 AND parent_call_id = ?2 ORDER BY started_at_ms DESC, rowid DESC LIMIT 1",
            [run, call.as_str()],
            |r| Ok((r.get(0)?, Some((r.get(1)?, r.get(2)?)))),
        ).optional()?.unwrap_or((0, None));
        out.push(AgentView {
            kind: "subagent".into(),
            name: input["description"].as_str().unwrap_or("Subagent").to_string(),
            detail: input["subagent_type"].as_str().unwrap_or("").to_string(),
            task_id: None,
            run_id: RunId::parse(run).ok(),
            conversation_id,
            since_ms: started,
            prompt: input["prompt"].as_str().map(|p| shorten(p, 600)),
            doing: latest.map(|(tool, input)| step_line(&tool, &serde_json::from_str(&input).unwrap_or_default())),
            steps,
        });
    }
    Ok(out)
}

/// The subagents running in the unfinished runs of one conversation, oldest first.
pub fn running_subagents(conn: &Connection, conversation: ConversationId) -> Result<Vec<AgentView>, StorageError> {
    let mut stmt = conn.prepare(
        "SELECT id FROM runs WHERE conversation_id = ?1 AND status IN ('created','starting','running','waiting','cancelling') ORDER BY started_at_ms",
    )?;
    let runs: Vec<String> = stmt.query_map([conversation.to_string()], |r| r.get(0))?.collect::<Result<_, _>>()?;
    let mut out = Vec::new();
    for run in runs {
        out.extend(subagents_of_run(conn, &run, Some(conversation))?);
    }
    Ok(out)
}

/// What runs for an employee right now (see `EmployeeSummary::agents`), oldest first.
pub fn running_agents(conn: &Connection, employee: EmployeeId) -> Result<Vec<AgentView>, StorageError> {
    use aurelius_domain::{ConversationId, RunId, TaskId};
    use AgentView;
    let m = employee.to_string();
    let mut out = Vec::new();
    // Its conversations that are answering, with the subagents they started.
    let mut stmt = conn.prepare(
        "SELECT r.id, COALESCE(c.title, ''), r.started_at_ms, c.id FROM runs r JOIN conversations c ON c.id = r.conversation_id
         WHERE c.employee_id = ?1 AND r.status IN ('created','starting','running','waiting','cancelling') ORDER BY r.started_at_ms",
    )?;
    let chats: Vec<(String, String, i64, String)> =
        stmt.query_map([&m], |r| Ok((r.get(0)?, r.get(1)?, r.get::<_, Option<i64>>(2)?.unwrap_or(0), r.get(3)?)))?.collect::<Result<_, _>>()?;
    for (run, title, since, conversation) in chats {
        let conversation_id = ConversationId::parse(&conversation).ok();
        out.push(AgentView {
            kind: "conversation".into(),
            name: if title.trim().is_empty() { "Conversation".into() } else { title },
            detail: String::new(),
            task_id: None,
            run_id: RunId::parse(&run).ok(),
            conversation_id,
            since_ms: since,
            ..Default::default()
        });
        out.extend(subagents_of_run(conn, &run, conversation_id)?);
    }
    // Task steps of this employee: planning, review, following up a colleague.
    // Following up shows in the conversation of its own task that delegated it.
    let mut stmt = conn.prepare(
        "SELECT r.id, r.task_id, t.title, t.status, t.employee_id, r.started_at_ms,
                CASE WHEN t.employee_id = ?1 THEN t.conversation_id ELSE p.conversation_id END
         FROM runs r JOIN tasks t ON t.id = r.task_id LEFT JOIN tasks p ON p.id = t.parent_task_id
         WHERE r.employee_id = ?1 AND r.task_id IS NOT NULL AND r.status IN ('created','starting','running','waiting','cancelling')
         ORDER BY r.started_at_ms",
    )?;
    let steps: Vec<(String, String, String, String, Option<String>, i64, Option<String>)> = stmt
        .query_map([&m], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get::<_, Option<i64>>(5)?.unwrap_or(0), r.get(6)?)))?
        .collect::<Result<_, _>>()?;
    for (run, task, title, status, owner, since, conversation) in steps {
        let (kind, name) = match owner.as_deref() {
            Some(o) if o != m.as_str() => ("follow_up", "Following up a colleague".to_string()),
            _ if status == "review" => ("review", "Reviewing".to_string()),
            _ => ("planning", "Planning".to_string()),
        };
        let conversation_id = conversation.and_then(|c| ConversationId::parse(&c).ok());
        out.push(AgentView { kind: kind.into(), name, detail: title, task_id: TaskId::parse(&task).ok(), run_id: RunId::parse(&run).ok(), conversation_id, since_ms: since, ..Default::default() });
    }
    // Sub-tasks it delegated that colleagues are still working on (not the
    // queued ones that wait their turn or a planned start).
    let mut stmt = conn.prepare(
        "SELECT t.id, t.title, COALESCE(cm.name, ''), t.status, t.created_at_ms, p.conversation_id, t.employee_id, j.state, j.not_before_ms
         FROM tasks t JOIN tasks p ON p.id = t.parent_task_id
         LEFT JOIN employees cm ON cm.id = t.employee_id LEFT JOIN jobs j ON j.id = t.id AND j.kind = 'task.run'
         WHERE p.employee_id = ?1 AND t.status NOT IN ('succeeded','failed','cancelled') ORDER BY t.created_at_ms",
    )?;
    #[allow(clippy::type_complexity)]
    let delegated: Vec<(String, String, String, String, i64, Option<String>, Option<String>, Option<String>, Option<i64>)> = stmt
        .query_map([&m], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?)))?
        .collect::<Result<_, _>>()?;
    let now = crate::now_ms();
    for (task, title, colleague, status, since, conversation, assignee, job_state, not_before) in delegated {
        if status == "queued" {
            let (Ok(id), assignee) = (TaskId::parse(&task), assignee.and_then(|a| EmployeeId::parse(&a).ok())) else { continue };
            if crate::tasks::start_wait(conn, id, assignee, job_state.as_deref(), not_before, now)?.0.is_some() {
                continue;
            }
        }
        let conversation_id = conversation.and_then(|c| ConversationId::parse(&c).ok());
        out.push(AgentView { kind: "delegated".into(), name: colleague, detail: format!("{title} · {status}"), task_id: TaskId::parse(&task).ok(), run_id: None, conversation_id, since_ms: since, ..Default::default() });
    }
    Ok(out)
}

/// At most `max` characters, with "…" when cut.
fn shorten(text: &str, max: usize) -> String {
    let text = text.trim();
    match text.char_indices().nth(max) {
        Some((at, _)) => format!("{}…", text[..at].trim_end()),
        None => text.to_string(),
    }
}

/// One tool use in a line: "Grep · poll_git_status", "Read · app.rs".
fn step_line(tool: &str, input: &serde_json::Value) -> String {
    let what = ["pattern", "file_path", "path", "command", "query", "url", "description"]
        .iter()
        .find_map(|k| input[*k].as_str())
        .map(|v| if v.contains(['/', '\\']) && !v.contains(' ') { v.rsplit(['/', '\\']).find(|p| !p.is_empty()).unwrap_or(v) } else { v })
        .map(|v| shorten(v.lines().next().unwrap_or(""), 80));
    match what {
        Some(w) if !w.is_empty() => format!("{tool} · {w}"),
        _ => tool.to_string(),
    }
}

/// Every employee that is not archived with the one it reports to: the
/// company's org chart.
pub fn org_entries(conn: &Connection) -> Result<Vec<(EmployeeId, Option<EmployeeId>)>, StorageError> {
    let mut stmt = conn.prepare("SELECT id, reports_to FROM employees WHERE status <> 'archived' ORDER BY created_at_ms, id")?;
    let rows = stmt.query_map([], |r| {
        Ok((
            parse(EmployeeId::parse(&r.get::<_, String>(0)?))?,
            r.get::<_, Option<String>>(1)?.map(|m| parse(EmployeeId::parse(&m))).transpose()?,
        ))
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// The company's org chart (employees that are not archived).
pub fn org(conn: &Connection) -> Result<aurelius_domain::org::Org, StorageError> {
    Ok(aurelius_domain::org::Org::new(org_entries(conn)?))
}

/// Also bumps the employee's version, like the other employee changes.
pub fn set_reports_to(tx: &Transaction<'_>, employee: EmployeeId, reports_to: Option<EmployeeId>) -> Result<(), StorageError> {
    tx.execute(
        "UPDATE employees SET reports_to = ?2, version = version + 1, updated_at_ms = ?3 WHERE id = ?1",
        params![employee.to_string(), reports_to.map(|m| m.to_string()), now_ms()],
    )?;
    Ok(())
}

/// Plugins (`name@marketplace`) an employee has enabled, in the order they were enabled.
pub fn plugins_of(conn: &Connection, employee: EmployeeId) -> Result<Vec<String>, StorageError> {
    let mut stmt = conn.prepare("SELECT plugin_id FROM employee_plugins WHERE employee_id = ?1 ORDER BY enabled_at_ms, plugin_id")?;
    let rows = stmt.query_map([employee.to_string()], |r| r.get(0))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn set_plugins(tx: &Transaction<'_>, employee: EmployeeId, plugins: &[String]) -> Result<(), StorageError> {
    tx.execute("DELETE FROM employee_plugins WHERE employee_id = ?1", [employee.to_string()])?;
    let now = now_ms();
    for (i, plugin) in plugins.iter().enumerate() {
        tx.execute(
            "INSERT OR IGNORE INTO employee_plugins(employee_id, plugin_id, enabled_at_ms) VALUES (?1, ?2, ?3)",
            params![employee.to_string(), plugin, now + i as i64],
        )?;
    }
    Ok(())
}

/// Which employees use which plugin.
pub fn plugin_users(conn: &Connection) -> Result<Vec<(String, EmployeeId)>, StorageError> {
    let mut stmt = conn.prepare("SELECT plugin_id, employee_id FROM employee_plugins ORDER BY plugin_id, employee_id")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, parse(EmployeeId::parse(&r.get::<_, String>(1)?))?)))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Plugins (`name@marketplace`) a project has enabled, in the order they were enabled.
/// Its project assistant and every employee run in its tasks load them.
pub fn project_plugins_of(conn: &Connection, project: ProjectId) -> Result<Vec<String>, StorageError> {
    let mut stmt = conn.prepare("SELECT plugin_id FROM project_plugins WHERE project_id = ?1 ORDER BY enabled_at_ms, plugin_id")?;
    let rows = stmt.query_map([project.to_string()], |r| r.get(0))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn set_project_plugins(tx: &Transaction<'_>, project: ProjectId, plugins: &[String]) -> Result<(), StorageError> {
    tx.execute("DELETE FROM project_plugins WHERE project_id = ?1", [project.to_string()])?;
    let now = now_ms();
    for (i, plugin) in plugins.iter().enumerate() {
        tx.execute(
            "INSERT OR IGNORE INTO project_plugins(project_id, plugin_id, enabled_at_ms) VALUES (?1, ?2, ?3)",
            params![project.to_string(), plugin, now + i as i64],
        )?;
    }
    Ok(())
}

/// Which projects use which plugin.
pub fn project_plugin_users(conn: &Connection) -> Result<Vec<(String, ProjectId)>, StorageError> {
    let mut stmt = conn.prepare("SELECT plugin_id, project_id FROM project_plugins ORDER BY plugin_id, project_id")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, parse(ProjectId::parse(&r.get::<_, String>(1)?))?)))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// The server setting "Follow the org chart".
pub const FOLLOW_ORG_CHART: &str = "follow_org_chart";

/// "Follow the org chart" (server setting); on unless the user turned it off.
pub fn follow_org_chart(conn: &Connection) -> Result<bool, StorageError> {
    Ok(crate::settings::get(conn, FOLLOW_ORG_CHART)?.map(|j| j.trim() != "false").unwrap_or(true))
}

pub fn set_follow_org_chart(tx: &Transaction<'_>, follow: bool) -> Result<(), StorageError> {
    crate::settings::put(tx, FOLLOW_ORG_CHART, if follow { "true" } else { "false" }, now_ms())
}

/// An employee's persistent chat.
pub fn main_chat(conn: &Connection, employee: EmployeeId) -> Result<Option<ConversationId>, StorageError> {
    let id: Option<String> = conn.query_row(&format!("SELECT {}", main_chat_of("?1")), [employee.to_string()], |r| r.get(0)).optional()?.flatten();
    Ok(id.and_then(|c| ConversationId::parse(&c).ok()))
}
