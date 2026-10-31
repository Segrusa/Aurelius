//! Tool use comes shortened in `conversation.get` (a task with hundreds of
//! calls stays small); `activity.get` returns one call whole (feature
//! `activity_preview`).

mod common;

use aurelius_client::{ClientError, envelope};
use aurelius_domain::{Command, ErrorCode, MessageId, conversation::{MessageStatus, SendChatMessage}};
use aurelius_protocol::{CommandResult, feature};
use aurelius_runtime::{Activity, AgentRuntime, RunHandle, RunRequest, RuntimeCapabilities, RuntimeError, RuntimeEvent, fake};
use common::*;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

/// Writes a big file, then answers.
struct BigTool;

fn content() -> String {
    "line of the spec\n".repeat(3_000)
}

fn output() -> String {
    "x".repeat(6_000)
}

#[async_trait::async_trait]
impl AgentRuntime for BigTool {
    async fn probe(&self) -> RuntimeCapabilities {
        fake::FakeRuntime::default().probe().await
    }
    async fn start(&self, request: RunRequest) -> Result<RunHandle, RuntimeError> {
        let answer = fake::answer_for(&request);
        let (tx, rx) = mpsc::channel(8);
        let (input_tx, _input_rx) = mpsc::channel(1);
        let (cancel_tx, _cancel_rx) = tokio::sync::watch::channel(false);
        tokio::spawn(async move {
            let _ = tx.send(RuntimeEvent::Started).await;
            let input = serde_json::json!({ "file_path": "spec.md", "content": content() });
            let _ = tx.send(RuntimeEvent::Activity(Activity::Started { call_id: "w1".into(), tool: "Write".into(), input, parent: None })).await;
            let _ = tx.send(RuntimeEvent::Activity(Activity::Finished { call_id: "w1".into(), ok: true, output: output() })).await;
            let _ = tx.send(RuntimeEvent::TextDelta(answer)).await;
            let _ = tx.send(RuntimeEvent::Completed).await;
        });
        Ok(RunHandle::new(request.run_id, rx, input_tx, cancel_tx))
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn tool_use_is_short_in_the_conversation_and_whole_on_request() {
    let dir = tempfile::tempdir().unwrap();
    let _daemon = start_with(dir.path(), Arc::new(BigTool)).await;
    let client = connect(dir.path()).await;
    assert!(client.supports(feature::ACTIVITY_PREVIEW));

    let send = SendChatMessage { confirm_secret: false, conversation_id: common::conversation(&client).await, text: "Write the spec".into(), images: Vec::new(), interrupt: false };
    let conversation_id = match client.execute(envelope(Command::SendChatMessage(send), None)).await.unwrap().result {
        CommandResult::ChatMessageSent { conversation_id, .. } => conversation_id,
        other => panic!("{other:?}"),
    };
    let mut answer = None;
    for _ in 0..400 {
        let messages = client.messages(conversation_id).await.unwrap().messages;
        answer = messages.into_iter().find(|m| m.status == MessageStatus::Complete && !m.activity.is_empty());
        if answer.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let answer = answer.expect("the answer completes with its tool call");

    // Short: the output to its preview with the whole size, long input strings cut, short ones kept.
    let [short] = &answer.activity[..] else { panic!("{:?}", answer.activity) };
    assert!(short.truncated);
    assert_eq!(short.output.as_deref().map(str::len), Some(aurelius_domain::activity::OUTPUT_PREVIEW_BYTES));
    assert_eq!(short.output_bytes, Some(output().len() as u32));
    assert_eq!(short.input["file_path"], "spec.md");
    assert_eq!(short.input["content"].as_str().map(str::len), Some(aurelius_domain::activity::INPUT_STRING_PREVIEW_BYTES));

    // Whole on request.
    let whole = client.activity(answer.id, "w1").await.unwrap();
    assert!(!whole.truncated);
    assert_eq!(whole.output, Some(output()));
    assert_eq!(whole.input["content"], content());
    assert_eq!((whole.ok, whole.offset, whole.tool.as_str()), (short.ok, short.offset, "Write"));

    // An unknown call or message is not found.
    for (message, call) in [(answer.id, "nope"), (MessageId::new(), "w1")] {
        match client.activity(message, call).await {
            Err(ClientError::Protocol(err)) => assert_eq!(err.code, ErrorCode::NotFound),
            other => panic!("{other:?}"),
        }
    }
}
