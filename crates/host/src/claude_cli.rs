//! The Claude Code CLI, run on the server's computer (argument lists only, never a shell).

use aurelius_protocol::{ErrorCode, ProtocolError};
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;

fn internal(e: impl ToString) -> ProtocolError {
    ProtocolError::new(ErrorCode::Internal, e.to_string())
}

/// Run `claude <args>` and return its stdout. A failure comes back as the CLI's
/// own words (stderr, else stdout), shortened.
pub async fn claude(args: &[&str], timeout: Duration) -> Result<String, ProtocolError> {
    let exe = aurelius_runtime::claude_code::locate().ok_or_else(|| internal("Claude Code is not installed on the server's computer."))?;
    let mut command = Command::new(exe);
    command.args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    let label = args.iter().take(3).copied().collect::<Vec<_>>().join(" ");
    let output = match tokio::time::timeout(timeout, command.output()).await {
        Err(_) => return Err(internal(format!("'claude {label}' took longer than {} seconds and was stopped.", timeout.as_secs()))),
        Ok(Err(e)) => return Err(internal(format!("could not run Claude Code: {e}"))),
        Ok(Ok(output)) => output,
    };
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    if output.status.success() {
        return Ok(stdout);
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let text = if stderr.trim().is_empty() { stdout.as_str() } else { &stderr };
    Err(internal(format!("'claude {label}' failed: {}", readable(text))))
}

/// The tail of a CLI message without control characters (colour codes, spinners).
fn readable(text: &str) -> String {
    let clean: String = text.chars().filter(|c| !c.is_control() || *c == '\n').collect();
    let clean = clean.trim();
    let tail: String = clean.chars().rev().take(600).collect::<Vec<_>>().into_iter().rev().collect();
    if tail.is_empty() { "no message".into() } else { tail }
}
