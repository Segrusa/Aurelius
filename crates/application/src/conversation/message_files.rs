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
        let title = if media == "text/html" { html_title(&bytes) } else { None };
        out.push(FileRef { id: blob.id.to_string(), name, media_type: media.into(), size: bytes.len() as u64, title });
    }
    Ok(out)
}

/// How far into an HTML file its `<title>` is looked for.
const TITLE_SCAN_BYTES: usize = 64 * 1024;

/// The text of an HTML page's `<title>` in its head: entities decoded, white space
/// collapsed, at most `FILE_TITLE_MAX` characters. None without a title or when it is empty.
pub fn html_title(bytes: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(&bytes[..bytes.len().min(TITLE_SCAN_BYTES)]);
    let lower = text.to_ascii_lowercase();
    // Only the head: an inline SVG in the body has <title> elements of its own.
    let head = lower.find("</head").unwrap_or(lower.len());
    let mut from = 0;
    let open = loop {
        let at = from + lower[from..head].find("<title")?;
        let next = lower[at + 6..].chars().next()?;
        if next == '>' || next.is_ascii_whitespace() {
            break at;
        }
        from = at + 6;
    };
    let start = open + lower[open..].find('>')? + 1;
    let end = start + lower[start..].find("</title")?;
    let decoded = text[start..end]
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#039;", "'")
        .replace("&nbsp;", " ")
        .replace("&amp;", "&");
    let title = decoded.split_whitespace().collect::<Vec<_>>().join(" ");
    if title.is_empty() {
        return None;
    }
    let max = aurelius_domain::views::FILE_TITLE_MAX;
    Some(if title.chars().count() > max { format!("{}…", title.chars().take(max - 1).collect::<String>()) } else { title })
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

    #[test]
    fn an_html_page_s_title_is_read_from_its_head() {
        let page = b"<!doctype html><html><head><meta charset=utf-8>\n<TITLE lang=en>\n  Pricing &amp; plans\n  &#8212; v2 </TITLE></head><body></body></html>";
        assert_eq!(html_title(page).as_deref(), Some("Pricing & plans &#8212; v2"));
    }

    #[test]
    fn a_page_without_a_head_title_has_none() {
        assert_eq!(html_title(b"<html><head></head><body><svg><title>Icon</title></svg></body></html>"), None);
        assert_eq!(html_title(b"<html><head><titlebar>x</titlebar><title>   </title></head></html>"), None);
        assert_eq!(html_title(b"<p>no html at all"), None);
        assert_eq!(html_title(b"<title>never closed"), None);
    }

    #[test]
    fn a_long_title_is_cut_to_the_maximum() {
        let page = format!("<title>{}</title>", "é".repeat(300));
        let title = html_title(page.as_bytes()).unwrap();
        assert_eq!(title.chars().count(), aurelius_domain::views::FILE_TITLE_MAX);
        assert!(title.ends_with('…'));
    }

    #[test]
    fn a_kept_design_carries_its_title_and_other_files_none() {
        let dir = tempfile::tempdir().unwrap();
        let store = std::sync::Arc::new(Store::open(&dir.path().join("data")).unwrap());
        let work = dir.path().join("work");
        std::fs::create_dir_all(&work).unwrap();
        let page = work.join("landing-page.html");
        std::fs::write(&page, "<html><head><title>Landing page</title></head></html>").unwrap();
        let notes = work.join("notes.md");
        std::fs::write(&notes, "<title>Not a page</title>").unwrap();
        let roots = vec![work.canonicalize().unwrap()];
        let text = format!("Made {}\nand {}", page.display(), notes.display());
        let kept = std::sync::Arc::clone(&store);
        let files = store.write(move |tx| keep(tx, &kept, &text, &roots)).unwrap();
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].title.as_deref(), Some("Landing page"));
        assert_eq!(files[1].title, None);
    }
}
