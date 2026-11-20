//! Backups: a consistent SQLite snapshot (`VACUUM INTO` on the
//! writer thread) plus the blob files and a manifest. Secrets never enter a
//! backup: they live in the OS credential store. Restoring happens at startup,
//! into place, with background work paused until the user resumes.

use crate::{Store, StorageError, now_ms};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const AUTO_KEEP: usize = 7;
pub const MANUAL_KEEP: usize = 20;
const MANIFEST: &str = "manifest.json";
/// How long a restore keeps trying to move a folder that something still holds open.
const RESTORE_RENAME_WAIT_MS: u64 = 10_000;
/// The pause between two tries.
const RESTORE_RENAME_RETRY_MS: u64 = 100;

/// `rename`, tried again for a while when the answer is "Access is denied": on Windows a folder
/// cannot move while any file in it is open, and a virus scanner, an indexer or the previous
/// session's last handle often holds one for a moment.
fn rename_patiently(from: &Path, to: &Path) -> std::io::Result<()> {
    let mut waited = 0;
    loop {
        match std::fs::rename(from, to) {
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied && waited < RESTORE_RENAME_WAIT_MS => {
                std::thread::sleep(std::time::Duration::from_millis(RESTORE_RENAME_RETRY_MS));
                waited += RESTORE_RENAME_RETRY_MS;
            }
            done => return done,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub format: u32,
    pub created_at_ms: i64,
    pub app_version: String,
    pub schema_version: i64,
    pub environment: String,
    /// "auto" or "manual"
    pub kind: String,
    pub db_sha256: String,
    pub blob_count: u64,
    pub bytes: u64,
    /// Content forgotten after this backup was made was removed from it.
    #[serde(default)]
    pub scrubbed_at_ms: Option<i64>,
    pub note: String,
}

pub fn backups_dir(root: &Path) -> PathBuf {
    root.join("backups")
}

impl Store {
    /// Atomically consistent copy of the database (serialized with all writes).
    pub fn snapshot_to(&self, target: &Path) -> Result<(), StorageError> {
        let target = target.to_string_lossy().into_owned();
        let (reply_tx, reply_rx) = std::sync::mpsc::sync_channel(1);
        let job: Box<dyn FnOnce(&mut rusqlite::Connection) + Send> = Box::new(move |conn| {
            let result = conn.execute("VACUUM INTO ?1", [&target]).map(|_| ()).map_err(StorageError::from);
            let _ = reply_tx.send(result);
        });
        self.writer.send(job).map_err(|_| StorageError::WriterStopped)?;
        reply_rx.recv().map_err(|_| StorageError::WriterStopped)?
    }
}

fn copy_tree(from: &Path, to: &Path, count: &mut u64, bytes: &mut u64) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        if path.is_dir() {
            if name == "tmp" {
                continue;
            }
            copy_tree(&path, &to.join(&name), count, bytes)?;
        } else {
            *bytes += std::fs::copy(&path, to.join(&name))?;
            *count += 1;
        }
    }
    Ok(())
}

fn sha256_file(path: &Path) -> std::io::Result<String> {
    use sha2::Digest;
    Ok(hex::encode(sha2::Sha256::digest(std::fs::read(path)?)))
}

/// Create a backup under `<root>/backups/<kind>-<timestamp>`.
pub fn create(store: &Store, root: &Path, kind: &str, app_version: &str, environment: &str) -> Result<(PathBuf, Manifest), StorageError> {
    create_with(root, kind, app_version, environment, |target| store.snapshot_to(target))
}

/// Before migrating an existing database: a
/// backup of the old schema, made before the store is opened. Keeps 3.
pub fn before_migration(root: &Path, app_version: &str, environment: &str) -> Result<Option<PathBuf>, StorageError> {
    let db = root.join("data").join("aurelius.sqlite");
    if !db.exists() {
        return Ok(None);
    }
    let supported = crate::migrate::supported_version();
    let conn = rusqlite::Connection::open(&db)?;
    let has_table: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='schema_migrations')", [], |r| r.get(0))?;
    let current: i64 = if has_table { conn.query_row("SELECT COALESCE(MAX(version), 0) FROM schema_migrations", [], |r| r.get(0))? } else { 0 };
    if current == 0 || current >= supported {
        return Ok(None);
    }
    let (dir, _) = create_with(root, "pre-update", app_version, environment, |target| {
        conn.execute("VACUUM INTO ?1", [target.to_string_lossy()])?;
        Ok(())
    })?;
    Ok(Some(dir))
}

fn create_with(root: &Path, kind: &str, app_version: &str, environment: &str, snapshot: impl FnOnce(&Path) -> Result<(), StorageError>) -> Result<(PathBuf, Manifest), StorageError> {
    let stamp = now_ms();
    let dir = backups_dir(root).join(format!("{kind}-{stamp}"));
    let partial = backups_dir(root).join(format!(".partial-{kind}-{stamp}"));
    let _ = std::fs::remove_dir_all(&partial);
    std::fs::create_dir_all(partial.join("data"))?;
    let db = partial.join("data").join("aurelius.sqlite");
    snapshot(&db)?;
    let (mut count, mut bytes) = (0u64, 0u64);
    let blobs = root.join("blobs");
    if blobs.is_dir() {
        copy_tree(&blobs, &partial.join("blobs"), &mut count, &mut bytes)?;
    }
    bytes += std::fs::metadata(&db)?.len();
    let schema_version: i64 = rusqlite::Connection::open(&db)?.query_row("SELECT COALESCE(MAX(version), 0) FROM schema_migrations", [], |r| r.get(0))?;
    let manifest = Manifest {
        format: 1,
        created_at_ms: stamp,
        app_version: app_version.into(),
        schema_version,
        environment: environment.into(),
        kind: kind.into(),
        db_sha256: sha256_file(&db)?,
        blob_count: count,
        bytes,
        scrubbed_at_ms: None,
        note: "Not encrypted by Aurelius; protected by your Windows account. API keys are not included (they stay in the OS credential store).".into(),
    };
    std::fs::write(partial.join(MANIFEST), serde_json::to_vec_pretty(&manifest)?)?;
    // Only complete backups get their final name.
    std::fs::rename(&partial, &dir)?;
    prune(root, kind)?;
    Ok((dir, manifest))
}

pub fn list(root: &Path) -> Vec<(PathBuf, Manifest)> {
    let mut out: Vec<(PathBuf, Manifest)> = std::fs::read_dir(backups_dir(root))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .filter_map(|e| {
            let manifest: Manifest = serde_json::from_slice(&std::fs::read(e.path().join(MANIFEST)).ok()?).ok()?;
            Some((e.path(), manifest))
        })
        .collect();
    out.sort_by(|a, b| b.1.created_at_ms.cmp(&a.1.created_at_ms));
    out
}

/// Keep the newest automatic (7), manual (20) and pre-update (3) backups.
pub fn prune(root: &Path, kind: &str) -> Result<(), StorageError> {
    let keep = match kind {
        "auto" => AUTO_KEEP,
        "pre-update" => 3,
        _ => MANUAL_KEEP,
    };
    for (path, _) in list(root).into_iter().filter(|(_, m)| m.kind == kind).skip(keep) {
        std::fs::remove_dir_all(path)?;
    }
    Ok(())
}

/// Verify a backup before restoring it.
pub fn verify(dir: &Path) -> Result<Manifest, StorageError> {
    let manifest: Manifest = serde_json::from_slice(&std::fs::read(dir.join(MANIFEST))?)?;
    let db = dir.join("data").join("aurelius.sqlite");
    if sha256_file(&db)? != manifest.db_sha256 {
        return Err(StorageError::Rejected("the backup's database does not match its manifest".into()));
    }
    Ok(manifest)
}

/// The first schema of v0.11 (protocol 4, the board). An older backup is a
/// protocol 3 workspace: this build does not restore it (going back is the
/// `pre-v0.11/` folder with the previous build).
pub const FRESH_START_SCHEMA: i64 = 18;

/// Why this build does not restore this backup, if it does not.
pub fn restore_refusal(manifest: &Manifest) -> Option<String> {
    (manifest.schema_version < FRESH_START_SCHEMA).then(|| {
        format!(
            "This backup is from before v0.11. To go back, restore {} with the previous build.",
            crate::fresh_start::BACKUP_DIR
        )
    })
}

/// Pending restore, performed by the daemon at its next start (before opening the store).
pub fn request_restore(root: &Path, backup: &Path) -> Result<(), StorageError> {
    let manifest = verify(backup)?;
    if let Some(refusal) = restore_refusal(&manifest) {
        return Err(StorageError::Rejected(refusal));
    }
    std::fs::write(root.join("data").join("restore-pending"), backup.to_string_lossy().as_bytes())?;
    Ok(())
}

/// Called at startup. If a restore is pending: keep the current data aside,
/// put the backup in place and mark the store paused. Returns the restored manifest.
pub fn perform_pending_restore(root: &Path) -> Result<Option<Manifest>, StorageError> {
    let marker = root.join("data").join("restore-pending");
    let Ok(source) = std::fs::read_to_string(&marker) else { return Ok(None) };
    let source = PathBuf::from(source.trim());
    let manifest = verify(&source)?;
    // A marker left by an older build: refused before anything moves.
    if let Some(refusal) = restore_refusal(&manifest) {
        return Err(StorageError::Rejected(refusal));
    }
    let stamp = now_ms();
    let aside = root.join(format!("before-restore-{stamp}"));
    std::fs::create_dir_all(&aside)?;
    for name in ["data", "blobs"] {
        let path = root.join(name);
        if path.exists() {
            rename_patiently(&path, &aside.join(name))?;
        }
    }
    let (mut count, mut bytes) = (0u64, 0u64);
    copy_tree(&source.join("data"), &root.join("data"), &mut count, &mut bytes)?;
    if source.join("blobs").is_dir() {
        copy_tree(&source.join("blobs"), &root.join("blobs"), &mut count, &mut bytes)?;
    }
    // Identity and configuration files stay those of this installation (a
    // backup holds only the database).
    for name in crate::fresh_start::IDENTITY_FILES.into_iter().chain(crate::fresh_start::CONFIG_FILES) {
        let old = aside.join("data").join(name);
        if old.exists() {
            let _ = std::fs::copy(&old, root.join("data").join(name));
        }
    }
    let _ = std::fs::remove_file(root.join("data").join("restore-pending"));
    std::fs::write(root.join("data").join("paused"), format!("restored from {}", source.display()))?;
    Ok(Some(manifest))
}

pub fn is_paused(root: &Path) -> bool {
    root.join("data").join("paused").exists()
}

pub fn resume(root: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(root.join("data").join("paused")) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// Remove forgotten content from every existing backup: blob files, FTS text
/// and chat titles. Forgotten content must not come back through a restore.
pub fn scrub(root: &Path, blob_keys: &[String], memory_ids: &[String], conversation_ids: &[String]) -> Result<usize, StorageError> {
    let mut scrubbed = 0;
    for (dir, mut manifest) in list(root) {
        for key in blob_keys {
            let _ = std::fs::remove_file(dir.join("blobs").join(key));
        }
        let db = dir.join("data").join("aurelius.sqlite");
        if (!memory_ids.is_empty() || !conversation_ids.is_empty()) && db.exists() {
            let conn = rusqlite::Connection::open(&db)?;
            for id in memory_ids {
                conn.execute("DELETE FROM chunk_fts WHERE chunk_id IN (SELECT id FROM retrieval_chunks WHERE memory_id = ?1)", [id])?;
                conn.execute("UPDATE memory_items SET status = 'forgotten' WHERE id = ?1", [id])?;
            }
            for id in conversation_ids {
                // Backups from before migration 0012 have no activity table.
                let _ = conn.execute("DELETE FROM run_activity WHERE message_id IN (SELECT id FROM messages WHERE conversation_id = ?1)", [id]);
                conn.execute("UPDATE conversations SET title = '', archived_at_ms = COALESCE(archived_at_ms, ?2) WHERE id = ?1", rusqlite::params![id, now_ms()])?;
            }
            conn.execute_batch("VACUUM")?;
            drop(conn);
            manifest.db_sha256 = sha256_file(&db)?;
        }
        manifest.scrubbed_at_ms = Some(now_ms());
        std::fs::write(dir.join(MANIFEST), serde_json::to_vec_pretty(&manifest)?)?;
        scrubbed += 1;
    }
    Ok(scrubbed)
}
