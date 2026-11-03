//! Ideas per project: noted by the user or an employee, made into a ticket or started as a task, or dropped.

mod common;

use aurelius_client::bridge::BridgeClient;
use aurelius_client::envelope;
use aurelius_domain::{
    Command, RunId,
    autonomy::AgentMode,
    idea::{AddIdea, DeleteIdea, IdeaKind, IdeaStatus, SetIdeaStatus, StartIdea, UpdateIdea},
    employee::CreateEmployee,
    project::CreateProject,
    task::{RunSettings, TaskStatus},
    ticket::CreateTicket,
};
use aurelius_protocol::{CommandResult, EmployeeTaskOp};
use common::*;
use std::time::Duration;

#[tokio::test(flavor = "multi_thread")]
async fn ideas_are_noted_started_as_tasks_or_dropped() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let CommandResult::ProjectCreated { project_id, .. } =
        client.execute(envelope(Command::CreateProject(CreateProject { name: "Atlas".into(), description: None, folder: None }), None)).await.unwrap().result
    else {
        panic!()
    };
    let CommandResult::EmployeeCreated { employee_id, .. } = client
        .execute(envelope(Command::CreateEmployee(CreateEmployee { name: "Alex".into(), role: "Developer".into(), title: None, reports_to: None, instructions: None, folder: None }), None))
        .await
        .unwrap()
        .result
    else {
        panic!()
    };

    // The user notes ideas; a title is required.
    assert!(client.execute(envelope(Command::AddIdea(AddIdea { confirm_secret: false, project_id, title: " ".into(), note: None, kind: Default::default(), files: vec![] }), None)).await.is_err());
    let add = |title: &str, note: Option<&str>| Command::AddIdea(AddIdea { confirm_secret: false, project_id, title: title.into(), note: note.map(str::to_owned), kind: Default::default(), files: vec![] });
    let CommandResult::IdeaAdded { idea_id: dark } = client.execute(envelope(add("Dark mode", Some("Voor 's avonds")), None)).await.unwrap().result else { panic!() };
    let CommandResult::IdeaAdded { idea_id: export } = client.execute(envelope(add("CSV export", None), None)).await.unwrap().result else { panic!() };
    client.execute(envelope(Command::UpdateIdea(UpdateIdea { confirm_secret: false, idea_id: export, title: "CSV-export".into(), note: Some("Per project".into()), kind: Some(IdeaKind::Bug), add_files: vec![], remove_files: vec![] }), None)).await.unwrap();

    // An employee notes one while working (the bridge's add_idea tool).
    let CommandResult::IdeaStarted { task_id, .. } = client.execute(envelope(Command::StartIdea(StartIdea { idea_id: dark, employee_id, run: Default::default() }), None)).await.unwrap().result else { panic!() };
    for _ in 0..400 {
        if client.tasks(None).await.unwrap().tasks.iter().any(|t| t.id == task_id && t.status.is_terminal()) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let run: String = db(dir.path()).query_row("SELECT id FROM runs WHERE task_id = ?1 LIMIT 1", [task_id.to_string()], |r| r.get(0)).unwrap();
    let (ok, text) = BridgeClient::connect(dir.path()).await.unwrap().employee_act(RunId::parse(&run).unwrap(), EmployeeTaskOp::AddIdea { title: "Thema per gebruiker".into(), note: "Kwam op tijdens dark mode".into(), kind: Default::default(), project: None }).await.unwrap();
    assert!(ok, "{text}");

    // Started: a task with the idea's title and note; the idea is done and points at it.
    let task = client.tasks(None).await.unwrap().tasks.into_iter().find(|t| t.id == task_id).unwrap();
    assert_eq!((task.title.as_str(), task.goal.as_str(), task.employee_id), ("Dark mode", "Voor 's avonds", Some(employee_id)));
    assert!(client.execute(envelope(Command::StartIdea(StartIdea { idea_id: dark, employee_id, run: Default::default() }), None)).await.is_err(), "only once");

    client.execute(envelope(Command::SetIdeaStatus(SetIdeaStatus { idea_id: export, status: IdeaStatus::Dropped }), None)).await.unwrap();
    let ideas = client.ideas(project_id).await.unwrap().ideas;
    assert_eq!(ideas.len(), 3);
    // The company-wide list has the same ideas, each still linked to its project.
    let everyone = client.all_ideas().await.unwrap().ideas;
    assert_eq!(everyone.len(), 3);
    assert!(everyone.iter().all(|i| i.project_id == project_id));
    assert_eq!(ideas.iter().find(|i| i.title == "CSV-export").unwrap().kind, IdeaKind::Bug, "update changes the kind");
    assert_eq!(ideas.iter().find(|i| i.title == "Dark mode").unwrap().kind, IdeaKind::Feature, "default kind");
    assert_eq!(ideas[0].title, "Thema per gebruiker", "open ideas first");
    assert_eq!((ideas[0].author.as_str(), ideas[0].status, ideas[0].employee_id), ("Alex", IdeaStatus::Open, Some(employee_id)));
    assert!(ideas[0].by_employee(), "an employee's idea is told apart from the user's");
    let started = ideas.iter().find(|i| i.id == dark).unwrap();
    assert_eq!((started.status, started.task_id, started.task_status), (IdeaStatus::Done, Some(task_id), Some(TaskStatus::Succeeded)));
    let dropped = ideas.iter().find(|i| i.id == export).unwrap();
    assert_eq!((dropped.title.as_str(), dropped.note.as_deref(), dropped.status, dropped.author.as_str()), ("CSV-export", Some("Per project"), IdeaStatus::Dropped, "You"));
    assert!(dropped.employee_id.is_none() && !dropped.by_employee());

    // Started in a chosen mode, the task conversation runs in it; without one it follows the employee
    // (Alex is the only employee, so the CEO: management starts in Plan and chooses itself).
    for mode in [Some(AgentMode::Plan), Some(AgentMode::Auto), Some(AgentMode::AcceptEdits), None] {
        let CommandResult::IdeaAdded { idea_id } = client.execute(envelope(add(&format!("Idee {mode:?}"), None), None)).await.unwrap().result else { panic!() };
        let run = RunSettings { mode, ..RunSettings::default() };
        let CommandResult::IdeaStarted { task_id, .. } = client.execute(envelope(Command::StartIdea(StartIdea { idea_id, employee_id, run }), None)).await.unwrap().result else { panic!() };
        let mut conversation = None;
        for _ in 0..400 {
            conversation = client.tasks(None).await.unwrap().tasks.into_iter().find(|t| t.id == task_id).and_then(|t| t.conversation_id);
            if conversation.is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        let mode_now = client.messages(conversation.expect("the task got its conversation")).await.unwrap().agent_mode;
        assert_eq!(mode_now, mode.unwrap_or(AgentMode::Plan), "{mode:?}");
    }

    // The project's timeline tells it.
    let page = client.timeline(aurelius_protocol::TimelineQuery { project_id: Some(project_id), before_seq: None, limit: 100, correlation_id: None, activity: false }).await.unwrap();
    assert!(page.cards.iter().any(|c| c.label == "IDEA" && c.title.contains("Alex noted an idea")), "{:?}", page.cards.iter().map(|c| &c.title).collect::<Vec<_>>());
    stop(dir.path(), daemon).await;
}

/// Files on an idea: kept in the project's uploads area, fetched by id, copied
/// to the task the idea becomes, and removed with the idea.
#[tokio::test(flavor = "multi_thread")]
async fn ideas_keep_files_that_go_with_them_into_the_task() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    assert!(client.supports(aurelius_protocol::feature::IDEA_ATTACHMENTS));
    let CommandResult::ProjectCreated { project_id, .. } =
        client.execute(envelope(Command::CreateProject(CreateProject { name: "Atlas".into(), description: None, folder: None }), None)).await.unwrap().result
    else {
        panic!()
    };
    let folder = std::path::PathBuf::from(client.projects().await.unwrap().projects.into_iter().find(|p| p.id == project_id).unwrap().folder.expect("folder"));
    let uploads = folder.join(".aurelius").join("uploads");
    let CommandResult::EmployeeCreated { employee_id, .. } = client
        .execute(envelope(Command::CreateEmployee(CreateEmployee { name: "Alex".into(), role: "Developer".into(), title: None, reports_to: None, instructions: None, folder: None }), None))
        .await
        .unwrap()
        .result
    else {
        panic!()
    };
    let (sketch, notes) = (dir.path().join("sketch.png"), dir.path().join("notes.txt"));
    let mut png = Vec::new();
    image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(400, 200, image::Rgb([10, 120, 200]))).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
    std::fs::write(&sketch, &png).unwrap();
    std::fs::write(&notes, b"first line\nsecond line").unwrap();
    let path = |p: &std::path::Path| p.display().to_string();

    // Added with a file and a note of several lines, kept as written.
    let note = "Steps:\n  1. open\n  2. look\n\nThat's it.";
    let add = AddIdea { confirm_secret: false, project_id, title: "Dark mode".into(), note: Some(note.into()), kind: Default::default(), files: vec![path(&sketch)] };
    let CommandResult::IdeaAdded { idea_id } = client.execute(envelope(Command::AddIdea(add), None)).await.unwrap().result else { panic!() };
    let idea = |ideas: Vec<aurelius_protocol::IdeaView>| ideas.into_iter().find(|i| i.id == idea_id).unwrap();
    let shown = idea(client.ideas(project_id).await.unwrap().ideas);
    assert_eq!(shown.note.as_deref(), Some(note), "lines and indentation stay");
    assert_eq!(shown.attachments.len(), 1);
    let file = &shown.attachments[0];
    assert_eq!((file.id.clone(), file.name.as_str(), file.path.as_str()), (Some(format!("i:{idea_id}:0")), "sketch.png", ".aurelius/uploads/sketch.png"));
    assert_eq!(client.upload(file.id.as_ref().unwrap()).await.unwrap().2, png, "fetched through the server");
    assert!(client.upload_thumbnail(file.id.as_ref().unwrap(), 100).await.is_ok(), "an image has a thumbnail");

    // Update: one more, then the first removed (its copy goes too); too many is refused.
    let update = |add_files: Vec<String>, remove_files: Vec<String>| {
        Command::UpdateIdea(UpdateIdea { confirm_secret: false, idea_id, title: "Dark mode".into(), note: Some(note.into()), kind: None, add_files, remove_files })
    };
    client.execute(envelope(update(vec![path(&notes)], vec![]), None)).await.unwrap();
    client.execute(envelope(update(vec![], vec![".aurelius/uploads/sketch.png".into()]), None)).await.unwrap();
    assert!(!uploads.join("sketch.png").exists(), "a removed file is deleted");
    let shown = idea(client.ideas(project_id).await.unwrap().ideas);
    assert_eq!(shown.attachments.iter().map(|f| (f.id.clone().unwrap(), f.path.as_str())).collect::<Vec<_>>(), vec![(format!("i:{idea_id}:0"), ".aurelius/uploads/notes.txt")]);
    let too_many = update(vec![path(&notes); aurelius_domain::conversation::UPLOADS_MAX], vec![]);
    assert!(client.execute(envelope(too_many, None)).await.is_err());
    assert_eq!(std::fs::read_dir(&uploads).unwrap().count(), 2, "nothing left behind by the refused update: .gitignore and notes.txt");

    // Started: the task has its own copy, named to the employee like a task attachment.
    let CommandResult::IdeaStarted { task_id, .. } = client.execute(envelope(Command::StartIdea(StartIdea { idea_id, employee_id, run: Default::default() }), None)).await.unwrap().result else { panic!() };
    let task = client.tasks(None).await.unwrap().tasks.into_iter().find(|t| t.id == task_id).unwrap();
    assert_eq!(task.goal, note);
    assert_eq!(task.attachments.len(), 1);
    assert_eq!((task.attachments[0].name.as_str(), task.attachments[0].id.clone()), ("notes (2).txt", Some(format!("t:{task_id}:0"))));
    assert_eq!(client.upload(&format!("t:{task_id}:0")).await.unwrap().2, b"first line\nsecond line");

    // Deleted: the idea and its files go; the task keeps its copy.
    client.execute(envelope(Command::DeleteIdea(DeleteIdea { idea_id }), None)).await.unwrap();
    assert!(client.ideas(project_id).await.unwrap().ideas.iter().all(|i| i.id != idea_id));
    assert!(!uploads.join("notes.txt").exists() && uploads.join("notes (2).txt").exists());
    assert!(client.upload(&format!("i:{idea_id}:0")).await.is_err());
    assert!(client.execute(envelope(Command::DeleteIdea(DeleteIdea { idea_id }), None)).await.is_err(), "gone");
    stop(dir.path(), daemon).await;
}

fn ticket_from(project_id: aurelius_domain::ProjectId, idea: aurelius_domain::IdeaId, title: &str, start: bool) -> Command {
    Command::CreateTicket(CreateTicket {
        responsible_user_id: None,
        confirm_secret: false,
        project_id,
        title: title.into(),
        description: Some("From the idea".into()),
        priority: Default::default(),
        labels: vec![],
        deadline_ms: None,
        owner_employee_id: None,
        run: Default::default(),
        files: vec![],
        start,
        from_message: None,
        from_idea: Some(idea),
    })
}

/// "Create ticket" on an idea (ticket #16): one transaction makes the ticket, closes
/// the idea and links them; the ticket has its own copies of the idea's files.
#[tokio::test(flavor = "multi_thread")]
async fn an_idea_becomes_a_ticket_once_with_copies_of_its_files() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    assert!(client.supports(aurelius_protocol::feature::TICKET_FROM_IDEA));
    let CommandResult::ProjectCreated { project_id, .. } =
        client.execute(envelope(Command::CreateProject(CreateProject { name: "Atlas".into(), description: None, folder: None }), None)).await.unwrap().result
    else {
        panic!()
    };
    let notes = dir.path().join("notes.txt");
    std::fs::write(&notes, b"the notes").unwrap();
    let add = |title: &str, files: Vec<String>| Command::AddIdea(AddIdea { confirm_secret: false, project_id, title: title.into(), note: Some("Its note".into()), kind: Default::default(), files });
    let CommandResult::IdeaAdded { idea_id } = client.execute(envelope(add("Dark mode", vec![notes.display().to_string()]), None)).await.unwrap().result else { panic!() };

    let CommandResult::TicketCreated { ticket_id, number } = client.execute(envelope(ticket_from(project_id, idea_id, "Dark mode", false), None)).await.unwrap().result else { panic!() };

    let idea = client.ideas(project_id).await.unwrap().ideas.into_iter().find(|i| i.id == idea_id).unwrap();
    assert_eq!((idea.status, idea.ticket_id, idea.ticket_number, idea.task_id), (IdeaStatus::Done, Some(ticket_id), Some(number), None));
    let ticket = client.ticket(ticket_id).await.unwrap();
    assert_eq!(ticket.ticket.attachments.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), ["notes (2).txt"], "its own copy, next to the idea's");
    assert_eq!(ticket.ticket.description.as_deref(), Some("From the idea"));

    // Its copy outlives the idea.
    client.execute(envelope(Command::DeleteIdea(DeleteIdea { idea_id }), None)).await.unwrap();
    let kept = client.ticket(ticket_id).await.unwrap().ticket.attachments[0].id.clone().expect("an upload id");
    assert_eq!(client.upload(&kept).await.unwrap().2, b"the notes", "the ticket keeps its own copy");
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_refused_ticket_leaves_the_idea_open_and_an_idea_with_a_ticket_or_task_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let project = |name: &str| Command::CreateProject(CreateProject { name: name.into(), description: None, folder: None });
    let CommandResult::ProjectCreated { project_id, .. } = client.execute(envelope(project("Atlas"), None)).await.unwrap().result else { panic!() };
    let CommandResult::ProjectCreated { project_id: other, .. } = client.execute(envelope(project("Zeta"), None)).await.unwrap().result else { panic!() };
    let CommandResult::EmployeeCreated { employee_id, .. } = client
        .execute(envelope(Command::CreateEmployee(CreateEmployee { name: "Alex".into(), role: "Developer".into(), title: None, reports_to: None, instructions: None, folder: None }), None))
        .await
        .unwrap()
        .result
    else {
        panic!()
    };
    let add = |title: &str| Command::AddIdea(AddIdea { confirm_secret: false, project_id, title: title.into(), note: None, kind: Default::default(), files: vec![] });
    let CommandResult::IdeaAdded { idea_id } = client.execute(envelope(add("One"), None)).await.unwrap().result else { panic!() };

    // Another project, or an idea that does not exist: refused, and nothing changed.
    let wrong = client.execute(envelope(ticket_from(other, idea_id, "One", false), None)).await.unwrap_err().to_string();
    assert!(wrong.contains("another project"), "{wrong}");
    assert!(client.execute(envelope(ticket_from(project_id, aurelius_domain::IdeaId::new(), "None", false), None)).await.is_err());
    let open = client.ideas(project_id).await.unwrap().ideas.into_iter().find(|i| i.id == idea_id).unwrap();
    assert_eq!((open.status, open.ticket_id), (IdeaStatus::Open, None));
    assert!(client.board(None).await.unwrap().columns.iter().all(|c| c.tickets.is_empty()), "no ticket was made");

    // Started: it goes to Todo, and the idea closes with it. A second ticket or a task (an older client) is refused.
    client.execute(envelope(ticket_from(project_id, idea_id, "One", true), None)).await.unwrap();
    let twice = client.execute(envelope(ticket_from(project_id, idea_id, "One again", false), None)).await.unwrap_err().to_string();
    assert!(twice.contains("already became a ticket"), "{twice}");
    let task = client.execute(envelope(Command::StartIdea(StartIdea { idea_id, employee_id, run: Default::default() }), None)).await.unwrap_err().to_string();
    assert!(task.contains("already became a ticket"), "{task}");

    // An idea that became a task before keeps that link: no ticket on top of it.
    let CommandResult::IdeaAdded { idea_id: started } = client.execute(envelope(add("Two"), None)).await.unwrap().result else { panic!() };
    client.execute(envelope(Command::StartIdea(StartIdea { idea_id: started, employee_id, run: Default::default() }), None)).await.unwrap();
    let on_task = client.execute(envelope(ticket_from(project_id, started, "Two", false), None)).await.unwrap_err().to_string();
    assert!(on_task.contains("already became a task"), "{on_task}");
    stop(dir.path(), daemon).await;
}
