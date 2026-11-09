//! Backups and export (B22). A daily automatic backup (keeps 7), manual
//! backups (keep 20), restore at the next start with work paused until the
//! user resumes, and a readable Markdown + JSON project export.

use crate::Service;
use aurelius_domain::ProjectId;
use aurelius_protocol::{BackupView, BackupsSnapshot};
use aurelius_storage::{backup, queries};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

const DAY_MS: i64 = 24 * 60 * 60 * 1000;

fn environment() -> &'static str {
    crate::Environment::current().as_str()
}

pub fn snapshot(root: &Path) -> BackupsSnapshot {
    let paused = std::fs::read_to_string(root.join("data").join("paused")).ok().map(|s| s.trim().to_string());
    let restore_pending = std::fs::read_to_string(root.join("data").join("restore-pending"))
        .ok()
        .map(|s| Path::new(s.trim()).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
    BackupsSnapshot {
        backups: backup::list(root)
            .into_iter()
            .map(|(dir, m)| BackupView {
                id: dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                kind: m.kind,
                created_at_ms: m.created_at_ms,
                app_version: m.app_version,
                bytes: m.bytes,
                files: m.blob_count,
                scrubbed: m.scrubbed_at_ms.is_some(),
            })
            .collect(),
        folder: backup::backups_dir(root).display().to_string(),
        paused,
        restore_pending,
        error: None,
    }
}

pub async fn create(shared: &Arc<impl Service>, kind: &'static str) -> BackupsSnapshot {
    let store = shared.store().clone();
    let root = shared.root().to_path_buf();
    let version = shared.version();
    let result = tokio::task::spawn_blocking(move || backup::create(&store, &root, kind, version, environment())).await;
    let mut snap = snapshot(shared.root());
    match result {
        Ok(Ok((dir, _))) => tracing::info!(dir = %dir.display(), kind, "backup created"),
        Ok(Err(err)) => snap.error = Some(format!("Backup failed: {err}")),
        Err(err) => snap.error = Some(format!("Backup failed: {err}")),
    }
    snap
}

/// Verify the backup, mark it for restore and stop the service; it starts
/// again by itself and restores before the database is opened.
pub async fn restore(shared: &Arc<impl Service>, id: &str) -> BackupsSnapshot {
    let mut snap = snapshot(shared.root());
    let Some(dir) = backup::list(shared.root()).into_iter().map(|(d, _)| d).find(|d| d.file_name().is_some_and(|n| n == id)) else {
        snap.error = Some("That backup no longer exists.".into());
        return snap;
    };
    let store = shared.store().clone();
    let root = shared.root().to_path_buf();
    let checked = tokio::task::spawn_blocking(move || -> Result<(), aurelius_storage::StorageError> {
        let manifest = backup::verify(&dir)?;
        let current: i64 = store.read(|c| Ok(c.query_row("SELECT COALESCE(MAX(version), 0) FROM schema_migrations", [], |r| r.get(0))?))?;
        if manifest.schema_version > current {
            return Err(aurelius_storage::StorageError::Rejected("This backup was made by a newer Aurelius; update first.".into()));
        }
        if manifest.environment != environment() {
            return Err(aurelius_storage::StorageError::Rejected(format!("This backup belongs to the {} environment.", manifest.environment)));
        }
        backup::request_restore(&root, &dir)
    })
    .await;
    match checked {
        Ok(Ok(())) => {
            tracing::info!(id, "restore requested; stopping for restart");
            snap = snapshot(shared.root());
            crate::restart_after_reply(shared);
        }
        Ok(Err(err)) => snap.error = Some(format!("Restore refused: {err}")),
        Err(err) => snap.error = Some(format!("Restore refused: {err}")),
    }
    snap
}

pub fn resume(shared: &Arc<impl Service>) -> BackupsSnapshot {
    let mut snap = match backup::resume(shared.root()) {
        Ok(()) => snapshot(shared.root()),
        Err(err) => BackupsSnapshot { error: Some(err.to_string()), ..snapshot(shared.root()) },
    };
    if snap.error.is_none() {
        shared.resume_work();
        tracing::info!("work resumed after restore");
        snap.paused = None;
    }
    snap
}

/// Daily automatic backup: shortly after start when the last one is older
/// than a day, then checked every hour. Skipped while paused after a restore
/// (rotating out good backups for restored state would be a bad trade).
pub fn start(shared: Arc<impl Service>) {
    if std::env::var_os("AURELIUS_NO_AUTO_BACKUP").is_some() {
        return;
    }
    tokio::spawn(async move {
        let mut shutdown = shared.shutdown_signal();
        let mut delay = Duration::from_secs(120);
        loop {
            tokio::select! {
                _ = tokio::time::sleep(delay) => {}
                _ = shutdown.changed() => return,
            }
            delay = Duration::from_secs(3600);
            if backup::is_paused(shared.root()) {
                continue;
            }
            let last = backup::list(shared.root()).into_iter().filter(|(_, m)| m.kind == "auto").map(|(_, m)| m.created_at_ms).max();
            if last.is_none_or(|t| aurelius_storage::now_ms() - t >= DAY_MS) {
                let snap = create(&shared, "auto").await;
                if let Some(err) = snap.error {
                    tracing::warn!(%err, "automatic backup failed");
                }
            }
        }
    });
}

/// Markdown + JSON export of one project into `<project folder>/exports`.
pub async fn export_project(shared: &Arc<impl Service>, project_id: ProjectId) -> Result<(String, String), String> {
    let store = shared.store().clone();
    let fallback = shared.root().join("exports");
    let version = shared.version();
    tokio::task::spawn_blocking(move || export_blocking(&store, project_id, fallback, version)).await.map_err(|e| e.to_string())?
}

fn export_blocking(store: &aurelius_storage::Store, project_id: ProjectId, fallback: PathBuf, version: &str) -> Result<(String, String), String> {
    use std::fmt::Write;
    let read = |key: &str| store.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
    let data = store
        .read(|c| {
            let project = queries::list_projects(c, &read)?.into_iter().find(|p| p.id == project_id);
            let tasks = aurelius_storage::tasks::list_tasks(c, Some(project_id), &read)?;
            // Employees are company-wide: the project's export names who worked in it.
            let employees: Vec<_> = aurelius_storage::employees::list_employees(c, &read)?
                .into_iter()
                .filter(|m| tasks.iter().any(|t| t.employee_id == Some(m.id)))
                .collect();
            let wanted = aurelius_storage::memory::Wanted { project: Some(project_id), ..Default::default() };
            let memory = aurelius_storage::memory::list(c, wanted, &read)?;
            // The project's conversations with its assistant.
            let mut chats = Vec::new();
            for p in aurelius_storage::conversations::project_conversations(c, project_id)? {
                chats.push((p.title.clone(), aurelius_storage::conversations::list_messages(c, p.id, &read)?));
            }
            Ok((project, employees, tasks, memory, chats))
        })
        .map_err(|e| e.to_string())?;
    let (Some(project), employees, tasks, memory, chats) = data else { return Err("Project not found.".into()) };
    let memory: Vec<_> = memory.into_iter().filter(|m| matches!(m.status, aurelius_domain::memory::MemoryStatus::Active) && m.scope != "global").collect();

    let when = |ms: i64| {
        let secs = ms / 1000 + crate::utc_offset_minutes_at(ms) * 60;
        let days = secs.div_euclid(86_400);
        let (y, mo, d) = civil(days);
        let t = secs.rem_euclid(86_400);
        format!("{y:04}-{mo:02}-{d:02} {:02}:{:02}", t / 3600, t % 3600 / 60)
    };
    let mut md = String::new();
    let _ = writeln!(md, "# {}\n", project.name);
    if let Some(d) = project.description.as_deref().filter(|d| !d.is_empty()) {
        let _ = writeln!(md, "{d}\n");
    }
    let _ = writeln!(md, "Exported from Aurelius {} on {}.\n", version, when(aurelius_storage::now_ms()));
    let _ = writeln!(md, "## Employees who worked here\n");
    for m in &employees {
        let _ = writeln!(md, "- **{}** — {}", m.name, m.role);
    }
    let _ = writeln!(md, "\n## Tasks\n");
    for t in &tasks {
        let _ = writeln!(md, "### {} ({:?})\n\nGoal: {}\n", t.title, t.status, t.goal);
        for c in &t.criteria {
            let _ = writeln!(md, "- [ ] {c}");
        }
        if let Some(r) = &t.result {
            // Headings inside a result sit below the task's own heading.
            let demoted: Vec<String> = r.lines().map(|l| if l.starts_with('#') { format!("###{l}") } else { l.to_string() }).collect();
            let _ = writeln!(md, "\n{}\n", demoted.join("\n"));
        }
    }
    let _ = writeln!(md, "\n## Memory\n");
    for m in &memory {
        let _ = writeln!(md, "- {}{}", m.content.replace('\n', " "), if m.pinned { " (pinned)" } else { "" });
    }
    for (title, messages) in &chats {
        let _ = writeln!(md, "\n## Conversation: {title}\n");
        for msg in messages {
            let who = match msg.role {
                aurelius_domain::conversation::MessageRole::User => "You",
                _ => msg.author.as_deref().unwrap_or("Project assistant"),
            };
            let _ = writeln!(md, "**{who}** · {}\n\n{}\n", when(msg.created_at_ms), msg.text);
        }
    }
    let json = serde_json::json!({
        "format": "aurelius-project-export",
        "version": 1,
        "app_version": version,
        "exported_at_ms": aurelius_storage::now_ms(),
        "project": project,
        "employees": employees,
        "tasks": tasks,
        "memory": memory,
        "conversations": chats.iter().map(|(t, m)| serde_json::json!({ "title": t, "messages": m })).collect::<Vec<_>>(),
    });

    let dir = project.folder.as_deref().map(|f| PathBuf::from(f).join("exports")).unwrap_or(fallback);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let stamp = when(aurelius_storage::now_ms()).replace([' ', ':'], "-");
    let base = format!("{} export {stamp}", aurelius_domain::project::folder_name(&project.name));
    let md_path = dir.join(format!("{base}.md"));
    let json_path = dir.join(format!("{base}.json"));
    std::fs::write(&md_path, md).map_err(|e| e.to_string())?;
    std::fs::write(&json_path, serde_json::to_vec_pretty(&json).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    Ok((md_path.display().to_string(), json_path.display().to_string()))
}

/// Days since 1970-01-01 → (year, month, day).
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aurelius_storage::Store;
    use std::sync::atomic::{AtomicBool, Ordering};

    struct Fake {
        store: Arc<Store>,
        root: PathBuf,
        power: crate::power::Power,
        shutdown: tokio::sync::watch::Sender<bool>,
        restarted: AtomicBool,
    }

    impl Service for Fake {
        fn store(&self) -> &Arc<Store> {
            &self.store
        }
        fn root(&self) -> &Path {
            &self.root
        }
        fn version(&self) -> &'static str {
            "0.11.0"
        }
        fn shutdown_signal(&self) -> tokio::sync::watch::Receiver<bool> {
            self.shutdown.subscribe()
        }
        fn power(&self) -> &crate::power::Power {
            &self.power
        }
        fn resume_work(&self) {}
        fn stop_for_restart(&self) {
            self.restarted.store(true, Ordering::SeqCst);
        }
    }

    fn files(dir: &Path) -> Vec<(String, Vec<u8>)> {
        let mut out: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .filter(|e| !e.file_name().to_string_lossy().ends_with("-shm"))
            .map(|e| (e.file_name().to_string_lossy().into_owned(), std::fs::read(e.path()).unwrap_or_default()))
            .collect();
        out.sort();
        out
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_backup_from_before_v011_is_refused_without_a_restart() {
        // A schema 17 backup (protocol 3), next to a v0.11 workspace.
        let old = tempfile::tempdir().unwrap();
        drop(Store::open_until(old.path(), 17).unwrap());
        let made = backup::before_migration(old.path(), "0.10.0", environment()).unwrap().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let store = Arc::new(Store::open(&root).unwrap());
        let id = made.file_name().unwrap().to_string_lossy().into_owned();
        std::fs::create_dir_all(backup::backups_dir(&root)).unwrap();
        std::fs::rename(&made, backup::backups_dir(&root).join(&id)).unwrap();
        let shared = Arc::new(Fake {
            store,
            root: root.clone(),
            power: Default::default(),
            shutdown: tokio::sync::watch::channel(false).0,
            restarted: AtomicBool::new(false),
        });
        let before = files(&root.join("data"));

        let snap = restore(&shared, &id).await;

        let error = snap.error.expect("the client sees why");
        assert!(error.contains("This backup is from before v0.11") && error.contains("previous build"), "{error}");
        assert_eq!(snap.restore_pending, None);
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert!(!shared.restarted.load(Ordering::SeqCst), "no restart");
        assert_eq!(files(&root.join("data")), before, "data/ is unchanged");
    }
}
