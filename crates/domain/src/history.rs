//! History (ticket #57, feature `ticket_history`): every ticket on Done, newest
//! first, with what it delivered. The summary is the one the extraction makes at
//! Done for the project's memory (MU1-18); it is kept on the ticket too
//! (`tickets.summary_text`), so the History needs no second model call.

use crate::{ConversationId, EmployeeId, ProjectId, TicketId, UserId};
use serde::{Deserialize, Serialize};

/// A ticket's summary on the ticket is cut here (the memory's is cut at the same length).
pub const HISTORY_SUMMARY_MAX_CHARS: usize = 800;
/// Rows of one `ticket.history` page when the client does not say.
pub const HISTORY_PAGE: u32 = 50;
/// Most rows one `ticket.history` page has, so one read stays bounded.
pub const HISTORY_PAGE_MAX: u32 = 100;
/// The text a History search looks for is cut here.
pub const HISTORY_QUERY_MAX_CHARS: usize = 200;

/// What `tickets.summary_state` holds (the column's CHECK lists the same words).
pub mod stored {
    /// The ticket went to Done; its extraction should bring the summary.
    pub const PENDING: &str = "pending";
    /// `tickets.summary_text` holds the summary.
    pub const READY: &str = "ready";
    /// The extraction failed or gave no summary; `ticket.retry_summary` tries again.
    pub const FAILED: &str = "failed";
}

/// Where a row's summary stands, as the History shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HistorySummaryState {
    /// `summary` holds the text.
    Ready,
    /// The extraction is queued or running ("Summarizing…").
    Summarizing,
    /// It failed or gave nothing ("Summary not available"); a board member may try again.
    NotAvailable,
    /// The ticket went to Done before the History existed and its project memory
    /// has no summary of it ("No summary (finished before History)").
    BeforeHistory,
}

impl HistorySummaryState {
    /// The state a row shows: a text wins, then a job under way, then what the
    /// ticket's own state says. `stored`: `tickets.summary_state` (none: a ticket
    /// that went to Done before the History, without a summary in memory).
    pub fn of(has_text: bool, pending_job: bool, stored: Option<&str>) -> Self {
        if has_text {
            Self::Ready
        } else if pending_job {
            Self::Summarizing
        } else if stored.is_some() {
            Self::NotAvailable
        } else {
            Self::BeforeHistory
        }
    }
}

/// One finished ticket in the History.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct HistoryEntry {
    pub ticket_id: TicketId,
    pub number: u32,
    pub title: String,
    pub project_id: ProjectId,
    pub project_name: String,
    /// The ticket's owner (an archived employee keeps its name); none: "—".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_employee_id: Option<EmployeeId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_name: Option<String>,
    /// When the ticket went to Done (for a quick task: after its merge).
    pub done_at_ms: i64,
    /// What the ticket delivered (through the vault redactor), when `summary_state` is `ready`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    pub summary_state: HistorySummaryState,
}

/// `ticket.history`: finished tickets, newest first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct HistoryPage {
    pub as_of_seq: u64,
    pub entries: Vec<HistoryEntry>,
    /// There are older rows: ask again with the last row's `done_at_ms` and `number`.
    #[serde(default)]
    pub has_more: bool,
}

/// A session's summary is made once it has been quiet this long after an answer
/// (feature `session_history`): one model call per stretch of work, not per answer.
pub const SESSION_SUMMARY_QUIET_MS: i64 = 10 * 60_000;
/// How much of a session's new messages one summary reads (the newest win).
pub const SESSION_SUMMARY_INPUT_MAX_CHARS: usize = 24_000;
/// One message in a session summary's input is cut here.
pub const SESSION_SUMMARY_MESSAGE_MAX_CHARS: usize = 4_000;

/// One work session in the History (feature `session_history`): on the day of its
/// last activity, with a summary of what was done in it. Every board member's
/// session is in it; only the one who started it writes in it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SessionHistoryEntry {
    pub conversation_id: ConversationId,
    pub title: String,
    pub project_id: ProjectId,
    pub project_name: String,
    pub employee_id: EmployeeId,
    pub employee_name: String,
    /// The board member who started it (the only one who writes in it).
    pub started_by: UserId,
    /// Their name; none when the user is gone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_by_name: Option<String>,
    /// The session's own branch, when it works in a worktree.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    pub created_at_ms: i64,
    /// The newest message (else its start): the day it shows on.
    pub last_activity_ms: i64,
    /// What was done in it (through the vault redactor), when `summary_state` is `ready`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// `before_history` here: no summary was asked for yet ("No summary yet").
    pub summary_state: HistorySummaryState,
}

/// `work_session.history`: sessions, the latest activity first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SessionHistoryPage {
    pub as_of_seq: u64,
    pub entries: Vec<SessionHistoryEntry>,
    /// There are older rows: ask again with the last row's `last_activity_ms` and `conversation_id`.
    #[serde(default)]
    pub has_more: bool,
}

/// The text kept on the ticket: whitespace flattened, empty is none, cut at
/// `HISTORY_SUMMARY_MAX_CHARS` with an ellipsis.
pub fn history_summary(raw: &str) -> Option<String> {
    let flat = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.is_empty() {
        return None;
    }
    if flat.chars().count() <= HISTORY_SUMMARY_MAX_CHARS {
        return Some(flat);
    }
    let mut cut: String = flat.chars().take(HISTORY_SUMMARY_MAX_CHARS - 1).collect();
    cut.push('…');
    Some(cut)
}

/// The summary in a ticket's project memory written before the History
/// ("Ticket #n “title” is done: …", `memory::ticket_summary`): the part after
/// the head. None when it has no body ("… is done.") or is not of that form.
pub fn summary_from_memory(number: u32, text: &str) -> Option<String> {
    let rest = text.trim().strip_prefix(&format!("Ticket #{number} “"))?;
    let (_, body) = rest.split_once("” is done: ")?;
    history_summary(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_text_wins_then_a_job_then_the_stored_state() {
        assert_eq!(HistorySummaryState::of(true, true, Some(stored::PENDING)), HistorySummaryState::Ready);
        assert_eq!(HistorySummaryState::of(false, true, Some(stored::FAILED)), HistorySummaryState::Summarizing);
        assert_eq!(HistorySummaryState::of(false, false, Some(stored::PENDING)), HistorySummaryState::NotAvailable, "pending without a job never waits forever");
        assert_eq!(HistorySummaryState::of(false, false, Some(stored::FAILED)), HistorySummaryState::NotAvailable);
        assert_eq!(HistorySummaryState::of(false, false, None), HistorySummaryState::BeforeHistory);
    }

    #[test]
    fn a_summary_is_flattened_and_cut() {
        assert_eq!(history_summary("  It shipped\n the  page. "), Some("It shipped the page.".into()));
        assert_eq!(history_summary(" \n "), None);
        let long = history_summary(&"a".repeat(HISTORY_SUMMARY_MAX_CHARS + 10)).unwrap();
        assert_eq!(long.chars().count(), HISTORY_SUMMARY_MAX_CHARS);
        assert!(long.ends_with('…'));
    }

    #[test]
    fn the_memory_summary_of_an_older_ticket_loses_its_head() {
        let text = crate::memory::ticket_summary(7, "Fix “the” login", Some("It fixed the login. Decided: no SSO."), &[]);
        assert_eq!(summary_from_memory(7, &text), Some("It fixed the login. Decided: no SSO.".into()));
        assert_eq!(summary_from_memory(7, &crate::memory::ticket_summary(7, "Empty", None, &[])), None, "only the close: no summary");
        assert_eq!(summary_from_memory(8, &text), None, "another ticket's number");
        assert_eq!(summary_from_memory(7, "Something else entirely"), None);
    }
}
