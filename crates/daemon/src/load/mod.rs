//! Machine load and run admission. A background meter samples the machine's
//! CPU use and available memory every 2 s and keeps the average of the last
//! ~10 s; before the dispatcher starts a new run it asks `admit`. Runs that
//! already run are never stopped, and with nothing running one always starts
//! (unless the run limit is 0). The user changes the limits from the app
//! (`load/set_limits`); they apply at once and are kept in `settings`.

use crate::Shared;
use aurelius_protocol::{ErrorCode, LoadGate, LoadHold, LoadHoldKind, LoadLimits, LoadLimitsUpdate, LoadSnapshot, ProtocolError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

mod meter;
#[cfg(test)]
mod tests;

pub use meter::{Meter, start};
#[cfg(test)]
use meter::average;

const MB: u64 = 1024 * 1024;
/// While runs wait, `load.changed` goes out at most this often (live CPU).
const PUSH_EVERY: Duration = Duration::from_secs(2);
/// The `settings` key of the limits the user set.
pub const SETTINGS_KEY: &str = "load_limits";

/// The machine's load, averaged over the window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Load {
    /// All cores together, 0–100.
    pub cpu_percent: f32,
    pub available_memory: u64,
}

/// When a new run may start. `AURELIUS_MAX_RUNS`, `AURELIUS_CPU_HIGH`,
/// `AURELIUS_CPU_LOW`, `AURELIUS_MIN_FREE_MB` override the defaults;
/// `AURELIUS_LOAD_ADMISSION=on|off` turns the CPU and memory checks on or off.
/// What the user set in the app wins over both (see `Admission`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Limits {
    /// Hard limit on runs at the same time (chat answers and task starts);
    /// 0 holds every new run.
    pub max_runs: u32,
    /// Hold new runs from this CPU use…
    pub cpu_high: f32,
    /// …until it is below this again (hysteresis).
    pub cpu_low: f32,
    pub min_available_memory: u64,
    /// Whether the measured load counts at all (only the hard limit otherwise).
    /// It only counts where the checks are active (`Admission::checks_active`).
    pub measured: bool,
}

impl Default for Limits {
    fn default() -> Self {
        Self { max_runs: 3, cpu_high: 75.0, cpu_low: 65.0, min_available_memory: 2048 * MB, measured: true }
    }
}

impl Limits {
    /// The limits from the environment.
    pub fn from_env() -> Self {
        Self::from_vars(|key| std::env::var(key).ok())
    }

    fn from_vars(var: impl Fn(&str) -> Option<String>) -> Self {
        let d = Self::default();
        let num = |key: &str| var(key).and_then(|v| v.trim().parse::<f64>().ok()).filter(|v| v.is_finite() && *v >= 0.0);
        let cpu_high = num("AURELIUS_CPU_HIGH").map_or(d.cpu_high, |v| v.min(100.0) as f32);
        let cpu_low = num("AURELIUS_CPU_LOW").map_or(d.cpu_low.min(cpu_high), |v| v as f32).min(cpu_high);
        let measured = match var("AURELIUS_LOAD_ADMISSION").as_deref().map(str::trim) {
            Some("on" | "1" | "true") => true,
            Some("off" | "0" | "false") => false,
            _ => d.measured,
        };
        Self {
            max_runs: num("AURELIUS_MAX_RUNS").map_or(d.max_runs, |v| (v as u32).max(1)),
            cpu_high,
            cpu_low,
            min_available_memory: num("AURELIUS_MIN_FREE_MB").map_or(d.min_available_memory, |v| v as u64 * MB),
            measured,
        }
    }

    /// These limits with what the user set on top.
    pub fn with(mut self, set: &LoadLimitsUpdate) -> Self {
        if let Some(v) = set.max_runs {
            self.max_runs = v;
        }
        if let Some(v) = set.cpu_high {
            self.cpu_high = f32::from(v);
        }
        if let Some(v) = set.cpu_low {
            self.cpu_low = f32::from(v);
        }
        if let Some(v) = set.mem_min_mb {
            self.min_available_memory = v.saturating_mul(MB);
        }
        if let Some(v) = set.enabled {
            self.measured = v;
        }
        self
    }

    pub fn view(&self) -> LoadLimits {
        LoadLimits {
            max_runs: self.max_runs,
            cpu_high: self.cpu_high.round().clamp(0.0, 100.0) as u8,
            cpu_low: self.cpu_low.round().clamp(0.0, 100.0) as u8,
            mem_min_mb: self.min_available_memory / MB,
            enabled: self.measured,
        }
    }
}

/// The CPU and memory checks only run for the standard data folder (or with
/// `AURELIUS_LOAD_ADMISSION=on`): tests and throwaway instances keep only the
/// run limit, as a busy test machine would hold their runs back.
pub fn checks_active(standard: bool) -> bool {
    standard || matches!(std::env::var("AURELIUS_LOAD_ADMISSION").as_deref().map(str::trim), Ok("on" | "1" | "true"))
}

/// Refuse limits that make no sense (only the CPU marks can).
fn validate(update: &LoadLimitsUpdate, limits: &Limits) -> Result<(), String> {
    if update.cpu_high.is_some_and(|v| v > 100) {
        return Err("The CPU limit is a percentage: at most 100.".into());
    }
    if (update.cpu_high.is_some() || update.cpu_low.is_some()) && limits.cpu_low >= limits.cpu_high {
        return Err(format!(
            "New runs start again once the CPU is below {} %; that must be lower than the {} % that holds them.",
            limits.cpu_low, limits.cpu_high
        ));
    }
    Ok(())
}

/// Why a new run waits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hold {
    MaxRuns,
    Cpu,
    Memory,
    /// The run limit is 0.
    DisabledAll,
}

impl Hold {
    /// The reason told in the conversation (`run.waiting`).
    pub fn code(self) -> &'static str {
        match self {
            Hold::MaxRuns => "load:max_runs",
            Hold::Cpu => "load:cpu",
            Hold::Memory => "load:memory",
            Hold::DisabledAll => "load:disabled_all",
        }
    }

    fn kind(self) -> LoadHoldKind {
        match self {
            Hold::MaxRuns => LoadHoldKind::MaxRuns,
            Hold::Cpu => LoadHoldKind::Cpu,
            Hold::Memory => LoadHoldKind::Memory,
            Hold::DisabledAll => LoadHoldKind::DisabledAll,
        }
    }
}

/// What `admit` remembers between calls: whether the CPU is "hot" (above
/// the high mark and not yet back under the low one).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Gate {
    pub cpu_hot: bool,
}

/// May a new run start, with `running` runs going and the machine at `load`
/// (`None`: not measured)? Pure; returns the gate for the next call.
pub fn admit(load: Option<Load>, limits: &Limits, running: u32, gate: Gate) -> (Result<(), Hold>, Gate) {
    let cpu_hot = match load {
        Some(load) if gate.cpu_hot => load.cpu_percent >= limits.cpu_low,
        Some(load) => load.cpu_percent >= limits.cpu_high,
        None => false,
    };
    let gate = Gate { cpu_hot };
    if limits.max_runs == 0 {
        return (Err(Hold::DisabledAll), gate);
    }
    // One run always goes, however busy the machine is.
    if running == 0 {
        return (Ok(()), gate);
    }
    if running >= limits.max_runs {
        return (Err(Hold::MaxRuns), gate);
    }
    let Some(load) = load else { return (Ok(()), gate) };
    if cpu_hot {
        return (Err(Hold::Cpu), gate);
    }
    if load.available_memory < limits.min_available_memory {
        return (Err(Hold::Memory), gate);
    }
    (Ok(()), gate)
}

/// When new runs may start, kept in `Shared`: the limits (environment, then
/// what the user set), what the dispatcher saw last, and the `load.changed`
/// pushes to the sessions.
pub struct Admission {
    /// Whether CPU and memory are measured at all.
    pub checks_active: bool,
    limits: Mutex<Configured>,
    seen: Mutex<Seen>,
    changes: tokio::sync::watch::Sender<Option<LoadSnapshot>>,
    /// Lets the dispatcher try waiting runs at once after a change.
    pub wake: tokio::sync::Notify,
    /// One `load/set_limits` at a time (read, store, apply).
    setting: tokio::sync::Mutex<()>,
}

struct Configured {
    /// From the environment and the defaults.
    base: Limits,
    /// What the user set in the app (stored).
    set: LoadLimitsUpdate,
}

#[derive(Default)]
struct Seen {
    hold: Option<Hold>,
    running: u32,
    held: u32,
    pushed_at: Option<Instant>,
}

impl Admission {
    pub fn new(base: Limits, set: LoadLimitsUpdate, checks_active: bool) -> Self {
        Self {
            checks_active,
            limits: Mutex::new(Configured { base, set }),
            seen: Mutex::new(Seen::default()),
            changes: tokio::sync::watch::channel(None).0,
            wake: tokio::sync::Notify::new(),
            setting: tokio::sync::Mutex::new(()),
        }
    }

    /// The limits in force now.
    pub fn limits(&self) -> Limits {
        self.limits.lock().map(|c| c.base.with(&c.set)).unwrap_or_default()
    }

    /// `load.changed` pushes (the latest snapshot).
    pub fn changes(&self) -> tokio::sync::watch::Receiver<Option<LoadSnapshot>> {
        self.changes.subscribe()
    }
}

/// The current snapshot.
pub fn snapshot(shared: &Shared) -> LoadSnapshot {
    let (hold, running, held) = shared.admission.seen.lock().map(|s| (s.hold, s.running, s.held)).unwrap_or_default();
    build(shared, hold, running, held)
}

fn build(shared: &Shared, hold: Option<Hold>, running: u32, held: u32) -> LoadSnapshot {
    let sample = shared.load.sample();
    let load = sample.map(|s| s.load);
    let cpu_pct = load.map_or(0.0, |l| l.cpu_percent);
    let mem_free_mb = load.map_or(0, |l| l.available_memory / MB);
    LoadSnapshot {
        cpu_pct,
        mem_free_mb,
        mem_total_mb: sample.map_or(0, |s| s.total_memory / MB),
        running,
        held,
        gate: if hold.is_some() { LoadGate::Held } else { LoadGate::Open },
        hold: hold.map(|h| LoadHold {
            kind: h.kind(),
            cpu_pct: (h == Hold::Cpu).then_some(cpu_pct),
            mem_free_mb: (h == Hold::Memory).then_some(mem_free_mb),
            running: (h == Hold::MaxRuns).then_some(running),
        }),
        limits: shared.admission.limits().view(),
        checks_active: shared.admission.checks_active,
        sampled_at_ms: sample.map_or_else(aurelius_storage::now_ms, |s| s.at_ms),
    }
}

/// What the dispatcher decided this round. Pushes `load.changed` when the
/// gate, the hold or the number of waiting runs changed, and while runs wait
/// at most every 2 s (the live CPU in the app); nothing while nothing waits.
pub fn observe(shared: &Shared, verdict: Result<(), Hold>, running: u32, held: u32) {
    let hold = verdict.err();
    let push = {
        let Ok(mut seen) = shared.admission.seen.lock() else { return };
        let changed = seen.hold != hold || seen.held != held;
        let due = held > 0 && seen.pushed_at.is_none_or(|t| t.elapsed() >= PUSH_EVERY);
        seen.hold = hold;
        seen.running = running;
        seen.held = held;
        if changed || due {
            seen.pushed_at = Some(Instant::now());
        }
        changed || due
    };
    if push {
        shared.admission.changes.send_replace(Some(build(shared, hold, running, held)));
    }
}

/// `load/set_limits`: check, store, apply at once and let waiting runs try again.
pub async fn set_limits(shared: &Arc<Shared>, update: LoadLimitsUpdate) -> Result<LoadSnapshot, ProtocolError> {
    let admission = &shared.admission;
    let _one = admission.setting.lock().await;
    let (base, mut set) = admission
        .limits
        .lock()
        .map(|c| (c.base, c.set.clone()))
        .map_err(|_| ProtocolError::new(ErrorCode::Internal, "load limits unavailable"))?;
    set.max_runs = update.max_runs.or(set.max_runs);
    set.cpu_high = update.cpu_high.or(set.cpu_high);
    set.cpu_low = update.cpu_low.or(set.cpu_low);
    set.mem_min_mb = update.mem_min_mb.or(set.mem_min_mb);
    set.enabled = update.enabled.or(set.enabled);
    validate(&update, &base.with(&set)).map_err(|e| ProtocolError::new(ErrorCode::Validation, e))?;

    let json = serde_json::to_string(&set).map_err(|e| ProtocolError::new(ErrorCode::Internal, e.to_string()))?;
    let store = shared.store().clone();
    tokio::task::spawn_blocking(move || store.write(move |tx| aurelius_storage::settings::put(tx, SETTINGS_KEY, &json, aurelius_storage::now_ms())))
        .await
        .map_err(|e| ProtocolError::new(ErrorCode::Internal, e.to_string()))?
        .map_err(|e| ProtocolError::new(ErrorCode::Internal, e.to_string()))?;
    if let Ok(mut configured) = admission.limits.lock() {
        configured.set = set;
    }
    tracing::info!(limits = ?admission.limits(), "load limits changed");
    let snapshot = snapshot(shared);
    if let Ok(mut seen) = admission.seen.lock() {
        seen.pushed_at = Some(Instant::now());
    }
    admission.changes.send_replace(Some(snapshot.clone()));
    admission.wake.notify_one();
    Ok(snapshot)
}

/// What the user set earlier, from `settings` (nothing when unreadable).
pub fn stored(store: &aurelius_storage::Store) -> LoadLimitsUpdate {
    match store.read(|c| aurelius_storage::settings::get(c, SETTINGS_KEY)) {
        Ok(Some(json)) => serde_json::from_str(&json).unwrap_or_else(|err| {
            tracing::warn!(error = %err, "stored load limits unreadable; using the environment");
            LoadLimitsUpdate::default()
        }),
        Ok(None) => LoadLimitsUpdate::default(),
        Err(err) => {
            tracing::warn!(error = %err, "could not read stored load limits");
            LoadLimitsUpdate::default()
        }
    }
}
