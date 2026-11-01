//! The org chart: who reports to whom (the company-wide Org chart page), and the colleagues employees propose, which the user approves or declines.

use crate::app::{AppView, ManagerRef};
use crate::keymap::*;
use crate::theme::{self, c};
use crate::ui::{self, ButtonKind, h, v};
use aurelius_domain::{EmployeeId, ProposalId, employee::LifecycleStatus, proposal::{ProposalKind, ProposalStatus}};
use aurelius_protocol::{ProposalView, EmployeeSummary};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

/// The company's employees as an org chart, with their depth: roots first, each followed by
/// the employees that report to it. Archived employees are left out; a boss that is archived
/// or unknown makes its reports roots.
pub fn org_rows(managers: &[EmployeeSummary]) -> Vec<(usize, &EmployeeSummary)> {
    fn walk<'a>(boss: &'a EmployeeSummary, depth: usize, active: &[&'a EmployeeSummary], seen: &mut Vec<EmployeeId>, out: &mut Vec<(usize, &'a EmployeeSummary)>) {
        if seen.contains(&boss.id) {
            return;
        }
        seen.push(boss.id);
        out.push((depth, boss));
        for report in active.iter().copied().filter(|m| m.reports_to == Some(boss.id)) {
            walk(report, depth + 1, active, seen, out);
        }
    }
    let active: Vec<&EmployeeSummary> = managers.iter().filter(|m| m.status != LifecycleStatus::Archived).collect();
    let mut out = Vec::new();
    let mut seen = Vec::new();
    for root in active.iter().copied().filter(|m| m.reports_to.is_none_or(|boss| !active.iter().any(|x| x.id == boss))) {
        walk(root, 0, &active, &mut seen, &mut out);
    }
    out
}

/// The direct reports of row `i` in `org_rows`' order: the rows one level deeper that
/// follow it, up to the next row at its level or above.
pub fn org_children<T>(rows: &[(usize, T)], i: usize) -> Vec<usize> {
    let depth = rows[i].0;
    rows.iter().enumerate().skip(i + 1).take_while(|(_, (d, _))| *d > depth).filter(|(_, (d, _))| *d == depth + 1).map(|(j, _)| j).collect()
}

/// Employees `manager` may report to: anyone else in the company who is not archived, without
/// those that report to it (directly or not), which would make a circle. `None`: one still to be added.
pub fn reports_candidates(managers: &[EmployeeSummary], manager: Option<EmployeeId>) -> Vec<&EmployeeSummary> {
    let mut below: Vec<EmployeeId> = Vec::new();
    let mut open: Vec<EmployeeId> = manager.into_iter().collect();
    while let Some(boss) = open.pop() {
        for report in managers.iter().filter(|m| m.reports_to == Some(boss)) {
            if !below.contains(&report.id) {
                below.push(report.id);
                open.push(report.id);
            }
        }
    }
    managers
        .iter()
        .filter(|m| m.status != LifecycleStatus::Archived && Some(m.id) != manager && !below.contains(&m.id))
        .collect()
}

/// Line diff of two texts (longest common subsequence): `' '` kept, `'-'` removed, `'+'` added.
pub fn line_diff<'a>(old: &'a str, new: &'a str) -> Vec<(char, &'a str)> {
    let (a, b): (Vec<&str>, Vec<&str>) = (old.lines().collect(), new.lines().collect());
    // shortcut: very long texts show as all removed then all added, instead of an n×m table.
    if a.len() * b.len() > 4_000_000 {
        return a.iter().map(|l| ('-', *l)).chain(b.iter().map(|l| ('+', *l))).collect();
    }
    // lcs[i][j]: common lines of a[i..] and b[j..].
    let mut lcs = vec![vec![0u32; b.len() + 1]; a.len() + 1];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            lcs[i][j] = if a[i] == b[j] { lcs[i + 1][j + 1] + 1 } else { lcs[i + 1][j].max(lcs[i][j + 1]) };
        }
    }
    let (mut i, mut j, mut out) = (0, 0, Vec::new());
    while i < a.len() || j < b.len() {
        if i < a.len() && j < b.len() && a[i] == b[j] {
            out.push((' ', a[i]));
            i += 1;
            j += 1;
        } else if i < a.len() && (j == b.len() || lcs[i + 1][j] >= lcs[i][j + 1]) {
            out.push(('-', a[i]));
            i += 1;
        } else {
            out.push(('+', b[j]));
            j += 1;
        }
    }
    out
}

/// "Head of engineering · Development manager", or just the role.
pub fn position(m: &EmployeeSummary) -> String {
    match &m.title {
        Some(title) => format!("{title} · {}", m.role),
        None => m.role.clone(),
    }
}

impl AppView {
    /// Colleagues managers proposed that wait for the user.
    pub fn pending_proposals(&self) -> Vec<&ProposalView> {
        self.proposals.iter().filter(|p| p.status == ProposalStatus::Pending).collect()
    }

    /// Items of the open inbox tab ahead of its results: approvals, then proposals.
    pub fn inbox_head(&self) -> usize {
        if self.inbox_tab.shows_head() { self.pending_approvals().count() + self.pending_proposals().len() + self.pending_guideline_proposals().len() } else { 0 }
    }

    pub fn decide_proposal(&mut self, proposal_id: ProposalId, approve: bool, cx: &mut Context<Self>) {
        let _ = self.requests_handle().send(crate::backend::Request::DecideProposal { proposal_id, approve, edits: None, note: None, instructions: None });
        cx.notify();
    }

    /// Approve an instruction proposal with the user's own version of the text.
    pub fn approve_edited_instructions(&mut self, proposal_id: ProposalId, instructions: String, cx: &mut Context<Self>) {
        let _ = self.requests_handle().send(crate::backend::Request::DecideProposal { proposal_id, approve: true, edits: None, note: None, instructions: Some(instructions) });
        cx.notify();
    }

    /// Put an earlier version of a manager's instructions back.
    pub fn restore_instructions(&mut self, employee_id: EmployeeId, version: u32, cx: &mut Context<Self>) {
        let _ = self.requests_handle().send(crate::backend::Request::RestoreInstructions { employee_id, version });
        cx.notify();
    }

    /// What an instruction proposal changes, line by line, with the conflict and the evidence.
    fn instructions_proposal_body(&self, p: &ProposalView, i: usize, cx: &mut Context<Self>) -> Div {
        let id = p.id;
        let target = p.target_employee_name.clone().unwrap_or_else(|| p.name.clone());
        let headline = if target == p.proposed_by_name { format!("{} proposes new instructions for itself", p.proposed_by_name) } else { format!("{} proposes new instructions for {target}", p.proposed_by_name) };
        let stale = matches!((p.base_version, p.current_version), (Some(base), Some(now)) if base != now);
        let current = p.current_instructions.clone().unwrap_or_default();
        let mono = ui::mono(cx);
        v().gap(px(10.))
            .child(h().gap(px(8.)).child(ui::diamond(9., theme::ACCENT_TEXT, false)).child(ui::eyebrow("INSTRUCTIONS PROPOSED · WAITING FOR YOU")))
            .child(div().font_family(ui::serif(cx)).text_size(px(20.)).child(headline))
            .when(stale, |el| {
                el.child(
                    div().px(px(12.)).py(px(9.)).rounded(px(7.)).border_1().border_color(c(theme::NEGATIVE)).text_size(px(12.5)).line_height(px(18.)).text_color(c(theme::NEGATIVE))
                        .child(format!(
                            "{target}'s instructions changed since this was proposed (version {} → {}), so it can't be approved. Decline it; the proposer can propose again on the new text.",
                            p.base_version.unwrap_or_default(), p.current_version.unwrap_or_default()
                        )),
                )
            })
            .when(!p.reason.trim().is_empty(), |el| {
                el.child(v().gap(px(3.)).child(ui::eyebrow("WHY")).child(div().text_size(px(13.)).line_height(px(19.)).child(p.reason.clone())))
            })
            .when(!p.evidence.is_empty(), |el| {
                el.child(v().gap(px(3.)).child(ui::eyebrow("BASED ON")).children(p.evidence.iter().map(|e| {
                    h().gap(px(6.)).items_start().text_size(px(12.5)).text_color(c(theme::TEXT_2))
                        .child(div().flex_none().text_color(c(theme::MUTED)).child(if e.kind == "memory" { "Lesson" } else { "Task" }))
                        .child(div().flex_1().min_w_0().line_clamp(2).child(e.label.clone()))
                })))
            })
            .child(
                v().gap(px(3.)).child(ui::eyebrow("CHANGE · NOW → PROPOSED"))
                    .child(v().p(px(10.)).rounded(px(6.)).bg(c(theme::SURFACE)).border_1().border_color(c(theme::BORDER)).font_family(mono).text_size(px(12.)).line_height(px(17.))
                        .children(line_diff(&current, &p.instructions).into_iter().map(|(mark, line)| {
                            let color = match mark { '+' => theme::ACCENT_TEXT, '-' => theme::NEGATIVE, _ => theme::TEXT_2 };
                            div().text_color(c(color)).when(mark != ' ', |el| el.bg(c(theme::SURFACE_2))).child(format!("{mark} {line}"))
                        }))),
            )
            .child(
                h().gap(px(8.)).justify_end()
                    .child(ui::button(("proposal-decline", i), "Decline", ButtonKind::Outline, None).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.decide_proposal(id, false, cx))))
                    .when(!stale, |el| {
                        el.child(ui::button(("proposal-edit", i), "Edit and approve…", ButtonKind::Outline, None).on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.edit_proposal(id, w, cx))))
                            .child(ui::button(("proposal-approve", i), "Approve", ButtonKind::Primary, None).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.decide_proposal(id, true, cx))))
                    }),
            )
    }

    /// Edit manager: the versions of its instructions, newest first, each but the current one with Restore.
    pub fn instruction_versions_section(&mut self, manager: EmployeeId, cx: &mut Context<Self>) -> Div {
        self.ensure_instruction_versions(manager);
        let section = v().gap(px(6.)).child(div().text_size(px(12.)).text_color(c(theme::TEXT_2)).child("Earlier instructions"));
        let list = match self.instruction_versions.as_ref().filter(|(m, _)| *m == manager) {
            None => return section.child(div().text_size(px(11.5)).text_color(c(theme::MUTED)).child("Loading…")),
            Some((_, Err(e))) => return section.child(div().text_size(px(11.5)).text_color(c(theme::NEGATIVE)).child(e.clone())),
            Some((_, Ok(list))) => list.clone(),
        };
        let source = |ver: &aurelius_protocol::EmployeeInstructionVersionView| match (ver.source.as_str(), ver.restored_from) {
            ("restored", Some(from)) => format!("restored from version {from}"),
            ("proposal", _) => "approved proposal".to_string(),
            ("initial", _) => "as it was".to_string(),
            (other, _) => other.to_string(),
        };
        section.children(list.versions.iter().enumerate().map(|(i, ver)| {
            let (version, current) = (ver.version, ver.version == list.current_version);
            let text = if ver.instructions.trim().is_empty() { "(no instructions)".to_string() } else { ver.instructions.clone() };
            v().id(("instr-version", i)).gap(px(3.)).px(px(10.)).py(px(7.)).rounded(px(6.)).border_1().border_color(c(theme::BORDER))
                .child(
                    h().gap(px(8.)).text_size(px(11.5)).text_color(c(theme::MUTED))
                        .child(div().text_color(c(theme::TEXT_2)).child(format!("Version {version}")))
                        .child(source(ver))
                        .child(div().flex_1().child(ui::fmt_when(ver.created_at_ms)))
                        .when(current, |el| el.child(div().text_color(c(theme::ACCENT_TEXT)).child("current")))
                        .when(!current, |el| {
                            el.child(ui::button(("instr-restore", i), "Restore", ButtonKind::Outline, None).h(px(24.)).on_click(cx.listener(move |this, _: &ClickEvent, w, cx| {
                                this.restore_instructions(manager, version, cx);
                                // The dialog holds the old text and version: close it instead of saving over the restore.
                                w.dispatch_action(Box::new(Back), cx);
                            })))
                        }),
                )
                .child(div().text_size(px(12.)).line_height(px(17.)).text_color(c(theme::TEXT_2)).line_clamp(3).child(text))
        }))
    }

    /// What a proposal says: who proposes whom, for which place, and why.
    pub fn proposal_body(&self, p: &ProposalView, i: usize, cx: &mut Context<Self>) -> Div {
        if p.kind == ProposalKind::Instructions {
            return self.instructions_proposal_body(p, i, cx);
        }
        let id = p.id;
        let place = match (&p.title, &p.reports_to_name) {
            (Some(title), Some(boss)) => format!("{title} · {} · reports to {boss}", p.role),
            (Some(title), None) => format!("{title} · {}", p.role),
            (None, Some(boss)) => format!("{} · reports to {boss}", p.role),
            (None, None) => p.role.clone(),
        };
        v().gap(px(10.))
            .child(h().gap(px(8.)).child(ui::diamond(9., theme::ACCENT_TEXT, false)).child(ui::eyebrow("COLLEAGUE PROPOSED · WAITING FOR YOU")))
            .child(div().font_family(ui::serif(cx)).text_size(px(20.)).child(format!("{} proposes {}", p.proposed_by_name, p.name)))
            .child(div().text_size(px(12.5)).text_color(c(theme::TEXT_2)).child(place))
            .when(!p.reason.trim().is_empty(), |el| {
                el.child(v().gap(px(3.)).child(ui::eyebrow("WHY")).child(div().text_size(px(13.)).line_height(px(19.)).child(p.reason.clone())))
            })
            .when(!p.instructions.trim().is_empty(), |el| {
                el.child(
                    v().gap(px(3.)).child(ui::eyebrow("INSTRUCTIONS"))
                        .child(div().text_size(px(12.5)).line_height(px(18.)).text_color(c(theme::TEXT_2)).line_clamp(4).child(p.instructions.clone())),
                )
            })
            .child(
                h().gap(px(8.)).justify_end()
                    .child(ui::button(("proposal-decline", i), "Decline", ButtonKind::Outline, None).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.decide_proposal(id, false, cx))))
                    .child(ui::button(("proposal-approve", i), "Approve", ButtonKind::Primary, None).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.decide_proposal(id, true, cx)))),
            )
    }

    /// One employee row of a chart or list; a click opens the employee.
    pub(crate) fn employee_row(&self, key: (&'static str, usize), depth: usize, m: &EmployeeSummary, note: Option<String>, cx: &mut Context<Self>) -> Stateful<Div> {
        let id = m.id;
        let paused = m.status == LifecycleStatus::Paused;
        h().id(key)
            .gap(px(10.))
            .pl(px(8. + 24. * depth as f32))
            .pr(px(8.))
            .py(px(6.))
            .rounded(px(7.))
            .cursor_pointer()
            .hover(|s| s.bg(c(theme::SURFACE_2)))
            .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.open_manager(ManagerRef::Real(id), w, cx)))
            .when(depth > 0, |el| el.child(div().w(px(10.)).flex_none().text_color(c(theme::MUTED)).child("└")))
            .child(ui::monogram(&m.name, 26., cx))
            .child(
                v().flex_1().min_w_0()
                    .child(div().truncate().text_size(px(13.)).font_weight(FontWeight::MEDIUM).child(m.name.clone()))
                    .child(div().truncate().text_size(px(11.5)).text_color(c(theme::MUTED)).child(position(m))),
            )
            .when_some(note, |el, n| el.child(div().flex_none().text_size(px(11.5)).text_color(c(theme::TEXT_2)).child(n)))
            .when(paused, |el| el.child(div().text_size(px(11.)).text_color(c(theme::MUTED)).child("Paused")))
            .when(!paused && m.working, |el| el.child(ui::pulse_dot((key.0, key.1 + 10_000), 6., theme::ACCENT)))
    }

    /// The Org chart page: proposals waiting for the user, then the whole company as a tree,
    /// the management team (roots) first.
    pub fn render_company_org(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let serif = ui::serif(cx);
        let proposals: Vec<ProposalView> = self.pending_proposals().into_iter().cloned().collect();
        let rows: Vec<(usize, EmployeeSummary)> = org_rows(&self.managers).into_iter().map(|(d, m)| (d, m.clone())).collect();
        let body = v().pt(px(40.)).pb(px(40.)).gap(px(14.))
            .child(
                h().justify_between()
                    .child(h().gap(px(10.)).items_center().child(div().font_family(serif).text_size(px(30.)).child("Org chart")).child(ui::info("org-about", "Who reports to whom across the company. Change it with Edit (F2) on an employee: Reports to.")))
                    .child(ui::button("org-add", "Add employee", ButtonKind::Outline, Some(ui::kbd("secondary-shift-m", cx))).h(px(30.)).on_click(cx.listener(|_, _: &ClickEvent, w, cx| w.dispatch_action(Box::new(AddEmployee), cx)))),
            )
            .children(proposals.iter().enumerate().map(|(i, p)| {
                self.proposal_body(p, i, cx).px(px(18.)).py(px(16.)).rounded(px(10.)).border_1().border_color(c(theme::ACCENT)).bg(c(theme::SURFACE))
            }))
            .when(rows.is_empty(), |el| el.child(div().p(px(8.)).text_color(c(theme::MUTED)).child("No employees yet.")))
            .when(!rows.is_empty(), |el| {
                let roots: Vec<usize> = rows.iter().enumerate().filter(|(_, (d, _))| *d == 0).map(|(i, _)| i).collect();
                // Wider than the page: the tree scrolls sideways, the page keeps scrolling down.
                el.child(
                    // mx_auto centres a narrow tree; a wide one starts at the left edge instead of being cut off.
                    h().id("org-tree").w_full().overflow_x_scroll().py(px(8.))
                        .child(h().flex_none().mx_auto().items_start().gap(px(28.)).children(roots.into_iter().map(|i| self.org_node(&rows, i, cx)))),
                )
            });
        self.page(body).into_any_element()
    }

    /// One employee of the org chart with its reports below it, joined by lines.
    fn org_node(&self, rows: &[(usize, EmployeeSummary)], i: usize, cx: &mut Context<Self>) -> AnyElement {
        let m = &rows[i].1;
        let id = m.id;
        let paused = m.status == LifecycleStatus::Paused;
        let line = || c(theme::BORDER_STRONG);
        let card = v().id(("org-node", i))
            .w(px(184.))
            .flex_none()
            .px(px(12.))
            .py(px(10.))
            .gap(px(6.))
            .rounded(px(8.))
            .border_1()
            .border_color(c(if m.working && !paused { theme::ACCENT } else { theme::BORDER }))
            .bg(c(theme::SURFACE))
            .cursor_pointer()
            .hover(|s| s.bg(c(theme::SURFACE_2)))
            .when(paused, |el| el.opacity(0.6))
            .tooltip(ui::tip(format!("{} · {}", m.name, position(m)), None))
            .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.open_manager(ManagerRef::Real(id), w, cx)))
            .child(
                h().gap(px(8.)).min_w_0()
                    .child(ui::monogram(&m.name, 24., cx).flex_none())
                    .child(div().flex_1().min_w_0().truncate().text_size(px(13.)).font_weight(FontWeight::MEDIUM).child(m.name.clone()))
                    .when(!paused && m.working, |el| el.child(ui::pulse_dot(("org-node-working", i), 6., theme::ACCENT))),
            )
            .child(div().truncate().text_size(px(11.5)).text_color(c(theme::TEXT_2)).child(position(m)))
            .when(paused, |el| el.child(div().truncate().text_size(px(11.)).text_color(c(theme::MUTED)).child("Paused")));
        let children = org_children(rows, i);
        let n = children.len();
        v().items_center().flex_none()
            .child(card)
            .when(n > 0, |el| {
                el.child(div().w(px(1.)).h(px(16.)).bg(line()))
                    .child(h().items_start().children(children.into_iter().enumerate().map(|(k, j)| {
                        // Each report hangs from a bar: left half for all but the first, right half for all but the last.
                        v().items_center()
                            .child(
                                h().w_full().h(px(16.))
                                    .child(div().flex_1().h_full().when(k > 0, |el| el.border_t_1().border_color(line())))
                                    .child(div().w(px(1.)).h_full().bg(line()))
                                    .child(div().flex_1().h_full().when(k + 1 < n, |el| el.border_t_1().border_color(line()))),
                            )
                            .child(div().px(px(10.)).child(self.org_node(rows, j, cx)))
                    })))
            })
            .into_any_element()
    }

    /// Settings → Company: management's mode, the management team and the default models.
    pub fn render_company_settings(&mut self, page: Div, cx: &mut Context<Self>) -> Div {
        let header = h().px(px(22.)).py(px(14.)).border_b_1().border_color(c(theme::BORDER)).child(self.settings_title(cx));
        let team: Vec<String> = crate::screens::dashboard::roots(&self.managers).iter().filter_map(|id| self.managers.iter().find(|m| m.id == *id)).map(|m| m.name.clone()).collect();
        let line = |text: String| div().text_size(px(12.5)).line_height(px(18.)).text_color(c(theme::TEXT_2)).child(text);
        let body = v().id("company-settings-body").flex_1().min_h_0().overflow_y_scrollbar().p(px(22.)).gap(px(16.))
            .when(self.management_mode_supported() && self.may_approve(), |el| {
                el.when_some(self.management_mode, |el, on| {
                    el.child(
                        v().gap(px(6.))
                            .child(h().gap(px(12.)).child(div().flex_1().text_size(px(13.5)).child(ui::label_with_info("Management chooses the mode", "On: when you pick no mode, the CEO and their direct reports start a task in Plan and choose Auto or Plan themselves, with a reason in the task thread. A mode you pick always wins."))).child(ui::toggle("company-management-mode", on, "Management chooses the mode", || Box::new(ToggleManagementMode)))),
                    )
                })
            })
            .child(
                v().gap(px(6.))
                    .child(ui::eyebrow_with_info("management-team-about", format!("MANAGEMENT TEAM · {}", team.len()), "Employees without a boss. New work from the Dashboard goes to them first."))
                    .child(line(if team.is_empty() { "Nobody yet.".into() } else { team.join(", ") })),
            )
            .child(self.render_members_section(cx))
            .child(self.render_background_login(cx))
            .child(self.employee_default_model_section(cx))
            .child(
                v().gap(px(6.))
                    .child(ui::eyebrow_with_info("subagent-default-about", "SUBAGENT DEFAULT", "Model of Claude's subagents for every employee. A task one employee gives another runs on the assignee's own default model. Each employee can override the subagent model under Advanced in Edit employee."))
                    .child(self.delegation_rows(None, cx)),
            );
        page.child(header).child(body)
    }

    /// Settings → Company: the model every employee runs on unless it has its own (or a chat
    /// or task overrides it). Pick one of the connections' models.
    fn employee_default_model_section(&self, cx: &mut Context<Self>) -> Div {
        let current = self.run_choice_target().map(|(_, settings, conn)| {
            let model = match settings.model {
                aurelius_domain::employee::Setting::Value(m) => Some(m),
                _ => conn.default_model.clone(),
            };
            let effort = match settings.effort {
                aurelius_domain::employee::Setting::Value(e) => Some(e),
                _ => None,
            };
            (conn.id, model.clone(), format!("{} · {}", Self::run_text(model.as_deref(), effort.as_deref()), conn.display_name))
        });
        let options = self.task_model_options();
        v().gap(px(6.))
            .child(ui::eyebrow_with_info("employee-default-model-about", "EMPLOYEE DEFAULT MODEL", "The model every employee runs on, unless the employee has its own default under Edit employee. A chat or task can override it for itself (Ctrl+M) without changing this."))
            .child(div().text_size(px(12.5)).text_color(c(theme::TEXT_2)).child(format!("Now: {}", current.as_ref().map_or("no connection yet (add one under App settings → Connections)".to_string(), |(_, _, l)| l.clone()))))
            .child(h().flex_wrap().gap(px(6.)).children(options.into_iter().enumerate().map(|(i, (conn, model, label))| {
                let on = current.as_ref().is_some_and(|(id, m, _)| *id == conn && m.as_deref() == Some(model.as_str()));
                h().id(("employee-default-model", i)).h(px(28.)).px(px(11.)).items_center().rounded(px(6.)).cursor_pointer().text_size(px(12.5)).border_1()
                    .border_color(c(if on { theme::ACCENT } else { theme::BORDER }))
                    .when(on, |el| el.bg(c(theme::SURFACE_2)))
                    .text_color(c(if on { theme::TEXT } else { theme::TEXT_2 }))
                    .hover(|s| s.bg(c(theme::SURFACE_2)))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.set_employee_default_model(conn, model.clone(), cx)))
                    .child(label)
            })))
    }

}

#[cfg(test)]
pub(crate) mod tests {
    use super::{line_diff, org_rows, reports_candidates};

    #[test]
    fn the_instruction_diff_keeps_common_lines_and_marks_the_rest() {
        let diff = line_diff("Be brief.\nUse Dutch.\nTest first.", "Be brief.\nUse English.\nTest first.\nAsk before pushing.");
        assert_eq!(diff, vec![(' ', "Be brief."), ('-', "Use Dutch."), ('+', "Use English."), (' ', "Test first."), ('+', "Ask before pushing.")]);
        assert_eq!(line_diff("", "New."), vec![('+', "New.")]);
        assert_eq!(line_diff("Old.", ""), vec![('-', "Old.")]);
    }
    use aurelius_domain::{ConversationId, EmployeeId, autonomy::AgentMode, employee::LifecycleStatus};
    use aurelius_protocol::EmployeeSummary;

    pub(crate) fn manager(name: &str, reports_to: Option<EmployeeId>) -> EmployeeSummary {
        EmployeeSummary {
            id: EmployeeId::new(),
            projects_active: vec![],
            name: name.into(),
            role: "Role".into(),
            title: None,
            reports_to,
            instructions: None,
            status: LifecycleStatus::Active,
            version: 1,
            conversation_id: ConversationId::new(),
            working: false,
            agent_mode: AgentMode::default(),
            browser_allowed: false,
            screen_allowed: false,
            folder: None,
            heartbeat: None,
            sequential_tasks: false,
            conversations: vec![],
            agents: vec![],
        }
    }

    fn names(rows: &[(usize, &EmployeeSummary)]) -> Vec<(usize, String)> {
        rows.iter().map(|(d, m)| (*d, m.name.clone())).collect()
    }

    #[test]
    fn the_chart_lists_roots_first_each_followed_by_its_reports() {
        let jean = manager("Jean", None);
        let simon = manager("Simon", Some(jean.id));
        let dean = manager("Dean", Some(simon.id));
        let max = manager("Max", Some(simon.id));
        let tom = manager("Tom", None);
        let all = vec![dean, tom, simon, max, jean];
        let rows = org_rows(&all);
        let expected: Vec<(usize, String)> = [(0, "Tom"), (0, "Jean"), (1, "Simon"), (2, "Dean"), (2, "Max")].into_iter().map(|(d, n)| (d, n.to_string())).collect();
        assert_eq!(names(&rows), expected);
    }

    #[test]
    fn the_tree_hangs_each_employee_under_its_own_boss_only() {
        use super::org_children;
        // Jean > (Simon > (Dean, Max), Ana), Tom: preorder rows as org_rows gives them.
        let rows = [(0, "Jean"), (1, "Simon"), (2, "Dean"), (2, "Max"), (1, "Ana"), (0, "Tom")];
        let kids = |i| org_children(&rows, i).into_iter().map(|j| rows[j].1).collect::<Vec<_>>();
        assert_eq!(kids(0), ["Simon", "Ana"]);
        assert_eq!(kids(1), ["Dean", "Max"]);
        assert!(kids(2).is_empty());
        assert!(kids(4).is_empty());
        assert!(kids(5).is_empty(), "the next root is not a report");
    }

    #[test]
    fn an_archived_boss_makes_its_reports_roots() {
        let mut boss = manager("Boss", None);
        let report = manager("Report", Some(boss.id));
        let elsewhere = manager("Elsewhere", None);
        boss.status = LifecycleStatus::Archived;
        let all = vec![boss, report, elsewhere];
        assert_eq!(names(&org_rows(&all)), vec![(0, "Report".to_string()), (0, "Elsewhere".to_string())]);
    }

    #[test]
    fn nobody_may_report_to_themselves_or_to_someone_below_them() {
        let jean = manager("Jean", None);
        let simon = manager("Simon", Some(jean.id));
        let dean = manager("Dean", Some(simon.id));
        let all = vec![jean.clone(), simon.clone(), dean.clone()];
        let for_jean: Vec<_> = reports_candidates(&all, Some(jean.id)).iter().map(|m| m.name.clone()).collect();
        assert!(for_jean.is_empty(), "everyone is below the root");
        let for_simon: Vec<_> = reports_candidates(&all, Some(simon.id)).iter().map(|m| m.name.clone()).collect();
        assert_eq!(for_simon, ["Jean"]);
        let for_new: Vec<_> = reports_candidates(&all, None).iter().map(|m| m.name.clone()).collect();
        assert_eq!(for_new, ["Jean", "Simon", "Dean"]);
    }
}
