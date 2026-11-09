//! `schedule_message` (R7): an employee sends itself or a colleague a message later,
//! in the thread the run works in. The login is that of the planning run, frozen
//! (assumption 5): a later run never borrows anyone else's, and a run without a login
//! plans a message that waits for one, as that run did.
//!
//! * A ticket's discussion or a channel: it goes out there as a post, and wakes the
//!   addressee in its own thread conversation like a handoff (`discussion::wake`).
//! * The employee's own chat: only for itself, as an update there.
//! * A run elsewhere (a task without a ticket) has no thread to come back to: refused.
//! * Hops: the message counts as what the run writes (R4), so a self-reminder that
//!   schedules itself again stops at the thread's limit until a board member speaks.
//! * Past due while the thread closed or the addressee was paused: it does not go out,
//!   and says why on the Scheduled page and in the thread.

use crate::conversation::discussion::{self, Discussion};
use crate::run::events::{Ev, emit};
use crate::run::queue::queue_run_marked;
use crate::task::task_threads::{self, Speaker};
use crate::{CommandContext, CommandService, Committed, storage_error};
use aurelius_domain::employee::CancelScheduledMessage;
use aurelius_domain::group_chat as group;
use aurelius_domain::scheduler::scheduled_message::{self as rules, DELIVERED, NOT_DELIVERED};
use aurelius_domain::views::{SystemLine, SystemLineKind};
use aurelius_domain::{ActorKind, AppError, CommandEnvelope, CommandResult, ConversationId, ErrorCode, EventNotice, RunId, ScheduledMessageId};
use aurelius_storage::scheduled::{self as store, Due, NewScheduled};
use aurelius_storage::{StorageError, Store, Tx};
use rusqlite::OptionalExtension;

/// The update kind of a scheduled message in a chat (`MessageView::update`).
pub(crate) const SCHEDULED_KIND: &str = "scheduled";

/// The event a change of a scheduled message goes out on: the kind every client knows
/// for "a schedule changed" (`EventKind` has no catch-all, so no new value is added).
const CHANGED: &str = "recurring_task.changed";

type Reply = (bool, String, Vec<EventNotice>);

fn changed(tx: &Tx<'_>, id: ScheduledMessageId, step: &str, correlation: &str, actor: ActorKind) -> Result<EventNotice, StorageError> {
    let (_, notice) = emit(tx, Ev {
        aggregate_type: "scheduled_message",
        aggregate_id: id.to_string(),
        event_type: CHANGED,
        actor,
        actor_id: None,
        correlation: correlation.into(),
        payload: None,
        metadata: serde_json::json!({ "scheduled_message": step }),
        dedupe: format!("scheduled:{id}:{step}"),
    })?;
    Ok(notice)
}

/// Whether a conversation is an employee's own chat: not a task's and not a quick answer's.
fn is_own_chat(conn: &rusqlite::Connection, conversation: ConversationId) -> Result<bool, StorageError> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM conversations c WHERE c.id = ?1 AND c.kind = 'employee' AND c.quick_answer = 0 AND c.thread_key IS NULL
               AND NOT EXISTS(SELECT 1 FROM tasks t WHERE t.conversation_id = c.id)",
            [conversation.to_string()],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

/// `schedule_message`: `to` a colleague's name, none or empty for the employee itself.
pub(crate) fn schedule_message(tx: &Tx<'_>, run: RunId, delay: &str, content: &str, to: Option<&str>) -> Result<Reply, StorageError> {
    let refuse = |why: String| Ok((false, why, vec![]));
    let Some(me) = crate::task::task_runs::run_owner(tx, run)?.and_then(|o| o.employee) else {
        return refuse("Only an employee can schedule a message.".into());
    };
    let content = content.trim();
    if content.is_empty() {
        return refuse("Write the message to schedule.".into());
    }
    if content.chars().count() > rules::CONTENT_MAX {
        return refuse(format!("Too long to schedule (at most {} characters).", rules::CONTENT_MAX));
    }
    let delay_ms = match rules::parse_delay(delay) {
        Ok(ms) => ms,
        Err(why) => return refuse(why),
    };
    if store::of_run(tx, run)? >= rules::MAX_PER_RUN {
        return refuse(format!("You already scheduled {} messages in this run.", rules::MAX_PER_RUN));
    }
    if store::pending_of(tx, me)? >= rules::MAX_PENDING_PER_EMPLOYEE {
        return refuse(format!("You already have {} scheduled messages waiting.", rules::MAX_PENDING_PER_EMPLOYEE));
    }
    let wanted = to.map(|t| t.trim().trim_start_matches('@').trim()).filter(|t| !t.is_empty());
    let members = discussion::team(tx)?;
    let (target, target_name) = match wanted {
        None => (me, discussion::employee_name(tx, me)?),
        Some(wanted) => {
            let Some((id, name, active)) = members.iter().find(|(id, n, _)| n.trim().eq_ignore_ascii_case(wanted) || id.to_string() == wanted) else {
                return refuse(format!("No colleague called {wanted} in the company."));
            };
            if !*active {
                return refuse(format!("{name} is paused and will not answer: schedule it for someone else, or for yourself."));
            }
            (*id, name.clone())
        }
    };
    let thread = discussion::run_handoff_thread(tx, run)?;
    let conversation = aurelius_storage::conversations::run_conversation(tx, run)?;
    let own_chat = match (&thread, conversation) {
        (Some(_), _) | (None, None) => None,
        (None, Some(conversation)) => is_own_chat(tx, conversation)?.then_some(conversation),
    };
    if thread.is_none() && own_chat.is_none() {
        return refuse("A scheduled message comes back in a ticket's discussion, a channel of the Team chat or your own chat, and this run is in none of those.".into());
    }
    if own_chat.is_some() && target != me {
        return refuse("In a chat you can only schedule a message for yourself. For a colleague, do it from a ticket or a channel.".into());
    }
    if let Some(d) = &thread
        && let Some(why) = d.read_only()
    {
        return refuse(why);
    }
    let hop = match &thread {
        Some(d) => discussion::next_hop(tx, run, d)?,
        None => aurelius_storage::inbox::run_hop(tx, run)? + 1,
    };
    let limit = group::hop_limit(thread.as_ref().is_none_or(|d| d.ticket().is_some()));
    if hop > limit {
        return refuse(format!("Limit reached: this thread already went {limit} steps without a board member. Wait for a board member before you schedule more."));
    }
    let (thread_key, ticket, place, parent) = match (&thread, own_chat) {
        (Some(d), _) => (discussion::thread_key(d), d.ticket().map(|t| t.id), d.label(), discussion::run_thread(tx, run, d)?),
        (None, Some(chat)) => (crate::run::inbox::chat_thread(chat), None, format!("{}'s own chat", discussion::employee_name(tx, me)?), None),
        (None, None) => unreachable!("refused above"),
    };
    let login = crate::run::login::of_run(tx, run)?;
    let due_at_ms = aurelius_storage::now_ms() + delay_ms;
    let id = store::insert(tx, NewScheduled {
        from: me,
        to: target,
        content,
        discussion: thread.as_ref().map(|d| d.id),
        chat: own_chat,
        thread_key: &thread_key,
        ticket,
        thread_parent: parent,
        place: &place,
        hop,
        login_user: login,
        planned_by_run: run,
        due_at_ms,
    })?;
    let notice = changed(tx, id, "scheduled", &run.to_string(), ActorKind::Employee)?;
    let who = if target == me { "You get".to_owned() } else { format!("{target_name} gets") };
    Ok((true, format!("Scheduled. {who} this in {} in {place}. It runs on this run's login.", delay.trim()), vec![notice]))
}

/// Why `m` cannot go out now, if that is so.
fn stopped(tx: &Tx<'_>, d: Option<&Discussion>, m: &Due, to_name: &str) -> Result<Option<String>, StorageError> {
    if let Some(why) = d.and_then(|d| d.read_only()) {
        return Ok(Some(why));
    }
    let active = discussion::team(tx)?.iter().any(|(id, _, active)| *id == m.to && *active);
    Ok((!active).then(|| format!("{to_name} is paused.")))
}

fn deliver_one(tx: &Tx<'_>, st: &Store, m: Due) -> Result<Vec<EventNotice>, StorageError> {
    let from = discussion::employee_name(tx, m.from)?;
    let to = discussion::employee_name(tx, m.to)?;
    let d = match m.discussion {
        Some(id) => discussion::load(tx, id)?,
        None => None,
    };
    if m.discussion.is_some() && d.is_none() {
        store::settle(tx, m.id, NOT_DELIVERED, None, Some("Its thread is gone."))?;
        return Ok(vec![changed(tx, m.id, "settled", &m.id.to_string(), ActorKind::System)?]);
    }
    let mut notices = Vec::new();
    if let Some(why) = stopped(tx, d.as_ref(), &m, &to)? {
        store::settle(tx, m.id, NOT_DELIVERED, None, Some(&why))?;
        if let Some(d) = &d {
            let text = format!("The scheduled message from {from} to {to} was not delivered: {why}");
            notices.push(discussion::system_line_in(tx, d.id, m.thread_parent, &SystemLine::new(SystemLineKind::Limit), Some(&text))?);
        }
        notices.push(changed(tx, m.id, "settled", &m.id.to_string(), ActorKind::System)?);
        return Ok(notices);
    }
    let run = match (&d, m.chat) {
        (Some(d), _) => discussion::deliver_scheduled(tx, st, d, &m, &from, &to)?,
        (None, Some(chat)) => {
            let text = if m.from == m.to { format!("You scheduled this for yourself earlier:\n\n{}", m.content) } else { m.content.clone() };
            let (update, posted) = task_threads::post_note(tx, st, chat, Speaker::Update { kind: SCHEDULED_KIND, from: &from }, &text)?;
            let meta = serde_json::json!({ "scheduled": m.id, "hop": m.hop, "login_user": m.login_user });
            let (run, queued) = queue_run_marked(tx, &meta, &m.id.to_string(), chat, update, None)?;
            Some((run, std::iter::once(posted).chain(queued).collect()))
        }
        (None, None) => None,
    };
    match run {
        Some((run, queued)) => {
            notices.extend(queued);
            store::settle(tx, m.id, DELIVERED, Some(run), None)?;
        }
        None => store::settle(tx, m.id, NOT_DELIVERED, None, Some(&format!("{to} has no chat to receive it in.")))?,
    }
    notices.push(changed(tx, m.id, "settled", &m.id.to_string(), ActorKind::System)?);
    Ok(notices)
}

impl CommandService {
    /// What came due goes out now (a pass of the dispatcher). Read first: there is
    /// almost never anything.
    pub fn deliver_scheduled_messages(&self) -> Result<Vec<EventNotice>, AppError> {
        let now = aurelius_storage::now_ms();
        if self.store.read(|c| store::due(c, now)).map_err(storage_error)?.is_empty() {
            return Ok(Vec::new());
        }
        let st = self.store.clone();
        self.store
            .write(move |tx| {
                let mut notices = Vec::new();
                for m in store::due(tx, now)? {
                    notices.extend(deliver_one(tx, &st, m)?);
                }
                Ok(notices)
            })
            .map_err(storage_error)
    }

    /// A board member takes a scheduled message back; someone else's private chat stays theirs.
    pub(crate) fn cancel_scheduled_message(&self, ctx: &CommandContext, envelope: &CommandEnvelope, hash: String, cmd: &CancelScheduledMessage) -> Result<Committed, AppError> {
        let (id, reader, command) = (cmd.message_id, ctx.user_id, envelope.command_id);
        self.commit(command, hash, move |tx| {
            if !store::cancel(tx, id, reader)? {
                return Ok(Err(AppError::new(ErrorCode::NotFound, "This scheduled message is no longer waiting.")));
            }
            let notice = changed(tx, id, "cancelled", &command.to_string(), ActorKind::User)?;
            Ok(Ok((CommandResult::ScheduledMessageCancelled { message_id: id }, vec![notice])))
        })
    }

    /// `scratchpad`: the employee's note is replaced.
    pub(crate) fn write_scratchpad(&self, run: RunId, content: &str) -> Result<Reply, AppError> {
        let text = content.trim().to_owned();
        self.store
            .write(move |tx| {
                let Some(me) = crate::task::task_runs::run_owner(tx, run)?.and_then(|o| o.employee) else {
                    return Ok((false, "Only an employee has a scratchpad.".to_owned(), vec![]));
                };
                if text.chars().count() > aurelius_domain::employee::SCRATCHPAD_MAX {
                    return Ok((false, format!("Too long for the scratchpad (at most {} characters). Keep what you need in every thread; the rest belongs in remember or a ticket.", aurelius_domain::employee::SCRATCHPAD_MAX), vec![]));
                }
                aurelius_storage::scratchpads::set(tx, me, &text, run)?;
                Ok((true, if text.is_empty() { "Your scratchpad is empty now.".to_owned() } else { "Saved. It is in your prompt in every thread from your next run.".to_owned() }, vec![]))
            })
            .map_err(storage_error)
    }
}
