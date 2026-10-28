//! MCP over Streamable HTTP, shared by the remote gateway (`POST /v1/mcp`, an
//! in-process daemon session per request) and `aurelius-mcp --http`
//! (`POST /mcp`, a WebSocket to the gateway per app key). The caller sends
//! `Authorization: Bearer amk_…` and a JSON-RPC message (or a batch) and gets
//! JSON back. No server-sent events and no MCP session state: the key goes to
//! Aurelius, which checks scopes, limits and revocation itself.
//!
//! Requests from a web page (an `Origin` header) are refused: only programs
//! call this, never a browser (DNS rebinding).

use crate::Server;
use aurelius_domain::app_key::KEY_PREFIX;
use axum::Json;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};
use std::sync::Arc;

/// Why no MCP server could be opened for a key.
pub enum Refused {
    /// Revoked, unknown or not an app key: 401.
    Unauthorized(String),
    /// Aurelius cannot be reached or is busy: 503.
    Unavailable(String),
}

/// Opens the MCP server of the app behind one app key.
pub trait Sessions: Send + Sync {
    fn open(&self, key: &str) -> impl Future<Output = Result<Arc<Server>, Refused>> + Send;
}

pub fn error(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

fn unauthorized(message: &str) -> Response {
    let mut response = error(StatusCode::UNAUTHORIZED, message);
    response.headers_mut().insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
    response
}

fn bearer(headers: &HeaderMap) -> Option<String> {
    headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("Bearer ")).map(|t| t.trim().to_owned())
}

/// `GET` (a server-sent stream) and `DELETE` (ending a session) are not offered.
pub async fn not_here() -> Response {
    let mut response = (StatusCode::METHOD_NOT_ALLOWED, "Use POST.").into_response();
    response.headers_mut().insert(header::ALLOW, HeaderValue::from_static("POST"));
    response
}

/// One `POST`. `fallback_key` serves callers that send no `Authorization` header.
pub async fn post(sessions: &impl Sessions, fallback_key: Option<&str>, headers: &HeaderMap, body: &[u8]) -> Response {
    if headers.contains_key(header::ORIGIN) {
        return error(StatusCode::FORBIDDEN, "Not for browsers.");
    }
    let sent = bearer(headers);
    let Some(key) = sent.as_deref().or(fallback_key).filter(|t| t.starts_with(KEY_PREFIX)) else {
        return unauthorized("Send the app key as Authorization: Bearer amk_…");
    };
    let server = match sessions.open(key).await {
        Ok(server) => server,
        Err(Refused::Unauthorized(message)) => return unauthorized(&message),
        Err(Refused::Unavailable(message)) => return error(StatusCode::SERVICE_UNAVAILABLE, &message),
    };
    let message: Value = match serde_json::from_slice(body) {
        Ok(message) => message,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(json!({ "jsonrpc": "2.0", "id": null, "error": { "code": -32700, "message": "Parse error" } }))).into_response(),
    };
    match server.handle(message).await {
        Some(reply) => Json(reply).into_response(),
        // Only notifications or responses: accepted, nothing to answer.
        None => StatusCode::ACCEPTED.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Knows no key; remembers which keys it was asked for.
    #[derive(Default)]
    struct NoKeys(Mutex<Vec<String>>);

    impl Sessions for NoKeys {
        async fn open(&self, key: &str) -> Result<Arc<Server>, Refused> {
            self.0.lock().unwrap().push(key.to_owned());
            Err(Refused::Unauthorized("This app key was revoked or does not exist.".into()))
        }
    }

    fn headers(pairs: &[(header::HeaderName, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.insert(name.clone(), HeaderValue::from_str(value).unwrap());
        }
        map
    }

    const LIST: &[u8] = br#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#;

    #[tokio::test]
    async fn no_key_is_401() {
        let sessions = NoKeys::default();
        let response = post(&sessions, None, &HeaderMap::new(), LIST).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(response.headers()[header::WWW_AUTHENTICATE], "Bearer");
        assert!(sessions.0.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn other_tokens_are_401_without_asking() {
        let sessions = NoKeys::default();
        let response = post(&sessions, Some("amk_fallback"), &headers(&[(header::AUTHORIZATION, "Bearer ak_function")]), LIST).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert!(sessions.0.lock().unwrap().is_empty(), "a sent header wins over the fallback key");
    }

    #[tokio::test]
    async fn the_sent_key_goes_through_and_its_refusal_is_401() {
        let sessions = NoKeys::default();
        let response = post(&sessions, Some("amk_fallback"), &headers(&[(header::AUTHORIZATION, "Bearer amk_client")]), LIST).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(*sessions.0.lock().unwrap(), ["amk_client"]);
    }

    #[tokio::test]
    async fn the_fallback_key_serves_callers_without_a_header() {
        let sessions = NoKeys::default();
        post(&sessions, Some("amk_fallback"), &HeaderMap::new(), LIST).await;
        assert_eq!(*sessions.0.lock().unwrap(), ["amk_fallback"]);
    }

    #[tokio::test]
    async fn browsers_are_refused() {
        let sessions = NoKeys::default();
        let response = post(&sessions, None, &headers(&[(header::AUTHORIZATION, "Bearer amk_client"), (header::ORIGIN, "https://evil.example")]), LIST).await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert!(sessions.0.lock().unwrap().is_empty());
    }
}
