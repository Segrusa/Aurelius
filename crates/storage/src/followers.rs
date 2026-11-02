//! Who follows a ticket (migration 29, feature `ticket_follow`). A follower gets
//! the ticket's notifications; unfollowing removes the row, so nothing remembers
//! that someone left. Who follows when is decided by the application.

use crate::StorageError;
use aurelius_domain::ticket::FollowerView;
use aurelius_domain::{TicketId, UserId};
use rusqlite::{Connection, Transaction, params};
use std::collections::HashSet;

/// Why a board member follows a ticket, as the `reason` of `ticket_followers`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FollowReason {
    /// They made the ticket.
    Created,
    /// They were made responsible for it.
    Responsible,
    /// They were named in it.
    Mentioned,
    /// They chose to.
    Manual,
}

impl FollowReason {
    /// The value in `ticket_followers.reason` and `FollowerView::reason`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Responsible => "responsible",
            Self::Mentioned => "mentioned",
            Self::Manual => "manual",
        }
    }
}

/// `user` follows `ticket` from `now_ms`. Who follows already keeps their reason
/// and time. Whether this added them.
pub fn follow(tx: &Transaction<'_>, ticket: TicketId, user: UserId, reason: FollowReason, now_ms: i64) -> Result<bool, StorageError> {
    Ok(tx.execute(
        "INSERT INTO ticket_followers(ticket_id, user_id, reason, followed_at_ms) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(ticket_id, user_id) DO NOTHING",
        params![ticket.to_string(), user.to_string(), reason.as_str(), now_ms],
    )? == 1)
}

/// `user` no longer follows `ticket`. Whether they did.
pub fn unfollow(tx: &Transaction<'_>, ticket: TicketId, user: UserId) -> Result<bool, StorageError> {
    Ok(tx.execute("DELETE FROM ticket_followers WHERE ticket_id = ?1 AND user_id = ?2", params![ticket.to_string(), user.to_string()])? == 1)
}

/// Whether `user` follows `ticket`.
pub fn is_following(conn: &Connection, ticket: TicketId, user: UserId) -> Result<bool, StorageError> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM ticket_followers WHERE ticket_id = ?1 AND user_id = ?2)",
        params![ticket.to_string(), user.to_string()],
        |r| r.get(0),
    )?)
}

/// `ticket.followers`: who follows `ticket`, first follower first. A disabled
/// board member stays listed.
pub fn list(conn: &Connection, ticket: TicketId) -> Result<Vec<FollowerView>, StorageError> {
    let mut stmt = conn.prepare(
        "SELECT f.user_id, u.username, u.disabled_at_ms IS NOT NULL, f.reason, f.followed_at_ms
         FROM ticket_followers f JOIN users u ON u.id = f.user_id WHERE f.ticket_id = ?1 ORDER BY f.followed_at_ms, u.username",
    )?;
    let rows = stmt.query_map([ticket.to_string()], |r| {
        Ok(FollowerView {
            user_id: UserId::parse(&r.get::<_, String>(0)?).map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))?,
            username: r.get(1)?,
            disabled: r.get(2)?,
            reason: r.get(3)?,
            followed_at_ms: r.get(4)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// The followers of `ticket` a notification can reach: everyone but the disabled.
/// For `timeline::notify`, which also leaves out the actor.
pub fn active_followers(conn: &Connection, ticket: TicketId) -> Result<Vec<UserId>, StorageError> {
    let mut stmt = conn.prepare(
        "SELECT f.user_id FROM ticket_followers f JOIN users u ON u.id = f.user_id
         WHERE f.ticket_id = ?1 AND u.disabled_at_ms IS NULL ORDER BY f.followed_at_ms, u.username",
    )?;
    let rows = stmt.query_map([ticket.to_string()], |r| r.get::<_, String>(0))?;
    let mut out = Vec::new();
    for row in rows {
        if let Ok(user) = UserId::parse(&row?) {
            out.push(user);
        }
    }
    Ok(out)
}

/// The tickets `user` follows, for the `following` of `ticket.my_tickets`.
pub fn following(conn: &Connection, user: UserId) -> Result<HashSet<TicketId>, StorageError> {
    let mut stmt = conn.prepare("SELECT ticket_id FROM ticket_followers WHERE user_id = ?1")?;
    let rows = stmt.query_map([user.to_string()], |r| r.get::<_, String>(0))?;
    let mut out = HashSet::new();
    for row in rows {
        if let Ok(ticket) = TicketId::parse(&row?) {
            out.insert(ticket);
        }
    }
    Ok(out)
}
