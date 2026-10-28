//! `aurelius-mcp`: MCP for one app key over stdio, or for any app key over
//! Streamable HTTP.
//!
//! ```text
//! aurelius-mcp [--key amk_…] [--url https://<pc>.<tailnet>.ts.net:8443] [--data-dir <dir>]
//! aurelius-mcp --http 0.0.0.0:7464 [--key amk_…] [--url …] [--data-dir <dir>]
//! ```
//!
//! * The key comes from `--key` or `AURELIUS_MCP_KEY` (prefer the variable:
//!   arguments show in process lists).
//! * Without `--url` (or `AURELIUS_MCP_URL`) it talks to the Aurelius service
//!   on this computer; with it, to the server's remote gateway over Tailscale.
//! * `--http <addr>` (or `AURELIUS_MCP_HTTP`) serves `POST /mcp` instead of
//!   stdio, as the Docker image does. Each caller sends its own key as
//!   `Authorization: Bearer amk_…`; the key above, if any, serves callers
//!   without that header. Each key gets its own session with Aurelius, kept
//!   open between requests and opened again once it ends (a revoked key: 401).
//!
//! For MCP over HTTP on the server itself nothing needs to run: the gateway serves `/v1/mcp`.
//! Logs go to stderr; in stdio mode stdout carries only MCP messages.

use aurelius_client::{Client, ClientError};
use aurelius_mcp::Server;
use aurelius_mcp::http::{Refused, Sessions};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

const USAGE: &str = "usage: aurelius-mcp [--key amk_…] [--url https://<pc>.<tailnet>.ts.net:8443] [--data-dir <dir>]
       aurelius-mcp --http 0.0.0.0:7464 [--key amk_…] [--url …] [--data-dir <dir>]
  The app key can also come from AURELIUS_MCP_KEY, the URL from AURELIUS_MCP_URL,
  the HTTP address from AURELIUS_MCP_HTTP.
  Without a URL it uses the Aurelius service on this computer.
  With --http it serves MCP over Streamable HTTP at /mcp; callers send their app key
  as Authorization: Bearer amk_…, and the key above (if any) serves callers without one.";

struct Options {
    key: Option<String>,
    url: Option<String>,
    data_dir: Option<PathBuf>,
    http: Option<String>,
}

fn options() -> anyhow::Result<Options> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("{USAGE}");
        std::process::exit(0);
    }
    let arg = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let env = |name: &str| std::env::var(name).ok().map(|v| v.trim().to_owned()).filter(|v| !v.is_empty());
    let key = arg("--key").or_else(|| env("AURELIUS_MCP_KEY"));
    let http = arg("--http").or_else(|| env("AURELIUS_MCP_HTTP"));
    match &key {
        Some(key) if !key.starts_with(aurelius_domain::app_key::KEY_PREFIX) => {
            anyhow::bail!("This is not an Aurelius app key (they start with {}). Make one in Aurelius under Settings.", aurelius_domain::app_key::KEY_PREFIX)
        }
        None if http.is_none() => anyhow::bail!("No app key: pass --key or set AURELIUS_MCP_KEY.\n{USAGE}"),
        _ => {}
    }
    Ok(Options { key, url: arg("--url").or_else(|| env("AURELIUS_MCP_URL")), data_dir: arg("--data-dir").map(Into::into), http })
}

async fn connect(options: &Options, key: &str) -> Result<Arc<Client>, ClientError> {
    let name = format!("aurelius-mcp {}", env!("CARGO_PKG_VERSION"));
    let client = match &options.url {
        Some(url) => Client::connect_app_remote(url, key, &name).await?,
        None => {
            let root = match &options.data_dir {
                Some(dir) => dir.clone(),
                None => aurelius_host::data_dir().map_err(|err| ClientError::Local(err.to_string()))?,
            };
            Client::connect_app(&aurelius_host::endpoint(&root), key, &name).await?
        }
    };
    Ok(Arc::new(client))
}

/// `--http`: one MCP server per app key, each on its own session with Aurelius.
struct Pool {
    options: Options,
    servers: tokio::sync::Mutex<HashMap<String, Arc<Server>>>,
}

impl Sessions for Pool {
    async fn open(&self, key: &str) -> Result<Arc<Server>, Refused> {
        let mut servers = self.servers.lock().await;
        if let Some(server) = servers.get(key) {
            // Revoking closes the session: connecting again then gets the 401.
            if !server.closed().await {
                return Ok(server.clone());
            }
            servers.remove(key);
        }
        match connect(&self.options, key).await {
            Ok(client) => {
                if let Some(app) = &client.hello.app {
                    eprintln!("aurelius-mcp: \"{}\" connected to {}", app.name, client.hello.host_name);
                }
                let server = Arc::new(Server::new(client));
                servers.insert(key.to_owned(), server.clone());
                Ok(server)
            }
            Err(err) => {
                // Over local IPC the hello refuses an unknown key as forbidden.
                let refused = err.is_final() || matches!(&err, ClientError::Protocol(e) if e.code == aurelius_domain::ErrorCode::Forbidden);
                Err(if refused { Refused::Unauthorized(err.to_string()) } else { Refused::Unavailable(err.to_string()) })
            }
        }
    }
}

async fn serve_http(options: Options) -> anyhow::Result<()> {
    use axum::routing::post;
    let addr = options.http.clone().unwrap_or_default();
    let target = options.url.clone().unwrap_or_else(|| "the Aurelius service on this computer".into());
    let fallback = if options.key.is_some() { ", AURELIUS_MCP_KEY for callers without a key" } else { "" };
    let pool = Arc::new(Pool { options, servers: Default::default() });
    let app = axum::Router::new().route(
        "/mcp",
        post(|axum::extract::State(pool): axum::extract::State<Arc<Pool>>, headers: axum::http::HeaderMap, body: axum::body::Bytes| async move {
            aurelius_mcp::http::post(pool.as_ref(), pool.options.key.as_deref(), &headers, &body).await
        })
        .get(aurelius_mcp::http::not_here)
        .delete(aurelius_mcp::http::not_here),
    );
    let listener = tokio::net::TcpListener::bind(&addr).await.map_err(|err| anyhow::anyhow!("Cannot listen on {addr}: {err}"))?;
    eprintln!("aurelius-mcp: MCP at http://{addr}/mcp, through {target}{fallback}");
    axum::serve(listener, app.with_state(pool))
        .with_graceful_shutdown(stopped())
        .await?;
    Ok(())
}

/// Ctrl+C, or `docker stop` (SIGTERM).
async fn stopped() {
    #[cfg(unix)]
    {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).expect("SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = term.recv() => {}
        }
    }
    #[cfg(not(unix))]
    let _ = tokio::signal::ctrl_c().await;
}

async fn serve_stdio(options: Options) -> anyhow::Result<()> {
    let key = options.key.clone().unwrap_or_default();
    let client = connect(&options, &key).await?;
    if let Some(app) = &client.hello.app {
        let scopes: Vec<&str> = app.scopes.iter().map(|s| s.as_str()).collect();
        eprintln!("aurelius-mcp: connected to {} as \"{}\" ({})", client.hello.host_name, app.name, scopes.join(", "));
    }
    let server = Server::new(client);
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut out = tokio::io::stdout();
    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        let message = match serde_json::from_str::<serde_json::Value>(&line) {
            Ok(message) => message,
            Err(_) => {
                let reply = serde_json::json!({ "jsonrpc": "2.0", "id": null, "error": { "code": -32700, "message": "Parse error" } });
                out.write_all(format!("{reply}\n").as_bytes()).await?;
                out.flush().await?;
                continue;
            }
        };
        // The service restarted (an update) or the network dropped: connect again before the next call.
        if server.lost() {
            match connect(&options, &key).await {
                Ok(client) => server.reconnect(client).await,
                Err(err) => eprintln!("aurelius-mcp: still no connection: {err}"),
            }
        }
        if let Some(reply) = server.handle(message).await {
            out.write_all(format!("{reply}\n").as_bytes()).await?;
            out.flush().await?;
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let options = options()?;
    if options.http.is_some() { serve_http(options).await } else { serve_stdio(options).await }
}
