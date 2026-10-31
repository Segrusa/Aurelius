//! The vault on a daemon (Sentinel SEC0): values sealed and never in a receipt,
//! request hash, event or table in plain (E1, E5); ciphertexts bound to their
//! item (E3); a key made only once and never when it cannot be read (E4); the
//! key export behind the password (E9); an import that never replaces a used
//! key (E10, E11); the checks of a new item (E7, E13, E18); B10 (E6).
#![cfg(unix)]

mod common;
use common::*;

use aurelius_client::Client;
use aurelius_domain::vault::{RevealPurpose, VaultKind};
use aurelius_domain::{CommandId, ErrorCode, VaultItemId};
use aurelius_protocol::{HostResult, HostWrite, VaultCreateParams, VaultUpdateParams, WriteMethod, WriteResult};
use sha2::Digest;
use std::path::Path;

const FAKE: &str = "sk-test-FAKE-0000-vault-canary";
const OTHER: &str = "sk-test-FAKE-1111-other-value";
/// Each test its own values: the redactor is the process's, shared by the daemons of this binary.
const T1_VALUE: &str = "sk-test-FAKE-4444-sealed-value";
const T1_OTHER: &str = "sk-test-FAKE-5555-sealed-other";
const E18_VALUE: &str = "sk-test-FAKE-2222-description";
const B10_VALUE: &str = "sk-test-FAKE-3333-pasted-b10";

fn create(kind: VaultKind, name: &str, env: &str, value: &str) -> VaultCreateParams {
    VaultCreateParams { project_id: None, kind, name: name.into(), env_name: env.into(), description: None, value: value.into() }
}

fn code(err: aurelius_client::ClientError) -> ErrorCode {
    match err {
        aurelius_client::ClientError::Protocol(e) => e.code,
        other => panic!("{other:?}"),
    }
}

async fn write(client: &Client, key: CommandId, write: HostWrite) -> Result<aurelius_protocol::Receipt, aurelius_client::ClientError> {
    client.write(key, None, WriteMethod::Host(write)).await
}

fn key_file(root: &Path) -> std::path::PathBuf {
    aurelius_host::secrets::file_in(&root.join("secrets"), "vault-master")
}

/// Every byte of the database files.
fn database_bytes(root: &Path) -> Vec<u8> {
    let mut all = Vec::new();
    for suffix in ["", "-wal", "-shm"] {
        if let Ok(bytes) = std::fs::read(root.join(format!("data/aurelius.sqlite{suffix}"))) {
            all.extend(bytes);
        }
    }
    all
}

fn holds(haystack: &[u8], needle: &str) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle.as_bytes())
}

/// SEC0 E1, E5: the value is sealed; the receipt keeps `{item_id, version}`;
/// the request hash is no plain hash of it; a replay with another value conflicts.
#[tokio::test(flavor = "multi_thread")]
async fn a_value_is_sealed_and_its_write_keeps_no_value_and_no_plain_hash() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let key = CommandId::new();
    let receipt = write(&client, key, HostWrite::VaultCreate(create(VaultKind::Secret, "GitHub token", "GITHUB_TOKEN", &format!("  {T1_VALUE}\n")))).await.unwrap();
    let WriteResult::Host(HostResult::VaultItem(item)) = receipt.result else { panic!() };
    assert!(item.value.is_none() && !item.undecryptable && item.version == 1);
    // Trimmed (E12): the value is what was meant, without spaces or the line end.
    assert_eq!(client.vault_reveal(item.id, RevealPurpose::Show).await.unwrap().value, T1_VALUE);

    let conn = db(dir.path());
    let (hash, result): (String, String) = conn.query_row("SELECT request_hash, result_json FROM command_receipts WHERE command_id = ?1", [key.to_string()], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    assert_eq!(serde_json::from_str::<serde_json::Value>(&result).unwrap(), serde_json::json!({ "item_id": item.id.to_string(), "version": 1 }));
    for plain in [T1_VALUE.to_owned(), format!("  {T1_VALUE}\n"), serde_json::to_string(&("vault.create", None::<u64>, T1_VALUE)).unwrap()] {
        assert_ne!(hash, hex::encode(sha2::Sha256::digest(plain.as_bytes())));
    }
    drop(conn);
    assert!(!holds(&database_bytes(dir.path()), T1_VALUE), "no plaintext in the database files");
    assert!(!holds(&database_bytes(dir.path()), "FAKE-4444"));

    // Same key, same value: the receipt, without the value. Same key, another value: a conflict.
    let replay = write(&client, key, HostWrite::VaultCreate(create(VaultKind::Secret, "GitHub token", "GITHUB_TOKEN", T1_VALUE))).await.unwrap();
    assert!(replay.replayed);
    let other = write(&client, key, HostWrite::VaultCreate(create(VaultKind::Secret, "GitHub token", "GITHUB_TOKEN", T1_OTHER))).await.unwrap_err();
    assert_eq!(code(other), ErrorCode::IdempotencyConflict);

    // A new value: version 2; the old one stays sealed (E16) and opens no more as the current one.
    let update = VaultUpdateParams { item_id: item.id, name: None, env_name: None, description: None, value: Some(T1_OTHER.into()), kind: None };
    let updated = client.vault_update(update).await.unwrap();
    assert_eq!(updated.version, 2);
    assert_eq!(client.vault_reveal(item.id, RevealPurpose::Copy).await.unwrap().value, T1_OTHER);
    let previous: i64 = db(dir.path()).query_row("SELECT COUNT(*) FROM vault_item_versions WHERE item_id = ?1", [item.id.to_string()], |r| r.get(0)).unwrap();
    assert_eq!(previous, 1);
    let audit = client.vault_audit(None, false, Some(item.id), None, None).await.unwrap();
    let actions: Vec<_> = audit.rows.iter().map(|r| r.action).collect();
    use aurelius_domain::vault::VaultAuditAction::*;
    assert_eq!(actions, vec![Copy, Replace, Reveal, Create]);
    // The device of a reveal and a copy (SEC2 m2): the client's name.
    for row in audit.rows.iter().filter(|r| matches!(r.action, Copy | Reveal)) {
        assert_eq!(row.via.as_deref(), Some("test"), "{:?}", row.action);
    }
    assert!(audit.rows[0].id > audit.rows[1].id, "newest first, by row id");
    // "Load older" pages on the audit rows' seq.
    let seqs: Vec<i64> = {
        let conn = db(dir.path());
        let mut stmt = conn.prepare("SELECT seq FROM vault_audit WHERE item_id = ?1 ORDER BY seq DESC").unwrap();
        stmt.query_map([item.id.to_string()], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
    };
    assert_eq!(audit.rows.iter().map(|r| r.id).collect::<Vec<_>>(), seqs);
    let first = client.vault_audit(None, false, Some(item.id), None, Some(2)).await.unwrap();
    assert!(first.more);
    let older = client.vault_audit(None, false, Some(item.id), Some(first.rows[1].id), Some(2)).await.unwrap();
    assert!(!older.more);
    assert_eq!(first.rows.iter().chain(&older.rows).map(|r| r.action).collect::<Vec<_>>(), vec![Copy, Replace, Reveal, Create]);
    drop(client);
    stop(dir.path(), daemon).await;

    // E16: after a restart the redactor is built from the database: the
    // previous value is redacted as well as the current one.
    aurelius_runtime::redact::install_set(&dir.path().display().to_string(), Vec::new());
    assert!(aurelius_runtime::redact::current().names_in(T1_VALUE).is_empty());
    let daemon = start(dir.path()).await;
    for value in [T1_VALUE, T1_OTHER] {
        assert_eq!(aurelius_runtime::redact::current().names_in(&format!("x {value} y")), vec!["GitHub token".to_owned()], "{value}");
    }
    stop(dir.path(), daemon).await;
}

/// SEC0 E5: the request hash is keyed with the server's own vault key: the
/// same request on two servers hashes differently (a plain hash of the value
/// would be the same everywhere, and could be checked against a guess).
#[tokio::test(flavor = "multi_thread")]
async fn the_request_hash_of_a_vault_write_is_keyed_per_server() {
    let key = CommandId::new();
    let mut hashes = Vec::new();
    for _ in 0..2 {
        let dir = tempfile::tempdir().unwrap();
        let daemon = start(dir.path()).await;
        let client = connect(dir.path()).await;
        write(&client, key, HostWrite::VaultCreate(create(VaultKind::Text, "Same", "SAME_NOTE", "the same text value"))).await.unwrap();
        hashes.push(db(dir.path()).query_row("SELECT request_hash FROM command_receipts WHERE command_id = ?1", [key.to_string()], |r| r.get::<_, String>(0)).unwrap());
        drop(client);
        stop(dir.path(), daemon).await;
    }
    assert_ne!(hashes[0], hashes[1]);
}

/// SEC0 E3 (and the canary's step 6): the ciphertext or nonce of two items swapped in the database: neither opens.
#[tokio::test(flavor = "multi_thread")]
async fn swapped_ciphertexts_do_not_open() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let a = client.vault_create(create(VaultKind::Secret, "A", "A_TOKEN", FAKE)).await.unwrap();
    let b = client.vault_create(create(VaultKind::Secret, "B", "B_TOKEN", OTHER)).await.unwrap();
    let conn = rusqlite::Connection::open(dir.path().join("data/aurelius.sqlite")).unwrap();
    conn.busy_timeout(std::time::Duration::from_secs(5)).unwrap();
    let sealed = |id: VaultItemId| -> (Vec<u8>, Vec<u8>) { conn.query_row("SELECT ciphertext, nonce FROM vault_items WHERE id = ?1", [id.to_string()], |r| Ok((r.get(0)?, r.get(1)?))).unwrap() };
    let ((ca, na), (cb, nb)) = (sealed(a.id), sealed(b.id));
    conn.execute("UPDATE vault_items SET ciphertext = ?2, nonce = ?3 WHERE id = ?1", rusqlite::params![a.id.to_string(), cb, nb]).unwrap();
    conn.execute("UPDATE vault_items SET ciphertext = ?2, nonce = ?3 WHERE id = ?1", rusqlite::params![b.id.to_string(), ca, na]).unwrap();
    for id in [a.id, b.id] {
        assert_eq!(code(client.vault_reveal(id, RevealPurpose::Show).await.unwrap_err()), ErrorCode::VaultUndecryptable);
    }
    // Only the nonce swapped back: still nothing.
    conn.execute("UPDATE vault_items SET ciphertext = ?2 WHERE id = ?1", rusqlite::params![a.id.to_string(), ca]).unwrap();
    assert_eq!(code(client.vault_reveal(a.id, RevealPurpose::Show).await.unwrap_err()), ErrorCode::VaultUndecryptable);
    assert!(client.vault_list(None).await.unwrap().items.iter().all(|i| i.undecryptable));
    stop(dir.path(), daemon).await;
}

/// SEC0 E4: without its key the vault is unavailable and no new key is made;
/// a key others may read is refused.
#[tokio::test(flavor = "multi_thread")]
async fn a_missing_or_open_key_never_makes_a_new_one() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    assert!(!key_file(dir.path()).exists(), "no key before the first item");
    let item = client.vault_create(create(VaultKind::Text, "Notes", "TEAM_NOTES", "a text value")).await.unwrap();
    let line = std::fs::read_to_string(key_file(dir.path())).unwrap();
    assert_eq!(std::fs::metadata(key_file(dir.path())).unwrap().permissions().mode() & 0o777, 0o600);
    assert_eq!(std::fs::metadata(dir.path().join("secrets")).unwrap().permissions().mode() & 0o777, 0o700);
    drop(client);
    stop(dir.path(), daemon).await;

    std::fs::remove_file(key_file(dir.path())).unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let listed = client.vault_list(None).await.unwrap();
    assert!(listed.items[0].undecryptable && listed.items[0].value.is_none(), "{listed:?}");
    assert_eq!(code(client.vault_create(create(VaultKind::Text, "More", "MORE_NOTES", "x-value")).await.unwrap_err()), ErrorCode::VaultUndecryptable);
    // QA1 m2: the export says the key is missing (import it), not that there is none yet.
    let aurelius_client::ClientError::Protocol(export) = client.vault_key_export(BOARD_PASSWORD.into()).await.unwrap_err() else { panic!() };
    assert_eq!(export.code, ErrorCode::VaultUndecryptable, "{}", export.message);
    assert!(export.message.contains("vault-key import"), "{}", export.message);
    assert!(!key_file(dir.path()).exists(), "never a new key while values are stored");
    drop(client);
    stop(dir.path(), daemon).await;

    std::fs::write(key_file(dir.path()), &line).unwrap();
    std::fs::set_permissions(key_file(dir.path()), std::fs::Permissions::from_mode(0o644)).unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    assert!(client.vault_list(None).await.unwrap().items[0].undecryptable, "a key others may read is not used");
    assert_eq!(std::fs::read_to_string(key_file(dir.path())).unwrap(), line, "and never replaced");
    std::fs::set_permissions(key_file(dir.path()), std::fs::Permissions::from_mode(0o600)).unwrap();
    let listed = client.vault_list(None).await.unwrap();
    assert_eq!(listed.items[0].value.as_deref(), Some("a text value"), "made private, it is read again");
    assert_eq!(listed.items[0].id, item.id);
    stop(dir.path(), daemon).await;
}

/// SEC0 E9: the export needs the password again (counted like a sign-in), is
/// one line with the key's id, is audited and told to every board member.
#[tokio::test(flavor = "multi_thread")]
async fn the_key_export_asks_the_password_counts_it_and_tells_every_board_member() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    assert_eq!(code(client.vault_key_export(BOARD_PASSWORD.into()).await.unwrap_err()), ErrorCode::NotFound, "no key before the first item");
    client.vault_create(create(VaultKind::Secret, "Token", "SOME_TOKEN", FAKE)).await.unwrap();
    assert_eq!(code(client.vault_key_export("not-the-password".into()).await.unwrap_err()), ErrorCode::WrongPassword);
    let export = client.vault_key_export(BOARD_PASSWORD.into()).await.unwrap();
    let parts: Vec<&str> = export.key.split(':').collect();
    assert_eq!(parts[..3], ["aurelius-vault-key", "v1", "dev"]);
    assert_eq!(parts[3], export.key_id);
    assert_eq!(std::fs::read_to_string(key_file(dir.path())).unwrap().trim(), export.key);
    assert!(!format!("{export:?}").contains(parts[4]));
    let audit = client.vault_audit(None, true, None, None, None).await.unwrap();
    assert!(audit.rows.iter().any(|r| r.action == aurelius_domain::vault::VaultAuditAction::KeyExport && r.item_id.is_none()));
    let general = client.group_chat().await.unwrap();
    assert!(general.messages.iter().any(|m| m.text.contains("exported the vault key")), "every board member sees it");
    // Counted: the connection's tries run out.
    let mut refused = None;
    for _ in 0..6 {
        if let Err(e) = client.vault_key_export("still-not-it".into()).await {
            let c = code(e);
            if c == ErrorCode::TooManyAttempts {
                refused = Some(c);
                break;
            }
        }
    }
    assert_eq!(refused, Some(ErrorCode::TooManyAttempts));
    stop(dir.path(), daemon).await;
}

/// SEC0 E7, E13, E18: names a run may not have, short secrets and a known
/// secret in a name or description are refused.
#[tokio::test(flavor = "multi_thread")]
async fn reserved_names_short_secrets_and_secrets_in_descriptions_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    for env in ["ANTHROPIC_API_KEY", "NODE_OPTIONS", "HTTPS_PROXY", "PATH", "Path", "OTEL_EXPORTER_OTLP_ENDPOINT"] {
        assert_eq!(code(client.vault_create(create(VaultKind::Secret, env, env, E18_VALUE)).await.unwrap_err()), ErrorCode::Validation, "{env}");
    }
    assert_eq!(code(client.vault_create(create(VaultKind::Secret, "Short", "SHORT_TOKEN", "  1234567  ")).await.unwrap_err()), ErrorCode::Validation, "7 characters once trimmed");
    client.vault_create(create(VaultKind::Secret, "Canary", "CANARY_TOKEN", E18_VALUE)).await.unwrap();
    let pasted = VaultCreateParams { description: Some(format!("the key is {E18_VALUE}")), ..create(VaultKind::Text, "Pasted", "PASTED_NOTE", "fine text") };
    let err = client.vault_create(pasted).await.unwrap_err();
    let aurelius_client::ClientError::Protocol(err) = err else { panic!() };
    assert_eq!((err.code, err.matched_items.clone()), (ErrorCode::LooksLikeSecret, vec!["Canary".to_owned()]), "the item's name");
    assert!(!err.message.contains("FAKE-2222"));
    stop(dir.path(), daemon).await;
}

/// SEC0 E6 (B10): a board member's text holding a secret is refused with its
/// name only and leaves nothing; confirmed, it is stored redacted.
#[tokio::test(flavor = "multi_thread")]
async fn a_pasted_secret_is_named_and_with_confirmation_stored_redacted() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    client.vault_create(create(VaultKind::Secret, "Canary", "CANARY_TOKEN", B10_VALUE)).await.unwrap();
    let send = |confirm| aurelius_domain::group_chat::SendGroupMessage { text: format!("here: {B10_VALUE}"), images: vec![], files: vec![], channel: None, confirm_secret: confirm };
    let err = client.send_group_message(send(false)).await.unwrap_err();
    let aurelius_client::ClientError::Protocol(err) = err else { panic!() };
    assert_eq!((err.code, err.matched_items.clone()), (ErrorCode::LooksLikeSecret, vec!["Canary".to_owned()]), "the item's name");
    // A text file attached with it is refused too.
    let file = dir.path().join("notes.txt");
    std::fs::write(&file, format!("token={B10_VALUE}\n")).unwrap();
    let with_file = aurelius_domain::group_chat::SendGroupMessage { text: "see file".into(), files: vec![file.display().to_string()], ..send(true) };
    assert_eq!(code(client.send_group_message(with_file).await.unwrap_err()), ErrorCode::LooksLikeSecret);
    client.send_group_message(send(true)).await.unwrap();
    let general = client.group_chat().await.unwrap();
    assert!(general.messages.iter().any(|m| m.text == "here: «secret:CANARY_TOKEN»"), "{:?}", general.messages.iter().map(|m| &m.text).collect::<Vec<_>>());
    stop(dir.path(), daemon).await;
    let bytes = database_bytes(dir.path());
    assert!(!holds(&bytes, "FAKE-3333"), "nothing of the value in the database, its receipts or events");
    let blobs = walk(&dir.path().join("blobs"));
    assert!(blobs.iter().all(|b| !holds(b, "FAKE-3333")), "nor in a blob");
}

fn walk(dir: &Path) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk(&path));
        } else if let Ok(bytes) = std::fs::read(&path) {
            out.push(bytes);
        }
    }
    out
}

/// SEC0 E10, E11: an import takes the key from standard input, never over a
/// key that encrypts values, never while the daemon runs, and only a key
/// that opens the stored values. A restore on a machine with an empty vault works.
#[tokio::test(flavor = "multi_thread")]
async fn an_import_never_replaces_a_used_key_and_must_fit() {
    use aurelius_daemon::vault::import::{Source, run};
    let first = tempfile::tempdir().unwrap();
    let daemon = start(first.path()).await;
    let client = connect(first.path()).await;
    let item = client.vault_create(create(VaultKind::Secret, "Token", "SOME_TOKEN", FAKE)).await.unwrap();
    let line = client.vault_key_export(BOARD_PASSWORD.into()).await.unwrap().key;
    let busy = run(first.path(), false, &Source::Stdin, &mut line.as_bytes()).unwrap_err();
    assert!(busy.contains("running"), "{busy}");
    drop(client);
    stop(first.path(), daemon).await;

    // Another server's key does not fit these values; over the used key nothing is replaced.
    let stranger = aurelius_daemon::vault::key::export_line(&aurelius_daemon::vault::crypto::MasterKey::generate().unwrap());
    assert!(run(first.path(), false, &Source::Stdin, &mut stranger.as_bytes()).unwrap_err().contains("does not open"));
    assert!(run(first.path(), false, &Source::Stdin, &mut b"aurelius-vault-key:v1:dev:00:AAAA".as_slice()).is_err());
    assert!(run(first.path(), false, &Source::Stdin, &mut line.as_bytes()).unwrap().contains("already in place"));

    // A restore on a machine whose vault is empty but already has a key of its own.
    let second = tempfile::tempdir().unwrap();
    let daemon = start(second.path()).await;
    let client = connect(second.path()).await;
    let throwaway = client.vault_create(create(VaultKind::Text, "Tmp", "TMP_NOTE", "temp")).await.unwrap();
    drop(client);
    stop(second.path(), daemon).await;
    // Its key encrypts a value: refused.
    assert!(run(second.path(), false, &Source::Stdin, &mut line.as_bytes()).unwrap_err().contains("does not open"));
    // Its database replaced by the first one's (the restore): its own key now encrypts nothing.
    // One consistent copy (VACUUM INTO), not the files one by one: under load a
    // late checkpoint between copying the database and its WAL lost the item.
    for suffix in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(second.path().join(format!("data/aurelius.sqlite{suffix}")));
    }
    let target = second.path().join("data/aurelius.sqlite");
    db(first.path()).execute("VACUUM INTO ?1", [target.to_str().unwrap()]).unwrap();
    let _ = throwaway;
    let key_path = second.path().join("first.key");
    std::fs::write(&key_path, &line).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(run(second.path(), false, &Source::File(key_path.clone()), &mut std::io::empty()).is_err(), "a key file others may read");
    std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert!(run(second.path(), false, &Source::File(key_path), &mut std::io::empty()).unwrap().contains("replaced an unused one"));
    let daemon = start(second.path()).await;
    let client = connect(second.path()).await;
    assert_eq!(client.vault_reveal(item.id, RevealPurpose::Show).await.unwrap().value, FAKE, "the restored vault opens");
    stop(second.path(), daemon).await;
}

/// A runtime whose every run answers with what the test put in it.
#[derive(Clone, Default)]
struct Printer(std::sync::Arc<std::sync::Mutex<String>>);

#[async_trait::async_trait]
impl aurelius_runtime::AgentRuntime for Printer {
    async fn probe(&self) -> aurelius_runtime::RuntimeCapabilities {
        aurelius_runtime::fake::FakeRuntime::default().probe().await
    }

    async fn start(&self, request: aurelius_runtime::RunRequest) -> Result<aurelius_runtime::RunHandle, aurelius_runtime::RuntimeError> {
        use aurelius_runtime::RuntimeEvent;
        let text = self.0.lock().unwrap().clone();
        let (tx, rx) = tokio::sync::mpsc::channel(8);
        let (input_tx, _input_rx) = tokio::sync::mpsc::channel(1);
        let (cancel_tx, _cancel_rx) = tokio::sync::watch::channel(false);
        tokio::spawn(async move {
            let _ = tx.send(RuntimeEvent::Started).await;
            let _ = tx.send(RuntimeEvent::TextDelta(text)).await;
            let _ = tx.send(RuntimeEvent::Completed).await;
        });
        Ok(aurelius_runtime::RunHandle::new(request.run_id, rx, input_tx, cancel_tx))
    }
}

/// The answer to `message` once it is complete.
async fn answer(client: &Client, conversation: aurelius_domain::ConversationId, message: aurelius_domain::MessageId) -> String {
    for _ in 0..400 {
        let messages = client.messages(conversation).await.unwrap().messages;
        if let Some(m) = messages.iter().find(|m| m.reply_to == Some(message) && m.status == aurelius_domain::conversation::MessageStatus::Complete) {
            return m.text.clone();
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    panic!("no answer")
}

const LATE_OLD: &str = "sk-test-FAKE-6666-late-key-old";
const LATE_NEW: &str = "sk-test-FAKE-7777-late-key-new";

/// SEC1 M1: a key that becomes readable after the start (here 0644 made
/// 0600) fills the redactor with every stored value before it is used: the
/// value before an update is redacted in a run's events and found by B10, as
/// well as the new one.
#[tokio::test(flavor = "multi_thread")]
async fn a_key_readable_only_after_the_start_fills_the_redactor() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let printer = Printer::default();
    let daemon = start_with(dir.path(), std::sync::Arc::new(printer.clone())).await;
    let client = connect(dir.path()).await;
    let item = client.vault_create(create(VaultKind::Secret, "Late", "LATE_TOKEN", LATE_OLD)).await.unwrap();
    drop(client);
    stop(dir.path(), daemon).await;

    std::fs::set_permissions(key_file(dir.path()), std::fs::Permissions::from_mode(0o644)).unwrap();
    // A new process: the redactor knows nothing of this daemon yet.
    aurelius_runtime::redact::install_set(&dir.path().display().to_string(), Vec::new());
    let daemon = start_with(dir.path(), std::sync::Arc::new(printer.clone())).await;
    assert!(aurelius_runtime::redact::current().names_in(LATE_OLD).is_empty(), "the start had no key to read the values with");
    std::fs::set_permissions(key_file(dir.path()), std::fs::Permissions::from_mode(0o600)).unwrap();
    let client = connect(dir.path()).await;
    let update = VaultUpdateParams { item_id: item.id, name: None, env_name: None, description: None, value: Some(LATE_NEW.into()), kind: None };
    assert_eq!(client.vault_update(update).await.unwrap().version, 2);

    *printer.0.lock().unwrap() = format!("old {LATE_OLD} new {LATE_NEW}");
    let (conversation, message, _) = send_new(&client, "print them").await;
    assert_eq!(answer(&client, conversation, message).await, "old «secret:LATE_TOKEN» new «secret:LATE_TOKEN»");
    for value in [LATE_OLD, LATE_NEW] {
        let paste = aurelius_domain::group_chat::SendGroupMessage { text: format!("here: {value}"), images: vec![], files: vec![], channel: None, confirm_secret: false };
        let aurelius_client::ClientError::Protocol(err) = client.send_group_message(paste).await.unwrap_err() else { panic!() };
        assert_eq!((err.code, err.matched_items), (ErrorCode::LooksLikeSecret, vec!["Late".to_owned()]), "{value}");
    }
    stop(dir.path(), daemon).await;
}

const REFUSED_STORED: &str = "sk-test-FAKE-8888-refused-stored";
const REFUSED_NEW: &str = "sk-test-FAKE-8889-refused-create";
const REFUSED_UPDATE: &str = "sk-test-FAKE-8890-refused-update";

/// R1 m5: a value goes into the redactor before its write commits; a write
/// that is refused takes it out again (what is stored stays in).
#[tokio::test(flavor = "multi_thread")]
async fn a_refused_write_leaves_its_value_out_of_the_redactor() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let item = client.vault_create(create(VaultKind::Secret, "Kept", "KEPT_TOKEN", REFUSED_STORED)).await.unwrap();
    client.vault_create(create(VaultKind::Text, "Other", "OTHER_NOTE", "some other text")).await.unwrap();
    let names = |value: &str| aurelius_runtime::redact::current().names_in(&format!("x {value} y"));

    let taken = create(VaultKind::Secret, "Kept", "NEW_TOKEN", REFUSED_NEW);
    assert_eq!(code(client.vault_create(taken).await.unwrap_err()), ErrorCode::Validation);
    assert!(names(REFUSED_NEW).is_empty(), "a refused create");

    let update = VaultUpdateParams { item_id: item.id, name: None, env_name: Some("OTHER_NOTE".into()), description: None, value: Some(REFUSED_UPDATE.into()), kind: None };
    assert_eq!(code(client.vault_update(update).await.unwrap_err()), ErrorCode::Validation);
    assert!(names(REFUSED_UPDATE).is_empty(), "a refused update");
    assert_eq!(names(REFUSED_STORED), vec!["Kept".to_owned()], "what is stored stays");
    stop(dir.path(), daemon).await;
}

const B7_SECRET: &str = "sk-test-FAKE-9999-kind-locked";

/// B7 through the API (QA1 m3, R1 m3): text and url trade places without a
/// new value (the value sealed again for the new kind, an `edit` of `kind`,
/// a url checked as one); a secret never changes kind, nor does anything become one.
#[tokio::test(flavor = "multi_thread")]
async fn text_and_url_trade_places_without_a_new_value_and_a_secret_stays_one() {
    use aurelius_domain::vault::VaultAuditAction;
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let to = |item_id, kind| VaultUpdateParams { item_id, name: None, env_name: None, description: None, value: None, kind: Some(kind) };

    let wiki = client.vault_create(create(VaultKind::Text, "Wiki", "WIKI_URL", "https://wiki.example.test")).await.unwrap();
    let url = client.vault_update(to(wiki.id, VaultKind::Url)).await.unwrap();
    assert_eq!((url.kind, url.version, url.value.as_deref()), (VaultKind::Url, 2, Some("https://wiki.example.test")));
    assert_eq!(client.vault_reveal(wiki.id, RevealPurpose::Show).await.unwrap().value, "https://wiki.example.test", "it opens as the new kind");
    let back = client.vault_update(to(wiki.id, VaultKind::Text)).await.unwrap();
    assert_eq!((back.kind, back.version), (VaultKind::Text, 3));
    let audit = client.vault_audit(None, false, Some(wiki.id), None, None).await.unwrap();
    let edits: Vec<_> = audit.rows.iter().filter(|r| r.action != VaultAuditAction::Reveal).map(|r| (r.action, r.changed_fields.clone())).collect();
    assert_eq!(edits, vec![(VaultAuditAction::Edit, vec!["kind".to_owned()]), (VaultAuditAction::Edit, vec!["kind".to_owned()]), (VaultAuditAction::Create, vec![])]);

    // A text that is no url stays a text.
    let note = client.vault_create(create(VaultKind::Text, "Note", "TEAM_NOTE", "not a url at all")).await.unwrap();
    assert_eq!(code(client.vault_update(to(note.id, VaultKind::Url)).await.unwrap_err()), ErrorCode::Validation);
    // With a new value it is still a replace.
    let with_value = VaultUpdateParams { value: Some("https://note.example.test".into()), ..to(note.id, VaultKind::Url) };
    assert_eq!(client.vault_update(with_value).await.unwrap().version, 2);
    let last = client.vault_audit(None, false, Some(note.id), None, Some(1)).await.unwrap();
    assert_eq!(last.rows[0].action, VaultAuditAction::Replace);

    let secret = client.vault_create(create(VaultKind::Secret, "Locked", "LOCKED_TOKEN", B7_SECRET)).await.unwrap();
    for (item, kind) in [(secret.id, VaultKind::Text), (secret.id, VaultKind::Url), (note.id, VaultKind::Secret)] {
        assert_eq!(code(client.vault_update(to(item, kind)).await.unwrap_err()), ErrorCode::VaultKindLocked, "{kind:?}");
        let with_value = VaultUpdateParams { value: Some("https://x.example.test/abcdefgh".into()), ..to(item, kind) };
        assert_eq!(code(client.vault_update(with_value).await.unwrap_err()), ErrorCode::VaultKindLocked, "{kind:?} with a value");
    }
    stop(dir.path(), daemon).await;
}

const NOTE_VALUE: &str = "sk-test-FAKE-1212-in-a-note";

/// R1 m7: a project note (runs read it as context) with a vault value is
/// refused with the item's name, before anything is stored.
#[tokio::test(flavor = "multi_thread")]
async fn a_project_note_with_a_vault_value_is_refused() {
    use aurelius_domain::{Command, CommandResult, project::CreateProject};
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    client.vault_create(create(VaultKind::Secret, "Noted", "NOTED_TOKEN", NOTE_VALUE)).await.unwrap();
    let created = client.execute(aurelius_client::envelope(Command::CreateProject(CreateProject { name: "Notes".into(), description: None, folder: None }), None)).await.unwrap();
    let CommandResult::ProjectCreated { project_id, .. } = created.result else { panic!() };

    let aurelius_client::ClientError::Protocol(err) = client.create_note(project_id, format!("deploy with {NOTE_VALUE}")).await.unwrap_err() else { panic!() };
    assert_eq!((err.code, err.matched_items), (ErrorCode::LooksLikeSecret, vec!["Noted".to_owned()]));
    let note = client.create_note(project_id, "deploy with $NOTED_TOKEN".into()).await.unwrap();
    let edit = client.update_note(note.summary.id, format!("deploy with {NOTE_VALUE}"), note.summary.revision).await.unwrap_err();
    assert_eq!(code(edit), ErrorCode::LooksLikeSecret);
    assert_eq!(client.note(note.summary.id).await.unwrap().body, "deploy with $NOTED_TOKEN");
    stop(dir.path(), daemon).await;
    assert!(!holds(&database_bytes(dir.path()), "FAKE-1212"), "nothing of it stored");
}

const EXPORT_VALUE: &str = "sk-test-FAKE-1313-export-told";

/// SEC1 m4: the key goes out only with its line in the general Team chat;
/// without that chat, nothing is exported and nothing is audited as exported.
#[tokio::test(flavor = "multi_thread")]
async fn no_key_export_without_its_line_in_the_team_chat() {
    use aurelius_domain::{Command, CommandResult, project::CreateProject};
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    client.vault_create(create(VaultKind::Secret, "Told", "TOLD_TOKEN", EXPORT_VALUE)).await.unwrap();
    let created = client.execute(aurelius_client::envelope(Command::CreateProject(CreateProject { name: "Elsewhere".into(), description: None, folder: None }), None)).await.unwrap();
    let CommandResult::ProjectCreated { project_id, .. } = created.result else { panic!() };
    // The general chat made a project's conversation: there is no general one.
    let conn = rusqlite::Connection::open(dir.path().join("data/aurelius.sqlite")).unwrap();
    conn.busy_timeout(std::time::Duration::from_secs(5)).unwrap();
    let general: String = conn.query_row("SELECT id FROM conversations WHERE kind = 'group' AND project_id IS NULL", [], |r| r.get(0)).unwrap();
    conn.execute("UPDATE conversations SET kind = 'project', project_id = ?2 WHERE id = ?1", rusqlite::params![general, project_id.to_string()]).unwrap();

    assert_eq!(code(client.vault_key_export(BOARD_PASSWORD.into()).await.unwrap_err()), ErrorCode::Validation);
    let exports: i64 = conn.query_row("SELECT COUNT(*) FROM vault_audit WHERE action = 'key_export'", [], |r| r.get(0)).unwrap();
    assert_eq!(exports, 0, "nothing exported, nothing audited");

    conn.execute("UPDATE conversations SET kind = 'group', project_id = NULL WHERE id = ?1", [general]).unwrap();
    client.vault_key_export(BOARD_PASSWORD.into()).await.unwrap();
    let exports: i64 = conn.query_row("SELECT COUNT(*) FROM vault_audit WHERE action = 'key_export'", [], |r| r.get(0)).unwrap();
    assert_eq!(exports, 1);
    drop(conn);
    stop(dir.path(), daemon).await;
}

const CURSOR_VALUE: &str = "sk-test-FAKE-1414-audit-cursor";

/// R1 m3: the audit's cursor through the API: pages of 2, `more`, newest
/// first, no row twice and none missing; the scope filter; a deleted item
/// is not revealed.
#[tokio::test(flavor = "multi_thread")]
async fn the_audit_pages_without_overlap_and_a_deleted_item_is_not_revealed() {
    use aurelius_domain::{Command, CommandResult, project::CreateProject};
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let a = client.vault_create(create(VaultKind::Secret, "Paged", "PAGED_TOKEN", CURSOR_VALUE)).await.unwrap();
    let b = client.vault_create(create(VaultKind::Text, "Other", "OTHER_TEXT", "another text")).await.unwrap();
    let created = client.execute(aurelius_client::envelope(Command::CreateProject(CreateProject { name: "Scoped".into(), description: None, folder: None }), None)).await.unwrap();
    let CommandResult::ProjectCreated { project_id, .. } = created.result else { panic!() };
    let p = client.vault_create(VaultCreateParams { project_id: Some(project_id), ..create(VaultKind::Text, "Own", "OWN_TEXT", "a project text") }).await.unwrap();
    for item in [a.id, b.id, a.id, p.id, a.id] {
        client.vault_reveal(item, RevealPurpose::Show).await.unwrap();
    }
    let all_company: Vec<i64> = {
        let conn = db(dir.path());
        let mut stmt = conn.prepare("SELECT seq FROM vault_audit WHERE project_id IS NULL ORDER BY seq DESC").unwrap();
        stmt.query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
    };
    assert_eq!(all_company.len(), 6, "2 creates and 4 reveals of company items");
    let mut seen = Vec::new();
    let mut before = None;
    loop {
        let page = client.vault_audit(None, false, None, before, Some(2)).await.unwrap();
        assert!(page.rows.len() <= 2);
        seen.extend(page.rows.iter().map(|r| r.id));
        assert!(page.rows.iter().all(|r| r.project_id.is_none()), "the company's only");
        if !page.more {
            break;
        }
        before = page.rows.last().map(|r| r.id);
    }
    assert_eq!(seen, all_company, "newest first, every row once");
    let item_page = client.vault_audit(None, false, Some(a.id), None, Some(2)).await.unwrap();
    assert!(item_page.more && item_page.rows.iter().all(|r| r.item_id == Some(a.id)));
    let project_rows = client.vault_audit(Some(project_id), false, None, None, None).await.unwrap();
    assert_eq!(project_rows.rows.len(), 2, "its create and its reveal");
    assert!(project_rows.rows.iter().all(|r| r.project_id == Some(project_id)));

    // The actor's name comes with the page (R1 m1).
    let page = client.vault_audit(None, false, None, None, Some(1)).await.unwrap();
    assert!(matches!(&page.rows[0].actor, aurelius_domain::vault::VaultActor::BoardMember { user } if user.username == BOARD_MEMBER), "{:?}", page.rows[0].actor);

    // SEC1 m3: each row keeps the version of the value it concerns.
    let update = VaultUpdateParams { item_id: b.id, name: None, env_name: None, description: None, value: Some("a newer text".into()), kind: None };
    client.vault_update(update).await.unwrap();
    client.vault_reveal(b.id, RevealPurpose::Copy).await.unwrap();
    client.vault_archive(b.id).await.unwrap();
    let versions: Vec<(String, Option<i64>)> = {
        let conn = db(dir.path());
        let mut stmt = conn.prepare("SELECT action, version FROM vault_audit WHERE item_id = ?1 ORDER BY seq").unwrap();
        stmt.query_map([b.id.to_string()], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(Result::unwrap).collect()
    };
    let expected: Vec<(String, Option<i64>)> = [("create", 1), ("reveal", 1), ("replace", 2), ("copy", 2), ("archive", 2)].iter().map(|(a, v)| (a.to_string(), Some(*v))).collect();
    assert_eq!(versions, expected);
    assert_eq!(code(client.vault_reveal(b.id, RevealPurpose::Show).await.unwrap_err()), ErrorCode::NotFound);
    assert_eq!(code(client.vault_reveal(b.id, RevealPurpose::Copy).await.unwrap_err()), ErrorCode::NotFound);
    stop(dir.path(), daemon).await;
}

/// QA2-M1: a project item's history with only its `item_id`, as both
/// clients ask it, gives its rows, also once archived; another project's
/// `project_id` or an unknown item gives an empty page; `all` keeps working.
#[tokio::test(flavor = "multi_thread")]
async fn a_project_items_history_needs_only_its_item_id_also_once_archived() {
    use aurelius_domain::vault::VaultAuditAction::*;
    use aurelius_domain::{Command, CommandResult, project::CreateProject};
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    let mut projects = Vec::new();
    for name in ["Scoped", "Elsewhere"] {
        let created = client.execute(aurelius_client::envelope(Command::CreateProject(CreateProject { name: name.into(), description: None, folder: None }), None)).await.unwrap();
        let CommandResult::ProjectCreated { project_id, .. } = created.result else { panic!() };
        projects.push(project_id);
    }
    let p = client.vault_create(VaultCreateParams { project_id: Some(projects[0]), ..create(VaultKind::Secret, "Deploy", "DEPLOY_TOKEN", CURSOR_VALUE) }).await.unwrap();
    client.vault_reveal(p.id, RevealPurpose::Show).await.unwrap();
    client.vault_reveal(p.id, RevealPurpose::Copy).await.unwrap();
    let actions = |page: &aurelius_domain::vault::VaultAuditPage| page.rows.iter().map(|r| (r.item_id, r.project_id, r.action)).collect::<Vec<_>>();
    let page = client.vault_audit(None, false, Some(p.id), None, None).await.unwrap();
    assert_eq!(actions(&page), [Copy, Reveal, Create].map(|a| (Some(p.id), Some(projects[0]), a)));
    assert!(!page.more);

    client.vault_archive(p.id).await.unwrap();
    let archived = client.vault_audit(None, false, Some(p.id), None, None).await.unwrap();
    assert_eq!(actions(&archived), [Archive, Copy, Reveal, Create].map(|a| (Some(p.id), Some(projects[0]), a)));
    let paged = client.vault_audit(None, false, Some(p.id), Some(archived.rows[1].id), Some(1)).await.unwrap();
    assert_eq!((paged.rows[0].action, paged.more), (Reveal, true), "the cursor in the item's scope");
    let own = client.vault_audit(Some(projects[0]), false, Some(p.id), None, None).await.unwrap();
    assert_eq!(own.rows.len(), 4, "its own project_id still fits");
    let all = client.vault_audit(None, true, Some(p.id), None, None).await.unwrap();
    assert_eq!(all.rows.len(), 4, "all as before");

    let other = client.vault_audit(Some(projects[1]), false, Some(p.id), None, None).await.unwrap();
    assert!(other.rows.is_empty() && !other.more, "another project's id: an empty page");
    let unknown = client.vault_audit(None, false, Some(aurelius_domain::VaultItemId::new()), None, None).await.unwrap();
    assert!(unknown.rows.is_empty() && !unknown.more);
    // A company item keeps the company's scope; a project's id does not fit it.
    let c = client.vault_create(create(VaultKind::Text, "Shared", "SHARED_TEXT", "a company text")).await.unwrap();
    assert_eq!(actions(&client.vault_audit(None, false, Some(c.id), None, None).await.unwrap()), [(Some(c.id), None, Create)]);
    assert!(client.vault_audit(Some(projects[0]), false, Some(c.id), None, None).await.unwrap().rows.is_empty());
    stop(dir.path(), daemon).await;
}

/// R1 m3: every vault method turns away whoever is not a signed-in board
/// member: no session, a temporary password, an app.
#[tokio::test(flavor = "multi_thread")]
async fn the_vault_methods_are_for_signed_in_board_members_only() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let alex = connect(dir.path()).await;
    let item = alex.vault_create(create(VaultKind::Text, "Note", "A_NOTE", "a text")).await.unwrap();
    async fn every(client: &Client, item: VaultItemId) -> Vec<ErrorCode> {
        let update = VaultUpdateParams { item_id: item, name: Some("Renamed".into()), env_name: None, description: None, value: None, kind: None };
        vec![
            code(client.vault_list(None).await.unwrap_err()),
            code(client.vault_reveal(item, RevealPurpose::Show).await.unwrap_err()),
            code(client.vault_audit(None, true, None, None, None).await.unwrap_err()),
            code(client.vault_key_export(BOARD_PASSWORD.into()).await.unwrap_err()),
            code(client.vault_create(create(VaultKind::Text, "New", "NEW_NOTE", "x text")).await.unwrap_err()),
            code(client.vault_update(update).await.unwrap_err()),
            code(client.vault_archive(item).await.unwrap_err()),
        ]
    }
    let anonymous = connect_anonymous(dir.path()).await;
    assert_eq!(every(&anonymous, item.id).await, vec![ErrorCode::Unauthenticated; 7]);

    let robin = connect_as(dir.path(), "robin").await;
    let robin_id = robin.user_whoami().await.unwrap().user.unwrap().id;
    alex.set_password(Some(robin_id), None, "a-temporary-password-for-robin").await.unwrap();
    let temporary = connect_anonymous(dir.path()).await;
    assert!(temporary.user_login("robin", "a-temporary-password-for-robin", "Laptop").await.unwrap().must_change_password);
    assert_eq!(every(&temporary, item.id).await, vec![ErrorCode::PasswordChangeRequired; 7]);

    let key = alex
        .create_app_key(aurelius_domain::app_key::CreateAppKey { name: "assistant".into(), scopes: aurelius_domain::app_key::AppScope::ALL.to_vec(), limit_per_hour: None, limit_concurrent: None, audience: Default::default(), webhook_url: None })
        .await
        .unwrap();
    let app = Client::connect_app(&aurelius_host::endpoint(dir.path()), &key.secret, "test app").await.unwrap();
    assert_eq!(every(&app, item.id).await, vec![ErrorCode::Forbidden; 7]);
    // Nothing changed.
    assert_eq!(alex.vault_list(None).await.unwrap().items.len(), 1);
    assert_eq!(alex.vault_list(None).await.unwrap().items[0].name, "Note");
    stop(dir.path(), daemon).await;
}
