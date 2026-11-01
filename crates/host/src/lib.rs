//! Platform adapter.

pub mod backups;
pub mod chrome;
pub mod frame;
pub mod ipc;
pub mod playwright;
pub mod plugins;
pub mod power;
pub mod secrets;
mod service;
pub mod startup;
pub mod terminal;
pub mod tray;
pub mod updater;
#[cfg(windows)]
mod windows_security;

pub use service::{Service, restart_after_reply};

use aurelius_domain::{HostId, PrincipalId};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Override for tests and demos: run a fully separate Aurelius instance.
pub const DATA_DIR_ENV: &str = "AURELIUS_DATA_DIR";
/// Override the environment (`dev` or `production`).
pub const ENVIRONMENT_ENV: &str = "AURELIUS_ENV";

/// Dev and production never share data, background service, pipe or secrets.
/// Debug builds are Dev and release builds are Production, unless `AURELIUS_ENV` says otherwise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Environment {
    Dev,
    Production,
}

impl Environment {
    pub fn current() -> Self {
        match std::env::var(ENVIRONMENT_ENV).ok().as_deref().map(str::to_ascii_lowercase).as_deref() {
            Some("dev" | "development") => Environment::Dev,
            Some("prod" | "production") => Environment::Production,
            _ if cfg!(debug_assertions) => Environment::Dev,
            _ => Environment::Production,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Environment::Dev => "dev",
            Environment::Production => "production",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text.trim() {
            "dev" => Some(Environment::Dev),
            "production" => Some(Environment::Production),
            _ => None,
        }
    }

    /// Folder name under the per-user data directory; production keeps the original name.
    pub fn dir_name(self) -> &'static str {
        match self {
            Environment::Dev => "Aurelius-Dev",
            Environment::Production => "Aurelius",
        }
    }
}

/// Bind a data directory to one environment. The first open writes the marker;
/// opening it from the other environment is refused, so a dev build can never
/// touch production data (and vice versa), even with `AURELIUS_DATA_DIR`.
pub fn claim_environment(root: &Path, env: Environment) -> io::Result<()> {
    let path = root.join("data").join("environment");
    match fs::read_to_string(&path) {
        Ok(text) => match Environment::parse(&text) {
            Some(found) if found != env => Err(io::Error::other(format!(
                "this data folder belongs to the {} environment, but this is {}; start the matching build",
                found.as_str(),
                env.as_str()
            ))),
            _ => Ok(()),
        },
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            fs::create_dir_all(path.parent().expect("has parent"))?;
            fs::write(&path, env.as_str())
        }
        Err(e) => Err(e),
    }
}

/// `%LOCALAPPDATA%\Aurelius` (production) or `%LOCALAPPDATA%\Aurelius-Dev` on Windows;
/// the same names under `~/Library/Application Support` on macOS.
pub fn data_dir() -> io::Result<PathBuf> {
    if let Some(dir) = std::env::var_os(DATA_DIR_ENV) {
        return Ok(PathBuf::from(dir));
    }
    default_data_dir()
}

/// `data_dir` without `AURELIUS_DATA_DIR`: the data folder of this user and
/// environment. Another root (a copy, a test instance) is not this host's own.
pub fn default_data_dir() -> io::Result<PathBuf> {
    #[cfg(windows)]
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    #[cfg(target_os = "macos")]
    let base = std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"));
    #[cfg(all(unix, not(target_os = "macos")))]
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")));
    base.map(|b| b.join(Environment::current().dir_name()))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "cannot determine the per-user data directory"))
}

pub fn ensure_private_dir(path: &Path) -> io::Result<()> {
    fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// Stable installation identity. Not a secret.
pub fn host_id(root: &Path) -> io::Result<HostId> {
    stable_uuid(root, "host_id").map(HostId)
}

/// The local OS user as an Aurelius principal; injected server-side into every command.
pub fn local_principal(root: &Path) -> io::Result<PrincipalId> {
    stable_uuid(root, "local_principal_id").map(PrincipalId)
}

fn stable_uuid(root: &Path, name: &str) -> io::Result<uuid::Uuid> {
    let path = root.join("data").join(name);
    if let Ok(text) = fs::read_to_string(&path) {
        if let Ok(id) = uuid::Uuid::parse_str(text.trim()) {
            return Ok(id);
        }
    }
    fs::create_dir_all(path.parent().expect("has parent"))?;
    let id = uuid::Uuid::now_v7();
    let tmp = path.with_extension("tmp");
    let mut file = File::create(&tmp)?;
    file.write_all(id.to_string().as_bytes())?;
    file.sync_all()?;
    fs::rename(tmp, &path)?;
    Ok(id)
}

/// Held for the daemon's lifetime; a second daemon fails to acquire it.
pub struct InstanceLock {
    _file: File,
}

#[derive(Debug, thiserror::Error)]
pub enum LockError {
    #[error("another Aurelius daemon is already running for this user")]
    AlreadyRunning,
    #[error(transparent)]
    Io(#[from] io::Error),
}

pub fn acquire_instance_lock(root: &Path) -> Result<InstanceLock, LockError> {
    fs::create_dir_all(root)?;
    let file = OpenOptions::new().create(true).truncate(false).write(true).open(root.join("daemon.lock"))?;
    match file.try_lock() {
        Ok(()) => Ok(InstanceLock { _file: file }),
        Err(fs::TryLockError::WouldBlock) => Err(LockError::AlreadyRunning),
        Err(fs::TryLockError::Error(err)) => Err(err.into()),
    }
}

/// Endpoint name derived from the data dir, so separate data dirs get separate daemons.
pub fn endpoint(root: &Path) -> ipc::Endpoint {
    use sha2::Digest;
    let digest = sha2::Sha256::digest(root.to_string_lossy().as_bytes());
    let short = &hex::encode(digest)[..16];
    #[cfg(windows)]
    {
        ipc::Endpoint(format!(r"\\.\pipe\aurelius-{short}"))
    }
    #[cfg(unix)]
    {
        let _ = short;
        ipc::Endpoint(root.join("ipc").join("daemon.sock").to_string_lossy().into_owned())
    }
}

/// The internal channel of the daemon: the agent bridge and `aurelius-ctl shutdown` use it,
/// clients never do. Same user-only protection as `endpoint`, another name.
pub fn bridge_endpoint(root: &Path) -> ipc::Endpoint {
    use sha2::Digest;
    let digest = sha2::Sha256::digest(root.to_string_lossy().as_bytes());
    let short = &hex::encode(digest)[..16];
    #[cfg(windows)]
    {
        ipc::Endpoint(format!(r"\\.\pipe\aurelius-bridge-{short}"))
    }
    #[cfg(unix)]
    {
        let _ = short;
        ipc::Endpoint(root.join("ipc").join("bridge.sock").to_string_lossy().into_owned())
    }
}

/// Start the daemon binary detached from the calling process, so closing
/// the UI does not end it.
pub fn spawn_daemon_detached(executable: &Path, root: &Path) -> io::Result<()> {
    let mut command = std::process::Command::new(executable);
    command
        .env(DATA_DIR_ENV, root)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // Our own std handles may be inheritable pipes from a terminal; a
        // long-lived daemon must not keep them open (the shell would hang).
        unsafe {
            use windows_sys::Win32::Foundation::{HANDLE_FLAG_INHERIT, SetHandleInformation};
            use windows_sys::Win32::System::Console::{GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE};
            for which in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
                let handle = GetStdHandle(which);
                if !handle.is_null() {
                    SetHandleInformation(handle, HANDLE_FLAG_INHERIT, 0);
                }
            }
        }
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    command.spawn().map(|_| ())
}

/// The daemon executable next to the current binary.
pub fn sibling_executable(name: &str) -> io::Result<PathBuf> {
    let current = std::env::current_exe()?;
    let dir = current.parent().ok_or_else(|| io::Error::other("executable has no directory"))?;
    Ok(dir.join(format!("{name}{}", std::env::consts::EXE_SUFFIX)))
}

/// Root for project folders: `Documents\Aurelius` (production) or
/// `Documents\Aurelius Dev` (dev). `AURELIUS_PROJECTS_DIR` overrides it.
pub fn projects_root() -> io::Result<PathBuf> {
    if let Some(dir) = std::env::var_os("AURELIUS_PROJECTS_DIR") {
        return Ok(PathBuf::from(dir));
    }
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "cannot determine the home folder"))?;
    let name = match Environment::current() {
        Environment::Dev => "Aurelius Dev",
        Environment::Production => "Aurelius",
    };
    Ok(home.join("Documents").join(name))
}

/// A safe folder name for a project (see `aurelius_domain::project::folder_name`).
pub use aurelius_domain::project::folder_name;

/// This computer's name, lowercase, as clients show the server.
pub fn host_name() -> String {
    let from_env = std::env::var("COMPUTERNAME").or_else(|_| std::env::var("HOSTNAME")).ok();
    #[cfg(target_os = "macos")]
    let from_env = from_env.or_else(|| {
        // GUI processes on macOS have neither variable set.
        std::process::Command::new("scutil")
            .args(["--get", "ComputerName"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
    });
    from_env
        .filter(|h| !h.trim().is_empty())
        .map(|h| h.trim().to_lowercase())
        .unwrap_or_else(|| "this computer".into())
}

/// The server's name as clients see it (feature `server_name`): the one a board
/// member chose, else `host_name()`. Blocking (one row); read from the database
/// every time, so a restored backup brings its name back at once (E5). Falls
/// back to the computer's name when the database cannot be read.
pub fn server_name(store: &aurelius_storage::Store) -> aurelius_domain::server::ServerNameView {
    let stored = store.read(aurelius_storage::server_name::get).unwrap_or_else(|err| {
        tracing::warn!(error = %err, "could not read the server's name; using the computer's name");
        Default::default()
    });
    aurelius_domain::server::ServerNameView::of(stored.chosen, host_name(), stored.version)
}

/// Where a folder picker on another computer starts: the projects folder,
/// home, and (Windows) every drive.
pub fn fs_roots() -> Vec<(String, PathBuf)> {
    let mut roots = Vec::new();
    if let Ok(projects) = projects_root() {
        roots.push(("Projects".to_owned(), projects));
    }
    if let Some(home) = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")) {
        roots.push(("Home".to_owned(), PathBuf::from(home)));
    }
    #[cfg(windows)]
    {
        // SAFETY: no arguments; returns a bitmask of the existing drive letters.
        let mask = unsafe { windows_sys::Win32::Storage::FileSystem::GetLogicalDrives() };
        for (i, letter) in (b'A'..=b'Z').enumerate() {
            if mask & (1 << i) != 0 {
                roots.push((format!("{}:", letter as char), PathBuf::from(format!("{}:\\", letter as char))));
            }
        }
    }
    #[cfg(not(windows))]
    roots.push(("/".to_owned(), PathBuf::from("/")));
    roots
}


/// Local UTC offset in minutes right now, including daylight saving time.
/// Only for "now"; a given moment needs [`utc_offset_minutes_at`].
pub fn local_utc_offset_minutes() -> i64 {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64);
    utc_offset_minutes_at(now)
}

/// Local UTC offset in minutes at the moment `ms` (Unix ms), with the
/// daylight saving time that applies then, not now.
/// `AURELIUS_UTC_OFFSET_MIN` overrides it (tests, demos).
pub fn utc_offset_minutes_at(ms: i64) -> i64 {
    if let Some(v) = std::env::var("AURELIUS_UTC_OFFSET_MIN").ok().and_then(|v| v.parse::<i64>().ok()) {
        return v;
    }
    #[cfg(windows)]
    {
        windows_offset_at(ms, std::ptr::null())
    }
    #[cfg(unix)]
    {
        let t = ms.div_euclid(1000) as libc::time_t;
        // SAFETY: localtime_r reads t and fills the zeroed tm we own.
        unsafe {
            let mut tm: libc::tm = std::mem::zeroed();
            if libc::localtime_r(&t, &mut tm).is_null() {
                return 0;
            }
            tm.tm_gmtoff as i64 / 60
        }
    }
}

/// Offset at `ms` in the time zone `tz` (null: this computer's).
#[cfg(windows)]
fn windows_offset_at(ms: i64, tz: *const windows_sys::Win32::System::Time::TIME_ZONE_INFORMATION) -> i64 {
    use windows_sys::Win32::Foundation::{FILETIME, SYSTEMTIME};
    use windows_sys::Win32::System::Time::{FileTimeToSystemTime, SystemTimeToFileTime, SystemTimeToTzSpecificLocalTime};
    // 100 ns ticks since 1601-01-01; whole seconds, so the offset comes out exact.
    let ticks = (ms.div_euclid(1000) + 11_644_473_600) as u64 * 10_000_000;
    let ft = FILETIME { dwLowDateTime: ticks as u32, dwHighDateTime: (ticks >> 32) as u32 };
    // SAFETY: every pointer is to a local we own or the caller's valid (or null) tz.
    unsafe {
        let (mut utc, mut local): (SYSTEMTIME, SYSTEMTIME) = (std::mem::zeroed(), std::mem::zeroed());
        let mut local_ft: FILETIME = std::mem::zeroed();
        if FileTimeToSystemTime(&ft, &mut utc) == 0 || SystemTimeToTzSpecificLocalTime(tz, &utc, &mut local) == 0 || SystemTimeToFileTime(&local, &mut local_ft) == 0 {
            return 0;
        }
        let local_ticks = (u64::from(local_ft.dwHighDateTime) << 32) | u64::from(local_ft.dwLowDateTime);
        (local_ticks as i64 - ticks as i64) / 600_000_000
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_data_dir_belongs_to_one_environment() {
        let dir = tempfile::tempdir().unwrap();
        claim_environment(dir.path(), Environment::Production).unwrap();
        claim_environment(dir.path(), Environment::Production).unwrap();
        let err = claim_environment(dir.path(), Environment::Dev).unwrap_err();
        assert!(err.to_string().contains("production"), "{err}");
        assert_ne!(Environment::Dev.dir_name(), Environment::Production.dir_name());
    }

    /// 2026-10-25 (last Sunday of October): Brussels goes from UTC+2 to UTC+1 at 01:00 UTC.
    const BRUSSELS_FALL_BACK_MS: i64 = 1_792_890_000_000;

    // POSIX `tzset`: not every `libc` target exports it (Linux glibc does not).
    #[cfg(unix)]
    unsafe extern "C" {
        fn tzset();
    }

    #[cfg(unix)]
    #[test]
    fn local_offset_follows_the_tz_variable() {
        // Only this test touches TZ; the override is unset so the libc path runs.
        unsafe {
            std::env::remove_var("AURELIUS_UTC_OFFSET_MIN");
            std::env::set_var("TZ", "Asia/Kolkata");
            tzset();
        }
        assert_eq!(local_utc_offset_minutes(), 330);
        unsafe {
            std::env::set_var("TZ", "Europe/Brussels");
            tzset();
        }
        assert_eq!(utc_offset_minutes_at(BRUSSELS_FALL_BACK_MS - 1000), 120);
        assert_eq!(utc_offset_minutes_at(BRUSSELS_FALL_BACK_MS), 60);
    }

    #[cfg(windows)]
    #[test]
    fn offset_is_the_one_at_the_moment_across_dst() {
        use windows_sys::Win32::{Foundation::SYSTEMTIME, System::Time::TIME_ZONE_INFORMATION};
        // Romance Standard Time: last Sunday of October 03:00 back to UTC+1, of March 02:00 on to UTC+2.
        let rule = |month, hour| SYSTEMTIME { wYear: 0, wMonth: month, wDayOfWeek: 0, wDay: 5, wHour: hour, wMinute: 0, wSecond: 0, wMilliseconds: 0 };
        let mut tz: TIME_ZONE_INFORMATION = unsafe { std::mem::zeroed() };
        tz.Bias = -60;
        tz.StandardDate = rule(10, 3);
        tz.DaylightDate = rule(3, 2);
        tz.DaylightBias = -60;
        assert_eq!(windows_offset_at(BRUSSELS_FALL_BACK_MS - 1000, &tz), 120);
        assert_eq!(windows_offset_at(BRUSSELS_FALL_BACK_MS, &tz), 60);
        assert_eq!(windows_offset_at(BRUSSELS_FALL_BACK_MS - 90 * 86_400_000, &tz), 120, "summer");
        assert_eq!(windows_offset_at(BRUSSELS_FALL_BACK_MS + 60 * 86_400_000, &tz), 60, "winter");
    }

    #[test]
    fn host_id_is_stable() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(host_id(dir.path()).unwrap(), host_id(dir.path()).unwrap());
    }

    #[test]
    fn second_lock_is_refused_until_released() {
        let dir = tempfile::tempdir().unwrap();
        let first = acquire_instance_lock(dir.path()).unwrap();
        assert!(matches!(acquire_instance_lock(dir.path()), Err(LockError::AlreadyRunning)));
        drop(first);
        acquire_instance_lock(dir.path()).unwrap();
    }
}
