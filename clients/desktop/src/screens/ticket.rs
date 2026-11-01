//! The ticket view in the right panel (ui-ontwerp §7), its review gate and
//! plan review (§9), the New ticket dialog (§10.1), and Settings → Board for
//! the columns and the company's dispatcher (§12.1).
//!
//! Every change is saved at once (no global Save). Plan review and Review
//! decide with a button and a text field (B2); the text goes to the main
//! task's conversation. Done closes the ticket: nothing is merged, pushed or
//! deployed (MU1-17).

use crate::app::{AppView, MenuItems, Overlay, Panel};
use crate::backend::{self, BoardCall};
use crate::screens::board::{BoardDialog, BoardDo, Dispatcher, DoneCheck, EditField, done_check, Needs, PRIORITIES, TicketTab, activity_detail, activity_text, deadline_input, feedback_label, mode_text, deadline_text, empty_update, feedback_ready, menu_entry, move_column, new_ticket_problems, parse_deadline, parse_labels, priority_label};
use crate::theme::{self, c};
use crate::ui::{self, ButtonKind, h, v};
use aurelius_domain::task::TaskStatus;
use aurelius_domain::ticket::{self, ColumnKind, PlanApproval, TicketView};
use aurelius_domain::{Command, TicketId};
use gpui_kit::component::input::{InputState, Textarea};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

/// Mode and model change only while the main task does not run (MU1-07 AC5, as the server checks).
pub fn run_locked(root: Option<TaskStatus>) -> bool {
    root == Some(TaskStatus::Running)
}

/// The fields of a ticket in Done are read-only (MU1-16 AC1).
pub fn editable(column: ColumnKind) -> bool {
    column != ColumnKind::Done
}

/// The owner can be chosen in Backlog, and in Todo while the main task is a draft (`check_assign`).
pub fn owner_choosable(t: &TicketView) -> bool {
    ticket::check_assign(t.column, t.root_task_status).is_ok()
}

/// The text of a run mode; none chosen: the owner's own.
fn mode_label(mode: Option<aurelius_domain::autonomy::AgentMode>) -> String {
    mode.map_or_else(|| "Default (the owner's)".to_string(), |m| m.label().to_string())
}

fn label(text: &'static str) -> Div {
    div().w(px(96.)).flex_none().text_size(px(12.)).text_color(c(theme::TEXT_2)).child(text)
}

fn row(text: &'static str, value: impl IntoElement) -> Div {
    h().gap(px(8.)).items_start().min_h(px(26.)).child(label(text)).child(div().flex_1().min_w_0().text_size(px(12.5)).child(value))
}

/// A button that cannot be pressed is dimmed and has no click handler.
fn dim(el: Stateful<Div>, enabled: bool) -> Stateful<Div> {
    if enabled { el } else { el.opacity(0.4).cursor_default() }
}

fn pill(text: String) -> Div {
    div().px(px(8.)).py(px(1.)).rounded(px(10.)).border_1().border_color(c(theme::BORDER_STRONG)).text_size(px(11.5)).child(text)
}

impl AppView {
    // ---- Behaviour ----------------------------------------------------------

    /// Approve the plan (Goedkeuren + Auto / Goedkeuren) or ask for a new one (Pas aan, feedback required).
    pub fn ticket_plan(&mut self, id: TicketId, decision: Option<PlanApproval>, window: &mut Window, cx: &mut Context<Self>) {
        let (input, discussion) = self.feedback_input(id);
        let feedback = input.read(cx).value().trim().to_string();
        let number = self.board.ticket(id).map_or(0, |t| t.number);
        let command = match decision {
            Some(decision) => Command::ApprovePlan(ticket::ApprovePlan { ticket_id: id, decision, feedback: Some(feedback.clone()).filter(|f| !f.is_empty()) }),
            None if !feedback_ready(&feedback) => {
                self.feedback_missing(discussion);
                input.update(cx, |f, cx| f.focus(window, cx));
                cx.notify();
                return;
            }
            None => Command::RevisePlan(ticket::RevisePlan { confirm_secret: false, ticket_id: id, feedback }),
        };
        let done = match decision {
            Some(PlanApproval::Approve) => format!("Plan of #{number} approved. The owner goes on in Auto."),
            Some(PlanApproval::ApproveEdits) => format!("Plan of #{number} approved. Edits go through; the owner asks for the rest."),
            None => format!("#{number}: the owner makes a new plan with your feedback."),
        };
        self.board_send(command, Some(done));
        self.board_feedback_sent(discussion, window, cx);
    }

    /// Feedback was needed and the field is empty: say so under the field it goes in.
    fn feedback_missing(&mut self, discussion: bool) {
        if discussion {
            self.board.composer_missing = true;
        } else {
            self.board.feedback_missing = true;
        }
    }

    /// Send back to Doing with the required feedback (B2, MU1-16 AC2/AC3).
    pub fn ticket_send_back(&mut self, id: TicketId, window: &mut Window, cx: &mut Context<Self>) {
        let (input, discussion) = self.feedback_input(id);
        let feedback = input.read(cx).value().trim().to_string();
        if !feedback_ready(&feedback) {
            self.feedback_missing(discussion);
            input.update(cx, |f, cx| f.focus(window, cx));
            cx.notify();
            return;
        }
        let t = self.board.ticket(id).cloned();
        let who = t.as_ref().and_then(|t| t.owner_name.clone()).unwrap_or_else(|| "The owner".into());
        let number = t.map_or(0, |t| t.number);
        self.board_send(Command::SendBackTicket(ticket::SendBackTicket { confirm_secret: false, ticket_id: id, feedback }), Some(format!("#{number} is back in Doing. {who} gets your feedback.")));
        self.board_feedback_sent(discussion, window, cx);
    }

    /// What still works on the ticket before Done (B6): its linked tasks, and a
    /// run in the main task's conversation as the jobs list shows it.
    pub fn ticket_done_check(&self, id: TicketId) -> DoneCheck {
        done_check(self.board.detail_of(id), |conversation| self.running_jobs().any(|j| j.conversation_id == Some(conversation)))
    }

    /// Mark as Done (MU1-16 AC1, after "Mark as Done?").
    pub fn ticket_accept(&mut self, id: TicketId, window: &mut Window, cx: &mut Context<Self>) {
        if !self.ticket_done_check(id).allowed(self.board_online()) {
            return;
        }
        let number = self.board.ticket(id).map_or(0, |t| t.number);
        self.board_send(Command::AcceptTicket(ticket::AcceptTicket { confirm_secret: false, ticket_id: id, note: None }), Some(format!("#{number} is Done. A summary goes to project memory.")));
        self.board_close_dialog(window, cx);
    }

    /// The feedback went: empty the field it came from (the composer: it is a message in the discussion now).
    fn board_feedback_sent(&mut self, discussion: bool, window: &mut Window, cx: &mut Context<Self>) {
        if discussion {
            self.board.composer.update(cx, |f, cx| f.set_value("", window, cx));
            self.board.composer_missing = false;
        } else {
            self.board.feedback.update(cx, |f, cx| f.set_value("", window, cx));
        }
        self.board.feedback_missing = false;
        if matches!(self.board.dialog, Some(BoardDialog::SendBack(_) | BoardDialog::Plan(_))) {
            self.board.dialog = None;
        }
        cx.notify();
    }

    /// Create (and maybe start) the ticket of the New ticket form (MU1-07, D-09).
    pub fn board_create(&mut self, start: bool, _window: &mut Window, cx: &mut Context<Self>) {
        if self.board.dialog != Some(BoardDialog::NewTicket) || self.board.form.busy {
            return;
        }
        let form = &self.board.form;
        let title = form.title.read(cx).value().trim().to_string();
        let (title_problem, project_problem) = new_ticket_problems(&title, form.project);
        let offset = aurelius_host::utc_offset_minutes_at(ui::now_ms());
        let deadline = parse_deadline(&form.deadline.read(cx).value(), offset);
        let description = form.description.read(cx).value().trim().to_string();
        let labels = parse_labels(&form.labels.read(cx).value());
        let (project, priority, owner, run) = (form.project, form.priority, form.owner, form.run.clone());
        let files: Vec<String> = form.files.iter().map(|(p, _)| p.display().to_string()).collect();
        self.board.form.sent = true;
        let (Some(project_id), None, None, Ok(deadline_ms)) = (project, title_problem, project_problem, deadline.clone()) else {
            self.board.form.error = deadline.err();
            cx.notify();
            return;
        };
        let from_message = crate::screens::discussion::from_message(self.board.form.from_message, self.ticket_discussion_supported());
        let create = ticket::CreateTicket { confirm_secret: false, project_id, title, description: Some(description).filter(|d| !d.is_empty()), priority, labels, deadline_ms, owner_employee_id: owner, run, files, start, from_message };
        let done = start.then(|| match owner.and_then(|e| self.employee_name(e)) {
            Some(name) => format!("Created and started with {name}."),
            None => {
                let dispatcher = self.dispatcher_for(project_id);
                match (dispatcher.name(), dispatcher.why_nobody()) {
                    (Some(d), _) => format!("Created and started. {d} chooses an employee."),
                    (None, why) => format!("Created and started. {}: choose an employee.", why.unwrap_or_default()),
                }
            }
        });
        self.board.form.busy = true;
        self.board.form.error = None;
        let _ = self.requests.send(backend::Request::Board(BoardCall::Write { command: Command::CreateTicket(create), expected_version: None, done }));
        cx.notify();
    }

    /// Start editing a field of the open ticket inline.
    pub fn ticket_edit(&mut self, field: EditField, window: &mut Window, cx: &mut Context<Self>) {
        let Panel::Ticket(id) = self.panel else { return };
        let Some(t) = self.board.ticket(id).cloned() else { return };
        let offset = aurelius_host::utc_offset_minutes_at(ui::now_ms());
        match field {
            EditField::Title => self.board.edit_title.update(cx, |i, cx| {
                i.set_value(t.title.clone(), window, cx);
                i.focus(window, cx);
            }),
            EditField::Description => self.board.edit_description.update(cx, |i, cx| {
                i.set_value(t.description.clone().unwrap_or_default(), window, cx);
                i.focus(window, cx);
            }),
            EditField::Labels => self.board.edit_labels.update(cx, |i, cx| {
                i.set_value(t.labels.join(", "), window, cx);
                i.focus(window, cx);
            }),
            EditField::Deadline => self.board.edit_deadline.update(cx, |i, cx| {
                i.set_value(t.deadline_ms.map(|d| deadline_input(d, offset)).unwrap_or_default(), window, cx);
                i.focus(window, cx);
            }),
        }
        self.board.editing = Some(field);
        self.board.edit_error = None;
        cx.notify();
    }

    /// Save the inline edit (Enter or Save); Cancel leaves it.
    pub fn ticket_save_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Panel::Ticket(id), Some(field)) = (self.panel, self.board.editing) else { return };
        let mut update = empty_update(id);
        match field {
            EditField::Title => {
                let title = self.board.edit_title.read(cx).value().trim().to_string();
                if let Err(e) = ticket::valid_title(&title) {
                    self.board.edit_error = Some(e.to_string());
                    cx.notify();
                    return;
                }
                update.title = Some(title);
            }
            EditField::Description => update.description = Some(self.board.edit_description.read(cx).value().trim().to_string()),
            EditField::Labels => update.labels = Some(parse_labels(&self.board.edit_labels.read(cx).value())),
            EditField::Deadline => match parse_deadline(&self.board.edit_deadline.read(cx).value(), aurelius_host::utc_offset_minutes_at(ui::now_ms())) {
                Ok(Some(ms)) => update.deadline_ms = Some(ms),
                Ok(None) => update.clear_deadline = true,
                Err(e) => {
                    self.board.edit_error = Some(e);
                    cx.notify();
                    return;
                }
            },
        }
        self.board.editing = None;
        self.board.edit_error = None;
        self.ticket_update(update, cx);
        self.focus.focus(window, cx);
    }

    pub fn ticket_cancel_edit(&mut self, cx: &mut Context<Self>) {
        self.board.editing = None;
        self.board.edit_error = None;
        cx.notify();
    }

    /// Settings → Board: a copy of the columns with a name field each.
    pub fn board_columns_begin(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut columns: Vec<ticket::BoardColumnView> = self.board.view.as_ref().map(|v| v.columns.iter().map(|c| c.column.clone()).collect()).unwrap_or_default();
        columns.sort_by_key(|c| c.position);
        self.board.columns = columns
            .into_iter()
            .map(|column| {
                let input = cx.new(|cx| InputState::new(window, cx).placeholder(ticket::column_title(column.kind)));
                input.update(cx, |i, cx| i.set_value(column.name.clone(), window, cx));
                (column, input)
            })
            .collect();
        self.board.columns_error = None;
    }

    /// Save the names and the order with the board's version (B3, MU1-06 AC1).
    pub fn board_columns_save(&mut self, cx: &mut Context<Self>) {
        let mut columns: Vec<ticket::BoardColumnView> = self.board.columns.iter().map(|(c, input)| ticket::BoardColumnView { name: input.read(cx).value().trim().to_string(), ..c.clone() }).collect();
        if let Some(blank) = columns.iter().find(|c| c.name.is_empty()) {
            self.board.columns_error = Some(format!("Give {} a name.", ticket::column_title(blank.kind)));
            cx.notify();
            return;
        }
        let mut seen = std::collections::HashSet::new();
        if columns.iter().any(|c| !seen.insert(c.name.to_lowercase())) {
            self.board.columns_error = Some("Each column needs its own name.".into());
            cx.notify();
            return;
        }
        let version = ticket::board_version(&columns);
        columns.sort_by_key(|c| c.position);
        let command = Command::UpdateBoardColumns(crate::screens::board::columns_edit(&columns));
        self.board.columns_error = None;
        let _ = self.requests.send(backend::Request::Board(BoardCall::Write { command, expected_version: Some(version), done: Some("Columns saved. Every client shows them now.".into()) }));
        cx.notify();
    }

    /// Restore defaults (MU1-06 AC5): the fields show the default names and order;
    /// nothing changes until Save columns (that is the confirmation).
    pub fn board_columns_restore(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut columns: Vec<ticket::BoardColumnView> = self.board.columns.iter().map(|(c, _)| c.clone()).collect();
        crate::screens::board::default_columns(&mut columns);
        for ((column, input), restored) in self.board.columns.iter_mut().zip(columns) {
            column.position = restored.position;
            input.update(cx, |i, cx| i.set_value(restored.name.clone(), window, cx));
        }
        self.board.columns.sort_by_key(|(c, _)| c.position);
        self.board.columns_error = None;
        self.notify_user("Default names and order. Save columns to apply them.", false, cx);
        cx.notify();
    }

    pub fn board_columns_move(&mut self, index: usize, left: bool, cx: &mut Context<Self>) {
        let mut columns: Vec<ticket::BoardColumnView> = self.board.columns.iter().map(|(c, _)| c.clone()).collect();
        if move_column(&mut columns, index, left) {
            for (i, column) in columns.into_iter().enumerate() {
                self.board.columns[i].0.position = column.position;
            }
            self.board.columns.sort_by_key(|(c, _)| c.position);
        }
        cx.notify();
    }

    fn owner_menu(&self, t: &TicketView) -> MenuItems {
        let mut items = Vec::new();
        if t.column == ColumnKind::Backlog {
            let name = self.dispatcher_for(t.project_id).choice();
            items.push(menu_entry(name, BoardDo::Owner(t.id, None), t.owner_employee_id.is_none(), None));
            items.push(None);
        }
        items.extend(self.employee_choices().into_iter().map(|(e, name)| menu_entry(name, BoardDo::Owner(t.id, Some(e)), t.owner_employee_id == Some(e), None)));
        items
    }

    fn mode_menu(current: Option<aurelius_domain::autonomy::AgentMode>, make: impl Fn(Option<aurelius_domain::autonomy::AgentMode>) -> BoardDo) -> MenuItems {
        std::iter::once(menu_entry(mode_label(None), make(None), current.is_none(), None))
            .chain(aurelius_domain::autonomy::AgentMode::ALL.into_iter().map(|m| menu_entry(m.label(), make(Some(m)), current == Some(m), None)))
            .collect()
    }

    fn model_menu_for(&self, current: Option<&str>, make: impl Fn(Option<(aurelius_domain::ProviderId, String)>) -> BoardDo) -> MenuItems {
        std::iter::once(menu_entry("Default", make(None), current.is_none(), None))
            .chain(self.task_model_options().into_iter().map(|(c, m, label)| {
                let on = current == Some(m.as_str());
                menu_entry(label, make(Some((c, m))), on, None)
            }))
            .collect()
    }

    fn form_menu(&self, field: &'static str) -> MenuItems {
        let form = &self.board.form;
        match field {
            "ticket-form-project" => self
                .projects
                .iter()
                .filter(|p| p.status == aurelius_domain::project::ProjectStatus::Active)
                .map(|p| menu_entry(p.name.clone(), BoardDo::FormProject(p.id), form.project == Some(p.id), None))
                .collect(),
            "ticket-form-priority" => PRIORITIES.iter().map(|p| menu_entry(priority_label(*p), BoardDo::FormPriority(*p), form.priority == *p, None)).collect(),
            "ticket-form-owner" => {
                let name = form.project.map_or_else(|| Dispatcher::None.choice(), |p| self.dispatcher_for(p).choice());
                let mut items = vec![menu_entry(name, BoardDo::FormOwner(None), form.owner.is_none(), None), None];
                items.extend(self.employee_choices().into_iter().map(|(e, name)| menu_entry(name, BoardDo::FormOwner(Some(e)), form.owner == Some(e), None)));
                items
            }
            "ticket-form-mode" => Self::mode_menu(form.run.mode, BoardDo::FormMode),
            _ => self.model_menu_for(form.run.model.as_deref(), BoardDo::FormModel),
        }
    }

    fn ticket_menu(&self, field: &'static str, t: &TicketView) -> MenuItems {
        let id = t.id;
        match field {
            "ticket-priority" => PRIORITIES.iter().map(|p| menu_entry(priority_label(*p), BoardDo::Priority(id, *p), t.priority == *p, None)).collect(),
            "ticket-mode" => Self::mode_menu(t.run.mode, move |m| BoardDo::Mode(id, m)),
            "ticket-model" => self.model_menu_for(t.run.model.as_deref(), move |m| BoardDo::Model(id, m)),
            _ => self.owner_menu(t),
        }
    }

    /// A dropdown of the ticket view (`ticket`) or of the New ticket form (none).
    fn board_picker(&self, id: &'static str, text: String, set: bool, enabled: bool, ticket: Option<TicketId>, cx: &mut Context<Self>) -> Stateful<Div> {
        let open = self.overlay == Overlay::Menu && self.menu.as_ref().is_some_and(|m| m.field == Some(id));
        let trigger = ui::dropdown_trigger(id, 28., set, open).max_w(px(260.)).child(div().min_w_0().truncate().child(text)).child(ui::caret()).child(ui::track_bounds(self.dropdown_bounds.clone(), id));
        if !enabled {
            return trigger.opacity(0.5).cursor_default();
        }
        trigger.on_click(cx.listener(move |this, _: &ClickEvent, w, cx| {
            let items = match ticket.and_then(|t| this.board.ticket(t).cloned()) {
                Some(t) => this.ticket_menu(id, &t),
                None => this.form_menu(id),
            };
            this.open_dropdown(id, items, w, cx);
        }))
    }

    /// The action bar: only what works in the ticket's column (§7.3).
    fn render_ticket_actions(&mut self, t: &TicketView, root: Option<&aurelius_protocol::TaskView>, needs: Option<Needs>, can: bool, cx: &mut Context<Self>) -> AnyElement {
        let id = t.id;
        let back = |cx: &mut Context<Self>| {
            dim(ui::button("ticket-back", "Back to Backlog", ButtonKind::Outline, None).h(px(28.)), can).when(can, |el| el.on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.board_do(BoardDo::Move(id, ColumnKind::Backlog), w, cx))))
        };
        let line = |text: String, color: u32| div().flex_1().min_w(px(160.)).text_size(px(12.5)).text_color(c(color)).child(text);
        let failed = t.root_task_status == Some(TaskStatus::Failed);
        let bar = h().flex_none().flex_wrap().gap(px(8.)).px(px(18.)).py(px(10.)).border_b_1().border_color(c(theme::BORDER)).bg(c(theme::PANEL));
        let root_id = root.filter(|r| r.conversation_id.is_some()).map(|r| r.id);
        let open_conversation = |cx: &mut Context<Self>| {
            ui::button("ticket-conversation", "Open conversation", ButtonKind::Outline, None).h(px(28.)).on_click(cx.listener(move |this, _: &ClickEvent, w, cx| {
                if let Some(r) = root_id {
                    this.open_ticket_task(r, w, cx);
                }
            }))
        };
        let dispatcher = self.dispatcher_for(t.project_id);
        let me = self.active_member().map(|u| u.id);
        // ui-ontwerp §5: the creator's login or quota; only the own login has a direct action.
        let wait = self.ticket_wait(t).filter(|_| matches!(t.column, ColumnKind::Todo | ColumnKind::Doing) && !failed);
        let el = match t.column {
            _ if wait.is_some() => {
                let own = needs == Some(Needs::ConnectLogin);
                let expired = self.claude.view.as_ref().is_some_and(|v| v.status == aurelius_domain::user::ClaudeLoginStatus::Expired);
                let wait = wait.unwrap_or_default();
                let quota = wait.starts_with('⏸');
                let text = match (own, quota) {
                    (true, _) if expired => format!("{wait} (expired)"),
                    (_, true) => format!("{wait}. Runs only on {} subscription.", crate::screens::board::login_owner(t, me)),
                    (false, false) => format!("{wait}. It runs only on {} subscription, unless you run it on yours.", crate::screens::board::login_owner(t, me)),
                    (true, false) => wait,
                };
                bar.child(line(text, if own { theme::ACCENT_TEXT } else { theme::TEXT_2 }))
                    .when(own, |el| {
                        el.child(ui::button("ticket-connect", if expired { "Reconnect Claude login" } else { "Connect Claude login" }, ButtonKind::Primary, None).h(px(28.)).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.claude_connect(cx))))
                    })
                    .when(crate::screens::board::take_over_offered(t, me), |el| {
                        el.child(dim(ui::button("ticket-use-my-login", "Run on my login", ButtonKind::Outline, None).h(px(28.)), can).when(can, |el| {
                            el.on_click(cx.listener(move |this, _: &ClickEvent, w, cx| {
                                this.overlay = crate::app::Overlay::Confirm(crate::app::ConfirmKind::UseMyLogin(id));
                                this.overlay_focus.focus(w, cx);
                                cx.notify();
                            }))
                        }))
                    })
                    .child(back(cx))
            }
            ColumnKind::Backlog => {
                let text = t.owner_name.clone().unwrap_or_else(|| dispatcher.choice());
                bar.child(div().text_size(px(12.)).text_color(c(theme::TEXT_2)).child("Owner:"))
                    .child(self.board_picker("ticket-backlog-owner", text, t.owner_employee_id.is_some(), can, Some(id), cx))
                    .child(div().flex_1())
                    .child(dim(ui::button("ticket-delete", "Delete", ButtonKind::Outline, None).h(px(28.)), can).when(can, |el| el.on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.board_dialog(BoardDialog::Delete(id), w, cx)))))
                    .child(dim(ui::button("ticket-start", "▸ Start", ButtonKind::Primary, None).h(px(28.)), can).when(can, |el| el.on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.ticket_start(id, None, cx)))))
            }
            // B6: a failed main task stays where it is, with its reason and "Start again".
            ColumnKind::Todo | ColumnKind::Doing if failed => {
                let why = root.and_then(|r| r.result.clone()).map(|r| format!(": {}", r.lines().next().unwrap_or(""))).unwrap_or_default();
                bar.child(line(format!("✕ The main task failed{why}"), theme::NEGATIVE))
                    .child(back(cx))
                    .child(dim(ui::button("ticket-start-again", "Start again", ButtonKind::Primary, None).h(px(28.)), can).when(can, |el| el.on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.ticket_start(id, None, cx)))))
            }
            ColumnKind::Todo if t.owner_employee_id.is_some() => bar.child(line(format!("Queued for {}.", t.owner_name.clone().unwrap_or_default()), theme::TEXT_2)).child(back(cx)),
            ColumnKind::Todo => {
                let choosing = self.ticket_choosing(t);
                // B5: no dispatcher, or it chose nobody: the board member chooses.
                let unknown = self.ticket_owner_unknown(t);
                let (text, color) = match (choosing, dispatcher.name()) {
                    (false, Some(_)) if unknown => ("◆ Needs you · Dispatcher unknown: its activity did not load. Choose an employee.".to_string(), theme::ACCENT_TEXT),
                    (true, name) => (format!("{} is choosing an employee…", name.unwrap_or("The dispatcher")), theme::TEXT_2),
                    (false, None) => (format!("◆ Needs you · {}. Choose an employee.", dispatcher.why_nobody().unwrap_or_default()), theme::ACCENT_TEXT),
                    (false, Some(name)) => (format!("◆ Needs you · {name} could not choose. Choose an employee."), theme::ACCENT_TEXT),
                };
                let label = if choosing { "Choose yourself" } else { "Choose employee" };
                bar.child(line(text, color)).child(self.board_picker("ticket-todo-owner", label.into(), false, can && owner_choosable(t), Some(id), cx)).child(back(cx))
            }
            ColumnKind::Doing => {
                let text = match (needs, &t.review_feedback) {
                    (Some(Needs::Approval), _) => "◆ Waiting for your approval (Inbox).".to_string(),
                    (_, Some(feedback)) => format!("{}: {}", feedback_label(self.ticket_activity(id)), feedback.lines().next().unwrap_or("")),
                    _ => root.and_then(ui::queued_wait).unwrap_or_else(|| "The owner works on it.".into()),
                };
                bar.child(line(text, if needs.is_some() { theme::ACCENT_TEXT } else { theme::TEXT_2 }))
                    .when(needs == Some(Needs::Approval), |el| el.child(ui::button("ticket-inbox", "Open in Inbox", ButtonKind::Outline, None).h(px(28.)).on_click(|_: &ClickEvent, w, cx| w.dispatch_action(Box::new(crate::keymap::OpenInbox), cx))))
                    .when(root_id.is_some(), |el| el.child(open_conversation(cx)))
                    .child(back(cx))
            }
            ColumnKind::PlanReview => bar.child(line("◆ Plan review · only a board member decides.".into(), theme::ACCENT_TEXT)).when(root_id.is_some(), |el| el.child(open_conversation(cx))),
            // Ticket #5: the tester decides by running the tests; a board member can only cancel.
            ColumnKind::Testing => {
                let tasks = self.board.detail_of(id).map(|d| d.tasks.clone()).unwrap_or_default();
                let (tester, task) = self.ticket_testing(t, &tasks);
                let test_task = task.map(|x| x.id);
                bar.child(line(crate::screens::board::testing_note(tester.as_deref()), theme::TEXT_2))
                    .when_some(test_task, |el, task_id| {
                        el.child(ui::button("ticket-test-task", "Open test task", ButtonKind::Outline, None).h(px(28.)).on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.open_ticket_task(task_id, w, cx))))
                    })
                    .when(root_id.is_some(), |el| el.child(open_conversation(cx)))
                    .child(back(cx))
            }
            ColumnKind::Review => bar.child(line("◆ Review · only a board member decides.".into(), theme::ACCENT_TEXT)).when(root_id.is_some(), |el| el.child(open_conversation(cx))).child(back(cx)),
            ColumnKind::Done => {
                // MU2-12: how the summary went, from the discussion; a failure can be tried again (AC4).
                let summary = match self.ticket_summary(t) {
                    Some(crate::screens::discussion::Summary::Saved) => "Nothing was merged, pushed or deployed. The summary is in project memory.".to_string(),
                    Some(crate::screens::discussion::Summary::Failed(why)) => format!("Nothing was merged, pushed or deployed. ✕ The summary failed{}", if why.is_empty() { ".".to_string() } else { format!(": {}", why.lines().next().unwrap_or("")) }),
                    _ => "Nothing was merged, pushed or deployed. A summary goes to project memory.".to_string(),
                };
                let failed = self.ticket_summary_failed(t);
                let retry = can && !self.board.retrying;
                bar.child(
                    // A min width as `line` has: without it the column takes the text's full width and cuts it off.
                    v().flex_1()
                        .min_w(px(160.))
                        .gap(px(2.))
                        .child(div().w_full().min_w_0().text_size(px(12.5)).text_color(c(theme::ACCENT_TEXT)).child(format!("✓ Closed{}", t.done_at_ms.map(|d| format!(" · {}", ui::fmt_when(d))).unwrap_or_default())))
                        .child(div().w_full().min_w_0().text_size(px(12.)).text_color(c(if failed { theme::NEGATIVE } else { theme::TEXT_2 })).child(summary))
                        .children(self.board.retry_error.clone().filter(|_| failed).map(|e| div().w_full().min_w_0().text_size(px(12.)).text_color(c(theme::NEGATIVE)).child(e))),
                )
                .when(failed, |el| el.child(dim(ui::button("ticket-retry-summary", if self.board.retrying { "Trying again…" } else { "Try again" }, ButtonKind::Outline, None).h(px(28.)), retry).when(retry, |el| el.on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.discussion_retry_summary(id, cx))))))
                .when(root_id.is_some(), |el| el.child(open_conversation(cx)))
            }
        };
        el.into_any_element()
    }

    /// The review gate (§7.3 Review, §9.2): the report, the required feedback, Send back and Done.
    fn render_review_gate(&mut self, t: &TicketView, root: Option<&aurelius_protocol::TaskView>, can: bool, in_dialog: bool, cx: &mut Context<Self>) -> AnyElement {
        let id = t.id;
        // With a discussion the feedback is a message there (MU2-05 AC4, P-27): the button jumps to its composer.
        let discussion = !in_dialog && t.conversation_id.is_some();
        let ready = discussion || feedback_ready(&self.board.feedback.read(cx).value());
        let report = root.and_then(|r| r.result.clone()).filter(|r| !r.trim().is_empty());
        let who = t.owner_name.clone().unwrap_or_else(|| "the owner".into());
        let report: AnyElement = match report {
            Some(text) => gpui_kit::component::text::TextView::markdown(SharedString::from(format!("ticket-report-{id}-{}", t.version)), text).selectable(true).on_link_click(self.markdown_link_handler(false, cx)).into_any_element(),
            None => div().text_size(px(12.5)).text_color(c(theme::TEXT_2)).child("No report text. See the conversation.").into_any_element(),
        };
        let send = ui::button("ticket-send-back", "Send back to Doing", ButtonKind::Outline, None).h(px(30.));
        let send = if ready { send } else { send.opacity(0.5).tooltip(ui::tip("Write what needs to change first.", None)) };
        v().gap(px(10.))
            .p(px(14.))
            .rounded(px(8.))
            .border_1()
            .border_color(c(theme::ATTENTION_BORDER))
            .bg(c(theme::ATTENTION_BG))
            .when(!in_dialog, |el| {
                el.child(h().gap(px(6.)).child(ui::diamond(7., theme::ACCENT_TEXT, true)).child(div().text_size(px(13.)).font_weight(FontWeight::MEDIUM).child("Review · only a board member decides")))
                    .child(div().text_size(px(12.)).text_color(c(theme::TEXT_2)).child(format!("Report from {who}")))
                    .child(div().id("ticket-report").max_h(px(260.)).overflow_y_scroll().p(px(10.)).rounded(px(6.)).bg(c(theme::SURFACE)).child(report))
                    .child(div().text_size(px(11.5)).text_color(c(theme::TEXT_2)).child("Branches and commits in the report are information only: nothing is merged automatically."))
            })
            .when(!discussion, |el| {
                el.child(div().text_size(px(12.)).text_color(c(theme::TEXT_2)).child("Feedback (required to send back)"))
                    .child(Textarea::new(&self.board.feedback).py(px(7.)).line_height(px(20.)))
                    .when(self.board.feedback_missing, |el| el.child(div().text_size(px(12.)).text_color(c(theme::NEGATIVE)).child("Write what needs to change.")))
            })
            .when(discussion, |el| el.child(div().text_size(px(12.)).text_color(c(theme::TEXT_2)).child("Send back writes your feedback as a message in the discussion.")))
            .child(
                h().gap(px(8.))
                    .child(dim(send, can).when(can, |el| el.on_click(cx.listener(move |this, _: &ClickEvent, w, cx| if discussion { this.ticket_to_composer(id, w, cx) } else { this.ticket_send_back(id, w, cx) }))))
                    .child(div().flex_1())
                    .when(!in_dialog, |el| el.child(dim(ui::button("ticket-done", "✓ Done", ButtonKind::Primary, None).h(px(30.)), can).when(can, |el| el.on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.board_dialog(BoardDialog::MarkDone(id), w, cx)))))),
            )
            .into_any_element()
    }

    /// Plan review (§9.1): the plan, the feedback and the four buttons of the existing component.
    fn render_plan_gate(&mut self, t: &TicketView, can: bool, in_dialog: bool, cx: &mut Context<Self>) -> AnyElement {
        let id = t.id;
        let discussion = !in_dialog && t.conversation_id.is_some();
        let ready = discussion || feedback_ready(&self.board.feedback.read(cx).value());
        let conversation = self.board.detail_of(id).and_then(|d| d.root_task.as_ref()).and_then(|r| r.conversation_id);
        let plan = conversation.and_then(|c| self.board.plans.get(&c).cloned());
        let text: AnyElement = match plan {
            Some(Some(text)) => gpui_kit::component::text::TextView::markdown(SharedString::from(format!("ticket-plan-{id}-{}", t.version)), text).selectable(true).plugin(crate::screens::mermaid::Mermaid).on_link_click(self.markdown_link_handler(false, cx)).into_any_element(),
            Some(None) => div().text_size(px(12.5)).text_color(c(theme::TEXT_2)).child("The plan is in the conversation.").into_any_element(),
            None => div().text_size(px(12.5)).text_color(c(theme::TEXT_2)).child("Loading the plan…").into_any_element(),
        };
        let revise = ui::button("ticket-plan-revise", "Pas aan", ButtonKind::Outline, None).h(px(30.));
        let revise = if ready { revise } else { revise.opacity(0.5).tooltip(ui::tip("Write what needs to change first.", None)) };
        let approve = |key: &'static str, text: &'static str, kind: ButtonKind, decision: PlanApproval, cx: &mut Context<Self>| {
            dim(ui::button(key, text, kind, None).h(px(30.)), can).when(can, |el| el.on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.ticket_plan(id, Some(decision), w, cx))))
        };
        v().gap(px(10.))
            .p(px(14.))
            .rounded(px(8.))
            .border_1()
            .border_color(c(theme::ATTENTION_BORDER))
            .bg(c(theme::ATTENTION_BG))
            .when(!in_dialog, |el| el.child(h().gap(px(6.)).child(ui::diamond(7., theme::ACCENT_TEXT, true)).child(div().text_size(px(13.)).font_weight(FontWeight::MEDIUM).child("Plan review · nothing changes until you approve"))))
            .child(div().id("ticket-plan").max_h(px(320.)).overflow_y_scroll().p(px(10.)).rounded(px(6.)).bg(c(theme::SURFACE)).child(text))
            .when(!discussion, |el| {
                el.child(div().text_size(px(12.)).text_color(c(theme::TEXT_2)).child("Feedback (needed for Pas aan)"))
                    .child(Textarea::new(&self.board.feedback).py(px(7.)).line_height(px(20.)))
                    .when(self.board.feedback_missing, |el| el.child(div().text_size(px(12.)).text_color(c(theme::NEGATIVE)).child("Write what needs to change.")))
            })
            .when(discussion, |el| el.child(div().text_size(px(12.)).text_color(c(theme::TEXT_2)).child("Pas aan writes your feedback as a message in the discussion.")))
            .child(
                h().gap(px(8.))
                    .flex_wrap()
                    .child(approve("ticket-plan-approve", "Goedkeuren + Auto", ButtonKind::Primary, PlanApproval::Approve, cx))
                    .child(approve("ticket-plan-approve-edits", "Goedkeuren", ButtonKind::Outline, PlanApproval::ApproveEdits, cx))
                    .child(dim(revise, can).when(can, |el| el.on_click(cx.listener(move |this, _: &ClickEvent, w, cx| if discussion { this.ticket_to_composer(id, w, cx) } else { this.ticket_plan(id, None, w, cx) }))))
                    .child(div().flex_1())
                    .child(dim(ui::button("ticket-plan-reject", "Afwijzen", ButtonKind::Outline, None).h(px(30.)), can).when(can, |el| el.on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.board_dialog(BoardDialog::Reject(id), w, cx))))),
            )
            .into_any_element()
    }

    // ---- The panel ----------------------------------------------------------

    pub fn render_ticket_panel(&mut self, id: TicketId, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(t) = self.board.ticket(id).cloned() else {
            let text = self.board.detail_error.clone().unwrap_or_else(|| "Loading the ticket…".into());
            return v().size_full().bg(c(theme::BG)).border_l_1().border_color(c(theme::BORDER))
                .child(h().h(px(48.)).px(px(16.)).justify_end().child(ui::icon_button("ticket-close").aria_label("Close").child("×").on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.close_panel(cx)))))
                .child(div().p(px(24.)).text_color(c(theme::TEXT_2)).child(text))
                .into_any_element();
        };
        let can = self.board_online() && !self.board.busy.contains(&id);
        let detail = self.board.detail.clone().filter(|d| d.ticket.id == id);
        let root = detail.as_ref().and_then(|d| d.root_task.clone());
        let needs = self.ticket_needs(&t);
        let editing = self.board.editing;
        let open_edit = editable(t.column) && can;

        let title: AnyElement = if editing == Some(EditField::Title) {
            self.inline_edit(ui::input(&self.board.edit_title).into_any_element(), "ticket-title", cx)
        } else {
            h().id("ticket-title")
                .gap(px(8.))
                .items_start()
                .when(open_edit, |el| el.cursor_pointer().on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.ticket_edit(EditField::Title, w, cx))))
                .child(div().flex_1().min_w_0().font_family(ui::serif(cx)).text_size(px(18.)).line_height(px(24.)).child(t.title.clone()))
                .when(open_edit, |el| el.child(div().text_size(px(12.)).text_color(c(theme::TEXT_2)).child("✎")))
                .into_any_element()
        };
        let header = v().flex_none().px(px(18.)).pt(px(12.)).pb(px(12.)).gap(px(8.)).border_b_1().border_color(c(theme::BORDER))
            .child(
                h().gap(px(8.))
                    .child(div().flex_1().font_family(ui::mono(cx)).text_size(px(11.5)).text_color(c(theme::TEXT_2)).child(format!("#{} · {}", t.number, t.project_name)))
                    .child(ui::icon_button("ticket-more").aria_label("More").tooltip(ui::tip("More", None)).child("⋯").on_click(cx.listener(move |this, e: &ClickEvent, w, cx| {
                        if let Some(t) = this.board.ticket(id).cloned() {
                            let items = this.card_menu(&t);
                            this.open_menu(e.position(), items, w, cx);
                        }
                    })))
                    .child(ui::icon_button("ticket-close").aria_label("Close").tooltip(ui::tip("Close", Some("escape"))).child("×").on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.close_panel(cx)))),
            )
            .child(title)
            .child(
                h().gap(px(8.))
                    .flex_wrap()
                    .child(pill(self.board.column_name(t.column)))
                    .children(root.as_ref().filter(|_| matches!(t.column, ColumnKind::Doing | ColumnKind::PlanReview | ColumnKind::Review)).map(|r| ui::task_mark(self.task_mark(r), "ticket-mark")))
                    .child(h().gap(px(6.)).text_size(px(12.)).text_color(c(theme::TEXT_2)).children(t.owner_name.as_ref().map(|n| ui::monogram(n, 18., cx))).child(t.owner_name.clone().unwrap_or_else(|| "No owner".into()))),
            );

        let actions = self.render_ticket_actions(&t, root.as_ref(), needs, can, cx);
        let gate = match t.column {
            ColumnKind::Review => Some(self.render_review_gate(&t, root.as_ref(), can, false, cx)),
            ColumnKind::PlanReview => Some(self.render_plan_gate(&t, can, false, cx)),
            _ => None,
        };
        let description: AnyElement = if editing == Some(EditField::Description) {
            self.inline_edit(Textarea::new(&self.board.edit_description).py(px(7.)).line_height(px(20.)).into_any_element(), "ticket-description", cx)
        } else {
            match t.description.clone().filter(|d| !d.trim().is_empty()) {
                Some(text) => gpui_kit::component::text::TextView::markdown(SharedString::from(format!("ticket-md-{id}-{}", t.version)), text).selectable(true).on_link_click(self.markdown_link_handler(false, cx)).into_any_element(),
                None => div().text_size(px(12.5)).text_color(c(theme::TEXT_2)).child("No description.").into_any_element(),
            }
        };
        let details = self.render_ticket_details(&t, open_edit, can, cx);

        // Tasks: the main task and its delegations as a tree (MU1-08).
        let tasks: Vec<aurelius_protocol::TaskView> = detail.as_ref().map(|d| d.tasks.clone()).unwrap_or_default();
        let refs: Vec<&aurelius_protocol::TaskView> = tasks.iter().collect();
        let tree = crate::screens::tasks::tree_order(&refs);
        let task_rows = v().gap(px(2.)).children(tree.iter().enumerate().map(|(i, (task, depth))| {
            let task_id = task.id;
            h().id(("ticket-task", i))
                .gap(px(8.))
                .pl(px(4. + 16. * *depth as f32))
                .pr(px(4.))
                .py(px(4.))
                .rounded(px(6.))
                .cursor_pointer()
                .hover(|s| s.bg(c(theme::SURFACE)))
                .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.open_ticket_task(task_id, w, cx)))
                .when(*depth > 0, |el| el.child(div().text_color(c(theme::TEXT_2)).child("↳")))
                .child(div().flex_1().min_w_0().truncate().text_size(px(12.5)).child(task.title.clone()))
                .child(div().flex_none().text_size(px(11.5)).text_color(c(theme::TEXT_2)).child(task.employee_name.clone().unwrap_or_default()))
                .child(ui::task_mark(self.task_mark(task), ("ticket-task-mark", i)))
        }));
        let tabs = self.render_ticket_tabs(id, cx);
        if self.board.tab == TicketTab::Discussion {
            let (body, footer) = self.render_ticket_discussion(&t, root.as_ref(), can, window, cx);
            return v().size_full().bg(c(theme::BG)).border_l_1().border_color(c(theme::BORDER)).child(header).child(actions).child(tabs).child(body).children(footer).into_any_element();
        }
        if self.board.tab == TicketTab::Activity {
            let body = v().id("ticket-body").flex_1().min_h_0().overflow_y_scroll().px(px(18.)).py(px(14.)).gap(px(6.)).child(self.render_ticket_activity(id, cx));
            return v().size_full().bg(c(theme::BG)).border_l_1().border_color(c(theme::BORDER)).child(header).child(actions).child(tabs).child(body).into_any_element();
        }
        let pencil = |cx: &mut Context<Self>| div().id("ticket-description-edit").cursor_pointer().text_size(px(12.)).text_color(c(theme::TEXT_2)).hover(|s| s.text_color(c(theme::TEXT))).child("✎").on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.ticket_edit(EditField::Description, w, cx)));
        let body = v().id("ticket-body").flex_1().min_h_0().overflow_y_scroll().px(px(18.)).py(px(14.)).gap(px(16.))
            .children(gate)
            .child(v().gap(px(6.)).child(h().child(div().flex_1().child(ui::eyebrow("DESCRIPTION"))).when(open_edit && editing != Some(EditField::Description), |el| el.child(pencil(cx)))).child(description))
            .child(v().gap(px(6.)).child(ui::eyebrow("DETAILS")).child(details))
            .child(
                v().gap(px(6.))
                    .child(ui::eyebrow(format!("TASKS ({})", tasks.len())))
                    .when(tasks.is_empty(), |el| el.child(div().text_size(px(12.5)).text_color(c(theme::TEXT_2)).child("The main task starts when the ticket starts.")))
                    .child(task_rows),
            );
        v().size_full().bg(c(theme::BG)).border_l_1().border_color(c(theme::BORDER)).child(header).child(actions).child(tabs).child(body).into_any_element()
    }

    /// Ticket | Discussion | Activity (fase 2 §1: Discussion takes the place of Conversation).
    fn render_ticket_tabs(&self, id: TicketId, cx: &mut Context<Self>) -> AnyElement {
        let current = self.board.tab;
        let discussion = self.discussion_tab_label(id);
        h().flex_none().gap(px(4.)).px(px(14.)).py(px(6.)).border_b_1().border_color(c(theme::BORDER))
            .children([(TicketTab::Ticket, "Ticket".to_string()), (TicketTab::Discussion, discussion), (TicketTab::Activity, "Activity".to_string())].into_iter().enumerate().map(|(i, (tab, text))| {
                let on = tab == current;
                h().id(("ticket-tab", i)).role(Role::Tab).aria_label(if on { format!("{text}, selected") } else { text.to_string() }).px(px(10.)).py(px(4.)).rounded(px(6.)).cursor_pointer()
                    .text_size(px(12.5))
                    .text_color(c(if on { theme::TEXT } else { theme::TEXT_2 }))
                    .when(on, |el| el.bg(c(theme::SURFACE_2)))
                    .hover(|s| s.bg(c(theme::SURFACE_2)).text_color(c(theme::TEXT)))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.ticket_tab(tab, cx)))
                    .child(text)
            }))
            .into_any_element()
    }

    /// Activity (§7.4, MU1-15 AC4): newest first, each row time · who · what.
    fn render_ticket_activity(&self, id: TicketId, cx: &mut Context<Self>) -> AnyElement {
        let note = |text: String| div().text_size(px(12.5)).text_color(c(theme::TEXT_2)).child(text).into_any_element();
        let cards = match self.board.activity.as_ref().filter(|(t, _)| *t == id) {
            None => return note("Loading the activity…".into()),
            Some((_, Err(e))) => return note(e.clone()),
            Some((_, Ok(cards))) if cards.is_empty() => return note("Nothing happened yet.".into()),
            Some((_, Ok(cards))) => cards,
        };
        let mono = ui::mono(cx);
        v().gap(px(2.))
            .children(cards.iter().enumerate().map(|(i, card)| {
                let text = activity_text(card);
                h().id(("ticket-activity", i)).role(Role::ListItem).aria_label(format!("{}, {text}", ui::fmt_when(card.at_ms))).gap(px(10.)).items_start().py(px(5.))
                    .child(div().w(px(110.)).flex_none().font_family(mono.clone()).text_size(px(11.5)).text_color(c(theme::TEXT_2)).child(ui::fmt_when(card.at_ms)))
                    .child(
                        v().flex_1().min_w_0().gap(px(1.))
                            .child(div().text_size(px(12.5)).child(text))
                            .children(activity_detail(card).map(|d| div().text_size(px(11.5)).text_color(c(theme::TEXT_2)).child(d.to_string()))),
                    )
            }))
            .into_any_element()
    }

    /// A field being edited with its Save and Cancel.
    fn inline_edit(&self, field: AnyElement, key: &'static str, cx: &mut Context<Self>) -> AnyElement {
        v().gap(px(6.))
            .child(field)
            .child(
                h().gap(px(8.))
                    .child(ui::button(SharedString::from(format!("{key}-save")), "Save", ButtonKind::Primary, None).h(px(26.)).on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.ticket_save_edit(w, cx))))
                    .child(ui::button(SharedString::from(format!("{key}-cancel")), "Cancel", ButtonKind::Outline, None).h(px(26.)).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.ticket_cancel_edit(cx)))),
            )
            .children(self.board.edit_error.clone().map(|e| div().text_size(px(12.)).text_color(c(theme::NEGATIVE)).child(e)))
            .into_any_element()
    }

    /// The ticket's fields (§7.2): each change is saved at once.
    fn render_ticket_details(&mut self, t: &TicketView, open_edit: bool, can: bool, cx: &mut Context<Self>) -> AnyElement {
        let id = t.id;
        let editing = self.board.editing;
        let locked = run_locked(t.root_task_status);
        let offset = aurelius_host::utc_offset_minutes_at(ui::now_ms());
        let dispatcher = self.dispatcher_for(t.project_id);
        let model_text = t.run.model.clone().map_or_else(|| "Default".to_string(), |m| AppView::friendly_model(&m));
        let pencil = |key: &'static str, field: EditField, cx: &mut Context<Self>| {
            div().id(key).cursor_pointer().text_size(px(12.)).text_color(c(theme::TEXT_2)).hover(|s| s.text_color(c(theme::TEXT))).child("✎").on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.ticket_edit(field, w, cx)))
        };
        let locked_value = |key: &'static str, text: String| {
            h().id(key).gap(px(6.)).child(text).when(locked, |el| el.tooltip(ui::tip("Locked while the main task runs", None)).child(div().text_color(c(theme::TEXT_2)).child("· locked, run active"))).into_any_element()
        };
        let priority: AnyElement = if open_edit {
            self.board_picker("ticket-priority", priority_label(t.priority).into(), t.priority != ticket::TicketPriority::Normal, true, Some(id), cx).into_any_element()
        } else {
            div().child(priority_label(t.priority)).into_any_element()
        };
        let owner: AnyElement = if owner_choosable(t) && can {
            let text = t.owner_name.clone().unwrap_or_else(|| dispatcher.choice());
            self.board_picker("ticket-owner", text, t.owner_employee_id.is_some(), true, Some(id), cx).into_any_element()
        } else {
            div().child(t.owner_name.clone().unwrap_or_else(|| "No owner".into())).into_any_element()
        };
        let root = self.board.detail.as_ref().filter(|d| d.ticket.id == t.id).and_then(|d| d.root_task.as_ref()).and_then(|r| r.conversation_id);
        let went = root.and_then(|c| self.board.modes.get(&c).copied());
        let mode: AnyElement = if open_edit && !locked { self.board_picker("ticket-mode", mode_label(t.run.mode), t.run.mode.is_some(), true, Some(id), cx).into_any_element() } else { locked_value("ticket-mode-locked", mode_text(t.run.mode, went).unwrap_or_else(|| mode_label(t.run.mode))) };
        let model: AnyElement = if open_edit && !locked { self.board_picker("ticket-model", model_text.clone(), t.run.model.is_some(), true, Some(id), cx).into_any_element() } else { locked_value("ticket-model-locked", model_text) };
        let labels: AnyElement = if editing == Some(EditField::Labels) {
            self.inline_edit(ui::input(&self.board.edit_labels).into_any_element(), "ticket-labels", cx)
        } else {
            h().gap(px(4.))
                .flex_wrap()
                .children(t.labels.iter().map(|l| div().px(px(6.)).py(px(1.)).rounded(px(8.)).bg(c(theme::SURFACE_2)).text_size(px(11.)).child(l.clone())))
                .when(t.labels.is_empty(), |el| el.child(div().text_color(c(theme::TEXT_2)).child("None")))
                .when(open_edit, |el| el.child(pencil("ticket-labels-edit", EditField::Labels, cx)))
                .into_any_element()
        };
        let deadline: AnyElement = if editing == Some(EditField::Deadline) {
            self.inline_edit(ui::input(&self.board.edit_deadline).into_any_element(), "ticket-deadline", cx)
        } else {
            let (text, color) = t.deadline_ms.map_or(("None".to_string(), theme::TEXT_2), |d| deadline_text(d, ui::now_ms(), t.column == ColumnKind::Done, offset));
            h().gap(px(6.)).child(div().text_color(c(color)).child(text)).when(open_edit, |el| el.child(pencil("ticket-deadline-edit", EditField::Deadline, cx))).into_any_element()
        };
        // As a task's: a click opens the copy (or the file fetched from the server).
        let attachments: AnyElement = if t.attachments.is_empty() {
            div().text_color(c(theme::TEXT_2)).child("None").into_any_element()
        } else {
            self.attachment_chips("ticket-att", &t.attachments, t.project_id, None, None, cx)
        };
        let settings = div().id("ticket-dispatcher-settings").cursor_pointer().text_size(px(11.5)).text_color(c(theme::ACCENT_TEXT)).child("Change in project settings").on_click(cx.listener(move |this, _: &ClickEvent, w, cx| {
            if let Some(p) = this.board.ticket(id).map(|t| t.project_id) {
                this.open_project_settings(p, w, cx);
            }
        }));
        v().gap(px(4.))
            .child(row("Project", div().child(t.project_name.clone())))
            .child(row("Priority", priority))
            .child(row("Owner", owner))
            .child(row("Dispatcher", v().gap(px(2.)).child(dispatcher.line()).child(settings)))
            .child(row("Mode", mode))
            .child(row("Model", model))
            .child(row("Labels", labels))
            .child(row("Deadline", deadline))
            .child(row("Attachments", attachments))
            .child(row("Created by", div().child(format!("{} · {}", t.created_by_username, ui::fmt_when(t.created_at_ms)))))
            // MU3-10 AC4: whose subscription its runs use; there is no choice of "someone's login".
            .child(row("Runs on", div().child(format!("{} Claude login", ui::capitalised(&crate::screens::board::login_owner(t, self.active_member().map(|u| u.id)))))))
            .into_any_element()
    }

    // ---- Dialogs ------------------------------------------------------------

    pub fn render_board_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(dialog) = self.board.dialog else { return div().into_any_element() };
        if self.board.form.reset {
            self.board.form.reset = false;
            for input in [self.board.form.title.clone(), self.board.form.labels.clone(), self.board.form.deadline.clone()] {
                input.update(cx, |i, cx| i.set_value("", window, cx));
            }
            self.board.form.description.update(cx, |i, cx| i.set_value("", window, cx));
        }
        // "Create ticket" from a message (MU2-10 AC1): the form starts filled in.
        if let Some(fill) = self.board.form.prefill.take() {
            self.board.form.title.update(cx, |i, cx| i.set_value(fill.title, window, cx));
            self.board.form.description.update(cx, |i, cx| i.set_value(fill.description, window, cx));
        }
        // After a conflict the columns show what the server has now (D-m2); the message stays.
        if self.board.columns_reload.take_rebuild() && dialog == BoardDialog::Columns {
            let message = self.board.columns_error.take();
            self.board_columns_begin(window, cx);
            self.board.columns_error = message;
        }
        let online = self.board_online();
        let ticket = dialog_ticket(dialog).and_then(|id| self.board.ticket(id).cloned());
        if dialog_ticket(dialog).is_some() && ticket.is_none() {
            return div().into_any_element();
        }
        let (title, width, body, buttons): (String, f32, AnyElement, Vec<AnyElement>) = match (dialog, ticket) {
            (BoardDialog::NewTicket, _) => {
                let (body, buttons) = self.render_new_ticket(online, cx);
                ("New ticket".into(), 680., body, buttons)
            }
            (BoardDialog::AnotherTicket, _) => {
                let Some(another) = self.board.another.clone() else { return div().into_any_element() };
                let go = dim(ui::button("board-another-confirm", "Create another", ButtonKind::Primary, None), online).when(online, |el| {
                    el.on_click(cx.listener(|this, _: &ClickEvent, w, cx| {
                        if let Some(a) = this.board.another.take() {
                            this.ticket_from_message_form(a.message, a.project, a.prefill, w, cx);
                        }
                    }))
                });
                (format!("{} was already created from this message", another.existing), 460., div().text_size(px(13.)).child(format!("{} was already created from this message. Create another?", another.existing)).into_any_element(), vec![go.into_any_element()])
            }
            (BoardDialog::Columns, _) => {
                let (body, buttons) = self.render_columns_settings(online, cx);
                ("Settings › Board".into(), 620., body, buttons)
            }
            (BoardDialog::SendBack(_), Some(t)) => {
                let who = t.owner_name.clone().unwrap_or_else(|| "The owner".into());
                let gate = self.render_review_gate(&t, None, online, true, cx);
                let text = format!("{who} gets your feedback and continues in the same conversation. The ticket returns to Review with a new report.");
                (format!("Send #{} back to Doing", t.number), 520., v().gap(px(10.)).child(div().text_size(px(12.5)).text_color(c(theme::TEXT_2)).child(text)).child(gate).into_any_element(), vec![])
            }
            (BoardDialog::Plan(_), Some(t)) => {
                let gate = self.render_plan_gate(&t, online, true, cx);
                (format!("Plan of #{}", t.number), 640., gate, vec![])
            }
            (BoardDialog::MarkDone(id), Some(t)) => {
                let check = self.ticket_done_check(id);
                let can = check.allowed(online);
                let wait = |text: String| div().text_size(px(12.)).text_color(c(theme::TEXT_2)).child(text);
                let body = v().gap(px(8.))
                    .child(div().text_size(px(13.)).child("Done closes the ticket. It cannot be reopened."))
                    .child(div().text_size(px(13.)).child("Nothing is merged, pushed or deployed."))
                    .map(|el| match &check {
                        DoneCheck::Loading => el.child(wait(self.board.dialog_error.clone().unwrap_or_else(|| "Checking what still runs…".into()))),
                        DoneCheck::Working { tasks, conversation } => {
                            let mut lines = v().gap(px(4.)).pt(px(6.));
                            if !tasks.is_empty() {
                                let head = if tasks.len() == 1 { "1 delegation is".to_string() } else { format!("{} delegations are", tasks.len()) };
                                lines = lines
                                    .child(div().text_size(px(12.5)).text_color(c(crate::screens::board::HIGH)).child(format!("⚠ {head} still running:")))
                                    .children(tasks.iter().map(|x| wait(format!("◌ {} — {}", x.title, x.employee_name.clone().unwrap_or_default()))));
                            }
                            if *conversation {
                                lines = lines.child(div().text_size(px(12.5)).text_color(c(crate::screens::board::HIGH)).child(format!("⚠ {} is still answering in the conversation.", t.owner_name.clone().unwrap_or_else(|| "The owner".into()))));
                            }
                            el.child(lines.child(wait("Done waits until nothing runs any more (B6).".into())))
                        }
                        DoneCheck::Ready => el,
                    });
                let done = dim(ui::button("board-done-confirm", "✓ Done", ButtonKind::Primary, None), can).when(can, |el| el.on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.ticket_accept(id, w, cx))));
                (format!("Mark #{} as Done?", t.number), 460., body.into_any_element(), vec![done.into_any_element()])
            }
            (BoardDialog::BackToBacklog(id) | BoardDialog::Reject(id), Some(t)) => {
                let reject = matches!(dialog, BoardDialog::Reject(_));
                let text = if reject {
                    "The plan is rejected: the main task stops and the ticket goes back to Backlog."
                } else {
                    "This stops the main task and its delegations. The ticket goes back to Backlog; Start makes a new main task."
                };
                let go = dim(ui::button("board-back-confirm", if reject { "Afwijzen" } else { "Back to Backlog" }, ButtonKind::Primary, None), online).when(online, |el| {
                    el.on_click(cx.listener(move |this, _: &ClickEvent, w, cx| {
                        this.ticket_cancel(id, cx);
                        this.board_close_dialog(w, cx);
                    }))
                });
                let title = if reject { format!("Reject the plan of #{}?", t.number) } else { format!("Back to Backlog: #{}?", t.number) };
                (title, 460., div().text_size(px(13.)).child(text).into_any_element(), vec![go.into_any_element()])
            }
            (BoardDialog::Delete(id), Some(t)) => {
                let go = dim(ui::button("board-delete-confirm", "Delete", ButtonKind::Primary, None), online).when(online, |el| {
                    el.on_click(cx.listener(move |this, _: &ClickEvent, w, cx| {
                        let number = this.board.ticket(id).map_or(0, |t| t.number);
                        this.board_send(Command::ArchiveTicket(ticket::ArchiveTicket { ticket_id: id }), Some(format!("#{number} deleted from the board.")));
                        if this.panel == Panel::Ticket(id) {
                            this.close_panel(cx);
                        }
                        this.board_close_dialog(w, cx);
                    }))
                });
                (format!("Delete #{}?", t.number), 440., div().text_size(px(13.)).child("It leaves the board and stays readable for the record.").into_any_element(), vec![go.into_any_element()])
            }
            _ => return div().into_any_element(),
        };
        let top = 70.;
        let max_h = (f32::from(window.viewport_size().height) - top - 24.).max(200.);
        let max_w = (f32::from(window.viewport_size().width) - 32.).max(280.);
        let scroll = window.use_keyed_state("board-dialog-scroll", cx, |_, _| ScrollHandle::default()).read(cx).clone();
        let card = crate::screens::overlays::card(width)
            // Files dropped on New ticket are attached to it.
            .when(dialog == BoardDialog::NewTicket, |el| {
                el.drag_over::<ExternalPaths>(|style, _, _, _| style.bg(c(theme::SURFACE)))
                    .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| this.board_add_files(paths.paths().to_vec(), cx)))
            })
            .child(div().px(px(22.)).pt(px(18.)).font_family(ui::serif(cx)).text_size(px(20.)).child(title))
            .child(ui::scroll_frame(&scroll, crate::screens::overlays::scroll_body("board-dialog-body", &scroll, (max_h - 130.).max(120.)).px(px(22.)).py(px(14.)).child(body)))
            .child(
                h().px(px(22.))
                    .py(px(14.))
                    .gap(px(8.))
                    .border_t_1()
                    .border_color(c(theme::BORDER))
                    .justify_end()
                    .child(ui::button("board-dialog-cancel", "Cancel", ButtonKind::Outline, Some(ui::kbd("escape", cx))).on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.board_close_dialog(w, cx))))
                    .children(buttons),
            );
        deferred(
            div().id("board-dialog-scrim").absolute().top_0().left_0().size_full().bg(Hsla { a: 0.55, ..c(0x000000) }).flex().justify_center().items_start().pt(px(top)).occlude()
                .key_context(crate::keymap::LOAD)
                .child(v().max_h(px(max_h)).max_w(px(max_w)).on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()).child(card)),
        )
        .with_priority(1)
        .into_any_element()
    }

    /// New ticket (§10.1).
    fn render_new_ticket(&mut self, online: bool, cx: &mut Context<Self>) -> (AnyElement, Vec<AnyElement>) {
        let form = &self.board.form;
        let title_text = form.title.read(cx).value().to_string();
        let (title_problem, project_problem) = new_ticket_problems(&title_text, form.project);
        let count = title_text.trim().chars().count();
        let (sent, busy) = (form.sent, form.busy);
        let project = form.project.and_then(|p| self.projects.iter().find(|x| x.id == p)).map_or_else(|| "Choose a project".to_string(), |p| p.name.clone());
        let owner = match form.owner.and_then(|e| self.employee_name(e)) {
            Some(n) => n,
            None => form.project.map_or_else(|| Dispatcher::None.choice(), |p| self.dispatcher_for(p).choice()),
        };
        let model = form.run.model.clone().map_or_else(|| "Default".to_string(), |m| AppView::friendly_model(&m));
        let me = self.members.who.as_ref().and_then(|w| w.user.as_ref()).map(|u| u.username.clone());
        let (title_input, description, labels, deadline) = (form.title.clone(), form.description.clone(), form.labels.clone(), form.deadline.clone());
        let (priority, mode, has_owner, has_project, has_model, error) = (form.priority, form.run.mode, form.owner.is_some(), form.project.is_some(), form.run.model.is_some(), form.error.clone());
        let field = |name: &'static str, content: AnyElement, problem: Option<&'static str>| {
            v().gap(px(5.)).child(div().text_size(px(12.)).text_color(c(theme::TEXT_2)).child(name)).child(content).children(problem.filter(|_| sent).map(|p| div().text_size(px(12.)).text_color(c(theme::NEGATIVE)).child(p)))
        };
        let counter = (count >= 140).then(|| div().text_size(px(11.5)).text_color(c(if count > 160 { theme::NEGATIVE } else { theme::TEXT_2 })).child(format!("{count} / 160")));
        let pickers = h().gap(px(8.))
            .flex_wrap()
            .child(self.board_picker("ticket-form-project", format!("Project: {project}"), has_project, !busy, None, cx))
            .child(self.board_picker("ticket-form-priority", format!("Priority: {}", priority_label(priority)), priority != ticket::TicketPriority::Normal, !busy, None, cx))
            .child(self.board_picker("ticket-form-owner", format!("Owner: {owner}"), has_owner, !busy, None, cx))
            .child(self.board_picker("ticket-form-mode", format!("Mode: {}", mode_label(mode)), mode.is_some(), !busy, None, cx))
            .child(self.board_picker("ticket-form-model", format!("Model: {model}"), has_model, !busy, None, cx));
        let body = v().gap(px(12.))
            .child(field("Title *", v().gap(px(3.)).child(ui::input(&title_input)).children(counter).into_any_element(), title_problem))
            .child(field("Description", Textarea::new(&description).py(px(7.)).line_height(px(20.)).into_any_element(), None))
            .child(pickers)
            .children(project_problem.filter(|_| sent).map(|p| div().text_size(px(12.)).text_color(c(theme::NEGATIVE)).child(p)))
            .when(mode == Some(aurelius_domain::autonomy::AgentMode::Plan), |el| el.child(div().text_size(px(12.)).text_color(c(theme::TEXT_2)).child("Stops in Plan review with a plan before anything changes.")))
            .child(h().gap(px(10.)).child(div().flex_1().child(field("Labels (comma separated)", ui::input(&labels).into_any_element(), None))).child(div().w(px(220.)).child(field("Deadline", ui::input(&deadline).into_any_element(), None))))
            .child(self.render_ticket_files(busy, cx))
            .children(error.map(|e| div().text_size(px(12.)).text_color(c(theme::NEGATIVE)).child(e)))
            .child(div().text_size(px(12.)).text_color(c(theme::TEXT_2)).child(match me {
                Some(name) => format!("Lands in Backlog. Nothing starts until you press Start. Created by {name}."),
                None => "Lands in Backlog. Nothing starts until you press Start.".to_string(),
            }));
        let enabled = online && !busy;
        let start = dim(ui::button("ticket-form-create-start", "Create and start", ButtonKind::Outline, None), enabled).when(enabled, |el| el.on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.board_create(true, w, cx))));
        let create = dim(ui::button("ticket-form-create", if busy { "Creating…" } else { "Create" }, ButtonKind::Primary, None), enabled).when(enabled, |el| el.on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.board_create(false, w, cx))));
        (body.into_any_element(), vec![start.into_any_element(), create.into_any_element()])
    }

    /// New ticket: the files to attach (MU1-07), with Add files… (Ctrl+O) and drop.
    fn render_ticket_files(&self, busy: bool, cx: &mut Context<Self>) -> AnyElement {
        let chips = self.board.form.files.iter().enumerate().map(|(i, (path, size))| {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            h().id(("ticket-form-file", i)).gap(px(6.)).px(px(9.)).py(px(4.)).rounded(px(6.)).border_1().border_color(c(theme::BORDER)).bg(c(theme::SURFACE)).text_size(px(12.))
                .child(div().text_color(c(theme::ACCENT_TEXT)).child("▤"))
                .child(div().max_w(px(220.)).truncate().child(name.clone()))
                .child(div().text_color(c(theme::TEXT_2)).child(crate::app::file_size(*size)))
                .child(
                    div().id(("ticket-form-file-x", i)).role(Role::Button).aria_label(format!("Remove {name}")).cursor_pointer().child("×")
                        .when(!busy, |el| el.on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            if i < this.board.form.files.len() {
                                this.board.form.files.remove(i);
                            }
                            cx.notify();
                        }))),
                )
        });
        let add = dim(ui::button("ticket-form-attach", "Add files…", ButtonKind::Outline, Some(ui::kbd("secondary-o", cx))).h(px(28.)), !busy).when(!busy, |el| el.on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.board_pick_files(cx))));
        v().gap(px(6.))
            .child(h().gap(px(8.)).child(div().flex_1().text_size(px(12.)).text_color(c(theme::TEXT_2)).child(ui::label_with_info("Attachments", "Pick files or drop them on this dialog. At most 8, 25 MB each; the owner finds them in its work folder."))).child(add))
            .when(!self.board.form.files.is_empty(), |el| el.child(h().flex_wrap().gap(px(6.)).children(chips)))
            .into_any_element()
    }

    /// Settings → Board (§12.1): rename and reorder the six columns, and the company's dispatcher.
    fn render_columns_settings(&mut self, online: bool, cx: &mut Context<Self>) -> (AnyElement, Vec<AnyElement>) {
        let count = self.board.columns.len();
        let rows = v().gap(px(6.)).children(self.board.columns.iter().enumerate().map(|(i, (column, input))| {
            h().id(("board-column-row", i))
                .gap(px(8.))
                .tooltip(ui::tip("This column has a role in the workflow and cannot be removed.", None))
                .child(ui::icon_button(("board-column-left", i)).aria_label("Move left").tooltip(ui::tip("Move left", None)).child("↑").when(i == 0, |el| el.opacity(0.3)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.board_columns_move(i, true, cx))))
                .child(ui::icon_button(("board-column-right", i)).aria_label("Move right").tooltip(ui::tip("Move right", None)).child("↓").when(i + 1 == count, |el| el.opacity(0.3)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.board_columns_move(i, false, cx))))
                .child(div().flex_1().child(ui::input(input)))
                .child(div().w(px(100.)).child(pill(column_role(column.kind).into())))
                .when(column.kind == ColumnKind::PlanReview, |el| el.child(div().text_size(px(11.5)).text_color(c(theme::TEXT_2)).child("Plan mode only")))
        }));
        let company = self.company.as_ref().and_then(|c| c.company.dispatcher_employee_id);
        let company_name = company.and_then(|e| self.employee_name(e)).unwrap_or_else(|| "Nobody".into());
        let open = self.overlay == Overlay::Menu && self.menu.as_ref().is_some_and(|m| m.field == Some("board-company-dispatcher"));
        let dispatcher = ui::dropdown_trigger("board-company-dispatcher", 28., company.is_some(), open)
            .max_w(px(260.))
            .child(div().min_w_0().truncate().child(company_name))
            .child(ui::caret())
            .child(ui::track_bounds(self.dropdown_bounds.clone(), "board-company-dispatcher"))
            .when(online, |el| {
                el.on_click(cx.listener(move |this, _: &ClickEvent, w, cx| {
                    let mut items: MenuItems = vec![menu_entry("Nobody (tickets wait in Todo)", BoardDo::CompanyDispatcher(None), company.is_none(), None), None];
                    items.extend(this.employee_choices().into_iter().map(|(e, name)| menu_entry(name, BoardDo::CompanyDispatcher(Some(e)), company == Some(e), None)));
                    this.open_dropdown("board-company-dispatcher", items, w, cx);
                }))
            });
        let body = v().gap(px(12.))
            .child(ui::eyebrow("COLUMNS"))
            .child(div().text_size(px(12.5)).text_color(c(theme::TEXT_2)).child("These columns apply to every project. Each has a fixed role in the workflow; you can rename and reorder them."))
            .when(self.board.columns.is_empty(), |el| el.child(div().text_size(px(12.5)).text_color(c(theme::TEXT_2)).child("Open the Board first to load its columns.")))
            .child(rows)
            .children(self.board.columns_error.clone().map(|e| div().text_size(px(12.)).text_color(c(theme::NEGATIVE)).child(e)))
            .when(count > 0, |el| {
                el.child(div().id("board-columns-restore").w_auto().cursor_pointer().text_size(px(12.5)).text_color(c(theme::ACCENT_TEXT)).role(Role::Button).aria_label("Restore defaults").child("Restore defaults").on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.board_columns_restore(w, cx))))
            })
            .child(div().pt(px(8.)).child(ui::eyebrow("DISPATCHER")))
            .child(h().gap(px(10.)).child(div().text_size(px(12.5)).child("Default dispatcher")).child(dispatcher))
            .child(div().text_size(px(12.)).text_color(c(theme::TEXT_2)).child("Chooses an owner for tickets that start without one. Projects can set their own in Project settings."))
            .children(self.render_tester_picker(online, cx));
        let ready = online && count > 0;
        let save = dim(ui::button("board-columns-save", "Save columns", ButtonKind::Primary, None), ready).when(ready, |el| el.on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.board_columns_save(cx))));
        (body.into_any_element(), vec![save.into_any_element()])
    }
}

impl AppView {
    /// Ticket #5: the company's tester, beside the dispatcher (Settings › Board) and in
    /// Settings → Company. Only on a server with `board_testing`.
    pub fn render_tester_picker(&mut self, online: bool, cx: &mut Context<Self>) -> Option<Div> {
        if !self.board_testing_supported() {
            return None;
        }
        let tester = self.company_tester();
        let name = tester.map(|e| self.employee_name(e).unwrap_or_else(|| "An employee who left".into())).unwrap_or_else(|| crate::screens::board::NO_TESTER.into());
        let open = self.overlay == Overlay::Menu && self.menu.as_ref().is_some_and(|m| m.field == Some("board-company-tester"));
        let picker = ui::dropdown_trigger("board-company-tester", 28., tester.is_some(), open)
            .max_w(px(260.))
            .child(div().min_w_0().truncate().child(name))
            .child(ui::caret())
            .child(ui::track_bounds(self.dropdown_bounds.clone(), "board-company-tester"))
            .when(online, |el| {
                el.on_click(cx.listener(move |this, _: &ClickEvent, w, cx| {
                    let mut items: MenuItems = vec![menu_entry(crate::screens::board::NO_TESTER, BoardDo::CompanyTester(None), tester.is_none(), None), None];
                    items.extend(this.employee_choices().into_iter().map(|(e, name)| menu_entry(name, BoardDo::CompanyTester(Some(e)), tester == Some(e), None)));
                    this.open_dropdown("board-company-tester", items, w, cx);
                }))
            });
        Some(
            v().gap(px(6.))
                .child(div().pt(px(8.)).child(ui::eyebrow("TESTER")))
                .child(h().gap(px(10.)).child(div().text_size(px(12.5)).child("Tester")).child(picker))
                .child(div().text_size(px(12.)).text_color(c(theme::TEXT_2)).child("Runs the full test suite once when the owner reports, in the Testing column. Green goes on to Review; red goes back to Doing with the failures.")),
        )
    }
}

/// The ticket a dialog is about.
fn dialog_ticket(dialog: BoardDialog) -> Option<TicketId> {
    match dialog {
        BoardDialog::SendBack(id) | BoardDialog::MarkDone(id) | BoardDialog::Plan(id) | BoardDialog::BackToBacklog(id) | BoardDialog::Reject(id) | BoardDialog::Delete(id) => Some(id),
        BoardDialog::NewTicket | BoardDialog::Columns | BoardDialog::AnotherTicket => None,
    }
}

/// The fixed role of a column, shown beside its name (§12.1, MU1-06 AC4).
pub fn column_role(kind: ColumnKind) -> &'static str {
    match kind {
        ColumnKind::Backlog => "Start",
        ColumnKind::Todo => "Dispatch",
        ColumnKind::Doing => "Work",
        ColumnKind::PlanReview => "Plan gate",
        ColumnKind::Testing => "Test gate",
        ColumnKind::Review => "Review gate",
        ColumnKind::Done => "Closed",
    }
}

#[cfg(test)]
mod tests {
    use super::{column_role, editable, owner_choosable, run_locked};
    use crate::screens::board::tests::card;
    use aurelius_domain::task::TaskStatus;
    use aurelius_domain::ticket::ColumnKind;

    #[test]
    fn mode_and_model_lock_while_the_main_task_runs() {
        assert!(run_locked(Some(TaskStatus::Running)), "MU1-07 AC5");
        for s in [None, Some(TaskStatus::Draft), Some(TaskStatus::Queued), Some(TaskStatus::Review), Some(TaskStatus::Failed)] {
            assert!(!run_locked(s), "{s:?}");
        }
    }

    #[test]
    fn done_is_read_only_and_the_owner_is_chosen_before_the_work() {
        assert!(!editable(ColumnKind::Done), "MU1-16 AC1");
        assert!(editable(ColumnKind::Review));
        let mut t = card(3, ColumnKind::Backlog);
        assert!(owner_choosable(&t), "Backlog: free, also back to the dispatcher");
        t.column = ColumnKind::Todo;
        t.root_task_status = Some(TaskStatus::Draft);
        assert!(owner_choosable(&t), "Todo with the dispatcher: Choose employee (MU1-12 AC2)");
        t.column = ColumnKind::Doing;
        t.root_task_status = Some(TaskStatus::Running);
        assert!(!owner_choosable(&t), "from Doing read-only");
    }

    #[test]
    fn every_column_shows_its_fixed_role() {
        assert_eq!(ColumnKind::ALL.map(column_role), ["Start", "Dispatch", "Work", "Plan gate", "Test gate", "Review gate", "Closed"]);
    }
}
