//! A browser against the gateway: cookies instead of headers, only from the gateway's own
//! origin, and the web client's files. One test: it sets the environment for the daemon.

mod common;

use aurelius_client::Client;
use aurelius_protocol::HostWrite;
use common::*;
use futures_util::{SinkExt, StreamExt};
use reqwest::header::{COOKIE, ORIGIN, SET_COOKIE};
use tokio_tungstenite::tungstenite::{Message, client::IntoClientRequest};

fn cookies(response: &reqwest::Response) -> Vec<String> {
    response.headers().get_all(SET_COOKIE).iter().map(|v| v.to_str().unwrap().to_owned()).collect()
}

/// `name=value` of the `Set-Cookie` line of `name`.
fn pair_of(lines: &[String], name: &str) -> String {
    let line = lines.iter().find(|l| l.starts_with(&format!("{name}="))).unwrap_or_else(|| panic!("no {name} in {lines:?}"));
    line.split(';').next().unwrap().to_owned()
}

async fn upgrade(base: &str, cookie: Option<&str>, origin: Option<&str>) -> Result<(), u16> {
    let mut request = format!("{}/v1/ws", base.replace("http://", "ws://")).into_client_request().unwrap();
    if let Some(cookie) = cookie {
        request.headers_mut().insert("cookie", cookie.parse().unwrap());
    }
    if let Some(origin) = origin {
        request.headers_mut().insert("origin", origin.parse().unwrap());
    }
    match tokio_tungstenite::connect_async(request).await {
        Ok((mut socket, _)) => {
            let hello = serde_json::json!({ "protocol": aurelius_protocol::PROTOCOL_MAJOR, "request_id": 1, "method": "user.whoami" });
            socket.send(Message::Text(hello.to_string().into())).await.unwrap();
            loop {
                let Message::Text(text) = socket.next().await.unwrap().unwrap() else { continue };
                let message: serde_json::Value = serde_json::from_str(text.as_str()).unwrap();
                if message["type"] == "response" {
                    assert_eq!(message["request_id"], 1);
                    return Ok(());
                }
            }
        }
        Err(tokio_tungstenite::tungstenite::Error::Http(response)) => Err(response.status().as_u16()),
        Err(other) => panic!("unexpected: {other}"),
    }
}

async fn pair_request(http: &reqwest::Client, base: &str, code: &str, origin: Option<&str>, role: Option<&str>) -> reqwest::Response {
    let mut request = http.post(format!("{base}/v1/pair")).json(&serde_json::json!({ "code": code, "device_name": "browser", "platform": "web", "role": role }));
    if let Some(origin) = origin {
        request = request.header(ORIGIN, origin);
    }
    request.send().await.unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_browser_pairs_and_connects_with_cookies_from_its_own_origin_only() {
    let root = tempfile::tempdir().unwrap();
    let site = tempfile::tempdir().unwrap();
    std::fs::write(site.path().join("index.html"), "<!doctype html><title>Aurelius</title>").unwrap();
    std::fs::write(site.path().join("app.js"), "// app").unwrap();
    std::fs::write(root.path().join("secret.txt"), "not for the web").unwrap();
    // SAFETY (test): the only test in this binary, before the daemon starts.
    unsafe {
        std::env::set_var("AURELIUS_REMOTE_PORT", "0");
        std::env::set_var("AURELIUS_ENV", "dev");
        std::env::set_var("AURELIUS_WEB_DIR", site.path());
    }
    let _daemon = start_with_delay(root.path(), 5).await;
    let desktop: Client = connect(root.path()).await;
    let status = desktop.remote(HostWrite::RemoteEnable).await.unwrap();
    let base = format!("http://127.0.0.1:{}", status.port);
    let own = base.as_str();
    let evil = "https://evil.example";
    let http = reqwest::Client::new();

    // The web client's files, with their security headers; a path of the app gets its page.
    let page = http.get(&base).send().await.unwrap();
    assert_eq!(page.status(), 200);
    assert!(page.headers()["content-security-policy"].to_str().unwrap().contains("default-src 'self'"));
    assert!(page.text().await.unwrap().contains("<title>Aurelius"));
    assert_eq!(http.get(format!("{base}/app.js")).send().await.unwrap().text().await.unwrap(), "// app");
    assert!(http.get(format!("{base}/board/21")).send().await.unwrap().text().await.unwrap().contains("<title>Aurelius"));
    // An unknown API route stays an API error, and nothing outside the folder is served.
    assert_eq!(http.get(format!("{base}/v1/nope")).send().await.unwrap().status(), 404);
    let escape = http.get(format!("{base}/%2e%2e/{}/secret.txt", root.path().file_name().unwrap().to_str().unwrap())).send().await.unwrap();
    assert!(!escape.text().await.unwrap().contains("not for the web"));

    // Pairing: only from its own origin, and never as owner.
    let code = desktop.pairing_start().await.unwrap().code;
    assert_eq!(pair_request(&http, &base, &code, Some(evil), None).await.status(), 403);
    assert_eq!(pair_request(&http, &base, &code, None, None).await.status(), 403);
    assert_eq!(pair_request(&http, &base, &code, Some(own), Some("owner")).await.status(), 400);
    let pending: serde_json::Value = pair_request(&http, &base, &code, Some(own), None).await.json().await.unwrap();
    let pairing_id = pending["pairing_id"].as_str().unwrap().to_owned();
    desktop.remote(HostWrite::RemoteDecidePairing { pairing_id: pairing_id.clone(), approve: true, owner: false }).await.unwrap();

    // The secret reaches the browser as an HttpOnly cookie only.
    let approved = http.get(format!("{base}/v1/pair/{pairing_id}")).send().await.unwrap();
    let lines = cookies(&approved);
    let body: serde_json::Value = approved.json().await.unwrap();
    assert_eq!(body["status"], "approved");
    assert!(body.get("device_secret").is_none(), "{body}");
    assert!(lines[0].contains("HttpOnly") && lines[0].contains("Secure") && lines[0].contains("SameSite=Strict") && lines[0].contains("Path=/v1/session"), "{lines:?}");
    let device = pair_of(&lines, "aurelius_device");

    // A session: only with the device cookie from its own origin, and again only as a cookie.
    let session = async |origin: Option<&str>, cookie: &str| {
        let mut request = http.post(format!("{base}/v1/session")).header(COOKIE, cookie);
        if let Some(origin) = origin {
            request = request.header(ORIGIN, origin);
        }
        request.send().await.unwrap()
    };
    assert_eq!(session(None, &device).await.status(), 403);
    assert_eq!(session(Some(evil), &device).await.status(), 403);
    assert_eq!(session(Some(own), "aurelius_device=nope").await.status(), 401);
    let granted = session(Some(own), &device).await;
    assert_eq!(granted.status(), 200);
    let lines = cookies(&granted);
    let body: serde_json::Value = granted.json().await.unwrap();
    assert!(body.get("token").is_none(), "{body}");
    assert!(lines[0].contains("HttpOnly") && lines[0].contains("Path=/v1;"), "{lines:?}");
    let cookie = pair_of(&lines, "aurelius_session");

    // The WebSocket: the cookie alone is not enough.
    assert_eq!(upgrade(&base, None, Some(own)).await, Err(401));
    assert_eq!(upgrade(&base, Some(&cookie), None).await, Err(403));
    assert_eq!(upgrade(&base, Some(&cookie), Some(evil)).await, Err(403));
    assert_eq!(upgrade(&base, Some(&cookie), Some(own)).await, Ok(()));

    // Upload takes the same cookie, under the same rule.
    let form = || reqwest::multipart::Form::new().part("file", reqwest::multipart::Part::bytes(vec![1, 2, 3]).file_name("a.bin"));
    let refused = http.post(format!("{base}/v1/upload")).header(COOKIE, &cookie).header(ORIGIN, evil).multipart(form()).send().await.unwrap();
    assert_eq!(refused.status(), 403);
    let uploaded = http.post(format!("{base}/v1/upload")).header(COOKIE, &cookie).header(ORIGIN, own).multipart(form()).send().await.unwrap();
    assert_eq!(uploaded.status(), 200);

    // Signing this browser out clears both cookies, from its own origin only.
    assert_eq!(http.delete(format!("{base}/v1/session")).header(ORIGIN, evil).send().await.unwrap().status(), 403);
    let out = http.delete(format!("{base}/v1/session")).header(ORIGIN, own).send().await.unwrap();
    assert_eq!(out.status(), 204);
    let lines = cookies(&out);
    assert_eq!(lines.len(), 2);
    assert!(lines.iter().all(|l| l.contains("Max-Age=0")), "{lines:?}");
}
