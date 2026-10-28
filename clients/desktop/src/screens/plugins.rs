//! Manager plugins (protocol 1.22): Settings → Plugins (what is installed, who uses it,
//! update and remove), the Add plugin dialog (a link or Browse → pick plugins and
//! managers → install), the Plugins section of Edit manager (a switch per plugin) and
//! the Project plugins section of Settings → Advanced → Plugins (the switches per project).
//! Claude Code's own plugin install is the source of truth; the server drives it.
//! Reading needs the Read right, every change the Approve right.

use crate::app::AppView;
use crate::backend::{self, Connection, PluginsCall, PluginsUpdate};
use crate::keymap::LOAD;
use crate::theme::{self, c};
use crate::ui::{self, ButtonKind, h, v};
use aurelius_domain::{EmployeeId, ProjectId};
use aurelius_protocol::{EmployeePlugins, PluginCatalog, PluginComponents, PluginInfo, PluginOffer, PluginsSnapshot, ProjectPlugins};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

/// Settings → Plugins (after API keys).
pub const PLUGINS_TAB: usize = 7;
pub const NO_APPROVE: &str = "You need the Approve right on this server to add, update or remove plugins.";

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AddTab {
    Link,
    Browse,
}

/// The Add plugin dialog.
pub struct AddPlugin {
    pub link: Entity<InputState>,
    pub tab: AddTab,
    /// The plugins of the link's marketplace, once it was looked up.
    pub previewed: Option<(String, Vec<PluginOffer>)>,
    pub picked: Vec<String>,
    pub managers: Vec<EmployeeId>,
    pub projects: Vec<ProjectId>,
    /// What the server is busy with, shown as progress.
    pub working: Option<String>,
    pub error: Option<String>,
}

#[derive(Default)]
pub struct PluginsState {
    pub installed: Option<Result<PluginsSnapshot, String>>,
    pub catalog: Option<Result<PluginCatalog, String>>,
    /// The manager open in Edit manager: its enabled plugins.
    pub manager: Option<EmployeePlugins>,
    /// The project whose Project settings are open, and its enabled plugins once loaded.
    pub project_open: Option<ProjectId>,
    pub project: Option<ProjectPlugins>,
    /// The plugins of that project's managers, as they load.
    pub project_managers: Vec<EmployeePlugins>,
    /// An update or remove that is running (plugin id).
    pub busy: Option<String>,
    pub confirm_remove: Option<String>,
    pub error: Option<String>,
    pub add: Option<AddPlugin>,
}

fn pchip(label: impl Into<SharedString>, color: u32) -> Div {
    div().px(px(8.)).py(px(2.)).rounded(px(5.)).border_1().border_color(c(theme::BORDER)).bg(c(theme::SURFACE_2)).text_size(px(11.5)).text_color(c(color)).child(label.into())
}

fn switch(id: impl Into<ElementId>, on: bool, label: &str) -> Stateful<Div> {
    h().id(id)
        .role(Role::Button)
        .aria_label(SharedString::from(format!("{label}: {}", if on { "on" } else { "off" })))
        .flex_none()
        .w(px(34.))
        .h(px(18.))
        .p(px(2.))
        .rounded_full()
        .cursor_pointer()
        .when(on, |el| el.justify_end().bg(c(theme::ACCENT)))
        .when(!on, |el| el.bg(c(theme::SURFACE_2)).border_1().border_color(c(theme::BORDER_STRONG)))
        .child(div().size(px(if on { 14. } else { 12. })).rounded_full().bg(c(if on { theme::BG } else { theme::TEXT_2 })))
}

fn notice(text: impl Into<SharedString>, bad: bool) -> Div {
    h().px(px(12.)).py(px(10.)).rounded(px(8.)).border_1()
        .border_color(c(if bad { theme::NEGATIVE } else { theme::ATTENTION_BORDER }))
        .bg(c(theme::ATTENTION_BG))
        .text_size(px(12.5))
        .text_color(c(if bad { theme::NEGATIVE } else { theme::TEXT_2 }))
        .child(div().flex_1().min_w_0().child(text.into()))
}

/// Who a set of plugin switches belongs to.
#[derive(Clone, Copy)]
pub enum Owner {
    Manager(EmployeeId),
    Project(ProjectId),
}

/// The label of a manager's plugin that its project also has, and whether its switch is locked:
/// only a plugin from the project alone is locked; one also in the manager's own list stays
/// switchable so that own entry can be removed.
fn from_project_label(own: bool, inherited: bool) -> Option<(&'static str, bool)> {
    match (inherited, own) {
        (false, _) => None,
        (true, true) => Some(("Also from project", false)),
        (true, false) => Some(("From project", true)),
    }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// The chips of what a plugin brings. `None`: unknown until it is installed.
fn component_chips(components: Option<&PluginComponents>) -> Div {
    let Some(k) = components else {
        return h().gap(px(6.)).flex_wrap().child(pchip("contents known after install", theme::MUTED));
    };
    let mut chips = h().gap(px(6.)).flex_wrap();
    if !k.skills.is_empty() {
        chips = chips.child(pchip(plural(k.skills.len(), "skill", "skills"), theme::TEXT_2));
    }
    if !k.commands.is_empty() {
        chips = chips.child(pchip(plural(k.commands.len(), "command", "commands"), theme::TEXT_2));
    }
    if !k.agents.is_empty() {
        chips = chips.child(pchip(plural(k.agents.len(), "agent", "agents"), theme::TEXT_2));
    }
    if k.hooks {
        chips = chips.child(pchip("hooks", theme::NEGATIVE));
    }
    if !k.mcp_servers.is_empty() {
        chips = chips.child(pchip(plural(k.mcp_servers.len(), "MCP server", "MCP servers"), theme::TEXT_2));
    }
    chips
}

/// What a plugin runs or skips, as warnings under its chips.
fn component_notes(k: &PluginComponents) -> Div {
    v().gap(px(4.))
        .when(k.hooks, |el| el.child(div().text_size(px(12.)).text_color(c(theme::NEGATIVE)).child("Runs hooks (commands on this PC) when an employee session starts or ends. They run outside the approval step: giving an employee this plugin is your consent.")))
        .when(!k.mcp_servers.is_empty(), |el| el.child(h().gap(px(6.)).items_center().text_size(px(12.)).text_color(c(theme::MUTED)).child("MCP servers not loaded").child(ui::info("plugin-mcp-about", "Employees use the plugin's skills, not its MCP servers."))))
}

/// Why plugin links can't be used; `None` when they can.
pub fn parse_source(text: &str) -> Result<String, String> {
    let t = text.trim();
    if t.is_empty() {
        return Err("Paste a link, for example https://github.com/owner/repo or owner/repo.".into());
    }
    let local = t.starts_with('/') || t.starts_with('.') || t.starts_with('~') || t.starts_with('\\') || t.as_bytes().get(1) == Some(&b':');
    if local {
        return Err("Local folders are not accepted. Use a GitHub link, owner/repo, or a git/https URL.".into());
    }
    let url = t.contains("://") || t.starts_with("git@");
    if !url && t.split('/').filter(|p| !p.is_empty()).count() != 2 {
        return Err("That doesn't look like a link. Use https://github.com/owner/repo, owner/repo, or a git/https URL.".into());
    }
    Ok(t.to_string())
}

impl AppView {
    /// Why plugins can't be used at all now; `None` when they can.
    pub fn plugins_unavailable(&self) -> Option<String> {
        match &self.connection {
            Connection::Online { .. } => None,
            _ => Some("Not connected to a server.".into()),
        }
    }

    /// Why changes are off now; `None` when this connection may change plugins.
    pub fn plugins_read_only(&self) -> Option<String> {
        match &self.connection {
            Connection::Online { server, .. } if !server.approve => Some(NO_APPROVE.into()),
            _ => None,
        }
    }

    /// The server has plugins per project (an older one doesn't know the methods).
    pub fn project_plugins_supported(&self) -> bool {
        matches!(&self.connection, Connection::Online { server, .. } if server.project_plugins)
    }

    /// The project's plugins apply to its managers too (else only to the project assistant).
    pub fn project_plugins_managers_supported(&self) -> bool {
        matches!(&self.connection, Connection::Online { server, .. } if server.project_plugins_managers)
    }

    pub fn project_description_supported(&self) -> bool {
        matches!(&self.connection, Connection::Online { server, .. } if server.project_description)
    }

    /// Project settings opened (`None`: closed): what is installed, the project's plugins and each manager's.
    pub fn plugins_for_project(&mut self, project: Option<ProjectId>) {
        self.plugins.project_open = project;
        self.plugins.project = None;
        self.plugins.project_managers.clear();
        self.plugins.error = None;
        let Some(project) = project else { return };
        if self.plugins_unavailable().is_none() && self.project_plugins_supported() {
            self.plugins_send(PluginsCall::List);
            self.plugins_send(PluginsCall::ProjectGet(project));
            // The employees working in the project (open tasks there).
            for m in self.managers.iter().filter(|m| m.projects_active.contains(&project)) {
                self.plugins_send(PluginsCall::ManagerGet(m.id));
            }
        }
    }

    fn plugins_send(&self, call: PluginsCall) {
        let _ = self.requests_handle().send(backend::Request::Plugins(call));
    }

    pub fn plugins_load(&mut self) {
        if self.plugins_unavailable().is_none() {
            self.plugins_send(PluginsCall::List);
        }
    }

    pub fn enter_plugins_tab(&mut self) {
        self.plugins.error = None;
        self.plugins.confirm_remove = None;
        self.plugins_load();
        // The project whose plugins show below the list loads again too.
        if let Some(project) = self.plugins.project_open {
            self.plugins_for_project(Some(project));
        }
    }

    /// Edit manager opened: what is installed and what this manager has.
    pub fn plugins_for_manager(&mut self, id: EmployeeId) {
        self.plugins.manager = None;
        self.plugins.error = None;
        if self.plugins_unavailable().is_none() {
            self.plugins_send(PluginsCall::List);
            self.plugins_send(PluginsCall::ManagerGet(id));
        }
    }

    pub fn plugins_update(&mut self, update: PluginsUpdate, cx: &mut Context<AppView>) {
        match update {
            PluginsUpdate::Installed(result) => {
                self.plugins.busy = None;
                self.plugins.confirm_remove = None;
                if result.is_ok() {
                    self.plugins.error = None;
                }
                self.plugins.installed = Some(result);
                self.plugins_refresh_manager();
            }
            PluginsUpdate::Catalog(result) => self.plugins.catalog = Some(result),
            PluginsUpdate::Preview(result) => {
                if let Some(add) = self.plugins.add.as_mut() {
                    add.working = None;
                    match result {
                        Ok(preview) => {
                            add.error = None;
                            let fresh: Vec<&PluginOffer> = preview.offers.iter().filter(|o| !o.installed).collect();
                            if fresh.len() == 1 {
                                add.picked = vec![fresh[0].id.clone()];
                            }
                            add.previewed = Some((preview.marketplace, preview.offers));
                        }
                        Err(why) => add.error = Some(why),
                    }
                }
            }
            PluginsUpdate::InstallDone(result) => match result {
                Ok(snapshot) => {
                    let n = self.plugins.add.take().map_or(0, |a| a.picked.len());
                    self.plugins.installed = Some(Ok(snapshot));
                    self.plugins_refresh_manager();
                    self.plugins_send(PluginsCall::Available);
                    self.notify_user(format!("{} installed. Employees use them from their next run.", plural(n, "plugin", "plugins")), false, cx);
                }
                Err(why) => {
                    if let Some(add) = self.plugins.add.as_mut() {
                        add.working = None;
                        add.error = Some(why);
                    }
                    // Some may have been installed before one failed.
                    self.plugins_load();
                }
            },
            PluginsUpdate::Manager(m) => {
                if self.managers.iter().any(|x| x.id == m.employee_id && self.plugins.project_open.is_some_and(|p| x.projects_active.contains(&p))) {
                    match self.plugins.project_managers.iter_mut().find(|x| x.employee_id == m.employee_id) {
                        Some(x) => *x = m.clone(),
                        None => self.plugins.project_managers.push(m.clone()),
                    }
                }
                self.plugins.manager = Some(m);
            }
            PluginsUpdate::Project(p) => {
                if self.plugins.project_open == Some(p.project_id) {
                    self.plugins.project = Some(p);
                }
            }
            PluginsUpdate::Failed(why) => {
                self.plugins.busy = None;
                self.plugins.error = Some(why);
                // The toggles go back to what the server has.
                self.plugins_refresh_manager();
            }
        }
        cx.notify();
    }

    fn plugins_refresh_manager(&self) {
        if let Some(id) = self.plugins.manager.as_ref().map(|m| m.employee_id) {
            self.plugins_send(PluginsCall::ManagerGet(id));
        }
        if let Some(id) = self.plugins.project_open {
            self.plugins_send(PluginsCall::ProjectGet(id));
            for m in &self.plugins.project_managers {
                self.plugins_send(PluginsCall::ManagerGet(m.employee_id));
            }
        }
    }

    fn plugin_installed_list(&self) -> Vec<PluginInfo> {
        match &self.plugins.installed {
            Some(Ok(s)) => s.plugins.clone(),
            _ => Vec::new(),
        }
    }

    fn manager_label(&self, id: EmployeeId) -> String {
        match self.managers.iter().find(|m| m.id == id) {
            Some(m) => match self.department_name(m.department_id) {
                d if d.is_empty() => m.name.clone(),
                d => format!("{} · {d}", m.name),
            },
            None => "(deleted employee)".into(),
        }
    }

    // ----- the Add plugin dialog ---------------------------------------------------------------

    /// Open Add plugin; `owner` is ticked from the start (the Edit manager or project shortcut).
    fn open_add_plugin_for(&mut self, owner: Owner, window: &mut Window, cx: &mut Context<Self>) {
        self.open_add_plugin(None, window, cx);
        if let Some(add) = self.plugins.add.as_mut() {
            match owner {
                Owner::Manager(id) => add.managers.push(id),
                Owner::Project(id) => add.projects.push(id),
            }
        }
    }

    /// Open Add plugin; `manager` is ticked from the start.
    pub fn open_add_plugin(&mut self, manager: Option<EmployeeId>, window: &mut Window, cx: &mut Context<Self>) {
        let link = cx.new(|cx| InputState::new(window, cx).placeholder("https://github.com/owner/repo  or  owner/repo"));
        cx.subscribe_in(&link, window, |this, _, event: &InputEvent, _, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                this.plugin_preview(cx);
            }
        })
        .detach();
        link.update(cx, |i, cx| i.focus(window, cx));
        self.plugins.add = Some(AddPlugin { link, tab: AddTab::Link, previewed: None, picked: Vec::new(), managers: manager.into_iter().collect(), projects: Vec::new(), working: None, error: None });
        self.plugins_load();
        cx.notify();
    }

    pub fn close_add_plugin(&mut self, cx: &mut Context<Self>) -> bool {
        let busy = self.plugins.add.as_ref().is_some_and(|a| a.working.is_some());
        if self.plugins.add.is_none() || busy {
            return self.plugins.add.is_some();
        }
        self.plugins.add = None;
        cx.notify();
        true
    }

    fn plugin_preview(&mut self, cx: &mut Context<Self>) {
        if self.plugins_unavailable().is_some() || self.plugins_read_only().is_some() {
            return;
        }
        let Some(add) = self.plugins.add.as_mut() else { return };
        if add.working.is_some() {
            return;
        }
        match parse_source(&add.link.read(cx).value()) {
            Ok(source) => {
                add.error = None;
                add.previewed = None;
                add.working = Some("Adding the marketplace and reading its plugins… (the first time it downloads it)".into());
                self.plugins_send(PluginsCall::Preview(source));
            }
            Err(why) => add.error = Some(why),
        }
        cx.notify();
    }

    fn plugin_install(&mut self, cx: &mut Context<Self>) {
        if self.plugins_unavailable().is_some() || self.plugins_read_only().is_some() {
            return;
        }
        let Some(add) = self.plugins.add.as_mut() else { return };
        if add.working.is_some() || add.picked.is_empty() {
            return;
        }
        add.error = None;
        add.working = Some(format!("Installing {}… this can take a minute.", plural(add.picked.len(), "plugin", "plugins")));
        let call = PluginsCall::Install { ids: add.picked.clone(), managers: add.managers.clone(), projects: add.projects.clone() };
        self.plugins_send(call);
        cx.notify();
    }

    fn offer_row(&self, prefix: &'static str, i: usize, offer: &PluginOffer, cx: &mut Context<Self>) -> Stateful<Div> {
        let picked = self.plugins.add.as_ref().is_some_and(|a| a.picked.contains(&offer.id));
        let id = offer.id.clone();
        h().id((prefix, i)).items_start().gap(px(10.)).px(px(12.)).py(px(10.)).rounded(px(8.)).border_1().cursor_pointer()
            .border_color(c(if picked { theme::ACCENT } else { theme::BORDER }))
            .when(picked, |el| el.bg(c(theme::SURFACE_2)))
            .hover(|s| s.border_color(c(theme::BORDER_STRONG)))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                if let Some(add) = this.plugins.add.as_mut().filter(|a| a.working.is_none()) {
                    match add.picked.iter().position(|x| *x == id) {
                        Some(at) => { add.picked.remove(at); }
                        None => add.picked.push(id.clone()),
                    }
                }
                cx.notify();
            }))
            .child(div().pt(px(1.)).text_color(c(if picked { theme::ACCENT_TEXT } else { theme::MUTED })).child(if picked { "☑" } else { "☐" }))
            .child(
                v().flex_1().min_w_0().gap(px(5.))
                    .child(
                        h().gap(px(8.))
                            .child(div().truncate().text_size(px(13.5)).font_weight(FontWeight::MEDIUM).child(offer.name.clone()))
                            .child(div().min_w_0().truncate().text_size(px(11.5)).text_color(c(theme::MUTED)).child(offer.marketplace.clone()))
                            .when(offer.installed, |el| el.child(pchip("installed", theme::ACCENT_TEXT))),
                    )
                    .when(!offer.description.is_empty(), |el| el.child(div().text_size(px(12.)).line_height(px(17.)).text_color(c(theme::TEXT_2)).child(offer.description.clone())))
                    .child(component_chips(offer.components.as_ref()))
                    .when_some(offer.components.as_ref(), |el, k| el.child(component_notes(k))),
            )
    }

    pub fn render_add_plugin(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(add) = self.plugins.add.as_ref() else { return div().into_any_element() };
        let (tab, working, error, picked_n) = (add.tab, add.working.clone(), add.error.clone(), add.picked.len());
        let link = add.link.clone();
        let top = 90.;
        let max_h = (f32::from(window.viewport_size().height) - top - 24.).max(160.);
        let max_w = (f32::from(window.viewport_size().width) - 32.).max(280.);
        let scroll = window.use_keyed_state("add-plugin-scroll", cx, |_, _| ScrollHandle::default()).read(cx).clone();
        let unavailable = self.plugins_unavailable();
        let read_only = self.plugins_read_only();
        let blocked = unavailable.is_some() || read_only.is_some();
        let busy = working.is_some();

        let offers: Vec<PluginOffer> = match tab {
            AddTab::Link => self.plugins.add.as_ref().and_then(|a| a.previewed.as_ref()).map(|(_, o)| o.clone()).unwrap_or_default(),
            AddTab::Browse => match &self.plugins.catalog {
                Some(Ok(c)) => c.offers.clone(),
                _ => Vec::new(),
            },
        };
        let marketplace = self.plugins.add.as_ref().and_then(|a| a.previewed.as_ref()).map(|(m, _)| m.clone());
        let prefix = if tab == AddTab::Link { "link-offer" } else { "browse-offer" };

        let tabs = h().gap(px(6.)).children([(AddTab::Link, "From a link"), (AddTab::Browse, "Browse")].into_iter().enumerate().map(|(i, (t, label))| {
            let on = tab == t;
            h().id(("add-plugin-tab", i)).role(Role::Tab).h(px(28.)).px(px(12.)).rounded(px(6.)).cursor_pointer().text_size(px(12.5)).border_1()
                .border_color(c(if on { theme::ACCENT } else { theme::BORDER }))
                .when(on, |el| el.bg(c(theme::SURFACE_2)))
                .text_color(c(if on { theme::TEXT } else { theme::TEXT_2 }))
                .child(label)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    if let Some(add) = this.plugins.add.as_mut().filter(|a| a.working.is_none()) {
                        add.tab = t;
                        add.error = None;
                    }
                    if t == AddTab::Browse {
                        this.plugins_send(PluginsCall::Available);
                    }
                    cx.notify();
                }))
        }));

        let link_part = v().gap(px(8.))
            .child(div().text_size(px(12.)).text_color(c(theme::TEXT_2)).child(ui::label_with_info("Link to a plugin marketplace", "A GitHub link, owner/repo, or a git/https URL. Aurelius adds the marketplace to Claude Code on the server and lists its plugins.")))
            .child(
                h().gap(px(8.)).items_start()
                    .child(div().flex_1().min_w_0().child(ui::input(&link)))
                    .child(ui::button("plugin-lookup", "Look up", ButtonKind::Primary, None).h(px(36.)).when(blocked || busy, |el| el.opacity(0.45)).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.plugin_preview(cx)))),
            )
            .when_some(marketplace.filter(|_| !offers.is_empty()), |el, m| el.child(div().text_size(px(12.)).text_color(c(theme::TEXT_2)).child(format!("Plugins in {m}"))));

        let browse_note = match (&self.plugins.catalog, tab) {
            (Some(Err(why)), AddTab::Browse) => Some(notice(why.clone(), true)),
            (None, AddTab::Browse) => Some(notice("Loading the marketplaces already added…", false)),
            (Some(Ok(_)), AddTab::Browse) if offers.is_empty() => Some(notice("No marketplaces added yet. Use “From a link” to add one.", false)),
            _ => None,
        };

        let managers: Vec<(EmployeeId, String)> = self.managers.iter().map(|m| (m.id, self.manager_label(m.id))).collect();
        let ticked = self.plugins.add.as_ref().map(|a| a.managers.clone()).unwrap_or_default();
        let manager_part = v().gap(px(6.))
            .child(div().text_size(px(12.)).text_color(c(theme::TEXT_2)).child("Turn on for these employees (you can change it later)"))
            .child(h().gap(px(6.)).flex_wrap().children(managers.into_iter().enumerate().map(|(i, (id, label))| {
                let on = ticked.contains(&id);
                h().id(("add-plugin-manager", i)).h(px(28.)).px(px(10.)).rounded(px(6.)).border_1().cursor_pointer().text_size(px(12.5))
                    .border_color(c(if on { theme::ACCENT } else { theme::BORDER }))
                    .when(on, |el| el.bg(c(theme::SURFACE_2)))
                    .text_color(c(if on { theme::TEXT } else { theme::TEXT_2 }))
                    .hover(|s| s.border_color(c(theme::BORDER_STRONG)))
                    .child(format!("{} {label}", if on { "☑" } else { "☐" }))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        if let Some(add) = this.plugins.add.as_mut().filter(|a| a.working.is_none()) {
                            match add.managers.iter().position(|x| *x == id) {
                                Some(at) => { add.managers.remove(at); }
                                None => add.managers.push(id),
                            }
                        }
                        cx.notify();
                    }))
            })));
        let projects: Vec<(ProjectId, String)> = if self.project_plugins_supported() { self.projects.iter().map(|p| (p.id, p.name.clone())).collect() } else { Vec::new() };
        let ticked = self.plugins.add.as_ref().map(|a| a.projects.clone()).unwrap_or_default();
        let project_part = v().gap(px(6.))
            .child(div().text_size(px(12.)).text_color(c(theme::TEXT_2)).child("And for the assistant of these projects"))
            .child(h().gap(px(6.)).flex_wrap().children(projects.iter().cloned().enumerate().map(|(i, (id, label))| {
                let on = ticked.contains(&id);
                h().id(("add-plugin-project", i)).h(px(28.)).px(px(10.)).rounded(px(6.)).border_1().cursor_pointer().text_size(px(12.5))
                    .border_color(c(if on { theme::ACCENT } else { theme::BORDER }))
                    .when(on, |el| el.bg(c(theme::SURFACE_2)))
                    .text_color(c(if on { theme::TEXT } else { theme::TEXT_2 }))
                    .hover(|s| s.border_color(c(theme::BORDER_STRONG)))
                    .child(format!("{} {label}", if on { "☑" } else { "☐" }))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        if let Some(add) = this.plugins.add.as_mut().filter(|a| a.working.is_none()) {
                            match add.projects.iter().position(|x| *x == id) {
                                Some(at) => { add.projects.remove(at); }
                                None => add.projects.push(id),
                            }
                        }
                        cx.notify();
                    }))
            })));

        let body = v().gap(px(14.))
            .child(h().gap(px(8.)).items_center().child(div().font_family(ui::serif(cx)).text_size(px(22.)).child("Add plugin")).child(ui::info("add-plugin-about", "Plugins give an employee extra skills. Pick the ones you want, then the employees that should use them.")))
            .when_some(unavailable.clone(), |el, why| el.child(notice(why, false)))
            .when_some(read_only.clone(), |el, why| el.child(notice(why, false)))
            .child(tabs)
            .when(tab == AddTab::Link, |el| el.child(link_part))
            .children(browse_note)
            .child(v().gap(px(8.)).children(offers.iter().enumerate().map(|(i, o)| self.offer_row(prefix, i, o, cx))))
            .when(!offers.is_empty(), |el| el.child(manager_part))
            .when(!offers.is_empty() && !projects.is_empty(), |el| el.child(project_part))
            .when_some(working.clone(), |el, text| el.child(h().gap(px(8.)).px(px(12.)).py(px(10.)).rounded(px(8.)).border_1().border_color(c(theme::BORDER)).bg(c(theme::SURFACE)).text_size(px(12.5)).text_color(c(theme::TEXT_2)).child(ui::dot(7., theme::ACCENT)).child(div().flex_1().min_w_0().child(text))))
            .when_some(error, |el, why| el.child(notice(why, true)));

        let can_install = !blocked && !busy && picked_n > 0;
        let card = crate::screens::overlays::card(560.)
            .child(ui::scroll_frame(&scroll, crate::screens::overlays::scroll_body("add-plugin-body", &scroll, (max_h - 70.).max(120.)).p(px(22.)).child(body)))
            .child(
                h().px(px(22.)).py(px(14.)).gap(px(8.)).border_t_1().border_color(c(theme::BORDER))
                    .child(div().flex_1().text_size(px(11.5)).text_color(c(theme::MUTED)).child(if picked_n == 0 { "Pick at least one plugin.".to_string() } else { format!("{} picked", plural(picked_n, "plugin", "plugins")) }))
                    .child(ui::button("add-plugin-cancel", "Close", ButtonKind::Outline, Some(ui::kbd("escape", cx))).when(busy, |el| el.opacity(0.45)).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.close_add_plugin(cx);
                    })))
                    .child(ui::button("add-plugin-install", if busy { "Working…" } else { "Install" }, ButtonKind::Primary, None).when(!can_install, |el| el.opacity(0.45)).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.plugin_install(cx)))),
            );
        deferred(
            div().id("add-plugin-scrim").absolute().top_0().left_0().size_full().bg(Hsla { a: 0.55, ..c(0x000000) }).flex().justify_center().items_start().pt(px(top)).occlude()
                .key_context(LOAD)
                .child(v().max_h(px(max_h)).max_w(px(max_w)).on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()).child(card)),
        )
        .with_priority(2)
        .into_any_element()
    }

    // ----- Settings → Plugins ------------------------------------------------------------------

    pub fn render_plugins_settings(&mut self, page: Div, cx: &mut Context<Self>) -> Div {
        let header = h().px(px(22.)).py(px(14.)).border_b_1().border_color(c(theme::BORDER)).child(self.settings_title(cx));
        let body = v().id("plugins-body").flex_1().min_h_0().overflow_y_scrollbar().p(px(22.)).gap(px(12.));
        if let Some(why) = self.plugins_unavailable() {
            return page.child(header).child(body.child(notice(why, false)));
        }
        let read_only = self.plugins_read_only();
        let can_change = read_only.is_none();
        let top = h().gap(px(14.))
            .child(div().flex_1())
            .child(ui::button("plugin-add", "Add plugin", ButtonKind::Primary, None).h(px(34.)).when(!can_change, |el| el.opacity(0.45)).on_click(cx.listener(|this, _: &ClickEvent, w, cx| {
                if this.plugins_read_only().is_none() {
                    this.open_add_plugin(None, w, cx);
                }
            })));
        let plugins = self.plugin_installed_list();
        let loaded = matches!(self.plugins.installed, Some(Ok(_)));
        let list = v().gap(px(10.)).children(plugins.iter().enumerate().map(|(i, p)| self.plugin_card(i, p, can_change, cx)));
        page.child(header).child(
            body.child(top)
                .when_some(read_only, |el, why| el.child(notice(why, false)))
                .when_some(self.plugins.error.clone(), |el, why| el.child(notice(why, true)))
                .when_some(self.plugins.installed.as_ref().and_then(|r| r.as_ref().err()).cloned(), |el, why| el.child(notice(why, true)))
                .when(self.plugins.installed.is_none(), |el| el.child(div().text_size(px(13.)).text_color(c(theme::MUTED)).child("Loading…")))
                .when(loaded && plugins.is_empty(), |el| el.child(div().text_size(px(13.)).text_color(c(theme::MUTED)).child("No plugins yet. Press Add plugin to install one.")))
                .child(list)
                .when(self.project_plugins_supported() && !self.projects.is_empty(), |el| el.child(self.project_plugins_section(cx))),
        )
    }

    /// The plugins each project switches on (they apply to its employees and its project assistant):
    /// choose a project, then its switches. A plugin card above names its projects too.
    fn project_plugins_section(&self, cx: &mut Context<Self>) -> Div {
        let open = self.plugins.project_open.filter(|id| self.projects.iter().any(|p| p.id == *id));
        let wide = self.project_plugins_managers_supported();
        let (title, intro) = if wide {
            ("Project: every employee working in it and the project assistant", "Plugins switched on for a project apply to every employee working in it and to the project assistant. An employee's own plugins are under Advanced in Edit employee.")
        } else {
            ("Project: the project assistant only", "This server applies a project's plugins to the project assistant only. An employee's own plugins are under Advanced in Edit employee.")
        };
        v().gap(px(10.)).mt(px(8.)).pt(px(14.)).border_t_1().border_color(c(theme::BORDER))
            .child(h().gap(px(6.)).items_center().child(ui::eyebrow("PROJECT PLUGINS")).child(ui::info("project-plugins-about", intro)))
            .child(h().flex_wrap().gap(px(6.)).children(self.projects.iter().enumerate().map(|(i, p)| {
                let id = p.id;
                crate::screens::app_keys::chip(("plugin-project", i), p.name.clone(), open == Some(id)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.plugins_for_project(Some(id));
                    cx.notify();
                }))
            })))
            .when(open.is_none(), |el| el.child(div().text_size(px(12.5)).text_color(c(theme::MUTED)).child("Choose a project to see and change its plugins.")))
            .when_some(open, |el, id| el.child(self.plugin_switches(Owner::Project(id), title.into(), cx)))
    }

    fn plugin_card(&self, i: usize, p: &PluginInfo, can_change: bool, cx: &mut Context<Self>) -> Div {
        let id = p.id.clone();
        let (update_id, remove_id, confirm_id, keep_id) = (id.clone(), id.clone(), id.clone(), id.clone());
        let busy = self.plugins.busy.as_deref() == Some(p.id.as_str());
        let asking = self.plugins.confirm_remove.as_deref() == Some(p.id.as_str());
        let mut used: Vec<String> = p.employees.iter().map(|m| self.manager_label(*m)).collect();
        used.extend(p.projects.iter().map(|id| match self.projects.iter().find(|x| x.id == *id) {
            Some(x) => format!("{} (project)", x.name),
            None => "(deleted project) (project)".into(),
        }));
        v().gap(px(8.)).p(px(14.)).rounded(px(10.)).border_1().border_color(c(theme::BORDER))
            .child(
                h().gap(px(8.)).items_start()
                    .child(
                        v().flex_1().min_w_0().gap(px(2.))
                            .child(h().gap(px(8.)).child(div().truncate().text_size(px(15.)).font_weight(FontWeight::MEDIUM).child(p.name.clone())).child(div().min_w_0().truncate().text_size(px(12.)).text_color(c(theme::MUTED)).child(format!("{} · {}", p.version, p.marketplace))))
                            .when(!p.description.is_empty(), |el| el.child(div().text_size(px(12.5)).line_height(px(18.)).text_color(c(theme::TEXT_2)).child(p.description.clone()))),
                    ),
            )
            .child(component_chips(Some(&p.components)))
            .child(component_notes(&p.components))
            .when(p.missing, |el| el.child(notice("The plugin's files are missing on the server. Update it or remove it.", true)))
            .child(div().text_size(px(12.)).text_color(c(theme::TEXT_2)).child(if used.is_empty() { "Used by: nobody yet".to_string() } else { format!("Used by: {}", used.join(", ")) }))
            .child(
                h().gap(px(8.))
                    .child(ui::button(("plugin-update", i), if busy { "Working…" } else { "Update" }, ButtonKind::Outline, None).h(px(28.)).when(!can_change || busy, |el| el.opacity(0.45)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        if this.plugins_read_only().is_none() && this.plugins.busy.is_none() {
                            this.plugins.busy = Some(update_id.clone());
                            this.plugins.error = None;
                            this.plugins_send(PluginsCall::Update(update_id.clone()));
                            cx.notify();
                        }
                    })))
                    .when(!asking, |el| {
                        el.child(ui::button(("plugin-remove", i), "Remove", ButtonKind::Outline, None).h(px(28.)).text_color(c(theme::NEGATIVE)).when(!can_change || busy, |el| el.opacity(0.45)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            if this.plugins_read_only().is_none() && this.plugins.busy.is_none() {
                                this.plugins.confirm_remove = Some(remove_id.clone());
                                cx.notify();
                            }
                        })))
                    })
                    .when(asking, |el| {
                        el.child(div().text_size(px(12.)).text_color(c(theme::NEGATIVE)).child("Remove it from Claude Code and from every employee?"))
                            .child(ui::button(("plugin-remove-yes", i), "Remove", ButtonKind::Primary, None).h(px(28.)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.plugins.busy = Some(confirm_id.clone());
                                this.plugins.confirm_remove = None;
                                this.plugins_send(PluginsCall::Remove(confirm_id.clone()));
                                cx.notify();
                            })))
                            .child(ui::button(("plugin-remove-no", i), "Keep", ButtonKind::Outline, None).h(px(28.)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                if this.plugins.confirm_remove.as_deref() == Some(keep_id.as_str()) {
                                    this.plugins.confirm_remove = None;
                                }
                                cx.notify();
                            })))
                    }),
            )
    }

    // ----- Edit manager → Plugins --------------------------------------------------------------

    fn owner_set_plugins(&mut self, owner: Owner, enabled: Vec<String>, cx: &mut Context<Self>) {
        if self.plugins_read_only().is_some() {
            return;
        }
        let (on, missing) = match owner {
            Owner::Manager(id) => match self.plugins.project_managers.iter_mut().find(|m| m.employee_id == id).or(self.plugins.manager.as_mut().filter(|m| m.employee_id == id)) {
                Some(m) => (&mut m.enabled, &mut m.missing),
                None => return,
            },
            Owner::Project(_) => match self.plugins.project.as_mut() {
                Some(p) => (&mut p.enabled, &mut p.missing),
                None => return,
            },
        };
        *on = enabled.clone();
        missing.retain(|id| enabled.contains(id));
        self.plugins_send(match owner {
            Owner::Manager(id) => PluginsCall::ManagerSet(id, enabled),
            Owner::Project(id) => PluginsCall::ProjectSet(id, enabled),
        });
        cx.notify();
    }

    /// The Plugins section of the Edit manager dialog. Switches save at once.
    pub fn manager_plugins_section(&self, manager: EmployeeId, cx: &mut Context<Self>) -> Div {
        self.plugin_switches(Owner::Manager(manager), "Plugins".into(), cx)
    }

    fn manager_plugins_of(&self, id: EmployeeId) -> Option<&EmployeePlugins> {
        self.plugins.project_managers.iter().find(|m| m.employee_id == id).or(self.plugins.manager.as_ref().filter(|m| m.employee_id == id))
    }

    /// A switch per installed plugin for a manager or a project. A manager's plugins that come
    /// only from its project show 'From project' with a locked switch; see `from_project_label`.
    pub fn plugin_switches(&self, owner: Owner, title: SharedString, cx: &mut Context<Self>) -> Div {
        let about_id = SharedString::from(format!("plugins-about-{title}"));
        let title = div().text_size(px(12.)).text_color(c(theme::TEXT_2)).child(title);
        if let Some(why) = self.plugins_unavailable() {
            return v().gap(px(6.)).child(title).child(div().text_size(px(11.5)).text_color(c(theme::MUTED)).child(why));
        }
        let title = h().gap(px(6.)).items_center().child(title).child(ui::info(about_id, "Switches save at once and apply from the next run. A plugin with hooks runs commands on the server's PC."));
        let read_only = self.plugins_read_only();
        let can_change = read_only.is_none();
        let installed = self.plugin_installed_list();
        let state = match owner {
            Owner::Manager(id) => self.manager_plugins_of(id).map(|m| (m.enabled.clone(), m.missing.clone())),
            Owner::Project(id) => self.plugins.project.as_ref().filter(|p| p.project_id == id).map(|p| (p.enabled.clone(), p.missing.clone())),
        };
        let loading = state.is_none() || self.plugins.installed.is_none();
        let (enabled, missing) = state.unwrap_or_default();
        // Employees are company-wide in 3.0: they inherit nothing from a project.
        let inherited: Vec<String> = Vec::new();
        let (prefix, forget_prefix) = match owner {
            Owner::Manager(_) => ("manager-plugin", "manager-plugin-forget"),
            Owner::Project(_) => ("project-plugin", "project-plugin-forget"),
        };
        let rows = installed.iter().enumerate().map(|(i, p)| {
            let on = enabled.contains(&p.id);
            let pid = p.id.clone();
            let enabled_now = enabled.clone();
            let from_project = from_project_label(on, inherited.contains(&p.id));
            let locked = from_project.is_some_and(|(_, locked)| locked);
            h().gap(px(10.)).items_start()
                // From the project only: on, but only the project's list turns it off.
                .when(locked, |el| el.child(switch((prefix, i), true, &p.name).opacity(0.45)))
                .when(!locked, |el| el.child(
                    switch((prefix, i), on, &p.name).when(!can_change || loading, |el| el.opacity(0.45)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        let mut next = enabled_now.clone();
                        match next.iter().position(|x| *x == pid) {
                            Some(at) => { next.remove(at); }
                            None => next.push(pid.clone()),
                        }
                        this.owner_set_plugins(owner, next, cx);
                    })),
                ))
                .child(
                    v().flex_1().min_w_0().gap(px(3.))
                        .child(h().gap(px(8.)).child(div().min_w_0().truncate().text_size(px(13.)).child(p.name.clone())).child(div().text_size(px(11.5)).text_color(c(theme::MUTED)).child(p.version.clone())).when_some(from_project, |el, (label, _)| el.child(pchip(label, theme::TEXT_2))))
                        .when(!p.description.is_empty(), |el| el.child(div().text_size(px(11.5)).text_color(c(theme::MUTED)).child(p.description.clone())))
                        .child(component_chips(Some(&p.components)))
                        .when(p.components.hooks, |el| el.child(div().text_size(px(11.5)).text_color(c(theme::NEGATIVE)).child("Runs hooks (commands on this PC)")))
                        .when(!p.components.mcp_servers.is_empty(), |el| el.child(div().text_size(px(11.5)).text_color(c(theme::MUTED)).child("MCP servers not loaded"))),
                )
        });
        let missing_rows = missing.into_iter().enumerate().map(|(i, id)| {
            let drop = id.clone();
            let enabled_now = enabled.clone();
            h().gap(px(10.))
                .child(div().flex_1().min_w_0().truncate().text_size(px(12.5)).text_color(c(theme::NEGATIVE)).child(format!("{id} is turned on but no longer installed; it is skipped in runs.")))
                .child(ui::button((forget_prefix, i), "Turn off", ButtonKind::Outline, None).h(px(26.)).when(!can_change, |el| el.opacity(0.45)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    let next: Vec<String> = enabled_now.iter().filter(|x| **x != drop).cloned().collect();
                    this.owner_set_plugins(owner, next, cx);
                })))
        });
        v().gap(px(8.))
            .child(
                h().gap(px(8.)).child(div().flex_1().child(title))
                    .child(ui::button("owner-plugin-add", "Add plugin…", ButtonKind::Outline, None).h(px(28.)).when(!can_change, |el| el.opacity(0.45)).on_click(cx.listener(move |this, _: &ClickEvent, w, cx| {
                        if this.plugins_read_only().is_none() {
                            this.open_add_plugin_for(owner, w, cx);
                        }
                    }))),
            )
            .when(loading && self.plugins.error.is_none(), |el| el.child(div().text_size(px(11.5)).text_color(c(theme::MUTED)).child("Loading…")))
            .when(!loading && installed.is_empty(), |el| el.child(div().text_size(px(11.5)).text_color(c(theme::MUTED)).child("No plugins installed yet. Add one for extra skills.")))
            .child(v().gap(px(10.)).children(rows))
            .children(missing_rows)
            .when_some(read_only, |el, why| el.child(div().text_size(px(11.5)).text_color(c(theme::MUTED)).child(why)))
            .when_some(self.plugins.error.clone(), |el, why| el.child(div().text_size(px(11.5)).text_color(c(theme::NEGATIVE)).child(why)))
    }
}

#[cfg(test)]
mod tests {
    use super::{PLUGINS_TAB, from_project_label, parse_source};

    #[test]
    fn only_a_plugin_from_the_project_alone_is_locked() {
        assert_eq!(from_project_label(true, true), Some(("Also from project", false)));
        assert_eq!(from_project_label(false, true), Some(("From project", true)));
        assert_eq!(from_project_label(true, false), None);
        assert_eq!(from_project_label(false, false), None);
    }

    #[test]
    fn links_are_github_owner_repo_or_urls() {
        assert!(parse_source("  owner/repo ").is_ok());
        assert!(parse_source("https://github.com/owner/repo").is_ok());
        assert!(parse_source("git@github.com:owner/repo.git").is_ok());
        assert!(parse_source("").is_err());
        assert!(parse_source("justaword").is_err());
        assert!(parse_source("C:\\plugins\\mine").is_err());
        assert!(parse_source("/home/me/plugins").is_err());
        assert!(parse_source("./plugins").is_err());
    }

    #[test]
    fn the_plugins_tab_follows_the_api_keys_tab() {
        assert_eq!(PLUGINS_TAB, crate::screens::app_keys::APP_KEYS_TAB + 1);
    }
}
