//! Versions of an employee's instructions (`migrations/0011`): every change
//! (create, edit, an approved proposal, a restore) is a new version, so the
//! user can see what changed and put an earlier one back. An employee from before
//! versions (or imported) gets its version 1 the first time one is needed.

use crate::{StorageError, now_ms};
use aurelius_domain::{EmployeeId, ProposalId, views::EmployeeInstructionVersionView};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

/// The version in use now; 1 for an employee without recorded versions.
pub fn current(conn: &Connection, employee: EmployeeId) -> Result<u32, StorageError> {
    let max: Option<i64> = conn.query_row("SELECT MAX(version) FROM employee_instruction_versions WHERE employee_id = ?1", [employee.to_string()], |r| r.get(0))?;
    Ok(max.map_or(1, |v| v.max(1) as u32))
}

/// An employee without versions gets version 1 with what it has now.
fn ensure_initial(tx: &Transaction<'_>, employee: EmployeeId) -> Result<(), StorageError> {
    tx.execute(
        "INSERT INTO employee_instruction_versions(employee_id, version, instructions_blob_id, source, created_at_ms)
         SELECT m.id, 1, m.instructions_blob_id, 'initial', m.updated_at_ms FROM employees m
         WHERE m.id = ?1 AND NOT EXISTS (SELECT 1 FROM employee_instruction_versions v WHERE v.employee_id = m.id)",
        [employee.to_string()],
    )?;
    Ok(())
}

/// How a version came about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Created,
    Edited,
    Proposal(ProposalId),
    Restored(u32),
}

/// Record the employee's instructions as they are now (after the change) as a new
/// version, unless they are the same as the current version's. Returns the
/// current version afterwards and whether a new one was made.
pub fn record(tx: &Transaction<'_>, employee: EmployeeId, source: Source) -> Result<(u32, bool), StorageError> {
    if source == Source::Created {
        tx.execute(
            "INSERT OR IGNORE INTO employee_instruction_versions(employee_id, version, instructions_blob_id, source, created_at_ms)
             SELECT id, 1, instructions_blob_id, 'created', ?2 FROM employees WHERE id = ?1",
            params![employee.to_string(), now_ms()],
        )?;
        return Ok((1, true));
    }
    ensure_initial(tx, employee)?;
    let now: Option<String> = tx.query_row("SELECT instructions_blob_id FROM employees WHERE id = ?1", [employee.to_string()], |r| r.get(0))?;
    let latest = current(tx, employee)?;
    let before: Option<String> = tx
        .query_row("SELECT instructions_blob_id FROM employee_instruction_versions WHERE employee_id = ?1 AND version = ?2", params![employee.to_string(), latest], |r| r.get(0))
        .optional()?
        .flatten();
    // Same text (another blob with the same content counts as the same).
    let same = match (&before, &now) {
        (None, None) => true,
        (Some(a), Some(b)) if a == b => true,
        (Some(a), Some(b)) => {
            let hash = |id: &str| tx.query_row("SELECT sha256 FROM blobs WHERE id = ?1", [id], |r| r.get::<_, String>(0)).optional();
            matches!((hash(a)?, hash(b)?), (Some(x), Some(y)) if x == y)
        }
        _ => false,
    };
    if same && !matches!(source, Source::Restored(_)) {
        return Ok((latest, false));
    }
    let version = latest + 1;
    let (label, proposal, restored_from) = match source {
        Source::Created => ("created", None, None),
        Source::Edited => ("edited", None, None),
        Source::Proposal(p) => ("proposal", Some(p.to_string()), None),
        Source::Restored(v) => ("restored", None, Some(v)),
    };
    tx.execute(
        "INSERT INTO employee_instruction_versions(employee_id, version, instructions_blob_id, source, proposal_id, restored_from, created_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![employee.to_string(), version, now, label, proposal, restored_from, now_ms()],
    )?;
    Ok((version, true))
}

/// The blob of one version (`None` inside: it had no instructions); `None` when there is no such version.
pub fn blob_of(conn: &Connection, employee: EmployeeId, version: u32) -> Result<Option<Option<String>>, StorageError> {
    Ok(conn
        .query_row("SELECT instructions_blob_id FROM employee_instruction_versions WHERE employee_id = ?1 AND version = ?2", params![employee.to_string(), version], |r| r.get::<_, Option<String>>(0))
        .optional()?)
}

/// All versions, newest first. An employee without recorded versions shows its
/// current instructions as version 1.
pub fn list(conn: &Connection, employee: EmployeeId, read_blob: &dyn Fn(&str) -> Option<String>) -> Result<Vec<EmployeeInstructionVersionView>, StorageError> {
    let mut stmt = conn.prepare(
        "SELECT v.version, b.storage_key, v.source, v.proposal_id, v.restored_from, v.created_at_ms
         FROM employee_instruction_versions v LEFT JOIN blobs b ON b.id = v.instructions_blob_id
         WHERE v.employee_id = ?1 ORDER BY v.version DESC",
    )?;
    let rows = stmt.query_map([employee.to_string()], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, String>(2)?, r.get::<_, Option<String>>(3)?, r.get::<_, Option<i64>>(4)?, r.get::<_, i64>(5)?))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (version, key, source, proposal, restored_from, at) = row?;
        out.push(EmployeeInstructionVersionView {
            version: version.max(1) as u32,
            instructions: key.as_deref().and_then(read_blob).unwrap_or_default(),
            source,
            proposal_id: proposal.and_then(|p| ProposalId::parse(&p).ok()),
            restored_from: restored_from.map(|v| v.max(0) as u32),
            created_at_ms: at,
        });
    }
    if out.is_empty() {
        let now: Option<(Option<String>, i64)> = conn
            .query_row(
                "SELECT b.storage_key, m.updated_at_ms FROM employees m LEFT JOIN blobs b ON b.id = m.instructions_blob_id WHERE m.id = ?1",
                [employee.to_string()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some((key, at)) = now {
            out.push(EmployeeInstructionVersionView { version: 1, instructions: key.as_deref().and_then(read_blob).unwrap_or_default(), source: "initial".into(), proposal_id: None, restored_from: None, created_at_ms: at });
        }
    }
    Ok(out)
}
