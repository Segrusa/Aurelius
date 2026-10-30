//! The webhook contract (feature `webhooks`), against a test receiver: a local
//! HTTP listener that records every request and answers as scripted.
//!
//! * every delivery is signed (HMAC-SHA256 over `timestamp.delivery_id.body`, the
//!   timestamp within the tolerance) and carries the headers of the contract;
//! * a failed attempt is retried with backoff, with the same delivery id, a new
//!   timestamp and a higher attempt number; any 2xx ends it;
//! * the outbox is persistent: a restart loses nothing;
//! * events go only to the app they concern; the secret is shown once and never listed.

mod common;

use aurelius_client::{Client, envelope};
use aurelius_domain::{
    Command, ConversationId, EmployeeId, ProjectId,
    app_key::{AppAudience, AppScope, CreateAppKey},
    employee::{CreateEmployee, SendEmployeeMessage},
    group_chat::SendGroupMessage,
    project::CreateProject,
    task::{CreateTask, RunSettings},
    webhook,
};
use aurelius_protocol::CommandResult;
use common::*;
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

fn env() {
    // SAFETY (test): every test in this binary sets the same values before its daemon starts.
    unsafe {
        std::env::set_var("AURELIUS_REMOTE_PORT", "0");
        std::env::set_var("AURELIUS_NO_TAILSCALE", "1");
        std::env::set_var("AURELIUS_ENV", "dev");
        // The wait before the second attempt; it doubles per failure (the contract says 5 s).
        std::env::set_var("AURELIUS_WEBHOOK_BACKOFF_MS", "150");
    }
}

#[derive(Debug, Clone)]
struct Received {
    path: String,
    headers: HashMap<String, String>,
    body: String,
}

/// A receiver on a free local port. It answers 500 to the first `fail` requests for the
/// event `fail_for`, then 200; other events always get 200.
struct Receiver {
    url: String,
    seen: Arc<Mutex<Vec<Received>>>,
    fail: Arc<AtomicUsize>,
}

impl Receiver {
    async fn start(fail: usize, fail_for: &'static str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://127.0.0.1:{}/hooks/aurelius", listener.local_addr().unwrap().port());
        let (seen, fail) = (Arc::new(Mutex::new(Vec::new())), Arc::new(AtomicUsize::new(fail)));
        let (seen_in, fail_in) = (seen.clone(), fail.clone());
        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else { break };
                let (seen, fail) = (seen_in.clone(), fail_in.clone());
                tokio::spawn(async move {
                    let mut buffer = Vec::new();
                    let mut chunk = [0u8; 4096];
                    let (head_end, length) = loop {
                        let n = match stream.read(&mut chunk).await {
                            Ok(0) | Err(_) => return,
                            Ok(n) => n,
                        };
                        buffer.extend_from_slice(&chunk[..n]);
                        if let Some(at) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
                            let head = String::from_utf8_lossy(&buffer[..at]).to_lowercase();
                            let length = head.lines().find_map(|l| l.strip_prefix("content-length:")).and_then(|v| v.trim().parse::<usize>().ok()).unwrap_or(0);
                            break (at + 4, length);
                        }
                    };
                    while buffer.len() < head_end + length {
                        match stream.read(&mut chunk).await {
                            Ok(0) | Err(_) => return,
                            Ok(n) => buffer.extend_from_slice(&chunk[..n]),
                        }
                    }
                    let head = String::from_utf8_lossy(&buffer[..head_end]).to_string();
                    let mut lines = head.lines();
                    let path = lines.next().unwrap_or("").split_whitespace().nth(1).unwrap_or("").to_owned();
                    let headers: HashMap<String, String> = lines.filter_map(|l| l.split_once(':')).map(|(k, v)| (k.trim().to_lowercase(), v.trim().to_owned())).collect();
                    let body = String::from_utf8_lossy(&buffer[head_end..head_end + length]).to_string();
                    let wanted = headers.get("x-aurelius-event").map(String::as_str) == Some(fail_for);
                    seen.lock().unwrap().push(Received { path, headers, body });
                    let failing = wanted && fail.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1)).is_ok();
                    let status = if failing { "500 Internal Server Error" } else { "200 OK" };
                    let _ = stream.write_all(format!("HTTP/1.1 {status}\r\ncontent-length: 0\r\nconnection: close\r\n\r\n").as_bytes()).await;
                    let _ = stream.shutdown().await;
                });
            }
        });
        Self { url, seen, fail }
    }

    fn requests(&self) -> Vec<Received> {
        self.seen.lock().unwrap().clone()
    }

    /// Wait until a delivery of `event` was answered with 2xx (it is the request after the failures).
    async fn delivery_of(&self, event: &str, min_requests: usize) -> Vec<Received> {
        for _ in 0..800 {
            let of_event: Vec<Received> = self.requests().into_iter().filter(|r| r.headers.get("x-aurelius-event").map(String::as_str) == Some(event)).collect();
            if of_event.len() >= min_requests {
                return of_event;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!("no {event} delivery with {min_requests} requests; got {:?}", self.requests().iter().map(|r| r.headers.get("x-aurelius-event").cloned()).collect::<Vec<_>>());
    }
}

struct Company {
    project: ProjectId,
    jean: (EmployeeId, ConversationId),
}

async fn company(owner: &Client) -> Company {
    let CommandResult::ProjectCreated { project_id, .. } =
        owner.execute(envelope(Command::CreateProject(CreateProject { name: "Atlas".into(), description: None, folder: None }), None)).await.unwrap().result
    else {
        panic!()
    };
    let jean = CreateEmployee { name: "Jean".into(), role: "Management".into(), title: Some("CEO".into()), reports_to: None, instructions: None, folder: None, department_id: None };
    let CommandResult::EmployeeCreated { employee_id, conversation_id } = owner.execute(envelope(Command::CreateEmployee(jean), None)).await.unwrap().result else { panic!() };
    Company { project: project_id, jean: (employee_id, conversation_id) }
}

async fn key_with_webhook(owner: &Client, name: &str, audience: AppAudience, url: &str) -> (aurelius_domain::AppKeyId, String, String) {
    let created = owner
        .create_app_key(CreateAppKey { name: name.into(), scopes: vec![AppScope::Read, AppScope::Run], limit_per_hour: None, limit_concurrent: None, audience, webhook_url: Some(url.into()) })
        .await
        .unwrap();
    let hook_secret = created.webhook_secret.clone().expect("the webhook secret is shown at create");
    (created.key.id, created.secret, hook_secret)
}

async fn app(root: &Path, secret: &str) -> Client {
    Client::connect_app(&aurelius_host::endpoint(root), secret, "marcus").await.expect("app session")
}

fn task(project: ProjectId, employee: EmployeeId, title: &str) -> Command {
    Command::CreateTask(CreateTask { project_id: project, employee_id: Some(employee), title: title.into(), goal: "Do it".into(), acceptance_criteria: vec![], depends_on: vec![], run: RunSettings::default(), files: vec![] })
}

fn now_secs() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64
}

/// What the contract promises about one request.
fn check_contract(request: &Received, secret: &str, event: &str) -> Value {
    assert_eq!(request.path, "/hooks/aurelius");
    let header = |name: &str| request.headers.get(name).unwrap_or_else(|| panic!("header {name} in {:?}", request.headers)).clone();
    assert_eq!(header("content-type"), "application/json");
    assert_eq!(header("x-aurelius-event"), event);
    let (id, timestamp, signature) = (header("x-aurelius-delivery"), header("x-aurelius-timestamp").parse::<i64>().expect("a unix timestamp"), header("x-aurelius-signature"));
    assert!(id.starts_with("dlv_"), "{id}");
    assert!(signature.starts_with("v1="), "{signature}");
    assert!(webhook::verify(secret, &signature, timestamp, &id, &request.body, now_secs()), "the signature must verify over `{{timestamp}}.{{delivery_id}}.{{body}}`: {request:?}");
    assert!(!webhook::verify("whsec_not_the_secret", &signature, timestamp, &id, &request.body, now_secs()), "and only with the secret");
    let body: Value = serde_json::from_str(&request.body).expect("a JSON body");
    assert_eq!(body["id"], id.as_str(), "the body names the delivery id too");
    assert_eq!(body["type"], event);
    assert!(body["created_at_ms"].is_i64() && body["app"]["id"].is_string() && body["app"]["name"].is_string() && body["data"].is_object(), "{body}");
    body
}

#[tokio::test(flavor = "multi_thread")]
async fn a_finished_task_is_posted_signed_and_retried_with_one_delivery_id() {
    env();
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let owner = connect(dir.path()).await;
    let c = company(&owner).await;
    // The first two attempts fail (HTTP 500): the third succeeds.
    let receiver = Receiver::start(2, "task.finished").await;
    let (key_id, secret, hook_secret) = key_with_webhook(&owner, "Marcus", AppAudience::Management, &receiver.url).await;
    assert!(hook_secret.starts_with("whsec_") && hook_secret.len() == 6 + 64);
    // Another app with its own receiver gets nothing of Marcus' work.
    let other = Receiver::start(0, "none").await;
    key_with_webhook(&owner, "Other", AppAudience::All, &other.url).await;

    // The secret is shown once: not in the list, not in the receipt; the list names the URL.
    let listed = serde_json::to_string(&owner.app_keys().await.unwrap()).unwrap();
    assert!(!listed.contains(&hook_secret) && listed.contains(&receiver.url), "{listed}");
    let conn = db(dir.path());
    let receipts: Vec<String> = conn.prepare("SELECT result_json FROM command_receipts WHERE method = 'app_key.create'").unwrap().query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
    assert!(receipts.iter().all(|r| !r.contains(&hook_secret)), "{receipts:?}");

    let marcus = app(dir.path(), &secret).await;
    marcus.execute(envelope(task(c.project, c.jean.0, "Plan the launch"), None)).await.unwrap();

    // 1 + 2 retries = 3 requests of the same delivery.
    let requests = receiver.delivery_of("task.finished", 3).await;
    let ids: Vec<&String> = requests.iter().map(|r| &r.headers["x-aurelius-delivery"]).collect();
    assert!(ids.iter().all(|id| *id == ids[0]), "the delivery id is stable over retries: {ids:?}");
    let attempts: Vec<&str> = requests.iter().map(|r| r.headers["x-aurelius-attempt"].as_str()).collect();
    assert_eq!(&attempts[..3], ["1", "2", "3"]);
    let mut timestamps = Vec::new();
    for request in &requests {
        let body = check_contract(request, &hook_secret, "task.finished");
        assert_eq!(body["app"]["name"], "Marcus");
        assert_eq!(body["data"]["status"], "succeeded", "{body}");
        assert_eq!((body["data"]["title"].as_str(), body["data"]["employee"].as_str(), body["data"]["created_by_this_app"].as_bool()), (Some("Plan the launch"), Some("Jean"), Some(true)));
        assert_eq!(request.body, requests[0].body, "the body is the same on every attempt");
        timestamps.push(request.headers["x-aurelius-timestamp"].parse::<i64>().unwrap());
    }
    assert!(timestamps.windows(2).all(|w| w[0] <= w[1]));
    // Delivered after the third attempt: nothing more arrives.
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert_eq!(receiver.delivery_of("task.finished", 1).await.len(), 3, "a 2xx ends the delivery");
    let row: (i64, Option<i64>, Option<i64>) = conn.query_row("SELECT attempts, delivered_at_ms, last_status FROM webhook_outbox WHERE id = ?1", [ids[0]], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap();
    assert_eq!((row.0, row.1.is_some(), row.2), (3, true, Some(200)));
    assert!(row.0 as u32 <= webhook::MAX_ATTEMPTS);
    assert!(other.requests().is_empty(), "events go to the app they concern only");
    let _ = key_id;
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_managers_reply_is_posted_to_the_app_that_asked() {
    env();
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let owner = connect(dir.path()).await;
    let c = company(&owner).await;
    let receiver = Receiver::start(0, "none").await;
    let (_, secret, hook_secret) = key_with_webhook(&owner, "Marcus", AppAudience::Management, &receiver.url).await;
    let other = Receiver::start(0, "none").await;
    key_with_webhook(&owner, "Other", AppAudience::Management, &other.url).await;
    let marcus = app(dir.path(), &secret).await;

    // The user's own chat with Jean is not the app's: its replies are not posted.
    owner.execute(envelope(Command::SendEmployeeMessage(SendEmployeeMessage { employee_id: c.jean.0, text: "A private word".into(), conversation_id: c.jean.1, images: vec![], files: vec![], interrupt: false }), None)).await.unwrap();
    tokio::time::sleep(Duration::from_millis(800)).await;
    assert!(receiver.requests().is_empty(), "{:?}", receiver.requests());

    marcus
        .execute(envelope(Command::SendEmployeeMessage(SendEmployeeMessage { employee_id: c.jean.0, text: "Question from the app".into(), conversation_id: c.jean.1, images: vec![], files: vec![], interrupt: false }), None))
        .await
        .unwrap();
    let requests = receiver.delivery_of("manager.replied", 1).await;
    let body = check_contract(&requests[0], &hook_secret, "manager.replied");
    assert_eq!((body["data"]["employee"].as_str(), body["data"]["conversation_id"].as_str()), (Some("Jean"), Some(c.jean.1.to_string().as_str())));
    assert!(body["data"]["text"].as_str().is_some_and(|t| !t.is_empty()) && body["data"]["message_id"].is_string(), "{body}");
    // One reply, once; the other management app never asked.
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(receiver.delivery_of("manager.replied", 1).await.len(), 1);
    assert!(other.requests().is_empty());
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_group_chat_answer_of_the_ceo_is_not_the_apps_reply() {
    env();
    let dir = tempfile::tempdir().unwrap();
    // Slow answers: the group chat's run is still busy in the CEO's main chat when the app asks there.
    let daemon = start_with_delay(dir.path(), 30).await;
    let owner = connect(dir.path()).await;
    let c = company(&owner).await;
    let receiver = Receiver::start(0, "none").await;
    let (_, secret, _) = key_with_webhook(&owner, "Marcus", AppAudience::Management, &receiver.url).await;
    let marcus = app(dir.path(), &secret).await;

    owner.execute(envelope(Command::SendGroupMessage(SendGroupMessage { text: "Hallo team".into(), images: vec![], files: vec![] }), None)).await.unwrap();
    marcus
        .execute(envelope(Command::SendEmployeeMessage(SendEmployeeMessage { employee_id: c.jean.0, text: "Question from the app".into(), conversation_id: c.jean.1, images: vec![], files: vec![], interrupt: false }), None))
        .await
        .unwrap();
    // The group's answer comes first, the app's reply after it: only that one is posted.
    let requests = receiver.delivery_of("manager.replied", 1).await;
    let body: Value = serde_json::from_str(&requests[0].body).unwrap();
    assert!(body["data"]["text"].as_str().is_some_and(|t| t.contains("Question from the app")), "{body}");
    let message = body["data"]["message_id"].as_str().unwrap().to_owned();
    let group: Option<String> = db(dir.path()).query_row("SELECT json_extract(j.payload_json, '$.group') FROM messages m JOIN jobs j ON j.id = m.run_id WHERE m.id = ?1", [&message], |r| r.get(0)).unwrap();
    assert!(group.is_none(), "the reply is not the group chat's run");
    assert!(owner.group_chat().await.unwrap().messages.iter().any(|m| m.author.as_deref() == Some("Jean")), "the CEO did answer the group");
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(receiver.delivery_of("manager.replied", 1).await.len(), 1);
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_restart_loses_no_delivery() {
    env();
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let owner = connect(dir.path()).await;
    let c = company(&owner).await;
    // The receiver refuses everything at first.
    let receiver = Receiver::start(usize::MAX, "task.finished").await;
    let (_, secret, hook_secret) = key_with_webhook(&owner, "Marcus", AppAudience::Management, &receiver.url).await;
    let marcus = app(dir.path(), &secret).await;
    marcus.execute(envelope(task(c.project, c.jean.0, "Survive a restart"), None)).await.unwrap();
    let first = receiver.delivery_of("task.finished", 1).await;
    let id = first[0].headers["x-aurelius-delivery"].clone();
    drop((owner, marcus));
    stop(dir.path(), daemon).await;
    let waiting: (i64, Option<i64>, Option<i64>) = db(dir.path())
        .query_row("SELECT attempts, delivered_at_ms, gave_up_at_ms FROM webhook_outbox WHERE id = ?1", [&id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap();
    assert!(waiting.0 >= 1 && waiting.1.is_none() && waiting.2.is_none(), "still waiting in the outbox: {waiting:?}");

    // The receiver recovers while the server is down; after the restart the delivery arrives, with the same id.
    receiver.fail.store(0, Ordering::SeqCst);
    let before = receiver.requests().len();
    let daemon = start(dir.path()).await;
    let _owner = connect(dir.path()).await;
    for _ in 0..800 {
        if receiver.requests().len() > before {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let after = receiver.requests();
    let delivered = after.last().expect("a request after the restart");
    assert!(after.len() > before, "the outbox survived the restart");
    assert_eq!(delivered.headers["x-aurelius-delivery"], id);
    check_contract(delivered, &hook_secret, "task.finished");
    for _ in 0..200 {
        let done: Option<i64> = db(dir.path()).query_row("SELECT delivered_at_ms FROM webhook_outbox WHERE id = ?1", [&id], |r| r.get(0)).unwrap();
        if done.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let done: Option<i64> = db(dir.path()).query_row("SELECT delivered_at_ms FROM webhook_outbox WHERE id = ?1", [&id], |r| r.get(0)).unwrap();
    assert!(done.is_some());
    stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn webhook_urls_are_validated_and_plain_http_is_fine() {
    env();
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let owner = connect(dir.path()).await;
    let make = |url: &str| CreateAppKey { name: "Marcus".into(), scopes: vec![AppScope::Read], limit_per_hour: None, limit_concurrent: None, audience: AppAudience::All, webhook_url: Some(url.into()) };
    for bad in ["ftp://home-server/x", "javascript:alert(1)", "http://user:pw@home-server/x", "not a url", "http://home-server:99999/x"] {
        assert!(owner.create_app_key(make(bad)).await.is_err(), "{bad}");
    }
    for good in ["http://home-server:8787", "https://marcus.example.com/hooks/aurelius"] {
        let created = owner.create_app_key(make(good)).await.unwrap();
        assert_eq!(created.key.webhook_url.as_deref(), Some(good));
        assert!(created.webhook_secret.is_some());
    }
    // No webhook: no secret, and the list says so.
    let plain = owner.create_app_key(CreateAppKey { webhook_url: None, ..make("http://x") }).await.unwrap();
    assert!(plain.webhook_secret.is_none() && plain.key.webhook_url.is_none());
    stop(dir.path(), daemon).await;
}
