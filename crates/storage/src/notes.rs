//! Notes of a project: Markdown the user writes, like Apple Notes. Each save
//! raises the revision; an update names the revision it edited.

use crate::StorageError;
use aurelius_domain::views::{NoteSummary, NoteView};
use aurelius_domain::{NoteId, ProjectId};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

/// Longest body a note may have, in bytes.
pub const MAX_BODY_BYTES: usize = 1024 * 1024;
const PREVIEW_CHARS: usize = 120;

/// What `update` did.
#[derive(Debug, Clone, PartialEq)]
pub enum Saved {
    Saved(NoteView),
    /// The note changed since the revision the caller edited; here is the current one.
    Stale(NoteView),
    NotFound,
}

/// Title and preview from the body: the first non-empty line (heading marks
/// stripped) and the rest of the text on one line.
pub fn title_and_preview(body: &str) -> (String, String) {
    let mut lines = body.lines().map(str::trim).filter(|l| !l.is_empty());
    let title = lines.next().map(|l| l.trim_start_matches('#').trim().to_owned()).unwrap_or_default();
    let mut preview = String::new();
    for line in lines {
        if preview.chars().count() >= PREVIEW_CHARS {
            break;
        }
        if !preview.is_empty() {
            preview.push(' ');
        }
        preview.push_str(line);
    }
    let preview = match preview.char_indices().nth(PREVIEW_CHARS) {
        Some((cut, _)) => preview[..cut].trim_end().to_owned(),
        None => preview,
    };
    (title, preview)
}

pub fn create(tx: &Transaction<'_>, project: ProjectId, body: &str, now: i64) -> Result<NoteView, StorageError> {
    let id = NoteId::new();
    tx.execute(
        "INSERT INTO notes (id, project_id, body, pinned, revision, created_at_ms, updated_at_ms) VALUES (?1, ?2, ?3, 0, 1, ?4, ?4)",
        params![id.to_string(), project.to_string(), body, now],
    )?;
    get(tx, id)?.ok_or_else(|| StorageError::Rejected("note not stored".into()))
}

pub fn update(tx: &Transaction<'_>, id: NoteId, body: &str, base_revision: u64, now: i64) -> Result<Saved, StorageError> {
    let changed = tx.execute(
        "UPDATE notes SET body = ?2, revision = revision + 1, updated_at_ms = ?3 WHERE id = ?1 AND revision = ?4",
        params![id.to_string(), body, now, base_revision as i64],
    )?;
    Ok(match (changed, get(tx, id)?) {
        (_, None) => Saved::NotFound,
        (0, Some(current)) => Saved::Stale(current),
        (_, Some(saved)) => Saved::Saved(saved),
    })
}

/// Pinning does not change the edit time or the revision (the body is the same).
pub fn set_pinned(tx: &Transaction<'_>, id: NoteId, pinned: bool) -> Result<Option<NoteView>, StorageError> {
    tx.execute("UPDATE notes SET pinned = ?2 WHERE id = ?1", params![id.to_string(), pinned])?;
    get(tx, id)
}

/// The deleted note, if there was one.
pub fn delete(tx: &Transaction<'_>, id: NoteId) -> Result<Option<NoteView>, StorageError> {
    let found = get(tx, id)?;
    tx.execute("DELETE FROM notes WHERE id = ?1", [id.to_string()])?;
    Ok(found)
}

pub fn get(conn: &Connection, id: NoteId) -> Result<Option<NoteView>, StorageError> {
    Ok(conn
        .query_row(
            "SELECT id, project_id, body, pinned, revision, created_at_ms, updated_at_ms FROM notes WHERE id = ?1",
            [id.to_string()],
            row,
        )
        .optional()?
        .flatten())
}

/// A project's notes, pinned first and then the last edited first. `query`
/// keeps those whose body contains it (case-insensitive for ASCII letters).
pub fn list(conn: &Connection, project: ProjectId, query: Option<&str>) -> Result<Vec<NoteSummary>, StorageError> {
    let query = query.map(str::trim).filter(|q| !q.is_empty());
    let mut stmt = conn.prepare(
        "SELECT id, project_id, body, pinned, revision, created_at_ms, updated_at_ms FROM notes
         WHERE project_id = ?1 AND (?2 IS NULL OR instr(lower(body), lower(?2)) > 0)
         ORDER BY pinned DESC, updated_at_ms DESC, id",
    )?;
    let rows = stmt.query_map(params![project.to_string(), query], row)?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows.into_iter().flatten().map(|n| n.summary).collect())
}

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Option<NoteView>> {
    let (id, project, body): (String, String, String) = (r.get(0)?, r.get(1)?, r.get(2)?);
    let (Ok(id), Ok(project_id)) = (NoteId::parse(&id), ProjectId::parse(&project)) else {
        return Ok(None);
    };
    let (title, preview) = title_and_preview(&body);
    Ok(Some(NoteView {
        summary: NoteSummary {
            id,
            project_id,
            title,
            preview,
            pinned: r.get::<_, i64>(3)? != 0,
            revision: r.get::<_, i64>(4)?.max(0) as u64,
            created_at_ms: r.get(5)?,
            updated_at_ms: r.get(6)?,
        },
        body,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_title_is_the_first_non_empty_line_and_the_preview_the_rest() {
        assert_eq!(title_and_preview("\n\n# Groceries\n- [ ] milk\n\n- [x] eggs\n"), ("Groceries".into(), "- [ ] milk - [x] eggs".into()));
        assert_eq!(title_and_preview("   \n"), (String::new(), String::new()));
        let (_, preview) = title_and_preview(&format!("t\n{}", "é".repeat(300)));
        assert_eq!(preview.chars().count(), 120);
    }
}
