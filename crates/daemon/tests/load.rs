//! Load limits from the app: kept over restarts, checked, applied at once,
//! and `load_changed` pushed while runs wait.

mod common;

use aurelius_client::{ClientError, envelope};
use aurelius_domain::{
    Command, ConversationId,
    conversation::{MessageRole, MessageStatus, SendChatMessage},
};
use aurelius_protocol::{CommandResult, ErrorCode, LoadGate, LoadHoldKind, LoadLimitsUpdate, LoadSnapshot};
use aurelius_runtime::fake::FakeRuntime;
use common::*;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

async fn start(root: &Path) -> Daemon {
    start_with(root, Arc::new(FakeRuntime { delay: Duration::from_millis(2), ..Default::default() })).await
}

async fn chat(client: &aurelius_client::Client, text: &str) -> ConversationId {
    let env = envelope(Command::SendChatMessage(SendChatMessage { confirm_secret: false, conversation_id: common::conversation(&client).await, text: text.into(), images: Vec::new(), files: vec![], interrupt: false }), None);
    match client.execute(env).await.unwrap().result {
        CommandResult::ChatMessageSent { conversation_id, .. } => conversation_id,
        other => panic!("{other:?}"),
    }
}

async fn answer(client: &aurelius_client::Client, id: ConversationId) -> Option<MessageStatus> {
    let messages = client.messages(id).await.unwrap().messages;
    messages.iter().find(|m| m.role == MessageRole::Assistant && m.status != MessageStatus::Partial).map(|m| m.status)
}

/// The next push that satisfies `wanted`, within `secs`.
async fn push(loads: &mut mpsc::UnboundedReceiver<LoadSnapshot>, secs: u64, wanted: impl Fn(&LoadSnapshot) -> bool) -> LoadSnapshot {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(secs);
    loop {
        let snapshot = tokio::time::timeout_at(deadline, loads.recv()).await.expect("load.changed in time").expect("pushes open");
        if wanted(&snapshot) {
            return snapshot;
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn limits_set_in_the_app_are_kept_over_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let daemon = start(&root).await;
    let client = connect(&root).await;
    let before = client.load_snapshot().await.unwrap();
    assert_eq!((before.limits.max_runs, before.limits.cpu_high, before.limits.cpu_low), (3, 75, 65));
    assert!(!before.checks_active, "a test data folder measures nothing");
    assert_eq!((before.gate, before.hold.clone(), before.held), (LoadGate::Open, None, 0));

    let after = client.set_load_limits(LoadLimitsUpdate { max_runs: Some(5), cpu_high: Some(90), mem_min_mb: Some(512), ..Default::default() }).await.unwrap();
    assert_eq!((after.limits.max_runs, after.limits.cpu_high, after.limits.cpu_low, after.limits.mem_min_mb), (5, 90, 65, 512));
    // Only what is given changes.
    let after = client.set_load_limits(LoadLimitsUpdate { enabled: Some(false), ..Default::default() }).await.unwrap();
    assert_eq!((after.limits.max_runs, after.limits.cpu_high, after.limits.enabled), (5, 90, false));
    stop(&root, daemon).await;

    let daemon = start(&root).await;
    let client = connect(&root).await;
    let limits = client.load_snapshot().await.unwrap().limits;
    assert_eq!((limits.max_runs, limits.cpu_high, limits.cpu_low, limits.mem_min_mb, limits.enabled), (5, 90, 65, 512, false));
    stop(&root, daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn nonsense_limits_are_refused_and_change_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let daemon = start(&root).await;
    let client = connect(&root).await;
    for update in [
        LoadLimitsUpdate { cpu_low: Some(80), ..Default::default() },
        LoadLimitsUpdate { cpu_high: Some(101), cpu_low: Some(50), ..Default::default() },
        LoadLimitsUpdate { cpu_high: Some(60), cpu_low: Some(60), ..Default::default() },
    ] {
        match client.set_load_limits(update.clone()).await {
            Err(ClientError::Protocol(err)) => assert_eq!(err.code, ErrorCode::Validation, "{update:?}"),
            other => panic!("{update:?} gave {other:?}"),
        }
    }
    let limits = client.load_snapshot().await.unwrap().limits;
    assert_eq!((limits.cpu_high, limits.cpu_low), (75, 65));
    // 0 runs is allowed: everything waits.
    assert_eq!(client.set_load_limits(LoadLimitsUpdate { max_runs: Some(0), ..Default::default() }).await.unwrap().limits.max_runs, 0);
    stop(&root, daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn held_runs_are_pushed_live_and_start_once_the_limit_allows() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let daemon = start(&root).await;
    let client = connect(&root).await;
    let mut loads = client.load_changes();
    client.subscribe(0).await.unwrap();

    let set = client.set_load_limits(LoadLimitsUpdate { max_runs: Some(0), ..Default::default() }).await.unwrap();
    assert_eq!(set.limits.max_runs, 0);
    push(&mut loads, 5, |s| s.limits.max_runs == 0).await;

    let conversation = chat(&client, "hello").await;
    let held = push(&mut loads, 10, |s| s.held == 1).await;
    assert_eq!(held.gate, LoadGate::Held);
    assert_eq!(held.hold.as_ref().map(|h| h.kind), Some(LoadHoldKind::DisabledAll));
    assert_eq!(client.load_snapshot().await.unwrap().held, 1);
    assert_eq!(answer(&client, conversation).await, None, "nothing starts while the limit is 0");

    // While a run waits: a push at most every 2 s, and they keep coming.
    let mut at = vec![Instant::now()];
    let until = Instant::now() + Duration::from_millis(5_000);
    while let Ok(Some(snapshot)) = tokio::time::timeout_at(until.into(), loads.recv()).await {
        assert_eq!(snapshot.held, 1);
        at.push(Instant::now());
    }
    assert!((2..=4).contains(&at.len()), "pushes while held: {}", at.len() - 1);
    for pair in at.windows(2) {
        assert!(pair[1] - pair[0] >= Duration::from_millis(1_800), "pushed again after {:?}", pair[1] - pair[0]);
    }

    // Applied at once: the waiting answer starts without a restart.
    client.set_load_limits(LoadLimitsUpdate { max_runs: Some(3), ..Default::default() }).await.unwrap();
    let open = push(&mut loads, 10, |s| s.held == 0 && s.gate == LoadGate::Open).await;
    assert_eq!(open.limits.max_runs, 3);
    let mut status = None;
    for _ in 0..200 {
        status = answer(&client, conversation).await;
        if status.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert_eq!(status, Some(MessageStatus::Complete));

    // Nothing waits: no more pushes.
    tokio::time::sleep(Duration::from_millis(300)).await;
    while loads.try_recv().is_ok() {}
    assert!(tokio::time::timeout(Duration::from_millis(2_500), loads.recv()).await.is_err(), "no pushes while nothing waits");
    stop(&root, daemon).await;
}
