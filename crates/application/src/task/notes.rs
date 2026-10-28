//! Notes per project (like Apple Notes): Markdown the user writes. They are
//! host writes, not domain commands, so autosaves stay out of the command log;
//! every change emits `note.created`, `note.updated` or `note.deleted` on the
//! note (with its project) so other clients refresh.

use crate::{CommandService, domain_error, notice, storage_error};
use aurelius_domain::views::{NoteView, NotesSnapshot};
use aurelius_domain::{ActorKind, AppError, DomainError, ErrorCode, EventKind, EventNotice, NoteId, ProjectId, employee::LifecycleStatus};
use aurelius_storage::events::{self, NewEvent};
use aurelius_storage::notes::{self as store, MAX_BODY_BYTES, Saved};
use aurelius_storage::{StorageError, Tx, conversations as conv, employees};

fn not_found(id: NoteId) -> AppError {
    AppError::new(ErrorCode::NotFound, format!("Note {id} not found."))
}

fn check_body(body: &str) -> Result<(), AppError> {
    if body.len() > MAX_BODY_BYTES {
        return Err(AppError::new(ErrorCode::Validation, format!("A note can hold at most {} KB.", MAX_BODY_BYTES / 1024)));
    }
    Ok(())
}

/// Notes can be written in active and paused projects, not in archived ones.
fn writable(tx: &Tx<'_>, project: ProjectId) -> Result<Result<(), AppError>, StorageError> {
    Ok(match employees::project_scope(tx, project)? {
        None => Err(domain_error(DomainError::ProjectNotFound(project))),
        Some((_, LifecycleStatus::Archived)) => Err(domain_error(DomainError::ProjectArchived)),
        Some(_) => Ok(()),
    })
}

fn note_event(tx: &Tx<'_>, kind: EventKind, note: &NoteView) -> Result<EventNotice, StorageError> {
    let (id, project) = (note.summary.id.to_string(), note.summary.project_id);
    let appended = events::append(
        tx,
        NewEvent {
            aggregate_type: "note",
            aggregate_id: id.clone(),
            aggregate_version: conv::next_version(tx, "note", &id)?,
            event_type: kind.as_str(),
            project_id: Some(project),
            actor_kind: ActorKind::User,
            actor_id: None,
            correlation_id: id.clone(),
            causation_id: None,
            payload_blob_id: None,
            metadata: serde_json::json!({ "revision": note.summary.revision, "pinned": note.summary.pinned }),
            dedupe_key: format!("note:{id}:{}:{}:{}", kind.as_str(), note.summary.revision, aurelius_storage::now_ms()),
        },
    )?;
    Ok(notice(appended.seq, kind.as_str(), id, Some(project)))
}

impl CommandService {
    /// `note.list`: pinned first, then the last edited; `query` searches the bodies.
    pub fn notes(&self, project: ProjectId, query: Option<String>) -> Result<NotesSnapshot, AppError> {
        self.store.read(|c| Ok(NotesSnapshot { notes: store::list(c, project, query.as_deref())? })).map_err(storage_error)
    }

    pub fn note(&self, id: NoteId) -> Result<NoteView, AppError> {
        self.store.read(|c| store::get(c, id)).map_err(storage_error)?.ok_or_else(|| not_found(id))
    }

    pub fn create_note(&self, project: ProjectId, body: String) -> Result<(NoteView, Vec<EventNotice>), AppError> {
        check_body(&body)?;
        self.store
            .write(move |tx| {
                if let Err(e) = writable(tx, project)? {
                    return Ok(Err(e));
                }
                let note = store::create(tx, project, &body, aurelius_storage::now_ms())?;
                let n = note_event(tx, EventKind::NoteCreated, &note)?;
                Ok(Ok((note, vec![n])))
            })
            .map_err(storage_error)?
    }

    /// Saves `body` if the note is still at `base_revision`; else refuses with a
    /// version conflict, and the client loads the current note again.
    pub fn update_note(&self, id: NoteId, body: String, base_revision: u64) -> Result<(NoteView, Vec<EventNotice>), AppError> {
        check_body(&body)?;
        self.store
            .write(move |tx| {
                let Some(current) = store::get(tx, id)? else { return Ok(Err(not_found(id))) };
                if let Err(e) = writable(tx, current.summary.project_id)? {
                    return Ok(Err(e));
                }
                // Nothing changed (an autosave of the same text): no new revision.
                if current.summary.revision == base_revision && current.body == body {
                    return Ok(Ok((current, vec![])));
                }
                Ok(match store::update(tx, id, &body, base_revision, aurelius_storage::now_ms())? {
                    Saved::Saved(note) => {
                        let n = note_event(tx, EventKind::NoteUpdated, &note)?;
                        Ok((note, vec![n]))
                    }
                    Saved::Stale(current) => Err(AppError::new(
                        ErrorCode::VersionConflict,
                        format!("The note changed elsewhere (revision {} instead of {base_revision}); load it again.", current.summary.revision),
                    )),
                    Saved::NotFound => Err(not_found(id)),
                })
            })
            .map_err(storage_error)?
    }

    pub fn set_note_pinned(&self, id: NoteId, pinned: bool) -> Result<(NoteView, Vec<EventNotice>), AppError> {
        self.store
            .write(move |tx| {
                let Some(current) = store::get(tx, id)? else { return Ok(Err(not_found(id))) };
                if let Err(e) = writable(tx, current.summary.project_id)? {
                    return Ok(Err(e));
                }
                if current.summary.pinned == pinned {
                    return Ok(Ok((current, vec![])));
                }
                let Some(note) = store::set_pinned(tx, id, pinned)? else { return Ok(Err(not_found(id))) };
                let n = note_event(tx, EventKind::NoteUpdated, &note)?;
                Ok(Ok((note, vec![n])))
            })
            .map_err(storage_error)?
    }

    pub fn delete_note(&self, id: NoteId) -> Result<Vec<EventNotice>, AppError> {
        self.store
            .write(move |tx| {
                let Some(current) = store::get(tx, id)? else { return Ok(Err(not_found(id))) };
                if let Err(e) = writable(tx, current.summary.project_id)? {
                    return Ok(Err(e));
                }
                store::delete(tx, id)?;
                let n = note_event(tx, EventKind::NoteDeleted, &current)?;
                Ok(Ok(vec![n]))
            })
            .map_err(storage_error)?
    }
}
