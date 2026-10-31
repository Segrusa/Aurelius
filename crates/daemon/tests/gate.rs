//! The gate (MU3-06): every method of the protocol, called by a connection
//! without a session, is refused with `unauthenticated`, except the handshake,
//! `user.whoami`, `user.login`, `user.list` and the first `user.create` (AC1,
//! AC4); a subscribe without a session gets nothing, not even history (AC2).
//!
//! Every method is taken from the JSON schemas of the protocol, with
//! parameters made from its schema, so a new method is covered without
//! touching this test.

mod common;

use aurelius_domain::{Command, ErrorCode};
use aurelius_host::ipc::{self, BoxStream, read_frame, write_frame};
use aurelius_protocol::{HostWrite, Method, Outcome, Request, ServerMessage};
use serde_json::{Map, Value, json};
use std::time::Duration;

/// A value that fits `schema` (the first choice of every `oneOf`/`anyOf`, the
/// smallest of everything else).
fn sample(schema: &Value, defs: &Map<String, Value>, depth: usize) -> Value {
    if depth > 12 {
        return Value::Null;
    }
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        let name = reference.rsplit('/').next().unwrap_or_default();
        return defs.get(name).map(|d| sample(d, defs, depth + 1)).unwrap_or(Value::Null);
    }
    if let Some(value) = schema.get("const") {
        return value.clone();
    }
    if let Some(first) = schema.get("enum").and_then(Value::as_array).and_then(|e| e.first()) {
        return first.clone();
    }
    for key in ["oneOf", "anyOf"] {
        if let Some(choices) = schema.get(key).and_then(Value::as_array) {
            // Prefer a choice that is not null.
            let choice = choices.iter().find(|c| c.get("type") != Some(&json!("null"))).or(choices.first());
            return choice.map(|c| sample(c, defs, depth + 1)).unwrap_or(Value::Null);
        }
    }
    if let Some(all) = schema.get("allOf").and_then(Value::as_array) {
        let mut out = Map::new();
        for part in all {
            if let Value::Object(o) = sample(part, defs, depth + 1) {
                out.extend(o);
            }
        }
        return Value::Object(out);
    }
    let kind = match schema.get("type") {
        Some(Value::Array(types)) => types.iter().find(|t| *t != "null").cloned().unwrap_or(json!("null")),
        Some(t) => t.clone(),
        None if schema.get("properties").is_some() => json!("object"),
        None => return Value::Null,
    };
    match kind.as_str().unwrap_or("null") {
        "object" => {
            let mut out = Map::new();
            let required: Vec<&str> = schema.get("required").and_then(Value::as_array).map(|r| r.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
            if let Some(Value::Object(properties)) = schema.get("properties") {
                for (name, property) in properties {
                    if required.contains(&name.as_str()) {
                        out.insert(name.clone(), sample(property, defs, depth + 1));
                    }
                }
            }
            Value::Object(out)
        }
        "array" => Value::Array(vec![]),
        "string" => match schema.get("format").and_then(Value::as_str) {
            Some("uuid") => json!(uuid::Uuid::now_v7()),
            _ => json!("x"),
        },
        "integer" | "number" => json!(schema.get("minimum").and_then(Value::as_u64).unwrap_or(1)),
        "boolean" => json!(false),
        _ => Value::Null,
    }
}

/// `{ method, params }` of every variant of a tagged enum's schema.
fn calls_of(schema: Value, tag: &str) -> Vec<(String, Value)> {
    let defs = schema.get("$defs").and_then(Value::as_object).cloned().unwrap_or_default();
    let mut out = Vec::new();
    for variant in schema["oneOf"].as_array().expect("a tagged enum") {
        let Some(name) = variant["properties"][tag]["const"].as_str() else { continue };
        let params = match tag {
            // Domain commands: the fields beside `type` are the params.
            "type" => {
                let mut value = sample(variant, &defs, 0);
                value.as_object_mut().map(|o| o.remove("type"));
                value
            }
            _ => variant["properties"].get("params").map(|p| sample(p, &defs, 0)).unwrap_or(Value::Null),
        };
        out.push((name.to_owned(), params));
    }
    out
}

/// Every method of protocol 5 on the wire, with parameters that parse.
fn every_call() -> Vec<Value> {
    let mut calls = calls_of(serde_json::to_value(schemars::schema_for!(Method)).unwrap(), "method");
    calls.extend(calls_of(serde_json::to_value(schemars::schema_for!(HostWrite)).unwrap(), "method"));
    let commands = calls_of(serde_json::to_value(schemars::schema_for!(Command)).unwrap(), "type");
    for (kind, params) in commands {
        let method = aurelius_domain::COMMAND_METHODS.iter().find(|(t, _)| *t == kind).map(|(_, m)| *m).expect("every command has a method");
        calls.push((method.to_owned(), params));
    }
    calls
        .into_iter()
        .enumerate()
        .map(|(i, (method, params))| {
            let mut request = json!({ "protocol": aurelius_protocol::PROTOCOL_MAJOR, "request_id": 100 + i, "method": method, "idempotency_key": aurelius_domain::CommandId::new() });
            if !params.is_null() {
                request["params"] = params;
            }
            request
        })
        .collect()
}

async fn ask(stream: &mut BoxStream, request: &Value) -> Outcome {
    let id = request["request_id"].as_u64().unwrap();
    write_frame(stream, request).await.unwrap();
    loop {
        let message = tokio::time::timeout(Duration::from_secs(10), read_frame::<ServerMessage, _>(stream)).await.expect("an answer in time").unwrap().expect("open");
        if let ServerMessage::Response { request_id, outcome } = message {
            if request_id == id {
                return outcome;
            }
        }
    }
}

const ALLOWED: &[&str] = &["session.hello", "user.whoami", "user.login", "user.list", "user.create"];

#[tokio::test(flavor = "multi_thread")]
async fn every_method_without_a_session_is_refused_but_the_sign_in() {
    // Should the gate ever let one through, no shutdown reaches this computer.
    unsafe {
        std::env::set_var("AURELIUS_POWER_COMMAND", "true");
    }
    let dir = tempfile::tempdir().unwrap();
    let daemon = common::start(dir.path()).await;
    // Board members exist, so `user.create` needs a session too (MU1-20 is over).
    drop(common::connect(dir.path()).await);
    let mut stream = ipc::connect(&aurelius_host::endpoint(dir.path())).await.unwrap();

    let calls = every_call();
    assert!(calls.len() > 150, "every method: {}", calls.len());
    let mut refused = 0;
    for request in &calls {
        let name = request["method"].as_str().unwrap().to_owned();
        serde_json::from_value::<Request>(request.clone()).unwrap_or_else(|e| panic!("{name}: the sample does not parse: {e}"));
        if ALLOWED.contains(&name.as_str()) {
            continue;
        }
        match ask(&mut stream, request).await {
            Outcome::Error(err) => assert_eq!(err.code, ErrorCode::Unauthenticated, "{name}: {}", err.message),
            other => panic!("{name} went through without a session: {other:?}"),
        }
        refused += 1;
    }
    assert_eq!(refused, calls.len() - ALLOWED.len());
    // The allowlist answers (a refused sign-in or onboarding says why, never unauthenticated).
    for request in calls.iter().filter(|r| ALLOWED.contains(&r["method"].as_str().unwrap())) {
        let name = request["method"].as_str().unwrap();
        let outcome = ask(&mut stream, request).await;
        let unauthenticated = matches!(&outcome, Outcome::Error(e) if e.code == ErrorCode::Unauthenticated);
        assert_eq!(unauthenticated, name == "user.create", "{name}: {outcome:?} (user.create only for the first board member)");
    }
    common::stop(dir.path(), daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_subscribe_without_a_session_gets_nothing_not_even_history() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = common::start(dir.path()).await;
    let client = common::connect(dir.path()).await;
    common::conversation(&client).await;
    let mut stream = ipc::connect(&aurelius_host::endpoint(dir.path())).await.unwrap();
    let subscribe = json!({ "protocol": aurelius_protocol::PROTOCOL_MAJOR, "request_id": 1, "method": "event.subscribe", "params": { "after_seq": 0 } });
    assert!(matches!(ask(&mut stream, &subscribe).await, Outcome::Error(e) if e.code == ErrorCode::Unauthenticated));
    // New events happen; none reach this connection, and none from before either.
    common::conversation(&client).await;
    let mut seen = Vec::new();
    while let Ok(Ok(Some(message))) = tokio::time::timeout(Duration::from_millis(500), read_frame::<ServerMessage, _>(&mut stream)).await {
        seen.push(message);
    }
    assert!(seen.iter().all(|m| !matches!(m, ServerMessage::Event(_) | ServerMessage::Stream(_) | ServerMessage::StreamAppend(_))), "{seen:?}");
    common::stop(dir.path(), daemon).await;
}

/// S7 (SEC1 M4): a session with a temporary password may only say hello, ask
/// who it is, sign out or in again and set its own new password; every other
/// method, `user.create` and `user.list` too, is `password_change_required`.
#[tokio::test(flavor = "multi_thread")]
async fn every_method_with_a_temporary_password_is_refused_but_its_change() {
    unsafe {
        std::env::set_var("AURELIUS_POWER_COMMAND", "true");
    }
    let dir = tempfile::tempdir().unwrap();
    let daemon = common::start(dir.path()).await;
    let alex = common::connect(dir.path()).await;
    let robin = common::connect_as(dir.path(), "robin").await;
    let robin_id = robin.user_whoami().await.unwrap().user.unwrap().id;
    alex.set_password(Some(robin_id), None, "a-temporary-password-for-robin").await.unwrap();
    let mut stream = ipc::connect(&aurelius_host::endpoint(dir.path())).await.unwrap();
    let login = json!({
        "protocol": aurelius_protocol::PROTOCOL_MAJOR, "request_id": 1, "method": "user.login",
        "params": { "username": "robin", "password": "a-temporary-password-for-robin", "device_name": "test" },
    });
    match ask(&mut stream, &login).await {
        Outcome::Ok(aurelius_protocol::ResponseBody::Login(login)) => assert!(login.must_change_password),
        other => panic!("{other:?}"),
    }
    const WHILE_CHANGING: &[&str] = &["session.hello", "user.whoami", "user.logout", "user.login", "user.set_password"];
    let calls = every_call();
    let mut refused = 0;
    for request in calls.iter().filter(|r| !WHILE_CHANGING.contains(&r["method"].as_str().unwrap())) {
        let name = request["method"].as_str().unwrap();
        match ask(&mut stream, request).await {
            Outcome::Error(err) => assert_eq!(err.code, ErrorCode::PasswordChangeRequired, "{name}: {}", err.message),
            other => panic!("{name} went through with a temporary password: {other:?}"),
        }
        refused += 1;
    }
    assert_eq!(refused, calls.len() - WHILE_CHANGING.len());
    drop((alex, robin));
    common::stop(dir.path(), daemon).await;
}
