//! What a run is given: its tools, permission mode and approval bridge.

use super::*;

/// What the runtime's own tools may do in this run. Only Claude Code has
/// tools of its own; other runtimes keep the text-only profile.
pub(super) fn tool_profile(shared: &Arc<Shared>, plan: &ChatRunPlan) -> ToolProfile {
    if plan.config.provider != Some(aurelius_domain::connection::ProviderKind::ClaudeCode) {
        return ToolProfile::TextOnly;
    }
    match &plan.work {
        Some((folders, mode)) => {
            started_in(shared, plan.run_id, *mode);
            ToolProfile::Work {
                folders: folders.clone(),
                permission_mode: permission_mode(*mode).into(),
                // An employee at work also has its own memory (the bridge always serves it).
                approval_bridge: {
                    let bridge = approval_bridge(shared, plan.run_id);
                    // The project assistant gives work to employees but cannot hand a task over.
                    let bridge = if plan.project_assistant { with_flag(bridge, "--project") } else { bridge };
                    // Management that chooses the mode of its task gets the set_mode tool.
                    if plan.management_mode { with_flag(bridge, "--set-mode") } else { bridge }
                },
                browser: plan.browser,
                browser_session: plan.browser_session.clone(),
                // Filled in by the dispatcher once Aurelius' browser is up (Playwright backend).
                playwright: None,
                // The user's own MCP servers the user enabled server-wide; none by default.
                mcp_servers: crate::mcp_servers::configs_for_run(shared),
                // The plugins this employee, or for the project assistant this project, has enabled.
                plugins: aurelius_host::plugins::paths_for_run(shared.store(), plan.conversation_id),
                // Claude Code artifacts (pages on claude.ai), when the user turned them on server-wide.
                artifacts: plan.artifacts,
            }
        }
        None => ToolProfile::TextOnly,
    }
}

/// Claude Code's `--permission-mode` for an Aurelius working mode.
pub(crate) fn permission_mode(mode: aurelius_domain::autonomy::AgentMode) -> &'static str {
    use aurelius_domain::autonomy::AgentMode;
    match mode {
        AgentMode::Ask => "manual",
        AgentMode::AcceptEdits => "acceptEdits",
        AgentMode::Auto => "auto",
        AgentMode::Plan => "plan",
    }
}

/// Remember the mode a run's Claude Code process starts in (or was switched to).
pub(crate) fn started_in(shared: &Shared, run_id: aurelius_domain::RunId, mode: aurelius_domain::autonomy::AgentMode) {
    if let Ok(mut map) = shared.run_modes.lock() {
        map.insert(run_id, mode);
    }
}

/// The approval bridge for one run: `aurelius-bridge`, the MCP server Claude Code
/// starts; it asks the user in Aurelius and serves the employee's own tools.
pub(crate) fn approval_bridge(shared: &Shared, run_id: aurelius_domain::RunId) -> ApprovalBridge {
    ApprovalBridge {
        program: bridge_program(),
        args: vec!["--data-dir".into(), shared.root.to_string_lossy().into_owned(), "--run".into(), run_id.to_string()],
    }
}

/// The bridge with an extra flag (`--project`: the project assistant).
pub(crate) fn with_flag(mut bridge: ApprovalBridge, flag: &str) -> ApprovalBridge {
    bridge.args.push(flag.into());
    bridge
}

/// aurelius-bridge next to this program (installed app and dev builds alike).
fn bridge_program() -> std::path::PathBuf {
    let exe = std::env::current_exe().unwrap_or_default();
    let name = if cfg!(windows) { "aurelius-bridge.exe" } else { "aurelius-bridge" };
    let mut dir = exe.parent().map(std::path::Path::to_path_buf).unwrap_or_default();
    // Test binaries live one folder deeper (target/debug/deps).
    if !dir.join(name).exists() {
        if let Some(parent) = dir.parent() {
            dir = parent.to_path_buf();
        }
    }
    dir.join(name)
}
