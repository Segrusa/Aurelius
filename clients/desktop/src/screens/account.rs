//! Settings → Account (protocol 5, Fase 3; ui-ontwerp §2 and §4): who is signed in
//! on this device, Sign out and Change password, the own sessions with Revoke,
//! and the own Claude login (MU3-09): its status, Connect, Reconnect, Disconnect.
//!
//! Connect runs the unmodified `claude auth login` on the server for this board
//! member's own folder (B3, Sentinel §2). The window shows only what Claude asks:
//! the sign-in link, opened in the system browser (never a web view, C3) or
//! copied, and a field for the code Claude shows after the sign-in at Anthropic.
//! The code goes to the server once (`claude_login.input`) and is gone: never in
//! ui.json, a log, a notice or a `Debug` of this window's state (C5–C8).

use crate::app::{AppView, ConfirmKind, Overlay, Page};
use crate::backend::{self, ClaudeCall, ClaudeUpdate, LoginCode, LoginPrompt, UserCall};
use crate::theme::{self, c};
use crate::ui::{self, ButtonKind, h, v};
use aurelius_domain::user::{ClaudeLoginStatus, ClaudeLoginView};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

/// Settings → Account.
pub const ACCOUNT_TAB: usize = 13;

/// The residual risk of F2 (D5, Sentinel's words after SEC2 m8): said where a login is connected.
pub const SAME_OS_USER: &str = "Your Claude login is kept on this server. Other board members' agents could read your Claude login's files: their file tools are blocked from it, but a command they run is not. Connect only if you trust the work that runs on this server.";

/// B1-2 (Forge's text): an app key, a schedule or a heartbeat runs on its maker's login.
pub const OWN_SUBSCRIPTION: &str = "Tasks of this app run on your own Claude subscription.";

/// B1-2 for an app key: whose login its calls use (made it, or last set its functions), and SEC3 N3: not for others.
pub const APP_KEY_LOGIN: &str = "Its calls run on the Claude login of the board member who made it or last set its functions. Only for your own use: don't give this key to others. For calls from others, use an employee on an Anthropic API key connection.";

/// SEC2 m4, after a Disconnect: the session at Anthropic is not ended by Aurelius (the same on mobile).
pub const REVOKE_AT_ANTHROPIC: &str = "To end it at Anthropic too, revoke the session at claude.ai → Settings.";

/// The toast after a Disconnect.
pub fn disconnected_notice() -> String {
    format!("Claude login disconnected. {REVOKE_AT_ANTHROPIC}")
}

/// The daemon gives up on reading Claude's link after this long (C12): the command is the way then.
pub const COMMAND_AFTER_MS: i64 = 15_000;

/// What the own `/usage` says (B12, P-37): only ever the own login's, never another's.
#[derive(Debug, Clone, PartialEq)]
pub enum Usage {
    /// Not read yet, or not this login's.
    Unknown,
    /// The server could not read it; its words.
    Unavailable(String),
    /// `/usage` without "% used" lines: an API login (MU3-09 AC5), never "0 %".
    NoSubscription,
    /// The plan's limits.
    Limits(Vec<aurelius_protocol::PlanLimitView>),
}

/// The `/usage` on screen only when it is of the own login (R3 m2, B12): the title
/// bar's "Session N%" and the Usage page, like Account.
pub fn own_subscription(usage: Option<&aurelius_protocol::SubscriptionUsage>, me: Option<aurelius_domain::UserId>) -> Option<&aurelius_protocol::SubscriptionUsage> {
    usage.filter(|u| me.is_some() && u.login_user == me)
}

pub fn usage_of(usage: Option<&aurelius_protocol::SubscriptionUsage>, me: aurelius_domain::UserId) -> Usage {
    let Some(u) = usage.filter(|u| u.login_user == Some(me)) else { return Usage::Unknown };
    if let Some(why) = u.error.clone() {
        return Usage::Unavailable(why);
    }
    match (u.fetched_at_ms, u.limits.is_empty()) {
        (0, _) => Usage::Unknown,
        (_, true) => Usage::NoSubscription,
        (_, false) => Usage::Limits(u.limits.clone()),
    }
}

/// A limit at 100 %: its reset ("16:00"), or an empty text when Claude gave none.
pub fn limit_reached(usage: &Usage) -> Option<String> {
    match usage {
        Usage::Limits(limits) => limits.iter().find(|l| l.percent >= 100).map(|l| l.resets.clone().unwrap_or_default()),
        _ => None,
    }
}

/// What Sentinel B1-3 asks the page to call it: the own account, at Anthropic.
pub const CONNECT_TITLE: &str = "Connect your own Claude account (at Anthropic)";

/// The status line of the own Claude login (ui-ontwerp §4), always with text and a shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusLine {
    pub mark: &'static str,
    pub text: String,
    /// The first action: Connect or Reconnect (none: Connected's own row).
    pub primary: Option<&'static str>,
}

/// The status of ui-ontwerp §4: the login's own, then what its `/usage` says.
pub fn status_line(view: &ClaudeLoginView, usage: &Usage) -> StatusLine {
    match view.status {
        ClaudeLoginStatus::Missing => StatusLine { mark: "○", text: "Not connected. Your tickets wait until you connect.".into(), primary: Some("Connect") },
        ClaudeLoginStatus::Expired => StatusLine { mark: "✕", text: "Expired. Claude asked to sign in again.".into(), primary: Some("Reconnect") },
        ClaudeLoginStatus::Linked if *usage == Usage::NoSubscription => StatusLine { mark: "⚠", text: "This login has no Claude subscription (API usage only).".into(), primary: Some("Reconnect") },
        ClaudeLoginStatus::Linked if limit_reached(usage).is_some() => {
            let until = limit_reached(usage).unwrap_or_default();
            StatusLine { mark: "⏸", text: if until.is_empty() { "Limit reached.".into() } else { format!("Limit reached until {until}.") }, primary: None }
        }
        ClaudeLoginStatus::Linked => {
            let who = [view.email.clone(), view.plan.as_deref().map(plan_name)].into_iter().flatten().collect::<Vec<_>>().join(" · ");
            StatusLine { mark: "●", text: if who.is_empty() { "Connected".into() } else { format!("Connected  {who}") }, primary: None }
        }
    }
}

/// "max" → "Max".
pub fn plan_name(plan: &str) -> String {
    let mut chars = plan.chars();
    chars.next().map_or_else(String::new, |f| f.to_uppercase().collect::<String>() + chars.as_str())
}

/// Where a Connect stands, from what the server said.
#[derive(Debug, Clone, PartialEq)]
pub enum ConnectStep {
    /// `claude_login.link` sent; no answer yet.
    Starting,
    /// The link runs; Claude did not print its link yet.
    Waiting,
    /// Claude's sign-in link, and whether it waits for the code.
    Open { url: Option<String>, asks_code: bool },
    /// The code went; Claude checks it.
    Checking,
    /// It ended without a login: Claude's own words, for Try again.
    Failed(String),
}

/// A running Connect in this window.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Link {
    pub id: Option<String>,
    pub prompt: Option<LoginPrompt>,
    /// The code went (`claude_login.input`), the answer is not here yet.
    pub sent: bool,
    /// Why it stopped, when it did.
    pub failure: Option<String>,
    /// When this window asked for it (ms since 1970).
    pub started_ms: i64,
}

/// Show command as the way out (P-32, C12): Claude's link did not come within
/// 15 s, or the server said to use it (its own words name "Show command").
pub fn command_fallback(link: &Link, now_ms: i64) -> bool {
    match connect_step(link) {
        ConnectStep::Failed(why) => why.contains("Show command"),
        ConnectStep::Starting | ConnectStep::Waiting => link.started_ms > 0 && now_ms - link.started_ms >= COMMAND_AFTER_MS,
        ConnectStep::Open { url, asks_code } => url.is_none() && !asks_code,
        ConnectStep::Checking => false,
    }
}

pub fn connect_step(link: &Link) -> ConnectStep {
    if let Some(why) = link.failure.clone().or_else(|| link.prompt.as_ref().and_then(|p| p.error.clone())) {
        return ConnectStep::Failed(why);
    }
    if link.sent {
        return ConnectStep::Checking;
    }
    match (&link.id, &link.prompt) {
        (None, _) => ConnectStep::Starting,
        (Some(_), None) => ConnectStep::Waiting,
        (Some(_), Some(p)) => ConnectStep::Open { url: p.url.clone(), asks_code: p.asks_code },
    }
}

/// The terminal route (B1-4, P-32): the same login on the server itself, with every
/// sign-in method of the CLI (`--console`, `--sso`). The folder is the board member's
/// own under the server's data folder, named by their id (never their username).
pub fn terminal_command(user_id: aurelius_domain::UserId) -> String {
    format!("CLAUDE_CONFIG_DIR=\"<server data folder>/logins/{user_id}\" claude auth login")
}

/// What the code field may hold before it goes (C7 on the server too): the code
/// as Claude shows it, without spaces around it. None: nothing to send.
pub fn code_to_send(text: &str) -> Option<String> {
    let code = text.trim();
    (!code.is_empty()).then(|| code.to_string())
}

/// The own Claude login's state in a window.
pub struct Claude {
    /// The server's last word (none: not read yet).
    pub view: Option<ClaudeLoginView>,
    /// The server has no Claude login per board member yet (`backend::NOT_BUILT`).
    pub unsupported: Option<String>,
    /// A Connect in this window.
    pub link: Option<Link>,
    /// Show the terminal command.
    pub show_command: bool,
    /// A call is on its way (Cancel, Disconnect, Check now).
    pub busy: bool,
    /// Disconnect was asked: its answer says how to end the session at Anthropic too.
    pub unlinking: bool,
    /// The code Claude showed; emptied as soon as it went.
    pub code: Entity<InputState>,
    pub clear_code: bool,
}

impl Claude {
    pub fn new(window: &mut Window, cx: &mut Context<AppView>) -> Self {
        Self {
            view: None,
            unsupported: None,
            link: None,
            show_command: false,
            busy: false,
            unlinking: false,
            code: cx.new(|cx| InputState::new(window, cx).placeholder("Paste the code Claude shows you")),
            clear_code: false,
        }
    }

    pub fn subscriptions(&self, window: &mut Window, cx: &mut Context<AppView>) -> Vec<Subscription> {
        vec![cx.subscribe_in(&self.code, window, |this: &mut AppView, _, event: &InputEvent, _, cx| {
            if let InputEvent::PressEnter { .. } = event {
                this.claude_send_code(cx);
            }
        })]
    }

    /// The own login is connected: tickets can run on it.
    pub fn linked(&self) -> bool {
        self.view.as_ref().is_some_and(|v| v.status == ClaudeLoginStatus::Linked)
    }
}

/// Where the code field is on screen, so its emptying can happen: the Connect
/// dialog over any page, or Settings → Account.
pub fn code_cleared_on(overlay: Overlay, screen: Page, settings_tab: usize) -> bool {
    overlay == Overlay::ConnectClaude || (screen == Page::Settings && settings_tab == ACCOUNT_TAB)
}

/// The Board's banner (ui-ontwerp §4): the own login is missing or expired. Not
/// while unknown, nor on a server without logins per board member.
pub fn needs_connect(view: Option<&ClaudeLoginView>, unsupported: bool) -> bool {
    !unsupported && view.is_some_and(|v| v.status != ClaudeLoginStatus::Linked)
}

impl AppView {
    /// A new board member or server: nothing of the previous login is kept.
    pub fn claude_reset(&mut self) {
        self.claude.view = None;
        self.claude.unsupported = None;
        self.claude.link = None;
        self.claude.show_command = false;
        self.claude.busy = false;
        self.claude.unlinking = false;
        self.claude.clear_code = true;
    }

    pub fn apply_claude(&mut self, update: ClaudeUpdate, cx: &mut Context<Self>) {
        self.claude.busy = false;
        match update {
            ClaudeUpdate::Status(view) => {
                let linked = view.status == ClaudeLoginStatus::Linked;
                // A Connect of this window worked: its code went (a Reconnect), or the login is new.
                let connected = linked && self.claude.link.as_ref().is_some_and(|l| l.sent || !self.claude.linked());
                if connected {
                    self.claude.link = None;
                    self.claude.clear_code = true;
                    if self.overlay == Overlay::ConnectClaude {
                        self.overlay = Overlay::None;
                    }
                    self.notify_user(format!("Claude login connected{}.", view.email.as_ref().map(|e| format!(" as {e}")).unwrap_or_default()), false, cx);
                    // The own usage shows with the login (B12: only the own).
                    let _ = self.requests_handle().send(backend::Request::LoadSubscription(false));
                }
                if std::mem::take(&mut self.claude.unlinking) && !linked {
                    self.notify_user(disconnected_notice(), false, cx);
                }
                self.claude.unsupported = None;
                self.claude.view = Some(view);
            }
            ClaudeUpdate::Linking(id) => {
                let link = self.claude.link.get_or_insert_with(Link::default);
                link.id = Some(id);
            }
            ClaudeUpdate::Prompt(prompt) => {
                // Only the link of this window (the server sends it to this connection only).
                if let Some(link) = self.claude.link.as_mut().filter(|l| l.id.is_none() || l.id.as_deref() == Some(prompt.link_id.as_str())) {
                    link.id = Some(prompt.link_id.clone());
                    link.sent = link.sent && prompt.error.is_none() && !prompt.asks_code;
                    link.prompt = Some(prompt);
                }
            }
            ClaudeUpdate::Failed(why) => {
                self.claude.unlinking = false;
                if why == backend::NOT_BUILT && self.claude.view.is_none() {
                    self.claude.unsupported = Some(why);
                } else if let Some(link) = self.claude.link.as_mut() {
                    link.sent = false;
                    link.failure = Some(why);
                } else {
                    self.notify_user(why, true, cx);
                }
            }
        }
        cx.notify();
    }

    /// Connect / Reconnect / Try again.
    pub fn claude_connect(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.claude.link.as_ref().and_then(|l| l.id.clone()) {
            let _ = self.requests_handle().send(backend::Request::Claude(ClaudeCall::Cancel(id)));
        }
        self.claude.link = Some(Link { started_ms: ui::now_ms(), ..Link::default() });
        self.claude.show_command = false;
        self.claude.clear_code = true;
        let _ = self.requests_handle().send(backend::Request::Claude(ClaudeCall::Link));
        // A dialog over the page it was asked from (Account, the Board's banner, a ticket).
        self.overlay = Overlay::ConnectClaude;
        // After 15 s without Claude's link the dialog offers the command by itself.
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(std::time::Duration::from_millis(COMMAND_AFTER_MS as u64)).await;
            let _ = this.update(cx, |_, cx| cx.notify());
        })
        .detach();
        cx.notify();
    }

    /// Open Settings → Account (the Board's banner, a ticket's Connect).
    pub fn open_account(&mut self, cx: &mut Context<Self>) {
        self.navigate(Page::Settings);
        self.set_settings_tab(ACCOUNT_TAB, cx);
        cx.notify();
    }

    pub fn claude_cancel(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.claude.link.take().and_then(|l| l.id) {
            self.claude.busy = true;
            let _ = self.requests_handle().send(backend::Request::Claude(ClaudeCall::Cancel(id)));
        }
        self.claude.clear_code = true;
        if self.overlay == Overlay::ConnectClaude {
            self.overlay = Overlay::None;
        }
        cx.notify();
    }

    /// The code goes once, then the field is emptied (C7: one write).
    pub fn claude_send_code(&mut self, cx: &mut Context<Self>) {
        let Some(link) = self.claude.link.as_mut() else { return };
        let (Some(id), false) = (link.id.clone(), link.sent) else { return };
        let Some(code) = code_to_send(&self.claude.code.read(cx).value()) else { return };
        link.sent = true;
        link.failure = None;
        self.claude.clear_code = true;
        let _ = self.requests_handle().send(backend::Request::Claude(ClaudeCall::Input { link_id: id, code: LoginCode::new(code) }));
        cx.notify();
    }

    pub fn claude_check(&mut self, cx: &mut Context<Self>) {
        self.claude.busy = true;
        let _ = self.requests_handle().send(backend::Request::Claude(ClaudeCall::Status));
        let _ = self.requests_handle().send(backend::Request::LoadSubscription(true));
        cx.notify();
    }

    pub fn claude_unlink(&mut self, cx: &mut Context<Self>) {
        self.claude.busy = true;
        self.claude.unlinking = true;
        let _ = self.requests_handle().send(backend::Request::Claude(ClaudeCall::Unlink));
        cx.notify();
    }

    /// Opening Settings → Account: read the sessions and the login again.
    pub fn enter_account_tab(&mut self) {
        let _ = self.requests_handle().send(backend::Request::BoardMembers(UserCall::Sessions(None)));
        let _ = self.requests_handle().send(backend::Request::Claude(ClaudeCall::Status));
    }

    /// The gate opened: what the open page shows loads again (it waited at the gate).
    pub fn signed_in_reload(&mut self) {
        self.board_refresh();
        let _ = self.requests_handle().send(backend::Request::LoadChannels);
        if self.screen == Page::Settings && self.settings_tab == ACCOUNT_TAB {
            self.enter_account_tab();
        }
    }

    /// Empties the code field once asked (sent, Try again, Cancel), wherever it shows:
    /// the dialog is also opened over the Board or a ticket (QA3 D-m3).
    pub fn claude_clear_code(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if code_cleared_on(self.overlay, self.screen, self.settings_tab) && std::mem::take(&mut self.claude.clear_code) {
            self.claude.code.update(cx, |i, cx| i.set_value("", window, cx));
        }
    }

    // ── Rendering ──────────────────────────────────────────────────────

    /// Settings → Account.
    pub fn render_account_settings(&mut self, page: Div, window: &mut Window, cx: &mut Context<Self>) -> Div {
        self.claude_clear_code(window, cx);
        let header = h().px(px(22.)).py(px(14.)).border_b_1().border_color(c(theme::BORDER)).child(self.settings_title(cx));
        let me = self.active_member().cloned();
        let host = self.remote_name().map_or("this computer".to_string(), str::to_owned);
        let others = self.members.sessions.get(&None).and_then(|r| r.as_ref().ok()).is_some_and(|l| l.sessions.iter().any(|s| !s.current));
        let body = v().id("account-settings-body").flex_1().min_h_0().overflow_y_scrollbar().p(px(22.)).gap(px(18.))
            .child(
                v().gap(px(8.))
                    .child(ui::eyebrow("SIGNED IN"))
                    .child(
                        h().gap(px(10.)).items_center()
                            .children(me.as_ref().map(|u| ui::member_monogram(&u.username, 26., cx)))
                            .child(v().flex_1().min_w_0()
                                .child(div().text_size(px(14.)).child(me.as_ref().map_or_else(|| "Nobody".to_string(), |u| u.username.clone())))
                                .child(div().text_size(px(12.)).text_color(c(theme::MUTED)).child(format!("on {host}"))))
                            .child(ui::button("account-change-password", "Change password…", ButtonKind::Outline, None).on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.members_change_password(w, cx))))
                            .child(ui::button("account-sign-out", "Sign out", ButtonKind::Outline, None).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.members_sign_out(cx)))),
                    ),
            )
            .child(
                v().gap(px(6.))
                    .child(ui::eyebrow_with_info("account-sessions-about", "SESSIONS", "Devices signed in as you. Revoke signs that device out at once: it needs your password to come back. A session ends by itself after 30 days without use, or after 90 days."))
                    .child(self.render_sessions(None, ("account-session", 0), cx))
                    .when(others, |el| {
                        el.child(h().pt(px(4.)).child(ui::button("account-revoke-others", "Sign out all other devices", ButtonKind::Outline, None).on_click(cx.listener(|this, _: &ClickEvent, w, cx| {
                            this.overlay = Overlay::Confirm(ConfirmKind::RevokeOtherSessions);
                            this.overlay_focus.focus(w, cx);
                            cx.notify();
                        }))))
                    }),
            )
            .child(self.render_claude_login(cx));
        page.child(header).child(body)
    }

    /// The own Claude login (ui-ontwerp §4).
    fn render_claude_login(&mut self, cx: &mut Context<Self>) -> Div {
        let section = v().gap(px(8.)).child(ui::eyebrow_with_info("account-claude-about", "CLAUDE LOGIN", "Tickets you create, and messages you send, run on your own Claude subscription. Aurelius never sees your Claude password and keeps no token: Claude Code keeps its own login in your folder on the server."));
        if let Some(why) = self.claude.unsupported.clone() {
            return section.child(div().text_size(px(12.5)).text_color(c(theme::TEXT_2)).child(why));
        }
        let Some(view) = self.claude.view.clone() else {
            return section.child(div().text_size(px(12.5)).text_color(c(theme::MUTED)).child("Loading…"));
        };
        let linked = view.status == ClaudeLoginStatus::Linked;
        // B12: only the own login's usage, and only while it is connected.
        let usage = if linked { usage_of(self.subscription.as_ref(), view.user_id) } else { Usage::Unknown };
        let line = status_line(&view, &usage);
        let busy = self.claude.busy;
        let mark_colour = match line.mark {
            "●" => theme::ACCENT_TEXT,
            "✕" => theme::NEGATIVE,
            "⚠" | "⏸" => theme::ACCENT_TEXT,
            _ => theme::TEXT_2,
        };
        section
            .child(div().text_size(px(12.5)).text_color(c(theme::TEXT_2)).child("Tickets you create run on your own Claude subscription."))
            .child(
                h().id("claude-status").role(Role::Status).aria_label(line.text.clone()).gap(px(8.)).items_center().text_size(px(13.))
                    .child(div().text_color(c(mark_colour)).child(line.mark))
                    .child(div().flex_1().child(line.text.clone())),
            )
            .map(|el| match &usage {
                Usage::Limits(limits) => el.child(v().id("claude-usage").gap(px(4.)).pl(px(18.)).children(limits.iter().map(|l| {
                    h().gap(px(8.)).text_size(px(12.)).text_color(c(theme::TEXT_2))
                        .child(div().w(px(110.)).child(l.label.clone()))
                        .child(div().w(px(120.)).h(px(6.)).rounded(px(3.)).bg(c(theme::SURFACE_2)).child(div().h_full().rounded(px(3.)).bg(c(theme::ACCENT)).w(px(1.2 * l.percent.min(100) as f32))))
                        .child(div().child(format!("{} %{}", l.percent, l.resets.as_ref().map(|r| format!(" · resets {r}")).unwrap_or_default())))
                }))),
                Usage::Unavailable(why) => el.child(div().pl(px(18.)).text_size(px(12.)).text_color(c(theme::MUTED)).child(why.clone())),
                // MU3-09 AC5: no usage lines is never "0 %"; the status already says why.
                Usage::NoSubscription => el,
                Usage::Unknown if linked => el.child(div().pl(px(18.)).text_size(px(12.)).text_color(c(theme::MUTED)).child("Your usage is not read yet. Check now reads it.")),
                Usage::Unknown => el,
            })
            .when(view.background, |el| el.child(div().pl(px(18.)).text_size(px(12.)).text_color(c(theme::TEXT_2)).child("Also runs the company's background jobs (Settings → Company).")))
            .map(|el| {
                el.child(
                    h().gap(px(8.)).pt(px(2.))
                        .when_some(line.primary, |el, label| el.child(ui::button("claude-connect", label, ButtonKind::Primary, None).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.claude_connect(cx)))))
                        .when(linked, |el| {
                            el.child(ui::button("claude-check", if busy { "Checking…" } else { "Check now" }, ButtonKind::Outline, None).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.claude_check(cx))))
                                .when(line.primary.is_none(), |el| el.child(ui::button("claude-reconnect", "Reconnect", ButtonKind::Outline, None).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.claude_connect(cx)))))
                        })
                        .when(view.status != ClaudeLoginStatus::Missing, |el| {
                            el.child(ui::button("claude-disconnect", "Disconnect…", ButtonKind::Outline, None).on_click(cx.listener(|this, _: &ClickEvent, w, cx| {
                                this.overlay = Overlay::Confirm(ConfirmKind::DisconnectClaude);
                                this.overlay_focus.focus(w, cx);
                                cx.notify();
                            })))
                        }),
                )
            })
            .child(div().text_size(px(11.5)).line_height(px(17.)).text_color(c(theme::MUTED)).child(SAME_OS_USER))
    }

    /// The Connect dialog over the window (ui-ontwerp §4).
    pub fn render_connect_dialog(&mut self, max_h: f32, cx: &mut Context<Self>) -> Div {
        let Some(user_id) = self.claude.view.as_ref().map(|v| v.user_id).or_else(|| self.active_member().map(|u| u.id)) else { return div() };
        if self.claude.link.is_none() {
            return div();
        }
        v().w(px(520.)).max_h(px(max_h)).occlude().shadow_lg().rounded(px(10.)).child(self.render_connect(user_id, cx).overflow_y_scroll())
    }

    /// Connect your Claude login (ui-ontwerp §4): the link Claude printed, the code it shows.
    fn render_connect(&mut self, user_id: aurelius_domain::UserId, cx: &mut Context<Self>) -> Stateful<Div> {
        let link = self.claude.link.clone().unwrap_or_default();
        let step = connect_step(&link);
        let fallback = command_fallback(&link, ui::now_ms());
        let show_command = self.claude.show_command || fallback;
        let command = terminal_command(user_id);
        let (url, asks_code) = match &step {
            ConnectStep::Open { url, asks_code } => (url.clone(), *asks_code),
            _ => (None, false),
        };
        let numbered = |n: &'static str, text: &'static str| h().gap(px(10.)).items_start().text_size(px(13.)).child(div().w(px(14.)).text_color(c(theme::MUTED)).child(n)).child(div().flex_1().child(text));
        let copied = self.copied.as_deref() == Some("claude-link");
        v().id("claude-connect-panel").role(Role::Dialog).aria_label(CONNECT_TITLE).gap(px(12.)).p(px(16.)).rounded(px(10.)).border_1().border_color(c(theme::BORDER_STRONG)).bg(c(theme::PANEL))
            .child(div().font_family(ui::serif(cx)).text_size(px(17.)).child(CONNECT_TITLE))
            .child(div().text_size(px(12.)).line_height(px(17.)).text_color(c(theme::TEXT_2)).child(SAME_OS_USER))
            .map(|el| match &step {
                ConnectStep::Starting | ConnectStep::Waiting if fallback => el.child(div().id("claude-connect-slow").role(Role::Status).text_size(px(12.5)).text_color(c(theme::TEXT_2)).child("Claude's sign-in link did not come. Use the command below on the server, or wait.")),
                ConnectStep::Starting | ConnectStep::Waiting => el.child(div().text_size(px(12.5)).text_color(c(theme::MUTED)).child("Starting Claude's sign-in on the server…")),
                ConnectStep::Checking => el.child(div().text_size(px(12.5)).text_color(c(theme::MUTED)).child("Claude checks the code…")),
                ConnectStep::Failed(why) => el.child(
                    v().gap(px(8.))
                        .child(h().id("claude-connect-failed").role(Role::Alert).gap(px(6.)).text_size(px(12.5)).text_color(c(theme::NEGATIVE)).child("✕").child(why.clone()))
                        .child(h().child(ui::button("claude-try-again", "Try again", ButtonKind::Primary, None).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.claude_connect(cx))))),
                ),
                ConnectStep::Open { url: None, asks_code: false } => el.child(div().text_size(px(12.5)).text_color(c(theme::TEXT_2)).child("Claude did not show a sign-in link here. Use the command below on the server.")),
                ConnectStep::Open { .. } => el
                    .child(numbered("1", "Open the Claude sign-in page and sign in yourself."))
                    .when_some(url.clone(), |el, url| {
                        let open = url.clone();
                        el.child(
                            h().pl(px(24.)).gap(px(8.))
                                .child(ui::button("claude-open-link", "Open sign-in page ↗", ButtonKind::Primary, None).on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.open_url(&open))))
                                .child(ui::button("claude-copy-link", if copied { "Copied" } else { "Copy link" }, ButtonKind::Outline, None).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.copy_text("claude-link".into(), url.clone(), cx)))),
                        )
                    })
                    .when(asks_code, |el| {
                        el.child(numbered("2", "Paste the code Claude shows you."))
                            .child(h().pl(px(24.)).gap(px(8.)).child(div().flex_1().child(ui::input(&self.claude.code))).child(
                                ui::button("claude-send-code", "Connect", ButtonKind::Primary, Some(ui::kbd("enter", cx))).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.claude_send_code(cx))),
                            ))
                    }),
            })
            .child(div().text_size(px(12.)).text_color(c(theme::MUTED)).child("Aurelius never sees your Claude password."))
            .when(!fallback, |el| el.child(
                h().gap(px(6.)).text_size(px(12.)).text_color(c(theme::TEXT_2))
                    .child("Prefer a terminal?")
                    .child(div().id("claude-show-command").role(Role::Button).cursor_pointer().underline().child(if show_command { "Hide command" } else { "Show command" }).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.claude.show_command = !this.claude.show_command;
                        cx.notify();
                    }))),
            ))
            .when(show_command, |el| {
                el.child(
                    v().gap(px(4.))
                        .child(div().px(px(10.)).py(px(8.)).rounded(px(6.)).bg(c(theme::SURFACE_2)).font_family(ui::mono(cx)).text_size(px(11.5)).child(command.clone()))
                        .child(div().text_size(px(11.5)).text_color(c(theme::MUTED)).child("On the server, as the user that runs Aurelius. Add --console or --sso to sign in another way.")),
                )
            })
            .child(h().justify_end().child(ui::button("claude-cancel", "Cancel", ButtonKind::Outline, None).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.claude_cancel(cx)))))
    }

    /// Settings → Company: whose Claude login runs the background jobs (B6). Only the
    /// own login can be chosen (Sentinel B1-2: with the board member's own consent).
    pub fn render_background_login(&self, cx: &mut Context<Self>) -> Div {
        let company = self.company.as_ref();
        let current = company.and_then(|c| c.background_login.clone());
        let me = self.active_member().map(|u| u.id);
        let mine = current.as_ref().is_some_and(|u| Some(u.id) == me);
        let text = match &current {
            Some(u) if mine => "Memory consolidation runs on your Claude login.".to_string(),
            Some(u) => format!("Memory consolidation runs on {}'s Claude login.", u.username),
            None => "No Claude login is chosen: background jobs (memory consolidation) do not run.".to_string(),
        };
        let linked = self.claude.linked();
        v().gap(px(6.))
            .child(ui::eyebrow_with_info("background-login-about", "BACKGROUND JOBS", "Work that belongs to no ticket or message, like memory consolidation, needs a board member's Claude login. Only you can choose your own; the jobs then use your Claude subscription. Nobody's login is used without that choice."))
            .child(div().text_size(px(12.5)).text_color(c(theme::TEXT_2)).child(text))
            .child(h().gap(px(8.)).map(|el| {
                if mine {
                    el.child(ui::button("background-login-stop", "Stop using my login", ButtonKind::Outline, None).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.set_background_login(false, cx))))
                } else if linked {
                    el.child(ui::button("background-login-use", "Use my Claude login", ButtonKind::Outline, None).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.set_background_login(true, cx))))
                } else {
                    el.child(ui::button("background-login-use", "Use my Claude login", ButtonKind::Outline, None).opacity(0.5).tooltip(ui::tip("Connect your Claude login first (Settings → Account).", None)))
                }
            }))
    }

    pub fn set_background_login(&mut self, enabled: bool, cx: &mut Context<Self>) {
        let done = if enabled { "Background jobs now run on your Claude login." } else { "Background jobs no longer run on your Claude login." };
        let command = aurelius_domain::Command::SetBackgroundLogin(aurelius_domain::company::SetBackgroundLogin { enabled });
        let _ = self.requests_handle().send(backend::Request::Board(backend::BoardCall::Write { command, expected_version: None, done: Some(done.into()) }));
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{APP_KEY_LOGIN, REVOKE_AT_ANTHROPIC, disconnected_notice, COMMAND_AFTER_MS, ConnectStep, Link, OWN_SUBSCRIPTION, SAME_OS_USER, Usage, code_cleared_on, code_to_send, command_fallback, own_subscription, connect_step, limit_reached, needs_connect, plan_name, status_line, terminal_command, usage_of};
    use crate::backend::LoginPrompt;
    use aurelius_domain::UserId;
    use aurelius_domain::user::{ClaudeLoginStatus, ClaudeLoginView};
    use aurelius_protocol::{PlanLimitView, SubscriptionUsage};

    fn view(status: ClaudeLoginStatus) -> ClaudeLoginView {
        ClaudeLoginView { user_id: UserId::new(), status, email: None, plan: None, linked_at_ms: None, checked_at_ms: None, link_id: None, background: false }
    }

    fn limit(label: &str, percent: u8, resets: &str) -> PlanLimitView {
        PlanLimitView { label: label.into(), percent, resets: Some(resets.into()) }
    }

    /// MU3-09 AC2 and ui-ontwerp §4: each status with text and a shape, and its first action.
    #[test]
    fn the_status_says_what_to_do() {
        let missing = status_line(&view(ClaudeLoginStatus::Missing), &Usage::Unknown);
        assert_eq!((missing.mark, missing.primary), ("○", Some("Connect")));
        assert!(missing.text.contains("Your tickets wait"));
        let expired = status_line(&view(ClaudeLoginStatus::Expired), &Usage::Unknown);
        assert_eq!((expired.mark, expired.primary), ("✕", Some("Reconnect")));
        let linked = ClaudeLoginView { email: Some("test@example.com".into()), plan: Some("max".into()), ..view(ClaudeLoginStatus::Linked) };
        let line = status_line(&linked, &Usage::Limits(vec![limit("Session limit", 68, "16:00")]));
        assert_eq!((line.mark, line.primary, line.text.as_str()), ("●", None, "Connected  test@example.com · Max"));
        assert_eq!(plan_name("pro"), "Pro");
        // Limit reached: no action, the tickets go on by themselves.
        let full = Usage::Limits(vec![limit("Session limit", 100, "16:00"), limit("Weekly limit", 21, "Mon")]);
        let line = status_line(&linked, &full);
        assert_eq!((line.mark, line.primary, line.text.as_str()), ("⏸", None, "Limit reached until 16:00."));
        // No subscription: an API login, with Reconnect.
        let line = status_line(&linked, &Usage::NoSubscription);
        assert_eq!((line.mark, line.primary, line.text.as_str()), ("⚠", Some("Reconnect"), "This login has no Claude subscription (API usage only)."));
    }

    /// B12 / P-37 / MU3-09 AC5: only the own login's usage, and no lines is never "0 %".
    #[test]
    fn the_usage_is_only_the_own_and_never_a_made_up_zero() {
        let me = UserId::new();
        let other = UserId::new();
        let read = |login_user, limits: Vec<PlanLimitView>, error: Option<&str>| SubscriptionUsage { login_user, limits, raw: "Usage: API".into(), fetched_at_ms: 1, error: error.map(Into::into), plan: None };
        assert_eq!(usage_of(None, me), Usage::Unknown);
        assert_eq!(usage_of(Some(&read(Some(other), vec![limit("Session limit", 40, "16:00")], None)), me), Usage::Unknown, "never another's");
        assert_eq!(usage_of(Some(&read(None, vec![limit("Session limit", 40, "16:00")], None)), me), Usage::Unknown, "the server's own login is nobody's");
        assert_eq!(usage_of(Some(&read(Some(me), vec![], None)), me), Usage::NoSubscription);
        assert_eq!(usage_of(Some(&SubscriptionUsage { login_user: Some(me), ..Default::default() }), me), Usage::Unknown, "not fetched yet");
        assert_eq!(usage_of(Some(&read(Some(me), vec![], Some("Connect your Claude login to see your usage."))), me), Usage::Unavailable("Connect your Claude login to see your usage.".into()));
        let mine = usage_of(Some(&read(Some(me), vec![limit("Session limit", 68, "16:00")], None)), me);
        assert_eq!(limit_reached(&mine), None);
        assert_eq!(limit_reached(&Usage::Limits(vec![limit("Weekly limit", 100, "Mon")])).as_deref(), Some("Mon"));
        // The title bar and the Usage page: the same rule (R3 m2).
        let theirs = read(Some(other), vec![limit("Session limit", 40, "16:00")], None);
        assert!(own_subscription(Some(&theirs), Some(me)).is_none(), "never another's");
        assert!(own_subscription(Some(&read(None, vec![], None)), None).is_none(), "signed out: nobody's");
        let mine = read(Some(me), vec![limit("Session limit", 68, "16:00")], None);
        assert_eq!(own_subscription(Some(&mine), Some(me)).map(|u| u.limits[0].percent), Some(68));
    }

    /// P-32 / C12: Show command comes by itself after 15 s without Claude's link, or when the daemon says so.
    #[test]
    fn show_command_is_the_fallback_after_fifteen_seconds() {
        let link = Link { id: Some("l1".into()), started_ms: 1_000, ..Link::default() };
        assert!(!command_fallback(&link, 1_000 + COMMAND_AFTER_MS - 1));
        assert!(command_fallback(&link, 1_000 + COMMAND_AFTER_MS));
        let open = Link { prompt: Some(LoginPrompt { link_id: "l1".into(), url: Some("https://claude.com/x".into()), asks_code: true, error: None }), ..link.clone() };
        assert!(!command_fallback(&open, 1_000 + 60_000), "the link came: no fallback");
        let refused = Link { failure: Some("Aurelius could not read Claude's sign-in link. Use Show command to connect in a terminal on the server.".into()), ..link.clone() };
        assert!(command_fallback(&refused, 0));
        let wrong = Link { failure: Some("The code was not accepted. Start again.".into()), ..link };
        assert!(!command_fallback(&wrong, 0), "a refused code is Try again, not the command");
    }

    /// D5 (Sentinel's words, R3 M3 / SEC2 m8; the same on mobile) and B1-2, as they are.
    #[test]
    fn the_notices_are_forges_words() {
        assert_eq!(SAME_OS_USER, "Your Claude login is kept on this server. Other board members' agents could read your Claude login's files: their file tools are blocked from it, but a command they run is not. Connect only if you trust the work that runs on this server.");
        assert_eq!(OWN_SUBSCRIPTION, "Tasks of this app run on your own Claude subscription.");
        assert_eq!(APP_KEY_LOGIN, "Its calls run on the Claude login of the board member who made it or last set its functions. Only for your own use: don't give this key to others. For calls from others, use an employee on an Anthropic API key connection.");
        // SEC2 m4: after a Disconnect.
        assert_eq!(REVOKE_AT_ANTHROPIC, "To end it at Anthropic too, revoke the session at claude.ai → Settings.");
        assert!(disconnected_notice().ends_with(REVOKE_AT_ANTHROPIC));
    }

    /// The Connect flow from the link to the code, and its failure for Try again.
    #[test]
    fn connect_follows_what_claude_asks() {
        let mut link = Link::default();
        assert_eq!(connect_step(&link), ConnectStep::Starting);
        link.id = Some("l1".into());
        assert_eq!(connect_step(&link), ConnectStep::Waiting);
        let url = "https://claude.com/cai/oauth/authorize?code=true&state=x".to_string();
        link.prompt = Some(LoginPrompt { link_id: "l1".into(), url: Some(url.clone()), asks_code: true, error: None });
        assert_eq!(connect_step(&link), ConnectStep::Open { url: Some(url), asks_code: true });
        link.sent = true;
        assert_eq!(connect_step(&link), ConnectStep::Checking);
        link.prompt = Some(LoginPrompt { link_id: "l1".into(), url: None, asks_code: false, error: Some("Login failed: That code wasn't accepted.".into()) });
        assert!(matches!(connect_step(&link), ConnectStep::Failed(why) if why.contains("wasn't accepted")), "Claude's own words");
    }

    /// B1-4 / P-32: the terminal route names the own folder by id, never by username.
    #[test]
    fn the_terminal_command_is_the_unmodified_login_in_the_own_folder() {
        let id = UserId::new();
        let command = terminal_command(id);
        assert!(command.contains(&format!("/logins/{id}")) && command.ends_with("claude auth login"), "{command}");
    }

    /// QA3 D-m3: after a refused code, Try again empties the field also when the
    /// dialog is over the Board or a ticket (not only on Settings → Account).
    #[test]
    fn the_refused_code_is_emptied_wherever_the_dialog_shows() {
        use crate::app::{Overlay, Page};
        assert!(code_cleared_on(Overlay::ConnectClaude, Page::Board, 0));
        assert!(code_cleared_on(Overlay::ConnectClaude, Page::Dashboard, 0));
        assert!(code_cleared_on(Overlay::None, Page::Settings, super::ACCOUNT_TAB));
        assert!(!code_cleared_on(Overlay::None, Page::Board, 0), "kept until the field shows again");
    }

    #[test]
    fn the_code_goes_as_claude_shows_it() {
        assert_eq!(code_to_send("  abc#def \n").as_deref(), Some("abc#def"));
        assert_eq!(code_to_send("   "), None);
    }

    /// ui-ontwerp §4: the Board's reminder only for a missing or expired own login.
    #[test]
    fn the_board_reminds_only_without_a_working_login() {
        assert!(needs_connect(Some(&view(ClaudeLoginStatus::Missing)), false));
        assert!(needs_connect(Some(&view(ClaudeLoginStatus::Expired)), false));
        assert!(!needs_connect(Some(&view(ClaudeLoginStatus::Linked)), false));
        assert!(!needs_connect(None, false), "not read yet");
        assert!(!needs_connect(Some(&view(ClaudeLoginStatus::Missing)), true), "a server without logins per board member");
    }
}
