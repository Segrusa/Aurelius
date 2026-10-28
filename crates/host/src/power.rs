//! Shutting down the PC of the server (`power/shutdown`, protocol 1.12). The
//! daemon owns the timer: `now` starts a grace countdown (60 s), `when_idle`
//! first waits until no run or task is going or queued. At the deadline the
//! OS power-off runs. The pending request lives in memory only.
//!
//! `AURELIUS_POWER_COMMAND` replaces the OS command (run by the shell in the
//! data folder) and `AURELIUS_POWER_GRACE_SECS` the 60 s grace; tests use both
//! so they never power anything off.

use crate::Service;
use aurelius_protocol::{PendingShutdown, PowerState, ShutdownWhen};
use aurelius_storage::{now_ms, queries};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const GRACE: Duration = Duration::from_secs(60);
const POLL: Duration = Duration::from_millis(200);

#[derive(Default)]
pub struct Power {
    pending: Mutex<Option<PendingShutdown>>,
    notify: PowerChanges,
}

/// `power.changed` pushes (the latest state).
pub struct PowerChanges(tokio::sync::watch::Sender<PowerState>);

impl Default for PowerChanges {
    fn default() -> Self {
        Self(tokio::sync::watch::channel(PowerState { pending: None }).0)
    }
}

impl Power {
    pub fn state(&self) -> PowerState {
        PowerState { pending: self.pending.lock().map(|p| p.clone()).unwrap_or_default() }
    }

    pub fn changes(&self) -> tokio::sync::watch::Receiver<PowerState> {
        self.notify.0.subscribe()
    }

    /// Store `next` and push it, when `expected` is still what is pending
    /// (`None` = whatever is pending). Answers whether it was stored.
    fn replace(&self, expected: Option<&PendingShutdown>, next: Option<PendingShutdown>) -> bool {
        let Ok(mut pending) = self.pending.lock() else { return false };
        if expected.is_some() && pending.as_ref() != expected {
            return false;
        }
        if *pending != next {
            *pending = next.clone();
            self.notify.0.send_replace(PowerState { pending: next });
        }
        true
    }
}

/// The countdown: `AURELIUS_POWER_GRACE_SECS`, else 60 s.
fn grace() -> Duration {
    std::env::var("AURELIUS_POWER_GRACE_SECS")
        .ok()
        .and_then(|v| v.trim().parse::<f64>().ok())
        .filter(|v| v.is_finite() && *v >= 0.0)
        .map_or(GRACE, Duration::from_secs_f64)
}

fn deadline_after_grace() -> i64 {
    now_ms() + grace().as_millis() as i64
}

/// A new request; it replaces the pending one.
pub fn request(power: &Power, when: ShutdownWhen) -> PowerState {
    let deadline_ms = match when {
        ShutdownWhen::Now => Some(deadline_after_grace()),
        ShutdownWhen::WhenIdle => None,
    };
    tracing::warn!(?when, "shutdown of this PC requested by a client");
    power.replace(None, Some(PendingShutdown { when, requested_at_ms: now_ms(), deadline_ms }));
    power.state()
}

pub fn cancel(power: &Power) -> PowerState {
    if power.state().pending.is_some() {
        tracing::info!("shutdown of this PC cancelled");
    }
    power.replace(None, None);
    power.state()
}

/// The timer; ends when the daemon stops.
pub fn start(shared: Arc<impl Service>) {
    tokio::spawn(async move {
        let mut shutdown = shared.shutdown_signal();
        loop {
            tokio::select! {
                _ = tokio::time::sleep(POLL) => {}
                _ = shutdown.changed() => return,
            }
            let Some(pending) = shared.power().state().pending else { continue };
            tick(&shared, pending).await;
        }
    });
}

async fn tick<S: Service>(shared: &Arc<S>, pending: PendingShutdown) {
    let mut next = pending.clone();
    if pending.when == ShutdownWhen::WhenIdle {
        let store = shared.store().clone();
        let busy = match tokio::task::spawn_blocking(move || store.read(queries::has_work)).await {
            Ok(Ok(busy)) => busy,
            // Unknown: do not count down on a guess.
            _ => true,
        };
        next.deadline_ms = match (busy, pending.deadline_ms) {
            (true, _) => None,
            (false, None) => Some(deadline_after_grace()),
            (false, Some(deadline)) => Some(deadline),
        };
        if next != pending && !shared.power().replace(Some(&pending), Some(next.clone())) {
            return;
        }
    }
    if next.deadline_ms.is_some_and(|deadline| now_ms() >= deadline) && shared.power().replace(Some(&next), None) {
        power_off(shared).await;
    }
}

#[cfg(windows)]
fn os_command() -> std::process::Command {
    let mut command = std::process::Command::new("shutdown");
    command.args(["/s", "/t", "0"]);
    command
}

#[cfg(target_os = "macos")]
fn os_command() -> std::process::Command {
    let mut command = std::process::Command::new("osascript");
    command.args(["-e", "tell app \"System Events\" to shut down"]);
    command
}

#[cfg(not(any(windows, target_os = "macos")))]
fn os_command() -> std::process::Command {
    let mut command = std::process::Command::new("systemctl");
    command.arg("poweroff");
    command
}

/// The override, run by the shell.
fn override_command(text: &str) -> std::process::Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let mut command = std::process::Command::new("cmd");
        command.arg("/C").raw_arg(text);
        command
    }
    #[cfg(not(windows))]
    {
        let mut command = std::process::Command::new("sh");
        command.arg("-c").arg(text);
        command
    }
}

async fn power_off<S: Service>(shared: &Arc<S>) {
    let custom = std::env::var("AURELIUS_POWER_COMMAND").ok().filter(|c| !c.trim().is_empty());
    tracing::warn!(overridden = custom.is_some(), "powering off this PC");
    let mut command = match &custom {
        Some(text) => override_command(text),
        None => os_command(),
    };
    command.current_dir(shared.root());
    match tokio::task::spawn_blocking(move || command.status()).await {
        Ok(Ok(status)) if status.success() => tracing::warn!("power-off command done"),
        Ok(Ok(status)) => tracing::error!(%status, "power-off command failed"),
        Ok(Err(err)) => tracing::error!(error = %err, "power-off command could not start"),
        Err(err) => tracing::error!(error = %err, "power-off command panicked"),
    }
}
