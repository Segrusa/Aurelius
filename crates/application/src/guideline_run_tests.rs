//! The guidelines a run gets (B3, impactanalyse §3): the block right after the
//! identity and the instructions, the set per kind of run, the manifest items,
//! the safety net (B4), the redactor (E6, SEC0 T18), the session's version set
//! (B6) and the snapshot with the board members' notice.

use super::*;
use aurelius_domain::employee::CreateEmployee;
use aurelius_domain::group_chat::SendGroupMessage;
use aurelius_domain::guideline::*;
use aurelius_domain::project::CreateProject;
use aurelius_domain::project_conversation::StartProjectConversation;
use aurelius_domain::task::{CreateTask, TaskStatus};
use aurelius_domain::views::SystemLineKind;
use aurelius_domain::{ConversationId, EmployeeId, GuidelineId, MessageId, RunId, UserId};

const PHC: &str = "$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHQ$aGFzaGhhc2hoYXNoaGFzaA";

struct Setup {
    _dir: tempfile::TempDir,
    service: CommandService,
    project: ProjectId,
    user: UserId,
    ann: EmployeeId,
    bo: EmployeeId,
}

fn ctx(user: Option<UserId>) -> CommandContext {
    CommandContext { principal_id: PrincipalId::new(), client_id: ClientId::new(), origin: Origin::LocalIpc, request_id: 1, user_id: user }
}

fn exec(s: &Setup, command: Command) -> CommandResult {
    s.service.execute(&ctx(Some(s.user)), CommandEnvelope { command_id: CommandId::new(), expected_version: None, command }).map(|c| c.receipt.result).unwrap()
}

fn hire(service: &CommandService, user: UserId, name: &str) -> EmployeeId {
    let create = CreateEmployee { access_level: None, tool_policy: None, start_profile: None, description: None, name: name.into(), role: "Dev".into(), title: None, reports_to: None, instructions: Some(format!("Focus of {name}.")), folder: None };
    let made = service.execute(&ctx(Some(user)), CommandEnvelope { command_id: CommandId::new(), expected_version: None, command: Command::CreateEmployee(create) }).unwrap().receipt.result;
    let CommandResult::EmployeeCreated { employee_id, .. } = made else { panic!("{made:?}") };
    employee_id
}

fn setup() -> Setup {
    let dir = tempfile::tempdir().unwrap();
    let service = CommandService::new(Arc::new(Store::open(dir.path()).unwrap()));
    let user = UserId::new();
    service
        .store()
        .write(move |tx| aurelius_storage::users::insert(tx, &aurelius_storage::users::NewUser { id: &user.to_string(), username: "grace", created_by_user: None, now_ms: 1 }, PHC))
        .unwrap();
    let made = service
        .execute(&ctx(Some(user)), CommandEnvelope { command_id: CommandId::new(), expected_version: None, command: Command::CreateProject(CreateProject { name: "Orbit".into(), description: None, folder: None }) })
        .unwrap()
        .receipt
        .result;
    let CommandResult::ProjectCreated { project_id, .. } = made else { panic!("{made:?}") };
    let ann = hire(&service, user, "Ann");
    let bo = hire(&service, user, "Bo");
    Setup { _dir: dir, service, project: project_id, user, ann, bo }
}

fn guideline(s: &Setup, title: &str, text: &str, links: Vec<GuidelineTarget>) -> GuidelineId {
    let CommandResult::GuidelineChanged { guideline_id, .. } = exec(s, Command::CreateGuideline(CreateGuideline { title: title.into(), text: text.into(), links })) else { panic!() };
    guideline_id
}

fn chat_of(s: &Setup, employee: EmployeeId) -> ConversationId {
    s.service.store().read(move |c| aurelius_storage::employees::main_chat(c, employee)).unwrap().unwrap()
}

/// A queued run in `conversation`.
fn queued(s: &Setup, conversation: ConversationId) -> RunId {
    s.service.store().write(move |tx| Ok(crate::run::queue::queue_run(tx, &serde_json::json!({}), "test", conversation, MessageId::new())?.0)).unwrap()
}

/// A running task of `employee` in the project, with a run in its conversation.
fn task_run(s: &Setup, employee: EmployeeId) -> RunId {
    let create = CreateTask { confirm_secret: false, project_id: s.project, employee_id: Some(employee), title: "Export".into(), goal: "Do it".into(), acceptance_criteria: vec![], run: Default::default(), files: vec![] };
    let CommandResult::TaskCreated { task_id } = exec(s, Command::CreateTask(create)) else { panic!() };
    let st = s.service.store().clone();
    s.service
        .store()
        .write(move |tx| {
            let (conversation, brief, _) = crate::task::task_threads::open_returning(tx, &st, task_id, employee, "brief")?;
            let _ = crate::task::task_line::set_status(tx, task_id, TaskStatus::Running, None)?;
            Ok(crate::run::queue::queue_run(tx, &serde_json::json!({}), &task_id.to_string(), conversation.unwrap(), brief.unwrap())?.0)
        })
        .unwrap()
}

fn plan(s: &Setup, run: RunId) -> crate::run::ChatRunPlan {
    let payload: String = s.service.store().read(move |c| Ok(c.query_row("SELECT payload_json FROM jobs WHERE id = ?1", [run.to_string()], |r| r.get(0))?)).unwrap();
    s.service.prepare_chat_run(&serde_json::from_str(&payload).unwrap()).unwrap().expect("a plan")
}

fn titles_in(system: &str) -> Vec<String> {
    let quoted = system.split_once(BLOCK_PREAMBLE).map(|(_, rest)| rest).unwrap_or("");
    quoted.lines().filter_map(|l| l.strip_prefix("### ")).map(str::to_owned).collect()
}

fn items(plan: &crate::run::ChatRunPlan) -> Vec<(String, bool, String)> {
    plan.manifest.as_ref().unwrap().items.iter().filter(|i| i.section == "guideline").map(|i| (i.id.clone(), i.included, i.reason.clone())).collect()
}

/// Every kind of run gets its set, in the place D-GL-01 gives it.
#[test]
fn the_block_comes_after_the_identity_with_the_set_of_the_run() {
    let s = setup();
    let own = guideline(&s, "Own rule", "Ann's way.", vec![GuidelineTarget::Employee(s.ann)]);
    let project = guideline(&s, "Project rule", "Orbit's way.", vec![GuidelineTarget::Project(s.project)]);
    let _other = guideline(&s, "Bo's rule", "Not for Ann.", vec![GuidelineTarget::Employee(s.bo)]);

    // Its own chat without a project: its own guidelines only.
    let chat = plan(&s, queued(&s, chat_of(&s, s.ann)));
    let system = chat.system.clone().unwrap();
    assert!(system.starts_with(crate::run::plan::PLATFORM_RULES), "the platform rules come first: {system}");
    assert!(system.contains("\n\nYou are Ann, Dev. Focus of Ann.\n\n## Guidelines\n"), "right after the identity and the instructions: {system}");
    assert_eq!(titles_in(&system), vec!["Own rule"]);
    assert_eq!(items(&chat), vec![(format!("g:{own}@1"), true, "required".to_string())]);
    assert_eq!(chat.session.context.guidelines, vec![format!("{own}@1")]);
    assert_eq!(chat.guidelines.guidelines.iter().map(|g| g.id).collect::<Vec<_>>(), vec![own]);

    // A task in the project: its own, then the project's; before the task's own section.
    let task = plan(&s, task_run(&s, s.ann));
    let system = task.system.clone().unwrap();
    assert!(system.contains("\n\nYou are Ann, Dev. Focus of Ann.\n\n## Guidelines\n"), "{system}");
    assert_eq!(titles_in(&system), vec!["Own rule", "Project rule"]);
    assert!(system.contains(&format!("<guideline id=\"g:{project}\" version=\"1\" level=\"project\" project=\"Orbit\">")), "{system}");
    assert!(system.find("## Guidelines").unwrap() < system.find("## This conversation is about your task").unwrap());
    assert!(system.find("</guideline>\n\n").is_some(), "a blank line after the block: {system}");

    // Woken in the project's channel: the project is the channel's.
    let channel = s.service.store().read(|c| aurelius_storage::discussions::channel(c, Some(s.project))).unwrap().unwrap();
    let CommandResult::GroupMessageSent { run_ids, .. } = exec(&s, Command::SendGroupMessage(SendGroupMessage { confirm_secret: false, text: "@Ann how is it going?".into(), images: vec![], files: vec![], channel: Some(channel), thread_parent: None })) else { panic!() };
    let woken = plan(&s, run_ids[0]);
    assert_eq!(titles_in(woken.system.as_deref().unwrap()), vec!["Own rule", "Project rule"]);

    // The project assistant: the project's only.
    let CommandResult::ProjectConversationStarted { conversation_id, .. } = exec(&s, Command::StartProjectConversation(StartProjectConversation { project_id: s.project, start: Default::default() })) else { panic!() };
    let assistant = plan(&s, queued(&s, conversation_id));
    let system = assistant.system.clone().unwrap();
    assert_eq!(titles_in(&system), vec!["Project rule"]);
    let block = system.find("## Guidelines").unwrap();
    assert!(system[..block].contains("Orbit"), "after the assistant's own text: {system}");

    // An employee without guidelines gets no block at all.
    let lone = hire(&s.service, s.user, "Cy");
    let none = plan(&s, queued(&s, chat_of(&s, lone)));
    assert!(!none.system.clone().unwrap().contains("## Guidelines"));
    assert!(items(&none).is_empty() && none.session.context.guidelines.is_empty());
}

/// The safety net (B4) at run start: what is left out is named in the block and
/// in the manifest, and the board members are told once until it changes.
#[test]
fn the_safety_net_leaves_out_and_tells_the_board_members() {
    let s = setup();
    let a = guideline(&s, "Alpha", &"a".repeat(7_000), vec![GuidelineTarget::Employee(s.ann)]);
    let b = guideline(&s, "Beta", &"b".repeat(7_000), vec![GuidelineTarget::Project(s.project)]);
    let c = guideline(&s, "Gamma", &"c".repeat(3_000), vec![]);
    // Over the limit straight in the database (as after a project came back from the archive).
    s.service
        .store()
        .write(move |tx| Ok(tx.execute("INSERT INTO guideline_links(guideline_id, employee_id, created_at_ms) VALUES (?1, ?2, 1)", [c.to_string(), s_ann(tx)?])?))
        .unwrap();
    let run = task_run(&s, s.ann);
    let p = plan(&s, run);
    let system = p.system.clone().unwrap();
    // Employee ones go first, from the back: Gamma.
    assert_eq!(titles_in(&system), vec!["Alpha", "Beta"]);
    assert!(system.contains("\nLeft out because this block was over its limit: Gamma.\n"), "{system}");
    assert_eq!(
        items(&p),
        vec![(format!("g:{a}@1"), true, "required".into()), (format!("g:{b}@1"), true, "required".into()), (format!("g:{c}@1"), false, "over the guideline limit".into())]
    );

    let conversation = p.conversation_id;
    let record = |set: crate::run::guidelines::RunSet| s.service.store().write(move |tx| crate::run::guidelines::record(tx, run, conversation, &set)).unwrap();
    let lines = |s: &Setup| -> Vec<String> {
        let st = s.service.store().clone();
        s.service
            .store()
            .read(move |c| aurelius_storage::conversations::list_messages(c, conversation, &|k| st.blobs().read(k).ok().and_then(|b| String::from_utf8(b).ok())))
            .unwrap()
            .into_iter()
            .filter(|m| m.system.as_ref().is_some_and(|l| l.kind == SystemLineKind::Limit))
            .map(|m| m.text)
            .collect()
    };
    assert_eq!(record(p.guidelines.clone()).len(), 1);
    assert_eq!(record(p.guidelines.clone()).len(), 0, "told once");
    let told = lines(&s);
    assert_eq!(told.len(), 1);
    assert!(told[0].contains("“Gamma”") && told[0].contains("16000"), "{told:?}");
    // The snapshot is what the block holds.
    let snapshot = s.service.store().read(move |c| aurelius_storage::guidelines::for_run(c, run)).unwrap();
    assert_eq!(snapshot.iter().map(|g| (g.guideline_id, g.version)).collect::<Vec<_>>(), vec![(a, 1), (b, 1)]);
    // Back under the limit: nothing to tell; over again: told again.
    assert_eq!(record(crate::run::guidelines::RunSet::default()).len(), 0);
    assert_eq!(record(p.guidelines.clone()).len(), 1);
}

fn s_ann(tx: &aurelius_storage::Tx<'_>) -> Result<String, aurelius_storage::StorageError> {
    Ok(tx.query_row("SELECT id FROM employees WHERE name = 'Ann'", [], |r| r.get(0))?)
}

/// T18 (E6): a value that became a vault secret after the guideline was written
/// reaches no system text; the manifest says "redacted" and the board members are told, by name.
#[test]
fn t18_the_redactor_is_the_safety_net_of_the_block() {
    let s = setup();
    let value = format!("ghp_{}", CommandId::new().0.simple());
    let held = guideline(&s, "Deploys", &format!("Deploy with the token {value} on Fridays."), vec![GuidelineTarget::Employee(s.ann)]);
    let owner = format!("guideline-run-test-{}", CommandId::new());
    let secret = aurelius_runtime::redact::Secret { name: "GH_TOKEN".into(), label: "GitHub token".into(), value: zeroize::Zeroizing::new(value.clone()) };
    aurelius_runtime::redact::install_set(&owner, vec![secret]);

    let run = queued(&s, chat_of(&s, s.ann));
    let p = plan(&s, run);
    aurelius_runtime::redact::install_set(&owner, Vec::new());
    let system = p.system.clone().unwrap();
    assert!(!system.contains(&value) && system.contains("Deploy with the token «secret:GH_TOKEN» on Fridays."), "{system}");
    assert_eq!(items(&p), vec![(format!("g:{held}@1"), true, "redacted".to_string())]);
    assert_eq!(p.session.context.guidelines, vec![format!("{held}@1#redacted")], "a session that had the value is not resumed");
    let notice = p.guidelines.notice().unwrap();
    assert!(notice.contains("GitHub token") && notice.contains("“Deploys”") && !notice.contains(&value), "{notice}");
    assert!(!format!("{:?}", p.manifest).contains(&value));
}

/// B6: the session keeps the version set; a new version changes it.
#[test]
fn b6_the_version_set_follows_the_versions() {
    let s = setup();
    let g = guideline(&s, "Rule", "One.", vec![GuidelineTarget::Employee(s.ann)]);
    let before = plan(&s, queued(&s, chat_of(&s, s.ann))).session.context.guidelines;
    exec(&s, Command::UpdateGuideline(UpdateGuideline { guideline_id: g, title: None, text: Some("Two.".into()) }));
    let after = plan(&s, queued(&s, chat_of(&s, s.ann))).session.context.guidelines;
    assert_eq!((before, after), (vec![format!("{g}@1")], vec![format!("{g}@2")]));
    // The stored context of an older session (without the field) reads as an empty set.
    let old: crate::run::session::SessionContext = serde_json::from_str("{\"mode\":\"ask\"}").unwrap();
    assert!(old.guidelines.is_empty());
}
