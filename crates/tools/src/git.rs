//! Read-only Git helpers: the branch, HEAD and the uncommitted change of a
//! work folder, for the diff view. Every call runs the installed `git` with a
//! typed argument list (no shell). Nothing here writes to a repository.

use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("git: {0}")]
pub struct GitError(pub String);

fn git(dir: &Path) -> Command {
    let mut command = Command::new("git");
    command.arg("-C").arg(dir);
    // Never wait for a terminal prompt (credentials, editors) in the background.
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
        Ok(String::from_utf8_lossy(&output.stdout).trim_end().to_owned())
    } else {
        let err = String::from_utf8_lossy(&output.stderr);
        let out = String::from_utf8_lossy(&output.stdout);
        let detail = err.lines().chain(out.lines()).map(str::trim).find(|l| !l.is_empty()).unwrap_or("failed").to_owned();
        Err(GitError(detail))
    }
}

/// The checked-out branch, or `None` when HEAD is detached.
pub fn current_branch(dir: &Path) -> Result<Option<String>, GitError> {
    let mut c = git(dir);
    c.args(["symbolic-ref", "--quiet", "--short", "HEAD"]);
    Ok(run(c).ok().filter(|b| !b.is_empty()))
}

/// Full commit id of HEAD.
pub fn head(dir: &Path) -> Result<String, GitError> {
    let mut c = git(dir);
    c.args(["rev-parse", "--verify", "--quiet", "HEAD^{commit}"]);
    run(c)
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiffStat {
    pub files: u32,
    pub insertions: u32,
    pub deletions: u32,
}

/// Size of the uncommitted change in the working tree (staged and unstaged,
/// against HEAD); untracked files only count as files.
pub fn working_stat(dir: &Path) -> Result<DiffStat, GitError> {
    let mut c = git(dir);
    c.args(["diff", "--numstat", "HEAD"]);
    let mut stat = numstat(&run(c)?);
    stat.files += untracked(dir)?.len() as u32;
    Ok(stat)
}

fn numstat(out: &str) -> DiffStat {
    let mut stat = DiffStat::default();
    for line in out.lines() {
        let mut parts = line.split('\t');
        let (Some(add), Some(del)) = (parts.next(), parts.next()) else { continue };
        stat.files += 1;
        stat.insertions += add.parse::<u32>().unwrap_or(0);
        stat.deletions += del.parse::<u32>().unwrap_or(0);
    }
    stat
}

fn untracked(dir: &Path) -> Result<Vec<String>, GitError> {
    let mut c = git(dir);
    c.args(["ls-files", "--others", "--exclude-standard"]);
    Ok(run(c)?.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect())
}

/// The uncommitted change in the working tree (staged and unstaged, against
/// HEAD) as a unified diff cut at `max_bytes`, whether it was cut, and the untracked files.
pub fn working_diff(dir: &Path, max_bytes: usize) -> Result<(String, bool, Vec<String>), GitError> {
    let mut c = git(dir);
    c.args(["diff", "--patch", "--no-color", "--no-ext-diff", "HEAD"]);
    let mut out = run(c)?;
    let truncated = truncate(&mut out, max_bytes);
    Ok((out, truncated, untracked(dir)?))
}

fn truncate(out: &mut String, max_bytes: usize) -> bool {
    if out.len() <= max_bytes {
        return false;
    }
    let mut cut = max_bytes;
    while !out.is_char_boundary(cut) {
        cut -= 1;
    }
    out.truncate(cut);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init(dir: &Path) {
        let steps: [&[&str]; 5] = [
            &["init", "--quiet", "--initial-branch=main"],
            &["config", "user.name", "Test"],
            &["config", "user.email", "t@example.com"],
            &["add", "-A"],
            &["commit", "--quiet", "-m", "initial"],
        ];
        std::fs::write(dir.join("readme.md"), "hello\n").unwrap();
        for args in steps {
            let mut c = git(dir);
            c.args(args);
            run(c).unwrap();
        }
    }

    #[test]
    fn working_stat_counts_staged_unstaged_and_untracked_changes() {
        let repo = tempfile::tempdir().unwrap();
        init(repo.path());
        assert_eq!(working_stat(repo.path()).unwrap(), DiffStat::default());
        std::fs::write(repo.path().join("readme.md"), "hi\nthere\n").unwrap();
        std::fs::write(repo.path().join("new.txt"), "x\n").unwrap();
        assert_eq!(working_stat(repo.path()).unwrap(), DiffStat { files: 2, insertions: 2, deletions: 1 });
        let mut c = git(repo.path());
        c.args(["add", "readme.md"]);
        run(c).unwrap();
        assert_eq!(working_stat(repo.path()).unwrap(), DiffStat { files: 2, insertions: 2, deletions: 1 }, "staged still counts");
        assert_eq!(current_branch(repo.path()).unwrap().as_deref(), Some("main"));
        assert_eq!(head(repo.path()).unwrap().len(), 40);
        let (patch, truncated, untracked) = working_diff(repo.path(), 1 << 20).unwrap();
        assert!(patch.contains("+there") && patch.contains("readme.md"), "{patch}");
        assert!(!truncated);
        assert_eq!(untracked, vec!["new.txt".to_string()]);
        let (patch, truncated, _) = working_diff(repo.path(), 10).unwrap();
        assert!(truncated && patch.len() <= 10);
    }
}
