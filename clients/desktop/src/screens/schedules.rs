//! Settings → Advanced → Recurring schedules: all recurring tasks, with their project.
//! A recurring task makes a normal task for an employee every N hours, daily or on chosen weekdays.

use crate::app::AppView;
use crate::keymap::*;
use crate::theme::{self, c};
use crate::ui::{self, ButtonKind, h, v};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

/// Settings → Advanced → Recurring schedules.
pub const SCHEDULES_TAB: usize = 11;

impl AppView {
    /// Settings → Advanced → Recurring schedules: every recurring task of every project with its
    /// project, recurrence, next and last time, and a detail panel with Edit, Pause / Resume, Run now
    /// and Delete.
    pub fn render_schedules_settings(&mut self, page: Div, cx: &mut Context<Self>) -> Div {
        let pk = self.page_key();
        let serif = ui::serif(cx);
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or_default();
        let schedules: Vec<aurelius_protocol::TaskScheduleView> = self.schedules.clone();
        let selected = self.settings_selected.min(schedules.len().saturating_sub(1));
        let project_name = |id: aurelius_domain::ProjectId| self.projects.iter().find(|p| p.id == id).map_or_else(|| "(deleted project)".to_string(), |p| p.name.clone());
        let last_line = |s: &aurelius_protocol::TaskScheduleView| -> String {
            let mut parts = Vec::new();
            if let Some(at) = s.last_run_ms {
                parts.push(format!("Last run {}", crate::app::when_label(at, now)));
            }
            if let Some(at) = s.last_skipped_ms {
                parts.push(format!("skipped {} (previous task still open)", crate::app::when_label(at, now)));
            }
            if parts.is_empty() { "Not run yet".to_string() } else { parts.join(" · ") }
        };

        let list = v().mt(px(14.))
            .when(schedules.is_empty(), |el| {
                el.child(
                    v().py(px(40.)).items_center().gap(px(10.))
                        .child(h().gap(px(8.)).items_center()
                            .child(div().font_family(serif.clone()).italic().text_size(px(19.)).text_color(c(theme::TEXT_2)).child("No recurring tasks yet."))
                            .child(ui::info("schedules-empty-about", "A recurring task makes a normal task for an employee every N hours, daily or on chosen weekdays.")))
                        .child(ui::button("empty-new-schedule", "New recurring task", ButtonKind::Outline, None).h(px(28.)).on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.pick_schedule_project(w, cx)))),
                )
            })
            .children(schedules.iter().enumerate().map(|(i, s)| {
                let is_selected = i == selected;
                let when = if s.paused { "Paused".to_string() } else { s.next_run_ms.map(|at| format!("Next {}", crate::app::when_label(at, now))).unwrap_or_default() };
                ui::rise(v().id(("rsched", i))
                    .on_mouse_down(MouseButton::Right, cx.listener(move |this, e: &MouseDownEvent, w, cx| {
                        this.settings_selected = i;
                        let items = this.schedule_menu();
                        if !items.is_empty() {
                            this.open_menu(e.position, items, w, cx);
                        }
                    })).px(px(16.)).py(px(13.)).gap(px(6.)).rounded(px(8.)).cursor_pointer()
                    .border_1().border_color(c(if is_selected { theme::FOCUS_RING } else { theme::BG }))
                    .when(is_selected, |el| el.bg(c(theme::SURFACE)))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.settings_selected = i;
                        cx.notify();
                    }))
                    .child(
                        h().gap(px(10.))
                            .child(ui::ring(6., if s.paused { theme::MUTED } else { theme::ACCENT }))
                            .child(ui::row_title().text_size(px(14.)).when(is_selected, |el| el.font_weight(FontWeight::MEDIUM)).child(s.title.clone()))
                            .child(div().flex_none().text_size(px(12.)).text_color(c(if s.paused { theme::MUTED } else { theme::ACCENT_TEXT })).child(when)),
                    )
                    .child(
                        h().pl(px(16.)).gap(px(10.)).text_size(px(12.)).text_color(c(theme::MUTED))
                            .child(div().flex_none().max_w(px(160.)).truncate().text_color(c(theme::ACCENT_TEXT)).child(project_name(s.project_id)))
                            .child(div().min_w_0().truncate().child(format!("{} · {}", s.employee_name, crate::app::recurrence_label(&s.recurrence))))
                            .child(ui::row_title().child(last_line(s))),
                    ), ("sched-row", pk.wrapping_add(i)), i)
            }));

        let detail: AnyElement = match schedules.get(selected) {
            None => v().size_full().border_l_1().border_color(c(theme::BORDER)).bg(c(theme::PANEL)).p(px(26.)).gap(px(14.)).child(ui::eyebrow("RECURRING TASK")).child(div().text_color(c(theme::MUTED)).child("Select a recurring task.")).into_any_element(),
            Some(s) => {
                let run_mode = s.run.mode.map(|m| m.label().to_string()).unwrap_or_else(|| "Employee default".into());
                let run_model = s.run.model.as_deref().map(AppView::friendly_model).unwrap_or_else(|| "Employee default".into());
                let run_effort = s.run.effort.as_deref().map(|e| {
                    let mut chars = e.chars();
                    chars.next().map(|f| f.to_uppercase().chain(chars).collect::<String>()).unwrap_or_default()
                }).unwrap_or_else(|| "Employee default".into());
                let row = |label: &'static str, value: String| {
                    h().gap(px(12.)).items_start().text_size(px(12.5)).child(div().w(px(90.)).flex_none().text_color(c(theme::MUTED)).child(label)).child(div().flex_1().min_w_0().child(value))
                };
                let open_task = s.open_task_id.or(s.last_task_id);
                let paused = s.paused;
                let folder_manager = Some(s.employee_id);
                v().size_full().border_l_1().border_color(c(theme::BORDER)).bg(c(theme::PANEL))
                    .child(
                        div().id("schedule-detail").size_full().overflow_y_scrollbar().px(px(26.)).py(px(24.)).flex().flex_col().gap(px(18.))
                            .child(
                                h().justify_between().child(ui::eyebrow("RECURRING TASK"))
                                    .child(h().gap(px(6.))
                                        .child(div().text_size(px(11.5)).text_color(c(if paused { theme::MUTED } else { theme::ACCENT_TEXT })).child(if paused { "Paused" } else { "Active" }))
                                        .child(ui::more_button("schedule-more").on_click(cx.listener(|this, e: &ClickEvent, w, cx| {
                                            let items = this.schedule_menu();
                                            this.open_menu(e.position(), items, w, cx);
                                        })))),
                            )
                            .child(
                                v().gap(px(6.)).flex_none()
                                    .child(div().font_family(serif.clone()).text_size(px(24.)).line_height(px(29.)).child(s.title.clone()))
                                    .child(div().text_size(px(12.5)).text_color(c(theme::TEXT_2)).child(format!("{} · {} · {}", project_name(s.project_id), s.employee_name, crate::app::recurrence_label(&s.recurrence)))),
                            )
                            .child(
                                v().gap(px(8.)).flex_none().px(px(14.)).py(px(12.)).rounded(px(8.)).border_1().border_color(c(theme::BORDER)).bg(c(theme::SURFACE))
                                    .child(row("Next run", if paused { "Paused: nothing is made until you resume it".into() } else { s.next_run_ms.map(|at| crate::app::when_label(at, now)).unwrap_or_else(|| "Not planned".into()) }))
                                    .child(row("Last run", s.last_run_ms.map(|at| crate::app::when_label(at, now)).unwrap_or_else(|| "Not run yet".into())))
                                    .when_some(s.last_skipped_ms, |el, at| el.child(row("Skipped", format!("{}: the previous task was still open", crate::app::when_label(at, now)))))
                                    .child(row("Times", "The server's local time".into())),
                            )
                            .when_some(open_task, |el, task| {
                                let open = s.open_task_id.is_some();
                                el.child(
                                    h().id("schedule-open-task").gap(px(6.)).flex_none().cursor_pointer().text_size(px(12.5)).text_color(c(theme::ACCENT_TEXT))
                                        .hover(|st| st.text_color(c(theme::TEXT)))
                                        .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.show_task(task, w, cx)))
                                        .child(if open { "Open the task that is still running →" } else { "Open the last task →" }),
                                )
                            })
                            .child(
                                h().gap(px(8.)).flex_none().flex_wrap()
                                    .child(ui::button("schedule-edit", "Edit", ButtonKind::Outline, None).h(px(28.)).on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.edit_schedule(w, cx))))
                                    .child(ui::button("schedule-pause", if paused { "Resume" } else { "Pause" }, ButtonKind::Outline, None).h(px(28.)).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_schedule_paused(cx))))
                                    .child(ui::button("schedule-run", "Run now", ButtonKind::Outline, None).h(px(28.)).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.run_schedule_now(cx))))
                                    .child(ui::button("schedule-delete", "Delete", ButtonKind::Outline, None).h(px(28.)).text_color(c(theme::NEGATIVE)).on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.ask_delete_schedule(w, cx)))),
                            )
                            .child(v().gap(px(6.)).flex_none().child(ui::eyebrow("GOAL")).child(div().text_size(px(13.)).line_height(px(19.)).child(s.goal.clone())))
                            .child(
                                v().gap(px(6.)).flex_none().child(ui::eyebrow("ACCEPTANCE CRITERIA"))
                                    .when(s.criteria.is_empty(), |el| el.child(div().text_size(px(12.5)).text_color(c(theme::MUTED)).child("No explicit criteria.")))
                                    .children(s.criteria.iter().map(|crit| h().gap(px(8.)).items_start().text_size(px(12.5)).child(div().w(px(14.)).text_color(c(theme::MUTED)).child("○")).child(div().flex_1().child(crit.clone())))),
                            )
                            .child(
                                v().gap(px(6.)).flex_none().child(ui::eyebrow("EACH TASK RUNS WITH"))
                                    .child(row("Mode", run_mode)).child(row("Model", run_model)).child(row("Effort", run_effort)),
                            )
                            .when(!s.attachments.is_empty(), |el| {
                                el.child(v().gap(px(8.)).flex_none().child(ui::eyebrow("ATTACHMENTS")).child(self.attachment_chips("sched-att", &s.attachments, s.project_id, folder_manager, None, cx)))
                            }),
                    )
                    .into_any_element()
            }
        };

        let new_button = ui::button("new-schedule", "New recurring task", ButtonKind::Outline, None)
            .h(px(28.))
            .on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.pick_schedule_project(w, cx)));
        page.child(h().px(px(22.)).py(px(14.)).gap(px(14.)).border_b_1().border_color(c(theme::BORDER)).child(self.settings_title(cx)).child(new_button))
            .child(
                h().flex_1().min_h_0().min_w_0().items_start()
                    .child(
                        div().id("schedules-left").flex_1().h_full().min_w_0().overflow_x_hidden().overflow_y_scroll()
                            .flex().flex_col().px(px(22.)).pb(px(30.))
                            .child(div().flex_none().w_full().min_w_0().child(list)),
                    )
                    .child(div().flex_none().w(px(420.)).h_full().child(detail)),
            )
    }
}
