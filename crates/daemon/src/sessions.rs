//! Claude Code sessions that runs left behind. A run that stops for a usage
//! limit keeps its session (under `~/.claude`) for its next attempt; when that
//! attempt never comes (the run or its task is stopped, the chat deleted, or the
//! daemon stopped meanwhile) nothing else removes it. A conversation keeps its
//! own session to go on in (a task's too, after the task ended); that one goes
//! when the conversation is deleted, or the run in it stopped without finishing.

use crate::Shared;
use aurelius_runtime::claude_code::session;
use aurelius_storage::Store;
use aurelius_storage::conversation_sessions::started_since;
use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

/// Remove the sessions of our runs that will not run again. In the background.
pub(crate) fn sweep(shared: &Arc<Shared>) {
    let Some(dir) = session::claude_dir() else { return };
    let store = shared.store().clone();
    let shared = shared.clone();
    tokio::task::spawn_blocking(move || {
        // A run still going removes its own session when it ends.
        let live: HashSet<String> = shared.cancellers.lock().map(|m| m.keys().map(|id| id.to_string()).collect()).unwrap_or_default();
        let since = shared.sessions_since;
        let removed = forget_over(&shared, &dir, &live, since) + forget_done(&store, &dir, &live, since);
        if removed > 0 {
            tracing::info!(removed, "removed Claude Code sessions of runs that will not resume");
        }
    });
}

/// The sessions of conversations that are over (deleted, task ended, or held by
/// a run that is over without finishing): forgotten in the database, and removed
/// when this data root started them (`since`, see `claim_root`).
fn forget_over(shared: &Shared, dir: &Path, live: &HashSet<String>, since: Option<i64>) -> usize {
    match shared.service.close_sessions_over(live.iter().cloned().collect()) {
        Ok(ids) => {
            let ours: Vec<&String> = ids.iter().filter(|id| started_since(id, since)).collect();
            for id in &ours {
                session::remove(dir, id);
            }
            ours.len()
        }
        Err(err) => {
            tracing::warn!(error = %err, "could not look up conversation sessions that are over");
            0
        }
    }
}

/// Only ids that are runs in our own database are touched, never other Claude Code
/// sessions, nor the sessions conversations go on in (named after the run that started them),
/// nor runs a copied or restored database brought along from another data root.
fn forget_done(store: &Store, dir: &Path, live: &HashSet<String>, since: Option<i64>) -> usize {
    let kept = match store.read(aurelius_storage::conversation_sessions::session_ids) {
        Ok(kept) => kept,
        Err(err) => {
            tracing::warn!(error = %err, "could not look up the sessions conversations keep");
            return 0;
        }
    };
    let ids: Vec<String> = session::ids(dir).into_iter().filter(|id| !live.contains(id) && !kept.contains(id) && started_since(id, since)).collect();
    if ids.is_empty() {
        return 0;
    }
    let done = match store.read(|c| aurelius_storage::conversations::runs_done_among(c, &ids)) {
        Ok(done) => done,
        Err(err) => {
            tracing::warn!(error = %err, "could not look up the runs of leftover Claude Code sessions");
            return 0;
        }
    };
    for id in &done {
        session::remove(dir, id);
    }
    done.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_sessions_of_our_runs_that_will_not_resume_are_removed() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path()).unwrap();
        let claude = root.path().join(".claude");
        let folder = claude.join("projects").join("C--work");
        std::fs::create_dir_all(&folder).unwrap();
        let id = |n: u32| format!("0199c3a2-0000-7000-8000-{n:012}");
        // Runs of ours: 1 finished, 2 stopped while it waited for the limit,
        // 3 waits for the limit, 4 is going on, 6 waits for the limit in a
        // cancelled task. 5 is someone else's session.
        let db = rusqlite::Connection::open(root.path().join("data/aurelius.sqlite")).unwrap();
        db.execute_batch("PRAGMA foreign_keys = OFF").unwrap();
        db.execute(
            "INSERT INTO tasks(id, project_id, title, brief_blob_id, status, version, created_at_ms, updated_at_ms) VALUES ('t', 'p', '', 'b', 'cancelled', 1, 0, 0)",
            [],
        )
        .unwrap();
        for (n, status, task) in [(1, "succeeded", None), (2, "cancelling", None), (3, "waiting", None), (4, "cancelling", None), (6, "waiting", Some("t"))] {
            db.execute(
                "INSERT INTO runs(id, conversation_id, task_id, attempt, runtime_kind, model_id, resolved_config_json, status) VALUES (?1, 'c', ?2, 1, 'claude_code', '', '{}', ?3)",
                rusqlite::params![id(n), task, status],
            )
            .unwrap();
        }
        for n in 1..=6 {
            std::fs::write(folder.join(format!("{}.jsonl", id(n))), "{}\n").unwrap();
            std::fs::create_dir_all(claude.join("session-env").join(id(n))).unwrap();
        }
        let live = HashSet::from([id(4)]);

        // A conversation goes on in the session run 1 started: it is not a leftover.
        db.execute("INSERT INTO conversation_sessions(conversation_id, session_id, updated_at_ms) VALUES ('c', ?1, 0)", [id(1)]).unwrap();

        assert_eq!(forget_done(&store, &claude, &live, None), 2);
        let left: Vec<String> = session::ids(&claude).into_iter().collect();
        assert_eq!(left, [id(1), id(3), id(4), id(5)], "the conversation's, the parked run, the live one and the stranger keep theirs");
        assert!(session::transcript(&claude, &id(3)).is_some());
    }

    /// Tom's live test: a daemon on a copy of the live data folder failed the
    /// copied runs and removed their sessions, which the live daemon still used.
    #[test]
    fn a_copied_database_leaves_the_sessions_it_brought_along() {
        use aurelius_storage::conversation_sessions::claim_root;
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path()).unwrap();
        let claude = root.path().join(".claude");
        let folder = claude.join("projects").join("C--work");
        std::fs::create_dir_all(&folder).unwrap();
        // A run id (UUIDv7) started at `ms`.
        let at = |ms: u64, n: u32| format!("{:08x}-{:04x}-7000-8000-{n:012}", ms >> 16, ms & 0xffff);
        // 1 ran in the live root before the copy; 2 ran in the copy. Both failed.
        let (before, after) = (at(1_000_000, 1), at(3_000_000, 2));
        let db = rusqlite::Connection::open(root.path().join("data/aurelius.sqlite")).unwrap();
        db.execute_batch("PRAGMA foreign_keys = OFF").unwrap();
        for id in [&before, &after] {
            db.execute(
                "INSERT INTO runs(id, conversation_id, attempt, runtime_kind, model_id, resolved_config_json, status) VALUES (?1, 'c', 1, 'claude_code', '', '{}', 'failed')",
                [id],
            )
            .unwrap();
            std::fs::write(folder.join(format!("{id}.jsonl")), "{}\n").unwrap();
        }
        let claim = |path: &'static str, default: bool, now: i64| store.write(move |tx| claim_root(tx, path, default, now)).unwrap();

        // The live root, this host's default: everything is its own, also on the next start.
        assert_eq!(claim("C:/live", true, 500), None);
        assert_eq!(claim("C:/live", true, 600), None);
        // The copy starts: what it brought along is not its own, now or later.
        assert_eq!(claim("C:/copy", false, 2_000_000), Some(2_000_000));
        assert_eq!(claim("C:/copy", false, 9_000_000), Some(2_000_000));
        assert!(started_since(&after, Some(2_000_000)) && !started_since(&before, Some(2_000_000)));
        assert!(!started_since("not-a-run", Some(1)) && started_since("not-a-run", None));

        assert_eq!(forget_done(&store, &claude, &HashSet::new(), Some(2_000_000)), 1);
        let left: Vec<String> = session::ids(&claude).into_iter().collect();
        assert_eq!(left, [before.clone()], "the live root's session stays");

        // An older database first opened outside the default root is a copy too.
        let other = tempfile::tempdir().unwrap();
        let fresh = Store::open(other.path()).unwrap();
        assert_eq!(fresh.write(|tx| claim_root(tx, "C:/test", false, 7)).unwrap(), Some(7));
    }

    #[test]
    fn sessions_of_conversations_that_are_over_are_taken() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path()).unwrap();
        let id = |n: u32| format!("0199c3a2-0000-7000-8000-{n:012}");
        let db = rusqlite::Connection::open(root.path().join("data/aurelius.sqlite")).unwrap();
        db.execute_batch("PRAGMA foreign_keys = OFF").unwrap();
        // 1 deleted, 2 its task ended (the user goes on talking in it), 3 goes on,
        // 4 held by a run that crashed, 5 held by a run that goes on, 6 a task
        // that was reopened after an earlier one ended.
        for (n, archived) in [(1, Some(5)), (2, None), (3, None), (4, None), (5, None), (6, None)] {
            db.execute(
                "INSERT INTO conversations(id, kind, project_id, title, version, created_at_ms, archived_at_ms) VALUES (?1, 'project', 'p', '', 1, 0, ?2)",
                rusqlite::params![format!("c{n}"), archived],
            )
            .unwrap();
        }
        for (task, conversation, status, created) in [("t2", "c2", "succeeded", 1), ("t6a", "c6", "cancelled", 1), ("t6b", "c6", "running", 2)] {
            db.execute(
                "INSERT INTO tasks(id, project_id, title, brief_blob_id, status, version, created_at_ms, updated_at_ms, conversation_id) VALUES (?1, 'p', '', 'b', ?2, 1, ?3, 0, ?4)",
                rusqlite::params![task, status, created, conversation],
            )
            .unwrap();
        }
        for (n, status) in [(4, "failed"), (5, "running")] {
            db.execute(
                "INSERT INTO runs(id, conversation_id, attempt, runtime_kind, model_id, resolved_config_json, status) VALUES (?1, ?2, 1, 'claude_code', '', '{}', ?3)",
                rusqlite::params![id(n), format!("c{n}"), status],
            )
            .unwrap();
        }
        for n in 1..=6 {
            let busy = matches!(n, 4 | 5).then(|| id(n));
            db.execute(
                "INSERT INTO conversation_sessions(conversation_id, session_id, busy_run_id, seen_digest, updated_at_ms) VALUES (?1, ?2, ?3, 'd', 0)",
                rusqlite::params![format!("c{n}"), format!("s{n}"), busy],
            )
            .unwrap();
        }
        let mut taken = store.write(|tx| aurelius_storage::conversation_sessions::take_over(tx, None, &[])).unwrap();
        taken.sort();
        assert_eq!(taken, ["s1", "s4"]);
        let left = store.read(aurelius_storage::conversation_sessions::session_ids).unwrap();
        assert_eq!(left, HashSet::from(["s2".to_string(), "s3".to_string(), "s5".to_string(), "s6".to_string()]));
        // Only the one conversation asked about.
        db.execute("UPDATE conversations SET archived_at_ms = 9 WHERE id IN ('c3', 'c6')", []).unwrap();
        let other = aurelius_domain::ConversationId::parse(&id(99)).unwrap();
        assert!(store.write(move |tx| aurelius_storage::conversation_sessions::take_over(tx, Some(other), &[])).unwrap().is_empty());
        // A run still streaming keeps its session, even when its status says otherwise.
        db.execute("UPDATE runs SET status = 'failed' WHERE id = ?1", [id(5)]).unwrap();
        let streaming = vec![id(5)];
        let mut taken = store.write(move |tx| aurelius_storage::conversation_sessions::take_over(tx, None, &streaming)).unwrap();
        taken.sort();
        assert_eq!(taken, ["s3", "s6"]);
    }
}
