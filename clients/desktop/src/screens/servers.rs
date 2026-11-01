//! One server, many clients: Settings → Servers picks which server
//! this app uses (the service on this computer, or one paired over
//! Tailscale), and the folder browser picks folders *on that server*, also
//! when it is this computer (with the system dialog as a fallback there).

use crate::app::{AppView, Overlay};
use crate::backend::{self, PairEvent};
use crate::keymap::*;
use crate::theme::{self, c};
use crate::ui::{self, ButtonKind, h, v};
use aurelius_client::Target;
use aurelius_client::servers::Servers;
use aurelius_domain::{HostId, EmployeeId, ProjectId};
use aurelius_protocol::{DirEntryView, DirListing};
use gpui_kit::component::input::Input;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use gpui_kit::component::scroll::ScrollableElement as _;

pub const SERVERS_TAB: usize = 4;

/// What a picked folder is for.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum FolderPurpose {
    /// The folder field of the New project / Add manager dialog.
    Dialog,
    ProjectFolder(ProjectId),
    ManagerFolder(EmployeeId),
}

/// Why New folder can't be used at the starting points.
pub const NO_PARENT: &str = "Open a folder first: a new folder goes into the folder shown.";

/// The folder browser over `fs/list_dir` (on this computer too).
pub struct FolderBrowser {
    pub purpose: FolderPurpose,
    /// Title of the system dialog, the fallback on this computer.
    pub prompt: &'static str,
    pub listing: Option<DirListing>,
    /// Index into the filtered rows.
    pub selected: usize,
    /// The overlay to return to (a dialog, Settings, or none).
    pub ret: Overlay,
    /// A listing or a new folder is on its way.
    pub loading: bool,
    /// Why the last folder could not be opened (shown in the list).
    pub error: Option<String>,
    /// Type-to-filter on the rows shown.
    pub filter: String,
    /// Ctrl+L: the path field replaces the crumbs.
    pub typing_path: bool,
    pub path_error: Option<String>,
    /// New folder: a name row on top of the list.
    pub naming: bool,
    pub name_error: Option<String>,
    /// Why New folder can't be used (shown above the list).
    pub notice: Option<String>,
    /// A field had the focus and went away: give it back to the list.
    pub refocus: bool,
}

impl FolderBrowser {
    pub fn new(purpose: FolderPurpose, prompt: &'static str, ret: Overlay) -> Self {
        Self {
            purpose,
            prompt,
            listing: None,
            selected: 0,
            ret,
            loading: false,
            error: None,
            filter: String::new(),
            typing_path: false,
            path_error: None,
            naming: false,
            name_error: None,
            notice: None,
            refocus: false,
        }
    }

    /// The rows: folders here, or the starting points, that match the filter.
    fn rows(&self) -> Vec<DirEntryView> {
        let all = match &self.listing {
            Some(l) if l.path.is_some() => l.entries.clone(),
            Some(l) => l.roots.clone(),
            None => Vec::new(),
        };
        filter_rows(all, &self.filter)
    }

    /// What "Use this folder" takes: the folder shown, at the starting points the selected one.
    pub fn target(&self) -> Option<String> {
        let listing = self.listing.as_ref()?;
        match &listing.path {
            Some(path) => Some(path.clone()),
            None => self.rows().get(self.selected).map(|r| r.path.clone()),
        }
    }

    /// Why New folder can't be used here.
    pub fn new_folder_blocked(&self) -> Option<&'static str> {
        match &self.listing {
            Some(l) if l.path.is_some() => None,
            _ => Some(NO_PARENT),
        }
    }
}

/// One row of Settings → Servers. `host_id: None` is this computer.
pub struct ServerRow {
    pub host_id: Option<HostId>,
    pub name: String,
    pub detail: String,
}

impl AppView {
    // ── Which server ────────────────────────────────────────────────────

    pub fn on_open_servers(&mut self, _: &OpenServers, window: &mut Window, cx: &mut Context<Self>) {
        self.open_settings(&OpenSettings, window, cx);
        self.set_settings_tab(SERVERS_TAB, cx);
    }

    pub fn server_rows(&self) -> Vec<ServerRow> {
        let mut rows = Vec::new();
        if backend::local_service_available() {
            // E3: the label stays, the server's name is its detail (live while it is in use).
            let live = match &self.connection {
                backend::Connection::Online { server, .. } if !server.remote => Some(server.host_name.as_str()),
                _ => None,
            };
            let detail = local_detail(live.or(self.servers.local_name.as_deref()));
            rows.push(ServerRow { host_id: None, name: "This computer".into(), detail });
        }
        for s in &self.servers.servers {
            let env = if s.environment == "dev" { " · dev" } else { "" };
            rows.push(ServerRow { host_id: Some(s.host_id), name: s.host_name.clone(), detail: format!("{}{env}", s.url) });
        }
        rows
    }

    pub fn reload_servers(&mut self) {
        if let Some(root) = &self.data_root {
            self.servers = Servers::load(root);
        }
    }

    /// The server in use: `None` = this computer.
    pub fn active_server(&self) -> Option<HostId> {
        self.remote_host.as_ref().map(|(id, _)| *id)
    }

    /// Use another server (`None`: this computer). The window starts over
    /// with that server's data; nothing of the previous one is kept.
    pub fn switch_server(&mut self, choice: Option<HostId>, cx: &mut Context<Self>) {
        let Some(root) = self.data_root.clone() else { return };
        let mut saved = Servers::load(&root);
        let target = match choice {
            None if backend::local_service_available() => Target::Local { root: root.clone(), spawn: true },
            None => {
                self.notify_user("This install has no background service of its own. Pair a server (N).", true, cx);
                return;
            }
            Some(id) => match saved.credentials(id) {
                Ok(server) => Target::Remote(server),
                Err(e) => {
                    self.notify_user(format!("This server can't be used: {e}. Pair it again (N)."), true, cx);
                    return;
                }
            },
        };
        saved.active = choice;
        if let Some(id) = choice {
            saved.touch(id);
        }
        if let Err(e) = saved.save(&root) {
            self.notify_user(format!("Could not remember the choice: {e}"), true, cx);
        }
        self.servers = saved;
        let name = match &target {
            Target::Remote(s) => s.host_name.clone(),
            Target::Local { .. } => "this computer".into(),
        };
        self.remote_host = match &target {
            Target::Remote(s) => Some((s.host_id, s.host_name.clone())),
            Target::Local { .. } => None,
        };
        self.restart_backend(Some(target), cx);
        self.notify_user(format!("Connecting to {name}…"), false, cx);
    }

    pub fn servers_connect(&mut self, cx: &mut Context<Self>) {
        let rows = self.server_rows();
        let Some(row) = rows.get(self.settings_selected.min(rows.len().saturating_sub(1))) else {
            self.notify_user("No server yet. Press N and paste the pairing text from the server.", false, cx);
            return;
        };
        if row.host_id == self.active_server() && !matches!(self.connection, backend::Connection::Unpaired(_) | backend::Connection::NoServer) {
            self.notify_user(format!("Already using {}.", row.name), false, cx);
            return;
        }
        self.switch_server(row.host_id, cx);
    }

    pub fn servers_add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.adding_server = true;
        self.server_input.update(cx, |input, cx| {
            input.set_value("", window, cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    /// Enter in the pairing field: pair as owner, then use that server.
    pub fn servers_submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.server_input.read(cx).value().trim().to_string();
        self.server_input.update(cx, |input, cx| input.set_value("", window, cx));
        self.adding_server = false;
        self.focus.focus(window, cx);
        if text.is_empty() {
            cx.notify();
            return;
        }
        self.pairing_wait = Some(String::new());
        let mut events = backend::pair_server(text, aurelius_host::host_name());
        cx.spawn(async move |this, cx| {
            while let Some(event) = events.recv().await {
                let done = matches!(event, PairEvent::Done(_));
                let _ = this.update(cx, |this, cx| this.pairing_progress(event, cx));
                if done {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
    }

    fn pairing_progress(&mut self, event: PairEvent, cx: &mut Context<Self>) {
        match event {
            PairEvent::Waiting(host) => {
                self.notify_user(format!("Waiting for \"Allow as owner\" on {host} (Settings → Remote access there)…"), false, cx);
                self.pairing_wait = Some(host);
            }
            PairEvent::Done(Ok(server)) => {
                self.pairing_wait = None;
                let Some(root) = self.data_root.clone() else { return };
                let mut saved = Servers::load(&root);
                if let Err(e) = saved.add(&server).and_then(|()| saved.save(&root)) {
                    self.notify_user(format!("Paired, but the pairing could not be stored: {e}"), true, cx);
                    return;
                }
                self.servers = saved;
                self.switch_server(Some(server.host_id), cx);
            }
            PairEvent::Done(Err(e)) => {
                self.pairing_wait = None;
                self.notify_user(e, true, cx);
            }
        }
        cx.notify();
    }

    /// Forget the selected server on this computer. While connected to it,
    /// this computer is also removed there.
    pub fn servers_forget(&mut self, cx: &mut Context<Self>) {
        let rows = self.server_rows();
        let Some(row) = rows.get(self.settings_selected.min(rows.len().saturating_sub(1))) else { return };
        let Some(host_id) = row.host_id else {
            self.notify_user("This computer's own service can't be forgotten.", false, cx);
            return;
        };
        let Some(root) = self.data_root.clone() else { return };
        let active = self.active_server() == Some(host_id);
        if let (true, backend::Connection::Online { server, .. }) = (active, &self.connection)
            && let Some(me) = server.client_id
        {
            let _ = self.requests_handle().send(backend::Request::Remote(backend::RemoteCall::Revoke(me)));
        }
        // Its session on this device goes too (S8): a forgotten server keeps no token here.
        backend::Tokens::os(host_id.to_string()).forget();
        let mut saved = Servers::load(&root);
        let forgotten = saved.forget(host_id).and_then(|()| saved.save(&root));
        self.servers = saved;
        self.settings_selected = 0;
        match forgotten {
            Ok(()) => self.notify_user(format!("{} forgotten on this computer.", row.name), false, cx),
            Err(e) => self.notify_user(format!("Could not forget {}: {e}", row.name), true, cx),
        }
        if active {
            self.remote_host = None;
            let target = backend::local_service_available().then_some(Target::Local { root, spawn: true });
            self.restart_backend(target, cx);
        }
        cx.notify();
    }

    pub fn render_servers_settings(&mut self, page: Div, cx: &mut Context<Self>) -> Div {
        let rows = self.server_rows();
        let selected = self.settings_selected.min(rows.len().saturating_sub(1));
        let active = self.active_server();
        let state = |row: &ServerRow| -> (SharedString, bool) {
            if row.host_id != active {
                return ("".into(), false);
            }
            match &self.connection {
                backend::Connection::Online { .. } => ("in use · connected".into(), true),
                backend::Connection::Connecting => ("in use · connecting…".into(), false),
                backend::Connection::Offline(_) => ("in use · offline, retrying".into(), false),
                backend::Connection::Unpaired(_) => ("removed there · pair again".into(), false),
                backend::Connection::NoServer => ("".into(), false),
                backend::Connection::UpdateNeeded(_) => ("in use · update needed".into(), false),
            }
        };
        page.child(
            h().px(px(22.)).py(px(14.)).border_b_1().border_color(c(theme::BORDER))
                .child(self.settings_title(cx)),
        )
        .child(
            v().id("servers-body").flex_1().min_h_0().overflow_y_scrollbar().p(px(22.)).gap(px(10.))
                .when_some(match &self.connection {
                    backend::Connection::Unpaired(reason) | backend::Connection::UpdateNeeded(reason) => Some(reason.clone()),
                    _ => None,
                }, |el, reason| el.child(div().text_size(px(12.5)).text_color(c(theme::NEGATIVE)).child(reason)))
                .when(rows.is_empty(), |el| el.child(div().py(px(14.)).text_color(c(theme::MUTED)).text_size(px(12.5)).child("No server yet. Press N and paste the pairing text from the server.")))
                .children(rows.iter().enumerate().map(|(i, row)| {
                    let (status, online) = state(row);
                    let using = row.host_id == active;
                    h().id(("server-row", i)).gap(px(12.)).px(px(12.)).py(px(9.)).rounded(px(6.)).cursor_pointer()
                        .when(i == selected, |el| el.bg(c(theme::SURFACE_2)))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.settings_selected = i;
                            cx.notify();
                        }))
                        .child(if online { ui::dot(7., theme::ACCENT) } else if using { ui::ring(7., theme::ACCENT) } else { ui::ring(7., theme::MUTED) })
                        .child(div().w(px(190.)).flex_none().truncate().text_size(px(13.)).child(row.name.clone()))
                        .child(div().flex_1().min_w_0().truncate().text_size(px(11.5)).text_color(c(theme::MUTED)).child(row.detail.clone()))
                        .child(div().text_size(px(11.5)).text_color(c(if online { theme::ACCENT_TEXT } else { theme::MUTED })).child(status))
                }))
                .when_some(self.pairing_wait.clone(), |el, host| {
                    el.child(
                        h().gap(px(10.)).px(px(12.)).py(px(10.)).rounded(px(8.)).bg(c(theme::ATTENTION_BG)).text_color(c(theme::ACCENT_TEXT)).text_size(px(12.5))
                            .child(ui::pulse_dot("pairing-wait", 7., theme::ACCENT))
                            .child(div().flex_1().child(if host.is_empty() {
                                "Pairing…".to_string()
                            } else {
                                format!("Waiting until you choose \"Allow as owner\" on {host}.")
                            })),
                    )
                })
                .when(self.adding_server, |el| {
                    el.child(
                        v().gap(px(6.)).pt(px(6.)).key_context("SettingsKey")
                            .child(h().gap(px(6.)).items_center().text_size(px(12.)).text_color(c(theme::TEXT_2))
                                .child("Paste the pairing text and press Enter.")
                                .child(ui::info("pairing-about", "On the server: Settings → Remote access (Ctrl Alt R) → N, then \"Copy pairing text\".")))
                            .child(Input::new(&self.server_input))
                            .child(div().text_size(px(11.5)).text_color(c(theme::MUTED)).child("A paired computer gets full access to that server.")),
                    )
                })
                ,
        )
        .child(
            h().px(px(22.)).py(px(12.)).gap(px(14.)).border_t_1().border_color(c(theme::BORDER)).text_size(px(11.5)).text_color(c(theme::MUTED))
                .child(ui::key_hint(&["down"], "select", cx))
                .child(ui::hint("srv-use", "enter", "Use this server", || Box::new(SettingsUse), cx))
                .child(ui::hint("srv-add", "n", "Pair a server…", || Box::new(SettingsNew), cx))
                .child(ui::hint("srv-forget", "delete", "Forget", || Box::new(SettingsRemove), cx)),
        )
    }

    // ── Folders on the server ───────────────────────────────────────────

    /// Pick a folder for `purpose` in the folder browser, on this computer as
    /// on another one, so it works the same everywhere and can make folders.
    /// The system dialog stays as a fallback here (and for servers before 1.3).
    pub fn pick_folder(&mut self, purpose: FolderPurpose, prompt: &'static str, window: &mut Window, cx: &mut Context<Self>) {
        self.folder = Some(FolderBrowser::new(purpose, prompt, self.overlay));
        self.overlay = Overlay::ServerFolder;
        let start = match purpose {
            FolderPurpose::ProjectFolder(id) => self.projects.iter().find(|p| p.id == id).and_then(|p| p.folder.clone()),
            FolderPurpose::ManagerFolder(id) => self.managers.iter().find(|m| m.id == id).and_then(|m| m.folder.clone()),
            FolderPurpose::Dialog => self.dialog_folder.clone(),
        };
        self.folder_browse(start, false);
        self.overlay_focus.focus(window, cx);
        cx.notify();
    }

    /// This computer's own folder dialog.
    fn system_folder_dialog(&mut self, purpose: FolderPurpose, prompt: &'static str, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions { files: false, directories: true, multiple: false, prompt: Some(prompt.into()) });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = paths.await else { return };
            let Some(path) = paths.into_iter().next() else { return };
            let _ = this.update(cx, |this, cx| this.folder_chosen(purpose, path.display().to_string(), cx));
        })
        .detach();
    }

    /// "System dialog…": leave the browser for the system dialog (this computer only).
    pub fn folder_system_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(b) = self.folder.as_ref() else { return };
        if self.remote_host.is_some() {
            return;
        }
        let (purpose, prompt) = (b.purpose, b.prompt);
        self.close_folder(window, cx);
        self.system_folder_dialog(purpose, prompt, cx);
    }

    pub fn folder_chosen(&mut self, purpose: FolderPurpose, path: String, cx: &mut Context<Self>) {
        match purpose {
            FolderPurpose::Dialog => self.dialog_folder = Some(path),
            FolderPurpose::ProjectFolder(project_id) => {
                let _ = self.requests_handle().send(backend::Request::SetProjectFolder { project_id, path });
            }
            FolderPurpose::ManagerFolder(employee_id) => {
                let _ = self.requests_handle().send(backend::Request::SetEmployeeFolder { employee_id, path: Some(path) });
            }
        }
        cx.notify();
    }

    /// Ask for a folder's listing (`None`: the starting points).
    fn folder_browse(&mut self, path: Option<String>, typed: bool) {
        let Some(b) = self.folder.as_mut() else { return };
        b.loading = true;
        if !typed {
            b.error = None;
        }
        let _ = self.requests_handle().send(backend::Request::BrowseDir { path, typed });
    }

    /// A crumb or a quick-access entry.
    pub fn folder_go(&mut self, path: Option<String>, cx: &mut Context<Self>) {
        self.folder_browse(path, false);
        cx.notify();
    }

    pub fn apply_dir_listing(&mut self, listing: DirListing) {
        let Some(b) = self.folder.as_mut() else { return };
        // Going up selects the folder we came from.
        let came_from = b.listing.as_ref().and_then(|l| l.path.clone());
        let shown = if listing.path.is_some() { &listing.entries } else { &listing.roots };
        b.selected = came_from.and_then(|p| shown.iter().position(|e| e.path == p)).unwrap_or(0);
        // A field that had the focus goes away: the list gets it back.
        b.refocus |= b.typing_path || b.naming;
        b.listing = Some(listing);
        b.loading = false;
        b.error = None;
        b.filter.clear();
        b.typing_path = false;
        b.path_error = None;
        b.naming = false;
        b.name_error = None;
        b.notice = None;
    }

    /// A request of the browser failed: the error shows where it was asked.
    pub fn apply_dir_failed(&mut self, ask: backend::DirAsk, message: String, cx: &mut Context<Self>) {
        let Some(b) = self.folder.as_mut() else {
            self.notify_user(message, true, cx);
            return;
        };
        b.loading = false;
        match ask {
            backend::DirAsk::Browse => b.error = Some(message),
            backend::DirAsk::Typed => b.path_error = Some(message),
            backend::DirAsk::Create => b.name_error = Some(message),
        }
    }

    pub fn folder_move(&mut self, down: bool, cx: &mut Context<Self>) {
        if let Some(b) = self.folder.as_mut() {
            let max = b.rows().len().saturating_sub(1);
            b.selected = if down { (b.selected + 1).min(max) } else { b.selected.saturating_sub(1) };
            cx.notify();
        }
    }

    /// Into the selected folder.
    pub fn folder_open(&mut self, cx: &mut Context<Self>) {
        let Some(b) = self.folder.as_ref() else { return };
        if let Some(row) = b.rows().get(b.selected) {
            let path = row.path.clone();
            self.folder_browse(Some(path), false);
        }
        cx.notify();
    }

    /// One level up; above a drive or the home folder: the starting points.
    pub fn folder_parent(&mut self, cx: &mut Context<Self>) {
        let Some(listing) = self.folder.as_ref().and_then(|b| b.listing.as_ref()) else { return };
        if listing.path.is_none() {
            return;
        }
        let parent = listing.parent.clone();
        self.folder_browse(parent, false);
        cx.notify();
    }

    /// Backspace: shorten the filter, or go up when there is none.
    pub fn folder_erase(&mut self, cx: &mut Context<Self>) {
        if let Some(b) = self.folder.as_mut().filter(|b| !b.filter.is_empty()) {
            b.filter.pop();
            b.selected = 0;
            cx.notify();
            return;
        }
        self.folder_parent(cx);
    }

    /// A typed character filters the list (when the list has the focus).
    fn folder_type(&mut self, keystroke: &Keystroke, cx: &mut Context<Self>) -> bool {
        let m = &keystroke.modifiers;
        if m.control || m.alt || m.platform || m.function {
            return false;
        }
        let Some(text) = keystroke.key_char.as_deref().filter(|t| !t.is_empty() && !t.chars().any(char::is_control)) else { return false };
        let Some(b) = self.folder.as_mut() else { return false };
        if b.filter.is_empty() && text.trim().is_empty() {
            return false;
        }
        b.filter.push_str(text);
        b.selected = 0;
        cx.notify();
        true
    }

    /// Esc: close the open field, then clear the filter, then close the browser.
    pub fn folder_back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(b) = self.folder.as_mut() else {
            self.close_folder(window, cx);
            return;
        };
        if b.naming || b.typing_path {
            b.naming = false;
            b.name_error = None;
            b.typing_path = false;
            b.path_error = None;
            self.overlay_focus.focus(window, cx);
            cx.notify();
            return;
        }
        if !b.filter.is_empty() {
            b.filter.clear();
            b.selected = 0;
            cx.notify();
            return;
        }
        self.close_folder(window, cx);
    }

    /// Ctrl+L: type or paste a path.
    pub fn folder_type_path(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(b) = self.folder.as_mut() else { return };
        b.naming = false;
        b.name_error = None;
        b.typing_path = true;
        b.path_error = None;
        let current = b.listing.as_ref().and_then(|l| l.path.clone()).unwrap_or_default();
        self.folder_path_input.update(cx, |input, cx| {
            input.set_value(current, window, cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    /// Enter in the path field.
    pub fn folder_go_typed(&mut self, cx: &mut Context<Self>) {
        let text = self.folder_path_input.read(cx).value().to_string();
        let path = typed_path(&text);
        let Some(b) = self.folder.as_mut() else { return };
        if path.is_empty() {
            b.path_error = Some("Type or paste the path of a folder.".into());
            cx.notify();
            return;
        }
        b.path_error = None;
        self.folder_browse(Some(path), true);
        cx.notify();
    }

    /// New folder (Ctrl+Shift+N): a name row on top of the list.
    pub fn folder_new(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(b) = self.folder.as_mut() else { return };
        if let Some(why) = b.new_folder_blocked() {
            b.notice = Some(why.into());
            cx.notify();
            return;
        }
        b.typing_path = false;
        b.path_error = None;
        b.naming = true;
        b.name_error = None;
        b.notice = None;
        b.filter.clear();
        self.folder_name_input.update(cx, |input, cx| {
            input.set_value("", window, cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    /// Enter in the name field: create it, then the browser opens into it.
    pub fn folder_create(&mut self, cx: &mut Context<Self>) {
        let name = self.folder_name_input.read(cx).value().trim().to_string();
        let Some(b) = self.folder.as_mut() else { return };
        let Some(parent) = b.listing.as_ref().and_then(|l| l.path.clone()) else { return };
        if name.is_empty() {
            b.name_error = Some("Type a name for the new folder.".into());
            cx.notify();
            return;
        }
        if b.loading {
            return;
        }
        b.name_error = None;
        b.loading = true;
        let _ = self.requests_handle().send(backend::Request::CreateDir { parent, name });
        cx.notify();
    }

    /// Typing a new name hides the previous error.
    pub fn folder_name_changed(&mut self, cx: &mut Context<Self>) {
        if let Some(b) = self.folder.as_mut().filter(|b| b.name_error.is_some()) {
            b.name_error = None;
            cx.notify();
        }
    }

    /// Use the folder being shown (at the starting points: the selected one).
    pub fn folder_use(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(b) = self.folder.as_ref() else { return };
        let Some(path) = b.target() else { return };
        let purpose = b.purpose;
        self.close_folder(window, cx);
        self.folder_chosen(purpose, path, cx);
    }

    /// Back to where the browser was opened from.
    pub fn close_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ret = self.folder.take().map_or(Overlay::None, |b| b.ret);
        self.overlay = ret;
        match ret {
            Overlay::Dialog(_) => self.name_input.update(cx, |input, cx| input.focus(window, cx)),
            Overlay::None => self.focus.focus(window, cx),
            _ => self.overlay_focus.focus(window, cx),
        }
        cx.notify();
    }

    /// The folders that projects and managers already work in.
    fn folders_in_use(&self) -> Vec<DirEntryView> {
        let paths = self.projects.iter().filter_map(|p| p.folder.clone()).chain(self.managers.iter().filter_map(|m| m.folder.clone()));
        folders_in_use(paths)
    }

    pub fn render_server_folder(&mut self, max_h: f32, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let host = self.remote_host.as_ref().map(|(_, n)| n.clone()).unwrap_or_else(|| "this computer".into());
        let local = self.remote_host.is_none();
        let in_use = self.folders_in_use();
        let mono = ui::mono(cx);
        let Some(b) = self.folder.as_mut() else { return div() };
        if std::mem::take(&mut b.refocus) {
            let focus = self.overlay_focus.clone();
            window.defer(cx, move |window, cx| focus.focus(window, cx));
        }
        let Some(b) = self.folder.as_ref() else { return div() };
        let rows = b.rows();
        let selected = b.selected.min(rows.len().saturating_sub(1));
        let path = b.listing.as_ref().and_then(|l| l.path.clone());
        let at_roots = b.listing.as_ref().is_some_and(|l| l.path.is_none());
        let truncated = b.listing.as_ref().is_some_and(|l| l.truncated);
        let roots = b.listing.as_ref().map(|l| l.roots.clone()).unwrap_or_default();
        let target = b.target();
        let blocked = b.new_folder_blocked();
        let (loading, listed, filter, typing_path, naming) = (b.loading, b.listing.is_some(), b.filter.clone(), b.typing_path, b.naming);
        let (error, path_error, name_error, notice) = (b.error.clone(), b.path_error.clone(), b.name_error.clone(), b.notice.clone());
        let list_h = (max_h - 250.).clamp(200., 440.);

        // Quick access: the starting points, then the folders in use.
        let quick = |id: &'static str, i: usize, entry: &DirEntryView, cx: &mut Context<Self>| {
            let here = path.as_deref() == Some(entry.path.as_str());
            let go = entry.path.clone();
            h().id((id, i)).gap(px(8.)).px(px(10.)).py(px(5.)).rounded(px(5.)).cursor_pointer()
                .text_size(px(12.5))
                .text_color(c(if here { theme::TEXT } else { theme::TEXT_2 }))
                .when(here, |el| el.bg(c(theme::SURFACE_2)))
                .hover(|s| s.bg(c(theme::SURFACE_2)).text_color(c(theme::TEXT)))
                .tooltip(ui::tip(entry.path.clone(), None))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.folder_go(Some(go.clone()), cx)))
                .child(ui::folder_glyph(if here { theme::ACCENT } else { theme::MUTED }))
                .child(div().flex_1().min_w_0().truncate().child(entry.name.clone()))
        };
        let quick_access = v().id("folder-quick").w(px(196.)).flex_none().h(px(list_h)).overflow_y_scrollbar().p(px(8.)).gap(px(1.)).border_r_1().border_color(c(theme::BORDER))
            .child(div().px(px(10.)).pt(px(4.)).pb(px(4.)).child(ui::eyebrow("STARTING POINTS")))
            .children(roots.iter().enumerate().map(|(i, r)| quick("folder-root", i, r, cx)))
            .when(!in_use.is_empty(), |el| {
                el.child(div().px(px(10.)).pt(px(12.)).pb(px(4.)).child(ui::eyebrow("IN USE")))
                    .children(in_use.iter().enumerate().map(|(i, r)| quick("folder-in-use", i, r, cx)))
            });

        // The path: crumbs to click, or the typed path (Ctrl+L).
        let path_bar = if typing_path {
            v().flex_1().min_w_0().gap(px(4.))
                .child(ui::input(&self.folder_path_input))
                .child(match path_error {
                    Some(e) => div().text_size(px(12.)).text_color(c(theme::NEGATIVE)).child(e),
                    None => div().text_size(px(11.5)).text_color(c(theme::MUTED)).child("Enter goes there · Esc cancels"),
                })
        } else {
            let crumbs = path.as_deref().map(crumbs).unwrap_or_default();
            let last = crumbs.len();
            h().flex_1().min_w_0().flex_wrap().gap(px(2.)).font_family(mono.clone()).text_size(px(12.))
                .child(crumb("folder-crumb-start", "Starting points".into(), last == 0).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.folder_go(None, cx))))
                .children(crumbs.into_iter().enumerate().map(|(i, (name, to))| {
                    h().gap(px(2.))
                        .child(div().text_color(c(theme::MUTED)).child("›"))
                        .child(crumb(("folder-crumb", i), name, i + 1 == last).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.folder_go(Some(to.clone()), cx))))
                }))
        };

        let row = |i: usize, entry: &DirEntryView, cx: &mut Context<Self>| {
            h().id(("folder-row", i)).gap(px(10.)).px(px(12.)).py(px(7.)).rounded(px(6.)).cursor_pointer()
                .when(i == selected, |el| el.bg(c(theme::SURFACE_2)))
                .hover(|s| s.bg(c(theme::SURFACE_2)))
                .on_click(cx.listener(move |this, e: &ClickEvent, _, cx| {
                    if let Some(b) = this.folder.as_mut() {
                        b.selected = i;
                    }
                    if e.click_count() >= 2 {
                        this.folder_open(cx);
                    }
                    cx.notify();
                }))
                .child(ui::folder_glyph(if i == selected { theme::ACCENT } else { theme::MUTED }))
                .child(div().flex_1().min_w_0().truncate().text_size(px(13.)).child(entry.name.clone()))
        };

        let empty = if !filter.is_empty() {
            format!("No folder matches \"{filter}\".")
        } else if at_roots {
            "No starting points on this server.".into()
        } else {
            "No folders here.".into()
        };
        let list = v().id("folder-list").flex_1().min_w_0().h(px(list_h)).overflow_y_scrollbar().p(px(8.)).gap(px(1.))
            .when(naming, |el| {
                el.child(
                    h().items_start().gap(px(10.)).px(px(12.)).py(px(6.)).mb(px(4.)).rounded(px(6.)).bg(c(theme::ATTENTION_BG)).border_1().border_color(c(theme::ATTENTION_BORDER))
                        .child(div().pt(px(13.)).child(ui::folder_glyph(theme::ACCENT)))
                        .child(
                            v().flex_1().min_w_0().gap(px(4.))
                                .child(ui::input(&self.folder_name_input))
                                .child(match name_error {
                                    Some(e) => div().text_size(px(12.)).text_color(c(theme::NEGATIVE)).child(e),
                                    None => div().text_size(px(11.5)).text_color(c(theme::MUTED)).child("Enter creates it and opens it · Esc cancels"),
                                }),
                        ),
                )
            })
            .when(!listed && error.is_none(), |el| {
                el.child(h().gap(px(8.)).p(px(12.)).text_color(c(theme::MUTED)).text_size(px(12.5)).child(ui::pulse_dot("folder-loading", 6., theme::ACCENT)).child(format!("Loading the folders on {host}…")))
            })
            .when_some(error.clone(), |el, e| {
                el.child(
                    v().gap(px(6.)).m(px(4.)).p(px(12.)).rounded(px(6.)).bg(c(theme::SURFACE_2))
                        .child(div().text_size(px(12.5)).text_color(c(theme::NEGATIVE)).child(e))
                        .child(div().text_size(px(12.)).text_color(c(theme::TEXT_2)).child("Pick a starting point on the left, go up or type a path.")),
                )
            })
            .when(listed && error.is_none() && rows.is_empty(), |el| el.child(div().p(px(12.)).text_color(c(theme::MUTED)).text_size(px(12.5)).child(empty)))
            .when(error.is_none(), |el| el.children(rows.iter().enumerate().map(|(i, entry)| row(i, entry, cx))))
            .when(truncated && error.is_none(), |el| el.child(div().p(px(10.)).text_size(px(11.5)).text_color(c(theme::MUTED)).child("Only the first 1000 folders are shown.")));

        let new_folder = ui::button("folder-new", "New folder", ButtonKind::Outline, Some(ui::kbd("secondary-shift-n", cx)))
            .when_some(blocked, |el, why| el.opacity(0.45).tooltip(ui::tip(why, None)))
            .on_click(|_, window, cx| window.dispatch_action(Box::new(FolderNewFolder), cx));
        let use_button = ui::button("folder-use", "Use this folder", ButtonKind::Primary, Some(ui::kbd("secondary-enter", cx)))
            .when(target.is_none(), |el| el.opacity(0.45))
            .on_click(|_, window, cx| window.dispatch_action(Box::new(FolderUse), cx));

        crate::screens::overlays::card(760.)
            .track_focus(&self.overlay_focus)
            .key_context(FOLDERS)
            .on_key_down(cx.listener(|this, e: &KeyDownEvent, window, cx| {
                if this.overlay_focus.is_focused(window) && this.folder_type(&e.keystroke, cx) {
                    cx.stop_propagation();
                }
            }))
            .child(
                h().px(px(22.)).py(px(14.)).gap(px(10.)).border_b_1().border_color(c(theme::BORDER))
                    .child(div().flex_1().font_family(ui::serif(cx)).text_size(px(20.)).child(format!("Choose a folder on {host}")))
                    .when(loading && listed, |el| el.child(ui::pulse_dot("folder-busy", 6., theme::ACCENT)))
                    .child(ui::close_button(cx)),
            )
            .child(
                h().items_start().px(px(16.)).py(px(8.)).gap(px(10.)).min_h(px(40.)).border_b_1().border_color(c(theme::BORDER))
                    .child(div().pt(px(if typing_path { 13. } else { 5. })).child(ui::folder_glyph(theme::ACCENT_TEXT)))
                    .child(path_bar)
                    .when(!typing_path, |el| el.child(ui::hint("folder-path", "secondary-l", "Type a path", || Box::new(FolderTypePath), cx).text_size(px(11.5)))),
            )
            .when(!filter.is_empty(), |el| {
                el.child(
                    h().px(px(22.)).py(px(6.)).gap(px(8.)).bg(c(theme::SURFACE)).border_b_1().border_color(c(theme::BORDER)).text_size(px(12.))
                        .child(div().text_color(c(theme::MUTED)).child("Filter"))
                        .child(div().font_family(mono.clone()).text_color(c(theme::TEXT)).child(filter.clone()))
                        .child(div().text_color(c(theme::MUTED)).child(format!("· {} found", rows.len())))
                        .child(div().flex_1())
                        .child(ui::key_hint(&["escape"], "clears", cx)),
                )
            })
            .when_some(notice, |el, notice| {
                el.child(div().px(px(22.)).py(px(8.)).bg(c(theme::ATTENTION_BG)).border_b_1().border_color(c(theme::BORDER)).text_size(px(12.)).text_color(c(theme::ACCENT_TEXT)).child(notice))
            })
            .child(h().items_start().child(quick_access).child(list))
            .child(
                v().px(px(22.)).py(px(10.)).gap(px(10.)).border_t_1().border_color(c(theme::BORDER))
                    .child(
                        h().gap(px(14.)).text_size(px(11.5)).text_color(c(theme::MUTED))
                            .child(ui::key_hint(&["up", "down"], "select", cx))
                            .child(ui::hint("folder-open", "enter", "Open", || Box::new(FolderOpen), cx))
                            .child(ui::hint("folder-up", "backspace", "Up", || Box::new(FolderParent), cx))
                            .child(div().child("Type to filter · double-click opens")),
                    )
                    .child(
                        h().gap(px(10.))
                            .child(new_folder)
                            .when(local, |el| {
                                el.child(ui::button("folder-system", "System dialog…", ButtonKind::Outline, Some(ui::kbd("secondary-o", cx))).on_click(|_, window, cx| window.dispatch_action(Box::new(FolderSystemDialog), cx)))
                            })
                            .child(
                                div().id("folder-target").flex_1().min_w_0().text_right().truncate().font_family(mono).text_size(px(11.5))
                                    .text_color(c(if target.is_some() { theme::TEXT_2 } else { theme::MUTED }))
                                    .when_some(target.clone(), |el, t| el.tooltip(ui::tip(t, None)))
                                    .child(target.clone().unwrap_or_else(|| if at_roots { "Select a starting point".into() } else { String::new() })),
                            )
                            .child(use_button),
                    ),
            )
    }
}

/// One clickable crumb of the path bar.
fn crumb(id: impl Into<ElementId>, name: String, current: bool) -> Stateful<Div> {
    div().id(id).px(px(5.)).py(px(2.)).rounded(px(4.)).cursor_pointer()
        .text_color(c(if current { theme::TEXT } else { theme::TEXT_2 }))
        .hover(|s| s.bg(c(theme::SURFACE_2)).text_color(c(theme::TEXT)))
        .child(name)
}

/// The crumbs of a server path: (name, path to go there), from the top.
/// `C:\Users\me` → `C:`, `Users`, `me`; `/home/me` → `/`, `home`, `me`.
pub fn crumbs(path: &str) -> Vec<(String, String)> {
    let windows = path.contains('\\') || path.as_bytes().get(1) == Some(&b':');
    let sep = if windows { '\\' } else { '/' };
    let mut out = Vec::new();
    let mut acc = String::new();
    let rest = if let Some(unc) = path.strip_prefix(r"\\") {
        // \\server\share is one crumb.
        let mut parts = unc.splitn(3, '\\');
        let (server, share) = (parts.next().unwrap_or_default(), parts.next().unwrap_or_default());
        acc = format!(r"\\{server}\{share}");
        out.push((acc.clone(), acc.clone()));
        parts.next().unwrap_or_default()
    } else if let Some(rest) = path.strip_prefix('/') {
        acc.push('/');
        out.push(("/".into(), "/".into()));
        rest
    } else if windows && path.len() >= 2 && path.as_bytes()[1] == b':' {
        acc = format!("{}\\", &path[..2]);
        out.push((path[..2].to_string(), acc.clone()));
        &path[2..]
    } else {
        path
    };
    for part in rest.split(sep).filter(|p| !p.is_empty()) {
        if !acc.is_empty() && !acc.ends_with(sep) {
            acc.push(sep);
        }
        acc.push_str(part);
        out.push((part.to_string(), acc.clone()));
    }
    out
}

/// A typed or pasted path: trimmed, without the quotes Explorer's "Copy as path" adds.
pub fn typed_path(text: &str) -> String {
    let text = text.trim();
    text.strip_prefix('"').and_then(|t| t.strip_suffix('"')).unwrap_or(text).trim().to_string()
}

/// Distinct folders, in order, named by their last part.
pub fn folders_in_use(paths: impl IntoIterator<Item = String>) -> Vec<DirEntryView> {
    let mut seen = std::collections::HashSet::new();
    paths
        .into_iter()
        .filter(|p| !p.trim().is_empty())
        .filter(|p| seen.insert(p.trim_end_matches(['/', '\\']).to_lowercase()))
        .map(|path| {
            let name = path.trim_end_matches(['/', '\\']).rsplit(['/', '\\']).next().filter(|n| !n.is_empty()).unwrap_or(&path).to_string();
            DirEntryView { name, path }
        })
        .collect()
}

/// The detail of the "This computer" row: the server's name when known (E3).
pub fn local_detail(name: Option<&str>) -> String {
    match name.map(str::trim).filter(|n| !n.is_empty()) {
        Some(name) => format!("{name} · the background service here"),
        None => "the background service here".into(),
    }
}

/// The rows that contain `filter` (any case).
pub fn filter_rows(rows: Vec<DirEntryView>, filter: &str) -> Vec<DirEntryView> {
    let needle = filter.trim().to_lowercase();
    if needle.is_empty() {
        return rows;
    }
    rows.into_iter().filter(|r| r.name.to_lowercase().contains(&needle)).collect()
}

#[cfg(test)]
mod tests {
    use super::{FolderBrowser, FolderPurpose, NO_PARENT, Overlay, crumbs, folders_in_use, local_detail, typed_path};
    use aurelius_protocol::{DirEntryView, DirListing};

    #[test]
    fn this_computer_keeps_its_label_with_the_servers_name_as_detail() {
        assert_eq!(local_detail(Some("Kantoor")), "Kantoor · the background service here");
        assert_eq!(local_detail(Some("  ")), "the background service here");
        assert_eq!(local_detail(None), "the background service here");
    }

    fn entry(name: &str) -> DirEntryView {
        DirEntryView { name: name.into(), path: format!("/x/{name}") }
    }

    fn listing(path: Option<&str>, entries: &[&str], roots: &[&str]) -> DirListing {
        DirListing {
            path: path.map(str::to_string),
            parent: None,
            entries: entries.iter().map(|n| entry(n)).collect(),
            roots: roots.iter().map(|n| entry(n)).collect(),
            truncated: false,
        }
    }

    #[test]
    fn crumbs_jump_to_every_level_on_windows_and_unix() {
        let pairs = |p: &str| crumbs(p).into_iter().map(|(n, p)| format!("{n}={p}")).collect::<Vec<_>>();
        assert_eq!(pairs(r"C:\Users\me"), [r"C:=C:\", r"Users=C:\Users", r"me=C:\Users\me"]);
        assert_eq!(pairs(r"D:\"), [r"D:=D:\"]);
        assert_eq!(pairs("/home/me/work"), ["/=/", "home=/home", "me=/home/me", "work=/home/me/work"]);
        assert_eq!(pairs("/"), ["/=/"]);
        assert_eq!(pairs(r"\\nas\share\docs"), [r"\\nas\share=\\nas\share", r"docs=\\nas\share\docs"]);
    }

    #[test]
    fn a_pasted_path_loses_its_quotes_and_spaces() {
        assert_eq!(typed_path("  \"C:\\My Work\"  "), "C:\\My Work");
        assert_eq!(typed_path("/srv/data "), "/srv/data");
        assert_eq!(typed_path("   "), "");
    }

    #[test]
    fn folders_in_use_are_distinct_and_named_by_their_last_part() {
        let found = folders_in_use([r"C:\Work\Atlas".to_string(), "/srv/beta/".into(), r"c:\work\atlas\".into(), String::new()]);
        let names: Vec<_> = found.iter().map(|e| (e.name.as_str(), e.path.as_str())).collect();
        assert_eq!(names, [("Atlas", r"C:\Work\Atlas"), ("beta", "/srv/beta/")]);
    }

    #[test]
    fn typing_filters_the_list_and_use_takes_the_shown_folder_or_the_selected_start() {
        let mut b = FolderBrowser::new(FolderPurpose::Dialog, "Choose folder", Overlay::None);
        assert_eq!(b.target(), None);
        b.listing = Some(listing(None, &[], &["Home", "C:", "D:"]));
        b.filter = "d".into();
        assert_eq!(b.rows().iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), ["D:"]);
        assert_eq!(b.target().as_deref(), Some("/x/D:"));
        assert_eq!(b.new_folder_blocked(), Some(NO_PARENT));
        b.listing = Some(listing(Some("/x"), &["Alpha", "beta", "Gamma"], &["Home"]));
        b.filter = "A".into();
        assert_eq!(b.rows().len(), 3, "a matches alpha, beta and gamma in any case");
        b.filter = "mm".into();
        assert_eq!(b.rows().iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), ["Gamma"]);
        assert_eq!(b.target().as_deref(), Some("/x"), "inside a folder, the shown folder is used");
        assert_eq!(b.new_folder_blocked(), None);
    }
}
