//! A fake remote MCP server behind OAuth, on localhost: `/mcp` answers 401 with
//! a `WWW-Authenticate` pointing at its protected-resource metadata until a
//! token it issued comes along, then speaks enough MCP (initialize, tools/list)
//! for Claude Code to connect. `/auth` is its authorization server: metadata,
//! dynamic registration and a token endpoint that checks PKCE.

use base64::Engine as _;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// What the fake knows: code → PKCE challenge, the registrations, the tokens it issued.
#[derive(Default)]
pub struct Fake {
    pub codes: HashMap<String, String>,
    pub registered_redirects: Vec<String>,
    pub token_requests: usize,
    pub issued: Vec<String>,
    pub tool_lists: usize,
    /// `METHOD /path auth=<bearer or ->` of every request.
    pub log: Vec<String>,
}

struct Request {
    method: String,
    target: String,
    headers: HashMap<String, String>,
    body: String,
}

async fn read_request(stream: &mut TcpStream) -> Option<Request> {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        let n = stream.read(&mut chunk).await.ok()?;
        if n == 0 {
            return None;
        }
        buffer.extend_from_slice(&chunk[..n]);
        if let Some(at) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
            break at + 4;
        }
    };
    let head = String::from_utf8_lossy(&buffer[..head_end]).to_string();
    let headers: HashMap<String, String> =
        head.lines().skip(1).filter_map(|l| l.split_once(':')).map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string())).collect();
    let length = headers.get("content-length").and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
    while buffer.len() < head_end + length {
        let n = stream.read(&mut chunk).await.ok()?;
        if n == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..n]);
    }
    let mut first = head.lines().next()?.split_whitespace();
    let (method, target) = (first.next()?.to_string(), first.next()?.to_string());
    Some(Request { method, target, headers, body: String::from_utf8_lossy(&buffer[head_end..]).to_string() })
}

async fn answer(stream: &mut TcpStream, status: &str, extra: &str, body: Option<&Value>) {
    let body = body.map(Value::to_string).unwrap_or_default();
    let response = format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.shutdown().await;
}

/// The MCP endpoint once the caller has a token.
fn mcp_answer(state: &Mutex<Fake>, body: &str) -> Option<Value> {
    let call: Value = serde_json::from_str(body).ok()?;
    let id = call.get("id").cloned().filter(|i| !i.is_null())?;
    let result = match call["method"].as_str()? {
        "initialize" => json!({ "protocolVersion": call["params"]["protocolVersion"].as_str().unwrap_or("2025-06-18"), "capabilities": { "tools": {} }, "serverInfo": { "name": "fake-recipes", "version": "1.0.0" } }),
        "tools/list" => {
            state.lock().unwrap().tool_lists += 1;
            json!({ "tools": [{ "name": "get_recipe", "description": "A recipe by name.", "inputSchema": { "type": "object", "properties": { "name": { "type": "string" } } } }] })
        }
        _ => json!({}),
    };
    Some(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
}

/// Start the fake; answers its base URL (`http://127.0.0.1:<port>`).
pub async fn start(state: Arc<Mutex<Fake>>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let b = base.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else { return };
            let (state, base) = (state.clone(), b.clone());
            tokio::spawn(async move {
                let Some(request) = read_request(&mut stream).await else { return };
                let path = request.target.split('?').next().unwrap_or("").to_string();
                let bearer = request.headers.get("authorization").cloned().unwrap_or_else(|| "-".into());
                state.lock().unwrap().log.push(format!("{} {} auth={bearer}", request.method, request.target));
                match (request.method.as_str(), path.as_str()) {
                    (_, "/mcp") => {
                        let bearer = request.headers.get("authorization").and_then(|a| a.strip_prefix("Bearer ")).map(str::to_owned);
                        let known = bearer.is_some_and(|t| state.lock().unwrap().issued.contains(&t));
                        if !known {
                            let challenge = format!("WWW-Authenticate: Bearer resource_metadata=\"{base}/.well-known/oauth-protected-resource/mcp\"\r\n");
                            answer(&mut stream, "401 Unauthorized", &challenge, Some(&json!({ "error": "unauthorized" }))).await;
                        } else if request.method != "POST" {
                            answer(&mut stream, "405 Method Not Allowed", "", None).await;
                        } else {
                            match mcp_answer(&state, &request.body) {
                                Some(reply) => answer(&mut stream, "200 OK", "", Some(&reply)).await,
                                None => answer(&mut stream, "202 Accepted", "", None).await,
                            }
                        }
                    }
                    ("GET", "/.well-known/oauth-protected-resource/mcp") => {
                        let doc = json!({ "resource": format!("{base}/mcp"), "authorization_servers": [format!("{base}/auth")], "scopes_supported": ["recipes"] });
                        answer(&mut stream, "200 OK", "", Some(&doc)).await;
                    }
                    ("GET", "/.well-known/oauth-authorization-server/auth") => {
                        let doc = json!({
                            "issuer": format!("{base}/auth"),
                            "authorization_endpoint": format!("{base}/auth/authorize"),
                            "token_endpoint": format!("{base}/auth/token"),
                            "registration_endpoint": format!("{base}/auth/register"),
                            "response_types_supported": ["code"],
                            "grant_types_supported": ["authorization_code", "refresh_token"],
                            "code_challenge_methods_supported": ["S256"],
                            "token_endpoint_auth_methods_supported": ["none"],
                        });
                        answer(&mut stream, "200 OK", "", Some(&doc)).await;
                    }
                    ("POST", "/auth/register") => {
                        let registration: Value = serde_json::from_str(&request.body).unwrap_or_default();
                        assert_eq!(registration["token_endpoint_auth_method"], "none");
                        state.lock().unwrap().registered_redirects.push(registration["redirect_uris"][0].as_str().unwrap_or("").to_string());
                        answer(&mut stream, "201 Created", "", Some(&json!({ "client_id": "client-1", "redirect_uris": registration["redirect_uris"] }))).await;
                    }
                    ("POST", "/auth/token") => {
                        let form: HashMap<String, String> = url::form_urlencoded::parse(request.body.as_bytes()).into_owned().collect();
                        let code = form.get("code").cloned().unwrap_or_default();
                        let verifier = form.get("code_verifier").cloned().unwrap_or_default();
                        let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
                        let known = {
                            let mut fake = state.lock().unwrap();
                            fake.token_requests += 1;
                            fake.codes.get(&code) == Some(&challenge)
                        };
                        let ok = known
                            && form.get("grant_type").map(String::as_str) == Some("authorization_code")
                            && form.get("client_id").map(String::as_str) == Some("client-1")
                            && form.get("resource").map(String::as_str) == Some(format!("{base}/mcp").as_str());
                        if ok {
                            let token = format!("AT-SECRET-{code}");
                            state.lock().unwrap().issued.push(token.clone());
                            let tokens = json!({ "access_token": token, "refresh_token": "RT-SECRET", "expires_in": 3600, "token_type": "Bearer", "scope": "recipes" });
                            answer(&mut stream, "200 OK", "", Some(&tokens)).await;
                        } else {
                            answer(&mut stream, "400 Bad Request", "", Some(&json!({ "error": "invalid_grant", "error_description": "bad code or verifier" }))).await;
                        }
                    }
                    _ => answer(&mut stream, "404 Not Found", "", Some(&json!({}))).await,
                }
            });
        }
    });
    base
}
