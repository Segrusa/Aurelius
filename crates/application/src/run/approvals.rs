//! Approvals: Claude Code asks the user before it acts (its mode requires asking),
//! asks a question, or hands in a plan. The user decides in the inbox or in the
//! conversation; Aurelius only records the decision, Claude Code carries out the
//! action itself. A decision is bound to the exact action shown (its digest),
//! expiry is never permission, and one approval is used once.

use crate::run::events::{Ev, emit};
use crate::{CommandContext, CommandService, Committed, domain_error, storage_error};
use aurelius_domain::{
    ActorKind, AppError, ApprovalId, CommandEnvelope, CommandResult, DomainError, EventNotice, RunId,
    approval::{ApprovalStatus, DecideApproval, Effect, always_allow_eligible},
    autonomy::AgentMode,
    views::{QuestionAnswer, RunDecision, run_decision},
};
use aurelius_storage::{StorageError, Tx, approvals, blobs, conversations as conv, now_ms, settings};
use rusqlite::OptionalExtension as _;
use sha2::{Digest, Sha256};

pub const APPROVAL_TTL_MS: i64 = 30 * 60 * 1000;

/// Tools that the runtime (Claude Code) runs itself; Aurelius only decides.
pub const RUNTIME_TOOL_PREFIX: &str = "claude.";

/// Claude Code's tool for asking the user questions (answered in the Inbox).
pub const QUESTION_TOOL: &str = "AskUserQuestion";

/// Claude Code's tool for handing in a plan in plan mode (decided in the Inbox).
pub const PLAN_TOOL: &str = "ExitPlanMode";

/// The browser's tools (Claude in Chrome or Playwright); they ask first, except in Auto (the user turned the browser on).
pub use aurelius_runtime::claude_code::is_browser_tool;
/// Aurelius' own screen tools (screenshots, window list), served by the approval bridge.
pub const SCREEN_TOOLS: &[&str] = &["mcp__aurelius__screenshot", "mcp__aurelius__list_windows"];

const ACCOUNT: &str = "local";

/// Outcome of a runtime's permission question.
pub enum RuntimePermission {
    Pending { approval_id: ApprovalId, notices: Vec<EventNotice> },
    Denied(String),
}

/// What a Claude Code tool call can do, for the approval and its audit trail.
pub fn runtime_tool_effect(tool: &str) -> Effect {
    match tool {
        // A question changes nothing; the user's answer is the decision.
        // A plan changes nothing either; the user's decision is what counts.
        "Read" | "Glob" | "Grep" | "WebSearch" | "WebFetch" | QUESTION_TOOL | PLAN_TOOL => Effect::Read,
        "Edit" | "Write" | "MultiEdit" | "NotebookEdit" => Effect::LocalWrite,
        // Looking at the screen changes nothing.
        t if SCREEN_TOOLS.contains(&t) => Effect::Read,
        _ => Effect::Execute,
    }
}

/// What the user approves: the tool and its exact arguments (object keys are sorted, so
/// the same action always has the same digest).
fn action_digest(tool: &str, arguments: &serde_json::Value) -> String {
    hex::encode(Sha256::digest(format!("{tool}\n{arguments}").as_bytes()))
}

fn ev<'a>(aggregate_type: &'a str, id: String, event_type: &'a str, actor: ActorKind, correlation: String, payload: Option<String>, metadata: serde_json::Value, dedupe: String) -> Ev<'a> {
    Ev { aggregate_type, aggregate_id: id, event_type, actor, actor_id: None, correlation, payload, metadata, dedupe }
}

impl CommandService {
    /// User decision on one approval. Re-validates status, expiry and digest;
    /// consumes on approval, so a second response finds it used.
    pub(crate) fn decide_approval(&self, ctx: &CommandContext, envelope: &CommandEnvelope, hash: String, cmd: &DecideApproval) -> Result<Committed, AppError> {
        let blobs_store = self.store.clone();
        let cmd = cmd.clone();
        let command_id = envelope.command_id;
        let meta = ctx.metadata();

        self.commit(command_id, hash, move |tx: &Tx<'_>| {
            let Some(row) = approvals::load_approval(tx, cmd.approval_id)? else {
                return Ok(Err(domain_error(DomainError::ApprovalNotFound(cmd.approval_id))));
            };
            if row.status != ApprovalStatus::Pending {
                return Ok(Err(domain_error(DomainError::ApprovalNotPending)));
            }
            // A plan is approved with `decide_plan` (it carries the decision the bridge reads);
            // refusing here keeps a client from approving it blind. Rejecting is fine.
            if cmd.approve && row.tool == format!("{RUNTIME_TOOL_PREFIX}{PLAN_TOOL}") {
                return Ok(Err(domain_error(DomainError::OutOfRange { field: "approval".into(), detail: "a plan is approved by deciding on the plan".into() })));
            }
            if cmd.always_allow && (!cmd.approve || !always_allow_eligible(&row.tool)) {
                return Ok(Err(domain_error(DomainError::OutOfRange { field: "always_allow".into(), detail: "only an approved MCP tool of your own servers or the browser can be always allowed".into() })));
            }
            let correlation = row.run_id.to_string();
            let aid = cmd.approval_id.to_string();
            let mut notices = Vec::new();
            let finish = |tx: &Tx<'_>, status: ApprovalStatus, event_type: &'static str, notices: &mut Vec<EventNotice>| -> Result<(), StorageError> {
                let (event_id, n) = emit(tx, ev("approval", aid.clone(), event_type, ActorKind::User, correlation.clone(), None, meta.clone(), format!("approval:{aid}:{}", status.as_str())))?;
                approvals::set_approval_status(tx, cmd.approval_id, status, Some(event_id))?;
                notices.push(n);
                Ok(())
            };
            if row.expires_at_ms <= now_ms() {
                // Committed as expired, but the caller gets a refusal: expiry is never permission.
                finish(tx, ApprovalStatus::Expired, "approval.expired", &mut notices)?;
                return Ok(Ok((CommandResult::ApprovalDecided { approval_id: cmd.approval_id, status: ApprovalStatus::Expired, ok: false, output: DomainError::ApprovalExpired.to_string() }, notices)));
            }
            // The shown digest must match what was stored, and the stored action must still hash to it.
            let action: serde_json::Value = blobs_store
                .blobs()
                .read(&row.action_blob_key)
                .ok()
                .and_then(|b| serde_json::from_slice(&b).ok())
                .ok_or_else(|| StorageError::Rejected("action payload missing".into()))?;
            let current = action_digest(action["tool"].as_str().unwrap_or(""), &action["arguments"]);
            if cmd.action_digest != row.digest || current != row.digest {
                finish(tx, ApprovalStatus::Invalidated, "approval.invalidated", &mut notices)?;
                return Ok(Ok((CommandResult::ApprovalDecided { approval_id: cmd.approval_id, status: ApprovalStatus::Invalidated, ok: false, output: DomainError::ApprovalInvalidated.to_string() }, notices)));
            }
            conv::resume_run(tx, row.run_id)?;
            if !cmd.approve {
                finish(tx, ApprovalStatus::Rejected, "approval.rejected", &mut notices)?;
                return Ok(Ok((CommandResult::ApprovalDecided { approval_id: cmd.approval_id, status: ApprovalStatus::Rejected, ok: false, output: "Rejected by the user.".into() }, notices)));
            }
            // Approve and consume in one transaction: a second response finds it consumed.
            finish(tx, ApprovalStatus::Approved, "approval.approved", &mut notices)?;
            finish(tx, ApprovalStatus::Consumed, "approval.consumed", &mut notices)?;
            if !cmd.always_allow {
                return Ok(Ok((CommandResult::ApprovalDecided { approval_id: cmd.approval_id, status: ApprovalStatus::Consumed, ok: true, output: "Approved; ran once.".into() }, notices)));
            }
            // Always allow: on the server-wide list, and its other asks waiting now go ahead too.
            let tool = row.tool.strip_prefix(RUNTIME_TOOL_PREFIX).unwrap_or(&row.tool).to_owned();
            let mut tools = settings::mcp_always_allow(tx)?;
            if !tools.contains(&tool) {
                tools.push(tool.clone());
                settings::set_mcp_always_allow(tx, &tools, now_ms())?;
            }
            for (other, run_id) in approvals::pending_for_tool(tx, &row.tool, now_ms())? {
                // Plan mode keeps asking, as it does for the list itself.
                if other == cmd.approval_id || conv::mode_for_run(tx, run_id)? == AgentMode::Plan {
                    continue;
                }
                let oid = other.to_string();
                let metadata = serde_json::json!({ "tool": tool, "by": aid, "ctx": meta.clone() });
                let (event_id, n) = emit(tx, ev("approval", oid.clone(), "approval.allowed_always", ActorKind::User, run_id.to_string(), None, metadata, format!("approval:{oid}:allowed_always")))?;
                approvals::set_approval_status(tx, other, ApprovalStatus::Consumed, Some(event_id))?;
                conv::resume_run(tx, run_id)?;
                notices.push(n);
            }
            Ok(Ok((CommandResult::ApprovalDecided { approval_id: cmd.approval_id, status: ApprovalStatus::Consumed, ok: true, output: format!("Approved; {tool} is always allowed from now on.") }, notices)))
        })
    }

    /// Claude Code asks to use one of its own tools during a run (its mode
    /// requires asking). The user decides in the inbox; Aurelius only records
    /// the decision, Claude Code carries out the action itself.
    pub fn request_runtime_permission(&self, run_id: RunId, call_id: Option<String>, tool: &str, input: &serde_json::Value) -> Result<RuntimePermission, AppError> {
        let tool_name = format!("{RUNTIME_TOOL_PREFIX}{tool}");
        let effect = runtime_tool_effect(tool);
        let digest = action_digest(&tool_name, input);
        let reasons = if tool == QUESTION_TOOL {
            serde_json::json!(["Claude asks you; it waits for your answer."])
        } else if tool == PLAN_TOOL {
            serde_json::json!(["Claude made a plan; it waits for your decision."])
        } else if is_browser_tool(tool) {
            let browser = if tool.starts_with(aurelius_runtime::claude_code::PLAYWRIGHT_TOOL_PREFIX) { "Uses Aurelius' browser window, with the logins made in it." } else { "Uses your own browser, with your logins." };
            serde_json::json!([format!("{browser} Aurelius asks in Ask and Accept edits mode; Auto lets it through.")])
        } else if SCREEN_TOOLS.contains(&tool) {
            serde_json::json!(["Looks at your screen; it can show anything that is open. Allowing it counts for the rest of this answer. Turn Screen on for this employee to stop asking."])
        } else if self.run_tool_policy(run_id)?.needs_approval(tool) {
            serde_json::json!(["This employee's tool settings ask first for this, in every mode."])
        } else {
            serde_json::json!(["The employee's mode asks before this action."])
        };
        let stored = serde_json::json!({ "tool": tool_name, "arguments": input, "effect": effect.as_str(), "resources": [], "account": ACCOUNT, "reasons": reasons });
        let action_blob = self.store.blobs().put_json(&stored).map_err(storage_error)?;
        let call = call_id.unwrap_or_else(|| format!("ask-{}", ApprovalId::new()));
        let st = self.store.clone();
        // A plan is shown in full in the conversation, not only in the inbox.
        let plan = (tool == PLAN_TOOL).then(|| input["plan"].as_str().unwrap_or("").trim().to_owned()).filter(|p| !p.is_empty());
        let questions = (tool == QUESTION_TOOL).then(|| crate::task::task_threads::describe_questions(input));
        self.store
            .write(move |tx| {
                let status: Option<String> = tx.query_row("SELECT status FROM runs WHERE id = ?1", [run_id.to_string()], |r| r.get(0)).optional()?;
                if !matches!(status.as_deref(), Some("starting" | "running" | "waiting")) {
                    return Ok(RuntimePermission::Denied("This answer is no longer running.".into()));
                }
                // Asked twice for the same call: wait for the same approval.
                let known: Option<String> = tx
                    .query_row("SELECT id FROM approvals WHERE run_id = ?1 AND call_id = ?2", [run_id.to_string(), call.clone()], |r| r.get(0))
                    .optional()?;
                if let Some(approval_id) = known.and_then(|a| ApprovalId::parse(&a).ok()) {
                    return Ok(RuntimePermission::Pending { approval_id, notices: vec![] });
                }
                blobs::register(tx, &action_blob)?;
                let approval_id = ApprovalId::new();
                let (event_id, n) = emit(tx, ev("approval", approval_id.to_string(), "approval.requested", ActorKind::System, run_id.to_string(), None, serde_json::json!({ "tool": tool_name }), format!("approval:{approval_id}:requested")))?;
                let mut notices = vec![n];
                approvals::insert_approval(
                    tx,
                    approvals::NewApproval {
                        id: approval_id,
                        run_id,
                        call_id: &call,
                        tool: &tool_name,
                        effect: effect.as_str(),
                        action_blob_id: &action_blob.id.to_string(),
                        digest: &digest,
                        request_event: event_id,
                        expires_at_ms: now_ms() + APPROVAL_TTL_MS,
                    },
                )?;
                conv::set_run_status(tx, run_id, "waiting", None)?;
                // A plan or a question: in full in the conversation the run answers in, chat or task.
                match (&plan, &questions) {
                    (Some(plan), _) => notices.extend(crate::task::task_threads::post_plan(tx, &st, run_id, approval_id, plan)?),
                    (None, Some(questions)) => notices.extend(crate::task::task_threads::post_question(tx, &st, run_id, approval_id, questions)?),
                    (None, None) => {}
                }
                Ok(RuntimePermission::Pending { approval_id, notices })
            })
            .map_err(storage_error)
    }

    /// The user's answers to a runtime's question: stored with the approval,
    /// which is then consumed (the waiting bridge hands them to Claude Code).
    pub(crate) fn answer_question(&self, ctx: &CommandContext, envelope: &CommandEnvelope, hash: String, cmd: &aurelius_domain::approval::AnswerQuestion) -> Result<Committed, AppError> {
        let answers: std::collections::BTreeMap<String, String> =
            cmd.answers.iter().map(|(q, a)| (q.trim().to_owned(), a.trim().to_owned())).filter(|(q, a)| !q.is_empty() && !a.is_empty()).collect();
        if answers.is_empty() {
            return Err(domain_error(DomainError::Required { field: "answer".into() }));
        }
        if answers.values().any(|a| a.chars().count() > 4_000) {
            return Err(domain_error(DomainError::TooLong { field: "answer".into(), max: 4_000 }));
        }
        let approval_id = cmd.approval_id;
        let meta = ctx.metadata();
        let st = self.store.clone();
        self.commit(envelope.command_id, hash, move |tx: &Tx<'_>| {
            let Some(row) = approvals::load_approval(tx, approval_id)? else {
                return Ok(Err(domain_error(DomainError::ApprovalNotFound(approval_id))));
            };
            if row.status != ApprovalStatus::Pending {
                return Ok(Err(domain_error(DomainError::ApprovalNotPending)));
            }
            if row.tool != format!("{RUNTIME_TOOL_PREFIX}{QUESTION_TOOL}") {
                return Ok(Err(domain_error(DomainError::OutOfRange { field: "approval".into(), detail: "this is not a question".into() })));
            }
            let aid = approval_id.to_string();
            let correlation = row.run_id.to_string();
            let mut notices = Vec::new();
            if row.expires_at_ms <= now_ms() {
                let (event_id, n) = emit(tx, ev("approval", aid.clone(), "approval.expired", ActorKind::User, correlation, None, meta, format!("approval:{aid}:expired")))?;
                approvals::set_approval_status(tx, approval_id, ApprovalStatus::Expired, Some(event_id))?;
                notices.push(n);
                return Ok(Ok((CommandResult::ApprovalDecided { approval_id, status: ApprovalStatus::Expired, ok: false, output: "This question has expired.".into() }, notices)));
            }
            approvals::set_approval_answer(tx, approval_id, &serde_json::json!(answers))?;
            let answered: Vec<String> = answers.iter().map(|(q, a)| format!("- {q}\n  **{a}**")).collect();
            let decision = RunDecision {
                kind: run_decision::ANSWERED.into(),
                answers: answers.iter().map(|(q, a)| QuestionAnswer { question: q.clone(), answer: a.clone() }).collect(),
                feedback: None,
            };
            notices.extend(crate::task::task_threads::post_run_decision(tx, &st, row.run_id, &format!("Answered:\n\n{}", answered.join("\n")), &decision)?);
            // Claude's answer goes on under the question and the answer, not above them.
            conv::move_run_message_to_end(tx, row.run_id)?;
            let (event_id, n) = emit(tx, ev("approval", aid.clone(), "approval.answered", ActorKind::User, correlation, None, meta, format!("approval:{aid}:answered")))?;
            approvals::set_approval_status(tx, approval_id, ApprovalStatus::Consumed, Some(event_id))?;
            conv::resume_run(tx, row.run_id)?;
            notices.push(n);
            Ok(Ok((CommandResult::ApprovalDecided { approval_id, status: ApprovalStatus::Consumed, ok: true, output: "Answer sent.".into() }, notices)))
        })
    }

    /// The user's decision on a plan Claude Code proposed. Approve (auto or
    /// accept-edits) lets the plan go ahead; revise (feedback required) and reject deny it with that feedback.
    /// The decision is stored with the approval for the waiting bridge to hand on.
    pub(crate) fn decide_plan(&self, ctx: &CommandContext, envelope: &CommandEnvelope, hash: String, cmd: &aurelius_domain::approval::DecidePlan) -> Result<Committed, AppError> {
        use aurelius_domain::approval::PlanDecision;
        let feedback = cmd.feedback.as_deref().map(str::trim).filter(|f| !f.is_empty()).map(str::to_owned);
        if cmd.decision == PlanDecision::Revise && feedback.is_none() {
            return Err(domain_error(DomainError::Required { field: "feedback".into() }));
        }
        if feedback.as_ref().is_some_and(|f| f.chars().count() > 4_000) {
            return Err(domain_error(DomainError::TooLong { field: "feedback".into(), max: 4_000 }));
        }
        let (approval_id, decision) = (cmd.approval_id, cmd.decision);
        let meta = ctx.metadata();
        let st = self.store.clone();
        self.commit(envelope.command_id, hash, move |tx: &Tx<'_>| {
            let Some(row) = approvals::load_approval(tx, approval_id)? else {
                return Ok(Err(domain_error(DomainError::ApprovalNotFound(approval_id))));
            };
            // Already decided, here or on another device.
            if row.status != ApprovalStatus::Pending {
                return Ok(Err(domain_error(DomainError::ApprovalNotPending)));
            }
            if row.tool != format!("{RUNTIME_TOOL_PREFIX}{PLAN_TOOL}") {
                return Ok(Err(domain_error(DomainError::OutOfRange { field: "approval".into(), detail: "this is not a plan".into() })));
            }
            let aid = approval_id.to_string();
            let correlation = row.run_id.to_string();
            if row.expires_at_ms <= now_ms() {
                let (event_id, n) = emit(tx, ev("approval", aid.clone(), "approval.expired", ActorKind::User, correlation, None, meta, format!("approval:{aid}:expired")))?;
                approvals::set_approval_status(tx, approval_id, ApprovalStatus::Expired, Some(event_id))?;
                return Ok(Ok((CommandResult::ApprovalDecided { approval_id, status: ApprovalStatus::Expired, ok: false, output: "This plan has expired.".into() }, vec![n])));
            }
            approvals::set_approval_answer(tx, approval_id, &serde_json::json!({ "decision": decision, "feedback": feedback }))?;
            let (status, event_type, output) = match decision {
                PlanDecision::Approve | PlanDecision::ApproveEdits => (ApprovalStatus::Consumed, "approval.plan_approved", "Plan approved; the work goes ahead."),
                PlanDecision::Revise => (ApprovalStatus::Rejected, "approval.plan_revision_requested", "Changes requested; the plan is revised."),
                PlanDecision::Reject => (ApprovalStatus::Rejected, "approval.plan_rejected", "Plan rejected; the work stops."),
            };
            let said = match (decision, &feedback) {
                (PlanDecision::Approve, _) => "Approved the plan.".to_string(),
                (PlanDecision::ApproveEdits, _) => "Approved the plan; edits go through, everything else is asked.".to_string(),
                (PlanDecision::Revise, Some(f)) => format!("Asked for changes to the plan: {f}"),
                (PlanDecision::Reject, Some(f)) => format!("Rejected the plan: {f}"),
                (_, None) => "Rejected the plan.".to_string(),
            };
            let kind = match decision {
                PlanDecision::Approve => run_decision::PLAN_APPROVED,
                PlanDecision::ApproveEdits => run_decision::PLAN_APPROVED_EDITS,
                PlanDecision::Revise => run_decision::PLAN_CHANGES,
                PlanDecision::Reject => run_decision::PLAN_REJECTED,
            };
            // Feedback goes with a revision or rejection only, as in `said`.
            let noted = match decision {
                PlanDecision::Revise | PlanDecision::Reject => feedback.clone(),
                PlanDecision::Approve | PlanDecision::ApproveEdits => None,
            };
            let card = RunDecision { kind: kind.into(), answers: Vec::new(), feedback: noted };
            let mut notices: Vec<EventNotice> = crate::task::task_threads::post_run_decision(tx, &st, row.run_id, &said, &card)?.into_iter().collect();
            // The answer goes on under the plan and the decision, not above them.
            conv::move_run_message_to_end(tx, row.run_id)?;
            // The conversation leaves plan mode too, so its mode shows it and the next answer runs in it.
            let mut mode = match decision {
                PlanDecision::Approve => Some(aurelius_domain::autonomy::AgentMode::Auto),
                PlanDecision::ApproveEdits => Some(aurelius_domain::autonomy::AgentMode::AcceptEdits),
                PlanDecision::Revise | PlanDecision::Reject => None,
            };
            // A design session stays in Auto (see `DESIGN_STAYS_AUTO`).
            if mode.is_some() && aurelius_storage::work_sessions::is_design_run(tx, row.run_id)? {
                mode = Some(aurelius_domain::autonomy::AgentMode::Auto);
            }
            if let Some(mode) = mode {
                if let Some(conversation) = conv::set_mode_for_run(tx, row.run_id, mode)? {
                    let metadata = serde_json::json!({ "agent_mode": mode.as_str(), "ctx": meta.clone() });
                    let (_, n) = emit(tx, ev("conversation", conversation.to_string(), "conversation.settings_changed", ActorKind::User, correlation.clone(), None, metadata, format!("approval:{aid}:mode")))?;
                    notices.push(n);
                }
            }
            let (event_id, n) = emit(tx, ev("approval", aid.clone(), event_type, ActorKind::User, correlation, None, meta, format!("approval:{aid}:{}", status.as_str())))?;
            approvals::set_approval_status(tx, approval_id, status, Some(event_id))?;
            conv::resume_run(tx, row.run_id)?;
            notices.push(n);
            Ok(Ok((CommandResult::ApprovalDecided { approval_id, status, ok: status == ApprovalStatus::Consumed, output: output.into() }, notices)))
        })
    }

    /// The working mode that governs this run now (the user may switch it mid-answer).
    pub fn run_mode(&self, run_id: RunId) -> Result<aurelius_domain::autonomy::AgentMode, AppError> {
        self.store.read(|c| conv::mode_for_run(c, run_id)).map_err(storage_error)
    }

    /// A pending runtime approval the user's new mode now covers (e.g. an edit
    /// after switching to Auto): it leaves the inbox as allowed by that mode.
    /// Returns nothing when it was decided meanwhile.
    pub fn allow_by_mode(&self, approval_id: ApprovalId, mode: aurelius_domain::autonomy::AgentMode) -> Result<Option<Vec<EventNotice>>, AppError> {
        self.store
            .write(move |tx| {
                let Some(row) = approvals::load_approval(tx, approval_id)? else { return Ok(None) };
                if row.status != ApprovalStatus::Pending {
                    return Ok(None);
                }
                let aid = approval_id.to_string();
                let (event_id, n) = emit(tx, ev("approval", aid.clone(), "approval.allowed_by_mode", ActorKind::System, row.run_id.to_string(), None, serde_json::json!({ "mode": mode.as_str() }), format!("approval:{aid}:allowed_by_mode")))?;
                approvals::set_approval_status(tx, approval_id, ApprovalStatus::Consumed, Some(event_id))?;
                conv::resume_run(tx, row.run_id)?;
                Ok(Some(vec![n]))
            })
            .map_err(storage_error)
    }

    /// Whether the user chose 'Always allow' for this MCP tool (never for Aurelius' own tools).
    pub fn always_allowed(&self, tool: &str) -> Result<bool, AppError> {
        if !always_allow_eligible(tool) {
            return Ok(false);
        }
        self.store.read(|c| Ok(settings::mcp_always_allow(c)?.iter().any(|t| t == tool))).map_err(storage_error)
    }

    /// A pending ask for a tool that became always allowed after it was asked
    /// (the race with `decide_approval`'s sweep): it leaves the inbox as allowed.
    /// Returns nothing when it was decided meanwhile.
    pub fn allow_always(&self, approval_id: ApprovalId) -> Result<Option<Vec<EventNotice>>, AppError> {
        self.store
            .write(move |tx| {
                let Some(row) = approvals::load_approval(tx, approval_id)? else { return Ok(None) };
                if row.status != ApprovalStatus::Pending {
                    return Ok(None);
                }
                let aid = approval_id.to_string();
                let metadata = serde_json::json!({ "tool": row.tool.strip_prefix(RUNTIME_TOOL_PREFIX).unwrap_or(&row.tool) });
                let (event_id, n) = emit(tx, ev("approval", aid.clone(), "approval.allowed_always", ActorKind::System, row.run_id.to_string(), None, metadata, format!("approval:{aid}:allowed_always")))?;
                approvals::set_approval_status(tx, approval_id, ApprovalStatus::Consumed, Some(event_id))?;
                conv::resume_run(tx, row.run_id)?;
                Ok(Some(vec![n]))
            })
            .map_err(storage_error)
    }

    /// Whether the user lets the employee behind this run take screenshots without asking.
    /// Which tools the employee of this run has and which ask first (C2), as it is now.
    pub fn run_tool_policy(&self, run_id: RunId) -> Result<aurelius_domain::employee_tools::ToolPolicy, AppError> {
        self.store.read(|c| aurelius_storage::employees::tool_policy_of_run(c, run_id)).map_err(storage_error)
    }

    pub fn run_screen_allowed(&self, run_id: RunId) -> Result<bool, AppError> {
        self.store.read(|c| conv::screen_for_run(c, run_id)).map_err(storage_error)
    }

    /// The answers given to a question (see `answer_question`).
    pub fn approval_answer(&self, approval_id: ApprovalId) -> Result<Option<serde_json::Value>, AppError> {
        self.store.read(|c| approvals::approval_answer(c, approval_id)).map_err(storage_error)
    }

    /// Current state of an approval (for the bridge that waits on it).
    pub fn approval_status(&self, approval_id: ApprovalId) -> Result<Option<ApprovalStatus>, AppError> {
        self.store
            .read(|c| {
                let status: Option<String> = c.query_row("SELECT status FROM approvals WHERE id = ?1", [approval_id.to_string()], |r| r.get(0)).optional()?;
                Ok(status.and_then(|s| ApprovalStatus::parse(&s)))
            })
            .map_err(storage_error)
    }

    /// Expire overdue approvals. The waiting bridge sees the status change and
    /// refuses the tool, so only the notices come back (no follow-ups any more).
    pub fn expire_approvals(&self) -> Result<Vec<EventNotice>, AppError> {
        self.store
            .write(|tx| {
                let mut notices = Vec::new();
                for approval_id in approvals::expired_pending(tx, now_ms())? {
                    let Some(row) = approvals::load_approval(tx, approval_id)? else { continue };
                    let aid = approval_id.to_string();
                    let (event_id, n) = emit(tx, ev("approval", aid.clone(), "approval.expired", ActorKind::System, row.run_id.to_string(), None, serde_json::json!({}), format!("approval:{aid}:expired")))?;
                    approvals::set_approval_status(tx, approval_id, ApprovalStatus::Expired, Some(event_id))?;
                    conv::resume_run(tx, row.run_id)?;
                    notices.push(n);
                }
                Ok(notices)
            })
            .map_err(storage_error)
    }

    /// Invalidate pending approvals of a run that was stopped or ended (also after a restart):
    /// they leave the inbox, and a waiting bridge hands Claude Code a refusal.
    pub(crate) fn invalidate_run_approvals(tx: &Tx<'_>, run_id: RunId) -> Result<Vec<EventNotice>, StorageError> {
        let mut notices = Vec::new();
        for approval_id in approvals::pending_for_run(tx, run_id)? {
            let aid = approval_id.to_string();
            let (event_id, n) = emit(tx, ev("approval", aid.clone(), "approval.invalidated", ActorKind::System, run_id.to_string(), None, serde_json::json!({ "reason": "run_ended" }), format!("approval:{aid}:invalidated")))?;
            approvals::set_approval_status(tx, approval_id, ApprovalStatus::Invalidated, Some(event_id))?;
            notices.push(n);
        }
        Ok(notices)
    }
}
