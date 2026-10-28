//! Claude Code's own session (its transcript under `~/.claude`), named after
//! a run id: the conversation's session it goes on in with `claude -p --resume
//! <id>`, or a run's own, kept to carry on after a usage limit (CLI 2.1.289).

use std::path::{Path, PathBuf};

/// Where Claude Code keeps its sessions. The child gets no `CLAUDE_CONFIG_DIR`,
/// so it is `~/.claude`.
pub fn claude_dir() -> Option<PathBuf> {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
    Some(PathBuf::from(home).join(".claude"))
}

/// The transcript of session `id` (`projects/<folder>/<id>.jsonl`), if Claude Code still has it.
pub fn transcript(claude_dir: &Path, id: &str) -> Option<PathBuf> {
    let name = format!("{id}.jsonl");
    std::fs::read_dir(claude_dir.join("projects"))
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path().join(&name))
        .find(|path| path.metadata().is_ok_and(|m| m.is_file() && m.len() > 0))
}

/// Where Claude Code keeps a session's scratch files (`<folder>/<id>/images`,
/// `scratchpad`, `tasks`): `claude` in the temp folder.
pub fn temp_dir() -> PathBuf {
    std::env::temp_dir().join("claude")
}

/// Remove what Claude Code kept of session `id`: its transcript, its subagents'
/// transcripts, its per-session state and its scratch files (image copies). Best effort.
pub fn remove(claude_dir: &Path, id: &str) {
    remove_in(claude_dir, &temp_dir(), id);
}

/// `remove` with the temp folder given. A folder left empty by what went of this
/// session goes too; folders it had nothing in are never touched.
pub fn remove_in(claude_dir: &Path, temp_dir: &Path, id: &str) {
    for folder in subfolders(&claude_dir.join("projects")) {
        let file = std::fs::remove_file(folder.join(format!("{id}.jsonl"))).is_ok();
        let dir = std::fs::remove_dir_all(folder.join(id)).is_ok();
        if file || dir {
            let _ = std::fs::remove_dir(&folder);
        }
    }
    for state in ["session-env", "file-history"] {
        let _ = std::fs::remove_dir_all(claude_dir.join(state).join(id));
    }
    for folder in subfolders(temp_dir) {
        if std::fs::remove_dir_all(folder.join(id)).is_ok() {
            let _ = std::fs::remove_dir(&folder);
        }
    }
}

fn subfolders(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir).into_iter().flatten().filter_map(Result::ok).map(|entry| entry.path()).filter(|p| p.is_dir()).collect()
}

/// The ids of every session Claude Code keeps something of, whoever started it.
/// Only names that are run ids (UUIDs) are listed; the caller decides which are its own.
pub fn ids(claude_dir: &Path) -> std::collections::BTreeSet<String> {
    let names = |dir: PathBuf| std::fs::read_dir(dir).into_iter().flatten().filter_map(Result::ok).map(|entry| entry.path());
    let mut found = std::collections::BTreeSet::new();
    for folder in names(claude_dir.join("projects")).filter(|p| p.is_dir()) {
        for path in names(folder) {
            let name = match path.extension() {
                Some(ext) if ext == "jsonl" => path.file_stem(),
                None if path.is_dir() => path.file_name(),
                _ => None,
            };
            found.extend(name.and_then(|n| n.to_str()).map(str::to_owned));
        }
    }
    for state in ["session-env", "file-history"] {
        found.extend(names(claude_dir.join(state)).filter_map(|p| p.file_name().and_then(|n| n.to_str()).map(str::to_owned)));
    }
    found.retain(|id| aurelius_domain::RunId::parse(id).is_ok());
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_and_removes_a_session_in_any_project_folder() {
        let dir = std::env::temp_dir().join(format!("aurelius-claude-session-{}", std::process::id()));
        let id = "0199c3a2-0000-7000-8000-000000000001";
        let folder = dir.join("projects").join("C--work-project");
        std::fs::create_dir_all(folder.join(id).join("subagents")).unwrap();
        std::fs::create_dir_all(dir.join("session-env").join(id)).unwrap();
        assert_eq!(transcript(&dir, id), None, "no transcript yet");
        std::fs::write(folder.join(format!("{id}.jsonl")), "").unwrap();
        assert_eq!(transcript(&dir, id), None, "an empty transcript cannot be resumed");
        std::fs::write(folder.join(format!("{id}.jsonl")), "{}\n").unwrap();
        assert_eq!(transcript(&dir, id), Some(folder.join(format!("{id}.jsonl"))));
        let other = "0199c3a2-0000-7000-8000-000000000002";
        std::fs::create_dir_all(dir.join("file-history").join(other)).unwrap();
        std::fs::write(folder.join("notes.jsonl"), "{}\n").unwrap();
        assert_eq!(ids(&dir).into_iter().collect::<Vec<_>>(), [id, other], "only session ids, each once");

        remove(&dir, id);
        assert_eq!(transcript(&dir, id), None);
        assert!(!folder.join(id).exists() && !dir.join("session-env").join(id).exists());
        assert!(folder.exists(), "the project folder itself stays");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn removes_the_image_copies_and_the_folders_it_leaves_empty() {
        let root = std::env::temp_dir().join(format!("aurelius-claude-temp-{}", std::process::id()));
        let (claude, temp) = (root.join(".claude"), root.join("tmp-claude"));
        let id = "0199c3a2-0000-7000-8000-000000000001";
        let other = "0199c3a2-0000-7000-8000-000000000002";
        // Only this session was in C--a; C--b also has another's; C--c was empty already.
        for f in ["C--a", "C--b"] {
            std::fs::create_dir_all(claude.join("projects").join(f)).unwrap();
            std::fs::write(claude.join("projects").join(f).join(format!("{id}.jsonl")), "{}\n").unwrap();
            std::fs::create_dir_all(temp.join(f).join(id).join("images")).unwrap();
            std::fs::write(temp.join(f).join(id).join("images").join("1.png"), "png").unwrap();
        }
        std::fs::write(claude.join("projects").join("C--b").join(format!("{other}.jsonl")), "{}\n").unwrap();
        std::fs::create_dir_all(temp.join("C--b").join(other).join("images")).unwrap();
        std::fs::create_dir_all(claude.join("projects").join("C--c")).unwrap();
        std::fs::create_dir_all(temp.join("C--c")).unwrap();

        remove_in(&claude, &temp, id);
        assert!(!claude.join("projects").join("C--a").exists() && !temp.join("C--a").exists(), "left empty: gone");
        assert!(!temp.join("C--b").join(id).exists());
        assert!(claude.join("projects").join("C--b").join(format!("{other}.jsonl")).exists(), "another session's transcript stays");
        assert!(temp.join("C--b").join(other).join("images").exists(), "another session's images stay");
        assert!(claude.join("projects").join("C--c").exists() && temp.join("C--c").exists(), "empty folders it had nothing in stay");
        let _ = std::fs::remove_dir_all(root);
    }
}
