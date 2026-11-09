//! Guidelines' surface (feature `guidelines`, additive in protocol 5): names on
//! the wire, optional filters, board-member writes that apps never make, and
//! the rights of a paired device.

use crate::method::Method;
use crate::remote::{self, Needs};
use crate::rpc::{Call, FEATURES, PROTOCOL_MAJOR, Request, feature};
use crate::write::WriteMethod;
use aurelius_domain::guideline::*;
use aurelius_domain::proposal::ProposalStatus;
use aurelius_domain::remote::Capability;
use aurelius_domain::{Command, CommandId, CommandResult, EmployeeId, ErrorCode, EventKind, GuidelineId, GuidelineProposalId, ProjectId, RunId};

fn reads() -> Vec<(Method, &'static str)> {
    let guideline_id = GuidelineId::new();
    vec![
        (Method::GuidelineList { archived: false }, "guideline.list"),
        (Method::GuidelineList { archived: true }, "guideline.list"),
        (Method::GuidelineGet { guideline_id, version: None }, "guideline.get"),
        (Method::GuidelineGet { guideline_id, version: Some(2) }, "guideline.get"),
        (Method::GuidelineVersions { guideline_id }, "guideline.versions"),
        (Method::GuidelineEffective { employee_id: Some(EmployeeId::new()), project_id: Some(ProjectId::new()) }, "guideline.effective"),
        (Method::GuidelineProposals { status: Some(ProposalStatus::Pending) }, "guideline.proposals"),
        (Method::GuidelineAffected { guideline_id }, "guideline.affected"),
        (Method::GuidelineStarterSet, "guideline.starter_set"),
        (Method::RunGuidelines { run_id: RunId::new() }, "run.guidelines"),
    ]
}

fn writes() -> Vec<(Command, &'static str)> {
    let guideline_id = GuidelineId::new();
    let target = GuidelineTarget::Project(ProjectId::new());
    vec![
        (Command::CreateGuideline(CreateGuideline { title: "Rust style".into(), text: "Run clippy.".into(), links: vec![GuidelineTarget::Employee(EmployeeId::new())] }), "guideline.create"),
        (Command::UpdateGuideline(UpdateGuideline { guideline_id, title: None, text: Some("Run clippy -D warnings.".into()) }), "guideline.update"),
        (Command::ArchiveGuideline(ArchiveGuideline { guideline_id }), "guideline.archive"),
        (Command::RestoreGuideline(RestoreGuideline { guideline_id }), "guideline.restore"),
        (Command::RestoreGuidelineVersion(RestoreGuidelineVersion { guideline_id, version: 1 }), "guideline.restore_version"),
        (Command::LinkGuideline(LinkGuideline { guideline_id, target }), "guideline.link"),
        (Command::UnlinkGuideline(UnlinkGuideline { guideline_id, target }), "guideline.unlink"),
        (
            Command::DecideGuidelineProposal(DecideGuidelineProposal { proposal_id: GuidelineProposalId::new(), approve: true, title: None, text: Some("Edited.".into()), links: Some(vec![target]), note: Some("Thanks".into()) }),
            "guideline.decide_proposal",
        ),
        (Command::AddStarterGuidelines(AddStarterGuidelines { keys: vec!["rust_style".into()] }), "guideline.add_starter_set"),
    ]
}

#[test]
fn the_guideline_methods_roundtrip_under_their_names() {
    assert_eq!(PROTOCOL_MAJOR, 5, "additive: the major stays");
    assert!(FEATURES.contains(&feature::GUIDELINES));
    for (method, name) in reads() {
        assert!(Method::knows(name), "{name}");
        let request = Request::read(1, method);
        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["method"], name);
        assert_eq!(serde_json::from_value::<Request>(json).unwrap(), request, "{name}");
    }
    for (command, name) in writes() {
        assert_eq!(command.method(), name);
        assert!(WriteMethod::knows(name), "{name}");
        let request = Request::write(2, CommandId::new(), WriteMethod::Domain(command));
        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["method"], name);
        assert!(json["params"].get("type").is_none());
        assert_eq!(serde_json::from_value::<Request>(json).unwrap(), request, "{name}");
    }
}

#[test]
fn filters_may_be_left_out_and_required_ids_may_not() {
    for (name, expected) in [
        ("guideline.list", Method::GuidelineList { archived: false }),
        ("guideline.effective", Method::GuidelineEffective { employee_id: None, project_id: None }),
        ("guideline.proposals", Method::GuidelineProposals { status: None }),
        ("guideline.starter_set", Method::GuidelineStarterSet),
    ] {
        for params in [None, Some(serde_json::Value::Null), Some(serde_json::json!({}))] {
            let mut json = serde_json::json!({ "protocol": 5, "request_id": 3, "method": name });
            if let Some(params) = params {
                json["params"] = params;
            }
            assert_eq!(serde_json::from_value::<Request>(json).unwrap().call, Call::Read(expected.clone()), "{name}");
        }
    }
    let err = serde_json::from_value::<Request>(serde_json::json!({ "protocol": 5, "request_id": 3, "method": "guideline.get" })).unwrap_err().to_string();
    assert!(err.contains("guideline_id"), "{err}");
    // A link names an employee or a project, never both (B2).
    let link = serde_json::json!({ "protocol": 5, "request_id": 4, "idempotency_key": CommandId::new(), "method": "guideline.link",
        "params": { "guideline_id": GuidelineId::new(), "target": { "kind": "employee", "id": EmployeeId::new() } } });
    assert!(serde_json::from_value::<Request>(link).is_ok());
    let both = serde_json::json!({ "protocol": 5, "request_id": 4, "idempotency_key": CommandId::new(), "method": "guideline.link",
        "params": { "guideline_id": GuidelineId::new(), "target": { "kind": "department", "id": EmployeeId::new() } } });
    assert!(serde_json::from_value::<Request>(both).is_err());
    // `create` without links, an update without a title.
    let create: Command = serde_json::from_value(serde_json::json!({ "type": "create_guideline", "title": "T", "text": "x" })).unwrap();
    assert!(matches!(create, Command::CreateGuideline(CreateGuideline { ref links, .. }) if links.is_empty()));
    let json = serde_json::to_value(&create).unwrap();
    assert!(json.get("links").is_none(), "the request hash of a write without links stays small");
}

#[test]
fn devices_need_read_to_read_and_task_control_to_change() {
    let phone = [Capability::Read, Capability::Chat, Capability::TaskControl, Capability::Approve];
    for (method, name) in reads() {
        assert_eq!(remote::required(&Call::Read(method)), Needs::Cap(Capability::Read), "{name}");
    }
    for (command, name) in writes() {
        let decide = matches!(command, Command::DecideGuidelineProposal(_));
        let needs = remote::required(&Call::Write(WriteMethod::Domain(command)));
        assert_eq!(needs, Needs::Cap(if decide { Capability::Approve } else { Capability::TaskControl }), "{name}");
        assert!(remote::grants(&phone, needs) && !remote::grants(&[Capability::Read, Capability::Chat], needs));
        // Deciding a proposal is approving (SEC0 T11): task control alone is not enough.
        assert_eq!(remote::grants(&[Capability::Read, Capability::TaskControl], needs), !decide, "{name}");
    }
}

#[test]
fn events_errors_and_results_are_new_names_only() {
    let kinds = [
        (EventKind::GuidelineCreated, "guideline.created"),
        (EventKind::GuidelineUpdated, "guideline.updated"),
        (EventKind::GuidelineArchived, "guideline.archived"),
        (EventKind::GuidelineRestored, "guideline.restored"),
        (EventKind::GuidelineLinked, "guideline.linked"),
        (EventKind::GuidelineUnlinked, "guideline.unlinked"),
        (EventKind::GuidelineProposed, "guideline.proposed"),
        (EventKind::GuidelineProposalDecided, "guideline.proposal_decided"),
    ];
    for (kind, name) in kinds {
        assert_eq!((kind.as_str(), kind.area()), (name, "guideline"));
    }
    assert_eq!(serde_json::to_value(ErrorCode::GuidelineTooLarge).unwrap(), "guideline_too_large");
    assert_eq!(serde_json::to_value(ErrorCode::GuidelineTitleTaken).unwrap(), "guideline_title_taken");
    // An error of an older server (without `too_large`) still parses.
    let old: aurelius_domain::AppError = serde_json::from_value(serde_json::json!({ "code": "validation", "message": "x" })).unwrap();
    assert!(old.too_large.is_none());
    let changed = CommandResult::GuidelineChanged { guideline_id: GuidelineId::new(), version: 3 };
    assert_eq!(serde_json::to_value(&changed).unwrap()["type"], "guideline_changed");
    let meta = GuidelineEventMeta { id: GuidelineId::new(), title: "Rust".into(), version: 1, by: None, target: None };
    let json = serde_json::to_value(&meta).unwrap();
    assert!(json.get("target").is_none() && json["by"].is_null());
}

#[test]
fn propose_guideline_travels_as_an_op() {
    use crate::bridge::EmployeeTaskOp;
    let op: EmployeeTaskOp = serde_json::from_value(serde_json::json!({ "op": "propose_guideline", "guideline": "g:01", "title": "Rust", "text": "Run clippy.", "reason": "Twice", "evidence": ["t1", "t2"] })).unwrap();
    assert_eq!(op, EmployeeTaskOp::ProposeGuideline { guideline: Some("g:01".into()), title: "Rust".into(), text: "Run clippy.".into(), reason: "Twice".into(), evidence: vec!["t1".into(), "t2".into()] });
    let json = serde_json::to_value(&op).unwrap();
    assert_eq!(json["op"], "propose_guideline");
}
