//! Guidelines storage (migration 23): the library, a run's set in B3 order, the
//! limit per combination, proposals, run snapshots, the seed and the starter set.

use crate::guidelines::{self, GuidelineOverflow, NewGuidelineProposal, NewVersion, SeedReport};
use crate::{StorageError, Store, employees, settings};
use aurelius_domain::guideline::{
    EffectiveGuideline, GuidelineLevel, GuidelineSource, GuidelineTarget, MAX_GUIDELINE_CHARS, MAX_RUN_CHARS, MAX_TITLE_CHARS,
};
use aurelius_domain::proposal::{ProposalEvidence, ProposalStatus};
use aurelius_domain::{ConversationId, EmployeeId, GuidelineId, GuidelineProposalId, ProjectId, RunId, UserId};
use rusqlite::params;

struct World {
    _dir: tempfile::TempDir,
    store: Store,
    alex: UserId,
}

fn world() -> World {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let alex = UserId::new();
    store
        .write(move |tx| {
            tx.execute(
                "INSERT INTO users(id, company_id, username, password_hash, created_at_ms, updated_at_ms) SELECT ?1, id, 'Alex', '$argon2id$x', 1, 1 FROM company",
                [alex.to_string()],
            )?;
            Ok(())
        })
        .unwrap();
    World { _dir: dir, store, alex }
}

impl World {
    fn project(&self, name: &'static str, status: &'static str) -> ProjectId {
        let id = ProjectId::new();
        self.store
            .write(move |tx| {
                let global: String = tx.query_row("SELECT id FROM scopes WHERE kind = 'global'", [], |r| r.get(0))?;
                tx.execute("INSERT INTO scopes(id, kind, parent_id, owner_id) VALUES (?1, 'project', ?2, ?3)", params![format!("s-{id}"), global, id.to_string()])?;
                tx.execute(
                    "INSERT INTO projects(id, scope_id, name, status, version, created_at_ms, updated_at_ms) VALUES (?1, ?2, ?3, ?4, 1, 1, 1)",
                    params![id.to_string(), format!("s-{id}"), name, status],
                )?;
                Ok(())
            })
            .unwrap();
        id
    }

    fn employee(&self, name: &'static str) -> EmployeeId {
        let id = EmployeeId::new();
        self.store
            .write(move |tx| employees::insert_employee(tx, id, name, "Dev", None, None, None, &serde_json::json!({}), ConversationId::new()))
            .unwrap();
        id
    }

    fn archive_employee(&self, id: EmployeeId) {
        self.store.write(move |tx| Ok(tx.execute("UPDATE employees SET status = 'archived' WHERE id = ?1", [id.to_string()])?)).unwrap();
    }

    fn create(&self, title: &'static str, text: String) -> GuidelineId {
        self.try_create(title, text).unwrap()
    }

    fn try_create(&self, title: &'static str, text: String) -> Result<GuidelineId, StorageError> {
        let (id, alex) = (GuidelineId::new(), self.alex);
        self.store.write(move |tx| {
            guidelines::create(tx, id, new_version(title, &text, GuidelineSource::Created, alex))?;
            Ok(id)
        })
    }

    fn edit(&self, id: GuidelineId, expected: u32, title: &'static str, text: &'static str) -> Result<u32, StorageError> {
        let alex = self.alex;
        self.store.write(move |tx| Ok(guidelines::add_version(tx, id, expected, new_version(title, text, GuidelineSource::Edited, alex))?.version))
    }

    fn link(&self, id: GuidelineId, target: GuidelineTarget) -> bool {
        let alex = self.alex;
        self.store.write(move |tx| guidelines::link(tx, id, target, alex, 5)).unwrap()
    }

    fn archive(&self, id: GuidelineId) {
        let alex = self.alex;
        self.store.write(move |tx| guidelines::archive(tx, id, alex, 7)).unwrap();
    }

    fn task(&self, project: ProjectId, employee: EmployeeId, status: &'static str) {
        self.store
            .write(move |tx| {
                tx.execute("INSERT OR IGNORE INTO blobs(id, storage_key, sha256, byte_length, media_type, created_at_ms) VALUES ('b', 'k', 's', 0, 'text/plain', 1)", [])?;
                tx.execute(
                    "INSERT INTO tasks(id, project_id, employee_id, title, brief_blob_id, status, version, created_at_ms, updated_at_ms) VALUES (?1, ?2, ?3, 'Work', 'b', ?4, 1, 1, 1)",
                    params![aurelius_domain::TaskId::new().to_string(), project.to_string(), employee.to_string(), status],
                )?;
                Ok(())
            })
            .unwrap();
    }

    fn seed(&self) -> SeedReport {
        self.store.write(|tx| guidelines::seed_guidelines_once(tx, 50)).unwrap()
    }
}

fn new_version<'a>(title: &'a str, text: &'a str, source: GuidelineSource, by: UserId) -> NewVersion<'a> {
    NewVersion { title, text, source, proposal_id: None, restored_from: None, by_user: Some(by), now_ms: 10 }
}

fn rejected<T: std::fmt::Debug>(result: Result<T, StorageError>, why: &str) {
    match result {
        Err(StorageError::Rejected(r)) => assert_eq!(r, why),
        other => panic!("expected Rejected({why}), got {other:?}"),
    }
}

fn titles(set: &[EffectiveGuideline]) -> Vec<(&str, GuidelineLevel)> {
    set.iter().map(|g| (g.title.as_str(), g.level)).collect()
}

// ---------------------------------------------------------------- library

#[test]
fn a_guideline_gets_versions_and_a_stale_version_is_a_conflict() {
    let w = world();
    let id = w.create("Style", "Be brief.".into());
    assert_eq!(w.edit(id, 1, "Style", "Be briefer.").unwrap(), 2);
    assert_eq!(w.edit(id, 2, "House style", "Be briefest.").unwrap(), 3);
    // Two edits made version 3; a client that saw version 2 is refused.
    rejected(w.edit(id, 2, "Style", "Old view."), guidelines::VERSION_CONFLICT);

    let versions = w.store.read(|c| guidelines::versions(c, id)).unwrap();
    assert_eq!(versions.iter().map(|v| (v.version, v.title.as_str(), v.source)).collect::<Vec<_>>(), [
        (3, "House style", GuidelineSource::Edited),
        (2, "Style", GuidelineSource::Edited),
        (1, "Style", GuidelineSource::Created)
    ]);
    assert_eq!(versions[0].by.as_ref().map(|u| (u.id, u.username.as_str())), Some((w.alex, "Alex")));
    let current = w.store.read(|c| guidelines::get(c, id, None)).unwrap().unwrap();
    assert_eq!((current.version, current.text.as_str()), (3, "Be briefest."));
    let first = w.store.read(|c| guidelines::get(c, id, Some(1))).unwrap().unwrap();
    assert_eq!(first.text, "Be brief.");
    assert_eq!(w.store.read(|c| guidelines::get(c, id, Some(9))).unwrap(), None);
    assert_eq!(w.store.read(|c| guidelines::get(c, GuidelineId::new(), None)).unwrap(), None);

    // A restore copies an older version as the newest and names it.
    let alex = w.alex;
    let restored = w
        .store
        .write(move |tx| {
            guidelines::add_version(tx, id, 3, NewVersion { restored_from: Some(1), ..new_version("Style", "Be brief.", GuidelineSource::Restored, alex) })
        })
        .unwrap();
    assert_eq!((restored.version, restored.restored_from), (4, Some(1)));
    rejected(
        w.store.write(move |tx| {
            guidelines::add_version(tx, id, 4, NewVersion { restored_from: Some(9), ..new_version("Style", "x", GuidelineSource::Restored, alex) })
        }),
        guidelines::VERSION_NOT_FOUND,
    );
    rejected(w.edit(GuidelineId::new(), 1, "Style", "x"), guidelines::NOT_FOUND);
}

#[test]
fn titles_and_texts_are_checked_before_anything_is_written() {
    let w = world();
    rejected(w.try_create("", "x".into()), guidelines::INVALID_TITLE);
    rejected(w.try_create(" ", "x".into()), guidelines::INVALID_TITLE);
    rejected(w.try_create("Style", "   ".into()), guidelines::INVALID_TEXT);
    rejected(w.try_create("Style", "x".repeat(MAX_GUIDELINE_CHARS + 1)), guidelines::INVALID_TEXT);
    let long: &'static str = Box::leak("t".repeat(MAX_TITLE_CHARS + 1).into_boxed_str());
    rejected(w.try_create(long, "x".into()), guidelines::INVALID_TITLE);
    // The limit counts characters: 8,000 three-byte characters fit.
    let id = w.create("Style", "€".repeat(MAX_GUIDELINE_CHARS));
    assert_eq!(w.store.read(|c| guidelines::view(c, id)).unwrap().unwrap().chars, MAX_GUIDELINE_CHARS as u32);
    assert_eq!(w.store.read(|c| guidelines::list(c, true)).unwrap().len(), 1);
}

#[test]
fn a_title_is_unique_without_case_among_guidelines_that_are_not_archived() {
    let w = world();
    let style = w.create("Rust style", "a".into());
    rejected(w.try_create("rust STYLE ", "b".into()), guidelines::TITLE_TAKEN);
    let other = w.create("Testing", "c".into());
    rejected(w.edit(other, 1, "RUST style", "c"), guidelines::TITLE_TAKEN);
    // Its own title is not taken.
    assert_eq!(w.edit(style, 1, "Rust Style", "a2").unwrap(), 2);
    assert!(w.store.read(move |c| guidelines::title_taken(c, " rust style", None)).unwrap());
    assert!(!w.store.read(move |c| guidelines::title_taken(c, "rust style", Some(style))).unwrap());

    // Archived: the title is free; the archived one cannot come back while it is taken.
    w.archive(style);
    rejected(w.store.write(move |tx| guidelines::archive(tx, style, UserId::new(), 8)), guidelines::ARCHIVED);
    let again = w.create("Rust style", "new".into());
    let alex = w.alex;
    rejected(w.store.write(move |tx| guidelines::restore(tx, style, alex, 9)), guidelines::TITLE_TAKEN);
    w.archive(again);
    w.store.write(move |tx| guidelines::restore(tx, style, alex, 9)).unwrap();
    rejected(w.store.write(move |tx| guidelines::restore(tx, style, alex, 9)), guidelines::NOT_ARCHIVED);
    rejected(w.edit(again, 1, "Rust style", "x"), guidelines::ARCHIVED);
}

#[test]
fn the_library_lists_links_proposals_and_archived_ones_last() {
    let w = world();
    let (alpha, beta) = (w.project("Alpha", "active"), w.project("Beta", "active"));
    let (sam, kim) = (w.employee("Sam"), w.employee("Kim"));
    let b = w.create("beta rules", "bb".into());
    let a = w.create("Alpha rules", "a".into());
    let z = w.create("Zulu rules", "zzz".into());
    w.link(a, GuidelineTarget::Project(beta));
    w.link(a, GuidelineTarget::Employee(sam));
    w.link(a, GuidelineTarget::Project(alpha));
    w.link(a, GuidelineTarget::Employee(kim));
    w.archive(z);
    propose(&w, Some(a), Some(1), sam, "Alpha rules");

    let list = w.store.read(|c| guidelines::list(c, false)).unwrap();
    assert_eq!(list.iter().map(|g| g.title.as_str()).collect::<Vec<_>>(), ["Alpha rules", "beta rules"]);
    let first = &list[0];
    // Employees first, then projects, each by name.
    assert_eq!(first.links, [GuidelineTarget::Employee(kim), GuidelineTarget::Employee(sam), GuidelineTarget::Project(alpha), GuidelineTarget::Project(beta)]);
    assert_eq!((first.version, first.chars, first.open_proposals, first.archived), (1, 1, 1, false));
    assert_eq!(first.updated_by.as_ref().map(|u| u.username.as_str()), Some("Alex"));
    assert_eq!(list[1].id, b);
    assert!(list[1].links.is_empty());

    let all = w.store.read(|c| guidelines::list(c, true)).unwrap();
    assert_eq!(all.iter().map(|g| (g.title.as_str(), g.archived)).collect::<Vec<_>>(), [("Alpha rules", false), ("beta rules", false), ("Zulu rules", true)]);
    assert_eq!(all[2].updated_at_ms, 7);
}

#[test]
fn link_and_unlink_are_idempotent_and_an_archived_guideline_is_not_linked() {
    let w = world();
    let (project, sam) = (w.project("Alpha", "active"), w.employee("Sam"));
    let id = w.create("Style", "x".into());
    assert!(w.link(id, GuidelineTarget::Employee(sam)));
    assert!(!w.link(id, GuidelineTarget::Employee(sam)));
    assert!(w.link(id, GuidelineTarget::Project(project)));
    let unlink = |target| w.store.write(move |tx| guidelines::unlink(tx, id, target)).unwrap();
    assert!(unlink(GuidelineTarget::Employee(sam)));
    assert!(!unlink(GuidelineTarget::Employee(sam)));
    assert_eq!(w.store.read(|c| guidelines::view(c, id)).unwrap().unwrap().links, [GuidelineTarget::Project(project)]);

    w.archive(id);
    let alex = w.alex;
    rejected(w.store.write(move |tx| guidelines::link(tx, id, GuidelineTarget::Employee(sam), alex, 5)), guidelines::ARCHIVED);
    assert!(unlink(GuidelineTarget::Project(project)), "an archived guideline can still be unlinked");
    rejected(w.store.write(move |tx| guidelines::link(tx, GuidelineId::new(), GuidelineTarget::Employee(sam), alex, 5)), guidelines::NOT_FOUND);
}

// ---------------------------------------------------------------- a run's set and the limit

#[test]
fn a_runs_set_is_employee_level_then_project_level_by_title() {
    let w = world();
    let (project, other) = (w.project("Alpha", "active"), w.project("Beta", "active"));
    let (sam, kim) = (w.employee("Sam"), w.employee("Kim"));
    let zulu = w.create("zulu", "z".repeat(10));
    let alpha = w.create("Alpha", "a".repeat(20));
    let shared = w.create("Mike", "m".repeat(30));
    let layers = w.create("Layers", "l".repeat(40));
    let bravo = w.create("bravo", "b".repeat(50));
    let gone = w.create("Archived", "x".into());
    let elsewhere = w.create("Elsewhere", "e".into());
    for g in [zulu, alpha, shared, gone] {
        w.link(g, GuidelineTarget::Employee(sam));
    }
    for g in [shared, layers, bravo, gone] {
        w.link(g, GuidelineTarget::Project(project));
    }
    w.link(elsewhere, GuidelineTarget::Project(other));
    w.link(elsewhere, GuidelineTarget::Employee(kim));
    w.archive(gone);

    let set = w.store.read(move |c| guidelines::effective(c, Some(sam), Some(project))).unwrap();
    // Employee level by title, then project level by title; Mike (linked to
    // both) once, at project level; the archived one never.
    assert_eq!(titles(&set.guidelines), [
        ("Alpha", GuidelineLevel::Employee),
        ("zulu", GuidelineLevel::Employee),
        ("bravo", GuidelineLevel::Project),
        ("Layers", GuidelineLevel::Project),
        ("Mike", GuidelineLevel::Project)
    ]);
    assert_eq!((set.total_chars, set.limit_chars, set.dropped.len()), (150, MAX_RUN_CHARS as u32, 0));
    assert_eq!((set.employee_id, set.project_id), (Some(sam), Some(project)));
    assert_eq!(set.guidelines[0].text, "a".repeat(20));
    assert_eq!(set.guidelines[0].version, 1);

    // No project: only the employee's (Mike now at employee level).
    let own = w.store.read(move |c| guidelines::effective(c, Some(sam), None)).unwrap();
    assert_eq!(titles(&own.guidelines), [("Alpha", GuidelineLevel::Employee), ("Mike", GuidelineLevel::Employee), ("zulu", GuidelineLevel::Employee)]);
    // No employee: only the project's.
    let assistant = w.store.read(move |c| guidelines::effective(c, None, Some(project))).unwrap();
    assert_eq!(titles(&assistant.guidelines), [("bravo", GuidelineLevel::Project), ("Layers", GuidelineLevel::Project), ("Mike", GuidelineLevel::Project)]);
    let nobody = w.store.read(|c| guidelines::effective(c, None, None)).unwrap();
    assert!(nobody.guidelines.is_empty() && nobody.total_chars == 0);
    // An edit shows its newest version.
    w.edit(zulu, 1, "zulu", "zz").unwrap();
    let set = w.store.read(move |c| guidelines::effective(c, Some(sam), Some(project))).unwrap();
    assert_eq!((set.guidelines[1].version, set.guidelines[1].chars, set.total_chars), (2, 2, 142));
}

#[test]
fn combinations_over_finds_every_employee_and_project_pair_over_the_limit() {
    let w = world();
    let (alpha, paused, old) = (w.project("Alpha", "active"), w.project("Paused", "paused"), w.project("Old", "archived"));
    let (sam, kim, gone) = (w.employee("Sam"), w.employee("Kim"), w.employee("Gone"));
    let own = w.create("Own", "o".repeat(6_000));
    let project = w.create("Project", "p".repeat(6_000));
    let shared = w.create("Shared", "s".repeat(6_000));
    w.link(own, GuidelineTarget::Employee(sam));
    w.link(shared, GuidelineTarget::Employee(sam));
    w.link(project, GuidelineTarget::Project(alpha));
    w.link(shared, GuidelineTarget::Project(alpha));
    w.link(own, GuidelineTarget::Project(paused));
    w.link(project, GuidelineTarget::Project(paused));
    // An archived employee and project never count.
    for g in [own, project, shared] {
        w.link(g, GuidelineTarget::Employee(gone));
        w.link(g, GuidelineTarget::Project(old));
    }
    w.archive_employee(gone);

    let over = |limit: usize| w.store.read(move |c| guidelines::combinations_over(c, limit)).unwrap();
    // Sam × Alpha = Own + Shared + Project (Shared once) = 18,000; Sam × Paused
    // = Own + Shared + Project = 18,000; Sam alone 12,000; Alpha alone 12,000.
    assert_eq!(over(MAX_RUN_CHARS), [
        GuidelineOverflow { employee_id: Some(sam), project_id: Some(alpha), total_chars: 18_000 },
        GuidelineOverflow { employee_id: Some(sam), project_id: Some(paused), total_chars: 18_000 },
    ]);
    // Lower: the project sets alone and the employees without project too, in order.
    let all = over(11_999);
    let pairs: Vec<(Option<EmployeeId>, Option<ProjectId>, u32)> = all.iter().map(|o| (o.employee_id, o.project_id, o.total_chars)).collect();
    assert_eq!(pairs, [
        (None, Some(alpha), 12_000),
        (None, Some(paused), 12_000),
        (Some(kim), Some(alpha), 12_000),
        (Some(kim), Some(paused), 12_000),
        (Some(sam), None, 12_000),
        (Some(sam), Some(alpha), 18_000),
        (Some(sam), Some(paused), 18_000),
    ]);
    // At the limit is not over it.
    assert!(over(18_000).is_empty());
    // An archived guideline drops out of every sum.
    w.archive(shared);
    assert_eq!(over(MAX_RUN_CHARS), []);
    // Left at 12,000: Paused alone (Own + Project), Kim × Paused, Sam × Alpha, Sam × Paused.
    assert_eq!(over(11_999).len(), 4);
}

#[test]
fn combinations_over_sees_an_uncommitted_write_in_the_same_transaction() {
    let w = world();
    let alpha = w.project("Alpha", "active");
    let sam = w.employee("Sam");
    let big = w.create("Big", "b".repeat(8_000));
    let other = w.create("Other", "o".repeat(8_000));
    w.link(big, GuidelineTarget::Employee(sam));
    w.link(other, GuidelineTarget::Project(alpha));
    // At the limit: Sam × Alpha = 16,000.
    assert!(w.store.read(|c| guidelines::combinations_over(c, MAX_RUN_CHARS)).unwrap().is_empty());
    let alex = w.alex;
    // The application's pattern: write, check, roll back on an overflow.
    let result = w.store.write(move |tx| {
        let extra = GuidelineId::new();
        guidelines::create(tx, extra, new_version("Extra", "e", GuidelineSource::Created, alex))?;
        guidelines::link(tx, extra, GuidelineTarget::Project(alpha), alex, 5)?;
        let over = guidelines::combinations_over(tx, MAX_RUN_CHARS)?;
        assert_eq!(over, [GuidelineOverflow { employee_id: Some(sam), project_id: Some(alpha), total_chars: 16_001 }]);
        Err::<(), _>(StorageError::Rejected("guideline_too_large".into()))
    });
    assert!(result.is_err());
    assert_eq!(w.store.read(|c| guidelines::list(c, true)).unwrap().len(), 2, "rolled back");
}

#[test]
fn affected_employees_are_the_linked_ones_and_those_with_open_work_in_a_linked_project() {
    let w = world();
    let (alpha, old) = (w.project("Alpha", "active"), w.project("Old", "archived"));
    let (sam, kim, lee, ray, gone) = (w.employee("Sam"), w.employee("Kim"), w.employee("Lee"), w.employee("Ray"), w.employee("Gone"));
    let id = w.create("Style", "x".into());
    w.link(id, GuidelineTarget::Employee(sam));
    w.link(id, GuidelineTarget::Employee(gone));
    w.link(id, GuidelineTarget::Project(alpha));
    w.link(id, GuidelineTarget::Project(old));
    w.task(alpha, kim, "running");
    w.task(alpha, sam, "queued");
    w.task(alpha, lee, "succeeded");
    w.task(old, ray, "queued");
    w.archive_employee(gone);
    // Kim (open task) and Sam (linked, once); not Lee (done), Ray (archived project) or Gone.
    assert_eq!(w.store.read(move |c| guidelines::affected_employees(c, id)).unwrap(), [kim, sam]);
}

// ---------------------------------------------------------------- proposals

fn propose(w: &World, guideline: Option<GuidelineId>, base: Option<u32>, by: EmployeeId, title: &'static str) -> GuidelineProposalId {
    let id = GuidelineProposalId::new();
    let (conversation, run) = (ConversationId::new(), RunId::new());
    let links = match guideline {
        None => vec![GuidelineTarget::Employee(by)],
        Some(_) => vec![],
    };
    w.store
        .write(move |tx| {
            guidelines::insert_proposal(
                tx,
                NewGuidelineProposal {
                    id,
                    guideline_id: guideline,
                    base_version: base,
                    proposed_by: by,
                    title,
                    text: "Prefer small functions.",
                    reason: "Seen twice in review.",
                    evidence: &[ProposalEvidence { kind: "message".into(), id: "m1".into(), label: "review".into() }],
                    suggested_links: &links,
                    origin_conversation_id: Some(conversation),
                    run_id: Some(run),
                    now_ms: 100 + by.to_string().len() as i64,
                },
            )
        })
        .unwrap();
    id
}

#[test]
fn proposals_are_stored_counted_decided_and_withdrawn() {
    let w = world();
    let (sam, kim) = (w.employee("Sam"), w.employee("Kim"));
    let style = w.create("Style", "x".into());
    let change = propose(&w, Some(style), Some(1), sam, "Style");
    let new1 = propose(&w, None, None, sam, "Small functions");
    let new2 = propose(&w, None, None, sam, "Naming");
    let other = propose(&w, None, None, kim, "Logging");

    let read = |id| w.store.read(move |c| guidelines::proposal(c, id)).unwrap().unwrap();
    let p = read(new1);
    assert_eq!((p.guideline_id, p.base_version, p.proposed_by, p.status), (None, None, sam, ProposalStatus::Pending));
    assert_eq!(p.suggested_links, [GuidelineTarget::Employee(sam)]);
    assert_eq!(p.evidence.len(), 1);
    assert!(p.origin_conversation_id.is_some() && p.run_id.is_some() && p.decided_by.is_none());
    assert_eq!(read(change).base_version, Some(1));
    assert_eq!(w.store.read(|c| guidelines::proposal(c, GuidelineProposalId::new())).unwrap(), None);

    assert_eq!(w.store.read(move |c| guidelines::count_open_for_guideline(c, style)).unwrap(), 1);
    assert_eq!(w.store.read(move |c| guidelines::count_open_new_for_employee(c, sam)).unwrap(), 2);
    assert_eq!(w.store.read(move |c| guidelines::count_open_new_for_employee(c, kim)).unwrap(), 1);
    assert_eq!(w.store.read(|c| guidelines::list(c, false)).unwrap()[0].open_proposals, 1);

    let alex = w.alex;
    let store = &w.store;
    let decide = move |id, status, note: Option<&'static str>| store.write(move |tx| guidelines::decide_proposal(tx, id, status, note, alex, 200));
    assert!(decide(change, ProposalStatus::Approved, Some("Thanks")).unwrap());
    assert!(!decide(change, ProposalStatus::Rejected, None).unwrap(), "already decided");
    rejected(decide(new1, ProposalStatus::Withdrawn, None), guidelines::INVALID_DECISION);
    rejected(decide(new1, ProposalStatus::Pending, None), guidelines::INVALID_DECISION);
    rejected(decide(GuidelineProposalId::new(), ProposalStatus::Rejected, None), guidelines::PROPOSAL_NOT_FOUND);
    assert!(decide(other, ProposalStatus::Rejected, Some("Not now")).unwrap());
    let decided = read(change);
    assert_eq!((decided.status, decided.decision_note.as_deref(), decided.decided_at_ms), (ProposalStatus::Approved, Some("Thanks"), Some(200)));
    assert_eq!(decided.decided_by.map(|u| u.username), Some("Alex".to_string()));
    assert_eq!(w.store.read(move |c| guidelines::count_open_for_guideline(c, style)).unwrap(), 0);

    let mut withdrawn = w.store.write(move |tx| guidelines::withdraw_for_employee(tx, sam, 300)).unwrap();
    withdrawn.sort_by_key(|id| id.to_string());
    let mut expected = vec![new1, new2];
    expected.sort_by_key(|id| id.to_string());
    assert_eq!(withdrawn, expected);
    assert_eq!(read(new2).status, ProposalStatus::Withdrawn);
    assert_eq!(read(new2).decided_by, None);
    assert_eq!(w.store.read(move |c| guidelines::count_open_new_for_employee(c, sam)).unwrap(), 0);

    let pending = w.store.read(|c| guidelines::proposals(c, Some(ProposalStatus::Pending))).unwrap();
    assert!(pending.is_empty());
    let all = w.store.read(|c| guidelines::proposals(c, None)).unwrap();
    assert_eq!(all.len(), 4);
    // Decided ones newest first: the withdrawals (300), then the decisions (200).
    assert!(all[..2].iter().all(|p| p.status == ProposalStatus::Withdrawn));
    let rejected_ones = w.store.read(|c| guidelines::proposals(c, Some(ProposalStatus::Rejected))).unwrap();
    assert_eq!(rejected_ones.iter().map(|p| p.id).collect::<Vec<_>>(), [other]);
}

#[test]
fn pending_proposals_are_listed_oldest_first() {
    let w = world();
    let sam = w.employee("Sam");
    let first = propose(&w, None, None, sam, "One");
    let second = propose(&w, None, None, sam, "Two");
    w.store.write(move |tx| Ok(tx.execute("UPDATE guideline_proposals SET created_at_ms = 1 WHERE id = ?1", [second.to_string()])?)).unwrap();
    let pending = w.store.read(|c| guidelines::proposals(c, Some(ProposalStatus::Pending))).unwrap();
    assert_eq!(pending.iter().map(|p| p.id).collect::<Vec<_>>(), [second, first]);
}

// ---------------------------------------------------------------- run snapshots

/// A run in the employee's own chat.
fn run(w: &World, employee: EmployeeId) -> RunId {
    let run = RunId::new();
    let conversation = w.store.read(move |c| employees::main_chat(c, employee)).unwrap().unwrap();
    w.store
        .write(move |tx| {
            tx.execute(
                "INSERT INTO runs(id, conversation_id, attempt, runtime_kind, model_id, resolved_config_json, status) VALUES (?1, ?2, 1, 'fake', 'm', '{}', 'running')",
                params![run.to_string(), conversation.to_string()],
            )?;
            Ok(())
        })
        .unwrap();
    run
}

#[test]
fn a_run_keeps_the_versions_it_got_in_block_order() {
    let w = world();
    let (project, sam) = (w.project("Alpha", "active"), w.employee("Sam"));
    let style = w.create("Style", "s".repeat(5));
    let layers = w.create("Layers", "l".repeat(7));
    let all = w.create("All", "a".into());
    w.link(style, GuidelineTarget::Employee(sam));
    w.link(all, GuidelineTarget::Employee(sam));
    w.link(layers, GuidelineTarget::Project(project));
    let run = run(&w, sam);
    let set = w.store.read(move |c| guidelines::effective(c, Some(sam), Some(project))).unwrap();
    let got = set.guidelines.clone();
    w.store.write(move |tx| guidelines::insert_for_run(tx, run, &got)).unwrap();

    // An edit after the start does not change the snapshot.
    w.edit(style, 1, "Style", "changed").unwrap();
    let snapshot = w.store.read(move |c| guidelines::for_run(c, run)).unwrap();
    assert_eq!(
        snapshot.iter().map(|g| (g.title.as_str(), g.version, g.level, g.chars)).collect::<Vec<_>>(),
        [("All", 1, GuidelineLevel::Employee, 1), ("Style", 1, GuidelineLevel::Employee, 5), ("Layers", 1, GuidelineLevel::Project, 7)]
    );
    assert!(snapshot.iter().all(|g| g.run_id == run));
    assert_eq!(snapshot[1].guideline_id, style);

    // Writing it again replaces it.
    let fewer = set.guidelines[..1].to_vec();
    w.store.write(move |tx| guidelines::insert_for_run(tx, run, &fewer)).unwrap();
    assert_eq!(w.store.read(move |c| guidelines::for_run(c, run)).unwrap().len(), 1);
    assert!(w.store.read(|c| guidelines::for_run(c, RunId::new())).unwrap().is_empty());
    // A deleted run takes its snapshot along.
    w.store.write(move |tx| Ok(tx.execute("DELETE FROM runs WHERE id = ?1", [run.to_string()])?)).unwrap();
    assert_eq!(w.store.read(|c| Ok(c.query_row("SELECT COUNT(*) FROM run_guidelines", [], |r| r.get::<_, i64>(0))?)).unwrap(), 0);
}

// ---------------------------------------------------------------- the seed and the starter set

/// The sizes of docs/product/guidelines/seed.md (characters, with line ends).
const SEED_SIZES: [(&str, u32); 8] = [("g1", 1477), ("g2", 790), ("g3", 743), ("g4", 1188), ("g5", 849), ("g6", 977), ("p1", 1114), ("p2", 1344)];
const TEAM: [&str; 14] = ["Forge", "Flux", "Bridge", "Nexus", "Schema", "Sentinel", "Pulse", "Orbit", "Trace", "Titan", "Orion", "Nova", "Pixel", "Atlas"];

#[test]
fn the_starter_set_is_seed_md_and_stays_within_the_limits() {
    let set = guidelines::starter_set();
    assert_eq!(set.iter().map(|s| (s.key, s.text.chars().count() as u32)).collect::<Vec<_>>(), SEED_SIZES);
    for s in set {
        assert!(s.text.chars().count() <= MAX_GUIDELINE_CHARS && s.title.chars().count() <= MAX_TITLE_CHARS, "{}", s.key);
        assert_eq!(s.level == GuidelineLevel::Project, s.employees.is_empty(), "{}", s.key);
        assert!(s.text.ends_with('\n') && !s.text.contains("~~~"), "{}: the text between the fences", s.key);
    }
    let project: usize = set.iter().filter(|s| s.level == GuidelineLevel::Project).map(|s| s.text.chars().count()).sum();
    // Every member of the team in the project Aurelius (seed.md "Som per run").
    let per_run: Vec<(&str, usize)> = TEAM
        .iter()
        .map(|name| (*name, project + set.iter().filter(|s| s.employees.contains(name)).map(|s| s.text.chars().count()).sum::<usize>()))
        .collect();
    assert!(per_run.iter().all(|(_, n)| *n <= MAX_RUN_CHARS));
    let largest = per_run.iter().max_by_key(|(_, n)| *n).unwrap();
    assert_eq!(*largest, ("Bridge", 7_505), "GL-18 AC5");
    assert_eq!(per_run.iter().find(|(n, _)| *n == "Orbit").unwrap().1, 3_646);
    assert_eq!(project, 2_458);
}

#[test]
fn the_seed_adds_the_starter_set_once_with_links_by_exact_name() {
    let w = world();
    let aurelius = w.project("Aurelius", "active");
    let other = w.project("Other", "active");
    let mut ids = std::collections::HashMap::new();
    for name in TEAM {
        // Orbit is archived and Pulse is missing: both are skipped and reported.
        if name != "Pulse" {
            ids.insert(name, w.employee(name));
        }
    }
    w.archive_employee(ids["Orbit"]);
    // Not an exact name.
    w.employee("pulse");

    let report = w.seed();
    let links_expected: usize = guidelines::starter_set().iter().map(|s| s.employees.iter().filter(|n| **n != "Pulse" && **n != "Orbit").count()).sum::<usize>() + 2;
    assert_eq!(report, SeedReport { ran: true, inserted: 8, linked: links_expected, missing: vec!["Pulse".into(), "Orbit".into()] });

    let list = w.store.read(|c| guidelines::list(c, false)).unwrap();
    assert_eq!(list.len(), 8);
    let by_title = |t: &str| list.iter().find(|g| g.title == t).unwrap().clone();
    assert_eq!(by_title("Aurelius: layers and boundaries").links, [GuidelineTarget::Project(aurelius)]);
    assert_eq!(by_title("Worktrees, builds and cleanup").links.len(), 8, "ten names, minus Pulse and Orbit");
    assert!(list.iter().all(|g| g.updated_by.is_none() && g.version == 1));
    let first = w.store.read(move |c| guidelines::get(c, by_title("Rust style").id, None)).unwrap().unwrap();
    assert_eq!((first.source, first.by, first.text.as_str()), (GuidelineSource::Seed, None, guidelines::starter_set()[0].text));

    // The seeded set of the largest member is the 7,505 of seed.md, and no combination is over 16k.
    let bridge_id = ids["Bridge"];
    let bridge = w.store.read(move |c| guidelines::effective(c, Some(bridge_id), Some(aurelius))).unwrap();
    assert_eq!((bridge.guidelines.len(), bridge.total_chars), (7, 7_505));
    assert_eq!(bridge.guidelines.iter().filter(|g| g.level == GuidelineLevel::Project).count(), 2);
    let elsewhere = w.store.read(move |c| guidelines::effective(c, Some(bridge_id), Some(other))).unwrap();
    assert_eq!(elsewhere.total_chars, 5_047, "the Aurelius rules only in Aurelius");
    assert!(w.store.read(|c| guidelines::combinations_over(c, MAX_RUN_CHARS)).unwrap().is_empty());

    // Once: a second start adds nothing, also after the library is emptied out by archiving.
    assert_eq!(w.seed(), SeedReport::default());
    for g in &list {
        w.archive(g.id);
    }
    assert_eq!(w.seed(), SeedReport::default());
    assert_eq!(w.store.read(|c| guidelines::list(c, true)).unwrap().len(), 8);
    let marker: serde_json::Value = serde_json::from_str(&w.store.read(|c| settings::get(c, guidelines::SEEDED)).unwrap().unwrap()).unwrap();
    assert_eq!((marker["inserted"].as_u64(), marker["at_ms"].as_i64()), (Some(8), Some(50)));
}

#[test]
fn without_a_project_aurelius_the_seed_only_sets_its_marker() {
    let w = world();
    w.employee("Forge");
    // Not exact, or archived: no project Aurelius.
    w.project("aurelius", "active");
    w.project("Aurelius", "archived");
    assert_eq!(w.seed(), SeedReport::default());
    assert!(w.store.read(|c| settings::get(c, guidelines::SEEDED)).unwrap().is_some());
    assert!(w.store.read(|c| guidelines::list(c, true)).unwrap().is_empty());
    // A project Aurelius made later does not start the seed again.
    w.project("Aurelius", "active");
    assert_eq!(w.seed(), SeedReport::default());
    assert!(w.store.read(|c| guidelines::list(c, true)).unwrap().is_empty());
}

#[test]
fn the_seed_leaves_a_library_that_is_not_empty_alone() {
    let w = world();
    w.project("Aurelius", "active");
    let own = w.create("Own rule", "x".into());
    w.archive(own);
    assert_eq!(w.seed(), SeedReport::default());
    assert_eq!(w.store.read(|c| guidelines::list(c, true)).unwrap().len(), 1);
    assert!(w.store.read(|c| settings::get(c, guidelines::SEEDED)).unwrap().is_some());
}

#[test]
fn the_seed_writes_nothing_when_its_transaction_fails() {
    let w = world();
    w.project("Aurelius", "active");
    w.employee("Forge");
    let result = w.store.write(|tx| {
        guidelines::seed_guidelines_once(tx, 50)?;
        Err::<(), _>(StorageError::Rejected("stop".into()))
    });
    assert!(result.is_err());
    assert!(w.store.read(|c| guidelines::list(c, true)).unwrap().is_empty());
    assert!(w.store.read(|c| settings::get(c, guidelines::SEEDED)).unwrap().is_none());
    assert!(w.seed().ran, "the next start seeds");
}

#[test]
fn add_starter_set_adds_unlinked_guidelines_and_skips_taken_titles() {
    let w = world();
    w.employee("Forge");
    w.project("Aurelius", "active");
    let alex = w.alex;
    let added = w.store.write(move |tx| guidelines::add_starter_set(tx, &["g1", "p1", "g1"], alex, 60)).unwrap();
    assert_eq!(added.added.iter().map(|(k, _)| *k).collect::<Vec<_>>(), ["g1", "p1"]);
    assert!(added.skipped.is_empty());
    let list = w.store.read(|c| guidelines::list(c, false)).unwrap();
    assert_eq!(list.len(), 2);
    assert!(list.iter().all(|g| g.links.is_empty()), "no links, so no run's set changes");
    let v = w.store.read(move |c| guidelines::get(c, added.added[0].1, None)).unwrap().unwrap();
    assert_eq!((v.title.as_str(), v.source, v.by.map(|u| u.id)), ("Rust style", GuidelineSource::Seed, Some(alex)));

    let again = w.store.write(move |tx| guidelines::add_starter_set(tx, &["g1", "g2"], alex, 61)).unwrap();
    assert_eq!((again.added.len(), again.skipped.as_slice()), (1, ["g1"].as_slice()));
    rejected(w.store.write(move |tx| guidelines::add_starter_set(tx, &["g3", "g9"], alex, 62)), guidelines::UNKNOWN_STARTER);
    assert_eq!(w.store.read(|c| guidelines::list(c, false)).unwrap().len(), 3, "an unknown key adds nothing");
}

// ---------------------------------------------------------------- backup

#[test]
fn a_backup_holds_the_guidelines() {
    let w = world();
    let sam = w.employee("Sam");
    let id = w.create("Style", "x".into());
    w.link(id, GuidelineTarget::Employee(sam));
    propose(&w, Some(id), Some(1), sam, "Style");
    let (dir, _) = crate::backup::create(&w.store, w.store.root(), "manual", "test", "test").unwrap();
    let conn = rusqlite::Connection::open(dir.join("data/aurelius.sqlite")).unwrap();
    for (table, n) in [("guidelines", 1), ("guideline_versions", 1), ("guideline_links", 1), ("guideline_proposals", 1)] {
        assert_eq!(conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get::<_, i64>(0)).unwrap(), n, "{table}");
    }
}
