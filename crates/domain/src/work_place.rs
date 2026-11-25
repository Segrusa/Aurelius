//! Where an employee works in a project with a git folder (ticket #86): never in the
//! project's folder itself, but always in a worktree of its own, so two runs never share
//! a checkout or switch a branch under each other. The project's folder stays on main;
//! only a merge touches it.
//!
//! - a work session: `session/<8>` in `worktrees/session-<8>`;
//! - a ticket's main task: `ticket/<N>` in `worktrees/ticket-<N>` (a quick task keeps
//!   `quick/ticket-<N>`);
//! - a delegated part of a ticket: `ticket/<N>-<employee>-<8>` in
//!   `worktrees/ticket-<N>-<8>`, branched from the task it was delegated from;
//! - a task without a ticket: `task/<8>` in `worktrees/task-<8>`.
//!
//! These are names only; the storage reads which one applies and the daemon makes it.

use crate::TaskId;
use std::path::{Path, PathBuf};

/// The folder in the data folder that holds every worktree Aurelius makes.
pub const WORKTREES_DIR: &str = "worktrees";

/// How many characters of an employee's name go into a delegated part's branch.
pub const EMPLOYEE_SLUG_MAX: usize = 24;

/// The folder of the worktrees under the data folder `data_root`.
pub fn worktrees_dir(data_root: &Path) -> PathBuf {
    data_root.join(WORKTREES_DIR)
}

/// The branch of a ticket's main task.
pub fn ticket_branch(number: u32) -> String {
    format!("ticket/{number}")
}

/// The worktree folder of a ticket's main task (a quick task uses the same one).
pub fn ticket_worktree(worktrees: &Path, number: u32) -> PathBuf {
    worktrees.join(format!("ticket-{number}"))
}

/// The branch of a part of ticket `number` delegated to `employee`: the ticket's number
/// first, so the Code tab links it to the ticket.
pub fn part_branch(number: u32, employee: &str, task: TaskId) -> String {
    let slug = slug(employee);
    if slug.is_empty() {
        format!("ticket/{number}-{}", short(task))
    } else {
        format!("ticket/{number}-{slug}-{}", short(task))
    }
}

/// The worktree folder of a delegated part of ticket `number`.
pub fn part_worktree(worktrees: &Path, number: u32, task: TaskId) -> PathBuf {
    worktrees.join(format!("ticket-{number}-{}", short(task)))
}

/// The branch of a task without a ticket.
pub fn task_branch(task: TaskId) -> String {
    format!("task/{}", short(task))
}

/// The worktree folder of a task without a ticket.
pub fn task_worktree(worktrees: &Path, task: TaskId) -> PathBuf {
    worktrees.join(format!("task-{}", short(task)))
}

/// The last eight characters of a task's id (its random part), as in a session's branch.
fn short(task: TaskId) -> String {
    let id = task.to_string();
    id[id.len().saturating_sub(8)..].to_owned()
}

/// A name as a branch part: lower-case ASCII letters and digits, the rest one `-`.
fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
        if out.len() >= EMPLOYEE_SLUG_MAX {
            break;
        }
    }
    out.trim_end_matches('-').to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_parts_branch_starts_with_its_tickets_number_so_the_code_tab_links_it() {
        let task = TaskId::new();
        let branch = part_branch(86, "Forge", task);
        assert!(branch.starts_with("ticket/86-forge-"), "{branch}");
        assert_eq!(crate::code::ticket_number_of(&branch), Some(86));
        assert_eq!(crate::code::ticket_number_of(&ticket_branch(86)), Some(86));
    }

    #[test]
    fn an_employee_name_becomes_a_safe_branch_part() {
        assert_eq!(slug("Sébastien de C."), "s-bastien-de-c");
        assert_eq!(slug("../--"), "");
        assert!(part_branch(3, "~:^", TaskId::new()).starts_with("ticket/3-"));
        assert!(slug(&"x".repeat(100)).len() <= EMPLOYEE_SLUG_MAX);
    }
}
