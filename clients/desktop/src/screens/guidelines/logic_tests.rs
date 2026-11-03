use super::*;
use crate::backend::{self, GuidelineWrite};
use aurelius_domain::guideline::{EffectiveGuideline, GuidelineLevel};
use aurelius_domain::proposal::ProposalStatus;
use aurelius_domain::vault::UserRef;
use aurelius_domain::RunId;
use std::prelude::rust_2024::test;

fn employee(name: &str, reports_to: Option<EmployeeId>, status: LifecycleStatus) -> EmployeeSummary {
    serde_json::from_value(serde_json::json!({
        "id": EmployeeId::new(), "name": name, "role": "Developer", "title": null, "reports_to": reports_to,
        "projects_active": [], "instructions": null, "status": status, "version": 1,
        "conversation_id": aurelius_domain::ConversationId::new(), "working": false, "agent_mode": "ask",
        "browser_allowed": false, "screen_allowed": false, "folder": null, "heartbeat": null,
        "sequential_tasks": false, "conversations": [], "agents": []
    }))
    .unwrap_or_else(|e| panic!("employee fixture: {e}"))
}

fn project(name: &str, status: ProjectStatus) -> ProjectSummary {
    ProjectSummary { id: ProjectId::new(), name: name.into(), description: None, status, version: 1, created_at_ms: 0, employees_active: 0, folder: None, dispatcher_employee_id: None }
}

fn guideline(title: &str, chars: u32, links: Vec<GuidelineTarget>) -> GuidelineView {
    GuidelineView { id: GuidelineId::new(), title: title.into(), version: 1, chars, links, updated_by: None, updated_at_ms: 0, archived: false, open_proposals: 0 }
}

fn set(employee: Option<EmployeeId>, project: Option<ProjectId>, items: &[(GuidelineId, u32, GuidelineLevel)]) -> EffectiveGuidelines {
    EffectiveGuidelines {
        employee_id: employee,
        project_id: project,
        guidelines: items.iter().map(|(id, chars, level)| EffectiveGuideline { id: *id, version: 1, title: "x".into(), text: String::new(), level: *level, chars: *chars }).collect(),
        total_chars: items.iter().map(|(_, c, _)| c).sum(),
        limit_chars: RUN_LIMIT,
        dropped: vec![],
    }
}

fn proposal(by: EmployeeId, guideline_id: Option<GuidelineId>, base_version: Option<u32>) -> GuidelineProposalView {
    GuidelineProposalView {
        id: GuidelineProposalId::new(),
        guideline_id,
        base_version,
        proposed_by: by,
        title: "Rust".into(),
        text: "t".into(),
        reason: "r".into(),
        evidence: vec![],
        suggested_links: vec![GuidelineTarget::Employee(by)],
        status: ProposalStatus::Pending,
        decision_note: None,
        decided_by: None,
        origin_conversation_id: None,
        run_id: None,
        created_at_ms: 0,
        decided_at_ms: None,
    }
}

/// GL-12 AC4, §4.1: thousands apart; calm, orange from 80 %, red over the limit, with a text.
#[test]
fn the_counter_warns_from_80_percent() {
    assert_eq!(count(940), "940");
    assert_eq!(count(6120), "6 120");
    assert_eq!(count(16_000), "16 000");
    assert_eq!(count(1_234_567), "1 234 567");
    assert_eq!(level(6399, 8000), Level::Calm);
    assert_eq!(level(6400, 8000), Level::Close, "80 % of 8 000");
    assert_eq!(level(8000, 8000), Level::Close, "the limit itself still fits");
    assert_eq!(level(8001, 8000), Level::Over);
    assert_eq!(level(12_799, RUN_LIMIT), Level::Calm);
    assert_eq!(level(12_800, RUN_LIMIT), Level::Close, "80 % of 16 000");
    assert_eq!(level_color(Level::Close), CLOSE);
    assert_eq!(level_color(Level::Over), theme::NEGATIVE);
    assert_eq!(counter_line(6120), "6 120 / 8 000 characters");
    assert_eq!(counter_note(6000), None);
    assert_eq!(counter_note(7000).as_deref(), Some("Close to the limit"));
    assert_eq!(counter_note(9240).as_deref(), Some("1 240 characters over the limit of 8 000"));
    assert_eq!(tokens(18_940), "≈ 4.7k tokens");
    // The counter counts the stored text: blank lines at the start and whitespace at the end go.
    assert_eq!(chars("\n\n  code\n\n"), 6);
}

/// GL-01 AC4, GL-12 AC1/AC2: Save says why it is off, before it goes.
#[test]
fn save_is_refused_before_it_goes() {
    let names = Names { employees: &[], projects: &[] };
    assert_eq!(save_problem("", "x", None, names).as_deref(), Some("Give it a title."));
    assert!(save_problem("Rust", "  \n ", None, names).is_some_and(|p| p.contains("archive")));
    assert_eq!(save_problem("Rust", &"a".repeat(8001), None, names).as_deref(), Some("1 characters over the limit of 8 000"));
    assert_eq!(save_problem("Rust", &"a".repeat(8000), None, names), None, "exactly 8 000 fits");
    assert_eq!(save_problem("Rust", "ok", Some(Block { employee: None, project: None, chars: 16_000 }), names), None, "exactly 16 000 fits");
    let over = save_problem("Rust", "ok", Some(Block { employee: None, project: None, chars: 16_001 }), names);
    assert_eq!(over.as_deref(), Some("A run would get 16 001 characters (max 16 000). Shorten this text or unlink something."));
}

/// GL-02 AC5: no change, no new version; only what changed goes.
#[test]
fn only_what_changed_is_sent() {
    assert_eq!(changes(("Rust", "a\n"), ("Rust ", "a")), None);
    assert_eq!(changes(("Rust", "a"), ("Rust 2", "a")), Some((Some("Rust 2".into()), None)));
    assert_eq!(changes(("Rust", "a"), ("Rust", "\nb\n")), Some((None, Some("b".into()))));
}

/// E14 (sec0 §2.5): no HTML, no image, no link that hides its address; code stays as written.
#[test]
fn a_preview_loads_nothing_and_hides_no_link() {
    let safe = safe_markdown("![x](https://evil.example/p.png?q=secret)\n<img src=\"https://evil.example/a.png\">\n[docs](https://evil.example/t)\n<https://auto.example>\n[ref]: https://evil.example/r\n");
    assert!(!safe.contains("!["), "{safe}");
    assert!(safe.contains("!\\[x](https://evil.example/p.png?q=secret)"), "the image shows as text with its address: {safe}");
    assert!(safe.contains("\\<img src="), "HTML shows as text: {safe}");
    assert!(safe.contains("\\[docs](https://evil.example/t)"), "a link shows its address: {safe}");
    assert!(safe.contains("\\<https://auto.example>"), "{safe}");
    assert!(safe.contains("\\[ref]: https://evil.example/r"), "a reference definition is text: {safe}");
    // Code spans and fences are left alone.
    assert_eq!(safe_markdown("Use `Vec<T>` and `[a]`.\n"), "Use `Vec<T>` and `[a]`.\n");
    assert_eq!(safe_markdown("``a ` <b>``\n"), "``a ` <b>``\n");
    assert_eq!(safe_markdown("```html\n<img src=x>\n[a](b)\n```\n<b>\n"), "```html\n<img src=x>\n[a](b)\n```\n\\<b>\n");
    assert_eq!(safe_markdown("~~~\n<x>\n~~~~\n<y>"), "~~~\n<x>\n~~~~\n\\<y>");
    // An unclosed backtick is no code span; an escape stays one escape.
    assert_eq!(safe_markdown("a ` <b> \\[c]"), "a ` \\<b> \\[c]");
    // Indented four spaces: no fence, so it is escaped too.
    assert_eq!(safe_markdown("    ```\n<b>"), "    ```\n\\<b>");
    // Headings, lists and emphasis stay markdown.
    assert_eq!(safe_markdown("## Errors\n- **No** `unwrap()`\n"), "## Errors\n- **No** `unwrap()`\n");
    // Sentinel M2: a table splits its cells before code spans, so a `|` in a code span
    // would end the cell and free what follows. In a code span it is escaped.
    let table = "| a | b |\n|---|---|\n| `x | ![i](https://e.invalid/p?d=1) ` | y |\n";
    assert_eq!(safe_markdown(table), "| a | b |\n|---|---|\n| `x \\| ![i](https://e.invalid/p?d=1) ` | y |\n");
    assert_eq!(safe_markdown("`a|b` c|d\n"), "`a\\|b` c|d\n", "outside a code span a `|` stays: tables still render");
    // What the renderer parses: no image, no image reference, no HTML; links show their address.
    let cases = [
        table,
        "| `x | <img src=https://e.invalid/a>` | y |\n|---|---|\n",
        "| `` a | ![i][r] `` | b |\n|---|---|\n\n[r]: https://e.invalid/r\n",
        "| a | b |\n|---|---|\n| ![i](https://e.invalid/p) | <b>x</b> |\n",
        "![x](https://e.invalid/p.png)\n<img src=\"https://e.invalid/a.png\">\n<https://auto.invalid>\n![r][ref]\n\n[ref]: https://e.invalid/r\n",
    ];
    for case in cases {
        let found = loading_nodes(&safe_markdown(case));
        assert!(found.is_empty(), "{case:?} still gives {found:?}");
    }
    // The table itself still renders as one.
    assert!(kinds(&safe_markdown(table)).contains(&"Table"), "a table stays a table");
    // The renderer's second guard: every image is the local placeholder, never a fetch.
    for url in ["https://e.invalid/p.png", "file:///etc/passwd", "data:image/png;base64,iVBORw0KGgo=", "./a.png"] {
        assert!(matches!(no_image(&url.into()), gpui_kit::ImageSource::Image(_)), "{url} loads nothing");
    }
}

/// The mdast of `text` with the options gpui-base's markdown view parses with.
fn mdast(text: &str) -> markdown::mdast::Node {
    let mut options = markdown::ParseOptions::gfm();
    options.constructs.math_text = true;
    options.constructs.math_flow = true;
    markdown::to_mdast(text, &options).unwrap_or_else(|e| panic!("mdast: {e}"))
}

/// Every node kind in `text`'s mdast, depth first.
fn kinds(text: &str) -> Vec<&'static str> {
    fn walk(node: &markdown::mdast::Node, out: &mut Vec<&'static str>) {
        use markdown::mdast::Node;
        out.push(match node {
            Node::Image(_) => "Image",
            Node::ImageReference(_) => "ImageReference",
            Node::Html(_) => "Html",
            Node::Table(_) => "Table",
            _ => "other",
        });
        for child in node.children().into_iter().flatten() {
            walk(child, out);
        }
    }
    let mut out = Vec::new();
    walk(&mdast(text), &mut out);
    out
}

/// What would load by itself or hide in the preview.
fn loading_nodes(text: &str) -> Vec<&'static str> {
    kinds(text).into_iter().filter(|k| matches!(*k, "Image" | "ImageReference" | "Html")).collect()
}

/// Pulse M1: the text box is as tall as the preview, and uses the window's height.
#[test]
fn the_editor_panes_fill_the_window() {
    assert_eq!(pane_height(900.), 470., "the preview's old height in a window 900 high");
    assert_eq!(pane_height(1400.), 970.);
    assert_eq!(pane_height(600.), 320., "never smaller than this");
}

/// Pulse m6: over 8 000 the counter says it; the line under it doesn't say it again.
#[test]
fn over_the_limit_is_said_once() {
    let names = Names { employees: &[], projects: &[] };
    let long = "a".repeat(8107);
    let n = chars(&long);
    let problem = save_problem("Rust", &long, None, names);
    assert_eq!(problem, counter_note(n), "the counter beside the bar has it");
    assert_eq!(editor_message(None, problem, "Rust", n), None);
    let block = save_problem("Rust", "ok", Some(Block { employee: None, project: None, chars: 16_001 }), names);
    assert_eq!(editor_message(None, block.clone(), "Rust", 2), block, "the block is said only here");
    assert_eq!(editor_message(None, save_problem("", "x", None, names), "", 1), None, "not on an empty new form");
    assert_eq!(editor_message(Some("refused".into()), None, "Rust", 2).as_deref(), Some("refused"));
}

/// Pulse m5: links made in the editor of a new guideline count in the line after Save.
#[test]
fn the_line_after_create_knows_its_links() {
    let flux = employee("Flux", None, LifecycleStatus::Active);
    let demo = project("Demo", ProjectStatus::Active);
    let (employees, projects) = (vec![flux.clone()], vec![demo.clone()]);
    let names = Names { employees: &employees, projects: &projects };
    assert_eq!(created_line("Commit messages", &[], names), "Saved “Commit messages” as version 1. Link it to use it.");
    let linked = created_line("Commit messages", &[GuidelineTarget::Project(demo.id), GuidelineTarget::Employee(flux.id)], names);
    assert_eq!(linked, "Saved “Commit messages” as version 1. It applies to Flux, ▸ Demo from the next run.");
}

/// Pulse m1/m2: the Context window names each guideline with its version.
#[test]
fn the_context_window_shows_versions() {
    let id = GuidelineId::new();
    assert_eq!(context_guideline(&format!("g:{id}@3"), "Rust style"), "Rust style v3");
    assert_eq!(context_guideline(&format!("g:{id}@12#redacted"), "Git"), "Git v12");
    assert_eq!(context_guideline("g:odd", "Odd"), "Odd", "no version: the title as it is");
}

/// P-54 (B2: no departments): grouped by whom they report to, with "n of m" and a tri-state box.
#[test]
fn the_links_group_by_reports_to() {
    let titan = employee("Titan", None, LifecycleStatus::Active);
    let forge = employee("Forge", Some(titan.id), LifecycleStatus::Active);
    let bridge = employee("Bridge", Some(titan.id), LifecycleStatus::Paused);
    let gone = employee("Gone", Some(titan.id), LifecycleStatus::Archived);
    let kept = employee("Kept", Some(titan.id), LifecycleStatus::Archived);
    let orphan = employee("Orphan", Some(EmployeeId::new()), LifecycleStatus::Active);
    let all = vec![titan.clone(), forge.clone(), bridge.clone(), gone, kept.clone(), orphan.clone()];
    let groups = groups(&all, &[kept.id]);
    assert_eq!(groups.iter().map(|g| g.label.as_str()).collect::<Vec<_>>(), ["Reports to no one", "Reports to Titan"]);
    assert_eq!(groups[0].members, vec![orphan.id, titan.id], "a boss that does not exist makes a root");
    assert_eq!(groups[1].members, vec![bridge.id, forge.id, kept.id], "paused stays; archived only while linked");
    let mut ticked = HashSet::new();
    assert_eq!(tri(&groups[1].members, &ticked), (0, 3, Tri::None));
    ticked.insert(GuidelineTarget::Employee(forge.id));
    assert_eq!(tri(&groups[1].members, &ticked), (1, 3, Tri::Some));
    ticked.insert(GuidelineTarget::Employee(bridge.id));
    ticked.insert(GuidelineTarget::Employee(kept.id));
    assert_eq!(tri(&groups[1].members, &ticked), (3, 3, Tri::All));
}

/// §5.1: Save sends the links, then the unlinks; nothing when nothing changed.
#[test]
fn the_links_save_as_changes() {
    let g = GuidelineId::new();
    let (a, b, p) = (GuidelineTarget::Employee(EmployeeId::new()), GuidelineTarget::Employee(EmployeeId::new()), GuidelineTarget::Project(ProjectId::new()));
    let before: HashSet<_> = [a, b].into_iter().collect();
    let after: HashSet<_> = [a, p].into_iter().collect();
    let changes = link_changes(g, &before, &after);
    assert_eq!(changes, vec![LinkChange { guideline_id: g, target: p, link: true }, LinkChange { guideline_id: g, target: b, link: false }]);
    assert!(link_changes(g, &before, &before).is_empty());
    let commands = backend::guideline_commands(&GuidelineWrite::Links(changes));
    assert!(matches!(&commands[0], (aurelius_domain::Command::LinkGuideline(l), None) if l.target == p));
    assert!(matches!(&commands[1], (aurelius_domain::Command::UnlinkGuideline(l), None) if l.target == b));
}

/// P-56, GL-12 AC2: the block per employee × project from `guideline.effective`, the
/// guideline counted once, and a box off when it would push a run over 16 000.
#[test]
fn the_largest_block_counts_each_guideline_once() {
    let forge = employee("Forge", None, LifecycleStatus::Active);
    let flux = employee("Flux", None, LifecycleStatus::Active);
    let gone = employee("Gone", None, LifecycleStatus::Archived);
    let (aurelius, web, old) = (project("aurelius", ProjectStatus::Active), project("web", ProjectStatus::Active), project("old", ProjectStatus::Archived));
    let rust = guideline("Rust", 6000, vec![GuidelineTarget::Employee(forge.id)]);
    let git = guideline("Git", 2000, vec![GuidelineTarget::Project(aurelius.id), GuidelineTarget::Employee(forge.id)]);
    let archived_link = guideline("Old", 100, vec![GuidelineTarget::Project(old.id)]);
    let list = vec![rust.clone(), git.clone(), archived_link];
    let projects = vec![aurelius.clone(), web.clone(), old];
    let ctx = contexts(&list, &projects);
    assert_eq!(ctx, vec![None, Some(aurelius.id)], "only a project with a guideline differs; an archived one is left out");
    let employees = vec![forge.clone(), flux.clone(), gone];
    assert_eq!(wanted(&employees, &ctx).len(), 4, "the archived employee is left out");
    let mut sets: Sets = HashMap::new();
    sets.insert((Some(forge.id), None), Ok(set(Some(forge.id), None, &[(git.id, 2000, GuidelineLevel::Employee), (rust.id, 6000, GuidelineLevel::Employee)])));
    // Git hangs on both: once, at project level (P-55).
    sets.insert((Some(forge.id), Some(aurelius.id)), Ok(set(Some(forge.id), Some(aurelius.id), &[(rust.id, 6000, GuidelineLevel::Employee), (git.id, 2000, GuidelineLevel::Project)])));
    sets.insert((Some(flux.id), None), Ok(set(Some(flux.id), None, &[])));
    sets.insert((Some(flux.id), Some(aurelius.id)), Ok(set(Some(flux.id), Some(aurelius.id), &[(git.id, 2000, GuidelineLevel::Project)])));
    // Rust grows to 7 900: Forge's blocks grow; Flux has no Rust.
    let largest = largest_with(&sets, Some(rust.id), 7900, &rust.links).unwrap_or_else(|| panic!("a block"));
    assert_eq!((largest.employee, largest.chars), (Some(forge.id), 9900));
    let flux_alone = sets[&(Some(flux.id), None)].as_ref().unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(block_with(flux_alone, Some(rust.id), 7900, false), 0);
    // A new guideline linked to Flux only: Flux's blocks.
    let new = largest_with(&sets, None, 8000, &[GuidelineTarget::Employee(flux.id)]).unwrap_or_else(|| panic!("a block"));
    assert_eq!((new.employee, new.project, new.chars), (Some(flux.id), Some(aurelius.id), 10_000));
    // Linking an 8 000 guideline to Forge: 16 000 fits, 16 001 does not.
    assert_eq!(employee_block(&sets, forge.id, &ctx, Some((None, 8000))).map(|b| b.chars), Some(16_000));
    assert!(employee_block(&sets, forge.id, &ctx, Some((None, 8001))).is_some_and(|b| b.chars > RUN_LIMIT));
    assert_eq!(employee_block(&sets, forge.id, &ctx, None).map(|b| b.chars), Some(8000));
    // A project without guidelines counts each employee's set without a project.
    let w = project_block(&sets, web.id, &ctx, &employees, None, 9000).unwrap_or_else(|| panic!("a block"));
    assert_eq!((w.employee, w.project, w.chars), (Some(forge.id), Some(web.id), 17_000));
    let names = Names { employees: &employees, projects: &projects };
    assert_eq!(block_sentence(w, names), "Forge would get 17 000 characters in web (max 16 000).");
    assert_eq!(block_where(largest, names), "Forge without a project");
}

/// GL-12 AC2, §4.1: guideline_too_large names the employee, the project and the total;
/// the other refusals in the user's words.
#[test]
fn refusals_name_what_is_wrong() {
    let bridge = employee("Bridge", None, LifecycleStatus::Active);
    let aurelius = project("aurelius", ProjectStatus::Active);
    let (employees, projects) = (vec![bridge.clone()], vec![aurelius.clone()]);
    let names = Names { employees: &employees, projects: &projects };
    let too_large = GuidelineFailure {
        code: Some(ErrorCode::GuidelineTooLarge),
        message: "x".into(),
        too_large: Some(GuidelineTooLarge { chars: 25_300, limit: 16_000, employee_id: Some(bridge.id), project_id: Some(aurelius.id) }),
        matched_items: vec![],
    };
    assert_eq!(failure_text(&too_large, names), "Bridge would get 25 300 characters in aurelius (max 16 000). Shorten this text or unlink something.");
    let alone = GuidelineTooLarge { chars: 17_000, limit: 16_000, employee_id: None, project_id: Some(aurelius.id) };
    assert_eq!(too_large_text(&alone, names), "A run in aurelius would get 17 000 characters (max 16 000). Shorten this text or unlink something.");
    let title = GuidelineFailure { code: Some(ErrorCode::GuidelineTitleTaken), ..GuidelineFailure::said("taken") };
    assert!(failure_text(&title, names).contains("already has this title"));
    let conflict = GuidelineFailure { code: Some(ErrorCode::VersionConflict), ..GuidelineFailure::said("stale") };
    assert!(failure_text(&conflict, names).contains("newer version"));
    let secret = GuidelineFailure { code: Some(ErrorCode::LooksLikeSecret), matched_items: vec!["GitHub token".into()], ..GuidelineFailure::said("x") };
    assert_eq!(failure_text(&secret, names), "This looks like a vault secret: GitHub token. Take the value out: guidelines go into every run.");
    // The daemon's placeholder until F2 reads as "not on this server yet".
    let placeholder = aurelius_client::ClientError::Protocol(aurelius_domain::AppError::new(ErrorCode::Validation, "Guidelines are not available on this server yet."));
    assert_eq!(GuidelineFailure::of(&placeholder).message, backend::GUIDELINES_NOT_BUILT);
    let large = aurelius_client::ClientError::Protocol(aurelius_domain::AppError::guideline_too_large(GuidelineTooLarge { chars: 1, limit: 2, employee_id: None, project_id: None }, "m"));
    assert_eq!(GuidelineFailure::of(&large).too_large.map(|t| t.chars), Some(1));
    let secret = aurelius_client::ClientError::Protocol(aurelius_domain::AppError::looks_like_secret(vec!["GitHub token".into()]));
    assert_eq!(GuidelineFailure::of(&secret).matched_items, ["GitHub token"]);
}

/// §2: the Applies to column, three then +n; not linked; search by title, @employee and ▸project; sort.
#[test]
fn the_library_lists_searches_and_sorts() {
    let forge = employee("Forge", None, LifecycleStatus::Active);
    let bridge = employee("Bridge", None, LifecycleStatus::Active);
    let nexus = employee("Nexus", None, LifecycleStatus::Active);
    let schema = employee("Schema", None, LifecycleStatus::Active);
    let aurelius = project("aurelius", ProjectStatus::Active);
    let (employees, projects) = (vec![forge.clone(), bridge.clone(), nexus.clone(), schema.clone()], vec![aurelius.clone()]);
    let names = Names { employees: &employees, projects: &projects };
    let rust_links = vec![GuidelineTarget::Project(aurelius.id), GuidelineTarget::Employee(schema.id), GuidelineTarget::Employee(forge.id), GuidelineTarget::Employee(bridge.id), GuidelineTarget::Employee(nexus.id)];
    assert_eq!(applies_to(&rust_links, names).0, "Bridge, Forge, Nexus +2");
    assert_eq!(applies_to(&rust_links, names).1, "Bridge, Forge, Nexus, Schema, ▸ aurelius");
    assert_eq!(applies_to(&[], names).0, NOT_LINKED);
    let mut rust = guideline("Rust", 6120, rust_links);
    rust.updated_at_ms = 5;
    let mut docs = guideline("Writing docs", 1200, vec![]);
    docs.updated_at_ms = 9;
    let mut git = guideline("git & worktrees", 2050, vec![GuidelineTarget::Project(aurelius.id)]);
    git.updated_at_ms = 1;
    let list = vec![rust, docs, git];
    let titles = |q: &str, s: Sort| shown(&list, q, s, names).into_iter().map(|g| g.title.clone()).collect::<Vec<_>>();
    assert_eq!(titles("", Sort::Title), ["git & worktrees", "Rust", "Writing docs"], "alphabetical without case, as in the block");
    assert_eq!(titles("", Sort::Size), ["Rust", "git & worktrees", "Writing docs"]);
    assert_eq!(titles("", Sort::Updated), ["Writing docs", "Rust", "git & worktrees"]);
    assert_eq!(titles("RUST", Sort::Title), ["Rust"]);
    assert_eq!(titles("@forge", Sort::Title), ["Rust"]);
    assert_eq!(titles("▸aurel", Sort::Title), ["git & worktrees", "Rust"]);
    assert_eq!(titles(">aurel", Sort::Title), ["git & worktrees", "Rust"]);
}

/// §4.2, P-59, B6: sources in the instructions' words; the trace's line; who a change reaches.
#[test]
fn history_trace_and_reach_read_well() {
    let forge = employee("Forge", None, LifecycleStatus::Active);
    let employees = vec![forge.clone()];
    let names = Names { employees: &employees, projects: &[] };
    let g = GuidelineId::new();
    let version = |source, proposal_id, restored_from| GuidelineVersionView { guideline_id: g, version: 2, title: "Rust".into(), text: "x".into(), source, proposal_id, restored_from, by: None, created_at_ms: 0 };
    let mut approved = proposal(forge.id, Some(g), Some(1));
    approved.status = ProposalStatus::Approved;
    let proposals = vec![approved.clone()];
    assert_eq!(source_label(&version(GuidelineSource::Seed, None, None), &proposals, names), "starter set");
    assert_eq!(source_label(&version(GuidelineSource::Edited, None, None), &proposals, names), "edited");
    assert_eq!(source_label(&version(GuidelineSource::Proposal, Some(approved.id), None), &proposals, names), "approved proposal from Forge");
    assert_eq!(source_label(&version(GuidelineSource::Restored, None, Some(1)), &proposals, names), "restored from version 1");
    let run = |title: &str, v: u32| RunGuideline { run_id: RunId::new(), guideline_id: GuidelineId::new(), version: v, title: title.into(), level: GuidelineLevel::Employee, chars: 1 };
    assert_eq!(run_line(&[run("Rust", 3), run("Git & worktrees", 4)]).as_deref(), Some("Guidelines: Rust v3, Git & worktrees v4"));
    assert_eq!(run_line(&[]), None);
    let reach = |employees, conversations| GuidelineAffected { guideline_id: g, employees, projects: 0, conversations };
    assert_eq!(affected_line(&reach(3, 0)), "Applies from the next run of 3 employees.");
    assert_eq!(affected_line(&reach(1, 2)), "Applies from the next run of 1 employee. 2 conversations start a new session.");
}

/// §7.1, P-61, A-04: the card's headline, a stale proposal, the links of a new one, the decision.
#[test]
fn a_proposal_says_what_it_changes() {
    let forge = employee("Forge", None, LifecycleStatus::Active);
    let employees = vec![forge.clone()];
    let names = Names { employees: &employees, projects: &[] };
    let mut rust = guideline("Rust", 10, vec![]);
    rust.version = 3;
    let mut p = proposal(forge.id, Some(rust.id), Some(3));
    assert_eq!(proposal_headline(&p, Some(&rust), names), "Forge proposes a change to “Rust”");
    assert_eq!(stale(&p, Some(&rust)), None);
    rust.version = 4;
    assert_eq!(stale(&p, Some(&rust)), Some((3, 4)), "approving would be a version_conflict");
    p.guideline_id = None;
    p.base_version = None;
    p.title = "Async Rust".into();
    assert_eq!(proposal_headline(&p, None, names), "Forge proposes a new guideline “Async Rust”");
    assert_eq!(default_links(&p), vec![GuidelineTarget::Employee(forge.id)], "the proposer by default");
    let reject = backend::guideline_commands(&GuidelineWrite::Decide { proposal_id: p.id, approve: false, title: None, text: None, links: None, note: Some("Not now".into()), expected_version: None });
    assert!(matches!(&reject[0], (aurelius_domain::Command::DecideGuidelineProposal(d), None) if !d.approve && d.note.as_deref() == Some("Not now")));
    let approve = backend::guideline_commands(&GuidelineWrite::Decide { proposal_id: p.id, approve: true, title: None, text: Some("u".into()), links: Some(vec![]), note: None, expected_version: Some(3) });
    assert!(matches!(&approve[0], (aurelius_domain::Command::DecideGuidelineProposal(d), Some(3)) if d.approve && d.text.as_deref() == Some("u")));
    let update = backend::guideline_commands(&GuidelineWrite::Update { guideline_id: rust.id, expected_version: 3, title: None, text: Some("x".into()) });
    assert_eq!(update[0].1, Some(3), "an update expects the version it started from (P-64)");
}

/// §8.1: the banner shows after the seed until this board member closes it.
#[test]
fn the_starter_banner_is_per_board_member() {
    let seeded = guideline("Rust", 10, vec![]);
    let mut edited = guideline("Git", 10, vec![]);
    edited.version = 2;
    edited.updated_by = Some(UserRef { id: aurelius_domain::UserId::new(), username: "alex".into() });
    assert!(banner_due(&[seeded.clone(), edited.clone()], Some("u1"), &[]));
    assert!(!banner_due(std::slice::from_ref(&seeded), Some("u1"), &["u1".into()]));
    assert!(banner_due(&[seeded], Some("u2"), &["u1".into()]));
    assert!(!banner_due(&[edited], Some("u1"), &[]), "nothing of the seed left as it was");
    assert!(!banner_due(&[], None, &[]));
}

/// GL-14 AC3/AC5, §7.1: Approve is off for a stale proposal, a text over 8 000 and a block over 16 000.
#[test]
fn approve_is_off_when_it_would_be_refused() {
    let forge = employee("Forge", None, LifecycleStatus::Active);
    let employees = vec![forge.clone()];
    let names = Names { employees: &employees, projects: &[] };
    let mut rust = guideline("Rust", 6000, vec![GuidelineTarget::Employee(forge.id)]);
    rust.version = 3;
    let mut sets: Sets = HashMap::new();
    sets.insert((Some(forge.id), None), Ok(set(Some(forge.id), None, &[(rust.id, 6000, GuidelineLevel::Employee), (GuidelineId::new(), 9000, GuidelineLevel::Employee)])));
    let mut p = proposal(forge.id, Some(rust.id), Some(3));
    p.text = "a".repeat(7000);
    assert_eq!(approve_problem(&p, Some(&rust), &rust.links, &sets, names), None, "15 000 fits");
    p.text = "a".repeat(7001);
    assert_eq!(approve_problem(&p, Some(&rust), &rust.links, &sets, names).as_deref(), Some("Forge would get 16 001 characters without a project (max 16 000)."));
    p.text = "a".repeat(8001);
    assert_eq!(approve_problem(&p, Some(&rust), &rust.links, &sets, names).as_deref(), Some("1 characters over the limit of 8 000"));
    rust.version = 4;
    assert!(approve_problem(&p, Some(&rust), &rust.links, &sets, names).is_some_and(|w| w.contains("changed")));
    // A new one weighs the links it would get: none, nothing to weigh.
    let mut new = proposal(forge.id, None, None);
    new.text = "a".repeat(7500);
    assert!(approve_problem(&new, None, &[GuidelineTarget::Employee(forge.id)], &sets, names).is_some());
    assert_eq!(approve_problem(&new, None, &[], &sets, names), None);
}

#[test]
fn the_first_proposal_after_an_empty_first_list_is_news() {
    let mut loaded = false;
    let none = HashSet::new();
    assert!(!list_brings_news(&mut loaded, &none, &[]), "the first list is empty");
    assert!(list_brings_news(&mut loaded, &none, &[GuidelineProposalId::new()]));
}

#[test]
fn what_the_first_list_holds_or_a_list_repeats_is_no_news() {
    let mut loaded = false;
    let id = GuidelineProposalId::new();
    assert!(!list_brings_news(&mut loaded, &HashSet::new(), &[id]), "it was already there");
    assert!(!list_brings_news(&mut loaded, &HashSet::from([id]), &[id]));
}
