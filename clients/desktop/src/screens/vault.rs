//! The vault (feature `vault`, Fase 4; ui-ontwerp Fase 4 §1–§6).
//!
//! * One page with a scope: the company or a project (MU4-01, MU4-02). A project
//!   lists its own items and, apart, the company's that its runs get too; a project
//!   item with a company item's key wears the pill "overrides Company" (B4, P-42).
//!   The project's own tab Vault shows the same on its scope.
//! * Text and url show their value; a secret is masked everywhere (MU4-03). Show is
//!   one click: the value shows for 30 s, until Hide, or until the page is left
//!   (B6, P-43, P-47). Copy never shows it, and clears the clipboard after 60 s, but
//!   only when it still holds our value (E19). Both are written to the audit log.
//! * New item and edit: a secret is never filled in, "leave empty to keep"; the type
//!   moves between text and url only, never to or from secret (B7, P-44).
//! * The audit log of a scope, of every scope, and of one item (History; MU4-09).
//! * The vault key (B8, E9): asked with the own password, saved to a file of mode 0600
//!   or shown once.
//! * B10: a message the server holds back because it holds a vault secret's value
//!   asks "Send anyway?" with the items' names; it goes again with `confirm_secret`.
//!
//! A value only lives in a [`backend::Revealed`]: never printed, wiped when dropped,
//! and gone from this state once it hides (E19, E20). Nothing here logs.

use crate::app::{AppView, ConfirmKind, Overlay, Page, Tab};
use crate::backend::{self, AuditAsk, Password, Revealed, SecretHold, VaultCall, VaultUpdate};
use crate::theme::{self, c};
use crate::ui::{self, ButtonKind, h, v};
use aurelius_domain::vault::{RevealPurpose, VaultActor, VaultAuditAction, VaultAuditRow, VaultItemView, VaultKind, VaultList};
use aurelius_domain::{ProjectId, VaultItemId};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::hash::BuildHasher;
use std::time::{Duration, Instant};

/// How long Show shows a value (B6, P-47).
pub const SHOW_FOR: Duration = Duration::from_secs(30);
/// When Copy clears the clipboard, if it still holds our value (B6, E19).
pub const CLEAR_CLIPBOARD_AFTER: Duration = Duration::from_secs(60);
/// Rows per read of the audit log.
pub const AUDIT_PAGE: u32 = 50;
/// The list's columns: the value keeps at least `VALUE_MIN_W` (Show and Copy), the others give way.
const NAME_W: f32 = 170.;
const KEY_W: f32 = 150.;
const UPDATED_W: f32 = 130.;
const VALUE_MIN_W: f32 = 140.;
/// What a secret shows instead of its value.
pub const MASK: &str = "••••••••";

/// E21 (R2): plugins and MCP servers inherit a run's environment.
pub const PLUGINS_SEE_SECRETS: &str = "Plugins and MCP servers of an employee see the secrets of its runs.";
/// E21 (R3): the key is on the server's disk.
pub const KEY_ON_DISK: &str = "On Linux, any process of the daemon's user, also an agent through Bash, can read the key.";
/// Under the editor (ui-ontwerp §4).
pub const EVERYONE_READS: &str = "All employees can read this item. Secrets reach runs only as an environment variable, never in a prompt, memory or discussion.";
pub const SHOW_TIP: &str = "Shows the value for 30 s. This is logged.";
pub const COPY_TIP: &str = "Copies without showing it, and clears the clipboard after 60 s. This is logged.";
pub const COPIED: &str = "Copied. Clears in 60 s.";
pub const UNDECRYPTABLE: &str = "Cannot be decrypted";
pub const KEEP_SECRET: &str = "Leave empty to keep the current secret";
/// Under a text or url value (m9, Sentinel sec1): only a secret is masked and redacted.
pub const NOT_SECRET: &str = "Not secret: shown to everyone and not redacted. Use Secret for keys and tokens.";
pub const URL_PROBLEM: &str = "Enter a full URL, starting with https://";
pub const DELETE_BODY: &str = "Runs and employees lose access, and runs that start from now on no longer get it. A run that already got it keeps it until it ends. This is logged.";
pub const HELD_BODY: &str = "Nothing was sent. Send anyway stores it with the value replaced by «secret:KEY», the item's key; Cancel puts it back to edit.";
/// A task, a recurring task or an idea whose dialog closed: Cancel has nothing to put back.
pub const HELD_FORM_BODY: &str = "Nothing was saved. Send anyway saves it with the value replaced by «secret:KEY», the item's key; Cancel drops it, so write it again without the value.";

/// What the "Send anyway?" dialog says for a held command.
pub fn held_body(command: Option<&aurelius_domain::Command>) -> &'static str {
    match command.map(held_in) {
        Some(HeldIn::Form) => HELD_FORM_BODY,
        _ => HELD_BODY,
    }
}
pub const KEY_ABOUT: &str = "Restoring a backup on another machine needs this key: without it the secrets cannot be decrypted. Keep it apart from the backups. Every export is logged and every board member is told.";

/// "Delete GitHub token (GITHUB_TOKEN)?" (ui-ontwerp §4, MU4-11 AC2).
pub fn delete_title(item: Option<&VaultItemView>) -> String {
    match item {
        Some(i) => format!("Delete {} ({})?", i.name, i.env_name),
        None => "Delete this item?".into(),
    }
}

/// The B10 question with the items' names (never a value; P-46).
pub fn held_title(names: &[String]) -> String {
    match names {
        [] => "This looks like a vault secret. Send anyway?".into(),
        names => format!("This looks like a vault secret: {}. Send anyway?", names.join(", ")),
    }
}

/// Where a held message was written (B10), so Cancel puts it back there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeldIn {
    /// A task's or an employee's conversation.
    Chat(aurelius_domain::ConversationId),
    /// The Team chat: a project's channel, or the general one.
    Channel(Option<aurelius_domain::ConversationId>),
    /// A ticket's discussion or the New ticket form, or a ticket's field: its draft stays where it was.
    Ticket,
    /// The feedback of Pas aan or Send back: back in the ticket's feedback field.
    Feedback(aurelius_domain::TicketId),
    /// A plan decision's feedback: back in the plan's feedback field.
    Plan,
    /// A dialog that closed (a task, a recurring task, an idea) or a question's answers, which stay: nothing comes back.
    Form,
}

pub fn held_in(command: &aurelius_domain::Command) -> HeldIn {
    use aurelius_domain::Command;
    match command {
        Command::SendEmployeeMessage(m) => HeldIn::Chat(m.conversation_id),
        Command::SendGroupMessage(m) => HeldIn::Channel(m.channel),
        Command::PostTicketMessage(_) | Command::CreateTicket(_) | Command::UpdateTicket(_) | Command::AcceptTicket(_) => HeldIn::Ticket,
        Command::RevisePlan(c) => HeldIn::Feedback(c.ticket_id),
        Command::SendBackTicket(c) => HeldIn::Feedback(c.ticket_id),
        Command::DecidePlan(_) => HeldIn::Plan,
        _ => HeldIn::Form,
    }
}

/// The held text back in a composer: alone when it is empty, else after what is typed there now (nothing is lost).
pub fn text_back(now: &str, held: &str) -> String {
    match now.trim() {
        "" => held.to_string(),
        now => format!("{now}\n\n{held}"),
    }
}

/// The composer's text after Cancel, once the window shows `place`, where the held
/// message was sent from (AUR-CHAT-001 R2 D-M1): the composer still holds the channel
/// it came from, so that text first stays with that channel's draft, and the held
/// text goes back into `place`'s own draft.
pub fn held_back_in(drafts: &mut crate::screens::discussion::Drafts, place: crate::screens::discussion::DraftPlace, composer: &str, held: &str) -> String {
    let now = drafts.moved(place, composer).unwrap_or_else(|| composer.to_string());
    text_back(&now, held)
}

/// The images and files of a held message, back beside the composer (those still there, once each).
pub fn attachments_back(command: &aurelius_domain::Command) -> (Vec<std::path::PathBuf>, Vec<std::path::PathBuf>) {
    use aurelius_domain::Command;
    let (images, files) = match command {
        Command::SendEmployeeMessage(m) => (&m.images, &m.files),
        Command::SendGroupMessage(m) => (&m.images, &m.files),
        _ => return (Vec::new(), Vec::new()),
    };
    let paths = |list: &Vec<String>| list.iter().map(std::path::PathBuf::from).collect();
    (paths(images), paths(files))
}

/// The key a name suggests ("GitHub token" → `GITHUB_TOKEN`), when the Key field is left empty.
pub fn env_from_name(name: &str) -> String {
    let mut out = String::new();
    for ch in name.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_uppercase());
        } else if !out.is_empty() && !out.ends_with('_') {
            out.push('_');
        }
    }
    let out = out.trim_start_matches(|ch: char| !ch.is_ascii_uppercase());
    let out: String = out.chars().take(aurelius_domain::vault::MAX_ENV_NAME_CHARS).collect();
    out.trim_end_matches('_').to_string()
}

/// The server's own check (E7): UPPER_SNAKE and not a name a run keeps.
pub fn env_problem(env: &str) -> Option<String> {
    aurelius_domain::vault::check_env_name(env).err()
}

/// The value's problem, by the server's rules; `keeping`: an edited secret whose field is empty keeps its value.
pub fn value_problem(kind: VaultKind, value: &str, keeping: bool) -> Option<String> {
    let value = value.trim();
    if keeping && value.is_empty() {
        return None;
    }
    aurelius_domain::vault::check_value(kind, value).err()
}

/// The hint under a text or url value: it is no secret (m9).
pub fn value_hint(kind: VaultKind) -> String {
    match kind {
        VaultKind::Url => format!("https://… · {NOT_SECRET}"),
        _ => NOT_SECRET.to_string(),
    }
}

/// The types an item may take: all three when new; text and url trade places; a secret stays one (B7).
pub fn kinds_for(current: Option<VaultKind>) -> Vec<VaultKind> {
    let all = [VaultKind::Text, VaultKind::Url, VaultKind::Secret];
    match current {
        None => all.to_vec(),
        Some(kind) => all.into_iter().filter(|to| kind.may_become(*to)).collect(),
    }
}

pub fn kind_label(kind: VaultKind) -> &'static str {
    match kind {
        VaultKind::Text => "Text",
        VaultKind::Url => "URL",
        VaultKind::Secret => "Secret",
    }
}

/// The type as an icon plus its word (ui-ontwerp §6: never the icon alone).
pub fn kind_tag(kind: VaultKind) -> &'static str {
    match kind {
        VaultKind::Text => "¶ Text",
        VaultKind::Url => "🔗 URL",
        VaultKind::Secret => "🔒 Secret",
    }
}

/// What the Value column shows. A secret's value only while it is shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cell {
    Masked,
    Shown(String),
    Text(String),
    Link(String),
    Undecryptable,
}

pub fn cell(item: &VaultItemView, shown: Option<&str>) -> Cell {
    if item.undecryptable {
        return Cell::Undecryptable;
    }
    match (item.kind, shown, item.value.as_deref()) {
        (VaultKind::Secret, Some(value), _) => Cell::Shown(value.to_string()),
        (VaultKind::Secret, None, _) => Cell::Masked,
        (VaultKind::Url, _, Some(url)) => Cell::Link(url.to_string()),
        (_, _, value) => Cell::Text(value.unwrap_or_default().lines().next().unwrap_or_default().to_string()),
    }
}

/// "Hides in 0:27" while a value shows.
pub fn hides_in(until: Instant, now: Instant) -> Option<String> {
    let left = until.checked_duration_since(now).filter(|d| !d.is_zero())?;
    let secs = left.as_secs() + u64::from(left.subsec_nanos() > 0);
    Some(format!("Hides in {}:{:02}", secs / 60, secs % 60))
}

/// A value shown on the page, until `until`.
#[derive(Debug)]
pub struct Shown {
    pub item_id: VaultItemId,
    pub value: Revealed,
    pub until: Instant,
}

/// What Copy put on the clipboard, known only by a keyed hash: the value itself is not kept (E19).
pub struct Copied {
    hasher: std::collections::hash_map::RandomState,
    digest: u64,
    pub epoch: u64,
}

impl Copied {
    pub fn of(value: &str, epoch: u64) -> Self {
        let hasher = std::collections::hash_map::RandomState::new();
        let digest = hasher.hash_one(value);
        Self { hasher, digest, epoch }
    }

    /// The clipboard still holds our value: then (and only then) it is cleared.
    pub fn is_ours(&self, clipboard: Option<&str>) -> bool {
        clipboard.is_some_and(|text| self.hasher.hash_one(text) == self.digest)
    }
}

impl std::fmt::Debug for Copied {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Copied").field("epoch", &self.epoch).finish_non_exhaustive()
    }
}

/// The page's tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VaultTab {
    #[default]
    Items,
    Audit,
}

/// New item (`item` none) or the one being edited.
#[derive(Debug, Clone, PartialEq)]
pub struct Editor {
    pub item: Option<VaultItemId>,
    pub scope: Option<ProjectId>,
    pub kind: VaultKind,
    pub busy: bool,
    pub failure: Option<String>,
}

/// The fields of the editor as typed (the value only for as long as the editor is open).
pub struct Typed<'a> {
    pub name: &'a str,
    pub env: &'a str,
    pub value: &'a str,
    pub note: &'a str,
}

impl Typed<'_> {
    /// The key: as typed, else from the name.
    pub fn env(&self) -> String {
        match self.env.trim() {
            "" => env_from_name(self.name),
            env => env.to_string(),
        }
    }
}

/// The edited item is no longer in the list (deleted elsewhere, or the list failed): Save must not make a new one.
pub const GONE_ELSEWHERE: &str = "This item was deleted or changed elsewhere.";

/// The editor's first problem: an edited item that is gone, the name, the key, then the value.
pub fn form_problem(editor: &Editor, item: Option<&VaultItemView>, typed: &Typed) -> Option<String> {
    if editor.item.is_some() && item.is_none() {
        return Some(GONE_ELSEWHERE.into());
    }
    if let Err(problem) = aurelius_domain::vault::check_name(typed.name) {
        return Some(problem);
    }
    if let Some(problem) = env_problem(&typed.env()) {
        return Some(problem);
    }
    let keeping = item.is_some_and(|i| i.kind == VaultKind::Secret);
    value_problem(editor.kind, typed.value, keeping)
}

/// `vault.create` from the editor.
pub fn create_params(editor: &Editor, typed: &Typed) -> aurelius_protocol::VaultCreateParams {
    aurelius_protocol::VaultCreateParams {
        project_id: editor.scope,
        kind: editor.kind,
        name: typed.name.trim().to_string(),
        env_name: typed.env(),
        description: Some(typed.note.trim().to_string()).filter(|n| !n.is_empty()),
        value: typed.value.trim().to_string(),
    }
}

/// `vault.update` with only what changed; none when nothing did. A secret's empty field keeps it.
pub fn update_params(editor: &Editor, item: &VaultItemView, typed: &Typed) -> Option<aurelius_protocol::VaultUpdateParams> {
    let changed = |now: &str, before: &str| (now != before).then(|| now.to_string());
    let name = changed(typed.name.trim(), &item.name);
    let env_name = changed(&typed.env(), &item.env_name);
    let description = changed(typed.note.trim(), item.description.as_deref().unwrap_or_default());
    let value = match item.kind {
        VaultKind::Secret => Some(typed.value.trim().to_string()).filter(|v| !v.is_empty()),
        _ => changed(typed.value.trim(), item.value.as_deref().unwrap_or_default()),
    };
    let kind = (editor.kind != item.kind).then_some(editor.kind);
    if name.is_none() && env_name.is_none() && description.is_none() && value.is_none() && kind.is_none() {
        return None;
    }
    Some(aurelius_protocol::VaultUpdateParams { item_id: item.id, name, env_name, description, value, kind })
}

/// An audit row's action in the log's words (ui-ontwerp §5).
pub fn action_label(action: VaultAuditAction) -> &'static str {
    match action {
        VaultAuditAction::Create => "Created",
        VaultAuditAction::Edit => "Edited",
        VaultAuditAction::Replace => "Replaced",
        VaultAuditAction::Archive => "Deleted",
        VaultAuditAction::Reveal => "Revealed",
        VaultAuditAction::Copy => "Copied",
        VaultAuditAction::Deliver => "Read by run",
        VaultAuditAction::ToolRead => "Read by tool",
        VaultAuditAction::KeyExport => "Exported the key",
    }
}

/// Who: "You", a board member's name, or "Forge (employee)".
pub fn who(actor: &VaultActor, me: Option<aurelius_domain::UserId>) -> String {
    match actor {
        VaultActor::BoardMember { user } if Some(user.id) == me => "You".into(),
        VaultActor::BoardMember { user } => user.username.clone(),
        VaultActor::Employee { name, .. } => format!("{name} (employee)"),
    }
}

/// Via: the board member's device, or the run (and its ticket) of an employee.
pub fn via(row: &VaultAuditRow) -> String {
    match (&row.actor, &row.via) {
        (_, Some(via)) => via.clone(),
        (VaultActor::Employee { ticket_id: Some(_), .. }, None) => "Ticket · run".into(),
        (VaultActor::Employee { run_id: Some(_), .. }, None) => "Run".into(),
        _ => String::new(),
    }
}

/// An edit's fields (never their values): "Edited · name, key", "Replaced · type" (§5, QA2 m4).
pub fn action_text(row: &VaultAuditRow) -> String {
    let fields: Vec<&str> = row.changed_fields.iter().filter(|f| *f != "value").map(|f| match f.as_str() {
        "env_name" => "key",
        "description" => "note",
        "kind" => "type",
        other => other,
    }).collect();
    match (row.action, fields.is_empty()) {
        (VaultAuditAction::Edit | VaultAuditAction::Replace, false) => format!("{} · {}", action_label(row.action), fields.join(", ")),
        (action, _) => action_label(action).into(),
    }
}

/// A page of the audit log after what shows (`before`), or the first page.
pub fn merge_audit(rows: &mut Vec<VaultAuditRow>, page: Vec<VaultAuditRow>, before: Option<i64>) {
    if before.is_none() {
        rows.clear();
    }
    for row in page {
        if !rows.iter().any(|r| r.id == row.id) {
            rows.push(row);
        }
    }
}

/// The scopes in the list: the company first, then the active projects and the archived
/// ones that still have items (MU4-02 AC3), each with its count.
pub fn scopes(list: Option<&VaultList>, projects: &[aurelius_protocol::ProjectSummary]) -> Vec<(Option<ProjectId>, String, u32)> {
    let count = |scope: Option<ProjectId>| list.and_then(|l| l.counts.iter().find(|c| c.project_id == scope)).map_or(0, |c| c.count);
    let mut out = vec![(None, "Company".to_string(), count(None))];
    for p in projects {
        if p.status == aurelius_domain::project::ProjectStatus::Active || count(Some(p.id)) > 0 {
            out.push((Some(p.id), p.name.clone(), count(Some(p.id))));
        }
    }
    out
}

/// The environment of a key line `aurelius-vault-key:v1:<env>:<key_id>:<base64url>` (E9).
pub fn key_env(key: &str) -> Option<&str> {
    let mut parts = key.split(':');
    (parts.next() == Some("aurelius-vault-key") && parts.next() == Some("v1")).then(|| parts.next()).flatten().filter(|env| matches!(*env, "dev" | "prod"))
}

/// The name of the saved key file, with its environment so a dev key is not taken for prod.
pub fn key_file_name(key_id: &str, key: &str) -> String {
    let id: String = key_id.chars().filter(|ch| ch.is_ascii_alphanumeric()).take(16).collect();
    match key_env(key) {
        Some(env) => format!("aurelius-vault-key-{env}-{id}.txt"),
        None => format!("aurelius-vault-key-{id}.txt"),
    }
}

/// Write the key to `path`, readable by this user only (0600, also when the file was there; E9).
pub fn save_key(path: &std::path::Path, key: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // A file that was there keeps its mode on open: set it on the descriptor.
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    file.write_all(key.as_bytes())?;
    file.write_all(b"\n")?;
    file.sync_all()
}

/// The key, shown once until `until` (or Done, or leaving the page). While the save dialog
/// is open its time stands still (`paused`: what was left), and it does not hide (QA2 m3).
#[derive(Debug)]
pub struct KeyShown {
    pub file: backend::KeyFile,
    pub until: Instant,
    pub paused: Option<Duration>,
}

impl KeyShown {
    pub fn new(file: backend::KeyFile, now: Instant) -> Self {
        Self { file, until: now + SHOW_FOR, paused: None }
    }

    /// The save dialog opens: the countdown stops where it is.
    pub fn pause(&mut self, now: Instant) {
        if self.paused.is_none() {
            self.paused = Some(self.until.saturating_duration_since(now));
        }
    }

    /// The save dialog closed (saved or not): the countdown goes on from where it stopped.
    pub fn resume(&mut self, now: Instant) {
        if let Some(left) = self.paused.take() {
            self.until = now + left;
        }
    }

    /// "Hides in 0:12", standing still while paused.
    pub fn hides_in(&self, now: Instant) -> Option<String> {
        match self.paused {
            Some(left) => hides_in(now + left, now),
            None => hides_in(self.until, now),
        }
    }
}

/// What the vault shows now: at most one value and the key, each until its time.
#[derive(Debug, Default)]
pub struct Showing {
    pub shown: Option<Shown>,
    pub key: Option<KeyShown>,
}

impl Showing {
    /// The value shown for `id`, if any.
    pub fn shown_value(&self, id: VaultItemId) -> Option<&str> {
        self.shown.as_ref().filter(|s| s.item_id == id).map(|s| s.value.expose())
    }

    /// Hide what shows: the value and the key leave this state, wiped (E19).
    pub fn hide(&mut self) {
        self.shown = None;
        self.key = None;
    }

    /// Hide what is due at `now`; true when something hid.
    pub fn hide_due(&mut self, now: Instant) -> bool {
        let mut hid = false;
        if self.shown.as_ref().is_some_and(|s| s.until <= now) {
            self.shown = None;
            hid = true;
        }
        if self.key.as_ref().is_some_and(|k| k.paused.is_none() && k.until <= now) {
            self.key = None;
            hid = true;
        }
        hid
    }

    pub fn showing(&self) -> bool {
        self.shown.is_some() || self.key.is_some()
    }

    /// The window lost the focus (another app, a locked screen): hide what shows (§3, D-m7),
    /// but not the key while its own save dialog has the focus (QA2 m3).
    pub fn hide_on_leave(&mut self) -> bool {
        let hid = self.shown.is_some() || self.key.as_ref().is_some_and(|k| k.paused.is_none());
        self.shown = None;
        if self.key.as_ref().is_some_and(|k| k.paused.is_none()) {
            self.key = None;
        }
        hid
    }
}

/// What the screen reader is told when a value shows or hides (§6).
pub const SHOWN_ANNOUNCE: &str = "Shown for 30 seconds";
pub const HIDDEN_ANNOUNCE: &str = "Hidden";

/// The vault's state in a window.
pub struct Vault {
    /// The scope the page shows (none: the company).
    pub scope: Option<ProjectId>,
    pub tab: VaultTab,
    /// The last list, for its scope.
    pub list: Option<(Option<ProjectId>, Result<VaultList, String>)>,
    /// The scope asked for last; a change of scope or an event asks again.
    pub asked: Option<Option<ProjectId>>,
    /// The project's company items show (they are folded by default).
    pub company_open: bool,
    /// The audit log on its tab: which, its rows, whether older ones exist, why it did not load.
    pub audit_all: bool,
    pub audit_asked: Option<AuditAsk>,
    pub audit: Vec<VaultAuditRow>,
    pub audit_more: bool,
    pub audit_error: Option<String>,
    /// One item's history in the editor.
    pub history: Option<(VaultItemId, Result<Vec<VaultAuditRow>, String>)>,
    /// What shows now: a value or the key (and nothing once it hid).
    pub out: Showing,
    /// A Show or Copy waits for the server (its button says so).
    pub revealing: Option<(VaultItemId, RevealPurpose)>,
    pub copied: Option<Copied>,
    pub copy_epoch: u64,
    pub editor: Option<Editor>,
    pub name: Entity<InputState>,
    pub env: Entity<InputState>,
    pub value: Entity<InputState>,
    pub note: Entity<InputState>,
    /// The value field shows what is typed (the editor's eye; local only, never logged).
    pub value_visible: bool,
    /// The key export: open, waiting, refused, shown.
    pub key_open: bool,
    pub key_busy: bool,
    pub key_error: Option<String>,
    pub password: Entity<InputState>,
    /// Empty the editor's (and the password's) fields on the next frame (an update has no window).
    pub clear_fields: bool,
    pub ticking: bool,
    /// A message B10 held back, while "Send anyway?" asks; the next ones wait their turn.
    pub held: Option<SecretHold>,
    pub held_next: std::collections::VecDeque<SecretHold>,
    /// What the screen reader is told now (a `Role::Status`), with a counter so a repeat is new (§6).
    pub announce: Option<(u64, &'static str)>,
}

impl Vault {
    pub fn new(window: &mut Window, cx: &mut Context<AppView>) -> Self {
        Self {
            scope: None,
            tab: VaultTab::Items,
            list: None,
            asked: None,
            company_open: false,
            audit_all: false,
            audit_asked: None,
            audit: Vec::new(),
            audit_more: false,
            audit_error: None,
            history: None,
            out: Showing::default(),
            revealing: None,
            copied: None,
            copy_epoch: 0,
            editor: None,
            name: cx.new(|cx| InputState::new(window, cx).placeholder("GitHub token")),
            env: cx.new(|cx| InputState::new(window, cx).placeholder("From the name")),
            value: cx.new(|cx| InputState::new(window, cx)),
            note: cx.new(|cx| InputState::new(window, cx).placeholder("Employees see this; never a secret")),
            value_visible: false,
            key_open: false,
            key_busy: false,
            key_error: None,
            password: cx.new(|cx| InputState::new(window, cx).masked(true).placeholder("Your password")),
            clear_fields: false,
            ticking: false,
            held: None,
            held_next: std::collections::VecDeque::new(),
            announce: None,
        }
    }

    /// The editor repaints as its fields change (the derived key, the problems); Ctrl+Enter
    /// saves; Enter in the password exports.
    pub fn subscriptions(&self, window: &mut Window, cx: &mut Context<AppView>) -> Vec<Subscription> {
        let mut out = Vec::new();
        for input in [&self.name, &self.env, &self.value, &self.note] {
            out.push(cx.subscribe_in(input, window, |this: &mut AppView, _, event: &InputEvent, _, cx| match event {
                InputEvent::Change => {
                    if let Some(e) = this.vault.editor.as_mut() {
                        e.failure = None;
                    }
                    cx.notify();
                }
                InputEvent::PressEnter { secondary: true, .. } => this.vault_save(cx),
                _ => {}
            }));
        }
        out.push(cx.subscribe_in(&self.password, window, |this: &mut AppView, _, event: &InputEvent, _, cx| {
            if let InputEvent::PressEnter { .. } = event {
                this.vault_export_key(cx);
            }
        }));
        // Another app or a locked screen takes the focus: what shows hides (§3, D-m7).
        out.push(cx.observe_window_activation(window, |this: &mut AppView, window, cx| {
            if !window.is_window_active() && this.vault.out.hide_on_leave() {
                this.vault_announce(HIDDEN_ANNOUNCE);
                cx.notify();
            }
        }));
        out
    }

    /// An item of the list in view (the scope's or the company's).
    pub fn item(&self, id: VaultItemId) -> Option<&VaultItemView> {
        let (_, Ok(list)) = self.list.as_ref()? else { return None };
        list.items.iter().chain(&list.company_items).find(|i| i.id == id)
    }

}

impl AppView {
    /// The vault shows (its page, or a project's tab Vault).
    pub fn vault_on_screen(&self) -> bool {
        matches!(self.screen, Page::Vault | Page::Project(_, Tab::Vault))
    }

    fn vault_ready(&self) -> bool {
        matches!(&self.connection, backend::Connection::Online { server, .. } if server.vault)
    }

    /// A new server, or another board member: nothing of the vault stays.
    pub fn vault_reset(&mut self) {
        let v = &mut self.vault;
        v.out.hide();
        v.list = None;
        v.asked = None;
        v.audit.clear();
        v.audit_asked = None;
        v.audit_error = None;
        v.history = None;
        v.revealing = None;
        v.editor = None;
        v.key_open = false;
        v.key_busy = false;
        v.key_error = None;
        v.held = None;
        v.held_next.clear();
        v.clear_fields = true;
    }

    fn vault_send(&self, call: VaultCall) {
        let _ = self.requests_handle().send(backend::Request::Vault(call));
    }

    /// Ask for the scope's list (and the audit log on its tab) once; an event asks again.
    fn vault_want(&mut self, scope: Option<ProjectId>) {
        if self.vault.asked != Some(scope) {
            self.vault.asked = Some(scope);
            self.vault_send(VaultCall::List(scope));
        }
        if self.vault.tab == VaultTab::Audit {
            let ask = AuditAsk { project_id: if self.vault.audit_all { None } else { scope }, all: self.vault.audit_all, item_id: None };
            if self.vault.audit_asked != Some(ask) {
                self.vault.audit_asked = Some(ask);
                self.vault.audit_more = false;
                self.vault.audit_error = None;
                self.vault_send(VaultCall::Audit { ask, before: None });
            }
        }
    }

    pub fn apply_vault(&mut self, update: VaultUpdate, cx: &mut Context<Self>) {
        match update {
            VaultUpdate::Listed(scope, result) => {
                if self.vault.asked == Some(scope) {
                    self.vault.list = Some((scope, result));
                }
            }
            VaultUpdate::Value { item_id, purpose, result } => {
                self.vault.revealing = None;
                match (purpose, result) {
                    // Shown only while the vault is on screen: a late answer after leaving is dropped (and wiped).
                    (RevealPurpose::Show, Ok(value)) if self.vault_on_screen() => {
                        self.vault.out.shown = Some(Shown { item_id, value, until: Instant::now() + SHOW_FOR });
                        self.vault_announce(SHOWN_ANNOUNCE);
                        self.vault_tick(cx);
                    }
                    (RevealPurpose::Show, Ok(_)) => {}
                    (RevealPurpose::Copy, Ok(value)) => self.vault_copy(value, cx),
                    (_, Err(why)) => self.notify_user(why, true, cx),
                }
            }
            VaultUpdate::Audit { ask, before, result } => {
                if ask.item_id.is_some() {
                    if let Some((_, rows)) = self.vault.history.as_mut().filter(|(item, _)| Some(*item) == ask.item_id) {
                        *rows = result.map(|p| p.rows);
                    }
                } else if self.vault.audit_asked == Some(ask) {
                    match result {
                        Ok(page) => {
                            self.vault.audit_more = page.more;
                            self.vault.audit_error = None;
                            merge_audit(&mut self.vault.audit, page.rows, before);
                        }
                        Err(why) => self.vault.audit_error = Some(why),
                    }
                }
            }
            VaultUpdate::Key(result) => {
                self.vault.key_busy = false;
                self.vault.clear_fields = true;
                match result {
                    Ok(file) if self.vault_on_screen() => {
                        self.vault.key_error = None;
                        self.vault.out.key = Some(KeyShown::new(file, Instant::now()));
                        self.vault_tick(cx);
                    }
                    Ok(_) => {}
                    Err(why) => self.vault.key_error = Some(why),
                }
            }
            VaultUpdate::Saved(result) => match result {
                Ok(item) => {
                    self.vault.editor = None;
                    self.vault.history = None;
                    self.vault.clear_fields = true;
                    self.vault.asked = None;
                    self.notify_user(format!("Saved {}.", item.name), false, cx);
                }
                Err(why) => {
                    if let Some(e) = self.vault.editor.as_mut() {
                        e.busy = false;
                        e.failure = Some(why);
                    }
                }
            },
            VaultUpdate::Archived(id, result) => match result {
                Ok(item) => {
                    if self.vault.editor.as_ref().is_some_and(|e| e.item == Some(id)) {
                        self.vault.editor = None;
                        self.vault.history = None;
                        self.vault.clear_fields = true;
                    }
                    self.vault.asked = None;
                    self.notify_user(format!("Deleted {}. Runs no longer get {}.", item.name, item.env_name), false, cx);
                }
                Err(why) => self.notify_user(why, true, cx),
            },
            VaultUpdate::Changed => {
                // Read again what shows; the rest when it shows again (MU4-01 AC2).
                self.vault.asked = None;
                self.vault.audit_asked = None;
                if let Some((item, _)) = self.vault.history {
                    self.vault_send(VaultCall::Audit { ask: AuditAsk { project_id: None, all: false, item_id: Some(item) }, before: None });
                }
            }
        }
        cx.notify();
    }

    /// Copy without showing: the clipboard gets it, this state keeps only a keyed hash.
    fn vault_copy(&mut self, value: Revealed, cx: &mut Context<Self>) {
        self.vault.copy_epoch += 1;
        let epoch = self.vault.copy_epoch;
        self.vault.copied = Some(Copied::of(value.expose(), epoch));
        cx.write_to_clipboard(ClipboardItem::new_string(value.expose().to_string()));
        drop(value);
        self.vault_clear_clipboard_later(epoch, cx);
        self.notify_user(COPIED, false, cx);
    }

    /// Clear the clipboard after 60 s, only if it still holds our value (E19).
    fn vault_clear_clipboard_later(&mut self, epoch: u64, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(CLEAR_CLIPBOARD_AFTER).await;
            let _ = this.update(cx, |this, cx| {
                let Some(copied) = this.vault.copied.take_if(|c| c.epoch == epoch) else { return };
                let mut clipboard = cx.read_from_clipboard().and_then(|item| item.text());
                if copied.is_ours(clipboard.as_deref()) {
                    cx.write_to_clipboard(ClipboardItem::new_string(String::new()));
                }
                if let Some(text) = clipboard.as_mut() {
                    backend::wipe(text);
                }
            });
        })
        .detach();
    }

    /// Repaint twice a second while a value or the key shows; hide it when due or off screen.
    fn vault_tick(&mut self, cx: &mut Context<Self>) {
        if self.vault.ticking {
            return;
        }
        self.vault.ticking = true;
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(Duration::from_millis(500)).await;
            let alive = this.update(cx, |this, cx| {
                this.vault_hide_if_left();
                let value = this.vault.out.shown.is_some();
                if this.vault.out.hide_due(Instant::now()) && value && this.vault.out.shown.is_none() {
                    this.vault_announce(HIDDEN_ANNOUNCE);
                }
                let running = this.vault.out.showing();
                this.vault.ticking = running;
                cx.notify();
                running
            });
            if !matches!(alive, Ok(true)) {
                break;
            }
        })
        .detach();
    }

    /// Leaving the page hides what shows (ui-ontwerp §3).
    pub fn vault_hide_if_left(&mut self) {
        if !self.vault_on_screen() && self.vault.out.showing() {
            self.vault.out.hide();
        }
    }

    fn vault_reveal(&mut self, item_id: VaultItemId, purpose: RevealPurpose, cx: &mut Context<Self>) {
        if purpose == RevealPurpose::Show && self.vault.out.shown.as_ref().is_some_and(|s| s.item_id == item_id) {
            // Hide: the focus stays on the same button (§6).
            self.vault.out.shown = None;
            self.vault_announce(HIDDEN_ANNOUNCE);
            cx.notify();
            return;
        }
        self.vault.revealing = Some((item_id, purpose));
        self.vault_send(VaultCall::Reveal { item_id, purpose });
        cx.notify();
    }

    fn vault_announce(&mut self, text: &'static str) {
        let n = self.vault.announce.map_or(0, |(n, _)| n + 1);
        self.vault.announce = Some((n, text));
    }

    fn vault_open_scope(&mut self, scope: Option<ProjectId>, cx: &mut Context<Self>) {
        if self.vault.scope != scope {
            self.vault.scope = scope;
            self.vault.out.hide();
            self.vault.editor = None;
            self.vault.history = None;
            self.vault.clear_fields = true;
        }
        cx.notify();
    }

    fn vault_set_tab(&mut self, tab: VaultTab, cx: &mut Context<Self>) {
        self.vault.tab = tab;
        cx.notify();
    }

    /// New item in `scope`.
    fn vault_new(&mut self, scope: Option<ProjectId>, window: &mut Window, cx: &mut Context<Self>) {
        self.vault.editor = Some(Editor { item: None, scope, kind: VaultKind::Text, busy: false, failure: None });
        self.vault.history = None;
        self.vault_fill(None, window, cx);
        self.vault.name.update(cx, |i, cx| i.focus(window, cx));
        cx.notify();
    }

    /// Edit an item: never with a secret's value filled in (§4), with its History.
    fn vault_edit(&mut self, id: VaultItemId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = self.vault.item(id).cloned() else { return };
        self.vault.editor = Some(Editor { item: Some(id), scope: item.project_id, kind: item.kind, busy: false, failure: None });
        self.vault.history = Some((id, Err("Loading…".into())));
        self.vault_send(VaultCall::Audit { ask: AuditAsk { project_id: None, all: false, item_id: Some(id) }, before: None });
        self.vault_fill(Some(&item), window, cx);
        cx.notify();
    }

    fn vault_fill(&mut self, item: Option<&VaultItemView>, window: &mut Window, cx: &mut Context<Self>) {
        let value = match item {
            Some(i) if i.kind != VaultKind::Secret => i.value.clone().unwrap_or_default(),
            _ => String::new(),
        };
        let secret = item.is_some_and(|i| i.kind == VaultKind::Secret);
        self.vault.value_visible = false;
        let fields = [
            (self.vault.name.clone(), item.map(|i| i.name.clone()).unwrap_or_default()),
            (self.vault.env.clone(), item.map(|i| i.env_name.clone()).unwrap_or_default()),
            (self.vault.value.clone(), value),
            (self.vault.note.clone(), item.and_then(|i| i.description.clone()).unwrap_or_default()),
        ];
        for (input, text) in fields {
            input.update(cx, |i, cx| i.set_value(text, window, cx));
        }
        self.vault.value.update(cx, |i, cx| i.set_masked(secret, window, cx));
    }

    fn vault_clear_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for input in [self.vault.name.clone(), self.vault.env.clone(), self.vault.value.clone(), self.vault.note.clone(), self.vault.password.clone()] {
            input.update(cx, |i, cx| i.set_value("", window, cx));
        }
    }

    fn vault_set_kind(&mut self, kind: VaultKind, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.vault.editor.as_mut() else { return };
        editor.kind = kind;
        let masked = kind == VaultKind::Secret && !self.vault.value_visible;
        self.vault.value.update(cx, |i, cx| i.set_masked(masked, window, cx));
        cx.notify();
    }

    fn vault_toggle_eye(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.vault.value_visible = !self.vault.value_visible;
        let masked = !self.vault.value_visible;
        self.vault.value.update(cx, |i, cx| i.set_masked(masked, window, cx));
        cx.notify();
    }

    fn vault_close_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.vault.editor = None;
        self.vault.history = None;
        self.vault_clear_fields(window, cx);
        cx.notify();
    }

    pub fn vault_save(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.vault.editor.clone() else { return };
        if editor.busy {
            return;
        }
        let (name, env, mut value, note) = (
            self.vault.name.read(cx).value().to_string(),
            self.vault.env.read(cx).value().to_string(),
            self.vault.value.read(cx).value().to_string(),
            self.vault.note.read(cx).value().to_string(),
        );
        let typed = Typed { name: &name, env: &env, value: &value, note: &note };
        let item = editor.item.and_then(|id| self.vault.item(id)).cloned();
        let problem = form_problem(&editor, item.as_ref(), &typed);
        let call = match (&problem, &item) {
            (Some(_), _) => None,
            (None, None) => Some(VaultCall::Create(create_params(&editor, &typed))),
            (None, Some(item)) => update_params(&editor, item, &typed).map(VaultCall::Update),
        };
        let unchanged = problem.is_none() && call.is_none();
        backend::wipe(&mut value);
        if unchanged {
            self.vault.editor = None;
            self.vault.history = None;
            self.vault.clear_fields = true;
            cx.notify();
            return;
        }
        if let Some(e) = self.vault.editor.as_mut() {
            e.failure = problem;
            e.busy = call.is_some();
        }
        if let Some(call) = call {
            self.vault_send(call);
        }
        cx.notify();
    }

    pub fn vault_archive(&mut self, item: VaultItemId, cx: &mut Context<Self>) {
        self.vault_send(VaultCall::Archive(item));
        cx.notify();
    }

    fn vault_ask_archive(&mut self, item: VaultItemId, window: &mut Window, cx: &mut Context<Self>) {
        self.overlay = Overlay::Confirm(ConfirmKind::ArchiveVaultItem(item));
        self.overlay_focus.focus(window, cx);
        cx.notify();
    }

    pub fn vault_export_key(&mut self, cx: &mut Context<Self>) {
        if self.vault.key_busy {
            return;
        }
        let password = self.vault.password.read(cx).value().to_string();
        if password.is_empty() {
            self.vault.key_error = Some("Type your password: exporting the key asks it again.".into());
            cx.notify();
            return;
        }
        self.vault.key_busy = true;
        self.vault.key_error = None;
        self.vault_send(VaultCall::KeyExport { password: Password::new(password) });
        cx.notify();
    }

    /// Save the shown key to a file of mode 0600 (E9); the key stays only as long as it shows.
    fn vault_save_key(&mut self, cx: &mut Context<Self>) {
        let Some(key) = self.vault.out.key.as_mut() else { return };
        if key.paused.is_some() {
            return;
        }
        let name = key_file_name(&key.file.key_id, key.file.key.expose());
        // The countdown waits while the dialog is open (QA2 m3).
        key.pause(Instant::now());
        let path = cx.prompt_for_new_path(&dirs_home(), Some(&name));
        cx.spawn(async move |this, cx| {
            let path = path.await;
            let _ = this.update(cx, |this, cx| {
                if let Some(key) = this.vault.out.key.as_mut() {
                    key.resume(Instant::now());
                }
                let Ok(Ok(Some(path))) = path else {
                    cx.notify();
                    return;
                };
                let Some(key) = this.vault.out.key.as_ref() else {
                    this.notify_user("The key was hidden before it was saved. Export it again.", true, cx);
                    return;
                };
                match save_key(&path, key.file.key.expose()) {
                    Ok(()) => {
                        this.vault.out.key = None;
                        this.vault.key_open = false;
                        this.notify_user(format!("Key saved to {} (only you can read it).", path.display()), false, cx);
                    }
                    Err(e) => this.notify_user(format!("Could not save the key: {e}"), true, cx),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn vault_copy_key(&mut self, cx: &mut Context<Self>) {
        let Some(key) = self.vault.out.key.as_ref() else { return };
        let value = Revealed::new(key.file.key.expose().to_string());
        self.vault_copy(value, cx);
    }

    /// Escape closes the editor or the key export first.
    pub fn vault_back(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.overlay != Overlay::None || !self.vault_on_screen() {
            return false;
        }
        if self.vault.editor.is_some() {
            self.vault_close_editor(window, cx);
            return true;
        }
        if self.vault.key_open {
            self.vault.key_open = false;
            self.vault.out.key = None;
            self.vault.clear_fields = true;
            cx.notify();
            return true;
        }
        false
    }

    // ── B10 ───────────────────────────────────────────────────────────

    /// A held message asks "Send anyway?"; while one asks, the next waits (none is dropped).
    pub fn apply_secret_hold(&mut self, hold: SecretHold, cx: &mut Context<Self>) {
        if self.vault.held.is_some() {
            self.vault.held_next.push_back(hold);
            return;
        }
        self.vault.held = Some(hold);
        self.overlay = Overlay::Confirm(ConfirmKind::LooksLikeSecret);
        cx.notify();
    }

    /// After Send anyway or Cancel: the next held message asks, if any.
    pub fn vault_hold_next(&mut self, cx: &mut Context<Self>) {
        if self.vault.held.is_none() {
            if let Some(next) = self.vault.held_next.pop_front() {
                self.apply_secret_hold(next, cx);
            }
        }
    }

    /// "Send anyway": the same message with `confirm_secret` (stored redacted).
    pub fn vault_send_anyway(&mut self, cx: &mut Context<Self>) {
        if let Some(hold) = self.vault.held.take() {
            match &hold.command {
                aurelius_domain::Command::PostTicketMessage(_) => {
                    self.board.posting = true;
                    self.board.post_error = None;
                }
                aurelius_domain::Command::CreateTicket(_) => {
                    self.board.form.busy = true;
                    self.board.form.error = None;
                }
                command => {
                    if let Some(ticket) = crate::backend::ticket_of(command) {
                        self.board.busy.insert(ticket);
                    }
                }
            }
            let _ = self.requests_handle().send(hold.resend());
        }
        cx.notify();
    }

    /// Where the composer writes now, as a held message's place.
    fn composer_in(&self) -> Option<HeldIn> {
        match self.thread_screen() {
            Page::GroupChat => Some(HeldIn::Channel(self.channel)),
            Page::Employee(_) => self.current_manager_thread().map(HeldIn::Chat),
            _ => None,
        }
    }

    /// Cancel: a chat message goes back to the composer of its own conversation or channel,
    /// with its images and files, after what is typed there (D-m4); a ticket's draft stayed.
    pub fn vault_hold_cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(hold) = self.vault.held.take() else { return };
        let place = held_in(&hold.command);
        match place {
            HeldIn::Ticket | HeldIn::Form => return,
            HeldIn::Feedback(ticket) => {
                let (input, _) = self.feedback_input(ticket);
                let text = text_back(&input.read(cx).value(), hold.text());
                input.update(cx, |i, cx| i.set_value(text, window, cx));
                return;
            }
            HeldIn::Plan => {
                self.plan_sending = None;
                let text = text_back(&self.plan_input.read(cx).value(), hold.text());
                self.plan_input.update(cx, |i, cx| i.set_value(text, window, cx));
                return;
            }
            HeldIn::Chat(_) | HeldIn::Channel(_) => {}
        }
        if self.composer_in() != Some(place) {
            match place {
                HeldIn::Chat(conversation) => self.open_conversation_anywhere(conversation, window, cx),
                HeldIn::Channel(channel) => {
                    let general = || self.channels.as_ref().and_then(|l| l.iter().find(|c| c.project_id.is_none())).map(|c| c.conversation_id);
                    match channel.or_else(general) {
                        Some(conversation) => self.go_to_channel(conversation, window, cx),
                        None => self.go_nav(crate::app::NavItem::Chat, window, cx),
                    }
                }
                HeldIn::Ticket | HeldIn::Feedback(_) | HeldIn::Plan | HeldIn::Form => {}
            }
        }
        let now = self.composer.read(cx).value().to_string();
        let place = self.draft_place();
        let text = held_back_in(&mut self.drafts, place, &now, hold.text());
        self.composer.update(cx, |i, cx| i.set_value(text, window, cx));
        let (images, files) = attachments_back(&hold.command);
        for image in images {
            if !self.pending_images.contains(&image) {
                self.pending_images.push(image);
            }
        }
        for file in files {
            let Ok(meta) = std::fs::metadata(&file) else { continue };
            if !self.pending_files.iter().any(|(p, _)| *p == file) {
                self.pending_files.push((file, meta.len()));
            }
        }
        drop(hold);
    }
}

impl AppView {
    /// The sidebar's Vault: the scopes on the left, the scope's vault beside them.
    pub fn render_vault_page(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let scope = self.vault.scope;
        let body = self.render_vault_body(scope, false, window, cx);
        let list = self.vault.list.as_ref().and_then(|(_, r)| r.as_ref().ok()).cloned();
        let rows = scopes(list.as_ref(), &self.projects);
        let scope_list = v().w(px(190.)).flex_none().gap(px(2.)).pt(px(26.)).pr(px(16.))
            .child(ui::eyebrow("SCOPE").pb(px(4.)))
            .children(rows.into_iter().enumerate().map(|(i, (id, name, count))| {
                let on = id == scope;
                h().id(("vault-scope", i)).role(Role::Tab).aria_label(format!("{name}, {count} items"))
                    .gap(px(8.)).px(px(8.)).py(px(6.)).rounded(px(6.)).cursor_pointer().text_size(px(13.))
                    .text_color(c(if on { theme::TEXT } else { theme::TEXT_2 }))
                    .when(on, |el| el.bg(c(theme::SURFACE_2)).font_weight(FontWeight::MEDIUM))
                    .hover(|s| s.bg(c(theme::SURFACE_2)))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.vault_open_scope(id, cx)))
                    .child(div().flex_1().min_w_0().truncate().child(if id.is_none() { format!("⌂ {name}") } else { name }))
                    .child(div().text_size(px(11.5)).text_color(c(theme::MUTED)).child(count.to_string()))
            }));
        self.page(h().items_start().child(scope_list).child(div().flex_1().min_w_0().child(body))).into_any_element()
    }

    /// The vault of one scope: its tabs, the items or the audit log, the editor and the key.
    /// `in_project`: the project's own tab (no scope list, no key export).
    pub fn render_vault_body(&mut self, scope: Option<ProjectId>, in_project: bool, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        if std::mem::take(&mut self.vault.clear_fields) {
            self.vault_clear_fields(window, cx);
        }
        if in_project && self.vault.scope != scope {
            self.vault_open_scope(scope, cx);
        }
        if !self.vault_ready() {
            let why = match &self.connection {
                backend::Connection::Online { .. } => backend::VAULT_NOT_BUILT,
                _ => "The vault shows once this window is connected to the server.",
            };
            return v().pt(px(26.)).child(div().text_size(px(13.)).text_color(c(theme::MUTED)).child(why)).into_any_element();
        }
        self.vault_want(scope);
        let scope_name = match scope {
            None => "Company".to_string(),
            Some(id) => self.projects.iter().find(|p| p.id == id).map_or_else(|| "This project".to_string(), |p| p.name.clone()),
        };
        let count = match self.vault.list.as_ref() {
            Some((s, Ok(l))) if *s == scope => Some(l.items.len()),
            _ => None,
        };
        let tab = self.vault.tab;
        let tab_button = |id: &'static str, label: &'static str, t: VaultTab, cx: &mut Context<Self>| {
            let on = tab == t;
            div().id(id).role(Role::Tab).aria_label(label).px(px(11.)).h(px(28.)).flex().items_center().rounded(px(7.)).cursor_pointer().text_size(px(13.))
                .text_color(c(if on { theme::TEXT } else { theme::MUTED }))
                .when(on, |el| el.bg(c(theme::SURFACE_2)).font_weight(FontWeight::MEDIUM))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.vault_set_tab(t, cx)))
                .child(label)
        };
        let header = h().gap(px(10.)).items_center().pt(px(22.))
            .child(div().font_family(ui::serif(cx)).text_size(px(22.)).child(scope_name.clone()))
            .when_some(count, |el, n| el.child(div().text_size(px(13.)).text_color(c(theme::MUTED)).child(format!("· {n} item{}", if n == 1 { "" } else { "s" }))))
            .child(div().flex_1())
            .when(!in_project, |el| {
                el.child(ui::button("vault-key", "Export key…", ButtonKind::Outline, None).on_click(cx.listener(|this, _: &ClickEvent, w, cx| {
                    this.vault.key_open = !this.vault.key_open;
                    this.vault.key_error = None;
                    if this.vault.key_open {
                        this.vault.password.update(cx, |i, cx| i.focus(w, cx));
                    } else {
                        this.vault.out.key = None;
                    }
                    cx.notify();
                })))
            })
            .child(ui::button("vault-new", "+ New item", ButtonKind::Primary, None).on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.vault_new(scope, w, cx))));
        let tabs = h().gap(px(2.)).pt(px(10.)).child(tab_button("vault-tab-items", "Items", VaultTab::Items, cx)).child(tab_button("vault-tab-audit", "Audit log", VaultTab::Audit, cx));
        let content = match tab {
            VaultTab::Items => self.render_vault_items(scope, &scope_name, cx),
            VaultTab::Audit => self.render_vault_audit(scope, cx),
        };
        let key = (self.vault.key_open && !in_project).then(|| self.render_vault_key(cx));
        let editor = self.vault.editor.is_some().then(|| self.render_vault_editor(cx));
        let announce = self.vault.announce.map(|(n, text)| div().id(("vault-announce", n as usize)).role(Role::Status).aria_label(text).absolute().top_0().left_0().size(px(1.)).overflow_hidden());
        v().relative().gap(px(12.)).pb(px(40.))
            .children(announce)
            .child(header)
            .children(key)
            .children(editor)
            .child(tabs)
            .child(content)
            .child(vault_notices())
            .into_any_element()
    }

    fn render_vault_items(&mut self, scope: Option<ProjectId>, scope_name: &str, cx: &mut Context<Self>) -> AnyElement {
        let list = match self.vault.list.as_ref() {
            Some((s, Ok(list))) if *s == scope => list.clone(),
            Some((s, Err(why))) if *s == scope => return div().text_size(px(13.)).text_color(c(theme::NEGATIVE)).child(why.clone()).into_any_element(),
            _ => return div().text_size(px(13.)).text_color(c(theme::MUTED)).child("Loading…").into_any_element(),
        };
        let mut out = v().gap(px(2.)).child(
            h().gap(px(10.)).px(px(8.)).py(px(4.)).text_size(px(11.)).text_color(c(theme::MUTED))
                .child(div().w(px(NAME_W)).min_w_0().child("Name"))
                .child(div().w(px(KEY_W)).min_w_0().child("Key"))
                .child(div().w(px(80.)).flex_none().child("Type"))
                .child(div().flex_1().min_w(px(VALUE_MIN_W)).child("Value"))
                .child(div().w(px(UPDATED_W)).min_w_0().child("Updated")),
        );
        if list.items.is_empty() {
            let who = if scope.is_some() { "Employees of this project" } else { "Every employee" };
            out = out.child(div().px(px(8.)).py(px(14.)).text_size(px(13.)).text_color(c(theme::TEXT_2)).child(format!("No items in {scope_name} yet. {who} can read every item here.")));
        }
        for (i, item) in list.items.iter().enumerate() {
            out = out.child(self.render_vault_row(item, i, cx));
        }
        if scope.is_some() && !list.company_items.is_empty() {
            let open = self.vault.company_open;
            out = out.child(
                h().id("vault-company-items").gap(px(6.)).px(px(8.)).pt(px(12.)).cursor_pointer().text_size(px(12.5)).text_color(c(theme::TEXT_2))
                    .hover(|s| s.text_color(c(theme::TEXT)))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.vault.company_open = !this.vault.company_open;
                        cx.notify();
                    }))
                    .child(format!("Company items also available here: {}", list.company_items.len()))
                    .child(ui::chevron(open, 10., theme::MUTED)),
            );
            if open {
                for (i, item) in list.company_items.iter().enumerate() {
                    out = out.child(self.render_vault_row(item, 1000 + i, cx));
                }
            }
        }
        out.into_any_element()
    }

    fn render_vault_row(&self, item: &VaultItemView, i: usize, cx: &mut Context<Self>) -> Stateful<Div> {
        let id = item.id;
        let me = self.active_member().map(|u| u.id);
        let by = if Some(item.updated_by.id) == me { "You".to_string() } else { item.updated_by.username.clone() };
        let shown = self.vault.out.shown_value(id);
        let until = self.vault.out.shown.as_ref().filter(|s| s.item_id == id).map(|s| s.until);
        let waiting = self.vault.revealing.filter(|(item, _)| *item == id).map(|(_, p)| p);
        let small = |key: &'static str, label: &'static str| ui::button((key, i), label, ButtonKind::Outline, None).h(px(22.)).px(px(7.)).text_size(px(11.5));
        let value: AnyElement = match cell(item, shown) {
            Cell::Masked | Cell::Shown(_) => {
                let visible = shown.is_some();
                let text = shown.map_or_else(|| MASK.to_string(), str::to_owned);
                let label = format!("{}, secret, {}", item.name, if visible { "shown for 30 seconds" } else { "hidden" });
                let show = match (waiting, visible) {
                    (Some(RevealPurpose::Show), _) => "…",
                    (_, true) => "Hide",
                    _ => "Show",
                };
                // The value on its own line, wrapped whole (QA2 m2); the buttons under it wrap too.
                v().gap(px(4.)).min_w_0()
                    .child(div().id(("vault-value", i)).aria_label(label).w_full().min_w_0().font_family(ui::mono(cx)).text_size(px(12.)).line_height(px(17.)).child(text))
                    .child(h().gap(px(6.)).flex_wrap().items_center().child(
                        small("vault-show", show)
                            .aria_label(format!("{} {}", if visible { "Hide" } else { "Show" }, item.name))
                            .tooltip(ui::tip(SHOW_TIP, None))
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.vault_reveal(id, RevealPurpose::Show, cx))),
                    )
                    .child(
                        small("vault-copy", if waiting == Some(RevealPurpose::Copy) { "…" } else { "Copy" })
                            .aria_label(format!("Copy {}", item.name))
                            .tooltip(ui::tip(COPY_TIP, None))
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.vault_reveal(id, RevealPurpose::Copy, cx))),
                    )
                    .when_some(until.and_then(|u| hides_in(u, Instant::now())), |el, left| el.child(div().text_size(px(11.)).text_color(c(theme::MUTED)).child(left))))
                    .into_any_element()
            }
            Cell::Link(url) => {
                let open = url.clone();
                div().id(("vault-link", i)).role(Role::Link).min_w_0().truncate().text_size(px(12.5)).text_color(c(theme::ACCENT_TEXT)).cursor_pointer().underline()
                    .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.open_url(&open)))
                    .child(url)
                    .into_any_element()
            }
            Cell::Text(text) => div().min_w_0().truncate().text_size(px(12.5)).text_color(c(theme::TEXT_2)).child(text).into_any_element(),
            Cell::Undecryptable => div().text_size(px(12.5)).text_color(c(theme::NEGATIVE)).child(UNDECRYPTABLE).into_any_element(),
        };
        h().id(("vault-row", i)).gap(px(10.)).px(px(8.)).py(px(6.)).rounded(px(6.)).items_center().text_size(px(13.))
            .hover(|s| s.bg(c(theme::SURFACE)))
            .child(
                // The pill under the name, so the name keeps the column's width (QA2 m1).
                v().id(("vault-name", i)).role(Role::Button).aria_label(format!("Edit {}", item.name)).w(px(NAME_W)).min_w_0().gap(px(3.)).items_start().cursor_pointer()
                    .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.vault_edit(id, w, cx)))
                    .child(div().w_full().min_w_0().truncate().child(item.name.clone()))
                    .when(item.overrides_company, |el| {
                        el.child(div().flex_none().px(px(6.)).rounded(px(8.)).border_1().border_color(c(theme::ATTENTION_BORDER)).text_size(px(10.5)).text_color(c(theme::ACCENT_TEXT)).child("overrides Company"))
                    }),
            )
            .child(div().w(px(KEY_W)).min_w_0().truncate().font_family(ui::mono(cx)).text_size(px(12.)).text_color(c(theme::TEXT_2)).child(item.env_name.clone()))
            .child(div().w(px(80.)).flex_none().text_size(px(12.)).text_color(c(theme::TEXT_2)).child(kind_tag(item.kind)))
            // The value keeps room for Show and Copy; the other columns give way first (QA2 m1).
            .child(div().flex_1().min_w(px(VALUE_MIN_W)).child(value))
            .child(div().w(px(UPDATED_W)).min_w_0().truncate().text_size(px(12.)).text_color(c(theme::MUTED)).child(format!("{by} · {}", ui::fmt_when(item.updated_at_ms))))
    }

    fn render_vault_editor(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let editor = self.vault.editor.clone().unwrap_or(Editor { item: None, scope: None, kind: VaultKind::Text, busy: false, failure: None });
        let item = editor.item.and_then(|id| self.vault.item(id)).cloned();
        let scope_name = match editor.scope {
            None => "Company".to_string(),
            Some(id) => self.projects.iter().find(|p| p.id == id).map_or_else(|| "this project".to_string(), |p| p.name.clone()),
        };
        let name = self.vault.name.read(cx).value().to_string();
        let env = self.vault.env.read(cx).value().to_string();
        let derived = env_from_name(&name);
        let env_now = if env.trim().is_empty() { derived.clone() } else { env.trim().to_string() };
        let renamed = item.as_ref().filter(|i| env_now != i.env_name).map(|i| i.env_name.clone());
        let title = match &item {
            Some(i) => format!("{} · {scope_name}", i.name),
            None => format!("New item · {scope_name}"),
        };
        let field = |label: &'static str, input: AnyElement, hint: Option<String>| {
            h().gap(px(10.)).items_start()
                .child(div().w(px(70.)).pt(px(8.)).text_size(px(12.)).text_color(c(theme::TEXT_2)).child(label))
                .child(v().flex_1().gap(px(4.)).child(input).when_some(hint, |el, hint| el.child(div().text_size(px(11.5)).text_color(c(theme::MUTED)).child(hint))))
        };
        let kind_row = h().gap(px(4.)).children(kinds_for(item.as_ref().map(|i| i.kind)).into_iter().enumerate().map(|(i, k)| {
            let on = k == editor.kind;
            div().id(("vault-kind", i)).role(Role::RadioButton).aria_label(kind_label(k)).aria_toggled(if on { Toggled::True } else { Toggled::False }).px(px(10.)).h(px(26.)).flex().items_center().rounded(px(6.)).border_1().cursor_pointer().text_size(px(12.5))
                .border_color(c(if on { theme::ACCENT } else { theme::BORDER }))
                .text_color(c(if on { theme::TEXT } else { theme::TEXT_2 }))
                .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.vault_set_kind(k, w, cx)))
                .child(kind_label(k))
        }));
        let kind_hint = item.as_ref().map(|i| match i.kind {
            VaultKind::Secret => "A secret stays a secret. Make a new item to change its type.".to_string(),
            _ => "Text and URL trade places; a secret needs a new item.".to_string(),
        });
        let visible = self.vault.value_visible;
        let eye = div().id("vault-eye").role(Role::Button).aria_label(if visible { "Hide what you type" } else { "Show what you type" })
            .px(px(6.)).pt(px(8.)).cursor_pointer().text_size(px(12.)).text_color(c(theme::TEXT_2))
            .on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.vault_toggle_eye(w, cx)))
            .child(if visible { "Hide" } else { "Show" });
        let value_input = h().gap(px(4.)).child(div().flex_1().child(ui::input(&self.vault.value))).when(editor.kind == VaultKind::Secret, |el| el.child(eye)).into_any_element();
        let value_hint = match (&item, editor.kind) {
            (Some(i), _) if i.kind == VaultKind::Secret => Some(KEEP_SECRET.to_string()),
            (_, VaultKind::Secret) => Some(format!("At least {} characters. It is never shown again here.", aurelius_domain::vault::MIN_SECRET_CHARS)),
            (_, kind) => Some(value_hint(kind)),
        };
        let env_hint = match (renamed, env.trim().is_empty() && !derived.is_empty()) {
            (Some(old), _) => format!("Runs that use {old} will no longer find it."),
            (None, true) => format!("{derived} · A–Z, 0–9 and _ · unique in this scope"),
            (None, false) => "A–Z, 0–9 and _ · unique in this scope".to_string(),
        };
        let item_id = item.as_ref().map(|i| i.id);
        v().id("vault-editor").gap(px(10.)).p(px(16.)).rounded(px(9.)).border_1().border_color(c(theme::BORDER)).bg(c(theme::SURFACE))
            .child(div().font_family(ui::serif(cx)).text_size(px(17.)).child(title))
            .child(field("Type", kind_row.into_any_element(), kind_hint))
            .child(field("Name *", ui::input(&self.vault.name).into_any_element(), None))
            .child(field("Key *", ui::input(&self.vault.env).into_any_element(), Some(env_hint)))
            .child(field("Value *", value_input, value_hint))
            .child(field("Note", ui::input(&self.vault.note).into_any_element(), None))
            .child(div().text_size(px(11.5)).line_height(px(16.)).text_color(c(theme::MUTED)).child(EVERYONE_READS))
            .when_some(editor.failure.clone(), |el, why| el.child(div().id("vault-editor-problem").role(Role::Alert).text_size(px(12.)).text_color(c(theme::NEGATIVE)).child(why)))
            .child(
                h().gap(px(8.)).items_center()
                    .when_some(item_id, |el, id| {
                        el.child(ui::button("vault-delete", "Delete…", ButtonKind::Outline, None).on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.vault_ask_archive(id, w, cx))))
                    })
                    .child(div().flex_1())
                    .child(ui::button("vault-cancel", "Cancel", ButtonKind::Outline, Some(ui::kbd("escape", cx))).on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.vault_close_editor(w, cx))))
                    .child(ui::button("vault-save", if editor.busy { "Saving…" } else { "Save" }, ButtonKind::Primary, Some(ui::kbd("secondary-enter", cx))).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.vault_save(cx)))),
            )
            .when(item.is_some(), |el| el.child(self.render_vault_history(cx)))
    }

    /// The item's History (MU4-09 AC2).
    fn render_vault_history(&self, cx: &mut Context<Self>) -> Div {
        let me = self.active_member().map(|u| u.id);
        let body: AnyElement = match self.vault.history.as_ref() {
            Some((_, Ok(rows))) if rows.is_empty() => div().text_size(px(12.)).text_color(c(theme::MUTED)).child("Nothing yet.").into_any_element(),
            Some((_, Ok(rows))) => v().gap(px(2.)).children(rows.iter().map(|r| self.audit_line(r, me, false, cx))).into_any_element(),
            Some((_, Err(why))) => div().text_size(px(12.)).text_color(c(theme::MUTED)).child(why.clone()).into_any_element(),
            None => div().into_any_element(),
        };
        v().gap(px(6.)).pt(px(8.)).border_t_1().border_color(c(theme::BORDER)).child(ui::eyebrow("HISTORY")).child(body)
    }

    fn audit_line(&self, row: &VaultAuditRow, me: Option<aurelius_domain::UserId>, item: bool, cx: &mut Context<Self>) -> Div {
        let name = who(&row.actor, me);
        let mark = match &row.actor {
            VaultActor::Employee { .. } => div().size(px(14.)).flex_none().rounded(px(3.)).border_1().border_color(c(theme::MANAGER_MARK)),
            VaultActor::BoardMember { user } => ui::member_monogram(&user.username, 16., cx),
        };
        h().gap(px(10.)).px(px(8.)).py(px(4.)).items_center().text_size(px(12.5))
            .child(div().w(px(110.)).text_color(c(theme::MUTED)).child(ui::fmt_when(row.at_ms)))
            .child(h().w(px(170.)).gap(px(6.)).items_center().min_w_0().child(mark).child(div().truncate().child(name)))
            .child(div().w(px(130.)).child(action_text(row)))
            .when(item, |el| el.child(div().w(px(150.)).truncate().text_color(c(theme::TEXT_2)).child(row.item_name.clone().unwrap_or_else(|| "—".into()))))
            .child(div().flex_1().min_w_0().truncate().text_color(c(theme::MUTED)).child(via(row)))
    }

    fn render_vault_audit(&mut self, scope: Option<ProjectId>, cx: &mut Context<Self>) -> AnyElement {
        let me = self.active_member().map(|u| u.id);
        let all = self.vault.audit_all;
        let toggle = h().id("vault-audit-all").role(Role::CheckBox).aria_label("All scopes").aria_toggled(if all { Toggled::True } else { Toggled::False }).gap(px(6.)).items_center().cursor_pointer().text_size(px(12.5)).text_color(c(theme::TEXT_2))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.vault.audit_all = !this.vault.audit_all;
                cx.notify();
            }))
            .child(div().size(px(13.)).rounded(px(3.)).border_1().border_color(c(theme::BORDER_STRONG)).flex().items_center().justify_center().when(all, |el| el.child(ui::check(9., theme::ACCENT_TEXT))))
            .child("All scopes");
        let mut out = v().gap(px(2.)).child(h().pb(px(6.)).child(toggle)).child(
            h().gap(px(10.)).px(px(8.)).py(px(4.)).text_size(px(11.)).text_color(c(theme::MUTED))
                .child(div().w(px(110.)).child("Time"))
                .child(div().w(px(170.)).child("Who"))
                .child(div().w(px(130.)).child("Action"))
                .child(div().w(px(150.)).child("Item"))
                .child(div().flex_1().child("Via")),
        );
        if let Some(why) = self.vault.audit_error.clone() {
            out = out.child(div().px(px(8.)).text_size(px(13.)).text_color(c(theme::NEGATIVE)).child(why));
        } else if self.vault.audit.is_empty() {
            out = out.child(div().px(px(8.)).py(px(10.)).text_size(px(13.)).text_color(c(theme::MUTED)).child("Nothing in the audit log yet."));
        }
        for row in &self.vault.audit {
            out = out.child(self.audit_line(row, me, true, cx));
        }
        if self.vault.audit_more {
            let before = self.vault.audit.last().map(|r| r.id);
            let ask = AuditAsk { project_id: if all { None } else { scope }, all, item_id: None };
            out = out.child(
                h().justify_center().pt(px(8.)).child(ui::button("vault-audit-older", "Load older", ButtonKind::Outline, None).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.vault_send(VaultCall::Audit { ask, before });
                    cx.notify();
                }))),
            );
        }
        out.child(div().pt(px(6.)).text_size(px(11.5)).text_color(c(theme::MUTED)).child("The audit log is read-only and never shows a value."))
            .into_any_element()
    }

    /// The vault key (B8, E9): the own password again, then the key once.
    fn render_vault_key(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let card = v().id("vault-key-card").gap(px(10.)).p(px(16.)).rounded(px(9.)).border_1().border_color(c(theme::BORDER)).bg(c(theme::SURFACE))
            .child(div().font_family(ui::serif(cx)).text_size(px(17.)).child("Export the vault key"))
            .child(div().text_size(px(12.5)).line_height(px(18.)).text_color(c(theme::TEXT_2)).child(KEY_ABOUT))
            .child(div().text_size(px(11.5)).text_color(c(theme::MUTED)).child(KEY_ON_DISK));
        match self.vault.out.key.as_ref() {
            Some(key) => {
                let left = key.hides_in(Instant::now()).unwrap_or_default();
                card.child(div().text_size(px(12.)).text_color(c(theme::TEXT_2)).child(match key_env(key.file.key.expose()) {
                    Some(env) => format!("Key {} · {env} · shown once", key.file.key_id),
                    None => format!("Key {} · shown once", key.file.key_id),
                }))
                    .child(div().id("vault-key-value").aria_label("The vault key, shown once").p(px(8.)).rounded(px(6.)).bg(c(theme::SURFACE_2)).font_family(ui::mono(cx)).text_size(px(12.)).child(key.file.key.expose().to_string()))
                    .child(
                        h().gap(px(8.)).items_center()
                            .child(div().text_size(px(11.5)).text_color(c(theme::MUTED)).child(left))
                            .child(div().flex_1())
                            .child(ui::button("vault-key-copy", "Copy", ButtonKind::Outline, None).tooltip(ui::tip(COPY_TIP, None)).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.vault_copy_key(cx))))
                            .child(ui::button("vault-key-save", "Save to file…", ButtonKind::Outline, None).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.vault_save_key(cx))))
                            .child(ui::button("vault-key-done", "Done", ButtonKind::Primary, None).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.vault.out.key = None;
                                this.vault.key_open = false;
                                cx.notify();
                            }))),
                    )
            }
            None => card
                .child(h().gap(px(10.)).items_center().child(div().w(px(70.)).text_size(px(12.)).text_color(c(theme::TEXT_2)).child("Password")).child(div().flex_1().child(ui::input(&self.vault.password))))
                .when_some(self.vault.key_error.clone(), |el, why| el.child(div().id("vault-key-problem").role(Role::Alert).text_size(px(12.)).text_color(c(theme::NEGATIVE)).child(why)))
                .child(
                    h().gap(px(8.)).justify_end()
                        .child(ui::button("vault-key-cancel", "Cancel", ButtonKind::Outline, None).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                            this.vault.key_open = false;
                            this.vault.clear_fields = true;
                            cx.notify();
                        })))
                        .child(ui::button("vault-key-export", if self.vault.key_busy { "Exporting…" } else { "Export key" }, ButtonKind::Primary, Some(ui::kbd("enter", cx))).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.vault_export_key(cx)))),
                ),
        }
    }
}

/// E21: what the vault does not protect against (R2, R3), under every vault view.
fn vault_notices() -> Stateful<Div> {
    v().id("vault-notices").gap(px(4.)).pt(px(14.)).text_size(px(11.5)).line_height(px(16.)).text_color(c(theme::MUTED))
        .child(h().gap(px(6.)).child("ⓘ").child(PLUGINS_SEE_SECRETS))
        .child(h().gap(px(6.)).child("ⓘ").child(KEY_ON_DISK))
}

/// Where the save dialog starts: the home folder.
fn dirs_home() -> std::path::PathBuf {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(std::path::PathBuf::from).unwrap_or_else(std::env::temp_dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    // `gpui_kit::*` brings gpui's own `test` attribute; these are plain tests.
    use core::prelude::v1::test;
    use aurelius_domain::UserId;
    use aurelius_domain::vault::{UserRef, VaultScopeCount};

    const FAKE: &str = "sk-test-FAKE-0000";

    fn alex() -> UserRef {
        UserRef { id: UserId::new(), username: "alex".into() }
    }

    fn item(kind: VaultKind, name: &str, env: &str, value: Option<&str>) -> VaultItemView {
        let by = alex();
        VaultItemView {
            id: VaultItemId::new(),
            project_id: None,
            kind,
            name: name.into(),
            env_name: env.into(),
            description: None,
            value: value.map(str::to_owned),
            undecryptable: false,
            overrides_company: false,
            version: 1,
            created_by: by.clone(),
            updated_by: by,
            created_at_ms: 1,
            updated_at_ms: 1,
        }
    }

    fn project(name: &str, status: aurelius_domain::project::ProjectStatus) -> aurelius_protocol::ProjectSummary {
        aurelius_protocol::ProjectSummary {
            id: ProjectId::new(),
            name: name.into(),
            description: None,
            status,
            version: 1,
            created_at_ms: 0,
            employees_active: 0,
            folder: None,
            dispatcher_employee_id: None,
        }
    }

    fn row(action: VaultAuditAction, actor: VaultActor) -> VaultAuditRow {
        VaultAuditRow { id: 1, item_id: Some(VaultItemId::new()), item_name: Some("GitHub token".into()), project_id: None, action, actor, changed_fields: vec![], via: None, at_ms: 1 }
    }

    /// MU4-01 / MU4-02 (AC1, AC3): the company first, then the active projects and an archived
    /// project that still has items, each with its count; an archived one without items is left out.
    #[test]
    fn the_scopes_are_the_company_and_its_projects() {
        use aurelius_domain::project::ProjectStatus;
        let (web, old, gone) = (project("segrusa-web", ProjectStatus::Active), project("old", ProjectStatus::Archived), project("gone", ProjectStatus::Archived));
        let list = VaultList {
            project_id: None,
            items: vec![],
            company_items: vec![],
            counts: vec![VaultScopeCount { project_id: None, count: 6 }, VaultScopeCount { project_id: Some(old.id), count: 2 }],
        };
        let rows = scopes(Some(&list), &[web.clone(), old.clone(), gone]);
        assert_eq!(rows, vec![(None, "Company".to_string(), 6), (Some(web.id), "segrusa-web".to_string(), 0), (Some(old.id), "old".to_string(), 2)]);
        assert_eq!(scopes(None, &[])[0], (None, "Company".to_string(), 0), "the company shows before its list loads");
    }

    /// MU4-03 AC1/AC2, MU4-04 AC3: text shows its first line, a url is a link, a secret is
    /// masked until it is shown, an undecryptable value says so instead of a value.
    #[test]
    fn a_value_shows_by_its_type() {
        assert_eq!(cell(&item(VaultKind::Text, "Deploy notes", "DEPLOY_NOTES", Some("Run make deploy\nthen wait")), None), Cell::Text("Run make deploy".into()));
        assert_eq!(cell(&item(VaultKind::Url, "Staging", "STAGING_URL", Some("https://stg.example.test")), None), Cell::Link("https://stg.example.test".into()));
        let secret = item(VaultKind::Secret, "GitHub token", "GITHUB_TOKEN", None);
        assert_eq!(cell(&secret, None), Cell::Masked);
        assert_eq!(cell(&secret, Some(FAKE)), Cell::Shown(FAKE.into()));
        let mut lost = item(VaultKind::Secret, "Sentry DSN", "SENTRY_DSN", None);
        lost.undecryptable = true;
        assert_eq!(cell(&lost, Some(FAKE)), Cell::Undecryptable);
        assert_eq!(kind_tag(VaultKind::Secret), "🔒 Secret", "the type is always a word too (§6)");
    }

    /// B7 / P-44: text and url trade places; a secret stays one, and nothing becomes one.
    #[test]
    fn the_type_moves_between_text_and_url_only() {
        assert_eq!(kinds_for(None), vec![VaultKind::Text, VaultKind::Url, VaultKind::Secret]);
        assert_eq!(kinds_for(Some(VaultKind::Text)), vec![VaultKind::Text, VaultKind::Url]);
        assert_eq!(kinds_for(Some(VaultKind::Url)), vec![VaultKind::Text, VaultKind::Url]);
        assert_eq!(kinds_for(Some(VaultKind::Secret)), vec![VaultKind::Secret]);
        // The url becomes text: only the kind goes.
        let url = item(VaultKind::Url, "Staging", "STAGING_URL", Some("https://stg.example.test"));
        let editor = Editor { item: Some(url.id), scope: None, kind: VaultKind::Text, busy: false, failure: None };
        let typed = Typed { name: "Staging", env: "STAGING_URL", value: "https://stg.example.test", note: "" };
        let params = update_params(&editor, &url, &typed).unwrap();
        assert_eq!((params.kind, params.name, params.value), (Some(VaultKind::Text), None, None));
        assert_eq!(backend::vault_failure(&aurelius_client::ClientError::Protocol(aurelius_domain::AppError::new(aurelius_domain::error::ErrorCode::VaultKindLocked, "locked"))), backend::VAULT_KIND_LOCKED);
    }

    /// ui-ontwerp §4: a secret is never filled in; empty keeps it, a new one replaces it.
    #[test]
    fn an_edited_secret_is_kept_unless_replaced() {
        let secret = item(VaultKind::Secret, "GitHub token", "GITHUB_TOKEN", None);
        let editor = Editor { item: Some(secret.id), scope: None, kind: VaultKind::Secret, busy: false, failure: None };
        let keep = Typed { name: "GitHub token", env: "GITHUB_TOKEN", value: "", note: "" };
        assert_eq!(form_problem(&editor, Some(&secret), &keep), None);
        assert!(update_params(&editor, &secret, &keep).is_none(), "nothing changed, nothing is sent");
        let renamed = Typed { name: "GitHub token", env: "GH_TOKEN", value: "", note: "Fine-grained" };
        let params = update_params(&editor, &secret, &renamed).unwrap();
        assert_eq!((params.env_name.as_deref(), params.value, params.description.as_deref()), (Some("GH_TOKEN"), None, Some("Fine-grained")));
        let replaced = Typed { name: "GitHub token", env: "GITHUB_TOKEN", value: FAKE, note: "" };
        assert_eq!(update_params(&editor, &secret, &replaced).unwrap().value.as_deref(), Some(FAKE));
        assert!(!format!("{:?}", update_params(&editor, &secret, &replaced).unwrap()).contains("FAKE"));
    }

    /// The editor's rules before anything goes: a name, a key of A–Z/0–9/_, a full url,
    /// a secret of at least 8 characters (E7, E13); the key follows from the name.
    #[test]
    fn the_editor_checks_before_it_sends() {
        assert_eq!(env_from_name("GitHub token"), "GITHUB_TOKEN");
        assert_eq!(env_from_name("  2nd staging-url! "), "ND_STAGING_URL");
        assert_eq!(env_from_name("Ünïcode"), "N_CODE");
        assert!(env_problem("GITHUB_TOKEN").is_none());
        for bad in ["", "github", "1TOKEN", "A-B", &"A".repeat(81)] {
            assert!(env_problem(bad).is_some(), "{bad}");
        }
        // The names a run keeps are refused here already, in the server's words.
        assert_eq!(env_problem("ANTHROPIC_API_KEY").as_deref(), Some("ANTHROPIC_API_KEY is kept by Aurelius or Claude Code for every run. Choose another key."));
        assert_eq!(value_problem(VaultKind::Secret, "sk-test\nFAKE-0000", false).as_deref(), Some("A secret is one line."));
        assert_eq!(value_problem(VaultKind::Url, "stg.example.test", false).as_deref(), Some(URL_PROBLEM));
        assert_eq!(value_problem(VaultKind::Url, "https://", false).as_deref(), Some(URL_PROBLEM));
        assert!(value_problem(VaultKind::Url, "http://localhost:3000", false).is_none());
        assert!(value_problem(VaultKind::Secret, "short", false).is_some());
        assert!(value_problem(VaultKind::Secret, FAKE, false).is_none());
        assert!(value_problem(VaultKind::Text, " ", false).is_some());
        let new = Editor { item: None, scope: Some(ProjectId::new()), kind: VaultKind::Secret, busy: false, failure: None };
        let typed = Typed { name: "GitHub token", env: "", value: &format!(" {FAKE}\n"), note: " " };
        assert_eq!(form_problem(&new, None, &typed), None);
        let params = create_params(&new, &typed);
        assert_eq!((params.env_name.as_str(), params.value.as_str(), params.description, params.project_id), ("GITHUB_TOKEN", FAKE, None, new.scope));
        assert_eq!(form_problem(&new, None, &Typed { name: "", ..typed }).as_deref(), Some("Give the item a name."));
    }

    /// B6 / P-47 / E19: Show hides after 30 s with a countdown; after it hid (by time, Hide or
    /// leaving) no value and no key is left in this state.
    #[test]
    fn a_shown_value_hides_and_leaves_no_trace() {
        let now = Instant::now();
        let id = VaultItemId::new();
        let mut out = Showing::default();
        out.shown = Some(Shown { item_id: id, value: Revealed::new(FAKE.into()), until: now + SHOW_FOR });
        out.key = Some(KeyShown { file: backend::KeyFile { key_id: "k1".into(), key: Revealed::new("aurelius-vault-key:v1:dev:k1:c2stdGVzdC1GQUtFLTAwMDA".into()) }, until: now + SHOW_FOR, paused: None });
        assert_eq!(out.shown_value(id), Some(FAKE));
        assert_eq!(out.shown_value(VaultItemId::new()), None, "one item at a time");
        assert_eq!(hides_in(now + SHOW_FOR, now).as_deref(), Some("Hides in 0:30"));
        assert_eq!(hides_in(now + Duration::from_millis(26_400), now).as_deref(), Some("Hides in 0:27"));
        assert!(!out.hide_due(now + Duration::from_secs(29)));
        assert!(out.showing());
        assert!(out.hide_due(now + SHOW_FOR));
        assert!(!out.showing() && out.shown_value(id).is_none());
        assert!(!format!("{out:?}").contains("FAKE"));
        assert_eq!(hides_in(now, now), None);
        // Hide (or leaving the page) takes both at once.
        out.shown = Some(Shown { item_id: id, value: Revealed::new(FAKE.into()), until: now + SHOW_FOR });
        out.hide();
        assert!(!out.showing());
        let mut text = FAKE.to_string();
        backend::wipe(&mut text);
        assert!(text.is_empty());
    }

    /// B6 / E19: the clipboard is cleared after 60 s only while it still holds our value;
    /// the window keeps a keyed hash, never the value.
    #[test]
    fn the_clipboard_is_cleared_only_when_it_is_still_ours() {
        assert_eq!(CLEAR_CLIPBOARD_AFTER, Duration::from_secs(60));
        let copied = Copied::of(FAKE, 1);
        assert!(copied.is_ours(Some(FAKE)));
        assert!(!copied.is_ours(Some("something Robin copied since")));
        assert!(!copied.is_ours(None), "an empty clipboard stays as it is");
        assert!(!format!("{copied:?}").contains("FAKE"));
        assert!(!Copied::of(FAKE, 2).is_ours(Some("sk-test-FAKE-0001")));
    }

    /// MU4-09 AC1/AC2 and ui-ontwerp §5: who, what and via, never a value; an edit names its fields.
    #[test]
    fn the_audit_log_says_who_did_what() {
        let me = alex();
        let you = row(VaultAuditAction::Reveal, VaultActor::BoardMember { user: me.clone() });
        assert_eq!((who(&you.actor, Some(me.id)), action_text(&you)), ("You".to_string(), "Revealed".to_string()));
        let robin = VaultActor::BoardMember { user: UserRef { id: UserId::new(), username: "robin".into() } };
        let mut edit = row(VaultAuditAction::Edit, robin);
        edit.changed_fields = vec!["name".into(), "env_name".into(), "description".into()];
        edit.via = Some("Desktop · dev-laptop (Linux)".into());
        assert_eq!((who(&edit.actor, Some(me.id)), action_text(&edit), via(&edit)), ("robin".to_string(), "Edited · name, key, note".to_string(), "Desktop · dev-laptop (Linux)".to_string()));
        let run = row(VaultAuditAction::Deliver, VaultActor::Employee { employee_id: aurelius_domain::EmployeeId::new(), name: "Forge".into(), run_id: Some(aurelius_domain::RunId::new()), task_id: None, ticket_id: Some(aurelius_domain::TicketId::new()) });
        assert_eq!((who(&run.actor, Some(me.id)), action_text(&run), via(&run)), ("Forge (employee)".to_string(), "Read by run".to_string(), "Ticket · run".to_string()));
        for (action, label) in [(VaultAuditAction::Create, "Created"), (VaultAuditAction::Replace, "Replaced"), (VaultAuditAction::Copy, "Copied"), (VaultAuditAction::Archive, "Deleted"), (VaultAuditAction::KeyExport, "Exported the key"), (VaultAuditAction::ToolRead, "Read by tool")] {
            assert_eq!(action_label(action), label);
        }
        // Load older appends; a first page replaces.
        let mut rows = vec![VaultAuditRow { id: 9, ..you.clone() }];
        merge_audit(&mut rows, vec![VaultAuditRow { id: 9, ..you.clone() }, VaultAuditRow { id: 8, ..you.clone() }], Some(9));
        assert_eq!(rows.iter().map(|r| r.id).collect::<Vec<_>>(), [9, 8]);
        merge_audit(&mut rows, vec![VaultAuditRow { id: 10, ..you }], None);
        assert_eq!(rows.iter().map(|r| r.id).collect::<Vec<_>>(), [10]);
    }

    /// MU4-11 AC2: Delete names the item and its key and says what runs lose.
    #[test]
    fn delete_says_what_runs_lose() {
        let secret = item(VaultKind::Secret, "GitHub token", "GITHUB_TOKEN", None);
        assert_eq!(delete_title(Some(&secret)), "Delete GitHub token (GITHUB_TOKEN)?");
        assert!(DELETE_BODY.contains("no longer get it") && DELETE_BODY.contains("keeps it until it ends") && DELETE_BODY.contains("logged"));
    }

    /// E21: the texts for R2 and R3 (Sentinel sec0), under the vault and with the key export.
    #[test]
    fn the_residual_risks_are_spelled_out() {
        assert_eq!(PLUGINS_SEE_SECRETS, "Plugins and MCP servers of an employee see the secrets of its runs.");
        assert_eq!(KEY_ON_DISK, "On Linux, any process of the daemon's user, also an agent through Bash, can read the key.");
    }

    /// B10 / P-46: the question names the items, never a value.
    #[test]
    fn the_secret_question_names_the_items() {
        assert_eq!(held_title(&["GitHub token".into(), "Sentry DSN".into()]), "This looks like a vault secret: GitHub token, Sentry DSN. Send anyway?");
        assert_eq!(held_title(&[]), "This looks like a vault secret. Send anyway?");
    }

    /// R2 D-m1 (m9): text and url say they are no secret; R2 D-m6: a long name gives a key the server takes.
    #[test]
    fn text_and_url_say_they_are_no_secret() {
        assert_eq!(NOT_SECRET, "Not secret: shown to everyone and not redacted. Use Secret for keys and tokens.");
        assert_eq!(value_hint(VaultKind::Text), NOT_SECRET);
        assert!(value_hint(VaultKind::Url).starts_with("https://…") && value_hint(VaultKind::Url).ends_with(NOT_SECRET));
        let long = "Staging database read only token for the nightly export job of the reporting service";
        let env = env_from_name(long);
        assert!(env.len() <= aurelius_domain::vault::MAX_ENV_NAME_CHARS && !env.ends_with('_'), "{env}");
        assert_eq!(env_problem(&env), None);
        assert_eq!(env_from_name(&format!("{} b", "A".repeat(63))), "A".repeat(63), "no _ left at the cut");
    }

    /// R2 D-m3: an edited item that left the list is not created anew on Save.
    #[test]
    fn an_item_gone_elsewhere_is_not_saved_as_new() {
        let gone = Editor { item: Some(VaultItemId::new()), scope: None, kind: VaultKind::Text, busy: false, failure: None };
        let typed = Typed { name: "Deploy notes", env: "", value: "Run make deploy", note: "" };
        assert_eq!(form_problem(&gone, None, &typed).as_deref(), Some(GONE_ELSEWHERE));
        let new = Editor { item: None, ..gone };
        assert_eq!(form_problem(&new, None, &typed), None);
    }

    /// R2 D-m7 / ui-ontwerp §3: losing the focus hides a shown value and the key; QA2 m3: the
    /// key's countdown stands still while its save dialog is open, and it does not hide then.
    #[test]
    fn the_key_waits_for_its_save_dialog() {
        let now = Instant::now();
        let file = || backend::KeyFile { key_id: "k1".into(), key: Revealed::new("aurelius-vault-key:v1:dev:k1:c2stdGVzdC1GQUtFLTAwMDA".into()) };
        let mut out = Showing::default();
        out.key = Some(KeyShown::new(file(), now));
        out.shown = Some(Shown { item_id: VaultItemId::new(), value: Revealed::new(FAKE.into()), until: now + SHOW_FOR });
        out.key.as_mut().unwrap().pause(now + Duration::from_secs(10));
        assert_eq!(out.key.as_ref().unwrap().hides_in(now + Duration::from_secs(50)).as_deref(), Some("Hides in 0:20"), "it stands still");
        assert!(out.hide_on_leave(), "the shown value hides when the window loses the focus");
        assert!(out.shown.is_none() && out.key.is_some(), "the key stays while its dialog has the focus");
        assert!(!out.hide_due(now + Duration::from_secs(60)) && out.key.is_some(), "the paused key does not hide");
        let back = now + Duration::from_secs(90);
        out.key.as_mut().unwrap().resume(back);
        assert_eq!(out.key.as_ref().unwrap().hides_in(back).as_deref(), Some("Hides in 0:20"));
        assert!(!out.hide_due(back + Duration::from_secs(19)));
        assert!(out.hide_due(back + Duration::from_secs(20)) && out.key.is_none());
        out.key = Some(KeyShown::new(file(), now));
        assert!(out.hide_on_leave() && !out.showing(), "without its dialog the key hides too");
        assert_eq!((SHOWN_ANNOUNCE, HIDDEN_ANNOUNCE), ("Shown for 30 seconds", "Hidden"));
    }

    /// QA2 m4 / ui-ontwerp §5: a change of type says so, with or without a new value.
    #[test]
    fn a_change_of_type_shows_in_the_log() {
        let robin = VaultActor::BoardMember { user: UserRef { id: UserId::new(), username: "robin".into() } };
        let mut edit = row(VaultAuditAction::Edit, robin.clone());
        edit.changed_fields = vec!["kind".into()];
        assert_eq!(action_text(&edit), "Edited · type");
        let mut replace = row(VaultAuditAction::Replace, robin);
        replace.changed_fields = vec!["kind".into(), "value".into()];
        assert_eq!(action_text(&replace), "Replaced · type");
        replace.changed_fields = vec!["value".into()];
        assert_eq!(action_text(&replace), "Replaced");
    }

    /// R2 D-m4: Cancel puts a held message back where it was written, after what is typed
    /// there, with its images and files; QA2 m5: the dialog names the mark as it is stored.
    #[test]
    fn a_held_message_goes_back_where_it_was_written() {
        use aurelius_domain::{Command, ConversationId, EmployeeId};
        let conversation = ConversationId::new();
        let chat = Command::SendEmployeeMessage(aurelius_domain::employee::SendEmployeeMessage { confirm_secret: true, employee_id: EmployeeId::new(), text: format!("token {FAKE}"), conversation_id: conversation, images: vec!["/tmp/shot.png".into()], files: vec!["/tmp/notes.txt".into()], interrupt: false });
        assert_eq!(held_in(&chat), HeldIn::Chat(conversation));
        assert_eq!(attachments_back(&chat), (vec![std::path::PathBuf::from("/tmp/shot.png")], vec![std::path::PathBuf::from("/tmp/notes.txt")]));
        let channel = Some(ConversationId::new());
        let group = Command::SendGroupMessage(aurelius_domain::group_chat::SendGroupMessage { confirm_secret: true, text: "x".into(), images: vec![], files: vec![], channel, thread_parent: None });
        assert_eq!(held_in(&group), HeldIn::Channel(channel));
        let post = Command::PostTicketMessage(aurelius_domain::ticket::PostTicketMessage { confirm_secret: true, ticket_id: aurelius_domain::TicketId::new(), text: "x".into(), images: vec![], files: vec![], thread_parent: None });
        assert_eq!(held_in(&post), HeldIn::Ticket);
        assert_eq!(text_back("", "held"), "held");
        assert_eq!(text_back("typed since", "held"), "typed since\n\nheld", "nothing is lost");
        assert!(HELD_BODY.contains("«secret:KEY»") && !HELD_BODY.contains('‹'));
    }

    /// AUR-CHAT-001 R2 D-M1: sent in #a, the user went to #b before the hold came; Cancel goes
    /// back to #a before the composer (still showing #b) is read, as `vault_hold_cancel` does.
    #[test]
    fn a_held_message_cancelled_after_switching_channels_comes_back_in_its_own_channel() {
        use crate::screens::discussion::{DraftPlace, Drafts};
        use aurelius_domain::ConversationId;
        let (a, b) = (DraftPlace::Channel(Some(ConversationId::new())), DraftPlace::Channel(Some(ConversationId::new())));
        let mut drafts = Drafts::default();
        assert_eq!(drafts.moved(a, ""), None, "the composer starts in #a");
        assert_eq!(drafts.moved(b, ""), Some(String::new()), "sent in #a, then over to #b");
        let typed_in_b = "typed in b";
        let in_a = held_back_in(&mut drafts, a, typed_in_b, "token sk-123");
        assert_eq!(in_a, "token sk-123", "the held text is back in #a's composer, without #b's text");
        assert_eq!(drafts.at(), Some(a), "render has nothing left to swap");
        assert_eq!(drafts.moved(b, &in_a).as_deref(), Some(typed_in_b), "#b keeps only its own draft");
        assert_eq!(drafts.moved(a, typed_in_b).as_deref(), Some("token sk-123"), "#a keeps the held text");
        // Cancel in the channel it was sent from: the text typed since stays above the held text.
        assert_eq!(held_back_in(&mut drafts, a, "since", "held"), "since\n\nheld");
    }

    /// AUR-MU-04-D3: the other text commands' Cancel. Feedback goes back to its field; the New ticket
    /// form and a ticket's field kept their text; a closed dialog has nothing to put back, and says so.
    #[test]
    fn a_held_command_says_what_cancel_does() {
        use aurelius_domain::{Command, ProjectId, TicketId};
        let ticket_id = TicketId::new();
        let revise = Command::RevisePlan(aurelius_domain::ticket::RevisePlan { confirm_secret: false, ticket_id, feedback: format!("Sign with {FAKE}") });
        let back = Command::SendBackTicket(aurelius_domain::ticket::SendBackTicket { confirm_secret: false, ticket_id, feedback: "x".into() });
        assert_eq!((held_in(&revise), held_in(&back)), (HeldIn::Feedback(ticket_id), HeldIn::Feedback(ticket_id)));
        let decide = Command::DecidePlan(aurelius_domain::approval::DecidePlan { confirm_secret: false, approval_id: aurelius_domain::ApprovalId::new(), decision: aurelius_domain::approval::PlanDecision::Revise, feedback: Some("x".into()) });
        assert_eq!(held_in(&decide), HeldIn::Plan);
        let edit = Command::UpdateTicket(crate::screens::board::empty_update(ticket_id));
        assert_eq!(held_in(&edit), HeldIn::Ticket);
        assert_eq!(held_body(Some(&edit)), HELD_BODY);
        let idea = Command::AddIdea(aurelius_domain::idea::AddIdea { confirm_secret: false, project_id: ProjectId::new(), title: "x".into(), note: None, kind: Default::default(), files: vec![] });
        assert_eq!(held_in(&idea), HeldIn::Form);
        assert_eq!(held_body(Some(&idea)), HELD_FORM_BODY);
        assert!(HELD_FORM_BODY.contains("«secret:KEY»") && HELD_FORM_BODY.contains("Cancel drops it"));
        let hold = crate::backend::SecretHold::new(vec![], revise);
        assert_eq!(hold.text(), format!("Sign with {FAKE}"), "what Cancel puts back");
    }

    /// E9: the key file is the user's only (0600), also over a file that was there.
    #[cfg(unix)]
    #[test]
    fn the_key_file_is_readable_by_its_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let line = "aurelius-vault-key:v1:prod:ab12cd34:c2stdGVzdC1GQUtFLTAwMDA";
        assert_eq!((key_env(line), key_env("aurelius-vault-key:v2:prod:x:y"), key_env("aurelius-vault-key:v1:qa:x:y")), (Some("prod"), None, None));
        assert_eq!(key_file_name("ab12-cd34/../x", "c2stdGVzdC1GQUtFLTAwMDA"), "aurelius-vault-key-ab12cd34x.txt");
        let path = dir.path().join(key_file_name("ab12-cd34/../x", line));
        assert_eq!(path.file_name().unwrap(), "aurelius-vault-key-prod-ab12cd34x.txt");
        std::fs::write(&path, "old").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        save_key(&path, line).unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), format!("{line}\n"), "the one line the import reads");
        let fresh = dir.path().join("fresh.txt");
        save_key(&fresh, "k").unwrap();
        assert_eq!(std::fs::metadata(&fresh).unwrap().permissions().mode() & 0o777, 0o600);
    }
}
