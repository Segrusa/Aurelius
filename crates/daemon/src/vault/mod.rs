//! The vault on the server (feature `vault`, Sentinel SEC0).
//!
//! * Its key ([`key`]) and cryptography ([`crypto`]); the import of a key
//!   from the command line ([`import`]).
//! * Its writes, before the host pipeline (E5): the value is sealed in the
//!   write transaction, the request hash holds an HMAC of it under the vault's
//!   own key (never a plain hash), a replay with another value is a conflict,
//!   and the receipt keeps only `{item_id, version}`.
//! * The redactor: every secret value the server stores (current and previous
//!   versions, archived items too) is in it (E16). A new value goes in
//!   **before** its transaction commits, so no run can get a value it lacks.
//! * Delivery: only a work run (E15); the items of
//!   the company and the run's project, names checked again (E7); one audit row
//!   `deliver` per item and run; without a key nothing at all.

pub mod crypto;
pub mod import;
pub mod key;

use crate::Shared;
use aurelius_domain::vault::{RevealPurpose, UserRef, VaultActor, VaultAuditAction, VaultAuditPage, VaultAuditRow, VaultEventMeta, VaultItemView, VaultList, VaultScopeCount, VaultValue};
use aurelius_domain::{CommandId, EventKind, ProjectId, UserId, VaultItemId};
use aurelius_domain::events::EventNotice;
use aurelius_protocol::{ErrorCode, ProtocolError, VaultCreateParams, VaultUpdateParams};
use aurelius_runtime::redact;
use aurelius_storage::StorageError;
use aurelius_storage::vault::{self as store, AuditAction, NewAudit, VaultKind as StoredKind, VaultScope};
use crypto::{Binding, Keys};
use rusqlite::{Connection, OptionalExtension, Transaction};
use std::sync::{Arc, Mutex};
use zeroize::Zeroizing;

/// The server's vault: its key and whose redactor set it fills.
pub struct Vault {
    store: key::Store,
    state: Mutex<Option<key::Loaded>>,
    /// The redactor's set of this daemon (its data folder).
    owner: String,
    /// The database, to fill the redactor when the key becomes readable.
    db: Arc<aurelius_storage::Store>,
    /// The items whose env name a run may no longer have that the Inbox
    /// was told about, with that name: once each, not at every run (SEC1 m4).
    told_refused: Mutex<std::collections::HashSet<(String, String)>>,
}

impl Vault {
    pub fn open(root: &std::path::Path, own_root: bool, db: Arc<aurelius_storage::Store>) -> Self {
        let vault = Self { store: key::Store::for_root(root, own_root), state: Mutex::new(None), owner: root.display().to_string(), db, told_refused: Mutex::default() };
        if let Err(why) = vault.keys() {
            tracing::error!(%why, "the vault is unavailable");
        }
        vault
    }

    /// The key, read again while there is none (a key imported or made
    /// private since): none yet, or why it cannot be used.
    pub fn keys(&self) -> Result<Option<Arc<Keys>>, String> {
        Ok(self.loaded()?.map(|l| l.keys))
    }

    fn loaded(&self) -> Result<Option<key::Loaded>, String> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.is_none() {
            let Some(loaded) = self.store.load()? else { return Ok(None) };
            // Readable only now (a keychain unlocked, a key file made private
            // after the start): every stored value into the redactor before
            // the key is used (E16, SEC1 M1). When that fails the key is not
            // kept, so nothing is delivered or shown and the next call tries
            // again (SEC2 m1).
            self.fill_redactor(Some(loaded.keys.clone())).map_err(|why| format!("the vault's values could not be loaded into the redactor: {why}"))?;
            *state = Some(loaded);
        }
        Ok(state.clone())
    }

    /// Every secret value stored, opened, into this daemon's redactor set
    /// (E16). Values that do not open cannot be delivered either. In a write
    /// transaction, so no write's new value (added before its commit) can be
    /// lost between the read and the install. Never from inside a write.
    fn fill_redactor(&self, keys: Option<Arc<Keys>>) -> Result<usize, String> {
        let owner = self.owner.clone();
        self.db
            .write(move |tx| {
                let values = store::redaction_values(tx)?;
                let secrets: Vec<redact::Secret> = match keys {
                    None => Vec::new(),
                    Some(keys) => values
                        .iter()
                        .filter(|v| v.kind == StoredKind::Secret)
                        .filter_map(|v| {
                            let binding = Binding::of_value(v)?;
                            let value = keys.open(&binding, &v.sealed).ok()?;
                            Some(redact::Secret { name: marker_name(v.env_name.as_deref(), &v.name), label: v.name.clone(), value })
                        })
                        .collect(),
                };
                let count = secrets.len();
                redact::install_set(&owner, secrets);
                Ok(count)
            })
            .map_err(|e| e.to_string())
    }

    /// The redactor built again from what is stored: at the start, and after a
    /// refused write whose value went in before it was stored (R1 m5).
    pub fn refresh_redactor(&self) -> Result<usize, String> {
        let keys = self.keys()?;
        self.fill_redactor(keys)
    }

    /// After a write that added `value` to the redactor and was not stored:
    /// the redactor again of what is stored only (R1 m5).
    fn forget_unstored<T>(&self, added: bool, outcome: &Result<Result<T, ProtocolError>, StorageError>) {
        if added && !matches!(outcome, Ok(Ok(_))) {
            if let Err(why) = self.refresh_redactor() {
                tracing::warn!(%why, "the vault's redactor could not be built again");
            }
        }
    }

    /// The key for a write: made now when there is none and nothing is
    /// encrypted yet (E4).
    pub fn keys_for_write(&self, any_ciphertext: impl FnOnce() -> Result<bool, String>) -> Result<Arc<Keys>, String> {
        if let Some(keys) = self.keys()? {
            return Ok(keys);
        }
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let loaded = self.store.load_or_create(any_ciphertext)?;
        let keys = loaded.keys.clone();
        *state = Some(loaded);
        Ok(keys)
    }

    pub fn owner(&self) -> &str {
        &self.owner
    }
}

fn unavailable(why: String) -> ProtocolError {
    tracing::warn!(%why, "vault unavailable");
    ProtocolError::new(ErrorCode::VaultUndecryptable, "The vault key of this server is missing or cannot be read. Import it (aurelius-daemon vault-key import), then restart.")
}

pub(crate) fn storage(err: StorageError) -> ProtocolError {
    match &err {
        StorageError::Rejected(why) => match why.as_str() {
            store::NOT_FOUND => ProtocolError::new(ErrorCode::NotFound, "This vault item does not exist."),
            store::ARCHIVED => ProtocolError::new(ErrorCode::Validation, "This vault item was deleted."),
            store::PROJECT_ARCHIVED => ProtocolError::new(ErrorCode::Validation, "This project is archived: its vault can only be read."),
            store::NAME_TAKEN => ProtocolError::new(ErrorCode::Validation, "Another item in this scope has this name."),
            store::ENV_NAME_TAKEN => ProtocolError::new(ErrorCode::Validation, "Another item in this scope has this key."),
            store::KIND_SECRET => ProtocolError::new(ErrorCode::VaultKindLocked, "A secret stays a secret, and nothing becomes one: make a new item."),
            store::VERSION_MISMATCH => ProtocolError::new(ErrorCode::VersionConflict, "This item changed in the meantime. Reload it."),
            _ => ProtocolError::new(ErrorCode::Validation, "This vault change is not possible."),
        },
        _ => {
            tracing::error!(error = %err, "vault storage failure");
            ProtocolError::new(ErrorCode::StorageUnavailable, "Could not read or save the vault.")
        }
    }
}

fn validation(message: impl Into<String>) -> ProtocolError {
    ProtocolError::new(ErrorCode::Validation, message)
}

fn domain_kind(kind: StoredKind) -> aurelius_domain::vault::VaultKind {
    match kind {
        StoredKind::Text => aurelius_domain::vault::VaultKind::Text,
        StoredKind::Url => aurelius_domain::vault::VaultKind::Url,
        StoredKind::Secret => aurelius_domain::vault::VaultKind::Secret,
    }
}

fn stored_kind(kind: aurelius_domain::vault::VaultKind) -> StoredKind {
    match kind {
        aurelius_domain::vault::VaultKind::Text => StoredKind::Text,
        aurelius_domain::vault::VaultKind::Url => StoredKind::Url,
        aurelius_domain::vault::VaultKind::Secret => StoredKind::Secret,
    }
}

/// The name a value is redacted under: its env name, else its name.
fn marker_name(env_name: Option<&str>, name: &str) -> String {
    env_name.filter(|e| !e.is_empty()).unwrap_or(name).to_owned()
}

/// Every secret value stored, opened, into this daemon's redactor set (E16):
/// at the start, after an import and as a check.
pub fn refresh_redactor(shared: &Shared) -> Result<usize, String> {
    shared.vault.refresh_redactor()
}

fn company_id(conn: &Connection) -> Result<String, StorageError> {
    Ok(conn.query_row("SELECT id FROM company", [], |r| r.get(0))?)
}

fn user_ref(conn: &Connection, id: &str) -> Result<UserRef, StorageError> {
    let username = aurelius_storage::users::get(conn, id)?.map(|u| u.username).unwrap_or_default();
    Ok(UserRef { id: UserId::parse(id).unwrap_or_default(), username })
}

/// One item as a client sees it: a text's or url's value, never a secret's.
fn view(conn: &Connection, keys: Option<&Keys>, row: &store::VaultItemRow, overrides_company: bool) -> Result<VaultItemView, StorageError> {
    let opened = match (keys, Binding::of_row(row)) {
        (Some(keys), Some(binding)) => keys.open(&binding, &row.sealed).ok(),
        _ => None,
    };
    let undecryptable = opened.is_none();
    let value = opened.filter(|_| row.kind != StoredKind::Secret).map(|v| v.to_string());
    Ok(VaultItemView {
        id: VaultItemId::parse(&row.id).unwrap_or_default(),
        project_id: row.project_id.as_deref().and_then(|p| ProjectId::parse(p).ok()),
        kind: domain_kind(row.kind),
        name: row.name.clone(),
        env_name: row.env_name.clone().unwrap_or_default(),
        description: row.description.clone(),
        value,
        undecryptable,
        overrides_company,
        version: u64::try_from(row.version).unwrap_or_default(),
        created_by: user_ref(conn, &row.created_by_user)?,
        updated_by: user_ref(conn, &row.updated_by_user)?,
        created_at_ms: row.created_at_ms,
        updated_at_ms: row.updated_at_ms,
    })
}

/// `vault.list`.
pub fn list(conn: &Connection, keys: Option<&Keys>, project: Option<ProjectId>) -> Result<VaultList, StorageError> {
    let (items, company_items) = match project {
        None => (store::list(conn, VaultScope::Company, false)?.iter().map(|r| view(conn, keys, r, false)).collect::<Result<_, _>>()?, Vec::new()),
        Some(p) => {
            let p = p.to_string();
            let company = store::list(conn, VaultScope::Company, false)?;
            let own = store::list(conn, VaultScope::Project(&p), false)?;
            let overrides = |r: &store::VaultItemRow| company.iter().any(|c| c.name.eq_ignore_ascii_case(&r.name) || (c.env_name.is_some() && c.env_name == r.env_name));
            let items = own.iter().map(|r| view(conn, keys, r, overrides(r))).collect::<Result<_, _>>()?;
            let company_items = company.iter().map(|r| view(conn, keys, r, false)).collect::<Result<_, _>>()?;
            (items, company_items)
        }
    };
    let mut counts = vec![VaultScopeCount { project_id: None, count: 0 }];
    let mut stmt = conn.prepare("SELECT project_id, COUNT(*) FROM vault_items WHERE archived_at_ms IS NULL GROUP BY project_id ORDER BY project_id IS NOT NULL, project_id")?;
    for row in stmt.query_map([], |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, i64>(1)?)))? {
        let (project, count) = row?;
        match project.and_then(|p| ProjectId::parse(&p).ok()) {
            None => counts[0].count = count as u32,
            Some(p) => counts.push(VaultScopeCount { project_id: Some(p), count: count as u32 }),
        }
    }
    Ok(VaultList { project_id: project, items, company_items, counts })
}

/// Who does it, for the audit: a board member on their session's device.
#[derive(Debug, Clone)]
pub struct BoardActor {
    pub user: UserId,
    pub session_id: Option<String>,
    pub via: Option<String>,
}

/// A board member's audit row; `version` the value's version it concerns
/// (SEC1 m3; `None` for a key export).
fn new_audit<'a>(id: &'a str, item: Option<&'a str>, version: Option<i64>, action: AuditAction, by: &'a BoardActor, user: &'a str) -> NewAudit<'a> {
    NewAudit { user_id: Some(user), session_id: by.session_id.as_deref(), via: by.via.as_deref(), version, ..NewAudit::new(id, item, action, aurelius_storage::now_ms()) }
}

/// `vault.reveal`: the value, with an audit row `reveal` or `copy`.
pub fn reveal(tx: &Transaction<'_>, keys: &Keys, item_id: VaultItemId, purpose: RevealPurpose, by: &BoardActor) -> Result<Result<VaultValue, ProtocolError>, StorageError> {
    let Some(row) = store::get(tx, &item_id.to_string())?.filter(|r| r.archived_at_ms.is_none()) else {
        return Ok(Err(ProtocolError::new(ErrorCode::NotFound, "This vault item does not exist.")));
    };
    let Some(value) = Binding::of_row(&row).and_then(|b| keys.open(&b, &row.sealed).ok()) else {
        return Ok(Err(ProtocolError::new(ErrorCode::VaultUndecryptable, "This value cannot be decrypted with this server's vault key.")));
    };
    let (id, user) = (aurelius_domain::EventId::new().to_string(), by.user.to_string());
    let action = match purpose {
        RevealPurpose::Show => AuditAction::Reveal,
        RevealPurpose::Copy => AuditAction::Copy,
    };
    store::insert_audit(tx, &new_audit(&id, Some(&row.id), Some(row.version), action, by, &user))?;
    Ok(Ok(VaultValue { item_id, value: value.to_string() }))
}

fn audit_action(action: AuditAction) -> VaultAuditAction {
    match action {
        AuditAction::Create => VaultAuditAction::Create,
        AuditAction::Edit => VaultAuditAction::Edit,
        AuditAction::Replace => VaultAuditAction::Replace,
        AuditAction::Archive => VaultAuditAction::Archive,
        AuditAction::Reveal => VaultAuditAction::Reveal,
        AuditAction::Copy => VaultAuditAction::Copy,
        AuditAction::Deliver => VaultAuditAction::Deliver,
        AuditAction::ToolRead => VaultAuditAction::ToolRead,
        AuditAction::KeyExport => VaultAuditAction::KeyExport,
    }
}

/// `vault.audit`, newest first. `before` and the rows' `id` are the audit
/// rows' `seq` (the order they were written in), newest highest. With an
/// item the item's own scope counts, archived or not (QA2-M1): a `project_id`
/// that is not the item's, or an unknown item, gives an empty page.
pub fn audit(conn: &Connection, project: Option<ProjectId>, all: bool, item: Option<VaultItemId>, before: Option<i64>, limit: Option<u32>) -> Result<VaultAuditPage, StorageError> {
    let limit = limit.unwrap_or(50).clamp(1, 200) as usize;
    let (mut project, item) = (project.map(|p| p.to_string()), item.map(|i| i.to_string()));
    if let (false, Some(id)) = (all, item.as_deref()) {
        match store::get(conn, id)? {
            Some(row) if project.is_none() || project == row.project_id => project = row.project_id,
            _ => return Ok(VaultAuditPage { rows: Vec::new(), more: false }),
        }
    }
    let scope = match (all, project.as_deref()) {
        (true, _) => None,
        (false, Some(p)) => Some(VaultScope::Project(p)),
        (false, None) => Some(VaultScope::Company),
    };
    let filter = store::AuditFilter { item_id: item.as_deref(), scope, before_seq: before, limit: limit + 1, ..Default::default() };
    let found = store::list_audit(conn, &filter)?;
    let more = found.len() > limit;
    let mut rows = Vec::new();
    // One query a page: the actors' names come with the rows (R1 m1).
    for row in found.into_iter().take(limit) {
        let actor = match (row.user_id, row.employee_id) {
            (Some(user), _) => VaultActor::BoardMember { user: UserRef { id: UserId::parse(&user).unwrap_or_default(), username: row.username.unwrap_or_default() } },
            (None, employee) => {
                let employee_id = employee.as_deref().and_then(|e| aurelius_domain::EmployeeId::parse(e).ok()).unwrap_or_default();
                let name = row.employee_name.unwrap_or_default();
                VaultActor::Employee {
                    employee_id,
                    name,
                    run_id: row.run_id.as_deref().and_then(|r| aurelius_domain::RunId::parse(r).ok()),
                    task_id: row.task_id.as_deref().and_then(|t| aurelius_domain::TaskId::parse(t).ok()),
                    ticket_id: row.ticket_id.as_deref().and_then(|t| aurelius_domain::TicketId::parse(t).ok()),
                }
            }
        };
        rows.push(VaultAuditRow {
            id: row.seq,
            item_id: row.item_id.as_deref().and_then(|i| VaultItemId::parse(i).ok()),
            item_name: row.item_name,
            project_id: row.project_id.as_deref().and_then(|p| ProjectId::parse(p).ok()),
            action: audit_action(row.action),
            actor,
            changed_fields: row.changed_fields.into_iter().filter(|f| f != "value").collect(),
            via: row.via,
            at_ms: row.at_ms,
        });
    }
    Ok(VaultAuditPage { rows, more })
}

/// A value as the vault keeps it: trimmed (spaces and line ends, E12).
fn trimmed(value: &str) -> Zeroizing<String> {
    Zeroizing::new(value.trim().to_owned())
}

/// A name or description holding a known secret value is refused (E18); so is
/// a project note's text (R1 m7).
pub(crate) fn refuse_secret_text(texts: &[Option<&str>]) -> Result<(), ProtocolError> {
    let redactor = redact::current();
    let mut names: Vec<String> = Vec::new();
    for text in texts.iter().flatten() {
        for name in redactor.names_in(text) {
            if !names.contains(&name) {
                names.push(name);
            }
        }
    }
    if names.is_empty() { Ok(()) } else { Err(aurelius_domain::AppError::looks_like_secret(names)) }
}

/// What a vault write answers: the item, and whether it was a replay.
pub struct Written {
    pub item: VaultItemView,
    pub replayed: bool,
    pub seq: u64,
    pub notices: Vec<EventNotice>,
}

/// The receipt of a vault write: only `{item_id, version}` (E5).
fn receipt_json(item_id: &str, version: i64) -> String {
    serde_json::json!({ "item_id": item_id, "version": version }).to_string()
}

/// A replay (same key, same request hash) answers the item as it is now,
/// without a value; another request under the key is a conflict.
fn replayed(tx: &Transaction<'_>, keys: &Keys, key: &str, hash: &str) -> Result<Option<Result<Written, ProtocolError>>, StorageError> {
    let Some(stored) = aurelius_storage::events::find_receipt(tx, key)? else { return Ok(None) };
    if stored.request_hash != hash {
        return Ok(Some(Err(ProtocolError::new(ErrorCode::IdempotencyConflict, "This command ID was already used for a different request."))));
    }
    let item_id = serde_json::from_str::<serde_json::Value>(&stored.result_json).ok().and_then(|v| v["item_id"].as_str().map(str::to_owned)).unwrap_or_default();
    let Some(row) = store::get(tx, &item_id)? else {
        return Ok(Some(Err(ProtocolError::new(ErrorCode::NotFound, "This vault item does not exist."))));
    };
    let mut item = view(tx, Some(keys), &row, false)?;
    item.value = None;
    Ok(Some(Ok(Written { item, replayed: true, seq: stored.committed_seq, notices: Vec::new() })))
}

fn event(tx: &Transaction<'_>, kind: EventKind, row: &store::VaultItemRow, by: UserId, ctx_meta: &serde_json::Value, key: &str, changed: &[&str]) -> Result<EventNotice, StorageError> {
    let meta = VaultEventMeta {
        id: VaultItemId::parse(&row.id).unwrap_or_default(),
        name: row.name.clone(),
        kind: domain_kind(row.kind),
        project_id: row.project_id.as_deref().and_then(|p| ProjectId::parse(p).ok()),
        version: u64::try_from(row.version).unwrap_or_default(),
        by,
        changed_fields: changed.iter().map(|f| f.to_string()).collect(),
    };
    let mut metadata = serde_json::to_value(&meta).unwrap_or_default();
    if let (Some(m), Some(ctx)) = (metadata.as_object_mut(), ctx_meta.as_object()) {
        for (k, v) in ctx {
            m.entry(k.clone()).or_insert(v.clone());
        }
    }
    let appended = aurelius_storage::events::append(
        tx,
        aurelius_storage::events::NewEvent {
            aggregate_type: "vault",
            aggregate_id: row.id.clone(),
            aggregate_version: aurelius_storage::conversations::next_version(tx, "vault", &row.id)?,
            event_type: kind.as_str(),
            project_id: meta.project_id,
            actor_kind: aurelius_domain::ActorKind::User,
            actor_id: Some(by.to_string()),
            correlation_id: key.to_owned(),
            causation_id: None,
            payload_blob_id: None,
            metadata,
            dedupe_key: format!("vault:{key}"),
        },
    )?;
    Ok(EventNotice { seq: appended.seq, kind, aggregate_id: row.id.clone(), project_id: meta.project_id, occurred_at_ms: aurelius_storage::now_ms() })
}

/// The request hash of a vault write (E5): the method, what the write says
/// besides the value, and an HMAC of the value under the vault's `idem` key.
pub fn request_hash(keys: &Keys, method: &str, expected_version: Option<u64>, fields: serde_json::Value, value: Option<&str>) -> String {
    let mac = value.map(|v| keys.idem(v.as_bytes()));
    aurelius_application::request_hash_of(&(method, expected_version, fields, mac))
}

/// `vault.create`.
pub fn create(
    shared: &Shared,
    key: CommandId,
    expected_version: Option<u64>,
    params: VaultCreateParams,
    by: BoardActor,
    ctx_meta: serde_json::Value,
) -> Result<Written, ProtocolError> {
    let VaultCreateParams { project_id, kind, name, env_name, description, value } = params;
    let value = trimmed(&value);
    check_new(kind, &value, &name, &env_name, description.as_deref())?;
    let store = shared.store().clone();
    let keys = shared.vault.keys_for_write(|| store.read(store::any_ciphertext).map_err(|e| e.to_string())).map_err(unavailable)?;
    let description = description.map(|d| d.trim().to_owned()).filter(|d| !d.is_empty());
    let fields = serde_json::json!({ "project_id": project_id, "kind": kind, "name": name.trim(), "env_name": env_name, "description": description });
    let hash = request_hash(&keys, "vault.create", expected_version, fields, Some(&value));
    let owner = shared.vault.owner().to_owned();
    let added = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = added.clone();
    let outcome = shared.store().write(move |tx| {
        let key_text = key.to_string();
        if let Some(answer) = replayed(tx, &keys, &key_text, &hash)? {
            return Ok(answer);
        }
        let item_id = VaultItemId::new().to_string();
        let company = company_id(tx)?;
        let project = project_id.map(|p| p.to_string());
        let skind = stored_kind(kind);
        let Some(binding) = Binding::parse(&item_id, 1, skind, &company, project.as_deref()) else {
            return Ok(Err(validation("This scope cannot hold vault items.")));
        };
        let Ok(sealed) = keys.seal(&binding, value.as_bytes()) else {
            return Ok(Err(ProtocolError::new(ErrorCode::Internal, "The value could not be encrypted.")));
        };
        // In the redactor before it can be delivered (E12).
        if kind == aurelius_domain::vault::VaultKind::Secret {
            redact::add_to_set(&owner, redact::Secret { name: env_name.clone(), label: name.trim().to_owned(), value: value.clone() });
            flag.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        let user = by.user.to_string();
        let inserted = store::insert(
            tx,
            store::NewVaultItem {
                id: &item_id,
                project_id: project.as_deref(),
                kind: skind,
                name: name.trim(),
                env_name: Some(&env_name),
                description: description.as_deref(),
                sealed,
                by_user: &user,
                now_ms: aurelius_storage::now_ms(),
            },
        );
        let row = match inserted {
            Ok(row) => row,
            Err(StorageError::Rejected(why)) => return Ok(Err(storage(StorageError::Rejected(why)))),
            Err(e) => return Err(e),
        };
        let audit_id = aurelius_domain::EventId::new().to_string();
        store::insert_audit(tx, &new_audit(&audit_id, Some(&row.id), Some(row.version), AuditAction::Create, &by, &user))?;
        let notice = event(tx, EventKind::VaultItemCreated, &row, by.user, &ctx_meta, &key_text, &[])?;
        aurelius_storage::events::insert_receipt(tx, &key_text, &hash, "vault.create", &receipt_json(&row.id, row.version), notice.seq)?;
        let item = view(tx, Some(&keys), &row, false)?;
        Ok(Ok(Written { item, replayed: false, seq: notice.seq, notices: vec![notice] }))
    });
    shared.vault.forget_unstored(added.load(std::sync::atomic::Ordering::Relaxed), &outcome);
    outcome.map_err(storage)?
}

/// The checks of a new item, before anything is sealed.
pub fn check_new(kind: aurelius_domain::vault::VaultKind, value: &str, name: &str, env_name: &str, description: Option<&str>) -> Result<(), ProtocolError> {
    aurelius_domain::vault::check_name(name).map_err(validation)?;
    aurelius_domain::vault::check_env_name(env_name).map_err(validation)?;
    aurelius_domain::vault::check_value(kind, value).map_err(validation)?;
    refuse_secret_text(&[Some(name), description])
}

/// `vault.update`: what is given changes; a new value is sealed for the next
/// version in the same transaction that stores it.
pub fn update(
    shared: &Shared,
    key: CommandId,
    expected_version: Option<u64>,
    params: VaultUpdateParams,
    by: BoardActor,
    ctx_meta: serde_json::Value,
) -> Result<Written, ProtocolError> {
    let VaultUpdateParams { item_id, name, env_name, description, value, kind } = params;
    let value = value.as_deref().map(trimmed);
    if let Some(name) = &name {
        aurelius_domain::vault::check_name(name).map_err(validation)?;
    }
    if let Some(env_name) = &env_name {
        aurelius_domain::vault::check_env_name(env_name).map_err(validation)?;
    }
    refuse_secret_text(&[name.as_deref(), description.as_deref()])?;
    let store = shared.store().clone();
    let keys = shared.vault.keys_for_write(|| store.read(store::any_ciphertext).map_err(|e| e.to_string())).map_err(unavailable)?;
    let fields = serde_json::json!({ "item_id": item_id, "name": name, "env_name": env_name, "description": description, "kind": kind });
    let hash = request_hash(&keys, "vault.update", expected_version, fields, value.as_ref().map(|v| v.as_str()));
    let owner = shared.vault.owner().to_owned();
    let added = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = added.clone();
    let outcome = shared.store().write(move |tx| {
        let key_text = key.to_string();
        if let Some(answer) = replayed(tx, &keys, &key_text, &hash)? {
            return Ok(answer);
        }
        let Some(row) = store::get(tx, &item_id.to_string())? else {
            return Ok(Err(storage(StorageError::Rejected(store::NOT_FOUND.into()))));
        };
        let target = kind.map(stored_kind).unwrap_or(row.kind);
        if target != row.kind && (target == StoredKind::Secret || row.kind == StoredKind::Secret) {
            return Ok(Err(storage(StorageError::Rejected(store::KIND_SECRET.into()))));
        }
        // B7 (Titan): text and url trade places with the value they have: it is
        // opened and sealed again for the new kind, as an edit of the kind.
        let resealed = match (target != row.kind, &value) {
            (true, None) => match Binding::of_row(&row).and_then(|b| keys.open(&b, &row.sealed).ok()) {
                Some(current) => Some(current),
                None => return Ok(Err(ProtocolError::new(ErrorCode::VaultUndecryptable, "This value cannot be decrypted with this server's vault key."))),
            },
            _ => None,
        };
        let value = value.or(resealed.clone());
        if let Some(value) = &value {
            if let Err(why) = aurelius_domain::vault::check_value(domain_kind(target), value) {
                return Ok(Err(validation(if resealed.is_some() { format!("The current value does not fit: {why}") } else { why })));
            }
        }
        let sealed = match &value {
            None => None,
            Some(value) => {
                let Some(binding) = Binding::parse(&row.id, row.version + 1, target, &row.company_id, row.project_id.as_deref()) else {
                    return Ok(Err(validation("This item cannot change.")));
                };
                let Ok(sealed) = keys.seal(&binding, value.as_bytes()) else {
                    return Ok(Err(ProtocolError::new(ErrorCode::Internal, "The value could not be encrypted.")));
                };
                if target == StoredKind::Secret {
                    let label = name.as_deref().unwrap_or(&row.name);
                    let name = marker_name(env_name.as_deref().or(row.env_name.as_deref()), &row.name);
                    redact::add_to_set(&owner, redact::Secret { name, label: label.trim().to_owned(), value: value.clone() });
                    flag.store(true, std::sync::atomic::Ordering::Relaxed);
                }
                Some(sealed)
            }
        };
        let user = by.user.to_string();
        let description = description.as_deref().map(str::trim).map(|d| (!d.is_empty()).then_some(d));
        let change = store::VaultUpdate {
            name: name.as_deref().map(str::trim),
            env_name: env_name.as_deref().map(Some),
            description,
            kind: kind.map(stored_kind),
            sealed,
            expected_version: Some(expected_version.map_or(row.version, |v| v as i64)),
            by_user: &user,
            now_ms: aurelius_storage::now_ms(),
        };
        let updated = match store::update(tx, &row.id, change) {
            Ok(updated) => updated,
            Err(StorageError::Rejected(why)) => return Ok(Err(storage(StorageError::Rejected(why)))),
            Err(e) => return Err(e),
        };
        // The same value sealed again for its new kind is no new value.
        let (action, changed) = match resealed {
            None => (updated.action(), updated.changed.clone()),
            Some(_) => (Some(AuditAction::Edit), updated.changed.iter().copied().filter(|f| *f != "value").collect()),
        };
        let mut notices = Vec::new();
        let seq = match action {
            None => aurelius_storage::queries::last_seq(tx)?,
            Some(action) => {
                let audit_id = aurelius_domain::EventId::new().to_string();
                store::insert_audit(tx, &NewAudit { changed_fields: &changed, ..new_audit(&audit_id, Some(&row.id), Some(updated.item.version), action, &by, &user) })?;
                let notice = event(tx, EventKind::VaultItemUpdated, &updated.item, by.user, &ctx_meta, &key_text, &changed)?;
                let seq = notice.seq;
                notices.push(notice);
                seq
            }
        };
        aurelius_storage::events::insert_receipt(tx, &key_text, &hash, "vault.update", &receipt_json(&updated.item.id, updated.item.version), seq)?;
        let item = view(tx, Some(&keys), &updated.item, false)?;
        Ok(Ok(Written { item, replayed: false, seq, notices }))
    });
    shared.vault.forget_unstored(added.load(std::sync::atomic::Ordering::Relaxed), &outcome);
    outcome.map_err(storage)?
}

/// `vault.archive` ("Delete"): runs no longer get it; its values stay in the
/// redactor (E16) and its audit stays.
pub fn archive(shared: &Shared, key: CommandId, item_id: VaultItemId, by: BoardActor, ctx_meta: serde_json::Value) -> Result<Written, ProtocolError> {
    let keys = shared.vault.keys().map_err(unavailable)?;
    let hash = aurelius_application::request_hash_of(&("vault.archive", item_id));
    shared
        .store()
        .write(move |tx| {
            let key_text = key.to_string();
            if let Some(stored) = aurelius_storage::events::find_receipt(tx, &key_text)? {
                if stored.request_hash != hash {
                    return Ok(Err(ProtocolError::new(ErrorCode::IdempotencyConflict, "This command ID was already used for a different request.")));
                }
                let Some(row) = store::get(tx, &item_id.to_string())? else { return Ok(Err(storage(StorageError::Rejected(store::NOT_FOUND.into())))) };
                let mut item = view(tx, keys.as_deref(), &row, false)?;
                item.value = None;
                return Ok(Ok(Written { item, replayed: true, seq: stored.committed_seq, notices: Vec::new() }));
            }
            let user = by.user.to_string();
            let row = match store::archive(tx, &item_id.to_string(), &user, aurelius_storage::now_ms()) {
                Ok(row) => row,
                Err(StorageError::Rejected(why)) => return Ok(Err(storage(StorageError::Rejected(why)))),
                Err(e) => return Err(e),
            };
            let audit_id = aurelius_domain::EventId::new().to_string();
            store::insert_audit(tx, &new_audit(&audit_id, Some(&row.id), Some(row.version), AuditAction::Archive, &by, &user))?;
            let notice = event(tx, EventKind::VaultItemArchived, &row, by.user, &ctx_meta, &key_text, &[])?;
            aurelius_storage::events::insert_receipt(tx, &key_text, &hash, "vault.archive", &receipt_json(&row.id, row.version), notice.seq)?;
            let item = view(tx, keys.as_deref(), &row, false)?;
            Ok(Ok(Written { item, replayed: false, seq: notice.seq, notices: vec![notice] }))
        })
        .map_err(storage)?
}

/// `vault.key_export` once the password was checked again: a notification in the
/// Inbox of every board member (E9), an audit row `key_export`, then the key's line.
/// No notification (it could not be written): no key (SEC1 m4). The answer is never logged.
pub fn key_export(shared: &Shared, by: &BoardActor) -> Result<(aurelius_domain::vault::VaultKeyExport, Vec<EventNotice>), ProtocolError> {
    let loaded = shared.vault.loaded().map_err(unavailable)?;
    let Some(loaded) = loaded else {
        // Values without their key: the key is missing, not yet to be made (QA1 m2).
        if shared.store().read(store::any_ciphertext).map_err(storage)? {
            return Err(unavailable("values are stored but there is no vault key".into()));
        }
        return Err(ProtocolError::new(ErrorCode::NotFound, "This server has no vault key yet: it is made with the first item."));
    };
    let user = by.user.to_string();
    let username = shared.store().read(|c| Ok(aurelius_storage::users::get(c, &user)?.map(|u| u.username).unwrap_or_default())).map_err(storage)?;
    let via = by.via.as_deref().map(|v| format!(" ({v})")).unwrap_or_default();
    let text = format!("{username} exported the vault key{via}. Whoever has it can open every vault value and every backup of this server.");
    let notices = match shared.service.board_notice(&text) {
        Ok(notices) if !notices.is_empty() => notices,
        outcome => {
            if let Err(e) = outcome {
                tracing::warn!(error = %e.message, "the key export notice could not be written");
            }
            return Err(ProtocolError::new(ErrorCode::Validation, "The key is only exported with a notification every board member gets, and that notification could not be written."));
        }
    };
    let line = key::export_line(&loaded.master);
    let by_c = by.clone();
    shared
        .store()
        .write(move |tx| {
            let id = aurelius_domain::EventId::new().to_string();
            let user = by_c.user.to_string();
            store::insert_audit(tx, &new_audit(&id, None, None, AuditAction::KeyExport, &by_c, &user))?;
            Ok(())
        })
        .map_err(storage)?;
    Ok((aurelius_domain::vault::VaultKeyExport { key_id: loaded.keys.key_id(), key: line.to_string() }, notices))
}

/// What a run gets from the vault.
pub struct Delivery {
    pub env: aurelius_runtime::claude_code::VaultEnv,
    pub names: Vec<String>,
    pub notices: Vec<EventNotice>,
}

/// Where a run is: its project, employee, task, conversation and ticket.
struct RunPlace {
    project: Option<String>,
    employee: Option<String>,
    task: Option<String>,
    ticket: Option<String>,
}

/// The ticket is the one of the run's task or of a task above it (a
/// delegation's main task carries `ticket_id`), else the ticket whose
/// discussion the run answers in (MU4-09 AC1, R1 M2).
fn run_place(conn: &Connection, run: &str) -> Result<Option<RunPlace>, StorageError> {
    Ok(conn
        .query_row(
            "WITH RECURSIVE place AS (
                 SELECT c.project_id, COALESCE(r.employee_id, c.employee_id) AS employee_id,
                        COALESCE(r.task_id, (SELECT t.id FROM tasks t WHERE t.conversation_id = c.id)) AS task_id, c.id AS conversation_id
                 FROM runs r JOIN conversations c ON c.id = r.conversation_id WHERE r.id = ?1
             ),
             chain(id, parent, ticket, depth) AS (
                 SELECT t.id, t.parent_task_id, t.ticket_id, 0 FROM tasks t WHERE t.id = (SELECT task_id FROM place)
                 UNION ALL
                 SELECT t.id, t.parent_task_id, t.ticket_id, chain.depth + 1 FROM tasks t JOIN chain ON t.id = chain.parent WHERE chain.depth < 64
             )
             SELECT project_id, employee_id, task_id,
                    COALESCE((SELECT ticket FROM chain WHERE ticket IS NOT NULL ORDER BY depth LIMIT 1),
                             (SELECT k.id FROM tickets k WHERE k.conversation_id = place.conversation_id))
             FROM place",
            [run],
            |r| Ok(RunPlace { project: r.get(0)?, employee: r.get(1)?, task: r.get(2)?, ticket: r.get(3)? }),
        )
        .optional()?)
}

/// The vault of a work run (E7, E15, E16): the items of the company and its
/// project, with a name a run may get, that open with the key. A run without
/// a key gets nothing. One `deliver` audit
/// row per item.
pub fn deliver(shared: &Shared, run: aurelius_domain::RunId) -> Option<Delivery> {
    let keys = match shared.vault.keys() {
        Ok(Some(keys)) => keys,
        Ok(None) => return None,
        Err(why) => {
            tracing::warn!(%run, %why, "the vault is unavailable: this run gets nothing from it");
            return None;
        }
    };
    let run_text = run.to_string();
    let found = shared.store().read(|conn| {
        let Some(place) = run_place(conn, &run_text)? else { return Ok(None) };
        Ok(Some((store::effective(conn, place.project.as_deref())?, place)))
    });
    let (items, place) = match found {
        Ok(Some(found)) => found,
        Ok(None) => return None,
        Err(e) => {
            tracing::warn!(%run, error = %e, "the vault could not be read for this run");
            return None;
        }
    };
    let mut vars = Vec::new();
    let mut delivered = Vec::new();
    let mut refused = Vec::new();
    for effective in items {
        let row = effective.item;
        let Some(env_name) = row.env_name.clone() else { continue };
        if aurelius_domain::vault::check_env_name(&env_name).is_err() {
            let first = shared.vault.told_refused.lock().unwrap_or_else(|e| e.into_inner()).insert((row.id.clone(), env_name.clone()));
            if first {
                refused.push(env_name);
            }
            continue;
        }
        let Some(value) = Binding::of_row(&row).and_then(|b| keys.open(&b, &row.sealed).ok()) else {
            tracing::warn!(%run, item = %row.id, "a vault value does not open with this server's key: not delivered");
            continue;
        };
        let secret = row.kind == StoredKind::Secret;
        if secret && redact::current().names_in(&value).is_empty() {
            redact::add_to_set(shared.vault.owner(), redact::Secret { name: env_name.clone(), label: row.name.clone(), value: value.clone() });
        }
        vars.push(aurelius_runtime::claude_code::VaultVar { name: env_name.clone(), value, secret });
        delivered.push((row.id.clone(), row.version, env_name));
    }
    let mut notices = Vec::new();
    if !refused.is_empty() {
        let text = format!("A run did not get {} from the vault: Aurelius no longer lets runs have these names. Rename the item(s).", refused.join(", "));
        notices.extend(shared.service.board_notice(&text).unwrap_or_default());
    }
    if delivered.is_empty() {
        return (!notices.is_empty()).then(|| Delivery { env: Default::default(), names: Vec::new(), notices });
    }
    // The version of the value the run got (SEC1 m3), not the one stored when the row is written.
    let ids: Vec<(String, i64)> = delivered.iter().map(|(id, version, _)| (id.clone(), *version)).collect();
    let audited = shared.store().write(move |tx| {
        for (item, version) in &ids {
            let id = aurelius_domain::EventId::new().to_string();
            store::insert_audit(
                tx,
                &NewAudit {
                    employee_id: place.employee.as_deref(),
                    run_id: Some(&run_text),
                    task_id: place.task.as_deref(),
                    ticket_id: place.ticket.as_deref(),
                    version: Some(*version),
                    ..NewAudit::new(&id, Some(item), AuditAction::Deliver, aurelius_storage::now_ms())
                },
            )?;
        }
        Ok(())
    });
    if let Err(e) = audited {
        // No audit, no delivery.
        tracing::warn!(%run, error = %e, "the vault delivery could not be audited: nothing delivered");
        return None;
    }
    let names = delivered.into_iter().map(|(_, _, name)| name).collect();
    Some(Delivery { env: aurelius_runtime::claude_code::VaultEnv::new(vars), names, notices })
}

/// The line a work run with vault values gets in its instructions: the
/// names only (never a value).
pub fn prompt_note(names: &[String]) -> String {
    let list = names.iter().map(|n| format!("${n}")).collect::<Vec<_>>().join(", ");
    format!(
        "Vault: you have these environment variables from the company's vault: {list}. Use them only inside commands (for example curl -H \"Authorization: Bearer $NAME\"). Never print, echo, log, write to a file or repeat their values, also not encoded, and never put them in messages, tasks, ideas, memory or reports."
    )
}

/// The bridge's `vault_list` and `vault_read` (E18: one `tool_read` audit
/// row per item they show).
pub fn tool(shared: &Shared, run: aurelius_domain::RunId, op: aurelius_protocol::EmployeeTaskOp) -> (bool, String) {
    use aurelius_protocol::EmployeeTaskOp;
    let run_text = run.to_string();
    let keys = shared.vault.keys().ok().flatten();
    let outcome = shared.store().write(move |tx| {
        let Some(place) = run_place(tx, &run_text)? else { return Ok((false, "This run is not known.".to_owned())) };
        let run_project = place.project.clone();
        let (asked, item) = match &op {
            EmployeeTaskOp::VaultList { project } => (project.clone(), None),
            EmployeeTaskOp::VaultRead { item, project } => (project.clone(), Some(item.trim().to_owned())),
            _ => return Ok((false, "Not a vault tool.".to_owned())),
        };
        let project = match asked.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
            None => run_project.clone(),
            Some(p) => match tx.query_row("SELECT id FROM projects WHERE id = ?1 OR name = ?1 COLLATE NOCASE", [p], |r| r.get::<_, String>(0)).optional()? {
                Some(id) => Some(id),
                None => return Ok((false, format!("There is no project {p}."))),
            },
        };
        let own = project == run_project;
        let items = store::effective(tx, project.as_deref())?;
        let shown: Vec<&store::EffectiveItem> = match &item {
            None => items.iter().collect(),
            Some(wanted) => items.iter().filter(|e| e.item.name.eq_ignore_ascii_case(wanted) || e.item.env_name.as_deref().is_some_and(|n| n.eq_ignore_ascii_case(wanted.trim_start_matches('$')))).take(1).collect(),
        };
        if shown.is_empty() {
            return Ok((item.is_none(), match item {
                None => "The vault has no items here.".to_owned(),
                Some(wanted) => format!("The vault has no item {wanted} here (vault_list shows what there is)."),
            }));
        }
        for e in &shown {
            let id = aurelius_domain::EventId::new().to_string();
            store::insert_audit(
                tx,
                &NewAudit {
                    employee_id: place.employee.as_deref(),
                    run_id: Some(&run_text),
                    task_id: place.task.as_deref(),
                    ticket_id: place.ticket.as_deref(),
                    version: Some(e.item.version),
                    ..NewAudit::new(&id, Some(&e.item.id), AuditAction::ToolRead, aurelius_storage::now_ms())
                },
            )?;
        }
        let scope = |e: &store::EffectiveItem| if e.item.project_id.is_none() { "company" } else { "project" };
        let env = |e: &store::EffectiveItem| e.item.env_name.clone().unwrap_or_default();
        if item.is_none() {
            let lines: Vec<String> = shown
                .iter()
                .map(|e| {
                    let description = e.item.description.as_deref().map(|d| format!(" — {d}")).unwrap_or_default();
                    format!("- {} ({}, {}): ${}{description}", e.item.name, e.item.kind.as_str(), scope(e), env(e))
                })
                .collect();
            return Ok((true, lines.join("\n")));
        }
        let e = shown[0];
        if e.item.kind == StoredKind::Secret {
            return Ok((true, if own { format!("{} is a secret: available as ${} in your environment. Use it only inside commands; never print it.", e.item.name, env(e)) } else { format!("{} is a secret of another project: runs of that project have it as ${}.", e.item.name, env(e)) }));
        }
        let value = keys.as_ref().and_then(|k| Binding::of_row(&e.item).and_then(|b| k.open(&b, &e.item.sealed).ok()));
        Ok(match value {
            Some(value) => (true, value.to_string()),
            None => (false, format!("{} cannot be decrypted with this server's vault key.", e.item.name)),
        })
    });
    outcome.unwrap_or_else(|e| (false, format!("The vault could not be read: {e}")))
}
