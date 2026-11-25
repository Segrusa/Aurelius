//! Reviewing a branch in the Code tab (ticket #84, feature `code_review`), the way a pull
//! request is reviewed: a board member comments on a line of the diff (a thread), others
//! reply, a thread is resolved. A new comment stays *pending* (only its writer sees it)
//! until they finish their review with a verdict, so the branch's owner is woken once with
//! all of it. Each board member also ticks the files they viewed, per branch head.

use crate::{CodeCommentId, CodeReviewId, CodeThreadId, DomainError, ProjectId, optional_text, require_text};
use serde::{Deserialize, Serialize};

/// Longest comment or reply.
pub const CODE_COMMENT_MAX_CHARS: usize = 8_000;
/// Longest summary of a finished review.
pub const CODE_REVIEW_BODY_MAX_CHARS: usize = 8_000;
/// Most diff lines a thread keeps as the code it is about.
pub const CODE_SNIPPET_LINES_MAX: usize = 6;
/// Longest snippet line kept.
pub const CODE_SNIPPET_LINE_MAX_CHARS: usize = 300;
/// Longest file path a thread or a viewed mark names.
pub const CODE_PATH_MAX_CHARS: usize = 1_024;
/// Longest branch name (git allows more; no branch of this company is that long).
pub const CODE_BRANCH_MAX_CHARS: usize = 255;
/// Longest commit id (a SHA-256 hash in hex).
pub const CODE_COMMIT_MAX_CHARS: usize = 64;
/// Shortest commit id: an abbreviated one could name another commit later.
pub const CODE_COMMIT_MIN_CHARS: usize = 7;
/// Highest line number a thread can be on.
pub const CODE_LINE_MAX: u32 = 10_000_000;

/// Which side of the diff a line comment is on: the branch's new line, or main's removed one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CodeLineSide {
    New,
    Old,
}

impl CodeLineSide {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::New => "new",
            Self::Old => "old",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "new" => Some(Self::New),
            "old" => Some(Self::Old),
            _ => None,
        }
    }
}

/// How a board member finished their review.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CodeVerdict {
    /// Comments only: the owner answers, nothing else changes.
    Comment,
    /// The board member agrees with the change; merging stays a separate action.
    Approved,
    /// The owner must change it (a ticket goes back to Doing; the client sends it back).
    Changes,
}

impl CodeVerdict {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Comment => "comment",
            Self::Approved => "approved",
            Self::Changes => "changes",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "comment" => Some(Self::Comment),
            "approved" => Some(Self::Approved),
            "changes" => Some(Self::Changes),
            _ => None,
        }
    }
}

fn branch(value: &str) -> Result<String, DomainError> {
    let b = require_text("branch", value, CODE_BRANCH_MAX_CHARS)?;
    if b.starts_with('-') || b.chars().any(char::is_control) {
        return Err(DomainError::OutOfRange { field: "branch".into(), detail: "not a branch name".into() });
    }
    Ok(b)
}

fn path(value: &str) -> Result<String, DomainError> {
    let p = require_text("path", value, CODE_PATH_MAX_CHARS)?;
    if p.chars().any(char::is_control) {
        return Err(DomainError::OutOfRange { field: "path".into(), detail: "not a file path".into() });
    }
    Ok(p)
}

fn commit(field: &str, value: &str) -> Result<String, DomainError> {
    let c = value.trim();
    if c.len() < CODE_COMMIT_MIN_CHARS || c.len() > CODE_COMMIT_MAX_CHARS || !c.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(DomainError::OutOfRange { field: field.into(), detail: format!("a commit id of {CODE_COMMIT_MIN_CHARS} to {CODE_COMMIT_MAX_CHARS} hex digits") });
    }
    Ok(c.to_ascii_lowercase())
}

fn comment(value: &str) -> Result<String, DomainError> {
    require_text("body", value, CODE_COMMENT_MAX_CHARS)
}

/// `code.start_thread`: a board member's first comment on a line of a branch's diff. It is
/// pending until they finish their review. Answers `CodeThreadStarted`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StartCodeThread {
    pub project_id: ProjectId,
    pub branch: String,
    pub path: String,
    /// The line number on `side` (1 is the first line).
    pub line: u32,
    pub side: CodeLineSide,
    /// The branch head the diff was read at (`CodeDiff.head`).
    pub commit: String,
    /// The diff lines the comment is about, as shown (' ', '+' or '-' first), the last
    /// one the commented line. Kept so the thread still shows its code after the line changes.
    #[serde(default)]
    pub snippet: Vec<String>,
    pub body: String,
}

/// A `StartCodeThread` as it is stored: trimmed and within the limits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidThread {
    pub project_id: ProjectId,
    pub branch: String,
    pub path: String,
    pub line: u32,
    pub side: CodeLineSide,
    pub commit: String,
    pub snippet: Vec<String>,
    pub body: String,
}

impl StartCodeThread {
    pub fn validate(&self) -> Result<ValidThread, DomainError> {
        if self.line == 0 || self.line > CODE_LINE_MAX {
            return Err(DomainError::OutOfRange { field: "line".into(), detail: format!("a line from 1 to {CODE_LINE_MAX}") });
        }
        // The last lines are kept: the commented line is the last one.
        let skip = self.snippet.len().saturating_sub(CODE_SNIPPET_LINES_MAX);
        let snippet = self.snippet[skip..].iter().map(|l| l.chars().take(CODE_SNIPPET_LINE_MAX_CHARS).collect()).collect();
        Ok(ValidThread {
            project_id: self.project_id,
            branch: branch(&self.branch)?,
            path: path(&self.path)?,
            line: self.line,
            side: self.side,
            commit: commit("commit", &self.commit)?,
            snippet,
            body: comment(&self.body)?,
        })
    }
}

/// `code.reply`: a reply in a thread, pending until its writer finishes their review.
/// Answers `CodeCommented`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReplyCodeThread {
    pub thread_id: CodeThreadId,
    pub body: String,
}

impl ReplyCodeThread {
    pub fn validate(&self) -> Result<String, DomainError> {
        comment(&self.body)
    }
}

/// `code.resolve_thread`: mark a thread resolved (it folds) or open again. Answers
/// `CodeThreadResolved`; the same state again changes nothing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResolveCodeThread {
    pub thread_id: CodeThreadId,
    pub resolved: bool,
}

/// `code.set_viewed`: the board member of the session viewed a file of a branch at a head
/// (or not any more). A new head starts unviewed. Answers `CodeViewedSet`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetCodeViewed {
    pub project_id: ProjectId,
    pub branch: String,
    pub head: String,
    pub path: String,
    pub viewed: bool,
}

/// A `SetCodeViewed` as it is stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidViewed {
    pub project_id: ProjectId,
    pub branch: String,
    pub head: String,
    pub path: String,
    pub viewed: bool,
}

impl SetCodeViewed {
    pub fn validate(&self) -> Result<ValidViewed, DomainError> {
        Ok(ValidViewed { project_id: self.project_id, branch: branch(&self.branch)?, head: commit("head", &self.head)?, path: path(&self.path)?, viewed: self.viewed })
    }
}

/// `code.submit_review`: finish the board member's review of a branch at `head`. Their
/// pending comments on it are sent with it. A comment or a request for changes needs a
/// summary or a pending comment; an approval needs neither. Answers `CodeReviewSubmitted`.
/// Waking the owner is the client's next write (`ticket.post`, `ticket.send_back` or
/// `chat.send`), with the text it shows the board member.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SubmitCodeReview {
    pub project_id: ProjectId,
    pub branch: String,
    pub head: String,
    pub verdict: CodeVerdict,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
}

/// A `SubmitCodeReview` as it is stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidReview {
    pub project_id: ProjectId,
    pub branch: String,
    pub head: String,
    pub verdict: CodeVerdict,
    pub body: String,
}

impl SubmitCodeReview {
    pub fn validate(&self) -> Result<ValidReview, DomainError> {
        Ok(ValidReview {
            project_id: self.project_id,
            branch: branch(&self.branch)?,
            head: commit("head", &self.head)?,
            verdict: self.verdict,
            body: optional_text("body", self.body.as_deref(), CODE_REVIEW_BODY_MAX_CHARS)?.unwrap_or_default(),
        })
    }
}

/// `code.review` (feature `code_review`): a branch's review as the reader sees it. Other
/// board members' pending comments are left out.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CodeReviewView {
    /// Oldest first.
    #[serde(default)]
    pub threads: Vec<CodeThreadView>,
    /// Finished reviews, oldest first.
    #[serde(default)]
    pub reviews: Vec<CodeReviewEntry>,
    /// The files the reader marked viewed at the asked head.
    #[serde(default)]
    pub viewed: Vec<String>,
}

/// One line thread.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CodeThreadView {
    pub id: CodeThreadId,
    pub path: String,
    pub line: u32,
    pub side: CodeLineSide,
    /// The head it was made on: a client marks it Outdated when the head moved on.
    pub commit: String,
    #[serde(default)]
    pub snippet: Vec<String>,
    #[serde(default)]
    pub resolved: bool,
    pub created_at_ms: i64,
    /// Oldest first; never empty for a thread the reader can see.
    #[serde(default)]
    pub comments: Vec<CodeCommentView>,
}

/// One comment of a thread.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CodeCommentView {
    pub id: CodeCommentId,
    pub username: String,
    /// Written by the reader.
    #[serde(default)]
    pub mine: bool,
    pub body: String,
    /// Not sent yet: only its writer sees it, until they finish their review.
    #[serde(default)]
    pub pending: bool,
    /// The review it was sent with.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_id: Option<CodeReviewId>,
    pub created_at_ms: i64,
}

/// One finished review.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CodeReviewEntry {
    pub id: CodeReviewId,
    pub username: String,
    #[serde(default)]
    pub mine: bool,
    pub verdict: CodeVerdict,
    #[serde(default)]
    pub body: String,
    /// The branch head it was given at: an approval of an older head is not one of this head.
    pub head: String,
    pub created_at_ms: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn thread() -> StartCodeThread {
        StartCodeThread {
            project_id: ProjectId::new(),
            branch: "feat/ticket-84".into(),
            path: "src/a.rs".into(),
            line: 12,
            side: CodeLineSide::New,
            commit: "ABCDEF1234".into(),
            snippet: (0..9).map(|n| format!("+line {n}")).collect(),
            body: "  Rename this.  ".into(),
        }
    }

    #[test]
    fn a_thread_keeps_the_last_snippet_lines_and_a_trimmed_comment() {
        let valid = thread().validate().unwrap();
        assert_eq!(valid.snippet.len(), CODE_SNIPPET_LINES_MAX);
        assert_eq!(valid.snippet.last().map(String::as_str), Some("+line 8"), "the commented line is the last");
        assert_eq!(valid.body, "Rename this.");
        assert_eq!(valid.commit, "abcdef1234");
    }

    #[test]
    fn a_thread_needs_a_real_line_a_commit_id_and_words() {
        assert!(StartCodeThread { line: 0, ..thread() }.validate().is_err());
        assert!(StartCodeThread { commit: "abc".into(), ..thread() }.validate().is_err(), "too short to be sure");
        assert!(StartCodeThread { commit: "main".into(), ..thread() }.validate().is_err(), "a name is no commit id");
        assert!(StartCodeThread { body: "   ".into(), ..thread() }.validate().is_err());
        assert!(StartCodeThread { branch: "--force".into(), ..thread() }.validate().is_err(), "never an option");
    }

    #[test]
    fn a_review_without_words_is_an_empty_summary() {
        let r = SubmitCodeReview { project_id: ProjectId::new(), branch: "b".into(), head: "1234567".into(), verdict: CodeVerdict::Approved, body: Some("  ".into()) };
        assert_eq!(r.validate().unwrap().body, "");
    }

    #[test]
    fn verdicts_and_sides_read_back_what_they_write() {
        for v in [CodeVerdict::Comment, CodeVerdict::Approved, CodeVerdict::Changes] {
            assert_eq!(CodeVerdict::parse(v.as_str()), Some(v));
        }
        for s in [CodeLineSide::New, CodeLineSide::Old] {
            assert_eq!(CodeLineSide::parse(s.as_str()), Some(s));
        }
    }
}
