//! Board members and their accounts (protocol 5). The daemon hashes every
//! password (argon2id) and every session token (SHA-256) before anything here
//! runs: `create_user` gets the domain command `CreateUser { username }` and the
//! PHC string beside it, and `account_write` gets a request hash and event
//! metadata the daemon built without them, so no command, receipt hash, event
//! or log holds a password, a token or a hash of either.

use crate::{COMMAND_METHOD, CommandContext, Committed, CommandService, domain_error, notice, request_hash_of};
use aurelius_domain::user::{CreateUser, UserView, UsersSnapshot, Username, WhoAmI, check_password};
use aurelius_domain::{ActorKind, AppError, CommandId, CommandResult, DomainError, ErrorCode, EventKind, UserId};
use aurelius_storage::events::{self, NewEvent};
use aurelius_storage::{StorageError, users};
use serde_json::json;

/// The cheap checks of `user.create`, before the password is hashed.
pub fn check_new_user(username: &str, password: &str) -> Result<Username, AppError> {
    let username = Username::parse(username).map_err(domain_error)?;
    check_password(password).map_err(domain_error)?;
    Ok(username)
}

fn view(row: users::UserRow) -> Option<UserView> {
    row.view()
}

pub fn list(conn: &rusqlite::Connection) -> Result<UsersSnapshot, StorageError> {
    Ok(UsersSnapshot { users: users::list(conn)?.into_iter().filter_map(view).collect() })
}

/// What the gate needs; `user` none when it is not (or no longer) a board member.
pub fn whoami(conn: &rusqlite::Connection, user: Option<UserId>) -> Result<WhoAmI, StorageError> {
    let user = match user {
        Some(id) => users::get(conn, &id.to_string())?.and_then(view),
        None => None,
    };
    Ok(WhoAmI { board_members_exist: users::count(conn)? > 0, user, must_change_password: false })
}

/// A write on accounts and sessions (`session.revoke`, `user.set_password`,
/// `user.disable`): what the daemon recorded for it. Nothing secret is in it.
pub struct AccountWrite {
    pub method: &'static str,
    /// Over the parameters without any password.
    pub request_hash: String,
    pub kind: EventKind,
    /// The board member it is about.
    pub user_id: UserId,
    /// Added to the event's metadata (beside `method`, the context and `by`).
    pub extra: serde_json::Value,
}

/// What an account write left: its result (as the receipt keeps it).
#[derive(Debug)]
pub struct AccountCommitted {
    pub result: serde_json::Value,
    pub committed_seq: u64,
    pub replayed: bool,
    pub notice: Option<aurelius_domain::EventNotice>,
}

type AccountAction = Box<dyn FnOnce(&rusqlite::Transaction<'_>) -> Result<Result<serde_json::Value, AppError>, StorageError> + Send>;

impl CommandService {
    /// One account write in one transaction: the change (`action`, which answers
    /// the result or why it is refused), its event and its receipt. The same key
    /// with the same request answers the stored result without doing it again.
    pub fn account_write(&self, ctx: &CommandContext, key: CommandId, write: AccountWrite, action: AccountAction) -> Result<AccountCommitted, AppError> {
        let id = key.to_string();
        let mut metadata = ctx.metadata();
        metadata["method"] = json!(write.method);
        metadata["by"] = json!(ctx.user_id);
        if let serde_json::Value::Object(extra) = write.extra {
            for (k, v) in extra {
                metadata[k] = v;
            }
        }
        let actor = ctx.principal_id.to_string();
        self.store
            .write(move |tx| {
                if let Some(stored) = events::find_receipt(tx, &id)? {
                    if stored.request_hash != write.request_hash {
                        return Ok(Err(crate::idempotency_conflict()));
                    }
                    let result = serde_json::from_str(&stored.result_json).unwrap_or(serde_json::Value::Null);
                    return Ok(Ok(AccountCommitted { result, committed_seq: stored.committed_seq, replayed: true, notice: None }));
                }
                let result = match action(tx)? {
                    Ok(result) => result,
                    Err(err) => return Ok(Err(err)),
                };
                // The next event of this board member (`user.created` was the first).
                let version: i64 = tx.query_row(
                    "SELECT COALESCE(MAX(aggregate_version), 0) + 1 FROM events WHERE aggregate_type = 'user' AND aggregate_id = ?1",
                    [write.user_id.to_string()],
                    |r| r.get(0),
                )?;
                let appended = events::append(
                    tx,
                    NewEvent {
                        aggregate_type: "user",
                        aggregate_id: write.user_id.to_string(),
                        aggregate_version: version as u64,
                        event_type: write.kind.as_str(),
                        project_id: None,
                        actor_kind: ActorKind::User,
                        actor_id: Some(actor),
                        correlation_id: id.clone(),
                        causation_id: None,
                        payload_blob_id: None,
                        metadata,
                        dedupe_key: format!("account:{id}"),
                    },
                )?;
                events::insert_receipt(tx, &id, &write.request_hash, write.method, &result.to_string(), appended.seq)?;
                let notice = notice(appended.seq, write.kind.as_str(), write.user_id.to_string(), None);
                Ok(Ok(AccountCommitted { result, committed_seq: appended.seq, replayed: false, notice: Some(notice) }))
            })
            .map_err(crate::storage_error)?
    }
}

/// The stored PHC string of a board member, only to check a password.
pub fn password_hash(conn: &rusqlite::Connection, user: UserId) -> Result<Option<String>, StorageError> {
    users::password_hash(conn, &user.to_string())
}

impl CommandService {
    /// `user.create` after the protocol hashed the password. The receipt's
    /// request hash covers the name only; the event `user.created` holds the
    /// new id (`created_user_id`), the name and who made it (`user_id`/`by`,
    /// none in the onboarding).
    ///
    /// Without a board member in `ctx` it is the onboarding, which only makes
    /// the very first one: that is checked in this write transaction (they are
    /// serialized), so of two at the same time exactly one succeeds and the
    /// other is `unauthenticated` (R8). The bootstrap window is the time between
    /// a fresh start of the server and that first board member: anyone with
    /// local access or a paired device may take it, so onboard right away.
    pub fn create_user(&self, ctx: &CommandContext, key: CommandId, expected_version: Option<u64>, cmd: CreateUser, password_hash: String) -> Result<Committed, AppError> {
        let username = Username::parse(&cmd.username).map_err(domain_error)?;
        let hash = request_hash_of(&(expected_version, "user.create", username.as_str()));
        COMMAND_METHOD.set("user.create");
        let meta = ctx.metadata();
        let actor = ctx.principal_id.to_string();
        let by = ctx.user_id;
        let result = self.commit(key, hash, move |tx| {
            if by.is_none() && users::count(tx)? > 0 {
                return Ok(Err(AppError::new(ErrorCode::Unauthenticated, "Sign in first.")));
            }
            if users::by_username(tx, username.as_str())?.is_some() {
                return Ok(Err(domain_error(DomainError::UsernameTaken)));
            }
            let id = UserId::new();
            let now = aurelius_storage::now_ms();
            let by_text = by.map(|u| u.to_string());
            match users::insert(tx, &users::NewUser { id: &id.to_string(), username: username.as_str(), created_by_user: by_text.as_deref(), now_ms: now }, &password_hash) {
                Ok(()) => {}
                // Another connection made the same name in between (MU1-20 AC4).
                Err(StorageError::Rejected(why)) if why == users::USERNAME_TAKEN => return Ok(Err(domain_error(DomainError::UsernameTaken))),
                Err(err) => return Err(err),
            }
            // `user_id` stays who made it (as in every event); the new one is `created_user_id`.
            let mut metadata = meta;
            metadata["created_user_id"] = json!(id);
            metadata["username"] = json!(username.as_str());
            metadata["by"] = json!(by);
            let appended = events::append(
                tx,
                NewEvent {
                    aggregate_type: "user",
                    aggregate_id: id.to_string(),
                    aggregate_version: 1,
                    event_type: EventKind::UserCreated.as_str(),
                    project_id: None,
                    actor_kind: ActorKind::User,
                    actor_id: Some(actor),
                    correlation_id: key.to_string(),
                    causation_id: None,
                    payload_blob_id: None,
                    metadata,
                    dedupe_key: format!("cmd:{key}:0"),
                },
            )?;
            Ok(Ok((CommandResult::UserCreated { user_id: id }, vec![notice(appended.seq, EventKind::UserCreated.as_str(), id.to_string(), None)])))
        });
        COMMAND_METHOD.set("");
        result
    }
}
