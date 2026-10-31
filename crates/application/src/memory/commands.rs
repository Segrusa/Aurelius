//! Memory commands, extraction results and conversation summaries.

use crate::run::events::{Ev, emit};
use crate::{CommandContext, CommandService, Committed, domain_error, storage_error};
use aurelius_domain::{
    ActorKind, ConversationId, DomainError, EventId, EmployeeId, MemoryId, TaskId, TicketId,
    memory::{self, Accepted, ConsolidateMemory, DecideMemory, ForgetMemory, MemoryKind, MemoryScope, MemoryStatus, PinMemory, Remember, ReviseMemory, Verdict},
};
use aurelius_domain::{CommandEnvelope, CommandResult, EventNotice, AppError};
use aurelius_storage::{StorageError, Tx, blobs, memory as store, queries};
use rusqlite::OptionalExtension;

pub const EXTRACT_JOB_KIND: &str = "memory.extract";
pub const SUMMARY_JOB_KIND: &str = "memory.summarize";
pub const CONSOLIDATE_JOB_KIND: &str = "memory.consolidate";
/// Summarize an employee conversation once this many complete messages are not yet covered.
pub const SUMMARY_EVERY: u32 = 16;
/// How much of a task's report goes into memory extraction.
pub const REPORT_MAX_CHARS: usize = 6_000;

fn not_found(id: MemoryId) -> AppError {
    domain_error(DomainError::MemoryNotFound(id))
}

fn mem_event(tx: &Tx<'_>, id: MemoryId, event_type: &'static str, actor: ActorKind, correlation: String, metadata: serde_json::Value, dedupe: String) -> Result<(EventId, EventNotice), StorageError> {
    emit(tx, Ev { aggregate_type: "memory", aggregate_id: id.to_string(), event_type, actor, actor_id: None, correlation, payload: None, metadata, dedupe })
}

/// Everything an extraction job needs: sources (label → event), existing memories and the prompt text.
#[derive(Debug, Clone)]
pub struct ExtractionInput {
    pub task_id: TaskId,
    pub employee_id: EmployeeId,
    pub project_scope: String,
    pub employee_scope: String,
    /// (label, event id, description) given to the model.
    pub sources: Vec<(String, EventId, String)>,
    pub existing: Vec<memory::Existing>,
    /// What went wrong or was corrected (label S1..S8, event, text); empty: no lessons.
    pub signals: Vec<(String, EventId, String)>,
    /// Lessons already in the employee's and the project's memory.
    pub lessons: Vec<memory::ExistingLesson>,
    pub config: crate::run::connections::ResolvedRunConfig,
    pub employee_system: String,
    /// The ticket whose Done this extraction follows (MU1-18): it also writes the ticket's one summary.
    pub ticket: Option<TicketRef>,
}

/// A ticket in Done, as its summary names it.
#[derive(Debug, Clone)]
pub struct TicketRef {
    pub id: TicketId,
    pub number: u32,
    pub title: String,
}

/// A conversation to summarize: the turns not yet covered.
#[derive(Debug, Clone)]
pub struct SummaryInput {
    pub conversation_id: ConversationId,
    pub employee_id: EmployeeId,
    pub employee_scope: String,
    pub previous: Option<String>,
    /// (ordinal, role label, text, source event)
    pub turns: Vec<(u32, String, String, EventId)>,
    pub config: crate::run::connections::ResolvedRunConfig,
}

impl CommandService {
    /// Run configuration for memory work of an employee (same connection as the employee).
    pub fn resolve_for_employee(&self, employee: EmployeeId, _purpose: aurelius_domain::connection::Purpose) -> Result<crate::run::connections::ResolvedRunConfig, AppError> {
        self.store
            .write(move |tx| {
                let conversation: String = tx.query_row("SELECT id FROM conversations WHERE employee_id = ?1 AND kind = 'employee' ORDER BY created_at_ms, id LIMIT 1", [employee.to_string()], |r| r.get(0))?;
                let conversation = ConversationId::parse(&conversation).map_err(|e| StorageError::Rejected(e.to_string()))?;
                crate::run::connections::resolve(tx, conversation, aurelius_domain::connection::Purpose::Employee)
            })
            .map_err(storage_error)
    }

    pub(crate) fn remember(&self, ctx: &CommandContext, envelope: &CommandEnvelope, hash: String, cmd: &Remember) -> Result<Committed, AppError> {
        let content = memory::validate_remember(cmd).map_err(domain_error)?;
        let blob = self.store.blobs().put(content.as_bytes(), "text/plain; charset=utf-8").map_err(storage_error)?;
        let cmd = cmd.clone();
        let id = MemoryId::new();
        let command_id = envelope.command_id;
        let meta = ctx.metadata();
        self.commit(command_id, hash, move |tx| {
            if cmd.scope == MemoryScope::Global {
                aurelius_storage::events::ensure_global_scope(tx)?;
            }
            let Some(scope) = store::scope_id(tx, cmd.scope)? else {
                return Ok(Err(domain_error(DomainError::Memory("This scope does not exist.".into()))));
            };
            blobs::register(tx, &blob)?;
            let (event_id, n) = mem_event(tx, id, "memory.remembered", ActorKind::User, command_id.to_string(), serde_json::json!({ "kind": cmd.kind.as_str(), "ctx": meta }), format!("cmd:{command_id}:0"))?;
            store::insert(tx, store::NewMemory {
                id,
                scope_id: &scope,
                kind: cmd.kind,
                blob_id: &blob.id.to_string(),
                text: &content,
                status: MemoryStatus::Active,
                confidence: 1.0,
                importance: if cmd.pinned { 0.9 } else { 0.6 },
                pinned: cmd.pinned,
                supersedes: None,
                conflicts_with: None,
                decided_by: Some("user"),
                locator: serde_json::json!({}),
                sources: vec![(event_id, serde_json::json!({ "authored": true }))],
                extraction_version: "user",
            })?;
            Ok(Ok((CommandResult::MemoryStored { memory_id: id }, vec![n])))
        })
    }

    pub(crate) fn decide_memory(&self, envelope: &CommandEnvelope, hash: String, cmd: &DecideMemory) -> Result<Committed, AppError> {
        let store_ref = self.store.clone();
        let cmd = cmd.clone();
        let command_id = envelope.command_id;
        self.commit(command_id, hash, move |tx| {
            let Some(row) = store::load(tx, cmd.memory_id)? else { return Ok(Err(not_found(cmd.memory_id))) };
            if row.status != MemoryStatus::Proposed {
                return Ok(Err(domain_error(DomainError::Memory("This proposal was already handled.".into()))));
            }
            let mut notices = Vec::new();
            let status = if cmd.accept { MemoryStatus::Active } else { MemoryStatus::Rejected };
            store::set_status(tx, cmd.memory_id, status, Some("user"))?;
            if cmd.accept {
                let text = store_ref.blobs().read(&row.blob_key).ok().and_then(|b| String::from_utf8(b).ok()).unwrap_or_default();
                store::index(tx, cmd.memory_id, &text)?;
                // Accepting a conflicting proposal supersedes the old memory (never erased).
                let conflicting: Option<String> = tx.query_row("SELECT conflicts_with_id FROM memory_items WHERE id = ?1", [cmd.memory_id.to_string()], |r| r.get(0))?;
                if let Some(old) = conflicting.and_then(|o| MemoryId::parse(&o).ok()) {
                    if store::load(tx, old)?.is_some_and(|o| o.status == MemoryStatus::Active) {
                        store::set_status(tx, old, MemoryStatus::Superseded, Some("user"))?;
                        store::deindex(tx, old)?;
                        tx.execute("UPDATE memory_items SET supersedes_id = ?2 WHERE id = ?1", [cmd.memory_id.to_string(), old.to_string()])?;
                        notices.push(mem_event(tx, old, "memory.superseded", ActorKind::User, command_id.to_string(), serde_json::json!({ "by": cmd.memory_id }), format!("cmd:{command_id}:1"))?.1);
                    }
                }
                // An accepted consolidation point replaces the memories it merges.
                let merged: Vec<MemoryId> = row.locator["consolidates"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().and_then(|s| MemoryId::parse(s).ok())).collect()).unwrap_or_default();
                notices.extend(supersede_all(tx, &merged, Some(cmd.memory_id), &command_id.to_string())?);
            }
            let event_type = if cmd.accept { "memory.accepted" } else { "memory.rejected" };
            notices.insert(0, mem_event(tx, cmd.memory_id, event_type, ActorKind::User, command_id.to_string(), serde_json::json!({}), format!("cmd:{command_id}:0"))?.1);
            Ok(Ok((CommandResult::MemoryDecided { memory_id: cmd.memory_id, status }, notices)))
        })
    }

    pub(crate) fn revise_memory(&self, envelope: &CommandEnvelope, hash: String, cmd: &ReviseMemory) -> Result<Committed, AppError> {
        let content = memory::validate_content(&cmd.content).map_err(domain_error)?;
        let blob = self.store.blobs().put(content.as_bytes(), "text/plain; charset=utf-8").map_err(storage_error)?;
        let old_id = cmd.memory_id;
        let id = MemoryId::new();
        let command_id = envelope.command_id;
        self.commit(command_id, hash, move |tx| {
            let Some(old) = store::load(tx, old_id)? else { return Ok(Err(not_found(old_id))) };
            if old.status != MemoryStatus::Active {
                return Ok(Err(domain_error(DomainError::Memory("Only active memories can be revised.".into()))));
            }
            blobs::register(tx, &blob)?;
            let (event_id, n) = mem_event(tx, id, "memory.revised", ActorKind::User, command_id.to_string(), serde_json::json!({ "supersedes": old_id }), format!("cmd:{command_id}:0"))?;
            // Provenance survives: the new version keeps the old sources plus this revision.
            let mut sources: Vec<(EventId, serde_json::Value)> = {
                let mut stmt = tx.prepare("SELECT event_id, locator_json FROM memory_sources WHERE memory_id = ?1")?;
                let rows = stmt.query_map([old_id.to_string()], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
                rows.filter_map(Result::ok).filter_map(|(e, l)| Some((EventId::parse(&e).ok()?, serde_json::from_str(&l).unwrap_or_default()))).collect()
            };
            sources.push((event_id, serde_json::json!({ "revision_of": old_id })));
            store::set_status(tx, old_id, MemoryStatus::Superseded, Some("user"))?;
            store::deindex(tx, old_id)?;
            store::insert(tx, store::NewMemory {
                id,
                scope_id: &old.scope_id,
                kind: old.kind,
                blob_id: &blob.id.to_string(),
                text: &content,
                status: MemoryStatus::Active,
                confidence: 1.0,
                importance: old.importance,
                pinned: old.pinned,
                supersedes: Some(old_id),
                conflicts_with: None,
                decided_by: Some("user"),
                locator: old.locator.clone(),
                sources,
                extraction_version: "user",
            })?;
            Ok(Ok((CommandResult::MemoryStored { memory_id: id }, vec![n])))
        })
    }

    pub(crate) fn forget_memory(&self, envelope: &CommandEnvelope, hash: String, cmd: &ForgetMemory) -> Result<Committed, AppError> {
        let id = cmd.memory_id;
        let command_id = envelope.command_id;
        let key = self.store.read(|c| store::load(c, id)).map_err(storage_error)?.map(|r| r.blob_key);
        let committed = self.commit(command_id, hash, move |tx| {
            let Some(row) = store::load(tx, id)? else { return Ok(Err(not_found(id))) };
            if row.status == MemoryStatus::Forgotten {
                return Ok(Ok((CommandResult::MemoryForgotten { memory_id: id }, vec![])));
            }
            store::set_status(tx, id, MemoryStatus::Forgotten, Some("user"))?;
            store::deindex(tx, id)?;
            let (_, n) = mem_event(tx, id, "memory.forgotten", ActorKind::User, command_id.to_string(), serde_json::json!({}), format!("cmd:{command_id}:0"))?;
            Ok(Ok((CommandResult::MemoryForgotten { memory_id: id }, vec![n])))
        })?;
        // After the commit: erase the content itself.
        if let Some(key) = key {
            if let Err(err) = self.store.blobs().erase(&key) {
                tracing::warn!(error = %err, "could not erase forgotten memory content");
            }
            // ...and from every backup, so a restore can't bring it back.
            if let Err(err) = aurelius_storage::backup::scrub(self.store.root(), &[key], &[id.to_string()], &[]) {
                tracing::warn!(error = %err, "could not scrub forgotten memory from backups");
            }
        }
        Ok(committed)
    }

    pub(crate) fn pin_memory(&self, envelope: &CommandEnvelope, hash: String, cmd: &PinMemory) -> Result<Committed, AppError> {
        let cmd = cmd.clone();
        let command_id = envelope.command_id;
        self.commit(command_id, hash, move |tx| {
            let Some(row) = store::load(tx, cmd.memory_id)? else { return Ok(Err(not_found(cmd.memory_id))) };
            if matches!(row.status, MemoryStatus::Forgotten | MemoryStatus::Rejected) {
                return Ok(Err(domain_error(DomainError::Memory("This memory is no longer active.".into()))));
            }
            store::set_pinned(tx, cmd.memory_id, cmd.pinned)?;
            let (_, n) = mem_event(tx, cmd.memory_id, "memory.pinned", ActorKind::User, command_id.to_string(), serde_json::json!({ "pinned": cmd.pinned }), format!("cmd:{command_id}:0"))?;
            Ok(Ok((CommandResult::MemoryDecided { memory_id: cmd.memory_id, status: row.status }, vec![n])))
        })
    }

    /// Inputs for extracting memories from a finished task; `None` when the
    /// task has no employee (nothing to remember for).
    /// With `ticket`: its summary too, if the ticket is in Done and has none yet.
    pub fn extraction_input(&self, task_id: TaskId, ticket: Option<TicketId>) -> Result<Option<ExtractionInput>, AppError> {
        let ctx = self.task_context(task_id)?;
        let Some(ctx) = ctx else { return Ok(None) };
        let Some(employee_id) = ctx.employee_id else { return Ok(None) };
        let store_ref = self.store.clone();
        let mut config = self.resolve_for_employee(employee_id, aurelius_domain::connection::Purpose::Memory)?;
        // On the task's own login: a ticket's summary after Done on the ticket's (MU3-10 AC1).
        config.login_user = self.store.read(|c| crate::run::login::of_task(c, task_id)).map_err(storage_error)?;
        self.store
            .read(|conn| {
                let read = |key: &str| store_ref.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
                // What a task teaches goes to the employee or the task's project.
                let scopes = store::employee_read_scopes(conn, employee_id, Some(ctx.project_id))?;
                let (Some(employee_scope), Some(project_scope)) = (scopes.own.clone(), scopes.project.clone()) else { return Ok(None) };
                // Sources: the assignment and the outcome (the evidence), labelled E1..En.
                let mut stmt = conn.prepare(
                    "SELECT id, type, metadata_json FROM events WHERE ((aggregate_type = 'task' AND aggregate_id = ?1) OR correlation_id = ?1)
                     AND type IN ('task.created','task.succeeded','task.failed') ORDER BY seq",
                )?;
                let rows = stmt.query_map([task_id.to_string()], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))?;
                // The task's report (its result), so what it found can be recalled later.
                let report: Option<String> = conn
                    .query_row("SELECT b.storage_key FROM tasks t JOIN blobs b ON b.id = t.result_blob_id WHERE t.id = ?1", [task_id.to_string()], |r| r.get::<_, String>(0))
                    .ok()
                    .and_then(|key| read(&key))
                    .map(|r| r.trim().chars().take(REPORT_MAX_CHARS).collect())
                    .filter(|r: &String| !r.is_empty());
                let mut sources = Vec::new();
                for row in rows {
                    let (id, event_type, metadata) = row?;
                    let what = match event_type.as_str() {
                        "task.created" => format!("Assignment: {} — {}", ctx.title, serde_json::Value::from(ctx.goal.as_str())),
                        "task.succeeded" => match &report {
                            Some(report) => format!("Task completed: {}\nReport (JSON-quoted):\n{}", ctx.title, serde_json::Value::from(report.as_str())),
                            None => format!("Task completed: {}", ctx.title),
                        },
                        "task.failed" => {
                            let reason = serde_json::from_str::<serde_json::Value>(&metadata).ok().and_then(|m| m["reason"].as_str().map(str::to_owned)).unwrap_or_default();
                            if reason.is_empty() { format!("Task failed: {}", ctx.title) } else { format!("Task failed: {} ({reason})", ctx.title) }
                        }
                        _ => continue,
                    };
                    if let Ok(event) = EventId::parse(&id) {
                        sources.push((format!("E{}", sources.len() + 1), event, what));
                    }
                }
                let scopes = [employee_scope.clone(), project_scope.clone()];
                let existing = store::existing(conn, &scopes, &read)?;
                // The gate: lessons only when something went wrong or was corrected.
                let signals = memory::gate_signals(crate::memory::signals::task_signals(conn, task_id, &read)?);
                let lessons = store::lessons(conn, &scopes, &read)?
                    .into_iter()
                    .map(|(row, content)| memory::ExistingLesson { id: row.id, content, employee_scope: row.scope_id == employee_scope })
                    .collect();
                let ticket = match ticket {
                    Some(id) => ticket_to_summarize(conn, id, &project_scope)?,
                    None => None,
                };
                // The ticket's summary rests on the whole ticket (MU2-12 B4): its fields, its
                // discussion and the reports of the tasks linked to it, besides the main task.
                if let Some(t) = &ticket {
                    for (event, what) in ticket_sources(conn, t.id, task_id, &read)? {
                        sources.push((format!("E{}", sources.len() + 1), event, what));
                    }
                }
                Ok(Some(ExtractionInput { task_id, employee_id, project_scope, employee_scope, sources, existing, signals, lessons, config, employee_system: ctx.employee_system.clone(), ticket }))
            })
            .map_err(storage_error)
    }

    /// Store validated candidates and close the job, atomically and fenced.
    /// `ticket_summary`: the model's summary of the ticket, if it gave one (only with `input.ticket`).
    pub fn record_extraction(&self, job_id: &str, epoch: i64, input: &ExtractionInput, verdicts: Vec<Verdict>, lessons: Vec<memory::LessonVerdict>, ticket_summary: Option<&str>) -> Result<Vec<EventNotice>, AppError> {
        let kept: Vec<Accepted> = verdicts.iter().filter_map(|v| if let Verdict::Keep(a) = v { Some(a.clone()) } else { None }).collect();
        let ticket_text = input.ticket.as_ref().map(|t| {
            let for_project: Vec<&str> = kept.iter().filter(|a| !a.employee_scope).map(|a| a.content.as_str()).collect();
            memory::ticket_summary(t.number, &t.title, ticket_summary, &for_project)
        });
        let ticket_blob = match &ticket_text {
            Some(text) => Some(self.store.blobs().put(text.as_bytes(), "text/plain; charset=utf-8").map_err(storage_error)?),
            None => None,
        };
        let dropped = verdicts.len() - kept.len();
        let mut prepared = Vec::new();
        for a in &kept {
            prepared.push(self.store.blobs().put(a.content.as_bytes(), "text/plain; charset=utf-8").map_err(storage_error)?);
        }
        let mut new_lessons = Vec::new();
        for v in &lessons {
            if let memory::LessonVerdict::New(l) = v {
                new_lessons.push((l.clone(), self.store.blobs().put(l.content.as_bytes(), "text/plain; charset=utf-8").map_err(storage_error)?));
            }
        }
        let reinforced: Vec<(MemoryId, memory::Lesson)> = lessons.iter().filter_map(|v| if let memory::LessonVerdict::Reinforce { existing, lesson } = v { Some((*existing, lesson.clone())) } else { None }).collect();
        let lessons_dropped = lessons.iter().filter(|v| matches!(v, memory::LessonVerdict::Drop { .. })).count();
        let input = input.clone();
        let job = job_id.to_owned();
        self.store
            .write(move |tx| {
                if !queries::holds_lease(tx, &job, epoch)? {
                    return Ok(Vec::new());
                }
                let mut notices = Vec::new();
                let mut last_event: Vec<(String, EventId)> = Vec::new();
                // The evidence of a lesson: its signals with their events.
                let evidence_of = |l: &memory::Lesson| -> (Vec<memory::LessonEvidence>, Vec<(EventId, serde_json::Value)>) {
                    let mut shown = Vec::new();
                    let mut sources = Vec::new();
                    for label in &l.evidence {
                        if let Some((_, event, text)) = input.signals.iter().find(|s| &s.0 == label) {
                            shown.push(memory::LessonEvidence { label: label.clone(), text: text.clone() });
                            sources.push((*event, serde_json::json!({ "label": label, "signal": true, "task_id": input.task_id })));
                        }
                    }
                    (shown, sources)
                };
                for (i, (l, blob)) in new_lessons.iter().enumerate() {
                    let (shown, sources) = evidence_of(l);
                    if sources.is_empty() {
                        continue;
                    }
                    blobs::register(tx, blob)?;
                    let id = MemoryId::new();
                    let scope = if l.employee_scope { &input.employee_scope } else { &input.project_scope };
                    // A lesson for the employee itself counts at once; one for the whole project needs the user.
                    let status = if l.employee_scope { MemoryStatus::Active } else { MemoryStatus::Proposed };
                    let event_type = if status == MemoryStatus::Active { "memory.extracted" } else { "memory.proposed" };
                    let (event_id, n) = mem_event(
                        tx,
                        id,
                        event_type,
                        ActorKind::Employee,
                        input.task_id.to_string(),
                        serde_json::json!({ "kind": MemoryKind::Episodic.as_str(), "lesson": true, "task_id": input.task_id }),
                        format!("memory:{}:lesson:{i}:{event_type}", input.task_id),
                    )?;
                    let locator = memory::LessonLocator { applies_when: l.applies_when.clone(), evidence: shown, reinforced: 0, tasks: vec![input.task_id] };
                    let mut all_sources = vec![(event_id, serde_json::json!({ "lesson": true, "task_id": input.task_id }))];
                    all_sources.extend(sources);
                    store::insert(tx, store::NewMemory {
                        id,
                        scope_id: scope,
                        kind: MemoryKind::Episodic,
                        blob_id: &blob.id.to_string(),
                        text: &l.content,
                        status,
                        confidence: 0.8,
                        importance: 0.7,
                        pinned: false,
                        supersedes: None,
                        conflicts_with: None,
                        decided_by: (status == MemoryStatus::Active).then_some("policy"),
                        locator: serde_json::json!({ "task_id": input.task_id, "lesson": locator }),
                        sources: all_sources,
                        extraction_version: memory::EXTRACTION_VERSION,
                    })?;
                    notices.push(n);
                }
                for (existing, l) in &reinforced {
                    let Some(row) = store::load(tx, *existing)? else { continue };
                    if !matches!(row.status, MemoryStatus::Active | MemoryStatus::Proposed) {
                        continue;
                    }
                    let mut lesson: memory::LessonLocator = serde_json::from_value(row.locator["lesson"].clone()).unwrap_or_default();
                    // The same task does not confirm its own lesson twice.
                    if lesson.tasks.contains(&input.task_id) {
                        continue;
                    }
                    let (shown, sources) = evidence_of(l);
                    lesson.reinforced += 1;
                    lesson.tasks.push(input.task_id);
                    lesson.evidence.extend(shown);
                    let cut = lesson.evidence.len().saturating_sub(memory::MAX_SIGNALS);
                    lesson.evidence.drain(..cut);
                    let mut locator = row.locator.clone();
                    locator["lesson"] = serde_json::to_value(&lesson).unwrap_or_default();
                    let (event_id, n) = mem_event(
                        tx,
                        *existing,
                        "memory.reinforced",
                        ActorKind::Employee,
                        input.task_id.to_string(),
                        serde_json::json!({ "lesson": true, "task_id": input.task_id, "reinforced": lesson.reinforced }),
                        format!("memory:{}:reinforce:{existing}", input.task_id),
                    )?;
                    let mut all_sources = vec![(event_id, serde_json::json!({ "reinforced": lesson.reinforced, "task_id": input.task_id }))];
                    all_sources.extend(sources);
                    store::reinforce(tx, *existing, &locator, row.importance + 0.1, &all_sources)?;
                    notices.push(n);
                }
                if lessons_dropped > 0 {
                    tracing::info!(task = %input.task_id, lessons_dropped, "lessons dropped by validation");
                }
                for (i, (a, blob)) in kept.iter().zip(&prepared).enumerate() {
                    let Some((_, source, _)) = input.sources.iter().find(|s| s.0 == a.source_label) else { continue };
                    blobs::register(tx, blob)?;
                    let id = MemoryId::new();
                    let scope = if a.employee_scope { &input.employee_scope } else { &input.project_scope };
                    let event_type = if a.status == MemoryStatus::Active { "memory.extracted" } else { "memory.proposed" };
                    let (event_id, n) = mem_event(
                        tx,
                        id,
                        event_type,
                        ActorKind::Employee,
                        input.task_id.to_string(),
                        serde_json::json!({ "kind": a.kind.as_str(), "task_id": input.task_id, "conflicts_with": a.conflicts_with }),
                        format!("memory:{}:{i}:{event_type}", input.task_id),
                    )?;
                    store::insert(tx, store::NewMemory {
                        id,
                        scope_id: scope,
                        kind: a.kind,
                        blob_id: &blob.id.to_string(),
                        text: &a.content,
                        status: a.status,
                        confidence: a.confidence,
                        importance: a.importance,
                        pinned: false,
                        supersedes: None,
                        conflicts_with: a.conflicts_with,
                        decided_by: (a.status == MemoryStatus::Active).then_some("policy"),
                        locator: serde_json::json!({ "task_id": input.task_id }),
                        sources: vec![(*source, serde_json::json!({ "label": a.source_label }))],
                        extraction_version: memory::EXTRACTION_VERSION,
                    })?;
                    notices.push(n);
                    last_event.push((scope.clone(), event_id));
                }
                // The ticket's one summary, in its project's memory (MU1-18); not twice,
                // also not when the job runs again or the ticket left Done meanwhile.
                if let (Some(ticket), Some(blob)) = (&input.ticket, &ticket_blob) {
                    let completed = input.sources.iter().rev().find(|s| s.2.starts_with("Task completed")).or(input.sources.last());
                    if let Some((label, source, _)) = completed {
                        if ticket_to_summarize(tx, ticket.id, &input.project_scope)?.is_some() {
                            blobs::register(tx, blob)?;
                            let id = MemoryId::new();
                            let (_, n) = mem_event(
                                tx,
                                id,
                                "memory.extracted",
                                ActorKind::Employee,
                                input.task_id.to_string(),
                                serde_json::json!({ "kind": MemoryKind::ProjectKnowledge.as_str(), "task_id": input.task_id, "ticket_id": ticket.id, "ticket_number": ticket.number }),
                                format!("memory:{}:ticket:{}", input.task_id, ticket.id),
                            )?;
                            store::insert(tx, store::NewMemory {
                                id,
                                scope_id: &input.project_scope,
                                kind: MemoryKind::ProjectKnowledge,
                                blob_id: &blob.id.to_string(),
                                text: ticket_text.as_deref().unwrap_or_default(),
                                status: MemoryStatus::Active,
                                confidence: 0.8,
                                importance: 0.7,
                                pinned: false,
                                supersedes: None,
                                conflicts_with: None,
                                decided_by: Some("policy"),
                                locator: serde_json::json!({ "task_id": input.task_id, "ticket_id": ticket.id, "ticket_number": ticket.number }),
                                sources: vec![(*source, serde_json::json!({ "label": label, "ticket_id": ticket.id }))],
                                extraction_version: memory::EXTRACTION_VERSION,
                            })?;
                            notices.push(n);
                            // AC2: the discussion says so (a system line, also on Done).
                            notices.extend(crate::conversation::discussion::ticket_line(tx, ticket.id, aurelius_domain::views::SystemLine::new(aurelius_domain::views::SystemLineKind::Summary), "")?);
                        }
                    }
                }
                for (scope, event) in last_event {
                    schedule_consolidation(tx, &scope, event, false)?;
                }
                if dropped > 0 {
                    tracing::info!(task = %input.task_id, dropped, "extraction candidates dropped by validation");
                }
                queries::complete_job(tx, &job, epoch)?;
                Ok(notices)
            })
            .map_err(storage_error)
    }

    /// The extraction after a ticket's Done failed (MU2-12 AC4: quota, a run that
    /// failed, nothing to summarize): the job ends (no retry by itself: a board
    /// member tries again with `ticket.retry_summary`) and, while the ticket has no
    /// summary, its discussion says so in neutral words (`SUMMARY_FAILED`): `why` (a
    /// runtime error, maybe with a path on the server) only goes to the server's log
    /// (the daemon logs it with the job, `memory::summary_failed`).
    /// The ticket stays on Done. Fenced like a result.
    pub fn ticket_summary_failed(&self, job_id: &str, epoch: i64, ticket: TicketId, _why: &str) -> Result<Vec<EventNotice>, AppError> {
        let job = job_id.to_owned();
        self.store
            .write(move |tx| {
                if !queries::kill_job(tx, &job, epoch, "memory_failed")? || has_ticket_summary(tx, ticket)? {
                    return Ok(Vec::new());
                }
                let line = aurelius_domain::views::SystemLine::new(aurelius_domain::views::SystemLineKind::SummaryFailed);
                crate::conversation::discussion::ticket_line(tx, ticket, line, SUMMARY_FAILED)
            })
            .map_err(storage_error)
    }

    /// Turns of an employee conversation not yet covered by a summary.
    /// `source`: the run that scheduled it (`maybe_schedule_summary`); none (an older job): it does not run.
    pub fn summary_input(&self, conversation_id: ConversationId, source: Option<aurelius_domain::RunId>) -> Result<Option<SummaryInput>, AppError> {
        let store_ref = self.store.clone();
        let employee = self
            .store
            .read(|c| Ok(c.query_row("SELECT employee_id FROM conversations WHERE id = ?1", [conversation_id.to_string()], |r| r.get::<_, Option<String>>(0)).ok().flatten()))
            .map_err(storage_error)?;
        let Some(employee_id) = employee.and_then(|m| EmployeeId::parse(&m).ok()) else { return Ok(None) };
        let mut config = self.resolve_for_employee(employee_id, aurelius_domain::connection::Purpose::Memory)?;
        // On the login of what it summarizes: the task's, else the login of the run
        // that scheduled it, never of another run of a shared conversation (B1-1).
        config.login_user = self
            .store
            .read(|c| {
                let task: Option<String> = c.query_row("SELECT id FROM tasks WHERE conversation_id = ?1", [conversation_id.to_string()], |r| r.get(0)).optional()?;
                match task.and_then(|t| TaskId::parse(&t).ok()) {
                    Some(task) => crate::run::login::of_task(c, task),
                    None => source.map_or(Ok(None), |run| crate::run::login::of_run(c, run)),
                }
            })
            .map_err(storage_error)?;
        self.store
            .read(|conn| {
                let read = |key: &str| store_ref.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
                let Some(employee_scope) = store::scope_id(conn, MemoryScope::Employee(employee_id))? else { return Ok(None) };
                let (previous, through) = match store::latest_summary(conn, conversation_id)? {
                    Some((row, through)) => (read(&row.blob_key), through),
                    None => (None, 0),
                };
                let mut stmt = conn.prepare(
                    "SELECT m.ordinal, m.role, b.storage_key, m.source_event_id FROM messages m JOIN blobs b ON b.id = m.content_blob_id
                     WHERE m.conversation_id = ?1 AND m.status = 'complete' AND m.ordinal > ?2 ORDER BY m.ordinal",
                )?;
                let turns = stmt
                    .query_map(rusqlite::params![conversation_id.to_string(), through], |r| Ok((r.get::<_, u32>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?)))?
                    .filter_map(Result::ok)
                    .filter_map(|(o, role, key, ev)| Some((o, match role.as_str() { "user" => "User".to_string(), "tool" => "Task update".to_string(), _ => "Employee".to_string() }, read(&key)?, EventId::parse(&ev).ok()?)))
                    .collect::<Vec<_>>();
                Ok(Some(SummaryInput { conversation_id, employee_id, employee_scope, previous, turns, config }))
            })
            .map_err(storage_error)
    }

    /// Store a conversation summary with exact coverage; the previous summary is superseded.
    pub fn record_summary(&self, job_id: &str, epoch: i64, input: &SummaryInput, text: &str) -> Result<Vec<EventNotice>, AppError> {
        let Some(last) = input.turns.last().cloned() else { return Ok(Vec::new()) };
        let text = text.trim().to_owned();
        let blob = self.store.blobs().put(text.as_bytes(), "text/plain; charset=utf-8").map_err(storage_error)?;
        let input = input.clone();
        let job = job_id.to_owned();
        self.store
            .write(move |tx| {
                if !queries::holds_lease(tx, &job, epoch)? {
                    return Ok(Vec::new());
                }
                let previous = store::latest_summary(tx, input.conversation_id)?;
                let id = MemoryId::new();
                blobs::register(tx, &blob)?;
                let (_, n) = mem_event(
                    tx,
                    id,
                    "memory.summarized",
                    ActorKind::Employee,
                    input.conversation_id.to_string(),
                    serde_json::json!({ "covers_through_ordinal": last.0 }),
                    format!("summary:{}:{}", input.conversation_id, last.0),
                )?;
                if let Some((old, _)) = &previous {
                    store::set_status(tx, old.id, MemoryStatus::Superseded, Some("policy"))?;
                    store::deindex(tx, old.id)?;
                }
                let first = input.turns.first().map(|t| t.0).unwrap_or(last.0);
                store::insert(tx, store::NewMemory {
                    id,
                    scope_id: &input.employee_scope,
                    kind: MemoryKind::ConversationSummary,
                    blob_id: &blob.id.to_string(),
                    text: &text,
                    status: MemoryStatus::Active,
                    confidence: 0.7,
                    importance: 0.5,
                    pinned: false,
                    supersedes: previous.as_ref().map(|(o, _)| o.id),
                    conflicts_with: None,
                    decided_by: Some("policy"),
                    locator: serde_json::json!({ "conversation_id": input.conversation_id, "covers_from_ordinal": first, "covers_through_ordinal": last.0, "covers_through_event": last.3 }),
                    sources: vec![(input.turns[0].3, serde_json::json!({ "ordinal": first })), (last.3, serde_json::json!({ "ordinal": last.0 }))],
                    extraction_version: memory::EXTRACTION_VERSION,
                })?;
                queries::complete_job(tx, &job, epoch)?;
                Ok(vec![n])
            })
            .map_err(storage_error)
    }
}

impl CommandService {
    /// An employee's memory tool during one of its runs (`remember` / `recall`).
    /// Returns (ok, text for the model, notices). A run without an employee has no
    /// memory of its own, except the project assistant's.
    pub fn employee_memory(&self, run: aurelius_domain::RunId, op: aurelius_domain::memory::EmployeeMemoryOp) -> Result<(bool, String, Vec<EventNotice>), AppError> {
        use aurelius_domain::memory::EmployeeMemoryOp as Op;
        let owner = self.store.read(|c| crate::task::task_runs::run_owner(c, run)).map_err(storage_error)?;
        // An employee has its own memory and (in a task) its project's; the assistant of a project conversation reads and writes the
        // project's (never an employee's private one).
        let (employee, project): (Option<EmployeeId>, Option<aurelius_domain::ProjectId>) = match owner {
            Some(o) if o.employee.is_some() || o.project.is_some() => (o.employee, o.project),
            _ => return Ok((false, "Only an employee has a memory.".into(), vec![])),
        };
        let scopes_of = move |conn: &rusqlite::Connection| -> Result<store::ReadScopes, StorageError> {
            match (employee, project) {
                (Some(m), project) => store::employee_read_scopes(conn, m, project),
                (None, Some(p)) => store::project_read_scopes(conn, p),
                (None, None) => Ok(store::ReadScopes::default()),
            }
        };
        let store_ref = self.store.clone();
        match op {
            Op::Recall { query } => {
                let text = self
                    .store
                    .read(|conn| {
                        let read = |key: &str| store_ref.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
                        let read_scopes = scopes_of(conn)?;
                        let mut hits = store::retrieve(conn, &read_scopes.all(), &query)?;
                        hits.retain(|(r, score)| *score > 0.0 && r.kind != MemoryKind::ConversationSummary);
                        hits.sort_by(|a, b| b.1.total_cmp(&a.1));
                        let lines: Vec<String> = hits
                            .iter()
                            .take(12)
                            .filter_map(|(r, _)| {
                                let scope = match read_scopes.label(&r.scope_id) {
                                    "employee" => "yours",
                                    other => other,
                                };
                                // A lesson says how often later tasks confirmed it.
                                let confirmed = r.locator["lesson"]["reinforced"].as_u64().filter(|n| *n > 0).map(|n| format!(" (confirmed {n}×)")).unwrap_or_default();
                                Some(format!("- [{} · {scope}] {}{confirmed}", crate::memory::context::kind_label(r.kind, &r.locator), read(&r.blob_key)?.trim()))
                            })
                            .collect();
                        Ok(if lines.is_empty() { format!("Nothing found for \"{query}\".") } else { lines.join("\n") })
                    })
                    .map_err(storage_error)?;
                Ok((true, text, vec![]))
            }
            Op::Remember { kind, scope, content } => {
                let Some(kind) = memory::employee_note_kind(&kind) else {
                    return Ok((false, "kind must be decision, project_knowledge or episodic.".into(), vec![]));
                };
                let content = match memory::validate_content(&content) {
                    Ok(c) => c,
                    Err(e) => return Ok((false, e.to_string(), vec![])),
                };
                if memory::looks_secret(&content) {
                    return Ok((false, "This looks like a secret; secrets are never stored in memory. Keep it in the vault and remember only its name (the environment variable, e.g. $NAME).".into(), vec![]));
                }
                let wanted = scope.trim().to_lowercase();
                let blob = self.store.blobs().put(content.as_bytes(), "text/plain; charset=utf-8").map_err(storage_error)?;
                let id = MemoryId::new();
                self.store
                    .write(move |tx| {
                        let read = |key: &str| store_ref.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
                        let scopes = scopes_of(tx)?;
                        let target = if employee.is_some() {
                            let Some(own) = scopes.own.clone() else {
                                return Ok((false, "This employee has no memory scope.".to_string(), vec![]));
                            };
                            match wanted.as_str() {
                                "employee" => own,
                                "project" => match scopes.project.clone() {
                                    Some(project) => project,
                                    None => return Ok((false, "You are not in a project here; remember with scope employee.".to_string(), vec![])),
                                },
                                // Decisions and knowledge help colleagues too (the project's);
                                // episodes are one's own.
                                _ if kind != MemoryKind::Episodic => scopes.project.clone().unwrap_or(own),
                                _ => own,
                            }
                        } else {
                            // The project assistant has no memory of its own: everything goes to the project.
                            let Some(project) = scopes.project.clone() else {
                                return Ok((false, "This project has no memory scope.".to_string(), vec![]));
                            };
                            project
                        };
                        let existing: Vec<memory::ExistingNote> = {
                            let mut stmt = tx.prepare(
                                "SELECT m.id, m.kind, b.storage_key, COALESCE(m.decided_by, '') FROM memory_items m JOIN blobs b ON b.id = m.content_blob_id
                                 WHERE m.scope_id = ?1 AND m.status IN ('active','proposed')",
                            )?;
                            let rows = stmt.query_map([&target], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?)))?;
                            rows.filter_map(Result::ok)
                                .filter_map(|(id, k, key, by)| {
                                    Some(memory::ExistingNote { id: MemoryId::parse(&id).ok()?, kind: MemoryKind::parse(&k)?, content: read(&key)?, by_user: by == "user" })
                                })
                                .collect()
                        };
                        let outcome = memory::place_employee_note(kind, &content, &existing);
                        if outcome == memory::NoteOutcome::Duplicate {
                            return Ok((true, "Already in memory.".to_string(), vec![]));
                        }
                        blobs::register(tx, &blob)?;
                        let (status, supersedes, conflicts, event_type) = match outcome {
                            memory::NoteOutcome::Replaces(old) => (MemoryStatus::Active, Some(old), None, "memory.remembered"),
                            memory::NoteOutcome::Proposal(old) => (MemoryStatus::Proposed, None, Some(old), "memory.proposed"),
                            _ => (MemoryStatus::Active, None, None, "memory.remembered"),
                        };
                        let (event_id, n) = emit(tx, Ev {
                            aggregate_type: "memory",
                            aggregate_id: id.to_string(),
                            event_type,
                            actor: ActorKind::Employee,
                            actor_id: employee.map(|m| m.to_string()),
                            correlation: run.to_string(),
                            payload: None,
                            metadata: serde_json::json!({ "kind": kind.as_str(), "employee_id": employee, "project_assistant": employee.is_none(), "run_id": run, "conflicts_with": conflicts, "supersedes": supersedes }),
                            dedupe: format!("note:{id}"),
                        })?;
                        let mut notices = vec![n];
                        if let Some(old) = supersedes {
                            store::set_status(tx, old, MemoryStatus::Superseded, Some("employee"))?;
                            store::deindex(tx, old)?;
                            notices.push(mem_event(tx, old, "memory.superseded", ActorKind::Employee, run.to_string(), serde_json::json!({ "by": id }), format!("note:{id}:superseded"))?.1);
                        }
                        store::insert(tx, store::NewMemory {
                            id,
                            scope_id: &target,
                            kind,
                            blob_id: &blob.id.to_string(),
                            text: &content,
                            status,
                            confidence: 0.9,
                            importance: if kind == MemoryKind::Decision { 0.7 } else { 0.6 },
                            pinned: false,
                            supersedes,
                            conflicts_with: conflicts,
                            decided_by: (status == MemoryStatus::Active).then_some("employee"),
                            locator: serde_json::json!({ "run_id": run }),
                            sources: vec![(event_id, if employee.is_some() { serde_json::json!({ "authored_by_employee": employee }) } else { serde_json::json!({ "authored_by_project_assistant": true }) })],
                            extraction_version: "employee",
                        })?;
                        schedule_consolidation(tx, &target, event_id, false)?;
                        let text = match status {
                            MemoryStatus::Proposed => "This contradicts something the user decided, so it was stored as a proposal for the user. Until they accept it, keep following their decision.".to_string(),
                            _ if supersedes.is_some() => "Remembered; it replaces an earlier note.".to_string(),
                            _ => "Remembered.".to_string(),
                        };
                        Ok((true, text, notices))
                    })
                    .map_err(storage_error)
            }
        }
    }
}

/// A scope to consolidate: its mergeable memories, labelled M1..Mn.
#[derive(Debug, Clone)]
pub struct ConsolidationInput {
    pub scope_id: String,
    pub items: Vec<memory::Mergeable>,
    pub config: crate::run::connections::ResolvedRunConfig,
}

impl CommandService {
    /// The user asks to consolidate a project's or an employee's memory now.
    pub(crate) fn consolidate_memory(&self, envelope: &CommandEnvelope, hash: String, cmd: &ConsolidateMemory) -> Result<Committed, AppError> {
        let scope = cmd.scope;
        let command_id = envelope.command_id;
        self.commit(command_id, hash, move |tx| {
            if scope == MemoryScope::Global {
                return Ok(Err(domain_error(DomainError::Memory("Global memory holds only what you wrote yourself; it is never consolidated.".into()))));
            }
            let Some(scope_id) = store::scope_id(tx, scope)? else {
                return Ok(Err(domain_error(DomainError::Memory("This scope does not exist.".into()))));
            };
            let (event_id, n) = emit(tx, Ev {
                aggregate_type: "scope",
                aggregate_id: scope_id.clone(),
                event_type: "memory.consolidation_requested",
                actor: ActorKind::User,
                actor_id: None,
                correlation: command_id.to_string(),
                payload: None,
                metadata: serde_json::json!({}),
                dedupe: format!("cmd:{command_id}:0"),
            })?;
            let queued = schedule_consolidation(tx, &scope_id, event_id, true)?;
            Ok(Ok((CommandResult::MemoryConsolidationQueued { queued }, vec![n])))
        })
    }

    /// What a consolidation job works on; `None` when there is nothing to
    /// merge or no employee whose connection could do it.
    pub fn consolidation_input(&self, scope_id: &str) -> Result<Option<ConsolidationInput>, AppError> {
        let scope = scope_id.to_owned();
        let employee = self
            .store
            .read(|c| {
                // An employee's own scope; a project's: who worked there most
                // recently; else the oldest employee.
                let own: Option<String> = c.query_row("SELECT id FROM employees WHERE scope_id = ?1", [&scope], |r| r.get(0)).optional()?;
                let any = || -> rusqlite::Result<Option<String>> {
                    let worked: Option<String> = c
                        .query_row(
                            "SELECT t.employee_id FROM tasks t JOIN projects p ON p.id = t.project_id WHERE p.scope_id = ?1 AND t.employee_id IS NOT NULL
                             ORDER BY t.updated_at_ms DESC LIMIT 1",
                            [&scope],
                            |r| r.get(0),
                        )
                        .optional()?;
                    if worked.is_some() {
                        return Ok(worked);
                    }
                    c.query_row("SELECT id FROM employees ORDER BY status <> 'active', created_at_ms LIMIT 1", [], |r| r.get(0)).optional()
                };
                Ok(match own {
                    Some(m) => Some(m),
                    None => any()?,
                })
            })
            .map_err(storage_error)?;
        let Some(employee) = employee.and_then(|m| EmployeeId::parse(&m).ok()) else { return Ok(None) };
        let mut config = self.resolve_for_employee(employee, aurelius_domain::connection::Purpose::Memory)?;
        // No source: the company's login (B6); without one it does not run.
        config.login_user = self.store.read(crate::run::login::of_company).map_err(storage_error)?;
        let store_ref = self.store.clone();
        let scope = scope_id.to_owned();
        self.store
            .read(move |conn| {
                let read = |key: &str| store_ref.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
                let items: Vec<memory::Mergeable> = store::mergeable(conn, &scope)?
                    .into_iter()
                    .filter_map(|r| Some((r.id, r.kind, read(&r.blob_key)?)))
                    .enumerate()
                    .map(|(i, (id, kind, content))| memory::Mergeable { label: format!("M{}", i + 1), id, kind, content })
                    .collect();
                Ok((items.len() >= 2).then(|| ConsolidationInput { scope_id: scope.clone(), items, config }))
            })
            .map_err(storage_error)
    }

    /// Apply a validated consolidation and close the job, atomically and
    /// fenced. Each point becomes one memory that keeps the provenance of
    /// what it replaces; the replaced memories are superseded, never erased.
    /// A plan that needs review only adds proposals: accepting one supersedes
    /// its sources then.
    pub fn record_consolidation(&self, job_id: &str, epoch: i64, input: &ConsolidationInput, plan: &memory::ConsolidationPlan) -> Result<Vec<EventNotice>, AppError> {
        let mut prepared = Vec::new();
        for p in &plan.points {
            prepared.push(self.store.blobs().put(p.content.as_bytes(), "text/plain; charset=utf-8").map_err(storage_error)?);
        }
        let (input, plan, job) = (input.clone(), plan.clone(), job_id.to_owned());
        self.store
            .write(move |tx| {
                if !queries::holds_lease(tx, &job, epoch)? {
                    return Ok(Vec::new());
                }
                // Only what is still mergeable now: the user may have pinned or changed something meanwhile.
                let still: Vec<MemoryId> = store::mergeable(tx, &input.scope_id)?.into_iter().map(|r| r.id).collect();
                let status = if plan.needs_review { MemoryStatus::Proposed } else { MemoryStatus::Active };
                let mut notices = Vec::new();
                for (i, (point, blob)) in plan.points.iter().zip(&prepared).enumerate() {
                    if !point.sources.iter().all(|s| still.contains(s)) {
                        continue;
                    }
                    blobs::register(tx, blob)?;
                    let id = MemoryId::new();
                    let event_type = if status == MemoryStatus::Active { "memory.consolidated" } else { "memory.proposed" };
                    let (event_id, n) = mem_event(
                        tx,
                        id,
                        event_type,
                        ActorKind::System,
                        job.clone(),
                        serde_json::json!({ "kind": point.kind.as_str(), "consolidates": point.sources }),
                        format!("consolidate:{job}:{i}"),
                    )?;
                    notices.push(n);
                    // Provenance survives: the point keeps every source of what it replaces.
                    let mut sources = vec![(event_id, serde_json::json!({ "consolidation": job }))];
                    for old in &point.sources {
                        let mut stmt = tx.prepare("SELECT event_id, locator_json FROM memory_sources WHERE memory_id = ?1")?;
                        let rows = stmt.query_map([old.to_string()], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
                        for (e, l) in rows.filter_map(Result::ok) {
                            if let Ok(e) = EventId::parse(&e) {
                                sources.push((e, serde_json::from_str(&l).unwrap_or_default()));
                            }
                        }
                    }
                    store::insert(tx, store::NewMemory {
                        id,
                        scope_id: &input.scope_id,
                        kind: point.kind,
                        blob_id: &blob.id.to_string(),
                        text: &point.content,
                        status,
                        confidence: 0.8,
                        importance: point.importance,
                        pinned: false,
                        supersedes: point.sources.first().copied(),
                        conflicts_with: None,
                        decided_by: (status == MemoryStatus::Active).then_some("policy"),
                        locator: serde_json::json!({ "consolidates": point.sources }),
                        sources,
                        extraction_version: memory::CONSOLIDATION_VERSION,
                    })?;
                    if status == MemoryStatus::Active {
                        notices.extend(supersede_all(tx, &point.sources, Some(id), &job)?);
                    }
                }
                if !plan.needs_review {
                    let dropped: Vec<MemoryId> = plan.dropped.iter().copied().filter(|d| still.contains(d)).collect();
                    notices.extend(supersede_all(tx, &dropped, None, &job)?);
                }
                queries::complete_job(tx, &job, epoch)?;
                Ok(notices)
            })
            .map_err(storage_error)
    }
}

/// Supersede still-active memories (`by`: the point that replaces them, or none when outdated).
fn supersede_all(tx: &Tx<'_>, ids: &[MemoryId], by: Option<MemoryId>, correlation: &str) -> Result<Vec<EventNotice>, StorageError> {
    let mut notices = Vec::new();
    for old in ids {
        if !store::load(tx, *old)?.is_some_and(|o| o.status == MemoryStatus::Active) {
            continue;
        }
        store::set_status(tx, *old, MemoryStatus::Superseded, Some("policy"))?;
        store::deindex(tx, *old)?;
        let by_text = by.map(|b| b.to_string()).unwrap_or_else(|| "outdated".into());
        notices.push(mem_event(tx, *old, "memory.superseded", ActorKind::System, correlation.to_owned(), serde_json::json!({ "by": by, "consolidation": correlation }), format!("consolidate:{correlation}:{old}:{by_text}"))?.1);
    }
    Ok(notices)
}

/// Queue a consolidation of a scope. Never twice at the same time; unless
/// `forced` (the user asked) only when the scope is large enough and was not
/// consolidated in the last day. Returns whether a job was queued.
pub(crate) fn schedule_consolidation(tx: &Tx<'_>, scope_id: &str, source: EventId, forced: bool) -> Result<bool, StorageError> {
    let pending: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM jobs WHERE kind = ?1 AND state IN ('ready','leased') AND json_extract(payload_json, '$.scope_id') = ?2)",
        rusqlite::params![CONSOLIDATE_JOB_KIND, scope_id],
        |r| r.get(0),
    )?;
    if pending {
        return Ok(false);
    }
    if !forced {
        let recent: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM jobs WHERE kind = ?1 AND json_extract(payload_json, '$.scope_id') = ?2 AND not_before_ms > ?3)",
            rusqlite::params![CONSOLIDATE_JOB_KIND, scope_id, aurelius_storage::now_ms() - memory::CONSOLIDATE_EVERY_MS],
            |r| r.get(0),
        )?;
        let (items, tokens) = store::mergeable_stats(tx, scope_id)?;
        if recent || !memory::needs_consolidation(items, tokens) {
            return Ok(false);
        }
    }
    let id = format!("consolidate-{}", aurelius_domain::JobId::new());
    aurelius_storage::events::insert_job(tx, &id, CONSOLIDATE_JOB_KIND, source, &id, &serde_json::json!({ "scope_id": scope_id, "forced": forced }))?;
    Ok(true)
}

/// How much of the ticket's discussion and of each linked task's report goes into its summary.
pub const TICKET_DISCUSSION_MESSAGES: u32 = 40;
pub const TICKET_DISCUSSION_CHARS: usize = 600;
pub const LINKED_REPORT_CHARS: usize = 1_500;
pub const LINKED_REPORTS: usize = 12;

/// The sources of a ticket's summary besides its main task (`root`): the ticket's
/// fields (its `ticket.created`), its discussion (its `ticket.accepted`) and the
/// reports of the other tasks linked to it (each its `task.succeeded` or `task.failed`).
fn ticket_sources(conn: &rusqlite::Connection, ticket: TicketId, root: TaskId, read: &dyn Fn(&str) -> Option<String>) -> Result<Vec<(EventId, String)>, StorageError> {
    let event_of = |sql: &str, id: &str| -> Result<Option<EventId>, StorageError> {
        let found: Option<String> = conn.query_row(sql, [id], |r| r.get(0)).optional()?;
        Ok(found.and_then(|e| EventId::parse(&e).ok()))
    };
    let mut out = Vec::new();
    let Some(view) = aurelius_storage::tickets::view(conn, ticket, read)? else { return Ok(out) };
    if let Some(event) = event_of("SELECT id FROM events WHERE aggregate_type = 'ticket' AND aggregate_id = ?1 AND type = 'ticket.created' ORDER BY seq LIMIT 1", &ticket.to_string())? {
        let priority = serde_json::to_value(view.priority).ok().and_then(|p| p.as_str().map(str::to_owned)).unwrap_or_default();
        let mut what = format!("Ticket #{} “{}”, priority {priority}", view.number, view.title);
        if !view.labels.is_empty() {
            what.push_str(&format!(", labels {}", view.labels.join(", ")));
        }
        if let Some(description) = view.description.as_deref().map(str::trim).filter(|d| !d.is_empty()) {
            what.push_str(&format!("\nDescription (JSON-quoted):\n{}", serde_json::Value::from(description.chars().take(REPORT_MAX_CHARS).collect::<String>())));
        }
        out.push((event, what));
    }
    if let (Some(d), Some(event)) = (
        crate::conversation::discussion::of_ticket(conn, ticket)?,
        event_of("SELECT id FROM events WHERE aggregate_type = 'ticket' AND aggregate_id = ?1 AND type = 'ticket.accepted' ORDER BY seq DESC LIMIT 1", &ticket.to_string())?,
    ) {
        let lines = crate::conversation::discussion::discussion_json(conn, read, d.id, None, TICKET_DISCUSSION_MESSAGES, TICKET_DISCUSSION_CHARS)?;
        if !lines.is_empty() {
            out.push((event, format!("Discussion of the ticket (oldest first, one JSON object per message):\n[\n{}\n]", lines.join(",\n"))));
        }
    }
    let mut stmt = conn.prepare(
        "SELECT t.id, t.title, t.status, COALESCE(m.name, ''), b.storage_key FROM tasks t LEFT JOIN employees m ON m.id = t.employee_id
         LEFT JOIN blobs b ON b.id = t.result_blob_id WHERE t.ticket_id = ?1 AND t.id <> ?2 AND t.status IN ('succeeded', 'failed') ORDER BY t.created_at_ms, t.id",
    )?;
    let rows = stmt.query_map([ticket.to_string(), root.to_string()], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?, r.get::<_, Option<String>>(4)?)))?;
    for row in rows.take(LINKED_REPORTS) {
        let (id, title, status, who, key) = row?;
        let ended = if status == "succeeded" { "task.succeeded" } else { "task.failed" };
        let Some(event) = conn
            .query_row("SELECT id FROM events WHERE aggregate_type = 'task' AND aggregate_id = ?1 AND type = ?2 ORDER BY seq DESC LIMIT 1", [id.as_str(), ended], |r| r.get::<_, String>(0))
            .optional()?
            .and_then(|e| EventId::parse(&e).ok())
        else {
            continue;
        };
        let who = if who.is_empty() { String::new() } else { format!(" ({who})") };
        let what = match (status.as_str(), key.and_then(|k| read(&k)).map(|r| r.trim().chars().take(LINKED_REPORT_CHARS).collect::<String>()).filter(|r| !r.is_empty())) {
            ("succeeded", Some(report)) => format!("Linked task completed: {title}{who}\nReport (JSON-quoted):\n{}", serde_json::Value::from(report)),
            ("succeeded", None) => format!("Linked task completed: {title}{who}"),
            _ => format!("Linked task failed: {title}{who}"),
        };
        out.push((event, what));
    }
    Ok(out)
}

/// The line in a ticket's discussion when its summary failed: never the error itself.
pub const SUMMARY_FAILED: &str = "The summary for project memory could not be made (quota or run error). A board member can try again.";

/// Whether the ticket's summary is in memory already (in any scope: never a second one).
pub(crate) fn has_ticket_summary(conn: &rusqlite::Connection, ticket: TicketId) -> Result<bool, StorageError> {
    Ok(conn.query_row("SELECT EXISTS(SELECT 1 FROM memory_items WHERE json_extract(locator_json, '$.ticket_id') = ?1)", [ticket.to_string()], |r| r.get(0))?)
}

/// Whether the extraction that writes the ticket's summary is queued or running.
pub(crate) fn ticket_summary_pending(conn: &rusqlite::Connection, ticket: TicketId) -> Result<bool, StorageError> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM jobs WHERE kind = ?1 AND state IN ('ready','leased') AND json_extract(payload_json, '$.ticket_id') = ?2)",
        rusqlite::params![EXTRACT_JOB_KIND, ticket.to_string()],
        |r| r.get(0),
    )?)
}

/// `ticket.retry_summary`: the ticket's extraction once more, as a job of its own.
pub(crate) fn schedule_ticket_summary(tx: &Tx<'_>, root: TaskId, ticket: TicketId, source: EventId) -> Result<(), StorageError> {
    let id = format!("extract-{root}-retry-{}", aurelius_domain::JobId::new());
    aurelius_storage::events::insert_job(tx, &id, EXTRACT_JOB_KIND, source, &id, &serde_json::json!({ "task_id": root, "ticket_id": ticket }))
}

/// The ticket to summarize: in Done and without a summary in `project_scope` yet.
fn ticket_to_summarize(conn: &rusqlite::Connection, ticket: TicketId, project_scope: &str) -> Result<Option<TicketRef>, StorageError> {
    let row: Option<(u32, String)> = conn
        .query_row(
            "SELECT k.number, k.title FROM tickets k JOIN board_columns c ON c.id = k.column_id
             WHERE k.id = ?1 AND c.kind = 'done' AND k.done_at_ms IS NOT NULL
             AND NOT EXISTS(SELECT 1 FROM memory_items m WHERE m.scope_id = ?2 AND json_extract(m.locator_json, '$.ticket_id') = ?1)",
            [ticket.to_string(), project_scope.to_owned()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    Ok(row.map(|(number, title)| TicketRef { id: ticket, number, title }))
}

/// Schedule extraction after a task succeeded or failed (called inside the task
/// transaction). A task retried after it failed is extracted again when it ends:
/// a lesson never confirms itself (`LessonLocator::tasks`), doubles are dropped.
/// With `ticket` (the main task of a ticket that went to Done) the same
/// extraction also writes the ticket's summary (MU1-18).
pub(crate) fn schedule_extraction(tx: &Tx<'_>, task: TaskId, ticket: Option<TicketId>, source: EventId) -> Result<(), StorageError> {
    let first = format!("extract-{task}");
    let taken: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM jobs WHERE id = ?1)", [&first], |r| r.get(0))?;
    let (id, dedupe) = if taken { (format!("extract-{task}-{source}"), format!("memory-extract:{task}:{source}")) } else { (first, format!("memory-extract:{task}")) };
    let payload = match ticket {
        Some(ticket) => serde_json::json!({ "task_id": task, "ticket_id": ticket }),
        None => serde_json::json!({ "task_id": task }),
    };
    aurelius_storage::events::insert_job(tx, &id, EXTRACT_JOB_KIND, source, &dedupe, &payload)
}

/// Schedule a summary when enough uncovered messages accumulated in an employee conversation.
/// `run`: the run whose answer completed it; the summary runs on that run's login.
pub(crate) fn maybe_schedule_summary(tx: &Tx<'_>, conversation: ConversationId, source: EventId, run: aurelius_domain::RunId) -> Result<(), StorageError> {
    let employee: Option<String> = tx.query_row("SELECT employee_id FROM conversations WHERE id = ?1", [conversation.to_string()], |r| r.get(0)).ok().flatten();
    if employee.is_none() {
        return Ok(());
    }
    let through = store::latest_summary(tx, conversation)?.map(|(_, t)| t).unwrap_or(0);
    let uncovered: u32 = tx.query_row(
        "SELECT COUNT(*) FROM messages WHERE conversation_id = ?1 AND status = 'complete' AND ordinal > ?2",
        rusqlite::params![conversation.to_string(), through],
        |r| r.get(0),
    )?;
    if uncovered < SUMMARY_EVERY {
        return Ok(());
    }
    let pending: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM jobs WHERE kind = ?1 AND state IN ('ready','leased') AND json_extract(payload_json, '$.conversation_id') = ?2)",
        rusqlite::params![SUMMARY_JOB_KIND, conversation.to_string()],
        |r| r.get(0),
    )?;
    if pending {
        return Ok(());
    }
    let id = format!("summary-{}", aurelius_domain::JobId::new());
    aurelius_storage::events::insert_job(tx, &id, SUMMARY_JOB_KIND, source, &id, &serde_json::json!({ "conversation_id": conversation, "run_id": run }))
}
