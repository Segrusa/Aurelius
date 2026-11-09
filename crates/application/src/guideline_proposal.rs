//! `propose_guideline` (B5, SEC0 E9–E13): an employee proposes a change of a
//! guideline in its own block, or a new one, during a run. Only a board member
//! decides (`guideline.decide_proposal`), in every mode, Auto too: nothing here
//! writes a guideline or a version.
//!
//! - E9: the guideline is looked up in the run's own snapshot (`run_guidelines`
//!   of the bridge call's run), never in the library: by `g:` id, id or exact
//!   title (case-insensitive, more than one match is refused). An archived one
//!   is refused; none is a new guideline. `base_version` is the snapshot's.
//! - E10: only a live run of an employee, in a conversation; never
//!   the project assistant or the dispatcher (triage). Text-only runs
//!   have no bridge; the daemon refuses them before this.
//! - E11: validated as a guideline write, plus evidence; the open limits are
//!   counted in the same (immediate) transaction as the insert. B4 waits for the
//!   approval.
//! - E12: `suggested_links` are the proposer and the run's project, set here.
//! - E13: the answer says only that it is proposed and a board member decides.

use crate::task::task_runs::{RunOwner, run_owner};
use crate::{CommandService, storage_error};
use aurelius_domain::{
    ActorKind, AppError, ConversationId, EmployeeId, EventKind, EventNotice, GuidelineId, GuidelineProposalId, ProjectId, RunId,
    employee::LifecycleStatus,
    guideline::{self, GuidelineProposedMeta, GuidelineTarget},
};
use aurelius_storage::{StorageError, Store, Tx, guidelines};
use rusqlite::OptionalExtension;

/// What the bridge tool `propose_guideline` sends.
#[derive(Debug, Clone)]
pub struct GuidelineProposalInput {
    /// A guideline of the run's own block (`g:` id, id or title); none: a new guideline.
    pub guideline: Option<String>,
    pub title: String,
    /// The whole new markdown.
    pub text: String,
    pub reason: String,
    /// Tasks (id or title) and memories (id or words of it) the proposal rests on.
    pub evidence: Vec<String>,
}

/// The only answer to a proposal that was taken (E13): no texts, no names.
pub const PROPOSED: &str = "Proposed; a board member decides, whatever mode you work in. Until then the guidelines stay as they are; carry on with your work.";
/// A guideline that is not in this run's block (E9).
pub const NOT_IN_BLOCK: &str = "You can only propose a change to a guideline in your own block, or a new one.";

impl CommandService {
    /// `propose_guideline` from a run. Returns (ok, text for the model, notices).
    pub fn propose_guideline(&self, run: RunId, input: GuidelineProposalInput) -> Result<(bool, String, Vec<EventNotice>), AppError> {
        let st = self.store.clone();
        self.store.write(move |tx| propose(tx, &st, run, input)).map_err(storage_error)
    }
}

fn propose(tx: &Tx<'_>, st: &Store, run: RunId, input: GuidelineProposalInput) -> Result<(bool, String, Vec<EventNotice>), StorageError> {
    let refuse = |why: String| Ok((false, why, vec![]));
    let EmployeeRun { employee: me, project, conversation, name } = match employee_run(tx, run, "propose a guideline")? {
        Ok(found) => found,
        Err(why) => return refuse(why),
    };

    // E11: the fields as a guideline write takes them (E1/E2 included: domain refuses what a board member could not see).
    if aurelius_domain::proposal::validate_evidence(&input.evidence).is_err() {
        return refuse("Name at least one task (its id or title) or memory (a lesson: its id or words of it) the proposal rests on, in evidence.".into());
    }
    let fields = match guideline::validate_proposal(&input.title, &input.text, &input.reason, &input.evidence) {
        Ok(fields) => fields,
        Err(e) => return refuse(e.to_string()),
    };

    // E9: the target from the run's own snapshot only.
    let target = match input.guideline.as_deref().map(str::trim).filter(|g| !g.is_empty()) {
        None => None,
        Some(wanted) => match in_block(tx, run, wanted)? {
            Ok(found) => Some(found),
            Err(why) => return refuse(why),
        },
    };
    if let Some((id, _)) = target {
        if guidelines::view(tx, id)?.is_none_or(|g| g.archived) {
            return refuse("That guideline is archived; propose a new one instead.".into());
        }
        if guidelines::count_open_for_guideline(tx, id)? >= guideline::MAX_OPEN_PER_GUIDELINE {
            return refuse("A proposal for this guideline already waits for a board member's decision; wait for the outcome first.".into());
        }
    } else if guidelines::count_open_new_for_employee(tx, me)? >= guideline::MAX_OPEN_NEW_PER_EMPLOYEE {
        return refuse(format!("You already have {} proposals for a new guideline waiting; wait for a decision first.", guideline::MAX_OPEN_NEW_PER_EMPLOYEE));
    }

    let mut evidence = Vec::new();
    for r in &fields.evidence {
        match crate::org::find_evidence(tx, st, project, me, r)? {
            Ok(found) => evidence.push(found),
            Err(why) => return refuse(why),
        }
    }

    // E12: the daemon's links, never the agent's: the proposer, and the run's project. A change
    // of an existing guideline suggests none: it never changes the links.
    let mut suggested_links = Vec::new();
    if target.is_none() {
        suggested_links.push(GuidelineTarget::Employee(me));
        suggested_links.extend(project.map(GuidelineTarget::Project));
    }
    let id = GuidelineProposalId::new();
    let now = aurelius_storage::now_ms();
    guidelines::insert_proposal(
        tx,
        guidelines::NewGuidelineProposal {
            id,
            guideline_id: target.map(|(g, _)| g),
            base_version: target.map(|(_, v)| v),
            proposed_by: me,
            title: &fields.title,
            text: &fields.text,
            reason: &fields.reason,
            evidence: &evidence,
            suggested_links: &suggested_links,
            origin_conversation_id: Some(conversation),
            run_id: Some(run),
            now_ms: now,
        },
    )?;
    // E15: what a notification and a push show is the title and who proposed it, never the text.
    let mut metadata = serde_json::to_value(GuidelineProposedMeta { proposal_id: id, guideline_id: target.map(|(g, _)| g), by_employee: me })?;
    metadata["subject"] = serde_json::Value::String(format!("“{}” by {}", fields.title, name));
    let (_, notice) = crate::org::append_event(
        tx,
        crate::org::OrgEvent {
            aggregate_type: "guideline_proposal",
            aggregate_id: id.to_string(),
            kind: EventKind::GuidelineProposed,
            project: None,
            actor: ActorKind::Employee,
            actor_id: Some(me.to_string()),
            correlation: run.to_string(),
            payload: None,
            metadata,
            dedupe: format!("guideline_proposal:{id}:proposed"),
        },
    )?;
    Ok((true, PROPOSED.into(), vec![notice]))
}

/// The employee whose run it is, as E10 found it.
pub(crate) struct EmployeeRun {
    pub employee: EmployeeId,
    pub project: Option<ProjectId>,
    pub conversation: ConversationId,
    pub name: String,
}

/// E10, for `propose_guideline` and GL-15's `list_guidelines`/`read_guideline`: a
/// live run of an active employee in one of its conversations, never the
/// project assistant (no employee) or the dispatcher (no conversation). `what`: what is refused ("propose a guideline").
pub(crate) fn employee_run(
    conn: &rusqlite::Connection,
    run: RunId,
    what: &str,
) -> Result<Result<EmployeeRun, String>, StorageError> {
    let Some(RunOwner { employee: Some(me), project, conversation: Some(conversation) }) = run_owner(conn, run)? else {
        return Ok(Err(format!("Only an employee can {what}.")));
    };
    let status: String = conn.query_row("SELECT status FROM runs WHERE id = ?1", [run.to_string()], |r| r.get(0))?;
    if matches!(status.as_str(), "succeeded" | "failed" | "cancelled" | "unknown") {
        return Ok(Err("This run has ended.".into()));
    }
    let employee: Option<(String, String)> = conn.query_row("SELECT name, status FROM employees WHERE id = ?1", [me.to_string()], |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
    let Some((name, _)) = employee.filter(|(_, status)| status == LifecycleStatus::Active.as_str()) else {
        return Ok(Err(format!("Only an active employee can {what}.")));
    };
    Ok(Ok(EmployeeRun { employee: me, project, conversation, name }))
}

/// The guideline `wanted` names in the run's snapshot, with the version the run had:
/// `g:<id>` (also with `@<version>`), the id, or the exact title (case-insensitive).
fn in_block(tx: &Tx<'_>, run: RunId, wanted: &str) -> Result<Result<(GuidelineId, u32), String>, StorageError> {
    let block = guidelines::for_run(tx, run)?;
    let bare = wanted.strip_prefix("g:").unwrap_or(wanted);
    let bare = bare.split_once('@').map_or(bare, |(id, _)| id).trim();
    if let Ok(id) = GuidelineId::parse(bare) {
        return Ok(block.iter().find(|g| g.guideline_id == id).map(|g| (g.guideline_id, g.version)).ok_or_else(|| NOT_IN_BLOCK.to_string()));
    }
    let titled: Vec<_> = block.iter().filter(|g| g.title.trim().eq_ignore_ascii_case(wanted)).collect();
    Ok(match titled.as_slice() {
        [one] => Ok((one.guideline_id, one.version)),
        [] => Err(NOT_IN_BLOCK.to_string()),
        _ => Err("More than one guideline in your block has that title; name it by its g: id.".to_string()),
    })
}

#[cfg(test)]
#[path = "guideline_proposal_tests.rs"]
mod tests;
