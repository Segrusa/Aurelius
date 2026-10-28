//! Files an agent mentions in what it says (a colleague's result, a report, an
//! answer) become part of that message: kept in Aurelius, so every client can
//! open them, also a phone or another computer. Only existing files with a
//! full path inside the project's folders (the project, its employees' own
//! folders, task worktrees, the conversations' own work folders); never
//! anything else on the computer.

use aurelius_domain::views::FileRef;
use aurelius_storage::{StorageError, Store, Tx, blobs};
use std::path::{Path, PathBuf};

/// Largest file kept with a message.
pub(crate) const MAX_BYTES: u64 = aurelius_domain::conversation::UPLOAD_MAX_BYTES;
/// Most files per message.
const MAX_FILES: usize = 8;
/// Longest path looked for.
const MAX_PATH: usize = 400;

/// Full paths in a text: Windows (`C:\…`, `C:/…`) and absolute Unix paths.
/// Paths may contain spaces; they end at a line break, a quote, a backtick or
/// a bracket, and trailing punctuation is dropped.
pub fn paths_in(text: &str) -> Vec<PathBuf> {
    let stop = |c: char| matches!(c, '\n' | '\r' | '`' | '"' | '\'' | '<' | '>' | '(' | ')' | '[' | ']' | '*' | '|');
    let mut out: Vec<PathBuf> = Vec::new();
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut i = 0;
    while i < chars.len() {
        let (at, ch) = chars[i];
        let windows = ch.is_ascii_alphabetic()
            && chars.get(i + 1).is_some_and(|(_, c)| *c == ':')
            && chars.get(i + 2).is_some_and(|(_, c)| *c == '\\' || *c == '/')
            && (i == 0 || !chars[i - 1].1.is_alphanumeric());
        let unix = ch == '/' && (i == 0 || chars[i - 1].1.is_whitespace() || stop(chars[i - 1].1)) && chars.get(i + 1).is_some_and(|(_, c)| c.is_alphanumeric() || *c == '.');
        if !(windows || unix) {
            i += 1;
            continue;
        }
        let mut end = at;
        let mut j = i;
        while j < chars.len() && !stop(chars[j].1) && chars[j].0 - at < MAX_PATH {
            end = chars[j].0 + chars[j].1.len_utf8();
            j += 1;
        }
        let candidate = text[at..end].trim_end_matches(|c: char| c.is_whitespace() || matches!(c, '.' | ',' | ';' | ':' | '!' | '?'));
        // The longest existing file within the span (paths may be followed by words).
        let mut found = None;
        let mut cut = candidate;
        while !cut.is_empty() {
            let p = Path::new(cut);
            if p.is_file() {
                found = Some(p.to_path_buf());
                break;
            }
            match cut.rfind(char::is_whitespace) {
                Some(k) => cut = cut[..k].trim_end_matches(|c: char| c.is_whitespace() || matches!(c, '.' | ',' | ';' | ':' | '!' | '?')),
                None => break,
            }
        }
        if let Some(p) = found {
            if !out.contains(&p) {
                out.push(p);
            }
        }
        i = j.max(i + 1);
    }
    out
}

/// What kind of file, by its extension (shown and opened accordingly).
pub fn media_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("bmp") => "image/bmp",
        Some("svg") => "image/svg+xml",
        Some("pdf") => "application/pdf",
        Some("md" | "markdown") => "text/markdown",
        Some("txt" | "log") => "text/plain",
        Some("csv") => "text/csv",
        Some("json") => "application/json",
        Some("html" | "htm") => "text/html",
        Some("zip") => "application/zip",
        Some("docx") => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        Some("xlsx") => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        Some("pptx") => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        _ => "application/octet-stream",
    }
}

/// Folders whose files may be kept with a message of this project: the
/// project folder, the employees' own folders (any of them may work in it) and
/// the task worktrees.
pub fn allowed_roots(conn: &rusqlite::Connection, project: aurelius_domain::ProjectId, data_root: &Path) -> Result<Vec<PathBuf>, StorageError> {
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(folder) = aurelius_storage::files::project_folder(conn, project)? {
        roots.push(PathBuf::from(folder));
    }
    let mut stmt = conn.prepare("SELECT folder FROM employees WHERE folder IS NOT NULL AND status <> 'archived'")?;
    for folder in stmt.query_map([], |r| r.get::<_, String>(0))? {
        roots.push(PathBuf::from(folder?));
    }
    roots.push(data_root.join("worktrees"));
    // Conversations without a folder work in one of their own.
    roots.push(data_root.join("workspaces"));
    Ok(roots.into_iter().filter_map(|r| r.canonicalize().ok()).collect())
}

/// Keep the files a text mentions (inside `roots`) as blobs; their references.
pub fn keep(tx: &Tx<'_>, store: &Store, text: &str, roots: &[PathBuf]) -> Result<Vec<FileRef>, StorageError> {
    let mut out = Vec::new();
    for path in paths_in(text) {
        if out.len() >= MAX_FILES {
            break;
        }
        let Ok(real) = path.canonicalize() else { continue };
        if !roots.iter().any(|r| real.starts_with(r)) {
            continue;
        }
        let Ok(meta) = std::fs::metadata(&real) else { continue };
        if !meta.is_file() || meta.len() > MAX_BYTES {
            continue;
        }
        let Ok(bytes) = std::fs::read(&real) else { continue };
        let media = media_type(&real);
        let blob = store.blobs().put(&bytes, media)?;
        blobs::register(tx, &blob)?;
        let name = real.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "file".into());
        out.push(FileRef { id: blob.id.to_string(), name, media_type: media.into(), size: bytes.len() as u64 });
    }
    Ok(out)
}

/// The files a text mentions, for a message of this project.
pub fn keep_for_project(tx: &Tx<'_>, store: &Store, text: &str, project: aurelius_domain::ProjectId) -> Result<Vec<FileRef>, StorageError> {
    if paths_in(text).is_empty() {
        return Ok(Vec::new());
    }
    let roots = allowed_roots(tx, project, store.root())?;
    keep(tx, store, text, &roots)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_paths_of_existing_files_are_found_in_prose() {
        let dir = tempfile::tempdir().unwrap();
        let shot = dir.path().join("scherm 1_2026.png");
        std::fs::write(&shot, b"png").unwrap();
        let report = dir.path().join("report.md");
        std::fs::write(&report, b"# r").unwrap();
        let text = format!(
            "Saved as {}. Also see `{}` and C:\\nope\\missing.png, plus relative src/main.rs.",
            shot.display(),
            report.display()
        );
        let found = paths_in(&text);
        assert_eq!(found, vec![shot.clone(), report.clone()], "{text}");
        assert_eq!(media_type(&shot), "image/png");
        assert_eq!(media_type(&report), "text/markdown");
    }
}
