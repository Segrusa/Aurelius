//! `aurelius-daemon vault-key import`: put an exported vault key in place on
//! this server (a restore on another machine, Sentinel SEC0 E10, E11).
//!
//! * The key comes from standard input or from a file only this user may read
//!   (0600), never as an argument (`ps`, shell history).
//! * It runs only while the daemon is stopped: it takes the daemon's own
//!   instance lock.
//! * It never replaces a key that encrypts a stored value; a key no value uses
//!   (a new machine's empty vault) may be replaced.
//! * The key must fit: its line is checked (format, environment, id) and, when
//!   values are stored, at least one of them must open with it.
//! * No rotation in phase 4: one key per server.

use super::crypto::{Binding, Keys};
use super::key::{self, Store};
use std::io::Read;
use std::path::{Path, PathBuf};

/// Where the key line comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    Stdin,
    /// A file of this user, mode 0600.
    File(PathBuf),
}

/// The arguments after `vault-key import`: nothing (standard input) or
/// `--file PATH`. Anything else is refused, never echoed: it may be the key.
pub fn parse_args(args: &[String]) -> Result<Source, String> {
    match args {
        [] => Ok(Source::Stdin),
        [flag, path] if flag == "--file" => Ok(Source::File(PathBuf::from(path))),
        _ => Err("usage: aurelius-daemon vault-key import [--file PATH]  (the key comes from standard input or a 0600 file, never from the command line)".into()),
    }
}

/// A key typed at a terminal would be echoed onto the screen and into any
/// terminal log (SEC1 m5): standard input must be a pipe or a file
/// (`aurelius-daemon vault-key import < key.txt`), or `--file` is used.
pub fn refuse_terminal(source: &Source, stdin_is_terminal: bool) -> Result<(), String> {
    match source {
        Source::Stdin if stdin_is_terminal => {
            Err("the key is not typed in (the terminal would show it): pipe the key in or use --file, for example aurelius-daemon vault-key import < key.txt".into())
        }
        _ => Ok(()),
    }
}

fn read(source: &Source, stdin: &mut dyn Read) -> Result<zeroize::Zeroizing<String>, String> {
    match source {
        Source::Stdin => {
            let mut text = zeroize::Zeroizing::new(String::new());
            stdin.take(4096).read_to_string(&mut text).map_err(|e| format!("could not read the key from standard input: {e}"))?;
            Ok(text)
        }
        Source::File(path) => match aurelius_host::secrets::at::get_private(path) {
            Ok(Some(text)) => Ok(zeroize::Zeroizing::new(text)),
            Ok(None) => Err(format!("{} does not exist", path.display())),
            Err(e) => Err(format!("{}: {e}", path.display())),
        },
    }
}

/// Import the key of `source` into the vault of `root`.
pub fn run(root: &Path, own_root: bool, source: &Source, stdin: &mut dyn Read) -> Result<String, String> {
    let _lock = aurelius_host::acquire_instance_lock(root).map_err(|e| match e {
        aurelius_host::LockError::AlreadyRunning => "the Aurelius daemon is running: stop it first, then import the key".to_string(),
        other => format!("could not take the daemon's lock: {other}"),
    })?;
    let line = read(source, stdin)?;
    let master = key::parse_line(&line).map_err(|e| e.to_string())?;
    let keys = Keys::derive(&master);
    let store = aurelius_storage::Store::open(root).map_err(|e| format!("could not open the database: {e}"))?;
    let values = store.read(aurelius_storage::vault::redaction_values).map_err(|e| format!("could not read the vault: {e}"))?;
    let mine: Vec<_> = values.iter().filter(|v| v.sealed.key_id == keys.key_id()).collect();
    if !values.is_empty() {
        let opens = mine.iter().any(|v| Binding::of_value(v).is_some_and(|b| keys.open(&b, &v.sealed).is_ok()));
        if !opens {
            return Err("this key does not open the values stored on this server: it is another server's key".into());
        }
    }
    let keeper = Store::for_root(root, own_root);
    match keeper.load() {
        Ok(Some(current)) if current.keys.key_id() == keys.key_id() => Ok(format!("The vault key {} is already in place.", keys.key_id())),
        Ok(Some(current)) => {
            let used = values.iter().any(|v| v.sealed.key_id == current.keys.key_id());
            if used {
                return Err(format!("this server's vault key {} encrypts stored values: it is never replaced", current.keys.key_id()));
            }
            keeper.replace(&key::export_line(&master)).map_err(|e| format!("could not store the key: {e}"))?;
            Ok(format!("The vault key {} replaced an unused one. Start Aurelius again.", keys.key_id()))
        }
        Ok(None) => {
            keeper.put_new(&key::export_line(&master)).map_err(|e| format!("could not store the key: {e}"))?;
            Ok(format!("The vault key {} is in place. Start Aurelius again.", keys.key_id()))
        }
        Err(why) => Err(format!("{why}; fix or remove it by hand first")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// SEC0 E11: the key never comes as an argument.
    #[test]
    fn the_key_is_never_taken_from_the_command_line() {
        assert_eq!(parse_args(&[]), Ok(Source::Stdin));
        assert_eq!(parse_args(&["--file".into(), "/tmp/k".into()]), Ok(Source::File("/tmp/k".into())));
        let err = parse_args(&["aurelius-vault-key:v1:dev:FAKEID:FAKEKEY".into()]).unwrap_err();
        assert!(!err.contains("FAKE"), "{err}");
        assert!(parse_args(&["--key".into(), "x".into()]).is_err());
    }

    /// SEC1 m5: never read from a terminal, which echoes what is typed.
    #[test]
    fn a_key_is_never_typed_at_a_terminal() {
        let err = refuse_terminal(&Source::Stdin, true).unwrap_err();
        assert!(err.contains("pipe the key in or use --file"), "{err}");
        assert_eq!(refuse_terminal(&Source::Stdin, false), Ok(()));
        assert_eq!(refuse_terminal(&Source::File("/tmp/k".into()), true), Ok(()));
    }

    /// SEC0 E11: a key file others may read is refused.
    #[cfg(unix)]
    #[test]
    fn a_key_file_must_be_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("key");
        std::fs::write(&file, "aurelius-vault-key:v1:dev:x:y").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(read(&Source::File(file.clone()), &mut std::io::empty()).is_err());
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(read(&Source::File(file), &mut std::io::empty()).is_ok());
    }
}
