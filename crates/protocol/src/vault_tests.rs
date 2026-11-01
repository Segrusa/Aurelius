//! The vault's surface (feature `vault`, additive in protocol 5): names on the
//! wire, no value in any Debug output or receipt, never for apps.

use crate::app;
use crate::method::{KeyExportParams, Method};
use crate::remote::{self, Needs};
use crate::rpc::{Call, FEATURES, PROTOCOL_MAJOR, Request, feature};
use crate::write::{HostResult, HostWrite, VaultCreateParams, VaultUpdateParams, WriteMethod};
use aurelius_domain::app_key::{AppAudience, AppScope};
use aurelius_domain::remote::Capability;
use aurelius_domain::vault::{RevealPurpose, UserRef, VaultItemView, VaultKind};
use aurelius_domain::{CommandId, EventKind, ProjectId, UserId, VaultItemId};

const FAKE: &str = "sk-test-FAKE-0000";

fn create() -> HostWrite {
    HostWrite::VaultCreate(VaultCreateParams { project_id: Some(ProjectId::new()), kind: VaultKind::Secret, name: "GitHub token".into(), env_name: "GITHUB_TOKEN".into(), description: None, value: FAKE.into() })
}

fn update() -> HostWrite {
    HostWrite::VaultUpdate(VaultUpdateParams { item_id: VaultItemId::new(), name: None, env_name: None, description: Some("rotated".into()), value: Some(FAKE.into()), kind: None })
}

fn reads() -> Vec<Method> {
    let item_id = VaultItemId::new();
    vec![
        Method::VaultList { project_id: None },
        Method::VaultList { project_id: Some(ProjectId::new()) },
        Method::VaultReveal { item_id, purpose: RevealPurpose::Show },
        Method::VaultReveal { item_id, purpose: RevealPurpose::Copy },
        Method::VaultAudit { project_id: None, all: true, item_id: Some(item_id), before: Some(9), limit: Some(20) },
        Method::VaultKeyExport(KeyExportParams { password: "PASSWORD-SECRET".into() }),
    ]
}

fn writes() -> Vec<HostWrite> {
    vec![create(), update(), HostWrite::VaultArchive { item_id: VaultItemId::new() }]
}

#[test]
fn the_vault_methods_roundtrip_under_their_names() {
    assert_eq!(PROTOCOL_MAJOR, 5, "additive: the major stays");
    assert!(FEATURES.contains(&feature::VAULT));
    let names = ["vault.list", "vault.list", "vault.reveal", "vault.reveal", "vault.audit", "vault.key_export"];
    for (method, name) in reads().into_iter().zip(names) {
        let request = Request::read(1, method);
        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["method"], name);
        assert_eq!(serde_json::from_value::<Request>(json).unwrap(), request, "{name}");
    }
    for (write, name) in writes().into_iter().zip(["vault.create", "vault.update", "vault.archive"]) {
        let request = Request::write(2, CommandId::new(), WriteMethod::Host(write));
        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["method"], name);
        assert!(WriteMethod::knows(name));
        assert_eq!(serde_json::from_value::<Request>(json).unwrap(), request, "{name}");
    }
    // Every filter of the list and the audit log may be left out.
    for name in ["vault.list", "vault.audit"] {
        let parsed: Request = serde_json::from_value(serde_json::json!({ "protocol": 5, "request_id": 3, "method": name })).unwrap();
        assert_eq!(serde_json::to_value(&parsed).unwrap()["method"], name);
    }
    for kind in [EventKind::VaultItemCreated, EventKind::VaultItemUpdated, EventKind::VaultItemArchived] {
        assert_eq!(kind.area(), "vault");
    }
}

#[test]
fn no_vault_value_shows_in_debug_output() {
    for write in writes() {
        let printed = format!("{:?} {:?}", write, Call::Write(WriteMethod::Host(write.clone())));
        assert!(!printed.contains("FAKE"), "{printed}");
    }
    let HostWrite::VaultCreate(params) = create() else { unreachable!() };
    let HostWrite::VaultUpdate(update) = update() else { unreachable!() };
    for printed in [format!("{params:?}"), format!("{update:?}")] {
        assert!(!printed.contains("FAKE") && printed.contains("<redacted>"), "{printed}");
    }
    let value = crate::ResponseBody::VaultValue(aurelius_domain::vault::VaultValue { item_id: VaultItemId::new(), value: FAKE.into() });
    let key = crate::ResponseBody::VaultKey(aurelius_domain::vault::VaultKeyExport { key_id: "abc".into(), key: "AAAAkey".into() });
    assert!(!format!("{value:?}").contains("FAKE") && !format!("{key:?}").contains("AAAAkey"));
}

#[test]
fn a_receipt_keeps_no_vault_value() {
    let by = UserRef { id: UserId::new(), username: "alex".into() };
    let item = VaultItemView {
        id: VaultItemId::new(),
        project_id: None,
        kind: VaultKind::Url,
        name: "Staging".into(),
        env_name: "STAGING_URL".into(),
        description: None,
        value: Some(format!("https://{FAKE}.example")),
        undecryptable: false,
        overrides_company: false,
        version: 1,
        created_by: by.clone(),
        updated_by: by,
        created_at_ms: 1,
        updated_at_ms: 1,
    };
    let result = HostResult::VaultItem(item);
    assert!(serde_json::to_string(&result).unwrap().contains("FAKE"), "the answer shows a url");
    assert!(!serde_json::to_string(&result.redacted()).unwrap().contains("FAKE"));
}

#[test]
fn apps_never_reach_the_vault() {
    let calls = reads().into_iter().map(Call::Read).chain(writes().into_iter().map(|w| Call::Write(WriteMethod::Host(w))));
    for call in calls {
        assert_eq!(app::required(&call), Err(app::NOT_FOR_APPS), "{call:?}");
        for audience in [AppAudience::All, AppAudience::Management] {
            assert!(app::allowed_for(&AppScope::ALL, audience, &call).is_err(), "{call:?}");
        }
    }
}

#[test]
fn devices_need_read_to_list_task_control_to_change_and_owner_to_export_the_key() {
    let phone = [Capability::Read, Capability::Chat, Capability::TaskControl, Capability::Approve];
    let needs = |call: Call| remote::required(&call);
    assert_eq!(needs(Call::Read(Method::VaultList { project_id: None })), Needs::Cap(Capability::Read));
    assert_eq!(needs(Call::Read(Method::VaultAudit { project_id: None, all: false, item_id: None, before: None, limit: None })), Needs::Cap(Capability::Read));
    assert_eq!(needs(Call::Read(Method::VaultReveal { item_id: VaultItemId::new(), purpose: RevealPurpose::Copy })), Needs::Cap(Capability::TaskControl));
    for write in writes() {
        assert_eq!(needs(Call::Write(WriteMethod::Host(write))), Needs::Cap(Capability::TaskControl));
    }
    let export = needs(Call::Read(Method::VaultKeyExport(KeyExportParams { password: "p".into() })));
    assert_eq!(export, Needs::Owner);
    assert!(!remote::grants(&phone, export) && remote::grants(&[Capability::Owner], export));
}

#[test]
fn confirm_secret_is_left_out_until_set() {
    let ticket = aurelius_domain::ticket::PostTicketMessage { ticket_id: aurelius_domain::TicketId::new(), text: "hi".into(), images: vec![], files: vec![], confirm_secret: false, thread_parent: None };
    let json = serde_json::to_value(&ticket).unwrap();
    assert!(json.get("confirm_secret").is_none(), "the request hash of older clients' writes stays the same");
    let parsed: aurelius_domain::group_chat::SendGroupMessage = serde_json::from_value(serde_json::json!({ "text": "hi", "confirm_secret": true })).unwrap();
    assert!(parsed.confirm_secret);
    let parsed: aurelius_domain::employee::SendEmployeeMessage =
        serde_json::from_value(serde_json::json!({ "employee_id": aurelius_domain::EmployeeId::new(), "text": "hi", "conversation_id": aurelius_domain::ConversationId::new() })).unwrap();
    assert!(!parsed.confirm_secret);
}

#[test]
fn the_vault_tools_of_a_run_travel_as_ops() {
    use crate::bridge::EmployeeTaskOp;
    let op: EmployeeTaskOp = serde_json::from_value(serde_json::json!({ "op": "vault_read", "item": "STAGING_URL" })).unwrap();
    assert_eq!(op, EmployeeTaskOp::VaultRead { item: "STAGING_URL".into(), project: None });
    let op: EmployeeTaskOp = serde_json::from_value(serde_json::json!({ "op": "vault_list" })).unwrap();
    assert_eq!(op, EmployeeTaskOp::VaultList { project: None });
}

#[test]
fn the_key_export_needs_the_password_and_never_prints_it() {
    let missing = serde_json::from_value::<Request>(serde_json::json!({ "protocol": 5, "request_id": 3, "method": "vault.key_export" }));
    assert!(missing.is_err(), "E9: no export without the password");
    let empty = serde_json::from_value::<Request>(serde_json::json!({ "protocol": 5, "request_id": 3, "method": "vault.key_export", "params": {} }));
    assert!(empty.is_err());
    let request = Request::read(4, Method::VaultKeyExport(KeyExportParams { password: "PASSWORD-SECRET".into() }));
    let json = serde_json::to_value(&request).unwrap();
    assert_eq!(json["params"]["password"], "PASSWORD-SECRET");
    let printed = format!("{request:?} {:?}", Call::Read(Method::VaultKeyExport(KeyExportParams { password: "PASSWORD-SECRET".into() })));
    assert!(!printed.contains("PASSWORD-SECRET") && printed.contains("<redacted>"), "{printed}");
}

#[test]
fn a_protocol_error_on_a_vault_method_never_quotes_its_params() {
    let bad = [
        serde_json::json!({ "protocol": 5, "request_id": 7, "idempotency_key": CommandId::new(), "method": "vault.create", "params": { "kind": "secret", "name": "x", "env_name": "X_TOKEN", "value": 12, "extra": FAKE } }),
        serde_json::json!({ "protocol": 5, "request_id": 7, "idempotency_key": CommandId::new(), "method": "vault.update", "params": { "item_id": FAKE, "value": FAKE } }),
        serde_json::json!({ "protocol": 5, "request_id": 7, "method": "vault.key_export", "params": { "password": [FAKE] } }),
        serde_json::json!({ "protocol": 5, "request_id": 7, "method": "vault.reveal", "params": { "item_id": FAKE, "purpose": FAKE } }),
    ];
    for value in bad {
        let err = serde_json::from_value::<Request>(value.clone()).unwrap_err();
        let reply = serde_json::to_string(&crate::reject_unparsed(Some(&value), &err)).unwrap();
        assert!(!reply.contains("FAKE") && reply.contains("is not valid"), "{reply}");
    }
}
