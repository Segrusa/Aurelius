//! Who follows a ticket (US-A2, feature `ticket_follow`): `ticket.follow` and the
//! automatic follow on a creation, a handover and a new mention. Each follow is in
//! the transaction of what caused it.
//!
//! `ticket.follow_changed` has its own aggregate (`FOLLOW_AGGREGATE`, the ticket's id
//! with a version of its own), so following never changes the ticket's version: a
//! client that does not accept the event would otherwise get a version conflict on
//! its next write, for a change it can never hear about.

use crate::run::events::{Ev, emit};
use crate::ticket::require_board_member;
use crate::{CommandContext, CommandService, Committed, domain_error};
use aurelius_domain::{
    ActorKind, AppError, CommandEnvelope, CommandResult, DomainError, ErrorCode, EventKind,
    EventNotice, ProjectId, TicketId, UserId, ticket::FollowTicket,
};
use aurelius_storage::{
    StorageError, Tx, conversations as conv, discussions,
    followers::{self, FollowReason},
    now_ms, tickets as store, users,
};
use rusqlite::Connection;
use serde_json::{Value, json};

/// The aggregate of `ticket.follow_changed`; its id is the ticket's.
pub(crate) const FOLLOW_AGGREGATE: &str = "ticket_follow";

/// Why the active Responsible cannot unfollow (B10).
pub(crate) const RESPONSIBLE_STAYS: &str =
    "You are responsible for this ticket; hand it over before you unfollow it.";

/// Who caused a follow, as its event names them.
pub(crate) struct Cause<'a> {
    pub actor: ActorKind,
    pub actor_id: Option<String>,
    pub correlation: &'a str,
    pub meta: Value,
}

/// `ticket.follow_changed` of a follower that came or left.
fn changed(
    tx: &Tx<'_>,
    ticket: TicketId,
    project: ProjectId,
    user: UserId,
    on: bool,
    reason: Option<FollowReason>,
    cause: &Cause<'_>,
) -> Result<EventNotice, StorageError> {
    let mut metadata = cause.meta.clone();
    if let Value::Object(meta) = &mut metadata {
        meta.insert("user_id".into(), json!(user));
        meta.insert("on".into(), json!(on));
        meta.insert("reason".into(), json!(reason.map(FollowReason::as_str)));
    }
    let aggregate_id = ticket.to_string();
    let dedupe = format!(
        "{FOLLOW_AGGREGATE}:{ticket}:{}",
        conv::next_version(tx, FOLLOW_AGGREGATE, &aggregate_id)?
    );
    let (_, mut notice) = emit(
        tx,
        Ev {
            aggregate_type: FOLLOW_AGGREGATE,
            aggregate_id,
            event_type: EventKind::TicketFollowChanged.as_str(),
            actor: cause.actor,
            actor_id: cause.actor_id.clone(),
            correlation: cause.correlation.to_owned(),
            payload: None,
            metadata,
            dedupe,
        },
    )?;
    notice.project_id = Some(project);
    Ok(notice)
}

/// `user` follows `ticket` now, with its event; nothing when they already did
/// (they keep their reason and time).
pub(crate) fn follow(
    tx: &Tx<'_>,
    ticket: TicketId,
    project: ProjectId,
    user: UserId,
    reason: FollowReason,
    cause: &Cause<'_>,
) -> Result<Option<EventNotice>, StorageError> {
    if !followers::follow(tx, ticket, user, reason, now_ms())? {
        return Ok(None);
    }
    changed(tx, ticket, project, user, true, Some(reason), cause).map(Some)
}

/// The active board members `text` names with `@username`.
pub(crate) fn mentioned_board_members(
    conn: &Connection,
    text: &str,
) -> Result<Vec<UserId>, StorageError> {
    Ok(users::list(conn)?
        .into_iter()
        .filter(|u| !u.disabled() && discussions::mentions(text, &u.username))
        .filter_map(|u| UserId::parse(&u.id).ok())
        .collect())
}

/// Whom a new description names that the old one did not: a typo fixed by someone
/// else must not bring back who unfollowed (US-A2).
pub(crate) fn newly_mentioned(
    conn: &Connection,
    old: Option<&str>,
    new: &str,
) -> Result<Vec<UserId>, StorageError> {
    let before = old.map_or(Ok(Vec::new()), |text| mentioned_board_members(conn, text))?;
    Ok(mentioned_board_members(conn, new)?
        .into_iter()
        .filter(|u| !before.contains(u))
        .collect())
}

/// Everyone a message in a ticket's discussion names follows the ticket.
pub(crate) fn follow_mentioned(
    tx: &Tx<'_>,
    ticket: TicketId,
    project: ProjectId,
    text: &str,
    cause: &Cause<'_>,
) -> Result<Vec<EventNotice>, StorageError> {
    let mut notices = Vec::new();
    for user in mentioned_board_members(tx, text)? {
        notices.extend(follow(
            tx,
            ticket,
            project,
            user,
            FollowReason::Mentioned,
            cause,
        )?);
    }
    Ok(notices)
}

impl CommandService {
    /// `ticket.follow`: the board member of the session starts or stops following.
    /// Idempotent; only a real change gives an event. The active Responsible cannot
    /// unfollow (B10). Closed tickets too: it changes the follower, not the ticket.
    pub(crate) fn follow_ticket(
        &self,
        ctx: &CommandContext,
        envelope: &CommandEnvelope,
        hash: String,
        cmd: &FollowTicket,
    ) -> Result<Committed, AppError> {
        let user = require_board_member(ctx)?;
        let (ticket, on) = (cmd.ticket_id, cmd.on);
        let meta = ctx.metadata();
        let actor_id = Some(ctx.principal_id.to_string());
        let correlation = envelope.command_id.to_string();
        self.commit(envelope.command_id, hash, move |tx| {
            let Some(state) = store::state(tx, ticket)? else {
                return Ok(Err(domain_error(DomainError::TicketNotFound(ticket))));
            };
            let cause = Cause {
                actor: ActorKind::User,
                actor_id,
                correlation: &correlation,
                meta,
            };
            let notice = if on {
                follow(
                    tx,
                    ticket,
                    state.project_id,
                    user,
                    FollowReason::Manual,
                    &cause,
                )?
            } else {
                if store::responsible(tx, ticket)?.is_some_and(|r| r.user_id == user && !r.disabled)
                {
                    return Ok(Err(AppError::new(ErrorCode::Validation, RESPONSIBLE_STAYS)));
                }
                match followers::unfollow(tx, ticket, user)? {
                    true => Some(changed(
                        tx,
                        ticket,
                        state.project_id,
                        user,
                        false,
                        None,
                        &cause,
                    )?),
                    false => None,
                }
            };
            let changed = notice.is_some();
            Ok(Ok((
                CommandResult::TicketFollowChanged {
                    ticket_id: ticket,
                    on,
                    changed,
                },
                notice.into_iter().collect(),
            )))
        })
    }
}
