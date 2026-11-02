//! Working together per person on the wire (phase A, AUR-FA): the freeze is
//! strictly additive. The `Old*` types and pinned enum values are the shapes
//! before the freeze, as an older client has them: it must read what this server
//! writes, and a new enum value in a snapshot would fail its whole read (R1).

use crate::app::{NOT_FOR_APPS, required as app_required};
use crate::channels_tests::enum_values;
use crate::method::{Method, client_feature};
use crate::remote::{Capability, Needs, required};
use crate::rpc::{Call, FEATURES, Outcome, Request, ServerMessage, feature, quotes_no_params, reject_unparsed};
use crate::events::Event;
use crate::write::WriteMethod;
use aurelius_domain::group_chat::{DeleteMessage, EditMessage};
use aurelius_domain::task::RunSettings;
use aurelius_domain::ticket::*;
use aurelius_domain::views::*;
use aurelius_domain::{BoardColumnId, Command, CommandId, CommandResult, ConversationId, EventKind, EventNotice, MessageId, ProjectId, TaskId, TicketId, UserId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

fn writes() -> Vec<(Command, &'static str)> {
    let t = TicketId::new();
    vec![
        (Command::SetTicketResponsible(SetTicketResponsible { ticket_id: t, responsible_user_id: Some(UserId::new()) }), "ticket.set_responsible"),
        (Command::SetTicketResponsible(SetTicketResponsible { ticket_id: t, responsible_user_id: None }), "ticket.set_responsible"),
        (Command::FollowTicket(FollowTicket { ticket_id: t, on: true }), "ticket.follow"),
        (Command::EditMessage(EditMessage { message_id: MessageId::new(), text: "Better".into(), confirm_secret: false }), "message.edit"),
        (Command::DeleteMessage(DeleteMessage { message_id: MessageId::new() }), "message.delete"),
    ]
}

fn reads() -> Vec<(Method, &'static str)> {
    vec![
        (Method::TicketFollowers { ticket_id: TicketId::new() }, "ticket.followers"),
        (Method::TicketMyTickets { project_id: Some(ProjectId::new()), include_done: true }, "ticket.my_tickets"),
        (Method::TicketMyTickets { project_id: None, include_done: false }, "ticket.my_tickets"),
        (Method::MessageLocate { message_id: MessageId::new() }, "message.locate"),
        (Method::MessageHistory { message_id: MessageId::new() }, "message.history"),
    ]
}

#[test]
fn every_new_method_round_trips_under_its_name() {
    for (command, name) in writes() {
        let request = Request::write(1, CommandId::new(), WriteMethod::Domain(command));
        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["method"], name);
        assert_eq!(serde_json::from_value::<Request>(json).unwrap(), request, "{name}");
    }
    for (method, name) in reads() {
        let request = Request::read(2, method);
        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["method"], name);
        assert_eq!(serde_json::from_value::<Request>(json).unwrap(), request, "{name}");
    }
}

#[test]
fn my_tickets_takes_no_viewer_and_needs_no_params() {
    let bare: Method = crate::rpc::from_method_value(serde_json::json!({ "method": "ticket.my_tickets" })).unwrap();
    assert_eq!(bare, Method::TicketMyTickets { project_id: None, include_done: false });
    assert_eq!(serde_json::to_value(&bare).unwrap(), serde_json::json!({ "method": "ticket.my_tickets", "params": {} }));
}

#[test]
fn a_ticket_create_without_a_responsible_is_written_as_before() {
    let create = CreateTicket {
        confirm_secret: false,
        project_id: ProjectId::new(),
        title: "Login".into(),
        description: None,
        priority: TicketPriority::default(),
        labels: vec![],
        deadline_ms: None,
        owner_employee_id: None,
        run: RunSettings::default(),
        files: vec![],
        start: false,
        from_message: None,
        responsible_user_id: None,
    };
    let json = serde_json::to_value(&create).unwrap();
    assert!(json.get("responsible_user_id").is_none(), "an older server refuses a field it does not know");
    let old = serde_json::json!({ "project_id": create.project_id, "title": "Login" });
    assert_eq!(serde_json::from_value::<CreateTicket>(old).unwrap().responsible_user_id, None);
    let chosen = CreateTicket { responsible_user_id: Some(UserId::new()), ..create };
    assert_eq!(serde_json::from_value::<CreateTicket>(serde_json::to_value(&chosen).unwrap()).unwrap(), chosen);
}

#[test]
fn set_responsible_refuses_a_field_it_does_not_know_and_reads_nobody_when_left_out() {
    let ticket_id = TicketId::new();
    let known: SetTicketResponsible = serde_json::from_value(serde_json::json!({ "ticket_id": ticket_id })).unwrap();
    assert_eq!(known.responsible_user_id, None);
    assert!(serde_json::from_value::<SetTicketResponsible>(serde_json::json!({ "ticket_id": ticket_id, "assignee": "x" })).is_err());
}

#[test]
fn an_edit_writes_confirm_secret_only_when_it_is_true() {
    let edit = EditMessage { message_id: MessageId::new(), text: "Better".into(), confirm_secret: false };
    assert!(serde_json::to_value(&edit).unwrap().get("confirm_secret").is_none());
    let read: EditMessage = serde_json::from_value(serde_json::json!({ "message_id": edit.message_id, "text": "Better" })).unwrap();
    assert_eq!(read, edit);
}

#[test]
fn a_refused_message_edit_never_repeats_what_was_sent() {
    let secret = "sk-test-FAKE-repeat-me";
    assert!(quotes_no_params("message.edit"));
    assert!(!quotes_no_params("message.delete"));
    let frame = serde_json::json!({ "protocol": 5, "request_id": 9, "idempotency_key": CommandId::new(), "method": "message.edit", "params": { "message_id": secret, "text": secret } });
    let err = serde_json::from_value::<Request>(frame.clone()).unwrap_err();
    let ServerMessage::Response { outcome: Outcome::Error(error), .. } = reject_unparsed(Some(&frame), &err) else { panic!() };
    assert_eq!(error.message, "The request for message.edit is not valid.");
}

#[test]
fn every_new_read_and_write_is_for_devices_and_never_for_apps() {
    for (method, name) in reads() {
        let call = Call::Read(method);
        assert_eq!(required(&call), Needs::Cap(Capability::Read), "{name}");
        assert_eq!(app_required(&call), Err(NOT_FOR_APPS), "{name}");
    }
    for (command, name) in writes() {
        let needs = if name.starts_with("ticket.") { Capability::TaskControl } else { Capability::Chat };
        let call = Call::Write(WriteMethod::Domain(command));
        assert_eq!(required(&call), Needs::Cap(needs), "{name}");
        assert_eq!(app_required(&call), Err(NOT_FOR_APPS), "{name}");
    }
}

#[test]
fn only_message_locate_is_advertised_until_the_server_parts_are_built() {
    assert!(FEATURES.contains(&feature::MESSAGE_LOCATE));
    // Whoever finishes a server part puts its feature in `FEATURES` and takes it off this list.
    for stub in [feature::TICKET_RESPONSIBLE, feature::TICKET_FOLLOW, feature::PERSONAL_INBOX, feature::MY_TICKETS, feature::MESSAGE_EDITS] {
        assert!(!FEATURES.contains(&stub), "{stub}: a client must never call a stub");
    }
}

#[test]
fn the_feature_and_client_flag_names_are_the_ones_the_freeze_gives() {
    let features = [feature::TICKET_RESPONSIBLE, feature::TICKET_FOLLOW, feature::PERSONAL_INBOX, feature::MY_TICKETS, feature::MESSAGE_LOCATE, feature::MESSAGE_EDITS];
    assert_eq!(features, ["ticket_responsible", "ticket_follow", "personal_inbox", "my_tickets", "message_locate", "message_edits"]);
    assert_eq!((client_feature::TICKET_FOLLOW, client_feature::MESSAGE_EDITS), (feature::TICKET_FOLLOW, feature::MESSAGE_EDITS));
    assert_eq!(system_action::RESPONSIBLE, "responsible");
}

#[test]
fn the_new_event_kinds_have_their_wire_names() {
    for (kind, text) in [(EventKind::TicketFollowChanged, "ticket.follow_changed"), (EventKind::MessageEdited, "message.edited")] {
        assert_eq!(kind.as_str(), text);
        assert_eq!(EventKind::parse(text), Some(kind));
    }
}

#[test]
fn a_new_command_result_round_trips() {
    let (conversation_id, message_id) = (ConversationId::new(), MessageId::new());
    for result in [
        CommandResult::TicketFollowChanged { ticket_id: TicketId::new(), on: true, changed: true },
        CommandResult::MessageEdited { conversation_id, message_id, edited_at_ms: 5 },
        CommandResult::MessageDeleted { conversation_id, message_id, deleted_at_ms: 6 },
    ] {
        assert_eq!(serde_json::from_value::<CommandResult>(serde_json::to_value(&result).unwrap()).unwrap(), result);
    }
    // The queued message's result stays what an older client reads.
    let queued = serde_json::to_value(CommandResult::QueuedMessageDeleted { conversation_id, message_id, run_id: aurelius_domain::RunId::new() }).unwrap();
    assert_eq!(queued["type"], "queued_message_deleted");
}

#[test]
fn the_new_answers_have_their_wire_form() {
    let location = MessageLocation { message_id: MessageId::new(), conversation_id: ConversationId::new(), ticket_id: None, ticket_number: None, thread_root: None };
    let json = serde_json::to_value(&location).unwrap();
    assert_eq!(json.as_object().unwrap().len(), 2, "a channel's top-level message: {json}");
    let in_ticket = MessageLocation { ticket_id: Some(TicketId::new()), ticket_number: Some(12), thread_root: Some(MessageId::new()), ..location };
    assert_eq!(serde_json::from_value::<MessageLocation>(serde_json::to_value(&in_ticket).unwrap()).unwrap(), in_ticket);
    assert_eq!(serde_json::to_value(crate::rpc::ResponseBody::MessageLocation(in_ticket)).unwrap()["type"], "message_location");

    let history = MessageHistoryView { versions: vec![MessageVersionView { text: "Before".into(), replaced_at_ms: 3 }], deleted: false };
    assert_eq!(serde_json::from_value::<MessageHistoryView>(serde_json::to_value(&history).unwrap()).unwrap(), history);

    let followers = TicketFollowersView { followers: vec![FollowerView { user_id: UserId::new(), username: "ada".into(), disabled: false, reason: "manual".into(), followed_at_ms: 4 }] };
    assert_eq!(serde_json::from_value::<TicketFollowersView>(serde_json::to_value(&followers).unwrap()).unwrap(), followers);

    let mine = MyTicketsView { tickets: vec![ticket()], following: vec![TicketId::new()] };
    assert_eq!(serde_json::from_value::<MyTicketsView>(serde_json::to_value(&mine).unwrap()).unwrap(), mine);
    assert_eq!(MY_TICKETS_MAX, 500);
}

// ---- what an older client reads ----

fn ticket() -> TicketView {
    TicketView {
        id: TicketId::new(),
        number: 12,
        project_id: ProjectId::new(),
        project_name: "Atlas".into(),
        title: "Login".into(),
        description: None,
        column_id: BoardColumnId::new(),
        column: ColumnKind::Review,
        position: 1.0,
        priority: TicketPriority::High,
        created_by_user: UserId::new(),
        created_by_username: "ada".into(),
        owner_employee_id: None,
        owner_name: None,
        run: RunSettings::default(),
        labels: vec![],
        attachments: vec![],
        deadline_ms: None,
        root_task_id: None,
        root_task_status: None,
        review_feedback: None,
        version: 3,
        created_at_ms: 1,
        updated_at_ms: 2,
        started_at_ms: None,
        done_at_ms: None,
        archived_at_ms: None,
        conversation_id: None,
        login_user: UserId::new(),
        login_username: "ada".into(),
        needs: None,
        responsible_user_id: None,
        responsible_username: None,
        responsible_disabled: false,
    }
}

fn notification() -> NotificationView {
    NotificationView {
        id: "n1".into(),
        severity: "warning".into(),
        title: "Marie made you responsible".into(),
        subject: "#12 Login".into(),
        created_at_ms: 5,
        read: false,
        task_id: Some(TaskId::new()),
        target_type: "ticket".into(),
        ticket_id: None,
        ticket_number: None,
        conversation_id: None,
        message_id: None,
        reason: None,
        merged_count: 0,
        needs_you: false,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct OldNotificationView {
    id: String,
    severity: String,
    title: String,
    subject: String,
    created_at_ms: i64,
    read: bool,
    task_id: Option<TaskId>,
    target_type: String,
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

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct OldSystemLine {
    kind: OldSystemLineKind,
    #[serde(default)]
    by: Option<String>,
    #[serde(default)]
    to: Option<String>,
}

#[test]
fn an_older_client_reads_a_responsible_line_with_its_own_enum() {
    let mut line = SystemLine::new(SystemLineKind::Moved).with_action(system_action::RESPONSIBLE);
    (line.by, line.to) = (Some("marie".into()), Some("ada".into()));
    let json = serde_json::to_value(&line).unwrap();
    assert_eq!(json["kind"], "moved");
    assert_eq!(json["action"], "responsible");
    let old: OldSystemLine = serde_json::from_value(json).unwrap();
    assert_eq!((old.kind, old.by.as_deref(), old.to.as_deref()), (OldSystemLineKind::Moved, Some("marie"), Some("ada")));
}

#[test]
fn an_older_client_reads_a_row_of_the_personal_inbox() {
    let row = NotificationView {
        ticket_id: Some(TicketId::new()),
        ticket_number: Some(12),
        conversation_id: Some(ConversationId::new()),
        message_id: Some(MessageId::new()),
        reason: Some("a_reason_of_a_newer_server".into()),
        merged_count: 2,
        needs_you: true,
        ..notification()
    };
    let json = serde_json::to_value(&row).unwrap();
    assert_eq!((json["merged_count"].as_u64(), json["needs_you"].as_bool()), (Some(2), Some(true)));
    let old: OldNotificationView = serde_json::from_value(json.clone()).unwrap();
    assert_eq!((old.title.as_str(), old.task_id), ("Marie made you responsible", row.task_id));
    // A reason is text: a new one fails nothing for a client of today either.
    assert_eq!(serde_json::from_value::<NotificationView>(json).unwrap(), row);
}

#[test]
fn a_view_without_the_new_values_is_written_exactly_as_before() {
    let plain = notification();
    let old: OldNotificationView = serde_json::from_value(serde_json::to_value(&plain).unwrap()).unwrap();
    assert_eq!(serde_json::to_value(&plain).unwrap(), serde_json::to_value(&old).unwrap());

    let card = serde_json::to_value(ticket()).unwrap();
    for field in ["responsible_user_id", "responsible_username", "responsible_disabled"] {
        assert!(card.get(field).is_none(), "{field}");
    }
    // What an older server wrote reads at the defaults.
    let read: TicketView = serde_json::from_value(card).unwrap();
    assert_eq!((read.responsible_user_id, read.responsible_username, read.responsible_disabled), (None, None, false));

    let message: MessageView = serde_json::from_value(serde_json::json!({
        "id": MessageId::new(), "ordinal": 1, "role": "user", "status": "complete", "text": "Hi", "created_at_ms": 1, "run_id": null, "reply_to": null,
        "produced_by": null, "author": null, "sources": [], "activity": [], "update": null, "approval_id": null, "files": [], "uploads": [], "queued": false,
    }))
    .unwrap();
    assert_eq!((message.edited_at_ms, message.deleted_at_ms, message.editable), (None, None, false));
    let json = serde_json::to_value(&message).unwrap();
    for field in ["edited_at_ms", "deleted_at_ms", "editable"] {
        assert!(json.get(field).is_none(), "{field}");
    }
}

/// The sorted wire values of every enum in a type's schema.
fn enums_of<T: JsonSchema>() -> Vec<String> {
    let mut values = Vec::new();
    enum_values(&serde_json::to_value(schemars::schema_for!(T)).unwrap(), &mut values);
    values.sort();
    values.dedup();
    values
}

const MESSAGE_ENUMS: &[&str] = &["accept_edits", "ask", "assistant", "auto", "complete", "created_from", "delegated", "done", "failed", "interrupted", "limit", "moved", "partial", "plan", "reported", "review", "summary", "summary_failed", "ticket_created", "user"];
const TICKET_ENUMS: &[&str] = &["accept_edits", "ask", "auto", "backlog", "cancelled", "doing", "done", "draft", "failed", "high", "login", "low", "normal", "plan", "plan_review", "queued", "review", "running", "succeeded", "testing", "todo", "urgent", "waiting"];

#[test]
fn no_snapshot_holds_an_enum_value_an_older_client_does_not_know() {
    // The values before the freeze. A new one here fails an older client's whole read (R1).
    assert_eq!(enums_of::<MessageView>(), MESSAGE_ENUMS);
    assert_eq!(enums_of::<TicketView>(), TICKET_ENUMS);
    assert_eq!(enums_of::<MyTicketsView>(), TICKET_ENUMS);
    // Everything new that can grow is text, so none of these has an enum at all.
    assert_eq!(enums_of::<NotificationView>(), Vec::<String>::new());
    assert_eq!(enums_of::<TicketFollowersView>(), Vec::<String>::new());
    assert_eq!(enums_of::<MessageLocation>(), Vec::<String>::new());
    assert_eq!(enums_of::<MessageHistoryView>(), Vec::<String>::new());
}

// ---- `message.deleted` for a message that was not queued ----

#[test]
fn an_older_client_reads_message_deleted_the_same_whatever_its_metadata_says() {
    // The handlers of both clients look at the kind only and read the conversation
    // again. Metadata such as `queued: false` stays in the log: it is not on the wire.
    let notice = EventNotice { seq: 8, kind: EventKind::MessageDeleted, aggregate_id: ConversationId::new().to_string(), project_id: None, occurred_at_ms: 9 };
    let json = serde_json::to_value(Event::from(notice.clone())).unwrap();
    assert_eq!(json.as_object().unwrap().keys().map(String::as_str).collect::<Vec<_>>(), ["aggregate_id", "at_ms", "kind", "project_id", "seq"]);
    assert_eq!(json["kind"], "message.deleted");
    // Even if a server ever added it, the event reads as before.
    let mut with_metadata = json;
    with_metadata["metadata"] = serde_json::json!({ "message_id": MessageId::new(), "by": UserId::new(), "queued": false });
    let read: Event = serde_json::from_value(with_metadata).unwrap();
    assert_eq!(read.kind, EventKind::MessageDeleted);
    assert_eq!(read.kind.area(), "message", "the desktop refreshes the chat on this area");
}
