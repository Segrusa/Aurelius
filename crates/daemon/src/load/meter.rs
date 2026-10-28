//! The machine's load, sampled in the background.

use super::Load;
use crate::Shared;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const SAMPLE_EVERY: Duration = Duration::from_secs(2);
/// Samples averaged: 5 × 2 s ≈ 10 s.
const WINDOW: usize = 5;

/// The latest averaged load, kept in `Shared`.
#[derive(Default)]
pub struct Meter {
    current: Mutex<Option<Sample>>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Sample {
    pub(super) load: Load,
    pub(super) total_memory: u64,
    pub(super) at_ms: i64,
}

impl Meter {
    pub fn current(&self) -> Option<Load> {
        self.sample().map(|s| s.load)
    }

    pub(super) fn sample(&self) -> Option<Sample> {
        self.current.lock().ok().and_then(|l| *l)
    }

    pub(super) fn set(&self, load: Load, total_memory: u64) {
        if let Ok(mut current) = self.current.lock() {
            *current = Some(Sample { load, total_memory, at_ms: aurelius_storage::now_ms() });
        }
    }
}

pub(super) fn average(samples: &VecDeque<Load>) -> Option<Load> {
    let n = samples.len();
    if n == 0 {
        return None;
    }
    Some(Load {
        cpu_percent: samples.iter().map(|s| s.cpu_percent).sum::<f32>() / n as f32,
        available_memory: samples.iter().map(|s| s.available_memory).sum::<u64>() / n as u64,
    })
}

/// Sample the machine until the daemon stops.
pub fn start(shared: Arc<Shared>) {
    tokio::spawn(async move {
        let mut shutdown = shared.shutdown.subscribe();
        let mut system = sysinfo::System::new();
        // The first CPU reading is only a baseline.
        system.refresh_cpu_usage();
        let mut samples = VecDeque::with_capacity(WINDOW + 1);
        loop {
            tokio::select! {
                _ = tokio::time::sleep(SAMPLE_EVERY) => {}
                _ = shutdown.changed() => return,
            }
            system.refresh_cpu_usage();
            system.refresh_memory();
            samples.push_back(Load { cpu_percent: system.global_cpu_usage(), available_memory: system.available_memory() });
            if samples.len() > WINDOW {
                samples.pop_front();
            }
            if let Some(load) = average(&samples) {
                shared.load.set(load, system.total_memory());
            }
        }
    });
}
