//! The mailbox of an employee (R1-R3, `aurelius_storage::inbox`): everything that
//! wakes an employee is an item, and its worker (one per employee, here a pass of
//! the dispatcher over the queue) takes the oldest item it may start, merges what
//! waits in that thread and only then makes the run, with the configuration and
//! login snapshots of that moment, and a leased job as its execution record.
//!
//! * One run at a time per employee (R2), held by the job's lease; a run that waits
//!   for an approval keeps its thread but not the slot (assumption 3). A quick
//!   answer is the bounded exception (assumption 2) and never comes here.
//! * Merged: the due items of one thread, in one conversation, on one login. The
//!   newest is what the run answers; the others are in its context by then.
//! * Parked (R3): a run that ended on a question parks its thread until a board
//!   member's item, of kind `resume`, is taken or the task goes on otherwise.
//! * Hops (R4): an item has a hop; a run has the highest of what it merged, counted from
//!   the last message of a person, and what it writes or wakes is one higher.
//! * The run keeps its frozen `login_user` (from the item) and quota parking per
//!   login: an item whose login waits for a limit waits with it, not on another.

use crate::run::events::{Ev, emit};
use crate::run::{connections, login};
use crate::{CHAT_RUN_KIND, CommandService, storage_error};
use aurelius_domain::{ActorKind, AppError, ConversationId, EmployeeId, EventNotice, MessageId, RunId, TaskId, TicketId, UserId};
use aurelius_storage::conversations::{self as conv, GroupRun};
use aurelius_storage::inbox::{self as store, Item, NewItem};
use aurelius_storage::queries::{self, JobRow};
use aurelius_storage::{StorageError, Tx, events};
use rusqlite::{Connection, OptionalExtension};
use std::collections::HashSet;

/// The key of a ticket's thread: everything an employee does on it.
pub(crate) fn ticket_thread(ticket: TicketId) -> String {
    format!("ticket:{ticket}")
}

/// The key of a channel's thread (Team chat).
pub(crate) fn channel_thread(channel: ConversationId) -> String {
    format!("channel:{channel}")
}

/// The key of a conversation of its own (a chat with an employee).
pub(crate) fn chat_thread(conversation: ConversationId) -> String {
    format!("chat:{conversation}")
}

/// The employee whose mailbox a run in `conversation` goes through: none for the
/// project assistant (assumption 4) and for a quick answer (assumption 2).
fn mailbox_of(conn: &Connection, conversation: ConversationId) -> Result<Option<EmployeeId>, StorageError> {
    let employee: Option<Option<String>> = conn
        .query_row("SELECT employee_id FROM conversations WHERE id = ?1 AND quick_answer = 0", [conversation.to_string()], |r| r.get(0))
        .optional()?;
    Ok(employee.flatten().and_then(|e| EmployeeId::parse(&e).ok()))
}

/// The thread of a run in `conversation`, with its ticket when it has one.
fn thread_of(tx: &Tx<'_>, conversation: ConversationId, group: Option<&GroupRun>) -> Result<(String, Option<TicketId>), StorageError> {
    if let Some(group) = group {
        return Ok(match aurelius_storage::discussions::ticket_of_conversation(tx, group.conversation_id)? {
            Some(ticket) => (ticket_thread(ticket), Some(ticket)),
            None => (channel_thread(group.conversation_id), None),
        });
    }
    if let Some(key) = store::conversation_thread(tx, conversation)? {
        let ticket = key.strip_prefix("ticket:").and_then(|t| TicketId::parse(t).ok());
        return Ok((key, ticket));
    }
    if let Some(task) = aurelius_storage::tasks::task_for_conversation(tx, conversation)?
        && let Some(ticket) = crate::task::ticket_line::ticket_of(tx, task)?
    {
        return Ok((ticket_thread(ticket), Some(ticket)));
    }
    Ok((chat_thread(conversation), None))
}

/// The hop in the queuing metadata of a run no group chat marking carries: a
/// delegated task's, a colleague's `message_task` (R4). 0 otherwise.
fn hop_of(meta: &serde_json::Value) -> u32 {
    meta.get("hop").and_then(|h| h.as_u64()).unwrap_or(0) as u32
}

/// Who an item comes from: a board member, a colleague, or Aurelius itself.
fn source_of(tx: &Tx<'_>, group: Option<&GroupRun>, meta: &serde_json::Value, reply_to: MessageId) -> Result<&'static str, StorageError> {
    if let Some(group) = group {
        return Ok(if group.hop == 0 { "board" } else { "employee" });
    }
    if meta.get("origin").and_then(|o| o.as_str()) == Some("delegated") || hop_of(meta) > 0 {
        return Ok("employee");
    }
    // A heartbeat names the login of who turned it on, and is nobody's message.
    // A scheduled message has a hop from its first run, so it is caught just above.
    if meta.get("login_user").is_some() {
        return Ok("system");
    }
    if meta.get("user_id").is_some_and(|u| !u.is_null()) || meta.get("discussion").is_some() {
        return Ok("board");
    }
    let human: Option<bool> = tx
        .query_row(
            "SELECT role = 'user' AND json_extract(meta_json, '$.update') IS NULL FROM messages WHERE id = ?1",
            [reply_to.to_string()],
            |r| r.get(0),
        )
        .optional()?;
    Ok(if human == Some(true) { "board" } else { "system" })
}

/// Queue a run for `reply_to` through the employee's mailbox, or `None` when the
/// conversation has none. The run id is the one the run gets when this item is the
/// last of those a run takes. `with_row`: the run row is made now, for what must refer
/// to it (a heartbeat's wake, a variant's trial); such an item is never merged.
pub(crate) fn enqueue(
    tx: &Tx<'_>,
    ctx_meta: &serde_json::Value,
    correlation: &str,
    conversation: ConversationId,
    reply_to: MessageId,
    group: Option<GroupRun>,
    with_row: bool,
) -> Result<Option<(RunId, Vec<EventNotice>)>, StorageError> {
    let Some(employee) = mailbox_of(tx, conversation)? else { return Ok(None) };
    let run = RunId::new();
    let login = login::for_run(tx, conversation, reply_to, group.as_ref().and_then(|g| g.login_user), ctx_meta)?;
    let (thread, ticket) = thread_of(tx, conversation, group.as_ref())?;
    let source = source_of(tx, group.as_ref(), ctx_meta, reply_to)?;
    let parked = store::is_parked(tx, &employee.to_string(), &thread)?;
    let kind = match () {
        _ if parked && source == "board" => "resume",
        _ if ctx_meta.get("scheduled").is_some() => "scheduled",
        _ if ctx_meta.get("login_user").is_some() => "heartbeat",
        _ if ctx_meta.get("task_id").is_some() && ctx_meta.get("origin").is_none() => "task",
        _ => "message",
    };
    let hop = group.as_ref().map_or(hop_of(ctx_meta), |g| g.hop);
    let (event, notice) = emit(tx, Ev {
        aggregate_type: "run",
        aggregate_id: run.to_string(),
        event_type: "run.queued",
        actor: ActorKind::User,
        actor_id: None,
        correlation: correlation.into(),
        payload: None,
        metadata: ctx_meta.clone(),
        dedupe: format!("run:{run}:queued"),
    })?;
    let mut payload = crate::run::queue::job_payload(run, conversation, MessageId::new(), reply_to, group);
    if with_row {
        payload["with_row"] = true.into();
        make_run(tx, run, conversation, login)?;
    }
    store::insert(tx, NewItem {
        employee,
        thread_key: &thread,
        ticket,
        conversation,
        kind,
        source,
        hop,
        payload: &payload,
        login_user: login.map(|u| u.to_string()).as_deref(),
        task: None,
        source_event: event,
        run,
    })?;
    // Waiting behind an open question it is not the answer to: the task stays as it
    // is, and hears of this item when the worker takes it.
    let mut notices = if parked && kind != "resume" { Vec::new() } else { crate::task::task_runs::link_run(tx, run, conversation)? };
    notices.push(notice);
    #[cfg(test)]
    if !tests_support::real_mailbox(employee) {
        // Unit tests that are not about the mailbox find a run and a ready job at once;
        // the daemon's tests and `inbox_tests` go through the real worker.
        let item = store::due(tx, i64::MAX / 2)?.into_iter().find(|i| i.run == run);
        if let Some(item) = item {
            promote(tx, &[&item], None)?;
        }
    }
    Ok(Some((run, notices)))
}

/// A run row with its configuration snapshot of now.
fn make_run(tx: &Tx<'_>, run: RunId, conversation: ConversationId, login: Option<UserId>) -> Result<(), StorageError> {
    let mut resolved = connections::resolve(tx, conversation, aurelius_domain::connection::Purpose::Employee)?;
    resolved.login_user = login;
    conv::insert_run(tx, run, conversation, &resolved)?;
    Ok(())
}

/// Whether the run row exists (not yet, for what still waits in a mailbox).
fn run_exists(conn: &Connection, run: RunId) -> Result<bool, StorageError> {
    Ok(conn.query_row("SELECT EXISTS(SELECT 1 FROM runs WHERE id = ?1)", [run.to_string()], |r| r.get(0))?)
}

/// The task a run or its item counts for, whichever exists now.
pub(crate) fn link_task(tx: &Tx<'_>, run: RunId, task: TaskId) -> Result<(), StorageError> {
    tx.execute("UPDATE runs SET task_id = ?2 WHERE id = ?1", [run.to_string(), task.to_string()])?;
    store::link_task(tx, run, task)
}

/// Take a queued item out of the mailbox: stopping or deleting what has not
/// started. Its run row is made here (cancelled by the caller), so whoever asks
/// what became of the run finds it. False when no item waits for `run`.
pub(crate) fn cancel_queued(tx: &Tx<'_>, run: RunId) -> Result<bool, StorageError> {
    let Some(item) = store::cancel(tx, run)? else { return Ok(false) };
    if !run_exists(tx, run)? {
        make_run(tx, run, item.conversation, item.login_user.as_deref().and_then(|u| UserId::parse(u).ok()))?;
    }
    if let Some(task) = item.task {
        tx.execute("UPDATE runs SET task_id = ?2 WHERE id = ?1", [run.to_string(), task.to_string()])?;
    }
    Ok(true)
}

/// A run ended on a question (`ends_on_question`; `MessageView::awaiting_user` says
/// the same to clients): its thread waits for the answer.
pub(crate) fn park_on_question(tx: &Tx<'_>, run: RunId, task: TaskId) -> Result<(), StorageError> {
    let Some((employee, thread)) = store::job_thread(tx, run)? else { return Ok(()) };
    store::park(tx, &employee, &thread, task, run)
}

fn thread_parent(item: &Item) -> Option<String> {
    item.payload["group"]["thread_parent"].as_str().map(str::to_owned)
}

/// The hop of a run that takes `items` (oldest first): the highest of them, counting
/// from the last one a person or Aurelius wrote (hop 0), because that resets it (R4).
fn merged_hop(items: &[&Item]) -> u32 {
    let from = items.iter().rposition(|i| i.hop == 0).unwrap_or(0);
    items[from..].iter().map(|i| i.hop).max().unwrap_or(0)
}

/// What the worker of `head`'s employee takes together with it: the due items of its
/// thread in the same conversation, on the same login and of the same urgency, and
/// never one whose run row exists already. A reply in a channel's thread stays
/// apart from the channel: its answer goes there.
fn merged<'a>(head: &'a Item, due: &'a [Item]) -> Vec<&'a Item> {
    if head.payload.get("with_row").is_some() {
        return vec![head];
    }
    due.iter()
        .filter(|i| {
            i.employee == head.employee
                && i.thread_key == head.thread_key
                && i.conversation == head.conversation
                && i.login_user == head.login_user
                && i.priority == head.priority
                && i.payload.get("with_row").is_none()
                && thread_parent(i) == thread_parent(head)
        })
        .collect()
}

/// Make the run for `items` (oldest first; the last is what it answers), and a job
/// for it: `lease` leased under that owner, else ready for the next claim.
fn promote(tx: &Tx<'_>, items: &[&Item], lease: Option<(&str, i64, i64)>) -> Result<Option<(JobRow, Vec<EventNotice>)>, StorageError> {
    let Some(last) = items.last() else { return Ok(None) };
    let (run, conversation) = (last.run, last.conversation);
    if !run_exists(tx, run)? {
        make_run(tx, run, conversation, last.login_user.as_deref().and_then(|u| UserId::parse(u).ok()))?;
    }
    // The items it replaces had an id of their own: nobody refers to those any more.
    let mut notices = Vec::new();
    match items.iter().rev().find_map(|i| i.task) {
        Some(task) => {
            tx.execute("UPDATE runs SET task_id = ?2 WHERE id = ?1", [run.to_string(), task.to_string()])?;
        }
        // Held back behind a question: only now it counts for the task.
        None => notices.extend(crate::task::task_runs::link_run(tx, run, conversation)?),
    }
    let mut payload = last.payload.clone();
    payload["employee_id"] = last.employee.clone().into();
    payload["thread_key"] = last.thread_key.clone().into();
    let hop = merged_hop(items);
    payload["hop"] = hop.into();
    if payload.get("group").is_some_and(|g| !g.is_null()) {
        payload["group"]["hop"] = hop.into();
    }
    events::insert_job(tx, &run.to_string(), CHAT_RUN_KIND, last.source_event, &format!("run:{run}"), &payload)?;
    let ids: Vec<String> = items.iter().map(|i| i.id.clone()).collect();
    store::claim(tx, &ids, run)?;
    store::unpark(tx, &last.employee, &last.thread_key)?;
    let job = match lease {
        Some((owner, now, lease_ms)) => queries::lease_new_job(tx, &run.to_string(), owner, now, lease_ms)?,
        None => queries::job_row(tx, &run.to_string())?,
    };
    Ok(Some((job, notices)))
}

/// One pass of the workers: the first item that may start becomes a run. `limit`:
/// the machine's limit on runs, as for jobs; 0 starts nothing.
fn claim_next(tx: &Tx<'_>, owner: &str, now: i64, lease_ms: i64, limit: u32) -> Result<Option<(JobRow, Vec<EventNotice>)>, StorageError> {
    if limit == 0 || queries::running_runs(tx)? >= limit {
        return Ok(None);
    }
    let due = store::due(tx, now)?;
    let mut busy: HashSet<&str> = HashSet::new();
    for item in &due {
        if busy.contains(item.employee.as_str()) {
            continue;
        }
        if !store::slot_free(tx, &item.employee)? {
            busy.insert(&item.employee);
            continue;
        }
        if store::thread_held(tx, &item.employee, &item.thread_key, item.login_user.as_deref())? {
            continue;
        }
        // A resume goes first; what is parked behind it waits for that.
        match store::resume_conversation(tx, &item.employee, &item.thread_key)? {
            Some(conversation) if conversation != item.conversation.to_string() => continue,
            Some(_) => {}
            None if store::is_parked(tx, &item.employee, &item.thread_key)? => continue,
            None => {}
        }
        // Its login is at a usage limit: it waits with what is parked there (MU3-11).
        if let Some(login) = &item.login_user
            && let Some(until) = quota_until(tx, item.conversation, login, now)?
        {
            store::defer(tx, &item.id, until, "quota")?;
            continue;
        }
        return promote(tx, &merged(item, &due), Some((owner, now, lease_ms)));
    }
    Ok(None)
}

fn quota_until(tx: &Tx<'_>, conversation: ConversationId, login: &str, now: i64) -> Result<Option<i64>, StorageError> {
    let resolved = connections::resolve(tx, conversation, aurelius_domain::connection::Purpose::Employee)?;
    store::quota_until(tx, resolved.runtime_kind(), login, now)
}

impl CommandService {
    /// What the dispatcher starts next: the first item a worker may take, else the first due job.
    pub fn claim_next_work(&self, owner: &str, lease_ms: i64, limit: u32) -> Result<Option<(JobRow, Vec<EventNotice>)>, AppError> {
        let owner = owner.to_owned();
        self.store
            .write(move |tx| {
                let now = aurelius_storage::now_ms();
                if let Some(work) = claim_next(tx, &owner, now, lease_ms, limit)? {
                    return Ok(Some(work));
                }
                Ok(queries::claim_due_job(tx, &owner, now, lease_ms, limit)?.map(|job| (job, Vec::new())))
            })
            .map_err(storage_error)
    }

    /// Handoffs held for a sender that has caught up are delivered now (R5).
    pub fn release_handoffs(&self) -> Result<Vec<EventNotice>, AppError> {
        // Read first: this runs at every pass of the dispatcher, and there is almost never one.
        if self.store.read(store::releasable_handoffs).map_err(storage_error)?.is_empty() {
            return Ok(Vec::new());
        }
        let handoffs = self.store.clone();
        self.store.write(move |tx| crate::conversation::discussion::release_handoffs(tx, &handoffs)).map_err(storage_error)
    }

    /// Note why runs that may start are held back by the machine.
    pub(crate) fn note_held_items(tx: &Tx<'_>, reason: &str, now: i64) -> Result<Vec<EventNotice>, StorageError> {
        let mut notices = Vec::new();
        for (run, conversation) in store::mark_held(tx, reason, now)? {
            let version = conv::next_version(tx, "run", &run)?;
            let (_, n) = emit(tx, Ev {
                aggregate_type: "run",
                aggregate_id: run.clone(),
                event_type: "run.waiting",
                actor: ActorKind::System,
                actor_id: None,
                correlation: run.clone(),
                payload: None,
                metadata: serde_json::json!({ "reason": reason, "conversation_id": conversation }),
                dedupe: format!("run:{run}:run.waiting:{version}"),
            })?;
            notices.push(n);
        }
        Ok(notices)
    }
}

#[cfg(test)]
pub(crate) mod tests_support {
    use aurelius_domain::EmployeeId;
    use std::collections::HashSet;
    use std::sync::Mutex;

    static REAL: Mutex<Option<HashSet<EmployeeId>>> = Mutex::new(None);

    /// This employee's mailbox works as in production in this test.
    pub(crate) fn use_real_mailbox(employee: EmployeeId) {
        REAL.lock().unwrap().get_or_insert_with(HashSet::new).insert(employee);
    }

    pub(super) fn real_mailbox(employee: EmployeeId) -> bool {
        REAL.lock().unwrap().as_ref().is_some_and(|s| s.contains(&employee))
    }
}
