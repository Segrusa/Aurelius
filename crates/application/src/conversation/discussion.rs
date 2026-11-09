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
//!   own: the main task's conversation answers it); else nobody, with a hint. What
//!   an employee writes wakes nobody: an @Name there only refers to someone. Handing
//!   work over is the explicit `hand_off` (bridge tool), a message in the discussion
//!   ("Flux handed over to Bo" with the handoff under it) and an inbox item at its hop.
//! * Hops (R4): a board member's message is 0, what a run writes one more than the
//!   run's hop, a handoff wakes the next run at its own (`next_hop`). A board member
//!   writing in the discussion resets it. Editing or deleting wakes nobody and adds none.
//! * Loop limits (RULE 08), the same constants for every discussion and counted
//!   over all of them together: at most `group::hop_limit` hops after one message of
//!   a board member, `WAKING_POSTS_PER_RUN` handoffs per run and `WAKING_POSTS_PER_WINDOW`
//!   per employee per hour. A handoff the limits refuse leaves a system line
//!   ("Limit reached: X was not woken."). System lines and copied answers never wake anyone.
//! * Held handoffs (R5): a handoff from an employee that still has unread items in
//!   the thread waits, with a line that says so (`system_action::HANDOFF_HELD`), and goes
//!   out unchanged once that employee has caught up (`release_handoffs`).
//! * How: the message, with the latest messages of the discussion (a JSON list,
//!   so no message can pass for another or for a board member's), goes to each
//!   of them as an update (`[Group chat · from …]`) in their own conversation for
//!   that ticket or channel (R6, `employee_threads`), where a normal run answers it. A woken employee that is not the ticket's owner
//!   only answers there: the ticket's column and main task stay as they are.
//! * What a woken employee reads (P2, P3): the lead of the discussion, and unless it
//!   is the lead or the only employee there, only the messages addressed to it and
//!   its own, with how many others there are (`ThreadView`). Clients show everything.
//! * The answer: copied to the discussion as the employee's message when the run
//!   ends (`mirror_answer`), streamed there live under that message's id meanwhile.
//! * A ticket on Done (or archived) and the channel of an archived project are
//!   read-only (B7).

use crate::run::events::{Ev, emit};
use crate::run::queue::queue_run_marked;
use crate::ticket_follow::Cause as FollowCause;
use crate::{CommandContext, CommandService, Committed, domain_error, storage_error};
use aurelius_domain::autonomy::AgentMode;
use aurelius_domain::conversation::{MessageRole, MessageStatus};
use aurelius_domain::group_chat::{self as group, EditRefusal, CONTEXT_CHARS, CONTEXT_MESSAGES, GROUP_KIND, OWNER, POST_MAX, WAKE_WINDOW_MS, WAKING_POSTS_PER_RUN, WAKING_POSTS_PER_WINDOW};
use aurelius_domain::ticket::ColumnKind;
use aurelius_domain::views::{MessageFileView, SystemLine, SystemLineKind, system_action};
use aurelius_domain::{ActorKind, AppError, CommandEnvelope, CommandResult, ConversationId, EmployeeId, ErrorCode, EventId, EventNotice, MessageId, ProjectId, RunId, TaskId, TicketId, UserId};
use aurelius_storage::conversations::{self as conv, GroupRun, NewMessage};
use aurelius_storage::{StorageError, Store, Tx, blobs, employees};
use rusqlite::OptionalExtension;

/// The `subject` of a row about a mention in the general channel.
const GENERAL_SUBJECT: &str = "#general";
/// Why a reply is refused whose `thread_parent` is not a message of the discussion.
const THREAD_NOT_HERE: &str = "The message this replies to is not in this channel.";
/// Why a reply to a reply is refused: a thread has one level (no nested threads).
const THREAD_NESTED: &str = "This message is a reply itself: answer in the thread of the message it replies to.";
/// Why a reaction is refused on a message that is in no channel or ticket's discussion.
const REACTION_NOT_HERE: &str = "Only a message of a channel or a ticket's discussion can get a reaction.";

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
pub(crate) fn team(conn: &rusqlite::Connection) -> Result<Vec<(EmployeeId, String, bool)>, StorageError> {
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
    Ok(aurelius_storage::management_mode::management(conn)?.into_iter().find(|m| m.is_ceo).map(|m| m.id))
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

/// Who answers a board member's `text` in `d`: the active employees it names;
/// without a name, the one B3 names. `owner`: a board member's message goes to the ticket's owner, in its main
/// task (also when it names the owner). `paused`: those it would have woken
/// but who are paused.
#[derive(Default)]
struct Addressees {
    awake: Vec<(EmployeeId, String)>,
    paused: Vec<String>,
    owner: Option<(EmployeeId, String)>,
}

fn addressees(conn: &rusqlite::Connection, d: &Discussion, text: &str) -> Result<Addressees, StorageError> {
    let team = team(conn)?;
    let names: Vec<(EmployeeId, String)> = team.iter().map(|(id, name, _)| (*id, name.clone())).collect();
    let mut chosen = group::mentions(text, &names);
    let mut out = Addressees::default();
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
    for (id, name, active) in chosen.into_iter().filter_map(|id| team.iter().find(|(e, _, _)| *e == id)) {
        if *active {
            out.awake.push((*id, name.clone()));
        } else {
            out.paused.push(name.clone());
        }
    }
    Ok(out)
}

/// One message of a discussion as a prompt quotes it.
struct ContextRow {
    /// The employee who wrote it; none: a board member.
    employee: Option<String>,
    /// The board member's username, when the message has one.
    board_user: Option<String>,
    /// The employee a handoff in this message was for.
    to: Option<EmployeeId>,
    text: String,
}

impl ContextRow {
    /// As one JSON object `{"from", "text"}` (text cut at `chars`): its newlines stay
    /// inside the quotes, so no message can pass for another. A board member's names
    /// them and says it is one (an employee may have the same name).
    fn json(&self, chars: usize) -> String {
        let text = group::shortened(&self.text, chars);
        match &self.board_user {
            Some(user) => serde_json::json!({ "from": user, "board_member": true, "text": text }),
            None => serde_json::json!({ "from": self.employee.as_deref().unwrap_or(OWNER), "text": text }),
        }
        .to_string()
    }
}

/// The latest `limit` messages of a discussion before `before` (all when none),
/// oldest first. `thread`: those of that thread (its root first) instead of the channel's own.
fn context_rows(conn: &rusqlite::Connection, read: &dyn Fn(&str) -> Option<String>, id: ConversationId, thread: Option<MessageId>, before: Option<MessageId>, limit: u32) -> Result<Vec<ContextRow>, StorageError> {
    let rows = aurelius_storage::discussions::latest_in(conn, id, thread, before, limit)?;
    Ok(rows
        .into_iter()
        .rev()
        .filter_map(|line| {
            let text = read(&line.storage_key)?;
            if text.trim().is_empty() {
                return None;
            }
            let board_user = match (&line.author, line.author_user) {
                (None, Some((_, user))) => Some(user),
                _ => None,
            };
            Some(ContextRow { employee: line.author, board_user, to: line.handoff_to, text })
        })
        .collect())
}

/// The latest `limit` messages of a discussion before `before` (all when none),
/// oldest first, each as one JSON object (text cut at `chars`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn discussion_json(conn: &rusqlite::Connection, read: &dyn Fn(&str) -> Option<String>, id: ConversationId, thread: Option<MessageId>, before: Option<MessageId>, limit: u32, chars: usize) -> Result<Vec<String>, StorageError> {
    Ok(context_rows(conn, read, id, thread, before, limit)?.iter().map(|row| row.json(chars)).collect())
}

/// Who leads a discussion (P2): whoever sees all of it and is the fallback for a handoff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Lead {
    BoardMember(String),
    Employee(EmployeeId, String),
}

/// The lead of `d`. On a ticket its responsible board member (an active one), else
/// its owner; in a project's channel the project's dispatcher, else the CEO; in the
/// general channel the CEO. None when there is nobody.
pub(crate) fn lead_of(conn: &rusqlite::Connection, read: &dyn Fn(&str) -> Option<String>, d: &Discussion) -> Result<Option<Lead>, StorageError> {
    let employee = match &d.place {
        Place::Ticket(t) => {
            let view = aurelius_storage::tickets::view(conn, t.id, read)?;
            if let Some(user) = view.filter(|v| !v.responsible_disabled).and_then(|v| v.responsible_username) {
                return Ok(Some(Lead::BoardMember(user)));
            }
            t.owner
        }
        Place::General => ceo(conn)?,
        Place::Project { project, .. } => match aurelius_storage::company::dispatcher_for(conn, *project)? {
            Some(dispatcher) => Some(dispatcher),
            None => ceo(conn)?,
        },
    };
    employee.map(|id| Ok(Lead::Employee(id, employee_name(conn, id)?))).transpose()
}

/// How many of a discussion's latest messages the scoped view looks through (P3).
const VIEW_WINDOW: u32 = 60;

/// What a prompt quotes of a discussion for one employee.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct ThreadContext {
    /// A JSON list, one object per message; empty when there is nothing to quote.
    pub json: String,
    /// Says the view is scoped, how much it leaves out and which tool reads that.
    pub note: Option<String>,
}

/// A discussion as the prompts of its employees see it: the latest messages, its
/// lead, and who is who. The clients always show all of it; only a prompt is scoped.
pub(crate) struct ThreadView {
    rows: Vec<ContextRow>,
    team: Vec<(EmployeeId, String)>,
    lead: Option<Lead>,
    owner: Option<EmployeeId>,
    /// The tool that reads what the scoped view leaves out.
    tool: &'static str,
}

impl ThreadView {
    /// The messages of `d` (of its `thread`, when given) before `before` (all when none).
    pub(crate) fn read(conn: &rusqlite::Connection, read: &dyn Fn(&str) -> Option<String>, d: &Discussion, thread: Option<MessageId>, before: Option<MessageId>) -> Result<Self, StorageError> {
        Ok(ThreadView {
            rows: context_rows(conn, read, d.id, thread, before, VIEW_WINDOW)?,
            team: team(conn)?.into_iter().map(|(id, name, _)| (id, name)).collect(),
            lead: lead_of(conn, read, d)?,
            owner: d.ticket().and_then(|t| t.owner),
            tool: if d.ticket().is_some() { "read_discussion" } else { "read_group_chat" },
        })
    }

    /// The whole view: the latest messages, as a board member reads them.
    fn all(&self) -> Vec<usize> {
        (0..self.rows.len()).collect()
    }

    /// What the prompt of `me` quotes (P3): a non-lead that is not alone gets the
    /// messages addressed to it and its own, and hears how many others there are.
    pub(crate) fn for_employee(&self, me: EmployeeId) -> ThreadContext {
        let name = self.team.iter().find(|(id, _)| *id == me).map_or("", |(_, name)| name.as_str());
        let lines: Vec<group::ViewLine<'_>> = self.rows.iter().map(|row| group::ViewLine { employee: row.employee.as_deref(), to: row.to, text: &row.text }).collect();
        let lead = matches!(&self.lead, Some(Lead::Employee(id, _)) if *id == me);
        let Some(shown) = group::scoped_view(&lines, (me, name), &self.team, lead, self.owner == Some(me)) else { return self.quoted(&self.all(), None) };
        let hidden = self.rows.len() - shown.len();
        let note = (hidden > 0).then(|| {
            let count = if self.rows.len() as u32 >= VIEW_WINDOW { format!("{hidden} or more") } else { hidden.to_string() };
            format!("Scoped view: only the messages addressed to you and your own are shown. Other messages in this thread, not shown here: {count}. {} reads them.", self.tool)
        });
        self.quoted(&shown, note)
    }

    /// The latest of the messages at `indices`, for a prompt.
    fn quoted(&self, indices: &[usize], note: Option<String>) -> ThreadContext {
        let start = indices.len().saturating_sub(CONTEXT_MESSAGES);
        let lines: Vec<String> = indices[start..].iter().map(|i| self.rows[*i].json(CONTEXT_CHARS)).collect();
        ThreadContext { json: if lines.is_empty() { String::new() } else { format!("[\n{}\n]", lines.join(",\n")) }, note }
    }

    /// Everything, with nothing left out: for a run that has no tools to read the rest,
    /// or that is no employee's.
    pub(crate) fn whole(&self) -> ThreadContext {
        self.quoted(&self.all(), None)
    }

    /// The sentence that tells `me` who leads this thread.
    pub(crate) fn lead_line(&self, me: EmployeeId) -> Option<String> {
        Some(match self.lead.as_ref()? {
            Lead::Employee(id, _) if *id == me => "You are the lead of this thread: you see all of it and decide who does what next.".to_owned(),
            Lead::Employee(_, name) => format!("The lead of this thread is @{name}."),
            Lead::BoardMember(user) => format!("The lead of this thread is the board member @{user}."),
        })
    }
}

fn reader(store: &Store) -> impl Fn(&str) -> Option<String> + '_ {
    |key: &str| store.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok())
}

/// What `change_message` does to the message.
enum Change {
    /// Replace the text by this one (already redacted).
    Edit(String),
    Delete,
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
/// under, for an answer), in the thread of `thread` when given.
#[allow(clippy::too_many_arguments)]
fn post_in(tx: &Tx<'_>, store: &Store, id: ConversationId, thread: Option<MessageId>, message: MessageId, kind: &str, from: &str, text: &str, extra: serde_json::Value) -> Result<EventNotice, StorageError> {
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
        metadata: with_mentions(tx, id, text, serde_json::json!({ "task_id": null, "kind": kind, "from": from, "message_id": message }))?,
        dedupe: format!("group-post:{message}"),
    })?;
    let mut meta = serde_json::json!({ "kind": kind, "from": from });
    if let (Some(m), Some(e)) = (meta.as_object_mut(), extra.as_object()) {
        m.extend(e.clone());
    }
    conv::insert_update(tx, message, id, &blob.id.to_string(), event, &meta)?;
    if let Some(root) = thread {
        aurelius_storage::threads::set_thread_parent(tx, message, root)?;
    }
    aurelius_storage::timeline::notify_message(tx, notice.seq, message)?;
    Ok(notice)
}

/// `metadata` of a message event, with who `text` names when it names board members,
/// where it was written for the row (`ticket_id`, or the channel as `subject`).
/// `notify` reads this: the text stays in its blob and never reaches a row or a push.
fn with_mentions(conn: &rusqlite::Connection, id: ConversationId, text: &str, mut metadata: serde_json::Value) -> Result<serde_json::Value, StorageError> {
    let named = crate::ticket_follow::mentioned_board_members(conn, text)?;
    if named.is_empty() {
        return Ok(metadata);
    }
    let Some(d) = load(conn, id)? else { return Ok(metadata) };
    metadata["mentioned_users"] = serde_json::json!(named);
    match &d.place {
        Place::General => metadata["subject"] = serde_json::json!(GENERAL_SUBJECT),
        // Stored and pushed, so through the redactor like everything else that leaves.
        Place::Project { name, .. } => metadata["subject"] = serde_json::json!(aurelius_runtime::redact::string(format!("#{name}"))),
        Place::Ticket(t) => metadata["ticket_id"] = serde_json::json!(t.id),
    }
    Ok(metadata)
}

/// A system line with its own words (`text`; none: `SystemLine::sentence`) in
/// any conversation, in the transaction of what caused it (MU2-03). It wakes
/// nobody and is never part of what a woken employee reads (`latest_messages`
/// takes only messages); a discussion on Done takes it too (B7 is for people and tools).
pub(crate) fn system_line(tx: &Tx<'_>, id: ConversationId, line: &SystemLine, text: Option<&str>) -> Result<EventNotice, StorageError> {
    system_line_in(tx, id, None, line, text)
}

/// `system_line`, in the thread of `thread` when given (a hint for a reply there).
pub(crate) fn system_line_in(tx: &Tx<'_>, id: ConversationId, thread: Option<MessageId>, line: &SystemLine, text: Option<&str>) -> Result<EventNotice, StorageError> {
    Ok(system_line_event(tx, id, thread, line, text)?.1)
}

/// `system_line_in`, with the event it became (what a held handoff refers to).
fn system_line_event(tx: &Tx<'_>, id: ConversationId, thread: Option<MessageId>, line: &SystemLine, text: Option<&str>) -> Result<(EventId, EventNotice), StorageError> {
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
    if let Some(root) = thread {
        aurelius_storage::threads::set_thread_parent(tx, message, root)?;
    }
    Ok((event, notice))
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

/// "Limit reached: X was not woken." for a mention the loop limits refused (in its thread).
fn limit_line(tx: &Tx<'_>, id: ConversationId, thread: Option<MessageId>, names: &str) -> Result<EventNotice, StorageError> {
    let mut line = SystemLine::new(SystemLineKind::Limit);
    line.to = Some(names.to_owned());
    system_line_in(tx, id, thread, &line, None)
}

/// What a message brings along for the employees: its images (shown to
/// them) and the files the board member shared (named by their path on the server).
#[derive(Default)]
struct Shared<'a> {
    images: &'a [aurelius_domain::views::FileRef],
    files: Vec<String>,
}

/// One message that wakes employees: where and who wrote it, its text, what it
/// brings along and the messages before it (`view`).
struct Wake<'a> {
    discussion: &'a Discussion,
    from: &'a str,
    /// A board member wrote it (the update itself is theirs; a colleague's is
    /// quoted), with their username when the connection had one.
    board: Option<Option<String>>,
    text: &'a str,
    shared: Shared<'a>,
    /// The discussion before this message, as each employee's prompt quotes it.
    view: ThreadView,
    hop: u32,
    /// The board member of hop 0, whose Claude login the woken runs use.
    login: Option<UserId>,
    /// The message was a reply in the thread of this one: the answer goes there too.
    thread: Option<MessageId>,
    /// What stands in front of a colleague's text, in place of "handed this over" (`schedule_message`).
    lead_in: Option<String>,
}

/// The thread key of a discussion (`employee_threads`, `employee_inbox`).
pub(crate) fn thread_key(d: &Discussion) -> String {
    match d.ticket() {
        Some(t) => crate::run::inbox::ticket_thread(t.id),
        None => crate::run::inbox::channel_thread(d.id),
    }
}

/// Wake `employee` for the message `w`: the message and the latest messages of
/// the discussion go to its main chat as an update, and a run there answers it.
/// Its answer comes back to the discussion (`mirror_answer`).
fn wake(tx: &Tx<'_>, store: &Store, meta: &serde_json::Value, correlation: &str, (employee, name): (EmployeeId, &str), w: &Wake<'_>) -> Result<Option<(RunId, Vec<EventNotice>)>, StorageError> {
    // Its own conversation for this thread (R6), with its own session; the main chat stays its own.
    let Some(chat) = aurelius_storage::inbox::thread_conversation(tx, employee, &thread_key(w.discussion), &w.discussion.label())? else { return Ok(None) };
    let (d, from, shared, hop) = (w.discussion, w.from, &w.shared, w.hop);
    let mut body = wake_body(w, employee, w.view.for_employee(employee));
    let place = if w.thread.is_some() { format!("that thread of {}", d.label()) } else { d.label() };
    body.push_str(&format!("\n\n---\n{name}: your answer is posted in {place} under your name. Keep it short."));
    let (update, posted) = crate::task::task_threads::post_note(tx, store, chat, crate::task::task_threads::Speaker::Update { kind: GROUP_KIND, from }, &body)?;
    // The images of the message come along, so the employee sees them (see `run::plan`).
    if !shared.images.is_empty() {
        conv::set_message_files(tx, update, shared.images)?;
    }
    // The employees of the Team chat always work in Auto.
    conv::set_agent_mode(tx, chat, AgentMode::Auto)?;
    // Only a board member waits for the answer; a colleague's mention would just add lines.
    let busy = if w.board.is_some() { busy_notices(tx, store, meta, correlation, (employee, name), w)? } else { Vec::new() };
    let marking = GroupRun { conversation_id: d.id, message_id: MessageId::new(), hop, mode: None, login_user: w.login, thread_parent: w.thread };
    let (run, queued) = queue_run_marked(tx, meta, correlation, chat, update, Some(marking))?;
    Ok(Some((run, std::iter::once(posted).chain(busy).chain(queued).collect())))
}

/// The text of an update that wakes `employee` for `w`: the heading and the message
/// (the board member's own words, or a colleague's quoted), the files, the earlier
/// messages (`context`), the lead, and for a ticket the employee does not own, what
/// not to take over. What tells it where its answer goes is the caller's.
fn wake_body(w: &Wake<'_>, employee: EmployeeId, context: ThreadContext) -> String {
    let (d, from, shared) = (w.discussion, w.from, &w.shared);
    let mut body = String::new();
    if let Some(heading) = d.heading() {
        body.push_str(&heading);
        body.push_str("\n\n");
    }
    if w.thread.is_some() {
        body.push_str(&format!("This is a reply in a thread of {} (the context below is that thread, its first message first).\n\n", d.label()));
    }
    if !shared.files.is_empty() {
        body.push_str(&format!("{from} shared these files in {}: {}\n\n", d.label(), shared.files.join(", ")));
    }
    // A board member's message is the update itself (its label names them); a colleague's
    // handoff is quoted as JSON, so it cannot look like a board member's or the context.
    if let Some(user) = &w.board {
        if let Some(user) = user {
            body.push_str(&format!("From the board member {user}:\n"));
        }
        body.push_str(w.text.trim());
    } else {
        let lead_in = w.lead_in.clone().unwrap_or_else(|| format!("{from} handed this over to you"));
        body.push_str(&format!("{lead_in} (JSON-quoted): {}", serde_json::Value::from(w.text.trim())));
    }
    if !context.json.is_empty() {
        body.push_str(&format!("\n\n---\nEarlier in {} (oldest first, one JSON object per message):\n{}", d.label(), context.json));
    }
    if let Some(note) = &context.note {
        body.push_str(&format!("\n\n---\n{note}"));
    }
    if let Some(lead) = w.view.lead_line(employee) {
        body.push_str(&format!("\n\n---\n{lead}"));
    }
    if let Some(t) = d.ticket().filter(|t| t.owner != Some(employee)) {
        body.push_str(&format!(
            "\n\n---\nYou are not the owner of ticket #{}: answer the question, but do not take over its work. To give work, delegate it or propose a new ticket to a board member.",
            t.number
        ));
    }
    body
}

/// What separates a copied message from the line that says where to answer it.
const OWNER_FOOTER_MARK: &str = "\n\n---\n";
/// How much of a question typed in the task's conversation `owner_answer` quotes.
const OWNER_QUOTE_CHARS: usize = 200;

/// A board member's message without `@Name` for the ticket's owner (B3): an
/// update in the main task's conversation. Only while the main task runs or is
/// in Review does a run answer it: the one already queued there, else one queued
/// behind the run that goes (its end counts for the task, as an answer in the
/// task's own conversation does: in Review the answer comes back here as the
/// owner's message, `owner_answer`). Otherwise nothing runs and the owner reads it when the
/// task goes on: before its start (it starts with the discussion as context),
/// while it waits for its turn (Queued: `start_task_conversation` goes on from
/// its conversation), and while it waits (Waiting: for the plan's decision, a
/// colleague, the usage limit or after a stop; a message here does not resume
/// it, unlike an answer in the task's own conversation, so a sequential owner
/// never jumps its line). `None`: it does not get there, the main task has ended
/// (e.g. failed in Doing, B6: Start makes a new one), with the hint for the discussion.
///
/// The update names the discussion message it copies (`from_message` in its meta):
/// an edit or a delete of that message reaches the copy (`change_message`).
///
/// A reply in a thread says so, and the update keeps the discussion and the
/// thread (`thread_parent` in its meta): what the owner posts with
/// `post_to_discussion` after it goes into that thread (`run_thread`).
fn to_owner(tx: &Tx<'_>, store: &Store, d: &Discussion, t: &TicketPlace, from: &str, text: &str, thread: Option<MessageId>, source: MessageId, busy_in: &mut Option<ConversationId>) -> Result<Option<Vec<EventNotice>>, StorageError> {
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
    let place = if thread.is_some() { "a thread in the discussion" } else { "the discussion" };
    let body = format!("{}{OWNER_FOOTER_MARK}From {place} of ticket #{}. Answer the board member there with post_to_discussion.", text.trim(), t.number);
    let extra = serde_json::json!({ "discussion": d.id, "thread_parent": thread, "from_message": source });
    let (message, posted) = crate::task::task_threads::post_returning_with(tx, store, conversation, root, crate::task::task_threads::Speaker::Update { kind: "message", from }, &body, extra)?;
    let mut notices = vec![posted];
    // A main task that waits for this answer to its question goes on with a run of its own.
    let answers_question = row.status == TaskStatus::Waiting && aurelius_storage::tasks::waiting_cause(tx, root)?.as_deref() == Some("question");
    if !matches!(row.status, TaskStatus::Running | TaskStatus::Review) && !answers_question {
        return Ok(Some(notices));
    }
    // The owner is mid-run: the caller says so now, the answer comes when that run ends.
    *busy_in = Some(conversation);
    let (run, queued) = crate::run::queue::queue_run(tx, &serde_json::json!({ "task_id": root, "discussion": t.id }), &root.to_string(), conversation, message)?;
    crate::run::inbox::link_task(tx, run, root)?;
    notices.extend(queued);
    notices.extend(crate::task::task_runs::live_run_notice(tx, conversation)?);
    Ok(Some(notices))
}

/// What an edit or a delete of a discussion message does to its copy in the
/// owner's task conversation (`to_owner`), in the same transaction. A delete
/// treats the copy as the message itself: placeholder text, files and uploads
/// gone, the earlier text only as the payload of its own `message.deleted`.
/// An edit replaces the text of the copy by the new one, keeping the line that
/// tells where to answer, and marks it edited: the owner then reads one version,
/// the current one, whether it reads it now or in a run that is yet to start (a
/// copy left with the old text would contradict the discussion, and a second
/// "edited" message would make the context longer for nothing). Like the change
/// itself it wakes nobody and queues no run: the notice only refreshes clients.
fn change_owner_copy(tx: &Tx<'_>, store: &Store, copy: &conv::TaskCopy, change: &Change, placeholder: &aurelius_storage::PreparedBlob, at: i64, (user, command_id): (UserId, aurelius_domain::CommandId)) -> Result<Vec<EventNotice>, StorageError> {
    let (blob, event_type) = match change {
        Change::Delete => (placeholder.clone(), "message.deleted"),
        Change::Edit(text) => {
            let old = copy.text_key.as_deref().and_then(|k| store.blobs().read(k).ok()).and_then(|b| String::from_utf8(b).ok()).unwrap_or_default();
            let footer = old.rsplit_once(OWNER_FOOTER_MARK).map_or(String::new(), |(_, f)| format!("{OWNER_FOOTER_MARK}{f}"));
            let body = crate::task::task_threads::capped(&format!("{}{footer}", text.trim()));
            let blob = store.blobs().put(body.as_bytes(), "text/markdown; charset=utf-8")?;
            blobs::register(tx, &blob)?;
            (blob, "message.edited")
        }
    };
    let replaced = match change {
        Change::Edit(_) => conv::replace_message_text(tx, copy.message, &blob.id.to_string(), at)?,
        Change::Delete => {
            conv::clear_message_attachments(tx, copy.message)?;
            conv::mark_message_deleted(tx, copy.message, &blob.id.to_string(), at)?
        }
    };
    let Some(replaced) = replaced else { return Ok(Vec::new()) };
    let metadata = match change {
        Change::Edit(_) => serde_json::json!({ "message_id": copy.message, "by": user }),
        Change::Delete => serde_json::json!({ "message_id": copy.message, "by": user, "queued": false }),
    };
    let (_, notice) = emit(tx, Ev {
        aggregate_type: "conversation",
        aggregate_id: replaced.conversation_id.to_string(),
        event_type,
        actor: ActorKind::User,
        actor_id: Some(user.to_string()),
        correlation: command_id.to_string(),
        payload: replaced.previous_blob,
        metadata,
        dedupe: format!("cmd:{command_id}:copy:{}", copy.message),
    })?;
    Ok(vec![notice])
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

/// How long a run has gone, in words ("45s", "14m", "2h 5m").
fn run_duration(ms: i64) -> String {
    let seconds = (ms / 1000).max(0);
    match seconds {
        0..=59 => format!("{seconds}s"),
        60..=3599 => format!("{}m", seconds / 60),
        _ => format!("{}h {}m", seconds / 3600, seconds % 3600 / 60),
    }
}

/// What a board member sees at once when the message they posted waits behind a
/// run going in `conversation` (a run reads only up to its own `reply_to`): a
/// quick answer when one can start, and the busy line. Nothing when no run is
/// going there. Call it before the run for the message is queued. The line is a
/// `Limit` system line with its own words, so old clients show it as it is.
fn busy_notices(tx: &Tx<'_>, store: &Store, meta: &serde_json::Value, correlation: &str, (employee, name): (EmployeeId, &str), w: &Wake<'_>) -> Result<Vec<EventNotice>, StorageError> {
    let Some(since) = conv::employee_going_since(tx, employee)? else { return Ok(Vec::new()) };
    let for_how_long = since.map_or_else(|| "just started".to_owned(), |at| run_duration(aurelius_storage::now_ms() - at));
    let quick = quick_answer(tx, store, meta, correlation, (employee, name), w, &for_how_long)?;
    let text = match quick {
        Some(_) => format!("{name} is busy with a run ({for_how_long}): a quick answer follows now, the full answer when the run ends."),
        None => format!("{name} is busy with a run ({for_how_long}), will answer when it ends."),
    };
    let mut notices = vec![system_line_in(tx, w.discussion.id, w.thread, &SystemLine::new(SystemLineKind::Limit), Some(&text))?];
    notices.extend(quick.into_iter().flatten());
    Ok(notices)
}

/// A quick answer for a board member whose employee is busy: a text-only run in the
/// employee's hidden quick-answer conversation, so it does not wait behind the long
/// run. It reads the same update as a woken employee, answers from it and what the
/// employee knows, and its answer goes to the discussion (`mirror_answer`), waking
/// nobody. `None` when it cannot start: the hourly cost cap is reached
/// (`QUICK_ANSWER_COST_PER_HOUR_MICRO`) or the employee's previous quick answer is
/// still going, then the busy line is all there is.
fn quick_answer(tx: &Tx<'_>, store: &Store, meta: &serde_json::Value, correlation: &str, (employee, name): (EmployeeId, &str), w: &Wake<'_>, for_how_long: &str) -> Result<Option<Vec<EventNotice>>, StorageError> {
    if conv::quick_answer_cost_since(tx, aurelius_storage::now_ms() - group::WAKE_WINDOW_MS)? >= group::QUICK_ANSWER_COST_PER_HOUR_MICRO {
        return Ok(None);
    }
    let Some(chat) = conv::quick_answer_chat(tx, employee)? else { return Ok(None) };
    if !conv::unfinished_runs_in(tx, chat)?.is_empty() {
        return Ok(None);
    }
    let d = w.discussion;
    let place = if w.thread.is_some() { format!("that thread of {}", d.label()) } else { d.label() };
    // It has no tools to read what a scoped view leaves out, so it gets the discussion as it is.
    let mut body = wake_body(w, employee, w.view.whole());
    body.push_str(&format!(
        "\n\n---\n{name}: you are in the middle of a longer run (for {for_how_long}) on your own work, so this is a quick answer from the side: you have no tools and cannot see your files or that run. \
         Answer from this discussion and what you know, in a few sentences, and say plainly what you cannot see. Do not promise to do work now: your run goes on and follows up when it ends. \
         Your answer is posted in {place} under your name."
    ));
    let (update, posted) = crate::task::task_threads::post_note(tx, store, chat, crate::task::task_threads::Speaker::Update { kind: GROUP_KIND, from: w.from }, &body)?;
    if !w.shared.images.is_empty() {
        conv::set_message_files(tx, update, w.shared.images)?;
    }
    let marking = GroupRun { conversation_id: d.id, message_id: MessageId::new(), hop: w.hop, mode: None, login_user: w.login, thread_parent: w.thread };
    let (_, queued) = queue_run_marked(tx, meta, correlation, chat, update, Some(marking))?;
    Ok(Some(std::iter::once(posted).chain(queued).collect()))
}

/// A line of Aurelius that tells the board member why a message reached nobody
/// (or not the owner): a system line (`Limit`, without `to`; it wakes nobody),
/// in the thread of the message when it was a reply.
fn nobody_line(tx: &Tx<'_>, id: ConversationId, thread: Option<MessageId>, text: &str) -> Result<EventNotice, StorageError> {
    system_line_in(tx, id, thread, &SystemLine::new(SystemLineKind::Limit), Some(text))
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
    // A quick answer that failed or was cut off posts nothing: the busy line stays, the long run answers.
    let quick = conv::is_quick_answer(tx, conversation)?;
    if quick && failed.is_some() {
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
    // The long run answered while the quick one was thinking: the quick answer is stale and would be a second reply.
    if quick && conv::answered_since_quick_question(tx, marking.conversation_id, conversation, &name)? {
        return Ok(Vec::new());
    }
    let notice = post_in(tx, store, marking.conversation_id, marking.thread_parent, marking.message_id, GROUP_KIND, &name, &text, serde_json::json!({ "run_id": run, "hop": marking.hop + 1, "mode": mode, "quick": quick.then_some(true) }))?;
    if !files.is_empty() {
        conv::set_message_files(tx, marking.message_id, files)?;
    }
    let mut notices = vec![notice];
    // A quick answer wakes nobody: it only answers the board member.
    if let Some(d) = load(tx, marking.conversation_id)?.filter(|_| !quick) {
        let run = run.to_string();
        notices.extend(follow_named(tx, &d, &text, employee_cause(&run))?);
    }
    Ok(notices)
}

/// The answer of a run of a ticket's main task in Review, as the owner's message in
/// the discussion: the board member reads it there, whether the message it answers
/// came from the discussion or from the task's own conversation (then it is quoted,
/// because the discussion does not have it). Nothing when the run's last word
/// already went there itself. It names the responsible board member, who is told by that; it
/// wakes nobody.
pub(crate) fn owner_answer(tx: &Tx<'_>, store: &Store, ticket: TicketId, run: RunId, answer: &str) -> Result<Vec<EventNotice>, StorageError> {
    let Some(d) = of_ticket(tx, ticket)? else { return Ok(Vec::new()) };
    if aurelius_storage::discussions::run_said_last_in(tx, d.id, run)? {
        return Ok(Vec::new());
    }
    let read = reader(store);
    let responsible = aurelius_storage::tickets::view(tx, ticket, &read)?.and_then(|v| v.responsible_username);
    let question = aurelius_storage::conversations::typed_question_of_run(tx, run)?.and_then(|key| read(&key));
    let body = owner_answer_text(answer, question.as_deref(), responsible.as_deref());
    let me = crate::task::task_runs::run_owner(tx, run)?.and_then(|o| o.employee);
    let name = match me {
        Some(me) => employee_name(tx, me)?,
        None => "The owner".into(),
    };
    let thread = run_thread(tx, run, &d)?;
    let message = MessageId::new();
    let mut notices = vec![post_in(tx, store, d.id, thread, message, GROUP_KIND, &name, &body, serde_json::json!({ "run_id": run, "employee_id": me, "woke": 0, "hop": next_hop(tx, run, &d)? }))?];
    if let Some(answered) = conv::message_for_run(tx, run)? {
        conv::set_message_files(tx, message, &conv::message_files(tx, answered)?)?;
    }
    notices.extend(follow_named(tx, &d, &body, employee_cause(&run.to_string()))?);
    Ok(notices)
}

/// The text of `owner_answer`: the question quoted on one line, then the answer
/// that opens with @`responsible` unless it names them already.
fn owner_answer_text(answer: &str, question: Option<&str>, responsible: Option<&str>) -> String {
    let answer = match answer.trim() {
        "" => "Done.",
        text => text,
    };
    let answer = match responsible {
        Some(u) if !names(answer, u) => format!("@{u} {answer}"),
        _ => answer.to_owned(),
    };
    let answer = group::shortened(&answer, crate::task::task_threads::MAX_POST);
    match question {
        Some(q) => {
            let flat = q.split_whitespace().collect::<Vec<_>>().join(" ");
            format!("> {}\n\n{answer}", group::shortened(&flat, OWNER_QUOTE_CHARS))
        }
        None => answer,
    }
}

/// `text` mentions @`username` itself, not a longer name that starts with it.
fn names(text: &str, username: &str) -> bool {
    let (text, mention) = (text.to_lowercase(), format!("@{}", username.to_lowercase()));
    text.match_indices(&mention).any(|(at, _)| !text[at + mention.len()..].starts_with(|c: char| c.is_alphanumeric() || c == '_' || c == '-'))
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
        self.send_in(ctx, envelope, hash, (discussion, cmd.thread_parent), text, &cmd.images, &cmd.files, |d, s| CommandResult::GroupMessageSent { conversation_id: d.id, message_id: s.0, to: s.1.to, run_ids: s.1.run_ids })
    }

    /// A board member writes in a ticket's discussion (`ticket.post`).
    pub(crate) fn post_ticket_message(&self, ctx: &CommandContext, envelope: &CommandEnvelope, hash: String, cmd: &aurelius_domain::ticket::PostTicketMessage) -> Result<Committed, AppError> {
        crate::ticket::require_board_member(ctx)?;
        let text = cmd.validate().map_err(domain_error)?;
        let ticket = cmd.ticket_id;
        let discussion = self
            .store
            .read(|c| of_ticket(c, ticket))
            .map_err(storage_error)?
            .ok_or_else(|| AppError::new(ErrorCode::NotFound, format!("Ticket {ticket} has no discussion.")))?;
        self.send_in(ctx, envelope, hash, (discussion, cmd.thread_parent), text, &cmd.images, &cmd.files, move |d, s| CommandResult::TicketMessagePosted { ticket_id: ticket, conversation_id: d.id, message_id: s.0, to: s.1.to, run_ids: s.1.run_ids })
    }

    #[allow(clippy::too_many_arguments)]
    fn send_in(
        &self,
        ctx: &CommandContext,
        envelope: &CommandEnvelope,
        hash: String,
        (discussion, thread): (Discussion, Option<MessageId>),
        text: String,
        images: &[String],
        files: &[String],
        result: impl FnOnce(&Discussion, (MessageId, Sent)) -> CommandResult + Send + 'static,
    ) -> Result<Committed, AppError> {
        // A closed channel refuses a reply as it refuses a message (`channel_archived`).
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
        let committed = self.commit_message(ctx, envelope, hash, (discussion, thread), text, images, &folder, uploads.clone(), result);
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
        (discussion, thread): (Discussion, Option<MessageId>),
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
            if let Some(parent) = thread
                && let Err(err) = thread_root(tx, &d, parent)?
            {
                return Ok(Err(err));
            }
            // Whoever answers on a ticket, from wherever, needs no line in the Inbox that they were asked there.
            if let Place::Ticket(t) = &d.place {
                aurelius_storage::timeline::clear_ticket_mentions_for(tx, &t.id.to_string(), user.map(|u| u.to_string()).as_deref())?;
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
                metadata: with_mentions(tx, d.id, &text, meta.clone())?,
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
            if let Some(root) = thread {
                aurelius_storage::threads::set_thread_parent(tx, message_id, root)?;
            }
            aurelius_storage::timeline::notify_message(tx, n.seq, message_id)?;
            crate::conversation::message_images::keep(tx, message_id, &images)?;
            conv::set_message_uploads(tx, message_id, &uploads)?;
            let kept: Vec<aurelius_domain::views::FileRef> = images.iter().map(|i| i.file.clone()).collect();
            let mut notices = vec![n];
            let from = board_name(tx, user)?;
            let sent = board_message(tx, &store, &meta, &correlation, (&d, thread), message_id, (from, user), &text, Shared { images: &kept, files: files.clone() }, &mut notices)?;
            notices.extend(follow_named(tx, &d, &text, FollowCause { actor: ActorKind::User, actor_id: Some(actor.clone()), correlation: &correlation, meta: meta.clone() })?);
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
        metadata: with_mentions(tx, d.id, text, meta.clone())?,
        dedupe: format!("cmd:{command_id}:feedback"),
    })?;
    let message = MessageId::new();
    conv::insert_message_by(tx, NewMessage {
        id: message,
        conversation_id: d.id,
        role: MessageRole::User,
        content_blob_id: Some(blob.id.to_string()),
        source_event_id: event_id,
        status: MessageStatus::Complete,
        run_id: None,
        reply_to: None,
    }, Some(user))?;
    aurelius_storage::timeline::notify_message(tx, notice.seq, message)?;
    let mut notices = vec![notice];
    notices.extend(follow_named(tx, &d, text, FollowCause { actor: ActorKind::User, actor_id: meta.get("principal_id").and_then(|p| p.as_str()).map(str::to_owned), correlation: command_id, meta: meta.clone() })?);
    Ok(notices)
}

/// Who a message in a ticket's discussion names follows the ticket (US-A2); nothing in
/// a channel, whose readers do not follow anything.
fn follow_named(tx: &Tx<'_>, d: &Discussion, text: &str, cause: FollowCause<'_>) -> Result<Vec<EventNotice>, StorageError> {
    match d.ticket() {
        Some(t) => crate::ticket_follow::follow_mentioned(tx, t.id, t.project, text, &cause),
        None => Ok(Vec::new()),
    }
}

/// An employee's message during `run` caused a follow.
fn employee_cause(run: &str) -> FollowCause<'_> {
    FollowCause { actor: ActorKind::Employee, actor_id: None, correlation: run, meta: serde_json::json!({ "run_id": run }) }
}

/// The thread a reply to `parent` goes into in `d`: `parent` itself, when it is
/// a message of `d` that is not a reply (one level of threads). Validation
/// otherwise, for every client (no new error code).
fn thread_root(conn: &rusqlite::Connection, d: &Discussion, parent: MessageId) -> Result<Result<MessageId, AppError>, StorageError> {
    Ok(match aurelius_storage::threads::place(conn, parent)? {
        Some(p) if p.conversation_id == d.id && p.thread_parent.is_none() => Ok(parent),
        Some(p) if p.conversation_id == d.id => Err(AppError::new(ErrorCode::Validation, THREAD_NESTED)),
        _ => Err(AppError::new(ErrorCode::Validation, THREAD_NOT_HERE)),
    })
}

/// The thread of `d` an employee's post during `run` goes into: the one its run
/// was woken in (`GroupRun::thread_parent`); for a run in a ticket's main task,
/// the thread of the latest board member's message that reached it from `d`
/// (`to_owner`); else none, the channel itself.
pub(crate) fn run_thread(conn: &rusqlite::Connection, run: RunId, d: &Discussion) -> Result<Option<MessageId>, StorageError> {
    if let Some(marking) = conv::group_run(conn, run)?.filter(|g| g.conversation_id == d.id) {
        return Ok(marking.thread_parent);
    }
    if d.ticket().is_none() {
        return Ok(None);
    }
    let latest: Option<Option<String>> = conn
        .query_row(
            "SELECT json_extract(m.meta_json, '$.thread_parent') FROM messages m
             WHERE m.conversation_id = (SELECT conversation_id FROM runs WHERE id = ?1) AND json_extract(m.meta_json, '$.discussion') = ?2
             ORDER BY m.ordinal DESC LIMIT 1",
            [run.to_string(), d.id.to_string()],
            |r| r.get(0),
        )
        .optional()?;
    let Some(root) = latest.flatten().and_then(|r| MessageId::parse(&r).ok()) else { return Ok(None) };
    // Only a thread that is (still) one of `d`.
    Ok(thread_root(conn, d, root)?.ok())
}

/// The username of the board member who wrote a message, if the connection had one.
fn board_name(conn: &rusqlite::Connection, user: Option<UserId>) -> Result<Option<String>, StorageError> {
    Ok(match user {
        Some(user) => aurelius_storage::users::get(conn, &user.to_string())?.map(|u| u.username),
        None => None,
    })
}

/// Route a board member's message in `d` (already stored as `message`; a reply
/// in the thread of `thread` when given): wake those it names (or the one B3
/// names), or give it to the ticket's owner. Their answers go where it is.
#[allow(clippy::too_many_arguments)]
fn board_message(tx: &Tx<'_>, store: &Store, meta: &serde_json::Value, correlation: &str, (d, thread): (&Discussion, Option<MessageId>), message: MessageId, (user, login): (Option<String>, Option<UserId>), text: &str, shared: Shared<'_>, notices: &mut Vec<EventNotice>) -> Result<Sent, StorageError> {
    let named = addressees(tx, d, text)?;
    let mut sent = Sent { to: Vec::new(), run_ids: Vec::new() };
    let mut ended = None;
    let mut owner_busy = None;
    if let (Some((owner, _)), Some(t)) = (&named.owner, d.ticket()) {
        match to_owner(tx, store, d, t, user.as_deref().unwrap_or(OWNER), text, thread, message, &mut owner_busy)? {
            Some(n) => {
                notices.extend(n);
                sent.to.push(*owner);
            }
            None => ended = Some(main_task_ended(tx, t)?),
        }
    }
    let read = reader(store);
    let w = Wake { discussion: d, from: OWNER, board: Some(user), text, shared, view: ThreadView::read(tx, &read, d, thread, Some(message))?, hop: 0, login, thread, lead_in: None };
    // The owner is mid-run: the board member hears it at once, and gets a quick answer.
    if let (Some(_), Some((owner, owner_name))) = (owner_busy, &named.owner) {
        notices.extend(busy_notices(tx, store, meta, correlation, (*owner, owner_name), &w)?);
    }
    for (employee, name) in &named.awake {
        if let Some((run, n)) = wake(tx, store, meta, correlation, (*employee, name), &w)? {
            sent.to.push(*employee);
            sent.run_ids.push(run);
            notices.extend(n);
        }
    }
    // The owner's main task ended, or nobody answers: the board member hears why, in the discussion.
    if let Some(hint) = ended {
        notices.push(nobody_line(tx, d.id, thread, &hint)?);
    } else if sent.to.is_empty() {
        notices.push(nobody_line(tx, d.id, thread, &nobody_answers(d, &named.paused))?);
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

/// The thread a run hands work over in: its ticket's discussion, else the channel that woke it.
pub(crate) fn run_handoff_thread(conn: &rusqlite::Connection, run: RunId) -> Result<Option<Discussion>, StorageError> {
    Ok(match run_discussion(conn, run)? {
        Some(d) => Some(d),
        None => waking_discussion(conn, run)?,
    })
}

/// The hop of what `run` writes in `d`: one more than the run's, or 1 when a board
/// member has written in `d` since the newest item that woke it (R4).
pub(crate) fn next_hop(conn: &rusqlite::Connection, run: RunId, d: &Discussion) -> Result<u32, StorageError> {
    if let Some(since) = aurelius_storage::inbox::woken_at(conn, run)?
        && aurelius_storage::discussions::board_spoke_since(conn, d.id, since)?
    {
        return Ok(1);
    }
    Ok(aurelius_storage::inbox::run_hop(conn, run)? + 1)
}

/// What an employee's message in `d` during `run` must be: its employee, its name
/// and its text, or why it is refused.
fn employee_post(tx: &Tx<'_>, run: RunId, d: &Discussion, message: &str) -> Result<Result<(EmployeeId, String, String), String>, StorageError> {
    let label = d.label();
    let Some(me) = crate::task::task_runs::run_owner(tx, run)?.and_then(|o| o.employee) else {
        return Ok(Err(format!("Only an employee can post in {label}.")));
    };
    let text = message.trim();
    if text.is_empty() {
        return Ok(Err("Write a message to post.".into()));
    }
    if text.chars().count() > POST_MAX {
        return Ok(Err(format!("Too long for {label} (at most {POST_MAX} characters); post a short summary.")));
    }
    if let Some(why) = d.read_only() {
        return Ok(Err(why));
    }
    let Some(name) = employees::load_employee(tx, me)?.map(|e| e.name) else { return Ok(Err(format!("Only an employee can post in {label}."))) };
    Ok(Ok((me, name, text.to_owned())))
}

/// An employee writes in a discussion during a run (`post_to_group_chat`,
/// `post_to_discussion`, `post_to_ticket`). It wakes nobody: an @Name only refers to
/// a colleague, and an @username of a board member notifies them. `hand_off` is what wakes.
pub(crate) fn post_from_run(tx: &Tx<'_>, store: &Store, run: RunId, d: &Discussion, message: &str) -> Result<(bool, String, Vec<EventNotice>), StorageError> {
    let (me, name, text) = match employee_post(tx, run, d, message)? {
        Ok(post) => post,
        Err(why) => return Ok((false, why, vec![])),
    };
    let label = d.label();
    let hop = next_hop(tx, run, d)?;
    let posted = MessageId::new();
    // An answer to a reply in a thread goes into that thread.
    let thread = run_thread(tx, run, d)?;
    let mut notices = vec![post_in(tx, store, d.id, thread, posted, GROUP_KIND, &name, &text, serde_json::json!({ "run_id": run, "hop": hop, "employee_id": me, "woke": 0 }))?];
    let run_text = run.to_string();
    notices.extend(follow_named(tx, d, &text, employee_cause(&run_text))?);
    let team: Vec<(EmployeeId, String)> = team(tx)?.into_iter().map(|(id, name, _)| (id, name)).collect();
    let others: Vec<String> = group::mentions(&text, &team).into_iter().filter(|id| *id != me).filter_map(|id| team.iter().find(|(e, _)| *e == id).map(|(_, n)| n.clone())).collect();
    let posted_in = if thread.is_some() { format!("Posted in the thread in {label}.") } else { format!("Posted in {label}.") };
    let woke_nobody = if others.is_empty() { String::new() } else { format!(" This wakes nobody: a name only refers to {}. Use hand_off when they should act.", others.join(", ")) };
    Ok((true, format!("{posted_in}{woke_nobody}"), notices))
}

/// A handoff on its way: who gives what to whom, where.
struct Handoff<'a> {
    discussion: &'a Discussion,
    run: RunId,
    from: (EmployeeId, &'a str),
    to: (EmployeeId, &'a str),
    text: &'a str,
    hop: u32,
    login: Option<UserId>,
    thread: Option<MessageId>,
}

/// The handoff goes out: a message in the thread under the sender's name, "Flux handed
/// over to Bo" and the handoff itself, and an inbox item for Bo at the handoff's hop.
fn deliver_handoff(tx: &Tx<'_>, store: &Store, h: Handoff<'_>) -> Result<Vec<EventNotice>, StorageError> {
    let d = h.discussion;
    let posted = MessageId::new();
    let body = format!("{} handed over to {}\n\n{}", h.from.1, h.to.1, h.text);
    let extra = serde_json::json!({ "run_id": h.run, "hop": h.hop, "employee_id": h.from.0, "woke": 1, "handoff_to": h.to.0 });
    let mut notices = vec![post_in(tx, store, d.id, h.thread, posted, GROUP_KIND, h.from.1, &body, extra)?];
    let run = h.run.to_string();
    notices.extend(follow_named(tx, d, h.text, employee_cause(&run))?);
    let read = reader(store);
    let w = Wake { discussion: d, from: h.from.1, board: None, text: h.text, shared: Shared::default(), view: ThreadView::read(tx, &read, d, h.thread, Some(posted))?, hop: h.hop, login: h.login, thread: h.thread, lead_in: None };
    let meta = serde_json::json!({ "run_id": h.run, "group_post": posted });
    if let Some((_, woken)) = wake(tx, store, &meta, &run, h.to, &w)? {
        notices.extend(woken);
    }
    Ok(notices)
}

/// A scheduled message comes due in a discussion (`schedule_message`): a post under
/// its sender's name, and it wakes the addressee like a handoff, at the hop it was
/// planned at, on the login of the run that planned it. `None`: the addressee has no chat.
pub(crate) fn deliver_scheduled(tx: &Tx<'_>, store: &Store, d: &Discussion, m: &aurelius_storage::scheduled::Due, from: &str, to: &str) -> Result<Option<(RunId, Vec<EventNotice>)>, StorageError> {
    let own = m.from == m.to;
    let posted = MessageId::new();
    let body = if own { format!("Scheduled note from {from}\n\n{}", m.content) } else { format!("{from} scheduled a message for {to}\n\n{}", m.content) };
    // No run id: this post is not what that run wrote last, so its run card stays where it is.
    let extra = serde_json::json!({ "hop": m.hop, "employee_id": m.from, "woke": 0, "handoff_to": m.to, "scheduled": m.id });
    let mut notices = vec![post_in(tx, store, d.id, m.thread_parent, posted, GROUP_KIND, from, &body, extra)?];
    let read = reader(store);
    let lead_in = if own { "You scheduled this for yourself earlier".to_owned() } else { format!("{from} scheduled this for you earlier") };
    let w = Wake { discussion: d, from, board: None, text: &m.content, shared: Shared::default(), view: ThreadView::read(tx, &read, d, m.thread_parent, Some(posted))?, hop: m.hop, login: m.login_user, thread: m.thread_parent, lead_in: Some(lead_in) };
    // Named even when empty: a run without a login waits for one, it never takes another.
    let meta = serde_json::json!({ "scheduled": m.id, "group_post": posted, "login_user": m.login_user });
    let Some((run, woken)) = wake(tx, store, &meta, &m.id.to_string(), (m.to, to), &w)? else { return Ok(None) };
    notices.extend(woken);
    Ok(Some((run, notices)))
}

/// `hand_off`: `run`'s employee gives the work in its thread to the colleague `to`.
/// Past the limits nothing goes out but a system line and the refusal; while the sender
/// has unread items in the thread it is held (R5), else it goes out now.
pub(crate) fn hand_off(tx: &Tx<'_>, store: &Store, run: RunId, to: &str, message: &str) -> Result<(bool, String, Vec<EventNotice>), StorageError> {
    let Some(d) = run_handoff_thread(tx, run)? else {
        return Ok((false, "A handoff is for a ticket's discussion or a channel of the Team chat, and this run is in neither. To give a colleague work, use delegate_task.".into(), vec![]));
    };
    let (me, name, text) = match employee_post(tx, run, &d, message)? {
        Ok(post) => post,
        Err(why) => return Ok((false, why, vec![])),
    };
    let wanted = to.trim().trim_start_matches('@').trim();
    let members = team(tx)?;
    let Some((colleague, to_name, active)) = members.iter().find(|(id, n, _)| n.trim().eq_ignore_ascii_case(wanted) || id.to_string() == wanted) else {
        return Ok((false, format!("No colleague called {wanted} in the company."), vec![]));
    };
    if *colleague == me {
        return Ok((false, "You cannot hand off to yourself.".into(), vec![]));
    }
    if !*active {
        return Ok((false, format!("{to_name} is paused and will not answer: hand off to someone else, or to the lead."), vec![]));
    }
    let thread = run_thread(tx, run, &d)?;
    let key = thread_key(&d);
    let Some(chat) = aurelius_storage::inbox::thread_conversation(tx, *colleague, &key, &d.label())? else {
        return Ok((false, format!("{to_name} cannot take work in a thread yet: they have no chat."), vec![]));
    };
    let hop = next_hop(tx, run, &d)?;
    let limit = group::hop_limit(d.ticket().is_some());
    // One budget over every channel and ticket together (B2), so a loop through three
    // discussions is no longer than one through a single one. A held handoff counts.
    let (of_run, of_mine) = aurelius_storage::discussions::waking_posts(tx, run, me, aurelius_storage::now_ms() - WAKE_WINDOW_MS)?;
    let of_run = of_run + aurelius_storage::inbox::held_of_run(tx, run)?;
    let refused = if hop > limit {
        Some(format!("colleagues already handed this over {limit} times after the board member's message; wait for a board member."))
    } else if of_run >= WAKING_POSTS_PER_RUN {
        Some(format!("you already handed over {WAKING_POSTS_PER_RUN} times in this run; wait for their answers."))
    } else if of_mine >= WAKING_POSTS_PER_WINDOW {
        Some(format!("you already handed over {WAKING_POSTS_PER_WINDOW} times in the last hour (in all channels and tickets together); wait for them or a board member."))
    } else {
        None
    };
    if let Some(refused) = refused {
        let notice = limit_line(tx, d.id, thread, to_name)?;
        return Ok((false, format!("Limit reached: {to_name} will not be woken: {refused}"), vec![notice]));
    }
    let login = crate::run::login::of_run(tx, run)?;
    let unread = aurelius_storage::inbox::unread(tx, me, &key)?;
    if unread > 0 {
        let mut line = SystemLine::new(SystemLineKind::Limit).with_action(system_action::HANDOFF_HELD);
        (line.from, line.to) = (Some(name.clone()), Some(to_name.clone()));
        let (event, notice) = system_line_event(tx, d.id, thread, &line, Some(&format!("Handoff held: {to_name} gets this from {name} once {name} has read the new messages in this thread.")))?;
        let payload = serde_json::json!({ "held_by": me, "run": run, "from": name, "text": text, "discussion": d.id, "thread_parent": thread });
        aurelius_storage::inbox::hold(tx, aurelius_storage::inbox::NewItem {
            employee: *colleague,
            thread_key: &key,
            ticket: d.ticket().map(|t| t.id),
            conversation: chat,
            kind: "message",
            source: "employee",
            hop,
            payload: &payload,
            login_user: login.map(|u| u.to_string()).as_deref(),
            task: None,
            source_event: event,
            run: RunId::new(),
        })?;
        return Ok((true, format!("Held: you still have {unread} unread message(s) in this thread. {to_name} gets this handoff unchanged once you have read them, so do not hand it off again. Finish this run; you read them next."), vec![notice]));
    }
    let place = if thread.is_some() { format!("the thread in {}", d.label()) } else { d.label() };
    let notices = deliver_handoff(tx, store, Handoff { discussion: &d, run, from: (me, &name), to: (*colleague, to_name), text: &text, hop, login, thread })?;
    Ok((true, format!("Handed over to {to_name} in {place}: {to_name} will answer there."), notices))
}

/// Deliver the handoffs held for a sender that has caught up (R5), unchanged. One whose
/// discussion closed or whose colleague was paused meanwhile leaves a line instead.
pub(crate) fn release_handoffs(tx: &Tx<'_>, store: &Store) -> Result<Vec<EventNotice>, StorageError> {
    let mut notices = Vec::new();
    for item in aurelius_storage::inbox::releasable_handoffs(tx)? {
        let p = &item.payload;
        let discussion = p["discussion"].as_str().and_then(|c| ConversationId::parse(c).ok());
        let held = (p["held_by"].as_str().and_then(|e| EmployeeId::parse(e).ok()), p["run"].as_str().and_then(|r| RunId::parse(r).ok()), EmployeeId::parse(&item.employee).ok());
        let d = match discussion {
            Some(id) => load(tx, id)?,
            None => None,
        };
        let (Some(d), (Some(from), Some(run), Some(to))) = (d, held) else {
            aurelius_storage::inbox::settle_handoff(tx, &item.id, "cancelled")?;
            continue;
        };
        let (from_name, text) = (p["from"].as_str().unwrap_or("A colleague"), p["text"].as_str().unwrap_or_default());
        let thread = p["thread_parent"].as_str().and_then(|m| MessageId::parse(m).ok());
        let to_name = employee_name(tx, to)?;
        let active = team(tx)?.iter().any(|(id, _, active)| *id == to && *active);
        if let Some(why) = d.read_only().or_else(|| (!active).then(|| format!("{to_name} is paused."))) {
            aurelius_storage::inbox::settle_handoff(tx, &item.id, "cancelled")?;
            notices.push(system_line_in(tx, d.id, thread, &SystemLine::new(SystemLineKind::Limit), Some(&format!("The handoff from {from_name} to {to_name} was not delivered: {why}")))?);
            continue;
        }
        aurelius_storage::inbox::settle_handoff(tx, &item.id, "done")?;
        let login = item.login_user.as_deref().and_then(|u| UserId::parse(u).ok());
        notices.extend(deliver_handoff(tx, store, Handoff { discussion: &d, run, from: (from, from_name), to: (to, &to_name), text, hop: item.hop, login, thread })?);
    }
    Ok(notices)
}

/// `read_group_chat` / `read_discussion`: the latest `last` messages of a
/// discussion (default and most 50), oldest first, one JSON object per message.
pub(crate) fn read_text(conn: &rusqlite::Connection, store: &Store, d: &Discussion, last: Option<u32>) -> Result<String, StorageError> {
    let read = reader(store);
    let lines = discussion_json(conn, &read, d.id, None, None, last.unwrap_or(50).clamp(1, 50), 2_000)?;
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
    /// A board member adds or takes back a reaction (`message.react`, feature
    /// `chat_reactions`): only on a message of a channel or a ticket's discussion
    /// that is not closed (`channel_archived`, `validation` for an older client).
    /// Idempotent: nothing changes, and no event, when it already is as asked.
    /// A change is one `message.reacted` event on the discussion.
    pub(crate) fn react_to_message(&self, ctx: &CommandContext, envelope: &CommandEnvelope, hash: String, cmd: &group::ReactToMessage) -> Result<Committed, AppError> {
        let user = crate::ticket::require_board_member(ctx)?;
        let emoji = cmd.validate().map_err(domain_error)?;
        let (message, on) = (cmd.message_id, cmd.on);
        let meta = ctx.metadata();
        let actor = ctx.principal_id.to_string();
        let command_id = envelope.command_id;
        self.commit(command_id, hash, move |tx| {
            let Some(place) = aurelius_storage::threads::place(tx, message)? else { return Ok(Err(AppError::new(ErrorCode::NotFound, "This message does not exist."))) };
            let Some(d) = load(tx, place.conversation_id)? else { return Ok(Err(AppError::new(ErrorCode::Validation, REACTION_NOT_HERE))) };
            if let Some(why) = d.read_only() {
                return Ok(Err(AppError::new(ErrorCode::ChannelArchived, why)));
            }
            let changed = aurelius_storage::threads::react(tx, message, user, &emoji, on)?;
            let mut notices = Vec::new();
            if changed {
                let mut metadata = meta.clone();
                if let Some(m) = metadata.as_object_mut() {
                    m.extend([("message_id".to_owned(), serde_json::json!(message)), ("emoji".to_owned(), serde_json::json!(emoji)), ("on".to_owned(), serde_json::json!(on))]);
                }
                let (_, notice) = emit(tx, Ev {
                    aggregate_type: "conversation",
                    aggregate_id: d.id.to_string(),
                    event_type: "message.reacted",
                    actor: ActorKind::User,
                    actor_id: Some(actor.clone()),
                    correlation: command_id.to_string(),
                    payload: None,
                    metadata,
                    dedupe: format!("cmd:{command_id}:reacted"),
                })?;
                notices.push(notice);
            }
            Ok(Ok((CommandResult::MessageReacted { conversation_id: d.id, message_id: message, emoji, on, changed }, notices)))
        })
    }

    /// A board member changes the text of their own message in a channel or a
    /// ticket's discussion (`message.edit`, feature `message_edits`). The new text
    /// goes through the vault redactor; the earlier blob is the payload of the one
    /// `message.edited` event, so the audit log keeps it. It calls nothing of
    /// `board_message`: no run, no mention, no follow, no notification row.
    pub(crate) fn edit_message(&self, ctx: &CommandContext, envelope: &CommandEnvelope, hash: String, cmd: &group::EditMessage) -> Result<Committed, AppError> {
        let user = crate::ticket::require_board_member(ctx)?;
        let text = aurelius_domain::conversation::validate_message(&cmd.text, &[]).map_err(domain_error)?;
        let text = aurelius_runtime::redact::string(text);
        self.change_message(envelope, hash, user, cmd.message_id, Change::Edit(text))
    }

    /// A board member deletes their own message in a channel or a ticket's
    /// discussion (`message.delete`, feature `message_edits`): the row stays with
    /// `MESSAGE_DELETED_TEXT`, and the earlier blob is the payload of the one
    /// `message.deleted` event. Nobody is woken, as for an edit.
    pub(crate) fn delete_message(&self, ctx: &CommandContext, envelope: &CommandEnvelope, hash: String, cmd: &group::DeleteMessage) -> Result<Committed, AppError> {
        let user = crate::ticket::require_board_member(ctx)?;
        self.change_message(envelope, hash, user, cmd.message_id, Change::Delete)
    }

    /// The shared body of `message.edit` and `message.delete`: the same
    /// `EditFacts::refusal` as the read's `editable`, then the new blob and the event.
    fn change_message(&self, envelope: &CommandEnvelope, hash: String, user: UserId, message: MessageId, change: Change) -> Result<Committed, AppError> {
        let command_id = envelope.command_id;
        let store = self.store.clone();
        self.commit(command_id, hash, move |tx| {
            let Some((conversation, facts)) = aurelius_storage::message_edits::facts(tx, message)? else {
                return Ok(Err(AppError::new(ErrorCode::NotFound, "This message does not exist.")));
            };
            if let Some(refusal) = facts.refusal(Some(user)) {
                let code = match refusal {
                    EditRefusal::ReadOnly => ErrorCode::ChannelArchived,
                    EditRefusal::NotYours => ErrorCode::Forbidden,
                    EditRefusal::Deleted | EditRefusal::NotHere => ErrorCode::NotFound,
                    EditRefusal::Queued | EditRefusal::DecisionFeedback => ErrorCode::Validation,
                };
                return Ok(Err(AppError::new(code, refusal.why())));
            }
            let (text, event_type, metadata) = match &change {
                Change::Edit(text) => (text.as_str(), "message.edited", serde_json::json!({ "message_id": message, "by": user })),
                Change::Delete => (group::MESSAGE_DELETED_TEXT, "message.deleted", serde_json::json!({ "message_id": message, "by": user, "queued": false })),
            };
            let blob = store.blobs().put(text.as_bytes(), "text/markdown; charset=utf-8")?;
            blobs::register(tx, &blob)?;
            let at = aurelius_storage::now_ms();
            let replaced = match change {
                Change::Edit(_) => conv::replace_message_text(tx, message, &blob.id.to_string(), at)?,
                Change::Delete => conv::mark_message_deleted(tx, message, &blob.id.to_string(), at)?,
            };
            let Some(replaced) = replaced else { return Ok(Err(AppError::new(ErrorCode::NotFound, "This message does not exist."))) };
            let mut notices = Vec::new();
            for copy in conv::task_copies_of(tx, message)? {
                notices.extend(change_owner_copy(tx, &store, &copy, &change, &blob, at, (user, command_id))?);
            }
            let (_, notice) = emit(tx, Ev {
                aggregate_type: "conversation",
                aggregate_id: conversation.to_string(),
                event_type,
                actor: ActorKind::User,
                actor_id: Some(user.to_string()),
                correlation: command_id.to_string(),
                payload: replaced.previous_blob,
                metadata,
                dedupe: format!("cmd:{command_id}:{}", if matches!(change, Change::Edit(_)) { "edited" } else { "deleted" }),
            })?;
            let result = match change {
                Change::Edit(_) => CommandResult::MessageEdited { conversation_id: conversation, message_id: message, edited_at_ms: at },
                Change::Delete => CommandResult::MessageDeleted { conversation_id: conversation, message_id: message, deleted_at_ms: at },
            };
            notices.insert(0, notice);
            Ok(Ok((result, notices)))
        })
    }

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

    #[test]
    fn a_run_duration_is_said_in_seconds_minutes_or_hours() {
        assert_eq!(run_duration(45_000), "45s");
        assert_eq!(run_duration(14 * 60_000 + 5_000), "14m");
        assert_eq!(run_duration(2 * 3_600_000 + 5 * 60_000), "2h 5m");
        assert_eq!(run_duration(-5), "0s");
    }

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

#[cfg(test)]
mod owner_answer_tests {
    use super::owner_answer_text;

    #[test]
    fn an_answer_names_the_responsible_member_once() {
        assert_eq!(owner_answer_text("It is done.", None, Some("sebastien")), "@sebastien It is done.");
        assert_eq!(owner_answer_text("@Sebastien it is done.", None, Some("sebastien")), "@Sebastien it is done.");
        assert_eq!(owner_answer_text("It is done.", None, None), "It is done.");
    }

    #[test]
    fn a_longer_name_that_starts_with_the_username_does_not_count_as_naming_them() {
        assert_eq!(owner_answer_text("Ask @sebastien.", None, Some("seb")), "@seb Ask @sebastien.");
        assert_eq!(owner_answer_text("Ask @seb, or @sebastien.", None, Some("seb")), "Ask @seb, or @sebastien.");
    }

    #[test]
    fn a_question_typed_in_the_task_is_quoted_on_one_line_above_the_answer() {
        assert_eq!(owner_answer_text("No.", Some("Are these\ncards now?"), None), "> Are these cards now?\n\nNo.");
    }

    #[test]
    fn an_empty_answer_still_tells_the_member_it_finished() {
        assert_eq!(owner_answer_text("  ", None, None), "Done.");
    }
}
