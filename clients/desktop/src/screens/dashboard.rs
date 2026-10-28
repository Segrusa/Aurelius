//! The Dashboard, one column: Give work, then what waits for you, then what is running now
//! (plan v0.9, phase 2).

use crate::app::{AppView, ManagerRef, NavItem, Overlay, Panel};
use crate::keymap::*;
use crate::theme::{self, c};
use crate::ui::{self, ButtonKind, h, v};
use aurelius_domain::{DepartmentId, EmployeeId, employee::LifecycleStatus};
use aurelius_protocol::EmployeeSummary;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

/// The management team: active employees without a boss (or whose boss is archived).
pub fn roots(managers: &[EmployeeSummary]) -> Vec<EmployeeId> {
    let active = |id: EmployeeId| managers.iter().any(|m| m.id == id && m.status != LifecycleStatus::Archived);
    managers
        .iter()
        .filter(|m| m.status == LifecycleStatus::Active && m.reports_to.is_none_or(|boss| !active(boss)))
        .map(|m| m.id)
        .collect()
}

/// The management team a task mode is chosen for: the roots and those who report to them directly
/// (the server: the CEO and their direct reports).
pub fn is_management(managers: &[EmployeeSummary], id: EmployeeId) -> bool {
    let team = roots(managers);
    team.contains(&id) || managers.iter().any(|m| m.id == id && m.status == LifecycleStatus::Active && m.reports_to.is_some_and(|boss| team.contains(&boss)))
}

/// Whom work from the Board goes to unless picked: the single root employee; with several
/// roots, `last` (whom the user last gave work to) while it is active, else the first of the
/// management team. Without roots the user picks.
pub fn default_assignee(managers: &[EmployeeSummary], last: Option<EmployeeId>) -> Option<EmployeeId> {
    let active = |id: &EmployeeId| managers.iter().any(|m| m.id == *id && m.status == LifecycleStatus::Active);
    match roots(managers).as_slice() {
        [] => None,
        [only] => Some(*only),
        several => last.filter(active).or(Some(several[0])),
    }
}

/// The assignee picker: the management team first, then the other active employees grouped
/// by department (those without one last), in the order they come.
pub fn assignee_order(managers: &[EmployeeSummary]) -> Vec<&EmployeeSummary> {
    let team = roots(managers);
    let active: Vec<&EmployeeSummary> = managers.iter().filter(|m| m.status == LifecycleStatus::Active).collect();
    let mut out: Vec<&EmployeeSummary> = active.iter().copied().filter(|m| team.contains(&m.id)).collect();
    let mut departments: Vec<Option<DepartmentId>> = Vec::new();
    for m in active.iter().filter(|m| !team.contains(&m.id)) {
        if m.department_id.is_some() && !departments.contains(&m.department_id) {
            departments.push(m.department_id);
        }
    }
    departments.push(None);
    for d in departments {
        out.extend(active.iter().copied().filter(|m| m.department_id == d && !team.contains(&m.id)));
    }
    out
}

/// What "Needs you" lists: approvals, proposals and tasks waiting for the user. Unread results
/// count on the inbox badge only, as the list does not show them.
pub fn needs_you(approvals: usize, proposals: usize, tasks: &[aurelius_protocol::TaskView]) -> usize {
    approvals + proposals + tasks.iter().filter(|t| ui::is_waiting(t)).count()
}

impl AppView {
    pub fn needs_you_count(&self) -> usize {
        needs_you(self.pending_approvals().count(), self.pending_proposals().len(), &self.tasks)
    }

    pub fn render_dashboard(&mut self, _wide: bool, cx: &mut Context<Self>) -> AnyElement {
        let body = v().min_w_0().gap(px(28.)).pt(px(32.)).pb(px(40.))
            .child(self.give_work_box(cx))
            .child(self.needs_you_section(cx))
            .child(self.now_section(cx));
        self.page(body).into_any_element()
    }

    /// A chip in the Give work bar: a click opens the same dropdown as its shortcut.
    fn give_chip(&self, id: &'static str, label: String, bordered: bool, open: fn() -> Box<dyn Action>, cx: &mut Context<Self>) -> Stateful<Div> {
        let bounds = self.dropdown_bounds.clone();
        let open_now = self.overlay == Overlay::Menu && self.menu.as_ref().is_some_and(|m| m.field == Some(id));
        let short = ui::short_default(&label);
        ui::dropdown_trigger(id, 28., bordered, open_now).aria_label(label.clone()).flex_shrink(1.).max_w(px(220.))
            .tooltip(ui::tip(label, None))
            .on_click(cx.listener(move |_, _: &ClickEvent, w, cx| w.dispatch_action(open(), cx)))
            .child(div().min_w_0().truncate().child(short))
            .child(ui::caret())
            .child(ui::track_bounds(bounds, id))
    }

    /// "Give work": only the text (the title follows from it), files as chips, and a bar with who
    /// gets it, the project, model and mode. Ctrl+Enter gives it and opens it in the right panel.
    fn give_work_box(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let who = self.give_assignee().and_then(|id| self.managers.iter().find(|m| m.id == id));
        let who_label = who.map_or_else(|| "Choose who".to_string(), |m| m.name.clone());
        let project = self.give_project_id().and_then(|id| self.projects.iter().find(|p| p.id == id)).map(|p| p.name.clone());
        let (mode, model, effort) = self.task_run_labels();
        let run = self.pending_task_run.clone();
        let show_effort = self.give_more_open || run.effort.is_some();
        // The default is spelled out in the summary; the chips show the short form.
        let short = |s: String| s.split(" · ").next().unwrap_or("").to_string();
        let model = if run.model.is_some() { short(model) } else { short(self.default_run_label(self.give_assignee(), self.give_project_id())) };
        // No mode picked: the employee's own (the one it was last given work in), unless management chooses.
        let mode = if run.mode.is_some() || self.manager_chooses_mode() { mode } else { who.map_or_else(|| "Auto".to_string(), |m| m.agent_mode.label().to_string()) };
        let this = cx.entity().downgrade();
        let has_files = !self.pending_task_files.is_empty();
        v().id("give-work")
            .key_context(GIVE_WORK)
            .gap(px(10.))
            .px(px(18.))
            .pt(px(16.))
            .pb(px(12.))
            .rounded(px(10.))
            .border_1()
            .border_color(c(theme::BORDER))
            .bg(c(theme::SURFACE))
            .child(gpui_kit::component::input::Textarea::new(&self.give_goal).py(px(4.)).line_height(px(22.)).on_paste(move |item, _, cx| {
                this.update(cx, |this, cx| this.paste_into_task_goal(item, cx)).unwrap_or(false)
            }))
            .when(has_files, |el| el.child(self.task_files_row(cx)))
            .child(div().h(px(1.)).bg(c(theme::BORDER)))
            .child(
                v().gap(px(10.)).child(h().gap(px(6.)).items_center().min_w_0()
                    .child(
                        h().id("task-attach").flex_none().h(px(28.)).px(px(12.)).gap(px(6.)).rounded(px(14.)).cursor_pointer().text_size(px(12.5)).text_color(c(theme::TEXT_2))
                            .hover(|s| s.bg(c(theme::SURFACE_2)))
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.pick_task_files(cx)))
                            .child("+ Attach"),
                    )
                    .child(self.give_chip("give-manager", format!("For {who_label}"), true, || Box::new(CycleTaskManager), cx))
                    .child(self.give_chip("give-project", project.unwrap_or_else(|| "Choose a project".into()), true, || Box::new(CycleGiveProject), cx))
                    .child(self.give_chip("task-model", model, false, || Box::new(CycleTaskModel), cx))
                    .child(self.give_chip("task-mode", mode, false, || Box::new(CycleTaskMode), cx))
                    .when(show_effort, |el| el.child(self.give_chip("task-effort", effort, false, || Box::new(CycleTaskEffort), cx)))
                ).child(h().justify_end().child(ui::button("give-work-submit", "Give work", ButtonKind::Primary, Some(ui::kbd("secondary-enter", cx))).flex_none().on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.give_work(w, cx))))),
            )
    }

    /// What waits for a decision, as cards (who · when, then the text); a click opens the inbox or the task.
    fn needs_you_section(&self, cx: &mut Context<Self>) -> impl IntoElement {
        // (who, when, text, task to open)
        let mut cards: Vec<(String, i64, String, Option<aurelius_domain::TaskId>)> = self.pending_approvals().take(4).map(|a| {
            let s = crate::screens::approval::approval_summary(a, &[]);
            (a.requester.clone(), a.requested_at_ms, format!("{} · {}", s.title, s.gist), None)
        }).collect();
        let room = 4usize.saturating_sub(cards.len());
        cards.extend(self.pending_proposals().into_iter().take(room).map(|p| (p.proposed_by_name.clone(), 0, format!("Proposes {}", p.name), None)));
        cards.extend(self.tasks.iter().filter(|t| ui::is_waiting(t)).take(4).map(|t| (t.employee_name.clone().unwrap_or_else(|| "Task".into()), t.updated_at_ms, t.title.clone(), Some(t.id))));
        v().gap(px(10.))
            .child(h().justify_between().child(ui::eyebrow(format!("WAITING FOR YOU · {}", self.needs_you_count()))).child(ui::hint("dash-inbox", "secondary-2", "Inbox →", || Box::new(OpenInbox), cx)))
            .when(cards.is_empty(), |el| el.child(div().text_size(px(12.5)).text_color(c(theme::MUTED)).child("Nothing is waiting for you.")))
            .children(cards.into_iter().enumerate().map(|(i, (who, at, text, task))| {
                v().id(("needs-you", i)).gap(px(4.)).px(px(16.)).py(px(12.)).rounded(px(10.)).border_1().border_color(c(theme::BORDER)).bg(c(theme::SURFACE)).cursor_pointer()
                    .hover(|s| s.border_color(c(theme::BORDER_STRONG)))
                    .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| match task {
                        Some(id) => this.open_panel(Panel::Task(id), w, cx),
                        None => this.go_nav(NavItem::Inbox, w, cx),
                    }))
                    .child(h().gap(px(6.)).text_size(px(13.)).child(div().font_weight(FontWeight::BOLD).child(who)).when(at > 0, |el| el.child(div().text_color(c(theme::MUTED)).child(format!("· {}", crate::screens::projects::clock(at))))))
                    .child(div().text_size(px(13.5)).line_height(px(20.)).line_clamp(3).child(text))
            }))
    }

    /// NOW: the tasks running, each with its employee and how long it has been at it.
    fn now_section(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let now = self.working_now();
        let at = ui::now_ms();
        v().gap(px(14.))
            .child(ui::eyebrow("NOW"))
            .when(now.is_empty(), |el| el.child(div().text_size(px(12.5)).text_color(c(theme::MUTED)).child("Nothing is running.")))
            .children(now.into_iter().enumerate().map(|(i, w)| {
                let id = w.manager;
                h().id(("now", i)).gap(px(12.)).items_start().cursor_pointer()
                    .on_click(cx.listener(move |this, _: &ClickEvent, win, cx| this.open_manager(ManagerRef::Real(id), win, cx)))
                    .child(div().mt(px(5.)).child(ui::pulse_dot(("now-dot", i), 8., theme::ACCENT)))
                    .child(
                        v().flex_1().min_w_0().gap(px(2.))
                            .child(h().gap(px(6.)).text_size(px(13.)).child(div().font_weight(FontWeight::BOLD).child(w.name))
                                .when(w.since_ms > 0, |el| el.child(div().text_color(c(theme::MUTED)).child(format!("· {}", crate::screens::projects::since_label(at - w.since_ms))))))
                            .child(div().text_size(px(13.)).line_height(px(19.)).text_color(c(theme::TEXT_2)).line_clamp(3).child(w.title)),
                    )
            }))
    }
}

#[cfg(test)]
mod tests {
    use super::{assignee_order, default_assignee, is_management, needs_you, roots};
    use crate::screens::org::tests::manager;
    use crate::screens::tasks::tests::task;
    use aurelius_domain::{DepartmentId, ProjectId, employee::LifecycleStatus, task::TaskStatus};

    #[test]
    fn management_is_the_root_and_its_direct_reports() {
        let p = DepartmentId::new();
        let ceo = manager(p, "Jean", None);
        let cto = manager(p, "Simon", Some(ceo.id));
        let dev = manager(p, "Dean", Some(cto.id));
        let all = [dev.clone(), cto.clone(), ceo.clone()];
        assert!(is_management(&all, ceo.id));
        assert!(is_management(&all, cto.id));
        assert!(!is_management(&all, dev.id), "two levels down is not management");
    }

    #[test]
    fn needs_you_counts_what_its_list_shows() {
        let p = ProjectId::new();
        let mut waiting = task(p, None, TaskStatus::Waiting, 1);
        assert_eq!(needs_you(1, 1, std::slice::from_ref(&waiting)), 3);
        // The task ended: nothing waits any more (unread results are not counted here).
        waiting.status = TaskStatus::Succeeded;
        assert_eq!(needs_you(0, 0, &[waiting, task(p, None, TaskStatus::Running, 2)]), 0);
    }

    #[test]
    fn the_single_root_is_the_default_assignee() {
        let p = DepartmentId::new();
        let ceo = manager(p, "Jean", None);
        let cto = manager(p, "Simon", Some(ceo.id));
        let dev = manager(p, "Dean", Some(cto.id));
        let all = vec![dev.clone(), cto.clone(), ceo.clone()];
        assert_eq!(default_assignee(&all, None), Some(ceo.id));
        assert_eq!(default_assignee(&all, Some(dev.id)), Some(ceo.id), "one root wins over the last choice");
    }

    #[test]
    fn several_roots_default_to_the_last_choice_else_the_first_root() {
        let p = DepartmentId::new();
        let ceo = manager(p, "Jean", None);
        let other = manager(DepartmentId::new(), "Tom", None);
        let dev = manager(p, "Dean", Some(ceo.id));
        let all = [ceo.clone(), other.clone(), dev.clone()];
        assert_eq!(default_assignee(&all, None), Some(ceo.id));
        assert_eq!(default_assignee(&all, Some(other.id)), Some(other.id));
        assert_eq!(default_assignee(&all, Some(dev.id)), Some(dev.id), "the last choice need not be a root");
        let mut paused = dev.clone();
        paused.status = LifecycleStatus::Paused;
        assert_eq!(default_assignee(&[ceo.clone(), other.clone(), paused], Some(dev.id)), Some(ceo.id), "a paused last choice falls back");
        assert_eq!(default_assignee(&[], Some(dev.id)), None);
    }

    #[test]
    fn a_paused_root_is_no_default_and_an_archived_boss_makes_a_root() {
        let p = DepartmentId::new();
        let mut paused = manager(p, "Jean", None);
        paused.status = LifecycleStatus::Paused;
        assert_eq!(default_assignee(&[paused], None), None);
        let mut gone = manager(p, "Gone", None);
        gone.status = LifecycleStatus::Archived;
        let left = manager(p, "Left", Some(gone.id));
        assert_eq!(default_assignee(&[gone, left.clone()], None), Some(left.id));
    }

    #[test]
    fn the_picker_lists_the_management_team_first_then_by_department() {
        let (a, b) = (DepartmentId::new(), DepartmentId::new());
        let ceo = manager(a, "Jean", None);
        let dev_b = manager(b, "Max", Some(ceo.id));
        let dev_a = manager(a, "Dean", Some(ceo.id));
        let lead_b = manager(b, "Tom", None);
        let mut archived = manager(a, "Old", Some(ceo.id));
        archived.status = LifecycleStatus::Archived;
        let all = vec![dev_b.clone(), dev_a.clone(), ceo.clone(), archived, lead_b.clone()];
        assert_eq!(roots(&all), vec![ceo.id, lead_b.id]);
        let order: Vec<String> = assignee_order(&all).iter().map(|m| m.name.clone()).collect();
        assert_eq!(order, ["Jean", "Tom", "Max", "Dean"]);
    }
}
