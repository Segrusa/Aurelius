//! Board members (Fase 1: identification). The protocol hashes the password
//! (argon2id) before anything here runs: `create_user` gets the domain command
//! `CreateUser { username }` and the PHC string beside it, so no command,
//! receipt hash, event or log holds the password or its hash.

use crate::{COMMAND_METHOD, CommandContext, Committed, CommandService, domain_error, notice, request_hash_of};
use aurelius_domain::user::{CreateUser, UserView, UsersSnapshot, Username, WhoAmI, check_password};
use aurelius_domain::{ActorKind, AppError, CommandId, CommandResult, DomainError, EventKind, UserId};
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
    Some(UserView {
        id: UserId::parse(&row.id).ok()?,
        username: row.username,
        created_at_ms: row.created_at_ms,
        created_by_user: row.created_by_user.and_then(|u| UserId::parse(&u).ok()),
    })
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
    Ok(WhoAmI { board_members_exist: users::count(conn)? > 0, user })
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
    pub fn create_user(&self, ctx: &CommandContext, key: CommandId, expected_version: Option<u64>, cmd: CreateUser, password_hash: String) -> Result<Committed, AppError> {
        let username = Username::parse(&cmd.username).map_err(domain_error)?;
        let hash = request_hash_of(&(expected_version, "user.create", username.as_str()));
        COMMAND_METHOD.set("user.create");
        let meta = ctx.metadata();
        let actor = ctx.principal_id.to_string();
        let by = ctx.user_id;
        let result = self.commit(key, hash, move |tx| {
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
