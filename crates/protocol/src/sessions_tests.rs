//! Protocol 5: sign-in, sessions, the gate's surface and the Claude login's.

use crate::app::{NOT_FOR_APPS, required as app_required};
use crate::remote::{Capability, Needs, required};
use crate::rpc::{Call, ClaudeLoginPrompt, PROTOCOL_MAJOR, Request, ServerMessage, reject_unparsed};
use crate::{ClaudeLoginInputParams, HostWrite, LoginParams, Method, SetPasswordParams, WriteMethod};
use aurelius_domain::user::SignOutReason;
use aurelius_domain::{Command, CommandId, ErrorCode, SessionId, TicketId, UserId};

fn parse(json: serde_json::Value) -> Result<Request, serde_json::Error> {
    serde_json::from_value(json)
}

#[test]
fn the_major_is_five_and_an_older_client_is_told_to_update() {
    assert_eq!(PROTOCOL_MAJOR, 5);
    let message = crate::incompatible_message(4);
    assert!(message.starts_with("Update needed") && message.contains("Update the app"), "{message}");
}

#[test]
fn the_hello_carries_a_session_token_never_printed_and_no_user_id() {
    let parsed = parse(serde_json::json!({
        "protocol": 5, "request_id": 1, "method": "session.hello",
        "params": { "client_name": "t", "client_version": "1", "session_token": "TOKEN-SECRET" }
    }))
    .unwrap();
    let Call::Read(Method::SessionHello(params)) = &parsed.call else { panic!() };
    assert_eq!(params.session_token.as_deref(), Some("TOKEN-SECRET"));
    assert!(!format!("{parsed:?}").contains("TOKEN-SECRET"));
    // A user id an older client remembered is no proof: it is not read at all.
    let parsed = parse(serde_json::json!({
        "protocol": 5, "request_id": 2, "method": "session.hello",
        "params": { "client_name": "t", "client_version": "1", "user_id": UserId::new() }
    }))
    .unwrap();
    let Call::Read(Method::SessionHello(params)) = parsed.call else { panic!() };
    assert_eq!(params.session_token, None);
    assert!(serde_json::to_value(&params).unwrap().get("user_id").is_none());
}

#[test]
fn user_use_is_gone() {
    let value = serde_json::json!({ "protocol": 5, "request_id": 3, "method": "user.use", "params": { "user_id": UserId::new(), "password": "x" } });
    let err = parse(value.clone()).unwrap_err();
    let ServerMessage::Response { outcome: crate::Outcome::Error(e), .. } = reject_unparsed(Some(&value), &err) else { panic!() };
    assert_eq!(e.code, ErrorCode::UnknownMethod);
}

#[test]
fn every_new_method_travels_under_its_name() {
    let user = UserId::new();
    let login = LoginParams { username: "alex".into(), password: "PASSWORD-SECRET".into(), device_name: "Desktop · laptop".into() };
    let reads = [
        (Method::UserLogin(login.clone()), "user.login"),
        (Method::UserLogout, "user.logout"),
        (Method::SessionList { user_id: None }, "session.list"),
        (Method::SessionList { user_id: Some(user) }, "session.list"),
        (Method::ClaudeLoginStatus, "claude_login.status"),
        (Method::ClaudeLoginInput(ClaudeLoginInputParams { link_id: "l1".into(), text: "CODE-SECRET".into() }), "claude_login.input"),
    ];
    for (method, name) in reads {
        let request = Request::read(1, method);
        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["method"], name);
        assert_eq!(parse(json).unwrap(), request, "{name}");
        assert!(Method::knows(name));
    }
    let password = SetPasswordParams { user_id: Some(user), old_password: Some("OLD-SECRET".into()), new_password: "NEW-SECRET".into() };
    let writes = [
        (WriteMethod::Host(HostWrite::SessionRevoke { session_id: SessionId::new() }), "session.revoke"),
        (WriteMethod::Host(HostWrite::UserSetPassword(password)), "user.set_password"),
        (WriteMethod::Host(HostWrite::UserDisable { user_id: user }), "user.disable"),
        (WriteMethod::Host(HostWrite::ClaudeLoginLink), "claude_login.link"),
        (WriteMethod::Host(HostWrite::ClaudeLoginCancel { link_id: "l1".into() }), "claude_login.cancel"),
        (WriteMethod::Host(HostWrite::ClaudeLoginUnlink), "claude_login.unlink"),
        (WriteMethod::Domain(Command::UseMyLogin(aurelius_domain::ticket::UseMyLogin { ticket_id: TicketId::new() })), "ticket.use_my_login"),
        (WriteMethod::Domain(Command::SetBackgroundLogin(aurelius_domain::company::SetBackgroundLogin { enabled: true })), "company.set_background_login"),
    ];
    for (write, name) in writes {
        let request = Request::write(2, CommandId::new(), write);
        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["method"], name);
        assert_eq!(parse(json).unwrap(), request, "{name}");
        assert!(WriteMethod::knows(name));
    }
}

#[test]
fn no_password_token_or_code_is_ever_printed() {
    let login = Method::UserLogin(LoginParams { username: "alex".into(), password: "PASSWORD-SECRET".into(), device_name: "d".into() });
    let input = Method::ClaudeLoginInput(ClaudeLoginInputParams { link_id: "l1".into(), text: "CODE-SECRET".into() });
    let password = SetPasswordParams { user_id: None, old_password: Some("OLD-SECRET".into()), new_password: "NEW-SECRET".into() };
    let host = WriteMethod::Host(HostWrite::UserSetPassword(password.clone()));
    for printed in [format!("{:?}", Request::read(1, login)), format!("{:?}", Request::read(2, input)), format!("{password:?}"), format!("{host:?}")] {
        assert!(!printed.contains("SECRET"), "{printed}");
    }
}

#[test]
fn the_server_messages_of_a_session() {
    let prompt = ServerMessage::ClaudeLoginPrompt(ClaudeLoginPrompt { link_id: "l1".into(), url: Some("https://claude.com/cai/oauth/authorize?x".into()), asks_code: true, error: None });
    let json = serde_json::to_value(&prompt).unwrap();
    assert_eq!(json["type"], "claude_login_prompt");
    assert!(json.get("error").is_none());
    assert_eq!(serde_json::from_value::<ServerMessage>(json).unwrap(), prompt);
    let out = serde_json::to_value(ServerMessage::SignedOut { reason: SignOutReason::PasswordChanged }).unwrap();
    assert_eq!(out, serde_json::json!({ "type": "signed_out", "reason": "password_changed" }));
}

#[test]
fn every_own_device_may_sign_in_and_apps_never() {
    let calls = [
        Call::Read(Method::UserLogin(LoginParams { username: "a".into(), password: "b".into(), device_name: "c".into() })),
        Call::Read(Method::UserLogout),
        Call::Read(Method::SessionList { user_id: None }),
        Call::Read(Method::ClaudeLoginStatus),
        Call::Write(WriteMethod::Host(HostWrite::SessionRevoke { session_id: SessionId::new() })),
        Call::Write(WriteMethod::Host(HostWrite::UserDisable { user_id: UserId::new() })),
        Call::Write(WriteMethod::Host(HostWrite::ClaudeLoginLink)),
    ];
    for call in calls {
        assert_eq!(required(&call), Needs::Cap(Capability::Read), "{call:?}");
        assert_eq!(app_required(&call).unwrap_err(), NOT_FOR_APPS, "{call:?}");
    }
    let take = Call::Write(WriteMethod::Domain(Command::UseMyLogin(aurelius_domain::ticket::UseMyLogin { ticket_id: TicketId::new() })));
    assert_eq!(required(&take), Needs::Cap(Capability::TaskControl));
    assert_eq!(app_required(&take).unwrap_err(), NOT_FOR_APPS);
}

#[test]
fn the_ticket_says_whose_login_it_runs_on() {
    use aurelius_domain::ticket::TicketNeed;
    assert_eq!(serde_json::to_value(TicketNeed::Login).unwrap(), "login");
    assert_eq!(aurelius_domain::task::WAITING_LOGIN, "login");
    assert!(aurelius_domain::EventKind::parse("user.disabled").is_some() && aurelius_domain::EventKind::parse("claude_login.updated").is_some());
}
