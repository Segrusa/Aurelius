//! Claude Code artifacts that runs published: kept by URL (so later runs
//! republish to the same page) and announced on the update stream.

use crate::{CommandService, notice, storage_error};
use aurelius_domain::views::{ArtifactContent, ArtifactKind, ArtifactView};
use aurelius_domain::{ActorKind, AppError, ConversationId, ErrorCode, EventKind, EventNotice};
use aurelius_storage::artifacts::{self, Filter, Published, PublishedFile};
use std::path::{Path, PathBuf};

/// Largest published file the server keeps (as the markdown viewer).
pub const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

/// The file a publish sent, read right after Claude Code reported it: `path`
/// as the run named it, relative to its first folder. None when it is missing,
/// larger than [`MAX_FILE_BYTES`] or not UTF-8 text.
pub fn read_published_file(path: &str, folders: &[PathBuf]) -> Option<PublishedFile> {
    let path = Path::new(path);
    let path = if path.is_relative() { folders.first()?.join(path) } else { path.to_path_buf() };
    let meta = std::fs::metadata(&path).ok()?;
    if !meta.is_file() || meta.len() > MAX_FILE_BYTES {
        tracing::info!(path = %path.display(), bytes = meta.len(), "published artifact file not kept");
        return None;
    }
    let content = String::from_utf8(std::fs::read(&path).ok()?).ok()?;
    let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let kind = match extension.as_str() {
        "html" | "htm" => ArtifactKind::Html,
        "md" | "markdown" => ArtifactKind::Markdown,
        _ => ArtifactKind::Text,
    };
    Some(PublishedFile { kind, file_name: path.file_name()?.to_string_lossy().into_owned(), content })
}
use aurelius_storage::conversations as conv;
use aurelius_storage::events::{self, NewEvent};

impl CommandService {
    /// A run in `conversation` published an artifact: keep (or update) it and
    /// emit `artifact.published` on the conversation, with the project set.
    pub fn record_artifact(&self, conversation: ConversationId, published: &Published) -> Result<Vec<EventNotice>, AppError> {
        let published = published.clone();
        self.store
            .write(move |tx| {
                let now = aurelius_storage::now_ms();
                let view = artifacts::record(tx, conversation, &published, now)?;
                let aggregate_id = conversation.to_string();
                let kind = EventKind::ArtifactPublished.as_str();
                let appended = events::append(
                    tx,
                    NewEvent {
                        aggregate_type: "conversation",
                        aggregate_id: aggregate_id.clone(),
                        aggregate_version: conv::next_version(tx, "conversation", &aggregate_id)?,
                        event_type: kind,
                        project_id: view.project_id,
                        actor_kind: ActorKind::Runtime,
                        actor_id: None,
                        correlation_id: aggregate_id.clone(),
                        causation_id: None,
                        payload_blob_id: None,
                        metadata: serde_json::json!({ "url": view.url, "title": view.title, "version": view.version, "task_id": view.task_id }),
                        dedupe_key: format!("artifact:{}:{}:{now}", view.url, view.version),
                    },
                )?;
                Ok(vec![notice(appended.seq, kind, aggregate_id, view.project_id)])
            })
            .map_err(storage_error)
    }

    /// `artifact.get_content`: the kept file of one version (else the newest);
    /// `snapshot` is none for pages published before servers kept them.
    pub fn artifact_content(&self, url: String, version: Option<u32>) -> Result<ArtifactContent, AppError> {
        self.store
            .read(move |c| artifacts::content(c, &url, version))
            .map_err(storage_error)?
            .ok_or_else(|| AppError::new(ErrorCode::NotFound, "This artifact is not known to the server (or its conversation was deleted)."))
    }

    /// `artifact.list`: by conversation, task or project (all set filters apply; none lists all).
    pub fn artifacts(&self, filter: Filter) -> Result<Vec<ArtifactView>, AppError> {
        self.store.read(|c| artifacts::list(c, filter)).map_err(storage_error)
    }
}
