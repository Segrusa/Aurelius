//! The Employees page: everyone in the company, pinned ones on top, the management team next, then grouped by
//! boss, with a search (plan v0.9, phase 2). An employee's own page is `manager.rs`.

use crate::app::{pinned_first, AppView};
use crate::keymap::*;
use crate::theme::{self, c};
use crate::ui::{self, ButtonKind, h, v};
use aurelius_domain::{EmployeeId, employee::LifecycleStatus};
use aurelius_protocol::EmployeeSummary;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

/// Whether an employee matches the search (name, role or title; case-insensitive).
pub fn matches_search(m: &EmployeeSummary, query: &str) -> bool {
    let q = query.trim().to_lowercase();
    q.is_empty() || [m.name.as_str(), m.role.as_str(), m.title.as_deref().unwrap_or("")].iter().any(|f| f.to_lowercase().contains(&q))
}

/// The groups of the Employees page: pinned first, in pin order; then the management team, then one
/// group per boss in the order they come, and last everyone shown who fell in none of those (a paused
/// employee without a boss is no root, as roots are active). Empty groups are left out.
pub fn employee_groups(managers: &[EmployeeSummary], shown: Vec<EmployeeSummary>, pins: &[EmployeeId]) -> Vec<(String, Vec<EmployeeSummary>)> {
    let team = crate::screens::dashboard::roots(managers);
    let pinned: Vec<EmployeeSummary> = pinned_first(shown.clone(), pins, |m| m.id).into_iter().filter(|m| pins.contains(&m.id)).collect();
    let mut rest: Vec<EmployeeSummary> = shown.into_iter().filter(|m| !pins.contains(&m.id)).collect();
    let (in_team, others): (Vec<_>, Vec<_>) = rest.drain(..).partition(|m| team.contains(&m.id));
    rest = others;
    let mut groups: Vec<(String, Vec<EmployeeSummary>)> = vec![("PINNED".into(), pinned), ("MANAGEMENT TEAM".into(), in_team)];
    let mut bosses: Vec<EmployeeId> = Vec::new();
    for boss in rest.iter().filter_map(|m| m.reports_to) {
        if !bosses.contains(&boss) {
            bosses.push(boss);
        }
    }
    for boss in bosses {
        let name = managers.iter().find(|m| m.id == boss).map_or_else(|| "(archived)".to_string(), |m| m.name.to_uppercase());
        let (reports, others): (Vec<_>, Vec<_>) = rest.drain(..).partition(|m| m.reports_to == Some(boss));
        rest = others;
        groups.push((format!("REPORTS TO {name}"), reports));
    }
    groups.push(("OTHERS".into(), rest));
    groups.retain(|(_, list)| !list.is_empty());
    groups
}

impl AppView {
    pub fn render_employees(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let serif = ui::serif(cx);
        let query = self.employee_search.read(cx).value().to_string();
        let shown: Vec<EmployeeSummary> = self.managers.iter()
            .filter(|m| m.status != LifecycleStatus::Archived && matches_search(m, &query))
            .cloned()
            .collect();
        let groups = employee_groups(&self.managers, shown, &self.employee_pins);
        let mut n = 0;
        let sections: Vec<AnyElement> = groups.iter().map(|(label, list)| {
            v().gap(px(2.))
                .child(div().px(px(8.)).pb(px(4.)).child(ui::eyebrow(format!("{label} · {}", list.len()))))
                .children(list.iter().map(|m| {
                    n += 1;
                    let id = m.id;
                    let pinned = self.employee_pins.contains(&id);
                    self.employee_row(("employee", n), 0, m, None, cx).child(
                        h().id(("employee-edit", n)).flex_none().h(px(24.)).px(px(8.)).items_center().rounded(px(5.)).cursor_pointer().text_size(px(12.)).text_color(c(theme::MUTED))
                            .hover(|s| s.bg(c(theme::BORDER)))
                            .tooltip(ui::tip("Edit this employee's settings", None))
                            .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| {
                                cx.stop_propagation();
                                this.open_edit_manager(crate::app::ManagerRef::Real(id), w, cx);
                            }))
                            .child("Edit"),
                    ).child(
                        h().id(("employee-pin", n)).flex_none().size(px(24.)).justify_center().rounded(px(5.)).cursor_pointer()
                            .hover(|s| s.bg(c(theme::BORDER)))
                            .tooltip(ui::tip(if pinned { "Unpin from the sidebar" } else { "Pin to the sidebar" }, None))
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                cx.stop_propagation();
                                this.toggle_employee_pin(id, cx);
                            }))
                            .child(ui::pin(pinned, 13., if pinned { theme::ACCENT_TEXT } else { theme::MUTED })),
                    )
                }))
                .into_any_element()
        }).collect();
        let body = v().pt(px(40.)).pb(px(40.)).gap(px(14.))
            .child(
                h().justify_between()
                    .child(div().font_family(serif).text_size(px(30.)).child("Employees"))
                    .child(ui::button("employees-add", "Add employee", ButtonKind::Outline, Some(ui::kbd("secondary-shift-m", cx))).h(px(30.)).on_click(cx.listener(|_, _: &ClickEvent, w, cx| w.dispatch_action(Box::new(AddEmployee), cx)))),
            )
            .child(div().max_w(px(420.)).child(ui::input(&self.employee_search)))
            .when(sections.is_empty(), |el| el.child(div().text_color(c(theme::MUTED)).child(if query.trim().is_empty() { "No employees yet." } else { "Nobody matches." })))
            .children(sections);
        self.page(body).into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{employee_groups, matches_search};
    use crate::screens::org::tests::manager;
    use aurelius_domain::employee::LifecycleStatus;

    fn labels(groups: &[(String, Vec<aurelius_protocol::EmployeeSummary>)]) -> Vec<(String, Vec<String>)> {
        groups.iter().map(|(l, list)| (l.clone(), list.iter().map(|m| m.name.clone()).collect())).collect()
    }

    #[test]
    fn a_paused_root_is_listed_under_others() {
        let mut ceo = manager("Atlas", None);
        ceo.status = LifecycleStatus::Paused;
        let cto = manager("Titan", Some(ceo.id));
        let dev = manager("Flux", Some(cto.id));
        let all = vec![ceo, cto, dev];
        let groups = employee_groups(&all, all.clone(), &[]);
        assert_eq!(labels(&groups), vec![
            ("REPORTS TO ATLAS".to_string(), vec!["Titan".to_string()]),
            ("REPORTS TO TITAN".to_string(), vec!["Flux".to_string()]),
            ("OTHERS".to_string(), vec!["Atlas".to_string()]),
        ]);
        // Found by the search too: only the paused root is shown, and it still gets a group.
        let shown: Vec<_> = all.iter().filter(|m| super::matches_search(m, "atl")).cloned().collect();
        assert_eq!(labels(&employee_groups(&all, shown, &[])), vec![("OTHERS".to_string(), vec!["Atlas".to_string()])]);
    }

    #[test]
    fn everyone_shown_is_in_exactly_one_group() {
        let ceo = manager("Atlas", None);
        let mut paused = manager("Nova", None);
        paused.status = LifecycleStatus::Paused;
        let cto = manager("Titan", Some(ceo.id));
        let dev = manager("Flux", Some(cto.id));
        let pins = vec![dev.id];
        let all = vec![ceo, paused, cto, dev];
        let groups = employee_groups(&all, all.clone(), &pins);
        assert_eq!(labels(&groups), vec![
            ("PINNED".to_string(), vec!["Flux".to_string()]),
            ("MANAGEMENT TEAM".to_string(), vec!["Atlas".to_string()]),
            ("REPORTS TO ATLAS".to_string(), vec!["Titan".to_string()]),
            ("OTHERS".to_string(), vec!["Nova".to_string()]),
        ]);
    }

    #[test]
    fn search_looks_at_name_role_and_title() {
        let mut m = manager("Simon", None);
        m.title = Some("Lead server".into());
        assert!(matches_search(&m, ""));
        assert!(matches_search(&m, "simon"));
        assert!(matches_search(&m, "SERVER"));
        assert!(matches_search(&m, "role"));
        assert!(!matches_search(&m, "mobile"));
    }
}
