//! Folders that may never be a work folder.

use std::path::Path;

/// Paths that may never be a work folder (spec: home indexing off, no credential stores).
pub fn forbidden_root(path: &Path) -> bool {
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let text = canonical.to_string_lossy().to_lowercase();
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(|h| std::fs::canonicalize(h).unwrap_or_default().to_string_lossy().to_lowercase());
    let is_drive_root = canonical.parent().is_none() || text.trim_end_matches('\\').ends_with(':') || text.trim_end_matches('\\').ends_with(":\\");
    let sensitive = [".ssh", ".aws", ".gnupg", "appdata\\roaming\\microsoft\\credentials", ".claude", ".codex"];
    is_drive_root || home.as_deref() == Some(text.as_str()) || sensitive.iter().any(|s| text.contains(s))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sensitive_roots_cannot_be_work_folders() {
        let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).unwrap();
        assert!(forbidden_root(Path::new(&home)));
        assert!(forbidden_root(&Path::new(&home).join(".ssh")));
        let dir = tempfile::tempdir().unwrap();
        assert!(!forbidden_root(dir.path()));
    }
}
