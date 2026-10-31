//! The Discussion tab of the ticket view (AUR-MU-02, ui-ontwerp fase 2 §1): the
//! ticket's own conversation with the board members' posts, the employees'
//! answers and Aurelius' system lines; the composer with `@` suggestions; and the
//! decisions of Plan review and Review under it (MU2-05, MU2-06), whose feedback
//! is the composer's text. What it decides lives in `discussion.rs`.

use crate::app::{AppView, Panel};
use crate::backend::{self, BoardCall};
use crate::keymap::{MENTIONS, MentionClose, MentionDown, MentionPick, MentionUp};
use crate::screens::board::{AnotherTicket, BoardDialog, TicketTab};
use crate::screens::discussion::{self as d, Mentionable, Summary};
use crate::theme::{self, c};
use crate::ui::{self, ButtonKind, h, v};
use aurelius_domain::conversation::MessageRole;
use aurelius_domain::employee::LifecycleStatus;
use aurelius_domain::ticket::{self, ColumnKind, PlanApproval, TicketView};
use aurelius_domain::views::SystemLineKind;
use aurelius_domain::{Command, ConversationId, MessageId, ProjectId, TicketId};
use aurelius_protocol::MessageView;
use gpui_kit::component::input::{Textarea, TextareaState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

/// A button that cannot be pressed is dimmed and has no click handler.
fn dim(el: Stateful<Div>, enabled: bool) -> Stateful<Div> {
    if enabled { el } else { el.opacity(0.4).cursor_default() }
}

impl AppView {
    // ---- Behaviour ----------------------------------------------------------

    /// The open ticket's discussion, when the server has one for it.
    pub fn discussion_of(&self, t: &TicketView) -> Option<ConversationId> {
        t.conversation_id
    }

    /// Follow the discussion of the ticket in the panel (read again on each change of
    /// the ticket; the backend reads it again on each chat event too).
    pub fn discussion_follow(&mut self, conversation: Option<ConversationId>) {
        match conversation {
            None => {
                if self.board.discussion.take().is_some() {
                    let _ = self.requests.send(backend::Request::Board(BoardCall::Discussion(None)));
                }
            }
            Some(c) => {
                if self.board.discussion.as_ref().map(|(x, _)| *x) != Some(c) {
                    self.board.discussion = Some((c, None));
                }
                let _ = self.requests.send(backend::Request::Board(BoardCall::Discussion(Some(c))));
            }
        }
    }

    pub fn discussion_loaded(&mut self, conversation: ConversationId, result: Result<Vec<MessageView>, String>) {
        let shown = match self.panel {
            Panel::Ticket(id) => self.board.ticket(id).and_then(|t| t.conversation_id) == Some(conversation),
            _ => false,
        };
        if !shown {
            // The panel moved on: stop reading it on every chat event.
            if self.board.discussion.as_ref().is_some_and(|(c, _)| *c == conversation) {
                self.board.discussion = None;
            }
            let _ = self.requests.send(backend::Request::Board(BoardCall::Discussion(None)));
            return;
        }
        let Panel::Ticket(id) = self.panel else { return };
        if let Ok(messages) = &result {
            // First seen now: only what comes later counts as new.
            let total = d::counted(messages);
            let seen = self.board.discussion_seen.entry(id).or_insert(total);
            if self.board.tab == TicketTab::Discussion {
                *seen = total;
            }
            // New messages and the Limit line come into view while it stands at the bottom (MU2-01 AC2).
            let before = match &self.board.discussion {
                Some((c, Some(Ok(m)))) if *c == conversation => Some(m.len()),
                _ => None,
            };
            let mine = std::mem::take(&mut self.board.discussion_mine);
            if d::follows(before.is_none(), mine, before.is_some_and(|n| n != messages.len()), self.discussion_at_bottom()) {
                self.board.discussion_scroll.scroll_to_bottom();
            }
        }
        self.board.discussion = Some((conversation, Some(result)));
    }

    /// The discussion stands at (or near) its bottom.
    pub fn discussion_at_bottom(&self) -> bool {
        let scroll = &self.board.discussion_scroll;
        -scroll.offset().y >= scroll.max_offset().y - px(120.)
    }

    /// The discussion's messages, when they loaded.
    pub fn discussion_messages(&self) -> Option<&[MessageView]> {
        match &self.board.discussion {
            Some((_, Some(Ok(m)))) => Some(m.as_slice()),
            _ => None,
        }
    }

    pub fn discussion_mark_seen(&mut self, id: TicketId) {
        if let Some(total) = self.discussion_messages().map(d::counted) {
            self.board.discussion_seen.insert(id, total);
        }
    }

    /// The tab's count of new messages (ui-ontwerp §1: "Discussion 3").
    pub fn discussion_unread(&self, id: TicketId) -> usize {
        let total = self.discussion_messages().map_or(0, d::counted);
        d::unread(total, self.board.discussion_seen.get(&id).copied(), self.board.tab == TicketTab::Discussion)
    }

    /// Whom an `@` can name: the active employees in org chart order, then the board members.
    pub fn mentionables(&self) -> Vec<Mentionable> {
        let employees = crate::screens::dashboard::assignee_order(&self.managers).into_iter().map(|m| Mentionable { name: m.name.clone(), kind: m.role.clone(), employee: true });
        let members = self.members.list.iter().map(|u| Mentionable { name: u.username.clone(), kind: "board member".into(), employee: false });
        employees.chain(members).collect()
    }

    /// The names of archived employees (a mention of them wakes nobody).
    pub fn archived_names(&self) -> Vec<&str> {
        self.managers.iter().filter(|m| m.status == LifecycleStatus::Archived).map(|m| m.name.as_str()).collect()
    }

    /// Who works in this window (its posts carry this name, A-01).
    pub fn me_name(&self) -> Option<String> {
        self.members.who.as_ref().and_then(|w| w.user.as_ref()).map(|u| u.username.clone())
    }

    /// Post the composer's text in the open ticket's discussion (`ticket.post`).
    pub fn discussion_post(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Panel::Ticket(id) = self.panel else { return };
        let Some(t) = self.board.ticket(id).cloned() else { return };
        if self.board.posting || !self.board_online() || !d::writable(&t) || t.conversation_id.is_none() {
            return;
        }
        let text = self.board.composer.read(cx).value().trim().to_string();
        if text.is_empty() {
            return;
        }
        self.board.posting = true;
        self.board.post_error = None;
        let command = Command::PostTicketMessage(ticket::PostTicketMessage { ticket_id: id, text, images: vec![], files: vec![] });
        let _ = self.requests.send(backend::Request::Board(BoardCall::Write { command, expected_version: None, done: None }));
        cx.notify();
    }

    pub fn discussion_posted(&mut self, ticket: TicketId, to: Vec<aurelius_domain::EmployeeId>, cx: &mut Context<Self>) {
        self.board.posting = false;
        self.board.post_error = None;
        if self.board.composer_for == Some(ticket) {
            self.board.composer_clear = true;
        }
        // A run of the owner in the main task is no "woken" (B3): see `reached`.
        let (owner, woken) = d::reached(self.board.ticket(ticket), &to);
        let names: Vec<String> = woken.iter().filter_map(|e| self.employee_name(*e)).collect();
        let text = d::posted_text(&names, owner);
        self.board.discussion_mine = true;
        self.board_announce(text.clone(), cx);
        self.notify_user(text, false, cx);
        if let Some((c, _)) = self.board.discussion {
            let _ = self.requests.send(backend::Request::Board(BoardCall::Discussion(Some(c))));
        }
    }

    /// Complete the `@word` being typed to a whole name.
    pub fn discussion_complete(&mut self, at: usize, name: String, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.board.composer.read(cx).value().to_string();
        if at > text.len() {
            return;
        }
        let done = d::complete(&text, at, &name);
        self.board.composer.update(cx, |i, cx| {
            i.set_value(done, window, cx);
            i.focus(window, cx);
        });
        cx.notify();
    }

    /// The `@` suggestions that show under the composer: where the `@` is, and whom.
    pub fn discussion_suggestions(&self, cx: &App) -> Vec<(usize, Mentionable)> {
        let text = self.board.composer.read(cx).value().to_string();
        if !self.board.mentions.shows(&text) {
            return Vec::new();
        }
        let people = self.mentionables();
        d::mention_suggestions(&text, &people).map(|(at, found)| found.into_iter().map(|p| (at, p.clone())).collect()).unwrap_or_default()
    }

    /// ↑ / ↓ in the suggestions.
    pub fn discussion_step_mention(&mut self, down: bool, cx: &mut Context<Self>) {
        let of = self.discussion_suggestions(cx).len();
        self.board.mentions.step(down, of);
        cx.notify();
    }

    /// Enter / Tab: the active suggestion.
    pub fn discussion_pick_mention(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let suggestions = self.discussion_suggestions(cx);
        if let Some((at, p)) = self.board.mentions.active(suggestions.len()).and_then(|i| suggestions.into_iter().nth(i)) {
            self.discussion_complete(at, p.name, window, cx);
        }
    }

    /// Esc: close the suggestions (they open again when the text changes).
    pub fn discussion_close_mentions(&mut self, cx: &mut Context<Self>) {
        let text = self.board.composer.read(cx).value().to_string();
        self.board.mentions.close(&text);
        cx.notify();
    }

    /// Try the failed summary again (MU2-12 AC4).
    pub fn discussion_retry_summary(&mut self, id: TicketId, cx: &mut Context<Self>) {
        if self.board.retrying || !self.board_online() {
            return;
        }
        self.board.retrying = true;
        self.board.retry_error = None;
        let command = Command::RetryTicketSummary(ticket::RetryTicketSummary { ticket_id: id });
        let _ = self.requests.send(backend::Request::Board(BoardCall::Write { command, expected_version: None, done: None }));
        cx.notify();
    }

    /// The decision buttons on the Ticket tab jump to the discussion's composer (P-27):
    /// the feedback is always a message there.
    pub fn ticket_to_composer(&mut self, id: TicketId, window: &mut Window, cx: &mut Context<Self>) {
        self.ticket_tab(TicketTab::Discussion, cx);
        self.board.composer_missing = self.board.composer.read(cx).value().trim().is_empty();
        self.board.composer.update(cx, |i, cx| i.focus(window, cx));
        let _ = id;
        cx.notify();
    }

    /// Where Send back and Pas aan take their feedback from: the dialog's field after a
    /// drag, the discussion's composer when the ticket has a discussion (MU2-05 AC4),
    /// else the feedback field of phase 1 (an older server).
    pub fn feedback_input(&self, id: TicketId) -> (Entity<TextareaState>, bool) {
        let dialog = matches!(self.board.dialog, Some(BoardDialog::SendBack(t) | BoardDialog::Plan(t)) if t == id);
        let discussion = !dialog && self.board.ticket(id).is_some_and(|t| t.conversation_id.is_some());
        if discussion { (self.board.composer.clone(), true) } else { (self.board.feedback.clone(), false) }
    }

    /// Open a task of a system line in the panel ("‹ #42" leads back).
    fn discussion_open_task(&mut self, task: aurelius_domain::TaskId, window: &mut Window, cx: &mut Context<Self>) {
        self.open_ticket_task(task, window, cx);
    }

    // ---- The tab ------------------------------------------------------------

    /// The Discussion tab: the messages (scrolling) and the composer with its decisions.
    pub fn render_ticket_discussion(&mut self, t: &TicketView, root: Option<&aurelius_protocol::TaskView>, can: bool, window: &mut Window, cx: &mut Context<Self>) -> (AnyElement, Option<AnyElement>) {
        let note = |text: String| div().text_size(px(12.5)).text_color(c(theme::TEXT_2)).child(text);
        let Some(conversation) = self.discussion_of(t) else {
            return (v().id("ticket-body").flex_1().min_h_0().px(px(18.)).py(px(14.)).child(note(d::NO_DISCUSSION.into())).into_any_element(), None);
        };
        if self.board.composer_for != Some(t.id) {
            self.board.composer_for = Some(t.id);
            self.board.composer_clear = true;
            self.board.post_error = None;
            self.board.retry_error = None;
        }
        if std::mem::take(&mut self.board.composer_clear) {
            self.board.composer.update(cx, |i, cx| i.set_value("", window, cx));
        }
        let state = match &self.board.discussion {
            Some((c, Some(Ok(m)))) if *c == conversation => d::Discussion::Ready(m.clone()),
            Some((c, Some(Err(e)))) if *c == conversation => d::Discussion::Failed(e.clone()),
            _ => d::Discussion::Loading,
        };
        let rows: AnyElement = match &state {
            d::Discussion::Loading => note("Loading the discussion…".into()).into_any_element(),
            d::Discussion::Failed(e) => v().gap(px(8.))
                .child(note(e.clone()))
                .child(h().child(ui::button("discussion-retry", "Try again", ButtonKind::Outline, None).h(px(28.)).on_click(cx.listener(move |this, _: &ClickEvent, _, _| {
                    this.discussion_follow(Some(conversation));
                }))))
                .into_any_element(),
            d::Discussion::Ready(messages) if messages.is_empty() => note(d::EMPTY.into()).into_any_element(),
            d::Discussion::Ready(messages) => {
                let retry = d::retry_line(messages);
                let me = self.me_name();
                let rows: Vec<AnyElement> = messages.iter().enumerate().map(|(i, m)| self.render_discussion_message(t, root, m, i, retry == Some(m.id), me.as_deref(), can, cx)).collect();
                v().gap(px(12.)).children(rows).into_any_element()
            }
        };
        // "Forge is answering…" while a run streams into the discussion.
        let messages = match &state {
            d::Discussion::Ready(m) => m.as_slice(),
            _ => &[],
        };
        let live = self.streams.get(&conversation).map(|s| (s.message_id, self.run_author(s.run_id), s.text.clone()));
        let answering = live.as_ref().and_then(|(id, who, _)| d::answering(messages, Some((id, who.clone()))));
        let streamed = live.and_then(|(_, _, text)| answering.as_ref().map(|_| text)).filter(|t| !t.trim().is_empty());
        let body = v().id("ticket-body").flex_1().min_h_0().overflow_y_scroll().track_scroll(&self.board.discussion_scroll).px(px(18.)).py(px(14.)).gap(px(12.))
            .child(rows)
            .children(answering.map(|a| {
                v().gap(px(4.))
                    .child(h().gap(px(7.)).text_size(px(12.)).text_color(c(theme::TEXT_2)).child(ui::blink_dot(6., theme::ACCENT)).child(a))
                    .children(streamed.map(|text| div().text_size(px(13.)).text_color(c(theme::TEXT_2)).child(text)))
            }));
        let footer = self.render_discussion_footer(t, can, cx);
        (ui::scroll_frame(&self.board.discussion_scroll, body).flex_1().into_any_element(), Some(footer))
    }

    /// One discussion message: a system line, a board member's post or an employee's answer.
    #[allow(clippy::too_many_arguments)]
    fn render_discussion_message(&self, t: &TicketView, root: Option<&aurelius_protocol::TaskView>, m: &MessageView, i: usize, retry: bool, me: Option<&str>, can: bool, cx: &mut Context<Self>) -> AnyElement {
        let id = t.id;
        let when = ui::fmt_when(m.created_at_ms);
        if let Some(line) = &m.system {
            let text = d::system_text(line, &m.text);
            let task = d::system_task(line);
            let reported = line.kind == SystemLineKind::Reported;
            let open = self.board.open_reports.contains(&m.id);
            let failed = line.kind == SystemLineKind::SummaryFailed;
            let first = text.lines().next().unwrap_or("").to_string();
            let shown = if reported && !open { first.clone() } else { text.clone() };
            let message = m.id;
            // The review card in the stream (ui-ontwerp §1): the owner's report, folded.
            if line.kind == SystemLineKind::Review {
                let report = root.and_then(|r| r.result.clone()).filter(|r| !r.trim().is_empty());
                return v().id(("discussion-review", i)).role(Role::Group).aria_label(d::system_label(&text, &when)).gap(px(6.)).p(px(10.)).rounded(px(8.)).border_1().border_color(c(theme::ATTENTION_BORDER)).bg(c(theme::ATTENTION_BG))
                    .child(h().gap(px(6.)).child(ui::diamond(7., theme::ACCENT_TEXT, true)).child(div().flex_1().text_size(px(12.5)).font_weight(FontWeight::MEDIUM).child(first.clone())).children(report.as_ref().map(|_| {
                        div().id(("discussion-review-toggle", i)).role(Role::Button).cursor_pointer().text_size(px(12.)).text_color(c(theme::ACCENT_TEXT)).child(if open { "Collapse" } else { "Expand" }).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            if !this.board.open_reports.remove(&message) {
                                this.board.open_reports.insert(message);
                            }
                            cx.notify();
                        }))
                    })))
                    .when_some(report.filter(|_| open), |el, text| el.child(gpui_kit::component::text::TextView::markdown(SharedString::from(format!("discussion-report-{}", m.id)), text).selectable(true).on_link_click(self.markdown_link_handler(false, cx))))
                    .into_any_element();
            }
            let color = if failed { theme::NEGATIVE } else { theme::TEXT_2 };
            // A long line (a failed summary's reason) wraps instead of being cut off.
            return v().id(("discussion-line", i)).w_full().items_center().gap(px(4.)).role(Role::Note).aria_label(d::system_label(&text, &when))
                .child(div().w_full().min_w_0().text_center().text_size(px(11.5)).text_color(c(color)).child(format!("┄ {shown} · {when} ┄")))
                .child(
                    h().gap(px(8.)).justify_center().flex_wrap().text_size(px(11.5)).text_color(c(color))
                        .when(reported && text.lines().count() > 1, |el| {
                            el.child(div().id(("discussion-report", i)).role(Role::Button).cursor_pointer().text_color(c(theme::ACCENT_TEXT)).child(if open { "Hide report" } else { "Open report" }).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                if !this.board.open_reports.remove(&message) {
                                    this.board.open_reports.insert(message);
                                }
                                cx.notify();
                            })))
                        })
                        .children(task.map(|task| {
                            div().id(("discussion-task", i)).role(Role::Link).cursor_pointer().text_color(c(theme::ACCENT_TEXT)).child("Open task").on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.discussion_open_task(task, w, cx)))
                        })),
                )
                // MU2-12 AC4: the failure stays visible, and a board member can try again.
                .when(failed && retry, |el| {
                    let busy = self.board.retrying;
                    let enabled = can && !busy;
                    el.child(dim(ui::button(("discussion-retry-summary", i), if busy { "Trying again…" } else { "Try again" }, ButtonKind::Outline, None).h(px(26.)), enabled).when(enabled, |b| b.on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.discussion_retry_summary(id, cx)))))
                        .children(self.board.retry_error.clone().map(|e| div().text_size(px(12.)).text_color(c(theme::NEGATIVE)).child(e)))
                })
                .into_any_element();
        }
        let who = d::author(m, |name| self.employee_role(name).map(str::to_owned));
        let marked = d::names_me(&m.text, me);
        let plan = m.update.as_deref() == Some("plan");
        let text = if plan && !self.board.open_reports.contains(&m.id) { None } else { Some(m.text.clone()) };
        let message = m.id;
        let label = format!("{}, {}, {when}{}", who.name, who.kind, if marked { ", mentions you" } else { "" });
        v().id(("discussion-message", i)).role(Role::Article).aria_label(label).gap(px(4.)).pl(px(if marked { 8. } else { 0. }))
            .when(marked, |el| el.border_l_2().border_color(c(theme::ACCENT)))
            .child(
                h().gap(px(8.))
                    .child(ui::monogram(&who.name, 20., cx))
                    .child(div().text_size(px(12.5)).font_weight(FontWeight::MEDIUM).child(who.name.clone()))
                    .child(div().text_size(px(11.5)).text_color(c(theme::TEXT_2)).child(if who.human { "· board member".to_string() } else { format!("— {}", who.kind) }))
                    .when(marked, |el| el.child(div().text_size(px(11.)).text_color(c(theme::ACCENT_TEXT)).child("@ you")))
                    .child(div().flex_1())
                    .child(div().text_size(px(11.)).text_color(c(theme::TEXT_2)).child(when.clone())),
            )
            // A plan in the stream (MU2-05 AC1): folded to its title, the decisions are under the composer.
            .when(plan, |el| {
                let title = m.text.lines().map(|l| l.trim().trim_start_matches('#').trim()).find(|l| !l.is_empty()).unwrap_or("Plan").to_string();
                let open = text.is_some();
                el.child(
                    h().gap(px(8.)).p(px(8.)).rounded(px(6.)).border_1().border_color(c(theme::ATTENTION_BORDER)).bg(c(theme::ATTENTION_BG))
                        .child(div().flex_1().min_w_0().truncate().text_size(px(12.5)).child(format!("◆ Plan · {title}")))
                        .child(div().id(("discussion-plan", i)).role(Role::Button).cursor_pointer().text_size(px(12.)).text_color(c(theme::ACCENT_TEXT)).child(if open { "Hide plan" } else { "Review plan" }).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            if !this.board.open_reports.remove(&message) {
                                this.board.open_reports.insert(message);
                            }
                            cx.notify();
                        }))),
                )
            })
            .when_some(text.filter(|t| !t.trim().is_empty()), |el, text| {
                let view = gpui_kit::component::text::TextView::markdown(SharedString::from(format!("discussion-md-{}-{}", m.id, text.len())), crate::screens::markdown::linked(&text)).selectable(true).on_link_click(self.markdown_link_handler(false, cx));
                el.child(div().pl(px(28.)).text_size(px(13.)).line_height(px(20.)).when(m.role == MessageRole::User, |el| el.text_color(c(theme::TEXT))).child(view))
            })
            .into_any_element()
    }

    /// Under the messages: the composer with its suggestions and warnings, and the
    /// decisions of the column (only a board member decides; Done is read-only).
    fn render_discussion_footer(&mut self, t: &TicketView, can: bool, cx: &mut Context<Self>) -> AnyElement {
        let id = t.id;
        let frame = v().flex_none().gap(px(8.)).px(px(18.)).py(px(10.)).border_t_1().border_color(c(theme::BORDER)).bg(c(theme::PANEL));
        if !d::writable(t) {
            // B7: read-only on Done and when archived; follow-up work is a new ticket.
            let summary = self.discussion_messages().map(d::summary_state).unwrap_or(Summary::Pending);
            let text = d::read_only_note(t, &summary);
            return frame.child(div().text_size(px(12.)).text_color(c(theme::TEXT_2)).child(text)).into_any_element();
        }
        let text = self.board.composer.read(cx).value().to_string();
        let people = self.mentionables();
        let suggestions = self.discussion_suggestions(cx);
        let warning = d::unknown_warning(&d::unknown_mentions(&text, &people, &self.archived_names()));
        let ready = !text.trim().is_empty();
        let enabled = can && !self.board.posting;
        let of = suggestions.len();
        let active = self.board.mentions.active(of);
        let list = (!suggestions.is_empty()).then(|| {
            v().id("discussion-mentions").role(Role::ListBox).aria_label("Mention someone").gap(px(2.)).p(px(4.)).rounded(px(6.)).border_1().border_color(c(theme::BORDER)).bg(c(theme::SURFACE))
                .children(suggestions.into_iter().enumerate().map(|(i, (at, p))| {
                    let name = p.name.clone();
                    let on = active == Some(i);
                    h().id(("discussion-mention", i)).role(Role::ListBoxOption).aria_label(d::suggestion_label(&p, i, of)).aria_selected(on).gap(px(8.)).px(px(6.)).py(px(3.)).rounded(px(4.)).cursor_pointer()
                        .when(on, |el| el.bg(c(theme::SURFACE_2)))
                        .hover(|s| s.bg(c(theme::SURFACE_2)))
                        .child(ui::monogram(&p.name, 16., cx))
                        .child(div().text_size(px(12.5)).child(p.name.clone()))
                        .child(div().text_size(px(11.5)).text_color(c(theme::TEXT_2)).child(p.kind.clone()))
                        .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.discussion_complete(at, name.clone(), w, cx)))
                }))
        });
        let send = dim(ui::button("discussion-send", if self.board.posting { "Sending…" } else { "Send ⏎" }, ButtonKind::Primary, None).h(px(28.)), enabled && ready).when(enabled && ready, |el| el.on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.discussion_post(w, cx))));
        let needs_text = |el: Stateful<Div>| if ready { el } else { el.opacity(0.5).tooltip(ui::tip(d::WRITE_FIRST, None)) };
        let decisions = match t.column {
            // MU2-06: Send back with the composer's text as message and feedback; Done asks first (§9.3).
            ColumnKind::Review => Some(
                h().gap(px(8.))
                    .child(dim(needs_text(ui::button("discussion-send-back", "Send back to Doing", ButtonKind::Outline, None).h(px(28.))), can).when(can, |el| el.on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.ticket_send_back(id, w, cx)))))
                    .child(div().flex_1())
                    .child(dim(ui::button("discussion-done", "✓ Done", ButtonKind::Primary, None).h(px(28.)), can).when(can, |el| el.on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.board_dialog(BoardDialog::MarkDone(id), w, cx))))),
            ),
            // MU2-05: the plan's four buttons; Pas aan takes the composer's text.
            ColumnKind::PlanReview => {
                let approve = |key: &'static str, text: &'static str, kind: ButtonKind, decision: PlanApproval, cx: &mut Context<Self>| {
                    dim(ui::button(key, text, kind, None).h(px(28.)), can).when(can, |el| el.on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.ticket_plan(id, Some(decision), w, cx))))
                };
                Some(
                    h().gap(px(8.)).flex_wrap()
                        .child(approve("discussion-plan-approve", "Goedkeuren + Auto", ButtonKind::Primary, PlanApproval::Approve, cx))
                        .child(approve("discussion-plan-approve-edits", "Goedkeuren", ButtonKind::Outline, PlanApproval::ApproveEdits, cx))
                        .child(dim(needs_text(ui::button("discussion-plan-revise", "Pas aan", ButtonKind::Outline, None).h(px(28.))), can).when(can, |el| el.on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.ticket_plan(id, None, w, cx)))))
                        .child(div().flex_1())
                        .child(dim(ui::button("discussion-plan-reject", "Afwijzen", ButtonKind::Outline, None).h(px(28.)), can).when(can, |el| el.on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.board_dialog(BoardDialog::Reject(id), w, cx))))),
                )
            }
            _ => None,
        };
        frame
            .child(div().text_size(px(11.5)).text_color(c(theme::TEXT_2)).child(d::composer_hint(t)))
            .children(list)
            // While suggestions show, ↑↓ / Enter / Tab / Esc are theirs (ui-ontwerp §1).
            .child(
                div().when(of > 0, |el| {
                    el.key_context(MENTIONS)
                        .on_action(cx.listener(|this, _: &MentionUp, _, cx| this.discussion_step_mention(false, cx)))
                        .on_action(cx.listener(|this, _: &MentionDown, _, cx| this.discussion_step_mention(true, cx)))
                        .on_action(cx.listener(|this, _: &MentionPick, w, cx| this.discussion_pick_mention(w, cx)))
                        .on_action(cx.listener(|this, _: &MentionClose, _, cx| this.discussion_close_mentions(cx)))
                })
                .child(Textarea::new(&self.board.composer).py(px(7.)).line_height(px(20.))),
            )
            .children(warning.map(|w| div().id("discussion-warning").role(Role::Status).text_size(px(12.)).text_color(c(crate::screens::board::HIGH)).child(w)))
            .when(self.board.composer_missing, |el| el.child(div().text_size(px(12.)).text_color(c(theme::NEGATIVE)).child(d::WRITE_FIRST)))
            .children(self.board.post_error.clone().map(|e| div().id("discussion-error").role(Role::Alert).text_size(px(12.)).text_color(c(theme::NEGATIVE)).child(e)))
            .when(!can, |el| el.child(div().text_size(px(12.)).text_color(c(theme::TEXT_2)).child("Offline · the draft stays until you are back.")))
            .child(h().gap(px(8.)).child(div().flex_1()).child(send))
            .children(decisions)
            .into_any_element()
    }

    /// The tab's label with its count of new messages.
    pub fn discussion_tab_label(&self, id: TicketId) -> String {
        match self.discussion_unread(id) {
            0 => "Discussion".into(),
            n => format!("Discussion {n}"),
        }
    }

    /// Whether the Done bar can say how the summary went (MU2-12 AC2/AC4).
    pub fn ticket_summary(&self, t: &TicketView) -> Option<Summary> {
        (t.column == ColumnKind::Done && t.conversation_id.is_some()).then(|| self.discussion_messages().map(d::summary_state)).flatten()
    }

    /// The message a "Try again" in the Done bar retries: same as in the stream.
    pub fn ticket_summary_failed(&self, t: &TicketView) -> bool {
        matches!(self.ticket_summary(t), Some(Summary::Failed(_))) && self.discussion_messages().is_some_and(|m| d::retry_line(m).is_some())
    }

    // ---- A ticket from a chat message (MU2-10, fase 2 §4) ----------------------------

    /// The project of the conversation a message is in: the channel's, the project
    /// assistant's, or the task's (none in #general and an employee's chat).
    pub fn message_project(&self) -> Option<ProjectId> {
        if let Some(task) = self.panel_task() {
            return Some(task.project_id);
        }
        match self.screen {
            crate::app::Page::GroupChat => self.channel_project(),
            crate::app::Page::Project(crate::app::ProjectRef::Real(p), _) => Some(p),
            _ => self.open_conversation.and_then(|c| self.tasks.iter().find(|t| t.conversation_id == Some(c))).map(|t| t.project_id),
        }
    }

    /// Where a message was said, for the ticket's description ("#general", "#demo", "a conversation").
    pub fn message_place(&self) -> String {
        match self.screen {
            crate::app::Page::GroupChat => self.current_channel().map_or_else(|| "the Team chat".to_string(), |c| d::channel_title(c).replace("# ", "#")),
            _ => self.panel_task().map_or_else(|| "a conversation".to_string(), |t| format!("the task “{}”", t.title)),
        }
    }

    /// "Create ticket" on a message of the open conversation: the New ticket form
    /// filled in from it, or first "Create another?" when it made a ticket already.
    pub fn ticket_from_message(&mut self, message: MessageId, window: &mut Window, cx: &mut Context<Self>) {
        let all = self.messages.as_ref().map(|s| s.messages.clone()).unwrap_or_default();
        let Some(m) = all.iter().find(|m| m.id == message) else { return };
        if !self.make_ticket_action_allowed(m) {
            return;
        }
        let author = match (&m.author_user, m.role) {
            (Some(u), _) => u.username.clone(),
            (None, MessageRole::User) => self.me_name().unwrap_or_else(|| "a board member".into()),
            (None, MessageRole::Assistant) => m.author.clone().unwrap_or_else(|| "Aurelius".into()),
        };
        let prefill = d::prefill(&m.text, &author, &self.message_place(), &ui::fmt_when(m.created_at_ms));
        let project = self.message_project();
        if let Some((_, existing)) = d::ticket_from(&all, message) {
            self.board.another = Some(AnotherTicket { message, project, prefill, existing });
            self.board_dialog(BoardDialog::AnotherTicket, window, cx);
            return;
        }
        self.ticket_from_message_form(message, project, prefill, window, cx);
    }

    pub fn ticket_from_message_form(&mut self, message: MessageId, project: Option<ProjectId>, prefill: d::Prefill, window: &mut Window, cx: &mut Context<Self>) {
        self.board_form_clear();
        self.board.form.project = project;
        self.board.form.from_message = Some(message);
        self.board.form.prefill = Some(prefill);
        self.board_dialog(BoardDialog::NewTicket, window, cx);
    }

    /// The "→ #52" pill on a message a ticket was made from (fase 2 §4).
    pub fn message_ticket_pill(&self, message: MessageId, cx: &mut Context<Self>) -> Option<AnyElement> {
        let all = self.messages.as_ref().map(|s| &s.messages[..])?;
        let (ticket, number) = d::ticket_from(all, message)?;
        Some(
            div().id(SharedString::from(format!("msg-ticket-{message}"))).role(Role::Link).aria_label(format!("Ticket {number}, made from this message")).px(px(7.)).py(px(1.)).rounded(px(8.)).border_1().border_color(c(theme::BORDER_STRONG)).cursor_pointer().text_size(px(11.5)).text_color(c(theme::ACCENT_TEXT))
                .child(format!("→ {number}"))
                .when_some(ticket, |el, id| el.on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.open_ticket(id, w, cx))))
                .into_any_element(),
        )
    }

    /// Whether a message offers "Create ticket": a finished message with text, a
    /// board member works in this window (B6: only a board member makes a ticket),
    /// and the server takes `from_message` (feature `ticket_discussion`).
    pub fn make_ticket_action_allowed(&self, m: &MessageView) -> bool {
        d::offers_ticket(m, self.members.who.as_ref().and_then(|w| w.user.as_ref()).is_some(), self.ticket_discussion_supported())
    }

    /// The "⌁ Ticket" action on a message (hover, fase 2 §4).
    pub fn make_ticket_action(&self, m: &MessageView, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.make_ticket_action_allowed(m) {
            return None;
        }
        let message = m.id;
        Some(
            ui::action_icon(SharedString::from(format!("make-ticket-{message}")), "⌁", "Create ticket…", None, None)
                .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| {
                    cx.stop_propagation();
                    this.ticket_from_message(message, w, cx);
                }))
                .into_any_element(),
        )
    }

    /// The project's channel in the Team chat (fase 2 §3: "Open #aurelius" on the project).
    pub fn open_project_channel(&mut self, project: ProjectId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(channel) = self.channels.as_ref().and_then(|l| l.iter().find(|c| c.project_id == Some(project))).map(|c| c.conversation_id) else { return };
        self.go_nav(crate::app::NavItem::Chat, window, cx);
        self.open_channel(channel, window, cx);
    }

    /// The link to it, when the server has project channels.
    pub fn project_channel_link(&self, project: ProjectId, cx: &mut Context<Self>) -> Option<AnyElement> {
        let channel = self.channels.as_ref()?.iter().find(|c| c.project_id == Some(project))?;
        let title = d::channel_title(channel).replace("# ", "#");
        Some(
            h().pt(px(18.)).child(
                div().id("project-channel").role(Role::Link).aria_label(format!("Open {title} in the Team chat")).cursor_pointer().text_size(px(13.)).text_color(c(theme::ACCENT_TEXT))
                    .child(format!("Open {title}"))
                    .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.open_project_channel(project, w, cx))),
            )
            .into_any_element(),
        )
    }
}
