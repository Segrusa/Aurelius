//! OS credential store. Secrets never enter SQLite, events,
//! logs or argv; the database only stores the reference (`secret_ref`).

use std::io;

/// Target name for a reference; dev and production never share credentials.
pub fn target(secret_ref: &str) -> String {
    format!("{}/{secret_ref}", crate::Environment::current().dir_name())
}

#[cfg(windows)]
mod imp {
    use super::*;
    use std::ptr;
    use windows_sys::Win32::Foundation::{ERROR_NOT_FOUND, GetLastError};
    use windows_sys::Win32::Security::Credentials::{
        CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredDeleteW, CredFree, CredReadW, CredWriteW,
    };

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(Some(0)).collect()
    }

    pub fn set(secret_ref: &str, secret: &str) -> io::Result<()> {
        let name = wide(&target(secret_ref));
        let user = wide("aurelius");
        let blob = secret.as_bytes();
        let credential = CREDENTIALW {
            Flags: 0,
            Type: CRED_TYPE_GENERIC,
            TargetName: name.as_ptr() as *mut u16,
            Comment: ptr::null_mut(),
            LastWritten: unsafe { std::mem::zeroed() },
            CredentialBlobSize: blob.len() as u32,
            CredentialBlob: blob.as_ptr() as *mut u8,
            // Per user; not roaming to other machines.
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            AttributeCount: 0,
            Attributes: ptr::null_mut(),
            TargetAlias: ptr::null_mut(),
            UserName: user.as_ptr() as *mut u16,
        };
        // SAFETY: all pointers are valid for the duration of the call.
        if unsafe { CredWriteW(&credential, 0) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    pub fn get(secret_ref: &str) -> io::Result<Option<String>> {
        let name = wide(&target(secret_ref));
        let mut out: *mut CREDENTIALW = ptr::null_mut();
        // SAFETY: CredReadW allocates `out`, freed with CredFree below.
        unsafe {
            if CredReadW(name.as_ptr(), CRED_TYPE_GENERIC, 0, &mut out) == 0 {
                if GetLastError() == ERROR_NOT_FOUND {
                    return Ok(None);
                }
                return Err(io::Error::last_os_error());
            }
            let cred = &*out;
            let bytes = std::slice::from_raw_parts(cred.CredentialBlob, cred.CredentialBlobSize as usize).to_vec();
            CredFree(out.cast());
            Ok(Some(String::from_utf8(bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?))
        }
    }

    pub fn create_new(secret_ref: &str, secret: &str) -> io::Result<()> {
        super::checked_create(secret_ref, secret, get, set)
    }

    pub fn get_private(secret_ref: &str) -> io::Result<Option<String>> {
        get(secret_ref)
    }

    pub fn delete(secret_ref: &str) -> io::Result<()> {
        let name = wide(&target(secret_ref));
        // SAFETY: plain call with a valid NUL-terminated name.
        unsafe {
            if CredDeleteW(name.as_ptr(), CRED_TYPE_GENERIC, 0) == 0 && GetLastError() != ERROR_NOT_FOUND {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(())
    }
}

/// A file per secret, readable only by this user, in `<data dir>/secrets/`.
/// For systems without a credential store Aurelius supports yet, and on macOS
/// with `AURELIUS_SECRETS=file` (unsigned dev builds would make the Keychain
/// ask again after every rebuild).
#[cfg(unix)]
mod file {
    use super::*;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    use std::path::{Path, PathBuf};

    fn path(secret_ref: &str) -> io::Result<PathBuf> {
        Ok(super::file_in(&crate::data_dir()?.join("secrets"), secret_ref))
    }

    pub fn set(secret_ref: &str, secret: &str) -> io::Result<()> {
        set_at(&path(secret_ref)?, secret)
    }

    pub fn get(secret_ref: &str) -> io::Result<Option<String>> {
        match std::fs::read_to_string(path(secret_ref)?) {
            Ok(text) => Ok(Some(text)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn create_new(secret_ref: &str, secret: &str) -> io::Result<()> {
        create_new_at(&path(secret_ref)?, secret)
    }

    pub fn get_private(secret_ref: &str) -> io::Result<Option<String>> {
        get_private_at(&path(secret_ref)?)
    }

    fn folder(file: &Path) -> io::Result<&Path> {
        let dir = file.parent().expect("secrets folder");
        std::fs::create_dir_all(dir)?;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
        Ok(dir)
    }

    /// A new temporary file next to `file`, 0600 on the descriptor itself
    /// (a stale `.tmp` with another mode never keeps it, SEC0 E20), written
    /// and flushed to disk.
    fn written(file: &Path, secret: &str) -> io::Result<PathBuf> {
        let tmp = file.with_extension(format!("{}.tmp", uuid::Uuid::now_v7().simple()));
        let mut out = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&tmp)?;
        let done = (|| {
            out.set_permissions(std::fs::Permissions::from_mode(0o600))?;
            std::io::Write::write_all(&mut out, secret.as_bytes())?;
            out.sync_all()
        })();
        if let Err(e) = done {
            let _ = std::fs::remove_file(&tmp);
            return Err(e);
        }
        Ok(tmp)
    }

    fn sync_dir(dir: &Path) -> io::Result<()> {
        std::fs::File::open(dir)?.sync_all()
    }

    pub fn set_at(file: &Path, secret: &str) -> io::Result<()> {
        let dir = folder(file)?;
        let tmp = written(file, secret)?;
        std::fs::rename(&tmp, file).inspect_err(|_| {
            let _ = std::fs::remove_file(&tmp);
        })?;
        sync_dir(dir)
    }

    /// Only when there is none yet: the new file is linked in place, which
    /// fails when `file` exists (`AlreadyExists`), so a key is never replaced.
    pub fn create_new_at(file: &Path, secret: &str) -> io::Result<()> {
        let dir = folder(file)?;
        let tmp = written(file, secret)?;
        let linked = std::fs::hard_link(&tmp, file);
        let _ = std::fs::remove_file(&tmp);
        linked?;
        sync_dir(dir)
    }

    /// Like `get`, but only from a file of this user that no one else may
    /// read (`mode & 0o077 == 0`), else `PermissionDenied` (SEC0 E4).
    pub fn get_private_at(file: &Path) -> io::Result<Option<String>> {
        use std::os::unix::fs::MetadataExt;
        let meta = match std::fs::symlink_metadata(file) {
            Ok(meta) => meta,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
        };
        // SAFETY: geteuid has no preconditions.
        let me = unsafe { libc::geteuid() };
        if !meta.is_file() || meta.mode() & 0o077 != 0 || meta.uid() != me {
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, format!("{} must be a file of this user with mode 0600", file.display())));
        }
        std::fs::read_to_string(file).map(Some)
    }

    pub fn delete(secret_ref: &str) -> io::Result<()> {
        match std::fs::remove_file(path(secret_ref)?) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }
}

/// The macOS Keychain (login keychain, generic passwords): service = the
/// target name, account "aurelius".
#[cfg(target_os = "macos")]
mod imp {
    use super::*;
    use security_framework::passwords;

    /// errSecItemNotFound
    const NOT_FOUND: i32 = -25300;
    const ACCOUNT: &str = "aurelius";

    fn files() -> bool {
        std::env::var("AURELIUS_SECRETS").is_ok_and(|v| v == "file")
    }

    fn os(err: security_framework::base::Error) -> io::Error {
        io::Error::other(format!("Keychain: {err}"))
    }

    pub fn set(secret_ref: &str, secret: &str) -> io::Result<()> {
        if files() {
            return super::file::set(secret_ref, secret);
        }
        passwords::set_generic_password(&target(secret_ref), ACCOUNT, secret.as_bytes()).map_err(os)
    }

    pub fn get(secret_ref: &str) -> io::Result<Option<String>> {
        if files() {
            return super::file::get(secret_ref);
        }
        match passwords::get_generic_password(&target(secret_ref), ACCOUNT) {
            Ok(bytes) => String::from_utf8(bytes).map(Some).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e)),
            Err(err) if err.code() == NOT_FOUND => Ok(None),
            Err(err) => Err(os(err)),
        }
    }

    pub fn create_new(secret_ref: &str, secret: &str) -> io::Result<()> {
        if files() {
            return super::file::create_new(secret_ref, secret);
        }
        super::checked_create(secret_ref, secret, get, set)
    }

    pub fn get_private(secret_ref: &str) -> io::Result<Option<String>> {
        if files() {
            return super::file::get_private(secret_ref);
        }
        get(secret_ref)
    }

    pub fn delete(secret_ref: &str) -> io::Result<()> {
        if files() {
            return super::file::delete(secret_ref);
        }
        match passwords::delete_generic_password(&target(secret_ref), ACCOUNT) {
            Err(err) if err.code() != NOT_FOUND => Err(os(err)),
            _ => Ok(()),
        }
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod imp {
    pub use super::file::{create_new, delete, get, get_private, set};
}

#[cfg(not(any(windows, unix)))]
mod imp {
    use super::*;
    fn unsupported() -> io::Error {
        io::Error::new(io::ErrorKind::Unsupported, "the OS credential store is not implemented on this platform yet")
    }
    pub fn set(_: &str, _: &str) -> io::Result<()> {
        Err(unsupported())
    }
    pub fn get(_: &str) -> io::Result<Option<String>> {
        Err(unsupported())
    }
    pub fn delete(_: &str) -> io::Result<()> {
        Err(unsupported())
    }
    pub fn create_new(_: &str, _: &str) -> io::Result<()> {
        Err(unsupported())
    }
    pub fn get_private(_: &str) -> io::Result<Option<String>> {
        Err(unsupported())
    }
}

/// `create_new` for a store without "create only if absent" (Keychain,
/// Credential Manager): read, write, read again and compare. A store that
/// fails to say whether it has one is an error, never "absent".
#[allow(dead_code)]
fn checked_create(secret_ref: &str, secret: &str, get: fn(&str) -> io::Result<Option<String>>, set: fn(&str, &str) -> io::Result<()>) -> io::Result<()> {
    if get(secret_ref)?.is_some() {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, "a secret with this name exists"));
    }
    set(secret_ref, secret)?;
    match get(secret_ref)? {
        Some(stored) if stored == secret => Ok(()),
        _ => Err(io::Error::other("the credential store did not keep the new secret")),
    }
}

/// The file a secret keeps in `dir` (a data folder's `secrets/`) in the file
/// store: what a test instance or `AURELIUS_SECRETS=file` uses on every system.
pub fn file_in(dir: &std::path::Path, secret_ref: &str) -> std::path::PathBuf {
    let name: String = target(secret_ref).chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '.' { c } else { '_' }).collect();
    dir.join(name)
}

/// The file store at an explicit file (`file_in`): `create_new`, `get_private`
/// and `set` as above.
pub mod at {
    #[cfg(not(unix))]
    use std::{io, path::Path};

    #[cfg(unix)]
    pub use super::file::{create_new_at as create_new, get_private_at as get_private, set_at as set};

    #[cfg(not(unix))]
    pub fn create_new(file: &Path, secret: &str) -> io::Result<()> {
        std::fs::create_dir_all(file.parent().expect("secrets folder"))?;
        let mut out = std::fs::OpenOptions::new().write(true).create_new(true).open(file)?;
        std::io::Write::write_all(&mut out, secret.as_bytes())?;
        out.sync_all()
    }

    #[cfg(not(unix))]
    pub fn get_private(file: &Path) -> io::Result<Option<String>> {
        match std::fs::read_to_string(file) {
            Ok(text) => Ok(Some(text)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    #[cfg(not(unix))]
    pub fn set(file: &Path, secret: &str) -> io::Result<()> {
        std::fs::create_dir_all(file.parent().expect("secrets folder"))?;
        std::fs::write(file, secret)
    }
}

/// Every secret: create one only where there is none (`AlreadyExists`
/// otherwise, never a replacement), read one only when it is private
/// (`get_private`: on Linux a file of this user, mode 0600).
pub use imp::{create_new, delete, get, get_private, set};

#[cfg(all(test, unix))]
mod file_tests {
    use super::file::*;
    use std::os::unix::fs::PermissionsExt;

    /// SEC0 E4, E20: never a replacement; 0600 set on the file itself; a
    /// file someone else may read is refused.
    #[test]
    fn a_secret_file_is_created_once_private_and_read_only_when_private() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("secrets").join("Aurelius-Dev_vault-master");
        assert_eq!(get_private_at(&file).unwrap(), None);
        create_new_at(&file, "first-FAKE-key").unwrap();
        let again = create_new_at(&file, "second-FAKE-key").unwrap_err();
        assert_eq!(again.kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(get_private_at(&file).unwrap().as_deref(), Some("first-FAKE-key"));
        assert_eq!(std::fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(std::fs::metadata(file.parent().unwrap()).unwrap().permissions().mode() & 0o777, 0o700);
        let left: Vec<_> = std::fs::read_dir(file.parent().unwrap()).unwrap().filter_map(Result::ok).map(|e| e.file_name()).collect();
        assert_eq!(left.len(), 1, "no temporary file is left: {left:?}");
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(get_private_at(&file).unwrap_err().kind(), std::io::ErrorKind::PermissionDenied);
        // `set` replaces, and makes it private again on the descriptor.
        set_at(&file, "third-FAKE-key").unwrap();
        assert_eq!(get_private_at(&file).unwrap().as_deref(), Some("third-FAKE-key"));
    }
}

#[cfg(all(test, windows))]
mod tests {
    #[test]
    fn roundtrip_in_credential_manager() {
        let r = format!("test-{}", aurelius_domain::ProviderId::new());
        assert_eq!(super::get(&r).unwrap(), None);
        super::set(&r, "sk-test-123").unwrap();
        assert_eq!(super::get(&r).unwrap().as_deref(), Some("sk-test-123"));
        super::delete(&r).unwrap();
        assert_eq!(super::get(&r).unwrap(), None);
    }
}
