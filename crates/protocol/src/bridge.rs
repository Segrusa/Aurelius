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
    /// A part for a colleague: a sub-task whose result comes back. Work always
    /// belongs to a ticket: in a task or a ticket's discussion it is that ticket,
    /// elsewhere `ticket` (`#12` or an id) names it and the task gets its project.
    /// `project` is what older callers still send; it decides nothing any more.
    Delegate {
        employee: String,
        title: String,
        #[serde(default)]
        goal: String,
        #[serde(default)]
        criteria: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ticket: Option<String>,
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
    /// The latest `last` messages of a channel of the Team chat (default and most 50).
    /// `channel`: `General`, a project's name or a channel id; none: the channel that
    /// woke the run, else its project's, else the general one.
    ReadGroupChat {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        last: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        channel: Option<String>,
    },
    /// Post in a channel under the employee's name; `@Name` wakes that colleague
    /// (never the poster, at most `group_chat::MAX_HOPS` hops after a board member's message).
    PostToGroupChat {
        message: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        channel: Option<String>,
    },
    /// The latest `last` messages of the discussion of the run's ticket (`--discussion` only).
    ReadDiscussion { #[serde(default, skip_serializing_if = "Option::is_none")] last: Option<u32> },
    /// Post in the discussion of the run's ticket; `@Name` wakes as in a channel (`--discussion` only).
    PostToDiscussion { message: String },
    /// The vault items a run may use (the bridge's `vault_list`, feature `vault`):
    /// name, kind, scope, description and env name, never a value. `project` (a
    /// name or id): another project's; none: the company's plus the run's project.
    VaultList { #[serde(default, skip_serializing_if = "Option::is_none")] project: Option<String> },
    /// One item (the bridge's `vault_read`; `item`: its name or env name). A text
    /// or url answers its value (audit `tool_read`); a secret only "available as
    /// $ENV_NAME", never its value (B3: secrets reach runs as environment only).
    VaultRead {
        item: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project: Option<String>,
    },
    /// The dispatcher of a ticket chooses its owner (the bridge's `assign_ticket`, `--triage` only).
    AssignTicket { employee: String, #[serde(default)] reason: String },
    /// The tester's verdict on a ticket in Testing (the bridge's `report_test_result`,
    /// `--testing` only, ticket #5): `passed` moves the card to Review, else back to
    /// Doing with `summary` for the owner. The daemon refuses it for any run other
    /// than the open test task of a ticket in Testing.
    ReportTestResult { passed: bool, #[serde(default)] summary: String },
    /// Propose a guideline (the bridge's `propose_guideline`, feature `guidelines`, B5):
    /// a change of one in the run's own block (`guideline`: its `g:` id, id or
    /// title), or none for a new one. `text` is the whole new markdown. Always
    /// waits for a board member, also in Auto; `evidence` names at least one task
    /// or memory; one open proposal per guideline, two new ones per employee.
    ProposeGuideline {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        guideline: Option<String>,
        title: String,
        text: String,
        reason: String,
        #[serde(default)]
        evidence: Vec<String>,
    },
    /// The guidelines in use (the bridge's `list_guidelines`, GL-15): id, title,
    /// current version and links by name, never a text.
    ListGuidelines {},
    /// One guideline's current text and version (the bridge's `read_guideline`,
    /// GL-15; `guideline`: its `g:` id, id or title). Information only: it never
    /// changes the run's block, and is no rule for the run.
    ReadGuideline { guideline: String },
    /// The board's tickets (the bridge's `list_tickets`): `project` (a name or id), else the run's
    /// project, else every project; `column` a column's name; `query` words, or `#12`; `limit` at most 50.
    ListTickets {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        column: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        query: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        limit: Option<u32>,
    },
    /// One ticket in full (the bridge's `read_ticket`; `ticket`: `#12`, a number or an id) with the
    /// latest `last` messages of its discussion.
    ReadTicket { ticket: String, #[serde(default, skip_serializing_if = "Option::is_none")] last: Option<u32> },
    /// A new ticket in Backlog (the bridge's `create_ticket`). Management only; it never starts a run.
    CreateTicket {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project: Option<String>,
        title: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        priority: Option<String>,
        #[serde(default)]
        labels: Vec<String>,
    },
    /// Change a ticket's title, description, priority or labels (the bridge's `update_ticket`):
    /// management, or the employee who works on the ticket.
    UpdateTicket {
        ticket: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        priority: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        labels: Option<Vec<String>>,
    },
    /// Start a ticket (the bridge's `move_ticket`): management moves Backlog → Todo; every other
    /// move is refused (a board member's decision, or the work's own).
    MoveTicket { ticket: String, column: String },
    /// A message in a ticket's discussion (the bridge's `post_to_ticket`), as `post_to_group_chat`
    /// does in a channel.
    PostToTicket { ticket: String, message: String },
    /// Hand the work in the run's thread to a colleague (the bridge's `hand_off`): `to`
    /// a name, `message` self-contained. Waits while the sender has unread items (R5).
    HandOff { to: String, message: String },
    /// Send the responsible board member of the run's ticket a message in their chat with
    /// this employee (the bridge's `tell_responsible`), with a link to the ticket. Marked
    /// unread there; it starts no run and completes nothing.
    TellResponsible { message: String },
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
        assert_eq!((json["method"].as_str(), json["protocol"].as_u64()), (Some("daemon.shutdown"), Some(5)));
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
        let op: EmployeeTaskOp = serde_json::from_value(serde_json::json!({ "op": "propose_guideline", "title": "Rust style", "text": "Run clippy.", "reason": "Twice a warning", "evidence": ["AUR-1"] })).unwrap();
        assert!(matches!(&op, EmployeeTaskOp::ProposeGuideline { guideline: None, evidence, .. } if evidence.len() == 1));
        assert_eq!(serde_json::from_value::<EmployeeTaskOp>(serde_json::to_value(&op).unwrap()).unwrap(), op);
        let op: EmployeeTaskOp = serde_json::from_value(serde_json::json!({ "op": "list_guidelines" })).unwrap();
        assert_eq!(op, EmployeeTaskOp::ListGuidelines {});
        let op: EmployeeTaskOp = serde_json::from_value(serde_json::json!({ "op": "read_guideline", "guideline": "Rust style" })).unwrap();
        assert!(matches!(&op, EmployeeTaskOp::ReadGuideline { guideline } if guideline == "Rust style"));
        assert_eq!(serde_json::from_value::<EmployeeTaskOp>(serde_json::to_value(&op).unwrap()).unwrap(), op);
    }

    /// The wire name of an op, by a match that lists every variant: a new variant (say a
    /// decision of a board member) does not compile until it is named here, where it is seen.
    fn op_name(op: &EmployeeTaskOp) -> &'static str {
        use EmployeeTaskOp::*;
        match op {
            Delegate { .. } => "delegate",
            HandOver { .. } => "hand_over",
            MessageTask { .. } => "message_task",
            AddIdea { .. } => "add_idea",
            ListTasks { .. } => "list_tasks",
            ReadTask { .. } => "read_task",
            ListConversations { .. } => "list_conversations",
            ReadConversation { .. } => "read_conversation",
            ProposeColleague { .. } => "propose_colleague",
            ProposeInstructions { .. } => "propose_instructions",
            SetMode { .. } => "set_mode",
            ReadGroupChat { .. } => "read_group_chat",
            PostToGroupChat { .. } => "post_to_group_chat",
            ReadDiscussion { .. } => "read_discussion",
            PostToDiscussion { .. } => "post_to_discussion",
            VaultList { .. } => "vault_list",
            VaultRead { .. } => "vault_read",
            AssignTicket { .. } => "assign_ticket",
            ReportTestResult { .. } => "report_test_result",
            ProposeGuideline { .. } => "propose_guideline",
            ListGuidelines { .. } => "list_guidelines",
            ReadGuideline { .. } => "read_guideline",
            ListTickets { .. } => "list_tickets",
            ReadTicket { .. } => "read_ticket",
            CreateTicket { .. } => "create_ticket",
            UpdateTicket { .. } => "update_ticket",
            MoveTicket { .. } => "move_ticket",
            PostToTicket { .. } => "post_to_ticket",
            HandOff { .. } => "hand_off",
            TellResponsible { .. } => "tell_responsible",
        }
    }

    #[test]
    fn the_board_ops_parse_with_their_defaults_and_round_trip() {
        let ops = [
            serde_json::json!({ "op": "list_tickets" }),
            serde_json::json!({ "op": "read_ticket", "ticket": "#12" }),
            serde_json::json!({ "op": "create_ticket", "title": "Export" }),
            serde_json::json!({ "op": "update_ticket", "ticket": "#12", "priority": "high" }),
            serde_json::json!({ "op": "move_ticket", "ticket": "#12", "column": "todo" }),
            serde_json::json!({ "op": "post_to_ticket", "ticket": "#12", "message": "hi" }),
        ];
        for json in ops {
            let op: EmployeeTaskOp = serde_json::from_value(json.clone()).unwrap();
            assert_eq!(op_name(&op), json["op"].as_str().unwrap());
            assert_eq!(serde_json::from_value::<EmployeeTaskOp>(serde_json::to_value(&op).unwrap()).unwrap(), op);
        }
        let op: EmployeeTaskOp = serde_json::from_value(serde_json::json!({ "op": "create_ticket", "title": "Export" })).unwrap();
        assert!(matches!(&op, EmployeeTaskOp::CreateTicket { project: None, description: None, priority: None, labels, .. } if labels.is_empty()));
    }

    #[test]
    fn a_board_members_decisions_are_no_employee_op() {
        // Nothing for accepting, plan decisions, sending back, cancelling, archiving, deleting or the Responsible.
        for op in ["accept_ticket", "accept", "approve_plan", "revise_plan", "send_back", "send_back_ticket", "cancel_ticket", "cancel", "archive_ticket", "delete_ticket", "set_responsible", "assign", "start_ticket"] {
            let parsed = serde_json::from_value::<EmployeeTaskOp>(serde_json::json!({ "op": op, "ticket": "#1", "feedback": "x", "employee": "Ann" }));
            assert!(parsed.is_err(), "{op} is an employee op");
        }
    }
}
