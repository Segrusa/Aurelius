//! The Tasks page: every task of every project, filtered by employee, department, project
//! and state; a task opens in the right panel (plan v0.9, phase 2).

use crate::app::{AppView, Overlay, DepartmentKey, MenuDo, MenuEntry, MenuItems, Panel, TaskChoice};
use crate::theme::{self, c};
use crate::ui::{self, h, v};
use aurelius_domain::{EmployeeId, ProjectId};
use aurelius_protocol::{EmployeeSummary, TaskView};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

/// Rows the Tasks page builds at a time: it re-renders on every scroll and hover frame,
/// and all 600+ tasks per frame kept a core busy. More load as the page nears its end.
pub const ROWS_STEP: usize = 100;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum TaskState {
    /// Not finished: drafts, queued, running, waiting.
    #[default]
    Open,
    /// Waiting for the user (input or an approval).
    NeedsYou,
    Done,
    All,
}

impl TaskState {
    pub const ALL: [TaskState; 4] = [TaskState::Open, TaskState::NeedsYou, TaskState::Done, TaskState::All];

    pub fn label(self) -> &'static str {
        match self {
            TaskState::Open => "Open",
            TaskState::NeedsYou => "Needs you",
            TaskState::Done => "Ended",
            TaskState::All => "All",
        }
    }

    fn admits(self, t: &TaskView) -> bool {
        match self {
            TaskState::Open => ui::is_active(t),
            TaskState::NeedsYou => ui::is_waiting(t) || t.pending_approvals > 0,
            TaskState::Done => !ui::is_active(t),
            TaskState::All => true,
        }
    }
}

/// The Tasks page's filters (`None`: any).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct TaskFilter {
    pub employee: Option<EmployeeId>,
    pub department: Option<DepartmentKey>,
    pub project: Option<ProjectId>,
    pub state: TaskState,
}

/// The tasks the filter lets through, newest change first. The department is the current
/// employee's (as `task.list {department_id}` reads it).
pub fn filter_tasks<'a>(tasks: &'a [TaskView], managers: &[EmployeeSummary], f: &TaskFilter) -> Vec<&'a TaskView> {
    let department_of = |t: &TaskView| t.employee_id.and_then(|id| managers.iter().find(|m| m.id == id)).and_then(|m| m.department_id);
    let mut out: Vec<&TaskView> = tasks
        .iter()
        .filter(|t| f.state.admits(t))
        .filter(|t| f.employee.is_none_or(|e| t.employee_id == Some(e)))
        .filter(|t| f.project.is_none_or(|p| t.project_id == p))
        .filter(|t| f.department.is_none_or(|d| department_of(t) == Some(d)))
        .collect();
    out.sort_by_key(|t| std::cmp::Reverse(t.updated_at_ms));
    out
}

/// The filtered tasks as a tree: a delegated task sits right under the task that delegated it
/// (oldest first, any depth). A task whose parent is not in `rows` stays top-level, in `rows`
/// order. Returns each task with its depth.
pub fn tree_order<'a>(rows: &[&'a TaskView]) -> Vec<(&'a TaskView, usize)> {
    use std::collections::{HashMap, HashSet};
    let present: HashSet<_> = rows.iter().map(|t| t.id).collect();
    let mut kids: HashMap<_, Vec<&'a TaskView>> = HashMap::new();
    let mut roots = Vec::new();
    for &t in rows {
        match t.parent_task_id.filter(|p| *p != t.id && present.contains(p)) {
            Some(p) => kids.entry(p).or_default().push(t),
            None => roots.push(t),
        }
    }
    for list in kids.values_mut() {
        list.sort_by_key(|t| t.created_at_ms);
    }
    let mut out = Vec::with_capacity(rows.len());
    let mut seen = HashSet::new();
    fn walk<'a>(t: &'a TaskView, depth: usize, kids: &HashMap<aurelius_domain::TaskId, Vec<&'a TaskView>>, seen: &mut HashSet<aurelius_domain::TaskId>, out: &mut Vec<(&'a TaskView, usize)>) {
        if !seen.insert(t.id) {
            return;
        }
        out.push((t, depth));
        for &k in kids.get(&t.id).into_iter().flatten() {
            walk(k, depth + 1, kids, seen, out);
        }
    }
    for t in roots {
        walk(t, 0, &kids, &mut seen, &mut out);
    }
    // A parent cycle has no root; keep those tasks visible anyway.
    for &t in rows {
        walk(t, 0, &kids, &mut seen, &mut out);
    }
    out
}

/// One visible row of the folded tree.
pub struct TreeRow<'a> {
    pub task: &'a TaskView,
    pub depth: usize,
    /// All tasks below this one (shown or not).
    pub below: usize,
    pub open: bool,
}

/// Hides what sits below a folded task. `tree` is in `tree_order`. A task with subtasks is open
/// when `folds` says so, else while it is not finished.
pub fn fold_tree<'a>(tree: &[(&'a TaskView, usize)], folds: &std::collections::HashMap<aurelius_domain::TaskId, bool>) -> Vec<TreeRow<'a>> {
    let mut out = Vec::with_capacity(tree.len());
    let mut hidden_from: Option<usize> = None;
    for (i, &(task, depth)) in tree.iter().enumerate() {
        if let Some(d) = hidden_from {
            if depth > d {
                continue;
            }
            hidden_from = None;
        }
        let below = tree[i + 1..].iter().take_while(|(_, d)| *d > depth).count();
        let open = below == 0 || folds.get(&task.id).copied().unwrap_or(!task.status.is_terminal());
        if !open {
            hidden_from = Some(depth);
        }
        out.push(TreeRow { task, depth, below, open });
    }
    out
}

impl AppView {
    fn filter_menu(&self, which: &'static str) -> MenuItems {
        let option = |label: String, choice: TaskChoice, checked: bool| Some(MenuEntry { label: label.into(), keys: None, run: MenuDo::Task(choice), danger: false, checked, detail: None });
        let f = self.tasks_filter;
        let mut items: MenuItems = Vec::new();
        match which {
            "tasks-employee" => {
                items.push(option("Anyone".into(), TaskChoice::FilterEmployee(None), f.employee.is_none()));
                items.push(None);
                items.extend(crate::screens::dashboard::assignee_order(&self.managers).into_iter().map(|m| option(m.name.clone(), TaskChoice::FilterEmployee(Some(m.id)), f.employee == Some(m.id))));
            }
            "tasks-department" => {
                items.push(option("Any department".into(), TaskChoice::FilterDepartment(None), f.department.is_none()));
                items.push(None);
                items.extend(self.department_list().into_iter().map(|d| option(d.name, TaskChoice::FilterDepartment(Some(d.key)), f.department == Some(d.key))));
            }
            _ => {
                items.push(option("Any project".into(), TaskChoice::FilterProject(None), f.project.is_none()));
                items.push(None);
                items.extend(self.projects.iter().map(|p| option(p.name.clone(), TaskChoice::FilterProject(Some(p.id)), f.project == Some(p.id))));
            }
        }
        items
    }

    pub fn render_all_tasks(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let serif = ui::serif(cx);
        let f = self.tasks_filter;
        let all = fold_tree(&tree_order(&filter_tasks(&self.tasks, &self.managers, &f)), &self.tasks_folds);
        let mut shown = if self.tasks_shown.0 == f { self.tasks_shown.1 } else { ROWS_STEP };
        let (offset, max) = (self.page_scroll.offset().y, self.page_scroll.max_offset().y);
        if shown < all.len() && max > px(0.) && -offset + px(800.) >= max {
            shown += ROWS_STEP;
            cx.notify();
        }
        self.tasks_shown = (f, shown);
        let rows = &all[..all.len().min(shown)];
        let open_task = match self.panel { Panel::Task(id) => Some(id), _ => None };
        let employee = f.employee.and_then(|id| self.managers.iter().find(|m| m.id == id)).map_or_else(|| "Anyone".to_string(), |m| m.name.clone());
        let department = f.department.map_or_else(|| "Any department".to_string(), |d| self.department_name(Some(d)));
        let project = f.project.and_then(|id| self.projects.iter().find(|p| p.id == id)).map_or_else(|| "Any project".to_string(), |p| p.name.clone());
        let picker = |id: &'static str, label: String, set: bool, cx: &mut Context<Self>| {
            let open = self.overlay == Overlay::Menu && self.menu.as_ref().is_some_and(|m| m.field == Some(id));
            ui::dropdown_trigger(id, 28., set, open).max_w(px(240.))
                .tooltip(ui::tip(label.clone(), None))
                .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| {
                    let items = this.filter_menu(id);
                    this.open_dropdown(id, items, w, cx);
                }))
                .child(div().min_w_0().truncate().child(label))
                .child(ui::caret())
                .child(ui::track_bounds(self.dropdown_bounds.clone(), id))
        };
        let states = h().gap(px(4.)).children(TaskState::ALL.into_iter().enumerate().map(|(i, s)| {
            let on = s == f.state;
            div().id(("tasks-state", i)).px(px(10.)).py(px(4.)).rounded(px(6.)).cursor_pointer().text_size(px(12.5))
                .text_color(c(if on { theme::TEXT } else { theme::MUTED }))
                .when(on, |el| el.bg(c(theme::SURFACE_2)))
                .hover(|st| st.bg(c(theme::SURFACE_2)))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.tasks_filter.state = s;
                    cx.notify();
                }))
                .child(s.label())
        }));
        let filters = h().gap(px(8.)).flex_wrap()
            .child(states)
            .child(div().w(px(1.)).h(px(20.)).bg(c(theme::BORDER)))
            .child(picker("tasks-employee", employee, f.employee.is_some(), cx))
            .child(picker("tasks-department", department, f.department.is_some(), cx))
            .child(picker("tasks-project", project, f.project.is_some(), cx));
        let list = v().rounded(px(9.)).border_1().border_color(c(theme::BORDER)).bg(c(theme::PANEL)).overflow_hidden()
            .children(rows.iter().enumerate().map(|(i, row)| {
                let (t, depth, below, open) = (row.task, row.depth, row.below, row.open);
                let id = t.id;
                let mark = self.task_mark(t);
                let project = self.projects.iter().find(|p| p.id == t.project_id).map_or_else(String::new, |p| p.name.clone());
                let who = t.employee_name.clone().unwrap_or_else(|| "Nobody yet (draft)".into());
                h().id(("all-task", i)).gap(px(12.)).pr(px(14.)).pl(px(14. + 20. * depth as f32)).py(px(9.)).cursor_pointer()
                    .when(i > 0, |el| el.border_t_1().border_color(c(theme::BORDER)))
                    .when(open_task == Some(id), |el| el.bg(c(theme::SURFACE_2)))
                    .hover(|s| s.bg(c(theme::SURFACE)))
                    .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.open_panel(Panel::Task(id), w, cx)))
                    .when(depth > 0, |el| el.child(div().flex_none().text_size(px(13.)).text_color(c(theme::MUTED)).child("↳")))
                    .when(below > 0, |el| {
                        el.child(
                            h().id(("all-task-fold", i)).flex_none().gap(px(4.)).items_center().cursor_pointer().text_color(c(theme::MUTED))
                                .hover(|s| s.text_color(c(theme::TEXT)))
                                .tooltip(ui::tip(if open { "Collapse subtasks" } else { "Expand subtasks" }, None))
                                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                    cx.stop_propagation();
                                    this.tasks_folds.insert(id, !open);
                                    cx.notify();
                                }))
                                .child(div().size(px(12.)).flex_none().child(ui::chevron(open, 12., theme::MUTED)))
                                .when(!open, |el| el.child(div().text_size(px(11.5)).child(below.to_string()))),
                        )
                    })
                    .child(
                        v().flex_1().min_w_0()
                            .child(div().truncate().text_size(px(13.5)).child(t.title.clone()))
                            .child(div().truncate().text_size(px(11.5)).text_color(c(theme::MUTED)).child(format!("{who} · {project}"))),
                    )
                    .child(div().flex_none().text_size(px(11.5)).text_color(c(theme::MUTED)).child(ui::fmt_when(t.updated_at_ms)))
                    .child(ui::task_mark(mark, ("all-task-mark", i)))
            }));
        let body = v().pt(px(40.)).pb(px(40.)).gap(px(14.))
            .child(div().font_family(serif).text_size(px(30.)).child("Tasks"))
            .child(filters)
            .when(rows.is_empty(), |el| el.child(div().py(px(10.)).text_color(c(theme::MUTED)).child("No tasks match these filters.")))
            .when(!rows.is_empty(), |el| el.child(list));
        self.page(body).into_any_element()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::{TaskFilter, TaskState, filter_tasks};
    use std::collections::HashMap;
    use crate::screens::org::tests::manager;
    use aurelius_domain::{DepartmentId, ProjectId, task::TaskStatus};

    pub(crate) fn task(project: ProjectId, manager: Option<aurelius_domain::EmployeeId>, status: TaskStatus, updated: i64) -> aurelius_protocol::TaskView {
        let mut t: aurelius_protocol::TaskView = serde_json::from_value(serde_json::json!({
            "id": aurelius_domain::TaskId::new(), "project_id": project, "employee_id": manager, "employee_name": null,
            "title": "Task", "goal": "", "criteria": [], "status": "queued", "waiting_reason": null,
            "created_at_ms": 0, "updated_at_ms": updated, "result": null, "depends_on": [], "pending_approvals": 0, "parent_task_id": null,
            "delegated_by": null, "children": [], "handoffs": [], "run": {}, "attachments": [], "schedule_id": null, "function_id": null,
            "conversation_id": null,
        }))
        .expect("a task view");
        t.status = status;
        t
    }

    #[test]
    fn filters_combine_and_the_newest_comes_first() {
        let (a, b) = (ProjectId::new(), ProjectId::new());
        let (servers, mobile) = (DepartmentId::new(), DepartmentId::new());
        let simon = manager(servers, "Simon", None);
        let max = manager(mobile, "Max", None);
        let managers = vec![simon.clone(), max.clone()];
        let tasks = vec![
            task(a, Some(simon.id), TaskStatus::Running, 1),
            task(a, Some(simon.id), TaskStatus::Succeeded, 2),
            task(b, Some(max.id), TaskStatus::Queued, 3),
            task(b, Some(simon.id), TaskStatus::Queued, 4),
        ];
        let open = filter_tasks(&tasks, &managers, &TaskFilter::default());
        assert_eq!(open.iter().map(|t| t.updated_at_ms).collect::<Vec<_>>(), [4, 3, 1]);
        let simons = filter_tasks(&tasks, &managers, &TaskFilter { employee: Some(simon.id), state: TaskState::All, ..Default::default() });
        assert_eq!(simons.len(), 3);
        let in_b = filter_tasks(&tasks, &managers, &TaskFilter { project: Some(b), ..Default::default() });
        assert_eq!(in_b.len(), 2);
        // Department: the employee's, wherever the task is.
        let dept_a = filter_tasks(&tasks, &managers, &TaskFilter { department: Some(servers), ..Default::default() });
        assert_eq!(dept_a.iter().map(|t| t.updated_at_ms).collect::<Vec<_>>(), [4, 1]);
        let ended = filter_tasks(&tasks, &managers, &TaskFilter { state: TaskState::Done, ..Default::default() });
        assert_eq!(ended.len(), 1);
    }

    #[test]
    fn a_finished_task_with_a_live_run_counts_as_open() {
        let p = ProjectId::new();
        let mut live = task(p, None, TaskStatus::Succeeded, 1);
        live.live_run = Some(aurelius_domain::views::LiveRun::Running);
        let tasks = vec![live, task(p, None, TaskStatus::Succeeded, 2)];
        let open = filter_tasks(&tasks, &[], &TaskFilter::default());
        assert_eq!(open.iter().map(|t| t.updated_at_ms).collect::<Vec<_>>(), [1]);
        let ended = filter_tasks(&tasks, &[], &TaskFilter { state: TaskState::Done, ..Default::default() });
        assert_eq!(ended.iter().map(|t| t.updated_at_ms).collect::<Vec<_>>(), [2]);
    }

    #[test]
    fn a_task_conversation_is_found_through_its_task() {
        let p = ProjectId::new();
        let mut a = task(p, None, TaskStatus::Running, 1);
        let mut b = task(p, None, TaskStatus::Running, 2);
        let (ca, cb) = (aurelius_domain::ConversationId::new(), aurelius_domain::ConversationId::new());
        a.conversation_id = Some(ca);
        b.conversation_id = Some(cb);
        let tasks = [a.clone(), b.clone()];
        assert_eq!(crate::app::task_of_conversation(&tasks, cb), Some(b.id));
        assert_eq!(crate::app::task_of_conversation(&tasks, ca), Some(a.id));
        assert_eq!(crate::app::task_of_conversation(&tasks, aurelius_domain::ConversationId::new()), None);
    }

    #[test]
    fn delegated_tasks_nest_under_their_parent_and_orphans_stay_top_level() {
        let p = ProjectId::new();
        let mut parent = task(p, None, TaskStatus::Running, 10);
        let mut child = task(p, None, TaskStatus::Running, 20);
        let mut grand = task(p, None, TaskStatus::Running, 5);
        let mut later = task(p, None, TaskStatus::Running, 30);
        parent.created_at_ms = 1;
        child.created_at_ms = 3;
        later.created_at_ms = 4;
        grand.created_at_ms = 5;
        child.parent_task_id = Some(parent.id);
        later.parent_task_id = Some(parent.id);
        grand.parent_task_id = Some(child.id);
        let all = [&later, &child, &parent, &grand];
        let tree = super::tree_order(&all);
        let shape: Vec<_> = tree.iter().map(|(t, d)| (t.id, *d)).collect();
        assert_eq!(shape, [(parent.id, 0), (child.id, 1), (grand.id, 2), (later.id, 1)]);
        // Parent filtered away: the child is top-level and keeps its own subtree.
        let tree = super::tree_order(&[&grand, &child]);
        assert_eq!(tree.iter().map(|(t, d)| (t.id, *d)).collect::<Vec<_>>(), [(child.id, 0), (grand.id, 1)]);
    }

    #[test]
    fn folded_trees_hide_their_subtasks_and_count_them() {
        let p = ProjectId::new();
        let mut root = task(p, None, TaskStatus::Succeeded, 1);
        let mut kid = task(p, None, TaskStatus::Succeeded, 2);
        let mut grand = task(p, None, TaskStatus::Running, 3);
        let other = task(p, None, TaskStatus::Running, 4);
        root.created_at_ms = 1;
        kid.created_at_ms = 2;
        grand.created_at_ms = 3;
        kid.parent_task_id = Some(root.id);
        grand.parent_task_id = Some(kid.id);
        let tree = super::tree_order(&[&root, &kid, &grand, &other]);
        // A finished tree starts folded; a running task without subtasks has nothing to fold.
        let rows = super::fold_tree(&tree, &HashMap::new());
        assert_eq!(rows.iter().map(|r| (r.depth, r.below, r.open)).collect::<Vec<_>>(), [(0, 2, false), (0, 0, true)]);
        // Open the root: its child shows, still folded (finished) with one task below.
        let rows = super::fold_tree(&tree, &HashMap::from([(root.id, true)]));
        assert_eq!(rows.iter().map(|r| (r.depth, r.below, r.open)).collect::<Vec<_>>(), [(0, 2, true), (1, 1, false), (0, 0, true)]);
        // Fold a running parent by hand.
        let rows = super::fold_tree(&tree, &HashMap::from([(root.id, true), (kid.id, true)]));
        assert_eq!(rows.len(), 4);
    }
}
