//! The catalogue of start profiles (C6, P6): the texts live in `seed/employees/`
//! and are compiled in, like the guidelines' starter set.
//!
//! Hiring from a profile is idempotent the way that seed is. Nothing is written at
//! start and the catalogue is no table: a profile is copied once, at `employee.create`.
//! The same command replays its answer, the same name is refused because names are
//! unique (a second engineer needs a name of its own), and a newer text in the
//! catalogue never touches an employee already hired.

use crate::StorageError;
use aurelius_domain::{employee::AccessLevel, employee_tools::{StartProfile, ToolPolicy}};
use rusqlite::Connection;

/// One entry of the catalogue.
#[derive(Debug, Clone, Copy)]
pub struct Profile {
    pub key: &'static str,
    pub label: &'static str,
    pub summary: &'static str,
    pub name: &'static str,
    pub role: &'static str,
    pub title: Option<&'static str>,
    pub description: &'static str,
    pub instructions: &'static str,
    pub access: AccessLevel,
    /// None: every tool of its level.
    pub tools: Option<&'static [&'static str]>,
    pub approval_tools: &'static [&'static str],
}

impl Profile {
    pub fn policy(&self) -> ToolPolicy {
        let owned = |names: &[&str]| names.iter().map(|n| n.to_string()).collect::<Vec<_>>();
        ToolPolicy { tools: self.tools.map(owned), approval_tools: owned(self.approval_tools), mcp_servers: None }
    }
}

const CATALOGUE: [Profile; 5] = [
    Profile {
        key: "chief_of_staff",
        label: "Chief of staff",
        summary: "Runs the work and delegates it at once; no file or command tools.",
        name: "Cleo",
        role: "Chief of staff",
        title: Some("Chief of staff"),
        description: "Splits work into tickets, picks who does what and keeps track of it. Does no work on files itself.",
        instructions: include_str!("../seed/employees/chief-of-staff.md"),
        access: AccessLevel::Inherit,
        tools: Some(&[
            "mcp__aurelius__delegate_task",
            "mcp__aurelius__message_task",
            "mcp__aurelius__hand_over_task",
            "mcp__aurelius__create_ticket",
            "mcp__aurelius__update_ticket",
            "mcp__aurelius__move_ticket",
            "mcp__aurelius__post_to_ticket",
            "mcp__aurelius__schedule_message",
            "mcp__aurelius__propose_colleague",
        ]),
        approval_tools: &[],
    },
    Profile {
        key: "engineer",
        label: "Engineer",
        summary: "Takes a change from question to verified result, with every work tool.",
        name: "Ezra",
        role: "Engineer",
        title: None,
        description: "Builds and fixes things in the code, and verifies them with tests before reporting back.",
        instructions: include_str!("../seed/employees/engineer.md"),
        access: AccessLevel::Inherit,
        tools: None,
        approval_tools: &[],
    },
    Profile {
        key: "reviewer",
        label: "Reviewer",
        summary: "Reads a change and labels findings BLOCKER or NIT; runs no tests.",
        name: "Rhea",
        role: "Code reviewer",
        title: None,
        description: "Reviews a change before it goes in and labels its findings BLOCKER or NIT. Changes no code and runs no tests.",
        instructions: include_str!("../seed/employees/reviewer.md"),
        access: AccessLevel::Inherit,
        tools: Some(&["Read", "Glob", "Grep", "Bash", "WebFetch", "mcp__aurelius__post_to_ticket"]),
        approval_tools: &[],
    },
    Profile {
        key: "qa",
        label: "QA",
        summary: "Tests the real flows and moves a ticket only after a board member said yes.",
        name: "Quinn",
        role: "QA",
        title: None,
        description: "Tests finished changes through the real flows and reports results with evidence. Merges only after a board member agreed.",
        instructions: include_str!("../seed/employees/qa.md"),
        access: AccessLevel::Inherit,
        tools: Some(&["Read", "Glob", "Grep", "Bash", "WebFetch", "Task", "mcp__aurelius__post_to_ticket", "mcp__aurelius__update_ticket", "mcp__aurelius__move_ticket"]),
        approval_tools: &["mcp__aurelius__move_ticket"],
    },
    Profile {
        key: "frontend_designer",
        label: "Frontend designer",
        summary: "Designs and builds screens from the existing design system.",
        name: "Dara",
        role: "Frontend designer",
        title: None,
        description: "Designs and builds screens with the existing design system, and checks every state in the running app.",
        instructions: include_str!("../seed/employees/frontend-designer.md"),
        access: AccessLevel::Inherit,
        tools: None,
        approval_tools: &[],
    },
];

/// The catalogue, in the order the dialog offers it.
pub fn catalogue() -> &'static [Profile] {
    &CATALOGUE
}

pub fn find(key: &str) -> Option<&'static Profile> {
    CATALOGUE.iter().find(|p| p.key == key)
}

/// The employees (not archived) that started from this profile, oldest first.
pub fn hired(conn: &Connection, key: &str) -> Result<Vec<String>, StorageError> {
    let mut stmt = conn.prepare("SELECT name FROM employees WHERE json_extract(config_json, '$.start_profile') = ?1 AND status <> 'archived' ORDER BY created_at_ms, id")?;
    let names = stmt.query_map([key], |r| r.get(0))?.collect::<Result<_, _>>()?;
    Ok(names)
}

/// The catalogue as `employee.start_profiles` sends it.
pub fn list(conn: &Connection) -> Result<Vec<StartProfile>, StorageError> {
    CATALOGUE
        .iter()
        .map(|p| {
            Ok(StartProfile {
                key: p.key.to_owned(),
                label: p.label.to_owned(),
                summary: p.summary.to_owned(),
                name: p.name.to_owned(),
                role: p.role.to_owned(),
                title: p.title.map(str::to_owned),
                description: p.description.to_owned(),
                instructions: p.instructions.to_owned(),
                access_level: p.access,
                tool_policy: p.policy(),
                hired: hired(conn, p.key)?,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use aurelius_domain::employee::{self, DESCRIPTION_MAX};

    #[test]
    fn every_profile_would_be_accepted() {
        let mut keys = std::collections::HashSet::new();
        for p in catalogue() {
            assert!(keys.insert(p.key), "{} is in the catalogue once", p.key);
            employee::validate_fields(p.name, p.role, Some(p.instructions)).unwrap_or_else(|e| panic!("{}: {e:?}", p.key));
            assert!(p.description.chars().count() <= DESCRIPTION_MAX, "{}: description too long", p.key);
            assert!(!p.instructions.trim().is_empty(), "{}", p.key);
            let policy = p.policy().validate().unwrap_or_else(|e| panic!("{}: {e:?}", p.key));
            for tool in &policy.approval_tools {
                assert!(policy.allows(tool), "{}: {tool} asks first but is off", p.key);
            }
            assert_eq!(p.access, p.access.validate().unwrap());
        }
        assert_eq!(catalogue().len(), 5);
    }

    #[test]
    fn the_chief_of_staff_has_no_file_or_command_tools_and_the_reviewer_changes_nothing() {
        let cos = find("chief_of_staff").unwrap().policy();
        for tool in ["Read", "Edit", "Write", "Bash", "Task"] {
            assert!(!cos.allows(tool), "{tool}");
        }
        let reviewer = find("reviewer").unwrap().policy();
        assert!(reviewer.allows("Read") && reviewer.allows("Bash"));
        assert!(!reviewer.allows("Edit") && !reviewer.allows("Write"));
    }

    #[test]
    fn hired_names_come_from_the_stored_profile_and_leave_out_the_archived() {
        let dir = tempfile::tempdir().unwrap();
        let store = crate::Store::open(dir.path()).unwrap();
        store
            .write(|tx| {
                let global: String = tx.query_row("SELECT id FROM scopes WHERE kind = 'global'", [], |r| r.get(0))?;
                let insert = |id: &str, name: &str, status: &str, config: &str| -> Result<(), crate::StorageError> {
                    tx.execute("INSERT INTO scopes(id, kind, parent_id, owner_id) VALUES (?1, 'employee', ?2, ?1)", [id, global.as_str()])?;
                    tx.execute(
                        "INSERT INTO employees(id, scope_id, name, role, config_json, status, version, created_at_ms, updated_at_ms) VALUES (?1, ?1, ?2, 'r', ?3, ?4, 1, 1, 1)",
                        rusqlite::params![id, name, config, status],
                    )?;
                    Ok(())
                };
                insert("e1", "Ezra", "active", r#"{"start_profile":"engineer"}"#)?;
                insert("e2", "Old", "archived", r#"{"start_profile":"engineer"}"#)?;
                insert("e3", "Other", "active", "{}")?;
                Ok(())
            })
            .unwrap();
        let names = store.read(|c| hired(c, "engineer")).unwrap();
        assert_eq!(names, vec!["Ezra".to_owned()]);
        let listed = store.read(list).unwrap();
        assert_eq!(listed.iter().find(|p| p.key == "engineer").unwrap().hired, vec!["Ezra".to_owned()]);
        assert!(listed.iter().find(|p| p.key == "qa").unwrap().hired.is_empty());
    }
}
