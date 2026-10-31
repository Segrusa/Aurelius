//! Board members in this window (Fase 1: identification, not a login; ui-ontwerp §11).
//!
//! * A server without board members shows only "Create the first board member"
//!   (MU1-20): no sidebar, no page, Settings → Servers stays reachable.
//! * A server with board members and none active here shows "Who's working
//!   here?" (MU1-21); the chosen one's password is checked once on this device
//!   (B8, `user.use`). The window remembers the choice per server in ui.json and
//!   says it in every hello, so a reconnect keeps it; an unknown one is forgotten
//!   and the choice shows again.
//! * A board member makes another one, or switches (MU1-22, §11.2).
//!
//! A password lives only in its field until the server answered: never in
//! ui.json, a log, a toast or a notice.

use crate::app::{AppView, Overlay};
use crate::backend::{self, MemberFailure, MemberField, Password, UserCall};
use crate::keymap::*;
use crate::theme::{self, c};
use crate::ui::{self, ButtonKind, h, v};
use aurelius_domain::UserId;
use aurelius_domain::user::{UserView, WhoAmI};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

/// What the window may show, from the server's answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gate {
    /// A board member works here (or the server did not say yet).
    Open,
    /// The server has no board members: only the onboarding (MU1-20 AC1).
    FirstBoardMember,
    /// Board members exist, none works here: choose first (MU1-21 AC2).
    Choose,
}

pub fn gate(who: Option<&WhoAmI>) -> Gate {
    match who {
        Some(w) if !w.board_members_exist => Gate::FirstBoardMember,
        Some(w) if w.user.is_none() => Gate::Choose,
        _ => Gate::Open,
    }
}

/// A board member screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Username, password, repeat (the onboarding, or a new board member).
    Create,
    /// "Who's working here?"
    Choose,
    /// The once-per-device password check of the chosen board member.
    Password(UserId),
}

/// The screen on top: what the user opened (Back pops it), else what the gate
/// demands. Nothing but the onboarding while the server has no board members.
pub fn current_step(gate: Gate, stack: &[Step]) -> Option<Step> {
    match gate {
        Gate::FirstBoardMember => Some(Step::Create),
        Gate::Choose => stack.last().copied().or(Some(Step::Choose)),
        Gate::Open => stack.last().copied(),
    }
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

/// The fields of the create form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Username,
    Password,
    Repeat,
}

/// A problem shows once the field was left or the form was sent (§11.1); a
/// refusal of the server shows under its field until that field changes.
pub fn shown_problem(field: Field, form: &Form, values: [&str; 3]) -> Option<String> {
    let server = form.failure.as_ref().filter(|f| match f.field {
        Some(MemberField::Username) => field == Field::Username,
        Some(MemberField::Password) => field == Field::Password,
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

/// What the forms keep besides their fields.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Form {
    /// Username, password, repeat: left at least once.
    pub left: [bool; 3],
    pub sent: bool,
    /// Waiting for the server.
    pub busy: bool,
    pub failure: Option<MemberFailure>,
    /// The eye: the password shown in this window.
    pub revealed: bool,
}

/// ui.json `board_members`: the board member this device works as, per server.
const KEY: &str = "board_members";

pub fn remembered_load(prefs: &serde_json::Value, server: &str) -> Option<UserId> {
    prefs[KEY][server].as_str().and_then(|id| UserId::parse(id).ok())
}

pub fn remembered_save(prefs: &mut serde_json::Value, server: &str, user: Option<UserId>) {
    if !prefs[KEY].is_object() {
        prefs[KEY] = serde_json::json!({});
    }
    match user {
        Some(id) => prefs[KEY][server] = serde_json::json!(id.to_string()),
        None => {
            if let Some(map) = prefs[KEY].as_object_mut() {
                map.remove(server);
            }
        }
    }
}

/// What a new answer of the server means for the window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Arrival {
    /// The same board member as before (a reconnect), or still none.
    Same,
    /// The onboarding is done: "Welcome, ada."
    Welcome(String),
    /// Chosen or switched: "Working as marie."
    Working(String),
}

/// `before`: none at the first hello of a backend, which greets nobody (the
/// remembered board member is simply back).
pub fn arrival(before: Option<&WhoAmI>, now: &WhoAmI) -> Arrival {
    let Some(before) = before else { return Arrival::Same };
    match &now.user {
        Some(u) if before.user.as_ref().map(|b| b.id) != Some(u.id) => {
            if before.board_members_exist {
                Arrival::Working(u.username.clone())
            } else {
                Arrival::Welcome(u.username.clone())
            }
        }
        _ => Arrival::Same,
    }
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
    /// Empty the fields on the next frame (an update has no window).
    pub clear_fields: bool,
    pub clear_password: bool,
}

impl Members {
    pub fn new(window: &mut Window, cx: &mut Context<AppView>) -> Self {
        Self {
            who: None,
            list: Vec::new(),
            stack: Vec::new(),
            form: Form::default(),
            username: cx.new(|cx| InputState::new(window, cx).placeholder("ada")),
            password: cx.new(|cx| InputState::new(window, cx).masked(true).placeholder("At least 12 characters")),
            repeat: cx.new(|cx| InputState::new(window, cx).masked(true).placeholder("The same password again")),
            clear_fields: false,
            clear_password: false,
        }
    }

    /// The fields' events: Enter goes to the next field and sends on the last,
    /// Ctrl+Enter sends from any; leaving a field shows its rule.
    pub fn subscriptions(&self, window: &mut Window, cx: &mut Context<AppView>) -> Vec<Subscription> {
        let mut out = Vec::new();
        for field in [Field::Username, Field::Password, Field::Repeat] {
            let input = match field {
                Field::Username => &self.username,
                Field::Password => &self.password,
                Field::Repeat => &self.repeat,
            };
            out.push(cx.subscribe_in(input, window, move |this: &mut AppView, _, event: &InputEvent, w, cx| match event {
                InputEvent::PressEnter { secondary, .. } => this.members_enter(field, *secondary, w, cx),
                InputEvent::Blur => {
                    this.members.form.left[field as usize] = true;
                    cx.notify();
                }
                InputEvent::Change => {
                    // A refusal of the server holds until its field changes.
                    let about = this.members.form.failure.as_ref().and_then(|f| f.field);
                    let changed = match field {
                        Field::Username => about == Some(MemberField::Username),
                        Field::Password => about == Some(MemberField::Password),
                        Field::Repeat => false,
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
    /// The key of the server in use in ui.json (the same as the pins').
    pub fn server_key(&self) -> String {
        self.remote_host.as_ref().map_or_else(|| "local".to_string(), |(id, _)| id.to_string())
    }

    pub fn remembered_member(&self) -> Option<UserId> {
        remembered_load(&crate::app::ui_prefs(), &self.server_key())
    }

    pub fn member_gate(&self) -> Gate {
        gate(self.members.who.as_ref())
    }

    /// The board member screen to show instead of the page, if any.
    pub fn member_step(&self) -> Option<Step> {
        current_step(self.member_gate(), &self.members.stack)
    }

    /// The active board member's name ("as ada").
    pub fn active_member(&self) -> Option<&UserView> {
        self.members.who.as_ref().and_then(|w| w.user.as_ref())
    }

    /// A new backend: nothing of the previous server's board members is kept.
    pub fn members_reset(&mut self, window: Option<&mut Window>, cx: &mut Context<Self>) {
        self.members.who = None;
        self.members.list.clear();
        self.members.stack.clear();
        if let Some(window) = window {
            self.members_clear(window, cx);
        } else {
            self.members.form = Form::default();
            self.members.clear_fields = true;
        }
    }

    pub fn apply_who(&mut self, who: WhoAmI, cx: &mut Context<Self>) {
        let mut prefs = crate::app::ui_prefs();
        let key = self.server_key();
        let now = who.user.as_ref().map(|u| u.id);
        if remembered_load(&prefs, &key) != now {
            // Remember the one chosen; forget one the server no longer has.
            remembered_save(&mut prefs, &key, now);
            crate::app::set_ui_value(KEY, prefs[KEY].clone());
        }
        let arrived = arrival(self.members.who.as_ref(), &who);
        self.members.who = Some(who);
        match arrived {
            Arrival::Same => {}
            Arrival::Welcome(name) => {
                self.members_done(cx);
                self.notify_user(format!("Welcome, {name}."), false, cx);
            }
            Arrival::Working(name) => {
                self.members_done(cx);
                self.notify_user(format!("Working as {name}."), false, cx);
            }
        }
        cx.notify();
    }

    /// The connection went: a form still waiting for `user.create` / `user.use`
    /// stops waiting and says so (Trace M3a m1), whatever the backend still sends.
    pub fn members_connection(&mut self, connection: &backend::Connection, cx: &mut Context<Self>) {
        if dropped_while_busy(&self.members.form, connection) {
            self.apply_member_failure(MemberFailure { field: None, message: backend::MEMBER_CONNECTION_DROPPED.into() }, cx);
        }
    }

    pub fn apply_member_failure(&mut self, failure: MemberFailure, cx: &mut Context<Self>) {
        self.members.form.busy = false;
        self.members.form.failure = Some(failure);
        // A refused password check starts from an empty field.
        if matches!(self.member_step(), Some(Step::Password(_))) {
            self.members.clear_password = true;
        }
        cx.notify();
    }

    pub fn apply_member_created(&mut self, name: String, cx: &mut Context<Self>) {
        self.members_done(cx);
        self.notify_user(format!("{name} is a board member now."), false, cx);
    }

    /// A write needs a board member: say so and open the choice (S5 gate).
    pub fn apply_member_required(&mut self, cx: &mut Context<Self>) {
        if self.members.stack.last() != Some(&Step::Choose) && self.member_gate() == Gate::Open {
            self.members.stack.push(Step::Choose);
            self.members.form = Form::default();
        }
        let _ = self.requests_handle().send(backend::Request::BoardMembers(UserCall::List));
        self.notify_user(format!("{} Choose your board member below.", backend::BOARD_MEMBER_REQUIRED), true, cx);
    }

    /// The flow ended well: back to the app, fields emptied on the next frame.
    fn members_done(&mut self, cx: &mut Context<Self>) {
        self.members.stack.clear();
        self.members.form = Form::default();
        self.members.clear_fields = true;
        cx.notify();
    }

    fn members_clear(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.members.form = Form::default();
        for input in [self.members.username.clone(), self.members.password.clone(), self.members.repeat.clone()] {
            input.update(cx, |i, cx| {
                i.set_value("", window, cx);
                i.set_masked(true, window, cx);
            });
        }
    }

    fn members_open(&mut self, step: Step, window: &mut Window, cx: &mut Context<Self>) {
        self.members_clear(window, cx);
        self.members.stack.push(step);
        self.close_overlay_now();
        match step {
            Step::Create => self.members.username.update(cx, |i, cx| i.focus(window, cx)),
            Step::Password(_) => self.members.password.update(cx, |i, cx| i.focus(window, cx)),
            Step::Choose => {
                let _ = self.requests_handle().send(backend::Request::BoardMembers(UserCall::List));
            }
        }
        cx.notify();
    }

    /// Switch board member… (sidebar, Settings → Company).
    pub fn members_switch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.members.stack.clear();
        self.members_open(Step::Choose, window, cx);
    }

    /// New board member (Settings → Company, or the choice).
    pub fn members_new(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.members_open(Step::Create, window, cx);
    }

    /// Back from a board member screen; false when there is none to go back from.
    pub fn members_back(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.members.stack.pop().is_none() {
            return false;
        }
        self.members_clear(window, cx);
        cx.notify();
        true
    }

    fn members_enter(&mut self, field: Field, secondary: bool, window: &mut Window, cx: &mut Context<Self>) {
        match self.member_step() {
            Some(Step::Create) if !secondary && field == Field::Username => self.members.password.update(cx, |i, cx| i.focus(window, cx)),
            Some(Step::Create) if !secondary && field == Field::Password => self.members.repeat.update(cx, |i, cx| i.focus(window, cx)),
            Some(Step::Create) => self.members_create(cx),
            Some(Step::Password(user_id)) => self.members_use(user_id, cx),
            _ => {}
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

    fn members_use(&mut self, user_id: UserId, cx: &mut Context<Self>) {
        if self.members.form.busy {
            return;
        }
        let password = self.members.password.read(cx).value().to_string();
        self.members.form.failure = None;
        if password.is_empty() {
            self.members.form.failure = Some(MemberFailure { field: Some(MemberField::Password), message: "Type the password of this board member.".into() });
        } else {
            self.members.form.busy = true;
            let _ = self.requests_handle().send(backend::Request::BoardMembers(UserCall::Use { user_id, password: Password::new(password) }));
        }
        cx.notify();
    }

    fn members_reveal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.members.form.revealed = !self.members.form.revealed;
        let masked = !self.members.form.revealed;
        for input in [self.members.password.clone(), self.members.repeat.clone()] {
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
            Step::Choose => self.render_member_choose(cx),
            Step::Password(user_id) => self.render_member_password(user_id, cx),
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

    fn member_card(&self, title: &'static str, about: Option<&'static str>, cx: &mut Context<Self>) -> Div {
        v().w(px(420.)).p(px(22.)).gap(px(14.)).rounded(px(10.)).border_1().border_color(c(theme::BORDER)).bg(c(theme::SURFACE))
            .child(div().font_family(ui::serif(cx)).text_size(px(19.)).child(title))
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

    fn render_member_create(&mut self, cx: &mut Context<Self>) -> Div {
        let first = self.member_gate() == Gate::FirstBoardMember;
        let username = self.members.username.read(cx).value().to_string();
        let password = self.members.password.read(cx).value().to_string();
        let repeat = self.members.repeat.read(cx).value().to_string();
        let values = [username.as_str(), password.as_str(), repeat.as_str()];
        let form = self.members.form.clone();
        let revealed = form.revealed;
        let (title, about) = if first {
            ("Create the first board member", Some("This server has no board members yet. Board members create tickets and decide in review."))
        } else {
            ("New board member", Some("They can create tickets and decide in review. Share the password with them yourself; deleting or resetting comes later."))
        };
        let eye = div().id("member-reveal").role(Role::Button).aria_label(if revealed { "Hide password" } else { "Show password" }).cursor_pointer()
            .px(px(6.)).text_size(px(12.)).text_color(c(theme::TEXT_2)).hover(|s| s.text_color(c(theme::TEXT)))
            .child(if revealed { "Hide" } else { "Show" })
            .on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.members_reveal(w, cx)));
        let general = form.failure.as_ref().filter(|f| f.field.is_none()).map(|f| f.message.clone());
        let back = !self.members.stack.is_empty();
        self.member_card(title, about, cx)
            .child(Self::member_field("Username", ui::input(&self.members.username).into_any_element(), Some(("3–32 characters: letters, digits, . _ -", username_problem(&username).is_none())), shown_problem(Field::Username, &form, values)))
            .child(Self::member_field(
                "Password",
                h().gap(px(4.)).child(div().flex_1().child(ui::input(&self.members.password))).child(eye).into_any_element(),
                Some(("At least 12 characters", password_problem(&password).is_none())),
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

    fn render_member_choose(&mut self, cx: &mut Context<Self>) -> Div {
        let active = self.active_member().map(|u| u.id);
        let back = !self.members.stack.is_empty() && self.member_gate() == Gate::Open;
        let rows: Vec<_> = self.members.list.iter().enumerate().map(|(i, u)| {
            let id = u.id;
            let you = Some(id) == active;
            h().id(("member-row", i)).role(Role::Button).aria_label(format!("Work as {}", u.username)).px(px(10.)).py(px(8.)).gap(px(10.)).rounded(px(7.)).cursor_pointer()
                .hover(|s| s.bg(c(theme::SURFACE_2)))
                .child(ui::monogram(&u.username, 26., cx))
                .child(div().flex_1().truncate().child(u.username.clone()))
                .when(you, |el| el.child(div().text_size(px(11.5)).text_color(c(theme::MUTED)).child("you")))
                .child(ui::chevron(false, 11., theme::MUTED))
                .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| {
                    if you {
                        this.members.stack.clear();
                        cx.notify();
                    } else {
                        this.members_open(Step::Password(id), w, cx);
                    }
                }))
        }).collect();
        let loading = self.members.list.is_empty();
        self.member_card("Who's working here?", Some("Choose your board member on this server."), cx)
            .child(v().gap(px(2.)).when(loading, |el| el.child(div().text_color(c(theme::MUTED)).child("Loading…"))).children(rows))
            .child(ui::hr())
            .child(
                h().id("member-new").role(Role::Button).aria_label("New board member").px(px(10.)).py(px(8.)).gap(px(10.)).rounded(px(7.)).cursor_pointer()
                    .hover(|s| s.bg(c(theme::SURFACE_2))).text_color(c(theme::TEXT_2))
                    .child("+ New board member")
                    .on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.members_new(w, cx))),
            )
            .when(back, |el| {
                el.child(h().justify_end().child(ui::button("member-cancel", "Cancel", ButtonKind::Outline, Some(ui::kbd("escape", cx))).on_click(cx.listener(|this, _: &ClickEvent, w, cx| {
                    this.members_back(w, cx);
                }))))
            })
    }

    fn render_member_password(&mut self, user_id: UserId, cx: &mut Context<Self>) -> Div {
        let name = self.members.list.iter().find(|u| u.id == user_id).map_or_else(|| "this board member".to_string(), |u| u.username.clone());
        let form = self.members.form.clone();
        let problem = form.failure.as_ref().map(|f| f.message.clone());
        let revealed = form.revealed;
        let eye = div().id("member-reveal").role(Role::Button).aria_label(if revealed { "Hide password" } else { "Show password" }).cursor_pointer()
            .px(px(6.)).text_size(px(12.)).text_color(c(theme::TEXT_2)).hover(|s| s.text_color(c(theme::TEXT)))
            .child(if revealed { "Hide" } else { "Show" })
            .on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.members_reveal(w, cx)));
        v().w(px(420.)).p(px(22.)).gap(px(14.)).rounded(px(10.)).border_1().border_color(c(theme::BORDER)).bg(c(theme::SURFACE))
            .child(h().gap(px(10.)).items_center().child(ui::monogram(&name, 30., cx)).child(div().font_family(ui::serif(cx)).text_size(px(19.)).child(name.clone())))
            .child(Self::member_field("Password", h().gap(px(4.)).child(div().flex_1().child(ui::input(&self.members.password))).child(eye).into_any_element(), None, problem))
            .child(div().text_size(px(12.)).text_color(c(theme::MUTED)).child("This device remembers you. Full sign-in comes later."))
            .child(
                h().gap(px(8.)).justify_end()
                    .child(ui::button("member-back", "Back", ButtonKind::Outline, Some(ui::kbd("escape", cx))).on_click(cx.listener(|this, _: &ClickEvent, w, cx| {
                        this.members_back(w, cx);
                    })))
                    .child(
                        ui::button("member-continue", if form.busy { "Checking…" } else { "Continue" }, ButtonKind::Primary, Some(ui::kbd("enter", cx)))
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.members_use(user_id, cx))),
                    ),
            )
    }

    /// Settings → Company: the board members, "you", New and Switch (§11.4).
    pub fn render_members_section(&self, cx: &mut Context<Self>) -> Div {
        let active = self.active_member().map(|u| u.id);
        v().gap(px(6.))
            .child(ui::eyebrow_with_info("board-members-about", format!("BOARD MEMBERS · {}", self.members.list.len()), "The people who create tickets and decide in review. This device works as the one marked you."))
            .children(self.members.list.iter().map(|u| {
                h().gap(px(8.)).text_size(px(12.5))
                    .child(ui::monogram(&u.username, 20., cx))
                    .child(div().child(u.username.clone()))
                    .when(Some(u.id) == active, |el| el.child(div().text_color(c(theme::MUTED)).child("you")))
            }))
            .child(
                h().gap(px(8.)).pt(px(4.))
                    .child(ui::button("company-member-new", "New board member", ButtonKind::Outline, None).on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.members_new(w, cx))))
                    .child(ui::button("company-member-switch", "Switch board member…", ButtonKind::Outline, None).on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.members_switch(w, cx)))),
            )
    }

    /// Overlays wait while the gate holds: nothing but Settings → Servers.
    pub fn members_hold(&mut self) {
        if self.member_gate() == Gate::Open {
            return;
        }
        if self.overlay != Overlay::None {
            self.close_overlay_now();
        }
        if self.screen == crate::app::Page::Settings {
            self.settings_tab = crate::screens::servers::SERVERS_TAB;
        }
    }
}

#[cfg(test)]
mod tests {
    // Not `super::*`: gpui's `test` attribute would shadow the standard one.
    use super::{Arrival, Field, Form, Gate, Step, arrival, create_ready, current_step, dropped_while_busy, gate, password_problem, remembered_load, remembered_save, repeat_problem, shown_problem, username_problem};
    use crate::backend::{MemberFailure, MemberField};
    use aurelius_domain::UserId;
    use aurelius_domain::user::{UserView, WhoAmI};

    fn user(name: &str, by: Option<UserId>) -> UserView {
        UserView { id: UserId::new(), username: name.into(), created_at_ms: 0, created_by_user: by }
    }

    fn who(exist: bool, user: Option<UserView>) -> WhoAmI {
        WhoAmI { board_members_exist: exist, user }
    }

    #[test]
    fn the_gate_follows_the_server() {
        assert_eq!(gate(None), Gate::Open, "nothing known yet: no flash of the onboarding");
        assert_eq!(gate(Some(&who(false, None))), Gate::FirstBoardMember);
        assert_eq!(gate(Some(&who(true, None))), Gate::Choose);
        assert_eq!(gate(Some(&who(true, Some(user("ada", None))))), Gate::Open);
    }

    #[test]
    fn without_board_members_only_the_onboarding_shows() {
        // MU1-20 AC1: whatever was opened, the create screen is all there is.
        assert_eq!(current_step(Gate::FirstBoardMember, &[]), Some(Step::Create));
        assert_eq!(current_step(Gate::FirstBoardMember, &[Step::Choose]), Some(Step::Create));
    }

    #[test]
    fn without_an_active_board_member_the_choice_comes_first() {
        let id = UserId::new();
        assert_eq!(current_step(Gate::Choose, &[]), Some(Step::Choose));
        assert_eq!(current_step(Gate::Choose, &[Step::Password(id)]), Some(Step::Password(id)));
        assert_eq!(current_step(Gate::Choose, &[Step::Create]), Some(Step::Create), "+ New board member from the choice");
        // Open: only what the user opened (Switch, New board member).
        assert_eq!(current_step(Gate::Open, &[]), None);
        assert_eq!(current_step(Gate::Open, &[Step::Choose, Step::Password(id)]), Some(Step::Password(id)));
    }

    #[test]
    fn the_rules_are_ov_12() {
        assert!(username_problem("ada").is_none());
        assert!(username_problem("se").is_some());
        assert!(username_problem("élodie").is_some());
        assert!(password_problem("eleven-char").is_some());
        assert!(password_problem("twelve-chars").is_none());
        assert!(repeat_problem("twelve-chars", "twelve-chars").is_none());
        assert!(repeat_problem("twelve-chars", "twelve-charz").is_some());
        assert!(create_ready(["ada", "twelve-chars", "twelve-chars"]));
        assert!(!create_ready(["ada", "twelve-chars", ""]));
        assert!(!create_ready(["", "twelve-chars", "twelve-chars"]), "MU1-20 AC3: an empty name");
    }

    #[test]
    fn a_problem_shows_after_leaving_the_field_or_sending() {
        let values = ["se", "short", "other"];
        let mut form = Form::default();
        assert_eq!(shown_problem(Field::Username, &form, values), None, "not while typing the first time");
        form.left[0] = true;
        assert!(shown_problem(Field::Username, &form, values).is_some());
        assert_eq!(shown_problem(Field::Password, &form, values), None);
        form.sent = true;
        assert!(shown_problem(Field::Password, &form, values).is_some());
        assert_eq!(shown_problem(Field::Repeat, &form, values).as_deref(), Some("The passwords are not the same."));
    }

    #[test]
    fn a_refusal_shows_under_its_field() {
        let values = ["ada", "twelve-chars", "twelve-chars"];
        let form = Form { failure: Some(MemberFailure { field: Some(MemberField::Username), message: "This username is taken.".into() }), ..Default::default() };
        assert_eq!(shown_problem(Field::Username, &form, values).as_deref(), Some("This username is taken."));
        assert_eq!(shown_problem(Field::Password, &form, values), None);
    }

    #[test]
    fn the_choice_is_remembered_per_server_and_forgotten_when_unknown() {
        let mut prefs = serde_json::json!({ "appearance": "dark" });
        let one = UserId::new();
        assert_eq!(remembered_load(&prefs, "local"), None);
        remembered_save(&mut prefs, "local", Some(one));
        assert_eq!(remembered_load(&prefs, "local"), Some(one));
        assert_eq!(remembered_load(&prefs, "other-host"), None, "per server");
        remembered_save(&mut prefs, "local", None);
        assert_eq!(remembered_load(&prefs, "local"), None);
        assert_eq!(prefs["appearance"], "dark", "the other preferences stay");
        assert!(!prefs.to_string().contains("password"), "only an id is kept");
    }

    #[test]
    fn arriving_says_welcome_once_and_nothing_on_a_reconnect() {
        let ada = user("ada", None);
        let marie = user("marie", Some(ada.id));
        // The onboarding: no board members before, one now.
        assert_eq!(arrival(Some(&who(false, None)), &who(true, Some(ada.clone()))), Arrival::Welcome("ada".into()));
        // A reconnect with the remembered one (Trace m9): nothing to say.
        assert_eq!(arrival(Some(&who(true, Some(ada.clone()))), &who(true, Some(ada.clone()))), Arrival::Same);
        // The first hello of a window that remembers ada: they are simply back.
        assert_eq!(arrival(None, &who(true, Some(ada.clone()))), Arrival::Same);
        // Chosen or switched.
        assert_eq!(arrival(Some(&who(true, None)), &who(true, Some(marie.clone()))), Arrival::Working("marie".into()));
        assert_eq!(arrival(Some(&who(true, Some(ada))), &who(true, Some(marie))), Arrival::Working("marie".into()));
        // The remembered one is gone: the choice shows, no greeting.
        assert_eq!(arrival(None, &who(true, None)), Arrival::Same);
    }

    /// Trace M3a m1: a form waiting on `user.create` / `user.use` stops waiting
    /// when the connection goes, with a reason; an idle form or a live one stays.
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
        assert_eq!(lost, MemberFailure { field: None, message: crate::backend::MEMBER_CONNECTION_DROPPED.into() });
    }
}
