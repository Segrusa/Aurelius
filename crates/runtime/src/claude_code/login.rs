//! A board member's own Claude login (phase 3): a config folder of its own,
//! `<data>/logins/<user_id>/`, passed to the child as `CLAUDE_CONFIG_DIR`.
//! The board member signs in there with the unmodified `claude auth login`;
//! Aurelius never reads, copies or proxies what `claude` keeps in it.
//!
//! The folders are made 0700 in one step (Sentinel D1) and checked when the
//! daemon starts. The folder name is the user's UUID, never a name, and the
//! path is checked to stay under `logins/`. There is no fallback: a run without
//! a login never starts (it waits), and no child ever sees the daemon user's
//! own `~/.claude` (`CLAUDE_CONFIG_DIR` always points somewhere of Aurelius').

use aurelius_domain::UserId;
use std::path::{Path, PathBuf};

/// The folder under the data folder that holds every board member's login.
pub const LOGINS_DIR: &str = "logins";

/// `CLAUDE_CONFIG_DIR`: where Claude Code keeps credentials, settings and sessions.
pub const CONFIG_DIR_ENV: &str = "CLAUDE_CONFIG_DIR";

/// One board member's Claude login for a child: only this login goes in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeLogin {
    user: UserId,
    dir: PathBuf,
}

impl ClaudeLogin {
    /// The login of `user` under the data folder `data`, its folders made (0700) when missing.
    pub fn of(data: &Path, user: UserId) -> std::io::Result<Self> {
        let dir = ensure_dir(data, user)?;
        Ok(Self { user, dir })
    }

    /// The login of `user`, as a path only (nothing made): for reading what is there.
    pub fn at(data: &Path, user: UserId) -> Self {
        Self { user, dir: logins_root(data).join(user.to_string()) }
    }

    pub fn user(&self) -> UserId {
        self.user
    }

    /// The folder the child keeps its config and sessions in.
    pub fn config_dir(&self) -> &Path {
        &self.dir
    }
}

/// `<data>/logins`.
pub fn logins_root(data: &Path) -> PathBuf {
    data.join(LOGINS_DIR)
}

/// `<data>/logins/<user>`, checked to stay right under `logins/`.
pub fn login_dir(data: &Path, user: UserId) -> std::io::Result<PathBuf> {
    let root = logins_root(data);
    let dir = root.join(user.to_string());
    if !dir.starts_with(&root) || dir.parent() != Some(root.as_path()) {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "a login folder must be right under logins/"));
    }
    Ok(dir)
}

/// Make `<data>/logins/` and `<data>/logins/<user>/` with mode 0700 in one step
/// (never made first and narrowed after), or narrow them when they exist.
pub fn ensure_dir(data: &Path, user: UserId) -> std::io::Result<PathBuf> {
    let dir = login_dir(data, user)?;
    make_private(&logins_root(data))?;
    keep_out_of_searches(&logins_root(data));
    make_private(&dir)?;
    Ok(dir)
}

/// `logins/.ignore` with `*`: ripgrep (behind Claude Code's Grep and Glob) skips
/// everything in `logins/` when it searches a folder above it. One more layer
/// for SEC2 M1, never the one relied on (`deny_rules`); a failure is no error.
fn keep_out_of_searches(root: &Path) {
    let file = root.join(".ignore");
    if !file.exists() {
        let _ = std::fs::write(file, "*\n");
    }
}

fn make_private(dir: &Path) -> std::io::Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    match builder.create(dir) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && dir.is_dir() => {
            if !is_private(dir) {
                narrow(dir)?;
            }
            Ok(())
        }
        Err(e) => Err(e),
    }
}

/// Whether only the owner may enter `dir` (always true where there are no modes).
pub fn is_private(dir: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::symlink_metadata(dir).is_ok_and(|m| m.is_dir() && m.permissions().mode() & 0o077 == 0)
    }
    #[cfg(not(unix))]
    {
        dir.is_dir()
    }
}

fn narrow(dir: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
    }
    #[cfg(not(unix))]
    {
        let _ = dir;
        Ok(())
    }
}

/// At the daemon's start: `logins/` and every login folder in it are 0700.
/// Returns the folders whose mode had to be put right (to warn about them);
/// entries that are not a user's UUID are left alone and not named.
pub fn check_all(data: &Path) -> std::io::Result<Vec<PathBuf>> {
    let root = logins_root(data);
    let mut fixed = Vec::new();
    if !root.exists() {
        return Ok(fixed);
    }
    if !is_private(&root) {
        narrow(&root)?;
        fixed.push(root.clone());
    }
    for entry in std::fs::read_dir(&root)?.filter_map(Result::ok) {
        let path = entry.path();
        let is_user = path.file_name().and_then(|n| n.to_str()).and_then(user_of).is_some();
        if is_user && path.is_dir() && !is_private(&path) {
            narrow(&path)?;
            fixed.push(path);
        }
    }
    Ok(fixed)
}

/// Every login folder there is (`logins/<uuid>`), for what goes over all of
/// them (the sweep of Claude Code's sessions).
pub fn all(data: &Path) -> Vec<ClaudeLogin> {
    let root = logins_root(data);
    std::fs::read_dir(&root)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().to_str().and_then(user_of))
        .map(|user| ClaudeLogin::at(data, user))
        .collect()
}

/// The user a folder name stands for: only the UUID exactly as Aurelius writes it.
fn user_of(name: &str) -> Option<UserId> {
    UserId::parse(name).ok().filter(|u| u.to_string() == name)
}

/// Remove a login folder and everything in it (unlink, a disabled board
/// member), and check it is gone. Only a folder right under `logins/`.
pub fn remove(data: &Path, user: UserId) -> std::io::Result<()> {
    let dir = login_dir(data, user)?;
    match std::fs::remove_dir_all(&dir) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    if dir.exists() {
        return Err(std::io::Error::other("the login folder is still there"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("aurelius-logins-{name}-{}-{}", std::process::id(), UserId::new()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[cfg(unix)]
    fn mode(path: &Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn a_login_folder_is_the_users_uuid_right_under_logins() {
        let data = temp("path");
        let user = UserId::new();
        let login = ClaudeLogin::of(&data, user).unwrap();
        assert_eq!(login.config_dir(), data.join("logins").join(user.to_string()));
        assert!(login.config_dir().starts_with(logins_root(&data)));
        assert_eq!(login.user(), user);
        let _ = std::fs::remove_dir_all(data);
    }

    #[cfg(unix)]
    #[test]
    fn the_folders_are_made_0700_in_one_step_and_put_right_at_the_start() {
        let data = temp("mode");
        let user = UserId::new();
        // A process umask never widens it: the mode is given when the folder is made.
        let login = ClaudeLogin::of(&data, user).unwrap();
        assert_eq!((mode(&logins_root(&data)), mode(login.config_dir())), (0o700, 0o700));

        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(login.config_dir(), std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::set_permissions(logins_root(&data), std::fs::Permissions::from_mode(0o751)).unwrap();
        // Something that is no login is not touched.
        std::fs::create_dir(logins_root(&data).join("notes")).unwrap();
        std::fs::set_permissions(logins_root(&data).join("notes"), std::fs::Permissions::from_mode(0o755)).unwrap();
        let fixed = check_all(&data).unwrap();
        assert_eq!(fixed, vec![logins_root(&data), login.config_dir().to_path_buf()]);
        assert_eq!((mode(&logins_root(&data)), mode(login.config_dir())), (0o700, 0o700));
        assert_eq!(mode(&logins_root(&data).join("notes")), 0o755);
        assert!(check_all(&data).unwrap().is_empty(), "nothing left to put right");

        // A folder made anew by `of` that exists already with a wider mode is narrowed too.
        std::fs::set_permissions(login.config_dir(), std::fs::Permissions::from_mode(0o777)).unwrap();
        ClaudeLogin::of(&data, user).unwrap();
        assert_eq!(mode(login.config_dir()), 0o700);
        let _ = std::fs::remove_dir_all(data);
    }

    #[test]
    fn every_login_folder_is_listed_and_one_can_be_removed() {
        let data = temp("all");
        let (alex, robin) = (UserId::new(), UserId::new());
        let a = ClaudeLogin::of(&data, alex).unwrap();
        ClaudeLogin::of(&data, robin).unwrap();
        std::fs::write(a.config_dir().join(".claude.json"), "{}").unwrap();
        std::fs::create_dir(logins_root(&data).join("not-a-user")).unwrap();
        let mut users: Vec<UserId> = all(&data).iter().map(ClaudeLogin::user).collect();
        users.sort_by_key(|u| u.to_string());
        let mut expected = vec![alex, robin];
        expected.sort_by_key(|u| u.to_string());
        assert_eq!(users, expected);

        remove(&data, alex).unwrap();
        assert!(!a.config_dir().exists());
        assert!(ClaudeLogin::at(&data, robin).config_dir().exists(), "only that one");
        remove(&data, alex).unwrap();
        let _ = std::fs::remove_dir_all(data);
    }
}
