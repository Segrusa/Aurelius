//! The fresh start of v0.11 (protocol 4): a protocol 3 workspace (schema 14–17)
//! gets a new, empty database with a one-time import, not a migration that
//! empties tables (the event log is append-only and almost everything points
//! at it).
//!
//! 1. Detect: schema 14–17 is protocol 3. An older one (≤ 13) is first migrated
//!    to 17 on a copy, with the existing chain.
//! 2. Set aside: the pre-update backup, then `data/` and `blobs/` are renamed to
//!    `<root>/pre-v0.11/` (refused when that exists; never pruned). The identity
//!    and configuration files are copied back, so the installation, its pairings
//!    and remote access stay the same.
//! 3. A new database (`Store::open` runs 0001–0018) and the import in ONE
//!    transaction: what to keep is decided per table in `DECISIONS`; work and
//!    history stay behind in `pre-v0.11/`. Schedules, heartbeats and agent
//!    functions come over paused. Department memory becomes company (global)
//!    memory with `locator_json.from_department`; a duplicate is superseded.
//!
//! The old database is only ever read from a copy (SQLite's online backup) in a
//! private scratch folder, and blob files are only read. A real run keeps that
//! folder below `<root>/`; a dry run uses the temp folder and writes nothing
//! below `<root>/`, so it also works on a read-only copy.

use crate::{StorageError, Store, backup, events, memory, migrate, now_ms};
use aurelius_domain::{ActorKind, MemoryId};
use rusqlite::types::Value;
use rusqlite::{Connection, OpenFlags, OptionalExtension, Transaction, params, params_from_iter};
use serde_json::json;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Where the old `data/` and `blobs/` go (named after the release).
pub const BACKUP_DIR: &str = "pre-v0.11";
/// Present while the move and import run; a start that finds it puts the old
/// folders back first (the new database never got past its import).
const PENDING: &str = "import-pending";
pub const FIRST_PROTOCOL_3_VERSION: i64 = 14;
pub const LAST_PROTOCOL_3_VERSION: i64 = 17;
/// Copied from the old `data/`: the installation and its pairings stay the same.
pub const IDENTITY_FILES: [&str; 3] = ["host_id", "local_principal_id", "environment"];
/// Copied from the old `data/` too: remote access (on/off, push ids) stays as it was.
pub const CONFIG_FILES: [&str; 1] = ["remote.json"];
/// Below `<root>/`: one private folder per run, for the copy of the old database.
pub const SCRATCH_DIR: &str = "fresh-start-tmp";
/// A dry run's private folder in the temp folder starts with this.
pub const DRY_RUN_PREFIX: &str = "aurelius-fresh-start-";
/// Held by a running run; a folder whose lock is free was left by a crash.
const SCRATCH_LOCK: &str = ".lock";
/// A folder without a lock this old was left by a crash between its creation
/// and its lock; a younger one may be a run that is just starting.
const SCRATCH_UNLOCKED_STALE: std::time::Duration = std::time::Duration::from_secs(60);
/// Settings that point into the old database's event log or sessions: they start over.
const SETTINGS_LEFT: [&str; 3] = ["webhook_cursor", "sessions_root", "sessions_owned_since_ms"];

/// What happens to a table of the old (schema 17) database.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Copied with the same ids (some rows or columns filtered, see the import).
    Copy,
    /// Copied, but switched off: it would make work outside the board.
    Paused,
    /// Not copied: rebuilt from what was copied (index, sources, blob rows).
    Rebuilt,
    /// Not copied: work and history, kept only in `pre-v0.11/`.
    Left,
    /// Bookkeeping of the database itself; the new one has its own.
    Own,
}

/// Every table of the protocol 3 schema with its decision (impactanalyse §1.3,
/// plus Orion's addition: API keys, their functions and remote sessions come
/// over; function calls, heartbeat wakes, file grants, message sources, run
/// activity, task dependencies, task variants and command receipts do not).
/// The checklist test fails for a table that is missing here.
pub const DECISIONS: &[(&str, Decision)] = &[
    ("scopes", Decision::Copy),
    ("projects", Decision::Copy),
    ("employees", Decision::Copy),
    ("employee_instruction_versions", Decision::Copy),
    ("memory_items", Decision::Copy),
    ("config_overrides", Decision::Copy),
    ("settings", Decision::Copy),
    ("connections", Decision::Copy),
    ("employee_plugins", Decision::Copy),
    ("project_plugins", Decision::Copy),
    ("remote_clients", Decision::Copy),
    ("push_registrations", Decision::Copy),
    ("remote_sessions", Decision::Copy),
    ("app_keys", Decision::Copy),
    ("api_keys", Decision::Copy),
    ("api_key_functions", Decision::Copy),
    ("notes", Decision::Copy),
    ("ideas", Decision::Copy),
    ("heartbeats", Decision::Paused),
    ("recurring_tasks", Decision::Paused),
    ("agent_functions", Decision::Paused),
    ("blobs", Decision::Rebuilt),
    ("memory_sources", Decision::Rebuilt),
    ("retrieval_chunks", Decision::Rebuilt),
    ("chunk_fts", Decision::Rebuilt),
    ("chunk_fts_config", Decision::Rebuilt),
    ("chunk_fts_content", Decision::Rebuilt),
    ("chunk_fts_data", Decision::Rebuilt),
    ("chunk_fts_docsize", Decision::Rebuilt),
    ("chunk_fts_idx", Decision::Rebuilt),
    ("tasks", Decision::Left),
    ("task_dependencies", Decision::Left),
    ("task_variant_sets", Decision::Left),
    ("task_variants", Decision::Left),
    ("runs", Decision::Left),
    ("run_activity", Decision::Left),
    ("run_traces", Decision::Left),
    ("conversations", Decision::Left),
    ("conversation_sessions", Decision::Left),
    ("messages", Decision::Left),
    ("message_sources", Decision::Left),
    ("approvals", Decision::Left),
    ("artifacts", Decision::Left),
    ("artifact_versions", Decision::Left),
    ("attachments", Decision::Left),
    ("shares", Decision::Left),
    ("usage_records", Decision::Left),
    ("jobs", Decision::Left),
    ("notifications", Decision::Left),
    ("proposals", Decision::Left),
    ("events", Decision::Left),
    ("command_receipts", Decision::Left),
    ("function_calls", Decision::Left),
    ("heartbeat_wakes", Decision::Left),
    ("file_grants", Decision::Left),
    ("app_key_threads", Decision::Left),
    ("webhook_outbox", Decision::Left),
    ("departments", Decision::Left),
    ("schema_migrations", Decision::Own),
    ("sqlite_sequence", Decision::Own),
    // Made and dropped inside migration 14; only a failed 14 leaves them.
    ("_company_convert", Decision::Own),
    ("_company_convert_proposals", Decision::Own),
];

pub fn decision(table: &str) -> Option<Decision> {
    DECISIONS.iter().find(|(t, _)| *t == table).map(|(_, d)| *d)
}

#[derive(Debug, thiserror::Error)]
pub enum FreshStartError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("{0}")]
    Storage(#[from] StorageError),
    #[error("file error: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Refused(String),
}

/// What the fresh start copied, paused and left behind. Counts, plus the names
/// of departments (only shown when asked for).
#[derive(Debug, Default, Clone, PartialEq)]
pub struct FreshStartReport {
    /// The schema of the old database (before a copy was migrated to 17).
    pub from_version: i64,
    /// Rows copied per table.
    pub copied: BTreeMap<String, u64>,
    /// Rows that were on before and are paused now, per table.
    pub paused: BTreeMap<String, u64>,
    /// Department memories now in the company scope, in total and per department.
    pub from_department: u64,
    pub per_department: BTreeMap<String, u64>,
    /// Memories marked superseded because the company already had the same text.
    pub superseded_duplicates: u64,
    /// Rows of the old database that were not copied, per table.
    pub not_copied: BTreeMap<String, u64>,
    /// Rows of the old database rebuilt from what was copied (blob rows, the
    /// retrieval index, memory sources), per table.
    pub rebuilt: BTreeMap<String, u64>,
    /// Old tables without a decision (a real fresh start refuses these).
    pub undecided: Vec<String>,
    /// Rows skipped inside copied tables, and why.
    pub notes: Vec<String>,
    /// The employee named Orion, now the company's dispatcher.
    pub dispatcher: Option<String>,
    /// Rows in the new database after the import, per table.
    pub new_database: BTreeMap<String, u64>,
    /// Where the old folders went (a real fresh start only).
    pub backup_dir: Option<PathBuf>,
}

impl FreshStartReport {
    /// The report as text lines. `with_names` adds the department names.
    pub fn lines(&self, with_names: bool) -> Vec<String> {
        let mut out = vec![format!("old schema version: {}", self.from_version)];
        out.push("copied:".into());
        out.extend(self.copied.iter().map(|(t, n)| format!("  {t}: {n}")));
        out.push("paused (were on, now off):".into());
        out.extend(self.paused.iter().map(|(t, n)| format!("  {t}: {n}")));
        out.push(format!("memories from departments (now company memory): {}", self.from_department));
        if with_names {
            out.extend(self.per_department.iter().map(|(d, n)| format!("  {d}: {n}")));
        } else {
            out.push(format!("  departments: {}", self.per_department.len()));
        }
        out.push(format!("duplicate memories superseded: {}", self.superseded_duplicates));
        out.push(format!("default dispatcher: {}", if self.dispatcher.is_some() { "set" } else { "none" }));
        out.push("not copied (stay in pre-v0.11/):".into());
        out.extend(self.not_copied.iter().map(|(t, n)| format!("  {t}: {n}")));
        out.push("rebuilt for what was copied (old rows stay in pre-v0.11/):".into());
        out.extend(self.rebuilt.iter().map(|(t, n)| format!("  {t}: {n}")));
        if !self.undecided.is_empty() {
            out.push(format!("tables without a decision: {}", self.undecided.join(", ")));
        }
        out.push("new database:".into());
        out.extend(self.new_database.iter().map(|(t, n)| format!("  {t}: {n}")));
        out.extend(self.notes.iter().map(|n| format!("note: {n}")));
        out
    }
}

fn schema_version(conn: &Connection) -> Result<Option<i64>, FreshStartError> {
    let has: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='schema_migrations')", [], |r| r.get(0))?;
    Ok(if has { conn.query_row("SELECT MAX(version) FROM schema_migrations", [], |r| r.get(0))? } else { None })
}

fn needs_fresh_start(version: Option<i64>) -> Option<i64> {
    version.filter(|v| (1..=LAST_PROTOCOL_3_VERSION).contains(v))
}

/// The schema version of the database below `root` when it needs a fresh start
/// (protocol 3, or an older one that is migrated on a copy first); `None` when
/// there is no database, it is already v0.11, or it is not one this build reads.
pub fn detect(root: &Path) -> Result<Option<i64>, FreshStartError> {
    detect_in(&root.join("data"))
}

fn detect_in(data: &Path) -> Result<Option<i64>, FreshStartError> {
    let db = data.join("aurelius.sqlite");
    if !db.is_file() {
        return Ok(None);
    }
    Ok(needs_fresh_start(schema_version(&open_read_only(&db)?)?))
}

/// A read-only connection that leaves the folder as it is. Without a WAL
/// everything is in the file: read it as immutable, so not even an empty
/// `-wal` or `-shm` appears next to it. With a WAL, its content counts.
pub(crate) fn open_read_only(db: &Path) -> rusqlite::Result<Connection> {
    let wal = db.with_file_name(format!("{}-wal", db.file_name().unwrap_or_default().to_string_lossy())).exists();
    let uri = format!("file:{}?mode=ro{}", db.to_string_lossy().replace('%', "%25").replace('?', "%3f").replace('#', "%23"), if wal { "" } else { "&immutable=1" });
    Connection::open_with_flags(uri, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX | OpenFlags::SQLITE_OPEN_URI)
}

/// A private folder (0700) for one run, removed again. The copy holds secrets
/// (webhook secrets, verifiers), so nobody else may read it; one a crash left
/// behind is removed by the next run. A real run keeps it below
/// `<root>/fresh-start-tmp/`, a dry run in the temp folder.
pub(crate) struct Scratch {
    dir: PathBuf,
    lock: Option<std::fs::File>,
    /// `<root>/fresh-start-tmp/` goes too when no other run uses it.
    remove_parent: bool,
}

fn private_dir(path: &Path) -> std::io::Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(path)?;
    #[cfg(unix)]
    std::fs::set_permissions(path, std::os::unix::fs::PermissionsExt::from_mode(0o700))?;
    Ok(())
}

impl Scratch {
    /// Below `<root>/fresh-start-tmp/`: a real run (the root is written anyway).
    pub(crate) fn new(root: &Path) -> std::io::Result<Self> {
        let parent = root.join(SCRATCH_DIR);
        private_dir(&parent)?;
        Self::new_in(&parent, "", true)
    }

    /// In the temp folder: a dry run writes nothing below the root.
    pub(crate) fn in_temp() -> std::io::Result<Self> {
        Self::new_in(&std::env::temp_dir(), DRY_RUN_PREFIX, false)
    }

    fn new_in(parent: &Path, prefix: &str, remove_parent: bool) -> std::io::Result<Self> {
        let dir = parent.join(format!("{prefix}{}", uuid::Uuid::now_v7().simple()));
        // Not recursive: an existing folder (someone else's, in a shared temp
        // folder) is never taken over.
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
        builder.create(&dir)?;
        // From here on a failure removes it again.
        let mut scratch = Self { dir, lock: None, remove_parent };
        #[cfg(unix)]
        std::fs::set_permissions(&scratch.dir, std::os::unix::fs::PermissionsExt::from_mode(0o700))?;
        let lock = std::fs::File::create_new(scratch.dir.join(SCRATCH_LOCK))?;
        lock.lock()?;
        scratch.lock = Some(lock);
        scratch.remove_stale(parent, prefix);
        Ok(scratch)
    }

    /// Folders of runs that crashed: ours (same owner), lock free, or without a
    /// lock for long enough that no run is still creating it.
    fn remove_stale(&self, parent: &Path, prefix: &str) {
        #[cfg(unix)]
        let own = std::fs::symlink_metadata(&self.dir).map(|m| std::os::unix::fs::MetadataExt::uid(&m)).ok();
        let Ok(entries) = std::fs::read_dir(parent) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path == self.dir || !entry.file_name().to_string_lossy().starts_with(prefix) {
                continue;
            }
            let Ok(meta) = std::fs::symlink_metadata(&path) else { continue };
            if !meta.is_dir() {
                continue;
            }
            #[cfg(unix)]
            if Some(std::os::unix::fs::MetadataExt::uid(&meta)) != own {
                continue;
            }
            let stale = match std::fs::File::open(path.join(SCRATCH_LOCK)) {
                Ok(file) => file.try_lock().is_ok(),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => meta.modified().ok().and_then(|m| m.elapsed().ok()).is_some_and(|age| age >= SCRATCH_UNLOCKED_STALE),
                Err(_) => false,
            };
            if stale {
                let _ = std::fs::remove_dir_all(&path);
            }
        }
    }

    pub(crate) fn path(&self) -> &Path {
        &self.dir
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        // Closed first: Windows does not remove an open file.
        drop(self.lock.take());
        let _ = std::fs::remove_dir_all(&self.dir);
        if let Some(parent) = self.dir.parent().filter(|_| self.remove_parent) {
            // Only when no other run uses it.
            let _ = std::fs::remove_dir(parent);
        }
    }
}

/// A copy of the old database in `scratch`, at schema 17. Only reads the source.
fn working_copy(data: &Path, scratch: &Scratch) -> Result<(Connection, i64), FreshStartError> {
    let db = data.join("aurelius.sqlite");
    if !db.is_file() {
        return Err(FreshStartError::Refused(format!("no database at {}", db.display())));
    }
    let copy = scratch.path().join("old.sqlite");
    let (wal, shm) = (data.join("aurelius.sqlite-wal"), data.join("aurelius.sqlite-shm"));
    let mut conn = Connection::open(&copy)?;
    if wal.is_file() && !shm.exists() {
        // A WAL without its shared memory: nobody has the database open (a
        // crash), so the files cannot change while they are copied; a read-only
        // connection would have to create the shared memory in the source.
        drop(conn);
        std::fs::copy(&db, &copy)?;
        std::fs::copy(&wal, scratch.path().join("old.sqlite-wal"))?;
        conn = Connection::open(&copy)?;
    } else {
        // SQLite's online backup from a read-only connection: one consistent
        // snapshot, also while a daemon writes and checkpoints. Without a WAL the
        // file is all there is (immutable: not even a shared memory file appears).
        let source = open_read_only(&db)?;
        rusqlite::backup::Backup::new(&source, &mut conn)?.run_to_completion(256, std::time::Duration::from_millis(10), None)?;
    }
    crate::configure(&conn)?;
    let version = needs_fresh_start(schema_version(&conn)?).ok_or_else(|| {
        FreshStartError::Refused(format!("{} is not a protocol 3 (or older) Aurelius database", db.display()))
    })?;
    if version < FIRST_PROTOCOL_3_VERSION {
        // No blob store: migration 14 would only read department descriptions, and
        // departments do not come over.
        migrate::run_until(&mut conn, None, LAST_PROTOCOL_3_VERSION)?;
    }
    Ok((conn, version))
}

/// A dry run: the whole import into a private temporary folder (not below
/// `root`), from a copy of the database below `root`. Writes nothing below
/// `root`, so a read-only folder works. Predicts the real run: `None` when
/// there is nothing to do, refused when the real run would refuse; after an
/// interrupted run it reads the old folders the real run would put back.
pub fn dry_run(root: &Path) -> Result<Option<FreshStartReport>, FreshStartError> {
    let aside = root.join(BACKUP_DIR);
    let pending = aside.join(PENDING).exists();
    let source = |name: &str| if pending && aside.join(name).exists() { aside.join(name) } else { root.join(name) };
    let Some(version) = detect_in(&source("data"))? else { return Ok(None) };
    if !pending {
        refuse_when_aside_exists(&aside)?;
    }
    let scratch = Scratch::in_temp()?;
    let (old, _) = working_copy(&source("data"), &scratch)?;
    refuse_undecided(&old)?;
    let target = scratch.path().join("new");
    let mut report = import(&old, &source("blobs"), &target)?;
    report.from_version = version;
    Ok(Some(report))
}

/// An empty `pre-v0.11/` holds nothing: a crash right after creating it, or
/// while removing it after its marker, left it. It does not stand in the way.
fn is_empty_dir(path: &Path) -> bool {
    std::fs::read_dir(path).is_ok_and(|mut entries| entries.next().is_none())
}

fn refuse_when_aside_exists(aside: &Path) -> Result<(), FreshStartError> {
    if aside.exists() && !is_empty_dir(aside) {
        return Err(FreshStartError::Refused(format!("{} already exists; move it away so the old workspace can be set aside", aside.display())));
    }
    Ok(())
}

fn refuse_undecided(old: &Connection) -> Result<(), FreshStartError> {
    let undecided = undecided_tables(old)?;
    if !undecided.is_empty() {
        return Err(FreshStartError::Refused(format!("the old database has tables without an import decision: {}", undecided.join(", "))));
    }
    Ok(())
}

/// Startup step (before `Store::open`): when `root` holds a protocol 3 (or
/// older) database, set it aside in `pre-v0.11/` and import it into a fresh
/// v0.11 database. `pre_update` makes the pre-update backup first (the daemon
/// already made it just before). `None`: nothing to do.
pub fn upgrade(root: &Path, pre_update: Option<(&str, &str)>) -> Result<Option<FreshStartReport>, FreshStartError> {
    let aside = root.join(BACKUP_DIR);
    if aside.join(PENDING).exists() {
        put_back(root)?;
    } else if is_empty_dir(&aside) {
        // Left by a crash around the marker (see `is_empty_dir`); fails when not empty.
        std::fs::remove_dir(&aside)?;
    }
    let Some(version) = detect(root)? else { return Ok(None) };
    refuse_when_aside_exists(&aside)?;
    if let Some((app_version, environment)) = pre_update {
        backup::before_migration(root, app_version, environment)?;
    }
    // Read from a copy made before anything moves: a bad database changes nothing.
    let scratch = Scratch::new(root)?;
    let (old, _) = working_copy(&root.join("data"), &scratch)?;
    refuse_undecided(&old)?;

    // A crash between these two leaves an empty `pre-v0.11/`: the next start removes it.
    std::fs::create_dir_all(&aside)?;
    std::fs::write(aside.join(PENDING), b"")?;
    std::fs::rename(root.join("data"), aside.join("data"))?;
    if root.join("blobs").exists() {
        std::fs::rename(root.join("blobs"), aside.join("blobs"))?;
    } else {
        // So that undoing it takes away the new blobs/ too.
        std::fs::create_dir_all(aside.join("blobs"))?;
    }
    let result = (|| {
        std::fs::create_dir_all(root.join("data"))?;
        for name in IDENTITY_FILES.into_iter().chain(CONFIG_FILES) {
            let old = aside.join("data").join(name);
            if old.is_file() {
                std::fs::copy(&old, root.join("data").join(name))?;
            }
        }
        import(&old, &aside.join("blobs"), root)
    })();
    match result {
        Ok(mut report) => {
            std::fs::remove_file(aside.join(PENDING))?;
            report.from_version = version;
            report.backup_dir = Some(aside);
            Ok(Some(report))
        }
        Err(err) => {
            drop(old);
            if let Err(back) = put_back(root) {
                tracing::error!(error = %back, "could not put the old workspace back; it is in {}", BACKUP_DIR);
            }
            Err(err)
        }
    }
}

/// Undo an interrupted fresh start: the new folders go, the old ones come back.
/// A folder counts as new only when its old one is aside (the move may have
/// stopped between `data/` and `blobs/`).
fn put_back(root: &Path) -> Result<(), FreshStartError> {
    let aside = root.join(BACKUP_DIR);
    for name in ["data", "blobs"] {
        let (old, new) = (aside.join(name), root.join(name));
        if !old.exists() {
            continue;
        }
        if new.exists() {
            std::fs::remove_dir_all(&new)?;
        }
        std::fs::rename(&old, &new)?;
    }
    // A crash between these two leaves an empty `pre-v0.11/`: the next start removes it.
    std::fs::remove_file(aside.join(PENDING))?;
    std::fs::remove_dir(&aside)?;
    tracing::warn!("an interrupted fresh start was undone; the old workspace is back in place");
    Ok(())
}

fn tables(conn: &Connection) -> Result<Vec<String>, FreshStartError> {
    let mut stmt = conn.prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")?;
    let names = stmt.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?;
    Ok(names)
}

/// Tables of the old database that `DECISIONS` does not know.
pub fn undecided_tables(conn: &Connection) -> Result<Vec<String>, FreshStartError> {
    Ok(tables(conn)?.into_iter().filter(|t| decision(t).is_none()).collect())
}

fn count(conn: &Connection, table: &str) -> Result<u64, FreshStartError> {
    Ok(conn.query_row(&format!("SELECT COUNT(*) FROM \"{table}\""), [], |r| r.get::<_, i64>(0))? as u64)
}

fn columns(conn: &Connection, table: &str) -> Result<Vec<String>, FreshStartError> {
    let mut stmt = conn.prepare(&format!("SELECT name FROM pragma_table_info('{table}') ORDER BY cid"))?;
    let names = stmt.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?;
    Ok(names)
}

/// Rows of an old table, by column name.
struct Rows {
    columns: Vec<String>,
    rows: Vec<Vec<Value>>,
}

impl Rows {
    fn read(old: &Connection, table: &str, filter: &str) -> Result<Self, FreshStartError> {
        let columns = columns(old, table)?;
        let list = columns.iter().map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(", ");
        let mut stmt = old.prepare(&format!("SELECT {list} FROM \"{table}\" WHERE {filter} ORDER BY rowid"))?;
        let n = columns.len();
        let rows = stmt.query_map([], |r| (0..n).map(|i| r.get::<_, Value>(i)).collect::<Result<Vec<_>, _>>())?.collect::<Result<_, _>>()?;
        Ok(Self { columns, rows })
    }

    fn at(&self, column: &str) -> usize {
        self.columns.iter().position(|c| c == column).unwrap_or_else(|| panic!("no column {column}"))
    }

    fn text(row: &[Value], i: usize) -> Option<String> {
        match &row[i] {
            Value::Text(s) => Some(s.clone()),
            _ => None,
        }
    }
}

/// Insert `rows` into the same table of the new database: the columns both
/// schemas have, minus `left`. Returns how many.
fn insert(tx: &Transaction<'_>, table: &str, rows: &Rows, left: &[&str]) -> Result<u64, FreshStartError> {
    let new: HashSet<String> = columns(tx, table)?.into_iter().collect();
    let keep: Vec<usize> = (0..rows.columns.len()).filter(|i| new.contains(&rows.columns[*i]) && !left.contains(&rows.columns[*i].as_str())).collect();
    let list = keep.iter().map(|i| format!("\"{}\"", rows.columns[*i])).collect::<Vec<_>>().join(", ");
    let marks = (1..=keep.len()).map(|i| format!("?{i}")).collect::<Vec<_>>().join(", ");
    let mut stmt = tx.prepare(&format!("INSERT INTO \"{table}\"({list}) VALUES ({marks})"))?;
    for row in &rows.rows {
        stmt.execute(params_from_iter(keep.iter().map(|i| &row[*i])))?;
    }
    Ok(rows.rows.len() as u64)
}

/// Blob rows and files are copied with the same id and storage key, only for
/// the rows that come over. Files are read, never moved, from `from`.
struct Blobs<'a> {
    old: &'a Connection,
    from: &'a Path,
    to: PathBuf,
    done: HashMap<String, bool>,
    texts: HashMap<String, String>,
}

impl Blobs<'_> {
    /// Copy blob `id`; `false` when its row or file is missing.
    fn copy(&mut self, tx: &Transaction<'_>, id: &str) -> Result<bool, FreshStartError> {
        if let Some(done) = self.done.get(id) {
            return Ok(*done);
        }
        let row = Rows::read(self.old, "blobs", &format!("id = '{}'", id.replace('\'', "''")))?;
        let found = match row.rows.first() {
            Some(r) => {
                let key = Rows::text(r, row.at("storage_key")).unwrap_or_default();
                match std::fs::read(self.from.join(&key)) {
                    Ok(bytes) => {
                        let path = self.to.join(&key);
                        std::fs::create_dir_all(path.parent().unwrap_or(&self.to))?;
                        // Written and flushed through one handle: Windows refuses to flush a
                        // file opened for reading only ("Access is denied").
                        let mut file = std::fs::File::create(&path)?;
                        std::io::Write::write_all(&mut file, &bytes)?;
                        file.sync_all()?;
                        self.texts.insert(id.to_string(), String::from_utf8_lossy(&bytes).into_owned());
                        insert(tx, "blobs", &row, &[])?;
                        true
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
                    Err(e) => return Err(e.into()),
                }
            }
            None => false,
        };
        self.done.insert(id.to_string(), found);
        Ok(found)
    }

    /// Copy the blobs in `columns` of every row; a missing one becomes NULL.
    fn copy_columns(&mut self, tx: &Transaction<'_>, rows: &mut Rows, columns: &[&str], what: &str, notes: &mut Vec<String>) -> Result<(), FreshStartError> {
        let mut missing = 0;
        for column in columns {
            let i = rows.at(column);
            for row in &mut rows.rows {
                if let Some(id) = Rows::text(row, i) {
                    if !self.copy(tx, &id)? {
                        row[i] = Value::Null;
                        missing += 1;
                    }
                }
            }
        }
        if missing > 0 {
            notes.push(format!("{missing} stored texts of {what} were missing; copied without them"));
        }
        Ok(())
    }
}

/// Open the new store below `target` and import `old` into it in one transaction.
fn import(old: &Connection, from_blobs: &Path, target: &Path) -> Result<FreshStartReport, FreshStartError> {
    if target.join("data").join("aurelius.sqlite").exists() {
        return Err(FreshStartError::Refused(format!("{} already holds a database", target.display())));
    }
    let mut report = FreshStartReport { undecided: undecided_tables(old)?, ..Default::default() };
    for table in tables(old)? {
        let into = match decision(&table) {
            Some(Decision::Left) => &mut report.not_copied,
            Some(Decision::Rebuilt) => &mut report.rebuilt,
            _ => continue,
        };
        let n = count(old, &table)?;
        if n > 0 {
            into.insert(table, n);
        }
    }
    let store = Store::open(target)?;
    let blob_root = store.blobs().root().to_path_buf();
    // The connection is not Send: the import runs on this thread, on its own
    // connection, in one transaction; the store's writer is idle meanwhile.
    let mut conn = Connection::open(store.db_path())?;
    crate::configure(&conn)?;
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    // Checked at the commit: the global scope keeps its old id, reports_to and
    // other references are filled in any order.
    tx.pragma_update(None, "defer_foreign_keys", "ON")?;
    let mut blobs = Blobs { old, from: from_blobs, to: blob_root, done: HashMap::new(), texts: HashMap::new() };
    apply(old, &tx, &mut blobs, &mut report)?;
    let broken: Option<String> = tx
        .query_row("SELECT \"table\" || ' -> ' || parent FROM pragma_foreign_key_check LIMIT 1", [], |r| r.get(0))
        .optional()?;
    if let Some(broken) = broken {
        return Err(FreshStartError::Refused(format!("the import left a broken reference ({broken})")));
    }
    // Counted before the commit: nothing after it can fail a done import.
    for table in ["projects", "employees", "memory_items", "tasks", "conversations", "messages", "runs", "events", "users", "tickets", "board_columns", "company"] {
        report.new_database.insert(table.into(), count(&tx, table)?);
    }
    tx.commit()?;
    drop(store);
    Ok(report)
}

fn apply(old: &Connection, tx: &Transaction<'_>, blobs: &mut Blobs<'_>, report: &mut FreshStartReport) -> Result<(), FreshStartError> {
    let now = now_ms();
    let mut copied: BTreeMap<String, u64> = BTreeMap::new();

    // One audit event stands for the import: memory sources and run defaults point at it.
    let event = events::append(
        tx,
        events::NewEvent {
            aggregate_type: "import",
            aggregate_id: BACKUP_DIR.into(),
            aggregate_version: 1,
            event_type: "memory.accepted",
            project_id: None,
            actor_kind: ActorKind::System,
            actor_id: None,
            correlation_id: "fresh-start".into(),
            causation_id: None,
            payload_blob_id: None,
            metadata: json!({ "imported_from": BACKUP_DIR, "reason": "memory.imported" }),
            dedupe_key: "fresh-start".into(),
        },
    )?
    .id
    .to_string();

    // Scopes: the global one keeps its old id (and stays the company's); project
    // and employee scopes come over, department scopes do not.
    let old_global: Option<String> = old.query_row("SELECT id FROM scopes WHERE kind = 'global'", [], |r| r.get(0)).optional()?;
    let global = match old_global {
        Some(id) => {
            let new: String = tx.query_row("SELECT id FROM scopes WHERE kind = 'global'", [], |r| r.get(0))?;
            tx.execute("UPDATE scopes SET id = ?2 WHERE id = ?1", params![new, id])?;
            tx.execute("UPDATE company SET scope_id = ?2 WHERE scope_id = ?1", params![new, id])?;
            id
        }
        None => tx.query_row("SELECT id FROM scopes WHERE kind = 'global'", [], |r| r.get(0))?,
    };
    let scopes = Rows::read(old, "scopes", "kind IN ('project','employee')")?;
    copied.insert("scopes".into(), insert(tx, "scopes", &scopes, &[])? + 1);
    let department_scopes: HashMap<String, String> = {
        let mut stmt = old.prepare("SELECT s.id, COALESCE(d.name, 'department') FROM scopes s LEFT JOIN departments d ON d.scope_id = s.id WHERE s.kind = 'department'")?;
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?
    };

    let mut projects = Rows::read(old, "projects", "1")?;
    blobs.copy_columns(tx, &mut projects, &["description_blob_id"], "projects", &mut report.notes)?;
    copied.insert("projects".into(), insert(tx, "projects", &projects, &["dispatcher_employee_id"])?);

    // Employees without their department; reports_to after every row exists (deferred).
    let mut employees = Rows::read(old, "employees", "1")?;
    blobs.copy_columns(tx, &mut employees, &["identity_blob_id", "instructions_blob_id"], "employees", &mut report.notes)?;
    copied.insert("employees".into(), insert(tx, "employees", &employees, &["department_id"])?);
    // Each employee gets a new, empty main chat (conversations stay behind).
    let (id_at, name_at) = (employees.at("id"), employees.at("name"));
    for row in &employees.rows {
        tx.execute(
            "INSERT INTO conversations(id, kind, project_id, employee_id, title, version, created_at_ms) VALUES (?1, 'employee', NULL, ?2, ?3, 1, ?4)",
            params![aurelius_domain::ConversationId::new().to_string(), row[id_at], row[name_at], now],
        )?;
    }

    let mut versions = Rows::read(old, "employee_instruction_versions", "1")?;
    blobs.copy_columns(tx, &mut versions, &["instructions_blob_id"], "instruction versions", &mut report.notes)?;
    let proposal_at = versions.at("proposal_id");
    for row in &mut versions.rows {
        // Proposals stay behind.
        row[proposal_at] = Value::Null;
    }
    copied.insert("employee_instruction_versions".into(), insert(tx, "employee_instruction_versions", &versions, &[])?);

    import_memory(old, tx, blobs, &global, &department_scopes, &event, &mut copied, report)?;

    // Run defaults per scope, but not those of a department.
    let mut overrides = Rows::read(old, "config_overrides", "scope_id NOT IN (SELECT id FROM scopes WHERE kind = 'department')")?;
    let source_at = overrides.at("source_event_id");
    for row in &mut overrides.rows {
        row[source_at] = Value::Text(event.clone());
    }
    copied.insert("config_overrides".into(), insert(tx, "config_overrides", &overrides, &[])?);
    let department_overrides: u64 = old.query_row("SELECT COUNT(*) FROM config_overrides WHERE scope_id IN (SELECT id FROM scopes WHERE kind = 'department')", [], |r| r.get::<_, i64>(0))? as u64;
    if department_overrides > 0 {
        report.notes.push(format!("{department_overrides} run defaults of departments were not copied"));
    }

    let left = SETTINGS_LEFT.iter().map(|k| format!("'{k}'")).collect::<Vec<_>>().join(",");
    let settings = Rows::read(old, "settings", &format!("key NOT IN ({left})"))?;
    copied.insert("settings".into(), insert(tx, "settings", &settings, &[])?);
    // Claude Code sessions of the old workspace are not this database's.
    crate::settings::put(tx, "sessions_owned_since_ms", &now.to_string(), now)?;

    for table in ["connections", "employee_plugins", "project_plugins", "remote_clients", "push_registrations", "remote_sessions", "app_keys", "api_keys", "notes"] {
        let rows = Rows::read(old, table, "1")?;
        copied.insert(table.into(), insert(tx, table, &rows, &[])?);
    }

    // Open ideas only, without their task.
    let mut ideas = Rows::read(old, "ideas", "status = 'open'")?;
    let task_at = ideas.at("task_id");
    for row in &mut ideas.rows {
        row[task_at] = Value::Null;
    }
    copied.insert("ideas".into(), insert(tx, "ideas", &ideas, &[])?);

    // Paused: they would make work outside the board. A heartbeat's cursor
    // pointed into the old event log.
    let mut paused: BTreeMap<String, u64> = BTreeMap::new();
    let mut heartbeats = Rows::read(old, "heartbeats", "1")?;
    let (enabled_at, cursor_at, waiting_at) = (heartbeats.at("enabled"), heartbeats.at("cursor_seq"), heartbeats.at("waiting_reason"));
    paused.insert("heartbeats".into(), heartbeats.rows.iter().filter(|r| r[enabled_at] == Value::Integer(1)).count() as u64);
    for row in &mut heartbeats.rows {
        row[enabled_at] = Value::Integer(0);
        row[cursor_at] = Value::Integer(0);
        row[waiting_at] = Value::Null;
    }
    copied.insert("heartbeats".into(), insert(tx, "heartbeats", &heartbeats, &[])?);

    let mut recurring = Rows::read(old, "recurring_tasks", "1")?;
    let (paused_at, last_task_at) = (recurring.at("paused"), recurring.at("last_task_id"));
    paused.insert("recurring_tasks".into(), recurring.rows.iter().filter(|r| r[paused_at] == Value::Integer(0)).count() as u64);
    for row in &mut recurring.rows {
        row[paused_at] = Value::Integer(1);
        row[last_task_at] = Value::Null;
    }
    copied.insert("recurring_tasks".into(), insert(tx, "recurring_tasks", &recurring, &[])?);

    let mut functions = Rows::read(old, "agent_functions", "1")?;
    let on_at = functions.at("enabled");
    paused.insert("agent_functions".into(), functions.rows.iter().filter(|r| r[on_at] == Value::Integer(1)).count() as u64);
    for row in &mut functions.rows {
        row[on_at] = Value::Integer(0);
    }
    copied.insert("agent_functions".into(), insert(tx, "agent_functions", &functions, &[])?);
    let key_functions = Rows::read(old, "api_key_functions", "1")?;
    copied.insert("api_key_functions".into(), insert(tx, "api_key_functions", &key_functions, &[])?);

    let skipped_ideas = count(old, "ideas")? - copied["ideas"];
    if skipped_ideas > 0 {
        report.notes.push(format!("{skipped_ideas} ideas that were done or dropped were not copied"));
    }

    // The default dispatcher: the employee named Orion, if there is one.
    let orion: Option<String> = tx
        .query_row(
            "SELECT id FROM employees WHERE name = 'Orion' COLLATE NOCASE ORDER BY status <> 'active', created_at_ms LIMIT 1",
            [],
            |r| r.get(0),
        )
        .optional()?;
    tx.execute("UPDATE company SET dispatcher_employee_id = ?1, updated_at_ms = ?2", params![orion, now])?;
    report.dispatcher = orion;
    report.copied = copied;
    report.paused = paused;
    Ok(())
}

/// Memories (active, proposed, superseded; not forgotten or rejected) with their
/// text. Department memory goes to the company (global) scope with
/// `locator_json.from_department`; when the company already has the same text,
/// the oldest stays and the other is superseded by it. The retrieval index is
/// built anew; one audit event is every memory's source.
fn import_memory(
    old: &Connection,
    tx: &Transaction<'_>,
    blobs: &mut Blobs<'_>,
    global: &str,
    department_scopes: &HashMap<String, String>,
    event: &str,
    copied_rows: &mut BTreeMap<String, u64>,
    report: &mut FreshStartReport,
) -> Result<(), FreshStartError> {
    let mut rows = Rows::read(
        old,
        "memory_items",
        "status IN ('active','proposed','superseded') AND scope_id IN (SELECT id FROM scopes WHERE kind IN ('global','project','employee','department'))",
    )?;
    let mut skipped = count(old, "memory_items")? - rows.rows.len() as u64;
    let (id_at, scope_at, blob_at, status_at, hash_at, locator_at, created_at, supersedes_at, conflicts_at, until_at) = (
        rows.at("id"),
        rows.at("scope_id"),
        rows.at("content_blob_id"),
        rows.at("status"),
        rows.at("content_hash"),
        rows.at("locator_json"),
        rows.at("created_at_ms"),
        rows.at("supersedes_id"),
        rows.at("conflicts_with_id"),
        rows.at("valid_until_ms"),
    );
    let mut texts: HashMap<String, String> = HashMap::new();
    let mut without_text = 0;
    let mut kept = Vec::with_capacity(rows.rows.len());
    for mut row in std::mem::take(&mut rows.rows) {
        let (Some(id), Some(blob)) = (Rows::text(&row, id_at), Rows::text(&row, blob_at)) else {
            skipped += 1;
            continue;
        };
        if !blobs.copy(tx, &blob)? {
            without_text += 1;
            continue;
        }
        let text = blobs.texts.get(&blob).cloned().unwrap_or_default();
        row[hash_at] = Value::Text(memory::hash(&text));
        if let Some(department) = Rows::text(&row, scope_at).and_then(|s| department_scopes.get(&s)) {
            let mut locator: serde_json::Value = Rows::text(&row, locator_at).and_then(|l| serde_json::from_str(&l).ok()).unwrap_or_else(|| json!({}));
            if !locator.is_object() {
                locator = json!({});
            }
            locator["from_department"] = json!(department);
            row[locator_at] = Value::Text(locator.to_string());
            row[scope_at] = Value::Text(global.to_string());
            report.from_department += 1;
            *report.per_department.entry(department.clone()).or_default() += 1;
        }
        texts.insert(id, text);
        kept.push(row);
    }
    if skipped > 0 {
        report.notes.push(format!("{skipped} memories were not copied (forgotten, rejected or without scope)"));
    }
    if without_text > 0 {
        report.notes.push(format!("{without_text} memories whose stored text is missing were not copied"));
    }
    rows.rows = kept;
    let copied: HashSet<String> = rows.rows.iter().filter_map(|r| Rows::text(r, id_at)).collect();
    // Links to memories that did not come over are dropped.
    for row in &mut rows.rows {
        for at in [supersedes_at, conflicts_at] {
            if Rows::text(row, at).is_some_and(|s| !copied.contains(&s)) {
                row[at] = Value::Null;
            }
        }
    }

    // Duplicates in the company scope after the move: the oldest live one stays.
    let live = |row: &Vec<Value>| matches!(Rows::text(row, status_at).as_deref(), Some("active" | "proposed"));
    let mut order: Vec<usize> = (0..rows.rows.len()).filter(|i| Rows::text(&rows.rows[*i], scope_at).as_deref() == Some(global) && live(&rows.rows[*i])).collect();
    order.sort_by_key(|i| (match &rows.rows[*i][created_at] { Value::Integer(n) => *n, _ => 0 }, Rows::text(&rows.rows[*i], id_at)));
    let mut first: HashMap<String, usize> = HashMap::new();
    let mut links: Vec<(usize, String)> = Vec::new();
    for i in order {
        let hash = Rows::text(&rows.rows[i], hash_at).unwrap_or_default();
        let moved = Rows::text(&rows.rows[i], locator_at).is_some_and(|l| l.contains("from_department"));
        match first.get(&hash) {
            Some(&keep) if moved || Rows::text(&rows.rows[keep], locator_at).is_some_and(|l| l.contains("from_department")) => {
                let dup = Rows::text(&rows.rows[i], id_at).unwrap_or_default();
                rows.rows[i][status_at] = Value::Text("superseded".into());
                // Like every supersede: valid until now.
                rows.rows[i][until_at] = Value::Integer(now_ms());
                links.push((keep, dup));
                report.superseded_duplicates += 1;
            }
            Some(_) => {}
            None => {
                first.insert(hash, i);
            }
        }
    }
    for (keep, dup) in links {
        if rows.rows[keep][supersedes_at] == Value::Null {
            rows.rows[keep][supersedes_at] = Value::Text(dup);
        }
    }

    copied_rows.insert("memory_items".into(), insert(tx, "memory_items", &rows, &[])?);
    for row in &rows.rows {
        let Some(id) = Rows::text(row, id_at) else { continue };
        tx.execute("INSERT INTO memory_sources(memory_id, event_id, locator_json) VALUES (?1, ?2, '{}')", params![id, event])?;
        if Rows::text(row, status_at).as_deref() == Some("active") {
            let memory_id = MemoryId::parse(&id).map_err(|_| StorageError::Rejected(format!("memory id {id} is not a valid id")))?;
            memory::index(tx, memory_id, &texts[&id])?;
        }
    }
    Ok(())
}
