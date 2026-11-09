//! Optional start at sign-in. Off by default. When on, the
//! background service starts with Windows so tasks continue
//! without the window; the app connects to it when opened.

use std::io;
use std::path::{Path, PathBuf};

#[cfg(windows)]
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

/// The folder of the installed production app, or why this isn't one
/// (dev builds and builds from source never register themselves).
pub fn installed_dir() -> Result<PathBuf, String> {
    if crate::Environment::current() != crate::Environment::Production {
        return Err("Only the installed production app can start at sign-in.".into());
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let dir = exe.parent().ok_or("no install folder")?.to_path_buf();
    if dir.components().any(|c| c.as_os_str() == "target") {
        return Err("This is a build from source; install it with scripts/install-production.ps1.".into());
    }
    Ok(dir)
}

#[cfg(windows)]
fn value_name() -> &'static str {
    crate::Environment::current().dir_name()
}

fn command(dir: &Path) -> String {
    format!("\"{}\"", dir.join("aurelius-daemon.exe").display())
}

#[cfg(windows)]
mod imp {
    use super::*;
    use std::ptr;
    use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
    use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW};

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub fn registered() -> Option<String> {
        let (key, name) = (wide(RUN_KEY), wide(value_name()));
        let mut size: u32 = 0;
        // SAFETY: valid NUL-terminated strings; first call only asks for the size.
        let status = unsafe { RegGetValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr(), RRF_RT_REG_SZ, ptr::null_mut(), ptr::null_mut(), &mut size) };
        if status != ERROR_SUCCESS || size == 0 {
            return None;
        }
        let mut buf = vec![0u16; (size as usize).div_ceil(2)];
        // SAFETY: `buf` holds `size` bytes.
        let status = unsafe { RegGetValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr(), RRF_RT_REG_SZ, ptr::null_mut(), buf.as_mut_ptr().cast(), &mut size) };
        (status == ERROR_SUCCESS).then(|| String::from_utf16_lossy(&buf).trim_end_matches('\0').to_string())
    }

    pub fn set(value: Option<&str>) -> io::Result<()> {
        let (key, name) = (wide(RUN_KEY), wide(value_name()));
        // SAFETY: valid NUL-terminated strings; data length includes the terminator.
        let status = unsafe {
            match value {
                Some(v) => {
                    let data = wide(v);
                    RegSetKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr(), REG_SZ, data.as_ptr().cast(), (data.len() * 2) as u32)
                }
                None => RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr()),
            }
        };
        match status {
            ERROR_SUCCESS => Ok(()),
            ERROR_FILE_NOT_FOUND if value.is_none() => Ok(()),
            code => Err(io::Error::from_raw_os_error(code as i32)),
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use super::*;
    pub fn registered() -> Option<String> {
        None
    }
    pub fn set(_: Option<&str>) -> io::Result<()> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "start at sign-in is not available on this platform yet"))
    }
}

/// Is the background service registered to start at sign-in (for this install)?
pub fn enabled() -> bool {
    match (imp::registered(), installed_dir()) {
        (Some(v), Ok(dir)) => v == command(&dir),
        _ => false,
    }
}

pub fn set_enabled(on: bool) -> Result<(), String> {
    let dir = installed_dir()?;
    imp::set(on.then(|| command(&dir)).as_deref()).map_err(|e| e.to_string())
}
