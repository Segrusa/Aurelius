//! stream-json line parser. Unknown line types and fields are ignored, as
//! the documentation allows new event types to be added.

use crate::{Activity, RuntimeEvent, ToolProfile};
use serde_json::Value;

#[derive(Default)]
pub struct StreamState {
    pub session_id: Option<String>,
    pub model: Option<String>,
    /// Text already emitted via partial deltas for the current message.
    streamed: String,
    saw_partials: bool,
    pending_failure: Option<RuntimeEvent>,
    /// A terminal failure was already forwarded to the run.
    pub failure_sent: bool,
    /// Plugins reported in the init line (audit only).
    pub plugins: Vec<String>,
    /// Tools the init line may list; anything else fails the run.
    allowed_tools: Vec<String>,
    /// MCP servers the init line may list.
    allowed_servers: Vec<String>,
    /// Claude in Chrome was asked for: its `mcp__claude-in-chrome__*` tools may appear.
    chrome: bool,
    /// Prefixes (`mcp__<server>__`) of the user's own MCP servers this run was given.
    user_mcp_prefixes: Vec<String>,
    /// Text streamed so far for the whole run (activity positions refer to it).
    pub text_len: usize,
    /// Output tokens of the run's finished messages.
    tokens_done: u64,
    /// Output tokens of the message streaming now (exact once its usage arrives).
    tokens_current: u64,
    /// Characters streamed in the current message, for the estimate meanwhile.
    message_chars: u64,
    /// Last reported progress (reported again only when it moved).
    tokens_reported: u64,
    /// Reset (epoch ms) from a `rate_limit_event` that rejected the request.
    limit_reset_ms: Option<i64>,
    /// Calls of the artifact tool that publish a page, until their result comes.
    artifact_publishes: Vec<(String, Option<String>)>,
    /// Usage of each model call so far, by message id (for the trace).
    steps: Vec<(String, crate::UsageStep)>,
    /// The message streaming now (its `message_delta` carries no id).
    current_message: Option<String>,
    /// Tools the init line listed.
    tools: Vec<String>,
}

impl StreamState {
    pub fn for_profile(profile: &ToolProfile) -> Self {
        let (tools, servers): (&[&str], &[&str]) = match profile {
            ToolProfile::TextOnly => (&[], &[]),
            ToolProfile::Work { .. } => (super::WORK_TOOLS, &["aurelius"]),
        };
        let mut allowed_tools: Vec<String> = tools.iter().map(|t| t.to_string()).collect();
        if matches!(profile, ToolProfile::Work { .. }) {
            allowed_tools.push(super::APPROVAL_TOOL.into());
            allowed_tools.push(super::TOOL_SEARCH_TOOL.into());
            allowed_tools.extend(super::SCREEN_TOOLS.iter().map(|t| t.to_string()));
        }
        if !servers.is_empty() {
            allowed_tools.extend(super::MEMORY_TOOLS.iter().map(|t| t.to_string()));
        }
        if matches!(profile, ToolProfile::Work { plugins, .. } if !plugins.is_empty()) {
            allowed_tools.push(super::SKILL_TOOL.into());
        }
        if matches!(profile, ToolProfile::Work { artifacts: true, .. }) {
            allowed_tools.extend(super::ARTIFACT_TOOLS.iter().map(|t| t.to_string()));
        }
        let mut allowed_servers: Vec<String> = servers.iter().map(|s| s.to_string()).collect();
        // Claude in Chrome connects itself even with --strict-mcp-config; only
        // a profile that asked for it may see it.
        let chrome = matches!(profile, ToolProfile::Work { browser: true, playwright: None, .. });
        if chrome {
            allowed_servers.push(super::CHROME_SERVER.into());
        }
        let mut user_mcp_prefixes = Vec::new();
        // Playwright instead of Chrome: its server and tools, like a user's MCP server.
        if matches!(profile, ToolProfile::Work { browser: true, playwright: Some(_), .. }) {
            allowed_servers.push(super::PLAYWRIGHT_SERVER.into());
            user_mcp_prefixes.push(super::PLAYWRIGHT_TOOL_PREFIX.into());
        }
        if let ToolProfile::Work { mcp_servers, .. } = profile {
            for name in mcp_servers.keys().filter(|n| !super::is_reserved_server(n)) {
                allowed_servers.push(name.clone());
                user_mcp_prefixes.push(format!("mcp__{}__", super::tool_server_name(name)));
            }
        }
        Self { allowed_tools, allowed_servers, chrome, user_mcp_prefixes, ..Self::default() }
    }

    /// Feed one JSONL line. `Err` means the profile was violated (fail closed).
    pub fn feed(&mut self, line: &str) -> Result<Vec<RuntimeEvent>, String> {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            return Ok(vec![]); // not JSON (e.g. a warning line): ignore
        };
        let kind = value["type"].as_str().unwrap_or("");
        match kind {
            "system" if value["subtype"] == "init" => {
                self.session_id = value["session_id"].as_str().map(str::to_owned);
                self.model = value["model"].as_str().map(str::to_owned);
                // What the model can *do* is what matters: only the profile's
                // tools and servers may appear, otherwise fail closed.
                let names = |key: &str, field: &str| -> Vec<String> {
                    value[key].as_array().map(|a| a.iter().filter_map(|t| t.as_str().or_else(|| t[field].as_str()).map(str::to_owned)).collect()).unwrap_or_default()
                };
                let chrome = self.chrome;
                let user_mcp = self.user_mcp_prefixes.clone();
                let extra_tools: Vec<String> = names("tools", "name")
                    .into_iter()
                    .filter(|t| !self.allowed_tools.contains(t) && !(chrome && t.starts_with(super::CHROME_TOOL_PREFIX)) && !user_mcp.iter().any(|p| t.starts_with(p.as_str())))
                    .collect();
                let extra_servers: Vec<String> = names("mcp_servers", "name").into_iter().filter(|s| !self.allowed_servers.contains(s)).collect();
                self.tools = names("tools", "name");
                if !extra_tools.is_empty() || !extra_servers.is_empty() {
                    // Named, so a tool a new CLI version adds can be found and allowed.
                    tracing::warn!(tools = ?extra_tools, servers = ?extra_servers, "claude code init listed tools or MCP servers outside the profile");
                    let list = |names: &[String]| if names.is_empty() { "none".to_owned() } else { names.join(", ") };
                    return Err(format!(
                        "Claude Code unexpectedly loaded tools ({}) or MCP servers ({}); the run was stopped.",
                        list(&extra_tools),
                        list(&extra_servers)
                    ));
                }
                // Plugins listed here are built into Claude Code (none are installed
                // and --safe-mode disables installed ones); without tools they cannot
                // act. Recorded for audit.
                self.plugins = value["plugins"]
                    .as_array()
                    .map(|a| a.iter().map(|p| p["name"].as_str().map(str::to_owned).unwrap_or_else(|| p.to_string())).collect())
                    .unwrap_or_default();
                if !self.plugins.is_empty() {
                    tracing::info!(plugins = ?self.plugins, "claude code init listed built-in plugins; tools and MCP are off");
                }
                let mut events = vec![RuntimeEvent::Started];
                if let Some(model) = &self.model {
                    events.push(RuntimeEvent::ModelReported(model.clone()));
                }
                Ok(events)
            }
            // The session was compacted (CLI 2.1.303: after `/compact`, also on --resume).
            "system" if value["subtype"] == "compact_boundary" => {
                let meta = &value["compact_metadata"];
                Ok(vec![RuntimeEvent::Compacted { pre_tokens: meta["pre_tokens"].as_u64().unwrap_or(0), post_tokens: meta["post_tokens"].as_u64().unwrap_or(0) }])
            }
            "stream_event" => {
                let event = &value["event"];
                let mut events = Vec::new();
                match event["type"].as_str() {
                    Some("message_start") => {
                        let subagent = !value["parent_tool_use_id"].is_null();
                        self.current_message = event["message"]["id"].as_str().map(str::to_owned);
                        if let Some(id) = self.current_message.clone() {
                            self.step(&id, &event["message"]["usage"], subagent);
                        }
                        self.tokens_done += self.tokens_current;
                        self.tokens_current = event["message"]["usage"]["output_tokens"].as_u64().unwrap_or(0);
                        self.message_chars = 0;
                    }
                    Some("message_delta") => {
                        if let Some(id) = self.current_message.clone() {
                            self.step(&id, &event["usage"], !value["parent_tool_use_id"].is_null());
                        }
                        if let Some(n) = event["usage"]["output_tokens"].as_u64() {
                            self.tokens_current = self.tokens_current.max(n);
                        }
                    }
                    // A new text block after earlier text (e.g. after a tool call): a paragraph break,
                    // so "I'll do X." and the next block's "## Result" don't run together.
                    Some("content_block_start") if event["content_block"]["type"] == "text" && !self.streamed.is_empty() && !self.streamed.ends_with('\n') => {
                        self.streamed.push_str("\n\n");
                        self.text_len += 2;
                        events.push(RuntimeEvent::TextDelta("\n\n".into()));
                    }
                    Some("content_block_delta") => {
                        let delta = &event["delta"];
                        let piece = delta["text"].as_str().or(delta["thinking"].as_str()).or(delta["partial_json"].as_str()).unwrap_or("");
                        self.message_chars += piece.chars().count() as u64;
                        self.tokens_current = self.tokens_current.max(self.message_chars / 4);
                        if delta["type"] == "text_delta" && !piece.is_empty() {
                            self.saw_partials = true;
                            self.streamed.push_str(piece);
                            self.text_len += piece.len();
                            events.push(RuntimeEvent::TextDelta(piece.to_owned()));
                        }
                    }
                    _ => {}
                }
                let total = self.tokens_done + self.tokens_current;
                // Not for every few characters: when it moved by 20 tokens, or exactly at a message's end.
                if total != self.tokens_reported && (total >= self.tokens_reported + 20 || event["type"] == "message_delta") {
                    self.tokens_reported = total;
                    events.push(RuntimeEvent::Progress { output_tokens: total });
                }
                Ok(events)
            }
            "assistant" => {
                if let Some(code) = value["error"].as_str() {
                    self.pending_failure = Some(RuntimeEvent::Failed { code: code.to_owned(), message: describe(code) });
                }
                // A subagent's messages carry the call that started it.
                let parent = value["parent_tool_use_id"].as_str().map(str::to_owned);
                let id = value["message"]["id"].as_str().map(str::to_owned).unwrap_or_else(|| format!("step-{}", self.steps.len() + 1));
                let step = self.step(&id, &value["message"]["usage"], parent.is_some());
                let called = value["message"]["content"].as_array().into_iter().flatten().filter(|b| b["type"] == "tool_use").filter_map(|b| b["name"].as_str());
                for tool in called {
                    if !step.tools.iter().any(|t| t == tool) {
                        step.tools.push(tool.to_owned());
                    }
                }
                if let Some(blocks) = value["message"]["content"].as_array() {
                    for b in blocks.iter().filter(|b| b["type"] == "tool_use") {
                        if super::artifact_call_publishes(b["name"].as_str().unwrap_or(""), &b["input"]) {
                            let file = b["input"]["file_path"].as_str().map(str::to_owned);
                            self.artifact_publishes.extend(b["id"].as_str().map(|id| (id.to_owned(), file)));
                        }
                    }
                }
                // Tool calls are part of the complete message (also with partials).
                let mut events: Vec<RuntimeEvent> = value["message"]["content"]
                    .as_array()
                    .map(|blocks| {
                        blocks
                            .iter()
                            .filter(|b| b["type"] == "tool_use" && b["name"].as_str() != Some(super::APPROVAL_TOOL))
                            .map(|b| RuntimeEvent::Activity(Activity::Started {
                                call_id: b["id"].as_str().unwrap_or_default().to_owned(),
                                tool: b["name"].as_str().unwrap_or_default().to_owned(),
                                input: b["input"].clone(),
                                parent: parent.clone(),
                            }))
                            .collect()
                    })
                    .unwrap_or_default();
                // Without partial messages, emit the complete text once. A subagent's
                // text is its own work, not the answer: its report comes back as the
                // tool result.
                if self.saw_partials || !value["parent_tool_use_id"].is_null() {
                    return Ok(events);
                }
                let text: String = value["message"]["content"]
                    .as_array()
                    .map(|blocks| blocks.iter().filter_map(|b| (b["type"] == "text").then(|| b["text"].as_str()).flatten()).collect())
                    .unwrap_or_default();
                if text.is_empty() {
                    return Ok(events);
                }
                let text = if !self.streamed.is_empty() && !self.streamed.ends_with('\n') { format!("\n\n{text}") } else { text };
                self.streamed.push_str(&text);
                self.text_len += text.len();
                events.insert(0, RuntimeEvent::TextDelta(text));
                Ok(events)
            }
            // Tool results come back as a "user" message.
            "user" => {
                let mut events: Vec<RuntimeEvent> = value["message"]["content"]
                    .as_array()
                    .map(|blocks| {
                        blocks
                            .iter()
                            .filter(|b| b["type"] == "tool_result")
                            .map(|b| RuntimeEvent::Activity(Activity::Finished {
                                call_id: b["tool_use_id"].as_str().unwrap_or_default().to_owned(),
                                ok: b["is_error"] != true,
                                output: tool_output(&b["content"]),
                            }))
                            .collect()
                    })
                    .unwrap_or_default();
                events.extend(self.artifact_published(&value));
                Ok(events)
            }
            // The limit's state before the result: only a rejection carries the reset that counts.
            "rate_limit_event" => {
                let info = &value["rate_limit_info"];
                if info["status"] == "rejected" {
                    self.limit_reset_ms = info["resetsAt"].as_i64().map(epoch_ms);
                }
                Ok(vec![])
            }
            "result" => {
                // A "success" subtype can still carry is_error: check both (docs).
                let ok = value["subtype"] == "success" && value["is_error"] != true;
                if ok {
                    // If nothing was streamed, the result text is the answer.
                    let mut events = Vec::new();
                    if self.streamed.is_empty() {
                        if let Some(text) = value["result"].as_str().filter(|t| !t.is_empty()) {
                            events.push(RuntimeEvent::TextDelta(text.to_owned()));
                        }
                    }
                    if let Some(mut usage) = usage_of(&value) {
                        usage.steps = self.steps.iter().map(|(_, s)| s.clone()).collect();
                        usage.tools = self.tools.clone();
                        events.push(RuntimeEvent::Usage(usage));
                    }
                    events.push(RuntimeEvent::Completed);
                    return Ok(events);
                }
                let code = self
                    .pending_failure
                    .take()
                    .and_then(|f| if let RuntimeEvent::Failed { code, .. } = f { Some(code) } else { None })
                    .or_else(|| value["subtype"].as_str().map(str::to_owned))
                    .unwrap_or_else(|| "error".into());
                let result_text = value["result"].as_str().unwrap_or_default();
                if code == "rate_limit" || quota_in(result_text).is_some() {
                    return Ok(vec![RuntimeEvent::Quota { reset_at_ms: quota_in(result_text).flatten().or(self.limit_reset_ms) }]);
                }
                let errors = value["errors"].as_array().map(|e| e.iter().filter_map(Value::as_str).collect::<Vec<_>>().join("; ")).unwrap_or_default();
                let message = if errors.is_empty() { describe(&code) } else { errors };
                Ok(vec![RuntimeEvent::Failed { code, message }])
            }
            _ => Ok(vec![]),
        }
    }
}

impl StreamState {
    /// The step of message `id`, with what `usage` says of it now (each count only grows:
    /// a message's lines repeat it, partly filled while it streams).
    fn step(&mut self, id: &str, usage: &Value, subagent: bool) -> &mut crate::UsageStep {
        let i = match self.steps.iter().position(|(s, _)| s == id) {
            Some(i) => i,
            None => {
                self.steps.push((id.to_owned(), crate::UsageStep { subagent, ..Default::default() }));
                self.steps.len() - 1
            }
        };
        let step = &mut self.steps[i].1;
        let n = |k: &str| usage[k].as_u64().unwrap_or(0);
        step.input_tokens = step.input_tokens.max(n("input_tokens"));
        step.cache_creation_tokens = step.cache_creation_tokens.max(n("cache_creation_input_tokens"));
        step.cache_read_tokens = step.cache_read_tokens.max(n("cache_read_input_tokens"));
        step.output_tokens = step.output_tokens.max(n("output_tokens"));
        step
    }

    /// The result of a publish call, as the stream-json `user` line carries it
    /// next to the tool result (CLI 2.1.289): `tool_use_result` with `url`,
    /// `title`, `seq` (the version number) and `path`. A refused or failed publish is none.
    fn artifact_published(&mut self, value: &Value) -> Option<RuntimeEvent> {
        let blocks = value["message"]["content"].as_array()?;
        let block = blocks.iter().find(|b| b["type"] == "tool_result" && self.artifact_publishes.iter().any(|(c, _)| b["tool_use_id"].as_str() == Some(c.as_str())))?;
        let call = block["tool_use_id"].as_str().unwrap_or_default().to_owned();
        let input_path = self.artifact_publishes.iter().find(|(c, _)| *c == call).and_then(|(_, p)| p.clone());
        self.artifact_publishes.retain(|(c, _)| *c != call);
        let result = &value["tool_use_result"];
        let url = result["url"].as_str().filter(|u| u.starts_with("https://claude.ai/"))?;
        if block["is_error"] == true {
            return None;
        }
        Some(RuntimeEvent::ArtifactPublished {
            url: url.to_owned(),
            title: result["title"].as_str().unwrap_or_default().to_owned(),
            version: result["seq"].as_u64().and_then(|n| u32::try_from(n).ok()),
            // The result's `path` is the file it published; else the call's `file_path`.
            path: result["path"].as_str().map(str::to_owned).or(input_path),
        })
    }
}

/// Epoch seconds (or already milliseconds) as milliseconds.
fn epoch_ms(n: i64) -> i64 {
    if n < 100_000_000_000 { n * 1000 } else { n }
}

/// A usage-limit message: `Some(reset)` when the text says the limit is reached
/// ("Claude AI usage limit reached|1760000000"), the reset only when it names one.
pub fn quota_in(text: &str) -> Option<Option<i64>> {
    let lower = text.to_lowercase();
    if !(lower.contains("usage limit reached") || lower.contains("limit reached|") || lower.contains("hit your limit")) {
        return None;
    }
    let after = text.split_once('|').map(|(_, rest)| rest);
    Some(after.and_then(|rest| rest.trim().split(|c: char| !c.is_ascii_digit()).next()?.parse::<i64>().ok()).filter(|n| *n > 1_000_000_000).map(epoch_ms))
}

/// Text of a tool result (a string or text blocks), capped for storage.
fn tool_output(content: &Value) -> String {
    let text = match content {
        Value::String(s) => s.clone(),
        Value::Array(blocks) => blocks.iter().filter_map(|b| b["text"].as_str()).collect::<Vec<_>>().join("\n"),
        _ => String::new(),
    };
    const MAX: usize = 8_000;
    if text.len() > MAX {
        let mut cut = MAX;
        while !text.is_char_boundary(cut) {
            cut -= 1;
        }
        format!("{}\n… ({} more characters)", &text[..cut], text.len() - cut)
    } else {
        text
    }
}

/// User-facing meaning of Claude Code's assistant error codes.
pub fn describe(code: &str) -> String {
    match code {
        "authentication_failed" => "Claude Code is not signed in. Sign in within Claude Code (`claude auth login`).".into(),
        "billing_error" => "There is a billing problem with your Claude account.".into(),
        "rate_limit" => "Your Claude limit has been reached; try again later.".into(),
        "overloaded" => "The Claude service is temporarily overloaded.".into(),
        "model_not_found" => "This model is not available for your account.".into(),
        "invalid_request" => "Claude Code rejected the request as invalid.".into(),
        other => format!("Claude Code reported an error: {other}"),
    }
}

/// Usage from the final `result` message. Via a subscription the cost is the
/// API-equivalent value, never an extra bill: always marked as an estimate.
fn usage_of(value: &Value) -> Option<crate::Usage> {
    let u = &value["usage"];
    if u.is_null() && value["total_cost_usd"].is_null() {
        return None;
    }
    let n = |k: &str| u[k].as_u64().unwrap_or(0);
    Some(crate::Usage {
        input_tokens: n("input_tokens") + n("cache_creation_input_tokens"),
        output_tokens: n("output_tokens"),
        cached_tokens: n("cache_read_input_tokens"),
        cache_creation_tokens: n("cache_creation_input_tokens"),
        cost_micro_usd: value["total_cost_usd"].as_f64().map(|c| (c * 1_000_000.0).round() as i64),
        estimate: true,
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const INIT: &str = r#"{"type":"system","subtype":"init","session_id":"s1","model":"claude-opus-5-5","tools":[],"mcp_servers":[],"plugins":[]}"#;

    /// An employee at work: the profile with web search among its tools.
    fn work() -> ToolProfile {
        ToolProfile::Work {
            folders: vec!["C:/p".into()], permission_mode: "manual".into(), approval_bridge: crate::ApprovalBridge { program: "x".into(), args: vec![] },
            browser: false, browser_session: None, playwright: None, mcp_servers: Default::default(), plugins: vec![], artifacts: false,
        }
    }

    #[test]
    fn a_compaction_reports_its_sizes() {
        // As Claude Code 2.1.303 answers `/compact` on a resumed session (probe 2026-10-19).
        let mut s = StreamState::default();
        assert!(s.feed(r#"{"type":"system","subtype":"status","status":"compacting","session_id":"s1"}"#).unwrap().is_empty());
        assert!(s.feed(r#"{"type":"system","subtype":"status","status":null,"compact_result":"success","session_id":"s1"}"#).unwrap().is_empty());
        s.feed(INIT).unwrap();
        let boundary = r#"{"type":"system","subtype":"compact_boundary","session_id":"s1","compact_metadata":{"trigger":"manual","pre_tokens":34747,"post_tokens":5059}}"#;
        assert_eq!(s.feed(boundary).unwrap(), vec![RuntimeEvent::Compacted { pre_tokens: 34747, post_tokens: 5059 }]);
        assert!(s.feed(r#"{"type":"user","message":{"role":"user","content":"<local-command-stdout>Compacted </local-command-stdout>"},"isReplay":true}"#).unwrap().is_empty());
        let done = s.feed(r#"{"type":"result","subtype":"success","is_error":false,"num_turns":0,"result":"","total_cost_usd":0.0068,"usage":{"input_tokens":0,"cache_creation_input_tokens":0,"cache_read_input_tokens":0,"output_tokens":0}}"#).unwrap();
        assert_eq!(done.last(), Some(&RuntimeEvent::Completed));
    }

    #[test]
    fn partial_stream_then_success() {
        let mut s = StreamState::default();
        assert_eq!(s.feed(INIT).unwrap(), vec![RuntimeEvent::Started, RuntimeEvent::ModelReported("claude-opus-5-5".into())]);
        let d = r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hal"}}}"#;
        assert_eq!(s.feed(d).unwrap(), vec![RuntimeEvent::TextDelta("Hal".into())]);
        let full = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Hallo"}]}}"#;
        assert!(s.feed(full).unwrap().is_empty(), "no duplicate text after partials");
        let result = r#"{"type":"result","subtype":"success","is_error":false,"result":"Hallo","session_id":"s1"}"#;
        assert_eq!(s.feed(result).unwrap(), vec![RuntimeEvent::Completed]);
        assert_eq!(s.session_id.as_deref(), Some("s1"));
    }

    #[test]
    fn progress_counts_output_tokens_across_messages() {
        let mut s = StreamState::default();
        s.feed(INIT).unwrap();
        let progress = |events: Vec<RuntimeEvent>| events.into_iter().find_map(|e| match e { RuntimeEvent::Progress { output_tokens } => Some(output_tokens), _ => None });
        s.feed(r#"{"type":"stream_event","event":{"type":"message_start","message":{"usage":{"output_tokens":1}}}}"#).unwrap();
        // Thinking counts too (estimated while it streams: 4 characters a token).
        let think = format!(r#"{{"type":"stream_event","event":{{"type":"content_block_delta","index":0,"delta":{{"type":"thinking_delta","thinking":"{}"}}}}}}"#, "x".repeat(400));
        assert_eq!(progress(s.feed(&think).unwrap()), Some(100));
        // A tiny delta does not report again.
        assert_eq!(progress(s.feed(r#"{"type":"stream_event","event":{"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"Hi"}}}"#).unwrap()), None);
        // The message's own count replaces the estimate.
        assert_eq!(progress(s.feed(r#"{"type":"stream_event","event":{"type":"message_delta","usage":{"output_tokens":130}}}"#).unwrap()), Some(130));
        // The next message adds up.
        s.feed(r#"{"type":"stream_event","event":{"type":"message_start","message":{"usage":{"output_tokens":1}}}}"#).unwrap();
        assert_eq!(progress(s.feed(r#"{"type":"stream_event","event":{"type":"message_delta","usage":{"output_tokens":70}}}"#).unwrap()), Some(200));
    }

    #[test]
    fn each_model_call_keeps_its_own_usage_with_the_cache_apart() {
        let mut s = StreamState::for_profile(&work());
        s.feed(r#"{"type":"system","subtype":"init","tools":["WebSearch"],"mcp_servers":[]}"#).unwrap();
        // As CLI 2.1.289 streams it: message_start, the assistant line per block, message_delta.
        s.feed(r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg_1","usage":{"input_tokens":3,"cache_creation_input_tokens":12000,"cache_read_input_tokens":9000,"output_tokens":1}}}}"#).unwrap();
        s.feed(r#"{"type":"assistant","message":{"id":"msg_1","content":[{"type":"tool_use","id":"t1","name":"WebSearch","input":{}}],"usage":{"input_tokens":3,"cache_creation_input_tokens":12000,"cache_read_input_tokens":9000,"output_tokens":1}}}"#).unwrap();
        s.feed(r#"{"type":"stream_event","event":{"type":"message_delta","usage":{"output_tokens":80}}}"#).unwrap();
        s.feed(r#"{"type":"assistant","parent_tool_use_id":"toolu_A","message":{"id":"msg_2","content":[{"type":"text","text":"x"}],"usage":{"input_tokens":5,"cache_read_input_tokens":21000,"output_tokens":40}}}"#).unwrap();
        let events = s.feed(r#"{"type":"result","subtype":"success","is_error":false,"result":"x","usage":{"input_tokens":8,"cache_creation_input_tokens":12000,"cache_read_input_tokens":30000,"output_tokens":120}}"#).unwrap();
        let usage = events.iter().find_map(|e| match e { RuntimeEvent::Usage(u) => Some(u.clone()), _ => None }).unwrap();
        assert_eq!((usage.input_tokens, usage.cache_creation_tokens, usage.cached_tokens), (12008, 12000, 30000), "cache writes are counted as input and kept apart");
        assert_eq!(usage.tools, ["WebSearch"]);
        assert_eq!(
            usage.steps,
            vec![
                crate::UsageStep { input_tokens: 3, cache_creation_tokens: 12000, cache_read_tokens: 9000, output_tokens: 80, tools: vec!["WebSearch".into()], subagent: false },
                crate::UsageStep { input_tokens: 5, cache_creation_tokens: 0, cache_read_tokens: 21000, output_tokens: 40, tools: vec![], subagent: true },
            ]
        );
    }

    #[test]
    fn usage_limit_in_result_text_is_a_quota_with_its_reset() {
        let mut s = StreamState::for_profile(&ToolProfile::TextOnly);
        let events = s.feed(r#"{"type":"result","subtype":"success","is_error":true,"result":"Claude AI usage limit reached|1760000000"}"#).unwrap();
        assert_eq!(events, vec![RuntimeEvent::Quota { reset_at_ms: Some(1_760_000_000_000) }]);
    }

    #[test]
    fn rate_limit_event_gives_the_reset_when_the_result_has_none() {
        let mut s = StreamState::for_profile(&ToolProfile::TextOnly);
        assert!(s.feed(r#"{"type":"rate_limit_event","rate_limit_info":{"status":"allowed_warning","resetsAt":1}}"#).unwrap().is_empty());
        assert!(s.feed(r#"{"type":"rate_limit_event","rate_limit_info":{"status":"rejected","resetsAt":1760000000,"rateLimitType":"five_hour"}}"#).unwrap().is_empty());
        s.feed(r#"{"type":"assistant","error":"rate_limit","message":{"content":[]}}"#).unwrap();
        let events = s.feed(r#"{"type":"result","subtype":"success","is_error":true,"result":""}"#).unwrap();
        assert_eq!(events, vec![RuntimeEvent::Quota { reset_at_ms: Some(1_760_000_000_000) }]);
    }

    #[test]
    fn rate_limit_without_any_reset_is_a_quota_without_time() {
        let mut s = StreamState::for_profile(&ToolProfile::TextOnly);
        s.feed(r#"{"type":"assistant","error":"rate_limit","message":{"content":[]}}"#).unwrap();
        let events = s.feed(r#"{"type":"result","subtype":"success","is_error":true,"result":"limit"}"#).unwrap();
        assert_eq!(events, vec![RuntimeEvent::Quota { reset_at_ms: None }]);
    }

    #[test]
    fn quota_text_in_stderr() {
        assert_eq!(quota_in("Error: Claude AI usage limit reached|1760000000
"), Some(Some(1_760_000_000_000)));
        assert_eq!(quota_in("You've hit your limit"), Some(None));
        assert_eq!(quota_in("some other crash"), None);
    }

    #[test]
    fn success_subtype_with_is_error_is_a_failure() {
        let mut s = StreamState::default();
        s.feed(INIT).unwrap();
        s.feed(r#"{"type":"assistant","error":"authentication_failed","message":{"content":[]}}"#).unwrap();
        let events = s.feed(r#"{"type":"result","subtype":"success","is_error":true,"result":""}"#).unwrap();
        assert!(matches!(&events[..], [RuntimeEvent::Failed { code, .. }] if code == "authentication_failed"));
    }

    #[test]
    fn loaded_tools_or_mcp_violate_the_text_only_profile() {
        let mut s = StreamState::default();
        let init = r#"{"type":"system","subtype":"init","tools":["Bash"],"mcp_servers":[],"plugins":[]}"#;
        assert!(s.feed(init).is_err());
        let mut s = StreamState::default();
        let init = r#"{"type":"system","subtype":"init","tools":[],"mcp_servers":[{"name":"x"}],"plugins":[]}"#;
        assert!(s.feed(init).is_err());
    }

    #[test]
    fn a_profile_violation_names_the_tools_and_servers() {
        let init = r#"{"type":"system","subtype":"init","tools":["Bash","Monitor"],"mcp_servers":[{"name":"x"}],"plugins":[]}"#;
        let message = StreamState::default().feed(init).unwrap_err();
        assert!(message.contains("tools (Bash, Monitor)") && message.contains("MCP servers (x)"), "{message}");
        let init = r#"{"type":"system","subtype":"init","tools":["Bash"],"mcp_servers":[],"plugins":[]}"#;
        assert!(StreamState::default().feed(init).unwrap_err().contains("MCP servers (none)"));
    }

    #[test]
    fn built_in_plugins_without_tools_are_allowed_and_recorded() {
        let mut s = StreamState::default();
        let init = r#"{"type":"system","subtype":"init","tools":[],"mcp_servers":[],"plugins":[{"name":"a"},{"name":"b"},{"name":"c"}]}"#;
        assert_eq!(s.feed(init).unwrap(), vec![RuntimeEvent::Started]);
        assert_eq!(s.plugins, ["a", "b", "c"]);
    }

    #[test]
    fn unknown_lines_are_ignored_and_result_text_is_used_without_deltas() {
        let mut s = StreamState::default();
        assert!(s.feed("not json").unwrap().is_empty());
        assert!(s.feed(r#"{"type":"system","subtype":"api_retry","attempt":1}"#).unwrap().is_empty());
        // The answer to a mode switch (CLI 2.1.287) and its status line.
        assert!(s.feed(r#"{"type":"control_response","response":{"subtype":"success","request_id":"mode-1","response":{"mode":"acceptEdits"}}}"#).unwrap().is_empty());
        assert!(s.feed(r#"{"type":"system","subtype":"status","status":null,"permissionMode":"acceptEdits"}"#).unwrap().is_empty());
        let events = s.feed(r#"{"type":"result","subtype":"success","is_error":false,"result":"Klaar"}"#).unwrap();
        assert!(!events.iter().any(|e| matches!(e, RuntimeEvent::Usage(_))));
        let with_usage = s.feed(r#"{"type":"result","subtype":"success","is_error":false,"result":"x","total_cost_usd":0.0123,"usage":{"input_tokens":10,"cache_creation_input_tokens":5,"cache_read_input_tokens":100,"output_tokens":7}}"#).unwrap();
        assert!(with_usage.contains(&RuntimeEvent::Usage(crate::Usage { input_tokens: 15, output_tokens: 7, cached_tokens: 100, cache_creation_tokens: 5, cost_micro_usd: Some(12300), estimate: true, ..Default::default() })));
        assert_eq!(events, vec![RuntimeEvent::TextDelta("Klaar".into()), RuntimeEvent::Completed]);
    }

    #[test]
    fn a_subagents_text_is_not_the_answer_but_its_tools_are_activity() {
        let mut s = StreamState::default();
        let events = s
            .feed(r#"{"type":"assistant","parent_tool_use_id":"toolu_A","message":{"content":[{"type":"text","text":"Ik lees het bestand."},{"type":"tool_use","id":"toolu_R","name":"Read","input":{"file_path":"secret.txt"}}]}}"#)
            .unwrap();
        assert!(!events.iter().any(|e| matches!(e, RuntimeEvent::TextDelta(_))));
        assert!(matches!(&events[..], [RuntimeEvent::Activity(Activity::Started { tool, .. })] if tool == "Read"));
        let events = s.feed(r#"{"type":"result","subtype":"success","is_error":false,"result":"ZEEPAARDJE"}"#).unwrap();
        assert_eq!(events, vec![RuntimeEvent::TextDelta("ZEEPAARDJE".into()), RuntimeEvent::Completed]);
    }

    #[test]
    fn error_result_lists_errors() {
        let mut s = StreamState::default();
        let events = s.feed(r#"{"type":"result","subtype":"error_during_execution","is_error":true,"errors":["boom"]}"#).unwrap();
        assert_eq!(events, vec![RuntimeEvent::Failed { code: "error_during_execution".into(), message: "boom".into() }]);
    }

    #[test]
    fn arguments_are_a_text_only_profile() {
        let args = super::super::arguments(Some("claude-sonnet-5-5"), Some(std::path::Path::new("system.md")), Some("high"));
        let joined = args.join(" ");
        assert!(joined.contains("--append-system-prompt-file system.md"), "the system text never goes on the command line");
        assert!(joined.contains("--tools  --disallowedTools mcp__*"));
        assert!(joined.contains("--safe-mode"));
        assert!(joined.contains("--effort high"));
        assert!(!super::super::arguments(None, None, Some("ludicrous")).contains(&"--effort".to_string()), "unknown levels are dropped");
        assert!(joined.contains("--permission-prompts none"));
        assert!(!joined.contains("bypass"), "never bypass permissions");
        assert!(!joined.contains("--bare"), "--bare would disable the user's own login");
        assert_eq!(args.last().unwrap(), "system.md");
    }

    #[test]
    fn work_profile_allows_its_tools_and_bridge_only() {
        let work = ToolProfile::Work {
            folders: vec!["C:/p".into(), "C:/w".into()],
            permission_mode: "acceptEdits".into(),
            approval_bridge: crate::ApprovalBridge { program: "ctl".into(), args: vec!["mcp-permissions".into()] },
            browser: false,
            browser_session: None,
            playwright: None,
            mcp_servers: Default::default(),
            plugins: vec![],
            artifacts: false,
        };
        let mut s = StreamState::for_profile(&work);
        let init = r#"{"type":"system","subtype":"init","tools":["Bash","Edit","mcp__aurelius__approve"],"mcp_servers":[{"name":"aurelius","status":"connected"}],"plugins":[]}"#;
        assert!(s.feed(init).is_ok());
        let mut s = StreamState::for_profile(&work);
        let init = r#"{"type":"system","subtype":"init","tools":["Bash","Agent"],"mcp_servers":[],"plugins":[]}"#;
        assert!(s.feed(init).is_err(), "a tool outside the profile fails the run");
        let args = crate::claude_code::profile_arguments(&work, None, None, None).join(" ");
        assert!(args.contains("--restricted") && args.contains("--permission-mode acceptEdits"));
        assert!(args.contains("--permission-prompt-tool mcp__aurelius__approve") && args.contains("--add-dir C:/w"));
        assert!(!args.contains("bypass") && !args.contains("--safe-mode"));
        let bad = ToolProfile::Work { folders: vec![], permission_mode: "bypassPermissions".into(), approval_bridge: crate::ApprovalBridge { program: "x".into(), args: vec![] }, browser: false, browser_session: None, playwright: None, mcp_servers: Default::default(), plugins: vec![], artifacts: false };
        assert!(crate::claude_code::profile_arguments(&bad, None, None, None).join(" ").contains("--permission-mode manual"), "never bypass");
    }

    #[test]
    fn enabled_user_mcp_servers_load_next_to_the_bridge_and_only_those() {
        let bridge = || crate::ApprovalBridge { program: "x".into(), args: vec![] };
        let mut servers = std::collections::BTreeMap::new();
        servers.insert("my.srv".to_string(), serde_json::json!({ "type": "stdio", "command": "node" }));
        servers.insert("aurelius".to_string(), serde_json::json!({ "command": "evil" }));
        let p = ToolProfile::Work { folders: vec!["C:/p".into()], permission_mode: "manual".into(), approval_bridge: bridge(), browser: false, browser_session: None, playwright: None, mcp_servers: servers, plugins: vec![], artifacts: false };
        let args = crate::claude_code::profile_arguments(&p, None, None, None);
        assert!(args.contains(&"--strict-mcp-config".to_string()));
        let mcp: serde_json::Value = serde_json::from_str(&args[args.iter().position(|a| a == "--mcp-config").unwrap() + 1]).unwrap();
        assert_eq!(mcp["mcpServers"]["my.srv"]["command"], "node");
        assert_eq!(mcp["mcpServers"]["aurelius"]["type"], "stdio", "the bridge is never replaced");
        let init = r#"{"type":"system","subtype":"init","tools":["Bash","mcp__aurelius__approve","mcp__my_srv__go"],"mcp_servers":[{"name":"aurelius"},{"name":"my.srv"}]}"#;
        assert!(StreamState::for_profile(&p).feed(init).is_ok());
        let none = ToolProfile::Work { folders: vec!["C:/p".into()], permission_mode: "manual".into(), approval_bridge: bridge(), browser: false, browser_session: None, playwright: None, mcp_servers: Default::default(), plugins: vec![], artifacts: false };
        assert!(StreamState::for_profile(&none).feed(init).is_err(), "without enabling, the server must not appear");
        let plain = crate::claude_code::profile_arguments(&none, None, None, None);
        let mcp: serde_json::Value = serde_json::from_str(&plain[plain.iter().position(|a| a == "--mcp-config").unwrap() + 1]).unwrap();
        assert_eq!(mcp["mcpServers"].as_object().unwrap().len(), 1);
    }

    #[test]
    fn artifacts_add_their_tools_only_when_on() {
        let bridge = || crate::ApprovalBridge { program: "x".into(), args: vec![] };
        let profile = |artifacts| ToolProfile::Work { folders: vec!["C:/p".into()], permission_mode: "manual".into(), approval_bridge: bridge(), browser: false, browser_session: None, playwright: None, mcp_servers: Default::default(), plugins: vec![], artifacts };
        let off = crate::claude_code::profile_arguments(&profile(false), None, None, None).join(" ");
        assert!(!off.contains("Artifact"), "off: the run is unchanged");
        let on = crate::claude_code::profile_arguments(&profile(true), None, None, None);
        let tools = &on[on.iter().position(|a| a == "--tools").unwrap() + 1];
        assert!(tools.ends_with(",Artifact,ArtifactData,ArtifactComments"), "{tools}");
        let allowed = &on[on.iter().position(|a| a == "--allowedTools").unwrap() + 1];
        assert!(!allowed.contains("Artifact"), "a publish is never allowed by rule");
        // The init line of CLI 2.1.289 with artifacts on.
        let init = r#"{"type":"system","subtype":"init","tools":["Bash","Artifact","ArtifactComments","ArtifactData","mcp__aurelius__approve"],"mcp_servers":[{"name":"aurelius"}]}"#;
        assert!(StreamState::for_profile(&profile(true)).feed(init).is_ok());
        assert!(StreamState::for_profile(&profile(false)).feed(init).is_err(), "off: the tools must not appear");
    }

    #[test]
    fn a_successful_publish_is_reported_once_with_its_url_title_and_version() {
        let mut s = StreamState::default();
        // As CLI 2.1.289 streams a publish (shortened).
        let call = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_P","name":"Artifact","input":{"file_path":"C:\\t\\spike.html","icon":"page","description":"A test."}}]}}"#;
        s.feed(call).unwrap();
        let result = r#"{"type":"user","message":{"role":"user","content":[{"tool_use_id":"toolu_P","type":"tool_result","content":"Published C:\\t\\spike.html at https://claude.ai/artifact/2Jwgr8SC4Y55s2rme4hyfb (Version 2, version id 1791545135-6850)"}]},"tool_use_result":{"url":"https://claude.ai/artifact/2Jwgr8SC4Y55s2rme4hyfb","path":"C:\\t\\spike.html","artifact_id":"0a9a7c1d-4f0e-4c6d-89f4-dc9da2f7b0ae","title":"Aurelius spike test","updated":true,"audience":"owner","seq":2,"version":"1791545135-6850"}}"#;
        let events = s.feed(result).unwrap();
        assert_eq!(
            events.last(),
            Some(&RuntimeEvent::ArtifactPublished { url: "https://claude.ai/artifact/2Jwgr8SC4Y55s2rme4hyfb".into(), title: "Aurelius spike test".into(), version: Some(2), path: Some(r"C:\t\spike.html".into()) })
        );
        assert!(!s.feed(result).unwrap().iter().any(|e| matches!(e, RuntimeEvent::ArtifactPublished { .. })), "reported once");
        // A read of an artifact also carries its URL, but publishes nothing.
        s.feed(r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_R","name":"Artifact","input":{"action":"read","url":"https://claude.ai/artifact/x"}}]}}"#).unwrap();
        let read = r#"{"type":"user","message":{"content":[{"tool_use_id":"toolu_R","type":"tool_result","content":"ok"}]},"tool_use_result":{"read":{"url":"https://claude.ai/artifact/x"},"url":"https://claude.ai/artifact/x","title":"x"}}"#;
        assert!(!s.feed(read).unwrap().iter().any(|e| matches!(e, RuntimeEvent::ArtifactPublished { .. })));
        // A refused publish is not one.
        s.feed(r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_D","name":"Artifact","input":{"file_path":"a.html"}}]}}"#).unwrap();
        let denied = r#"{"type":"user","message":{"content":[{"tool_use_id":"toolu_D","type":"tool_result","is_error":true,"content":"Permission denied"}]},"tool_use_result":"Error: denied"}"#;
        assert!(!s.feed(denied).unwrap().iter().any(|e| matches!(e, RuntimeEvent::ArtifactPublished { .. })));
        // Without a `path` in the result, the call's own file.
        s.feed(r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_M","name":"Artifact","input":{"file_path":"notes.md"}}]}}"#).unwrap();
        let result = r#"{"type":"user","message":{"content":[{"tool_use_id":"toolu_M","type":"tool_result","content":"Published"}]},"tool_use_result":{"url":"https://claude.ai/artifact/m","title":"Notes","seq":1}}"#;
        assert!(s.feed(result).unwrap().iter().any(|e| matches!(e, RuntimeEvent::ArtifactPublished { path: Some(p), .. } if p == "notes.md")));
    }

    #[test]
    fn artifact_calls_read_or_act_by_their_action() {
        use crate::claude_code::{artifact_call_publishes, artifact_call_reads};
        use serde_json::json as j;
        assert_eq!(artifact_call_reads("Artifact", &j!({"file_path": "a.html"})), Some(false), "no action is a publish");
        assert_eq!(artifact_call_reads("Artifact", &j!({"action": "list"})), Some(true));
        assert_eq!(artifact_call_reads("Artifact", &j!({"action": "read", "url": "u"})), Some(true));
        for action in ["publish", "delete", "pin", "unpin", "open", "quickstart"] {
            assert_eq!(artifact_call_reads("Artifact", &j!({ "action": action })), Some(false), "{action}");
        }
        for action in ["get", "list", "query", "profiles"] {
            assert_eq!(artifact_call_reads("ArtifactData", &j!({ "action": action })), Some(true), "{action}");
        }
        for action in ["set", "update", "str_replace", "delete", "batch"] {
            assert_eq!(artifact_call_reads("ArtifactData", &j!({ "action": action })), Some(false), "{action}");
        }
        assert_eq!(artifact_call_reads("ArtifactComments", &j!({"action": "read"})), Some(true));
        for action in ["reply", "resolve", "watch"] {
            assert_eq!(artifact_call_reads("ArtifactComments", &j!({ "action": action })), Some(false), "{action}");
        }
        assert_eq!(artifact_call_reads("WebFetch", &j!({})), None);
        assert!(artifact_call_publishes("Artifact", &j!({"file_path": "a.html"})) && artifact_call_publishes("Artifact", &j!({"action": "publish", "url": "u"})));
        assert!(!artifact_call_publishes("Artifact", &j!({"action": "publish", "asset": true})) && !artifact_call_publishes("Artifact", &j!({"action": "delete"})));
    }

    #[test]
    fn enabled_plugins_add_the_skill_tool_and_their_folders_only() {
        let bridge = || crate::ApprovalBridge { program: "x".into(), args: vec![] };
        let with = ToolProfile::Work { folders: vec!["C:/p".into()], permission_mode: "manual".into(), approval_bridge: bridge(), browser: false, browser_session: None, playwright: None, mcp_servers: Default::default(), plugins: vec!["C:/c/ponytail".into(), "C:/c/other".into()], artifacts: false };
        let without = ToolProfile::Work { folders: vec!["C:/p".into()], permission_mode: "manual".into(), approval_bridge: bridge(), browser: false, browser_session: None, playwright: None, mcp_servers: Default::default(), plugins: vec![], artifacts: false };
        let args = crate::claude_code::profile_arguments(&with, None, None, None);
        let dirs: Vec<&String> = args.iter().enumerate().filter(|(_, a)| *a == "--plugin-dir").map(|(i, _)| &args[i + 1]).collect();
        assert_eq!(dirs, vec!["C:/c/ponytail", "C:/c/other"]);
        let value_of = |args: &[String], flag: &str| args[args.iter().position(|a| a == flag).unwrap() + 1].clone();
        assert!(value_of(&args, "--tools").split(',').any(|t| t == "Skill"));
        assert!(value_of(&args, "--allowedTools").split(',').any(|t| t == "Skill"));
        assert!(args.contains(&"--restricted".to_string()) && args.contains(&"--strict-mcp-config".to_string()));
        let plain = crate::claude_code::profile_arguments(&without, None, None, None);
        assert!(!plain.contains(&"--plugin-dir".to_string()));
        assert!(!value_of(&plain, "--tools").split(',').any(|t| t == "Skill"));
        assert!(!value_of(&plain, "--allowedTools").split(',').any(|t| t == "Skill"));
        // The init line of CLI 2.1.289 with ponytail: the Skill tool and the plugin are fine, but only with plugins.
        let init = r#"{"type":"system","subtype":"init","tools":["Read","Bash","Skill","mcp__aurelius__approve"],"mcp_servers":[{"name":"aurelius"}],"plugins":[{"name":"ponytail","source":"ponytail@inline"}],"skills":["ponytail:ponytail"]}"#;
        assert!(StreamState::for_profile(&with).feed(init).is_ok());
        assert!(StreamState::for_profile(&without).feed(init).is_err(), "Skill without plugins fails closed");
        let other = r#"{"type":"system","subtype":"init","tools":["Bash","Skill","WebFetch2"],"mcp_servers":[]}"#;
        assert!(StreamState::for_profile(&with).feed(other).is_err(), "anything else still fails closed");
        // Never for the text-only profile.
        assert!(StreamState::for_profile(&ToolProfile::TextOnly).feed(r#"{"type":"system","subtype":"init","tools":["Read","Skill"],"mcp_servers":[]}"#).is_err());
    }

    #[test]
    fn browser_is_off_unless_asked_for() {
        let bridge = || crate::ApprovalBridge { program: "ctl".into(), args: vec![] };
        let off = ToolProfile::Work { folders: vec!["C:/p".into()], permission_mode: "manual".into(), approval_bridge: bridge(), browser: false, browser_session: None, playwright: None, mcp_servers: Default::default(), plugins: vec![], artifacts: false };
        let on = ToolProfile::Work { folders: vec!["C:/p".into()], permission_mode: "manual".into(), approval_bridge: bridge(), browser: true, browser_session: None, playwright: None, mcp_servers: Default::default(), plugins: vec![], artifacts: false };
        assert!(!crate::claude_code::profile_arguments(&off, None, None, None).contains(&"--chrome".to_string()));
        assert!(crate::claude_code::profile_arguments(&on, None, None, None).contains(&"--chrome".to_string()));
        // --strict-mcp-config stays: Chrome connects itself regardless (checked against CLI 2.1.287).
        assert!(crate::claude_code::profile_arguments(&on, None, None, None).contains(&"--strict-mcp-config".to_string()));
        // The employee's own session id keeps one tab group; only a well-formed one, and only with the browser.
        let key = "0192f1a2-3b4c-7d5e-8f60-718293a4b5c6";
        let keyed = |browser, id: &str| ToolProfile::Work { folders: vec!["C:/p".into()], permission_mode: "manual".into(), approval_bridge: bridge(), browser, browser_session: Some(id.into()), playwright: None, mcp_servers: Default::default(), plugins: vec![], artifacts: false };
        let args = crate::claude_code::profile_arguments(&keyed(true, key), None, None, None).join(" ");
        assert!(args.contains(&format!("--session-id {key}")) && args.contains("--no-session-persistence"));
        assert!(!crate::claude_code::profile_arguments(&keyed(false, key), None, None, None).contains(&"--session-id".to_string()));
        for bad in ["0192F1A2-3B4C-7D5E-8F60-718293A4B5C6", "chat:0192f1a2-3b4c-7d5e-8f60-718293a4b5c6", "--resume", ""] {
            assert!(!crate::claude_code::profile_arguments(&keyed(true, bad), None, None, None).contains(&"--session-id".to_string()), "{bad}");
        }
        let init = r#"{"type":"system","subtype":"init","tools":["Bash","mcp__aurelius__approve","mcp__claude-in-chrome__navigate","mcp__claude-in-chrome__computer"],"mcp_servers":[{"name":"aurelius"},{"name":"claude-in-chrome"}]}"#;
        assert!(StreamState::for_profile(&on).feed(init).is_ok(), "chrome tools and server are allowed when asked for");
        assert!(StreamState::for_profile(&off).feed(init).is_err(), "chrome showing up without being asked for fails the run");
        let other = r#"{"type":"system","subtype":"init","tools":["mcp__claude-in-chrome__navigate","mcp__other__x"],"mcp_servers":[{"name":"claude-in-chrome"}]}"#;
        assert!(StreamState::for_profile(&on).feed(other).is_err(), "only Chrome's own tools are added");
    }

    #[test]
    fn playwright_replaces_chrome_as_the_browser() {
        let bridge = || crate::ApprovalBridge { program: "ctl".into(), args: vec![] };
        let config = serde_json::json!({ "type": "stdio", "command": "node", "args": ["cli.js", "--cdp-endpoint", "http://127.0.0.1:9"] });
        let profile = |browser, user: Option<&str>| ToolProfile::Work {
            folders: vec!["C:/p".into()], permission_mode: "manual".into(), approval_bridge: bridge(), browser,
            browser_session: Some("0192f1a2-3b4c-7d5e-8f60-718293a4b5c6".into()), playwright: Some(config.clone()),
            mcp_servers: user.map(|n| [(n.to_string(), serde_json::json!({ "command": "evil" }))].into_iter().collect()).unwrap_or_default(),
            plugins: vec![], artifacts: false,
        };
        let on = crate::claude_code::profile_arguments(&profile(true, Some(crate::claude_code::PLAYWRIGHT_SERVER)), None, None, None);
        assert!(!on.contains(&"--chrome".to_string()) && !on.contains(&"--session-id".to_string()), "no Claude in Chrome next to it");
        let mcp: serde_json::Value = serde_json::from_str(&on[on.iter().position(|a| a == "--mcp-config").unwrap() + 1]).unwrap();
        assert_eq!(mcp["mcpServers"][crate::claude_code::PLAYWRIGHT_SERVER], config, "Aurelius' config, never a user server of that name");
        let off = crate::claude_code::profile_arguments(&profile(false, None), None, None, None);
        let mcp: serde_json::Value = serde_json::from_str(&off[off.iter().position(|a| a == "--mcp-config").unwrap() + 1]).unwrap();
        assert!(mcp["mcpServers"].get(crate::claude_code::PLAYWRIGHT_SERVER).is_none(), "only when the user allowed the browser");
        let init = r#"{"type":"system","subtype":"init","tools":["Bash","mcp__aurelius__approve","mcp__aurelius-browser__browser_navigate"],"mcp_servers":[{"name":"aurelius"},{"name":"aurelius-browser"}]}"#;
        assert!(StreamState::for_profile(&profile(true, None)).feed(init).is_ok());
        assert!(StreamState::for_profile(&profile(false, None)).feed(init).is_err());
        let chrome = r#"{"type":"system","subtype":"init","tools":["mcp__claude-in-chrome__navigate"],"mcp_servers":[{"name":"claude-in-chrome"}]}"#;
        assert!(StreamState::for_profile(&profile(true, None)).feed(chrome).is_err(), "Chrome is not allowed next to Playwright");
        assert!(crate::claude_code::is_browser_tool("mcp__aurelius-browser__browser_click") && crate::claude_code::is_browser_tool("mcp__claude-in-chrome__navigate"));
        assert!(!crate::claude_code::is_browser_tool("mcp__playwright__browser_click"));
    }

    #[test]
    fn mcp_tools_are_looked_up_with_tool_search() {
        // Without ToolSearch in --tools, Claude Code sends every MCP tool definition upfront (CLI 2.1.301).
        let bridge = crate::ApprovalBridge { program: "ctl".into(), args: vec![] };
        let p = ToolProfile::Work { folders: vec!["C:/p".into()], permission_mode: "manual".into(), approval_bridge: bridge, browser: false, browser_session: None, playwright: None, mcp_servers: Default::default(), plugins: vec![], artifacts: false };
        let args = crate::claude_code::profile_arguments(&p, None, None, None);
        let value = |flag: &str| args[args.iter().position(|a| a == flag).unwrap() + 1].clone();
        assert!(value("--tools").split(',').any(|t| t == "ToolSearch"));
        assert!(value("--allowedTools").split(',').any(|t| t == "ToolSearch"), "it only reads: never asked");
        let init = r#"{"type":"system","subtype":"init","tools":["Bash","ToolSearch","mcp__aurelius__approve"],"mcp_servers":[{"name":"aurelius"}]}"#;
        assert!(StreamState::for_profile(&p).feed(init).is_ok());
        let text_only = crate::claude_code::profile_arguments(&ToolProfile::TextOnly, None, None, None);
        assert!(!text_only.join(" ").contains("ToolSearch"));
    }

    #[test]
    fn screen_tools_are_never_asked_by_claude_code() {
        // The bridge decides (employee's Screen on, or the inbox); Claude Code must not ask too, nor wave them through.
        let bridge = || crate::ApprovalBridge { program: "ctl".into(), args: vec![] };
        for (mode, browser) in [("manual", false), ("auto", false), ("auto", true), ("plan", false)] {
            let p = ToolProfile::Work { folders: vec!["C:/p".into()], permission_mode: mode.into(), approval_bridge: bridge(), browser, browser_session: None, playwright: None, mcp_servers: Default::default(), plugins: vec![], artifacts: false };
            let args = crate::claude_code::profile_arguments(&p, None, None, None);
            assert_eq!(args.iter().filter(|a| *a == "--allowedTools").count(), 1, "{mode}: one allow list");
            let i = args.iter().position(|a| a == "--allowedTools").unwrap();
            assert!(args[i + 1].contains("mcp__aurelius__screenshot") && args[i + 1].contains("mcp__aurelius__list_windows"));
            assert!(!args[i + 1].contains("approve"));
            // Auto is decided per call by the bridge: an allow rule could not be taken back mid-answer.
            assert!(!args[i + 1].split(',').any(|t| matches!(t, "Edit" | "Write" | "Bash" | "WebFetch")), "{mode}: no work tools by rule");
            assert!(!args.contains(&"auto".to_string()), "{mode}: never Claude Code's own auto");
        }
        let p = ToolProfile::Work { folders: vec!["C:/p".into()], permission_mode: "manual".into(), approval_bridge: bridge(), browser: false, browser_session: None, playwright: None, mcp_servers: Default::default(), plugins: vec![], artifacts: false };
        let init = r#"{"type":"system","subtype":"init","tools":["Bash","mcp__aurelius__approve","mcp__aurelius__screenshot","mcp__aurelius__list_windows"],"mcp_servers":[{"name":"aurelius"}]}"#;
        assert!(StreamState::for_profile(&p).feed(init).is_ok(), "the bridge's screen tools belong to the work profile");
        assert!(StreamState::for_profile(&ToolProfile::TextOnly).feed(r#"{"type":"system","subtype":"init","tools":["mcp__aurelius__screenshot"],"mcp_servers":[]}"#).is_err(), "never outside work");
    }

    #[test]
    fn text_blocks_after_a_tool_call_start_a_new_paragraph() {
        let mut s = StreamState::default();
        let block = |i: u32, kind: &str| format!(r#"{{"type":"stream_event","event":{{"type":"content_block_start","index":{i},"content_block":{{"type":"{kind}"}}}}}}"#);
        let delta = |i: u32, t: &str| format!(r#"{{"type":"stream_event","event":{{"type":"content_block_delta","index":{i},"delta":{{"type":"text_delta","text":"{t}"}}}}}}"#);
        assert!(s.feed(&block(0, "text")).unwrap().is_empty(), "nothing before the first text");
        s.feed(&delta(0, "I'll do it.")).unwrap();
        s.feed(&block(1, "tool_use")).unwrap();
        assert_eq!(s.feed(&block(0, "text")).unwrap(), vec![RuntimeEvent::TextDelta("\n\n".into())]);
        s.feed(&delta(0, "## Result")).unwrap();
        assert_eq!(s.streamed, "I'll do it.\n\n## Result");
    }

    #[test]
    fn tool_use_and_result_become_activity() {
        let mut s = StreamState::for_profile(&work());
        let call = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Searching."},{"type":"tool_use","id":"t1","name":"WebSearch","input":{"query":"rust gpui"}}]}}"#;
        let events = s.feed(call).unwrap();
        assert!(events.contains(&RuntimeEvent::Activity(Activity::Started { call_id: "t1".into(), tool: "WebSearch".into(), input: serde_json::json!({"query": "rust gpui"}), parent: None })));
        let result = r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":[{"type":"text","text":"3 results"}]}]}}"#;
        assert_eq!(s.feed(result).unwrap(), vec![RuntimeEvent::Activity(Activity::Finished { call_id: "t1".into(), ok: true, output: "3 results".into() })]);
    }

    #[test]
    fn a_subagents_tool_use_belongs_to_the_call_that_started_it() {
        // As CLI 2.1.287 streams it: the subagent's messages carry parent_tool_use_id.
        let mut s = StreamState::for_profile(&work());
        let step = r#"{"type":"assistant","parent_tool_use_id":"toolu_agent","message":{"content":[{"type":"text","text":"Looking."},{"type":"tool_use","id":"t2","name":"Read","input":{"file_path":"a.txt"}}]}}"#;
        assert_eq!(
            s.feed(step).unwrap(),
            vec![RuntimeEvent::Activity(Activity::Started { call_id: "t2".into(), tool: "Read".into(), input: serde_json::json!({"file_path": "a.txt"}), parent: Some("toolu_agent".into()) })]
        );
        assert!(s.streamed.is_empty(), "a subagent's text is not part of the answer");
    }
}
