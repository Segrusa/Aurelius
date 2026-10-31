use crate::run::approvals::RuntimePermission;
use crate::*;
use aurelius_domain::{
    ApprovalId, RunId,
    approval::{ApprovalStatus, DecideApproval},
    conversation::SendChatMessage,
};
use aurelius_storage::Store;

fn ctx() -> CommandContext {
    CommandContext { principal_id: PrincipalId::new(), client_id: ClientId::new(), origin: Origin::LocalIpc, request_id: 1, app: None, user_id: None }
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

#[test]
fn an_artifact_publish_is_an_external_action_named_by_its_title_and_reads_are_reads() {
    use crate::run::approvals::runtime_tool_effect;
    use aurelius_domain::approval::Effect;
    let (dir, service, run) = setup();
    let page = dir.path().join("report.html");
    std::fs::write(&page, "<!doctype html><TITLE>\n  Q3   revenue </title><p>hi</p>").unwrap();
    let input = serde_json::json!({ "file_path": page.to_string_lossy(), "icon": "chart" });
    assert_eq!(runtime_tool_effect("Artifact", &input), Effect::ExternalWrite);
    assert_eq!(runtime_tool_effect("ArtifactComments", &serde_json::json!({ "action": "reply", "url": "u", "thread_id": "t" })), Effect::ExternalWrite);
    assert_eq!(runtime_tool_effect("Artifact", &serde_json::json!({ "action": "list" })), Effect::Read);
    assert_eq!(runtime_tool_effect("ArtifactData", &serde_json::json!({ "action": "query", "url": "u" })), Effect::Read);
    let RuntimePermission::Pending { approval_id, .. } = service.request_runtime_permission(run, Some("a1".into()), "Artifact", &input).unwrap() else { panic!("a publish is asked") };
    let store = service.store().clone();
    let approval = store
        .read(|c| {
            let read = |k: &str| store.blobs().read(k).ok().and_then(|b| String::from_utf8(b).ok());
            Ok(aurelius_storage::approvals::list_approvals(c, &read)?)
        })
        .unwrap()
        .into_iter()
        .find(|a| a.id == approval_id)
        .unwrap();
    assert_eq!(approval.effect, "external_write");
    assert_eq!(approval.reasons[0], "Publish an artifact to claude.ai: Q3 revenue");
    // A new version of a known page names it; without a readable file, the title argument.
    let republish = serde_json::json!({ "file_path": "missing.html", "title": "Board", "url": "https://claude.ai/artifact/abc" });
    assert_eq!(crate::run::approvals::artifact_reason("Artifact", &republish), "Publish a new version of the artifact https://claude.ai/artifact/abc: Board");
}

#[test]
fn published_artifacts_are_kept_by_url_listed_and_announced() {
    use aurelius_storage::artifacts::{Filter, Published};
    let (_d, service, run) = setup();
    let conversation = service
        .store()
        .read(move |c| Ok(c.query_row("SELECT conversation_id FROM runs WHERE id = ?1", [run.to_string()], |r| r.get::<_, String>(0))?))
        .map(|c| aurelius_domain::ConversationId::parse(&c).unwrap())
        .unwrap();
    let url = "https://claude.ai/artifact/2Jwgr8SC4Y55s2rme4hyfb";
    let notices = service.record_artifact(conversation, &Published { url: url.into(), title: "Spike".into(), version: Some(1), file: None }).unwrap();
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].kind, aurelius_domain::EventKind::ArtifactPublished);
    assert_eq!(notices[0].aggregate_id, conversation.to_string());
    // Republished: one row, newer version and title, first publish kept.
    service.record_artifact(conversation, &Published { url: url.into(), title: "Spike v2".into(), version: Some(2), file: None }).unwrap();
    service.record_artifact(conversation, &Published { url: "https://claude.ai/artifact/other".into(), title: " ".into(), version: None, file: None }).unwrap();
    let all = service.artifacts(Filter { conversation_id: Some(conversation), ..Default::default() }).unwrap();
    assert_eq!(all.len(), 2);
    let spike = all.iter().find(|a| a.url == url).unwrap();
    assert_eq!((spike.title.as_str(), spike.version, spike.task_id), ("Spike v2", 2, None));
    assert!(spike.project_id.is_some(), "a project conversation's artifact belongs to its project");
    assert!(spike.first_published_at_ms <= spike.last_published_at_ms);
    assert_eq!(all.iter().find(|a| a.url != url).unwrap().title, "Untitled artifact");
    assert!(service.artifacts(Filter { conversation_id: Some(aurelius_domain::ConversationId::new()), ..Default::default() }).unwrap().is_empty());
    assert_eq!(service.artifacts(Filter::default()).unwrap().len(), 2);
    // What a later run is told to republish to.
    let note = crate::run::plan::artifact_note(&service.store().read(|c| aurelius_storage::artifacts::for_run(c, conversation)).unwrap());
    assert!(note.contains(&format!("- Spike v2: {url} (version 2)")) && note.contains("republish to its URL"), "{note}");
    assert!(!crate::run::plan::artifact_note(&[]).contains("already published"));
}

#[test]
fn published_files_are_kept_per_version_and_deleted_conversations_drop_out() {
    use crate::run::artifacts::read_published_file;
    use aurelius_domain::views::ArtifactKind;
    use aurelius_storage::artifacts::{Filter, Published};
    let (dir, service, run) = setup();
    let conversation = service
        .store()
        .read(move |c| Ok(c.query_row("SELECT conversation_id FROM runs WHERE id = ?1", [run.to_string()], |r| r.get::<_, String>(0))?))
        .map(|c| aurelius_domain::ConversationId::parse(&c).unwrap())
        .unwrap();
    let folder = dir.path().join("work");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("page.html"), "<h1>v1</h1>").unwrap();
    std::fs::write(folder.join("notes.MD"), "# Notes").unwrap();
    std::fs::write(folder.join("big.html"), vec![b'a'; (crate::run::artifacts::MAX_FILE_BYTES + 1) as usize]).unwrap();
    std::fs::write(folder.join("binary.html"), [0xff, 0xfe, 0x00]).unwrap();
    let folders = vec![folder.clone()];
    // Relative to the run's folder, or absolute; too large, not text or missing is none.
    let page = read_published_file("page.html", &folders).unwrap();
    assert_eq!((page.kind, page.file_name.as_str(), page.content.as_str()), (ArtifactKind::Html, "page.html", "<h1>v1</h1>"));
    assert_eq!(read_published_file(folder.join("notes.MD").to_str().unwrap(), &[]).unwrap().kind, ArtifactKind::Markdown);
    assert!(read_published_file("big.html", &folders).is_none());
    assert!(read_published_file("binary.html", &folders).is_none());
    assert!(read_published_file("missing.html", &folders).is_none());

    let url = "https://claude.ai/artifact/kept";
    let publish = |version, file| service.record_artifact(conversation, &Published { url: url.into(), title: "Kept".into(), version, file }).unwrap();
    // Published before servers kept files: listed without content, and the content says so.
    service.record_artifact(conversation, &Published { url: "https://claude.ai/artifact/old".into(), title: "Old".into(), version: Some(1), file: None }).unwrap();
    let old = service.artifact_content("https://claude.ai/artifact/old".into(), None).unwrap();
    assert!(!old.artifact.has_content && old.snapshot.is_none() && old.versions.is_empty());
    publish(Some(1), Some(page));
    std::fs::write(folder.join("page.html"), "<h1>v2</h1>").unwrap();
    publish(Some(2), read_published_file("page.html", &folders));
    // Without a version from Claude Code the server counts on.
    std::fs::write(folder.join("page.html"), "<h1>v3</h1>").unwrap();
    publish(None, read_published_file("page.html", &folders));
    let latest = service.artifact_content(url.into(), None).unwrap();
    assert!(latest.artifact.has_content);
    assert_eq!(latest.versions, vec![3, 2, 1]);
    let snapshot = latest.snapshot.unwrap();
    assert_eq!((snapshot.version, snapshot.content.as_str()), (3, "<h1>v3</h1>"));
    assert_eq!(service.artifact_content(url.into(), Some(1)).unwrap().snapshot.unwrap().content, "<h1>v1</h1>");
    assert!(service.artifact_content(url.into(), Some(9)).unwrap().snapshot.is_none(), "a version without a kept file");
    assert_eq!(service.artifact_content("https://claude.ai/artifact/unknown".into(), None).unwrap_err().code, aurelius_domain::ErrorCode::NotFound);
    // Only the newest versions are kept.
    for v in 4..=(aurelius_storage::artifacts::KEPT_VERSIONS as u32 + 5) {
        publish(Some(v), read_published_file("page.html", &folders));
    }
    let kept = service.artifact_content(url.into(), None).unwrap().versions;
    assert_eq!(kept.len(), aurelius_storage::artifacts::KEPT_VERSIONS as usize);
    assert_eq!(kept.first(), Some(&(aurelius_storage::artifacts::KEPT_VERSIONS as u32 + 5)));
    // A deleted conversation's pages leave the list and cannot be opened.
    service.store().write(move |tx| aurelius_storage::conversations::delete_conversation(tx, conversation)).unwrap();
    assert!(service.artifacts(Filter::default()).unwrap().is_empty());
    assert_eq!(service.artifact_content(url.into(), None).unwrap_err().code, aurelius_domain::ErrorCode::NotFound);
    assert_eq!(service.artifact_content(url.into(), Some(1)).unwrap_err().code, aurelius_domain::ErrorCode::NotFound);
    // The kept files stay stored; only reading them is closed.
    let rows = service.store().read(move |c| Ok(c.query_row("SELECT COUNT(*) FROM artifact_versions WHERE url = ?1", [url], |r| r.get::<_, i64>(0))?)).unwrap();
    assert_eq!(rows, aurelius_storage::artifacts::KEPT_VERSIONS as i64);
}

#[test]
fn artifacts_of_a_deleted_conversation_are_not_listed() {
    use aurelius_storage::artifacts::{Filter, Published};
    let (_d, service, run) = setup();
    let conversation = service
        .store()
        .read(move |c| Ok(c.query_row("SELECT conversation_id FROM runs WHERE id = ?1", [run.to_string()], |r| r.get::<_, String>(0))?))
        .map(|c| aurelius_domain::ConversationId::parse(&c).unwrap())
        .unwrap();
    service.record_artifact(conversation, &Published { url: "https://claude.ai/artifact/gone".into(), title: "Gone".into(), version: Some(1), file: None }).unwrap();
    assert_eq!(service.artifacts(Filter::default()).unwrap().len(), 1);
    service.store().write(move |tx| aurelius_storage::conversations::delete_conversation(tx, conversation)).unwrap();
    assert!(service.artifacts(Filter::default()).unwrap().is_empty());
    assert!(service.artifacts(Filter { conversation_id: Some(conversation), ..Default::default() }).unwrap().is_empty());
    assert!(service.store().read(|c| aurelius_storage::artifacts::for_run(c, conversation)).unwrap().is_empty());
    // A publish that lands after the delete is still kept, just not listed.
    service.record_artifact(conversation, &Published { url: "https://claude.ai/artifact/late".into(), title: "Late".into(), version: None, file: None }).unwrap();
    assert!(service.artifacts(Filter::default()).unwrap().is_empty());
}

#[test]
fn artifacts_are_off_by_default_and_runs_are_unchanged_then() {
    let (_d, service, _run) = setup();
    assert!(!service.store().read(aurelius_storage::artifacts::enabled).unwrap());
    service.store().write(|tx| aurelius_storage::artifacts::set_enabled(tx, true, 1)).unwrap();
    assert!(service.store().read(aurelius_storage::artifacts::enabled).unwrap());
    service.store().write(|tx| aurelius_storage::artifacts::set_enabled(tx, false, 2)).unwrap();
    assert!(!service.store().read(aurelius_storage::artifacts::enabled).unwrap());
}
