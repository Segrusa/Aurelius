//! The two ways to the server, as a pair of channels: requests out, server
//! messages in. `Client` is the same over both.
//!
//! * pipe: the local named pipe / Unix socket, length-prefixed JSON frames;
//! * WebSocket: the gateway over Tailscale Serve (TLS), one JSON message per
//!   text frame, with a session token.

use crate::ClientError;
use aurelius_host::ipc::{self, Endpoint, read_frame, write_frame};
use aurelius_protocol::{Request, ServerMessage};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::{self, Message, client::IntoClientRequest, protocol::WebSocketConfig};

/// Close code of a revoked device (or remote access turned off).
pub const CLOSE_REVOKED: u16 = 4401;
/// No frame at all (the gateway pings every 20 s) for this long: offline.
const WATCHDOG: Duration = Duration::from_secs(45);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) struct Duplex {
    pub outgoing: mpsc::Sender<Request>,
    pub incoming: mpsc::Receiver<ServerMessage>,
    /// Set when the server closed with `CLOSE_REVOKED`.
    pub revoked: Arc<AtomicBool>,
}

pub(crate) async fn pipe(endpoint: &Endpoint) -> Result<Duplex, ClientError> {
    let stream = ipc::connect(endpoint).await.map_err(|e| ClientError::Unreachable(e.to_string()))?;
    let (mut reader, mut writer) = tokio::io::split(stream);
    let (outgoing, mut outgoing_rx) = mpsc::channel::<Request>(64);
    let (incoming_tx, incoming) = mpsc::channel::<ServerMessage>(256);
    tokio::spawn(async move {
        while let Some(request) = outgoing_rx.recv().await {
            if write_frame(&mut writer, &request).await.is_err() {
                break;
            }
        }
    });
    tokio::spawn(async move {
        while let Ok(Some(message)) = read_frame::<ServerMessage, _>(&mut reader).await {
            if incoming_tx.send(message).await.is_err() {
                break;
            }
        }
    });
    Ok(Duplex { outgoing, incoming, revoked: Arc::default() })
}

fn tls_config() -> Result<Arc<rustls::ClientConfig>, ClientError> {
    use rustls_platform_verifier::BuilderVerifierExt;
    // Explicit provider: more than one is compiled in through dependencies.
    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .and_then(|b| b.with_platform_verifier())
        .map_err(|e| ClientError::Unreachable(format!("TLS setup failed: {e}")))?
        .with_no_client_auth();
    Ok(Arc::new(config))
}

/// Open the gateway's WebSocket at `base` (`https://…` or loopback `http://…`).
pub(crate) async fn websocket(base: &str, token: &str) -> Result<Duplex, ClientError> {
    let url = crate::remote::check_url(base)?;
    let host = url.host_str().ok_or_else(|| ClientError::Unreachable("no host in the server address".into()))?.trim_matches(['[', ']']).to_owned();
    let tls = url.scheme() == "https";
    let port = url.port_or_known_default().unwrap_or(if tls { 443 } else { 80 });
    let ws_url = format!("{}://{}/v1/ws", if tls { "wss" } else { "ws" }, url.host_str().map(|h| format!("{h}:{port}")).unwrap_or_default());
    let mut request = ws_url.into_client_request().map_err(|e| ClientError::Unreachable(e.to_string()))?;
    let bearer = format!("Bearer {token}").parse().map_err(|_| ClientError::UnexpectedResponse)?;
    request.headers_mut().insert("authorization", bearer);

    let tcp = tokio::time::timeout(CONNECT_TIMEOUT, tokio::net::TcpStream::connect((host.as_str(), port)))
        .await
        .map_err(|_| ClientError::Unreachable(format!("{host} did not answer in time. Is it on, with Tailscale connected?")))?
        .map_err(|e| ClientError::Unreachable(format!("{host}: {e}")))?;
    let _ = tcp.set_nodelay(true);
    let mut config = WebSocketConfig::default();
    config.max_message_size = Some(aurelius_host::frame::MAX_FRAME_BYTES);
    if tls {
        let name = rustls::pki_types::ServerName::try_from(host.clone()).map_err(|e| ClientError::Unreachable(e.to_string()))?;
        let stream = tokio_rustls::TlsConnector::from(tls_config()?)
            .connect(name, tcp)
            .await
            .map_err(|e| ClientError::Unreachable(format!("{host}: secure connection failed ({e})")))?;
        handshake(request, stream, config).await
    } else {
        handshake(request, tcp, config).await
    }
}

async fn handshake<S>(request: tungstenite::handshake::client::Request, stream: S, config: WebSocketConfig) -> Result<Duplex, ClientError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let (socket, _) = match tokio::time::timeout(CONNECT_TIMEOUT, tokio_tungstenite::client_async_with_config(request, stream, Some(config))).await {
        Err(_) => return Err(ClientError::Unreachable("The server did not finish the connection in time.".into())),
        Ok(Err(tungstenite::Error::Http(response))) if response.status() == 401 => {
            return Err(ClientError::Unauthorized("This computer is no longer paired with the server. Pair it again.".into()));
        }
        Ok(Err(err)) => return Err(ClientError::Unreachable(err.to_string())),
        Ok(Ok(pair)) => pair,
    };
    let (outgoing, mut outgoing_rx) = mpsc::channel::<Request>(64);
    let (incoming_tx, incoming) = mpsc::channel::<ServerMessage>(256);
    let revoked = Arc::new(AtomicBool::new(false));
    let revoked_flag = revoked.clone();
    tokio::spawn(async move {
        let (mut sink, mut stream) = socket.split();
        let mut deadline = tokio::time::Instant::now() + WATCHDOG;
        loop {
            tokio::select! {
                request = outgoing_rx.recv() => match request {
                    Some(request) => {
                        let Ok(text) = serde_json::to_string(&request) else { continue };
                        if sink.send(Message::Text(text.into())).await.is_err() {
                            break;
                        }
                    }
                    None => {
                        let _ = sink.send(Message::Close(None)).await;
                        break;
                    }
                },
                frame = stream.next() => {
                    deadline = tokio::time::Instant::now() + WATCHDOG;
                    match frame {
                        Some(Ok(Message::Text(text))) => match serde_json::from_str::<ServerMessage>(text.as_str()) {
                            Ok(message) => {
                                if incoming_tx.send(message).await.is_err() {
                                    break;
                                }
                            }
                            Err(err) => tracing::warn!(error = %err, "unreadable message from the server"),
                        },
                        Some(Ok(Message::Ping(payload))) => {
                            if sink.send(Message::Pong(payload)).await.is_err() {
                                break;
                            }
                        }
                        Some(Ok(Message::Close(frame))) => {
                            if frame.is_some_and(|f| u16::from(f.code) == CLOSE_REVOKED) {
                                revoked_flag.store(true, Ordering::SeqCst);
                            }
                            break;
                        }
                        Some(Ok(_)) => {}
                        Some(Err(_)) | None => break,
                    }
                }
                _ = tokio::time::sleep_until(deadline) => {
                    tracing::info!("no word from the server for {}s; reconnecting", WATCHDOG.as_secs());
                    break;
                }
            }
        }
        // Dropping `incoming_tx` tells the client the connection is gone.
    });
    Ok(Duplex { outgoing, incoming, revoked })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Manual, against a real gateway behind Tailscale Serve: proves the TLS
    /// setup (platform verifier, aws-lc-rs) and the WebSocket upgrade. An
    /// invalid token must come back as "unauthorized", not as a TLS error.
    ///   AURELIUS_TLS_TEST_URL=https://<pc>.<tailnet>.ts.net:8443 cargo test -p aurelius-client tls -- --ignored
    #[tokio::test]
    #[ignore]
    async fn tls_reaches_a_real_gateway() {
        let url = std::env::var("AURELIUS_TLS_TEST_URL").expect("AURELIUS_TLS_TEST_URL");
        match websocket(&url, "not-a-token").await {
            Err(ClientError::Unauthorized(_)) => {}
            Err(other) => panic!("{other}"),
            Ok(_) => panic!("an invalid token was accepted"),
        }
    }
}
