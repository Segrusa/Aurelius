//! The vault and the runs that get it: the audit rows of a ticket's runs name
//! the ticket (MU4-09 AC1, R1 M2, QA1 m1); a name a run may no longer have is
//! told once (SEC1 m4). Scratch data folder; no real Claude.
#![cfg(unix)]

mod common;
use common::*;

use aurelius_client::bridge::BridgeClient;
use aurelius_domain::connection::{AddProvider, ConfigScope, ProviderKind, Purpose, RunDefaults, SetRunDefaults};
use aurelius_domain::employee::{CreateEmployee, Setting};
use aurelius_domain::project::CreateProject;
use aurelius_domain::ticket::{CreateTicket, TicketPriority};
use aurelius_domain::vault::VaultKind;
use aurelius_domain::{Command, CommandResult};
use aurelius_protocol::{EmployeeTaskOp, VaultCreateParams};
use aurelius_runtime::claude_code::VaultEnv;
use aurelius_runtime::{AgentRuntime, RunHandle, RunRequest, RuntimeCapabilities, RuntimeError, RuntimeEvent, fake};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::mpsc;

const VALUE: &str = "sk-test-FAKE-ticket-audit-0000";

/// A runtime that takes the vault: every run reads the vault list; the first
/// one with a value delegates a part to Bridge.
#[derive(Clone)]
struct Worker {
    root: PathBuf,
    vault: VaultEnv,
    delegated: Arc<AtomicBool>,
}

#[async_trait::async_trait]
impl AgentRuntime for Worker {
    async fn probe(&self) -> RuntimeCapabilities {
        fake::FakeRuntime::default().probe().await
    }

    fn with_vault(&self, vault: VaultEnv) -> Option<Arc<dyn AgentRuntime>> {
        Some(Arc::new(Worker { vault, ..self.clone() }))
    }

    async fn start(&self, request: RunRequest) -> Result<RunHandle, RuntimeError> {
        let has_value = !self.vault.0.is_empty();
        let delegate = has_value && !self.delegated.swap(true, Ordering::SeqCst);
        let (tx, rx) = mpsc::channel(16);
        let (input_tx, _input_rx) = mpsc::channel(1);
        let (cancel_tx, _cancel_rx) = tokio::sync::watch::channel(false);
        let (root, run) = (self.root.clone(), request.run_id);
        tokio::spawn(async move {
            let _ = tx.send(RuntimeEvent::Started).await;
            if has_value {
                let bridge = BridgeClient::connect(&root).await.unwrap();
                let _ = bridge.employee_act(run, EmployeeTaskOp::VaultList { project: None }).await;
                if delegate {
                    let part = EmployeeTaskOp::Delegate { employee: "Bridge".into(), title: "A part".into(), goal: "Do the part".into(), criteria: vec![], project: None };
                    let _ = bridge.employee_act(run, part).await;
                }
            }
            let _ = tx.send(RuntimeEvent::TextDelta("Done.".into())).await;
            let _ = tx.send(RuntimeEvent::Completed).await;
        });
        Ok(RunHandle::new(request.run_id, rx, input_tx, cancel_tx))
    }
}

/// A daemon whose runs are work runs on `Worker` (a Claude Code connection).
async fn work_daemon(dir: &std::path::Path) -> (Daemon, aurelius_client::Client) {
    let runtime = Worker { root: dir.to_path_buf(), vault: VaultEnv::default(), delegated: Arc::default() };
    let daemon = start_with(dir, Arc::new(runtime)).await;
    let client = connect(dir).await;
    free_chart(&client).await;
    let add = AddProvider { provider: ProviderKind::ClaudeCode, display_name: "Claude Code".into(), runtime_path: Some(dir.join("no-claude").display().to_string()), default_model: None };
    let CommandResult::ProviderAdded { connection_id } = client.execute(aurelius_client::envelope(Command::AddProvider(add), None)).await.unwrap().result else { panic!() };
    let settings = RunDefaults { connection: Setting::Value(connection_id), model: Setting::Value("sonnet".into()), persona: Setting::Inherit, effort: Setting::Inherit };
    client.execute(aurelius_client::envelope(Command::SetRunDefaults(SetRunDefaults { scope: ConfigScope::Global, purpose: Purpose::Employee, settings }), None)).await.unwrap();
    (daemon, client)
}

#[tokio::test(flavor = "multi_thread")]
async fn the_audit_of_a_tickets_runs_names_the_ticket() {
    let dir = tempfile::tempdir().unwrap();
    let (daemon, client) = work_daemon(dir.path()).await;
    let mut employees = Vec::new();
    for name in ["Forge", "Bridge"] {
        let create = CreateEmployee { name: name.into(), role: "Engineer".into(), title: None, reports_to: None, instructions: None, folder: None };
        let CommandResult::EmployeeCreated { employee_id, .. } = client.execute(aurelius_client::envelope(Command::CreateEmployee(create), None)).await.unwrap().result else { panic!() };
        employees.push(employee_id);
    }
    let CommandResult::ProjectCreated { project_id, .. } = client.execute(aurelius_client::envelope(Command::CreateProject(CreateProject { name: "Shop".into(), description: None, folder: None }), None)).await.unwrap().result else { panic!() };
    client.vault_create(VaultCreateParams { project_id: None, kind: VaultKind::Secret, name: "Deploy key".into(), env_name: "DEPLOY_KEY".into(), description: None, value: VALUE.into() }).await.unwrap();

    let ticket = CreateTicket {
        responsible_user_id: None,
        confirm_secret: false,
        project_id,
        title: "Ship it".into(),
        description: None,
        priority: TicketPriority::default(),
        labels: vec![],
        deadline_ms: None,
        owner_employee_id: Some(employees[0]),
        run: Default::default(),
        files: vec![],
        start: true,
        from_message: None,
    };
    let CommandResult::TicketCreated { ticket_id, .. } = client.create_ticket(ticket).await.unwrap().result else { panic!() };

    // The main task's run and the delegation's run, each with deliver and tool_read rows.
    let rows = |conn: &rusqlite::Connection| -> Vec<(String, String, Option<String>, Option<i64>)> {
        let mut stmt = conn.prepare("SELECT run_id, action, ticket_id, version FROM vault_audit WHERE action IN ('deliver','tool_read') ORDER BY seq").unwrap();
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).unwrap().map(Result::unwrap).collect()
    };
    let mut found = Vec::new();
    for _ in 0..400 {
        found = rows(&db(dir.path()));
        let runs: std::collections::BTreeSet<&String> = found.iter().filter(|r| r.1 == "tool_read").map(|r| &r.0).collect();
        if runs.len() >= 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let runs: std::collections::BTreeSet<&String> = found.iter().map(|r| &r.0).collect();
    assert!(runs.len() >= 2, "the main run and the delegation's run: {found:?}");
    for action in ["deliver", "tool_read"] {
        assert!(found.iter().any(|r| r.1 == action), "{action}: {found:?}");
    }
    let ticket_text = ticket_id.to_string();
    assert!(found.iter().all(|r| r.2.as_deref() == Some(ticket_text.as_str())), "every row names the ticket: {found:?}");
    assert!(found.iter().all(|r| r.3 == Some(1)), "and the version of the value (SEC1 m3): {found:?}");
    // And the API shows it.
    let audit = client.vault_audit(None, true, None, None, None).await.unwrap();
    let employee_rows: Vec<_> = audit.rows.iter().filter_map(|r| match &r.actor {
        aurelius_domain::vault::VaultActor::Employee { ticket_id, name, .. } => Some((*ticket_id, name.clone())),
        _ => None,
    }).collect();
    assert!(!employee_rows.is_empty() && employee_rows.iter().all(|(t, _)| *t == Some(ticket_id)), "{employee_rows:?}");
    // The employees' names come with the page (R1 m1).
    let names: std::collections::BTreeSet<&str> = employee_rows.iter().map(|(_, n)| n.as_str()).collect();
    assert_eq!(names, ["Bridge", "Forge"].into_iter().collect(), "{employee_rows:?}");
    drop(client);
    stop(dir.path(), daemon).await;
}

const UNFILLED: &str = "sk-test-FAKE-unfilled-redactor-0000";

/// SEC2 m1: a key that becomes readable after the start (0644 made 0600)
/// while the redactor cannot be filled (here a stored value it cannot read)
/// is not used: a run gets nothing, reveal is refused; once the redactor
/// fills, the next call has the key again.
#[tokio::test(flavor = "multi_thread")]
async fn a_redactor_that_cannot_be_filled_delivers_nothing() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let (daemon, client) = work_daemon(dir.path()).await;
    let item = client.vault_create(VaultCreateParams { project_id: None, kind: VaultKind::Secret, name: "Unfilled".into(), env_name: "UNFILLED_TOKEN".into(), description: None, value: UNFILLED.into() }).await.unwrap();
    drop(client);
    stop(dir.path(), daemon).await;

    let key = aurelius_host::secrets::file_in(&dir.path().join("secrets"), "vault-master");
    std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o644)).unwrap();
    // A previous value of an unknown kind: every read of all values fails,
    // the run's own items (the current ones) still read.
    let conn = rusqlite::Connection::open(dir.path().join("data/aurelius.sqlite")).unwrap();
    conn.execute_batch("PRAGMA ignore_check_constraints = ON").unwrap();
    conn.execute(
        "INSERT INTO vault_item_versions(item_id, kind, version, ciphertext, nonce, key_id, replaced_at_ms) VALUES (?1, 'bogus', 99, x'01', x'02', 'k0', 1)",
        [item.id.to_string()],
    )
    .unwrap();
    drop(conn);
    aurelius_runtime::redact::install_set(&dir.path().display().to_string(), Vec::new());
    let runtime = Worker { root: dir.path().to_path_buf(), vault: VaultEnv::default(), delegated: Arc::default() };
    let daemon = start_with(dir.path(), Arc::new(runtime)).await;
    std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o600)).unwrap();
    let client = connect(dir.path()).await;

    let (conversation, message, _) = send_new(&client, "work").await;
    let mut done = false;
    for _ in 0..400 {
        let messages = client.messages(conversation).await.unwrap().messages;
        if messages.iter().any(|m| m.reply_to == Some(message) && m.status == aurelius_domain::conversation::MessageStatus::Complete) {
            done = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(done, "the run ends");
    let delivered: i64 = db(dir.path()).query_row("SELECT COUNT(*) FROM vault_audit WHERE action IN ('deliver', 'tool_read')", [], |r| r.get(0)).unwrap();
    assert_eq!(delivered, 0, "the run got nothing from the vault");
    let aurelius_client::ClientError::Protocol(err) = client.vault_reveal(item.id, aurelius_domain::vault::RevealPurpose::Show).await.unwrap_err() else { panic!() };
    assert_eq!(err.code, aurelius_domain::ErrorCode::VaultUndecryptable);
    assert!(aurelius_runtime::redact::current().names_in(UNFILLED).is_empty());

    // The value readable again: the next call fills the redactor and has the key.
    let conn = rusqlite::Connection::open(dir.path().join("data/aurelius.sqlite")).unwrap();
    conn.busy_timeout(Duration::from_secs(5)).unwrap();
    conn.execute_batch("DROP TRIGGER vault_item_versions_no_delete; DELETE FROM vault_item_versions WHERE kind = 'bogus'").unwrap();
    drop(conn);
    assert_eq!(client.vault_reveal(item.id, aurelius_domain::vault::RevealPurpose::Show).await.unwrap().value, UNFILLED);
    assert_eq!(aurelius_runtime::redact::current().names_in(UNFILLED), vec!["Unfilled".to_owned()]);
    drop(client);
    stop(dir.path(), daemon).await;
}

/// SEC1 m4: an item whose env name runs may no longer have (an older rule)
/// is told about in the Team chat once, not at every run.
#[tokio::test(flavor = "multi_thread")]
async fn a_refused_name_is_told_once() {
    let dir = tempfile::tempdir().unwrap();
    let (daemon, client) = work_daemon(dir.path()).await;
    let item = client.vault_create(VaultCreateParams { project_id: None, kind: VaultKind::Text, name: "Old rule".into(), env_name: "OLD_RULE".into(), description: None, value: "some text".into() }).await.unwrap();
    let conn = rusqlite::Connection::open(dir.path().join("data/aurelius.sqlite")).unwrap();
    conn.busy_timeout(Duration::from_secs(5)).unwrap();
    conn.execute("UPDATE vault_items SET env_name = 'NODE_OPTIONS' WHERE id = ?1", [item.id.to_string()]).unwrap();
    drop(conn);
    for _ in 0..2 {
        let (conversation, message, _) = send_new(&client, "work").await;
        for _ in 0..400 {
            let messages = client.messages(conversation).await.unwrap().messages;
            if messages.iter().any(|m| m.reply_to == Some(message) && m.status == aurelius_domain::conversation::MessageStatus::Complete) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }
    let told = client.group_chat().await.unwrap().messages.iter().filter(|m| m.text.contains("did not get NODE_OPTIONS")).count();
    assert_eq!(told, 1);
    drop(client);
    stop(dir.path(), daemon).await;
}
