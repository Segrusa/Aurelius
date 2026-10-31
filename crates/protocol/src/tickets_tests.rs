//! The board on the wire (protocol 4): names, round trips and rights.

use crate::method::Method;
use crate::remote::{Capability, Needs, required};
use crate::rpc::{Call, Request};
use crate::write::WriteMethod;
use aurelius_domain::app_key::{AppAudience, AppScope};
use aurelius_domain::autonomy::AgentMode;
use aurelius_domain::company::{UpdateCompany, UpdateProject};
use aurelius_domain::task::RunSettings;
use aurelius_domain::ticket::*;
use aurelius_domain::{BoardColumnId, Command, CommandId, EmployeeId, ProjectId, TicketId};

fn create(mode: Option<AgentMode>) -> CreateTicket {
    CreateTicket {
        project_id: ProjectId::new(),
        title: "Login".into(),
        description: None,
        priority: TicketPriority::default(),
        labels: vec![],
        deadline_ms: None,
        owner_employee_id: None,
        run: RunSettings { mode, ..Default::default() },
        files: vec![],
        start: false,
    }
}

fn writes() -> Vec<Command> {
    let t = TicketId::new();
    vec![
        Command::CreateTicket(create(None)),
        Command::UpdateTicket(UpdateTicket { ticket_id: t, title: Some("x".into()), description: None, priority: None, labels: None, deadline_ms: None, clear_deadline: false, run: None }),
        Command::MoveTicket(MoveTicket { ticket_id: t, column_id: BoardColumnId::new(), before_ticket_id: None }),
        Command::StartTicket(StartTicket { ticket_id: t, employee_id: None }),
        Command::AssignTicket(AssignTicket { ticket_id: t, employee_id: Some(EmployeeId::new()), reason: None }),
        Command::ApprovePlan(ApprovePlan { ticket_id: t, decision: PlanApproval::Approve, feedback: None }),
        Command::RevisePlan(RevisePlan { ticket_id: t, feedback: "f".into() }),
        Command::AcceptTicket(AcceptTicket { ticket_id: t, note: None }),
        Command::SendBackTicket(SendBackTicket { ticket_id: t, feedback: "f".into() }),
        Command::CancelTicket(CancelTicket { ticket_id: t }),
        Command::ArchiveTicket(ArchiveTicket { ticket_id: t }),
        Command::UpdateBoardColumns(UpdateBoardColumns { columns: vec![BoardColumnEdit { column_id: BoardColumnId::new(), name: "Ideas".into() }] }),
        Command::UpdateCompany(UpdateCompany { name: None, dispatcher: Some(SetDispatcher { employee_id: None }) }),
        Command::UpdateProject(UpdateProject { project_id: ProjectId::new(), dispatcher: Some(SetDispatcher { employee_id: Some(EmployeeId::new()) }) }),
    ]
}

#[test]
fn every_board_method_round_trips_under_its_name() {
    let names = [
        "ticket.create", "ticket.update", "ticket.move", "ticket.start", "ticket.assign", "ticket.approve_plan", "ticket.revise_plan",
        "ticket.accept", "ticket.send_back", "ticket.cancel", "ticket.archive", "board.columns_update", "company.update", "project.update",
    ];
    for (command, name) in writes().into_iter().zip(names) {
        let request = Request::write(1, CommandId::new(), WriteMethod::Domain(command));
        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["method"], name);
        assert_eq!(serde_json::from_value::<Request>(json).unwrap(), request, "{name}");
    }
    let reads = [
        (Method::BoardGet { project_id: None }, "board.get"),
        (Method::TicketGet { ticket_id: TicketId::new() }, "ticket.get"),
        (Method::TicketList { project_id: None, column: Some(ColumnKind::PlanReview), query: Some("#3".into()) }, "ticket.list"),
    ];
    for (method, name) in reads {
        let request = Request::read(2, method);
        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["method"], name);
        assert_eq!(serde_json::from_value::<Request>(json).unwrap(), request, "{name}");
    }
    // A minimal create: priority Normal (B9), no start.
    let parsed: Request = serde_json::from_value(serde_json::json!({
        "protocol": 4, "request_id": 3, "idempotency_key": CommandId::new(), "method": "ticket.create",
        "params": { "project_id": ProjectId::new(), "title": "Hi" }
    }))
    .unwrap();
    let Call::Write(WriteMethod::Domain(Command::CreateTicket(created))) = parsed.call else { panic!() };
    assert_eq!((created.priority, created.start), (TicketPriority::Normal, false));
}

#[test]
fn devices_need_task_control_for_tickets_and_read_for_the_board() {
    for method in [Method::BoardGet { project_id: None }, Method::TicketGet { ticket_id: TicketId::new() }, Method::TicketList { project_id: None, column: None, query: None }] {
        assert_eq!(required(&Call::Read(method)), Needs::Cap(Capability::Read));
    }
    for command in writes() {
        assert_eq!(required(&Call::Write(WriteMethod::Domain(command))), Needs::Cap(Capability::TaskControl));
    }
    let auto = Call::Write(WriteMethod::Domain(Command::CreateTicket(create(Some(AgentMode::Auto)))));
    assert_eq!(required(&auto), Needs::Cap(Capability::Approve), "Auto needs what a task in Auto needs");
}

#[test]
fn apps_read_the_board_but_never_accept_or_send_back() {
    let all = [AppScope::Read, AppScope::Draft, AppScope::Run, AppScope::Approve];
    for audience in [AppAudience::All, AppAudience::Management] {
        assert!(crate::app::allowed_for(&all, audience, &Call::Read(Method::BoardGet { project_id: None })).is_ok());
        assert!(crate::app::allowed_for(&all, audience, &Call::Read(Method::TicketList { project_id: None, column: None, query: None })).is_ok());
        for command in writes() {
            let gate = matches!(command, Command::AcceptTicket(_) | Command::SendBackTicket(_));
            let result = crate::app::allowed_for(&all, audience, &Call::Write(WriteMethod::Domain(command)));
            if gate {
                assert_eq!(result.unwrap_err(), crate::app::REVIEW_GATE, "{audience:?}");
            }
        }
    }
    // Board settings are never an app's.
    let columns = Call::Write(WriteMethod::Domain(writes().remove(11)));
    assert!(crate::app::allowed(&all, &columns).is_err());
}

#[test]
fn a_move_with_the_old_position_is_refused_on_the_wire() {
    let request = |params: serde_json::Value| {
        serde_json::from_value::<Request>(serde_json::json!({
            "protocol": 4, "request_id": 4, "idempotency_key": CommandId::new(), "method": "ticket.move", "params": params
        }))
    };
    let (t, c) = (TicketId::new(), BoardColumnId::new());
    let err = request(serde_json::json!({ "ticket_id": t, "column_id": c, "position": 0 })).unwrap_err();
    assert!(err.to_string().contains("unknown field `position`"), "{err}");
    assert!(request(serde_json::json!({ "ticket_id": t, "column_id": c, "before_ticket_id": TicketId::new() })).is_ok());
    // `ticket.assign` with `employee_id: null`: back to the dispatcher.
    let parsed: Request = serde_json::from_value(serde_json::json!({
        "protocol": 4, "request_id": 5, "idempotency_key": CommandId::new(), "method": "ticket.assign",
        "params": { "ticket_id": t, "employee_id": null }
    }))
    .unwrap();
    let Call::Write(WriteMethod::Domain(Command::AssignTicket(assign))) = parsed.call else { panic!() };
    assert_eq!(assign.employee_id, None);
}

#[test]
fn a_damaged_remembered_user_does_not_fail_the_hello() {
    let hello = |user_id: serde_json::Value| {
        let parsed: Request = serde_json::from_value(serde_json::json!({
            "protocol": 4, "request_id": 6, "method": "session.hello",
            "params": { "client_name": "t", "client_version": "1", "user_id": user_id }
        }))
        .unwrap();
        let Call::Read(Method::SessionHello(params)) = parsed.call else { panic!() };
        params.user_id
    };
    assert_eq!(hello(serde_json::json!("not-a-uuid")), None);
    assert_eq!(hello(serde_json::json!(42)), None);
    assert_eq!(hello(serde_json::Value::Null), None);
    let id = aurelius_domain::UserId::new();
    assert_eq!(hello(serde_json::json!(id)), Some(id));
}
