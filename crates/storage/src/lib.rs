//! Persistence for the daemon. Only the daemon links this crate.
//!
//! * One writer thread owns the write connection; every write runs in a
//!   single `BEGIN IMMEDIATE` transaction that commits or rolls back as a whole.
//! * Reads use a separate connection (WAL allows concurrent readers).
//! * Blobs are written to disk *before* the transaction (temp file, fsync,
//!   atomic rename); a crash can leave an orphan file, never a half-visible row.

pub mod artifacts;
pub mod auto_allow;
pub mod management_mode;
pub mod backup;
pub mod blobs;
pub mod heartbeats;
pub mod conversations;
pub mod discussions;
pub mod threads;
pub mod message_edits;
pub mod conversation_sessions;
pub mod employees;
pub mod employee_chats;
pub mod inbox;
pub mod company_convert;
pub mod approvals;
pub mod proposals;
pub mod instruction_versions;
pub mod connections;
pub mod tasks;
pub mod ideas;
pub mod notes;
pub mod task_schedules;
pub mod agent_functions;
pub mod app_keys;
pub mod webhooks;
pub mod files;
pub mod timeline;
pub mod memory;
pub mod usage;
pub mod variants;
pub mod shares;
pub mod events;
pub mod import;
pub mod fresh_start;
pub mod migrate;
pub mod queries;
pub mod remote;
pub mod settings;
pub mod traces;
pub mod users;
pub mod user_sessions;
pub mod claude_logins;
pub mod company;
pub mod tickets;
pub mod followers;
pub mod vault;
pub mod guidelines;
pub mod server_name;

use rusqlite::{Connection, OpenFlags, Transaction, TransactionBehavior};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, TryLockError, mpsc};
use std::thread;

pub use blobs::{BlobStore, PreparedBlob};

/// Transaction handle passed to write closures.
pub type Tx<'a> = rusqlite::Transaction<'a>;
pub type Conn = rusqlite::Connection;

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("file error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("migration {version} was changed after it was applied (checksum mismatch)")]
    MigrationChecksum { version: i64 },
    #[error("database schema version {found} is not one this build can open (it knows up to {supported}); a database from an older Aurelius needs the aurelius-import tool, a newer one needs an update")]
    DatabaseTooNew { found: i64, supported: i64 },
    #[error("aggregate {aggregate_type}/{aggregate_id}: expected next version {expected}, store has {actual}")]
    AggregateVersion { aggregate_type: String, aggregate_id: String, expected: u64, actual: u64 },
    #[error("the storage writer has stopped")]
    WriterStopped,
    /// Raised by command logic inside a write closure; rolls back the transaction.
    #[error("{0}")]
    Rejected(String),
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

/// Apply per-connection pragmas required by migration 0001.
pub fn configure(conn: &Connection) -> Result<(), StorageError> {
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "FULL")?;
    conn.busy_timeout(std::time::Duration::from_millis(5000))?;
    Ok(())
}

type WriteJob = Box<dyn FnOnce(&mut Connection) + Send>;

/// Read connections: several clients and the runs read at once (WAL lets
/// readers work beside the writer), so one slow read no longer holds up the rest.
const READERS: usize = 4;

pub struct Store {
    writer: mpsc::Sender<WriteJob>,
    readers: Vec<Mutex<Connection>>,
    /// Which reader a read waits for when all are busy.
    next_reader: AtomicUsize,
    blobs: BlobStore,
    db_path: PathBuf,
}

impl Store {
    /// Open (and migrate) the store below `root`:
    /// `root/data/aurelius.sqlite` and `root/blobs/`.
    pub fn open(root: &Path) -> Result<Self, StorageError> {
        Self::open_until(root, migrate::supported_version())
    }

    /// Open with the migrations up to schema `until` only: the v0.6 import
    /// writes the last protocol 2 schema, which the next `open` converts.
    pub fn open_until(root: &Path, until: i64) -> Result<Self, StorageError> {
        let data_dir = root.join("data");
        std::fs::create_dir_all(&data_dir)?;
        let db_path = data_dir.join("aurelius.sqlite");
        let blobs = BlobStore::new(root.join("blobs"))?;

        let mut write_conn = Connection::open(&db_path)?;
        configure(&write_conn)?;
        migrate::run_until(&mut write_conn, Some(&blobs), until)?;
        if until >= conversations::GROUP_CHAT_SCHEMA && conversations::ensure_group_conversation(&write_conn)? {
            tracing::warn!("the group chat was missing; made it again");
        }

        let readers = (0..READERS)
            .map(|_| {
                let reader = Connection::open_with_flags(
                    &db_path,
                    OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX | OpenFlags::SQLITE_OPEN_URI,
                )?;
                reader.busy_timeout(std::time::Duration::from_millis(5000))?;
                Ok(Mutex::new(reader))
            })
            .collect::<Result<Vec<_>, StorageError>>()?;

        let (tx, rx) = mpsc::channel::<WriteJob>();
        thread::Builder::new().name("aurelius-db-writer".into()).spawn(move || {
            let mut conn = write_conn;
            for job in rx {
                job(&mut conn);
            }
        })?;

        Ok(Self { writer: tx, readers, next_reader: AtomicUsize::new(0), blobs, db_path })
    }

    pub fn blobs(&self) -> &BlobStore {
        &self.blobs
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    /// The installation root (`data/`, `blobs/`, `backups/` live below it).
    pub fn root(&self) -> &Path {
        self.db_path.parent().and_then(Path::parent).unwrap_or(Path::new("."))
    }

    /// Run `f` inside one immediate transaction on the writer thread.
    /// `Ok` commits; any `Err` rolls back everything `f` wrote.
    pub fn write<R, F>(&self, f: F) -> Result<R, StorageError>
    where
        R: Send + 'static,
        F: FnOnce(&Transaction<'_>) -> Result<R, StorageError> + Send + 'static,
    {
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        let job: WriteJob = Box::new(move |conn| {
            let result = (|| {
                let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
                let value = f(&tx)?;
                tx.commit()?;
                Ok(value)
            })();
            let _ = reply_tx.send(result);
        });
        self.writer.send(job).map_err(|_| StorageError::WriterStopped)?;
        reply_rx.recv().map_err(|_| StorageError::WriterStopped)?
    }

    /// Run `f` on a free read connection; when all are busy, wait for one in turn.
    /// Never call `read` again inside `f`.
    pub fn read<R>(&self, f: impl FnOnce(&Connection) -> Result<R, StorageError>) -> Result<R, StorageError> {
        for reader in &self.readers {
            match reader.try_lock() {
                Ok(conn) => return f(&conn),
                Err(TryLockError::Poisoned(poisoned)) => return f(&poisoned.into_inner()),
                Err(TryLockError::WouldBlock) => {}
            }
        }
        let turn = self.next_reader.fetch_add(1, Ordering::Relaxed) % self.readers.len();
        let conn = self.readers[turn].lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        f(&conn)
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod schema18_tests;
#[cfg(test)]
mod schema19_tests;
#[cfg(test)]
mod users_tests;
#[cfg(test)]
mod schema21_tests;
#[cfg(test)]
mod sessions_tests;
#[cfg(test)]
mod tickets_tests;
#[cfg(test)]
mod fresh_start_tests;
#[cfg(test)]
mod vault_tests;
#[cfg(test)]
mod schema22_tests;
#[cfg(test)]
mod schema23_tests;
#[cfg(test)]
mod schema24_tests;
#[cfg(test)]
mod schema25_tests;
#[cfg(test)]
mod schema26_tests;
#[cfg(test)]
mod schema27_tests;
#[cfg(test)]
mod schema28_tests;
#[cfg(test)]
mod schema32_tests;
#[cfg(test)]
mod schema33_tests;
#[cfg(test)]
mod schema34_tests;
#[cfg(test)]
mod schema35_tests;
#[cfg(test)]
mod schema36_tests;
#[cfg(test)]
mod schema37_tests;
#[cfg(test)]
mod schema29_tests;
#[cfg(test)]
mod schema30_tests;
#[cfg(test)]
mod schema31_tests;
#[cfg(test)]
mod threads_tests;
#[cfg(test)]
mod followers_tests;
#[cfg(test)]
mod message_edits_tests;
#[cfg(test)]
mod message_editable_tests;
#[cfg(test)]
mod guidelines_tests;
#[cfg(test)]
mod notifications_tests;
