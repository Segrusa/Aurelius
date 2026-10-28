//! Tool implementations: files, Git, web and text extraction. Nothing here
//! decides who may use them; callers check that first.

pub mod extract;
pub mod files;
pub mod git;
pub mod html;
pub mod web;
pub mod worktree;

pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest;
    hex::encode(sha2::Sha256::digest(bytes))
}
