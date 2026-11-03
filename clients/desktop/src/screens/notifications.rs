//! The bell beside "Session" (ticket #12): its popout lists what waits for the reader and what
//! happened, and each row leads to the place it is about, so a question is answered in the channel
//! or conversation it was asked in. Something new also shows for a moment as a toast.
//! What has no conversation of its own (a proposal) is decided in a dialog (`render_decision`).

use crate::app::{AppView, ConfirmKind, Decision, NoteKind, Overlay, PLAN_TOOL, Page, Panel, QUESTION_TOOL, note_kind};
use crate::backend;
use crate::keymap::NOTIFICATIONS;
use crate::screens::updates;
use crate::theme::{self, c};
use crate::ui::{self, h, v};
use aurelius_domain::proposal::ProposalKind;
use aurelius_domain::{ApprovalId, ConversationId, GuidelineProposalId, ProposalId};
use aurelius_protocol::{ApprovalView, NotificationView};
use gpui_kit::base::{Toast, ToastManager, ToastMotion, ToastOptions, ToastStack, ToastStackState, ToastTransitionStatus};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

/// How long a toast stays, while the mouse is not on the stack.
pub const TOAST_MS: u64 = 6_000;
/// And one that waits for the reader: it should not be missed.
pub const TOAST_NEEDS_YOU_MS: u64 = 10_000;
/// How many toasts are kept at once; older ones remain in the bell's list.
pub const TOASTS_MAX: usize = 5;
/// How often the toasts' clock advances while there are any.
const TOAST_TICK_MS: u64 = 100;
const TOAST_W: f32 = 360.;
/// Below the title bar, under the bell.
const TOAST_TOP: f32 = 54.;
const POPOUT_W: f32 = 400.;
const POPOUT_H_MAX: f32 = 520.;
const POPOUT_HEADER_H: f32 = 44.;
/// The badge shows at most this number, then "99+".
const BADGE_MAX: usize = 99;
/// The bell's place in `dropdown_bounds`.
const BELL: &str = "notifications-button";
const NEEDS_YOU: &str = "NEEDS YOU";

/// What a row of the list or a toast is about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Item {
    Approval(ApprovalId),
    Proposal(ProposalId),
    Guideline(GuidelineProposalId),
    /// A notification, by its id.
    Note(String),
}

impl Item {
    /// Stable over renders: the key of a toast.
    fn key(&self) -> String {
        match self {
            Item::Approval(id) => format!("approval-{id}"),
            Item::Proposal(id) => format!("proposal-{id}"),
            Item::Guideline(id) => format!("guideline-{id}"),
            Item::Note(id) => format!("note-{id}"),
        }
    }
}

/// The mark in front of a row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    /// ◆: it waits for the reader; solid for a plan or a ticket's row.
    Waits { solid: bool },
    Unread,
    Failed,
    Read,
}

/// One row of the list.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub item: Item,
    pub mark: Mark,
    pub title: String,
    pub meta: String,
    pub at_ms: i64,
    pub unread: bool,
    /// `+2`: more events behind this row, in the ticket's Activity.
    pub more: Option<String>,
}

impl Row {
    fn waits(&self) -> bool {
        matches!(self.mark, Mark::Waits { .. })
    }

    /// The section a row stands under; `day` is the day's name of its time.
    fn section(&self, day: impl Fn(i64) -> &'static str) -> &'static str {
        if self.waits() { NEEDS_YOU } else { day(self.at_ms) }
    }
}

/// One toast: a row in short, kept until it leaves.
pub struct ToastCard {
    item: Item,
    title: String,
    meta: String,
    waits: bool,
}

/// The state of the bell, its popout and the toasts.
pub struct Bell {
    pub selected: usize,
    /// Notifications that were unread when the popout opened (shown as new).
    pub new: Vec<String>,
    /// Rows opened here, with their `created_at_ms` then: read until the server has said so, or
    /// until a new event moves them.
    pub opened: HashMap<String, i64>,
    pub scroll: ScrollHandle,
    toasts: ToastManager<String, ToastCard>,
    stack: ToastStackState,
    ticking: bool,
    /// The first list after connecting is what was already there: no toasts for it.
    approvals_loaded: bool,
    proposals_loaded: bool,
}

impl Default for Bell {
    fn default() -> Self {
        Self {
            selected: 0,
            new: Vec::new(),
            opened: HashMap::new(),
            scroll: ScrollHandle::default(),
            toasts: ToastManager::new(ToastMotion::sonner()),
            stack: ToastStackState::default(),
            ticking: false,
            approvals_loaded: false,
            proposals_loaded: false,
        }
    }
}

/// Whether a notification waits for this reader. `mine`: a ticket's main task waits for them
/// (freeze §5); a row of the personal Inbox has `needs_you` from the server.
pub fn note_waits(n: &NotificationView, mine: bool) -> bool {
    match note_kind(n) {
        NoteKind::Update => n.needs_you,
        NoteKind::NeedsYou => mine,
        _ => false,
    }
}

/// The notifications the bell lists: those that wait first, then as the server sends them. The
/// alert of an approval or proposal is not among them: that has its own row.
pub fn listed_notes<'a>(notes: &[&'a NotificationView], mine: impl Fn(&NotificationView) -> bool) -> Vec<&'a NotificationView> {
    let mut out: Vec<&NotificationView> = notes.iter().copied().filter(|n| note_kind(n) != NoteKind::Hidden).collect();
    out.sort_by_key(|n| !note_waits(n, mine(n)));
    out
}

/// The bell's number (ui-ontwerp §4.3): what waits, what failed and is unread, and the unread rows
/// of the personal Inbox. A row counts once. Results of finished tasks give no number. `head`: the
/// pending approvals and proposals.
pub fn badge_count(head: usize, notes: &[&NotificationView], mine: impl Fn(&NotificationView) -> bool, opened: &HashMap<String, i64>) -> usize {
    let counted = notes.iter().filter(|n| match note_kind(n) {
        NoteKind::Failed => !n.read && mine(n),
        NoteKind::Update => n.needs_you || updates::row_unread(n, opened),
        NoteKind::NeedsYou => mine(n),
        _ => false,
    });
    head + counted.count()
}

/// `3`, or `99+` past what fits the badge.
pub fn badge_text(count: usize) -> String {
    if count > BADGE_MAX { format!("{BADGE_MAX}+") } else { count.to_string() }
}

/// What the bell says to a screen reader.
pub fn bell_label(count: usize) -> String {
    match count {
        0 => "Notifications".into(),
        n => format!("Notifications, {n} new"),
    }
}

/// Whether opening the popout marks everything read: as ever, unless an unread row of the personal
/// Inbox would go with it; that one is read when it is opened, not when it is seen (§4.4).
pub fn opening_marks_all_read(notes: &[&NotificationView], opened: &HashMap<String, i64>) -> bool {
    let unread_update = notes.iter().any(|n| note_kind(n) == NoteKind::Update && updates::row_unread(n, opened));
    !unread_update && notes.iter().any(|n| !n.read)
}

/// What deserves a toast in a new list: unread notifications that were not in the last list, or
/// that got a new event since. `before`: `None` for the first list after connecting.
pub fn new_notes<'a>(before: Option<&[NotificationView]>, after: &'a [NotificationView]) -> Vec<&'a NotificationView> {
    let Some(before) = before else { return Vec::new() };
    after
        .iter()
        .filter(|n| !n.read && note_kind(n) != NoteKind::Hidden)
        .filter(|n| before.iter().find(|b| b.id == n.id).is_none_or(|b| n.created_at_ms > b.created_at_ms))
        .collect()
}

/// Which child of the list row `index` is: each section starts with a header.
fn child_of(rows: &[Row], index: usize, day: impl Fn(i64) -> &'static str) -> usize {
    let mut children = 0;
    let mut section = None;
    for (i, row) in rows.iter().enumerate() {
        let here = row.section(&day);
        if section != Some(here) {
            section = Some(here);
            children += 1;
        }
        if i == index {
            return children;
        }
        children += 1;
    }
    children
}

/// Where the popout goes: its left, top and greatest height. It hangs under the bell with their
/// right edges in line, and stays in the window. `bell`: the bell's right and bottom, once drawn.
fn popout_frame(vw: f32, vh: f32, bell: Option<(f32, f32)>) -> (f32, f32, f32) {
    let (right, bottom) = bell.unwrap_or((vw - 8., 46.));
    let top = bottom + 4.;
    let x = (right - POPOUT_W).min(vw - POPOUT_W - 8.).max(8.);
    (x, top, (vh - top - 12.).clamp(160., POPOUT_H_MAX))
}

/// The sentence of an approval: who asks what.
pub fn approval_title(a: &ApprovalView) -> String {
    if a.tool == QUESTION_TOOL {
        format!("{} asks you", a.requester)
    } else if a.tool == PLAN_TOOL {
        format!("{} has a plan", a.requester)
    } else {
        crate::screens::approval::sentence(a)
    }
}

/// The bell, drawn with divs like the other glyphs of the title bar.
fn bell_glyph(color: u32) -> Div {
    v().w(px(13.)).h(px(14.)).items_center()
        .child(div().w(px(9.)).h(px(8.)).rounded_tl(px(5.)).rounded_tr(px(5.)).bg(c(color)))
        .child(div().w(px(13.)).h(px(2.)).rounded(px(1.)).bg(c(color)))
        .child(div().mt(px(1.)).w(px(4.)).h(px(2.)).rounded_bl(px(2.)).rounded_br(px(2.)).bg(c(color)))
}

fn mark_of(mark: Mark) -> AnyElement {
    match mark {
        Mark::Waits { solid } => ui::diamond(8., theme::ACCENT_TEXT, solid).into_any_element(),
        Mark::Unread => ui::dot(7., theme::ACCENT).into_any_element(),
        Mark::Failed => ui::dot(7., theme::NEGATIVE).into_any_element(),
        Mark::Read => ui::ring(7., theme::MUTED).into_any_element(),
    }
}

impl AppView {
    /// The bell's number.
    pub fn notification_count(&self) -> usize {
        badge_count(self.waiting_head(), &self.all_notifications(), |n| self.note_is_mine(n), &self.bell.opened)
    }

    /// The bell in the title bar; a click opens the popout right below it.
    pub fn render_bell(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let count = self.notification_count();
        let open = self.overlay == Overlay::Notifications;
        h().id(BELL).relative().flex_none().justify_center().w(px(32.)).h(px(28.)).rounded(px(6.)).cursor_pointer()
            .role(Role::Button)
            .aria_label(bell_label(count))
            .when(open, |el| el.bg(c(theme::SURFACE_2)))
            .hover(|s| s.bg(c(theme::SURFACE_2)))
            .tooltip(ui::tip("Notifications", Some("secondary-i")))
            .occlude()
            .on_click(cx.listener(|_, _: &ClickEvent, w, cx| w.dispatch_action(Box::new(crate::keymap::OpenNotifications), cx)))
            .child(bell_glyph(if count > 0 { theme::TEXT } else { theme::TEXT_2 }))
            .when(count > 0, |el| {
                el.child(
                    div().absolute().top(px(1.)).right(px(1.)).min_w(px(14.)).h(px(14.)).px(px(3.)).rounded(px(7.)).flex().items_center().justify_center()
                        .bg(c(theme::ACCENT)).text_color(c(theme::BG)).text_size(px(10.)).font_weight(FontWeight::BOLD)
                        .child(badge_text(count)),
                )
            })
            .child(ui::track_bounds(self.dropdown_bounds.clone(), BELL))
    }

    /// Ctrl+I and the bell: open the popout, or close it again.
    pub fn toggle_notifications(&mut self, _: &crate::keymap::OpenNotifications, window: &mut Window, cx: &mut Context<Self>) {
        if self.overlay == Overlay::Notifications {
            self.overlay = Overlay::None;
            self.focus.focus(window, cx);
            cx.notify();
            return;
        }
        // A board dialog draws over everything: the popout would open unseen beneath it.
        if self.board.dialog.is_some() {
            return;
        }
        self.bell.selected = 0;
        self.bell.new = self.all_notifications().iter().filter(|n| !n.read && note_kind(n) != NoteKind::Update).map(|n| n.id.clone()).collect();
        if opening_marks_all_read(&self.all_notifications(), &self.bell.opened) {
            let _ = self.requests_handle().send(backend::Request::MarkNotificationsRead);
        }
        self.dismiss_all_toasts(cx);
        self.overlay = Overlay::Notifications;
        self.overlay_focus.focus(window, cx);
        cx.notify();
    }

    /// The rows of the list: what waits first, then the rest as the server sends it.
    pub fn notification_rows(&self) -> Vec<Row> {
        let mut rows: Vec<Row> = Vec::new();
        rows.extend(self.pending_approvals().map(|a| Row {
            item: Item::Approval(a.id),
            mark: Mark::Waits { solid: a.tool == PLAN_TOOL },
            title: approval_title(a),
            meta: self.notification_meta(None, a.task_id, ""),
            at_ms: a.requested_at_ms,
            unread: false,
            more: None,
        }));
        rows.extend(self.pending_proposals().into_iter().map(|p| Row {
            item: Item::Proposal(p.id),
            mark: Mark::Waits { solid: false },
            title: if p.kind == ProposalKind::Instructions { format!("Instructions: {}", p.target_employee_name.clone().unwrap_or_else(|| p.name.clone())) } else { format!("Colleague: {}", p.name) },
            meta: format!("Proposed by {}", p.proposed_by_name),
            at_ms: p.created_at_ms,
            unread: false,
            more: None,
        }));
        rows.extend(self.pending_guideline_proposals().into_iter().map(|p| {
            let (title, meta) = self.guideline_proposal_row(p);
            Row { item: Item::Guideline(p.id), mark: Mark::Waits { solid: false }, title, meta, at_ms: p.created_at_ms, unread: false, more: None }
        }));
        let notes = self.all_notifications();
        rows.extend(listed_notes(&notes, |n| self.note_is_mine(n)).into_iter().map(|n| self.note_row(n)));
        // What waits stands together, whichever list it comes from.
        rows.sort_by_key(|r| !r.waits());
        rows
    }

    fn note_row(&self, n: &NotificationView) -> Row {
        let waits = note_waits(n, self.note_is_mine(n));
        if updates::is_update(n) {
            let unread = updates::row_unread(n, &self.bell.opened);
            let title = n.ticket_id.and_then(|id| self.board.ticket(id)).map(|t| t.title.as_str());
            let mark = match updates::row_mark(n, unread) {
                _ if waits => Mark::Waits { solid: true },
                updates::RowMark::Unread => Mark::Unread,
                _ => Mark::Read,
            };
            return Row { item: Item::Note(n.id.clone()), mark, title: updates::heading(n), meta: updates::place(n, title), at_ms: n.created_at_ms, unread, more: updates::more(n) };
        }
        let unread = self.bell.new.contains(&n.id) || !n.read;
        let mark = match note_kind(n) {
            _ if waits => Mark::Waits { solid: false },
            NoteKind::Failed => Mark::Failed,
            _ if unread => Mark::Unread,
            _ => Mark::Read,
        };
        Row { item: Item::Note(n.id.clone()), mark, title: n.title.clone(), meta: self.notification_meta(None, n.task_id, &n.subject), at_ms: n.created_at_ms, unread, more: None }
    }

    /// Up or down in the list, keeping the row in view.
    pub fn notification_move(&mut self, delta: isize, cx: &mut Context<Self>) {
        let rows = self.notification_rows();
        let last = rows.len().saturating_sub(1) as isize;
        self.bell.selected = (self.bell.selected as isize + delta).clamp(0, last) as usize;
        self.bell.scroll.scroll_to_item(child_of(&rows, self.bell.selected, ui::day_group));
        cx.notify();
    }

    /// Enter in the list.
    pub fn open_selected_notification(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(row) = self.notification_rows().into_iter().nth(self.bell.selected) {
            self.open_notification(row.item, window, cx);
        }
    }

    /// Go to what a row or toast is about: the conversation or channel where it is asked. What has
    /// no such place is decided in its dialog.
    pub fn open_notification(&mut self, item: Item, window: &mut Window, cx: &mut Context<Self>) {
        self.dismiss_toast(&item, cx);
        self.overlay = Overlay::None;
        // A toast can be clicked over a dialog; what it opens would stay hidden behind that.
        if self.board.dialog.is_some() {
            self.board_close_dialog(window, cx);
        }
        if self.project_settings.is_some() {
            self.close_project_settings(cx);
        }
        match item {
            Item::Approval(id) => match self.approval_home(id) {
                Some(conversation) => self.open_conversation_anywhere(conversation, window, cx),
                None => self.open_decision(Decision::Approval(id), window, cx),
            },
            Item::Proposal(id) => self.open_decision(Decision::Proposal(id), window, cx),
            Item::Guideline(id) => self.open_decision(Decision::Guideline(id), window, cx),
            Item::Note(id) => self.open_note(&id, window, cx),
        }
        cx.notify();
    }

    /// The conversation that shows an approval's card, when this client can open it.
    fn approval_home(&self, id: ApprovalId) -> Option<ConversationId> {
        let a = self.pending_approvals().find(|a| a.id == id)?;
        let conversation = a.conversation_id.or_else(|| a.task_id.and_then(|t| self.tasks.iter().find(|x| x.id == t)).and_then(|t| t.conversation_id))?;
        self.conversation_can_open(conversation).then_some(conversation)
    }

    /// Open a notification: a row of the personal Inbox on the right place, another on its task's
    /// conversation, or else on the task.
    fn open_note(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(n) = self.all_notifications().into_iter().find(|n| n.id == id).cloned() else {
            self.notify_user("This notification is gone.", false, cx);
            return;
        };
        if updates::is_update(&n) {
            self.open_update_row(&n, window, cx);
            return;
        }
        let Some(task) = n.task_id.and_then(|t| self.tasks.iter().find(|x| x.id == t)).cloned() else {
            self.notify_user("This notification has nothing to open.", false, cx);
            return;
        };
        match task.conversation_id.filter(|c| self.conversation_can_open(*c)) {
            Some(conversation) => self.open_conversation_anywhere(conversation, window, cx),
            None => {
                self.show_task(task.id, window, cx);
                self.focus.focus(window, cx);
            }
        }
    }

    /// A task's Review: its first pending approval, on the place it is asked.
    pub fn open_task_approval(&mut self, task: aurelius_domain::TaskId, window: &mut Window, cx: &mut Context<Self>) {
        let first = self.pending_approvals().find(|a| a.task_id == Some(task)).map(|a| a.id);
        match first {
            Some(id) => self.open_notification(Item::Approval(id), window, cx),
            None => self.notify_user("Nothing of this task waits for you any more.", false, cx),
        }
    }

    /// Open the dialog of something to decide on.
    pub fn open_decision(&mut self, decision: Decision, window: &mut Window, cx: &mut Context<Self>) {
        self.question_focus = 0;
        self.overlay = Overlay::Decision(decision);
        self.overlay_focus.focus(window, cx);
        cx.notify();
    }

    /// The approval of an open dialog, when it is still pending.
    pub fn decision_approval(&self) -> Option<&ApprovalView> {
        let Overlay::Decision(Decision::Approval(id)) = self.overlay else { return None };
        self.pending_approvals().find(|a| a.id == id)
    }

    /// Whether what a dialog is about still waits.
    fn decision_waits(&self, decision: Decision) -> bool {
        match decision {
            Decision::Approval(id) => self.pending_approvals().any(|a| a.id == id),
            Decision::Proposal(id) => self.pending_proposals().iter().any(|p| p.id == id),
            Decision::Guideline(id) => self.pending_guideline_proposals().iter().any(|p| p.id == id),
        }
    }

    /// A list came in: a dialog on something decided (here or on another device) closes, and its
    /// toast goes.
    pub fn after_waiting_changed(&mut self, cx: &mut Context<Self>) {
        if let Overlay::Decision(decision) = self.overlay {
            if !self.decision_waits(decision) {
                self.overlay = Overlay::None;
            }
        }
        let gone: Vec<Item> = self.bell.toasts.iter().filter(|(_, card, status)| *status != ToastTransitionStatus::Ending && !self.item_still_here(&card.item)).map(|(_, card, _)| card.item.clone()).collect();
        for item in gone {
            self.dismiss_toast(&item, cx);
        }
        let rows = self.notification_rows().len();
        self.bell.selected = self.bell.selected.min(rows.saturating_sub(1));
    }

    fn item_still_here(&self, item: &Item) -> bool {
        match item {
            Item::Approval(id) => self.decision_waits(Decision::Approval(*id)),
            Item::Proposal(id) => self.decision_waits(Decision::Proposal(*id)),
            Item::Guideline(id) => self.decision_waits(Decision::Guideline(*id)),
            Item::Note(id) => self.all_notifications().iter().any(|n| n.id == *id),
        }
    }

    /// The approvals changed: a toast for each that is new since `before`.
    pub fn announce_approvals(&mut self, before: &HashSet<ApprovalId>, cx: &mut Context<Self>) {
        if !std::mem::replace(&mut self.bell.approvals_loaded, true) {
            return;
        }
        let new: Vec<ApprovalView> = self.pending_approvals().filter(|a| !before.contains(&a.id)).cloned().collect();
        for a in new {
            let meta = self.notification_meta(None, a.task_id, "");
            self.announce(Item::Approval(a.id), approval_title(&a), meta, true, a.conversation_id, cx);
        }
    }

    /// The proposals changed: a toast for each that is new since `before`.
    pub fn announce_proposals(&mut self, before: &HashSet<ProposalId>, cx: &mut Context<Self>) {
        if !std::mem::replace(&mut self.bell.proposals_loaded, true) {
            return;
        }
        let new: Vec<_> = self.pending_proposals().into_iter().filter(|p| !before.contains(&p.id)).map(|p| (p.id, p.name.clone(), p.proposed_by_name.clone(), p.kind)).collect();
        for (id, name, by, kind) in new {
            let title = if kind == ProposalKind::Instructions { format!("{by} proposes new instructions") } else { format!("{by} proposes {name}") };
            self.announce(Item::Proposal(id), title, "Waiting for your decision".into(), true, None, cx);
        }
    }

    /// The notifications changed: a toast for each that is new since `before`.
    pub fn announce_notes(&mut self, before: Option<&[NotificationView]>, after: &[NotificationView], cx: &mut Context<Self>) {
        for n in new_notes(before, after).into_iter().rev() {
            let mine = self.note_is_mine(n);
            if !mine && note_kind(n) != NoteKind::Update {
                continue;
            }
            let row = self.note_row(n);
            self.announce(row.item, row.title, row.meta, note_waits(n, mine), n.conversation_id, cx);
        }
    }

    /// Show a toast, unless the popout already lists it or its conversation is in view.
    pub fn announce(&mut self, item: Item, title: String, meta: String, waits: bool, conversation: Option<ConversationId>, cx: &mut Context<Self>) {
        if self.overlay == Overlay::Notifications || conversation.is_some_and(|id| self.conversation_in_view(id)) {
            return;
        }
        let timeout = Duration::from_millis(if waits { TOAST_NEEDS_YOU_MS } else { TOAST_MS });
        let key = item.key();
        self.bell.toasts.push(key, ToastCard { item, title, meta, waits }, ToastOptions { timeout: Some(timeout) }, Instant::now());
        // The oldest make way once there are too many; the bell still lists them.
        let active: Vec<String> = self.bell.toasts.iter().filter(|(_, _, s)| *s != ToastTransitionStatus::Ending).map(|(k, _, _)| k.clone()).collect();
        for key in active.iter().take(active.len().saturating_sub(TOASTS_MAX)) {
            self.bell.toasts.dismiss(key, Instant::now());
        }
        self.tick_toasts(cx);
        cx.notify();
    }

    /// The conversation is on screen right now.
    fn conversation_in_view(&self, id: ConversationId) -> bool {
        self.open_conversation == Some(id) && (matches!(self.screen, Page::Employee(_) | Page::Project(..) | Page::GroupChat) || matches!(self.panel, Panel::Task(_) | Panel::Chat(_)))
    }

    /// Advance the toasts' clock for as long as there are any. Hovering the stack pauses it.
    fn tick_toasts(&mut self, cx: &mut Context<Self>) {
        if std::mem::replace(&mut self.bell.ticking, true) {
            return;
        }
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(Duration::from_millis(TOAST_TICK_MS)).await;
            let more = this.update(cx, |this, cx| {
                let paused = this.bell.stack.is_expanded();
                if this.bell.toasts.advance(Instant::now(), paused).changed {
                    cx.notify();
                }
                let more = !this.bell.toasts.is_empty();
                this.bell.ticking = more;
                more
            });
            if !more.unwrap_or(false) {
                break;
            }
        })
        .detach();
    }

    fn dismiss_toast(&mut self, item: &Item, cx: &mut Context<Self>) {
        if self.bell.toasts.dismiss(&item.key(), Instant::now()) {
            cx.notify();
        }
    }

    fn dismiss_all_toasts(&mut self, cx: &mut Context<Self>) {
        if !self.bell.toasts.dismiss_all(Instant::now()).is_empty() {
            cx.notify();
        }
    }

    /// The popout under the bell. A click beside it closes it, as with a menu.
    pub fn render_notifications(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let size = window.viewport_size();
        let (vw, vh) = (f32::from(size.width), f32::from(size.height));
        let bell = self.dropdown_bounds.borrow().get(BELL).copied();
        let (x, top, height) = popout_frame(vw, vh, bell.map(|b| (f32::from(b.origin.x + b.size.width), f32::from(b.origin.y + b.size.height))));
        let now = ui::now_ms();
        let rows = self.notification_rows();
        let selected = self.bell.selected.min(rows.len().saturating_sub(1));
        let mut list: Vec<AnyElement> = Vec::new();
        let mut section = None;
        for (i, row) in rows.iter().enumerate() {
            let here = row.section(ui::day_group);
            if section != Some(here) {
                section = Some(here);
                list.push(h().px(px(10.)).pt(px(10.)).pb(px(4.)).child(ui::eyebrow(here)).into_any_element());
            }
            list.push(self.notification_row(row, i, i == selected, now, cx));
        }
        let unread = self.personal_inbox_supported() && self.all_notifications().iter().any(|n| !n.read);
        let clearable = self.all_notifications().iter().any(|n| matches!(note_kind(n), NoteKind::Result | NoteKind::Failed | NoteKind::Update));
        let link = |id: &'static str, text: &'static str| div().id(id).flex_none().text_size(px(11.5)).text_color(c(theme::ACCENT_TEXT)).cursor_pointer().hover(|s| s.text_color(c(theme::TEXT))).child(text);
        let scroll = self.bell.scroll.clone();
        deferred(
            div().id("notifications-scrim").absolute().top_0().left_0().size_full().occlude()
                .on_mouse_down(MouseButton::Left, cx.listener(|_, _, w, cx| w.dispatch_action(Box::new(crate::keymap::Back), cx)))
                .on_mouse_down(MouseButton::Right, cx.listener(|_, _, w, cx| w.dispatch_action(Box::new(crate::keymap::Back), cx)))
                .child(ui::fade_in(
                    v().id("notifications").role(Role::Dialog).aria_label("Notifications").absolute().left(px(x)).top(px(top)).w(px(POPOUT_W)).max_h(px(height))
                        .overflow_hidden().rounded(px(10.)).border_1().border_color(c(theme::BORDER_STRONG)).bg(c(theme::PANEL)).shadow_lg()
                        .track_focus(&self.overlay_focus)
                        .key_context(NOTIFICATIONS)
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
                        .child(
                            h().flex_none().h(px(POPOUT_HEADER_H)).px(px(14.)).gap(px(12.)).border_b_1().border_color(c(theme::BORDER))
                                .child(div().flex_1().min_w_0().font_family(ui::serif(cx)).text_size(px(16.)).child("Notifications"))
                                .when(unread, |el| {
                                    el.child(link("notifications-read", "Mark all read").tooltip(ui::tip(format!("Only for you. {}", updates::OTHERS_KEEP_THEIRS), None)).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                        let _ = this.requests_handle().send(backend::Request::MarkNotificationsRead);
                                        cx.notify();
                                    })))
                                })
                                .when(clearable, |el| {
                                    el.child(link("notifications-clear", "Clear all").on_click(cx.listener(|this, _: &ClickEvent, w, cx| {
                                        this.overlay = Overlay::Confirm(ConfirmKind::ClearResults);
                                        this.overlay_focus.focus(w, cx);
                                        cx.notify();
                                    })))
                                }),
                        )
                        .when(rows.is_empty(), |el| {
                            el.child(
                                v().p(px(22.)).gap(px(6.)).items_center()
                                    .child(div().font_family(ui::serif(cx)).italic().text_size(px(16.)).text_color(c(theme::TEXT_2)).child("Nothing here."))
                                    .child(div().text_size(px(12.)).line_height(px(17.)).text_center().text_color(c(theme::MUTED)).child("Questions, approvals and plans for you appear here, and each leads to where it was asked.")),
                            )
                        })
                        .when(!rows.is_empty(), |el| {
                            el.child(ui::scroll_frame(
                                &scroll,
                                v().id("notifications-list").w_full().max_h(px(height - POPOUT_HEADER_H)).overflow_y_scroll().track_scroll(&scroll).p(px(6.)).gap(px(1.)).children(list),
                            ))
                        }),
                    "notifications-fade",
                    90,
                )),
        )
        .with_priority(2)
        .into_any_element()
    }

    fn notification_row(&self, row: &Row, i: usize, selected: bool, now: i64, cx: &mut Context<Self>) -> AnyElement {
        let age = ui::relative_age(row.at_ms, now);
        let item = row.item.clone();
        let more = row.more.clone().map(|text| {
            let id = row.item.clone();
            div().id(("notification-more", i)).flex_none().px(px(4.)).rounded(px(4.)).text_size(px(11.5)).text_color(c(theme::ACCENT_TEXT)).cursor_pointer()
                .hover(|s| s.bg(c(theme::BORDER)))
                .tooltip(ui::tip("Open the Activity", None))
                .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| {
                    cx.stop_propagation();
                    this.overlay = Overlay::None;
                    if let Item::Note(id) = &id {
                        if let Some(n) = this.all_notifications().into_iter().find(|n| n.id == *id).cloned() {
                            this.open_row_activity(&n, w, cx);
                        }
                    }
                }))
                .child(text)
        });
        // An approval's card is in its conversation; Details shows it here too, with all it holds.
        let details = match row.item {
            Item::Approval(id) => Some(
                div().id(("notification-details", i)).flex_none().text_size(px(11.5)).text_color(c(theme::ACCENT_TEXT)).cursor_pointer()
                    .hover(|s| s.text_color(c(theme::TEXT)))
                    .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| {
                        cx.stop_propagation();
                        this.open_decision(Decision::Approval(id), w, cx);
                    }))
                    .child("Details"),
            ),
            _ => None,
        };
        h().id(("notification", i)).role(Role::Button).aria_label(format!("{}, {}, {age}", row.title, row.meta))
            .gap(px(8.)).items_start().px(px(10.)).py(px(8.)).rounded(px(6.)).cursor_pointer()
            .when(selected, |el| el.bg(c(theme::SURFACE_2)))
            .on_mouse_move(cx.listener(move |this, _, _, cx| {
                if this.bell.selected != i {
                    this.bell.selected = i;
                    cx.notify();
                }
            }))
            .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.open_notification(item.clone(), w, cx)))
            .child(h().w(px(10.)).h(px(18.)).flex_none().justify_center().items_center().child(mark_of(row.mark)))
            .child(
                v().flex_1().min_w_0().gap(px(1.))
                    .child(h().gap(px(6.))
                        .child(div().flex_1().min_w_0().truncate().text_size(px(13.)).when(row.unread, |el| el.font_weight(FontWeight::BOLD)).child(row.title.clone()))
                        .children(more)
                        .child(div().flex_none().text_size(px(11.)).text_color(c(theme::MUTED)).child(age)))
                    .child(h().gap(px(8.))
                        .child(div().flex_1().min_w_0().truncate().text_size(px(11.5)).text_color(c(theme::MUTED)).child(row.meta.clone()))
                        .children(details)),
            )
            .into_any_element()
    }

    /// The toasts, top right under the bell and over everything else. The newest lies in front;
    /// hovering fans the stack out and stops the clock.
    pub fn render_toasts(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.bell.toasts.is_empty() {
            return None;
        }
        let mut stack = ToastStack::new("toasts", self.bell.stack.clone()).w_full();
        let cards: Vec<(String, Item, String, String, bool, ToastTransitionStatus)> =
            self.bell.toasts.iter().map(|(key, card, status)| (key.clone(), card.item.clone(), card.title.clone(), card.meta.clone(), card.waits, status)).collect();
        for (key, item, title, meta, waits, status) in cards {
            let id = SharedString::from(key.clone());
            let close = item.clone();
            let card = Toast::new(SharedString::from(format!("toast-{key}"))).transition_status(status)
                .aria_label(format!("{title}, {meta}"))
                .flex().flex_row().items_start().gap(px(10.)).w_full().px(px(14.)).py(px(11.)).rounded(px(10.)).border_1().cursor_pointer().shadow_lg()
                .border_color(c(if waits { theme::ATTENTION_BORDER } else { theme::BORDER_STRONG }))
                .bg(c(theme::PANEL))
                .occlude()
                .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.open_notification(item.clone(), w, cx)))
                .child(h().w(px(10.)).h(px(18.)).flex_none().justify_center().items_center().child(if waits { ui::diamond(8., theme::ACCENT_TEXT, true).into_any_element() } else { ui::dot(7., theme::ACCENT).into_any_element() }))
                .child(
                    v().flex_1().min_w_0().gap(px(2.))
                        .child(div().text_size(px(13.)).line_height(px(18.)).font_weight(FontWeight::MEDIUM).line_clamp(2).child(title))
                        .when(!meta.is_empty(), |el| el.child(div().truncate().text_size(px(11.5)).text_color(c(theme::MUTED)).child(meta))),
                )
                .child(
                    div().id(SharedString::from(format!("toast-close-{key}"))).flex_none().size(px(18.)).flex().items_center().justify_center().rounded(px(4.))
                        .role(Role::Button).aria_label("Dismiss").text_size(px(12.)).text_color(c(theme::MUTED))
                        .hover(|s| s.bg(c(theme::SURFACE_2)).text_color(c(theme::TEXT)))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            cx.stop_propagation();
                            this.dismiss_toast(&close, cx);
                        }))
                        .child("✕"),
                );
            let shown = if status == ToastTransitionStatus::Ending {
                card.with_animation(SharedString::from(format!("toast-out-{key}")), Animation::new(ToastMotion::sonner().exit_duration), |el, t| el.opacity(1. - t)).into_any_element()
            } else {
                ui::rise(card, SharedString::from(format!("toast-in-{key}")), 0).into_any_element()
            };
            stack = stack.item(id, shown);
        }
        Some(deferred(div().absolute().top(px(TOAST_TOP)).right(px(14.)).w(px(TOAST_W)).child(stack)).with_priority(3).into_any_element())
    }
}

#[cfg(test)]
mod tests {
    use super::{Item, Mark, NEEDS_YOU, Row, badge_count, badge_text, bell_label, child_of, listed_notes, new_notes, note_waits, opening_marks_all_read, popout_frame};
    use crate::screens::updates::tests::row as update;
    use aurelius_domain::ApprovalId;
    use aurelius_protocol::NotificationView;
    use gpui_kit::base::{ToastManager, ToastMotion, ToastOptions};
    use std::collections::HashMap;
    use std::time::Instant;

    /// A notification without a reason, as before the personal Inbox.
    fn note(id: &str, severity: &str, target: &str) -> NotificationView {
        NotificationView { reason: None, severity: severity.into(), target_type: target.into(), ticket_id: None, ticket_number: None, conversation_id: None, ..update(id, None) }
    }

    fn ids<'a>(notes: &[&'a NotificationView]) -> Vec<&'a str> {
        notes.iter().map(|n| n.id.as_str()).collect()
    }

    fn list_row(item: Item, mark: Mark, at_ms: i64) -> Row {
        Row { item, mark, title: String::new(), meta: String::new(), at_ms, unread: false, more: None }
    }

    #[test]
    fn the_list_puts_what_waits_first_and_leaves_out_the_alerts_of_what_has_its_own_row() {
        let notes = [
            note("ok", "info", "task.succeeded"),
            note("approval", "action", "approval.requested"),
            note("failed", "warning", "task.failed"),
            note("escalated", "action", "task.waiting"),
            note("proposal", "action", "employee.proposed"),
            note("guideline", "action", "guideline.proposed"),
            NotificationView { needs_you: true, ..update("review", Some("review")) },
            update("mention", Some("mention")),
        ];
        let refs: Vec<&NotificationView> = notes.iter().collect();
        assert_eq!(ids(&listed_notes(&refs, |_| true)), ["escalated", "review", "ok", "failed", "mention"]);
    }

    #[test]
    fn a_waiting_task_of_someone_elses_ticket_stays_listed_but_does_not_wait_for_me() {
        let waiting = note("waiting", "action", "task.waiting");
        assert!(note_waits(&waiting, true));
        assert!(!note_waits(&waiting, false));
        assert_eq!(ids(&listed_notes(&[&waiting], |_| false)), ["waiting"], "it stays findable");
    }

    #[test]
    fn the_badge_counts_what_waits_what_failed_and_unread_updates_once_each() {
        let review = NotificationView { needs_you: true, ..update("review", Some("review")) };
        let mention = update("mention", Some("mention"));
        let failed = NotificationView { severity: "warning".into(), ..update("failed", Some("failed")) };
        let result = note("ok", "info", "task.succeeded");
        let refs = [&mention, &review, &failed, &result];
        let mut opened = HashMap::new();
        assert_eq!(badge_count(1, &refs, |_| true, &opened), 1 + 3, "the approval, ◆ once, two unread; a result gives no number");
        opened.insert("mention".to_string(), mention.created_at_ms);
        assert_eq!(badge_count(1, &refs, |_| true, &opened), 1 + 2, "a row opened here no longer counts");
    }

    #[test]
    fn a_read_row_that_waits_still_counts_until_the_work_is_done() {
        let read = NotificationView { read: true, needs_you: true, ..update("review", Some("review")) };
        assert_eq!(badge_count(0, &[&read], |_| false, &HashMap::new()), 1);
        let done = NotificationView { needs_you: false, ..read };
        assert_eq!(badge_count(0, &[&done], |_| false, &HashMap::new()), 0);
    }

    #[test]
    fn someone_elses_waiting_task_and_failure_give_no_number() {
        let waiting = note("waiting", "action", "task.waiting");
        let failed = note("failed", "warning", "task.failed");
        assert_eq!(badge_count(1, &[&waiting, &failed], |_| false, &HashMap::new()), 1, "only the approval");
        assert_eq!(badge_count(1, &[&waiting, &failed], |_| true, &HashMap::new()), 3);
    }

    #[test]
    fn the_badge_stops_at_what_fits() {
        assert_eq!(badge_text(3), "3");
        assert_eq!(badge_text(99), "99");
        assert_eq!(badge_text(100), "99+");
    }

    #[test]
    fn the_bell_says_how_many_to_a_screen_reader() {
        assert_eq!(bell_label(0), "Notifications");
        assert_eq!(bell_label(2), "Notifications, 2 new");
    }

    #[test]
    fn seeing_the_list_does_not_read_a_row_of_the_personal_inbox() {
        let mention = update("mention", Some("mention"));
        let result = note("ok", "info", "task.succeeded");
        assert!(!opening_marks_all_read(&[&mention, &result], &HashMap::new()));
        let read = NotificationView { read: true, ..mention };
        assert!(opening_marks_all_read(&[&read, &result], &HashMap::new()), "no unread row left: the old rule holds");
        assert!(!opening_marks_all_read(&[&read], &HashMap::new()), "nothing to mark");
    }

    #[test]
    fn the_first_list_after_connecting_gives_no_toasts() {
        let after = [update("a", Some("mention"))];
        assert!(new_notes(None, &after).is_empty());
    }

    #[test]
    fn a_new_unread_notification_gets_a_toast_and_one_seen_before_does_not() {
        let seen = update("seen", Some("mention"));
        let before = [seen.clone()];
        let after = [update("new", Some("mention")), seen];
        assert_eq!(ids(&new_notes(Some(&before), &after)), ["new"]);
    }

    #[test]
    fn a_new_event_on_a_row_gets_a_toast_again() {
        let row = update("a", Some("mention"));
        let before = [row.clone()];
        let after = [NotificationView { created_at_ms: row.created_at_ms + 1, merged_count: 1, ..row }];
        assert_eq!(ids(&new_notes(Some(&before), &after)), ["a"]);
    }

    #[test]
    fn what_is_read_or_has_its_own_row_gets_no_toast_as_a_notification() {
        let read = NotificationView { read: true, ..update("read", Some("mention")) };
        let alert = note("alert", "action", "approval.requested");
        assert!(new_notes(Some(&[]), &[read, alert]).is_empty());
    }

    #[test]
    fn a_row_of_the_list_knows_which_child_it_is_past_the_section_headers() {
        let day = |at: i64| if at < 10 { "TODAY" } else { "EARLIER" };
        let rows = [
            list_row(Item::Note("a".into()), Mark::Waits { solid: false }, 1),
            list_row(Item::Note("b".into()), Mark::Unread, 2),
            list_row(Item::Note("c".into()), Mark::Read, 3),
            list_row(Item::Note("d".into()), Mark::Read, 20),
        ];
        assert_eq!(child_of(&rows, 0, day), 1, "under NEEDS YOU");
        assert_eq!(child_of(&rows, 1, day), 3, "under TODAY");
        assert_eq!(child_of(&rows, 2, day), 4);
        assert_eq!(child_of(&rows, 3, day), 6, "under EARLIER");
    }

    #[test]
    fn what_waits_goes_under_needs_you_whatever_its_day() {
        let waits = list_row(Item::Note("a".into()), Mark::Waits { solid: true }, 1);
        assert_eq!(waits.section(|_| "TODAY"), NEEDS_YOU);
        let plain = list_row(Item::Note("b".into()), Mark::Read, 1);
        assert_eq!(plain.section(|_| "TODAY"), "TODAY");
    }

    #[test]
    fn the_popout_hangs_under_the_bell_and_stays_in_the_window() {
        assert_eq!(popout_frame(1400., 900., Some((1300., 40.))), (900., 44., 520.), "right edges in line, at its full height");
        assert_eq!(popout_frame(1400., 400., Some((1300., 40.))).2, 344., "a low window: it shrinks");
        assert_eq!(popout_frame(300., 900., Some((280., 40.))).0, 8., "a narrow window: not off the left edge");
        assert_eq!(popout_frame(1400., 900., None).0, 992., "before the bell was drawn: the window's right");
    }

    #[test]
    fn a_toast_replaces_the_one_of_the_same_row_instead_of_stacking() {
        let mut toasts: ToastManager<String, u8> = ToastManager::new(ToastMotion::sonner());
        let now = Instant::now();
        let key = Item::Note("a".into()).key();
        toasts.push(key.clone(), 1, ToastOptions::default(), now);
        toasts.push(key, 2, ToastOptions::default(), now);
        assert_eq!(toasts.len(), 1);
    }

    #[test]
    fn each_kind_of_item_has_its_own_toast_key() {
        let id = ApprovalId::new();
        assert_ne!(Item::Approval(id).key(), Item::Note(id.to_string()).key());
    }
}
