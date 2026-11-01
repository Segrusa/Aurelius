//! Where else guidelines show: what an employee gets, a project's block, the trace's
//! line and the proposal card in the Inbox.

use super::logic::*;
use super::state::*;
use super::view::{bar, diff_box, level_text, on, tick};
use crate::app::{AppView, ManagerRef};
use crate::backend::GuidelineCall;
use crate::theme::{self, c};
use crate::ui::{self, ButtonKind, h, v};
use aurelius_domain::guideline::{EffectiveGuideline, GuidelineLevel, GuidelineProposalView, GuidelineTarget, GuidelineView};
use aurelius_domain::{EmployeeId, GuidelineId, ProjectId, RunId};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::collections::HashMap;

impl AppView {
    /// What one employee gets: its own guidelines, then per project what comes on top, with
    /// the block per context against 16 000 (GL-07, §6.1).
    pub(super) fn render_guidelines_of(&mut self, employee: EmployeeId, cx: &mut Context<Self>) -> AnyElement {
        self.guideline_effective_want();
        let names = self.gl_names();
        let name = names.employee(employee);
        let contexts = contexts(self.guidelines.active(), &self.projects);
        let can_edit = self.guidelines_can_edit();
        let back = h().id("gl-of-back").gap(px(6.)).cursor_pointer().text_size(px(12.5)).text_color(c(theme::MUTED)).hover(|s| s.text_color(c(theme::TEXT)))
            .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.open_manager(ManagerRef::Real(employee), w, cx)))
            .child(format!("‹ {name}"));
        let head = h().gap(px(10.)).items_center()
            .child(div().font_family(ui::serif(cx)).text_size(px(26.)).child(format!("Guidelines of {name}")))
            .child(div().flex_1())
            .when(can_edit, |el| {
                el.child(ui::button("gl-of-manage", "Manage links…", ButtonKind::Outline, None).on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.guideline_links(Subject::Target(GuidelineTarget::Employee(employee)), w, cx))))
                    .child(ui::button("gl-of-new", "+ New guideline", ButtonKind::Outline, None).on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.guideline_open(None, w, cx))))
            });
        let open_of: HashMap<GuidelineId, usize> = self.guidelines.pending().iter().filter_map(|p| p.guideline_id).fold(HashMap::new(), |mut m, g| {
            *m.entry(g).or_default() += 1;
            m
        });
        let row = |key: (&'static str, usize), g: &EffectiveGuideline, note: Option<String>, cx: &mut Context<Self>| {
            let id = g.id;
            let open = open_of.get(&id).copied().unwrap_or(0);
            h().id(key).gap(px(12.)).px(px(10.)).py(px(5.)).rounded(px(6.)).cursor_pointer().text_size(px(13.))
                .hover(|s| s.bg(c(theme::SURFACE_2)))
                .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.guideline_open(Some(id), w, cx)))
                .child(div().w(px(220.)).flex_none().truncate().child(g.title.clone()))
                .child(div().w(px(40.)).flex_none().text_color(c(theme::TEXT_2)).child(format!("v{}", g.version)))
                .child(div().w(px(70.)).flex_none().text_color(c(theme::TEXT_2)).child(count(g.chars)))
                .children(note.map(|n| div().text_size(px(12.)).text_color(c(theme::MUTED)).child(n)))
                .when(open > 0, |el| {
                    el.child(h().gap(px(4.)).text_size(px(12.)).text_color(c(theme::ACCENT_TEXT)).child(ui::diamond(7., theme::ACCENT_TEXT, false)).child(format!("{open} proposal{}", if open == 1 { "" } else { "s" })))
                })
        };
        let total = |chars: u32| {
            let lvl = level(chars, RUN_LIMIT);
            h().gap(px(8.)).items_center().text_size(px(12.))
                .child(bar(chars, RUN_LIMIT, 120.))
                .child(div().text_color(c(level_text(lvl))).child(format!("block {} / {}", count(chars), count(RUN_LIMIT))))
                .when(lvl == Level::Close, |el| el.child(div().text_color(c(CLOSE)).child("close to the limit")))
                .when(lvl == Level::Over, |el| el.child(div().text_color(c(theme::NEGATIVE)).child("over the limit: its runs leave some out")))
        };
        let mut body = v().gap(px(10.)).child(back).child(head);
        match self.guidelines.effective.get(&(Some(employee), None)).cloned() {
            None => body = body.child(div().text_size(px(13.)).text_color(c(theme::MUTED)).child("Loading…")),
            Some(Err(why)) => body = body.child(div().text_size(px(13.)).text_color(c(theme::NEGATIVE)).child(why)),
            Some(Ok(own)) => {
                // Its own guideline that a project also has: there it counts once, at project level (P-55).
                let also: HashMap<GuidelineId, String> = own
                    .guidelines
                    .iter()
                    .filter_map(|g| {
                        let links = self.guidelines.guideline(g.id).map(|x| x.links.clone()).unwrap_or_default();
                        let via: Vec<String> = contexts.iter().flatten().filter(|p| links.contains(&GuidelineTarget::Project(**p))).map(|p| format!("also via ▸ {}", names.project(*p))).collect();
                        (!via.is_empty()).then(|| (g.id, via.join(", ")))
                    })
                    .collect();
                let projects: Vec<ProjectId> = contexts.iter().flatten().copied().collect();
                if own.guidelines.is_empty() && projects.is_empty() {
                    body = body.child(div().text_size(px(13.)).text_color(c(theme::TEXT_2)).child(format!("No guidelines yet. {name} works with its instructions only.")));
                } else {
                    body = body.child(ui::eyebrow(format!("OWN (LINKED TO {})", name.to_uppercase())));
                    if own.guidelines.is_empty() {
                        body = body.child(div().px(px(10.)).text_size(px(12.5)).text_color(c(theme::MUTED)).child("—"));
                    }
                    for (i, g) in own.guidelines.iter().enumerate() {
                        body = body.child(row(("gl-of-own", i), g, also.get(&g.id).cloned(), cx));
                    }
                    body = body.child(h().gap(px(10.)).px(px(10.)).child(div().w(px(220.)).text_size(px(12.)).text_color(c(theme::TEXT_2)).child("Without a project")).child(total(own.total_chars)));
                    if !projects.is_empty() {
                        body = body.child(div().pt(px(6.)).child(ui::eyebrow("FROM A PROJECT (ONLY IN RUNS OF THAT PROJECT)")));
                    }
                    for (j, p) in projects.iter().enumerate() {
                        let set = self.guidelines.effective.get(&(Some(employee), Some(*p))).and_then(|s| s.as_ref().ok()).cloned();
                        body = body.child(
                            h().gap(px(10.)).px(px(10.)).pt(px(4.))
                                .child(div().w(px(220.)).text_size(px(12.5)).text_color(c(theme::TEXT_2)).child(format!("▸ {}", names.project(*p))))
                                .children(set.as_ref().map(|s| total(s.total_chars))),
                        );
                        match set {
                            None => body = body.child(div().px(px(10.)).text_size(px(12.)).text_color(c(theme::MUTED)).child("Loading…")),
                            Some(s) => {
                                for (k, g) in s.guidelines.iter().filter(|g| g.level == GuidelineLevel::Project).enumerate() {
                                    body = body.child(row(("gl-of-project", j * 1000 + k), g, None, cx));
                                }
                            }
                        }
                    }
                }
                body = body.child(div().pt(px(6.)).text_size(px(12.)).text_color(c(theme::MUTED)).child(format!("Without a project: {} / {} characters · {}. {PRECEDENCE}", count(own.total_chars), count(RUN_LIMIT), tokens(own.total_chars))));
            }
        }
        self.page(v().pt(px(24.)).pb(px(40.)).px(px(28.)).child(body)).into_any_element()
    }

    /// The Guidelines chip on an employee's page: "Guidelines · 3 · 9 110 / 16 000" (§6.3).
    pub fn guidelines_chip(&mut self, employee: EmployeeId, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        if !self.guidelines_ready() {
            return None;
        }
        self.guidelines_want();
        let key = (Some(employee), None);
        if self.guidelines.effective_asked.insert(key) {
            self.guideline_send(GuidelineCall::Effective(key));
        }
        let set = self.guideline_set(Some(employee), None);
        let label = match set {
            Some(set) => format!("Guidelines · {} · {} / {}", set.guidelines.len(), count(set.total_chars), count(RUN_LIMIT)),
            None => "Guidelines".to_string(),
        };
        let lvl = set.map_or(Level::Calm, |s| level(s.total_chars, RUN_LIMIT));
        Some(
            h().id("employee-guidelines").gap(px(6.)).cursor_pointer().text_color(c(level_text(lvl)))
                .hover(|s| s.text_color(c(theme::TEXT)))
                .tooltip(ui::tip("What its runs get on top of its instructions", None))
                .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.guidelines_go(GuidelinesView::Employee(employee), w, cx)))
                .child(label),
        )
    }

    /// A project's Overview: what every run in it gets on top (§6.2, P-53, GL-06 AC2).
    pub fn render_project_guidelines(&mut self, project: ProjectId, cx: &mut Context<Self>) -> Option<Div> {
        if !self.guidelines_ready() {
            return None;
        }
        self.guidelines_want();
        let target = GuidelineTarget::Project(project);
        let mut linked: Vec<GuidelineView> = self.guidelines.active().iter().filter(|g| g.links.contains(&target)).cloned().collect();
        linked.sort_by_key(|g| g.title.to_lowercase());
        if !linked.is_empty() {
            self.guideline_effective_want();
        }
        let names = self.gl_names();
        let added: u32 = linked.iter().map(|g| g.chars).sum();
        let contexts = contexts(self.guidelines.active(), &self.projects);
        let largest = project_block(&self.guidelines.effective, project, &contexts, &self.managers, None, 0);
        let can_edit = self.guidelines_can_edit();
        let name = names.project(project);
        let summary = (!linked.is_empty()).then(|| {
            let (who, lvl) = match largest.and_then(|b| Some((b.employee?, b.chars))) {
                Some((e, n)) => (format!(" · largest block: {} {}", names.employee(e), count(n)), level(n, RUN_LIMIT)),
                None => (String::new(), Level::Calm),
            };
            div().text_size(px(12.)).text_color(c(level_text(lvl))).child(format!("Added to each run here: {} characters{who}", count(added)))
        });
        Some(
            v().gap(px(6.)).p(px(16.)).rounded(px(9.)).border_1().border_color(c(theme::BORDER))
                .child(
                    h().gap(px(8.))
                        .child(div().flex_1().child(ui::eyebrow(format!("GUIDELINES FOR EVERYONE IN {}", name.to_uppercase()))))
                        .when(can_edit, |el| {
                            el.child(ui::button("project-guidelines-manage", "Manage links…", ButtonKind::Outline, None).h(px(26.)).on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.guideline_links(Subject::Target(target), w, cx))))
                        }),
                )
                .when(linked.is_empty(), |el| el.child(div().text_size(px(12.5)).text_color(c(theme::MUTED)).child("None: runs here get only their employee's guidelines.")))
                .children(linked.iter().enumerate().map(|(i, g)| {
                    let id = g.id;
                    h().id(("project-guideline", i)).gap(px(12.)).px(px(6.)).py(px(3.)).rounded(px(5.)).cursor_pointer().text_size(px(13.))
                        .hover(|s| s.bg(c(theme::SURFACE_2)))
                        .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.guideline_open(Some(id), w, cx)))
                        .child(div().w(px(200.)).flex_none().truncate().child(g.title.clone()))
                        .child(div().w(px(40.)).flex_none().text_color(c(theme::TEXT_2)).child(format!("v{}", g.version)))
                        .child(div().text_color(c(theme::TEXT_2)).child(count(g.chars)))
                }))
                .children(summary),
        )
    }

    /// The trace's line: what guidelines the run had, each opening that version (P-59, GL-11 AC3).
    pub fn render_run_guidelines(&self, run_id: RunId, cx: &mut Context<Self>) -> Option<Div> {
        let list = match self.guidelines.runs.get(&run_id)? {
            Ok(list) => list.clone(),
            Err(why) => return Some(div().text_size(px(12.)).text_color(c(theme::MUTED)).child(format!("Guidelines: {why}"))),
        };
        let Some(line) = run_line(&list) else {
            return Some(div().text_size(px(12.)).text_color(c(theme::MUTED)).child("Guidelines: none"));
        };
        let last = list.len() - 1;
        Some(
            div().child(h().id("trace-guidelines").gap(px(6.)).flex_wrap().text_size(px(12.5)).aria_label(line)
                .child(div().text_color(c(theme::MUTED)).child("Guidelines:"))
                .children(list.iter().enumerate().map(|(i, g)| {
                    let (id, version) = (g.guideline_id, g.version);
                    div().id(("trace-guideline", i)).cursor_pointer().text_color(c(theme::ACCENT_TEXT)).hover(|s| s.text_color(c(theme::TEXT)))
                        .tooltip(ui::tip(format!("{} level · {} characters · opens this version", g.level.as_str(), count(g.chars)), None))
                        .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.guideline_open_version(id, version, w, cx)))
                        .child(format!("{} v{}{}", g.title, g.version, if i < last { "," } else { "" }))
                }))),
        )
    }

    /// A guideline proposal's inbox row: its title and who proposed it.
    pub fn guideline_proposal_row(&self, p: &GuidelineProposalView) -> (String, String) {
        let title = match p.guideline_id.and_then(|g| self.guidelines.guideline(g)) {
            Some(g) => format!("Guideline: {}", g.title),
            None => format!("New guideline: {}", p.title),
        };
        (title, format!("Proposed by {}", self.gl_names().employee(p.proposed_by)))
    }

    /// The Inbox card of a guideline proposal (§7.1): who, what, why, the diff against the
    /// version it rests on, the limits after Approve, and Reject / Edit and approve… / Approve.
    pub fn guideline_proposal_body(&mut self, p: &GuidelineProposalView, i: usize, cx: &mut Context<Self>) -> Div {
        let id = p.id;
        let base = p.guideline_id.zip(p.base_version);
        if let Some((g, v)) = base {
            self.guideline_text_want(g, v);
        }
        self.guidelines_want();
        self.guideline_effective_want();
        let names = self.gl_names();
        let current = p.guideline_id.and_then(|g| self.guidelines.guideline(g)).cloned();
        let headline = proposal_headline(p, current.as_ref(), names);
        let stale = stale(p, current.as_ref());
        let old = match base {
            Some(key) => self.guidelines.texts.get(&key).map(|v| v.text.clone()),
            None => Some(String::new()),
        };
        let links = self.guidelines.links_for(p);
        let n = chars(&p.text);
        let largest = largest_with(&self.guidelines.effective, p.guideline_id, n, &links);
        let problem = self.guidelines.approve_problem(p, names).filter(|_| stale.is_none());
        let can_edit = self.guidelines_can_edit();
        let busy = self.guidelines.deciding == Some(id);
        let mono = ui::mono(cx);
        let (applies, all) = applies_to(&links, names);
        let version_line = match &current {
            Some(g) => format!("Applies to {applies} · version {} → {}", g.version, g.version + 1),
            None => format!("New guideline · applies to {applies}"),
        };
        let proposer = names.employee(p.proposed_by);
        let mut card = v().gap(px(10.))
            .child(h().gap(px(8.)).child(ui::diamond(9., theme::ACCENT_TEXT, false)).child(ui::eyebrow("GUIDELINE PROPOSED · WAITING FOR YOU")))
            .child(div().font_family(ui::serif(cx)).text_size(px(20.)).child(headline))
            .child(div().id(("gl-proposal-applies", i)).text_size(px(12.5)).text_color(c(theme::TEXT_2)).tooltip(ui::tip(all, None)).child(version_line))
            .when_some(stale, |el, (base, now)| {
                let title = current.as_ref().map_or(p.title.clone(), |g| g.title.clone());
                el.child(
                    div().px(px(12.)).py(px(9.)).rounded(px(7.)).border_1().border_color(c(theme::NEGATIVE)).text_size(px(12.5)).line_height(px(18.)).text_color(c(theme::NEGATIVE))
                        .child(format!("“{title}” changed since this was proposed (version {base} → {now}), so it can't be approved. Reject it; {proposer} can propose again on the new text.")),
                )
            })
            .when(!p.reason.trim().is_empty(), |el| el.child(v().gap(px(3.)).child(ui::eyebrow("WHY")).child(div().text_size(px(13.)).line_height(px(19.)).child(p.reason.clone()))))
            .when(!p.evidence.is_empty(), |el| {
                el.child(v().gap(px(3.)).child(ui::eyebrow("BASED ON")).children(p.evidence.iter().map(|e| {
                    h().gap(px(6.)).items_start().text_size(px(12.5)).text_color(c(theme::TEXT_2))
                        .child(div().flex_none().text_color(c(theme::MUTED)).child(if e.kind == "memory" { "Lesson" } else { "Task" }))
                        .child(div().flex_1().min_w_0().line_clamp(2).child(e.label.clone()))
                })))
            });
        if p.guideline_id.is_none() {
            // A new one: its links as boxes, to change before Approve (P-61).
            let mut choices: Vec<GuidelineTarget> = default_links(p);
            for t in &links {
                if !choices.contains(t) {
                    choices.push(*t);
                }
            }
            let choices = sorted_targets(&choices, names);
            let rows: Vec<Stateful<Div>> = choices.into_iter().enumerate().map(|(j, t)| {
                let ticked = links.contains(&t);
                let proposal = p.clone();
                h().id(("gl-proposal-link", i * 100 + j)).gap(px(8.)).items_center().text_size(px(12.5))
                    .when(can_edit, |el| {
                        el.cursor_pointer().on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            let mut now = this.guidelines.proposal_links_of(&proposal);
                            match now.iter().position(|x| *x == t) {
                                Some(at) => {
                                    now.remove(at);
                                }
                                None => now.push(t),
                            }
                            this.guidelines.proposal_links.insert(proposal.id, now);
                            cx.notify();
                        }))
                    })
                    .child(tick(on(ticked), can_edit))
                    .child(names.target(t))
            }).collect();
            card = card.child(
                v().gap(px(4.)).child(ui::eyebrow("APPLIES TO")).children(rows).when(can_edit, |el| {
                    el.child(
                        div().id(("gl-proposal-links", i)).cursor_pointer().text_size(px(12.)).text_color(c(theme::ACCENT_TEXT)).hover(|s| s.text_color(c(theme::TEXT)))
                            .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.guideline_links(Subject::Proposal(id), w, cx)))
                            .child("Others…"),
                    )
                }),
            );
        }
        let before = current.as_ref().map(|g| g.chars);
        card.child(
            v().gap(px(3.))
                .child(
                    h().gap(px(8.))
                        .child(div().flex_1().child(ui::eyebrow(if p.guideline_id.is_some() { "CHANGE · NOW → PROPOSED" } else { "TEXT" })))
                        .child(div().text_size(px(12.)).text_color(c(level_text(level(n, TEXT_LIMIT)))).child(match before {
                            Some(b) => format!("{} → {} / {}", count(b), count(n), count(TEXT_LIMIT)),
                            None => format!("{} / {}", count(n), count(TEXT_LIMIT)),
                        })),
                )
                .child(match old {
                    Some(old) => v().id(("gl-proposal-diff", i)).max_h(px(360.)).overflow_y_scrollbar().child(diff_box(&old, &p.text, mono)).into_any_element(),
                    None => div().text_size(px(12.)).text_color(c(theme::MUTED)).child("Loading the version it rests on…").into_any_element(),
                }),
        )
        .children(largest.map(|b| {
            div().text_size(px(12.5)).text_color(c(level_text(level(b.chars, RUN_LIMIT)))).child(format!("Largest block after this: {} {} / {}", block_where(b, names), count(b.chars), count(RUN_LIMIT)))
        }))
        .children(problem.clone().map(|why| div().text_size(px(12.5)).text_color(c(theme::NEGATIVE)).child(format!("{why} Approve is off; Edit and approve… lets you shorten it."))))
        .when(!can_edit, |el| el.child(div().text_size(px(12.)).text_color(c(theme::MUTED)).child("Only a board member decides.")))
        .when(can_edit, |el| {
            let off = problem.is_some() || busy;
            el.child(
                h().gap(px(8.)).justify_end()
                    .child(ui::button(("gl-proposal-reject", i), "Reject", ButtonKind::Outline, None).on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.guideline_decide_selected(id, false, w, cx))))
                    .when(stale.is_none(), |el| {
                        el.child(ui::button(("gl-proposal-edit", i), "Edit and approve…", ButtonKind::Outline, None).on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.guideline_edit_proposal(id, w, cx))))
                            .child(
                                ui::button(("gl-proposal-approve", i), if busy { "Approving…" } else { "Approve" }, ButtonKind::Primary, None)
                                    .when(off, |el| el.opacity(0.5))
                                    .when(!off, |el| el.on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.guideline_decide(id, true, cx)))),
                            )
                    }),
            )
        })
    }
}
