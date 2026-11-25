//! Reviewing a branch in the Code tab (ticket #84, feature `code_review`): line threads,
//! replies, resolving, viewed files and finishing a review. Only a board member reviews.
//! Every change is one `code.review_changed` event on the branch's aggregate
//! (`<project_id>:<branch>`), so the reader's other devices and other board members read
//! the review again. Waking the owner is not done here: the client posts the review's text
//! with `ticket.post`, `ticket.send_back` or `chat.send` after `code.submit_review`.

use crate::ticket::require_board_member;
use crate::{CommandContext, CommandService, Committed, domain_error, notice};
use aurelius_domain::code_review::{CodeVerdict, ReplyCodeThread, ResolveCodeThread, SetCodeViewed, StartCodeThread, SubmitCodeReview};
use aurelius_domain::{ActorKind, AppError, CommandEnvelope, CommandResult, DomainError, ErrorCode, EventKind, EventNotice, ProjectId, UserId};
use aurelius_storage::StorageError;
use aurelius_storage::Tx as Transaction;
use aurelius_storage::code_review as store;
use aurelius_storage::events::{self, NewEvent};
use serde_json::json;

/// The aggregate of a branch's review in the event log.
pub const CODE_REVIEW_AGGREGATE: &str = "code_review";

/// Why a thread named in a command is not there.
fn no_thread() -> AppError {
    AppError::new(ErrorCode::NotFound, "This comment thread does not exist (anymore).")
}

/// One `code.review_changed` event for the branch, with the command's metadata.
fn changed(tx: &Transaction<'_>, ctx_meta: &serde_json::Value, actor: &str, command_id: &str, user: UserId, project: ProjectId, branch: &str) -> Result<EventNotice, StorageError> {
    let mut metadata = json!({ "project_id": project, "branch": branch, "user_id": user });
    if let (Some(m), Some(ctx)) = (metadata.as_object_mut(), ctx_meta.as_object()) {
        for (k, v) in ctx {
            m.entry(k.clone()).or_insert(v.clone());
        }
    }
    let aggregate_id = format!("{project}:{branch}");
    let kind = EventKind::CodeReviewChanged.as_str();
    let appended = events::append(
        tx,
        NewEvent {
            aggregate_type: CODE_REVIEW_AGGREGATE,
            aggregate_version: crate::chat_next_version(tx, CODE_REVIEW_AGGREGATE, &aggregate_id)?,
            aggregate_id: aggregate_id.clone(),
            event_type: kind,
            project_id: Some(project),
            actor_kind: ActorKind::User,
            actor_id: Some(actor.to_owned()),
            correlation_id: command_id.to_owned(),
            causation_id: None,
            payload_blob_id: None,
            metadata,
            dedupe_key: format!("cmd:{command_id}:0"),
        },
    )?;
    Ok(notice(appended.seq, kind, aggregate_id, Some(project)))
}

impl CommandService {
    pub(crate) fn start_code_thread(&self, ctx: &CommandContext, envelope: &CommandEnvelope, hash: String, cmd: &StartCodeThread) -> Result<Committed, AppError> {
        let user = require_board_member(ctx)?;
        let thread = cmd.validate().map_err(domain_error)?;
        let (command_id, meta, actor) = (envelope.command_id.to_string(), ctx.metadata(), ctx.principal_id.to_string());
        self.commit(envelope.command_id, hash, move |tx| {
            if !store::project_exists(tx, thread.project_id)? {
                return Ok(Err(domain_error(DomainError::ProjectNotFound(thread.project_id))));
            }
            let (thread_id, comment_id) = store::start_thread(tx, user, &thread, aurelius_storage::now_ms())?;
            let n = changed(tx, &meta, &actor, &command_id, user, thread.project_id, &thread.branch)?;
            Ok(Ok((CommandResult::CodeThreadStarted { thread_id, comment_id }, vec![n])))
        })
    }

    pub(crate) fn reply_code_thread(&self, ctx: &CommandContext, envelope: &CommandEnvelope, hash: String, cmd: &ReplyCodeThread) -> Result<Committed, AppError> {
        let user = require_board_member(ctx)?;
        let body = cmd.validate().map_err(domain_error)?;
        let thread_id = cmd.thread_id;
        let (command_id, meta, actor) = (envelope.command_id.to_string(), ctx.metadata(), ctx.principal_id.to_string());
        self.commit(envelope.command_id, hash, move |tx| {
            let Some(place) = store::thread_place(tx, thread_id)? else { return Ok(Err(no_thread())) };
            let comment_id = store::reply(tx, thread_id, user, &body, aurelius_storage::now_ms())?;
            let n = changed(tx, &meta, &actor, &command_id, user, place.project_id, &place.branch)?;
            Ok(Ok((CommandResult::CodeCommented { thread_id, comment_id }, vec![n])))
        })
    }

    pub(crate) fn resolve_code_thread(&self, ctx: &CommandContext, envelope: &CommandEnvelope, hash: String, cmd: &ResolveCodeThread) -> Result<Committed, AppError> {
        let user = require_board_member(ctx)?;
        let (thread_id, resolved) = (cmd.thread_id, cmd.resolved);
        let (command_id, meta, actor) = (envelope.command_id.to_string(), ctx.metadata(), ctx.principal_id.to_string());
        self.commit(envelope.command_id, hash, move |tx| {
            let Some(place) = store::thread_place(tx, thread_id)? else { return Ok(Err(no_thread())) };
            if !store::set_resolved(tx, thread_id, resolved)? {
                return Ok(Ok((CommandResult::CodeThreadResolved { thread_id, resolved, changed: false }, Vec::new())));
            }
            let n = changed(tx, &meta, &actor, &command_id, user, place.project_id, &place.branch)?;
            Ok(Ok((CommandResult::CodeThreadResolved { thread_id, resolved, changed: true }, vec![n])))
        })
    }

    pub(crate) fn set_code_viewed(&self, ctx: &CommandContext, envelope: &CommandEnvelope, hash: String, cmd: &SetCodeViewed) -> Result<Committed, AppError> {
        let user = require_board_member(ctx)?;
        let viewed = cmd.validate().map_err(domain_error)?;
        let (command_id, meta, actor) = (envelope.command_id.to_string(), ctx.metadata(), ctx.principal_id.to_string());
        self.commit(envelope.command_id, hash, move |tx| {
            if !store::project_exists(tx, viewed.project_id)? {
                return Ok(Err(domain_error(DomainError::ProjectNotFound(viewed.project_id))));
            }
            if !store::set_viewed(tx, user, &viewed)? {
                return Ok(Ok((CommandResult::CodeViewedSet { viewed: viewed.viewed, changed: false }, Vec::new())));
            }
            let n = changed(tx, &meta, &actor, &command_id, user, viewed.project_id, &viewed.branch)?;
            Ok(Ok((CommandResult::CodeViewedSet { viewed: viewed.viewed, changed: true }, vec![n])))
        })
    }

    pub(crate) fn submit_code_review(&self, ctx: &CommandContext, envelope: &CommandEnvelope, hash: String, cmd: &SubmitCodeReview) -> Result<Committed, AppError> {
        let user = require_board_member(ctx)?;
        let review = cmd.validate().map_err(domain_error)?;
        let (command_id, meta, actor) = (envelope.command_id.to_string(), ctx.metadata(), ctx.principal_id.to_string());
        self.commit(envelope.command_id, hash, move |tx| {
            if !store::project_exists(tx, review.project_id)? {
                return Ok(Err(domain_error(DomainError::ProjectNotFound(review.project_id))));
            }
            // A comment or a request for changes says something: a summary or a line comment.
            if review.verdict != CodeVerdict::Approved && review.body.is_empty() && store::pending_count(tx, user, review.project_id, &review.branch)? == 0 {
                return Ok(Err(domain_error(DomainError::Required { field: "body".into() })));
            }
            let (review_id, comments) = store::submit(tx, user, &review, aurelius_storage::now_ms())?;
            let n = changed(tx, &meta, &actor, &command_id, user, review.project_id, &review.branch)?;
            Ok(Ok((CommandResult::CodeReviewSubmitted { review_id, comments }, vec![n])))
        })
    }
}
