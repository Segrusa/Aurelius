//! A server on another computer: pairing, session tokens and
//! uploads over the gateway's HTTPS. The WebSocket itself is in `transport`.
//!
//! Secrets travel only in `Authorization` headers and are never logged.

use crate::ClientError;
use aurelius_domain::{ClientId, HostId};
use aurelius_protocol::remote::PairingPayload;
use base64::Engine;
use serde::Deserialize;
use std::path::Path;
use std::time::Duration;

/// A paired server: where it is and the device credential this computer got.
#[derive(Clone, PartialEq, Eq)]
pub struct RemoteServer {
    /// `https://<machine>.<tailnet>.ts.net:8443` (`http://` only on loopback).
    pub url: String,
    pub host_id: HostId,
    pub host_name: String,
    pub client_id: ClientId,
    pub secret: String,
    /// `dev` or `production`.
    pub environment: String,
}

impl std::fmt::Debug for RemoteServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemoteServer")
            .field("url", &self.url)
            .field("host_id", &self.host_id)
            .field("host_name", &self.host_name)
            .field("client_id", &self.client_id)
            .field("secret", &"<redacted>")
            .field("environment", &self.environment)
            .finish()
    }
}

/// HTTPS anywhere; plain HTTP only to this computer (dev, tests).
pub fn check_url(url: &str) -> Result<reqwest::Url, ClientError> {
    let parsed = reqwest::Url::parse(url.trim()).map_err(|_| ClientError::Unreachable(format!("\"{url}\" is not a server address")))?;
    // `host.docker.internal`: a container (aurelius-mcp's Docker image) on the server PC
    // reaching the gateway's loopback port; the traffic stays on the PC.
    let loopback = matches!(parsed.host_str(), Some("127.0.0.1" | "localhost" | "[::1]" | "::1" | "host.docker.internal"));
    match parsed.scheme() {
        "https" => Ok(parsed),
        "http" if loopback => Ok(parsed),
        "http" => Err(ClientError::Unreachable("Only https:// addresses (Tailscale Serve) are allowed for another computer.".into())),
        _ => Err(ClientError::Unreachable(format!("\"{url}\" is not a server address"))),
    }
}

pub(crate) fn http() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        // Long enough for the 25 s pairing long poll.
        .timeout(Duration::from_secs(40))
        .build()
        .unwrap_or_default()
}

fn join(base: &str, path: &str) -> String {
    format!("{}{path}", base.trim_end_matches('/'))
}

#[derive(Deserialize)]
struct ErrorBody {
    error: Option<String>,
}

async fn error_text(response: reqwest::Response) -> String {
    let status = response.status();
    response
        .json::<ErrorBody>()
        .await
        .ok()
        .and_then(|b| b.error)
        .unwrap_or_else(|| format!("The server answered {status}."))
}

fn unreachable(server: &str, err: reqwest::Error) -> ClientError {
    ClientError::Unreachable(format!("{server} is not reachable ({err}). Is it on, with Tailscale connected?"))
}

/// A session token for `server` (12 hours), in exchange for the device secret.
pub(crate) async fn session(http: &reqwest::Client, server: &RemoteServer) -> Result<String, ClientError> {
    let basic = base64::engine::general_purpose::STANDARD.encode(format!("{}:{}", server.client_id, server.secret));
    let response = http
        .post(join(&server.url, "/v1/session"))
        .header("authorization", format!("Basic {basic}"))
        .send()
        .await
        .map_err(|e| unreachable(&server.host_name, e))?;
    match response.status() {
        s if s.is_success() => {
            #[derive(Deserialize)]
            struct Session {
                token: String,
            }
            let body: Session = response.json().await.map_err(|_| ClientError::UnexpectedResponse)?;
            Ok(body.token)
        }
        reqwest::StatusCode::UNAUTHORIZED => Err(ClientError::Unauthorized(format!(
            "This computer is no longer paired with {}. Pair it again.",
            server.host_name
        ))),
        _ => Err(ClientError::Unreachable(error_text(response).await)),
    }
}

/// Upload a file from this computer into this device's inbox on the server;
/// returns the path there, for `attach_file`.
pub(crate) async fn upload(http: &reqwest::Client, server: &RemoteServer, token: &str, local: &Path) -> Result<Result<String, reqwest::StatusCode>, ClientError> {
    let bytes = tokio::fs::read(local).await.map_err(|e| ClientError::Local(format!("{}: {e}", local.display())))?;
    let name = local.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "file".into());
    let form = reqwest::multipart::Form::new().part("file", reqwest::multipart::Part::bytes(bytes).file_name(name));
    let response = http
        .post(join(&server.url, "/v1/upload"))
        .header("authorization", format!("Bearer {token}"))
        .timeout(Duration::from_secs(300))
        .multipart(form)
        .send()
        .await
        .map_err(|e| unreachable(&server.host_name, e))?;
    match response.status() {
        s if s.is_success() => {
            #[derive(Deserialize)]
            struct Uploaded {
                path: String,
            }
            let body: Uploaded = response.json().await.map_err(|_| ClientError::UnexpectedResponse)?;
            Ok(Ok(body.path))
        }
        reqwest::StatusCode::UNAUTHORIZED => Ok(Err(reqwest::StatusCode::UNAUTHORIZED)),
        _ => Err(ClientError::Local(error_text(response).await)),
    }
}

/// Where pairing stands while the server's user decides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PairProgress {
    /// The code was accepted; waiting for "Allow as owner" on the server.
    Waiting { host_name: String },
}

/// Pair this computer with the server in `pairing_text` (`aurelius-pair:…` or
/// the QR's JSON), asking for full access. Ends when the server's user allows
/// or refuses, or the request expires (5 minutes).
pub async fn pair(
    pairing_text: &str,
    device_name: &str,
    progress: impl Fn(PairProgress),
) -> Result<RemoteServer, ClientError> {
    let payload = PairingPayload::from_text(pairing_text)
        .ok_or_else(|| ClientError::Local("This is not a pairing text. Copy it again from Remote access on the server.".into()))?;
    let mine = aurelius_host::Environment::current().as_str();
    if payload.env != mine {
        return Err(ClientError::Local(format!(
            "This pairing text is for the {} server; this app is {mine}. Dev and production never mix.",
            payload.env
        )));
    }
    check_url(&payload.url)?;
    let http = http();
    let response = http
        .post(join(&payload.url, "/v1/pair"))
        .json(&serde_json::json!({
            "code": payload.code,
            "device_name": device_name,
            "platform": std::env::consts::OS,
            "role": "owner",
        }))
        .send()
        .await
        .map_err(|e| unreachable(&payload.host_name, e))?;
    if !response.status().is_success() {
        return Err(ClientError::Local(error_text(response).await));
    }
    #[derive(Deserialize)]
    struct Claimed {
        pairing_id: String,
    }
    let claimed: Claimed = response.json().await.map_err(|_| ClientError::UnexpectedResponse)?;
    progress(PairProgress::Waiting { host_name: payload.host_name.clone() });

    #[derive(Deserialize)]
    struct Poll {
        status: String,
        client_id: Option<ClientId>,
        device_secret: Option<String>,
        host_id: Option<HostId>,
        host_name: Option<String>,
        environment: Option<String>,
    }
    loop {
        let response = http
            .get(join(&payload.url, &format!("/v1/pair/{}", claimed.pairing_id)))
            .send()
            .await
            .map_err(|e| unreachable(&payload.host_name, e))?;
        match response.status() {
            reqwest::StatusCode::FORBIDDEN => {
                return Err(ClientError::Local(format!("{} refused this computer.", payload.host_name)));
            }
            reqwest::StatusCode::NOT_FOUND => {
                return Err(ClientError::Local("The pairing request expired. Start again on the server.".into()));
            }
            s if !s.is_success() => return Err(ClientError::Local(error_text(response).await)),
            _ => {}
        }
        let poll: Poll = response.json().await.map_err(|_| ClientError::UnexpectedResponse)?;
        if poll.status != "approved" {
            continue;
        }
        let (Some(client_id), Some(secret), Some(host_id)) = (poll.client_id, poll.device_secret, poll.host_id) else {
            return Err(ClientError::UnexpectedResponse);
        };
        if host_id != payload.host_id {
            return Err(ClientError::WrongServer("The server that answered is not the one in the pairing text.".into()));
        }
        return Ok(RemoteServer {
            url: payload.url.trim_end_matches('/').to_owned(),
            host_id,
            host_name: poll.host_name.unwrap_or(payload.host_name),
            client_id,
            secret,
            environment: poll.environment.unwrap_or(payload.env),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_https_leaves_this_computer() {
        assert!(check_url("https://desktop.tail1.ts.net:8443").is_ok());
        assert!(check_url("http://127.0.0.1:47614").is_ok());
        assert!(check_url("http://localhost:47614").is_ok());
        assert!(check_url("http://desktop.tail1.ts.net:8443").is_err());
        assert!(check_url("ftp://x").is_err());
        assert!(check_url("not a url").is_err());
    }

    #[test]
    fn the_secret_never_shows_in_debug() {
        let server = RemoteServer {
            url: "https://x".into(),
            host_id: HostId::new(),
            host_name: "pc".into(),
            client_id: ClientId::new(),
            secret: "s3cr3t".into(),
            environment: "dev".into(),
        };
        assert!(!format!("{server:?}").contains("s3cr3t"));
    }
}
