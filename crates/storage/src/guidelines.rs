//! Guidelines (migration 23, docs/product/guidelines B1–B7): a company library
//! of working rules with append-only versions, linked to employees or projects.
//! Storage returns the contract types of `aurelius_domain::guideline`.
//!
//! * A version is complete (title and text) and never changes; a guideline is
//!   archived, never deleted (B1).
//! * A run's set ([`effective`]) is in block order (B3): employee level first,
//!   then project level, by title within each level; a guideline linked to both
//!   is in it once, at project level.
//! * The limit per run (B4) is checked by the application inside its write:
//!   it writes, then asks [`combinations_over`] and rolls back on an overflow.
//! * A run's snapshot ([`insert_for_run`], B6) and the proposals of
//!   `propose_guideline` (B5) live here too, and the one-time seed step (B7).
//!
//! There is one company per database: "per company" means that company.

use crate::{StorageError, settings};
use aurelius_domain::guideline::{
    EffectiveGuideline, EffectiveGuidelines, GuidelineLevel, GuidelineProposalView, GuidelineSource, GuidelineTarget,
    GuidelineVersionView, GuidelineView, MAX_GUIDELINE_CHARS, MAX_RUN_CHARS, MAX_TITLE_CHARS, RunGuideline,
};
use aurelius_domain::proposal::{ProposalEvidence, ProposalStatus};
use aurelius_domain::vault::UserRef;
use aurelius_domain::{ConversationId, EmployeeId, GuidelineId, GuidelineProposalId, ProjectId, RunId, UserId};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use std::collections::{BTreeSet, HashMap};

pub const NOT_FOUND: &str = "guideline_not_found";
pub const ARCHIVED: &str = "guideline_archived";
pub const NOT_ARCHIVED: &str = "guideline_not_archived";
/// The client's `expected_version` is not the current version any more.
pub const VERSION_CONFLICT: &str = "guideline_version_conflict";
/// `restored_from` names a version the guideline does not have.
pub const VERSION_NOT_FOUND: &str = "guideline_version_not_found";
/// Another guideline that is not archived has this title (trimmed, NOCASE).
pub const TITLE_TAKEN: &str = "guideline_title_taken";
pub const INVALID_TITLE: &str = "guideline_invalid_title";
/// Empty, or more than [`MAX_GUIDELINE_CHARS`] characters.
pub const INVALID_TEXT: &str = "guideline_invalid_text";
pub const PROPOSAL_NOT_FOUND: &str = "guideline_proposal_not_found";
/// A decision is `approved` or `rejected`.
pub const INVALID_DECISION: &str = "guideline_invalid_decision";
pub const UNKNOWN_STARTER: &str = "guideline_unknown_starter";

fn rejected(reason: &str) -> StorageError {
    StorageError::Rejected(reason.into())
}

fn parsed<T, E: std::error::Error + Send + Sync + 'static>(index: usize, r: Result<T, E>) -> rusqlite::Result<T> {
    r.map_err(|e| rusqlite::Error::FromSqlConversionFailure(index, rusqlite::types::Type::Text, Box::new(e)))
}

fn bad(index: usize, what: String) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(index, rusqlite::types::Type::Text, what.into())
}

/// A board member from a user id and its username (LEFT JOIN users): None when
/// there is no user (the seed) or it is gone.
fn user_ref(r: &rusqlite::Row<'_>, id: usize, name: usize) -> rusqlite::Result<Option<UserRef>> {
    match (r.get::<_, Option<String>>(id)?, r.get::<_, Option<String>>(name)?) {
        (Some(user), Some(username)) => Ok(Some(UserRef { id: parsed(id, UserId::parse(&user))?, username })),
        _ => Ok(None),
    }
}

fn valid_title(title: &str) -> bool {
    !title.trim().is_empty() && title.chars().count() <= MAX_TITLE_CHARS
}

fn valid_text(text: &str) -> bool {
    !text.trim().is_empty() && text.chars().count() <= MAX_GUIDELINE_CHARS
}

fn validate(title: &str, text: &str) -> Result<(), StorageError> {
    if !valid_title(title) {
        return Err(rejected(INVALID_TITLE));
    }
    if !valid_text(text) {
        return Err(rejected(INVALID_TEXT));
    }
    Ok(())
}

// ---------------------------------------------------------------- library

/// A new version: the first ([`create`]) or the next ([`add_version`]).
#[derive(Debug, Clone)]
pub struct NewVersion<'a> {
    pub title: &'a str,
    pub text: &'a str,
    pub source: GuidelineSource,
    /// `Proposal`: the approved proposal.
    pub proposal_id: Option<GuidelineProposalId>,
    /// `Restored`: the version it copies.
    pub restored_from: Option<u32>,
    /// None: the seed.
    pub by_user: Option<UserId>,
    pub now_ms: i64,
}

fn insert_version(tx: &Transaction<'_>, id: GuidelineId, version: u32, v: &NewVersion<'_>) -> Result<(), StorageError> {
    tx.execute(
        "INSERT INTO guideline_versions(guideline_id, version, title, text, source, proposal_id, restored_from, by_user, created_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            id.to_string(),
            version,
            v.title,
            v.text,
            v.source.as_str(),
            v.proposal_id.map(|p| p.to_string()),
            v.restored_from,
            v.by_user.map(|u| u.to_string()),
            v.now_ms
        ],
    )?;
    Ok(())
}

fn version_view(tx: &Connection, id: GuidelineId, version: u32) -> Result<GuidelineVersionView, StorageError> {
    get(tx, id, Some(version))?.ok_or_else(|| rejected(NOT_FOUND))
}

/// A new guideline at version 1, without links. Refuses a title that another
/// guideline (not archived) has.
pub fn create(tx: &Transaction<'_>, id: GuidelineId, v: NewVersion<'_>) -> Result<GuidelineVersionView, StorageError> {
    validate(v.title, v.text)?;
    if title_taken(tx, v.title, None)? {
        return Err(rejected(TITLE_TAKEN));
    }
    tx.execute(
        "INSERT INTO guidelines(id, company_id, current_version, created_by_user, updated_by_user, created_at_ms, updated_at_ms)
         SELECT ?1, id, 1, ?2, ?2, ?3, ?3 FROM company",
        params![id.to_string(), v.by_user.map(|u| u.to_string()), v.now_ms],
    )?;
    insert_version(tx, id, 1, &v)?;
    version_view(tx, id, 1)
}

/// The current version and whether the guideline is archived.
fn state(conn: &Connection, id: GuidelineId) -> Result<(u32, bool), StorageError> {
    conn.query_row("SELECT current_version, archived_at_ms IS NOT NULL FROM guidelines WHERE id = ?1", [id.to_string()], |r| {
        Ok((r.get(0)?, r.get(1)?))
    })
    .optional()?
    .ok_or_else(|| rejected(NOT_FOUND))
}

/// The next version of a guideline that is not archived: an edit, an approved
/// proposal or a restored older version. `expected_version` is the version the
/// client saw; another current version is [`VERSION_CONFLICT`].
pub fn add_version(tx: &Transaction<'_>, id: GuidelineId, expected_version: u32, v: NewVersion<'_>) -> Result<GuidelineVersionView, StorageError> {
    let (current, archived) = state(tx, id)?;
    if archived {
        return Err(rejected(ARCHIVED));
    }
    if current != expected_version {
        return Err(rejected(VERSION_CONFLICT));
    }
    validate(v.title, v.text)?;
    if v.restored_from.is_some_and(|from| from == 0 || from > current) {
        return Err(rejected(VERSION_NOT_FOUND));
    }
    if title_taken(tx, v.title, Some(id))? {
        return Err(rejected(TITLE_TAKEN));
    }
    let next = current + 1;
    insert_version(tx, id, next, &v)?;
    tx.execute(
        "UPDATE guidelines SET current_version = ?2, updated_by_user = ?3, updated_at_ms = ?4 WHERE id = ?1",
        params![id.to_string(), next, v.by_user.map(|u| u.to_string()), v.now_ms],
    )?;
    version_view(tx, id, next)
}

/// "Delete": archive a guideline. Its links stay (a restore brings it back as
/// it was), but no run gets it and its title is free again.
pub fn archive(tx: &Transaction<'_>, id: GuidelineId, by_user: UserId, now_ms: i64) -> Result<(), StorageError> {
    if state(tx, id)?.1 {
        return Err(rejected(ARCHIVED));
    }
    tx.execute(
        "UPDATE guidelines SET archived_at_ms = ?2, updated_by_user = ?3, updated_at_ms = ?2 WHERE id = ?1",
        params![id.to_string(), now_ms, by_user.to_string()],
    )?;
    Ok(())
}

/// Bring an archived guideline back with its links. Refused while another
/// guideline has its title.
pub fn restore(tx: &Transaction<'_>, id: GuidelineId, by_user: UserId, now_ms: i64) -> Result<(), StorageError> {
    let (current, archived) = state(tx, id)?;
    if !archived {
        return Err(rejected(NOT_ARCHIVED));
    }
    let title = version_view(tx, id, current)?.title;
    if title_taken(tx, &title, Some(id))? {
        return Err(rejected(TITLE_TAKEN));
    }
    tx.execute(
        "UPDATE guidelines SET archived_at_ms = NULL, updated_by_user = ?3, updated_at_ms = ?2 WHERE id = ?1",
        params![id.to_string(), now_ms, by_user.to_string()],
    )?;
    Ok(())
}

/// Another guideline that is not archived (and not `except`) has this title,
/// trimmed and without case (ASCII, SQLite's NOCASE).
pub fn title_taken(conn: &Connection, title: &str, except: Option<GuidelineId>) -> Result<bool, StorageError> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM guidelines g
                       JOIN guideline_versions v ON v.guideline_id = g.id AND v.version = g.current_version
                       WHERE g.archived_at_ms IS NULL AND g.company_id = (SELECT id FROM company)
                         AND trim(v.title) = trim(?1) COLLATE NOCASE AND (?2 IS NULL OR g.id <> ?2))",
        params![title, except.map(|e| e.to_string())],
        |r| r.get(0),
    )?)
}

/// The links of every guideline (or of one), employees first, then projects,
/// each by name.
fn links_by_guideline(conn: &Connection, only: Option<GuidelineId>) -> Result<HashMap<String, Vec<GuidelineTarget>>, StorageError> {
    let mut stmt = conn.prepare(
        "SELECT l.guideline_id, l.employee_id, l.project_id
         FROM guideline_links l
         LEFT JOIN employees e ON e.id = l.employee_id
         LEFT JOIN projects p ON p.id = l.project_id
         WHERE ?1 IS NULL OR l.guideline_id = ?1
         ORDER BY l.guideline_id, l.employee_id IS NULL, COALESCE(e.name, p.name) COLLATE NOCASE, l.id",
    )?;
    let rows = stmt.query_map([only.map(|g| g.to_string())], |r| {
        let target = match (r.get::<_, Option<String>>(1)?, r.get::<_, Option<String>>(2)?) {
            (Some(e), None) => GuidelineTarget::Employee(parsed(1, EmployeeId::parse(&e))?),
            (None, Some(p)) => GuidelineTarget::Project(parsed(2, ProjectId::parse(&p))?),
            _ => return Err(bad(1, "a guideline link needs one target".into())),
        };
        Ok((r.get::<_, String>(0)?, target))
    })?;
    let mut out: HashMap<String, Vec<GuidelineTarget>> = HashMap::new();
    for row in rows {
        let (guideline, target) = row?;
        out.entry(guideline).or_default().push(target);
    }
    Ok(out)
}

fn views(conn: &Connection, only: Option<GuidelineId>, include_archived: bool) -> Result<Vec<GuidelineView>, StorageError> {
    let mut links = links_by_guideline(conn, only)?;
    let mut stmt = conn.prepare(
        "SELECT g.id, v.title, g.current_version, length(v.text), g.updated_by_user, u.username, g.updated_at_ms,
                g.archived_at_ms IS NOT NULL,
                (SELECT COUNT(*) FROM guideline_proposals p WHERE p.guideline_id = g.id AND p.status = 'pending')
         FROM guidelines g
         JOIN guideline_versions v ON v.guideline_id = g.id AND v.version = g.current_version
         LEFT JOIN users u ON u.id = g.updated_by_user
         WHERE (?1 IS NULL OR g.id = ?1) AND (?2 OR g.archived_at_ms IS NULL)
         ORDER BY g.archived_at_ms IS NOT NULL, v.title COLLATE NOCASE, g.id",
    )?;
    let rows = stmt.query_map(params![only.map(|g| g.to_string()), include_archived], |r| {
        Ok(GuidelineView {
            id: parsed(0, GuidelineId::parse(&r.get::<_, String>(0)?))?,
            title: r.get(1)?,
            version: r.get(2)?,
            chars: r.get(3)?,
            links: Vec::new(),
            updated_by: user_ref(r, 4, 5)?,
            updated_at_ms: r.get(6)?,
            archived: r.get(7)?,
            open_proposals: r.get(8)?,
        })
    })?;
    let mut out: Vec<GuidelineView> = rows.collect::<Result<_, _>>()?;
    for view in &mut out {
        view.links = links.remove(&view.id.to_string()).unwrap_or_default();
    }
    Ok(out)
}

/// The library: guidelines that are not archived by title, then (with
/// `include_archived`) the archived ones by title. With links and the number of
/// pending proposals, without text.
pub fn list(conn: &Connection, include_archived: bool) -> Result<Vec<GuidelineView>, StorageError> {
    views(conn, None, include_archived)
}

/// One guideline as [`list`] shows it, archived or not.
pub fn view(conn: &Connection, id: GuidelineId) -> Result<Option<GuidelineView>, StorageError> {
    Ok(views(conn, Some(id), true)?.pop())
}

const VERSION_COLUMNS: &str = "v.guideline_id, v.version, v.title, v.text, v.source, v.proposal_id, v.restored_from, v.by_user, u.username, v.created_at_ms";

fn version_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<GuidelineVersionView> {
    let source: String = r.get(4)?;
    Ok(GuidelineVersionView {
        guideline_id: parsed(0, GuidelineId::parse(&r.get::<_, String>(0)?))?,
        version: r.get(1)?,
        title: r.get(2)?,
        text: r.get(3)?,
        source: GuidelineSource::parse(&source).ok_or_else(|| bad(4, format!("unknown guideline source {source:?}")))?,
        proposal_id: r.get::<_, Option<String>>(5)?.map(|p| parsed(5, GuidelineProposalId::parse(&p))).transpose()?,
        restored_from: r.get(6)?,
        by: user_ref(r, 7, 8)?,
        created_at_ms: r.get(9)?,
    })
}

/// A version with its text; `None` for the version: the current one.
pub fn get(conn: &Connection, id: GuidelineId, version: Option<u32>) -> Result<Option<GuidelineVersionView>, StorageError> {
    Ok(conn
        .query_row(
            &format!(
                "SELECT {VERSION_COLUMNS} FROM guideline_versions v
                 JOIN guidelines g ON g.id = v.guideline_id
                 LEFT JOIN users u ON u.id = v.by_user
                 WHERE v.guideline_id = ?1 AND v.version = COALESCE(?2, g.current_version)"
            ),
            params![id.to_string(), version],
            version_row,
        )
        .optional()?)
}

/// Every version of a guideline with its text, newest first.
pub fn versions(conn: &Connection, id: GuidelineId) -> Result<Vec<GuidelineVersionView>, StorageError> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {VERSION_COLUMNS} FROM guideline_versions v LEFT JOIN users u ON u.id = v.by_user
         WHERE v.guideline_id = ?1 ORDER BY v.version DESC"
    ))?;
    let rows = stmt.query_map([id.to_string()], version_row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

fn target_columns(target: GuidelineTarget) -> (Option<String>, Option<String>) {
    match target {
        GuidelineTarget::Employee(e) => (Some(e.to_string()), None),
        GuidelineTarget::Project(p) => (None, Some(p.to_string())),
    }
}

fn insert_link(tx: &Transaction<'_>, id: GuidelineId, target: GuidelineTarget, by_user: Option<UserId>, now_ms: i64) -> Result<bool, StorageError> {
    let (employee, project) = target_columns(target);
    let added = tx.execute(
        "INSERT INTO guideline_links(guideline_id, employee_id, project_id, created_by_user, created_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5) ON CONFLICT DO NOTHING",
        params![id.to_string(), employee, project, by_user.map(|u| u.to_string()), now_ms],
    )?;
    Ok(added == 1)
}

/// Link a guideline that is not archived to an employee or a project.
/// Idempotent: false when the link was already there.
pub fn link(tx: &Transaction<'_>, id: GuidelineId, target: GuidelineTarget, by_user: UserId, now_ms: i64) -> Result<bool, StorageError> {
    if state(tx, id)?.1 {
        return Err(rejected(ARCHIVED));
    }
    insert_link(tx, id, target, Some(by_user), now_ms)
}

/// Remove a link. Idempotent: false when there was none. Also for an archived
/// guideline (it then stays unlinked when restored).
pub fn unlink(tx: &Transaction<'_>, id: GuidelineId, target: GuidelineTarget) -> Result<bool, StorageError> {
    let (employee, project) = target_columns(target);
    let removed = tx.execute(
        "DELETE FROM guideline_links WHERE guideline_id = ?1 AND employee_id IS ?2 AND project_id IS ?3",
        params![id.to_string(), employee, project],
    )?;
    Ok(removed > 0)
}

// ---------------------------------------------------------------- a run's set and the limit

/// The set a run of this employee in this project gets (B3), in block order:
/// employee level first, then project level, by title (NOCASE) within a level.
/// A guideline linked to both is in it once, at project level. Archived
/// guidelines never count. No employee: only the project's (the project
/// assistant); no project: only the employee's (a chat without project).
/// `dropped` stays empty: the safety net at run start is the application's.
pub fn effective(conn: &Connection, employee: Option<EmployeeId>, project: Option<ProjectId>) -> Result<EffectiveGuidelines, StorageError> {
    let mut stmt = conn.prepare(
        "SELECT g.id, g.current_version, v.title, v.text, length(v.text),
                EXISTS(SELECT 1 FROM guideline_links l WHERE l.guideline_id = g.id AND l.project_id = ?2) AS on_project
         FROM guidelines g
         JOIN guideline_versions v ON v.guideline_id = g.id AND v.version = g.current_version
         WHERE g.archived_at_ms IS NULL
           AND EXISTS(SELECT 1 FROM guideline_links l WHERE l.guideline_id = g.id AND (l.employee_id = ?1 OR l.project_id = ?2))
         ORDER BY on_project, v.title COLLATE NOCASE, g.id",
    )?;
    let rows = stmt.query_map(params![employee.map(|e| e.to_string()), project.map(|p| p.to_string())], |r| {
        Ok(EffectiveGuideline {
            id: parsed(0, GuidelineId::parse(&r.get::<_, String>(0)?))?,
            version: r.get(1)?,
            title: r.get(2)?,
            text: r.get(3)?,
            chars: r.get(4)?,
            level: if r.get(5)? { GuidelineLevel::Project } else { GuidelineLevel::Employee },
        })
    })?;
    let guidelines: Vec<EffectiveGuideline> = rows.collect::<Result<_, _>>()?;
    Ok(EffectiveGuidelines {
        employee_id: employee,
        project_id: project,
        total_chars: guidelines.iter().map(|g| g.chars).sum(),
        guidelines,
        limit_chars: MAX_RUN_CHARS as u32,
        dropped: Vec::new(),
    })
}

/// A combination whose guidelines together are over the limit (B4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuidelineOverflow {
    /// None: the project's set alone (the project assistant).
    pub employee_id: Option<EmployeeId>,
    /// None: the employee without a project (its own chat).
    pub project_id: Option<ProjectId>,
    pub total_chars: u32,
}

/// Every combination a run can have whose guidelines (not archived) are more
/// than `limit` characters together: each employee that is not archived with
/// each project that is not archived and with "no project", and each such
/// project alone. A guideline linked to both counts once. Paused projects
/// count too: they come back without a write that checks. Employees by name,
/// then projects by name ("no project" first).
///
/// Call it inside the write that changes a sum, after writing; an overflow
/// means: roll back and answer `guideline_too_large` with the first one.
pub fn combinations_over(conn: &Connection, limit: usize) -> Result<Vec<GuidelineOverflow>, StorageError> {
    let mut chars: HashMap<String, u64> = HashMap::new();
    {
        let mut stmt = conn.prepare(
            "SELECT g.id, length(v.text) FROM guidelines g
             JOIN guideline_versions v ON v.guideline_id = g.id AND v.version = g.current_version
             WHERE g.archived_at_ms IS NULL",
        )?;
        for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))? {
            let (id, n) = row?;
            chars.insert(id, n.max(0) as u64);
        }
    }
    // (id, name) of each employee or project that is not archived, with the
    // guidelines (not archived) linked to it.
    let sets = |sql: &str| -> Result<Vec<(String, BTreeSet<String>)>, StorageError> {
        let mut stmt = conn.prepare(sql)?;
        let mut out: Vec<(String, BTreeSet<String>)> = Vec::new();
        for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)))? {
            let (owner, guideline) = row?;
            if out.last().is_none_or(|(o, _)| *o != owner) {
                out.push((owner, BTreeSet::new()));
            }
            if let Some(g) = guideline.filter(|g| chars.contains_key(g))
                && let Some((_, set)) = out.last_mut()
            {
                set.insert(g);
            }
        }
        Ok(out)
    };
    let employees = sets(
        "SELECT e.id, l.guideline_id FROM employees e LEFT JOIN guideline_links l ON l.employee_id = e.id
         WHERE e.status <> 'archived' ORDER BY e.name COLLATE NOCASE, e.id",
    )?;
    let projects = sets(
        "SELECT p.id, l.guideline_id FROM projects p LEFT JOIN guideline_links l ON l.project_id = p.id
         WHERE p.status <> 'archived' ORDER BY p.name COLLATE NOCASE, p.id",
    )?;
    let sum = |a: &BTreeSet<String>, b: Option<&BTreeSet<String>>| -> u64 {
        a.union(b.unwrap_or(&BTreeSet::new())).map(|g| chars[g]).sum()
    };
    let employee_id = |id: &str| EmployeeId::parse(id).map_err(|e| StorageError::Rejected(format!("bad employee id {id}: {e}")));
    let project_id = |id: &str| ProjectId::parse(id).map_err(|e| StorageError::Rejected(format!("bad project id {id}: {e}")));
    let mut over = Vec::new();
    let mut push = |employee: Option<&str>, project: Option<&str>, total: u64| -> Result<(), StorageError> {
        if total > limit as u64 {
            over.push(GuidelineOverflow {
                employee_id: employee.map(employee_id).transpose()?,
                project_id: project.map(project_id).transpose()?,
                total_chars: total.min(u32::MAX as u64) as u32,
            });
        }
        Ok(())
    };
    for (project, set) in &projects {
        push(None, Some(project), sum(set, None))?;
    }
    for (employee, own) in &employees {
        push(Some(employee), None, sum(own, None))?;
        for (project, set) in &projects {
            push(Some(employee), Some(project), sum(own, Some(set)))?;
        }
    }
    Ok(over)
}

/// The employees whose next run a change to this guideline reaches ("holds
/// from the next run of N employees"): the employees linked to it, plus the
/// employees with an open task (not succeeded, failed or cancelled) in a
/// project linked to it. Not archived, by name. A chat, channel or discussion
/// run in a linked project by someone without an open task there is not
/// counted: who that is cannot be known before the run.
pub fn affected_employees(conn: &Connection, id: GuidelineId) -> Result<Vec<EmployeeId>, StorageError> {
    let mut stmt = conn.prepare(
        "SELECT e.id FROM employees e
         WHERE e.status <> 'archived'
           AND (EXISTS(SELECT 1 FROM guideline_links l WHERE l.guideline_id = ?1 AND l.employee_id = e.id)
                OR EXISTS(SELECT 1 FROM guideline_links l
                          JOIN projects p ON p.id = l.project_id AND p.status <> 'archived'
                          JOIN tasks t ON t.project_id = p.id AND t.employee_id = e.id
                          WHERE l.guideline_id = ?1 AND t.status NOT IN ('succeeded','failed','cancelled')))
         ORDER BY e.name COLLATE NOCASE, e.id",
    )?;
    let rows = stmt.query_map([id.to_string()], |r| parsed(0, EmployeeId::parse(&r.get::<_, String>(0)?)))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

// ---------------------------------------------------------------- proposals

/// What `propose_guideline` stores (validated by the application).
#[derive(Debug, Clone)]
pub struct NewGuidelineProposal<'a> {
    pub id: GuidelineProposalId,
    /// None: a new guideline (then no base version).
    pub guideline_id: Option<GuidelineId>,
    pub base_version: Option<u32>,
    pub proposed_by: EmployeeId,
    pub title: &'a str,
    pub text: &'a str,
    pub reason: &'a str,
    pub evidence: &'a [ProposalEvidence],
    /// For a new guideline: the links the proposer suggests.
    pub suggested_links: &'a [GuidelineTarget],
    pub origin_conversation_id: Option<ConversationId>,
    pub run_id: Option<RunId>,
    pub now_ms: i64,
}

/// Store a pending proposal.
pub fn insert_proposal(tx: &Transaction<'_>, p: NewGuidelineProposal<'_>) -> Result<(), StorageError> {
    tx.execute(
        "INSERT INTO guideline_proposals(id, guideline_id, base_version, proposed_by, title, text, reason, evidence_json,
                                         suggested_links_json, status, origin_conversation_id, run_id, created_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'pending', ?10, ?11, ?12)",
        params![
            p.id.to_string(),
            p.guideline_id.map(|g| g.to_string()),
            p.base_version,
            p.proposed_by.to_string(),
            p.title,
            p.text,
            p.reason,
            serde_json::to_string(p.evidence)?,
            serde_json::to_string(p.suggested_links)?,
            p.origin_conversation_id.map(|c| c.to_string()),
            p.run_id.map(|r| r.to_string()),
            p.now_ms
        ],
    )?;
    Ok(())
}

const PROPOSAL_COLUMNS: &str = "p.id, p.guideline_id, p.base_version, p.proposed_by, p.title, p.text, p.reason, p.evidence_json,
     p.suggested_links_json, p.status, p.decision_note, p.decided_by_user, u.username, p.origin_conversation_id, p.run_id,
     p.created_at_ms, p.decided_at_ms";

fn from_json<T: serde::de::DeserializeOwned>(r: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<T> {
    serde_json::from_str(&r.get::<_, String>(index)?).map_err(|e| bad(index, e.to_string()))
}

fn proposal_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<GuidelineProposalView> {
    let status: String = r.get(9)?;
    Ok(GuidelineProposalView {
        id: parsed(0, GuidelineProposalId::parse(&r.get::<_, String>(0)?))?,
        guideline_id: r.get::<_, Option<String>>(1)?.map(|g| parsed(1, GuidelineId::parse(&g))).transpose()?,
        base_version: r.get(2)?,
        proposed_by: parsed(3, EmployeeId::parse(&r.get::<_, String>(3)?))?,
        title: r.get(4)?,
        text: r.get(5)?,
        reason: r.get(6)?,
        evidence: from_json(r, 7)?,
        suggested_links: from_json(r, 8)?,
        status: ProposalStatus::parse(&status).ok_or_else(|| bad(9, format!("unknown proposal status {status:?}")))?,
        decision_note: r.get(10)?,
        decided_by: user_ref(r, 11, 12)?,
        origin_conversation_id: r.get::<_, Option<String>>(13)?.map(|c| parsed(13, ConversationId::parse(&c))).transpose()?,
        run_id: r.get::<_, Option<String>>(14)?.map(|c| parsed(14, RunId::parse(&c))).transpose()?,
        created_at_ms: r.get(15)?,
        decided_at_ms: r.get(16)?,
    })
}

/// Proposals with this status (all without one): pending ones oldest first,
/// then decided ones newest first.
pub fn proposals(conn: &Connection, status: Option<ProposalStatus>) -> Result<Vec<GuidelineProposalView>, StorageError> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {PROPOSAL_COLUMNS} FROM guideline_proposals p LEFT JOIN users u ON u.id = p.decided_by_user
         WHERE ?1 IS NULL OR p.status = ?1
         ORDER BY p.status <> 'pending',
                  CASE WHEN p.status = 'pending' THEN p.created_at_ms ELSE -COALESCE(p.decided_at_ms, 0) END, p.id"
    ))?;
    let rows = stmt.query_map([status.map(ProposalStatus::as_str)], proposal_row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn proposal(conn: &Connection, id: GuidelineProposalId) -> Result<Option<GuidelineProposalView>, StorageError> {
    Ok(conn
        .query_row(
            &format!("SELECT {PROPOSAL_COLUMNS} FROM guideline_proposals p LEFT JOIN users u ON u.id = p.decided_by_user WHERE p.id = ?1"),
            [id.to_string()],
            proposal_row,
        )
        .optional()?)
}

/// A board member's decision on a pending proposal: `Approved` or `Rejected`.
/// False when it was no longer pending (nothing written). Writing the approved
/// version is the caller's ([`add_version`] or [`create`] with `Proposal`).
pub fn decide_proposal(
    tx: &Transaction<'_>,
    id: GuidelineProposalId,
    status: ProposalStatus,
    note: Option<&str>,
    by_user: UserId,
    now_ms: i64,
) -> Result<bool, StorageError> {
    if !matches!(status, ProposalStatus::Approved | ProposalStatus::Rejected) {
        return Err(rejected(INVALID_DECISION));
    }
    let changed = tx.execute(
        "UPDATE guideline_proposals SET status = ?2, decision_note = ?3, decided_by_user = ?4, decided_at_ms = ?5
         WHERE id = ?1 AND status = 'pending'",
        params![id.to_string(), status.as_str(), note, by_user.to_string(), now_ms],
    )?;
    if changed == 0 && proposal(tx, id)?.is_none() {
        return Err(rejected(PROPOSAL_NOT_FOUND));
    }
    Ok(changed == 1)
}

/// Withdraw the pending proposals of an employee that is being archived.
pub fn withdraw_for_employee(tx: &Transaction<'_>, employee: EmployeeId, now_ms: i64) -> Result<Vec<GuidelineProposalId>, StorageError> {
    let mut stmt = tx.prepare(
        "UPDATE guideline_proposals SET status = 'withdrawn', decided_at_ms = ?2
         WHERE proposed_by = ?1 AND status = 'pending' RETURNING id",
    )?;
    let rows = stmt.query_map(params![employee.to_string(), now_ms], |r| parsed(0, GuidelineProposalId::parse(&r.get::<_, String>(0)?)))?;
    let mut ids: Vec<GuidelineProposalId> = rows.collect::<Result<_, _>>()?;
    ids.sort_by_key(|id| id.to_string());
    Ok(ids)
}

/// Pending proposals for this guideline (at most `MAX_OPEN_PER_GUIDELINE`).
pub fn count_open_for_guideline(conn: &Connection, id: GuidelineId) -> Result<usize, StorageError> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM guideline_proposals WHERE guideline_id = ?1 AND status = 'pending'",
        [id.to_string()],
        |r| r.get::<_, i64>(0),
    )? as usize)
}

/// Pending proposals of this employee for a new guideline (at most `MAX_OPEN_NEW_PER_EMPLOYEE`).
pub fn count_open_new_for_employee(conn: &Connection, employee: EmployeeId) -> Result<usize, StorageError> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM guideline_proposals WHERE proposed_by = ?1 AND guideline_id IS NULL AND status = 'pending'",
        [employee.to_string()],
        |r| r.get::<_, i64>(0),
    )? as usize)
}

// ---------------------------------------------------------------- a run's snapshot (B6)

/// Store what a run has in its system text (replacing an earlier snapshot of
/// the same run), in the transaction that claims its session.
pub fn insert_for_run(tx: &Transaction<'_>, run_id: RunId, guidelines: &[EffectiveGuideline]) -> Result<(), StorageError> {
    tx.execute("DELETE FROM run_guidelines WHERE run_id = ?1", [run_id.to_string()])?;
    let mut stmt = tx.prepare(
        "INSERT INTO run_guidelines(run_id, guideline_id, version, title, level, chars) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )?;
    for g in guidelines {
        stmt.execute(params![run_id.to_string(), g.id.to_string(), g.version, g.title, g.level.as_str(), g.chars])?;
    }
    Ok(())
}

/// A run's snapshot in block order (employee level first, then by title).
pub fn for_run(conn: &Connection, run_id: RunId) -> Result<Vec<RunGuideline>, StorageError> {
    let mut stmt = conn.prepare(
        "SELECT run_id, guideline_id, version, title, level, chars FROM run_guidelines WHERE run_id = ?1
         ORDER BY level = 'project', title COLLATE NOCASE, guideline_id",
    )?;
    let rows = stmt.query_map([run_id.to_string()], |r| {
        let level: String = r.get(4)?;
        Ok(RunGuideline {
            run_id: parsed(0, RunId::parse(&r.get::<_, String>(0)?))?,
            guideline_id: parsed(1, GuidelineId::parse(&r.get::<_, String>(1)?))?,
            version: r.get(2)?,
            title: r.get(3)?,
            level: match level.as_str() {
                "employee" => GuidelineLevel::Employee,
                "project" => GuidelineLevel::Project,
                _ => return Err(bad(4, format!("unknown guideline level {level:?}"))),
            },
            chars: r.get(5)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

// ---------------------------------------------------------------- the starter set (B7)

/// One guideline of the starter set (docs/product/guidelines/seed.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StarterGuideline {
    /// `g1`–`g6`, `p1`, `p2`, as in seed.md.
    pub key: &'static str,
    pub title: &'static str,
    pub text: &'static str,
    /// Employee: linked to the employees below by the seed. Project: linked to
    /// the project [`SEED_PROJECT`].
    pub level: GuidelineLevel,
    /// The exact names the seed links (employee level only).
    pub employees: &'static [&'static str],
}

const STARTER: [StarterGuideline; 8] = [
    StarterGuideline {
        key: "g1",
        title: "Rust style",
        text: include_str!("../seed/guidelines/g1-rust-style.md"),
        level: GuidelineLevel::Employee,
        employees: &["Forge", "Bridge", "Nexus", "Schema", "Sentinel", "Trace", "Titan"],
    },
    StarterGuideline {
        key: "g2",
        title: "Flutter and Dart (mobile app)",
        text: include_str!("../seed/guidelines/g2-flutter-dart.md"),
        level: GuidelineLevel::Employee,
        employees: &["Bridge", "Flux"],
    },
    StarterGuideline {
        key: "g3",
        title: "Desktop app (gpui-kit)",
        text: include_str!("../seed/guidelines/g3-desktop-gpui-kit.md"),
        level: GuidelineLevel::Employee,
        employees: &["Flux", "Bridge"],
    },
    StarterGuideline {
        key: "g4",
        title: "Worktrees, builds and cleanup",
        text: include_str!("../seed/guidelines/g4-worktrees-builds-cleanup.md"),
        level: GuidelineLevel::Employee,
        employees: &["Forge", "Flux", "Bridge", "Nexus", "Schema", "Sentinel", "Pulse", "Orbit", "Trace", "Titan"],
    },
    StarterGuideline {
        key: "g5",
        title: "Tests and evidence",
        text: include_str!("../seed/guidelines/g5-tests-evidence.md"),
        level: GuidelineLevel::Employee,
        employees: &["Forge", "Flux", "Bridge", "Nexus", "Schema", "Sentinel", "Pulse"],
    },
    StarterGuideline {
        key: "g6",
        title: "Code review",
        text: include_str!("../seed/guidelines/g6-code-review.md"),
        level: GuidelineLevel::Employee,
        employees: &["Trace", "Sentinel", "Pulse", "Titan"],
    },
    StarterGuideline {
        key: "p1",
        title: "Aurelius: layers and boundaries",
        text: include_str!("../seed/guidelines/p1-aurelius-layers.md"),
        level: GuidelineLevel::Project,
        employees: &[],
    },
    StarterGuideline {
        key: "p2",
        title: "Aurelius: protocol and migrations",
        text: include_str!("../seed/guidelines/p2-aurelius-protocol-migrations.md"),
        level: GuidelineLevel::Project,
        employees: &[],
    },
];

/// The project the seed links P1 and P2 to, and without which it adds nothing.
pub const SEED_PROJECT: &str = "Aurelius";
/// The settings key that marks the seed step as done (whatever it did).
pub const SEEDED: &str = "guidelines_seeded";

/// The starter set, in seed.md order: the templates of *Add starter set…*.
pub fn starter_set() -> &'static [StarterGuideline] {
    &STARTER
}

/// What the seed step did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SeedReport {
    /// It added the starter set (false: marker already set, library not empty,
    /// or no project [`SEED_PROJECT`]).
    pub ran: bool,
    /// Guidelines added.
    pub inserted: usize,
    /// Links made (to employees and to the project).
    pub linked: usize,
    /// Employee names of the starter set that had no employee (not archived)
    /// with exactly that name: those links were skipped. In seed.md order.
    pub missing: Vec<String>,
}

/// The one-time seed step (B7, GL-18) at the start after migration 23, in the
/// caller's one transaction: `store.write(|tx| seed_guidelines_once(tx, now))`.
///
/// It runs once: the first call sets [`SEEDED`] in the settings, whatever it
/// finds. It adds the starter set only when the library is empty (archived
/// guidelines count) and a project that is not archived is named exactly
/// [`SEED_PROJECT`]: G1–G6 linked to employees (not archived) by exact name, P1
/// and P2 to that project. Source `seed`, no board member. Without that project
/// it adds nothing; a board member can use *Add starter set…* instead.
pub fn seed_guidelines_once(tx: &Transaction<'_>, now_ms: i64) -> Result<SeedReport, StorageError> {
    let mut report = SeedReport::default();
    if settings::get(tx, SEEDED)?.is_some() {
        return Ok(report);
    }
    let empty: bool = tx.query_row("SELECT NOT EXISTS(SELECT 1 FROM guidelines)", [], |r| r.get(0))?;
    let project: Option<String> = tx
        .query_row(
            "SELECT id FROM projects WHERE name = ?1 AND status <> 'archived' ORDER BY created_at_ms, id LIMIT 1",
            [SEED_PROJECT],
            |r| r.get(0),
        )
        .optional()?;
    if let (true, Some(project)) = (empty, project) {
        let project = ProjectId::parse(&project).map_err(|e| StorageError::Rejected(format!("bad project id {project}: {e}")))?;
        report.ran = true;
        for starter in &STARTER {
            let id = GuidelineId::new();
            create(
                tx,
                id,
                NewVersion {
                    title: starter.title,
                    text: starter.text,
                    source: GuidelineSource::Seed,
                    proposal_id: None,
                    restored_from: None,
                    by_user: None,
                    now_ms,
                },
            )?;
            report.inserted += 1;
            if starter.level == GuidelineLevel::Project && insert_link(tx, id, GuidelineTarget::Project(project), None, now_ms)? {
                report.linked += 1;
            }
            for name in starter.employees {
                let employee: Option<String> = tx
                    .query_row(
                        "SELECT id FROM employees WHERE name = ?1 AND status <> 'archived' ORDER BY created_at_ms, id LIMIT 1",
                        [name],
                        |r| r.get(0),
                    )
                    .optional()?;
                match employee.map(|e| EmployeeId::parse(&e)) {
                    Some(Ok(employee)) => {
                        if insert_link(tx, id, GuidelineTarget::Employee(employee), None, now_ms)? {
                            report.linked += 1;
                        }
                    }
                    _ => {
                        if !report.missing.iter().any(|m| m == name) {
                            report.missing.push(name.to_string());
                        }
                    }
                }
            }
        }
    }
    let marker = serde_json::json!({ "at_ms": now_ms, "inserted": report.inserted, "linked": report.linked, "missing": report.missing });
    settings::put(tx, SEEDED, &marker.to_string(), now_ms)?;
    Ok(report)
}

/// What *Add starter set…* did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StarterAdded {
    /// The keys added, with their new guideline.
    pub added: Vec<(&'static str, GuidelineId)>,
    /// The keys skipped because a guideline (not archived) has that title.
    pub skipped: Vec<&'static str>,
}

/// *Add starter set…*: the chosen templates as ordinary guidelines without
/// links (source `seed`, by this board member). A key whose title is taken is
/// skipped; an unknown key refuses the whole call. Adding without links never
/// changes a run's set, so it needs no limit check.
pub fn add_starter_set(tx: &Transaction<'_>, keys: &[&str], by_user: UserId, now_ms: i64) -> Result<StarterAdded, StorageError> {
    let chosen = keys
        .iter()
        .map(|key| STARTER.iter().find(|s| s.key == *key).ok_or_else(|| rejected(UNKNOWN_STARTER)))
        .collect::<Result<Vec<_>, _>>()?;
    let mut out = StarterAdded::default();
    for starter in chosen {
        if out.added.iter().any(|(k, _)| *k == starter.key) || out.skipped.contains(&starter.key) {
            continue;
        }
        if title_taken(tx, starter.title, None)? {
            out.skipped.push(starter.key);
            continue;
        }
        let id = GuidelineId::new();
        create(
            tx,
            id,
            NewVersion {
                title: starter.title,
                text: starter.text,
                source: GuidelineSource::Seed,
                proposal_id: None,
                restored_from: None,
                by_user: Some(by_user),
                now_ms,
            },
        )?;
        out.added.push((starter.key, id));
    }
    Ok(out)
}
