use crate::users::{self, NewUser, USERNAME_TAKEN};
use crate::{Store, StorageError};

const PHC: &str = "$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHQ$aGFzaGhhc2hoYXNoaGFzaA";
const A: &str = "00000000-0000-7000-8000-000000099999";

fn uid(n: u32) -> String {
    format!("00000000-0000-7000-8000-0000000{n:05}")
}

fn add(store: &Store, id: &str, name: &str, by: Option<&str>, at: i64) -> Result<(), StorageError> {
    let (id, name, by) = (id.to_string(), name.to_string(), by.map(str::to_string));
    store.write(move |tx| users::insert(tx, &NewUser { id: &id, username: &name, created_by_user: by.as_deref(), now_ms: at }, PHC))
}

#[test]
fn board_members_are_added_listed_and_counted() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    assert_eq!(store.read(|c| users::count(c)).unwrap(), 0);

    add(&store, &uid(1), "grace", None, 10).unwrap();
    add(&store, &uid(2), "ada", Some(&uid(1)), 20).unwrap();

    store
        .read(|c| {
            assert_eq!(users::count(c)?, 2);
            let all = users::list(c)?;
            assert_eq!(all.iter().map(|u| u.username.as_str()).collect::<Vec<_>>(), ["grace", "ada"], "oldest first");
            let company: String = c.query_row("SELECT id FROM company", [], |r| r.get(0))?;
            assert!(all.iter().all(|u| u.company_id == company));
            let ada = users::get(c, &uid(2))?.unwrap();
            assert_eq!(ada.created_by_user.as_deref(), Some(uid(1).as_str()));
            assert_eq!((ada.created_at_ms, ada.updated_at_ms, ada.version), (20, 20, 1));
            assert!(users::get(c, A)?.is_none());
            // The lookup by name ignores case, like the unique index.
            assert_eq!(users::by_username(c, "GRACE")?.unwrap().id, uid(1));
            assert!(users::by_username(c, "nobody")?.is_none());
            assert_eq!(users::password_hash(c, &uid(1))?.as_deref(), Some(PHC));
            Ok(())
        })
        .unwrap();
}

#[test]
fn a_name_is_unique_whatever_its_case() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    add(&store, &uid(1), "Grace", None, 1).unwrap();
    let err = add(&store, &uid(2), "grace", None, 2).unwrap_err();
    assert!(matches!(&err, StorageError::Rejected(m) if m == USERNAME_TAKEN), "{err}");
    assert_eq!(store.read(|c| users::count(c)).unwrap(), 1);
}

#[test]
fn a_name_has_3_to_32_characters() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    assert!(add(&store, &uid(1), "ab", None, 1).is_err());
    assert!(add(&store, &uid(2), &"x".repeat(33), None, 1).is_err());
    add(&store, &uid(3), "abc", None, 1).unwrap();
    add(&store, &uid(4), &"y".repeat(32), None, 1).unwrap();
    // An unknown creator is a broken reference.
    assert!(add(&store, &uid(5), "orphan", Some(A), 1).is_err());
}

#[test]
fn a_read_never_carries_the_hash() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    add(&store, &uid(1), "grace", None, 1).unwrap();
    let shown = store.read(|c| Ok(format!("{:?} {:?} {:?}", users::list(c)?, users::get(c, &uid(1))?, users::by_username(c, "grace")?))).unwrap();
    assert!(!shown.contains("argon2"), "{shown}");
}

#[test]
fn owners_reports_an_unreadable_row_instead_of_skipping_it() {
    // Trace R1 m6: a read error must not quietly leave the Owner out of a ticket's channel.
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    add(&store, &uid(1), "grace", None, 10).unwrap();
    add(&store, &uid(2), "ada", Some(&uid(1)), 20).unwrap();
    let owners = store.read(crate::discussions::owners).unwrap();
    assert_eq!(owners.iter().map(ToString::to_string).collect::<Vec<_>>(), [uid(1)], "only the one made in the onboarding");
    add(&store, "not-a-user-id", "broken", None, 30).unwrap();
    assert!(store.read(crate::discussions::owners).is_err());
}
