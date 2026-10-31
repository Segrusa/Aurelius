//! Shutting down the server's PC : the countdown, cancel and
//! waiting for work. `AURELIUS_POWER_COMMAND` points to a command that only
//! writes a file in the data folder, so no test ever powers anything off.

mod common;
use common::*;

use aurelius_client::{Client, envelope};
use aurelius_domain::{Command, conversation::SendChatMessage};
use aurelius_protocol::{PowerState, ShutdownWhen};
use aurelius_runtime::fake::FakeRuntime;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

const MARKER: &str = "power-fired.txt";

async fn start_power(root: &Path, delay_ms: u64) -> Daemon {
    // SAFETY (test): every test in this binary sets the same values.
    unsafe {
        // Never the real power-off: the command only writes a file (the shell runs in the data folder).
        std::env::set_var("AURELIUS_POWER_COMMAND", format!("echo fired > {MARKER}"));
        std::env::set_var("AURELIUS_POWER_GRACE_SECS", "1.5");
    }
    start_with(root, Arc::new(FakeRuntime { delay: Duration::from_millis(delay_ms), ..Default::default() })).await
}

/// Poll `power.get` until `wanted`, within `secs`.
async fn until(client: &Client, secs: u64, wanted: impl Fn(&PowerState) -> bool) -> PowerState {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(secs);
    loop {
        let state = client.query_power().await.unwrap();
        if wanted(&state) {
            return state;
        }
        assert!(tokio::time::Instant::now() < deadline, "state in time, last {state:?}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn marker_appears(root: &Path, secs: u64) -> bool {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(secs);
    while tokio::time::Instant::now() < deadline {
        if root.join(MARKER).exists() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    false
}

#[tokio::test(flavor = "multi_thread")]
async fn now_counts_down_and_then_runs_the_command() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start_power(dir.path(), 2).await;
    let client = connect(dir.path()).await;
    client.subscribe(0).await.unwrap();
    let mut pushes = client.power_changes();
    assert_eq!(client.query_power().await.unwrap(), PowerState { pending: None });

    let state = client.power_shutdown(ShutdownWhen::Now).await.unwrap();
    let pending = state.pending.clone().expect("pending");
    assert_eq!(pending.when, ShutdownWhen::Now);
    let deadline = pending.deadline_ms.expect("countdown runs");
    assert!(deadline > pending.requested_at_ms);
    assert_eq!(client.query_power().await.unwrap(), state);
    assert_eq!(tokio::time::timeout(Duration::from_secs(5), pushes.recv()).await.unwrap().unwrap(), state);
    assert!(!dir.path().join(MARKER).exists(), "not before the deadline");

    assert!(marker_appears(dir.path(), 10).await, "the command ran");
    assert_eq!(client.query_power().await.unwrap(), PowerState { pending: None });
    assert_eq!(tokio::time::timeout(Duration::from_secs(5), pushes.recv()).await.unwrap().unwrap(), PowerState { pending: None });
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn cancel_stops_the_countdown() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start_power(dir.path(), 2).await;
    let client = connect(dir.path()).await;
    client.power_shutdown(ShutdownWhen::Now).await.unwrap();
    assert_eq!(client.power_cancel().await.unwrap(), PowerState { pending: None });
    assert!(!marker_appears(dir.path(), 3).await, "cancelled: nothing runs");
    assert_eq!(client.query_power().await.unwrap(), PowerState { pending: None });
    // A new request replaces the pending one.
    let first = client.power_shutdown(ShutdownWhen::WhenIdle).await.unwrap();
    let second = client.power_shutdown(ShutdownWhen::Now).await.unwrap();
    assert_eq!(first.pending.unwrap().when, ShutdownWhen::WhenIdle);
    assert_eq!(second.pending.unwrap().when, ShutdownWhen::Now);
    client.power_cancel().await.unwrap();
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn when_idle_waits_for_a_running_run_and_then_counts_down() {
    let dir = tempfile::tempdir().unwrap();
    // Slow answer: the fake runtime streams with this delay per step.
    let daemon = start_power(dir.path(), 400).await;
    let client = connect(dir.path()).await;
    let send = Command::SendChatMessage(SendChatMessage { confirm_secret: false, conversation_id: common::conversation(&client).await, text: "hello".into(), images: Vec::new(), interrupt: false });
    client.execute(envelope(send, None)).await.unwrap();

    let state = client.power_shutdown(ShutdownWhen::WhenIdle).await.unwrap();
    assert_eq!(state.pending.as_ref().unwrap().deadline_ms, None, "waits while the run is going");
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert_eq!(client.query_power().await.unwrap().pending.unwrap().deadline_ms, None);
    assert!(!dir.path().join(MARKER).exists());

    let counting = until(&client, 60, |s| s.pending.as_ref().is_some_and(|p| p.deadline_ms.is_some())).await;
    assert_eq!(counting.pending.unwrap().when, ShutdownWhen::WhenIdle);
    assert!(marker_appears(dir.path(), 15).await, "the command ran once idle");
    assert_eq!(client.query_power().await.unwrap(), PowerState { pending: None });
    stop(dir.path(), daemon).await;
}
