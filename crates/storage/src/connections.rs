//! Connections and per-scope run defaults (`config_overrides`).

use crate::{StorageError, now_ms};
use aurelius_domain::{
    ProviderId, ConversationId, EventId,
    connection::{ConfigScope, Origin, ProviderKind, Purpose, RunDefaults},
    employee::Setting,
};
use aurelius_domain::views::{ProviderView, DefaultsView};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

fn parse<T, E: std::error::Error + Send + Sync + 'static>(r: Result<T, E>) -> rusqlite::Result<T> {
    r.map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))
}

pub struct ProviderRow {
    pub id: ProviderId,
    pub provider: ProviderKind,
    pub display_name: String,
    pub status: String,
    pub secret_ref: Option<String>,
    pub runtime_path: Option<String>,
    pub config: serde_json::Value,
}

impl ProviderRow {
    pub fn default_model(&self) -> Option<String> {
        self.config["default_model"].as_str().map(str::to_owned)
    }
}

const COLUMNS: &str = "id, provider_kind, display_name, status, secret_ref, runtime_path, config_json";

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<ProviderRow> {
    let provider: String = r.get(1)?;
    let config: String = r.get(6)?;
    Ok(ProviderRow {
        id: parse(ProviderId::parse(&r.get::<_, String>(0)?))?,
        provider: ProviderKind::parse(&provider).unwrap_or(ProviderKind::Fake),
        display_name: r.get(2)?,
        status: r.get(3)?,
        secret_ref: r.get(4)?,
        runtime_path: r.get(5)?,
        config: serde_json::from_str(&config).unwrap_or_default(),
    })
}

pub fn insert(
    tx: &Transaction<'_>,
    id: ProviderId,
    provider: ProviderKind,
    name: &str,
    secret_ref: Option<&str>,
    runtime_path: Option<&str>,
    config: &serde_json::Value,
) -> Result<(), StorageError> {
    tx.execute(
        "INSERT INTO connections(id, provider_kind, auth_kind, display_name, secret_ref, runtime_path, status, config_json, updated_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            id.to_string(),
            provider.as_str(),
            provider.auth_kind(),
            name,
            secret_ref,
            runtime_path,
            if provider == ProviderKind::Fake { "ready" } else { "needs_auth" },
            config.to_string(),
            now_ms()
        ],
    )?;
    Ok(())
}

pub fn load(conn: &Connection, id: ProviderId) -> Result<Option<ProviderRow>, StorageError> {
    Ok(conn.query_row(&format!("SELECT {COLUMNS} FROM connections WHERE id = ?1"), [id.to_string()], row).optional()?)
}

pub fn find_by_provider(conn: &Connection, provider: ProviderKind) -> Result<Option<ProviderRow>, StorageError> {
    Ok(conn
        .query_row(
            &format!("SELECT {COLUMNS} FROM connections WHERE provider_kind = ?1 AND status <> 'disconnected' ORDER BY rowid LIMIT 1"),
            [provider.as_str()],
            row,
        )
        .optional()?)
}

pub fn set_status(tx: &Transaction<'_>, id: ProviderId, status: &str, config_patch: &serde_json::Value) -> Result<(), StorageError> {
    let current: String = tx.query_row("SELECT config_json FROM connections WHERE id = ?1", [id.to_string()], |r| r.get(0))?;
    let mut config: serde_json::Value = serde_json::from_str(&current).unwrap_or_else(|_| serde_json::json!({}));
    if let (Some(target), Some(patch)) = (config.as_object_mut(), config_patch.as_object()) {
        for (k, v) in patch {
            target.insert(k.clone(), v.clone());
        }
    }
    tx.execute(
        "UPDATE connections SET status = ?2, config_json = ?3, updated_at_ms = ?4 WHERE id = ?1",
        params![id.to_string(), status, config.to_string(), now_ms()],
    )?;
    Ok(())
}

pub fn list(conn: &Connection, has_secret: &dyn Fn(&str) -> bool) -> Result<Vec<ProviderView>, StorageError> {
    let mut stmt = conn.prepare(&format!("SELECT {COLUMNS} FROM connections WHERE status <> 'disconnected' ORDER BY rowid"))?;
    let rows = stmt.query_map([], row)?;
    let mut out = Vec::new();
    for r in rows {
        let r = r?;
        out.push(ProviderView {
            id: r.id,
            provider: r.provider.as_str().into(),
            display_name: r.display_name.clone(),
            auth_kind: r.provider.auth_kind().into(),
            billing: r.provider.billing().into(),
            status: r.status.clone(),
            has_secret: r.secret_ref.as_deref().is_some_and(has_secret),
            runtime_path: r.runtime_path.clone(),
            default_model: r.default_model(),
            models: serde_json::from_value(r.config["models"].clone()).unwrap_or_default(),
            detail: r.config["detail"].as_str().map(str::to_owned),
            last_verified_ms: r.config["last_verified_ms"].as_i64(),
        });
    }
    Ok(out)
}

// ── Scoped defaults ─────────────────────────────────────────────────────

/// The scope row id for a config scope (global scope is created on demand).
pub fn scope_id(tx: &Transaction<'_>, scope: ConfigScope) -> Result<Option<String>, StorageError> {
    Ok(match scope {
        ConfigScope::Global => Some(crate::events::ensure_global_scope(tx)?),
        ConfigScope::Project(p) => tx.query_row("SELECT scope_id FROM projects WHERE id = ?1", [p.to_string()], |r| r.get(0)).optional()?,
        ConfigScope::Employee(m) => tx.query_row("SELECT scope_id FROM employees WHERE id = ?1", [m.to_string()], |r| r.get(0)).optional()?,
    })
}

pub fn upsert_defaults(tx: &Transaction<'_>, scope_id: &str, purpose: Purpose, settings: &RunDefaults, source_event: EventId) -> Result<(), StorageError> {
    tx.execute(
        "INSERT INTO config_overrides(scope_id, purpose, version, config_json, source_event_id) VALUES (?1, ?2, 1, ?3, ?4)
         ON CONFLICT(scope_id, purpose) DO UPDATE SET version = version + 1, config_json = excluded.config_json, source_event_id = excluded.source_event_id",
        params![scope_id, purpose.as_str(), serde_json::to_string(settings)?, source_event.to_string()],
    )?;
    Ok(())
}

pub fn list_defaults(conn: &Connection) -> Result<Vec<DefaultsView>, StorageError> {
    let mut stmt = conn.prepare(
        "SELECT s.kind, s.owner_id, o.purpose, o.config_json FROM config_overrides o JOIN scopes s ON s.id = o.scope_id",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?)))?;
    let mut out = Vec::new();
    for r in rows {
        let (kind, owner, purpose, config) = r?;
        let scope = match (kind.as_str(), owner) {
            ("project", Some(o)) => aurelius_domain::ProjectId::parse(&o).map(ConfigScope::Project).ok(),
            ("employee", Some(o)) => aurelius_domain::EmployeeId::parse(&o).map(ConfigScope::Employee).ok(),
            _ => Some(ConfigScope::Global),
        };
        let purpose: Option<Purpose> = serde_json::from_value(serde_json::Value::String(purpose)).ok();
        if let (Some(scope), Some(purpose)) = (scope, purpose) {
            out.push(DefaultsView { scope, purpose, settings: serde_json::from_str(&config).unwrap_or_default() });
        }
    }
    Ok(out)
}

/// Layers for a conversation, most specific first: employee, project, global.
/// Subagents get only their own scoped defaults, not what was chosen for the conversation.
pub fn layers_for(conn: &Connection, conversation: ConversationId, purpose: Purpose) -> Result<Vec<(Origin, RunDefaults)>, StorageError> {
    let (project, employee, employee_name, project_name): (Option<String>, Option<String>, Option<String>, Option<String>) = conn
        .query_row(
            "SELECT c.project_id, c.employee_id, m.name, p.name FROM conversations c
             LEFT JOIN employees m ON m.id = c.employee_id LEFT JOIN projects p ON p.id = c.project_id WHERE c.id = ?1",
            [conversation.to_string()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()?
        .unwrap_or((None, None, None, None));
    let lookup = |scope_kind: &str, owner: Option<&str>, purpose: Purpose| -> Result<Option<RunDefaults>, StorageError> {
        let config: Option<String> = conn
            .query_row(
                "SELECT o.config_json FROM config_overrides o JOIN scopes s ON s.id = o.scope_id
                 WHERE s.kind = ?1 AND (?2 IS NULL OR s.owner_id = ?2) AND o.purpose = ?3",
                params![scope_kind, owner, purpose.as_str()],
                |r| r.get(0),
            )
            .optional()?;
        Ok(config.and_then(|c| serde_json::from_str(&c).ok()))
    };
    let mut layers = Vec::new();
    // What was chosen for this conversation itself comes first, then its task's.
    let chosen = purpose != Purpose::Subagent;
    // An employee's conversation, or a project conversation (no employee, a project).
    if chosen && (employee.is_some() || project.is_some()) {
        let run = crate::conversations::conversation_run(conn, conversation)?;
        if !run.is_empty() {
            let value = |v: Option<String>| v.map(aurelius_domain::employee::Setting::Value).unwrap_or_default();
            let settings = RunDefaults { connection: Default::default(), model: value(run.model), persona: Default::default(), effort: value(run.effort) };
            let scope = match (employee.as_deref(), project.as_deref()) {
                (Some(m), _) => ConfigScope::Employee(aurelius_domain::EmployeeId::parse(m).map_err(|e| StorageError::Rejected(e.to_string()))?),
                (None, Some(p)) => ConfigScope::Project(aurelius_domain::ProjectId::parse(p).map_err(|e| StorageError::Rejected(e.to_string()))?),
                (None, None) => unreachable!("checked above"),
            };
            layers.push((Origin { scope, label: "this conversation".into() }, settings));
        }
    }
    if let Some(m) = employee.as_deref().filter(|_| chosen) {
        let task: Option<(String, Option<String>)> = conn
            .query_row("SELECT title, run_json FROM tasks WHERE conversation_id = ?1", [conversation.to_string()], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?;
        if let Some((title, Some(json))) = task {
            if let Ok(run) = serde_json::from_str::<aurelius_domain::task::RunSettings>(&json) {
                let value = |v: Option<String>| v.map(aurelius_domain::employee::Setting::Value).unwrap_or_default();
                let settings = RunDefaults {
                    connection: run.connection_id.map(aurelius_domain::employee::Setting::Value).unwrap_or_default(),
                    // A connection without a model: that connection's own default, not an inherited one.
                    model: value(run.model.or_else(|| run.connection_id.and_then(|id| load(conn, id).ok().flatten()).and_then(|r| r.default_model()))),
                    persona: Default::default(),
                    effort: value(run.effort),
                };
                let id = aurelius_domain::EmployeeId::parse(m).map_err(|e| StorageError::Rejected(e.to_string()))?;
                layers.push((Origin { scope: ConfigScope::Employee(id), label: format!("task {title}") }, settings));
            }
        }
    }
    let mut scoped = |purpose: Purpose, what: &str| -> Result<(), StorageError> {
        if let (Some(m), Some(name)) = (employee.as_deref(), employee_name.as_deref()) {
            if let Some(s) = lookup("employee", Some(m), purpose)? {
                let id = aurelius_domain::EmployeeId::parse(m).map_err(|e| StorageError::Rejected(e.to_string()))?;
                layers.push((Origin { scope: ConfigScope::Employee(id), label: format!("{what}employee {name}") }, s));
            }
        }
        if let (Some(p), Some(name)) = (project.as_deref(), project_name.as_deref()) {
            if let Some(s) = lookup("project", Some(p), purpose)? {
                let id = aurelius_domain::ProjectId::parse(p).map_err(|e| StorageError::Rejected(e.to_string()))?;
                layers.push((Origin { scope: ConfigScope::Project(id), label: format!("{what}project {name}") }, s));
            }
        }
        if let Some(s) = lookup("global", None, purpose)? {
            let label = if what.is_empty() { "global default".to_owned() } else { format!("{what}global default") };
            layers.push((Origin { scope: ConfigScope::Global, label }, s));
        }
        Ok(())
    };
    scoped(purpose, "")?;
    Ok(layers)
}

pub fn split_layers(layers: &[(Origin, RunDefaults)]) -> (Vec<(Origin, Setting<ProviderId>)>, Vec<(Origin, Setting<String>)>) {
    (
        layers.iter().map(|(o, s)| (o.clone(), s.connection.clone())).collect(),
        layers.iter().map(|(o, s)| (o.clone(), s.model.clone())).collect(),
    )
}
