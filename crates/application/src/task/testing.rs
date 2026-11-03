//! The Testing column (ticket #5, feature `board_testing`). With a company
//! tester, a ticket's main task that reports does not go to Review at once: the
//! card goes to Testing and the engine gives the tester a test sub-task (in the
//! same transaction) that runs the full test suite once. Its run gives the
//! verdict with `report_test_result`:
//!
//! - passed: the card goes to Review as after any report, with the summary in
//!   the discussion;
//! - failed: the summary goes to the owner in the main task's conversation and
//!   the main task works on (the card goes back to Doing); its next report goes
//!   to Testing again.
//!
//! A test sub-task that ends without a verdict (failed, cancelled, or finished
//! without calling the tool) puts the card in Review with a line that says
//! so: a board member decides, and a card never stays in Testing. The main
//! task keeps status Review the whole time; only the card is in Testing.
//!
//! Without a tester (or with a paused one) nothing of this happens: the card
//! goes from Doing to Review as before.

use crate::task::ticket_line::{self, ticket_event};
use aurelius_domain::{
    EventKind, EventNotice, ProjectId, RunId, TaskId, TicketId,
    task::TaskStatus,
    ticket::{Actor, ColumnKind},
    views::{SystemLine, SystemLineKind},
};
use aurelius_storage::{StorageError, Store, Tx, company as company_store, tasks as task_store, tickets as store};
use serde_json::json;

/// The most a verdict's summary may hold (characters).
pub const TEST_SUMMARY_MAX: usize = 8_000;

/// What a run is told when it may not give a verdict.
pub const NOT_THE_TEST_TASK: &str = "Only the open test task of a ticket in Testing can report a test result.";

/// What every run of a ticket's task is told while the company has a tester
/// (the run's system text, its tail), other than the test task itself.
pub const TARGETED_TESTS_ONLY: &str = "Do not run the full test suite: the Testing column runs it once after you report. Run only targeted tests for what you changed (cargo test -p <crate> <filter>, gen-dart --check). If you review work on this ticket, do not run the suite again either: use the Testing result in the ticket's discussion.";

/// How a run's task stands to the Testing column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestingRole {
    /// No tester, or not a ticket's task: nothing changes.
    Outside,
    /// Work on a ticket (the main task, a delegation, a review): targeted tests only.
    Worker,
    /// The ticket's test task: it runs the suite once and gives the verdict.
    Tester,
}

impl TestingRole {
    /// The line for the run's system text, if any.
    pub fn note(self) -> Option<&'static str> {
        match self {
            Self::Worker => Some(TARGETED_TESTS_ONLY),
            Self::Outside | Self::Tester => None,
        }
    }
}

/// How the task of a run stands to the Testing column.
pub(crate) fn role(conn: &rusqlite::Connection, task: TaskId) -> Result<TestingRole, StorageError> {
    if company_store::active_tester(conn)?.is_none() || ticket_line::ticket_of(conn, task)?.is_none() {
        return Ok(TestingRole::Outside);
    }
    Ok(if is_test_task(conn, task)? { TestingRole::Tester } else { TestingRole::Worker })
}

/// The test sub-task's title.
pub(crate) fn test_title(number: u32) -> String {
    format!("Test ticket #{number}")
}

/// The test sub-task's goal: the owner's report, then how to test it once.
pub(crate) fn test_goal(number: u32, report: &str) -> String {
    let report = report.trim();
    let report = if report.is_empty() { "(The owner's report is empty: find the branch in the ticket's discussion.)" } else { report };
    format!(
        "Run the full test suite once for ticket #{number} and give the verdict with report_test_result.\n\n\
         The owner's report:\n\n---\n{report}\n---\n\n\
         How:\n\
         1. Make your own clean review worktree of the branch named in the report. Do not work in the owner's worktree and do not change the branch: no commits, no merges, no pushes.\n\
         2. Run the full test suite exactly once, in the background, with its output in target/<task-id>/suite.log, and poll it until it ends.\n\
         3. Compare the failures with the known baseline failures of the branch it started from: a known failure is not a new one, but name it.\n\
         4. Finish with report_test_result: passed true only when there is no new failure. The summary gives N passed, N failed, N ignored, every new failure by name with its first lines, and the log path.\n\
         5. Remove your worktree afterwards (only with a clean git status)."
    )
}

/// The tester's message to the owner after a red run.
pub(crate) fn failed_text(summary: &str) -> String {
    format!("Testing failed: {}\n\nFix it and report again.", summary.trim())
}

/// The ticket of a test sub-task (the latest one the engine made for it).
pub(crate) fn ticket_of_test(conn: &rusqlite::Connection, task: TaskId) -> Result<Option<TicketId>, StorageError> {
    store::tested_by(conn, task)
}

/// Whether `task` is a ticket's test sub-task: it is not the owner's work, so
/// the main task never waits for it (it keeps status Review).
pub(crate) fn is_test_task(conn: &rusqlite::Connection, task: TaskId) -> Result<bool, StorageError> {
    Ok(ticket_of_test(conn, task)?.is_some())
}

/// The ticket's test sub-task while it has not ended.
fn open_test_task(tx: &Tx<'_>, ticket: TicketId) -> Result<Option<TaskId>, StorageError> {
    let Some(task) = store::test_task(tx, ticket)? else { return Ok(None) };
    Ok(task_store::load_task(tx, task)?.filter(|r| !r.status.is_terminal()).map(|_| task))
}

/// Where a main task that just reported goes: Testing with an active tester, else Review.
pub(crate) fn column_after_report(tx: &Tx<'_>) -> Result<ColumnKind, StorageError> {
    Ok(if company_store::active_tester(tx)?.is_some() { ColumnKind::Testing } else { ColumnKind::Review })
}

/// The main task `root` reported and its card is in Testing: the tester gets a
/// test sub-task with the report, unless one is still open (an answer of the
/// owner while the tests run does not test again). No tester (any more): the
/// card goes on to Review as it would have.
pub(crate) fn start(tx: &Tx<'_>, st: &Store, root: TaskId, ticket: TicketId, project: ProjectId, report: &str) -> Result<Vec<EventNotice>, StorageError> {
    if open_test_task(tx, ticket)?.is_some() {
        return Ok(vec![]);
    }
    let read = |key: &str| st.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
    let tester = company_store::active_tester(tx)?;
    let colleague = crate::task::tasks::colleagues(tx, &read)?.into_iter().find(|c| Some(c.id) == tester);
    let Some(tester) = colleague else { return to_review(tx, ticket, project, root, ColumnKind::Testing, "there is no active tester") };
    let number = ticket_number(tx, ticket)?.unwrap_or_default();
    let title = test_title(number);
    let note = format!("(Made by the Testing column of ticket #{number}: give the verdict with report_test_result.)");
    let Some((child, title, mut notices)) =
        crate::task::task_runs::insert_part(tx, st, project, None, &tester, &title, &test_goal(number, report), &[], &note, Some(root), None, None, None)?
    else {
        return Ok(vec![]);
    };
    store::set_test_task(tx, ticket, Some(child))?;
    let mut line = SystemLine::new(SystemLineKind::Delegated);
    (line.from, line.to, line.task_id) = (Some(ColumnKind::Testing.as_str().to_owned()), Some(tester.name.clone()), Some(child));
    notices.extend(crate::conversation::discussion::ticket_line(tx, ticket, line, &format!("{} runs the full test suite once: “{title}”", tester.name))?);
    Ok(notices)
}

fn ticket_number(tx: &Tx<'_>, ticket: TicketId) -> Result<Option<u32>, StorageError> {
    use rusqlite::OptionalExtension;
    Ok(tx.query_row("SELECT number FROM tickets WHERE id = ?1", [ticket.to_string()], |r| r.get::<_, i64>(0)).optional()?.map(|n| n as u32))
}

/// The card goes from `from` to Review, where a board member decides: the
/// usual `ticket.review` and Review line of the main task's report, and `why`
/// in the discussion when the tests did not pass through.
fn to_review(tx: &Tx<'_>, ticket: TicketId, project: ProjectId, root: TaskId, from: ColumnKind, why: &str) -> Result<Vec<EventNotice>, StorageError> {
    let mut notices = move_card(tx, ticket, project, from, ColumnKind::Review, why)?;
    notices.extend(ticket_line::review_lines(tx, ticket, project, root)?);
    Ok(notices)
}

fn move_card(tx: &Tx<'_>, ticket: TicketId, project: ProjectId, from: ColumnKind, to: ColumnKind, reason: &str) -> Result<Vec<EventNotice>, StorageError> {
    let target = company_store::column_of_kind(tx, to)?;
    store::move_to(tx, ticket, target.id, None)?;
    Ok(vec![ticket_event(tx, ticket, project, EventKind::TicketMoved, None, json!({ "from": from, "to": to, "reason": reason }))?])
}

/// `report_test_result` of a run: only the run of the open test sub-task of a
/// ticket in Testing gives a verdict; every other call is refused (a tool call
/// is never trusted). Returns (ok, text for the model, notices).
pub(crate) fn verdict(tx: &Tx<'_>, st: &Store, run: RunId, passed: bool, summary: &str) -> Result<(bool, String, Vec<EventNotice>), StorageError> {
    let refuse = |why: &str| Ok((false, why.to_owned(), vec![]));
    let Some((task, row)) = crate::task::task_runs::live_task_of(tx, run)? else { return refuse(NOT_THE_TEST_TASK) };
    let Some(ticket) = ticket_of_test(tx, task)? else { return refuse(NOT_THE_TEST_TASK) };
    let Some(state) = store::state(tx, ticket)? else { return refuse(NOT_THE_TEST_TASK) };
    if state.archived || state.column != ColumnKind::Testing || open_test_task(tx, ticket)? != Some(task) {
        return refuse(NOT_THE_TEST_TASK);
    }
    let Some(root) = state.root_task_id else { return refuse(NOT_THE_TEST_TASK) };
    let summary = summary.trim();
    if summary.is_empty() {
        return refuse("Give a summary: N passed, N failed, N ignored, the new failures and the log path.");
    }
    if summary.chars().count() > TEST_SUMMARY_MAX {
        return refuse(&format!("The summary is too long: at most {TEST_SUMMARY_MAX} characters. Name the new failures and point to the log."));
    }
    let tester = row.employee_id.map(|employee_id| Actor::Employee { employee_id });
    let tester_name = crate::conversation::discussion::actor_name(tx, tester)?;
    let mut notices = vec![ticket_event(tx, ticket, state.project_id, EventKind::TicketTested, tester, json!({ "passed": passed, "task_id": task }))?];
    let mut line = SystemLine::new(SystemLineKind::Reported).with_action(if passed { aurelius_domain::views::system_action::TESTED_PASSED } else { aurelius_domain::views::system_action::TESTED_FAILED });
    (line.from, line.task_id) = (Some(tester_name.clone()), Some(task));
    let outcome = if passed { "the tests passed" } else { "the tests failed" };
    notices.extend(crate::conversation::discussion::ticket_line(tx, ticket, line, &format!("{tester_name}: {outcome} · {summary}"))?);
    let answer = if passed {
        notices.extend(to_review(tx, ticket, state.project_id, root, ColumnKind::Testing, "tested")?);
        "Thanks: the tests passed, the card is in Review. Stop now; this test task is done."
    } else {
        // As a send back, but from the tester: the owner works on and reports again (→ Testing).
        notices.extend(ticket_line::resume_as(tx, st, root, &tester_name, &failed_text(summary), "tests_failed")?);
        "Thanks: the owner has the failures and works on; the next report is tested again. Stop now; this test task is done."
    };
    let report = format!("{}\n\n{summary}", if passed { "Tests passed." } else { "Tests failed." });
    notices.extend(crate::task::task_runs::complete(tx, st, task, &report)?);
    Ok((true, answer.to_owned(), notices))
}

/// The hook of `task_line::set_status` for a test sub-task that ended: while the
/// card is still in Testing no verdict came, so it goes to Review with a line
/// that says why (a board member decides). After a verdict the card has left
/// Testing already and nothing happens.
pub(crate) fn on_ended(tx: &Tx<'_>, task: TaskId, status: TaskStatus) -> Result<Vec<EventNotice>, StorageError> {
    if !status.is_terminal() {
        return Ok(vec![]);
    }
    let Some(ticket) = ticket_of_test(tx, task)? else { return Ok(vec![]) };
    let Some(state) = store::state(tx, ticket)? else { return Ok(vec![]) };
    if state.archived || state.column != ColumnKind::Testing {
        return Ok(vec![]);
    }
    let Some(root) = state.root_task_id else { return Ok(vec![]) };
    let why = match status {
        TaskStatus::Cancelled => "the test task was cancelled",
        TaskStatus::Failed => "the test task failed",
        _ => "the tester finished without a verdict",
    };
    let mut notices = to_review(tx, ticket, state.project_id, root, ColumnKind::Testing, "untested")?;
    let mut line = SystemLine::new(SystemLineKind::Reported);
    (line.from, line.task_id) = (Some("Aurelius".to_owned()), Some(task));
    notices.extend(crate::conversation::discussion::ticket_line(tx, ticket, line, &format!("Testing could not run: {why}. A board member decides."))?);
    Ok(notices)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_test_goal_holds_the_report_and_asks_for_one_background_run_in_its_own_worktree() {
        let goal = test_goal(5, "Done on feat/aur-t5-engine.");
        assert!(goal.contains("ticket #5") && goal.contains("Done on feat/aur-t5-engine."));
        for needed in ["exactly once", "in the background", "suite.log", "own clean review worktree", "do not change the branch", "baseline", "report_test_result"] {
            assert!(goal.contains(needed), "{needed}");
        }
        assert!(test_goal(5, "  ").contains("report is empty"));
        assert_eq!(test_title(5), "Test ticket #5");
    }

    #[test]
    fn only_work_on_a_ticket_is_told_to_run_targeted_tests_only() {
        assert_eq!(TestingRole::Worker.note(), Some(TARGETED_TESTS_ONLY));
        assert_eq!((TestingRole::Outside.note(), TestingRole::Tester.note()), (None, None));
        for needed in ["Do not run the full test suite", "cargo test -p <crate> <filter>", "gen-dart --check", "use the Testing result in the ticket's discussion"] {
            assert!(TARGETED_TESTS_ONLY.contains(needed), "{needed}");
        }
    }

    #[test]
    fn a_red_verdict_tells_the_owner_to_fix_it_and_report_again() {
        assert_eq!(failed_text(" 2 new failures: a, b "), "Testing failed: 2 new failures: a, b\n\nFix it and report again.");
    }
}
