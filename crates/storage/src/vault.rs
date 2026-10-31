//! The vault (migration 22): items of the company or of one project, and their
//! append-only audit. Storage never sees a plaintext value: every value (text
//! and url too, B1) arrives already encrypted as a [`Sealed`] and leaves the
//! same way; the daemon holds the key and does the crypto.
//!
//! * An item is never deleted; "Delete" archives it. Names and environment
//!   variables are unique per scope among the items that are not archived.
//! * `version` is the version of the value: a new ciphertext raises it by one;
//!   a change of name, env name or description does not. The value it replaces
//!   is kept, still encrypted, in `vault_item_versions` (E16), for the redactor
//!   ([`redaction_values`]). Nothing here assumes a version number is unique
//!   over time: a restored backup can bring older numbers back. Every row keeps
//!   its own key id.
//! * The kind changes between text and url only (B7; the database refuses the
//!   rest too), and only with a new ciphertext, since the daemon binds the value
//!   to its kind.
//! * A run gets the company's items plus its project's; a project item wins
//!   over a company item with the same name or env name (B4, [`effective`]).
//! * Items of an archived project stay readable and can be archived, but no
//!   item is made or changed in it.

use crate::StorageError;
use rusqlite::{Connection, OptionalExtension, Transaction, params, params_from_iter, types::Value};

/// `Rejected` reasons, for the daemon to map to its own errors.
pub const NOT_FOUND: &str = "vault_item_not_found";
pub const ARCHIVED: &str = "vault_item_archived";
pub const PROJECT_ARCHIVED: &str = "vault_project_archived";
pub const NAME_TAKEN: &str = "vault_name_taken";
pub const ENV_NAME_TAKEN: &str = "vault_env_name_taken";
pub const INVALID_NAME: &str = "vault_invalid_name";
pub const INVALID_ENV_NAME: &str = "vault_invalid_env_name";
/// A secret needs an env name.
pub const ENV_NAME_REQUIRED: &str = "vault_env_name_required";
/// The kind changes between text and url only (B7).
pub const KIND_SECRET: &str = "vault_kind_secret";
/// A change of kind comes with a new ciphertext.
pub const KIND_NEEDS_VALUE: &str = "vault_kind_needs_value";
pub const VERSION_MISMATCH: &str = "vault_version_mismatch";
/// A row changed under a key rotation (it cannot within one transaction).
pub const ROTATION_CONFLICT: &str = "vault_rotation_conflict";
/// An audit row that does not fit its action (an item for a key export, a
/// field name that is not one of [`FIELDS`], no actor).
pub const INVALID_AUDIT: &str = "vault_invalid_audit";

/// The field names an edit can record in the audit (never a value).
pub const FIELDS: [&str; 5] = ["name", "env_name", "description", "kind", "value"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VaultKind {
    Text,
    Url,
    Secret,
}

impl VaultKind {
    pub fn as_str(self) -> &'static str {
        match self {
            VaultKind::Text => "text",
            VaultKind::Url => "url",
            VaultKind::Secret => "secret",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "text" => Some(VaultKind::Text),
            "url" => Some(VaultKind::Url),
            "secret" => Some(VaultKind::Secret),
            _ => None,
        }
    }
}

/// An encrypted value as the daemon made it. `Debug` shows only lengths and
/// the key id.
#[derive(Clone, PartialEq, Eq)]
pub struct Sealed {
    pub ciphertext: Vec<u8>,
    pub nonce: Vec<u8>,
    pub key_id: String,
}

impl std::fmt::Debug for Sealed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sealed")
            .field("ciphertext", &format_args!("{} bytes", self.ciphertext.len()))
            .field("nonce", &format_args!("{} bytes", self.nonce.len()))
            .field("key_id", &self.key_id)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultItemRow {
    pub id: String,
    pub company_id: String,
    /// None: a company item.
    pub project_id: Option<String>,
    pub kind: VaultKind,
    pub name: String,
    pub env_name: Option<String>,
    pub description: Option<String>,
    pub sealed: Sealed,
    pub version: i64,
    pub created_by_user: String,
    pub updated_by_user: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub archived_at_ms: Option<i64>,
}

/// Which items: the company's, one project's, or a project's with the company's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VaultScope<'a> {
    Company,
    Project(&'a str),
    ProjectAndCompany(&'a str),
}

pub struct NewVaultItem<'a> {
    pub id: &'a str,
    /// None: a company item.
    pub project_id: Option<&'a str>,
    pub kind: VaultKind,
    pub name: &'a str,
    pub env_name: Option<&'a str>,
    pub description: Option<&'a str>,
    pub sealed: Sealed,
    pub by_user: &'a str,
    pub now_ms: i64,
}

/// A change to an item; `None` leaves a field as it is.
pub struct VaultUpdate<'a> {
    pub name: Option<&'a str>,
    /// `Some(None)` clears it (not for a secret).
    pub env_name: Option<Option<&'a str>>,
    /// `Some(None)` clears it.
    pub description: Option<Option<&'a str>>,
    /// Text ↔ url only, and only with `sealed`.
    pub kind: Option<VaultKind>,
    /// A new value: the version goes up by one.
    pub sealed: Option<Sealed>,
    /// The value version the client saw; another one is `VERSION_MISMATCH`.
    pub expected_version: Option<i64>,
    pub by_user: &'a str,
    pub now_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Updated {
    pub item: VaultItemRow,
    /// The fields that changed, in [`FIELDS`] order ("value": a new ciphertext).
    /// Empty: nothing changed and nothing was written.
    pub changed: Vec<&'static str>,
}

impl Updated {
    /// The audit action of this change: `Replace` with a new value, `Edit`
    /// otherwise, none when nothing changed.
    pub fn action(&self) -> Option<AuditAction> {
        if self.changed.is_empty() {
            None
        } else if self.changed.contains(&"value") {
            Some(AuditAction::Replace)
        } else {
            Some(AuditAction::Edit)
        }
    }
}

/// An item in a run's set (or a project's view of it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveItem {
    pub item: VaultItemRow,
    /// A project item with the name or env name of a company item, which it
    /// hides in this project ("overrides Company").
    pub overrides_company: bool,
}

/// An environment variable name as the vault takes it: UPPER_SNAKE (A-Z, 0-9
/// and _, not starting with a digit), 1 to 128 characters. Which names a run
/// may not get (PATH, ANTHROPIC_*, …) is the daemon's rule, not this one.
pub fn valid_env_name(name: &str) -> bool {
    (1..=128).contains(&name.len())
        && !name.starts_with(|c: char| c.is_ascii_digit())
        && name.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
}

fn valid_name(name: &str) -> bool {
    !name.trim().is_empty() && name.chars().count() <= 200
}

const COLUMNS: &str = "v.id, v.company_id, v.project_id, v.kind, v.name, v.env_name, v.description, v.ciphertext, v.nonce, v.key_id,
     v.version, v.created_by_user, v.updated_by_user, v.created_at_ms, v.updated_at_ms, v.archived_at_ms";

fn bad_kind(index: usize, kind: &str) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(index, rusqlite::types::Type::Text, format!("unknown vault kind {kind:?}").into())
}

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<VaultItemRow> {
    let kind: String = r.get(3)?;
    Ok(VaultItemRow {
        id: r.get(0)?,
        company_id: r.get(1)?,
        project_id: r.get(2)?,
        kind: VaultKind::parse(&kind).ok_or_else(|| bad_kind(3, &kind))?,
        name: r.get(4)?,
        env_name: r.get(5)?,
        description: r.get(6)?,
        sealed: Sealed { ciphertext: r.get(7)?, nonce: r.get(8)?, key_id: r.get(9)? },
        version: r.get(10)?,
        created_by_user: r.get(11)?,
        updated_by_user: r.get(12)?,
        created_at_ms: r.get(13)?,
        updated_at_ms: r.get(14)?,
        archived_at_ms: r.get(15)?,
    })
}

fn company_id(conn: &Connection) -> Result<String, StorageError> {
    Ok(conn.query_row("SELECT id FROM company", [], |r| r.get(0))?)
}

fn refuse_archived_project(conn: &Connection, project_id: Option<&str>) -> Result<(), StorageError> {
    let Some(project) = project_id else { return Ok(()) };
    let status: Option<String> = conn.query_row("SELECT status FROM projects WHERE id = ?1", [project], |r| r.get(0)).optional()?;
    match status.as_deref() {
        Some("archived") => Err(StorageError::Rejected(PROJECT_ARCHIVED.into())),
        // An unknown project fails on its foreign key.
        _ => Ok(()),
    }
}

/// A taken name or env name as its `Rejected` reason.
fn taken(e: rusqlite::Error) -> StorageError {
    if let rusqlite::Error::SqliteFailure(f, Some(msg)) = &e
        && f.code == rusqlite::ErrorCode::ConstraintViolation
    {
        if msg.contains("vault_items_env_name") {
            return StorageError::Rejected(ENV_NAME_TAKEN.into());
        }
        if msg.contains("vault_items_name") {
            return StorageError::Rejected(NAME_TAKEN.into());
        }
    }
    e.into()
}

/// Store a new item at version 1 with its already encrypted value.
pub fn insert(tx: &Transaction<'_>, item: NewVaultItem<'_>) -> Result<VaultItemRow, StorageError> {
    if !valid_name(item.name) {
        return Err(StorageError::Rejected(INVALID_NAME.into()));
    }
    match item.env_name {
        Some(env) if !valid_env_name(env) => return Err(StorageError::Rejected(INVALID_ENV_NAME.into())),
        None if item.kind == VaultKind::Secret => return Err(StorageError::Rejected(ENV_NAME_REQUIRED.into())),
        _ => {}
    }
    refuse_archived_project(tx, item.project_id)?;
    let company = company_id(tx)?;
    tx.execute(
        "INSERT INTO vault_items(id, company_id, project_id, kind, name, env_name, description, ciphertext, nonce, key_id,
                                 version, created_by_user, updated_by_user, created_at_ms, updated_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 1, ?11, ?11, ?12, ?12)",
        params![
            item.id,
            company,
            item.project_id,
            item.kind.as_str(),
            item.name,
            item.env_name,
            item.description,
            item.sealed.ciphertext,
            item.sealed.nonce,
            item.sealed.key_id,
            item.by_user,
            item.now_ms
        ],
    )
    .map_err(taken)?;
    Ok(VaultItemRow {
        id: item.id.to_string(),
        company_id: company,
        project_id: item.project_id.map(str::to_string),
        kind: item.kind,
        name: item.name.to_string(),
        env_name: item.env_name.map(str::to_string),
        description: item.description.map(str::to_string),
        sealed: item.sealed,
        version: 1,
        created_by_user: item.by_user.to_string(),
        updated_by_user: item.by_user.to_string(),
        created_at_ms: item.now_ms,
        updated_at_ms: item.now_ms,
        archived_at_ms: None,
    })
}

/// Any item by id, archived or not.
pub fn get(conn: &Connection, id: &str) -> Result<Option<VaultItemRow>, StorageError> {
    Ok(conn.query_row(&format!("SELECT {COLUMNS} FROM vault_items v WHERE v.id = ?1"), [id], row).optional()?)
}

/// Change an item that is not archived. Nothing changed: nothing is written
/// and `changed` is empty. The caller writes the audit row ([`Updated::action`]).
pub fn update(tx: &Transaction<'_>, id: &str, change: VaultUpdate<'_>) -> Result<Updated, StorageError> {
    let mut item = get(tx, id)?.ok_or_else(|| StorageError::Rejected(NOT_FOUND.into()))?;
    if item.archived_at_ms.is_some() {
        return Err(StorageError::Rejected(ARCHIVED.into()));
    }
    refuse_archived_project(tx, item.project_id.as_deref())?;
    if change.expected_version.is_some_and(|v| v != item.version) {
        return Err(StorageError::Rejected(VERSION_MISMATCH.into()));
    }
    // The kind and version the current value was sealed for, before they change.
    let previous = (item.kind, item.version);
    let mut changed = Vec::new();
    if let Some(name) = change.name
        && name != item.name
    {
        if !valid_name(name) {
            return Err(StorageError::Rejected(INVALID_NAME.into()));
        }
        item.name = name.to_string();
        changed.push("name");
    }
    if let Some(env) = change.env_name
        && env != item.env_name.as_deref()
    {
        if env.is_some_and(|e| !valid_env_name(e)) {
            return Err(StorageError::Rejected(INVALID_ENV_NAME.into()));
        }
        item.env_name = env.map(str::to_string);
        changed.push("env_name");
    }
    if let Some(description) = change.description
        && description != item.description.as_deref()
    {
        item.description = description.map(str::to_string);
        changed.push("description");
    }
    if let Some(kind) = change.kind
        && kind != item.kind
    {
        if kind == VaultKind::Secret || item.kind == VaultKind::Secret {
            return Err(StorageError::Rejected(KIND_SECRET.into()));
        }
        if change.sealed.is_none() {
            return Err(StorageError::Rejected(KIND_NEEDS_VALUE.into()));
        }
        item.kind = kind;
        changed.push("kind");
    }
    if item.kind == VaultKind::Secret && item.env_name.is_none() {
        return Err(StorageError::Rejected(ENV_NAME_REQUIRED.into()));
    }
    let replaced = change.sealed.map(|sealed| {
        item.version += 1;
        changed.push("value");
        std::mem::replace(&mut item.sealed, sealed)
    });
    if changed.is_empty() {
        return Ok(Updated { item, changed });
    }
    item.updated_by_user = change.by_user.to_string();
    item.updated_at_ms = change.now_ms;
    // The item first: a taken name or env name refuses the change before
    // anything is written. The daemon commits a `Rejected` (it answers it), and
    // a previous value written ahead of it could never be taken back.
    tx.execute(
        "UPDATE vault_items SET kind = ?2, name = ?3, env_name = ?4, description = ?5, ciphertext = ?6, nonce = ?7, key_id = ?8,
                version = ?9, updated_by_user = ?10, updated_at_ms = ?11
         WHERE id = ?1 AND archived_at_ms IS NULL",
        params![
            item.id,
            item.kind.as_str(),
            item.name,
            item.env_name,
            item.description,
            item.sealed.ciphertext,
            item.sealed.nonce,
            item.sealed.key_id,
            item.version,
            item.updated_by_user,
            item.updated_at_ms
        ],
    )
    .map_err(taken)?;
    if let Some(old) = replaced {
        tx.execute(
            "INSERT INTO vault_item_versions(item_id, kind, version, ciphertext, nonce, key_id, replaced_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![item.id, previous.0.as_str(), previous.1, old.ciphertext, old.nonce, old.key_id, change.now_ms],
        )?;
    }
    Ok(Updated { item, changed })
}

/// "Delete": archive an item, also one of an archived project (MU4-02 AC3).
/// Its name and env name are free again; its audit stays.
pub fn archive(tx: &Transaction<'_>, id: &str, by_user: &str, now_ms: i64) -> Result<VaultItemRow, StorageError> {
    let mut item = get(tx, id)?.ok_or_else(|| StorageError::Rejected(NOT_FOUND.into()))?;
    if item.archived_at_ms.is_some() {
        return Err(StorageError::Rejected(ARCHIVED.into()));
    }
    tx.execute(
        "UPDATE vault_items SET archived_at_ms = ?2, updated_by_user = ?3, updated_at_ms = ?2 WHERE id = ?1",
        params![id, now_ms, by_user],
    )?;
    item.archived_at_ms = Some(now_ms);
    item.updated_by_user = by_user.to_string();
    item.updated_at_ms = now_ms;
    Ok(item)
}

/// The items of a scope, company items first, then by name (without case).
/// Items of an archived project are listed like any other.
pub fn list(conn: &Connection, scope: VaultScope<'_>, include_archived: bool) -> Result<Vec<VaultItemRow>, StorageError> {
    let (filter, project) = match scope {
        VaultScope::Company => ("v.project_id IS NULL", None),
        VaultScope::Project(p) => ("v.project_id = ?1", Some(p)),
        VaultScope::ProjectAndCompany(p) => ("(v.project_id IS NULL OR v.project_id = ?1)", Some(p)),
    };
    let archived = if include_archived { "" } else { " AND v.archived_at_ms IS NULL" };
    let sql = format!(
        "SELECT {COLUMNS} FROM vault_items v WHERE {filter}{archived}
         ORDER BY v.project_id IS NOT NULL, v.name COLLATE NOCASE, v.created_at_ms, v.id"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = match project {
        Some(p) => stmt.query_map([p], row)?.collect::<Result<Vec<_>, _>>()?,
        None => stmt.query_map([], row)?.collect::<Result<Vec<_>, _>>()?,
    };
    Ok(rows)
}

/// The set a run gets (B4): the company's items that are not archived plus
/// those of `project_id`, where a project item hides every company item with
/// its name (without case) or its env name. Project items first. Without a
/// project: the company's only. The env names in the result are unique.
pub fn effective(conn: &Connection, project_id: Option<&str>) -> Result<Vec<EffectiveItem>, StorageError> {
    let same = "o.company_id = v.company_id AND o.archived_at_ms IS NULL
                AND (o.name = v.name COLLATE NOCASE OR o.env_name = v.env_name)";
    let sql = format!(
        "SELECT {COLUMNS},
                v.project_id IS NOT NULL AND EXISTS (SELECT 1 FROM vault_items o WHERE o.project_id IS NULL AND {same})
         FROM vault_items v
         WHERE v.archived_at_ms IS NULL
           AND (v.project_id = ?1
                OR (v.project_id IS NULL AND NOT EXISTS (SELECT 1 FROM vault_items o WHERE o.project_id = ?1 AND {same})))
         ORDER BY v.project_id IS NULL, v.name COLLATE NOCASE, v.id"
    );
    let mut stmt = conn.prepare(&sql)?;
    let items = stmt
        .query_map([project_id], |r| Ok(EffectiveItem { item: row(r)?, overrides_company: r.get(16)? }))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(items)
}

/// Every secret that is not archived, in every scope: what the redactor must
/// know. Oldest first.
pub fn active_secrets(conn: &Connection) -> Result<Vec<VaultItemRow>, StorageError> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM vault_items v WHERE v.kind = 'secret' AND v.archived_at_ms IS NULL ORDER BY v.created_at_ms, v.id"
    ))?;
    let rows = stmt.query_map([], row)?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// The env names of the items that are not archived, each once, sorted. `scope`
/// none: every scope (what a command that prints the environment is checked
/// against, E17).
pub fn env_names(conn: &Connection, scope: Option<VaultScope<'_>>) -> Result<Vec<String>, StorageError> {
    let (filter, project) = match scope {
        None => ("", None),
        Some(VaultScope::Company) => (" AND project_id IS NULL", None),
        Some(VaultScope::Project(p)) => (" AND project_id = ?1", Some(p)),
        Some(VaultScope::ProjectAndCompany(p)) => (" AND (project_id IS NULL OR project_id = ?1)", Some(p)),
    };
    let mut stmt = conn.prepare(&format!(
        "SELECT DISTINCT env_name FROM vault_items WHERE archived_at_ms IS NULL AND env_name IS NOT NULL{filter} ORDER BY env_name"
    ))?;
    let names = match project {
        Some(p) => stmt.query_map([p], |r| r.get(0))?.collect::<Result<Vec<_>, _>>()?,
        None => stmt.query_map([], |r| r.get(0))?.collect::<Result<Vec<_>, _>>()?,
    };
    Ok(names)
}

/// Where a stored value lives: an item's current value, or a previous one
/// (its row id in `vault_item_versions`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueSource {
    Current,
    Previous(i64),
}

/// One encrypted value with what it was sealed for: the item, its scope and
/// the kind and version of that value. `name` and `env_name` are the item's
/// current ones (a previous value is redacted under the current name).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredValue {
    pub item_id: String,
    pub company_id: String,
    pub project_id: Option<String>,
    pub kind: VaultKind,
    pub version: i64,
    pub sealed: Sealed,
    pub source: ValueSource,
    pub name: String,
    pub env_name: Option<String>,
    /// The item is archived.
    pub archived: bool,
}

/// Every stored value: the current one of each item and each previous one.
const VALUES: &str = "SELECT v.id AS item_id, v.company_id, v.project_id, v.kind, v.version, v.ciphertext, v.nonce, v.key_id,
            NULL AS previous, v.name, v.env_name, v.archived_at_ms IS NOT NULL AS archived
     FROM vault_items v
     UNION ALL
     SELECT o.item_id, v.company_id, v.project_id, o.kind, o.version, o.ciphertext, o.nonce, o.key_id,
            o.id, v.name, v.env_name, v.archived_at_ms IS NOT NULL
     FROM vault_item_versions o JOIN vault_items v ON v.id = o.item_id";

fn stored_value(r: &rusqlite::Row<'_>) -> rusqlite::Result<StoredValue> {
    let kind: String = r.get(3)?;
    let previous: Option<i64> = r.get(8)?;
    Ok(StoredValue {
        item_id: r.get(0)?,
        company_id: r.get(1)?,
        project_id: r.get(2)?,
        kind: VaultKind::parse(&kind).ok_or_else(|| bad_kind(3, &kind))?,
        version: r.get(4)?,
        sealed: Sealed { ciphertext: r.get(5)?, nonce: r.get(6)?, key_id: r.get(7)? },
        source: previous.map_or(ValueSource::Current, ValueSource::Previous),
        name: r.get(9)?,
        env_name: r.get(10)?,
        archived: r.get(11)?,
    })
}

fn values(conn: &Connection, filter: &str, args: &[&dyn rusqlite::ToSql]) -> Result<Vec<StoredValue>, StorageError> {
    // Per item: the current value, then the previous ones from newest to oldest.
    let mut stmt = conn.prepare(&format!(
        "SELECT * FROM ({VALUES}) {filter} ORDER BY item_id, previous IS NOT NULL, previous DESC"
    ))?;
    let rows = stmt.query_map(args, stored_value)?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// What the redactor must know (E16): every value of every item, of every kind
/// and scope, archived items too, current and previous. Which kinds it redacts
/// is the daemon's choice.
pub fn redaction_values(conn: &Connection) -> Result<Vec<StoredValue>, StorageError> {
    values(conn, "", &[])
}

/// The previous values of one item, newest first.
pub fn previous_values(conn: &Connection, item_id: &str) -> Result<Vec<StoredValue>, StorageError> {
    values(conn, "WHERE item_id = ?1 AND previous IS NOT NULL", &[&item_id])
}

/// Whether any ciphertext is stored at all, current or previous, archived or
/// not (E4: only without it may a new master key be made).
pub fn any_ciphertext(conn: &Connection) -> Result<bool, StorageError> {
    Ok(conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM vault_items) OR EXISTS (SELECT 1 FROM vault_item_versions)",
        [],
        |r| r.get(0),
    )?)
}

/// How many stored values (current and previous, archived items too) each key
/// encrypts, by key id.
pub fn key_ids(conn: &Connection) -> Result<Vec<(String, i64)>, StorageError> {
    let mut stmt = conn.prepare(&format!("SELECT key_id, COUNT(*) FROM ({VALUES}) GROUP BY key_id ORDER BY key_id"))?;
    let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Key rotation: encrypt every stored value under `key_id` again with `seal`
/// (current and previous values, archived items too). `seal` gets the value as
/// it is and returns its new ciphertext, nonce and key id. Runs in the caller's
/// transaction, so all rows change or, when `seal` fails, none. A row is matched
/// on its id and its old nonce, never on a version number; the version, kind
/// and the rest of the item stay. Returns how many values changed.
pub fn reencrypt<F>(tx: &Transaction<'_>, key_id: &str, mut seal: F) -> Result<usize, StorageError>
where
    F: FnMut(&StoredValue) -> Result<Sealed, StorageError>,
{
    let all = values(tx, "WHERE key_id = ?1", &[&key_id])?;
    let mut current = tx.prepare("UPDATE vault_items SET ciphertext = ?3, nonce = ?4, key_id = ?5 WHERE id = ?1 AND nonce = ?2 AND key_id = ?6")?;
    let mut previous =
        tx.prepare("UPDATE vault_item_versions SET ciphertext = ?3, nonce = ?4, key_id = ?5 WHERE id = ?1 AND nonce = ?2 AND key_id = ?6")?;
    for value in &all {
        let sealed = seal(value)?;
        let changed = match value.source {
            ValueSource::Current => current.execute(params![value.item_id, value.sealed.nonce, sealed.ciphertext, sealed.nonce, sealed.key_id, key_id])?,
            ValueSource::Previous(id) => previous.execute(params![id, value.sealed.nonce, sealed.ciphertext, sealed.nonce, sealed.key_id, key_id])?,
        };
        if changed != 1 {
            return Err(StorageError::Rejected(ROTATION_CONFLICT.into()));
        }
    }
    Ok(all.len())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AuditAction {
    Create,
    Edit,
    Replace,
    Archive,
    Reveal,
    Copy,
    Deliver,
    ToolRead,
    KeyExport,
}

impl AuditAction {
    pub fn as_str(self) -> &'static str {
        match self {
            AuditAction::Create => "create",
            AuditAction::Edit => "edit",
            AuditAction::Replace => "replace",
            AuditAction::Archive => "archive",
            AuditAction::Reveal => "reveal",
            AuditAction::Copy => "copy",
            AuditAction::Deliver => "deliver",
            AuditAction::ToolRead => "tool_read",
            AuditAction::KeyExport => "key_export",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "create" => AuditAction::Create,
            "edit" => AuditAction::Edit,
            "replace" => AuditAction::Replace,
            "archive" => AuditAction::Archive,
            "reveal" => AuditAction::Reveal,
            "copy" => AuditAction::Copy,
            "deliver" => AuditAction::Deliver,
            "tool_read" => AuditAction::ToolRead,
            "key_export" => AuditAction::KeyExport,
            _ => return None,
        })
    }
}

/// One audit row. Who: a board member (`user_id`, with the session and device
/// as "Via"), or an employee with its run, task and ticket.
#[derive(Debug)]
pub struct NewAudit<'a> {
    pub id: &'a str,
    /// None only for `KeyExport`.
    pub item_id: Option<&'a str>,
    pub action: AuditAction,
    /// Names from [`FIELDS`] (an edit or replace), never a value.
    pub changed_fields: &'a [&'a str],
    pub user_id: Option<&'a str>,
    pub session_id: Option<&'a str>,
    pub via: Option<&'a str>,
    pub employee_id: Option<&'a str>,
    pub run_id: Option<&'a str>,
    pub task_id: Option<&'a str>,
    pub ticket_id: Option<&'a str>,
    /// The version of the item's value that was read or written (E15). None:
    /// the item's version as it is in this transaction, which is right for a
    /// mutation and a read in the same transaction; a run that read the value
    /// earlier (deliver, tool_read) sets the one it got. Always none for a key
    /// export.
    pub version: Option<i64>,
    pub at_ms: i64,
}

impl<'a> NewAudit<'a> {
    /// A row without actor or "Via"; set those with struct update syntax
    /// (`NewAudit { user_id: Some(u), ..NewAudit::new(..) }`).
    pub fn new(id: &'a str, item_id: Option<&'a str>, action: AuditAction, at_ms: i64) -> Self {
        NewAudit {
            id,
            item_id,
            action,
            changed_fields: &[],
            user_id: None,
            session_id: None,
            via: None,
            employee_id: None,
            run_id: None,
            task_id: None,
            ticket_id: None,
            version: None,
            at_ms,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditRow {
    /// The order the row was written in; the protocol pages on it.
    pub seq: i64,
    pub id: String,
    pub item_id: Option<String>,
    /// The item's current name and kind (none for a key export).
    pub item_name: Option<String>,
    pub item_kind: Option<VaultKind>,
    pub company_id: String,
    /// The item's scope (none: company).
    pub project_id: Option<String>,
    pub action: AuditAction,
    pub changed_fields: Vec<String>,
    pub user_id: Option<String>,
    pub session_id: Option<String>,
    pub via: Option<String>,
    pub employee_id: Option<String>,
    pub run_id: Option<String>,
    pub task_id: Option<String>,
    pub ticket_id: Option<String>,
    /// The version of the value this row is about (none for a key export).
    pub version: Option<i64>,
    /// The board member's current username, and the employee's current name:
    /// none when the actor is not one, or no longer exists.
    pub username: Option<String>,
    pub employee_name: Option<String>,
    pub at_ms: i64,
}

impl AuditRow {
    /// The cursor for the page after this row ("Load older").
    pub fn cursor(&self) -> AuditCursor {
        AuditCursor { at_ms: self.at_ms, id: self.id.clone() }
    }
}

/// Rows strictly older than this one by time (`at_ms` DESC, `id` DESC). The
/// protocol pages by [`AuditFilter::before_seq`] instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditCursor {
    pub at_ms: i64,
    pub id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditActor<'a> {
    User(&'a str),
    Employee(&'a str),
}

/// What the log shows; every `None` is "any". `scope` none: all scopes.
#[derive(Debug, Clone, Default)]
pub struct AuditFilter<'a> {
    pub item_id: Option<&'a str>,
    pub scope: Option<VaultScope<'a>>,
    pub actor: Option<AuditActor<'a>>,
    pub action: Option<AuditAction>,
    /// Only rows at or after this time ("Last 30 days").
    pub since_ms: Option<i64>,
    /// "Load older" as the protocol pages: rows written before this
    /// [`AuditRow::seq`], newest first by seq.
    pub before_seq: Option<i64>,
    /// "Load older" by time; with it the rows come by `at_ms` DESC, `id` DESC.
    pub before: Option<AuditCursor>,
    /// At most this many rows (0: 50).
    pub limit: usize,
}

const AUDIT_COLUMNS: &str = "a.id, a.item_id, v.name, v.kind, a.company_id, a.project_id, a.action, a.changed_fields, a.user_id,
     a.session_id, a.via, a.employee_id, a.run_id, a.task_id, a.ticket_id, a.at_ms, a.seq,
     a.version, u.username, e.name";

/// The audit with its item and the actor's names, so a page is one query.
const AUDIT_FROM: &str = "vault_audit a LEFT JOIN vault_items v ON v.id = a.item_id
     LEFT JOIN users u ON u.id = a.user_id LEFT JOIN employees e ON e.id = a.employee_id";

fn audit_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<AuditRow> {
    let kind: Option<String> = r.get(3)?;
    let action: String = r.get(6)?;
    let fields: Option<String> = r.get(7)?;
    Ok(AuditRow {
        id: r.get(0)?,
        item_id: r.get(1)?,
        item_name: r.get(2)?,
        item_kind: kind.map(|k| VaultKind::parse(&k).ok_or_else(|| bad_kind(3, &k))).transpose()?,
        company_id: r.get(4)?,
        project_id: r.get(5)?,
        action: AuditAction::parse(&action).ok_or_else(|| {
            rusqlite::Error::FromSqlConversionFailure(6, rusqlite::types::Type::Text, format!("unknown vault action {action:?}").into())
        })?,
        changed_fields: fields.map(|f| f.split(',').map(str::to_string).collect()).unwrap_or_default(),
        user_id: r.get(8)?,
        session_id: r.get(9)?,
        via: r.get(10)?,
        employee_id: r.get(11)?,
        run_id: r.get(12)?,
        task_id: r.get(13)?,
        ticket_id: r.get(14)?,
        at_ms: r.get(15)?,
        seq: r.get(16)?,
        version: r.get(17)?,
        username: r.get(18)?,
        employee_name: r.get(19)?,
    })
}

/// Append an audit row. Its scope is the item's (a key export: the company).
pub fn insert_audit(tx: &Transaction<'_>, audit: &NewAudit<'_>) -> Result<AuditRow, StorageError> {
    let invalid = || StorageError::Rejected(INVALID_AUDIT.into());
    let action = audit.action;
    if (action == AuditAction::KeyExport) != audit.item_id.is_none()
        || (audit.user_id.is_none() && audit.employee_id.is_none() && audit.run_id.is_none())
        || audit.changed_fields.iter().any(|f| !FIELDS.contains(f))
        || (action == AuditAction::KeyExport && audit.version.is_some())
        || audit.version.is_some_and(|v| v < 1)
    {
        return Err(invalid());
    }
    let (company, project, current): (String, Option<String>, Option<i64>) = match audit.item_id {
        Some(item) => tx
            .query_row("SELECT company_id, project_id, version FROM vault_items WHERE id = ?1", [item], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .optional()?
            .ok_or_else(|| StorageError::Rejected(NOT_FOUND.into()))?,
        None => (company_id(tx)?, None, None),
    };
    let version = audit.version.or(current);
    let fields = (!audit.changed_fields.is_empty()).then(|| audit.changed_fields.join(","));
    tx.execute(
        "INSERT INTO vault_audit(id, item_id, company_id, project_id, action, changed_fields, user_id, session_id, via,
                                 employee_id, run_id, task_id, ticket_id, version, at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        params![
            audit.id,
            audit.item_id,
            company,
            project,
            action.as_str(),
            fields,
            audit.user_id,
            audit.session_id,
            audit.via,
            audit.employee_id,
            audit.run_id,
            audit.task_id,
            audit.ticket_id,
            version,
            audit.at_ms
        ],
    )?;
    Ok(tx.query_row(
        &format!("SELECT {AUDIT_COLUMNS} FROM {AUDIT_FROM} WHERE a.id = ?1"),
        [audit.id],
        audit_row,
    )?)
}

/// The audit log, newest first, one query per page (the item's and the
/// actor's names come with each row): by seq (the order rows were written in, the
/// next page `before_seq` the last row's seq), or by time when `before` is set
/// (`at_ms` DESC, `id` DESC; the next page at [`AuditRow::cursor`]).
pub fn list_audit(conn: &Connection, filter: &AuditFilter<'_>) -> Result<Vec<AuditRow>, StorageError> {
    fn arg(args: &mut Vec<Value>, v: impl Into<Value>) -> usize {
        args.push(v.into());
        args.len()
    }
    let mut args: Vec<Value> = Vec::new();
    let mut clauses: Vec<String> = Vec::new();
    if let Some(item) = filter.item_id {
        clauses.push(format!("a.item_id = ?{}", arg(&mut args, item.to_string())));
    }
    match filter.scope {
        None => {}
        Some(VaultScope::Company) => clauses.push("a.project_id IS NULL".into()),
        Some(VaultScope::Project(p)) => clauses.push(format!("a.project_id = ?{}", arg(&mut args, p.to_string()))),
        Some(VaultScope::ProjectAndCompany(p)) => {
            clauses.push(format!("(a.project_id IS NULL OR a.project_id = ?{})", arg(&mut args, p.to_string())))
        }
    }
    match filter.actor {
        None => {}
        Some(AuditActor::User(u)) => clauses.push(format!("a.user_id = ?{}", arg(&mut args, u.to_string()))),
        Some(AuditActor::Employee(e)) => clauses.push(format!("a.employee_id = ?{}", arg(&mut args, e.to_string()))),
    }
    if let Some(action) = filter.action {
        clauses.push(format!("a.action = ?{}", arg(&mut args, action.as_str().to_string())));
    }
    if let Some(since) = filter.since_ms {
        clauses.push(format!("a.at_ms >= ?{}", arg(&mut args, since)));
    }
    if let Some(before) = &filter.before {
        let at = arg(&mut args, before.at_ms);
        let id = arg(&mut args, before.id.clone());
        clauses.push(format!("(a.at_ms < ?{at} OR (a.at_ms = ?{at} AND a.id < ?{id}))"));
    }
    if let Some(seq) = filter.before_seq {
        clauses.push(format!("a.seq < ?{}", arg(&mut args, seq)));
    }
    let order = if filter.before.is_some() { "a.at_ms DESC, a.id DESC" } else { "a.seq DESC" };
    let limit = if filter.limit == 0 { 50 } else { filter.limit };
    let n = arg(&mut args, limit as i64);
    let where_sql = if clauses.is_empty() { String::new() } else { format!("WHERE {}", clauses.join(" AND ")) };
    let sql = format!(
        "SELECT {AUDIT_COLUMNS} FROM {AUDIT_FROM}
         {where_sql} ORDER BY {order} LIMIT ?{n}"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(args), audit_row)?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}
