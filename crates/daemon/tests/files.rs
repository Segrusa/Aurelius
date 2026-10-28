//! B15 acceptance: attachments.

mod common;

use aurelius_client::{Client, envelope};
use aurelius_domain::{
    Command, ConversationId,
    conversation::{MessageRole, MessageStatus, SendChatMessage},
    files::AttachFile,
};
use aurelius_protocol::{CommandResult, MessageView};
use common::*;
use std::time::Duration;

async fn send(client: &Client, conversation_id: Option<ConversationId>, text: &str) -> ConversationId {
    match client.execute(envelope(Command::SendChatMessage(SendChatMessage { conversation_id: match conversation_id { Some(id) => id, None => common::conversation(client).await }, text: text.into(), images: Vec::new(), interrupt: false }), None)).await.unwrap().result {
        CommandResult::ChatMessageSent { conversation_id, .. } => conversation_id,
        other => panic!("{other:?}"),
    }
}

async fn last_answer(client: &Client, id: ConversationId) -> MessageView {
    for _ in 0..400 {
        let messages = client.messages(id).await.unwrap().messages;
        if let Some(a) = messages.iter().rev().find(|m| m.role == MessageRole::Assistant && m.status != MessageStatus::Partial) {
            return a.clone();
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("no answer");
}

#[tokio::test(flavor = "multi_thread")]
async fn attachments_enter_the_context_as_labelled_data() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let file = dir.path().join("notities.md");
    std::fs::write(&file, "# Plan\nBouw eerst de kern.").unwrap();

    let conversation_id = common::conversation(&client).await;
    let CommandResult::AttachmentAdded { conversation_id, .. } =
        client.execute(envelope(Command::AttachFile(AttachFile { conversation_id, path: file.display().to_string() }), None)).await.unwrap().result
    else {
        panic!()
    };
    let snapshot = client.messages(conversation_id).await.unwrap();
    assert_eq!(snapshot.attachments.len(), 1);
    assert_eq!(snapshot.attachments[0].kind, "markdown");

    send(&client, Some(conversation_id), "Vat de bijlage samen").await;
    let answer = last_answer(&client, conversation_id).await;
    // The fake echoes (part of) its input: the attachment block came first, labelled as data.
    assert!(answer.text.contains("Attachments to this conversation"), "{}", answer.text);

    // Unsupported binary files are refused with a clear reason.
    let binary = dir.path().join("foto.png");
    std::fs::write(&binary, [0x89, b'P', b'N', b'G', 0, 0, 1]).unwrap();
    let err = client.execute(envelope(Command::AttachFile(AttachFile { conversation_id, path: binary.display().to_string() }), None)).await;
    assert!(err.unwrap_err().to_string().contains("not supported"));
    stop(dir.path(), daemon).await;
}
