//! Sentinel L1 and D3: a run of the whole sign-in and Connect flow with known
//! secrets (a password, a session token, Claude's sign-in state and two codes),
//! then a search through the daemon's log (every level), the events, the
//! command journal, every other table, a backup and a project export: none of
//! them may hold any of it, and the backup and the export hold no login folder.
//! Its own test binary: it catches the log of the whole process.
#![cfg(unix)]

mod common;
use common::*;

use aurelius_client::SessionPush;
use aurelius_domain::connection::{AddProvider, ConfigScope, ProviderKind, Purpose, RunDefaults, SetRunDefaults};
use aurelius_domain::employee::Setting;
use aurelius_domain::project::CreateProject;
use aurelius_domain::{Command, CommandResult};
use std::io::Write;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const PASSWORD: &str = "Canary-Password-C0FFEE-4242";
const GOOD_CODE: &str = "good-CANARYCODE-77aa#FAKESTATE";
const WRONG_CODE: &str = "CANARYWRONG-99bb#FAKESTATE";
/// In the stand-in's sign-in link (`state=FAKESTATE`).
const STATE: &str = "FAKESTATE";

/// Everything the daemon logs, at every level.
#[derive(Clone, Default)]
struct Log(Arc<Mutex<Vec<u8>>>);

impl Write for Log {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Every file under `dir`: (path, contents).
fn files(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d).into_iter().flatten().filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.push((path.display().to_string(), std::fs::read(&path).unwrap_or_default()));
            }
        }
    }
    out
}

fn every_text(root: &Path) -> String {
    let conn = db(root);
    let tables: Vec<String> = conn.prepare("SELECT name FROM sqlite_master WHERE type = 'table'").unwrap().query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
    let mut all = String::new();
    for table in tables {
        let Ok(mut stmt) = conn.prepare(&format!("SELECT * FROM \"{table}\"")) else { continue };
        let n = stmt.column_count();
        for row in stmt.query_map([], |r| Ok((0..n).map(|i| r.get::<_, rusqlite::types::Value>(i).ok()).collect::<Vec<_>>())).unwrap().flatten() {
            for value in row.into_iter().flatten() {
                if let rusqlite::types::Value::Text(t) = value {
                    all.push_str(&t);
                    all.push('\n');
                }
            }
        }
    }
    all
}

async fn prompt(pushes: &mut tokio::sync::mpsc::UnboundedReceiver<SessionPush>) -> aurelius_protocol::ClaudeLoginPrompt {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Some(SessionPush::ClaudeLoginPrompt(p)) = pushes.recv().await {
                return p;
            }
        }
    })
    .await
    .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn no_password_token_or_code_is_kept_anywhere_and_no_login_goes_into_a_backup() {
    let log = Log::default();
    let writer = log.clone();
    tracing_subscriber::fmt().with_env_filter("trace").with_writer(move || writer.clone()).with_ansi(false).init();

    let dir = tempfile::tempdir().unwrap();
    let daemon = start_real(dir.path()).await;
    let admin = connect_as(dir.path(), "alex").await;
    // A board member with a known password, signed in: a known session token.
    admin.user_create("robin", PASSWORD).await.unwrap();
    let robin = connect_anonymous(dir.path()).await;
    let token = robin.user_login("robin", PASSWORD, "test").await.unwrap().session_token;
    assert!(robin.user_login("robin", "Canary-Wrong-Password-1", "test").await.is_err());
    let mut pushes = robin.session_pushes();

    // The stand-in claude, and Robin's login connected: a wrong code, then a good one.
    let bin = dir.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let program = bin.join("claude");
    std::fs::copy(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/fake-claude"), &program).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let add = AddProvider { provider: ProviderKind::ClaudeCode, display_name: "Claude Code".into(), runtime_path: Some(program.display().to_string()), default_model: None };
    let CommandResult::ProviderAdded { connection_id } = admin.execute(aurelius_client::envelope(Command::AddProvider(add), None)).await.unwrap().result else { panic!() };
    let settings = RunDefaults { connection: Setting::Value(connection_id), model: Setting::Inherit, persona: Setting::Inherit, effort: Setting::Inherit };
    admin.execute(aurelius_client::envelope(Command::SetRunDefaults(SetRunDefaults { scope: ConfigScope::Global, purpose: Purpose::Employee, settings }), None)).await.unwrap();
    let link = robin.claude_login_link().await.unwrap();
    assert!(prompt(&mut pushes).await.url.unwrap().contains(STATE));
    robin.claude_login_input(&link, WRONG_CODE).await.unwrap();
    assert!(prompt(&mut pushes).await.error.is_some());
    let link = robin.claude_login_link().await.unwrap();
    prompt(&mut pushes).await;
    robin.claude_login_input(&link, GOOD_CODE).await.unwrap();
    for _ in 0..200 {
        if robin.claude_login().await.unwrap().status == aurelius_domain::user::ClaudeLoginStatus::Linked {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    // What a Claude login holds on disk (stands in for the token claude keeps there).
    let robin_id = robin.user_whoami().await.unwrap().user.unwrap().id;
    let folder = dir.path().join("logins").join(robin_id.to_string());
    std::fs::write(folder.join(".credentials.json"), r#"{"claudeAiOauth":{"accessToken":"sk-ant-oat01-CANARYTOKEN"}}"#).unwrap();

    // D3: a backup and a project export.
    let CommandResult::ProjectCreated { project_id, .. } =
        admin.execute(aurelius_client::envelope(Command::CreateProject(CreateProject { name: "Atlas".into(), description: None, folder: None }), None)).await.unwrap().result
    else {
        panic!()
    };
    admin.create_backup().await.unwrap();
    let (markdown, json) = admin.export_project(project_id).await.unwrap();
    drop((admin, robin));
    stop(dir.path(), daemon).await;

    let canaries = [PASSWORD, "Canary-Wrong-Password-1", token.as_str(), GOOD_CODE, WRONG_CODE, STATE, "CANARYTOKEN", "oauth/authorize"];
    // The log of every level.
    let log = String::from_utf8_lossy(&log.0.lock().unwrap()).into_owned();
    assert!(log.contains("Claude sign-in"), "the sign-in is logged (by its states)");
    for canary in canaries {
        assert!(!log.contains(canary), "the log holds {canary}");
    }
    // The events, the command journal and every other table.
    let all = every_text(dir.path());
    assert!(all.contains("claude_login.link"), "the journal has the link's receipt");
    for canary in canaries {
        assert!(!all.contains(canary), "the database holds {canary}");
    }
    // The backup: no login folder, none of the secrets.
    let backups = files(&dir.path().join("backups"));
    assert!(!backups.is_empty());
    for (path, contents) in &backups {
        assert!(!path.contains("logins") && !path.contains(".credentials"), "{path} in a backup");
        let text = String::from_utf8_lossy(contents);
        for canary in canaries {
            assert!(!text.contains(canary), "{path} holds {canary}");
        }
    }
    // The export: no login, no secret.
    for path in [markdown, json] {
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains("logins/") && !text.contains(".credentials"), "{path}");
        for canary in canaries {
            assert!(!text.contains(canary), "{path} holds {canary}");
        }
    }
    // A fresh start carries only the server's remote settings over, never a login.
    assert_eq!(aurelius_storage::fresh_start::CONFIG_FILES, ["remote.json"]);
}
