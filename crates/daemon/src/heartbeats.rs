//! Heartbeat tick (B21): a local, inference-free check every 30 seconds.
//! Waking an employee goes through the normal job queue, so pause, limits and
//! the dispatcher's rules apply unchanged.

use crate::Shared;
use std::sync::Arc;
use std::time::Duration;

pub fn start(shared: Arc<Shared>) {
    let tick = std::env::var("AURELIUS_HEARTBEAT_TICK_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(30_000);
    tokio::spawn(async move {
        let mut shutdown = shared.shutdown.subscribe();
        loop {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(tick)) => {}
                _ = shutdown.changed() => return,
            }
            if shared.paused.load(std::sync::atomic::Ordering::Relaxed) {
                continue;
            }
            let s = shared.clone();
            let now = aurelius_storage::now_ms();
            match tokio::task::spawn_blocking(move || s.service.evaluate_heartbeats(now, aurelius_host::utc_offset_minutes_at)).await {
                Ok(Ok(notices)) => shared.publish(notices),
                Ok(Err(err)) => tracing::warn!(error = %err.message, "heartbeat tick failed"),
                Err(err) => tracing::warn!(error = %err, "heartbeat tick panicked"),
            }
        }
    });
}
