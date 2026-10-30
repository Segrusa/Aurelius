//! The fixed left sidebar: Dashboard, Team chat, Inbox, Tasks, Employees and Projects (the last two foldable,
//! with their pinned employees or projects) and the server picker at the bottom (a click on it opens Settings).

use crate::app::{AppView, ManagerRef, MenuDo, MenuEntry, MenuItems, NavItem, Page, ProjectRef};
use crate::backend::Connection;
use crate::keymap::*;
use crate::theme::{self, c};
use crate::ui::{self, h, v};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

pub const SIDEBAR_WIDTH: f32 = 220.;

/// What a click on a pinned row opens.
#[derive(Clone, Copy)]
enum PinOpen {
    Project(aurelius_domain::ProjectId),
    Employee(aurelius_domain::EmployeeId),
}

/// A sidebar item's icon, drawn with divs like the app's other glyphs, in a 16 x 14 box.
fn nav_icon(nav: NavItem, color: u32) -> Div {
    let fill = |w: f32, h_: f32, r: f32| div().w(px(w)).h(px(h_)).rounded(px(r)).bg(c(color));
    let frame = |w: f32, h_: f32, r: f32| h().w(px(w)).h(px(h_)).rounded(px(r)).border_1().border_color(c(color));
    let boxed = h().w(px(16.)).h(px(14.)).flex_none().justify_center().items_center();
    match nav {
        // Four tiles.
        NavItem::Dashboard => boxed.child(
            v().gap(px(2.)).child(h().gap(px(2.)).child(fill(5., 5., 1.)).child(fill(5., 5., 1.))).child(h().gap(px(2.)).child(fill(5., 5., 1.)).child(fill(5., 5., 1.))),
        ),
        // A speech bubble: a box with two lines of text.
        NavItem::Chat => boxed.child(frame(14., 11., 3.).flex_col().justify_center().items_start().gap(px(2.)).pl(px(2.5)).child(fill(7., 1.5, 1.)).child(fill(5., 1.5, 1.))),
        // A tray: a box with a slot line.
        NavItem::Inbox => boxed.child(frame(14., 10., 2.).items_end().justify_center().pb(px(2.)).child(fill(6., 1.5, 1.))),
        // A checkbox.
        NavItem::Tasks => boxed.child(frame(11., 11., 2.).items_center().justify_center().child(fill(5., 5., 1.))),
        // A person: head over shoulders.
        NavItem::Employees => boxed.child(v().items_center().gap(px(1.)).child(fill(6., 6., 3.)).child(div().w(px(12.)).h(px(5.)).rounded_tl(px(5.)).rounded_tr(px(5.)).bg(c(color)))),
        NavItem::Projects => boxed.child(ui::folder_glyph(color)),
        // A ring with a hub.
        NavItem::Settings => boxed.child(h().size(px(12.)).rounded(px(6.)).border_2().border_color(c(color)).justify_center().items_center().child(fill(3., 3., 2.))),
    }
}

/// The sidebar's rows, top to bottom: every entry but Settings (the server picker opens it).
pub fn sidebar_items() -> impl Iterator<Item = NavItem> {
    NavItem::ALL.into_iter().filter(|n| *n != NavItem::Settings)
}

impl AppView {
    /// What the inbox badge counts: what needs you (the inbox's first tab) and what failed and is
    /// not read yet. Results of finished tasks give no badge.
    pub fn inbox_count(&self) -> usize {
        let failed = self.inbox_notifications().iter().filter(|n| !n.read && crate::app::note_kind(n) == crate::app::NoteKind::Failed).count();
        self.inbox_counts()[0] + failed
    }

    /// Collapsed (Ctrl+B) the sidebar is gone; hovering the title strip's sidebar button
    /// then shows it whole over the page (`peek`) for as long as the mouse stays in it.
    pub fn render_sidebar(&self, peek: bool, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let current = NavItem::of(self.screen);
        let open_project = match self.screen {
            Page::Project(ProjectRef::Real(id), _) => Some(id),
            _ => None,
        };
        let inbox = self.inbox_count();
        let item = |nav: NavItem, cx: &mut Context<Self>| {
            let active = current == nav;
            let icon_color = if active { theme::ACCENT_TEXT } else { theme::MUTED };
            let group = SharedString::from(format!("nav-{}", nav as usize));
            h().id(("nav", nav as usize))
                .group(group.clone())
                .role(Role::Tab)
                .aria_label(nav.label())
                .h(px(30.))
                .px(px(10.))
                .gap(px(9.))
                .rounded(px(6.))
                .cursor_pointer()
                .text_size(px(13.))
                .text_color(c(if active { theme::TEXT } else { theme::TEXT_2 }))
                .when(active, |el| el.bg(c(theme::SURFACE_2)).font_weight(FontWeight::MEDIUM))
                .hover(|s| s.bg(c(theme::SURFACE_2)).text_color(c(theme::TEXT)))
                .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.go_nav(nav, w, cx)))
                .child(div().w(px(3.)).h(px(14.)).flex_none().rounded(px(2.)).when(active, |el| el.bg(c(theme::ACCENT))))
                .child(nav_icon(nav, icon_color))
                .child(div().flex_1().min_w_0().truncate().child(nav.label()))
                .when(nav == NavItem::Inbox && inbox > 0, |el| {
                    el.child(div().px(px(6.)).rounded(px(8.)).bg(c(theme::ACCENT)).text_color(c(theme::BG)).text_size(px(11.)).font_weight(FontWeight::BOLD).child(inbox.to_string()))
                })
                .child(ui::kbd(nav.keys(), cx).opacity(0.).group_hover(group, |s| s.opacity(0.6)))
                .when(matches!(nav, NavItem::Employees | NavItem::Projects), |el| {
                    let folded = self.folded.contains(&nav);
                    el.child(
                        h().id(("nav-fold", nav as usize)).size(px(18.)).flex_none().justify_center().rounded(px(4.))
                            .hover(|s| s.bg(c(theme::BORDER)))
                            .tooltip(ui::tip(if folded { "Show pinned" } else { "Hide pinned" }, None))
                            .aria_label(if folded { "Show pinned" } else { "Hide pinned" })
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                cx.stop_propagation();
                                this.toggle_folded(nav, cx);
                            }))
                            .child(ui::chevron(!folded, 10., theme::MUTED)),
                    )
                })
        };
        let open_employee = match self.screen {
            Page::Employee(ManagerRef::Real(id)) => Some(id),
            _ => None,
        };
        // One pinned row: a lead glyph, the name and an × that unpins; the click opens it, a right-click offers Unpin.
        let pin_row = |key: (&'static str, usize), active: bool, lead: AnyElement, name: String, tip: String, open: PinOpen, unpin: MenuDo, cx: &mut Context<Self>| {
            let group = SharedString::from(format!("{}-{}", key.0, key.1));
            h().id(key)
                .group(group.clone())
                .h(px(26.))
                .pl(px(26.))
                .pr(px(6.))
                .gap(px(8.))
                .rounded(px(6.))
                .cursor_pointer()
                .text_size(px(12.5))
                .text_color(c(if active { theme::TEXT } else { theme::TEXT_2 }))
                .when(active, |el| el.bg(c(theme::SURFACE_2)))
                .hover(|s| s.bg(c(theme::SURFACE_2)).text_color(c(theme::TEXT)))
                .tooltip(ui::tip(tip, None))
                .on_click(cx.listener(move |this, e: &ClickEvent, w, cx| match open {
                    PinOpen::Project(id) if e.modifiers().secondary() => this.open_project_window(id, cx),
                    PinOpen::Project(id) => this.open_project(ProjectRef::Real(id), w, cx),
                    PinOpen::Employee(id) => this.open_manager(ManagerRef::Real(id), w, cx),
                }))
                .on_mouse_down(MouseButton::Right, cx.listener({
                    let unpin = unpin.clone();
                    move |this, e: &MouseDownEvent, w, cx| {
                        let items: MenuItems = vec![Some(MenuEntry { label: "Unpin".into(), keys: None, run: unpin.clone(), danger: false, checked: false, detail: None })];
                        this.open_menu(e.position, items, w, cx);
                    }
                }))
                .child(lead)
                .child(div().flex_1().min_w_0().truncate().child(name))
                .child(
                    div().id((key.0, key.1 + 1000)).size(px(16.)).flex_none().flex().items_center().justify_center().rounded(px(4.)).text_size(px(12.))
                        .opacity(0.)
                        .group_hover(group, |s| s.opacity(0.7))
                        .hover(|s| s.bg(c(theme::BORDER)).opacity(1.))
                        .tooltip(ui::tip("Unpin", None))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            cx.stop_propagation();
                            match &unpin {
                                MenuDo::Unpin(id) => this.toggle_pin(*id, cx),
                                MenuDo::TogglePinEmployee(id) => this.toggle_employee_pin(*id, cx),
                                _ => {}
                            }
                        }))
                        .child("×"),
                )
        };
        let project_pins: Vec<(aurelius_domain::ProjectId, String)> = self.pinned_projects().into_iter().map(|p| (p.id, p.name.clone())).collect();
        let mut project_rows: Vec<_> = project_pins.into_iter().enumerate().map(|(i, (id, name))| {
            let (state, color) = self.project_state(id);
            let dot = if state == "Quiet" { ui::ring(6., color) } else { ui::dot(6., color) };
            pin_row(("pin", i), open_project == Some(id), dot.into_any_element(), name.clone(), format!("{name} · {state}"), PinOpen::Project(id), MenuDo::Unpin(id), cx)
        }).collect();
        let employee_pins: Vec<(aurelius_domain::EmployeeId, String, String)> = self.pinned_employees().into_iter().map(|m| (m.id, m.name.clone(), m.role.clone())).collect();
        let mut employee_rows: Vec<_> = employee_pins.into_iter().enumerate().map(|(i, (id, name, role))| {
            let lead = ui::monogram(&name, 16., cx).into_any_element();
            pin_row(("pin-employee", i), open_employee == Some(id), lead, name.clone(), format!("{name} · {role}"), PinOpen::Employee(id), MenuDo::TogglePinEmployee(id), cx)
        }).collect();
        // Every item but Settings, Team chat right under Dashboard; the pinned rows sit under their item, unless it is folded.
        let mut nav = v().gap(px(1.));
        for n in sidebar_items() {
            nav = nav.child(item(n, cx));
            let rows = match n {
                NavItem::Employees => std::mem::take(&mut employee_rows),
                NavItem::Projects => std::mem::take(&mut project_rows),
                _ => continue,
            };
            if !self.folded.contains(&n) {
                nav = nav.children(rows.into_iter().map(IntoElement::into_any_element));
            }
        }
        let mac_buttons = cfg!(target_os = "macos") && !window.is_fullscreen();
        v().id("sidebar")
            .w(px(SIDEBAR_WIDTH))
            .flex_none()
            .h_full()
            .bg(c(theme::SIDEBAR))
            .border_r_1()
            .border_color(c(theme::BORDER))
            // Peeking, it floats over the page and goes away once the mouse leaves it.
            .when(peek, |el| {
                el.occlude().shadow_lg().on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                    if !*hovered && this.sidebar_peek {
                        this.sidebar_peek = false;
                        cx.notify();
                    }
                }))
            })
            .child(
                // The OS title bar is hidden: this strip drags the window too.
                // Peeking, the sidebar button sits where the title strip has it, so the
                // mouse stays on it; a click keeps the sidebar open.
                h().id("sidebar-drag").h(px(46.)).flex_none()
                    .pl(px(if mac_buttons { 84. } else if peek { 12. } else { 16. }))
                    .gap(px(10.))
                    .window_control_area(WindowControlArea::Drag)
                    .when(peek, |el| {
                        el.child(
                            ui::icon_button("sidebar-pin")
                                .aria_label("Keep the sidebar open")
                                .tooltip(ui::tip("Keep sidebar open", Some("secondary-b")))
                                .occlude()
                                .on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.on_toggle_sidebar(&ToggleSidebar, w, cx)))
                                .child(ui::sidebar_glyph(false)),
                        )
                    })
                    .child(ui::logo(17., theme::ACCENT))
                    .child(div().font_family(ui::serif(cx)).text_size(px(16.)).child("Aurelius"))
                    .when(aurelius_host::Environment::current() == aurelius_host::Environment::Dev, |el| {
                        el.child(
                            div().px(px(6.)).py(px(1.)).rounded(px(4.)).bg(rgb(0xb7791f)).text_color(rgb(0x1a1206))
                                .text_size(px(10.5)).font_weight(FontWeight::BOLD).child("DEV"),
                        )
                    }),
            )
            .child(v().id("sidebar-nav").flex_1().min_h_0().overflow_y_scroll().px(px(8.)).pt(px(6.)).child(nav))
            .child(self.server_picker(cx))
    }

    /// The bottom of the sidebar: which server, how the connection stands; a click
    /// chooses another server (or updates, when the server needs a newer app).
    fn server_picker(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let host = self.remote_name().map_or("This computer".to_string(), str::to_owned);
        let (dot, state): (u32, String) = match &self.connection {
            Connection::Connecting => (theme::MUTED, "Connecting…".into()),
            Connection::Online { .. } => (theme::ACCENT, String::new()),
            Connection::Offline(_) => (theme::NEGATIVE, "Offline · reconnecting".into()),
            Connection::Unpaired(_) => (theme::NEGATIVE, "Not paired".into()),
            Connection::NoServer => (theme::MUTED, "No server chosen".into()),
            Connection::UpdateNeeded(_) => (theme::NEGATIVE, "Update needed".into()),
        };
        let update_needed = matches!(self.connection, Connection::UpdateNeeded(_));
        h().id("server-picker")
            .role(Role::Button)
            .aria_label("Choose a server")
            .mx(px(8.))
            .mb(px(10.))
            .px(px(10.))
            .py(px(7.))
            .gap(px(9.))
            .rounded(px(7.))
            .border_1()
            .border_color(c(if update_needed { theme::NEGATIVE } else { theme::BORDER }))
            .cursor_pointer()
            .hover(|s| s.bg(c(theme::SURFACE_2)))
            .tooltip(ui::tip(if update_needed { "Updates" } else { "Servers" }, Some(if update_needed { "secondary-shift-u" } else { "secondary-alt-s" })))
            .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| {
                if update_needed {
                    w.dispatch_action(Box::new(OpenUpdates), cx);
                } else {
                    this.on_open_servers(&OpenServers, w, cx);
                }
            }))
            .child(ui::dot(7., dot))
            .child(
                v().flex_1().min_w_0()
                    .child(div().truncate().text_size(px(12.5)).text_color(c(theme::TEXT)).child(host))
                    .when(!state.is_empty(), |d| d.child(div().truncate().text_size(px(11.)).text_color(c(if update_needed { theme::NEGATIVE } else { theme::MUTED })).child(state))),
            )
            .child(ui::chevron(true, 11., theme::MUTED))
    }
}

#[cfg(test)]
mod tests {
    use super::sidebar_items;
    use crate::app::{NavItem, Page};

    #[test]
    fn team_chat_sits_right_under_dashboard() {
        let rows: Vec<NavItem> = sidebar_items().collect();
        assert_eq!(rows, [NavItem::Dashboard, NavItem::Chat, NavItem::Inbox, NavItem::Tasks, NavItem::Employees, NavItem::Projects]);
        assert_eq!(NavItem::Chat.label(), "Team chat");
        assert_eq!(NavItem::of(Page::GroupChat), NavItem::Chat, "the group chat page lights up its row");
    }
}
