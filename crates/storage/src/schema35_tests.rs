//! Migration 35: a usage record counts its model calls (U4). A schema 34 store keeps
//! its records, their calls not known (0).

use crate::{Store, usage};
use aurelius_domain::RunId;
use rusqlite::{Connection, params};

const RUN: &str = "00000000-0000-7000-8000-000000000351";
const CHAT: &str = "00000000-0000-7000-8000-000000000352";

fn run_row(conn: &Connection) {
    conn.pragma_update(None, "foreign_keys", "OFF").unwrap();
    conn.execute("INSERT INTO runs(id, conversation_id, attempt, runtime_kind, model_id, resolved_config_json, status) VALUES (?1, ?2, 1, 'fake', '', '{}', 'succeeded')", params![RUN, CHAT]).unwrap();
}

#[test]
fn a_schema_34_store_keeps_its_records_with_calls_not_known() {
    let dir = tempfile::tempdir().unwrap();
    drop(Store::open_until(dir.path(), 34).unwrap());
    let conn = Connection::open(dir.path().join("data/aurelius.sqlite")).unwrap();
    run_row(&conn);
    conn.execute(
        "INSERT INTO usage_records(id, run_id, source_event_id, input_tokens, output_tokens, cached_tokens, estimate, provider_record_key, recorded_at_ms, cache_creation_tokens)
         VALUES ('u1', ?1, 'e', 10, 2, 0, 1, 'k', 5, 0)",
        [RUN],
    )
    .unwrap();
    assert!(conn.prepare("SELECT model_calls FROM usage_records").is_err(), "the column is new in 35");
    drop(conn);

    let store = Store::open(dir.path()).unwrap();
    let run = RunId::parse(RUN).unwrap();
    let before = store.read(move |c| usage::for_run(c, run)).unwrap().unwrap();
    assert_eq!((before.input_tokens, before.model_calls), (10, 0));

    let conn = Connection::open(dir.path().join("data/aurelius.sqlite")).unwrap();
    conn.pragma_update(None, "foreign_keys", "OFF").unwrap();
    conn.execute(
        "INSERT INTO usage_records(id, run_id, source_event_id, input_tokens, output_tokens, cached_tokens, estimate, provider_record_key, recorded_at_ms, cache_creation_tokens, model_calls)
         VALUES ('u2', ?1, 'e', 5, 1, 0, 1, 'k2', 6, 0, 3)",
        [RUN],
    )
    .unwrap();
    drop(conn);
    let after = store.read(move |c| usage::for_run(c, run)).unwrap().unwrap();
    assert_eq!((after.input_tokens, after.model_calls, after.runs), (15, 3, 1));
}
