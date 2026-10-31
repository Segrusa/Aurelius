//! App-owned API adapters.
//!
//! Each provider has its own request serializer, SSE parser and error mapping;
//! nothing provider-specific leaks into the domain. A run only counts as
//! success on the provider's terminal success event; an error after deltas
//! is a failure. Costs are not estimated here (unknown stays unknown).
//!
//! Endpoints and event names were verified against the official docs on
//! 2026-10-01.

pub mod anthropic;
pub mod openai;
pub mod sse;

use aurelius_domain::connection::ProviderKind;
use crate::*;
use std::sync::Arc;
use std::time::Duration;

pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// Idle limit between stream chunks.
pub const READ_TIMEOUT: Duration = Duration::from_secs(120);

pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .read_timeout(READ_TIMEOUT)
        .user_agent(concat!("Aurelius/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("http client")
}

pub fn for_provider(provider: ProviderKind, api_key: String, model: Option<String>) -> Arc<dyn AgentRuntime> {
    match provider {
        ProviderKind::OpenaiApi => Arc::new(openai::OpenAi::new(api_key, model)),
        _ => Arc::new(anthropic::Anthropic::new(api_key, model)),
    }
}

pub async fn list_models(provider: ProviderKind, api_key: &str) -> Result<Vec<String>, RuntimeError> {
    match provider {
        ProviderKind::OpenaiApi => openai::list_models(api_key).await,
        _ => anthropic::list_models(api_key).await,
    }
}

/// Map an HTTP error status to a scheduler-relevant error.
pub fn map_status(status: u16, retry_after_s: Option<i64>, body: &str) -> RuntimeError {
    let detail: String = body.chars().take(300).collect();
    match status {
        401 | 403 => RuntimeError::Auth(format!("The provider rejected the API key ({status}). {detail}")),
        402 => RuntimeError::Quota { reset_at_ms: None },
        429 => RuntimeError::Quota {
            reset_at_ms: retry_after_s.map(|s| now_ms() + s * 1000),
        },
        408 | 409 | 500..=599 => RuntimeError::Unavailable(format!("Provider temporarily unavailable ({status}). {detail}")),
        _ => RuntimeError::Permanent(format!("Request refused ({status}). {detail}")),
    }
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

pub fn transport(e: reqwest::Error) -> RuntimeError {
    RuntimeError::Unavailable(format!("Netwerkfout: {e}"))
}

/// Merge consecutive turns of the same role (APIs require alternation).
pub fn alternating(turns: &[Turn]) -> Vec<Turn> {
    let mut out: Vec<Turn> = Vec::new();
    for turn in turns {
        match out.last_mut() {
            Some(last) if last.role == turn.role => {
                last.text.push_str("\n\n");
                last.text.push_str(&turn.text);
                last.images.extend(turn.images.iter().cloned());
            }
            _ => out.push(turn.clone()),
        }
    }
    // A conversation must start with the user.
    while out.first().is_some_and(|t| t.role != TurnRole::User) {
        out.remove(0);
    }
    out
}

/// Spawn a streaming request; the returned handle streams normalized events.
pub(crate) fn spawn_stream(
    run_id: aurelius_domain::RunId,
    model: String,
    request: reqwest::RequestBuilder,
    mut on_event: impl FnMut(&sse::Event) -> Vec<RuntimeEvent> + Send + 'static,
) -> RunHandle {
    let (tx, rx) = tokio::sync::mpsc::channel(EVENT_BUFFER);
    let (input_tx, _input_rx) = tokio::sync::mpsc::channel(1);
    let (cancel_tx, mut cancel_rx) = tokio::sync::watch::channel(false);
    tokio::spawn(async move {
        let work = async {
            let response = match request.send().await {
                Ok(r) => r,
                Err(e) => {
                    let _ = tx.send(RuntimeEvent::Failed { code: "network".into(), message: transport(e).to_string() }).await;
                    return;
                }
            };
            let status = response.status().as_u16();
            if status >= 400 {
                let retry = response.headers().get("retry-after").and_then(|v| v.to_str().ok()).and_then(|v| v.parse().ok());
                let body = response.text().await.unwrap_or_default();
                let err = map_status(status, retry, &body);
                let code = match &err {
                    RuntimeError::Auth(_) | RuntimeError::Login(_) => "authentication_failed",
                    RuntimeError::Quota { .. } => "rate_limit",
                    RuntimeError::Unavailable(_) => "unavailable",
                    RuntimeError::Permanent(_) => "invalid_request",
                };
                let _ = tx.send(RuntimeEvent::Failed { code: code.into(), message: err.to_string() }).await;
                return;
            }
            let _ = tx.send(RuntimeEvent::Started).await;
            let _ = tx.send(RuntimeEvent::ModelReported(model)).await;
            let mut response = response;
            let mut parser = sse::Parser::default();
            loop {
                match response.chunk().await {
                    Ok(Some(bytes)) => {
                        for event in parser.push(&bytes) {
                            for normalized in on_event(&event) {
                                let terminal = matches!(normalized, RuntimeEvent::Completed | RuntimeEvent::Failed { .. });
                                if tx.send(normalized).await.is_err() || terminal {
                                    return;
                                }
                            }
                        }
                    }
                    Ok(None) => {
                        // Stream ended without terminal success: never success.
                        let _ = tx.send(RuntimeEvent::Failed { code: "stream_incomplete".into(), message: "The connection closed before the answer was complete.".into() }).await;
                        return;
                    }
                    Err(e) => {
                        let _ = tx.send(RuntimeEvent::Failed { code: "network".into(), message: transport(e).to_string() }).await;
                        return;
                    }
                }
            }
        };
        tokio::select! {
            _ = work => {}
            _ = cancel_rx.changed() => {
                // Dropping the request future aborts the HTTP stream.
                let _ = tx.send(RuntimeEvent::Interrupted).await;
            }
        }
    });
    RunHandle::new(run_id, rx, input_tx, cancel_tx)
}

pub(crate) fn capabilities(kind: &str) -> RuntimeCapabilities {
    RuntimeCapabilities {
        runtime_kind: kind.into(),
        version: "api".into(),
        streaming: Capability::Supported,
        cancel: Capability::Supported,
        resume: Capability::Unsupported,
        // The HTTP API adapters are text-only: no tool calls are sent or handled.
        tools: Capability::Unsupported,
        native_sandbox: Capability::Unsupported,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_mapping_separates_auth_quota_transient_permanent() {
        assert!(matches!(map_status(401, None, ""), RuntimeError::Auth(_)));
        assert!(matches!(map_status(429, Some(30), ""), RuntimeError::Quota { reset_at_ms: Some(_) }));
        assert!(matches!(map_status(429, None, ""), RuntimeError::Quota { reset_at_ms: None }));
        assert!(matches!(map_status(529, None, ""), RuntimeError::Unavailable(_)));
        assert!(matches!(map_status(400, None, ""), RuntimeError::Permanent(_)));
    }

    #[test]
    fn turns_alternate_and_start_with_user() {
        let t = |role, text: &str| Turn { role, text: text.into(), images: Vec::new() };
        let out = alternating(&[t(TurnRole::Assistant, "x"), t(TurnRole::User, "a"), t(TurnRole::User, "b"), t(TurnRole::Assistant, "c")]);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].text, "a\n\nb");
    }
}
