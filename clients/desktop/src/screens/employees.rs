//! The Employees page: everyone in the company, pinned ones on top, the management team next, then grouped by
//! department, with a search (plan v0.9, phase 2). An employee's own page is `manager.rs`.

use crate::app::{pinned_first, AppView};
use crate::keymap::*;
use crate::theme::{self, c};
use crate::ui::{self, ButtonKind, h, v};
use aurelius_domain::{DepartmentId, employee::LifecycleStatus};
use aurelius_protocol::EmployeeSummary;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

/// Whether an employee matches the search (name, role, title or department; case-insensitive).
pub fn matches_search(m: &EmployeeSummary, department: &str, query: &str) -> bool {
    let q = query.trim().to_lowercase();
    q.is_empty() || [m.name.as_str(), m.role.as_str(), m.title.as_deref().unwrap_or(""), department].iter().any(|f| f.to_lowercase().contains(&q))
}

impl AppView {
    pub fn render_employees(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let serif = ui::serif(cx);
        let query = self.employee_search.read(cx).value().to_string();
        let team = crate::screens::dashboard::roots(&self.managers);
        let shown: Vec<EmployeeSummary> = self.managers.iter()
            .filter(|m| m.status != LifecycleStatus::Archived && matches_search(m, &self.department_name(m.department_id), &query))
            .cloned()
            .collect();
        // Pinned first, in pin order; then the management team, then one group per department in the order they come.
        let pinned: Vec<EmployeeSummary> = pinned_first(shown.clone(), &self.employee_pins, |m| m.id).into_iter().filter(|m| self.employee_pins.contains(&m.id)).collect();
        let shown: Vec<EmployeeSummary> = shown.into_iter().filter(|m| !self.employee_pins.contains(&m.id)).collect();
        let mut groups: Vec<(String, Vec<EmployeeSummary>)> = vec![("PINNED".into(), pinned), ("MANAGEMENT TEAM".into(), shown.iter().filter(|m| team.contains(&m.id)).cloned().collect())];
        let mut departments: Vec<Option<DepartmentId>> = Vec::new();
        for m in shown.iter().filter(|m| !team.contains(&m.id)) {
            if m.department_id.is_some() && !departments.contains(&m.department_id) {
                departments.push(m.department_id);
            }
        }
        departments.push(None);
        for d in departments {
            let label = if d.is_some() { self.department_name(d).to_uppercase() } else { "NO DEPARTMENT".to_string() };
            groups.push((label, shown.iter().filter(|m| m.department_id == d && !team.contains(&m.id)).cloned().collect()));
        }
        groups.retain(|(_, list)| !list.is_empty());
        let mut n = 0;
        let sections: Vec<AnyElement> = groups.iter().map(|(label, list)| {
            v().gap(px(2.))
                .child(div().px(px(8.)).pb(px(4.)).child(ui::eyebrow(format!("{label} · {}", list.len()))))
                .children(list.iter().map(|m| {
                    n += 1;
                    let department = self.department_name(m.department_id);
                    let id = m.id;
                    let pinned = self.employee_pins.contains(&id);
                    self.employee_row(("employee", n), 0, m, (!department.is_empty()).then_some(department), cx).child(
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
    use super::matches_search;
    use crate::screens::org::tests::manager;
    use aurelius_domain::DepartmentId;

    #[test]
    fn search_looks_at_name_role_title_and_department() {
        let mut m = manager(DepartmentId::new(), "Simon", None);
        m.title = Some("Lead server".into());
        assert!(matches_search(&m, "Aurelius", ""));
        assert!(matches_search(&m, "Aurelius", "simon"));
        assert!(matches_search(&m, "Aurelius", "SERVER"));
        assert!(matches_search(&m, "Aurelius", "aure"));
        assert!(!matches_search(&m, "Aurelius", "mobile"));
    }
}
