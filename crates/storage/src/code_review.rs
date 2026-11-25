//! A branch's review in the Code tab (migration 53, feature `code_review`, ticket #84):
//! line threads with their comments, finished reviews and the files each board member
//! viewed per branch head. A comment without a review is pending: only its writer sees it.

use crate::StorageError;
use aurelius_domain::code_review::{CodeCommentView, CodeLineSide, CodeReviewEntry, CodeReviewView, CodeThreadView, CodeVerdict, ValidReview, ValidThread, ValidViewed};
use aurelius_domain::{CodeCommentId, CodeReviewId, CodeThreadId, ProjectId, UserId};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

/// Where a thread is: its project and branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadPlace {
    pub project_id: ProjectId,
    pub branch: String,
}

/// Whether the project exists (an archived one too).
pub fn project_exists(conn: &Connection, project: ProjectId) -> Result<bool, StorageError> {
    Ok(conn.query_row("SELECT EXISTS(SELECT 1 FROM projects WHERE id = ?1)", [project.to_string()], |r| r.get(0))?)
}

/// A new thread with its first comment, pending.
pub fn start_thread(tx: &Transaction<'_>, user: UserId, thread: &ValidThread, now_ms: i64) -> Result<(CodeThreadId, CodeCommentId), StorageError> {
    let id = CodeThreadId::new();
    let snippet = serde_json::to_string(&thread.snippet).map_err(|e| StorageError::Rejected(format!("snippet: {e}")))?;
    tx.execute(
        "INSERT INTO code_review_threads(id, project_id, branch, path, line, side, commit_sha, snippet, created_by, created_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![id.to_string(), thread.project_id.to_string(), thread.branch, thread.path, thread.line, thread.side.as_str(), thread.commit, snippet, user.to_string(), now_ms],
    )?;
    let comment = add_comment(tx, id, user, &thread.body, now_ms)?;
    Ok((id, comment))
}

fn add_comment(tx: &Transaction<'_>, thread: CodeThreadId, user: UserId, body: &str, now_ms: i64) -> Result<CodeCommentId, StorageError> {
    let id = CodeCommentId::new();
    tx.execute(
        "INSERT INTO code_review_comments(id, thread_id, user_id, body, created_at_ms) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id.to_string(), thread.to_string(), user.to_string(), body, now_ms],
    )?;
    Ok(id)
}

/// Where the thread is; none: no such thread.
pub fn thread_place(conn: &Connection, thread: CodeThreadId) -> Result<Option<ThreadPlace>, StorageError> {
    conn.query_row("SELECT project_id, branch FROM code_review_threads WHERE id = ?1", [thread.to_string()], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .optional()?
        .map(|(project, branch)| Ok(ThreadPlace { project_id: ProjectId::parse(&project).map_err(|e| StorageError::Rejected(format!("project id: {e}")))?, branch }))
        .transpose()
}

/// A pending reply in an existing thread (the caller checked it exists).
pub fn reply(tx: &Transaction<'_>, thread: CodeThreadId, user: UserId, body: &str, now_ms: i64) -> Result<CodeCommentId, StorageError> {
    add_comment(tx, thread, user, body, now_ms)
}

/// Resolve or reopen a thread. Whether that changed it.
pub fn set_resolved(tx: &Transaction<'_>, thread: CodeThreadId, resolved: bool) -> Result<bool, StorageError> {
    let n = tx.execute("UPDATE code_review_threads SET resolved = ?2 WHERE id = ?1 AND resolved <> ?2", params![thread.to_string(), resolved])?;
    Ok(n > 0)
}

/// Mark a file viewed (or not) for the board member at a head. Whether that changed it.
pub fn set_viewed(tx: &Transaction<'_>, user: UserId, v: &ValidViewed) -> Result<bool, StorageError> {
    let key = params![user.to_string(), v.project_id.to_string(), v.branch, v.head, v.path];
    let n = if v.viewed {
        tx.execute("INSERT OR IGNORE INTO code_viewed_files(user_id, project_id, branch, head, path) VALUES (?1, ?2, ?3, ?4, ?5)", key)?
    } else {
        tx.execute("DELETE FROM code_viewed_files WHERE user_id = ?1 AND project_id = ?2 AND branch = ?3 AND head = ?4 AND path = ?5", key)?
    };
    Ok(n > 0)
}

/// How many of the board member's comments on this branch are pending.
pub fn pending_count(conn: &Connection, user: UserId, project: ProjectId, branch: &str) -> Result<u32, StorageError> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM code_review_comments c JOIN code_review_threads t ON t.id = c.thread_id
         WHERE c.user_id = ?1 AND c.review_id IS NULL AND t.project_id = ?2 AND t.branch = ?3",
        params![user.to_string(), project.to_string(), branch],
        |r| r.get(0),
    )?)
}

/// Store a finished review and send the board member's pending comments on the branch with
/// it. How many comments went with it.
pub fn submit(tx: &Transaction<'_>, user: UserId, review: &ValidReview, now_ms: i64) -> Result<(CodeReviewId, u32), StorageError> {
    let id = CodeReviewId::new();
    tx.execute(
        "INSERT INTO code_reviews(id, project_id, branch, head, user_id, verdict, body, created_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![id.to_string(), review.project_id.to_string(), review.branch, review.head, user.to_string(), review.verdict.as_str(), review.body, now_ms],
    )?;
    let sent = tx.execute(
        "UPDATE code_review_comments SET review_id = ?1
         WHERE user_id = ?2 AND review_id IS NULL
           AND thread_id IN (SELECT id FROM code_review_threads WHERE project_id = ?3 AND branch = ?4)",
        params![id.to_string(), user.to_string(), review.project_id.to_string(), review.branch],
    )?;
    Ok((id, sent as u32))
}

fn parse<T>(text: &str, what: &str, f: impl FnOnce(&str) -> Option<T>) -> Result<T, rusqlite::Error> {
    f(text).ok_or_else(|| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, format!("bad {what}: {text}").into()))
}

fn uuid<T>(text: &str, what: &str, f: impl FnOnce(&str) -> Result<T, uuid::Error>) -> Result<T, rusqlite::Error> {
    parse(text, what, |t| f(t).ok())
}

/// The branch's review as `reader` sees it: threads with every sent comment and the
/// reader's own pending ones (a thread with nothing visible is left out), the finished
/// reviews, and the files the reader viewed at `head`.
pub fn view(conn: &Connection, reader: UserId, project: ProjectId, branch: &str, head: Option<&str>) -> Result<CodeReviewView, StorageError> {
    let me = reader.to_string();
    let mut threads: Vec<CodeThreadView> = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT id, path, line, side, commit_sha, snippet, resolved, created_at_ms FROM code_review_threads
             WHERE project_id = ?1 AND branch = ?2 ORDER BY created_at_ms, id",
        )?;
        let rows = stmt.query_map(params![project.to_string(), branch], |r| {
            let snippet: String = r.get(5)?;
            Ok(CodeThreadView {
                id: uuid(&r.get::<_, String>(0)?, "thread id", CodeThreadId::parse)?,
                path: r.get(1)?,
                line: r.get(2)?,
                side: parse(&r.get::<_, String>(3)?, "side", CodeLineSide::parse)?,
                commit: r.get(4)?,
                snippet: serde_json::from_str(&snippet).unwrap_or_default(),
                resolved: r.get(6)?,
                created_at_ms: r.get(7)?,
                comments: vec![],
            })
        })?;
        for row in rows {
            threads.push(row?);
        }
    }
    {
        let mut stmt = conn.prepare(
            "SELECT c.id, c.thread_id, u.username, c.user_id, c.body, c.review_id, c.created_at_ms
             FROM code_review_comments c JOIN code_review_threads t ON t.id = c.thread_id JOIN users u ON u.id = c.user_id
             WHERE t.project_id = ?1 AND t.branch = ?2 AND (c.review_id IS NOT NULL OR c.user_id = ?3)
             ORDER BY c.created_at_ms, c.id",
        )?;
        let rows = stmt.query_map(params![project.to_string(), branch, me], |r| {
            let review: Option<String> = r.get(5)?;
            let comment = CodeCommentView {
                id: uuid(&r.get::<_, String>(0)?, "comment id", CodeCommentId::parse)?,
                username: r.get(2)?,
                mine: r.get::<_, String>(3)? == me,
                body: r.get(4)?,
                pending: review.is_none(),
                review_id: review.map(|id| uuid(&id, "review id", CodeReviewId::parse)).transpose()?,
                created_at_ms: r.get(6)?,
            };
            Ok((r.get::<_, String>(1)?, comment))
        })?;
        for row in rows {
            let (thread, comment) = row?;
            if let Some(t) = threads.iter_mut().find(|t| t.id.to_string() == thread) {
                t.comments.push(comment);
            }
        }
    }
    threads.retain(|t| !t.comments.is_empty());

    let mut reviews = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT r.id, u.username, r.user_id, r.verdict, r.body, r.head, r.created_at_ms
             FROM code_reviews r JOIN users u ON u.id = r.user_id
             WHERE r.project_id = ?1 AND r.branch = ?2 ORDER BY r.created_at_ms, r.id",
        )?;
        let rows = stmt.query_map(params![project.to_string(), branch], |r| {
            Ok(CodeReviewEntry {
                id: uuid(&r.get::<_, String>(0)?, "review id", CodeReviewId::parse)?,
                username: r.get(1)?,
                mine: r.get::<_, String>(2)? == me,
                verdict: parse(&r.get::<_, String>(3)?, "verdict", CodeVerdict::parse)?,
                body: r.get(4)?,
                head: r.get(5)?,
                created_at_ms: r.get(6)?,
            })
        })?;
        for row in rows {
            reviews.push(row?);
        }
    }

    let mut viewed = Vec::new();
    if let Some(head) = head {
        let mut stmt = conn.prepare("SELECT path FROM code_viewed_files WHERE user_id = ?1 AND project_id = ?2 AND branch = ?3 AND head = ?4 ORDER BY path")?;
        let rows = stmt.query_map(params![me, project.to_string(), branch, head.to_ascii_lowercase()], |r| r.get::<_, String>(0))?;
        for row in rows {
            viewed.push(row?);
        }
    }
    Ok(CodeReviewView { threads, reviews, viewed })
}
