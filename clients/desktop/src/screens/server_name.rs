//! Settings → Server & PC: the server's name (feature `server_name`,
//! docs/product/servernaam). A board member with the owner right chooses it;
//! empty puts the computer's name back. Every place that names the server
//! follows `server.renamed` without reconnecting, and `servers.json` keeps it.

use crate::app::AppView;
use crate::backend::{self, Connection, ServerInfo, ServerNameCall, ServerNameUpdate};
use crate::theme::{self, c};
use crate::ui::{self, ButtonKind, h, v};
use aurelius_domain::server::ServerNameView;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

/// What Settings shows of the server's name.
#[derive(Debug, Default)]
pub struct ServerName {
    /// `server.get_name` as last read; none before the first answer.
    pub view: Option<ServerNameView>,
    /// The name field with Save and Cancel is open.
    pub editing: bool,
    /// A `server.set_name` is on its way.
    pub saving: bool,
    /// Why the last read or save failed, in the server's words (shown inline).
    pub error: Option<String>,
}

/// The edit button shows only when the server has the feature, a board member
/// is signed in and the connection has the owner right (always on this computer).
pub fn can_rename(server: &ServerInfo, signed_in: bool) -> bool {
    server.server_name && signed_in && server.owner
}

/// "Default: ‹computer name›", only when a board member chose a name.
pub fn default_line(view: &ServerNameView) -> Option<String> {
    view.chosen.as_ref().map(|_| format!("Default: {}", view.computer_name))
}

/// The hint under the name field: empty saves the computer's name.
pub fn empty_hint(view: Option<&ServerNameView>) -> String {
    match view {
        Some(view) => format!("Leave it empty to use the computer's name ({}). Enter saves · Esc cancels", view.computer_name),
        None => "Leave it empty to use the computer's name. Enter saves · Esc cancels".into(),
    }
}

impl AppView {
    /// The server's info while online.
    fn online_server(&self) -> Option<&ServerInfo> {
        match &self.connection {
            Connection::Online { server, .. } => Some(server),
            _ => None,
        }
    }

    /// Whether this window may choose the server's name now.
    pub fn server_rename_allowed(&self) -> bool {
        self.online_server().is_some_and(|s| can_rename(s, self.active_member().is_some()))
    }

    /// Online: keep the hello's name (SN-02 AC2) and read where it comes from.
    /// An older server has no `server.get_name`, so it is not asked (SN-02 AC4).
    pub fn server_name_online(&mut self, cx: &mut Context<Self>) {
        let Some(server) = self.online_server().cloned() else { return };
        self.server_name_shown(&server.host_name, cx);
        if server.server_name {
            let _ = self.requests.send(backend::Request::ServerName(ServerNameCall::Load));
        }
    }

    /// A new backend: nothing of the previous server's name stays.
    pub fn server_name_reset(&mut self) {
        self.server_name = ServerName::default();
    }

    /// The server gave a name (its hello, `server.renamed` or a save): every
    /// place that names it shows it, and `servers.json` keeps it by host id.
    fn server_name_shown(&mut self, name: &str, cx: &mut Context<Self>) {
        let Connection::Online { server, .. } = &mut self.connection else { return };
        server.host_name = name.to_owned();
        let (host_id, local) = (server.host_id, !server.remote);
        if let Some((id, shown)) = self.remote_host.as_mut()
            && Some(*id) == host_id
        {
            *shown = name.to_owned();
        }
        if let (Some(root), Some(host_id)) = (self.data_root.clone(), host_id) {
            match aurelius_client::servers::Servers::remember_name(&root, host_id, name, local) {
                Ok(servers) => self.servers = servers,
                Err(e) => self.notify_user(format!("Could not remember the server's name: {e}"), true, cx),
            }
        }
        cx.notify();
    }

    /// The name's answers from the backend.
    pub fn apply_server_name(&mut self, update: ServerNameUpdate, cx: &mut Context<Self>) {
        match update {
            ServerNameUpdate::Loaded(Ok(view)) => {
                let name = view.name.clone();
                self.server_name.view = Some(view);
                if !self.server_name.editing {
                    self.server_name.error = None;
                }
                self.server_name_shown(&name, cx);
            }
            ServerNameUpdate::Loaded(Err(e)) => self.server_name.error = Some(e),
            ServerNameUpdate::Saved(Ok(name)) => {
                self.server_name.saving = false;
                self.server_name.editing = false;
                self.server_name.error = None;
                self.server_name_shown(&name, cx);
                // Whether a name is chosen now; `server.renamed` follows only when it changed.
                let _ = self.requests.send(backend::Request::ServerName(ServerNameCall::Load));
            }
            ServerNameUpdate::Saved(Err(e)) => {
                self.server_name.saving = false;
                self.server_name.error = Some(e);
            }
        }
        cx.notify();
    }

    /// "Rename…" (F2): the name field, with the chosen name in it.
    pub fn server_name_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.server_rename_allowed() {
            return;
        }
        let chosen = self.server_name.view.as_ref().and_then(|v| v.chosen.clone()).unwrap_or_default();
        self.server_name.editing = true;
        self.server_name.error = None;
        self.server_name_input.update(cx, |input, cx| {
            input.set_value(chosen, window, cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    /// Enter or Save: send the name; empty puts the computer's name back.
    pub fn server_name_save(&mut self, cx: &mut Context<Self>) {
        if !self.server_name.editing || self.server_name.saving {
            return;
        }
        let name = self.server_name_input.read(cx).value().to_string();
        self.server_name.saving = true;
        self.server_name.error = None;
        let _ = self.requests.send(backend::Request::ServerName(ServerNameCall::Set(name)));
        cx.notify();
    }

    /// Esc or Cancel: close the field; the name stays as it was. False when it was not open.
    pub fn server_name_cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self.server_name.editing {
            return false;
        }
        self.server_name.editing = false;
        self.server_name.saving = false;
        self.server_name.error = None;
        self.focus.focus(window, cx);
        cx.notify();
        true
    }

    /// Settings → Server & PC opens with the server's name: always when the
    /// server has the feature, else only for another computer (as before).
    pub fn server_name_section(&self) -> bool {
        self.online_server().is_some_and(|s| s.server_name || s.remote)
    }

    /// The NAME section on top of Settings → Server & PC.
    pub fn render_server_name(&self, cx: &mut Context<Self>) -> Div {
        let Some(server) = self.online_server() else { return div() };
        let label = |text: &'static str| div().w(px(130.)).flex_none().text_color(c(theme::MUTED)).child(text);
        if !server.server_name {
            // An older server: its name as before, nothing to change (SN-02 AC4).
            return match server.remote {
                true => h().gap(px(12.)).items_start().text_size(px(13.)).child(label("Server")).child(div().flex_1().child(server.host_name.clone())),
                false => div(),
            };
        }
        let state = &self.server_name;
        let name = state.view.as_ref().map_or_else(|| server.host_name.clone(), |v| v.name.clone());
        let default = state.view.as_ref().and_then(default_line);
        let shown = h().gap(px(12.)).items_start().text_size(px(13.))
            .child(label("Name"))
            .child(
                v().flex_1().min_w_0().gap(px(2.))
                    .child(div().child(name))
                    .when_some(default, |el, d| el.child(div().text_size(px(11.5)).text_color(c(theme::MUTED)).child(d))),
            )
            .when(!state.editing && self.server_rename_allowed(), |el| {
                el.child(ui::button("server-rename", "Rename…", ButtonKind::Outline, Some(ui::kbd("f2", cx)))
                    .on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.server_name_edit(w, cx))))
            });
        let editor = state.editing.then(|| {
            let hint = match &state.error {
                Some(e) => div().text_size(px(12.)).text_color(c(theme::NEGATIVE)).child(e.clone()),
                None => div().text_size(px(11.5)).text_color(c(theme::MUTED)).child(empty_hint(state.view.as_ref())),
            };
            h().gap(px(12.)).items_start().text_size(px(13.))
                .child(div().w(px(130.)).flex_none())
                .child(
                    v().flex_1().min_w_0().gap(px(6.)).key_context("SettingsKey")
                        .child(ui::input(&self.server_name_input))
                        .child(hint)
                        .child(
                            h().gap(px(8.))
                                .child(ui::button("server-name-save", if state.saving { "Saving…" } else { "Save" }, ButtonKind::Primary, None)
                                    .when(state.saving, |el| el.opacity(0.45))
                                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.server_name_save(cx))))
                                .child(ui::button("server-name-cancel", "Cancel", ButtonKind::Outline, None)
                                    .on_click(cx.listener(|this, _: &ClickEvent, w, cx| {
                                        this.server_name_cancel(w, cx);
                                    }))),
                        ),
                )
        });
        // A read that failed shows where the name is (the field shows its own errors).
        let read_error = state.error.clone().filter(|_| !state.editing);
        v().gap(px(8.))
            .child(ui::eyebrow("NAME"))
            .child(shown)
            .children(editor)
            .when_some(read_error, |el, e| el.child(div().text_size(px(12.)).text_color(c(theme::NEGATIVE)).child(e)))
    }
}

#[cfg(test)]
mod tests {
    use super::{can_rename, default_line, empty_hint};
    use crate::backend::ServerInfo;
    use aurelius_domain::server::ServerNameView;

    fn server(feature: bool, owner: bool) -> ServerInfo {
        ServerInfo { server_name: feature, owner, ..ServerInfo::default() }
    }

    #[test]
    fn the_edit_button_needs_the_feature_a_board_member_and_the_owner_right() {
        assert!(can_rename(&server(true, true), true));
        assert!(!can_rename(&server(false, true), true), "an older server without server_name");
        assert!(!can_rename(&server(true, true), false), "nobody signed in");
        assert!(!can_rename(&server(true, false), true), "a remote device without the owner right");
        assert!(!can_rename(&server(false, false), false));
    }

    #[test]
    fn the_default_shows_only_when_a_name_is_chosen() {
        let chosen = ServerNameView::of(Some("Kantoor".into()), "desktop-1".into(), 2);
        assert_eq!(default_line(&chosen).as_deref(), Some("Default: desktop-1"));
        assert_eq!(default_line(&ServerNameView::of(None, "desktop-1".into(), 0)), None);
    }

    #[test]
    fn the_hint_says_that_empty_brings_the_computers_name_back() {
        let view = ServerNameView::of(None, "desktop-1".into(), 0);
        assert!(empty_hint(Some(&view)).starts_with("Leave it empty to use the computer's name (desktop-1)."));
        assert!(empty_hint(None).contains("Leave it empty"));
    }
}
