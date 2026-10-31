use super::*;
use aurelius_domain::project::{CreateProject, RenameProject, SetProjectDescription};
use aurelius_storage::queries;

fn ctx() -> CommandContext {
    CommandContext { principal_id: PrincipalId::new(), client_id: ClientId::new(), origin: Origin::LocalIpc, request_id: 1, app: None, user_id: None }
}

fn create(name: &str) -> CommandEnvelope {
    CommandEnvelope {
        command_id: CommandId::new(),
        expected_version: None,
        command: Command::CreateProject(CreateProject { name: name.into(), description: Some("Product development".into()), folder: None }),
    }
}

fn project_count(store: &Store) -> i64 {
    store.read(|c| Ok(c.query_row("SELECT COUNT(*) FROM projects", [], |r| r.get(0))?)).unwrap()
}

fn open(dir: &std::path::Path) -> CommandService {
    CommandService::new(Arc::new(Store::open(dir).unwrap()))
}

/// A conversation to send to with `chat.send`: a project conversation of a new
/// project (there are no standalone chats since protocol 3.0).
pub(crate) fn conversation(service: &CommandService) -> aurelius_domain::ConversationId {
    let context = ctx();
    let name = format!("Talk {}", aurelius_domain::ConversationId::new());
    let created = service.execute(&context, create(&name)).unwrap().receipt.result;
    let CommandResult::ProjectCreated { project_id, .. } = created else { panic!("{created:?}") };
    let start = CommandEnvelope {
        command_id: CommandId::new(),
        expected_version: None,
        command: Command::StartProjectConversation(aurelius_domain::project_conversation::StartProjectConversation { project_id, start: Default::default() }),
    };
    let started = service.execute(&context, start).unwrap().receipt.result;
    let CommandResult::ProjectConversationStarted { conversation_id, .. } = started else { panic!("{started:?}") };
    conversation_id
}

#[test]
fn project_survives_restart_with_description() {
    let dir = tempfile::tempdir().unwrap();
    let id = {
        let service = open(dir.path());
        let done = service.execute(&ctx(), create("Atlas")).unwrap();
        match done.receipt.result {
            CommandResult::ProjectCreated { project_id, version: 1 } => project_id,
            other => panic!("unexpected {other:?}"),
        }
    };

    let service = open(dir.path());
    let store = service.store().clone();
    let projects = store
        .read(|c| queries::list_projects(c, &|key| store.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok())))
        .unwrap();
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].id, id);
    assert_eq!(projects[0].name, "Atlas");
    assert_eq!(projects[0].description.as_deref(), Some("Product development"));
    assert_eq!(projects[0].employees_active, 0, "a new project has nobody at work");
}

#[test]
fn duplicate_command_returns_existing_receipt() {
    let dir = tempfile::tempdir().unwrap();
    let service = open(dir.path());
    let envelope = create("Atlas");

    let first = service.execute(&ctx(), envelope.clone()).unwrap();
    let second = service.execute(&ctx(), envelope).unwrap();

    assert!(!first.receipt.replayed);
    assert!(second.receipt.replayed);
    assert_eq!(first.receipt.result, second.receipt.result);
    assert_eq!(first.receipt.committed_seq, second.receipt.committed_seq);
    assert!(second.notices.is_empty(), "a replay publishes nothing new");
    assert_eq!(project_count(service.store()), 1);
}

#[test]
fn concurrent_duplicates_create_one_project() {
    let dir = tempfile::tempdir().unwrap();
    let service = Arc::new(open(dir.path()));
    let envelope = create("Atlas");
    let handles: Vec<_> = (0..8)
        .map(|_| {
            let service = service.clone();
            let envelope = envelope.clone();
            std::thread::spawn(move || service.execute(&ctx(), envelope).unwrap().receipt.result)
        })
        .collect();
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert!(results.windows(2).all(|w| w[0] == w[1]));
    assert_eq!(project_count(service.store()), 1);
}

#[test]
fn same_command_id_with_different_payload_conflicts() {
    let dir = tempfile::tempdir().unwrap();
    let service = open(dir.path());
    let envelope = create("Atlas");
    service.execute(&ctx(), envelope.clone()).unwrap();

    let mut altered = envelope;
    altered.command = Command::CreateProject(CreateProject { name: "Phoenix".into(), description: None, folder: None });
    let err = service.execute(&ctx(), altered).err().unwrap();
    assert_eq!(err.code, ErrorCode::IdempotencyConflict);
    assert_eq!(project_count(service.store()), 1);
}

#[test]
fn rename_with_stale_expected_version_conflicts() {
    let dir = tempfile::tempdir().unwrap();
    let service = open(dir.path());
    let CommandResult::ProjectCreated { project_id, .. } = service.execute(&ctx(), create("Atlas")).unwrap().receipt.result
    else {
        panic!()
    };
    let rename = |name: &str, expected| CommandEnvelope {
        command_id: CommandId::new(),
        expected_version: Some(expected),
        command: Command::RenameProject(RenameProject { project_id, name: name.into() }),
    };

    let ok = service.execute(&ctx(), rename("Atlas II", 1)).unwrap();
    assert_eq!(ok.receipt.result, CommandResult::ProjectRenamed { project_id, version: 2 });

    let err = service.execute(&ctx(), rename("Atlas III", 1)).err().unwrap();
    assert_eq!(err.code, ErrorCode::VersionConflict);

    let missing = CommandEnvelope {
        command_id: CommandId::new(),
        expected_version: None,
        command: Command::RenameProject(RenameProject { project_id: ProjectId::new(), name: "x".into() }),
    };
    assert_eq!(service.execute(&ctx(), missing).err().unwrap().code, ErrorCode::NotFound);
}

#[test]
fn set_description_bumps_version_clears_and_checks() {
    let dir = tempfile::tempdir().unwrap();
    let service = open(dir.path());
    let CommandResult::ProjectCreated { project_id, .. } = service.execute(&ctx(), create("Atlas")).unwrap().receipt.result
    else {
        panic!()
    };
    let set = |text: &str, expected| CommandEnvelope {
        command_id: CommandId::new(),
        expected_version: Some(expected),
        command: Command::SetProjectDescription(SetProjectDescription { project_id, description: text.into() }),
    };
    let store = service.store().clone();
    let shown = || {
        let projects = store
            .read(|c| queries::list_projects(c, &|key| store.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok())))
            .unwrap();
        (projects[0].description.clone(), projects[0].version)
    };

    let ok = service.execute(&ctx(), set("  Launch the app  ", 1)).unwrap();
    assert_eq!(ok.receipt.result, CommandResult::ProjectDescriptionSet { project_id, version: 2 });
    assert_eq!(ok.notices.len(), 1, "clients get a live project update");
    assert_eq!(ok.notices[0].kind, aurelius_domain::EventKind::ProjectUpdated);
    assert_eq!(shown(), (Some("Launch the app".into()), 2));

    assert_eq!(service.execute(&ctx(), set("Other", 1)).err().unwrap().code, ErrorCode::VersionConflict);
    let too_long = "a".repeat(project::DESCRIPTION_MAX + 1);
    assert_eq!(service.execute(&ctx(), set(&too_long, 2)).err().unwrap().code, ErrorCode::Validation);
    assert_eq!(shown(), (Some("Launch the app".into()), 2));

    service.execute(&ctx(), set("   ", 2)).unwrap();
    assert_eq!(shown(), (None, 3));

    let missing = CommandEnvelope {
        command_id: CommandId::new(),
        expected_version: None,
        command: Command::SetProjectDescription(SetProjectDescription { project_id: ProjectId::new(), description: "x".into() }),
    };
    assert_eq!(service.execute(&ctx(), missing).err().unwrap().code, ErrorCode::NotFound);
}

#[test]
fn invalid_command_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let service = open(dir.path());
    let err = service.execute(&ctx(), create("   ")).err().unwrap();
    assert_eq!(err.code, ErrorCode::Validation);
    let seq = service.store().read(|c| queries::last_seq(c)).unwrap();
    assert_eq!(seq, 0);
}

#[test]
fn events_record_server_side_context() {
    let dir = tempfile::tempdir().unwrap();
    let service = open(dir.path());
    let context = ctx();
    service.execute(&context, create("Atlas")).unwrap();
    let meta: String = service
        .store()
        .read(|c| Ok(c.query_row("SELECT metadata_json FROM events", [], |r| r.get(0))?))
        .unwrap();
    let meta: serde_json::Value = serde_json::from_str(&meta).unwrap();
    assert_eq!(meta["origin"], "local_ipc");
    assert_eq!(meta["client_id"], context.client_id.to_string());
}

#[test]
fn an_image_stays_with_the_message_and_reaches_the_model() {
    let dir = tempfile::tempdir().unwrap();
    let service = open(&dir.path().join("data"));
    let shot = dir.path().join("shot.png");
    image::RgbImage::from_pixel(2000, 1000, image::Rgb([10, 20, 30])).save(&shot).unwrap();
    let send = CommandEnvelope {
        command_id: CommandId::new(),
        expected_version: None,
        command: Command::SendChatMessage(aurelius_domain::conversation::SendChatMessage {
            conversation_id: conversation(&service),
            text: String::new(),
            images: vec![shot.display().to_string()],
            interrupt: false,
        }),
    };
    let CommandResult::ChatMessageSent { conversation_id, run_id, .. } = service.execute(&ctx(), send).unwrap().receipt.result else { panic!() };

    let read = |key: &str| service.store().blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
    let messages = service.store().read(|c| aurelius_storage::conversations::list_messages(c, conversation_id, &read)).unwrap();
    assert_eq!(messages[0].files.len(), 1);
    assert_eq!((messages[0].files[0].name.as_str(), messages[0].files[0].media_type.as_str()), ("shot.jpg", "image/jpeg"));

    let payload: String = service
        .store()
        .read(|c| Ok(c.query_row("SELECT payload_json FROM jobs WHERE id = ?1", [run_id.to_string()], |r| r.get(0))?))
        .unwrap();
    let plan = service.prepare_chat_run(&serde_json::from_str(&payload).unwrap()).unwrap().expect("a run to prepare");
    let request = plan.turns.last().unwrap();
    assert_eq!(request.text, "[Image 1: shot.jpg]");
    assert_eq!(request.images.len(), 1);
    let seen = image::load_from_memory(&request.images[0].data).unwrap();
    assert_eq!((seen.width(), seen.height()), (1568, 784), "scaled down");
}

#[test]
fn a_host_write_replays_with_the_same_parameters_and_conflicts_with_others() {
    let dir = tempfile::tempdir().unwrap();
    let service = open(dir.path());
    let key = CommandId::new();
    let hash = request_hash_of(&serde_json::json!({ "name": "github" }));
    assert!(service.host_receipt(key, &hash).unwrap().is_none(), "not used yet");

    let result = serde_json::json!({ "ok": true });
    let (seq, notice) = service.record_host_write(&ctx(), key, hash.clone(), "mcp.add", &result).unwrap();
    assert_eq!((notice.seq, notice.kind), (seq, EventKind::HostWrite));
    assert_eq!(service.host_receipt(key, &hash).unwrap(), Some((seq, result)));

    let other = request_hash_of(&serde_json::json!({ "name": "gitlab" }));
    assert_eq!(service.host_receipt(key, &other).err().unwrap().code, ErrorCode::IdempotencyConflict);
    assert_eq!(service.record_host_write(&ctx(), key, other, "mcp.add", &serde_json::json!({})).err().unwrap().code, ErrorCode::IdempotencyConflict);

    // The audit event names the method and the caller, never the parameters.
    let (method, meta): (String, String) = service
        .store()
        .read(|c| Ok(c.query_row("SELECT method, (SELECT metadata_json FROM events WHERE type = 'host.write') FROM command_receipts", [], |r| Ok((r.get(0)?, r.get(1)?)))?))
        .unwrap();
    assert_eq!(method, "mcp.add");
    assert!(meta.contains("mcp.add") && !meta.contains("github"));
}

#[test]
fn a_command_receipt_keeps_its_method() {
    let dir = tempfile::tempdir().unwrap();
    let service = open(dir.path());
    service.execute(&ctx(), create("Atlas")).unwrap();
    let method: String = service.store().read(|c| Ok(c.query_row("SELECT method FROM command_receipts", [], |r| r.get(0))?)).unwrap();
    assert_eq!(method, "project.create");
}

#[test]
fn notes_list_pinned_first_refuse_stale_saves_and_stay_out_of_archived_projects() {
    let dir = tempfile::tempdir().unwrap();
    let service = open(dir.path());
    let project = match service.execute(&ctx(), create("Atlas")).unwrap().receipt.result {
        CommandResult::ProjectCreated { project_id, .. } => project_id,
        other => panic!("unexpected {other:?}"),
    };

    let (first, notices) = service.create_note(project, "# Groceries\n- [ ] milk".into()).unwrap();
    assert_eq!((first.summary.title.as_str(), first.summary.preview.as_str(), first.summary.revision), ("Groceries", "- [ ] milk", 1));
    assert_eq!(notices[0].kind, EventKind::NoteCreated);
    assert_eq!(notices[0].project_id, Some(project));
    std::thread::sleep(std::time::Duration::from_millis(5));
    let (second, _) = service.create_note(project, "Ideas".into()).unwrap();
    let ids = |q: Option<&str>| service.notes(project, q.map(Into::into)).unwrap().notes.into_iter().map(|n| n.id).collect::<Vec<_>>();
    assert_eq!(ids(None), vec![second.summary.id, first.summary.id], "last edited first");

    let (pinned, notices) = service.set_note_pinned(first.summary.id, true).unwrap();
    assert!(pinned.summary.pinned && pinned.summary.revision == 1, "pinning keeps the revision");
    assert_eq!(notices[0].kind, EventKind::NoteUpdated);
    assert_eq!(ids(None), vec![first.summary.id, second.summary.id], "pinned first");
    assert_eq!(ids(Some("MILK")), vec![first.summary.id], "search ignores case");

    let (saved, notices) = service.update_note(first.summary.id, "# Groceries\n- [x] milk".into(), 1).unwrap();
    assert_eq!((saved.summary.revision, notices.len()), (2, 1));
    let (same, notices) = service.update_note(first.summary.id, "# Groceries\n- [x] milk".into(), 2).unwrap();
    assert_eq!((same.summary.revision, notices.len()), (2, 0), "the same text is no new revision");
    let stale = service.update_note(first.summary.id, "other device".into(), 1).unwrap_err();
    assert_eq!(stale.code, ErrorCode::VersionConflict);
    assert_eq!(service.note(first.summary.id).unwrap().body, "# Groceries\n- [x] milk");
    assert_eq!(service.update_note(first.summary.id, "x".repeat(aurelius_storage::notes::MAX_BODY_BYTES + 1), 2).unwrap_err().code, ErrorCode::Validation);

    let notices = service.delete_note(second.summary.id).unwrap();
    assert_eq!(notices[0].kind, EventKind::NoteDeleted);
    assert_eq!(service.note(second.summary.id).unwrap_err().code, ErrorCode::NotFound);

    let archive = CommandEnvelope {
        command_id: CommandId::new(),
        expected_version: None,
        command: Command::SetProjectStatus(aurelius_domain::employee::SetProjectStatus { project_id: project, status: aurelius_domain::employee::LifecycleStatus::Archived }),
    };
    service.execute(&ctx(), archive).unwrap();
    assert_eq!(service.create_note(project, "late".into()).unwrap_err().code, ErrorCode::Validation);
    assert_eq!(ids(None).len(), 1, "an archived project keeps its notes readable");
}

#[test]
fn a_waiting_run_whose_job_is_claimed_still_goes_through_cancelling() {
    let dir = tempfile::tempdir().unwrap();
    let service = open(dir.path());
    let send = CommandEnvelope {
        command_id: CommandId::new(),
        expected_version: None,
        command: Command::SendChatMessage(aurelius_domain::conversation::SendChatMessage {
            conversation_id: conversation(&service),
            text: "hello".into(),
            images: Vec::new(),
            interrupt: false,
        }),
    };
    let CommandResult::ChatMessageSent { run_id, .. } = service.execute(&ctx(), send).unwrap().receipt.result else { panic!() };
    // Waiting while its process runs (e.g. for an approval): the job is leased.
    service
        .store()
        .write(move |tx| {
            tx.execute("UPDATE runs SET status = 'waiting' WHERE id = ?1", [run_id.to_string()])?;
            tx.execute("UPDATE jobs SET state = 'leased', lease_owner = 'test', lease_until_ms = 0 WHERE id = ?1", [run_id.to_string()])?;
            Ok(())
        })
        .unwrap();
    let stop = CommandEnvelope { command_id: CommandId::new(), expected_version: None, command: Command::StopRun(aurelius_domain::conversation::StopRun { run_id }) };
    service.execute(&ctx(), stop).unwrap();
    let (run, job): (String, String) = service
        .store()
        .read(|c| Ok(c.query_row("SELECT r.status, j.state FROM runs r JOIN jobs j ON j.id = r.id WHERE r.id = ?1", [run_id.to_string()], |r| Ok((r.get(0)?, r.get(1)?)))?))
        .unwrap();
    assert_eq!((run.as_str(), job.as_str()), ("cancelling", "leased"), "the daemon interrupts its process");
}
