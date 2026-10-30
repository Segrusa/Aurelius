//! Webhooks of app keys (feature `webhooks`): the events of the company that
//! concern an app are posted to the app's webhook URL, signed.
//!
//! One loop does two things. It reads the event log after a persistent cursor and
//! writes a row into the outbox for every app the event concerns (the row and the
//! cursor in one transaction, the row id derived from key and event, so handling
//! an event twice queues it once). And it delivers the rows that are due: a POST
//! signed with the key's webhook secret (`aurelius_domain::webhook`); any 2xx
//! counts as delivered, anything else is retried with exponential backoff up to
//! `MAX_ATTEMPTS`, the delivery id staying the same. The outbox is in the
//! database, so a restart loses nothing. See docs/aurelius-mcp.md for the contract.
//!
//! Which events an app gets: those of tasks it created, and, for an app with the
//! audience `management`, those of the management's tasks and questions. A reply
//! (`manager.replied`) goes to the app that wrote into that conversation.

use crate::Shared;
use aurelius_domain::app_key::AppAudience;
use aurelius_domain::webhook::{self, Envelope, EnvelopeApp, WebhookEvent};
use aurelius_domain::{AppKeyId, EmployeeId, EventKind, EventNotice, RunId};
use aurelius_storage::app_keys::{self as keys, AppKeyRow, ManagementMember};
use aurelius_storage::{Store, StorageError, conversations as conv, queries, webhooks as outbox};
use rusqlite::{Connection, OptionalExtension};
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;

/// Events read per round.
const PAGE: u32 = 200;
/// Deliveries attempted per round.
const BATCH: u32 = 20;
/// The longest the loop sleeps without looking at the outbox.
const IDLE: Duration = Duration::from_secs(5);
const PURGE_EVERY_MS: i64 = 3_600_000;

struct Config {
    backoff_base_ms: i64,
}

impl Config {
    fn from_env() -> Self {
        // Tests shorten the wait between attempts.
        let backoff_base_ms = std::env::var("AURELIUS_WEBHOOK_BACKOFF_MS").ok().and_then(|v| v.parse().ok()).filter(|v: &i64| *v > 0).unwrap_or(webhook::BACKOFF_BASE_MS);
        Self { backoff_base_ms }
    }
}

pub fn start(shared: Arc<Shared>) {
    tokio::spawn(run(shared));
}

async fn run(shared: Arc<Shared>) {
    let config = Config::from_env();
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(webhook::TIMEOUT_SECS))
        .connect_timeout(Duration::from_secs(5))
        // A redirect is not followed: it counts as a failed attempt (the receiver's URL is wrong).
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(format!("Aurelius/{}", crate::VERSION))
        .build()
    {
        Ok(client) => client,
        Err(err) => {
            tracing::error!(error = %err, "webhooks: no HTTP client, webhooks are off");
            return;
        }
    };
    let mut events = shared.events.subscribe();
    let mut shutdown = shared.shutdown.subscribe();
    let mut cursor = match init_cursor(&shared).await {
        Ok(cursor) => cursor,
        Err(err) => {
            tracing::error!(error = %err, "webhooks: cannot read the cursor, webhooks are off");
            return;
        }
    };
    let mut last_purge = 0i64;
    loop {
        match enqueue_new(&shared, cursor).await {
            Ok(next) => cursor = next,
            Err(err) => tracing::warn!(error = %err, "webhooks: reading events failed, will retry"),
        }
        let next_due = deliver_due(&shared, &client, &config).await;
        let now = aurelius_storage::now_ms();
        if now - last_purge > PURGE_EVERY_MS {
            last_purge = now;
            let store = shared.store().clone();
            let _ = tokio::task::spawn_blocking(move || store.write(outbox::purge)).await;
        }
        let wait = next_due.map(|at| Duration::from_millis((at - now).clamp(50, IDLE.as_millis() as i64) as u64)).unwrap_or(IDLE);
        tokio::select! {
            received = events.recv() => {
                if matches!(received, Err(tokio::sync::broadcast::error::RecvError::Closed)) {
                    break;
                }
            }
            _ = tokio::time::sleep(wait) => {}
            _ = shutdown.changed() => break,
        }
    }
}

/// Where to read on: the stored cursor, or, the first time, the end of the log
/// (a new feature does not replay history).
async fn init_cursor(shared: &Arc<Shared>) -> Result<u64, StorageError> {
    let store = shared.store().clone();
    let join = tokio::task::spawn_blocking(move || {
        let (stored, last) = store.read(|c| Ok((outbox::cursor(c)?, queries::last_seq(c)?)))?;
        match stored {
            Some(cursor) => Ok(cursor),
            None => {
                store.write(move |tx| outbox::set_cursor(tx, last))?;
                Ok(last)
            }
        }
    })
    .await;
    join.unwrap_or_else(|err| Err(StorageError::Rejected(err.to_string())))
}

/// One delivery to queue.
struct NewDelivery {
    id: String,
    key: AppKeyId,
    event: WebhookEvent,
    body: String,
    /// A reply was queued: the app waits for the next message again.
    reply_in: Option<String>,
}

/// Queue what the events after `cursor` mean for apps; the new cursor.
async fn enqueue_new(shared: &Arc<Shared>, mut cursor: u64) -> Result<u64, StorageError> {
    loop {
        let store = shared.store().clone();
        let after = cursor;
        let join = tokio::task::spawn_blocking(move || -> Result<(u64, usize), StorageError> {
            let (deliveries, last, count) = store.read(|c| plan(&store, c, after))?;
            if count == 0 {
                return Ok((after, 0));
            }
            store.write(move |tx| {
                for d in &deliveries {
                    outbox::enqueue(tx, &d.id, d.key, d.event.as_str(), &d.body)?;
                    if let Some(conversation) = &d.reply_in {
                        keys::clear_awaiting(tx, d.key, conversation)?;
                    }
                }
                outbox::set_cursor(tx, last)
            })?;
            Ok((last, count))
        })
        .await;
        let (next, count) = join.unwrap_or_else(|err| Err(StorageError::Rejected(err.to_string())))?;
        cursor = next;
        if count < PAGE as usize {
            return Ok(cursor);
        }
    }
}

/// The deliveries for one page of events after `after`, the last seq of the page, and its size.
fn plan(store: &Store, conn: &Connection, after: u64) -> Result<(Vec<NewDelivery>, u64, usize), StorageError> {
    let page = queries::events_after(conn, after, PAGE)?;
    let Some(last) = page.last().map(|n| n.seq) else { return Ok((Vec::new(), after, 0)) };
    let apps = keys::webhook_keys(conn)?;
    let mut out = Vec::new();
    if !apps.is_empty() {
        let members = keys::management(conn)?;
        for notice in &page {
            out.extend(deliveries(store, conn, &apps, &members, notice)?);
        }
    }
    Ok((out, last, page.len()))
}

/// The delivery id: stable for one app and one event, so it never repeats for another.
fn delivery_id(key: AppKeyId, seq: u64, event: WebhookEvent) -> String {
    use sha2::{Digest, Sha256};
    let digest = hex::encode(Sha256::digest(format!("{key}:{seq}:{}", event.as_str()).as_bytes()));
    format!("dlv_{}", &digest[..32])
}

fn envelope(key: &AppKeyRow, id: &str, event: WebhookEvent, created_at_ms: i64, data: Value) -> String {
    let envelope = Envelope { id: id.to_owned(), kind: event.as_str().to_owned(), created_at_ms, app: EnvelopeApp { id: key.id.to_string(), name: key.name.clone() }, data };
    serde_json::to_string(&envelope).unwrap_or_else(|_| "{}".into())
}

fn clip(text: &str) -> String {
    if text.chars().count() <= webhook::TEXT_MAX_CHARS {
        return text.to_owned();
    }
    let cut: String = text.chars().take(webhook::TEXT_MAX_CHARS).collect();
    format!("{cut}… (cut)")
}

fn read_blob(store: &Store, key: Option<String>) -> Option<String> {
    let bytes = store.blobs().read(&key?).ok()?;
    String::from_utf8(bytes).ok()
}

/// Whether a task of `employee` made by `created_by` concerns `key`.
fn concerns(key: &AppKeyRow, members: &[ManagementMember], created_by: Option<AppKeyId>, employee: Option<EmployeeId>) -> bool {
    created_by == Some(key.id) || (key.audience == AppAudience::Management && employee.is_some_and(|e| members.iter().any(|m| m.id == e)))
}

struct TaskInfo {
    title: String,
    status: String,
    waiting_reason: Option<String>,
    employee_id: Option<EmployeeId>,
    employee: Option<String>,
    project_id: String,
    project: String,
    created_by: Option<AppKeyId>,
    conversation_id: Option<String>,
    report: Option<String>,
}

fn task_info(store: &Store, conn: &Connection, id: &str) -> Result<Option<TaskInfo>, StorageError> {
    let row = conn
        .query_row(
            "SELECT t.title, t.status, t.waiting_reason, t.employee_id, m.name, t.project_id, p.name, t.created_by_app_key_id, t.conversation_id, rb.storage_key
             FROM tasks t LEFT JOIN employees m ON m.id = t.employee_id JOIN projects p ON p.id = t.project_id
             LEFT JOIN blobs rb ON rb.id = t.result_blob_id WHERE t.id = ?1",
            [id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, Option<String>>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, String>(6)?,
                    r.get::<_, Option<String>>(7)?,
                    r.get::<_, Option<String>>(8)?,
                    r.get::<_, Option<String>>(9)?,
                ))
            },
        )
        .optional()?;
    Ok(row.map(|(title, status, waiting_reason, employee_id, employee, project_id, project, created_by, conversation_id, report)| TaskInfo {
        title,
        status,
        waiting_reason,
        employee_id: employee_id.and_then(|e| EmployeeId::parse(&e).ok()),
        employee,
        project_id,
        project,
        created_by: created_by.and_then(|k| AppKeyId::parse(&k).ok()),
        conversation_id,
        report: read_blob(store, report).map(|t| clip(&t)),
    }))
}

fn task_data(info: &TaskInfo, task_id: &str, key: &AppKeyRow) -> Value {
    json!({
        "task_id": task_id,
        "title": info.title,
        "status": info.status,
        "project_id": info.project_id,
        "project": info.project,
        "employee_id": info.employee_id,
        "employee": info.employee,
        "conversation_id": info.conversation_id,
        "created_by_this_app": info.created_by == Some(key.id),
    })
}

/// What one event means for each app with a webhook.
fn deliveries(store: &Store, conn: &Connection, apps: &[AppKeyRow], members: &[ManagementMember], notice: &EventNotice) -> Result<Vec<NewDelivery>, StorageError> {
    let mut out = Vec::new();
    match notice.kind {
        EventKind::TaskSucceeded | EventKind::TaskFailed | EventKind::TaskCancelled | EventKind::TaskWaiting => {
            let Some(info) = task_info(store, conn, &notice.aggregate_id)? else { return Ok(out) };
            let waiting = notice.kind == EventKind::TaskWaiting;
            let event = if waiting { WebhookEvent::TaskWaiting } else { WebhookEvent::TaskFinished };
            let detail = if waiting { outbox::event_detail(conn, notice.seq)?.map(|(_, _, meta)| meta) } else { None };
            for key in apps.iter().filter(|k| concerns(k, members, info.created_by, info.employee_id)) {
                let mut data = task_data(&info, &notice.aggregate_id, key);
                if waiting {
                    data["status"] = json!("waiting");
                    data["reason"] = detail.as_ref().and_then(|m| m["reason"].as_str()).map(Value::from).or_else(|| info.waiting_reason.clone().map(Value::from)).unwrap_or(Value::Null);
                    data["until_ms"] = detail.as_ref().map(|m| m["until_ms"].clone()).unwrap_or(Value::Null);
                } else {
                    data["status"] = json!(match notice.kind {
                        EventKind::TaskSucceeded => "succeeded",
                        EventKind::TaskFailed => "failed",
                        _ => "cancelled",
                    });
                    data["report"] = info.report.clone().map(Value::from).unwrap_or(Value::Null);
                }
                let id = delivery_id(key.id, notice.seq, event);
                out.push(NewDelivery { body: envelope(key, &id, event, notice.occurred_at_ms, data), id, key: key.id, event, reply_in: None });
            }
        }
        EventKind::MessageAssistantCommitted => {
            // aggregate: the conversation. Only the apps waiting for a reply in it.
            let waiting = keys::awaiting_keys(conn, &notice.aggregate_id)?;
            if waiting.is_empty() {
                return Ok(out);
            }
            let Some((_, _, meta)) = outbox::event_detail(conn, notice.seq)? else { return Ok(out) };
            // An answer to the group chat (the CEO's main chat is also the apps' channel) is not the app's reply.
            if let Some(run) = meta["run_id"].as_str().and_then(|r| RunId::parse(r).ok())
                && conv::group_run(conn, run)?.is_some()
            {
                return Ok(out);
            }
            let message_id = meta["message_id"].as_str().unwrap_or("").to_owned();
            let text: Option<String> = conn
                .query_row("SELECT b.storage_key FROM messages m JOIN blobs b ON b.id = m.content_blob_id WHERE m.id = ?1", [&message_id], |r| r.get(0))
                .optional()?;
            let employee: Option<(String, String)> = conn
                .query_row("SELECT e.id, e.name FROM conversations c JOIN employees e ON e.id = c.employee_id WHERE c.id = ?1", [&notice.aggregate_id], |r| Ok((r.get(0)?, r.get(1)?)))
                .optional()?;
            for key in apps.iter().filter(|k| waiting.contains(&k.id)) {
                let data = json!({
                    "conversation_id": notice.aggregate_id,
                    "message_id": message_id,
                    "employee_id": employee.as_ref().map(|e| e.0.clone()),
                    "employee": employee.as_ref().map(|e| e.1.clone()),
                    "text": read_blob(store, text.clone()).map(|t| clip(&t)).unwrap_or_default(),
                });
                let event = WebhookEvent::ManagerReplied;
                let id = delivery_id(key.id, notice.seq, event);
                out.push(NewDelivery { body: envelope(key, &id, event, notice.occurred_at_ms, data), id, key: key.id, event, reply_in: Some(notice.aggregate_id.clone()) });
            }
        }
        EventKind::ApprovalRequested => {
            let row = conn
                .query_row(
                    "SELECT b.storage_key, r.conversation_id, r.task_id, COALESCE(c.employee_id, r.employee_id), COALESCE(m.name, rm.name), a.expires_at_ms
                     FROM approvals a JOIN blobs b ON b.id = a.action_blob_id JOIN runs r ON r.id = a.run_id
                     LEFT JOIN conversations c ON c.id = r.conversation_id LEFT JOIN employees m ON m.id = c.employee_id
                     LEFT JOIN employees rm ON rm.id = r.employee_id WHERE a.id = ?1",
                    [&notice.aggregate_id],
                    |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, Option<String>>(2)?, r.get::<_, Option<String>>(3)?, r.get::<_, Option<String>>(4)?, r.get::<_, i64>(5)?)),
                )
                .optional()?;
            let Some((action_key, conversation, task, employee_id, employee, expires_at_ms)) = row else { return Ok(out) };
            let action: Value = read_blob(store, Some(action_key)).and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
            // Only the questions Claude asks the user (AskUserQuestion); plans and tool approvals are the user's own.
            if action["tool"].as_str() != Some("AskUserQuestion") {
                return Ok(out);
            }
            let employee_id = employee_id.and_then(|e| EmployeeId::parse(&e).ok());
            let created_by = match &task {
                Some(task) => task_info(store, conn, task)?.and_then(|t| t.created_by),
                None => None,
            };
            // A reply thread of the app counts as its own: the app asked in that conversation.
            let thread = conversation.as_deref().and_then(|c| aurelius_domain::ConversationId::parse(c).ok());
            let threads: Vec<AppKeyId> = match thread {
                Some(c) => apps.iter().filter(|k| keys::has_thread(conn, k.id, c).unwrap_or(false)).map(|k| k.id).collect(),
                None => Vec::new(),
            };
            for key in apps.iter().filter(|k| concerns(k, members, created_by, employee_id) || threads.contains(&k.id)) {
                let data = json!({
                    "id": notice.aggregate_id,
                    "kind": "question",
                    "employee_id": employee_id,
                    "employee": employee,
                    "task_id": task,
                    "conversation_id": conversation,
                    "questions": action["arguments"]["questions"],
                    "expires_at_ms": expires_at_ms,
                });
                let event = WebhookEvent::QuestionForYou;
                let id = delivery_id(key.id, notice.seq, event);
                out.push(NewDelivery { body: envelope(key, &id, event, notice.occurred_at_ms, data), id, key: key.id, event, reply_in: None });
            }
        }
        _ => {}
    }
    Ok(out)
}

/// Attempt the deliveries that are due; when the next one is due (none: nothing waits).
async fn deliver_due(shared: &Arc<Shared>, client: &reqwest::Client, config: &Config) -> Option<i64> {
    let store = shared.store().clone();
    let now = aurelius_storage::now_ms();
    let due = match tokio::task::spawn_blocking(move || store.read(|c| outbox::due(c, now, BATCH))).await {
        Ok(Ok(due)) => due,
        _ => return None,
    };
    for row in due {
        let store = shared.store().clone();
        let key_id = row.key_id;
        let key = tokio::task::spawn_blocking(move || store.read(|c| keys::load(c, key_id))).await.ok().and_then(Result::ok).flatten();
        let target = key.filter(|k| !k.revoked()).and_then(|k| Some((k.webhook_url.clone()?, k.webhook_secret.clone()?)));
        let Some((url, secret)) = target else {
            let (store, id) = (shared.store().clone(), row.id.clone());
            let _ = tokio::task::spawn_blocking(move || store.write(move |tx| outbox::give_up(tx, &id, "the app key was revoked or has no webhook"))).await;
            continue;
        };
        let result = attempt(client, &url, &secret, &row).await;
        let attempts_made = row.attempts + 1;
        let (store, id) = (shared.store().clone(), row.id.clone());
        let recorded = match result {
            Ok(status) if webhook::delivered(status) => tokio::task::spawn_blocking(move || store.write(move |tx| outbox::mark_delivered(tx, &id, status))).await,
            failed => {
                let (status, error) = match failed {
                    Ok(status) => (Some(status), format!("the receiver answered HTTP {status}")),
                    Err(error) => (None, error),
                };
                let retry_at = webhook::retry_delay_ms(attempts_made, config.backoff_base_ms).map(|delay| aurelius_storage::now_ms() + delay);
                if retry_at.is_none() {
                    tracing::warn!(delivery = %row.id, event = %row.event, "webhooks: gave up after {attempts_made} attempts: {error}");
                }
                tokio::task::spawn_blocking(move || store.write(move |tx| outbox::mark_failed(tx, &id, retry_at, status, &error))).await
            }
        };
        if let Ok(Err(err)) = recorded {
            tracing::warn!(error = %err, "webhooks: could not record an attempt");
        }
    }
    let store = shared.store().clone();
    tokio::task::spawn_blocking(move || store.read(outbox::next_due_ms)).await.ok().and_then(Result::ok).flatten()
}

/// One signed POST; the HTTP status, or why it did not get one.
async fn attempt(client: &reqwest::Client, url: &str, secret: &str, row: &outbox::OutboxRow) -> Result<u16, String> {
    let timestamp = aurelius_storage::now_ms() / 1000;
    let signature = webhook::signature(secret, timestamp, &row.id, &row.body_json);
    let response = client
        .post(url)
        .header("Content-Type", "application/json")
        .header(webhook::HEADER_DELIVERY, row.id.as_str())
        .header(webhook::HEADER_EVENT, row.event.as_str())
        .header(webhook::HEADER_TIMESTAMP, timestamp.to_string())
        .header(webhook::HEADER_ATTEMPT, (row.attempts + 1).to_string())
        .header(webhook::HEADER_SIGNATURE, signature)
        .body(row.body_json.clone())
        .send()
        .await
        .map_err(|err| {
            let text = err.to_string();
            if err.is_timeout() { format!("timed out after {} s", webhook::TIMEOUT_SECS) } else { text.chars().take(300).collect() }
        })?;
    Ok(response.status().as_u16())
}
