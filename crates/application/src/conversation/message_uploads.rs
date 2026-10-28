//! Files of any type the user sends to an employee with a message. They are
//! copied into the conversation's work folder under `.aurelius/uploads/`,
//! where the employee reads them with its own tools; git ignores that folder.
//! (Standalone chats keep their text attachments, see `files`.)

use crate::domain_error;
use aurelius_domain::{DomainError, conversation::UPLOAD_MAX_BYTES};
use aurelius_domain::{AppError};
use aurelius_domain::views::{MessageFileView};
use std::io::Read;
use std::path::{Path, PathBuf};

/// Where uploads land, relative to the work folder.
pub const DIR: &str = ".aurelius/uploads";
/// Longest file name kept (the extension stays).
const NAME_MAX: usize = 120;

/// A file to upload, checked to exist and be small enough.
#[derive(Debug, Clone)]
pub struct Source {
    pub path: PathBuf,
    /// Safe name to copy it under.
    pub name: String,
}

fn upload_error(msg: impl Into<String>) -> AppError {
    domain_error(DomainError::File(msg.into()))
}

pub(crate) fn too_large(shown: &str, bytes: u64) -> AppError {
    upload_error(format!("{shown}: the file is too large ({}; at most {} MB).", size_label(bytes), UPLOAD_MAX_BYTES / (1024 * 1024)))
}

/// Check that each file is there, is a file and is not too large.
pub fn check(paths: &[String]) -> Result<Vec<Source>, AppError> {
    paths
        .iter()
        .map(|raw| {
            let path = PathBuf::from(raw.trim());
            let shown = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| raw.trim().to_owned());
            let metadata = std::fs::metadata(&path).map_err(|_| upload_error(format!("{shown}: the file is missing or can't be read.")))?;
            if !metadata.is_file() {
                return Err(upload_error(format!("{shown}: choose a file, not a folder.")));
            }
            if metadata.len() > UPLOAD_MAX_BYTES {
                return Err(too_large(&shown, metadata.len()));
            }
            Ok(Source { name: safe_name(&shown), path })
        })
        .collect()
}

/// The folder a conversation's uploads go to: its first work folder that exists.
pub fn work_folder(folders: &[String]) -> Result<PathBuf, AppError> {
    folders
        .iter()
        .map(PathBuf::from)
        .find(|d| d.is_dir())
        .ok_or_else(|| upload_error("This conversation has no work folder on the server to put files in; give the project or the employee a folder first."))
}

/// Copy the files into `<folder>/.aurelius/uploads/` under unique names, with
/// a `.gitignore` that keeps the folder out of git. On failure the files
/// copied so far are removed again.
pub fn copy(folder: &Path, sources: &[Source]) -> Result<Vec<MessageFileView>, AppError> {
    let dir = folder.join(DIR);
    std::fs::create_dir_all(&dir).map_err(|e| upload_error(format!("Could not create {DIR} in the work folder: {e}")))?;
    let ignore = dir.join(".gitignore");
    if !ignore.exists() {
        std::fs::write(&ignore, "*\n").map_err(|e| upload_error(format!("Could not write {DIR}/.gitignore: {e}")))?;
    }
    let mut out: Vec<MessageFileView> = Vec::new();
    for source in sources {
        match copy_one(&dir, source) {
            Ok(view) => out.push(view),
            Err(e) => {
                remove(folder, &out);
                return Err(e);
            }
        }
    }
    Ok(out)
}

fn copy_one(dir: &Path, source: &Source) -> Result<MessageFileView, AppError> {
    let failed = |e: std::io::Error| upload_error(format!("{}: could not copy it into the work folder: {e}", source.name));
    let input = std::fs::File::open(&source.path).map_err(|_| upload_error(format!("{}: the file is missing or can't be read.", source.name)))?;
    let (name, mut output) = create_unique(dir, &source.name).map_err(failed)?;
    // The file may have grown since it was checked.
    let copied = std::io::copy(&mut input.take(UPLOAD_MAX_BYTES + 1), &mut output);
    drop(output);
    match copied {
        Ok(n) if n <= UPLOAD_MAX_BYTES => Ok(MessageFileView { id: None, path: format!("{DIR}/{name}"), name, byte_length: n }),
        Ok(n) => {
            let _ = std::fs::remove_file(dir.join(&name));
            Err(too_large(&source.name, n))
        }
        Err(e) => {
            let _ = std::fs::remove_file(dir.join(&name));
            Err(failed(e))
        }
    }
}

/// Create `name` in `dir`, or "stem (2).ext", "stem (3).ext"… when taken.
fn create_unique(dir: &Path, name: &str) -> std::io::Result<(String, std::fs::File)> {
    let (stem, extension) = split_extension(name);
    for n in 1..10_000 {
        let candidate = if n == 1 { name.to_owned() } else { format!("{stem} ({n}){extension}") };
        match std::fs::OpenOptions::new().write(true).create_new(true).open(dir.join(&candidate)) {
            Ok(file) => return Ok((candidate, file)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::other("too many files with this name"))
}

/// Remove uploaded copies again (a command that failed or was a replay).
pub fn remove(folder: &Path, uploads: &[MessageFileView]) {
    for upload in uploads {
        let _ = std::fs::remove_file(folder.join(&upload.path));
    }
}

/// ("report", ".pdf"); a leading dot is part of the stem (".env" has no extension).
fn split_extension(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(i) if i > 0 && name.len() - i <= 20 => name.split_at(i),
        _ => (name, ""),
    }
}

/// A file name without directories, characters Windows refuses or device names.
pub fn safe_name(raw: &str) -> String {
    let base = raw.rsplit(['/', '\\']).next().unwrap_or_default();
    let cleaned: String = base.chars().map(|c| if c.is_control() || "<>:\"|?*".contains(c) { '_' } else { c }).collect();
    let cleaned = cleaned.trim().trim_end_matches(['.', ' ']).trim_start_matches(' ');
    let (stem, extension) = split_extension(cleaned);
    let stem: String = stem.chars().take(NAME_MAX.saturating_sub(extension.chars().count())).collect();
    let device = matches!(stem.to_ascii_uppercase().as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ["COM", "LPT"].iter().any(|p| stem.len() == 4 && stem.to_ascii_uppercase().starts_with(p) && stem.as_bytes()[3].is_ascii_digit());
    let name = if device { format!("_{stem}{extension}") } else { format!("{stem}{extension}") };
    if name.is_empty() || name == "." || name == ".." { "file".into() } else { name }
}

/// "512 B", "3 KB", "1.2 MB".
pub fn size_label(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    let b = bytes as f64;
    if bytes < 1024 {
        format!("{bytes} B")
    } else if b < KB * KB {
        format!("{:.0} KB", b / KB)
    } else {
        format!("{:.1} MB", b / (KB * KB))
    }
}

/// The line in front of the user's text on the turn the files were sent.
pub fn prompt_line(uploads: &[MessageFileView]) -> Option<String> {
    if uploads.is_empty() {
        return None;
    }
    let list: Vec<String> = uploads.iter().map(|u| format!("{} ({})", u.path, size_label(u.byte_length))).collect();
    Some(format!("The user uploaded these files into your work folder: {}", list.join(", ")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_file_path_has_the_upload_limit() {
        assert_eq!(aurelius_tools::extract::MAX_FILE_BYTES as u64, UPLOAD_MAX_BYTES, "AttachFile");
        assert_eq!(super::super::message_images::MAX_BYTES, UPLOAD_MAX_BYTES, "message images");
        assert_eq!(super::super::message_files::MAX_BYTES, UPLOAD_MAX_BYTES, "message files");
    }

    #[test]
    fn names_are_made_safe() {
        assert_eq!(safe_name("report.pdf"), "report.pdf");
        assert_eq!(safe_name("C:\\Users\\x\\a:b?.txt"), "a_b_.txt");
        assert_eq!(safe_name("notes. . "), "notes");
        assert_eq!(safe_name("con.txt"), "_con.txt");
        assert_eq!(safe_name("COM1"), "_COM1");
        assert_eq!(safe_name(".env"), ".env");
        assert_eq!(safe_name(".."), "file");
        assert_eq!(safe_name(""), "file");
        let long = format!("{}.pdf", "a".repeat(300));
        let safe = safe_name(&long);
        assert_eq!(safe.chars().count(), NAME_MAX);
        assert!(safe.ends_with(".pdf"));
    }

    #[test]
    fn sizes_read_well() {
        assert_eq!(size_label(512), "512 B");
        assert_eq!(size_label(3 * 1024), "3 KB");
        assert_eq!(size_label(1_258_291), "1.2 MB");
    }

    #[test]
    fn copies_get_unique_names_and_git_ignores_them() {
        let dir = tempfile::tempdir().unwrap();
        let work = dir.path().join("work");
        std::fs::create_dir_all(&work).unwrap();
        let file = dir.path().join("report.pdf");
        std::fs::write(&file, b"%PDF-1.7 hello").unwrap();
        let sources = check(&[file.to_string_lossy().into_owned(), file.to_string_lossy().into_owned()]).unwrap();
        let first = copy(&work, &sources).unwrap();
        let again = copy(&work, &sources[..1]).unwrap();
        let paths: Vec<&str> = first.iter().chain(&again).map(|u| u.path.as_str()).collect();
        assert_eq!(paths, [".aurelius/uploads/report.pdf", ".aurelius/uploads/report (2).pdf", ".aurelius/uploads/report (3).pdf"]);
        assert_eq!(first[1].name, "report (2).pdf");
        assert_eq!(first[0].byte_length, 14);
        assert_eq!(std::fs::read(work.join(".aurelius/uploads/report (3).pdf")).unwrap(), b"%PDF-1.7 hello");
        assert_eq!(std::fs::read_to_string(work.join(".aurelius/uploads/.gitignore")).unwrap(), "*\n");
        assert_eq!(
            prompt_line(&first).unwrap(),
            "The user uploaded these files into your work folder: .aurelius/uploads/report.pdf (14 B), .aurelius/uploads/report (2).pdf (14 B)"
        );
        remove(&work, &again);
        assert!(!work.join(".aurelius/uploads/report (3).pdf").exists());
    }

    #[test]
    fn missing_large_or_folder_files_are_refused_clearly() {
        let dir = tempfile::tempdir().unwrap();
        let missing = check(&[dir.path().join("gone.txt").to_string_lossy().into_owned()]).unwrap_err();
        assert!(missing.message.contains("gone.txt: the file is missing"), "{}", missing.message);
        let big = dir.path().join("big.bin");
        std::fs::File::create(&big).unwrap().set_len(UPLOAD_MAX_BYTES + 1).unwrap();
        let large = check(&[big.to_string_lossy().into_owned()]).unwrap_err();
        assert!(large.message.contains("big.bin: the file is too large"), "{}", large.message);
        let folder = check(&[dir.path().to_string_lossy().into_owned()]).unwrap_err();
        assert!(folder.message.contains("not a folder"), "{}", folder.message);
        let none = work_folder(&[dir.path().join("nope").to_string_lossy().into_owned()]).unwrap_err();
        assert!(none.message.contains("no work folder"), "{}", none.message);
    }
}
