//! A ticket's channel (feature `ticket_channels`, AUR-CHAT-001). It is the
//! ticket's discussion (kind `ticket`, made with the ticket): no copy, one
//! source of messages. Its name is computed when read (`ticket_channel_name`).
//!
//! * Members: the board member who made the ticket, the Owner (`discussions::owners`),
//!   the owner employee and the main task's employee. A new owner joins; nobody
//!   is ever removed.
//! * Closed: on Done or archived, the channel is archived (read-only, still
//!   listed and searchable); leaving Done reopens it. Never deleted.
//! * Its events are system lines in the channel only.

use super::discussion::{Discussion, actor_name, of_ticket, system_line};
use aurelius_domain::ticket::{Actor, ColumnKind};
use aurelius_domain::views::{SystemLine, SystemLineKind, system_action};
use aurelius_domain::{EventNotice, TicketId, UserId};
use aurelius_storage::discussions::{self, Member};
use aurelius_storage::{StorageError, Tx};
use rusqlite::OptionalExtension;

/// A new ticket's channel opens: its creator and the Owner join (the owner
/// employee too, by `sync`).
pub(crate) fn opened(tx: &Tx<'_>, ticket: TicketId, creator: UserId) -> Result<Vec<EventNotice>, StorageError> {
    opened_by(tx, ticket, creator, Actor::BoardMember { user_id: creator })
}

/// `opened` for a ticket that `by` made for the board member `creator` (an employee's `create_ticket`).
pub(crate) fn opened_by(tx: &Tx<'_>, ticket: TicketId, creator: UserId, by: Actor) -> Result<Vec<EventNotice>, StorageError> {
    let Some(d) = of_ticket(tx, ticket)? else { return Ok(Vec::new()) };
    let now = aurelius_storage::now_ms();
    discussions::join(tx, d.id, Member::User(creator), now)?;
    for owner in discussions::owners(tx)? {
        discussions::join(tx, d.id, Member::User(owner), now)?;
    }
    sync(tx, ticket, Some(by))
}

/// Bring the channel of `ticket` in line with the ticket, after anything that
/// changed it (`by`: who): its owner and its main task's employee join; on Done
/// or archived it closes (a line here); off Done it reopens
/// (a line here). Nothing changes when it already matches, so calling it again is safe.
pub(crate) fn sync(tx: &Tx<'_>, ticket: TicketId, by: Option<Actor>) -> Result<Vec<EventNotice>, StorageError> {
    let Some(d) = of_ticket(tx, ticket)? else { return Ok(Vec::new()) };
    let Some(t) = d.ticket().cloned() else { return Ok(Vec::new()) };
    let now = aurelius_storage::now_ms();
    let worker: Option<String> = match t.root_task {
        Some(task) => tx.query_row("SELECT employee_id FROM tasks WHERE id = ?1", [task.to_string()], |r| r.get(0)).optional()?.flatten(),
        None => None,
    };
    for employee in t.owner.into_iter().chain(worker.and_then(|e| aurelius_domain::EmployeeId::parse(&e).ok())) {
        discussions::join(tx, d.id, Member::Employee(employee), now)?;
    }
    let closed = is_closed(t.column, t.archived);
    let mut notices = Vec::new();
    match (closed, discussions::archived_at(tx, d.id)?) {
        (true, None) => {
            discussions::set_ticket_channel_archived(tx, d.id, Some(now))?;
            let who = actor_name(tx, by)?;
            notices.extend(channel_line(tx, &d, SystemLineKind::Done, system_action::CLOSED, &who, &format!("#{} is closed · this channel can only be read now", t.number))?);
        }
        (false, Some(_)) => {
            discussions::set_ticket_channel_archived(tx, d.id, None)?;
            let who = actor_name(tx, by)?;
            notices.extend(channel_line(tx, &d, SystemLineKind::Moved, system_action::REOPENED, &who, &format!("{who} reopened #{} · this channel can be written in again", t.number))?);
        }
        _ => {}
    }
    Ok(notices)
}

/// Whether a ticket in `column` (`archived` or not) has a closed channel.
pub(crate) fn is_closed(column: ColumnKind, archived: bool) -> bool {
    archived || column == ColumnKind::Done
}

fn channel_line(tx: &Tx<'_>, d: &Discussion, kind: SystemLineKind, action: &str, by: &str, text: &str) -> Result<Vec<EventNotice>, StorageError> {
    let Some(t) = d.ticket() else { return Ok(Vec::new()) };
    let mut line = SystemLine::new(kind).with_action(action);
    (line.by, line.ticket_id, line.channel_id) = (Some(by.to_owned()), Some(t.id), Some(d.id));
    Ok(vec![system_line(tx, d.id, &line, Some(text))?])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_channel_is_closed_on_done_or_when_archived_and_open_anywhere_else() {
        assert!(is_closed(ColumnKind::Done, false));
        assert!(is_closed(ColumnKind::Todo, true));
        for column in [ColumnKind::Backlog, ColumnKind::Todo, ColumnKind::Doing, ColumnKind::PlanReview, ColumnKind::Testing, ColumnKind::Review] {
            assert!(!is_closed(column, false), "{column:?}");
        }
    }
}
