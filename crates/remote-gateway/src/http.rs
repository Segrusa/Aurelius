//! HTTP surface of the gateway. Tokens travel only in `Authorization`
//! headers, never in URLs, and are never logged.

use crate::auth::{self, ClaimError, PairState};
use crate::{Hub, MAX_UPLOAD_BYTES, SESSION_MS};
use aurelius_domain::agent_function::{CallError, CallState, Remaining};
use aurelius_storage::remote::RemoteClient;
use axum::extract::{DefaultBodyLimit, Multipart, Path, State, WebSocketUpgrade};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use base64::Engine;
use serde::Deserialize;
use serde_json::json;
use std::time::Duration;

pub fn router(hub: Hub) -> Router {
    Router::new()
        .route("/v1/health", get(health))
        .route("/v1/pair", post(pair))
        .route("/v1/pair/{id}", get(pair_poll))
        .route("/v1/session", post(session))
        .route("/v1/ws", get(websocket))
        .route("/v1/upload", post(upload).layer(DefaultBodyLimit::max(MAX_UPLOAD_BYTES + 64 * 1024)))
        .route("/v1/push", put(push_register).delete(push_unregister))
        // MCP over Streamable HTTP for outside apps with an app key (`amk_…`).
        .route("/v1/mcp", post(crate::mcp::post).get(crate::mcp::not_here).delete(crate::mcp::not_here))
        // Apps with an API key reach only these two routes (never ws, upload or push).
        .route("/v1/functions/{slug}/calls", post(function_call))
        .route("/v1/calls/{call_id}", get(function_call_state))
        .layer(DefaultBodyLimit::max(64 * 1024))
        .with_state(hub)
}

pub(crate) fn error(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

async fn health(State(hub): State<Hub>) -> Response {
    Json(json!({
        "host_id": hub.inner.host_id,
        "version": hub.inner.version,
        "environment": hub.inner.environment,
        "protocol": aurelius_protocol::PROTOCOL_MAJOR,
    }))
    .into_response()
}

#[derive(Deserialize)]
struct PairBody {
    code: String,
    device_name: String,
    platform: String,
    /// `owner`: a computer asking for full access.
    #[serde(default)]
    role: Option<String>,
}

/// Phones pair as devices; computers may ask to be an owner.
const PHONES: [&str; 2] = ["ios", "android"];
const COMPUTERS: [&str; 3] = ["windows", "macos", "linux"];

async fn pair(State(hub): State<Hub>, Json(body): Json<PairBody>) -> Response {
    let platform = body.platform.as_str();
    let computer = COMPUTERS.contains(&platform);
    if !computer && !PHONES.contains(&platform) {
        return error(StatusCode::BAD_REQUEST, "Unknown platform.");
    }
    let owner = match body.role.as_deref() {
        None | Some("device") => false,
        Some("owner") if computer => true,
        Some(_) => return error(StatusCode::BAD_REQUEST, "Only a computer can ask for full access."),
    };
    let name = body.device_name.trim();
    let name = if name.is_empty() { if computer { "Computer" } else { "Phone" } } else { name };
    let claimed = hub.inner.pairings.lock().unwrap().claim(&body.code, name, platform, owner);
    match claimed {
        Ok(request) => {
            tracing::info!(device = %request.device_name, "pairing requested; waiting for the desktop");
            (StatusCode::ACCEPTED, Json(json!({ "pairing_id": request.id, "expires_at_ms": request.expires_at_ms }))).into_response()
        }
        Err(ClaimError::TooManyAttempts) => error(StatusCode::TOO_MANY_REQUESTS, "Too many attempts. Wait a minute."),
        Err(ClaimError::Invalid) => error(StatusCode::BAD_REQUEST, "This code is not valid (anymore). Show a new code on the PC."),
    }
}

/// Long poll (up to 25 s) until the desktop allows or refuses the device.
async fn pair_poll(State(hub): State<Hub>, Path(id): Path<String>) -> Response {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(25);
    loop {
        let polled = hub.inner.pairings.lock().unwrap().poll(&id);
        let Some((state, notify)) = polled else {
            return error(StatusCode::NOT_FOUND, "This pairing request expired.");
        };
        match state {
            PairState::Approved { client_id, secret } => {
                return Json(json!({
                    "status": "approved",
                    "client_id": client_id,
                    "device_secret": secret,
                    "host_id": hub.inner.host_id,
                    "host_name": aurelius_host::host_name(),
                    "environment": hub.inner.environment,
                }))
                .into_response();
            }
            PairState::Rejected => return (StatusCode::FORBIDDEN, Json(json!({ "status": "rejected" }))).into_response(),
            PairState::Pending => {
                let notified = notify.notified();
                if tokio::time::timeout_at(deadline, notified).await.is_err() {
                    return Json(json!({ "status": "pending" })).into_response();
                }
            }
        }
    }
}

/// `Authorization: Basic base64(client_id:device_secret)` → a session token.
async fn session(State(hub): State<Hub>, headers: HeaderMap) -> Response {
    let Some((client_id, secret)) = basic(&headers) else {
        return error(StatusCode::UNAUTHORIZED, "Missing credentials.");
    };
    let store = hub.inner.store.clone();
    let found = tokio::task::spawn_blocking(move || store.read(|c| aurelius_storage::remote::find_client(c, client_id))).await;
    let Ok(Ok(Some(client))) = found else {
        return error(StatusCode::UNAUTHORIZED, "This device is not paired with this PC.");
    };
    if client.revoked || !auth::same(&client.credential_verifier, &auth::verifier(&secret)) {
        return error(StatusCode::UNAUTHORIZED, "This device is not paired with this PC.");
    }
    let token = auth::random_token();
    let token_verifier = auth::verifier(&token);
    let expires_at_ms = aurelius_storage::now_ms() + SESSION_MS;
    let session_id = uuid::Uuid::now_v7().to_string();
    let store = hub.inner.store.clone();
    let created = tokio::task::spawn_blocking(move || {
        store.write(move |tx| {
            aurelius_storage::remote::create_session(tx, &session_id, client_id, &token_verifier, expires_at_ms)?;
            aurelius_storage::remote::touch(tx, client_id)
        })
    })
    .await;
    if !matches!(created, Ok(Ok(()))) {
        return error(StatusCode::SERVICE_UNAVAILABLE, "The database is busy. Try again.");
    }
    Json(json!({
        "token": token,
        "expires_at_ms": expires_at_ms,
        "capabilities": client.capabilities,
        "name": client.name,
    }))
    .into_response()
}

fn basic(headers: &HeaderMap) -> Option<(aurelius_domain::ClientId, String)> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let encoded = value.strip_prefix("Basic ")?;
    let decoded = base64::engine::general_purpose::STANDARD.decode(encoded.trim()).ok()?;
    let text = String::from_utf8(decoded).ok()?;
    let (id, secret) = text.split_once(':')?;
    Some((aurelius_domain::ClientId::parse(id).ok()?, secret.to_owned()))
}

pub(crate) fn bearer(headers: &HeaderMap) -> Option<String> {
    headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("Bearer ")).map(|t| t.trim().to_owned())
}

/// The device behind `Authorization: Bearer <session token>`.
async fn authed(hub: &Hub, headers: &HeaderMap) -> Result<RemoteClient, Response> {
    let token = bearer(headers).ok_or_else(|| error(StatusCode::UNAUTHORIZED, "Missing session."))?;
    // An API key is for functions only, an app key for the WebSocket and MCP; neither is a session.
    if token.starts_with(aurelius_domain::agent_function::KEY_PREFIX) || token.starts_with(aurelius_domain::app_key::KEY_PREFIX) {
        return Err(error(StatusCode::UNAUTHORIZED, "This credential cannot be used here."));
    }
    let store = hub.inner.store.clone();
    let verifier = auth::verifier(&token);
    let found = tokio::task::spawn_blocking(move || {
        store.read(|c| aurelius_storage::remote::session_client(c, &verifier, aurelius_storage::now_ms()))
    })
    .await;
    match found {
        Ok(Ok(Some(client))) => Ok(client),
        Ok(Ok(None)) => Err(error(StatusCode::UNAUTHORIZED, "Session expired or device revoked.")),
        _ => Err(error(StatusCode::SERVICE_UNAVAILABLE, "The database is busy. Try again.")),
    }
}

/// A paired device with its session token, or an outside app with its app key.
async fn websocket(State(hub): State<Hub>, headers: HeaderMap, upgrade: WebSocketUpgrade) -> Response {
    let caller = match bearer(&headers).filter(|t| t.starts_with(aurelius_domain::app_key::KEY_PREFIX)) {
        Some(secret) => match hub.app_key(&secret).await {
            Ok(Some(key)) => crate::ws::Caller::App(key),
            Ok(None) => return error(StatusCode::UNAUTHORIZED, "This app key was revoked or does not exist."),
            Err(()) => return error(StatusCode::SERVICE_UNAVAILABLE, "The database is busy. Try again."),
        },
        None => match authed(&hub, &headers).await {
            Ok(client) => crate::ws::Caller::Device(client),
            Err(response) => return response,
        },
    };
    upgrade
        .max_message_size(aurelius_host::frame::MAX_FRAME_BYTES)
        .on_upgrade(move |socket| crate::ws::serve(hub, caller, socket))
}

/// One file (`file` field) into this device's inbox. The phone then sends
/// `attachment.add` with the returned host path over the WebSocket.
async fn upload(State(hub): State<Hub>, headers: HeaderMap, mut multipart: Multipart) -> Response {
    let client = match authed(&hub, &headers).await {
        Ok(client) => client,
        Err(response) => return response,
    };
    use aurelius_protocol::remote::{Capability, Needs, grants};
    if !grants(&client.capabilities, Needs::Cap(Capability::Chat)) {
        return error(StatusCode::FORBIDDEN, "This device may not attach files.");
    }
    let field = loop {
        match multipart.next_field().await {
            Ok(Some(field)) if field.name() == Some("file") => break field,
            Ok(Some(_)) => continue,
            Ok(None) => return error(StatusCode::BAD_REQUEST, "No file."),
            Err(_) => return error(StatusCode::BAD_REQUEST, "The upload was interrupted."),
        }
    };
    let name = safe_name(field.file_name().unwrap_or("file"));
    let bytes = match field.bytes().await {
        Ok(bytes) if bytes.len() <= MAX_UPLOAD_BYTES => bytes,
        Ok(_) => return error(StatusCode::PAYLOAD_TOO_LARGE, "Files up to 25 MB."),
        Err(_) => return error(StatusCode::BAD_REQUEST, "The upload was interrupted."),
    };
    let dir = hub.inbox(client.id).join(uuid::Uuid::now_v7().simple().to_string());
    let path = dir.join(&name);
    let len = bytes.len();
    let written = tokio::task::spawn_blocking({
        let path = path.clone();
        move || -> std::io::Result<()> {
            std::fs::create_dir_all(&dir)?;
            std::fs::write(&path, &bytes)
        }
    })
    .await;
    if !matches!(written, Ok(Ok(()))) {
        return error(StatusCode::INSUFFICIENT_STORAGE, "Could not store the file on the PC.");
    }
    Json(json!({ "path": path.to_string_lossy(), "name": name, "byte_length": len })).into_response()
}

/// A file name without directories or characters Windows refuses.
fn safe_name(raw: &str) -> String {
    let base = raw.rsplit(['/', '\\']).next().unwrap_or("file");
    let cleaned: String = base
        .chars()
        .map(|c| if c.is_control() || "<>:\"|?*".contains(c) { '_' } else { c })
        .collect::<String>()
        .trim_matches(|c: char| c == '.' || c.is_whitespace())
        .chars()
        .take(120)
        .collect();
    if cleaned.is_empty() { "file".into() } else { cleaned }
}

#[derive(Deserialize)]
struct PushBody {
    platform: String,
    token: String,
    environment: String,
}

async fn push_register(State(hub): State<Hub>, headers: HeaderMap, Json(body): Json<PushBody>) -> Response {
    let client = match authed(&hub, &headers).await {
        Ok(client) => client,
        Err(response) => return response,
    };
    if !matches!(body.platform.as_str(), "apns" | "fcm") || !matches!(body.environment.as_str(), "sandbox" | "production") {
        return error(StatusCode::BAD_REQUEST, "Unknown push platform or environment.");
    }
    let token = body.token.trim().to_owned();
    if token.is_empty() || token.len() > 512 {
        return error(StatusCode::BAD_REQUEST, "Invalid push token.");
    }
    let store = hub.inner.store.clone();
    let id = client.id;
    let saved = tokio::task::spawn_blocking(move || {
        store.write(move |tx| aurelius_storage::remote::set_push(tx, id, &body.platform, &token, &body.environment))
    })
    .await;
    match saved {
        Ok(Ok(())) => StatusCode::NO_CONTENT.into_response(),
        _ => error(StatusCode::SERVICE_UNAVAILABLE, "The database is busy. Try again."),
    }
}

async fn push_unregister(State(hub): State<Hub>, headers: HeaderMap) -> Response {
    let client = match authed(&hub, &headers).await {
        Ok(client) => client,
        Err(response) => return response,
    };
    let store = hub.inner.store.clone();
    let id = client.id;
    let _ = tokio::task::spawn_blocking(move || store.write(move |tx| aurelius_storage::remote::delete_push(tx, id))).await;
    StatusCode::NO_CONTENT.into_response()
}

/// The hash of the API key in `Authorization: Bearer ak_...`; the secret
/// itself goes no further. Anything else (a session token too) is refused.
fn api_key_verifier(headers: &HeaderMap) -> Option<String> {
    bearer(headers).filter(|t| t.starts_with(aurelius_domain::agent_function::KEY_PREFIX)).map(|t| auth::verifier(&t))
}

fn function_host(hub: &Hub) -> Result<std::sync::Arc<dyn crate::FunctionHost>, Response> {
    hub.inner.functions.get().cloned().ok_or_else(|| error(StatusCode::SERVICE_UNAVAILABLE, "Agent functions are not available."))
}

/// The HTTP status and JSON body for a call that was not accepted.
fn call_error(err: &CallError) -> (StatusCode, serde_json::Value) {
    let plain = |status, message: &str| (status, json!({ "error": message }));
    match err {
        CallError::Unauthorized => plain(StatusCode::UNAUTHORIZED, "Missing, unknown or revoked API key."),
        CallError::SwitchedOff => plain(StatusCode::SERVICE_UNAVAILABLE, "Agent functions are switched off."),
        CallError::NotFound => plain(StatusCode::NOT_FOUND, "Not found."),
        CallError::Forbidden => plain(StatusCode::FORBIDDEN, "This API key may not call this function."),
        CallError::BadParams(message) => plain(StatusCode::BAD_REQUEST, message),
        CallError::Unavailable(message) => plain(StatusCode::CONFLICT, message),
        CallError::Busy => plain(StatusCode::SERVICE_UNAVAILABLE, "The database is busy. Try again."),
        CallError::OverLimit(over) => (
            StatusCode::TOO_MANY_REQUESTS,
            json!({ "error": "Call limit reached.", "retry_after": over.retry_after_secs, "remaining": remaining_json(over.remaining) }),
        ),
    }
}

fn remaining_json(r: Remaining) -> serde_json::Value {
    json!({ "day": r.day, "week": r.week })
}

fn call_error_response(err: CallError) -> Response {
    let (status, body) = call_error(&err);
    let mut response = (status, Json(body)).into_response();
    if let CallError::OverLimit(over) = err {
        if let Ok(value) = over.retry_after_secs.to_string().parse() {
            response.headers_mut().insert(header::RETRY_AFTER, value);
        }
    }
    response
}

#[derive(Deserialize, Default)]
struct CallBody {
    #[serde(default)]
    params: serde_json::Map<String, serde_json::Value>,
}

/// `POST /v1/functions/{slug}/calls` with `{ "params": { ... } }`.
async fn function_call(State(hub): State<Hub>, Path(slug): Path<String>, headers: HeaderMap, body: axum::body::Bytes) -> Response {
    let Some(verifier) = api_key_verifier(&headers) else { return call_error_response(CallError::Unauthorized) };
    let host = match function_host(&hub) {
        Ok(host) => host,
        Err(response) => return response,
    };
    let body: CallBody = if body.iter().all(u8::is_ascii_whitespace) {
        CallBody::default()
    } else {
        match serde_json::from_slice(&body) {
            Ok(body) => body,
            Err(_) => return error(StatusCode::BAD_REQUEST, "The body must be JSON like {\"params\": {...}}."),
        }
    };
    let called = tokio::task::spawn_blocking(move || host.call(&verifier, &slug, &body.params)).await;
    match called {
        Ok(Ok(accepted)) => (
            StatusCode::ACCEPTED,
            Json(json!({ "call_id": accepted.call_id.to_string(), "status": "queued", "remaining": remaining_json(accepted.remaining) })),
        )
            .into_response(),
        Ok(Err(err)) => call_error_response(err),
        Err(_) => call_error_response(CallError::Busy),
    }
}

/// `GET /v1/calls/{call_id}`: only for the key that made the call.
async fn function_call_state(State(hub): State<Hub>, Path(call_id): Path<String>, headers: HeaderMap) -> Response {
    let Some(verifier) = api_key_verifier(&headers) else { return call_error_response(CallError::Unauthorized) };
    let host = match function_host(&hub) {
        Ok(host) => host,
        Err(response) => return response,
    };
    let id = call_id.clone();
    match tokio::task::spawn_blocking(move || host.state(&verifier, &id)).await {
        Ok(Ok(state)) => Json(call_state_json(&call_id, &state)).into_response(),
        Ok(Err(err)) => call_error_response(err),
        Err(_) => call_error_response(CallError::Busy),
    }
}

fn call_state_json(call_id: &str, state: &CallState) -> serde_json::Value {
    json!({
        "call_id": call_id,
        "status": state.status.as_str(),
        "result": state.result,
        "created_at": aurelius_domain::agent_function::iso_utc(state.created_at_ms),
        "finished_at": state.finished_at_ms.map(aurelius_domain::agent_function::iso_utc),
    })
}

#[cfg(test)]
mod tests {
    use super::safe_name;
    use super::*;
    use aurelius_domain::agent_function::{CallStatus, OverLimit};

    #[test]
    fn api_keys_are_recognised_only_by_their_prefix_and_are_hashed() {
        let mut headers = HeaderMap::new();
        assert_eq!(api_key_verifier(&headers), None);
        headers.insert(header::AUTHORIZATION, "Bearer session-token".parse().unwrap());
        assert_eq!(api_key_verifier(&headers), None, "a session token is not an API key");
        headers.insert(header::AUTHORIZATION, "Bearer ak_secret".parse().unwrap());
        let verifier = api_key_verifier(&headers).unwrap();
        assert_eq!(verifier, auth::verifier("ak_secret"));
        assert!(!verifier.contains("secret"));
        // The same hash the application stores when the key is made.
        assert_eq!(verifier, aurelius_storage::agent_functions::secret_verifier("ak_secret"));
    }

    #[test]
    fn call_errors_map_to_http_statuses() {
        let status = |e: CallError| call_error(&e).0.as_u16();
        assert_eq!(status(CallError::Unauthorized), 401);
        assert_eq!(status(CallError::Forbidden), 403);
        assert_eq!(status(CallError::NotFound), 404);
        assert_eq!(status(CallError::BadParams("x".into())), 400);
        assert_eq!(status(CallError::SwitchedOff), 503);
        let over = OverLimit { retry_after_secs: 90, remaining: Remaining { day: Some(0), week: None } };
        let (code, body) = call_error(&CallError::OverLimit(over));
        assert_eq!(code.as_u16(), 429);
        assert_eq!(body["retry_after"], 90);
        assert_eq!(body["remaining"], json!({ "day": 0, "week": null }));
        let response = call_error_response(CallError::OverLimit(over));
        assert_eq!(response.headers()[header::RETRY_AFTER], "90");
    }

    #[test]
    fn a_polled_call_shows_status_report_and_times() {
        let state = CallState { status: CallStatus::Succeeded, result: Some("# Done".into()), created_at_ms: 0, finished_at_ms: Some(86_400_000) };
        let json = call_state_json("c1", &state);
        assert_eq!(json["status"], "succeeded");
        assert_eq!(json["result"], "# Done");
        assert_eq!(json["created_at"], "1970-01-01T00:00:00Z");
        assert_eq!(json["finished_at"], "1970-01-02T00:00:00Z");
        let waiting = CallState { status: CallStatus::Queued, result: None, created_at_ms: 0, finished_at_ms: None };
        assert!(call_state_json("c1", &waiting)["finished_at"].is_null());
    }

    #[test]
    fn upload_names_cannot_escape_the_inbox() {
        assert_eq!(safe_name("../../etc/passwd"), "passwd");
        assert_eq!(safe_name("C:\\Windows\\win.ini"), "win.ini");
        assert_eq!(safe_name("IMG 0001.HEIC"), "IMG 0001.HEIC");
        assert_eq!(safe_name("a<b>:c?.txt"), "a_b__c_.txt");
        assert_eq!(safe_name(".."), "file");
    }
}
