//! Claude Code's permission questions during employee runs (approval bridge).
//! Each becomes an approval in the user's inbox; this waits for the decision.
//! No answer before the approval expires is a deny, never an allow.
//! A question (AskUserQuestion) is answered in the inbox; the answers go back
//! as the tool's updated input. A plan (ExitPlanMode) is approved, sent back
//! with feedback, or rejected in the inbox. Tools of the user's browser
//! always come here, also in auto mode. Screenshots (the bridge's own screen
//! tools) are allowed at once when the employee's Screen is on, else asked.
//! Aurelius' own memory and task tools are never asked, in any mode.
//! The plan of a ticket's main task is no inbox approval: it waits for the
//! board (`plan_review`) and Claude is told to stop.
//! An MCP tool the user answered with 'Always allow' (`mcp_always_allow`) is
//! let through in Ask, Accept edits and Auto, never in Plan; Aurelius' own
//! `mcp__aurelius__*` tools never qualify.
//! A mode switched to mid-answer reaches the running Claude Code here: what
//! the new mode covers goes ahead (also while it waits in the inbox) and the
//! session moves to that mode. Auto is decided here on every call from the
//! user's current mode (no allow rules in Claude Code), so a switch back to
//! Ask or Accept edits holds at once, also mid-answer.

use crate::Shared;
use aurelius_application::run::approvals::{PLAN_TOOL, QUESTION_TOOL, RuntimePermission, SCREEN_TOOLS};
use aurelius_domain::{RunId, approval::{ApprovalStatus, PlanDecision, always_allow_eligible}, autonomy::AgentMode};
use aurelius_protocol::{BridgeOutcome, ErrorCode, ProtocolError};
use aurelius_runtime::RuntimeInput;
use std::sync::Arc;
use std::time::Duration;

/// Whether a Bash command prints the environment or a vault variable
/// (`echo $GITHUB_TOKEN`, `printf '%s' "${GITHUB_TOKEN}"`, `env`, `printenv`,
/// `/proc/*/environ`), SEC0 E17. Defence in depth, not a boundary: a script,
/// `sh -c` or a rewritten value gets past it; what Aurelius keeps is redacted
/// anyway. `curl -H "Authorization: Bearer $TOKEN"` passes.
pub fn prints_vault_env(command: &str, names: &[String]) -> bool {
    let mentions = |segment: &str| {
        names.iter().any(|n| {
            let (plain, braced) = (format!("${n}"), format!("${{{n}"));
            segment.match_indices(&plain).any(|(i, _)| !segment[i + plain.len()..].starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_')) || segment.contains(&braced)
        })
    };
    command
        .split(|c| matches!(c, ';' | '|' | '&' | '\n' | '(' | ')' | '`'))
        .map(|segment| segment.trim().trim_start_matches('$').trim())
        .any(|segment| {
            let mut words = segment.split_whitespace().skip_while(|w| matches!(*w, "sudo" | "command" | "builtin" | "exec" | "nohup" | "time") || w.contains('='));
            let first = words.next().unwrap_or("").rsplit('/').next().unwrap_or("");
            let rest: Vec<&str> = words.collect();
            match first {
                "env" | "printenv" => rest.iter().all(|w| w.starts_with('-') || names.iter().any(|n| n == w)) || rest.is_empty(),
                "set" => rest.is_empty(),
                "export" | "declare" | "typeset" => rest.iter().any(|w| matches!(*w, "-p" | "-x" | "-px" | "-xp")),
                "echo" | "printf" | "print" | "cat" | "tee" | "logger" => mentions(segment) || (first == "cat" && segment.contains("/environ")),
                "strings" | "xargs" | "tr" | "od" | "xxd" | "base64" | "hexdump" => segment.contains("/environ") || mentions(segment),
                _ => segment.contains("/proc/") && segment.contains("environ"),
            }
        })
}

/// The env names of the vault's items (any scope): what `prints_vault_env` looks for.
async fn vault_names(shared: &Arc<Shared>) -> Vec<String> {
    let store = shared.store().clone();
    tokio::task::spawn_blocking(move || {
        store.read(|c| aurelius_storage::vault::env_names(c, None)).unwrap_or_default()
    })
    .await
    .unwrap_or_default()
}

pub async fn ask(shared: &Arc<Shared>, run_id: RunId, call_id: Option<String>, tool: String, input: serde_json::Value) -> Result<BridgeOutcome, ProtocolError> {
    // A command that would print the vault's values is refused in every mode
    // this is asked in (E17; in Auto the `--settings` deny rules do it).
    if tool == "Bash" {
        let command = input["command"].as_str().unwrap_or("");
        if !command.is_empty() && prints_vault_env(command, &vault_names(shared).await) {
            return Ok(deny("Aurelius refuses commands that print the environment or a vault variable. Use $NAME only inside the command that needs it (for example curl -H \"Authorization: Bearer $NAME\"), and never show its value.".into()));
        }
    }
    // A trial of a task (variants) runs unattended in its own worktree: nobody is
    // asked; what stays inside goes, anything outside is refused.
    let s = shared.clone();
    let probe = tokio::task::spawn_blocking(move || s.service.probe_worktree(run_id)).await.map_err(|e| ProtocolError::new(ErrorCode::Internal, e.to_string()))??;
    if let Some(worktree) = probe {
        return Ok(match aurelius_domain::variants::probe_verdict(&tool, &input, &worktree) {
            aurelius_domain::variants::ProbeVerdict::Allow => BridgeOutcome::Permission { allow: true, message: None, updated_input: None, updated_permissions: None },
            aurelius_domain::variants::ProbeVerdict::Deny(why) => deny(why),
        });
    }
    // Allowed by rule, but Claude Code's plan mode still asks for MCP tools.
    if aurelius_runtime::claude_code::MEMORY_TOOLS.contains(&tool.as_str()) {
        return Ok(BridgeOutcome::Permission { allow: true, message: None, updated_input: None, updated_permissions: None });
    }
    // Screenshots the user allowed for this employee need no question.
    if SCREEN_TOOLS.contains(&tool.as_str()) {
        let s = shared.clone();
        let allowed = tokio::task::spawn_blocking(move || s.service.run_screen_allowed(run_id))
            .await
            .map_err(|e| ProtocolError::new(ErrorCode::Internal, e.to_string()))??;
        if allowed {
            return Ok(BridgeOutcome::Permission { allow: true, message: None, updated_input: None, updated_permissions: None });
        }
    }
    // The safe read-only tools (server setting, on by default) pass in every mode but Plan.
    if aurelius_protocol::auto_allow_tool(&tool) && mode_now(shared, run_id).await? != AgentMode::Plan {
        let store = shared.store().clone();
        let on = tokio::task::spawn_blocking(move || store.read(aurelius_storage::auto_allow::enabled))
            .await
            .map_err(|e| ProtocolError::new(ErrorCode::Internal, e.to_string()))?
            .map_err(|e| ProtocolError::new(ErrorCode::Internal, e.to_string()))?;
        if on {
            return Ok(BridgeOutcome::Permission { allow: true, message: None, updated_input: None, updated_permissions: None });
        }
    }
    let question = tool == QUESTION_TOOL;
    let plan = tool == PLAN_TOOL;
    // The plan of a ticket's main task waits for the board, not in the inbox: Claude is told to stop (plan_review).
    if plan {
        let (s, plan_input) = (shared.clone(), input.clone());
        let handed_in = tokio::task::spawn_blocking(move || s.service.ticket_plan_handed_in(run_id, &plan_input)).await.map_err(|e| ProtocolError::new(ErrorCode::Internal, e.to_string()))??;
        if let Some((message, notices)) = handed_in {
            shared.publish(notices);
            return Ok(deny(message));
        }
    }
    // Files, commands and the web follow a mode the user switched to during this answer.
    // The browser follows the mode too: turning it on for the employee is the consent, Auto then does not ask again.
    let work = !question && !plan && !SCREEN_TOOLS.contains(&tool.as_str());
    let eligible = always_allow_eligible(&tool);
    if work {
        let running = shared.run_modes.lock().ok().and_then(|m| m.get(&run_id).copied());
        let s = shared.clone();
        let now = tokio::task::spawn_blocking(move || s.service.run_mode(run_id))
            .await
            .map_err(|e| ProtocolError::new(ErrorCode::Internal, e.to_string()))??;
        // The user's own MCP servers they enabled are let through in Auto like the browser: enabling them is the consent.
        let user_mcp = crate::mcp_servers::tool_is_enabled(shared, &tool).await;
        // Claude Code artifacts: turning them on is the consent for Auto; a publish
        // (or any other change on claude.ai) is an external action that Plan blocks.
        let artifact = aurelius_runtime::claude_code::artifact_call_reads(&tool, &input);
        if let Some(outcome) = artifact_in_plan(artifact, now) {
            return Ok(outcome);
        }
        let auto_allows = |t: &str| user_mcp || artifact.is_some() || auto_allows(t);
        // An MCP tool the user always allows passes in every mode but Plan.
        if now != AgentMode::Plan && always_allowed(shared, &tool).await? {
            return Ok(BridgeOutcome::Permission { allow: true, message: None, updated_input: None, updated_permissions: None });
        }
        match (running.filter(|r| *r != now).map(|_| now), now) {
            (Some(AgentMode::Plan), _) => return Ok(deny("The user switched to Plan mode: change and run nothing now. Finish your plan and present it with ExitPlanMode.".into())),
            (Some(mode), _) if covers(mode, &tool) => return take(shared, run_id, mode, None).await,
            // Auto holds only while the user's mode is Auto: decided here per call, so a switch back takes effect at once.
            (None, AgentMode::Auto) if auto_allows(&tool) => return Ok(BridgeOutcome::Permission { allow: true, message: None, updated_input: None, updated_permissions: None }),
            (Some(AgentMode::Auto), _) if auto_allows(&tool) => return take(shared, run_id, AgentMode::Auto, None).await,
            _ => {}
        }
    }
    // A plan handed in from Plan mode: a switch to a working mode approves it.
    let from_plan = plan && shared.run_modes.lock().ok().and_then(|m| m.get(&run_id).copied()) == Some(AgentMode::Plan);
    let (s, original, tool_name) = (shared.clone(), input.clone(), tool.clone());
    let asked = tokio::task::spawn_blocking(move || s.service.request_runtime_permission(run_id, call_id, &tool, &input))
        .await
        .map_err(|e| ProtocolError::new(ErrorCode::Internal, e.to_string()))??;
    let approval_id = match asked {
        RuntimePermission::Denied(message) => return Ok(deny(message)),
        RuntimePermission::Pending { approval_id, notices } => {
            shared.publish(notices);
            approval_id
        }
    };
    let mut shutdown = shared.shutdown.subscribe();
    loop {
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(400)) => {}
            _ = shutdown.changed() => return Ok(deny("Aurelius is stopping.".into())),
        }
        let s = shared.clone();
        let status = tokio::task::spawn_blocking(move || s.service.approval_status(approval_id))
            .await
            .map_err(|e| ProtocolError::new(ErrorCode::Internal, e.to_string()))??;
        if status == Some(ApprovalStatus::Pending) {
            // Still waiting while the user switches to a mode that covers it: go ahead.
            if work {
                if let Some(mode) = switched(shared, run_id).await?.filter(|m| covers(*m, &tool_name)) {
                    let s = shared.clone();
                    let allowed = tokio::task::spawn_blocking(move || s.service.allow_by_mode(approval_id, mode))
                        .await
                        .map_err(|e| ProtocolError::new(ErrorCode::Internal, e.to_string()))??;
                    if let Some(notices) = allowed {
                        shared.publish(notices);
                        return take(shared, run_id, mode, None).await;
                    }
                }
                // Always allowed meanwhile (by an answer to another ask of this tool).
                if eligible && always_allowed(shared, &tool_name).await? && mode_now(shared, run_id).await? != AgentMode::Plan {
                    let s = shared.clone();
                    let allowed = tokio::task::spawn_blocking(move || s.service.allow_always(approval_id))
                        .await
                        .map_err(|e| ProtocolError::new(ErrorCode::Internal, e.to_string()))??;
                    if let Some(notices) = allowed {
                        shared.publish(notices);
                        return Ok(BridgeOutcome::Permission { allow: true, message: None, updated_input: None, updated_permissions: None });
                    }
                }
            }
            if from_plan {
                let s = shared.clone();
                let now = tokio::task::spawn_blocking(move || s.service.run_mode(run_id))
                    .await
                    .map_err(|e| ProtocolError::new(ErrorCode::Internal, e.to_string()))??;
                if matches!(now, AgentMode::AcceptEdits | AgentMode::Auto) {
                    let s = shared.clone();
                    let allowed = tokio::task::spawn_blocking(move || s.service.allow_by_mode(approval_id, now))
                        .await
                        .map_err(|e| ProtocolError::new(ErrorCode::Internal, e.to_string()))??;
                    if let Some(notices) = allowed {
                        shared.publish(notices);
                        return take(shared, run_id, now, Some(original)).await;
                    }
                }
            }
            continue;
        }
        if question {
            return answer(shared, approval_id, status, original).await;
        }
        if plan {
            return plan_decision(shared, run_id, approval_id, status, original).await;
        }
        let (allow, message) = match status {
            Some(ApprovalStatus::Pending) => continue,
            Some(ApprovalStatus::Approved | ApprovalStatus::Consumed) => (true, None),
            Some(ApprovalStatus::Rejected) => (false, Some("The user rejected this action.".to_string())),
            Some(ApprovalStatus::Expired) => (false, Some("No answer in time; that never counts as permission.".to_string())),
            Some(ApprovalStatus::Invalidated) | None => (false, Some("This approval no longer applies.".to_string())),
        };
        // Approved after a switch to a looser mode: from now on that mode decides (e.g. Auto's own judgement).
        if allow && work {
            if let Some(mode) = switched(shared, run_id).await?.filter(|m| matches!(m, AgentMode::AcceptEdits | AgentMode::Auto)) {
                return take(shared, run_id, mode, None).await;
            }
        }
        return Ok(BridgeOutcome::Permission { allow, message, updated_input: None, updated_permissions: None });
    }
}

fn deny(message: String) -> BridgeOutcome {
    BridgeOutcome::Permission { allow: false, message: Some(message), updated_input: None, updated_permissions: None }
}

/// The mode that governs this run now.
async fn mode_now(shared: &Arc<Shared>, run_id: RunId) -> Result<AgentMode, ProtocolError> {
    let s = shared.clone();
    tokio::task::spawn_blocking(move || s.service.run_mode(run_id)).await.map_err(|e| ProtocolError::new(ErrorCode::Internal, e.to_string()))?
}

/// Whether the user chose 'Always allow' for this MCP tool (never Aurelius' own tools).
async fn always_allowed(shared: &Arc<Shared>, tool: &str) -> Result<bool, ProtocolError> {
    if !always_allow_eligible(tool) {
        return Ok(false);
    }
    let (s, tool) = (shared.clone(), tool.to_owned());
    tokio::task::spawn_blocking(move || s.service.always_allowed(&tool)).await.map_err(|e| ProtocolError::new(ErrorCode::Internal, e.to_string()))?
}

/// The mode the user switched this run to while it was running, if any.
/// Claude Code keeps the mode it started in; only this bridge can carry a switch over.
async fn switched(shared: &Arc<Shared>, run_id: RunId) -> Result<Option<AgentMode>, ProtocolError> {
    let Some(running) = shared.run_modes.lock().ok().and_then(|m| m.get(&run_id).copied()) else {
        return Ok(None);
    };
    let s = shared.clone();
    let now = tokio::task::spawn_blocking(move || s.service.run_mode(run_id))
        .await
        .map_err(|e| ProtocolError::new(ErrorCode::Internal, e.to_string()))??;
    Ok((now != running).then_some(now))
}

/// What Auto lets go ahead on a new call: the work tools and the browser the
/// user switched on for this employee, never questions, plans or anything else.
fn auto_allows(tool: &str) -> bool {
    aurelius_application::run::approvals::is_browser_tool(tool) || (aurelius_runtime::claude_code::WORK_TOOLS.contains(&tool) && !aurelius_runtime::claude_code::ALWAYS_ASKED_TOOLS.contains(&tool))
}

/// Plan mode refuses an artifact call that acts on claude.ai (publish, delete,
/// data writes, comment replies); reads and other tools are decided as usual.
fn artifact_in_plan(artifact_reads: Option<bool>, mode: AgentMode) -> Option<BridgeOutcome> {
    (artifact_reads == Some(false) && mode == AgentMode::Plan)
        .then(|| deny("Plan mode: publishing or changing artifacts on claude.ai is blocked. Describe the page in your plan; publish it once the plan is approved.".into()))
}

/// Whether a switched-to mode lets an approval that is already waiting go
/// ahead. A command the user is looking at is never waved through by a switch.
fn covers(mode: AgentMode, tool: &str) -> bool {
    match mode {
        AgentMode::AcceptEdits => matches!(tool, "Edit" | "Write"),
        AgentMode::Auto => matches!(tool, "Edit" | "Write" | "Read" | "Glob" | "Grep" | "WebSearch" | "WebFetch"),
        AgentMode::Ask | AgentMode::Plan => false,
    }
}

/// Allow this call and move Claude Code over to the user's new mode.
async fn take(shared: &Arc<Shared>, run_id: RunId, mode: AgentMode, updated_input: Option<serde_json::Value>) -> Result<BridgeOutcome, ProtocolError> {
    crate::dispatcher::started_in(shared, run_id, mode);
    Ok(BridgeOutcome::Permission { allow: true, message: None, updated_input, updated_permissions: Some(mode_permissions(mode)) })
}

/// Tell the running Claude Code processes about a mode the user switched to,
/// at once (a control request on their stdin), instead of after the answer.
pub fn forward_modes(shared: Arc<Shared>) {
    tokio::spawn(async move {
        let runs: Vec<(RunId, AgentMode)> = shared.run_modes.lock().map(|m| m.iter().map(|(r, m)| (*r, *m)).collect()).unwrap_or_default();
        for (run_id, mode) in runs {
            let s = shared.clone();
            let Ok(Ok(now)) = tokio::task::spawn_blocking(move || s.service.run_mode(run_id)).await else { continue };
            let sent = shared.run_switches.lock().ok().and_then(|m| m.get(&run_id).copied()).unwrap_or(mode);
            if now == sent {
                continue;
            }
            let Some(sender) = shared.run_inputs.lock().ok().and_then(|m| m.get(&run_id).cloned()) else { continue };
            if sender.try_send(RuntimeInput::SetPermissionMode { mode: session_mode(now).into() }).is_err() {
                continue;
            }
            if let Ok(mut m) = shared.run_switches.lock() {
                m.insert(run_id, now);
            }
            // Auto is Claude Code's manual mode with the bridge allowing the work;
            // the next call through the bridge (`take`) completes the switch.
            if now != AgentMode::Auto {
                crate::dispatcher::started_in(&shared, run_id, now);
            }
        }
    });
}

/// Claude Code's permission mode for a running session in an Aurelius mode.
fn session_mode(mode: AgentMode) -> &'static str {
    match mode {
        AgentMode::Ask | AgentMode::Auto => "manual",
        AgentMode::AcceptEdits => "acceptEdits",
        AgentMode::Plan => "plan",
    }
}

/// What switching a running session to `mode` changes (Claude Code's `updatedPermissions`).
/// Auto adds no allow rules (they would outlive a switch back to a stricter
/// mode): the session asks as in manual mode and the bridge allows the work.
fn mode_permissions(mode: AgentMode) -> serde_json::Value {
    let mode = match mode {
        AgentMode::Auto => "default",
        _ => crate::dispatcher::permission_mode(mode),
    };
    serde_json::json!([{ "type": "setMode", "mode": mode, "destination": "session" }])
}

/// Claude's plan was decided (or not). Approve: the plan goes ahead and the
/// session continues in Aurelius' Auto (the bridge allows the work tools; the
/// browser keeps asking).
/// Approve with edits: the plan goes ahead in accept-edits mode, so edits go
/// through and everything else is asked.
/// Revise and reject are denials carrying the user's words.
async fn plan_decision(shared: &Arc<Shared>, run_id: RunId, approval_id: aurelius_domain::ApprovalId, status: Option<ApprovalStatus>, input: serde_json::Value) -> Result<BridgeOutcome, ProtocolError> {
    let s = shared.clone();
    let answer = tokio::task::spawn_blocking(move || s.service.approval_answer(approval_id))
        .await
        .map_err(|e| ProtocolError::new(ErrorCode::Internal, e.to_string()))??;
    let decision = answer.as_ref().and_then(|a| serde_json::from_value::<PlanDecision>(a["decision"].clone()).ok());
    let feedback = answer.as_ref().and_then(|a| a["feedback"].as_str()).map(str::to_owned);
    // Approving also moved the conversation to that mode; the session now runs in it, so it is no switch.
    if matches!(status, Some(ApprovalStatus::Consumed | ApprovalStatus::Approved)) {
        match decision {
            Some(PlanDecision::Approve) => crate::dispatcher::started_in(shared, run_id, AgentMode::Auto),
            Some(PlanDecision::ApproveEdits) => crate::dispatcher::started_in(shared, run_id, AgentMode::AcceptEdits),
            _ => {}
        }
    }
    Ok(match (status, decision) {
        (Some(ApprovalStatus::Consumed | ApprovalStatus::Approved), Some(PlanDecision::Approve)) => BridgeOutcome::Permission {
            allow: true,
            message: None,
            updated_input: Some(input),
            updated_permissions: Some(mode_permissions(AgentMode::Auto)),
        },
        // Set explicitly: otherwise the session could stay in plan mode.
        (Some(ApprovalStatus::Consumed | ApprovalStatus::Approved), Some(PlanDecision::ApproveEdits)) => BridgeOutcome::Permission {
            allow: true,
            message: None,
            updated_input: Some(input),
            updated_permissions: Some(mode_permissions(AgentMode::AcceptEdits)),
        },
        (Some(ApprovalStatus::Rejected), Some(PlanDecision::Revise)) => deny(format!(
            "The user wants changes to this plan: {}\nRevise the plan and present it again with ExitPlanMode. Do not start the work yet.",
            feedback.unwrap_or_default()
        )),
        (Some(ApprovalStatus::Rejected), _) => deny(match feedback {
            Some(f) => format!("The user rejected this plan: {f}\nStop; do not start the work."),
            None => "The user rejected this plan. Stop; do not start the work.".into(),
        }),
        (Some(ApprovalStatus::Expired), _) => deny("No answer in time; the plan was not approved. Do not start the work.".into()),
        _ => deny("This plan no longer applies. Do not start the work.".into()),
    })
}

/// Claude Code's question was answered (or not): the answers become the
/// tool's input; otherwise Claude continues on its own judgment.
async fn answer(shared: &Arc<Shared>, approval_id: aurelius_domain::ApprovalId, status: Option<ApprovalStatus>, input: serde_json::Value) -> Result<BridgeOutcome, ProtocolError> {
    let s = shared.clone();
    let answers = tokio::task::spawn_blocking(move || s.service.approval_answer(approval_id))
        .await
        .map_err(|e| ProtocolError::new(ErrorCode::Internal, e.to_string()))??;
    let no_answer = |why: &str| Ok(deny(format!("{why} Continue with your best judgment and say what you assumed.")));
    match (status, answers) {
        (Some(ApprovalStatus::Consumed | ApprovalStatus::Approved), Some(answers)) => {
            let mut updated = input;
            updated["answers"] = answers;
            Ok(BridgeOutcome::Permission { allow: true, message: None, updated_input: Some(updated), updated_permissions: None })
        }
        (Some(ApprovalStatus::Rejected), _) => no_answer("The user chose not to answer."),
        (Some(ApprovalStatus::Expired), _) => no_answer("The user did not answer in time."),
        _ => no_answer("The question was not answered."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_switched_mode_covers_what_it_would_have_allowed() {
        assert!(covers(AgentMode::AcceptEdits, "Edit") && covers(AgentMode::AcceptEdits, "Write"));
        assert!(!covers(AgentMode::AcceptEdits, "Bash"));
        assert!(covers(AgentMode::Auto, "Write") && covers(AgentMode::Auto, "WebFetch"));
        // A command is judged once more (by the user), then by Claude Code's Auto mode.
        assert!(!covers(AgentMode::Auto, "Bash"));
        assert!(!covers(AgentMode::Ask, "Edit") && !covers(AgentMode::Plan, "Read"));
    }

    #[test]
    fn switching_moves_the_session_without_rules_that_outlive_the_mode() {
        // Auto: no allow rules (a switch back could not take them away); the bridge allows the work.
        assert_eq!(mode_permissions(AgentMode::Auto), serde_json::json!([{ "type": "setMode", "mode": "default", "destination": "session" }]));
        assert_eq!(mode_permissions(AgentMode::AcceptEdits), serde_json::json!([{ "type": "setMode", "mode": "acceptEdits", "destination": "session" }]));
        assert!(auto_allows("Bash") && auto_allows("Write") && auto_allows("WebFetch"));
        assert!(!auto_allows("AskUserQuestion") && !auto_allows("ExitPlanMode") && auto_allows("mcp__claude-in-chrome__navigate") && auto_allows("mcp__aurelius-browser__browser_click") && !auto_allows("mcp__other__x"));
    }

    #[test]
    fn plan_mode_blocks_artifact_publishes_but_not_reads() {
        let publish = aurelius_runtime::claude_code::artifact_call_reads("Artifact", &serde_json::json!({ "file_path": "a.html" }));
        let reply = aurelius_runtime::claude_code::artifact_call_reads("ArtifactComments", &serde_json::json!({ "action": "reply", "url": "u" }));
        let read = aurelius_runtime::claude_code::artifact_call_reads("Artifact", &serde_json::json!({ "action": "read", "url": "u" }));
        for acts in [publish, reply] {
            assert!(matches!(artifact_in_plan(acts, AgentMode::Plan), Some(BridgeOutcome::Permission { allow: false, .. })));
            for mode in [AgentMode::Ask, AgentMode::AcceptEdits, AgentMode::Auto] {
                assert!(artifact_in_plan(acts, mode).is_none(), "{mode:?} decides as usual");
            }
        }
        assert!(artifact_in_plan(read, AgentMode::Plan).is_none());
        assert!(artifact_in_plan(None, AgentMode::Plan).is_none(), "other tools are not artifacts");
        // Ask and Accept edits ask: a switch to Accept edits never waves a publish through.
        assert!(!covers(AgentMode::AcceptEdits, "Artifact") && !covers(AgentMode::Auto, "Artifact"));
        // Auto lets artifact calls through only because they are artifact calls (enabling them is the consent).
        assert!(!auto_allows("Artifact"));
    }
}

#[cfg(test)]
mod vault_env_tests {
    use super::prints_vault_env;

    /// SEC0 E17: printing a vault variable or the environment is refused, using it is not.
    #[test]
    fn printing_the_vault_or_the_environment_is_refused_using_it_is_not() {
        let names = vec!["GITHUB_TOKEN".to_owned(), "CANARY_TOKEN".to_owned()];
        for refused in [
            "echo $GITHUB_TOKEN",
            "echo \"token: ${GITHUB_TOKEN}\"",
            "printf '%s' \"$CANARY_TOKEN\"",
            "ls && echo $GITHUB_TOKEN | base64",
            "/bin/echo $GITHUB_TOKEN",
            "env",
            "env | grep TOKEN",
            "printenv",
            "printenv GITHUB_TOKEN",
            "export -p",
            "declare -x",
            "set",
            "cat /proc/self/environ",
            "strings /proc/1234/environ",
            "tr '\\0' '\\n' < /proc/self/environ",
            "echo $GITHUB_TOKEN > token.txt",
        ] {
            assert!(prints_vault_env(refused, &names), "{refused}");
        }
        for allowed in [
            "curl -H \"Authorization: Bearer $GITHUB_TOKEN\" https://api.example",
            "git push https://x:${GITHUB_TOKEN}@example/repo.git",
            "echo $GITHUB_TOKEN_URL_HINT",
            "echo hello",
            "env FOO=1 cargo test",
            "cargo test",
            "set -e",
            "export PATH=$PATH:/x",
        ] {
            assert!(!prints_vault_env(allowed, &names), "{allowed}");
        }
    }
}
