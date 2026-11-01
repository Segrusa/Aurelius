//! The guidelines' state in the window, and what it does: asking the server, taking
//! its answers, saving, linking, deciding.

use super::logic::*;
use crate::app::{AppView, EmployeesTab, Overlay, Page};
use crate::backend::{self, GuidelineCall, GuidelineFailure, GuidelineUpdate, GuidelineWrite, LinkChange};
use aurelius_domain::error::ErrorCode;
use aurelius_domain::guideline::{EffectiveGuidelines, GuidelineAffected, GuidelineProposalView, GuidelineTarget, GuidelineVersionView, GuidelineView, RunGuideline, StarterTemplate};
use aurelius_domain::proposal::ProposalStatus;
use aurelius_domain::{CommandResult, EmployeeId, GuidelineId, GuidelineProposalId, RunId};
use gpui_kit::component::input::{InputEvent, InputState, TextareaState};
use gpui_kit::*;
use std::collections::{BTreeSet, HashMap, HashSet};

/// What the Guidelines tab shows.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum GuidelinesView {
    #[default]
    Library,
    Editor,
    /// What one employee gets (GL-07, §6.1).
    Employee(EmployeeId),
}

/// The editor's panes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Write,
    Preview,
    Side,
}

#[derive(Clone, Debug)]
pub struct Editor {
    /// None: a new guideline.
    pub guideline: Option<GuidelineId>,
    /// Edit and approve: Save approves this proposal with the text as it is.
    pub proposal: Option<GuidelineProposalId>,
    /// The version the text came from (0: new).
    pub base_version: u32,
    /// Title and text as loaded: what Save compares against.
    pub original: (String, String),
    /// The text arrived (an existing guideline loads first).
    pub loaded: bool,
    pub mode: Mode,
    pub history: bool,
    /// A version picked in History: its diff against the editor's text.
    pub pick: Option<u32>,
    /// A new guideline's links (also a new one from a proposal).
    pub links: Vec<GuidelineTarget>,
    pub busy: bool,
    pub failure: Option<GuidelineFailure>,
    /// Someone saved this version meanwhile (P-64): Compare or Discard mine.
    pub conflict: Option<u32>,
    pub compare: bool,
    pub archived: bool,
}

impl Editor {
    pub fn new(guideline: Option<GuidelineId>, mode: Mode) -> Self {
        Self {
            guideline,
            proposal: None,
            base_version: 0,
            original: (String::new(), String::new()),
            loaded: guideline.is_none(),
            mode,
            history: false,
            pick: None,
            links: Vec::new(),
            busy: false,
            failure: None,
            conflict: None,
            compare: false,
            archived: false,
        }
    }
}

/// Whose links a Links dialog edits.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Subject {
    /// One guideline: employees and projects (§5.1).
    Guideline(GuidelineId),
    /// The editor's new guideline: its links go with Save.
    Draft,
    /// A proposal of a new guideline: its links go with Approve.
    Proposal(GuidelineProposalId),
    /// One employee or project: every guideline, the other way round (Manage links…).
    Target(GuidelineTarget),
}

#[derive(Clone, Debug)]
pub struct LinksDialog {
    pub subject: Subject,
    /// A guideline's (or a draft's, a proposal's) links before and now.
    pub before: HashSet<GuidelineTarget>,
    pub after: HashSet<GuidelineTarget>,
    /// For a target: the guidelines linked to it before and now.
    pub guidelines_before: BTreeSet<GuidelineId>,
    pub guidelines_after: BTreeSet<GuidelineId>,
    /// The size the guideline counts with (the editor's text, or as saved).
    pub chars: u32,
    /// Groups folded away, by their boss.
    pub folded: HashSet<Option<EmployeeId>>,
    pub busy: bool,
    pub failure: Option<String>,
}

#[derive(Clone, Debug)]
pub enum Dialog {
    Links(LinksDialog),
    /// Add starter set…: the templates with a box each (§8.2).
    Starter { templates: Option<Result<Vec<StarterTemplate>, String>>, picked: BTreeSet<String>, busy: bool },
    /// Reject with a note for the proposer (P-62).
    Reject { proposal: GuidelineProposalId, busy: bool },
    /// Archive, restore a version, or leave the editor with changes (GL-04, GL-03, GL-16 AC3).
    Confirm(Confirm),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Confirm {
    Archive(GuidelineId),
    RestoreVersion(GuidelineId, u32),
    /// Leave the editor and lose what was typed; then show this.
    Discard(GuidelinesView),
}

pub struct Guidelines {
    pub view: GuidelinesView,
    /// The library shows the archived ones (its filter).
    pub archived: bool,
    pub sort: Sort,
    pub list: Option<Result<Vec<GuidelineView>, String>>,
    pub archived_list: Option<Result<Vec<GuidelineView>, String>>,
    /// The lists asked for (false: active, true: archived); an event asks again.
    pub asked: HashSet<bool>,
    pub proposals: Vec<GuidelineProposalView>,
    pub proposals_error: Option<String>,
    pub proposals_asked: bool,
    pub effective: Sets,
    pub effective_asked: HashSet<backend::EffectiveKey>,
    /// Texts of versions (a proposal's base, History, a conflict), by guideline and version.
    pub texts: HashMap<(GuidelineId, u32), GuidelineVersionView>,
    pub texts_asked: HashSet<(GuidelineId, u32)>,
    pub versions: HashMap<GuidelineId, Result<Vec<GuidelineVersionView>, String>>,
    pub affected: HashMap<GuidelineId, Result<GuidelineAffected, String>>,
    pub runs: HashMap<RunId, Result<Vec<RunGuideline>, String>>,
    pub editor: Option<Editor>,
    pub dialog: Option<Dialog>,
    /// The links a new guideline of a proposal gets on Approve (ticked on its card).
    pub proposal_links: HashMap<GuidelineProposalId, Vec<GuidelineTarget>>,
    pub deciding: Option<GuidelineProposalId>,
    /// Board members (ids) who closed the starter set banner here.
    pub banner_done: Vec<String>,
    pub title: Entity<InputState>,
    pub text: Entity<TextareaState>,
    pub query: Entity<InputState>,
    pub note: Entity<TextareaState>,
    /// Title and text to put in the editor on the next frame (an answer has no window).
    pub fill: Option<(String, String)>,
    /// The note empties on the next frame.
    pub clear_note: bool,
}

impl Guidelines {
    pub fn new(window: &mut Window, cx: &mut Context<AppView>) -> Self {
        let banner_done = crate::app::ui_prefs()[BANNER_KEY].as_array().into_iter().flatten().filter_map(|v| v.as_str().map(str::to_owned)).collect();
        Self {
            view: GuidelinesView::Library,
            archived: false,
            sort: Sort::Title,
            list: None,
            archived_list: None,
            asked: HashSet::new(),
            proposals: Vec::new(),
            proposals_error: None,
            proposals_asked: false,
            effective: HashMap::new(),
            effective_asked: HashSet::new(),
            texts: HashMap::new(),
            texts_asked: HashSet::new(),
            versions: HashMap::new(),
            affected: HashMap::new(),
            runs: HashMap::new(),
            editor: None,
            dialog: None,
            proposal_links: HashMap::new(),
            deciding: None,
            banner_done,
            title: cx.new(|cx| InputState::new(window, cx).placeholder("Rust")),
            text: cx.new(|cx| TextareaState::new(window, cx).rows(22).placeholder("The rules, in Markdown: ## headings and - lists work.")),
            query: cx.new(|cx| InputState::new(window, cx).placeholder("Find… (@Forge, ▸project)")),
            note: cx.new(|cx| TextareaState::new(window, cx).auto_grow(3, 8).placeholder("Optional: why, for the employee who proposed it")),
            fill: None,
            clear_note: false,
        }
    }

    /// The editor repaints as its fields change; Ctrl+Enter saves.
    pub fn subscriptions(&self, window: &mut Window, cx: &mut Context<AppView>) -> Vec<Subscription> {
        let mut out = vec![cx.subscribe_in(&self.title, window, |this: &mut AppView, _, event: &InputEvent, _, cx| match event {
            InputEvent::Change => {
                if let Some(e) = this.guidelines.editor.as_mut() {
                    e.failure = None;
                }
                cx.notify();
            }
            InputEvent::PressEnter { secondary: true, .. } => this.guideline_save(cx),
            _ => {}
        })];
        out.push(cx.subscribe_in(&self.text, window, |this: &mut AppView, _, event: &InputEvent, _, cx| match event {
            InputEvent::Change => {
                if let Some(e) = this.guidelines.editor.as_mut() {
                    e.failure = None;
                }
                cx.notify();
            }
            InputEvent::PressEnter { secondary: true, .. } => this.guideline_save(cx),
            _ => {}
        }));
        out.push(cx.subscribe_in(&self.query, window, |_: &mut AppView, _, event: &InputEvent, _, cx| {
            if let InputEvent::Change = event {
                cx.notify();
            }
        }));
        out
    }

    /// The library (active), when it loaded.
    pub fn active(&self) -> &[GuidelineView] {
        match &self.list {
            Some(Ok(list)) => list,
            _ => &[],
        }
    }

    /// A guideline of either list.
    pub fn guideline(&self, id: GuidelineId) -> Option<&GuidelineView> {
        let archived = match &self.archived_list {
            Some(Ok(list)) => list.as_slice(),
            _ => &[],
        };
        self.active().iter().chain(archived).find(|g| g.id == id)
    }

    /// The proposals that wait for a board member, oldest first.
    pub fn pending(&self) -> Vec<&GuidelineProposalView> {
        let mut out: Vec<&GuidelineProposalView> = self.proposals.iter().filter(|p| p.status == ProposalStatus::Pending).collect();
        out.sort_by_key(|p| p.created_at_ms);
        out
    }

    pub fn proposal(&self, id: GuidelineProposalId) -> Option<&GuidelineProposalView> {
        self.proposals.iter().find(|p| p.id == id)
    }

    /// The links a proposal of a new guideline gets on Approve.
    pub fn proposal_links_of(&self, p: &GuidelineProposalView) -> Vec<GuidelineTarget> {
        self.proposal_links.get(&p.id).cloned().unwrap_or_else(|| default_links(p))
    }

    /// The links Approve would weigh: the guideline's own, or the new one's.
    pub fn links_for(&self, p: &GuidelineProposalView) -> Vec<GuidelineTarget> {
        match p.guideline_id.and_then(|g| self.guideline(g)) {
            Some(g) => g.links.clone(),
            None => self.proposal_links_of(p),
        }
    }

    /// Why Approve is off for a proposal as it is (stale, over a limit), or none.
    pub fn approve_problem(&self, p: &GuidelineProposalView, names: Names) -> Option<String> {
        approve_problem(p, p.guideline_id.and_then(|g| self.guideline(g)), &self.links_for(p), &self.effective, names)
    }
}

impl AppView {
    /// The server has guidelines (it says so in its hello).
    pub fn guidelines_ready(&self) -> bool {
        matches!(&self.connection, backend::Connection::Online { server, .. } if server.guidelines)
    }

    /// Only a board member sees what changes something.
    pub fn guidelines_can_edit(&self) -> bool {
        self.active_member().is_some()
    }

    pub(crate) fn gl_names(&self) -> Names<'_> {
        Names { employees: &self.managers, projects: &self.projects }
    }

    pub fn guideline_send(&self, call: GuidelineCall) {
        let _ = self.requests_handle().send(backend::Request::Guidelines(call));
    }

    /// A new server or another board member: nothing of the last one's guidelines stays.
    pub fn guidelines_reset(&mut self) {
        let g = &mut self.guidelines;
        g.list = None;
        g.archived_list = None;
        g.asked.clear();
        g.proposals.clear();
        g.proposals_error = None;
        g.proposals_asked = false;
        g.effective.clear();
        g.effective_asked.clear();
        g.texts.clear();
        g.texts_asked.clear();
        g.versions.clear();
        g.affected.clear();
        g.runs.clear();
        g.editor = None;
        g.dialog = None;
        g.proposal_links.clear();
        g.deciding = None;
        g.view = GuidelinesView::Library;
        g.fill = Some((String::new(), String::new()));
    }

    /// Ask once for what shows: the list of the filter and the proposals (the Inbox counts them).
    pub fn guidelines_want(&mut self) {
        if !self.guidelines_ready() {
            return;
        }
        if self.guidelines.asked.insert(false) {
            self.guideline_send(GuidelineCall::List { archived: false });
        }
        // The archived ones too, once asked for: a guideline opened from there stays found.
        if self.guidelines.archived && self.guidelines.asked.insert(true) {
            self.guideline_send(GuidelineCall::List { archived: true });
        }
        self.guideline_proposals_want();
    }

    pub fn guideline_proposals_want(&mut self) {
        if self.guidelines_ready() && !self.guidelines.proposals_asked {
            self.guidelines.proposals_asked = true;
            self.guideline_send(GuidelineCall::Proposals);
        }
    }

    /// Ask for every set the limit needs (once each; an event asks again).
    pub fn guideline_effective_want(&mut self) {
        if !self.guidelines_ready() || !matches!(self.guidelines.list, Some(Ok(_))) {
            return;
        }
        let contexts = contexts(self.guidelines.active(), &self.projects);
        for key in wanted(&self.managers, &contexts) {
            if self.guidelines.effective_asked.insert(key) {
                self.guideline_send(GuidelineCall::Effective(key));
            }
        }
    }

    pub(crate) fn guideline_text_want(&mut self, guideline_id: GuidelineId, version: u32) {
        if self.guidelines_ready() && self.guidelines.texts_asked.insert((guideline_id, version)) {
            self.guideline_send(GuidelineCall::Get { guideline_id, version: Some(version) });
        }
    }

    pub fn apply_guidelines(&mut self, update: GuidelineUpdate, cx: &mut Context<Self>) {
        match update {
            GuidelineUpdate::Listed { archived, result } => {
                if archived {
                    self.guidelines.archived_list = Some(result);
                } else {
                    self.guidelines.list = Some(result);
                }
            }
            GuidelineUpdate::Detail { guideline_id, version, result } => match result {
                Ok(detail) => {
                    self.guidelines.texts.insert((guideline_id, detail.version.version), detail.version.clone());
                    if version.is_none() {
                        self.guideline_detail_arrived(detail);
                    }
                }
                Err(why) => {
                    if let Some(e) = self.guidelines.editor.as_mut().filter(|e| version.is_none() && e.guideline == Some(guideline_id) && !e.loaded) {
                        e.failure = Some(GuidelineFailure::said(&why));
                    }
                }
            },
            GuidelineUpdate::Versions(id, result) => {
                if let Ok(versions) = &result {
                    for v in versions {
                        self.guidelines.texts.insert((id, v.version), v.clone());
                    }
                }
                self.guidelines.versions.insert(id, result);
            }
            GuidelineUpdate::Effective(key, result) => {
                self.guidelines.effective.insert(key, result);
            }
            GuidelineUpdate::Proposals(result) => match result {
                Ok(proposals) => {
                    let first = self.guidelines.proposals.is_empty() && self.guidelines.proposals_error.is_none();
                    let before: HashSet<GuidelineProposalId> = self.guidelines.pending().into_iter().map(|p| p.id).collect();
                    self.guidelines.proposals = proposals;
                    self.guidelines.proposals_error = None;
                    let new = self.guidelines.pending().into_iter().any(|p| !before.contains(&p.id));
                    if new && !first {
                        self.notify_user("An employee proposed a guideline. Ctrl I opens the inbox.", false, cx);
                    }
                    self.inbox_selected = self.inbox_selected.min(self.inbox_len().saturating_sub(1));
                }
                Err(why) => self.guidelines.proposals_error = Some(why),
            },
            GuidelineUpdate::Affected(id, result) => {
                self.guidelines.affected.insert(id, result);
            }
            GuidelineUpdate::Starter(result) => {
                if let Some(Dialog::Starter { templates, picked, .. }) = self.guidelines.dialog.as_mut() {
                    if let Ok(list) = &result {
                        // Ticked: what is not in the library yet.
                        *picked = list.iter().filter(|t| !t.in_library).map(|t| t.key.clone()).collect();
                    }
                    *templates = Some(result);
                }
            }
            GuidelineUpdate::Run(run_id, result) => {
                self.guidelines.runs.insert(run_id, result);
            }
            GuidelineUpdate::Written { write, done, result } => self.guideline_written(write, done, result, cx),
            GuidelineUpdate::Changed => self.guidelines_changed(),
        }
        cx.notify();
    }

    /// A `guideline.*` event or a write: read again the lists, the proposals and the sets
    /// that showed; the open editor's history and reach too.
    pub fn guidelines_changed(&mut self) {
        let g = &mut self.guidelines;
        g.asked.clear();
        g.proposals_asked = false;
        g.effective_asked.clear();
        g.affected.clear();
        let open = g.editor.as_ref().and_then(|e| e.guideline);
        let history = g.editor.as_ref().is_some_and(|e| e.history);
        let had_sets = !g.effective.is_empty();
        if !self.guidelines_ready() {
            return;
        }
        if self.guidelines.list.is_some() || self.guidelines.proposals_error.is_some() || !self.guidelines.proposals.is_empty() {
            self.guidelines_want();
        } else {
            self.guideline_proposals_want();
        }
        if let Some(id) = open {
            self.guideline_send(GuidelineCall::Affected(id));
            if history {
                self.guideline_send(GuidelineCall::Versions(id));
            }
        }
        // Sets shown before are asked again now (the new list may add contexts once it is in).
        if had_sets {
            self.guidelines.effective.clear();
            self.guideline_effective_want();
        }
    }

    /// The current version of an open guideline arrived: fill the editor (once), or note the
    /// newest version after a refused Save.
    fn guideline_detail_arrived(&mut self, detail: aurelius_domain::guideline::GuidelineDetail) {
        let Some(editor) = self.guidelines.editor.as_mut() else { return };
        if editor.guideline != Some(detail.guideline.id) || editor.proposal.is_some() {
            return;
        }
        if !editor.loaded {
            editor.loaded = true;
            editor.base_version = detail.guideline.version;
            editor.archived = detail.guideline.archived;
            editor.original = (detail.version.title.clone(), detail.version.text.clone());
            self.guidelines.fill = Some((detail.version.title, detail.version.text));
        } else if editor.conflict.is_some() {
            editor.conflict = Some(detail.guideline.version);
        }
    }

    fn guideline_written(&mut self, write: GuidelineWrite, done: usize, result: Result<Option<CommandResult>, GuidelineFailure>, cx: &mut Context<Self>) {
        let why = result.as_ref().err().map(|f| failure_text(f, self.gl_names())).unwrap_or_default();
        // Whatever happened, the server's state is read again.
        self.guidelines_changed();
        let version_of = |result: &Option<CommandResult>| match result {
            Some(CommandResult::GuidelineChanged { version, .. }) => format!(" as version {version}"),
            _ => String::new(),
        };
        match (&write, result) {
            (GuidelineWrite::Create { title, links, .. }, Ok(_)) => {
                self.guideline_close_editor();
                let line = created_line(title, links, self.gl_names());
                self.notify_user(line, false, cx);
            }
            (GuidelineWrite::Update { .. }, Ok(result)) => {
                self.guideline_close_editor();
                self.notify_user(format!("Saved{}. It applies from the next run.", version_of(&result)), false, cx);
            }
            (GuidelineWrite::Create { .. } | GuidelineWrite::Update { .. }, Err(f)) => {
                let conflict = f.code == Some(ErrorCode::VersionConflict);
                let mut ask = None;
                if let Some(e) = self.guidelines.editor.as_mut() {
                    e.busy = false;
                    if conflict {
                        e.conflict = Some(e.base_version + 1);
                        ask = e.guideline;
                    }
                    e.failure = Some(f);
                }
                if let Some(id) = ask {
                    self.guideline_send(GuidelineCall::Get { guideline_id: id, version: None });
                }
            }
            (GuidelineWrite::Archive(id), Ok(_)) => {
                let title = self.guidelines.guideline(*id).map(|g| g.title.clone()).unwrap_or_default();
                if self.guidelines.editor.as_ref().is_some_and(|e| e.guideline == Some(*id)) {
                    self.guideline_close_editor();
                }
                self.notify_user(format!("Archived “{title}”. No new run gets it; it is under Archived."), false, cx);
            }
            (GuidelineWrite::Restore(id), Ok(_)) => {
                let title = self.guidelines.guideline(*id).map(|g| g.title.clone()).unwrap_or_default();
                self.notify_user(format!("Restored “{title}”. Link it again to use it."), false, cx);
            }
            (GuidelineWrite::RestoreVersion { version, .. }, Ok(result)) => {
                // The editor held the old text: it closes instead of saving over the restore.
                self.guideline_close_editor();
                self.notify_user(format!("Restored version {version}{}. It applies from the next run.", version_of(&result)), false, cx);
            }
            (GuidelineWrite::Links(changes), Ok(_)) => {
                if matches!(self.guidelines.dialog, Some(Dialog::Links(_))) {
                    self.guideline_drop_dialog();
                }
                let n = changes.len();
                self.notify_user(format!("Saved {n} link change{}. It applies from the next run.", if n == 1 { "" } else { "s" }), false, cx);
            }
            (GuidelineWrite::Links(changes), Err(_)) => {
                let why = if done > 0 { format!("{done} of {} changes were saved, then: {why}", changes.len()) } else { why };
                match self.guidelines.dialog.as_mut() {
                    // The dialog stays open with the server's message; the selection stays (§5.1).
                    Some(Dialog::Links(d)) => {
                        d.busy = false;
                        d.failure = Some(why);
                    }
                    _ => self.notify_user(why, true, cx),
                }
            }
            (GuidelineWrite::Decide { proposal_id, approve, .. }, Ok(_)) => {
                self.guidelines.deciding = None;
                self.guidelines.proposal_links.remove(proposal_id);
                if self.guidelines.editor.as_ref().is_some_and(|e| e.proposal == Some(*proposal_id)) {
                    self.guideline_close_editor();
                }
                if matches!(self.guidelines.dialog, Some(Dialog::Reject { proposal, .. }) if proposal == *proposal_id) {
                    self.guideline_drop_dialog();
                    self.guidelines.clear_note = true;
                }
                self.notify_user(if *approve { "Approved. It applies from the next run." } else { "Rejected. The employee gets your decision and note." }, false, cx);
            }
            (GuidelineWrite::Decide { proposal_id, .. }, Err(f)) => {
                // GL-14 AC5: the proposal stays open.
                self.guidelines.deciding = None;
                if let Some(e) = self.guidelines.editor.as_mut().filter(|e| e.proposal == Some(*proposal_id)) {
                    e.busy = false;
                    e.failure = Some(f);
                } else {
                    if let Some(Dialog::Reject { busy, .. }) = self.guidelines.dialog.as_mut() {
                        *busy = false;
                    }
                    self.notify_user(why, true, cx);
                }
            }
            (GuidelineWrite::AddStarter(_), Ok(result)) => {
                let n = match result {
                    Some(CommandResult::StarterGuidelinesAdded { guideline_ids }) => guideline_ids.len(),
                    _ => 0,
                };
                self.guideline_drop_dialog();
                self.notify_user(format!("Added {n} guideline{}. Link them to employees to use them.", if n == 1 { "" } else { "s" }), false, cx);
            }
            (GuidelineWrite::AddStarter(_), Err(_)) => {
                if let Some(Dialog::Starter { busy, .. }) = self.guidelines.dialog.as_mut() {
                    *busy = false;
                }
                self.notify_user(why, true, cx);
            }
            (_, Err(_)) => self.notify_user(why, true, cx),
        }
    }

    fn guideline_drop_dialog(&mut self) {
        self.guidelines.dialog = None;
        if self.overlay == Overlay::Guidelines {
            self.overlay = Overlay::None;
        }
    }

    fn guideline_close_editor(&mut self) {
        self.guidelines.editor = None;
        if self.guidelines.view == GuidelinesView::Editor {
            self.guidelines.view = GuidelinesView::Library;
        }
        self.guidelines.fill = Some((String::new(), String::new()));
    }

    /// Employees › Guidelines, on `view`.
    pub fn guidelines_go(&mut self, view: GuidelinesView, window: &mut Window, cx: &mut Context<Self>) {
        self.guidelines.view = view;
        self.employees_tab = EmployeesTab::Guidelines;
        if self.screen != Page::Employees {
            self.navigate(Page::Employees);
        }
        self.focus.focus(window, cx);
        cx.notify();
    }

    /// The palette's "Find guideline…": the library, typing in its search (ui §1).
    pub fn guidelines_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.guidelines.archived = false;
        self.guidelines_go(GuidelinesView::Library, window, cx);
        self.guidelines.query.update(cx, |i, cx| i.focus(window, cx));
    }

    fn editor_mode(window: &Window) -> Mode {
        if f32::from(window.viewport_size().width) >= SIDE_MIN_W { Mode::Side } else { Mode::Write }
    }

    /// Open the editor on a guideline (or a new one: none).
    pub fn guideline_open(&mut self, id: Option<GuidelineId>, window: &mut Window, cx: &mut Context<Self>) {
        let mut editor = Editor::new(id, Self::editor_mode(window));
        if let Some(id) = id {
            editor.archived = self.guidelines.guideline(id).is_some_and(|g| g.archived);
            self.guideline_send(GuidelineCall::Get { guideline_id: id, version: None });
            self.guideline_send(GuidelineCall::Affected(id));
        }
        self.guidelines.editor = Some(editor);
        self.guidelines.fill = Some((String::new(), String::new()));
        self.guideline_effective_want();
        self.guidelines_go(GuidelinesView::Editor, window, cx);
        if id.is_none() {
            self.guidelines.title.update(cx, |i, cx| i.focus(window, cx));
        }
    }

    /// Open a guideline at a version a run had: its History with that version picked (GL-11 AC3).
    pub fn guideline_open_version(&mut self, id: GuidelineId, version: u32, window: &mut Window, cx: &mut Context<Self>) {
        if self.overlay == Overlay::Trace {
            self.overlay = Overlay::None;
        }
        self.guideline_open(Some(id), window, cx);
        if let Some(e) = self.guidelines.editor.as_mut() {
            e.history = true;
            e.pick = Some(version);
        }
        self.guideline_send(GuidelineCall::Versions(id));
    }

    /// Edit and approve: the editor with the proposed text; its Approve approves it (§7.1).
    pub fn guideline_edit_proposal(&mut self, id: GuidelineProposalId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(p) = self.guidelines.proposal(id).cloned() else { return };
        let mut editor = Editor::new(p.guideline_id, Self::editor_mode(window));
        editor.proposal = Some(id);
        editor.loaded = true;
        editor.base_version = p.base_version.unwrap_or(0);
        editor.links = self.guidelines.proposal_links_of(&p);
        editor.original = p.guideline_id.zip(p.base_version).and_then(|k| self.guidelines.texts.get(&k)).map(|v| (v.title.clone(), v.text.clone())).unwrap_or_default();
        self.guidelines.editor = Some(editor);
        self.guidelines.fill = Some((p.title.clone(), p.text.clone()));
        self.guideline_effective_want();
        self.guidelines_go(GuidelinesView::Editor, window, cx);
    }

    /// The editor's title and text now.
    pub(crate) fn guideline_typed(&self, cx: &App) -> (String, String) {
        (self.guidelines.title.read(cx).value().to_string(), self.guidelines.text.read(cx).value().to_string())
    }

    /// The editor holds something Save would keep (GL-16 AC3).
    pub fn guideline_dirty(&self, cx: &App) -> bool {
        let Some(e) = self.guidelines.editor.as_ref() else { return false };
        if !e.loaded || e.busy {
            return false;
        }
        let (title, text) = self.guideline_typed(cx);
        match e.proposal.and_then(|p| self.guidelines.proposal(p)) {
            Some(p) => changes((&p.title, &p.text), (&title, &text)).is_some(),
            None => changes((&e.original.0, &e.original.1), (&title, &text)).is_some(),
        }
    }

    /// The links the editor's guideline weighs with: its own, or a new one's.
    pub(crate) fn guideline_editor_links(&self) -> Vec<GuidelineTarget> {
        let Some(e) = self.guidelines.editor.as_ref() else { return Vec::new() };
        match e.guideline.and_then(|id| self.guidelines.guideline(id)) {
            Some(g) => g.links.clone(),
            None => e.links.clone(),
        }
    }

    /// The largest block a run would get with the editor's text.
    pub(crate) fn guideline_largest(&self, chars: u32) -> Option<Block> {
        let e = self.guidelines.editor.as_ref()?;
        largest_with(&self.guidelines.effective, e.guideline, chars, &self.guideline_editor_links())
    }

    pub fn guideline_save(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.guidelines.editor.clone() else { return };
        if editor.busy || !editor.loaded || editor.archived || !self.guidelines_can_edit() {
            return;
        }
        let (title, text) = self.guideline_typed(cx);
        if let Some(problem) = save_problem(&title, &text, self.guideline_largest(chars(&text)), self.gl_names()) {
            if let Some(e) = self.guidelines.editor.as_mut() {
                e.failure = Some(GuidelineFailure::said(&problem));
            }
            cx.notify();
            return;
        }
        let write = match (editor.proposal, editor.guideline) {
            (Some(proposal_id), guideline) => {
                let p = self.guidelines.proposal(proposal_id).cloned();
                // Only what differs from the proposal goes; a new one takes its links.
                let edits = p.as_ref().map_or(Some((Some(title.trim().to_string()), Some(stored(&text).to_string()))), |p| changes((&p.title, &p.text), (&title, &text)));
                let (title, text) = edits.unwrap_or_default();
                GuidelineWrite::Decide {
                    proposal_id,
                    approve: true,
                    title,
                    text,
                    links: guideline.is_none().then(|| editor.links.clone()),
                    note: None,
                    expected_version: guideline.and(p.and_then(|p| p.base_version)),
                }
            }
            (None, None) => GuidelineWrite::Create { title: title.trim().to_string(), text: stored(&text).to_string(), links: editor.links.clone() },
            (None, Some(id)) => match changes((&editor.original.0, &editor.original.1), (&title, &text)) {
                // Nothing changed: no new version (GL-02 AC5).
                None => {
                    self.guideline_close_editor();
                    cx.notify();
                    return;
                }
                Some((title, text)) => GuidelineWrite::Update { guideline_id: id, expected_version: editor.base_version, title, text },
            },
        };
        if let Some(e) = self.guidelines.editor.as_mut() {
            e.busy = true;
            e.failure = None;
        }
        self.guideline_send(GuidelineCall::Write(write));
        cx.notify();
    }

    /// Leave the editor: asks first when something typed would be lost (GL-16 AC3).
    pub fn guideline_leave(&mut self, to: GuidelinesView, window: &mut Window, cx: &mut Context<Self>) {
        if self.guideline_dirty(cx) {
            self.guideline_dialog(Dialog::Confirm(Confirm::Discard(to)), window, cx);
            return;
        }
        self.guideline_close_editor();
        self.guidelines_go(to, window, cx);
    }

    /// Esc on the Guidelines tab: out of the editor (asking first) or back to the library.
    pub fn guidelines_back(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.screen != Page::Employees || self.employees_tab != EmployeesTab::Guidelines || self.overlay != Overlay::None {
            return false;
        }
        match self.guidelines.view {
            GuidelinesView::Editor => self.guideline_leave(GuidelinesView::Library, window, cx),
            GuidelinesView::Employee(_) => self.guidelines_go(GuidelinesView::Library, window, cx),
            GuidelinesView::Library => return false,
        }
        true
    }

    /// Discard mine after a conflict: the newest version in the editor (P-64).
    pub(crate) fn guideline_take_newest(&mut self, cx: &mut Context<Self>) {
        let Some(e) = self.guidelines.editor.as_ref() else { return };
        let Some(id) = e.guideline else { return };
        let newest = e.conflict.unwrap_or(e.base_version);
        let Some(text) = self.guidelines.texts.get(&(id, newest)).cloned() else {
            self.guideline_send(GuidelineCall::Get { guideline_id: id, version: None });
            return;
        };
        if let Some(e) = self.guidelines.editor.as_mut() {
            e.base_version = newest;
            e.original = (text.title.clone(), text.text.clone());
            e.conflict = None;
            e.compare = false;
            e.failure = None;
        }
        self.guidelines.fill = Some((text.title, text.text));
        cx.notify();
    }

    pub fn guideline_dialog(&mut self, dialog: Dialog, window: &mut Window, cx: &mut Context<Self>) {
        self.guidelines.dialog = Some(dialog);
        self.overlay = Overlay::Guidelines;
        self.overlay_focus.focus(window, cx);
        cx.notify();
    }

    pub(crate) fn guideline_close_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.guideline_drop_dialog();
        self.focus.focus(window, cx);
        cx.notify();
    }

    /// Links… of a guideline, of the editor's new one, of a proposal, or of an employee or project.
    pub fn guideline_links(&mut self, subject: Subject, window: &mut Window, cx: &mut Context<Self>) {
        let typed = self.guidelines.editor.as_ref().map(|_| chars(&self.guideline_typed(cx).1));
        let mut d = LinksDialog {
            subject,
            before: HashSet::new(),
            after: HashSet::new(),
            guidelines_before: BTreeSet::new(),
            guidelines_after: BTreeSet::new(),
            chars: 0,
            folded: HashSet::new(),
            busy: false,
            failure: None,
        };
        match subject {
            Subject::Guideline(id) => {
                let g = self.guidelines.guideline(id);
                d.before = g.map(|g| g.links.iter().copied().collect()).unwrap_or_default();
                let open = self.guidelines.editor.as_ref().is_some_and(|e| e.guideline == Some(id));
                d.chars = if open { typed.unwrap_or(0) } else { g.map_or(0, |g| g.chars) };
            }
            Subject::Draft => {
                d.before = self.guidelines.editor.as_ref().map(|e| e.links.iter().copied().collect()).unwrap_or_default();
                d.chars = typed.unwrap_or(0);
            }
            Subject::Proposal(id) => {
                let p = self.guidelines.proposal(id);
                d.before = p.map(|p| self.guidelines.proposal_links_of(p)).unwrap_or_default().into_iter().collect();
                d.chars = p.map_or(0, |p| chars(&p.text));
            }
            Subject::Target(target) => {
                d.guidelines_before = self.guidelines.active().iter().filter(|g| g.links.contains(&target)).map(|g| g.id).collect();
            }
        }
        d.after = d.before.clone();
        d.guidelines_after = d.guidelines_before.clone();
        self.guideline_effective_want();
        self.guideline_dialog(Dialog::Links(d), window, cx);
    }

    pub(crate) fn guideline_links_save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Dialog::Links(d)) = self.guidelines.dialog.clone() else { return };
        if d.busy {
            return;
        }
        let changes: Vec<LinkChange> = match d.subject {
            Subject::Guideline(id) => link_changes(id, &d.before, &d.after),
            Subject::Target(target) => {
                let add = d.guidelines_after.difference(&d.guidelines_before).map(|g| LinkChange { guideline_id: *g, target, link: true });
                let remove = d.guidelines_before.difference(&d.guidelines_after).map(|g| LinkChange { guideline_id: *g, target, link: false });
                add.chain(remove).collect()
            }
            Subject::Draft => {
                if let Some(e) = self.guidelines.editor.as_mut() {
                    e.links = d.after.iter().copied().collect();
                }
                self.guideline_close_dialog(window, cx);
                return;
            }
            Subject::Proposal(id) => {
                let links: Vec<GuidelineTarget> = d.after.iter().copied().collect();
                if let Some(e) = self.guidelines.editor.as_mut().filter(|e| e.proposal == Some(id)) {
                    e.links = links.clone();
                }
                self.guidelines.proposal_links.insert(id, links);
                self.guideline_close_dialog(window, cx);
                return;
            }
        };
        if changes.is_empty() {
            self.guideline_close_dialog(window, cx);
            return;
        }
        if let Some(Dialog::Links(d)) = self.guidelines.dialog.as_mut() {
            d.busy = true;
            d.failure = None;
        }
        self.guideline_send(GuidelineCall::Write(GuidelineWrite::Links(changes)));
        cx.notify();
    }

    /// Approve as proposed (with the new one's links), or reject with the note.
    pub fn guideline_decide(&mut self, id: GuidelineProposalId, approve: bool, cx: &mut Context<Self>) {
        let Some(p) = self.guidelines.proposal(id).cloned() else { return };
        if self.guidelines.deciding.is_some() || !self.guidelines_can_edit() {
            return;
        }
        let note = if approve { None } else { Some(self.guidelines.note.read(cx).value().trim().to_string()).filter(|n| !n.is_empty()) };
        let links = (approve && p.guideline_id.is_none()).then(|| self.guidelines.proposal_links_of(&p));
        self.guidelines.deciding = Some(id);
        if let Some(Dialog::Reject { busy, .. }) = self.guidelines.dialog.as_mut() {
            *busy = true;
        }
        self.guideline_send(GuidelineCall::Write(GuidelineWrite::Decide { proposal_id: id, approve, title: None, text: None, links, note, expected_version: p.guideline_id.and(p.base_version) }));
        cx.notify();
    }

    /// The inbox's keyboard: Approve only when the card allows it; Reject asks for the note.
    pub fn guideline_decide_selected(&mut self, id: GuidelineProposalId, approve: bool, window: &mut Window, cx: &mut Context<Self>) {
        if !approve {
            self.guidelines.clear_note = true;
            self.guideline_dialog(Dialog::Reject { proposal: id, busy: false }, window, cx);
            return;
        }
        let Some(p) = self.guidelines.proposal(id).cloned() else { return };
        if self.guidelines.approve_problem(&p, self.gl_names()).is_some() {
            self.notify_user("This proposal can't be approved as it is. Reject it, or use Edit and approve…", true, cx);
            return;
        }
        self.guideline_decide(id, true, cx);
    }

    pub(crate) fn guideline_confirmed(&mut self, confirm: Confirm, window: &mut Window, cx: &mut Context<Self>) {
        self.guideline_close_dialog(window, cx);
        match confirm {
            Confirm::Archive(id) => self.guideline_send(GuidelineCall::Write(GuidelineWrite::Archive(id))),
            Confirm::RestoreVersion(id, version) => self.guideline_send(GuidelineCall::Write(GuidelineWrite::RestoreVersion { guideline_id: id, version })),
            Confirm::Discard(to) => {
                self.guideline_close_editor();
                self.guidelines_go(to, window, cx);
            }
        }
    }

    /// Got it: the banner stays away for this board member, here (§8.1).
    pub(crate) fn guideline_banner_done(&mut self, cx: &mut Context<Self>) {
        if let Some(me) = self.active_member().map(|u| u.id.to_string()) {
            if !self.guidelines.banner_done.contains(&me) {
                self.guidelines.banner_done.push(me);
            }
            crate::app::set_ui_value(BANNER_KEY, serde_json::json!(self.guidelines.banner_done));
        }
        cx.notify();
    }

    /// The trace of a run opened: what guidelines it had (P-59).
    pub fn guideline_run_want(&mut self, run_id: RunId) {
        if self.guidelines_ready() {
            self.guidelines.runs.remove(&run_id);
            self.guideline_send(GuidelineCall::Run(run_id));
        }
    }

    /// The guideline proposals the Inbox lists after the other proposals.
    pub fn pending_guideline_proposals(&self) -> Vec<&GuidelineProposalView> {
        if self.guidelines_ready() { self.guidelines.pending() } else { Vec::new() }
    }

    /// The selected inbox item, when it is a guideline proposal.
    pub fn inbox_guideline_proposal(&self) -> Option<&GuidelineProposalView> {
        let i = self.inbox_selected.checked_sub(self.pending_approvals().count() + self.pending_proposals().len()).filter(|_| self.inbox_tab.shows_head())?;
        self.pending_guideline_proposals().get(i).copied()
    }

    /// The sets of the effective view, for tests and views.
    pub fn guideline_set(&self, employee: Option<EmployeeId>, project: Option<aurelius_domain::ProjectId>) -> Option<&EffectiveGuidelines> {
        self.guidelines.effective.get(&(employee, project))?.as_ref().ok()
    }
}
