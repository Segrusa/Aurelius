//! HTTP surface of the gateway. Tokens travel only in `Authorization`
//! headers or a browser's `HttpOnly` cookies (`web`), never in URLs, and are never logged.

use crate::auth::{self, ClaimError, PairState};
use crate::{Hub, MAX_UPLOAD_BYTES, SESSION_MS, web};
use aurelius_storage::remote::RemoteClient;
use axum::extract::{DefaultBodyLimit, Multipart, Path, State, WebSocketUpgrade};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get, post, put};
use axum::{Json, Router};
use base64::Engine;
use serde::Deserialize;
use serde_json::json;
use std::time::Duration;

pub fn router(hub: Hub) -> Router {
    let api = Router::new()
        .route("/v1/health", get(health))
        .route("/v1/pair", post(pair))
        .route("/v1/pair/{id}", get(pair_poll))
        .route("/v1/session", post(session).delete(end_browser_session))
        .route("/v1/ws", get(websocket))
        .route("/v1/upload", post(upload).layer(DefaultBodyLimit::max(MAX_UPLOAD_BYTES + 64 * 1024)))
        .route("/v1/push", put(push_register).delete(push_unregister))
        .route("/v1/{*rest}", any(|| async { error(StatusCode::NOT_FOUND, "Not found.") }))
        .layer(DefaultBodyLimit::max(64 * 1024))
        .with_state(hub);
    let Some(dir) = web::site_dir() else { return api };
    // A path that is no file is a page of the app: it gets index.html and routes itself.
    let site = tower_http::services::ServeDir::new(&dir).fallback(tower_http::services::ServeFile::new(dir.join("index.html")));
    api.fallback_service(site).layer(axum::middleware::map_response(security_headers))
}

async fn security_headers(mut response: Response) -> Response {
    for (name, value) in web::SECURITY_HEADERS {
        response.headers_mut().insert(HeaderName::from_static(name), HeaderValue::from_static(value));
    }
    response
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

/// Phones and browsers pair as devices; computers may ask to be an owner.
const PHONES: [&str; 2] = ["ios", "android"];
const COMPUTERS: [&str; 3] = ["windows", "macos", "linux"];
/// Its secret goes into an `HttpOnly` cookie, out of reach of the page's scripts.
const BROWSER: &str = "web";

async fn pair(State(hub): State<Hub>, headers: HeaderMap, Json(body): Json<PairBody>) -> Response {
    let platform = body.platform.as_str();
    let computer = COMPUTERS.contains(&platform);
    let browser = platform == BROWSER;
    if !computer && !browser && !PHONES.contains(&platform) {
        return error(StatusCode::BAD_REQUEST, "Unknown platform.");
    }
    if browser && !hub.allows_origin(&headers).await {
        return error(StatusCode::FORBIDDEN, "This page may not pair with this server.");
    }
    let owner = match body.role.as_deref() {
        None | Some("device") => false,
        Some("owner") if computer => true,
        Some(_) => return error(StatusCode::BAD_REQUEST, "Only a computer can ask for full access."),
    };
    let name = body.device_name.trim();
    let name = match (name.is_empty(), computer, browser) {
        (false, ..) => name,
        (true, true, _) => "Computer",
        (true, _, true) => "Browser",
        (true, ..) => "Phone",
    };
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
                let host_name = hub.server_name().await;
                let store = hub.inner.store.clone();
                let client = tokio::task::spawn_blocking(move || store.read(|c| aurelius_storage::remote::find_client(c, client_id))).await;
                let Ok(Ok(Some(client))) = client else {
                    return error(StatusCode::SERVICE_UNAVAILABLE, "The database is busy. Try again.");
                };
                let mut body = json!({
                    "status": "approved",
                    "client_id": client_id,
                    "host_id": hub.inner.host_id,
                    "host_name": host_name,
                    "environment": hub.inner.environment,
                });
                if client.platform != BROWSER {
                    body["device_secret"] = json!(secret);
                    return Json(body).into_response();
                }
                let mut response = Json(body).into_response();
                web::set_device_cookie(&mut response, &client_id.to_string(), &secret);
                return response;
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

/// `Authorization: Basic base64(client_id:device_secret)` → a session token. A browser's
/// device cookie, from its own origin, → the same session as a cookie; scripts never see it.
async fn session(State(hub): State<Hub>, headers: HeaderMap) -> Response {
    let browser = !headers.contains_key(header::AUTHORIZATION);
    let credentials = if browser { web::device_cookie(&headers) } else { basic(&headers) };
    let Some((client_id, secret)) = credentials else {
        return error(StatusCode::UNAUTHORIZED, "Missing credentials.");
    };
    if browser && !hub.allows_origin(&headers).await {
        return error(StatusCode::FORBIDDEN, "This page may not use this server.");
    }
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
    let mut body = json!({
        "expires_at_ms": expires_at_ms,
        "capabilities": client.capabilities,
        "name": client.name,
    });
    if !browser {
        body["token"] = json!(token);
        return Json(body).into_response();
    }
    let mut response = Json(body).into_response();
    web::set_session_cookie(&mut response, &token, SESSION_MS / 1000);
    response
}

/// A browser forgets its pairing and session. The server keeps the device until it is revoked.
async fn end_browser_session(State(hub): State<Hub>, headers: HeaderMap) -> Response {
    if !hub.allows_origin(&headers).await {
        return error(StatusCode::FORBIDDEN, "This page may not use this server.");
    }
    let mut response = StatusCode::NO_CONTENT.into_response();
    web::clear_cookies(&mut response);
    response
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

/// The device behind `Authorization: Bearer <session token>`, or a browser's session cookie
/// sent from its own origin.
async fn authed(hub: &Hub, headers: &HeaderMap) -> Result<RemoteClient, Response> {
    let token = match bearer(headers) {
        Some(token) => token,
        None => {
            let token = web::session_cookie(headers).ok_or_else(|| error(StatusCode::UNAUTHORIZED, "Missing session."))?;
            if !hub.allows_origin(headers).await {
                return Err(error(StatusCode::FORBIDDEN, "This page may not use this server."));
            }
            token
        }
    };
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

/// A paired device with its session token.
async fn websocket(State(hub): State<Hub>, headers: HeaderMap, upgrade: WebSocketUpgrade) -> Response {
    let client = match authed(&hub, &headers).await {
        Ok(client) => client,
        Err(response) => return response,
    };
    upgrade
        .max_message_size(aurelius_host::frame::MAX_FRAME_BYTES)
        .on_upgrade(move |socket| crate::ws::serve(hub, client, socket))
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

#[cfg(test)]
mod tests {
    use super::safe_name;

    #[test]
    fn upload_names_cannot_escape_the_inbox() {
        assert_eq!(safe_name("../../etc/passwd"), "passwd");
        assert_eq!(safe_name("C:\\Windows\\win.ini"), "win.ini");
        assert_eq!(safe_name("IMG 0001.HEIC"), "IMG 0001.HEIC");
        assert_eq!(safe_name("a<b>:c?.txt"), "a_b__c_.txt");
        assert_eq!(safe_name(".."), "file");
    }
}
