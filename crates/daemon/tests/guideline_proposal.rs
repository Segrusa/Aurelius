//! `propose_guideline` over the bridge channel (B5, SEC0 E10): a text-only run
//! (no tools, so no bridge of its own) is refused before anything is stored. The
//! same holds for reading guidelines (`list_guidelines`, `read_guideline`, GL-15). The
//! working run's way, with redaction (T15), is in `vault_canary.rs`; the rules
//! themselves are `aurelius_application::guideline_proposal`'s tests.

mod common;

use aurelius_client::bridge::BridgeClient;
use aurelius_client::envelope;
use aurelius_domain::employee::{CreateEmployee, SendEmployeeMessage};
use aurelius_domain::{Command, CommandResult};
use aurelius_protocol::EmployeeTaskOp;
use common::*;
use std::time::Duration;

#[tokio::test(flavor = "multi_thread")]
async fn a_text_only_run_of_an_employee_cannot_propose_a_guideline() {
    let dir = tempfile::tempdir().unwrap();
    // The fake runtime is not Claude Code: its runs are text-only.
    let daemon = start_with_delay(dir.path(), 100).await;
    let client = connect(dir.path()).await;
    let create = CreateEmployee { access_level: None, tool_policy: None, start_profile: None, description: None, name: "Ann".into(), role: "Dev".into(), title: None, reports_to: None, instructions: None, folder: None };
    let CommandResult::EmployeeCreated { employee_id, conversation_id } = client.execute(envelope(Command::CreateEmployee(create), None)).await.unwrap().result else { panic!() };
    let send = SendEmployeeMessage { confirm_secret: false, employee_id, text: "houd me wakker".into(), conversation_id, images: vec![], files: vec![], interrupt: false };
    let CommandResult::EmployeeMessageSent { run_id, .. } = client.execute(envelope(Command::SendEmployeeMessage(send), None)).await.unwrap().result else { panic!() };
    for _ in 0..200 {
        // The run row is made when its turn comes.
        let status: String = db(dir.path()).query_row("SELECT status FROM runs WHERE id = ?1", [run_id.to_string()], |r| r.get(0)).unwrap_or_default();
        if matches!(status.as_str(), "starting" | "running" | "waiting") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }

    let bridge = BridgeClient::connect(dir.path()).await.unwrap();
    let op = EmployeeTaskOp::ProposeGuideline { guideline: None, title: "Small steps".into(), text: "Work in small steps.".into(), reason: "Twice".into(), evidence: vec!["anything".into()] };
    let (ok, text) = bridge.employee_act(run_id, op).await.unwrap();
    assert!(!ok, "{text}");
    assert_eq!(text, "Only a working run of an employee can propose a guideline.");
    let stored: i64 = db(dir.path()).query_row("SELECT COUNT(*) FROM guideline_proposals", [], |r| r.get(0)).unwrap();
    assert_eq!(stored, 0);
    let events: i64 = db(dir.path()).query_row("SELECT COUNT(*) FROM events WHERE type = 'guideline.proposed'", [], |r| r.get(0)).unwrap();
    assert_eq!(events, 0);

    // GL-15: nor does it read guidelines (E10, as propose_guideline).
    for op in [EmployeeTaskOp::ListGuidelines {}, EmployeeTaskOp::ReadGuideline { guideline: "Rust".into() }] {
        let (ok, text) = bridge.employee_act(run_id, op).await.unwrap();
        assert!(!ok, "{text}");
        assert_eq!(text, "Only a working run of an employee can read guidelines.");
    }

    stop(dir.path(), daemon).await;
}
