//! Runtime selection and connection probes (B09/B12). A run uses exactly the
//! connection in its config snapshot; there is no silent fallback.

use crate::Shared;
use aurelius_application::run::ChatRunPlan;
use aurelius_application::run::connections::ResolvedRunConfig;
use aurelius_domain::{ProviderId, connection::ProviderKind};
use aurelius_protocol::{ErrorCode, ProtocolError};
use aurelius_runtime::{AgentRuntime, RuntimeError};
use aurelius_runtime::claude_code::{ClaudeCodeRuntime, ClaudeLogin};
use std::path::PathBuf;
use std::sync::Arc;

/// Aliases accepted by Claude Code's `--model` (verified in the CLI reference).
pub const CLAUDE_CODE_MODELS: &[&str] = &["sonnet", "opus", "fable", "haiku"];

pub fn runtime_for(shared: &Arc<Shared>, plan: &ChatRunPlan) -> Result<Arc<dyn AgentRuntime>, RuntimeError> {
    runtime_for_config(shared, &plan.config)
}

pub fn runtime_for_config(shared: &Arc<Shared>, config: &ResolvedRunConfig) -> Result<Arc<dyn AgentRuntime>, RuntimeError> {
    #[cfg(feature = "testing")]
    if let Some(runtime) = &shared.runtime_override {
        return Ok(runtime.clone());
    }
    let Some(connection_id) = config.connection_id else {
        return Err(RuntimeError::Permanent(
            "No connection is configured for this conversation. Choose one in Settings (Ctrl ,).".into(),
        ));
    };
    let row = shared
        .service
        .connection(connection_id)
        .map_err(|e| RuntimeError::Unavailable(e.message))?
        .filter(|r| r.status != "disconnected")
        .ok_or_else(|| RuntimeError::Permanent("This conversation's connection was removed.".into()))?;
    match row.provider {
        #[cfg(feature = "testing")]
        ProviderKind::Fake => Ok(shared.fake.clone()),
        #[cfg(not(feature = "testing"))]
        ProviderKind::Fake => Err(RuntimeError::Permanent("The built-in test connection is not available.".into())),
        ProviderKind::ClaudeCode => {
            let executable = row
                .runtime_path
                .clone()
                .ok_or_else(|| RuntimeError::Permanent("The path to Claude Code is missing.".into()))?;
            let login = claude_login(shared, config.login_user)?;
            Ok(Arc::new(ClaudeCodeRuntime {
                executable: PathBuf::from(executable),
                run_root: shared.root.join("runtimes").join(connection_id.to_string()).join("runs"),
                model: config.model.clone(),
                login: Some(login),
            }))
        }
        ProviderKind::AnthropicApi | ProviderKind::OpenaiApi => {
            let secret = row
                .secret_ref
                .as_deref()
                .and_then(|r| aurelius_host::secrets::get(r).ok().flatten())
                .ok_or_else(|| RuntimeError::Auth("No API key has been saved for this connection yet.".into()))?;
            Ok(aurelius_runtime::http_api::for_provider(row.provider, secret, config.model.clone()))
        }
    }
}

/// The Claude login a run on Claude Code uses: its board member's own, linked
/// and not disabled. Never another one and never the daemon user's: without it
/// the run waits for it (`RuntimeError::Login`, reason 'login').
pub fn claude_login(shared: &Arc<Shared>, user: Option<aurelius_domain::UserId>) -> Result<ClaudeLogin, RuntimeError> {
    let Some(user) = user else {
        return Err(RuntimeError::Login("No board member's Claude login belongs to this run.".into()));
    };
    let linked = shared.store().read(|c| aurelius_storage::claude_logins::is_linked(c, &user.to_string())).map_err(|e| RuntimeError::Unavailable(e.to_string()))?;
    if !linked {
        return Err(RuntimeError::Login("The board member's Claude login is not connected.".into()));
    }
    ClaudeLogin::of(&shared.root, user).map_err(|e| RuntimeError::Unavailable(format!("the Claude login folder: {}", e.kind())))
}

/// Probe a connection without sending a prompt, then record status and models.
pub fn spawn_verification(shared: Arc<Shared>, id: ProviderId) {
    tokio::spawn(async move {
        let Ok(Some(row)) = shared.service.connection(id) else { return };
        let (status, models, detail): (&str, Vec<String>, Option<String>) = match row.provider {
            #[cfg(feature = "testing")]
            ProviderKind::Fake => ("ready", vec![aurelius_runtime::fake::MODEL_ID.into()], Some("Local, no network".into())),
            #[cfg(not(feature = "testing"))]
            ProviderKind::Fake => ("unavailable", vec![], Some("The built-in test connection is not available.".into())),
            ProviderKind::ClaudeCode => {
                // Only that it runs: every board member signs in with their own
                // login (`claude_login.link`); the daemon user's is never used.
                let runtime = ClaudeCodeRuntime {
                    executable: PathBuf::from(row.runtime_path.clone().unwrap_or_default()),
                    run_root: shared.root.join("runtimes"),
                    model: None,
                    login: None,
                };
                match runtime.version().await {
                    Err(e) => ("unavailable", vec![], Some(e.to_string())),
                    Ok(version) => ("ready", CLAUDE_CODE_MODELS.iter().map(|s| s.to_string()).collect(), Some(format!("{version} · runs on each board member's own Claude login"))),
                }
            }
            ProviderKind::AnthropicApi | ProviderKind::OpenaiApi => {
                let secret = row.secret_ref.as_deref().and_then(|r| aurelius_host::secrets::get(r).ok().flatten());
                match secret {
                    None => ("needs_auth", vec![], Some("Add your API key".into())),
                    Some(key) => match aurelius_runtime::http_api::list_models(row.provider, &key).await {
                        Ok(models) => ("ready", models, Some("API-key geaccepteerd".into())),
                        Err(RuntimeError::Auth(m)) => ("needs_auth", vec![], Some(m)),
                        Err(e) => ("unavailable", vec![], Some(e.to_string())),
                    },
                }
            }
        };
        let s = shared.clone();
        let detail2 = detail.clone();
        let status2 = status.to_owned();
        if let Ok(Ok(notices)) = tokio::task::spawn_blocking(move || s.service.record_verification(id, &status2, models, detail2)).await {
            shared.publish(notices);
        }
        if status == "ready" {
            // Runs parked for sign-in can continue now.
            let s = shared.clone();
            let _ = tokio::task::spawn_blocking(move || s.service.wake_parked_runs("auth")).await;
        }
    });
}

/// `secret.set`: store in the OS credential store, then re-verify.
pub async fn store_secret(shared: &Arc<Shared>, id: ProviderId, secret: String) -> Result<(), ProtocolError> {
    let row = shared
        .service
        .connection(id)?
        .ok_or_else(|| ProtocolError::new(ErrorCode::NotFound, "This connection does not exist."))?;
    let reference = row
        .secret_ref
        .ok_or_else(|| ProtocolError::new(ErrorCode::Validation, "This connection doesn't use an API key."))?;
    let trimmed = secret.trim().to_owned();
    if trimmed.is_empty() {
        return Err(ProtocolError::new(ErrorCode::Validation, "The API key is empty."));
    }
    tokio::task::spawn_blocking(move || aurelius_host::secrets::set(&reference, &trimmed))
        .await
        .map_err(|e| ProtocolError::new(ErrorCode::Internal, e.to_string()))?
        .map_err(|e| ProtocolError::new(ErrorCode::StorageUnavailable, format!("Could not save the key in the OS credential store: {e}")))?;
    spawn_verification(shared.clone(), id);
    Ok(())
}

/// The plan limits of `user`'s own Claude login (local `/usage` of their login
/// folder: no prompt to a model, no cost). Answers from the cache at once and
/// refreshes it in the background when older than a minute; `refresh` waits for
/// fresh data. Only ever the asker's own (B12); without a board member, nothing.
pub async fn subscription_usage(shared: &Arc<Shared>, user: Option<aurelius_domain::UserId>, refresh: bool) -> aurelius_protocol::SubscriptionUsage {
    let now = aurelius_storage::now_ms();
    let Some(user) = user else {
        return aurelius_protocol::SubscriptionUsage { fetched_at_ms: now, error: Some("Only a board member has a Claude login.".into()), ..Default::default() };
    };
    let cached = shared.subscription.lock().ok().and_then(|c| c.get(&user).cloned());
    if refresh {
        return fetch_subscription(shared, user).await;
    }
    let stale = cached.as_ref().is_none_or(|c| now - c.fetched_at_ms >= 60_000);
    let start = stale && shared.subscription_busy.lock().map(|mut busy| busy.insert(user)).unwrap_or(false);
    if start {
        let s = shared.clone();
        tokio::spawn(async move {
            fetch_subscription(&s, user).await;
            if let Ok(mut busy) = s.subscription_busy.lock() {
                busy.remove(&user);
            }
        });
    }
    cached.unwrap_or(aurelius_protocol::SubscriptionUsage { login_user: Some(user), ..Default::default() })
}

/// Forget what `/usage` said for `user` (their login was unlinked or linked
/// again): the next read fetches it for the login there is now (R2 m5).
pub fn forget_usage(shared: &Shared, user: aurelius_domain::UserId) {
    if let Ok(mut cache) = shared.subscription.lock() {
        cache.remove(&user);
    }
    if let Ok(mut busy) = shared.subscription_busy.lock() {
        busy.remove(&user);
    }
}

/// `/usage` of `user`'s login now. Its lines without "% used" are no limits:
/// no subscription (an API login), never 0% (MU3-09 AC5).
async fn fetch_subscription(shared: &Arc<Shared>, user: aurelius_domain::UserId) -> aurelius_protocol::SubscriptionUsage {
    use aurelius_protocol::{PlanLimitView, SubscriptionUsage};
    let now = aurelius_storage::now_ms();
    let store = shared.store().clone();
    let me = user.to_string();
    let found = tokio::task::spawn_blocking(move || {
        store.read(move |c| Ok((aurelius_storage::connections::find_by_provider(c, ProviderKind::ClaudeCode)?, aurelius_storage::claude_logins::is_linked(c, &me)?)))
    })
    .await
    .ok()
    .and_then(Result::ok);
    let (row, linked) = found.unwrap_or((None, false));
    let path = row.filter(|row| row.status == "ready").and_then(|row| row.runtime_path);
    let failed = |error: String| SubscriptionUsage { login_user: Some(user), fetched_at_ms: now, error: Some(error), ..Default::default() };
    let result = match (path, linked) {
        (None, _) => failed("No working Claude Code connection (Settings → Connections).".into()),
        (Some(_), false) => failed("Connect your Claude login to see your usage.".into()),
        (Some(path), true) => {
            let runtime = ClaudeCodeRuntime { executable: PathBuf::from(path), run_root: shared.root.join("runtimes").join("usage"), model: None, login: Some(ClaudeLogin::at(&shared.root, user)) };
            let plan = runtime.auth_status().await.ok().and_then(|s| s.plan);
            match runtime.subscription_usage().await {
                Ok(raw) => SubscriptionUsage {
                    plan,
                    limits: aurelius_runtime::claude_code::parse_plan_limits(&raw)
                        .into_iter()
                        .map(|l| PlanLimitView { label: l.label, percent: l.percent, resets: l.resets })
                        .collect(),
                    raw,
                    fetched_at_ms: now,
                    error: None,
                    login_user: Some(user),
                },
                Err(e) => SubscriptionUsage { plan, ..failed(e.to_string()) },
            }
        }
    };
    if let Ok(mut cache) = shared.subscription.lock() {
        cache.insert(user, result.clone());
    }
    result
}
