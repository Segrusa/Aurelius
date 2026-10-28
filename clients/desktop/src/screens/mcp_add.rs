//! Settings → MCP & tools: the Add MCP server dialog and the Remove action
//! (protocol 1.23, owner right). The server runs `claude mcp add/remove --scope user`.
//! Env and header values are secrets: they sit in masked fields, go out once with the
//! request, are never logged and are dropped as soon as the dialog closes.

use crate::app::AppView;
use crate::backend::{self, Connection};
use crate::keymap::LOAD;
use crate::theme::{self, c};
use crate::ui::{self, ButtonKind, h, v};
use aurelius_protocol::{McpAddParams, McpAuthStatus, McpLogin, McpLoginState, McpServerInfo, McpServers, McpTransport};
use gpui_kit::component::input::InputState;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

pub const NO_OWNER: &str = "You need the Owner right on this server to add or remove MCP servers.";
pub const OAUTH_NOTE: &str = "A remote server that needs an OAuth login: use Log in next to it in the list after adding it.";
pub const NO_APPROVE: &str = "You need the Approve right on this server to log in to MCP servers.";
pub const TOO_OLD: &str = "The server did not answer. If it is older than this app, update it to log in to MCP servers.";

/// An MCP login in progress (or one that ended with an error).
pub struct McpLoginRow {
    pub name: String,
    /// `None` until `mcp.login_start` answers.
    pub login: Option<McpLogin>,
    /// The address the browser ended on, for a server on another computer.
    pub paste: Entity<InputState>,
    pub working: bool,
    pub error: Option<String>,
}

/// A key with a (masked) value.
pub struct PairRow {
    pub key: Entity<InputState>,
    pub value: Entity<InputState>,
}

/// The Add MCP server dialog.
pub struct AddMcp {
    pub name: Entity<InputState>,
    pub remote: bool,
    pub command: Entity<InputState>,
    pub args: Entity<InputState>,
    pub env: Vec<PairRow>,
    pub url: Entity<InputState>,
    pub headers: Vec<PairRow>,
    pub enable: bool,
    pub working: bool,
    pub error: Option<String>,
}

#[derive(Default)]
pub struct McpManage {
    pub add: Option<AddMcp>,
    /// The server a Remove is asked about, and the one being removed.
    pub confirm_remove: Option<String>,
    pub removing: Option<String>,
    pub login: Option<McpLoginRow>,
    pub logging_out: Option<String>,
}

/// Servers with `mcp.login_start` always fill `auth`, so a stdio server is `none`
/// there; an older server leaves it out and every server reads `unknown`.
pub fn login_supported(list: &McpServers) -> bool {
    !list.servers.iter().any(|s| s.transport == McpTransport::Stdio && s.auth == McpAuthStatus::Unknown)
}

/// Split arguments like a shell would: spaces separate, quotes group.
pub fn split_args(text: &str) -> Vec<String> {
    let (mut out, mut cur, mut quote, mut any) = (Vec::new(), String::new(), None::<char>, false);
    for ch in text.chars() {
        match (quote, ch) {
            (Some(q), ch) if ch == q => quote = None,
            (Some(_), ch) => cur.push(ch),
            (None, '"' | '\'') => {
                quote = Some(ch);
                any = true;
            }
            (None, ch) if ch.is_whitespace() => {
                if any || !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                    any = false;
                }
            }
            (None, ch) => cur.push(ch),
        }
    }
    if any || !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Replace every secret value in `text` so an error can be shown safely.
pub fn scrub(text: &str, params: &McpAddParams) -> String {
    let mut out = text.to_owned();
    for secret in params.env.values().chain(params.headers.values()).chain(params.args.iter()) {
        if secret.len() >= 3 {
            out = out.replace(secret.as_str(), "•••");
        }
    }
    out
}

impl AppView {
    /// Why Add and Remove are off now; `None` when this connection may use them.
    pub fn mcp_manage_blocked(&self) -> Option<String> {
        match &self.connection {
            Connection::Online { server, .. } if !server.owner => Some(NO_OWNER.into()),
            Connection::Online { .. } => None,
            _ => Some("Not connected to a server.".into()),
        }
    }

    pub fn open_add_mcp(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.mcp_manage_blocked().is_some() {
            return;
        }
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("e.g. github"));
        let command = cx.new(|cx| InputState::new(window, cx).placeholder("e.g. npx"));
        let args = cx.new(|cx| InputState::new(window, cx).placeholder("e.g. -y @modelcontextprotocol/server-everything"));
        let url = cx.new(|cx| InputState::new(window, cx).placeholder("https://example.com/mcp"));
        name.update(cx, |i, cx| i.focus(window, cx));
        self.mcp_manage.add = Some(AddMcp { name, remote: false, command, args, env: Vec::new(), url, headers: Vec::new(), enable: true, working: false, error: None });
        cx.notify();
    }

    pub fn close_add_mcp(&mut self, cx: &mut Context<Self>) -> bool {
        match &self.mcp_manage.add {
            None => false,
            Some(a) if a.working => true,
            Some(_) => {
                self.mcp_manage.add = None;
                cx.notify();
                true
            }
        }
    }

    fn pair_row(window: &mut Window, cx: &mut Context<Self>, key_hint: &str, value_hint: &str) -> PairRow {
        let key = cx.new(|cx| InputState::new(window, cx).placeholder(key_hint.to_owned()));
        let value = cx.new(|cx| InputState::new(window, cx).masked(true).placeholder(value_hint.to_owned()));
        PairRow { key, value }
    }

    fn submit_add_mcp(&mut self, cx: &mut Context<Self>) {
        if self.mcp_manage_blocked().is_some() {
            return;
        }
        let Some(add) = self.mcp_manage.add.as_mut() else { return };
        if add.working {
            return;
        }
        let name = add.name.read(cx).value().trim().to_string();
        if name.is_empty() || name.contains(char::is_whitespace) {
            add.error = Some("Give the server a name without spaces.".into());
            cx.notify();
            return;
        }
        let pairs = |rows: &[PairRow], cx: &App| -> std::collections::BTreeMap<String, String> {
            rows.iter()
                .map(|r| (r.key.read(cx).value().trim().to_string(), r.value.read(cx).value().to_string()))
                .filter(|(k, _)| !k.is_empty())
                .collect()
        };
        let params = if add.remote {
            let url = add.url.read(cx).value().trim().to_string();
            if !(url.starts_with("https://") || url.starts_with("http://")) {
                add.error = Some("The URL must start with https:// (or http://).".into());
                cx.notify();
                return;
            }
            McpAddParams { name, transport: McpTransport::Http, command: None, args: Vec::new(), env: Default::default(), url: Some(url), headers: pairs(&add.headers, cx), enable: add.enable }
        } else {
            let command = add.command.read(cx).value().trim().to_string();
            if command.is_empty() {
                add.error = Some("Enter the command to run.".into());
                cx.notify();
                return;
            }
            McpAddParams { name, transport: McpTransport::Stdio, command: Some(command), args: split_args(&add.args.read(cx).value()), env: pairs(&add.env, cx), url: None, headers: Default::default(), enable: add.enable }
        };
        add.error = None;
        add.working = true;
        let _ = self.requests_handle().send(backend::Request::McpAdd(params));
        cx.notify();
    }

    pub fn remove_mcp(&mut self, name: &str, cx: &mut Context<Self>) {
        if self.mcp_manage_blocked().is_some() || self.mcp_manage.removing.is_some() {
            return;
        }
        self.mcp_manage.confirm_remove = None;
        self.mcp_manage.removing = Some(name.to_owned());
        self.mcp_error = None;
        let _ = self.requests_handle().send(backend::Request::McpRemove(name.to_owned()));
        cx.notify();
    }

    /// The server accepted the new server: close the dialog, dropping every secret field.
    pub fn mcp_added(&mut self, cx: &mut Context<Self>) {
        self.mcp_manage.add = None;
        cx.notify();
    }

    pub fn mcp_add_failed(&mut self, why: String, cx: &mut Context<Self>) {
        if let Some(add) = self.mcp_manage.add.as_mut() {
            add.working = false;
            add.error = Some(why);
        }
        cx.notify();
    }

    pub fn mcp_remove_failed(&mut self, why: String, cx: &mut Context<Self>) {
        self.mcp_manage.removing = None;
        self.mcp_error = Some(why);
        cx.notify();
    }

    /// Why Log in and Log out are off now; `None` when this connection may use them.
    pub fn mcp_auth_blocked(&self) -> Option<&'static str> {
        match &self.connection {
            Connection::Online { server, .. } if !server.approve => Some(NO_APPROVE),
            Connection::Online { .. } => None,
            _ => Some("Not connected to a server."),
        }
    }

    pub fn start_mcp_login(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.mcp_auth_blocked().is_some() || self.mcp_manage.login.as_ref().is_some_and(|l| l.working) {
            return;
        }
        let paste = cx.new(|cx| InputState::new(window, cx).placeholder("http://localhost:…/callback?code=…"));
        self.mcp_manage.login = Some(McpLoginRow { name: name.to_owned(), login: None, paste, working: true, error: None });
        self.mcp_error = None;
        let _ = self.requests_handle().send(backend::Request::McpAuth(backend::McpAuthCall::Start(name.to_owned())));
        cx.notify();
    }

    fn submit_mcp_paste(&mut self, cx: &mut Context<Self>) {
        let Some(row) = self.mcp_manage.login.as_mut().filter(|l| !l.working) else { return };
        let Some(login_id) = row.login.as_ref().filter(|l| l.state == McpLoginState::Pending).map(|l| l.login_id.clone()) else { return };
        let redirect_url = row.paste.read(cx).value().trim().to_string();
        if !redirect_url.contains("code=") {
            row.error = Some("Paste the full address the browser ended on; it contains code=….".into());
            cx.notify();
            return;
        }
        row.working = true;
        row.error = None;
        let _ = self.requests_handle().send(backend::Request::McpAuth(backend::McpAuthCall::Complete { login_id, redirect_url }));
        cx.notify();
    }

    pub fn logout_mcp(&mut self, name: &str, cx: &mut Context<Self>) {
        if self.mcp_auth_blocked().is_some() || self.mcp_manage.logging_out.is_some() {
            return;
        }
        self.mcp_manage.logging_out = Some(name.to_owned());
        self.mcp_error = None;
        let _ = self.requests_handle().send(backend::Request::McpAuth(backend::McpAuthCall::Logout(name.to_owned())));
        cx.notify();
    }

    /// A login answered or finished: open the browser on start, close the row on success.
    pub fn mcp_login_update(&mut self, login: McpLogin, cx: &mut Context<Self>) {
        let Some(row) = self.mcp_manage.login.as_mut() else { return };
        // Before the start answer only a pending login is ours: the one it replaced ends as expired.
        let ours = row.login.as_ref().map_or(row.name == login.name && login.state == McpLoginState::Pending, |l| l.login_id == login.login_id);
        if !ours {
            return;
        }
        if row.login.is_none() && login.state == McpLoginState::Pending {
            cx.open_url(&login.authorization_url);
        }
        row.working = false;
        row.error = match login.state {
            McpLoginState::Pending | McpLoginState::Succeeded => None,
            McpLoginState::Failed => Some(login.error.clone().unwrap_or_else(|| "The login failed.".into())),
            McpLoginState::Expired => Some("The login expired. Log in again.".into()),
        };
        if login.state == McpLoginState::Succeeded {
            self.mcp_manage.login = None;
        } else {
            row.login = Some(login);
        }
        cx.notify();
    }

    pub fn mcp_auth_failed(&mut self, why: String, cx: &mut Context<Self>) {
        match self.mcp_manage.login.as_mut().filter(|l| l.working) {
            Some(row) => {
                row.working = false;
                row.error = Some(why);
            }
            None => {
                self.mcp_manage.logging_out = None;
                self.mcp_error = Some(why);
            }
        }
        cx.notify();
    }

    /// The auth badge and Log in / Log out of one server row; nothing on older servers.
    pub fn mcp_auth_controls(&self, s: &McpServerInfo, i: usize, supported: bool, cx: &mut Context<Self>) -> Option<Div> {
        let (text, color) = match s.auth {
            _ if !supported => return None,
            McpAuthStatus::None => return None,
            McpAuthStatus::Authorized => ("Connected", theme::ACCENT_TEXT),
            McpAuthStatus::NeedsLogin => ("Needs login", theme::MANAGER_MARK),
            McpAuthStatus::Unknown => ("Unknown", theme::MUTED),
        };
        let allowed = self.mcp_auth_blocked().is_none();
        let name = s.name.clone();
        let badge = div().flex_none().px(px(8.)).py(px(1.)).rounded(px(5.)).border_1().border_color(c(theme::BORDER)).text_size(px(11.5)).text_color(c(color)).child(text);
        let busy = self.mcp_manage.login.as_ref().is_some_and(|l| l.name == s.name) || self.mcp_manage.logging_out.as_deref() == Some(s.name.as_str());
        let action = if busy {
            None
        } else if s.auth == McpAuthStatus::Authorized {
            Some(ui::button(("mcp-logout", i), "Log out", ButtonKind::Outline, None).h(px(28.)).when(!allowed, |el| el.opacity(0.45))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.logout_mcp(&name, cx))))
        } else {
            Some(ui::button(("mcp-login", i), "Log in", ButtonKind::Outline, None).h(px(28.)).when(!allowed, |el| el.opacity(0.45))
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.start_mcp_login(&name, window, cx))))
        };
        Some(h().gap(px(8.)).items_center().flex_none().child(badge).children(action)
            .when(self.mcp_manage.logging_out.as_deref() == Some(s.name.as_str()), |el| el.child(div().text_color(c(theme::MUTED)).child("Logging out…"))))
    }

    /// Under a server row while its login runs: Open again, the paste field (server on
    /// another computer), Cancel and the error.
    pub fn mcp_login_panel(&self, s: &McpServerInfo, cx: &mut Context<Self>) -> Option<Div> {
        let row = self.mcp_manage.login.as_ref().filter(|l| l.name == s.name)?;
        let pending = row.login.as_ref().filter(|l| l.state == McpLoginState::Pending);
        let remote = matches!(&self.connection, Connection::Online { server, .. } if server.remote);
        let hint = |t: &'static str| div().text_size(px(11.5)).line_height(px(17.)).text_color(c(theme::MUTED)).child(t);
        let status = match (row.working, pending.is_some()) {
            (true, false) => "Starting the login…",
            (true, true) => "Finishing the login…",
            (false, true) if remote => "Log in in the browser. It then ends on a page that does not load: copy that page's address and paste it here.",
            (false, true) => "Log in in the browser that opened; this updates by itself when you are done.",
            (false, false) => "",
        };
        let url = pending.map(|l| l.authorization_url.clone());
        Some(
            v().gap(px(8.)).ml(px(142.)).px(px(12.)).py(px(10.)).rounded(px(8.)).border_1().border_color(c(theme::BORDER)).bg(c(theme::SURFACE)).text_size(px(12.5))
                .when(!status.is_empty(), |el| el.child(hint(status)))
                .when(remote && pending.is_some(), |el| {
                    el.child(
                        h().gap(px(6.)).items_center()
                            .child(div().flex_1().min_w_0().child(ui::input(&row.paste)))
                            .child(ui::button("mcp-login-paste", "Finish login", ButtonKind::Primary, None).h(px(32.)).when(row.working, |el| el.opacity(0.45))
                                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.submit_mcp_paste(cx)))),
                    )
                })
                .when_some(row.error.clone(), |el, why| el.child(div().text_color(c(theme::NEGATIVE)).child(why)))
                .child(
                    h().gap(px(8.))
                        .when_some(url, |el, url| {
                            el.child(ui::button("mcp-login-open", "Open again", ButtonKind::Outline, None).h(px(28.))
                                .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.open_url(&url))))
                        })
                        .when(pending.is_none() && !row.working, |el| {
                            let name = row.name.clone();
                            el.child(ui::button("mcp-login-retry", "Log in again", ButtonKind::Outline, None).h(px(28.))
                                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.start_mcp_login(&name, window, cx))))
                        })
                        .child(ui::button("mcp-login-cancel", "Cancel", ButtonKind::Outline, None).h(px(28.)).when(row.working, |el| el.opacity(0.45))
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                // The server lets an unfinished login expire by itself (10 minutes).
                                if this.mcp_manage.login.as_ref().is_some_and(|l| !l.working) {
                                    this.mcp_manage.login = None;
                                    cx.notify();
                                }
                            }))),
                ),
        )
    }

    fn pair_editor(&self, id: &'static str, rows: &[PairRow], add_label: &'static str, headers: bool, cx: &mut Context<Self>) -> Div {
        v().gap(px(6.))
            .children(rows.iter().enumerate().map(|(i, r)| {
                h().gap(px(6.)).items_start()
                    .child(div().w(px(150.)).flex_none().child(ui::input(&r.key)))
                    .child(div().flex_1().min_w_0().child(ui::input(&r.value)))
                    .child(ui::button((id, i), "✕", ButtonKind::Outline, None).h(px(36.)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        if let Some(add) = this.mcp_manage.add.as_mut().filter(|a| !a.working) {
                            let list = if headers { &mut add.headers } else { &mut add.env };
                            if i < list.len() {
                                list.remove(i);
                            }
                        }
                        cx.notify();
                    })))
            }))
            .child(ui::button((id, 1000usize), add_label, ButtonKind::Outline, None).h(px(28.)).on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                let row = if headers { Self::pair_row(window, cx, "Header", "Value (hidden)") } else { Self::pair_row(window, cx, "NAME", "Value (hidden)") };
                if let Some(add) = this.mcp_manage.add.as_mut().filter(|a| !a.working) {
                    row.key.update(cx, |i, cx| i.focus(window, cx));
                    if headers {
                        add.headers.push(row)
                    } else {
                        add.env.push(row)
                    }
                }
                cx.notify();
            })))
    }

    pub fn render_add_mcp(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(add) = self.mcp_manage.add.as_ref() else { return div().into_any_element() };
        let (remote, working, error, enable) = (add.remote, add.working, add.error.clone(), add.enable);
        let (name, command, args, url) = (add.name.clone(), add.command.clone(), add.args.clone(), add.url.clone());
        let top = 90.;
        let max_h = (f32::from(window.viewport_size().height) - top - 24.).max(160.);
        let max_w = (f32::from(window.viewport_size().width) - 32.).max(280.);
        let scroll = window.use_keyed_state("add-mcp-scroll", cx, |_, _| ScrollHandle::default()).read(cx).clone();
        let blocked = self.mcp_manage_blocked();
        let label = |t: &'static str| div().text_size(px(12.)).text_color(c(theme::TEXT_2)).child(t);
        let label_info = |t: &'static str, about: &'static str| div().text_size(px(12.)).text_color(c(theme::TEXT_2)).child(ui::label_with_info(t, about));

        let tabs = h().gap(px(6.)).children([(false, "Local command"), (true, "Remote URL")].into_iter().enumerate().map(|(i, (is_remote, text))| {
            let on = remote == is_remote;
            h().id(("add-mcp-transport", i)).role(Role::Tab).h(px(28.)).px(px(12.)).rounded(px(6.)).cursor_pointer().text_size(px(12.5)).border_1()
                .border_color(c(if on { theme::ACCENT } else { theme::BORDER }))
                .when(on, |el| el.bg(c(theme::SURFACE_2)))
                .text_color(c(if on { theme::TEXT } else { theme::TEXT_2 }))
                .child(text)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    if let Some(add) = this.mcp_manage.add.as_mut().filter(|a| !a.working) {
                        add.remote = is_remote;
                        add.error = None;
                    }
                    cx.notify();
                }))
        }));

        let local = {
            let env = self.mcp_manage.add.as_ref().map(|a| &a.env[..]).unwrap_or(&[]);
            v().gap(px(8.))
                .child(label("Command"))
                .child(ui::input(&command))
                .child(label_info("Arguments", "Separated by spaces; use quotes for an argument with spaces."))
                .child(ui::input(&args))
                .child(label_info("Environment variables", "Values are hidden and are never shown again after you add the server."))
                .child(self.pair_editor("add-mcp-env", env, "Add variable", false, cx))
        };
        let remote_part = {
            let headers = self.mcp_manage.add.as_ref().map(|a| &a.headers[..]).unwrap_or(&[]);
            v().gap(px(8.))
                .child(label_info("URL", OAUTH_NOTE))
                .child(ui::input(&url))
                .child(label_info("Headers", "Values are hidden and are never shown again after you add the server."))
                .child(self.pair_editor("add-mcp-header", headers, "Add header", true, cx))
        };

        let body = v().gap(px(14.))
            .child(h().gap(px(8.)).items_center().child(div().font_family(ui::serif(cx)).text_size(px(22.)).child("Add MCP server")).child(ui::info("add-mcp-about", "Adds the server to Claude Code on the server PC (user scope).")))
            .when_some(blocked.clone(), |el, why| el.child(div().text_size(px(12.5)).text_color(c(theme::MUTED)).child(why)))
            .child(v().gap(px(6.)).child(label("Name")).child(ui::input(&name)))
            .child(tabs)
            .child(if remote { remote_part } else { local })
            .child(
                h().id("add-mcp-enable").role(Role::CheckBox).gap(px(8.)).items_center().cursor_pointer().text_size(px(13.))
                    .child(div().text_color(c(if enable { theme::ACCENT_TEXT } else { theme::MUTED })).child(if enable { "☑" } else { "☐" }))
                    .child("Turn on now")
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        if let Some(add) = this.mcp_manage.add.as_mut().filter(|a| !a.working) {
                            add.enable = !add.enable;
                        }
                        cx.notify();
                    })),
            )
            .when(working, |el| el.child(div().text_size(px(12.5)).text_color(c(theme::TEXT_2)).child("Adding the server…")))
            .when_some(error, |el, why| el.child(div().px(px(12.)).py(px(10.)).rounded(px(8.)).border_1().border_color(c(theme::NEGATIVE)).text_size(px(12.5)).text_color(c(theme::NEGATIVE)).child(why)));

        let can_add = blocked.is_none() && !working;
        let card = crate::screens::overlays::card(560.)
            .child(ui::scroll_frame(&scroll, crate::screens::overlays::scroll_body("add-mcp-body", &scroll, (max_h - 70.).max(120.)).p(px(22.)).child(body)))
            .child(
                h().px(px(22.)).py(px(14.)).gap(px(8.)).justify_end().border_t_1().border_color(c(theme::BORDER))
                    .child(ui::button("add-mcp-cancel", "Cancel", ButtonKind::Outline, Some(ui::kbd("escape", cx))).when(working, |el| el.opacity(0.45)).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.close_add_mcp(cx);
                    })))
                    .child(ui::button("add-mcp-submit", if working { "Adding…" } else { "Add server" }, ButtonKind::Primary, None).when(!can_add, |el| el.opacity(0.45)).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.submit_add_mcp(cx)))),
            );
        deferred(
            div().id("add-mcp-scrim").absolute().top_0().left_0().size_full().bg(Hsla { a: 0.55, ..c(0x000000) }).flex().justify_center().items_start().pt(px(top)).occlude()
                .key_context(LOAD)
                .child(v().max_h(px(max_h)).max_w(px(max_w)).on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()).child(card)),
        )
        .with_priority(2)
        .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{McpAddParams, McpAuthStatus, McpServerInfo, McpServers, McpTransport, login_supported, scrub, split_args};

    #[test]
    fn login_is_hidden_when_a_stdio_server_has_no_auth_state() {
        let server = |transport, auth| McpServerInfo { name: "s".into(), scope: aurelius_protocol::McpScope::User, transport, summary: String::new(), enabled: true, auth };
        let list = |servers| McpServers { servers, missing: vec![] };
        assert!(login_supported(&list(vec![server(McpTransport::Stdio, McpAuthStatus::None), server(McpTransport::Http, McpAuthStatus::Unknown)])));
        assert!(!login_supported(&list(vec![server(McpTransport::Stdio, McpAuthStatus::Unknown), server(McpTransport::Http, McpAuthStatus::Unknown)])));
    }

    #[test]
    fn splits_arguments_like_a_shell() {
        assert_eq!(split_args("-y  @scope/pkg"), ["-y", "@scope/pkg"]);
        assert_eq!(split_args("--dir \"my folder\" 'a b' x"), ["--dir", "my folder", "a b", "x"]);
        assert!(split_args("   ").is_empty());
    }

    #[test]
    fn scrub_hides_secrets_from_errors() {
        let p = McpAddParams { name: "n".into(), transport: McpTransport::Stdio, command: Some("x".into()), args: vec![], env: [("K".to_string(), "sekret123".to_string())].into(), url: None, headers: Default::default(), enable: true };
        assert_eq!(scrub("bad value sekret123 here", &p), "bad value ••• here");
    }
}
