//! Connections and run configuration.
//! Secrets live in the OS credential store; here we only keep references.
//! Every run gets an immutable snapshot of its resolved configuration.

use crate::run::events::{Ev, emit};
use crate::{CommandContext, CommandService, Committed, Followup, domain_error, storage_error};
use aurelius_domain::{
    ActorKind, ProviderId, ConversationId, DomainError,
    connection::{self, ConfigScope, Origin, ProviderKind, Purpose, Resolved, RunDefaults, resolve_field},
    employee::Setting,
};
use aurelius_domain::views::{EmployeeRunLine, EmployeeRunsView, RunValueSource};
use aurelius_domain::{AppError, CommandEnvelope, CommandResult};
use aurelius_storage::{StorageError, Tx, connections as store_conn};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

/// Immutable per-run configuration with the origin of every value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolvedRunConfig {
    pub purpose: Purpose,
    pub connection_id: Option<ProviderId>,
    pub provider: Option<ProviderKind>,
    pub model: Option<String>,
    pub connection_origin: Option<String>,
    pub model_origin: Option<String>,
    /// Chat persona name and instructions (none for employees or when disabled).
    #[serde(default)]
    pub persona_name: Option<String>,
    #[serde(default)]
    pub persona_prompt: Option<String>,
    #[serde(default)]
    pub effort: Option<String>,
    /// Model for the runtime's own subagents (Claude Code's Agent tool): none
    /// = the runtime's default, empty = the runtime decides (set to Disabled).
    #[serde(default)]
    pub subagent_model: Option<String>,
    /// The board member whose own Claude login the run uses (phase 3), fixed
    /// when the run is queued: retries and resumptions keep it (A-02). None: a
    /// run with no board member behind it, which never starts on Claude Code.
    #[serde(default)]
    pub login_user: Option<aurelius_domain::UserId>,
}

impl ResolvedRunConfig {
    pub fn runtime_kind(&self) -> &'static str {
        self.provider.map_or("none", ProviderKind::as_str)
    }
}

fn changed(tx: &Tx<'_>, id: ProviderId, event_type: &'static str, metadata: serde_json::Value, dedupe: String) -> Result<aurelius_domain::EventNotice, StorageError> {
    Ok(emit(tx, Ev {
        aggregate_type: "connection",
        aggregate_id: id.to_string(),
        event_type,
        actor: ActorKind::User,
        actor_id: None,
        correlation: id.to_string(),
        payload: None,
        metadata,
        dedupe,
    })?
    .1)
}

/// The effort the layers choose, and what of it a run can use.
struct PickedEffort {
    resolved: Resolved<String>,
    /// What the run passes; a level the model lacks is left out, as Claude Code would not honour it.
    usable: Option<String>,
}

/// What a set of layers comes to. One function for a run and for the overview,
/// so that what Settings shows is what a run uses.
struct Picked {
    conn: Resolved<ProviderId>,
    row: Option<store_conn::ProviderRow>,
    model: Resolved<String>,
    model_value: Option<String>,
    effort: PickedEffort,
}

fn pick(conn: &Connection, layers: &[(Origin, RunDefaults)]) -> Result<Picked, StorageError> {
    let (connections, models) = store_conn::split_layers(layers);
    let chosen = resolve_field(&connections);
    let row = match chosen.value {
        Some(id) => store_conn::load(conn, id)?.filter(|r| r.status != "disconnected"),
        None => None,
    };
    let model = resolve_field(&models);
    let model_value = model.value.clone().or_else(|| row.as_ref().and_then(|r| r.default_model()));
    let efforts: Vec<_> = layers.iter().map(|(o, s)| (o.clone(), s.effort.clone())).collect();
    let resolved = resolve_field(&efforts);
    let provider = row.as_ref().map(|r| r.provider);
    let usable = resolved.value.clone().filter(|e| connection::supported_efforts(provider, model_value.as_deref()).contains(&e.as_str()));
    Ok(Picked { conn: chosen, row, model, model_value, effort: PickedEffort { resolved, usable } })
}

/// Where a resolved value came from, for the overview; `fallback` when no layer set one.
fn source_of<T>(resolved: &Resolved<T>, fallback: RunValueSource) -> RunValueSource {
    match (&resolved.value, resolved.origin.as_ref().map(|o| o.scope)) {
        (Some(_), Some(ConfigScope::Employee(_))) => RunValueSource::Employee,
        (Some(_), Some(_)) => RunValueSource::Company,
        _ => fallback,
    }
}

/// What a new run of each employee on `reader`'s login uses: `reader`'s own
/// defaults, else the connection's and the model's. A project, a task or a chat
/// can still choose differently for itself; the chat's choice is included per employee.
pub fn employee_runs(conn: &Connection, reader: Option<aurelius_domain::UserId>) -> Result<EmployeeRunsView, StorageError> {
    let mut employees = Vec::new();
    for employee in aurelius_storage::employees::list_employees(conn, &|_| None)? {
        let layers = store_conn::employee_layers(conn, employee.id, &employee.name, Purpose::Employee, reader)?;
        let picked = pick(conn, &layers)?;
        let provider = picked.row.as_ref().map(|r| r.provider);
        let model = picked.model_value.as_deref();
        let model_source = source_of(&picked.model, if picked.row.is_some() { RunValueSource::Connection } else { RunValueSource::Runtime });
        let chat = aurelius_storage::conversations::conversation_run(conn, employee.conversation_id)?;
        let chat_model = chat.model.filter(|m| !m.is_empty());
        let chat_supported = connection::supported_efforts(provider, chat_model.as_deref().or(model));
        employees.push(EmployeeRunLine {
            employee_id: employee.id,
            name: employee.name.clone(),
            status: employee.status,
            connection: picked.row.as_ref().map(|r| r.display_name.clone()),
            model: picked.model_value.clone(),
            model_source,
            effort_source: match &picked.effort.usable {
                Some(_) => source_of(&picked.effort.resolved, RunValueSource::Runtime),
                None => RunValueSource::Runtime,
            },
            unsupported_effort: picked.effort.resolved.value.clone().filter(|_| picked.effort.usable.is_none()),
            effort: picked.effort.usable.clone(),
            model_default_effort: connection::model_default_effort(provider, model).map(str::to_owned),
            supported_efforts: connection::supported_efforts(provider, model).into_iter().map(str::to_owned).collect(),
            own: store_conn::defaults_at(conn, ConfigScope::Employee(employee.id), Purpose::Employee)?.unwrap_or_default(),
            chat_effort: chat.effort.filter(|e| chat_supported.contains(&e.as_str())),
            chat_model,
        });
    }
    let company = store_conn::defaults_at(conn, ConfigScope::Global, Purpose::Employee)?.unwrap_or_default();
    let (provider, company_model) = scope_model(conn, &company, None)?;
    Ok(EmployeeRunsView {
        employees,
        company_supported_efforts: connection::supported_efforts(provider, company_model.as_deref()).into_iter().map(str::to_owned).collect(),
        company_model_default_effort: connection::model_default_effort(provider, company_model.as_deref()).map(str::to_owned),
        company,
        company_model,
    })
}

/// `user.run_defaults`: what `user` chose and what a run of theirs comes to
/// without a choice of its own, with the company's connection.
pub fn user_runs(conn: &Connection, user: aurelius_domain::UserId) -> Result<aurelius_domain::user::UserRunDefaultsView, StorageError> {
    let defaults = aurelius_storage::user_run_defaults::get(conn, user)?;
    let mut layers: Vec<_> = store_conn::user_layer(conn, Some(user), Purpose::Employee)?.into_iter().collect();
    if let Some(company) = store_conn::defaults_at(conn, ConfigScope::Global, Purpose::Employee)? {
        // Only its connection still counts (migration 52).
        let connection_only = RunDefaults { connection: company.connection, ..Default::default() };
        layers.push((Origin { scope: ConfigScope::Global, label: "global default".into() }, connection_only));
    }
    let picked = pick(conn, &layers)?;
    let provider = picked.row.as_ref().map(|r| r.provider);
    let model = picked.model_value.as_deref();
    Ok(aurelius_domain::user::UserRunDefaultsView {
        unsupported_effort: picked.effort.resolved.value.clone().filter(|_| picked.effort.usable.is_none()),
        supported_efforts: connection::supported_efforts(provider, model).into_iter().map(str::to_owned).collect(),
        model_default_effort: connection::model_default_effort(provider, model).map(str::to_owned),
        model: picked.model_value.clone(),
        defaults,
    })
}

/// The provider and model of `settings`, with what they inherit from `below`.
fn scope_model(conn: &Connection, settings: &RunDefaults, below: Option<&RunDefaults>) -> Result<(Option<ProviderKind>, Option<String>), StorageError> {
    let origin = Origin { scope: ConfigScope::Global, label: String::new() };
    let mut layers = vec![(origin.clone(), settings.clone())];
    layers.extend(below.map(|b| (origin, b.clone())));
    let picked = pick(conn, &layers)?;
    Ok((picked.row.as_ref().map(|r| r.provider), picked.model_value))
}

/// Resolve connection and model for a new run in `conversation` on `login`'s
/// Claude login: the conversation's and its task's choice, then `login`'s own
/// defaults, then the connection's default model. The connection comes from
/// employee → project → global. Explicit `Disabled` stays disabled; there is
/// never a silent fallback to another provider.
pub fn resolve(tx: &Tx<'_>, conversation: ConversationId, purpose: Purpose, login: Option<aurelius_domain::UserId>) -> Result<ResolvedRunConfig, StorageError> {
    let layers = store_conn::layers_for(tx, conversation, purpose, login)?;
    let Picked { conn, row, model, model_value, effort: chosen, .. } = pick(tx, &layers)?;
    let effort = chosen.usable;
    let model_origin = match (&model.origin, &model.value) {
        (Some(o), Some(_)) => Some(o.label.clone()),
        _ => row.as_ref().map(|r| format!("default of connection {}", r.display_name)),
    };
    let subagents = store_conn::layers_for(tx, conversation, Purpose::Subagent, login)?;
    let (_, subagent_models) = store_conn::split_layers(&subagents);
    let subagent = resolve_field(&subagent_models);
    let subagent_model = match (subagent.value, subagent.origin) {
        (Some(model), _) => Some(model),
        (None, Some(_)) => Some(String::new()),
        (None, None) => None,
    };
    // Chat answers neutrally: no built-in persona.
    let (persona_name, persona_prompt): (Option<String>, Option<String>) = (None, None);
    Ok(ResolvedRunConfig {
        persona_name,
        persona_prompt,
        purpose,
        connection_id: row.as_ref().map(|r| r.id),
        provider: row.as_ref().map(|r| r.provider),
        model: model_value,
        effort,
        subagent_model,
        connection_origin: conn.origin.map(|o| o.label),
        model_origin,
        login_user: login,
    })
}

/// Why the effort of this change cannot be stored. Only a newly chosen effort
/// is judged: changing the model must not fail on the effort it already had.
fn refused_effort(conn: &Connection, cmd: &connection::SetRunDefaults) -> Result<Option<String>, StorageError> {
    let Setting::Value(effort) = &cmd.settings.effort else { return Ok(None) };
    let stored = store_conn::defaults_at(conn, cmd.scope, cmd.purpose)?.unwrap_or_default();
    if stored.effort == cmd.settings.effort {
        return Ok(None);
    }
    if cmd.purpose != Purpose::Employee {
        // Subagents and memory have their own models; only the level can be judged here.
        return Ok(connection::effort_refusal(effort, None, None));
    }
    let company = store_conn::defaults_at(conn, ConfigScope::Global, cmd.purpose)?;
    let (provider, model) = scope_model(conn, &cmd.settings, company.as_ref().filter(|_| cmd.scope != ConfigScope::Global))?;
    Ok(connection::effort_refusal(effort, provider, model.as_deref()))
}

/// Why `user`'s new defaults cannot be stored: an effort their model does not
/// take (their own model, else the company connection's default). Only a newly
/// chosen effort is judged, as for `SetRunDefaults`.
pub(crate) fn refused_user_effort(conn: &Connection, user: aurelius_domain::UserId, defaults: &aurelius_domain::user::UserRunDefaults) -> Result<Option<String>, StorageError> {
    let Some(effort) = defaults.effort.as_deref() else { return Ok(None) };
    if aurelius_storage::user_run_defaults::get(conn, user)?.effort == defaults.effort {
        return Ok(None);
    }
    let own = RunDefaults { model: defaults.model.clone().map(Setting::Value).unwrap_or_default(), ..Default::default() };
    let company = store_conn::defaults_at(conn, ConfigScope::Global, Purpose::Employee)?.map(|c| RunDefaults { connection: c.connection, ..Default::default() });
    let (provider, model) = scope_model(conn, &own, company.as_ref())?;
    Ok(connection::effort_refusal(effort, provider, model.as_deref()))
}

impl CommandService {
    /// A built-in local test connection as global default, so tests and demos run
    /// without a provider. Production has none: the user adds a real connection.
    #[cfg(feature = "testing")]
    pub fn ensure_builtin_connection(&self) -> Result<(), AppError> {
        self.store
            .write(|tx| {
                if store_conn::find_by_provider(tx, ProviderKind::Fake)?.is_some() {
                    return Ok(());
                }
                let id = ProviderId::new();
                store_conn::insert(tx, id, ProviderKind::Fake, "Local test runtime", None, None, &serde_json::json!({ "default_model": aurelius_runtime::fake::MODEL_ID, "models": [aurelius_runtime::fake::MODEL_ID] }))?;
                let n = changed(tx, id, "connection.changed", serde_json::json!({ "action": "builtin" }), format!("connection:{id}:builtin"))?;
                let global = store_conn::scope_id(tx, connection::ConfigScope::Global)?.expect("global scope");
                let event_id = tx.query_row("SELECT id FROM events WHERE seq = ?1", [n.seq as i64], |r| r.get::<_, String>(0))?;
                let event_id = aurelius_domain::EventId::parse(&event_id).map_err(|e| StorageError::Rejected(e.to_string()))?;
                let settings = connection::RunDefaults { connection: Setting::Value(id), model: Setting::Inherit, persona: Setting::Inherit, effort: Setting::Inherit };
                for purpose in [Purpose::Employee, Purpose::Memory] {
                    store_conn::upsert_defaults(tx, &global, purpose, &settings, event_id)?;
                }
                Ok(())
            })
            .map_err(storage_error)
    }

    pub(crate) fn add_connection(&self, ctx: &CommandContext, envelope: &CommandEnvelope, hash: String, cmd: &connection::AddProvider) -> Result<Committed, AppError> {
        let (name, path, model) = connection::validate_add(cmd).map_err(domain_error)?;
        let provider = cmd.provider;
        let id = ProviderId::new();
        let command_id = envelope.command_id;
        let meta = ctx.metadata();
        let mut committed = self.commit(command_id, hash, move |tx| {
            let secret_ref = provider.needs_api_key().then(|| format!("conn-{id}"));
            let config = serde_json::json!({ "default_model": model });
            store_conn::insert(tx, id, provider, &name, secret_ref.as_deref(), path.as_deref(), &config)?;
            let n = changed(tx, id, "connection.changed", serde_json::json!({ "action": "added", "provider": provider.as_str(), "ctx": meta }), format!("cmd:{command_id}:0"))?;
            Ok(Ok((CommandResult::ProviderAdded { connection_id: id }, vec![n])))
        })?;
        if !committed.receipt.replayed {
            committed.followups.push(Followup::VerifyProvider(id));
        }
        Ok(committed)
    }

    pub(crate) fn remove_connection(&self, ctx: &CommandContext, envelope: &CommandEnvelope, hash: String, cmd: &connection::RemoveProvider) -> Result<Committed, AppError> {
        let id = cmd.connection_id;
        let command_id = envelope.command_id;
        let meta = ctx.metadata();
        self.commit(command_id, hash, move |tx| {
            if store_conn::load(tx, id)?.is_none() {
                return Ok(Err(domain_error(DomainError::ProviderNotFound(id))));
            }
            store_conn::set_status(tx, id, "disconnected", &serde_json::json!({}))?;
            let n = changed(tx, id, "connection.changed", serde_json::json!({ "action": "removed", "ctx": meta }), format!("cmd:{command_id}:0"))?;
            Ok(Ok((CommandResult::ProviderRemoved { connection_id: id }, vec![n])))
        })
    }

    pub(crate) fn verify_connection(&self, envelope: &CommandEnvelope, hash: String, cmd: &connection::VerifyProvider) -> Result<Committed, AppError> {
        let id = cmd.connection_id;
        let command_id = envelope.command_id;
        let mut committed = self.commit(command_id, hash, move |tx| {
            if store_conn::load(tx, id)?.is_none() {
                return Ok(Err(domain_error(DomainError::ProviderNotFound(id))));
            }
            Ok(Ok((CommandResult::VerificationStarted { connection_id: id }, vec![])))
        })?;
        if !committed.receipt.replayed {
            committed.followups.push(Followup::VerifyProvider(id));
        }
        Ok(committed)
    }

    pub(crate) fn set_run_defaults(&self, ctx: &CommandContext, envelope: &CommandEnvelope, hash: String, cmd: &connection::SetRunDefaults) -> Result<Committed, AppError> {
        crate::ticket::require_board_member(ctx)?;
        let cmd = cmd.clone();
        let command_id = envelope.command_id;
        let meta = ctx.metadata();
        self.commit(command_id, hash, move |tx| {
            let Some(scope) = store_conn::scope_id(tx, cmd.scope)? else {
                return Ok(Err(domain_error(DomainError::OutOfRange { field: "scope".into(), detail: "unknown scope".into() })));
            };
            if let Some(detail) = refused_effort(tx, &cmd)? {
                return Ok(Err(domain_error(DomainError::OutOfRange { field: "effort".into(), detail })));
            }
            if let Setting::Value(id) = cmd.settings.connection {
                if store_conn::load(tx, id)?.filter(|r| r.status != "disconnected").is_none() {
                    return Ok(Err(domain_error(DomainError::ProviderNotFound(id))));
                }
            }
            let (event_id, n) = emit(tx, Ev {
                aggregate_type: "config",
                aggregate_id: format!("{scope}:{}", cmd.purpose.as_str()),
                event_type: "config.changed",
                actor: ActorKind::User,
                actor_id: None,
                correlation: command_id.to_string(),
                payload: None,
                metadata: serde_json::json!({ "settings": cmd.settings, "ctx": meta }),
                dedupe: format!("cmd:{command_id}:0"),
            })?;
            store_conn::upsert_defaults(tx, &scope, cmd.purpose, &cmd.settings, event_id)?;
            Ok(Ok((CommandResult::RunDefaultsSet, vec![n])))
        })
    }

    /// Store a probe result. Called by the daemon after it contacted the provider.
    pub fn record_verification(&self, id: ProviderId, status: &str, models: Vec<String>, detail: Option<String>) -> Result<Vec<aurelius_domain::EventNotice>, AppError> {
        let status = status.to_owned();
        self.store
            .write(move |tx| {
                let patch = serde_json::json!({ "models": models, "detail": detail, "last_verified_ms": aurelius_storage::now_ms() });
                store_conn::set_status(tx, id, &status, &patch)?;
                let version = crate::chat_next_version(tx, "connection", &id.to_string())?;
                Ok(vec![changed(tx, id, "connection.changed", serde_json::json!({ "action": "verified", "status": status }), format!("connection:{id}:verified:{version}"))?])
            })
            .map_err(storage_error)
    }

    pub fn connection(&self, id: ProviderId) -> Result<Option<store_conn::ProviderRow>, AppError> {
        self.store.read(|c| store_conn::load(c, id)).map_err(storage_error)
    }
}

impl aurelius_storage::conversations::RunConfigInfo for ResolvedRunConfig {
    fn runtime_kind(&self) -> &'static str {
        ResolvedRunConfig::runtime_kind(self)
    }
    fn connection(&self) -> Option<String> {
        self.connection_id.map(|c| c.to_string())
    }
    fn model(&self) -> Option<String> {
        self.model.clone()
    }
    fn login_user(&self) -> Option<String> {
        self.login_user.map(|u| u.to_string())
    }
}
