//! What the guideline screens compute, without a window: counters and limits, the
//! groups of the Links dialog, the largest block, the texts, and E14's safe markdown.

use crate::backend::{EffectiveKey, GuidelineFailure, LinkChange};
use crate::theme;
use aurelius_domain::employee::LifecycleStatus;
use aurelius_domain::error::ErrorCode;
use aurelius_domain::guideline::{
    EffectiveGuidelines, GuidelineAffected, GuidelineProposalView, GuidelineSource, GuidelineTarget, GuidelineTooLarge, GuidelineVersionView, GuidelineView,
    MAX_GUIDELINE_CHARS, MAX_RUN_CHARS, MAX_TITLE_CHARS, RunGuideline, WARN_PERCENT,
};
use aurelius_domain::project::ProjectStatus;
use aurelius_domain::{EmployeeId, GuidelineId, ProjectId};
use aurelius_protocol::{EmployeeSummary, ProjectSummary};
use std::collections::{HashMap, HashSet};

/// Most characters of one guideline (B4).
pub const TEXT_LIMIT: u32 = MAX_GUIDELINE_CHARS as u32;
/// Most characters of guidelines one run gets (B4, corrected to 16 000).
pub const RUN_LIMIT: u32 = MAX_RUN_CHARS as u32;
/// The counter's orange (ui-ontwerp §4.1).
pub const CLOSE: u32 = 0xD99A3A;
/// The local key of the board members who closed the starter set banner (ui.json).
pub const BANNER_KEY: &str = "guidelines_banner_done";
/// Below this width the editor starts on Write instead of Side by side.
pub const SIDE_MIN_W: f32 = 1100.;

pub const NOT_LINKED: &str = "— not linked";
pub const ABOUT: &str = "Rules every linked run gets, on top of its instructions.";
pub const EMPTY_ABOUT: &str = "Guidelines are working rules (how to write Rust, how to use worktrees) that you link to employees or projects. They come on top of an employee's instructions in every run.";
pub const EMPTY_PROPOSE: &str = "Employees can also propose one; the bell tells you.";
pub const BANNER: &str = "Aurelius added a starter set with the team's current rules. Check them and their links; they apply from the next run.";
pub const GROUP_NOTE: &str = "Selecting a group links its current members one by one. New reports are not linked automatically.";
pub const PRECEDENCE: &str = "Instructions, task, agent rules and permissions always go before these.";
pub const READ_ONLY: &str = "Only a board member changes guidelines. Sign in as one to edit.";

/// The editor's panes, Write and Preview alike: what a window of `viewport` leaves under
/// the title, the tabs and above the counters and Save (Pulse M1). 470 in a window 900 high.
pub fn pane_height(viewport: f32) -> f32 {
    (viewport - 430.).max(320.)
}

/// The preview's image resolver: whatever a guideline's markdown names, a local
/// placeholder. Nothing is ever fetched or read from disk (E14, Sentinel M2).
pub fn no_image(_: &gpui_kit::SharedUri) -> gpui_kit::ImageSource {
    static PLACEHOLDER: std::sync::OnceLock<std::sync::Arc<gpui_kit::Image>> = std::sync::OnceLock::new();
    let image = PLACEHOLDER.get_or_init(|| {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="18" viewBox="0 0 24 18"><rect x="0.5" y="0.5" width="23" height="17" rx="2" fill="none" stroke="#8a8a8a"/><path d="M4 14l5-6 4 4 3-3 4 5z" fill="#8a8a8a"/></svg>"##;
        std::sync::Arc::new(gpui_kit::Image::from_bytes(gpui_kit::ImageFormat::Svg, svg.as_bytes().to_vec()))
    });
    gpui_kit::ImageSource::Image(image.clone())
}

/// "6 120", "16 000", "940": thousands apart, as in the design.
pub fn count(n: u32) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(' ');
        }
        out.push(ch);
    }
    out
}

/// How full a counter is: calm, close to the limit (from 80 %), or over it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Level {
    Calm,
    Close,
    Over,
}

pub fn level(chars: u32, limit: u32) -> Level {
    if chars > limit {
        Level::Over
    } else if u64::from(chars) * 100 >= u64::from(limit) * WARN_PERCENT as u64 {
        Level::Close
    } else {
        Level::Calm
    }
}

/// The colour of a counter at `level`; the text beside it always says it too (§9).
pub fn level_color(level: Level) -> u32 {
    match level {
        Level::Calm => theme::BORDER_STRONG,
        Level::Close => CLOSE,
        Level::Over => theme::NEGATIVE,
    }
}

/// The text as the server stores it: blank lines at the start and whitespace at the end go.
pub fn stored(text: &str) -> &str {
    text.trim_start_matches(['\n', '\r']).trim_end()
}

/// Characters as the limit counts them (SQLite's `length()`).
pub fn chars(text: &str) -> u32 {
    stored(text).chars().count() as u32
}

/// "6 120 / 8 000 characters".
pub fn counter_line(chars: u32) -> String {
    format!("{} / {} characters", count(chars), count(TEXT_LIMIT))
}

/// What the counter says beside its bar: nothing, "Close to the limit", or how far over.
pub fn counter_note(chars: u32) -> Option<String> {
    match level(chars, TEXT_LIMIT) {
        Level::Calm => None,
        Level::Close => Some("Close to the limit".into()),
        Level::Over => Some(format!("{} characters over the limit of {}", count(chars - TEXT_LIMIT), count(TEXT_LIMIT))),
    }
}

/// "≈ 4.7k tokens" (P-65: tokens only as an estimate, 4 characters each).
pub fn tokens(chars: u32) -> String {
    let t = f64::from(chars) / 4.;
    if t >= 1000. { format!("≈ {:.1}k tokens", t / 1000.) } else { format!("≈ {} tokens", t.round() as u32) }
}

/// Names of employees and projects, for every line that names one.
#[derive(Clone, Copy)]
pub struct Names<'a> {
    pub employees: &'a [EmployeeSummary],
    pub projects: &'a [ProjectSummary],
}

impl Names<'_> {
    pub fn employee(&self, id: EmployeeId) -> String {
        self.employees.iter().find(|e| e.id == id).map_or_else(|| "An employee".into(), |e| e.name.clone())
    }

    pub fn project(&self, id: ProjectId) -> String {
        self.projects.iter().find(|p| p.id == id).map_or_else(|| "a project".into(), |p| p.name.clone())
    }

    /// "Forge", "▸ aurelius"; "(archived)" when it no longer runs (GL-05 AC4, GL-06 AC4).
    pub fn target(&self, target: GuidelineTarget) -> String {
        match target {
            GuidelineTarget::Employee(id) => {
                let archived = self.employees.iter().find(|e| e.id == id).is_none_or(|e| e.status == LifecycleStatus::Archived);
                format!("{}{}", self.employee(id), if archived { " (archived)" } else { "" })
            }
            GuidelineTarget::Project(id) => {
                let archived = self.projects.iter().find(|p| p.id == id).is_none_or(|p| p.status == ProjectStatus::Archived);
                format!("▸ {}{}", self.project(id), if archived { " (archived)" } else { "" })
            }
        }
    }
}

/// Employees first, then projects, each by name.
pub fn sorted_targets(links: &[GuidelineTarget], names: Names) -> Vec<GuidelineTarget> {
    let mut out = links.to_vec();
    out.sort_by_key(|t| match t {
        GuidelineTarget::Employee(_) => (0, names.target(*t).to_lowercase()),
        GuidelineTarget::Project(_) => (1, names.target(*t).to_lowercase()),
    });
    out
}

/// The Applies to column: at most three, then "+n"; the full list for the tooltip (§2).
pub fn applies_to(links: &[GuidelineTarget], names: Names) -> (String, String) {
    if links.is_empty() {
        return (NOT_LINKED.into(), "Not linked: no run gets it.".into());
    }
    let all: Vec<String> = sorted_targets(links, names).into_iter().map(|t| names.target(t)).collect();
    let mut shown = all.iter().take(3).cloned().collect::<Vec<_>>().join(", ");
    if all.len() > 3 {
        shown.push_str(&format!(" +{}", all.len() - 3));
    }
    (shown, all.join(", "))
}

/// The order of the library: by title (B3's order), by size or by the last change.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Sort {
    #[default]
    Title,
    Size,
    Updated,
}

/// The library as it shows: what the search asks, in the chosen order. `@Forge` finds
/// what is linked to Forge, `▸aurelius` (or `>aurelius`) what is linked to that project;
/// anything else searches the titles.
pub fn shown<'a>(list: &'a [GuidelineView], query: &str, sort: Sort, names: Names) -> Vec<&'a GuidelineView> {
    let q = query.trim().to_lowercase();
    let mut out: Vec<&GuidelineView> = list
        .iter()
        .filter(|g| {
            if let Some(who) = q.strip_prefix('@') {
                g.links.iter().any(|t| matches!(t, GuidelineTarget::Employee(id) if names.employee(*id).to_lowercase().contains(who.trim())))
            } else if let Some(project) = q.strip_prefix('▸').or_else(|| q.strip_prefix('>')) {
                g.links.iter().any(|t| matches!(t, GuidelineTarget::Project(id) if names.project(*id).to_lowercase().contains(project.trim())))
            } else {
                g.title.to_lowercase().contains(&q)
            }
        })
        .collect();
    match sort {
        Sort::Title => out.sort_by_key(|g| g.title.to_lowercase()),
        Sort::Size => out.sort_by(|a, b| b.chars.cmp(&a.chars).then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))),
        Sort::Updated => out.sort_by_key(|g| std::cmp::Reverse(g.updated_at_ms)),
    }
    out
}

/// E14 (sec0 §2.5): markdown for a preview of a guideline or a proposal without HTML,
/// without images or anything else that loads by itself, and without a link that hides
/// its address. Outside code, `<` and `[` are escaped: raw HTML, autolinks `<…>`,
/// `![image](…)`, `[text](…)` and reference definitions all show as the text they are,
/// address included. Code spans and fenced code stay as written, except `|`: in a code
/// span it becomes `\|`, because a GFM table splits its cells before it sees code spans
/// (Sentinel M2). In a cell that gives `|` back; outside a table the backslash only
/// shows. A `|` outside code spans stays, so tables still render: a cell then only ends
/// where this escaping saw no code span either. The preview's image resolver is the
/// second guard.
pub fn safe_markdown(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + text.len() / 8);
    // The open fence: its character and its length.
    let mut fence: Option<(char, usize)> = None;
    for line in text.split_inclusive('\n') {
        let body = line.trim_start_matches(' ');
        let indent = line.len() - body.len();
        let marker = if indent < 4 { fence_marker(body) } else { None };
        match (fence, marker) {
            (None, Some((ch, n, _))) => {
                fence = Some((ch, n));
                out.push_str(line);
                continue;
            }
            (Some((open, len)), Some((ch, n, rest))) if ch == open && n >= len && rest.trim().is_empty() => {
                fence = None;
                out.push_str(line);
                continue;
            }
            _ => {}
        }
        if fence.is_some() {
            out.push_str(line);
        } else {
            escape_inline(line, &mut out);
        }
    }
    out
}

/// "```rust" → ('`', 3, "rust"): a line that opens or closes a fenced code block.
fn fence_marker(body: &str) -> Option<(char, usize, &str)> {
    let ch = body.chars().next().filter(|c| *c == '`' || *c == '~')?;
    let n = body.chars().take_while(|c| *c == ch).count();
    let rest = &body[n..];
    // The info string of a backtick fence holds no backtick.
    (n >= 3 && !(ch == '`' && rest.contains('`'))).then_some((ch, n, rest))
}

fn escape_inline(line: &str, out: &mut String) {
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        if ch == '`' {
            let n = chars[i..].iter().take_while(|c| **c == '`').count();
            // A code span ends at the next run of exactly as many backticks.
            let close = (i + n..chars.len()).find(|&j| chars[j] == '`' && chars[j - 1] != '`' && chars[j..].iter().take_while(|c| **c == '`').count() == n);
            match close {
                Some(j) => {
                    for &c in &chars[i..j + n] {
                        if c == '|' {
                            out.push('\\');
                        }
                        out.push(c);
                    }
                    i = j + n;
                }
                None => {
                    out.extend(&chars[i..i + n]);
                    i += n;
                }
            }
            continue;
        }
        if ch == '\\' && i + 1 < chars.len() {
            out.push(ch);
            out.push(chars[i + 1]);
            i += 2;
            continue;
        }
        if ch == '<' || ch == '[' {
            out.push('\\');
        }
        out.push(ch);
        i += 1;
    }
}

/// A group of the Links dialog: who reports to the same employee (P-54, B2: no departments).
#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    pub boss: Option<EmployeeId>,
    pub label: String,
    pub members: Vec<EmployeeId>,
}

/// The employees that can be linked, grouped by whom they report to: "Reports to Titan".
/// Archived employees only while `keep` holds them (a link that is still there, GL-05 AC4).
/// Groups and members by name; those who report to no one first.
pub fn groups(employees: &[EmployeeSummary], keep: &[EmployeeId]) -> Vec<Group> {
    let mut members: Vec<&EmployeeSummary> = employees.iter().filter(|e| e.status != LifecycleStatus::Archived || keep.contains(&e.id)).collect();
    members.sort_by_key(|e| e.name.to_lowercase());
    let mut out: Vec<Group> = Vec::new();
    for e in members {
        let boss = e.reports_to.filter(|b| employees.iter().any(|x| x.id == *b));
        match out.iter_mut().find(|g| g.boss == boss) {
            Some(g) => g.members.push(e.id),
            None => out.push(Group {
                boss,
                label: match boss {
                    Some(b) => format!("Reports to {}", employees.iter().find(|x| x.id == b).map_or("someone", |x| x.name.as_str())),
                    None => "Reports to no one".into(),
                },
                members: vec![e.id],
            }),
        }
    }
    out.sort_by_key(|g| (g.boss.is_some(), g.label.to_lowercase()));
    out
}

/// A group's box: none, some or all of its members ticked.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tri {
    None,
    Some,
    All,
}

/// "4 of 7" and the group's box.
pub fn tri(members: &[EmployeeId], ticked: &HashSet<GuidelineTarget>) -> (usize, usize, Tri) {
    let n = members.iter().filter(|m| ticked.contains(&GuidelineTarget::Employee(**m))).count();
    let state = match n {
        0 => Tri::None,
        n if n == members.len() => Tri::All,
        _ => Tri::Some,
    };
    (n, members.len(), state)
}

/// A stable order for targets: employees first, then projects, each by id.
pub fn target_key(t: &GuidelineTarget) -> (u8, String) {
    match t {
        GuidelineTarget::Employee(id) => (0, id.to_string()),
        GuidelineTarget::Project(id) => (1, id.to_string()),
    }
}

/// What Save of the Links dialog sends: the links first, then the unlinks, each in a stable order.
pub fn link_changes(guideline_id: GuidelineId, before: &HashSet<GuidelineTarget>, after: &HashSet<GuidelineTarget>) -> Vec<LinkChange> {
    let mut add: Vec<GuidelineTarget> = after.difference(before).copied().collect();
    let mut remove: Vec<GuidelineTarget> = before.difference(after).copied().collect();
    add.sort_by_key(target_key);
    remove.sort_by_key(target_key);
    let add = add.into_iter().map(|target| LinkChange { guideline_id, target, link: true });
    let remove = remove.into_iter().map(|target| LinkChange { guideline_id, target, link: false });
    add.chain(remove).collect()
}

/// The contexts a run can have that may differ: without a project, and each project that
/// is not archived with a guideline linked to it (any other project adds nothing). The
/// server checks each employee that is not archived in each of these (storage
/// `combinations_over`).
pub fn contexts(list: &[GuidelineView], projects: &[ProjectSummary]) -> Vec<Option<ProjectId>> {
    let mut linked: Vec<&ProjectSummary> = projects
        .iter()
        .filter(|p| p.status != ProjectStatus::Archived && list.iter().any(|g| !g.archived && g.links.contains(&GuidelineTarget::Project(p.id))))
        .collect();
    linked.sort_by_key(|p| p.name.to_lowercase());
    std::iter::once(None).chain(linked.into_iter().map(|p| Some(p.id))).collect()
}

/// Every `guideline.effective` the limit needs: each employee that is not archived in each context.
pub fn wanted(employees: &[EmployeeSummary], contexts: &[Option<ProjectId>]) -> Vec<EffectiveKey> {
    employees
        .iter()
        .filter(|e| e.status != LifecycleStatus::Archived)
        .flat_map(|e| contexts.iter().map(move |p| (Some(e.id), *p)))
        .collect()
}

/// The block of a set once guideline `g` has `chars` characters; `joins`: it comes in
/// when it is not in it yet. A guideline counts once (P-55).
pub fn block_with(set: &EffectiveGuidelines, g: Option<GuidelineId>, chars: u32, joins: bool) -> u32 {
    let had = g.and_then(|g| set.guidelines.iter().find(|x| x.id == g)).map(|x| x.chars);
    match had {
        Some(old) => set.total_chars.saturating_sub(old) + chars,
        None if joins => set.total_chars + chars,
        None => set.total_chars,
    }
}

/// A run's block: whose, where, how big.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Block {
    pub employee: Option<EmployeeId>,
    pub project: Option<ProjectId>,
    pub chars: u32,
}

/// The sets `guideline.effective` gave, by employee and project.
pub type Sets = HashMap<EffectiveKey, Result<EffectiveGuidelines, String>>;

/// The largest block a run would get with guideline `g` (none: a new one) at `chars`
/// characters and linked to `links`: over every known set it is in or comes into.
pub fn largest_with(sets: &Sets, g: Option<GuidelineId>, chars: u32, links: &[GuidelineTarget]) -> Option<Block> {
    sets.iter()
        .filter_map(|(key, set)| Some((*key, set.as_ref().ok()?)))
        .filter(|((employee, project), set)| {
            g.is_some_and(|g| set.guidelines.iter().any(|x| x.id == g))
                || employee.is_some_and(|e| links.contains(&GuidelineTarget::Employee(e)))
                || project.is_some_and(|p| links.contains(&GuidelineTarget::Project(p)))
        })
        .map(|((employee, project), set)| Block { employee, project, chars: block_with(set, g, chars, true) })
        .max_by_key(|b| (b.chars, b.employee.is_some(), b.project.is_none()))
}

/// The largest block of an employee over its contexts, now (`g` none) or with `g` at its size linked to it.
pub fn employee_block(sets: &Sets, employee: EmployeeId, contexts: &[Option<ProjectId>], g: Option<(Option<GuidelineId>, u32)>) -> Option<Block> {
    contexts
        .iter()
        .filter_map(|p| {
            let set = sets.get(&(Some(employee), *p))?.as_ref().ok()?;
            let chars = match g {
                Some((g, chars)) => block_with(set, g, chars, true),
                None => set.total_chars,
            };
            Some(Block { employee: Some(employee), project: *p, chars })
        })
        .max_by_key(|b| b.chars)
}

/// The largest block in a project with `g` linked to it: each employee's set there (or,
/// for a project without guidelines yet, its set without a project).
pub fn project_block(sets: &Sets, project: ProjectId, contexts: &[Option<ProjectId>], employees: &[EmployeeSummary], g: Option<GuidelineId>, chars: u32) -> Option<Block> {
    let at = if contexts.contains(&Some(project)) { Some(project) } else { None };
    employees
        .iter()
        .filter(|e| e.status != LifecycleStatus::Archived)
        .filter_map(|e| {
            let set = sets.get(&(Some(e.id), at))?.as_ref().ok()?;
            Some(Block { employee: Some(e.id), project: Some(project), chars: block_with(set, g, chars, true) })
        })
        .max_by_key(|b| b.chars)
}

/// "Bridge would get 25 300 characters in aurelius (max 16 000)."
pub fn block_sentence(block: Block, names: Names) -> String {
    sentence_with_max(block, RUN_LIMIT, names)
}

fn sentence_with_max(block: Block, limit: u32, names: Names) -> String {
    let n = count(block.chars);
    let max = count(limit);
    match (block.employee, block.project) {
        (Some(e), Some(p)) => format!("{} would get {n} characters in {} (max {max}).", names.employee(e), names.project(p)),
        (Some(e), None) => format!("{} would get {n} characters without a project (max {max}).", names.employee(e)),
        (None, Some(p)) => format!("A run in {} would get {n} characters (max {max}).", names.project(p)),
        (None, None) => format!("A run would get {n} characters (max {max})."),
    }
}

/// "Bridge in aurelius", "Forge without a project".
pub fn block_where(block: Block, names: Names) -> String {
    match (block.employee, block.project) {
        (Some(e), Some(p)) => format!("{} in {}", names.employee(e), names.project(p)),
        (Some(e), None) => format!("{} without a project", names.employee(e)),
        (None, Some(p)) => format!("a run in {}", names.project(p)),
        (None, None) => "a run".into(),
    }
}

/// `guideline_too_large` in the user's words: the employee, the project and the total (GL-12 AC2).
pub fn too_large_text(t: &GuidelineTooLarge, names: Names) -> String {
    let block = Block { employee: t.employee_id, project: t.project_id, chars: t.chars };
    format!("{} Shorten this text or unlink something.", sentence_with_max(block, t.limit, names))
}

/// A refused guideline write in the user's words.
pub fn failure_text(f: &GuidelineFailure, names: Names) -> String {
    match f.code {
        Some(ErrorCode::GuidelineTooLarge) => match &f.too_large {
            Some(t) => too_large_text(t, names),
            None => format!("{} Shorten this text or unlink something.", f.message),
        },
        Some(ErrorCode::GuidelineTitleTaken) => "Another guideline already has this title. Choose another title.".into(),
        Some(ErrorCode::VersionConflict) => "Someone saved a newer version while you were editing. Compare, or discard yours.".into(),
        Some(ErrorCode::LooksLikeSecret) if !f.matched_items.is_empty() => {
            format!("This looks like a vault secret: {}. Take the value out: guidelines go into every run.", f.matched_items.join(", "))
        }
        Some(ErrorCode::LooksLikeSecret) => "This looks like a vault secret. Take the value out: guidelines go into every run.".into(),
        _ => f.message.clone(),
    }
}

/// Where a version came from, in the words of the instructions' history (§4.2).
pub fn source_label(version: &GuidelineVersionView, proposals: &[GuidelineProposalView], names: Names) -> String {
    match version.source {
        GuidelineSource::Seed => "starter set".into(),
        GuidelineSource::Created => "created".into(),
        GuidelineSource::Edited => "edited".into(),
        GuidelineSource::Proposal => match version.proposal_id.and_then(|id| proposals.iter().find(|p| p.id == id)) {
            Some(p) => format!("approved proposal from {}", names.employee(p.proposed_by)),
            None => "approved proposal".into(),
        },
        GuidelineSource::Restored => match version.restored_from {
            Some(n) => format!("restored from version {n}"),
            None => "restored".into(),
        },
    }
}

/// "Guidelines: Rust v3, Git & worktrees v4" (P-59); none for a run without any.
pub fn run_line(guidelines: &[RunGuideline]) -> Option<String> {
    (!guidelines.is_empty()).then(|| format!("Guidelines: {}", guidelines.iter().map(|g| format!("{} v{}", g.title, g.version)).collect::<Vec<_>>().join(", ")))
}

/// "Applies from the next run of 3 employees." (B6), with the conversations that start anew.
pub fn affected_line(a: &GuidelineAffected) -> String {
    let who = match a.employees {
        0 => "Applies from the next run; no employee gets it now.".to_string(),
        1 => "Applies from the next run of 1 employee.".to_string(),
        n => format!("Applies from the next run of {n} employees."),
    };
    match a.conversations {
        0 => who,
        1 => format!("{who} 1 conversation starts a new session."),
        n => format!("{who} {n} conversations start a new session."),
    }
}

/// The proposal card's headline (P-61).
pub fn proposal_headline(p: &GuidelineProposalView, current: Option<&GuidelineView>, names: Names) -> String {
    let who = names.employee(p.proposed_by);
    match (p.guideline_id, current) {
        (None, _) => format!("{who} proposes a new guideline “{}”", p.title),
        (Some(_), Some(g)) => format!("{who} proposes a change to “{}”", g.title),
        (Some(_), None) => format!("{who} proposes a change to “{}”", p.title),
    }
}

/// A change proposed on a version that is no longer the current one (A-04): only Reject.
pub fn stale(p: &GuidelineProposalView, current: Option<&GuidelineView>) -> Option<(u32, u32)> {
    match (p.guideline_id, p.base_version, current) {
        (Some(_), Some(base), Some(g)) if g.version != base => Some((base, g.version)),
        _ => None,
    }
}

/// Why Approve is off for a proposal as it is: stale (A-04), over 8 000, or a run over
/// 16 000 with its links (GL-14 AC5); none when it can go.
pub fn approve_problem(p: &GuidelineProposalView, current: Option<&GuidelineView>, links: &[GuidelineTarget], sets: &Sets, names: Names) -> Option<String> {
    if stale(p, current).is_some() {
        return Some("It changed since this was proposed: reject it.".into());
    }
    let n = chars(&p.text);
    if n > TEXT_LIMIT {
        return counter_note(n);
    }
    largest_with(sets, p.guideline_id, n, links).filter(|b| b.chars > RUN_LIMIT).map(|b| block_sentence(b, names))
}

/// The links a new guideline from a proposal gets by default: those it suggests.
pub fn default_links(p: &GuidelineProposalView) -> Vec<GuidelineTarget> {
    p.suggested_links.clone()
}

/// The starter set banner shows while a seeded guideline is still as the seed made it
/// and this board member has not closed it (§8.1).
pub fn banner_due(list: &[GuidelineView], me: Option<&str>, done: &[String]) -> bool {
    me.is_some_and(|me| !done.iter().any(|d| d == me)) && list.iter().any(|g| g.version == 1 && g.updated_by.is_none())
}

/// The line under the counters: why Save is off, once there is something to say (not on
/// an empty new form), and not what the counter beside its bar already says (m6).
pub fn editor_message(failure: Option<String>, problem: Option<String>, title: &str, chars: u32) -> Option<String> {
    failure.or_else(|| problem.filter(|p| chars > 0 && !title.trim().is_empty() && counter_note(chars).as_ref() != Some(p)))
}

/// The status line after a new guideline is saved: with links it already applies (m5).
pub fn created_line(title: &str, links: &[GuidelineTarget], names: Names) -> String {
    if links.is_empty() {
        format!("Saved “{title}” as version 1. Link it to use it.")
    } else {
        format!("Saved “{title}” as version 1. It applies to {} from the next run.", applies_to(links, names).0)
    }
}

/// A Context item of a run's guideline (section `guideline`, id `g:<id>@<version>`):
/// "Rust style v3", so the versions show without trace mode (P-59, Pulse m1/m2).
pub fn context_guideline(id: &str, label: &str) -> String {
    match id.rsplit_once('@').and_then(|(_, v)| v.split('#').next()?.parse::<u32>().ok()) {
        Some(v) => format!("{label} v{v}"),
        None => label.to_string(),
    }
}

/// What the editor's Save would refuse before it goes, or none.
pub fn save_problem(title: &str, text: &str, largest: Option<Block>, names: Names) -> Option<String> {
    let n = chars(text);
    if title.trim().is_empty() {
        Some("Give it a title.".into())
    } else if title.trim().chars().count() > MAX_TITLE_CHARS {
        Some(format!("A title has at most {MAX_TITLE_CHARS} characters."))
    } else if n == 0 {
        Some("A guideline needs a text. To stop using one, archive it.".into())
    } else if n > TEXT_LIMIT {
        counter_note(n)
    } else {
        largest.filter(|b| b.chars > RUN_LIMIT).map(|b| format!("{} Shorten this text or unlink something.", block_sentence(b, names)))
    }
}

/// The editor's changes as `update` sends them: only what changed; none when nothing did (GL-02 AC5).
pub fn changes(original: (&str, &str), now: (&str, &str)) -> Option<(Option<String>, Option<String>)> {
    let title = (original.0.trim() != now.0.trim()).then(|| now.0.trim().to_string());
    let text = (stored(original.1) != stored(now.1)).then(|| stored(now.1).to_string());
    (title.is_some() || text.is_some()).then_some((title, text))
}

#[cfg(test)]
#[path = "logic_tests.rs"]
mod tests;
