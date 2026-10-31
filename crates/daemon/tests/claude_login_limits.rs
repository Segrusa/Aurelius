//! The time limits of a sign-in to Claude (Sentinel SEC0 C12, C13), shortened
//! for the test through the daemon's environment (its own test binary, as the
//! environment is the process's): no link shown in time gives "Show command",
//! no code in time ends it; either way the process is gone.
#![cfg(unix)]

mod common;
use common::*;

use aurelius_client::SessionPush;
use aurelius_domain::connection::{AddProvider, ConfigScope, ProviderKind, Purpose, RunDefaults, SetRunDefaults};
use aurelius_domain::employee::Setting;
use aurelius_domain::{Command, CommandResult};
use std::path::Path;
use std::time::Duration;

async fn error_of(pushes: &mut tokio::sync::mpsc::UnboundedReceiver<SessionPush>) -> Option<String> {
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            if let Some(SessionPush::ClaudeLoginPrompt(p)) = pushes.recv().await {
                if !p.asks_code {
                    return p.error;
                }
            }
        }
    })
    .await
    .expect("the sign-in ends")
}

async fn gone(bin: &Path) {
    let pid: u32 = std::fs::read_to_string(bin.join("login-pids")).unwrap().lines().last().unwrap().trim().parse().unwrap();
    for _ in 0..100 {
        if !Path::new(&format!("/proc/{pid}")).exists() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the sign-in process {pid} is still there");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_link_not_shown_in_time_gives_show_command_and_a_code_not_given_in_time_ends_it() {
    // SAFETY: set before the daemon starts; no other thread of this test binary reads the environment yet.
    unsafe {
        std::env::set_var("AURELIUS_CLAUDE_LINK_RECOGNIZE_MS", "800");
        std::env::set_var("AURELIUS_CLAUDE_LINK_TIMEOUT_MS", "2500");
    }
    let dir = tempfile::tempdir().unwrap();
    let daemon = start_real(dir.path()).await;
    let alex = connect_as(dir.path(), "alex").await;
    let mut pushes = alex.session_pushes();
    let bin = dir.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let program = bin.join("claude");
    std::fs::copy(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/fake-claude"), &program).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let add = AddProvider { provider: ProviderKind::ClaudeCode, display_name: "Claude Code".into(), runtime_path: Some(program.display().to_string()), default_model: None };
    let CommandResult::ProviderAdded { connection_id } = alex.execute(aurelius_client::envelope(Command::AddProvider(add), None)).await.unwrap().result else { panic!() };
    let settings = RunDefaults { connection: Setting::Value(connection_id), model: Setting::Inherit, persona: Setting::Inherit, effort: Setting::Inherit };
    alex.execute(aurelius_client::envelope(Command::SetRunDefaults(SetRunDefaults { scope: ConfigScope::Global, purpose: Purpose::Employee, settings }), None)).await.unwrap();

    // C12: Claude prints nothing recognizable: killed, and the client falls back to "Show command".
    std::fs::write(bin.join("login-mode"), "silent").unwrap();
    alex.claude_login_link().await.unwrap();
    let error = error_of(&mut pushes).await.expect("a reason");
    assert!(error.contains("Show command"), "{error}");
    gone(&bin).await;

    // C13: the link is shown but no code comes: it ends after the time-out.
    std::fs::remove_file(bin.join("login-mode")).unwrap();
    alex.claude_login_link().await.unwrap();
    assert_eq!(error_of(&mut pushes).await.as_deref(), Some("The sign-in took longer than 10 minutes. Start again."));
    gone(&bin).await;
    assert_eq!(alex.claude_login().await.unwrap().link_id, None);

    // C13: the daemon stops: a running sign-in ends with it.
    alex.claude_login_link().await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    drop(alex);
    stop(dir.path(), daemon).await;
    gone(&bin).await;
}
