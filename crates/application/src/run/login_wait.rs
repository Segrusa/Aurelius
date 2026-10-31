//! A run that cannot start for want of its board member's Claude login (none
//! linked, expired after a 401, or its board member disabled) waits with the
//! reason 'login' (MU3-12): never on another login. Its task waits with it,
//! the ticket shows `needs: login`, its discussion says why once per wait, and
//! the board member of that login gets a note in their own Inbox (AC2). Once
//! the login is (re)linked, everything that waited for it goes on by itself
//! (B11): the parked jobs are due again, the tasks run and the discussion says
//! "Resumed.".

use crate::run::events::{Ev, emit};
use crate::task::tasks::task_event;
use crate::{CommandService, storage_error};
use aurelius_domain::{
    ActorKind, AppError, ConversationId, EventKind, EventNotice, RunId, TaskId, TicketId, UserId,
    task::{TaskStatus, WAITING_LOGIN},
    views::{SystemLine, SystemLineKind},
};
use aurelius_storage::{StorageError, Store, Tx, claude_logins, conversations as conv, queries, tasks as task_store, tickets as ticket_store};
use serde_json::json;

/// The line in a discussion when what waited goes on again.
pub const RESUMED: &str = "Resumed.";

/// Why `user`'s login cannot run anything: `disabled`, `expired` or `missing`.
pub(crate) fn why_not(conn: &rusqlite::Connection, user: Option<UserId>) -> Result<&'static str, StorageError> {
    let Some(user) = user else { return Ok("missing") };
    if aurelius_storage::users::get(conn, &user.to_string())?.is_some_and(|u| u.disabled()) {
        return Ok("disabled");
    }
    Ok(match claude_logins::get(conn, &user.to_string())?.map(|l| l.status) {
        Some(claude_logins::LoginStatus::Expired) => "expired",
        _ => "missing",
    })
}

/// The board member's name as lines and notes say it.
pub(crate) fn name_of(conn: &rusqlite::Connection, user: Option<UserId>) -> Result<String, StorageError> {
    Ok(match user {
        Some(user) => aurelius_storage::users::get(conn, &user.to_string())?.map(|u| u.username).unwrap_or_else(|| "A board member".into()),
        None => "A board member".into(),
    })
}

/// What waits, in words (ui-ontwerp §5): the discussion's line.
pub(crate) fn wait_text(name: &str, why: &str) -> String {
    match why {
        "expired" => format!("Waiting: {name}'s Claude login has expired. It goes on once {name} reconnects it."),
        "disabled" => format!("Waiting: {name} is disabled, so their Claude login is gone. A board member can run it on their own login."),
        _ => format!("Waiting: {name}'s Claude login is missing. It goes on once {name} connects it."),
    }
}

/// A system line in the ticket's discussion (`Limit`: something holds it up; the text says what).
fn ticket_note(tx: &Tx<'_>, ticket: TicketId, text: &str) -> Result<Vec<EventNotice>, StorageError> {
    crate::conversation::discussion::ticket_line(tx, ticket, SystemLine::new(SystemLineKind::Limit), text)
}

/// The ticket now waits for `login` (call before its task is set to wait):
/// once per wait, the event that makes clients show `needs: login` and gives
/// the board member of that login a note in their Inbox, and the discussion's line.
pub(crate) fn ticket_waits(tx: &Tx<'_>, ticket: TicketId, login: Option<UserId>) -> Result<Vec<EventNotice>, StorageError> {
    if ticket_store::waits_for_login(tx, ticket)? {
        return Ok(vec![]);
    }
    let Some(state) = ticket_store::state(tx, ticket)? else { return Ok(vec![]) };
    let why = why_not(tx, login)?;
    let name = name_of(tx, login)?;
    let title: String = tx.query_row("SELECT title FROM tickets WHERE id = ?1", [ticket.to_string()], |r| r.get(0))?;
    let mut notices = vec![crate::task::ticket_line::ticket_event(
        tx,
        ticket,
        state.project_id,
        EventKind::TicketUpdated,
        None,
        json!({ "changed": [], "needs": "login", "login": why, "login_user": login, "for_user": login, "subject": title }),
    )?];
    notices.extend(ticket_note(tx, ticket, &wait_text(&name, why))?);
    Ok(notices)
}

/// The run `run` waits for its login (`run.waiting` 'login' was recorded): its
/// task waits with it and the ticket says so; a chat without a task says it in
/// the conversation (and in the Team chat that woke it).
pub(crate) fn on_run_login_wait(tx: &Tx<'_>, st: &Store, run: RunId) -> Result<Vec<EventNotice>, StorageError> {
    let login = crate::run::login::of_run(tx, run)?;
    let task: Option<String> = tx.query_row("SELECT task_id FROM runs WHERE id = ?1", [run.to_string()], |r| r.get(0)).ok().flatten();
    let task = task.and_then(|t| TaskId::parse(&t).ok());
    let mut notices = Vec::new();
    match task.and_then(|t| task_store::load_task(tx, t).ok().flatten().map(|row| (t, row))) {
        Some((task, row)) => {
            if !matches!(row.status, TaskStatus::Running | TaskStatus::Queued) {
                return Ok(notices);
            }
            let ticket = crate::task::ticket_line::ticket_of(tx, task)?;
            if let Some(ticket) = ticket {
                notices.extend(ticket_waits(tx, ticket, login)?);
            }
            notices.extend(crate::task::task_line::set_status(tx, task, TaskStatus::Waiting, Some(WAITING_LOGIN))?.into_vec());
            // A task without a ticket tells its board member itself.
            let mut meta = json!({ "reason": WAITING_LOGIN, "login": why_not(tx, login)?, "login_user": login });
            if ticket.is_none() {
                meta["needs"] = json!("login");
                meta["for_user"] = json!(login);
            }
            notices.push(task_event(tx, task, row.project_id, "task.waiting", meta)?);
        }
        None => {
            let Some(conversation) = conv::run_conversation(tx, run)? else { return Ok(notices) };
            notices.extend(chat_waits(tx, st, run, conversation, login)?);
        }
    }
    Ok(notices)
}

/// A chat run without a task waits: the conversation (and the Team chat or
/// discussion that woke it) says so, and its board member gets a note.
fn chat_waits(tx: &Tx<'_>, _st: &Store, run: RunId, conversation: ConversationId, login: Option<UserId>) -> Result<Vec<EventNotice>, StorageError> {
    let why = why_not(tx, login)?;
    let name = name_of(tx, login)?;
    let text = match why {
        "expired" => format!("{name}'s Claude login has expired: this answer waits until {name} reconnects it."),
        "disabled" => format!("{name} is disabled: this answer waits, as their Claude login is gone."),
        _ => format!("Connect your Claude login to get an answer. ({name}'s Claude login is missing; this answer waits for it.)"),
    };
    let line = SystemLine::new(SystemLineKind::Limit);
    let mut notices = vec![crate::conversation::discussion::system_line(tx, conversation, &line, Some(&text))?];
    if let Some(group) = conv::group_run(tx, run)? {
        notices.push(crate::conversation::discussion::system_line(tx, group.conversation_id, &line, Some(&text))?);
    }
    let (_, note) = emit(tx, Ev {
        aggregate_type: "run",
        aggregate_id: run.to_string(),
        event_type: "run.waiting",
        actor: ActorKind::System,
        actor_id: None,
        correlation: run.to_string(),
        payload: None,
        metadata: json!({ "reason": WAITING_LOGIN, "needs": "login", "login": why, "for_user": login, "conversation_id": conversation, "subject": "An answer waits for your Claude login" }),
        dedupe: format!("run:{run}:login-note:{}", aurelius_domain::EventId::new()),
    })?;
    notices.push(note);
    Ok(notices)
}

/// `user`'s login is linked (again): every job parked for it is due now, the
/// tasks that waited go on, and each ticket's discussion says "Resumed." (B11).
pub(crate) fn resume_login(tx: &Tx<'_>, user: UserId) -> Result<Vec<EventNotice>, StorageError> {
    let me = user.to_string();
    let mut notices = Vec::new();
    let mut tickets: Vec<TicketId> = Vec::new();
    // The dispatcher's job of a ticket that waited (before its run rows).
    for job in queries::wake_parked_for_login(tx, WAITING_LOGIN, Some(&me), aurelius_storage::now_ms())? {
        if let Ok(run) = RunId::parse(&job) {
            if conv::run_status(tx, run)?.as_deref() == Some("waiting") {
                conv::set_run_status(tx, run, "created", None)?;
            }
            continue;
        }
        let ticket: Option<String> = tx.query_row("SELECT json_extract(payload_json, '$.ticket_id') FROM jobs WHERE id = ?1 AND kind = 'ticket.triage'", [&job], |r| r.get(0)).ok().flatten();
        tickets.extend(ticket.and_then(|t| TicketId::parse(&t).ok()));
    }
    for task in queries::tasks_waiting_for_login(tx, &me)? {
        let Some(row) = task_store::load_task(tx, task)? else { continue };
        notices.extend(crate::task::task_line::set_status(tx, task, TaskStatus::Queued, None)?);
        notices.extend(crate::task::task_line::set_status(tx, task, TaskStatus::Running, None)?);
        notices.push(task_event(tx, task, row.project_id, "task.resumed", json!({ "reason": WAITING_LOGIN }))?);
        tickets.extend(crate::task::ticket_line::ticket_of(tx, task)?);
    }
    tickets.sort_by_key(|t| t.to_string());
    tickets.dedup();
    for ticket in tickets {
        if ticket_store::waits_for_login(tx, ticket)? {
            continue;
        }
        let Some(state) = ticket_store::state(tx, ticket)? else { continue };
        notices.push(crate::task::ticket_line::ticket_event(tx, ticket, state.project_id, EventKind::TicketUpdated, None, json!({ "changed": [], "needs": null, "reason": "login_linked" }))?);
        notices.extend(ticket_note(tx, ticket, RESUMED)?);
    }
    Ok(notices)
}

/// `ticket.use_my_login`: the ticket runs on `user`'s login from now on. Its
/// runs that have not started yet (queued, or parked for a login or a limit)
/// move to it with the ticket, and so does its dispatcher's waiting job; a run
/// that is running keeps its own. When `user`'s login is linked, what waited goes on.
pub(crate) fn take_over(tx: &Tx<'_>, ticket: TicketId, user: UserId) -> Result<Vec<EventNotice>, StorageError> {
    let me = user.to_string();
    tx.execute(
        "UPDATE runs SET login_user = ?2, resolved_config_json = json_set(resolved_config_json, '$.login_user', ?2)
         WHERE status IN ('created', 'waiting') AND task_id IN (SELECT id FROM tasks WHERE ticket_id = ?1)
           AND EXISTS (SELECT 1 FROM jobs j WHERE j.id = runs.id AND j.state = 'ready')",
        rusqlite::params![ticket.to_string(), me],
    )?;
    tx.execute(
        "UPDATE jobs SET payload_json = json_set(payload_json, '$.login_user', ?2)
         WHERE kind = 'ticket.triage' AND state = 'ready' AND json_extract(payload_json, '$.ticket_id') = ?1",
        rusqlite::params![ticket.to_string(), me],
    )?;
    // What waited for the old login's limit is due now: the new login has its own (R2 m7).
    tx.execute(
        "UPDATE jobs SET not_before_ms = ?3 WHERE state = 'ready' AND last_error_code = 'quota'
           AND id IN (SELECT r.id FROM runs r JOIN tasks t ON t.id = r.task_id WHERE t.ticket_id = ?1 AND r.login_user = ?2 AND r.status IN ('created', 'waiting'))",
        rusqlite::params![ticket.to_string(), me, aurelius_storage::now_ms()],
    )?;
    if claude_logins::is_linked(tx, &me)? { resume_login(tx, user) } else { Ok(vec![]) }
}

/// The company's background login changed from `old` (another board member's,
/// none, or `old` unlinked): its jobs without a source that waited for `old`'s
/// login are due now and find their login again when they run (R2 m6, B6).
pub(crate) fn background_login_left(tx: &Tx<'_>, old: UserId) -> Result<usize, StorageError> {
    Ok(tx.execute(
        "UPDATE jobs SET not_before_ms = ?3, last_error_code = NULL
         WHERE kind = ?1 AND state = 'ready' AND last_error_code = 'login' AND json_extract(payload_json, '$.login_user') = ?2",
        rusqlite::params![crate::memory::CONSOLIDATE_JOB_KIND, old.to_string(), aurelius_storage::now_ms()],
    )?)
}

/// Whether a task of `ticket` waits for a usage limit.
fn waits_for_quota(conn: &rusqlite::Connection, ticket: TicketId) -> Result<bool, StorageError> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM tasks WHERE ticket_id = ?1 AND status = 'waiting' AND waiting_reason = 'quota')",
        [ticket.to_string()],
        |r| r.get(0),
    )?)
}

/// `at` (epoch ms) as the server's clock shows it: "16:00", with the day when it is not today.
pub(crate) fn clock(at: i64, now: i64) -> String {
    let local = |ms: i64| ms + aurelius_host::utc_offset_minutes_at(ms) * 60_000;
    let (at_local, now_local) = (local(at), local(now));
    let day = |ms: i64| ms.div_euclid(86_400_000);
    let minutes = at_local.rem_euclid(86_400_000) / 60_000;
    let time = format!("{:02}:{:02}", minutes / 60, minutes % 60);
    if day(at_local) == day(now_local) {
        return time;
    }
    // Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
    let z = day(at_local) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    format!("{d} {} {time}", MONTHS[(m - 1) as usize])
}

/// A task of `ticket` waits for the limit of `run`'s login until `until_ms`
/// (call before it is set to wait): once per wait, the discussion says whose.
pub(crate) fn quota_paused(tx: &Tx<'_>, ticket: TicketId, run: RunId, until_ms: i64) -> Result<Vec<EventNotice>, StorageError> {
    if waits_for_quota(tx, ticket)? {
        return Ok(vec![]);
    }
    let name = name_of(tx, crate::run::login::of_run(tx, run)?)?;
    ticket_note(tx, ticket, &format!("Paused: {name}'s Claude limit is reached until {}.", clock(until_ms, aurelius_storage::now_ms())))
}

/// A task of `ticket` that waited for a limit goes again: "Resumed." once nothing of it waits for one.
pub(crate) fn quota_resumed(tx: &Tx<'_>, ticket: TicketId) -> Result<Vec<EventNotice>, StorageError> {
    if waits_for_quota(tx, ticket)? {
        return Ok(vec![]);
    }
    ticket_note(tx, ticket, RESUMED)
}

/// A run that waited for its login goes again: so does its task (a link that
/// came in while it was being claimed).
pub(crate) fn on_run_login_resumed(tx: &Tx<'_>, run: RunId) -> Result<Vec<EventNotice>, StorageError> {
    let task: Option<String> = tx.query_row("SELECT task_id FROM runs WHERE id = ?1", [run.to_string()], |r| r.get(0)).ok().flatten();
    let Some(task) = task.and_then(|t| TaskId::parse(&t).ok()) else { return Ok(vec![]) };
    let Some(row) = task_store::load_task(tx, task)? else { return Ok(vec![]) };
    if row.status != TaskStatus::Waiting || task_store::waiting_cause(tx, task)?.as_deref() != Some(WAITING_LOGIN) {
        return Ok(vec![]);
    }
    let mut notices = crate::task::task_line::set_status(tx, task, TaskStatus::Queued, None)?.into_vec();
    notices.extend(crate::task::task_line::set_status(tx, task, TaskStatus::Running, None)?);
    notices.push(task_event(tx, task, row.project_id, "task.resumed", json!({ "reason": WAITING_LOGIN }))?);
    Ok(notices)
}

/// The `claude_login.updated` event: whose login changed and how, never its
/// e-mail or plan (B12); each client asks `claude_login.status` for its own.
fn login_event(tx: &Tx<'_>, user: UserId, status: &str) -> Result<EventNotice, StorageError> {
    let (_, notice) = emit(tx, Ev {
        aggregate_type: "claude_login",
        aggregate_id: user.to_string(),
        event_type: EventKind::ClaudeLoginUpdated.as_str(),
        actor: ActorKind::System,
        actor_id: None,
        correlation: user.to_string(),
        payload: None,
        metadata: json!({ "user_id": user, "status": status }),
        dedupe: format!("claude-login:{user}:{status}:{}", aurelius_domain::EventId::new()),
    })?;
    Ok(notice)
}

impl CommandService {
    /// A link worked (`auth status` says signed in): the login is linked with
    /// what Claude says of it, and everything that waited for it goes on (B11).
    pub fn claude_login_linked(&self, user: UserId, email: Option<String>, plan: Option<String>) -> Result<Vec<EventNotice>, AppError> {
        self.store
            .write(move |tx| {
                claude_logins::upsert(tx, &user.to_string(), claude_logins::LoginStatus::Linked, email.as_deref(), plan.as_deref(), aurelius_storage::now_ms())?;
                let mut notices = vec![login_event(tx, user, "linked")?];
                notices.extend(resume_login(tx, user)?);
                Ok(notices)
            })
            .map_err(storage_error)
    }

    /// Claude refused the login in a run (a 401): it is expired until relinked.
    pub fn claude_login_expired(&self, user: UserId) -> Result<Vec<EventNotice>, AppError> {
        self.store
            .write(move |tx| {
                let Some(row) = claude_logins::get(tx, &user.to_string())? else { return Ok(vec![]) };
                if row.status == claude_logins::LoginStatus::Expired {
                    return Ok(vec![]);
                }
                claude_logins::upsert(tx, &user.to_string(), claude_logins::LoginStatus::Expired, row.email.as_deref(), row.plan.as_deref(), aurelius_storage::now_ms())?;
                Ok(vec![login_event(tx, user, "expired")?])
            })
            .map_err(storage_error)
    }

    /// The login was unlinked (or its board member disabled): forgotten.
    pub fn claude_login_removed(&self, user: UserId) -> Result<Vec<EventNotice>, AppError> {
        self.store
            .write(move |tx| {
                claude_logins::delete(tx, &user.to_string())?;
                // Background jobs no longer run on it (B6); those that waited for it find none and end.
                if aurelius_storage::company::background_login_user(tx)? == Some(user) {
                    aurelius_storage::company::set_background_login_user(tx, None)?;
                    background_login_left(tx, user)?;
                }
                Ok(vec![login_event(tx, user, "missing")?])
            })
            .map_err(storage_error)
    }

    /// The dispatcher of `ticket` hit its login's usage limit: its run ends
    /// without counting as a try and its job (`job`, leased under `epoch`) waits
    /// until `until_ms`, as the runs on that login do (R2 m8, MU3-11 AC2).
    pub fn triage_waits_for_quota(&self, ticket: TicketId, run: RunId, job: String, epoch: i64, until_ms: i64) -> Result<Vec<EventNotice>, AppError> {
        self.store
            .write(move |tx| {
                tx.execute(
                    "UPDATE runs SET status = 'cancelled', error_code = 'quota', ended_at_ms = ?2 WHERE id = ?1",
                    rusqlite::params![run.to_string(), aurelius_storage::now_ms()],
                )?;
                let notices = quota_paused(tx, ticket, run, until_ms)?;
                if !queries::reschedule_job(tx, &job, epoch, until_ms, "quota", false)? {
                    return Ok(vec![]);
                }
                Ok(notices)
            })
            .map_err(storage_error)
    }

    /// The dispatcher of `ticket` could not run for want of its login: its run
    /// ends without counting as a try, its job (`job`, leased under `epoch`)
    /// waits for the login, and the ticket says so (`needs: login`).
    pub fn triage_waits_for_login(&self, ticket: TicketId, run: RunId, job: String, epoch: i64) -> Result<Vec<EventNotice>, AppError> {
        self.store
            .write(move |tx| {
                tx.execute(
                    "UPDATE runs SET status = 'cancelled', error_code = 'login', ended_at_ms = ?2 WHERE id = ?1",
                    rusqlite::params![run.to_string(), aurelius_storage::now_ms()],
                )?;
                let login = aurelius_storage::tickets::login_user(tx, ticket)?;
                let notices = ticket_waits(tx, ticket, login)?;
                if !queries::park_job_for_login(tx, &job, epoch, login.map(|u| u.to_string()).as_deref())? {
                    return Ok(vec![]);
                }
                Ok(notices)
            })
            .map_err(storage_error)
    }
}
