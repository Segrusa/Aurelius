//! A ticket's title made from its description: when it is asked for, what it
//! reads and when it replaces the first-line title.

use super::*;
use crate::*;
use aurelius_domain::employee::CreateEmployee;
use aurelius_domain::project::CreateProject;
use aurelius_domain::ticket::{CreateTicket, UpdateTicket};
use aurelius_domain::{EmployeeId, ProjectId, UserId};
use aurelius_storage::Store;
use std::sync::Arc;

const PHC: &str = "$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHQ$aGFzaGhhc2hoYXNoaGFzaA";
const DESCRIPTION: &str = "kan je eens kijken waarom de export zo traag is\nVooral bij grote projecten.";
const FIRST_LINE: &str = "kan je eens kijken waarom de export zo traag is";

struct Setup {
    _dir: tempfile::TempDir,
    service: CommandService,
    project: ProjectId,
    grace: UserId,
    orion: EmployeeId,
}

fn ctx(user: Option<UserId>) -> CommandContext {
    CommandContext { principal_id: aurelius_domain::PrincipalId::new(), client_id: aurelius_domain::ClientId::new(), origin: Origin::LocalIpc, request_id: 1, user_id: user }
}

fn exec(s: &Setup, command: Command) -> Result<CommandResult, AppError> {
    s.service.execute(&ctx(Some(s.grace)), CommandEnvelope { command_id: aurelius_domain::CommandId::new(), expected_version: None, command }).map(|c| c.receipt.result)
}

fn setup() -> Setup {
    let dir = tempfile::tempdir().unwrap();
    let service = CommandService::new(Arc::new(Store::open(&dir.path().join("data")).unwrap()));
    let grace = UserId::new();
    service.store().write(move |tx| aurelius_storage::users::insert(tx, &aurelius_storage::users::NewUser { id: &grace.to_string(), username: "grace", created_by_user: None, now_ms: 1 }, PHC)).unwrap();
    let mut s = Setup { _dir: dir, service, project: ProjectId::new(), grace, orion: EmployeeId::new() };
    let made = exec(&s, Command::CreateProject(CreateProject { name: "Planora".into(), description: None, folder: None })).unwrap();
    let CommandResult::ProjectCreated { project_id, .. } = made else { panic!("{made:?}") };
    let create = CreateEmployee { access_level: None, tool_policy: None, start_profile: None, description: None, name: "Orion".into(), role: "Project Manager".into(), title: None, reports_to: None, instructions: None, folder: None };
    let made = exec(&s, Command::CreateEmployee(create)).unwrap();
    let CommandResult::EmployeeCreated { employee_id, .. } = made else { panic!("{made:?}") };
    (s.project, s.orion) = (project_id, employee_id);
    s
}

fn create(s: &Setup, title: &str, description: Option<&str>) -> Result<TicketId, AppError> {
    let cmd = CreateTicket {
        responsible_user_id: None,
        confirm_secret: false,
        project_id: s.project,
        title: title.into(),
        description: description.map(Into::into),
        priority: Default::default(),
        labels: vec![],
        deadline_ms: None,
        owner_employee_id: Some(s.orion),
        run: Default::default(),
        files: vec![],
        start: false,
        from_message: None,
        from_idea: None,
        kind: None,
    };
    match exec(s, Command::CreateTicket(cmd))? {
        CommandResult::TicketCreated { ticket_id, .. } => Ok(ticket_id),
        other => panic!("{other:?}"),
    }
}

/// (state, placeholder) of the ticket's title jobs.
fn jobs(s: &Setup, ticket: TicketId) -> Vec<(String, String)> {
    s.service
        .store()
        .read(move |c| {
            let mut stmt = c.prepare("SELECT state, json_extract(payload_json, '$.placeholder') FROM jobs WHERE kind = ?1 AND json_extract(payload_json, '$.ticket_id') = ?2")?;
            let rows = stmt.query_map([TICKET_TITLE_JOB_KIND, &ticket.to_string()], |r| Ok((r.get(0)?, r.get(1)?)))?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .unwrap()
}

/// The ticket's title and the title of its discussion.
fn titles(s: &Setup, ticket: TicketId) -> (String, String) {
    s.service
        .store()
        .read(move |c| Ok(c.query_row("SELECT k.title, c.title FROM tickets k JOIN conversations c ON c.id = k.conversation_id WHERE k.id = ?1", [ticket.to_string()], |r| Ok((r.get(0)?, r.get(1)?)))?))
        .unwrap()
}

/// Lease the ticket's ready title job now, as the dispatcher would; (job id, epoch).
fn lease(s: &Setup, ticket: TicketId) -> (String, i64) {
    s.service
        .store()
        .write(move |tx| {
            let id: String = tx.query_row(
                "SELECT id FROM jobs WHERE kind = ?1 AND state = 'ready' AND json_extract(payload_json, '$.ticket_id') = ?2",
                [TICKET_TITLE_JOB_KIND, &ticket.to_string()],
                |r| r.get(0),
            )?;
            tx.execute("UPDATE jobs SET state = 'leased', lease_owner = 'test', lease_until_ms = ?2, lease_epoch = lease_epoch + 1 WHERE id = ?1", rusqlite::params![id, aurelius_storage::now_ms() + 60_000])?;
            let epoch: i64 = tx.query_row("SELECT lease_epoch FROM jobs WHERE id = ?1", [&id], |r| r.get(0))?;
            Ok((id, epoch))
        })
        .unwrap()
}

#[test]
fn an_empty_title_with_a_description_starts_as_its_first_line_and_asks_for_a_title_once() {
    let s = setup();
    let ticket = create(&s, "  ", Some(DESCRIPTION)).unwrap();
    assert_eq!(titles(&s, ticket).0, FIRST_LINE);
    assert_eq!(jobs(&s, ticket), vec![("ready".to_owned(), FIRST_LINE.to_owned())]);
}

#[test]
fn a_typed_title_asks_for_no_title() {
    let s = setup();
    let ticket = create(&s, "Trage export", Some(DESCRIPTION)).unwrap();
    assert_eq!(titles(&s, ticket).0, "Trage export");
    assert!(jobs(&s, ticket).is_empty());
}

#[test]
fn an_empty_title_without_a_description_is_still_refused() {
    let s = setup();
    assert!(create(&s, "", None).is_err());
    assert!(create(&s, "", Some("   ")).is_err());
}

#[test]
fn the_input_reads_the_description_as_its_owner_on_the_tickets_login() {
    let s = setup();
    let ticket = create(&s, "", Some(DESCRIPTION)).unwrap();
    let input = s.service.ticket_title_input(ticket).unwrap().expect("a description to read");
    assert_eq!(input.project_name, "Planora");
    assert_eq!(input.description, DESCRIPTION);
    assert_eq!(input.config.login_user, Some(s.grace));
}

#[test]
fn a_made_title_is_cleaned_redacted_and_replaces_the_first_line() {
    let s = setup();
    let ticket = create(&s, "", Some(DESCRIPTION)).unwrap();
    let (job, epoch) = lease(&s, ticket);
    let value = format!("ghp_{}", aurelius_domain::CommandId::new().0.simple());
    let owner = format!("ticket-title-test-{}", aurelius_domain::CommandId::new());
    let secret = aurelius_runtime::redact::Secret { name: "GH_TOKEN".into(), label: "GitHub token".into(), value: zeroize::Zeroizing::new(value.clone()) };
    aurelius_runtime::redact::install_set(&owner, vec![secret]);
    let notices = s.service.record_ticket_title(&job, epoch, ticket, FIRST_LINE, &format!("Title: \"Trage export met {value}.\"\nBecause it is slow."));
    aurelius_runtime::redact::install_set(&owner, Vec::new());
    let notices = notices.unwrap();
    assert_eq!(notices.len(), 1, "open boards hear of it");
    assert_eq!(notices[0].kind, aurelius_domain::EventKind::TicketUpdated);
    let (title, discussion) = titles(&s, ticket);
    assert_eq!(title, "Trage export met «secret:GH_TOKEN»");
    assert!(discussion.ends_with(" Trage export met «secret:GH_TOKEN»"), "its discussion follows: {discussion}");
    assert_eq!(jobs(&s, ticket)[0].0, "done");
}

#[test]
fn a_title_renamed_meanwhile_is_kept() {
    let s = setup();
    let ticket = create(&s, "", Some(DESCRIPTION)).unwrap();
    let rename = UpdateTicket {
        design_url: None,
        confirm_secret: false,
        ticket_id: ticket,
        title: Some("Mijn eigen titel".into()),
        description: None,
        priority: None,
        labels: None,
        deadline_ms: None,
        clear_deadline: false,
        run: None,
        add_files: None,
    };
    exec(&s, Command::UpdateTicket(rename)).unwrap();
    let (job, epoch) = lease(&s, ticket);
    assert!(s.service.record_ticket_title(&job, epoch, ticket, FIRST_LINE, "Trage export").unwrap().is_empty());
    assert_eq!(titles(&s, ticket).0, "Mijn eigen titel");
    assert_eq!(jobs(&s, ticket)[0].0, "done");
}

#[test]
fn an_empty_answer_or_a_lost_lease_keeps_the_first_line() {
    let s = setup();
    let ticket = create(&s, "", Some(DESCRIPTION)).unwrap();
    let (job, epoch) = lease(&s, ticket);
    assert!(s.service.record_ticket_title(&job, epoch + 1, ticket, FIRST_LINE, "Too late").unwrap().is_empty());
    assert_eq!(titles(&s, ticket).0, FIRST_LINE);
    assert!(s.service.record_ticket_title(&job, epoch, ticket, FIRST_LINE, "  \n\"\"").unwrap().is_empty());
    assert_eq!(titles(&s, ticket).0, FIRST_LINE);
    assert_eq!(jobs(&s, ticket)[0].0, "done");
}
