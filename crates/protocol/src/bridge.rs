//! The internal bridge channel. The agent bridge (`aurelius-bridge`, the MCP
//! server Claude Code starts for a run) and `aurelius-ctl shutdown` talk to
//! the daemon over their own local endpoint with these messages. They are not
//! part of the client protocol and never travel over the network.

use aurelius_domain::{RunId, idea::IdeaKind, memory::EmployeeMemoryOp};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Bridge → daemon.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BridgeRequest {
    pub protocol: u16,
    pub request_id: u64,
    #[serde(flatten)]
    pub call: BridgeCall,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "method", content = "params")]
pub enum BridgeCall {
    /// Claude Code asks permission for one of its own tools during a run. Answers
    /// when the user decides; no answer in time is a deny, never an allow.
    #[serde(rename = "permission.ask")]
    PermissionAsk { run_id: RunId, call_id: Option<String>, tool: String, input: serde_json::Value },
    /// An employee's own memory during a run (the bridge's `remember` and `recall` tools).
    #[serde(rename = "memory.run")]
    MemoryRun { run_id: RunId, op: EmployeeMemoryOp },
    /// An employee's action during a run: delegate, hand over, message a task, note an idea,
    /// read tasks and conversations, propose a colleague, read and post in the group chat.
    #[serde(rename = "employee.act")]
    EmployeeAct { run_id: RunId, op: EmployeeTaskOp },
    /// Stop the daemon (`aurelius-ctl shutdown`).
    #[serde(rename = "daemon.shutdown")]
    DaemonShutdown,
}

/// An employee's action on its task.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum EmployeeTaskOp {
    /// A part for a colleague: a sub-task whose result comes back. `project` (a
    /// name or id) is required from an employee's chat (no project there); in a
    /// task the part stays in the task's project.
    Delegate {
        employee: String,
        title: String,
        #[serde(default)]
        goal: String,
        #[serde(default)]
        criteria: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project: Option<String>,
    },
    /// The whole task to a colleague who suits it better.
    HandOver { employee: String, #[serde(default)] reason: String },
    /// A message to a task this employee delegated; `interrupt` (default) stops the
    /// other employee's unfinished runs first ("Send now"), else it queues behind them.
    MessageTask { task: String, message: String, #[serde(default = "yes")] interrupt: bool },
    /// Note an idea for a project (for later; not to start now). `project` (a
    /// name or id) is required from an employee's chat; in a task it is the task's.
    AddIdea {
        title: String,
        #[serde(default)]
        note: String,
        #[serde(default)]
        kind: IdeaKind,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project: Option<String>,
    },
    /// Tasks, filtered (status: a task status, "open" or "ended"): in a task those
    /// of its project, in a chat those of every project unless `project` names one.
    ListTasks {
        #[serde(default)]
        status: String,
        #[serde(default)]
        employee: String,
        #[serde(default)]
        query: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project: Option<String>,
    },
    /// One task in full (an id, an id's start, or its title).
    ReadTask { task: String },
    /// The employee's own chats (not task conversations), filtered by words.
    ListConversations { #[serde(default)] query: String },
    /// One of them: its summary and latest messages (an id, an id's start, or its title).
    ReadConversation { conversation: String },
    /// Propose a new colleague. Always waits for the user, also in Auto; at most two
    /// proposals of one employee are open at a time. `reports_to` is an employee's name
    /// (default: the proposer).
    ProposeColleague {
        name: String,
        role: String,
        #[serde(default)]
        title: Option<String>,
        instructions: String,
        #[serde(default)]
        reports_to: Option<String>,
        reason: String,
    },
    /// Propose new instructions for oneself or a direct report (`employee`: a name;
    /// default the proposer). Always waits for the user, also in Auto; `evidence`
    /// names at least one task or memory; one open proposal per employee's instructions.
    ProposeInstructions { #[serde(default)] employee: Option<String>, instructions: String, reason: String, #[serde(default)] evidence: Vec<String> },
    /// Management chooses the mode of its own task conversation (`auto` or `plan`), with a reason
    /// that is shown in the thread. Only management, only without an open plan approval.
    SetMode { mode: String, #[serde(default)] reason: String },
    /// The latest `last` messages of the company's group chat (default and most 50).
    ReadGroupChat { #[serde(default, skip_serializing_if = "Option::is_none")] last: Option<u32> },
    /// Post in the group chat under the employee's name; `@Name` wakes that colleague
    /// (never the poster, at most `group_chat::MAX_HOPS` hops after the user's message).
    PostToGroupChat { message: String },
    /// The dispatcher of a ticket chooses its owner (the bridge's `assign_ticket`, `--triage` only).
    AssignTicket { employee: String, #[serde(default)] reason: String },
}

fn yes() -> bool {
    true
}

/// Daemon → bridge.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BridgeResponse {
    pub request_id: u64,
    #[serde(flatten)]
    pub outcome: BridgeOutcome,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BridgeOutcome {
    /// `updated_input`: the tool input to use instead (e.g. a question with its answers).
    /// `updated_permissions`: permission changes to apply with an allow (e.g. a plan switching to auto mode).
    Permission { allow: bool, message: Option<String>, updated_input: Option<serde_json::Value>, updated_permissions: Option<serde_json::Value> },
    /// What an employee's tool tells the model.
    Text { ok: bool, text: String },
    Done,
    Error { message: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_messages_use_their_own_method_names() {
        let request = BridgeRequest { protocol: crate::PROTOCOL_MAJOR, request_id: 1, call: BridgeCall::DaemonShutdown };
        let json = serde_json::to_value(&request).unwrap();
        assert_eq!((json["method"].as_str(), json["protocol"].as_u64()), (Some("daemon.shutdown"), Some(4)));
        assert_eq!(serde_json::from_value::<BridgeRequest>(json).unwrap(), request);
        // A client method is not a bridge method, and the other way round.
        let client = serde_json::json!({ "protocol": 3, "request_id": 1, "method": "project.list" });
        assert!(serde_json::from_value::<BridgeRequest>(client).is_err());
        let bridge = serde_json::json!({ "protocol": 3, "request_id": 1, "method": "daemon.shutdown" });
        assert!(serde_json::from_value::<crate::Request>(bridge).is_err());
    }

    #[test]
    fn a_proposal_op_parses_with_its_defaults() {
        let op: EmployeeTaskOp = serde_json::from_value(serde_json::json!({ "op": "propose_colleague", "name": "Eva", "role": "Designer", "instructions": "Design.", "reason": "UI work" })).unwrap();
        assert!(matches!(op, EmployeeTaskOp::ProposeColleague { title: None, reports_to: None, .. }));
        let op: EmployeeTaskOp = serde_json::from_value(serde_json::json!({ "op": "delegate", "employee": "Dean", "title": "Port", "project": "Atlas" })).unwrap();
        assert!(matches!(&op, EmployeeTaskOp::Delegate { project: Some(p), .. } if p == "Atlas"));
        let op: EmployeeTaskOp = serde_json::from_value(serde_json::json!({ "op": "propose_instructions", "instructions": "Work test-first.", "reason": "Two failed tasks", "evidence": ["Migration"] })).unwrap();
        assert!(matches!(&op, EmployeeTaskOp::ProposeInstructions { employee: None, evidence, .. } if evidence.len() == 1));
        assert_eq!(serde_json::from_value::<EmployeeTaskOp>(serde_json::to_value(&op).unwrap()).unwrap(), op);
    }
}
