//! A board member's lasting chat with an employee: one per board member per employee
//! (kind `employee`), through the mailbox as thread `chat:<conversation>` and always on
//! that board member's Claude login.
//!
//! * Talking: the employee talks here and, once the work is concrete, makes ordinary
//!   tickets with `create_ticket`. A chat is no ticket's parent and waits on none.
//! * Coming back: the server records which chat a ticket was made in. Each arrival in
//!   Review counts in the transaction that moves the card (`chat_tickets`); a pass of
//!   the dispatcher turns what a chat is owed into one message and one mailbox item, and
//!   in that transaction the arrivals count as told, so none is delivered twice.
//! * `tell_responsible`: a message from a ticket in the responsible's chat, unread there.
//!   It starts no run and completes nothing.
//!
//! Nothing here notifies (P-82): a chat's unread mark is all a board member gets.

use crate::run::events::{Ev, emit};
use crate::run::queue::queue_run;
use crate::{CommandContext, CommandService, Committed, domain_error, storage_error};
use aurelius_domain::conversation::{MESSAGE_MAX, TICKET_NOTE_KIND, TICKET_UPDATE_KIND};
use aurelius_domain::employee::{LifecycleStatus, OpenEmployeeChat};
use aurelius_domain::{ActorKind, AppError, CommandEnvelope, CommandResult, ConversationId, DomainError, EventNotice, MessageId, RunId, TicketId, UserId};
use aurelius_storage::employee_chats::{self as chats, WaitingUpdate};
use aurelius_storage::{StorageError, Store, Tx, blobs, conversations as conv, employees};

/// Why `tell_responsible` needs a ticket.
pub const NO_TICKET_TO_TELL: &str = "tell_responsible works in a run on a ticket: its responsible board member gets the message. There is no ticket here.";
/// Why `tell_responsible` found nobody.
pub const NO_RESPONSIBLE: &str = "This ticket has no responsible board member. Post in its discussion with post_to_discussion instead.";

/// The Markdown link to a ticket, for the app to open.
pub(crate) fn ticket_link(st: &Store, ticket: TicketId, number: u32) -> String {
    match aurelius_host::host_id(st.root()) {
        Ok(host) => format!("[#{number}](aurelius://{host}/ticket/{ticket})"),
        Err(_) => format!("#{number}"),
    }
}

/// Why `user` cannot write in `conversation` now, if so: another board member's chat,
/// an employee that is not at work, or a conversation `chat.send` does not take.
pub(crate) fn write_refusal(tx: &Tx<'_>, conversation: ConversationId, user: Option<UserId>) -> Result<Option<AppError>, StorageError> {
    let not_found = || domain_error(DomainError::ConversationNotFound(conversation));
    let Some(chat) = chats::of_conversation(tx, conversation)? else {
        if conv::is_older_employee_chat(tx, conversation)? {
            return Ok(Some(domain_error(DomainError::OutOfRange {
                field: "conversation_id".into(),
                detail: "This is an older conversation; write in the employee's chat".into(),
            })));
        }
        return Ok((!conv::conversation_exists(tx, conversation)?).then(not_found));
    };
    if user != Some(chat.user) {
        return Ok(Some(not_found()));
    }
    let Some(employee) = employees::load_employee(tx, chat.employee)? else { return Ok(Some(not_found())) };
    Ok(match employee.status {
        LifecycleStatus::Archived => Some(domain_error(DomainError::EmployeeArchived)),
        LifecycleStatus::Paused => Some(domain_error(DomainError::EmployeePaused)),
        LifecycleStatus::Active => {
            aurelius_storage::inbox::follow_main_chat(tx, conversation, chat.employee)?;
            None
        }
    })
}

/// What the employee is told about this chat, in place of its own workspace's text.
pub(crate) fn guidance(username: &str) -> String {
    format!(
        "This is your lasting one-to-one chat with the board member @{username}; only they read it. Talk as a colleague: plain and short, and ask what you need to know in the text of your reply. AskUserQuestion interrupts them with a card: use it only for a real decision you cannot go on without.\n\
         Once the work is concrete, make an ordinary ticket with create_ticket, in the project where it belongs (ask if that is unclear), and put the link it gives you in your reply. If only management may make it, say so and name who can. This chat is no ticket's parent and waits on no ticket: say what you made and stop there; the work goes on on the ticket.\n\
         A [Ticket update from Aurelius] is not {username} speaking: tickets from this chat reached Review. Write {username} one short update, with each ticket's link and what is ready to look at, and start no new work on it. Colleagues do not read this chat: hand_off and post_to_group_chat are for threads, not for here. Remember what you learn in your own memory (remember with scope employee)."
    )
}

fn update_text(st: &Store, username: &str, tickets: &[&WaitingUpdate]) -> String {
    let lines: Vec<String> = tickets.iter().map(|u| format!("- {} {}", ticket_link(st, u.ticket, u.number), u.title)).collect();
    format!("Tickets you made in this chat reached Review:\n{}\nWrite {username} one short update now. read_ticket shows the report.", lines.join("\n"))
}

/// Everything chats are owed, one message per chat: what waits is said once, together.
fn deliver_updates(tx: &Tx<'_>, st: &Store) -> Result<Vec<EventNotice>, StorageError> {
    let waiting = chats::waiting_updates(tx)?;
    let mut notices = Vec::new();
    for group in waiting.chunk_by(|a, b| a.conversation == b.conversation) {
        let Some(first) = group.first() else { continue };
        let Some(chat) = chats::of_conversation(tx, first.conversation)? else { continue };
        let tickets: Vec<&WaitingUpdate> = group.iter().collect();
        let text = update_text(st, &chat.username, &tickets);
        let blob = st.blobs().put(text.as_bytes(), "text/markdown; charset=utf-8")?;
        blobs::register(tx, &blob)?;
        let message = MessageId::new();
        let correlation = message.to_string();
        let (event, n) = emit(tx, Ev {
            aggregate_type: "conversation",
            aggregate_id: first.conversation.to_string(),
            event_type: "message.user_committed",
            actor: ActorKind::System,
            actor_id: None,
            correlation: correlation.clone(),
            payload: Some(blob.id.to_string()),
            metadata: serde_json::json!({ "kind": TICKET_UPDATE_KIND, "message_id": message }),
            dedupe: format!("chat-update:{message}"),
        })?;
        notices.push(n);
        conv::insert_update(tx, message, first.conversation, &blob.id.to_string(), event, &serde_json::json!({ "kind": TICKET_UPDATE_KIND, "from": "Aurelius" }))?;
        aurelius_storage::inbox::follow_main_chat(tx, first.conversation, chat.employee)?;
        let (_, queued) = queue_run(tx, &serde_json::json!({ "origin": TICKET_UPDATE_KIND }), &correlation, first.conversation, message)?;
        notices.extend(queued);
        for update in group {
            chats::told(tx, update.ticket)?;
        }
    }
    Ok(notices)
}

/// `tell_responsible`: a message in the chat of the responsible of the run's ticket
/// with this employee, unread there. Returns (ok, text for the model, notices).
pub(crate) fn tell_responsible(tx: &Tx<'_>, st: &Store, run: RunId, message: &str) -> Result<(bool, String, Vec<EventNotice>), StorageError> {
    let refuse = |why: String| Ok((false, why, vec![]));
    let Some(me) = crate::task::task_runs::run_owner(tx, run)?.and_then(|o| o.employee) else {
        return refuse("Only an employee can use tell_responsible.".into());
    };
    let text = message.trim();
    if text.is_empty() {
        return refuse("Write a message to send.".into());
    }
    if text.chars().count() > MESSAGE_MAX {
        return refuse(format!("The message is too long: at most {MESSAGE_MAX} characters."));
    }
    let Some(ticket) = crate::conversation::discussion::run_handoff_thread(tx, run)?.and_then(|d| d.ticket().cloned()) else {
        return refuse(NO_TICKET_TO_TELL.into());
    };
    let read = |key: &str| st.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
    let responsible = aurelius_storage::tickets::view(tx, ticket.id, &read)?.filter(|v| !v.responsible_disabled).and_then(|v| v.responsible_user_id.zip(v.responsible_username));
    let Some((user, username)) = responsible else { return refuse(NO_RESPONSIBLE.into()) };
    let Some(chat) = chats::ensure(tx, me, user)? else { return refuse("Only an employee can use tell_responsible.".into()) };
    let from = crate::conversation::discussion::employee_name(tx, me)?;
    let shown = format!("{text}\n\nFrom ticket {} {}", ticket_link(st, ticket.id, ticket.number), ticket.title);
    let blob = st.blobs().put(shown.as_bytes(), "text/markdown; charset=utf-8")?;
    blobs::register(tx, &blob)?;
    let id = MessageId::new();
    let (event, n) = emit(tx, Ev {
        aggregate_type: "conversation",
        aggregate_id: chat.to_string(),
        event_type: "message.assistant_committed",
        actor: ActorKind::Employee,
        actor_id: Some(me.to_string()),
        correlation: run.to_string(),
        payload: Some(blob.id.to_string()),
        metadata: serde_json::json!({ "kind": TICKET_NOTE_KIND, "message_id": id, "ticket_id": ticket.id, "run_id": run }),
        dedupe: format!("ticket-note:{id}"),
    })?;
    conv::insert_update(tx, id, chat, &blob.id.to_string(), event, &serde_json::json!({ "kind": TICKET_NOTE_KIND, "from": from, "ticket_id": ticket.id }))?;
    Ok((true, format!("Sent to @{username}'s chat with you, marked unread. It starts no run and changes nothing on the ticket."), vec![n]))
}

impl CommandService {
    /// The acting board member's chat with an employee, made the first time.
    pub(crate) fn open_employee_chat(&self, ctx: &CommandContext, envelope: &CommandEnvelope, hash: String, cmd: &OpenEmployeeChat) -> Result<Committed, AppError> {
        let user = ctx.user_id.filter(|_| ctx.app.is_none()).ok_or_else(|| domain_error(DomainError::BoardMemberRequired))?;
        let employee = cmd.employee_id;
        self.commit(envelope.command_id, hash, move |tx| {
            match employees::load_employee(tx, employee)?.map(|e| e.status) {
                None => return Ok(Err(domain_error(DomainError::EmployeeNotFound(employee)))),
                Some(LifecycleStatus::Archived) => return Ok(Err(domain_error(DomainError::EmployeeArchived))),
                Some(_) => {}
            }
            let Some(conversation_id) = chats::ensure(tx, employee, user)? else {
                return Ok(Err(domain_error(DomainError::EmployeeNotFound(employee))));
            };
            Ok(Ok((CommandResult::EmployeeChatOpened { conversation_id }, vec![])))
        })
    }

    /// What chats are owed goes out now (a pass of the dispatcher). Read first: there
    /// is almost never anything.
    pub fn deliver_chat_updates(&self) -> Result<Vec<EventNotice>, AppError> {
        if !self.store.read(chats::updates_waiting).map_err(storage_error)? {
            return Ok(Vec::new());
        }
        let st = self.store.clone();
        self.store.write(move |tx| deliver_updates(tx, &st)).map_err(storage_error)
    }
}
