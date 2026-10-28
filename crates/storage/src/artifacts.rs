//! Claude Code artifacts (pages on claude.ai) that runs published, and the
//! server-wide setting that gives runs the artifact tools (off by default).

use crate::StorageError;
use aurelius_domain::views::{ArtifactContent, ArtifactKind, ArtifactSnapshot, ArtifactView};
use aurelius_domain::{ConversationId, ProjectId, TaskId};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

/// `settings` key: `true` gives every Claude Code run the artifact tools.
pub const SETTING_KEY: &str = "claude_artifacts";

/// Whether runs may publish artifacts; off unless the user turned it on.
pub fn enabled(conn: &Connection) -> Result<bool, StorageError> {
    Ok(crate::settings::get(conn, SETTING_KEY)?.and_then(|j| serde_json::from_str::<bool>(&j).ok()).unwrap_or(false))
}

pub fn set_enabled(tx: &Transaction<'_>, enabled: bool, now: i64) -> Result<(), StorageError> {
    crate::settings::put(tx, SETTING_KEY, if enabled { "true" } else { "false" }, now)
}

/// One publish as Claude Code reported it.
#[derive(Debug, Clone)]
pub struct Published {
    pub url: String,
    pub title: String,
    pub version: Option<u32>,
    /// The published file, when it could be read.
    pub file: Option<PublishedFile>,
}

/// The file a publish sent to claude.ai.
#[derive(Debug, Clone)]
pub struct PublishedFile {
    pub kind: ArtifactKind,
    pub file_name: String,
    pub content: String,
}

/// Versions kept per page; older files are dropped.
pub const KEPT_VERSIONS: i64 = 20;

/// Which artifacts to list; none set lists them all.
#[derive(Debug, Clone, Copy, Default)]
pub struct Filter {
    pub conversation_id: Option<ConversationId>,
    pub task_id: Option<TaskId>,
    pub project_id: Option<ProjectId>,
}

/// Keep a publish from a run in `conversation`. The URL is the key: a republish
/// updates the title, version and time; the first conversation, task and project stay.
pub fn record(tx: &Transaction<'_>, conversation: ConversationId, published: &Published, now: i64) -> Result<ArtifactView, StorageError> {
    let (project, task): (Option<String>, Option<String>) = tx
        .query_row(
            "SELECT c.project_id, (SELECT t.id FROM tasks t WHERE t.conversation_id = c.id ORDER BY t.created_at_ms DESC LIMIT 1)
             FROM conversations c WHERE c.id = ?1",
            [conversation.to_string()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .unwrap_or((None, None));
    let title = published.title.trim();
    let title = if title.is_empty() { "Untitled artifact" } else { title };
    tx.execute(
        "INSERT INTO artifacts (url, title, conversation_id, task_id, project_id, version, first_published_at_ms, last_published_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)
         ON CONFLICT(url) DO UPDATE SET title = excluded.title,
           version = CASE WHEN ?8 THEN excluded.version ELSE artifacts.version + 1 END,
           last_published_at_ms = excluded.last_published_at_ms",
        params![published.url, title, conversation.to_string(), task, project, published.version.unwrap_or(1), now, published.version.is_some()],
    )?;
    let version: i64 = tx.query_row("SELECT version FROM artifacts WHERE url = ?1", [&published.url], |r| r.get(0))?;
    if let Some(file) = &published.file {
        tx.execute(
            "INSERT OR REPLACE INTO artifact_versions (url, version, kind, file_name, content, published_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![published.url, version, kind_str(file.kind), file.file_name, file.content, now],
        )?;
        tx.execute(
            "DELETE FROM artifact_versions WHERE url = ?1 AND version NOT IN
               (SELECT version FROM artifact_versions WHERE url = ?1 ORDER BY version DESC LIMIT ?2)",
            params![published.url, KEPT_VERSIONS],
        )?;
    }
    let found = list_where(tx, "url = ?1", &[&published.url])?;
    found.into_iter().next().ok_or_else(|| StorageError::Rejected("artifact not stored".into()))
}

/// `artifact.get_content`: the artifact at `url` and the kept file of `version`
/// (else the newest kept one). None when the URL is unknown or its conversation deleted.
pub fn content(conn: &Connection, url: &str, version: Option<u32>) -> Result<Option<ArtifactContent>, StorageError> {
    let Some(artifact) = list_where(conn, &format!("url = ?1 AND {LIVE}"), &[&url])?.into_iter().next() else {
        return Ok(None);
    };
    let versions = conn
        .prepare("SELECT version FROM artifact_versions WHERE url = ?1 ORDER BY version DESC")?
        .query_map([url], |r| r.get::<_, i64>(0))?
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .map(|v| v.max(0) as u32)
        .collect::<Vec<_>>();
    let wanted = version.or_else(|| versions.first().copied());
    let snapshot = match wanted {
        Some(v) => conn
            .query_row(
                "SELECT kind, file_name, content, published_at_ms FROM artifact_versions WHERE url = ?1 AND version = ?2",
                params![url, v],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)?)),
            )
            .optional()?
            .map(|(kind, file_name, content, published_at_ms)| ArtifactSnapshot { version: v, kind: kind_of(&kind), file_name, content, published_at_ms }),
        None => None,
    };
    Ok(Some(ArtifactContent { artifact, versions, snapshot }))
}

fn kind_str(kind: ArtifactKind) -> &'static str {
    match kind {
        ArtifactKind::Html => "html",
        ArtifactKind::Markdown => "markdown",
        ArtifactKind::Text => "text",
    }
}

fn kind_of(kind: &str) -> ArtifactKind {
    match kind {
        "html" => ArtifactKind::Html,
        "markdown" => ArtifactKind::Markdown,
        _ => ArtifactKind::Text,
    }
}

/// Artifacts whose conversation still exists; a deleted chat is archived.
const LIVE: &str = "conversation_id IN (SELECT id FROM conversations WHERE archived_at_ms IS NULL)";

/// Newest publish first; artifacts of deleted conversations are left out.
pub fn list(conn: &Connection, filter: Filter) -> Result<Vec<ArtifactView>, StorageError> {
    let (conversation, task, project) = (filter.conversation_id.map(|c| c.to_string()), filter.task_id.map(|t| t.to_string()), filter.project_id.map(|p| p.to_string()));
    list_where(
        conn,
        &format!("(?1 IS NULL OR conversation_id = ?1) AND (?2 IS NULL OR task_id = ?2) AND (?3 IS NULL OR project_id = ?3) AND {LIVE}"),
        &[&conversation, &task, &project],
    )
}

/// What a run in `conversation` should republish to: the conversation's own
/// artifacts and those of its task (also published from elsewhere).
pub fn for_run(conn: &Connection, conversation: ConversationId) -> Result<Vec<ArtifactView>, StorageError> {
    let task = crate::tasks::task_for_conversation(conn, conversation)?.map(|t| t.to_string());
    list_where(conn, &format!("(conversation_id = ?1 OR (?2 IS NOT NULL AND task_id = ?2)) AND {LIVE}"), &[&conversation.to_string(), &task])
}

fn list_where(conn: &Connection, condition: &str, args: &[&dyn rusqlite::ToSql]) -> Result<Vec<ArtifactView>, StorageError> {
    let sql = format!(
        "SELECT url, title, conversation_id, task_id, project_id, version, first_published_at_ms, last_published_at_ms,
           EXISTS(SELECT 1 FROM artifact_versions v WHERE v.url = artifacts.url)
         FROM artifacts WHERE ({condition})
         ORDER BY last_published_at_ms DESC, url LIMIT 500"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(args, |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, i64>(6)?,
                r.get::<_, i64>(7)?,
                r.get::<_, bool>(8)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows
        .into_iter()
        .filter_map(|(url, title, conversation, task, project, version, first, last, has_content)| {
            Some(ArtifactView {
                url,
                title,
                conversation_id: ConversationId::parse(&conversation).ok()?,
                task_id: task.and_then(|t| TaskId::parse(&t).ok()),
                project_id: project.and_then(|p| ProjectId::parse(&p).ok()),
                version: version.max(0) as u32,
                first_published_at_ms: first,
                last_published_at_ms: last,
                has_content,
            })
        })
        .collect())
}
