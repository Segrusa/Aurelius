//! What an outside app may call (feature `app_keys`). A session that
//! authenticated with an app key (`aurelius-mcp`, over local IPC or the
//! gateway) passes `required` before every request instead of the device
//! rights of `remote`: an allow-list of the methods its tools use, each with
//! the scopes it needs. Everything else is refused.
//!
//! * `read`: projects, employees, tasks and their conversations, project
//!   conversations, the inbox (notifications and approvals), ideas, memory.
//! * `draft`: a task without an employee (a draft), an idea, a memory.
//! * `run`: a task with an employee (it starts), a message to a task, cancelling
//!   a task, the project assistant, a mode other than Auto.
//! * `approve`: Auto (for a new task or a conversation), answering approvals.
//!
//! An app never passes files or images (paths on the server) or a connection,
//! and never answers with "always allow" (a server-wide setting). What a call
//! points at is checked by the daemon: `conversation.get` only reads a
//! conversation of a project, never one of the user's own chats.

use crate::method::Method;
use crate::rpc::Call;
use crate::write::WriteMethod;
use aurelius_domain::Command;
use aurelius_domain::app_key::{AppAudience, AppScope};
use aurelius_domain::autonomy::AgentMode;

/// Why an app may not make a call.
pub const NOT_FOR_APPS: &str = "Apps cannot use this.";

/// The scopes an app needs for `call` (all of them), or why it may not make it.
pub fn required(call: &Call) -> Result<Vec<AppScope>, &'static str> {
    use AppScope::*;
    match call {
        Call::Read(method) => match method {
            Method::SessionHello(_) | Method::ServerGetHealth => Ok(vec![]),
            Method::ProjectList
            | Method::EmployeeList { .. }
            | Method::DepartmentList { .. }
            | Method::CompanyGet
            | Method::TaskList { .. }
            | Method::ConversationGet { .. }
            | Method::ProjectConversationList { .. }
            | Method::ApprovalList
            | Method::NotificationList
            | Method::MemoryList { .. }
            | Method::MemoryGetContext { .. }
            | Method::IdeaList { .. } | Method::IdeaListAll {} => Ok(vec![Read]),
            _ => Err(NOT_FOR_APPS),
        },
        Call::Write(WriteMethod::Domain(command)) => command_needs(command),
        Call::Write(WriteMethod::Host(_)) => Err(NOT_FOR_APPS),
    }
}

fn command_needs(command: &Command) -> Result<Vec<AppScope>, &'static str> {
    use AppScope::*;
    const NO_FILES: &str = "Apps cannot attach files or images.";
    let auto = |mode: Option<AgentMode>, mut scopes: Vec<AppScope>| {
        if mode == Some(AgentMode::Auto) {
            scopes.push(Approve);
        }
        scopes
    };
    match command {
        Command::CreateTask(t) => {
            if !t.files.is_empty() {
                return Err(NO_FILES);
            }
            if t.run.connection_id.is_some() {
                return Err("Apps cannot choose the connection; leave it to the employee.");
            }
            // Without an employee the task stays a draft; with one it starts.
            let base = if t.employee_id.is_some() { Run } else { Draft };
            Ok(auto(t.run.mode, vec![base]))
        }
        Command::SendEmployeeMessage(m) if !m.images.is_empty() || !m.files.is_empty() => Err(NO_FILES),
        Command::SendEmployeeMessage(_) | Command::CancelTask(_) => Ok(vec![Run]),
        Command::StartProjectConversation(s) => Ok(if s.start.needs_approve() { vec![Run, Approve] } else { vec![Run] }),
        Command::SendProjectMessage(m) if !m.images.is_empty() || !m.files.is_empty() => Err(NO_FILES),
        Command::SendProjectMessage(m) => Ok(if m.conversation_id.is_none() && m.start.needs_approve() { vec![Run, Approve] } else { vec![Run] }),
        Command::AddIdea(i) if !i.files.is_empty() => Err(NO_FILES),
        Command::AddIdea(_) | Command::Remember(_) => Ok(vec![Draft]),
        Command::SetAgentMode(m) => Ok(auto(Some(m.mode), if m.mode == AgentMode::Auto { vec![] } else { vec![Run] })),
        Command::DecideApproval(d) if d.always_allow => Err("Apps cannot choose 'always allow'; the user decides that in Aurelius."),
        Command::DecideApproval(_) | Command::AnswerQuestion(_) | Command::DecidePlan(_) => Ok(vec![Approve]),
        _ => Err(NOT_FOR_APPS),
    }
}

/// `Ok` when `held` covers everything `call` needs, else the reason to refuse.
pub fn allowed(held: &[AppScope], call: &Call) -> Result<(), String> {
    let needed = required(call).map_err(str::to_owned)?;
    match needed.iter().find(|s| !aurelius_domain::app_key::grants(held, **s)) {
        Some(missing) => Err(format!("This app key does not have the {} scope.", missing.as_str())),
        None => Ok(()),
    }
}

/// As `allowed`, for a key with `audience`. A management app (feature
/// `management_channel`) answers a manager's question with `run` instead of
/// `approve` (an answer is not an approval of an action), and may not see the
/// org chart or talk to a project's assistant. What it reaches among the
/// employees depends on the company, so the daemon checks that per call.
pub fn allowed_for(held: &[AppScope], audience: AppAudience, call: &Call) -> Result<(), String> {
    if audience == AppAudience::All {
        return allowed(held, call);
    }
    let missing = |needed: &[AppScope]| match needed.iter().find(|s| !aurelius_domain::app_key::grants(held, **s)) {
        Some(missing) => Err(format!("This app key does not have the {} scope.", missing.as_str())),
        None => Ok(()),
    };
    match call {
        Call::Read(Method::DepartmentList { .. } | Method::CompanyGet) => Err("This app key reaches the management only; it cannot read the org chart.".into()),
        Call::Write(WriteMethod::Domain(Command::StartProjectConversation(_) | Command::SendProjectMessage(_))) => Err(aurelius_domain::app_key::NO_PROJECT_ASSISTANT.into()),
        Call::Write(WriteMethod::Domain(Command::AnswerQuestion(_))) => missing(&[AppScope::Run]),
        _ => allowed(held, call),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::write::HostWrite;
    use aurelius_domain::{
        ApprovalId, ConversationId, EmployeeId, ProjectId, TaskId,
        approval::{DecideApproval, DecidePlan, PlanDecision},
        autonomy::SetAgentMode,
        conversation::ChatStart,
        idea::AddIdea,
        employee::SendEmployeeMessage,
        project::CreateProject,
        project_conversation::SendProjectMessage,
        task::{CancelTask, CreateTask, RunSettings},
    };

    fn exec(command: Command) -> Call {
        Call::Write(WriteMethod::Domain(command))
    }

    fn task(employee: bool, mode: Option<AgentMode>) -> Call {
        exec(Command::CreateTask(CreateTask {
            project_id: ProjectId::new(),
            employee_id: employee.then(EmployeeId::new),
            title: "t".into(),
            goal: "g".into(),
            acceptance_criteria: vec![],
            depends_on: vec![],
            run: RunSettings { mode, ..Default::default() },
            files: vec![],
        }))
    }

    fn may(held: &[AppScope], call: &Call) -> bool {
        allowed(held, call).is_ok()
    }

    #[test]
    fn read_covers_the_reads_the_tools_use_and_nothing_else() {
        let r = [AppScope::Read];
        for method in [Method::ProjectList, Method::TaskList { project_id: None, employee_id: None, department_id: None, state: None }, Method::EmployeeList { department_id: None }, Method::CompanyGet, Method::NotificationList, Method::ApprovalList, Method::MemoryList { project_id: None, employee_id: None, department_id: None }] {
            assert!(may(&r, &Call::Read(method.clone())), "{method:?}");
            assert!(!may(&[AppScope::Run, AppScope::Approve], &Call::Read(method.clone())), "{method:?} needs read");
        }
        // Terminals, settings and server management are never for apps.
        for method in [Method::ApiKeyList, Method::AppKeyList, Method::McpList, Method::FsListDir { path: None }, Method::EventSubscribe { after_seq: 0 }] {
            assert!(!may(&AppScope::ALL, &Call::Read(method.clone())), "{method:?}");
        }
        assert!(may(&[], &Call::Read(Method::ServerGetHealth)));
    }

    #[test]
    fn draft_run_and_approve_split_task_creation() {
        assert!(may(&[AppScope::Draft], &task(false, None)));
        assert!(!may(&[AppScope::Draft], &task(true, None)), "starting needs run");
        assert!(may(&[AppScope::Run], &task(true, None)));
        assert!(may(&[AppScope::Run], &task(false, None)), "run includes draft");
        assert!(may(&[AppScope::Run], &task(true, Some(AgentMode::AcceptEdits))));
        assert!(!may(&[AppScope::Run], &task(true, Some(AgentMode::Auto))), "Auto needs approve");
        assert!(may(&[AppScope::Run, AppScope::Approve], &task(true, Some(AgentMode::Auto))));
        assert!(!may(&[AppScope::Read], &task(false, None)));
        let err = allowed(&[AppScope::Draft], &task(true, None)).unwrap_err();
        assert!(err.contains("run"), "{err}");
    }

    #[test]
    fn running_work_and_approvals() {
        let message = exec(Command::SendEmployeeMessage(SendEmployeeMessage {
            employee_id: EmployeeId::new(),
            text: "hi".into(),
            conversation_id: ConversationId::new(),
            images: vec![],
            files: vec![],
            interrupt: false,
        }));
        let cancel = exec(Command::CancelTask(CancelTask { task_id: TaskId::new() }));
        let assistant = exec(Command::SendProjectMessage(SendProjectMessage {
            project_id: ProjectId::new(),
            text: "hi".into(),
            conversation_id: None,
            images: vec![],
            files: vec![],
            interrupt: false,
            start: ChatStart::default(),
        }));
        for call in [&message, &cancel, &assistant] {
            assert!(may(&[AppScope::Run], call) && !may(&[AppScope::Draft, AppScope::Read, AppScope::Approve], call));
        }
        let auto = exec(Command::SetAgentMode(SetAgentMode { conversation_id: ConversationId::new(), mode: AgentMode::Auto }));
        let ask = exec(Command::SetAgentMode(SetAgentMode { conversation_id: ConversationId::new(), mode: AgentMode::Ask }));
        assert!(may(&[AppScope::Approve], &auto) && !may(&[AppScope::Run], &auto));
        assert!(may(&[AppScope::Run], &ask));
        let decide = exec(Command::DecideApproval(DecideApproval { approval_id: ApprovalId::new(), action_digest: "d".into(), approve: true, always_allow: false }));
        let plan = exec(Command::DecidePlan(DecidePlan { approval_id: ApprovalId::new(), decision: PlanDecision::Approve, feedback: None }));
        for call in [&decide, &plan] {
            assert!(may(&[AppScope::Approve], call) && !may(&[AppScope::Run, AppScope::Read, AppScope::Draft], call));
        }
        let always = exec(Command::DecideApproval(DecideApproval { approval_id: ApprovalId::new(), action_digest: "d".into(), approve: true, always_allow: true }));
        assert!(!may(&AppScope::ALL, &always));
        let idea = exec(Command::AddIdea(AddIdea { project_id: ProjectId::new(), title: "i".into(), note: None, kind: Default::default(), files: vec![] }));
        assert!(may(&[AppScope::Draft], &idea) && !may(&[AppScope::Read], &idea));
        let with_file = exec(Command::AddIdea(AddIdea { project_id: ProjectId::new(), title: "i".into(), note: None, kind: Default::default(), files: vec!["C:/x.txt".into()] }));
        assert!(!may(&AppScope::ALL, &with_file), "apps never pass server files");
    }


    #[test]
    fn a_management_app_answers_with_run_and_cannot_use_the_project_assistant() {
        use aurelius_domain::approval::AnswerQuestion;
        let answer = exec(Command::AnswerQuestion(AnswerQuestion { approval_id: ApprovalId::new(), answers: Default::default() }));
        let for_all = |held: &[AppScope]| allowed_for(held, AppAudience::All, &answer).is_ok();
        let for_management = |held: &[AppScope]| allowed_for(held, AppAudience::Management, &answer).is_ok();
        assert!(for_all(&[AppScope::Approve]) && !for_all(&[AppScope::Run]), "unchanged for other keys");
        assert!(for_management(&[AppScope::Run]) && !for_management(&[AppScope::Read, AppScope::Draft]));
        let assistant = exec(Command::SendProjectMessage(SendProjectMessage {
            project_id: ProjectId::new(),
            text: "hi".into(),
            conversation_id: None,
            images: vec![],
            files: vec![],
            interrupt: false,
            start: ChatStart::default(),
        }));
        assert!(allowed_for(&[AppScope::Run], AppAudience::All, &assistant).is_ok());
        let refused = allowed_for(&[AppScope::Run], AppAudience::Management, &assistant).unwrap_err();
        assert!(refused.contains("management"), "{refused}");
        assert!(allowed_for(&AppScope::ALL, AppAudience::Management, &Call::Read(Method::CompanyGet)).is_err());
        assert!(allowed_for(&AppScope::ALL, AppAudience::All, &Call::Read(Method::CompanyGet)).is_ok());
        assert!(allowed_for(&[AppScope::Read], AppAudience::Management, &Call::Read(Method::EmployeeList { department_id: None })).is_ok());
    }

    #[test]
    fn apps_never_pass_files_or_manage_the_server() {
        let with_file = exec(Command::SendEmployeeMessage(SendEmployeeMessage {
            employee_id: EmployeeId::new(),
            text: "hi".into(),
            conversation_id: ConversationId::new(),
            images: vec![],
            files: vec!["C:/secret.txt".into()],
            interrupt: false,
        }));
        assert!(!may(&AppScope::ALL, &with_file));
        let Call::Write(WriteMethod::Domain(Command::CreateTask(mut t))) = task(true, None) else { unreachable!() };
        t.files = vec!["C:/secret.txt".into()];
        assert!(!may(&AppScope::ALL, &exec(Command::CreateTask(t.clone()))));
        t.files.clear();
        t.run.connection_id = Some(aurelius_domain::ProviderId::new());
        assert!(!may(&AppScope::ALL, &exec(Command::CreateTask(t))));
        assert!(!may(&AppScope::ALL, &exec(Command::CreateProject(CreateProject { name: "x".into(), description: None, folder: None }))));
        assert!(!may(&AppScope::ALL, &Call::Write(WriteMethod::Host(HostWrite::NotificationDismissAll))));
        assert!(!may(&AppScope::ALL, &Call::Write(WriteMethod::Host(HostWrite::AppKeyRevoke { key_id: aurelius_domain::AppKeyId::new() }))));
    }
}
