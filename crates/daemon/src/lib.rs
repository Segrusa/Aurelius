//! Daemon composition root. Startup order:
//! instance lock → schema check/migrate → reconcile leases → IPC (clients and
//! the internal bridge channel) → dispatcher.

pub mod api;
mod remote;
mod browse;
mod markdown;
mod uploads;
mod dispatcher;
mod runtimes;
mod tasks;
mod triage;
mod memory;
mod updates;
mod recurring_tasks;
mod permissions;
mod mcp_servers;
mod mcp_oauth;
pub(crate) mod live;
mod load;
mod sessions;
pub mod claude_connect;
pub mod vault;

use aurelius_application::{CommandService, Followup};
use aurelius_domain::RunId;
use aurelius_domain::{EventNotice, HostId, PrincipalId};
use aurelius_protocol::StreamDelta;
#[cfg(feature = "testing")]
use aurelius_runtime::AgentRuntime;
use aurelius_runtime::{Canceller, RuntimeInput};
use aurelius_storage::{Store, queries};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::sync::{broadcast, oneshot, watch};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// How often notifications older than `NOTIFICATION_KEEP_MS` are removed.
const NOTIFICATION_PRUNE_EVERY: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);

pub struct Shared {
    pub host_id: HostId,
    pub principal_id: PrincipalId,
    pub service: CommandService,
    pub events: broadcast::Sender<EventNotice>,
    /// Ephemeral text deltas; dropped for slow consumers.
    pub streams: broadcast::Sender<StreamDelta>,
    /// Test override: when set, every run uses this runtime.
    #[cfg(feature = "testing")]
    pub runtime_override: Option<Arc<dyn AgentRuntime>>,
    #[cfg(feature = "testing")]
    pub fake: Arc<aurelius_runtime::fake::FakeRuntime>,
    pub root: PathBuf,
    /// `root` is this host's default data folder (not a copy or a test instance).
    pub own_root: bool,
    /// Claude Code sessions started before this were started by another data
    /// root (this database is a copy or a restore); the sweep leaves them alone.
    pub sessions_since: Option<i64>,
    /// Cancel switch per running run; stopping one never touches another.
    pub cancellers: Mutex<HashMap<RunId, Canceller>>,
    /// Latest streamed text per running run, for snapshots after reconnect.
    pub live_text: Mutex<HashMap<RunId, String>>,
    pub limits: aurelius_domain::scheduler::Limits,
    /// When a new run may start (run limit, CPU, memory), changed live from
    /// the app; also the `load.changed` pushes.
    pub admission: load::Admission,
    /// The machine's load, measured in the background.
    pub load: load::Meter,
    /// Base for retry backoff; `AURELIUS_BACKOFF_BASE_MS` overrides (tests).
    pub backoff_base_ms: i64,
    /// Input channel per running run (a working mode switched mid-answer).
    pub run_inputs: Mutex<HashMap<RunId, tokio::sync::mpsc::Sender<RuntimeInput>>>,
    /// The working mode each running Claude Code process is in, so a mode the
    /// user switches to mid-answer reaches it (see `permissions`).
    pub run_modes: Mutex<HashMap<RunId, aurelius_domain::autonomy::AgentMode>>,
    /// The mode last sent into each running Claude Code process as a control
    /// request (Auto is completed by the bridge, so it can differ from `run_modes`).
    pub run_switches: Mutex<HashMap<RunId, aurelius_domain::autonomy::AgentMode>>,
    pub shutdown: watch::Sender<bool>,
    pub started_at_ms: i64,
    /// Running jobs: since when, tokens so far (in memory, for the UI).
    pub live: live::Live,
    /// Last `/usage` result per board member's own Claude login; fetched at most once a minute each.
    pub subscription: Mutex<HashMap<aurelius_domain::UserId, aurelius_protocol::SubscriptionUsage>>,
    /// The logins whose `/usage` is being fetched now.
    pub subscription_busy: Mutex<std::collections::HashSet<aurelius_domain::UserId>>,
    pub update: Mutex<aurelius_protocol::UpdateStatus>,
    /// Set after a restore: no jobs are claimed until the user resumes.
    pub paused: std::sync::atomic::AtomicBool,
    /// Optional remote gateway for paired phones; off by default.
    pub remote: aurelius_remote_gateway::Hub,
    /// Stopping to come back: after an update or before a restore. The
    /// installed service starts itself again, so a server without a window
    /// (or with only remote clients) returns on its own.
    pub restart: std::sync::atomic::AtomicBool,
    /// The pending shutdown of this PC (not kept over restarts).
    pub power: aurelius_host::power::Power,
    /// The vault's key (feature `vault`).
    pub vault: vault::Vault,
    /// The chat jobs this daemon drives right now, with the lease epoch each driver
    /// holds; a leased one whose current lease is not among them and ran out lost
    /// its driver (see the dispatcher).
    pub drivers: Mutex<std::collections::HashSet<(String, i64)>>,
    /// How long a run may go without progress, when not the setting's;
    /// `AURELIUS_RUN_IDLE_MS` sets it (tests).
    pub run_idle_ms: Option<i64>,
    /// The limits on `user.login` per username and `user.create` on the server (B9).
    pub sign_in_limits: api::limits::Limits,
    /// The open connections of the board members' sessions (to close them at once).
    pub sessions: api::sessions::Hub,
    /// How often an open connection checks its session against the database:
    /// `AURELIUS_SESSION_CHECK_MS` (tests), else a minute.
    pub session_check_ms: i64,
    /// The running sign-ins of board members' Claude logins (`claude_login.link`).
    pub claude_links: claude_connect::Links,
    /// How long a sign-in may take to show Claude's link: `AURELIUS_CLAUDE_LINK_RECOGNIZE_MS` (tests), else 15 s.
    pub claude_link_recognize: Option<std::time::Duration>,
    /// How long a sign-in may take in all: `AURELIUS_CLAUDE_LINK_TIMEOUT_MS` (tests), else 10 minutes.
    pub claude_link_timeout: Option<std::time::Duration>,
}

impl Shared {
    /// Stop now and start again (`run_service` respawns the installed program).
    pub fn stop_for_restart(&self) {
        self.restart.store(true, std::sync::atomic::Ordering::SeqCst);
        let _ = self.shutdown.send(true);
    }

    /// `stop_for_restart` in a moment, so the reply that asked for it
    /// reaches the client first (update, restore, `server.restart`).
    pub fn restart_after_reply(self: &Arc<Self>) {
        let shared = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            shared.stop_for_restart();
        });
    }
}

impl aurelius_host::Service for Shared {
    fn store(&self) -> &Arc<Store> {
        Shared::store(self)
    }

    fn root(&self) -> &std::path::Path {
        &self.root
    }

    fn version(&self) -> &'static str {
        VERSION
    }

    fn shutdown_signal(&self) -> watch::Receiver<bool> {
        self.shutdown.subscribe()
    }

    fn power(&self) -> &aurelius_host::power::Power {
        &self.power
    }

    fn resume_work(&self) {
        self.paused.store(false, std::sync::atomic::Ordering::Relaxed);
    }

    fn stop_for_restart(&self) {
        Shared::stop_for_restart(self)
    }
}

impl Shared {
    pub fn store(&self) -> &Arc<Store> {
        self.service.store()
    }

    /// Whether this data root started Claude Code session `id` and may remove it.
    pub fn owns_session(&self, id: &str) -> bool {
        aurelius_storage::conversation_sessions::started_since(id, self.sessions_since)
    }

    /// Deliver post-commit work, e.g. a connection check or a mode switch into the running answers.
    pub fn follow_up(self: &Arc<Self>, followups: Vec<Followup>) {
        for followup in followups {
            match followup {
                Followup::VerifyProvider(id) => runtimes::spawn_verification(self.clone(), id),
                Followup::AgentModeChanged(_) => permissions::forward_modes(self.clone()),
            }
        }
    }

    pub fn publish(&self, notices: impl IntoIterator<Item = EventNotice>) {
        for notice in notices {
            // No subscribers is fine; durable events stay in the DB for replay.
            let _ = self.events.send(notice);
        }
    }
}

/// Fake runtime; `AURELIUS_FAKE_DELAY_MS` adjusts streaming speed (tests, demos).
#[cfg(feature = "testing")]
fn fake_runtime() -> aurelius_runtime::fake::FakeRuntime {
    let mut fake = aurelius_runtime::fake::FakeRuntime::default();
    if let Some(ms) = std::env::var("AURELIUS_FAKE_DELAY_MS").ok().and_then(|v| v.parse().ok()) {
        fake.delay = std::time::Duration::from_millis(ms);
    }
    fake
}

/// A panic's message and place go to the log redacted of the vault's values,
/// never as they are to stderr (SEC1 m7): a host that keeps the daemon's
/// stderr would otherwise hold a value a panic quoted.
pub fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| (*s).to_owned())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "(no message)".to_owned());
        let at = info.location().map(|l| format!("{}:{}", l.file(), l.line())).unwrap_or_default();
        let thread = std::thread::current().name().unwrap_or("unnamed").to_owned();
        tracing::error!(%thread, %at, message = %aurelius_runtime::redact::text(&message), "panic");
    }));
}

/// Run until the bridge channel's `daemon.shutdown` (or the tray's quit) is received.
/// `ready` is signalled once the IPC endpoint accepts connections.
pub async fn run(root: PathBuf, ready: Option<oneshot::Sender<()>>) -> anyhow::Result<()> {
    run_inner(root, ready, false, #[cfg(feature = "testing")] None).await.map(drop)
}

/// The installed background service: as `run`, plus the tray icon.
/// Returns `Stopped::Restart` when it stopped to start again.
pub async fn run_service(root: PathBuf) -> anyhow::Result<Stopped> {
    run_inner(root, None, true, #[cfg(feature = "testing")] None).await
}

/// As `run`, telling why it stopped (tests; no tray).
pub async fn run_stopped(root: PathBuf, ready: Option<oneshot::Sender<()>>) -> anyhow::Result<Stopped> {
    run_inner(root, ready, false, #[cfg(feature = "testing")] None).await
}

/// As `run`, with an explicit runtime adapter for every run (tests).
#[cfg(feature = "testing")]
pub async fn run_with_runtime(root: PathBuf, ready: Option<oneshot::Sender<()>>, runtime: Arc<dyn AgentRuntime>) -> anyhow::Result<()> {
    run_inner(root, ready, false, Some(runtime)).await.map(drop)
}

/// Why the service stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stopped {
    Shutdown,
    /// After an update, for a restore or on `server.restart`; the instance
    /// lock is released.
    Restart,
}

/// Old notifications go at the start and then once a day; their events stay.
async fn prune_notifications(shared: Arc<Shared>) {
    let mut every = tokio::time::interval(NOTIFICATION_PRUNE_EVERY);
    loop {
        every.tick().await;
        let store = shared.store().clone();
        let pruned = tokio::task::spawn_blocking(move || store.write(|tx| aurelius_storage::timeline::prune_notifications(tx, aurelius_storage::now_ms()))).await;
        match pruned {
            Ok(Ok(0)) => {}
            Ok(Ok(count)) => tracing::info!(count, "old notifications removed"),
            Ok(Err(err)) => tracing::warn!(error = %err, "could not remove old notifications"),
            Err(err) => tracing::warn!(error = %err, "could not remove old notifications"),
        }
        let store = shared.store().clone();
        let pruned = tokio::task::spawn_blocking(move || store.write(|tx| aurelius_storage::activity_log::prune(tx, aurelius_storage::now_ms()))).await;
        if !matches!(pruned, Ok(Ok(_))) {
            tracing::warn!("could not remove old rows of the activity log");
        }
    }
}

async fn run_inner(
    root: PathBuf,
    ready: Option<oneshot::Sender<()>>,
    with_tray: bool,
    #[cfg(feature = "testing")] runtime_override: Option<Arc<dyn AgentRuntime>>,
) -> anyhow::Result<Stopped> {
    aurelius_host::claim_environment(&root, aurelius_host::Environment::current())?;
    let _lock = aurelius_host::acquire_instance_lock(&root)?;
    let host_id = aurelius_host::host_id(&root)?;
    let principal_id = aurelius_host::local_principal(&root)?;
    // A restore requested in the previous session happens before the database opens.
    match aurelius_storage::backup::perform_pending_restore(&root) {
        Ok(Some(manifest)) => tracing::warn!(created_at_ms = manifest.created_at_ms, "restored a backup; work is paused until resumed"),
        Ok(None) => {}
        Err(err) => {
            tracing::error!(error = %err, "restore failed; keeping the current data");
            let _ = std::fs::remove_file(root.join("data").join("restore-pending"));
        }
    }

    // A schema update gets a backup of the old database first.
    match aurelius_storage::backup::before_migration(&root, VERSION, aurelius_host::Environment::current().as_str()) {
        Ok(Some(dir)) => tracing::info!(dir = %dir.display(), "backup made before the database update"),
        Ok(None) => {}
        Err(err) => tracing::warn!(error = %err, "no backup before the database update"),
    }
    let store = {
        let root = root.clone();
        tokio::task::spawn_blocking(move || -> anyhow::Result<Store> {
            // An auto-update from v0.6 leaves a database this build cannot open; import it first.
            if let Some(r) = aurelius_storage::import::upgrade_legacy(&root)? {
                tracing::info!(projects = r.projects, managers = r.managers, "imported the v0.6 data; the old data is in v0.6-backup");
            }
            // A protocol 3 workspace gets a fresh v0.11 database; the old one is set aside in pre-v0.11
            // (its pre-update backup was made above).
            if let Some(r) = aurelius_storage::fresh_start::upgrade(&root, None)? {
                tracing::info!(report = r.lines(false).join("; "), "fresh start: imported the old workspace; it is kept in pre-v0.11");
            }
            Ok(Store::open(&root)?)
        })
        .await??
    };
    let store = Arc::new(store);
    // The guidelines' starter set, once (B7): only into an empty library with a project
    // "Aurelius"; otherwise only its marker (a board member uses Add starter set).
    {
        let store = store.clone();
        match tokio::task::spawn_blocking(move || store.write(|tx| aurelius_storage::guidelines::seed_guidelines_once(tx, aurelius_storage::now_ms()))).await? {
            Ok(report) if report.ran => {
                tracing::info!(inserted = report.inserted, linked = report.linked, "added the guidelines' starter set");
                if !report.missing.is_empty() {
                    tracing::warn!(missing = %report.missing.join(", "), "starter guidelines not linked: no employee with these names");
                }
            }
            Ok(_) => {}
            Err(err) => tracing::warn!(error = %err, "could not run the guidelines' seed step; it runs again at the next start"),
        }
    }
    // The board members' Claude logins: `logins/` and each folder in it only for this user (D1).
    match aurelius_runtime::claude_code::login::check_all(&root) {
        Ok(fixed) if !fixed.is_empty() => tracing::warn!(folders = fixed.len(), "Claude login folders were open to others; set back to 0700"),
        Ok(_) => {}
        Err(err) => tracing::warn!(error = %err.kind(), "could not check the Claude login folders"),
    }

    // We hold the instance lock, so every lease belongs to a dead daemon.
    // Task executors are resumable from durable state; their leases can be released.
    {
        let store = store.clone();
        tokio::task::spawn_blocking(move || store.write(|tx| queries::release_leases(tx, aurelius_application::task::tasks::TASK_JOB_KIND, i64::MAX))).await??;
    }
    // So is the board's dispatcher job: its run of the dead daemon ends when it is claimed again
    // (`triage_claimed`). This assumes one daemon per data folder (the instance lock): with
    // several, it would free the leases of workers that are still alive.
    {
        let store = store.clone();
        tokio::task::spawn_blocking(move || store.write(|tx| queries::release_leases(tx, aurelius_application::task::triage::TRIAGE_JOB_KIND, i64::MAX))).await??;
    }
    // This host's own data folder, not a copy or a test instance (also with AURELIUS_DATA_DIR).
    let own_root = aurelius_host::default_data_dir().ok().is_some_and(|d| d == root);
    // Claude Code's sessions are per user, shared by every data root: in a copied
    // or restored database only the sessions this root started are ours to remove.
    let sessions_since = {
        let store = store.clone();
        let root = root.to_string_lossy().into_owned();
        tokio::task::spawn_blocking(move || {
            store.write(move |tx| aurelius_storage::conversation_sessions::claim_root(tx, &root, own_root, aurelius_storage::now_ms()))
        })
        .await??
    };
    if let Some(since) = sessions_since {
        tracing::info!(since, "this database came from another data folder; older Claude Code sessions are left alone");
    }
    let remote = aurelius_remote_gateway::Hub::new(store.clone(), host_id, principal_id, root.clone(), VERSION);
    let service = CommandService::new(store);
    #[cfg(feature = "testing")]
    tokio::task::block_in_place(|| service.ensure_builtin_connection())?;
    // Real installs: Documents\Aurelius (or "Aurelius Dev"). Any other data folder
    // (tests, test instances) keeps its projects inside that data folder.
    let standard = aurelius_host::data_dir().ok().is_some_and(|d| d == root);
    if standard || std::env::var_os("AURELIUS_PROJECTS_DIR").is_some() {
        match aurelius_host::projects_root() {
            Ok(projects) => service.set_projects_root(projects),
            Err(err) => tracing::warn!(error = %err, "no folder for projects"),
        }
    } else {
        service.set_projects_root(root.join("projects"));
    }
    // Every project has its own folder (existing projects get theirs on first start).
    if let Err(err) = tokio::task::block_in_place(|| service.ensure_project_folders()) {
        tracing::warn!(error = %err, "could not create project folders");
    }
    let interrupted = tokio::task::block_in_place(|| service.reconcile_chat_runs())?;
    if interrupted > 0 {
        tracing::warn!(interrupted, "chat runs interrupted by a previous daemon stop");
    }

    // The load limits: what the user set in the app, else the environment.
    aurelius_host::chrome::load_stored(service.store());
    let admission = load::Admission::new(load::Limits::from_env(), tokio::task::block_in_place(|| load::stored(service.store())), load::checks_active(standard));
    let (events, _) = broadcast::channel(1024);
    let (streams, _) = broadcast::channel(512);
    let (shutdown, mut shutdown_rx) = watch::channel(false);
    let vault = vault::Vault::open(&root, own_root, service.store().clone());
    let shared = Arc::new(Shared {
        host_id,
        principal_id,
        service,
        events,
        streams,
        #[cfg(feature = "testing")]
        runtime_override,
        #[cfg(feature = "testing")]
        fake: Arc::new(fake_runtime()),
        root: root.clone(),
        own_root,
        sessions_since,
        limits: aurelius_domain::scheduler::Limits::default(),
        admission,
        load: load::Meter::default(),
        backoff_base_ms: std::env::var("AURELIUS_BACKOFF_BASE_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(aurelius_domain::scheduler::BASE_BACKOFF_MS),
        cancellers: Mutex::new(HashMap::new()),
        drivers: Mutex::new(Default::default()),
        run_idle_ms: std::env::var("AURELIUS_RUN_IDLE_MS").ok().and_then(|v| v.parse().ok()).filter(|v: &i64| *v > 0),
        sign_in_limits: Default::default(),
        sessions: Default::default(),
        session_check_ms: std::env::var("AURELIUS_SESSION_CHECK_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(api::SESSION_CHECK_MS),
        claude_links: Default::default(),
        claude_link_recognize: std::env::var("AURELIUS_CLAUDE_LINK_RECOGNIZE_MS").ok().and_then(|v| v.parse().ok()).map(std::time::Duration::from_millis),
        claude_link_timeout: std::env::var("AURELIUS_CLAUDE_LINK_TIMEOUT_MS").ok().and_then(|v| v.parse().ok()).map(std::time::Duration::from_millis),
        subscription: Mutex::new(HashMap::new()),
        subscription_busy: Mutex::new(Default::default()),
        update: Mutex::new(Default::default()),
        paused: std::sync::atomic::AtomicBool::new(aurelius_storage::backup::is_paused(&root)),
        live_text: Mutex::new(HashMap::new()),
        run_inputs: Mutex::new(HashMap::new()),
        run_modes: Mutex::new(HashMap::new()),
        run_switches: Mutex::new(HashMap::new()),
        shutdown,
        started_at_ms: aurelius_storage::now_ms(),
        live: live::Live::default(),
        remote,
        restart: std::sync::atomic::AtomicBool::new(false),
        power: aurelius_host::power::Power::default(),
        vault,
    });
    // Every stored secret in the redactor before any run starts (SEC0 E16).
    {
        let s = shared.clone();
        match tokio::task::spawn_blocking(move || vault::refresh_redactor(&s)).await {
            Ok(Ok(count)) if count > 0 => tracing::info!(count, "vault values in the redactor"),
            Ok(Ok(_)) => {}
            Ok(Err(why)) => tracing::warn!(%why, "the vault's values could not be loaded into the redactor"),
            Err(_) => tracing::warn!("the vault's redactor could not be loaded"),
        }
    }

    let endpoint = aurelius_host::endpoint(&root);
    let mut listener = aurelius_host::ipc::Listener::bind(&endpoint)?;
    // The internal channel of the agent bridge and `aurelius-ctl`; clients never use it.
    let bridge_endpoint = aurelius_host::bridge_endpoint(&root);
    let mut bridge_listener = aurelius_host::ipc::Listener::bind(&bridge_endpoint)?;
    tracing::info!(endpoint = %endpoint.0, bridge = %bridge_endpoint.0, %host_id, "daemon listening");
    if let Some(ready) = ready {
        let _ = ready.send(());
    }

    remote::start(&shared).await;
    updates::start(shared.clone());
    aurelius_host::backups::start(shared.clone());
    recurring_tasks::start(shared.clone());
    aurelius_host::power::start(shared.clone());
    if shared.admission.checks_active {
        load::start(shared.clone());
    }
    if with_tray && std::env::var_os("AURELIUS_NO_TRAY").is_none() {
        let name = if aurelius_host::Environment::current() == aurelius_host::Environment::Dev { "Aurelius Dev" } else { "Aurelius" };
        let quit = shared.clone();
        aurelius_host::tray::start(format!("{name} · background service running"), Box::new(move || {
            tracing::info!("quit from the tray");
            let _ = quit.shutdown.send(true);
        }));
    }
    // Sessions kept for a usage limit whose run was stopped, or ended while the daemon was down.
    sessions::sweep(&shared);
    let dispatcher = tokio::spawn(dispatcher::run(shared.clone()));
    let pruner = tokio::spawn(prune_notifications(shared.clone()));
    let mut connections = tokio::task::JoinSet::new();
    let mut bridge_connections = tokio::task::JoinSet::new();

    loop {
        tokio::select! {
            accepted = listener.accept() => match accepted {
                Ok(stream) => {
                    connections.spawn(api::serve(shared.clone(), stream));
                }
                Err(err) => tracing::warn!(error = %err, "accept failed"),
            },
            accepted = bridge_listener.accept() => match accepted {
                Ok(stream) => {
                    bridge_connections.spawn(api::serve_bridge(shared.clone(), stream));
                }
                Err(err) => tracing::warn!(error = %err, "bridge accept failed"),
            },
            Some(_) = connections.join_next(), if !connections.is_empty() => {}
            Some(_) = bridge_connections.join_next(), if !bridge_connections.is_empty() => {}
            _ = shutdown_rx.changed() => break,
        }
    }

    tracing::info!("controlled shutdown");
    // Sign-ins to Claude end with the daemon (C13).
    shared.claude_links.cancel_all();
    shared.remote.shutdown().await;
    // Give the shutdown response a moment to flush, then close every connection.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    connections.shutdown().await;
    bridge_connections.shutdown().await;
    dispatcher.abort();
    pruner.abort();
    aurelius_host::tray::stop();
    drop(listener);
    drop(bridge_listener);
    Ok(if shared.restart.load(std::sync::atomic::Ordering::SeqCst) { Stopped::Restart } else { Stopped::Shutdown })
}
