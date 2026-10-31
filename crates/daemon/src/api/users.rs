//! Board members on a connection (Fase 1: identification, not authentication).
//!
//! * `user.create` hashes the password here, before the application runs: the
//!   domain command is `CreateUser { username }` and the PHC string goes beside
//!   it straight to storage, so it is in no command, receipt hash, event or log.
//! * `user.use` is the one password check; a wrong password and an unknown board
//!   member answer the same `wrong_password` after the same work and a delay,
//!   and a connection gets at most `ATTEMPTS` tries per `WINDOW`.
//!
//! argon2id with the OWASP parameters (m = 19 MiB, t = 2, p = 1), a random salt
//! per password, as a PHC string (the parameters travel in it). Hashing and
//! checking cost tens of milliseconds of CPU, so they run on the blocking pool,
//! at most `HASHING` at a time over all connections (memory stays bounded).

use super::{Peer, app_task, internal, storage_task};
use crate::Shared;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng};
use argon2::{Algorithm, Argon2, Params, Version};
use aurelius_application::{CommandContext, user};
use aurelius_domain::user::{CreateUser, WhoAmI};
use aurelius_domain::{CommandId, CommandResult, UserId};
use aurelius_protocol::{ErrorCode, HostWrite, ProtocolError, Receipt, UserCreateParams, UserUseParams, WriteMethod, WriteResult};
use std::collections::VecDeque;
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;

/// Tries of `user.use` per connection within `WINDOW`.
pub(super) const ATTEMPTS: usize = 5;
pub(super) const WINDOW: Duration = Duration::from_secs(60);
/// Added to every wrong answer, so guessing is slow even within the limit.
const WRONG_DELAY: Duration = Duration::from_millis(500);
/// argon2 hashes or checks at the same time, daemon-wide: 4 × 19 MiB at most.
pub(super) const HASHING: usize = 4;
static HASHERS: Semaphore = Semaphore::const_new(HASHING);

fn hasher() -> Argon2<'static> {
    let params = Params::new(19 * 1024, 2, 1, None).expect("valid argon2 parameters");
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
}

fn hash(password: &str) -> Result<String, ProtocolError> {
    let salt = SaltString::generate(&mut OsRng);
    hasher().hash_password(password.as_bytes(), &salt).map(|h| h.to_string()).map_err(|_| internal("could not hash the password"))
}

/// Checked with the parameters stored in the PHC string.
fn verify(password: &str, phc: &str) -> bool {
    PasswordHash::new(phc).is_ok_and(|parsed| hasher().verify_password(password.as_bytes(), &parsed).is_ok())
}

/// What an unknown board member is checked against: the same work as a real one.
/// Should hashing fail, a fixed valid PHC string with the same parameters keeps
/// the work (an empty one would be refused at once, and answer faster).
static DECOY: LazyLock<String> = LazyLock::new(|| hash("not a board member's password").unwrap_or_else(|_| DECOY_FALLBACK.to_owned()));
const DECOY_FALLBACK: &str = "$argon2id$v=19$m=19456,t=2,p=1$bm90IGEgYm9hcmQgbWVtYmVy$cPg9lHz6gdbTJp8wOAKX6tvl1t5dzk5L1yEbyT7mGmI";

/// Compute `DECOY` now, at the start of the daemon, so the first unknown board
/// member does not pay a hash more than a known one.
pub fn warm_up() {
    LazyLock::force(&DECOY);
}

/// argon2 work on the blocking pool, waiting for one of the `HASHING` places.
async fn argon2_task<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T, ProtocolError> {
    let _place = HASHERS.acquire().await.map_err(internal)?;
    tokio::task::spawn_blocking(f).await.map_err(internal)
}

/// The tries of `user.use` on one connection; also its `user.create`s.
#[derive(Debug, Default)]
pub(super) struct Attempts(VecDeque<Instant>);

impl Attempts {
    /// Counts this try, or refuses it when the connection used up its tries.
    fn take(&mut self, now: Instant) -> Result<(), ProtocolError> {
        while self.0.front().is_some_and(|at| now.duration_since(*at) >= WINDOW) {
            self.0.pop_front();
        }
        if self.0.len() >= ATTEMPTS {
            return Err(ProtocolError::new(ErrorCode::TooManyAttempts, "Too many tries. Wait a minute and try again."));
        }
        self.0.push_back(now);
        Ok(())
    }
}

/// Counts a `user.create` on the connection: at most `ATTEMPTS` per `WINDOW`,
/// so a paired device cannot make board members (or hash) without end (S5-3).
/// One refused for its name or password costs nothing and is not counted;
/// other writes pass.
pub(super) fn count_create(creates: &mut Attempts, write: &WriteMethod) -> Result<(), ProtocolError> {
    match write {
        WriteMethod::Host(HostWrite::UserCreate(p)) if user::check_new_user(&p.username, &p.password).is_ok() => creates.take(Instant::now()),
        _ => Ok(()),
    }
}

/// `user.create`: check, hash, record; a connection without a board member (and
/// not an app) becomes the new one. A replayed receipt changes no connection.
pub(super) async fn create(shared: &Arc<Shared>, peer: &mut Peer, ctx: CommandContext, key: CommandId, expected_version: Option<u64>, params: UserCreateParams) -> Result<Receipt, ProtocolError> {
    let UserCreateParams { username, password } = params;
    let username = user::check_new_user(&username, &password)?;
    let phc = argon2_task(move || hash(&password)).await??;
    let s = shared.clone();
    let cmd = CreateUser { username: username.as_str().to_owned() };
    let committed = app_task(move || s.service.create_user(&ctx, key, expected_version, cmd, phc)).await?;
    let receipt = committed.receipt;
    if let (false, CommandResult::UserCreated { user_id }) = (receipt.replayed, &receipt.result) {
        if peer.user.is_none() && peer.app.is_none() {
            peer.user = Some(*user_id);
        }
    }
    shared.publish(committed.notices);
    Ok(Receipt { idempotency_key: receipt.command_id, committed_seq: receipt.committed_seq, replayed: receipt.replayed, result: WriteResult::Command(receipt.result) })
}

/// `user.use`: the one password check of Fase 1.
pub(super) async fn use_user(shared: &Arc<Shared>, peer: &mut Peer, attempts: &mut Attempts, params: UserUseParams) -> Result<WhoAmI, ProtocolError> {
    if peer.app.is_some() {
        return Err(ProtocolError::new(ErrorCode::Forbidden, aurelius_protocol::app::NOT_FOR_APPS));
    }
    attempts.take(Instant::now())?;
    let UserUseParams { user_id, password } = params;
    let store = shared.store().clone();
    let stored = storage_task(move || store.read(|c| user::password_hash(c, user_id))).await?;
    let found = stored.is_some();
    let right = argon2_task(move || {
        let phc = stored.unwrap_or_else(|| DECOY.clone());
        verify(&password, &phc) && found
    })
    .await?;
    if !right {
        tokio::time::sleep(WRONG_DELAY).await;
        return Err(ProtocolError::new(ErrorCode::WrongPassword, "That board member or password is wrong."));
    }
    peer.user = Some(user_id);
    whoami(shared, peer.user).await
}

pub(super) async fn whoami(shared: &Arc<Shared>, user: Option<UserId>) -> Result<WhoAmI, ProtocolError> {
    let store = shared.store().clone();
    storage_task(move || store.read(|c| user::whoami(c, user))).await
}

/// The hello's remembered board member: active when it exists (no password: it
/// was checked once with `user.use`), else none. Apps have no board member.
pub(super) async fn remembered(shared: &Arc<Shared>, peer: &mut Peer, user: Option<UserId>) -> Result<WhoAmI, ProtocolError> {
    if let (Some(id), None) = (user, &peer.app) {
        let known = whoami(shared, Some(id)).await?;
        peer.user = known.user.as_ref().map(|u| u.id);
        return Ok(known);
    }
    whoami(shared, peer.user).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hash_is_argon2id_with_the_owasp_parameters_and_a_fresh_salt() {
        let one = hash("correct horse battery").unwrap();
        let two = hash("correct horse battery").unwrap();
        assert!(one.starts_with("$argon2id$v=19$m=19456,t=2,p=1$"), "{one}");
        assert_ne!(one, two, "a salt per hash");
        assert!(verify("correct horse battery", &one));
        assert!(!verify("correct horse batterY", &one));
        assert!(!verify("anything", "not a phc string"));
    }

    #[test]
    fn the_decoy_fallback_is_a_real_hash_with_the_same_parameters() {
        let parsed = PasswordHash::new(DECOY_FALLBACK).expect("a valid PHC string");
        assert_eq!(parsed.to_string(), DECOY_FALLBACK);
        assert!(DECOY_FALLBACK.starts_with("$argon2id$v=19$m=19456,t=2,p=1$"));
        assert!(!verify("not a board member's password", DECOY_FALLBACK));
        warm_up();
        assert!(DECOY.starts_with("$argon2id$v=19$m=19456,t=2,p=1$"), "{}", *DECOY);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn at_most_hashing_argon2_runs_at_a_time() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NOW: AtomicUsize = AtomicUsize::new(0);
        static MOST: AtomicUsize = AtomicUsize::new(0);
        let runs = (0..HASHING * 3).map(|_| {
            tokio::spawn(argon2_task(|| {
                let at = NOW.fetch_add(1, Ordering::SeqCst) + 1;
                MOST.fetch_max(at, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(30));
                NOW.fetch_sub(1, Ordering::SeqCst);
            }))
        });
        for run in runs.collect::<Vec<_>>() {
            run.await.unwrap().unwrap();
        }
        // Other tests may hash at the same time and hold places: only the bound is exact.
        let most = MOST.load(Ordering::SeqCst);
        assert!((1..=HASHING).contains(&most), "{most} at a time");
    }

    #[test]
    fn five_tries_a_minute_per_connection() {
        let mut tries = Attempts::default();
        let start = Instant::now();
        for _ in 0..ATTEMPTS {
            tries.take(start).unwrap();
        }
        assert_eq!(tries.take(start).unwrap_err().code, ErrorCode::TooManyAttempts);
        assert!(tries.take(start + WINDOW).is_ok(), "a minute later it opens again");
    }

    #[test]
    fn five_new_board_members_a_minute_per_connection() {
        let create = |username: &str, password: &str| WriteMethod::Host(HostWrite::UserCreate(UserCreateParams { username: username.into(), password: password.into() }));
        let mut creates = Attempts::default();
        for i in 0..ATTEMPTS {
            count_create(&mut creates, &create(&format!("user{i}"), "a-long-password")).unwrap();
        }
        assert_eq!(count_create(&mut creates, &create("more", "a-long-password")).unwrap_err().code, ErrorCode::TooManyAttempts);
        // Refused anyway (name, password) or another write: not counted, not stopped here.
        assert!(count_create(&mut creates, &create("x", "a-long-password")).is_ok());
        assert!(count_create(&mut creates, &create("more", "short")).is_ok());
        assert!(count_create(&mut creates, &WriteMethod::Host(HostWrite::McpSet { enabled: vec![] })).is_ok());
    }
}
