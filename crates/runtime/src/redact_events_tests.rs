//! SEC0 E14 (a): a run's events leave the adapter redacted, whatever adapter.

use super::*;
use crate::redact::{self, Redactor, Secret};
use zeroize::Zeroizing;

const FAKE: &str = "sk-test-FAKE-0000-events";

#[tokio::test]
async fn every_event_of_a_run_leaves_its_adapter_redacted() {
    let _global = redact::TEST_GLOBAL.lock().unwrap_or_else(|e| e.into_inner());
    redact::install(Redactor::new(&[Secret { name: "CANARY_TOKEN".into(), label: String::new(), value: Zeroizing::new(FAKE.into()) }]));
    let (tx, rx) = mpsc::channel(EVENT_BUFFER);
    let (input_tx, _input_rx) = mpsc::channel(1);
    let (cancel_tx, _) = watch::channel(false);
    let mut handle = RunHandle::new(RunId::new(), rx, input_tx, cancel_tx);
    let (head, tail) = FAKE.split_at(9);
    let sent = SentRequest { args: vec![format!("--x={FAKE}")], prompt: format!("use {FAKE}"), ..Default::default() };
    for event in [
        RuntimeEvent::Sent(Box::new(sent)),
        RuntimeEvent::TextDelta(format!("the value is {head}")),
        RuntimeEvent::TextDelta(format!("{tail}, done")),
        RuntimeEvent::Activity(Activity::Started { call_id: "1".into(), tool: "Bash".into(), input: serde_json::json!({ "command": format!("echo {FAKE}") }), parent: None }),
        RuntimeEvent::Activity(Activity::Finished { call_id: "1".into(), ok: true, output: FAKE.into() }),
        RuntimeEvent::Activity(Activity::Drafting { call_id: "2".into(), path: format!("/p/{FAKE}.html"), content: format!("<p>{FAKE}</p>") }),
        RuntimeEvent::TextDelta(format!("last {FAKE}")),
        RuntimeEvent::Failed { code: "stream_incomplete".into(), message: format!("stderr: {FAKE}") },
    ] {
        tx.send(event).await.unwrap();
    }
    drop(tx);
    let mut seen = Vec::new();
    while let Some(event) = handle.events.recv().await {
        seen.push(event);
    }
    redact::install(Redactor::default());
    let printed = format!("{seen:?}");
    assert!(!printed.contains("FAKE-0000"), "{printed}");
    let text: String = seen.iter().filter_map(|e| if let RuntimeEvent::TextDelta(t) = e { Some(t.as_str()) } else { None }).collect();
    assert_eq!(text, "the value is «secret:CANARY_TOKEN», donelast «secret:CANARY_TOKEN»");
    assert!(matches!(seen.last(), Some(RuntimeEvent::Failed { .. })));
}
