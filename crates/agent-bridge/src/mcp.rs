//! `aurelius-bridge --data-dir <dir> --run <run_id> [--project]`: the MCP server
//! Claude Code starts for one run (the daemon passes it as the run's MCP config
//! and as `--permission-prompt-tool mcp__aurelius__approve`). A minimal MCP
//! server over stdio. Every question goes to the running Aurelius service
//! over the internal bridge channel (`aurelius_client::bridge`); the bridge
//! never starts a service itself, and anything unexpected is a deny.
//!
//! Tools: `approve` (the permission prompt: an approval in the user's inbox,
//! the answer is Claude Code's allow/deny); `remember` and `recall` (the
//! employee's own memory, always on); `list_tasks`, `read_task`,
//! `list_conversations`, `read_conversation`; `set_mode` (only for management in a task whose
//! mode is theirs to choose, `--set-mode`); `delegate_task`, `message_task`,
//! `hand_over_task`, `add_idea`, `propose_colleague` and `propose_instructions`. `--project` (the
//! assistant of a project conversation) leaves out `hand_over_task`.
//!
//! It also gives the agent eyes: `screenshot` and `list_windows`. Claude Code
//! never asks for these; the bridge asks the service itself, which allows them
//! at once when the employee's Screen is on and otherwise puts the question in
//! the inbox. Once allowed, the rest of this run (one answer) may look.

use aurelius_client::ClientError;
use aurelius_client::bridge::{BridgeClient, IdeaKind, EmployeeMemoryOp, EmployeeTaskOp, PermissionAnswer, RunId};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

pub async fn serve(args: &[String]) -> anyhow::Result<()> {
    let arg = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let root = PathBuf::from(arg("--data-dir").ok_or_else(|| anyhow::anyhow!("--data-dir is required"))?);
    let run_id = RunId::parse(&arg("--run").ok_or_else(|| anyhow::anyhow!("--run is required"))?)?;
    // The assistant of a project conversation gives work to employees but cannot hand a task over.
    let project = args.iter().any(|a| a == "--project");
    // Management that chooses the mode of its task offers set_mode (the daemon decides per run).
    let set_mode = args.iter().any(|a| a == "--set-mode");
    let mut client: Option<BridgeClient> = None;
    let mut screen_granted = false;
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut out = tokio::io::stdout();
    while let Some(line) = lines.next_line().await? {
        let Ok(msg) = serde_json::from_str::<Value>(&line) else { continue };
        let Some(id) = msg.get("id").cloned() else { continue }; // notifications need no answer
        let result = match msg["method"].as_str().unwrap_or("") {
            "initialize" => Ok(json!({
                "protocolVersion": msg["params"]["protocolVersion"].as_str().unwrap_or("2025-06-18"),
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "aurelius", "version": env!("CARGO_PKG_VERSION") },
            })),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": tool_definitions(project, set_mode) })),
            "tools/call" => {
                let name = msg["params"]["name"].as_str().unwrap_or("").to_owned();
                let args = msg["params"]["arguments"].clone();
                match call_tool(&mut client, &root, run_id, project, set_mode, &mut screen_granted, &name, args).await {
                    Some(result) => Ok(result),
                    None => Err(json!({ "code": -32601, "message": "Method not found" })),
                }
            }
            _ => Err(json!({ "code": -32601, "message": "Method not found" })),
        };
        let reply = match result {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err(error) => json!({ "jsonrpc": "2.0", "id": id, "error": error }),
        };
        out.write_all(format!("{reply}\n").as_bytes()).await?;
        out.flush().await?;
    }
    Ok(())
}

/// Every tool this run offers.
fn tool_definitions(project: bool, set_mode: bool) -> Vec<Value> {
    let mut tools = vec![json!({
        "name": "approve",
        "description": "Asks the Aurelius user to allow or deny one action. Only Claude Code calls this.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "tool_name": { "type": "string" },
                "input": { "type": "object" },
                "tool_use_id": { "type": "string" }
            },
            "required": ["tool_name", "input"]
        }
    })];
    tools.extend(crate::screen::tool_definitions());
    tools.extend(memory_tool_definitions());
    tools.push(suggest_replies_definition());
    tools.extend(project_tool_definitions());
    tools.extend(task_tool_definitions().into_iter().filter(|t| !project || t["name"] != "hand_over_task"));
    tools.push(json!({
        "name": "add_idea",
        "description": "Note an idea for a project that is worth doing later but is not part of the current work (an improvement, a follow-up, a risk to look at). The user sees it in the project's ideas and decides if and when it becomes a task.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "title": { "type": "string" },
                "note": { "type": "string", "description": "Why, and what it would take." },
                "kind": { "type": "string", "enum": ["feature", "bug"], "description": "bug for something broken or behaving wrongly; feature for new or improved behaviour (default feature)." },
                "project": { "type": "string", "description": "The project (its name) the idea is for: needed in your chat with the user (outside projects); in a task or project conversation it is that project." }
            },
            "required": ["title"]
        }
    }));
    if set_mode {
        tools.push(json!({
            "name": "set_mode",
            "description": "Choose the mode of this task: auto (work without asking, for clear, well-bounded work that can be undone) or plan (read and plan only, then present the plan with ExitPlanMode). Always give a reason; the user sees it in the thread. Refused while a plan waits for the user's approval.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "mode": { "type": "string", "enum": ["auto", "plan"] },
                    "reason": { "type": "string", "description": "Why this mode, in one short sentence." }
                },
                "required": ["mode", "reason"]
            }
        }));
    }
    tools.push(json!({
        "name": "propose_colleague",
        "description": "Propose a new colleague (employee) for the company. The user always decides, also in Auto mode, so this only adds a proposal; you get no answer now. At most two of your proposals are open at a time, and names are unique in the company. Use it when the work clearly needs a role nobody in the company has.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": "The new colleague's name." },
                "role": { "type": "string", "description": "Their role, e.g. Designer." },
                "title": { "type": "string", "description": "Optional job title." },
                "instructions": { "type": "string", "description": "What they do and how they work." },
                "reports_to": { "type": "string", "description": "A colleague's name; default yourself." },
                "department": { "type": "string", "description": "The department they join (its name); default your own." },
                "reason": { "type": "string", "description": "Why the team needs them." }
            },
            "required": ["name", "role", "instructions", "reason"]
        }
    }));
    tools.push(json!({
        "name": "propose_instructions",
        "description": "Propose better instructions for yourself or for a colleague who reports to you directly, based on what tasks taught you. The user always decides (and may edit them), also in Auto mode, so this only adds a proposal; you get no answer now. Give the whole new text, why, and as evidence at least one task (its id or title) or memory (a lesson) it rests on. One open proposal per employee's instructions.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "employee": { "type": "string", "description": "A colleague who reports to you directly; default yourself." },
                "instructions": { "type": "string", "description": "The complete new instructions (they replace the current ones)." },
                "reason": { "type": "string", "description": "What goes wrong now and how this fixes it." },
                "evidence": { "type": "array", "items": { "type": "string" }, "description": "Tasks (id or title) and memories (a lesson: words of it) this rests on; at least one." }
            },
            "required": ["instructions", "reason", "evidence"]
        }
    }));
    tools
}

/// Run one tool; `None` when the tool is unknown.
async fn call_tool(client: &mut Option<BridgeClient>, root: &Path, run_id: RunId, project: bool, set_mode: bool, screen_granted: &mut bool, name: &str, args: Value) -> Option<Value> {
    let text = |k: &str| args[k].as_str().unwrap_or("").to_owned();
    let optional = |k: &str| args[k].as_str().map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned);
    let op = match name {
        "approve" => {
            let input = args["input"].clone();
            let call_id = args["tool_use_id"].as_str().map(str::to_owned);
            let answer = ask(client, root, run_id, call_id, text("tool_name"), input.clone()).await;
            let decision = if answer.allow {
                // A question comes back with the user's answers in its input;
                // an approved plan may switch the session's mode.
                let mut decision = json!({ "behavior": "allow", "updatedInput": answer.updated_input.unwrap_or(input) });
                if let Some(permissions) = answer.updated_permissions {
                    decision["updatedPermissions"] = permissions;
                }
                decision
            } else {
                json!({ "behavior": "deny", "message": answer.message.unwrap_or_else(|| "Not allowed in Aurelius.".into()) })
            };
            return Some(json!({ "content": [{ "type": "text", "text": decision.to_string() }] }));
        }
        "suggest_replies" => return Some(reply(true, "Shown.".into())),
        "screenshot" | "list_windows" => return Some(screen_call(client, root, run_id, screen_granted, name, args).await),
        "remember" => return Some(memory_call(client, root, run_id, EmployeeMemoryOp::Remember { kind: text("kind"), scope: text("scope"), content: text("content") }).await),
        "recall" => return Some(memory_call(client, root, run_id, EmployeeMemoryOp::Recall { query: text("query") }).await),
        "list_tasks" => EmployeeTaskOp::ListTasks { status: text("status"), employee: text("employee"), query: text("query"), project: optional("project") },
        "read_task" => EmployeeTaskOp::ReadTask { task: text("task") },
        "list_conversations" => EmployeeTaskOp::ListConversations { query: text("query") },
        "read_conversation" => EmployeeTaskOp::ReadConversation { conversation: text("conversation") },
        "add_idea" => EmployeeTaskOp::AddIdea { title: text("title"), note: text("note"), kind: args["kind"].as_str().and_then(IdeaKind::parse).unwrap_or_default(), project: optional("project") },
        "message_task" => EmployeeTaskOp::MessageTask { task: text("task"), message: text("message"), interrupt: args["interrupt"].as_bool().unwrap_or(true) },
        "delegate_task" => {
            let criteria = args["criteria"].as_array().map(|a| a.iter().filter_map(|c| c.as_str().map(str::to_owned)).collect()).unwrap_or_default();
            EmployeeTaskOp::Delegate { employee: text("employee"), title: text("title"), goal: text("goal"), criteria, project: optional("project") }
        }
        "set_mode" if set_mode => EmployeeTaskOp::SetMode { mode: text("mode"), reason: text("reason") },
        "hand_over_task" if !project => EmployeeTaskOp::HandOver { employee: text("employee"), reason: text("reason") },
        "propose_colleague" => EmployeeTaskOp::ProposeColleague {
            name: text("name"),
            role: text("role"),
            title: optional("title"),
            instructions: text("instructions"),
            reports_to: optional("reports_to"),
            reason: text("reason"),
            department: optional("department"),
        },
        "propose_instructions" => EmployeeTaskOp::ProposeInstructions {
            employee: optional("employee"),
            instructions: text("instructions"),
            reason: text("reason"),
            evidence: args["evidence"].as_array().map(|a| a.iter().filter_map(|e| e.as_str().map(str::to_owned)).collect()).unwrap_or_default(),
        },
        _ => return None,
    };
    Some(act_call(client, root, run_id, op).await)
}

/// The connection to the service, made on first use and again after it dropped.
async fn connected<'a>(client: &'a mut Option<BridgeClient>, root: &Path) -> Result<&'a BridgeClient, ClientError> {
    if client.is_none() {
        *client = Some(BridgeClient::connect(root).await?);
    }
    Ok(client.as_ref().expect("connected"))
}

fn reply(ok: bool, text: String) -> Value {
    json!({ "content": [{ "type": "text", "text": text }], "isError": !ok })
}

/// A screen tool: ask first unless already allowed in this run, then look.
async fn screen_call(client: &mut Option<BridgeClient>, root: &Path, run_id: RunId, granted: &mut bool, name: &str, args: Value) -> Value {
    let failed = |text: String| json!({ "content": [{ "type": "text", "text": text }], "isError": true });
    if !*granted {
        let answer = ask(client, root, run_id, None, format!("mcp__aurelius__{name}"), args.clone()).await;
        if !answer.allow {
            return failed(answer.message.unwrap_or_else(|| "The user did not allow looking at the screen.".into()));
        }
        *granted = true;
    }
    let window = args["window"].as_str().map(str::to_owned);
    let monitor = args["monitor"].as_u64().map(|n| n as usize);
    let list = name == "list_windows";
    let root = root.to_path_buf();
    let result = tokio::task::spawn_blocking(move || {
        if list {
            crate::screen::list_windows().map(|text| json!({ "content": [{ "type": "text", "text": text }] }))
        } else {
            crate::screen::screenshot(window.as_deref(), monitor).map(|shot| {
                use base64::Engine;
                // Kept so the user can look at it too (the id is in the text the UI sees).
                let caption = match crate::screen::keep(&root, &shot.jpeg) {
                    Some(id) => format!("{} Saved as screenshot {id}.", shot.caption),
                    None => shot.caption,
                };
                json!({ "content": [
                    { "type": "text", "text": caption },
                    { "type": "image", "data": base64::engine::general_purpose::STANDARD.encode(&shot.jpeg), "mimeType": "image/jpeg" }
                ] })
            })
        }
    })
    .await;
    match result {
        Ok(Ok(value)) => value,
        Ok(Err(e)) => failed(format!("Could not look at the screen: {e}")),
        Err(e) => failed(format!("Could not look at the screen: {e}")),
    }
}

/// `suggest_replies`: the clients read the call from the answer's tool uses and
/// show the options as buttons (feature `quick_replies`); nothing is stored and
/// the run is not blocked, the call only answers "Shown.".
fn suggest_replies_definition() -> Value {
    json!({
        "name": "suggest_replies",
        "description": "At the very end of your answer, offer the user 2 or 3 short ready-made replies to a question you just asked, shown as buttons. Only suggestions: the user may type anything else. Call it once, last, after your answer text; then stop. Put the preferred reply first.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "question": { "type": "string", "description": "The short question, as the user sees it above the buttons." },
                "options": { "type": "array", "minItems": 2, "maxItems": 3, "items": { "type": "string", "description": "A reply of at most about 6 words, in the user's language; it is sent as the user's message." } }
            },
            "required": ["options"]
        }
    })
}

/// The memory tools (see the module docs).
fn memory_tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "remember",
            "description": "Store something you want to know later, active at once: a decision you took (and why), project knowledge you learned, or an episode worth remembering. Keep it to one short, self-contained statement. Never store secrets.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "content": { "type": "string", "description": "One self-contained statement (at most 2000 characters)." },
                    "kind": { "type": "string", "enum": ["decision", "project_knowledge", "episodic"] },
                    "scope": { "type": "string", "enum": ["employee", "department", "project"], "description": "employee: only you; department: everyone in your department; project: everyone working in this project (only in a task or project conversation). Default: decisions and knowledge to the project (else your department), episodes to you." }
                },
                "required": ["content", "kind"]
            }
        }),
        json!({
            "name": "recall",
            "description": "Search the memories you may read (yours, your department's, the project's when you work in one, and global ones) for earlier decisions and knowledge before you decide something.",
            "inputSchema": {
                "type": "object",
                "properties": { "query": { "type": "string", "description": "What you are looking for, in a few words." } },
                "required": ["query"]
            }
        }),
    ]
}

/// Reading tasks and conversations (nothing changes).
fn project_tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "list_tasks",
            "description": "List tasks, most recently active first: title, status, employee, id. In a task or project conversation those of its project; in your chat with the user those of every project (or of the one you name). Use it to find work done or going on in other tasks and conversations.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "status": { "type": "string", "description": "open, ended, or one of draft, queued, running, waiting, review, succeeded, failed, cancelled. Empty: all." },
                    "employee": { "type": "string", "description": "Only tasks of this employee (a name)." },
                    "query": { "type": "string", "description": "Words that must be in the title." },
                    "project": { "type": "string", "description": "Only this project's tasks (its name); used in your chat with the user." }
                }
            }
        }),
        json!({
            "name": "read_task",
            "description": "Read one task: goal, criteria, workers, delegated parts, its result (the report) and the latest messages of its conversation.",
            "inputSchema": {
                "type": "object",
                "properties": { "task": { "type": "string", "description": "The task's id (or the short id list_tasks shows) or its title." } },
                "required": ["task"]
            }
        }),
        json!({
            "name": "list_conversations",
            "description": "List conversations with the user, most recently active first: title, messages, id. In a project: its conversations with the project assistant; for an employee also its own chats with the user (not task conversations).",
            "inputSchema": {
                "type": "object",
                "properties": { "query": { "type": "string", "description": "Words that must be in the title." } }
            }
        }),
        json!({
            "name": "read_conversation",
            "description": "Read one of those conversations: its summary and its latest messages.",
            "inputSchema": {
                "type": "object",
                "properties": { "conversation": { "type": "string", "description": "The conversation's id (or the short id list_conversations shows) or its title." } },
                "required": ["conversation"]
            }
        }),
    ]
}

/// An employee's tools to let colleagues work (delegate_task also outside a task).
fn task_tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "delegate_task",
            "description": "Give work to an employee of the company: a part of your task (it stays in your task's project), or, outside a task, something from this conversation. They work on it as a task in a conversation of their own and their result comes back here. Use it for work that clearly suits them better; do the rest yourself.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "employee": { "type": "string", "description": "The colleague's name." },
                    "title": { "type": "string" },
                    "goal": { "type": "string", "description": "What they must deliver." },
                    "criteria": { "type": "array", "items": { "type": "string" } },
                    "project": { "type": "string", "description": "The project (its name) the task is for: required in your chat with the user (outside projects); ignored in a task, which keeps its project." }
                },
                "required": ["employee", "title", "goal"]
            }
        }),
        json!({
            "name": "message_task",
            "description": "Send a message to a task you delegated to a colleague (still open). It appears in their task conversation under your name. By default it interrupts what they are doing so they handle it at once; set interrupt to false to let them finish first.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "task": { "type": "string", "description": "The task's id (or the short id list_tasks shows) or its title." },
                    "message": { "type": "string" },
                    "interrupt": { "type": "boolean", "description": "Stop their current work and answer now (default true)." }
                },
                "required": ["task", "message"]
            }
        }),
        json!({
            "name": "hand_over_task",
            "description": "Hand the whole task over to a colleague who suits it clearly better. Stop working on it afterwards.",
            "inputSchema": {
                "type": "object",
                "properties": { "employee": { "type": "string" }, "reason": { "type": "string" } },
                "required": ["employee", "reason"]
            }
        }),
    ]
}

/// One employee action through the service; failures are reported to the model.
async fn act_call(client: &mut Option<BridgeClient>, root: &Path, run_id: RunId, op: EmployeeTaskOp) -> Value {
    let result = match connected(client, root).await {
        Ok(bridge) => bridge.employee_act(run_id, op).await,
        Err(e) => return reply(false, format!("Aurelius is not reachable: {e}")),
    };
    match result {
        Ok((ok, text)) => reply(ok, text),
        Err(e) => {
            forget_if_gone(client, &e);
            reply(false, format!("Aurelius could not do this: {e}"))
        }
    }
}

/// One memory call through the service; failures are reported to the model.
async fn memory_call(client: &mut Option<BridgeClient>, root: &Path, run_id: RunId, op: EmployeeMemoryOp) -> Value {
    let result = match connected(client, root).await {
        Ok(bridge) => bridge.memory_run(run_id, op).await,
        Err(e) => return reply(false, format!("Aurelius is not reachable: {e}")),
    };
    match result {
        Ok((ok, text)) => reply(ok, text),
        Err(e) => {
            forget_if_gone(client, &e);
            reply(false, format!("Memory is not available: {e}"))
        }
    }
}

/// Ask the service; any failure is a deny (never an implicit allow).
async fn ask(client: &mut Option<BridgeClient>, root: &Path, run_id: RunId, call_id: Option<String>, tool: String, input: Value) -> PermissionAnswer {
    let deny = |message: String| PermissionAnswer { allow: false, message: Some(message), updated_input: None, updated_permissions: None };
    let result = match connected(client, root).await {
        Ok(bridge) => bridge.permission_ask(run_id, call_id, tool, input).await,
        Err(e) => return deny(format!("Aurelius is not reachable: {e}")),
    };
    match result {
        Ok(answer) => answer,
        Err(e) => {
            forget_if_gone(client, &e);
            deny(format!("Aurelius could not ask: {e}"))
        }
    }
}

/// A dropped connection is made again by the next call.
fn forget_if_gone(client: &mut Option<BridgeClient>, err: &ClientError) {
    if err.is_disconnect() {
        *client = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_project_assistant_cannot_hand_a_task_over() {
        let names = |project| tool_definitions(project, false).iter().map(|t| t["name"].as_str().unwrap().to_owned()).collect::<Vec<_>>();
        assert!(names(false).contains(&"hand_over_task".to_owned()));
        assert!(!names(true).contains(&"hand_over_task".to_owned()));
        for tool in ["approve", "remember", "recall", "propose_colleague", "propose_instructions", "add_idea", "suggest_replies", "screenshot", "list_windows"] {
            assert!(names(true).contains(&tool.to_owned()), "{tool}");
        }
    }

    #[test]
    fn set_mode_is_offered_only_when_the_daemon_asks_for_it() {
        let names = |set_mode| tool_definitions(false, set_mode).iter().map(|t| t["name"].as_str().unwrap().to_owned()).collect::<Vec<_>>();
        assert!(!names(false).contains(&"set_mode".to_owned()));
        assert!(names(true).contains(&"set_mode".to_owned()));
    }
}
