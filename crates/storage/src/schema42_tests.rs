//! Migration 42: the History (ticket #57). A schema 41 store keeps every ticket as it
//! was; a ticket on Done gets the summary its project's memory has of it (A3), the
//! others none, and the new columns refuse what their CHECKs do not allow.

use crate::{Store, blobs, history};
use aurelius_domain::history::HistorySummaryState;
use rusqlite::Connection;

const P: &str = "00000000-0000-7000-8000-000000000001";
const ALEX: &str = "00000000-0000-7000-8000-000000000003";
/// On Done, with its summary in the project's memory.
const T1: &str = "00000000-0000-7000-8000-000000000011";
/// On Done, without a summary in memory.
const T2: &str = "00000000-0000-7000-8000-000000000012";
/// In Doing (a memory with its id does not put it in the History).
const T3: &str = "00000000-0000-7000-8000-000000000013";
/// On Done, but its summary in memory was forgotten.
const T4: &str = "00000000-0000-7000-8000-000000000014";

fn db(dir: &std::path::Path) -> Connection {
    let conn = Connection::open(dir.join("data/aurelius.sqlite")).unwrap();
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();
    conn
}

fn column(conn: &Connection, kind: &str) -> String {
    conn.query_row("SELECT id FROM board_columns WHERE kind = ?1", [kind], |r| r.get(0)).unwrap()
}

const TICKET_COLUMNS: &str = "id, number, title, column_id, position, priority, created_by_user, version, created_at_ms, updated_at_ms, done_at_ms, conversation_id, login_user, kind";

fn ticket_rows(conn: &Connection) -> Vec<String> {
    let sql = format!("SELECT json_array({TICKET_COLUMNS}) FROM tickets ORDER BY id");
    conn.prepare(&sql).unwrap().query_map([], |r| r.get(0)).unwrap().collect::<Result<_, _>>().unwrap()
}

/// A memory of the project about `ticket` with `text`, in `status`.
fn memory(store: &Store, id: &str, ticket: &str, text: &str, status: &str) {
    let blob = store.blobs().put(text.as_bytes(), "text/plain; charset=utf-8").unwrap();
    let (id, ticket, status) = (id.to_owned(), ticket.to_owned(), status.to_owned());
    store
        .write(move |tx| {
            blobs::register(tx, &blob)?;
            tx.execute(
                "INSERT INTO memory_items(id, scope_id, kind, content_blob_id, status, confidence, importance, valid_from_ms, extraction_version, created_at_ms, locator_json)
                 VALUES (?1, 'ps', 'project_knowledge', ?2, ?3, 0.8, 0.7, 1, 'v1', 1, json_object('ticket_id', ?4, 'ticket_number', 1))",
                rusqlite::params![id, blob.id.to_string(), status, ticket],
            )?;
            Ok(())
        })
        .unwrap();
}

/// A schema 41 store with Alex, #1, #2 and #4 on Done and #3 in Doing.
fn schema_41_database(dir: &std::path::Path) -> Vec<String> {
    let store = Store::open_until(dir, 41).unwrap();
    {
        let conn = db(dir);
        assert!(conn.prepare("SELECT summary_text FROM tickets").is_err(), "new in 42");
        let company: String = conn.query_row("SELECT id FROM company", [], |r| r.get(0)).unwrap();
        let global: String = conn.query_row("SELECT id FROM scopes WHERE kind = 'global'", [], |r| r.get(0)).unwrap();
        let (doing, done) = (column(&conn, "doing"), column(&conn, "done"));
        conn.execute_batch(&format!(
            "INSERT INTO scopes(id, kind, parent_id, owner_id) VALUES ('ps', 'project', '{global}', '{P}');
             INSERT INTO projects(id, scope_id, name, status, version, created_at_ms, updated_at_ms) VALUES ('{P}', 'ps', 'Alpha', 'active', 1, 1, 1);
             INSERT INTO users(id, company_id, username, password_hash, created_at_ms, updated_at_ms) VALUES ('{ALEX}', '{company}', 'alex', '$argon2id$x', 1, 1);
             INSERT INTO conversations(id, kind, project_id, title, version, created_at_ms) VALUES
               ('c1', 'ticket', '{P}', '#1', 1, 10), ('c2', 'ticket', '{P}', '#2', 1, 20), ('c3', 'ticket', '{P}', '#3', 1, 30), ('c4', 'ticket', '{P}', '#4', 1, 40);
             INSERT INTO tickets(id, company_id, project_id, number, title, column_id, position, priority, created_by_user,
                                 version, created_at_ms, updated_at_ms, done_at_ms, conversation_id, login_user) VALUES
               ('{T1}', '{company}', '{P}', 1, 'Fix the login', '{done}', 1.0, 'high', '{ALEX}', 4, 10, 11, 11, 'c1', '{ALEX}'),
               ('{T2}', '{company}', '{P}', 2, 'Write the docs', '{done}', 2.0, 'normal', '{ALEX}', 2, 20, 21, 21, 'c2', '{ALEX}'),
               ('{T3}', '{company}', '{P}', 3, 'Still busy', '{doing}', 1.0, 'normal', '{ALEX}', 2, 30, 31, NULL, 'c3', '{ALEX}'),
               ('{T4}', '{company}', '{P}', 4, 'Forgotten', '{done}', 3.0, 'normal', '{ALEX}', 2, 40, 41, 41, 'c4', '{ALEX}');"
        ))
        .unwrap();
    }
    memory(&store, "m1", T1, "Ticket #1 “Fix the login” is done: The login works again. Decided: no SSO yet.", "active");
    memory(&store, "m3", T3, "Ticket #3 “Still busy” is done: Not really.", "active");
    memory(&store, "m4", T4, "Ticket #4 “Forgotten” is done: Removed by a board member.", "forgotten");
    drop(store);
    ticket_rows(&db(dir))
}

fn states(conn: &Connection) -> Vec<(Option<String>, Option<String>)> {
    conn.prepare("SELECT summary_state, summary_text FROM tickets ORDER BY id").unwrap().query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().collect::<Result<_, _>>().unwrap()
}

#[test]
fn a_schema_41_store_keeps_its_tickets_and_a_done_one_gets_the_summary_from_its_memory() {
    let dir = tempfile::tempdir().unwrap();
    let before = schema_41_database(dir.path());

    let store = Store::open(dir.path()).unwrap();
    let conn = db(dir.path());
    assert_eq!(ticket_rows(&conn), before, "every ticket keeps what it had");
    assert_eq!(
        states(&conn),
        vec![
            (Some("ready".into()), Some("The login works again. Decided: no SSO yet.".into())),
            (None, None),
            (None, None),
            (None, None),
        ],
        "only #1: on Done with an active summary in memory"
    );

    let (rows, more) = store.read(|c| history::list(c, &history::Filter::default(), 10, "memory.extract")).unwrap();
    assert!(!more);
    let shown: Vec<(u32, HistorySummaryState)> = rows.iter().map(|r| (r.number, r.summary_state)).collect();
    assert_eq!(shown, vec![(4, HistorySummaryState::BeforeHistory), (2, HistorySummaryState::BeforeHistory), (1, HistorySummaryState::Ready)], "Done only, newest first");
    assert_eq!(rows[2].project_name, "Alpha");
}

#[test]
fn the_new_columns_refuse_what_their_checks_do_not_allow() {
    let dir = tempfile::tempdir().unwrap();
    schema_41_database(dir.path());
    drop(Store::open(dir.path()).unwrap());
    let conn = db(dir.path());
    assert!(conn.execute("UPDATE tickets SET summary_state = 'done' WHERE id = ?1", [T2]).is_err(), "only pending, ready or failed");
    assert!(conn.execute("UPDATE tickets SET summary_state = 'pending', summary_text = 'x' WHERE id = ?1", [T2]).is_err(), "a text only when ready");
    assert!(conn.execute("UPDATE tickets SET summary_state = 'ready', summary_text = '' WHERE id = ?1", [T2]).is_err(), "never empty");
    assert!(conn.execute("UPDATE tickets SET summary_state = 'ready', summary_text = ?2 WHERE id = ?1", [T2, &"a".repeat(801)]).is_err(), "at most 800 characters");
    conn.execute("UPDATE tickets SET summary_state = 'failed' WHERE id = ?1", [T2]).unwrap();
    conn.execute("UPDATE tickets SET summary_state = 'ready', summary_text = 'It shipped.' WHERE id = ?1", [T2]).unwrap();
}
