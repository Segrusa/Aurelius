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
    use std::path::PathBuf;

    fn path(secret_ref: &str) -> io::Result<PathBuf> {
        let name: String = target(secret_ref).chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '.' { c } else { '_' }).collect();
        Ok(crate::data_dir()?.join("secrets").join(name))
    }

    pub fn set(secret_ref: &str, secret: &str) -> io::Result<()> {
        let file = path(secret_ref)?;
        let dir = file.parent().expect("secrets folder");
        std::fs::create_dir_all(dir)?;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
        let tmp = file.with_extension("tmp");
        let mut out = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&tmp)?;
        std::io::Write::write_all(&mut out, secret.as_bytes())?;
        std::fs::rename(tmp, file)
    }

    pub fn get(secret_ref: &str) -> io::Result<Option<String>> {
        match std::fs::read_to_string(path(secret_ref)?) {
            Ok(text) => Ok(Some(text)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
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
    pub use super::file::{delete, get, set};
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
}

pub use imp::{delete, get, set};

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
