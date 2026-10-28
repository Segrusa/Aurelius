//! Built-in terminal (protocol 1.5): a shell in a pseudo terminal, in the
//! folder an employee's conversation works in. Its output goes only to the
//! sessions attached to it and is never stored; the last `SCROLLBACK` bytes
//! stay in memory so a reopened panel shows what came before. A terminal
//! nobody is attached to ends after `IDLE`. A conversation may run up to
//! `MAX_PER_CONVERSATION` terminals side by side (feature `terminal.multi`); a
//! session detaches from the ones it doesn't show and gets no output from them.

use crate::Service;
use aurelius_domain::ConversationId;
use aurelius_protocol::{ErrorCode, ProtocolError, ServerMessage, TerminalExit, TerminalOutput, TerminalSummary, TerminalView};
use base64::Engine;
use portable_pty::{ChildKiller, CommandBuilder, MasterPty, PtySize};
use std::collections::{HashMap, VecDeque};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, mpsc};

/// Output kept per terminal for the next attach.
pub const SCROLLBACK: usize = 256 * 1024;
/// Shells running at once on this server.
pub const MAX_TERMINALS: usize = 8;
/// Shells running at once for one conversation.
pub const MAX_PER_CONVERSATION: usize = 4;
/// How long a terminal lives with nobody attached.
pub const IDLE: Duration = Duration::from_secs(10 * 60);
/// Largest piece of output in one message.
const CHUNK: usize = 16 * 1024;

#[derive(Debug, Clone)]
enum Chunk {
    Output(Arc<[u8]>),
    Exit(Option<i32>),
}

struct Terminal {
    id: String,
    conversation: ConversationId,
    cwd: String,
    shell: String,
    /// `Terminal <number>` in lists.
    number: usize,
    created_at_ms: i64,
    /// Taken (closing the pseudo console) when the shell ends.
    master: Mutex<Option<Box<dyn MasterPty + Send>>>,
    writer: Mutex<Box<dyn Write + Send>>,
    killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
    state: Mutex<State>,
}

struct State {
    scrollback: VecDeque<u8>,
    /// Sent under the `state` lock, so an attach never misses or repeats output.
    tx: broadcast::Sender<Chunk>,
    /// `Some(code)` once the shell ended.
    exited: Option<Option<i32>>,
    attached: usize,
    idle_since: Instant,
}

impl Terminal {
    fn view(&self, reattached: bool) -> TerminalView {
        TerminalView {
            terminal_id: self.id.clone(),
            conversation_id: self.conversation,
            cwd: self.cwd.clone(),
            shell: self.shell.clone(),
            reattached,
        }
    }

    fn alive(&self) -> bool {
        self.state.lock().is_ok_and(|s| s.exited.is_none())
    }

    fn summary(&self) -> TerminalSummary {
        let (alive, attached) = self.state.lock().map(|s| (s.exited.is_none(), s.attached)).unwrap_or((false, 0));
        TerminalSummary {
            terminal_id: self.id.clone(),
            conversation_id: self.conversation,
            title: format!("Terminal {}", self.number),
            cwd: self.cwd.clone(),
            shell: self.shell.clone(),
            created_at_ms: self.created_at_ms,
            alive,
            attached: attached as u32,
        }
    }

    fn kill(&self) {
        if let Ok(mut killer) = self.killer.lock() {
            let _ = killer.kill();
        }
    }
}

/// Every terminal of this server.
#[derive(Default)]
pub struct Terminals {
    map: Mutex<HashMap<String, Arc<Terminal>>>,
}

/// One session's view of a terminal: forwards its output until dropped.
pub struct Attachment {
    terminal: Arc<Terminal>,
    task: tokio::task::JoinHandle<()>,
    /// Output waits for this, so the `terminal/open` response goes first.
    gate: Option<tokio::sync::oneshot::Sender<()>>,
}

impl Attachment {
    /// Start forwarding output (after the response was sent).
    pub fn release(&mut self) {
        if let Some(gate) = self.gate.take() {
            let _ = gate.send(());
        }
    }

    pub fn is_for(&self, conversation: ConversationId) -> bool {
        self.terminal.conversation == conversation
    }

    pub fn terminal_id(&self) -> &str {
        &self.terminal.id
    }
}

impl Drop for Attachment {
    fn drop(&mut self) {
        self.task.abort();
        if let Ok(mut state) = self.terminal.state.lock() {
            state.attached = state.attached.saturating_sub(1);
            if state.attached == 0 {
                state.idle_since = Instant::now();
            }
        }
    }
}

fn error(code: ErrorCode, message: impl Into<String>) -> ProtocolError {
    ProtocolError::new(code, message)
}

impl Terminals {
    fn get(&self, id: &str) -> Result<Arc<Terminal>, ProtocolError> {
        self.map
            .lock()
            .ok()
            .and_then(|m| m.get(id).cloned())
            .ok_or_else(|| error(ErrorCode::NotFound, "This terminal has ended."))
    }

    /// Keystrokes or pasted text for the shell.
    pub async fn input(&self, id: &str, data_base64: &str) -> Result<(), ProtocolError> {
        let terminal = self.get(id)?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(data_base64)
            .map_err(|e| error(ErrorCode::InvalidRequest, e.to_string()))?;
        // A shell that doesn't read can block the pipe: never on a runtime thread.
        tokio::task::spawn_blocking(move || {
            let mut writer = terminal.writer.lock().map_err(|_| error(ErrorCode::Internal, "terminal writer poisoned"))?;
            writer.write_all(&bytes).and_then(|()| writer.flush()).map_err(|e| error(ErrorCode::Internal, e.to_string()))
        })
        .await
        .map_err(|e| error(ErrorCode::Internal, e.to_string()))?
    }

    pub fn resize(&self, id: &str, cols: u16, rows: u16) -> Result<(), ProtocolError> {
        let terminal = self.get(id)?;
        let master = terminal.master.lock().map_err(|_| error(ErrorCode::Internal, "terminal poisoned"))?;
        if let Some(master) = master.as_ref() {
            master.resize(size(cols, rows)).map_err(|e| error(ErrorCode::Internal, e.to_string()))?;
        }
        Ok(())
    }

    /// Ends the shell; attached sessions get `TerminalExit`.
    pub fn close(&self, id: &str) -> Result<(), ProtocolError> {
        let terminal = self.map.lock().ok().and_then(|mut m| m.remove(id)).ok_or_else(|| error(ErrorCode::NotFound, "This terminal has ended."))?;
        terminal.kill();
        Ok(())
    }

    /// Ends every shell (the server stops).
    pub fn close_all(&self) {
        let all: Vec<_> = self.map.lock().map(|mut m| m.drain().map(|(_, t)| t).collect()).unwrap_or_default();
        for terminal in all {
            terminal.kill();
        }
    }

    /// Forgets ended terminals and ends the ones nobody looked at for `idle`.
    pub fn sweep(&self, idle: Duration) {
        let Ok(mut map) = self.map.lock() else { return };
        map.retain(|_, terminal| {
            let Ok(state) = terminal.state.lock() else { return false };
            if state.exited.is_some() && state.attached == 0 {
                return false;
            }
            if state.attached == 0 && state.idle_since.elapsed() >= idle {
                tracing::info!(terminal = %terminal.id, "ending an unattached terminal");
                terminal.kill();
                return false;
            }
            true
        });
    }

    /// Whether a terminal with this id exists (running or not yet forgotten).
    pub fn exists(&self, id: &str) -> bool {
        self.map.lock().is_ok_and(|m| m.contains_key(id))
    }

    /// The conversation's terminals, oldest first.
    fn of(&self, conversation: ConversationId) -> Vec<Arc<Terminal>> {
        let Ok(map) = self.map.lock() else { return Vec::new() };
        let mut all: Vec<_> = map.values().filter(|t| t.conversation == conversation).cloned().collect();
        // Ids are UUIDv7: they sort by start time even within one millisecond.
        all.sort_by(|a, b| a.id.cmp(&b.id));
        all
    }

    /// The oldest running terminal of the conversation (the one a plain open attaches to).
    fn running_for(&self, conversation: ConversationId) -> Option<Arc<Terminal>> {
        self.of(conversation).into_iter().find(|t| t.alive())
    }

    /// The conversation's terminals for `terminal.list`, oldest first.
    pub fn list(&self, conversation: ConversationId) -> Vec<TerminalSummary> {
        self.of(conversation).iter().map(|t| t.summary()).collect()
    }

    /// Room for one more shell, in the conversation and on the server; the
    /// number of its title.
    fn check_room(&self, conversation: ConversationId) -> Result<usize, ProtocolError> {
        let running: Vec<usize> = self.of(conversation).iter().filter(|t| t.alive()).map(|t| t.number).collect();
        let total = self.map.lock().map(|m| m.len()).unwrap_or(0);
        room(running.len(), total)?;
        Ok(lowest_free(&running))
    }

    fn attach(terminal: Arc<Terminal>, out: mpsc::Sender<ServerMessage>) -> Attachment {
        let (backlog, exited, mut rx) = {
            let mut state = terminal.state.lock().expect("terminal state");
            state.attached += 1;
            let backlog: Vec<u8> = state.scrollback.iter().copied().collect();
            (backlog, state.exited, state.tx.subscribe())
        };
        let id = terminal.id.clone();
        let (gate, released) = tokio::sync::oneshot::channel::<()>();
        let task = tokio::spawn(async move {
            if released.await.is_err() {
                return;
            }
            let output = |bytes: &[u8]| {
                ServerMessage::TerminalOutput(TerminalOutput {
                    terminal_id: id.clone(),
                    data_base64: base64::engine::general_purpose::STANDARD.encode(bytes),
                })
            };
            for piece in backlog.chunks(CHUNK) {
                if out.send(output(piece)).await.is_err() {
                    return;
                }
            }
            let exit = |code| ServerMessage::TerminalExit(TerminalExit { terminal_id: id.clone(), code });
            if let Some(code) = exited {
                let _ = out.send(exit(code)).await;
                return;
            }
            loop {
                let message = match rx.recv().await {
                    Ok(Chunk::Output(bytes)) => output(&bytes),
                    Ok(Chunk::Exit(code)) => {
                        let _ = out.send(exit(code)).await;
                        return;
                    }
                    // This session read too slowly; say so instead of blocking the shell.
                    Err(broadcast::error::RecvError::Lagged(_)) => output(b"\r\n\x1b[2m[output skipped]\x1b[0m\r\n"),
                    Err(broadcast::error::RecvError::Closed) => return,
                };
                if out.send(message).await.is_err() {
                    return;
                }
            }
        });
        Attachment { terminal, task, gate: Some(gate) }
    }
}

/// The limits: `in_conversation` running in the conversation, `total` on the server.
fn room(in_conversation: usize, total: usize) -> Result<(), ProtocolError> {
    if in_conversation >= MAX_PER_CONVERSATION {
        return Err(error(ErrorCode::Validation, format!("At most {MAX_PER_CONVERSATION} terminals can run in one conversation; close one first.")));
    }
    if total >= MAX_TERMINALS {
        return Err(error(ErrorCode::Validation, format!("At most {MAX_TERMINALS} terminals can run at once on this server; close one first.")));
    }
    Ok(())
}

/// The lowest number from 1 that no running terminal uses.
fn lowest_free(used: &[usize]) -> usize {
    (1..).find(|n| !used.contains(n)).expect("a free number")
}

fn size(cols: u16, rows: u16) -> PtySize {
    PtySize { rows: rows.clamp(2, 500), cols: cols.clamp(10, 1000), pixel_width: 0, pixel_height: 0 }
}

/// The shell to start: PowerShell 7 when installed, else Windows PowerShell;
/// the login shell elsewhere.
fn shell() -> (String, Vec<String>) {
    if cfg!(windows) {
        let pwsh = std::env::var_os("PATH")
            .map(|path| std::env::split_paths(&path).any(|dir| dir.join("pwsh.exe").is_file()))
            .unwrap_or(false);
        let program = if pwsh { "pwsh.exe" } else { "powershell.exe" };
        (program.into(), vec!["-NoLogo".into()])
    } else {
        let program = std::env::var("SHELL").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| "/bin/bash".into());
        (program, vec!["-l".into()])
    }
}

/// `terminal.list`: the conversation's terminals, oldest first.
pub async fn list(shared: &Arc<impl Service>, conversation: ConversationId) -> Result<Vec<TerminalSummary>, ProtocolError> {
    Ok(shared.terminals().list(conversation))
}

/// How `terminal.open` picks its terminal.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OpenOptions {
    /// End the running terminal and start a new one.
    pub fresh: bool,
    /// Attach to exactly this terminal of the conversation.
    pub terminal_id: Option<String>,
    /// Start another terminal beside the running ones.
    pub additional: bool,
}

/// Attach this session to a terminal of the conversation. Without options:
/// the oldest running one, starting one when none runs (or when `fresh`,
/// after ending it). With `terminal_id`: exactly that one. With `additional`:
/// a new one beside the others.
pub async fn open(
    shared: &Arc<impl Service>,
    conversation: ConversationId,
    cols: u16,
    rows: u16,
    options: OpenOptions,
    out: mpsc::Sender<ServerMessage>,
) -> Result<(TerminalView, Attachment), ProtocolError> {
    let terminals = shared.terminals();
    if let Some(id) = &options.terminal_id {
        let terminal = terminals.get(id).ok().filter(|t| t.conversation == conversation);
        let terminal = terminal.ok_or_else(|| error(ErrorCode::NotFound, "This terminal has ended."))?;
        let _ = terminals.resize(&terminal.id, cols, rows);
        let view = terminal.view(true);
        return Ok((view, Terminals::attach(terminal, out)));
    }
    if !options.additional {
        if let Some(running) = terminals.running_for(conversation) {
            if options.fresh {
                let _ = terminals.close(&running.id);
            } else {
                let _ = terminals.resize(&running.id, cols, rows);
                let view = running.view(true);
                return Ok((view, Terminals::attach(running, out)));
            }
        }
    }
    terminals.sweep(IDLE);
    let number = terminals.check_room(conversation)?;

    let store = shared.store().clone();
    let folders = tokio::task::spawn_blocking(move || store.read(|c| aurelius_storage::conversations::work_folders(c, conversation)))
        .await
        .map_err(|e| error(ErrorCode::Internal, e.to_string()))?
        .map_err(|e| error(ErrorCode::StorageUnavailable, e.to_string()))?;
    let cwd = folders
        .into_iter()
        .find(|d| std::path::Path::new(d).is_dir())
        .ok_or_else(|| error(ErrorCode::NotFound, "This conversation has no folder on the server to open a terminal in."))?;

    let terminal = tokio::task::spawn_blocking(move || start(conversation, cwd, number, cols, rows))
        .await
        .map_err(|e| error(ErrorCode::Internal, e.to_string()))??;
    // Another session may have started one meanwhile: check again under the lock.
    {
        let mut map = terminals.map.lock().map_err(|_| error(ErrorCode::Internal, "terminals poisoned"))?;
        let in_conversation = map.values().filter(|t| t.conversation == conversation && t.alive()).count();
        if let Err(err) = room(in_conversation, map.len()) {
            drop(map);
            terminal.kill();
            return Err(err);
        }
        map.insert(terminal.id.clone(), terminal.clone());
    }
    tracing::info!(terminal = %terminal.id, %conversation, shell = %terminal.shell, "terminal started");
    let view = terminal.view(false);
    Ok((view, Terminals::attach(terminal, out)))
}

fn start(conversation: ConversationId, cwd: String, number: usize, cols: u16, rows: u16) -> Result<Arc<Terminal>, ProtocolError> {
    let internal = |e: anyhow::Error| error(ErrorCode::Internal, format!("could not start a terminal: {e}"));
    let pair = portable_pty::native_pty_system().openpty(size(cols, rows)).map_err(internal)?;
    let (program, args) = shell();
    let mut command = CommandBuilder::new(&program);
    command.args(&args);
    command.cwd(&cwd);
    command.env("TERM", "xterm-256color");
    command.env("COLORTERM", "truecolor");
    command.env("AURELIUS_TERMINAL", "1");
    let mut child = pair.slave.spawn_command(command).map_err(internal)?;
    drop(pair.slave);
    let mut reader = pair.master.try_clone_reader().map_err(internal)?;
    let writer = pair.master.take_writer().map_err(internal)?;
    let killer = child.clone_killer();
    let (tx, _) = broadcast::channel(256);
    let terminal = Arc::new(Terminal {
        id: uuid::Uuid::now_v7().to_string(),
        conversation,
        cwd,
        shell: program,
        number,
        created_at_ms: aurelius_storage::now_ms(),
        master: Mutex::new(Some(pair.master)),
        writer: Mutex::new(writer),
        killer: Mutex::new(killer),
        state: Mutex::new(State { scrollback: VecDeque::new(), tx, exited: None, attached: 0, idle_since: Instant::now() }),
    });

    let reading = terminal.clone();
    std::thread::Builder::new()
        .name("terminal-read".into())
        .spawn(move || {
            let mut buf = vec![0u8; CHUNK];
            loop {
                let n = match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => n,
                };
                let Ok(mut state) = reading.state.lock() else { break };
                state.scrollback.extend(&buf[..n]);
                let over = state.scrollback.len().saturating_sub(SCROLLBACK);
                state.scrollback.drain(..over);
                let _ = state.tx.send(Chunk::Output(Arc::from(&buf[..n])));
            }
        })
        .map_err(|e| error(ErrorCode::Internal, e.to_string()))?;

    // On Windows the output pipe stays open until the pseudo console closes,
    // so the end of the shell is noticed here, not by the reader.
    let waiting = terminal.clone();
    std::thread::Builder::new()
        .name("terminal-wait".into())
        .spawn(move || {
            let code = child.wait().ok().map(|status| status.exit_code() as i32);
            // Let the reader pass on the last output before the console closes.
            std::thread::sleep(Duration::from_millis(100));
            drop(waiting.master.lock().ok().and_then(|mut m| m.take()));
            if let Ok(mut state) = waiting.state.lock() {
                state.exited = Some(code);
                let _ = state.tx.send(Chunk::Exit(code));
            }
            tracing::info!(terminal = %waiting.id, ?code, "terminal ended");
        })
        .map_err(|e| error(ErrorCode::Internal, e.to_string()))?;
    Ok(terminal)
}

/// Ends unattached terminals after `IDLE`, until the server stops.
pub fn start_sweeper(shared: Arc<impl Service>) {
    tokio::spawn(async move {
        let mut shutdown = shared.shutdown_signal();
        loop {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(30)) => {}
                // Holding `shared` past the stop would keep the database open.
                _ = shutdown.changed() => return,
            }
            shared.terminals().sweep(IDLE);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_take_the_lowest_free_number() {
        assert_eq!(lowest_free(&[]), 1);
        assert_eq!(lowest_free(&[1, 2]), 3);
        assert_eq!(lowest_free(&[2, 3]), 1);
        assert_eq!(lowest_free(&[1, 3]), 2);
    }

    #[test]
    fn four_per_conversation_and_eight_per_server() {
        assert!(room(MAX_PER_CONVERSATION - 1, MAX_TERMINALS - 1).is_ok());
        let full = room(MAX_PER_CONVERSATION, 4).unwrap_err();
        assert_eq!(full.code, ErrorCode::Validation);
        assert!(full.message.contains("4 terminals can run in one conversation"), "{}", full.message);
        let server = room(1, MAX_TERMINALS).unwrap_err();
        assert!(server.message.contains("8 terminals can run at once on this server"), "{}", server.message);
    }

    /// Output until `needle` shows up; answers ConPTY's cursor position query.
    async fn read_until(terminals: &Terminals, id: &str, rx: &mut mpsc::Receiver<ServerMessage>, needle: &str) -> String {
        let mut seen = Vec::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
        loop {
            let text = String::from_utf8_lossy(&seen).into_owned();
            if text.contains(needle) {
                return text;
            }
            match tokio::time::timeout_at(deadline, rx.recv()).await {
                Ok(Some(ServerMessage::TerminalOutput(o))) => {
                    let bytes = base64::engine::general_purpose::STANDARD.decode(o.data_base64).unwrap();
                    if bytes.windows(4).any(|w| w == b"\x1b[6n") {
                        terminals.input(id, &base64::engine::general_purpose::STANDARD.encode(b"\x1b[1;1R")).await.unwrap();
                    }
                    seen.extend(bytes);
                }
                Ok(other) => panic!("{other:?} before {needle:?}:\n{text}"),
                Err(_) => panic!("no {needle:?} in time:\n{text}"),
            }
        }
    }

    async fn type_line(terminals: &Terminals, id: &str, line: &str) {
        terminals.input(id, &base64::engine::general_purpose::STANDARD.encode(format!("{line}\r"))).await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_detached_session_gets_nothing_while_the_shell_runs_on() {
        let dir = std::env::temp_dir();
        let conversation = ConversationId::new();
        let terminals = Terminals::default();
        let terminal = start(conversation, dir.display().to_string(), 1, 80, 24).unwrap();
        terminals.map.lock().unwrap().insert(terminal.id.clone(), terminal.clone());
        let id = terminal.id.clone();

        let (tx, mut rx) = mpsc::channel(1024);
        let mut attachment = Terminals::attach(terminal.clone(), tx);
        attachment.release();
        type_line(&terminals, &id, "echo before-detach").await;
        read_until(&terminals, &id, &mut rx, "before-detach").await;
        assert_eq!(terminals.list(conversation)[0].attached, 1);

        // Detaching ends the forwarding: the channel closes, the shell does not.
        drop(attachment);
        let summary = &terminals.list(conversation)[0];
        assert_eq!((summary.alive, summary.attached, summary.title.as_str()), (true, 0, "Terminal 1"));
        let closed = tokio::time::timeout(Duration::from_secs(5), async {
            while let Some(message) = rx.recv().await {
                assert!(matches!(message, ServerMessage::TerminalOutput(_)), "{message:?}");
            }
        })
        .await;
        assert!(closed.is_ok(), "the detached session's channel closes");
        type_line(&terminals, &id, "echo while-detached").await;
        tokio::time::sleep(Duration::from_secs(2)).await;
        assert!(terminal.alive());

        // The shell kept running and its scrollback kept growing; a new attach replays it.
        let (tx, mut rx) = mpsc::channel(1024);
        let mut again = Terminals::attach(terminal.clone(), tx);
        again.release();
        let text = read_until(&terminals, &id, &mut rx, "while-detached").await;
        assert!(text.contains("before-detach"), "the scrollback is replayed:\n{text}");
        drop(again);
        terminals.close_all();
    }
}
