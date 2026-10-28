//! OpenAI Responses API (POST /v1/responses, Bearer key).

use super::{alternating, capabilities, client, map_status, spawn_stream, sse, transport};
use crate::*;
use serde_json::{Value, json};

pub const BASE: &str = "https://api.openai.com";
/// Flagship general model (verified 2026-10-01).
pub const DEFAULT_MODEL: &str = "gpt-6-astra";

pub struct OpenAi {
    key: String,
    model: Option<String>,
    http: reqwest::Client,
}

impl OpenAi {
    pub fn new(key: String, model: Option<String>) -> Self {
        Self { key, model, http: client() }
    }
}

pub fn body(model: &str, request: &RunRequest) -> Value {
    let input: Vec<Value> = alternating(&request.turns)
        .iter()
        .map(|t| json!({ "role": if t.role == TurnRole::User { "user" } else { "assistant" }, "content": content(t) }))
        .collect();
    let mut body = json!({ "model": model, "input": input, "stream": true });
    if let Some(system) = request.system.as_deref().filter(|s| !s.is_empty()) {
        body["instructions"] = json!(system);
    }
    body
}

/// A turn's content: plain text, or its images (as data URLs) and then the text.
fn content(turn: &Turn) -> Value {
    if turn.images.is_empty() {
        return json!(turn.text);
    }
    let mut parts: Vec<Value> = turn
        .images
        .iter()
        .map(|i| json!({ "type": "input_image", "image_url": format!("data:{};base64,{}", i.media_type, i.base64()) }))
        .collect();
    if !turn.text.trim().is_empty() {
        parts.push(json!({ "type": "input_text", "text": turn.text }));
    }
    json!(parts)
}

pub fn normalize(event: &sse::Event) -> Vec<RuntimeEvent> {
    let Ok(data) = serde_json::from_str::<Value>(&event.data) else { return vec![] };
    let kind = data["type"].as_str().or(event.event.as_deref()).unwrap_or("");
    match kind {
        "response.output_text.delta" => data["delta"].as_str().map(|t| vec![RuntimeEvent::TextDelta(t.to_owned())]).unwrap_or_default(),
        "response.completed" => {
            let u = &data["response"]["usage"];
            let mut events = Vec::new();
            if u.is_object() {
                events.push(RuntimeEvent::Usage(crate::Usage {
                    input_tokens: u["input_tokens"].as_u64().unwrap_or(0),
                    output_tokens: u["output_tokens"].as_u64().unwrap_or(0),
                    cached_tokens: u["input_tokens_details"]["cached_tokens"].as_u64().unwrap_or(0),
                    ..Default::default()
                }));
            }
            events.push(RuntimeEvent::Completed);
            events
        }
        "response.failed" | "response.incomplete" => vec![RuntimeEvent::Failed {
            code: kind.trim_start_matches("response.").to_owned(),
            message: data["response"]["error"]["message"]
                .as_str()
                .or(data["response"]["incomplete_details"]["reason"].as_str())
                .unwrap_or("The answer is incomplete.")
                .to_owned(),
        }],
        "error" => vec![RuntimeEvent::Failed {
            code: data["code"].as_str().unwrap_or("error").to_owned(),
            message: data["message"].as_str().unwrap_or("OpenAI API error").to_owned(),
        }],
        _ => vec![],
    }
}

#[async_trait::async_trait]
impl AgentRuntime for OpenAi {
    async fn probe(&self) -> RuntimeCapabilities {
        capabilities("openai_api")
    }

    async fn start(&self, request: RunRequest) -> Result<RunHandle, RuntimeError> {
        let model = self.model.clone().filter(|m| !m.is_empty() && !m.starts_with("fake")).unwrap_or_else(|| DEFAULT_MODEL.into());
        let req = self.http.post(format!("{BASE}/v1/responses")).bearer_auth(&self.key).json(&body(&model, &request));
        Ok(spawn_stream(request.run_id, model, req, normalize))
    }
}

pub async fn list_models(key: &str) -> Result<Vec<String>, RuntimeError> {
    let response = client().get(format!("{BASE}/v1/models")).bearer_auth(key).send().await.map_err(transport)?;
    let status = response.status().as_u16();
    if status >= 400 {
        return Err(map_status(status, None, &response.text().await.unwrap_or_default()));
    }
    let json: Value = response.json().await.map_err(transport)?;
    let mut models: Vec<String> = json["data"].as_array().map(|a| a.iter().filter_map(|m| m["id"].as_str().map(str::to_owned)).collect()).unwrap_or_default();
    models.sort();
    Ok(models)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(data: &str) -> sse::Event {
        sse::Event { event: None, data: data.into() }
    }

    #[test]
    fn stream_events_are_normalized() {
        assert_eq!(normalize(&ev(r#"{"type":"response.output_text.delta","delta":"Hoi","item_id":"x"}"#)), vec![RuntimeEvent::TextDelta("Hoi".into())]);
        assert!(normalize(&ev(r#"{"type":"response.created"}"#)).is_empty());
        assert_eq!(normalize(&ev(r#"{"type":"response.completed","response":{}}"#)), vec![RuntimeEvent::Completed]);
        assert!(matches!(&normalize(&ev(r#"{"type":"response.failed","response":{"error":{"message":"x"}}}"#))[..], [RuntimeEvent::Failed { .. }]));
        assert!(matches!(&normalize(&ev(r#"{"type":"error","code":"rate_limit_exceeded","message":"slow"}"#))[..], [RuntimeEvent::Failed { code, .. }] if code == "rate_limit_exceeded"));
    }

    #[test]
    fn images_are_sent_as_data_urls_before_the_text() {
        let image = ImagePart { name: "shot.jpg".into(), media_type: "image/jpeg".into(), data: vec![1, 2, 3] };
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
        let content = &body("gpt", &request)["input"][0]["content"];
        assert_eq!(content[0]["type"], "input_image");
        assert_eq!(content[0]["image_url"], "data:image/jpeg;base64,AQID");
        assert_eq!(content[1]["type"], "input_text");
    }
}
