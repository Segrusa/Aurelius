//! Command palette (Ctrl+K), shortcut overview (Ctrl+/), project dialog and approval review.

use crate::screens::updates;
use crate::app::{AppView, ConfirmKind, DialogKind, InboxTab, ManagerRef, NoteKind, Overlay, ProjectRef, note_kind};
use crate::screens::approval::approval_summary;
use crate::keymap::*;
use crate::preview;
use crate::theme::{self, c};
use crate::ui::{self, ButtonKind, h, v};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use gpui_kit::component::scroll::ScrollableElement as _;

impl AppView {
    pub fn render_overlay(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        // A board dialog, with its dropdown menus on top of it.
        if self.board.dialog.is_some() {
            let dialog = self.render_board_dialog(window, cx);
            let menu = (self.overlay == Overlay::Menu).then(|| self.render_menu(window, cx));
            return div().absolute().top_0().left_0().size_full().child(dialog).children(menu).into_any_element();
        }
        let under = if self.overlay == Overlay::None { div().into_any_element() } else { self.render_overlay_layer(window, cx) };
        if self.mcp_manage.add.is_some() {
            let add = self.render_add_mcp(window, cx);
            return div().absolute().top_0().left_0().size_full().child(under).child(add).into_any_element();
        }
        // Project settings; Add plugin can open on top of it.
        let under = match self.project_settings {
            Some(_) => {
                let card = self.render_project_settings(window, cx);
                div().absolute().top_0().left_0().size_full().child(under).child(card).into_any_element()
            }
            None => under,
        };
        if self.plugins.add.is_none() {
            return under;
        }
        // Add plugin sits on top of Settings or Edit manager, which stay as they are.
        let add = self.render_add_plugin(window, cx);
        div().absolute().top_0().left_0().size_full().child(under).child(add).into_any_element()
    }

    fn render_overlay_layer(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        if let Overlay::PlanApproval(id) = self.overlay {
            return self.render_plan_approval_panel(id, cx);
        }
        if self.overlay == Overlay::Menu {
            let menu = self.render_menu(window, cx);
            // A dropdown of a dialog: the dialog stays visible beneath it.
            let Some(parent) = self.menu.as_ref().and_then(|m| m.parent.as_ref()).map(|(o, _)| *o) else { return menu };
            self.overlay = parent;
            let under = self.render_overlay_layer(window, cx);
            self.overlay = Overlay::Menu;
            return div().absolute().top_0().left_0().size_full().child(under).child(menu).into_any_element();
        }
        if self.overlay == Overlay::Screenshot {
            return self.render_screenshot_panel(cx);
        }
        if self.overlay == Overlay::Markdown {
            return self.render_markdown_panel(cx);
        }
        if self.overlay == Overlay::Trace {
            return self.render_trace_panel(cx);
        }
        if self.overlay == Overlay::Artifact {
            return self.render_artifact_panel(window, cx);
        }
        // Every dialog fits the window (below the scrim's top padding, with a margin):
        // a taller card shrinks and its middle part scrolls; header and footer stay.
        // Every scroll body also gets its size from this (max_h minus its own header and
        // footer): a squeezed gpui-component scroll area takes its content height and won't
        // scroll. A body that is as tall as its content up to that limit uses scroll_body().
        let top = 110.;
        let max_h = (f32::from(window.viewport_size().height) - top - 24.).max(160.);
        // Likewise in width: a card never gets wider than the window minus a margin
        // (card() caps itself at its parent), so a narrow window doesn't cut it off.
        let viewport_w = f32::from(window.viewport_size().width);
        let max_w = (viewport_w - 32.).max(280.);
        let ok = self.overlay_key();
        let scroll = window.use_keyed_state(("overlay-scroll", ok), cx, |_, _| ScrollHandle::default()).read(cx).clone();
        let content: AnyElement = match self.overlay {
            Overlay::Palette => self.render_palette(max_h, &scroll, cx).into_any_element(),
            Overlay::Dialog(kind) => self.render_dialog(kind, max_h, &scroll, cx).into_any_element(),
            Overlay::Review => self.render_review(max_h, &scroll, cx).into_any_element(),
            Overlay::Confirm(kind) => self.render_confirm(kind, max_h, &scroll, cx).into_any_element(),
            Overlay::Context => self.render_context(max_h, &scroll, cx).into_any_element(),
            Overlay::Usage => self.render_usage(max_h, cx).into_any_element(),
            Overlay::ServerFolder => self.render_server_folder(max_h, window, cx).into_any_element(),
            Overlay::ConnectClaude => self.render_connect_dialog(max_h, cx).into_any_element(),
            Overlay::SignedOut => self.render_signed_out(cx).into_any_element(),
            Overlay::Guidelines => self.render_guidelines_dialog(max_h, cx),
            Overlay::None | Overlay::PlanApproval(_) | Overlay::Menu | Overlay::Screenshot | Overlay::Markdown | Overlay::Trace | Overlay::Artifact => div().into_any_element(),
        };
        deferred(ui::fade_in(
            div()
                .id("scrim")
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .bg(Hsla { a: 0.55, ..c(0x000000) })
                .flex()
                .justify_center()
                .items_start()
                .pt(px(top))
                .occlude()
                .on_mouse_down(MouseButton::Left, cx.listener(|this, _, w, cx| this_back(this, w, cx)))
                .child(ui::rise(v().max_h(px(max_h)).max_w(px(max_w)).on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()).child(content), ("overlay", ok), 0)),
            ("scrim", ok),
            160,
        ))
        .with_priority(1)
        .into_any_element()
    }

    /// The Settings page: the section list on the left, the open section beside it.
    pub fn render_settings_page(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        // The app sidebar takes 220 px too, so the section list narrows sooner than it did in the dialog.
        let narrow = f32::from(window.viewport_size().width) < 1180.;
        let section = if self.settings_tab == crate::screens::load::LOAD_TAB {
            self.render_load_settings(window, cx).into_any_element()
        } else if self.settings_tab == crate::screens::account::ACCOUNT_TAB {
            let page = settings_page().track_focus(&self.focus).key_context(LOAD);
            self.render_account_settings(page, window, cx).into_any_element()
        } else {
            self.render_settings(cx).into_any_element()
        };
        // Servers, Remote access and Server & PC are one section: tabs above the open page.
        let section = if SERVER_TABS.contains(&self.settings_tab) {
            v().flex_1().min_w_0().h_full().child(self.server_tabs(cx)).child(section).into_any_element()
        } else {
            section
        };
        h().size_full().min_w_0().items_stretch().child(self.settings_nav(narrow, cx)).child(section).into_any_element()
    }

    pub fn render_inbox_page(&mut self, cx: &mut Context<Self>) -> AnyElement {
        self.render_inbox(cx).into_any_element()
    }

    /// "Employee · Project · Task" of an inbox row, from its task when it has one
    /// (else `fallback`, the notification's subject).
    fn inbox_meta(&self, who: Option<&str>, task: Option<aurelius_domain::TaskId>, fallback: &str) -> String {
        let task = task.and_then(|id| self.tasks.iter().find(|t| t.id == id));
        let who = who.map(str::to_owned).or_else(|| task.and_then(|t| t.employee_name.clone()));
        let project = task.and_then(|t| self.projects.iter().find(|p| p.id == t.project_id)).map(|p| p.name.clone());
        let what = task.map(|t| t.title.clone()).or_else(|| Some(fallback.to_owned()));
        [who, project, what].into_iter().flatten().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · ")
    }

    /// The detail's context: who asked, the project and the task (each opens what it names),
    /// the conversation, and when ("Today 14:05 · 5m ago").
    fn inbox_context(&self, who: Option<String>, task: Option<aurelius_domain::TaskId>, conversation: Option<aurelius_domain::ConversationId>, at_ms: i64, now: i64, cx: &mut Context<Self>) -> Div {
        let task = task.and_then(|id| self.tasks.iter().find(|t| t.id == id));
        let who = who.filter(|w| !w.is_empty()).or_else(|| task.and_then(|t| t.employee_name.clone()));
        let employee = task.and_then(|t| t.employee_id).or_else(|| who.as_ref().and_then(|w| self.managers.iter().find(|m| &m.name == w)).map(|m| m.id));
        let project = task.and_then(|t| self.projects.iter().find(|p| p.id == t.project_id));
        let conversation = conversation.or_else(|| task.and_then(|t| t.conversation_id));
        let link = |id: &'static str, text: String| div().id(id).cursor_pointer().text_color(c(theme::ACCENT_TEXT)).hover(|s| s.text_color(c(theme::TEXT))).child(text);
        let mut parts: Vec<AnyElement> = Vec::new();
        if let Some(who) = who {
            parts.push(match employee {
                Some(id) => link("inbox-employee", who).on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.open_manager(ManagerRef::Real(id), w, cx))).into_any_element(),
                None => div().child(who).into_any_element(),
            });
        }
        if let Some(p) = project {
            let id = p.id;
            parts.push(link("inbox-project", p.name.clone()).on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.open_project(ProjectRef::Real(id), w, cx))).into_any_element());
        }
        if let Some(t) = task {
            let id = t.id;
            parts.push(link("inbox-task", t.title.clone()).on_click(cx.listener(move |this, _: &ClickEvent, w, cx| {
                this.show_task(id, w, cx);
                this.focus.focus(w, cx);
                cx.notify();
            })).into_any_element());
        }
        if let Some(id) = conversation {
            parts.push(link("inbox-conversation", "Open conversation".into()).on_click(cx.listener(move |this, _: &ClickEvent, w, cx| {
                this.open_conversation_anywhere(id, w, cx);
                cx.notify();
            })).into_any_element());
        }
        let mut line = h().gap(px(6.)).flex_wrap().text_size(px(13.)).text_color(c(theme::TEXT_2));
        for (i, part) in parts.into_iter().enumerate() {
            if i > 0 {
                line = line.child(div().text_color(c(theme::MUTED)).child("·"));
            }
            line = line.child(part);
        }
        v().gap(px(4.)).child(line).child(div().text_size(px(12.)).text_color(c(theme::MUTED)).child(format!("{} · {}", ui::fmt_when(at_ms), ui::relative_age(at_ms, now))))
    }

    /// Title of the open settings section; what it is for sits behind an info icon.
    pub(crate) fn settings_title(&self, cx: &mut Context<Self>) -> Div {
        let tab = self.settings_tab.min(SETTINGS_SECTIONS.len() - 1);
        h().flex_1().min_w_0().gap(px(8.)).items_center()
            .child(div().font_family(ui::serif(cx)).text_size(px(21.)).child(SETTINGS_SECTIONS[tab]))
            .child(ui::info(("settings-about", tab), SETTINGS_ABOUT[tab]))
    }

    /// The four Settings sections as a list on the left (Tab also moves between the pages), the
    /// pages of Advanced below it while it is open. On a narrow window the list gets narrower,
    /// so the open page keeps room.
    fn settings_nav(&self, narrow: bool, cx: &mut Context<Self>) -> Div {
        let active = self.settings_tab;
        let current = nav_section(active);
        let row = |id: (&'static str, usize), label: &'static str, on: bool, indent: bool, to: usize, cx: &mut Context<Self>| {
            h().id(id)
                .role(Role::Tab)
                .aria_label(label)
                .gap(px(9.))
                .pl(px(if indent { 22. } else { 8. }))
                .pr(px(10.))
                .py(px(7.))
                .rounded(px(6.))
                .cursor_pointer()
                .text_size(px(13.))
                .text_color(c(if on { theme::TEXT } else { theme::TEXT_2 }))
                .when(on, |el| el.bg(c(theme::SURFACE_2)))
                .hover(|s| s.bg(c(theme::SURFACE_2)).text_color(c(theme::TEXT)))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.set_settings_tab(to, cx)))
                .child(div().w(px(3.)).h(px(14.)).flex_none().rounded(px(2.)).when(on, |el| el.bg(c(theme::ACCENT))))
                .child(div().flex_1().min_w_0().truncate().child(label))
        };
        let mut sections = v().gap(px(1.));
        for (i, (label, first)) in SETTINGS_NAV.iter().enumerate() {
            // Advanced itself is not highlighted while one of its pages is: the page is.
            let advanced_open = i == ADVANCED && current == ADVANCED;
            sections = sections.child(row(("settings-section", i), *label, i == current && !advanced_open, false, *first, cx));
            if advanced_open {
                for (j, tab) in ADVANCED_TABS.iter().enumerate() {
                    sections = sections.child(row(("settings-tab", j), SETTINGS_SECTIONS[*tab], *tab == active, true, *tab, cx));
                }
            }
        }
        v().w(px(if narrow { 160. } else { 216. }))
            .flex_none()
            .bg(c(theme::SIDEBAR))
            .border_r_1()
            .border_color(c(theme::BORDER))
            .px(px(10.))
            .pt(px(20.))
            .pb(px(14.))
            .child(div().px(px(10.)).pb(px(10.)).font_family(ui::serif(cx)).text_size(px(22.)).child("Settings"))
            .child(v().id("settings-nav").flex_1().min_h_0().overflow_y_scroll().gap(px(14.)).child(sections))
    }

    /// "Servers & PC": a row of tabs (Servers, Remote access, Server & PC) above these three pages.
    fn server_tabs(&self, cx: &mut Context<Self>) -> Div {
        let labels = [(SERVER_TABS[0], "Servers"), (SERVER_TABS[1], "Remote access"), (SERVER_TABS[2], "Server & PC")];
        h().flex_none().h(px(44.)).px(px(22.)).gap(px(2.)).border_b_1().border_color(c(theme::BORDER)).bg(c(theme::BG))
            .children(labels.into_iter().enumerate().map(|(i, (tab, label))| {
                let on = self.settings_tab == tab;
                h().id(("server-tab", i)).role(Role::Tab).aria_label(label).flex_none().px(px(12.)).h(px(30.)).mt(px(7.)).rounded(px(7.)).cursor_pointer().text_size(px(13.))
                    .text_color(c(if on { theme::TEXT } else { theme::MUTED }))
                    .when(on, |el| el.bg(c(theme::SURFACE_2)).font_weight(FontWeight::MEDIUM))
                    .hover(|s| s.text_color(c(theme::TEXT)).bg(c(theme::SURFACE)))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.set_settings_tab(tab, cx)))
                    .child(label)
            }))
    }

    /// A dropdown field: label, current value and ▾; a click (or its shortcut) opens
    /// the options right below it (`open` is the action that opens them).
    pub(crate) fn dropdown(&self, id: &'static str, label: &'static str, value: String, set: bool, keys: &'static str, open: fn() -> Box<dyn Action>, cx: &mut Context<Self>) -> Stateful<Div> {
        let bounds = self.dropdown_bounds.clone();
        let open_now = self.overlay == Overlay::Menu && self.menu.as_ref().is_some_and(|m| m.field == Some(id));
        ui::dropdown_trigger(id, 36., set, open_now)
            .aria_label(label)
            .gap(px(10.))
            .on_click(cx.listener(move |_, _: &ClickEvent, w, cx| w.dispatch_action(open(), cx)))
            .child(div().min_w(px(56.)).flex_none().whitespace_nowrap().text_size(px(12.)).text_color(c(theme::TEXT_2)).child(label))
            .child(div().flex_1().min_w_0().truncate().text_size(px(13.)).text_color(c(if set { theme::TEXT } else { theme::MUTED })).child(value))
            .child(ui::kbd(keys, cx))
            .child(ui::caret())
            .child(ui::track_bounds(bounds, id))
    }

    /// Popup menu at the click position, kept inside the window.
    fn render_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(menu) = self.menu.as_ref() else { return div().into_any_element() };
        let natural: f32 = if menu.entries.iter().flatten().any(|e| e.detail.is_some()) { 360. } else { 240. };
        // At least as wide as the trigger it belongs to.
        let width = menu.width.map_or(natural, |w| w.max(natural.min(280.)).max(180.));
        let content = menu.entries.iter().map(|e| if e.is_some() { 32. } else { 9. }).sum::<f32>() + 8.;
        let size = window.viewport_size();
        let (vw, vh) = (f32::from(size.width), f32::from(size.height));
        let below = |y: f32| vh - 8. - y;
        let anchor_y = f32::from(menu.anchor.y);
        // Opens upward from the trigger when it fits better there; the list scrolls past `max_h`.
        let up = menu.trigger_top.is_some_and(|top| content > below(anchor_y) && top - 8. > below(anchor_y));
        let room = if up { menu.trigger_top.unwrap_or(anchor_y) - 12. } else { below(anchor_y) };
        let max_h = room.min(360.).max(96.);
        let height = content.min(max_h);
        let x = f32::from(menu.anchor.x).min(vw - width - 8.).max(8.);
        let y = match menu.trigger_top {
            Some(top) if up => (top - 4. - height).max(8.),
            _ if anchor_y + height > vh - 8. => (anchor_y - height).max(8.),
            _ => anchor_y,
        };
        let selected = menu.selected;
        let rows: Vec<AnyElement> = menu
            .entries
            .iter()
            .enumerate()
            .map(|(i, e)| match e {
                None => div().h(px(1.)).my(px(4.)).bg(c(theme::BORDER)).into_any_element(),
                Some(e) => h()
                    .id(("menu-item", i))
                    .role(Role::MenuItem)
                    .aria_label(e.label.clone())
                    .w_full()
                    .flex_none()
                    .h(px(32.))
                    .px(px(14.))
                    .gap(px(10.))
                    .cursor_pointer()
                    .text_size(px(13.))
                    .text_color(c(if e.danger { theme::NEGATIVE } else { theme::TEXT }))
                    .when(i == selected, |el| el.bg(c(theme::SURFACE_2)))
                    .hover(|s| s.bg(c(theme::SURFACE_2)))
                    .on_mouse_move(cx.listener(move |this, _, _, cx| {
                        if let Some(m) = this.menu.as_mut().filter(|m| m.selected != i) {
                            m.selected = i;
                            cx.notify();
                        }
                    }))
                    .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.run_menu_entry(i, w, cx)))
                    .child(div().flex_1().min_w_0().truncate().child(e.label.clone()))
                    .when_some(e.detail.clone(), |el, d| el.child(div().flex_none().text_size(px(11.5)).text_color(c(theme::MUTED)).child(d)))
                    .when_some(e.keys, |el, k| el.child(ui::kbd_always(k, cx).opacity(0.7)))
                    .when(e.checked, |el| el.child(div().flex_none().w(px(12.)).text_color(c(theme::ACCENT_TEXT)).child("✓")))
                    .into_any_element(),
            })
            .collect();
        deferred(
            div()
                .id("menu-scrim")
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .occlude()
                .on_mouse_down(MouseButton::Left, cx.listener(|this, _, w, cx| this_back(this, w, cx)))
                .on_mouse_down(MouseButton::Right, cx.listener(|this, _, w, cx| this_back(this, w, cx)))
                .child(ui::fade_in(
                    v().id("menu")
                        .role(Role::Menu)
                        .absolute()
                        .left(px(x))
                        .top(px(y))
                        .w(px(width))
                        .h(px(height))
                        .py(px(4.))
                        .overflow_hidden()
                        .rounded(px(9.))
                        .border_1()
                        .border_color(c(theme::BORDER_STRONG))
                        .bg(c(theme::PANEL))
                        .shadow_lg()
                        .track_focus(&self.overlay_focus)
                        .key_context(MENU)
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(v().id("menu-rows").size_full().overflow_y_scroll().track_scroll(&self.menu_scroll).children(rows)),
                    "menu-fade",
                    90,
                )),
        )
        .with_priority(2)
        .into_any_element()
    }

    fn render_palette(&mut self, max_h: f32, scroll: &ScrollHandle, cx: &mut Context<Self>) -> impl IntoElement {
        let entries = self.palette_entries(cx);
        let selected = self.palette_selected.min(entries.len().saturating_sub(1));
        card(620.)
            .key_context(PALETTE)
            .child(div().px(px(14.)).py(px(10.)).border_b_1().border_color(c(theme::BORDER)).child(ui::input(&self.palette_input).appearance(false)))
            .child(
                ui::scroll_frame(scroll, scroll_body("palette-list", scroll, (max_h - 100.).clamp(120., 420.)).p(px(6.))
                    .when(entries.is_empty(), |el| el.child(div().p(px(14.)).text_color(c(theme::MUTED)).child("Nothing found.")))
                    .children(entries.iter().enumerate().map(|(i, entry)| {
                        let active = i == selected;
                        h().id(("palette", i))
                            .px(px(12.))
                            .py(px(8.))
                            .gap(px(10.))
                            .rounded(px(6.))
                            .cursor_pointer()
                            .when(active, |el| el.bg(c(theme::SURFACE_2)))
                            .hover(|s| s.bg(c(theme::SURFACE_2)))
                            .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.run_palette_entry(i, w, cx)))
                            .child(div().flex_1().text_size(px(13.5)).child(entry.label.clone()))
                            .child(div().text_size(px(11.5)).text_color(c(theme::MUTED)).child(entry.hint.clone()))
                            .when_some(entry.keys, |el, keys| el.child(ui::kbd_always(keys, cx)))
                    }))),
            )
            .child(
                h().px(px(14.)).py(px(8.)).gap(px(14.)).border_t_1().border_color(c(theme::BORDER)).text_size(px(11.)).text_color(c(theme::MUTED))
                    .child(h().gap(px(5.)).child(ui::kbd_always("up", cx)).child(ui::kbd_always("down", cx)).child("select"))
                    .child(h().gap(px(5.)).child(ui::kbd_always("enter", cx)).child("run"))
                    .child(h().gap(px(5.)).child(ui::kbd_always("escape", cx)).child("close")),
            )
    }

    fn render_dialog(&mut self, kind: DialogKind, max_h: f32, scroll: &ScrollHandle, cx: &mut Context<Self>) -> impl IntoElement {
        let (title, confirm) = match kind {
            DialogKind::NewProject => ("New project", "Create project"),
            DialogKind::NewManager => ("Add employee", "Add"),
            DialogKind::NewTask { .. } => ("New task", "Start task"),
            DialogKind::TaskSchedule { schedule: None, .. } => ("New recurring task", "Save recurring task"),
            DialogKind::TaskSchedule { schedule: Some(_), .. } => ("Edit recurring task", "Save"),
            DialogKind::EditManager { .. } => ("Edit employee", "Save"),
            DialogKind::NewMemory { .. } => ("Remember for this project", "Remember"),
            DialogKind::NewIdea { .. } => ("New idea", "Add idea"),
            DialogKind::EditIdea { .. } => ("Edit idea", "Save"),
            DialogKind::ReviseMemory { .. } => ("Revise memory", "Save new version"),
            DialogKind::QuestionAnswer { .. } => ("Your own answer", "Use this answer"),
            DialogKind::EditProposal { .. } => ("Edit and approve instructions", "Approve these"),
        };
        let is_proposal = matches!(kind, DialogKind::EditProposal { .. });
        let versions = match kind {
            DialogKind::EditManager { id, .. } if self.instruction_proposals_on() => Some(self.instruction_versions_section(id, cx)),
            _ => None,
        };
        let asked = match kind {
            DialogKind::QuestionAnswer { approval, index, .. } => self
                .approvals
                .iter()
                .find(|a| a.id == approval)
                .and_then(|a| crate::app::questions_of(a).get(index).map(|q| q.text.clone())),
            _ => None,
        };
        let is_memory = matches!(kind, DialogKind::NewMemory { .. } | DialogKind::ReviseMemory { .. });
        let is_manager = matches!(kind, DialogKind::NewManager | DialogKind::EditManager { .. });
        let is_task = matches!(kind, DialogKind::NewTask { .. } | DialogKind::TaskSchedule { .. });
        let is_idea = matches!(kind, DialogKind::NewIdea { .. } | DialogKind::EditIdea { .. });
        let multi_field = is_manager || is_task || is_idea || kind == DialogKind::NewProject;
        // What the dialog is for and how it saves: behind an info icon next to the title.
        let about: Option<&str> = if is_proposal {
            Some("This text replaces the employee's instructions as a new version; the proposer is told it was edited. Enter adds a line; Ctrl+Enter approves.")
        } else if kind == DialogKind::NewProject {
            Some("A new project starts empty and works without an employee or repository.")
        } else if is_idea {
            Some("For later. When its time comes, start it as a task for an employee (Ctrl+T in Ideas). Enter adds a line; Ctrl+Enter saves.")
        } else if matches!(kind, DialogKind::NewMemory { .. }) {
            Some("You are the source. Employees of this project use this as context; you can revise or forget it later.")
        } else if matches!(kind, DialogKind::ReviseMemory { .. }) {
            Some("The old version stays visible in the history (filter 3) but is no longer used.")
        } else if matches!(kind, DialogKind::TaskSchedule { .. }) {
            Some("Each time it is due, a normal task is made from this for the employee. If the previous one is still open, that time is skipped. Enter adds a line; Ctrl+Enter saves. Newlines are kept as typed.")
        } else if is_task {
            Some("The employee works on it in a conversation of its own, may give parts of it to colleagues, and reports back. The criteria say when it is done. Enter adds a line; Ctrl+Enter saves. Newlines are kept as typed.")
        } else if is_manager {
            Some("Changes apply to new runs; a running run keeps its configuration.")
        } else {
            None
        };
        let idea_files = is_idea && self.idea_files_on();
        card(480.)
            .key_context(DIALOG)
            // Files dropped on a task or idea dialog are attached to it.
            .when(is_task || idea_files, |el| {
                el.drag_over::<ExternalPaths>(|style, _, _, _| style.bg(c(theme::SURFACE)))
                    .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| this.add_task_files(paths.paths().to_vec(), cx)))
            })
            .child(
                ui::scroll_frame(scroll, scroll_body("dialog-body", scroll, (max_h - 70.).max(120.)).p(px(22.)).gap(px(14.))
                    .child(h().gap(px(8.)).items_center().child(div().font_family(ui::serif(cx)).text_size(px(22.)).child(title)).when_some(about, |el, text| el.child(ui::info("dialog-about", text))))
                    .when_some(asked.clone(), |el, q| el.child(div().text_size(px(13.5)).text_color(c(theme::TEXT_2)).child(q)))
                    .when(!is_proposal, |el| el.child(field(
                        if is_memory { "What should Aurelius remember?" } else if is_idea { "Idea" } else if asked.is_some() { "Your answer" } else if is_task { "Title" } else { "Name" },
                        ui::input(&self.name_input).into_any_element(),
                    )))
                    .when(is_proposal, |el| {
                        el.child(field("Instructions", gpui_kit::component::input::Textarea::new(&self.goal_input).py(px(7.)).line_height(px(20.)).into_any_element()))
                    })
                    .when(is_idea, |el| {
                        // The note has several lines; with files on, pasting an image or files attaches them.
                        let this = cx.entity().downgrade();
                        let mut note = gpui_kit::component::input::Textarea::new(&self.goal_input).py(px(7.)).line_height(px(22.));
                        if idea_files {
                            note = note.on_paste(move |item, _, cx| this.update(cx, |this, cx| this.paste_into_task_goal(item, cx)).unwrap_or(false));
                        }
                        el.child(field("Note", note.into_any_element()))
                            .when(idea_files, |el| el.child(self.task_files_row(cx)))
                            .map(|el| {
                                use aurelius_domain::idea::IdeaKind;
                                el.child(field("Kind", h().gap(px(6.)).children([(IdeaKind::Feature, "Feature"), (IdeaKind::Bug, "Bug")].into_iter().map(|(k, label)| {
                                    let on = self.pending_idea_kind == k;
                                    div().id(("idea-kind-pick", k as usize)).px(px(12.)).py(px(5.)).rounded(px(6.)).cursor_pointer().border_1().text_size(px(12.5))
                                        .border_color(c(if on { theme::FOCUS_RING } else { theme::BORDER }))
                                        .when(on, |el| el.bg(c(theme::SURFACE_2)))
                                        .text_color(c(if on { theme::TEXT } else { theme::TEXT_2 }))
                                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                            this.pending_idea_kind = k;
                                            cx.notify();
                                        }))
                                        .child(label)
                                })).into_any_element()))
                            })
                            .when(matches!(kind, DialogKind::NewIdea { .. }), |el| {
                                // An idea always belongs to a project: choose which.
                                let chosen = self.pending_idea_project;
                                el.child(field("Project", h().flex_wrap().gap(px(6.)).children(self.projects.iter().enumerate().map(|(i, p)| {
                                    let (id, on) = (p.id, chosen == Some(p.id));
                                    div().id(("idea-project-pick", i)).px(px(12.)).py(px(5.)).rounded(px(6.)).cursor_pointer().border_1().text_size(px(12.5))
                                        .border_color(c(if on { theme::FOCUS_RING } else { theme::BORDER }))
                                        .when(on, |el| el.bg(c(theme::SURFACE_2)))
                                        .text_color(c(if on { theme::TEXT } else { theme::TEXT_2 }))
                                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                            this.pending_idea_project = Some(id);
                                            cx.notify();
                                        }))
                                        .child(p.name.clone())
                                })).into_any_element()))
                            })
                    })
                    .when(matches!(kind, DialogKind::NewMemory { .. }), |el| {
                        el.child(field("Kind", h().gap(px(6.)).children([(false, "Knowledge"), (true, "Decision")].into_iter().map(|(decision, label)| {
                            let on = self.pending_memory_decision == decision;
                            div().id(("memory-kind-pick", decision as usize)).px(px(12.)).py(px(5.)).rounded(px(6.)).cursor_pointer().border_1().text_size(px(12.5))
                                .role(Role::Button).aria_label(SharedString::from(format!("Kind: {label}")))
                                .border_color(c(if on { theme::FOCUS_RING } else { theme::BORDER }))
                                .when(on, |el| el.bg(c(theme::SURFACE_2)))
                                .text_color(c(if on { theme::TEXT } else { theme::TEXT_2 }))
                                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                    this.pending_memory_decision = decision;
                                    cx.notify();
                                }))
                                .child(label)
                        })).into_any_element()))
                    })
                    .when(kind == DialogKind::NewProject, |el| el.child(field("Description", ui::input(&self.description_input).into_any_element())))
                    .when(matches!(kind, DialogKind::NewProject | DialogKind::NewManager), |el| {
                        let required = kind == DialogKind::NewProject;
                        let chosen = self.dialog_folder.clone();
                        el.child(
                            v().gap(px(6.))
                                .child(div().text_size(px(12.)).text_color(c(theme::TEXT_2)).child(if required { div().child("Project folder (required)") } else { div().child(ui::label_with_info("Own folder (optional)", "Where its chat works. In a task the project folder applies.")) }))
                                .child(
                                    h().gap(px(10.)).px(px(10.)).py(px(8.)).rounded(px(6.)).border_1()
                                        .border_color(c(if required && chosen.is_none() { theme::ACCENT } else { theme::BORDER }))
                                        .child(ui::folder_glyph(theme::MUTED))
                                        .child(div().flex_1().min_w_0().font_family(ui::mono(cx)).text_size(px(11.5))
                                            .text_color(c(if chosen.is_some() { theme::TEXT } else { theme::MUTED }))
                                            .child(chosen.unwrap_or_else(|| "No folder chosen".into())))
                                        .child(
                                            ui::button("choose-folder", "Choose…", ButtonKind::Outline, Some(ui::kbd("secondary-o", cx)))
                                                .h(px(28.))
                                                .on_click(cx.listener(|_, _: &ClickEvent, w, cx| w.dispatch_action(Box::new(ChooseDialogFolder), cx))),
                                        ),
                                ),
                        )
                    })
                    .when(is_task, |el| {
                        let schedule = matches!(kind, DialogKind::TaskSchedule { .. });
                        let editing = matches!(kind, DialogKind::TaskSchedule { schedule: Some(_), .. });
                        let chosen = self.pending_task_manager.and_then(|m| self.managers.iter().find(|x| x.id == m)).map(|m| format!("{} · {}", m.name, m.role));
                        // How the task's conversation runs; each unset value follows the manager.
                        let (mode, model, effort) = self.task_run_labels();
                        let run = self.pending_task_run.clone();
                        let manager_set = chosen.is_some();
                        let this = cx.entity().downgrade();
                        // The goal has several lines; pasting an image or files attaches them.
                        let goal = gpui_kit::component::input::Textarea::new(&self.goal_input).py(px(7.)).line_height(px(22.)).on_paste(move |item, _, cx| {
                            this.update(cx, |this, cx| this.paste_into_task_goal(item, cx)).unwrap_or(false)
                        });
                        let project = match kind { DialogKind::NewTask { project } | DialogKind::TaskSchedule { project, .. } => self.projects.iter().find(|p| p.id == project).map(|p| p.name.clone()), _ => None };
                        let more = self.task_more_open;
                        let criteria = !self.role_input.read(cx).value().trim().is_empty();
                        let summary = self.task_more_summary(criteria);
                        el.child(self.dropdown("task-manager", "For", chosen.unwrap_or_else(|| if schedule { "Choose an employee".into() } else { "No employee: saved as a draft".into() }), manager_set, "alt-m", || Box::new(CycleTaskManager), cx))
                            .child(h().gap(px(8.)).text_size(px(12.5)).child(div().text_color(c(theme::TEXT_2)).child("Project")).child(div().min_w_0().truncate().child(project.unwrap_or_default())))
                            .child(field("Goal", goal.into_any_element()))
                            .child(self.more_toggle("task-more", more, summary, |this| this.task_more_open = !this.task_more_open, cx))
                            .when(more, |el| {
                                el.child(self.dropdown("task-mode", "Mode", mode, run.mode.is_some(), "alt-o", || Box::new(CycleTaskMode), cx))
                                    .child(self.dropdown("task-model", "Model · this task", model, run.model.is_some(), "alt-l", || Box::new(CycleTaskModel), cx))
                                    .child(self.dropdown("task-effort", "Effort · this task", effort, run.effort.is_some(), "alt-e", || Box::new(CycleTaskEffort), cx))
                                    .child(field("Acceptance criteria (separated by ;)", ui::input(&self.role_input).into_any_element()))
                                    .child(self.task_files_row(cx))
                            })
                            .when(schedule, |el| el.child(self.recurrence_editor(editing, cx)))
                    })
                    .when(is_manager, |el| {
                        let boss = self.pending_reports_to.and_then(|id| self.managers.iter().find(|m| m.id == id)).map(|m| m.name.clone());
                        let boss_set = boss.is_some();
                        el.child(field("Role (free text)", ui::input(&self.role_input).into_any_element()))
                            .child(field("Title on the org chart", ui::input(&self.title_input).into_any_element()))
                            .child(self.dropdown("reports-to", "Reports to", boss.unwrap_or_else(|| "Nobody (management team)".into()), boss_set, "alt-p", || Box::new(CycleReportsTo), cx))
                            .child(field("Instructions", ui::input(&self.instructions_input).into_any_element()))
                            .when_some(match kind { DialogKind::EditManager { id, .. } => Some(id), _ => None }, |el, id| {
                                use aurelius_domain::employee::Setting;
                                // The manager's own default model and effort; Inherit follows the project or global default.
                                let (model, effort, effective) = self.manager_run_labels(id);
                                let run = &self.pending_manager_run;
                                let (model_set, effort_set) = (matches!(run.model, Setting::Value(_)), matches!(run.effort, Setting::Value(_)));
                                el.child(self.dropdown("manager-model", "Model", model, model_set, "alt-l", || Box::new(CycleTaskModel), cx))
                                    .child(self.dropdown("manager-effort", "Effort", effort, effort_set, "alt-e", || Box::new(CycleTaskEffort), cx))
                                    .child(
                                        h().w_full().min_w_0().gap(px(6.)).text_size(px(11.5))
                                            .child(div().flex_none().text_color(c(theme::MUTED)).child(ui::label_with_info("Runs on", "The default for this employee's chat and tasks. A chat or task can override it for itself (Ctrl+M) without changing this.")))
                                            .child(div().flex_1().min_w_0().truncate().text_color(c(theme::TEXT_2)).child(effective)),
                                    )
                                    .child({
                                        let advanced = self.employee_advanced_open;
                                        let summary = format!("Subagents, run one at a time{}, plugins{}", if self.pending_manager_sequential { " (on)" } else { "" }, if versions.is_some() { ", instruction versions" } else { "" });
                                        self.more_toggle("employee-advanced", advanced, if advanced { "Advanced".into() } else { format!("Advanced: {summary}") }, |this| this.employee_advanced_open = !this.employee_advanced_open, cx)
                                    })
                                    .when(self.employee_advanced_open, |el| {
                                        el.child(self.delegation_rows(Some(id), cx))
                                            .child(self.sequential_row(cx))
                                            .child(self.manager_plugins_section(id, cx))
                                            .children(versions)
                                    })
                            })
                    })),
            )
            .child(
                h().px(px(22.)).py(px(14.)).gap(px(8.)).border_t_1().border_color(c(theme::BORDER))
                    .child(h().flex_1().gap(px(5.)).text_size(px(11.)).text_color(c(theme::MUTED)).when(multi_field, |el| el.child(ui::key_hint(&["tab"], "next field", cx))))
                    .child(ui::button("cancel", "Cancel", ButtonKind::Outline, Some(ui::kbd("escape", cx))).on_click(cx.listener(|this, _: &ClickEvent, w, cx| this_back(this, w, cx))))
                    .child(ui::button("confirm", confirm, ButtonKind::Primary, Some(ui::kbd("enter", cx))).on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.confirm_dialog(w, cx)))),
            )
    }

    /// "More ▾" / "Less ▴" with a line saying what is folded away.
    pub(crate) fn more_toggle(&self, id: &'static str, open: bool, summary: String, toggle: fn(&mut AppView), cx: &mut Context<Self>) -> Stateful<Div> {
        h().id(id).w_full().min_w_0().gap(px(8.)).cursor_pointer().text_size(px(12.5))
            .role(Role::Button).aria_label(SharedString::from(if open { "Show less" } else { "Show more" }))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                toggle(this);
                cx.notify();
            }))
            .child(div().flex_none().text_color(c(theme::ACCENT_TEXT)).child(if open { "Less ▴" } else { "More ▾" }))
            .child(div().flex_1().min_w_0().truncate().text_color(c(theme::MUTED)).child(summary))
    }

    /// New task and recurring task dialogs: the files to attach (this computer's, and for an
    /// edited recurring task the ones it has), or why files can't be attached.
    pub(crate) fn task_files_row(&self, cx: &mut Context<Self>) -> Div {
        let chip = |id: SharedString, name: String, size: u64, remove: Box<dyn Fn(&mut AppView)>, cx: &mut Context<Self>| {
            h().id(id.clone()).gap(px(6.)).px(px(9.)).py(px(4.)).rounded(px(6.)).border_1().border_color(c(theme::BORDER)).bg(c(theme::SURFACE)).text_size(px(12.))
                .child(div().text_color(c(theme::ACCENT_TEXT)).child("▤"))
                .child(div().max_w(px(220.)).truncate().child(name))
                .child(div().text_color(c(theme::MUTED)).child(crate::app::file_size(size)))
                .child(
                    div().id(SharedString::from(format!("{id}-x"))).cursor_pointer().text_color(c(theme::TEXT)).child("×")
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            remove(this);
                            cx.notify();
                        })),
                )
        };
        let mut chips: Vec<AnyElement> = Vec::new();
        for (i, f) in self.pending_schedule_files.iter().enumerate().filter(|(_, f)| !self.pending_schedule_remove.contains(&f.path)) {
            let path = f.path.clone();
            chips.push(chip(SharedString::from(format!("kept-file-{i}")), f.name.clone(), f.byte_length, Box::new(move |this| this.pending_schedule_remove.push(path.clone())), cx).into_any_element());
        }
        for (i, (path, size)) in self.pending_task_files.iter().enumerate() {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            chips.push(chip(SharedString::from(format!("new-file-{i}")), name, *size, Box::new(move |this| {
                if i < this.pending_task_files.len() {
                    this.pending_task_files.remove(i);
                }
            }), cx).into_any_element());
        }
        v().gap(px(6.))
            .child(
                h().gap(px(8.)).child(div().flex_1().text_size(px(12.)).text_color(c(theme::TEXT_2)).child(ui::label_with_info("Screenshots and files", if self.idea_files_on() {
                        "Pick files, paste an image into the note, or drop files on this dialog. At most 8, 25 MB each; a task started from the idea gets them too."
                    } else {
                        "Pick files, paste an image into the goal, or drop files on this dialog. At most 8, 25 MB each; the employee finds them in its work folder."
                    })))
                    .child(
                        ui::button("task-attach", "Attach…", ButtonKind::Outline, Some(ui::kbd("secondary-o", cx))).h(px(28.))
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.pick_task_files(cx))),
                    ),
            )
            .when(!chips.is_empty(), |el| el.child(h().flex_wrap().gap(px(6.)).children(chips)))
    }

    /// Recurring task dialog: every N hours, daily or weekly (with weekday toggles), the time,
    /// and (for a new one) "Start paused". Times are the server's local time.
    fn recurrence_editor(&self, editing: bool, cx: &mut Context<Self>) -> Div {
        let kind = self.pending_recurrence;
        let chip = |id: SharedString, label: &'static str, on: bool| {
            h().id(id).h(px(28.)).px(px(11.)).rounded(px(6.)).cursor_pointer().text_size(px(12.5)).border_1()
                .border_color(c(if on { theme::ACCENT } else { theme::BORDER }))
                .when(on, |el| el.bg(c(theme::SURFACE_2)))
                .text_color(c(if on { theme::TEXT } else { theme::TEXT_2 }))
                .child(label)
        };
        let summary = match self.read_recurrence(cx) {
            Ok(r) => crate::app::recurrence_label(&r),
            Err(e) => e,
        };
        v().gap(px(8.))
            .child(div().text_size(px(12.)).text_color(c(theme::TEXT_2)).child(ui::label_with_info("Repeats", "Times are the server's local time.")))
            // B1-2: a schedule runs on its maker's own Claude login.
            .child(div().id("schedule-own-subscription").text_size(px(12.)).text_color(c(theme::MUTED)).child(crate::screens::account::OWN_SUBSCRIPTION))
            .child(h().gap(px(6.)).children(["Every N hours", "Daily", "Weekly"].into_iter().enumerate().map(|(i, label)| {
                chip(SharedString::from(format!("recur-kind-{i}")), label, kind == i).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.pending_recurrence = i;
                    cx.notify();
                }))
            })))
            .when(kind == 0, |el| el.child(field("Every how many hours", ui::input(&self.recur_hours_input).into_any_element())))
            .when(kind == 2, |el| {
                el.child(h().gap(px(6.)).flex_wrap().children(["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"].into_iter().enumerate().map(|(i, label)| {
                    chip(SharedString::from(format!("recur-day-{i}")), label, self.pending_weekdays[i]).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.pending_weekdays[i] = !this.pending_weekdays[i];
                        cx.notify();
                    }))
                })))
            })
            .when(kind != 0, |el| el.child(field("At (HH:MM)", ui::input(&self.recur_time_input).into_any_element())))
            .child(div().text_size(px(12.)).text_color(c(theme::ACCENT_TEXT)).child(summary))
            .when(!editing, |el| {
                let on = self.pending_schedule_paused;
                el.child(
                    h().id("recur-paused").gap(px(8.)).cursor_pointer().text_size(px(12.5))
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                            this.pending_schedule_paused = !this.pending_schedule_paused;
                            cx.notify();
                        }))
                        .child(div().text_color(c(if on { theme::ACCENT_TEXT } else { theme::MUTED })).child(if on { "☑" } else { "☐" }))
                        .child(ui::label_with_info("Start paused", "Nothing is made until you resume it.")),
                )
            })
    }

    /// Model of the Claude subagents: for this manager (Edit manager dialog) or the global
    /// default (Settings → Connections), with what it uses and where that comes from.
    /// Subagents have no effort of their own: Claude Code runs them at the manager's.
    pub(crate) fn delegation_rows(&self, manager: Option<aurelius_domain::EmployeeId>, cx: &mut Context<Self>) -> Div {
        use crate::app::DelegationRow;
        let row = DelegationRow::Subagents;
        let title = if manager.is_some() { "Subagents of this employee" } else { "Subagents" };
        let (model, _effort, effective) = self.delegation_labels(row, manager);
        let model_set = model != "Inherit";
        v().gap(px(12.)).child(
            v().gap(px(6.))
                .child(div().text_size(px(12.)).text_color(c(theme::TEXT_2)).child(title))
                .child(
                    v().gap(px(6.))
                        .child(self.dropdown("subagent-model", "Model", model, model_set, "alt-u", || Box::new(CycleSubagentModel), cx))
                        .child(h().text_size(px(12.)).text_color(c(theme::MUTED)).child("Effort: follows the employee")),
                )
                .child(
                    h().w_full().min_w_0().gap(px(6.)).text_size(px(11.5))
                        .child(div().flex_none().text_color(c(theme::MUTED)).child("Uses"))
                        .child(div().flex_1().min_w_0().truncate().text_color(c(theme::TEXT_2)).child(effective)),
                ),
        )
    }

    /// Edit manager dialog: "Run tasks one at a time".
    fn sequential_row(&self, cx: &App) -> Div {
        let on = self.pending_manager_sequential;
        v().gap(px(4.))
            .child(
                h().gap(px(10.)).items_center().text_size(px(13.))
                    .child(div().child(ui::toggle("manager-sequential", on, "Run tasks one at a time", || Box::new(ToggleManagerSequential))))
                    .child(div().flex_1().child(ui::label_with_info("Run tasks one at a time", "Tasks wait in line and start one after the other.")))
                    .child(ui::kbd("alt-t", cx)),
            )
    }

    fn render_review(&mut self, max_h: f32, scroll: &ScrollHandle, cx: &mut Context<Self>) -> impl IntoElement {
        let mono = ui::mono(cx);
        let row = |label: &'static str, value: &'static str| {
            h().gap(px(12.)).text_size(px(13.)).child(div().w(px(110.)).text_color(c(theme::MUTED)).child(label)).child(div().flex_1().child(value))
        };
        card(560.)
            .child(
                ui::scroll_frame(scroll, scroll_body("review-body", scroll, (max_h - 70.).max(120.)).p(px(22.)).gap(px(14.))
                    .child(h().gap(px(8.)).child(ui::diamond(10., theme::ACCENT_TEXT, false)).child(ui::eyebrow("APPROVAL")).child(ui::preview_tag(cx)))
                    .child(h().font_family(ui::serif(cx)).text_size(px(21.)).child(preview::APPROVAL.title).child(div().font_family(mono.clone()).text_size(px(16.)).child(preview::APPROVAL.code)))
                    .child(row("Requested by", "Alex · Development Manager"))
                    .child(row("Goal", "Fix authentication race"))
                    .child(row("Scope", preview::APPROVAL.detail))
                    .child(row("Expires", "in 2 hours; expiry never counts as permission"))
                    .child(
                        div().p(px(12.)).rounded(px(6.)).bg(c(theme::SURFACE)).border_1().border_color(c(theme::BORDER)).font_family(mono).text_size(px(12.)).text_color(c(theme::TEXT_2))
                            .child("ALTER TABLE sessions ADD COLUMN refreshed_at INTEGER;"),
                    )),
            )
            .child(
                h().px(px(22.)).py(px(14.)).gap(px(8.)).border_t_1().border_color(c(theme::BORDER))
                    .child(div().flex_1().text_size(px(11.)).text_color(c(theme::MUTED)).child("Only this exact action. No 'always allow'."))
                    .child(ui::button("reject", "Reject", ButtonKind::Outline, Some(ui::kbd("escape", cx))).on_click(cx.listener(|this, _: &ClickEvent, w, cx| this_back(this, w, cx))))
                    .child(ui::button("approve", "Approve once", ButtonKind::Primary, Some(ui::kbd("secondary-enter", cx))).on_click(cx.listener(|_, _: &ClickEvent, w, cx| w.dispatch_action(Box::new(ConfirmPrimary), cx)))),
            )
    }
}

impl AppView {
    /// Settings → Connections: status, billing, models, defaults.
    fn render_settings(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let mono = ui::mono(cx);
        // Servers is how you get connected: it never waits for a server.
        if self.settings_tab == crate::screens::servers::SERVERS_TAB {
            let page = settings_page().track_focus(&self.focus).key_context(SETTINGS);
            return self.render_servers_settings(page, cx);
        }
        if self.settings_tab == crate::screens::app_keys::APP_KEYS_TAB {
            let page = settings_page().track_focus(&self.focus).key_context(LOAD);
            return self.render_app_keys_settings(page, cx);
        }
        if self.settings_tab == crate::screens::schedules::SCHEDULES_TAB {
            let page = settings_page().track_focus(&self.focus).key_context(LOAD);
            return self.render_schedules_settings(page, cx);
        }
        if self.settings_tab == crate::screens::trace::TRACE_TAB {
            let page = settings_page().track_focus(&self.focus);
            return self.render_trace_settings(page, cx);
        }
        if self.settings_tab == crate::screens::plugins::PLUGINS_TAB {
            let page = settings_page().track_focus(&self.focus).key_context(LOAD);
            return self.render_plugins_settings(page, cx);
        }
        if self.settings_tab == COMPANY_TAB {
            let page = settings_page().track_focus(&self.focus).key_context(LOAD);
            return self.render_company_settings(page, cx);
        }

        if self.settings_tab == crate::screens::mcp::MCP_TAB {
            let page = settings_page().track_focus(&self.focus);
            return self.render_mcp_settings(page, cx);
        }
        let Some(snapshot) = self.connections.clone() else {
            return settings_page().track_focus(&self.focus).key_context(SETTINGS).child(div().p(px(22.)).child("Loading…"));
        };
        let selected = self.settings_selected.min(snapshot.connections.len().saturating_sub(1));
        let status_color = |status: &str| match status {
            "ready" => theme::ACCENT_TEXT,
            "needs_auth" => theme::TEXT_2,
            _ => theme::NEGATIVE,
        };
        let status_label = |status: &str| match status {
            "ready" => "ready",
            "needs_auth" => "sign-in / key needed",
            "unavailable" => "unreachable",
            other => other,
        }
        .to_string();

        let list = v().w(px(270.)).flex_shrink(1.).min_w(px(170.)).p(px(8.)).gap(px(2.)).border_r_1().border_color(c(theme::BORDER)).children(
            snapshot.connections.iter().enumerate().map(|(i, conn)| {
                v().id(("conn", i)).px(px(10.)).py(px(8.)).rounded(px(6.)).cursor_pointer()
                    .when(i == selected, |el| el.bg(c(theme::SURFACE_2)))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.settings_selected = i;
                        cx.notify();
                    }))
                    .child(h().gap(px(7.)).child(ui::dot(6., status_color(&conn.status))).child(div().flex_1().text_size(px(13.)).child(conn.display_name.clone())))
                    .child(div().pl(px(13.)).text_size(px(11.5)).text_color(c(theme::MUTED)).child(status_label(&conn.status)))
            }),
        );

        let detail: AnyElement = if self.adding_connection {
            v().p(px(22.)).gap(px(12.))
                .child(h().gap(px(8.)).items_center()
                    .child(div().font_family(ui::serif(cx)).text_size(px(20.)).child("Add connection"))
                    .child(ui::info("add-connection-about", "Choose a runtime. Nothing is connected automatically and there is never a silent switch to a paid service.")))
                .children([
                    ("1", "Claude Code", "Your own, unmodified Claude Code. You sign in within Claude Code itself; Aurelius never sees your login.", (|| Box::new(Provider1)) as fn() -> Box<dyn Action>),
                    ("2", "Anthropic API", "Your own API key, pay per use.", || Box::new(Provider2)),
                    ("3", "OpenAI API", "Your own API key, pay per use.", || Box::new(Provider3)),
                ].map(|(key, name, text, action)| {
                    h().id(SharedString::from(format!("provider-{key}"))).cursor_pointer().hover(|s| s.bg(c(theme::SURFACE_2)))
                        .on_click(move |_, w, cx| w.dispatch_action(action(), cx))
                        .gap(px(12.)).items_start().px(px(12.)).py(px(10.)).rounded(px(8.)).border_1().border_color(c(theme::BORDER))
                        .child(ui::kbd(key, cx))
                        .child(h().gap(px(6.)).items_center().text_size(px(14.)).child(name).child(ui::info(SharedString::from(format!("provider-info-{key}")), text)))
                }))
                .into_any_element()
        } else if let Some(conn) = snapshot.connections.get(selected).cloned() {
            let row = |label: &'static str, value: String| {
                h().gap(px(12.)).items_start().text_size(px(13.)).child(div().w(px(120.)).flex_none().text_color(c(theme::MUTED)).child(label)).child(div().flex_1().child(value))
            };
            v().p(px(22.)).gap(px(12.))
                .child(div().font_family(ui::serif(cx)).text_size(px(21.)).child(conn.display_name.clone()))
                .child(row("Status", format!("{}{}", status_label(&conn.status), conn.detail.clone().map(|d| format!(" · {d}")).unwrap_or_default())))
                .child(row("Billing", conn.billing.clone()))
                .child(row("Sign-in", match conn.auth_kind.as_str() {
                    "api_key" => if conn.has_secret { "API key is in the OS credential store".into() } else { "No API key yet (S)".into() },
                    "runtime_login" => "In Claude Code itself (`claude auth login`)".into(),
                    _ => "Not needed".into(),
                }))
                .when_some(conn.runtime_path.clone(), |el, p| el.child(row("Program", p)))
                .when(!conn.models.is_empty(), |el| el.child(row("Available", conn.models.join(", "))))
                .when(self.entering_key, |el| {
                    el.child(
                        v().gap(px(6.)).key_context("SettingsKey")
                            .child(div().text_size(px(12.)).text_color(c(theme::TEXT_2)).child("API key (Enter saves, Esc cancels)"))
                            .child(ui::input(&self.key_input)),
                    )
                })
                .child(div().font_family(mono.clone()).text_size(px(10.5)).text_color(c(theme::MUTED)).child(format!("id {}", conn.id)))
                .into_any_element()
        } else {
            div().p(px(22.)).child("No connections.").into_any_element()
        };

        if self.settings_tab == crate::screens::remote::REMOTE_TAB {
            let page = settings_page().track_focus(&self.focus).key_context(SETTINGS);
            return self.render_remote_settings(page, cx);
        }
        if self.settings_tab == 2 {
            let snap = self.backups.clone().unwrap_or_default();
            let selected = self.settings_selected.min(snap.backups.len().saturating_sub(1));
            let size = |bytes: u64| if bytes >= 1 << 20 { format!("{:.1} MB", bytes as f64 / (1u64 << 20) as f64) } else { format!("{} KB", bytes.div_ceil(1024)) };
            return settings_page()
                .track_focus(&self.focus)
                .key_context(SETTINGS)
                .child(
                    h().px(px(22.)).py(px(14.)).border_b_1().border_color(c(theme::BORDER))
                        .child(self.settings_title(cx)),
                )
                .child(
                    v().id("backups-body").flex_1().min_h_0().overflow_y_scrollbar().p(px(22.)).gap(px(8.))
                        .when_some(snap.paused.clone(), |el, _| {
                            el.child(
                                h().gap(px(10.)).px(px(12.)).py(px(10.)).rounded(px(8.)).bg(c(theme::ATTENTION_BG)).text_color(c(theme::ACCENT_TEXT)).text_size(px(12.5))
                                    .child(ui::pulse_dot("paused-dot-settings", 7., theme::ACCENT))
                                    .child(div().flex_1().child("Restored from a backup. Nothing runs (tasks, chats, schedules) until you resume."))
                                    .child(ui::button("resume-work", "Resume", ButtonKind::Primary, Some(ui::kbd("p", cx))).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.resume_work(cx)))),
                            )
                        })
                        .when(snap.backups.is_empty(), |el| el.child(div().py(px(20.)).text_color(c(theme::MUTED)).child("No backups yet. Press B to make one.")))
                        .children(snap.backups.iter().enumerate().map(|(i, b)| {
                            h().id(("backup", i)).gap(px(12.)).px(px(12.)).py(px(9.)).rounded(px(6.)).cursor_pointer()
                                .when(i == selected, |el| el.bg(c(theme::SURFACE_2)))
                                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                    this.settings_selected = i;
                                    cx.notify();
                                }))
                                .child(div().w(px(190.)).flex_none().text_size(px(13.)).child(ui::fmt_when(b.created_at_ms)))
                                .child(div().w(px(70.)).flex_none().text_size(px(11.5)).text_color(c(theme::MUTED)).child(if b.kind == "auto" { "daily" } else { "manual" }))
                                .child(div().flex_1().text_size(px(11.5)).text_color(c(theme::MUTED)).child(format!("{} · {} files · v{}", size(b.bytes), b.files, b.app_version)))
                                .when(b.scrubbed, |el| el.child(div().text_size(px(11.)).text_color(c(theme::MUTED)).child("forgotten content removed")))
                        }))
                        .child(div().pt(px(8.)).font_family(mono.clone()).text_size(px(10.5)).text_color(c(theme::MUTED)).child(snap.folder.clone()))
                        ,
                )
                .child(
                    h().px(px(22.)).py(px(12.)).gap(px(14.)).border_t_1().border_color(c(theme::BORDER)).text_size(px(11.5)).text_color(c(theme::MUTED))
                        .child(ui::key_hint(&["down"], "select", cx))
                        .child(ui::hint("s-backup", "b", "Back up now", || Box::new(SettingsBackupNow), cx))
                        .child(ui::hint("s-restore", "r", "Restore selected…", || Box::new(SettingsRestore), cx))
                        .child(ui::hint("s-resume", "p", "Resume after restore", || Box::new(SettingsResume), cx))
                        ,
                );
        }
        if self.settings_tab == 1 || self.settings_tab == SERVER_PC_TAB {
            let u = self.update.clone().unwrap_or_default();
            let app = self.app_update().cloned();
            let row = |label: &'static str, value: String| {
                h().gap(px(12.)).items_start().text_size(px(13.)).child(div().w(px(130.)).flex_none().text_color(c(theme::MUTED)).child(label)).child(div().flex_1().child(value))
            };
            let state = |u: &aurelius_protocol::UpdateStatus| -> String {
                if !u.enabled {
                    u.reason.clone().unwrap_or_else(|| "Updates are off.".into())
                } else if let Some(e) = &u.error {
                    e.clone()
                } else if u.available {
                    format!("Version {} is available.", u.latest.clone().unwrap_or_default())
                } else if u.checked_at_ms > 0 {
                    "Up to date.".into()
                } else {
                    "Not checked yet.".into()
                }
            };
            // The server and this app are released separately: each its own version and notes.
            // Rendered with the viewer's Markdown, in a box of fixed height that scrolls.
            let notes = |id: &'static str, notes: Option<String>, handler: Box<dyn Fn(&SharedString, &ClickEvent, &mut Window, &mut App) + Send + Sync>| {
                notes.filter(|n| !n.trim().is_empty()).map(|notes| {
                    let view = gpui_kit::component::text::TextView::markdown(SharedString::from(format!("{id}-{}", notes.len())), notes)
                        .selectable(true)
                        .on_link_click(handler);
                    v().gap(px(6.))
                        .child(ui::eyebrow("WHAT'S NEW"))
                        .child(
                            div().id(id).h(px(260.)).flex_none().overflow_y_scrollbar().px(px(12.)).py(px(10.)).rounded(px(6.))
                                .border_1().border_color(c(theme::BORDER)).bg(c(theme::SURFACE))
                                .child(div().text_size(px(12.5)).line_height(px(18.)).text_color(c(theme::TEXT_2)).child(view)),
                        )
                })
            };
            let section = |title: &'static str| div().mt(px(8.)).pt(px(14.)).border_t_1().border_color(c(theme::BORDER)).child(ui::eyebrow(title));
            let page = settings_page()
                .track_focus(&self.focus)
                .key_context(SETTINGS)
                .child(
                    h().px(px(22.)).py(px(14.)).border_b_1().border_color(c(theme::BORDER))
                        .child(self.settings_title(cx)),
                );
            let footer = h().px(px(22.)).py(px(12.)).gap_x(px(14.)).gap_y(px(6.)).flex_wrap().border_t_1().border_color(c(theme::BORDER)).text_size(px(11.5)).text_color(c(theme::MUTED))
                .child(ui::hint("s-check", "c", "Check for updates", || Box::new(SettingsCheckUpdate), cx));
            if self.settings_tab == SERVER_PC_TAB {
                return page
                    .child(
                        v().id("server-pc-body").flex_1().min_h_0().overflow_y_scrollbar().p(px(22.)).gap(px(12.))
                            .child(self.render_server_name(cx))
                            .child(if self.server_name_section() { section("UPDATES") } else { div().child(ui::eyebrow("UPDATES")) })
                            .child(row("Installed", u.current.clone()))
                            .child(row("Latest", u.latest.clone().unwrap_or_else(|| "—".into())))
                            .child(row("Status", state(&u)))
                            .children(notes("notes-server", u.notes.clone(), Box::new(self.markdown_link_handler(false, cx))))
                            .child(section("POWER"))
                            .child(
                                h().gap(px(12.)).items_center().text_size(px(13.))
                                    .child(div().w(px(130.)).flex_none().text_color(c(theme::MUTED)).child("Restart"))
                                    .child(ui::button("restart-server", "Restart server", ButtonKind::Outline, Some(ui::kbd("e", cx)))
                                        .on_click(cx.listener(|_, _: &ClickEvent, w, cx| w.dispatch_action(Box::new(SettingsRestartServer), cx))))
                            )
                            .child({
                                let label = div().w(px(130.)).flex_none().text_color(c(theme::MUTED)).child("Shut down this PC");
                                h().gap(px(12.)).items_center().flex_wrap().text_size(px(13.))
                                    .child(label)
                                    .child(ui::button("shutdown-now", "Shut down now", ButtonKind::Outline, Some(ui::kbd("q", cx)))
                                        .on_click(cx.listener(|_, _: &ClickEvent, w, cx| w.dispatch_action(Box::new(SettingsShutdownNow), cx))))
                                    .child(ui::button("shutdown-idle", "When all work is done", ButtonKind::Outline, Some(ui::kbd("g", cx)))
                                        .on_click(cx.listener(|_, _: &ClickEvent, w, cx| w.dispatch_action(Box::new(SettingsShutdownIdle), cx))))
                            }),
                    )
                    .child(
                        footer
                            .when(self.server_rename_allowed(), |el| el.child(ui::hint("s-rename", "f2", "Rename server", || Box::new(SettingsRenameServer), cx)))
                            .child(ui::hint("s-install", "u", "Update server", || Box::new(SettingsInstallUpdate), cx))
                            .child(ui::hint("s-restart", "e", "Restart server", || Box::new(SettingsRestartServer), cx))
                            .child(ui::hint("s-shutdown", "q", "Shut down PC", || Box::new(SettingsShutdownNow), cx))
                            .child(ui::hint("s-shutdown-idle", "g", "Shut down when idle", || Box::new(SettingsShutdownIdle), cx)),
                    );
            }
            let app_state = match (&app, self.remote_name()) {
                (Some(a), _) => state(a),
                (None, Some(_)) => "Update this app on this computer: scripts\\install-production.ps1 -Role client.".into(),
                (None, None) => "The server does not install this app (a build from source, or a server from before the split).".into(),
            };
            return page
                .child(
                    v().id("app-body").flex_1().min_h_0().overflow_y_scrollbar().p(px(22.)).gap(px(12.))
                        .child(ui::eyebrow("PREFERENCES"))
                        .child(match (self.remote_name(), aurelius_host::startup::installed_dir()) {
                            (Some(host), _) => row("Start at sign-in", format!("Managed on {host} (aurelius-ctl autostart on)")),
                            (None, Err(reason)) => row("Start at sign-in", reason),
                            (None, Ok(_)) => h().gap(px(12.)).items_center().text_size(px(13.))
                                .child(div().w(px(130.)).flex_none().text_color(c(theme::MUTED)).child(ui::label_with_info("Start at sign-in", "When on, the background service starts when you sign in to Windows.")))
                                .child(ui::toggle("login-start", self.login_start, "Start at sign-in", || Box::new(SettingsLoginStart)))
                                .child(div().flex_1().child(if self.login_start { "On" } else { "Off" })),
                        })
                        .child(h().gap(px(12.)).items_start().text_size(px(13.))
                            .child(div().w(px(130.)).flex_none().text_color(c(theme::MUTED)).child(ui::label_with_info("Closing", "Closing the window: background work continues. The tray icon shows it; Quit there stops it.")))
                            .child(div().flex_1().child("Keeps running in the tray")))
                        .child({
                            let always = cx.try_global::<ui::Hints>().is_some_and(|h| h.always);
                            h().gap(px(12.)).items_center().text_size(px(13.))
                                .child(div().w(px(130.)).flex_none().text_color(c(theme::MUTED)).child(ui::label_with_info("Shortcut hints", "When on, shortcut hints are always shown. When off they are hidden.")))
                                .child(ui::toggle("show-hints", always, "Always show shortcut hints", || Box::new(SettingsShowHints)))
                                .child(div().flex_1().child(if always { "On" } else { "Off" }))
                        })
                        .child({
                            let current = theme::Appearance::current(cx);
                            h().gap(px(12.)).items_center().text_size(px(13.))
                                .child(div().w(px(130.)).flex_none().text_color(c(theme::MUTED)).child(ui::label_with_info("Appearance", "System follows light or dark mode in your system settings.")))
                                .child(
                                    h().flex_none().p(px(2.)).gap(px(2.)).rounded(px(7.)).border_1().border_color(c(theme::BORDER))
                                        .children(theme::Appearance::ALL.into_iter().map(|a| {
                                            let on = a == current;
                                            div().id(a.key()).role(Role::Button).aria_label(SharedString::from(format!("Appearance: {}", a.label())))
                                                .px(px(10.)).py(px(3.)).rounded(px(5.)).cursor_pointer()
                                                .text_color(c(if on { theme::TEXT } else { theme::TEXT_2 }))
                                                .when(on, |el| el.bg(c(theme::SURFACE_2)))
                                                .when(!on, |el| el.hover(|s| s.text_color(c(theme::TEXT))))
                                                .child(a.label())
                                                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.set_appearance(a, cx)))
                                        })),
                                )
                        })
                        .child(div().mt(px(8.)).pt(px(14.)).border_t_1().border_color(c(theme::BORDER)).child(h().gap(px(6.)).items_center().child(ui::eyebrow("UPDATES")).child(ui::info(
                            "app-updates-about",
                            "The server and this app are released separately; they work together as long as their protocol matches. Releases come from the private GitHub repository via your own GitHub CLI login. A package is installed only if its signature matches the Aurelius release key. Your data is not touched; running work continues after a server restart.",
                        ))))
                        .child(row("Installed", env!("CARGO_PKG_VERSION").into()))
                        .child(row("Latest", app.as_ref().and_then(|a| a.latest.clone()).unwrap_or_else(|| "—".into())))
                        .child(row("Status", app_state))
                        .children(notes("notes-app", app.as_ref().and_then(|a| a.notes.clone()), Box::new(self.markdown_link_handler(false, cx))))
                )
                .child(
                    footer
                        .child(ui::hint("s-install-app", "i", "Update app", || Box::new(SettingsInstallAppUpdate), cx))
                        .child(ui::hint("s-login", "l", "Start at sign-in on/off", || Box::new(SettingsLoginStart), cx))
                        .child(ui::hint("s-hints", "h", "Shortcut hints on/off", || Box::new(SettingsShowHints), cx))
                        .child(ui::hint("s-appearance", "w", "Appearance", || Box::new(SettingsAppearance), cx)),
                );
        }
        settings_page()
            .track_focus(&self.focus)
            .key_context(SETTINGS)
            .child(
                h().px(px(22.)).py(px(14.)).border_b_1().border_color(c(theme::BORDER))
                    .child(self.settings_title(cx)),
            )
            .child(h().id("connections-body").flex_1().min_h_0().overflow_y_scrollbar().items_start().child(list).child(div().flex_1().min_w_0().child(detail)))
            .child(
                h().px(px(22.)).py(px(12.)).gap(px(14.)).flex_wrap().border_t_1().border_color(c(theme::BORDER)).text_size(px(11.5)).text_color(c(theme::MUTED))
                    .child(ui::key_hint(&["down"], "select", cx))
                    .child(ui::hint("s-verify", "v", "Verify", || Box::new(SettingsVerify), cx))
                    .child(ui::hint("s-key", "s", "API key…", || Box::new(SettingsKey), cx))
                    .child(ui::hint("s-new", "n", "Add connection…", || Box::new(SettingsNew), cx))
                    .child(ui::hint("s-remove", "delete", "Remove", || Box::new(SettingsRemove), cx)),
            )
    }

    /// Inbox: every pending approval with the exact operation, then
    /// notifications (results, failures, decisions needed). Spec §12.5.
    /// Claude's questions with their options, to choose from or answer in your own words:
    /// in the inbox (`focus` is the focused question, with its keys) or in the conversation.
    pub fn question_form(&self, a: &aurelius_protocol::ApprovalView, focus: Option<usize>, cx: &mut Context<Self>) -> impl IntoElement {
        let id = a.id;
        let in_inbox = focus.is_some();
        v().gap(px(10.)).children(crate::app::questions_of(a).into_iter().enumerate().map(|(i, q)| {
            let chosen = self.question_choices.get(&(id, i)).cloned().unwrap_or_default();
            let own = self.question_own.get(&(id, i)).cloned();
            let focused = focus == Some(i);
            let multi = q.multi;
            v().id(SharedString::from(format!("question-{id}-{i}"))).gap(px(8.)).p(px(12.)).rounded(px(8.)).border_1()
                .border_color(c(if focused { theme::FOCUS_RING } else { theme::BORDER }))
                .when(in_inbox, |el| el.on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.focus_question(i, cx))))
                .child(h().flex_wrap().gap(px(8.))
                    .when(!q.header.is_empty(), |el| el.child(div().px(px(6.)).py(px(1.)).rounded(px(4.)).bg(c(theme::SURFACE_2)).text_size(px(11.)).text_color(c(theme::TEXT_2)).child(q.header.clone())))
                    .when(multi, |el| el.child(div().text_size(px(11.)).text_color(c(theme::MUTED)).child("choose one or more"))))
                .child(div().text_size(px(14.)).line_height(px(20.)).child(q.text.clone()))
                .children(q.options.into_iter().enumerate().map(|(n, (label, description))| {
                    let on = chosen.contains(&n);
                    let mark = match (multi, on) { (true, true) => "☑", (true, false) => "☐", (false, true) => "●", (false, false) => "○" };
                    h().id(SharedString::from(format!("option-{id}-{i}-{n}"))).gap(px(10.)).items_start().px(px(8.)).py(px(6.)).rounded(px(6.)).cursor_pointer()
                        .when(on, |el| el.bg(c(theme::SURFACE_2)))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            if in_inbox {
                                this.question_focus = i;
                                this.choose_option(n, cx);
                            } else {
                                this.choose_option_for(id, i, n, cx);
                            }
                        }))
                        .child(div().w(px(14.)).flex_none().text_color(c(if on { theme::ACCENT_TEXT } else { theme::MUTED })).child(mark))
                        .child(v().flex_1().min_w_0().gap(px(2.))
                            .child(div().text_size(px(13.)).when(on, |el| el.font_weight(FontWeight::MEDIUM)).child(label))
                            .when(!description.is_empty(), |el| el.child(div().text_size(px(12.)).text_color(c(theme::TEXT_2)).child(description))))
                        .when(focused, |el| el.child(ui::kbd(&(n + 1).to_string(), cx).opacity(0.6)))
                }))
                .child(
                    h().id(SharedString::from(format!("own-{id}-{i}"))).gap(px(10.)).items_start().px(px(8.)).py(px(6.)).rounded(px(6.)).cursor_pointer()
                        .when(own.is_some(), |el| el.bg(c(theme::SURFACE_2)))
                        .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| {
                            if in_inbox {
                                this.question_focus = i;
                            }
                            this.open_own_answer(id, i, in_inbox, w, cx);
                        }))
                        .child(div().w(px(14.)).flex_none().text_color(c(if own.is_some() { theme::ACCENT_TEXT } else { theme::MUTED })).child(if own.is_some() { "●" } else { "○" }))
                        .child(div().flex_1().min_w_0().text_size(px(13.)).text_color(c(if own.is_some() { theme::TEXT } else { theme::TEXT_2 })).child(own.unwrap_or_else(|| "Something else…".into())))
                        .when(focused, |el| el.child(ui::kbd("t", cx).opacity(0.6))),
                )
        }))
    }

    fn render_inbox(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let mono = ui::mono(cx);
        let tab = self.inbox_tab;
        let counts = self.inbox_counts();
        let pending: Vec<_> = if tab.shows_head() { self.pending_approvals().cloned().collect() } else { Vec::new() };
        let notes: Vec<aurelius_protocol::NotificationView> = self.inbox_notes().into_iter().cloned().collect();
        // A colleague a manager proposed sits between the approvals and the results.
        let proposals: Vec<aurelius_protocol::ProposalView> = if tab.shows_head() { self.pending_proposals().into_iter().cloned().collect() } else { Vec::new() };
        // Then the guidelines employees proposed (B5), beside the instruction proposals.
        let guideline_proposals: Vec<aurelius_domain::guideline::GuidelineProposalView> = if tab.shows_head() { self.pending_guideline_proposals().into_iter().cloned().collect() } else { Vec::new() };
        let head = pending.len() + proposals.len() + guideline_proposals.len();
        let total = head + notes.len();
        let selected = self.inbox_selected.min(total.saturating_sub(1));
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0);
        let note_selected = selected.checked_sub(head).and_then(|i| notes.get(i)).cloned();
        let update_selected = note_selected.clone().filter(updates::is_update);
        if let Some(id) = update_selected.as_ref().and_then(|n| n.ticket_id) {
            self.inbox_ask_followers(id);
        }
        let note_task_id = note_selected.as_ref().and_then(|n| n.task_id);
        let note_conversation = note_task_id.and_then(|id| self.tasks.iter().find(|t| t.id == id)).and_then(|t| t.conversation_id);
        let proposal_selected = selected.checked_sub(pending.len()).filter(|i| *i < proposals.len());
        let guideline_selected = selected.checked_sub(pending.len() + proposals.len()).filter(|i| *i < guideline_proposals.len());
        let question_selected = pending.get(selected).is_some_and(|a| a.tool == crate::app::QUESTION_TOOL);
        let plan_selected = pending.get(selected).is_some_and(|a| a.tool == crate::app::PLAN_TOOL);
        let always = pending.get(selected).and_then(|a| self.always_allow_button(a, "inbox", cx));
        let detail: AnyElement = match pending.get(selected) {
            None if proposal_selected.is_some() => {
                let i = proposal_selected.unwrap_or_default();
                v().p(px(22.)).gap(px(12.)).child(self.proposal_body(&proposals[i], i, cx)).into_any_element()
            }
            None if guideline_selected.is_some() => {
                let i = guideline_selected.unwrap_or_default();
                v().id("inbox-guideline").p(px(22.)).gap(px(12.)).overflow_y_scrollbar().child(self.guideline_proposal_body(&guideline_proposals[i], i, cx)).into_any_element()
            }
            None if update_selected.is_some() => self.update_detail(update_selected.as_ref().unwrap_or_else(|| unreachable!()), now, cx),
            None if note_selected.is_some() => {
                let n = note_selected.clone().unwrap_or_else(|| unreachable!());
                let (eyebrow, color) = note_mark(&n);
                let task = n.task_id.and_then(|id| self.tasks.iter().find(|t| t.id == id)).cloned();
                v().p(px(22.)).gap(px(12.))
                    .child(h().gap(px(8.)).child(ui::dot(8., color)).child(ui::eyebrow(eyebrow)))
                    .child(div().font_family(ui::serif(cx)).text_size(px(21.)).child(n.title.clone()))
                    .when(!n.subject.is_empty() && task.is_none(), |el| el.child(div().text_size(px(14.)).text_color(c(theme::TEXT_2)).child(n.subject.clone())))
                    .child(self.inbox_context(None, n.task_id, None, n.created_at_ms, now, cx))
                    // What the task last reported: its result, or why it failed.
                    .children(task.as_ref().and_then(|t| t.result.clone()).filter(|r| !r.trim().is_empty()).map(|r| {
                        v().gap(px(4.)).child(ui::eyebrow("LAST ACTIVITY"))
                            .child(div().text_size(px(13.)).line_height(px(19.)).text_color(c(theme::TEXT_2)).line_clamp(6).child(r))
                    }))
                    .into_any_element()
            }
            None => {
                let (title, about) = match tab {
                    InboxTab::NeedsYou => ("Nothing needs you.", "Approvals, questions, plans and proposals appear here, with exactly what will run."),
                    InboxTab::Updates => (updates::NO_UPDATES, updates::NO_UPDATES_ABOUT),
                    InboxTab::Results => ("No results yet.", "Finished tasks report here."),
                    InboxTab::Failed => ("Nothing failed.", "Tasks that fail appear here, so you can follow them up."),
                    InboxTab::All => ("The inbox is empty.", "What needs you and what finished both appear here."),
                };
                v().p(px(22.)).gap(px(8.))
                    .child(h().gap(px(8.)).items_center()
                        .child(div().font_family(ui::serif(cx)).italic().text_size(px(19.)).text_color(c(theme::TEXT_2)).child(title))
                        .child(ui::info("inbox-empty-about", about)))
                    .into_any_element()
            }
            Some(a) if a.tool == crate::app::PLAN_TOOL => {
                let minutes = ((a.expires_at_ms - now).max(0) / 60_000) as i64;
                let plan = a.clone();
                v().p(px(22.)).gap(px(12.))
                    .child(h().gap(px(8.)).child(ui::diamond(10., theme::ACCENT_TEXT, true)).child(ui::eyebrow("PLAN · WAITING FOR YOU")))
                    .child(div().font_family(ui::serif(cx)).text_size(px(21.)).child(format!("{} has a plan", a.requester)))
                    .child(self.inbox_context(Some(a.requester.clone()), a.task_id, a.conversation_id, a.requested_at_ms, now, cx))
                    .child(div().text_size(px(14.)).line_height(px(20.)).text_color(c(theme::TEXT_2)).child(crate::app::plan_headline_of(a)))
                    .child(
                        h().child(
                            ui::button("review-plan", "Review plan", ButtonKind::Primary, None)
                                .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.open_plan_approval(plan.clone(), w, cx))),
                        ),
                    )
                    .child(h().gap(px(6.)).items_center().text_size(px(11.5)).text_color(c(theme::MUTED)).child(format!("Expires in {minutes} min")).child(ui::info("plan-expiry-about", "Nothing has run yet. Expiry never counts as permission.")))
                    .into_any_element()
            }
            Some(a) if a.tool == crate::app::QUESTION_TOOL => {
                let questions = crate::app::questions_of(a);
                let focus = self.question_focus.min(questions.len().saturating_sub(1));
                let minutes = ((a.expires_at_ms - now).max(0) / 60_000) as i64;
                v().p(px(22.)).gap(px(14.))
                    .child(h().gap(px(8.)).child(ui::diamond(10., theme::ACCENT_TEXT, false)).child(ui::eyebrow("QUESTION")))
                    .child(div().font_family(ui::serif(cx)).text_size(px(21.)).child(format!("{} asks you", a.requester)))
                    .child(self.inbox_context(Some(a.requester.clone()), a.task_id, a.conversation_id, a.requested_at_ms, now, cx))
                    .child(self.question_form(a, Some(focus), cx))
                    .child(h().gap(px(6.)).items_center().text_size(px(11.5)).text_color(c(theme::MUTED)).child(format!("Expires in {minutes} min")).child(ui::info("question-expiry-about", "After that, Claude continues on its own judgment.")))
                    .into_any_element()
            }
            Some(a) => {
                let minutes = ((a.expires_at_ms - now).max(0) / 60_000) as i64;
                let row = |label: &'static str, value: String| {
                    h().gap(px(12.)).items_start().text_size(px(13.)).child(div().w(px(110.)).flex_none().text_color(c(theme::MUTED)).child(label)).child(div().flex_1().min_w_0().child(value))
                };
                let (id, raw) = (a.id, self.approval_raw.contains(&a.id));
                v().p(px(22.)).gap(px(12.))
                    .child(h().gap(px(8.)).child(ui::diamond(10., theme::ACCENT_TEXT, false)).child(ui::eyebrow("APPROVAL NEEDED")))
                    .child(div().font_family(ui::serif(cx)).text_size(px(21.)).child(crate::screens::approval::sentence(a)))
                    .child(self.inbox_context(Some(a.requester.clone()), a.task_id, a.conversation_id, a.requested_at_ms, now, cx))
                    .child(self.approval_detail(a, "inbox", cx))
                    .when(!a.reasons.is_empty(), |el| el.child(row("Why it asks", a.reasons.join(" "))))
                    .child(h().gap(px(12.)).items_start().text_size(px(13.)).child(div().w(px(110.)).flex_none().text_color(c(theme::MUTED)).child(ui::label_with_info("Expires", "Expiry never counts as permission."))).child(div().flex_1().min_w_0().child(format!("in {minutes} min"))))
                    // Everything as the policy sees it, folded away.
                    .child(h().id("inbox-raw").gap(px(6.)).cursor_pointer().text_size(px(12.)).text_color(c(theme::TEXT_2)).hover(|s| s.text_color(c(theme::TEXT)))
                        .child(if raw { "▾ Details" } else { "▸ Details" })
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            if !this.approval_raw.remove(&id) {
                                this.approval_raw.insert(id);
                            }
                            cx.notify();
                        })))
                    .when(raw, |el| {
                        el.child(row("Tool", a.tool.clone()))
                            .child(row("Effect", a.effect.replace('_', " ")))
                            .child(row("Destination", if a.resources.is_empty() { "—".into() } else { a.resources.join(", ") }))
                            .child(row("Account", a.account.clone()))
                            .child(
                                div().p(px(12.)).rounded(px(6.)).bg(c(theme::SURFACE)).border_1().border_color(c(theme::BORDER))
                                    .font_family(mono.clone()).text_size(px(12.)).text_color(c(theme::TEXT_2))
                                    .child(serde_json_pretty(&a.arguments)),
                            )
                            .child(div().text_size(px(11.)).text_color(c(theme::MUTED)).font_family(mono.clone()).child(format!("digest {}", &a.action_digest[..16.min(a.action_digest.len())])))
                    })
                    .into_any_element()
            }
        };
        let unread_updates = self.inbox_notifications().iter().any(|n| updates::is_update(n) && updates::row_unread(n, &self.inbox_opened));
        let tabs: Vec<AnyElement> = InboxTab::shown(self.personal_inbox_supported()).iter().map(|&t| {
            let i = t as usize;
            let on = t == tab;
            h().id(("inbox-tab", i)).role(Role::Tab).aria_label(t.label()).gap(px(6.)).px(px(10.)).py(px(5.)).rounded(px(6.)).cursor_pointer()
                .text_size(px(13.))
                .text_color(c(if on { theme::TEXT } else { theme::TEXT_2 }))
                .when(on, |el| el.bg(c(theme::SURFACE_2)))
                .hover(|s| s.bg(c(theme::SURFACE_2)).text_color(c(theme::TEXT)))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.set_inbox_tab(t, cx)))
                .when(t == InboxTab::Updates && unread_updates, |el| el.child(ui::dot(6., theme::ACCENT)))
                .child(t.label())
                .child(div().text_size(px(11.5)).text_color(c(if t == InboxTab::NeedsYou && counts[i] > 0 { theme::ACCENT_TEXT } else { theme::MUTED })).child(counts[i].to_string()))
                .into_any_element()
        }).collect();
        // Clear all removes results only, so it sits on the tabs that list them all.
        let mut clear = (matches!(tab, InboxTab::Results | InboxTab::All) || tab == InboxTab::Updates && !notes.is_empty()).then(|| {
            div().id("clear-results").text_size(px(11.5)).text_color(c(theme::ACCENT_TEXT)).cursor_pointer()
                .child("Clear all")
                .on_click(cx.listener(|this, _: &ClickEvent, w, cx| {
                    this.overlay = Overlay::Confirm(ConfirmKind::ClearResults);
                    this.overlay_focus.focus(w, cx);
                    cx.notify();
                }))
        });
        let section = |label: &'static str| h().px(px(10.)).pt(px(10.)).pb(px(4.)).items_center().child(div().flex_1().child(ui::eyebrow(label)));
        // One row: mark, title with unread dot and age, an optional gist, who/project/task, urgency.
        let row = |id: ElementId, i: usize, mark: AnyElement, title: String, unread: bool, gist: Option<String>, meta: String, age: String, urgent: Option<String>| -> AnyElement {
            h().id(id).gap(px(8.)).items_start().px(px(10.)).py(px(8.)).rounded(px(6.)).cursor_pointer()
                .when(i == selected, |el| el.bg(c(theme::SURFACE_2)))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.inbox_selected = i;
                    this.question_focus = 0;
                    cx.notify();
                }))
                .child(h().w(px(10.)).h(px(18.)).flex_none().justify_center().items_center().child(mark))
                .child(
                    v().flex_1().min_w_0().gap(px(1.))
                        .child(h().gap(px(6.))
                            .child(div().flex_1().min_w_0().truncate().text_size(px(13.)).when(unread, |el| el.font_weight(FontWeight::MEDIUM)).child(title))
                            .when(unread, |el| el.child(ui::dot(6., theme::ACCENT)))
                            .child(div().flex_none().text_size(px(11.)).text_color(c(theme::MUTED)).child(age)))
                        .children(gist.map(|g| div().text_size(px(12.)).text_color(c(theme::TEXT_2)).font_family(mono.clone()).truncate().child(g)))
                        .when(!meta.is_empty(), |el| el.child(div().text_size(px(11.5)).truncate().text_color(c(theme::MUTED)).child(meta)))
                        .children(urgent.map(|u| div().text_size(px(11.5)).text_color(c(theme::ACCENT_TEXT)).child(u))),
                )
                .into_any_element()
        };
        // What needs you first, then the results by day, each under its header.
        let mut rows: Vec<AnyElement> = Vec::new();
        if head + notes.iter().filter(|n| crate::app::note_needs_you(n)).count() > 0 {
            rows.push(section("NEEDS YOU").into_any_element());
        }
        for (i, a) in pending.iter().enumerate() {
            let (title, gist) = if a.tool == crate::app::QUESTION_TOOL {
                ("Question".to_string(), String::new())
            } else if a.tool == crate::app::PLAN_TOOL {
                ("Plan".to_string(), crate::app::plan_headline_of(a))
            } else {
                let summary = approval_summary(a, &self.approval_folders(a));
                (summary.title, summary.gist)
            };
            let left = a.expires_at_ms - now;
            let urgent = (left < 10 * 60_000).then(|| format!("Expires in {} min", left.max(0) / 60_000));
            let mark = ui::diamond(8., theme::ACCENT_TEXT, a.tool == crate::app::PLAN_TOOL).into_any_element();
            rows.push(row(("inbox", i).into(), i, mark, title, false, (!gist.is_empty()).then_some(gist), self.inbox_meta(Some(&a.requester), a.task_id, ""), ui::relative_age(a.requested_at_ms, now), urgent));
        }
        for (j, p) in proposals.iter().enumerate() {
            let title = if p.kind == aurelius_domain::proposal::ProposalKind::Instructions {
                format!("Instructions: {}", p.target_employee_name.clone().unwrap_or_else(|| p.name.clone()))
            } else {
                format!("Colleague: {}", p.name)
            };
            let mark = ui::dot(7., theme::MANAGER_MARK).into_any_element();
            rows.push(row(("inbox-proposal", j).into(), pending.len() + j, mark, title, false, None, format!("Proposed by {}", p.proposed_by_name), ui::relative_age(p.created_at_ms, now), None));
        }
        for (j, p) in guideline_proposals.iter().enumerate() {
            let (title, meta) = self.guideline_proposal_row(p);
            let mark = ui::dot(7., theme::MANAGER_MARK).into_any_element();
            rows.push(row(("inbox-guideline-proposal", j).into(), pending.len() + proposals.len() + j, mark, title, false, None, meta, ui::relative_age(p.created_at_ms, now), None));
        }
        let mut day = None;
        for (j, n) in notes.iter().enumerate() {
            if !crate::app::note_needs_you(n) {
                let group = ui::day_group(n.created_at_ms);
                if day != Some(group) {
                    day = Some(group);
                    rows.push(section(group).children(clear.take()).into_any_element());
                }
            }
            if updates::is_update(n) {
                rows.push(self.update_row(n, j, head + j, head + j == selected, now, cx));
                continue;
            }
            let (_, color) = note_mark(n);
            let mark = if color == theme::TEXT_2 { ui::ring(7., theme::MUTED) } else { ui::dot(7., color) }.into_any_element();
            let meta = self.inbox_meta(None, n.task_id, &n.subject);
            rows.push(row(("inbox-note", j).into(), head + j, mark, n.title.clone(), self.inbox_new.contains(&n.id), None, meta, ui::relative_age(n.created_at_ms, now), None));
        }
        v().size_full()
            .min_w_0()
            .track_focus(&self.focus)
            .key_context(INBOX)
            .child(
                h().px(px(22.)).py(px(14.)).gap(px(18.)).border_b_1().border_color(c(theme::BORDER))
                    .child(div().flex_none().font_family(ui::serif(cx)).text_size(px(22.)).child("Inbox"))
                    .child(h().flex_1().min_w_0().gap(px(2.)).children(tabs))
                    .when(self.personal_inbox_supported() && self.inbox_notifications().iter().any(|n| !n.read), |el| {
                        el.child(div().id("inbox-mark-all-read").flex_none().text_size(px(11.5)).text_color(c(theme::ACCENT_TEXT)).cursor_pointer()
                            .tooltip(ui::tip(format!("Only for you. {}", updates::OTHERS_KEEP_THEIRS), None))
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                let _ = this.requests.send(crate::backend::Request::MarkNotificationsRead);
                                cx.notify();
                            }))
                            .child("Mark all read"))
                    }),
            )
            .child(
                h().items_stretch().flex_1().min_h_0()
                    .child(
                        v().id("inbox-list").w(px(340.)).flex_shrink(1.).min_w(px(200.)).overflow_y_scrollbar().p(px(8.)).gap(px(2.)).border_r_1().border_color(c(theme::BORDER))
                            .children(rows),
                    )
                    .child(div().id("inbox-detail").flex_1().min_w_0().overflow_y_scrollbar().child(detail)),
            )
            .child(
                h().px(px(22.)).py(px(14.)).gap(px(8.)).border_t_1().border_color(c(theme::BORDER))
                    .when_some(update_selected.as_ref(), |el, n| {
                        el.child(div().flex_1().min_w_0().text_size(px(11.)).text_color(c(theme::MUTED)).child(ui::key_hint(&["down", "up"], "select · H L tabs", cx)))
                            .children(self.update_actions(n, cx))
                    })
                    .when(note_selected.is_some() && update_selected.is_none(), |el| {
                        el.child(div().flex_1().min_w_0().text_size(px(11.)).text_color(c(theme::MUTED)).child(ui::key_hint(&["down", "up"], "select · H L tabs", cx)))
                            .children(note_conversation.map(|id| {
                                ui::button("open-note-conversation", "Open conversation", ButtonKind::Outline, None)
                                    .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.open_conversation_anywhere(id, w, cx)))
                            }))
                            .when(note_task_id.is_some(), |el| el.child(ui::button("open-note", "Open task", ButtonKind::Primary, Some(ui::kbd("enter", cx))).on_click(cx.listener(|_, _: &ClickEvent, w, cx| w.dispatch_action(Box::new(OpenSelected), cx)))))
                    })
                    .when(question_selected, |el| {
                        el.child(div().flex_1().min_w_0().text_size(px(11.)).text_color(c(theme::MUTED)).child(ui::key_hint(&["1", "4"], "choose · Tab next question · T your own answer", cx)))
                            .child(ui::button("skip-question", "Skip", ButtonKind::Outline, Some(ui::kbd("secondary-backspace", cx))).on_click(cx.listener(|_, _: &ClickEvent, w, cx| w.dispatch_action(Box::new(RejectApproval), cx))))
                            .child(ui::button("send-answer", "Send answer", ButtonKind::Primary, Some(ui::kbd("secondary-enter", cx))).on_click(cx.listener(|_, _: &ClickEvent, w, cx| w.dispatch_action(Box::new(ConfirmPrimary), cx))))
                    })
                    .when(plan_selected, |el| {
                        el.child(div().flex_1().min_w_0().text_size(px(11.)).text_color(c(theme::MUTED)).child(ui::key_hint(&["down", "up"], "select", cx)).child(ui::info("plan-footer-about", "You decide on the plan in its panel.")))
                            .child(ui::button("open-plan-approval", "Review plan", ButtonKind::Primary, Some(ui::kbd("secondary-enter", cx))).on_click(cx.listener(|_, _: &ClickEvent, w, cx| w.dispatch_action(Box::new(ConfirmPrimary), cx))))
                    })
                    .when(proposal_selected.is_some(), |el| {
                        el.child(div().flex_1().min_w_0().text_size(px(11.)).text_color(c(theme::MUTED)).child(ui::key_hint(&["down", "up"], "select", cx)).child(ui::info("proposal-footer-about", "Nothing changes until you approve.")))
                            .child(ui::button("decline-proposal", "Decline", ButtonKind::Outline, Some(ui::kbd("secondary-backspace", cx))).on_click(cx.listener(|_, _: &ClickEvent, w, cx| w.dispatch_action(Box::new(RejectApproval), cx))))
                            .child(ui::button("approve-proposal", "Approve", ButtonKind::Primary, Some(ui::kbd("secondary-enter", cx))).on_click(cx.listener(|_, _: &ClickEvent, w, cx| w.dispatch_action(Box::new(ConfirmPrimary), cx))))
                    })
                    .when(note_selected.is_none() && proposal_selected.is_none() && !question_selected && !plan_selected, |el| el
                    .child(div().flex_1().min_w_0().text_size(px(11.)).text_color(c(theme::MUTED)).child(ui::key_hint(&["down", "up"], "select", cx)).child(ui::info("approve-footer-about", if always.is_some() { "Approve once allows only this exact action." } else { "Only this exact action; there is no 'always allow'." })))
                    .child(ui::button("reject-approval", "Reject", ButtonKind::Outline, Some(ui::kbd("secondary-backspace", cx))).on_click(cx.listener(|_, _: &ClickEvent, w, cx| w.dispatch_action(Box::new(RejectApproval), cx))))
                    .children(always)
                    .child(ui::button("approve-once", "Approve once", ButtonKind::Primary, Some(ui::kbd("secondary-enter", cx))).on_click(cx.listener(|_, _: &ClickEvent, w, cx| w.dispatch_action(Box::new(ConfirmPrimary), cx))))),
            )
    }

    fn render_confirm(&mut self, kind: ConfirmKind, max_h: f32, scroll: &ScrollHandle, cx: &mut Context<Self>) -> impl IntoElement {
        let (title, body): (String, &str) = match kind {
            ConfirmKind::ArchiveManager(id) => (
                format!("Archive {}?", self.managers.iter().find(|m| m.id == id).map_or("Manager", |m| m.name.as_str())),
                "The employee stops starting work and disappears from the lists. Conversations and history are kept.",
            ),
            ConfirmKind::ArchiveProject(id) => (
                format!("Archive {}?", self.projects.iter().find(|p| p.id == id).map_or("Project", |p| p.name.as_str())),
                "The project disappears from Projects. Nothing is deleted; the history is kept.",
            ),
            ConfirmKind::DeleteConversation(id) => (
                format!(
                    "Delete “{}”?",
                    self.managers.iter().flat_map(|m| &m.conversations).find(|c| c.id == id).map(|c| c.title.as_str())
                        .or_else(|| self.tasks.iter().find(|t| t.conversation_id == Some(id)).map(|t| t.title.as_str()))
                        .unwrap_or("this conversation")
                ),
                "The conversation disappears and its messages and attachments are erased; an answer still running in it is stopped. This can't be undone.",
            ),
            ConfirmKind::RestoreBackup(index) => (
                format!("Restore the backup of {}?", self.backups.as_ref().and_then(|b| b.backups.get(index)).map_or(String::new(), |b| ui::fmt_when(b.created_at_ms))),
                "Aurelius restarts and returns to that moment. Your current data is kept aside (not deleted). Afterwards nothing runs until you resume. API keys are not in backups; reconnect them if asked.",
            ),
            ConfirmKind::AllowBrowser(_) => (
                "Allow the browser?".to_string(),
                "This agent can then use your own Chrome, which is signed in to your accounts. Every browser action is still shown to you first, also in Auto mode. Task workers never get the browser.",
            ),
            ConfirmKind::AllowScreen(_) => (
                "Let this agent see your screen?".to_string(),
                "It can then take screenshots of your screen or of one window without asking, whenever it thinks that helps. A screenshot shows anything that is open, also private things. Off, it still asks you for each screenshot. Task workers never get this.",
            ),
            ConfirmKind::RestartServer => (
                format!("Restart {}?", self.remote_name().unwrap_or("the server")),
                "Answers that are running now are interrupted. This app reconnects by itself once the server is back.",
            ),
            ConfirmKind::ClearResults => (
                if self.personal_inbox_supported() {
                    format!("Clear all {} results and updates?", self.inbox_notifications().iter().filter(|n| matches!(note_kind(n), NoteKind::Result | NoteKind::Failed | NoteKind::Update)).count())
                } else {
                    format!("Clear all {} results?", self.inbox_counts()[InboxTab::Results as usize])
                },
                if self.personal_inbox_supported() {
                    "Every result and update in your inbox is removed. Approvals waiting for you are not touched. Others keep theirs."
                } else {
                    "Every result in the inbox is removed. Approvals waiting for you are not touched."
                },
            ),
            ConfirmKind::ShutdownNow => (
                format!("Shut down {}?", self.remote_name().map_or("this PC".to_string(), |h| format!("{h}'s PC"))),
                "The PC that runs the server powers off after a 60 second countdown. Answers that are running are lost, and the server is unreachable until someone starts the PC again. You can cancel during the countdown.",
            ),
            ConfirmKind::DeleteSchedule(_) => (
                "Delete this recurring task?".to_string(),
                "It stops making tasks. Tasks it already made stay where they are.",
            ),
            ConfirmKind::DeleteIdea(id) => (
                format!("Delete “{}”?", self.ideas.iter().flat_map(|(_, l)| l).find(|i| i.id == id).map_or("this idea", |i| i.title.as_str())),
                "The idea and its files are erased; Drop keeps it instead. A task it became stays, with its own copies of the files. This can't be undone.",
            ),
            ConfirmKind::ForgetMemory(_) => (
                "Forget this memory?".to_string(),
                "It is never used as context again, leaves the search index and its content is erased. The timeline keeps the fact that it existed.",
            ),
            ConfirmKind::RevokeSession(session, of) => (
                format!("Sign out {}?", self.session_name(session, of)),
                "It needs your password to come back. That device is signed out at once; the timeline says who did it.",
            ),
            ConfirmKind::RevokeOtherSessions => (
                "Sign out all other devices?".to_string(),
                "Every device signed in as you, except this one, is signed out at once and needs your password to come back.",
            ),
            ConfirmKind::DisableMember(user) => (
                format!("Disable {}?", self.member_name(user)),
                if self.active_member().map(|u| u.id) == Some(user) {
                    "That is you: you are signed out on every device at once, also here, and cannot sign in again. Your Claude login is disconnected. Your tickets keep you as their creator; they wait until another board member runs them on their own login."
                } else {
                    "They are signed out on every device at once and cannot sign in again. Their Claude login is disconnected. Their tickets keep them as creator and wait until another board member runs them on their own login."
                },
            ),
            ConfirmKind::UseMyLogin(ticket) => (
                format!("Run #{} on your Claude login?", self.board.ticket(ticket).map_or(0, |t| t.number)),
                "From now on this ticket and all its work use your own Claude subscription instead of its creator's. Its creator stays. Nothing else runs on your login.",
            ),
            ConfirmKind::LinkOtherServer(link) => (
                self.link_question(&link).0,
                "The window starts over on that server, and the link opens there.",
            ),
            ConfirmKind::ArchiveVaultItem(item) => (
                crate::screens::vault::delete_title(self.vault.item(item)),
                crate::screens::vault::DELETE_BODY,
            ),
            ConfirmKind::LooksLikeSecret => (
                crate::screens::vault::held_title(self.vault.held.as_ref().map_or(&[][..], |h| h.names.as_slice())),
                crate::screens::vault::held_body(self.vault.held.as_ref().map(|h| &h.command)),
            ),
            ConfirmKind::DeleteMessage(message) => (
                crate::screens::message_edit::DELETE_TITLE.to_string(),
                crate::screens::message_edit::DELETE_BODY,
            ),
            ConfirmKind::DisconnectClaude => (
                "Disconnect your Claude login?".to_string(),
                "Claude Code signs out on the server and your login's folder there is removed. Your tickets wait until you connect again; nothing runs on someone else's login.",
            ),
        };
        // AUR-FA §2.4: their tickets keep them as Responsible; nothing is wiped silently.
        let responsible_for = match kind {
            ConfirmKind::DisableMember(user) if self.ticket_responsible_supported() => {
                let tickets = self.board.view.iter().flat_map(|v| v.columns.iter()).flat_map(|c| c.tickets.iter());
                crate::screens::who::disable_note(&self.member_name(user), tickets.filter(|t| t.responsible_user_id == Some(user) && t.column != aurelius_domain::ticket::ColumnKind::Done).count())
            }
            _ => None,
        };
        let responsible_for = match kind {
            ConfirmKind::DeleteMessage(message) => self.delete_ticket_note(message),
            _ => responsible_for,
        };
        let switch_label = match kind {
            ConfirmKind::LinkOtherServer(link) => format!("Switch to {}", self.link_question(&link).1),
            _ => String::new(),
        };
        let confirm_label = match kind {
            ConfirmKind::LinkOtherServer(_) => switch_label.as_str(),
            ConfirmKind::ForgetMemory(_) => "Forget",
            ConfirmKind::DeleteConversation(_) | ConfirmKind::DeleteSchedule(_) | ConfirmKind::DeleteIdea(_) => "Delete",
            ConfirmKind::RestoreBackup(_) => "Restore",
            ConfirmKind::AllowBrowser(_) => "Allow browser",
            ConfirmKind::AllowScreen(_) => "Allow screen",
            ConfirmKind::RestartServer => "Restart",
            ConfirmKind::ShutdownNow => "Shut down",
            ConfirmKind::ClearResults => "Clear all",
            ConfirmKind::RevokeSession(..) | ConfirmKind::RevokeOtherSessions => "Sign out",
            ConfirmKind::DisableMember(_) => "Disable",
            ConfirmKind::DisconnectClaude => "Disconnect",
            ConfirmKind::UseMyLogin(_) => "Run on my login",
            ConfirmKind::ArchiveVaultItem(_) | ConfirmKind::DeleteMessage(_) => "Delete",
            ConfirmKind::LooksLikeSecret => "Send anyway",
            _ => "Archive",
        };
        card(460.)
            .track_focus(&self.overlay_focus)
            .key_context(DIALOG)
            .child(
                ui::scroll_frame(scroll, scroll_body("confirm-body", scroll, (max_h - 70.).max(120.)).p(px(22.)).gap(px(10.))
                    .child(div().font_family(ui::serif(cx)).text_size(px(21.)).child(title))
                    .child(div().text_size(px(13.)).line_height(px(19.)).text_color(c(theme::TEXT_2)).child(body))
                    .children(responsible_for.map(|text| div().text_size(px(13.)).line_height(px(19.)).text_color(c(theme::TEXT_2)).child(text)))),
            )
            .child(
                h().px(px(22.)).py(px(14.)).gap(px(8.)).justify_end().border_t_1().border_color(c(theme::BORDER))
                    .child(ui::button("keep", "Cancel", ButtonKind::Outline, Some(ui::kbd("escape", cx))).on_click(cx.listener(|this, _: &ClickEvent, w, cx| this_back(this, w, cx))))
                    .child(ui::button("archive", confirm_label, ButtonKind::Primary, Some(ui::kbd("secondary-enter", cx))).on_click(cx.listener(|_, _: &ClickEvent, w, cx| w.dispatch_action(Box::new(ConfirmPrimary), cx)))),
            )
    }
}

impl AppView {
    fn render_subscription(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let s = crate::screens::account::own_subscription(self.subscription.as_ref(), self.active_member().map(|u| u.id)).cloned();
        let plan = s.as_ref().and_then(|s| s.plan.clone()).map(|p| format!(" · {}", p.to_uppercase())).unwrap_or_default();
        let mut section = v().gap(px(12.)).p(px(16.)).rounded(px(9.)).border_1().border_color(c(theme::BORDER)).bg(c(theme::SURFACE))
            .child(h().justify_between().child(ui::eyebrow(format!("CLAUDE SUBSCRIPTION{plan}"))).child(
                h().id("sub-refresh").gap(px(5.)).cursor_pointer().text_size(px(11.5)).text_color(c(theme::MUTED)).hover(|st| st.text_color(c(theme::TEXT)))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        let _ = this.requests_handle().send(crate::backend::Request::LoadSubscription(true));
                        cx.notify();
                    }))
                    .child("Refresh"),
            ));
        match s {
            Some(s) if s.fetched_at_ms == 0 => section = section.child(div().text_size(px(12.5)).text_color(c(theme::MUTED)).child("Fetching via Claude Code…")),
            None => section = section.child(div().text_size(px(12.5)).text_color(c(theme::MUTED)).child("Fetching via Claude Code…")),
            Some(s) if s.error.is_some() => section = section.child(div().text_size(px(12.5)).text_color(c(theme::MUTED)).child(s.error.unwrap_or_default())),
            Some(s) if s.limits.is_empty() => {
                section = section.child(div().text_size(px(12.)).line_height(px(17.)).text_color(c(theme::TEXT_2)).child(s.raw.lines().take(8).collect::<Vec<_>>().join("\n")))
            }
            Some(s) => {
                for (i, l) in s.limits.iter().enumerate() {
                    let pct = l.percent as f32 / 100.0;
                    let color = if l.percent >= 90 { theme::NEGATIVE } else if l.percent >= 70 { 0xd99a3a } else { theme::ACCENT };
                    section = section.child(
                        v().gap(px(6.))
                            .child(h().justify_between().text_size(px(13.))
                                .child(div().child(l.label.clone()))
                                .child(div().font_weight(FontWeight::MEDIUM).child(format!("{}%", l.percent))))
                            .child(
                                div().w_full().h(px(8.)).rounded(px(4.)).bg(c(theme::SURFACE_2)).child(
                                    div().h_full().rounded(px(4.)).bg(c(color)).w(relative(0.0))
                                        .with_animation(("sub-bar", i), Animation::new(std::time::Duration::from_millis(600 + i as u64 * 80)), move |el, t| {
                                            el.w(relative(pct * (1.0 - (1.0 - t).powi(3))))
                                        }),
                                ),
                            )
                            .when_some(l.resets.clone(), |el, r| el.child(div().text_size(px(11.5)).text_color(c(theme::MUTED)).child(format!("Resets: {r}")))),
                    );
                }
                section = section.child(div().text_size(px(11.)).text_color(c(theme::MUTED)).child("Via Claude Code (/usage) with your own login; free to check. Includes usage outside Aurelius."));
            }
        }
        section
    }

    /// Usage: tokens (and cost when reported) today, 7 and 30 days, per day and per source.
    fn render_usage(&mut self, max_h: f32, cx: &mut Context<Self>) -> impl IntoElement {
        let mono = ui::mono(cx);
        let serif = ui::serif(cx);
        let Some(usage) = self.usage.clone() else {
            return card(760.).track_focus(&self.overlay_focus).key_context(INBOX).child(div().p(px(22.)).text_color(c(theme::MUTED)).child("Loading usage…"));
        };
        let cost_text = |cost: Option<i64>, estimate: bool| match cost {
            Some(0) => "no cost".to_string(),
            Some(c) => format!("{}{}", ui::fmt_cost(c), if estimate { " (estimate)" } else { "" }),
            None => "cost not reported".to_string(),
        };
        let windows = h().gap(px(12.)).children(usage.windows.iter().enumerate().map(|(i, w)| {
            ui::rise(
                v().flex_1().gap(px(6.)).p(px(16.)).rounded(px(9.)).border_1().border_color(c(theme::BORDER)).bg(c(theme::SURFACE))
                    .child(ui::eyebrow(w.label.to_uppercase()))
                    .child(div().font_family(serif.clone()).text_size(px(28.)).child(ui::fmt_tokens(w.input_tokens + w.output_tokens)))
                    .child(div().text_size(px(12.)).text_color(c(theme::TEXT_2)).child(format!(
                        "in {} · out {} · cache {}",
                        ui::fmt_tokens(w.input_tokens), ui::fmt_tokens(w.output_tokens), ui::fmt_tokens(w.cached_tokens)
                    )))
                    .child(div().text_size(px(12.)).text_color(c(theme::MUTED)).child(format!("{} runs · {}", w.runs, cost_text(w.cost_micro_usd, w.estimate)))),
                ("usage-window", i),
                i,
            )
        }));
        let max = usage.by_day.iter().map(|d| d.tokens).max().unwrap_or(0).max(1);
        let weekday = |ms: i64| {
            let local = ms + aurelius_host::utc_offset_minutes_at(ms) * 60_000;
            ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"][local.div_euclid(86_400_000).rem_euclid(7) as usize]
        };
        let selected = self.usage_selected_day();
        let chart = h().h(px(120.)).gap(px(6.)).items_end().children(usage.by_day.iter().enumerate().map(|(i, d)| {
            let height = if d.tokens == 0 { 2.0 } else { 6.0 + 94.0 * d.tokens as f32 / max as f32 };
            let picked = selected == Some(d.day_start_ms);
            let day = d.day_start_ms;
            v().id(("usage-day", i)).flex_1().h_full().justify_end().items_center().gap(px(5.)).rounded(px(4.)).cursor_pointer()
                .role(Role::Button)
                .aria_label(ui::fmt_day(day))
                .hover(|s| s.bg(c(theme::SURFACE_2)))
                .tooltip(ui::tip(format!("{} · {} tokens", ui::fmt_day(day), ui::fmt_tokens(d.tokens)), None))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.select_usage_day(day, cx)))
                .child(div().text_size(px(9.5)).text_color(c(theme::MUTED)).child(if d.tokens > 0 { ui::fmt_tokens(d.tokens) } else { String::new() }))
                .child(
                    div().w_full().h(px(height)).rounded(px(3.)).bg(c(if picked { theme::ACCENT } else { theme::BORDER_STRONG }))
                        .with_animation(("usage-bar", i), Animation::new(std::time::Duration::from_millis(500 + i as u64 * 25)), move |el, t| {
                            let e = 1.0 - (1.0 - t).powi(3);
                            el.h(px((height * e).max(1.0)))
                        }),
                )
                .child(div().text_size(px(10.)).text_color(c(if picked { theme::TEXT } else { theme::MUTED })).child(weekday(d.day_start_ms)))
        }));
        let hours = self.render_usage_hours(&usage, selected, cx);
        let rows = v().gap(px(2.)).children(usage.by_source.iter().map(|r| {
            h().gap(px(12.)).py(px(7.)).border_b_1().border_color(c(theme::SURFACE_2)).text_size(px(12.5))
                .child(v().flex_1().min_w_0().child(div().child(r.connection.clone())).child(div().text_size(px(11.5)).text_color(c(theme::MUTED)).child(r.model.clone())))
                .child(div().w(px(70.)).text_color(c(theme::TEXT_2)).child(format!("{} runs", r.runs)))
                .child(div().w(px(150.)).font_family(mono.clone()).text_size(px(11.5)).text_color(c(theme::TEXT_2)).child(format!("{} in · {} out", ui::fmt_tokens(r.input_tokens), ui::fmt_tokens(r.output_tokens))))
                .child(div().w(px(130.)).text_color(c(theme::TEXT_2)).child(cost_text(r.cost_micro_usd, r.estimate)))
        }));
        card(820.)
            .track_focus(&self.overlay_focus)
            .key_context(INBOX)
            .child(
                h().px(px(22.)).py(px(14.)).border_b_1().border_color(c(theme::BORDER))
                    .child(h().flex_1().gap(px(10.)).child(ui::usage_glyph(theme::ACCENT)).child(div().font_family(serif.clone()).text_size(px(20.)).child("Usage")))
                    .child(ui::close_button(cx)),
            )
            .child(
                v().id("usage-body").h(px((max_h - 150.).clamp(120., 600.))).min_h_0().overflow_y_scrollbar().px(px(22.)).py(px(18.)).gap(px(22.))
                    .child(self.render_subscription(cx))
                    .child(v().gap(px(10.)).child(ui::eyebrow("AURELIUS · TOKENS")).child(windows))
                    .child(v().gap(px(10.)).child(ui::eyebrow("TOKENS PER DAY · 14 DAYS")).child(chart))
                    .child(hours)
                    .child(
                        v().gap(px(6.)).child(ui::eyebrow("PER CONNECTION AND MODEL · 30 DAYS"))
                            .when(usage.by_source.is_empty(), |el| el.child(div().text_size(px(12.5)).text_color(c(theme::MUTED)).child("No usage yet.")))
                            .child(rows),
                    ),
            )
            .child(
                div().px(px(22.)).py(px(12.)).border_t_1().border_color(c(theme::BORDER)).text_size(px(11.5)).line_height(px(17.)).text_color(c(theme::MUTED))
                    .child("Claude Code on your subscription: costs are the API-equivalent value for reference; you pay nothing extra. API keys: tokens as reported by the provider. The local test runtime estimates tokens (4 characters ≈ 1 token)."),
            )
    }

    /// The selected day per hour: 24 bars (input and output stacked), the
    /// day's total, and buttons to the previous / next of the 14 days.
    fn render_usage_hours(&self, usage: &aurelius_protocol::UsageSnapshot, selected: Option<i64>, cx: &mut Context<Self>) -> Div {
        let Some(day) = selected else { return v() };
        let index = usage.by_day.iter().position(|d| d.day_start_ms == day).unwrap_or(0);
        let prev = index.checked_sub(1).and_then(|i| usage.by_day.get(i)).map(|d| d.day_start_ms);
        let next = usage.by_day.get(index + 1).map(|d| d.day_start_ms);
        let step = |id: &'static str, forward: bool, to: Option<i64>| {
            let label = if forward { "Next day" } else { "Previous day" };
            ui::icon_button(id)
                .aria_label(label)
                .tooltip(ui::tip(label, None))
                .when(to.is_none(), |el| el.opacity(0.3).cursor_default())
                .when_some(to, |el, to| el.on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.select_usage_day(to, cx))))
                .child(ui::nav_arrow(forward, 14., theme::TEXT_2))
        };
        let (prev_button, next_button) = (step("usage-prev-day", false, prev), step("usage-next-day", true, next));
        let data = self.usage_hours.as_ref().filter(|(d, _)| *d == day).map(|(_, r)| r);
        let total = data.and_then(|r| r.as_ref().ok()).map(|hours| {
            let sum = |f: fn(&aurelius_protocol::UsageHour) -> u64| hours.hours.iter().map(f).sum::<u64>();
            let costs: Vec<i64> = hours.hours.iter().filter_map(|x| x.cost_micro_usd).collect();
            let cost = match costs.is_empty() {
                true => String::new(),
                false => format!(" · {}{}", ui::fmt_cost(costs.iter().sum()), if hours.hours.iter().any(|x| x.estimate) { " (estimate)" } else { "" }),
            };
            let (input, output) = (sum(|x| x.input_tokens), sum(|x| x.output_tokens));
            format!(
                "{} tokens · in {} · out {} · cache {} · {} runs{cost}",
                ui::fmt_tokens(input + output), ui::fmt_tokens(input), ui::fmt_tokens(output), ui::fmt_tokens(sum(|x| x.cached_tokens)), sum(|x| x.runs)
            )
        });
        let note = |text: String| div().h(px(120.)).flex().items_center().text_size(px(12.5)).text_color(c(theme::MUTED)).child(text);
        let body = match data {
            None => note("Loading…".into()).into_any_element(),
            Some(Err(e)) => note(e.clone()).into_any_element(),
            Some(Ok(hours)) => {
                let max = hours.hours.iter().map(|x| x.input_tokens + x.output_tokens).max().unwrap_or(0).max(1);
                let bars = h().h(px(120.)).gap(px(3.)).items_end().children(hours.hours.iter().enumerate().map(|(i, x)| {
                    let tokens = x.input_tokens + x.output_tokens;
                    let height = if tokens == 0 { 2.0 } else { 4.0 + 96.0 * tokens as f32 / max as f32 };
                    let out_h = if tokens == 0 { 0. } else { height * x.output_tokens as f32 / tokens as f32 };
                    let cost = match x.cost_micro_usd {
                        Some(c) => format!(" · {}{}", ui::fmt_cost(c), if x.estimate { " (estimate)" } else { "" }),
                        None => String::new(),
                    };
                    let tip = format!(
                        "{i:02}:00–{:02}:00 · in {} · out {} · cache {} · {} run{}{cost}",
                        (i + 1) % 24, ui::fmt_tokens(x.input_tokens), ui::fmt_tokens(x.output_tokens), ui::fmt_tokens(x.cached_tokens), x.runs, if x.runs == 1 { "" } else { "s" }
                    );
                    v().id(("usage-hour", i)).flex_1().h_full().justify_end().items_center().gap(px(5.)).rounded(px(3.))
                        .hover(|s| s.bg(c(theme::SURFACE_2)))
                        .tooltip(ui::tip(tip, None))
                        .child(
                            // Output stacked on input; the bar grows again when the day changes.
                            v().w_full().h(px(height)).justify_end().overflow_hidden().rounded(px(2.))
                                .when(tokens == 0, |el| el.bg(c(theme::BORDER)))
                                .child(div().w_full().h(px(out_h)).flex_none().bg(c(theme::ACCENT)))
                                .child(div().w_full().h(px(if tokens == 0 { 0. } else { height - out_h })).flex_none().bg(c(theme::BORDER_STRONG)))
                                .with_animation(("usage-hour-bar", index * 24 + i), Animation::new(std::time::Duration::from_millis(350 + i as u64 * 12)), move |el, t| {
                                    let e = 1.0 - (1.0 - t).powi(3);
                                    el.h(px((height * e).max(1.0)))
                                }),
                        )
                        .child(div().text_size(px(9.5)).text_color(c(theme::MUTED)).child(format!("{i:02}")))
                }));
                let swatch = |color: u32, label: &'static str| h().gap(px(5.)).child(div().size(px(8.)).rounded(px(2.)).bg(c(color))).child(label);
                v().gap(px(8.))
                    .child(bars)
                    .child(h().gap(px(14.)).text_size(px(11.)).text_color(c(theme::MUTED)).child(swatch(theme::BORDER_STRONG, "Input")).child(swatch(theme::ACCENT, "Output")))
                    .into_any_element()
            }
        };
        v().gap(px(10.))
            .child(
                h().gap(px(8.))
                    .child(ui::eyebrow(format!("PER HOUR · {}", ui::fmt_day(day).to_uppercase())))
                    .child(div().flex_1().min_w_0().truncate().text_size(px(11.5)).text_color(c(theme::TEXT_2)).child(total.unwrap_or_default()))
                    .child(prev_button)
                    .child(next_button),
            )
            .child(body)
    }

    /// Context inspector: what the Context Builder used for the last answer, and why.
    fn render_context(&mut self, max_h: f32, scroll: &ScrollHandle, cx: &mut Context<Self>) -> impl IntoElement {
        let mono = ui::mono(cx);
        let Some(view) = self.context_view.clone() else { return card(720.).child(div().p(px(22.)).child("Loading…")) };
        let section_title = |s: &str| match s {
            "rules" => "Instructions",
            "request" => "Question",
            "turn" => "Conversation",
            "summary" => "Summary",
            "memory" => "Memory",
            "guideline" => "Guidelines",
            _ => "Other",
        };
        // A guideline's version is in its id (P-59): visible here without trace mode too.
        let label = |i: &aurelius_protocol::ContextItemView| if i.section == "guideline" { crate::screens::guidelines::context_guideline(&i.id, &i.label) } else { i.label.clone() };
        let used = view.items.iter().filter(|i| i.included).count();
        card(760.)
            .track_focus(&self.overlay_focus)
            .key_context(INBOX)
            .child(
                h().px(px(22.)).py(px(14.)).border_b_1().border_color(c(theme::BORDER))
                    .child(v().flex_1().gap(px(2.))
                        .child(div().font_family(ui::serif(cx)).text_size(px(20.)).child("Context used"))
                        .child(div().text_size(px(12.)).text_color(c(theme::MUTED)).child(format!(
                            "{used} of {} items · ~{} of {} tokens · {} permitted scopes{}",
                            view.items.len(), view.used_tokens, view.budget_tokens, view.scopes.len(),
                            if view.runtime_managed { " · runtime beheert eigen historie" } else { "" }
                        ))))
                    .child(ui::close_button(cx)),
            )
            .child(
                ui::scroll_frame(scroll, scroll_body("context-items", scroll, (max_h - 130.).clamp(120., 460.)).px(px(22.)).py(px(12.)).gap(px(6.)).children(view.items.iter().map(|i| {
                    h().gap(px(10.)).items_start().text_size(px(12.5))
                        .child(div().w(px(14.)).flex_none().text_color(c(if i.included { theme::ACCENT } else { theme::MUTED })).child(if i.included { "✓" } else { "–" }))
                        .child(div().w(px(96.)).flex_none().text_color(c(theme::MUTED)).child(section_title(&i.section)))
                        .child(v().flex_1().min_w_0()
                            .child(div().text_color(c(if i.included { theme::TEXT } else { theme::TEXT_2 })).child(label(i)))
                            .child(div().text_size(px(11.5)).text_color(c(theme::MUTED)).child(i.reason.clone())))
                        .child(div().flex_none().font_family(mono.clone()).text_size(px(11.)).text_color(c(theme::MUTED)).child(match i.score {
                            Some(s) => format!("{} tok · {s:.2}", i.tokens),
                            None => format!("{} tok", i.tokens),
                        }))
                }))),
            )
            .child(
                h().px(px(22.)).py(px(12.)).border_t_1().border_color(c(theme::BORDER)).text_size(px(11.5)).text_color(c(theme::MUTED))
                    .child("Only permitted scopes are searched, before any text is loaded. Memory is data, not instructions."),
            )
    }
}

fn this_back(_: &mut AppView, window: &mut Window, cx: &mut Context<AppView>) {
    window.dispatch_action(Box::new(Back), cx);
}

/// Every page of Settings, by the index `settings_tab` holds. The navigation shows four
/// sections (`SETTINGS_NAV`); Advanced holds eight of these pages, "Servers & PC" three.
const SETTINGS_SECTIONS: [&str; 14] = ["Connections", "App", "Backups", "Remote access", "Servers", "Load limits", "App keys", "Plugins", "MCP & tools", "Server & PC", "Company", "Recurring schedules", "Trace", "Account"];

/// Settings → Server & PC: the server's version and updates, restart and shutdown.
pub const SERVER_PC_TAB: usize = 9;

/// Settings → Company: management's mode, the management team, the 3.0 conversion.
pub const COMPANY_TAB: usize = 10;

/// One line under each page's title (same order as SETTINGS_SECTIONS).
const SETTINGS_ABOUT: [&str; 14] = [
    "Runtimes and how to sign in to them",
    "Appearance, start-up and this app's updates",
    "A backup holds the database and all stored files, checked with a manifest. One is made automatically every day (the last 7 are kept); manual backups keep the last 20. Content you delete or forget is removed from existing backups too. Backups are not encrypted by Aurelius; they are protected by your Windows account, like the app data itself. API keys are never in a backup (they stay in the Windows credential store). Export a project as Markdown + JSON with Ctrl Shift A.",
    "Use Aurelius from your phone or another computer over your own Tailscale network. This PC stays the server: projects, memory, credentials and runtimes never leave it. Only devices you allow here can connect; Tailscale decides who can reach this PC at all. No public internet route (no Funnel).",
    "Aurelius runs on one server: the computer that holds your projects, memory, keys and runtimes. This app shows that server. Use the service on this computer, or connect to Aurelius on another computer of yours over Tailscale. A computer you pair gets full access to that server (keys, folders, backups, updates), after you allow it there. Both computers need Tailscale. Forgetting a server here also removes this computer there when it is connected.",
    "How busy the server's computer may get before new runs wait. A waiting run starts by itself once there is room; running answers are never stopped.",
    "Keys that let other apps use Aurelius through aurelius-mcp",
    "Extra skills for employees and projects, from Claude Code plugins. Add one from a link, then choose which employees use it. A change applies from the employee's next run.",
    "MCP servers, always allowed tools, Claude artifacts and the browser for Claude in Chrome",
    "The server's version and updates, restart and shut down. On a remote server this page is about that server and its PC; this app updates separately under App.",
    "How work flows through the company: the management team and the default models",
    "Every recurring task, with its project",
    "What each answer sent and what it cost (rare, server-wide)",
    "Who is signed in on this device, your sessions, your password and your own Claude login",
];

/// The pages behind "Servers & PC" (a row of tabs on top of them) and behind Advanced.
const SERVER_TABS: [usize; 3] = [crate::screens::servers::SERVERS_TAB, crate::screens::remote::REMOTE_TAB, SERVER_PC_TAB];
const ADVANCED_TABS: [usize; 8] = [
    0,
    crate::screens::mcp::MCP_TAB,
    crate::screens::plugins::PLUGINS_TAB,
    crate::screens::app_keys::APP_KEYS_TAB,
    crate::screens::schedules::SCHEDULES_TAB,
    2,
    crate::screens::load::LOAD_TAB,
    crate::screens::trace::TRACE_TAB,
];

/// The navigation: five sections, each with the page it opens first (Advanced's pages show
/// below it while it is open).
static SETTINGS_NAV: [(&str, usize); 5] = [
    ("Account", crate::screens::account::ACCOUNT_TAB),
    ("App", 1),
    ("Company", COMPANY_TAB),
    ("Servers & PC", crate::screens::servers::SERVERS_TAB),
    ("Advanced", 0),
];

/// The navigation's sections "Servers & PC" and Advanced (by index in `SETTINGS_NAV`).
const SERVERS: usize = 3;
const ADVANCED: usize = 4;

/// Which of the navigation sections a page belongs to.
pub(crate) fn nav_section(tab: usize) -> usize {
    if SERVER_TABS.contains(&tab) {
        SERVERS
    } else if ADVANCED_TABS.contains(&tab) {
        ADVANCED
    } else {
        SETTINGS_NAV.iter().position(|(_, first)| *first == tab).unwrap_or(0)
    }
}

/// The page after `tab` in the order the navigation shows them (Tab key).
pub(crate) fn next_settings_tab(tab: usize) -> usize {
    let order: Vec<usize> = [crate::screens::account::ACCOUNT_TAB, 1, COMPANY_TAB].into_iter().chain(SERVER_TABS).chain(ADVANCED_TABS).collect();
    let at = order.iter().position(|&t| t == tab).unwrap_or(order.len() - 1);
    order[(at + 1) % order.len()]
}

/// Right-hand side of the settings dialog (the card and navigation wrap it).
pub(crate) fn settings_page() -> Div {
    v().flex_1().min_w_0()
}

/// The middle part of a centered dialog: as tall as its content up to `max_h`, then it
/// scrolls (wrap it in `ui::scroll_frame` for the scrollbar). Not overflow_y_scrollbar():
/// gpui-component wraps that body in a block with 100% height, which collapses to 0 px
/// in a card that takes its height from its content, so only the footer was left.
pub(crate) fn scroll_body(id: &'static str, scroll: &ScrollHandle, max_h: f32) -> Stateful<Div> {
    v().id(id).max_h(px(max_h)).min_h_0().overflow_y_scroll().track_scroll(scroll)
}

pub(crate) fn card(width: f32) -> Div {
    v().w(px(width))
        .max_w_full()
        .rounded(px(10.))
        .border_1()
        .border_color(c(theme::BORDER_STRONG))
        .bg(c(theme::PANEL))
        .shadow_lg()
        .overflow_hidden()
}

fn field(label: &'static str, input: AnyElement) -> Div {
    v().gap(px(6.)).child(div().text_size(px(12.)).text_color(c(theme::TEXT_2)).child(label)).child(input)
}

fn serde_json_pretty(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_default()
}

/// Eyebrow and colour of a notification in the inbox.
fn note_mark(n: &aurelius_protocol::NotificationView) -> (&'static str, u32) {
    match note_kind(n) {
        NoteKind::NeedsYou => ("NEEDS YOU", theme::ACCENT_TEXT),
        NoteKind::Failed => ("TASK FAILED", theme::NEGATIVE),
        _ if n.severity == "warning" => ("ATTENTION", theme::ACCENT_TEXT),
        _ => ("RESULT", theme::TEXT_2),
    }
}

#[cfg(test)]
mod tests {
    use super::{nav_section, next_settings_tab, ADVANCED_TABS, SERVER_TABS, SETTINGS_ABOUT, SETTINGS_NAV, SETTINGS_SECTIONS};

    #[test]
    fn tab_visits_every_settings_page_once() {
        let mut seen = vec![1];
        let mut tab = next_settings_tab(1);
        while tab != 1 {
            assert!(!seen.contains(&tab), "page {tab} twice");
            seen.push(tab);
            tab = next_settings_tab(tab);
        }
        assert_eq!(seen.len(), SETTINGS_SECTIONS.len());
        assert_eq!(SETTINGS_ABOUT.len(), SETTINGS_SECTIONS.len());
    }

    #[test]
    fn settings_has_five_sections_with_eight_advanced_pages() {
        assert_eq!(SETTINGS_NAV.len(), 5);
        assert_eq!(ADVANCED_TABS.len(), 8);
        assert_eq!(SERVER_TABS.len(), 3);
        for tab in 0..SETTINGS_SECTIONS.len() {
            assert!(nav_section(tab) < SETTINGS_NAV.len());
        }
        assert_eq!(nav_section(crate::screens::schedules::SCHEDULES_TAB), super::ADVANCED);
        assert_eq!(nav_section(crate::screens::remote::REMOTE_TAB), super::SERVERS);
        assert_eq!(SETTINGS_NAV[nav_section(crate::screens::account::ACCOUNT_TAB)].0, "Account");
        assert_eq!(SETTINGS_NAV[nav_section(super::COMPANY_TAB)].0, "Company");
    }
}
