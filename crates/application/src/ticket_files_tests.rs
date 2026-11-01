//! Files added to a ticket whose main task already works (`ticket.update {
//! add_files }`): they reach its employee through its conversation (AUR-T3-F2).

use super::*;
use aurelius_domain::employee::CreateEmployee;
use aurelius_domain::project::CreateProject;
use aurelius_domain::task::TaskStatus;
use aurelius_domain::ticket;
use aurelius_domain::views::MessageView;
use aurelius_domain::{ConversationId, EmployeeId, RunId, TaskId, TicketId, UserId};

const PHC: &str = "$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHQ$aGFzaGhhc2hoYXNoaGFzaA";

struct Setup {
    _dir: tempfile::TempDir,
    service: CommandService,
    project: ProjectId,
    user: UserId,
    ann: EmployeeId,
    /// The project's work folder, where the ticket keeps its files.
    folder: std::path::PathBuf,
    /// A pasted file outside it, to attach.
    shot: String,
}

fn ctx(user: Option<UserId>) -> CommandContext {
    CommandContext { principal_id: PrincipalId::new(), client_id: ClientId::new(), origin: Origin::LocalIpc, request_id: 1, app: None, user_id: user }
}

fn envelope(command: Command) -> CommandEnvelope {
    CommandEnvelope { command_id: CommandId::new(), expected_version: None, command }
}

fn exec(s: &Setup, command: Command) -> Result<CommandResult, AppError> {
    s.service.execute(&ctx(Some(s.user)), envelope(command)).map(|c| c.receipt.result)
}

fn setup() -> Setup {
    let dir = tempfile::tempdir().unwrap();
    let service = CommandService::new(Arc::new(Store::open(dir.path()).unwrap()));
    let user = UserId::new();
    service
        .store()
        .write(move |tx| aurelius_storage::users::insert(tx, &aurelius_storage::users::NewUser { id: &user.to_string(), username: "grace", created_by_user: None, now_ms: 1 }, PHC))
        .unwrap();
    let made = service.execute(&ctx(Some(user)), envelope(Command::CreateProject(CreateProject { name: "Orbit".into(), description: None, folder: None }))).unwrap().receipt.result;
    let CommandResult::ProjectCreated { project_id, .. } = made else { panic!("{made:?}") };
    let create = CreateEmployee { name: "Ann".into(), role: "Dev".into(), title: None, reports_to: None, instructions: None, folder: None };
    let made = service.execute(&ctx(Some(user)), envelope(Command::CreateEmployee(create))).unwrap().receipt.result;
    let CommandResult::EmployeeCreated { employee_id: ann, .. } = made else { panic!("{made:?}") };
    let folder = dir.path().join("work");
    std::fs::create_dir_all(&folder).unwrap();
    let path = folder.display().to_string();
    service.store().write(move |tx| Ok(tx.execute("UPDATE projects SET folder = ?2 WHERE id = ?1", [project_id.to_string(), path])?)).unwrap();
    let pasted = dir.path().join("pasted");
    std::fs::create_dir_all(&pasted).unwrap();
    let shot = pasted.join("shot.png");
    std::fs::write(&shot, b"png").unwrap();
    let shot = shot.display().to_string();
    Setup { _dir: dir, service, project: project_id, user, ann, folder, shot }
}

fn new_ticket(s: &Setup) -> TicketId {
    let create = ticket::CreateTicket {
        confirm_secret: false,
        project_id: s.project,
        title: "Login".into(),
        description: None,
        priority: Default::default(),
        labels: vec![],
        deadline_ms: None,
        owner_employee_id: Some(s.ann),
        run: Default::default(),
        files: vec![],
        start: false,
        from_message: None,
    };
    let CommandResult::TicketCreated { ticket_id, .. } = exec(s, Command::CreateTicket(create)).unwrap() else { panic!() };
    ticket_id
}

fn add_files(id: TicketId, files: &[String]) -> Command {
    Command::UpdateTicket(ticket::UpdateTicket {
        confirm_secret: false,
        ticket_id: id,
        title: None,
        description: None,
        priority: None,
        labels: None,
        deadline_ms: None,
        clear_deadline: false,
        run: None,
        add_files: Some(files.to_vec()),
    })
}

fn root_of(s: &Setup, id: TicketId) -> TaskId {
    let read = |_: &str| None;
    s.service.store().read(|c| aurelius_storage::tickets::view(c, id, &read)).unwrap().unwrap().root_task_id.expect("a main task")
}

/// Let the main task's job open its conversation, as the dispatcher does.
fn open_conversation(s: &Setup, root: TaskId) -> ConversationId {
    let epoch = s
        .service
        .store()
        .write(move |tx| {
            tx.execute("UPDATE jobs SET state = 'leased', lease_owner = 'test', lease_until_ms = ?2, lease_epoch = lease_epoch + 1 WHERE id = ?1", rusqlite::params![root.to_string(), aurelius_storage::now_ms() + 60_000])?;
            Ok(tx.query_row("SELECT lease_epoch FROM jobs WHERE id = ?1", [root.to_string()], |r| r.get::<_, i64>(0))?)
        })
        .unwrap();
    let task = s.service.task_context(root).unwrap().unwrap();
    s.service.start_task_conversation(&task, epoch).unwrap();
    conversation_of(s, root).expect("its conversation")
}

fn conversation_of(s: &Setup, root: TaskId) -> Option<ConversationId> {
    s.service.store().read(move |c| aurelius_storage::tasks::task_conversation(c, root)).unwrap().map(|(c, _)| c)
}

/// A started ticket: its main task Running in its open conversation, with the brief's run queued.
fn working(s: &Setup) -> (TicketId, TaskId, ConversationId) {
    let id = new_ticket(s);
    exec(s, Command::StartTicket(ticket::StartTicket { ticket_id: id, employee_id: None })).unwrap();
    let root = root_of(s, id);
    let conversation = open_conversation(s, root);
    assert_eq!(status(s, root), TaskStatus::Running);
    (id, root, conversation)
}

fn status(s: &Setup, task: TaskId) -> TaskStatus {
    s.service.store().read(move |c| aurelius_storage::tasks::load_task(c, task)).unwrap().unwrap().status
}

fn messages(s: &Setup, conversation: ConversationId) -> Vec<MessageView> {
    let st = s.service.store().clone();
    s.service.store().read(move |c| aurelius_storage::conversations::list_messages(c, conversation, &|k| st.blobs().read(k).ok().and_then(|b| String::from_utf8(b).ok()))).unwrap()
}

fn file_updates(s: &Setup, conversation: ConversationId) -> Vec<MessageView> {
    messages(s, conversation).into_iter().filter(|m| m.update.as_deref() == Some("files")).collect()
}

/// The runs of the conversation, oldest first, with their status.
fn runs(s: &Setup, conversation: ConversationId) -> Vec<(RunId, String)> {
    s.service
        .store()
        .read(move |c| {
            let mut stmt = c.prepare("SELECT id, status FROM runs WHERE conversation_id = ?1 ORDER BY rowid")?;
            let rows = stmt.query_map([conversation.to_string()], |r| Ok((RunId::parse(&r.get::<_, String>(0)?).unwrap(), r.get::<_, String>(1)?)))?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .unwrap()
}

fn set_runs(s: &Setup, conversation: ConversationId, status: &'static str) {
    s.service.store().write(move |tx| Ok(tx.execute("UPDATE runs SET status = ?2 WHERE conversation_id = ?1", [conversation.to_string(), status.to_string()])?)).unwrap();
}

/// What the run will read: the texts of its turns.
fn turns_of(s: &Setup, run: RunId) -> Vec<String> {
    let payload: String = s.service.store().read(move |c| Ok(c.query_row("SELECT payload_json FROM jobs WHERE id = ?1", [run.to_string()], |r| r.get(0))?)).unwrap();
    let plan = s.service.prepare_chat_run(&serde_json::from_str(&payload).unwrap()).unwrap().expect("a plan");
    plan.turns.iter().map(|t| t.text.clone()).collect()
}

/// Where the ticket keeps the copy of `shot.png` (an absolute path in its uploads).
fn kept_shot(s: &Setup, id: TicketId) -> String {
    let files = s.service.store().read(move |c| aurelius_storage::tickets::attachments_of(c, id)).unwrap().expect("the ticket's files");
    let shot = files.files.iter().find(|f| f.name == "shot.png").expect("shot.png on the ticket");
    std::path::Path::new(&files.folder).join(&shot.path).display().to_string()
}

#[test]
fn files_added_to_a_working_main_task_reach_its_next_run() {
    let s = setup();
    let (id, _, conversation) = working(&s);
    let before = runs(&s, conversation);
    exec(&s, add_files(id, std::slice::from_ref(&s.shot))).unwrap();
    let path = kept_shot(&s, id);
    assert!(path.starts_with(&s.folder.display().to_string()), "{path}");
    let after = runs(&s, conversation);
    assert_eq!(after.len(), before.len() + 1, "a run answers the files: {after:?}");
    let (next, state) = after.last().unwrap().clone();
    assert_eq!(state, "created");
    let turns = turns_of(&s, next);
    assert!(turns.iter().any(|t| t.contains(&path) && t.contains("(3 bytes)")), "the next run reads where the file is: {turns:?}");
}

#[test]
fn a_live_run_is_not_interrupted_and_the_run_after_it_reads_the_files() {
    let s = setup();
    let (id, _, conversation) = working(&s);
    set_runs(&s, conversation, "running");
    let (live, _) = runs(&s, conversation)[0];
    exec(&s, add_files(id, std::slice::from_ref(&s.shot))).unwrap();
    let after = runs(&s, conversation);
    // A stop would make it `cancelling`.
    assert_eq!(after[0], (live, "running".to_string()), "the live run goes on: {after:?}");
    assert_eq!(after.len(), 2);
    assert!(turns_of(&s, after[1].0).iter().any(|t| t.contains(&kept_shot(&s, id))));
}

#[test]
fn a_running_main_task_without_a_run_gets_one_and_a_waiting_one_only_the_message() {
    let s = setup();
    let (id, root, conversation) = working(&s);
    set_runs(&s, conversation, "succeeded");
    exec(&s, add_files(id, std::slice::from_ref(&s.shot))).unwrap();
    let queued: Vec<_> = runs(&s, conversation).into_iter().filter(|(_, st)| st == "created").collect();
    assert_eq!(queued.len(), 1, "one run for the files");
    let counted: Option<String> = s.service.store().read(move |c| Ok(c.query_row("SELECT task_id FROM runs WHERE id = ?1", [queued[0].0.to_string()], |r| r.get(0))?)).unwrap();
    assert_eq!(counted, Some(root.to_string()), "the run counts for the main task");

    // Waiting (here for a plan's decision): the message stays for its next run, nothing runs.
    set_runs(&s, conversation, "succeeded");
    s.service.store().write(move |tx| crate::task::task_line::set_status(tx, root, TaskStatus::Waiting, Some(aurelius_domain::task::WAITING_PLAN_REVIEW)).map(|_| ())).unwrap();
    exec(&s, add_files(id, std::slice::from_ref(&s.shot))).unwrap();
    assert!(runs(&s, conversation).iter().all(|(_, st)| st == "succeeded"), "a waiting task is not woken");
    assert_eq!(file_updates(&s, conversation).len(), 2, "but it has the message");
    assert_eq!(status(&s, root), TaskStatus::Waiting);
}

#[test]
fn a_main_task_without_a_conversation_gets_the_files_through_its_brief_only() {
    let s = setup();
    let id = new_ticket(&s);
    exec(&s, Command::StartTicket(ticket::StartTicket { ticket_id: id, employee_id: None })).unwrap();
    let root = root_of(&s, id);
    assert_eq!(conversation_of(&s, root), None);
    exec(&s, add_files(id, std::slice::from_ref(&s.shot))).unwrap();
    assert_eq!(conversation_of(&s, root), None, "nothing opens its conversation");
    let conversation = open_conversation(&s, root);
    assert!(file_updates(&s, conversation).is_empty(), "no extra message");
    let path = kept_shot(&s, id);
    let briefs: Vec<_> = messages(&s, conversation).into_iter().filter(|m| m.update.as_deref() == Some("brief")).collect();
    assert_eq!(briefs.len(), 1);
    assert!(briefs[0].text.contains(&path), "the brief names it: {}", briefs[0].text);
}

#[test]
fn a_finished_main_task_is_not_told_and_keeps_its_own_files() {
    let s = setup();
    let (id, root, conversation) = working(&s);
    set_runs(&s, conversation, "succeeded");
    s.service.store().write(move |tx| aurelius_storage::tasks::set_task_status(tx, root, TaskStatus::Succeeded, None)).unwrap();
    let (count, runs_before) = (messages(&s, conversation).len(), runs(&s, conversation));
    exec(&s, add_files(id, std::slice::from_ref(&s.shot))).unwrap();
    assert_eq!((messages(&s, conversation).len(), runs(&s, conversation)), (count, runs_before));
    assert!(s.service.store().read(move |c| aurelius_storage::tasks::attachments_of(c, root)).unwrap().is_none());
}

#[test]
fn a_main_task_whose_files_are_elsewhere_keeps_them_and_is_still_told_of_the_new_ones() {
    let s = setup();
    let (id, root, conversation) = working(&s);
    let elsewhere = aurelius_storage::tasks::TaskAttachments {
        folder: s._dir.path().join("other").display().to_string(),
        files: vec![aurelius_domain::views::MessageFileView { id: None, name: "old.txt".into(), path: "old.txt".into(), byte_length: 1 }],
    };
    let set = elsewhere.clone();
    s.service.store().write(move |tx| aurelius_storage::tasks::set_attachments(tx, root, Some(&set))).unwrap();
    exec(&s, add_files(id, std::slice::from_ref(&s.shot))).unwrap();
    let kept = s.service.store().read(move |c| aurelius_storage::tasks::attachments_of(c, root)).unwrap();
    assert_eq!(kept, Some(elsewhere), "its row keeps its own files, relative to its own folder");
    let told = file_updates(&s, conversation);
    assert_eq!(told.len(), 1);
    assert!(told[0].text.contains(&kept_shot(&s, id)), "the update names the new file where it is: {}", told[0].text);
}

#[test]
fn a_replayed_update_tells_the_main_task_once() {
    let s = setup();
    let (id, _, conversation) = working(&s);
    let runs_before = runs(&s, conversation).len();
    let once = envelope(add_files(id, std::slice::from_ref(&s.shot)));
    let first = s.service.execute(&ctx(Some(s.user)), once.clone()).unwrap();
    let again = s.service.execute(&ctx(Some(s.user)), once).unwrap();
    assert!(!first.receipt.replayed && again.receipt.replayed);
    assert_eq!(file_updates(&s, conversation).len(), 1, "one message");
    assert_eq!(runs(&s, conversation).len(), runs_before + 1, "one run");
}
