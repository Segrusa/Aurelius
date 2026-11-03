//! A local screen counter (ticket #19): how often each screen opens on this device.
//!
//! Only counts, kept in `<data dir>/screen_use.json` next to `ui.json`. It never leaves
//! this computer: no network, no server, no event, and the keys are fixed names without
//! ids or project names ("project.board", "employees.guidelines").

use std::collections::BTreeMap;
use std::sync::Mutex;

use crate::app::{EmployeesTab, Page, Panel, ProjectsTab, Tab};

/// The file name in the client's data folder.
pub const SCREEN_USE_FILE: &str = "screen_use.json";
/// The counter keeps at most this many screens; a new one beyond it is not counted.
pub const SCREEN_USE_MAX: usize = 64;
/// The longest key kept (the keys are fixed names, this only guards a damaged file).
pub const SCREEN_KEY_MAX: usize = 48;
/// A change is written this long after the last one, not on every switch.
pub const SCREEN_USE_SAVE_DELAY_MS: u64 = 5_000;

/// What is known about one screen.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Use {
    /// How many times it opened.
    pub opens: u64,
    /// When it opened first (ms since the epoch).
    pub first_ms: i64,
    /// When it opened last (ms since the epoch).
    pub last_ms: i64,
}

/// The counts of every screen, by key.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct ScreenUse {
    pub screens: BTreeMap<String, Use>,
}

impl ScreenUse {
    /// Count one open of `key` at `now_ms`, unless it is the screen that was already
    /// showing (`previous`). Returns whether it counted.
    pub fn note(&mut self, previous: Option<&str>, key: &str, now_ms: i64) -> bool {
        if previous == Some(key) || key.is_empty() || key.len() > SCREEN_KEY_MAX {
            return false;
        }
        if !self.screens.contains_key(key) && self.screens.len() >= SCREEN_USE_MAX {
            return false;
        }
        let entry = self.screens.entry(key.to_string()).or_insert(Use { opens: 0, first_ms: now_ms, last_ms: now_ms });
        entry.opens = entry.opens.saturating_add(1);
        entry.last_ms = now_ms;
        true
    }

    /// The screens, the most opened first (ties by key).
    pub fn sorted(&self) -> Vec<(String, Use)> {
        let mut rows: Vec<(String, Use)> = self.screens.iter().map(|(k, u)| (k.clone(), *u)).collect();
        rows.sort_by(|a, b| b.1.opens.cmp(&a.1.opens).then_with(|| a.0.cmp(&b.0)));
        rows
    }

    /// The file's content: `{"screens": {"board": {"opens": 3, "first_ms": 1, "last_ms": 2}}}`.
    pub fn to_json(&self) -> serde_json::Value {
        let screens: serde_json::Map<String, serde_json::Value> = self
            .screens
            .iter()
            .map(|(k, u)| (k.clone(), serde_json::json!({ "opens": u.opens, "first_ms": u.first_ms, "last_ms": u.last_ms })))
            .collect();
        serde_json::json!({ "screens": screens })
    }

    /// Read the file's content; whatever is damaged or over the limits is left out.
    pub fn from_json(value: &serde_json::Value) -> ScreenUse {
        let mut out = ScreenUse::default();
        for (key, u) in value["screens"].as_object().into_iter().flatten() {
            if key.is_empty() || key.len() > SCREEN_KEY_MAX || out.screens.len() >= SCREEN_USE_MAX {
                continue;
            }
            let Some(opens) = u["opens"].as_u64() else { continue };
            out.screens.insert(
                key.clone(),
                Use { opens, first_ms: u["first_ms"].as_i64().unwrap_or(0), last_ms: u["last_ms"].as_i64().unwrap_or(0) },
            );
        }
        out
    }
}

/// The key of the page showing: a fixed name, never an id or a name.
pub fn page_key(page: Page, employees: EmployeesTab, projects: ProjectsTab) -> &'static str {
    match page {
        Page::Dashboard => "dashboard",
        Page::Board => "board",
        Page::GroupChat => "team_chat",
        Page::Employees => match employees {
            EmployeesTab::List => "employees.list",
            EmployeesTab::OrgChart => "employees.org_chart",
            EmployeesTab::Guidelines => "employees.guidelines",
        },
        Page::Employee(_) => "employee",
        Page::Projects => match projects {
            ProjectsTab::Projects => "projects.list",
            ProjectsTab::Ideas => "projects.ideas",
        },
        Page::Project(_, tab) => match tab {
            Tab::Overview => "project.overview",
            Tab::Board => "project.board",
            Tab::Tasks => "project.tasks",
            Tab::Ideas => "project.ideas",
            Tab::Memory => "project.memory",
            Tab::Vault => "project.vault",
        },
        Page::Tasks => "tasks",
        Page::Settings => "settings",
        Page::Vault => "vault",
    }
}

/// The key of the panel beside the page, if one shows.
pub fn panel_key(panel: Panel) -> Option<&'static str> {
    match panel {
        Panel::None => None,
        Panel::Chat(_) => Some("panel.chat"),
        Panel::Task(_) => Some("panel.task"),
        Panel::Ticket(_) => Some("panel.ticket"),
        Panel::Thread(_) => Some("panel.thread"),
    }
}

/// Every window of the app counts into the same counts.
static COUNTS: Mutex<Option<ScreenUse>> = Mutex::new(None);

fn path() -> Option<std::path::PathBuf> {
    aurelius_host::data_dir().ok().map(|d| d.join(SCREEN_USE_FILE))
}

fn with_counts<R>(f: impl FnOnce(&mut ScreenUse) -> R) -> R {
    let mut guard = COUNTS.lock().unwrap_or_else(|e| e.into_inner());
    let counts = guard.get_or_insert_with(|| {
        path()
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
            .map(|v| ScreenUse::from_json(&v))
            .unwrap_or_default()
    });
    f(counts)
}

/// Count an open of `key` (see `ScreenUse::note`). The caller saves later with `save`.
pub fn note(previous: Option<&str>, key: &str) -> bool {
    with_counts(|c| c.note(previous, key, crate::ui::now_ms()))
}

/// A copy of the counts, for the Settings table.
pub fn snapshot() -> ScreenUse {
    with_counts(|c| c.clone())
}

/// Forget every count (and write the empty file).
pub fn reset() {
    with_counts(|c| c.screens.clear());
    save();
}

/// Write the counts: to a temporary file, then renamed over the real one.
pub fn save() {
    let Some(path) = path() else { return };
    let text = with_counts(|c| c.to_json().to_string());
    let tmp = path.with_extension("json.tmp");
    if std::fs::write(&tmp, text).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_switch_counts_and_keeps_the_first_and_last_time() {
        let mut c = ScreenUse::default();
        assert!(c.note(None, "board", 10));
        assert!(c.note(Some("tasks"), "board", 20));
        assert_eq!(c.screens["board"], Use { opens: 2, first_ms: 10, last_ms: 20 });
    }

    #[test]
    fn the_same_screen_again_is_not_a_switch() {
        let mut c = ScreenUse::default();
        assert!(!c.note(Some("board"), "board", 10));
        assert!(c.screens.is_empty());
    }

    #[test]
    fn the_counts_stay_inside_their_limits() {
        let mut c = ScreenUse::default();
        for i in 0..SCREEN_USE_MAX {
            assert!(c.note(None, &format!("s{i}"), 1));
        }
        assert!(!c.note(None, "one.more", 1));
        assert!(c.note(None, "s0", 2));
        assert!(!c.note(None, &"x".repeat(SCREEN_KEY_MAX + 1), 1));
        assert_eq!(c.screens.len(), SCREEN_USE_MAX);
    }

    #[test]
    fn the_table_is_sorted_by_opens() {
        let mut c = ScreenUse::default();
        c.note(None, "b", 1);
        c.note(None, "a", 1);
        c.note(None, "a", 1);
        c.note(None, "c", 1);
        let keys: Vec<String> = c.sorted().into_iter().map(|(k, _)| k).collect();
        assert_eq!(keys, ["a", "b", "c"]);
    }

    #[test]
    fn the_file_round_trips_and_skips_what_is_damaged() {
        let mut c = ScreenUse::default();
        c.note(None, "project.board", 5);
        c.note(None, "settings", 6);
        assert_eq!(ScreenUse::from_json(&c.to_json()), c);
        let damaged = serde_json::json!({ "screens": { "ok": { "opens": 2 }, "bad": { "opens": "x" }, "": { "opens": 1 } } });
        let read = ScreenUse::from_json(&damaged);
        assert_eq!(read.screens.len(), 1);
        assert_eq!(read.screens["ok"].opens, 2);
        assert!(ScreenUse::from_json(&serde_json::json!(null)).screens.is_empty());
    }

    #[test]
    fn the_keys_hold_no_ids() {
        assert_eq!(page_key(Page::Project(crate::app::ProjectRef::Sample(0), Tab::Board), EmployeesTab::List, ProjectsTab::Projects), "project.board");
        assert_eq!(page_key(Page::Employees, EmployeesTab::Guidelines, ProjectsTab::Projects), "employees.guidelines");
    }
}
