//! Guideline writes (AUR-GL-01-F2): a board member first (SEC0 T12), B4 on
//! every write with a rollback, B10 on the texts a command does not carry
//! (T14, T16, T17), the proposal decisions of E12 and the events.

use super::*;
use aurelius_domain::guideline::*;
use aurelius_domain::project::CreateProject;
use aurelius_domain::proposal::ProposalStatus;
use aurelius_domain::{EmployeeId, GuidelineId, GuidelineProposalId, UserId};
use aurelius_storage::guidelines as store;

const PHC: &str = "$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHQ$aGFzaGhhc2hoYXNoaGFzaA";

struct Setup {
    _dir: tempfile::TempDir,
    service: CommandService,
    project: ProjectId,
    user: UserId,
}

fn ctx(user: Option<UserId>) -> CommandContext {
    CommandContext { principal_id: PrincipalId::new(), client_id: ClientId::new(), origin: Origin::LocalIpc, request_id: 1, user_id: user }
}

fn run(s: &Setup, command: Command) -> Result<CommandResult, AppError> {
    run_as(s, Some(s.user), command, None)
}

fn run_as(s: &Setup, user: Option<UserId>, command: Command, expected_version: Option<u64>) -> Result<CommandResult, AppError> {
    s.service.execute(&ctx(user), CommandEnvelope { command_id: CommandId::new(), expected_version, command }).map(|c| c.receipt.result)
}

fn setup() -> Setup {
    let dir = tempfile::tempdir().unwrap();
    let service = CommandService::new(Arc::new(Store::open(dir.path()).unwrap()));
    let created = service
        .execute(&ctx(None), CommandEnvelope { command_id: CommandId::new(), expected_version: None, command: Command::CreateProject(CreateProject { name: "Atlas".into(), description: None, folder: None }) })
        .unwrap()
        .receipt
        .result;
    let CommandResult::ProjectCreated { project_id, .. } = created else { panic!("{created:?}") };
    let user = UserId::new();
    service
        .store()
        .write(move |tx| aurelius_storage::users::insert(tx, &aurelius_storage::users::NewUser { id: &user.to_string(), username: "grace", created_by_user: None, now_ms: 1 }, PHC))
        .unwrap();
    Setup { _dir: dir, service, project: project_id, user }
}

fn employee(s: &Setup, name: &str) -> EmployeeId {
    let (id, name) = (EmployeeId::new(), name.to_owned());
    s.service
        .store()
        .write(move |tx| {
            tx.execute(
                "INSERT INTO scopes(id, kind, parent_id, owner_id) VALUES (?1, 'employee', (SELECT id FROM scopes WHERE kind = 'global'), ?2)",
                [aurelius_domain::ScopeId::new().to_string(), id.to_string()],
            )?;
            tx.execute(
                "INSERT INTO employees(id, scope_id, name, role, config_json, status, version, created_at_ms, updated_at_ms)
                 VALUES (?1, (SELECT id FROM scopes WHERE owner_id = ?1), ?2, 'r', '{}', 'active', 1, 1, 1)",
                [id.to_string(), name],
            )?;
            Ok(())
        })
        .unwrap();
    id
}

fn create(s: &Setup, title: &str, text: &str, links: Vec<GuidelineTarget>) -> Result<GuidelineId, AppError> {
    match run(s, Command::CreateGuideline(CreateGuideline { title: title.into(), text: text.into(), links }))? {
        CommandResult::GuidelineChanged { guideline_id, version: 1 } => Ok(guideline_id),
        other => panic!("{other:?}"),
    }
}

fn current(s: &Setup, id: GuidelineId) -> GuidelineVersionView {
    s.service.store().read(|c| store::get(c, id, None)).unwrap().unwrap()
}

fn view(s: &Setup, id: GuidelineId) -> GuidelineView {
    s.service.store().read(|c| store::view(c, id)).unwrap().unwrap()
}

fn guideline_events(s: &Setup) -> i64 {
    s.service.store().read(|c| Ok(c.query_row("SELECT COUNT(*) FROM events WHERE type LIKE 'guideline.%'", [], |r| r.get(0))?)).unwrap()
}

fn proposal(s: &Setup, guideline: Option<GuidelineId>, base: Option<u32>, by: EmployeeId, title: &str, text: &str, suggested: Vec<GuidelineTarget>) -> GuidelineProposalId {
    let id = GuidelineProposalId::new();
    let (title, text) = (title.to_owned(), text.to_owned());
    s.service
        .store()
        .write(move |tx| {
            store::insert_proposal(
                tx,
                store::NewGuidelineProposal {
                    id,
                    guideline_id: guideline,
                    base_version: base,
                    proposed_by: by,
                    title: &title,
                    text: &text,
                    reason: "Twice a review found it",
                    evidence: &[],
                    suggested_links: &suggested,
                    origin_conversation_id: None,
                    run_id: None,
                    now_ms: 5,
                },
            )
        })
        .unwrap();
    id
}

fn decide(id: GuidelineProposalId, approve: bool) -> DecideGuidelineProposal {
    DecideGuidelineProposal { proposal_id: id, approve, title: None, text: None, links: None, note: None }
}

/// A vault value in the process' redactor for the length of a test.
struct Vault(String);

impl Vault {
    fn with(value: &str) -> Self {
        let owner = format!("guideline-test-{}", aurelius_domain::CommandId::new().0);
        let secret = aurelius_runtime::redact::Secret { name: "GH_TOKEN".into(), label: "GitHub token".into(), value: zeroize::Zeroizing::new(value.into()) };
        aurelius_runtime::redact::install_set(&owner, vec![secret]);
        Vault(owner)
    }
}

impl Drop for Vault {
    fn drop(&mut self) {
        aurelius_runtime::redact::install_set(&self.0, Vec::new());
    }
}

fn assert_secret(result: Result<CommandResult, AppError>) {
    let err = result.unwrap_err();
    assert_eq!(err.code, ErrorCode::LooksLikeSecret, "{err:?}");
    assert_eq!(err.matched_items, vec!["GitHub token".to_string()], "the item's name only");
}

fn too_large(result: Result<CommandResult, AppError>) -> GuidelineTooLarge {
    let err = result.unwrap_err();
    assert_eq!(err.code, ErrorCode::GuidelineTooLarge, "{err:?}");
    err.too_large.expect("names the combination")
}

const BIG: usize = 7_000;

fn big(c: char) -> String {
    c.to_string().repeat(BIG)
}

/// T12: every write without a board member is refused before anything else.
#[test]
fn t12_every_write_needs_a_board_member() {
    let s = setup();
    let id = create(&s, "Rust", "Use clippy.", vec![]).unwrap();
    let target = GuidelineTarget::Project(s.project);
    let writes = [
        Command::CreateGuideline(CreateGuideline { title: "X".into(), text: "y".into(), links: vec![] }),
        Command::UpdateGuideline(UpdateGuideline { guideline_id: id, title: None, text: Some("z".into()) }),
        Command::ArchiveGuideline(ArchiveGuideline { guideline_id: id }),
        Command::RestoreGuideline(RestoreGuideline { guideline_id: id }),
        Command::RestoreGuidelineVersion(RestoreGuidelineVersion { guideline_id: id, version: 1 }),
        Command::LinkGuideline(LinkGuideline { guideline_id: id, target }),
        Command::UnlinkGuideline(UnlinkGuideline { guideline_id: id, target }),
        Command::DecideGuidelineProposal(decide(GuidelineProposalId::new(), true)),
        // Even an invalid command: the board member comes first.
        Command::AddStarterGuidelines(AddStarterGuidelines { keys: vec![] }),
    ];
    assert_eq!(writes.len(), 9);
    let before = guideline_events(&s);
    for command in writes {
        let method = command.method();
        let err = run_as(&s, None, command.clone(), None).unwrap_err();
        assert_eq!(err.code, ErrorCode::BoardMemberRequired, "{method}");
    }
    assert_eq!(guideline_events(&s), before, "nothing was written");
    assert_eq!(current(&s, id).version, 1);
}

#[test]
fn create_update_archive_restore_and_versions() {
    let s = setup();
    let ann = employee(&s, "Ann");
    let id = create(&s, " Rust style ", "Use clippy.", vec![GuidelineTarget::Employee(ann)]).unwrap();
    let v = view(&s, id);
    assert_eq!((v.title.as_str(), v.version, v.links.clone()), ("Rust style", 1, vec![GuidelineTarget::Employee(ann)]));
    assert_eq!(create(&s, "rust STYLE", "x", vec![]).unwrap_err().code, ErrorCode::GuidelineTitleTaken);

    // update: what is left out stays; the editor's version must still be current.
    let up = |text: &str, expected| run_as(&s, Some(s.user), Command::UpdateGuideline(UpdateGuideline { guideline_id: id, title: None, text: Some(text.into()) }), expected);
    assert_eq!(up("Use clippy and fmt.", Some(1)).unwrap(), CommandResult::GuidelineChanged { guideline_id: id, version: 2 });
    assert_eq!(up("Stale.", Some(1)).unwrap_err().code, ErrorCode::VersionConflict);
    assert_eq!(up("Use clippy and fmt.", None).unwrap(), CommandResult::GuidelineChanged { guideline_id: id, version: 2 }, "the same text makes no version");
    let v2 = current(&s, id);
    assert_eq!((v2.title.as_str(), v2.text.as_str(), v2.source, v2.by.map(|u| u.id)), ("Rust style", "Use clippy and fmt.", GuidelineSource::Edited, Some(s.user)));

    // restore_version: a copy of 1 as 3.
    let restored = run(&s, Command::RestoreGuidelineVersion(RestoreGuidelineVersion { guideline_id: id, version: 1 })).unwrap();
    assert_eq!(restored, CommandResult::GuidelineChanged { guideline_id: id, version: 3 });
    let v3 = current(&s, id);
    assert_eq!((v3.text.as_str(), v3.source, v3.restored_from), ("Use clippy.", GuidelineSource::Restored, Some(1)));
    assert_eq!(run(&s, Command::RestoreGuidelineVersion(RestoreGuidelineVersion { guideline_id: id, version: 9 })).unwrap_err().code, ErrorCode::NotFound);

    // archive: the title is free; restore refuses while another has it.
    run(&s, Command::ArchiveGuideline(ArchiveGuideline { guideline_id: id })).unwrap();
    assert!(view(&s, id).archived);
    assert_eq!(run(&s, Command::ArchiveGuideline(ArchiveGuideline { guideline_id: id })).unwrap_err().code, ErrorCode::Validation);
    let other = create(&s, "Rust style", "Another.", vec![]).unwrap();
    assert_eq!(run(&s, Command::RestoreGuideline(RestoreGuideline { guideline_id: id })).unwrap_err().code, ErrorCode::GuidelineTitleTaken);
    run(&s, Command::ArchiveGuideline(ArchiveGuideline { guideline_id: other })).unwrap();
    run(&s, Command::RestoreGuideline(RestoreGuideline { guideline_id: id })).unwrap();
    assert!(!view(&s, id).archived && view(&s, id).links == vec![GuidelineTarget::Employee(ann)], "back with its links");

    // link / unlink: idempotent, an event only for a change.
    let events = guideline_events(&s);
    let target = GuidelineTarget::Project(s.project);
    run(&s, Command::LinkGuideline(LinkGuideline { guideline_id: id, target })).unwrap();
    run(&s, Command::LinkGuideline(LinkGuideline { guideline_id: id, target })).unwrap();
    assert_eq!(guideline_events(&s), events + 1);
    run(&s, Command::UnlinkGuideline(UnlinkGuideline { guideline_id: id, target })).unwrap();
    run(&s, Command::UnlinkGuideline(UnlinkGuideline { guideline_id: id, target })).unwrap();
    assert_eq!(guideline_events(&s), events + 2);
    let gone = GuidelineTarget::Employee(EmployeeId::new());
    assert_eq!(run(&s, Command::LinkGuideline(LinkGuideline { guideline_id: id, target: gone })).unwrap_err().code, ErrorCode::NotFound);
    assert_eq!(run(&s, Command::UnlinkGuideline(UnlinkGuideline { guideline_id: GuidelineId::new(), target })).unwrap_err().code, ErrorCode::NotFound);

    // The events carry GuidelineEventMeta and who acted.
    let (kind, meta): (String, String) = s
        .service
        .store()
        .read(|c| Ok(c.query_row("SELECT type, metadata_json FROM events WHERE type = 'guideline.linked' ORDER BY seq DESC LIMIT 1", [], |r| Ok((r.get(0)?, r.get(1)?)))?))
        .unwrap();
    let meta: GuidelineEventMeta = serde_json::from_str(&meta).unwrap();
    assert_eq!((kind.as_str(), meta.id, meta.by, meta.target), ("guideline.linked", id, Some(s.user), Some(target)));
}

/// T5 (application half): a hidden character never reaches storage.
#[test]
fn t5_hidden_characters_are_refused_at_the_write() {
    let s = setup();
    for (title, text) in [("Line\nbreak", "ok"), ("Fine", "right\u{202E}left"), ("Fine", "zero\u{200B}width"), ("Fine", "\u{FEFF}bom"), ("Fine", "tag\u{E0041}")] {
        assert_eq!(create(&s, title, text, vec![]).unwrap_err().code, ErrorCode::Validation, "{title:?} {text:?}");
    }
    assert_eq!(guideline_events(&s), 0);
}

/// B4 on every write that can grow a set; the write is rolled back.
#[test]
fn b4_every_write_that_grows_a_set_is_refused_over_the_limit() {
    let s = setup();
    let ann = employee(&s, "Ann");
    let me = GuidelineTarget::Employee(ann);
    let project = GuidelineTarget::Project(s.project);
    let n = |c: char, n: usize| c.to_string().repeat(n);
    let a = create(&s, "A", &n('a', 7_000), vec![me]).unwrap();
    let _b = create(&s, "B", &n('b', 7_000), vec![project]).unwrap();
    let c = create(&s, "C", &n('c', 3_000), vec![]).unwrap();
    let combination = |chars: u32| GuidelineTooLarge { chars, limit: 16_000, employee_id: Some(ann), project_id: Some(s.project) };

    // link: Ann in the project would have 17 000.
    assert_eq!(too_large(run(&s, Command::LinkGuideline(LinkGuideline { guideline_id: c, target: me }))), combination(17_000));
    assert!(view(&s, c).links.is_empty(), "rolled back");

    // create with links.
    let err = run(&s, Command::CreateGuideline(CreateGuideline { title: "D".into(), text: n('d', 3_000), links: vec![project] })).unwrap_err();
    assert_eq!((err.code, err.too_large.clone()), (ErrorCode::GuidelineTooLarge, Some(combination(17_000))));
    assert!(err.message.contains("Ann in the project Atlas") && err.message.contains("17000"), "{}", err.message);
    assert!(s.service.store().read(|c| store::list(c, true)).unwrap().iter().all(|g| g.title != "D"), "nothing saved");

    // update: S grows the set over the limit.
    let small = create(&s, "S", &n('s', 1_500), vec![me]).unwrap();
    let set_text = |id, text: String| run(&s, Command::UpdateGuideline(UpdateGuideline { guideline_id: id, title: None, text: Some(text) }));
    assert_eq!(too_large(set_text(small, n('s', 3_000))), combination(17_000));
    assert_eq!(current(&s, small).version, 1, "rolled back");

    // restore_version: S shrinks, A grows, then S's old version no longer fits.
    set_text(small, n('s', 500)).unwrap();
    set_text(a, n('a', 8_000)).unwrap();
    assert_eq!(too_large(run(&s, Command::RestoreGuidelineVersion(RestoreGuidelineVersion { guideline_id: small, version: 1 }))), combination(16_500));
    assert_eq!(current(&s, small).version, 2, "rolled back");

    // restore: S back from the archive, with its link, after E took its room.
    run(&s, Command::ArchiveGuideline(ArchiveGuideline { guideline_id: small })).unwrap();
    let e = create(&s, "E", &n('e', 1_000), vec![project]).unwrap();
    assert_eq!(too_large(run(&s, Command::RestoreGuideline(RestoreGuideline { guideline_id: small }))), combination(16_500));
    assert!(view(&s, small).archived, "rolled back");

    // decide: approving a change that makes E too big; the decision is rolled back too.
    let p = proposal(&s, Some(e), Some(1), ann, "E", &n('e', 1_500), vec![]);
    assert_eq!(too_large(run(&s, Command::DecideGuidelineProposal(decide(p, true)))), combination(16_500));
    let still = s.service.store().read(|c| store::proposal(c, p)).unwrap().unwrap();
    assert_eq!((still.status, current(&s, e).version), (ProposalStatus::Pending, 1));

    // A new guideline from a proposal, with links that overflow.
    let p = proposal(&s, None, None, ann, "F", &n('f', 1_000), vec![me]);
    assert_eq!(too_large(run(&s, Command::DecideGuidelineProposal(decide(p, true)))), combination(17_000));
    assert!(s.service.store().read(|c| store::list(c, true)).unwrap().iter().all(|g| g.title != "F"));

    // Writes that make a set smaller always pass.
    run(&s, Command::UnlinkGuideline(UnlinkGuideline { guideline_id: e, target: project })).unwrap();
    run(&s, Command::ArchiveGuideline(ArchiveGuideline { guideline_id: a })).unwrap();
}

/// B4 counts what a write changes: a combination already over (a project back
/// from the archive) does not block an unrelated write, but a write that makes it worse is refused.
#[test]
fn b4_an_overflow_from_before_does_not_block_other_writes() {
    let s = setup();
    let ann = employee(&s, "Ann");
    let a = create(&s, "A", &big('a'), vec![GuidelineTarget::Employee(ann)]).unwrap();
    let b = create(&s, "B", &big('b'), vec![]).unwrap();
    let c = create(&s, "C", &big('c'), vec![]).unwrap();
    // Two more links straight in the database: Ann is at 21 000 already.
    s.service
        .store()
        .write(move |tx| {
            for g in [b, c] {
                tx.execute("INSERT INTO guideline_links(guideline_id, employee_id, created_at_ms) VALUES (?1, ?2, 1)", [g.to_string(), ann.to_string()])?;
            }
            Ok(())
        })
        .unwrap();
    let other = create(&s, "Other", "Small.", vec![]).unwrap();
    run(&s, Command::UpdateGuideline(UpdateGuideline { guideline_id: other, title: None, text: Some("Still small.".into()) })).unwrap();
    // Making Ann's set smaller passes (still over), larger than it was is refused.
    run(&s, Command::UpdateGuideline(UpdateGuideline { guideline_id: a, title: None, text: Some("a".repeat(6_000)) })).unwrap();
    let over = too_large(run(&s, Command::UpdateGuideline(UpdateGuideline { guideline_id: a, title: None, text: Some("a".repeat(6_500)) })));
    assert_eq!((over.employee_id, over.chars), (Some(ann), 20_500));
}

#[test]
fn b4_the_starter_set_adds_without_links() {
    let s = setup();
    let ann = employee(&s, "Ann");
    let _ = create(&s, "Full", &"f".repeat(8_000), vec![GuidelineTarget::Employee(ann)]).unwrap();
    let added = run(&s, Command::AddStarterGuidelines(AddStarterGuidelines { keys: vec!["g1".into(), "p1".into()] })).unwrap();
    let CommandResult::StarterGuidelinesAdded { guideline_ids } = added else { panic!("{added:?}") };
    assert_eq!(guideline_ids.len(), 2);
    assert!(guideline_ids.iter().all(|id| view(&s, *id).links.is_empty() && current(&s, *id).source == GuidelineSource::Seed));
    // Again: skipped, by title.
    let again = run(&s, Command::AddStarterGuidelines(AddStarterGuidelines { keys: vec!["g1".into()] })).unwrap();
    assert_eq!(again, CommandResult::StarterGuidelinesAdded { guideline_ids: vec![] });
    assert_eq!(run(&s, Command::AddStarterGuidelines(AddStarterGuidelines { keys: vec!["nope".into()] })).unwrap_err().code, ErrorCode::Validation);
}

/// T14 (application half): a vault value in a title or text is refused, nothing stays.
#[test]
fn t14_a_vault_value_in_a_write_is_refused() {
    let s = setup();
    let value = format!("ghp_{}", aurelius_domain::CommandId::new().0.simple());
    let _vault = Vault::with(&value);
    let id = create(&s, "Clean", "Nothing secret.", vec![]).unwrap();
    let events = guideline_events(&s);
    assert_secret(run(&s, Command::CreateGuideline(CreateGuideline { title: "T".into(), text: format!("token {value}"), links: vec![] })));
    assert_secret(run(&s, Command::CreateGuideline(CreateGuideline { title: format!("T {value}"), text: "x".into(), links: vec![] })));
    assert_secret(run(&s, Command::UpdateGuideline(UpdateGuideline { guideline_id: id, title: None, text: Some(value.clone()) })));
    let ann = employee(&s, "Ann");
    let p = proposal(&s, Some(id), Some(1), ann, "Clean", "fine", vec![]);
    assert_secret(run(&s, Command::DecideGuidelineProposal(DecideGuidelineProposal { text: Some(format!("now {value}")), ..decide(p, true) })));
    assert_eq!((guideline_events(&s), current(&s, id).version), (events, 1));
    let receipts: i64 = s.service.store().read(|c| Ok(c.query_row("SELECT COUNT(*) FROM command_receipts WHERE method LIKE 'guideline.%'", [], |r| r.get(0))?)).unwrap();
    assert_eq!(receipts, 1, "only the clean create has a receipt");
}

/// T16 (E7): approved as it is, the stored text is checked against today's vault.
#[test]
fn t16_approving_as_is_checks_the_stored_text() {
    let s = setup();
    let ann = employee(&s, "Ann");
    let value = format!("sk-{}", aurelius_domain::CommandId::new().0.simple());
    let p = proposal(&s, None, None, ann, "Deploys", &format!("Use the key {value}."), vec![GuidelineTarget::Employee(ann)]);
    let _vault = Vault::with(&value);
    assert_secret(run(&s, Command::DecideGuidelineProposal(decide(p, true))));
    assert!(s.service.store().read(|c| store::list(c, true)).unwrap().is_empty(), "no guideline was made");
    assert_eq!(s.service.store().read(|c| store::proposal(c, p)).unwrap().unwrap().status, ProposalStatus::Pending);
    // Rejecting it is fine: nothing is stored.
    run(&s, Command::DecideGuidelineProposal(decide(p, false))).unwrap();
}

/// T17 (E8): an old version that holds what is a vault value now cannot come back.
#[test]
fn t17_restoring_checks_the_old_text() {
    let s = setup();
    let value = format!("pw-{}", aurelius_domain::CommandId::new().0.simple());
    let id = create(&s, "Ops", &format!("The password is {value}."), vec![]).unwrap();
    run(&s, Command::UpdateGuideline(UpdateGuideline { guideline_id: id, title: None, text: Some("Ask the board.".into()) })).unwrap();
    let _vault = Vault::with(&value);
    assert_secret(run(&s, Command::RestoreGuidelineVersion(RestoreGuidelineVersion { guideline_id: id, version: 1 })));
    assert_eq!(current(&s, id).version, 2);
    // restore from the archive too, when the current text holds it.
    let other = create(&s, "Old", "clean", vec![]).unwrap();
    s.service.store().write(move |tx| Ok(tx.execute("UPDATE guidelines SET archived_at_ms = 1 WHERE id = ?1", [other.to_string()])?)).unwrap();
    let held = s.service.store().read(move |c| Ok(c.query_row("SELECT text FROM guideline_versions WHERE guideline_id = ?1", [other.to_string()], |r| r.get::<_, String>(0))?)).unwrap();
    assert_eq!(held, "clean");
    run(&s, Command::RestoreGuideline(RestoreGuideline { guideline_id: other })).unwrap();
}

/// E12 and T23: links of a new guideline, a change keeps its links, a stale base, an archived proposer.
#[test]
fn deciding_proposals() {
    let s = setup();
    let ann = employee(&s, "Ann");
    let bob = employee(&s, "Bob");
    let suggested = vec![GuidelineTarget::Employee(ann), GuidelineTarget::Project(s.project)];

    // A new one with the server's suggested links.
    let p = proposal(&s, None, None, ann, "Reviews", "Ask Trace.", suggested.clone());
    let CommandResult::GuidelineProposalDecided { status, guideline_id: Some(made), .. } = run(&s, Command::DecideGuidelineProposal(decide(p, true))).unwrap() else { panic!() };
    assert_eq!(status, ProposalStatus::Approved);
    assert_eq!(view(&s, made).links, suggested);
    let v = current(&s, made);
    assert_eq!((v.source, v.proposal_id), (GuidelineSource::Proposal, Some(p)));
    assert_eq!(run(&s, Command::DecideGuidelineProposal(decide(p, true))).unwrap_err().code, ErrorCode::Validation, "no longer pending");

    // A new one with the board member's links and edits: only those.
    let p = proposal(&s, None, None, ann, "Tests", "Run them.", suggested.clone());
    let edited = DecideGuidelineProposal { title: Some("Tests first".into()), links: Some(vec![GuidelineTarget::Employee(bob)]), ..decide(p, true) };
    let CommandResult::GuidelineProposalDecided { guideline_id: Some(made), .. } = run(&s, Command::DecideGuidelineProposal(edited)).unwrap() else { panic!() };
    assert_eq!((view(&s, made).links, view(&s, made).title), (vec![GuidelineTarget::Employee(bob)], "Tests first".to_string()));

    // A change: a new version, the links stay; links with it are refused.
    let p = proposal(&s, Some(made), Some(1), ann, "Tests first", "Run them twice.", vec![]);
    let with_links = DecideGuidelineProposal { links: Some(vec![GuidelineTarget::Employee(ann)]), ..decide(p, true) };
    assert_eq!(run(&s, Command::DecideGuidelineProposal(with_links)).unwrap_err().code, ErrorCode::Validation);
    run(&s, Command::DecideGuidelineProposal(decide(p, true))).unwrap();
    assert_eq!((current(&s, made).version, view(&s, made).links.clone()), (2, vec![GuidelineTarget::Employee(bob)]));

    // T23: the guideline moved on since the proposal's base.
    let p = proposal(&s, Some(made), Some(1), ann, "Tests first", "Stale.", vec![]);
    assert_eq!(run(&s, Command::DecideGuidelineProposal(decide(p, true))).unwrap_err().code, ErrorCode::VersionConflict);
    assert_eq!(current(&s, made).version, 2);

    // T23: an archived proposer: the proposal is withdrawn.
    let p = proposal(&s, None, None, bob, "Bob's", "Rule.", vec![]);
    s.service.store().write(move |tx| Ok(tx.execute("UPDATE employees SET status = 'archived' WHERE id = ?1", [bob.to_string()])?)).unwrap();
    let err = run(&s, Command::DecideGuidelineProposal(decide(p, true))).unwrap_err();
    assert_eq!(err.code, ErrorCode::Validation);
    assert!(err.message.contains("withdrawn"), "{}", err.message);
    assert_eq!(s.service.store().read(|c| store::proposal(c, p)).unwrap().unwrap().status, ProposalStatus::Withdrawn);
    // T23: withdrawn when its proposer was archived: deciding says withdrawn, an approval or a rejection.
    let carl = employee(&s, "Carl");
    let p = proposal(&s, None, None, carl, "Carl's", "Rule.", vec![]);
    // What `org::on_employee_archived` does (the command itself: guideline_proposal_tests).
    s.service
        .store()
        .write(move |tx| {
            tx.execute("UPDATE employees SET status = 'archived' WHERE id = ?1", [carl.to_string()])?;
            store::withdraw_for_employee(tx, carl, 9)
        })
        .unwrap();
    assert_eq!(s.service.store().read(|c| store::proposal(c, p)).unwrap().unwrap().status, ProposalStatus::Withdrawn);
    for approve in [true, false] {
        let err = run(&s, Command::DecideGuidelineProposal(decide(p, approve))).unwrap_err();
        assert!(err.message.contains("withdrawn"), "{}", err.message);
    }

    // A rejection with a note, and its event.
    let p = proposal(&s, None, None, ann, "Nope", "No.", vec![]);
    let reject = DecideGuidelineProposal { note: Some("Not now".into()), ..decide(p, false) };
    assert_eq!(
        run(&s, Command::DecideGuidelineProposal(reject)).unwrap(),
        CommandResult::GuidelineProposalDecided { proposal_id: p, status: ProposalStatus::Rejected, guideline_id: None }
    );
    let stored = s.service.store().read(|c| store::proposal(c, p)).unwrap().unwrap();
    assert_eq!((stored.decision_note.as_deref(), stored.decided_by.map(|u| u.id)), (Some("Not now"), Some(s.user)));
    let meta: String = s
        .service
        .store()
        .read(move |c| Ok(c.query_row("SELECT metadata_json FROM events WHERE aggregate_id = ?1 AND type = 'guideline.proposal_decided'", [p.to_string()], |r| r.get(0))?))
        .unwrap();
    let meta: GuidelineProposalDecidedMeta = serde_json::from_str(&meta).unwrap();
    assert_eq!((meta.proposal_id, meta.approved), (p, false));
    assert_eq!(run(&s, Command::DecideGuidelineProposal(decide(GuidelineProposalId::new(), false))).unwrap_err().code, ErrorCode::NotFound);
}
