//! The Projects page (list), the project page and its tabs (Overview, Tasks, Ideas; the
//! memory list opens from the project menu), and the page dispatcher.

use crate::app::{AppView, EmployeesTab, Overlay, ManagerRef, MenuDo, MenuEntry, MenuItems, ProjectRef, ProjectsTab, Page, Tab, TaskChoice, WorkingNow};
use crate::keymap::*;
use aurelius_domain::employee::LifecycleStatus;
use crate::preview::{self, Marker, Step};
use crate::theme::{self, c};
use crate::resize::Edge;
use crate::ui::{self, ButtonKind, h, v};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use gpui_kit::component::scroll::ScrollableElement as _;

/// Manager monograms shown on a project row before the "+n".
const ROW_MANAGERS: usize = 4;
/// Idea rows built at a time (see `render_ideas`).
pub const IDEAS_STEP: usize = 100;

/// The managers cell of a project row: monograms (a pulse ring when working), "+n" and a count.
fn row_managers(names: &[(String, bool)], key: usize, cx: &App) -> Div {
    let working = names.iter().filter(|(_, w)| *w).count();
    let hidden = names.len().saturating_sub(ROW_MANAGERS);
    let label = match (names.len(), working) {
        (0, _) => "Nobody yet".to_string(),
        (n, 0) => plural(n, "employee", "employees"),
        (n, w) => format!("{} · {w} working", plural(n, "employee", "employees")),
    };
    // Shrinks before the project name does; the label truncates.
    h().w(px(230.)).min_w_0().gap(px(8.)).text_size(px(12.)).text_color(c(theme::MUTED))
        .child(h().flex_none().gap(px(3.)).children(names.iter().take(ROW_MANAGERS).enumerate().map(|(n, (name, w))| {
            div().relative().child(ui::monogram(name, 18., cx))
                .when(*w, |el| el.child(div().absolute().right(px(-2.)).bottom(px(-2.)).child(ui::pulse_dot(("row-pulse", key * 100 + n), 5., theme::ACCENT))))
        })))
        .when(hidden > 0, |el| el.child(div().flex_none().child(format!("+{hidden}"))))
        .child(div().min_w_0().truncate().child(label))
}

/// Under a lesson (feature `task_lessons`): when it applies, how often it was
/// confirmed, and the signals of the task it was learned from (S1..S8).
fn lesson_lines(lesson: &aurelius_domain::memory::LessonLocator) -> Div {
    let when = match lesson.reinforced {
        0 => format!("Applies when {}", lesson.applies_when),
        n => format!("Applies when {} · confirmed {n}×", lesson.applies_when),
    };
    v().gap(px(3.)).text_size(px(12.)).line_height(px(17.))
        .child(div().text_color(c(theme::TEXT_2)).child(when))
        .children(lesson.evidence.iter().map(|e| {
            h().gap(px(6.)).items_start().text_color(c(theme::MUTED))
                .child(div().flex_none().text_color(c(theme::ACCENT_TEXT)).child(e.label.clone()))
                .child(div().flex_1().min_w_0().line_clamp(2).child(e.text.clone()))
        }))
}

/// Local "HH:MM" for a timestamp in ms.
pub fn clock(ms: i64) -> String {
    let local = ms / 1000 + aurelius_host::utc_offset_minutes_at(ms) * 60;
    let minutes = local.rem_euclid(86_400) / 60;
    format!("{:02}:{:02}", minutes / 60, minutes % 60)
}

/// Who noted an idea: "Idea from <manager>" with its monogram, or "You" in the accent colour.
fn idea_author(idea: &aurelius_protocol::IdeaView, cx: &App) -> AnyElement {
    if idea.by_employee() {
        h().gap(px(5.)).text_color(c(theme::TEXT_2))
            .child(ui::monogram(&idea.author, 16., cx))
            .child(format!("Idea from {}", idea.author))
            .into_any_element()
    } else {
        div().text_color(c(theme::ACCENT_TEXT)).child("You").into_any_element()
    }
}

/// The coloured bar down the left of a manager's idea.
fn manager_bar(inset: f32) -> Div {
    div().absolute().left(px(inset)).top(px(inset + 2.)).bottom(px(inset + 2.)).w(px(3.)).rounded(px(2.)).bg(c(theme::MANAGER_MARK))
}

/// "Bug" in red, "Feature" muted.
fn kind_badge(kind: aurelius_domain::idea::IdeaKind) -> Div {
    let bug = kind == aurelius_domain::idea::IdeaKind::Bug;
    div().px(px(6.)).rounded(px(4.)).border_1().text_size(px(10.5))
        .border_color(c(if bug { theme::NEGATIVE } else { theme::BORDER }))
        .text_color(c(if bug { theme::NEGATIVE } else { theme::MUTED }))
        .child(if bug { "Bug" } else { "Feature" })
}

/// Header above a group of ideas ("FROM YOU · 2").
fn ideas_group_header(label: &str, count: usize) -> Div {
    ui::eyebrow(format!("{label} · {count}"))
}

impl AppView {
    /// The page the sidebar picked.
    pub fn render_company(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let wide = f32::from(window.viewport_size().width) >= crate::screens::activity::PREVIEW_MIN_WINDOW;
        match self.screen {
            Page::Dashboard => {
                let dashboard = self.render_dashboard(wide, cx);
                self.with_preview(dashboard, wide, cx)
            }
            Page::Projects => self.render_projects_page(cx),
            Page::Project(project, tab) => self.render_project(project, tab, window, cx).into_any_element(),
            Page::Employee(manager) => self.render_manager(manager, window, cx).into_any_element(),
            Page::Employees => self.render_employees_page(cx),
            Page::Department(key) => self.render_department(key, cx),
            Page::Tasks => self.render_all_tasks(cx),
            Page::Inbox => self.render_inbox_page(cx),
            Page::Settings => self.render_settings_page(window, cx),
        }
    }

    pub(crate) fn page(&self, content: impl IntoElement) -> Div {
        ui::scroll_frame(&self.page_scroll, div()
            .id("page")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.page_scroll)
            .track_focus(&self.focus)
            .key_context(NAV)
            .flex()
            .flex_col()
            .items_center()
            .px(px(if self.panel_shown() { 24. } else { 60. }))
            .child(ui::rise(div().flex_none().w_full().max_w(px(1060.)).child(content), ("page", self.page_key()), 0)))
            .flex_1()
    }

    // ── Pages with tabs: Employees and Projects ─────────────────────────

    /// A row of tabs on top of a page; `on` switches to tab `i`.
    fn page_tab_bar(&self, id: &'static str, labels: &[&'static str], active: usize, on: fn(&mut AppView, usize, &mut Context<AppView>), cx: &mut Context<Self>) -> Stateful<Div> {
        h().id(id).flex_none().w_full().h(px(48.)).px(px(22.)).gap(px(2.)).border_b_1().border_color(c(theme::BORDER))
            .children(labels.iter().enumerate().map(|(i, label)| {
                let on_tab = i == active;
                h().id((id, i)).role(Role::Tab).aria_label(*label).flex_none().px(px(12.)).h(px(30.)).rounded(px(7.)).cursor_pointer().text_size(px(13.))
                    .text_color(c(if on_tab { theme::TEXT } else { theme::MUTED }))
                    .when(on_tab, |el| el.bg(c(theme::SURFACE_2)).font_weight(FontWeight::MEDIUM))
                    .hover(|s| s.text_color(c(theme::TEXT)).bg(c(theme::SURFACE)))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| on(this, i, cx)))
                    .child(*label)
            }))
    }

    /// Employees: the list, the org chart and the departments as tabs (Ctrl+Tab and h / l switch).
    fn render_employees_page(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let labels = EmployeesTab::ALL.map(EmployeesTab::label);
        let active = EmployeesTab::ALL.iter().position(|t| *t == self.employees_tab).unwrap_or(0);
        let bar = self.page_tab_bar("employees-tabs", &labels, active, |this, i, cx| this.set_employees_tab(EmployeesTab::ALL[i], cx), cx);
        let content = match self.employees_tab {
            EmployeesTab::List => self.render_employees(cx),
            EmployeesTab::OrgChart => self.render_company_org(cx),
            EmployeesTab::Departments => self.render_departments(cx),
        };
        v().flex_1().min_h_0().w_full().child(bar).child(content).into_any_element()
    }

    /// Projects: the projects and the ideas of every project as tabs.
    fn render_projects_page(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let labels = ProjectsTab::ALL.map(ProjectsTab::label);
        let active = ProjectsTab::ALL.iter().position(|t| *t == self.projects_tab).unwrap_or(0);
        let bar = self.page_tab_bar("projects-tabs", &labels, active, |this, i, cx| this.set_projects_tab(ProjectsTab::ALL[i], cx), cx);
        let content = match self.projects_tab {
            ProjectsTab::Projects => self.render_home(cx).into_any_element(),
            ProjectsTab::Ideas => self.render_ideas_page(cx),
        };
        v().flex_1().min_h_0().w_full().child(bar).child(content).into_any_element()
    }

    // ── 02 Company home ─────────────────────────────────────────────────

    /// `content` with the manager preview pane on its right, when one is selected and the window is wide enough.
    pub(crate) fn with_preview(&mut self, content: AnyElement, wide: bool, cx: &mut Context<Self>) -> AnyElement {
        let Some(manager) = self.preview_manager().filter(|_| wide) else { return content };
        let pane = self.render_manager_preview(manager, cx);
        h().flex_1().min_h_0().w_full().items_stretch()
            .child(content)
            .child(self.resizable("activity_preview", 400., Edge::Left, pane, cx))
            .into_any_element()
    }

    /// The Projects page: one row per project (Ctrl+click: its own window), pin to the sidebar.
    fn render_home(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let pk = self.page_key();
        let serif = ui::serif(cx);
        let projects = self.home_projects();

        let rows = projects.iter().enumerate().map(|(i, project)| {
            let selected = i == self.selected;
            let project = *project;
            let row = h()
                .id(("project", i))
                .w_full()
                .h(px(48.))
                .rounded(px(8.))
                .px(px(14.))
                .gap(px(14.))
                .cursor_pointer()
                .bg(c(if selected { theme::SURFACE } else { theme::PANEL }))
                .border_1()
                .border_color(c(if selected { theme::FOCUS_RING } else { theme::BORDER }))
                .hover(|s| s.bg(c(theme::SURFACE)).border_color(c(theme::BORDER_STRONG)))
                .on_click(cx.listener(move |this, e: &ClickEvent, w, cx| match project {
                    // Ctrl+click (Cmd on macOS) opens the project in its own window.
                    ProjectRef::Real(id) if e.modifiers().secondary() => this.open_project_window(id, cx),
                    _ => this.open_project(project, w, cx),
                }))
                .when_some(match project { ProjectRef::Real(id) => Some(id), ProjectRef::Sample(_) => None }, |el, id| {
                    el.on_aux_click(cx.listener(move |this, e: &ClickEvent, _, cx| {
                        if e.is_middle_click() {
                            this.open_project_window(id, cx);
                        }
                    }))
                })
                .when(matches!(project, ProjectRef::Real(_)), |el| {
                    el.on_mouse_down(MouseButton::Right, cx.listener(move |this, e: &MouseDownEvent, w, cx| {
                        this.selected = i;
                        let items = this.project_menu();
                        this.open_menu(e.position, items, w, cx);
                    }))
                });
            // Columns: name and one-line summary · managers · counts · status.
            let name_cell = |name: SharedString, summary: SharedString, tag: Option<Div>| {
                h().flex_1().min_w(px(160.)).gap(px(12.)).items_baseline()
                    .child(div().min_w_0().max_w(px(260.)).font_family(serif.clone()).text_size(px(16.)).truncate().child(name))
                    .when_some(tag, |el, t| el.child(t.flex_none()))
                    .child(div().flex_1().min_w_0().text_size(px(12.)).text_color(c(theme::MUTED)).truncate().child(summary))
            };
            let counts_cell = |waiting: String, attention: bool, active: u32| {
                h().w(px(250.)).min_w_0().justify_end().gap(px(12.)).text_size(px(11.5))
                    .child(div().min_w_0().truncate().text_color(c(if attention { theme::ACCENT_TEXT } else { theme::MUTED })).child(waiting))
                    .when(active > 0, |el| el.child(div().min_w_0().truncate().text_color(c(theme::TEXT_2)).child(format!("{active} active {}", if active == 1 { "task" } else { "tasks" }))))
            };
            match project {
                ProjectRef::Sample(_) => {
                    let s = self.sample(project).expect("sample");
                    let attention = s.approvals > 0 && !self.approval_done;
                    let managers: Vec<(String, bool)> = s.managers.iter().map(|m| (m.name.to_string(), m.working)).collect();
                    let badge = if attention {
                        h().gap(px(6.)).text_color(c(theme::ACCENT_TEXT)).child(ui::diamond(8., theme::ACCENT_TEXT, false)).child(format!("Needs you · {}", s.approvals))
                    } else if s.active_tasks > 0 {
                        h().gap(px(6.)).text_color(c(theme::TEXT_2)).child(ui::dot(6., theme::ACCENT)).child("Working")
                    } else {
                        h().gap(px(6.)).text_color(c(theme::MUTED)).child(ui::ring(6., theme::MUTED)).child("Quiet")
                    };
                    row.child(name_cell(s.name.into(), s.kind.into(), Some(ui::preview_tag(cx))))
                        .child(row_managers(&managers, i, cx))
                        .child(counts_cell(if attention { format!("Needs you · {} approval", s.approvals) } else { "Nothing waiting".into() }, attention, s.active_tasks as u32))
                        .child(badge.flex_none().w(px(110.)).text_size(px(12.)))
                }
                ProjectRef::Real(id) => {
                    let p = self.real(project).cloned();
                    let (name, description, paused) = p
                        .map(|p| (p.name, p.description.unwrap_or_default(), p.status == aurelius_domain::ProjectStatus::Paused))
                        .unwrap_or_default();
                    // Who works here: employees with open tasks in the project.
                    let managers: Vec<(String, bool)> = self.managers.iter()
                        .filter(|m| m.projects_active.contains(&id) && m.status != LifecycleStatus::Archived)
                        .map(|m| (m.name.clone(), m.working))
                        .collect();
                    let working = managers.iter().filter(|(_, w)| *w).count();
                    let needs = self.waiting_in(id);
                    let active = self.active_task_count(id);
                    let badge = if paused {
                        h().gap(px(6.)).text_color(c(theme::MUTED)).child(ui::ring(6., theme::MUTED)).child("Paused")
                    } else if needs.total() > 0 {
                        h().gap(px(6.)).text_color(c(theme::ACCENT_TEXT)).child(ui::diamond(8., theme::ACCENT_TEXT, false)).child(format!("Needs you · {}", needs.total()))
                    } else if working > 0 {
                        h().gap(px(6.)).text_color(c(theme::TEXT_2)).child(ui::pulse_dot(("project-pulse", i), 6., theme::ACCENT)).child("Working")
                    } else {
                        h().gap(px(6.)).text_color(c(theme::MUTED)).child(ui::ring(6., theme::MUTED)).child("Quiet")
                    };
                    let waiting_text = match (needs.approvals, needs.decisions) {
                        (0, 0) => "Nothing waiting".to_string(),
                        (a, 0) => counted(a, "approval", "approvals"),
                        (0, d) => counted(d, "decision", "decisions"),
                        (a, d) => format!("{}, {}", counted(a, "approval", "approvals"), counted(d, "decision", "decisions")),
                    };
                    let pinned = self.pins.contains(&id);
                    row.child(name_cell(name.into(), if description.is_empty() { "No description".into() } else { description.into() }, None))
                        .child(row_managers(&managers, i, cx))
                        .child(counts_cell(waiting_text, needs.total() > 0, active as u32))
                        .child(
                            h().id(("project-pin", i)).flex_none().size(px(24.)).justify_center().rounded(px(5.)).cursor_pointer()
                                .hover(|s| s.bg(c(theme::BORDER)))
                                .tooltip(ui::tip(if pinned { "Unpin from the sidebar" } else { "Pin to the sidebar" }, None))
                                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                    cx.stop_propagation();
                                    this.toggle_pin(id, cx);
                                }))
                                .child(ui::pin(pinned, 13., if pinned { theme::ACCENT_TEXT } else { theme::MUTED })),
                        )
                        .child(
                            h().flex_none().w(px(140.)).justify_between().text_size(px(12.))
                                .child(badge)
                                .child(ui::more_button(("project-more", i)).on_click(cx.listener(move |this, e: &ClickEvent, w, cx| {
                                    cx.stop_propagation();
                                    this.selected = i;
                                    let items = this.project_menu();
                                    this.open_menu(e.position(), items, w, cx);
                                }))),
                        )
                }
            }
        });

        let new_row = h()
            .id("new-project-row")
            .w_full()
            .h(px(48.))
            .rounded(px(8.))
            .border_1()
            .border_dashed()
            .border_color(c(theme::BORDER))
            .px(px(14.))
            .gap(px(12.))
            .cursor_pointer()
            .hover(|s| s.border_color(c(theme::BORDER_STRONG)))
            .on_click(cx.listener(|_, _: &ClickEvent, w, cx| w.dispatch_action(Box::new(NewProject), cx)))
            .child(div().flex_none().font_family(serif.clone()).italic().text_size(px(15.)).text_color(c(theme::TEXT_2)).child("Begin with what is in your control."))
            .child(div().flex_1().min_w_0().truncate().text_size(px(12.5)).text_color(c(theme::MUTED)).child("Create a project to give an employee a mandate."))
            .child(ui::kbd("secondary-shift-n", cx));

        let rows: Vec<AnyElement> = rows
            .chain(std::iter::once(new_row))
            .enumerate()
            .map(|(n, r)| ui::rise(r, ("home-row", pk.wrapping_add(n)), n).into_any_element())
            .collect();

        self.page(
            v().pt(px(40.))
                .pb(px(40.))
                .child(
                    h().justify_between()
                        .mb(px(18.))
                        .child(div().font_family(serif.clone()).text_size(px(30.)).child("Projects"))
                        .child(
                            ui::button("new-project", "New project", ButtonKind::Outline, Some(ui::kbd("secondary-shift-n", cx)))
                                .h(px(30.))
                                .on_click(cx.listener(|_, _: &ClickEvent, w, cx| w.dispatch_action(Box::new(NewProject), cx))),
                        ),
                )
                .child(v().gap(px(6.)).children(rows))
                .child(
                    h().mt(px(28.)).gap(px(16.)).text_size(px(11.5)).text_color(c(theme::MUTED))
                        .child(ui::key_hint(&["down", "up"], "select project", cx))
                        .child(div().child("Ctrl+click: its own window"))
                        .child(ui::key_hint(&["enter"], "open", cx))
                        .child(ui::key_hint(&["f2"], "rename", cx)),
                ),
        )
    }

    /// "AT WORK NOW": every manager working right now, across all projects.
    /// `wide`: a click selects the row and its manager shows in the preview pane
    /// (a second click or Enter opens it); narrow, a click opens the manager.
    pub(crate) fn render_at_work(&self, now: &[WorkingNow], wide: bool, cx: &mut Context<Self>) -> Option<Div> {
        if now.is_empty() {
            return None;
        }
        let at = ui::now_ms();
        let rows = now.iter().enumerate().map(|(n, w)| {
            let manager = ManagerRef::Real(w.manager);
            let selected = self.at_work_selected == Some(n) && !self.preview_closed;
            h().id(("at-work", n))
                .gap(px(12.))
                .px(px(14.))
                .py(px(9.))
                .cursor_pointer()
                .when(n > 0, |el| el.border_t_1().border_color(c(theme::BORDER)))
                .when(selected, |el| el.bg(c(theme::SURFACE)))
                .hover(|s| s.bg(c(theme::SURFACE)))
                .on_click(cx.listener(move |this, _: &ClickEvent, win, cx| {
                    if wide && (this.at_work_selected != Some(n) || this.preview_closed) {
                        this.at_work_selected = Some(n);
                        this.preview_closed = false;
                        cx.notify();
                    } else {
                        this.open_manager(manager, win, cx);
                    }
                }))
                .child(ui::pulse_dot(("at-work-pulse", n), 6., theme::ACCENT))
                .child(ui::monogram(&w.name, 22., cx))
                .child(
                    h().flex_none().w(px(280.)).gap(px(8.)).text_size(px(13.))
                        .child(div().flex_none().max_w(px(220.)).truncate().child(w.name.clone()))
                        .child(div().min_w_0().truncate().text_size(px(12.)).text_color(c(theme::MUTED)).child(format!("· {}", w.project_name))),
                )
                .child(div().flex_1().min_w_0().truncate().text_size(px(12.5)).text_color(c(theme::TEXT_2)).child(w.title.clone()))
                .when(w.since_ms > 0, |el| el.child(div().flex_none().text_size(px(11.5)).text_color(c(theme::MUTED)).child(since_label(at - w.since_ms))))
        });
        Some(
            v().mt(px(28.))
                .gap(px(10.))
                .child(ui::eyebrow(format!("AT WORK NOW · {}", now.len())))
                .child(v().rounded(px(9.)).border_1().border_color(c(theme::BORDER)).bg(c(theme::PANEL)).overflow_hidden().children(rows)),
        )
    }

    // ── Project header + tabs ──────────────────────────────────────────

    /// One bar on top of every project page: the project's name, the tabs
    /// beside it, and the project folder and its menu (⋯) on the right.
    pub fn project_bar(&mut self, project: ProjectRef, tab: Tab, cx: &mut Context<Self>) -> AnyElement {
        let serif = ui::serif(cx);
        let name: SharedString = match project {
            ProjectRef::Sample(i) => preview::PROJECTS[i].name.into(),
            ProjectRef::Real(_) => self.real(project).map(|p| p.name.clone()).unwrap_or_else(|| "Project".into()).into(),
        };
        let is_sample = matches!(project, ProjectRef::Sample(_));
        let folder = match project {
            ProjectRef::Real(_) => self.real(project).and_then(|p| p.folder.clone()),
            ProjectRef::Sample(_) => None,
        };
        // Narrow windows: the tabs scroll sideways (wheel too) so the buttons on the right stay visible.
        let tabs = h().id("project-tabs").flex_1().min_w_0().overflow_x_scroll().gap(px(2.)).children(Tab::ALL.iter().enumerate().map(|(i, t)| {
            let active = *t == tab;
            let t = *t;
            h().id(("tab", i))
                .flex_none()
                .gap(px(6.))
                .px(px(11.))
                .h(px(30.))
                .rounded(px(7.))
                .cursor_pointer()
                .text_size(px(13.))
                .text_color(c(if active { theme::TEXT } else { theme::MUTED }))
                .when(active, |el| el.bg(c(theme::SURFACE_2)).font_weight(FontWeight::MEDIUM))
                .hover(|s| s.text_color(c(theme::TEXT)).bg(c(theme::SURFACE)))
                .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.set_tab(t, w, cx)))
                .child(t.title())
        }))
        // The memory list is no tab: it opens from the project menu, so say where we are.
        .when(tab == Tab::Memory, |el| el.child(div().flex_none().px(px(11.)).h(px(30.)).flex().items_center().text_size(px(13.)).font_weight(FontWeight::MEDIUM).text_color(c(theme::TEXT)).child(Tab::Memory.title())));
        h().id("project-bar")
            .flex_none()
            .w_full()
            .h(px(56.))
            .px(px(22.))
            .gap(px(16.))
            .border_b_1()
            .border_color(c(theme::BORDER))
            .child(
                h().flex_none().gap(px(6.)).max_w(px(316.))
                    // A project window has only this project: nothing to go back to.
                    .when(self.window_project.is_none(), |el| el.child(
                        ui::icon_button("project-back")
                            .aria_label("Back to projects")
                            .tooltip(ui::tip("Back to projects", None))
                            .rounded(px(7.))
                            .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| {
                                this.navigate(Page::Projects);
                                // Land on the project just left, so Enter reopens it.
                                this.selected = this.home_projects().iter().position(|p| *p == project).unwrap_or(0);
                                this.focus.focus(w, cx);
                                cx.notify();
                            }))
                            .child(ui::nav_arrow(false, 18., theme::MUTED)),
                    ))
                    .child(div().min_w_0().truncate().font_family(serif).text_size(px(17.)).child(name))
                    .when(is_sample, |el| el.child(ui::preview_tag(cx))),
            )
            .child(div().flex_none().w(px(1.)).h(px(22.)).bg(c(theme::BORDER)))
            .child(tabs)
            .children(folder.map(|f| {
                ui::icon_button("open-folder")
                    .flex_none()
                    .h(px(26.))
                    .tooltip(ui::tip(f.clone(), Some("secondary-shift-o")))
                    .aria_label(f)
                    .on_click(cx.listener(|_, _: &ClickEvent, w, cx| w.dispatch_action(Box::new(OpenProjectFolder), cx)))
                    .child(ui::folder_glyph(theme::MUTED))
            }))
            .when(!is_sample, |el| el.child(ui::more_button("project-menu").flex_none().h(px(26.)).on_click(cx.listener(|this, e: &ClickEvent, w, cx| {
                let items = this.project_menu();
                this.open_menu(e.position(), items, w, cx);
            }))))
            .into_any_element()
    }

    fn render_project(&mut self, project: ProjectRef, tab: Tab, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_sample = matches!(project, ProjectRef::Sample(_));
        let bar = self.project_bar(project, tab, cx);
        // The Tasks list scrolls beside its detail; its top is just a little room now.
        let header = v().pt(px(18.));
        let content: AnyElement = match (tab, is_sample) {
            (Tab::Overview, _) => {
                let about: Option<String> = match project {
                    ProjectRef::Sample(i) => Some(preview::PROJECTS[i].description).filter(|d| !d.is_empty()).map(str::to_owned),
                    ProjectRef::Real(_) => self.real(project).and_then(|p| p.description.clone()).filter(|d| !d.trim().is_empty()),
                };
                v().children(about.map(|d| div().pt(px(22.)).text_size(px(14.)).text_color(c(theme::TEXT_2)).child(d)))
                    .child(self.render_overview(project, cx))
                    .into_any_element()
            }
            (Tab::Tasks, true) => {
                let tasks = self.render_tasks(header, window, cx).into_any_element();
                return v().flex_1().min_h_0().w_full().child(bar).child(h().flex_1().min_h_0().min_w_0().items_stretch().child(tasks)).into_any_element();
            }
            (Tab::Tasks, false) => {
                let tasks = self.render_real_tasks(header, project, cx).into_any_element();
                return v().flex_1().min_h_0().w_full().child(bar).child(h().flex_1().min_h_0().min_w_0().items_stretch().child(tasks)).into_any_element();
            }
            (Tab::Ideas, _) => match project {
                ProjectRef::Real(id) => self.render_ideas(Some(id), cx).into_any_element(),
                ProjectRef::Sample(_) => div().mt(px(24.)).text_color(c(theme::MUTED)).child("Ideas of your own projects show here.").into_any_element(),
            },
            (Tab::Memory, true) => div().mt(px(24.)).text_color(c(theme::MUTED)).child("The memory of your own projects shows here.").into_any_element(),
            (Tab::Memory, false) => match project {
                ProjectRef::Real(id) => self.render_real_memory(id, cx).into_any_element(),
                ProjectRef::Sample(_) => unreachable!(),
            },
        };

        let page = self.page(v().pt(px(4.)).pb(px(40.)).child(content)).into_any_element();
        let wide = f32::from(window.viewport_size().width) >= crate::screens::activity::PREVIEW_MIN_WINDOW;
        let page = self.with_preview(page, wide, cx);
        v().flex_1().min_h_0().w_full().child(bar).child(page).into_any_element()
    }

    fn render_overview(&mut self, project: ProjectRef, cx: &mut Context<Self>) -> impl IntoElement {
        let mono = ui::mono(cx);
        let show_approval = self.sample(project).is_some_and(|s| s.approvals > 0) && !self.approval_done;
        let paused = self.real(project).is_some_and(|p| p.status == aurelius_domain::ProjectStatus::Paused);
        v().when(paused, |el| {
            el.child(
                h().mt(px(24.)).gap(px(10.)).px(px(16.)).py(px(12.)).rounded(px(8.)).border_1().border_color(c(theme::BORDER_STRONG))
                    .text_color(c(theme::TEXT_2))
                    .child("This project is paused. No new automatic work starts.")
                    .child(div().flex_1())
                    .child(ui::hint("resume-project", "p", "Resume", || Box::new(TogglePause), cx)),
            )
        })
        .when(show_approval, |el| {
            el.child(ui::eyebrow("NEEDS YOUR ATTENTION").mt(px(30.)).mb(px(12.))).child(
                h().gap(px(18.))
                    .px(px(20.))
                    .py(px(18.))
                    .rounded(px(10.))
                    .border_1()
                    .border_color(c(theme::ATTENTION_BORDER))
                    .bg(c(theme::ATTENTION_BG))
                    .child(div().px(px(4.)).child(ui::diamond(12., theme::ACCENT_TEXT, false)))
                    .child(
                        v().flex_1()
                            .gap(px(4.))
                            .child(div().text_size(px(12.5)).text_color(c(theme::TEXT_2)).child(preview::APPROVAL.requester))
                            .child(
                                h().text_size(px(15.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(preview::APPROVAL.title)
                                    .child(div().font_family(mono.clone()).text_size(px(13.)).child(preview::APPROVAL.code)),
                            )
                            .child(div().text_size(px(12.5)).text_color(c(theme::MUTED)).child(preview::APPROVAL.detail)),
                    )
                    .child(
                        ui::button("review", "Review", ButtonKind::Primary, Some(ui::kbd("r", cx)))
                            .on_click(cx.listener(|_, _: &ClickEvent, w, cx| w.dispatch_action(Box::new(ReviewApproval), cx))),
                    ),
            )
        })
        .map(|el| match project {
            ProjectRef::Real(id) => el.child(self.overview_sections(id, cx)),
            ProjectRef::Sample(_) => el,
        })
    }

    /// Overview of your own project, as cards: who works here (read-only) and the
    /// open tasks beside the open ideas. Each row opens its item; "All" opens the tab.
    fn overview_sections(&mut self, project: aurelius_domain::ProjectId, cx: &mut Context<Self>) -> AnyElement {
        use aurelius_domain::idea::IdeaStatus;
        use aurelius_domain::task::TaskStatus;
        self.ensure_ideas(Some(project));
        let serif = ui::serif(cx);

        let card = || v().w_full().px(px(18.)).pt(px(14.)).pb(px(12.)).rounded(px(10.)).bg(c(theme::SURFACE)).border_1().border_color(c(theme::BORDER));
        let section = |id: &'static str, title: String, tab: Tab, cx: &mut Context<Self>| {
            h().mb(px(8.)).justify_between()
                .child(ui::eyebrow(title))
                .child(
                    h().id(id).gap(px(6.)).cursor_pointer().text_size(px(12.)).text_color(c(theme::ACCENT))
                        .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.set_tab(tab, w, cx)))
                        .child("All →"),
                )
        };
        let empty = |text: &'static str| div().py(px(8.)).text_size(px(13.)).text_color(c(theme::MUTED)).child(text);

        // Who works here: read-only, derived from the project's tasks; a row opens the employee.
        let managers = self.project_managers(ProjectRef::Real(project));
        let managers_el = card()
            .child(
                h().mb(px(8.)).justify_between()
                    .child(ui::eyebrow(if managers.is_empty() { "WHO WORKS HERE".to_string() } else { format!("WHO WORKS HERE · {}", managers.len()) })),
            )
            .when(managers.is_empty(), |el| el.child(empty("Nobody works here yet. Give a task to an employee.")))
            .children(managers.into_iter().enumerate().map(|(i, m)| {
                let key = m.key;
                let paused = m.status == aurelius_domain::employee::LifecycleStatus::Paused;
                h().id(("ov-manager", i)).gap(px(10.)).px(px(8.)).py(px(6.)).rounded(px(7.)).cursor_pointer()
                    .hover(|s| s.bg(c(theme::SURFACE_2)))
                    .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.open_manager(key, w, cx)))
                    .child(ui::monogram(&m.name, 28., cx))
                    .child(
                        v().flex_1().min_w_0()
                            .child(div().text_size(px(13.5)).font_weight(FontWeight::MEDIUM).truncate().child(m.name.clone()))
                            .child(div().text_size(px(11.5)).text_color(c(theme::TEXT_2)).truncate().child(m.role.clone())),
                    )
                    .child(if paused {
                        h().flex_none().gap(px(6.)).text_size(px(11.5)).text_color(c(theme::MUTED)).child(ui::ring(5., theme::MUTED)).child("Paused")
                    } else if m.working {
                        h().flex_none().gap(px(6.)).text_size(px(11.5)).child(ui::pulse_dot(("ov-mgr-working", i), 5., theme::ACCENT)).child("Working")
                    } else {
                        h().flex_none().gap(px(6.)).text_size(px(11.5)).text_color(c(theme::TEXT_2)).child(ui::ring(5., theme::TEXT_2)).child("Available")
                    })
            }));

        // Tasks: everything not finished yet, waiting-for-you first.
        let mut tasks: Vec<&aurelius_protocol::TaskView> = self.tasks.iter().filter(|t| t.project_id == project && ui::is_active(t)).collect();
        tasks.sort_by_key(|t| (!ui::is_waiting(t) && t.pending_approvals == 0, !matches!(t.status, TaskStatus::Running | TaskStatus::Review)));
        let task_total = tasks.len();
        let task_rows = tasks.into_iter().take(6).enumerate().map(|(i, t)| {
            let live = t.status.is_terminal() && t.live_run.is_some();
            let busy = matches!(t.status, TaskStatus::Running | TaskStatus::Review) || t.live_run == Some(aurelius_domain::views::LiveRun::Running);
            let (label, color) = match t.status {
                _ if live => (if busy { "Running" } else { "Queued" }, if busy { theme::ACCENT_TEXT } else { theme::TEXT_2 }),
                TaskStatus::Draft => ("Draft", theme::MUTED),
                TaskStatus::Queued => ("Queued", theme::TEXT_2),
                TaskStatus::Running if t.pending_approvals > 0 => ("Waiting for you", theme::ACCENT_TEXT),
                TaskStatus::Running => ("Running", theme::ACCENT_TEXT),
                TaskStatus::Review => ("In review", theme::ACCENT_TEXT),
                TaskStatus::Waiting => match t.waiting_reason.as_deref() {
                    Some("input") => ("Waiting for you", theme::ACCENT_TEXT),
                    Some("quota") => (ui::LIMIT_WAIT, theme::TEXT_2),
                    _ => ("Waiting", theme::TEXT_2),
                },
                _ => ("Done", theme::MUTED),
            };
            let label = ui::queued_wait(&t).unwrap_or_else(|| if label == ui::LIMIT_WAIT { ui::limit_wait(t.waiting_until_ms) } else { label.to_string() });
            let (filter, id) = (if ui::is_waiting(&t) { 1 } else { 0 }, t.id);
            h().id(("ov-task", i)).gap(px(10.)).px(px(6.)).py(px(8.)).rounded(px(6.)).cursor_pointer()
                .hover(|s| s.bg(c(theme::SURFACE_2)))
                .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| {
                    this.task_filter = filter;
                    this.set_tab(Tab::Tasks, w, cx);
                    this.selected = this.real_tasks(project).iter().position(|x| x.id == id).unwrap_or(0);
                }))
                .child(if busy { ui::pulse_dot(("ov-busy", i), 6., theme::ACCENT) } else { ui::ring(6., color).into_any_element() })
                .child(
                    v().flex_1().min_w_0()
                        .child(div().text_size(px(13.5)).truncate().child(t.title.clone()))
                        .child(div().text_size(px(11.5)).text_color(c(theme::MUTED)).child(t.employee_name.clone().unwrap_or_else(|| "Nobody yet".into()))),
                )
                .child(div().flex_none().text_size(px(11.5)).text_color(c(color)).child(label))
        }).collect::<Vec<_>>();
        let tasks_el = card()
            .child(section("ov-tasks", if task_total > 0 { format!("TASKS · {task_total} OPEN") } else { "TASKS".into() }, Tab::Tasks, cx))
            .when(task_rows.is_empty(), |el| el.child(div().child(empty("No open tasks.")).child(div().text_size(px(11.5)).child(ui::hint("ov-new-task", "secondary-t", "New task", || Box::new(NewTask), cx)))))
            .children(task_rows);

        // Ideas: the open ones, newest first as the Ideas tab lists them.
        let ideas_loading = self.ideas.as_ref().is_none_or(|(p, _)| *p != Some(project));
        let ideas: Vec<&aurelius_protocol::IdeaView> = self.ideas.as_ref().filter(|(p, _)| *p == Some(project))
            .map(|(_, l)| l.iter().filter(|i| i.status == IdeaStatus::Open).collect()).unwrap_or_default();
        let idea_total = ideas.len();
        let (yours, managers) = crate::app::split_ideas(ideas);
        let (yours_total, managers_total) = (yours.len(), managers.len());
        // At most five rows, with room for both groups when both have ideas.
        let managers_shown = managers_total.min(5 - yours_total.min(if managers_total > 0 { 3 } else { 5 }));
        let yours_shown = yours_total.min(5 - managers_shown);
        let mut ov_rows: Vec<AnyElement> = Vec::new();
        for (group, (label, list, shown)) in [("FROM YOU", yours, yours_shown), ("FROM EMPLOYEES", managers, managers_shown)].into_iter().enumerate() {
            if list.is_empty() { continue; }
            ov_rows.push(div().px(px(6.)).pt(px(if ov_rows.is_empty() { 2. } else { 10. })).pb(px(2.)).child(ideas_group_header(label, list.len())).into_any_element());
            for (k, idea) in list.into_iter().take(shown).enumerate() {
                let (id, by_manager) = (idea.id, idea.by_employee());
                ov_rows.push(v().id(("ov-idea", group * 5 + k)).relative().gap(px(2.)).px(px(6.)).py(px(8.)).rounded(px(6.)).cursor_pointer()
                    .when(by_manager, |el| el.pl(px(14.)).child(manager_bar(2.)))
                    .hover(|s| s.bg(c(theme::SURFACE_2)))
                    .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| {
                        this.ideas_filter = 0;
                        this.ideas_author = crate::app::IdeaAuthor::Anyone;
                        this.set_tab(Tab::Ideas, w, cx);
                        this.selected = this.real_ideas(Some(project)).iter().position(|x| x.id == id).unwrap_or(0);
                    }))
                    .child(div().font_family(serif.clone()).text_size(px(15.)).line_height(px(21.)).line_clamp(2).child(idea.title.clone()))
                    .child(h().flex_wrap().gap(px(8.)).text_size(px(11.5)).text_color(c(theme::MUTED))
                        .child(idea_author(idea, cx))
                        .child(format!("· {}", clock(idea.created_at_ms))))
                    .into_any_element());
            }
        }
        let ideas_el = card()
            .child(section("ov-ideas", if idea_total > 0 { format!("IDEAS · {idea_total} OPEN") } else { "IDEAS".into() }, Tab::Ideas, cx))
            .when(ideas_loading, |el| el.child(empty("Loading ideas…")))
            .when(!ideas_loading && idea_total == 0, |el| el.child(empty("No open ideas. Note one in the Ideas tab with Ctrl+N.")))
            .children(ov_rows);

        h().mt(px(28.)).gap(px(16.)).items_start()
            .child(v().flex_1().min_w_0().gap(px(16.)).child(managers_el).child(tasks_el))
            .child(v().flex_1().min_w_0().gap(px(16.)).child(ideas_el))
            .into_any_element()
    }

    // ── 08 / 09 Tasks ──────────────────────────────────────────────────

    fn render_tasks(&mut self, header: Div, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mono = ui::mono(cx);
        let serif = ui::serif(cx);
        let filters = [("Active", 3usize), ("Waiting", 1), ("Completed", 29)];
        let filter_bar = h().gap(px(6.)).mt(px(20.)).text_size(px(12.5)).children(filters.iter().enumerate().map(|(i, (label, count))| {
            let active = i == self.task_filter;
            h().id(("filter", i))
                .h(px(28.))
                .px(px(11.))
                .gap(px(7.))
                .rounded(px(6.))
                .cursor_pointer()
                .border_1()
                .border_color(c(if active { theme::BORDER } else { theme::BG }))
                .when(active, |el| el.bg(c(theme::SURFACE_2)))
                .text_color(c(if active { theme::TEXT } else { theme::TEXT_2 }))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.task_filter = i;
                    this.selected = 0;
                    cx.notify();
                }))
                .child(*label)
                .child(div().text_color(c(if active { theme::TEXT_2 } else { theme::MUTED })).child(count.to_string()))
                .child(ui::kbd(&(i + 1).to_string(), cx).opacity(0.5))
        }));

        let list: AnyElement = if self.task_filter < 2 {
            let waiting = self.task_filter == 1;
            v().mt(px(14.))
                .children(preview::ACTIVE_TASKS.iter().filter(|t| t.waiting == waiting).enumerate().map(|(i, task)| {
                    let selected = i == self.selected;
                    v().id(("task", i))
                        .px(px(16.))
                        .py(px(14.))
                        .gap(px(9.))
                        .rounded(px(8.))
                        .cursor_pointer()
                        .border_1()
                        .border_color(c(if selected { theme::FOCUS_RING } else { theme::BG }))
                        .when(selected, |el| el.bg(c(theme::SURFACE)))
                        .when(!selected, |el| el.border_b_1().border_color(c(theme::SURFACE_2)))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.selected = i;
                            cx.notify();
                        }))
                        .child(
                            h().gap(px(10.))
                                .child(if task.waiting { ui::diamond(8., theme::ACCENT_TEXT, false).into_any_element() } else { ui::dot(6., theme::ACCENT).into_any_element() })
                                .child(ui::row_title().text_size(px(14.)).when(selected, |el| el.font_weight(FontWeight::MEDIUM)).child(task.title))
                                .child(match (task.waiting, task.branch) {
                                    (true, _) => div().flex_none().text_size(px(12.)).text_color(c(theme::ACCENT_TEXT)).child("Waiting for you"),
                                    (false, Some(b)) => div().max_w(px(200.)).truncate().font_family(mono.clone()).text_size(px(11.5)).text_color(c(theme::MUTED)).child(b),
                                    (false, None) => div().flex_none().text_size(px(11.5)).text_color(c(theme::MUTED)).child("No repository"),
                                }),
                        )
                        .when(!task.steps.is_empty(), |el| {
                            el.child(div().pl(px(16.)).text_size(px(12.5)).text_color(c(theme::TEXT_2)).child(task.manager)).child(
                                v().pl(px(16.)).font_family(mono.clone()).text_size(px(12.)).line_height(px(21.)).text_color(c(theme::TEXT_2)).children(
                                    task.steps.iter().enumerate().map(|(j, (step, label))| {
                                        let branch = if j + 1 == task.steps.len() { "└─ " } else { "├─ " };
                                        let (mark, color) = match step {
                                            Step::Done => ("✓ ", theme::TEXT_2),
                                            Step::Running => ("● ", theme::TEXT),
                                            Step::Pending => ("○ ", theme::TEXT_2),
                                        };
                                        h().child(div().flex_none().text_color(c(theme::MUTED)).child(branch)).child(ui::row_title().text_color(c(color)).child(format!("{mark}{label}")))
                                    }),
                                ),
                            )
                        })
                }))
                .into_any_element()
        } else {
            let mut last_group = "";
            v().mt(px(14.))
                .text_size(px(14.))
                .children(preview::COMPLETED_TASKS.iter().enumerate().flat_map(|(i, (group, title, reference, is_commit, when))| {
                    let mut out: Vec<AnyElement> = Vec::new();
                    if *group != last_group {
                        last_group = group;
                        out.push(ui::eyebrow(*group).px(px(16.)).pt(px(if i == 0 { 6. } else { 18. })).pb(px(8.)).into_any_element());
                    }
                    let selected = i == self.selected;
                    out.push(
                        h().id(("done", i))
                            .px(px(16.))
                            .py(px(13.))
                            .gap(px(12.))
                            .rounded(px(8.))
                            .cursor_pointer()
                            .border_1()
                            .border_color(c(if selected { theme::FOCUS_RING } else { theme::BG }))
                            .when(selected, |el| el.bg(c(theme::SURFACE)))
                            .text_color(c(theme::TEXT_2))
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.selected = i;
                                cx.notify();
                            }))
                            .child(div().flex_none().w(px(16.)).child("✓"))
                            .child(ui::row_title().text_color(c(theme::TEXT)).when(selected, |el| el.font_weight(FontWeight::MEDIUM)).child(*title))
                            .child(
                                div()
                                    .max_w(px(200.))
                                    .truncate()
                                    .text_size(px(12.))
                                    .when(*is_commit, |el| el.font_family(mono.clone()))
                                    .text_color(c(if *is_commit && selected { theme::ACCENT_TEXT } else { theme::MUTED }))
                                    .child(*reference),
                            )
                            .child(div().flex_none().w(px(44.)).text_right().text_size(px(12.)).text_color(c(theme::MUTED)).child(*when))
                            .into_any_element(),
                    );
                    out
                }))
                .into_any_element()
        };

        let detail = if self.task_filter == 2 { self.task_detail_completed(&serif, &mono, cx).into_any_element() } else { self.task_detail_active(&serif, &mono, cx).into_any_element() };

        h().flex_1().min_w_0()
            .min_h_0()
            .items_start()
            .child(ui::scroll_frame(&self.page_scroll,
                div()
                    .id("tasks-left")
                    .flex_1()
                    .h_full()
                    .min_w_0()
                    .overflow_x_hidden()
                    .overflow_y_scroll()
                    .track_scroll(&self.page_scroll)
                    .track_focus(&self.focus)
                    .key_context(NAV)
                    .flex()
                    .flex_col()
                    .pl(px(60.))
                    .pr(px(40.))
                    .child(div().flex_none().w_full().min_w_0().child(header))
                    .child(div().flex_none().w_full().min_w_0().child(filter_bar))
                    .child(div().flex_none().w_full().min_w_0().child(list)),
            ).flex_1().h_full().min_w_0())
            .child(self.resizable("task_detail", 480., Edge::Left, detail, cx))
    }

    fn task_detail_active(&self, serif: &SharedString, mono: &SharedString, cx: &mut Context<Self>) -> impl IntoElement {
        let row = |label: &'static str, value: AnyElement| {
            h().justify_between().text_size(px(12.)).text_color(c(theme::MUTED)).child(label).child(value)
        };
        side_panel()
            .child(h().justify_between().child(ui::eyebrow("TASK")).child(ui::kbd("escape", cx).opacity(0.6)))
            .child(
                v().gap(px(6.)).mt(px(-8.))
                    .child(div().font_family(serif.clone()).text_size(px(24.)).line_height(px(29.)).child("Fix authentication race"))
                    .child(div().text_size(px(12.5)).text_color(c(theme::TEXT_2)).child("Alex · started 18:04 · 3 workers")),
            )
            .child(
                v().gap(px(8.)).px(px(14.)).py(px(12.)).rounded(px(8.)).border_1().border_color(c(theme::BORDER)).bg(c(theme::SURFACE))
                    .child(row("Repository", div().font_family(mono.clone()).text_color(c(theme::TEXT)).child("atlas-desktop").into_any_element()))
                    .child(row("Branch", h().font_family(mono.clone()).text_color(c(theme::TEXT)).child("main ").child(div().text_color(c(theme::MUTED)).child("← ")).child("alex/auth-refresh-lock").into_any_element()))
                    .child(row("Changes", h().gap(px(6.)).font_family(mono.clone()).text_color(c(theme::TEXT_2)).child("3 files").child(div().text_color(c(theme::ACCENT_TEXT)).child("+41")).child(div().text_color(c(theme::NEGATIVE)).child("−9")).into_any_element())),
            )
            .child(
                v().child(h().justify_between().mb(px(12.)).child(ui::eyebrow("HISTORY")).child(ui::eyebrow("4 commits")))
                    .child(timeline_row(ui::ring(9., theme::ACCENT).into_any_element(), v().gap(px(3.)).child(div().text_size(px(13.)).child("Review in progress")).child(small("Worker C · independent review · 1m")), false))
                    .children(preview::COMMITS.iter().map(|(sha, msg, meta)| {
                        timeline_row(
                            ui::dot(7., theme::TEXT).into_any_element(),
                            v().gap(px(3.))
                                .child(h().gap(px(9.)).text_size(px(13.)).child(div().font_family(mono.clone()).text_size(px(12.)).text_color(c(theme::ACCENT_TEXT)).child(*sha)).child(*msg))
                                .child(small(*meta)),
                            false,
                        )
                    }))
                    .child(timeline_row(
                        ui::diamond(8., theme::TEXT_2, false).into_any_element(),
                        v().gap(px(3.))
                            .child(h().gap(px(5.)).text_size(px(13.)).text_color(c(theme::TEXT_2)).child("Branched from").child(div().font_family(mono.clone()).text_size(px(12.)).child("main @ 7d0e912")))
                            .child(small("Alex · 18:05")),
                        true,
                    )),
            )
            .child(div().flex_1())
            .child(
                v().gap(px(12.)).pt(px(16.)).border_t_1().border_color(c(theme::BORDER))
                    .child(div().text_size(px(12.5)).line_height(px(18.75)).text_color(c(theme::TEXT_2)).child("Alex will open a merge request after review. Merges to main need your approval."))
                    .child(
                        h().gap(px(8.))
                            .child(
                                ui::button("message-alex", "Message Alex", ButtonKind::Outline, Some(ui::kbd("secondary-l", cx)))
                                    .flex_1()
                                    .on_click(cx.listener(|this, _: &ClickEvent, w, cx| {
                                        this.open_manager(crate::app::ManagerRef::Sample(0, 0), w, cx);
                                        this.on_focus_composer(&FocusComposer, w, cx);
                                    })),
                            ),
                    ),
            )
    }

    fn task_detail_completed(&self, serif: &SharedString, mono: &SharedString, cx: &mut Context<Self>) -> impl IntoElement {
        side_panel()
            .child(h().justify_between().child(ui::eyebrow("TASK · COMPLETED")).child(ui::kbd("escape", cx).opacity(0.6)))
            .child(
                v().gap(px(6.)).mt(px(-8.))
                    .child(div().font_family(serif.clone()).text_size(px(24.)).line_height(px(29.)).child("Fix authentication race"))
                    .child(div().text_size(px(12.5)).text_color(c(theme::TEXT_2)).child("Alex · 18:04 → 18:42 · 4 workers · 38 min")),
            )
            .child(
                v().gap(px(8.)).px(px(16.)).py(px(14.)).rounded(px(8.)).border_1().border_color(c(theme::ATTENTION_BORDER)).bg(c(theme::ATTENTION_BG))
                    .child(h().gap(px(10.)).text_size(px(13.)).child(div().text_color(c(theme::ACCENT_TEXT)).child("✓")).child("Merged into").child(div().font_family(mono.clone()).text_size(px(12.)).child("main")))
                    .child(h().gap(px(10.)).items_baseline().child(div().font_family(mono.clone()).text_size(px(15.)).text_color(c(theme::ACCENT_TEXT)).child("a3f9c21")).child(div().text_size(px(13.)).child("Fix token refresh race (#142)")))
                    .child(h().gap(px(5.)).text_size(px(11.5)).text_color(c(theme::MUTED)).child("Squash merge · approved by you at 18:41 · 3 files").child(div().text_color(c(theme::ACCENT_TEXT)).child("+41")).child(div().text_color(c(theme::NEGATIVE)).child("−9"))),
            )
            .child(
                v().child(
                    h().justify_between().mb(px(12.))
                        .child(ui::eyebrow("HOW IT GOT HERE")),
                )
                .child(timeline_row(ui::dot(9., theme::ACCENT).into_any_element(), v().gap(px(3.)).child(div().text_size(px(13.)).child("Merged after your approval")).child(small("18:42 · CI passed · 214 tests")), false))
                .child(timeline_row(ui::dot(7., theme::TEXT).into_any_element(), v().gap(px(3.)).child(div().text_size(px(13.)).child("Review passed")).child(small("Worker C · \"Lock scope is minimal; public API unchanged.\"")), false))
                .child(timeline_row(
                    ui::ring(7., theme::TEXT_2).into_any_element(),
                    v().gap(px(5.)).child(div().text_size(px(13.)).text_color(c(theme::TEXT_2)).child("3 commits squashed")).child(
                        v().font_family(mono.clone()).text_size(px(11.5)).line_height(px(19.5)).text_color(c(theme::MUTED))
                            .child(h().gap(px(8.)).child(div().text_color(c(theme::TEXT_2)).child("e41b07c")).child("regression test · B"))
                            .child(h().gap(px(8.)).child(div().text_color(c(theme::TEXT_2)).child("9c2d4aa")).child("single-flight lock · A"))
                            .child(h().gap(px(8.)).child(div().text_color(c(theme::TEXT_2)).child("51fa3e0")).child("refresh timing log · B")),
                    ),
                    false,
                ))
                .child(timeline_row(
                    ui::diamond(8., theme::TEXT_2, false).into_any_element(),
                    v().gap(px(5.)).child(div().text_size(px(13.)).text_color(c(theme::TEXT_2)).child("Your request")).child(quote(preview::MANAGER_REQUEST, serif, 14.5)),
                    true,
                )),
            )
            .child(div().flex_1())
            .child(
                h().gap(px(8.)).pt(px(16.)).border_t_1().border_color(c(theme::BORDER))
                    .child(ui::button("revert", "Revert…", ButtonKind::Outline, None).flex_1().text_color(c(theme::TEXT_2)).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.notify_user("Sample: reverting would need an approval.", false, cx)))),
            )
    }

    // ── Real tasks (B14) ───────────────────────────────────────────────

    fn render_real_tasks(&mut self, header: Div, project: ProjectRef, cx: &mut Context<Self>) -> impl IntoElement {
        let pk = self.page_key();
        use aurelius_domain::task::TaskStatus;
        let ProjectRef::Real(project_id) = project else { return div().into_any_element() };
        let previewed = {
            let tasks = self.real_tasks(project_id);
            tasks.get(self.selected.min(tasks.len().saturating_sub(1))).and_then(|t| t.conversation_id)
        };
        if let Some(conversation) = previewed {
            self.ensure_preview_messages(conversation);
        }
        let serif = ui::serif(cx);
        let all: Vec<_> = self.tasks.iter().filter(|t| t.project_id == project_id).collect();
        let counts = [
            all.iter().filter(|t| (matches!(t.status, TaskStatus::Draft | TaskStatus::Queued | TaskStatus::Running | TaskStatus::Review) || (t.status.is_terminal() && t.live_run.is_some())) && !ui::is_waiting(t)).count(),
            all.iter().filter(|t| ui::is_waiting(t)).count(),
            all.iter().filter(|t| t.status.is_terminal() && t.live_run.is_none()).count(),
        ];
        let filters = ["Active", "Waiting", "Completed"];
        let filter_bar = h().flex_wrap().gap(px(6.)).mt(px(20.)).text_size(px(12.5))
            .children(filters.iter().enumerate().map(|(i, label)| {
                let active = i == self.task_filter;
                h().id(("rfilter", i)).h(px(28.)).px(px(11.)).gap(px(7.)).rounded(px(6.)).cursor_pointer()
                    .border_1().border_color(c(if active { theme::BORDER } else { theme::BG }))
                    .when(active, |el| el.bg(c(theme::SURFACE_2)))
                    .text_color(c(if active { theme::TEXT } else { theme::TEXT_2 }))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.task_filter = i;
                        this.selected = 0;
                        cx.notify();
                    }))
                    .child(*label)
                    .child(div().text_color(c(theme::MUTED)).child(counts[i].to_string()))
                    .child(ui::kbd(&(i + 1).to_string(), cx).opacity(0.5))
            }))
            .child(div().flex_1())
            .child(
                ui::button("new-task", "New task", ButtonKind::Outline, Some(ui::kbd("secondary-t", cx)))
                    .h(px(28.))
                    .on_click(cx.listener(|_, _: &ClickEvent, w, cx| w.dispatch_action(Box::new(NewTask), cx))),
            );

        let tasks = self.real_tasks(project_id);
        let selected = self.selected.min(tasks.len().saturating_sub(1));
        // Build 100 rows at a time: the page re-renders on every scroll and hover frame.
        let key = (Some(project_id), self.task_filter);
        let (last_key, last_shown) = self.project_tasks_shown.get();
        let mut shown = if last_key == key { last_shown } else { IDEAS_STEP };
        let (offset, max) = (self.page_scroll.offset().y, self.page_scroll.max_offset().y);
        if shown < tasks.len() && max > px(0.) && -offset + px(800.) >= max {
            shown += IDEAS_STEP;
            cx.notify();
        }
        shown = shown.max((selected + 1).min(tasks.len()));
        self.project_tasks_shown.set((key, shown));
        let status_label = |t: &aurelius_protocol::TaskView| -> (String, u32) {
            let (label, color) = match t.status {
                _ if t.status.is_terminal() && t.live_run.is_some() => if t.live_run == Some(aurelius_domain::views::LiveRun::Running) { ("Running", theme::ACCENT_TEXT) } else { ("Queued", theme::TEXT_2) },
                TaskStatus::Draft => ("Draft (no employee)", theme::MUTED),
                TaskStatus::Queued => ("Queued", theme::TEXT_2),
                TaskStatus::Running if t.pending_approvals > 0 => ("Waiting for you in the Inbox", theme::ACCENT_TEXT),
                TaskStatus::Running => ("Running", theme::ACCENT_TEXT),
                TaskStatus::Review => ("In employee review", theme::ACCENT_TEXT),
                TaskStatus::Waiting => match t.waiting_reason.as_deref() {
                    Some("input") => ("Waiting for you", theme::ACCENT_TEXT),
                    Some("dependency") => ("Waiting for another task", theme::TEXT_2),
                    Some("paused") => ("Employee paused", theme::TEXT_2),
                    Some("quota") => (ui::LIMIT_WAIT, theme::TEXT_2),
                    _ => ("Waiting for connection", theme::TEXT_2),
                },
                TaskStatus::Succeeded => ("Done", theme::ACCENT_TEXT),
                TaskStatus::Failed => ("Failed", theme::NEGATIVE),
                TaskStatus::Cancelled => ("Cancelled", theme::MUTED),
            };
            (ui::queued_wait(t).unwrap_or_else(|| if label == ui::LIMIT_WAIT { ui::limit_wait(t.waiting_until_ms) } else { label.to_string() }), color)
        };
        let list = v().mt(px(14.))
            .when(tasks.is_empty(), |el| {
                el.child(
                    v().py(px(40.)).items_center().gap(px(8.))
                        .child(div().font_family(serif.clone()).italic().text_size(px(19.)).text_color(c(theme::TEXT_2)).child("No tasks in this list."))
                        .child(div().text_size(px(11.5)).child(ui::hint("empty-new-task", "secondary-t", "New task", || Box::new(NewTask), cx))),
                )
            })
            .children(tasks.iter().take(shown).enumerate().map(|(i, t)| {
                let (label, color) = status_label(t);
                let mark = self.task_mark(t);
                let is_selected = i == selected;
                let busy = matches!(t.status, TaskStatus::Running | TaskStatus::Review) || t.live_run == Some(aurelius_domain::views::LiveRun::Running);
                ui::rise(v().id(("rtask", i))
                    .on_mouse_down(MouseButton::Right, cx.listener(move |this, e: &MouseDownEvent, w, cx| {
                        this.selected = i;
                        let items = this.task_menu();
                        if !items.is_empty() {
                            this.open_menu(e.position, items, w, cx);
                        }
                    })).px(px(16.)).py(px(13.)).gap(px(6.)).rounded(px(8.)).cursor_pointer()
                    .border_1().border_color(c(if is_selected { theme::FOCUS_RING } else { theme::BG }))
                    .when(is_selected, |el| el.bg(c(theme::SURFACE)))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.selected = i;
                        cx.notify();
                    }))
                    .child(
                        h().gap(px(10.))
                            .child(if busy { ui::pulse_dot("task-busy", 6., theme::ACCENT) } else { ui::ring(6., color).into_any_element() })
                            .child(ui::row_title().text_size(px(14.)).when(is_selected, |el| el.font_weight(FontWeight::MEDIUM)).child(t.title.clone()))
                            .child(if mark.is_finished() { ui::task_mark(mark, ("task-mark", i)).flex_none() } else { div().max_w(px(220.)).truncate().text_size(px(12.)).text_color(c(color)).child(label) }),
                    )
                    .child(
                        h().pl(px(16.)).gap(px(10.)).text_size(px(12.)).text_color(c(theme::MUTED))
                            .child(div().min_w_0().truncate().child(t.employee_name.clone().unwrap_or_else(|| "No employee".into())))
                            .when_some(t.created_by_app.clone(), |el, app| el.child(div().min_w_0().truncate().text_color(c(theme::ACCENT_TEXT)).child(format!("via {app}"))))
                    ), ("task-row", pk.wrapping_add(i)), i)
            }));

        let detail: AnyElement = match tasks.get(selected) {
            None => side_panel().child(ui::eyebrow("TASK")).child(div().text_color(c(theme::MUTED)).child("Select a task.")).into_any_element(),
            Some(t) => {
                let (label, color) = status_label(t);
                let mark = self.task_mark(t);
                v().size_full().border_l_1().border_color(c(theme::BORDER)).bg(c(theme::PANEL))
                    .child(
                        div().id("task-detail").size_full().overflow_y_scrollbar().px(px(26.)).py(px(24.)).flex().flex_col().gap(px(18.))
                            .child(
                                h().justify_between().child(ui::eyebrow("TASK"))
                                    .child(h().gap(px(6.)).child(if mark.is_finished() { ui::task_mark(mark, "detail-mark") } else { div().text_size(px(11.5)).text_color(c(color)).child(label) }).child(ui::more_button("task-more").on_click(cx.listener(|this, e: &ClickEvent, w, cx| {
                                        let items = this.task_menu();
                                        this.open_menu(e.position(), items, w, cx);
                                    })))),
                            )
                            .child(
                                v().gap(px(6.)).flex_none()
                                    .child(div().font_family(serif.clone()).text_size(px(24.)).line_height(px(29.)).child(t.title.clone()))
                                    .child(div().text_size(px(12.5)).text_color(c(theme::TEXT_2)).child(t.employee_name.clone().unwrap_or_else(|| "No employee".into())))
                                    .when_some(t.delegated_by.clone(), |el, by| {
                                        let parent = t.parent_task_id.and_then(|p| self.tasks.iter().find(|x| x.id == p).map(|x| x.title.clone()));
                                        el.child(div().text_size(px(12.)).text_color(c(theme::ACCENT_TEXT)).child(match parent {
                                            Some(title) => format!("Delegated by {by} for “{title}”"),
                                            None => format!("Delegated by {by}"),
                                        }))
                                    })
                                    .when_some(t.created_by_app.clone(), |el, app| {
                                        el.child(div().text_size(px(12.)).text_color(c(theme::ACCENT_TEXT)).child(format!("Created via {app} (aurelius-mcp)")))
                                    })
                                    .when_some(t.schedule_id, |el, id| {
                                        let title = self.schedules.iter().find(|s| s.id == id).map(|s| format!(" “{}”", s.title)).unwrap_or_default();
                                        el.child(
                                            h().id("from-schedule").gap(px(6.)).cursor_pointer().text_size(px(12.)).text_color(c(theme::ACCENT_TEXT))
                                                .hover(|s| s.text_color(c(theme::TEXT)))
                                                .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.open_schedule(id, w, cx)))
                                                .child(format!("↻ From schedule{title}")),
                                        )
                                    })
                                    .children(t.handoffs.iter().map(|hnd| {
                                        div().text_size(px(12.)).text_color(c(theme::TEXT_2)).child(if hnd.reason.is_empty() {
                                            format!("Handed over: {} → {}", hnd.from, hnd.to)
                                        } else {
                                            format!("Handed over: {} → {} ({})", hnd.from, hnd.to, hnd.reason)
                                        })
                                    }))
                                    .when(t.conversation_id.is_some(), |el| {
                                        el.child(h().mt(px(4.)).gap(px(8.)).flex_wrap()
                                            .child(
                                                ui::button("task-conversation", "Open conversation", ButtonKind::Outline, Some(ui::kbd("alt-c", cx)))
                                                    .h(px(28.))
                                                    .on_click(cx.listener(|_, _: &ClickEvent, w, cx| w.dispatch_action(Box::new(OpenTaskConversation), cx))),
                                            ))
                                    })
                                    .when(matches!(t.status, TaskStatus::Draft | TaskStatus::Queued | TaskStatus::Waiting), |el| {
                                        el.child(h().mt(px(4.)).child(
                                            ui::button("assign-task", if t.employee_id.is_some() { "Give to another employee" } else { "Give to an employee" }, ButtonKind::Outline, Some(ui::kbd("alt-a", cx)))
                                                .h(px(28.))
                                                .on_click(cx.listener(|_, _: &ClickEvent, w, cx| w.dispatch_action(Box::new(AssignSelectedTask), cx))),
                                        ))
                                    }),
                            )
                            .when(t.pending_approvals > 0, |el| {
                                let n = t.pending_approvals;
                                el.child(
                                    h().gap(px(10.)).flex_none().px(px(14.)).py(px(11.)).rounded(px(8.)).border_1().border_color(c(theme::ATTENTION_BORDER)).bg(c(theme::ATTENTION_BG))
                                        .child(ui::diamond(8., theme::ACCENT_TEXT, false))
                                        .child(div().flex_1().min_w_0().text_size(px(12.5)).child(if n == 1 { "This task waits for you: an approval or an answer.".to_string() } else { format!("This task waits for you: {n} approvals or answers.") }))
                                        .child(ui::button("task-approvals", "Review", ButtonKind::Primary, Some(ui::kbd("secondary-i", cx))).on_click(cx.listener(|_, _: &ClickEvent, w, cx| w.dispatch_action(Box::new(OpenInbox), cx)))),
                                )
                            })
                            .child({
                                let mode = t.run.mode.or_else(|| t.employee_id.and_then(|id| self.managers.iter().find(|m| m.id == id)).map(|m| m.agent_mode));
                                h().gap(px(10.)).flex_none().text_size(px(12.)).text_color(c(theme::MUTED))
                                    .when_some(mode, |el, mode| el.child(format!("{} {}", crate::screens::chat::mode_icon(mode), mode.label())))
                                    .child(if t.status.is_terminal() {
                                        format!("Took {}", ui::fmt_elapsed(t.updated_at_ms - t.created_at_ms))
                                    } else {
                                        format!("{} since it was made", ui::fmt_elapsed(ui::now_ms() - t.created_at_ms))
                                    })
                            })
                            .children(t.conversation_id.and_then(|conv| self.live_section(conv, cx)))
                            .children(t.conversation_id.and_then(|conv| self.recent_messages(conv)))
                            .child(v().gap(px(6.)).flex_none().child(ui::eyebrow("GOAL")).child(div().text_size(px(13.)).line_height(px(19.)).child(t.goal.clone())))
                            .when(!t.attachments.is_empty(), |el| {
                                let folder: Option<String> = None;
                                el.child(v().gap(px(8.)).flex_none().child(ui::eyebrow("ATTACHMENTS")).child(self.attachment_chips("task-att", &t.attachments, t.project_id, t.employee_id, folder, cx)))
                            })
                            .child(
                                v().gap(px(6.)).flex_none().child(ui::eyebrow("ACCEPTANCE CRITERIA"))
                                    .when(t.criteria.is_empty(), |el| el.child(div().text_size(px(12.5)).text_color(c(theme::MUTED)).child("No explicit criteria.")))
                                    .children(t.criteria.iter().map(|crit| {
                                        h().gap(px(8.)).items_start().text_size(px(12.5))
                                            .child(div().w(px(14.)).text_color(c(theme::MUTED)).child("○"))
                                            .child(v().flex_1().min_w_0().child(crit.clone()))
                                    })),
                            )
                            .when(!t.children.is_empty(), |el| {
                                let all = &self.tasks;
                                el.child(
                                    v().gap(px(8.)).flex_none().child(ui::eyebrow("DELEGATED TO COLLEAGUES"))
                                        .children(t.children.iter().filter_map(|id| all.iter().find(|x| x.id == *id)).map(|sub| {
                                            let (sl, sc) = status_label(sub);
                                            v().gap(px(4.)).px(px(12.)).py(px(10.)).rounded(px(8.)).border_1().border_color(c(theme::BORDER)).bg(c(theme::SURFACE))
                                                .child(h().gap(px(8.))
                                                    .child(ui::row_title().text_size(px(13.)).font_weight(FontWeight::MEDIUM).child(sub.title.clone()))
                                                    .child(div().max_w(px(160.)).truncate().text_size(px(11.5)).text_color(c(sc)).child(sl)))
                                                .child(div().truncate().text_size(px(12.)).text_color(c(theme::TEXT_2)).child(sub.employee_name.clone().unwrap_or_else(|| "No employee".into())))
                                        })),
                                )
                            })
                            .when_some(t.result.clone(), |el, report| {
                                el.child(
                                    v().gap(px(6.)).flex_none().child(ui::eyebrow("REPORT"))
                                        .child(div().p(px(12.)).rounded(px(6.)).bg(c(theme::SURFACE)).border_1().border_color(c(theme::BORDER)).text_size(px(13.)).child(
                                            gpui_kit::component::text::TextView::markdown(SharedString::from(format!("task-report-{}", t.id)), crate::screens::markdown::linked(&report)).plugin(crate::screens::mermaid::Mermaid)
                                                .selectable(true)
                                                .on_link_click(self.markdown_link_handler(false, cx)),
                                        )),
                                )
                            })
                            .when(!t.status.is_terminal(), |el| {
                                el.child(
                                    h().gap(px(8.)).pt(px(12.)).border_t_1().border_color(c(theme::BORDER)).flex_none().flex_wrap()
                                        .child(ui::button("cancel-task", "Cancel", ButtonKind::Outline, Some(ui::kbd("x", cx))).on_click(cx.listener(|_, _: &ClickEvent, w, cx| w.dispatch_action(Box::new(CancelSelectedTask), cx)))),
                                )
                            }),
                    )
                    .into_any_element()
            }
        };

        h().flex_1().min_h_0().min_w_0().items_start()
            .child(ui::scroll_frame(&self.page_scroll,
                div().id("tasks-left").flex_1().h_full().min_w_0().overflow_x_hidden().overflow_y_scroll().track_scroll(&self.page_scroll).track_focus(&self.focus).key_context(NAV)
                    .flex().flex_col().pl(px(60.)).pr(px(40.)).pb(px(30.))
                    .child(div().flex_none().w_full().min_w_0().child(header))
                    .child(div().flex_none().w_full().min_w_0().child(filter_bar))
                    .child(div().flex_none().w_full().min_w_0().child(list)),
            ).flex_1().h_full().min_w_0())
            .child(self.resizable("task_detail", 480., Edge::Left, detail, cx))
            .into_any_element()
    }

    /// Files attached to a task or recurring task: image thumbnails and file chips. A click opens
    /// the copy in the work folder on a server on this computer, else the file fetched from the
    /// server; a server too old to fetch files shows them only.
    pub fn attachment_chips(&self, prefix: &'static str, files: &[aurelius_protocol::MessageFileView], project: aurelius_domain::ProjectId, manager: Option<aurelius_domain::EmployeeId>, task_folder: Option<String>, cx: &mut Context<Self>) -> AnyElement {
        let fetchable = self.uploads_fetchable();
        h().flex_wrap().gap(px(8.))
            .children(files.iter().enumerate().map(|(i, f)| {
                let found = self.work_file(project, manager, task_folder.clone(), &f.path);
                let image = crate::screens::side_panels::is_image_name(&f.name);
                let thumb: Option<ImageSource> = match found.clone().filter(|_| image) {
                    Some(path) => Some(std::sync::Arc::<std::path::Path>::from(path).into()),
                    None => f.id.as_ref().and_then(|id| self.upload_thumbs.get(id)).cloned().flatten().map(ImageSource::from),
                };
                let open = found.clone();
                let remote = (open.is_none() && fetchable && f.id.is_some()).then(|| f.clone());
                v().id(SharedString::from(format!("{prefix}-{i}"))).gap(px(5.)).p(px(7.)).rounded(px(7.)).border_1().border_color(c(theme::BORDER)).bg(c(theme::SURFACE))
                    .tooltip(ui::tip(f.path.clone(), None))
                    .when(open.is_some() || remote.is_some(), |el| {
                        el.cursor_pointer().hover(|s| s.border_color(c(theme::BORDER_STRONG))).on_click(cx.listener(move |this, _: &ClickEvent, w, cx| {
                            if let Some(path) = &open {
                                cx.open_with_system(path);
                            } else if let Some(file) = &remote {
                                this.open_upload_file(file, w, cx);
                            }
                        }))
                    })
                    .when_some(thumb, |el, source| {
                        el.child(img(source).w(px(120.)).h(px(80.)).object_fit(ObjectFit::Cover).rounded(px(5.)))
                    })
                    .child(
                        h().gap(px(6.)).text_size(px(12.))
                            .child(div().text_color(c(theme::ACCENT_TEXT)).child(if image { "▣" } else { "▤" }))
                            .child(div().max_w(px(160.)).truncate().child(f.name.clone()))
                            .child(div().text_color(c(theme::MUTED)).child(crate::app::file_size(f.byte_length))),
                    )
            }))
            .into_any_element()
    }

    pub fn render_ideas_page(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let serif = ui::serif(cx);
        let filtered = self.ideas_project_filter.and_then(|id| self.projects.iter().find(|p| p.id == id));
        let label = filtered.map_or_else(|| "All projects".to_string(), |p| p.name.clone());
        let set = filtered.is_some();
        let open = self.overlay == Overlay::Menu && self.menu.as_ref().is_some_and(|m| m.field == Some("ideas-project"));
        let picker = ui::dropdown_trigger("ideas-project", 28., set, open).max_w(px(240.))
            .tooltip(ui::tip(label.clone(), None))
            .on_click(cx.listener(|this, _: &ClickEvent, w, cx| {
                let current = this.ideas_project_filter;
                let option = |label: String, choice: TaskChoice, checked: bool| Some(MenuEntry { label: label.into(), keys: None, run: MenuDo::Task(choice), danger: false, checked, detail: None });
                let mut items: MenuItems = vec![option("All projects".into(), TaskChoice::IdeasProject(None), current.is_none()), None];
                items.extend(this.projects.iter().map(|p| option(p.name.clone(), TaskChoice::IdeasProject(Some(p.id)), current == Some(p.id))));
                this.open_dropdown("ideas-project", items, w, cx);
            }))
            .child(div().min_w_0().truncate().child(label))
            .child(ui::caret())
            .child(ui::track_bounds(self.dropdown_bounds.clone(), "ideas-project"));
        let body = v().pt(px(40.)).pb(px(40.))
            .child(h().gap(px(14.)).child(div().font_family(serif).text_size(px(30.)).child("Ideas")).child(div().pt(px(8.)).child(picker)))
            .child(self.render_ideas(None, cx));
        self.page(body).into_any_element()
    }

    /// Ideas noted by you or an employee, of one project (`Some`) or of every project (`None`);
    /// start one as a task when its time comes.
    fn render_ideas(&mut self, scope: Option<aurelius_domain::ProjectId>, cx: &mut Context<Self>) -> impl IntoElement {
        use aurelius_domain::idea::{IdeaKind, IdeaStatus};
        self.ensure_ideas(scope);
        let serif = ui::serif(cx);
        let loading = self.ideas.as_ref().is_none_or(|(p, _)| *p != scope);
        let only = if scope.is_none() { self.ideas_project_filter } else { None };
        let author_filter = self.ideas_author;
        // Borrowed, not cloned: the page re-renders on every scroll and hover frame, and only
        // `shown` rows are built; 100 more load as the page nears its end (reset when a filter changes).
        let all = self.real_ideas(scope);
        let key = (scope, only, self.ideas_filter, author_filter, self.ideas_kind);
        let (last_key, last_shown) = self.ideas_shown.get();
        let mut shown = if last_key == key { last_shown } else { IDEAS_STEP };
        let (offset, max) = (self.page_scroll.offset().y, self.page_scroll.max_offset().y);
        if shown < all.len() && max > px(0.) && -offset + px(800.) >= max {
            shown += IDEAS_STEP;
            cx.notify();
        }
        // Keyboard selection can run past the built rows.
        shown = shown.max((self.selected + 1).min(all.len()));
        self.ideas_shown.set((key, shown));
        let total = all.len();
        let items = &all[..total.min(shown)];
        let kinds = h().gap(px(4.)).text_size(px(12.))
            .children([(None, "All"), (Some(IdeaKind::Feature), "Features"), (Some(IdeaKind::Bug), "Bugs")].into_iter().enumerate().map(|(i, (kind, label))| {
                let active = kind == self.ideas_kind;
                div().id(("idea-kind", i)).px(px(9.)).py(px(4.)).rounded(px(5.)).cursor_pointer()
                    .border_1().border_color(c(if active { theme::BORDER } else { theme::BG }))
                    .when(active, |el| el.bg(c(theme::SURFACE_2)))
                    .text_color(c(if active { theme::TEXT } else { theme::TEXT_2 }))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.ideas_kind = kind;
                        this.selected = 0;
                        cx.notify();
                    }))
                    .child(label)
            }));
        let open_count = self.ideas.as_ref().filter(|(p, _)| *p == scope)
            .map_or(0, |(_, l)| l.iter().filter(|i| only.is_none_or(|p| i.project_id == p) && i.status == IdeaStatus::Open && author_filter.admits(i)).count());
        let selected = self.selected.min(total.saturating_sub(1));
        let filters = ["Open", "Done", "Dropped"];
        let authors = h().gap(px(4.)).text_size(px(12.))
            .children(crate::app::IdeaAuthor::ALL.into_iter().enumerate().map(|(i, who)| {
                let active = who == author_filter;
                div().id(("idea-author", i)).px(px(9.)).py(px(4.)).rounded(px(5.)).cursor_pointer()
                    .border_1().border_color(c(if active { theme::BORDER } else { theme::BG }))
                    .when(active, |el| el.bg(c(theme::SURFACE_2)))
                    .text_color(c(if active { theme::TEXT } else { theme::TEXT_2 }))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.ideas_author = who;
                        this.selected = 0;
                        cx.notify();
                    }))
                    .child(who.label())
            }))
            .child(ui::kbd("4", cx).opacity(0.5));
        v().child(
            h().flex_wrap().gap(px(10.)).mt(px(24.)).justify_between()
                .child(h().gap(px(18.)).child(h().gap(px(4.)).text_size(px(12.)).children(filters.iter().enumerate().map(|(i, label)| {
                    let active = i == self.ideas_filter;
                    h().id(("idea-filter", i)).gap(px(6.)).px(px(9.)).py(px(4.)).rounded(px(5.)).cursor_pointer()
                        .border_1().border_color(c(if active { theme::BORDER } else { theme::BG }))
                        .when(active, |el| el.bg(c(theme::SURFACE_2)))
                        .text_color(c(if active { theme::TEXT } else { theme::TEXT_2 }))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.ideas_filter = i;
                            this.selected = 0;
                            cx.notify();
                        }))
                        .child(if i == 0 && open_count > 0 { format!("{label} ({open_count})") } else { label.to_string() })
                        .child(ui::kbd(&(i + 1).to_string(), cx).opacity(0.5))
                }))).child(authors).child(kinds))
                .child(ui::button("add-idea", "New idea", ButtonKind::Outline, Some(ui::kbd("secondary-n", cx))).h(px(32.)).on_click(cx.listener(|_, _: &ClickEvent, w, cx| w.dispatch_action(Box::new(NewItem), cx)))),
        )
        .when(loading, |el| el.child(div().mt(px(20.)).text_color(c(theme::MUTED)).child("Loading ideas…")))
        .when(!loading && total == 0, |el| {
            el.child(div().mt(px(24.)).text_size(px(13.5)).text_color(c(theme::MUTED)).child(match (self.ideas_filter, author_filter) {
                (0, crate::app::IdeaAuthor::Anyone) => "No open ideas. Note one with Ctrl+N.",
                (0, crate::app::IdeaAuthor::You) => "You have no open ideas. Note one with Ctrl+N.",
                (0, crate::app::IdeaAuthor::Managers) => "No open ideas from employees yet.",
                (1, _) => "Nothing done yet.",
                _ => "Nothing dropped.",
            }))
        })
        .children({
        // real_ideas lists yours first, so the first `yours` rows are yours.
        let total_yours = all.iter().filter(|i| !i.by_employee()).count();
        let yours = items.iter().filter(|i| !i.by_employee()).count();
        let mut rows: Vec<AnyElement> = items.iter().enumerate().map(|(i, &idea)| {
            let is_selected = i == selected;
            let by_manager = idea.by_employee();
            let project_label = scope.is_none().then(|| self.projects.iter().find(|p| p.id == idea.project_id).map_or_else(|| "Unknown project".to_string(), |p| p.name.clone()));
            let task = idea.task_id.map(|_| match idea.task_status {
                Some(s) if s.is_terminal() => format!("task {}", s.as_str()),
                Some(s) => format!("task {}", s.as_str()),
                None => "task".to_string(),
            });
            v().id(("idea", i)).relative().gap(px(5.)).px(px(14.)).py(px(12.)).rounded(px(8.)).border_1().cursor_pointer()
                .when(by_manager, |el| el.pl(px(20.)).child(manager_bar(6.)))
                .border_color(c(if is_selected { theme::FOCUS_RING } else { theme::BORDER }))
                .bg(c(theme::SURFACE))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.selected = i;
                    cx.notify();
                }))
                .on_mouse_down(MouseButton::Right, cx.listener(move |this, e: &MouseDownEvent, w, cx| {
                    this.selected = i;
                    this.open_idea_menu(e.position, w, cx);
                }))
                .child(h().flex_wrap().gap(px(8.)).text_size(px(11.5)).text_color(c(theme::MUTED))
                    .child(idea_author(idea, cx))
                    .child(kind_badge(idea.kind))
                    .when_some(project_label, |el, p| el.child(div().px(px(6.)).rounded(px(4.)).bg(c(theme::SURFACE_2)).text_color(c(theme::TEXT_2)).child(p)))
                    .child(format!("· {}", clock(idea.created_at_ms)))
                    .when_some(task, |el, t| el.child(div().text_color(c(theme::ACCENT_TEXT)).child(format!("· {t}")))))
                .child(div().font_family(serif.clone()).text_size(px(17.)).line_height(px(24.))
                    .text_color(c(if idea.status == IdeaStatus::Open { theme::TEXT } else { theme::MUTED }))
                    .child(idea.title.clone()))
                .when_some(idea.note.clone(), |el, n| el.child(div().text_size(px(13.)).line_height(px(19.)).text_color(c(theme::TEXT_2)).child(n)))
                .when(!idea.attachments.is_empty(), |el| el.child(self.attachment_chips("idea-file", &idea.attachments, idea.project_id, None, None, cx)))
                .into_any_element()
        }).collect();
        let theirs = rows.split_off(yours);
        let group = |label: &str, rows: Vec<AnyElement>, count: usize, first: bool| (!rows.is_empty()).then(|| {
            v().mt(px(if first { 18. } else { 26. })).gap(px(8.)).child(ideas_group_header(label, count)).children(rows)
        });
        let first_mine = !rows.is_empty();
        [group("FROM YOU", rows, total_yours, true), group("FROM EMPLOYEES", theirs, total - total_yours, !first_mine)].into_iter().flatten()
        })
        .child(h().mt(px(14.)).gap(px(14.)).text_size(px(11.5)).text_color(c(theme::MUTED))
            .child(ui::key_hint(&["secondary-t"], "start as a task", cx))
            .child(ui::key_hint(&["f2"], "edit", cx))
            .child(ui::key_hint(&["delete"], "drop", cx)))
    }

    fn render_real_memory(&mut self, project: aurelius_domain::ProjectId, cx: &mut Context<Self>) -> impl IntoElement {
        let pk = self.page_key();
        use aurelius_domain::memory::MemoryStatus;
        self.ensure_memory(project);
        if self.memory_filter == 0 {
            self.ensure_memory_ctx(project);
        }
        let serif = ui::serif(cx);
        // Borrowed, not cloned: this page re-renders on every scroll and hover frame.
        let items: Vec<&aurelius_protocol::MemoryView> = self.real_memory(project);
        let all: &[aurelius_protocol::MemoryView] = self.memory.as_ref().map_or(&[], |(_, m)| m.as_slice());
        let loading = self.memory.as_ref().is_none_or(|(p, _)| *p != project);
        let proposals = all.iter().filter(|m| m.status == MemoryStatus::Proposed).count();
        let selected = self.selected.min(items.len().saturating_sub(1));
        let filters = ["In context", "Proposals", "History"];
        let filter = self.memory_filter;
        // Build 100 cards at a time (reset when the tab or project changes).
        let key = (Some(project), 100 + filter);
        let (last_key, last_shown) = self.project_tasks_shown.get();
        let mut shown = if last_key == key { last_shown } else { IDEAS_STEP };
        let (offset, max) = (self.page_scroll.offset().y, self.page_scroll.max_offset().y);
        if shown < items.len() && max > px(0.) && -offset + px(800.) >= max {
            shown += IDEAS_STEP;
            cx.notify();
        }
        shown = shown.max((selected + 1).min(items.len()));
        self.project_tasks_shown.set((key, shown));
        let card = |i: usize, m: &aurelius_protocol::MemoryView| {
            let is_selected = i == selected;
            let (badge, badge_color) = match m.status {
                MemoryStatus::Proposed => ("PROPOSAL", theme::ACCENT_TEXT),
                MemoryStatus::Superseded => ("SUPERSEDED", theme::MUTED),
                MemoryStatus::Forgotten => ("FORGOTTEN", theme::MUTED),
                MemoryStatus::Rejected => ("REJECTED", theme::MUTED),
                MemoryStatus::Active => ("", theme::MUTED),
            };
            let conflict = m.conflicts_with_id.and_then(|id| all.iter().find(|x| x.id == id)).map(|x| x.content.clone());
            let source = m.sources.first().map(|s| format!("source: {} · {}", s.label, clock(s.at_ms))).unwrap_or_else(|| "no source".into());
            v().id(("mem", i))
                .on_mouse_down(MouseButton::Right, cx.listener(move |this, e: &MouseDownEvent, w, cx| {
                    this.selected = i;
                    let items = this.memory_menu();
                    this.open_menu(e.position, items, w, cx);
                }))
                .py(px(14.))
                .px(px(10.))
                .gap(px(6.))
                .rounded(px(6.))
                .border_1()
                .border_color(c(if is_selected { theme::FOCUS_RING } else { theme::BG }))
                .cursor_pointer()
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.selected = i;
                    cx.notify();
                }))
                .child(
                    h().flex_wrap().gap(px(8.)).text_size(px(11.5)).text_color(c(theme::TEXT_2))
                        .child(if m.lesson.is_some() { "Lesson" } else { m.kind.label() })
                        .when_some(m.employee_name.clone(), |el, n| el.child(div().text_color(c(theme::MUTED)).child(format!("· {n}"))))
                        .when(m.scope == "global", |el| el.child(div().text_color(c(theme::MUTED)).child("· global")))
                        .when(m.pinned, |el| el.child(ui::diamond(7., theme::ACCENT_TEXT, true)))
                        .when(!badge.is_empty(), |el| el.child(div().text_size(px(10.5)).text_color(c(badge_color)).child(badge))),
                )
                .child(div().font_family(serif.clone()).text_size(px(17.)).line_height(px(24.))
                    .text_color(c(if m.status == MemoryStatus::Active || m.status == MemoryStatus::Proposed { theme::TEXT } else { theme::MUTED }))
                    .child(if m.content.is_empty() { "(content erased)".to_string() } else { m.content.clone() }))
                .when_some(m.lesson.as_ref(), |el, l| el.child(lesson_lines(l)))
                .when_some(conflict, |el, text| el.child(div().text_size(px(12.)).text_color(c(theme::ACCENT_TEXT)).child(format!("Conflicts with: {text}"))))
                .child(div().text_size(px(12.)).text_color(c(theme::MUTED)).child(source))
        };
        let tabs = h().gap(px(4.)).text_size(px(12.)).children(filters.iter().enumerate().map(|(i, label)| {
            let active = i == filter;
            h().id(("mem-filter", i)).gap(px(6.)).px(px(9.)).py(px(4.)).rounded(px(5.)).cursor_pointer()
                .border_1().border_color(c(if active { theme::BORDER } else { theme::BG }))
                .when(active, |el| el.bg(c(theme::SURFACE_2)))
                .text_color(c(if active { theme::TEXT } else { theme::TEXT_2 }))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.memory_filter = i;
                    this.selected = 0;
                    cx.notify();
                }))
                .child(if i == 1 && proposals > 0 { format!("{label} ({proposals})") } else { label.to_string() })
                .child(ui::kbd(&(i + 1).to_string(), cx).opacity(0.5))
        }));
        let header = h().flex_wrap().gap(px(10.)).mt(px(24.)).justify_between()
            .child(tabs)
            .child(
                h().gap(px(8.))
                    .child(ui::button("consolidate-memory", "Consolidate", ButtonKind::Outline, None).h(px(32.)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.consolidate_memory(project, cx))))
                    .child(ui::button("add-memory", "Remember", ButtonKind::Outline, Some(ui::kbd("n", cx))).h(px(32.)).on_click(cx.listener(|_, _: &ClickEvent, w, cx| w.dispatch_action(Box::new(NewItem), cx)))),
            );
        let body: AnyElement = match filter {
            0 => self.render_memory_context(project, cx).into_any_element(),
            1 => {
                v().mt(px(24.)).gap(px(6.))
                    .when(loading, |el| el.child(div().text_color(c(theme::MUTED)).child("Loading memory…")))
                    .when(!loading && items.is_empty(), |el| el.child(div().text_size(px(13.5)).text_color(c(theme::MUTED)).child("No proposals.")))
                    .children(items.iter().take(shown).enumerate().map(|(i, m)| {
                        let id = m.id;
                        let row = card(i, m).bg(c(theme::SURFACE)).border_color(c(if i == selected { theme::FOCUS_RING } else { theme::BORDER })).p(px(14.))
                            .child(
                                h().gap(px(8.)).mt(px(4.))
                                    .child(ui::button(("mem-accept-btn", i), "Accept", ButtonKind::Primary, None).h(px(30.)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.decide_memory(id, true, cx))))
                                    .child(ui::button(("mem-reject-btn", i), "Reject", ButtonKind::Outline, None).h(px(30.)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.decide_memory(id, false, cx)))),
                            );
                        ui::rise(row, ("mem-card", pk.wrapping_add(i)), i)
                    }))
                    .into_any_element()
            }
            _ => {
                let history = items.len();
                v().mt(px(24.)).gap(px(28.))
                    .child(
                        v().gap(px(2.))
                            .child(h().justify_between().pb(px(10.)).border_b_1().border_color(c(theme::BORDER)).child(ui::eyebrow("HISTORY")).child(ui::eyebrow(history.to_string())))
                            .when(loading, |el| el.child(div().py(px(14.)).text_color(c(theme::MUTED)).child("Loading memory…")))
                            .when(history == 0 && !loading, |el| el.child(div().py(px(14.)).text_color(c(theme::MUTED)).text_size(px(13.)).child("Nothing superseded, rejected or forgotten yet.")))
                            .children(items.iter().take(shown).enumerate().map(|(i, m)| ui::rise(card(i, m), ("mem-card", pk.wrapping_add(i)), i))),
                    )
                    .into_any_element()
            }
        };
        v().child(header).child(body).child(
            h().mt(px(20.)).gap(px(14.)).flex_wrap().text_size(px(11.5)).text_color(c(theme::MUTED))
                .child(div().child("Selected memory:"))
                .child(ui::hint("mem-accept", "y", "Accept proposal", || Box::new(AcceptMemory), cx))
                .child(ui::hint("mem-remove", "delete", "Reject / forget", || Box::new(RemoveMemory), cx))
                .child(ui::hint("mem-pin", "b", "Pin", || Box::new(TogglePin), cx))
                .child(ui::hint("mem-revise", "f2", "Revise", || Box::new(EditItem), cx))
                .child(div().child("· right-click a memory for all actions")),
        )
    }

    /// "In context": what goes into one manager's context, with token counts.
    fn render_memory_context(&mut self, project: aurelius_domain::ProjectId, cx: &mut Context<Self>) -> impl IntoElement {
        let serif = ui::serif(cx);
        let manager = self.memory_ctx_manager_for(project);
        let choices: Vec<(Option<aurelius_domain::EmployeeId>, String)> = self.managers.iter().filter(|m| m.projects_active.contains(&project))
            .map(|m| (Some(m.id), m.name.clone())).chain(std::iter::once((None, "No employee".to_string()))).collect();
        let picker = h().mt(px(20.)).gap(px(4.)).flex_wrap().text_size(px(12.)).items_center()
            .child(div().mr(px(6.)).text_color(c(theme::MUTED)).child("Context of"))
            .children(choices.into_iter().enumerate().map(|(i, (id, name))| {
                let active = id == manager;
                div().id(("mem-ctx-manager", i)).px(px(9.)).py(px(4.)).rounded(px(5.)).cursor_pointer()
                    .border_1().border_color(c(if active { theme::BORDER } else { theme::BG }))
                    .when(active, |el| el.bg(c(theme::SURFACE_2)))
                    .text_color(c(if active { theme::TEXT } else { theme::TEXT_2 }))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.pick_memory_ctx_manager(id);
                        cx.notify();
                    }))
                    .child(name)
            }));
        let note = |text: String| div().mt(px(20.)).text_size(px(13.5)).text_color(c(theme::MUTED)).child(text).into_any_element();
        let Some(view) = self.memory_ctx_view(project).cloned() else {
            let failed = match self.memory_ctx.as_ref() {
                Some((p, m, Err(e))) if *p == project && *m == manager => Some(e.clone()),
                _ => None,
            };
            return v().child(picker).child(note(failed.unwrap_or_else(|| "Loading what goes into the context…".into())));
        };
        let selected = self.selected;
        let over_open = self.memory_over_open;
        let dups_open = self.memory_dups_open;
        let rows: Vec<aurelius_domain::MemoryId> = self.memory_ctx_rows(project).iter().map(|r| r.memory_id).collect();
        // A lesson's when and evidence come from the memory list (the context view has only its text).
        let lessons: Vec<(aurelius_domain::MemoryId, aurelius_domain::memory::LessonLocator)> = self.memory.as_ref().filter(|(p, _)| *p == project)
            .map(|(_, l)| l.iter().filter_map(|m| m.lesson.clone().map(|lesson| (m.id, lesson))).collect()).unwrap_or_default();
        let item_row = |item: &aurelius_protocol::MemoryContextItem, opacity: f32| {
            let lesson = lessons.iter().find(|(id, _)| *id == item.memory_id).map(|(_, l)| lesson_lines(l));
            let i = rows.iter().position(|id| *id == item.memory_id).unwrap_or(usize::MAX);
            let is_selected = i == selected;
            v().id(("mem-ctx", i)).py(px(10.)).px(px(10.)).gap(px(4.)).rounded(px(6.)).border_1().cursor_pointer().opacity(opacity)
                .border_color(c(if is_selected { theme::FOCUS_RING } else { theme::BG }))
                .on_mouse_down(MouseButton::Right, cx.listener(move |this, e: &MouseDownEvent, w, cx| {
                    this.selected = i;
                    let items = this.memory_menu();
                    this.open_menu(e.position, items, w, cx);
                }))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.selected = i;
                    cx.notify();
                }))
                .child(
                    h().gap(px(12.)).items_start().justify_between()
                        .child(div().flex_1().min_w_0().font_family(serif.clone()).text_size(px(16.)).line_height(px(23.)).text_color(c(theme::TEXT)).child(item.content.clone()))
                        .child(div().flex_none().text_size(px(12.)).text_color(c(theme::TEXT_2)).child(format!("~{} tok", item.tokens))),
                )
                .children(lesson)
                .child(
                    h().gap(px(8.)).text_size(px(11.5)).text_color(c(theme::MUTED))
                        .child(item.label.clone())
                        .when(item.pinned, |el| el.child(ui::diamond(7., theme::ACCENT_TEXT, true))),
                )
        };
        let section = |title: String, count: usize, explain: Option<&'static str>, body: Vec<AnyElement>| {
            v().mt(px(26.)).gap(px(4.))
                .child(h().justify_between().pb(px(8.)).border_b_1().border_color(c(theme::BORDER))
                    .child(h().gap(px(6.)).items_center().child(ui::eyebrow(title.clone())).when_some(explain, |el, t| el.child(ui::info(SharedString::from(format!("memory-about-{title}")), t))))
                    .child(ui::eyebrow(count.to_string())))
                .children(body)
        };
        let fold = |id: &'static str, open: bool, label: String, set: bool, which_over: bool| {
            div().id(id).mt(px(26.)).pb(px(8.)).border_b_1().border_color(c(theme::BORDER)).cursor_pointer()
                .text_size(px(12.)).text_color(c(theme::TEXT_2))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    if which_over { this.memory_over_open = set } else { this.memory_dups_open = set }
                    cx.notify();
                }))
                .child(format!("{} {label}", if open { "▾" } else { "▸" }))
        };
        let always: Vec<_> = view.items.iter().filter(|i| i.placement == "always").collect();
        let relevant: Vec<_> = view.items.iter().filter(|i| i.placement == "when_relevant").collect();
        let over: Vec<_> = view.items.iter().filter(|i| i.placement == "over_budget").collect();
        let dups: Vec<_> = view.items.iter().filter(|i| i.placement == "duplicate").collect();
        let budget = view.memory_budget_tokens.max(1);
        let fraction = (view.always_tokens as f32 / budget as f32).clamp(0., 1.);
        let summary = v().mt(px(18.)).gap(px(8.))
            .child(div().text_size(px(13.)).text_color(c(theme::TEXT_2)).child({
                let memory = format!("Memory {} / {} tokens", thousands(view.always_tokens), thousands(view.memory_budget_tokens));
                // 0 means "not known" (no manager): no instructions figure then.
                match view.system_tokens {
                    0 => memory,
                    n => format!("Instructions ~{} tokens · {memory}", thousands(n)),
                }
            }))
            .child(div().h(px(4.)).w(px(320.)).rounded(px(2.)).bg(c(theme::SURFACE_2))
                .child(div().h_full().w(px(320. * fraction)).rounded(px(2.)).bg(c(if view.always_tokens > view.memory_budget_tokens { theme::NEGATIVE } else { theme::ACCENT }))));
        let nothing = always.is_empty() && relevant.is_empty() && over.is_empty() && dups.is_empty();
        let over_folded = over.len() > 5 && !over_open;
        v().child(picker).child(summary)
            .when(nothing, |el| el.child(note("No memory yet for this employee. Use Remember, or accept proposals.".into())))
            .child(section(format!("ALWAYS IN CONTEXT · {} · {} TOKENS", always.len(), thousands(view.always_tokens)), always.len(), None,
                always.iter().map(|i| item_row(i, 1.).into_any_element()).collect()))
            .when(!relevant.is_empty(), |el| el.child(section(format!("ONLY WHEN RELEVANT TO THE QUESTION · {}", relevant.len()), relevant.len(),
                Some("Added only when the question matches and budget is left."),
                relevant.iter().map(|i| item_row(i, 0.75).into_any_element()).collect())))
            .when(!over.is_empty() && !over_folded, |el| el.child(section(format!("DOES NOT FIT THE BUDGET · {}", over.len()), over.len(),
                Some("Not sent; the employee can still find these with recall."),
                over.iter().map(|i| item_row(i, 0.55).into_any_element()).collect())))
            .when(over_folded, |el| el.child(fold("mem-over-fold", false, format!("Does not fit the budget · {}", over.len()), true, true)))
            .when(over.len() > 5 && over_open, |el| el.child(fold("mem-over-unfold", true, "Fold the budget list above".into(), false, true)))
            .when(!dups.is_empty() && !dups_open, |el| el.child(fold("mem-dup-fold", false, format!("Duplicates · {}", dups.len()), true, false)))
            .when(!dups.is_empty() && dups_open, |el| el
                .child(fold("mem-dup-unfold", true, format!("Duplicates · {}", dups.len()), false, false))
                .children(dups.iter().map(|i| item_row(i, 0.45).into_any_element())))
    }
}

/// "2 approvals" / "1 approval" (digits, for badges and footers).
fn counted(n: u32, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// How long something runs, coarse so it needs no ticking clock: "just now", "12 min", "2 h 05 min".
pub(crate) fn since_label(ms: i64) -> String {
    let minutes = ms.max(0) / 60_000;
    match minutes {
        0 => "just now".into(),
        1..=59 => format!("{minutes} min"),
        _ => format!("{} h {:02} min", minutes / 60, minutes % 60),
    }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    let word = match n {
        1 => "One",
        2 => "Two",
        3 => "Three",
        _ => "",
    };
    if word.is_empty() { format!("{n} {many}") } else if n == 1 { format!("{word} {one}") } else { format!("{word} {many}") }
}

fn small(text: impl Into<SharedString>) -> Div {
    div().text_size(px(11.5)).text_color(c(theme::MUTED)).child(text.into())
}

fn quote(text: &'static str, serif: &SharedString, size: f32) -> Div {
    div()
        .font_family(serif.clone())
        .italic()
        .text_size(px(size))
        .line_height(px(size * 1.45))
        .text_color(c(theme::TEXT_2))
        .border_l_1()
        .border_color(c(theme::BORDER_STRONG))
        .pl(px(12.))
        .max_w(px(440.))
        .child(format!("“{text}”"))
}

fn side_panel() -> Div {
    v().size_full()
        .px(px(26.))
        .py(px(24.))
        .gap(px(20.))
        .border_l_1()
        .border_color(c(theme::BORDER))
        .bg(c(theme::PANEL))
}

fn c_muted() -> Hsla {
    c(theme::MUTED)
}

fn c_text(strong: bool) -> Hsla {
    c(if strong { theme::ACCENT_TEXT } else { theme::TEXT_2 })
}

fn timeline_row(marker: AnyElement, body: Div, last: bool) -> Div {
    h().items_start()
        .gap(px(12.))
        .child(
            v().flex_none().w(px(18.)).h_full().items_center()
                .child(div().h(px(17.)).flex().items_center().child(marker))
                .when(!last, |el| el.child(div().flex_1().w(px(1.)).min_h(px(14.)).bg(c(theme::BORDER)))),
        )
        .child(body.flex_1().min_w_0().pb(px(if last { 0. } else { 14. })))
}

/// 1234 -> "1,234".
fn thousands(n: u32) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}
