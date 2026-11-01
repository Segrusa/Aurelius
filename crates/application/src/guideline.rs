//! Guideline writes (feature `guidelines`, AUR-GL-01). Every one needs an
//! active board member on the connection, as its first step (SEC0 R-a): no
//! app, no agent and no mode ever changes or approves a guideline.
//!
//! Each write runs in a savepoint of the command's transaction: a refusal
//! after something was written (B4's limit, a storage rule) rolls it back, so
//! nothing of a refused write stays. B4: after writing, every combination of an
//! employee and a project is counted again (`combinations_over`); a write that
//! makes a combination go over 16 000 characters, or further over, is refused
//! with `guideline_too_large`. A combination that was over already (a project
//! back from the archive) does not block a write that leaves it as it was.
//!
//! B10: the daemon refuses a command whose title or text holds a vault value
//! before it gets here. A write that stores a text the command does not carry
//! (approving a proposal as it is, restoring a version or an archived
//! guideline) checks that text here against the current redactor (SEC0 E7, E8);
//! so do the others, in case the command did not come through the daemon.

use crate::ticket::require_board_member;
use crate::{CommandContext, CommandService, Committed, TxResult, domain_error, notice, storage_error};
use aurelius_domain::guideline::{
    self as gl, AddStarterGuidelines, CreateGuideline, DecideGuidelineProposal, GuidelineEventMeta, GuidelineProposalDecidedMeta, GuidelineSource,
    GuidelineTarget, GuidelineTooLarge, LinkGuideline, MAX_RUN_CHARS, RestoreGuidelineVersion, UnlinkGuideline, UpdateGuideline,
};
use aurelius_domain::proposal::ProposalStatus;
use aurelius_domain::{ActorKind, AppError, Command, CommandEnvelope, CommandResult, DomainError, EventKind, EventNotice, GuidelineId, UserId};
use aurelius_storage::guidelines::{self as store, GuidelineOverflow, NewVersion};
use aurelius_storage::{StorageError, Tx as Transaction, events::{self, NewEvent}};
use rusqlite::OptionalExtension;
use serde_json::Value;

/// Who writes, owned so it can go into the transaction.
#[derive(Clone)]
struct Writer {
    user: UserId,
    command_id: String,
    meta: Value,
    actor: String,
}

impl Writer {
    /// One `guideline.*` event about the guideline at version `v`, its metadata a `GuidelineEventMeta`.
    fn event(&self, tx: &Transaction<'_>, kind: EventKind, v: &gl::GuidelineVersionView, target: Option<GuidelineTarget>, n: usize) -> Result<EventNotice, StorageError> {
        let id = v.guideline_id;
        let meta = GuidelineEventMeta { id, title: v.title.clone(), version: v.version, by: Some(self.user), target };
        self.append(tx, kind, "guideline", id.to_string(), serde_json::to_value(&meta)?, n)
    }

    fn append(&self, tx: &Transaction<'_>, kind: EventKind, aggregate_type: &'static str, aggregate_id: String, meta: Value, n: usize) -> Result<EventNotice, StorageError> {
        let mut metadata = meta;
        if let (Some(m), Some(ctx)) = (metadata.as_object_mut(), self.meta.as_object()) {
            for (k, v) in ctx {
                m.entry(k.clone()).or_insert(v.clone());
            }
        }
        let appended = events::append(
            tx,
            NewEvent {
                aggregate_type,
                aggregate_version: crate::chat_next_version(tx, aggregate_type, &aggregate_id)?,
                aggregate_id: aggregate_id.clone(),
                event_type: kind.as_str(),
                project_id: None,
                actor_kind: ActorKind::User,
                actor_id: Some(self.actor.clone()),
                correlation_id: self.command_id.clone(),
                causation_id: None,
                payload_blob_id: None,
                metadata,
                dedupe_key: format!("cmd:{}:{n}", self.command_id),
            },
        )?;
        Ok(notice(appended.seq, kind.as_str(), aggregate_id, None))
    }
}

/// A storage refusal (`StorageError::Rejected` with a `guideline_*` code) as the client's error.
fn refusal(code: &str, id: Option<GuidelineId>) -> Option<AppError> {
    let id = id.unwrap_or_default();
    Some(match code {
        store::NOT_FOUND => domain_error(DomainError::GuidelineNotFound(id)),
        store::PROPOSAL_NOT_FOUND => domain_error(DomainError::GuidelineProposalNotFound(Default::default())),
        store::TITLE_TAKEN => domain_error(DomainError::GuidelineTitleTaken),
        store::ARCHIVED => domain_error(DomainError::OutOfRange { field: "guideline_id".into(), detail: "this guideline is archived; restore it first".into() }),
        store::NOT_ARCHIVED => domain_error(DomainError::OutOfRange { field: "guideline_id".into(), detail: "this guideline is not archived".into() }),
        store::VERSION_NOT_FOUND => AppError::new(aurelius_domain::ErrorCode::NotFound, "This version of the guideline does not exist."),
        store::INVALID_TITLE => domain_error(DomainError::OutOfRange { field: "title".into(), detail: "1 to 120 characters".into() }),
        store::INVALID_TEXT => domain_error(DomainError::TooLong { field: "text".into(), max: gl::MAX_GUIDELINE_CHARS }),
        store::UNKNOWN_STARTER => domain_error(DomainError::OutOfRange { field: "keys".into(), detail: "not a key of the starter set".into() }),
        store::INVALID_DECISION => domain_error(DomainError::OutOfRange { field: "approve".into(), detail: "approve or reject".into() }),
        // The version moved on between the read and the write (only a race gets here).
        store::VERSION_CONFLICT => domain_error(DomainError::VersionConflict { expected: 0, actual: 0 }),
        _ => return None,
    })
}

/// `body` in a savepoint: a refusal (its own, or a storage code) rolls back what it wrote.
fn guarded<T>(tx: &Transaction<'_>, id: Option<GuidelineId>, body: impl FnOnce() -> TxResult<T>) -> TxResult<T> {
    tx.execute_batch("SAVEPOINT guideline_write")?;
    let outcome = match body() {
        Ok(Ok(done)) => {
            tx.execute_batch("RELEASE guideline_write")?;
            return Ok(Ok(done));
        }
        Ok(Err(refused)) => Ok(Err(refused)),
        Err(StorageError::Rejected(code)) => match refusal(&code, id) {
            Some(refused) => Ok(Err(refused)),
            None => Err(StorageError::Rejected(code)),
        },
        Err(other) => Err(other),
    };
    tx.execute_batch("ROLLBACK TO guideline_write; RELEASE guideline_write")?;
    outcome
}

/// Refused when the title or text holds the value of a vault item (B10, SEC0 E7/E8): the items' names only.
fn no_secret(title: &str, text: &str) -> Result<(), AppError> {
    let redactor = aurelius_runtime::redact::current();
    let mut names = redactor.names_in(title);
    for name in redactor.names_in(text) {
        if !names.contains(&name) {
            names.push(name);
        }
    }
    if names.is_empty() { Ok(()) } else { Err(AppError::looks_like_secret(names)) }
}

/// B4 after a write: refused when a combination is over the limit that was not
/// before (`before`), or is further over now.
fn within_limit(tx: &Transaction<'_>, before: &[GuidelineOverflow]) -> Result<Result<(), AppError>, StorageError> {
    let after = store::combinations_over(tx, MAX_RUN_CHARS)?;
    let worse = after.iter().find(|o| {
        !before.iter().any(|b| b.employee_id == o.employee_id && b.project_id == o.project_id && b.total_chars >= o.total_chars)
    });
    let Some(over) = worse else { return Ok(Ok(())) };
    let name = |sql: &str, id: String| -> Result<Option<String>, StorageError> { Ok(tx.query_row(sql, [id], |r| r.get(0)).optional()?) };
    let employee = match over.employee_id {
        Some(e) => name("SELECT name FROM employees WHERE id = ?1", e.to_string())?,
        None => None,
    };
    let project = match over.project_id {
        Some(p) => name("SELECT name FROM projects WHERE id = ?1", p.to_string())?,
        None => None,
    };
    let whose = match (employee, project) {
        (Some(e), Some(p)) => format!("{e} in the project {p}"),
        (Some(e), None) => format!("{e} outside a project"),
        (None, Some(p)) => format!("the project {p}"),
        (None, None) => "a run".to_string(),
    };
    let message = format!(
        "Together the guidelines of {whose} would be {} characters; a run takes at most {MAX_RUN_CHARS}. Shorten a guideline or unlink one first.",
        over.total_chars
    );
    Ok(Err(AppError::guideline_too_large(
        GuidelineTooLarge { chars: over.total_chars, limit: MAX_RUN_CHARS as u32, employee_id: over.employee_id, project_id: over.project_id },
        message,
    )))
}

/// A link target that exists and is not archived.
fn check_target(tx: &Transaction<'_>, target: GuidelineTarget) -> Result<Result<(), AppError>, StorageError> {
    let (sql, id) = match target {
        GuidelineTarget::Employee(e) => ("SELECT status FROM employees WHERE id = ?1", e.to_string()),
        GuidelineTarget::Project(p) => ("SELECT status FROM projects WHERE id = ?1", p.to_string()),
    };
    let status: Option<String> = tx.query_row(sql, [id], |r| r.get(0)).optional()?;
    Ok(match (status.as_deref(), target) {
        (None, GuidelineTarget::Employee(e)) => Err(domain_error(DomainError::EmployeeNotFound(e))),
        (None, GuidelineTarget::Project(p)) => Err(domain_error(DomainError::ProjectNotFound(p))),
        (Some("archived"), _) => Err(domain_error(DomainError::OutOfRange { field: "target".into(), detail: "an archived employee or project gets no guidelines".into() })),
        _ => Ok(()),
    })
}

macro_rules! check {
    ($e:expr) => {
        match $e {
            Ok(value) => value,
            Err(err) => return Ok(Err(err)),
        }
    };
}

fn version_of(tx: &Transaction<'_>, id: GuidelineId) -> Result<Result<gl::GuidelineVersionView, AppError>, StorageError> {
    Ok(store::get(tx, id, None)?.ok_or_else(|| domain_error(DomainError::GuidelineNotFound(id))))
}

fn changed(guideline_id: GuidelineId, version: u32) -> CommandResult {
    CommandResult::GuidelineChanged { guideline_id, version }
}

impl CommandService {
    /// The nine guideline writes. The board member first, then the command's own checks.
    pub(crate) fn guideline_command(&self, ctx: &CommandContext, envelope: &CommandEnvelope, hash: String) -> Result<Committed, AppError> {
        let user = require_board_member(ctx)?;
        let writer = Writer { user, command_id: envelope.command_id.to_string(), meta: ctx.metadata(), actor: ctx.principal_id.to_string() };
        let expected = envelope.expected_version;
        let now = aurelius_storage::now_ms();
        let command_id = envelope.command_id;
        match &envelope.command {
            Command::CreateGuideline(cmd) => {
                let cmd = cmd.validated().map_err(domain_error)?;
                no_secret(&cmd.title, &cmd.text)?;
                self.commit(command_id, hash, move |tx| create(tx, &writer, cmd, now))
            }
            Command::UpdateGuideline(cmd) => {
                let cmd = cmd.validated().map_err(domain_error)?;
                no_secret(cmd.title.as_deref().unwrap_or_default(), cmd.text.as_deref().unwrap_or_default())?;
                self.commit(command_id, hash, move |tx| update(tx, &writer, cmd, expected, now))
            }
            Command::ArchiveGuideline(cmd) => {
                let id = cmd.guideline_id;
                self.commit(command_id, hash, move |tx| {
                    guarded(tx, Some(id), || {
                        store::archive(tx, id, writer.user, now)?;
                        let current = check!(version_of(tx, id)?);
                        let n = writer.event(tx, EventKind::GuidelineArchived, &current, None, 0)?;
                        Ok(Ok((changed(id, current.version), vec![n])))
                    })
                })
            }
            Command::RestoreGuideline(cmd) => {
                let id = cmd.guideline_id;
                self.commit(command_id, hash, move |tx| {
                    guarded(tx, Some(id), || {
                        // E8: the text that comes back may hold a value that became a vault secret since.
                        let current = check!(version_of(tx, id)?);
                        check!(no_secret(&current.title, &current.text));
                        let before = store::combinations_over(tx, MAX_RUN_CHARS)?;
                        store::restore(tx, id, writer.user, now)?;
                        check!(within_limit(tx, &before)?);
                        let n = writer.event(tx, EventKind::GuidelineRestored, &current, None, 0)?;
                        Ok(Ok((changed(id, current.version), vec![n])))
                    })
                })
            }
            Command::RestoreGuidelineVersion(cmd) => {
                let cmd = cmd.validated().map_err(domain_error)?;
                self.commit(command_id, hash, move |tx| restore_version(tx, &writer, cmd, expected, now))
            }
            Command::LinkGuideline(cmd) => {
                let cmd = cmd.clone();
                self.commit(command_id, hash, move |tx| link(tx, &writer, cmd, now))
            }
            Command::UnlinkGuideline(cmd) => {
                let cmd = cmd.clone();
                self.commit(command_id, hash, move |tx| unlink(tx, &writer, cmd))
            }
            Command::DecideGuidelineProposal(cmd) => {
                // The title and text it stores are checked in the transaction (E7).
                let cmd = cmd.validated().map_err(domain_error)?;
                self.commit(command_id, hash, move |tx| decide(tx, &writer, cmd, now))
            }
            Command::AddStarterGuidelines(cmd) => {
                let cmd = cmd.validated().map_err(domain_error)?;
                self.commit(command_id, hash, move |tx| add_starter_set(tx, &writer, cmd, now))
            }
            other => Err(AppError::new(aurelius_domain::ErrorCode::Internal, format!("{} is not a guideline write", other.method()))),
        }
    }
}

type Done = TxResult<(CommandResult, Vec<EventNotice>)>;

fn create(tx: &Transaction<'_>, w: &Writer, cmd: CreateGuideline, now: i64) -> Done {
    let id = GuidelineId::new();
    guarded(tx, Some(id), || {
        for target in &cmd.links {
            check!(check_target(tx, *target)?);
        }
        let before = store::combinations_over(tx, MAX_RUN_CHARS)?;
        let v = NewVersion { title: &cmd.title, text: &cmd.text, source: GuidelineSource::Created, proposal_id: None, restored_from: None, by_user: Some(w.user), now_ms: now };
        let made = store::create(tx, id, v)?;
        for target in &cmd.links {
            store::link(tx, id, *target, w.user, now)?;
        }
        check!(within_limit(tx, &before)?);
        let n = w.event(tx, EventKind::GuidelineCreated, &made, None, 0)?;
        Ok(Ok((changed(id, made.version), vec![n])))
    })
}

fn update(tx: &Transaction<'_>, w: &Writer, cmd: UpdateGuideline, expected: Option<u64>, now: i64) -> Done {
    let id = cmd.guideline_id;
    guarded(tx, Some(id), || {
        let current = check!(version_of(tx, id)?);
        if let Some(expected) = expected.filter(|e| *e != u64::from(current.version)) {
            return Ok(Err(domain_error(DomainError::VersionConflict { expected, actual: u64::from(current.version) })));
        }
        let title = cmd.title.clone().unwrap_or(current.title.clone());
        let text = cmd.text.clone().unwrap_or(current.text.clone());
        if title == current.title && text == current.text {
            // Nothing changed: no new version.
            return Ok(Ok((changed(id, current.version), vec![])));
        }
        let before = store::combinations_over(tx, MAX_RUN_CHARS)?;
        let v = NewVersion { title: &title, text: &text, source: GuidelineSource::Edited, proposal_id: None, restored_from: None, by_user: Some(w.user), now_ms: now };
        let made = store::add_version(tx, id, current.version, v)?;
        check!(within_limit(tx, &before)?);
        let n = w.event(tx, EventKind::GuidelineUpdated, &made, None, 0)?;
        Ok(Ok((changed(id, made.version), vec![n])))
    })
}

fn restore_version(tx: &Transaction<'_>, w: &Writer, cmd: RestoreGuidelineVersion, expected: Option<u64>, now: i64) -> Done {
    let id = cmd.guideline_id;
    guarded(tx, Some(id), || {
        let current = check!(version_of(tx, id)?);
        if let Some(expected) = expected.filter(|e| *e != u64::from(current.version)) {
            return Ok(Err(domain_error(DomainError::VersionConflict { expected, actual: u64::from(current.version) })));
        }
        let Some(old) = store::get(tx, id, Some(cmd.version))? else {
            return Ok(Err(AppError::new(aurelius_domain::ErrorCode::NotFound, "This version of the guideline does not exist.")));
        };
        // E8: an old text may hold a value that became a vault secret since.
        check!(no_secret(&old.title, &old.text));
        let before = store::combinations_over(tx, MAX_RUN_CHARS)?;
        let v = NewVersion { title: &old.title, text: &old.text, source: GuidelineSource::Restored, proposal_id: None, restored_from: Some(cmd.version), by_user: Some(w.user), now_ms: now };
        let made = store::add_version(tx, id, current.version, v)?;
        check!(within_limit(tx, &before)?);
        let n = w.event(tx, EventKind::GuidelineUpdated, &made, None, 0)?;
        Ok(Ok((changed(id, made.version), vec![n])))
    })
}

fn link(tx: &Transaction<'_>, w: &Writer, cmd: LinkGuideline, now: i64) -> Done {
    let id = cmd.guideline_id;
    guarded(tx, Some(id), || {
        let current = check!(version_of(tx, id)?);
        check!(check_target(tx, cmd.target)?);
        let before = store::combinations_over(tx, MAX_RUN_CHARS)?;
        if !store::link(tx, id, cmd.target, w.user, now)? {
            return Ok(Ok((changed(id, current.version), vec![])));
        }
        check!(within_limit(tx, &before)?);
        let n = w.event(tx, EventKind::GuidelineLinked, &current, Some(cmd.target), 0)?;
        Ok(Ok((changed(id, current.version), vec![n])))
    })
}

fn unlink(tx: &Transaction<'_>, w: &Writer, cmd: UnlinkGuideline) -> Done {
    let id = cmd.guideline_id;
    guarded(tx, Some(id), || {
        let current = check!(version_of(tx, id)?);
        // Less never goes over the limit: no B4 check.
        if !store::unlink(tx, id, cmd.target)? {
            return Ok(Ok((changed(id, current.version), vec![])));
        }
        let n = w.event(tx, EventKind::GuidelineUnlinked, &current, Some(cmd.target), 0)?;
        Ok(Ok((changed(id, current.version), vec![n])))
    })
}

fn decide(tx: &Transaction<'_>, w: &Writer, cmd: DecideGuidelineProposal, now: i64) -> Done {
    let pid = cmd.proposal_id;
    let Some(p) = store::proposal(tx, pid)? else { return Ok(Err(domain_error(DomainError::GuidelineProposalNotFound(pid)))) };
    // T23: one withdrawn when its proposer was archived says so; any other is handled already.
    if p.status == ProposalStatus::Withdrawn {
        return Ok(Err(domain_error(DomainError::GuidelineProposalWithdrawn)));
    }
    if p.status != ProposalStatus::Pending {
        return Ok(Err(domain_error(DomainError::ApprovalNotPending)));
    }
    let decided = |tx: &Transaction<'_>, approved: bool, n: usize| {
        let meta = serde_json::to_value(GuidelineProposalDecidedMeta { proposal_id: pid, approved })?;
        w.append(tx, EventKind::GuidelineProposalDecided, "guideline_proposal", pid.to_string(), meta, n)
    };
    if !cmd.approve {
        return guarded(tx, p.guideline_id, || {
            store::decide_proposal(tx, pid, ProposalStatus::Rejected, cmd.note.as_deref(), w.user, now)?;
            let n = decided(tx, false, 0)?;
            Ok(Ok((CommandResult::GuidelineProposalDecided { proposal_id: pid, status: ProposalStatus::Rejected, guideline_id: p.guideline_id }, vec![n])))
        });
    }
    // The proposer must still work here; otherwise the proposal lapses (it stays withdrawn).
    let proposer: Option<String> = tx.query_row("SELECT status FROM employees WHERE id = ?1", [p.proposed_by.to_string()], |r| r.get(0)).optional()?;
    if proposer.as_deref().is_none_or(|s| s == "archived") {
        store::withdraw_for_employee(tx, p.proposed_by, now)?;
        return Ok(Err(domain_error(DomainError::GuidelineProposalWithdrawn)));
    }
    let title = cmd.title.clone().unwrap_or(p.title.clone());
    let text = cmd.text.clone().unwrap_or(p.text.clone());
    // E7: approved as it is, the stored text may hold a value that is a vault secret since the proposal.
    check!(no_secret(&title, &text));
    guarded(tx, p.guideline_id, || {
        let before = store::combinations_over(tx, MAX_RUN_CHARS)?;
        let v = NewVersion { title: &title, text: &text, source: GuidelineSource::Proposal, proposal_id: Some(pid), restored_from: None, by_user: Some(w.user), now_ms: now };
        let (id, made, kind) = match (p.guideline_id, p.base_version) {
            (Some(id), base) => {
                // A change never touches the links (E12).
                if cmd.links.is_some() {
                    return Ok(Err(domain_error(DomainError::OutOfRange { field: "links".into(), detail: "links come with a new guideline only; a change keeps its links".into() })));
                }
                let current = check!(version_of(tx, id)?);
                let base = base.unwrap_or(current.version);
                if base != current.version {
                    return Ok(Err(domain_error(DomainError::VersionConflict { expected: u64::from(base), actual: u64::from(current.version) })));
                }
                (id, store::add_version(tx, id, base, v)?, EventKind::GuidelineUpdated)
            }
            (None, _) => {
                // The board member's links, or else the ones the server suggested (E12).
                let links = cmd.links.clone().unwrap_or(p.suggested_links.clone());
                let links = check!(gl::validate_links(&links).map_err(domain_error));
                for target in &links {
                    check!(check_target(tx, *target)?);
                }
                let id = GuidelineId::new();
                let made = store::create(tx, id, v)?;
                for target in &links {
                    store::link(tx, id, *target, w.user, now)?;
                }
                (id, made, EventKind::GuidelineCreated)
            }
        };
        if !store::decide_proposal(tx, pid, ProposalStatus::Approved, cmd.note.as_deref(), w.user, now)? {
            return Ok(Err(domain_error(DomainError::ApprovalNotPending)));
        }
        check!(within_limit(tx, &before)?);
        let notices = vec![w.event(tx, kind, &made, None, 0)?, decided(tx, true, 1)?];
        Ok(Ok((CommandResult::GuidelineProposalDecided { proposal_id: pid, status: ProposalStatus::Approved, guideline_id: Some(id) }, notices)))
    })
}

fn add_starter_set(tx: &Transaction<'_>, w: &Writer, cmd: AddStarterGuidelines, now: i64) -> Done {
    guarded(tx, None, || {
        let before = store::combinations_over(tx, MAX_RUN_CHARS)?;
        let keys: Vec<&str> = cmd.keys.iter().map(String::as_str).collect();
        let added = store::add_starter_set(tx, &keys, w.user, now)?;
        // Without links nothing changes for a run; checked all the same, as every write is.
        check!(within_limit(tx, &before)?);
        let mut notices = Vec::new();
        for (n, (_, id)) in added.added.iter().enumerate() {
            let made = check!(version_of(tx, *id)?);
            notices.push(w.event(tx, EventKind::GuidelineCreated, &made, None, n)?);
        }
        Ok(Ok((CommandResult::StarterGuidelinesAdded { guideline_ids: added.added.iter().map(|(_, id)| *id).collect() }, notices)))
    })
}

/// Read side for the daemon: the conversations whose current session holds a
/// version of this guideline (they start a new session at their next run, B6).
pub fn sessions_holding(conn: &rusqlite::Connection, id: GuidelineId) -> Result<u32, AppError> {
    conn.query_row(
        "SELECT COUNT(*) FROM conversation_sessions s
         WHERE json_valid(s.context_json)
           AND EXISTS(SELECT 1 FROM json_each(s.context_json, '$.guidelines') j WHERE j.value LIKE ?1 || '@%')",
        [id.to_string()],
        |r| r.get::<_, i64>(0),
    )
    .map(|n| n as u32)
    .map_err(|e| storage_error(e.into()))
}
