//! `query/markdown_files` and `query/markdown_file` (1.16): read-only access
//! to the Markdown files of a work folder, for the in-app viewer on a client
//! that is not on this computer. Every path is confined to the work folder:
//! `..` is refused and the real path (symlinks and junctions resolved) must
//! lie inside the real work folder.

use crate::Shared;
use aurelius_protocol::{
    ErrorCode, FileScope, MARKDOWN_MAX_BYTES, MARKDOWN_MAX_FILES, MarkdownFileContent, MarkdownFileEntry, MarkdownFileList, ProtocolError,
};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

/// Folders never listed, besides what `.gitignore` ignores.
const SKIPPED_DIRS: [&str; 4] = ["target", "node_modules", ".git", ".aurelius"];
/// Where files the user sent with a message are kept (see `message_uploads`).
const UPLOADS_DIR: &str = ".aurelius/uploads";
const MAX_DEPTH: usize = 12;

pub async fn list(shared: &Arc<Shared>, scope: FileScope) -> Result<MarkdownFileList, ProtocolError> {
    let roots = roots_of(shared, scope).await?;
    spawn(move || list_in(&roots)).await
}

pub async fn read(shared: &Arc<Shared>, scope: FileScope, path: String) -> Result<MarkdownFileContent, ProtocolError> {
    let roots = roots_of(shared, scope).await?;
    spawn(move || read_in(&roots, &path)).await
}

async fn spawn<T: Send + 'static>(f: impl FnOnce() -> Result<T, ProtocolError> + Send + 'static) -> Result<T, ProtocolError> {
    tokio::task::spawn_blocking(f).await.map_err(|e| ProtocolError::new(ErrorCode::Internal, e.to_string()))?
}

/// The real work folders of the scope that exist on this computer, in order.
async fn roots_of(shared: &Arc<Shared>, scope: FileScope) -> Result<Vec<PathBuf>, ProtocolError> {
    let store = shared.store().clone();
    let folders: Vec<String> = spawn(move || {
        store
            .read(|c| {
                use aurelius_storage::{conversations, tasks};
                Ok(match scope {
                    FileScope::Project { project_id } => tasks::work_folders(c, project_id, None)?,
                    // A ticket's discussion or a channel has no employee: it reads from its project's folder.
                    FileScope::Conversation { conversation_id } => match conversations::work_folders(c, conversation_id)? {
                        folders if folders.is_empty() => match conversations::project_id_of(c, conversation_id)? {
                            Some(project) => tasks::work_folders(c, project, None)?,
                            None => folders,
                        },
                        folders => folders,
                    },
                    FileScope::Task { task_id } => match tasks::task_conversation(c, task_id)? {
                        Some((id, _)) => conversations::work_folders(c, id)?,
                        None => vec![],
                    },
                })
            })
            .map_err(|e: aurelius_storage::StorageError| ProtocolError::new(ErrorCode::StorageUnavailable, e.to_string()))
    })
    .await?;
    Ok(folders.iter().filter_map(|f| std::fs::canonicalize(f).ok()).filter(|d| d.is_dir()).collect())
}

fn is_markdown(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("md") || e.eq_ignore_ascii_case("markdown"))
}

fn modified_ms(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn slashes(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Markdown files of the first work folder (`roots[0]`), `.gitignore` respected.
fn list_in(roots: &[PathBuf]) -> Result<MarkdownFileList, ProtocolError> {
    let Some(root) = roots.first() else { return Ok(MarkdownFileList::default()) };
    let mut files: Vec<MarkdownFileEntry> = Vec::new();
    let mut truncated = false;
    let walker = ignore::WalkBuilder::new(root)
        .hidden(false)
        .require_git(false)
        .follow_links(false)
        .max_depth(Some(MAX_DEPTH))
        .sort_by_file_name(|a, b| a.cmp(b))
        .filter_entry(|e| {
            let is_dir = e.file_type().is_some_and(|t| t.is_dir());
            !(is_dir && e.depth() > 0 && e.file_name().to_str().is_some_and(|n| SKIPPED_DIRS.contains(&n)))
        })
        .build();
    // false: the cap is reached.
    let push = |path: &Path, files: &mut Vec<MarkdownFileEntry>| -> bool {
        let Ok(meta) = std::fs::symlink_metadata(path) else { return true };
        if !meta.is_file() || !is_markdown(path) {
            return true;
        }
        if files.len() >= MARKDOWN_MAX_FILES {
            return false;
        }
        if let Ok(rel) = path.strip_prefix(root) {
            files.push(MarkdownFileEntry { path: slashes(rel), byte_length: meta.len(), modified_ms: modified_ms(&meta) });
        }
        true
    };
    for entry in walker.flatten() {
        if !push(entry.path(), &mut files) {
            truncated = true;
            break;
        }
    }
    // Uploads are kept out of git by their own .gitignore, but they are the user's files.
    if let Ok(entries) = std::fs::read_dir(root.join(UPLOADS_DIR)) {
        let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
        paths.sort();
        for path in paths {
            if !push(&path, &mut files) {
                truncated = true;
                break;
            }
        }
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(MarkdownFileList { root: display(root), files, truncated })
}

/// Paths as the user knows them: without Windows' `\\?\` prefix.
fn display(path: &Path) -> String {
    let s = path.to_string_lossy();
    s.strip_prefix(r"\\?\").unwrap_or(&s).to_owned()
}

fn refuse(code: ErrorCode, message: impl Into<String>) -> ProtocolError {
    ProtocolError::new(code, message)
}

/// The real file `input` names, inside one of the roots, and its path
/// relative to that root.
fn resolve(roots: &[PathBuf], input: &str) -> Result<(PathBuf, String), ProtocolError> {
    let input = input.trim();
    if input.is_empty() || input.contains('\0') {
        return Err(refuse(ErrorCode::Validation, "Give the path of a Markdown file."));
    }
    let given = Path::new(input);
    if given.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(refuse(ErrorCode::Forbidden, "A path with '..' is not allowed; it must stay inside the work folder."));
    }
    if roots.is_empty() {
        return Err(refuse(ErrorCode::NotFound, "There is no work folder on the server to read files from."));
    }
    let candidates: Vec<PathBuf> = if given.is_absolute() { vec![given.to_path_buf()] } else { roots.iter().map(|r| r.join(given)).collect() };
    let mut outside = false;
    for candidate in candidates {
        // canonicalize resolves symlinks and junctions: what is left is the real place.
        let Ok(real) = std::fs::canonicalize(&candidate) else { continue };
        match roots.iter().find(|r| real.starts_with(r)) {
            Some(root) => {
                let rel = real.strip_prefix(root).map(slashes).unwrap_or_default();
                return Ok((real, rel));
            }
            None => outside = true,
        }
    }
    if outside {
        Err(refuse(ErrorCode::Forbidden, "This file is outside the work folder."))
    } else {
        Err(refuse(ErrorCode::NotFound, "This file does not exist (anymore)."))
    }
}

/// The git worktrees registered in the roots' repositories (`.git/worktrees/*/gitdir`),
/// real paths that exist. Employees write in a worktree, so a file they name
/// often lives only there until their branch is merged.
fn worktrees_of(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = Vec::new();
    for root in roots {
        let Ok(entries) = std::fs::read_dir(root.join(".git").join("worktrees")) else { continue };
        for entry in entries.flatten() {
            let Ok(gitdir) = std::fs::read_to_string(entry.path().join("gitdir")) else { continue };
            // gitdir names the worktree's `.git` file; the worktree is its folder.
            let Some(tree) = Path::new(gitdir.trim()).parent() else { continue };
            if let Ok(real) = std::fs::canonicalize(tree)
                && real.is_dir()
                && !roots.contains(&real)
                && !found.contains(&real)
            {
                found.push(real);
            }
        }
    }
    found
}

/// Like `resolve`, but a file that is not in the roots is looked up in their
/// worktrees; when several have it, the most recently changed one wins.
fn resolve_with_worktrees(roots: &[PathBuf], input: &str) -> Result<(PathBuf, String), ProtocolError> {
    let first = resolve(roots, input);
    // Forbidden too: an absolute path into a worktree outside the root; each worktree checks it again.
    let missing = matches!(&first, Err(e) if matches!(e.code, ErrorCode::NotFound | ErrorCode::Forbidden)) && !roots.is_empty();
    if !missing {
        return first;
    }
    let newest = worktrees_of(roots)
        .into_iter()
        .filter_map(|tree| resolve(std::slice::from_ref(&tree), input).ok())
        .max_by_key(|(real, _)| std::fs::metadata(real).ok().and_then(|m| m.modified().ok()));
    newest.map_or(first, Ok)
}

fn read_in(roots: &[PathBuf], input: &str) -> Result<MarkdownFileContent, ProtocolError> {
    let (real, rel) = resolve_with_worktrees(roots, input)?;
    if !is_markdown(&real) {
        return Err(refuse(ErrorCode::Validation, "Only Markdown files (.md, .markdown) can be shown."));
    }
    let meta = std::fs::metadata(&real).map_err(|_| refuse(ErrorCode::NotFound, "This file does not exist (anymore)."))?;
    if !meta.is_file() {
        return Err(refuse(ErrorCode::Validation, "This is a folder, not a file."));
    }
    let too_large = |len: u64| {
        refuse(
            ErrorCode::Validation,
            format!("This file is too large to show ({:.1} MB; the limit is {} MB).", len as f64 / 1_048_576.0, MARKDOWN_MAX_BYTES / 1_048_576),
        )
    };
    if meta.len() > MARKDOWN_MAX_BYTES {
        return Err(too_large(meta.len()));
    }
    // Read one byte more than the limit: the file may have grown meanwhile.
    let mut bytes = Vec::new();
    {
        use std::io::Read;
        let file = std::fs::File::open(&real).map_err(|e| refuse(ErrorCode::Validation, format!("This file can't be read: {e}")))?;
        file.take(MARKDOWN_MAX_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| refuse(ErrorCode::Validation, format!("This file can't be read: {e}")))?;
    }
    if bytes.len() as u64 > MARKDOWN_MAX_BYTES {
        return Err(too_large(bytes.len() as u64));
    }
    let byte_length = bytes.len() as u64;
    let text = String::from_utf8(bytes).map_err(|_| refuse(ErrorCode::Validation, "This file is not UTF-8 text, so it can't be shown."))?;
    let text = text.strip_prefix('\u{feff}').map(str::to_owned).unwrap_or(text);
    Ok(MarkdownFileContent { path: rel, text, byte_length, modified_ms: modified_ms(&meta) })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(dir: &tempfile::TempDir) -> Vec<PathBuf> {
        vec![std::fs::canonicalize(dir.path()).unwrap()]
    }

    fn write(dir: &tempfile::TempDir, rel: &str, body: &[u8]) {
        let p = dir.path().join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }

    #[test]
    fn lists_markdown_but_skips_ignored_and_build_folders() {
        let d = tempfile::tempdir().unwrap();
        write(&d, "README.md", b"# hi");
        write(&d, "docs/a.markdown", b"a");
        write(&d, "docs/b.txt", b"b");
        write(&d, "target/x.md", b"x");
        write(&d, "node_modules/p/y.md", b"y");
        write(&d, ".git/z.md", b"z");
        write(&d, "private/secret.md", b"s");
        write(&d, ".gitignore", b"private/\n");
        write(&d, ".aurelius/uploads/note.md", b"n");
        let list = list_in(&root(&d)).unwrap();
        let paths: Vec<&str> = list.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, vec![".aurelius/uploads/note.md", "README.md", "docs/a.markdown"]);
        assert!(!list.truncated);
        assert_eq!(list.files[1].byte_length, 4);
    }

    #[test]
    fn the_list_is_capped() {
        let d = tempfile::tempdir().unwrap();
        for i in 0..MARKDOWN_MAX_FILES + 5 {
            write(&d, &format!("n{i:04}.md"), b"x");
        }
        let list = list_in(&root(&d)).unwrap();
        assert_eq!(list.files.len(), MARKDOWN_MAX_FILES);
        assert!(list.truncated);
    }

    #[test]
    fn reads_by_relative_and_absolute_path() {
        let d = tempfile::tempdir().unwrap();
        write(&d, "docs/a.md", "héllo".as_bytes());
        let roots = root(&d);
        let rel = read_in(&roots, "docs/a.md").unwrap();
        assert_eq!((rel.path.as_str(), rel.text.as_str(), rel.byte_length), ("docs/a.md", "héllo", 6));
        let abs = read_in(&roots, &d.path().join("docs").join("a.md").to_string_lossy()).unwrap();
        assert_eq!(abs.path, "docs/a.md");
        write(&d, ".aurelius/uploads/u.md", b"up");
        assert_eq!(read_in(&roots, ".aurelius/uploads/u.md").unwrap().text, "up");
    }

    #[test]
    fn refuses_parent_dirs_and_outside_paths() {
        let d = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        write(&d, "ok.md", b"ok");
        write(&other, "out.md", b"out");
        let roots = root(&d);
        assert_eq!(read_in(&roots, "../x.md").unwrap_err().code, ErrorCode::Forbidden);
        assert_eq!(read_in(&roots, "docs/../../x.md").unwrap_err().code, ErrorCode::Forbidden);
        let outside = other.path().join("out.md");
        assert_eq!(read_in(&roots, &outside.to_string_lossy()).unwrap_err().code, ErrorCode::Forbidden);
        assert_eq!(read_in(&roots, "missing.md").unwrap_err().code, ErrorCode::NotFound);
        assert_eq!(read_in(&[], "ok.md").unwrap_err().code, ErrorCode::NotFound);
    }

    /// Registers `tree` as a git worktree of `repo`, the way git does.
    fn add_worktree(repo: &tempfile::TempDir, name: &str, tree: &Path) {
        write(repo, &format!(".git/worktrees/{name}/gitdir"), format!("{}\n", tree.join(".git").display()).as_bytes());
        std::fs::write(tree.join(".git"), b"gitdir: elsewhere").unwrap();
    }

    #[test]
    fn a_file_only_in_a_worktree_is_read_from_the_newest_worktree() {
        let d = tempfile::tempdir().unwrap();
        let away = tempfile::tempdir().unwrap();
        write(&d, "target/wt-a/docs/design.md", b"old");
        add_worktree(&d, "wt-a", &d.path().join("target/wt-a"));
        std::thread::sleep(std::time::Duration::from_millis(20));
        write(&away, "docs/design.md", b"new");
        add_worktree(&d, "away", away.path());
        let roots = root(&d);
        let got = read_in(&roots, "docs/design.md").unwrap();
        assert_eq!((got.text.as_str(), got.path.as_str()), ("new", "docs/design.md"));
        let abs = away.path().join("docs/design.md");
        assert_eq!(read_in(&roots, &abs.to_string_lossy()).unwrap().text, "new");
    }

    #[test]
    fn the_root_wins_over_a_worktree_and_worktrees_stay_confined() {
        let d = tempfile::tempdir().unwrap();
        let away = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        write(&d, "docs/design.md", b"main");
        write(&away, "docs/design.md", b"branch");
        write(&other, "out.md", b"out");
        add_worktree(&d, "away", away.path());
        let roots = root(&d);
        assert_eq!(read_in(&roots, "docs/design.md").unwrap().text, "main");
        assert_eq!(read_in(&roots, "../x.md").unwrap_err().code, ErrorCode::Forbidden);
        assert_eq!(read_in(&roots, &other.path().join("out.md").to_string_lossy()).unwrap_err().code, ErrorCode::Forbidden);
        assert_eq!(read_in(&roots, "nowhere.md").unwrap_err().code, ErrorCode::NotFound);
    }

    #[cfg(unix)]
    #[test]
    fn refuses_a_symlink_that_leaves_the_folder() {
        let d = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        write(&other, "out.md", b"out");
        std::os::unix::fs::symlink(other.path().join("out.md"), d.path().join("link.md")).unwrap();
        std::os::unix::fs::symlink(other.path(), d.path().join("dir")).unwrap();
        let roots = root(&d);
        assert_eq!(read_in(&roots, "link.md").unwrap_err().code, ErrorCode::Forbidden);
        assert_eq!(read_in(&roots, "dir/out.md").unwrap_err().code, ErrorCode::Forbidden);
    }

    #[test]
    fn refuses_other_types_large_and_non_utf8_files() {
        let d = tempfile::tempdir().unwrap();
        write(&d, "a.txt", b"t");
        write(&d, "bin.md", &[0xff, 0xfe, 0x00, 0x80]);
        write(&d, "big.md", &vec![b'a'; MARKDOWN_MAX_BYTES as usize + 1]);
        write(&d, "edge.md", &vec![b'a'; MARKDOWN_MAX_BYTES as usize]);
        std::fs::create_dir_all(d.path().join("dir.md")).unwrap();
        let roots = root(&d);
        let msg = |p: &str| read_in(&roots, p).unwrap_err().message;
        assert!(msg("a.txt").contains("Only Markdown"));
        assert!(msg("bin.md").contains("not UTF-8"));
        assert!(msg("big.md").contains("too large"));
        assert!(msg("dir.md").contains("folder"));
        assert_eq!(read_in(&roots, "edge.md").unwrap().byte_length, MARKDOWN_MAX_BYTES);
    }
}
