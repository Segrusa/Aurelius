//! Work sessions (feature `work_sessions`): the database side of `work_session.start`.
//! The worktree itself is the daemon's (`daemon/src/quick_tests.rs`).

use crate::*;
use aurelius_domain::employee::CreateEmployee;
use aurelius_domain::project::CreateProject;
use aurelius_domain::task::RunSettings;
use aurelius_domain::work_session::{StartWorkSession, branch_of};
use aurelius_domain::{ConversationId, EmployeeId, ProjectId, UserId};
use aurelius_storage::Store;
use std::sync::Arc;

const PHC: &str = "$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHQ$aGFzaGhhc2hoYXNoaGFzaA";

struct Setup {
    _dir: tempfile::TempDir,
    service: CommandService,
    /// A project whose folder is a git repository.
    repo: ProjectId,
    repo_folder: std::path::PathBuf,
    /// A project whose folder is not.
    plain: ProjectId,
    user: UserId,
    other: UserId,
    orion: EmployeeId,
}

fn ctx(user: Option<UserId>) -> CommandContext {
    CommandContext { principal_id: aurelius_domain::PrincipalId::new(), client_id: aurelius_domain::ClientId::new(), origin: Origin::LocalIpc, request_id: 1, user_id: user }
}

fn exec_as(s: &Setup, user: Option<UserId>, command: Command) -> Result<CommandResult, AppError> {
    s.service.execute(&ctx(user), CommandEnvelope { command_id: aurelius_domain::CommandId::new(), expected_version: None, command }).map(|c| c.receipt.result)
}

fn add_user(service: &CommandService, name: &str) -> UserId {
    let user = UserId::new();
    let name = name.to_owned();
    service.store().write(move |tx| aurelius_storage::users::insert(tx, &aurelius_storage::users::NewUser { id: &user.to_string(), username: &name, created_by_user: None, now_ms: 1 }, PHC)).unwrap();
    user
}

fn setup() -> Setup {
    let dir = tempfile::tempdir().unwrap();
    let service = CommandService::new(Arc::new(Store::open(&dir.path().join("data")).unwrap()));
    let user = add_user(&service, "grace");
    let other = add_user(&service, "ada");
    let make = |name: &str, folder: &std::path::Path| {
        std::fs::create_dir_all(folder).unwrap();
        let command = Command::CreateProject(CreateProject { name: name.into(), description: None, folder: Some(folder.to_string_lossy().into_owned()) });
        let made = service.execute(&ctx(Some(user)), CommandEnvelope { command_id: aurelius_domain::CommandId::new(), expected_version: None, command }).unwrap().receipt.result;
        let CommandResult::ProjectCreated { project_id, .. } = made else { panic!("{made:?}") };
        project_id
    };
    let repo_folder = dir.path().join("repo");
    std::fs::create_dir_all(repo_folder.join(".git")).unwrap();
    // The folder as a project stores it: resolved, so on Windows a short temp path
    // (C:\Users\RUNNER~1\...) becomes the long one.
    let resolved = std::fs::canonicalize(&repo_folder).unwrap().to_string_lossy().into_owned();
    let repo_folder = std::path::PathBuf::from(resolved.strip_prefix(r"\\?\").unwrap_or(&resolved));
    let repo = make("Planora", &repo_folder);
    let plain = make("Plain", &dir.path().join("plain"));
    let create = CreateEmployee { access_level: None, tool_policy: None, start_profile: None, description: None, name: "Orion".into(), role: "Project Manager".into(), title: None, reports_to: None, instructions: None, folder: None };
    let made = service.execute(&ctx(Some(user)), CommandEnvelope { command_id: aurelius_domain::CommandId::new(), expected_version: None, command: Command::CreateEmployee(create) }).unwrap().receipt.result;
    let CommandResult::EmployeeCreated { employee_id: orion, .. } = made else { panic!("{made:?}") };
    Setup { _dir: dir, service, repo, repo_folder, plain, user, other, orion }
}

fn worktrees(s: &Setup) -> std::path::PathBuf {
    aurelius_domain::work_place::worktrees_dir(s.service.store().root())
}

/// A session as an older client asks for it: in a worktree of its own, or else on a branch of its own.
fn start(project: ProjectId, employee: EmployeeId, worktree: bool) -> Command {
    Command::StartWorkSession(StartWorkSession { project_id: project, employee_id: employee, text: "Geef me een status van de web app.".into(), images: vec![], files: vec![], worktree, own_branch: !worktree, run: RunSettings::default(), confirm_secret: false, design: false })
}

fn started(result: CommandResult) -> (ConversationId, aurelius_domain::RunId) {
    let CommandResult::WorkSessionStarted { conversation_id, run_id, .. } = result else { panic!("{result:?}") };
    (conversation_id, run_id)
}

#[test]
fn a_session_is_the_employees_conversation_in_the_project_without_a_ticket() {
    let s = setup();
    let (conversation, run) = started(exec_as(&s, Some(s.user), start(s.repo, s.orion, false)).unwrap());
    let (kind, project, employee, title, tickets, run_conversation): (String, String, String, String, i64, String) = s
        .service
        .store()
        .read(|c| {
            Ok(c.query_row(
                "SELECT c.kind, c.project_id, c.employee_id, c.title, (SELECT COUNT(*) FROM tickets), (SELECT conversation_id FROM runs WHERE id = ?2)
                 FROM conversations c WHERE c.id = ?1",
                [conversation.to_string(), run.to_string()],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
            )?)
        })
        .unwrap();
    assert_eq!((kind.as_str(), project, employee), ("session", s.repo.to_string(), s.orion.to_string()));
    assert_eq!(title, "Geef me een status van de web app.");
    assert_eq!(tickets, 0, "no ticket");
    assert_eq!(run_conversation, conversation.to_string(), "its first message is being answered");
    // It works in its own worktree, never in the project's folder (ticket #86).
    let folders = s.service.store().read(|c| aurelius_storage::conversations::work_folders(c, &worktrees(&s), conversation)).unwrap();
    let worktree = crate::conversation::work_session::worktree_path(s.service.store().root(), conversation);
    assert_eq!(folders.first().map(String::as_str), Some(worktree.to_string_lossy().as_ref()));
}

#[test]
fn only_the_board_member_who_started_a_session_lists_it() {
    let s = setup();
    let (conversation, _) = started(exec_as(&s, Some(s.user), start(s.repo, s.orion, false)).unwrap());
    let mine = s.service.store().read(|c| aurelius_storage::work_sessions::list(c, s.user, None)).unwrap();
    assert_eq!(mine.iter().map(|v| v.conversation_id).collect::<Vec<_>>(), [conversation]);
    assert_eq!((mine[0].employee_name.as_str(), mine[0].project_name.as_str(), mine[0].branch.as_deref()), ("Orion", "Planora", Some(branch_of(conversation).as_str())));
    assert!(mine[0].running, "its first run is queued");
    assert!(s.service.store().read(|c| aurelius_storage::work_sessions::list(c, s.other, None)).unwrap().is_empty());
    assert!(s.service.store().read(|c| aurelius_storage::work_sessions::list(c, s.user, Some(s.plain))).unwrap().is_empty(), "another project's");
}

#[test]
fn a_session_in_its_own_worktree_works_there_instead_of_in_the_project_folder() {
    let s = setup();
    let (conversation, _) = started(exec_as(&s, Some(s.user), start(s.repo, s.orion, true)).unwrap());
    let session = s.service.store().read(|c| aurelius_storage::work_sessions::of_conversation(c, conversation)).unwrap().unwrap();
    let (worktree, branch) = session.worktree.clone().expect("a worktree");
    assert_eq!(branch, branch_of(conversation));
    assert_eq!(std::path::PathBuf::from(&worktree), crate::conversation::work_session::worktree_path(s.service.store().root(), conversation));
    let folders = s.service.store().read(|c| aurelius_storage::conversations::work_folders(c, &worktrees(&s), conversation)).unwrap();
    assert_eq!(folders.first(), Some(&worktree), "never the project's folder");
    assert!(!folders.contains(&s.repo_folder.to_string_lossy().into_owned()));
}

#[test]
fn a_worktree_needs_a_project_folder_that_is_a_git_repository() {
    let s = setup();
    let refused = exec_as(&s, Some(s.user), start(s.plain, s.orion, true)).unwrap_err();
    assert!(refused.message.contains("git repository"), "{}", refused.message);
    // Nothing was made.
    let sessions: i64 = s.service.store().read(|c| Ok(c.query_row("SELECT COUNT(*) FROM conversations WHERE kind = 'session'", [], |r| r.get(0))?)).unwrap();
    assert_eq!(sessions, 0);
}

#[test]
fn every_session_gets_a_worktree_of_its_own_whatever_an_older_client_asks() {
    let s = setup();
    for (worktree, own_branch) in [(false, false), (false, true), (true, true)] {
        let mut command = start(s.repo, s.orion, worktree);
        if let Command::StartWorkSession(start) = &mut command {
            start.own_branch = own_branch;
        }
        let (conversation, _) = started(exec_as(&s, Some(s.user), command).unwrap());
        let session = s.service.store().read(|c| aurelius_storage::work_sessions::of_conversation(c, conversation)).unwrap().unwrap();
        let expected = crate::conversation::work_session::worktree_path(s.service.store().root(), conversation);
        assert_eq!(session.worktree, Some((expected.to_string_lossy().into_owned(), branch_of(conversation))), "asked {worktree}/{own_branch}");
        assert!(!session.own_branch, "never on its own branch in the project's folder");
    }
}

#[test]
fn files_with_a_sessions_first_message_go_into_its_worktree_folder() {
    let s = setup();
    let source = s._dir.path().join("brief.txt");
    std::fs::write(&source, "the brief").unwrap();
    let mut command = start(s.repo, s.orion, true);
    if let Command::StartWorkSession(start) = &mut command {
        start.files = vec![source.to_string_lossy().into_owned()];
    }
    let (conversation, _) = started(exec_as(&s, Some(s.user), command).unwrap());
    let worktree = crate::conversation::work_session::worktree_path(s.service.store().root(), conversation);
    let copied = worktree.join(crate::conversation::message_uploads::DIR).join("brief.txt");
    assert_eq!(std::fs::read_to_string(copied).unwrap(), "the brief", "waiting there until the daemon makes the worktree around it");
    assert!(!s.repo_folder.join(crate::conversation::message_uploads::DIR).exists(), "nothing in the project's folder");
}

#[test]
fn a_session_needs_a_board_member() {
    let s = setup();
    let refused = exec_as(&s, None, start(s.repo, s.orion, false)).unwrap_err();
    assert_eq!(refused.code, aurelius_domain::ErrorCode::BoardMemberRequired);
}

#[test]
fn a_session_starts_with_the_chosen_mode_model_and_effort() {
    let s = setup();
    let mut command = start(s.repo, s.orion, false);
    if let Command::StartWorkSession(start) = &mut command {
        start.run = RunSettings { mode: Some(aurelius_domain::autonomy::AgentMode::AcceptEdits), model: Some("sonnet".into()), effort: Some("high".into()), ..Default::default() };
    }
    let (conversation, _) = started(exec_as(&s, Some(s.user), command).unwrap());
    let (mode, run) = s
        .service
        .store()
        .read(|c| Ok((aurelius_storage::conversations::agent_mode(c, conversation)?, aurelius_storage::conversations::conversation_run(c, conversation)?)))
        .unwrap();
    assert_eq!(mode, aurelius_domain::autonomy::AgentMode::AcceptEdits);
    assert_eq!((run.model.as_deref(), run.effort.as_deref()), (Some("sonnet"), Some("high")));
}

#[test]
fn a_sessions_run_is_told_it_is_a_session_and_where_it_works() {
    let folder = crate::run::plan::work_session_guidance(None, false);
    assert!(folder.contains("## This is a work session") && folder.contains("no ticket") && folder.contains("project's folder itself"));
    assert!(!folder.contains("design session"), "a session that is no design");
    let own = crate::run::plan::work_session_guidance(Some(("/data/worktrees/session-1a2b3c4d", "session/1a2b3c4d")), false);
    assert!(own.contains("/data/worktrees/session-1a2b3c4d") && own.contains("session/1a2b3c4d") && own.contains("never merge"));
}

#[test]
fn an_older_session_in_the_project_folder_moves_to_a_worktree_of_its_own() {
    let s = setup();
    let (project, user, orion) = (s.repo, s.user, s.orion);
    let folder = s.repo_folder.to_string_lossy().into_owned();
    let (own, plain) = (ConversationId::new(), ConversationId::new());
    s.service
        .store()
        .write(move |tx| {
            aurelius_storage::work_sessions::insert(tx, own, project, orion, "On its branch", user, Some((&folder, &branch_of(own))), None, false, true)?;
            aurelius_storage::work_sessions::insert(tx, plain, project, orion, "In the folder", user, None, None, false, false)
        })
        .unwrap();
    for conversation in [own, plain] {
        let place = s.service.work_place_of_run(conversation, None).unwrap().expect("a place");
        let expected = crate::conversation::work_session::worktree_path(s.service.store().root(), conversation);
        assert_eq!((place.worktree.clone(), place.branch.clone(), place.repo.clone()), (expected.clone(), branch_of(conversation), s.repo_folder.clone()));
        assert_eq!(place.kind, crate::run::work_place::PlaceKind::Session { moves: true });
        s.service.move_session_to_worktree(conversation, &place.worktree, &place.branch).unwrap();
        let session = s.service.store().read(|c| aurelius_storage::work_sessions::of_conversation(c, conversation)).unwrap().unwrap();
        assert_eq!(session.worktree, Some((expected.to_string_lossy().into_owned(), branch_of(conversation))));
        assert!(!session.own_branch);
        let moved = s.service.work_place_of_run(conversation, None).unwrap().unwrap();
        assert_eq!(moved.kind, crate::run::work_place::PlaceKind::Session { moves: false }, "it stays there");
    }
}

#[test]
fn a_session_needs_a_git_folder_also_when_an_older_client_asks_for_its_own_branch() {
    let s = setup();
    let mut command = start(s.plain, s.orion, false);
    if let Command::StartWorkSession(start) = &mut command {
        start.own_branch = true;
    }
    let refused = exec_as(&s, Some(s.user), command).unwrap_err();
    assert!(refused.message.contains("git repository"), "{}", refused.message);
    let sessions: i64 = s.service.store().read(|c| Ok(c.query_row("SELECT COUNT(*) FROM conversations WHERE kind = 'session'", [], |r| r.get(0))?)).unwrap();
    assert_eq!(sessions, 0, "nothing was made");
}

#[test]
fn a_design_sessions_run_is_told_to_make_html_pages_and_name_their_paths() {
    let design = crate::run::plan::work_session_guidance(None, true);
    assert!(design.contains("## This is a work session") && design.contains("## This is a design session"), "{design}");
    assert!(design.contains("design/<kebab-name>.html") && design.contains("Name the full path"), "{design}");
}

#[test]
fn a_design_session_keeps_that_it_is_one() {
    let s = setup();
    let mut cmd = start(s.repo, s.orion, false);
    if let Command::StartWorkSession(start) = &mut cmd {
        start.design = true;
    }
    let (conversation, _) = started(exec_as(&s, Some(s.user), cmd).unwrap());
    let session = s.service.store().read(|c| aurelius_storage::work_sessions::of_conversation(c, conversation)).unwrap().unwrap();
    assert!(session.design);
    let listed = s.service.store().read(|c| aurelius_storage::work_sessions::list(c, s.user, None)).unwrap();
    assert!(listed.iter().any(|v| v.conversation_id == conversation && v.design), "{listed:?}");
}

#[test]
fn a_design_session_always_starts_in_auto_whatever_mode_was_asked() {
    use aurelius_domain::autonomy::AgentMode;
    let s = setup();
    let mut cmd = start(s.repo, s.orion, false);
    if let Command::StartWorkSession(start) = &mut cmd {
        start.design = true;
        start.run.mode = Some(AgentMode::Plan);
    }
    let (conversation, _) = started(exec_as(&s, Some(s.user), cmd).unwrap());
    let mode = s.service.store().read(|c| aurelius_storage::conversations::agent_mode(c, conversation)).unwrap();
    assert_eq!(mode, AgentMode::Auto, "a design writes its pages, which Plan or Ask would stop");
}

#[test]
fn a_design_sessions_mode_cannot_leave_auto_but_a_plain_session_can() {
    use aurelius_domain::autonomy::{AgentMode, SetAgentMode};
    let s = setup();
    let mut cmd = start(s.repo, s.orion, false);
    if let Command::StartWorkSession(start) = &mut cmd {
        start.design = true;
    }
    let (design, _) = started(exec_as(&s, Some(s.user), cmd).unwrap());
    for mode in [AgentMode::Plan, AgentMode::Ask, AgentMode::AcceptEdits] {
        let refused = exec_as(&s, Some(s.user), Command::SetAgentMode(SetAgentMode { conversation_id: design, mode }));
        let refused = refused.expect_err("a design stays in Auto");
        assert_eq!(refused.code, aurelius_domain::ErrorCode::Validation, "{mode:?}: {refused:?}");
    }
    exec_as(&s, Some(s.user), Command::SetAgentMode(SetAgentMode { conversation_id: design, mode: AgentMode::Auto })).unwrap();
    assert_eq!(s.service.store().read(|c| aurelius_storage::conversations::agent_mode(c, design)).unwrap(), AgentMode::Auto);
    let (plain, _) = started(exec_as(&s, Some(s.user), start(s.repo, s.orion, false)).unwrap());
    exec_as(&s, Some(s.user), Command::SetAgentMode(SetAgentMode { conversation_id: plain, mode: AgentMode::Plan })).unwrap();
}

#[test]
fn approving_a_designs_plan_with_accept_edits_keeps_it_in_auto() {
    use aurelius_domain::approval::{DecidePlan, PlanDecision};
    use aurelius_domain::autonomy::AgentMode;
    let s = setup();
    let mut cmd = start(s.repo, s.orion, false);
    if let Command::StartWorkSession(start) = &mut cmd {
        start.design = true;
    }
    let (_, run) = started(exec_as(&s, Some(s.user), cmd).unwrap());
    // Running, so its job is claimed by the dispatcher.
    s.service
        .store()
        .write(move |tx| {
            tx.execute("UPDATE runs SET status = 'running' WHERE id = ?1", [run.to_string()])?;
            Ok(tx.execute("UPDATE jobs SET state = 'leased', lease_owner = 'test', lease_until_ms = 0 WHERE id = ?1", [run.to_string()])?)
        })
        .unwrap();
    let input = serde_json::json!({ "plan": "# Plan\n\n1. Make the page" });
    let crate::run::approvals::RuntimePermission::Pending { approval_id, .. } = s.service.request_runtime_permission(run, Some("p1".into()), "ExitPlanMode", &input).unwrap() else { panic!("a plan waits for the user") };
    exec_as(&s, Some(s.user), Command::DecidePlan(DecidePlan { confirm_secret: false, approval_id, decision: PlanDecision::ApproveEdits, feedback: None })).unwrap();
    assert_eq!(s.service.run_mode(run).unwrap(), AgentMode::Auto);
}

#[test]
fn the_board_member_who_started_a_session_goes_on_in_it_with_chat_send() {
    let s = setup();
    let (conversation, _) = started(exec_as(&s, Some(s.user), start(s.repo, s.orion, false)).unwrap());
    let send = || Command::SendChatMessage(aurelius_domain::conversation::SendChatMessage { conversation_id: conversation, text: "En de API?".into(), images: vec![], files: vec![], interrupt: false, confirm_secret: false });
    let sent = exec_as(&s, Some(s.user), send()).unwrap();
    assert!(matches!(sent, CommandResult::ChatMessageSent { conversation_id, .. } if conversation_id == conversation), "{sent:?}");
    let refused = exec_as(&s, Some(s.other), send()).unwrap_err();
    assert_eq!(refused.code, aurelius_domain::ErrorCode::NotFound, "another board member's session");
}

/// A file outside every work folder, as a pasted or chosen file the client kept.
fn outside_file(s: &Setup, name: &str) -> String {
    let path = s._dir.path().join("kept").join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "notes").unwrap();
    path.to_string_lossy().into_owned()
}

/// The uploads kept with a conversation's only (or last) user message.
fn uploads_of(s: &Setup, conversation: ConversationId) -> Vec<String> {
    let read = |key: &str| s.service.store().blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
    let messages = s.service.store().read(|c| aurelius_storage::conversations::list_messages(c, conversation, &read)).unwrap();
    let last = messages.iter().rev().find(|m| m.role == aurelius_domain::conversation::MessageRole::User).expect("a user message");
    last.uploads.iter().map(|u| u.path.clone()).collect()
}

#[test]
fn chat_send_in_a_session_copies_its_files_into_the_work_folder_and_may_have_no_text() {
    let s = setup();
    let (conversation, _) = started(exec_as(&s, Some(s.user), start(s.repo, s.orion, false)).unwrap());
    let send = Command::SendChatMessage(aurelius_domain::conversation::SendChatMessage { conversation_id: conversation, text: String::new(), images: vec![], files: vec![outside_file(&s, "data.csv")], interrupt: false, confirm_secret: false });
    exec_as(&s, Some(s.user), send).unwrap();
    let uploads = uploads_of(&s, conversation);
    assert_eq!(uploads.len(), 1);
    let worktree = crate::conversation::work_session::worktree_path(s.service.store().root(), conversation);
    assert!(worktree.join(&uploads[0]).is_file(), "in the session's worktree folder");
    assert!(!s.repo_folder.join(&uploads[0]).exists(), "not in the project's folder");
}

#[test]
fn chat_send_refuses_a_folder_as_a_file() {
    let s = setup();
    let (conversation, _) = started(exec_as(&s, Some(s.user), start(s.repo, s.orion, false)).unwrap());
    let folder = s._dir.path().join("plain").to_string_lossy().into_owned();
    let send = Command::SendChatMessage(aurelius_domain::conversation::SendChatMessage { conversation_id: conversation, text: "Kijk".into(), images: vec![], files: vec![folder], interrupt: false, confirm_secret: false });
    let refused = exec_as(&s, Some(s.user), send).unwrap_err();
    assert!(refused.message.contains("not a folder"), "{}", refused.message);
}

fn send_in(conversation: ConversationId) -> Command {
    Command::SendChatMessage(aurelius_domain::conversation::SendChatMessage { conversation_id: conversation, text: "Nog iets.".into(), images: vec![], files: vec![], interrupt: false, confirm_secret: false })
}

#[test]
fn writing_in_a_session_is_refused_while_its_employee_is_paused_or_archived() {
    use aurelius_domain::employee::{LifecycleStatus, SetEmployeeStatus};
    let s = setup();
    let (conversation, _) = started(exec_as(&s, Some(s.user), start(s.repo, s.orion, false)).unwrap());
    for (status, said) in [(LifecycleStatus::Paused, "is paused"), (LifecycleStatus::Archived, "is archived")] {
        exec_as(&s, Some(s.user), Command::SetEmployeeStatus(SetEmployeeStatus { employee_id: s.orion, status })).unwrap();
        let refused = exec_as(&s, Some(s.user), send_in(conversation)).unwrap_err();
        assert!(refused.message.contains(said), "{}", refused.message);
    }
}

#[test]
fn writing_in_a_session_the_dispatcher_has_not_filled_is_refused() {
    let s = setup();
    let (conversation, _) = started(exec_as(&s, Some(s.user), start(s.repo, s.orion, false)).unwrap());
    s.service.store().write(move |tx| Ok(tx.execute("UPDATE work_sessions SET dispatch = 'nobody' WHERE conversation_id = ?1", [conversation.to_string()])?)).unwrap();
    let refused = exec_as(&s, Some(s.user), send_in(conversation)).unwrap_err();
    assert!(refused.message.contains(crate::conversation::session_dispatch::NOBODY_YET), "{}", refused.message);
}

#[test]
fn writing_in_a_conversation_that_does_not_exist_is_not_found() {
    let s = setup();
    let refused = exec_as(&s, Some(s.user), send_in(ConversationId::new())).unwrap_err();
    assert_eq!(refused.code, aurelius_domain::ErrorCode::NotFound);
}

/// The plan of the next run, as the daemon claims and prepares it (after it made the
/// sessions' worktrees: here only their folders, as git is the daemon's).
fn next_plan(s: &Setup) -> crate::run::plan::ChatRunPlan {
    let job = s.service.store().write(|tx| aurelius_storage::queries::claim_due_job(tx, "test", aurelius_storage::now_ms(), 45_000, 4)).unwrap().expect("a job");
    if let Some(conversation) = job.payload["conversation_id"].as_str().and_then(|c| ConversationId::parse(c).ok()) {
        std::fs::create_dir_all(crate::conversation::work_session::worktree_path(s.service.store().root(), conversation).join(".git")).unwrap();
    }
    s.service.prepare_chat_run(&job.payload).unwrap().expect("a run to prepare")
}

#[test]
fn only_a_design_sessions_run_may_render_its_pages_and_is_told_to_look_at_them() {
    let s = setup();
    let mut cmd = start(s.repo, s.orion, false);
    if let Command::StartWorkSession(start) = &mut cmd {
        start.design = true;
    }
    let (conversation, _) = started(exec_as(&s, Some(s.user), cmd).unwrap());
    let plan = next_plan(&s);
    assert_eq!(plan.conversation_id, conversation);
    assert!(plan.design_session, "a design session's run offers view_design");
    assert!(plan.work.is_some(), "with the work folder it renders from");
    assert!(plan.system.as_deref().unwrap_or("").contains("view_design"), "and is told to use it");

    let s = setup();
    let (conversation, _) = started(exec_as(&s, Some(s.user), start(s.repo, s.orion, false)).unwrap());
    let plan = next_plan(&s);
    assert_eq!(plan.conversation_id, conversation);
    assert!(!plan.design_session, "a plain session does not");
    assert!(!plan.system.as_deref().unwrap_or("").contains("view_design"));
}

fn ticket_from(s: &Setup, project: ProjectId, message: aurelius_domain::MessageId) -> Result<CommandResult, AppError> {
    let create = aurelius_domain::ticket::CreateTicket {
        responsible_user_id: None,
        confirm_secret: false,
        project_id: project,
        title: "Build pricing.html".into(),
        description: None,
        priority: Default::default(),
        labels: vec![],
        deadline_ms: None,
        owner_employee_id: None,
        run: Default::default(),
        files: vec![],
        start: false,
        from_message: Some(message),
        from_idea: None,
        kind: None,
    };
    exec_as(s, Some(s.user), Command::CreateTicket(create))
}

#[test]
fn a_ticket_made_from_a_sessions_message_is_marked_in_the_session() {
    // "Create ticket" in a (design) session: the session gets ticket_created at the
    // message, so the client stops offering the button for it.
    let s = setup();
    let (conversation, _) = started(exec_as(&s, Some(s.user), start(s.repo, s.orion, false)).unwrap());
    let list = |s: &Setup| {
        let st = s.service.store().clone();
        s.service.store().read(move |c| aurelius_storage::conversations::list_messages(c, conversation, &|k| st.blobs().read(k).ok().and_then(|b| String::from_utf8(b).ok()))).unwrap()
    };
    let message = list(&s).first().expect("the session's first message").id;
    let refused = ticket_from(&s, s.plain, message).unwrap_err();
    assert!(refused.message.contains("another project"), "{}", refused.message);

    let CommandResult::TicketCreated { ticket_id, number } = ticket_from(&s, s.repo, message).unwrap() else { panic!("no ticket") };
    let marks: Vec<_> = list(&s).into_iter().filter(|m| m.system.as_ref().is_some_and(|l| l.kind == aurelius_domain::views::SystemLineKind::TicketCreated)).collect();
    assert_eq!(marks.len(), 1);
    let line = marks[0].system.clone().unwrap();
    assert_eq!((line.ticket_id, line.message_id), (Some(ticket_id), Some(message)));
    assert_eq!(marks[0].text, format!("grace created #{number} from a message"));
}
