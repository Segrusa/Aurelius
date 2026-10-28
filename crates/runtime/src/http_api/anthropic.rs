//! Anthropic Messages API (POST /v1/messages, `anthropic-version: 2023-06-01`).

use super::{alternating, capabilities, client, map_status, spawn_stream, sse, transport};
use crate::*;
use serde_json::{Value, json};

pub const BASE: &str = "https://api.anthropic.com";
pub const VERSION: &str = "2023-06-01";
/// The docs' recommended starting point (verified 2026-10-01).
pub const DEFAULT_MODEL: &str = "claude-opus-5-5";
/// Verified current IDs; used if the models endpoint is unavailable.
pub const KNOWN_MODELS: &[&str] = &["claude-opus-5-5", "claude-sonnet-5-5", "claude-fable-5-1", "claude-haiku-4-5"];
pub const MAX_TOKENS: u32 = 8_192;

pub struct Anthropic {
    key: String,
    model: Option<String>,
    http: reqwest::Client,
}

impl Anthropic {
    pub fn new(key: String, model: Option<String>) -> Self {
        Self { key, model, http: client() }
    }
}

/// Request body; only fields documented for the Messages API.
pub fn body(model: &str, request: &RunRequest) -> Value {
    let messages: Vec<Value> = alternating(&request.turns)
        .iter()
        .map(|t| json!({ "role": if t.role == TurnRole::User { "user" } else { "assistant" }, "content": content(t) }))
        .collect();
    let mut body = json!({ "model": model, "max_tokens": MAX_TOKENS, "messages": messages, "stream": true });
    if let Some(system) = request.system.as_deref().filter(|s| !s.is_empty()) {
        body["system"] = json!(system);
    }
    body
}

/// A turn's content: plain text, or its images (first, as the docs advise) and then the text.
fn content(turn: &Turn) -> Value {
    if turn.images.is_empty() {
        return json!(turn.text);
    }
    let mut blocks: Vec<Value> = turn
        .images
        .iter()
        .map(|i| json!({ "type": "image", "source": { "type": "base64", "media_type": i.media_type, "data": i.base64() } }))
        .collect();
    if !turn.text.trim().is_empty() {
        blocks.push(json!({ "type": "text", "text": turn.text }));
    }
    json!(blocks)
}

/// Normalize one SSE event. Unknown events (e.g. `ping`, future types) are ignored.
pub fn normalize(event: &sse::Event) -> Vec<RuntimeEvent> {
    let Ok(data) = serde_json::from_str::<Value>(&event.data) else { return vec![] };
    match data["type"].as_str().unwrap_or("") {
        "content_block_delta" if data["delta"]["type"] == "text_delta" => {
            data["delta"]["text"].as_str().map(|t| vec![RuntimeEvent::TextDelta(t.to_owned())]).unwrap_or_default()
        }
        "message_start" => {
            let u = &data["message"]["usage"];
            let n = |k: &str| u[k].as_u64().unwrap_or(0);
            vec![RuntimeEvent::Usage(crate::Usage {
                input_tokens: n("input_tokens") + n("cache_creation_input_tokens"),
                cached_tokens: n("cache_read_input_tokens"),
                cache_creation_tokens: n("cache_creation_input_tokens"),
                ..Default::default()
            })]
        }
        "message_delta" if data["usage"]["output_tokens"].is_u64() => vec![RuntimeEvent::Usage(crate::Usage {
            output_tokens: data["usage"]["output_tokens"].as_u64().unwrap_or(0),
            ..Default::default()
        })],
        "message_stop" => vec![RuntimeEvent::Completed],
        "error" => vec![RuntimeEvent::Failed {
            code: data["error"]["type"].as_str().unwrap_or("error").to_owned(),
            message: data["error"]["message"].as_str().unwrap_or("Anthropic API error").to_owned(),
        }],
        _ => vec![],
    }
}

#[async_trait::async_trait]
impl AgentRuntime for Anthropic {
    async fn probe(&self) -> RuntimeCapabilities {
        capabilities("anthropic_api")
    }

    async fn start(&self, request: RunRequest) -> Result<RunHandle, RuntimeError> {
        let model = self.model.clone().filter(|m| !m.is_empty() && !m.starts_with("fake")).unwrap_or_else(|| DEFAULT_MODEL.into());
        let req = self
            .http
            .post(format!("{BASE}/v1/messages"))
            .header("x-api-key", &self.key)
            .header("anthropic-version", VERSION)
            .json(&body(&model, &request));
        Ok(spawn_stream(request.run_id, model, req, normalize))
    }
}

pub async fn list_models(key: &str) -> Result<Vec<String>, RuntimeError> {
    let response = client()
        .get(format!("{BASE}/v1/models"))
        .header("x-api-key", key)
        .header("anthropic-version", VERSION)
        .send()
        .await
        .map_err(transport)?;
    let status = response.status().as_u16();
    if status == 404 {
        return Ok(KNOWN_MODELS.iter().map(|s| s.to_string()).collect());
    }
    if status >= 400 {
        return Err(map_status(status, None, &response.text().await.unwrap_or_default()));
    }
    let json: Value = response.json().await.map_err(transport)?;
    let models: Vec<String> = json["data"].as_array().map(|a| a.iter().filter_map(|m| m["id"].as_str().map(str::to_owned)).collect()).unwrap_or_default();
    Ok(if models.is_empty() { KNOWN_MODELS.iter().map(|s| s.to_string()).collect() } else { models })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(data: &str) -> sse::Event {
        sse::Event { event: None, data: data.into() }
    }

    #[test]
    fn stream_events_are_normalized() {
        assert_eq!(normalize(&ev(r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hoi"}}"#)), vec![RuntimeEvent::TextDelta("Hoi".into())]);
        assert!(normalize(&ev(r#"{"type":"ping"}"#)).is_empty());
        assert!(normalize(&ev(r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"}}"#)).is_empty());
        assert_eq!(normalize(&ev(r#"{"type":"message_stop"}"#)), vec![RuntimeEvent::Completed]);
        let err = normalize(&ev(r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#));
        assert_eq!(err, vec![RuntimeEvent::Failed { code: "overloaded_error".into(), message: "Overloaded".into() }]);
    }

    #[test]
    fn body_has_system_and_alternating_messages() {
        let request = RunRequest {
            run_id: aurelius_domain::RunId::new(),
            model_id: String::new(),
            system: Some("Je bent Alex.".into()),
            turns: vec![Turn { role: TurnRole::User, text: "Hoi".into(), images: Vec::new() }],
            effort: None,
            profile: Default::default(),
            subagent_model: None,
            session: Default::default(),
            session_id: None,
            keep_session: false,
        };
        let b = body("claude-opus-5-5", &request);
        assert_eq!(b["system"], "Je bent Alex.");
        assert_eq!(b["stream"], true);
        assert_eq!(b["messages"][0]["role"], "user");
        assert_eq!(b["messages"][0]["content"], "Hoi");
    }

    #[test]
    fn images_go_before_the_text_as_base64_blocks() {
        let image = ImagePart { name: "shot.png".into(), media_type: "image/png".into(), data: vec![1, 2, 3] };
        let request = RunRequest {
            run_id: aurelius_domain::RunId::new(),
            model_id: String::new(),
            system: None,
            turns: vec![Turn { role: TurnRole::User, text: "Wat zie je?".into(), images: vec![image] }],
            effort: None,
            profile: Default::default(),
            subagent_model: None,
            session: Default::default(),
            session_id: None,
            keep_session: false,
        };
        let content = &body("claude-opus-5-5", &request)["messages"][0]["content"];
        assert_eq!(content[0]["type"], "image");
        assert_eq!(content[0]["source"]["media_type"], "image/png");
        assert_eq!(content[0]["source"]["data"], "AQID");
        assert_eq!(content[1]["text"], "Wat zie je?");
    }
}
