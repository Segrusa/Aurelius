//! `list_guidelines` and `read_guideline` (GL-15): an employee reads guidelines
//! that are not in its block, to propose a change or to know a colleague's or
//! another project's rules. Reading only: through a read connection, so the
//! run's block, its snapshot (`run_guidelines`) and its session's version set
//! (B6) stay as they are (AC3).
//!
//! - AC1: the list has id, title, current version and links (by name) of the
//!   guidelines in use, never a text.
//! - AC2: one guideline by `g:` id (a `@<version>` is ignored: always the
//!   current one), id or exact title (case-insensitive); archived, unknown and
//!   ambiguous names get fixed refusals without names or texts (as E13).
//! - What is read is quoted as the block quotes it (E3-E5, `level="read"`) and
//!   redacted with the current redactor (E6); every answer says it is no rule
//!   for this run and that guidelines never give rights.
//! - E10 as `propose_guideline`: a live run of an active employee in a
//!   conversation, never a trial, the project assistant or the dispatcher.

use crate::guideline_proposal::employee_run;
use crate::{CommandService, storage_error};
use aurelius_domain::guideline::{self, GuidelineTarget, GuidelineView};
use aurelius_domain::{AppError, GuidelineId, RunId};
use aurelius_storage::{StorageError, guidelines};
use rusqlite::Connection;

/// A guideline that is not in the library, or not in use.
pub const UNKNOWN: &str = "There is no guideline in use by that name; list_guidelines shows them.";
/// An archived guideline (AC2).
pub const ARCHIVED: &str = "That guideline is archived; it is no longer in use.";
/// More than one guideline in use has that title.
pub const AMBIGUOUS: &str = "More than one guideline has that title; name it by its g: id (list_guidelines shows them).";
impl CommandService {
    /// `list_guidelines` from a run. Returns (ok, text for the model).
    pub fn list_guidelines(&self, run: RunId) -> Result<(bool, String), AppError> {
        self.store.read(|conn| list(conn, run)).map_err(storage_error)
    }

    /// `read_guideline` from a run (`wanted`: a `g:` id, id or title). Returns (ok, text for the model).
    pub fn read_guideline(&self, run: RunId, wanted: &str) -> Result<(bool, String), AppError> {
        self.store.read(|conn| read(conn, run, wanted)).map_err(storage_error)
    }
}

fn list(conn: &Connection, run: RunId) -> Result<(bool, String), StorageError> {
    if let Err(why) = employee_run(conn, run, "read guidelines")? {
        return Ok((false, why));
    }
    let library = guidelines::list(conn, false)?;
    if library.is_empty() {
        return Ok((true, format!("{}\n\nThe company has no guidelines in use.", guideline::READ_PREAMBLE)));
    }
    let block = guidelines::for_run(conn, run)?;
    let redactor = aurelius_runtime::redact::current();
    let name = |text: &str| guideline::render_name(&redactor.redact(text));
    let mut out = format!("{}\nRead one with read_guideline (its g: id or title).\n", guideline::READ_PREAMBLE);
    for g in &library {
        let (mut employees, mut projects) = (Vec::new(), Vec::new());
        for link in &g.links {
            match *link {
                GuidelineTarget::Employee(e) => employees.extend(employee_name(conn, e)?.map(|n| name(&n))),
                GuidelineTarget::Project(p) => projects.extend(crate::queries_project_name(conn, p).map(|n| name(&n))),
            }
        }
        let mut links = Vec::new();
        if !employees.is_empty() {
            links.push(format!("employees: {}", employees.join(", ")));
        }
        if !projects.is_empty() {
            links.push(format!("projects: {}", projects.join(", ")));
        }
        let links = if links.is_empty() { "not linked".to_owned() } else { links.join("; ") };
        let mine = block.iter().find(|b| b.guideline_id == g.id).map(|b| format!(" (in your block at version {})", b.version)).unwrap_or_default();
        out.push_str(&format!("\n- g:{} version {} “{}”: {links}{mine}", g.id.0, g.version, name(&g.title)));
    }
    Ok((true, out))
}

fn read(conn: &Connection, run: RunId, wanted: &str) -> Result<(bool, String), StorageError> {
    if let Err(why) = employee_run(conn, run, "read guidelines")? {
        return Ok((false, why));
    }
    let id = match find(conn, wanted.trim())? {
        Ok(id) => id,
        Err(why) => return Ok((false, why.into())),
    };
    let Some(current) = guidelines::get(conn, id, None)? else { return Ok((false, UNKNOWN.into())) };
    let redactor = aurelius_runtime::redact::current();
    Ok((true, guideline::render_read(id, current.version, &redactor.redact(&current.title), &redactor.redact(&current.text))))
}

/// The guideline `wanted` names in the library: `g:<id>` (also with `@<version>`),
/// the id, or the exact title (case-insensitive) of one in use.
fn find(conn: &Connection, wanted: &str) -> Result<Result<GuidelineId, &'static str>, StorageError> {
    if wanted.is_empty() {
        return Ok(Err(UNKNOWN));
    }
    let bare = wanted.strip_prefix("g:").unwrap_or(wanted);
    let bare = bare.split_once('@').map_or(bare, |(id, _)| id).trim();
    if let Ok(id) = GuidelineId::parse(bare) {
        return Ok(match guidelines::view(conn, id)? {
            None => Err(UNKNOWN),
            Some(g) if g.archived => Err(ARCHIVED),
            Some(g) => Ok(g.id),
        });
    }
    let titled: Vec<GuidelineView> = guidelines::list(conn, true)?.into_iter().filter(|g| g.title.trim().eq_ignore_ascii_case(wanted)).collect();
    let in_use: Vec<&GuidelineView> = titled.iter().filter(|g| !g.archived).collect();
    Ok(match in_use.as_slice() {
        [one] => Ok(one.id),
        [_, _, ..] => Err(AMBIGUOUS),
        [] if titled.is_empty() => Err(UNKNOWN),
        [] => Err(ARCHIVED),
    })
}

fn employee_name(conn: &Connection, id: aurelius_domain::EmployeeId) -> Result<Option<String>, StorageError> {
    use rusqlite::OptionalExtension;
    Ok(conn.query_row("SELECT name FROM employees WHERE id = ?1", [id.to_string()], |r| r.get(0)).optional()?)
}

#[cfg(test)]
#[path = "guideline_read_tests.rs"]
mod tests;
