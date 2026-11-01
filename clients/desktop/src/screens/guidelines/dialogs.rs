//! The guideline dialogs (`Overlay::Guidelines`): Links, Add starter set, Reject with a
//! note, and the confirmations.

use super::logic::*;
use super::state::*;
use super::view::{level_text, on, tick};
use crate::app::AppView;
use crate::backend::{GuidelineCall, GuidelineWrite};
use crate::theme::{self, c};
use crate::ui::{self, ButtonKind, h, v};
use aurelius_domain::EmployeeId;
use aurelius_domain::employee::LifecycleStatus;
use aurelius_domain::guideline::{GuidelineLevel, GuidelineTarget, GuidelineView};
use aurelius_domain::project::ProjectStatus;
use aurelius_protocol::ProjectSummary;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

impl AppView {
    fn guideline_card(&self, width: f32, max_h: f32) -> Div {
        v().w(px(width)).max_h(px(max_h)).rounded(px(12.)).border_1().border_color(c(theme::BORDER_STRONG)).bg(c(theme::PANEL))
            .track_focus(&self.overlay_focus).key_context(crate::keymap::DIALOG)
    }

    fn guideline_foot(&self, cx: &mut Context<Self>) -> Div {
        h().px(px(22.)).py(px(14.)).gap(px(8.)).justify_end().border_t_1().border_color(c(theme::BORDER))
            .child(ui::button("gl-dialog-cancel", "Cancel", ButtonKind::Outline, Some(ui::kbd("escape", cx))).on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.guideline_close_dialog(w, cx))))
    }

    pub fn render_guidelines_dialog(&mut self, max_h: f32, cx: &mut Context<Self>) -> AnyElement {
        let Some(dialog) = self.guidelines.dialog.clone() else { return div().into_any_element() };
        let serif = ui::serif(cx);
        match dialog {
            Dialog::Links(d) => self.render_links_dialog(d, max_h, cx),
            Dialog::Starter { templates, picked, busy } => {
                let rows: AnyElement = match &templates {
                    None => div().text_size(px(13.)).text_color(c(theme::MUTED)).child("Loading…").into_any_element(),
                    Some(Err(why)) => div().text_size(px(13.)).text_color(c(theme::NEGATIVE)).child(why.clone()).into_any_element(),
                    Some(Ok(list)) => {
                        let rows: Vec<Stateful<Div>> = list.iter().enumerate().map(|(i, t)| {
                            let key = t.key.clone();
                            let enabled = !t.in_library;
                            let ticked = enabled && picked.contains(&t.key);
                            h().id(("gl-starter", i)).gap(px(10.)).items_start().px(px(8.)).py(px(6.)).rounded(px(6.))
                                .when(enabled, |el| {
                                    el.cursor_pointer().hover(|s| s.bg(c(theme::SURFACE_2))).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                        if let Some(Dialog::Starter { picked, .. }) = this.guidelines.dialog.as_mut()
                                            && !picked.remove(&key)
                                        {
                                            picked.insert(key.clone());
                                        }
                                        cx.notify();
                                    }))
                                })
                                .child(div().pt(px(2.)).child(tick(on(ticked), enabled)))
                                .child(
                                    v().flex_1().min_w_0()
                                        .child(div().text_size(px(13.)).font_weight(FontWeight::MEDIUM).child(t.title.clone()))
                                        .child(div().text_size(px(12.)).text_color(c(theme::MUTED)).child(format!(
                                            "{} characters · for {}{}",
                                            count(t.chars),
                                            if t.level == GuidelineLevel::Project { "a project" } else { "employees" },
                                            if t.in_library { " · already in the library" } else { "" }
                                        ))),
                                )
                        }).collect();
                        v().gap(px(4.)).children(rows).into_any_element()
                    }
                };
                let n = picked.len();
                self.guideline_card(520., max_h)
                    .child(
                        v().p(px(22.)).gap(px(10.))
                            .child(div().font_family(serif).text_size(px(21.)).child("Add starter set"))
                            .child(div().text_size(px(13.)).line_height(px(19.)).text_color(c(theme::TEXT_2)).child("Templates to start from. They become ordinary guidelines without links: link them to use them."))
                            .child(v().id("gl-starter-list").max_h(px((max_h - 220.).max(120.))).overflow_y_scrollbar().child(rows)),
                    )
                    .child(self.guideline_foot(cx).child(
                        ui::button("gl-starter-add", if busy { "Adding…".to_string() } else { format!("Add {n}") }, ButtonKind::Primary, None)
                            .when(n == 0 || busy, |el| el.opacity(0.5))
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                let Some(Dialog::Starter { picked, busy, .. }) = this.guidelines.dialog.as_mut() else { return };
                                if picked.is_empty() || *busy {
                                    return;
                                }
                                *busy = true;
                                let keys = picked.iter().cloned().collect();
                                this.guideline_send(GuidelineCall::Write(GuidelineWrite::AddStarter(keys)));
                                cx.notify();
                            })),
                    ))
                    .into_any_element()
            }
            Dialog::Reject { proposal, busy } => {
                let who = self.guidelines.proposal(proposal).map(|p| self.gl_names().employee(p.proposed_by)).unwrap_or_else(|| "the employee".into());
                self.guideline_card(480., max_h)
                    .child(
                        v().p(px(22.)).gap(px(10.))
                            .child(div().font_family(serif).text_size(px(21.)).child("Reject this proposal?"))
                            .child(div().text_size(px(12.5)).text_color(c(theme::TEXT_2)).child(format!("Note for {who}")))
                            .child(Textarea::new(&self.guidelines.note).py(px(7.)).line_height(px(20.)))
                            .child(div().text_size(px(12.)).text_color(c(theme::MUTED)).child(format!("{who} gets your decision and the note."))),
                    )
                    .child(self.guideline_foot(cx).child(
                        ui::button("gl-reject", if busy { "Rejecting…" } else { "Reject" }, ButtonKind::Primary, None)
                            .when(busy, |el| el.opacity(0.5))
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.guideline_decide(proposal, false, cx))),
                    ))
                    .into_any_element()
            }
            Dialog::Confirm(confirm) => {
                let names = self.gl_names();
                let (title, body, label) = match confirm {
                    Confirm::Archive(id) => {
                        let g = self.guidelines.guideline(id);
                        let links = g.map_or(&[][..], |g| g.links.as_slice());
                        let now = if links.is_empty() { "it is not linked".to_string() } else { format!("now {}", applies_to(links, names).1) };
                        (
                            format!("Archive “{}”?", g.map_or("this guideline", |g| g.title.as_str())),
                            format!("No new run gets it; a run that already started keeps it. Its links go ({now}). Its versions stay, and Restore under Archived brings it back."),
                            "Archive",
                        )
                    }
                    Confirm::RestoreVersion(id, version) => {
                        let g = self.guidelines.guideline(id);
                        (
                            format!("Restore version {version} of {}?", g.map_or("this guideline", |g| g.title.as_str())),
                            format!("This becomes version {} and applies from the next run. No version is lost.", g.map_or(0, |g| g.version) + 1),
                            "Restore",
                        )
                    }
                    Confirm::Discard(_) => ("Discard your changes?".to_string(), "What you typed in this guideline is not saved.".to_string(), "Discard"),
                };
                self.guideline_card(460., max_h)
                    .child(v().p(px(22.)).gap(px(10.)).child(div().font_family(serif).text_size(px(21.)).child(title)).child(div().text_size(px(13.)).line_height(px(19.)).text_color(c(theme::TEXT_2)).child(body)))
                    .child(self.guideline_foot(cx).child(ui::button("gl-confirm", label, ButtonKind::Primary, Some(ui::kbd("secondary-enter", cx))).on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.guideline_confirmed(confirm, w, cx)))))
                    .into_any_element()
            }
        }
    }

    /// Ctrl+Enter in a guideline dialog: its main button.
    pub fn guideline_dialog_primary(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.guidelines.dialog.clone() {
            Some(Dialog::Links(_)) => self.guideline_links_save(window, cx),
            Some(Dialog::Confirm(confirm)) => self.guideline_confirmed(confirm, window, cx),
            Some(Dialog::Reject { proposal, .. }) => self.guideline_decide(proposal, false, cx),
            Some(Dialog::Starter { picked, busy: false, .. }) if !picked.is_empty() => {
                if let Some(Dialog::Starter { busy, .. }) = self.guidelines.dialog.as_mut() {
                    *busy = true;
                }
                self.guideline_send(GuidelineCall::Write(GuidelineWrite::AddStarter(picked.into_iter().collect())));
                cx.notify();
            }
            _ => {}
        }
    }

    fn render_links_dialog(&mut self, d: LinksDialog, max_h: f32, cx: &mut Context<Self>) -> AnyElement {
        let names = self.gl_names();
        let contexts = contexts(self.guidelines.active(), &self.projects);
        let sets = &self.guidelines.effective;
        let serif = ui::serif(cx);
        let local = matches!(d.subject, Subject::Draft | Subject::Proposal(_));
        let foot = h().px(px(22.)).py(px(14.)).gap(px(8.)).justify_end().border_t_1().border_color(c(theme::BORDER))
            .children(d.failure.clone().map(|f| div().flex_1().text_size(px(12.)).line_height(px(17.)).text_color(c(theme::NEGATIVE)).child(f)))
            .child(ui::button("gl-links-cancel", "Cancel", ButtonKind::Outline, Some(ui::kbd("escape", cx))).on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.guideline_close_dialog(w, cx))))
            .child(
                ui::button("gl-links-save", if d.busy { "Saving…" } else if local { "Done" } else { "Save" }, ButtonKind::Primary, Some(ui::kbd("secondary-enter", cx)))
                    .when(d.busy, |el| el.opacity(0.5))
                    .on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.guideline_links_save(w, cx))),
            );
        let card = self.guideline_card(640., max_h);
        if let Subject::Target(target) = d.subject {
            // The other way round: every guideline with a box (Manage links…).
            let title = match target {
                GuidelineTarget::Employee(e) => format!("Guidelines for {}", names.employee(e)),
                GuidelineTarget::Project(p) => format!("Guidelines for everyone in {}", names.project(p)),
            };
            let mut list: Vec<GuidelineView> = self.guidelines.active().to_vec();
            list.sort_by_key(|g| g.title.to_lowercase());
            let rows: Vec<Stateful<Div>> = list.iter().enumerate().map(|(i, g)| {
                let gid = g.id;
                let ticked = d.guidelines_after.contains(&gid);
                let was = d.guidelines_before.contains(&gid);
                let after = match target {
                    GuidelineTarget::Employee(e) => employee_block(sets, e, &contexts, Some((Some(gid), g.chars))),
                    GuidelineTarget::Project(p) => project_block(sets, p, &contexts, &self.managers, Some(gid), g.chars),
                };
                let blocked = !was && !ticked && after.is_some_and(|b| b.chars > RUN_LIMIT);
                let reason = after.filter(|_| blocked).map(|b| block_sentence(b, names));
                h().id(("gl-link-g", i)).gap(px(10.)).items_center().px(px(8.)).py(px(5.)).rounded(px(6.)).text_size(px(13.))
                    .aria_label(reason.clone().map_or_else(|| g.title.clone(), |r| format!("{}: {r}", g.title)))
                    .when(!blocked, |el| {
                        el.cursor_pointer().hover(|s| s.bg(c(theme::SURFACE_2))).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            if let Some(Dialog::Links(d)) = this.guidelines.dialog.as_mut()
                                && !d.guidelines_after.remove(&gid)
                            {
                                d.guidelines_after.insert(gid);
                            }
                            cx.notify();
                        }))
                    })
                    .when_some(reason, |el, r| el.tooltip(ui::tip(r, None)))
                    .child(tick(on(ticked), !blocked))
                    .child(div().flex_1().min_w_0().truncate().child(g.title.clone()))
                    .child(div().text_size(px(12.)).text_color(c(theme::TEXT_2)).child(format!("v{} · {}", g.version, count(g.chars))))
                    .children(after.filter(|b| ticked || blocked || b.chars > RUN_LIMIT).map(|b| div().w(px(70.)).text_size(px(12.)).text_color(c(level_text(level(b.chars, RUN_LIMIT)))).child(count(b.chars))))
            }).collect();
            let empty = rows.is_empty();
            return card
                .child(
                    v().p(px(22.)).gap(px(8.))
                        .child(div().font_family(serif).text_size(px(21.)).child(title))
                        .when(empty, |el| el.child(div().text_size(px(13.)).text_color(c(theme::MUTED)).child("The library is empty.")))
                        .child(v().id("gl-links-list").max_h(px((max_h - 180.).max(120.))).overflow_y_scrollbar().children(rows)),
                )
                .child(foot)
                .into_any_element();
        }
        let (g, title) = match d.subject {
            Subject::Guideline(id) => (Some(id), self.guidelines.guideline(id).map(|g| g.title.clone()).unwrap_or_default()),
            Subject::Proposal(id) => (None, self.guidelines.proposal(id).map(|p| p.title.clone()).unwrap_or_default()),
            _ => (None, self.guidelines.title.read(cx).value().to_string()),
        };
        let keep: Vec<EmployeeId> = d.before.iter().filter_map(|t| match t {
            GuidelineTarget::Employee(e) => Some(*e),
            GuidelineTarget::Project(_) => None,
        }).collect();
        let mut rows: Vec<AnyElement> = vec![ui::eyebrow("EMPLOYEES").into_any_element()];
        for (gi, group) in groups(&self.managers, &keep).into_iter().enumerate() {
            let (n, m, state) = tri(&group.members, &d.after);
            let folded = d.folded.contains(&group.boss);
            let boss = group.boss;
            // The group's box ticks only members that still fit; it unticks all of them.
            let allowed: Vec<EmployeeId> = group.members.iter().copied().filter(|e| {
                d.before.contains(&GuidelineTarget::Employee(*e)) || employee_block(sets, *e, &contexts, Some((g, d.chars))).is_none_or(|b| b.chars <= RUN_LIMIT)
            }).collect();
            let members = group.members.clone();
            rows.push(
                h().id(("gl-group", gi)).gap(px(10.)).items_center().px(px(6.)).pt(px(8.)).pb(px(3.)).text_size(px(13.))
                    .aria_label(format!("{}, {n} of {m} selected", group.label))
                    .child(div().id(("gl-group-fold", gi)).w(px(12.)).cursor_pointer().text_color(c(theme::MUTED)).child(if folded { "▸" } else { "▾" }).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        if let Some(Dialog::Links(d)) = this.guidelines.dialog.as_mut()
                            && !d.folded.remove(&boss)
                        {
                            d.folded.insert(boss);
                        }
                        cx.notify();
                    })))
                    .child(div().id(("gl-group-tick", gi)).cursor_pointer().child(tick(state, true)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        if let Some(Dialog::Links(d)) = this.guidelines.dialog.as_mut() {
                            let all = members.iter().all(|e| d.after.contains(&GuidelineTarget::Employee(*e)));
                            for e in &members {
                                let t = GuidelineTarget::Employee(*e);
                                if all {
                                    d.after.remove(&t);
                                } else if allowed.contains(e) {
                                    d.after.insert(t);
                                }
                            }
                        }
                        cx.notify();
                    })))
                    .child(div().flex_1().font_weight(FontWeight::MEDIUM).child(group.label.clone()))
                    .child(div().text_size(px(12.)).text_color(c(theme::MUTED)).child(format!("{n} of {m}")))
                    .into_any_element(),
            );
            if folded {
                continue;
            }
            for (ei, e) in group.members.iter().enumerate() {
                let Some(emp) = self.managers.iter().find(|x| x.id == *e) else { continue };
                let target = GuidelineTarget::Employee(*e);
                let ticked = d.after.contains(&target);
                let was = d.before.contains(&target);
                let now = employee_block(sets, *e, &contexts, None);
                let after = employee_block(sets, *e, &contexts, Some((g, d.chars)));
                let blocked = !was && !ticked && after.is_some_and(|b| b.chars > RUN_LIMIT);
                let reason = after.filter(|_| blocked).map(|b| block_sentence(b, names));
                let status = match emp.status {
                    LifecycleStatus::Paused => " (paused)",
                    LifecycleStatus::Archived => " (archived)",
                    _ => "",
                };
                let size = match (now, after, ticked) {
                    (Some(n), Some(a), true) if n.chars != a.chars => format!("{} → {}", count(n.chars), count(a.chars)),
                    (_, Some(a), true) => count(a.chars),
                    (Some(n), _, false) => count(n.chars),
                    _ => String::new(),
                };
                let size_level = after.filter(|_| ticked || blocked).map_or(Level::Calm, |a| level(a.chars, RUN_LIMIT));
                rows.push(
                    h().id(("gl-link-e", gi * 1000 + ei)).gap(px(10.)).items_center().pl(px(28.)).pr(px(6.)).py(px(4.)).rounded(px(6.)).text_size(px(13.))
                        .aria_label(reason.clone().map_or_else(|| emp.name.clone(), |r| format!("{}: {r}", emp.name)))
                        .when(!blocked, |el| {
                            el.cursor_pointer().hover(|s| s.bg(c(theme::SURFACE_2))).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                if let Some(Dialog::Links(d)) = this.guidelines.dialog.as_mut()
                                    && !d.after.remove(&target)
                                {
                                    d.after.insert(target);
                                }
                                cx.notify();
                            }))
                        })
                        .when_some(reason, |el, r| el.tooltip(ui::tip(r, None)))
                        .child(tick(on(ticked), !blocked))
                        .child(div().w(px(150.)).flex_none().truncate().child(format!("{}{status}", emp.name)))
                        .child(div().flex_1().min_w_0().truncate().text_size(px(12.)).text_color(c(theme::MUTED)).child(emp.role.clone()))
                        .child(div().text_size(px(12.)).text_color(c(level_text(size_level))).child(size))
                        .into_any_element(),
                );
            }
        }
        rows.push(div().pt(px(12.)).child(ui::eyebrow("PROJECTS")).into_any_element());
        let mut projects: Vec<&ProjectSummary> = self.projects.iter().filter(|p| p.status != ProjectStatus::Archived || d.before.contains(&GuidelineTarget::Project(p.id))).collect();
        projects.sort_by_key(|p| p.name.to_lowercase());
        for (pi, p) in projects.into_iter().enumerate() {
            let target = GuidelineTarget::Project(p.id);
            let ticked = d.after.contains(&target);
            let was = d.before.contains(&target);
            let after = project_block(sets, p.id, &contexts, &self.managers, g, d.chars);
            let blocked = !was && !ticked && after.is_some_and(|b| b.chars > RUN_LIMIT);
            let reason = after.filter(|_| blocked).map(|b| block_sentence(b, names));
            let note = match p.status {
                ProjectStatus::Archived => " (archived)",
                ProjectStatus::Paused => " (paused)",
                ProjectStatus::Active => "",
            };
            rows.push(
                h().id(("gl-link-p", pi)).gap(px(10.)).items_center().px(px(6.)).py(px(4.)).rounded(px(6.)).text_size(px(13.))
                    .aria_label(reason.clone().map_or_else(|| p.name.clone(), |r| format!("{}: {r}", p.name)))
                    .when(!blocked, |el| {
                        el.cursor_pointer().hover(|s| s.bg(c(theme::SURFACE_2))).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            if let Some(Dialog::Links(d)) = this.guidelines.dialog.as_mut()
                                && !d.after.remove(&target)
                            {
                                d.after.insert(target);
                            }
                            cx.notify();
                        }))
                    })
                    .when_some(reason, |el, r| el.tooltip(ui::tip(r, None)))
                    .child(tick(on(ticked), !blocked))
                    .child(div().w(px(170.)).flex_none().truncate().child(format!("{}{note}", p.name)))
                    .child(div().flex_1().min_w_0().truncate().text_size(px(12.)).text_color(c(theme::MUTED)).child("everyone working in this project"))
                    .children(after.filter(|_| ticked || blocked).and_then(|b| Some((b.employee?, b.chars))).map(|(e, n)| {
                        div().text_size(px(12.)).text_color(c(level_text(level(n, RUN_LIMIT)))).child(format!("largest: {} · {}", names.employee(e), count(n)))
                    }))
                    .into_any_element(),
            );
        }
        rows.push(h().gap(px(6.)).pt(px(10.)).text_size(px(11.5)).text_color(c(theme::MUTED)).child("ⓘ").child(GROUP_NOTE).into_any_element());
        card.child(
            v().p(px(22.)).gap(px(8.))
                .child(div().font_family(serif).text_size(px(21.)).child(format!("Who gets “{title}”")))
                .child(v().id("gl-links-list").max_h(px((max_h - 180.).max(120.))).overflow_y_scrollbar().gap(px(1.)).children(rows)),
        )
        .child(foot)
        .into_any_element()
    }
}
