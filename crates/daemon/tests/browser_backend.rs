//! The browser backend (Claude in Chrome or Playwright): sent with `browser.get`,
//! changed with `browser.set_backend`, kept over a restart.

mod common;

use aurelius_protocol::{BrowserBackend, BrowserChoice};
use aurelius_runtime::fake::FakeRuntime;
use common::*;
use std::sync::Arc;

#[tokio::test(flavor = "multi_thread")]
async fn the_backend_choice_is_kept_over_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let daemon = start_with(&root, Arc::new(FakeRuntime::default())).await;
    let client = connect(&root).await;
    assert_eq!(client.browser_setting().await.unwrap().backend, Some(BrowserBackend::ClaudeInChrome), "Claude in Chrome by default");

    let set = client.set_browser_backend(BrowserBackend::Playwright).await.unwrap();
    assert_eq!(set.backend, Some(BrowserBackend::Playwright));
    // The browser choice (Chrome or Edge) stays as it was and still applies: Playwright starts that browser.
    let set = client.set_browser(BrowserChoice::Edge).await.unwrap();
    assert_eq!((set.choice, set.backend), (BrowserChoice::Edge, Some(BrowserBackend::Playwright)));
    stop(&root, daemon).await;

    let daemon = start_with(&root, Arc::new(FakeRuntime::default())).await;
    let client = connect(&root).await;
    let kept = client.browser_setting().await.unwrap();
    assert_eq!((kept.choice, kept.backend), (BrowserChoice::Edge, Some(BrowserBackend::Playwright)));
    client.set_browser_backend(BrowserBackend::ClaudeInChrome).await.unwrap();
    assert_eq!(client.browser_setting().await.unwrap().backend, Some(BrowserBackend::ClaudeInChrome));
    stop(&root, daemon).await;
}
