//! Checksummed, versioned migrations.

use crate::{BlobStore, StorageError, now_ms};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

/// Work in Rust after a migration's SQL, in the same transaction (with the blob
/// store when the database belongs to a data folder).
pub type Then = fn(&Transaction<'_>, Option<&BlobStore>) -> Result<(), StorageError>;

pub struct Migration {
    pub version: i64,
    pub sql: &'static str,
    /// Rebuilds tables: runs with foreign keys off (the SQLite way to change a
    /// table), and `PRAGMA foreign_key_check` must be clean before it commits.
    pub rebuild: bool,
    pub then: Option<Then>,
}

const fn plain(version: i64, sql: &'static str) -> Migration {
    Migration { version, sql, rebuild: false, then: None }
}

/// v0.7 starts with one schema. A v0.6 database (versions up to 28) is "too new"
/// for it; its data comes over with the one-time `aurelius-import` tool.
/// 14 is v0.9's company (protocol 3.0): managers become employees; 15 deletes
/// the plain chats (every conversation has a project, task or employee); 16 adds
/// the audience, webhook and outbox of app keys; 17 the company's group chat
/// (conversation kind 'group', made by the migration itself); 18 is v0.11's
/// board (protocol 4): company, users, board columns, tickets, departments gone;
/// 19 the discussions: a channel per project, a discussion per ticket, and the
/// board member who wrote a message; 20 the read markers; 21 phase 3's logins:
/// user sessions, disabled users, Claude logins and whose login runs use; 22
/// phase 4's vault: encrypted items per scope and their append-only audit; 23
/// the guidelines: a library with append-only versions, links, proposals and a
/// snapshot per run (no content: the starter set is a seed step, B7); 25 the
/// Testing column and the company's tester (24 is the server's name, on its own
/// branch). A 14–17
/// database does not migrate into 18: `fresh_start` imports it.
pub const MIGRATIONS: &[Migration] = &[
    plain(1, include_str!("../../../migrations/0001_schema.sql")),
    plain(2, include_str!("../../../migrations/0002_artifacts.sql")),
    plain(3, include_str!("../../../migrations/0003_artifact_versions.sql")),
    plain(4, include_str!("../../../migrations/0004_notes.sql")),
    plain(5, include_str!("../../../migrations/0005_project_plugins.sql")),
    plain(6, include_str!("../../../migrations/0006_app_keys.sql")),
    plain(7, include_str!("../../../migrations/0007_idea_attachments.sql")),
    plain(8, include_str!("../../../migrations/0008_conversation_sessions.sql")),
    plain(9, include_str!("../../../migrations/0009_run_traces.sql")),
    plain(10, include_str!("../../../migrations/0010_session_system_text.sql")),
    plain(11, include_str!("../../../migrations/0011_instruction_proposals.sql")),
    plain(12, include_str!("../../../migrations/0012_task_variants.sql")),
    plain(13, include_str!("../../../migrations/0013_session_context_tokens.sql")),
    Migration { version: 14, sql: include_str!("../../../migrations/0014_company.sql"), rebuild: true, then: Some(crate::company_convert::convert) },
    Migration { version: 15, sql: include_str!("../../../migrations/0015_no_plain_chats.sql"), rebuild: true, then: None },
    plain(16, include_str!("../../../migrations/0016_management_channel.sql")),
    Migration { version: 17, sql: include_str!("../../../migrations/0017_group_chat.sql"), rebuild: true, then: None },
    Migration { version: 18, sql: include_str!("../../../migrations/0018_board.sql"), rebuild: true, then: None },
    Migration { version: 19, sql: include_str!("../../../migrations/0019_discussions.sql"), rebuild: true, then: None },
    plain(20, include_str!("../../../migrations/0020_discussion_reads.sql")),
    Migration { version: 21, sql: include_str!("../../../migrations/0021_multi_user_sessions.sql"), rebuild: true, then: None },
    plain(22, include_str!("../../../migrations/0022_vault.sql")),
    plain(23, include_str!("../../../migrations/0023_guidelines.sql")),
    plain(24, include_str!("../../../migrations/0024_server_name.sql")),
    Migration { version: 25, sql: include_str!("../../../migrations/0025_board_testing.sql"), rebuild: true, then: None },
    plain(26, include_str!("../../../migrations/0026_ticket_channels.sql")),
    plain(27, include_str!("../../../migrations/0027_thread_reads.sql")),
    plain(28, include_str!("../../../migrations/0028_notification_reads.sql")),
    plain(29, include_str!("../../../migrations/0029_working_together.sql")),
    plain(30, include_str!("../../../migrations/0030_quick_answers.sql")),
    plain(31, include_str!("../../../migrations/0031_idea_ticket.sql")),
    Migration { version: 32, sql: include_str!("../../../migrations/0032_web_clients.sql"), rebuild: true, then: None },
    plain(33, include_str!("../../../migrations/0033_employee_description.sql")),
    plain(34, include_str!("../../../migrations/0034_employee_access_level.sql")),
    plain(35, include_str!("../../../migrations/0035_usage_model_calls.sql")),
    plain(36, include_str!("../../../migrations/0036_employee_inbox.sql")),
    plain(37, include_str!("../../../migrations/0037_employee_chats.sql")),
    plain(38, include_str!("../../../migrations/0038_scheduled_messages.sql")),
    plain(39, include_str!("../../../migrations/0039_activity_log.sql")),
    plain(40, include_str!("../../../migrations/0040_design_links.sql")),
    plain(41, include_str!("../../../migrations/0041_quick_tasks.sql")),
    Migration { version: 42, sql: include_str!("../../../migrations/0042_ticket_history.sql"), rebuild: false, then: Some(crate::history::backfill) },
    Migration { version: 43, sql: include_str!("../../../migrations/0043_work_sessions.sql"), rebuild: true, then: None },
    plain(44, include_str!("../../../migrations/0044_session_dispatch.sql")),
    Migration { version: 45, sql: include_str!("../../../migrations/0045_no_employee_chats.sql"), rebuild: true, then: None },
    plain(46, include_str!("../../../migrations/0046_session_history.sql")),
    plain(47, include_str!("../../../migrations/0047_session_titles.sql")),
    plain(48, include_str!("../../../migrations/0048_design_sessions.sql")),
    plain(49, include_str!("../../../migrations/0049_session_own_branch.sql")),
    plain(50, include_str!("../../../migrations/0050_device_owner.sql")),
];

/// The last schema of protocol 2 (managers per project). The v0.6 import writes
/// into it; migration 14 converts it like any 2.x database.
pub const LAST_PROTOCOL_2_VERSION: i64 = 13;

/// Highest schema version this build knows.
pub fn supported_version() -> i64 {
    MIGRATIONS.last().map(|m| m.version).unwrap_or(0)
}

pub fn checksum(sql: &str) -> String {
    hex::encode(Sha256::digest(sql.as_bytes()))
}

pub fn run(conn: &mut Connection, blobs: Option<&BlobStore>) -> Result<(), StorageError> {
    run_until(conn, blobs, supported_version())
}

/// Apply the migrations up to `until`, each in its own transaction.
pub fn run_until(conn: &mut Connection, blobs: Option<&BlobStore>, until: i64) -> Result<(), StorageError> {
    let has_table: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='schema_migrations')",
        [],
        |row| row.get(0),
    )?;

    let supported = supported_version();
    let mut applied: HashMap<i64, String> = HashMap::new();
    if has_table {
        let found: Option<i64> = conn.query_row("SELECT MAX(version) FROM schema_migrations", [], |row| row.get(0))?;
        if let Some(found) = found.filter(|v| *v > supported) {
            return Err(StorageError::DatabaseTooNew { found, supported });
        }
        let mut stmt = conn.prepare("SELECT version, checksum FROM schema_migrations")?;
        applied = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?;
    }

    for migration in MIGRATIONS.iter().filter(|m| m.version <= until) {
        let sum = checksum(migration.sql);
        match applied.get(&migration.version) {
            Some(existing) if *existing == sum => continue,
            Some(_) => return Err(StorageError::MigrationChecksum { version: migration.version }),
            None => {}
        }
        tracing::info!(version = migration.version, "applying migration");
        // foreign_keys cannot change inside a transaction: switch it around it.
        if migration.rebuild {
            conn.pragma_update(None, "foreign_keys", "OFF")?;
        }
        let result = apply(conn, migration, &sum, blobs);
        if migration.rebuild {
            conn.pragma_update(None, "foreign_keys", "ON")?;
        }
        result?;
    }
    Ok(())
}

fn apply(conn: &mut Connection, migration: &Migration, sum: &str, blobs: Option<&BlobStore>) -> Result<(), StorageError> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Exclusive)?;
    tx.execute_batch(migration.sql)?;
    if let Some(then) = migration.then {
        then(&tx, blobs)?;
    }
    if migration.rebuild {
        let broken: Option<String> = tx
            .query_row("SELECT \"table\" || ' -> ' || parent FROM pragma_foreign_key_check LIMIT 1", [], |r| r.get(0))
            .optional()?;
        if let Some(broken) = broken {
            return Err(StorageError::Rejected(format!("migration {} left a broken reference ({broken})", migration.version)));
        }
    }
    tx.execute(
        "INSERT INTO schema_migrations(version, checksum, applied_at_ms) VALUES (?1, ?2, ?3)",
        rusqlite::params![migration.version, sum, now_ms()],
    )?;
    tx.commit()?;
    Ok(())
}
