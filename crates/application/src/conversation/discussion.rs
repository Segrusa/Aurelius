//! The discussions: one engine for the Team chat's channels (the general one,
//! one per project) and the discussion of each ticket. Every discussion is a
//! conversation (kind `group` or `ticket`); the rules differ only in who answers
//! a board member's message without `@Name` (B3).
//!
//! * Who answers: the employees the message names with `@Name`. Without a name,
//!   from a board member: in the general channel the top of the org chart (the
//!   CEO); in a project channel the project's dispatcher, else the CEO; in a
//!   ticket's discussion the owner, while the ticket is in Todo (with an owner),
//!   Doing, Plan review or Review, as a message in its main task (no run of its
//!   own: the main task's conversation answers it); else nobody, with a hint. A post
//!   of an employee wakes only the colleagues it names, never itself.
//! * Loop limits (RULE 08), the same constants for every discussion and counted
//!   over all of them together: at most `MAX_HOPS` hops after one message of a
//!   board member, `WAKING_POSTS_PER_RUN` waking posts per run and
//!   `WAKING_POSTS_PER_WINDOW` per employee per hour. A mention the limits refuse
//!   leaves a system line ("Limit reached: X was not woken."). System lines and
//!   copied answers never wake anyone.
//! * How: the message, with the latest messages of the discussion (a JSON list,
//!   so no message can pass for another or for a board member's), goes to each
//!   of them as an update (`[Group chat · from …]`) in their own main chat, where
//!   a normal run answers it. A woken employee that is not the ticket's owner
//!   only answers there: the ticket's column and main task stay as they are.
//! * The answer: copied to the discussion as the employee's message when the run
//!   ends (`mirror_answer`), streamed there live under that message's id meanwhile.
//! * A ticket on Done (or archived) and the channel of an archived project are
//!   read-only (B7).

use crate::run::events::{Ev, emit};
use crate::run::queue::queue_run_marked;
use crate::{CommandContext, CommandService, Committed, domain_error, storage_error};
use aurelius_domain::autonomy::AgentMode;
use aurelius_domain::conversation::{MessageRole, MessageStatus};
use aurelius_domain::group_chat::{self as group, CONTEXT_CHARS, CONTEXT_MESSAGES, GROUP_KIND, MAX_HOPS, OWNER, POST_MAX, WAKE_WINDOW_MS, WAKING_POSTS_PER_RUN, WAKING_POSTS_PER_WINDOW};
use aurelius_domain::ticket::ColumnKind;
use aurelius_domain::views::{MessageFileView, SystemLine, SystemLineKind};
use aurelius_domain::{ActorKind, AppError, CommandEnvelope, CommandResult, ConversationId, EmployeeId, ErrorCode, EventNotice, MessageId, ProjectId, RunId, TaskId, TicketId, UserId};
use aurelius_storage::conversations::{self as conv, GroupRun, NewMessage};
use aurelius_storage::{StorageError, Store, Tx, blobs, employees};
use rusqlite::OptionalExtension;

/// Why `message.react` is refused: reactions are phase 2 (feature `chat_reactions`, not advertised yet).
pub(crate) const REACTIONS_NOT_YET: &str = "Reactions are not supported by this server yet.";
/// Why a reply in a thread is refused: threads are phase 2 (feature `chat_threads`, not advertised yet).
pub(crate) const THREADS_NOT_YET: &str = "Replies in a thread are not supported by this server yet.";

/// The ticket a discussion belongs to, as the engine needs it.
#[derive(Debug, Clone)]
pub(crate) struct TicketPlace {
    pub id: TicketId,
    pub number: u32,
    pub title: String,
    pub project: ProjectId,
    pub column: ColumnKind,
    pub owner: Option<EmployeeId>,
    pub root_task: Option<TaskId>,
    pub archived: bool,
}

#[derive(Debug, Clone)]
pub(crate) enum Place {
    /// The general channel (the group chat of before the channels).
    General,
    /// A project's channel.
    Project { project: ProjectId, name: String, archived: bool },
    Ticket(TicketPlace),
}

/// One discussion: its conversation and where it is.
#[derive(Debug, Clone)]
pub(crate) struct Discussion {
    pub id: ConversationId,
    pub place: Place,
}

impl Discussion {
    /// Why nobody can write here any more, if so.
    pub(crate) fn read_only(&self) -> Option<String> {
        match &self.place {
            Place::General => None,
            Place::Project { name, archived: true, .. } => Some(format!("The project {name} is archived: its channel can only be read.")),
            Place::Project { .. } => None,
            Place::Ticket(t) if t.column == ColumnKind::Done => Some(format!("Ticket #{} is done: its discussion can only be read. Follow-up work is a new ticket.", t.number)),
            Place::Ticket(t) if t.archived => Some(format!("Ticket #{} is archived: its discussion can only be read.", t.number)),
            Place::Ticket(_) => None,
        }
    }

    /// How a tool answer names it.
    pub(crate) fn label(&self) -> String {
        match &self.place {
            Place::General => "the group chat".into(),
            Place::Project { name, .. } => format!("the #{name} channel"),
            Place::Ticket(t) => format!("the discussion of ticket #{}", t.number),
        }
    }

    /// The line in front of a message that wakes an employee: where it was written.
    fn heading(&self) -> Option<String> {
        match &self.place {
            Place::General => None,
            Place::Project { name, .. } => Some(format!("In the #{name} channel of the Team chat (about the project {name}):")),
            Place::Ticket(t) => Some(format!("In the discussion of ticket #{} “{}”:", t.number, t.title)),
        }
    }

    pub(crate) fn ticket(&self) -> Option<&TicketPlace> {
        match &self.place {
            Place::Ticket(t) => Some(t),
            _ => None,
        }
    }
}

/// The discussion `id` is, if it is one (a channel or a ticket's discussion).
pub(crate) fn load(conn: &rusqlite::Connection, id: ConversationId) -> Result<Option<Discussion>, StorageError> {
    let row: Option<(String, Option<String>)> =
        conn.query_row("SELECT kind, project_id FROM conversations WHERE id = ?1 AND kind IN ('group', 'ticket')", [id.to_string()], |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
    let Some((kind, project)) = row else { return Ok(None) };
    let place = match (kind.as_str(), project.and_then(|p| ProjectId::parse(&p).ok())) {
        ("group", None) => Place::General,
        ("group", Some(project)) => {
            let Some((name, archived)) = conn
                .query_row("SELECT name, status = 'archived' FROM projects WHERE id = ?1", [project.to_string()], |r| Ok((r.get::<_, String>(0)?, r.get::<_, bool>(1)?)))
                .optional()?
            else {
                return Ok(None);
            };
            Place::Project { project, name, archived }
        }
        _ => {
            let Some(ticket) = aurelius_storage::discussions::ticket_of_conversation(conn, id)? else { return Ok(None) };
            match ticket_place(conn, ticket)? {
                Some(t) => Place::Ticket(t),
                None => return Ok(None),
            }
        }
    };
    Ok(Some(Discussion { id, place }))
}

fn ticket_place(conn: &rusqlite::Connection, ticket: TicketId) -> Result<Option<TicketPlace>, StorageError> {
    let Some(state) = aurelius_storage::tickets::state(conn, ticket)? else { return Ok(None) };
    let (number, title): (i64, String) = conn.query_row("SELECT number, title FROM tickets WHERE id = ?1", [ticket.to_string()], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(Some(TicketPlace { id: ticket, number: number as u32, title, project: state.project_id, column: state.column, owner: state.owner, root_task: state.root_task_id, archived: state.archived }))
}

/// The general channel.
pub(crate) fn general(conn: &rusqlite::Connection) -> Result<Option<Discussion>, StorageError> {
    Ok(conv::group_conversation(conn)?.map(|id| Discussion { id, place: Place::General }))
}

/// The channel of `project`, if it has one.
pub(crate) fn project_channel(conn: &rusqlite::Connection, project: ProjectId) -> Result<Option<Discussion>, StorageError> {
    if !aurelius_storage::discussions::has_discussions(conn)? {
        return Ok(None);
    }
    match aurelius_storage::discussions::channel(conn, Some(project))? {
        Some(id) => load(conn, id),
        None => Ok(None),
    }
}

/// The discussion of `ticket`.
pub(crate) fn of_ticket(conn: &rusqlite::Connection, ticket: TicketId) -> Result<Option<Discussion>, StorageError> {
    if !aurelius_storage::discussions::has_discussions(conn)? {
        return Ok(None);
    }
    match aurelius_storage::discussions::ticket_conversation(conn, ticket)? {
        Some(id) => load(conn, id),
        None => Ok(None),
    }
}

/// The employees of the company that are not archived: (id, name, active).
fn team(conn: &rusqlite::Connection) -> Result<Vec<(EmployeeId, String, bool)>, StorageError> {
    let mut stmt = conn.prepare("SELECT id, name, status FROM employees WHERE status <> 'archived' ORDER BY created_at_ms, id")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))?;
    let mut out = Vec::new();
    for row in rows {
        let (id, name, status) = row?;
        if let Ok(id) = EmployeeId::parse(&id) {
            out.push((id, name, status == "active"));
        }
    }
    Ok(out)
}

fn ceo(conn: &rusqlite::Connection) -> Result<Option<EmployeeId>, StorageError> {
    Ok(aurelius_storage::app_keys::management(conn)?.into_iter().find(|m| m.is_ceo).map(|m| m.id))
}

/// Whether a board member's message without `@Name` goes to the ticket's owner
/// (B3): while the ticket is in Todo with an owner, Doing, Plan review, Testing
/// or Review (the owner's report often ends with a question the board member
/// answers here; Testing counts like Review).
fn owner_answers(t: &TicketPlace) -> bool {
    t.owner.is_some()
        && !t.archived
        && matches!(t.column, ColumnKind::Todo | ColumnKind::Doing | ColumnKind::PlanReview | ColumnKind::Testing | ColumnKind::Review)
}

/// Who answers `text` in `d`: the active employees it names (not `author`);
/// without a name, from a board member (`author` none), the one B3 names.
/// `owner`: a board member's message goes to the ticket's owner, in its main
/// task (also when it names the owner). `paused`: those it would have woken
/// but who are paused.
#[derive(Default)]
struct Addressees {
    awake: Vec<(EmployeeId, String)>,
    paused: Vec<String>,
    owner: Option<(EmployeeId, String)>,
}

fn addressees(conn: &rusqlite::Connection, d: &Discussion, text: &str, author: Option<EmployeeId>) -> Result<Addressees, StorageError> {
    let team = team(conn)?;
    let names: Vec<(EmployeeId, String)> = team.iter().map(|(id, name, _)| (*id, name.clone())).collect();
    let mut chosen = group::mentions(text, &names);
    let mut out = Addressees::default();
    if author.is_none() {
        match &d.place {
            Place::Ticket(t) if owner_answers(t) => {
                let owner = t.owner.expect("owner_answers needs one");
                if chosen.is_empty() || chosen.contains(&owner) {
                    chosen.retain(|e| *e != owner);
                    match team.iter().find(|(e, _, _)| *e == owner) {
                        Some((_, name, true)) => out.owner = Some((owner, name.clone())),
                        Some((_, name, false)) => out.paused.push(name.clone()),
                        None => {}
                    }
                }
            }
            Place::Ticket(_) => {}
            Place::General if chosen.is_empty() => chosen.extend(ceo(conn)?),
            Place::Project { project, .. } if chosen.is_empty() => match aurelius_storage::company::dispatcher_for(conn, *project)? {
                Some(dispatcher) => chosen.push(dispatcher),
                None => chosen.extend(ceo(conn)?),
            },
            _ => {}
        }
    }
    for (id, name, active) in chosen.into_iter().filter(|id| Some(*id) != author).filter_map(|id| team.iter().find(|(e, _, _)| *e == id)) {
        if *active {
            out.awake.push((*id, name.clone()));
        } else {
            out.paused.push(name.clone());
        }
    }
    Ok(out)
}

/// The latest `limit` messages of a discussion before `before` (all when none),
/// oldest first, each as one JSON object `{"from", "text"}` (text cut at
/// `chars`): its newlines stay inside the quotes, so no message can pass for another.
pub(crate) fn discussion_json(conn: &rusqlite::Connection, read: &dyn Fn(&str) -> Option<String>, id: ConversationId, before: Option<MessageId>, limit: u32, chars: usize) -> Result<Vec<String>, StorageError> {
    let rows = aurelius_storage::discussions::latest_messages(conn, id, before, limit)?;
    Ok(rows
        .into_iter()
        .rev()
        .filter_map(|line| {
            let text = group::shortened(&read(&line.storage_key)?, chars);
            if text.is_empty() {
                return None;
            }
            // A board member's message names them, and says it is one (an employee may have the same name).
            Some(match (line.author, line.author_user) {
                (None, Some((_, user))) => serde_json::json!({ "from": user, "board_member": true, "text": text }),
                (from, _) => serde_json::json!({ "from": from.as_deref().unwrap_or(OWNER), "text": text }),
            }
            .to_string())
        })
        .collect())
}

/// The context of the latest messages of `id` before `before` (all when none),
/// as a JSON list (empty when there are none): what a woken employee, or a run
/// under a ticket at its start, gets with it.
pub(crate) fn context_text(conn: &rusqlite::Connection, read: &dyn Fn(&str) -> Option<String>, id: ConversationId, before: Option<MessageId>) -> Result<String, StorageError> {
    let lines = discussion_json(conn, read, id, before, CONTEXT_MESSAGES as u32, CONTEXT_CHARS)?;
    Ok(if lines.is_empty() { String::new() } else { format!("[\n{}\n]", lines.join(",\n")) })
}

fn reader(store: &Store) -> impl Fn(&str) -> Option<String> + '_ {
    |key: &str| store.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok())
}

/// What the discussion hears when a board member's message wakes nobody.
fn nobody_answers(d: &Discussion, paused: &[String]) -> String {
    match (&d.place, paused) {
        (_, [one]) => format!("Nobody was woken: {one} is paused. Resume them, or name a colleague with @Name."),
        (_, [_, ..]) => format!("Nobody was woken: {} are paused. Resume them, or name a colleague with @Name.", paused.join(", ")),
        (Place::Ticket(t), []) if t.owner.is_none() => "Nobody was woken: this ticket has no owner yet. Name a colleague with @Name.".into(),
        (Place::Ticket(_), []) => "Nobody was woken: the owner only answers here while the ticket is in Todo, Doing, Plan review, Testing or Review. Name a colleague with @Name.".into(),
        (_, []) => "Nobody was woken: there is no CEO to answer a message without @Name. Name a colleague with @Name.".into(),
    }
}

/// A message in a discussion: an update (`kind`: an employee's, or a note of
/// Aurelius) with `from` as author, under `message` (the id it was streamed
/// under, for an answer).
#[allow(clippy::too_many_arguments)]
fn post_in(tx: &Tx<'_>, store: &Store, id: ConversationId, message: MessageId, kind: &str, from: &str, text: &str, extra: serde_json::Value) -> Result<EventNotice, StorageError> {
    let blob = store.blobs().put(text.as_bytes(), "text/markdown; charset=utf-8")?;
    blobs::register(tx, &blob)?;
    let (event, notice) = emit(tx, Ev {
        aggregate_type: "conversation",
        aggregate_id: id.to_string(),
        event_type: "task.posted",
        actor: ActorKind::Employee,
        actor_id: None,
        correlation: id.to_string(),
        payload: Some(blob.id.to_string()),
        metadata: serde_json::json!({ "task_id": null, "kind": kind, "from": from, "message_id": message }),
        dedupe: format!("group-post:{message}"),
    })?;
    let mut meta = serde_json::json!({ "kind": kind, "from": from });
    if let (Some(m), Some(e)) = (meta.as_object_mut(), extra.as_object()) {
        m.extend(e.clone());
    }
    conv::insert_update(tx, message, id, &blob.id.to_string(), event, &meta)?;
    Ok(notice)
}

/// A system line of Aurelius in a discussion (`MessageView::system`): never a
/// message employees read as one of the discussion's, and it wakes nobody. Its
/// text is the line in words (`SystemLine::sentence`), with the ticket's number.
pub(crate) fn post_system(tx: &Tx<'_>, id: ConversationId, line: &SystemLine) -> Result<EventNotice, StorageError> {
    system_line(tx, id, line, None)
}

/// A system line with its own words (`text`; none: `SystemLine::sentence`) in
/// any conversation, in the transaction of what caused it (MU2-03). It wakes
/// nobody and is never part of what a woken employee reads (`latest_messages`
/// takes only messages); a discussion on Done takes it too (B7 is for people and tools).
pub(crate) fn system_line(tx: &Tx<'_>, id: ConversationId, line: &SystemLine, text: Option<&str>) -> Result<EventNotice, StorageError> {
    let text = match text.map(str::trim).filter(|t| !t.is_empty()) {
        Some(text) => text.to_owned(),
        None => {
            let number = match line.ticket_id {
                Some(ticket) => tx.query_row("SELECT number FROM tickets WHERE id = ?1", [ticket.to_string()], |r| r.get::<_, i64>(0)).optional()?.map(|n| n as u32),
                None => None,
            };
            line.sentence(number)
        }
    };
    let blob = aurelius_storage::blobs::put_beside(tx, text.as_bytes(), "text/markdown; charset=utf-8")?;
    let message = MessageId::new();
    let (event, notice) = emit(tx, Ev {
        aggregate_type: "conversation",
        aggregate_id: id.to_string(),
        event_type: "task.posted",
        actor: ActorKind::System,
        actor_id: None,
        correlation: id.to_string(),
        payload: Some(blob.id.to_string()),
        metadata: serde_json::json!({ "task_id": null, "kind": "status", "from": "Aurelius", "message_id": message, "system": line.kind }),
        dedupe: format!("system-line:{message}"),
    })?;
    conv::insert_update(tx, message, id, &blob.id.to_string(), event, &serde_json::json!({ "kind": "status", "from": "Aurelius", "system": line }))?;
    Ok(notice)
}

/// A system line in the discussion of `ticket` (none before 0019): `line.ticket_id` is set to it.
pub(crate) fn ticket_line(tx: &Tx<'_>, ticket: TicketId, mut line: SystemLine, text: &str) -> Result<Vec<EventNotice>, StorageError> {
    let Some(d) = of_ticket(tx, ticket)? else { return Ok(Vec::new()) };
    line.ticket_id = Some(ticket);
    Ok(vec![system_line(tx, d.id, &line, Some(text))?])
}

/// Who did something, as a system line names them: the board member's username,
/// the employee's name, or "Aurelius" for the system.
pub(crate) fn actor_name(conn: &rusqlite::Connection, by: Option<aurelius_domain::ticket::Actor>) -> Result<String, StorageError> {
    use aurelius_domain::ticket::Actor;
    Ok(match by {
        Some(Actor::BoardMember { user_id }) => board_name(conn, Some(user_id))?.unwrap_or_else(|| "A board member".into()),
        Some(Actor::Employee { employee_id }) => employee_name(conn, employee_id)?,
        None => "Aurelius".into(),
    })
}

pub(crate) fn employee_name(conn: &rusqlite::Connection, employee: EmployeeId) -> Result<String, StorageError> {
    let name: Option<String> = conn.query_row("SELECT name FROM employees WHERE id = ?1", [employee.to_string()], |r| r.get(0)).optional()?;
    Ok(name.unwrap_or_else(|| "An employee".into()))
}

/// The conversation of a message, how a line names it ("the #Web channel",
/// "Ann's chat", the task's, the project assistant's), for a ticket made from it,
/// and the project it belongs to (none: the General channel, an employee's chat).
/// None for a message that does not exist or is in no known kind of conversation.
pub(crate) fn message_place(conn: &rusqlite::Connection, message: MessageId) -> Result<Option<(ConversationId, String, Option<ProjectId>)>, StorageError> {
    let row: Option<(String, String, Option<String>, Option<String>, Option<String>, Option<String>)> = conn
        .query_row(
            "SELECT c.id, c.kind, p.name, e.name, (SELECT t.title FROM tasks t WHERE t.conversation_id = c.id ORDER BY t.created_at_ms DESC LIMIT 1), c.project_id
             FROM messages m JOIN conversations c ON c.id = m.conversation_id LEFT JOIN projects p ON p.id = c.project_id LEFT JOIN employees e ON e.id = c.employee_id
             WHERE m.id = ?1",
            [message.to_string()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
        )
        .optional()?;
    let Some((id, kind, project, employee, task, project_id)) = row else { return Ok(None) };
    let Ok(id) = ConversationId::parse(&id) else { return Ok(None) };
    if !matches!(kind.as_str(), "group" | "ticket" | "employee" | "project" | "task") {
        return Ok(None);
    }
    let project_id = match project_id {
        Some(p) => match ProjectId::parse(&p) {
            Ok(p) => Some(p),
            Err(_) => return Ok(None),
        },
        None => None,
    };
    let label = match (kind.as_str(), project, employee, task) {
        ("group", None, _, _) => "the General channel".to_owned(),
        ("group", Some(project), _, _) => format!("the #{project} channel"),
        ("ticket", _, _, _) => match aurelius_storage::discussions::ticket_of_conversation(conn, id)?.map(|t| ticket_place(conn, t)).transpose()?.flatten() {
            Some(t) => format!("the discussion of #{}", t.number),
            None => "a ticket's discussion".to_owned(),
        },
        ("task", _, _, Some(task)) => format!("the task “{task}”"),
        ("project", Some(project), _, _) => format!("the assistant of {project}"),
        (_, _, Some(employee), _) => format!("{employee}'s chat"),
        _ => "a chat".to_owned(),
    };
    Ok(Some((id, label, project_id)))
}

/// A ticket made from a message (MU2-10 AC3): its discussion's first line links to
/// the message (`created_from`), and the message's conversation gets `ticket_created`
/// with the message's id, so clients show "→ #N" at it. Neither wakes anyone.
pub(crate) fn created_from(tx: &Tx<'_>, ticket: TicketId, number: u32, user: UserId, conversation: ConversationId, message: MessageId, label: &str) -> Result<Vec<EventNotice>, StorageError> {
    let by = board_name(tx, Some(user))?.unwrap_or_else(|| "A board member".into());
    let mut first = SystemLine::new(SystemLineKind::CreatedFrom);
    (first.by, first.conversation_id, first.message_id) = (Some(by.clone()), Some(conversation), Some(message));
    let mut notices = ticket_line(tx, ticket, first, &format!("#{number} created from a message in {label} · {by}"))?;
    let mut back = SystemLine::new(SystemLineKind::TicketCreated);
    (back.by, back.ticket_id, back.message_id) = (Some(by), Some(ticket), Some(message));
    // The link to the ticket's channel; in General this is its one line on opening.
    back.channel_id = of_ticket(tx, ticket)?.map(|d| d.id);
    if general(tx)?.is_some_and(|g| g.id == conversation) {
        back.action = Some(aurelius_domain::views::system_action::OPENED.to_owned());
    }
    notices.push(system_line(tx, conversation, &back, None)?);
    Ok(notices)
}

/// "Limit reached: X was not woken." for a mention the loop limits refused.
fn limit_line(tx: &Tx<'_>, id: ConversationId, names: &str) -> Result<EventNotice, StorageError> {
    let mut line = SystemLine::new(SystemLineKind::Limit);
    line.to = Some(names.to_owned());
    post_system(tx, id, &line)
}

/// What a message brings along for the employees: its images (shown to
/// them) and the files the board member shared (named by their path on the server).
#[derive(Default)]
struct Shared<'a> {
    images: &'a [aurelius_domain::views::FileRef],
    files: Vec<String>,
}

/// One message that wakes employees: where and who wrote it, its text, what it
/// brings along and the messages before it (`context_text`).
struct Wake<'a> {
    discussion: &'a Discussion,
    from: &'a str,
    /// A board member wrote it (the update itself is theirs; a colleague's is
    /// quoted), with their username when the connection had one.
    board: Option<Option<String>>,
    text: &'a str,
    shared: Shared<'a>,
    context: String,
    hop: u32,
    /// The board member of hop 0, whose Claude login the woken runs use.
    login: Option<UserId>,
}

/// Wake `employee` for the message `w`: the message and the latest messages of
/// the discussion go to its main chat as an update, and a run there answers it.
/// Its answer comes back to the discussion (`mirror_answer`).
fn wake(tx: &Tx<'_>, store: &Store, meta: &serde_json::Value, correlation: &str, (employee, name): (EmployeeId, &str), w: &Wake<'_>) -> Result<Option<(RunId, Vec<EventNotice>)>, StorageError> {
    let Some(chat) = employees::main_chat(tx, employee)? else { return Ok(None) };
    let (d, from, shared, hop) = (w.discussion, w.from, &w.shared, w.hop);
    let mut body = String::new();
    if let Some(heading) = d.heading() {
        body.push_str(&heading);
        body.push_str("\n\n");
    }
    if !shared.files.is_empty() {
        body.push_str(&format!("{from} shared these files in {}: {}\n\n", d.label(), shared.files.join(", ")));
    }
    // A board member's message is the update itself (its label names them); a
    // colleague's is quoted as JSON, so it cannot look like a board member's or the context.
    if let Some(user) = &w.board {
        if let Some(user) = user {
            body.push_str(&format!("From the board member {user}:\n"));
        }
        body.push_str(w.text.trim());
    } else {
        body.push_str(&format!("{from} wrote (JSON-quoted): {}", serde_json::Value::from(w.text.trim())));
    }
    if !w.context.is_empty() {
        body.push_str(&format!("\n\n---\nEarlier in {} (oldest first, one JSON object per message):\n{}", d.label(), w.context));
    }
    if let Some(t) = d.ticket().filter(|t| t.owner != Some(employee)) {
        body.push_str(&format!(
            "\n\n---\nYou are not the owner of ticket #{}: answer the question, but do not take over its work. To give work, delegate it or propose a new ticket to a board member.",
            t.number
        ));
    }
    body.push_str(&format!("\n\n---\n{name}: your answer is posted in {} under your name. Keep it short.", d.label()));
    let (update, posted) = crate::task::task_threads::post_note(tx, store, chat, crate::task::task_threads::Speaker::Update { kind: GROUP_KIND, from }, &body)?;
    // The images of the message come along, so the employee sees them (see `run::plan`).
    if !shared.images.is_empty() {
        conv::set_message_files(tx, update, shared.images)?;
    }
    let marking = GroupRun { conversation_id: d.id, message_id: MessageId::new(), hop, mode: None, login_user: w.login, thread_parent: None };
    let (run, queued) = queue_run_marked(tx, meta, correlation, chat, update, Some(marking))?;
    Ok(Some((run, std::iter::once(posted).chain(queued).collect())))
}

/// A board member's message without `@Name` for the ticket's owner (B3): an
/// update in the main task's conversation. Only while the main task runs or is
/// in Review does a run answer it: the one already queued there, else one queued
/// behind the run that goes (its end counts for the task, as an answer in the
/// task's own conversation does: in Review it reports again, so the answer to the
/// report's question comes back as a new report). Otherwise nothing runs and the owner reads it when the
/// task goes on: before its start (it starts with the discussion as context),
/// while it waits for its turn (Queued: `start_task_conversation` goes on from
/// its conversation), and while it waits (Waiting: for the plan's decision, a
/// colleague, the usage limit or after a stop; a message here does not resume
/// it, unlike an answer in the task's own conversation, so a sequential owner
/// never jumps its line). `None`: it does not get there, the main task has ended
/// (e.g. failed in Doing, B6: Start makes a new one), with the hint for the discussion.
fn to_owner(tx: &Tx<'_>, store: &Store, t: &TicketPlace, from: &str, text: &str) -> Result<Option<Vec<EventNotice>>, StorageError> {
    use aurelius_domain::task::TaskStatus;
    let Some(root) = t.root_task else { return Ok(Some(vec![])) };
    let Some(row) = aurelius_storage::tasks::load_task(tx, root)? else { return Ok(Some(vec![])) };
    if row.status.is_terminal() {
        return Ok(None);
    }
    let Some((conversation, _)) = aurelius_storage::tasks::task_conversation(tx, root)? else { return Ok(Some(vec![])) };
    if row.status == TaskStatus::Draft {
        return Ok(Some(vec![]));
    }
    let body = format!(
        "{}\n\n---\nFrom the discussion of ticket #{}. Answer the board member there with post_to_discussion.",
        text.trim(),
        t.number
    );
    let (message, posted) = crate::task::task_threads::post_returning(tx, store, conversation, root, crate::task::task_threads::Speaker::Update { kind: "message", from }, &body)?;
    let mut notices = vec![posted];
    if !matches!(row.status, TaskStatus::Running | TaskStatus::Review) {
        return Ok(Some(notices));
    }
    let runs = conv::unfinished_runs_in(tx, conversation)?;
    if runs.iter().any(|(_, status)| status == "created") {
        return Ok(Some(notices));
    }
    let (run, queued) = crate::run::queue::queue_run(tx, &serde_json::json!({ "task_id": root, "discussion": t.id }), &root.to_string(), conversation, message)?;
    tx.execute("UPDATE runs SET task_id = ?2 WHERE id = ?1", [run.to_string(), root.to_string()])?;
    notices.extend(queued);
    Ok(Some(notices))
}

/// The hint when the owner's main task has ended: the message did not reach it.
fn main_task_ended(tx: &Tx<'_>, t: &TicketPlace) -> Result<String, StorageError> {
    let failed = match t.root_task {
        Some(root) => aurelius_storage::tasks::load_task(tx, root)?.is_some_and(|r| r.status == aurelius_domain::task::TaskStatus::Failed),
        None => false,
    };
    let what = if failed { "The main task failed" } else { "The main task has ended" };
    Ok(format!("{what} · Start makes a new one. The owner did not get this message; name a colleague with @Name to ask someone now."))
}

/// A line of Aurelius that tells the board member why a message reached nobody
/// (or not the owner): a system line (`Limit`, without `to`; it wakes nobody).
fn nobody_line(tx: &Tx<'_>, id: ConversationId, text: &str) -> Result<EventNotice, StorageError> {
    system_line(tx, id, &SystemLine::new(SystemLineKind::Limit), Some(text))
}

/// The answer of a run a discussion woke, copied there as the employee's
/// message (`GroupRun::message_id`, the id it streamed under). Nothing for
/// another run, or for an answer that is empty and did not fail. `mode`: the
/// mode the run started in (none when it never started), shown with it.
///
/// Also after the ticket went to Done meanwhile (B7 is for people and tools): the
/// answer of an employee woken before Done still lands in the read-only
/// discussion, so the question it answers is not left open (accepted, AUR-MU-02-F3).
#[allow(clippy::too_many_arguments)]
pub(crate) fn mirror_answer(tx: &Tx<'_>, store: &Store, run: RunId, conversation: ConversationId, text: &str, failed: Option<&str>, files: &[aurelius_domain::views::FileRef], mode: Option<AgentMode>) -> Result<Vec<EventNotice>, StorageError> {
    let Some(marking) = conv::group_run(tx, run)? else { return Ok(Vec::new()) };
    // Already copied (the run's job finished twice: never).
    if conv::message_ordinal(tx, marking.message_id)?.is_some() {
        return Ok(Vec::new());
    }
    let name = employees::employee_for_conversation(tx, conversation)?.map(|(name, _, _)| name).unwrap_or_else(|| "Aurelius".into());
    let text = match (text.trim(), failed) {
        ("", None) => return Ok(Vec::new()),
        ("", Some(code)) => format!("(Could not answer: {code}.)"),
        (text, None) => group::shortened(text, crate::task::task_threads::MAX_POST),
        // A partial answer says it is one.
        (text, Some("interrupted")) => format!("{}\n\n(interrupted)", group::shortened(text, crate::task::task_threads::MAX_POST)),
        (text, Some(code)) => format!("{}\n\n(interrupted: {code})", group::shortened(text, crate::task::task_threads::MAX_POST)),
    };
    let notice = post_in(tx, store, marking.conversation_id, marking.message_id, GROUP_KIND, &name, &text, serde_json::json!({ "run_id": run, "hop": marking.hop, "mode": mode }))?;
    if !files.is_empty() {
        conv::set_message_files(tx, marking.message_id, files)?;
    }
    Ok(vec![notice])
}

/// What a board member's message in a discussion did (`GroupMessageSent` /
/// `TicketMessagePosted`): `to` the employees it reached, `run_ids` the runs it
/// woke, one per woken employee in the same order. The owner of a ticket, when
/// the message went to its main task (B3), comes first in `to` and has no entry
/// in `run_ids` (whatever runs there is the task's): so `to.len() > run_ids.len()`
/// means "the owner gets it in the main task", and `to[1..]` were woken.
struct Sent {
    to: Vec<EmployeeId>,
    run_ids: Vec<RunId>,
}

impl CommandService {
    /// A board member writes in a channel of the Team chat (`group_chat.send`): the
    /// message is stored, and the employees it names (or the one B3 names) each
    /// answer from their own main chat.
    pub(crate) fn send_group_message(&self, ctx: &CommandContext, envelope: &CommandEnvelope, hash: String, cmd: &group::SendGroupMessage) -> Result<Committed, AppError> {
        if cmd.thread_parent.is_some() {
            return Err(AppError::new(ErrorCode::Validation, THREADS_NOT_YET));
        }
        let text = cmd.validate().map_err(domain_error)?;
        let discussion = self
            .store
            .read(|c| match cmd.channel {
                None => general(c),
                // A ticket's channel too (feature `ticket_channels`): the same discussion as `ticket.post`.
                Some(id) => load(c, id),
            })
            .map_err(storage_error)?
            .ok_or_else(|| AppError::new(ErrorCode::NotFound, if cmd.channel.is_some() { "This channel does not exist." } else { "The group chat does not exist." }))?;
        if discussion.ticket().is_some() {
            crate::ticket::require_board_member(ctx)?;
        }
        self.send_in(ctx, envelope, hash, discussion, text, &cmd.images, &cmd.files, |d, s| CommandResult::GroupMessageSent { conversation_id: d.id, message_id: s.0, to: s.1.to, run_ids: s.1.run_ids })
    }

    /// A board member writes in a ticket's discussion (`ticket.post`).
    pub(crate) fn post_ticket_message(&self, ctx: &CommandContext, envelope: &CommandEnvelope, hash: String, cmd: &aurelius_domain::ticket::PostTicketMessage) -> Result<Committed, AppError> {
        crate::ticket::require_board_member(ctx)?;
        if cmd.thread_parent.is_some() {
            return Err(AppError::new(ErrorCode::Validation, THREADS_NOT_YET));
        }
        let text = cmd.validate().map_err(domain_error)?;
        let ticket = cmd.ticket_id;
        let discussion = self
            .store
            .read(|c| of_ticket(c, ticket))
            .map_err(storage_error)?
            .ok_or_else(|| AppError::new(ErrorCode::NotFound, format!("Ticket {ticket} has no discussion.")))?;
        self.send_in(ctx, envelope, hash, discussion, text, &cmd.images, &cmd.files, move |d, s| CommandResult::TicketMessagePosted { ticket_id: ticket, conversation_id: d.id, message_id: s.0, to: s.1.to, run_ids: s.1.run_ids })
    }

    #[allow(clippy::too_many_arguments)]
    fn send_in(
        &self,
        ctx: &CommandContext,
        envelope: &CommandEnvelope,
        hash: String,
        discussion: Discussion,
        text: String,
        images: &[String],
        files: &[String],
        result: impl FnOnce(&Discussion, (MessageId, Sent)) -> CommandResult + Send + 'static,
    ) -> Result<Committed, AppError> {
        if let Some(why) = discussion.read_only() {
            return Err(AppError::new(ErrorCode::ChannelArchived, why));
        }
        let sources = crate::conversation::message_uploads::check(files)?;
        let images = crate::conversation::message_images::prepare(&self.store, images)?;
        // Files go into the discussion's own folder before the commit; a failed or
        // replayed command removes them again.
        let folder = self.store.root().join("workspaces").join(discussion.id.to_string());
        let uploads = if sources.is_empty() {
            Vec::new()
        } else {
            std::fs::create_dir_all(&folder).map_err(|e| AppError::new(ErrorCode::Internal, format!("Could not make the discussion's folder: {e}")))?;
            crate::conversation::message_uploads::copy(&folder, &sources)?
        };
        let committed = self.commit_message(ctx, envelope, hash, discussion, text, images, &folder, uploads.clone(), result);
        if !uploads.is_empty() && committed.as_ref().map_or(true, |c| c.receipt.replayed) {
            crate::conversation::message_uploads::remove(&folder, &uploads);
        }
        committed
    }

    #[allow(clippy::too_many_arguments)]
    fn commit_message(
        &self,
        ctx: &CommandContext,
        envelope: &CommandEnvelope,
        hash: String,
        discussion: Discussion,
        text: String,
        images: Vec<crate::conversation::message_images::PreparedImage>,
        folder: &std::path::Path,
        uploads: Vec<MessageFileView>,
        result: impl FnOnce(&Discussion, (MessageId, Sent)) -> CommandResult + Send + 'static,
    ) -> Result<Committed, AppError> {
        let text_blob = self.store.blobs().put(text.as_bytes(), "text/plain; charset=utf-8").map_err(storage_error)?;
        let message_id = MessageId::new();
        let command_id = envelope.command_id;
        let meta = ctx.metadata();
        let actor = ctx.principal_id.to_string();
        let user = ctx.user_id;
        let store = self.store.clone();
        let files: Vec<String> = uploads.iter().map(|u| format!("{} ({})", folder.join(&u.path).display(), crate::conversation::message_uploads::size_label(u.byte_length))).collect();
        self.commit(command_id, hash, move |tx| {
            // Read again inside the transaction: the ticket may have moved to Done meanwhile.
            let Some(d) = load(tx, discussion.id)? else { return Ok(Err(AppError::new(ErrorCode::NotFound, "This discussion no longer exists."))) };
            if let Some(why) = d.read_only() {
                return Ok(Err(AppError::new(ErrorCode::ChannelArchived, why)));
            }
            let correlation = command_id.to_string();
            blobs::register(tx, &text_blob)?;
            let (event_id, n) = emit(tx, Ev {
                aggregate_type: "conversation",
                aggregate_id: d.id.to_string(),
                event_type: "message.user_committed",
                actor: ActorKind::User,
                actor_id: Some(actor.clone()),
                correlation: correlation.clone(),
                payload: Some(text_blob.id.to_string()),
                metadata: meta.clone(),
                dedupe: format!("cmd:{command_id}:message"),
            })?;
            conv::insert_message_by(tx, NewMessage {
                id: message_id,
                conversation_id: d.id,
                role: MessageRole::User,
                content_blob_id: Some(text_blob.id.to_string()),
                source_event_id: event_id,
                status: MessageStatus::Complete,
                run_id: None,
                reply_to: None,
            }, user)?;
            crate::conversation::message_images::keep(tx, message_id, &images)?;
            conv::set_message_uploads(tx, message_id, &uploads)?;
            let kept: Vec<aurelius_domain::views::FileRef> = images.iter().map(|i| i.file.clone()).collect();
            let mut notices = vec![n];
            let from = board_name(tx, user)?;
            let sent = board_message(tx, &store, &meta, &correlation, &d, message_id, (from, user), &text, Shared { images: &kept, files: files.clone() }, &mut notices)?;
            Ok(Ok((result(&d, (message_id, sent)), notices)))
        })
    }
}

/// The feedback a board member gave with a decision on a ticket (approve or
/// revise the plan, send back), as their own message in the ticket's discussion
/// (MU2-05 AC3). It wakes nobody: the decision already gives it to the owner (B2).
/// Nothing when the ticket has no discussion (a store from before 0019).
pub(crate) fn decision_feedback(tx: &Tx<'_>, store: &Store, ticket: TicketId, user: UserId, meta: &serde_json::Value, command_id: &str, text: &str) -> Result<Vec<EventNotice>, StorageError> {
    let Some(d) = of_ticket(tx, ticket)? else { return Ok(Vec::new()) };
    let blob = store.blobs().put(text.as_bytes(), "text/plain; charset=utf-8")?;
    blobs::register(tx, &blob)?;
    let (event_id, notice) = emit(tx, Ev {
        aggregate_type: "conversation",
        aggregate_id: d.id.to_string(),
        event_type: "message.user_committed",
        actor: ActorKind::User,
        actor_id: meta.get("principal_id").and_then(|p| p.as_str()).map(str::to_owned),
        correlation: command_id.to_owned(),
        payload: Some(blob.id.to_string()),
        metadata: meta.clone(),
        dedupe: format!("cmd:{command_id}:feedback"),
    })?;
    conv::insert_message_by(tx, NewMessage {
        id: MessageId::new(),
        conversation_id: d.id,
        role: MessageRole::User,
        content_blob_id: Some(blob.id.to_string()),
        source_event_id: event_id,
        status: MessageStatus::Complete,
        run_id: None,
        reply_to: None,
    }, Some(user))?;
    Ok(vec![notice])
}

/// The username of the board member who wrote a message, if the connection had one.
fn board_name(conn: &rusqlite::Connection, user: Option<UserId>) -> Result<Option<String>, StorageError> {
    Ok(match user {
        Some(user) => aurelius_storage::users::get(conn, &user.to_string())?.map(|u| u.username),
        None => None,
    })
}

/// Route a board member's message in `d` (already stored as `message`): wake
/// those it names (or the one B3 names), or give it to the ticket's owner.
#[allow(clippy::too_many_arguments)]
fn board_message(tx: &Tx<'_>, store: &Store, meta: &serde_json::Value, correlation: &str, d: &Discussion, message: MessageId, (user, login): (Option<String>, Option<UserId>), text: &str, shared: Shared<'_>, notices: &mut Vec<EventNotice>) -> Result<Sent, StorageError> {
    let named = addressees(tx, d, text, None)?;
    let mut sent = Sent { to: Vec::new(), run_ids: Vec::new() };
    let mut ended = None;
    if let (Some((owner, _)), Some(t)) = (named.owner, d.ticket()) {
        match to_owner(tx, store, t, user.as_deref().unwrap_or(OWNER), text)? {
            Some(n) => {
                notices.extend(n);
                sent.to.push(owner);
            }
            None => ended = Some(main_task_ended(tx, t)?),
        }
    }
    let read = reader(store);
    let w = Wake { discussion: d, from: OWNER, board: Some(user), text, shared, context: context_text(tx, &read, d.id, Some(message))?, hop: 0, login };
    for (employee, name) in &named.awake {
        if let Some((run, n)) = wake(tx, store, meta, correlation, (*employee, name), &w)? {
            sent.to.push(*employee);
            sent.run_ids.push(run);
            notices.extend(n);
        }
    }
    // The owner's main task ended, or nobody answers: the board member hears why, in the discussion.
    if let Some(hint) = ended {
        notices.push(nobody_line(tx, d.id, &hint)?);
    } else if sent.to.is_empty() {
        notices.push(nobody_line(tx, d.id, &nobody_answers(d, &named.paused))?);
    }
    Ok(sent)
}

/// The discussion a run woke, if one did.
fn waking_discussion(conn: &rusqlite::Connection, run: RunId) -> Result<Option<Discussion>, StorageError> {
    match conv::group_run(conn, run)? {
        Some(marking) => load(conn, marking.conversation_id),
        None => Ok(None),
    }
}

/// The ticket of a run's task (a main task or a delegation below it).
fn run_ticket(conn: &rusqlite::Connection, run: RunId) -> Result<Option<TicketId>, StorageError> {
    let ticket: Option<Option<String>> = conn
        .query_row("SELECT t.ticket_id FROM runs r JOIN tasks t ON t.id = r.task_id WHERE r.id = ?1", [run.to_string()], |r| r.get(0))
        .optional()?;
    Ok(ticket.flatten().and_then(|t| TicketId::parse(&t).ok()))
}

/// The project whose context a run a discussion woke answers in (MU2-08 AC4):
/// a project's channel's, or the project of a ticket's discussion; none for the
/// General channel (the company's context) or a run no discussion woke.
pub(crate) fn run_channel_project(conn: &rusqlite::Connection, run: RunId) -> Result<Option<(ProjectId, String)>, StorageError> {
    Ok(match waking_discussion(conn, run)?.map(|d| d.place) {
        Some(Place::Project { project, name, .. }) => Some((project, name)),
        Some(Place::Ticket(t)) => crate::queries_project_name(conn, t.project).map(|name| (t.project, name)),
        _ => None,
    })
}

/// The ticket's discussion of a run: of the ticket of its task, else the one
/// that woke it. `read_discussion` and `post_to_discussion` work only there.
pub(crate) fn run_discussion(conn: &rusqlite::Connection, run: RunId) -> Result<Option<Discussion>, StorageError> {
    if let Some(ticket) = run_ticket(conn, run)? {
        if let Some(d) = of_ticket(conn, ticket)? {
            return Ok(Some(d));
        }
    }
    Ok(waking_discussion(conn, run)?.filter(|d| d.ticket().is_some()))
}

/// The ticket's discussion a run starts with, before its first turn: the one of
/// the ticket of its task (`run_discussion`). Also whether it gets the tools.
pub(crate) fn conversation_discussion(conn: &rusqlite::Connection, conversation: ConversationId) -> Result<Option<Discussion>, StorageError> {
    let ticket: Option<Option<String>> = conn.query_row("SELECT ticket_id FROM tasks WHERE conversation_id = ?1 ORDER BY created_at_ms DESC LIMIT 1", [conversation.to_string()], |r| r.get(0)).optional()?;
    match ticket.flatten().and_then(|t| TicketId::parse(&t).ok()) {
        Some(ticket) => of_ticket(conn, ticket),
        None => Ok(None),
    }
}

/// The channel a run's `post_to_group_chat` / `read_group_chat` means: the one
/// named (`General`, a project's name or a channel id), else the channel that
/// woke the run, else its project's (a ticket's that woke it), else the general one. Refused for an
/// unknown channel.
pub(crate) fn run_channel(conn: &rusqlite::Connection, run: RunId, named: Option<&str>) -> Result<Result<Discussion, String>, StorageError> {
    if let Some(named) = named.map(str::trim).filter(|n| !n.is_empty()) {
        let bare = named.trim_start_matches('#');
        if ["general", "algemeen", "team chat", "group chat"].iter().any(|g| bare.eq_ignore_ascii_case(g)) {
            return Ok(general(conn)?.ok_or_else(|| "There is no group chat.".to_owned()));
        }
        if let Ok(id) = ConversationId::parse(bare) {
            if let Some(d) = load(conn, id)?.filter(|d| d.ticket().is_none()) {
                return Ok(Ok(d));
            }
        }
        let project: Option<String> = conn.query_row("SELECT id FROM projects WHERE name = ?1 COLLATE NOCASE ORDER BY created_at_ms LIMIT 1", [bare], |r| r.get(0)).optional()?;
        if let Some(d) = match project.and_then(|p| ProjectId::parse(&p).ok()) {
            Some(project) => project_channel(conn, project)?,
            None => None,
        } {
            return Ok(Ok(d));
        }
        let mut known = vec!["General".to_owned()];
        let mut stmt = conn.prepare("SELECT p.name FROM conversations c JOIN projects p ON p.id = c.project_id WHERE c.kind = 'group' ORDER BY p.name")?;
        known.extend(stmt.query_map([], |r| r.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?);
        return Ok(Err(format!("There is no channel “{named}”. The channels are: {}.", known.join(", "))));
    }
    // Woken by a ticket's discussion: that ticket's project.
    let mut project = None;
    match waking_discussion(conn, run)? {
        Some(d) if d.ticket().is_none() => return Ok(Ok(d)),
        Some(d) => project = d.ticket().map(|t| t.project),
        None => {}
    }
    if let Some(project) = project.or(crate::task::task_runs::run_owner(conn, run)?.and_then(|o| o.project)) {
        if let Some(d) = project_channel(conn, project)? {
            return Ok(Ok(d));
        }
    }
    Ok(general(conn)?.ok_or_else(|| "There is no group chat.".to_owned()))
}

/// An employee writes in a discussion during a run (`post_to_group_chat`,
/// `post_to_discussion`). It wakes only the colleagues it names (never itself),
/// only up to `MAX_HOPS` hops after the board member's message that started the
/// exchange, and only a few times per run and per hour over every discussion
/// (`WAKING_POSTS_PER_RUN`, `WAKING_POSTS_PER_WINDOW`). A refused mention leaves
/// a system line in the discussion and is named in the tool's answer.
pub(crate) fn post_from_run(tx: &Tx<'_>, store: &Store, run: RunId, d: &Discussion, message: &str) -> Result<(bool, String, Vec<EventNotice>), StorageError> {
    let label = d.label();
    let Some(me) = crate::task::task_runs::run_owner(tx, run)?.and_then(|o| o.employee) else {
        return Ok((false, format!("Only an employee can post in {label}."), vec![]));
    };
    let text = message.trim();
    if text.is_empty() {
        return Ok((false, "Write a message to post.".into(), vec![]));
    }
    if text.chars().count() > POST_MAX {
        return Ok((false, format!("Too long for {label} (at most {POST_MAX} characters); post a short summary."), vec![]));
    }
    if let Some(why) = d.read_only() {
        return Ok((false, why, vec![]));
    }
    let Some(name) = employees::load_employee(tx, me)?.map(|e| e.name) else { return Ok((false, format!("Only an employee can post in {label}."), vec![])) };
    // The hop it would wake colleagues at: one more than its own run's, or 1 from elsewhere (a task).
    let hop = conv::group_run(tx, run)?.map_or(1, |g| g.hop + 1);
    let mut named = addressees(tx, d, text, Some(me))?;
    // Only those `wake` can wake (they have a main chat) count, also for the budget.
    let mut reachable = Vec::new();
    for (employee, name) in std::mem::take(&mut named.awake) {
        if employees::main_chat(tx, employee)?.is_some() {
            reachable.push((employee, name));
        }
    }
    named.awake = reachable;
    let names = named.awake.iter().map(|(_, n)| n.as_str()).collect::<Vec<_>>().join(", ");
    let refused = if named.awake.is_empty() {
        None
    } else if hop > MAX_HOPS {
        Some(format!("{names} will not be woken: colleagues already answered each other {MAX_HOPS} times after the board member's message; wait for a board member."))
    } else {
        // One budget over every channel and ticket together (B2), so a loop through three
        // discussions is no longer than one through a single one.
        let (of_run, of_mine) = aurelius_storage::discussions::waking_posts(tx, run, me, aurelius_storage::now_ms() - WAKE_WINDOW_MS)?;
        if of_run >= WAKING_POSTS_PER_RUN {
            Some(format!("{names} will not be woken: you already brought in colleagues {WAKING_POSTS_PER_RUN} times in this run; wait for their answers."))
        } else if of_mine >= WAKING_POSTS_PER_WINDOW {
            Some(format!("{names} will not be woken: you already brought in colleagues {WAKING_POSTS_PER_WINDOW} times in the last hour (in all channels and tickets together); wait for them or a board member."))
        } else {
            None
        }
    };
    let woke = if refused.is_none() { named.awake.len() } else { 0 };
    let posted = MessageId::new();
    let mut notices = vec![post_in(tx, store, d.id, posted, GROUP_KIND, &name, text, serde_json::json!({ "run_id": run, "hop": hop, "employee_id": me, "woke": woke }))?];
    let paused = match named.paused.as_slice() {
        [] => String::new(),
        [one] => format!(" {one} is paused and will not answer."),
        many => format!(" {} are paused and will not answer.", many.join(", ")),
    };
    let posted_in = format!("Posted in {label}.");
    if let Some(refused) = refused {
        notices.push(limit_line(tx, d.id, &names)?);
        return Ok((true, format!("{posted_in} Limit reached: {refused}{paused}"), notices));
    }
    if named.awake.is_empty() {
        return Ok((true, format!("{posted_in}{paused}"), notices));
    }
    let meta = serde_json::json!({ "run_id": run, "group_post": posted });
    let read = reader(store);
    // The login of hop 0 goes on with every colleague it wakes (its own run's).
    let login = crate::run::login::of_run(tx, run)?;
    let w = Wake { discussion: d, from: &name, board: None, text, shared: Shared::default(), context: context_text(tx, &read, d.id, Some(posted))?, hop, login };
    let mut woken = Vec::new();
    for (employee, colleague) in &named.awake {
        if let Some((_, n)) = wake(tx, store, &meta, &run.to_string(), (*employee, colleague), &w)? {
            woken.push(colleague.as_str());
            notices.extend(n);
        }
    }
    Ok((true, format!("{posted_in} {} will answer there.{paused}", woken.join(", ")), notices))
}

/// `read_group_chat` / `read_discussion`: the latest `last` messages of a
/// discussion (default and most 50), oldest first, one JSON object per message.
pub(crate) fn read_text(conn: &rusqlite::Connection, store: &Store, d: &Discussion, last: Option<u32>) -> Result<String, StorageError> {
    let read = reader(store);
    let lines = discussion_json(conn, &read, d.id, None, last.unwrap_or(50).clamp(1, 50), 2_000)?;
    let label = d.label();
    let mut head = label[..1].to_uppercase();
    head.push_str(&label[1..]);
    if lines.is_empty() {
        return Ok(format!("{head} is empty."));
    }
    let total = conv::group_message_count(conn, d.id)?;
    let read_only = d.read_only().map(|why| format!("\n\n{why}")).unwrap_or_default();
    Ok(format!("{head}, latest {} of {total} messages (oldest first, one JSON object per message):\n\n[\n{}\n]{read_only}", lines.len(), lines.join(",\n")))
}

impl CommandService {
    /// A notice of the server to every board member: a system line in the
    /// general channel of the Team chat (a vault key export, a vault name runs
    /// may no longer have). Nothing when there is no Team chat (yet).
    pub fn team_notice(&self, text: &str) -> Result<Vec<EventNotice>, AppError> {
        let text = text.to_owned();
        self.store
            .write(move |tx| {
                let Some(d) = general(tx)? else { return Ok(Vec::new()) };
                Ok(vec![system_line(tx, d.id, &SystemLine::new(SystemLineKind::Limit), Some(&text))?])
            })
            .map_err(storage_error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn place(column: ColumnKind) -> TicketPlace {
        TicketPlace { id: TicketId::new(), number: 1, title: "t".into(), project: ProjectId::new(), column, owner: Some(EmployeeId::new()), root_task: None, archived: false }
    }

    #[test]
    fn a_word_in_testing_goes_to_the_owner_as_in_review() {
        assert!(owner_answers(&place(ColumnKind::Testing)));
        assert!(owner_answers(&place(ColumnKind::Review)));
        assert!(!owner_answers(&place(ColumnKind::Backlog)) && !owner_answers(&place(ColumnKind::Done)));
        assert!(!owner_answers(&TicketPlace { owner: None, ..place(ColumnKind::Testing) }));
    }
}
