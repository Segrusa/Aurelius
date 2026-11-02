//! Editing and deleting a message of a channel or a ticket's discussion
//! (AUR-FA-F3, A6): the text changes, the earlier blob is the event's payload,
//! and nothing wakes, follows or notifies.

use super::*;
use aurelius_domain::MessageId;
use aurelius_domain::group_chat::{DeleteMessage, EditMessage, MESSAGE_DELETED_TEXT};

fn edit(s: &Setup, message: MessageId, text: &str) -> Result<CommandResult, AppError> {
    exec(s, Command::EditMessage(EditMessage { message_id: message, text: text.into(), confirm_secret: false }))
}

fn delete(s: &Setup, message: MessageId) -> Result<CommandResult, AppError> {
    exec(s, Command::DeleteMessage(DeleteMessage { message_id: message }))
}

fn count(s: &Setup, sql: &'static str) -> i64 {
    s.service.store().read(move |c| Ok(c.query_row(sql, [], |r| r.get::<_, i64>(0))?)).unwrap()
}

/// Everything an edit or a delete must leave as it is: runs, jobs, notification rows, follow rows.
fn untouched(s: &Setup) -> [i64; 5] {
    [
        count(s, "SELECT COUNT(*) FROM runs"),
        count(s, "SELECT COUNT(*) FROM jobs"),
        count(s, "SELECT COUNT(*) FROM notifications"),
        count(s, "SELECT COUNT(*) FROM ticket_followers"),
        count(s, "SELECT COUNT(*) FROM messages"),
    ]
}

/// The newest event of `kind`: (payload text, metadata).
fn last_event(s: &Setup, kind: &'static str) -> (Option<String>, serde_json::Value) {
    let st = s.service.store().clone();
    s.service
        .store()
        .read(move |c| {
            let (key, meta): (Option<String>, String) = c.query_row(
                "SELECT b.storage_key, e.metadata_json FROM events e LEFT JOIN blobs b ON b.id = e.payload_blob_id WHERE e.type = ?1 ORDER BY e.seq DESC LIMIT 1",
                [kind],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            Ok((key.and_then(|k| st.blobs().read(&k).ok()).and_then(|b| String::from_utf8(b).ok()), serde_json::from_str(&meta).unwrap()))
        })
        .unwrap()
}

fn view(s: &Setup, channel: ConversationId, message: MessageId) -> aurelius_domain::views::MessageView {
    messages(s, channel).into_iter().find(|m| m.id == message).unwrap()
}

fn other_member(s: &Setup) -> UserId {
    let other = UserId::new();
    s.service.store().write(move |tx| aurelius_storage::users::insert(tx, &aurelius_storage::users::NewUser { id: &other.to_string(), username: "dana", created_by_user: None, now_ms: 2 }, PHC)).unwrap();
    other
}

#[test]
fn an_edit_replaces_the_text_and_keeps_the_earlier_one_as_the_events_payload() {
    let s = setup();
    let orbit = channel(&s, s.project);
    let message = sent_message(&s, Some(orbit), "First version");
    let result = edit(&s, message, "Second version").unwrap();
    assert!(matches!(result, CommandResult::MessageEdited { conversation_id, message_id, .. } if conversation_id == orbit && message_id == message));
    let read = view(&s, orbit, message);
    assert_eq!((read.text.as_str(), read.edited_at_ms.is_some(), read.deleted_at_ms), ("Second version", true, None));
    let (payload, meta) = last_event(&s, "message.edited");
    assert_eq!(payload.as_deref(), Some("First version"));
    assert_eq!(meta, serde_json::json!({ "message_id": message, "by": s.user }), "no text and nothing else in the metadata");
}

#[test]
fn an_edit_with_a_new_mention_wakes_nobody() {
    let s = setup();
    let orbit = channel(&s, s.project);
    let t = new_ticket(&s, "Login", Some(s.ann));
    for (message, in_ticket) in [(sent_message(&s, Some(orbit), "Nothing to see"), false), (post_message(&s, t, "Nothing either"), true)] {
        let before = untouched(&s);
        let woken_before = (woken(&s, s.ann).len(), woken(&s, s.bo).len(), woken(&s, s.ceo).len());
        let dana_before = followers(&s, t);
        edit(&s, message, "@Ann and @Bo and @dana, please look").unwrap();
        assert_eq!(untouched(&s), before, "ticket: {in_ticket}");
        assert_eq!((woken(&s, s.ann).len(), woken(&s, s.bo).len(), woken(&s, s.ceo).len()), woken_before, "ticket: {in_ticket}");
        assert_eq!(followers(&s, t), dana_before, "a name in an edit makes nobody follow");
        let (_, meta) = last_event(&s, "message.edited");
        assert!(meta.get("mentioned_users").is_none() && meta.get("ticket_id").is_none(), "{meta}");
    }
}

#[test]
fn a_deleted_message_starts_no_run() {
    let s = setup();
    let orbit = channel(&s, s.project);
    let message = sent_message(&s, Some(orbit), "@Ann can you look?");
    let before = untouched(&s);
    let runs_before = woken(&s, s.ann).len();
    assert!(matches!(delete(&s, message).unwrap(), CommandResult::MessageDeleted { message_id, .. } if message_id == message));
    assert_eq!(untouched(&s), before, "the row stays, nothing else changes");
    assert_eq!(woken(&s, s.ann).len(), runs_before);
    let read = view(&s, orbit, message);
    assert_eq!((read.text.as_str(), read.deleted_at_ms.is_some(), read.editable), (MESSAGE_DELETED_TEXT, true, false));
}

#[test]
fn a_delete_keeps_the_earlier_text_in_the_audit_and_none_in_the_metadata() {
    let s = setup();
    let orbit = channel(&s, s.project);
    let message = sent_message(&s, Some(orbit), "The launch code is 1234");
    delete(&s, message).unwrap();
    let (payload, meta) = last_event(&s, "message.deleted");
    assert_eq!(payload.as_deref(), Some("The launch code is 1234"));
    assert_eq!(meta, serde_json::json!({ "message_id": message, "by": s.user, "queued": false }));
    assert!(!meta.to_string().contains("launch"));
}

#[test]
fn what_a_deleted_message_said_is_in_no_row_and_no_history() {
    let s = setup();
    let orbit = channel(&s, s.project);
    let message = sent_message(&s, Some(orbit), "@dana the secret plan");
    let before = count(&s, "SELECT COUNT(*) FROM notifications");
    edit(&s, message, "@dana the changed secret plan").unwrap();
    delete(&s, message).unwrap();
    assert_eq!(count(&s, "SELECT COUNT(*) FROM notifications"), before);
    let leaked = count(&s, "SELECT COUNT(*) FROM notifications WHERE target_json LIKE '%secret plan%'");
    assert_eq!(leaked, 0, "the text never reaches a notification row");
    let in_metadata = count(&s, "SELECT COUNT(*) FROM events WHERE type IN ('message.edited', 'message.deleted') AND metadata_json LIKE '%secret plan%'");
    assert_eq!(in_metadata, 0, "nor the metadata of the edit and delete events");
}

#[test]
fn only_the_author_may_change_a_message_and_not_a_decisions_feedback_or_a_deleted_one() {
    let s = setup();
    let orbit = channel(&s, s.project);
    let message = sent_message(&s, Some(orbit), "Mine");
    let dana = other_member(&s);
    let as_dana = |command| s.service.execute(&ctx(Some(dana)), CommandEnvelope { command_id: CommandId::new(), expected_version: None, command }).map(|c| c.receipt.result);
    let err = as_dana(Command::EditMessage(EditMessage { message_id: message, text: "Hers".into(), confirm_secret: false })).unwrap_err();
    assert_eq!(err.code, ErrorCode::Forbidden);
    let err = as_dana(Command::DeleteMessage(DeleteMessage { message_id: message })).unwrap_err();
    assert_eq!(err.code, ErrorCode::Forbidden);
    assert_eq!(view(&s, orbit, message).text, "Mine");
    // Feedback of a decision.
    let t = new_ticket(&s, "Login", Some(s.ann));
    let (st, user) = (s.service.store().clone(), s.user);
    s.service.store().write(move |tx| discussion::decision_feedback(tx, &st, t, user, &serde_json::json!({}), "cmd", "Not good")).unwrap();
    let ticket_channel = s.service.store().read(move |c| Ok(aurelius_storage::discussions::ticket_conversation(c, t)?.unwrap())).unwrap();
    let feedback = messages(&s, ticket_channel).into_iter().find(|m| m.text == "Not good").unwrap();
    assert_eq!(edit(&s, feedback.id, "Fine").unwrap_err().code, ErrorCode::Validation);
    assert!(!feedback.editable, "the read refuses what the command refuses");
    // Deleted, then again.
    delete(&s, message).unwrap();
    assert_eq!(edit(&s, message, "Back").unwrap_err().code, ErrorCode::NotFound);
    assert_eq!(delete(&s, message).unwrap_err().code, ErrorCode::NotFound);
    assert_eq!(edit(&s, MessageId::new(), "x").unwrap_err().code, ErrorCode::NotFound);
}

#[test]
fn a_closed_discussion_refuses_an_edit_and_a_delete() {
    let s = setup();
    let orbit = channel(&s, s.project);
    let message = sent_message(&s, Some(orbit), "Before the archive");
    s.service.store().write(|tx| Ok(tx.execute("UPDATE projects SET status = 'archived'", [])?)).unwrap();
    assert_eq!(edit(&s, message, "After").unwrap_err().code, ErrorCode::ChannelArchived);
    assert_eq!(delete(&s, message).unwrap_err().code, ErrorCode::ChannelArchived);
    assert_eq!(view(&s, orbit, message).text, "Before the archive");
}

#[test]
fn an_edit_stores_the_text_through_the_vault_redactor() {
    let s = setup();
    let orbit = channel(&s, s.project);
    let message = sent_message(&s, Some(orbit), "Nothing yet");
    let value = format!("ghp_{}", CommandId::new().0.simple());
    let owner = format!("edit-test-{}", CommandId::new());
    let secret = aurelius_runtime::redact::Secret { name: "GH_TOKEN".into(), label: "GitHub token".into(), value: zeroize::Zeroizing::new(value.clone()) };
    aurelius_runtime::redact::install_set(&owner, vec![secret]);
    let done = edit(&s, message, &format!("Use {value} here"));
    aurelius_runtime::redact::install_set(&owner, Vec::new());
    done.unwrap();
    let stored = view(&s, orbit, message).text;
    assert!(!stored.contains(&value) && stored.contains("«secret:GH_TOKEN»"), "{stored}");
}

#[test]
fn the_text_of_an_edit_is_validated_like_a_message() {
    let s = setup();
    let orbit = channel(&s, s.project);
    let message = sent_message(&s, Some(orbit), "Keep");
    assert_eq!(edit(&s, message, "   ").unwrap_err().code, ErrorCode::Validation);
    assert_eq!(view(&s, orbit, message).text, "Keep");
}

fn post_message(s: &Setup, ticket: TicketId, text: &str) -> MessageId {
    match exec(s, Command::PostTicketMessage(ticket::PostTicketMessage { confirm_secret: false, ticket_id: ticket, text: text.into(), images: vec![], files: vec![], thread_parent: None })).unwrap() {
        CommandResult::TicketMessagePosted { message_id, .. } => message_id,
        other => panic!("{other:?}"),
    }
}
