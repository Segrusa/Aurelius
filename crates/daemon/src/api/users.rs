//! Board members and sessions on a connection (protocol 5).
//!
//! * `user.create` hashes the password here, before the application runs: the
//!   domain command is `CreateUser { username }` and the PHC string goes beside
//!   it straight to storage, so it is in no command, receipt hash, event or log.
//! * `user.login` checks the password; a wrong password and an unknown board
//!   member answer the same `wrong_password` after the same work and a delay,
//!   and a connection gets at most `ATTEMPTS` tries per `WINDOW`.
//!
//! argon2id with Sentinel's target parameters (m = 64 MiB, t = 3, p = 1; A1:
//! measured at about 140 ms a hash on the server, well under the 1 s bound, so
//! not the OWASP minimum), a random salt per password, as a PHC string (the
//! parameters travel in it; an older, weaker hash is replaced at the next
//! sign-in). Hashing and checking cost CPU, so they run on the blocking pool,
//! at most `HASHING` at a time over all connections (memory stays bounded),
//! and a try that waits longer than `HASH_WAIT` for a place is refused.

use super::{Peer, app_task, internal, storage_task};
use crate::Shared;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng};
use argon2::{Algorithm, Argon2, Params, Version};
use aurelius_application::{CommandContext, user};
use aurelius_domain::user::{ClaudeLoginStatus, ClaudeLoginView, CreateUser, LoginResult, PASSWORD_MAX, SessionView, SessionsSnapshot, SignOutReason, Username, WhoAmI};
use aurelius_domain::{CommandId, CommandResult, UserId};
use aurelius_protocol::{ErrorCode, HostWrite, LoginParams, Method, ProtocolError, Receipt, ResponseBody, UserCreateParams, WriteResult};
use std::collections::VecDeque;
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};
use tokio::sync::{Semaphore, mpsc};

/// Tries of `user.login` (and of an old password) per connection within `WINDOW`.
pub(super) const ATTEMPTS: usize = 5;
pub(super) const WINDOW: Duration = Duration::from_secs(60);
/// Added to every wrong answer, so guessing is slow even within the limit.
const WRONG_DELAY: Duration = Duration::from_millis(500);
/// argon2 hashes or checks at the same time, daemon-wide: 4 × 64 MiB at most.
pub(super) const HASHING: usize = 4;
static HASHERS: Semaphore = Semaphore::const_new(HASHING);
/// How long a hash waits for one of the `HASHING` places before it is refused (A2).
const HASH_WAIT: Duration = Duration::from_secs(10);
/// An open connection checks its session against the database this often
/// (`Shared::session_check_ms`), so an end that did not reach it signs it out.
pub const SESSION_CHECK_MS: i64 = 60_000;
/// And marks it used at most this often (S6).
const TOUCH_EVERY_MS: i64 = 3_600_000;

fn hasher() -> Argon2<'static> {
    let params = Params::new(64 * 1024, 3, 1, None).expect("valid argon2 parameters");
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
const DECOY_FALLBACK: &str = "$argon2id$v=19$m=65536,t=3,p=1$oyzIKK13v0odviZvk/M75g$EaDCwTNlgu3MRh7MGueRjlsfy9ay5V6Em9AI2VmmHDQ";

/// Compute `DECOY` now, at the start of the daemon, so the first unknown board
/// member does not pay a hash more than a known one.
pub fn warm_up() {
    LazyLock::force(&DECOY);
}

/// argon2 work on the blocking pool, waiting for one of the `HASHING` places
/// (`too_many_attempts` after `HASH_WAIT`).
async fn argon2_task<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T, ProtocolError> {
    argon2_within(HASH_WAIT, f).await
}

async fn argon2_within<T: Send + 'static>(wait: Duration, f: impl FnOnce() -> T + Send + 'static) -> Result<T, ProtocolError> {
    let _place = match tokio::time::timeout(wait, HASHERS.acquire()).await {
        Ok(place) => place.map_err(internal)?,
        Err(_) => return Err(ProtocolError::too_many_attempts(1_000)),
    };
    tokio::task::spawn_blocking(f).await.map_err(internal)
}

/// The tries of `user.login` on one connection; also its `user.create`s.
#[derive(Debug, Default)]
pub(super) struct Attempts(VecDeque<Instant>);

impl Attempts {
    /// Counts this try, or refuses it when the connection used up its tries.
    fn take(&mut self, now: Instant) -> Result<(), ProtocolError> {
        while self.0.front().is_some_and(|at| now.duration_since(*at) >= WINDOW) {
            self.0.pop_front();
        }
        if let (true, Some(oldest)) = (self.0.len() >= ATTEMPTS, self.0.front()) {
            let wait = WINDOW - now.duration_since(*oldest);
            return Err(ProtocolError::too_many_attempts(wait.as_millis().max(1) as u64));
        }
        self.0.push_back(now);
        Ok(())
    }
}

/// Counts a `user.create` that will be hashed: at most `ATTEMPTS` per `WINDOW`
/// on the connection, so a paired device cannot make board members (or hash)
/// without end (S5-3), and `limits::CREATES_PER_HOUR` on the server (B9).
fn count_create(limits: &super::limits::Limits, creates: &mut Attempts) -> Result<(), ProtocolError> {
    let now = Instant::now();
    creates.take(now)?;
    limits.take_create(now)
}

/// `user.create`: check, hash, record. Without a session only the first board
/// member (the onboarding, decided in the write transaction: of two at once
/// exactly one): that connection then works as them, on this connection only
/// (no token; signing in gives one). A replayed receipt changes no connection.
/// Counted (`count_create`) only once it gets this far that it is hashed: one
/// refused for the session, its name or its password costs nothing (R5).
#[allow(clippy::too_many_arguments)]
pub(super) async fn create(
    shared: &Arc<Shared>,
    peer: &mut Peer,
    creates: &mut Attempts,
    end: &mpsc::UnboundedSender<SignOutReason>,
    ctx: CommandContext,
    key: CommandId,
    expected_version: Option<u64>,
    params: UserCreateParams,
) -> Result<Receipt, ProtocolError> {
    if peer.app.is_some() {
        return Err(ProtocolError::new(ErrorCode::Forbidden, aurelius_protocol::app::NOT_FOR_APPS));
    }
    // Without a session nothing is said about the name or the password once the onboarding is over.
    let bootstrap = peer.session.is_none();
    if bootstrap && whoami(shared, None).await?.board_members_exist {
        return Err(unauthenticated());
    }
    let UserCreateParams { username, password } = params;
    let username = user::check_new_user(&username, &password)?;
    count_create(&shared.sign_in_limits, creates)?;
    let phc = argon2_task(move || hash(&password)).await??;
    let s = shared.clone();
    let cmd = CreateUser { username: username.as_str().to_owned() };
    let committed = app_task(move || s.service.create_user(&ctx, key, expected_version, cmd, phc)).await?;
    let receipt = committed.receipt;
    if let (false, true, CommandResult::UserCreated { user_id }) = (receipt.replayed, bootstrap, &receipt.result) {
        peer.user = Some(*user_id);
        peer.session = Some(PeerSession { id: None, must_change_password: false, expires_at_ms: i64::MAX, seen_at_ms: now_ms(), checked_at_ms: now_ms() });
        register(shared, peer, end);
    }
    shared.publish(committed.notices);
    Ok(Receipt { idempotency_key: receipt.command_id, committed_seq: receipt.committed_seq, replayed: receipt.replayed, result: WriteResult::Command(receipt.result) })
}

/// The signed-in board member's own password, asked again (`vault.key_export`,
/// SEC0 E9): the same per-connection tries, sign-in limits per username,
/// argon2 work and wrong answer as `user.login`.
pub(super) async fn confirm_password(shared: &Arc<Shared>, peer: &Peer, attempts: &mut Attempts, password: String) -> Result<UserId, ProtocolError> {
    let Some(user) = peer.user.filter(|_| peer.app.is_none() && peer.session.is_some()) else {
        return Err(ProtocolError::new(ErrorCode::BoardMemberRequired, "Only a signed-in board member can do this."));
    };
    let now = Instant::now();
    attempts.take(now)?;
    let store = shared.store().clone();
    let id = user.to_string();
    let stored = storage_task(move || {
        store.read(|c| {
            let Some(row) = aurelius_storage::users::get(c, &id)? else { return Ok(None) };
            Ok(aurelius_storage::users::password_hash(c, &id)?.map(|phc| (phc, row.username)))
        })
    })
    .await?;
    let Some((phc, username)) = stored else { return Err(wrong_login()) };
    let name = super::limits::key(&username);
    let attempt = shared.sign_in_limits.login_begin(&name, now)?;
    let right = argon2_task(move || password.len() <= PASSWORD_MAX && verify(&password, &phc)).await?;
    if !right {
        let (failures, _) = attempt.failed(Instant::now());
        tracing::info!(username = %name, conn = peer.conn, failures, "password check failed");
        tokio::time::sleep(WRONG_DELAY).await;
        return Err(wrong_login());
    }
    attempt.succeeded();
    Ok(user)
}

/// The session of a connection (protocol 5). `id` none: the first board member
/// of the onboarding, on that connection only.
#[derive(Debug, Clone, PartialEq)]
pub struct PeerSession {
    pub id: Option<String>,
    pub must_change_password: bool,
    pub expires_at_ms: i64,
    /// When it was last marked used in the database.
    pub seen_at_ms: i64,
    /// When it was last found valid in the database (`keep_session`).
    pub checked_at_ms: i64,
}

/// The hub's view of this connection: its board member and session now.
fn register(shared: &Shared, peer: &Peer, end: &mpsc::UnboundedSender<SignOutReason>) {
    let now = peer.user.filter(|_| peer.app.is_none()).map(|u| (peer.session.as_ref().and_then(|s| s.id.clone()), u, device(peer)));
    shared.sessions.set(peer.conn, now, end);
}

/// A device name as others see it in the session list and the timeline: no
/// control or bidirectional characters (SEC1 m13), at most 80 characters.
pub(super) fn clean_device_name(name: &str) -> String {
    let bidi = |c: char| matches!(c, '\u{200E}' | '\u{200F}' | '\u{061C}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}');
    let kept: String = name.chars().filter(|c| !c.is_control() && !bidi(*c)).collect();
    match kept.trim() {
        "" => "Unknown device".to_owned(),
        name => name.chars().take(80).collect(),
    }
}

fn now_ms() -> i64 {
    aurelius_storage::now_ms()
}

pub(super) fn unauthenticated() -> ProtocolError {
    ProtocolError::new(ErrorCode::Unauthenticated, "Sign in first.")
}

fn wrong_login() -> ProtocolError {
    ProtocolError::new(ErrorCode::WrongPassword, "Username or password is wrong.")
}

/// The device a session belongs to: the paired device, none for local IPC.
fn device(peer: &Peer) -> Option<aurelius_domain::ClientId> {
    peer.remote.as_ref().map(|_| peer.client_id)
}

/// Whether a PHC string was made with weaker parameters than `hasher` uses now.
fn weaker(phc: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(phc) else { return false };
    let Ok(params) = Params::try_from(&parsed) else { return true };
    let now = hasher();
    let now = now.params();
    parsed.algorithm.as_str() != "argon2id" || params.m_cost() < now.m_cost() || params.t_cost() < now.t_cost() || params.p_cost() < now.p_cost()
}

/// `user.login`: one answer for an unknown username, a disabled board member
/// and a wrong password, after the same argon2 work and a delay; the limits of
/// the connection and of the username (B9). A right one makes a session for
/// this device (a remote one is tied to its paired device) and answers its
/// token once; an earlier session of this connection ends.
///
/// The try of the username is reserved before its work (R1 M-3). The
/// connection is in the hub before the session is stored, and the transaction
/// that stores it checks that the password checked is still the board
/// member's and that they are not disabled: an end committed before it is seen
/// there, one after it reaches this connection (R1 M-2).
pub(super) async fn login(shared: &Arc<Shared>, peer: &mut Peer, attempts: &mut Attempts, end: &mpsc::UnboundedSender<SignOutReason>, params: LoginParams) -> Result<LoginResult, ProtocolError> {
    if peer.app.is_some() {
        return Err(ProtocolError::new(ErrorCode::Forbidden, aurelius_protocol::app::NOT_FOR_APPS));
    }
    let now = Instant::now();
    attempts.take(now)?;
    let LoginParams { username, password, device_name } = params;
    // A name no board member can have: the same work, wait and answer, but no
    // count and nothing kept for it (SEC1 M3: the limits stay small).
    let Ok(username) = Username::parse(&username) else {
        argon2_task(|| verify("", &DECOY)).await?;
        tokio::time::sleep(WRONG_DELAY).await;
        return Err(wrong_login());
    };
    let name = super::limits::key(username.as_str());
    let attempt = shared.sign_in_limits.login_begin(&name, now)?;
    let store = shared.store().clone();
    let stored = storage_task(move || {
        store.read(|c| {
            let Some(row) = aurelius_storage::users::by_username(c, username.as_str())? else { return Ok(None) };
            Ok(aurelius_storage::users::password_hash(c, &row.id)?.map(|phc| (phc, row)))
        })
    })
    .await?;
    let (phc, row) = match stored {
        Some((phc, row)) => (phc, Some(row)),
        None => (DECOY.clone(), None),
    };
    let (checked, active) = (phc.clone(), row.as_ref().is_some_and(|r| !r.disabled()));
    let (right, rehashed) = argon2_task(move || {
        // Longer than any password (A5): wrong, after the same work.
        if password.len() > PASSWORD_MAX {
            verify("", &checked);
            return (false, None);
        }
        let right = verify(&password, &checked);
        // Stronger parameters than the stored hash: hash it again now; never for
        // a disabled one (SEC1 m8: the extra work would say the password is right).
        let rehashed = (right && active && weaker(&checked)).then(|| hash(&password).ok()).flatten();
        (right, rehashed)
    })
    .await?;
    let Some(row) = row.filter(|r| right && !r.disabled()) else {
        let (failures, wait) = attempt.failed(Instant::now());
        // L3/R4: the name as counted, never the password or whether the name exists.
        tracing::info!(username = %name, conn = peer.conn, failures, "sign-in failed");
        if !wait.is_zero() {
            tracing::warn!(username = %name, conn = peer.conn, failures, wait_s = wait.as_secs(), "sign-in: too many failures, the next try waits");
        }
        tokio::time::sleep(WRONG_DELAY).await;
        return Err(wrong_login());
    };
    attempt.succeeded();
    let user = row.view().ok_or_else(|| internal("a damaged board member id"))?;
    let token = super::sessions::new_token();
    let verifier = super::sessions::verifier(&token);
    let (session_id, client) = (aurelius_domain::SessionId::new().to_string(), device(peer));
    let device_name = clean_device_name(&device_name);
    shared.sessions.set(peer.conn, Some((Some(session_id.clone()), user.id, client)), end);
    let (user_id, earlier) = (row.id.clone(), peer.session.as_ref().and_then(|s| s.id.clone()));
    let id = session_id.clone();
    let store = shared.store().clone();
    let session = storage_task(move || {
        store.write(move |tx| {
            let at = aurelius_storage::now_ms();
            let still = aurelius_storage::users::get(tx, &user_id)?.is_some_and(|u| !u.disabled())
                && aurelius_storage::users::password_hash(tx, &user_id)?.as_deref() == Some(phc.as_str());
            if !still {
                return Ok(None);
            }
            if let Some(phc) = rehashed {
                aurelius_storage::users::replace_hash(tx, &user_id, &phc)?;
            }
            if let Some(earlier) = earlier {
                if let Some(old) = aurelius_storage::user_sessions::get(tx, &earlier)? {
                    aurelius_storage::user_sessions::revoke(tx, &old.user_id, &earlier, at)?;
                }
            }
            let new = aurelius_storage::user_sessions::NewSession { id: &id, user_id: &user_id, client_id: client, device_name: &device_name, now_ms: at };
            aurelius_storage::user_sessions::insert(tx, &new, &verifier)?;
            // A device revoked in between: the session is no good.
            aurelius_storage::user_sessions::valid(tx, &id, at)
        })
    })
    .await;
    let session = match session {
        Ok(Some(session)) => session,
        // A new password or a disable came in between: as a wrong password.
        outcome => {
            register(shared, peer, end);
            outcome?;
            tokio::time::sleep(WRONG_DELAY).await;
            return Err(wrong_login());
        }
    };
    let earlier = peer.session.as_ref().and_then(|s| s.id.clone());
    if let Some(earlier) = &earlier {
        shared.sessions.end_session(earlier, SignOutReason::Revoked, Some(peer.conn));
    }
    // Another board member on this connection: as a logout, the earlier one's
    // sign-in to Claude ends (R4 m-2); `api::run` drops this connection's.
    if let (Some(earlier), true) = (&earlier, peer.user.is_some_and(|u| u != user.id)) {
        shared.claude_links.cancel_session(earlier);
    }
    peer.user = Some(user.id);
    let checked_at_ms = now_ms();
    peer.session = Some(PeerSession { id: Some(session_id), must_change_password: row.must_change_password, expires_at_ms: session.expires_at_ms, seen_at_ms: session.last_seen_ms, checked_at_ms });
    Ok(LoginResult { user, session_token: token, expires_at_ms: session.expires_at_ms, must_change_password: row.must_change_password })
}

/// `user.logout`: the session ends (its other connections close); this
/// connection stays, without a board member.
pub(super) async fn logout(shared: &Arc<Shared>, peer: &mut Peer) -> Result<WhoAmI, ProtocolError> {
    if let (Some(id), Some(user)) = (peer.session.as_ref().and_then(|s| s.id.clone()), peer.user) {
        let (store, sid) = (shared.store().clone(), id.clone());
        storage_task(move || store.write(move |tx| aurelius_storage::user_sessions::revoke(tx, &user.to_string(), &sid, aurelius_storage::now_ms()))).await?;
        shared.sessions.end_session(&id, SignOutReason::LoggedOut, Some(peer.conn));
        // A sign-in to Claude of this session ends with it (C13).
        shared.claude_links.cancel_session(&id);
    }
    shared.claude_links.cancel_conn(peer.conn);
    peer.user = None;
    peer.session = None;
    whoami(shared, None).await
}

pub(super) async fn whoami(shared: &Arc<Shared>, user: Option<UserId>) -> Result<WhoAmI, ProtocolError> {
    let store = shared.store().clone();
    storage_task(move || store.read(|c| user::whoami(c, user))).await
}

/// `user.whoami` of a connection: with its session's temporary-password flag.
pub(super) async fn whoami_of(shared: &Arc<Shared>, peer: &Peer) -> Result<WhoAmI, ProtocolError> {
    let mut who = whoami(shared, peer.user).await?;
    who.must_change_password = peer.session.as_ref().is_some_and(|s| s.must_change_password);
    Ok(who)
}

/// What the hello answers about the board member: the session's, and their Claude login.
pub(super) struct HelloWho {
    pub who: WhoAmI,
    pub claude_login: Option<ClaudeLoginView>,
}

/// The hello's session: a valid token of this device makes its board member
/// this connection's; an unknown, revoked or expired one (or one of another
/// device) leaves the connection without one. No token: the connection keeps
/// what it has. Apps have no board member.
///
/// The device is checked before the session is marked used (S5, R1 m-8), and
/// the connection is in the hub before the session is checked once more (and
/// marked used, at most once an hour): an end committed before that check is
/// seen there, one after it reaches this connection (R1 M-2).
pub(super) async fn hello_session(shared: &Arc<Shared>, peer: &mut Peer, end: &mpsc::UnboundedSender<SignOutReason>, token: Option<String>) -> Result<HelloWho, ProtocolError> {
    if let (Some(token), None) = (token, &peer.app) {
        let verifier = super::sessions::verifier(&token);
        let (store, client) = (shared.store().clone(), device(peer));
        let found = storage_task(move || {
            store.read(move |c| {
                let now = aurelius_storage::now_ms();
                let Some(session) = aurelius_storage::user_sessions::by_verifier(c, &verifier, now)?.filter(|s| s.client_id == client) else { return Ok(None) };
                Ok(aurelius_storage::users::get(c, &session.user_id)?.map(|user| (session, user)))
            })
        })
        .await?;
        let mine = match found.and_then(|(session, user)| user.view().map(|v| (session, user, v))) {
            Some((session, row, view)) => {
                shared.sessions.set(peer.conn, Some((Some(session.id.clone()), view.id, client)), end);
                let (store, id) = (shared.store().clone(), session.id.clone());
                let still = storage_task(move || {
                    store.write(move |tx| {
                        let now = aurelius_storage::now_ms();
                        let Some(valid) = aurelius_storage::user_sessions::valid(tx, &id, now)? else { return Ok(None) };
                        if now - valid.last_seen_ms < TOUCH_EVERY_MS {
                            return Ok(Some(valid.last_seen_ms));
                        }
                        aurelius_storage::user_sessions::touch(tx, &id, now)?;
                        Ok(Some(now))
                    })
                })
                .await;
                match still {
                    Ok(seen) => seen.map(|seen| (session, row, view, seen)),
                    Err(err) => {
                        register(shared, peer, end);
                        return Err(err);
                    }
                }
            }
            None => None,
        };
        match mine {
            Some((session, row, view, seen_at_ms)) => {
                peer.user = Some(view.id);
                let session = PeerSession { id: Some(session.id), must_change_password: row.must_change_password, expires_at_ms: session.expires_at_ms, seen_at_ms, checked_at_ms: now_ms() };
                peer.session = Some(session);
            }
            None => {
                peer.user = None;
                peer.session = None;
                shared.sessions.forget(peer.conn);
            }
        }
    }
    let user = peer.user.filter(|_| peer.app.is_none());
    let who = whoami_of(shared, peer).await?;
    let claude_login = match user {
        Some(user) => Some(claude_login(shared, user).await?),
        None => None,
    };
    Ok(HelloWho { who, claude_login })
}

/// Before a request: a session past its end (90 days, or 30 without use) is
/// over. Every `Shared::session_check_ms` (a minute) it is checked against the
/// database, so a revoke, a new password or a disable that did not reach this
/// connection through the hub still signs it out (R1 M-2); it is marked used
/// at most once an hour (S6).
pub(super) async fn keep_session(shared: &Arc<Shared>, peer: &mut Peer) -> Result<(), SignOutReason> {
    let Some(session) = peer.session.as_ref() else { return Ok(()) };
    let Some(id) = session.id.clone() else { return Ok(()) };
    let now = now_ms();
    if now >= session.expires_at_ms || now - session.seen_at_ms >= aurelius_storage::user_sessions::IDLE_MS {
        peer.user = None;
        peer.session = None;
        return Err(SignOutReason::Expired);
    }
    if now - session.checked_at_ms < shared.session_check_ms {
        return Ok(());
    }
    let touch = now - session.seen_at_ms >= TOUCH_EVERY_MS;
    let store = shared.store().clone();
    let still = storage_task(move || {
        store.write(move |tx| {
            let valid = aurelius_storage::user_sessions::valid(tx, &id, now)?;
            if valid.is_some() && touch {
                aurelius_storage::user_sessions::touch(tx, &id, now)?;
            }
            Ok(valid.is_some())
        })
    })
    .await;
    match still {
        Ok(true) => {
            if let Some(session) = peer.session.as_mut() {
                session.checked_at_ms = now;
                if touch {
                    session.seen_at_ms = now;
                }
            }
            Ok(())
        }
        Ok(false) => {
            peer.user = None;
            peer.session = None;
            Err(SignOutReason::Revoked)
        }
        // The database could not say: ask again at the next request.
        Err(_) => Ok(()),
    }
}

/// A board member's own Claude login as the client sees it.
async fn claude_login(shared: &Arc<Shared>, user_id: UserId) -> Result<ClaudeLoginView, ProtocolError> {
    let store = shared.store().clone();
    let (row, background) = storage_task(move || {
        store.read(|c| Ok((aurelius_storage::claude_logins::get(c, &user_id.to_string())?, aurelius_storage::company::background_login_user(c)?)))
    })
    .await?;
    let status = match row.as_ref().map(|r| r.status) {
        Some(aurelius_storage::claude_logins::LoginStatus::Linked) => ClaudeLoginStatus::Linked,
        Some(aurelius_storage::claude_logins::LoginStatus::Expired) => ClaudeLoginStatus::Expired,
        _ => ClaudeLoginStatus::Missing,
    };
    Ok(ClaudeLoginView {
        user_id,
        status,
        email: row.as_ref().and_then(|r| r.email.clone()),
        plan: row.as_ref().and_then(|r| r.plan.clone()),
        linked_at_ms: row.as_ref().and_then(|r| r.linked_at_ms),
        checked_at_ms: row.as_ref().map(|r| r.checked_at_ms),
        link_id: shared.claude_links.running(user_id),
        background: background == Some(user_id),
    })
}

/// `session.list`, `claude_login.status`, `claude_login.input`.
pub(super) async fn session_read(shared: &Arc<Shared>, peer: &Peer, method: Method) -> Result<ResponseBody, ProtocolError> {
    if peer.app.is_some() {
        return Err(ProtocolError::new(ErrorCode::Forbidden, aurelius_protocol::app::NOT_FOR_APPS));
    }
    let Some(me) = peer.user else { return Err(unauthenticated()) };
    match method {
        Method::SessionList { user_id } => {
            let user_id = user_id.unwrap_or(me);
            let current = peer.session.as_ref().and_then(|s| s.id.clone());
            let store = shared.store().clone();
            let rows = storage_task(move || store.read(|c| aurelius_storage::user_sessions::list_for_user(c, &user_id.to_string(), aurelius_storage::now_ms()))).await?;
            let sessions = rows
                .into_iter()
                .filter_map(|r| {
                    Some(SessionView {
                        id: aurelius_domain::SessionId::parse(&r.id).ok()?,
                        user_id,
                        current: current.as_deref() == Some(r.id.as_str()),
                        device_name: r.device_name,
                        client_id: r.client_id,
                        created_at_ms: r.created_at_ms,
                        last_seen_ms: r.last_seen_ms,
                        expires_at_ms: r.expires_at_ms,
                    })
                })
                .collect();
            Ok(ResponseBody::Sessions(SessionsSnapshot { user_id, sessions }))
        }
        Method::ClaudeLoginStatus => claude_login(shared, me).await.map(ResponseBody::ClaudeLogin),
        // The code Claude showed: checked and written into the sign-in's PTY once (C5, C7);
        // no receipt, no request hash, no event, never logged.
        Method::ClaudeLoginInput(params) => {
            shared.claude_links.input(me, peer.conn, &params.link_id, &params.text)?;
            claude_login(shared, me).await.map(ResponseBody::ClaudeLogin)
        }
        _ => Err(internal("not a session read")),
    }
}

/// The receipt of an account write.
fn account_receipt(key: CommandId, committed: &aurelius_application::user::AccountCommitted) -> Result<ResponseBody, ProtocolError> {
    let result: aurelius_protocol::HostResult = serde_json::from_value(committed.result.clone()).map_err(internal)?;
    Ok(ResponseBody::Receipt(Receipt { idempotency_key: key, committed_seq: committed.committed_seq, replayed: committed.replayed, result: WriteResult::Host(result) }))
}

fn done() -> serde_json::Value {
    serde_json::to_value(aurelius_protocol::HostResult::Done).unwrap_or_default()
}

/// `session.revoke`, `user.set_password`, `user.disable` and the Claude login's
/// writes. Not through the host pipeline: their receipt and event are made by
/// `account_write` from parameters without a password. The open connections of
/// every session that ends close at once.
pub(super) async fn session_write(shared: &Arc<Shared>, peer: &mut Peer, ctx: CommandContext, key: CommandId, attempts: &mut Attempts, write: HostWrite) -> Result<ResponseBody, ProtocolError> {
    if peer.app.is_some() {
        return Err(ProtocolError::new(ErrorCode::Forbidden, aurelius_protocol::app::NOT_FOR_APPS));
    }
    let Some(me) = peer.user else { return Err(unauthenticated()) };
    let current = peer.session.as_ref().and_then(|s| s.id.clone());
    let named = |s: &Arc<Shared>, id: UserId| {
        let store = s.store().clone();
        async move { storage_task(move || store.read(move |c| aurelius_storage::users::get(c, &id.to_string()))).await }
    };
    match write {
        HostWrite::SessionRevoke { session_id } => {
            let sid = session_id.to_string();
            if current.as_deref() == Some(sid.as_str()) {
                return Err(ProtocolError::new(ErrorCode::Validation, "This is the session of this device: sign out instead."));
            }
            let store = shared.store().clone();
            let id = sid.clone();
            let row = storage_task(move || store.read(move |c| aurelius_storage::user_sessions::get(c, &id))).await?;
            let Some(row) = row else { return Err(ProtocolError::new(ErrorCode::NotFound, "This session does not exist.")) };
            let owner = UserId::parse(&row.user_id).map_err(internal)?;
            let owner_name = named(shared, owner).await?.map(|u| u.username).unwrap_or_default();
            let account = user::AccountWrite {
                method: "session.revoke",
                request_hash: aurelius_application::request_hash_of(&("session.revoke", &sid)),
                kind: aurelius_domain::EventKind::HostWrite,
                user_id: owner,
                extra: serde_json::json!({ "session_id": sid, "subject_user_id": owner, "subject": owner_name, "device_name": row.device_name }),
            };
            let (s, at) = (shared.clone(), sid.clone());
            let committed = app_task(move || {
                s.service.account_write(&ctx, key, account, Box::new(move |tx| {
                    let revoked = aurelius_storage::user_sessions::revoke(tx, &row.user_id, &at, aurelius_storage::now_ms())?;
                    Ok(if revoked { Ok(done()) } else { Err(ProtocolError::new(ErrorCode::NotFound, "This session has already ended.")) })
                }))
            })
            .await?;
            shared.sessions.end_session(&sid, SignOutReason::Revoked, None);
            shared.publish(committed.notice.iter().cloned().collect::<Vec<_>>());
            account_receipt(key, &committed)
        }
        HostWrite::UserSetPassword(params) => {
            let aurelius_protocol::SetPasswordParams { user_id, old_password, new_password } = params;
            let target = user_id.unwrap_or(me);
            let own = target == me;
            aurelius_domain::user::check_password(&new_password).map_err(|e| ProtocolError::new(ErrorCode::WeakPassword, e.to_string()))?;
            let Some(row) = named(shared, target).await?.filter(|r| !r.disabled()) else {
                return Err(ProtocolError::new(ErrorCode::NotFound, "This board member does not exist or is disabled."));
            };
            if own {
                // The old password, checked like a sign-in (and counted on this connection).
                attempts.take(Instant::now())?;
                let Some(old) = old_password else { return Err(ProtocolError::new(ErrorCode::WrongPassword, "Enter your current password.")) };
                let store = shared.store().clone();
                let id = row.id.clone();
                let phc = storage_task(move || store.read(move |c| aurelius_storage::users::password_hash(c, &id))).await?.unwrap_or_else(|| DECOY.clone());
                // Longer than any password (A5): wrong, after the same work.
                let right = argon2_task(move || {
                    if old.len() > PASSWORD_MAX {
                        verify("", &phc);
                        return false;
                    }
                    verify(&old, &phc)
                });
                if !right.await? {
                    tokio::time::sleep(WRONG_DELAY).await;
                    return Err(ProtocolError::new(ErrorCode::WrongPassword, "Your current password is wrong."));
                }
            }
            let phc = argon2_task(move || hash(&new_password)).await??;
            let account = user::AccountWrite {
                method: "user.set_password",
                request_hash: aurelius_application::request_hash_of(&("user.set_password", target, key)),
                kind: aurelius_domain::EventKind::HostWrite,
                user_id: target,
                extra: serde_json::json!({ "subject_user_id": target, "subject": row.username, "temporary": !own }),
            };
            let (s, keep, id) = (shared.clone(), current.clone().filter(|_| own), row.id.clone());
            let committed = app_task(move || {
                s.service.account_write(&ctx, key, account, Box::new(move |tx| {
                    aurelius_storage::users::set_password(tx, &id, &phc, !own, keep.as_deref(), aurelius_storage::now_ms())?;
                    Ok(Ok(done()))
                }))
            })
            .await?;
            if !committed.replayed {
                let keep = current.as_deref().filter(|_| own);
                shared.sessions.end_user(target, keep, SignOutReason::PasswordChanged, own.then_some(peer.conn));
                if own {
                    if let Some(session) = peer.session.as_mut() {
                        session.must_change_password = false;
                    }
                }
            }
            shared.publish(committed.notice.iter().cloned().collect::<Vec<_>>());
            account_receipt(key, &committed)
        }
        HostWrite::UserDisable { user_id } => {
            let Some(row) = named(shared, user_id).await? else { return Err(ProtocolError::new(ErrorCode::NotFound, "This board member does not exist.")) };
            let account = user::AccountWrite {
                method: "user.disable",
                request_hash: aurelius_application::request_hash_of(&("user.disable", user_id)),
                kind: aurelius_domain::EventKind::UserDisabled,
                user_id,
                extra: serde_json::json!({ "disabled_user_id": user_id, "subject": row.username }),
            };
            let (s, id) = (shared.clone(), row.id.clone());
            let committed = app_task(move || {
                s.service.account_write(&ctx, key, account, Box::new(move |tx| {
                    let target = aurelius_storage::users::get(tx, &id)?;
                    if target.as_ref().is_some_and(|t| t.disabled()) {
                        return Ok(Err(ProtocolError::new(ErrorCode::Validation, "This board member is already disabled.")));
                    }
                    if aurelius_storage::users::count_active(tx)? <= 1 {
                        return Ok(Err(ProtocolError::new(ErrorCode::LastBoardMember, "The last active board member cannot be disabled.")));
                    }
                    aurelius_storage::users::disable(tx, &id, aurelius_storage::now_ms())?;
                    // Their Claude login goes too (its folder with the Claude login runtime, F2).
                    aurelius_storage::claude_logins::delete(tx, &id)?;
                    let view = aurelius_storage::users::get(tx, &id)?.and_then(|u| u.view());
                    Ok(Ok(serde_json::to_value(view.map(aurelius_protocol::HostResult::User)).unwrap_or_default()))
                }))
            })
            .await?;
            if !committed.replayed {
                // Their Claude login: signed out and its folder removed (MU3-09 AC4, C15).
                if let Err(e) = crate::claude_connect::unlink(shared, user_id).await {
                    tracing::warn!(user = %user_id, error = %e.message, "the disabled board member's Claude login could not be removed");
                }
                shared.sessions.end_user(user_id, None, SignOutReason::Disabled, (user_id == me).then_some(peer.conn));
                if user_id == me {
                    peer.user = None;
                    peer.session = None;
                }
            }
            shared.publish(committed.notice.iter().cloned().collect::<Vec<_>>());
            account_receipt(key, &committed)
        }
        HostWrite::ClaudeLoginCancel { link_id } => {
            let account = user::AccountWrite {
                method: "claude_login.cancel",
                request_hash: aurelius_application::request_hash_of(&("claude_login.cancel", me, &link_id)),
                kind: aurelius_domain::EventKind::HostWrite,
                user_id: me,
                extra: serde_json::json!({ "subject_user_id": me }),
            };
            let (s, ended) = (shared.clone(), shared.claude_links.cancel(me, &link_id));
            let view = claude_login(shared, me).await?;
            let result = serde_json::to_value(aurelius_protocol::HostResult::ClaudeLogin(view)).map_err(internal)?;
            let committed = app_task(move || s.service.account_write(&ctx, key, account, Box::new(move |_| Ok(Ok(result))))).await?;
            if !ended && !committed.replayed {
                tracing::debug!(user = %me, "claude_login.cancel: nothing was running");
            }
            shared.publish(committed.notice.iter().cloned().collect::<Vec<_>>());
            account_receipt(key, &committed)
        }
        // Only the board member's own login; a disabled one's goes with `user.disable` (C1, C15).
        HostWrite::ClaudeLoginUnlink => {
            let account = user::AccountWrite {
                method: "claude_login.unlink",
                request_hash: aurelius_application::request_hash_of(&("claude_login.unlink", me, key)),
                kind: aurelius_domain::EventKind::HostWrite,
                user_id: me,
                extra: serde_json::json!({ "subject_user_id": me }),
            };
            let s = shared.clone();
            let committed = app_task(move || s.service.account_write(&ctx, key, account, Box::new(move |_| Ok(Ok(serde_json::Value::Null))))).await?;
            if !committed.replayed {
                crate::claude_connect::unlink(shared, me).await?;
            }
            let view = claude_login(shared, me).await?;
            shared.publish(committed.notice.iter().cloned().collect::<Vec<_>>());
            let result = WriteResult::Host(aurelius_protocol::HostResult::ClaudeLogin(view));
            Ok(ResponseBody::Receipt(Receipt { idempotency_key: key, committed_seq: committed.committed_seq, replayed: committed.replayed, result }))
        }
        HostWrite::ClaudeLoginLink => Err(internal("claude_login.link is answered by the connection")),
        _ => Err(internal("not an account write")),
    }
}

/// `claude_login.link` (C1–C16): only the session's own login, the prompt only to
/// `out` (this connection). Answers the link's id; a replay answers the same id
/// and starts nothing.
pub(super) async fn claude_link(shared: &Arc<Shared>, peer: &Peer, ctx: CommandContext, key: CommandId, out: tokio::sync::mpsc::Sender<aurelius_protocol::ServerMessage>) -> Result<ResponseBody, ProtocolError> {
    if peer.app.is_some() {
        return Err(ProtocolError::new(ErrorCode::Forbidden, aurelius_protocol::app::NOT_FOR_APPS));
    }
    let Some(me) = peer.user else { return Err(unauthenticated()) };
    let s = shared.clone();
    let replay = app_task(move || s.service.host_receipt(key, &aurelius_application::request_hash_of(&("claude_login.link", me, key)))).await?;
    if let Some((committed_seq, stored)) = replay {
        let result: aurelius_protocol::HostResult = serde_json::from_value(stored).map_err(internal)?;
        return Ok(ResponseBody::Receipt(Receipt { idempotency_key: key, committed_seq, replayed: true, result: WriteResult::Host(result) }));
    }
    let session = peer.session.as_ref().and_then(|s| s.id.clone());
    let link_id = crate::claude_connect::link(shared, me, peer.conn, session, out).await?;
    let account = user::AccountWrite {
        method: "claude_login.link",
        request_hash: aurelius_application::request_hash_of(&("claude_login.link", me, key)),
        kind: aurelius_domain::EventKind::HostWrite,
        user_id: me,
        extra: serde_json::json!({ "subject_user_id": me }),
    };
    let result = serde_json::to_value(aurelius_protocol::HostResult::ClaudeLoginLink { link_id }).map_err(internal)?;
    let s = shared.clone();
    let committed = app_task(move || s.service.account_write(&ctx, key, account, Box::new(move |_| Ok(Ok(result))))).await?;
    shared.publish(committed.notice.iter().cloned().collect::<Vec<_>>());
    account_receipt(key, &committed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hash_is_argon2id_with_the_target_parameters_and_a_fresh_salt() {
        let one = hash("correct horse battery").unwrap();
        let two = hash("correct horse battery").unwrap();
        assert!(one.starts_with("$argon2id$v=19$m=65536,t=3,p=1$"), "{one}");
        assert_ne!(one, two, "a salt per hash");
        assert!(verify("correct horse battery", &one));
        assert!(!verify("correct horse batterY", &one));
        assert!(!verify("anything", "not a phc string"));
    }

    #[test]
    fn the_decoy_fallback_is_a_real_hash_with_the_same_parameters() {
        let parsed = PasswordHash::new(DECOY_FALLBACK).expect("a valid PHC string");
        assert_eq!(parsed.to_string(), DECOY_FALLBACK);
        assert!(DECOY_FALLBACK.starts_with("$argon2id$v=19$m=65536,t=3,p=1$"));
        assert!(!verify("not a board member's password", DECOY_FALLBACK));
        assert!(!weaker(DECOY_FALLBACK), "the same parameters as a hash made now");
        warm_up();
        assert!(DECOY.starts_with("$argon2id$v=19$m=65536,t=3,p=1$"), "{}", *DECOY);
        // A hash with the OWASP minimum of before is weaker: replaced at the next sign-in.
        assert!(weaker("$argon2id$v=19$m=19456,t=2,p=1$bm90IGEgYm9hcmQgbWVtYmVy$cPg9lHz6gdbTJp8wOAKX6tvl1t5dzk5L1yEbyT7mGmI"));
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
        let mut creates = Attempts::default();
        let limits = super::super::limits::Limits::default();
        for _ in 0..ATTEMPTS {
            count_create(&limits, &mut creates).unwrap();
        }
        assert_eq!(count_create(&limits, &mut creates).unwrap_err().code, ErrorCode::TooManyAttempts);
    }

    /// A2 (SEC1 m6): a hash that waits too long for a place is refused, not queued without end.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_hash_waiting_too_long_for_a_place_is_refused() {
        let held = HASHERS.acquire_many(HASHING as u32).await.unwrap();
        let refused = argon2_within(Duration::from_millis(50), || ()).await.unwrap_err();
        assert_eq!(refused.code, ErrorCode::TooManyAttempts);
        drop(held);
        assert!(argon2_within(Duration::from_secs(5), || ()).await.is_ok());
    }

    #[test]
    fn a_device_name_has_no_control_or_bidi_characters() {
        assert_eq!(clean_device_name("Alex's\u{202E}enohp\u{202C} laptop\n\t\u{0007}"), "Alex'senohp laptop");
        assert_eq!(clean_device_name("\u{2066}\u{200F}\r\n "), "Unknown device");
        assert_eq!(clean_device_name(&"x".repeat(200)).len(), 80);
        assert_eq!(clean_device_name("  Laptop  "), "Laptop");
    }
}
