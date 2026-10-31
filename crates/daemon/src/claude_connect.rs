//! Connecting a board member's own Claude login (MU3-09, Sentinel SEC0 C1–C16).
//!
//! `claude_login.link` runs the unmodified `claude auth login --claudeai` for
//! `<data>/logins/<user_id>/` in a PTY of its own (no shell, an allowlisted
//! environment, `BROWSER=/bin/false`). Aurelius reads only two things from it:
//! Anthropic's sign-in link and the question for the code. Those go to the one
//! connection that asked (`ServerMessage::ClaudeLoginPrompt`), never into an
//! event, the database or a log. The board member signs in in their own browser
//! and sends the code back (`claude_login.input`, a session method); it is
//! checked and written into the PTY once. Whether it worked is what `claude`
//! says itself: its exit code and `claude auth status --json`. Aurelius never
//! opens `.credentials.json`.
//!
//! A link ends after 10 minutes, and at once when its connection goes, on
//! `claude_login.cancel`, on sign-out and when the daemon stops: the whole
//! process group is ended (SIGTERM, SIGKILL after 2 s) and waited for; when
//! `claude` ended by itself, what is left of its group is killed too. One link
//! per board member at a time (C2: a new one ends the one before, which hears
//! "replaced"), at most 5 in 10 minutes.

use crate::Shared;
use aurelius_domain::UserId;
use aurelius_protocol::{ClaudeLoginPrompt, ErrorCode, ProtocolError, ServerMessage};
use aurelius_runtime::claude_code::{ClaudeCodeRuntime, ClaudeLogin, login::CONFIG_DIR_ENV};
use std::collections::{HashMap, VecDeque};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, watch};

/// The only sign-in link passed on (C12): Anthropic's own authorize page.
pub const SIGN_IN_PREFIX: &str = "https://claude.com/cai/oauth/authorize?";
/// The line that holds the link (CLI 2.1.312), and the question for the code.
const VISIT: &str = "If the browser didn't open, visit:";
const PROMPT: &str = "Paste code here if prompted >";
/// A sign-in link is at most this long (C12).
pub const URL_MAX: usize = 2048;
/// A code is 1–512 bytes of these characters (C7).
pub const CODE_MAX: usize = 512;
/// Of the output, at most this much is looked at (C9).
const BUFFER_MAX: usize = 64 * 1024;
/// Of the output after the code, at most this much is looked at, for a refusal.
const AFTER_CODE_MAX: usize = 4 * 1024;
/// What the CLI says to a code it refuses (2.1.312, QA2 `real-cli-pty-*.txt`):
/// a well-formed wrong one ends it ("Login failed: …"); one that is not whole
/// ("Invalid code. …") leaves it waiting for another, so Aurelius ends it.
const REFUSALS: &[&str] = &["Login failed:", "Invalid code."];
/// Without the link and the question by then, the link ends: "Show command" (C12).
const RECOGNIZE: Duration = Duration::from_secs(15);
/// A link ends after this long (C13).
const TIMEOUT: Duration = Duration::from_secs(10 * 60);
/// Links per board member in `WINDOW` (C16).
const LINKS_PER_WINDOW: usize = 5;
const WINDOW: Duration = Duration::from_secs(10 * 60);
/// How long unlink waits for a running sign-in to end (R2 m3).
const UNLINK_WAIT: Duration = Duration::from_secs(3);
/// The PTY is wide, so the link is never wrapped (C12).
const COLUMNS: u16 = 512;

/// What a board member reads when a link ends without a login (C14): never Claude's raw output.
pub const CODE_REFUSED: &str = "The code was not accepted. Start again.";
pub const NOT_RECOGNIZED: &str = "Aurelius could not read Claude's sign-in link. Use Show command to connect in a terminal on the server.";
pub const TIMED_OUT: &str = "The sign-in took longer than 10 minutes. Start again.";
pub const NOT_STARTED: &str = "Claude could not be started to sign in. Use Show command to connect in a terminal on the server.";

/// The variables of the daemon's environment the login process may get (C11).
const ENV_ALLOWLIST: &[&str] = &["PATH", "HOME", "USER", "LOGNAME", "LANG", "LC_ALL", "TMPDIR"];

/// The environment of `claude auth login` for `dir` (C11): the allowlist from the
/// daemon's environment (`parent`), a terminal, the login folder and no browser.
/// Never DISPLAY, a session bus, an API key or token, or another login folder.
pub fn login_env(parent: impl Fn(&str) -> Option<OsString>, dir: &Path) -> Vec<(String, OsString)> {
    let mut env: Vec<(String, OsString)> = ENV_ALLOWLIST.iter().filter_map(|k| parent(k).map(|v| (k.to_string(), v))).collect();
    env.push(("TERM".into(), "xterm-256color".into()));
    env.push((CONFIG_DIR_ENV.into(), dir.as_os_str().to_owned()));
    env.push(("BROWSER".into(), "/bin/false".into()));
    // portable-pty always sets SHELL; it must point at nothing that runs.
    env.push(("SHELL".into(), "/bin/false".into()));
    env
}

/// The output without its terminal codes (CSI, OSC up to BEL or ST, other escapes).
pub fn strip_ansi(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        match chars.next() {
            // CSI: parameters and intermediates up to a final byte @–~.
            Some('[') => {
                for c in chars.by_ref() {
                    if ('@'..='~').contains(&c) {
                        break;
                    }
                }
            }
            // OSC: up to BEL or ESC \.
            Some(']') => {
                while let Some(c) = chars.next() {
                    if c == '\u{7}' {
                        break;
                    }
                    if c == '\u{1b}' && chars.peek() == Some(&'\\') {
                        chars.next();
                        break;
                    }
                }
            }
            // A two-character escape (ESC =, ESC (B, …).
            Some('(' | ')') => {
                chars.next();
            }
            _ => {}
        }
    }
    out
}

/// Whether `url` may go to a client (C12): Anthropic's authorize page, no
/// control characters or spaces, at most 2 KiB.
pub fn valid_url(url: &str) -> bool {
    url.starts_with(SIGN_IN_PREFIX) && url.len() <= URL_MAX && !url.chars().any(|c| c.is_control() || c.is_whitespace())
}

/// The sign-in link, once the output (without its codes) shows it and asks for
/// the code; none until both are there or when the link is not a valid one.
pub fn recognize(text: &str) -> Option<String> {
    if !text.contains(PROMPT) {
        return None;
    }
    let line = text.lines().find_map(|l| l.trim_start().strip_prefix(VISIT))?;
    let url = line.split_whitespace().next()?;
    valid_url(url).then(|| url.to_owned())
}

/// Whether a pasted code may go into the PTY (C7): 1–512 bytes of `[A-Za-z0-9._~#-]`.
pub fn valid_code(text: &str) -> bool {
    (1..=CODE_MAX).contains(&text.len()) && text.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'~' | b'#' | b'-'))
}

/// Whether a code is whole: Claude's code page gives `<code>#<state>` (QA2 M1);
/// without the `#` the CLI only says "Invalid code" and keeps waiting.
pub fn whole_code(text: &str) -> bool {
    text.split_once('#').is_some_and(|(code, state)| !code.is_empty() && !state.is_empty())
}

/// Whether the output after the code (without its terminal codes) refuses it.
pub fn refused(text: &str) -> bool {
    REFUSALS.iter().any(|r| text.contains(r))
}

/// 128 random bits (C2), never a time-ordered id.
fn new_link_id() -> String {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).expect("OS random source");
    hex::encode(bytes)
}

/// Overwrite what was read before it is dropped (C9).
fn wipe(buf: &mut Vec<u8>) {
    for b in buf.iter_mut() {
        // SAFETY: a plain write to an initialized byte of this vector, kept by `volatile`.
        unsafe { std::ptr::write_volatile(b, 0) };
    }
    buf.clear();
}

/// A code on its way into the PTY; wiped when dropped, never printed.
struct Code(Vec<u8>);

impl Drop for Code {
    fn drop(&mut self) {
        wipe(&mut self.0);
    }
}

/// One running link.
struct Link {
    id: String,
    /// The connection that started it: only it gets the prompt and may answer (C4, C5).
    conn: u64,
    /// The session behind that connection (sign-out ends the link).
    session: Option<String>,
    code: Option<mpsc::Sender<Code>>,
    stop: watch::Sender<Option<&'static str>>,
}

/// Every running link and when each board member started theirs.
#[derive(Default)]
pub struct Links {
    running: Mutex<HashMap<UserId, Link>>,
    started: Mutex<HashMap<UserId, VecDeque<Instant>>>,
}

impl Links {
    /// The id of `user`'s running link.
    pub fn running(&self, user: UserId) -> Option<String> {
        self.running.lock().unwrap().get(&user).map(|l| l.id.clone())
    }

    /// May `user` start a link now (five in ten minutes)? `count`: and count it,
    /// under the same lock, so links started at once cannot all pass (SEC2 m1).
    fn check(&self, user: UserId, now: Instant, count: bool) -> Result<(), ProtocolError> {
        let mut started = self.started.lock().unwrap();
        let times = started.entry(user).or_default();
        while times.front().is_some_and(|t| now.duration_since(*t) >= WINDOW) {
            times.pop_front();
        }
        if times.len() >= LINKS_PER_WINDOW {
            let wait = WINDOW.saturating_sub(now.duration_since(*times.front().expect("five")));
            let mut err = ProtocolError::new(ErrorCode::TooManyAttempts, "Too many sign-ins to Claude in a short time. Try again later.");
            err.retry_after_ms = Some(wait.as_millis() as u64);
            return Err(err);
        }
        if count {
            times.push_back(now);
        }
        Ok(())
    }

    fn begin(&self, user: UserId, conn: u64, session: Option<String>, now: Instant) -> Result<(String, mpsc::Receiver<Code>, watch::Receiver<Option<&'static str>>), ProtocolError> {
        self.check(user, now, true)?;
        // One at a time (C2): the link before ends, explicitly.
        self.stop(|u, _| *u == user, "replaced");
        let (code_tx, code_rx) = mpsc::channel(1);
        let (stop_tx, stop_rx) = watch::channel(None);
        let id = new_link_id();
        self.running.lock().unwrap().insert(user, Link { id: id.clone(), conn, session, code: Some(code_tx), stop: stop_tx });
        Ok((id, code_rx, stop_rx))
    }

    /// The code for the link `link_id` of `user`, from connection `conn` (C5, C7):
    /// all three must match, else `NotFound` without saying which; once only.
    /// A code that is not whole (no `#`) is refused before it is written: the
    /// input is taken and the link ends exactly as when Claude refuses a code
    /// (its prompt says `CODE_REFUSED`), so a client needs nothing new (QA2 M1).
    pub fn input(&self, user: UserId, conn: u64, link_id: &str, text: &str) -> Result<(), ProtocolError> {
        let not_found = || ProtocolError::new(ErrorCode::NotFound, "This sign-in to Claude is not running (any more).");
        let sender = {
            let mut running = self.running.lock().unwrap();
            let link = running.get_mut(&user).filter(|l| l.id == link_id && l.conn == conn).ok_or_else(not_found)?;
            // Checked before it is taken: a wrong format can be fixed and sent again.
            if !valid_code(text) {
                return Err(ProtocolError::new(ErrorCode::Validation, "That is not a code Claude gives: paste only the code."));
            }
            if link.code.is_none() {
                return Err(not_found());
            }
            if !whole_code(text) {
                link.code = None;
                tracing::info!(link = %link.id, %user, state = "refused", "Claude sign-in ends");
                let _ = link.stop.send(Some("refused"));
                return Ok(());
            }
            link.code.take().ok_or_else(not_found)?
        };
        sender.try_send(Code(text.as_bytes().to_vec())).map_err(|_| not_found())
    }

    /// End `user`'s link `link_id` (`claude_login.cancel`); false when it is not running.
    pub fn cancel(&self, user: UserId, link_id: &str) -> bool {
        self.stop(|u, l| *u == user && l.id == link_id, "cancelled") > 0
    }

    /// End the link of `user`, whatever it is (unlink, disabled).
    pub fn cancel_user(&self, user: UserId) {
        self.stop(|u, _| *u == user, "cancelled");
    }

    /// The connection went: its link goes with it.
    pub fn cancel_conn(&self, conn: u64) {
        self.stop(|_, l| l.conn == conn, "disconnected");
    }

    /// The session ended (signed out, revoked).
    pub fn cancel_session(&self, session: &str) {
        self.stop(|_, l| l.session.as_deref() == Some(session), "signed out");
    }

    /// The daemon stops.
    pub fn cancel_all(&self) {
        self.stop(|_, _| true, "shutdown");
    }

    fn stop(&self, matches: impl Fn(&UserId, &Link) -> bool, why: &'static str) -> usize {
        let running = self.running.lock().unwrap();
        let mut n = 0;
        for (user, link) in running.iter().filter(|(u, l)| matches(u, l)) {
            tracing::info!(link = %link.id, %user, state = why, "Claude sign-in ends");
            let _ = link.stop.send(Some(why));
            n += 1;
        }
        n
    }

    /// Wait (at most `max`) until `user` has no link running any more: its
    /// process is gone (R2 m3). False when it is still there after `max`.
    pub async fn stopped(&self, user: UserId, max: Duration) -> bool {
        let until = tokio::time::Instant::now() + max;
        while self.running(user).is_some() {
            if tokio::time::Instant::now() >= until {
                return false;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        true
    }

    fn end(&self, user: UserId, link_id: &str) {
        let mut running = self.running.lock().unwrap();
        if running.get(&user).is_some_and(|l| l.id == link_id) {
            running.remove(&user);
        }
    }
}

/// The `claude` program of the Claude Code connection (the one runs use).
async fn claude_program(shared: &Arc<Shared>) -> Result<PathBuf, ProtocolError> {
    let store = shared.store().clone();
    let row = tokio::task::spawn_blocking(move || store.read(|c| aurelius_storage::connections::find_by_provider(c, aurelius_domain::connection::ProviderKind::ClaudeCode)))
        .await
        .map_err(|e| ProtocolError::new(ErrorCode::Internal, e.to_string()))?
        .map_err(|e| ProtocolError::new(ErrorCode::StorageUnavailable, e.to_string()))?;
    let path = row.filter(|r| r.status != "disconnected").and_then(|r| r.runtime_path).map(PathBuf::from);
    match path {
        Some(path) if path.is_absolute() && path.is_file() => Ok(path),
        _ => Err(ProtocolError::new(ErrorCode::Validation, "There is no Claude Code connection on this server. Add one in Settings → Connections first.")),
    }
}

/// `claude_login.link`: start the sign-in for the session's own login (C1).
/// The prompt goes to `out` only; the answer is the link's id.
pub async fn link(shared: &Arc<Shared>, user: UserId, conn: u64, session: Option<String>, out: mpsc::Sender<ServerMessage>) -> Result<String, ProtocolError> {
    shared.claude_links.check(user, Instant::now(), false)?;
    let program = claude_program(shared).await?;
    let login = ClaudeLogin::of(&shared.root, user).map_err(|e| ProtocolError::new(ErrorCode::Internal, format!("the Claude login folder: {}", e.kind())))?;
    let (id, code_rx, stop_rx) = shared.claude_links.begin(user, conn, session, Instant::now())?;
    tracing::info!(link = %id, %user, state = "started", "Claude sign-in");
    let (s, link_id) = (shared.clone(), id.clone());
    let limits = (shared.claude_link_recognize.unwrap_or(RECOGNIZE), shared.claude_link_timeout.unwrap_or(TIMEOUT));
    tokio::spawn(async move {
        let state = drive(&s, &program, &login, &link_id, out.clone(), code_rx, stop_rx, limits).await;
        s.claude_links.end(user, &link_id);
        match state {
            Ok(()) => {}
            Err(error) => {
                let _ = out.send(ServerMessage::ClaudeLoginPrompt(ClaudeLoginPrompt { link_id: link_id.clone(), url: None, asks_code: false, error: Some(error.into()) })).await;
            }
        }
    });
    Ok(id)
}

/// The PTY's output, read on a thread of its own (C9: not the terminal's reader).
fn read_output(mut reader: Box<dyn std::io::Read + Send>, tx: mpsc::Sender<Vec<u8>>) {
    let mut chunk = [0u8; 4096];
    loop {
        match reader.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                if tx.blocking_send(chunk[..n].to_vec()).is_err() {
                    break;
                }
            }
        }
    }
    chunk.iter_mut().for_each(|b| *b = 0);
}

/// End the process group of `pid` (SIGTERM, SIGKILL after 2 s) (C13).
#[cfg(unix)]
async fn end_group(pid: Option<u32>, exited: &mut tokio::sync::oneshot::Receiver<Option<u32>>) -> Option<u32> {
    if let Some(pid) = pid {
        // SAFETY: signalling a process group we started (`setsid` made it its leader).
        unsafe { libc::kill(-(pid as i32), libc::SIGTERM) };
        if let Ok(Ok(code)) = tokio::time::timeout(Duration::from_secs(2), &mut *exited).await {
            return code;
        }
        // SAFETY: as above.
        unsafe { libc::kill(-(pid as i32), libc::SIGKILL) };
    }
    exited.await.ok().flatten()
}

#[cfg(not(unix))]
async fn end_group(_pid: Option<u32>, exited: &mut tokio::sync::oneshot::Receiver<Option<u32>>) -> Option<u32> {
    exited.await.ok().flatten()
}

/// `claude` ended by itself: kill what is left of its group (children that
/// ignored the hang-up), R2 m2 / SEC2 m5. The group id cannot be reused while
/// one of them lives; with none left this does nothing.
#[cfg(unix)]
fn kill_rest(pid: Option<u32>) {
    if let Some(pid) = pid {
        // SAFETY: signalling the process group we started (`setsid`).
        unsafe { libc::kill(-(pid as i32), libc::SIGKILL) };
    }
}

#[cfg(not(unix))]
fn kill_rest(_pid: Option<u32>) {}

/// One sign-in, from the start of `claude auth login` to its end. `Ok` when it
/// linked (the login is linked and everything waiting for it resumed); else the
/// fixed text the board member reads.
#[allow(clippy::too_many_arguments)]
async fn drive(
    shared: &Arc<Shared>,
    program: &Path,
    login: &ClaudeLogin,
    link_id: &str,
    out: mpsc::Sender<ServerMessage>,
    mut code_rx: mpsc::Receiver<Code>,
    mut stop_rx: watch::Receiver<Option<&'static str>>,
    (recognize_within, timeout): (Duration, Duration),
) -> Result<(), &'static str> {
    use portable_pty::{CommandBuilder, PtySize};
    let dir = login.config_dir().to_path_buf();
    let mut command = CommandBuilder::new(program);
    command.args(["auth", "login", "--claudeai"]);
    command.env_clear();
    for (key, value) in login_env(|k| std::env::var_os(k), &dir) {
        command.env(key, value);
    }
    command.cwd(&dir);
    let spawned = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        let pair = portable_pty::native_pty_system().openpty(PtySize { rows: 24, cols: COLUMNS, pixel_width: 0, pixel_height: 0 })?;
        let child = pair.slave.spawn_command(command)?;
        let reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;
        Ok((pair.master, child, reader, writer))
    })
    .await;
    let Ok(Ok((master, mut child, reader, mut writer))) = spawned else {
        tracing::warn!(link = %link_id, state = "failed", "Claude sign-in could not start");
        return Err(NOT_STARTED);
    };
    let pid = child.process_id();
    let (exit_tx, mut exited) = tokio::sync::oneshot::channel::<Option<u32>>();
    std::thread::spawn(move || {
        let code = child.wait().ok().map(|s| s.exit_code());
        let _ = exit_tx.send(code);
    });
    let (chunk_tx, mut chunks) = mpsc::channel::<Vec<u8>>(64);
    std::thread::spawn(move || read_output(reader, chunk_tx));

    let started = tokio::time::Instant::now();
    let mut buf: Vec<u8> = Vec::new();
    // After the code: only what is needed to see a refusal, wiped (C9).
    let mut after: Vec<u8> = Vec::new();
    let mut prompted = false;
    let mut consumed = false;
    let mut reading = true;
    // Whether `claude` ended (by itself, or ended by Aurelius).
    let mut ended = false;
    let result = loop {
        let deadline = if prompted { started + timeout } else { started + recognize_within.min(timeout) };
        tokio::select! {
            // Always read, so `claude` never blocks on a full PTY; keep only what comes before the prompt.
            chunk = chunks.recv(), if reading => match chunk {
                Some(mut chunk) => {
                    // After the code only a refusal matters; it ends the sign-in (QA2 M1).
                    if consumed && after.len() < AFTER_CODE_MAX {
                        let room = AFTER_CODE_MAX - after.len();
                        after.extend_from_slice(&chunk[..chunk.len().min(room)]);
                        if refused(&strip_ansi(&after)) {
                            wipe(&mut chunk);
                            let code = end_group(pid, &mut exited).await;
                            ended = true;
                            tracing::info!(link = %link_id, state = "refused", exit = ?code, "Claude sign-in");
                            break Err(CODE_REFUSED);
                        }
                    }
                    // After the prompt nothing else is kept: only the end matters now (C9).
                    if !prompted && buf.len() < BUFFER_MAX {
                        let room = BUFFER_MAX - buf.len();
                        buf.extend_from_slice(&chunk[..chunk.len().min(room)]);
                        if let Some(url) = recognize(&strip_ansi(&buf)) {
                            prompted = true;
                            tracing::info!(link = %link_id, state = "prompt", "Claude sign-in");
                            let _ = out.send(ServerMessage::ClaudeLoginPrompt(ClaudeLoginPrompt { link_id: link_id.to_owned(), url: Some(url), asks_code: true, error: None })).await;
                            wipe(&mut buf);
                        }
                    }
                    wipe(&mut chunk);
                }
                None => reading = false,
            },
            code = code_rx.recv(), if prompted && !consumed => {
                let Some(code) = code else { continue };
                consumed = true;
                let mut line = code.0.clone();
                line.push(b'\r');
                let written = tokio::task::spawn_blocking(move || {
                    let ok = std::io::Write::write_all(&mut writer, &line).and_then(|_| std::io::Write::flush(&mut writer)).is_ok();
                    wipe(&mut line);
                    (ok, writer)
                })
                .await;
                drop(code);
                match written {
                    Ok((true, w)) => {
                        writer = w;
                        tracing::info!(link = %link_id, state = "consumed", "Claude sign-in");
                    }
                    _ => break Err(CODE_REFUSED),
                }
            }
            code = &mut exited => {
                let code = code.ok().flatten();
                ended = true;
                kill_rest(pid);
                tracing::info!(link = %link_id, state = if code == Some(0) { "exited" } else { "failed" }, exit = ?code, "Claude sign-in");
                break if code == Some(0) { Ok(()) } else if prompted { Err(CODE_REFUSED) } else { Err(NOT_RECOGNIZED) };
            }
            _ = stop_rx.changed() => {
                let why = (*stop_rx.borrow()).unwrap_or("cancelled");
                let code = end_group(pid, &mut exited).await;
                tracing::info!(link = %link_id, state = why, exit = ?code, "Claude sign-in");
                drop(master);
                wipe(&mut buf);
                wipe(&mut after);
                return Err(match why {
                    "cancelled" => "The sign-in was cancelled.",
                    "replaced" => "A new sign-in to Claude was started; this one ended.",
                    "refused" => CODE_REFUSED,
                    _ => "The sign-in ended.",
                });
            }
            _ = tokio::time::sleep_until(deadline) => {
                let code = end_group(pid, &mut exited).await;
                ended = true;
                let state = if prompted { "timeout" } else { "not recognized" };
                tracing::info!(link = %link_id, state, exit = ?code, "Claude sign-in");
                break Err(if prompted { TIMED_OUT } else { NOT_RECOGNIZED });
            }
        }
    };
    // Whatever ended the loop (a write that failed too), the group goes (R2 m2, SEC2 m5).
    if !ended {
        let code = end_group(pid, &mut exited).await;
        tracing::info!(link = %link_id, state = "ended", exit = ?code, "Claude sign-in");
    }
    drop(master);
    wipe(&mut buf);
    wipe(&mut after);
    result?;
    // Linked? Claude says so itself (C14), never the credentials file.
    let runtime = ClaudeCodeRuntime { executable: program.to_path_buf(), run_root: shared.root.join("runtimes"), model: None, login: Some(login.clone()), vault: Default::default() };
    match runtime.auth_status().await {
        Ok(status) if status.logged_in => {
            tracing::info!(link = %link_id, state = "ok", "Claude sign-in");
            let (s, user, email, plan) = (shared.clone(), login.user(), status.email, status.plan);
            match tokio::task::spawn_blocking(move || s.service.claude_login_linked(user, email, plan)).await {
                Ok(Ok(notices)) => {
                    crate::runtimes::forget_usage(shared, user);
                    shared.publish(notices);
                }
                other => tracing::error!(link = %link_id, ?other, "could not record the Claude login"),
            }
            Ok(())
        }
        _ => {
            tracing::info!(link = %link_id, state = "failed", "Claude sign-in: not signed in after all");
            Err(CODE_REFUSED)
        }
    }
}

/// `claude_login.unlink` (and a board member disabled): stop a running link,
/// sign out with `claude auth logout` in the login's own environment, remove
/// the folder and check it is gone (C15), then forget the login.
///
/// Runs already running on that login go on until they end (the token is in
/// their process); only new runs wait for a login (R2 m4, accepted).
pub async fn unlink(shared: &Arc<Shared>, user: UserId) -> Result<(), ProtocolError> {
    shared.claude_links.cancel_user(user);
    // The sign-in process is gone before the folder goes, so it cannot write it again (R2 m3).
    if !shared.claude_links.stopped(user, UNLINK_WAIT).await {
        tracing::warn!(%user, state = "still running", "Claude sign-in did not end in time; the login is removed anyway");
    }
    let login = ClaudeLogin::at(&shared.root, user);
    if login.config_dir().exists() {
        let logged_out = match claude_program(shared).await {
            Ok(program) => {
                let runtime = ClaudeCodeRuntime { executable: program, run_root: shared.root.join("runtimes"), model: None, login: Some(login.clone()), vault: Default::default() };
                match tokio::time::timeout(Duration::from_secs(30), runtime.logout()).await.unwrap_or(Err(aurelius_runtime::RuntimeError::Unavailable("timed out".into()))) {
                    Ok(()) => true,
                    Err(e) => {
                        tracing::warn!(%user, error = %e, "claude auth logout failed; the folder is removed anyway");
                        false
                    }
                }
            }
            Err(_) => false,
        };
        // Then Anthropic was not told: the board member can revoke the session at claude.ai (SEC2 m4).
        if !logged_out {
            tracing::warn!(%user, state = "removed without logout", "Claude login");
        }
        let root = shared.root.clone();
        tokio::task::spawn_blocking(move || aurelius_runtime::claude_code::login::remove(&root, user))
            .await
            .map_err(|e| ProtocolError::new(ErrorCode::Internal, e.to_string()))?
            .map_err(|e| ProtocolError::new(ErrorCode::Internal, format!("the Claude login folder could not be removed: {}", e.kind())))?;
    }
    let s = shared.clone();
    let notices = tokio::task::spawn_blocking(move || s.service.claude_login_removed(user))
        .await
        .map_err(|e| ProtocolError::new(ErrorCode::Internal, e.to_string()))?
        .map_err(|e| ProtocolError::new(e.code, e.message))?;
    crate::runtimes::forget_usage(shared, user);
    shared.publish(notices);
    tracing::info!(%user, state = "unlinked", "Claude login");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What `claude` 2.1.312 printed up to the prompt (Sentinel SEC0 §2.2, state and challenge masked).
    const MEASURED: &[u8] = b"Opening browser to sign in\xe2\x80\xa6\r\nIf the browser didn't open, visit: \x1b]8;;https://claude.com/cai/oauth/authorize?code=true&client_id=9d1c250a-e61b-44d9-88ed-5944d1962f5e&response_type=code&redirect_uri=https%3A%2F%2Fplatform.claude.com%2Foauth%2Fcode%2Fcallback&scope=org%3Acreate_api_key+user%3Aprofile+user%3Ainference+user%3Asessions%3Aclaude_code+user%3Amcp_servers+user%3Afile_upload+user%3Aplugins&code_challenge=CHALLENGE&code_challenge_method=S256&state=STATE\x07\x1b[94mhttps://claude.com/cai/oauth/authorize?code=true&client_id=9d1c250a-e61b-44d9-88ed-5944d1962f5e&response_type=code&redirect_uri=https%3A%2F%2Fplatform.claude.com%2Foauth%2Fcode%2Fcallback&scope=org%3Acreate_api_key+user%3Aprofile+user%3Ainference+user%3Asessions%3Aclaude_code+user%3Amcp_servers+user%3Afile_upload+user%3Aplugins&code_challenge=CHALLENGE&code_challenge_method=S256&state=STATE\x1b[39m\x1b]8;;\x07\r\nPaste code here if prompted > ";
    /// After a wrong code (§2.3).
    const REFUSED: &[u8] = b"Login failed: That code wasn't accepted. It may be incomplete, expired, already used, or from an earlier sign-in attempt. Sign in again to get a new code.\r\n\x1b[?25h";
    /// After a code without its `#state` (QA2 `real-cli-pty-cr.txt`, 2.1.312): it then waits for another.
    const NOT_WHOLE: &[u8] = b"Invalid code. Please make sure the full code was copied.\r\n";

    #[test]
    fn what_the_cli_says_to_a_wrong_or_incomplete_code_is_a_refusal() {
        assert!(refused(&strip_ansi(NOT_WHOLE)), "QA2 M1: the CLI waits on, so Aurelius must end it");
        assert!(refused(&strip_ansi(REFUSED)));
        assert!(!refused(&strip_ansi(MEASURED)), "the prompt itself is no refusal");
        assert!(!refused("Login successful.\r\n"));
        // What Claude's code page gives is `<code>#<state>`; anything else is not whole.
        assert!(whole_code("abcDEF123#STATE"));
        for bad in ["abc", "abc#", "#STATE", ""] {
            assert!(!whole_code(bad), "{bad:?}");
        }
    }

    #[test]
    fn a_code_that_is_not_whole_is_refused_before_it_is_written_and_ends_the_link() {
        let links = Links::default();
        let alex = UserId::new();
        let (id, mut code_rx, stop) = links.begin(alex, 1, None, Instant::now()).unwrap();
        links.input(alex, 1, &id, "abc").unwrap();
        assert!(code_rx.try_recv().is_err(), "nothing was written");
        assert_eq!(*stop.borrow(), Some("refused"));
        assert_eq!(links.input(alex, 1, &id, "good#state").err().map(|e| e.code), Some(ErrorCode::NotFound), "it is over");
    }

    #[test]
    fn links_started_at_once_pass_the_limit_only_five_times() {
        // SEC2 m1: the check and the count are one step. A race: tried many times.
        for _ in 0..50 {
            let links = Arc::new(Links::default());
            let alex = UserId::new();
            let now = Instant::now();
            let gate = Arc::new(std::sync::Barrier::new(10));
            let threads: Vec<_> = (0..10)
                .map(|conn| {
                    let (links, gate) = (links.clone(), gate.clone());
                    std::thread::spawn(move || {
                        gate.wait();
                        links.begin(alex, conn, None, now).is_ok()
                    })
                })
                .collect();
            let ok = threads.into_iter().map(|t| t.join().unwrap()).filter(|ok| *ok).count();
            assert_eq!(ok, 5);
        }
    }

    #[tokio::test]
    async fn unlink_waits_until_the_sign_in_is_gone() {
        // R2 m3: the link ends (its process waited for) before the folder goes.
        let links = Arc::new(Links::default());
        let alex = UserId::new();
        let (id, _, _) = links.begin(alex, 1, None, Instant::now()).unwrap();
        assert!(!links.stopped(alex, Duration::from_millis(100)).await, "still running");
        let (l, i) = (links.clone(), id.clone());
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(150)).await;
            l.end(alex, &i);
        });
        assert!(links.stopped(alex, Duration::from_secs(3)).await);
    }

    #[test]
    fn the_measured_output_gives_the_link_once_the_code_is_asked() {
        let url = recognize(&strip_ansi(MEASURED)).expect("recognized");
        assert!(url.starts_with(SIGN_IN_PREFIX) && url.ends_with("state=STATE"), "{url}");
        assert!(!url.contains('\u{1b}') && !url.contains('\u{7}'));
        // Before the question there is nothing to pass on yet, also with the link.
        let half = &MEASURED[..MEASURED.len() - "Paste code here if prompted > ".len()];
        assert_eq!(recognize(&strip_ansi(half)), None);
        // A wrong code's output holds no link.
        assert_eq!(recognize(&strip_ansi(REFUSED)), None);
        assert_eq!(strip_ansi(REFUSED).trim(), "Login failed: That code wasn't accepted. It may be incomplete, expired, already used, or from an earlier sign-in attempt. Sign in again to get a new code.");
    }

    #[test]
    fn only_anthropics_authorize_page_is_passed_on() {
        let ok = format!("{SIGN_IN_PREFIX}code=true&state=x");
        assert!(valid_url(&ok));
        for bad in [
            "https://claude.com.evil.example/cai/oauth/authorize?x".to_string(),
            "http://claude.com/cai/oauth/authorize?x".into(),
            "https://claude.ai/oauth/authorize?x".into(),
            format!("{SIGN_IN_PREFIX}a\u{7}b"),
            format!("{SIGN_IN_PREFIX}a b"),
            format!("{SIGN_IN_PREFIX}{}", "a".repeat(URL_MAX)),
        ] {
            assert!(!valid_url(&bad), "{bad:?}");
        }
        let fake = "If the browser didn't open, visit: https://evil.example/login\r\nPaste code here if prompted > ";
        assert_eq!(recognize(fake), None, "another site never reaches a client");
    }

    #[test]
    fn a_code_is_checked_before_it_goes_in() {
        assert!(valid_code("abcDEF123_-.~#xyz"));
        for bad in ["", "a b", "abc\r", "abc\ndef", "code;rm", "ä", &"a".repeat(CODE_MAX + 1)] {
            assert!(!valid_code(bad), "{bad:?}");
        }
        assert!(valid_code(&"a".repeat(CODE_MAX)));
    }

    #[test]
    fn the_login_process_gets_only_its_allowlist() {
        let parent = |k: &str| -> Option<OsString> {
            match k {
                "PATH" => Some("/usr/bin".into()),
                "HOME" => Some("/home/daemon".into()),
                "DISPLAY" => Some(":0".into()),
                "WAYLAND_DISPLAY" => Some("wayland-0".into()),
                "DBUS_SESSION_BUS_ADDRESS" => Some("unix:path=/run/bus".into()),
                "ANTHROPIC_API_KEY" | "ANTHROPIC_AUTH_TOKEN" | "ANTHROPIC_PROFILE" | "CLAUDE_CODE_OAUTH_TOKEN" => Some("PARENT-SECRET".into()),
                "CLAUDE_CONFIG_DIR" => Some("/home/daemon/.claude".into()),
                "BROWSER" => Some("firefox".into()),
                _ => None,
            }
        };
        let env = login_env(parent, Path::new("/srv/aurelius/logins/u"));
        let keys: Vec<&str> = env.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, ["PATH", "HOME", "TERM", "CLAUDE_CONFIG_DIR", "BROWSER", "SHELL"]);
        let get = |k: &str| env.iter().find(|(key, _)| key == k).map(|(_, v)| v.to_string_lossy().into_owned());
        assert_eq!(get("CLAUDE_CONFIG_DIR").as_deref(), Some("/srv/aurelius/logins/u"));
        assert_eq!((get("BROWSER").as_deref(), get("SHELL").as_deref(), get("TERM").as_deref()), (Some("/bin/false"), Some("/bin/false"), Some("xterm-256color")));
        assert!(!format!("{env:?}").contains("PARENT-SECRET") && !format!("{env:?}").contains(".claude"));
    }

    #[test]
    fn a_link_id_is_128_random_bits() {
        let (a, b) = (new_link_id(), new_link_id());
        assert_eq!(a.len(), 32);
        assert_ne!(a, b);
        assert!(a.bytes().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn one_link_at_a_time_five_in_ten_minutes_and_only_its_own_connection_answers() {
        let links = Links::default();
        let (alex, robin) = (UserId::new(), UserId::new());
        let t0 = Instant::now();
        let (first, _, first_stop) = links.begin(alex, 1, None, t0).unwrap();
        // C2: a second one ends the first, which hears why; its code is no longer taken.
        let (id, mut code_rx, _stop) = links.begin(alex, 1, None, t0).unwrap();
        assert_eq!(*first_stop.borrow(), Some("replaced"));
        assert_eq!(links.input(alex, 1, &first, "good-code#state").err().map(|e| e.code), Some(ErrorCode::NotFound));
        assert_eq!(links.running(alex), Some(id.clone()));
        links.end(alex, &first);
        assert_eq!(links.running(alex), Some(id.clone()), "the first one ending leaves the second");
        // C5: another connection, another user or another id: NotFound alike.
        for (user, conn, link) in [(alex, 2, id.as_str()), (robin, 1, id.as_str()), (alex, 1, "0000")] {
            assert_eq!(links.input(user, conn, link, "good-code#state").err().map(|e| e.code), Some(ErrorCode::NotFound));
        }
        assert_eq!(links.input(alex, 1, &id, "bad code\r").err().map(|e| e.code), Some(ErrorCode::Validation));
        links.input(alex, 1, &id, "good-code#state").unwrap();
        assert_eq!(code_rx.try_recv().unwrap().0, b"good-code#state");
        assert_eq!(links.input(alex, 1, &id, "good-code#state").err().map(|e| e.code), Some(ErrorCode::NotFound), "C7: once only");
        links.end(alex, &id);
        // C16: five in ten minutes, the sixth waits.
        for _ in 0..3 {
            let (id, _, _) = links.begin(alex, 1, None, t0).unwrap();
            links.end(alex, &id);
        }
        let refused = links.begin(alex, 1, None, t0).err().unwrap();
        assert_eq!(refused.code, ErrorCode::TooManyAttempts);
        assert!(refused.retry_after_ms.is_some());
        assert!(links.begin(alex, 1, None, t0 + WINDOW).is_ok(), "ten minutes later it opens again");
        assert!(links.begin(robin, 3, None, t0).is_ok(), "per board member");
    }

    #[test]
    fn ending_a_connection_session_or_the_daemon_stops_its_link() {
        let links = Links::default();
        let (alex, robin) = (UserId::new(), UserId::new());
        let (_, _, alex_stop) = links.begin(alex, 1, Some("s1".into()), Instant::now()).unwrap();
        let (_, _, robin_stop) = links.begin(robin, 2, Some("s2".into()), Instant::now()).unwrap();
        links.cancel_conn(1);
        assert_eq!(*alex_stop.borrow(), Some("disconnected"));
        assert_eq!(*robin_stop.borrow(), None, "only that connection's");
        links.cancel_session("s2");
        assert_eq!(*robin_stop.borrow(), Some("signed out"));
        links.cancel_all();
        assert_eq!(*alex_stop.borrow(), Some("shutdown"));
    }
}
