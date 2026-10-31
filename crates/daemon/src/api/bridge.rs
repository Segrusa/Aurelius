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

async fn answer(shared: &Arc<Shared>, request: BridgeRequest) -> BridgeOutcome {
    if request.protocol != PROTOCOL_MAJOR {
        return BridgeOutcome::Error {
            message: format!("Update Aurelius: this bridge speaks protocol {}, the server protocol {PROTOCOL_MAJOR}.", request.protocol),
        };
    }
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
        EmployeeTaskOp::ReadGroupChat { last } => EmployeeAction::ReadGroupChat { last },
        EmployeeTaskOp::PostToGroupChat { message } => EmployeeAction::PostToGroupChat { message },
        EmployeeTaskOp::AssignTicket { employee, reason } => EmployeeAction::AssignTicket { employee, reason },
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
