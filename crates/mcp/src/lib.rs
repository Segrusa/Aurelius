//! `aurelius-mcp`: Aurelius as an MCP server for outside apps and agents
//! (Claude Code, the personal assistant, ...). It acts for the app of one app
//! key (feature `app_keys`) and has no API of its own: every tool calls
//! existing protocol methods through `aurelius_client::Client`, and the server
//! checks the key's scopes, limits and revocation on every call.
//!
//! * stdio (`main.rs`): one session, local IPC with the key in the hello, or
//!   the gateway's WebSocket with the key as bearer.
//! * Streamable HTTP (`http`): the remote gateway serves `/v1/mcp` with this
//!   same `Server`, on an in-process session of the app; `aurelius-mcp --http`
//!   serves `/mcp` itself (the Docker image), with a gateway WebSocket per key.
//!
//! Tools: `list_projects`, `list_employees`, `list_tasks`, `read_task`,
//! `create_task`, `message_task`, `stop_task`, `ask_project_assistant`,
//! `read_conversation`, `add_idea`, `recall`, `remember`, `read_inbox`; with
//! the approve scope also `set_task_mode` and `answer_approval`. A tool the
//! key's scopes do not cover is not listed and refused when called.
//!
//! Management channel (feature `management_channel`): `list_open_questions` for
//! every key; `ask_management` and `answer_question` for a key with the audience
//! `management`, which the server confines to the CEO and the CEO's direct reports
//! (the tools only hide what the server would refuse). `list_managers` is an old
//! name for `list_employees`.

use aurelius_client::{Client, ClientError, envelope};
use aurelius_domain::app_key::{self, AppAudience, AppScope};
use aurelius_domain::autonomy::AgentMode;
use aurelius_domain::conversation::{ChatStart, MessageRole, MessageStatus};
use aurelius_domain::views::{ApprovalView, EmployeeSummary, ProjectSummary, TaskView};
use aurelius_domain::{Command, CommandResult, ConversationId};
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::RwLock;

pub mod http;

/// MCP revisions this server speaks, newest first.
pub const PROTOCOL_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];
/// Longest a tool waits for the project assistant's answer.
const ASSISTANT_WAIT_MAX_SECS: u64 = 600;
/// Longest text of one message a tool returns.
const MESSAGE_MAX_CHARS: usize = 6_000;
const QUESTION_TOOL: &str = "AskUserQuestion";
const PLAN_TOOL: &str = "ExitPlanMode";

/// One app's MCP server over one Aurelius session.
pub struct Server {
    client: RwLock<Arc<Client>>,
    app: String,
    scopes: Vec<AppScope>,
    audience: AppAudience,
    /// The features of the server behind this session (the hello); the tools of a feature
    /// it lacks are not offered.
    features: Vec<String>,
    lost: AtomicBool,
}

/// What a tool says back: text, and whether it failed.
type ToolResult = Result<String, String>;

fn failure(err: ClientError) -> String {
    match err {
        ClientError::Protocol(e) => e.message,
        other => other.to_string(),
    }
}

fn text_arg(args: &Value, key: &str) -> String {
    args[key].as_str().map(str::trim).unwrap_or("").to_owned()
}

fn optional_arg(args: &Value, key: &str) -> Option<String> {
    args[key].as_str().map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned)
}

fn required_arg(args: &Value, key: &str) -> Result<String, String> {
    optional_arg(args, key).ok_or_else(|| format!("`{key}` is required."))
}

fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let cut: String = text.chars().take(max).collect();
    format!("{cut}… (cut)")
}

/// The modes a task can start in, as the server accepts them: Auto needs the approve scope, so a key without it is not offered it.
const TASK_MODES: [&str; 4] = ["ask", "accept_edits", "plan", "auto"];

fn task_modes(approve: bool) -> Vec<&'static str> {
    TASK_MODES.iter().copied().filter(|m| approve || *m != "auto").collect()
}

/// "ask", "accept_edits" (or "accept edits"), "plan", "auto".
fn parse_mode(text: &str) -> Result<AgentMode, String> {
    match text.trim().to_lowercase().replace([' ', '-'], "_").as_str() {
        "ask" => Ok(AgentMode::Ask),
        "accept_edits" => Ok(AgentMode::AcceptEdits),
        "plan" => Ok(AgentMode::Plan),
        "auto" => Ok(AgentMode::Auto),
        other => Err(format!("Unknown mode \"{other}\": use ask, accept_edits, plan or auto.")),
    }
}

/// The one item `arg` names: its id (or the first 8+ characters of it), its
/// exact name, or the only name that contains it.
fn pick<T>(items: Vec<T>, arg: &str, what: &str, id: impl Fn(&T) -> String, name: impl Fn(&T) -> String) -> Result<T, String> {
    let wanted = arg.trim().to_lowercase();
    if wanted.is_empty() {
        return Err(format!("Name the {what}."));
    }
    let by_id = |x: &T| {
        let id = id(x).to_lowercase();
        id == wanted || (wanted.len() >= 8 && id.starts_with(&wanted))
    };
    let mut items = items;
    if let Some(i) = items.iter().position(by_id) {
        return Ok(items.swap_remove(i));
    }
    let exact: Vec<usize> = items.iter().enumerate().filter(|(_, x)| name(x).to_lowercase() == wanted).map(|(i, _)| i).collect();
    if exact.len() == 1 {
        return Ok(items.swap_remove(exact[0]));
    }
    let partial: Vec<usize> = items.iter().enumerate().filter(|(_, x)| name(x).to_lowercase().contains(&wanted)).map(|(i, _)| i).collect();
    match partial.len() {
        1 => Ok(items.swap_remove(partial[0])),
        0 => Err(format!("No {what} matches \"{}\".", arg.trim())),
        _ => {
            let names: Vec<String> = partial.iter().take(6).map(|i| format!("{} ({})", name(&items[*i]), id(&items[*i]))).collect();
            Err(format!("Several {what}s match \"{}\": {}. Use the id.", arg.trim(), names.join(", ")))
        }
    }
}

fn task_line(t: &TaskView) -> String {
    let employee = t.employee_name.as_deref().map(|m| format!(" — {m}")).unwrap_or_else(|| " — no employee (draft)".into());
    let app = t.created_by_app.as_deref().map(|a| format!(" — created by app {a}")).unwrap_or_default();
    let waiting = t.waiting_reason.as_deref().map(|w| format!(", {w}")).unwrap_or_default();
    format!("- {} [{}{waiting}]{employee}{app} (id {})", t.title, t.status.as_str(), t.id)
}

fn employee_line(m: &EmployeeSummary, all: &[EmployeeSummary]) -> String {
    let title = m.title.as_deref().map(|t| format!(", {t}")).unwrap_or_default();
    let boss = m.reports_to.and_then(|b| all.iter().find(|x| x.id == b)).map(|b| format!(", reports to {}", b.name)).unwrap_or_default();
    let state = if m.working { "working" } else { "idle" };
    format!("- {} — {}{title} (id {}) — mode {}, {state}{boss}", m.name, m.role, m.id, m.agent_mode.as_str())
}

fn project_line(p: &ProjectSummary) -> String {
    let description = p.description.as_deref().map(|d| format!(": {}", clip(d, 200))).unwrap_or_default();
    format!("- {} (id {}) — {} employee(s) at work{description}", p.name, p.id, p.employees_active)
}

fn json_text(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

/// Whether `arg` names the item with id `full`: the whole id, or its first 8+ characters.
fn id_matches(full: &str, arg: &str) -> bool {
    let (full, arg) = (full.to_lowercase(), arg.trim().to_lowercase());
    full == arg || (arg.len() >= 8 && full.starts_with(&arg))
}

/// The questions Claude asked (`AskUserQuestion`), as their texts.
fn question_texts(approval: &ApprovalView) -> Vec<String> {
    approval.arguments["questions"].as_array().into_iter().flatten().filter_map(|q| q["question"].as_str()).map(str::to_owned).collect()
}

/// One open question of a manager, for `list_open_questions`.
fn question_json(approval: &ApprovalView) -> Value {
    json!({
        "id": approval.id,
        "type": "question",
        "from": approval.requester,
        "task_id": approval.task_id,
        "conversation_id": approval.conversation_id,
        "requested_at_ms": approval.requested_at_ms,
        "expires_at_ms": approval.expires_at_ms,
        "questions": approval.arguments["questions"],
    })
}

impl Server {
    pub fn new(client: Arc<Client>) -> Self {
        let (app, scopes, audience) = match &client.hello.app {
            Some(app) => (app.name.clone(), app.scopes.clone(), app.audience),
            None => (String::new(), Vec::new(), AppAudience::All),
        };
        let features = client.hello.features.clone();
        Self { client: RwLock::new(client), app, scopes, audience, features, lost: AtomicBool::new(false) }
    }

    /// The scopes of the app this server acts for.
    pub fn scopes(&self) -> &[AppScope] {
        &self.scopes
    }

    /// The connection to Aurelius was lost during a call; `reconnect` before the next.
    pub fn lost(&self) -> bool {
        self.lost.load(Ordering::SeqCst)
    }

    pub async fn reconnect(&self, client: Arc<Client>) {
        *self.client.write().await = client;
        self.lost.store(false, Ordering::SeqCst);
    }

    /// The session ended, for example because the key was revoked; open a new one.
    pub async fn closed(&self) -> bool {
        self.lost() || self.client().await.is_closed()
    }

    async fn client(&self) -> Arc<Client> {
        self.client.read().await.clone()
    }

    fn has(&self, scope: AppScope) -> bool {
        app_key::grants(&self.scopes, scope)
    }

    fn fail(&self, err: ClientError) -> String {
        if err.is_disconnect() {
            self.lost.store(true, Ordering::SeqCst);
            return "The connection to Aurelius was lost. Try again in a moment.".into();
        }
        failure(err)
    }

    /// One JSON-RPC message, or a batch (an array). `None` when nothing is
    /// answered (notifications and responses).
    pub async fn handle(&self, message: Value) -> Option<Value> {
        if let Value::Array(batch) = message {
            let mut out = Vec::new();
            for m in batch {
                if let Some(reply) = Box::pin(self.handle(m)).await {
                    out.push(reply);
                }
            }
            return (!out.is_empty()).then_some(Value::Array(out));
        }
        let id = message.get("id").cloned()?;
        let Some(method) = message.get("method").and_then(Value::as_str) else { return None };
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        let result = match method {
            "initialize" => Ok(self.initialize(&params)),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": self.tool_definitions() })),
            "tools/call" => {
                let name = params["name"].as_str().unwrap_or("").to_owned();
                let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
                match self.call_tool(&name, &args).await {
                    Some(Ok(text)) => Ok(json!({ "content": [{ "type": "text", "text": text }], "isError": false })),
                    Some(Err(text)) => Ok(json!({ "content": [{ "type": "text", "text": text }], "isError": true })),
                    None => Err(json!({ "code": -32602, "message": format!("Unknown tool: {name}") })),
                }
            }
            _ => Err(json!({ "code": -32601, "message": format!("Method not found: {method}") })),
        };
        Some(match result {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err(error) => json!({ "jsonrpc": "2.0", "id": id, "error": error }),
        })
    }

    fn initialize(&self, params: &Value) -> Value {
        let asked = params["protocolVersion"].as_str().unwrap_or("");
        let version = PROTOCOL_VERSIONS.iter().find(|v| **v == asked).copied().unwrap_or(PROTOCOL_VERSIONS[0]);
        let scopes: Vec<&str> = self.scopes.iter().map(|s| s.as_str()).collect();
        json!({
            "protocolVersion": version,
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "aurelius", "version": env!("CARGO_PKG_VERSION") },
            "instructions": format!(
                "Aurelius runs projects with AI employees that work on tasks. You act as the app \"{}\" with the scopes {}. \
                 Find projects and employees with list_projects and list_employees, follow work with list_tasks and read_task. \
                 Tasks you create show your app's name to the user.{}",
                self.app,
                scopes.join(", "),
                if self.audience == AppAudience::Management {
                    " This key reaches the management only (the CEO and the CEO's direct reports): talk to them with ask_management, and answer their questions with list_open_questions and answer_question."
                } else {
                    ""
                }
            ),
        })
    }

    /// The tools this app's scopes allow.
    pub fn tool_definitions(&self) -> Vec<Value> {
        tool_catalog().into_iter().filter(|(scope, tool)| self.has(*scope) && tool["name"].as_str().is_some_and(|n| self.available(n))).map(|(_, tool)| tool).collect()
    }

    /// Run one tool; `None` when there is no such tool.
    pub async fn call_tool(&self, name: &str, args: &Value) -> Option<ToolResult> {
        // `list_managers` is the old name of `list_employees`.
        let name = if name == "list_managers" { "list_employees" } else { name };
        let scope = tool_catalog().into_iter().find(|(_, t)| t["name"] == name).map(|(s, _)| s)?;
        if !self.has(scope) {
            return Some(Err(format!("This app key does not have the {} scope for {name}.", scope.as_str())));
        }
        if !self.available(name) {
            return Some(Err(format!("{name} is not available for this app key or this Aurelius server.")));
        }
        Some(match name {
            "list_projects" => self.list_projects().await,
            "list_employees" => self.list_employees(args).await,
            "list_tasks" => self.list_tasks(args).await,
            "read_task" => self.read_task(args).await,
            "create_task" => self.create_task(args).await,
            "message_task" => self.message_task(args).await,
            "stop_task" => self.stop_task(args).await,
            "ask_project_assistant" => self.ask_project_assistant(args).await,
            "read_conversation" => self.read_conversation(args).await,
            "add_idea" => self.add_idea(args).await,
            "recall" => self.recall(args).await,
            "remember" => self.remember(args).await,
            "read_inbox" => self.read_inbox().await,
            "set_task_mode" => self.set_task_mode(args).await,
            "answer_approval" => self.answer_approval(args).await,
            "ask_management" => self.ask_management(args).await,
            "list_open_questions" => self.list_open_questions(args).await,
            "answer_question" => self.answer_question(args).await,
            _ => return None,
        })
    }

    // ---- lookups ----

    async fn project(&self, arg: &str) -> Result<ProjectSummary, String> {
        let projects = self.client().await.projects().await.map_err(|e| self.fail(e))?.projects;
        pick(projects, arg, "project", |p| p.id.to_string(), |p| p.name.clone())
    }

    /// The company's employees: any of them may work in any project.
    async fn employees(&self) -> Result<Vec<EmployeeSummary>, String> {
        Ok(self.client().await.employees().await.map_err(|e| self.fail(e))?.employees)
    }

    async fn employee(&self, arg: &str) -> Result<EmployeeSummary, String> {
        pick(self.employees().await?, arg, "employee", |m| m.id.to_string(), |m| m.name.clone()).map_err(|e| {
            if self.audience == AppAudience::Management { format!("{e} {}", app_key::NOT_MANAGEMENT) } else { e }
        })
    }

    /// A task by id (or its first 8+ characters) or title, in one project or all.
    async fn task(&self, arg: &str, project: Option<&str>) -> Result<TaskView, String> {
        let project = match project {
            Some(p) => Some(self.project(p).await?.id),
            None => None,
        };
        let tasks = self.client().await.tasks(project).await.map_err(|e| self.fail(e))?.tasks;
        pick(tasks, arg, "task", |t| t.id.to_string(), |t| t.title.clone())
    }

    async fn execute(&self, command: Command) -> Result<CommandResult, String> {
        Ok(self.client().await.execute(envelope(command, None)).await.map_err(|e| self.fail(e))?.result)
    }

    // ---- read ----

    async fn list_projects(&self) -> ToolResult {
        let projects = self.client().await.projects().await.map_err(|e| self.fail(e))?.projects;
        if self.audience == AppAudience::Management {
            // JSON for a program (the personal assistant's picker): the active projects and the modes a task may start in.
            let list: Vec<Value> = projects
                .iter()
                .filter(|p| p.status.as_str() == "active")
                .map(|p| json!({ "id": p.id, "name": p.name, "description": p.description, "employees_active": p.employees_active }))
                .collect();
            return Ok(json_text(&json!({ "projects": list, "task_modes": task_modes(self.has(AppScope::Approve)) })));
        }
        let active: Vec<String> = projects.iter().filter(|p| p.status.as_str() == "active").map(project_line).collect();
        Ok(if active.is_empty() { "No active projects.".into() } else { active.join("\n") })
    }

    async fn list_employees(&self, _args: &Value) -> ToolResult {
        let employees = self.employees().await?;
        if employees.is_empty() {
            return Ok("The company has no employees yet.".to_string());
        }
        Ok(employees.iter().map(|m| employee_line(m, &employees)).collect::<Vec<_>>().join("\n"))
    }

    async fn list_tasks(&self, args: &Value) -> ToolResult {
        let project = match optional_arg(args, "project") {
            Some(p) => Some(self.project(&p).await?),
            None => None,
        };
        let tasks = self.client().await.tasks(project.as_ref().map(|p| p.id)).await.map_err(|e| self.fail(e))?.tasks;
        let status = optional_arg(args, "status").map(|s| s.to_lowercase());
        let employee = optional_arg(args, "employee").map(|m| m.to_lowercase());
        let query = optional_arg(args, "query").map(|q| q.to_lowercase());
        let mine = args["created_by_this_app"].as_bool().unwrap_or(false);
        let lines: Vec<String> = tasks
            .iter()
            .filter(|t| match status.as_deref() {
                None | Some("all") => true,
                Some("open") => !t.status.is_terminal(),
                Some("ended") => t.status.is_terminal(),
                Some(s) => t.status.as_str() == s,
            })
            .filter(|t| employee.as_deref().is_none_or(|m| t.employee_name.as_deref().is_some_and(|n| n.to_lowercase() == m)))
            .filter(|t| query.as_deref().is_none_or(|q| t.title.to_lowercase().contains(q) || t.goal.to_lowercase().contains(q)))
            .filter(|t| !mine || t.created_by_app.as_deref() == Some(self.app.as_str()))
            .take(100)
            .map(task_line)
            .collect();
        Ok(if lines.is_empty() { "No tasks match.".into() } else { lines.join("\n") })
    }

    async fn read_task(&self, args: &Value) -> ToolResult {
        let task = self.task(&text_arg(args, "task"), optional_arg(args, "project").as_deref()).await?;
        let mut out = format!("# {}\n\nStatus: {}", task.title, task.status.as_str());
        if let Some(w) = &task.waiting_reason {
            out.push_str(&format!(" ({w})"));
        }
        out.push_str(&format!("\nEmployee: {}", task.employee_name.as_deref().unwrap_or("none (draft)")));
        if let Some(app) = &task.created_by_app {
            out.push_str(&format!("\nCreated by app: {app}"));
        }
        if let Some(mode) = task.run.mode {
            out.push_str(&format!("\nMode: {}", mode.as_str()));
        }
        if task.pending_approvals > 0 {
            out.push_str(&format!("\nWaiting for the user: {} approval(s)", task.pending_approvals));
        }
        out.push_str(&format!("\nId: {}\n\n## Goal\n\n{}\n", task.id, task.goal));
        if !task.criteria.is_empty() {
            out.push_str("\n## Acceptance criteria\n\n");
            for c in &task.criteria {
                out.push_str(&format!("- {c}\n"));
            }
        }
        if let Some(report) = &task.result {
            out.push_str(&format!("\n## Report\n\n{}\n", clip(report, MESSAGE_MAX_CHARS)));
        }
        if let Some(conversation) = task.conversation_id {
            let latest = self.conversation_text(conversation, 4).await?;
            if !latest.is_empty() {
                out.push_str(&format!("\n## Latest in its conversation ({conversation})\n\n{latest}"));
            }
        }
        Ok(out)
    }

    /// The last `n` messages of a conversation, oldest first.
    async fn conversation_text(&self, conversation: ConversationId, n: usize) -> Result<String, String> {
        let snapshot = self.client().await.messages(conversation).await.map_err(|e| self.fail(e))?;
        let messages: Vec<_> = snapshot.messages.iter().filter(|m| !m.text.trim().is_empty()).collect();
        let skip = messages.len().saturating_sub(n);
        Ok(messages
            .iter()
            .skip(skip)
            .map(|m| {
                let who = match (m.role, &m.author, &m.update) {
                    (MessageRole::User, _, _) => "User".to_owned(),
                    (_, Some(author), _) => author.clone(),
                    (_, None, Some(update)) => format!("Update ({update})"),
                    _ => "Assistant".to_owned(),
                };
                let partial = if m.status == MessageStatus::Partial { " (still writing)" } else { "" };
                format!("**{who}**{partial}:\n{}\n", clip(&m.text, MESSAGE_MAX_CHARS))
            })
            .collect::<Vec<_>>()
            .join("\n"))
    }

    async fn read_conversation(&self, args: &Value) -> ToolResult {
        let id = ConversationId::parse(&text_arg(args, "conversation")).map_err(|_| "`conversation` must be a conversation id.".to_owned())?;
        let last = args["last"].as_u64().unwrap_or(10).clamp(1, 50) as usize;
        let text = self.conversation_text(id, last).await?;
        Ok(if text.is_empty() { "No messages yet.".into() } else { text })
    }

    async fn read_inbox(&self) -> ToolResult {
        let client = self.client().await;
        let notifications = client.notifications().await.map_err(|e| self.fail(e))?;
        let approvals = client.approvals().await.map_err(|e| self.fail(e))?.approvals;
        let mut out = String::new();
        let pending: Vec<_> = approvals.iter().filter(|a| a.status.as_str() == "pending").collect();
        if !pending.is_empty() {
            out.push_str("## Waiting for the user\n\n");
            for a in pending {
                let kind = match a.tool.as_str() {
                    PLAN_TOOL => "plan".to_owned(),
                    QUESTION_TOOL => "question".to_owned(),
                    tool => format!("{tool} ({})", a.effect),
                };
                let task = a.task_id.map(|t| format!(", task {t}")).unwrap_or_default();
                out.push_str(&format!("- {kind} from {}{task} (approval id {})\n", a.requester, a.id));
            }
            out.push('\n');
        }
        out.push_str(&format!("## Results ({} unread)\n\n", notifications.unread));
        if notifications.notifications.is_empty() {
            out.push_str("Nothing new.\n");
        }
        for n in notifications.notifications.iter().take(30) {
            let task = n.task_id.map(|t| format!(" (task {t})")).unwrap_or_default();
            let unread = if n.read { "" } else { "• " };
            out.push_str(&format!("- {unread}{}: {}{task}\n", n.title, clip(&n.subject, 300)));
        }
        Ok(out)
    }

    async fn recall(&self, args: &Value) -> ToolResult {
        let project = match optional_arg(args, "project") {
            Some(p) => Some(self.project(&p).await?.id),
            None => None,
        };
        let query = text_arg(args, "query").to_lowercase();
        let words: Vec<&str> = query.split_whitespace().filter(|w| w.len() > 2).collect();
        let items = self.client().await.memory(project).await.map_err(|e| self.fail(e))?.items;
        let mut scored: Vec<(usize, String)> = items
            .into_iter()
            .filter(|m| m.status.as_str() == "active")
            .filter_map(|m| {
                let content = m.content.to_lowercase();
                let score = words.iter().filter(|w| content.contains(**w)).count();
                (words.is_empty() || score > 0).then(|| (score, format!("- [{} · {}] {}", m.kind.as_str(), m.scope, clip(&m.content, 1_500))))
            })
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0));
        let lines: Vec<String> = scored.into_iter().take(15).map(|(_, l)| l).collect();
        Ok(if lines.is_empty() { "Nothing found.".into() } else { lines.join("\n") })
    }

    // ---- draft ----

    async fn create_task(&self, args: &Value) -> ToolResult {
        let project = self.project(&text_arg(args, "project")).await?;
        let title = required_arg(args, "title")?;
        let goal = optional_arg(args, "goal").unwrap_or_else(|| title.clone());
        let mode = optional_arg(args, "mode").map(|m| parse_mode(&m)).transpose()?;
        let management = self.audience == AppAudience::Management;
        if let Some(mode) = mode {
            if management && !task_modes(self.has(AppScope::Approve)).contains(&mode.as_str()) {
                return Err(format!("Mode \"{}\" is not allowed for this app key: use {}.", mode.as_str(), task_modes(self.has(AppScope::Approve)).join(", ")));
            }
        }
        let employee_arg = optional_arg(args, "employee");
        let start = args["start"].as_bool().unwrap_or((employee_arg.is_some() || management) && self.has(AppScope::Run));
        let employee = match (start, &employee_arg) {
            // A management key names a manager by role (CEO, CTO, CMO), name or id; no one named: the CEO.
            (true, arg) if management => Some(self.manager(arg.as_deref()).await?),
            (true, Some(m)) => Some(self.employee(m).await?),
            (true, None) => return Err("To start a task, name its employee (see list_employees); or create it as a draft with start: false.".into()),
            (false, _) => None,
        };
        let criteria: Vec<String> = args["criteria"].as_array().into_iter().flatten().filter_map(Value::as_str).map(str::to_owned).collect();
        let command = Command::CreateTask(aurelius_domain::task::CreateTask {
            project_id: project.id,
            employee_id: employee.as_ref().map(|m| m.id),
            title: title.clone(),
            goal,
            acceptance_criteria: criteria,
            depends_on: vec![],
            run: aurelius_domain::task::RunSettings { mode, ..Default::default() },
            files: vec![],
        });
        let CommandResult::TaskCreated { task_id } = self.execute(command).await? else { return Err("Unexpected answer from Aurelius.".into()) };
        if management {
            // JSON, like the other management tools: the id lets the app follow the task (task.finished, message_task).
            return Ok(json_text(&json!({
                "status": if employee.is_some() { "queued" } else { "draft" },
                "task_id": task_id,
                "title": title,
                "project": { "id": project.id, "name": project.name },
                "employee": employee.as_ref().map(|m| json!({ "id": m.id, "name": m.name, "role": m.role, "title": m.title })),
                "mode": mode.map(|m| m.as_str()),
            })));
        }
        Ok(match employee {
            Some(m) => format!("Created task \"{title}\" (id {task_id}) for {} in {}. It starts when {} is free; follow it with read_task.", m.name, project.name, m.name),
            None => {
                let note = if employee_arg.is_some() { " The employee was not set: a draft has none until the user gives it to one in Aurelius." } else { "" };
                format!("Saved task \"{title}\" (id {task_id}) as a draft in {}. The user starts it in Aurelius.{note}", project.name)
            }
        })
    }

    async fn add_idea(&self, args: &Value) -> ToolResult {
        let project = self.project(&text_arg(args, "project")).await?;
        let title = required_arg(args, "title")?;
        let kind = match optional_arg(args, "kind").as_deref() {
            Some("bug") => aurelius_domain::idea::IdeaKind::Bug,
            _ => aurelius_domain::idea::IdeaKind::Feature,
        };
        let command = Command::AddIdea(aurelius_domain::idea::AddIdea { project_id: project.id, title: title.clone(), note: optional_arg(args, "note"), kind, files: Vec::new() });
        self.execute(command).await?;
        Ok(format!("Added the idea \"{title}\" to {}.", project.name))
    }

    async fn remember(&self, args: &Value) -> ToolResult {
        use aurelius_domain::memory::{MemoryKind, MemoryScope, Remember};
        let content = required_arg(args, "content")?;
        let kind = match optional_arg(args, "kind").as_deref() {
            None | Some("project_knowledge") | Some("knowledge") => MemoryKind::ProjectKnowledge,
            Some("decision") => MemoryKind::Decision,
            Some("episodic") | Some("episode") => MemoryKind::Episodic,
            Some(other) => return Err(format!("Unknown kind \"{other}\": use decision, project_knowledge or episodic.")),
        };
        let (scope, place) = match optional_arg(args, "project") {
            Some(p) => {
                let project = self.project(&p).await?;
                (MemoryScope::Project(project.id), project.name)
            }
            None => (MemoryScope::Global, "global memory".to_owned()),
        };
        self.execute(Command::Remember(Remember { scope, kind, content, pinned: false })).await?;
        Ok(format!("Remembered in {place}."))
    }

    // ---- run ----

    async fn message_task(&self, args: &Value) -> ToolResult {
        let task = self.task(&text_arg(args, "task"), optional_arg(args, "project").as_deref()).await?;
        let text = required_arg(args, "message")?;
        let (Some(employee_id), Some(conversation_id)) = (task.employee_id, task.conversation_id) else {
            return Err(format!("\"{}\" has not started yet (no employee conversation); message it once it runs.", task.title));
        };
        let command = Command::SendEmployeeMessage(aurelius_domain::employee::SendEmployeeMessage {
            employee_id,
            text,
            conversation_id,
            images: vec![],
            files: vec![],
            interrupt: args["interrupt"].as_bool().unwrap_or(false),
        });
        self.execute(command).await?;
        Ok(format!("Sent to {} in \"{}\". Read the answer with read_task or read_conversation ({conversation_id}).", task.employee_name.as_deref().unwrap_or("the employee"), task.title))
    }

    async fn stop_task(&self, args: &Value) -> ToolResult {
        let task = self.task(&text_arg(args, "task"), optional_arg(args, "project").as_deref()).await?;
        self.execute(Command::CancelTask(aurelius_domain::task::CancelTask { task_id: task.id })).await?;
        Ok(format!("Stopped \"{}\" (cancelled).", task.title))
    }

    async fn ask_project_assistant(&self, args: &Value) -> ToolResult {
        let project = self.project(&text_arg(args, "project")).await?;
        let text = required_arg(args, "message")?;
        let conversation = match optional_arg(args, "conversation") {
            Some(c) => Some(ConversationId::parse(&c).map_err(|_| "`conversation` must be a conversation id.".to_owned())?),
            None => None,
        };
        let wait = args["wait_seconds"].as_u64().unwrap_or(120).min(ASSISTANT_WAIT_MAX_SECS);
        let command = Command::SendProjectMessage(aurelius_domain::project_conversation::SendProjectMessage {
            project_id: project.id,
            text,
            conversation_id: conversation,
            images: vec![],
            files: vec![],
            interrupt: false,
            start: ChatStart::default(),
        });
        let CommandResult::ProjectMessageSent { conversation_id, run_id, .. } = self.execute(command).await? else {
            return Err("Unexpected answer from Aurelius.".into());
        };
        let deadline = tokio::time::Instant::now() + Duration::from_secs(wait);
        loop {
            let snapshot = self.client().await.messages(conversation_id).await.map_err(|e| self.fail(e))?;
            let answer = snapshot.messages.iter().find(|m| m.role == MessageRole::Assistant && m.run_id == Some(run_id) && m.status != MessageStatus::Partial);
            if let Some(answer) = answer {
                let note = match answer.status {
                    MessageStatus::Complete => "",
                    MessageStatus::Interrupted => " (interrupted)",
                    _ => " (failed)",
                };
                return Ok(format!("{}\n\n(conversation {conversation_id}{note}; pass it as `conversation` to continue)", clip(&answer.text, MESSAGE_MAX_CHARS * 2)));
            }
            if tokio::time::Instant::now() >= deadline {
                return Ok(format!("The project assistant is still working. Read its answer later with read_conversation (conversation {conversation_id})."));
            }
            tokio::time::sleep(Duration::from_millis(800)).await;
        }
    }

    // ---- management channel (feature `management_channel`) ----

    fn has_feature(&self, feature: &str) -> bool {
        self.features.iter().any(|f| f == feature)
    }

    /// Whether this key sees `tool`: a tool of a feature the server lacks, or one the
    /// key's audience cannot use (the server refuses it as well), is hidden.
    fn available(&self, tool: &str) -> bool {
        let channel = self.has_feature(aurelius_protocol::feature::MANAGEMENT_CHANNEL);
        match tool {
            "ask_management" | "answer_question" => channel && self.audience == AppAudience::Management,
            "list_open_questions" => channel,
            "ask_project_assistant" | "set_task_mode" | "answer_approval" => self.audience == AppAudience::All,
            _ => true,
        }
    }

    /// The manager `to` names: "CEO", "CTO", "CMO" (by role or title), a name or an id.
    /// Empty: the CEO. Only the management is in the list the server gives.
    async fn manager(&self, to: Option<&str>) -> Result<EmployeeSummary, String> {
        let employees = self.employees().await?;
        let wanted = to.unwrap_or("CEO").trim().to_lowercase();
        let phrases: &[&str] = match wanted.as_str() {
            "ceo" => &["chief executive"],
            "cto" => &["chief technology", "chief technical"],
            "cmo" => &["chief marketing"],
            _ => &[],
        };
        let names = |text: &str| {
            let text = text.to_lowercase();
            text.split(|c: char| !c.is_alphanumeric()).any(|w| w == wanted) || phrases.iter().any(|p| text.contains(p))
        };
        let titled: Vec<&EmployeeSummary> = employees.iter().filter(|m| names(&m.role) || m.title.as_deref().is_some_and(names)).collect();
        match titled.len() {
            1 => return Ok(titled[0].clone()),
            0 => {}
            _ => {
                // Several: the one whose title or role is exactly the word.
                if let Some(exact) = titled.iter().find(|m| m.role.eq_ignore_ascii_case(&wanted) || m.title.as_deref().is_some_and(|t| t.eq_ignore_ascii_case(&wanted))) {
                    return Ok((*exact).clone());
                }
                let names: Vec<String> = titled.iter().map(|m| format!("{} ({})", m.name, m.id)).collect();
                return Err(format!("Several managers match \"{wanted}\": {}. Use the name or the id.", names.join(", ")));
            }
        }
        if to.is_none() {
            // No one is titled CEO: the top of the chart.
            if let Some(root) = employees.iter().find(|m| m.reports_to.is_none()) {
                return Ok(root.clone());
            }
        }
        pick(employees, to.unwrap_or("CEO"), "management member", |m| m.id.to_string(), |m| m.name.clone())
    }

    /// The assistant message that answers `run`, waiting up to `wait` seconds.
    async fn wait_reply(&self, conversation: ConversationId, run: aurelius_domain::RunId, wait: u64) -> Result<Option<(String, &'static str)>, String> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(wait);
        loop {
            let snapshot = self.client().await.messages(conversation).await.map_err(|e| self.fail(e))?;
            if let Some(answer) = snapshot.messages.iter().find(|m| m.role == MessageRole::Assistant && m.run_id == Some(run) && m.status != MessageStatus::Partial) {
                let status = match answer.status {
                    MessageStatus::Complete => "complete",
                    MessageStatus::Interrupted => "interrupted",
                    _ => "failed",
                };
                return Ok(Some((clip(&answer.text, MESSAGE_MAX_CHARS * 2), status)));
            }
            if tokio::time::Instant::now() >= deadline {
                return Ok(None);
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    /// `ask_management`: a message to a manager, in the conversation that app has with them.
    async fn ask_management(&self, args: &Value) -> ToolResult {
        let text = required_arg(args, "message")?;
        let kind = match optional_arg(args, "kind").as_deref() {
            None | Some("question") => "question",
            Some("decision") => "decision",
            Some(other) => return Err(format!("Unknown kind \"{other}\": use question or decision.")),
        };
        let manager = self.manager(optional_arg(args, "to").as_deref()).await?;
        let project = match optional_arg(args, "project") {
            Some(p) => Some(self.project(&p).await?),
            None => None,
        };
        let wait = args["wait_seconds"].as_u64().unwrap_or(0).min(ASSISTANT_WAIT_MAX_SECS);
        let mut body = match kind {
            "decision" => format!("Decision requested by the app \"{}\":\n\n{text}\n\nAnswer with your decision (approve, decline or a choice) and a short reason.", self.app),
            _ => format!("Question from the app \"{}\":\n\n{text}", self.app),
        };
        if let Some(p) = &project {
            body = format!("About the project \"{}\" (id {}).\n\n{body}", p.name, p.id);
        }
        let command = Command::SendEmployeeMessage(aurelius_domain::employee::SendEmployeeMessage {
            employee_id: manager.id,
            text: body,
            conversation_id: manager.conversation_id,
            images: vec![],
            files: vec![],
            interrupt: false,
        });
        let CommandResult::EmployeeMessageSent { conversation_id, message_id, run_id } = self.execute(command).await? else {
            return Err("Unexpected answer from Aurelius.".into());
        };
        let mut out = json!({
            "status": "sent",
            "kind": kind,
            "to": { "id": manager.id, "name": manager.name, "role": manager.role, "title": manager.title },
            "project": project.as_ref().map(|p| json!({ "id": p.id, "name": p.name })),
            "conversation_id": conversation_id,
            "message_id": message_id,
            "run_id": run_id,
            "reply": null,
        });
        if wait > 0 {
            if let Some((reply, status)) = self.wait_reply(conversation_id, run_id, wait).await? {
                out["status"] = json!("answered");
                out["reply"] = json!({ "text": reply, "status": status });
            }
        }
        Ok(json_text(&out))
    }

    /// `list_open_questions`: what waits for the user: questions and decisions of managers,
    /// tasks that wait, and the inbox items that need the user.
    async fn list_open_questions(&self, args: &Value) -> ToolResult {
        let project = match optional_arg(args, "project") {
            Some(p) => Some(self.project(&p).await?),
            None => None,
        };
        let client = self.client().await;
        let approvals = client.approvals().await.map_err(|e| self.fail(e))?.approvals;
        let tasks = client.tasks(project.as_ref().map(|p| p.id)).await.map_err(|e| self.fail(e))?.tasks;
        let notifications = client.notifications().await.map_err(|e| self.fail(e))?.notifications;
        let questions: Vec<Value> = approvals
            .iter()
            .filter(|a| a.status.as_str() == "pending" && a.tool == QUESTION_TOOL)
            .filter(|a| project.as_ref().is_none_or(|p| a.task_id.is_some_and(|t| tasks.iter().any(|x| x.id == t && x.project_id == p.id))))
            .map(question_json)
            .collect();
        let waiting: Vec<Value> = tasks
            .iter()
            .filter(|t| matches!(t.status.as_str(), "waiting" | "review"))
            .take(100)
            .map(|t| {
                json!({
                    "id": t.id,
                    "title": t.title,
                    "status": t.status.as_str(),
                    "reason": t.waiting_reason,
                    "employee": t.employee_name,
                    "project_id": t.project_id,
                    "pending_approvals": t.pending_approvals,
                    "updated_at_ms": t.updated_at_ms,
                })
            })
            .collect();
        let needs_you: Vec<Value> = notifications
            .iter()
            .filter(|n| n.severity == "action" && !n.read)
            .take(50)
            .map(|n| json!({ "id": n.id, "title": n.title, "subject": clip(&n.subject, 300), "task_id": n.task_id, "created_at_ms": n.created_at_ms }))
            .collect();
        Ok(json_text(&json!({ "questions": questions, "waiting_tasks": waiting, "needs_you": needs_you })))
    }

    /// `answer_question`: the answer goes back to the manager who asked. `id` is a
    /// question from `list_open_questions`, or a waiting task (the answer is then a
    /// message to its employee).
    async fn answer_question(&self, args: &Value) -> ToolResult {
        use aurelius_domain::approval::AnswerQuestion;
        let id = required_arg(args, "id")?;
        let answer = optional_arg(args, "answer");
        let per_question: Option<std::collections::BTreeMap<String, String>> =
            args["answers"].as_object().map(|o| o.iter().map(|(q, a)| (q.clone(), a.as_str().map(str::to_owned).unwrap_or_else(|| a.to_string()))).collect());
        if answer.is_none() && per_question.is_none() {
            return Err("`answer` is required.".into());
        }
        let client = self.client().await;
        let approvals = client.approvals().await.map_err(|e| self.fail(e))?.approvals;
        if let Some(approval) = approvals.into_iter().find(|a| a.status.as_str() == "pending" && a.tool == QUESTION_TOOL && id_matches(&a.id.to_string(), &id)) {
            let answers = match (per_question, &answer) {
                (Some(map), _) => map,
                (None, Some(answer)) => {
                    let asked = question_texts(&approval);
                    if asked.is_empty() {
                        return Err("This question has no text to answer; ask the user in Aurelius.".into());
                    }
                    asked.into_iter().map(|q| (q, answer.clone())).collect()
                }
                (None, None) => unreachable!("checked above"),
            };
            self.execute(Command::AnswerQuestion(AnswerQuestion { approval_id: approval.id, answers })).await?;
            return Ok(json_text(&json!({ "answered": "question", "id": approval.id, "from": approval.requester })));
        }
        let Some(answer) = answer else { return Err(format!("No open question matches \"{id}\". Look at list_open_questions.")) };
        let tasks = client.tasks(None).await.map_err(|e| self.fail(e))?.tasks;
        let Some(task) = tasks.into_iter().find(|t| matches!(t.status.as_str(), "waiting" | "review") && id_matches(&t.id.to_string(), &id)) else {
            return Err(format!("No open question or waiting task matches \"{id}\". Look at list_open_questions."));
        };
        let (Some(employee_id), Some(conversation_id)) = (task.employee_id, task.conversation_id) else {
            return Err(format!("\"{}\" has no employee to answer.", task.title));
        };
        let command = Command::SendEmployeeMessage(aurelius_domain::employee::SendEmployeeMessage { employee_id, text: answer, conversation_id, images: vec![], files: vec![], interrupt: false });
        self.execute(command).await?;
        Ok(json_text(&json!({ "answered": "task_message", "id": task.id, "employee": task.employee_name, "conversation_id": conversation_id })))
    }

    // ---- approve ----

    async fn set_task_mode(&self, args: &Value) -> ToolResult {
        let task = self.task(&text_arg(args, "task"), optional_arg(args, "project").as_deref()).await?;
        let mode = parse_mode(&required_arg(args, "mode")?)?;
        let Some(conversation_id) = task.conversation_id else {
            return Err(format!("\"{}\" has not started yet; give it a mode when you create it.", task.title));
        };
        self.execute(Command::SetAgentMode(aurelius_domain::autonomy::SetAgentMode { conversation_id, mode })).await?;
        Ok(format!("\"{}\" now works in {} mode.", task.title, mode.as_str()))
    }

    async fn answer_approval(&self, args: &Value) -> ToolResult {
        use aurelius_domain::approval::{AnswerQuestion, DecideApproval, DecidePlan, PlanDecision};
        let approvals = self.client().await.approvals().await.map_err(|e| self.fail(e))?.approvals;
        let pending: Vec<_> = approvals.into_iter().filter(|a| a.status.as_str() == "pending").collect();
        let approval = pick(pending, &text_arg(args, "approval"), "pending approval", |a| a.id.to_string(), |a| a.id.to_string())?;
        let decision = text_arg(args, "decision").to_lowercase();
        let feedback = optional_arg(args, "feedback");
        let command = match approval.tool.as_str() {
            PLAN_TOOL => {
                let decision = match decision.as_str() {
                    "approve" | "allow" => PlanDecision::Approve,
                    "approve_edits" => PlanDecision::ApproveEdits,
                    "revise" => PlanDecision::Revise,
                    "reject" | "deny" => PlanDecision::Reject,
                    _ => return Err("A plan takes decision approve, approve_edits, revise (with feedback) or reject.".into()),
                };
                Command::DecidePlan(DecidePlan { approval_id: approval.id, decision, feedback })
            }
            QUESTION_TOOL => {
                let Some(answers) = args["answers"].as_object() else {
                    return Err("A question takes `answers`: an object from each question to its answer.".into());
                };
                let answers = answers.iter().map(|(q, a)| (q.clone(), a.as_str().map(str::to_owned).unwrap_or_else(|| a.to_string()))).collect();
                Command::AnswerQuestion(AnswerQuestion { approval_id: approval.id, answers })
            }
            _ => {
                let approve = match decision.as_str() {
                    "allow" | "approve" => true,
                    "deny" | "reject" => false,
                    _ => return Err("An action takes decision allow or deny.".into()),
                };
                Command::DecideApproval(DecideApproval { approval_id: approval.id, action_digest: approval.action_digest.clone(), approve, always_allow: false })
            }
        };
        self.execute(command).await?;
        Ok(format!("Answered the {} from {}.", if approval.tool == PLAN_TOOL { "plan" } else if approval.tool == QUESTION_TOOL { "question" } else { "approval" }, approval.requester))
    }
}

fn object(properties: Value, required: &[&str]) -> Value {
    json!({ "type": "object", "properties": properties, "required": required })
}

/// Every tool with the scope it needs.
fn tool_catalog() -> Vec<(AppScope, Value)> {
    use AppScope::*;
    let project = json!({ "type": "string", "description": "The project's name or id." });
    let task = json!({ "type": "string", "description": "The task's id (or its first 8 characters) or title." });
    vec![
        (Read, json!({
            "name": "list_projects",
            "description": "List the active Aurelius projects with their ids. For a management key the answer is JSON: projects (id, name, description, employees_active) and task_modes, the modes create_task accepts.",
            "inputSchema": object(json!({}), &[]),
        })),
        (Read, json!({
            "name": "list_employees",
            "description": "List the company's employees (AI colleagues; any of them can work in any project): name, role, id, mode, whether they work now and who they report to.",
            "inputSchema": object(json!({}), &[]),
        })),
        (Read, json!({
            "name": "list_tasks",
            "description": "List tasks, newest first: title, status, employee, the app that created them and the id.",
            "inputSchema": object(json!({
                "project": { "type": "string", "description": "Only this project (name or id); empty: all projects." },
                "status": { "type": "string", "description": "open, ended, all (default) or one status: draft, queued, running, waiting, review, succeeded, failed, cancelled." },
                "employee": { "type": "string", "description": "Only tasks of this employee (a name)." },
                "query": { "type": "string", "description": "Words that must be in the title or goal." },
                "created_by_this_app": { "type": "boolean", "description": "Only tasks this app created." }
            }), &[]),
        })),
        (Read, json!({
            "name": "read_task",
            "description": "Read one task: status, employee, goal, criteria, its report when done and the latest messages of its conversation.",
            "inputSchema": object(json!({ "task": task, "project": { "type": "string", "description": "The project, to tell tasks with the same title apart." } }), &["task"]),
        })),
        (Read, json!({
            "name": "read_conversation",
            "description": "Read the latest messages of a project conversation, such as a task's conversation or one with the project assistant.",
            "inputSchema": object(json!({
                "conversation": { "type": "string", "description": "The conversation id." },
                "last": { "type": "integer", "description": "How many messages (default 10, at most 50)." }
            }), &["conversation"]),
        })),
        (Read, json!({
            "name": "read_inbox",
            "description": "Read the user's inbox: approvals waiting for the user and the latest results of tasks.",
            "inputSchema": object(json!({}), &[]),
        })),
        (Read, json!({
            "name": "recall",
            "description": "Search Aurelius' memory (global and, with a project, that project's and its employees') for earlier decisions and knowledge.",
            "inputSchema": object(json!({
                "query": { "type": "string", "description": "What you are looking for, in a few words." },
                "project": { "type": "string", "description": "Also search this project's memory (name or id)." }
            }), &["query"]),
        })),
        (Draft, json!({
            "name": "create_task",
            "description": "Create a task in a project. With an employee and start (the default when you may run tasks) it is queued for that employee and starts when they are free, in the employee's own mode; without, it is saved as a draft the user starts in Aurelius. The task shows this app's name.",
            "inputSchema": object(json!({
                "project": project,
                "title": { "type": "string" },
                "goal": { "type": "string", "description": "What the employee must deliver." },
                "criteria": { "type": "array", "items": { "type": "string" }, "description": "Acceptance criteria." },
                "employee": { "type": "string", "description": "The employee's name or id (see list_employees). A management key can also say CEO, CTO or CMO; default CEO." },
                "start": { "type": "boolean", "description": "Start it now (needs the run scope and an employee); false: a draft. A management key starts by default." },
                "mode": { "type": "string", "enum": ["ask", "accept_edits", "plan", "auto"], "description": "Leave empty for the employee's own mode. auto needs the approve scope (a management key never has it). A management key gets JSON back: status, task_id, title, project, employee, mode." }
            }), &["project", "title", "goal"]),
        })),
        (Draft, json!({
            "name": "add_idea",
            "description": "Note an idea for a project (an improvement, a follow-up, a bug). The user decides if and when it becomes a task.",
            "inputSchema": object(json!({
                "project": project,
                "title": { "type": "string" },
                "note": { "type": "string", "description": "Why, and what it would take." },
                "kind": { "type": "string", "enum": ["feature", "bug"] }
            }), &["project", "title"]),
        })),
        (Draft, json!({
            "name": "remember",
            "description": "Store one short, self-contained statement in Aurelius' memory: in a project (its employees see it) or, without a project, globally. Never store secrets.",
            "inputSchema": object(json!({
                "content": { "type": "string" },
                "kind": { "type": "string", "enum": ["decision", "project_knowledge", "episodic"] },
                "project": { "type": "string", "description": "The project (name or id); empty: global memory." }
            }), &["content"]),
        })),
        (Run, json!({
            "name": "message_task",
            "description": "Send a message to the employee working on a task, in the task's conversation. It is answered after the current answer, or at once with interrupt.",
            "inputSchema": object(json!({
                "task": task,
                "message": { "type": "string" },
                "interrupt": { "type": "boolean", "description": "Stop the employee's current answer and handle this now." },
                "project": { "type": "string" }
            }), &["task", "message"]),
        })),
        (Run, json!({
            "name": "stop_task",
            "description": "Stop a task: it is cancelled and its running work ends.",
            "inputSchema": object(json!({ "task": task, "project": { "type": "string" } }), &["task"]),
        })),
        (Run, json!({
            "name": "ask_project_assistant",
            "description": "Talk with a project's assistant, which knows the project and can give work to its employees. Waits for the answer (up to wait_seconds); pass the returned conversation to continue the same conversation.",
            "inputSchema": object(json!({
                "project": project,
                "message": { "type": "string" },
                "conversation": { "type": "string", "description": "Continue this conversation (id); empty: a new one." },
                "wait_seconds": { "type": "integer", "description": "How long to wait for the answer (default 120, at most 600)." }
            }), &["project", "message"]),
        })),
        (Run, json!({
            "name": "ask_management",
            "description": "Ask a manager of the company a question or for a decision (only for apps with the audience management). Starts or continues your conversation with that manager; the answer comes asynchronously (a manager.replied webhook, or read_conversation with the returned conversation_id), or set wait_seconds to wait for it here. Answers JSON: status sent|answered, to, conversation_id, message_id, run_id, reply.",
            "inputSchema": object(json!({
                "to": { "type": "string", "description": "CEO, CTO or CMO (by role or title), or a manager's name or id. Default: the CEO." },
                "project": { "type": "string", "description": "The project this is about (name or id); optional." },
                "message": { "type": "string" },
                "kind": { "type": "string", "enum": ["question", "decision"], "description": "question (default) or decision: the manager is asked for a clear decision." },
                "wait_seconds": { "type": "integer", "description": "Wait for the answer up to this long (default 0: do not wait; at most 600)." }
            }), &["message"]),
        })),
        (Read, json!({
            "name": "list_open_questions",
            "description": "What waits for the user: questions and decisions managers asked the user (questions), tasks that wait (waiting_tasks) and inbox items that need the user (needs_you). JSON. Answer a question with answer_question.",
            "inputSchema": object(json!({ "project": { "type": "string", "description": "Only this project's tasks and questions (name or id)." } }), &[]),
        })),
        (Run, json!({
            "name": "answer_question",
            "description": "Answer an open question (only for apps with the audience management): the answer goes back to the manager who asked. id is a question id from list_open_questions, or a waiting task's id (the answer is then a message to its employee). JSON: answered, id.",
            "inputSchema": object(json!({
                "id": { "type": "string", "description": "The question id (or the first 8 characters), or a waiting task's id." },
                "answer": { "type": "string", "description": "The answer; for a question with several parts it answers all of them." },
                "answers": { "type": "object", "additionalProperties": { "type": "string" }, "description": "For several parts: an object from each question's text to its answer." }
            }), &["id"]),
        })),
        (Approve, json!({
            "name": "set_task_mode",
            "description": "Change the working mode of a task that runs: ask, accept_edits, plan or auto (auto lets the employee act without asking).",
            "inputSchema": object(json!({ "task": task, "mode": { "type": "string", "enum": ["ask", "accept_edits", "plan", "auto"] }, "project": { "type": "string" } }), &["task", "mode"]),
        })),
        (Approve, json!({
            "name": "answer_approval",
            "description": "Answer something waiting for the user (see read_inbox). An action: decision allow or deny. A plan: approve, approve_edits, revise (with feedback) or reject. A question: answers, an object from each question to its answer.",
            "inputSchema": object(json!({
                "approval": { "type": "string", "description": "The approval id from read_inbox." },
                "decision": { "type": "string" },
                "feedback": { "type": "string" },
                "answers": { "type": "object", "additionalProperties": { "type": "string" } }
            }), &["approval"]),
        })),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_has_a_scope_name_and_schema() {
        let catalog = tool_catalog();
        let names: Vec<&str> = catalog.iter().map(|(_, t)| t["name"].as_str().unwrap()).collect();
        for wanted in [
            "list_projects", "list_employees", "list_tasks", "read_task", "create_task", "message_task", "stop_task", "ask_project_assistant",
            "add_idea", "recall", "remember", "read_inbox", "set_task_mode", "answer_approval", "ask_management", "list_open_questions", "answer_question",
        ] {
            assert!(names.contains(&wanted), "{wanted}");
        }
        let mut unique = names.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), names.len());
        for (_, tool) in &catalog {
            assert_eq!(tool["inputSchema"]["type"], "object", "{tool}");
            assert!(tool["description"].as_str().is_some_and(|d| !d.is_empty()));
        }
        // Approve tools are only for approve; nothing else needs it.
        let approve: Vec<&str> = catalog.iter().filter(|(s, _)| *s == AppScope::Approve).map(|(_, t)| t["name"].as_str().unwrap()).collect();
        assert_eq!(approve, vec!["set_task_mode", "answer_approval"]);
    }

    #[test]
    fn names_ids_and_prefixes_pick_one() {
        let items = vec![("0192aaaa-1111".to_owned(), "Server work".to_owned()), ("0192bbbb-2222".to_owned(), "Mobile work".to_owned())];
        let pick_name = |arg: &str| pick(items.clone(), arg, "task", |t| t.0.clone(), |t| t.1.clone());
        assert_eq!(pick_name("server work").unwrap().0, "0192aaaa-1111");
        assert_eq!(pick_name("0192bbbb").unwrap().1, "Mobile work");
        assert_eq!(pick_name("mobile").unwrap().0, "0192bbbb-2222");
        assert!(pick_name("work").unwrap_err().contains("Several"));
        assert!(pick_name("other").unwrap_err().contains("No task"));
        assert!(pick_name(" ").is_err());
    }

    #[test]
    fn ids_and_questions_are_read_from_what_managers_ask() {
        assert!(id_matches("0192aaaa-1111-7000-8000-000000000001", "0192AAAA"));
        assert!(id_matches("0192aaaa-1111", "0192aaaa-1111"));
        assert!(!id_matches("0192aaaa-1111", "0192"), "a prefix needs 8 characters");
        assert!(!id_matches("0192aaaa-1111", "0193aaaa"));
        assert!(json_text(&json!({ "a": 1 })).contains("\"a\": 1"));
    }

    #[test]
    fn auto_is_only_offered_with_approve() {
        assert_eq!(task_modes(false), vec!["ask", "accept_edits", "plan"]);
        assert_eq!(task_modes(true), vec!["ask", "accept_edits", "plan", "auto"]);
        assert!(TASK_MODES.iter().all(|m| parse_mode(m).is_ok()), "every offered mode is one parse_mode knows");
    }

    #[test]
    fn modes_parse_from_what_people_write() {
        assert_eq!(parse_mode("Accept edits").unwrap(), AgentMode::AcceptEdits);
        assert_eq!(parse_mode("auto").unwrap(), AgentMode::Auto);
        assert!(parse_mode("yolo").is_err());
        assert_eq!(clip("abcdef", 3), "abc… (cut)");
    }
}
