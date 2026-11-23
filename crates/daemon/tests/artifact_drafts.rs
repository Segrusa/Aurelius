//! A design written live (feature `artifact_drafts`), through the daemon's API:
//! while an employee's `Write` of an HTML file streams, a connection that
//! accepts `artifact_drafts` gets the content so far in its live updates; a
//! connection without the flag gets none of those updates, and nothing of the
//! draft is stored.

mod common;

use aurelius_client::envelope;
use aurelius_domain::{Command, conversation::SendChatMessage};
use aurelius_host::ipc::{self, BoxStream, read_frame, write_frame};
use aurelius_protocol::{
    ArtifactDraft, CommandResult, HelloParams, Method, Request, ServerMessage, client_feature,
    feature,
};
use aurelius_runtime::{
    Activity, AgentRuntime, RunHandle, RunRequest, RuntimeCapabilities, RuntimeError, RuntimeEvent,
    fake,
};
use common::*;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

const PATH: &str = "/p/landing.html";

fn page() -> String {
    format!("<html><body>{}</body></html>", "<p>Hello</p>".repeat(200))
}

/// Writes a design in three growing drafts, then the whole call, then answers.
struct Designer;

#[async_trait::async_trait]
impl AgentRuntime for Designer {
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
            let page = page();
            for cut in [100, 101, 900] {
                let content = page[..cut].to_owned();
                let _ = tx
                    .send(RuntimeEvent::Activity(Activity::Drafting {
                        call_id: "w1".into(),
                        path: PATH.into(),
                        content,
                    }))
                    .await;
                // The first two come within the throttle: only the first goes out.
                if cut != 100 {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
            }
            let input = serde_json::json!({ "file_path": PATH, "content": page });
            let _ = tx
                .send(RuntimeEvent::Activity(Activity::Started {
                    call_id: "w1".into(),
                    tool: "Write".into(),
                    input,
                    parent: None,
                }))
                .await;
            // A draft that arrives after the call is whole is late: it never goes out.
            let _ = tx
                .send(RuntimeEvent::Activity(Activity::Drafting {
                    call_id: "w1".into(),
                    path: PATH.into(),
                    content: "late".into(),
                }))
                .await;
            let _ = tx
                .send(RuntimeEvent::Activity(Activity::Finished {
                    call_id: "w1".into(),
                    ok: true,
                    output: "written".into(),
                }))
                .await;
            let _ = tx.send(RuntimeEvent::TextDelta(answer)).await;
            let _ = tx.send(RuntimeEvent::Completed).await;
        });
        Ok(RunHandle::new(request.run_id, rx, input_tx, cancel_tx))
    }
}

/// A raw connection whose hello accepts `accepts`, subscribed to live updates.
async fn subscribed(root: &std::path::Path, accepts: &[&str]) -> BoxStream {
    let mut stream = ipc::connect(&aurelius_host::endpoint(root)).await.unwrap();
    let token = sign_in(root, &connect_anonymous(root).await, BOARD_MEMBER)
        .await
        .session_token;
    let accepts = accepts.iter().map(|a| (*a).to_owned()).collect();
    let hello = Request::read(
        1,
        Method::SessionHello(HelloParams {
            client_name: "raw".into(),
            client_version: "0".into(),
            accepts,
            session_token: Some(token),
        }),
    );
    write_frame(&mut stream, &hello).await.unwrap();
    write_frame(
        &mut stream,
        &Request::read(2, Method::EventSubscribe { after_seq: 0 }),
    )
    .await
    .unwrap();
    loop {
        if let ServerMessage::Response { request_id: 2, .. } =
            read_frame::<ServerMessage, _>(&mut stream)
                .await
                .unwrap()
                .unwrap()
        {
            return stream;
        }
    }
}

/// The live updates of a connection until it has been quiet for a while: how
/// many there were and the drafts among them.
async fn drafts(stream: &mut BoxStream) -> (usize, Vec<ArtifactDraft>) {
    let (mut updates, mut drafts) = (0, Vec::new());
    while let Ok(Ok(Some(message))) = tokio::time::timeout(
        Duration::from_millis(2_500),
        read_frame::<ServerMessage, _>(stream),
    )
    .await
    {
        if let ServerMessage::Stream(delta) = message {
            updates += 1;
            drafts.extend(delta.draft);
        }
    }
    (updates, drafts)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_design_streams_to_a_client_that_shows_it_and_to_no_other() {
    let dir = tempfile::tempdir().unwrap();
    let _daemon = start_with(dir.path(), Arc::new(Designer)).await;
    let client = connect(dir.path()).await;
    assert!(client.supports(feature::ARTIFACT_DRAFTS));
    let mut newer = subscribed(dir.path(), &[client_feature::ARTIFACT_DRAFTS]).await;
    let mut older = subscribed(dir.path(), &[]).await;

    let send = SendChatMessage {
        confirm_secret: false,
        conversation_id: common::conversation(&client).await,
        text: "Design the landing page".into(),
        images: Vec::new(),
        files: vec![],
        interrupt: false,
    };
    let conversation_id = match client
        .execute(envelope(Command::SendChatMessage(send), None))
        .await
        .unwrap()
        .result
    {
        CommandResult::ChatMessageSent {
            conversation_id, ..
        } => conversation_id,
        other => panic!("{other:?}"),
    };
    let ((_, seen), (older_updates, older_drafts)) =
        tokio::join!(drafts(&mut newer), drafts(&mut older));

    let page = page();
    assert_eq!(
        seen.iter().map(|d| d.content.len()).collect::<Vec<_>>(),
        vec![100, 900],
        "throttled, and never after the call is whole"
    );
    assert!(
        seen.iter()
            .all(|d| d.call_id == "w1" && d.path == PATH && page.starts_with(&d.content))
    );
    assert!(
        older_drafts.is_empty() && older_updates > 0,
        "an older client gets its updates, without drafts"
    );

    // Nothing of the draft is kept: the answer holds the whole call only.
    let messages = client.messages(conversation_id).await.unwrap().messages;
    let answer = messages
        .iter()
        .find(|m| !m.activity.is_empty())
        .expect("the answer with its call");
    assert_eq!(
        answer
            .activity
            .iter()
            .map(|a| a.call_id.as_str())
            .collect::<Vec<_>>(),
        vec!["w1"]
    );
    assert_eq!(answer.activity[0].tool, "Write");
}
