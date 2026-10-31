//! The org chart of the company: who reports to whom. Pure rules, no IO.
//!
//! The company's employees form a forest (several roots are fine: the roots
//! are the management team). Archiving an employee moves its reports one level
//! up. With the server setting "Follow the org chart" on (the default),
//! delegation goes to direct reports only, a task is handed over to the
//! boss or a peer, and "Needs you" goes to the boss first.

use crate::EmployeeId;
use std::collections::{HashMap, HashSet};

/// Whether a role or title names the CEO ("CEO", "Chief Executive Officer").
pub fn names_ceo(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.split(|c: char| !c.is_alphanumeric()).any(|w| w == "ceo") || lower.contains("chief executive")
}

/// The CEO among the roots of the chart, which are given as `(employee, role,
/// title)` oldest first: the first whose role or title names the CEO, else the
/// first root (a company with one root has its CEO there).
pub fn pick_ceo(roots: &[(EmployeeId, String, Option<String>)]) -> Option<EmployeeId> {
    roots
        .iter()
        .find(|(_, role, title)| names_ceo(role) || title.as_deref().is_some_and(names_ceo))
        .or_else(|| roots.first())
        .map(|(id, _, _)| *id)
}

/// The deepest delegation chain the org chart allows.
pub const MAX_CHART_DEPTH: u32 = 6;

#[derive(Debug, Clone, Default)]
pub struct Org {
    parents: HashMap<EmployeeId, Option<EmployeeId>>,
}

impl Org {
    /// `(employee, reports_to)` for every employee on the chart (not archived).
    pub fn new(entries: impl IntoIterator<Item = (EmployeeId, Option<EmployeeId>)>) -> Self {
        Self { parents: entries.into_iter().collect() }
    }

    /// Those without a boss on the chart (the management team), in id order.
    pub fn roots(&self) -> Vec<EmployeeId> {
        let mut out: Vec<EmployeeId> = self.parents.keys().copied().filter(|m| self.boss(*m).is_none()).collect();
        out.sort();
        out
    }

    pub fn contains(&self, employee: EmployeeId) -> bool {
        self.parents.contains_key(&employee)
    }

    /// The boss of `employee`, if any (and still on the chart).
    pub fn boss(&self, employee: EmployeeId) -> Option<EmployeeId> {
        self.parents.get(&employee).copied().flatten().filter(|b| self.contains(*b))
    }

    /// `employee` itself, its boss, the boss's boss … up to a root.
    pub fn chain(&self, employee: EmployeeId) -> Vec<EmployeeId> {
        let mut out = vec![employee];
        let mut seen: HashSet<EmployeeId> = HashSet::from([employee]);
        let mut at = employee;
        while let Some(boss) = self.boss(at) {
            if !seen.insert(boss) {
                break;
            }
            out.push(boss);
            at = boss;
        }
        out
    }

    /// Making `boss` the boss of `employee` would make a loop (or an employee its own boss).
    pub fn would_cycle(&self, employee: EmployeeId, boss: EmployeeId) -> bool {
        self.chain(boss).contains(&employee)
    }

    /// Everyone who reports to `employee`, directly or not.
    pub fn subtree(&self, employee: EmployeeId) -> Vec<EmployeeId> {
        let mut out = Vec::new();
        let mut stack = self.reports(employee);
        let mut seen: HashSet<EmployeeId> = HashSet::from([employee]);
        while let Some(next) = stack.pop() {
            if seen.insert(next) {
                out.push(next);
                stack.extend(self.reports(next));
            }
        }
        out
    }

    /// Those who report directly to `employee`.
    pub fn reports(&self, employee: EmployeeId) -> Vec<EmployeeId> {
        let mut out: Vec<EmployeeId> = self.parents.keys().copied().filter(|m| self.boss(*m) == Some(employee)).collect();
        out.sort();
        out
    }

    /// The management for an app key with audience `management`: the CEO and
    /// those who report to the CEO directly (CEO first, then in id order).
    pub fn management(&self, ceo: EmployeeId) -> Vec<EmployeeId> {
        if !self.contains(ceo) {
            return Vec::new();
        }
        let mut out = vec![ceo];
        out.extend(self.reports(ceo));
        out
    }

    /// Employees with the same boss (roots are peers of each other), not `employee` itself.
    pub fn peers(&self, employee: EmployeeId) -> Vec<EmployeeId> {
        let boss = self.boss(employee);
        let mut out: Vec<EmployeeId> = self.parents.keys().copied().filter(|m| *m != employee && self.boss(*m) == boss).collect();
        out.sort();
        out
    }

    /// Levels in the longest chain: a flat company is 1, boss → report is 2.
    pub fn height(&self) -> u32 {
        self.parents.keys().map(|m| self.chain(*m).len() as u32).max().unwrap_or(1)
    }

    /// How deep tasks may be delegated while the chart is followed: the depth
    /// of the tree, at most `MAX_CHART_DEPTH`.
    pub fn depth_limit(&self) -> u32 {
        self.height().min(MAX_CHART_DEPTH)
    }

    /// `to` reports directly to `from`: delegation goes one level down the chart only.
    pub fn may_delegate(&self, from: EmployeeId, to: EmployeeId) -> bool {
        self.boss(to) == Some(from)
    }

    /// For a `to` that is below `from` but not a direct report: the direct report of `from` to route through.
    pub fn route_down(&self, from: EmployeeId, to: EmployeeId) -> Option<EmployeeId> {
        self.chain(to).into_iter().find(|a| self.boss(*a) == Some(from))
    }

    /// A whole task goes to the boss or to a peer, never down or further up.
    pub fn may_hand_over(&self, from: EmployeeId, to: EmployeeId) -> bool {
        self.boss(from) == Some(to) || self.peers(from).contains(&to)
    }

    /// Where "Needs you" goes first: the boss. A root escalates to the user.
    pub fn escalation_target(&self, employee: EmployeeId) -> Option<EmployeeId> {
        self.boss(employee)
    }

    /// Archiving `employee`: its reports move one level up, to its boss (or become roots).
    pub fn on_archive(&self, employee: EmployeeId) -> Vec<(EmployeeId, Option<EmployeeId>)> {
        let boss = self.boss(employee);
        self.reports(employee).into_iter().map(|m| (m, boss)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// jean → simon → (dean, max), and a separate root ana.
    fn chart() -> (Org, [EmployeeId; 5]) {
        let ids = [EmployeeId::new(), EmployeeId::new(), EmployeeId::new(), EmployeeId::new(), EmployeeId::new()];
        let [jean, simon, dean, max, ana] = ids;
        (Org::new([(jean, None), (simon, Some(jean)), (dean, Some(simon)), (max, Some(simon)), (ana, None)]), ids)
    }

    #[test]
    fn a_employee_cannot_report_into_its_own_subtree() {
        let (org, [jean, simon, dean, max, ana]) = chart();
        assert!(org.would_cycle(jean, dean), "jean under dean closes the loop");
        assert!(org.would_cycle(simon, simon));
        assert!(org.would_cycle(jean, jean));
        assert!(!org.would_cycle(dean, max), "peers may be re-ordered");
        assert!(!org.would_cycle(dean, ana));
        assert!(!org.would_cycle(simon, ana));
    }

    #[test]
    fn the_management_is_the_ceo_and_its_direct_reports() {
        let (org, [jean, simon, dean, _, ana]) = chart();
        assert_eq!(org.management(jean), vec![jean, simon]);
        assert!(!org.management(jean).contains(&dean), "two levels down is not management");
        assert!(!org.management(jean).contains(&ana), "another root is not a report");
        assert_eq!(org.management(EmployeeId::new()), Vec::<EmployeeId>::new());
        let roots = vec![(ana, "Assistant".to_owned(), None), (jean, "Lead".to_owned(), Some("Chief Executive Officer".to_owned()))];
        assert_eq!(pick_ceo(&roots), Some(jean), "the title names the CEO");
        assert_eq!(pick_ceo(&[(ana, "Assistant".into(), None), (jean, "Lead".into(), None)]), Some(ana), "else the oldest root");
        assert!(names_ceo("CEO") && names_ceo("co-ceo of x") && !names_ceo("Receptionist") && !names_ceo("Concierge"));
        assert_eq!(pick_ceo(&[]), None);
    }

    #[test]
    fn delegation_goes_down_and_hand_over_goes_up_or_sideways() {
        let (org, [jean, simon, dean, max, ana]) = chart();
        assert!(org.may_delegate(simon, dean) && org.may_delegate(simon, max) && org.may_delegate(jean, simon));
        assert!(!org.may_delegate(jean, dean), "two levels down goes through the report");
        assert_eq!(org.route_down(jean, dean), Some(simon));
        assert_eq!(org.route_down(simon, jean), None);
        assert!(!org.may_delegate(dean, max), "sideways is not down");
        assert!(!org.may_delegate(simon, jean));
        assert!(!org.may_delegate(jean, ana), "another root is outside the subtree");
        assert!(org.may_hand_over(dean, simon) && org.may_hand_over(dean, max));
        assert!(!org.may_hand_over(dean, jean), "only the boss, not the boss's boss");
        assert!(!org.may_hand_over(simon, dean));
        assert!(org.may_hand_over(jean, ana), "roots are peers");
    }

    #[test]
    fn escalation_climbs_to_the_root_and_the_depth_follows_the_tree() {
        let (org, [jean, simon, dean, _, ana]) = chart();
        assert_eq!(org.escalation_target(dean), Some(simon));
        assert_eq!(org.escalation_target(simon), Some(jean));
        assert_eq!(org.escalation_target(jean), None);
        assert_eq!(org.escalation_target(ana), None);
        assert_eq!(org.chain(dean), vec![dean, simon, jean]);
        assert_eq!(org.depth_limit(), 3);
        assert_eq!(Org::default().depth_limit(), 1);
        // A very deep chart stops at the maximum.
        let ids: Vec<EmployeeId> = (0..7).map(|_| EmployeeId::new()).collect();
        let deep = Org::new(ids.iter().enumerate().map(|(i, m)| (*m, i.checked_sub(1).map(|p| ids[p]))));
        assert_eq!(deep.depth_limit(), MAX_CHART_DEPTH);
    }

    #[test]
    fn archiving_moves_the_reports_one_level_up() {
        let (org, [jean, simon, dean, max, _]) = chart();
        let mut moved = org.on_archive(simon);
        moved.sort();
        let mut expected = vec![(dean, Some(jean)), (max, Some(jean))];
        expected.sort();
        assert_eq!(moved, expected);
        assert_eq!(org.on_archive(jean), vec![(simon, None)]);
        assert!(org.on_archive(dean).is_empty());
    }

    #[test]
    fn a_flat_company_has_no_bosses_and_everyone_is_a_peer() {
        let (a, b) = (EmployeeId::new(), EmployeeId::new());
        let org = Org::new([(a, None), (b, None)]);
        assert_eq!(org.boss(a), None);
        assert!(org.may_hand_over(a, b) && !org.may_delegate(a, b));
        assert_eq!(org.height(), 1);
        let mut roots = vec![a, b];
        roots.sort();
        assert_eq!(org.roots(), roots);
    }

    #[test]
    fn the_company_chart_knows_its_roots() {
        let (ceo, cto, dev) = (EmployeeId::new(), EmployeeId::new(), EmployeeId::new());
        let org = Org::new([(ceo, None), (cto, Some(ceo)), (dev, Some(cto))]);
        assert_eq!(org.roots(), vec![ceo]);
        assert!(org.may_delegate(ceo, cto) && !org.may_delegate(ceo, dev), "delegation goes one level down");
        assert_eq!(org.depth_limit(), 3);
    }
}
