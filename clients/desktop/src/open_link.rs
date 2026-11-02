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

/// Listen for links from later processes and send them to `tx`. Call only when no app answered
/// `pass_to_running`: a socket left by a crash is replaced.
#[cfg(unix)]
pub fn listen(data_root: &std::path::Path, tx: UnboundedSender<String>) {
    use std::io::{BufRead, Read};
    let path = socket_path(data_root);
    let _ = std::fs::remove_file(&path);
    let Ok(listener) = std::os::unix::net::UnixListener::bind(&path) else { return };
    // Only this user may write to it.
    let _ = std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o600));
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(2)));
            let mut line = String::new();
            // Bounded, so a client that never ends its line cannot grow it.
            if std::io::BufReader::new(stream.take(MAX_LINK as u64)).read_line(&mut line).is_ok() {
                let url = line.trim();
                if !url.is_empty() && tx.send(url.to_string()).is_err() {
                    return;
                }
            }
        }
    });
}

#[cfg(not(unix))]
pub fn listen(_data_root: &std::path::Path, _tx: UnboundedSender<String>) {}

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
        listen(dir.path(), tx);
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
        // The probe sent nothing, and the order is kept.
        assert_eq!(got, (LINK.to_string(), "aurelius://nonsense".to_string()));
    }

    #[test]
    fn a_socket_left_by_a_crash_is_replaced() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(super::socket_path(dir.path()), b"stale").unwrap();
        assert!(!pass_to_running(dir.path(), LINK));
        let (tx, _rx) = channel();
        listen(dir.path(), tx);
        assert!(pass_to_running(dir.path(), LINK));
    }
}
