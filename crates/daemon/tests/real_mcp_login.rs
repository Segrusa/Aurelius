//! Live MCP OAuth logins, ignored by default; run one at a time (they set the
//! process environment).
//!
//! `log_in_to_a_real_mcp_server`: one of the user's real remote MCP servers,
//! through a daemon on a scratch data folder with the user's own Claude Code
//! files. Opens the authorization page in the default browser and waits up to
//! five minutes for the user to finish it; it writes the real
//! `~/.claude/.credentials.json`.
//! Run with: AURELIUS_LIVE_MCP=<server name> cargo test -p aurelius-daemon --test real_mcp_login log_in_to_a_real -- --ignored --nocapture
//!
//! `claude_code_uses_tokens_from_an_aurelius_login`: the real Claude Code CLI
//! (`claude mcp list`, a scratch `CLAUDE_CONFIG_DIR`, no Claude login needed)
//! sees a fake OAuth server as needing authentication, and connected after a
//! login through the daemon.
//! Run with: cargo test -p aurelius-daemon --test real_mcp_login claude_code_uses -- --ignored --nocapture

mod common;

use aurelius_protocol::{McpAuthStatus, McpLoginState};
use common::fake_mcp::{self, Fake};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn log_in_to_a_real_mcp_server() {
    let name = std::env::var("AURELIUS_LIVE_MCP").expect("AURELIUS_LIVE_MCP names the server");
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("data");
    let daemon = common::start(&root).await;
    let client = common::connect(&root).await;

    let before = client.mcp_list().await.unwrap();
    let auth = before.servers.iter().find(|s| s.name == name).expect("a user-scope server").auth;
    println!("{name} before: {auth:?}");

    let login = client.mcp_login_start(&name).await.unwrap();
    println!("redirect_uri: {}", login.redirect_uri);
    println!("open: {}", login.authorization_url);
    #[cfg(windows)]
    let _ = std::process::Command::new("rundll32").args(["url.dll,FileProtocolHandler", &login.authorization_url]).spawn();

    let mut state = login.state;
    for _ in 0..300 {
        state = client.mcp_login_get(&login.login_id).await.unwrap().state;
        if state != McpLoginState::Pending {
            break;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    let ended = client.mcp_login_get(&login.login_id).await.unwrap();
    println!("login: {:?} {:?}", ended.state, ended.error);
    assert_eq!(state, McpLoginState::Succeeded);
    let after = client.mcp_list().await.unwrap();
    assert_eq!(after.servers.iter().find(|s| s.name == name).unwrap().auth, McpAuthStatus::Authorized);
    common::stop(&root, daemon).await;
}

async fn claude_mcp_list(config: &std::path::Path) -> String {
    let claude = aurelius_runtime::claude_code::locate().expect("Claude Code is installed");
    let output = tokio::process::Command::new(claude).args(["mcp", "list"]).env("CLAUDE_CONFIG_DIR", config).output().await.unwrap();
    format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr))
}

fn line_of<'a>(listing: &'a str, name: &str) -> &'a str {
    listing.lines().find(|l| l.starts_with(&format!("{name}:"))).unwrap_or_else(|| panic!("{name} not listed:
{listing}"))
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn claude_code_uses_tokens_from_an_aurelius_login() {
    let fake = Arc::new(Mutex::new(Fake::default()));
    let base = fake_mcp::start(fake.clone()).await;
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("claude");
    std::fs::create_dir_all(&config).unwrap();
    let claude_json = config.join(".claude.json");
    let servers = serde_json::json!({ "mcpServers": { "fake-recipes": { "type": "http", "url": format!("{base}/mcp") } } });
    std::fs::write(&claude_json, servers.to_string()).unwrap();
    // SAFETY of the test: set before the daemon starts; run this test on its own.
    unsafe {
        std::env::set_var("CLAUDE_CONFIG_DIR", &config);
        std::env::set_var("AURELIUS_CLAUDE_JSON", &claude_json);
    }

    let before = claude_mcp_list(&config).await;
    println!("claude mcp list before:
{before}");
    assert!(line_of(&before, "fake-recipes").contains("Needs authentication"));

    let root = dir.path().join("data");
    let daemon = common::start(&root).await;
    let client = common::connect(&root).await;
    let auth = |l: &aurelius_protocol::McpServers| l.servers.iter().find(|s| s.name == "fake-recipes").unwrap().auth;
    println!("Aurelius status before: {:?}", auth(&client.mcp_list().await.unwrap()));

    // The browser on this computer: the authorization server sends it to the daemon's callback.
    let login = client.mcp_login_start("fake-recipes").await.unwrap();
    let q: HashMap<String, String> = url::Url::parse(&login.authorization_url).unwrap().query_pairs().into_owned().collect();
    fake.lock().unwrap().codes.insert("live".into(), q["code_challenge"].clone());
    let port = url::Url::parse(&login.redirect_uri).unwrap().port().unwrap();
    let page = reqwest::get(format!("http://127.0.0.1:{port}/callback?code=live&state={}", q["state"])).await.unwrap();
    assert_eq!(page.status(), 200);
    assert_eq!(client.mcp_login_get(&login.login_id).await.unwrap().state, McpLoginState::Succeeded);
    assert_eq!(auth(&client.mcp_list().await.unwrap()), McpAuthStatus::Authorized);

    let after = claude_mcp_list(&config).await;
    println!("claude mcp list after:
{after}");
    for line in &fake.lock().unwrap().log {
        println!("fake: {line}");
    }
    for entry in std::fs::read_dir(&config).unwrap().flatten() {
        let text = std::fs::read_to_string(entry.path()).unwrap_or_default();
        println!("--- {}
{}", entry.file_name().to_string_lossy(), text.chars().take(1500).collect::<String>());
    }
    assert!(line_of(&after, "fake-recipes").contains("Connected"), "Claude Code connects with the token Aurelius stored");
    common::stop(&root, daemon).await;
}
