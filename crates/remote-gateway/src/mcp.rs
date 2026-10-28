//! MCP over Streamable HTTP for outside apps (feature `app_keys`):
//! `POST /v1/mcp` with `Authorization: Bearer amk_…`. The HTTP side is
//! `aurelius_mcp::http`, shared with `aurelius-mcp --http`; here each request
//! opens an in-process daemon session of that app and runs the tools of
//! `aurelius-mcp` on it through `aurelius_client`. The key is checked on every
//! request, so a revoked key stops at once.

use crate::Hub;
use aurelius_mcp::http::{Refused, Sessions};
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::Response;
use std::sync::Arc;
use tokio::sync::mpsc;

pub use aurelius_mcp::http::not_here;

impl Sessions for Hub {
    async fn open(&self, key: &str) -> Result<Arc<aurelius_mcp::Server>, Refused> {
        let key = match self.app_key(key).await {
            Ok(Some(key)) => key,
            Ok(None) => return Err(Refused::Unauthorized("This app key was revoked or does not exist.".into())),
            Err(()) => return Err(Refused::Unavailable("The database is busy. Try again.".into())),
        };
        let Some(dispatch) = self.inner.dispatch.get().cloned() else {
            return Err(Refused::Unavailable("Aurelius is starting. Try again.".into()));
        };
        // A daemon session of this app, in-process; `aurelius_client` speaks to it as to any server.
        let (req_tx, req_rx) = mpsc::channel(16);
        let (out_tx, out_rx) = mpsc::channel(256);
        dispatch.open_app(key, req_rx, out_tx);
        let client = aurelius_client::Client::over_channels(req_tx, out_rx, "aurelius-mcp (http)").await.map_err(|err| Refused::Unavailable(err.to_string()))?;
        Ok(Arc::new(aurelius_mcp::Server::new(Arc::new(client))))
    }
}

pub async fn post(State(hub): State<Hub>, headers: HeaderMap, body: axum::body::Bytes) -> Response {
    aurelius_mcp::http::post(&hub, None, &headers, &body).await
}
