//! The workspaces each board member pins to their rail (migration 51, feature
//! `rail_pins`). A board member without a row in `user_rails` never chose: the
//! client then pins every open workspace. A choice is a whole ordered list.

use crate::StorageError;
use aurelius_domain::user::RailPinsView;
use aurelius_domain::{ProjectId, UserId};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

/// The rail of `user`: whether they chose, and their pins in rail order.
pub fn get(conn: &Connection, user: UserId) -> Result<RailPinsView, StorageError> {
    let chosen = conn.query_row("SELECT 1 FROM user_rails WHERE user_id = ?1", [user.to_string()], |_| Ok(())).optional()?.is_some();
    let mut stmt = conn.prepare("SELECT project_id FROM user_rail_pins WHERE user_id = ?1 ORDER BY position")?;
    let rows = stmt.query_map([user.to_string()], |r| r.get::<_, String>(0))?;
    let mut project_ids = Vec::new();
    for row in rows {
        if let Ok(id) = ProjectId::parse(&row?) {
            project_ids.push(id);
        }
    }
    Ok(RailPinsView { chosen, project_ids })
}

/// The ids in `ids` that name no project (archived ones exist).
pub fn unknown_projects(conn: &Connection, ids: &[ProjectId]) -> Result<Vec<ProjectId>, StorageError> {
    let mut stmt = conn.prepare("SELECT EXISTS(SELECT 1 FROM projects WHERE id = ?1)")?;
    let mut unknown = Vec::new();
    for id in ids {
        if !stmt.query_row([id.to_string()], |r| r.get::<_, bool>(0))? {
            unknown.push(*id);
        }
    }
    Ok(unknown)
}

/// Store `pins` (validated, without duplicates) as the rail of `user`. Whether
/// that changed anything: the same list a second time is no change.
pub fn set(tx: &Transaction<'_>, user: UserId, pins: &[ProjectId], now_ms: i64) -> Result<bool, StorageError> {
    let current = get(tx, user)?;
    if current.chosen && current.project_ids == pins {
        return Ok(false);
    }
    tx.execute(
        "INSERT INTO user_rails(user_id, chosen_at_ms) VALUES (?1, ?2) ON CONFLICT(user_id) DO UPDATE SET chosen_at_ms = excluded.chosen_at_ms",
        params![user.to_string(), now_ms],
    )?;
    tx.execute("DELETE FROM user_rail_pins WHERE user_id = ?1", [user.to_string()])?;
    let mut insert = tx.prepare("INSERT INTO user_rail_pins(user_id, project_id, position) VALUES (?1, ?2, ?3)")?;
    for (position, id) in pins.iter().enumerate() {
        insert.execute(params![user.to_string(), id.to_string(), position as i64])?;
    }
    Ok(true)
}
