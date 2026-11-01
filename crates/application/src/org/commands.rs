//! The company's org chart (who reports to whom) and colleague proposals.
//!
//! The rules themselves are pure and live in `aurelius_domain::org`; this module
//! applies them: the commands that change the chart, the text that tells an employee
//! about its place on it, the delegation limits and escalation to the boss while the
//! server setting "Follow the org chart" is on (the default), and
//! `propose_colleague` with the user's decision on it. A proposal is only ever
//! approved by `DecideProposal` (the user); no mode, not even Auto, approves one.

use crate::org::employee::{NewEmployee, insert_new_employee};
use crate::task::task_runs::{RunOwner, insert_part, live_in, run_owner};
use crate::task::task_threads::{self as thread, Speaker};
use crate::task::tasks::Colleague;
use crate::{CommandContext, CommandService, Committed, domain_error, notice, storage_error};
use aurelius_domain::{
    ActorKind, AppError, CommandEnvelope, CommandId, CommandResult, ConversationId, DomainError, EventId, EventKind, EventNotice, EmployeeId, ProposalId, ProjectId, RunId, TaskId,
    employee::{LifecycleStatus, SetFollowOrgChart, SetEmployeeReportsTo},
    org::{MAX_CHART_DEPTH, Org},
    proposal::{self, DecideProposal, ProposalEvidence, ProposalFields, ProposalKind, ProposalStatus, RestoreEmployeeInstructions},
    views::{EmployeeInstructionVersionsSnapshot, ProposalsSnapshot},
};
use aurelius_storage::instruction_versions::{self as versions, Source};
use aurelius_storage::{
    StorageError, Store, Tx, blobs,
    events::{self, NewEvent},
    employees, proposals, queries,
    tasks as store,
};
use rusqlite::Connection;

/// One event of the org chart or a proposal; company-wide, so
/// mostly without a project.
pub(crate) struct OrgEvent {
    pub aggregate_type: &'static str,
    pub aggregate_id: String,
    pub kind: EventKind,
    pub project: Option<ProjectId>,
    pub actor: ActorKind,
    pub actor_id: Option<String>,
    pub correlation: String,
    pub payload: Option<String>,
    pub metadata: serde_json::Value,
    pub dedupe: String,
}

pub(crate) fn append_event(tx: &Tx<'_>, e: OrgEvent) -> Result<(EventId, EventNotice), StorageError> {
    let version = aurelius_storage::conversations::next_version(tx, e.aggregate_type, &e.aggregate_id)?;
    let appended = events::append(
        tx,
        NewEvent {
            aggregate_type: e.aggregate_type,
            aggregate_id: e.aggregate_id.clone(),
            aggregate_version: version,
            event_type: e.kind.as_str(),
            project_id: e.project,
            actor_kind: e.actor,
            actor_id: e.actor_id,
            correlation_id: e.correlation,
            causation_id: None,
            payload_blob_id: e.payload,
            metadata: e.metadata,
            dedupe_key: e.dedupe,
        },
    )?;
    Ok((appended.id, notice(appended.seq, e.kind.as_str(), e.aggregate_id, e.project)))
}

/// The company's org chart while the company follows it; `None` when it does
/// not (then nothing about delegation, hand-over or escalation changes).
pub(crate) fn chart(conn: &Connection) -> Result<Option<Org>, StorageError> {
    if !employees::follow_org_chart(conn)? {
        return Ok(None);
    }
    Ok(Some(employees::org(conn)?))
}

// ── What an employee is told ──────────────────────────────────────────────

/// The org chart as text for the system prompt of employee `me`: whom it reports
/// to, who reports to it, the other colleagues. `None` while nobody in the company
/// has a title or a boss: a flat company's text stays exactly as it was.
pub(crate) fn chart_text(me: EmployeeId, colleagues: &[Colleague], follow: bool) -> Option<String> {
    if !colleagues.iter().any(|c| c.title.is_some() || c.reports_to.is_some()) {
        return None;
    }
    let mine = colleagues.iter().find(|c| c.id == me);
    let boss = mine.and_then(|m| m.reports_to).and_then(|b| colleagues.iter().find(|c| c.id == b));
    let reports: Vec<&Colleague> = colleagues.iter().filter(|c| c.reports_to == Some(me)).collect();
    let others: Vec<&Colleague> = colleagues.iter().filter(|c| c.id != me && Some(c.id) != boss.map(|b| b.id) && c.reports_to != Some(me)).collect();
    // With the chart followed only peers (same boss) are reachable; the rest is informative.
    let (peers, far): (Vec<&Colleague>, Vec<&Colleague>) = if follow {
        others.iter().copied().partition(|c| c.reports_to == mine.and_then(|m| m.reports_to))
    } else {
        (others.clone(), vec![])
    };
    let mut lines = Vec::new();
    if let Some(title) = mine.and_then(|m| m.title.as_deref()) {
        lines.push(format!("Your position: {title}."));
    }
    if let Some(boss) = boss {
        lines.push(format!("You report to {}.", label(boss)));
    }
    if !reports.is_empty() {
        lines.push(format!("Reporting to you: {}.", list(&reports)));
    }
    if !peers.is_empty() {
        lines.push(format!("{}: {}.", if follow { "Peers (same boss)" } else { "Other colleagues" }, list(&peers)));
    }
    if !far.is_empty() {
        lines.push(format!("Not directly reachable, go via your boss or your report: {}.", list(&far)));
    }
    if follow {
        let org = Org::new(colleagues.iter().map(|c| (c.id, c.reports_to)));
        let below = names_of(colleagues, &org.reports(me));
        lines.push(format!(
            "The company follows its org chart: delegate_task only works for {}, and hand_over_task only to your boss or a peer. Colleagues further away are not directly reachable: go through your boss or the report who leads them.",
            if below.is_empty() { "nobody (nobody reports to you)".to_string() } else { below.join(", ") }
        ));
    }
    if lines.is_empty() {
        return None;
    }
    Some(format!("\n\nOrg chart of the company. {}", lines.join(" ")))
}

fn label(c: &Colleague) -> String {
    format!("{} ({})", c.name, c.title.as_deref().unwrap_or(&c.role))
}

fn list(of: &[&Colleague]) -> String {
    of.iter().map(|c| label(c)).collect::<Vec<_>>().join(", ")
}

fn names_of(colleagues: &[Colleague], ids: &[EmployeeId]) -> Vec<String> {
    colleagues.iter().filter(|c| ids.contains(&c.id)).map(|c| c.name.clone()).collect()
}

/// The tool answer when `delegate_task` goes to someone who does not report to the caller directly.
pub(crate) fn refuse_delegation(org: &Org, me: EmployeeId, to: EmployeeId, colleagues: &[Colleague]) -> String {
    let name = |id: EmployeeId| names_of(colleagues, &[id]).into_iter().next().unwrap_or_default();
    let reports = names_of(colleagues, &org.reports(me));
    if let Some(route) = org.route_down(me, to) {
        return match org.boss(to) {
            Some(boss) if boss == route => format!("{} reports to {}; delegate to {} instead.", name(to), name(boss), name(route)),
            Some(boss) => format!("{} reports to {}, who is below {}; delegate to {} instead.", name(to), name(boss), name(route), name(route)),
            None => format!("Delegate to {} instead.", name(route)),
        };
    }
    if reports.is_empty() {
        "Delegation follows the org chart and nobody reports to you, so you cannot give work to a colleague; do this part yourself or hand the task over to your boss or a peer.".to_string()
    } else {
        format!("{} is not below you in the org chart. Delegation follows the org chart: you can give work to {} only (the colleagues who report to you directly). Do this part yourself or hand the task over to your boss or a peer.", name(to), reports.join(", "))
    }
}

/// The tool answer when `hand_over_task` goes to someone other than the boss or a peer.
pub(crate) fn refuse_hand_over(org: &Org, me: EmployeeId, colleagues: &[Colleague]) -> String {
    let mut allowed: Vec<EmployeeId> = org.boss(me).into_iter().collect();
    allowed.extend(org.peers(me));
    let names = names_of(colleagues, &allowed);
    if names.is_empty() {
        "Handing over follows the org chart and there is nobody you can hand this task to; finish it yourself.".to_string()
    } else {
        format!("Handing over follows the org chart: you can hand a task over to {} only (your boss or a peer).", names.join(", "))
    }
}

// ── Escalation to the boss ──────────────────────────────────────────────

/// A task of a non-root employee failed or stopped and waits for the user, and the
/// company follows its org chart: the boss gets a task about it first (in the
/// task's project). The boss's
/// answer comes back as an update in the original task's conversation. Returns
/// the boss's name and the notices when the task went up; `None` when nothing
/// changes (the setting is off, the employee is a root, the task is a sub-task whose
/// delegating employee already follows it up): then the user is told as before.
/// The boss's own escalation task escalates the same way, so it climbs to a root.
pub(crate) fn escalate(tx: &Tx<'_>, st: &Store, task: TaskId, row: &store::TaskRow, why: &str, so_far: &str) -> Result<Option<(String, Vec<EventNotice>)>, StorageError> {
    if row.parent.is_some() {
        return Ok(None);
    }
    let Some(me) = row.employee_id else { return Ok(None) };
    let Some(org) = chart(tx)? else { return Ok(None) };
    let Some(boss) = org.escalation_target(me) else { return Ok(None) };
    let (Some(boss_row), Some(me_row)) = (employees::load_employee(tx, boss)?, employees::load_employee(tx, me)?) else { return Ok(None) };
    if boss_row.status != LifecycleStatus::Active {
        return Ok(None);
    }
    let Some((conversation, _)) = store::task_conversation(tx, task)? else { return Ok(None) };
    let goal = st.blobs().read(&row.goal_key).ok().and_then(|b| String::from_utf8(b).ok()).unwrap_or_default();
    let so_far: String = so_far.trim().chars().take(3_000).collect();
    let result = if so_far.is_empty() { String::new() } else { format!("What the task has so far:\n{so_far}\n\n") };
    let text = format!(
        "{who} ({role}) could not finish “{title}”: {why}.\n\nThe task's goal:\n{goal}\n\n{result}You can look into it, talk with {who} in the task's conversation with message_task (find the task with list_tasks), take it over, or tell the user what is needed. Your answer goes back to {who} as an update in that conversation.",
        who = me_row.name,
        role = me_row.role,
        title = row.title,
        goal = goal.trim(),
    );
    let colleague = Colleague { id: boss, name: boss_row.name.clone(), role: boss_row.role, focus: String::new(), title: boss_row.title, reports_to: boss_row.reports_to };
    let note = format!("(Escalated by {} because of the org chart.)", me_row.name);
    // On the escalated task's login.
    let login = crate::run::login::of_task(tx, task)?;
    let Some((_, _, notices)) = insert_part(tx, st, row.project_id, Some(me), &colleague, &format!("Escalated: {}", row.title), &text, &[], &note, None, Some(conversation), login)? else {
        return Ok(None);
    };
    Ok(Some((boss_row.name, notices)))
}

// ── Archiving ───────────────────────────────────────────────────────────

/// Called before `employee` is archived, while it is still on the chart: its direct
/// reports move one level up (one `employee.reports_to_set` event each), its open
/// proposals are withdrawn (of instructions and colleagues, and of guidelines, B5).
pub(crate) fn on_employee_archived(tx: &Tx<'_>, employee: EmployeeId, command_id: CommandId, meta: &serde_json::Value) -> Result<Vec<EventNotice>, StorageError> {
    let moved = employees::org(tx)?.on_archive(employee);
    let mut notices = Vec::new();
    for (child, boss) in moved {
        employees::set_reports_to(tx, child, boss)?;
        let (_, n) = append_event(
            tx,
            OrgEvent {
                aggregate_type: "employee",
                aggregate_id: child.to_string(),
                kind: EventKind::EmployeeReportsToSet,
                project: None,
                actor: ActorKind::User,
                actor_id: None,
                correlation: command_id.to_string(),
                payload: None,
                metadata: serde_json::json!({ "from": employee, "to": boss, "reason": "boss_archived", "ctx": meta }),
                dedupe: format!("cmd:{command_id}:moved:{child}"),
            },
        )?;
        notices.push(n);
    }
    proposals::withdraw_for(tx, employee)?;
    aurelius_storage::guidelines::withdraw_for_employee(tx, employee, aurelius_storage::now_ms())?;
    Ok(notices)
}

// ── Commands ────────────────────────────────────────────────────────────

/// `boss` is an employee who is not archived; `employee` under it closes no loop
/// and keeps the chart at most `MAX_CHART_DEPTH` levels deep.
pub(crate) fn check_boss(tx: &Tx<'_>, employee: EmployeeId, boss: EmployeeId) -> Result<Result<(), DomainError>, StorageError> {
    match employees::load_employee(tx, boss)? {
        None => return Ok(Err(DomainError::EmployeeNotFound(boss))),
        Some(row) if row.status == LifecycleStatus::Archived => {
            return Ok(Err(DomainError::Org("An employee reports to a colleague who is not archived.".into())));
        }
        Some(_) => {}
    }
    let entries = employees::org_entries(tx)?;
    if Org::new(entries.iter().copied()).would_cycle(employee, boss) {
        return Ok(Err(DomainError::OrgCycle));
    }
    let moved = Org::new(entries.iter().map(|(e, b)| if *e == employee { (*e, Some(boss)) } else { (*e, *b) }));
    if moved.height() > MAX_CHART_DEPTH {
        return Ok(Err(DomainError::Org(format!("The org chart may be at most {MAX_CHART_DEPTH} levels deep."))));
    }
    Ok(Ok(()))
}

impl CommandService {
    pub(crate) fn set_employee_reports_to(&self, ctx: &CommandContext, envelope: &CommandEnvelope, hash: String, cmd: &SetEmployeeReportsTo) -> Result<Committed, AppError> {
        let (employee_id, boss) = (cmd.employee_id, cmd.reports_to);
        let command_id = envelope.command_id;
        let meta = ctx.metadata();
        self.commit(command_id, hash, move |tx| {
            let Some(row) = employees::load_employee(tx, employee_id)? else {
                return Ok(Err(domain_error(DomainError::EmployeeNotFound(employee_id))));
            };
            if row.status == LifecycleStatus::Archived {
                return Ok(Err(domain_error(DomainError::EmployeeArchived)));
            }
            if let Some(boss) = boss {
                if let Err(refused) = check_boss(tx, employee_id, boss)? {
                    return Ok(Err(domain_error(refused)));
                }
            }
            let result = CommandResult::EmployeeReportsToSet { employee_id, reports_to: boss };
            // The same boss again changes nothing (but is still answered with a receipt).
            if row.reports_to == boss {
                return Ok(Ok((result, vec![])));
            }
            employees::set_reports_to(tx, employee_id, boss)?;
            let (_, n) = append_event(
                tx,
                OrgEvent {
                    aggregate_type: "employee",
                    aggregate_id: employee_id.to_string(),
                    kind: EventKind::EmployeeReportsToSet,
                    project: None,
                    actor: ActorKind::User,
                    actor_id: None,
                    correlation: command_id.to_string(),
                    payload: None,
                    metadata: serde_json::json!({ "from": row.reports_to, "to": boss, "ctx": meta }),
                    dedupe: format!("cmd:{command_id}:0"),
                },
            )?;
            Ok(Ok((result, vec![n])))
        })
    }

    /// The server setting "Follow the org chart".
    pub(crate) fn set_follow_org_chart(&self, ctx: &CommandContext, envelope: &CommandEnvelope, hash: String, cmd: &SetFollowOrgChart) -> Result<Committed, AppError> {
        let follow = cmd.follow;
        let command_id = envelope.command_id;
        let meta = ctx.metadata();
        self.commit(command_id, hash, move |tx| {
            let result = CommandResult::FollowOrgChartSet { follow };
            if employees::follow_org_chart(tx)? == follow {
                return Ok(Ok((result, vec![])));
            }
            employees::set_follow_org_chart(tx, follow)?;
            let (_, n) = append_event(
                tx,
                OrgEvent {
                    aggregate_type: "company",
                    aggregate_id: "company".to_string(),
                    kind: EventKind::SettingsFollowOrgChart,
                    project: None,
                    actor: ActorKind::User,
                    actor_id: None,
                    correlation: command_id.to_string(),
                    payload: None,
                    metadata: serde_json::json!({ "follow": follow, "ctx": meta }),
                    dedupe: format!("cmd:{command_id}:0"),
                },
            )?;
            Ok(Ok((result, vec![n])))
        })
    }

    /// The user's decision on a proposed colleague. Approving creates the employee
    /// (like `employee.create`), sets it under its boss and marks the proposal in one
    /// transaction; either way the decision is posted into the proposer's
    /// conversation, and the proposer answers it unless a run of that conversation is live.
    pub(crate) fn decide_proposal(&self, ctx: &CommandContext, envelope: &CommandEnvelope, hash: String, cmd: &DecideProposal) -> Result<Committed, AppError> {
        let id = cmd.proposal_id;
        let note = proposal::validate_note(cmd.note.as_deref()).map_err(domain_error)?;
        let st = self.store.clone();
        let existing = self
            .store
            .read(|c| proposals::load(c, id))
            .map_err(storage_error)?
            .ok_or_else(|| domain_error(DomainError::ProposalNotFound(id)))?;
        if existing.status != ProposalStatus::Pending {
            return Err(domain_error(DomainError::ApprovalNotPending));
        }
        if existing.kind == ProposalKind::Instructions {
            return self.decide_instructions_proposal(ctx, envelope, hash, cmd, existing, note);
        }
        // Approving: the employee to create (with the user's edits, if any), prepared before the transaction.
        let prepared = if cmd.approve {
            let fields = match &cmd.edits {
                Some(edits) => edits.clone(),
                None => ProposalFields {
                    name: existing.name.clone(),
                    role: existing.role.clone(),
                    title: existing.title.clone(),
                    instructions: st.blobs().read(&existing.instructions_key).ok().and_then(|b| String::from_utf8(b).ok()).unwrap_or_default(),
                    reports_to: existing.reports_to,
                },
            }
            .validated()
            .map_err(domain_error)?;
            let boss = fields.reports_to.or(existing.reports_to).or(Some(existing.proposed_by));
            Some(NewEmployee::prepare(&st, &fields.name, &fields.role, fields.title.clone(), boss, Some(&fields.instructions), None)?)
        } else {
            None
        };
        let (approve, command_id) = (cmd.approve, envelope.command_id);
        let meta = ctx.metadata();
        let actor = ctx.principal_id.to_string();
        self.commit(command_id, hash, move |tx| {
            let Some(p) = proposals::load(tx, id)? else {
                return Ok(Err(domain_error(DomainError::ProposalNotFound(id))));
            };
            if p.status != ProposalStatus::Pending {
                return Ok(Err(domain_error(DomainError::ApprovalNotPending)));
            }
            let mut notices = Vec::new();
            let (status, created, text, subject) = match &prepared {
                Some(new) => {
                    // The proposer must still work here; otherwise the proposal lapses.
                    if !employees::load_employee(tx, p.proposed_by)?.is_some_and(|m| m.status != LifecycleStatus::Archived) {
                        proposals::decide(tx, id, ProposalStatus::Withdrawn, None, None)?;
                        return Ok(Err(domain_error(DomainError::EmployeeArchived)));
                    }
                    let correlation = command_id.to_string();
                    match insert_new_employee(tx, new, ActorKind::User, Some(actor.clone()), &correlation, meta.clone(), format!("cmd:{command_id}:created"))? {
                        Ok(n) => notices.push(n),
                        Err(refused) => return Ok(Err(refused)),
                    }
                    let boss = match new.reports_to {
                        Some(b) => employees::load_employee(tx, b)?.map(|m| m.name),
                        None => None,
                    };
                    let text = match boss {
                        Some(boss) => format!("{} joined as {}, reporting to {boss}.", new.name, new.role),
                        None => format!("{} joined as {}.", new.name, new.role),
                    };
                    (ProposalStatus::Approved, Some(new.employee_id), text, new.name.clone())
                }
                None => {
                    let text = match &note {
                        Some(note) => format!("Rejected: {note}"),
                        None => "Rejected.".to_string(),
                    };
                    (ProposalStatus::Rejected, None, text, p.name.clone())
                }
            };
            proposals::decide(tx, id, status, note.as_deref(), created)?;
            let (_, decided) = append_event(
                tx,
                OrgEvent {
                    aggregate_type: "proposal",
                    aggregate_id: id.to_string(),
                    kind: EventKind::ProposalDecided,
                    project: None,
                    actor: ActorKind::User,
                    actor_id: Some(actor),
                    correlation: command_id.to_string(),
                    payload: None,
                    metadata: serde_json::json!({ "approved": approve, "kind": "colleague", "subject": subject, "note": note, "proposal_id": id, "employee_id": created, "ctx": meta }),
                    dedupe: format!("cmd:{command_id}:decided"),
                },
            )?;
            notices.push(decided);
            notices.extend(answer_proposer(tx, &st, &p, id, &text, &meta["user_id"])?);
            Ok(Ok((CommandResult::ProposalDecided { proposal_id: id, status, employee_id: created }, notices)))
        })
    }

    /// The user's decision on new instructions an employee proposed. Approving
    /// applies them (or the user's edited text) as a new version, unless the
    /// instructions changed since the proposal was made (`version_conflict`);
    /// the employee's conversations then start a new session with them.
    fn decide_instructions_proposal(&self, ctx: &CommandContext, envelope: &CommandEnvelope, hash: String, cmd: &DecideProposal, existing: proposals::ProposalRow, note: Option<String>) -> Result<Committed, AppError> {
        let id = cmd.proposal_id;
        let st = self.store.clone();
        let edited = cmd.instructions.as_deref().map(proposal::validate_instructions).transpose().map_err(domain_error)?;
        let edited_blob = match &edited {
            Some(text) if cmd.approve => Some(st.blobs().put(text.as_bytes(), "text/plain; charset=utf-8").map_err(storage_error)?),
            _ => None,
        };
        let Some(target) = existing.target_employee_id else {
            return Err(domain_error(DomainError::Org("This proposal names no employee.".into())));
        };
        let (approve, command_id) = (cmd.approve, envelope.command_id);
        let meta = ctx.metadata();
        let actor = ctx.principal_id.to_string();
        self.commit(command_id, hash, move |tx| {
            let Some(p) = proposals::load(tx, id)? else {
                return Ok(Err(domain_error(DomainError::ProposalNotFound(id))));
            };
            if p.status != ProposalStatus::Pending {
                return Ok(Err(domain_error(DomainError::ApprovalNotPending)));
            }
            let Some(row) = employees::load_employee(tx, target)? else {
                return Ok(Err(domain_error(DomainError::EmployeeNotFound(target))));
            };
            let mut notices = Vec::new();
            let (status, text) = if approve {
                if row.status == LifecycleStatus::Archived {
                    proposals::decide(tx, id, ProposalStatus::Withdrawn, None, None)?;
                    return Ok(Err(domain_error(DomainError::EmployeeArchived)));
                }
                // Made on an older version: the user must look again at what is there now.
                let current = versions::current(tx, target)?;
                let base = p.base_version.unwrap_or(current);
                if base != current {
                    return Ok(Err(domain_error(DomainError::VersionConflict { expected: base as u64, actual: current as u64 })));
                }
                let blob_id = match &edited_blob {
                    Some(blob) => {
                        blobs::register(tx, blob)?;
                        blob.id.to_string()
                    }
                    None => tx.query_row("SELECT instructions_blob_id FROM proposals WHERE id = ?1", [id.to_string()], |r| r.get::<_, String>(0))?,
                };
                let applied = apply_instructions(tx, target, &row, Some(&blob_id), Source::Proposal(id), command_id, &meta, serde_json::json!({ "proposal_id": id }))?;
                notices.push(applied.1);
                let how = if edited_blob.is_some() { " with the user's changes" } else { "" };
                (ProposalStatus::Approved, format!("Approved{how}: {}'s instructions are now version {}. Conversations start with them from their next answer.", row.name, applied.0))
            } else {
                (ProposalStatus::Rejected, match &note {
                    Some(note) => format!("Rejected the proposed instructions for {}: {note}", row.name),
                    None => format!("Rejected the proposed instructions for {}.", row.name),
                })
            };
            proposals::decide(tx, id, status, note.as_deref(), None)?;
            let (_, decided) = append_event(
                tx,
                OrgEvent {
                    aggregate_type: "proposal",
                    aggregate_id: id.to_string(),
                    kind: EventKind::ProposalDecided,
                    project: None,
                    actor: ActorKind::User,
                    actor_id: Some(actor),
                    correlation: command_id.to_string(),
                    payload: None,
                    metadata: serde_json::json!({ "approved": approve, "kind": "instructions", "subject": row.name, "note": note, "proposal_id": id, "target_employee_id": target, "edited": edited_blob.is_some(), "ctx": meta }),
                    dedupe: format!("cmd:{command_id}:decided"),
                },
            )?;
            notices.push(decided);
            notices.extend(answer_proposer(tx, &st, &p, id, &text, &meta["user_id"])?);
            Ok(Ok((CommandResult::ProposalDecided { proposal_id: id, status, employee_id: None }, notices)))
        })
    }

    /// Put an earlier version of an employee's instructions back, as a new version.
    pub(crate) fn restore_employee_instructions(&self, ctx: &CommandContext, envelope: &CommandEnvelope, hash: String, cmd: &RestoreEmployeeInstructions) -> Result<Committed, AppError> {
        let (employee_id, wanted) = (cmd.employee_id, cmd.version);
        let command_id = envelope.command_id;
        let meta = ctx.metadata();
        self.commit(command_id, hash, move |tx| {
            let Some(row) = employees::load_employee(tx, employee_id)? else {
                return Ok(Err(domain_error(DomainError::EmployeeNotFound(employee_id))));
            };
            if row.status == LifecycleStatus::Archived {
                return Ok(Err(domain_error(DomainError::EmployeeArchived)));
            }
            if versions::current(tx, employee_id)? == wanted {
                return Ok(Err(domain_error(DomainError::OutOfRange { field: "version".into(), detail: format!("version {wanted} is the one in use") })));
            }
            let Some(blob) = versions::blob_of(tx, employee_id, wanted)? else {
                return Ok(Err(domain_error(DomainError::OutOfRange { field: "version".into(), detail: format!("this employee has no version {wanted}") })));
            };
            let (version, notice) = apply_instructions(tx, employee_id, &row, blob.as_deref(), Source::Restored(wanted), command_id, &meta, serde_json::json!({ "restored_from": wanted }))?;
            Ok(Ok((CommandResult::EmployeeInstructionsRestored { employee_id, version }, vec![notice])))
        })
    }

    /// The versions of an employee's instructions, newest first.
    pub fn employee_instruction_versions(&self, employee_id: EmployeeId) -> Result<EmployeeInstructionVersionsSnapshot, AppError> {
        let st = self.store.clone();
        self.store
            .read(|conn| {
                let known: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM employees WHERE id = ?1)", [employee_id.to_string()], |r| r.get(0))?;
                if !known {
                    return Ok(Err(domain_error(DomainError::EmployeeNotFound(employee_id))));
                }
                let read = |key: &str| st.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
                Ok(Ok(EmployeeInstructionVersionsSnapshot { employee_id, current_version: versions::current(conn, employee_id)?, versions: versions::list(conn, employee_id, &read)? }))
            })
            .map_err(storage_error)?
    }

    /// Pending proposals first, then the latest decisions.
    pub fn proposals(&self) -> Result<ProposalsSnapshot, AppError> {
        let st = self.store.clone();
        self.store
            .read(|conn| {
                let read = |key: &str| st.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
                Ok(ProposalsSnapshot { as_of_seq: queries::last_seq(conn)?, proposals: proposals::list(conn, &read)? })
            })
            .map_err(storage_error)
    }
}

/// Set an employee's instructions to `blob` (no instructions: `None`) as a new
/// version, and start its conversations over with them. Returns the version and
/// the `employee.config_changed` notice.
#[allow(clippy::too_many_arguments)]
fn apply_instructions(
    tx: &Tx<'_>,
    employee: EmployeeId,
    row: &employees::EmployeeRow,
    blob: Option<&str>,
    source: Source,
    command_id: CommandId,
    meta: &serde_json::Value,
    detail: serde_json::Value,
) -> Result<(u32, EventNotice), StorageError> {
    employees::update_employee(tx, employee, &row.name, &row.role, row.title.as_deref(), blob, row.version + 1)?;
    let (version, _) = versions::record(tx, employee, source)?;
    aurelius_storage::conversation_sessions::expire_for_employee(tx, employee)?;
    let mut metadata = serde_json::json!({ "instructions_version": version, "ctx": meta });
    if let (Some(m), Some(d)) = (metadata.as_object_mut(), detail.as_object()) {
        m.extend(d.clone());
    }
    let (_, notice) = append_event(
        tx,
        OrgEvent {
            aggregate_type: "employee",
            aggregate_id: employee.to_string(),
            kind: EventKind::EmployeeConfigChanged,
            project: None,
            actor: ActorKind::User,
            actor_id: None,
            correlation: command_id.to_string(),
            payload: None,
            metadata,
            dedupe: format!("cmd:{command_id}:instructions"),
        },
    )?;
    Ok((version, notice))
}

/// The decision on a proposal goes back to the proposer, in the conversation it
/// proposed from; the proposer answers it unless a run of that conversation is live.
/// `user`: the board member who decided (whose Claude login the answer runs on, outside a task).
fn answer_proposer(tx: &Tx<'_>, st: &Store, p: &proposals::ProposalRow, id: ProposalId, text: &str, user: &serde_json::Value) -> Result<Vec<EventNotice>, StorageError> {
    let mut notices = Vec::new();
    if let Some(conversation) = p.origin_conversation_id.filter(|c| conversation_is_open(tx, *c)) {
        let (message, mut posted) = thread::post_note(tx, st, conversation, Speaker::Update { kind: "status", from: "You" }, text)?;
        posted.project_id = aurelius_storage::conversations::owner_of(tx, conversation)?.1;
        notices.push(posted);
        if !live_in(tx, conversation, None)? {
            let (_, queued) = crate::run::queue::queue_run(tx, &serde_json::json!({ "origin": "proposal", "proposal_id": id, "user_id": user }), &id.to_string(), conversation, message)?;
            notices.extend(queued);
        }
    }
    Ok(notices)
}

fn conversation_is_open(conn: &Connection, conversation: ConversationId) -> bool {
    conn.query_row("SELECT EXISTS(SELECT 1 FROM conversations WHERE id = ?1 AND archived_at_ms IS NULL)", [conversation.to_string()], |r| r.get(0)).unwrap_or(false)
}

// ── propose_colleague ───────────────────────────────────────────────────

/// What the bridge tool `propose_colleague` sends.
pub(crate) struct ProposalInput {
    pub name: String,
    pub role: String,
    pub title: Option<String>,
    pub instructions: String,
    /// A colleague's name (or id); the proposer when left out.
    pub reports_to: Option<String>,
    pub reason: String,
}

/// An employee proposes a colleague during a run. The proposal waits for the user, in
/// every mode; the answer (text for the model) says so. A refusal is an answer, not an error.
pub(crate) fn propose_colleague(tx: &Tx<'_>, st: &Store, run: RunId, input: ProposalInput) -> Result<(bool, String, Vec<EventNotice>), StorageError> {
    let refuse = |why: String| Ok((false, why, vec![]));
    let Some(RunOwner { employee: Some(me), conversation, .. }) = run_owner(tx, run)? else {
        return refuse("Only an employee can propose a colleague.".into());
    };
    if employees::load_employee(tx, me)?.filter(|m| m.status == LifecycleStatus::Active).is_none() {
        return refuse("Only an active employee can propose a colleague.".into());
    }
    let open = proposals::open_count(tx, me)?;
    if open >= proposal::MAX_OPEN_PER_EMPLOYEE {
        return refuse(format!("You already have {open} proposals waiting for the user's decision; wait for the outcome before proposing another colleague."));
    }
    let read = |key: &str| st.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
    let colleagues = crate::task::tasks::colleagues(tx, &read)?;
    let boss = match input.reports_to.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
        None => me,
        Some(name) => match colleagues.iter().find(|c| c.name.trim().eq_ignore_ascii_case(name) || c.id.to_string() == name) {
            Some(c) => c.id,
            None => return refuse(format!("No active colleague called {name} in the company to report to; leave reports_to out to have the new colleague report to you.")),
        },
    };
    let fields = ProposalFields { name: input.name, role: input.role, title: input.title, instructions: input.instructions, reports_to: Some(boss) };
    let fields = match fields.validated() {
        Ok(fields) => fields,
        Err(e) => return refuse(e.to_string()),
    };
    if employees::name_taken(tx, &fields.name, None)? {
        return refuse(format!("An employee called {} already works in the company; names are unique, so propose another name.", fields.name));
    }
    let reason = match proposal::validate_reason(&input.reason) {
        Ok(reason) => reason,
        Err(e) => return refuse(e.to_string()),
    };
    let blob = st.blobs().put(fields.instructions.as_bytes(), "text/plain; charset=utf-8")?;
    blobs::register(tx, &blob)?;
    let id = ProposalId::new();
    proposals::insert(
        tx,
        proposals::NewProposal {
            id,
            proposed_by: me,
            name: &fields.name,
            role: &fields.role,
            title: fields.title.as_deref(),
            instructions_blob_id: &blob.id.to_string(),
            reports_to: Some(boss),
            reason: &reason,
            origin_conversation_id: conversation,
            kind: ProposalKind::Colleague,
            target_employee_id: None,
            base_version: None,
            evidence: Vec::new(),
        },
    )?;
    let (_, n) = append_event(
        tx,
        OrgEvent {
            aggregate_type: "proposal",
            aggregate_id: id.to_string(),
            kind: EventKind::EmployeeProposed,
            project: None,
            actor: ActorKind::Employee,
            actor_id: Some(me.to_string()),
            correlation: run.to_string(),
            payload: None,
            metadata: serde_json::json!({ "subject": fields.name, "proposed_by": me, "reason": reason, "proposal_id": id }),
            dedupe: format!("proposal:{id}:proposed"),
        },
    )?;
    Ok((
        true,
        format!(
            "Proposed {} ({}) to the user. A new colleague is only added when the user approves, whatever mode you work in; their decision will arrive in this conversation. Carry on with your work meanwhile.",
            fields.name, fields.role
        ),
        vec![n],
    ))
}

// ── propose_instructions ────────────────────────────────────────────────

/// What the bridge tool `propose_instructions` sends.
pub(crate) struct InstructionsInput {
    /// A direct report's name (or id); the proposer itself when left out.
    pub employee: Option<String>,
    pub instructions: String,
    pub reason: String,
    /// Tasks (id or title) and memories (id or words of it) the proposal rests on.
    pub evidence: Vec<String>,
}

/// An employee proposes new instructions for itself or a direct report during a
/// run (Harness² B). Only the employee's own instructions can change this way;
/// the app's rules around them stay as they are. The proposal waits for the
/// user in every mode, needs at least one task or memory as evidence, and each
/// employee's instructions have at most one open proposal.
pub(crate) fn propose_instructions(tx: &Tx<'_>, st: &Store, run: RunId, input: InstructionsInput) -> Result<(bool, String, Vec<EventNotice>), StorageError> {
    let refuse = |why: String| Ok((false, why, vec![]));
    let Some(RunOwner { employee: Some(me), project, conversation }) = run_owner(tx, run)? else {
        return refuse("Only an employee can propose instructions.".into());
    };
    if !employees::load_employee(tx, me)?.is_some_and(|m| m.status == LifecycleStatus::Active) {
        return refuse("Only an active employee can propose instructions.".into());
    }
    let read = |key: &str| st.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
    let colleagues = crate::task::tasks::colleagues(tx, &read)?;
    let target = match input.employee.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
        None => me,
        Some(name) => match colleagues.iter().find(|c| c.name.trim().eq_ignore_ascii_case(name) || c.id.to_string() == name) {
            Some(c) if c.id == me => me,
            Some(c) if c.reports_to == Some(me) => c.id,
            Some(c) => return refuse(format!("{} does not report to you; you can only propose instructions for yourself or someone who reports to you directly.", c.name)),
            None => return refuse(format!("No active colleague called {name} in the company; leave employee out to propose instructions for yourself.")),
        },
    };
    let Some(target_row) = employees::load_employee(tx, target)?.filter(|m| m.status != LifecycleStatus::Archived) else {
        return refuse("That employee no longer works here.".into());
    };
    if proposals::open_for_target(tx, target)? >= proposal::MAX_OPEN_PER_TARGET {
        let whose = if target == me { "your".to_string() } else { format!("{}'s", target_row.name) };
        return refuse(format!("A proposal for {whose} instructions already waits for the user's decision; wait for the outcome first."));
    }
    let instructions = match proposal::validate_instructions(&input.instructions) {
        Ok(text) => text,
        Err(e) => return refuse(e.to_string()),
    };
    let reason = match proposal::validate_reason(&input.reason) {
        Ok(reason) => reason,
        Err(e) => return refuse(e.to_string()),
    };
    let Ok(refs) = proposal::validate_evidence(&input.evidence) else {
        return refuse("Name at least one task (its id or title) or memory (a lesson: its id or words of it) the proposal rests on, in evidence.".into());
    };
    let mut evidence = Vec::new();
    for r in &refs {
        match find_evidence(tx, st, project, me, r)? {
            Ok(found) => evidence.push(found),
            Err(why) => return refuse(why),
        }
    }
    let current_text = tx
        .query_row("SELECT b.storage_key FROM employees m JOIN blobs b ON b.id = m.instructions_blob_id WHERE m.id = ?1", [target.to_string()], |r| r.get::<_, String>(0))
        .ok()
        .and_then(|k| read(&k))
        .unwrap_or_default();
    if aurelius_domain::memory::normalize(&current_text) == aurelius_domain::memory::normalize(&instructions) {
        return refuse("These are the instructions already in use; propose a change.".into());
    }
    let base_version = versions::current(tx, target)?;
    let blob = st.blobs().put(instructions.as_bytes(), "text/plain; charset=utf-8")?;
    blobs::register(tx, &blob)?;
    let id = ProposalId::new();
    proposals::insert(
        tx,
        proposals::NewProposal {
            id,
            proposed_by: me,
            name: &target_row.name,
            role: &target_row.role,
            title: target_row.title.as_deref(),
            instructions_blob_id: &blob.id.to_string(),
            reports_to: target_row.reports_to,
            reason: &reason,
            origin_conversation_id: conversation,
            kind: ProposalKind::Instructions,
            target_employee_id: Some(target),
            base_version: Some(base_version),
            evidence: evidence.clone(),
        },
    )?;
    let (_, n) = append_event(
        tx,
        OrgEvent {
            aggregate_type: "proposal",
            aggregate_id: id.to_string(),
            kind: EventKind::EmployeeInstructionsProposed,
            project: None,
            actor: ActorKind::Employee,
            actor_id: Some(me.to_string()),
            correlation: run.to_string(),
            payload: None,
            metadata: serde_json::json!({ "subject": target_row.name, "proposed_by": me, "target_employee_id": target, "base_version": base_version, "reason": reason, "proposal_id": id, "evidence": evidence.len() }),
            dedupe: format!("proposal:{id}:proposed"),
        },
    )?;
    let whose = if target == me { "your own".to_string() } else { format!("{}'s", target_row.name) };
    Ok((
        true,
        format!(
            "Proposed new instructions for {whose} work to the user. They only change when the user approves (possibly with edits), whatever mode you work in; the decision will arrive in this conversation. Until then the current instructions apply; carry on with your work."
        ),
        vec![n],
    ))
}

/// One reference of a proposal's evidence: a memory by its id, a task by its id
/// or title, or else the memory that best matches the words.
pub(crate) fn find_evidence(tx: &Tx<'_>, st: &Store, project: Option<ProjectId>, me: EmployeeId, needle: &str) -> Result<Result<ProposalEvidence, String>, StorageError> {
    let read = |key: &str| st.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
    let scopes = aurelius_storage::memory::employee_read_scopes(tx, me, project)?.all();
    let short = |text: &str| {
        let text = text.trim().replace('\n', " ");
        if text.chars().count() > 160 { format!("{}…", text.chars().take(159).collect::<String>()) } else { text }
    };
    let usable = |row: &aurelius_storage::memory::MemoryRow| {
        scopes.contains(&row.scope_id) && matches!(row.status, aurelius_domain::memory::MemoryStatus::Active | aurelius_domain::memory::MemoryStatus::Proposed)
    };
    if let Ok(id) = aurelius_domain::MemoryId::parse(needle.trim()) {
        if let Some(row) = aurelius_storage::memory::load(tx, id)?.filter(|r| usable(r)) {
            return Ok(Ok(ProposalEvidence { kind: "memory".into(), id: id.to_string(), label: short(&read(&row.blob_key).unwrap_or_default()) }));
        }
    }
    if let Ok(task) = crate::conversation::overview::find(tx, project, needle)? {
        let title: String = tx.query_row("SELECT title FROM tasks WHERE id = ?1", [task.to_string()], |r| r.get(0))?;
        return Ok(Ok(ProposalEvidence { kind: "task".into(), id: task.to_string(), label: title }));
    }
    let best = aurelius_storage::memory::retrieve(tx, &scopes, needle)?
        .into_iter()
        .filter(|(r, score)| *score > 0.0 && r.kind != aurelius_domain::memory::MemoryKind::ConversationSummary)
        .max_by(|a, b| a.1.total_cmp(&b.1));
    Ok(match best {
        Some((row, _)) => Ok(ProposalEvidence { kind: "memory".into(), id: row.id.to_string(), label: short(&read(&row.blob_key).unwrap_or_default()) }),
        None => Err(format!("No task or memory matches “{}”; name a task by its id or title (list_tasks) or a memory by words of it (recall).", needle.trim())),
    })
}
