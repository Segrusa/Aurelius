//! SEC1 m7: a panic goes to the log through the redactor, never as it is.
//! Its own test binary: the panic hook and the subscriber are the process's.

use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn a_panic_is_logged_redacted() {
    let value = "sk-test-FAKE-panic-hook-0000";
    aurelius_runtime::redact::add_to_set("panic-hook-test", aurelius_runtime::redact::Secret { name: "PANIC_TOKEN".into(), label: String::new(), value: zeroize::Zeroizing::new(value.into()) });
    let captured = Captured::default();
    let sink = captured.clone();
    tracing_subscriber::fmt().with_writer(move || sink.clone()).with_ansi(false).init();
    aurelius_daemon::install_panic_hook();

    let outcome = std::thread::Builder::new().name("panicking".into()).spawn(move || panic!("could not use {value}")).unwrap().join();
    assert!(outcome.is_err());
    let line = String::from_utf8(captured.0.lock().unwrap().clone()).unwrap();
    assert!(line.contains("panic") && line.contains("«secret:PANIC_TOKEN»"), "{line}");
    assert!(!line.contains("FAKE-panic"), "{line}");
    assert!(line.contains("panic_hook.rs"), "{line}");
}
