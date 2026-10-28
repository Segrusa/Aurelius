//! `fs/list_dir`: folders on the server, for a client on another computer
//! that picks a project or granted folder. Folders only, never
//! file names or contents; hidden and system folders are left out.
//! `fs/create_dir` (1.10) makes one new folder there.

use crate::Shared;
use aurelius_protocol::{DirEntryView, DirListing, ErrorCode, ProtocolError};
use std::path::Path;
use std::sync::Arc;

/// More folders than this in one place are cut off (`truncated`).
const MAX_ENTRIES: usize = 1000;

pub async fn list(_shared: &Arc<Shared>, path: Option<String>) -> Result<DirListing, ProtocolError> {
    tokio::task::spawn_blocking(move || list_blocking(path.as_deref()))
        .await
        .map_err(|e| ProtocolError::new(ErrorCode::Internal, e.to_string()))?
}

/// A new folder `name` in `parent`; the answer is the new folder's listing.
pub async fn create(_shared: &Arc<Shared>, parent: String, name: String) -> Result<DirListing, ProtocolError> {
    tokio::task::spawn_blocking(move || create_blocking(&parent, &name))
        .await
        .map_err(|e| ProtocolError::new(ErrorCode::Internal, e.to_string()))?
}

fn roots() -> Vec<DirEntryView> {
    aurelius_host::fs_roots()
        .into_iter()
        .map(|(name, path)| DirEntryView { name, path: display(&path) })
        .collect()
}

fn list_blocking(path: Option<&str>) -> Result<DirListing, ProtocolError> {
    let Some(path) = path.map(str::trim).filter(|p| !p.is_empty()) else {
        return Ok(DirListing { roots: roots(), ..DirListing::default() });
    };
    let dir = std::fs::canonicalize(path)
        .map_err(|_| ProtocolError::new(ErrorCode::NotFound, "This folder does not exist (anymore)."))?;
    if !dir.is_dir() {
        return Err(ProtocolError::new(ErrorCode::Validation, "This is not a folder."));
    }
    let (entries, truncated) = folders_in(&dir)
        .map_err(|e| ProtocolError::new(ErrorCode::Validation, format!("This folder can't be opened: {e}")))?;
    Ok(DirListing {
        path: Some(display(&dir)),
        parent: dir.parent().map(display),
        entries,
        roots: roots(),
        truncated,
    })
}

fn create_blocking(parent: &str, name: &str) -> Result<DirListing, ProtocolError> {
    let name = valid_name(name).map_err(|why| ProtocolError::new(ErrorCode::Validation, why))?;
    let parent = parent.trim();
    let dir = (!parent.is_empty())
        .then(|| std::fs::canonicalize(parent).ok())
        .flatten()
        .ok_or_else(|| ProtocolError::new(ErrorCode::NotFound, "The folder to create it in does not exist (anymore)."))?;
    if !dir.is_dir() {
        return Err(ProtocolError::new(ErrorCode::Validation, "The place to create it in is not a folder."));
    }
    let target = dir.join(name);
    match std::fs::create_dir(&target) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err(ProtocolError::new(ErrorCode::Validation, "A folder with this name already exists."));
        }
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            return Err(ProtocolError::new(ErrorCode::Validation, "You may not create a folder here."));
        }
        Err(e) => return Err(ProtocolError::new(ErrorCode::Validation, format!("The folder can't be created: {e}"))),
    }
    list_blocking(Some(&target.display().to_string()))
}

/// The trimmed name, if it is a valid folder name on every OS the server runs on.
fn valid_name(name: &str) -> Result<&str, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Give the folder a name.".into());
    }
    if name == "." || name == ".." {
        return Err("\".\" and \"..\" can't be used as a folder name.".into());
    }
    if let Some(c) = name.chars().find(|c| matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control()) {
        return Err(if c.is_control() {
            "A folder name can't contain control characters.".into()
        } else {
            format!("A folder name can't contain {c} (not allowed: / \\ : * ? \" < > |).")
        });
    }
    if name.ends_with('.') {
        return Err("A folder name can't end with a dot.".into());
    }
    if reserved(name) {
        return Err(format!("\"{name}\" is a name Windows keeps for itself; choose another."));
    }
    Ok(name)
}

/// Windows device names (CON, NUL, COM1…), also with an extension ("nul.txt").
fn reserved(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name).trim_end().to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ["COM", "LPT"].iter().any(|p| {
            stem.strip_prefix(p).is_some_and(|n| matches!(n, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"))
        })
}

fn folders_in(dir: &Path) -> std::io::Result<(Vec<DirEntryView>, bool)> {
    let mut entries: Vec<DirEntryView> = std::fs::read_dir(dir)?
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()) && !hidden(e))
        .map(|e| DirEntryView { name: e.file_name().to_string_lossy().into_owned(), path: display(&e.path()) })
        .collect();
    entries.sort_by_key(|e| e.name.to_lowercase());
    let truncated = entries.len() > MAX_ENTRIES;
    entries.truncate(MAX_ENTRIES);
    Ok((entries, truncated))
}

fn hidden(entry: &std::fs::DirEntry) -> bool {
    if entry.file_name().to_string_lossy().starts_with('.') {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const HIDDEN: u32 = 0x2;
        const SYSTEM: u32 = 0x4;
        if entry.metadata().is_ok_and(|m| m.file_attributes() & (HIDDEN | SYSTEM) != 0) {
            return true;
        }
    }
    false
}

/// Paths as the user knows them: without Windows' `\\?\` prefix from canonicalize.
fn display(path: &Path) -> String {
    let text = path.display().to_string();
    match text.strip_prefix(r"\\?\UNC\") {
        Some(unc) => format!(r"\\{unc}"),
        None => text.strip_prefix(r"\\?\").map(str::to_owned).unwrap_or(text),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_folders_only_sorted_without_hidden() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["beta", "Alpha", ".git"] {
            std::fs::create_dir(dir.path().join(name)).unwrap();
        }
        std::fs::write(dir.path().join("notes.txt"), "x").unwrap();
        let listing = list_blocking(Some(&dir.path().display().to_string())).unwrap();
        let names: Vec<_> = listing.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["Alpha", "beta"]);
        assert!(!listing.truncated);
        assert!(listing.parent.is_some());
        assert!(!listing.path.unwrap().starts_with(r"\\?\"));
    }

    #[test]
    fn no_path_lists_only_the_starting_points() {
        let listing = list_blocking(None).unwrap();
        assert!(listing.path.is_none());
        assert!(listing.entries.is_empty());
        assert!(!listing.roots.is_empty());
    }

    #[test]
    fn a_missing_folder_is_not_found() {
        let err = list_blocking(Some("Z:/does/not/exist/aurelius")).unwrap_err();
        assert_eq!(err.code, ErrorCode::NotFound);
    }

    #[test]
    fn creates_a_folder_and_lists_it() {
        let dir = tempfile::tempdir().unwrap();
        let listing = create_blocking(&dir.path().display().to_string(), "  Plans 2026 ").unwrap();
        assert!(dir.path().join("Plans 2026").is_dir());
        let path = listing.path.unwrap();
        assert!(path.ends_with("Plans 2026"), "{path}");
        assert!(!path.starts_with(r"\\?\"));
        assert!(listing.entries.is_empty());
        assert!(listing.parent.is_some());
    }

    #[test]
    fn an_existing_folder_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("Docs")).unwrap();
        let err = create_blocking(&dir.path().display().to_string(), "Docs").unwrap_err();
        assert_eq!(err.message, "A folder with this name already exists.");
        std::fs::write(dir.path().join("notes"), "x").unwrap();
        let err = create_blocking(&dir.path().display().to_string(), "notes").unwrap_err();
        assert_eq!(err.message, "A folder with this name already exists.");
    }

    #[test]
    fn invalid_and_reserved_names_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let parent = dir.path().display().to_string();
        for name in ["", "   ", ".", "..", "a/b", "a\\b", "c:d", "x*", "why?", "\"q\"", "<a>", "a|b", "tab\there", "dot.", "CON", "nul", "Com1", "lpt9.txt", "aux "] {
            let err = create_blocking(&parent, name).unwrap_err();
            assert_eq!(err.code, ErrorCode::Validation, "{name:?}");
            assert!(!err.message.is_empty());
        }
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0, "nothing was created");
        // Names that only look like reserved ones are fine.
        for name in ["CONSOLE", "com10", "nullable", ".config"] {
            create_blocking(&parent, name).unwrap_or_else(|e| panic!("{name}: {}", e.message));
        }
    }

    #[test]
    fn the_parent_must_be_an_existing_folder() {
        let err = create_blocking("Z:/does/not/exist/aurelius", "New").unwrap_err();
        assert_eq!(err.code, ErrorCode::NotFound);
        let err = create_blocking("  ", "New").unwrap_err();
        assert_eq!(err.code, ErrorCode::NotFound);
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("file.txt");
        std::fs::write(&file, "x").unwrap();
        let err = create_blocking(&file.display().to_string(), "New").unwrap_err();
        assert_eq!(err.code, ErrorCode::Validation);
    }

    #[test]
    fn long_folders_are_cut_off() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..MAX_ENTRIES + 3 {
            std::fs::create_dir(dir.path().join(format!("d{i:04}"))).unwrap();
        }
        let (entries, truncated) = folders_in(dir.path()).unwrap();
        assert_eq!(entries.len(), MAX_ENTRIES);
        assert!(truncated);
    }
}
