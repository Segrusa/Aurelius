//! MCP OAuth login against a fake MCP server and authorization server on
//! localhost: the status in `mcp.list`, `mcp.login_start`, the browser coming back
//! to the daemon's own callback, the pasted address of a browser elsewhere,
//! a refused login, `mcp.logout`, the events, and that no token reaches a client.
//!
//! One test in this binary: it points the daemon at its own Claude Code files
//! through environment variables.

mod common;

use aurelius_domain::EventKind;
use aurelius_protocol::{Event, McpAuthStatus, McpLoginState};
use common::fake_mcp::{self, Fake};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn query(url: &str) -> HashMap<String, String> {
    url::Url::parse(url).unwrap().query_pairs().into_owned().collect()
}

async fn wait_for(rx: &mut tokio::sync::mpsc::UnboundedReceiver<Event>, kind: EventKind, aggregate: &str) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(event) = rx.recv().await {
            if event.kind == kind && event.aggregate_id == aggregate {
                return;
            }
        }
        panic!("events ended");
    })
    .await
    .unwrap_or_else(|_| panic!("no {kind:?} for {aggregate}"));
}

fn no_tokens(value: Value) {
    let text = value.to_string();
    assert!(!text.contains("SECRET") && !text.contains("code_verifier"), "{text}");
}

#[tokio::test(flavor = "multi_thread")]
async fn logging_in_to_an_mcp_server() {
    let fake = Arc::new(Mutex::new(Fake::default()));
    let base = fake_mcp::start(fake.clone()).await;

    let dir = tempfile::tempdir().unwrap();
    let claude_json = dir.path().join("claude.json");
    std::fs::write(&claude_json, json!({ "mcpServers": { "recipes": { "type": "http", "url": format!("{base}/mcp") }, "local": { "command": "npx" } } }).to_string()).unwrap();
    let claude_dir = dir.path().join("claude");
    std::fs::create_dir_all(&claude_dir).unwrap();
    let credentials = claude_dir.join(".credentials.json");
    std::fs::write(&credentials, r#"{"claudeAiOauth":{"accessToken":"keep-me"}}"#).unwrap();
    std::fs::write(claude_dir.join("mcp-needs-auth-cache.json"), r#"{"recipes":{"timestamp":1}}"#).unwrap();
    // SAFETY of the test: the only test of this binary, set before the daemon starts.
    unsafe {
        std::env::set_var("AURELIUS_CLAUDE_JSON", &claude_json);
        std::env::set_var("AURELIUS_CLAUDE_CREDENTIALS", &credentials);
    }

    let root = dir.path().join("data");
    let daemon = common::start(&root).await;
    let (client, mut events) = common::connect_events(&root).await;
    client.subscribe(0).await.unwrap();

    let status = |servers: &aurelius_protocol::McpServers, name: &str| servers.servers.iter().find(|s| s.name == name).unwrap().auth;
    let listed = client.mcp_list().await.unwrap();
    assert_eq!((status(&listed, "recipes"), status(&listed, "local")), (McpAuthStatus::NeedsLogin, McpAuthStatus::None));
    assert!(client.mcp_login_start("local").await.is_err(), "stdio servers have no login");
    assert!(client.mcp_login_start("nope").await.is_err());

    // 1. The browser on the server's computer comes back to the daemon's callback.
    let login = client.mcp_login_start("recipes").await.unwrap();
    assert_eq!(login.state, McpLoginState::Pending);
    let q = query(&login.authorization_url);
    assert!(login.authorization_url.starts_with(&format!("{base}/auth/authorize?")));
    assert_eq!((q["client_id"].as_str(), q["code_challenge_method"].as_str(), q["scope"].as_str()), ("client-1", "S256", "recipes"));
    assert_eq!((q["redirect_uri"].as_str(), q["resource"].clone()), (login.redirect_uri.as_str(), format!("{base}/mcp")));
    assert_eq!(fake.lock().unwrap().registered_redirects, vec![login.redirect_uri.clone()]);
    fake.lock().unwrap().codes.insert("one".into(), q["code_challenge"].clone());
    let port = url::Url::parse(&login.redirect_uri).unwrap().port().unwrap();
    let http = reqwest::Client::new();
    let wrong = http.get(format!("http://127.0.0.1:{port}/callback?code=one&state=other")).send().await.unwrap();
    assert_eq!(wrong.status(), 400, "a foreign state leaves the login waiting");
    let page = http.get(format!("http://127.0.0.1:{port}/callback?code=one&state={}", q["state"])).send().await.unwrap();
    assert_eq!(page.status(), 200);
    assert!(page.text().await.unwrap().contains("Logged in"));
    wait_for(&mut events, EventKind::McpLoginFinished, &login.login_id).await;
    wait_for(&mut events, EventKind::McpAuthChanged, "recipes").await;
    assert_eq!(client.mcp_login_get(&login.login_id).await.unwrap().state, McpLoginState::Succeeded);
    let listed = client.mcp_list().await.unwrap();
    assert_eq!(status(&listed, "recipes"), McpAuthStatus::Authorized);
    no_tokens(serde_json::to_value(&listed).unwrap());

    // The tokens are where Claude Code reads them, next to what was there.
    let stored: Value = serde_json::from_slice(&std::fs::read(&credentials).unwrap()).unwrap();
    assert_eq!(stored["claudeAiOauth"]["accessToken"], "keep-me");
    let (key, entry) = stored["mcpOAuth"].as_object().unwrap().iter().next().unwrap();
    assert!(key.starts_with("recipes|") && key.len() == "recipes|".len() + 16, "{key}");
    assert_eq!((entry["accessToken"].as_str(), entry["refreshToken"].as_str()), (Some("AT-SECRET-one"), Some("RT-SECRET")));
    assert_eq!((entry["clientId"].as_str(), entry["redirectUri"].as_str()), (Some("client-1"), Some(login.redirect_uri.as_str())));
    assert_eq!(entry["serverUrl"], json!(format!("{base}/mcp")));
    assert_eq!(entry["discoveryState"]["authorizationServerUrl"], json!(format!("{base}/auth")));
    assert!(entry["expiresAt"].as_i64().unwrap() > aurelius_storage::now_ms());
    let cache: Value = serde_json::from_slice(&std::fs::read(claude_dir.join("mcp-needs-auth-cache.json")).unwrap()).unwrap();
    assert!(cache.get("recipes").is_none(), "Claude Code tries the server again");

    // 2. Logout: the tokens go, the registration stays.
    let after = client.mcp_logout("recipes").await.unwrap();
    assert_eq!(status(&after, "recipes"), McpAuthStatus::NeedsLogin);
    wait_for(&mut events, EventKind::McpAuthChanged, "recipes").await;
    let stored: Value = serde_json::from_slice(&std::fs::read(&credentials).unwrap()).unwrap();
    let entry = stored["mcpOAuth"].as_object().unwrap().values().next().unwrap();
    assert_eq!((entry["accessToken"].as_str(), entry.get("refreshToken"), entry["clientId"].as_str()), (Some(""), None, Some("client-1")));

    // 3. A browser elsewhere: the user pastes the address it ended on.
    let login = client.mcp_login_start("recipes").await.unwrap();
    let q = query(&login.authorization_url);
    fake.lock().unwrap().codes.insert("two".into(), q["code_challenge"].clone());
    let foreign = format!("{}?code=two&state=not-this-one", login.redirect_uri);
    assert!(client.mcp_login_complete(&login.login_id, &foreign).await.is_err());
    assert_eq!(client.mcp_login_get(&login.login_id).await.unwrap().state, McpLoginState::Pending);
    let pasted = format!("{}?code=two&state={}", login.redirect_uri, urlencode(&q["state"]));
    let done = client.mcp_login_complete(&login.login_id, &pasted).await.unwrap();
    assert_eq!(done.state, McpLoginState::Succeeded);
    no_tokens(serde_json::to_value(&done).unwrap());
    assert_eq!(status(&client.mcp_list().await.unwrap(), "recipes"), McpAuthStatus::Authorized);
    assert!(client.mcp_login_complete(&login.login_id, &pasted).await.is_err(), "a login completes once");
    tokio::time::sleep(Duration::from_millis(100)).await;
    let port = url::Url::parse(&login.redirect_uri).unwrap().port().unwrap();
    assert!(http.get(format!("http://127.0.0.1:{port}/callback")).send().await.is_err(), "the callback stopped listening");

    // 4. A refused login ends as failed; a bad code fails at the token request.
    let login = client.mcp_login_start("recipes").await.unwrap();
    let state = query(&login.authorization_url)["state"].clone();
    let refused = format!("{}?error=access_denied&error_description=No+thanks&state={}", login.redirect_uri, urlencode(&state));
    let error = client.mcp_login_complete(&login.login_id, &refused).await.unwrap_err();
    assert!(error.to_string().contains("No thanks"), "{error}");
    let failed = client.mcp_login_get(&login.login_id).await.unwrap();
    assert_eq!(failed.state, McpLoginState::Failed);
    let login = client.mcp_login_start("recipes").await.unwrap();
    let state = query(&login.authorization_url)["state"].clone();
    let bad = format!("{}?code=unknown&state={}", login.redirect_uri, urlencode(&state));
    assert!(client.mcp_login_complete(&login.login_id, &bad).await.unwrap_err().to_string().contains("bad code or verifier"));
    assert_eq!(client.mcp_login_get(&login.login_id).await.unwrap().state, McpLoginState::Failed);
    assert_eq!(status(&client.mcp_list().await.unwrap(), "recipes"), McpAuthStatus::Authorized, "a failed login keeps the earlier tokens");
    assert_eq!(fake.lock().unwrap().token_requests, 3);

    // 5. A newer login replaces a waiting one.
    let first = client.mcp_login_start("recipes").await.unwrap();
    let second = client.mcp_login_start("recipes").await.unwrap();
    assert_eq!(client.mcp_login_get(&first.login_id).await.unwrap().state, McpLoginState::Expired);
    assert_eq!(client.mcp_login_get(&second.login_id).await.unwrap().state, McpLoginState::Pending);

    common::stop(&root, daemon).await;
}

fn urlencode(text: &str) -> String {
    url::form_urlencoded::byte_serialize(text.as_bytes()).collect()
}
