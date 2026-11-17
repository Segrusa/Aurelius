//! A board member chooses above how many tokens a session is compacted
//! (feature `compaction_setting`): read, changed, and refused outside its limits.

mod common;

use aurelius_client::ClientError;
use aurelius_domain::ErrorCode;
use common::*;

#[tokio::test(flavor = "multi_thread")]
async fn the_compaction_threshold_can_be_chosen_within_its_limits() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path()).await;
    let client = connect(dir.path()).await;
    assert!(aurelius_protocol::FEATURES.contains(&"compaction_setting"));

    let default = client.compaction_setting().await.unwrap();
    assert_eq!((default.threshold_tokens, default.default_tokens), (120_000, 120_000));
    assert!(default.min_tokens < default.default_tokens && default.default_tokens < default.max_tokens);

    assert_eq!(client.set_compact_threshold(400_000).await.unwrap().threshold_tokens, 400_000);
    assert_eq!(client.compaction_setting().await.unwrap().threshold_tokens, 400_000, "kept for the next runs");

    match client.set_compact_threshold(5).await {
        Err(ClientError::Protocol(e)) => assert!(e.code == ErrorCode::Validation && e.message.contains("threshold_tokens"), "{e:?}"),
        other => panic!("{other:?}"),
    }
    assert!(client.set_compact_threshold(default.max_tokens + 1).await.is_err());
    assert_eq!(client.compaction_setting().await.unwrap().threshold_tokens, 400_000, "a refused value changes nothing");

    assert_eq!(client.set_compact_threshold(0).await.unwrap().threshold_tokens, 0, "0: never");
    stop(dir.path(), daemon).await;
}
