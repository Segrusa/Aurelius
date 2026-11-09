use crate::run::approvals::RuntimePermission;
use crate::*;
use aurelius_domain::{
    ApprovalId, RunId,
    approval::{ApprovalStatus, DecideApproval},
    conversation::SendChatMessage,
};
use aurelius_storage::Store;

fn ctx() -> CommandContext {
    CommandContext { principal_id: PrincipalId::new(), client_id: ClientId::new(), origin: Origin::LocalIpc, request_id: 1, user_id: None }
}

/// A service with a running chat run, the state in which Claude Code can ask.
fn setup() -> (tempfile::TempDir, CommandService, RunId) {
    let dir = tempfile::tempdir().unwrap();
    let service = CommandService::new(std::sync::Arc::new(Store::open(dir.path()).unwrap()));
    let env = CommandEnvelope {
        command_id: CommandId::new(),
        expected_version: None,
        command: Command::SendChatMessage(SendChatMessage { confirm_secret: false, conversation_id: crate::tests::conversation(&service), text: "hoi".into(), images: Vec::new(), interrupt: false }),
    };
    let CommandResult::ChatMessageSent { run_id, .. } = service.execute(&ctx(), env).unwrap().receipt.result else { panic!() };
    // Running, so its job is claimed by the dispatcher.
    service
        .store()
        .write(move |tx| {
            tx.execute("UPDATE runs SET status = 'running' WHERE id = ?1", [run_id.to_string()])?;
            Ok(tx.execute("UPDATE jobs SET state = 'leased', lease_owner = 'test', lease_until_ms = 0 WHERE id = ?1", [run_id.to_string()])?)
        })
        .unwrap();
    (dir, service, run_id)
}

/// Claude Code wants to run a command; the user has to decide.
fn ask(service: &CommandService, run: RunId, call: &str, command: &str) -> ApprovalId {
    match service.request_runtime_permission(run, Some(call.into()), "Bash", &serde_json::json!({ "command": command })).unwrap() {
        RuntimePermission::Pending { approval_id, .. } => approval_id,
        RuntimePermission::Denied(why) => panic!("expected a pending approval: {why}"),
    }
}

fn digest_of(service: &CommandService, approval_id: ApprovalId) -> String {
    let store = service.store().clone();
    store
        .read(|c| {
            let read = |k: &str| store.blobs().read(k).ok().and_then(|b| String::from_utf8(b).ok());
            Ok(aurelius_storage::approvals::list_approvals(c, &read)?)
        })
        .unwrap()
        .into_iter()
        .find(|a| a.id == approval_id)
        .unwrap()
        .action_digest
}

fn decide(service: &CommandService, approval_id: ApprovalId, digest: String, approve: bool) -> Result<Committed, AppError> {
    service.execute(
        &ctx(),
        CommandEnvelope {
            command_id: CommandId::new(),
            expected_version: None,
            command: Command::DecideApproval(DecideApproval { approval_id, action_digest: digest, approve, always_allow: false }),
        },
    )
}

#[test]
fn an_approval_is_used_once() {
    let (_d, service, run) = setup();
    let approval = ask(&service, run, "c1", "ls");
    // Asked twice for the same call: the same approval, not a second one.
    assert_eq!(ask(&service, run, "c1", "ls"), approval);
    let digest = digest_of(&service, approval);
    let first = decide(&service, approval, digest.clone(), true).unwrap();
    assert!(matches!(first.receipt.result, CommandResult::ApprovalDecided { status: ApprovalStatus::Consumed, ok: true, .. }));
    let second = decide(&service, approval, digest, true).err().unwrap();
    assert_eq!(second.code, ErrorCode::Validation, "second response: already handled");
    assert_eq!(service.approval_status(approval).unwrap(), Some(ApprovalStatus::Consumed));
}

#[test]
fn an_approval_is_bound_to_the_shown_action() {
    let (_d, service, run) = setup();
    let approval = ask(&service, run, "c1", "ls");
    // The client shows a digest for another action (e.g. a stale card).
    let other = ask(&service, run, "c2", "rm -rf build");
    let wrong_digest = digest_of(&service, other);
    let result = decide(&service, approval, wrong_digest, true).unwrap();
    assert!(matches!(result.receipt.result, CommandResult::ApprovalDecided { status: ApprovalStatus::Invalidated, ok: false, .. }));
    assert_eq!(service.approval_status(approval).unwrap(), Some(ApprovalStatus::Invalidated));
}

#[test]
fn an_expired_approval_is_never_permission() {
    let (_d, service, run) = setup();
    let approval = ask(&service, run, "c1", "ls");
    let digest = digest_of(&service, approval);
    let id = approval.to_string();
    service.store().write(move |tx| Ok(tx.execute("UPDATE approvals SET expires_at_ms = 0 WHERE id = ?1", [id])?)).unwrap();
    let result = decide(&service, approval, digest, true).unwrap();
    assert!(matches!(result.receipt.result, CommandResult::ApprovalDecided { status: ApprovalStatus::Expired, ok: false, .. }));
}

#[test]
fn overdue_approvals_expire_on_their_own() {
    let (_d, service, run) = setup();
    let approval = ask(&service, run, "c1", "ls");
    let id = approval.to_string();
    service.store().write(move |tx| Ok(tx.execute("UPDATE approvals SET expires_at_ms = 0 WHERE id = ?1", [id])?)).unwrap();
    let notices = service.expire_approvals().unwrap();
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].kind, EventKind::ApprovalExpired);
    assert_eq!(service.approval_status(approval).unwrap(), Some(ApprovalStatus::Expired));
    assert!(service.expire_approvals().unwrap().is_empty(), "nothing left to expire");
}

#[test]
fn a_rejection_is_recorded() {
    let (_d, service, run) = setup();
    let approval = ask(&service, run, "c1", "ls");
    let digest = digest_of(&service, approval);
    let result = decide(&service, approval, digest, false).unwrap();
    assert!(matches!(result.receipt.result, CommandResult::ApprovalDecided { status: ApprovalStatus::Rejected, ok: false, .. }));
    assert_eq!(service.approval_status(approval).unwrap(), Some(ApprovalStatus::Rejected));
}

/// Claude Code's AskUserQuestion reaches the user as an approval; the
/// answers are stored with it and only a pending question can be answered.
#[test]
fn questions_are_answered_once_and_kept_for_the_bridge() {
    let (_d, service, run) = setup();
    let input = serde_json::json!({ "questions": [{ "question": "Which format?", "header": "Format", "multiSelect": false,
        "options": [{ "label": "Summary", "description": "Short" }, { "label": "Detailed", "description": "Long" }] }] });
    let RuntimePermission::Pending { approval_id, .. } = service.request_runtime_permission(run, Some("q1".into()), "AskUserQuestion", &input).unwrap() else {
        panic!("a question waits for the user")
    };
    let answer = |text: &str| {
        service.execute(
            &ctx(),
            CommandEnvelope {
                command_id: CommandId::new(),
                expected_version: None,
                command: Command::AnswerQuestion(aurelius_domain::approval::AnswerQuestion {
                    confirm_secret: false,
                    approval_id,
                    answers: [("Which format?".to_string(), text.to_string())].into_iter().collect(),
                }),
            },
        )
    };
    // Asked in a chat: the question and its options stand in the chat, like a plan.
    let conversation: String = service.store().read(move |c| Ok(c.query_row("SELECT conversation_id FROM runs WHERE id = ?1", [run.to_string()], |r| r.get(0))?)).unwrap();
    let conversation = aurelius_domain::ConversationId::parse(&conversation).unwrap();
    let messages = || {
        let read = |key: &str| service.store().blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
        service.store().read(|c| aurelius_storage::conversations::list_messages(c, conversation, &read)).unwrap()
    };
    let asked = messages().into_iter().find(|m| m.approval_id == Some(approval_id)).expect("the question is in the chat");
    assert_eq!(asked.update.as_deref(), Some("approval"));
    assert!(asked.text.contains("Which format?") && asked.text.contains("**Detailed** — Long"), "{}", asked.text);

    assert!(answer("  ").is_err(), "an empty answer is no answer");
    assert!(answer("Summary").is_ok());
    assert_eq!(service.approval_status(approval_id).unwrap(), Some(ApprovalStatus::Consumed));
    assert_eq!(service.approval_answer(approval_id).unwrap(), Some(serde_json::json!({ "Which format?": "Summary" })));
    // The answer is noted in the chat, under the question.
    let after = messages();
    let question_at = after.iter().position(|m| m.approval_id == Some(approval_id)).unwrap();
    let answered_at = after.iter().position(|m| m.text.starts_with("Answered:") && m.author.as_deref() == Some("You")).expect("the answer is in the chat");
    assert!(question_at < answered_at);
    assert!(answer("Detailed").is_err(), "answered once");
    // An ordinary approval cannot be "answered".
    let approval = ask(&service, run, "c1", "ls");
    let other = service.execute(
        &ctx(),
        CommandEnvelope {
            command_id: CommandId::new(),
            expected_version: None,
            command: Command::AnswerQuestion(aurelius_domain::approval::AnswerQuestion { confirm_secret: false, approval_id: approval, answers: [("x".to_string(), "y".to_string())].into_iter().collect() }),
        },
    );
    assert!(other.is_err());
}

/// Stopping a run closes its open approvals and questions; deciding one later
/// is refused and never sets the stopped run back to running.
#[test]
fn stopping_a_run_invalidates_its_open_approvals() {
    let (_d, service, run) = setup();
    let approval = ask(&service, run, "c1", "ls");
    let digest = digest_of(&service, approval);
    let input = serde_json::json!({ "questions": [{ "question": "Which format?", "header": "Format", "multiSelect": false, "options": [] }] });
    let RuntimePermission::Pending { approval_id: question, .. } = service.request_runtime_permission(run, Some("q1".into()), "AskUserQuestion", &input).unwrap() else {
        panic!("a question waits for the user")
    };
    let status = || service.store().read(move |c| Ok(c.query_row("SELECT status FROM runs WHERE id = ?1", [run.to_string()], |r| r.get::<_, String>(0))?)).unwrap();
    assert_eq!(status(), "waiting");

    let stop = CommandEnvelope { command_id: CommandId::new(), expected_version: None, command: Command::StopRun(aurelius_domain::conversation::StopRun { run_id: run }) };
    service.execute(&ctx(), stop).unwrap();
    assert_eq!(status(), "cancelling");
    assert_eq!(service.approval_status(approval).unwrap(), Some(ApprovalStatus::Invalidated));
    assert_eq!(service.approval_status(question).unwrap(), Some(ApprovalStatus::Invalidated));

    assert!(decide(&service, approval, digest, true).is_err(), "no longer pending");
    let answer = CommandEnvelope {
        command_id: CommandId::new(),
        expected_version: None,
        command: Command::AnswerQuestion(aurelius_domain::approval::AnswerQuestion { confirm_secret: false, approval_id: question, answers: [("Which format?".to_string(), "Summary".to_string())].into_iter().collect() }),
    };
    assert!(service.execute(&ctx(), answer).is_err(), "no longer pending");
    assert_eq!(status(), "cancelling", "a late decision does not revive the stopped run");
}

/// Claude Code's plan (ExitPlanMode) reaches the user as an approval. The user
/// approves it, asks for changes (feedback required) or rejects it; the
/// decision is stored for the bridge and can only be made once.
#[test]
fn plans_are_decided_once_and_the_decision_is_kept_for_the_bridge() {
    use aurelius_domain::approval::{DecidePlan, PlanDecision};
    let (_d, service, run) = setup();
    let decide_generic = decide;
    let input = serde_json::json!({ "plan": "# Plan\n\n1. Create hello.txt", "planFilePath": "C:/plans/p.md" });
    let ask_plan = |call: &str| match service.request_runtime_permission(run, Some(call.into()), "ExitPlanMode", &input).unwrap() {
        RuntimePermission::Pending { approval_id, .. } => approval_id,
        RuntimePermission::Denied(why) => panic!("a plan waits for the user: {why}"),
    };
    let decide = |approval_id, decision, feedback: Option<&str>| {
        service.execute(
            &ctx(),
            CommandEnvelope {
                command_id: CommandId::new(),
                expected_version: None,
                command: Command::DecidePlan(DecidePlan { confirm_secret: false, approval_id, decision, feedback: feedback.map(str::to_owned) }),
            },
        )
    };

    // Changes need words; an empty or blank feedback is refused and leaves the plan pending.
    let first = ask_plan("p1");
    assert!(decide(first, PlanDecision::Revise, None).is_err());
    assert!(decide(first, PlanDecision::Revise, Some("   ")).is_err());
    assert_eq!(service.approval_status(first).unwrap(), Some(ApprovalStatus::Pending));
    let mode = || service.run_mode(run).unwrap();
    let before = mode();
    assert!(decide(first, PlanDecision::Revise, Some("Also add a README")).is_ok());
    assert_eq!(service.approval_status(first).unwrap(), Some(ApprovalStatus::Rejected));
    // Asking for changes leaves the conversation's mode as it was.
    assert_eq!(mode(), before);
    assert_eq!(service.approval_answer(first).unwrap(), Some(serde_json::json!({ "decision": "revise", "feedback": "Also add a README" })));
    // Handled on another device already: a second decision is refused and changes nothing.
    assert!(decide(first, PlanDecision::Approve, None).is_err());
    assert_eq!(service.approval_status(first).unwrap(), Some(ApprovalStatus::Rejected));

    // The revised plan is a new approval; approving needs no feedback.
    let second = ask_plan("p2");
    assert_ne!(first, second);
    assert!(decide(second, PlanDecision::Approve, None).is_ok());
    assert_eq!(service.approval_status(second).unwrap(), Some(ApprovalStatus::Consumed));
    assert_eq!(service.approval_answer(second).unwrap(), Some(serde_json::json!({ "decision": "approve", "feedback": null })));
    assert!(decide(second, PlanDecision::ApproveEdits, None).is_err());
    // Approve & auto moves the conversation to Auto, so its mode chip shows it.
    assert_eq!(mode(), aurelius_domain::autonomy::AgentMode::Auto);

    // Approving in accept-edits mode is kept as such for the bridge.
    let edits = ask_plan("p2e");
    assert!(decide(edits, PlanDecision::ApproveEdits, None).is_ok());
    assert_eq!(service.approval_status(edits).unwrap(), Some(ApprovalStatus::Consumed));
    assert_eq!(service.approval_answer(edits).unwrap(), Some(serde_json::json!({ "decision": "approve_edits", "feedback": null })));
    assert_eq!(mode(), aurelius_domain::autonomy::AgentMode::AcceptEdits);

    // Rejecting works with or without a reason.
    let third = ask_plan("p3");
    assert!(decide(third, PlanDecision::Reject, None).is_ok());
    assert_eq!(service.approval_status(third).unwrap(), Some(ApprovalStatus::Rejected));
    assert_eq!(mode(), aurelius_domain::autonomy::AgentMode::AcceptEdits);

    // The generic approve button cannot approve a plan blind; rejecting works.
    let fourth = ask_plan("p4");
    let digest = digest_of(&service, fourth);
    assert!(decide_generic(&service, fourth, digest.clone(), true).is_err());
    assert_eq!(service.approval_status(fourth).unwrap(), Some(ApprovalStatus::Pending));
    assert!(decide_generic(&service, fourth, digest, false).is_ok());
    assert_eq!(service.approval_status(fourth).unwrap(), Some(ApprovalStatus::Rejected));

    // Only plans can be decided this way.
    let other = ask(&service, run, "c1", "ls");
    assert!(decide(other, PlanDecision::Approve, None).is_err());
    assert_eq!(service.approval_status(other).unwrap(), Some(ApprovalStatus::Pending));
}

