//! `aurelius://` links that the OS hands to the app (AUR-FA A5b). A click starts the app with
//! the link as its first argument (Linux `%u`, Windows `%1`); macOS sends an open-URL event
//! instead. A second process passes its link to the running app over a local socket and ends,
//! so one click opens one place in one window. The link holds no token and only navigates; the
//! strict parse and the "This isn't a valid Aurelius link." message are `screens::links`'.

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

/// The longest link the socket takes: a link is 100 characters, so this is generous.
const MAX_LINK: usize = 2048;

/// The first argument that is an `aurelius://` address (the program name is not one).
pub fn link_arg(args: impl IntoIterator<Item = String>) -> Option<String> {
    args.into_iter().skip(1).find(|a| crate::screens::links::is_aurelius(a))
}

/// Links on their way to the window: from the first argument, the socket and the OS event.
pub fn channel() -> (UnboundedSender<String>, UnboundedReceiver<String>) {
    unbounded_channel()
}

/// Where the running app listens: a socket beside its data.
#[cfg(unix)]
pub fn socket_path(data_root: &std::path::Path) -> std::path::PathBuf {
    data_root.join("desktop-link.sock")
}

/// Hand `url` to the app that already runs; false when none answers (then this process is the
/// app). An empty `url` only asks whether one runs.
#[cfg(unix)]
pub fn pass_to_running(data_root: &std::path::Path, url: &str) -> bool {
    use std::io::Write;
    let Ok(mut stream) = std::os::unix::net::UnixStream::connect(socket_path(data_root)) else { return false };
    let _ = stream.set_write_timeout(Some(std::time::Duration::from_secs(2)));
    stream.write_all(format!("{url}\n").as_bytes()).is_ok()
}

/// No socket on this platform: every click is its own process, with its own window.
#[cfg(not(unix))]
pub fn pass_to_running(_data_root: &std::path::Path, _url: &str) -> bool {
    false
}

/// Connections served at once; a silent client holds a thread for the read timeout, so the cap
/// keeps a flood from the same user cheap.
#[cfg(unix)]
const MAX_CLIENTS: usize = 16;

/// Bind a socket that is never reachable by others and put it at `path` only when nothing is
/// there. The socket is bound inside a private (0700) folder, made 0600, and then hard-linked
/// to `path`: the link fails when the name is taken, so two starts at once cannot both win and
/// the name never holds a socket with the umask's rights.
#[cfg(unix)]
fn bind_private(path: &std::path::Path) -> std::io::Result<std::os::unix::net::UnixListener> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    use std::sync::atomic::{AtomicU32, Ordering};
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let dir = path.with_file_name(format!(".link-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::DirBuilder::new().mode(0o700).create(&dir)?;
    let tmp = dir.join("s");
    let result = (|| {
        let listener = std::os::unix::net::UnixListener::bind(&tmp)?;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
        std::fs::hard_link(&tmp, path)?;
        Ok(listener)
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

/// Listen for links from later processes and send them to `tx`; false when this process could
/// not become the listener. Call only when no app answered `pass_to_running`. A name that
/// is taken is replaced only when nothing answers on it (a socket left by a crash): a live
/// listener of a concurrent start stays.
#[cfg(unix)]
pub fn listen(data_root: &std::path::Path, tx: UnboundedSender<String>) -> bool {
    use std::io::{BufRead, Read};
    use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
    let path = socket_path(data_root);
    let listener = match bind_private(&path) {
        Ok(listener) => listener,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            if std::os::unix::net::UnixStream::connect(&path).is_ok() {
                return false;
            }
            let _ = std::fs::remove_file(&path);
            match bind_private(&path) {
                Ok(listener) => listener,
                Err(_) => return false,
            }
        }
        Err(_) => return false,
    };
    let active = Arc::new(AtomicUsize::new(0));
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            if active.fetch_add(1, Ordering::SeqCst) >= MAX_CLIENTS {
                active.fetch_sub(1, Ordering::SeqCst);
                continue;
            }
            let (tx, active) = (tx.clone(), active.clone());
            // One thread per connection: a client that stays silent holds only its own.
            std::thread::spawn(move || {
                let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(2)));
                let mut line = String::new();
                // Bounded, so a client that never ends its line cannot grow it.
                if std::io::BufReader::new(stream.take(MAX_LINK as u64)).read_line(&mut line).is_ok() {
                    let url = line.trim();
                    if !url.is_empty() {
                        let _ = tx.send(url.to_string());
                    }
                }
                active.fetch_sub(1, Ordering::SeqCst);
            });
        }
    });
    true
}

#[cfg(not(unix))]
pub fn listen(_data_root: &std::path::Path, _tx: UnboundedSender<String>) -> bool {
    false
}

#[cfg(all(test, unix))]
mod tests {
    use super::{channel, link_arg, listen, pass_to_running};

    const LINK: &str = "aurelius://11111111-1111-4111-8111-111111111111/ticket/aaaaaaaa-2222-4222-8222-222222222222";

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn only_an_aurelius_address_after_the_program_name_is_a_link() {
        assert_eq!(link_arg(args(&["aurelius", LINK])), Some(LINK.to_string()));
        assert_eq!(link_arg(args(&["aurelius", "--flag", LINK])), Some(LINK.to_string()));
        assert_eq!(link_arg(args(&["aurelius"])), None);
        assert_eq!(link_arg(args(&["aurelius", "https://example.com"])), None);
        // The program name is never read as a link.
        assert_eq!(link_arg(args(&[LINK])), None);
        // A malformed one still counts as ours, so it gets the fixed message, not silence.
        assert_eq!(link_arg(args(&["aurelius", "aurelius://nonsense"])), Some("aurelius://nonsense".to_string()));
    }

    #[test]
    fn a_second_process_reaches_the_running_app_and_nobody_answers_when_none_runs() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!pass_to_running(dir.path(), LINK), "no app runs yet");
        let (tx, mut rx) = channel();
        assert!(listen(dir.path(), tx));
        assert!(pass_to_running(dir.path(), ""), "a probe finds it");
        assert!(pass_to_running(dir.path(), LINK));
        assert!(pass_to_running(dir.path(), "aurelius://nonsense"));
        let rt = tokio::runtime::Builder::new_current_thread().enable_time().build().unwrap();
        let got = rt.block_on(async {
            let wait = std::time::Duration::from_secs(5);
            let a = tokio::time::timeout(wait, rx.recv()).await.unwrap().unwrap();
            let b = tokio::time::timeout(wait, rx.recv()).await.unwrap().unwrap();
            (a, b)
        });
        // The probe sent nothing; each connection has its own thread, so the order is not fixed.
        let mut got = [got.0, got.1];
        got.sort();
        assert_eq!(got, [LINK.to_string(), "aurelius://nonsense".to_string()]);
    }

    #[test]
    fn a_socket_left_by_a_crash_is_replaced() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(super::socket_path(dir.path()), b"stale").unwrap();
        assert!(!pass_to_running(dir.path(), LINK));
        let (tx, _rx) = channel();
        assert!(listen(dir.path(), tx));
        assert!(pass_to_running(dir.path(), LINK));
    }

    #[test]
    fn the_socket_is_private_from_the_first_moment_and_leaves_nothing_behind() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = channel();
        assert!(listen(dir.path(), tx));
        let mode = std::fs::metadata(super::socket_path(dir.path())).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let names: Vec<_> = std::fs::read_dir(dir.path()).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(names, [std::ffi::OsString::from("desktop-link.sock")], "the private folder is gone");
    }

    #[test]
    fn a_second_start_does_not_take_the_socket_of_a_live_listener() {
        let dir = tempfile::tempdir().unwrap();
        let (first, mut rx) = channel();
        assert!(listen(dir.path(), first));
        let (second, _rx2) = channel();
        assert!(!listen(dir.path(), second), "the live one stays");
        assert!(pass_to_running(dir.path(), LINK));
        let rt = tokio::runtime::Builder::new_current_thread().enable_time().build().unwrap();
        let got = rt.block_on(async { tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv()).await });
        assert_eq!(got.unwrap().as_deref(), Some(LINK));
    }

    #[test]
    fn many_starts_at_once_leave_exactly_one_listener() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let path = path.clone();
                std::thread::spawn(move || listen(&path, channel().0))
            })
            .collect();
        let wins = handles.into_iter().filter(|_| true).map(|h| usize::from(h.join().unwrap())).sum::<usize>();
        assert_eq!(wins, 1);
        assert!(pass_to_running(dir.path(), ""));
    }

    #[test]
    fn a_silent_client_does_not_hold_up_the_next_link() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, mut rx) = channel();
        assert!(listen(dir.path(), tx));
        // Connects and says nothing; the listener would read it for 2 s.
        let _silent = std::os::unix::net::UnixStream::connect(super::socket_path(dir.path())).unwrap();
        assert!(pass_to_running(dir.path(), LINK));
        let rt = tokio::runtime::Builder::new_current_thread().enable_time().build().unwrap();
        let got = rt.block_on(async { tokio::time::timeout(std::time::Duration::from_millis(1000), rx.recv()).await });
        assert_eq!(got.unwrap().as_deref(), Some(LINK));
    }
}
