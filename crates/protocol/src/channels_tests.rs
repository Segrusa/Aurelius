//! Ticket channels on the wire (feature `ticket_channels`, AUR-CHAT-001): the
//! freeze is strictly additive. The `Old*` types are copies of the shapes on
//! feat/aur-t5 (before the freeze), as an older client has them: they must read
//! what this server writes, and this server must read what they wrote.

use crate::method::{Method, client_feature};
use crate::rpc::{FEATURES, feature};
use aurelius_domain::conversation::{MessageRole, MessageStatus};
use aurelius_domain::ticket::ColumnKind;
use aurelius_domain::views::*;
use aurelius_domain::{ApprovalId, Command, ConversationId, ErrorCode, MessageId, ProjectId, RunId, TaskId, TicketId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct OldChannelView {
    conversation_id: ConversationId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    project_id: Option<ProjectId>,
    name: String,
    unread: u32,
    mentions: u32,
    archived: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_activity_ms: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum OldSystemLineKind {
    Delegated,
    Reported,
    Moved,
    Plan,
    Review,
    Done,
    CreatedFrom,
    TicketCreated,
    Summary,
    SummaryFailed,
    Limit,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct OldSystemLine {
    kind: OldSystemLineKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    task_id: Option<TaskId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    to: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ticket_id: Option<TicketId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    conversation_id: Option<ConversationId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    message_id: Option<MessageId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct OldMessageView {
    id: MessageId,
    ordinal: u32,
    role: MessageRole,
    status: MessageStatus,
    text: String,
    created_at_ms: i64,
    run_id: Option<RunId>,
    reply_to: Option<MessageId>,
    produced_by: Option<String>,
    author: Option<String>,
    sources: Vec<SourceView>,
    activity: Vec<ActivityView>,
    update: Option<String>,
    approval_id: Option<ApprovalId>,
    files: Vec<FileRef>,
    uploads: Vec<MessageFileView>,
    queued: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    usage: Option<TokenUsage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mode: Option<aurelius_domain::autonomy::AgentMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    author_user: Option<aurelius_domain::user::UserView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    system: Option<OldSystemLine>,
}

fn ticket_channel() -> ChannelView {
    ChannelView {
        conversation_id: ConversationId::new(),
        project_id: Some(ProjectId::new()),
        name: ticket_channel_name(12, "Fix the login page"),
        unread: 2,
        mentions: 1,
        archived: true,
        last_activity_ms: Some(1_700_000_000_000),
        kind: ChannelKind::Ticket,
        ticket_id: Some(TicketId::new()),
        ticket_number: Some(12),
        ticket_column: Some(ColumnKind::Done),
        member: true,
        member_count: Some(3),
        read_through: Some(7),
    }
}

fn closed_line() -> SystemLine {
    let mut line = SystemLine::new(SystemLineKind::Done).with_action(system_action::CLOSED);
    (line.by, line.ticket_id, line.channel_id) = (Some("sebastien".into()), Some(TicketId::new()), Some(ConversationId::new()));
    line
}

fn message(system: Option<SystemLine>) -> MessageView {
    MessageView {
        edited_at_ms: None, deleted_at_ms: None, editable: false, awaiting_user: false, run: None,
        id: MessageId::new(),
        ordinal: 4,
        role: MessageRole::Assistant,
        status: MessageStatus::Complete,
        text: "#12 closed · sebastien".into(),
        created_at_ms: 1_700_000_000_000,
        run_id: None,
        reply_to: None,
        produced_by: None,
        author: Some("Aurelius".into()),
        sources: vec![],
        activity: vec![],
        update: Some("status".into()),
        approval_id: None,
        files: vec![],
        uploads: vec![],
        queued: false,
        usage: None,
        mode: None,
        author_user: None,
        system,
        thread_parent: Some(MessageId::new()),
        thread_reply_count: Some(2),
        thread_last_reply_ms: Some(1_700_000_000_500),
        thread_unread: Some(1),
        thread_repliers: vec!["Atlas".into(), "sebastien".into()],
        reactions: vec![ReactionView { emoji: "👍".into(), count: 2, mine: true, by: vec!["sebastien".into(), "Atlas".into()] }],
    }
}

#[test]
fn an_older_client_reads_a_ticket_channel_of_the_new_list() {
    let new = ticket_channel();
    let old: OldChannelView = serde_json::from_value(serde_json::to_value(&new).unwrap()).unwrap();
    assert_eq!(old, OldChannelView { conversation_id: new.conversation_id, project_id: new.project_id, name: "#12-fix-the-login-page".into(), unread: 2, mentions: 1, archived: true, last_activity_ms: new.last_activity_ms });
}

#[test]
fn a_channel_an_older_server_wrote_reads_with_every_new_field_at_its_default() {
    let old = OldChannelView { conversation_id: ConversationId::new(), project_id: None, name: "General".into(), unread: 0, mentions: 0, archived: false, last_activity_ms: None };
    let new: ChannelView = serde_json::from_value(serde_json::to_value(&old).unwrap()).unwrap();
    assert_eq!((new.kind, new.ticket_id, new.ticket_number, new.ticket_column, new.member, new.member_count, new.read_through), (ChannelKind::General, None, None, None, false, None, None));
    // And the new server writes General and a project channel exactly as before.
    assert_eq!(serde_json::to_value(&new).unwrap(), serde_json::to_value(&old).unwrap());
}

#[test]
fn a_channel_kind_of_a_newer_server_reads_as_unknown() {
    let mut json = serde_json::to_value(ticket_channel()).unwrap();
    json["kind"] = "huddle".into();
    assert_eq!(serde_json::from_value::<ChannelView>(json).unwrap().kind, ChannelKind::Unknown);
    assert_eq!(serde_json::to_value(ChannelKind::Ticket).unwrap(), "ticket");
}

#[test]
fn an_older_client_reads_a_message_with_threads_reactions_and_a_line_with_an_action() {
    let new = message(Some(closed_line()));
    let json = serde_json::to_value(&new).unwrap();
    assert_eq!(json["system"]["action"], "closed");
    assert_eq!(json["reactions"][0]["emoji"], "👍");
    assert_eq!(json["thread_repliers"], serde_json::json!(["Atlas", "sebastien"]));
    let old: OldMessageView = serde_json::from_value(json).unwrap();
    let line = old.system.unwrap();
    assert_eq!((line.kind, line.by.as_deref()), (OldSystemLineKind::Done, Some("sebastien")));
    assert_eq!(old.text, new.text, "the readable text is what an older client shows");
}

#[test]
fn a_message_an_older_server_wrote_reads_without_threads_or_reactions() {
    let mut json = serde_json::to_value(message(None)).unwrap();
    for field in ["thread_parent", "thread_reply_count", "thread_last_reply_ms", "thread_unread", "thread_repliers", "reactions"] {
        json.as_object_mut().unwrap().remove(field);
    }
    let old: OldMessageView = serde_json::from_value(json).unwrap();
    let new: MessageView = serde_json::from_value(serde_json::to_value(&old).unwrap()).unwrap();
    assert_eq!((new.thread_parent, new.thread_reply_count, new.thread_last_reply_ms, new.thread_unread, new.thread_repliers.len(), new.reactions.len()), (None, None, None, None, 0, 0));
    // Without threads or reactions the new server writes a message exactly as before.
    assert_eq!(serde_json::to_value(&new).unwrap(), serde_json::to_value(&old).unwrap());
}

/// The wire names in a JSON schema of a unit enum (its `enum` lists and `const`s, also inside `oneOf`).
pub(crate) fn enum_values(schema: &serde_json::Value, out: &mut Vec<String>) {
    match schema {
        serde_json::Value::Object(map) => {
            if let Some(serde_json::Value::Array(values)) = map.get("enum") {
                out.extend(values.iter().filter_map(|v| v.as_str().map(str::to_owned)));
            }
            if let Some(serde_json::Value::String(value)) = map.get("const") {
                out.push(value.clone());
            }
            map.values().for_each(|v| enum_values(v, out));
        }
        serde_json::Value::Array(items) => items.iter().for_each(|v| enum_values(v, out)),
        _ => {}
    }
}

#[test]
fn every_system_line_kind_is_one_an_older_client_knows() {
    let mut kinds = Vec::new();
    enum_values(&serde_json::to_value(schemars::schema_for!(SystemLineKind)).unwrap(), &mut kinds);
    assert_eq!(kinds.len(), 11, "no new SystemLineKind variant: {kinds:?}");
    for kind in kinds {
        assert!(serde_json::from_value::<OldSystemLineKind>(serde_json::Value::String(kind.clone())).is_ok(), "{kind} is new");
    }
}

#[test]
fn a_line_an_older_server_wrote_reads_without_an_action() {
    let old = OldSystemLine { kind: OldSystemLineKind::Moved, task_id: None, from: Some("Doing".into()), to: Some("Review".into()), by: None, ticket_id: None, conversation_id: None, message_id: None };
    let new: SystemLine = serde_json::from_value(serde_json::to_value(&old).unwrap()).unwrap();
    assert_eq!((new.kind, new.action, new.channel_id), (SystemLineKind::Moved, None, None));
}

#[test]
fn channel_list_keeps_its_wire_form_and_search_is_a_method_of_its_own() {
    assert_eq!(serde_json::to_value(Method::ChannelList).unwrap(), serde_json::json!({ "method": "channel.list" }));
    assert_eq!(serde_json::from_value::<Method>(serde_json::json!({ "method": "channel.list" })).unwrap(), Method::ChannelList);
    let search = Method::ChannelSearch { query: "#12".into() };
    assert_eq!(serde_json::to_value(&search).unwrap(), serde_json::json!({ "method": "channel.search", "params": { "query": "#12" } }));
}

#[test]
fn ticket_channels_threads_and_reactions_are_advertised_with_client_flags_of_the_same_name() {
    for (server, client) in [
        (feature::TICKET_CHANNELS, client_feature::TICKET_CHANNELS),
        (feature::CHAT_THREADS, client_feature::CHAT_THREADS),
        (feature::CHAT_REACTIONS, client_feature::CHAT_REACTIONS),
    ] {
        assert!(FEATURES.contains(&server), "{server}");
        assert_eq!(client, server);
    }
}

#[test]
fn a_thread_is_read_with_a_method_of_its_own() {
    let message_id = MessageId::new();
    let read = Method::ThreadRead { message_id };
    assert_eq!(serde_json::to_value(&read).unwrap(), serde_json::json!({ "method": "thread.read", "params": { "message_id": message_id } }));
    assert_eq!(serde_json::from_value::<Method>(serde_json::to_value(&read).unwrap()).unwrap(), read);
}

#[test]
fn a_thread_snapshot_has_its_root_and_replies_and_leaves_out_an_open_channel() {
    let root = message(None);
    let reply = MessageView { thread_parent: Some(root.id), thread_reply_count: None, thread_last_reply_ms: None, thread_unread: None, thread_repliers: vec![], reactions: vec![], ..message(None) };
    let snapshot = ThreadSnapshot { as_of_seq: 4, conversation_id: ConversationId::new(), root: root.clone(), replies: vec![reply], read_only: false };
    let json = serde_json::to_value(&snapshot).unwrap();
    assert!(json.get("read_only").is_none());
    assert_eq!(json["replies"][0]["thread_parent"], serde_json::to_value(root.id).unwrap());
    assert_eq!(serde_json::from_value::<ThreadSnapshot>(json).unwrap(), snapshot);
    let body = serde_json::to_value(crate::rpc::ResponseBody::Thread(snapshot)).unwrap();
    assert_eq!(body["type"], "thread");
}

#[test]
fn a_reaction_is_an_event_kind_of_its_own() {
    assert_eq!(aurelius_domain::EventKind::MessageReacted.as_str(), "message.reacted");
    assert_eq!(aurelius_domain::EventKind::parse("message.reacted"), Some(aurelius_domain::EventKind::MessageReacted));
}

#[test]
fn a_stream_outside_a_thread_is_written_as_before() {
    let delta = crate::StreamDelta { run_id: aurelius_domain::RunId::new(), conversation_id: ConversationId::new(), message_id: MessageId::new(), text: "x".into(), activity: vec![], progress: None, activity_changes: false, mode: None, thread_parent: None };
    assert!(serde_json::to_value(&delta).unwrap().get("thread_parent").is_none());
    let root = MessageId::new();
    let json = serde_json::to_value(crate::StreamDelta { thread_parent: Some(root), ..delta }).unwrap();
    assert_eq!(json["thread_parent"], serde_json::to_value(root).unwrap());
}

#[test]
fn a_reaction_is_the_write_message_react() {
    let react = Command::ReactToMessage(aurelius_domain::group_chat::ReactToMessage { message_id: MessageId::new(), emoji: "🎉".into(), on: true });
    assert_eq!(react.method(), "message.react");
    let json = serde_json::to_value(&react).unwrap();
    assert_eq!((json["emoji"].as_str(), json["on"].as_bool()), (Some("🎉"), Some(true)));
}

#[test]
fn a_send_without_a_thread_is_written_as_before() {
    let send = aurelius_domain::group_chat::SendGroupMessage { text: "Hi".into(), images: vec![], files: vec![], channel: None, confirm_secret: false, thread_parent: None };
    assert!(serde_json::to_value(&send).unwrap().get("thread_parent").is_none());
    let old = serde_json::json!({ "text": "Hi", "images": [], "files": [] });
    assert_eq!(serde_json::from_value::<aurelius_domain::group_chat::SendGroupMessage>(old).unwrap().thread_parent, None);
}

#[test]
fn channel_archived_is_its_own_error_code() {
    assert_eq!(serde_json::to_value(ErrorCode::ChannelArchived).unwrap(), "channel_archived");
}

#[test]
fn discussion_audience_keeps_its_wire_form_and_its_feature_is_offered() {
    let general = serde_json::json!({ "method": "discussion.audience" });
    assert_eq!(serde_json::to_value(Method::DiscussionAudience { channel: None }).unwrap(), general);
    assert_eq!(serde_json::from_value::<Method>(general).unwrap(), Method::DiscussionAudience { channel: None });
    let id = aurelius_domain::ConversationId::new();
    let named = serde_json::json!({ "method": "discussion.audience", "channel": id });
    assert_eq!(serde_json::from_value::<Method>(named).unwrap(), Method::DiscussionAudience { channel: Some(id) });
    assert!(crate::FEATURES.contains(&crate::feature::EVERYONE_MENTION));
    let body = crate::rpc::ResponseBody::DiscussionAudience(Default::default());
    assert_eq!(serde_json::to_value(body).unwrap()["type"], "discussion_audience");
}
