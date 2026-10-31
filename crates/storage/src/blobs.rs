//! Immutable content-addressed payload files.

use crate::{StorageError, now_ms};
use aurelius_domain::BlobId;
use rusqlite::Transaction;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub struct BlobStore {
    root: PathBuf,
    cache: Mutex<Cache>,
}

/// Small blobs read before, kept in memory: a blob never changes once written
/// (only `erase` removes it), and lists such as `memory.list` read thousands
/// of them on every reload, each a file open.
#[derive(Default)]
struct Cache {
    blobs: HashMap<String, Arc<[u8]>>,
    bytes: usize,
}

impl Cache {
    /// Larger blobs are read from disk each time.
    const BLOB_MAX: usize = 64 * 1024;
    /// All cached blobs together; past it the cache starts over.
    const TOTAL_MAX: usize = 32 * 1024 * 1024;

    fn keep(&mut self, key: &str, bytes: &[u8]) {
        if bytes.len() > Self::BLOB_MAX {
            return;
        }
        if self.bytes + bytes.len() > Self::TOTAL_MAX {
            self.blobs.clear();
            self.bytes = 0;
        }
        if self.blobs.insert(key.to_owned(), Arc::from(bytes)).is_none() {
            self.bytes += bytes.len();
        }
    }
}

/// A blob that is durable on disk but not yet registered in the database.
#[derive(Debug, Clone)]
pub struct PreparedBlob {
    pub id: BlobId,
    pub storage_key: String,
    pub sha256: String,
    pub byte_length: i64,
    pub media_type: String,
}

impl BlobStore {
    pub fn new(root: PathBuf) -> Result<Self, StorageError> {
        fs::create_dir_all(root.join("tmp"))?;
        Ok(Self { root, cache: Mutex::default() })
    }

    /// The folder the blob files live in.
    pub fn root(&self) -> &std::path::Path {
        &self.root
    }

    /// Write bytes to a temp file, fsync, then atomically rename into place.
    pub fn put(&self, bytes: &[u8], media_type: &str) -> Result<PreparedBlob, StorageError> {
        let id = BlobId::new();
        let name = id.0.simple().to_string();
        let prefix = &name[name.len() - 2..];
        let storage_key = format!("{prefix}/{name}");

        let tmp_path = self.root.join("tmp").join(&name);
        {
            let mut file = File::create(&tmp_path)?;
            file.write_all(bytes)?;
            file.sync_all()?;
        }
        let final_dir = self.root.join(prefix);
        fs::create_dir_all(&final_dir)?;
        fs::rename(&tmp_path, final_dir.join(&name))?;

        Ok(PreparedBlob {
            id,
            storage_key,
            sha256: hex::encode(Sha256::digest(bytes)),
            byte_length: bytes.len() as i64,
            media_type: media_type.to_owned(),
        })
    }

    pub fn put_json<T: serde::Serialize>(&self, value: &T) -> Result<PreparedBlob, StorageError> {
        self.put(&serde_json::to_vec(value)?, "application/json")
    }

    pub fn read(&self, storage_key: &str) -> Result<Vec<u8>, StorageError> {
        if let Some(bytes) = self.cache.lock().unwrap_or_else(|p| p.into_inner()).blobs.get(storage_key) {
            return Ok(bytes.to_vec());
        }
        let bytes = fs::read(self.root.join(storage_key))?;
        self.cache.lock().unwrap_or_else(|p| p.into_inner()).keep(storage_key, &bytes);
        Ok(bytes)
    }

    /// Erase content (forget). The row stays for referential integrity;
    /// reading afterwards fails. Missing files are fine (idempotent).
    pub fn erase(&self, storage_key: &str) -> Result<(), StorageError> {
        let mut cache = self.cache.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(gone) = cache.blobs.remove(storage_key) {
            cache.bytes -= gone.len();
        }
        drop(cache);
        match fs::remove_file(self.root.join(storage_key)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
            _ => Ok(()),
        }
    }
}

/// Write `bytes` as a blob of the store `tx` writes to and register it, inside the
/// transaction: for a short text the engine writes deep in a hook that has no
/// `Store` (a system line of a discussion). The blobs live beside the database
/// (`root/blobs`, see `Store::open`); a rollback leaves only an unreferenced file.
pub fn put_beside(tx: &Transaction<'_>, bytes: &[u8], media_type: &str) -> Result<PreparedBlob, StorageError> {
    let db = tx.path().filter(|p| !p.is_empty()).ok_or_else(|| StorageError::Rejected("the database has no file to keep blobs beside".into()))?;
    let root = std::path::Path::new(db).parent().and_then(std::path::Path::parent).ok_or_else(|| StorageError::Rejected("the database is not below a store root".into()))?;
    let blob = BlobStore::new(root.join("blobs"))?.put(bytes, media_type)?;
    register(tx, &blob)?;
    Ok(blob)
}

/// Register a prepared blob inside the command transaction.
pub fn register(tx: &Transaction<'_>, blob: &PreparedBlob) -> Result<(), StorageError> {
    tx.execute(
        "INSERT INTO blobs(id, storage_key, sha256, byte_length, media_type, created_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            blob.id.to_string(),
            blob.storage_key,
            blob.sha256,
            blob.byte_length,
            blob.media_type,
            now_ms()
        ],
    )?;
    Ok(())
}
