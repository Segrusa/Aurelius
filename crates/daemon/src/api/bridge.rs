//! The internal bridge channel: what the agent bridge (`aurelius-bridge`, the MCP
//! server Claude Code starts for a run) and `aurelius-ctl shutdown` ask of the
//! daemon. It has its own local endpoint (`aurelius_host::bridge_endpoint`), is never
//! reachable from the client protocol and never goes through the remote gateway.

use super::{app_task, sql, storage_task};
use crate::Shared;
use aurelius_application::task::task_runs::EmployeeAction;
use aurelius_host::ipc::{BoxStream, IpcError, read_frame, write_frame};
use aurelius_protocol::{BridgeCall, BridgeOutcome, BridgeRequest, BridgeResponse, EmployeeTaskOp, PROTOCOL_MAJOR};
use std::sync::Arc;

/// One bridge connection: requests in order, a response to each.
pub(crate) async fn serve(shared: Arc<Shared>, stream: BoxStream) {
    let (mut reader, mut writer) = tokio::io::split(stream);
    loop {
        let request: BridgeRequest = match read_frame(&mut reader).await {
            Ok(Some(request)) => request,
            Ok(None) | Err(IpcError::Io(_)) => break,
            Err(IpcError::Frame(err)) => {
                let _ = write_frame(&mut writer, &BridgeResponse { request_id: 0, outcome: BridgeOutcome::Error { message: err.to_string() } }).await;
                break;
            }
        };
        let request_id = request.request_id;
        let outcome = answer(&shared, request).await;
        if write_frame(&mut writer, &BridgeResponse { request_id, outcome }).await.is_err() {
            break;
        }
    }
}

/// A request with every vault value in it redacted (SEC0 E14 b): what a run
/// sends through any of Aurelius' tools (`approve` included, so approvals, the
/// inbox and push), before anything stores or forwards it. Fail-closed (SEC1
/// m2): a call that cannot be redacted is refused, never let through as it is.
pub(crate) fn redacted<T: serde::Serialize + serde::de::DeserializeOwned>(call: T) -> Result<T, String> {
    let redactor = aurelius_runtime::redact::current();
    if redactor.is_empty() {
        return Ok(call);
    }
    let refused = |_| "This request could not be checked for vault values, so it was not carried out.".to_owned();
    let mut value = serde_json::to_value(&call).map_err(refused)?;
    redactor.redact_json(&mut value);
    serde_json::from_value(value).map_err(refused)
}

async fn answer(shared: &Arc<Shared>, request: BridgeRequest) -> BridgeOutcome {
    if request.protocol != PROTOCOL_MAJOR {
        return BridgeOutcome::Error {
            message: format!("Update Aurelius: this bridge speaks protocol {}, the server protocol {PROTOCOL_MAJOR}.", request.protocol),
        };
    }
    let call = match redacted(request.call) {
        Ok(call) => call,
        Err(message) => return BridgeOutcome::Error { message },
    };
    let request = BridgeRequest { call, ..request };
    // A trial of a task (variants) may only read with Aurelius' own tools: Claude Code
    // never asks for them, so the sandbox is kept here.
    let own_tool = match &request.call {
        BridgeCall::MemoryRun { run_id, op } => Some((*run_id, match op {
            aurelius_domain::memory::EmployeeMemoryOp::Remember { .. } => "remember",
            aurelius_domain::memory::EmployeeMemoryOp::Recall { .. } => "recall",
        })),
        // Only the reading ones pass; anything that acts (delegating, ideas, proposals, …) is refused.
        BridgeCall::EmployeeAct { run_id, op } => Some((*run_id, match op {
            EmployeeTaskOp::ListTasks { .. } => "list_tasks",
            EmployeeTaskOp::ReadTask { .. } => "read_task",
            EmployeeTaskOp::ListConversations { .. } => "list_conversations",
            EmployeeTaskOp::ReadConversation { .. } => "read_conversation",
            EmployeeTaskOp::ReadGroupChat { .. } => "read_group_chat",
            EmployeeTaskOp::ReadDiscussion { .. } => "read_discussion",
            EmployeeTaskOp::VaultList { .. } => "vault_list",
            EmployeeTaskOp::VaultRead { .. } => "vault_read",
            _ => "act",
        })),
        _ => None,
    };
    if let Some((run_id, name)) = own_tool {
        let s = shared.clone();
        if let Ok(Some(worktree)) = app_task(move || s.service.probe_worktree(run_id)).await {
            let tool = format!("mcp__aurelius__{name}");
            if let aurelius_domain::variants::ProbeVerdict::Deny(why) = aurelius_domain::variants::probe_verdict(&tool, &serde_json::json!({}), &worktree) {
                return BridgeOutcome::Text { ok: false, text: why };
            }
        }
    }
    let outcome = match request.call {
        BridgeCall::PermissionAsk { run_id, call_id, tool, input } => crate::permissions::ask(shared, run_id, call_id, tool, input).await,
        BridgeCall::MemoryRun { run_id, op } => {
            let s = shared.clone();
            match app_task(move || s.service.employee_memory(run_id, op)).await {
                Ok((ok, text, notices)) => {
                    shared.publish(notices);
                    Ok(BridgeOutcome::Text { ok, text })
                }
                Err(err) => Err(err),
            }
        }
        BridgeCall::EmployeeAct { run_id, op } => employee_act(shared, run_id, op).await,
        BridgeCall::DaemonShutdown => {
            let _ = shared.shutdown.send(true);
            Ok(BridgeOutcome::Done)
        }
    };
    outcome.unwrap_or_else(|err| BridgeOutcome::Error { message: err.message })
}

async fn employee_act(shared: &Arc<Shared>, run_id: aurelius_domain::RunId, op: EmployeeTaskOp) -> Result<BridgeOutcome, aurelius_protocol::ProtocolError> {
    let action = match op {
        EmployeeTaskOp::Delegate { employee, title, goal, criteria, project } => EmployeeAction::Delegate { employee, title, goal, criteria, project },
        EmployeeTaskOp::HandOver { employee, reason } => EmployeeAction::HandOver { employee, reason },
        EmployeeTaskOp::MessageTask { task, message, interrupt } => EmployeeAction::MessageTask { task, message, interrupt },
        EmployeeTaskOp::AddIdea { title, note, kind, project } => EmployeeAction::AddIdea { title, note, kind, project },
        EmployeeTaskOp::ListTasks { status, employee, query, project } => EmployeeAction::ListTasks { status, employee, query, project },
        EmployeeTaskOp::ReadTask { task } => EmployeeAction::ReadTask { task },
        EmployeeTaskOp::ListConversations { query } => EmployeeAction::ListConversations { query },
        EmployeeTaskOp::ReadConversation { conversation } => EmployeeAction::ReadConversation { conversation },
        EmployeeTaskOp::ProposeColleague { name, role, title, instructions, reports_to, reason } => {
            EmployeeAction::ProposeColleague { name, role, title, instructions, reports_to, reason }
        }
        EmployeeTaskOp::ProposeInstructions { employee, instructions, reason, evidence } => EmployeeAction::ProposeInstructions { employee, instructions, reason, evidence },
        EmployeeTaskOp::SetMode { mode, reason } => EmployeeAction::SetMode { mode, reason },
        EmployeeTaskOp::ReadGroupChat { last, channel } => EmployeeAction::ReadGroupChat { last, channel },
        EmployeeTaskOp::PostToGroupChat { message, channel } => EmployeeAction::PostToGroupChat { message, channel },
        EmployeeTaskOp::ReadDiscussion { last } => EmployeeAction::ReadDiscussion { last },
        EmployeeTaskOp::PostToDiscussion { message } => EmployeeAction::PostToDiscussion { message },
        EmployeeTaskOp::AssignTicket { employee, reason } => EmployeeAction::AssignTicket { employee, reason },
        op @ (EmployeeTaskOp::VaultList { .. } | EmployeeTaskOp::VaultRead { .. }) => {
            let s = shared.clone();
            let (ok, text) = app_task(move || Ok(crate::vault::tool(&s, run_id, op))).await?;
            return Ok(BridgeOutcome::Text { ok, text });
        }
    };
    let switches = matches!(action, EmployeeAction::SetMode { .. });
    let interrupts = matches!(action, EmployeeAction::MessageTask { interrupt: true, .. });
    let s = shared.clone();
    let (ok, text, notices) = app_task(move || s.service.employee_action(run_id, action)).await?;
    if ok && interrupts {
        // "Send now" for a colleague's task: the runs it stopped are interrupted at once.
        let store = shared.store().clone();
        let stopping = storage_task(move || store.read(sql::cancelling_runs)).await?;
        for id in stopping {
            if let Some(canceller) = shared.cancellers.lock().ok().and_then(|m| m.get(&id).cloned()) {
                canceller.cancel();
            }
        }
    }
    shared.publish(notices);
    if ok && switches {
        // The mode changed under the running Claude Code: tell it at once (the work of Followup::AgentModeChanged).
        crate::permissions::forward_modes(shared.clone());
    }
    Ok(BridgeOutcome::Text { ok, text })
}

#[cfg(test)]
mod tests {
    /// SEC1 m2: what cannot be redacted is refused, not passed on unredacted.
    #[test]
    fn a_call_that_cannot_be_redacted_is_refused() {
        let owner = "bridge-fail-closed-test";
        let value = "sk-test-FAKE-bridge-fail-closed";
        aurelius_runtime::redact::add_to_set(owner, aurelius_runtime::redact::Secret { name: "BRIDGE_FC".into(), label: String::new(), value: zeroize::Zeroizing::new(value.into()) });
        // A map with keys that are not strings serializes to no JSON value.
        let call: std::collections::BTreeMap<(u8, u8), String> = [((1, 2), value.to_owned())].into();
        let outcome = super::redacted(call);
        aurelius_runtime::redact::install_set(owner, Vec::new());
        assert!(outcome.is_err(), "passed through: {outcome:?}");
        assert!(!outcome.unwrap_err().contains(value));
    }
}
