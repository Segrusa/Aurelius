//! The employee page (profile, its settings, its tasks across projects and their
//! conversations; the user talks with it in the Team chat), the right panel (a task or
//! conversation opened from another page) and 11 task history.

use super::chat::{browser_chip, composer, mode_chip, screen_chip, user_bubble};
use crate::app::{AppView, ManagerRef, NeedsYou, Panel, ProjectRef, Page};
use aurelius_domain::employee::LifecycleStatus;
use crate::keymap::*;
use crate::preview::{self, Step};
use crate::theme::{self, c};
use crate::resize::Edge;
use crate::ui::{self, ButtonKind, h, v};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use gpui_kit::component::scroll::ScrollableElement as _;

impl AppView {
    pub fn render_manager(&mut self, key: ManagerRef, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(manager) = self.manager_card(key) else {
            return v().flex_1().items_center().justify_center().text_color(c(theme::MUTED)).child("This employee no longer exists (archived?). Esc goes back.").into_any_element();
        };
        let project_name = key.sample_project().map(|p| self.project_name(p)).unwrap_or_default();
        let real = matches!(key, ManagerRef::Real(_));
        let paused = manager.status == LifecycleStatus::Paused;
        // In a task conversation the header names the task and how it stands.
        let open_task = if real { self.current_manager_thread().and_then(|id| self.tasks.iter().find(|t| t.conversation_id == Some(id))) } else { None };
        let task_line = open_task.map(|t| {
            h().gap(px(8.)).min_w_0()
                .child(div().truncate().text_size(px(12.)).text_color(c(theme::TEXT_2)).child(t.title.clone()))
                .child(ui::task_mark(self.task_mark(t), "header-task-mark"))
        });

        let header = h()
            .h(px(68.))
            .flex_none()
            .px(px(28.))
            .gap(px(14.))
            .border_b_1()
            .border_color(c(theme::BORDER))
            .child(ui::monogram(&manager.name, 36., cx))
            .child(
                v().flex_1().min_w_0()
                    .when(!real, |el| el.child(h().gap(px(8.)).text_size(px(12.)).text_color(c(theme::MUTED)).child(format!("{project_name} /")).child(ui::preview_tag(cx))))
                    .child(h().gap(px(5.)).items_baseline().child(div().text_size(px(15.)).font_weight(FontWeight::MEDIUM).child(manager.name.clone())).child(div().text_size(px(13.)).text_color(c(theme::TEXT_2)).child(match &manager.title { Some(title) => format!("· {title} · {}", manager.role), None => format!("· {}", manager.role) })))
                    .children(task_line),
            )
            .when(real, |el| el.children(self.usage_total_chip()))
            .child(if paused {
                h().gap(px(7.)).text_size(px(12.5)).text_color(c(theme::MUTED)).child(ui::ring(6., theme::MUTED)).child("Paused")
            } else if manager.working {
                h().gap(px(7.)).text_size(px(12.5)).child(ui::pulse_dot("manager-working", 6., theme::ACCENT)).child("Working")
            } else {
                h().gap(px(7.)).text_size(px(12.5)).text_color(c(theme::TEXT_2)).child(ui::ring(6., theme::TEXT_2)).child("Available")
            })
            .when(real, |el| {
                let folder = self.real_manager(manager.key).and_then(|m| m.folder.clone());
                let label = folder.unwrap_or_else(|| "Own folder: none (its chat has no work folder; a task works in its project folder)".into());
                let employee_id = match manager.key { ManagerRef::Real(id) => Some(id), _ => None };
                let pinned = employee_id.is_some_and(|id| self.employee_pins.contains(&id));
                el.when_some(employee_id, |el, id| {
                    el.child(
                        ui::icon_button("manager-pin")
                            .h(px(30.))
                            .tooltip(ui::tip(if pinned { "Unpin from the sidebar" } else { "Pin to the sidebar" }, None))
                            .aria_label(if pinned { "Unpin from the sidebar" } else { "Pin to the sidebar" })
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_employee_pin(id, cx)))
                            .child(ui::pin(pinned, 13., if pinned { theme::ACCENT_TEXT } else { theme::MUTED })),
                    )
                }).child(
                    ui::icon_button("manager-folder")
                        .h(px(30.))
                        .tooltip(ui::tip(label.clone(), Some("secondary-shift-g")))
                        .aria_label(label)
                        .on_click(cx.listener(|_, _: &ClickEvent, w, cx| w.dispatch_action(Box::new(ChangeProjectFolder), cx)))
                        .child(ui::folder_glyph(theme::MUTED)),
                )
                .child(ui::more_button("manager-menu").h(px(30.)).on_click(cx.listener(|this, e: &ClickEvent, w, cx| {
                    let items = this.manager_menu();
                    this.open_menu(e.position(), items, w, cx);
                })))
            })
            .when(!real, |el| el.child(
                ui::button("history", "History", ButtonKind::Outline, Some(ui::kbd("g", cx)))
                    .h(px(30.))
                    .when(self.history_open, |el| el.bg(c(theme::SURFACE_2)).border_color(c(theme::BORDER_STRONG)))
                    .on_click(cx.listener(|_, _: &ClickEvent, w, cx| w.dispatch_action(Box::new(ToggleHistory), cx))),
            ));

        // Its own settings, for all its work: mode (the default for new work), browser and screen.
        // What its runs get on top of its instructions (GL-05 AC2, GL-07).
        let guidelines = match manager.key {
            ManagerRef::Real(id) => self.guidelines_chip(id, cx),
            ManagerRef::Sample(..) => None,
        };
        let settings = self.real_manager(manager.key).map(|m| {
            let (id, mode, browser, screen) = (m.id, m.agent_mode, m.browser_allowed, m.screen_allowed);
            let task_open = self.current_manager_thread().is_some();
            h().flex_none().px(px(28.)).py(px(8.)).gap(px(10.)).gap_y(px(4.)).flex_wrap().border_b_1().border_color(c(theme::BORDER))
                .text_size(px(12.5)).text_color(c(theme::TEXT_2))
                .child(div().text_color(c(theme::MUTED)).child("Settings"))
                .child(div().id("employee-mode").tooltip(ui::tip("Its mode for new work", None)).child(
                    // Shift+Tab switches the open task's mode, else this one.
                    mode_chip("employee-mode-chip", mode, (!task_open).then_some("shift-tab"), cx, move |this, _, cx| this.cycle_employee_mode(id, cx)),
                ))
                .child(browser_chip(browser, cx))
                .child(screen_chip(screen, cx))
                .children(guidelines)
                .child(div().flex_1())
                .child(
                    h().id("employee-team-chat").gap(px(6.)).cursor_pointer().text_color(c(theme::ACCENT_TEXT)).hover(|s| s.text_color(c(theme::TEXT)))
                        .on_click(cx.listener(|_, _: &ClickEvent, w, cx| w.dispatch_action(Box::new(GoTeamChat), cx)))
                        .child(format!("Talk with {} in Team chat →", m.name))
                        .child(ui::kbd("secondary-8", cx).opacity(0.6)),
                )
        });

        let agents = if real { self.real_manager(manager.key).map(|m| m.agents.clone()).unwrap_or_default() } else { Vec::new() };
        // What runs for the open conversation; its own answer already streams in the chat,
        // its subagents show in the chip above the composer.
        let current = self.current_manager_thread();
        let here: Vec<_> = agents.iter().filter(|a| current.is_some() && a.conversation_id == current && !matches!(a.kind.as_str(), "conversation" | "subagent")).cloned().collect();
        let main = v()
            .flex_1()
            .min_w_0()
            .h_full()
            // Files dropped anywhere on the conversation go with the next message.
            .when(real, |el| {
                el.drag_over::<ExternalPaths>(|style, _, _, _| style.bg(c(theme::SURFACE)))
                    .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| this.drop_files(paths.paths(), cx)))
            })
            .child(header)
            .children(settings)
            .child(self.conversation_pane(move |this, _, cx| this.manager_scroll(key, cx), cx))
            .when(!here.is_empty(), |el| el.child(render_agents(&here, cx)))
            .when(current.is_some() || !real, |el| el.child(composer(self, "", false, cx)));

        let body = h().flex_1()
            .min_h_0()
            .w_full()
            .items_start()
            .child(main)
            .when(self.history_open && !real, |el| {
                let history = self.render_history_panel(&manager.name, cx).into_any_element();
                el.child(self.resizable("task_history", 400., Edge::Left, history, cx))
            })
            .when(real, |el| {
                let threads = self.render_manager_threads(&agents, cx);
                el.child(self.resizable("manager_threads", 270., Edge::Left, threads, cx))
            });
        v().flex_1().min_h_0().w_full().child(body).into_any_element()
    }

    /// The right panel: a task (or conversation) opened from a page, with its conversation
    /// and composer; ↗ opens it on its employee's page, × closes it (Esc too).
    pub fn render_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        if let Panel::Ticket(id) = self.panel {
            return self.render_ticket_panel(id, window, cx);
        }
        let task = self.panel_task().cloned();
        let manager = self.panel_manager().cloned();
        let conversation = self.current_manager_thread();
        let title: SharedString = task.as_ref().map(|t| t.title.clone()).or_else(|| manager.as_ref().map(|m| m.name.clone())).unwrap_or_else(|| "Conversation".into()).into();
        let project = task.as_ref().map(|t| self.project_name(ProjectRef::Real(t.project_id)).to_string()).unwrap_or_default();
        let who = manager.as_ref().map_or_else(|| "Nobody yet".to_string(), |m| m.name.clone());
        let author: SharedString = manager.as_ref().map_or_else(|| "Aurelius".to_string(), |m| m.name.clone()).into();
        // A task opened from a ticket goes back to it (MU1-08 AC4).
        let back = self.board.back_to.and_then(|t| self.board.ticket(t).map(|x| (t, x.number))).filter(|_| matches!(self.panel, Panel::Task(_)));
        let header = h().h(px(56.)).flex_none().px(px(16.)).gap(px(10.)).border_b_1().border_color(c(theme::BORDER))
            .children(back.map(|(ticket, number)| ui::button("panel-back-ticket", format!("‹ #{number}"), ui::ButtonKind::Outline, None).h(px(26.)).px(px(8.))
                .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.open_ticket(ticket, w, cx)))))
            .children(manager.as_ref().map(|m| ui::monogram(&m.name, 28., cx)))
            .child(
                v().flex_1().min_w_0()
                    .child(div().truncate().text_size(px(13.5)).font_weight(FontWeight::MEDIUM).child(title))
                    .child(div().truncate().text_size(px(11.5)).text_color(c(theme::MUTED)).child(if project.is_empty() { who } else { format!("{who} · {project}") })),
            )
            .children(task.as_ref().map(|t| ui::task_mark(self.task_mark(t), "panel-task-mark")))
            .when(manager.is_some() && conversation.is_some(), |el| {
                el.child(ui::icon_button("panel-pop-out").aria_label("Open on the employee's page").tooltip(ui::tip("Open on the employee's page", None)).child("↗")
                    .on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.pop_out_panel(w, cx))))
            })
            .child(ui::icon_button("panel-close").aria_label("Close").tooltip(ui::tip("Close", Some("escape"))).child("×")
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.close_panel(cx))));
        let body: AnyElement = match (conversation, &task) {
            (Some(_), _) => {
                let author = author.clone();
                self.conversation_pane(move |this, _, cx| this.panel_scroll(author.clone(), cx), cx)
            }
            (None, Some(t)) if t.employee_id.is_none() => v().p(px(24.)).gap(px(6.)).text_color(c(theme::MUTED))
                .child("A draft: nobody works on it yet.")
                .child(div().text_size(px(12.)).child("Give it to an employee from the project's Tasks tab (Alt+A)."))
                .into_any_element(),
            _ => div().p(px(24.)).text_color(c(theme::MUTED)).child("Starting…").into_any_element(),
        };
        v().size_full().bg(c(theme::BG)).border_l_1().border_color(c(theme::BORDER))
            .drag_over::<ExternalPaths>(|style, _, _, _| style.bg(c(theme::SURFACE)))
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| this.drop_files(paths.paths(), cx)))
            .child(header)
            .child(body)
            .when(conversation.is_some(), |el| el.child(composer(self, "", false, cx)))
            .into_any_element()
    }

    /// The panel's conversation, in the conversation pane.
    fn panel_scroll(&mut self, author: SharedString, cx: &mut Context<Self>) -> AnyElement {
        ui::scroll_frame(&self.scroll,
            div()
                .id("panel-scroll")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .track_scroll(&self.scroll)
                .flex()
                .flex_col()
                .items_center()
                .px(px(18.))
                .pt(px(18.))
                .pb(px(12.))
                .child(div().flex_none().w_full().flex().justify_center().child(self.render_messages(author, cx))),
        ).flex_1().into_any_element()
    }

    /// The panel's task, full size on its employee's page.
    pub fn pop_out_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(m), Some(conversation)) = (self.panel_manager().map(|m| m.id), self.current_manager_thread()) else { return };
        self.panel = Panel::None;
        self.open_manager(ManagerRef::Real(m), window, cx);
        self.select_manager_thread(conversation, cx);
    }

    /// The manager conversation's scroll area, in the conversation pane.
    fn manager_scroll(&mut self, key: ManagerRef, cx: &mut Context<Self>) -> AnyElement {
        let mono = ui::mono(cx);
        let Some(manager) = self.manager_card(key) else { return div().into_any_element() };
        let real = matches!(key, ManagerRef::Real(_));
        let is_alex = key == ManagerRef::Sample(0, 0);

        let current_work = v()
            .mt(px(4.))
            .px(px(18.))
            .py(px(16.))
            .gap(px(12.))
            .rounded(px(10.))
            .border_1()
            .border_color(c(theme::BORDER))
            .bg(c(theme::SURFACE))
            .child(
                h().justify_between()
                    .child(ui::eyebrow("CURRENT WORK"))
                    .child(if self.history_open {
                        div().font_family(mono.clone()).text_size(px(11.)).text_color(c(theme::MUTED)).child("alex/auth-refresh-lock")
                    } else {
                        ui::eyebrow("3 workers")
                    }),
            )
            .when(!self.history_open, |el| el.child(div().text_size(px(14.)).font_weight(FontWeight::MEDIUM).child("Authentication fix")))
            .child(v().gap(px(9.)).text_size(px(13.)).children(preview::CURRENT_WORK.iter().map(|(step, label, time)| {
                let (mark, color): (AnyElement, u32) = match step {
                    Step::Done => (div().child("✓").into_any_element(), theme::TEXT_2),
                    Step::Running => (ui::dot(6., theme::ACCENT).into_any_element(), theme::TEXT),
                    Step::Pending => (ui::ring(6., theme::MUTED).into_any_element(), theme::MUTED),
                };
                h().gap(px(10.))
                    .text_color(c(color))
                    .child(div().w(px(14.)).flex().justify_center().child(mark))
                    .child(div().flex_1().child(*label))
                    .child(div().font_family(mono.clone()).text_size(px(11.5)).text_color(c(if *step == Step::Done { theme::MUTED } else { theme::TEXT_2 })).child(*time))
            })));

        let conversation = v()
            .w_full()
            .max_w(px(if self.history_open { 620. } else { 700. }))
            .gap(px(22.))
            .when(self.history_open && is_alex, |el| {
                el.child(
                    h().gap(px(12.)).text_size(px(11.5)).text_color(c(theme::MUTED))
                        .child(div().flex_1().h(px(1.)).bg(c(theme::BORDER)))
                        .child("Fix sync conflicts · Sep 29 · merged c9e02b1")
                        .child(div().flex_1().h(px(1.)).bg(c(theme::BORDER))),
                )
            })
            .when(is_alex, |el| {
                el.child(user_bubble(preview::MANAGER_REQUEST))
                    .child(
                        v().gap(px(14.))
                            .child(div().text_size(px(12.5)).font_weight(FontWeight::MEDIUM).text_color(c(theme::TEXT_2)).child(manager.name.clone()))
                            .children(preview::MANAGER_REPLY.iter().map(|p| div().text_size(px(15.)).line_height(px(25.5)).child(*p)))
                            .child(current_work),
                    )
                    .child(user_bubble(preview::MANAGER_FOLLOWUP))
                    .child(
                        v().gap(px(8.))
                            .child(div().text_size(px(12.5)).font_weight(FontWeight::MEDIUM).text_color(c(theme::TEXT_2)).child(manager.name.clone()))
                            .child(div().text_size(px(15.)).line_height(px(25.5)).child(preview::MANAGER_ACK))
                            .child(
                                h().id("saved-memory").gap(px(5.)).text_size(px(12.)).text_color(c(theme::MUTED))
                                    .child("Saved to memory"),
                            ),
                    )
            })
            .when(!is_alex, |el| {
                el.child(v().gap(px(8.)).child(div().text_size(px(12.5)).font_weight(FontWeight::MEDIUM).text_color(c(theme::TEXT_2)).child(manager.name.clone())).child(div().text_size(px(15.)).line_height(px(25.5)).child(manager.summary.clone())))
            })
            .children(self.manager_extra.iter().map(|text| user_bubble(text.clone())))
            .when(!self.manager_extra.is_empty(), |el| {
                el.child(div().text_size(px(12.)).text_color(c(theme::MUTED)).child("Sample employee: messages are not saved."))
            });
        let conversation: AnyElement = if real && self.current_manager_thread().is_none() {
            // No task (yet): the page has no conversation of its own.
            v().w_full().max_w(px(700.)).gap(px(6.)).text_size(px(13.5)).text_color(c(theme::TEXT_2))
                .child(div().text_size(px(15.)).text_color(c(theme::TEXT)).child(format!("{} has no tasks yet.", manager.name)))
                .child(format!("Tasks you give {} show on the right, each with its conversation. Talk with {} in the Team chat.", manager.name, manager.name))
                .into_any_element()
        } else if real {
            v().w_full().max_w(px(700.)).gap(px(16.))
                .child(self.render_messages(manager.name.clone(), cx))
                .into_any_element()
        } else {
            conversation.into_any_element()
        };

        ui::scroll_frame(&self.scroll,
            div()
                .id("manager-scroll")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .track_scroll(&self.scroll)
                .track_focus(&self.focus)
                .key_context(NAV)
                .flex()
                .flex_col()
                .items_center()
                .px(px(36.))
                .pt(px(26.))
                .pb(px(16.))
                .child(ui::rise(div().flex_none().w_full().flex().justify_center().child(conversation), ("page", self.page_key()), 0)),
        ).flex_1().into_any_element()
    }

    /// One conversation in the right panel. A past one (ended, nothing going on)
    /// is dimmed, back to full strength on hover or when it is the open one.
    fn thread_row(&self, i: usize, id: aurelius_domain::ConversationId, title: SharedString, detail: String, active: bool, past: bool, working: bool, needs: Option<NeedsYou>, mark: Option<ui::TaskMark>, cx: &mut Context<Self>) -> Stateful<Div> {
        let group = SharedString::from(format!("thread-row-{i}"));
        let dim = past && !active;
        h().id(("thread", i))
            .group(group.clone())
            .pl(px(10.))
            .pr(px(4.))
            .py(px(5.))
            .rounded(px(6.))
            .cursor_pointer()
            .text_color(c(if active || !past { theme::TEXT } else { theme::MUTED }))
            .when(dim, |el| el.opacity(0.6))
            .when(active, |el| el.bg(c(theme::SURFACE_2)))
            .hover(move |s| if dim { s.bg(c(theme::SURFACE_2)).opacity(1.) } else { s.bg(c(theme::SURFACE_2)) })
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.select_manager_thread(id, cx)))
            .on_mouse_down(MouseButton::Right, cx.listener(move |this, e: &MouseDownEvent, w, cx| {
                this.select_manager_thread(id, cx);
                let items = this.thread_menu();
                this.open_menu(e.position, items, w, cx);
            }))
            .child(
                v().flex_1().min_w_0()
                    .child(div().truncate().text_size(px(13.)).child(title))
                    .child(div().truncate().text_size(px(11.)).text_color(c(theme::MUTED)).child(detail)),
            )
            .when_some(needs, |el, n| el.child(ui::needs_you_tag(n.label(), n.color()).ml(px(6.))))
            .when(working, |el| el.child(ui::pulse_dot(("thread-dot", i), 6., theme::ACCENT)))
            // A finished task keeps its Done / Failed / Cancelled chip, so it reads without opening it.
            .when_some(mark.filter(|m| m.is_finished() && needs.is_none() && !working), |el, m| el.child(ui::task_mark(m, ("thread-mark", i)).ml(px(6.))))
            .child(
                ui::more_button(("thread-more", i))
                    .h(px(20.))
                    .when(!active, |b| b.opacity(0.).group_hover(group.clone(), |s| s.opacity(1.)))
                    .on_click(cx.listener(move |this, e: &ClickEvent, w, cx| {
                        cx.stop_propagation();
                        this.select_manager_thread(id, cx);
                        let items = this.thread_menu();
                        this.open_menu(e.position(), items, w, cx);
                    })),
            )
    }

    /// The employee's tasks across projects, on the right, each opening its conversation
    /// (a dot marks those at work).
    fn render_manager_threads(&self, agents: &[aurelius_protocol::AgentView], cx: &mut Context<Self>) -> AnyElement {
        let Page::Employee(key) = self.screen else { return div().into_any_element() };
        let Some(m) = self.real_manager(key) else { return div().into_any_element() };
        let current = self.current_manager_thread();
        let working: std::collections::HashSet<aurelius_domain::ConversationId> = agents.iter().filter_map(|a| a.conversation_id).collect();
        // Its tasks' conversations come from the task list: protocol 3 lists only its chats on the employee.
        let mut task_threads: Vec<(aurelius_domain::ConversationId, &aurelius_protocol::TaskView)> =
            self.tasks.iter().filter(|t| t.employee_id == Some(m.id)).filter_map(|t| t.conversation_id.map(|c| (c, t))).collect();
        // Those at work on top, then the latest change first.
        task_threads.sort_by_key(|(c, t)| (std::cmp::Reverse(working.contains(c)), std::cmp::Reverse(t.updated_at_ms)));
        let mut panel = v().id("manager-threads")
            .size_full()
            .overflow_y_scrollbar()
            .bg(c(theme::SIDEBAR))
            .border_l_1()
            .border_color(c(theme::BORDER))
            .px(px(10.))
            .py(px(14.))
            .gap(px(2.));
        let now = crate::screens::chat::now_ms();
        let task_groups: Vec<(usize, aurelius_domain::ConversationId, &aurelius_protocol::TaskView, ThreadGroup)> = task_threads.iter().enumerate().map(|(i, (id, t))| {
            let ongoing = ui::is_active(t) || working.contains(id) || self.needs_you(*id).is_some();
            (i, *id, *t, thread_group(ongoing, current == Some(*id), t.updated_at_ms, now))
        }).collect();
        let tasks_active = task_groups.iter().filter(|(.., g)| *g == ThreadGroup::Active).count();
        // How many (accent while some are active).
        let tasks_count = if tasks_active > 0 { format!("{tasks_active} active · {}", task_threads.len()) } else { task_threads.len().to_string() };
        panel = panel.child(
            h().gap(px(6.)).px(px(10.)).py(px(5.)).pb(px(8.)).mb(px(4.)).border_b_1().border_color(c(theme::BORDER)).text_size(px(13.))
                .child("Tasks")
                .child(div().text_size(px(11.5)).text_color(c(if tasks_active > 0 { theme::ACCENT_TEXT } else { theme::MUTED })).child(tasks_count)),
        );

        // Within a section, what is going on comes first; below "Past" what ended
        // today, dimmed; the rest folds under "Earlier".
        let nothing_active = || div().px(px(10.)).py(px(4.)).text_size(px(11.5)).text_color(c(theme::MUTED)).child("Nothing active");
        let group_divider = |id: &'static str, label: &'static str| {
            h().id(id)
                .mt(px(6.))
                .mb(px(2.))
                .pl(px(10.))
                .gap(px(8.))
                .child(div().text_size(px(10.5)).text_color(c(theme::MUTED)).child(label))
                .child(div().flex_1().h(px(1.)).bg(c(theme::BORDER)))
        };
        let earlier = |id: &'static str, count: usize, open: bool, toggle: fn(&mut AppView), cx: &mut Context<Self>| {
            h().id(id)
                .mt(px(4.))
                .pl(px(10.))
                .py(px(3.))
                .gap(px(6.))
                .rounded(px(5.))
                .cursor_pointer()
                .text_size(px(11.5))
                .text_color(c(theme::MUTED))
                .hover(|s| s.bg(c(theme::SURFACE_2)))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    toggle(this);
                    cx.notify();
                }))
                .child(ui::chevron(open, 9., theme::MUTED))
                .child(format!("Earlier · {count}"))
        };

        {
            if task_threads.is_empty() {
                panel = panel.child(div().px(px(10.)).py(px(4.)).text_size(px(11.5)).text_color(c(theme::MUTED)).child("Tasks you give this employee show here, from every project."));
            } else if tasks_active == 0 {
                panel = panel.child(nothing_active());
            }
            let task_row = |this: &Self, i: usize, id: aurelius_domain::ConversationId, task: &aurelius_protocol::TaskView, past: bool, cx: &mut Context<Self>| {
                let mark = this.task_mark(task);
                let project = this.project_name(ProjectRef::Real(task.project_id));
                this.thread_row(i, id, task.title.clone().into(), format!("{} · {project} · {}", mark.label(), ui::fmt_when(task.updated_at_ms)), current == Some(id), past, working.contains(&id), this.needs_you(id), Some(mark), cx)
            };
            for (i, id, t, _) in task_groups.iter().filter(|(.., g)| *g == ThreadGroup::Active) {
                panel = panel.child(task_row(self, *i, *id, t, false, cx));
            }
            if tasks_active < task_groups.len() {
                panel = panel.child(group_divider("tasks-past", "Past"));
            }
            for (i, id, t, _) in task_groups.iter().filter(|(.., g)| *g == ThreadGroup::Past) {
                panel = panel.child(task_row(self, *i, *id, t, true, cx));
            }
            let older: Vec<_> = task_groups.iter().filter(|(.., g)| *g == ThreadGroup::Earlier).map(|(i, id, t, _)| (*i, *id, *t)).collect();
            if !older.is_empty() {
                let open = self.threads_tasks_earlier_open;
                panel = panel.child(earlier("tasks-earlier", older.len(), open, |this| this.threads_tasks_earlier_open = !this.threads_tasks_earlier_open, cx));
                if open {
                    for (i, id, t) in &older {
                        panel = panel.child(task_row(self, *i, *id, t, true, cx));
                    }
                }
            }
        }

        panel.into_any_element()
    }

    fn render_history_panel(&self, name: &str, cx: &mut Context<Self>) -> impl IntoElement {
        let mono = ui::mono(cx);
        let serif = ui::serif(cx);
        let stat = |value: &'static str, label: &'static str, divider: bool| {
            v().flex_1()
                .px(px(12.))
                .py(px(11.))
                .gap(px(3.))
                .when(divider, |el| el.border_r_1().border_color(c(theme::BORDER)))
                .child(div().font_family(serif.clone()).text_size(px(22.)).child(value))
                .child(div().text_size(px(11.5)).text_color(c(theme::MUTED)).child(label))
        };
        v().size_full()
            .px(px(24.))
            .py(px(22.))
            .gap(px(18.))
            .border_l_1()
            .border_color(c(theme::BORDER))
            .bg(c(theme::PANEL))
            .child(h().justify_between().child(ui::eyebrow(format!("TASK HISTORY · {}", name.to_uppercase()))).child(ui::hint("close-history", "escape", "✕", || Box::new(ToggleHistory), cx)))
            .child(h().rounded(px(8.)).border_1().border_color(c(theme::BORDER)).bg(c(theme::SURFACE)).child(stat("24", "tasks", true)).child(stat("21", "merged", true)).child(stat("1", "reverted", false)))
            .child(v().id("history-body").flex_1().min_h_0().overflow_y_scrollbar().gap(px(18.)).child(
                v().child(ui::eyebrow("IN PROGRESS").pb(px(8.))).children(preview::HISTORY_IN_PROGRESS.iter().map(|(title, branch, current)| {
                    v().px(px(12.))
                        .py(px(11.))
                        .gap(px(4.))
                        .rounded(px(7.))
                        .when(*current, |el| el.border_1().border_color(c(theme::BORDER_STRONG)).bg(c(theme::SURFACE_2)))
                        .child(h().gap(px(9.)).text_size(px(13.5)).child(ui::dot(6., theme::ACCENT)).child(ui::row_title().child(*title)).when(*current, |el| el.child(div().flex_none().text_size(px(11.5)).text_color(c(theme::MUTED)).child("This chat"))))
                        .child(div().pl(px(15.)).truncate().font_family(mono.clone()).text_size(px(11.5)).text_color(c(theme::MUTED)).child(*branch))
                })),
            )
            .child(
                v().child(ui::eyebrow("COMPLETED").pb(px(4.))).children(preview::HISTORY_DONE.iter().enumerate().map(|(i, item)| {
                    v().px(px(12.))
                        .py(px(10.))
                        .gap(px(4.))
                        .when(i + 1 < preview::HISTORY_DONE.len(), |el| el.border_b_1().border_color(c(theme::SURFACE_2)))
                        .child(
                            h().gap(px(10.))
                                .child(div().flex_none().w(px(14.)).text_size(px(12.)).text_color(c(if item.reverted { theme::NEGATIVE } else { theme::TEXT_2 })).child(if item.reverted { "↺" } else { "✓" }))
                                .child(ui::row_title().text_size(px(13.5)).text_color(c(if item.reverted { theme::TEXT_2 } else { theme::TEXT })).child(item.title))
                                .child(div().flex_none().text_size(px(11.5)).text_color(c(theme::MUTED)).child(item.date)),
                        )
                        .child(
                            h().pl(px(24.)).gap(px(5.)).font_family(mono.clone()).text_size(px(11.5))
                                .when(item.reverted, |el| el.text_color(c(theme::MUTED)).child(format!("reverted in {}", item.commit)))
                                .when(!item.reverted, |el| el.child(div().text_color(c(theme::ACCENT_TEXT)).child(item.commit)).child(div().text_color(c(theme::MUTED)).child(format!("· {}", item.diff)))),
                        )
                })),
            ))
            .child(
                h().justify_between().pt(px(14.)).border_t_1().border_color(c(theme::BORDER)).text_size(px(12.5))
                    .child(div().text_color(c(theme::MUTED)).child("Show all 24"))
                    .child(
                        h().id("open-tasks").gap(px(6.)).cursor_pointer().text_color(c(theme::ACCENT))
                            .on_click(cx.listener(|_, _: &ClickEvent, w, cx| w.dispatch_action(Box::new(GoTasks), cx)))
                            .child("Open in Tasks →")
                            .child(ui::kbd("secondary-7", cx).opacity(0.6)),
                    ),
            )
    }
}

/// What runs for the open conversation, in a small card above the composer:
/// task steps, workers, subagents and sub-tasks delegated to colleagues.
fn render_agents(agents: &[aurelius_protocol::AgentView], cx: &mut Context<AppView>) -> impl IntoElement {
    let now = aurelius_storage_now();
    let label = |kind: &str| match kind {
        "conversation" => "Answering",
        "planning" => "Planning",
        "review" => "Reviewing",
        "follow_up" => "Following up",
        "worker" => "Worker",
        "subagent" => "Subagent",
        "delegated" => "Delegated to",
        _ => "Running",
    };
    let rows = agents.iter().enumerate().map(|(i, a)| {
        let minutes = ((now - a.since_ms).max(0) / 60_000) as u64;
        let since = if minutes == 0 { "just now".to_string() } else { format!("{minutes} min") };
        h().id(("agent", i)).gap(px(8.)).px(px(6.)).py(px(3.)).rounded(px(5.)).text_size(px(12.))
            // A subagent belongs to the step above it.
            .when(a.kind == "subagent", |el| el.pl(px(20.)))
            .when(a.task_id.is_some(), |el| {
                el.cursor_pointer().hover(|s| s.bg(c(theme::SURFACE_2))).on_click(cx.listener(|_, _: &ClickEvent, w, cx| w.dispatch_action(Box::new(GoTasks), cx)))
            })
            .child(if a.kind == "delegated" { ui::ring(6., theme::TEXT_2).into_any_element() } else { ui::pulse_dot(("agent-dot", i), 6., theme::ACCENT).into_any_element() })
            .child(div().flex_none().text_color(c(theme::MUTED)).child(label(&a.kind)))
            .child(div().flex_none().max_w(px(220.)).truncate().font_weight(FontWeight::MEDIUM).child(a.name.clone()))
            .child(div().flex_1().min_w_0().truncate().text_color(c(theme::TEXT_2)).child(a.detail.clone()))
            .child(div().flex_none().text_size(px(11.)).text_color(c(theme::MUTED)).child(since))
    });
    h().w_full().justify_center().px(px(40.)).pt(px(10.)).child(
        v().id("at-work").w_full().max_w(px(700.)).max_h(px(160.)).overflow_y_scroll().px(px(8.)).py(px(6.)).gap(px(1.))
            .rounded(px(9.)).border_1().border_color(c(theme::BORDER)).bg(c(theme::SURFACE))
            .children(rows),
    )
}

fn aurelius_storage_now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// Where a task or conversation goes in the manager's panel.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum ThreadGroup {
    /// Going on: a task not ended, at work, or waiting for the user.
    Active,
    /// Ended, shown dimmed below "Past": changed today, or the open one.
    Past,
    /// Ended before today, folded under "Earlier".
    Earlier,
}

/// The open thread is not promoted to Active, but never hides under "Earlier".
pub(crate) fn thread_group(ongoing: bool, open: bool, updated_at_ms: i64, now_ms: i64) -> ThreadGroup {
    if ongoing {
        ThreadGroup::Active
    } else if open || crate::screens::chat::days_ago(updated_at_ms, now_ms) <= 0 {
        ThreadGroup::Past
    } else {
        ThreadGroup::Earlier
    }
}

#[cfg(test)]
mod tests {
    use super::{thread_group, ThreadGroup};

    #[test]
    fn threads_split_into_active_past_and_earlier() {
        let now = 1_759_665_600_000; // 2025-10-05 12:00 UTC
        let hour = 3_600_000;
        assert_eq!(thread_group(true, false, now - 30 * 24 * hour, now), ThreadGroup::Active, "still going on");
        assert_eq!(thread_group(true, false, now - hour, now), ThreadGroup::Active, "going on, changed today");
        assert_eq!(thread_group(false, false, now - hour, now), ThreadGroup::Past, "ended today");
        assert_eq!(thread_group(false, false, now - 30 * hour, now), ThreadGroup::Earlier, "ended yesterday");
        assert_eq!(thread_group(false, true, now - 30 * hour, now), ThreadGroup::Past, "open, ended yesterday");
        assert_eq!(thread_group(false, true, now - hour, now), ThreadGroup::Past, "open is not promoted to active");
    }
}
