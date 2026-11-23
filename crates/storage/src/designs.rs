//! The designs of a project (`design.list`): the HTML files runs kept with their
//! messages, grouped per conversation into pages (one per file name, the newest).

use crate::StorageError;
use aurelius_domain::views::{DESIGNS_MAX, DesignPageView, DesignView, FileRef};
use aurelius_domain::{ConversationId, MessageId, ProjectId, TaskId};
use rusqlite::{Connection, params};

/// Whether a kept file is a design: its media type is `text/html` (any parameters).
const IS_HTML: &str = "(lower(json_extract(f.value, '$.media_type')) = 'text/html' OR lower(json_extract(f.value, '$.media_type')) LIKE 'text/html;%')";

/// The designs of `project` (all projects without one), the one with the newest page
/// first, at most `DESIGNS_MAX`. A deleted message no longer offers its files.
pub fn list(
    conn: &Connection,
    project: Option<ProjectId>,
) -> Result<Vec<DesignView>, StorageError> {
    let mut stmt = conn.prepare(&format!(
        "SELECT c.id, c.kind, c.project_id, c.title, t.number, k.id, m.id, m.created_at_ms, f.value
         FROM messages m
         JOIN conversations c ON c.id = m.conversation_id
         JOIN json_each(m.meta_json, '$.files') f
         LEFT JOIN tickets t ON t.conversation_id = c.id
         LEFT JOIN tasks k ON k.conversation_id = c.id
         WHERE m.meta_json IS NOT NULL AND m.deleted_at_ms IS NULL AND c.project_id IS NOT NULL
           AND (?1 IS NULL OR c.project_id = ?1) AND {IS_HTML}
         ORDER BY c.id, m.ordinal, f.key"
    ))?;
    let rows = stmt.query_map(params![project.map(|p| p.to_string())], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, Option<i64>>(4)?,
            r.get::<_, Option<String>>(5)?,
            r.get::<_, String>(6)?,
            r.get::<_, i64>(7)?,
            r.get::<_, String>(8)?,
        ))
    })?;
    let mut designs: Vec<DesignView> = Vec::new();
    for row in rows {
        let (conversation, kind, project_id, title, ticket_number, task, message, at_ms, file) =
            row?;
        let Ok(file) = serde_json::from_str::<FileRef>(&file) else {
            continue;
        };
        let page = DesignPageView {
            file,
            message_id: parse(MessageId::parse(&message))?,
            at_ms,
        };
        let conversation = parse(ConversationId::parse(&conversation))?;
        match designs
            .last_mut()
            .filter(|d| d.conversation_id == conversation)
        {
            Some(design) => {
                design.last_at_ms = design.last_at_ms.max(at_ms);
                match design
                    .pages
                    .iter_mut()
                    .find(|p| p.file.name == page.file.name)
                {
                    Some(older) => *older = page,
                    None => design.pages.push(page),
                }
            }
            None => designs.push(DesignView {
                conversation_id: conversation,
                kind,
                project_id: parse(ProjectId::parse(&project_id))?,
                title,
                ticket_number,
                task_id: task.map(|t| parse(TaskId::parse(&t))).transpose()?,
                pages: vec![page],
                last_at_ms: at_ms,
            }),
        }
    }
    designs.sort_by_key(|d| std::cmp::Reverse(d.last_at_ms));
    designs.truncate(DESIGNS_MAX);
    Ok(designs)
}

fn parse<T, E: std::fmt::Display>(value: Result<T, E>) -> Result<T, StorageError> {
    value.map_err(|e| StorageError::Rejected(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Store;

    const ALPHA: &str = "00000000-0000-7000-8000-0000000007a1";
    const BETA: &str = "00000000-0000-7000-8000-0000000007a2";
    const PIXEL: &str = "00000000-0000-7000-8000-0000000007b1";
    /// A session of Alpha that wrote landing.html twice and pricing.html once.
    const S1: &str = "00000000-0000-7000-8000-0000000007d1";
    /// A later session of Alpha with one page, and a PNG that is no design.
    const S2: &str = "00000000-0000-7000-8000-0000000007d2";
    /// A session of Beta.
    const S3: &str = "00000000-0000-7000-8000-0000000007d3";

    fn file(id: &str, name: &str, media: &str) -> String {
        format!(r#"{{"id":"{id}","name":"{name}","media_type":"{media}","size":10}}"#)
    }

    fn store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let landing1 = format!("[{}]", file("l1", "landing.html", "text/html"));
        let pricing = format!(
            "[{}]",
            file("p1", "pricing.html", "text/html; charset=utf-8")
        );
        let landing2 = format!("[{}]", file("l2", "landing.html", "text/html"));
        let s2 = format!(
            "[{},{}]",
            file("shot", "shot.png", "image/png"),
            file("a1", "about.html", "text/html")
        );
        let gone = format!("[{}]", file("g1", "gone.html", "text/html"));
        let beta = format!("[{}]", file("b1", "beta.html", "text/html"));
        store
            .write(move |tx| {
                let global: String = tx.query_row("SELECT id FROM scopes WHERE kind = 'global'", [], |r| r.get(0))?;
                tx.execute_batch(&format!(
                    "INSERT INTO scopes(id, kind, parent_id, owner_id) VALUES ('pa', 'project', '{global}', '{ALPHA}'), ('pb', 'project', '{global}', '{BETA}'), ('ep', 'employee', '{global}', '{PIXEL}');
                     INSERT INTO projects(id, scope_id, name, status, version, created_at_ms, updated_at_ms) VALUES
                       ('{ALPHA}', 'pa', 'Alpha', 'active', 1, 1, 1), ('{BETA}', 'pb', 'Beta', 'active', 1, 1, 1);
                     INSERT INTO employees(id, scope_id, name, role, config_json, status, version, created_at_ms, updated_at_ms) VALUES
                       ('{PIXEL}', 'ep', 'Pixel', 'Designer', '{{}}', 'active', 1, 1, 1);
                     INSERT INTO conversations(id, kind, project_id, employee_id, title, version, created_at_ms) VALUES
                       ('{S1}', 'session', '{ALPHA}', '{PIXEL}', 'Landing page', 1, 10),
                       ('{S2}', 'session', '{ALPHA}', '{PIXEL}', 'About page', 1, 20),
                       ('{S3}', 'session', '{BETA}', '{PIXEL}', 'Beta page', 1, 30);
                     INSERT INTO events(id, aggregate_type, aggregate_id, aggregate_version, type, schema_version, actor_kind, correlation_id, metadata_json, dedupe_key, occurred_at_ms, recorded_at_ms) VALUES
                       ('e1', 'conversation', '{S1}', 1, 'message.assistant_committed', 1, 'runtime', 'c', '{{}}', 'k1', 1, 1);"
                ))?;
                let mut insert = tx.prepare(
                    "INSERT INTO messages(id, conversation_id, ordinal, role, source_event_id, status, input_kind, created_at_ms, meta_json, deleted_at_ms)
                     VALUES (?1, ?2, ?3, 'assistant', 'e1', 'complete', 'text', ?4, json_object('files', json(?5)), ?6)",
                )?;
                for (id, conv, ordinal, at, files, deleted) in [
                    ("00000000-0000-7000-8000-0000000007e1", S1, 1, 100, &landing1, None::<i64>),
                    ("00000000-0000-7000-8000-0000000007e2", S1, 2, 200, &pricing, None),
                    ("00000000-0000-7000-8000-0000000007e3", S1, 3, 300, &landing2, None),
                    ("00000000-0000-7000-8000-0000000007e4", S1, 4, 900, &gone, Some(950)),
                    ("00000000-0000-7000-8000-0000000007e5", S2, 1, 400, &s2, None),
                    ("00000000-0000-7000-8000-0000000007e6", S3, 1, 500, &beta, None),
                ] {
                    insert.execute(params![id, conv, ordinal, at, files, deleted])?;
                }
                Ok(())
            })
            .unwrap();
        (dir, store)
    }

    fn alpha() -> Option<ProjectId> {
        Some(ProjectId::parse(ALPHA).unwrap())
    }

    #[test]
    fn a_project_lists_its_designs_latest_first_with_the_newest_file_of_each_page() {
        let (_dir, store) = store();
        let designs = store.read(|c| list(c, alpha())).unwrap();
        let shown: Vec<_> = designs
            .iter()
            .map(|d| {
                (
                    d.title.as_str(),
                    d.last_at_ms,
                    d.pages
                        .iter()
                        .map(|p| (p.file.name.as_str(), p.file.id.as_str()))
                        .collect::<Vec<_>>(),
                )
            })
            .collect();
        assert_eq!(
            shown,
            [
                ("About page", 400, vec![("about.html", "a1")]),
                (
                    "Landing page",
                    300,
                    vec![("landing.html", "l2"), ("pricing.html", "p1")]
                )
            ]
        );
        assert_eq!(
            designs[1].pages[0].message_id.to_string(),
            "00000000-0000-7000-8000-0000000007e3",
            "the page is read from the message that kept its newest file"
        );
        assert_eq!(designs[1].kind, "session");
    }

    #[test]
    fn a_deleted_message_and_a_file_that_is_no_html_are_no_design() {
        let (_dir, store) = store();
        let names: Vec<String> = store
            .read(|c| list(c, alpha()))
            .unwrap()
            .into_iter()
            .flat_map(|d| d.pages)
            .map(|p| p.file.name)
            .collect();
        assert!(!names.contains(&"gone.html".to_owned()), "{names:?}");
        assert!(!names.contains(&"shot.png".to_owned()), "{names:?}");
    }

    #[test]
    fn without_a_project_every_project_counts() {
        let (_dir, store) = store();
        let titles: Vec<String> = store
            .read(|c| list(c, None))
            .unwrap()
            .into_iter()
            .map(|d| d.title)
            .collect();
        assert_eq!(titles, ["Beta page", "About page", "Landing page"]);
    }
}
