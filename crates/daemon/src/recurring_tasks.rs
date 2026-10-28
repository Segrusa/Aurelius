//! Recurring tasks tick: a local check every 30 seconds, and once right at
//! start so a recurring task that came due during downtime makes its one
//! catch-up task. A task made here goes through the normal job queue.

use crate::Shared;
use std::sync::Arc;
use std::time::Duration;

pub fn start(shared: Arc<Shared>) {
    let tick = std::env::var("AURELIUS_SCHEDULE_TICK_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(30_000);
    tokio::spawn(async move {
        let mut shutdown = shared.shutdown.subscribe();
        loop {
            if !shared.paused.load(std::sync::atomic::Ordering::Relaxed) {
                let s = shared.clone();
                let now = aurelius_storage::now_ms();
                match tokio::task::spawn_blocking(move || s.service.fire_due_task_schedules(now, aurelius_host::utc_offset_minutes_at)).await {
                    Ok(Ok(notices)) => shared.publish(notices),
                    Ok(Err(err)) => tracing::warn!(error = %err.message, "recurring tasks tick failed"),
                    Err(err) => tracing::warn!(error = %err, "recurring tasks tick panicked"),
                }
            }
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(tick)) => {}
                _ = shutdown.changed() => return,
            }
        }
    });
}
