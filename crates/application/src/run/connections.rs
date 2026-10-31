//! Connections and run configuration.
//! Secrets live in the OS credential store; here we only keep references.
//! Every run gets an immutable snapshot of its resolved configuration.

use crate::run::events::{Ev, emit};
use crate::{CommandContext, CommandService, Committed, Followup, domain_error, storage_error};
use aurelius_domain::{
    ActorKind, ProviderId, ConversationId, DomainError,
    connection::{self, ProviderKind, Purpose, resolve_field},
    employee::Setting,
};
use aurelius_domain::{AppError, CommandEnvelope, CommandResult};
use aurelius_storage::{StorageError, Tx, connections as store_conn};
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

/// Resolve connection and model for a new run in `conversation` (employee → project → global).
/// Explicit `Disabled` stays disabled; there is never a silent fallback to another provider.
pub fn resolve(tx: &Tx<'_>, conversation: ConversationId, purpose: Purpose) -> Result<ResolvedRunConfig, StorageError> {
    let layers = store_conn::layers_for(tx, conversation, purpose)?;
    let (connections, models) = store_conn::split_layers(&layers);
    let conn = resolve_field(&connections);
    let row = match conn.value {
        Some(id) => store_conn::load(tx, id)?.filter(|r| r.status != "disconnected"),
        None => None,
    };
    let model = resolve_field(&models);
    let efforts: Vec<_> = layers.iter().map(|(o, s)| (o.clone(), s.effort.clone())).collect();
    let effort = resolve_field(&efforts).value.filter(|e| connection::EFFORT_LEVELS.contains(&e.as_str()));
    let model_value = model.value.clone().or_else(|| row.as_ref().and_then(|r| r.default_model()));
    let model_origin = match (&model.origin, &model.value) {
        (Some(o), Some(_)) => Some(o.label.clone()),
        _ => row.as_ref().map(|r| format!("default of connection {}", r.display_name)),
    };
    let subagents = store_conn::layers_for(tx, conversation, Purpose::Subagent)?;
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
        login_user: None,
    })
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
        let cmd = cmd.clone();
        let command_id = envelope.command_id;
        let meta = ctx.metadata();
        self.commit(command_id, hash, move |tx| {
            let Some(scope) = store_conn::scope_id(tx, cmd.scope)? else {
                return Ok(Err(domain_error(DomainError::OutOfRange { field: "scope".into(), detail: "unknown scope".into() })));
            };
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
