//! Throwaway git worktrees for task variants (Harness² C): one per trial, made
//! from the work folder's HEAD, its whole change taken as a patch, applied to
//! the real work folder when chosen, and removed. Every call runs the installed
//! `git` with a typed argument list (no shell).

use crate::git::GitError;
use std::path::{Path, PathBuf};
use std::process::Command;

fn git(dir: &Path) -> Command {
    let mut command = Command::new("git");
    command.arg("-C").arg(dir);
    command.env("GIT_TERMINAL_PROMPT", "0").env("GIT_EDITOR", "true").env("GIT_MERGE_AUTOEDIT", "no");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    command
}

fn run(mut command: Command) -> Result<String, GitError> {
    let output = command.output().map_err(|e| GitError(format!("could not start git ({e}); is Git installed?")))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        let err = String::from_utf8_lossy(&output.stderr);
        let out = String::from_utf8_lossy(&output.stdout);
        Err(GitError(err.lines().chain(out.lines()).map(str::trim).find(|l| !l.is_empty()).unwrap_or("failed").to_owned()))
    }
}

/// Where trial `n` of a task lives: `<repo>/target/wt-probe-<task>-<n>`, the
/// worktree convention of the repository (each with its own `target` inside).
pub fn probe_path(repo: &Path, task: &str, n: u8) -> PathBuf {
    let short: String = task.chars().filter(|c| c.is_ascii_alphanumeric()).take(8).collect();
    repo.join("target").join(format!("wt-probe-{short}-{n}"))
}

/// A new detached worktree at `path` from `base`. One left there by an earlier
/// attempt is removed first.
pub fn add(repo: &Path, path: &Path, base: &str) -> Result<(), GitError> {
    if path.exists() {
        remove(repo, path)?;
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| GitError(format!("could not make {}: {e}", parent.display())))?;
    }
    let mut c = git(repo);
    c.args(["worktree", "add", "--detach"]).arg(path).arg(base);
    run(c).map(|_| ())
}

/// The whole change in the worktree against its HEAD (new files included, its
/// own `target` left out) as a binary-safe patch, and its size (files, added, removed).
pub fn change(path: &Path) -> Result<(String, (u32, u32, u32)), GitError> {
    let mut add = git(path);
    add.args(["add", "-A", "--", ".", ":(exclude)target"]);
    run(add)?;
    let mut stat = git(path);
    stat.args(["diff", "--cached", "--numstat", "HEAD"]);
    let mut size = (0, 0, 0);
    for line in run(stat)?.lines() {
        let mut parts = line.split('\t');
        let (Some(a), Some(d)) = (parts.next(), parts.next()) else { continue };
        size.0 += 1;
        size.1 += a.parse::<u32>().unwrap_or(0);
        size.2 += d.parse::<u32>().unwrap_or(0);
    }
    let mut diff = git(path);
    diff.args(["diff", "--cached", "--binary", "--no-color", "--no-ext-diff", "HEAD"]);
    Ok((run(diff)?, size))
}

/// Apply a patch from `change` to the real work folder (its working tree, not
/// committed). Nothing happens for an empty patch; a patch that does not apply
/// changes nothing.
pub fn apply(repo: &Path, patch: &str) -> Result<(), GitError> {
    if patch.trim().is_empty() {
        return Ok(());
    }
    let file = std::env::temp_dir().join(format!("aurelius-variant-{}.patch", std::process::id() as u64 * 1_000_003 + patch.len() as u64));
    std::fs::write(&file, patch).map_err(|e| GitError(format!("could not write the patch: {e}")))?;
    let mut check = git(repo);
    check.args(["apply", "--check", "--whitespace=nowarn"]).arg(&file);
    let result = run(check).and_then(|_| {
        let mut c = git(repo);
        c.args(["apply", "--whitespace=nowarn"]).arg(&file);
        run(c).map(|_| ())
    });
    let _ = std::fs::remove_file(&file);
    result
}

/// Remove a trial's worktree (its folder too, also when git no longer knows it).
pub fn remove(repo: &Path, path: &Path) -> Result<(), GitError> {
    let mut c = git(repo);
    c.args(["worktree", "remove", "--force"]).arg(path);
    let removed = run(c);
    if path.exists() {
        std::fs::remove_dir_all(path).map_err(|e| GitError(format!("could not remove {}: {e}", path.display())))?;
    }
    let mut prune = git(repo);
    prune.args(["worktree", "prune"]);
    let _ = run(prune);
    match removed {
        Err(e) if path.exists() => Err(e),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init(dir: &Path) {
        std::fs::write(dir.join("readme.md"), "hello\n").unwrap();
        let steps: [&[&str]; 6] = [
            &["init", "--quiet", "--initial-branch=main"],
            &["config", "user.name", "Test"],
            &["config", "user.email", "t@example.com"],
            &["config", "core.autocrlf", "false"],
            &["add", "-A"],
            &["commit", "--quiet", "-m", "initial"],
        ];
        for args in steps {
            let mut c = git(dir);
            c.args(args);
            run(c).unwrap();
        }
    }

    #[test]
    fn a_trial_worktree_gives_its_change_as_a_patch_that_applies_and_goes_away() {
        let repo = tempfile::tempdir().unwrap();
        init(repo.path());
        let head = crate::git::head(repo.path()).unwrap();
        let wt = probe_path(repo.path(), "0f1e2d3c-aaaa", 2);
        assert_eq!(wt, repo.path().join("target").join("wt-probe-0f1e2d3c-2"));
        add(repo.path(), &wt, &head).unwrap();
        std::fs::write(wt.join("readme.md"), "hello\nworld\n").unwrap();
        std::fs::write(wt.join("new.txt"), "new\n").unwrap();
        std::fs::create_dir_all(wt.join("target")).unwrap();
        std::fs::write(wt.join("target").join("junk.bin"), "x").unwrap();
        let (patch, size) = change(&wt).unwrap();
        assert_eq!(size, (2, 2, 0), "the build folder is left out");
        assert!(patch.contains("new.txt") && !patch.contains("junk.bin"), "{patch}");
        assert_eq!(std::fs::read_to_string(repo.path().join("readme.md")).unwrap(), "hello\n", "the real folder is untouched by the trial");
        apply(repo.path(), &patch).unwrap();
        assert_eq!(std::fs::read_to_string(repo.path().join("readme.md")).unwrap().replace("\r\n", "\n"), "hello\nworld\n");
        assert!(repo.path().join("new.txt").exists());
        assert!(apply(repo.path(), &patch).is_err(), "a patch that no longer applies changes nothing");
        remove(repo.path(), &wt).unwrap();
        assert!(!wt.exists());
        apply(repo.path(), "").unwrap();
    }
}
