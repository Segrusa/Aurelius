//! Proposals: an employee suggests a new employee or new instructions for an
//! existing one, the user decides. The proposed instructions live in a blob,
//! like an employee's own.

use crate::{StorageError, now_ms};
use aurelius_domain::{
    ConversationId, EmployeeId, ProposalId,
    proposal::{ProposalEvidence, ProposalKind, ProposalStatus},
    views::ProposalView,
};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

fn parse<T, E: std::error::Error + Send + Sync + 'static>(r: Result<T, E>) -> rusqlite::Result<T> {
    r.map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))
}

pub struct NewProposal<'a> {
    pub id: ProposalId,
    pub proposed_by: EmployeeId,
    pub name: &'a str,
    pub role: &'a str,
    pub title: Option<&'a str>,
    /// The (registered) blob holding the proposed instructions.
    pub instructions_blob_id: &'a str,
    pub reports_to: Option<EmployeeId>,
    pub reason: &'a str,
    /// The conversation the proposer was in; the decision goes back there.
    pub origin_conversation_id: Option<ConversationId>,
    pub kind: ProposalKind,
    /// Instruction proposals: whose instructions, on which version, and what they rest on.
    pub target_employee_id: Option<EmployeeId>,
    pub base_version: Option<u32>,
    pub evidence: Vec<ProposalEvidence>,
}

pub fn insert(tx: &Transaction<'_>, p: NewProposal<'_>) -> Result<(), StorageError> {
    let evidence = if p.evidence.is_empty() { None } else { Some(serde_json::to_string(&p.evidence)?) };
    tx.execute(
        "INSERT INTO proposals(id, proposed_by, status, name, role, title, instructions_blob_id, reports_to, reason, origin_conversation_id, created_at_ms,
                                       kind, target_employee_id, base_version, evidence_json)
         VALUES (?1, ?2, 'pending', ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        params![
            p.id.to_string(),
            p.proposed_by.to_string(),
            p.name,
            p.role,
            p.title,
            p.instructions_blob_id,
            p.reports_to.map(|m| m.to_string()),
            p.reason,
            p.origin_conversation_id.map(|c| c.to_string()),
            now_ms(),
            p.kind.as_str(),
            p.target_employee_id.map(|m| m.to_string()),
            p.base_version,
            evidence,
        ],
    )?;
    Ok(())
}

pub struct ProposalRow {
    pub proposed_by: EmployeeId,
    pub status: ProposalStatus,
    pub name: String,
    pub role: String,
    pub title: Option<String>,
    /// Storage key of the proposed instructions.
    pub instructions_key: String,
    pub reports_to: Option<EmployeeId>,
    pub reason: String,
    pub origin_conversation_id: Option<ConversationId>,
    pub kind: ProposalKind,
    pub target_employee_id: Option<EmployeeId>,
    pub base_version: Option<u32>,
}

pub fn load(conn: &Connection, id: ProposalId) -> Result<Option<ProposalRow>, StorageError> {
    Ok(conn
        .query_row(
            "SELECT p.proposed_by, p.status, p.name, p.role, p.title, b.storage_key, p.reports_to, p.reason, p.origin_conversation_id,
                    p.kind, p.target_employee_id, p.base_version
             FROM proposals p JOIN blobs b ON b.id = p.instructions_blob_id WHERE p.id = ?1",
            [id.to_string()],
            |r| {
                Ok(ProposalRow {
                    proposed_by: parse(EmployeeId::parse(&r.get::<_, String>(0)?))?,
                    status: ProposalStatus::parse(&r.get::<_, String>(1)?).unwrap_or(ProposalStatus::Withdrawn),
                    name: r.get(2)?,
                    role: r.get(3)?,
                    title: r.get(4)?,
                    instructions_key: r.get(5)?,
                    reports_to: r.get::<_, Option<String>>(6)?.and_then(|m| EmployeeId::parse(&m).ok()),
                    reason: r.get(7)?,
                    origin_conversation_id: r.get::<_, Option<String>>(8)?.and_then(|c| ConversationId::parse(&c).ok()),
                    kind: ProposalKind::parse(&r.get::<_, String>(9)?).unwrap_or_default(),
                    target_employee_id: r.get::<_, Option<String>>(10)?.and_then(|m| EmployeeId::parse(&m).ok()),
                    base_version: r.get::<_, Option<i64>>(11)?.map(|v| v.max(0) as u32),
                })
            },
        )
        .optional()?)
}

/// What the list query gives for one proposal, before the views are made.
struct Listed {
    view: ProposalView,
    instructions_key: String,
    current_key: Option<String>,
    evidence: Option<String>,
}

/// Pending proposals first (oldest first), then the 20 latest decisions.
pub fn list(conn: &Connection, read_blob: &dyn Fn(&str) -> Option<String>) -> Result<Vec<ProposalView>, StorageError> {
    let mut stmt = conn.prepare(
        "SELECT p.id, p.proposed_by, pm.name, p.status, p.name, p.role, p.title, b.storage_key, p.reports_to, rm.name,
                p.reason, p.decision_note, p.created_at_ms, p.decided_at_ms, p.created_employee_id,
                p.kind, p.target_employee_id, tm.name, p.base_version, p.evidence_json, tb.storage_key
         FROM proposals p
         JOIN employees pm ON pm.id = p.proposed_by
         JOIN blobs b ON b.id = p.instructions_blob_id
         LEFT JOIN employees rm ON rm.id = p.reports_to
         LEFT JOIN employees tm ON tm.id = p.target_employee_id
         LEFT JOIN blobs tb ON tb.id = tm.instructions_blob_id
         WHERE p.status = 'pending'
                OR p.id IN (SELECT id FROM proposals WHERE status <> 'pending' ORDER BY decided_at_ms DESC LIMIT 20)
         ORDER BY p.status <> 'pending', CASE WHEN p.status = 'pending' THEN p.created_at_ms ELSE -COALESCE(p.decided_at_ms, 0) END",
    )?;
    let rows = stmt.query_map([], |r| {
        let kind = ProposalKind::parse(&r.get::<_, String>(15)?).unwrap_or_default();
        Ok(Listed {
            view: ProposalView {
                id: parse(ProposalId::parse(&r.get::<_, String>(0)?))?,
                proposed_by: parse(EmployeeId::parse(&r.get::<_, String>(1)?))?,
                proposed_by_name: r.get(2)?,
                status: ProposalStatus::parse(&r.get::<_, String>(3)?).unwrap_or(ProposalStatus::Withdrawn),
                name: r.get(4)?,
                role: r.get(5)?,
                title: r.get(6)?,
                instructions: String::new(),
                reports_to: r.get::<_, Option<String>>(8)?.and_then(|m| EmployeeId::parse(&m).ok()),
                reports_to_name: r.get(9)?,
                reason: r.get(10)?,
                decision_note: r.get(11)?,
                created_at_ms: r.get(12)?,
                decided_at_ms: r.get(13)?,
                created_employee_id: r.get::<_, Option<String>>(14)?.and_then(|m| EmployeeId::parse(&m).ok()),
                kind,
                target_employee_id: r.get::<_, Option<String>>(16)?.and_then(|m| EmployeeId::parse(&m).ok()),
                target_employee_name: r.get(17)?,
                base_version: r.get::<_, Option<i64>>(18)?.map(|v| v.max(0) as u32),
                current_instructions: None,
                current_version: None,
                evidence: Vec::new(),
            },
            instructions_key: r.get(7)?,
            current_key: r.get(20)?,
            evidence: r.get(19)?,
        })
    })?;
    let listed: Vec<Listed> = rows.collect::<Result<_, _>>()?;
    let mut out = Vec::new();
    for l in listed {
        let mut view = l.view;
        view.instructions = read_blob(&l.instructions_key).unwrap_or_default();
        view.evidence = l.evidence.and_then(|e| serde_json::from_str(&e).ok()).unwrap_or_default();
        if view.kind == ProposalKind::Instructions {
            view.current_instructions = Some(l.current_key.as_deref().and_then(read_blob).unwrap_or_default());
            if let Some(target) = view.target_employee_id {
                view.current_version = Some(crate::instruction_versions::current(conn, target)?);
            }
        }
        out.push(view);
    }
    Ok(out)
}

/// Pending colleague proposals of an employee (at most `proposal::MAX_OPEN_PER_EMPLOYEE` may be open).
pub fn open_count(conn: &Connection, employee: EmployeeId) -> Result<usize, StorageError> {
    Ok(conn.query_row("SELECT COUNT(*) FROM proposals WHERE proposed_by = ?1 AND kind = 'colleague' AND status = 'pending'", [employee.to_string()], |r| r.get::<_, i64>(0))? as usize)
}

/// Pending instruction proposals for `target`'s instructions (at most `proposal::MAX_OPEN_PER_TARGET`).
pub fn open_for_target(conn: &Connection, target: EmployeeId) -> Result<usize, StorageError> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM proposals WHERE target_employee_id = ?1 AND kind = 'instructions' AND status = 'pending'",
        [target.to_string()],
        |r| r.get::<_, i64>(0),
    )? as usize)
}

/// Record the decision of a pending proposal; false when it was not pending (decided meanwhile).
pub fn decide(tx: &Transaction<'_>, id: ProposalId, status: ProposalStatus, note: Option<&str>, created_employee: Option<EmployeeId>) -> Result<bool, StorageError> {
    Ok(tx.execute(
        "UPDATE proposals SET status = ?2, decision_note = ?3, created_employee_id = ?4, decided_at_ms = ?5 WHERE id = ?1 AND status = 'pending'",
        params![id.to_string(), status.as_str(), note, created_employee.map(|m| m.to_string()), now_ms()],
    )? == 1)
}

/// The proposer (or the employee whose instructions it is about) was archived:
/// those open proposals are withdrawn. Returns them.
pub fn withdraw_for(tx: &Transaction<'_>, employee: EmployeeId) -> Result<Vec<ProposalId>, StorageError> {
    let mut stmt = tx.prepare("SELECT id FROM proposals WHERE (proposed_by = ?1 OR target_employee_id = ?1) AND status = 'pending'")?;
    let ids: Vec<ProposalId> =
        stmt.query_map([employee.to_string()], |r| parse(ProposalId::parse(&r.get::<_, String>(0)?)))?.collect::<Result<_, _>>()?;
    drop(stmt);
    for id in &ids {
        decide(tx, *id, ProposalStatus::Withdrawn, None, None)?;
    }
    Ok(ids)
}
