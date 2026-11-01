//! Board members and signing in (protocol 5, Fase 3; ui-ontwerp §1, §2, P-34).
//!
//! * A server without board members shows only "Create the first board member"
//!   (MU1-20, MU3-02 AC5): no sidebar, no page, Settings → Servers stays reachable;
//!   the new board member is signed in on this device at once.
//! * Without a session this window shows only "Sign in" (MU3-02 AC1, the gate of
//!   MU3-06): username and password, one generic refusal, a countdown after too
//!   many tries. A device that was signed out on the server says so on it.
//! * A temporary password (B10) must be changed before anything else (P-34).
//! * Signed in, a board member makes another one, sets a temporary password for
//!   one, or disables one (MU1-22, MU3-05).
//!
//! A password lives only in its field until the server answered: never in
//! ui.json, a log, a toast or a notice. The session token lives in the OS
//! credential store (`backend::Tokens`), never in this window.

use crate::app::{AppView, ConfirmKind, Overlay};
use crate::backend::{self, MemberFailure, MemberField, Password, UserCall};
use crate::keymap::*;
use crate::theme::{self, c};
use crate::ui::{self, ButtonKind, h, v};
use aurelius_domain::UserId;
use aurelius_domain::user::{SessionsSnapshot, SignOutReason, UserView, WhoAmI};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::collections::HashMap;

/// What the window may show, from the server's answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gate {
    /// A board member is signed in here (or the server did not say yet).
    Open,
    /// The server has no board members: only the onboarding (MU1-20 AC1).
    FirstBoardMember,
    /// Board members exist, this device has no session: sign in (MU3-02 AC1).
    SignIn,
    /// Signed in with a temporary password: change it first (B10).
    ChangePassword,
}

pub fn gate(who: Option<&WhoAmI>) -> Gate {
    match who {
        Some(w) if !w.board_members_exist => Gate::FirstBoardMember,
        Some(w) if w.user.is_none() => Gate::SignIn,
        Some(w) if w.must_change_password => Gate::ChangePassword,
        _ => Gate::Open,
    }
}

/// A board member screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Username, password, repeat (the onboarding, or a new board member).
    Create,
    /// "Sign in".
    SignIn,
    /// The own password: current (or temporary), new, repeat.
    ChangePassword,
    /// A temporary password for another board member (B10, P-34).
    TempPassword(UserId),
}

/// The screen on top: what the gate demands, else what the user opened (Back pops it).
pub fn current_step(gate: Gate, stack: &[Step]) -> Option<Step> {
    match gate {
        Gate::FirstBoardMember => Some(Step::Create),
        Gate::SignIn => Some(Step::SignIn),
        Gate::ChangePassword => Some(Step::ChangePassword),
        Gate::Open => stack.last().copied(),
    }
}

/// The board member screen over `screen`: at the gate Settings shows only its
/// Servers tab (another server can be chosen there), elsewhere the gate's card.
pub fn shown_step(gate: Gate, stack: &[Step], screen: crate::app::Page) -> Option<Step> {
    current_step(gate, stack).filter(|_| !(gate != Gate::Open && screen == crate::app::Page::Settings))
}

/// The page when the gate closes (a sign-out, or the server ended the session):
/// Settings (Servers, Account, Sessions) gives way to the page before it, so the
/// sign-in card is on top at once (QA3 D1). Settings opened at the gate stays.
pub fn page_at_the_gate(screen: crate::app::Page, back_stack: &mut Vec<crate::app::Page>) -> crate::app::Page {
    if screen != crate::app::Page::Settings {
        return screen;
    }
    while let Some(previous) = back_stack.pop() {
        if previous != crate::app::Page::Settings {
            return previous;
        }
    }
    crate::app::Page::Dashboard
}

/// OV-12 for the username, as the field shows it; `None`: it is fine.
pub fn username_problem(text: &str) -> Option<String> {
    aurelius_domain::user::Username::parse(text).err().map(|e| e.to_string())
}

pub fn password_problem(text: &str) -> Option<String> {
    aurelius_domain::user::check_password(text).err().map(|e| e.to_string())
}

pub fn repeat_problem(password: &str, repeat: &str) -> Option<String> {
    (password != repeat).then(|| "The passwords are not the same.".to_string())
}

pub fn current_problem(text: &str) -> Option<String> {
    text.is_empty().then(|| "Type your current password.".to_string())
}

/// The fields of the forms (`Form::left` by this index).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Username,
    Password,
    Repeat,
    /// The current (or temporary) password of a password change.
    Current,
}

/// A problem shows once the field was left or the form was sent (§11.1); a
/// refusal of the server shows under its field until that field changes.
/// `values`: username, password, repeat, current.
pub fn shown_problem(field: Field, form: &Form, values: [&str; 4]) -> Option<String> {
    let server = form.failure.as_ref().filter(|f| match f.field {
        Some(MemberField::Username) => field == Field::Username,
        Some(MemberField::Password) => field == Field::Password,
        Some(MemberField::Current) => field == Field::Current,
        None => false,
    });
    if let Some(f) = server {
        return Some(f.message.clone());
    }
    let index = field as usize;
    if !(form.left[index] || form.sent) {
        return None;
    }
    match field {
        Field::Username => username_problem(values[0]),
        Field::Password => password_problem(values[1]),
        Field::Repeat => repeat_problem(values[1], values[2]),
        Field::Current => current_problem(values[3]),
    }
}

/// A busy form whose connection is no longer online will hear nothing more.
pub fn dropped_while_busy(form: &Form, connection: &backend::Connection) -> bool {
    form.busy && !matches!(connection, backend::Connection::Online { .. })
}

/// Every rule of the create form holds.
pub fn create_ready(values: [&str; 3]) -> bool {
    username_problem(values[0]).is_none() && password_problem(values[1]).is_none() && repeat_problem(values[1], values[2]).is_none()
}

/// A new password with its repeat holds (and the current one is typed, for the own).
pub fn password_ready(new: &str, repeat: &str, current: Option<&str>) -> bool {
    password_problem(new).is_none() && repeat_problem(new, repeat).is_none() && current.is_none_or(|c| current_problem(c).is_none())
}

/// The sign-in may go: both fields typed. No rule is shown about them (that would
/// tell what a username looks like).
pub fn sign_in_problem(username: &str, password: &str) -> Option<&'static str> {
    if username.trim().is_empty() || password.is_empty() {
        return Some("Type your username and password.");
    }
    None
}

/// The countdown after too many tries ("Try again in 0:27"); none once it passed.
pub fn countdown(until_ms: Option<i64>, now_ms: i64) -> Option<String> {
    let left = until_ms? - now_ms;
    if left <= 0 {
        return None;
    }
    let secs = (left as u64).div_ceil(1000);
    Some(format!("Try again in {}:{:02}", secs / 60, secs % 60))
}

/// The sign-in's refusal under its fields: "Too many attempts" counts down and is
/// gone at 0 (the server's frozen "Try again in 0:44." no longer holds, QA3 D-m1).
pub fn sign_in_refusal(failure: Option<&MemberFailure>, wait: Option<&str>) -> Option<String> {
    let f = failure?;
    match (f.retry_after_ms, wait) {
        (_, Some(wait)) => Some(format!("Too many attempts. {wait}.")),
        (Some(_), None) => None,
        (None, None) => Some(f.message.clone()),
    }
}

/// What stays open while only the sign-in shows: nothing but the signed-out modal over it.
pub fn kept_at_the_gate(overlay: Overlay) -> bool {
    matches!(overlay, Overlay::None | Overlay::SignedOut)
}

/// The modal's title (ui-ontwerp §1).
pub const SIGNED_OUT_TITLE: &str = "You were signed out on this device.";

/// What the sign-in screen says when the server ended this device's session.
pub fn signed_out_notice(reason: SignOutReason) -> &'static str {
    match reason {
        SignOutReason::LoggedOut | SignOutReason::Revoked | SignOutReason::Expired => "You were signed out on this device.",
        SignOutReason::PasswordChanged => "You were signed out on this device: the password changed.",
        SignOutReason::Disabled => "You were signed out on this device: this board member was disabled.",
    }
}

/// What the forms keep besides their fields.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Form {
    /// Username, password, repeat, current: left at least once.
    pub left: [bool; 4],
    pub sent: bool,
    /// Waiting for the server.
    pub busy: bool,
    pub failure: Option<MemberFailure>,
    /// The eye: the password shown in this window.
    pub revealed: bool,
    /// Too many tries: no sign-in before this moment (ms since 1970).
    pub retry_until_ms: Option<i64>,
}

/// ui.json `board_members`: the board member of Fase 1's choice per server. Protocol 5
/// keeps no such choice (a session token instead), so it is removed when seen.
const OLD_KEY: &str = "board_members";

/// What a new answer of the server means for the window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Arrival {
    /// The same board member as before (a reconnect), or still none.
    Same,
    /// The onboarding is done: "Welcome, alex."
    Welcome(String),
    /// Signed in: "Signed in as robin."
    SignedIn(String),
}

/// `before`: none at the first hello of a backend, which greets nobody (the
/// session of this device is simply back).
pub fn arrival(before: Option<&WhoAmI>, now: &WhoAmI) -> Arrival {
    let Some(before) = before else { return Arrival::Same };
    match &now.user {
        Some(u) if before.user.as_ref().map(|b| b.id) != Some(u.id) => {
            if before.board_members_exist {
                Arrival::SignedIn(u.username.clone())
            } else {
                Arrival::Welcome(u.username.clone())
            }
        }
        _ => Arrival::Same,
    }
}

/// The active board members (not disabled).
pub fn active_count(list: &[UserView]) -> usize {
    list.iter().filter(|u| u.disabled_at_ms.is_none()).count()
}

/// Disable is offered for an active board member, but never for the last one
/// (MU3-05 AC4; the server refuses it too with `last_board_member`).
pub fn may_disable(list: &[UserView], user: &UserView) -> Result<(), &'static str> {
    if user.disabled_at_ms.is_some() {
        return Err("Disabled already.");
    }
    if active_count(list) <= 1 {
        return Err(backend::LAST_BOARD_MEMBER);
    }
    Ok(())
}

/// What a board member was typing on this window (R3 m1): the chat composer with
/// its picked images and files, the drafts of the other Team chat channels
/// (AUR-CHAT-001 R2 D-M2), the ticket discussion's composer and the feedback field,
/// each with the ticket it belongs to.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Drafts {
    pub chat: String,
    pub images: Vec<std::path::PathBuf>,
    pub files: Vec<(std::path::PathBuf, u64)>,
    pub channels: crate::screens::discussion::Drafts,
    pub discussion: String,
    pub discussion_for: Option<aurelius_domain::TicketId>,
    pub feedback: String,
    pub feedback_for: Option<aurelius_domain::TicketId>,
}

/// Another board member on this window (or none, signed out): `shown`, the drafts
/// on screen, are kept for the one before, and the drafts the next one left here
/// come back (empty for someone new). Nobody sees or sends another's draft.
pub fn swap_drafts(kept: &mut HashMap<UserId, Drafts>, from: Option<UserId>, to: Option<UserId>, shown: Drafts) -> Drafts {
    if let Some(from) = from {
        if shown.is_empty() {
            kept.remove(&from);
        } else {
            kept.insert(from, shown);
        }
    }
    to.and_then(|to| kept.remove(&to)).unwrap_or_default()
}

impl Drafts {
    /// Nothing typed: where the composer was does not count.
    pub fn is_empty(&self) -> bool {
        self.channels.is_empty() && *self == Drafts { channels: self.channels.clone(), ..Drafts::default() }
    }
}

/// The drafts on screen leave with their board member (R2 D-M2): the channels'
/// drafts go along, and the window keeps none of them for the next one.
pub fn leaving(mut shown: Drafts, channels: &mut crate::screens::discussion::Drafts) -> Drafts {
    shown.channels = std::mem::take(channels);
    shown
}

/// The board member screens' state in a window.
pub struct Members {
    /// The server's last word (none until the first hello of this backend).
    pub who: Option<WhoAmI>,
    pub list: Vec<UserView>,
    /// What the user opened on top of the gate (Back pops it).
    pub stack: Vec<Step>,
    pub form: Form,
    pub username: Entity<InputState>,
    pub password: Entity<InputState>,
    pub repeat: Entity<InputState>,
    pub current: Entity<InputState>,
    /// Empty the fields on the next frame (an update has no window).
    pub clear_fields: bool,
    pub clear_password: bool,
    /// The server ended this device's session: the sign-in screen says so until the next sign-in.
    pub signed_out: Option<SignOutReason>,
    /// Signed-in devices by board member (none: the own), as last read.
    pub sessions: HashMap<Option<UserId>, Result<SessionsSnapshot, String>>,
    /// Whose sessions show under their row in Board members.
    pub sessions_open: Option<UserId>,
    /// The countdown repaints once a second while it runs.
    pub counting: bool,
    /// Drafts of the board members not signed in now, by user id (R3 m1).
    pub drafts: HashMap<UserId, Drafts>,
    /// Drafts to put in the composers on the next frame (an update has no window).
    pub drafts_shown: Option<Drafts>,
}

impl Members {
    pub fn new(window: &mut Window, cx: &mut Context<AppView>) -> Self {
        Self {
            who: None,
            list: Vec::new(),
            stack: Vec::new(),
            form: Form::default(),
            username: cx.new(|cx| InputState::new(window, cx).placeholder("alex")),
            password: cx.new(|cx| InputState::new(window, cx).masked(true).placeholder("At least 12 characters")),
            repeat: cx.new(|cx| InputState::new(window, cx).masked(true).placeholder("The same password again")),
            current: cx.new(|cx| InputState::new(window, cx).masked(true)),
            clear_fields: false,
            clear_password: false,
            signed_out: None,
            sessions: HashMap::new(),
            sessions_open: None,
            counting: false,
            drafts: HashMap::new(),
            drafts_shown: None,
        }
    }

    /// The fields' events: Enter goes to the next field and sends on the last,
    /// Ctrl+Enter sends from any; leaving a field shows its rule.
    pub fn subscriptions(&self, window: &mut Window, cx: &mut Context<AppView>) -> Vec<Subscription> {
        let mut out = Vec::new();
        for field in [Field::Username, Field::Password, Field::Repeat, Field::Current] {
            let input = match field {
                Field::Username => &self.username,
                Field::Password => &self.password,
                Field::Repeat => &self.repeat,
                Field::Current => &self.current,
            };
            out.push(cx.subscribe_in(input, window, move |this: &mut AppView, _, event: &InputEvent, w, cx| match event {
                InputEvent::PressEnter { secondary, .. } => this.members_enter(field, *secondary, w, cx),
                InputEvent::Blur => {
                    this.members.form.left[field as usize] = true;
                    cx.notify();
                }
                InputEvent::Change => {
                    // A refusal of the server holds until its field changes; the sign-in's
                    // one generic refusal until either changes.
                    let about = this.members.form.failure.as_ref().map(|f| f.field);
                    let changed = match about {
                        Some(Some(MemberField::Username)) => field == Field::Username,
                        Some(Some(MemberField::Password)) => field == Field::Password,
                        Some(Some(MemberField::Current)) => field == Field::Current,
                        Some(None) => this.member_step() == Some(Step::SignIn) && this.members.form.retry_until_ms.is_none(),
                        None => false,
                    };
                    if changed {
                        this.members.form.failure = None;
                    }
                    cx.notify();
                }
                _ => {}
            }));
        }
        out
    }
}

impl AppView {
    /// The key of the server in use in ui.json and the credential store (the same as the pins').
    pub fn server_key(&self) -> String {
        self.remote_host.as_ref().map_or_else(|| "local".to_string(), |(id, _)| id.to_string())
    }

    /// Where this window keeps the session token of the server in use.
    pub fn server_tokens(&self) -> backend::Tokens {
        backend::Tokens::os(self.server_key())
    }

    pub fn member_gate(&self) -> Gate {
        gate(self.members.who.as_ref())
    }

    /// The board member screen to show instead of the page, if any.
    pub fn member_step(&self) -> Option<Step> {
        current_step(self.member_gate(), &self.members.stack)
    }

    /// The signed-in board member ("as alex").
    pub fn active_member(&self) -> Option<&UserView> {
        self.members.who.as_ref().and_then(|w| w.user.as_ref())
    }

    /// A new backend: nothing of the previous server's board members is kept.
    pub fn members_reset(&mut self, window: Option<&mut Window>, cx: &mut Context<Self>) {
        self.members.who = None;
        self.members.list.clear();
        self.members.stack.clear();
        self.members.sessions.clear();
        self.members.sessions_open = None;
        self.members.signed_out = None;
        if let Some(window) = window {
            self.members_clear(window, cx);
        } else {
            self.members.form = Form::default();
            self.members.clear_fields = true;
        }
    }

    pub fn apply_who(&mut self, who: WhoAmI, cx: &mut Context<Self>) {
        // Fase 1 remembered a board member per server; a session replaces it.
        let prefs = crate::app::ui_prefs();
        if !prefs[OLD_KEY].is_null() {
            crate::app::set_ui_value(OLD_KEY, serde_json::Value::Null);
        }
        let arrived = arrival(self.members.who.as_ref(), &who);
        let was = self.member_gate();
        let from = self.active_member().map(|u| u.id);
        let to = who.user.as_ref().map(|u| u.id);
        let changed_user = from != to;
        // The first hello of a backend: nobody's drafts on screen yet.
        let first = self.members.who.is_none();
        self.members.who = Some(who);
        if changed_user {
            self.members.sessions.clear();
            self.claude_reset();
            // The previous board member's /usage is not this one's (R3 m2, P-37).
            self.subscription = None;
            if !first {
                let shown = match self.members.drafts_shown.take() {
                    Some(shown) => shown,
                    None => leaving(self.drafts_on_screen(cx), &mut self.drafts),
                };
                self.members.drafts_shown = Some(swap_drafts(&mut self.members.drafts, from, to, shown));
            }
        }
        match arrived {
            Arrival::Same => {}
            Arrival::Welcome(name) => {
                self.members_done(cx);
                self.notify_user(format!("Welcome, {name}."), false, cx);
            }
            Arrival::SignedIn(name) => {
                self.members.signed_out = None;
                self.members_done(cx);
                if self.member_gate() == Gate::Open {
                    self.notify_user(format!("Signed in as {name}."), false, cx);
                }
            }
        }
        // Signed out: the sign-in starts from empty fields, its card on top.
        if was == Gate::Open && self.member_gate() == Gate::SignIn {
            self.members.stack.clear();
            self.members.form = Form::default();
            self.members.clear_fields = true;
            self.screen = page_at_the_gate(self.screen, &mut self.back_stack);
        }
        // The gate opened (signed in, or the temporary password changed): load what the page shows.
        if was != Gate::Open && self.member_gate() == Gate::Open {
            self.members_done(cx);
            self.signed_in_reload();
        }
        cx.notify();
    }

    fn drafts_on_screen(&self, cx: &Context<Self>) -> Drafts {
        Drafts {
            chat: self.composer.read(cx).value().to_string(),
            images: self.pending_images.clone(),
            files: self.pending_files.clone(),
            // Taken along by `leaving`.
            channels: Default::default(),
            discussion: self.board.composer.read(cx).value().to_string(),
            discussion_for: self.board.composer_for,
            feedback: self.board.feedback.read(cx).value().to_string(),
            feedback_for: self.board.feedback_for,
        }
    }

    /// Puts the drafts of the board member signed in now in the composers.
    pub fn members_show_drafts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(d) = self.members.drafts_shown.take() else { return };
        self.composer.update(cx, |input, cx| input.set_value(d.chat, window, cx));
        self.pending_images = d.images;
        self.pending_files = d.files;
        self.board.composer.update(cx, |input, cx| input.set_value(d.discussion, window, cx));
        self.board.composer_for = d.discussion_for;
        self.board.composer_clear = false;
        self.board.feedback.update(cx, |input, cx| input.set_value(d.feedback, window, cx));
        self.board.feedback_for = d.feedback_for;
        // `chat` is the draft of the place the channels' drafts were left at.
        self.drafts = d.channels;
        self.settle_draft(window, cx);
    }

    /// The server ended this device's session (MU3-04 AC2, MU3-06 AC3): the sign-in
    /// screen at once, with why. A draft in a composer stays on this device, for the
    /// same board member only (`swap_drafts`).
    pub fn apply_signed_out(&mut self, reason: SignOutReason, cx: &mut Context<Self>) {
        self.members.signed_out = Some(reason);
        // A modal first (ui-ontwerp §1), over whatever was open; the sign-in is beneath it.
        self.overlay = Overlay::SignedOut;
        self.screen = page_at_the_gate(self.screen, &mut self.back_stack);
        self.members.stack.clear();
        self.members.form = Form::default();
        self.members.clear_fields = true;
        cx.notify();
    }

    /// The connection went: a form still waiting for the server stops waiting
    /// and says so (Trace M3a m1), whatever the backend still sends.
    pub fn members_connection(&mut self, connection: &backend::Connection, cx: &mut Context<Self>) {
        if dropped_while_busy(&self.members.form, connection) {
            self.apply_member_failure(MemberFailure::general(backend::MEMBER_CONNECTION_DROPPED), cx);
        }
    }

    pub fn apply_member_failure(&mut self, failure: MemberFailure, cx: &mut Context<Self>) {
        self.members.form.busy = false;
        // A refused sign-in starts from an empty password.
        if self.member_step() == Some(Step::SignIn) {
            self.members.clear_password = true;
            self.members.form.retry_until_ms = failure.retry_after_ms.map(|ms| ui::now_ms() + ms as i64);
            if self.members.form.retry_until_ms.is_some() {
                self.members_count_down(cx);
            }
        }
        self.members.form.failure = Some(failure);
        cx.notify();
    }

    /// Repaint once a second while the sign-in counts down.
    fn members_count_down(&mut self, cx: &mut Context<Self>) {
        if self.members.counting {
            return;
        }
        self.members.counting = true;
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(std::time::Duration::from_secs(1)).await;
            let alive = this.update(cx, |this, cx| {
                let running = countdown(this.members.form.retry_until_ms, ui::now_ms()).is_some();
                if !running {
                    this.members.counting = false;
                    this.members.form.retry_until_ms = None;
                    // At 0 the sign-in is open again: no refusal left (QA3 D-m1).
                    if this.members.form.failure.as_ref().is_some_and(|f| f.retry_after_ms.is_some()) {
                        this.members.form.failure = None;
                    }
                }
                cx.notify();
                running
            });
            if !matches!(alive, Ok(true)) {
                break;
            }
        })
        .detach();
    }

    pub fn apply_member_created(&mut self, name: String, cx: &mut Context<Self>) {
        self.members_done(cx);
        self.notify_user(format!("{name} is a board member now."), false, cx);
    }

    /// A write needs a board member: the session ended somewhere; sign in again.
    pub fn apply_member_required(&mut self, cx: &mut Context<Self>) {
        let _ = self.requests_handle().send(backend::Request::BoardMembers(UserCall::List));
        self.notify_user(backend::BOARD_MEMBER_REQUIRED, true, cx);
    }

    /// What Settings → Account and Board members heard back.
    pub fn apply_account(&mut self, update: backend::AccountUpdate, cx: &mut Context<Self>) {
        use backend::AccountUpdate::*;
        match update {
            PasswordSaved(None) => {
                self.members_done(cx);
                self.notify_user("Password changed. Your other devices are signed out.", false, cx);
            }
            PasswordSaved(Some(name)) => {
                self.members_done(cx);
                self.notify_user(format!("Temporary password set. {name} must change it at the next sign-in; their devices are signed out."), false, cx);
            }
            PasswordFailed(failure) => {
                self.members.form.busy = false;
                self.members.form.failure = Some(failure);
            }
            Revoked(1) => self.notify_user("Signed out. That device needs its password to come back.", false, cx),
            Revoked(n) => self.notify_user(format!("{n} devices signed out."), false, cx),
            Disabled(user) => self.notify_user(format!("{} is disabled: signed out everywhere, and their Claude login is disconnected. Their tickets keep them as creator.", user.username), false, cx),
            Failed(message) => self.notify_user(message, true, cx),
        }
        cx.notify();
    }

    pub fn apply_sessions(&mut self, of: Option<UserId>, result: Result<SessionsSnapshot, String>, cx: &mut Context<Self>) {
        self.members.sessions.insert(of, result);
        cx.notify();
    }

    /// The flow ended well: back to the app, fields emptied on the next frame.
    fn members_done(&mut self, cx: &mut Context<Self>) {
        self.members.stack.clear();
        self.members.form = Form::default();
        self.members.clear_fields = true;
        cx.notify();
    }

    fn members_clear(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let retry = self.members.form.retry_until_ms;
        self.members.form = Form { retry_until_ms: retry, ..Form::default() };
        for input in [self.members.username.clone(), self.members.password.clone(), self.members.repeat.clone(), self.members.current.clone()] {
            input.update(cx, |i, cx| {
                i.set_value("", window, cx);
                i.set_masked(true, window, cx);
            });
        }
        // The username field is a username field again (the create form shows its rule).
        self.members.username.update(cx, |i, cx| i.set_masked(false, window, cx));
    }

    fn members_open(&mut self, step: Step, window: &mut Window, cx: &mut Context<Self>) {
        self.members_clear(window, cx);
        self.members.stack.push(step);
        self.close_overlay_now();
        match step {
            Step::Create | Step::SignIn => self.members.username.update(cx, |i, cx| i.focus(window, cx)),
            Step::ChangePassword => self.members.current.update(cx, |i, cx| i.focus(window, cx)),
            Step::TempPassword(_) => self.members.password.update(cx, |i, cx| i.focus(window, cx)),
        }
        cx.notify();
    }

    /// New board member (Settings → Company).
    pub fn members_new(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.members_open(Step::Create, window, cx);
    }

    /// Change password… (Settings → Account).
    pub fn members_change_password(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.members_open(Step::ChangePassword, window, cx);
    }

    /// Set a temporary password… for another board member (P-34).
    pub fn members_temp_password(&mut self, user: UserId, window: &mut Window, cx: &mut Context<Self>) {
        self.members_open(Step::TempPassword(user), window, cx);
    }

    /// Sign out on this device (MU3-04 AC3): the sign-in screen follows.
    pub fn members_sign_out(&mut self, cx: &mut Context<Self>) {
        let _ = self.requests_handle().send(backend::Request::BoardMembers(UserCall::Logout));
        self.members.signed_out = None;
        self.close_overlay_now();
        cx.notify();
    }

    /// Back from a board member screen; false when there is none to go back from.
    pub fn members_back(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.member_gate() != Gate::Open || self.members.stack.pop().is_none() {
            return false;
        }
        self.members_clear(window, cx);
        cx.notify();
        true
    }

    fn members_enter(&mut self, field: Field, secondary: bool, window: &mut Window, cx: &mut Context<Self>) {
        let focus = |input: &Entity<InputState>, window: &mut Window, cx: &mut Context<Self>| input.update(cx, |i, cx| i.focus(window, cx));
        match self.member_step() {
            Some(Step::Create) if !secondary && field == Field::Username => focus(&self.members.password.clone(), window, cx),
            Some(Step::Create) if !secondary && field == Field::Password => focus(&self.members.repeat.clone(), window, cx),
            Some(Step::Create) => self.members_create(cx),
            Some(Step::SignIn) if !secondary && field == Field::Username => focus(&self.members.password.clone(), window, cx),
            Some(Step::SignIn) => self.members_sign_in(cx),
            Some(Step::ChangePassword) if !secondary && field == Field::Current => focus(&self.members.password.clone(), window, cx),
            Some(Step::ChangePassword | Step::TempPassword(_)) if !secondary && field == Field::Password => focus(&self.members.repeat.clone(), window, cx),
            Some(Step::ChangePassword) => self.members_save_password(None, cx),
            Some(Step::TempPassword(user)) => self.members_save_password(Some(user), cx),
            None => {}
        }
    }

    pub fn members_create(&mut self, cx: &mut Context<Self>) {
        if self.members.form.busy {
            return;
        }
        let username = self.members.username.read(cx).value().to_string();
        let password = self.members.password.read(cx).value().to_string();
        let repeat = self.members.repeat.read(cx).value().to_string();
        self.members.form.sent = true;
        self.members.form.failure = None;
        if create_ready([&username, &password, &repeat]) {
            self.members.form.busy = true;
            let call = UserCall::Create { username: username.trim().to_string(), password: Password::new(password) };
            let _ = self.requests_handle().send(backend::Request::BoardMembers(call));
        }
        cx.notify();
    }

    pub fn members_sign_in(&mut self, cx: &mut Context<Self>) {
        if self.members.form.busy || countdown(self.members.form.retry_until_ms, ui::now_ms()).is_some() {
            return;
        }
        let username = self.members.username.read(cx).value().trim().to_string();
        let password = self.members.password.read(cx).value().to_string();
        self.members.form.sent = true;
        self.members.form.retry_until_ms = None;
        if let Some(problem) = sign_in_problem(&username, &password) {
            self.members.form.failure = Some(MemberFailure::general(problem));
        } else {
            self.members.form.failure = None;
            self.members.form.busy = true;
            let _ = self.requests_handle().send(backend::Request::BoardMembers(UserCall::Login { username, password: Password::new(password) }));
        }
        cx.notify();
    }

    /// The own password (`None`, with the current one), or a temporary one for `user`.
    pub fn members_save_password(&mut self, user: Option<UserId>, cx: &mut Context<Self>) {
        if self.members.form.busy {
            return;
        }
        let new = self.members.password.read(cx).value().to_string();
        let repeat = self.members.repeat.read(cx).value().to_string();
        let current = self.members.current.read(cx).value().to_string();
        self.members.form.sent = true;
        self.members.form.failure = None;
        let own = user.is_none();
        if password_ready(&new, &repeat, own.then_some(current.as_str())) {
            self.members.form.busy = true;
            let call = UserCall::SetPassword { user_id: user, old: own.then(|| Password::new(current)), new: Password::new(new) };
            let _ = self.requests_handle().send(backend::Request::BoardMembers(call));
        }
        cx.notify();
    }

    /// Disable a board member, after the confirmation (MU3-05 AC3).
    pub fn members_disable(&mut self, user: UserId, cx: &mut Context<Self>) {
        let _ = self.requests_handle().send(backend::Request::BoardMembers(UserCall::Disable(user)));
        cx.notify();
    }

    /// Show (or hide) a board member's signed-in devices under their row (MU3-04 AC4).
    fn members_toggle_sessions(&mut self, user: UserId, cx: &mut Context<Self>) {
        if self.members.sessions_open == Some(user) {
            self.members.sessions_open = None;
        } else {
            self.members.sessions_open = Some(user);
            let key = (self.active_member().map(|u| u.id) != Some(user)).then_some(user);
            let _ = self.requests_handle().send(backend::Request::BoardMembers(UserCall::Sessions(key)));
        }
        cx.notify();
    }

    fn members_reveal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.members.form.revealed = !self.members.form.revealed;
        let masked = !self.members.form.revealed;
        for input in [self.members.password.clone(), self.members.repeat.clone(), self.members.current.clone()] {
            input.update(cx, |i, cx| i.set_masked(masked, window, cx));
        }
        cx.notify();
    }

    // ── Rendering ──────────────────────────────────────────────────────

    /// The board member screen in place of the page: a 420 px card in the middle (§11.1).
    pub fn render_members(&mut self, step: Step, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        if std::mem::take(&mut self.members.clear_fields) {
            self.members_clear(window, cx);
        }
        if std::mem::take(&mut self.members.clear_password) {
            self.members.password.update(cx, |i, cx| i.set_value("", window, cx));
        }
        let card = match step {
            Step::Create => self.render_member_create(cx),
            Step::SignIn => self.render_sign_in(cx),
            Step::ChangePassword => self.render_change_password(None, cx),
            Step::TempPassword(user) => self.render_change_password(Some(user), cx),
        };
        let host = self.remote_name().map_or("This computer".to_string(), str::to_owned);
        let forced = self.member_gate() != Gate::Open;
        v().id("board-member-screen")
            .size_full()
            .items_center()
            .justify_center()
            .gap(px(22.))
            .overflow_y_hidden()
            .child(h().gap(px(10.)).items_center().child(ui::logo(22., theme::ACCENT)).child(div().font_family(ui::serif(cx)).text_size(px(20.)).child("Aurelius")))
            .child(card)
            .when(forced, |el| {
                el.child(
                    h().gap(px(8.)).items_center().text_size(px(12.)).text_color(c(theme::MUTED))
                        .child(ui::dot(6., if matches!(self.connection, backend::Connection::Online { .. }) { theme::ACCENT } else { theme::MUTED }))
                        .child(format!("Connected to {host}"))
                        .child("·")
                        .child(
                            div().id("member-other-server").role(Role::Button).aria_label("Choose another server").cursor_pointer().underline().text_color(c(theme::TEXT_2))
                                .child("Choose another server")
                                .on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.on_open_servers(&OpenServers, w, cx))),
                        ),
                )
            })
            .into_any_element()
    }

    fn member_card(&self, title: impl Into<SharedString>, about: Option<String>, cx: &mut Context<Self>) -> Div {
        v().w(px(420.)).p(px(22.)).gap(px(14.)).rounded(px(10.)).border_1().border_color(c(theme::BORDER)).bg(c(theme::SURFACE))
            .child(div().font_family(ui::serif(cx)).text_size(px(19.)).child(title.into()))
            .when_some(about, |el, about| el.child(div().text_size(px(12.5)).line_height(px(18.)).text_color(c(theme::TEXT_2)).child(about)))
    }

    /// A field with its label, and its rule or problem under it.
    fn member_field(label: &'static str, input: AnyElement, rule: Option<(&'static str, bool)>, problem: Option<String>) -> Div {
        v().gap(px(5.))
            .child(div().text_size(px(12.)).text_color(c(theme::TEXT_2)).child(label))
            .child(input)
            .map(|el| match (problem, rule) {
                (Some(problem), _) => el.child(div().text_size(px(11.5)).text_color(c(theme::NEGATIVE)).child(problem)),
                (None, Some((rule, ok))) => el.child(
                    h().gap(px(6.)).text_size(px(11.5)).text_color(c(theme::MUTED)).child(div().flex_1().child(rule)).when(ok, |el| el.child(ui::check(10., theme::ACCENT_TEXT))),
                ),
                (None, None) => el,
            })
    }

    fn member_eye(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let revealed = self.members.form.revealed;
        div().id("member-reveal").role(Role::Button).aria_label(if revealed { "Hide password" } else { "Show password" }).cursor_pointer()
            .px(px(6.)).text_size(px(12.)).text_color(c(theme::TEXT_2)).hover(|s| s.text_color(c(theme::TEXT)))
            .child(if revealed { "Hide" } else { "Show" })
            .on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.members_reveal(w, cx)))
    }

    fn values(&self, cx: &mut Context<Self>) -> [String; 4] {
        [
            self.members.username.read(cx).value().to_string(),
            self.members.password.read(cx).value().to_string(),
            self.members.repeat.read(cx).value().to_string(),
            self.members.current.read(cx).value().to_string(),
        ]
    }

    fn render_member_create(&mut self, cx: &mut Context<Self>) -> Div {
        let first = self.member_gate() == Gate::FirstBoardMember;
        let values = self.values(cx);
        let values = [values[0].as_str(), values[1].as_str(), values[2].as_str(), values[3].as_str()];
        let form = self.members.form.clone();
        let (title, about) = if first {
            ("Create the first board member", "This server has no board members yet. Board members create tickets and decide in review. You are signed in on this device right after.")
        } else {
            ("New board member", "They can create tickets and decide in review. Share the password with them yourself; they sign in with it on their own devices.")
        };
        let general = form.failure.as_ref().filter(|f| f.field.is_none()).map(|f| f.message.clone());
        let back = !self.members.stack.is_empty();
        self.member_card(title, Some(about.into()), cx)
            .child(Self::member_field("Username", ui::input(&self.members.username).into_any_element(), Some(("3–32 characters: letters, digits, . _ -", username_problem(values[0]).is_none())), shown_problem(Field::Username, &form, values)))
            .child(Self::member_field(
                "Password",
                h().gap(px(4.)).child(div().flex_1().child(ui::input(&self.members.password))).child(self.member_eye(cx)).into_any_element(),
                Some(("At least 12 characters", password_problem(values[1]).is_none())),
                shown_problem(Field::Password, &form, values),
            ))
            .child(Self::member_field("Repeat password", ui::input(&self.members.repeat).into_any_element(), None, shown_problem(Field::Repeat, &form, values)))
            .when_some(general, |el, text| el.child(div().text_size(px(12.)).text_color(c(theme::NEGATIVE)).child(text)))
            .child(
                h().gap(px(8.)).justify_end()
                    .when(back, |el| el.child(ui::button("member-back", "Back", ButtonKind::Outline, Some(ui::kbd("escape", cx))).on_click(cx.listener(|this, _: &ClickEvent, w, cx| {
                        this.members_back(w, cx);
                    }))))
                    .child(
                        ui::button("member-create", if form.busy { "Creating…" } else { "Create board member" }, ButtonKind::Primary, Some(ui::kbd("secondary-enter", cx)))
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.members_create(cx))),
                    ),
            )
    }

    /// "Sign in" (ui-ontwerp §1): one generic refusal, a countdown after too many tries.
    fn render_sign_in(&mut self, cx: &mut Context<Self>) -> Div {
        let form = self.members.form.clone();
        let host = self.remote_name().map_or("this computer".to_string(), str::to_owned);
        let wait = countdown(form.retry_until_ms, ui::now_ms());
        let problem = sign_in_refusal(form.failure.as_ref(), wait.as_deref());
        let notice = self.members.signed_out.map(signed_out_notice);
        let label = match (&wait, form.busy) {
            (Some(wait), _) => wait.clone(),
            (None, true) => "Signing in…".to_string(),
            (None, false) => "Sign in".to_string(),
        };
        self.member_card("Sign in", Some(format!("to {host}")), cx)
            .when_some(notice, |el, notice| {
                el.child(
                    h().id("signed-out-notice").role(Role::Status).aria_label(notice).gap(px(8.)).px(px(10.)).py(px(8.)).rounded(px(7.)).bg(c(theme::SURFACE_2)).text_size(px(12.5))
                        .child(ui::diamond(7., theme::ACCENT_TEXT, true))
                        .child(div().flex_1().child(notice)),
                )
            })
            .child(Self::member_field("Username", ui::input(&self.members.username).into_any_element(), None, None))
            .child(Self::member_field("Password", h().gap(px(4.)).child(div().flex_1().child(ui::input(&self.members.password))).child(self.member_eye(cx)).into_any_element(), None, None))
            .when_some(problem, |el, text| el.child(h().id("sign-in-problem").role(Role::Alert).gap(px(6.)).text_size(px(12.)).text_color(c(theme::NEGATIVE)).child("✕").child(text)))
            .child(
                h().justify_end().child(
                    ui::button("member-sign-in", label, ButtonKind::Primary, Some(ui::kbd("enter", cx)))
                        .when(wait.is_some(), |el| el.opacity(0.55))
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.members_sign_in(cx))),
                ),
            )
            .child(div().text_size(px(12.)).text_color(c(theme::MUTED)).child("Forgot it? Ask another board member to reset your password."))
    }

    /// The own password (`None`; forced after a temporary one), or a temporary one for `user`.
    fn render_change_password(&mut self, user: Option<UserId>, cx: &mut Context<Self>) -> Div {
        let forced = self.member_gate() == Gate::ChangePassword;
        let values = self.values(cx);
        let values = [values[0].as_str(), values[1].as_str(), values[2].as_str(), values[3].as_str()];
        let form = self.members.form.clone();
        let me = self.active_member().map_or_else(|| "you".to_string(), |u| u.username.clone());
        let name = user.map(|id| self.members.list.iter().find(|u| u.id == id).map_or_else(|| "this board member".to_string(), |u| u.username.clone()));
        let (title, about) = match (&name, forced) {
            (Some(name), _) => (format!("Temporary password for {name}"), format!("{name} must change it at the next sign-in, and is signed out on every device now. Give it to them yourself.")),
            (None, true) => ("Choose a new password".to_string(), format!("{me}, you signed in with a temporary password. Choose your own to continue.")),
            (None, false) => ("Change password".to_string(), "Your other devices are signed out; this one stays signed in.".to_string()),
        };
        let general = form.failure.as_ref().filter(|f| f.field.is_none()).map(|f| f.message.clone());
        let busy = form.busy;
        self.member_card(title, Some(about), cx)
            .when(user.is_none(), |el| {
                el.child(Self::member_field(
                    if forced { "Temporary password" } else { "Current password" },
                    ui::input(&self.members.current).into_any_element(),
                    None,
                    shown_problem(Field::Current, &form, values),
                ))
            })
            .child(Self::member_field(
                "New password",
                h().gap(px(4.)).child(div().flex_1().child(ui::input(&self.members.password))).child(self.member_eye(cx)).into_any_element(),
                Some(("At least 12 characters", password_problem(values[1]).is_none())),
                shown_problem(Field::Password, &form, values),
            ))
            .child(Self::member_field("Repeat password", ui::input(&self.members.repeat).into_any_element(), None, shown_problem(Field::Repeat, &form, values)))
            .when_some(general, |el, text| el.child(div().text_size(px(12.)).text_color(c(theme::NEGATIVE)).child(text)))
            .child(
                h().gap(px(8.)).justify_end()
                    .when(forced, |el| el.child(ui::button("member-sign-out", "Sign out", ButtonKind::Outline, None).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.members_sign_out(cx)))))
                    .when(!forced, |el| el.child(ui::button("member-back", "Back", ButtonKind::Outline, Some(ui::kbd("escape", cx))).on_click(cx.listener(|this, _: &ClickEvent, w, cx| {
                        this.members_back(w, cx);
                    }))))
                    .child(
                        ui::button("member-save-password", if busy { "Saving…" } else if user.is_some() { "Set temporary password" } else { "Save password" }, ButtonKind::Primary, Some(ui::kbd("secondary-enter", cx)))
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.members_save_password(user, cx))),
                    ),
            )
    }

    /// Settings → Company: the board members, "you", their sessions, a temporary
    /// password and Disable (§11.4, MU3-04 AC4, MU3-05, P-34).
    pub fn render_members_section(&self, cx: &mut Context<Self>) -> Div {
        let active = self.active_member().map(|u| u.id);
        let list = self.members.list.clone();
        let small = |id: (&'static str, usize), label: &'static str| ui::button(id, label, ButtonKind::Outline, None).h(px(24.)).px(px(8.)).text_size(px(12.));
        v().gap(px(6.))
            .child(ui::eyebrow_with_info("board-members-about", format!("BOARD MEMBERS · {}", active_count(&list)), "The people who create tickets and decide in review. Each signs in with their own username and password. Any board member can see and end another's sessions, set a temporary password for them, or disable them."))
            .children(list.iter().enumerate().map(|(i, u)| {
                let id = u.id;
                let you = Some(id) == active;
                let disabled = u.disabled_at_ms.is_some();
                let disable = may_disable(&list, u);
                let open = self.members.sessions_open == Some(id);
                v().gap(px(4.))
                    .child(
                        h().gap(px(8.)).text_size(px(12.5)).items_center()
                            .child(ui::member_monogram(&u.username, 20., cx))
                            .child(div().child(u.username.clone()))
                            .when(you, |el| el.child(div().text_color(c(theme::MUTED)).child("you")))
                            .when(disabled, |el| el.child(div().text_color(c(theme::MUTED)).child("disabled")))
                            .child(div().flex_1())
                            .when(!disabled, |el| {
                                el.child(small(("member-sessions", i), if open { "Hide sessions" } else { "Sessions" }).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.members_toggle_sessions(id, cx))))
                                    .when(!you, |el| el.child(small(("member-temp-password", i), "Temporary password…").on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.members_temp_password(id, w, cx)))))
                                    .child(match disable {
                                        Ok(()) => small(("member-disable", i), "Disable…").on_click(cx.listener(move |this, _: &ClickEvent, w, cx| {
                                            this.overlay = Overlay::Confirm(ConfirmKind::DisableMember(id));
                                            this.overlay_focus.focus(w, cx);
                                            cx.notify();
                                        })),
                                        Err(why) => small(("member-disable", i), "Disable…").opacity(0.5).tooltip(ui::tip(why, None)),
                                    })
                            })
                    )
                    .when(open, |el| el.child(div().pl(px(28.)).child(self.render_sessions(if you { None } else { Some(id) }, ("member-session", i), cx))))
            }))
            .child(
                h().gap(px(8.)).pt(px(4.))
                    .child(ui::button("company-member-new", "New board member", ButtonKind::Outline, None).on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.members_new(w, cx)))),
            )
    }

    /// A board member's signed-in devices with Revoke (ui-ontwerp §2); this device
    /// is marked and cannot be revoked here (Sign out instead).
    pub fn render_sessions(&self, of: Option<UserId>, id: (&'static str, usize), cx: &mut Context<Self>) -> Div {
        let now = ui::now_ms();
        match self.members.sessions.get(&of) {
            None => v().child(div().text_size(px(12.)).text_color(c(theme::MUTED)).child("Loading…")),
            Some(Err(why)) => v().child(div().text_size(px(12.)).text_color(c(theme::NEGATIVE)).child(why.clone())),
            Some(Ok(list)) if list.sessions.is_empty() => v().child(div().text_size(px(12.)).text_color(c(theme::MUTED)).child("Not signed in anywhere.")),
            Some(Ok(list)) => v().gap(px(2.)).children(list.sessions.iter().enumerate().map(|(k, s)| {
                let session = s.id;
                let current = s.current;
                h().id((id.0, id.1 * 1000 + k)).role(Role::ListItem).aria_label(session_label(s, now)).gap(px(10.)).py(px(6.)).items_center()
                    .child(div().text_size(px(13.)).child(if current { "▣" } else { "▯" }))
                    .child(
                        v().flex_1().min_w_0()
                            .child(div().truncate().text_size(px(12.5)).child(s.device_name.clone()))
                            .child(div().text_size(px(11.5)).text_color(c(theme::MUTED)).child(session_detail(s, now))),
                    )
                    .child(if current {
                        div().text_size(px(11.5)).text_color(c(theme::TEXT_2)).child("This device").into_any_element()
                    } else {
                        ui::button((id.0, id.1 * 1000 + k + 500), "Revoke", ButtonKind::Outline, None).h(px(24.)).px(px(8.)).text_size(px(12.))
                            .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| {
                                this.overlay = Overlay::Confirm(ConfirmKind::RevokeSession(session, of));
                                this.overlay_focus.focus(w, cx);
                                cx.notify();
                            }))
                            .into_any_element()
                    })
            })),
        }
    }

    /// The device and its user of a session to revoke ("iPhone · Robin's iPhone").
    pub fn session_name(&self, session: aurelius_domain::SessionId, of: Option<UserId>) -> String {
        self.members.sessions.get(&of).and_then(|r| r.as_ref().ok()).and_then(|l| l.sessions.iter().find(|s| s.id == session)).map_or_else(|| "this device".to_string(), |s| s.device_name.clone())
    }

    /// A board member's name for a dialog.
    pub fn member_name(&self, user: UserId) -> String {
        self.members.list.iter().find(|u| u.id == user).map_or_else(|| "this board member".to_string(), |u| u.username.clone())
    }

    /// The modal when the server ended this device's session (MU3-04 AC2, ui-ontwerp §1).
    pub fn render_signed_out(&mut self, cx: &mut Context<Self>) -> Stateful<Div> {
        let reason = self.members.signed_out.map_or("You were signed out on this device.", signed_out_notice);
        v().id("signed-out-dialog").role(Role::AlertDialog).aria_label(reason).w(px(420.)).gap(px(12.)).p(px(22.)).rounded(px(10.)).occlude().shadow_lg().border_1().border_color(c(theme::BORDER_STRONG)).bg(c(theme::PANEL))
            .child(div().font_family(ui::serif(cx)).text_size(px(19.)).child(SIGNED_OUT_TITLE))
            .when(reason != SIGNED_OUT_TITLE, |el| el.child(div().text_size(px(13.)).text_color(c(theme::TEXT_2)).child(reason)))
            .child(div().text_size(px(12.5)).text_color(c(theme::TEXT_2)).child("Sign in again to go on. What you were typing stays on this device."))
            .child(h().justify_end().child(ui::button("signed-out-ok", "Sign in", ButtonKind::Primary, None).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.overlay = Overlay::None;
                cx.notify();
            }))))
    }

    /// Overlays wait while the gate holds: nothing but Settings → Servers.
    pub fn members_hold(&mut self) {
        if self.member_gate() == Gate::Open {
            return;
        }
        if !kept_at_the_gate(self.overlay) {
            self.close_overlay_now();
        }
        if self.screen == crate::app::Page::Settings {
            self.settings_tab = crate::screens::servers::SERVERS_TAB;
        }
    }
}

/// "Signed in Today 09:12 · active now" for a session row.
pub fn session_detail(s: &aurelius_domain::user::SessionView, now_ms: i64) -> String {
    let active = if s.current || now_ms - s.last_seen_ms < 60_000 { "active now".to_string() } else { format!("active {}", ui::relative_age(s.last_seen_ms, now_ms)) };
    format!("Signed in {} · {active}", ui::fmt_when(s.created_at_ms))
}

/// What a screen reader says of a session row.
pub fn session_label(s: &aurelius_domain::user::SessionView, now_ms: i64) -> String {
    format!("{}{}, {}", s.device_name, if s.current { ", this device" } else { "" }, session_detail(s, now_ms))
}

#[cfg(test)]
mod tests {
    // Not `super::*`: gpui's `test` attribute would shadow the standard one.
    use super::{SIGNED_OUT_TITLE, kept_at_the_gate, leaving, page_at_the_gate, shown_step, sign_in_refusal, swap_drafts, Drafts, Arrival, Field, Form, Gate, Step, active_count, arrival, countdown, create_ready, current_step, dropped_while_busy, gate, may_disable, password_problem, password_ready, repeat_problem, session_detail, shown_problem, sign_in_problem, signed_out_notice, username_problem};
    use crate::backend::{MemberFailure, MemberField};
    use aurelius_domain::user::{SessionView, SignOutReason, UserView, WhoAmI};
    use aurelius_domain::{SessionId, UserId};

    fn user(name: &str, by: Option<UserId>) -> UserView {
        UserView { id: UserId::new(), username: name.into(), created_at_ms: 0, created_by_user: by, disabled_at_ms: None }
    }

    fn who(exist: bool, user: Option<UserView>) -> WhoAmI {
        WhoAmI { board_members_exist: exist, user, must_change_password: false }
    }

    /// MU3-02 AC1, MU3-06: without a session only the sign-in; a temporary password first (B10).
    #[test]
    fn the_gate_follows_the_server() {
        assert_eq!(gate(None), Gate::Open, "nothing known yet: no flash of the onboarding");
        assert_eq!(gate(Some(&who(false, None))), Gate::FirstBoardMember);
        assert_eq!(gate(Some(&who(true, None))), Gate::SignIn);
        assert_eq!(gate(Some(&who(true, Some(user("alex", None))))), Gate::Open);
        let temporary = WhoAmI { must_change_password: true, ..who(true, Some(user("robin", None))) };
        assert_eq!(gate(Some(&temporary)), Gate::ChangePassword);
    }

    #[test]
    fn the_gate_shows_its_one_screen_whatever_was_opened() {
        // MU1-20 AC1 and MU3-02 AC1: whatever was opened, the gate's screen is all there is.
        assert_eq!(current_step(Gate::FirstBoardMember, &[]), Some(Step::Create));
        assert_eq!(current_step(Gate::FirstBoardMember, &[Step::ChangePassword]), Some(Step::Create));
        assert_eq!(current_step(Gate::SignIn, &[Step::Create]), Some(Step::SignIn), "no new board member without a session");
        assert_eq!(current_step(Gate::ChangePassword, &[]), Some(Step::ChangePassword));
        // Open: only what the user opened (New board member, Change password, a temporary one).
        let robin = UserId::new();
        assert_eq!(current_step(Gate::Open, &[]), None);
        assert_eq!(current_step(Gate::Open, &[Step::TempPassword(robin)]), Some(Step::TempPassword(robin)));
    }

    #[test]
    fn the_rules_are_ov_12() {
        assert!(username_problem("alex").is_none());
        assert!(username_problem("al").is_some());
        assert!(username_problem("élodie").is_some());
        assert!(password_problem("eleven-char").is_some());
        assert!(password_problem("twelve-chars").is_none());
        assert!(repeat_problem("twelve-chars", "twelve-chars").is_none());
        assert!(repeat_problem("twelve-chars", "twelve-charz").is_some());
        assert!(create_ready(["alex", "twelve-chars", "twelve-chars"]));
        assert!(!create_ready(["alex", "twelve-chars", ""]));
        assert!(!create_ready(["", "twelve-chars", "twelve-chars"]), "MU1-20 AC3: an empty name");
    }

    /// MU3-05 AC1 and B10: the own password needs the current one; a temporary one does not.
    #[test]
    fn a_new_password_needs_its_repeat_and_the_own_the_current_one() {
        assert!(password_ready("a-new-password", "a-new-password", Some("the-old-one")));
        assert!(!password_ready("a-new-password", "a-new-password", Some("")), "the own needs the current one");
        assert!(password_ready("a-new-password", "a-new-password", None), "a temporary one for another board member");
        assert!(!password_ready("short", "short", None));
        assert!(!password_ready("a-new-password", "another-password", None));
    }

    #[test]
    fn a_problem_shows_after_leaving_the_field_or_sending() {
        let values = ["al", "short", "other", ""];
        let mut form = Form::default();
        assert_eq!(shown_problem(Field::Username, &form, values), None, "not while typing the first time");
        form.left[0] = true;
        assert!(shown_problem(Field::Username, &form, values).is_some());
        assert_eq!(shown_problem(Field::Password, &form, values), None);
        form.sent = true;
        assert!(shown_problem(Field::Password, &form, values).is_some());
        assert_eq!(shown_problem(Field::Repeat, &form, values).as_deref(), Some("The passwords are not the same."));
        assert!(shown_problem(Field::Current, &form, values).is_some());
    }

    #[test]
    fn a_refusal_shows_under_its_field() {
        let values = ["alex", "twelve-chars", "twelve-chars", "wrong-old-one"];
        let refused = |field, text: &str| Form { failure: Some(MemberFailure { field: Some(field), message: text.into(), retry_after_ms: None }), ..Default::default() };
        let form = refused(MemberField::Username, "This username is taken.");
        assert_eq!(shown_problem(Field::Username, &form, values).as_deref(), Some("This username is taken."));
        assert_eq!(shown_problem(Field::Password, &form, values), None);
        // MU3-05 AC1: a wrong current password is its field's.
        let form = refused(MemberField::Current, "That password is wrong.");
        assert_eq!(shown_problem(Field::Current, &form, values).as_deref(), Some("That password is wrong."));
    }

    /// MU3-02 AC3/AC4: no rule about the fields, and a countdown after too many tries.
    #[test]
    fn the_sign_in_asks_both_fields_and_counts_down() {
        assert!(sign_in_problem("", "x").is_some());
        assert!(sign_in_problem("alex", "").is_some());
        assert_eq!(sign_in_problem("al", "short"), None, "no rule: it would tell what a username looks like");
        assert_eq!(countdown(Some(30_000), 600).as_deref(), Some("Try again in 0:30"));
        assert_eq!(countdown(Some(15 * 60_000), 0).as_deref(), Some("Try again in 15:00"));
        assert_eq!(countdown(Some(1_000), 1_000), None, "passed");
        assert_eq!(countdown(None, 0), None);
    }

    /// MU3-04 AC2, MU3-06 AC3: a signed-out device says so on its sign-in screen.
    #[test]
    fn a_signed_out_device_says_so() {
        assert_eq!(signed_out_notice(SignOutReason::Revoked), "You were signed out on this device.");
        assert!(signed_out_notice(SignOutReason::PasswordChanged).contains("password"));
        assert!(signed_out_notice(SignOutReason::Disabled).contains("disabled"));
        // ui-ontwerp §1: a modal first, which the gate keeps open over the sign-in; every reason starts with its title.
        for reason in [SignOutReason::LoggedOut, SignOutReason::Revoked, SignOutReason::Expired, SignOutReason::PasswordChanged, SignOutReason::Disabled] {
            assert!(signed_out_notice(reason).starts_with(SIGNED_OUT_TITLE.trim_end_matches('.')), "{reason:?}");
        }
        assert!(kept_at_the_gate(crate::app::Overlay::SignedOut));
        assert!(!kept_at_the_gate(crate::app::Overlay::ConnectClaude), "anything else closes at the gate");
    }

    #[test]
    fn arriving_says_welcome_once_and_nothing_on_a_reconnect() {
        let alex = user("alex", None);
        let robin = user("robin", Some(alex.id));
        // The onboarding: no board members before, one now.
        assert_eq!(arrival(Some(&who(false, None)), &who(true, Some(alex.clone()))), Arrival::Welcome("alex".into()));
        // A reconnect with the kept session (MU3-03 AC2): nothing to say.
        assert_eq!(arrival(Some(&who(true, Some(alex.clone()))), &who(true, Some(alex.clone()))), Arrival::Same);
        assert_eq!(arrival(None, &who(true, Some(alex.clone()))), Arrival::Same);
        // Signed in (again, or as someone else after a sign-out).
        assert_eq!(arrival(Some(&who(true, None)), &who(true, Some(robin.clone()))), Arrival::SignedIn("robin".into()));
        // The session is gone: the sign-in shows, no greeting.
        assert_eq!(arrival(Some(&who(true, Some(robin))), &who(true, None)), Arrival::Same);
    }

    /// MU3-05 AC4: never the last active board member; a disabled one has nothing to disable.
    #[test]
    fn the_last_active_board_member_cannot_be_disabled() {
        let alex = user("alex", None);
        let mut robin = user("robin", None);
        assert!(may_disable(&[alex.clone(), robin.clone()], &robin).is_ok());
        robin.disabled_at_ms = Some(1);
        let list = [alex.clone(), robin.clone()];
        assert_eq!(active_count(&list), 1);
        assert_eq!(may_disable(&list, &alex), Err(crate::backend::LAST_BOARD_MEMBER));
        assert!(may_disable(&list, &robin).is_err());
    }

    #[test]
    fn a_session_row_says_when_it_began_and_was_last_used() {
        let now = 1_000_000_000;
        let s = SessionView { id: SessionId::new(), user_id: UserId::new(), device_name: "Desktop · dev-laptop (Linux)".into(), client_id: None, created_at_ms: now - 3_600_000, last_seen_ms: now - 7_200_000, expires_at_ms: now, current: false };
        assert!(session_detail(&s, now).ends_with("active 2h ago"), "{}", session_detail(&s, now));
        let here = SessionView { current: true, ..s };
        assert!(session_detail(&here, now).ends_with("active now"));
    }

    /// Trace M3a m1: a form waiting on the server stops waiting when the connection
    /// goes, with a reason; an idle form or a live one stays.
    #[test]
    fn a_busy_form_stops_waiting_when_the_connection_drops() {
        use crate::backend::Connection;
        let busy = Form { busy: true, ..Form::default() };
        let online = Connection::Online { daemon_version: "1".into(), server: Default::default() };
        assert!(!dropped_while_busy(&busy, &online));
        assert!(dropped_while_busy(&busy, &Connection::Connecting));
        assert!(dropped_while_busy(&busy, &Connection::Offline("reset".into())));
        assert!(!dropped_while_busy(&Form::default(), &Connection::Connecting), "nothing was waiting");
        let lost = crate::backend::member_call_failure(&aurelius_client::ClientError::Disconnected);
        assert_eq!(lost, MemberFailure::general(crate::backend::MEMBER_CONNECTION_DROPPED));
    }

    /// QA3 D1: signed out (by the server, or Sign out) while Settings was open: the
    /// sign-in card is on top, not Settings › Servers; Settings opened at the gate stays.
    #[test]
    fn a_sign_out_from_settings_puts_the_sign_in_card_on_top() {
        use crate::app::Page;
        for open in [vec![], vec![Page::Board], vec![Page::Board, Page::Settings]] {
            let mut back = open.clone();
            let page = page_at_the_gate(Page::Settings, &mut back);
            assert_ne!(page, Page::Settings, "{open:?}");
            assert_eq!(shown_step(Gate::SignIn, &[], page), Some(Step::SignIn), "{open:?}");
        }
        let mut back = vec![Page::Dashboard];
        assert_eq!(page_at_the_gate(Page::Board, &mut back), Page::Board);
        assert_eq!(back, vec![Page::Dashboard], "another page is left as it is");
        // Settings chosen at the gate (another server): Servers, no card over it.
        assert_eq!(shown_step(Gate::SignIn, &[], Page::Settings), None);
        assert_eq!(shown_step(Gate::Open, &[Step::ChangePassword], Page::Settings), Some(Step::ChangePassword));
    }

    /// QA3 D-m1: the countdown's text until 0, then nothing (the button is active again).
    #[test]
    fn too_many_attempts_is_gone_when_the_countdown_ends() {
        let limited = MemberFailure { retry_after_ms: Some(44_000), ..MemberFailure::general("Too many attempts. Try again in 0:44.") };
        let until = Some(1_000 + 44_000);
        let wait = countdown(until, 1_000 + 30_000);
        assert_eq!(sign_in_refusal(Some(&limited), wait.as_deref()).as_deref(), Some("Too many attempts. Try again in 0:14."));
        let wait = countdown(until, 1_000 + 44_000);
        assert_eq!(wait, None, "the button is Sign in again");
        assert_eq!(sign_in_refusal(Some(&limited), wait.as_deref()), None, "not the server's frozen text");
        let wrong = MemberFailure::general("Username or password is wrong.");
        assert_eq!(sign_in_refusal(Some(&wrong), None).as_deref(), Some("Username or password is wrong."));
        assert_eq!(sign_in_refusal(None, None), None);
    }

    /// R3 m1: a draft belongs to its board member: gone for another, back for the same.
    #[test]
    fn a_draft_is_kept_for_its_board_member_only() {
        let (alex, robin) = (UserId::new(), UserId::new());
        let mut kept = std::collections::HashMap::new();
        let typed = Drafts { chat: "Half a message from Alex".into(), discussion: "A reply".into(), ..Drafts::default() };
        // Alex is signed out: the sign-in shows, the draft waits for Alex.
        let shown = swap_drafts(&mut kept, Some(alex), None, typed.clone());
        assert_eq!(shown, Drafts::default());
        // Robin signs in on the same window: nothing of Alex's.
        let shown = swap_drafts(&mut kept, None, Some(robin), shown);
        assert_eq!(shown, Drafts::default(), "Robin never sees Alex's draft");
        let robins = Drafts { feedback: "Robin's feedback".into(), ..Drafts::default() };
        // Robin signs out, Alex is back: Alex's draft, Robin's kept apart.
        let shown = swap_drafts(&mut kept, Some(robin), None, robins.clone());
        let shown = swap_drafts(&mut kept, None, Some(alex), shown);
        assert_eq!(shown, typed);
        let shown = swap_drafts(&mut kept, Some(alex), Some(robin), Drafts::default());
        assert_eq!(shown, robins);
        assert!(kept.is_empty(), "an emptied draft is not kept");
    }

    /// AUR-CHAT-001 R2 D-M2: Alex typed in #3 and went to #general; Robin signs in on this
    /// window and opens #3: empty. Alex comes back and finds the draft in #3 again.
    #[test]
    fn another_board_member_never_gets_a_channel_draft_of_the_one_before() {
        use crate::screens::discussion::{DraftPlace, Drafts as Channels};
        let (alex, robin) = (UserId::new(), UserId::new());
        let (general, three) = (DraftPlace::Channel(None), DraftPlace::Channel(Some(aurelius_domain::ConversationId::new())));
        let mut kept = std::collections::HashMap::new();
        // The window's channel drafts, as AppView holds them.
        let mut on_screen = Channels::default();
        on_screen.moved(three, "");
        assert_eq!(on_screen.moved(general, "alex: half a thought for #3"), Some(String::new()));
        // Alex signs out: the sign-in shows, the drafts wait for Alex.
        let shown = swap_drafts(&mut kept, Some(alex), None, leaving(Drafts::default(), &mut on_screen));
        assert!(shown.is_empty());
        // Robin signs in and opens #3.
        let mut shown = swap_drafts(&mut kept, None, Some(robin), leaving(shown, &mut on_screen));
        on_screen = std::mem::take(&mut shown.channels);
        on_screen.moved(general, &shown.chat);
        assert_eq!(on_screen.moved(three, "").as_deref(), Some(""), "Robin starts empty in #3");
        // Robin signs out without typing, Alex is back: the #3 draft is Alex's again.
        let shown = swap_drafts(&mut kept, Some(robin), None, leaving(Drafts::default(), &mut on_screen));
        let mut shown = swap_drafts(&mut kept, None, Some(alex), leaving(shown, &mut on_screen));
        assert_eq!(shown.chat, "", "Alex was in #general");
        on_screen = std::mem::take(&mut shown.channels);
        assert_eq!(on_screen.moved(three, &shown.chat).as_deref(), Some("alex: half a thought for #3"), "Alex finds the #3 draft again");
        assert!(kept.is_empty(), "Robin left nothing, Alex took theirs back");
    }
}
