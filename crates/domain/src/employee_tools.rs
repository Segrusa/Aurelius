//! Which tools an employee has and which of them ask first (C2), and the
//! catalogue of start profiles a new employee can begin from (C6).
//!
//! The list is an allow-list and can only narrow what the access level gives:
//! no list is every tool of the level. Not every tool can be switched: asking the
//! board member, handing in a plan and what a run needs to read and report (its
//! own memory, its thread, handing over) stay on, or the platform rules in its
//! prompt would be wrong. See [`SWITCHABLE_TOOLS`].

use crate::{
    DomainError,
    employee::{AccessLevel, Setting},
};
use serde::{Deserialize, Serialize};

/// How Claude Code names a tool of Aurelius' bridge.
pub const BRIDGE_TOOL_PREFIX: &str = "mcp__aurelius__";

/// The most names in one list of a tool policy.
pub const TOOL_POLICY_MAX: usize = 64;
const TOOL_NAME_MAX: usize = 128;

/// A tool a board member can switch per employee.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SwitchableTool {
    /// As Claude Code names it.
    pub name: &'static str,
    pub label: &'static str,
    pub group: &'static str,
}

const fn tool(name: &'static str, label: &'static str, group: &'static str) -> SwitchableTool {
    SwitchableTool { name, label, group }
}

const FILES: &str = "Files";
const COMMANDS: &str = "Commands and web";
const COMPANY: &str = "Company";

/// Every tool that can be off or ask first, in the order the dialog shows them.
///
/// What is left out stays on and is never asked: `AskUserQuestion`,
/// `ExitPlanMode`, and of the bridge the employee's own memory and scratchpad,
/// reading its work (tasks, tickets, discussions, guidelines), `post_to_discussion`,
/// `hand_off`, `suggest_replies`, `set_mode` and
/// `report_test_result` and `report_ready`. They only touch the employee's own thread or read, and
/// the platform rules count on them.
pub const SWITCHABLE_TOOLS: &[SwitchableTool] = &[
    tool("Read", "Read files", FILES),
    tool("Glob", "Find files", FILES),
    tool("Grep", "Search in files", FILES),
    tool("Edit", "Edit files", FILES),
    tool("Write", "Write files", FILES),
    tool("Bash", "Run commands", COMMANDS),
    tool("WebSearch", "Search the web", COMMANDS),
    tool("WebFetch", "Fetch web pages", COMMANDS),
    tool("Task", "Start subagents", COMMANDS),
    tool("mcp__aurelius__delegate_task", "Delegate a task", COMPANY),
    tool("mcp__aurelius__message_task", "Message a task", COMPANY),
    tool("mcp__aurelius__hand_over_task", "Hand over a task", COMPANY),
    tool("mcp__aurelius__create_ticket", "Create tickets", COMPANY),
    tool("mcp__aurelius__update_ticket", "Update tickets", COMPANY),
    tool("mcp__aurelius__move_ticket", "Move tickets", COMPANY),
    tool("mcp__aurelius__post_to_ticket", "Post on other tickets", COMPANY),
    tool("mcp__aurelius__schedule_message", "Schedule messages", COMPANY),
    tool("mcp__aurelius__propose_colleague", "Propose a colleague", COMPANY),
    tool("mcp__aurelius__propose_instructions", "Propose instructions", COMPANY),
    tool("mcp__aurelius__propose_guideline", "Propose a guideline", COMPANY),
    tool("mcp__aurelius__vault_list", "List vault items", COMPANY),
    tool("mcp__aurelius__vault_read", "Use vault items", COMPANY),
];

/// Tools that are no longer offered. A list stored while they still were may name them: it stays
/// valid, and the next save drops them.
const RETIRED_TOOLS: &[&str] = &["mcp__aurelius__add_idea"];

pub fn is_switchable(name: &str) -> bool {
    SWITCHABLE_TOOLS.iter().any(|t| t.name == name)
}

/// `EmployeeConfig::tool_policy` (C2).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ToolPolicy {
    /// The switchable tools it has. None: all that its level gives.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<String>>,
    /// The tools that always go to a board member first, in every mode:
    /// a tool of [`SWITCHABLE_TOOLS`], or a whole MCP server as `mcp__<server>__*`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub approval_tools: Vec<String>,
    /// The MCP servers it may use, of those enabled server-wide. None: all of them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp_servers: Option<Vec<String>>,
}

impl ToolPolicy {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// Whether a run of this employee has `tool`. What cannot be switched is always there.
    pub fn allows(&self, tool: &str) -> bool {
        !is_switchable(tool) || self.tools.as_ref().is_none_or(|list| list.iter().any(|t| t == tool))
    }

    /// Whether `tool` must go to a board member first, whatever the mode.
    pub fn needs_approval(&self, tool: &str) -> bool {
        self.approval_tools.iter().any(|rule| match rule.strip_suffix('*') {
            Some(prefix) if prefix.starts_with("mcp__") => tool.starts_with(prefix),
            _ => rule == tool,
        })
    }

    /// Whether this employee may use the MCP server `name`.
    pub fn allows_server(&self, name: &str) -> bool {
        self.mcp_servers.as_ref().is_none_or(|list| list.iter().any(|s| s == name))
    }

    /// The bridge's tools (short names) that this list switches off.
    pub fn bridge_tools_off(&self) -> Vec<&'static str> {
        SWITCHABLE_TOOLS.iter().filter(|t| !self.allows(t.name)).filter_map(|t| t.name.strip_prefix(BRIDGE_TOOL_PREFIX)).collect()
    }

    /// The policy as stored: only names that mean something, sorted, each once.
    pub fn validate(mut self) -> Result<Self, DomainError> {
        let out_of_range = |detail: String| DomainError::OutOfRange { field: "tool_policy".into(), detail };
        for list in self.tools.iter_mut().chain(self.mcp_servers.iter_mut()).chain(std::iter::once(&mut self.approval_tools)) {
            if list.len() > TOOL_POLICY_MAX {
                return Err(out_of_range(format!("at most {TOOL_POLICY_MAX} names in a list")));
            }
            for name in list.iter_mut() {
                *name = name.trim().to_owned();
                if name.is_empty() || name.chars().count() > TOOL_NAME_MAX {
                    return Err(out_of_range(format!("a name is 1 to {TOOL_NAME_MAX} characters")));
                }
            }
            list.retain(|name| !RETIRED_TOOLS.contains(&name.as_str()));
            list.sort();
            list.dedup();
        }
        for name in self.tools.iter().flatten() {
            if !is_switchable(name) {
                return Err(out_of_range(format!("{name} is not a tool that can be switched")));
            }
        }
        for rule in &self.approval_tools {
            if !is_switchable(rule) && !is_user_server_rule(rule) {
                return Err(out_of_range(format!("{rule} cannot ask first")));
            }
        }
        Ok(self)
    }
}

/// `mcp__<server>__*`, or one tool of a server, for a server of the user's own.
fn is_user_server_rule(rule: &str) -> bool {
    let Some((server, tool)) = rule.strip_prefix("mcp__").and_then(|r| r.split_once("__")) else { return false };
    !server.is_empty() && !tool.is_empty() && !matches!(server, "aurelius" | "claude-in-chrome" | "aurelius-browser")
}

/// A stored `tool_policy`: anything else than a policy (an imported older value) is Inherit.
pub(crate) fn lenient_tool_policy<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Setting<ToolPolicy>, D::Error> {
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(serde_json::from_value(value).unwrap_or_default())
}

/// The policy in an employee's `config_json`.
pub fn tool_policy_in(config_json: &str) -> ToolPolicy {
    let config: serde_json::Value = serde_json::from_str(config_json).unwrap_or_default();
    match config.get("tool_policy").cloned().map(serde_json::from_value::<Setting<ToolPolicy>>) {
        Some(Ok(Setting::Value(policy))) => policy,
        Some(Ok(Setting::Disabled)) => ToolPolicy { tools: Some(Vec::new()), ..Default::default() },
        _ => ToolPolicy::default(),
    }
}

/// One line of `employee.tool_options`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ToolOption {
    pub name: String,
    pub label: String,
    pub group: String,
    /// The access levels that have this tool at all.
    pub levels: Vec<AccessLevel>,
}

/// `employee.tool_options` (feature `employee_tools`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ToolOptions {
    pub tools: Vec<ToolOption>,
}

/// One entry of the catalogue in the repo (`employee.start_profiles`, C6).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct StartProfile {
    /// Stable key for `employee.create` (`engineer`, `chief_of_staff`, ...).
    pub key: String,
    pub label: String,
    /// One sentence for the picker.
    pub summary: String,
    /// A suggested name; another employee may already have it.
    pub name: String,
    pub role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub description: String,
    pub instructions: String,
    #[serde(default, skip_serializing_if = "AccessLevel::is_inherit")]
    pub access_level: AccessLevel,
    #[serde(default, skip_serializing_if = "ToolPolicy::is_default")]
    pub tool_policy: ToolPolicy,
    /// The employees (not archived) that started from this profile.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hired: Vec<String>,
}

/// `employee.start_profiles` (feature `employee_profiles`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct StartProfiles {
    pub profiles: Vec<StartProfile>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(tools: Option<&[&str]>, approval: &[&str]) -> ToolPolicy {
        ToolPolicy { tools: tools.map(|t| t.iter().map(|s| s.to_string()).collect()), approval_tools: approval.iter().map(|s| s.to_string()).collect(), mcp_servers: None }
    }

    #[test]
    fn a_stored_list_that_names_a_retired_tool_stays_valid_and_loses_the_name() {
        let saved = policy(Some(&["Read", "mcp__aurelius__add_idea"]), &["mcp__aurelius__add_idea"]).validate().unwrap();
        assert_eq!(saved, policy(Some(&["Read"]), &[]));
        assert!(!is_switchable("mcp__aurelius__add_idea"));
    }

    #[test]
    fn without_a_list_everything_is_on_and_nothing_asks() {
        let open = ToolPolicy::default();
        assert!(open.allows("Bash") && open.allows("mcp__aurelius__move_ticket"));
        assert!(!open.needs_approval("Bash"));
        assert!(open.allows_server("github"));
        assert!(open.bridge_tools_off().is_empty());
    }

    #[test]
    fn a_list_switches_off_what_it_leaves_out_and_never_what_cannot_be_switched() {
        let p = policy(Some(&["Read", "mcp__aurelius__move_ticket"]), &[]);
        assert!(p.allows("Read") && p.allows("mcp__aurelius__move_ticket"));
        assert!(!p.allows("Bash") && !p.allows("mcp__aurelius__create_ticket"));
        // The run's way to ask and its own memory stay.
        assert!(p.allows("AskUserQuestion") && p.allows("mcp__aurelius__remember") && p.allows("mcp__aurelius__hand_off"));
        let off = p.bridge_tools_off();
        assert!(off.contains(&"create_ticket") && !off.contains(&"move_ticket") && !off.contains(&"remember"));
    }

    #[test]
    fn a_server_rule_covers_its_tools_only() {
        let p = policy(None, &["mcp__github__*", "mcp__aurelius__vault_read"]);
        assert!(p.needs_approval("mcp__github__create_issue") && p.needs_approval("mcp__aurelius__vault_read"));
        assert!(!p.needs_approval("mcp__githubber__x") && !p.needs_approval("mcp__aurelius__vault_list") && !p.needs_approval("Bash"));
    }

    #[test]
    fn validate_sorts_and_refuses_what_means_nothing() {
        let ok = policy(Some(&["Read", "Bash", "Read"]), &["Bash", "mcp__github__*"]).validate().unwrap();
        assert_eq!(ok.tools.unwrap(), vec!["Bash".to_owned(), "Read".to_owned()]);
        for bad in [policy(Some(&["AskUserQuestion"]), &[]), policy(Some(&["mcp__aurelius__remember"]), &[]), policy(None, &["mcp__aurelius__remember"]), policy(None, &["mcp__aurelius__*"]), policy(None, &["mcp__claude-in-chrome__*"]), policy(None, &["Nope"])] {
            assert!(bad.clone().validate().is_err(), "{bad:?}");
        }
        let too_many = ToolPolicy { mcp_servers: Some((0..=TOOL_POLICY_MAX).map(|n| format!("s{n}")).collect()), ..Default::default() };
        assert!(too_many.validate().is_err());
    }

    #[test]
    fn a_stored_value_that_is_no_policy_reads_as_inherit() {
        assert_eq!(tool_policy_in(r#"{"tool_policy":{"mode":"value","value":"x"}}"#), ToolPolicy::default());
        assert_eq!(tool_policy_in(r#"{"tool_policy":{"mode":"inherit"}}"#), ToolPolicy::default());
        assert_eq!(tool_policy_in("{}"), ToolPolicy::default());
        let stored = tool_policy_in(r#"{"tool_policy":{"mode":"value","value":{"tools":["Read"]}}}"#);
        assert_eq!(stored.tools.unwrap(), vec!["Read".to_owned()]);
        assert_eq!(tool_policy_in(r#"{"tool_policy":{"mode":"disabled"}}"#).tools.unwrap(), Vec::<String>::new());
    }

    #[test]
    fn an_old_config_still_loads() {
        let config: crate::employee::EmployeeConfig = serde_json::from_value(serde_json::json!({
            "identity": "i", "memory_policy": {"mode": "inherit"}, "tool_policy": {"mode": "value", "value": "x"},
            "autonomy_policy": {"mode": "inherit"}, "runtime_selection": {"mode": "inherit"},
            "task_policy": {"delegation_allowed": false, "max_depth": 4}
        })).unwrap();
        assert_eq!(config.task_policy.max_depth, 4, "the task policy of an imported employee survives");
        assert_eq!(config.tool_policy, Setting::Inherit);
    }
}
