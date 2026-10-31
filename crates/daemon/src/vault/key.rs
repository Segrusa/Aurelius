//! The vault's master key: where it is kept, how it is made, exported and
//! imported (Sentinel SEC0 E4, E9, E10, E11).
//!
//! * Kept in the host's secret store under `vault-master` (Keychain,
//!   Credential Manager; on Linux, for a test instance or with
//!   `AURELIUS_SECRETS=file` a file 0600 in `<data>/secrets/`), never in the
//!   database or a backup. Stored as its export line, so its id travels along.
//! * Made only when the store says there is none (`Ok(None)`) **and** no
//!   ciphertext is stored anywhere; made with `create_new`, which never
//!   replaces one. A store that cannot answer (`Err`) stops the vault
//!   ("unavailable"): it never counts as "there is none".
//! * Read on Linux only from a file of this user that no one else may read.
//! * Never rotated in phase 4 (E10): one key per server.

use super::crypto::{KEY_LEN, Keys, MasterKey};
use base64::Engine;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use zeroize::Zeroizing;

/// The host secret store's name for the key.
pub const KEY_REF: &str = "vault-master";
const PREFIX: &str = "aurelius-vault-key";

/// `dev` or `prod`: a key of one environment never goes into the other.
pub fn env_label() -> &'static str {
    match aurelius_host::Environment::current() {
        aurelius_host::Environment::Dev => "dev",
        aurelius_host::Environment::Production => "prod",
    }
}

/// `aurelius-vault-key:v1:<env>:<key_id>:<base64url>`.
pub fn export_line(master: &MasterKey) -> Zeroizing<String> {
    let keys = Keys::derive(master);
    let encoded = Zeroizing::new(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(master.bytes()));
    Zeroizing::new(format!("{PREFIX}:v1:{}:{}:{}", env_label(), keys.key_id(), encoded.as_str()))
}

/// Why an export line is refused (never quoting it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BadLine {
    Format,
    /// Made by a server of the other environment.
    Environment(String),
    /// The id does not belong to the key (a typo, a cut line).
    KeyId,
}

impl std::fmt::Display for BadLine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BadLine::Format => write!(f, "this is not an Aurelius vault key (aurelius-vault-key:v1:…)"),
            BadLine::Environment(env) => write!(f, "this key belongs to a {env} server, not to this {} one", env_label()),
            BadLine::KeyId => write!(f, "the key does not match its id: it is damaged or cut off"),
        }
    }
}

/// The key in an export line, checked: format, environment, and that the id
/// is the key's.
pub fn parse_line(line: &str) -> Result<MasterKey, BadLine> {
    let line = line.trim();
    let parts: Vec<&str> = line.split(':').collect();
    let [prefix, version, env, key_id, encoded] = parts[..] else { return Err(BadLine::Format) };
    if prefix != PREFIX || version != "v1" {
        return Err(BadLine::Format);
    }
    if env != env_label() {
        return Err(BadLine::Environment(env.chars().filter(char::is_ascii_alphanumeric).take(16).collect()));
    }
    let bytes = Zeroizing::new(base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(encoded).map_err(|_| BadLine::Format)?);
    if bytes.len() != KEY_LEN {
        return Err(BadLine::Format);
    }
    let master = MasterKey::from_bytes(&bytes).ok_or(BadLine::Format)?;
    if Keys::derive(&master).key_id() != key_id {
        return Err(BadLine::KeyId);
    }
    Ok(master)
}

/// Where the key is kept.
#[derive(Debug, Clone)]
pub enum Store {
    /// The host's own store (`aurelius_host::secrets`).
    Os,
    /// A file in this folder (a data folder's `secrets/`).
    Dir(PathBuf),
}

impl Store {
    /// The OS store only for this host's own data folder on macOS and Windows
    /// (unless `AURELIUS_SECRETS=file`); a file in `<root>/secrets` otherwise,
    /// so a test instance never touches the system's keychain.
    pub fn for_root(root: &std::path::Path, own_root: bool) -> Self {
        let files = !cfg!(any(target_os = "macos", windows)) || !own_root || std::env::var("AURELIUS_SECRETS").is_ok_and(|v| v == "file");
        if files { Store::Dir(root.join("secrets")) } else { Store::Os }
    }

    fn get(&self) -> io::Result<Option<Zeroizing<String>>> {
        let text = match self {
            Store::Os => aurelius_host::secrets::get_private(KEY_REF)?,
            Store::Dir(dir) => aurelius_host::secrets::at::get_private(&aurelius_host::secrets::file_in(dir, KEY_REF))?,
        };
        Ok(text.map(Zeroizing::new))
    }

    fn create_new(&self, line: &str) -> io::Result<()> {
        match self {
            Store::Os => aurelius_host::secrets::create_new(KEY_REF, line),
            Store::Dir(dir) => aurelius_host::secrets::at::create_new(&aurelius_host::secrets::file_in(dir, KEY_REF), line),
        }
    }

    /// Replace the key (an import over one no ciphertext uses, E10).
    pub(crate) fn replace(&self, line: &str) -> io::Result<()> {
        match self {
            Store::Os => aurelius_host::secrets::set(KEY_REF, line),
            Store::Dir(dir) => aurelius_host::secrets::at::set(&aurelius_host::secrets::file_in(dir, KEY_REF), line),
        }
    }

    pub(crate) fn put_new(&self, line: &str) -> io::Result<()> {
        self.create_new(line)
    }

    /// The key that is kept: none, or why it cannot be used.
    pub fn load(&self) -> Result<Option<Loaded>, String> {
        match self.get() {
            Ok(None) => Ok(None),
            Ok(Some(line)) => match parse_line(&line) {
                Ok(master) => Ok(Some(Loaded { keys: Arc::new(Keys::derive(&master)), master: Arc::new(master) })),
                Err(why) => Err(format!("the stored vault key is not usable: {why}")),
            },
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied => Err(format!("the vault key may be read by others: {e}. Make it private (chmod 600) and restart")),
            Err(e) => Err(format!("the vault key could not be read: {e}")),
        }
    }

    /// The key, made now when there is none and nothing is encrypted yet.
    /// `any_ciphertext` asks the database; it is asked only when there is no key.
    pub fn load_or_create(&self, any_ciphertext: impl FnOnce() -> Result<bool, String>) -> Result<Loaded, String> {
        if let Some(loaded) = self.load()? {
            return Ok(loaded);
        }
        if any_ciphertext()? {
            return Err("the vault key is missing while values are stored: import it (aurelius-daemon vault-key import)".into());
        }
        let master = MasterKey::generate().map_err(|_| "no random bytes for a vault key".to_string())?;
        match self.create_new(&export_line(&master)) {
            Ok(()) => {}
            // Made at the same time by someone else: use that one.
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(format!("the vault key could not be stored: {e}")),
        }
        // Read back: only what the store really keeps is used.
        self.load()?.ok_or_else(|| "the vault key was not kept".to_string())
    }
}

/// A usable key. Prints its id only.
#[derive(Clone)]
pub struct Loaded {
    pub keys: Arc<Keys>,
    pub master: Arc<MasterKey>,
}

impl std::fmt::Debug for Loaded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Loaded").field("key_id", &self.keys.key_id()).finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_export_line_carries_the_key_and_its_id_and_is_checked() {
        let master = MasterKey::generate().unwrap();
        let line = export_line(&master);
        let parts: Vec<&str> = line.split(':').collect();
        assert_eq!(parts[..3], ["aurelius-vault-key", "v1", env_label()]);
        assert_eq!(parts[3], Keys::derive(&master).key_id());
        assert_eq!(parse_line(&format!("  {}\n", line.as_str())).unwrap().bytes(), master.bytes());
        let other_env = line.replace(&format!(":{}:", env_label()), if env_label() == "dev" { ":prod:" } else { ":dev:" });
        assert!(matches!(parse_line(&other_env), Err(BadLine::Environment(_))));
        let other_id = format!("{PREFIX}:v1:{}:{}:{}", env_label(), "0".repeat(32), parts[4]);
        assert_eq!(parse_line(&other_id).unwrap_err(), BadLine::KeyId);
        assert_eq!(parse_line(&line[..line.len() - 2]).unwrap_err(), BadLine::Format);
        assert_eq!(parse_line("sk-test-FAKE-0000").unwrap_err(), BadLine::Format);
        assert!(!BadLine::KeyId.to_string().contains(parts[4]));
    }

    /// SEC0 E4: a key only when there is none and nothing is encrypted; never
    /// when the store fails; never replaced; a key others may read is refused.
    #[test]
    fn a_key_is_made_once_only_without_ciphertext_and_never_when_the_store_fails() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::Dir(dir.path().join("secrets"));
        assert!(store.load().unwrap().is_none());
        let refused = store.load_or_create(|| Ok(true)).unwrap_err();
        assert!(refused.contains("missing"), "{refused}");
        assert!(store.load().unwrap().is_none(), "no key made while ciphertext exists");
        assert!(store.load_or_create(|| Err("database down".into())).is_err());
        assert!(store.load().unwrap().is_none(), "no key made when the database cannot say");
        let first = store.load_or_create(|| Ok(false)).unwrap();
        let again = store.load_or_create(|| panic!("not asked: there is a key")).unwrap();
        assert_eq!(first.keys.key_id(), again.keys.key_id());
        // A store that cannot be read is "unavailable", never "absent".
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let file = aurelius_host::secrets::file_in(&dir.path().join("secrets"), KEY_REF);
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
            let err = store.load_or_create(|| Ok(false)).err().expect("refused");
            assert!(err.contains("chmod 600"), "{err}");
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
            assert_eq!(store.load().unwrap().unwrap().keys.key_id(), first.keys.key_id(), "the key was never replaced");
            std::fs::write(&file, "damaged").unwrap();
            assert!(store.load_or_create(|| Ok(false)).is_err(), "a damaged key is never replaced by a new one");
        }
    }
}
