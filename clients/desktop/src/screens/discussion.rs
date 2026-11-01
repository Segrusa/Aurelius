//! Phase 2 (AUR-MU-02, ui-ontwerp fase 2): what the ticket discussion, the Team
//! chat's channels and "Create ticket" from a message decide, apart from how they
//! render (`ticket.rs`, `group_chat.rs`, `chat.rs`), so it is tested without a window.
//!
//! The discussion is the ticket's own conversation (`TicketView::conversation_id`,
//! B1), read with `conversation.get` and written with `ticket.post`. A server
//! without the discussion engine leaves the id out, or answers `ticket.post` /
//! `ticket.retry_summary` with `unknown_method`: the ticket then works as in
//! phase 1 (the feedback field of B2) and the discussion tab says why.

use aurelius_client::ClientError;
use aurelius_domain::conversation::{MessageRole, MessageStatus};
use aurelius_domain::error::ErrorCode;
use aurelius_domain::ticket::{ColumnKind, TicketView};
use aurelius_domain::views::{ChannelKind, ChannelView, SystemLine, SystemLineKind};
use aurelius_domain::{EmployeeId, MessageId, TicketId};
use aurelius_protocol::MessageView;

// ---- System lines -------------------------------------------------------------

/// A system line in words (ui-ontwerp §1): what the server wrote as `text` when it
/// wrote one, else made from its fields, so an older or terse server still reads well.
pub fn system_text(line: &SystemLine, text: &str) -> String {
    let text = text.trim();
    if !text.is_empty() {
        return text.to_string();
    }
    let or = |v: &Option<String>, d: &str| v.as_deref().map(str::trim).filter(|s| !s.is_empty()).unwrap_or(d).to_string();
    let by = or(&line.by, "Aurelius");
    match line.kind {
        SystemLineKind::Delegated => format!("{} delegated a task to {}", or(&line.from, "An employee"), or(&line.to, "a colleague")),
        SystemLineKind::Reported => format!("{} reported", or(&line.from, "A colleague")),
        SystemLineKind::Moved => match (&line.from, &line.to) {
            (Some(from), Some(to)) => format!("{by} moved {from} → {to}"),
            (_, Some(to)) => format!("{by} moved to {to}"),
            _ => format!("{by} moved the ticket"),
        },
        SystemLineKind::Plan => format!("{} handed in a plan · waiting for a board member", or(&line.from, "The owner")),
        SystemLineKind::Review => format!("{} handed in the report · Review", or(&line.from, "The owner")),
        SystemLineKind::Done => format!("{by} put it on Done"),
        SystemLineKind::CreatedFrom => format!("Created from a message{}", line.by.as_deref().map(|b| format!(" · {b}")).unwrap_or_default()),
        SystemLineKind::TicketCreated => format!("{by} created a ticket from a message"),
        SystemLineKind::Summary => "Summary saved to project memory".to_string(),
        SystemLineKind::SummaryFailed => "The summary for project memory failed".to_string(),
        SystemLineKind::Limit => format!("Limit reached: {} was not woken.", or(&line.to, "nobody")),
    }
}

/// A `Limit` line about a Claude login or its quota (ui-ontwerp §5, F2 B11): the
/// server writes them with its own text and no protocol variant of their own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginLine {
    /// "Paused: Alex's Claude limit is reached until 16:00."
    Paused,
    /// "Resumed."
    Resumed,
    /// "Waiting: Alex's Claude login is missing. …"
    Waiting,
    /// "Connect your Claude login to get an answer. …" (a run without a ticket).
    Connect,
}

pub fn login_line(line: &SystemLine, text: &str) -> Option<LoginLine> {
    if line.kind != SystemLineKind::Limit {
        return None;
    }
    let text = text.trim();
    if text.starts_with("Paused:") {
        Some(LoginLine::Paused)
    } else if text == "Resumed." {
        Some(LoginLine::Resumed)
    } else if text.starts_with("Connect your Claude login") {
        Some(LoginLine::Connect)
    } else if text.starts_with("Waiting:") {
        Some(LoginLine::Waiting)
    } else {
        None
    }
}

/// The shape before such a line's text, never only a colour.
pub fn login_mark(kind: Option<LoginLine>) -> &'static str {
    match kind {
        Some(LoginLine::Paused) => "⏸ ",
        Some(LoginLine::Resumed) => "▸ ",
        Some(LoginLine::Waiting | LoginLine::Connect) => "◆ ",
        None => "",
    }
}

/// What a screen reader hears for a system line (ui-ontwerp §5): "System: … at 10:41".
pub fn system_label(text: &str, at: &str) -> String {
    format!("System: {text} at {at}")
}

/// Lines a board member can act on show their action: Open task, Try again.
pub fn system_task(line: &SystemLine) -> Option<aurelius_domain::TaskId> {
    line.task_id.filter(|_| matches!(line.kind, SystemLineKind::Delegated | SystemLineKind::Reported | SystemLineKind::Plan | SystemLineKind::Review))
}

/// Where the ticket's summary for project memory stands, from its discussion (MU2-12).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Summary {
    /// Not on Done yet, or no line about it yet (it is being made).
    Pending,
    Saved,
    /// The last attempt failed: the reason, and a board member can try again (AC4).
    Failed(String),
}

/// The newest summary line decides (a retry that worked comes after the failure).
pub fn summary_state(messages: &[MessageView]) -> Summary {
    for m in messages.iter().rev() {
        match m.system.as_ref().map(|s| s.kind) {
            Some(SystemLineKind::Summary) => return Summary::Saved,
            Some(SystemLineKind::SummaryFailed) => return Summary::Failed(m.text.trim().to_string()),
            _ => {}
        }
    }
    Summary::Pending
}

/// The failed summary line that shows "Try again": only the newest summary line,
/// and only while it is the failure (an older failure was tried again already).
pub fn retry_line(messages: &[MessageView]) -> Option<MessageId> {
    messages.iter().rev().find(|m| matches!(m.system.as_ref().map(|s| s.kind), Some(SystemLineKind::Summary | SystemLineKind::SummaryFailed))).filter(|m| m.system.as_ref().is_some_and(|s| s.kind == SystemLineKind::SummaryFailed)).map(|m| m.id)
}

// ---- Authors --------------------------------------------------------------------

/// Who wrote a discussion message and what they are (ui-ontwerp §1): a board member
/// ("board member"), an employee (its role), or Aurelius (a system line).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Author {
    pub name: String,
    pub kind: String,
    pub human: bool,
}

pub fn author(m: &MessageView, role_of: impl Fn(&str) -> Option<String>) -> Author {
    if let Some(user) = &m.author_user {
        return Author { name: user.username.clone(), kind: "board member".into(), human: true };
    }
    match m.role {
        // A board member's post from a server that does not name them yet (A-01).
        MessageRole::User => Author { name: "Board member".into(), kind: "board member".into(), human: true },
        MessageRole::Assistant => {
            let name = m.author.as_deref().map(str::trim).filter(|a| !a.is_empty()).unwrap_or("Aurelius").to_string();
            let kind = role_of(&name).unwrap_or_else(|| "employee".into());
            Author { name, kind, human: false }
        }
    }
}

// ---- Mentions ------------------------------------------------------------------

/// Someone an `@` can name in a discussion or channel (ui-ontwerp §1): the active
/// employees in org chart order, then the board members.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mentionable {
    pub name: String,
    /// The employee's role, or "board member".
    pub kind: String,
    pub employee: bool,
}

/// The `@word` being typed at the end of `text`: where its `@` is and what follows.
/// An `@` inside a word is an e-mail address, not a mention.
pub fn typing_mention(text: &str) -> Option<(usize, &str)> {
    let at = text.rfind('@')?;
    let before = text[..at].chars().next_back();
    if before.is_some_and(|c| !c.is_whitespace() && c != '(') {
        return None;
    }
    let word = &text[at + 1..];
    (!word.chars().any(char::is_whitespace)).then_some((at, word))
}

/// The suggestions for the `@` being typed: names that start with it (any case),
/// employees first. None when nothing is typed after a mention or it is complete.
pub fn mention_suggestions<'a>(text: &str, people: &'a [Mentionable]) -> Option<(usize, Vec<&'a Mentionable>)> {
    let (at, word) = typing_mention(text)?;
    let word = word.to_lowercase();
    if people.iter().any(|p| p.name.to_lowercase() == word) {
        return None;
    }
    let found: Vec<&Mentionable> = people.iter().filter(|p| p.name.to_lowercase().starts_with(&word)).collect();
    (!found.is_empty()).then_some((at, found))
}

/// What a screen reader hears for a suggestion (ui-ontwerp §5): "Sentinel, Security Engineer, 1 of 2".
pub fn suggestion_label(p: &Mentionable, index: usize, of: usize) -> String {
    format!("{}, {}, {} of {of}", p.name, p.kind, index + 1)
}

/// `text` with the `@word` at `at` replaced by the whole name and a space.
pub fn complete(text: &str, at: usize, name: &str) -> String {
    format!("{}@{name} ", &text[..at])
}

/// The keyboard of the `@` suggestions (ui-ontwerp §1): ↑↓ move the active one
/// (round), Enter or Tab picks it, Esc closes the list until the text changes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MentionPicker {
    pub selected: usize,
    /// The text the list was closed on with Esc.
    closed: Option<String>,
}

impl MentionPicker {
    /// The text changed: the list starts at the top again, and opens again after Esc.
    pub fn changed(&mut self, text: &str) {
        self.selected = 0;
        if self.closed.as_deref() != Some(text) {
            self.closed = None;
        }
    }

    /// Whether the list shows for `text` (it has suggestions and was not closed on it).
    pub fn shows(&self, text: &str) -> bool {
        self.closed.as_deref() != Some(text)
    }

    pub fn close(&mut self, text: &str) {
        self.closed = Some(text.to_string());
    }

    pub fn step(&mut self, down: bool, of: usize) {
        if of == 0 {
            return;
        }
        let at = self.selected.min(of - 1);
        self.selected = if down { (at + 1) % of } else { (at + of - 1) % of };
    }

    /// The active suggestion among `of` (the list may have shrunk since).
    pub fn active(&self, of: usize) -> Option<usize> {
        (of > 0).then(|| self.selected.min(of - 1))
    }
}

/// The `@names` in a message, as written (without the `@`), in order and once each.
pub fn mentions(text: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for (i, _) in text.match_indices('@') {
        let before = text[..i].chars().next_back();
        if before.is_some_and(|c| !c.is_whitespace() && c != '(') {
            continue;
        }
        let name: String = text[i + 1..].chars().take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '-' || *c == '.').collect();
        let name = name.trim_end_matches('.').to_string();
        if !name.is_empty() && !found.iter().any(|f| f.eq_ignore_ascii_case(&name)) {
            found.push(name);
        }
    }
    found
}

/// `@names` in a message that wake nobody (MU2-04 AC5): unknown, or an archived
/// employee. The client says so before it is sent; the server decides in the end.
pub fn unknown_mentions(text: &str, people: &[Mentionable], archived: &[&str]) -> Vec<String> {
    mentions(text)
        .into_iter()
        .filter(|n| archived.iter().any(|a| a.eq_ignore_ascii_case(n)) || !people.iter().any(|p| p.name.eq_ignore_ascii_case(n)))
        .collect()
}

/// The warning under the composer for those names (none: all known).
pub fn unknown_warning(names: &[String]) -> Option<String> {
    match names {
        [] => None,
        [one] => Some(format!("Nobody active is called @{one}: it wakes no one.")),
        many => Some(format!("Nobody active is called {}: they wake no one.", many.iter().map(|n| format!("@{n}")).collect::<Vec<_>>().join(", "))),
    }
}

/// Whether a message names this board member (B9: marked, no notification yet).
pub fn names_me(text: &str, me: Option<&str>) -> bool {
    me.is_some_and(|me| mentions(text).iter().any(|n| n.eq_ignore_ascii_case(me)))
}

// ---- The composer -----------------------------------------------------------------

/// Whether a board member can still write in the discussion (MU2-01 AC3, B7: Done
/// is read-only, and so is an archived ticket: the server refuses a post in either).
pub fn writable(t: &TicketView) -> bool {
    t.column != ColumnKind::Done && t.archived_at_ms.is_none()
}

/// Whom a post without `@` reaches (B3, P-21): the owner while the ticket is in
/// Todo with an owner, Doing, Plan review, Testing or Review and not archived; else
/// nobody. The same rule as the server's `owner_answers`.
pub fn reaches_owner(t: &TicketView) -> bool {
    t.owner_employee_id.is_some() && t.archived_at_ms.is_none() && matches!(t.column, ColumnKind::Todo | ColumnKind::Doing | ColumnKind::PlanReview | ColumnKind::Testing | ColumnKind::Review)
}

/// What the read-only footer says (B7): Done with how the summary went, or archived.
pub fn read_only_note(t: &TicketView, summary: &Summary) -> &'static str {
    if t.column != ColumnKind::Done {
        return "Archived · read-only. Follow-up work: create a new ticket.";
    }
    match summary {
        Summary::Saved => "Done · read-only. The summary is in project memory. Follow-up work: create a new ticket.",
        Summary::Failed(_) => "Done · read-only. The summary failed: try again above. Follow-up work: create a new ticket.",
        Summary::Pending => "Done · read-only. Follow-up work: create a new ticket.",
    }
}

/// The composer's placeholder (ui-ontwerp §1, P-21).
pub fn composer_hint(t: &TicketView) -> String {
    match (&t.owner_name, reaches_owner(t)) {
        (Some(owner), true) => format!("Message {owner} (owner) · @ to mention someone else"),
        _ => "No one is woken. @mention to ask someone.".to_string(),
    }
}

/// The discussion's empty state before anything was said (ui-ontwerp §1).
pub const EMPTY: &str = "Discuss the ticket here. Employees join when you @mention them or when it starts.";

/// Send back / Pas aan without text (ui-ontwerp §1).
pub const WRITE_FIRST: &str = "Write what needs to change first.";

/// A ticket from a server without the discussion: the tab says why (phase 1 still works).
pub const NO_DISCUSSION: &str = "This server has no ticket discussion yet. Update the server; until then the feedback field on the Ticket tab and the main task's conversation work as before.";

/// Where the discussion stands for the open ticket.
#[derive(Debug, Clone, PartialEq)]
pub enum Discussion {
    Loading,
    Failed(String),
    Ready(Vec<MessageView>),
}

/// Messages since the board member last looked (the tab's count, ui-ontwerp §1):
/// none while the tab shows or before anything was seen.
pub fn unread(total: usize, seen: Option<usize>, showing: bool) -> usize {
    if showing { 0 } else { seen.map_or(0, |s| total.saturating_sub(s)) }
}

/// A discussion message that counts as said (system lines and streaming ones too, once there).
pub fn counted(messages: &[MessageView]) -> usize {
    messages.iter().filter(|m| !m.queued).count()
}

/// The employees answering right now: a run streams into the discussion and its
/// message is not there yet (ui-ontwerp §1: "Forge is answering…").
pub fn answering(messages: &[MessageView], streaming: Option<(&MessageId, Option<String>)>) -> Option<String> {
    let (id, who) = streaming?;
    if messages.iter().any(|m| m.id == *id && m.status != MessageStatus::Partial) {
        return None;
    }
    Some(format!("{} is answering…", who.unwrap_or_else(|| "An employee".into())))
}

// ---- Server answers ---------------------------------------------------------------

/// Why a post in the discussion did not go (the draft stays, ui-ontwerp §1).
pub fn post_failure(e: &ClientError) -> String {
    match e {
        ClientError::Protocol(p) if p.code == ErrorCode::UnknownMethod => "This server cannot take discussion messages yet (ticket.post). Update the server; your text stays here.".into(),
        e if e.is_disconnect() || e.is_final() => "The connection dropped before the server answered. Your text stays here; send it again when it is back.".into(),
        ClientError::Protocol(p) => p.message.clone(),
        e => e.to_string(),
    }
}

/// Why the discussion did not load (shown with Try again, ui-ontwerp §1).
pub fn load_failure(e: &ClientError) -> String {
    match e {
        ClientError::TimedOut(_) => "The server did not answer in time.".into(),
        e => format!("The discussion did not load: {e}"),
    }
}

/// Why "Try again" on a failed summary did not go (MU2-12 AC4).
pub fn retry_failure(e: &ClientError) -> String {
    match e {
        ClientError::Protocol(p) if p.code == ErrorCode::UnknownMethod => "This server cannot try the summary again yet (ticket.retry_summary). Update the server.".into(),
        e if e.is_disconnect() || e.is_final() => "The connection dropped before the server answered. Try again when it is back.".into(),
        ClientError::Protocol(p) => p.message.clone(),
        e => e.to_string(),
    }
}

/// Whom a post reached, from `ticket.post`'s `to` (`TicketMessagePosted`): the
/// server puts the owner first when the post went to them in the main task (B3,
/// `owner_answers`, the same rule as `reaches_owner`), with or without a run of
/// its own; every other one in `to` was woken in the discussion. Whether the
/// owner took it: the owner is in `to` while the post reaches them. Outside that
/// (Backlog) an `@owner` wakes them like anyone else.
pub fn reached(t: Option<&TicketView>, to: &[EmployeeId]) -> (bool, Vec<EmployeeId>) {
    let owner = t.filter(|t| reaches_owner(t)).and_then(|t| t.owner_employee_id).filter(|o| to.contains(o));
    (owner.is_some(), to.iter().copied().filter(|e| Some(*e) != owner).collect())
}

/// What the board member hears after a post: whom it reached (B3).
pub fn posted_text(woken: &[String], owner_took_it: bool) -> String {
    let woke = match woken {
        [] => None,
        [one] => Some(format!("{one} is woken")),
        many => Some(format!("{} are woken", many.join(", "))),
    };
    match (owner_took_it, woke) {
        (true, None) => "Sent. The owner gets it in the main task.".into(),
        (true, Some(w)) => format!("Sent. The owner gets it in the main task; {w}."),
        (false, Some(w)) => format!("Sent. {w}."),
        (false, None) => "Sent. No one is woken.".into(),
    }
}

// ---- Channels ---------------------------------------------------------------------

/// Rows a section of the channel list shows before its "Show more" (ui-ontwerp §1, O-5).
pub const SHOW_MORE_AFTER: usize = 15;

/// What a channel is, also on a server without `ticket_channels` (which leaves
/// `kind` out, so it reads as General): a ticket's, else no project = general,
/// else a project's (an unknown kind of a newer server too).
pub fn kind_of(c: &ChannelView) -> ChannelKind {
    match c.kind {
        ChannelKind::Ticket => ChannelKind::Ticket,
        _ if c.project_id.is_none() => ChannelKind::General,
        _ => ChannelKind::Project,
    }
}

/// The channel list in its sections (ui-ontwerp §1).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Sections<'a> {
    /// `# general`, first under CHANNELS.
    pub general: Option<&'a ChannelView>,
    /// The active projects' channels A–Z, under CHANNELS after general.
    pub projects: Vec<&'a ChannelView>,
    /// The open tickets' channels, newest activity first (same moment: the higher number first).
    pub tickets: Vec<&'a ChannelView>,
    /// Archived projects A–Z, then closed tickets newest first: read-only, folded away.
    pub archived: Vec<&'a ChannelView>,
}

fn by_activity(a: &&ChannelView, b: &&ChannelView) -> std::cmp::Ordering {
    (b.last_activity_ms, b.ticket_number).cmp(&(a.last_activity_ms, a.ticket_number))
}

pub fn sections(channels: &[ChannelView]) -> Sections<'_> {
    let general = channels.iter().find(|c| kind_of(c) == ChannelKind::General);
    let of = |kind: ChannelKind, archived: bool| channels.iter().filter(move |c| kind_of(c) == kind && c.archived == archived);
    let mut projects: Vec<&ChannelView> = of(ChannelKind::Project, false).collect();
    projects.sort_by_key(|c| c.name.to_lowercase());
    let mut tickets: Vec<&ChannelView> = of(ChannelKind::Ticket, false).collect();
    tickets.sort_by(by_activity);
    let mut archived: Vec<&ChannelView> = of(ChannelKind::Project, true).collect();
    archived.sort_by_key(|c| c.name.to_lowercase());
    let mut closed: Vec<&ChannelView> = of(ChannelKind::Ticket, true).collect();
    closed.sort_by(by_activity);
    archived.extend(closed);
    Sections { general, projects, tickets, archived }
}

/// `list` in the order of `frozen` (the order shown when the pointer came over the
/// list), the ones not in it after, as they were: a row never jumps under the click.
pub fn keep_order<'a>(mut list: Vec<&'a ChannelView>, frozen: &[aurelius_domain::ConversationId]) -> Vec<&'a ChannelView> {
    list.sort_by_key(|c| frozen.iter().position(|f| *f == c.conversation_id).unwrap_or(usize::MAX));
    list
}

/// How many of a section's `len` rows show: all once expanded or when few enough.
pub fn shown(len: usize, expanded: bool) -> usize {
    if expanded { len } else { len.min(SHOW_MORE_AFTER) }
}

/// The channels in the order the list shows them, the ones a reader can reach:
/// the archived ones only while unfolded, a section's rows up to its "Show more".
pub fn visible(s: &Sections, tickets_all: bool, archived_open: bool, archived_all: bool) -> Vec<aurelius_domain::ConversationId> {
    let tickets = s.tickets.iter().take(shown(s.tickets.len(), tickets_all));
    let archived = s.archived.iter().take(if archived_open { shown(s.archived.len(), archived_all) } else { 0 });
    s.general.into_iter().chain(s.projects.iter().copied()).chain(tickets.copied()).chain(archived.copied()).map(|c| c.conversation_id).collect()
}

/// A channel's name without the `#`: "general", "aurelius", "42-login-per-board-member".
fn bare_name(c: &ChannelView) -> String {
    match kind_of(c) {
        ChannelKind::General => "general".into(),
        // The server names it `#<number>-<slug>`.
        ChannelKind::Ticket => c.name.trim_start_matches('#').to_string(),
        _ => c.name.clone(),
    }
}

/// "# general", "# aurelius", "# 42-login-per-board-member": a channel's name as the list shows it.
pub fn channel_title(c: &ChannelView) -> String {
    format!("# {}", bare_name(c))
}

/// Ctrl+K "Go to #…" (ui-ontwerp §3): "Go to #general", "Go to #42-login".
pub fn go_to_channel_label(c: &ChannelView) -> String {
    format!("Go to #{}", bare_name(c))
}

/// What a screen reader hears for a channel (ui-ontwerp §5): "aurelius, 3 unread, 1 mention",
/// "Ticket 42, login-per-board-member, 5 unread".
pub fn channel_label(c: &ChannelView) -> String {
    let name = bare_name(c);
    let name = match (kind_of(c), c.ticket_number) {
        (ChannelKind::Ticket, Some(n)) => {
            let slug = name.strip_prefix(&n.to_string()).map(|s| s.trim_start_matches('-')).unwrap_or(&name);
            if slug.is_empty() { format!("Ticket {n}") } else { format!("Ticket {n}, {slug}") }
        }
        _ => name,
    };
    let mut parts = vec![name];
    if c.unread > 0 {
        parts.push(format!("{} unread", c.unread));
    }
    if c.mentions > 0 {
        parts.push(format!("{} mention{}", c.mentions, if c.mentions == 1 { "" } else { "s" }));
    }
    if c.archived {
        parts.push("archived, read-only".into());
    }
    parts.join(", ")
}

/// The Team chat's sidebar badge (B8, P-24): the mentions, and whether anything is unread (a dot).
pub fn badge(channels: &[ChannelView]) -> (u32, bool) {
    (channels.iter().map(|c| c.mentions).sum(), channels.iter().any(|c| c.unread > 0))
}

/// The open channel counts as read (AUR-CHAT-001 Q1): the sidebar badge has no unread
/// or mentions of it, also from a list read just before the channel was.
pub fn mark_read(channels: &mut [ChannelView], open: aurelius_domain::ConversationId) {
    for c in channels.iter_mut().filter(|c| c.conversation_id == open) {
        c.unread = 0;
        c.mentions = 0;
    }
}

/// The channel Alt+↑/↓ goes to among the `visible` ones (ui-ontwerp §1: Channels →
/// Tickets → Archived while unfolded); the list's ends stop it.
pub fn step_channel(visible: &[aurelius_domain::ConversationId], current: Option<aurelius_domain::ConversationId>, down: bool) -> Option<aurelius_domain::ConversationId> {
    if visible.is_empty() {
        return None;
    }
    let at = current.and_then(|c| visible.iter().position(|x| *x == c)).unwrap_or(0);
    let next = if down { (at + 1).min(visible.len() - 1) } else { at.saturating_sub(1) };
    Some(visible[next])
}

/// Where the "New" divider goes when a channel opens with `unread` new messages
/// (ui-ontwerp §3): before the first of them; none when nothing is new.
pub fn new_divider(total: usize, unread: u32) -> Option<usize> {
    let unread = unread as usize;
    (unread > 0 && unread <= total).then(|| total - unread)
}

/// The composer's placeholder in a channel (ui-ontwerp §4).
pub fn channel_hint(c: Option<&ChannelView>) -> String {
    match c {
        Some(c) if c.archived => read_only_bar(c),
        Some(c) => format!("Message #{} · @ to mention", bare_name(c)),
        None => "Message the team · @ to mention".into(),
    }
}

/// The bar instead of the composer in a read-only channel (ui-ontwerp §3): a closed
/// ticket's ("#42 is Done · read-only.") or an archived project's.
pub fn read_only_bar(c: &ChannelView) -> String {
    match (kind_of(c), c.ticket_number) {
        (ChannelKind::Ticket, number) => {
            let number = number.map_or_else(|| "This ticket".to_string(), |n| format!("#{n}"));
            if c.ticket_column == Some(ColumnKind::Done) { format!("{number} is Done · read-only.") } else { format!("{number} is archived · read-only.") }
        }
        _ => "This project is archived · read-only.".into(),
    }
}

/// The people on a ticket for its channel's header (ui-ontwerp §3): the owner first,
/// then the employees of its other tasks (delegations, the tester), each once.
pub fn assignees(detail: &aurelius_domain::ticket::TicketDetail) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    let owner = detail.ticket.owner_name.clone().or_else(|| detail.root_task.as_ref().and_then(|t| t.employee_name.clone()));
    for name in owner.into_iter().chain(detail.tasks.iter().filter_map(|t| t.employee_name.clone())) {
        let name = name.trim().to_string();
        if !name.is_empty() && !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

/// Assignees shown by name before the rest fold into "+n" (ui-ontwerp §3).
pub const ASSIGNEES_SHOWN: usize = 2;

// ---- A ticket from a message -----------------------------------------------------------

/// Longest title the form fills in from a message.
pub const TITLE_MAX: usize = 120;

/// The New ticket form filled in from a chat message (MU2-10 AC1, Fase 1 §10.3):
/// its first line as title, the message quoted as description with where it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prefill {
    pub title: String,
    pub description: String,
}

pub fn prefill(text: &str, author: &str, place: &str, when: &str) -> Prefill {
    let first = text.lines().map(|l| l.trim().trim_start_matches('#').trim()).find(|l| !l.is_empty()).unwrap_or("From a message");
    let title = if first.chars().count() > TITLE_MAX { format!("{}…", first.chars().take(TITLE_MAX - 1).collect::<String>().trim_end()) } else { first.to_string() };
    let quoted: Vec<String> = text.trim().lines().map(|l| if l.is_empty() { ">".to_string() } else { format!("> {l}") }).collect();
    let description = format!("{}\n\n— {author} in {place} · {when}", quoted.join("\n"));
    Prefill { title, description }
}

/// The ticket already made from this message, from the conversation's `ticket_created`
/// lines (ui-ontwerp §4: the "→ #52" pill, and "Create another?").
pub fn ticket_from(messages: &[MessageView], message: MessageId) -> Option<(Option<TicketId>, String)> {
    messages.iter().rev().find_map(|m| {
        let line = m.system.as_ref().filter(|s| s.kind == SystemLineKind::TicketCreated && s.message_id == Some(message))?;
        Some((line.ticket_id, ticket_number(&m.text).map_or_else(|| "a ticket".to_string(), |n| format!("#{n}"))))
    })
}

/// The first "#123" in a text.
pub fn ticket_number(text: &str) -> Option<u32> {
    text.match_indices('#').find_map(|(i, _)| {
        let digits: String = text[i + 1..].chars().take_while(char::is_ascii_digit).collect();
        digits.parse().ok()
    })
}

/// A message a ticket can be made from (ui-ontwerp §4): a finished one with text,
/// not a system line.
pub fn can_make_ticket(m: &MessageView) -> bool {
    m.system.is_none() && m.status != MessageStatus::Partial && !m.queued && !m.text.trim().is_empty()
}

/// Whether a message offers "Create ticket…": one a ticket can be made from, a
/// board member works in this window (B6), and the server knows `from_message`
/// (feature `ticket_discussion`): a phase 1 server refuses the whole create on an
/// unknown field, so then nothing offers it.
pub fn offers_ticket(m: &MessageView, board_member: bool, ticket_discussion: bool) -> bool {
    ticket_discussion && board_member && can_make_ticket(m)
}

/// The `from_message` a New ticket sends: only to a server with `ticket_discussion`.
pub fn from_message(message: Option<MessageId>, ticket_discussion: bool) -> Option<MessageId> {
    message.filter(|_| ticket_discussion)
}

// ---- Scrolling ------------------------------------------------------------------------

/// Whether the discussion goes to its bottom when its messages are read again
/// (MU2-01 AC2): when it just opened, after the board member's own post, or when
/// something came while the view stood at the bottom (like the chat). Scrolled
/// up to read, it stays where it is.
pub fn follows(first: bool, mine: bool, grew: bool, at_bottom: bool) -> bool {
    first || mine || (grew && at_bottom)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aurelius_domain::ConversationId;

    fn msg(role: MessageRole, text: &str) -> MessageView {
        MessageView {
            id: MessageId::new(),
            ordinal: 0,
            role,
            status: MessageStatus::Complete,
            text: text.into(),
            created_at_ms: 0,
            run_id: None,
            reply_to: None,
            produced_by: None,
            author: None,
            sources: vec![],
            activity: vec![],
            update: None,
            approval_id: None,
            files: vec![],
            uploads: vec![],
            queued: false,
            usage: None,
            mode: None,
            author_user: None,
            system: None,
            thread_parent: None,
            thread_reply_count: None,
            thread_last_reply_ms: None,
            reactions: Vec::new(),
        }
    }

    fn line(kind: SystemLineKind, text: &str) -> MessageView {
        let mut m = msg(MessageRole::Assistant, text);
        m.system = Some(SystemLine::new(kind));
        m
    }

    fn people() -> Vec<Mentionable> {
        vec![
            Mentionable { name: "Sentinel".into(), kind: "Security Engineer".into(), employee: true },
            Mentionable { name: "Schema".into(), kind: "Database Engineer".into(), employee: true },
            Mentionable { name: "Alex".into(), kind: "board member".into(), employee: false },
        ]
    }

    #[test]
    fn system_lines_read_from_their_fields_when_the_server_gives_no_text() {
        let mut moved = SystemLine::new(SystemLineKind::Moved);
        moved.by = Some("Forge".into());
        moved.from = Some("Doing".into());
        moved.to = Some("Review".into());
        assert_eq!(system_text(&moved, ""), "Forge moved Doing → Review");
        assert_eq!(system_text(&moved, "  Forge moved #42 Doing → Review "), "Forge moved #42 Doing → Review", "the server's text wins");
        let mut delegated = SystemLine::new(SystemLineKind::Delegated);
        delegated.from = Some("Orion".into());
        delegated.to = Some("Schema".into());
        delegated.task_id = Some(aurelius_domain::TaskId::new());
        assert_eq!(system_text(&delegated, ""), "Orion delegated a task to Schema");
        assert!(system_task(&delegated).is_some(), "a delegation links to its task (MU2-03 AC2)");
        let mut limit = SystemLine::new(SystemLineKind::Limit);
        limit.to = Some("Forge".into());
        assert_eq!(system_text(&limit, ""), "Limit reached: Forge was not woken.", "never left out silently");
        assert_eq!(system_text(&SystemLine::new(SystemLineKind::Done), ""), "Aurelius put it on Done");
        assert_eq!(system_label("Forge moved", "10:41"), "System: Forge moved at 10:41");
        assert!(system_task(&SystemLine::new(SystemLineKind::Moved)).is_none());
    }

    /// ui-ontwerp §5 / F2 B11: the login and quota lines (`Limit`, the server's text) get a shape.
    #[test]
    fn paused_and_resumed_lines_read_as_such() {
        let limit = SystemLine::new(SystemLineKind::Limit);
        let paused = "Paused: robin's Claude limit is reached until 16:00.";
        assert_eq!(login_line(&limit, paused), Some(LoginLine::Paused));
        assert_eq!(login_mark(login_line(&limit, paused)), "⏸ ");
        assert_eq!(system_text(&limit, paused), paused, "the server's words as they are");
        assert_eq!(login_line(&limit, "Resumed."), Some(LoginLine::Resumed));
        assert_eq!(login_mark(Some(LoginLine::Resumed)), "▸ ");
        assert_eq!(login_line(&limit, "Waiting: alex's Claude login is missing. It goes on once alex connects it."), Some(LoginLine::Waiting));
        assert_eq!(login_line(&limit, "Connect your Claude login to get an answer. (alex's Claude login is missing; this answer waits for it.)"), Some(LoginLine::Connect));
        assert_eq!(login_line(&limit, "Limit reached: Forge was not woken."), None, "a loop limit is no login line");
        assert_eq!(login_line(&SystemLine::new(SystemLineKind::Moved), "Resumed."), None);
        assert_eq!(login_mark(None), "");
    }

    #[test]
    fn a_failed_summary_shows_try_again_until_a_later_one_worked() {
        let failed = line(SystemLineKind::SummaryFailed, "The summary failed: quota reached.");
        let mut messages = vec![msg(MessageRole::User, "hi"), failed.clone()];
        assert_eq!(summary_state(&messages), Summary::Failed("The summary failed: quota reached.".into()));
        assert_eq!(retry_line(&messages), Some(failed.id), "MU2-12 AC4: visible and retryable");
        messages.push(line(SystemLineKind::Summary, "Summary saved"));
        assert_eq!(summary_state(&messages), Summary::Saved);
        assert_eq!(retry_line(&messages), None, "the older failure was tried again");
        assert_eq!(summary_state(&[]), Summary::Pending);
    }

    #[test]
    fn authors_are_a_board_member_an_employee_with_role_or_aurelius() {
        let mut post = msg(MessageRole::User, "Please keep it configurable.");
        post.author_user = Some(aurelius_domain::user::UserView { id: aurelius_domain::UserId::new(), username: "alex".into(), created_at_ms: 0, created_by_user: None, disabled_at_ms: None });
        assert_eq!(author(&post, |_| None), Author { name: "alex".into(), kind: "board member".into(), human: true });
        let mut answer = msg(MessageRole::Assistant, "Done.");
        answer.author = Some("Forge".into());
        assert_eq!(author(&answer, |n| (n == "Forge").then(|| "Backend Developer".into())).kind, "Backend Developer");
        assert_eq!(author(&msg(MessageRole::Assistant, "x"), |_| None).name, "Aurelius");
    }

    #[test]
    fn the_at_offers_employees_and_board_members_and_completes_a_name() {
        let p = people();
        let (at, found) = mention_suggestions("Add a test for @Se", &p).unwrap();
        assert_eq!(at, 15);
        assert_eq!(found.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["Sentinel"], "prefix, any case: Schema has no 'se'");
        assert_eq!(mention_suggestions("@", &p).unwrap().1.len(), 3, "a bare @ offers everyone");
        assert_eq!(mention_suggestions("@sentinel", &p), None, "complete already");
        assert_eq!(mention_suggestions("mail me@se", &p), None, "an e-mail address");
        assert_eq!(complete("Add a test for @Se", at, "Sentinel"), "Add a test for @Sentinel ");
        assert_eq!(suggestion_label(&p[0], 0, 2), "Sentinel, Security Engineer, 1 of 2");
    }

    #[test]
    fn the_suggestions_go_by_keyboard() {
        let mut picker = MentionPicker::default();
        assert_eq!(picker.active(3), Some(0));
        picker.step(true, 3);
        picker.step(true, 3);
        assert_eq!(picker.active(3), Some(2));
        picker.step(true, 3);
        assert_eq!(picker.active(3), Some(0), "↓ at the bottom goes round");
        picker.step(false, 3);
        assert_eq!(picker.active(3), Some(2), "↑ at the top goes round");
        assert_eq!(picker.active(1), Some(0), "a shorter list keeps a valid one");
        assert_eq!(picker.active(0), None);
        picker.changed("@S");
        assert_eq!(picker.active(3), Some(0), "typing starts at the top");
        picker.close("@S");
        assert!(!picker.shows("@S"), "Esc closes it");
        picker.changed("@S");
        assert!(!picker.shows("@S"), "the same text keeps it closed");
        picker.changed("@Se");
        assert!(picker.shows("@Se"), "typing on opens it again");
    }

    #[test]
    fn an_unknown_or_archived_mention_is_said_before_sending() {
        let p = people();
        assert_eq!(mentions("@Sentinel and (@Alex), mail a@b.c, @Sentinel again."), ["Sentinel", "Alex"]);
        assert!(unknown_mentions("@Sentinel please", &p, &[]).is_empty());
        assert_eq!(unknown_mentions("@Nobody and @Schema", &p, &["Schema"]), ["Nobody", "Schema"], "MU2-04 AC5");
        assert_eq!(unknown_warning(&["Nobody".into()]).unwrap(), "Nobody active is called @Nobody: it wakes no one.");
        assert_eq!(unknown_warning(&[]), None);
        assert!(names_me("Done. @alex the cost is set via config.", Some("Alex")), "B9: marked");
        assert!(!names_me("Done.", Some("alex")));
    }

    fn ticket(column: ColumnKind, owner: bool) -> TicketView {
        TicketView {
            id: TicketId::new(),
            number: 42,
            project_id: aurelius_domain::ProjectId::new(),
            project_name: "demo".into(),
            column_id: aurelius_domain::BoardColumnId::new(),
            column,
            position: 0.0,
            title: "Login".into(),
            description: None,
            priority: aurelius_domain::ticket::TicketPriority::Normal,
            labels: vec![],
            deadline_ms: None,
            owner_employee_id: owner.then(aurelius_domain::EmployeeId::new),
            owner_name: owner.then(|| "Forge".into()),
            created_by_user: aurelius_domain::UserId::new(),
            created_by_username: "alex".into(),
            run: Default::default(),
            root_task_id: None,
            root_task_status: None,
            review_feedback: None,
            attachments: vec![],
            version: 1,
            created_at_ms: 0,
            updated_at_ms: 0,
            started_at_ms: None,
            done_at_ms: None,
            archived_at_ms: None,
            conversation_id: Some(ConversationId::new()),
            login_user: aurelius_domain::UserId::new(),
            login_username: "alex".into(),
            needs: None,
        }
    }

    #[test]
    fn a_post_without_at_reaches_the_owner_only_while_the_ticket_runs() {
        assert_eq!(composer_hint(&ticket(ColumnKind::Doing, true)), "Message Forge (owner) · @ to mention someone else");
        assert!(reaches_owner(&ticket(ColumnKind::Todo, true)) && reaches_owner(&ticket(ColumnKind::PlanReview, true)));
        // AUR-BUG-Q: the report in Review may ask a question; the answer goes to the owner.
        assert!(reaches_owner(&ticket(ColumnKind::Review, true)));
        assert_eq!(composer_hint(&ticket(ColumnKind::Review, true)), "Message Forge (owner) · @ to mention someone else");
        for column in [ColumnKind::Backlog, ColumnKind::Done] {
            assert_eq!(composer_hint(&ticket(column, true)), "No one is woken. @mention to ask someone.", "P-21: {column:?}");
        }
        assert!(!reaches_owner(&ticket(ColumnKind::Doing, false)));
        assert!(!writable(&ticket(ColumnKind::Done, true)), "MU2-01 AC3: read-only on Done");
        assert!(writable(&ticket(ColumnKind::Review, true)));
    }

    #[test]
    fn a_post_without_at_reaches_the_owner_while_the_tester_runs_the_tests() {
        let testing = ticket(ColumnKind::Testing, true);
        assert!(reaches_owner(&testing), "ticket #5: as in Review");
        assert_eq!(composer_hint(&testing), "Message Forge (owner) · @ to mention someone else");
        assert!(writable(&testing));
        let owner = testing.owner_employee_id.unwrap();
        assert_eq!(reached(Some(&testing), &[owner]), (true, vec![]));
    }

    #[test]
    fn an_archived_ticket_is_read_only_and_reaches_nobody() {
        let mut archived = ticket(ColumnKind::Backlog, true);
        archived.archived_at_ms = Some(1);
        assert!(!writable(&archived), "the server refuses a post in an archived ticket");
        assert_eq!(read_only_note(&archived, &Summary::Pending), "Archived · read-only. Follow-up work: create a new ticket.");
        // Archived from Todo with an owner: the owner no longer answers (owner_answers).
        archived.column = ColumnKind::Todo;
        assert!(!reaches_owner(&archived));
        assert_eq!(composer_hint(&archived), "No one is woken. @mention to ask someone.");
        assert!(reaches_owner(&ticket(ColumnKind::Todo, true)), "not archived: the owner answers");
        assert!(read_only_note(&ticket(ColumnKind::Done, true), &Summary::Saved).starts_with("Done · read-only. The summary is in project memory."));
    }

    #[test]
    fn the_owner_in_to_gets_it_in_the_main_task_and_the_others_are_woken() {
        let doing = ticket(ColumnKind::Doing, true);
        let owner = doing.owner_employee_id.unwrap();
        let sentinel = aurelius_domain::EmployeeId::new();
        // The server's `to` puts the owner first, whether it started a run for them or not:
        // a run of the owner is no "woken".
        assert_eq!(reached(Some(&doing), &[owner]), (true, vec![]));
        assert_eq!(reached(Some(&doing), &[owner, sentinel]), (true, vec![sentinel]));
        // F3: the main task is terminal, the owner is not in `to`: nobody took it.
        assert_eq!(reached(Some(&doing), &[]), (false, vec![]));
        assert_eq!(reached(Some(&doing), &[sentinel]), (false, vec![sentinel]));
        // In Review the owner takes it in the main task, as in Doing.
        let review = ticket(ColumnKind::Review, true);
        let owner = review.owner_employee_id.unwrap();
        assert_eq!(reached(Some(&review), &[owner]), (true, vec![]));
        // In Backlog an @owner wakes the owner in the discussion, like anyone.
        let backlog = ticket(ColumnKind::Backlog, true);
        let owner = backlog.owner_employee_id.unwrap();
        assert_eq!(reached(Some(&backlog), &[owner]), (false, vec![owner]));
        // The ticket is gone from the board: everyone named is woken.
        assert_eq!(reached(None, &[sentinel]), (false, vec![sentinel]));
        assert_eq!(posted_text(&[], true), "Sent. The owner gets it in the main task.");
        assert_eq!(posted_text(&["Sentinel".into()], true), "Sent. The owner gets it in the main task; Sentinel is woken.");
        assert_eq!(posted_text(&["Sentinel".into(), "Schema".into()], false), "Sent. Sentinel, Schema are woken.");
    }

    #[test]
    fn nobody_woken_is_a_system_line_in_the_servers_words() {
        // F3: "Nobody was woken…" comes as a system line (meta.system); it reads as the
        // server wrote it and as a system line, not as a message of "Aurelius".
        let text = "Nobody was woken: the owner only answers here while the ticket is in Todo, Doing, Plan review or Review. Name a colleague with @Name.";
        let nobody = line(SystemLineKind::Limit, text);
        let s = nobody.system.as_ref().expect("a system line");
        assert_eq!(system_text(s, &nobody.text), text);
        assert_eq!(system_task(s), None, "no Open task on it");
        assert!(!can_make_ticket(&nobody));
        assert_eq!(summary_state(std::slice::from_ref(&nobody)), Summary::Pending);
    }

    #[test]
    fn create_ticket_needs_a_server_with_the_discussion() {
        let m = msg(MessageRole::Assistant, "Can we add an audit export?");
        assert!(offers_ticket(&m, true, true));
        assert!(!offers_ticket(&m, true, false), "a phase 1 server refuses from_message (deny_unknown_fields)");
        assert!(!offers_ticket(&m, false, true), "B6: only a board member");
        let id = MessageId::new();
        assert_eq!(from_message(Some(id), true), Some(id));
        assert_eq!(from_message(Some(id), false), None);
    }

    #[test]
    fn the_discussion_follows_its_bottom_only_while_it_stands_there() {
        assert!(follows(true, false, false, false), "just opened: at the bottom");
        assert!(follows(false, true, true, false), "after your own post");
        assert!(follows(false, false, true, true), "a new message or Limit line while at the bottom");
        assert!(!follows(false, false, true, false), "scrolled up to read: it stays");
        assert!(!follows(false, false, false, true), "nothing new: no jump");
    }

    #[test]
    fn the_tab_counts_what_came_since_it_was_seen() {
        assert_eq!(unread(5, Some(2), false), 3);
        assert_eq!(unread(5, Some(2), true), 0, "showing: read");
        assert_eq!(unread(5, None, false), 0, "never opened: no count");
        let streaming = MessageId::new();
        assert_eq!(answering(&[], Some((&streaming, Some("Forge".into())))), Some("Forge is answering…".into()));
        let mut done = msg(MessageRole::Assistant, "x");
        done.id = streaming;
        assert_eq!(answering(&[done], Some((&streaming, None))), None, "it is a message now");
    }

    #[test]
    fn unknown_method_keeps_the_draft_and_says_to_update_the_server() {
        let old = aurelius_domain::error::AppError::new(ErrorCode::UnknownMethod, "This server does not know the method ticket.post.");
        assert!(post_failure(&ClientError::Protocol(old.clone())).contains("Update the server; your text stays here"));
        assert!(retry_failure(&ClientError::Protocol(old)).contains("Update the server"));
        let refused = aurelius_domain::error::AppError::new(ErrorCode::Validation, "The discussion of a ticket on Done is read-only.");
        assert_eq!(post_failure(&ClientError::Protocol(refused)), "The discussion of a ticket on Done is read-only.");
        assert!(post_failure(&ClientError::Disconnected).contains("Your text stays here"));
        assert_eq!(posted_text(&["Sentinel".into()], false), "Sent. Sentinel is woken.");
        assert_eq!(posted_text(&[], false), "Sent. No one is woken.");
    }

    fn channel(name: &str, project: bool, archived: bool, unread: u32, mentions: u32) -> ChannelView {
        ChannelView { conversation_id: ConversationId::new(), project_id: project.then(aurelius_domain::ProjectId::new), name: name.into(), unread, mentions, archived, last_activity_ms: None, kind: Default::default(), ticket_id: None, ticket_number: None, ticket_column: None, member: false, member_count: None, read_through: None }
    }

    fn ticket_channel(number: u32, archived: bool, last: i64, unread: u32, mentions: u32) -> ChannelView {
        let name = aurelius_domain::views::ticket_channel_name(number, "Login per board member");
        ChannelView { kind: ChannelKind::Ticket, ticket_id: Some(TicketId::new()), ticket_number: Some(number), ticket_column: Some(if archived { ColumnKind::Done } else { ColumnKind::Review }), last_activity_ms: Some(last), ..channel(&name, true, archived, unread, mentions) }
    }

    #[test]
    fn channels_are_general_then_projects_by_name_then_archived() {
        let list = vec![channel("web", true, false, 0, 0), channel("old", true, true, 0, 0), channel("General", false, false, 0, 0), channel("Aurelius", true, false, 3, 1)];
        let s = sections(&list);
        assert_eq!(s.general.map(channel_title).as_deref(), Some("# general"));
        assert_eq!(s.projects.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["Aurelius", "web"]);
        assert_eq!(s.archived.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["old"]);
        assert!(s.tickets.is_empty(), "a server without ticket channels: the list of before");
        assert_eq!(channel_label(&list[3]), "Aurelius, 3 unread, 1 mention");
        assert_eq!(channel_label(&list[1]), "old, archived, read-only");
        assert_eq!(badge(&list), (1, true), "B8: mentions count, a dot for unread");
        assert_eq!(go_to_channel_label(&list[2]), "Go to #general");
        assert_eq!(go_to_channel_label(&list[3]), "Go to #Aurelius");
        assert_eq!(badge(&[channel("General", false, false, 0, 0)]), (0, false));
        assert_eq!(channel_hint(Some(&list[3])), "Message #Aurelius · @ to mention");
        assert_eq!(channel_hint(Some(&list[1])), "This project is archived · read-only.", "MU2-08 AC2");
    }

    /// Q1 minor: reading the only channel with a mention clears the sidebar's "@1" at
    /// once, and the open channel keeps adding nothing while a list says otherwise.
    #[test]
    fn reading_a_channel_clears_its_part_of_the_sidebar_badge() {
        let mut list = vec![channel("General", false, false, 0, 0), channel("Aurelius", true, false, 2, 1), ticket_channel(3, false, 100, 4, 0)];
        assert_eq!(badge(&list), (1, true));
        let aurelius = list[1].conversation_id;
        mark_read(&mut list, aurelius);
        assert_eq!(badge(&list), (0, true), "the mention is read; #3 stays unread");
        assert_eq!((list[2].unread, list[2].mentions), (4, 0), "another channel keeps its counts");
        let three = list[2].conversation_id;
        mark_read(&mut list, three);
        assert_eq!(badge(&list), (0, false), "nothing unread: no dot");
    }

    #[test]
    fn open_tickets_are_their_own_section_newest_activity_first_and_closed_ones_are_archived() {
        let list = vec![channel("General", false, false, 0, 0), ticket_channel(42, false, 100, 5, 0), ticket_channel(52, false, 300, 0, 1), ticket_channel(47, false, 100, 0, 0), ticket_channel(12, true, 900, 0, 0), channel("old", true, true, 0, 0)];
        let s = sections(&list);
        assert_eq!(s.tickets.iter().map(|c| c.ticket_number).collect::<Vec<_>>(), [Some(52), Some(47), Some(42)], "same moment: the higher number first");
        assert!(s.projects.is_empty(), "a ticket channel has a project but is no project channel");
        assert_eq!(s.archived.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["old", "#12-login-per-board-member"], "archived projects, then closed tickets");
        assert_eq!(channel_title(&list[1]), "# 42-login-per-board-member");
        assert_eq!(go_to_channel_label(&list[1]), "Go to #42-login-per-board-member");
        assert_eq!(channel_label(&list[1]), "Ticket 42, login-per-board-member, 5 unread");
        assert_eq!(channel_hint(Some(&list[1])), "Message #42-login-per-board-member · @ to mention");
        assert_eq!(channel_hint(Some(&list[4])), "#12 is Done · read-only.");
        let archived = ChannelView { ticket_column: Some(ColumnKind::Doing), ..list[4].clone() };
        assert_eq!(read_only_bar(&archived), "#12 is archived · read-only.");
        // A kind of a newer server is listed with the channels, never lost.
        let newer = ChannelView { kind: ChannelKind::Unknown, ..channel("huddle", true, false, 0, 0) };
        assert_eq!(kind_of(&newer), ChannelKind::Project);
    }

    #[test]
    fn tickets_and_archived_show_fifteen_rows_before_show_more() {
        let list: Vec<ChannelView> = (1..=20).map(|n| ticket_channel(n, false, i64::from(n), 0, 0)).chain((21..=40).map(|n| ticket_channel(n, true, i64::from(n), 0, 0))).collect();
        let s = sections(&list);
        assert_eq!(SHOW_MORE_AFTER, 15);
        assert_eq!((shown(20, false), shown(20, true), shown(3, false)), (15, 20, 3));
        assert_eq!(visible(&s, false, false, false).len(), 15, "archived folded away");
        assert_eq!(visible(&s, true, false, false).len(), 20);
        assert_eq!(visible(&s, false, true, false).len(), 30);
        assert_eq!(visible(&s, true, true, true).len(), 40);
    }

    #[test]
    fn the_ticket_order_stays_put_while_the_pointer_is_over_the_list() {
        let list = vec![ticket_channel(1, false, 10, 0, 0), ticket_channel(2, false, 20, 0, 0), ticket_channel(3, false, 30, 0, 0)];
        let frozen = [list[0].conversation_id, list[1].conversation_id];
        let kept = keep_order(sections(&list).tickets, &frozen);
        assert_eq!(kept.iter().map(|c| c.ticket_number).collect::<Vec<_>>(), [Some(1), Some(2), Some(3)], "a newly active one goes below");
    }

    #[test]
    fn alt_arrows_walk_the_list_and_stop_at_its_ends() {
        let list = vec![channel("b", true, false, 0, 0), channel("General", false, false, 0, 0), channel("a", true, false, 0, 0), ticket_channel(7, false, 1, 0, 0), channel("old", true, true, 0, 0)];
        let (g, a, b, t, old) = (list[1].conversation_id, list[2].conversation_id, list[0].conversation_id, list[3].conversation_id, list[4].conversation_id);
        let folded = visible(&sections(&list), false, false, false);
        assert_eq!(step_channel(&folded, Some(g), true), Some(a));
        assert_eq!(step_channel(&folded, Some(a), true), Some(b));
        assert_eq!(step_channel(&folded, Some(b), true), Some(t), "Channels, then Tickets");
        assert_eq!(step_channel(&folded, Some(t), true), Some(t), "the end: Archived is folded away");
        assert_eq!(step_channel(&visible(&sections(&list), false, true, false), Some(t), true), Some(old));
        assert_eq!(step_channel(&folded, Some(g), false), Some(g));
        assert_eq!(step_channel(&folded, None, true), Some(a), "none open: from general");
        assert_eq!(step_channel(&[], None, true), None);
        assert_eq!(new_divider(10, 3), Some(7));
        assert_eq!(new_divider(10, 0), None);
        assert_eq!(new_divider(2, 5), None, "more unread than shown: no divider");
    }

    #[test]
    fn a_ticket_channels_header_names_the_owner_first_then_the_other_employees_once() {
        use crate::screens::tasks::tests::task;
        let t = ticket(ColumnKind::Doing, true);
        let staffed = |name: &str| {
            let mut task = task(t.project_id, None, aurelius_domain::task::TaskStatus::Running, 0);
            task.employee_name = Some(name.into());
            task
        };
        let owner = t.owner_name.clone().unwrap_or_default();
        let detail = aurelius_domain::ticket::TicketDetail { as_of_seq: 1, ticket: t.clone(), root_task: Some(staffed(&owner)), tasks: vec![staffed(&owner), staffed("Schema"), staffed("Schema"), staffed("Pulse")] };
        assert_eq!(assignees(&detail), [owner.as_str(), "Schema", "Pulse"]);
        let unowned = aurelius_domain::ticket::TicketDetail { ticket: ticket(ColumnKind::Backlog, false), root_task: None, tasks: vec![], ..detail };
        assert!(assignees(&unowned).is_empty());
    }

    #[test]
    fn a_message_fills_the_new_ticket_form_and_is_not_made_twice_unasked() {
        let p = prefill("## Audit export\nCan we add an audit export?", "Atlas", "#demo", "10:12");
        assert_eq!(p.title, "Audit export");
        assert_eq!(p.description, "> ## Audit export\n> Can we add an audit export?\n\n— Atlas in #demo · 10:12");
        let long = "x".repeat(200);
        assert_eq!(prefill(&long, "a", "b", "c").title.chars().count(), TITLE_MAX);
        let source = msg(MessageRole::Assistant, "Can we add an audit export?");
        assert!(can_make_ticket(&source));
        let mut created = line(SystemLineKind::TicketCreated, "alex created #52 from a message");
        let ticket = TicketId::new();
        if let Some(s) = created.system.as_mut() {
            s.message_id = Some(source.id);
            s.ticket_id = Some(ticket);
        }
        assert!(!can_make_ticket(&created), "a system line is no source");
        let messages = vec![source.clone(), created];
        assert_eq!(ticket_from(&messages, source.id), Some((Some(ticket), "#52".into())), "the → #52 pill and Create another?");
        assert_eq!(ticket_from(&messages, MessageId::new()), None);
        assert_eq!(ticket_number("see #7 and #8"), Some(7));
    }
}
