//! Tokens per answer, per conversation, and trace mode (what a run sent, kept for the latest runs).

mod common;

use aurelius_client::{Client, envelope};
use aurelius_domain::{
    Command, ConversationId,
    conversation::{MessageRole, MessageStatus, SendChatMessage},
};
use aurelius_protocol::{CommandResult, MessageView};
use common::*;
use std::time::Duration;

async fn send(client: &Client, conversation_id: Option<ConversationId>, text: &str) -> ConversationId {
    match client.execute(envelope(Command::SendChatMessage(SendChatMessage { confirm_secret: false, conversation_id: match conversation_id { Some(id) => id, None => common::conversation(client).await }, text: text.into(), images: Vec::new(), interrupt: false }), None)).await.unwrap().result {
        CommandResult::ChatMessageSent { conversation_id, .. } => conversation_id,
        other => panic!("{other:?}"),
    }
}

async fn answer(client: &Client, id: ConversationId, n: usize) -> MessageView {
    for _ in 0..400 {
        let messages = client.messages(id).await.unwrap().messages;
        let done: Vec<&MessageView> = messages.iter().filter(|m| m.role == MessageRole::Assistant && m.status == MessageStatus::Complete).collect();
        if done.len() >= n {
            return done[n - 1].clone();
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("no answer {n}");
}

#[tokio::test(flavor = "multi_thread")]
async fn every_answer_has_its_tokens_and_trace_mode_keeps_what_was_sent() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    assert!(aurelius_protocol::FEATURES.contains(&"run_usage") && aurelius_protocol::FEATURES.contains(&"run_trace"));

    // Off by default: answers have their tokens, but no trace is kept.
    assert!(!client.trace_setting().await.unwrap().enabled);
    let conversation = send(&client, None, "Eerste vraag").await;
    let first = answer(&client, conversation, 1).await;
    let usage = first.usage.clone().expect("the answer's tokens");
    assert_eq!(usage.runs, 1);
    assert!(usage.input_tokens > 0 && usage.output_tokens > 0, "{usage:?}");
    assert!(client.run_trace(first.run_id.unwrap()).await.is_err(), "no trace while trace mode is off");
    let user = client.messages(conversation).await.unwrap().messages.into_iter().find(|m| m.role == MessageRole::User).unwrap();
    assert!(user.usage.is_none(), "only answers carry tokens");

    // On: the next run keeps what it sent and what it cost.
    let setting = client.set_trace_enabled(true).await.unwrap();
    assert!(setting.enabled && setting.keep_runs > 0 && setting.keep_days > 0);
    send(&client, Some(conversation), "Tweede vraag over het plan").await;
    let second = answer(&client, conversation, 2).await;
    let trace = client.run_trace(second.run_id.unwrap()).await.unwrap();
    assert_eq!(trace.conversation_id, Some(conversation));
    assert!(trace.user_turn.contains("Tweede vraag over het plan"), "{}", trace.user_turn);
    assert!(trace.parts.iter().any(|p| p.area == "prompt" && p.label == "New message" && p.tokens > 0), "{:?}", trace.parts);
    assert!(trace.parts.iter().any(|p| p.label.starts_with("Earlier conversation")), "the first question and answer went in again: {:?}", trace.parts);
    assert_eq!(trace.usage.as_ref().map(|u| u.output_tokens), second.usage.as_ref().map(|u| u.output_tokens));

    // Totals: both runs of the conversation.
    let totals = client.usage_totals(Some(conversation), None).await.unwrap();
    assert_eq!(totals.own.runs, 2);
    assert_eq!(totals.total, totals.own, "no tasks were given away from it");
    assert_eq!(totals.own.output_tokens, usage.output_tokens + second.usage.unwrap().output_tokens);
    assert!(client.usage_totals(None, None).await.is_err(), "a conversation or a task is needed");

    // Off again: the kept traces go.
    assert!(!client.set_trace_enabled(false).await.unwrap().enabled);
    assert!(client.run_trace(second.run_id.unwrap()).await.is_err());
    stop(dir.path(), daemon).await;
}
